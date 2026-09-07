//! Tauri 命令层：launch_game / stop_game 与游戏会话状态。
//!
//! 会话 = DOSBox 进程（CreateProcessW 挂起创建）+ Job Object + 端口；生命周期
//! 保障依赖 `KILL_ON_JOB_CLOSE`——launcher 进程退出（含崩溃）时进程树必死。
//! 进程自退/崩溃由退出监听线程捕获：清槽 + 广播 `session-ended` 事件，UI
//! 不会停留在"会话仍在"的假状态。
//! "无会话"约定：get_frame / inject_input 返回 `Err(NO_SESSION: ...)`（前端
//! 据前缀识别并停轮询）；stop_game 对无会话是幂等成功（Ok）。
//! 安全边界：spawn 参数全部来自仓库内 `retro/game.json` 与 u16 端口探测值，
//! 无任何外部用户输入参与；进程创建经 `winproc` 的 CreateProcessW 参数列表完成。

use std::io::Read as _;
use std::net::TcpListener;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use tauri::Emitter;
use windows::Win32::Foundation::HANDLE;

use crate::gameconf;
use crate::hud::{HudPollEvent, HudPoller};
use crate::jobctl::JobHandle;
use crate::logging;
use crate::winembed;
use crate::winproc::{self, SendHandle, WinProcess};

/// HUD 轮询间隔毫秒（VD3 实测口径：launcher 进程 CPU 增量 < 2%）。
const HUD_POLL_INTERVAL_MS: u64 = 500;

/// webserver 就绪探测超时（秒）：VB1 实测 REST 10 秒内可达，留余量。
const WEBSERVER_READY_TIMEOUT_SECS: u64 = 15;

/// "无会话"错误前缀：前端据此程序化识别并停帧轮询（见模块文档约定）。
const NO_SESSION: &str = "NO_SESSION";

/// 全局会话槽（同一时刻至多一个游戏会话）。Arc 包装以便退出监听线程
/// 持有同一槽位引用（State 内部不转移所有权）。
#[derive(Default)]
pub struct SessionState {
    inner: Arc<Mutex<Option<Session>>>,
}

/// 一次游戏会话的全部资源。
pub struct Session {
    process: WinProcess,
    job: JobHandle,
    port: u16,
    profile: PathBuf,
    hud: Option<HudPoller>,
    agent: ureq::Agent,
    pid: u32,
}

impl Session {
    /// 进程是否仍在运行。
    pub fn is_alive(&mut self) -> bool {
        self.process.is_running()
    }

    /// 终止会话（HUD 轮询 + Job 级杀树）并等待子进程退出。
    pub fn stop(&mut self) -> Result<()> {
        if let Some(poller) = self.hud.take() {
            poller.stop();
        }
        self.job.kill().context("终止 Job 失败")?;
        if !self.process.wait_exit(5000) {
            return Err(anyhow!("进程在 5 秒内未退出"));
        }
        Ok(())
    }

    /// 本次会话的 webserver 端口。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 启动 HUD 轮询（事件经回调交给宿主转发 tauri event）。
    pub fn start_hud(&mut self, on_event: Box<dyn Fn(HudPollEvent) + Send>) {
        self.hud = Some(HudPoller::start(
            self.port,
            self.profile.clone(),
            HUD_POLL_INTERVAL_MS,
            on_event,
        ));
    }

    /// 取帧流快照：`GET /api/v2/frame` 的二进制透传，前置 20 字节会话内元数据头
    /// （帧号 u64 LE + 宽/高/行距 u32 LE ×3），前端据此解码与去重。
    ///
    /// # Errors
    /// 无会话、元数据头缺失、帧体长度与元数据不符（截断或膨胀都是协议错位，
    /// 静默传递会在前端表现为花屏——严格报错）、上游 503（尚无帧）或 HTTP
    /// 失败时返回错误消息。
    pub fn get_frame(&self) -> Result<Vec<u8>, String> {
        let resp = self
            .agent
            .get(&format!("http://127.0.0.1:{}/api/v2/frame", self.port))
            .call()
            .map_err(|e| format!("帧流请求失败: {e}"))?;

        let header = |name: &str| -> Result<u64, String> {
            resp.header(name)
                .ok_or_else(|| format!("帧流响应缺少 {name}"))?
                .parse()
                .map_err(|e| format!("{name} 解析失败: {e}"))
        };
        let number = header("X-Capsule-Frame-Number")?;
        let width = header("X-Capsule-Frame-Width")? as u32;
        let height = header("X-Capsule-Frame-Height")? as u32;
        let pitch = header("X-Capsule-Frame-Pitch")? as u32;

        let mut pixels = Vec::new();
        resp.into_reader()
            .read_to_end(&mut pixels)
            .map_err(|e| format!("帧体读取失败: {e}"))?;
        // 帧体完整性：最小 = 最后一行的有效像素；最大 = pitch×height 全缓冲。
        // 越界说明上游协议漂移或传输截断，宁可报错不可静默错值。
        let min_len = (pitch as usize) * (height as usize - 1) + (width as usize) * 4;
        let max_len = (pitch as usize) * (height as usize);
        if pixels.len() < min_len || pixels.len() > max_len {
            return Err(format!(
                "帧体长度异常: {} 字节（元数据 {}x{} pitch {} 允许 {min_len}..={max_len}）",
                pixels.len(),
                width,
                height,
                pitch
            ));
        }

        let mut out = Vec::with_capacity(20 + pixels.len());
        out.extend_from_slice(&number.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&pitch.to_le_bytes());
        out.extend_from_slice(&pixels);
        Ok(out)
    }

