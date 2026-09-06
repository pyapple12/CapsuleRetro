//! CapsuleRetro launcher 库：UI 进程编排——spawn DOSBox、Job Object 生命周期、
//! game.json 适配层消费、conf 生成。设计理由：lib 与 bin 分离，集成测试可直接
//! 导入 start_session/stop_session 做真实进程验证。

pub mod commands;
pub mod gameconf;
pub mod hud;
pub mod jobctl;
pub mod winembed;
pub mod winproc;

/// 组装并运行 Tauri 应用。
pub fn run() {
    let app = tauri::Builder::default()
        .manage(commands::SessionState::default())
        .invoke_handler(tauri::generate_handler![
            commands::launch_game,
            commands::stop_game,
            commands::get_frame,
            commands::inject_input
        ]);
    if let Err(err) = app.run(tauri::generate_context!()) {
        eprintln!("launcher 运行失败: {err}");
        std::process::exit(1);
    }
}
