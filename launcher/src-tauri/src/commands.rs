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
//! "无会话"语义：inject_input 返回错误消息"当前无游戏会话"；stop_game
//! 对无会话是幂等成功（Ok）。
//! 安全边界：spawn 参数全部来自仓库内 `retro/game.json` 与 u16 端口探测值，
//! 无任何外部用户输入参与；进程创建经 `winproc` 的 CreateProcessW 参数列表完成。

use std::io::Read as _;
use std::net::TcpListener;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use tauri::Emitter;
use windows::Win32::Foundation::HANDLE;

use crate::gameconf;
use crate::hud::{HudPollEvent, HudPoller};
use crate::jobctl::JobHandle;
use crate::logging;
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

/// 会话序号发生器（FIX002.23）：launch 次序单调递增，退出事件携带序号供
/// 前端与陈旧事件比对，防止旧会话的事件把新会话 UI 拉回待机。
static SESSION_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 一次游戏会话的全部资源。
pub struct Session {
    process: WinProcess,
    job: JobHandle,
    port: u16,
    profile: PathBuf,
    hud: Option<HudPoller>,
    agent: ureq::Agent,
    pid: u32,
    /// 会话序号（launch 单调递增，退出事件比对依据）
    seq: u64,
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

/// 端口锁死校验（PL005 定案"端口锁死 18086"，FIX002.1 落实）：仅测试
/// `port` 本身可绑定，冲突时显式报错——不做回退。理由：前端帧流地址与
/// CSP 均只放行该端口，回退会让画面链路静默失联，甚至在端口被另一个
/// CapsuleRetro 实例占用时串显别个会话的画面。
///
/// # 已知竞态（TOCTOU）
/// 本函数释放端口到 DOSBox 实际 bind 之间存在被其他进程抢占的窗口；被抢时
/// webserver 起不来，由 [`start_session`] 的就绪探测以超时报错兜底呈现，
/// 不做更复杂的预占持有（持有再转交需要 fd 传递，复杂度不值）。
pub fn probe_port(port: u16) -> Result<u16> {
    // 对 WSAEADDRINUSE(10048) 短重试：上一会话刚终止时监听端点有内核延迟
    // 清理窗口，立刻重启会瞬时占用报错（与 release_work 的 error 5 同族）；
    // 重试耗尽仍占用则显式报错——不回退端口，锁死契约不变
    let mut attempt = 0;
    loop {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => {
                drop(listener);
                return Ok(port);
            }
            Err(err) if err.raw_os_error() == Some(10048) && attempt < 4 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(400));
            }
            Err(err) => {
                return Err(anyhow!(
                    "端口 {port} 被其他程序占用（可能是另一个 dosbox/会话）——帧流与注入通道锁死该端口，请释放后重试: {err}"
                ))
            }
        }
    }
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