    /// 注入输入事件：JSON body 透传 `POST /api/v2/input/event`。
    ///
    /// # Errors
    /// 无会话或上游返回非 2xx 时返回错误消息（400 = 前端参数错误）。
    pub fn inject_input(&self, payload: &str) -> Result<String, String> {
        let resp = self
            .agent
            .post(&format!(
                "http://127.0.0.1:{}/api/v2/input/event",
                self.port
            ))
            .set("Content-Type", "application/json")
            .send_string(payload)
            .map_err(|e| format!("注入请求失败: {e}"))?;
        let mut body = String::new();
        resp.into_reader()
            .read_to_string(&mut body)
            .map_err(|e| format!("注入响应读取失败: {e}"))?;
        Ok(body)
    }
}

/// 端口探测：从 `start` 起 5 个端口内找第一个可绑定的；绑定测试后立即释放。
///
/// # 已知竞态（TOCTOU）
/// 本函数释放端口到 DOSBox 实际 bind 之间存在被其他进程抢占的窗口；被抢时
/// webserver 起不来，由 [`start_session`]. 的就绪探测以超时报错兜底呈现，
/// 不做更复杂的预占持有（持有再转交需要 fd 传递，复杂度不值）。
pub fn probe_port(start: u16) -> Result<u16> {
    let mut port = start;
    for _ in 0..5 {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
        port = port
            .checked_add(1)
            .ok_or_else(|| anyhow!("端口探测到达 65535 上限"))?;
    }
    Err(anyhow!("端口 {start} 起连续 5 个全被占用"))
}

/// 构造 DOSBox 命令行参数列表（值来源：webserver 开关为常量、端口为 u16 探测值、
/// conf 路径为运行时目录拼接——均为程序内部生成，无外部输入）。
fn dosbox_args(port: u16, conf_path: &Path) -> Vec<String> {
    vec![
        "--set".to_string(),
        "webserver_enabled=on".to_string(),
        "--set".to_string(),
        format!("webserver_port={port}"),
        "-conf".to_string(),
        conf_path.to_string_lossy().into_owned(),
    ]
}

