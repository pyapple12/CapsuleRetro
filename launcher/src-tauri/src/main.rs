//! launcher 可执行入口：薄壳调用 lib 的 run()（Tauri 2 模板形态，lib 与 bin 分离
//! 便于集成测试直接导入模块）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    launcher_lib::run()
}
