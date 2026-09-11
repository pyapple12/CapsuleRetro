//! game_config 加载与校验（TDD 场景：合法样例 / 缺字段 / 非法端口 / 真实档案
//! 文件 / name 与 v2 字段校验）。结构样例取自 retro/game.json 真实片段
//! （独立化：非实现自证 mock）。

use std::path::Path;

use capsule_core::game_config::GameConfig;

/// v2 结构合法样例（paths = game_dir + save_files，patches 数组）。
fn 合法样例文本() -> String {
    r#"{
        "name": "XEEN",
        "version": "1.0",
        "dosbox": {
            "exe_path": "assets/dosbox/dosbox-staging/dosbox-staging-v0.83.0/dosbox.exe",
            "webserver_port": 18086,
            "cpu_cycles": 5000
        },
        "assets": { "memory_map": "retro/memory_map.json", "guide": "retro/xeen-guide.md" },
        "paths": { "game_dir": "XEEN", "save_files": ["*.SAV", "XEEN.DAT"] },
        "patches": [{ "id": "test-hack", "name": "测试补丁", "patch": "patches/test-hack.zip" }]
    }"#
    .to_string()
}

#[test]
fn 合法样例解析字段正确() {
    let config = 合法样例文本()
        .parse::<GameConfig>()
        .expect("合法样例应解析成功");
    assert_eq!(config.name, "XEEN");
    assert_eq!(config.dosbox.webserver_port, 18086);
    assert_eq!(config.dosbox.cpu_cycles, 5000);
    assert_eq!(config.paths.game_dir, Path::new("XEEN"));
    assert_eq!(config.paths.save_files, vec!["*.SAV", "XEEN.DAT"]);
    assert_eq!(config.patches.len(), 1);
    assert_eq!(config.patches[0].id, "test-hack");
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
        "paths": { "game_dir": "XEEN", "save_files": ["*.SAV"] },
        "patches": []
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
    assert_eq!(config.dosbox.cpu_cycles, 6500, "PL005 实测定案值");
    assert!(
        config.paths.save_files.iter().any(|p| p == "*.SAV"),
        "真实档案应声明存档清单"
    );
}

/// name 含空格等非法字符 → 报 InvalidName（name 是运行时目录与启动命令来源）。
#[test]
fn 非法name报错() {
    for name in ["XEEN 游戏版", ""] {
        let text = 合法样例文本().replace("XEEN", name);
        let err = text.parse::<GameConfig>().expect_err("非法 name 应报错");
        assert!(err.to_string().contains("非法游戏名"), "实际错误: {err}");
    }
}

/// 合法 name（字母数字下划线连字符）应通过校验。
#[test]
fn 合法name通过() {
    let config = 合法样例文本()
        .replace("XEEN", "XEEN-2_ok")
        .parse::<GameConfig>()
        .expect("合法 name 应通过");
    assert_eq!(config.name, "XEEN-2_ok");
}

/// PL003.8 验证：save_files 含空白条目或路径分隔符 → 报 InvalidSaveFiles。
#[test]
fn 非法save_files报错() {
    for bad in ["", "  ", "a/b", "a\\b"] {
        // serde_json 构造保证任意字符串的 JSON 转义正确（反斜杠等）
        let paths = serde_json::json!({ "game_dir": "XEEN", "save_files": [bad] }).to_string();
        let text = 合法样例文本().replace(
            r#""paths": { "game_dir": "XEEN", "save_files": ["*.SAV", "XEEN.DAT"] }"#,
            &format!(r#""paths": {paths}"#),
        );
        let err = text
            .parse::<GameConfig>()
            .expect_err("非法 save_files 应报错");
        assert!(
            err.to_string().contains("非法 save_files"),
            "实际错误: {err}"
        );
    }
}

/// FIX002.15 验证：game_dir 为空报 InvalidGameDir 专用变体（不再误挂
/// InvalidPatches，程序化归因正确）。
#[test]
fn game_dir为空报专用错误() {
    use capsule_core::game_config::GameConfigError;
    let text = 合法样例文本().replace(
        r#""paths": { "game_dir": "XEEN", "save_files": ["*.SAV", "XEEN.DAT"] }"#,
        r#""paths": { "game_dir": "", "save_files": ["*.SAV"] }"#,
    );
    let err = text.parse::<GameConfig>().expect_err("空 game_dir 应报错");
    assert!(
        matches!(err, GameConfigError::InvalidGameDir(_)),
        "应报 InvalidGameDir，实际: {err:?}"
    );
}

/// FIX002.25 验证：guest_exe 缺省回落 name 小写派生（适配层约定），
/// 显式声明优先（exe 基名 ≠ name 的下一款游戏可配置）。
#[test]
fn guest_exe派生与显式覆盖() {
    let config = 合法样例文本()
        .parse::<GameConfig>()
        .expect("合法样例应解析成功");
    assert_eq!(
        config.guest_exe_name(),
        "xeen.exe",
        "缺省应回落 name 小写派生"
    );

    let explicit = 合法样例文本()
        .replace(
            r#""paths": { "game_dir": "XEEN", "save_files": ["*.SAV", "XEEN.DAT"] }"#,
            r#""paths": { "game_dir": "XEEN", "guest_exe": "MAGIC4.EXE", "save_files": ["*.SAV"] }"#,
        )
        .parse::<GameConfig>()
        .expect("显式 guest_exe 应解析成功");
    assert_eq!(explicit.guest_exe_name(), "MAGIC4.EXE", "显式声明优先");
}
/// PL003.8 验证：patches id 为空或重复 → 报 InvalidPatches。
#[test]
fn 非法patches报错() {
    const ENTRY: &str =
        r#"{ "id": "test-hack", "name": "测试补丁", "patch": "patches/test-hack.zip" }"#;
    const NEEDLE: &str = r#""patches": [{ "id": "test-hack", "name": "测试补丁", "patch": "patches/test-hack.zip" }]"#;

    let empty_id = 合法样例文本().replace(
        NEEDLE,
        r#""patches": [{ "id": "", "name": "测试补丁", "patch": "patches/test-hack.zip" }]"#,
    );
    let err = empty_id.parse::<GameConfig>().expect_err("空 id 应报错");
    assert!(err.to_string().contains("非法 patches"), "实际错误: {err}");

    let duplicated = 合法样例文本().replace(NEEDLE, &format!(r#""patches": [{ENTRY}, {ENTRY}]"#));
    let err = duplicated
        .parse::<GameConfig>()
        .expect_err("重复 id 应报错");
    assert!(err.to_string().contains("重复"), "实际错误: {err}");
}

/// PL009.2 验证：faces_dir 可选——缺省 None（HUD 回落槽位号占位），
/// 配置后解析为目录路径（打包收集与 HUD 取图源依据）。
#[test]
fn faces_dir可选解析() {
    let config = 合法样例文本()
        .parse::<GameConfig>()
        .expect("合法样例应解析成功");
    assert_eq!(config.assets.faces_dir, None, "缺省应为 None");

    let explicit = 合法样例文本()
        .replace(
            r#""guide": "retro/xeen-guide.md" }"#,
            r#""guide": "retro/xeen-guide.md", "faces_dir": "retro/artwork/faces" }"#,
        )
        .parse::<GameConfig>()
        .expect("含 faces_dir 应解析成功");
    assert_eq!(
        explicit.assets.faces_dir.as_deref(),
        Some(Path::new("retro/artwork/faces"))
    );
}