/// webserver 就绪探测：就绪前逐次失败属预期（进程尚未监听），限时内轮询
/// 直到成功；超时仍不通则以错误终止（错误策略主线：显式暴露，不静默降级）。
fn wait_webserver_ready(port: u16, timeout: Duration) -> Result<()> {
    let client = capsule_core::monitor::rest::RestClient::new(port);
    let deadline = Instant::now() + timeout;
    loop {
        if client.cpu_state().is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "DOSBox webserver 端口 {port} 在 {} 秒内未就绪（进程可能启动失败或端口被抢）",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// 启动游戏会话：加载 game.json → 校验工作副本 → 探测端口 → 生成 conf →
/// 挂起创建 DOSBox → 挂 Job → 恢复执行 → 等 webserver 就绪。
pub fn start_session() -> Result<Session> {
    let cwd = std::env::current_dir().context("读取当前目录失败")?;
    let repo_root = gameconf::resolve_repo_root(&cwd).context("定位仓库根失败")?;
    let config = capsule_core::game_config::GameConfig::load(&repo_root.join("retro/game.json"))
        .context("加载 retro/game.json 失败")?;

    // 工作副本前置校验：缺失时用户只会看到黑窗，提前显式报错指明根因
    // （释放逻辑属 PL003 save 生命周期，当前工作副本由适配层预置）
    let work_dir = repo_root.join(&config.paths.work_dir);
    if !work_dir.is_dir() {
        return Err(anyhow!(
            "工作副本目录不存在: {}（释放流程未执行，检查 game.json paths.work_dir）",
            work_dir.display()
        ));
    }
    let guest_exe = format!("{}.exe", config.name.to_ascii_lowercase());
    if !work_dir.join(&guest_exe).is_file() {
        return Err(anyhow!(
            "工作副本缺少游戏可执行文件: {}（检查释放内容完整性）",
            work_dir.join(&guest_exe).display()
        ));
    }

    let port = probe_port(config.dosbox.webserver_port)?;
    let conf_text = gameconf::generate_conf(&config, &repo_root);
    let runtime = gameconf::runtime_dir(&config.name)?;
    let conf_path = gameconf::write_conf(&conf_text, &runtime)?;

    let exe = repo_root.join(&config.dosbox.exe_path);
    let log_path = runtime.join("dosbox.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("打开日志失败: {}", log_path.display()))?;
    let log_handle = HANDLE(log.as_raw_handle());

    winproc::make_inheritable(log_handle).context("日志句柄设置可继承失败")?;

    let process =
        WinProcess::spawn_suspended(&exe, &dosbox_args(port, &conf_path), log_handle, log_handle)
            .context("创建 DOSBox 进程失败")?;
    // Job 挂载失败路径必须显式终止挂起进程，否则留下无人认领的僵尸
    let job = match JobHandle::create() {
        Ok(job) => job,
        Err(err) => {
            if let Err(term_err) = process.terminate() {
                return Err(anyhow!(err)
                    .context("创建 Job Object 失败")
                    .context(format!("清理挂起进程也失败: {term_err}")));
            }
            return Err(anyhow!(err).context("创建 Job Object 失败"));
        }
    };
    if let Err(err) = job.assign(process.handle()) {
        if let Err(term_err) = process.terminate() {
            return Err(anyhow!(err)
                .context("挂载 Job Object 失败")
                .context(format!("清理挂起进程也失败: {term_err}")));
        }
        return Err(anyhow!(err).context("挂载 Job Object 失败"));
    }
    process.resume().context("恢复进程执行失败")?;
    wait_webserver_ready(port, Duration::from_secs(WEBSERVER_READY_TIMEOUT_SECS))?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .build();
    Ok(Session {
        pid: process.pid(),
        process,
        job,
        port,
        profile: repo_root.join(&config.assets.memory_map),
        hud: None,
        agent,
    })
}

/// 进程退出监听线程：等句柄触发（进程死）→ 若槽内仍是本会话则清槽、停
/// HUD、广播 `session-ended`。持有的是 DuplicateHandle 出的独立句柄副本，
/// 与 Session 的句柄生命周期解耦。
fn spawn_exit_watcher(
    app: tauri::AppHandle,
    sessions: Arc<Mutex<Option<Session>>>,
    watch: SendHandle,
    pid: u32,
) {
    std::thread::spawn(move || {
        winproc::wait_forever_and_close(watch);
        let is_same = {
            let slot = match sessions.lock() {
                Ok(slot) => slot,
                Err(_) => return,
            };
            slot.as_ref().map(|s| s.pid == pid).unwrap_or(false)
        };
        if !is_same {
            return; // 已被 stop_game 正常收尾或已是新会话，不越权处理
        }
        let mut slot = sessions.lock().expect("会话槽锁已毒化");
        if let Some(mut session) = slot.take() {
            if let Some(poller) = session.hud.take() {
                poller.stop();
            }
            // 进程已死，Job 终止仅为兜底收尾；失败不影响事件广播
            let _ = session.job.kill();
        }
        drop(slot);
        logging::info(&format!("游戏进程退出（pid {pid}），会话槽已清"));
        if let Err(err) = app.emit("session-ended", pid) {
            logging::error(&format!("session-ended 事件发送失败: {err}"));
        }
    });
}

/// 启动游戏（UI 命令）。
///
/// async 化：构造流程（含就绪探测最长 15 秒）在异步运行时线程执行，不占
/// 主线程；锁内只做检查与槽位交换（构造在锁外完成，避免持锁跨越重活）。
#[tauri::command]
pub async fn launch_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<String, String> {
    if state.inner.lock().map_err(|e| e.to_string())?.is_some() {
        return Err("已有游戏会话在运行".into());
    }
    let mut session = start_session().map_err(|e| e.to_string())?;
    let port = session.port();
    let emitter = app.clone();
    session.start_hud(Box::new(move |event| match event {
        HudPollEvent::Snapshot(snapshot) => {
            if let Err(err) = emitter.emit("hud", &snapshot) {
                logging::error(&format!("hud 事件发送失败: {err}"));
            }
        }
        HudPollEvent::Stalled(message) => {
            logging::error(&format!("HUD 停滞: {message}"));
            if let Err(err) = emitter.emit("hud-error", &message) {
                logging::error(&format!("hud 错误事件发送失败: {err}"));
            }
        }
    }));
    // 窗口离屏托管：SDL 窗口创建约需 1-2 秒（慢机器更久），轮询重试而非固定
    // 2s 竞速；多次失败不阻断会话（窗口仍在屏幕上，退化形态可用）
    let embed_pid = session.pid;
    std::thread::spawn(move || {
        for attempt in 0..10 {
            std::thread::sleep(Duration::from_millis(500));
            match winembed::embed_offscreen(embed_pid) {
                Ok(()) => return,
                Err(err) if attempt == 9 => {
                    logging::error(&format!("窗口离屏托管失败（退化为本机窗口形态）: {err}"));
                }
                Err(_) => continue, // 窗口尚未创建，下一轮重试
            }
        }
    });
    // 退出监听：句柄副本 + 槽位 Arc 引用，进程自退/崩溃时自动清槽并通知 UI
    let watch_handle = session
        .process
        .duplicate_process_handle()
        .map_err(|e| format!("复制监听句柄失败: {e}"))?;
    spawn_exit_watcher(
        app.clone(),
        Arc::clone(&state.inner),
        SendHandle(watch_handle),
        session.pid,
    );

    let mut slot = state.inner.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
        // 竞态兜底：等待期间另一路已入槽，退掉本路刚构造的会话
        drop(slot);
        let _ = session.stop();
        return Err("已有游戏会话在运行".into());
    }
    *slot = Some(session);
    Ok(format!("游戏已启动（webserver 端口 {port}）"))
}

/// 取帧流快照（UI 命令，二进制：20 字节元数据头 + BGRX 像素）。async 化：
/// 上游最坏 3s 超时不占主线程。
#[tauri::command]
pub async fn get_frame(
    state: tauri::State<'_, SessionState>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = {
        let slot = state.inner.lock().map_err(|e| e.to_string())?;
        let session = slot
            .as_ref()
            .ok_or_else(|| format!("{NO_SESSION}: 没有运行中的游戏会话"))?;
        session.get_frame()?
    };
    Ok(tauri::ipc::Response::new(bytes))
}

/// 注入输入事件（UI 命令，body 为注入端点的 JSON 字符串透传）。async 化理由同 get_frame。
#[tauri::command]
pub async fn inject_input(
    state: tauri::State<'_, SessionState>,
    payload: String,
) -> Result<String, String> {
    let body = {
        let slot = state.inner.lock().map_err(|e| e.to_string())?;
        let session = slot
            .as_ref()
            .ok_or_else(|| format!("{NO_SESSION}: 没有运行中的游戏会话"))?;
        session.inject_input(&payload)?
    };
    Ok(body)
}

/// 停止游戏（UI 命令）。无会话是幂等成功（Ok），与会话清理完成语义一致。
#[tauri::command]
pub async fn stop_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<String, String> {
    let mut slot = state.inner.lock().map_err(|e| e.to_string())?;
    match slot.as_mut() {
        Some(session) => {
            session.stop().map_err(|e| e.to_string())?;
            *slot = None;
            if let Err(err) = app.emit("hud-clear", ()) {
                logging::error(&format!("hud-clear 事件发送失败: {err}"));
            }
            Ok("游戏已停止".into())
        }
        None => Ok("没有运行中的游戏会话".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PL002.11 验证：探测跳过被占端口。
    #[test]
    fn 端口探测跳过被占端口() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定临时端口");
        let busy = listener.local_addr().unwrap().port();
        let picked = probe_port(busy).expect("应跳过被占端口找到可用端口");
        assert_ne!(picked, busy);
    }

    /// 参数列表构造：端口值正确进参数、顺序固定。
    #[test]
    fn dosbox参数列表构造正确() {
        let conf = std::path::Path::new("E:/rt/game.conf");
        assert_eq!(
            dosbox_args(18086, conf),
            vec![
                "--set".to_string(),
                "webserver_enabled=on".to_string(),
                "--set".to_string(),
                "webserver_port=18086".to_string(),
                "-conf".to_string(),
                "E:/rt/game.conf".to_string(),
            ]
        );
    }
}
