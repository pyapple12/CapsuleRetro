//! save 档案（`GameName.save`，zip）的打包、解包与原子替换。
//!
//! 结构定案（z.plan 附录 PL003）：zip 内 `meta.json`（[`SaveMeta`] serde
//! 单一来源）+ `save/`（按 game.json `paths.save_files` 清单从工作副本收集
//! 的存档文件）。设计理由：save 是产品唯一用户数据，打包/解包/替换逻辑
//! 只写这一份供 launcher 与未来 packer 共用；损坏处理走 AGENTS.md 容错
//! 白名单登记的降级路径（备份 .corrupt + 全新空档继续），其余一律严格报错。

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// save zip 内 meta 条目名。
const META_ENTRY: &str = "meta.json";
/// save zip 内存档文件的前缀目录。
const SAVE_PREFIX: &str = "save/";
/// save 格式版本单一来源（结构变更时递增；写侧恒用当前版本，调用方不再
/// 各自硬编码字面量——FIX002.13）。
pub const SAVE_META_VERSION: &str = "1";

/// save 档案读写错误。
#[derive(Debug, Error)]
pub enum SaveError {
    /// 文件读写失败。
    #[error("save 档案读写失败 {path}: {source}")]
    Io {
        /// 出错的文件路径。
        path: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// 档案损坏（非 zip 字节 / 缺 meta / meta 非法）——调用方可据此走降级路径。
    #[error("save 档案损坏: {0}")]
    Corrupted(String),
    /// 条目路径越出 save 结构约定（save/ 前缀之外的条目或路径穿越）。
    #[error("save 条目路径非法: {0}")]
    UnsafeEntry(String),
    /// 存档清单通配符为空或非法。
    #[error("存档清单通配符非法: {0}")]
    InvalidPattern(String),
    /// zip 写入或序列化失败。
    #[error("zip 操作失败: {0}")]
    Zip(String),
}

/// save 元数据（meta.json 的类型化形态，serde 单一来源）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMeta {
    /// save 格式版本（结构变更时递增）。
    pub version: String,
    /// 当前 hack 补丁 id（None = 原版），对应 game.json patches[].id。
    pub hack: Option<String>,
    /// 用户设置（字段位预留，本期恒为空对象）。
    pub settings: serde_json::Value,
}

impl SaveMeta {
    /// 构造空设置的 meta（settings 字段位预留，调用方无需接触 serde_json）。
    pub fn new(version: impl Into<String>, hack: Option<String>) -> Self {
        Self {
            version: version.into(),
            hack,
            settings: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

/// 通配符匹配：仅支持 `*`（匹配任意字符序列，含空），大小写不敏感；
/// 无 `*` 时为精确相等（不做前后缀宽松匹配）。
pub fn match_pattern(name: &str, pattern: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let pat = pattern.to_ascii_lowercase();
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return name == pat;
    }
    // 首段必须前缀命中，尾段必须后缀命中，中段按序出现
    let Some(rest) = name.strip_prefix(parts[0]) else {
        return false;
    };
    let last = parts[parts.len() - 1];
    let mut middle = rest;
    if !last.is_empty() {
        let Some(stripped) = middle.strip_suffix(last) else {
            return false;
        };
        middle = stripped;
    }
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match middle.find(part) {
            Some(pos) => middle = &middle[pos + part.len()..],
            None => return false,
        }
    }
    true
}

/// 解包 save 档案：`save/` 下条目写入工作副本（覆盖），返回解析出的 meta。
///
/// # Errors
/// 文件打开失败返回 [`SaveError::Io`]；zip 结构非法、缺 `meta.json` 或 meta
/// 解析失败返回 [`SaveError::Corrupted`]（调用方可据此降级）；条目越出
/// `save/` 结构约定返回 [`SaveError::UnsafeEntry`]。
pub fn unpack(archive_path: &Path, work_dir: &Path) -> Result<SaveMeta, SaveError> {
    let file = std::fs::File::open(archive_path).map_err(|source| SaveError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| SaveError::Corrupted(format!("zip 结构非法: {e}")))?;

    let meta = read_meta_from_archive(&mut archive)?;

    let mut written: Vec<PathBuf> = Vec::new();
    // FIX005.7：save 外普通文件被白名单跳过时聚合计数收尾告警——损坏降级有
    // ERROR 留痕，此处此前零留痕，"存档没回来"类问题无诊断线索
    let mut skipped: Vec<String> = Vec::new();
    let result = (|| {
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| SaveError::Corrupted(format!("条目 {index} 读取失败: {e}")))?;
            // 显式目录条目仅承载结构（资源管理器/7-Zip 重打包必产生），文件
            // 写入时 create_dir_all 已按需建目录——跳过而非报错（FIX002.3），
            // 结构性条目不进告警防噪声
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            if name == META_ENTRY {
                continue;
            }
            // save/ 前缀之外的普通文件按良性噪声忽略（预留 config/ 节、
            // __MACOSX 噪声；AGENTS.md 容错白名单登记）——穿越形态仍由
            // validate_rel_path 严格拒绝；跳过聚合计数收尾告警（FIX005.7）
            let Some(rel) = name.strip_prefix(SAVE_PREFIX).filter(|r| !r.is_empty()) else {
                skipped.push(name);
                continue;
            };
            // save/ 下嵌套子路径（如 save/sub/X.SAV）超出 save_files 只认
            // 顶层文件名的契约：解包收得进、退出回写收不回，往返必丢——
            // 严格拒绝（A003 #1），坏档在启动瞬间显式暴露
            let target_rel = crate::validate_rel_path(rel)
                .filter(|p| p.components().count() == 1)
                .ok_or_else(|| SaveError::UnsafeEntry(name.clone()))?;
            let target = work_dir.join(target_rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|source| SaveError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| SaveError::Corrupted(format!("条目 {name} 读取失败: {e}")))?;
            std::fs::write(&target, bytes).map_err(|source| SaveError::Io {
                path: target.clone(),
                source,
            })?;
            written.push(target);
        }
        Ok(())
    })();
    if result.is_err() {
        // 中途失败清理已写入文件（FIX002.12）：不留下"部分解包 + 原版释放"
        // 的混合态；删除失败不掩盖主错误，残留由下次启动全量重建自愈
        for path in &written {
            if let Err(err) = std::fs::remove_file(path) {
                eprintln!("警告: 解包残留清理失败 {}: {err}", path.display());
            }
        }
    }
    result?;
    if !skipped.is_empty() {
        eprintln!(
            "警告: 忽略 save 外条目 {} 个: {}",
            skipped.len(),
            skipped
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("、")
        );
    }
    Ok(meta)
}

