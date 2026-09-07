//! game_config 加载与校验（TDD 四场景：合法样例 / 缺字段 / 非法端口 / 真实档案文件）。
//! 结构样例取自 retro/game.json 真实片段（独立化：非实现自证 mock）。

use std::path::Path;

use capsule_core::game_config::GameConfig;

#[test]
fn 合法样例解析字段正确() {
    let text = r#"{
        "name": "XEEN",
        "version": "1.0",
        "dosbox": {
            "exe_path": "assets/dosbox/dosbox-staging/dosbox-staging-v0.83.0/dosbox.exe",
            "webserver_port": 18086,
            "cpu_cycles": 5000
        },
        "assets": {
            "memory_map": "retro/memory_map.json",
            "guide": "retro/xeen-guide.md"
        },
        "paths": { "work_dir": ".temp/xeen-work" }
    }"#;
    let config = text.parse::<GameConfig>().expect("合法样例应解析成功");
    assert_eq!(config.name, "XEEN");
    assert_eq!(config.dosbox.webserver_port, 18086);
    assert_eq!(config.dosbox.cpu_cycles, 5000);
    assert_eq!(config.paths.work_dir, Path::new(".temp/xeen-work"));
}

#[test]
fn 缺字段报错() {
    let text = r#"{
        "name": "XEEN",
        "version": "1.0",
        "dosbox": { "exe_path": "a.exe", "webserver_port": 18086, "cpu_cycles": 5000 }
    }"#;
    let err = text.parse::<GameConfig>().expect_err("缺 assets 节应报错");
    assert!(err.to_string().contains("missing field"), "实际错误: {err}");
}

#[test]
fn 非法端口报错() {
    let text = r#"{
        "name": "XEEN",
        "version": "1.0",
        "dosbox": { "exe_path": "a.exe", "webserver_port": 80, "cpu_cycles": 5000 },
        "assets": { "memory_map": "m.json", "guide": "g.md" },
        "paths": { "work_dir": "work" }
    }"#;
    let err = text.parse::<GameConfig>().expect_err("端口 80 应报错");
    assert!(
        err.to_string().contains("非法 webserver 端口"),
        "实际错误: {err}"
    );
}

#[test]
fn 真实档案文件可加载() {
    let config =
        GameConfig::load(Path::new("../retro/game.json")).expect("真实 game.json 应可加载");
    assert_eq!(config.name, "XEEN");
    assert_eq!(config.dosbox.cpu_cycles, 5000);
}

/// name 含空格等非法字符 → 报 InvalidName（name 是运行时目录与启动命令来源）。
#[test]
fn 非法name报错() {
    let text = r#"{
        "name": "XEEN 游戏版",
        "version": "1.0",
        "dosbox": { "exe_path": "a.exe", "webserver_port": 18086, "cpu_cycles": 5000 },
        "assets": { "memory_map": "m.json", "guide": "g.md" },
        "paths": { "work_dir": "work" }
    }"#;
    let err = text.parse::<GameConfig>().expect_err("含空格 name 应报错");
    assert!(err.to_string().contains("非法游戏名"), "实际错误: {err}");

    let text = r#"{
        "name": "",
        "version": "1.0",
        "dosbox": { "exe_path": "a.exe", "webserver_port": 18086, "cpu_cycles": 5000 },
        "assets": { "memory_map": "m.json", "guide": "g.md" },
        "paths": { "work_dir": "work" }
    }"#;
    let err = text.parse::<GameConfig>().expect_err("空 name 应报错");
    assert!(err.to_string().contains("非法游戏名"), "实际错误: {err}");
}

/// 合法 name（字母数字下划线连字符）应通过校验。
#[test]
fn 合法name通过() {
    let text = r#"{
        "name": "XEEN-2_ok",
        "version": "1.0",
        "dosbox": { "exe_path": "a.exe", "webserver_port": 18086, "cpu_cycles": 5000 },
        "assets": { "memory_map": "m.json", "guide": "g.md" },
        "paths": { "work_dir": "work" }
    }"#;
    let config = text.parse::<GameConfig>().expect("合法 name 应通过");
    assert_eq!(config.name, "XEEN-2_ok");
}
