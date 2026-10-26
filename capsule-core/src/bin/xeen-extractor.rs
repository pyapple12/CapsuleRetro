//! xeen-extractor：XEEN CC 资源容器提取器（PL008）。
//!
//! `list` 打印容器资源清单（ID/名字/偏移/长度），`extract` 全量导出到指定
//! 目录（默认 `.xeen-extract/<容器基名小写>/`；对照表未命中的资源按
//! `XXXX.bin` 命名，XXXX 为十六进制 ID），`mazes` 解析 CUR 容器内全部迷宫
//! 布局并输出汇总校验（PL010）。名字来自 capsule-core 内置对照表
//! （来源与覆盖率见 cc_archive_names.csv 头注）。关联：解析逻辑在
//! [`capsule_core::cc_archive`]，本 bin 只做 CLI 编排；错误经 String 透传
//! 非零退出（与 xmonitor 同款严格抛错风格）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use capsule_core::cc_archive::CcArchive;
use capsule_core::maze::{maze_file_name, MazeMap};

#[derive(Parser)]
#[command(
    name = "xeen-extractor",
    about = "XEEN CC 资源容器提取器（list/extract/mazes）"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 列出容器内全部资源（ID/名字/偏移/长度）
    List {
        /// CC 容器路径
        cc: PathBuf,
    },
    /// 全量导出容器资源（面向 XOR 混淆的 .CC 容器；明文 .CUR 请用 mazes）
    Extract {
        /// CC 容器路径
        cc: PathBuf,
        /// 输出目录（默认 .xeen-extract/<容器基名小写>）
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// 解析容器内全部迷宫数据并输出汇总（CUR 容器，PL010.2 全量校验）
    Mazes {
        /// CUR 容器路径（数据区为明文，不做 XOR）
        cc: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::List { cc } => list(&cc),
        Cmd::Extract { cc, out } => extract(&cc, out.as_deref()),
        Cmd::Mazes { cc } => mazes(&cc),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("失败: {err}");
            ExitCode::FAILURE
        }
    }
}

/// 打印容器资源清单（ID/名字/偏移/长度）与命名覆盖率摘要。
fn list(cc: &Path) -> Result<(), String> {
    let arch = CcArchive::load(cc).map_err(|e| format!("加载 {} 失败: {e}", cc.display()))?;
    let named = arch.entries.iter().filter(|e| e.name.is_some()).count();
    println!(
        "# {} — {} 资源（有名 {named}），容器 {} 字节",
        cc.display(),
        arch.entries.len(),
        arch.size()
    );
    for e in &arch.entries {
        let name = e.name.as_deref().unwrap_or("-");
        println!(
            "{:#06x} {name:<16} off={:#010x} len={}",
            e.id, e.offset, e.len
        );
    }
    Ok(())
}

/// 全量导出容器资源；输出目录缺省回落 `<容器基名小写>/`。
fn extract(cc: &Path, out: Option<&Path>) -> Result<(), String> {
    let arch = CcArchive::load(cc).map_err(|e| format!("加载 {} 失败: {e}", cc.display()))?;
    // FIX005.8：CUR 容器数据区为明文（见 mazes 子命令），extract 恒走 XOR
    // 0x35 还原——对 CUR 误用会静默成功产出全错字节，在此显式警告（保留
    // 取证用途不硬拒绝）
    if cc
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cur"))
    {
        eprintln!(
            "警告: {} 为明文 CUR 容器，extract（XOR 0x35）将产出乱码字节——CUR 请用 mazes 子命令",
            cc.display()
        );
    }
    let stem = cc
        .file_stem()
        .ok_or_else(|| "容器路径缺文件名".to_string())?
        .to_string_lossy()
        .to_ascii_lowercase();
    let dir = out
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".xeen-extract").join(&stem));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("创建输出目录失败 {}: {e}", dir.display()))?;
    let mut named = 0usize;
    for e in &arch.entries {
        let data = arch.read(e).map_err(|err| format!("{err}"))?;
        let file = match &e.name {
            Some(name) => {
                named += 1;
                name.to_string()
            }
            None => format!("{:04X}.bin", e.id),
        };
        std::fs::write(dir.join(&file), &data).map_err(|err| format!("写出 {file} 失败: {err}"))?;
    }
    println!(
        "已导出 {} 资源（有名 {named}）→ {}",
        arch.entries.len(),
        dir.display()
    );
    Ok(())
}

/// 全量解析容器内迷宫（CUR 明文数据），汇总主图/变体/标志；任一解析错误即整体失败。
fn mazes(cc: &Path) -> Result<(), String> {
    let arch = CcArchive::load(cc).map_err(|e| format!("加载 {} 失败: {e}", cc.display()))?;
    let mut primary = 0usize;
    let mut variants: Vec<String> = Vec::new();
    let mut outdoor = 0usize;
    let mut dark = 0usize;
    let mut rest = 0usize;
    let mut savable = 0usize;
    for mid in 1u16..1000 {
        let name = maze_file_name(mid);
        let Some(entry) = arch.find(&name) else {
            continue;
        };
        let raw = arch
            .read_raw(entry)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?;
        let m = MazeMap::parse(mid, &raw).map_err(|e| format!("{name}: {e}"))?;
        if m.id_field == m.file_id {
            primary += 1;
            if m.is_outdoors {
                outdoor += 1;
            }
            if m.is_dark {
                dark += 1;
            }
            if m.can_rest {
                rest += 1;
            }
            if m.can_save {
                savable += 1;
            }
        } else {
            variants.push(format!("{}→{}", m.file_id, m.id_field));
        }
    }
    println!(
        "# {} — 迷宫 {}（主 {primary} / 变体 {}），户外 {outdoor}、黑暗 {dark}、可休息 {rest}、可存盘 {savable}",
        cc.display(),
        primary + variants.len(),
        variants.len()
    );
    println!("变体: {}", variants.join(", "));
    Ok(())
}