/// 收集工作副本中命中任一 pattern 的顶层文件，连同 meta 打包为 save zip 写到
/// `out_path`，返回打包的存档文件数；`None` = 零命中（patterns 无一匹配——
/// 多为通配符拼写笔误，调用方应区分决策而非当正常 0 消费，A004 #1）。
///
/// # Errors
/// 清单为空或含空白条目返回 [`SaveError::InvalidPattern`]；工作副本不可读或
/// zip 写入失败返回对应错误。
pub fn pack(
    work_dir: &Path,
    patterns: &[String],
    meta: &SaveMeta,
    out_path: &Path,
) -> Result<Option<usize>, SaveError> {
    if patterns.is_empty() {
        return Err(SaveError::InvalidPattern("存档清单为空".to_string()));
    }
    if patterns.iter().any(|p| p.trim().is_empty()) {
        return Err(SaveError::InvalidPattern(format!(
            "{patterns:?} 含空白条目"
        )));
    }
    // 目录项迭代/file_type 错误一律严格上抛（FIX005.6，与模块"其余错误严格
    // 上抛"口径一致；原 A002 条件豁免的静默跳过随本修复闭环）——静默落选会
    // 让单个存档无声缺席且回写后不可恢复
    let names: Vec<String> = std::fs::read_dir(work_dir)
        .map_err(|source| SaveError::Io {
            path: work_dir.to_path_buf(),
            source,
        })?
        .map(|entry| {
            let entry = entry.map_err(|source| SaveError::Io {
                path: work_dir.to_path_buf(),
                source,
            })?;
            let is_file =
                entry
                    .file_type()
                    .map(|t| t.is_file())
                    .map_err(|source| SaveError::Io {
                        path: entry.path(),
                        source,
                    })?;
            Ok(is_file.then(|| entry.file_name().to_string_lossy().into_owned()))
        })
        .collect::<Result<Vec<_>, SaveError>>()?
        .into_iter()
        .flatten()
        .filter(|name| patterns.iter().any(|p| match_pattern(name, p)))
        .collect();

    let meta_text =
        serde_json::to_string_pretty(meta).map_err(|e| SaveError::Zip(e.to_string()))?;
    let file = std::fs::File::create(out_path).map_err(|source| SaveError::Io {
        path: out_path.to_path_buf(),
        source,
    })?;
    let mut writer = ZipWriter::new(file);
    writer
        .start_file(META_ENTRY.to_string(), SimpleFileOptions::default())
        .map_err(|e| SaveError::Zip(e.to_string()))?;
    std::io::Write::write_all(&mut writer, meta_text.as_bytes())
        .map_err(|e| SaveError::Zip(e.to_string()))?;
    for name in &names {
        let bytes = std::fs::read(work_dir.join(name)).map_err(|source| SaveError::Io {
            path: work_dir.join(name),
            source,
        })?;
        writer
            .start_file(format!("{SAVE_PREFIX}{name}"), SimpleFileOptions::default())
            .map_err(|e| SaveError::Zip(e.to_string()))?;
        std::io::Write::write_all(&mut writer, &bytes)
            .map_err(|e| SaveError::Zip(e.to_string()))?;
    }
    writer.finish().map_err(|e| SaveError::Zip(e.to_string()))?;
    let count = names.len();
    Ok((count > 0).then_some(count))
}

