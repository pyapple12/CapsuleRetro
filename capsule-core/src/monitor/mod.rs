//! monitor：DOSBox Staging HTTP REST 观察库。
//!
//! 通过 DOSBox Staging 0.83 内置的 webserver REST API（仅绑定 127.0.0.1）读取
//! 模拟器的 CPU 状态与底层内存，配合 memory_map 档案（[`profile`]）把原始字节
//! 解码为游戏数值；[`diff`] 提供快照差分以定位内存偏移。
//! 设计理由：观察逻辑（轮询/解码/差分）不含任何界面，供 A 壳 CLI 与 B 壳
//! launcher 复用同一份实现。

pub mod diff;
pub mod profile;
pub mod rest;
