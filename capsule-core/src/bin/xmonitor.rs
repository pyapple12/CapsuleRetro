//! xmonitor：DOSBox Staging REST 观察诊断工具（PL001 的 A 壳）。
//!
//! 三个子命令：`watch`（按档案轮询显示游戏数值）、`search`（快照差分搜索内存
//! 偏移）、`bench`（三档读取长度性能基准）。设计理由：独立于 launcher 的诊断
//! 工具，适配任何新游戏时用它实测偏移并产出 memory_map 档案。
//!
//! 错误策略（AGENTS.md 容错白名单已登记）：watch 单次瞬时失败打印警告并计数，
//! 连续 3 次失败以退出码 1 终止；其余命令严格抛错、失败即非零退出。

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use capsule_core::game_config::GameConfig;
use capsule_core::monitor::diff::{diff_snapshots, filter_by_change};
use capsule_core::monitor::profile::MemoryMap;
use capsule_core::monitor::rest::{MonitorError, RestClient};
use clap::{Parser, Subcommand};

/// 连续失败多少次后 watch 退出（容错白名单登记值）。
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// 每档基准测试的请求次数。
const BENCH_ROUNDS: u32 = 20;

#[derive(Parser)]
#[command(name = "xmonitor", about = "DOSBox Staging REST 观察诊断工具")]
struct Cli {
    /// DOSBox webserver 端口。与 retro/game.json 的 dosbox.webserver_port 是
    /// 双处事实源、须保持一致；建议改用 --game-json 直接从配置派生
    #[arg(long, default_value_t = 18086)]
    port: u16,
    /// game.json 路径：提供时从 dosbox.webserver_port 派生端口（覆盖 --port）
    #[arg(long)]
    game_json: Option<PathBuf>,
    /// memory_map 档案路径（watch 必填）
    #[arg(long)]
    profile: Option<PathBuf>,
    /// 轮询间隔毫秒（覆盖档案中的 poll_interval_ms）
    #[arg(long)]
    interval: Option<u64>,
    /// 快照存档目录（默认定位仓库根 .temp；定位失败退化为相对 CWD 的 .temp）
    #[arg(long)]
    out_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 按档案轮询显示游戏数值（Ctrl+C 退出）
    Watch,
    /// 快照差分搜索内存偏移（可交互拍快照，或 --s1/--s2 离线回放）
    Search {
        /// 离线回放：变化前快照文件（跳过交互；与 --s2 成对使用）
        #[arg(long, requires = "s2")]
        s1: Option<PathBuf>,
        /// 离线回放：变化后快照文件（与 --s1 成对使用）
        #[arg(long, requires = "s1")]
        s2: Option<PathBuf>,
        /// 离线回放：第三快照（可选，用于二次收敛；依赖 --s1/--s2）
        #[arg(long, requires = "s1")]
        s3: Option<PathBuf>,
    },
    /// 性能基准：1KB / 64KB / 1MB 三档各取平均耗时
    Bench,
}

/// 解析生效端口：--game-json 提供时从 GameConfig 派生（端口双处事实源的
/// 同步机制），否则用 --port 默认值。
///
/// # 返回
/// 生效端口；game.json 读取或校验失败时返回退出码 2。
fn resolve_port(game_json: Option<&Path>, fallback: u16) -> Result<u16, ExitCode> {
    let Some(path) = game_json else {
        return Ok(fallback);
    };
    let config = GameConfig::load(path).map_err(|err| {
        eprintln!("game.json 加载失败 {}: {err}", path.display());
        ExitCode::from(2)
    })?;
    Ok(config.dosbox.webserver_port)
}

