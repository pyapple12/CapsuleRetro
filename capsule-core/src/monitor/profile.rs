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
    /// 条目 len 与 value_type 宽度不一致或为 0（加载期校验）。
    #[error("条目 {name} 的 len 非法: {len}（value_type 宽度 {width}，须相等且非 0）")]
    InvalidLen {
        /// 非法条目名。
        name: String,
        /// 档案声明的 len。
        len: u8,
        /// value_type 实际宽度。
        width: usize,
    },
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
    /// 把目标数值按类型饱和封顶到合法区间（写内存前的加法回绕防护）。
    ///
    /// # 参数
    /// - `value`：加法后的目标值（i64 承载）
    ///
    /// # 返回
    /// 封顶后的值：无符号类型区间 [0, 类型最大值]；I16LE 双向封顶。
    pub fn clamp_value(self, value: i64) -> i64 {
        match self {
            ValueType::U8 => value.clamp(0, i64::from(u8::MAX)),
            ValueType::U16LE => value.clamp(0, i64::from(u16::MAX)),
            ValueType::U32LE => value.clamp(0, i64::from(u32::MAX)),
            ValueType::I16LE => value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)),
        }
    }

    /// 把数值封顶并编码为该类型宽度的内存字节（小端）。
    ///
    /// # 参数
    /// - `value`：目标数值（内部先经 [`Self::clamp_value`] 封顶，之后按
    ///   类型宽度截取不会丢位）
    ///
    /// # 返回
    /// 类型宽度的 LE 字节序列。
    pub fn encode(self, value: i64) -> Vec<u8> {
        let value = self.clamp_value(value);
        match self {
            ValueType::U8 => vec![value as u8],
            ValueType::U16LE => (value as u16).to_le_bytes().to_vec(),
            ValueType::U32LE => (value as u32).to_le_bytes().to_vec(),
            ValueType::I16LE => (value as i16).to_le_bytes().to_vec(),
        }
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
    /// 加载期校验每条 entry 的 `len` 与 `value_type` 宽度一致且非 0：
    /// len:0 或宽度错位的条目在轮询期必然解码失败，提前在加载期报错可把
    /// 故障定位从运行中轮询前移到启动时。
    ///
    /// # 参数
    /// - `path`：memory_map.json 的路径
    ///
    /// # 返回
    /// 解析并校验通过的档案；文件读取、JSON 解析或 len 校验失败时返回
    /// [`ProfileError`]。
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let text = std::fs::read_to_string(path)?;
        let map: Self = serde_json::from_str(&text)?;
        map.entries.iter().try_for_each(|entry| {
            if entry.len == 0 || entry.len as usize != entry.value_type.width() {
                return Err(ProfileError::InvalidLen {
                    name: entry.name.clone(),
                    len: entry.len,
                    width: entry.value_type.width(),
                });
            }
            Ok(())
        })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PL007.2 验证：编码前按类型饱和封顶——u32 封顶不回绕、i16 双向
    /// 封顶；普通值按小端编码。
    #[test]
    fn 编码前按类型饱和封顶() {
        assert_eq!(
            ValueType::U32LE.encode(i64::from(u32::MAX) + 10_000),
            [0xFF, 0xFF, 0xFF, 0xFF]
        );
        assert_eq!(ValueType::U32LE.encode(50_000), [0x50, 0xC3, 0x00, 0x00]);
        assert_eq!(ValueType::U16LE.encode(50_000), [0x50, 0xC3]);
        assert_eq!(ValueType::I16LE.encode(40_000), [0xFF, 0x7F]);
        assert_eq!(ValueType::I16LE.encode(-40_000), [0x00, 0x80]);
        assert_eq!(ValueType::U8.encode(300), [0xFF]);
    }
}
