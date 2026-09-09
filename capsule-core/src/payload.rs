//! exe 尾部 payload（zip + trailer）的拼装、解析与 extract-once 释放。
//!
//! 格式定案（z.plan 附录 PL004「数据格式」，PL004.1 打样验证）：`GameName.exe`
//! = launcher 本体 + zip payload + 28 字节 trailer（magic `"CRPAYLD1"` + 格式版本
//! u32 LE + payload_len u64 LE + payload_offset u64 LE）。设计理由：packer（拼装）
//! 与 launcher（运行期识别与释放）共用同一份格式逻辑，只写这一份；PE loader 不读
//! 尾部追加字节，launcher 运行期读自身定位 payload，exe 与 payload 永远只读。
//! payload 布局镜像仓库（`retro/` 前缀），dev 形态的相对路径在打包形态原样成立。

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;
use zip::ZipArchive;

/// trailer magic（CapsuleRetro PAYLoad v1）。
pub const MAGIC: [u8; 8] = *b"CRPAYLD1";
/// payload 格式版本（结构变更时递增；不识别的版本显式报错）。
pub const FORMAT_VERSION: u32 = 1;
/// trailer 总字节数（magic 8 + format 4 + len 8 + offset 8）。
pub const TRAILER_SIZE: u64 = 28;
/// 内嵌 game.json 条目名（镜像仓库布局）。
pub const GAME_JSON_ENTRY: &str = "retro/game.json";
/// 版本标记条目名（释放后即为 extract-once 的标记文件）。
pub const BUILD_ID_ENTRY: &str = "payload.json";

/// payload 读写错误。
#[derive(Debug, Error)]
pub enum PayloadError {
    /// 文件读写失败。
    #[error("payload 文件读写失败 {path}: {source}")]
    Io {
        /// 出错的文件路径。
        path: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// trailer magic 命中但字段非法（格式版本未知 / 偏移长度与文件大小矛盾）。
    #[error("payload trailer 非法 {path}: {reason}")]
    BadTrailer {
        /// 出错的 exe 路径。
        path: PathBuf,
        /// 非法原因。
        reason: String,
    },
    /// payload zip 结构非法（trailer 完好但 zip 数据损坏）。
    #[error("payload zip 结构非法: {0}")]
    Zip(String),
    /// 必需条目缺失（如 payload.json）。
    #[error("payload 缺少必需条目: {0}")]
    MissingEntry(String),
    /// 条目路径越出 payload 根（zip slip / 反斜杠 / 绝对路径等穿越形态）。
    #[error("payload 条目路径非法: {0}")]
    UnsafeEntry(String),
    /// 目标 exe 已含 payload（防重复打包产出双 payload 坏档）。
    #[error("目标已含 payload: {0}")]
    AlreadyPacked(PathBuf),
}

/// payload.json 的类型化形态（extract-once 比对依据，serde 单一来源）。
#[derive(Debug, Deserialize)]
struct BuildMeta {
    /// 构建标识（`<产品版本>+<unix秒>`，由 packer 生成）。
    build: String,
}

/// 把 payload zip 追加到 exe 尾部并写 trailer（就地修改，调用方先复制好目标）。
///
/// # 参数
/// - `exe_path`：目标 exe（应为 launcher 本体的副本，不得已含 payload）
/// - `payload_zip`：资源 zip（布局见模块文档）
///
/// # Errors
/// 目标已含 payload 返回 [`PayloadError::AlreadyPacked`]；文件读写失败返回
/// [`PayloadError::Io`]。
pub fn append_payload(exe_path: &Path, payload_zip: &Path) -> Result<(), PayloadError> {
    if PayloadReader::open(exe_path)?.is_some() {
        return Err(PayloadError::AlreadyPacked(exe_path.to_path_buf()));
    }
    let offset = std::fs::metadata(exe_path)
        .map_err(|source| PayloadError::Io {
            path: exe_path.to_path_buf(),
            source,
        })?
        .len();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(exe_path)
        .map_err(|source| PayloadError::Io {
            path: exe_path.to_path_buf(),
            source,
        })?;
    // 流式追加（FIX002.11）：payload 可达数十 MB，整段读入内存与读侧
    // "区间游标不整段载内存"的设计不对称；len 取实际复制字节数
    let mut payload = std::fs::File::open(payload_zip).map_err(|source| PayloadError::Io {
        path: payload_zip.to_path_buf(),
        source,
    })?;
    let copied = std::io::copy(&mut payload, &mut file).map_err(|source| PayloadError::Io {
        path: payload_zip.to_path_buf(),
        source,
    })?;
    let mut trailer = Vec::with_capacity(TRAILER_SIZE as usize);
    trailer.extend_from_slice(&MAGIC);
    trailer.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    trailer.extend_from_slice(&copied.to_le_bytes());
    trailer.extend_from_slice(&offset.to_le_bytes());
    file.write_all(&trailer)
        .map_err(|source| PayloadError::Io {
            path: exe_path.to_path_buf(),
            source,
        })?;
    Ok(())
}

/// payload 区间只读游标：把 exe 文件的 `[base, base+len)` 区间呈现为独立的
/// Read + Seek 流（不整段载入内存），供 [`ZipArchive`] 消费。
struct PayloadSource {
    file: std::fs::File,
    base: u64,
    len: u64,
    pos: u64,
}

impl Read for PayloadSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = (self.len - self.pos) as usize;
        let want = buf.len().min(remaining);
        if want == 0 {
            return Ok(0);
        }
        self.file.seek(SeekFrom::Start(self.base + self.pos))?;
        let read = self.file.read(&mut buf[..want])?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for PayloadSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next: i128 = match pos {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
        };
        self.pos = next.clamp(0, self.len as i128) as u64;
        Ok(self.pos)
    }
}

