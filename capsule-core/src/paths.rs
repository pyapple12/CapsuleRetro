//! 运行时目录释放与 save 生命周期编排（启动前置序列 / 退出回写序列）。
//!
//! 设计理由：launcher 只看到 [`prepare_session`] 与 [`writeback`] 两个编排
//! 入口，释放/补丁/存档的次序与幂等性收敛在此，供 launcher 与未来 packer
//! 共用。自愈策略定案：**每次启动全量重建工作副本**（先清空后从原版复制）
//! ——残留与损坏天然自愈，hack 切换互不污染（VA2 的根基），无需增量校验。

use std::path::Path;

use thiserror::Error;

use crate::patch::{self, PatchError};
use crate::save::{self, SaveError, SaveMeta};

/// 生命周期编排错误。
#[derive(Debug, Error)]
pub enum LifecycleError {
    /// 原版游戏目录不存在（game.json paths.game_dir 配错或素材缺失）。
    #[error("原版游戏目录不存在: {0}")]
    MissingOriginal(String),
    /// 工作副本释放（清空/复制）失败。
    #[error("释放工作副本失败: {0}")]
    Release(#[from] std::io::Error),
    /// save 档案读写失败。
    #[error(transparent)]
    Save(#[from] SaveError),
    /// 补丁应用失败。
    #[error(transparent)]
    Patch(#[from] PatchError),
}

/// 启动前置序列：全量重建工作副本 → 应用补丁（可选）→ 解包 save（含损坏
/// 降级），返回 `(meta, recovered)`。
///
/// # 参数
/// - `original_dir`：原版游戏目录（只读源，本函数保证不写入）
/// - `work_dir`：工作副本目录（运行时释放目标，会被清空重建）
/// - `patch_zip`：hack 补丁包路径（None = 原版形态）
/// - `save_zip`：save 档案路径（不存在时以 `default_meta` 继续）
/// - `default_meta`：save 缺失/降级时使用的元数据
///
/// # Errors
/// 原版目录缺失、释放失败、补丁校验失败严格上抛；save 损坏走白名单降级
/// （见 [`save::load_or_recover`]）。
pub fn prepare_session(
    original_dir: &Path,
    work_dir: &Path,
    patch_zip: Option<&Path>,
    save_zip: &Path,
    default_meta: SaveMeta,
) -> Result<(SaveMeta, bool), LifecycleError> {
    release_work(original_dir, work_dir)?;
    if let Some(patch_zip) = patch_zip {
        patch::apply(patch_zip, work_dir)?;
    }
    let (meta, recovered) = save::load_or_recover(save_zip, work_dir, default_meta)?;
    Ok((meta, recovered))
}

/// 退出回写序列：按 patterns 收集工作副本存档 → 打包到同目录临时文件 →
/// 原子替换 save 档案，返回打包的存档文件数。
///
/// 临时文件在失败时可能残留（部分写入），不做事后清理——下次打包会以
/// `File::create` 截断重建，残留无害且自愈。
///
/// # Errors
/// 打包或替换失败严格上抛，旧 save 保持完整可用。
pub fn writeback(
    work_dir: &Path,
    save_zip: &Path,
    patterns: &[String],
    meta: &SaveMeta,
) -> Result<usize, LifecycleError> {
    let tmp = tmp_sibling(save_zip);
    let packed = save::pack(work_dir, patterns, meta, &tmp)?;
    save::replace_atomically(&tmp, save_zip)?;
    Ok(packed)
}

/// save 档案的同目录临时兄弟路径（原名追加 `.tmp`，保证与目标同卷可原子替换）。
fn tmp_sibling(save_zip: &Path) -> std::path::PathBuf {
    let mut name = save_zip
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    save_zip.with_file_name(name)
}

/// 全量释放工作副本：清空 `work_dir` 后从 `original_dir` 递归复制。
///
/// # Errors
/// 原版目录缺失返回 [`LifecycleError::MissingOriginal`]；清空/复制 IO 失败
/// 返回 [`LifecycleError::Release`]。
pub fn release_work(original_dir: &Path, work_dir: &Path) -> Result<(), LifecycleError> {
    if !original_dir.is_dir() {
        return Err(LifecycleError::MissingOriginal(
            original_dir.display().to_string(),
        ));
    }
    if work_dir.exists() {
        std::fs::remove_dir_all(work_dir)?;
    }
    copy_tree(original_dir, work_dir)?;
    Ok(())
}

/// 递归复制目录树（仅普通文件与子目录；符号链接等特殊形态在 DOS 游戏素材
/// 中不存在，跳过）。
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
