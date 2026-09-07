//! HUD 数据链路验证（VD1/VD3）。
//!
//! VD1：poller 快照与独立 REST 直读双通道对照一致（值/解码/字段映射正确）。
//! VD3：500ms 轮询下本进程 10 秒 CPU 占用 < 2%（GetProcessTimes 采样）。
//! 真实进程 + 轮询线程会运行，默认 `#[ignore]`——显式
//! `cargo test -p launcher -- --ignored` 运行。

use std::sync::mpsc;

use launcher_lib::commands::start_session;
use launcher_lib::gameconf;
use launcher_lib::hud::{HudPollEvent, HudPoller};

/// VD1 + VD3 联合验证。
#[test]
#[ignore = "真实进程测试：DOSBox 窗口闪现 + 10 秒 CPU 采样，显式 --ignored 运行"]
fn hud_双通道对照与cpu开销() {
    let mut session = start_session(None).expect("启动会话应成功");
    let (tx, rx) = mpsc::channel::<HudPollEvent>();

    let repo_root = gameconf::resolve_repo_root(&std::env::current_dir().expect("cwd"))
        .expect("仓库根应可定位");
    let config_path = repo_root.join("retro").join("game.json");
    let config = capsule_core::game_config::GameConfig::load(&config_path).expect("加载配置");
    let map_path = repo_root.join(&config.assets.memory_map);

    let poller = HudPoller::start(
        session.port(),
        map_path.clone(),
        500u64,
        Box::new(move |event| {
            let _ = tx.send(event);
        }),
    );

    // VD3：10 秒窗口内采样本进程 CPU（轮询线程在其中运行）
    let t0 = cpu_seconds();
    std::thread::sleep(std::time::Duration::from_secs(10));
    let t1 = cpu_seconds();
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let cpu_pct = (t1 - t0) / 10.0 / cores as f64 * 100.0;
    poller.stop();

    // VD1：取最后一个快照与独立 REST 直读对照
    let mut last = None;
    while let Ok(event) = rx.try_recv() {
        if let HudPollEvent::Snapshot(s) = event {
            last = Some(s);
        }
    }
    let snap = last.expect("10 秒内应收到 HUD 快照");

    // 独立通道：新 RestClient + 独立档案加载直读同名字段
    let direct = direct_read(session.port(), &map_path);
    assert_eq!(snap.hp, direct.hp, "HP 双通道不一致");
    assert_eq!(snap.gold, direct.gold, "金币双通道不一致");
    assert_eq!(snap.gems, direct.gems, "宝石双通道不一致");
    assert_eq!(snap.food, direct.food, "食物双通道不一致");
    assert_eq!(snap.food_days, direct.food / 18, "食物天数派生不一致");

    assert!(
        cpu_pct < 2.0,
        "VD3 失败：轮询 CPU 占用 {cpu_pct:.2}% 超过 2%"
    );

    session.stop().expect("终止会话应成功");
    assert!(!session.is_alive());
}

/// 独立直读：不经 HudPoller，用独立客户端与档案实例读同名字段。
fn direct_read(port: u16, map_path: &std::path::Path) -> DirectValues {
    use capsule_core::monitor::profile::MemoryMap;
    use capsule_core::monitor::rest::RestClient;

    let client = RestClient::new(port);
    let map = MemoryMap::load(map_path).expect("档案加载");
    let mut values = std::collections::HashMap::new();
    for entry in &map.entries {
        let wanted = matches!(
            entry.name.as_str(),
            "hp1" | "hp2" | "hp3" | "hp4" | "hp5" | "hp6" | "gold" | "gems" | "food"
        );
        if !wanted {
            continue;
        }
        let bytes = client
            .memory(Some(entry.segment), entry.offset, u32::from(entry.len))
            .expect("直读内存");
        values.insert(
            entry.name.clone(),
            entry.value_type.decode(&bytes).expect("解码"),
        );
    }
    DirectValues {
        hp: [
            values["hp1"],
            values["hp2"],
            values["hp3"],
            values["hp4"],
            values["hp5"],
            values["hp6"],
        ],
        gold: values["gold"],
        gems: values["gems"],
        food: values["food"],
    }
}

struct DirectValues {
    hp: [i64; 6],
    gold: i64,
    gems: i64,
    food: i64,
}

/// 本进程累计 CPU 时间（user+kernel，秒）。
fn cpu_seconds() -> f64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let handle = unsafe { GetCurrentProcess() };
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
            .expect("GetProcessTimes");
    }
    let to_secs = |ft: FILETIME| {
        let raw = (ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64;
        raw as f64 / 10_000_000.0
    };
    to_secs(kernel) + to_secs(user)
}
