//! capsule-core：CapsuleRetro 的核心库 crate。
//!
//! 职责：save 打包/解包、补丁应用、路径管理，以及 [`monitor`]（DOSBox Staging
//! REST 观察库）。设计理由：launcher 与 packer 需要共用同一份数据逻辑，收敛在
//! 一个 crate 里避免重复实现；monitor 作为公开模块，供 launcher（B 壳）与
//! xmonitor CLI（A 壳诊断工具）共同消费。

pub mod monitor;
