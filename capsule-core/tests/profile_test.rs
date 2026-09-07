//! ValueType 解码的 TDD 验证（PL001.2，对应 z.plan 附录 PL001 验证方案 V1）。
//!
//! 四用例：u16le 解码 1337、u8、u32le、字节宽度与类型不匹配报 WidthMismatch。

use capsule_core::monitor::profile::{MemoryMap, ProfileError, ValueType};

#[test]
fn u16le_decodes_little_endian_1337() {
    let vt = ValueType::U16LE;
    let bytes = [0x39, 0x05]; // 小端：低字节在前 → 0x0539 = 1337
    assert_eq!(vt.decode(&bytes).unwrap(), 1337);
}

#[test]
fn u8_decodes_single_byte() {
    let vt = ValueType::U8;
    assert_eq!(vt.decode(&[0xAB]).unwrap(), 0xAB);
}

#[test]
fn u32le_decodes_four_bytes() {
    let vt = ValueType::U32LE;
    let bytes = [0x78, 0x56, 0x34, 0x12]; // 小端 → 0x12345678
    assert_eq!(vt.decode(&bytes).unwrap(), 0x1234_5678);
}

/// i16le 解码负数：XEEN 昏迷队员 HP=-4 的真实字节 `fc ff`（2026-09-05 实测抓取）。
#[test]
fn i16le_decodes_negative_hp() {
    let vt = ValueType::I16LE;
    let bytes = [0xFC, 0xFF]; // 小端有符号 → -4
    assert_eq!(vt.decode(&bytes).unwrap(), -4);
    assert_eq!(vt.decode(&[0x0B, 0x00]).unwrap(), 11); // 正常值不受影响
}

#[test]
fn width_mismatch_returns_error() {
    let vt = ValueType::U16LE;
    let bytes = [0x01, 0x02, 0x03]; // 3 字节 ≠ u16le 的 2 字节
    let err = vt.decode(&bytes).unwrap_err();
    assert!(matches!(
        err,
        ProfileError::WidthMismatch {
            expected: 2,
            actual: 3
        }
    ));
}

/// 合法档案：全字段、十六进制字符串地址，应完整解析。
#[test]
fn memory_map_valid_json_parses() {
    let json = r#"{
        "game": "xeen",
        "poll_interval_ms": 500,
        "entries": [
            { "name": "hp", "segment": "0x1234", "offset": "0x5678", "len": 2, "value_type": "u16le" }
        ]
    }"#;
    let map: MemoryMap = serde_json::from_str(json).unwrap();
    assert_eq!(map.game, "xeen");
    assert_eq!(map.poll_interval_ms, 500);
    assert_eq!(map.entries.len(), 1);
    let e = &map.entries[0];
    assert_eq!(e.name, "hp");
    assert_eq!(e.segment, 0x1234);
    assert_eq!(e.offset, 0x5678);
    assert_eq!(e.len, 2);
    assert_eq!(e.value_type, ValueType::U16LE);
}

/// 缺 poll_interval_ms 字段 → 解析必须报错（严格抛错，不静默给默认值）。
#[test]
fn memory_map_missing_poll_interval_errors() {
    let json = r#"{
        "game": "xeen",
        "entries": []
    }"#;
    assert!(serde_json::from_str::<MemoryMap>(json).is_err());
}

/// entries 为空数组：结构合法，解析成功且条目数为 0。
#[test]
fn memory_map_empty_entries_parses() {
    let json = r#"{ "game": "xeen", "poll_interval_ms": 500, "entries": [] }"#;
    let map: MemoryMap = serde_json::from_str(json).unwrap();
    assert!(map.entries.is_empty());
}

/// 未知 value_type 字符串 → 解析必须报错。
#[test]
fn memory_map_unknown_value_type_errors() {
    let json = r#"{
        "game": "xeen",
        "poll_interval_ms": 500,
        "entries": [
            { "name": "hp", "segment": "0x1234", "offset": "0x5678", "len": 2, "value_type": "f64" }
        ]
    }"#;
    let err = serde_json::from_str::<MemoryMap>(json).unwrap_err();
    assert!(err.to_string().contains("未知 value_type"));
}

/// 裸字符串地址（无 0x 前缀）→ 必须报错，防止与十进制数字同形异义。
#[test]
fn memory_map_bare_string_address_errors() {
    let json = r#"{
        "game": "xeen",
        "poll_interval_ms": 500,
        "entries": [
            { "name": "hp", "segment": "1000", "offset": "0x10", "len": 2, "value_type": "u16le" }
        ]
    }"#;
    let err = serde_json::from_str::<MemoryMap>(json).unwrap_err();
    assert!(err.to_string().contains("0x 前缀"));
}

/// len 与 value_type 宽度不符的档案必须在加载期报 InvalidLen（FIX001.16）。
#[test]
fn memory_map_width_mismatch_len_errors_on_load() {
    let json = r#"{
        "game": "xeen",
        "poll_interval_ms": 500,
        "entries": [
            { "name": "hp", "segment": "0x1234", "offset": "0x5678", "len": 3, "value_type": "u16le" }
        ]
    }"#;
    let path = std::env::temp_dir().join("capsule-core-profile-bad-len.json");
    std::fs::write(&path, json).unwrap();
    let err = MemoryMap::load(&path).unwrap_err();
    assert!(matches!(
        err,
        ProfileError::InvalidLen {
            ref name,
            len: 3,
            width: 2
        } if name == "hp"
    ));
    let _ = std::fs::remove_file(&path);
}

/// len=0 的档案必须在加载期报 InvalidLen（轮询期必然解码失败的条目提前拦截）。
#[test]
fn memory_map_zero_len_errors_on_load() {
    let json = r#"{
        "game": "xeen",
        "poll_interval_ms": 500,
        "entries": [
            { "name": "hp", "segment": "0x1234", "offset": "0x5678", "len": 0, "value_type": "u8" }
        ]
    }"#;
    let path = std::env::temp_dir().join("capsule-core-profile-zero-len.json");
    std::fs::write(&path, json).unwrap();
    let err = MemoryMap::load(&path).unwrap_err();
    assert!(matches!(
        err,
        ProfileError::InvalidLen {
            ref name,
            len: 0,
            width: 1
        } if name == "hp"
    ));
    let _ = std::fs::remove_file(&path);
}

/// 合法档案走 MemoryMap::load 全链路（磁盘读取 + 解析 + len 校验）应成功。
#[test]
fn memory_map_valid_file_loads() {
    let json = r#"{
        "game": "xeen",
        "poll_interval_ms": 500,
        "entries": [
            { "name": "hp", "segment": "0x1234", "offset": "0x5678", "len": 2, "value_type": "u16le" }
        ]
    }"#;
    let path = std::env::temp_dir().join("capsule-core-profile-good.json");
    std::fs::write(&path, json).unwrap();
    let map = MemoryMap::load(&path).expect("合法档案应加载成功");
    assert_eq!(map.entries.len(), 1);
    let _ = std::fs::remove_file(&path);
}
