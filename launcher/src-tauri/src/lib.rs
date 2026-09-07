//! CapsuleRetro launcher 库：UI 进程编排——spawn DOSBox、Job Object 生命周期、
//! game.json 适配层消费、conf 生成。设计理由：lib 与 bin 分离，集成测试可直接
//! 导入 start_session/stop_session 做真实进程验证。

pub mod commands;
pub mod gameconf;
pub mod hud;
pub mod jobctl;
pub mod logging;
pub mod winembed;
pub mod winproc;

/// 组装并运行 Tauri 应用。
pub fn run() {
    // 文件日志先于一切业务初始化（release 无控制台，日志是唯一诊断通道）；
    // 目录定位失败仅停用文件日志并显式告警，不阻断应用启动
    match gameconf::runtime_dir("launcher") {
        Ok(dir) => logging::init(&dir),
        Err(err) => eprintln!("日志目录定位失败，文件日志停用: {err}"),
    }
    let app = tauri::Builder::default()
        .manage(commands::SessionState::default())
        .invoke_handler(tauri::generate_handler![
            commands::launch_game,
            commands::stop_game,
            commands::get_game_info,
            commands::get_guide,
            commands::get_frame,
            commands::inject_input
        ]);
    if let Err(err) = app.run(tauri::generate_context!()) {
        logging::error(&format!("launcher 运行失败: {err}"));
        eprintln!("launcher 运行失败: {err}");
        std::process::exit(1);
    }
}
