//! Tauri 命令层：launch_game / stop_game / get_game_info 与游戏会话状态。
//!
//! 会话 = DOSBox 进程（CreateProcessW 挂起创建）+ Job Object + 端口；生命周期
//! 保障依赖 `KILL_ON_JOB_CLOSE`——launcher 进程退出（含崩溃）时进程树必死。
//! 进程自退/崩溃由退出监听线程捕获：清槽 + 回写 save + 广播 `session-ended`
//! 事件，UI 不会停留在"会话仍在"的假状态。
//! save 生命周期（PL003）：启动前置序列（释放工作副本 → 补丁 → 解包 save）
//! 在 [`start_session`] 内完成；退出回写在 exit-watcher 与 stop_game 两个
//! 挂点走同一 `writeback_save`（幂等，先到先写）。
//! 双形态资源解析（PL004）：读自身 exe 尾部 trailer 识别 dev / 打包形态
//! （`resolve_resource_ctx`），打包形态把 payload 释放到运行时 resources/
//! 目录作为资源根，两形态下游逻辑完全共用。
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

/// dosbox.log 轮转阈值（字节）：超过则改名 .1 保留一份旧日志（审计观察项 #13 治理）。
const DOSBOX_LOG_ROTATE_BYTES: u64 = 5 * 1024 * 1024;

/// 会话 save 生命周期数据（回写所需的全部上下文）。
struct SessionLifecycle {
    /// 工作副本目录（运行时释放目标，存档收集源）
    work: PathBuf,
    /// save 档案路径（仓库根 `<name>.save`）
    save: PathBuf,
    /// 存档收集清单（game.json paths.save_files）
    patterns: Vec<String>,
    /// 本次会话的元数据（hack 状态随回写持久化）
    meta: capsule_core::save::SaveMeta,
}

/// "无会话"错误前缀：前端据此程序化识别并停帧轮询（见模块文档约定）。
const NO_SESSION: &str = "NO_SESSION";

/// 双形态资源上下文（PL004）：资源解析的锚点集合。
///
/// dev 形态：资源根 = 仓库根、save 根 = 仓库根、dosbox 路径随 game.json
/// `dosbox.exe_path` 解析；打包形态（自身 exe 尾部含 payload）：资源根 =
/// 运行时 `resources/` 释放目录、save 根 = exe 同目录（exe 改名无感，save
/// 名从 game.json `name` 派生）、dosbox 路径固定为 `<资源根>/dosbox/<exe 名>`。
struct ResourceCtx {
    /// 资源根（dev = 仓库根；打包形态 = payload 释放目录）
    root: PathBuf,
    /// save 根（`<name>.save` 的所在目录）
    save_root: PathBuf,
    /// 打包形态的 dosbox 可执行路径（dev 形态为 None，走 game.json exe_path）
    packed_dosbox: Option<PathBuf>,
}

/// 解析双形态资源上下文：读自身 exe 尾部 trailer 识别形态（PL004 定案）。
///
/// 打包形态按需 extract-once 释放 payload 到运行时 `resources/` 目录。
///
/// # Errors
/// payload trailer 非法 / 内嵌 game.json 非法 / 资源释放失败严格报错；dev
/// 形态仓库根定位失败同样报错（现行语义不变）。
fn resolve_resource_ctx() -> Result<ResourceCtx> {
    let exe = std::env::current_exe().context("读取自身路径失败")?;
    let mut reader = match capsule_core::payload::PayloadReader::open(&exe)
        .context("读取自身 payload 失败")?
    {
        Some(reader) => reader,
        None => {
            let cwd = std::env::current_dir().context("读取当前目录失败")?;
            let root = gameconf::resolve_repo_root(&cwd).context("定位仓库根失败")?;
            return Ok(ResourceCtx {
                save_root: root.clone(),
                root,
                packed_dosbox: None,
            });
        }
    };
    let config_text = reader
        .read_entry_to_string(capsule_core::payload::GAME_JSON_ENTRY)
        .context("读取内嵌 game.json 失败")?;
    let config: capsule_core::game_config::GameConfig =
        config_text.parse().context("解析内嵌 game.json 失败")?;
    let resources = gameconf::runtime_dir(&config.name)?.join("resources");
    let extracted = reader
        .extract_once(&resources)
        .map_err(|e| anyhow!("payload 资源释放失败: {e}"))?;
    if extracted {
        logging::info(&format!("payload 资源已释放: {}", resources.display()));
    }
    let dosbox_name = config
        .dosbox
        .exe_path
        .file_name()
        .context("内嵌配置 dosbox.exe_path 缺少文件名")?
        .to_string_lossy()
        .into_owned();
    let save_root = exe.parent().context("自身路径缺少父目录")?.to_path_buf();
    Ok(ResourceCtx {
        // 布局镜像 dosbox 构建树：bin（exe+DLL）与 Resources（shader 等资源）
        // 互为兄弟，运行期资源查找依赖该相对结构
        packed_dosbox: Some(resources.join("dosbox").join("bin").join(dosbox_name)),
        root: resources,
        save_root,
    })
}

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
    /// save 生命周期上下文（回写后被 take 走，保证幂等）
    lifecycle: Option<SessionLifecycle>,
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

