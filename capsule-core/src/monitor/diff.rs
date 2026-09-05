//! diff：内存快照差分纯函数。
//!
//! 差分搜索的核心：对两份等长快照逐字节对比，找出发生变化的地址。配合
//! "游戏内改变数值前后各拍一份快照"的操作规程，候选地址可快速收敛到目标
//! 变量。纯函数、无副作用，便于单元测试。

use crate::monitor::rest::MonitorError;

/// 对比两份等长快照，返回发生变化的字节地址（升序）。
///
/// # 参数
/// - `prev`：变化前的快照
/// - `curr`：变化后的快照
///
/// # 返回
/// 变化地址列表；两份快照长度不一致时返回 [`MonitorError::LengthMismatch`]。
pub fn diff_snapshots(prev: &[u8], curr: &[u8]) -> Result<Vec<u32>, MonitorError> {
    if prev.len() != curr.len() {
        return Err(MonitorError::LengthMismatch {
            prev: prev.len(),
            curr: curr.len(),
        });
    }
    let changed = prev
        .iter()
        .zip(curr.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(idx, _)| idx as u32)
        .collect();
    Ok(changed)
}

/// 二次收敛：从首轮候选地址中挑出"在第二轮再次变化"的地址。
///
/// # 参数
/// - `prev`：第二轮变化前的快照
/// - `curr`：第二轮变化后的快照
/// - `candidates`：首轮差分得到的候选地址
///
/// # 返回
/// 在第二轮同样发生了变化的候选地址（升序）；长度不一致时返回
/// [`MonitorError::LengthMismatch`]。
pub fn filter_by_change(
    prev: &[u8],
    curr: &[u8],
    candidates: &[u32],
) -> Result<Vec<u32>, MonitorError> {
    if prev.len() != curr.len() {
        return Err(MonitorError::LengthMismatch {
            prev: prev.len(),
            curr: curr.len(),
        });
    }
    let kept = candidates
        .iter()
        .copied()
        .filter(|addr| {
            let idx = *addr as usize;
            idx < prev.len() && prev[idx] != curr[idx]
        })
        .collect();
    Ok(kept)
}
