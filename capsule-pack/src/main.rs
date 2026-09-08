//! capsule-pack CLI 壳：参数解析与仓库根 / dosbox 目录推导，打包逻辑在 lib。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

/// capsule-pack 命令行参数。
#[derive(Parser)]
#[command(about = "CapsuleRetro 打包器：game.json + launcher exe → GameName.exe + readme.md")]
struct Args {
    /// game.json 路径（应形如 <仓库根>/retro/game.json）
    game_json: PathBuf,
    /// launcher release exe（未含 payload 的裸 launcher，如 target/release/launcher.exe）
    #[arg(long)]
    launcher: PathBuf,
    /// 输出目录
    #[arg(short, long, default_value = "dist")]
    out: PathBuf,
    /// dosbox 构建产物目录（内含 dosbox exe 与 DLL；默认取 game.json dosbox.exe_path 的父目录）
    #[arg(long)]
    dosbox_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo_root = args
        .game_json
        .parent()
        .and_then(Path::parent)
        .context("game.json 路径须形如 <仓库根>/retro/game.json（至少两级）")?
        .to_path_buf();
    let config = capsule_core::game_config::GameConfig::load(&args.game_json)
        .with_context(|| format!("加载 {}", args.game_json.display()))?;
    let dosbox_dir = match args.dosbox_dir {
        Some(dir) => dir,
        None => repo_root
            .join(&config.dosbox.exe_path)
            .parent()
            .with_context(|| {
                format!(
                    "dosbox.exe_path 缺少父目录: {}",
                    config.dosbox.exe_path.display()
                )
            })?
            .to_path_buf(),
    };
    let outcome = capsule_pack::build_package(&capsule_pack::PackInputs {
        repo_root,
        launcher_exe: args.launcher,
        dosbox_dir,
        out_dir: args.out,
    })?;
    println!("已生成 {}", outcome.exe.display());
    println!("已生成 {}", outcome.readme.display());
    println!(
        "payload {} 个条目，产物 {} 字节",
        outcome.entries, outcome.exe_bytes
    );
    Ok(())
}
