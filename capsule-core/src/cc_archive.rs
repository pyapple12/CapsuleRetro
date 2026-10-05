//! cc_archive：XEEN CC 资源容器解析（PL008，xeen-extractor 的解析核心）。
//!
//! 格式（Xeen Wiki 官方文档 + OpenXeen 实现交叉验证，双锚点实测成立）：
//! 头 2 字节 = 资源个数（uint16 LE，明文）；随后为每项 8 字节的目录表（存储
//! 态逐字节混淆，还原 = 循环左移 2 位后加滚动计数；计数初值 0xAC，每字节后
//! +0x67）；每项 = 资源 ID（uint16 LE，= 文件名哈希）+ 偏移（uint24 LE）+
//! 长度（uint16 LE）+ 1 字节填充；资源数据逐字节 XOR 0x35 还原。文件名哈希：
//! 16 位循环右移 7 位后加字符值（资源名惯用大写）；独立锚点 DARK.PAL =
//! 0x48AA（Xeen Wiki 官方示例）。
//! 设计理由：解析核心与 CLI 壳（bin/xeen-extractor）分离，便于单测与复用；
//! 放 capsule-core 使其与 xmonitor 同享依赖与门禁。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// 内嵌资源名对照表（数据事实；来源 OpenXeen 项目，覆盖本仓三容器有名 ID）。
const NAMES_CSV: &str = include_str!("cc_archive_names.csv");

