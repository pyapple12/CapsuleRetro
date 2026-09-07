//! launcher 会话真实进程测试：spawn 真实 DOSBox 并验证日志 / REST 可达 /
//! Job Object 终结。GUI 会闪现，默认 `#[ignore]`——显式
//! `cargo test -p launcher -- --ignored` 运行（VB1–VB4 的自动化形态）。

use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use launcher_lib::commands::start_session;
use launcher_lib::gameconf;

/// PL002.10/11 验证：启动会话 → REST 可达 + 日志零配置告警 → 终止 → 进程退出。
#[test]
#[ignore = "真实进程测试：闪现 DOSBox 窗口，显式 --ignored 运行"]
fn 启动会话_日志与rest可达_终止() {
    let mut session = start_session().expect("启动会话应成功");
    assert!(session.is_alive(), "DOSBox 进程应存活");

    // REST 可达：10 秒内重试（webserver 启动需要片刻）
    let addr = ("127.0.0.1", session.port());
    let mut up = false;
    for _ in 0..20 {
        if TcpStream::connect(addr).is_ok() {
            up = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(up, "webserver 未在 10 秒内可达（端口 {}）", session.port());

    // 日志落盘且零配置告警（VB3；沿用死键审计手法）
    std::thread::sleep(Duration::from_millis(1500));
    let log = std::fs::read_to_string(
        gameconf::runtime_dir("xeen")
            .expect("本机 LOCALAPPDATA 应存在")
            .join("dosbox.log"),
    )
    .expect("dosbox.log 应存在");
    assert!(!log.contains("CONFIG: Deprecated"), "日志含弃用键告警");
    assert!(!log.contains("No config section"), "日志含无效配置键告警");

    session.stop().expect("终止会话应成功");
    assert!(!session.is_alive(), "终止后进程不应存活");
}

/// PL002.11 验证：连续 5 轮启动/停止，无残留进程、端口每次都能复用。
#[test]
#[ignore = "真实进程测试：闪现 DOSBox 窗口，显式 --ignored 运行"]
fn 五轮启动停止无残留() {
    for round in 1..=5 {
        let mut session = start_session().expect("第 {round} 轮启动应成功");
        std::thread::sleep(Duration::from_millis(1500));
        session.stop().expect("第 {round} 轮停止应成功");
        assert!(!session.is_alive(), "第 {round} 轮终止后进程不应存活");
        // 端口已释放：探测应能重新绑定（等价于 tasklist 无残留的强断言）
        assert!(
            TcpListener::bind(("127.0.0.1", session.port())).is_ok(),
            "第 {round} 轮结束后端口未被释放（存在残留）"
        );
    }
}