/// 已打包 exe 的 payload 只读句柄。
pub struct PayloadReader {
    archive: ZipArchive<PayloadSource>,
    build_id: String,
}

impl std::fmt::Debug for PayloadReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PayloadReader")
            .field("build_id", &self.build_id)
            .finish()
    }
}

impl PayloadReader {
    /// 读取 exe 尾部 trailer 并打开 payload。
    ///
    /// # 返回
    /// `Ok(None)` = 无 trailer（非打包形态，调用方走 dev 分支）。
    ///
    /// # Errors
    /// magic 命中但字段非法返回 [`PayloadError::BadTrailer`]；zip 结构损坏或
    /// 缺 `payload.json` 返回对应错误。
    pub fn open(exe_path: &Path) -> Result<Option<Self>, PayloadError> {
        let mut file = std::fs::File::open(exe_path).map_err(|source| PayloadError::Io {
            path: exe_path.to_path_buf(),
            source,
        })?;
        let size = file
            .metadata()
            .map_err(|source| PayloadError::Io {
                path: exe_path.to_path_buf(),
                source,
            })?
            .len();
        if size < TRAILER_SIZE {
            return Ok(None);
        }
        file.seek(SeekFrom::End(-(TRAILER_SIZE as i64)))
            .map_err(|source| PayloadError::Io {
                path: exe_path.to_path_buf(),
                source,
            })?;
        let mut trailer = [0u8; 28];
        file.read_exact(&mut trailer)
            .map_err(|source| PayloadError::Io {
                path: exe_path.to_path_buf(),
                source,
            })?;
        if trailer[0..8] != MAGIC {
            return Ok(None);
        }
        let format = u32::from_le_bytes(trailer[8..12].try_into().expect("切片长度恒为 4"));
        let payload_len = u64::from_le_bytes(trailer[12..20].try_into().expect("切片长度恒为 8"));
        let payload_offset =
            u64::from_le_bytes(trailer[20..28].try_into().expect("切片长度恒为 8"));
        let io = |source| PayloadError::Io {
            path: exe_path.to_path_buf(),
            source,
        };
        if format != FORMAT_VERSION {
            return Err(PayloadError::BadTrailer {
                path: exe_path.to_path_buf(),
                reason: format!("未知格式版本 {format}（本程序支持 {FORMAT_VERSION}）"),
            });
        }
        let tail = payload_offset
            .checked_add(payload_len)
            .and_then(|sum| sum.checked_add(TRAILER_SIZE));
        if tail != Some(size) {
            return Err(PayloadError::BadTrailer {
                path: exe_path.to_path_buf(),
                reason: format!("offset+len+trailer（{tail:?}）与文件大小（{size}）矛盾"),
            });
        }
        file.seek(SeekFrom::Start(payload_offset)).map_err(io)?;
        let mut magic2 = [0u8; 2];
        file.read_exact(&mut magic2).map_err(io)?;
        if magic2 != *b"PK" {
            return Err(PayloadError::Zip("payload 区间非 zip 数据".to_string()));
        }
        let source = PayloadSource {
            file,
            base: payload_offset,
            len: payload_len,
            pos: 0,
        };
        let mut archive =
            ZipArchive::new(source).map_err(|e| PayloadError::Zip(format!("zip 打开失败: {e}")))?;
        let build_id = {
            let mut entry = archive
                .by_name(BUILD_ID_ENTRY)
                .map_err(|e| PayloadError::MissingEntry(format!("{BUILD_ID_ENTRY}: {e}")))?;
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| PayloadError::Zip(format!("{BUILD_ID_ENTRY} 读取失败: {e}")))?;
            serde_json::from_str::<BuildMeta>(&text)
                .map_err(|e| PayloadError::Zip(format!("{BUILD_ID_ENTRY} 解析失败: {e}")))?
                .build
        };
        Ok(Some(Self { archive, build_id }))
    }

    /// 构建标识（open 时读自 `payload.json`）。
    pub fn build_id(&self) -> &str {
        &self.build_id
    }

    /// 全部条目名（升序）。
    pub fn entry_names(&mut self) -> Vec<String> {
        let mut names: Vec<String> = (0..self.archive.len())
            .filter_map(|i| self.archive.by_index(i).ok().map(|e| e.name().to_string()))
            .collect();
        names.sort();
        names
    }

    /// 读取单个文本条目。
    ///
    /// # Errors
    /// 条目不存在返回 [`PayloadError::MissingEntry`]；读取失败返回
    /// [`PayloadError::Zip`]。
    pub fn read_entry_to_string(&mut self, name: &str) -> Result<String, PayloadError> {
        let mut entry = self
            .archive
            .by_name(name)
            .map_err(|e| PayloadError::MissingEntry(format!("{name}: {e}")))?;
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|e| PayloadError::Zip(format!("条目 {name} 读取失败: {e}")))?;
        Ok(text)
    }

    /// 全量释放 payload 到 `dest`（目录结构按条目名重建，已存在文件覆盖）。
    ///
    /// # Errors
    /// 条目路径越界返回 [`PayloadError::UnsafeEntry`]；读写失败返回
    /// [`PayloadError::Io`] / [`PayloadError::Zip`]。
    pub fn extract_all(&mut self, dest: &Path) -> Result<(), PayloadError> {
        for index in 0..self.archive.len() {
            let mut entry = self
                .archive
                .by_index(index)
                .map_err(|e| PayloadError::Zip(format!("条目 {index} 读取失败: {e}")))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let unix_mode = entry.unix_mode();
            let rel = validate_entry(&name)?;
            let target = dest.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|source| PayloadError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| PayloadError::Zip(format!("条目 {name} 读取失败: {e}")))?;
            std::fs::write(&target, bytes).map_err(|source| PayloadError::Io {
                path: target.clone(),
                source,
            })?;
            restore_unix_mode(unix_mode, &target);
        }
        Ok(())
    }

    /// extract-once：`dest/payload.json` 标记与本次构建标识相符**且全部条目
    /// 在盘**则跳过（返回 `false`）；标记缺失 / 损坏 / 不符，或条目缺失
    /// （部分丢失自愈，FIX002.9）则清空 `dest` 全量重释放（返回 `true`）。
    ///
    /// # Errors
    /// 释放过程错误同 [`Self::extract_all`]；旧目录删除失败返回
    /// [`PayloadError::Io`]。
    pub fn extract_once(&mut self, dest: &Path) -> Result<bool, PayloadError> {
        let marker = dest.join(BUILD_ID_ENTRY);
        if self.marker_fresh(&marker) && self.all_entries_present(dest) {
            return Ok(false);
        }
        if dest.exists() {
            std::fs::remove_dir_all(dest).map_err(|source| PayloadError::Io {
                path: dest.to_path_buf(),
                source,
            })?;
        }
        std::fs::create_dir_all(dest).map_err(|source| PayloadError::Io {
            path: dest.to_path_buf(),
            source,
        })?;
        self.extract_all(dest)?;
        Ok(true)
    }

    /// 标记新鲜：payload.json 可读、可解析且构建标识相符。读取/解析失败
    /// 按"标记无效"处理走全量重释放（容错已登记 AGENTS.md 白名单，FIX002.4）。
    fn marker_fresh(&self, marker: &Path) -> bool {
        let Ok(text) = std::fs::read_to_string(marker) else {
            return false;
        };
        match serde_json::from_str::<BuildMeta>(&text) {
            Ok(meta) => meta.build == self.build_id,
            Err(_) => false,
        }
    }

    /// 条目落盘抽查（FIX002.9）：逐条目核对释放产物存在。标记文件幸存而
    /// 部分资源被清理工具/用户删除时，仅凭标记会永久跳过导致资源缺失
    /// （运行期完整性是硬依赖，PL004 shader ABORT 实证），全部命中才跳过。
    fn all_entries_present(&self, dest: &Path) -> bool {
        self.archive
            .file_names()
            .all(|name| dest.join(name).exists())
    }
}

