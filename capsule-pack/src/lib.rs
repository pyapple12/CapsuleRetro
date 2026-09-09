//! capsule-pack 打包编排库：game.json → 资源校验 → payload zip → 三文件产物。
//!
//! 设计理由：编排逻辑与 CLI 壳分离，集成测试可直接驱动 [`build_package`]；
//! 格式逻辑（trailer / payload 读取 / 释放）全部复用 capsule-core 的
//! [`capsule_core::payload`]，本 crate 只负责"收集资源 + 组装产物"。
//! 产物定案（z.plan 附录 PL004）：`GameName.exe`（launcher + payload）+
//! `readme.md`（模板注入）；save 由产品首次会话生成，packer 不预置。

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// readme 模板（编译期内嵌；占位符 `{{GAME_NAME}}` / `{{VERSION}}`）。
pub const README_TEMPLATE: &str = include_str!("../readme-template.md");

/// 打包输入（路径与配置全部解析完毕，库函数不做 CLI 语义推导）。
pub struct PackInputs {
    /// 仓库根（game.json 应位于 `<仓库根>/retro/game.json`，仓库相对路径基准）
    pub repo_root: PathBuf,
    /// 适配层聚合配置（调用方已加载校验——同一份打包只读一次，FIX002.34）
    pub config: capsule_core::game_config::GameConfig,
    /// launcher release exe（裸 launcher，不得已含 payload）
    pub launcher_exe: PathBuf,
    /// dosbox 构建产物目录（内含 dosbox exe 与运行期 DLL）
    pub dosbox_dir: PathBuf,
    /// 输出目录（不存在则创建）
    pub out_dir: PathBuf,
}

/// 打包结果（CLI 打印与测试断言用）。
#[derive(Debug)]
pub struct BuildOutcome {
    /// 生成的 `GameName.exe` 路径
    pub exe: PathBuf,
    /// 生成的 readme.md 路径
    pub readme: PathBuf,
    /// payload 条目数（含 payload.json）
    pub entries: usize,
    /// 产物 exe 体积（字节）
    pub exe_bytes: u64,
    /// 非致命告警（如 dosbox Resources 缺失——打包可完成但产物不完整）
    pub warnings: Vec<String>,
}