/// save 根可写探针（FIX002.37）：create_dir_all + 临时探针文件写入后删除。
/// exe 被放进只读目录（如 Program Files）时回写会恒败，启动期显式报错
/// 优于结束期静默丢档。
///
/// # Errors
/// 目录不可创建或探针文件不可写时返回带部署指引的错误。
fn ensure_save_root_writable(save_root: &Path) -> Result<()> {
    std::fs::create_dir_all(save_root).with_context(|| {
        format!(
            "save 目录不可创建（位置是否只读或受限？请将 exe 放在普通可写目录）: {}",
            save_root.display()
        )
    })?;
    let probe = save_root.join(".capsuleretro-probe.tmp");
    std::fs::write(&probe, b"probe").with_context(|| {
        format!(
            "save 目录不可写（exe 是否放在只读/需管理员权限目录，如 Program Files？请移到普通可写目录）: {}",
            save_root.display()
        )
    })?;
    if let Err(err) = std::fs::remove_file(&probe) {
        logging::error(&format!(
            "save 可写探针文件清理失败 {}: {err}",
            probe.display()
        ));
    }
    Ok(())
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
    ensure_save_root_writable(&ctx.save_root)?;
    let patch_zip = resolve_patch_zip(&ctx.root, &config, hack.as_deref())?;
    let default_meta =
        capsule_core::save::SaveMeta::new(capsule_core::save::SAVE_META_VERSION, hack.clone());
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

    // 完整性断言：释放内容必须含游戏可执行文件（game.json 配错在此显式
    // 暴露；exe 基名 ≠ name 的游戏由 paths.guest_exe 显式声明，FIX002.25）
    let guest_exe = config.guest_exe_name();
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
    let seq = SESSION_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    Ok(Session {
        pid: process.pid(),
        seq,
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

/// session-ended 事件载荷：会话标识（pid + seq）与回写结果。前端据 seq
/// 比对事件是否属于当前会话（陈旧事件忽略，FIX002.23），据 saved 区分
/// "存档已保存 / 回写失败"文案（FIX002.30）。
#[derive(Clone, serde::Serialize)]
pub struct SessionEnded {
    /// 退出的 DOSBox 进程 pid
    pub pid: u32,
    /// 会话序号（launch 单调递增）
    pub seq: u64,
    /// save 回写是否成功
    pub saved: bool,
}

/// 进程退出监听线程：等句柄触发（进程死）→ 若槽内仍是本会话（pid + seq
/// 双比对，FIX002.23）则取走会话——检查与清槽合并为一次持锁、回写与广播
/// 不持锁（不阻塞 launch 首查 / 注入），停 HUD、回写 save、广播
/// `session-ended`。持有的是 DuplicateHandle 出的独立句柄副本，与 Session
/// 的句柄生命周期解耦。
fn spawn_exit_watcher(
    app: tauri::AppHandle,
    sessions: Arc<Mutex<Option<Session>>>,
    watch: SendHandle,
    pid: u32,
    seq: u64,
) {
    std::thread::spawn(move || {
        winproc::wait_forever_and_close(watch);
        // 检查与清槽一次持锁：消除"检查时是本会话、取走时已被换新"的释放窗口
        let ended = {
            let mut slot = match sessions.lock() {
                Ok(slot) => slot,
                Err(_) => return,
            };
            let is_same = slot.as_ref().is_some_and(|s| s.pid == pid && s.seq == seq);
            if is_same {
                slot.take()
            } else {
                None
            }
        };
        let Some(mut session) = ended else {
            return; // 已被 stop_game 正常收尾或已是新会话，不越权处理
        };
        if let Some(poller) = session.hud.take() {
            poller.stop();
        }
        // 进程已死，Job 终止仅为兜底收尾；失败不影响事件广播
        let _ = session.job.kill();
        // 崩溃/自退路径同样回写（被杀时游戏已写盘的存档不丢）
        let saved = match writeback_save(&mut session) {
            Ok(()) => true,
            Err(err) => {
                logging::error(&format!("会话退出回写失败: {err:#}"));
                false
            }
        };
        logging::info(&format!("游戏进程退出（pid {pid}），会话槽已清"));
        if let Err(err) = app.emit("session-ended", SessionEnded { pid, seq, saved }) {
            logging::error(&format!("session-ended 事件发送失败: {err}"));
        }
    });
}

/// launch 成功信息（FIX002.30/FIX002.23：seq 供前端比对 session-ended 事件）。
#[derive(serde::Serialize)]
pub struct LaunchInfo {
    /// webserver 端口（锁死 18086，展示用）
    pub port: u16,
    /// 会话序号：session-ended 事件据此比对是否当前会话
    pub seq: u64,
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
) -> Result<LaunchInfo, String> {
    if state.inner.lock().map_err(|e| e.to_string())?.is_some() {
        return Err("已有游戏会话在运行".into());
    }
    let mut session = start_session(hack).map_err(|e| format!("{e:#}"))?;
    let port = session.port();
    let seq = session.seq;
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
        seq,
    );

    let mut slot = state.inner.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
        // 竞态兜底：等待期间另一路已入槽，退掉本路刚构造的会话
        drop(slot);
        if let Err(err) = session.stop() {
            logging::error(&format!("竞态败者会话停止失败（Job 兜底杀树）: {err:#}"));
        }
        return Err("已有游戏会话在运行".into());
    }
    *slot = Some(session);
    Ok(LaunchInfo { port, seq })
}

/// 注入输入事件（UI 命令，body 为注入端点的 JSON 字符串透传）。async 化：
/// 注入 HTTP 等待（超时 3s）在异步运行时线程执行，不占主线程。
#[tauri::command]
pub async fn inject_input(
    state: tauri::State<'_, SessionState>,
    payload: String,
) -> Result<String, String> {
    let body = {
        // 锁边界注记（FIX002.22）：持锁跨越注入 HTTP 请求（超时 3s），期间
        // launch 首查 / exit-watcher 清槽被阻塞；注入高频且必须绑定会话
        // 生命周期，保持现状
        let slot = state.inner.lock().map_err(|e| e.to_string())?;
        let session = slot.as_ref().ok_or_else(|| "当前无游戏会话".to_string())?;
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
    // 锁边界注记（FIX002.22）：持锁跨越 wait_exit（≤5s）+ zip 回写——期间
    // launch 首查 / 注入被阻塞；停止是终态操作且要求独占会话，有界可接受
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
/// 锁边界注记（FIX002.22）：运行于主线程窗口事件回调，持锁跨越停止（≤5s）
/// 与回写——关窗是一次性终态操作，可接受。
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

/// 性能采样分布（PL005 测量）：前端环形采样的摘要，经 log_perf_stats 落
/// launcher.log 的 PERF 行——基线与改后对比共用同一通道。
#[derive(serde::Deserialize)]
pub struct PerfStats {
    /// 采样类别（inject_rtt / frame_age / parse_draw）
    pub kind: String,
    /// 样本数
    pub count: u32,
    /// p50（毫秒）
    pub p50: f64,
    /// p95（毫秒）
    pub p95: f64,
    /// 最大值（毫秒）
    pub max: f64,
}

/// 上报一条性能采样摘要（PL005 测量通道）。
#[tauri::command]
pub async fn log_perf_stats(stats: PerfStats) -> Result<(), String> {
    logging::info(&format!(
        "PERF {} count={} p50={:.2}ms p95={:.2}ms max={:.2}ms",
        stats.kind, stats.count, stats.p50, stats.p95, stats.max
    ));
    Ok(())
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

/// 待机页聚合信息：游戏名 / 封面 / save 档案状态 / 可选补丁列表（hack 切换数据源）。
#[derive(serde::Serialize)]
pub struct GameInfo {
    /// 游戏名（game.json name）
    pub name: String,
    /// 封面 data URL（None = 未配置封面，前端回落占位）；以内联 data: 下发，
    /// 复用 CSP 已放行的 img-src data:，不引入 asset protocol 文件暴露面
    pub cover: Option<String>,
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
    let cover = match &config.assets.cover {
        Some(cover) => {
            let path = ctx.root.join(cover);
            let bytes = std::fs::read(&path)
                .with_context(|| format!("读取封面失败: {}", path.display()))?;
            Some(cover_data_url(&bytes, &path)?)
        }
        None => None,
    };
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
        cover,
        save,
        patches,
    })
}

/// 会话内数值修改结果（金手指反馈）。
#[derive(serde::Serialize)]
pub struct CheatAdjusted {
    /// 修改的档案条目名（如 "gold"）。
    pub field: String,
    /// 饱和封顶后的新值。
    pub value: i64,
}

/// 队伍头像图（PL009.2）：id = 面容文件号（1 起，HUD face 值 + 1）。
#[derive(serde::Serialize)]
pub struct FaceImage {
    /// 面容文件号（名册槽位 + 1）
    pub id: u8,
    /// PNG data URL（内联下发，复用 CSP 已放行的 img-src data:）
    pub data_url: String,
}

/// 读取队伍头像资产（faces_dir 内 charNN.png，一次性取图）。
///
/// # Errors
/// game.json 缺失/非法、faces_dir 已配置但目录缺失或文件名不合
/// charNN.png 规范时返回错误消息；faces_dir 缺省返回空表（HUD 回落占位）。
#[tauri::command]
pub async fn get_faces() -> Result<Vec<FaceImage>, String> {
    get_faces_impl().map_err(|e| format!("{e:#}"))
}

/// get_faces 的同步实现：枚举 faces_dir 并按文件号排序编码。
fn get_faces_impl() -> Result<Vec<FaceImage>> {
    let ctx = resolve_resource_ctx()?;
    let config = capsule_core::game_config::GameConfig::load(&ctx.root.join("retro/game.json"))
        .context("加载 retro/game.json 失败")?;
    let Some(faces_dir) = &config.assets.faces_dir else {
        return Ok(Vec::new());
    };
    let dir = ctx.root.join(faces_dir);
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("读取头像目录失败: {}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("遍历头像目录失败: {}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    let mut faces = Vec::new();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = face_id_from_name(&name) else {
            anyhow::bail!("头像目录存在不合规范的文件: {}", name);
        };
        let bytes = std::fs::read(entry.path())
            .with_context(|| format!("读取头像失败: {}", entry.path().display()))?;
        faces.push(FaceImage {
            id,
            data_url: cover_data_url(&bytes, &entry.path())?,
        });
    }
    Ok(faces)
}

/// 从 `charNN.png` 文件名解析面容文件号（NN 为两位数字）。
fn face_id_from_name(name: &str) -> Option<u8> {
    let stem = name.strip_prefix("char")?.strip_suffix(".png")?;
    if stem.len() != 2 || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    stem.parse().ok()
}

/// 会话内数值修改（金手指）：按 memory_map 档案条目读-改-写，白名单由
/// capsule-core monitor 把关。async 化：两次 HTTP（读+写，各 5s 超时）在
/// 异步运行时线程执行，不占主线程。
///
/// # Errors
/// 无会话 / 档案缺失 / 未知条目 / 白名单拒绝 / 传输失败时返回错误消息。
#[tauri::command]
pub async fn cheat_adjust(
    state: tauri::State<'_, SessionState>,
    field: String,
    delta: i64,
) -> Result<CheatAdjusted, String> {
    let port = {
        let slot = state.inner.lock().map_err(|e| e.to_string())?;
        let session = slot.as_ref().ok_or_else(|| "当前无游戏会话".to_string())?;
        session.port()
    };
    // 锁外执行 HTTP——与注入不同：修改非高频、无需绑定会话生命周期；
    // 会话中途退出表现为传输失败，无一致性风险
    cheat_adjust_impl(port, &field, delta).map_err(|e| format!("{e:#}"))
}

/// cheat_adjust 的同步实现（便于错误上下文组织）。
fn cheat_adjust_impl(port: u16, field: &str, delta: i64) -> Result<CheatAdjusted> {
    let ctx = resolve_resource_ctx()?;
    let config = capsule_core::game_config::GameConfig::load(&ctx.root.join("retro/game.json"))
        .context("加载 retro/game.json 失败")?;
    let map_path = ctx.root.join(&config.assets.memory_map);
    let map = capsule_core::monitor::profile::MemoryMap::load(&map_path)
        .with_context(|| format!("加载内存档案失败: {}", map_path.display()))?;
    let client = capsule_core::monitor::rest::RestClient::new(port);
    let value = client
        .adjust_field(&map, field, delta)
        .with_context(|| format!("修改 {field} 失败"))?;
    Ok(CheatAdjusted {
        field: field.to_string(),
        value,
    })
}

/// 封面字节转 data URL（按扩展名定 MIME；未知扩展名严格报错不猜类型）。
///
/// # Errors
/// 封面扩展名非 jpg/jpeg/png 时返回错误。
fn cover_data_url(bytes: &[u8], path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let mime = match ext.as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => return Err(anyhow!("封面仅支持 jpg/png: {}", path.display())),
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PL006.4 验证：封面按扩展名转 data URL（大小写不敏感），编码可无损
    /// 解码回原字节；未知扩展名严格报错。
    #[test]
    fn 封面data_url按扩展名判定() {
        let raw = b"\x89PNG\r\n\x1a\nfake";
        let url = cover_data_url(raw, Path::new("cover.PNG")).expect("png 应可转");
        assert!(url.starts_with("data:image/png;base64,"), "{url}");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(url.trim_start_matches("data:image/png;base64,"))
            .expect("data URL 应可解码");
        assert_eq!(decoded, raw);

        let err = cover_data_url(b"GIF", Path::new("cover.gif")).expect_err("未知扩展名应报错");
        assert!(err.to_string().contains("jpg/png"), "{err:#}");
    }

    /// FIX002.1 验证：端口锁死无回退——被占端口显式报错（含占用提示），
    /// 空闲端口原样返回。
    #[test]
    fn 端口探测锁死无回退() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定临时端口");
        let busy = listener.local_addr().unwrap().port();
        let err = probe_port(busy).expect_err("被占端口应报错");
        assert!(err.to_string().contains("被其他程序占用"), "实际: {err}");

        let listener2 = TcpListener::bind("127.0.0.1:0").expect("再绑临时端口");
        let free = listener2.local_addr().unwrap().port();
        drop(listener2);
        let picked = probe_port(free).expect("空闲端口应可用");
        assert_eq!(picked, free, "锁死契约应原样返回请求端口");
    }

    /// FIX002.37 验证：save 根可写探针——文件路径冒充目录 → 明确报错；
    /// 正常目录 → 通过且探针文件被清理。
    #[test]
    fn save根可写探针() {
        let dir = std::env::temp_dir().join(format!("cr-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        ensure_save_root_writable(&dir).expect("正常目录应通过");
        assert!(
            !dir.join(".capsuleretro-probe.tmp").exists(),
            "探针文件应随探测删除"
        );

        let file_as_dir = dir.join("not-a-dir");
        std::fs::write(&file_as_dir, b"x").expect("写占位文件");
        let err = ensure_save_root_writable(&file_as_dir).expect_err("文件路径应报错");
        assert!(
            err.to_string().contains("save 目录"),
            "错误应带 save 目录指引，实际: {err}"
        );
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