/// 恢复条目 unix 权限位（FIX002.10）：payload 携带的 unix_mode 原样落地，
/// 0.2.0 三端适配时 dosbox 二进制的执行位依赖此处（Windows 无权限位语义，
/// cfg 隔离为 no-op；实机验证挂 y.problems #19）。
#[cfg(unix)]
fn restore_unix_mode(mode: Option<u32>, target: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Some(mode) = mode {
        if let Err(err) = std::fs::set_permissions(target, std::fs::Permissions::from_mode(mode)) {
            eprintln!("警告: 恢复条目权限失败 {}: {err}", target.display());
        }
    }
}

/// Windows 平台无 unix 权限语义，no-op。
#[cfg(not(unix))]
fn restore_unix_mode(_mode: Option<u32>, _target: &Path) {}

/// 校验条目相对路径：非空、不含反斜杠、组件全部为普通名（拒绝 `..` / 根 /
/// 盘符等穿越形态）。与 save.rs 的 save 条目校验同语义（两种格式各自收口）。
fn validate_entry(name: &str) -> Result<PathBuf, PayloadError> {
    use std::path::Component;
    if name.is_empty() || name.contains('\\') {
        return Err(PayloadError::UnsafeEntry(name.to_string()));
    }
    let path = Path::new(name);
    if !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(PayloadError::UnsafeEntry(name.to_string()));
    }
    Ok(path.to_path_buf())
}