/// 打包主流程：校验 → 收集条目 → payload zip → 拼装 exe → 注入 readme。
///
/// # Errors
/// 任一必需资源缺失、launcher 已含 payload、或 zip / IO 失败时严格报错
/// （错误信息指明缺失资源，缺一不打）。
pub fn build_package(inputs: &PackInputs) -> Result<BuildOutcome> {
    let config = &inputs.config;
    let game_json = inputs.repo_root.join("retro/game.json");

    // 打包前校验：缺一严格报错（错误策略主线）；launcher 先查存在与 payload
    // 状态，避免白拷贝 10MB 后才在拼装步报错
    require_file(&inputs.launcher_exe, "launcher exe（--launcher）")?;
    if capsule_core::payload::PayloadReader::open(&inputs.launcher_exe)
        .map_err(|e| anyhow!("launcher 读取失败: {e}"))?
        .is_some()
    {
        return Err(anyhow!(
            "launcher 已含 payload，请提供未打包的裸 launcher: {}",
            inputs.launcher_exe.display()
        ));
    }
    ensure_safe_rel(&config.paths.game_dir, "游戏目录（paths.game_dir）")?;
    let game_dir = inputs.repo_root.join("retro").join(&config.paths.game_dir);
    require_dir(&game_dir, "游戏目录（paths.game_dir）")?;
    ensure_safe_rel(&config.assets.guide, "攻略（assets.guide）")?;
    let guide = inputs.repo_root.join(&config.assets.guide);
    require_file(&guide, "攻略（assets.guide）")?;
    ensure_safe_rel(&config.assets.memory_map, "内存档案（assets.memory_map）")?;
    let memory_map = inputs.repo_root.join(&config.assets.memory_map);
    require_file(&memory_map, "内存档案（assets.memory_map）")?;
    for patch in &config.patches {
        ensure_safe_rel(
            &patch.patch,
            &format!("补丁包 {}（patches[].patch）", patch.id),
        )?;
        let path = inputs.repo_root.join("retro").join(&patch.patch);
        require_file(&path, &format!("补丁包 {}（patches[].patch）", patch.id))?;
    }
    let dosbox_exe_name = config
        .dosbox
        .exe_path
        .file_name()
        .context("dosbox.exe_path 缺少文件名")?
        .to_string_lossy()
        .into_owned();
    let dosbox_exe = inputs.dosbox_dir.join(&dosbox_exe_name);
    require_file(&dosbox_exe, "dosbox 可执行（--dosbox-dir）")?;

    // 收集条目（布局镜像仓库；排序保证产物可重现）
    let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    entries.push(("retro/game.json".to_string(), game_json));
    collect_dir(
        &game_dir,
        &format!("retro/{}", slash(&config.paths.game_dir)),
        &[],
        &mut entries,
    )?;
    entries.push((slash(&config.assets.guide), guide));
    entries.push((slash(&config.assets.memory_map), memory_map));
    for patch in &config.patches {
        entries.push((
            format!("retro/{}", slash(&patch.patch)),
            inputs.repo_root.join("retro").join(&patch.patch),
        ));
    }
    // dosbox 部署树（布局镜像构建树：bin 与 Resources 互为兄弟——shader 等
    // 运行期资源的查找依赖该相对结构，实测缺失即 ABORT）；.lib/.pdb 为构建
    // 中间产物不作部署
    collect_deploy_tree(&inputs.dosbox_dir, "dosbox/bin", &mut entries)?;
    // Resources 兄弟目录缺失不阻断打包，但必须告警（FIX002.38）：异常构建树
    // 会复现 PL004 的 shader ABORT，打包期提示优于运行期排障
    let mut warnings: Vec<String> = Vec::new();
    match inputs
        .dosbox_dir
        .parent()
        .map(|p| p.join("Resources"))
        .filter(|p| p.is_dir())
    {
        Some(res) => collect_deploy_tree(&res, "dosbox/Resources", &mut entries)?,
        None => warnings.push(format!(
            "dosbox Resources 目录缺失（期望 {}），shader 等运行期资源将不随包——请确认构建树完整",
            inputs
                .dosbox_dir
                .parent()
                .map(|p| p.join("Resources").display().to_string())
                .unwrap_or_else(|| "dosbox 目录父级的 Resources/".to_string())
        )),
    }
    // 封面/主视觉（PL006.2）：可选资产——cover 缺省仅告警不阻断（待机页
    // 回落占位）；已配置则按必需资源严格校验并随包，越界路径照常拒绝
    if let Some(cover) = &config.assets.cover {
        ensure_safe_rel(cover, "封面（assets.cover）")?;
        let path = inputs.repo_root.join(cover);
        require_file(&path, "封面（assets.cover）")?;
        entries.push((slash(cover), path));
    } else {
        warnings.push("未配置封面（assets.cover），待机页将显示占位".to_string());
    }
    if let Some(hero) = &config.assets.hero {
        ensure_safe_rel(hero, "主视觉（assets.hero）")?;
        let path = inputs.repo_root.join(hero);
        require_file(&path, "主视觉（assets.hero）")?;
        entries.push((slash(hero), path));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // 产物：payload zip → 复制 launcher → 拼装 → readme 注入
    std::fs::create_dir_all(&inputs.out_dir)
        .with_context(|| format!("创建输出目录失败: {}", inputs.out_dir.display()))?;
    let build_id = format!("{}+{}", env!("CARGO_PKG_VERSION"), unix_secs()?);
    let build_meta = serde_json::json!({ "build": build_id }).to_string();
    let payload_tmp = inputs.out_dir.join(format!("{}.payload.tmp", config.name));
    let count = write_payload_zip(&payload_tmp, &build_meta, &entries)?;
    let out_exe = inputs.out_dir.join(format!("{}.exe", config.name));
    std::fs::copy(&inputs.launcher_exe, &out_exe).with_context(|| {
        format!(
            "复制 launcher 失败: {} -> {}",
            inputs.launcher_exe.display(),
            out_exe.display()
        )
    })?;
    capsule_core::payload::append_payload(&out_exe, &payload_tmp)
        .map_err(|e| anyhow!("payload 拼装失败: {e}"))?;
    if let Err(err) = std::fs::remove_file(&payload_tmp) {
        eprintln!("警告: 临时 payload 清理失败（不影响产物正确性）: {err}");
    }
    let readme_text = README_TEMPLATE
        .replace("{{GAME_NAME}}", &config.name)
        // 运行时目录按 name 小写派生（gameconf::runtime_dir 同规则），模板
        // 单独占位注入，避免大小写漂移（FIX002.35）
        .replace("{{GAME_DIR}}", &config.name.to_ascii_lowercase())
        .replace("{{VERSION}}", env!("CARGO_PKG_VERSION"));
    let readme = inputs.out_dir.join("readme.md");
    std::fs::write(&readme, readme_text)
        .with_context(|| format!("写入 readme 失败: {}", readme.display()))?;
    let exe_bytes = std::fs::metadata(&out_exe)
        .with_context(|| format!("读取产物大小失败: {}", out_exe.display()))?
        .len();
    Ok(BuildOutcome {
        exe: out_exe,
        readme,
        entries: count,
        exe_bytes,
        warnings,
    })
}

/// 断言路径为已存在文件，缺失时报出带资源语义的错误。
fn require_file(path: &Path, label: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(anyhow!("缺少{label}: {}", path.display()))
    }
}

