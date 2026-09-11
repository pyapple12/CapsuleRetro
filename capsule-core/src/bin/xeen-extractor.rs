//! xeen-extractor：XEEN CC 资源容器提取器（PL008）。
//!
//! `list` 打印容器资源清单（ID/名字/偏移/长度），`extract` 全量导出到指定
//! 目录（默认 `.xeen-extract/<容器基名小写>/`；对照表未命中的资源按
//! `XXXX.bin` 命名，XXXX 为十六进制 ID）。名字来自 capsule-core 内置对照表
//! （来源与覆盖率见 cc_archive_names.csv 头注）。关联：解析逻辑在
//! [`capsule_core::cc_archive`]，本 bin 只做 CLI 编排；错误经 String 透传
//! 非零退出（与 xmonitor 同款严格抛错风格）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use capsule_core::cc_archive::CcArchive;

#[derive(Parser)]
#[command(
    name = "xeen-extractor",
    about = "XEEN CC 资源容器提取器（list/extract）"
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
    /// 全量导出容器资源
    Extract {
        /// CC 容器路径
        cc: PathBuf,
        /// 输出目录（默认 .xeen-extract/<容器基名小写>）
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::List { cc } => list(&cc),
        Cmd::Extract { cc, out } => extract(&cc, out.as_deref()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("失败: {err}");
            ExitCode::FAILURE
        }
    }
}

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

fn extract(cc: &Path, out: Option<&Path>) -> Result<(), String> {
    let arch = CcArchive::load(cc).map_err(|e| format!("加载 {} 失败: {e}", cc.display()))?;
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
