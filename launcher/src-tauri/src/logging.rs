//! 极简文件日志：release 形态无控制台（`windows_subsystem = "windows"`），
//! eprintln! 输出全部丢失；统一追加写到 runtime_dir 的 launcher.log，保证
//! 诊断信息可回收。不做轮转与级别过滤——dosbox.log 的轮转治理属 PL003
//! 范围（审计观察项 #13），届时一并处理。
//!
//! 容错边界（AGENTS.md 白名单登记）：写日志自身失败静默忽略——日志组件
//! 不得反噬主流程；init 未调用时日志调用为空操作。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 初始化日志文件路径（`dir/launcher.log`）；须在应用启动时先于任何日志调用。
/// 目录可能不存在（launcher 目录无其他释放路径），init 时创建；创建失败则
/// 停用文件日志（后续写入按模块容错边界静默跳过）。
pub fn init(dir: &std::path::Path) {
    if std::fs::create_dir_all(dir).is_err() {
        eprintln!("日志目录创建失败，文件日志停用: {}", dir.display());
        return;
    }
    let _ = LOG_PATH.set(dir.join("launcher.log"));
}

/// 追加一行日志（`[HH:MM:SS] message`，UTC 当日时间）。写失败静默——见模块
/// 容错边界说明。
pub fn write_line(message: &str) {
    let Some(path) = LOG_PATH.get() else {
        return;
    };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let time = format!(
        "{:02}:{:02}:{:02}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{time}] {message}");
    }
}

/// 记录一般信息。
pub fn info(message: &str) {
    write_line(&format!("INFO {message}"));
}

/// 记录错误（替代 release 下丢失的 eprintln! 关键诊断）。
pub fn error(message: &str) {
    write_line(&format!("ERROR {message}"));
}
