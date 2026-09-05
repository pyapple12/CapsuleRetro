//! 快照差分纯函数的单元测试（PL001.6，对应验证方案 V1 精神：纯逻辑可断言）。

use capsule_core::monitor::diff::{diff_snapshots, filter_by_change};
use capsule_core::monitor::rest::MonitorError;

/// 16 字节快照中有 3 处变化 → 恰好返回 3 个地址且有序。
#[test]
fn diff_finds_exactly_changed_addresses() {
    let prev = [0u8; 16];
    let mut curr = [0u8; 16];
    curr[2] = 9;
    curr[7] = 1;
    curr[15] = 0xFF;
    let changed = diff_snapshots(&prev, &curr).unwrap();
    assert_eq!(changed, vec![2, 7, 15]);
}

/// 无变化 → 空列表。
#[test]
fn diff_of_identical_snapshots_is_empty() {
    let snap = [5u8; 16];
    assert!(diff_snapshots(&snap, &snap).unwrap().is_empty());
}

/// 长度不等 → LengthMismatch 错误（快照必须等长）。
#[test]
fn diff_rejects_unequal_lengths() {
    let prev = [0u8; 8];
    let curr = [0u8; 16];
    let err = diff_snapshots(&prev, &curr).unwrap_err();
    assert!(matches!(
        err,
        MonitorError::LengthMismatch { prev: 8, curr: 16 }
    ));
}

/// 二次过滤：候选中只有"在第二轮再次变化"的地址被保留。
#[test]
fn filter_keeps_only_changed_again() {
    // s1 → s2：地址 2、7、15 变化（首轮候选；s1 为全零基准，隐含在下方初值中）
    let mut s2 = [0u8; 16];
    s2[2] = 9;
    s2[7] = 1;
    s2[15] = 0xFF;
    // s2 → s3：只有地址 7、15 再次变化（地址 2 回到原值或未再变）
    let mut s3 = s2;
    s3[7] = 3;
    s3[15] = 0x11;
    let kept = filter_by_change(&s2, &s3, &[2, 7, 15]).unwrap();
    assert_eq!(kept, vec![7, 15]);
}