/// CC 容器解析或读取错误。
#[derive(Debug, Error)]
pub enum CcError {
    /// 容器文件读取失败。
    #[error("CC 容器读取失败 {path}: {source}")]
    Io {
        /// 失败的容器路径。
        path: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// 文件过小或头部非法（不像 CC 容器）。
    #[error("CC 容器头部非法或文件过小: {path}")]
    Malformed {
        /// 非法的容器路径。
        path: PathBuf,
    },
    /// 资源声明区间越出容器（目录损坏或非本系列文件）。
    #[error("资源 {id:#06x} 区间越界: {offset:#x}+{len} 超出容器 {size} 字节")]
    OutOfRange {
        /// 越界的资源 ID。
        id: u16,
        /// 声明偏移。
        offset: u32,
        /// 声明长度。
        len: usize,
        /// 容器实际大小。
        size: usize,
    },
}

/// 目录表中的一条资源。
#[derive(Debug, Clone)]
pub struct CcEntry {
    /// 资源 ID（文件名哈希）。
    pub id: u16,
    /// 还原出的文件名（对照表未命中时为 None）。
    pub name: Option<String>,
    /// 容器内绝对偏移。
    pub offset: u32,
    /// 字节长度。
    pub len: u16,
}

/// 已解析的 CC 容器（持有原始字节，供按条目读取）。
#[derive(Debug)]
pub struct CcArchive {
    data: Vec<u8>,
    /// 全部资源条目（目录顺序）。
    pub entries: Vec<CcEntry>,
}

impl CcArchive {
    /// 从文件加载并解析目录表，套用内置名字对照表。
    ///
    /// # Errors
    /// 文件读取失败或头部非法时返回 [`CcError`]。
    pub fn load(path: &Path) -> Result<Self, CcError> {
        let data = std::fs::read(path).map_err(|source| CcError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(data, &builtin_names(), path)
    }

    /// 解析容器字节（names：ID → 文件名，可为空表）。
    ///
    /// # Errors
    /// 字节过小或目录表不完整时返回 [`CcError::Malformed`]。
    pub fn parse(
        data: Vec<u8>,
        names: &HashMap<u16, String>,
        path: &Path,
    ) -> Result<Self, CcError> {
        if data.len() < 2 {
            return Err(CcError::Malformed {
                path: path.to_path_buf(),
            });
        }
        let count = u16::from_le_bytes([data[0], data[1]]) as usize;
        let toc_len = count * 8;
        if data.len() < 2 + toc_len {
            return Err(CcError::Malformed {
                path: path.to_path_buf(),
            });
        }
        let toc = decrypt_toc(&data[2..2 + toc_len]);
        let mut entries = Vec::with_capacity(count);
        for chunk in toc.as_chunks::<8>().0 {
            let id = u16::from_le_bytes([chunk[0], chunk[1]]);
            let offset =
                u32::from(chunk[2]) | (u32::from(chunk[3]) << 8) | (u32::from(chunk[4]) << 16);
            let len = u16::from_le_bytes([chunk[5], chunk[6]]);
            entries.push(CcEntry {
                id,
                name: names.get(&id).cloned(),
                offset,
                len,
            });
        }
        Ok(Self { data, entries })
    }

    /// 按文件名查条目（名字经哈希对目录 ID）。
    pub fn find(&self, name: &str) -> Option<&CcEntry> {
        let id = hash_name(name)?;
        self.entries.iter().find(|e| e.id == id)
    }

    /// 读取并还原资源字节（XOR 0x35）；越界严格报错。
    ///
    /// # Errors
    /// 条目声明区间越出容器时返回 [`CcError`]。
    pub fn read(&self, entry: &CcEntry) -> Result<Vec<u8>, CcError> {
        Ok(self.slice(entry)?.iter().map(|b| b ^ 0x35).collect())
    }

    /// 读取资源原始字节（不做 XOR；CUR 容器数据区为明文——PL010）。
    ///
    /// # Errors
    /// 条目声明区间越出容器时返回 [`CcError`]。
    pub fn read_raw(&self, entry: &CcEntry) -> Result<Vec<u8>, CcError> {
        Ok(self.slice(entry)?.to_vec())
    }

    /// 条目声明区间的切片（越界严格报错）。
    fn slice(&self, entry: &CcEntry) -> Result<&[u8], CcError> {
        let start = entry.offset as usize;
        let end = start + entry.len as usize;
        if end > self.data.len() {
            return Err(CcError::OutOfRange {
                id: entry.id,
                offset: entry.offset,
                len: entry.len as usize,
                size: self.data.len(),
            });
        }
        Ok(&self.data[start..end])
    }

    /// 容器原始大小（字节）。
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// 文件名 → 16 位哈希（与上游一致的"循环右移 7 位后加字符值"；空名 None）。
pub fn hash_name(name: &str) -> Option<u16> {
    if name.is_empty() {
        return None;
    }
    let mut h: u16 = 0;
    for ch in name.chars() {
        h = h.rotate_right(7);
        // 资源名约定为 ASCII 大写；高位字符按低 16 位截断（与上游实现一致）
        h = h.wrapping_add(ch as u16);
    }
    Some(h)
}

/// 解析资源名对照表文本（`#` 注释行 + "0xID,名字"）；格式非法的非注释行
/// 打印警告后跳过——坏行不阻断提取，但必须显式记录而非静默吞掉（A003 #5）。
fn parse_names_table(text: &str) -> HashMap<u16, String> {
    let mut names = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((id, name)) = line.split_once(',') else {
            eprintln!(
                "警告: cc_archive_names.csv 第 {} 行缺逗号，已跳过: {line}",
                n + 1
            );
            continue;
        };
        let Ok(id) = u16::from_str_radix(id.trim().trim_start_matches("0x"), 16) else {
            eprintln!(
                "警告: cc_archive_names.csv 第 {} 行 ID 非十六进制，已跳过: {line}",
                n + 1
            );
            continue;
        };
        names.insert(id, name.trim().to_string());
    }
    names
}

/// 内置资源名对照表（cc_archive_names.csv：`#` 注释行 + "0xID,名字"）。
pub fn builtin_names() -> HashMap<u16, String> {
    parse_names_table(NAMES_CSV)
}

fn decrypt_toc(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut cnt: u8 = 0xAC;
    for &b in raw {
        out.push(b.rotate_left(2).wrapping_add(cnt));
        cnt = cnt.wrapping_add(0x67);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PL008.1 独立锚点（Xeen Wiki 官方示例，与实现无关的第三方事实）：
    /// DARK.PAL 的文件名哈希必须是 0x48AA。
    #[test]
    fn 文件名哈希独立锚点() {
        assert_eq!(hash_name("DARK.PAL"), Some(0x48AA));
        assert_eq!(hash_name(""), None);
    }

    /// 构造合成小容器（加密 = 解密的逆变换，仅用于夹具；布局依据
    /// Xeen Wiki 格式文档，非实现同源推导）。
    fn synthetic_cc() -> Vec<u8> {
        let raw = [0x11u8, 0x22, 0x33, 0x44];
        // 条目 ID = 文件名哈希（与上游容器同构），find 按名查找才可命中
        let id = hash_name("TEST.RES").expect("非空名应有哈希");
        // 目录表仅含条目本身（8 字节）；资源个数在容器头（明文），不入目录
        let mut plain = Vec::new();
        plain.extend_from_slice(&id.to_le_bytes());
        plain.extend_from_slice(&[10, 0x00, 0x00]);
        plain.extend_from_slice(&4u16.to_le_bytes());
        plain.push(0);
        let mut blob = 1u16.to_le_bytes().to_vec();
        let mut cnt: u8 = 0xAC;
        for &b in &plain {
            blob.push(b.wrapping_sub(cnt).rotate_right(2));
            cnt = cnt.wrapping_add(0x67);
        }
        blob.extend(raw.iter().map(|b| b ^ 0x35));
        blob
    }

    /// PL008.3 验证：合成容器全链路——目录解密、条目解析、名字命中、
    /// 按名查找、数据还原。
    #[test]
    fn 合成容器解析与读回() {
        let blob = synthetic_cc();
        let mut names = HashMap::new();
        let id = hash_name("TEST.RES").expect("非空名应有哈希");
        names.insert(id, "TEST.RES".to_string());
        let arch = CcArchive::parse(blob.clone(), &names, Path::new("syn.cc")).expect("应可解析");
        assert_eq!(arch.entries.len(), 1);
        let e = &arch.entries[0];
        assert_eq!(e.id, id);
        assert_eq!(e.offset, 10);
        assert_eq!(e.len, 4);
        assert_eq!(e.name.as_deref(), Some("TEST.RES"));
        assert!(arch.find("TEST.RES").is_some(), "按名查找应命中");
        assert_eq!(
            arch.read(e).expect("应可读取"),
            vec![0x11, 0x22, 0x33, 0x44]
        );
    }

    /// PL008.3 验证：内置名字表命中独立锚点 DARK.PAL = 0x48AA。
    #[test]
    fn 内置名字表锚点() {
        let names = builtin_names();
        assert_eq!(names.get(&0x48AA).map(String::as_str), Some("DARK.PAL"));
    }

    /// PL008.3 验证：头非法（过小）严格报错不静默。
    #[test]
    fn 头部非法报错() {
        let err = CcArchive::parse(vec![0u8], &HashMap::new(), Path::new("bad.cc"))
            .expect_err("过小容器应报错");
        assert!(matches!(err, CcError::Malformed { .. }), "{err}");
    }

    /// A003 #5（FIX003.5）验证：坏行（缺逗号 / ID 非 hex）被跳过且不吞好行
    /// ——解析照常完成，坏行有 eprintln 警告（行为锚）。
    #[test]
    fn 名字表坏行跳过不吞好行() {
        let table = parse_names_table(
            "# 注释\n0x0010,GOOD.ICN\n缺逗号行\n0xZZZZ,BAD.ID\n\n0x0020,ALSO.GOOD\n",
        );
        assert_eq!(table.get(&0x0010).map(String::as_str), Some("GOOD.ICN"));
        assert_eq!(table.get(&0x0020).map(String::as_str), Some("ALSO.GOOD"));
        assert_eq!(table.len(), 2, "坏行不应产出条目");
    }
}
