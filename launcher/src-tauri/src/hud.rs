//! HUD 轮询器：按 500ms 轮询 DOSBox REST，把游戏数值解码为快照推给宿主。
//!
//! 设计理由：HUD 只读游戏状态，数据通道与 PL001 完全一致；轮询在独立线程
//! 运行，通过回调把快照/停滞事件交给宿主（命令层转发 tauri event，测试层
//! 记录做双通道对照）——不直接依赖 tauri，保持可测性。
//! 容错（AGENTS.md 白名单已登记）：瞬时失败跳过本轮，连续 5 次失败发停滞
//! 事件并停止轮询（显式暴露、不吞错）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use capsule_core::monitor::profile::{Entry, MemoryMap};
use capsule_core::monitor::rest::{MonitorError, RestClient};

/// HUD 连续失败多少次后停止轮询（容错白名单登记值）。
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// HUD 关注的档案条目名。
const WATCHED: [&str; 26] = [
    "hp1", "hp2", "hp3", "hp4", "hp5", "hp6", "occ1", "occ2", "occ3", "occ4", "occ5", "occ6",
    "face1", "face2", "face3", "face4", "face5", "face6", "gold", "gems", "food", "dir", "pos_x",
    "pos_y", "map_id", "side",
];

/// 队伍姓名块（seg, off）：与 occ 同址起读，长度 [`NAME_LEN`]；内容为 Big5
/// 原始字节（头部可能带 `%E` 控制码，由前端解码时剥离——PL009.2）。
const NAME_BLOCKS: [(u16, u32); 6] = [
    (0x56E5, 0x0000),
    (0x56FB, 0x0004),
    (0x5711, 0x0006),
    (0x5727, 0x0008),
    (0x573D, 0x000A),
    (0x5753, 0x000C),
];

/// 姓名块读取长度（名字字段 16 字节，含 NUL 填充）。
const NAME_LEN: u32 = 16;

/// 一帧 HUD 快照。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HudSnapshot {
    /// 全队 HP（hp1–hp6，i16 有符号）
    pub hp: [i64; 6],
    /// 槽位占用（occ1–occ6：名字首字节非 0 = 有角色；解雇/空槽为 0——PL009.2）
    pub occupied: [bool; 6],
    /// 全队面容（face1–face6，u8 名册槽位；面容文件号 = 值 + 1——PL009.2）
    pub face: [u8; 6],
    /// 随身金币
    pub gold: i64,
    /// 随身宝石
    pub gems: i64,
    /// 食物份数
    pub food: i64,
    /// 食物可维持天数（food / 18 取整，定案派生公式）
    pub food_days: i64,
    /// 全队姓名（Big5 原始字节，含 `%E` 控制码头与 NUL 填充，前端解码——PL009.2）
    pub name: [[u8; 16]; 6],
    /// 队伍朝向（0–3；方位语义经用户 live 判收确认——PL011）
    pub direction: i64,
    /// 队伍所在格 X（迷宫图 0–15；城镇连续空间可超 16，引擎真值原样显示——PL011）
    pub pos_x: i64,
    /// 队伍所在格 Y（同上——PL011）
    pub pos_y: i64,
    /// 所在地图编号（CUR 容器主图号；城镇连续空间保持进城锚点号——PL011）
    pub map_id: i64,
    /// 当前侧标志（0x5A7CE；1=云端星云面 / 0=暗面，经用户 live 判收确认——PL011）。
    /// 后端查区域名表的内部用途，不随 hud 事件序列化（前端消费派生字段——A003 #12）。
    #[serde(skip_serializing)]
    pub side: i64,
    /// 所在地图名（区域名表查得；无名图/侧未知为 None）
    pub map_name: Option<String>,
    /// 所在地图是否户外（区域名表查得；表未收录为 None）
    pub map_outdoors: Option<bool>,
}

/// HUD 轮询事件。
#[derive(Debug, Clone)]
pub enum HudPollEvent {
    /// 正常快照（Box 化：快照体积大，避免与 Stalled 变体尺寸差过大）。
    Snapshot(Box<HudSnapshot>),
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

/// HUD 轮询主循环：加载档案 → 过滤 WATCHED 条目 → 按间隔轮询快照；单次失败
/// 跳过本轮累计计数，连续超 [`MAX_CONSECUTIVE_FAILURES`] 发 Stalled 并退出
/// （AGENTS.md 容错白名单口径：显式暴露、不吞错）。
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
    // 只保留关注条目：26 个条目 / 500ms，避免 109 条目全量轮询的开销
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
                on_event(HudPollEvent::Snapshot(Box::new(snapshot)));
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
    let mut occupied = [false; 6];
    let mut face = [0u8; 6];
    let mut gold = 0i64;
    let mut gems = 0i64;
    let mut food = 0i64;
    let mut direction = 0i64;
    let mut pos_x = 0i64;
    let mut pos_y = 0i64;
    let mut map_id = 0i64;
    let mut side = 0i64;
    for entry in entries {
        let bytes = client.memory(Some(entry.segment), entry.offset, u32::from(entry.len))?;
        let value = entry.value_type.decode(&bytes)?;
        match entry.name.as_str() {
            "gold" => gold = value,
            "gems" => gems = value,
            "food" => food = value,
            "dir" => direction = value,
            "pos_x" => pos_x = value,
            "pos_y" => pos_y = value,
            "map_id" => map_id = value,
            "side" => side = value,
            name => {
                if let Some(index) = name
                    .strip_prefix("hp")
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    if (1..=6).contains(&index) {
                        hp[index - 1] = value;
                    }
                } else if let Some(index) = name
                    .strip_prefix("occ")
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    if (1..=6).contains(&index) {
                        occupied[index - 1] = value != 0;
                    }
                } else if let Some(index) = name
                    .strip_prefix("face")
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    if (1..=6).contains(&index) {
                        face[index - 1] = u8::try_from(value).unwrap_or(0);
                    }
                }
            }
        }
    }
    let (map_name, map_outdoors) = map_lookup(side, map_id);
    Ok(HudSnapshot {
        hp,
        occupied,
        face,
        name: read_names(client)?,
        gold,
        gems,
        food,
        food_days: food / 18,
        direction,
        pos_x,
        pos_y,
        map_id,
        side,
        map_name,
        map_outdoors,
    })
}

