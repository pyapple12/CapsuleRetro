//! Tauri 命令层：launch_game / stop_game 与游戏会话状态。
//!
//! 会话 = DOSBox 进程（CreateProcessW 挂起创建）+ Job Object + 端口；生命周期
//! 保障依赖 `KILL_ON_JOB_CLOSE`——launcher 进程退出（含崩溃）时进程树必死。
//! 安全边界：spawn 参数全部来自仓库内 `retro/game.json` 与 u16 端口探测值，
//! 无任何外部用户输入参与；进程创建经 `winproc` 的 CreateProcessW 参数列表完成。

use std::io::Read as _;
use std::net::TcpListener;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tauri::Emitter;
use windows::Win32::Foundation::HANDLE;

use crate::gameconf;
use crate::hud::{HudPollEvent, HudPoller};
use crate::jobctl::JobHandle;
use crate::winembed;
use crate::winproc::{self, WinProcess};

/// 全局会话槽（同一时刻至多一个游戏会话）。
#[derive(Default)]
pub struct SessionState {
    inner: Mutex<Option<Session>>,
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
            500,
            on_event,
        ));
    }

    /// 取帧流快照：`GET /api/v2/frame` 的二进制透传，前置 20 字节会话内元数据头
    /// （帧号 u64 LE + 宽/高/行距 u32 LE ×3），前端据此解码与去重。
    ///
    /// # Errors
    /// 无会话、上游 503（尚无帧）或 HTTP 失败时返回错误消息。
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
            .take(16 * 1024 * 1024)
            .read_to_end(&mut pixels)
            .map_err(|e| format!("帧体读取失败: {e}"))?;

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

/// 启动游戏会话：加载 game.json → 探测端口 → 生成 conf → 挂起创建 DOSBox →
/// 挂 Job → 恢复执行。
pub fn start_session() -> Result<Session> {
    let repo_root = gameconf::resolve_repo_root(&std::env::current_dir()?);
    let config = capsule_core::game_config::GameConfig::load(&repo_root.join("retro/game.json"))
        .context("加载 retro/game.json 失败")?;
    let port = probe_port(config.dosbox.webserver_port)?;
    let conf_text = gameconf::generate_conf(&config, &repo_root);
    let runtime = gameconf::runtime_dir();
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
    let job = JobHandle::create().context("创建 Job Object 失败")?;
    job.assign(process.handle())
        .context("挂载 Job Object 失败")?;
    process.resume().context("恢复进程执行失败")?;
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

/// 启动游戏（UI 命令）。
#[tauri::command]
pub fn launch_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<String, String> {
    let mut slot = state.inner.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
        return Err("已有游戏会话在运行".into());
    }
    start_session()
        .map(|mut session| {
            let port = session.port();
            let emitter = app.clone();
            session.start_hud(Box::new(move |event| match event {
                HudPollEvent::Snapshot(snapshot) => {
                    if let Err(err) = emitter.emit("hud", &snapshot) {
                        eprintln!("hud 事件发送失败: {err}");
                    }
                }
                HudPollEvent::Stalled(message) => {
                    eprintln!("HUD 停滞: {message}");
                    if let Err(err) = emitter.emit("hud-error", &message) {
                        eprintln!("hud 错误事件发送失败: {err}");
                    }
                }
            }));
            // 窗口离屏托管：SDL 窗口创建约需 1-2 秒，延后嵌入；失败不阻断会话
            // （窗口仍在屏幕上，帧流与注入不受影响——退化形态可用）
            let embed_pid = session.pid;
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                if let Err(err) = winembed::embed_offscreen(embed_pid) {
                    eprintln!("窗口离屏托管失败（退化为本机窗口形态）: {err}");
                }
            });
            *slot = Some(session);
            format!("游戏已启动（webserver 端口 {port}）")
        })
        .map_err(|e| e.to_string())
}

/// 取帧流快照（UI 命令，二进制：20 字节元数据头 + BGRX 像素）。
#[tauri::command]
pub fn get_frame(state: tauri::State<'_, SessionState>) -> Result<tauri::ipc::Response, String> {
    let slot = state.inner.lock().map_err(|e| e.to_string())?;
    let session = slot.as_ref().ok_or("没有运行中的游戏会话")?;
    let bytes = session.get_frame()?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 注入输入事件（UI 命令，body 为注入端点的 JSON 字符串透传）。
#[tauri::command]
pub fn inject_input(
    state: tauri::State<'_, SessionState>,
    payload: String,
) -> Result<String, String> {
    let slot = state.inner.lock().map_err(|e| e.to_string())?;
    let session = slot.as_ref().ok_or("没有运行中的游戏会话")?;
    session.inject_input(&payload)
}

/// 停止游戏（UI 命令）。
#[tauri::command]
pub fn stop_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<String, String> {
    let mut slot = state.inner.lock().map_err(|e| e.to_string())?;
    match slot.as_mut() {
        Some(session) => {
            session.stop().map_err(|e| e.to_string())?;
            *slot = None;
            if let Err(err) = app.emit("hud-clear", ()) {
                eprintln!("hud-clear 事件发送失败: {err}");
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