/// 只读解析 save 档案的 meta（不解包、不落盘）——UI 待机页状态展示用。
///
/// # 返回
/// `Ok(None)` = 档案不存在（尚无用户数据）。
///
/// # Errors
/// 文件打开失败返回 [`SaveError::Io`]；zip 结构非法 / 缺 meta / meta 非法
/// 返回 [`SaveError::Corrupted`]（UI 可据此提示"损坏，下次启动降级"）。
pub fn read_meta(archive_path: &Path) -> Result<Option<SaveMeta>, SaveError> {
    if !archive_path.exists() {
        return Ok(None);
    }
    let file = std::fs::File::open(archive_path).map_err(|source| SaveError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| SaveError::Corrupted(format!("zip 结构非法: {e}")))?;
    read_meta_from_archive(&mut archive).map(Some)
}

/// 统计 save 档案内的存档数据文件数（`save/` 前缀条目，不含 meta）——UI
/// 待机页状态展示用。
///
/// # 返回
/// `Ok(None)` = 档案不存在。
///
/// # Errors
/// zip 结构非法时返回 [`SaveError::Corrupted`]（与 [`read_meta`] 同口径：
/// UI 提示损坏，下次启动将降级）。
pub fn save_entry_count(archive_path: &Path) -> Result<Option<usize>, SaveError> {
    if !archive_path.exists() {
        return Ok(None);
    }
    let file = std::fs::File::open(archive_path).map_err(|source| SaveError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let archive =
        ZipArchive::new(file).map_err(|e| SaveError::Corrupted(format!("zip 结构非法: {e}")))?;
    let count = archive
        .file_names()
        .filter(|name| name.starts_with(SAVE_PREFIX) && name.len() > SAVE_PREFIX.len())
        .count();
    Ok(Some(count))
}

/// 原子替换：把临时档 rename 覆盖目标（同目录同卷，Windows 下 rename 覆盖
/// 已存在文件）。失败时目标保持旧内容，可自愈。
///
/// # Errors
/// rename 失败返回 [`SaveError::Io`]。
pub fn replace_atomically(tmp_path: &Path, target: &Path) -> Result<(), SaveError> {
    std::fs::rename(tmp_path, target).map_err(|source| SaveError::Io {
        path: target.to_path_buf(),
        source,
    })
}

/// 加载 save 档案并解包到工作副本（生命周期入口，含白名单降级）。
///
/// 三态：档案不存在 → 返回 `(default_meta, false)`，不落盘；档案完好 →
/// 解包返回 `(meta, false)`；档案损坏 → 白名单降级（备份为 `<原名>.corrupt`、
/// 仅保留最新一份，返回 `(default_meta, true)` 继续运行）。
///
/// # 参数
/// - `archive_path`：save 档案路径
/// - `work_dir`：解包目标工作副本
/// - `default_meta`：缺失/降级时使用的元数据（hack 选择由调用方决定）
///
/// # Errors
/// [`SaveError::Corrupted`] 以外的错误（IO / 越界条目）严格上抛，不走降级。
pub fn load_or_recover(
    archive_path: &Path,
    work_dir: &Path,
    default_meta: SaveMeta,
) -> Result<(SaveMeta, bool), SaveError> {
    if !archive_path.exists() {
        return Ok((default_meta, false));
    }
    match unpack(archive_path, work_dir) {
        Ok(meta) => Ok((meta, false)),
        Err(SaveError::Corrupted(_)) => {
            let backup = corrupt_backup_path(archive_path)?;
            if backup.exists() {
                std::fs::remove_file(&backup).map_err(|source| SaveError::Io {
                    path: backup.clone(),
                    source,
                })?;
            }
            std::fs::rename(archive_path, &backup).map_err(|source| SaveError::Io {
                path: archive_path.to_path_buf(),
                source,
            })?;
            Ok((default_meta, true))
        }
        Err(err) => Err(err),
    }
}

/// 损坏备份路径：原名追加 `.corrupt` 后缀。
///
/// # Errors
/// 路径缺少文件名返回 [`SaveError::Io`]（InvalidInput）——输入形状非法并非
/// 档案损坏，不借用 [`SaveError::Corrupted`] 的降级语义（A003 #9）。
fn corrupt_backup_path(archive_path: &Path) -> Result<PathBuf, SaveError> {
    let name = archive_path.file_name().ok_or_else(|| SaveError::Io {
        path: archive_path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "档案路径缺少文件名"),
    })?;
    let mut os = name.to_os_string();
    os.push(".corrupt");
    Ok(archive_path.with_file_name(os))
}

/// 从已打开的 save zip 读取并解析 meta.json（unpack 与 read_meta 共用单份，
/// 错误文案随之统一——A003 #7）。
///
/// # Errors
/// 缺 meta、读取失败或 JSON 解析失败返回 [`SaveError::Corrupted`]（调用方
/// 可据此走降级）。
fn read_meta_from_archive(archive: &mut ZipArchive<std::fs::File>) -> Result<SaveMeta, SaveError> {
    let mut entry = archive
        .by_name(META_ENTRY)
        .map_err(|e| SaveError::Corrupted(format!("缺 {META_ENTRY}: {e}")))?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|e| SaveError::Corrupted(format!("{META_ENTRY} 读取失败: {e}")))?;
    serde_json::from_str(&text).map_err(|e| SaveError::Corrupted(format!("meta 解析失败: {e}")))
}