/// 地图名条目（来源 = PL010 盘点：CUR 主图 + DARK.CC 区域名，内嵌 mapnames.csv）。
/// `pub(crate)` 及字段可见性供 commands.rs 测试模块断言（生产消费仅 hud 内部）。
pub(crate) struct MapNameEntry {
    pub(crate) name: Option<String>,
    pub(crate) outdoors: bool,
}

/// 按侧分表（cloud/dark 各按地图号索引）；两侧共用编号空间，25–28 号户外标志相反。
/// `pub(crate)` 及字段可见性供 commands.rs 测试模块断言（生产消费仅 hud 内部）。
pub(crate) struct MapNameTable {
    pub(crate) cloud: std::collections::HashMap<u8, MapNameEntry>,
    pub(crate) dark: std::collections::HashMap<u8, MapNameEntry>,
}

/// 解析内嵌 CSV（行格式 `side,map_id,outdoors,name`；`#` 注释行跳过）。
/// 坏行（字段缺失/id 非 u8/侧名非法）与重复 map_id 打警告后跳过——内嵌
/// 资产解析失败属程序缺陷，必须显式留痕（对齐 FIX003.5 同语义标准，A004 #7；
/// 用 launcher 的 logging 而非 cc_archive 的 eprintln：库 crate 无 logging）。
/// `pub(crate)` 供 commands.rs 测试模块直测（生产消费仅 hud 内部）。
pub(crate) fn parse_map_names(csv: &'static str) -> MapNameTable {
    let mut table = MapNameTable {
        cloud: std::collections::HashMap::new(),
        dark: std::collections::HashMap::new(),
    };
    for (n, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(4, ',');
        let (side, id, outdoors, name) =
            match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some(s), Some(i), Some(o), Some(n)) => (s, i, o, n),
                _ => {
                    crate::logging::warn(&format!(
                        "mapnames.csv 第 {} 行字段不足，已跳过: {line}",
                        n + 1
                    ));
                    continue;
                }
            };
        let (id, outdoors) = match (id.parse::<u8>(), outdoors) {
            (Ok(id), "0" | "1") => (id, outdoors == "1"),
            _ => {
                crate::logging::warn(&format!(
                    "mapnames.csv 第 {} 行 id/outdoors 非法，已跳过: {line}",
                    n + 1
                ));
                continue;
            }
        };
        let name = if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        };
        let target = match side {
            "cloud" => &mut table.cloud,
            "dark" => &mut table.dark,
            _ => {
                crate::logging::warn(&format!(
                    "mapnames.csv 第 {} 行侧名非法，已跳过: {line}",
                    n + 1
                ));
                continue;
            }
        };
        if target.contains_key(&id) {
            crate::logging::warn(&format!(
                "mapnames.csv 第 {} 行重复 map_id {id}，后者覆盖前者: {line}",
                n + 1
            ));
        }
        target.insert(id, MapNameEntry { name, outdoors });
    }
    table
}

/// 内嵌名表单例（编译期资产，解析失败属程序缺陷——与 cc_archive_names.csv 同类）。
fn map_names() -> &'static MapNameTable {
    static TABLE: OnceLock<MapNameTable> = OnceLock::new();
    TABLE.get_or_init(|| parse_map_names(include_str!("mapnames.csv")))
}

/// 按侧 + 地图号查名字与户外标志；表未收录（侧值异常/号越界）回落 None。
fn map_lookup(side: i64, map_id: i64) -> (Option<String>, Option<bool>) {
    let table = map_names();
    let by_side = if side == 1 { &table.cloud } else { &table.dark };
    match u8::try_from(map_id).ok().and_then(|id| by_side.get(&id)) {
        Some(entry) => (entry.name.clone(), Some(entry.outdoors)),
        None => (None, None),
    }
}

/// 读取全队姓名块（Big5 原始字节，交由前端解码）。
fn read_names(client: &RestClient) -> Result<[[u8; 16]; 6], MonitorError> {
    let mut name = [[0u8; 16]; 6];
    for (k, (seg, off)) in NAME_BLOCKS.iter().enumerate() {
        let bytes = client.memory(Some(*seg), *off, NAME_LEN)?;
        let n = bytes.len().min(NAME_LEN as usize);
        name[k][..n].copy_from_slice(&bytes[..n]);
    }
    Ok(name)
}
