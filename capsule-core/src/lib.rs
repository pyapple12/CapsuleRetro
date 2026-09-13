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
