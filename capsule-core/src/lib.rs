//! capsule-core：CapsuleRetro 的核心库 crate。
//!
//! 职责：save 打包/解包、补丁应用、路径管理，以及 [`monitor`]（DOSBox Staging
//! REST 观察库）、[`cc_archive`]（XEEN CC 资源容器解析，PL008）与 [`maze`]
//! （XEEN 迷宫数据解析，PL010）。设计理由：launcher 与 packer 需要共用同一份
//! 数据逻辑，收敛在一个 crate 里避免重复实现；monitor 作为公开模块，供
//! launcher（B 壳）与 xmonitor CLI（A 壳诊断工具）共同消费；cc_archive 与
//! maze 供 xeen-extractor CLI 使用。

pub mod cc_archive;
pub mod game_config;
pub mod maze;
pub mod monitor;
pub mod patch;
pub mod paths;
pub mod payload;
pub mod save;

/// 校验 zip 条目相对路径：非空、不含反斜杠、组件全部为普通名（拒绝 `..` /
/// 根 / 盘符等路径穿越形态，zip slip 家族的唯一写侧闸门）。save / patch /
/// payload 三格式共用单份校验体（A003 #6 收敛），错误映射由调用方按各自
/// 错误变体完成。
///
/// # 返回
/// `Some(rel)` = 合法相对路径；`None` = 非法（调用方决定报错形态）。
pub(crate) fn validate_rel_path(path: &str) -> Option<std::path::PathBuf> {
    use std::path::Component;
    if path.is_empty() || path.contains('\\') {
        return None;
    }
    let path = std::path::Path::new(path);
    if !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(path.to_path_buf())
}
