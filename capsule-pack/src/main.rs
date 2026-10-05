//! capsule-pack CLI 壳：参数解析与仓库根 / dosbox 目录推导，打包逻辑在 lib。

use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
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
    // 校验与内嵌一致性断言（A003 #3）：build_package 用 args.game_json 加载的
    // config 做校验、内嵌的却是 <repo_root>/retro/game.json——两者必须同一
    // 文件，否则"校验 A 打包 B"的产物静默错配
    let expected = repo_root.join("retro").join("game.json");
    let actual = args
        .game_json
        .canonicalize()
        .with_context(|| format!("定位 {}", args.game_json.display()))?;
    let expected_canonical = expected.canonicalize().with_context(|| {
        format!(
            "仓库内缺少 {}（校验与内嵌必须同一文件——请把 game.json 放到该规范路径，\
             或用 <仓库根>/retro/game.json 调用打包器）",
            expected.display()
        )
    })?;
    ensure!(
        actual == expected_canonical,
        "game.json 与内嵌路径不一致：校验用 {}，内嵌用 {}（须同一文件——请用 \
         <仓库根>/retro/game.json 调用打包器）",
        args.game_json.display(),
        expected.display()
    );
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
        config,
        launcher_exe: args.launcher,
        dosbox_dir,
        out_dir: args.out,
    })?;
    for warning in &outcome.warnings {
        println!("警告: {warning}");
    }
    println!("已生成 {}", outcome.exe.display());
    println!("已生成 {}", outcome.readme.display());
    println!(
        "payload {} 个条目，产物 {} 字节",
        outcome.entries, outcome.exe_bytes
    );
    Ok(())
}
