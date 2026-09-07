//! conf 生成器：依据 GameConfig 生成 DOSBox conf 文件。
//!
//! 设计理由：launcher 不再消费 `.temp` 临时 conf——生成模板的内容是
//! xeen-test.conf 的定案值全集（z.plan PL002 阶段 B），单元测试断言生成
//! 结果与定案 conf 逐键语义一致，模板漂移会被门禁拦截。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use capsule_core::game_config::GameConfig;

/// 运行时目录：`%LOCALAPPDATA%/CapsuleRetro/<game>/`（game 由 config.name
/// 小写派生，游戏身份单一来源、不再第二处硬编码），环境变量
/// `CAPSULERETRO_RUNTIME` 可覆盖（排障/探针用）。
///
/// # Errors
/// `LOCALAPPDATA` 环境变量缺失时严格报错（审计建议：Windows 标准环境变量
/// 不做兜底降级，缺失败真环境问题应显式暴露）。
pub fn runtime_dir(game: &str) -> Result<PathBuf> {
    if let Ok(over) = std::env::var("CAPSULERETRO_RUNTIME") {
        return Ok(PathBuf::from(over));
    }
    let base = std::env::var("LOCALAPPDATA")
        .map_err(|_| anyhow::anyhow!("LOCALAPPDATA 环境变量缺失，无法定位运行时目录"))?;
    Ok(PathBuf::from(base)
        .join("CapsuleRetro")
        .join(game.to_ascii_lowercase()))
}

/// 从 `start` 向上查找包含 `retro/game.json` 的目录作为仓库根。
///
/// # Errors
/// 自 `start` 至文件系统根都找不到 `retro/game.json` 时显式报错并报告探测
/// 起点（静默返回 start 会把根因掩盖成 Io 错误）。
pub fn resolve_repo_root(start: &Path) -> Result<PathBuf> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        if d.join("retro").join("game.json").is_file() {
            return Ok(d);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Err(anyhow::anyhow!(
        "自 {} 向上未找到 retro/game.json，无法定位仓库根——请从仓库内启动",
        start.display()
    ))
}

/// 依据配置生成 DOSBox conf 全文（键值与 xeen-test.conf 定案值一致）。
pub fn generate_conf(config: &GameConfig, repo_root: &Path) -> String {
    // 统一正斜杠：与已验证可用的 mount 写法一致，避免 conf 内反斜杠歧义
    let work_dir = repo_root
        .join(&config.paths.work_dir)
        .to_string_lossy()
        .replace('\\', "/");
    // 适配层约定：game.json 的 name 即 C: 盘内启动命令的小写来源（XEEN → xeen）
    let guest = config.name.to_ascii_lowercase();
    format!(
        "# DosboxCapsule launcher 生成（来源 retro/game.json，勿手改）\n\
         [sdl]\n\
         fullscreen = false\n\n\
         [mouse]\n\
         builtin_dos_mouse_driver_options = immediate\n\
         mouse_capture = nomouse\n\
         mouse_sensitivity = 125\n\
         mouse_raw_input = on\n\n\
         [render]\n\
         aspect = auto\n\n\
         [cpu]\n\
         cpu_cycles = {cycles}\n\
         core = auto\n\n\
         [autoexec]\n\
         mount c: \"{work}\"\n\
         c:\n\
         {guest}\n\
         exit\n",
        cycles = config.dosbox.cpu_cycles,
        work = work_dir,
        guest = guest,
    )
}

/// 把 conf 写入运行时目录并返回文件路径。
pub fn write_conf(content: &str, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("创建运行时目录失败: {}", dir.display()))?;
    let path = dir.join("game.conf");
    std::fs::write(&path, content)
        .with_context(|| format!("写入 conf 失败: {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PL002.9 验证：生成 conf 与 .temp/xeen-test.conf 逐键语义一致。
    ///
    /// 注意：参考文件 .temp/xeen-test.conf 已 gitignore（y.problems #12）——
    /// 全新 clone 上此测试会因文件缺失失败，属已知登记问题，随 PL003 测试
    /// 资产落地一并修复。
    #[test]
    fn 生成conf与定案conf逐键一致() {
        let cwd = std::env::current_dir().unwrap();
        let root = resolve_repo_root(&cwd).expect("仓库根应可定位");
        let config = GameConfig::load(&root.join("retro").join("game.json")).unwrap();
        let generated = generate_conf(&config, &root);
        let reference = std::fs::read_to_string(root.join(".temp").join("xeen-test.conf")).unwrap();
        for line in reference.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            assert!(
                generated.lines().any(|g| g.trim() == t),
                "生成 conf 缺少定案行: {t}"
            );
        }
    }

    /// FIX001.24 验证：runtime_dir 按 game 小写派生，游戏身份不再硬编码。
    #[test]
    fn 运行时目录按game派生() {
        let dir = runtime_dir("XEEN").expect("本机 LOCALAPPDATA 应存在");
        let lowered = dir.to_string_lossy().to_ascii_lowercase();
        assert!(
            lowered.ends_with("\\capsuleretro\\xeen"),
            "运行时目录应以小写 xeen 结尾，实际: {}",
            dir.display()
        );
    }

    /// FIX001.21 验证：找不到仓库根必须显式报错（不再静默返回起点）。
    #[test]
    fn 仓库根定位失败显式报错() {
        // 系统临时目录下不存在 retro/game.json，向上搜索也不应命中
        let start = std::env::temp_dir();
        let err = resolve_repo_root(&start).expect_err("应显式报错");
        assert!(err.to_string().contains("retro/game.json"));
    }
}
