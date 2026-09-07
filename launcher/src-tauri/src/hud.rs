//! HUD 轮询器：按 500ms 轮询 DOSBox REST，把游戏数值解码为快照推给宿主。
//!
//! 设计理由：HUD 只读游戏状态，数据通道与 PL001 完全一致；轮询在独立线程
//! 运行，通过回调把快照/停滞事件交给宿主（命令层转发 tauri event，测试层
//! 记录做双通道对照）——不直接依赖 tauri，保持可测性。
//! 容错（AGENTS.md 白名单已登记）：瞬时失败跳过本轮，连续 5 次失败发停滞
//! 事件并停止轮询（显式暴露、不吞错）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use capsule_core::monitor::profile::{Entry, MemoryMap};
use capsule_core::monitor::rest::{MonitorError, RestClient};

/// HUD 连续失败多少次后停止轮询（容错白名单登记值）。
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// HUD 关注的档案条目名。
const WATCHED: [&str; 9] = [
    "hp1", "hp2", "hp3", "hp4", "hp5", "hp6", "gold", "gems", "food",
];

/// 一帧 HUD 快照。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HudSnapshot {
    /// 全队 HP（hp1–hp6，i16 有符号）
    pub hp: [i64; 6],
    /// 随身金币
    pub gold: i64,
    /// 随身宝石
    pub gems: i64,
    /// 食物份数
    pub food: i64,
    /// 食物可维持天数（food / 18 取整，定案派生公式）
    pub food_days: i64,
}

/// HUD 轮询事件。
#[derive(Debug, Clone)]
pub enum HudPollEvent {
    /// 正常快照。
    Snapshot(HudSnapshot),
    /// 连续失败达到上限，轮询已停止。
    Stalled(String),
}

/// HUD 轮询器（Drop 或显式 [`HudPoller::stop`] 停止后台线程）。
pub struct HudPoller {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl HudPoller {
    /// 启动轮询线程。
    ///
    /// # 参数
    /// - `port`：webserver 端口
    /// - `profile`：memory_map 档案路径
    /// - `interval_ms`：轮询间隔（毫秒）
    /// - `on_event`：快照/停滞事件回调
    pub fn start(
        port: u16,
        profile: PathBuf,
        interval_ms: u64,
        on_event: Box<dyn Fn(HudPollEvent) + Send>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            poll_loop(port, profile, interval_ms, &flag, on_event);
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// 停止轮询并等待线程结束。
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for HudPoller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn poll_loop(
    port: u16,
    profile: PathBuf,
    interval_ms: u64,
    stop: &AtomicBool,
    on_event: Box<dyn Fn(HudPollEvent) + Send>,
) {
    let client = RestClient::new(port);
    let map = match MemoryMap::load(&profile) {
        Ok(map) => map,
        Err(err) => {
            on_event(HudPollEvent::Stalled(format!("档案加载失败: {err}")));
            return;
        }
    };
    // 只保留关注条目：9 个条目 / 500ms，避免 92 条目全量轮询的开销
    let entries: Vec<&Entry> = map
        .entries
        .iter()
        .filter(|e| WATCHED.contains(&e.name.as_str()))
        .collect();

    let mut failures = 0u32;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match poll_once(&client, &entries) {
            Ok(snapshot) => {
                failures = 0;
                on_event(HudPollEvent::Snapshot(snapshot));
            }
            Err(err) => {
                failures += 1;
                if failures >= MAX_CONSECUTIVE_FAILURES {
                    on_event(HudPollEvent::Stalled(format!(
                        "HUD 轮询连续 {failures} 次失败: {err}"
                    )));
                    return;
                }
            }
        }
        interruptible_sleep(Duration::from_millis(interval_ms), stop);
    }
}

/// 轮间等待：整段 sleep 改为 50ms 步进，stop() 最坏等待从 interval（500ms）
/// 降到 50ms，停止响应性不牺牲轮询节奏。
fn interruptible_sleep(duration: Duration, stop: &AtomicBool) {
    let step = Duration::from_millis(50);
    let mut remaining = duration;
    while !stop.load(Ordering::Relaxed) {
        if remaining <= step {
            if !remaining.is_zero() {
                std::thread::sleep(remaining);
            }
            return;
        }
        std::thread::sleep(step);
        remaining -= step;
    }
}

/// 单轮轮询：读关注条目并解码为快照。
fn poll_once(client: &RestClient, entries: &[&Entry]) -> Result<HudSnapshot, MonitorError> {
    let mut hp = [0i64; 6];
    let mut gold = 0i64;
    let mut gems = 0i64;
    let mut food = 0i64;
    for entry in entries {
        let bytes = client.memory(Some(entry.segment), entry.offset, u32::from(entry.len))?;
        let value = entry.value_type.decode(&bytes)?;
        match entry.name.as_str() {
            "gold" => gold = value,
            "gems" => gems = value,
            "food" => food = value,
            name => {
                if let Some(index) = name
                    .strip_prefix("hp")
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    if (1..=6).contains(&index) {
                        hp[index - 1] = value;
                    }
                }
            }
        }
    }
    Ok(HudSnapshot {
        hp,
        gold,
        gems,
        food,
        food_days: food / 18,
    })
}