/// 断言路径为已存在目录，缺失时报出带资源语义的错误。
fn require_dir(path: &Path, label: &str) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(anyhow!("缺少{label}: {}", path.display()))
    }
}

/// 当前 UNIX 秒（payload 构建标识的时间成分）。
///
/// # Errors
/// 系统时钟早于 UNIX 纪元时报错（现实环境不应发生，显式暴露不做兜底）。
fn unix_secs() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("系统时钟早于 UNIX 纪元")?
        .as_secs())
}

/// 校验条目相对配置值为安全仓库相对路径（组件全部为普通名）——zip 条目名由
/// 这些值拼出，穿越形态在此拒绝（写侧防护；读侧另有 payload 校验兜底）。
fn ensure_safe_rel(value: &Path, label: &str) -> Result<()> {
    use std::path::Component;
    let ok = !value.as_os_str().is_empty()
        && value
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if ok {
        Ok(())
    } else {
        Err(anyhow!(
            "{label} 须为仓库相对路径（不含 .. / 绝对路径）: {}",
            value.display()
        ))
    }
}

/// 递归收集目录下全部文件为 zip 条目（条目名 = `<zip_prefix>/<相对名>`，
/// 跳过 `exclude_ext` 中列出的扩展名）。
fn collect_dir(
    dir: &Path,
    zip_prefix: &str,
    exclude_ext: &[&str],
    entries: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("读取目录失败: {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("遍历目录项失败: {}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let child = format!("{zip_prefix}/{name}");
        let is_dir = entry
            .file_type()
            .with_context(|| format!("读取类型失败: {}", entry.path().display()))?
            .is_dir();
        if is_dir {
            collect_dir(&entry.path(), &child, exclude_ext, entries)?;
        } else {
            let excluded = exclude_ext.iter().any(|ext| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|e| e.to_string_lossy().to_ascii_lowercase() == *ext)
            });
            if !excluded {
                entries.push((child, entry.path()));
            }
        }
    }
    Ok(())
}

/// 收集 dosbox 部署树（排除 .lib / .pdb 构建中间产物）。
fn collect_deploy_tree(
    dir: &Path,
    zip_prefix: &str,
    entries: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    collect_dir(dir, zip_prefix, &["lib", "pdb"], entries)
}

/// 写 payload zip：`payload.json` 版本标记 + 资源条目（deflate），返回条目数。
fn write_payload_zip(
    path: &Path,
    build_meta: &str,
    entries: &[(String, PathBuf)],
) -> Result<usize> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("创建临时 payload 失败: {}", path.display()))?;
    let mut writer = ZipWriter::new(file);
    let write_entry =
        |writer: &mut ZipWriter<std::fs::File>, name: &str, bytes: &[u8]| -> Result<()> {
            writer
                .start_file(name.to_string(), SimpleFileOptions::default())
                .with_context(|| format!("写入 zip 条目失败: {name}"))?;
            writer
                .write_all(bytes)
                .with_context(|| format!("写入条目字节失败: {name}"))?;
            Ok(())
        };
    write_entry(&mut writer, "payload.json", build_meta.as_bytes())?;
    for (name, src) in entries {
        let bytes =
            std::fs::read(src).with_context(|| format!("读取资源失败: {}", src.display()))?;
        write_entry(&mut writer, name, &bytes)?;
    }
    writer
        .finish()
        .with_context(|| format!("收尾 payload zip 失败: {}", path.display()))?;
    Ok(entries.len() + 1)
}