/// dosbox.log 轮转：超过阈值时改名 `.log.1`（覆盖上一份轮转，仅保留一份）。
/// 轮转失败记录日志后继续追加（仅影响日志体积，不影响功能；白名单登记见 AGENTS.md）。
fn rotate_dosbox_log(log_path: &Path) {
    let oversized = std::fs::metadata(log_path)
        .map(|meta| meta.len() > DOSBOX_LOG_ROTATE_BYTES)
        .unwrap_or(false);
    if !oversized {
        return;
    }
    let rotated = log_path.with_extension("log.1");
    match std::fs::rename(log_path, &rotated) {
        Ok(()) => logging::info(&format!("dosbox.log 已轮转: {}", rotated.display())),
        Err(err) => logging::error(&format!("dosbox.log 轮转失败（继续追加）: {err}")),
    }
}

/// 解析 hack 选择对应的补丁包路径；选了补丁但 id 不在 game.json 里严格报错。
fn resolve_patch_zip(
    resource_root: &Path,
    config: &capsule_core::game_config::GameConfig,
    hack: Option<&str>,
) -> Result<Option<PathBuf>> {
    let Some(id) = hack else {
        return Ok(None);
    };
    let patch = config
        .patches
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow!("hack id {id:?} 在 game.json patches 中不存在"))?;
    Ok(Some(resource_root.join("retro").join(&patch.patch)))
}

/// 启动游戏会话：加载 game.json → 生命周期前置序列（释放工作副本 → 补丁 →
/// 解包 save）→ 校验工作副本 → 探测端口 → 生成 conf → 挂起创建 DOSBox →
/// 挂 Job → 恢复执行 → 等 webserver 就绪。
///
/// # 参数
/// - `hack`：本次会话的补丁选择（None = 原版；随 save meta 持久化）
pub fn start_session(hack: Option<String>) -> Result<Session> {
    let ctx = resolve_resource_ctx()?;
    let config = capsule_core::game_config::GameConfig::load(&ctx.root.join("retro/game.json"))
        .context("加载 retro/game.json 失败")?;
    logging::info(&format!(
        "资源根: {}（{}）",
        ctx.root.display(),
        if ctx.packed_dosbox.is_some() {
            "打包形态"
        } else {
            "dev 仓库形态"
        }
    ));

    // 生命周期前置序列（PL003）：每次启动全量重建工作副本（自愈 + hack
    // 切换互不污染），补丁只作用于释放出的工作副本，原版目录零写入
    let runtime = gameconf::runtime_dir(&config.name)?;
    let work_dir = runtime.join("work");
    let original_dir = ctx.root.join("retro").join(&config.paths.game_dir);
    let save_path = gameconf::save_path(&ctx.save_root, &config.name);
    let patch_zip = resolve_patch_zip(&ctx.root, &config, hack.as_deref())?;
    let default_meta = capsule_core::save::SaveMeta::new("1", hack.clone());
    let (mut meta, recovered) = capsule_core::paths::prepare_session(
        &original_dir,
        &work_dir,
        patch_zip.as_deref(),
        &save_path,
        default_meta,
    )
    .context("会话前置序列失败（释放/补丁/存档）")?;
    // 本次会话的 UI 选择是权威源：save 里旧 meta.hack 不能推翻本次补丁形态
    //（否则解包出的旧值会在回写时把选择覆盖丢失）
    meta.hack = hack;
    if recovered {
        logging::error(&format!(
            "save 档案损坏，已按白名单降级（备份 .corrupt + 全新空档继续）: {}",
            save_path.display()
        ));
    }
    match &patch_zip {
        Some(path) => logging::info(&format!(
            "补丁已应用（hack: {}）: {}",
            meta.hack.clone().unwrap_or_default(),
            path.display()
        )),
        None => logging::info("原版形态启动（无补丁）"),
    }

    // 完整性断言：释放内容必须含游戏可执行文件（game.json 配错在此显式暴露）
    let guest_exe = format!("{}.exe", config.name.to_ascii_lowercase());
    if !work_dir.join(&guest_exe).is_file() {
        return Err(anyhow!(
            "释放后的工作副本缺少游戏可执行文件: {}（检查 game.json paths.game_dir）",
            work_dir.join(&guest_exe).display()
        ));
    }

    let port = probe_port(config.dosbox.webserver_port)?;
    let conf_text = gameconf::generate_conf(&config, &work_dir);
    let conf_path = gameconf::write_conf(&conf_text, &runtime)?;

    let dosbox_exe = ctx
        .packed_dosbox
        .clone()
        .unwrap_or_else(|| ctx.root.join(&config.dosbox.exe_path));
    let log_path = runtime.join("dosbox.log");
    rotate_dosbox_log(&log_path);
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("打开日志失败: {}", log_path.display()))?;
    let log_handle = HANDLE(log.as_raw_handle());

    winproc::make_inheritable(log_handle).context("日志句柄设置可继承失败")?;

    let process = WinProcess::spawn_suspended(
        &dosbox_exe,
        &dosbox_args(port, &conf_path),
        log_handle,
        log_handle,
    )
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
        profile: ctx.root.join(&config.assets.memory_map),
        hud: None,
        agent,
        lifecycle: Some(SessionLifecycle {
            work: work_dir,
            save: save_path,
            patterns: config.paths.save_files.clone(),
            meta,
        }),
    })
}