/// 解析快照存档目录：优先 --out-dir；否则自 CWD 向上搜索仓库根（同时含
/// `Cargo.toml` 与 `capsule-core/` 的层）落 `<root>/.temp`；找不到时退化为
/// 相对 CWD 的 `.temp`（容错白名单登记：快照存档降级，存档失败不影响拍取）。
fn resolve_snapshot_dir(explicit: Option<&Path>) -> PathBuf {
    if let Some(dir) = explicit {
        return dir.to_path_buf();
    }
    let mut current = std::env::current_dir().ok();
    while let Some(dir) = current {
        if dir.join("Cargo.toml").is_file() && dir.join("capsule-core").is_dir() {
            return dir.join(".temp");
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    eprintln!("警告: 未定位到仓库根，快照存档退化为相对 CWD 的 .temp");
    PathBuf::from(".temp")
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let port = match resolve_port(cli.game_json.as_deref(), cli.port) {
        Ok(port) => port,
        Err(code) => return code,
    };
    let out_dir = resolve_snapshot_dir(cli.out_dir.as_deref());
    let client = RestClient::new(port);
    let result = match cli.command {
        Command::Watch => run_watch(&client, cli.profile.as_deref(), cli.interval),
        Command::Search { s1, s2, s3 } => run_search(&client, s1, s2, s3, &out_dir),
        Command::Bench => run_bench(&client),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

/// watch：轮询档案条目并打印解码值。
///
/// 单次瞬时失败（连接拒绝/超时）只警告并计数，连续 [`MAX_CONSECUTIVE_FAILURES`]
/// 次失败以退出码 1 终止（容错白名单登记的边界）。
fn run_watch(
    client: &RestClient,
    profile: Option<&Path>,
    interval: Option<u64>,
) -> Result<(), ExitCode> {
    let Some(profile_path) = profile else {
        eprintln!("watch 需要 --profile <memory_map.json>");
        return Err(ExitCode::from(2));
    };
    let map = MemoryMap::load(profile_path).map_err(|err| {
        eprintln!("档案加载失败: {err}");
        ExitCode::from(2)
    })?;
    let interval_ms = interval.unwrap_or(map.poll_interval_ms).max(50);
    println!(
        "watching game={} entries={} interval={interval_ms}ms（Ctrl+C 退出）",
        map.game,
        map.entries.len()
    );

    let mut polls: u64 = 0;
    let mut failures: u64 = 0;
    let mut consecutive_failures: u32 = 0;
    loop {
        match poll_once(client, &map) {
            Ok(()) => consecutive_failures = 0,
            Err(err) => {
                failures += 1;
                consecutive_failures += 1;
                eprintln!("警告: 第 {polls} 轮轮询失败（连续第 {consecutive_failures} 次）: {err}");
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!(
                        "连续 {consecutive_failures} 次失败，终止。总轮询 {polls} 轮，失败 {failures} 次"
                    );
                    return Err(ExitCode::from(1));
                }
            }
        }
        polls += 1;
        if polls.is_multiple_of(20) {
            eprintln!("统计: 已轮询 {polls} 轮，失败 {failures} 次");
        }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
}

/// 单轮轮询：取档案全部条目并打印 `名称 | 原始字节 | 解码值`。
///
/// 读取/解码失败统一包上条目名（[`MonitorError::Entry`]），多条目轮询中
/// 可直接定位问题条目，不再裸抛 WidthMismatch 等无名错误。
fn poll_once(client: &RestClient, map: &MemoryMap) -> Result<(), MonitorError> {
    for entry in &map.entries {
        let bytes = client
            .memory(Some(entry.segment), entry.offset, u32::from(entry.len))
            .map_err(|err| MonitorError::Entry {
                name: entry.name.clone(),
                source: Box::new(err),
            })?;
        let value = entry
            .value_type
            .decode(&bytes)
            .map_err(|err| MonitorError::Entry {
                name: entry.name.clone(),
                source: Box::new(err.into()),
            })?;
        println!("{:<10} {:02x?} -> {value}", entry.name, bytes);
    }
    Ok(())
}

/// 从文件或交互式输入取得一份快照；交互模式下把快照存档到快照目录便于回放。
///
/// # 参数
/// - `offline`：离线回放的快照文件路径（None 时交互式从 REST 拍取）
/// - `tag`：快照标记（s1/s2/s3），用于提示与存档命名
/// - `out_dir`：快照存档目录（见 [`resolve_snapshot_dir`]）
fn take_snapshot(
    client: &RestClient,
    offline: Option<&Path>,
    tag: &str,
    out_dir: &Path,
) -> Result<Vec<u8>, ExitCode> {
    if let Some(path) = offline {
        return std::fs::read(path).map_err(|err| {
            eprintln!("快照文件读取失败 {}: {err}", path.display());
            ExitCode::from(1)
        });
    }
    print!("按回车拍快照 {tag}（先在游戏内准备好状态）...");
    flush_stdout();
    if read_interactive_line().is_none() {
        eprintln!("交互输入不可用，无法在确认时机拍快照，按失败退出（可用 --s1/--s2 离线回放）");
        return Err(ExitCode::from(1));
    }
    let snap = client.snapshot_lower_memory().map_err(report)?;
    if let Err(err) = std::fs::create_dir_all(out_dir) {
        eprintln!("警告: 创建 {} 失败，快照不入档: {err}", out_dir.display());
    }
    let path = out_dir.join(format!("snapshot_{tag}.bin"));
    match std::fs::write(&path, &snap) {
        Ok(()) => println!(
            "快照 {tag} 已拍，{} 字节（存档 {}）",
            snap.len(),
            path.display()
        ),
        Err(err) => eprintln!("警告: 快照存档失败 {}: {err}", path.display()),
    }
    Ok(snap)
}

/// search：差分搜索内存偏移。
fn run_search(
    client: &RestClient,
    s1: Option<PathBuf>,
    s2: Option<PathBuf>,
    s3: Option<PathBuf>,
    out_dir: &Path,
) -> Result<(), ExitCode> {
    let interactive = s1.is_none() && s2.is_none();
    let snap1 = take_snapshot(client, s1.as_deref(), "s1", out_dir)?;
    let snap2 = take_snapshot(client, s2.as_deref(), "s2", out_dir)?;
    let candidates = diff_snapshots(&snap1, &snap2).map_err(report)?;
    println!("首轮差分：{} 个候选地址", candidates.len());
    print_candidates(&snap1, &snap2, &candidates);
    if candidates.len() > 200 {
        println!("（候选过多，仅显示前 200 条；继续在游戏内改变该数值可进一步收敛）");
    }

    // 第三快照仅在交互模式下询问；离线回放必须保持非交互、可复现
    let third = if let Some(path) = s3 {
        Some(take_snapshot(client, Some(path.as_path()), "s3", out_dir)?)
    } else if interactive {
        prompt_third_snapshot(client, out_dir)?
    } else {
        None
    };
    if let Some(snap3) = third {
        let kept = filter_by_change(&snap2, &snap3, &candidates).map_err(report)?;
        println!("二次收敛：{} 个地址在第三快照中再次变化", kept.len());
        print_candidates(&snap2, &snap3, &kept);
    }
    Ok(())
}

/// 交互询问是否拍第三快照；仅交互模式会走到这里（离线回放不询问）。
fn prompt_third_snapshot(client: &RestClient, out_dir: &Path) -> Result<Option<Vec<u8>>, ExitCode> {
    print!("是否拍第三快照做二次收敛？(y/N) ");
    flush_stdout();
    let confirm = match read_interactive_line() {
        Some(line) => line.trim().eq_ignore_ascii_case("y"),
        None => false,
    };
    if confirm {
        let snap = take_snapshot(client, None, "s3", out_dir)?;
        Ok(Some(snap))
    } else {
        Ok(None)
    }
}

/// 刷新标准输出；失败打印警告（提示语可能滞留缓冲，不影响后续流程）。
fn flush_stdout() {
    if let Err(err) = std::io::stdout().flush() {
        eprintln!("警告: 刷新标准输出失败: {err}");
    }
}

/// 读一行交互输入；stdin 关闭（EOF）或读取出错时打印提示并返回 None
/// （FIX002.17：交互确认按拒绝处理，不再静默吞错）。
fn read_interactive_line() -> Option<String> {
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => {
            eprintln!("提示: 标准输入已关闭（EOF），交互确认按拒绝处理");
            None
        }
        Ok(_) => Some(line),
        Err(err) => {
            eprintln!("提示: 读取标准输入失败（{err}），交互确认按拒绝处理");
            None
        }
    }
}

/// 打印候选地址表：地址、旧值、新值（各一字节）。
fn print_candidates(prev: &[u8], curr: &[u8], addrs: &[u32]) {
    for addr in addrs.iter().take(200) {
        let idx = *addr as usize;
        println!("  {addr:#010x}  {:02x} -> {:02x}", prev[idx], curr[idx]);
    }
}

/// bench：三档读取长度各 20 次取平均耗时。
fn run_bench(client: &RestClient) -> Result<(), ExitCode> {
    println!("bench：每档 {BENCH_ROUNDS} 次请求取平均");
    for &len in &[1024u32, 65_536, 0x10_0000] {
        let mut total = Duration::ZERO;
        for _ in 0..BENCH_ROUNDS {
            let start = Instant::now();
            // 基准样本必须干净：任一请求失败即非零退出（严格抛错，不进容错白名单）
            match client.memory(None, 0, len) {
                Ok(_) => total += start.elapsed(),
                Err(err) => {
                    eprintln!("{len} 字档请求失败: {err}");
                    return Err(ExitCode::from(1));
                }
            }
        }
        let avg_ms = total.as_secs_f64() * 1000.0 / f64::from(BENCH_ROUNDS);
        println!("{len:>7} 字节  平均 {avg_ms:.2} ms");
    }
    Ok(())
}

/// 统一的错误汇报：打印后以非零码退出。
fn report(err: MonitorError) -> ExitCode {
    eprintln!("失败: {err}");
    ExitCode::from(1)
}
