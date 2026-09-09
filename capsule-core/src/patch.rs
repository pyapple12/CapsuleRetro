//! 补丁包（zip：`manifest.json` + `files/`）的解析与应用。
//!
//! 补丁纪律（产品核心）：[`apply`] 只写 `work_dir`——调用方保证那是运行时
//! 释放出的工作副本，绝不是原版目录；原版游戏文件永不被修改。
//! manifest 定案（z.plan 附录 PL003 §数据格式）：条目数组，`action` 仅
//! `replace`（覆盖已存在文件，目标缺失报错）/ `add`（新增文件，目标已存在
//! 报错）；`path` 为工作副本内相对路径，严格拒绝路径穿越（zip slip 家族）。

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;
use zip::ZipArchive;

/// 补丁包内 manifest 条目名。
const MANIFEST_ENTRY: &str = "manifest.json";

/// 补丁解析与应用错误。
#[derive(Debug, Error)]
pub enum PatchError {
    /// 补丁包文件读取失败。
    #[error("补丁包读取失败 {path}: {source}")]
    Io {
        /// 出错的路径。
        path: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// 补丁包结构损坏（非 zip / 缺 manifest / manifest 非法）。
    #[error("补丁包损坏: {0}")]
    Corrupted(String),
    /// manifest path 非法（路径穿越 / 绝对路径 / 反斜杠 / 空）。
    #[error("manifest 路径非法: {0}")]
    UnsafePath(String),
    /// manifest action 不在 replace / add 之内。
    #[error("manifest action 非法: {0}")]
    UnknownAction(String),
    /// replace 目标不存在（manifest 写错或释放源变更）。
    #[error("replace 目标不存在: {0}")]
    MissingTarget(String),
    /// add 目标已存在（重复 add 或释放源已含同名文件）。
    #[error("add 目标已存在: {0}")]
    TargetExists(String),
    /// manifest source 指向的补丁内文件缺失。
    #[error("补丁 source 缺失: {0}")]
    MissingSource(String),
}

/// manifest 单条目（serde 形态）。
#[derive(Debug, Deserialize)]
struct ManifestEntry {
    /// 动作：replace（覆盖已存在）或 add（新增）。
    action: String,
    /// 工作副本内目标相对路径（`/` 分隔）。
    path: String,
    /// 补丁包内来源路径（相对包根，通常 `files/...`）。
    source: String,
}

/// 应用补丁包到工作副本，返回成功应用的条目数。
///
/// # Errors
/// 包打开失败返回 [`PatchError::Io`]；结构损坏返回 [`PatchError::Corrupted`]；
/// 任一条目校验失败（路径越界 / action 非法 / 目标冲突 / source 缺失）严格
/// 上抛并中止（工作副本可能已写入前序条目，由启动序列的全量重建保证下次
/// 启动自愈）。
pub fn apply(patch_zip: &Path, work_dir: &Path) -> Result<usize, PatchError> {
    let file = std::fs::File::open(patch_zip).map_err(|source| PatchError::Io {
        path: patch_zip.to_path_buf(),
        source,
    })?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| PatchError::Corrupted(format!("zip 结构非法: {e}")))?;

    let manifest_text = {
        let mut entry = archive
            .by_name(MANIFEST_ENTRY)
            .map_err(|e| PatchError::Corrupted(format!("缺 {MANIFEST_ENTRY}: {e}")))?;
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|e| PatchError::Corrupted(format!("{MANIFEST_ENTRY} 读取失败: {e}")))?;
        text
    };
    let entries: Vec<ManifestEntry> = serde_json::from_str(&manifest_text)
        .map_err(|e| PatchError::Corrupted(format!("{MANIFEST_ENTRY} 解析失败: {e}")))?;

    let mut applied = 0;
    for entry in &entries {
        let target_rel = validate_rel(&entry.path)?;
        let target = work_dir.join(&target_rel);
        // 结构校验（action / 目标冲突）先于读 source，错误语义不混淆
        match entry.action.as_str() {
            "replace" => {
                if !target.is_file() {
                    return Err(PatchError::MissingTarget(entry.path.clone()));
                }
            }
            "add" => {
                if target.exists() {
                    return Err(PatchError::TargetExists(entry.path.clone()));
                }
            }
            other => return Err(PatchError::UnknownAction(other.to_string())),
        }
        let bytes = read_zip_file(&mut archive, &entry.source)?.ok_or_else(|| {
            PatchError::MissingSource(format!("{}（条目 {}）", entry.source, entry.path))
        })?;

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|source| PatchError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(&target, bytes).map_err(|source| PatchError::Io {
            path: target.clone(),
            source,
        })?;
        applied += 1;
    }
    Ok(applied)
}

/// 读 zip 内指定条目字节；`Ok(None)` = 条目不存在（调用方报 MissingSource），
/// 条目存在但读取/解压失败返回 [`PatchError::Corrupted`]——两种失败区分
/// 报告，避免数据损坏被误判为缺失误导排查（FIX002.14）。by_name 阶段的
/// 失败视为缺失：ZipArchive::new 已完成中心目录解析，此后按名查不到即
/// 不存在，读取期错误由 read_to_end 以 Corrupted 呈现。
fn read_zip_file(
    archive: &mut ZipArchive<std::fs::File>,
    name: &str,
) -> Result<Option<Vec<u8>>, PatchError> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| PatchError::Corrupted(format!("补丁条目读取失败 {name}: {e}")))?;
    Ok(Some(bytes))
}

/// 校验 manifest path：非空、不含反斜杠、组件全部为普通名（拒绝 `..` / 根 /
/// 盘符等路径穿越形态）。
fn validate_rel(path: &str) -> Result<PathBuf, PatchError> {
    if path.is_empty() || path.contains('\\') {
        return Err(PatchError::UnsafePath(path.to_string()));
    }
    let rel = Path::new(path);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(PatchError::UnsafePath(path.to_string()));
    }
    Ok(rel.to_path_buf())
}