/// 会话 save 回写（幂等）：lifecycle 上下文先到先写（take 走），回写失败
/// 严格记录错误——旧 save 未被触碰，保持完整可用（原子替换语义）。
fn writeback_save(session: &mut Session) -> Result<()> {
    let Some(lifecycle) = session.lifecycle.take() else {
        return Ok(());
    };
    let count = capsule_core::paths::writeback(
        &lifecycle.work,
        &lifecycle.save,
        &lifecycle.patterns,
        &lifecycle.meta,
    )
    .with_context(|| format!("save 回写失败: {}", lifecycle.save.display()))?;
    logging::info(&format!(
        "save 回写完成（{count} 个存档文件）: {}",
        lifecycle.save.display()
    ));
    Ok(())
}

/// 进程退出监听线程：等句柄触发（进程死）→ 若槽内仍是本会话则清槽、停
/// HUD、回写 save、广播 `session-ended`。持有的是 DuplicateHandle 出的独立
/// 句柄副本，与 Session 的句柄生命周期解耦。
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
            // 崩溃/自退路径同样回写（被杀时游戏已写盘的存档不丢）
            if let Err(err) = writeback_save(&mut session) {
                logging::error(&format!("会话退出回写失败: {err:#}"));
            }
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
///
/// # 参数
/// - `hack`：待机页选择的补丁 id（null = 原版），随 save meta 持久化
#[tauri::command]
pub async fn launch_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
    hack: Option<String>,
) -> Result<String, String> {
    if state.inner.lock().map_err(|e| e.to_string())?.is_some() {
        return Err("已有游戏会话在运行".into());
    }
    let mut session = start_session(hack).map_err(|e| e.to_string())?;
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
/// 正常停止路径回写 save（与 exit-watcher 崩溃路径共用幂等的 writeback_save）。
#[tauri::command]
pub async fn stop_game(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<String, String> {
    let mut slot = state.inner.lock().map_err(|e| e.to_string())?;
    match slot.as_mut() {
        Some(session) => {
            session.stop().map_err(|e| e.to_string())?;
            let message = match writeback_save(session) {
                Ok(()) => "游戏已停止，save 已回写".to_string(),
                Err(err) => {
                    // 回写失败显式呈现给用户（旧 save 完整，可重试会话再回写）
                    logging::error(&format!("正常停止回写失败: {err:#}"));
                    format!("游戏已停止，但 save 回写失败: {err:#}")
                }
            };
            *slot = None;
            if let Err(err) = app.emit("hud-clear", ()) {
                logging::error(&format!("hud-clear 事件发送失败: {err}"));
            }
            Ok(message)
        }
        None => Ok("没有运行中的游戏会话".into()),
    }
}

/// 窗口关闭兜底回写（PL004.10）：主窗口被直接关闭时，会话仍在运行则先停
/// 会话再回写 save（与 stop_game / exit-watcher 同一幂等 `writeback_save`），
/// 随后放行关闭；被杀 / 崩溃路径仍由 exit-watcher + Job Object 现状兜底。
///
/// # Errors
/// 会话停止或回写失败时返回错误（调用方落日志；关闭流程继续放行，旧 save
/// 保持完整可自愈）。
pub fn writeback_on_close(state: &SessionState) -> Result<()> {
    let mut slot = state
        .inner
        .lock()
        .map_err(|e| anyhow!("会话槽锁失败: {e}"))?;
    match slot.as_mut() {
        Some(session) => {
            session.stop().context("窗口关闭前停止会话失败")?;
            let result = writeback_save(session);
            *slot = None;
            result
        }
        None => Ok(()),
    }
}

/// 攻略 Markdown 原文（前端 marked 解析 + DOMPurify 消毒后渲染，CSP 兜底）。
#[tauri::command]
pub async fn get_guide() -> Result<String, String> {
    get_guide_impl().map_err(|e| format!("{e:#}"))
}

/// get_guide 的同步实现（读 game.json assets.guide 指向的适配层文档）。
fn get_guide_impl() -> Result<String> {
    let ctx = resolve_resource_ctx()?;
    let config = capsule_core::game_config::GameConfig::load(&ctx.root.join("retro/game.json"))
        .context("加载 retro/game.json 失败")?;
    let guide_path = ctx.root.join(&config.assets.guide);
    std::fs::read_to_string(&guide_path)
        .with_context(|| format!("读取攻略失败: {}", guide_path.display()))
}

/// 待机页聚合信息：游戏名 / save 档案状态 / 可选补丁列表（hack 切换数据源）。
#[derive(serde::Serialize)]
pub struct GameInfo {
    /// 游戏名（game.json name）
    pub name: String,
    /// save 档案 meta（None = 尚无 save 用户数据）
    pub save: Option<SaveMetaView>,
    /// 可选补丁列表
    pub patches: Vec<PatchView>,
}

/// save 状态视图（UI 展示形态）。
#[derive(serde::Serialize)]
pub struct SaveMetaView {
    /// save 格式版本
    pub version: String,
    /// 当前 hack 补丁 id（null = 原版）
    pub hack: Option<String>,
    /// 存档数据文件数（save/ 前缀条目）
    pub files: usize,
}

/// 补丁选项视图（UI 待机页切换项）。
#[derive(serde::Serialize)]
pub struct PatchView {
    /// 补丁 id（launch_game 的 hack 参数取值）
    pub id: String,
    /// 展示名
    pub name: String,
}

/// 读取待机页聚合信息（轻量文件读取，同步完成后由 Tauri 异步调度返回）。
#[tauri::command]
pub async fn get_game_info() -> Result<GameInfo, String> {
    get_game_info_impl().map_err(|e| format!("{e:#}"))
}

/// get_game_info 的同步实现（便于错误上下文组织）。
fn get_game_info_impl() -> Result<GameInfo> {
    let ctx = resolve_resource_ctx()?;
    let config = capsule_core::game_config::GameConfig::load(&ctx.root.join("retro/game.json"))
        .context("加载 retro/game.json 失败")?;
    let save_path = gameconf::save_path(&ctx.save_root, &config.name);
    let save = match capsule_core::save::read_meta(&save_path)
        .with_context(|| format!("读取 save 状态失败: {}", save_path.display()))?
    {
        Some(meta) => {
            let files = capsule_core::save::save_entry_count(&save_path)
                .with_context(|| format!("读取 save 内容失败: {}", save_path.display()))?
                .unwrap_or(0);
            Some(SaveMetaView {
                version: meta.version,
                hack: meta.hack,
                files,
            })
        }
        None => None,
    };
    let patches = config
        .patches
        .iter()
        .map(|p| PatchView {
            id: p.id.clone(),
            name: p.name.clone(),
        })
        .collect();
    Ok(GameInfo {
        name: config.name,
        save,
        patches,
    })
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

    /// PL004.8 验证：dev 形态识别——测试二进制无 payload，应走仓库根分支
    /// （资源根含 retro/game.json，save 根与资源根同目录，无打包 dosbox）。
    #[test]
    fn 双形态识别dev形态() {
        let ctx = resolve_resource_ctx().expect("dev 形态应可解析");
        assert!(ctx.packed_dosbox.is_none(), "测试二进制不应携带 payload");
        assert!(
            ctx.root.join("retro/game.json").is_file(),
            "dev 资源根应为仓库根: {}",
            ctx.root.display()
        );
        assert_eq!(ctx.save_root, ctx.root);
    }
}
