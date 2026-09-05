//! profile：memory_map 档案解析。
//!
//! 定义与 `retro/memory_map.json` 一一对应的数据结构（[`MemoryMap`] / [`Entry`] /
//! [`ValueType`]）与解码逻辑。设计理由：档案是"游戏内存偏移"的单一来源，由打包
//! 器随包分发、A/B 两壳共用；解析错误必须显式枚举（严格抛错策略），不允许散装
//! HashMap 取值。

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

/// 档案解析与解码过程中的全部错误类型。
#[derive(Debug, Error)]
pub enum ProfileError {
    /// 档案 JSON 非法或字段缺失/类型不符。
    #[error("档案 JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    /// 档案文件读取失败（不存在/无权限等）。
    #[error("档案文件读取失败: {0}")]
    Io(#[from] std::io::Error),
    /// entries 中出现了未知的 value_type 字符串。
    #[error("未知 value_type: {0}")]
    UnknownValueType(String),
    /// 待解码字节流的长度与 value_type 的宽度不一致。
    #[error("字节宽度不匹配: 期望 {expected} 字节，实际 {actual} 字节")]
    WidthMismatch { expected: usize, actual: usize },
    /// segment/offset 字段既不是数字也不是合法的十六进制字符串。
    #[error("地址字段非法: {0}")]
    InvalidAddress(String),
}

/// 数值的内存类型与解码方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    /// 单字节无符号整数。
    U8,
    /// 两字节小端无符号整数。
    U16LE,
    /// 四字节小端无符号整数。
    U32LE,
    /// 两字节小端有符号整数（XEEN 的 HP 支持负值：昏迷队员为负，实测 `fc ff` = -4）。
    I16LE,
}

impl ValueType {
    /// 该类型的字节宽度。
    ///
    /// # 返回
    /// 宽度字节数（U8=1，U16LE=2，U32LE=4，I16LE=2）。
    pub fn width(self) -> usize {
        match self {
            ValueType::U8 => 1,
            ValueType::U16LE => 2,
            ValueType::U32LE => 4,
            ValueType::I16LE => 2,
        }
    }

    /// 把从内存读到的原始字节解码为数值（统一用 i64 承载，兼容有符号类型）。
    ///
    /// # 参数
    /// - `bytes`：内存快照中该条目位置起的原始字节
    ///
    /// # 返回
    /// 解码后的数值；字节宽度不匹配时返回 [`ProfileError::WidthMismatch`]。
    pub fn decode(self, bytes: &[u8]) -> Result<i64, ProfileError> {
        let expected = self.width();
        if bytes.len() != expected {
            return Err(ProfileError::WidthMismatch {
                expected,
                actual: bytes.len(),
            });
        }
        let value = match self {
            ValueType::U8 => i64::from(bytes[0]),
            ValueType::U16LE => i64::from(u16::from_le_bytes([bytes[0], bytes[1]])),
            ValueType::U32LE => {
                i64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            ValueType::I16LE => i64::from(i16::from_le_bytes([bytes[0], bytes[1]])),
        };
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for ValueType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "u8" => Ok(ValueType::U8),
            "u16le" => Ok(ValueType::U16LE),
            "u32le" => Ok(ValueType::U32LE),
            "i16le" => Ok(ValueType::I16LE),
            other => Err(serde::de::Error::custom(format!(
                "未知 value_type: {other}"
            ))),
        }
    }
}

/// 解析档案里的地址字段：接受十进制数字或带 `"0x"` 前缀的十六进制字符串
/// （无前缀的裸字符串一律报错，防止与十进制数字同形异义），并按目标类型
/// （segment→u16 / offset→u32）做范围校验，越界在加载期即报错。
fn parse_address<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: TryFrom<u64>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let raw: u64 = match value {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom(format!("地址越界: {n}")))?,
        serde_json::Value::String(s) => {
            // 严格抛错：字符串形式必须带 0x 前缀（裸字符串如 "1000" 一律报错，
            // 避免与数字十进制产生同形异义的地雷）
            let trimmed = s.trim();
            let digits = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
                .ok_or_else(|| {
                    serde::de::Error::custom(format!("字符串地址必须带 0x 前缀: {s}"))
                })?;
            u64::from_str_radix(digits, 16)
                .map_err(|_| serde::de::Error::custom(format!("地址字段非法: {s}")))?
        }
        other => {
            return Err(serde::de::Error::custom(format!("地址字段非法: {other}")));
        }
    };
    T::try_from(raw).map_err(|_| serde::de::Error::custom(format!("地址超出范围: {raw}")))
}

/// 一款游戏的内存观察档案，对应 `retro/memory_map.json` 顶层结构。
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryMap {
    /// 游戏标识（如 "xeen"）。
    pub game: String,
    /// 建议轮询间隔（毫秒）。
    pub poll_interval_ms: u64,
    /// 观察条目列表（HP、金币等）。
    pub entries: Vec<Entry>,
}

impl MemoryMap {
    /// 从磁盘加载并解析档案文件。
    ///
    /// # 参数
    /// - `path`：memory_map.json 的路径
    ///
    /// # 返回
    /// 解析后的档案；文件读取或 JSON 解析失败时返回 [`ProfileError`]。
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let text = std::fs::read_to_string(path)?;
        let map = serde_json::from_str(&text)?;
        Ok(map)
    }
}

/// 档案中的一条观察条目（如 HP、金币在内存中的位置）。
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    /// 条目名（如 "hp"、"gold"）。
    pub name: String,
    /// 段地址（实模式 segment，16 位）。
    #[serde(deserialize_with = "parse_address")]
    pub segment: u16,
    /// 段内偏移（32 位）。
    #[serde(deserialize_with = "parse_address")]
    pub offset: u32,
    /// 读取字节数（应等于 value_type 的宽度）。
    pub len: u8,
    /// 数值类型与解码方式。
    pub value_type: ValueType,
}
