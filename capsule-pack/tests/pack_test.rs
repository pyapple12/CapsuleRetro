//! PL004.6/7 集成测试：capsule-pack 打包主流程（校验、拼装、readme 注入）。
//!
//! 夹具为合成仓库结构（不含真实游戏文件）：`<root>/retro/` 下有 game.json、
//! 游戏目录、攻略、内存档案与补丁包；`<root>/tools/dosbox/` 为 dosbox 构建
//! 产物目录（exe 与 DLL 及无关文件）；另有伪 launcher exe。

use std::io::Write as _;
use std::path::{Path, PathBuf};

use capsule_core::payload::PayloadReader;
use capsule_pack::PackInputs;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const GAME_JSON: &str = r#"{
    "name": "TESTGAME",
    "version": "1.0",
    "dosbox": { "exe_path": "tools/dosbox/dosbox.exe", "webserver_port": 18086, "cpu_cycles": 5000 },
    "assets": { "memory_map": "retro/memory_map.json", "guide": "retro/test-guide.md" },
    "paths": { "game_dir": "GAMEDIR", "save_files": ["*.SAV"] },
    "patches": [ { "id": "t", "name": "测试补丁", "patch": "patches/t.zip" } ]
}"#;

/// 独立临时目录（按 tag 隔离，先清残留）。
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cr-pack-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

/// 写文件（自动建父目录）。
fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("有父目录")).expect("建父目录");
    std::fs::write(path, bytes).expect("写文件");
}

/// 构造合成仓库，返回仓库根。
fn fixture_repo(tag: &str) -> PathBuf {
    let root = temp_dir(tag);
    write(&root.join("retro/game.json"), GAME_JSON.as_bytes());
    write(&root.join("retro/GAMEDIR/GAME.EXE"), b"MZGAME");
    write(&root.join("retro/GAMEDIR/DATA/ITEM.DAT"), b"ITEM");
    write(&root.join("retro/test-guide.md"), b"# guide");
    write(&root.join("retro/memory_map.json"), b"{}");
    let patch = root.join("retro/patches/t.zip");
    write(&root.join("retro/patches/t.zip"), b"");
    let file = std::fs::File::create(&patch).expect("创建补丁 zip");
    let mut writer = ZipWriter::new(file);
    writer
        .start_file("files/x.txt", SimpleFileOptions::default())
        .expect("条目");
    writer.write_all(b"x").expect("字节");
    writer.finish().expect("收尾");
    write(&root.join("tools/dosbox/dosbox.exe"), b"MZDOSBOX");
    write(&root.join("tools/dosbox/SDL2.dll"), b"DLL");
    write(&root.join("tools/dosbox/junk.lib"), b"BUILD-ARTIFACT");
    write(&root.join("launcher.exe"), b"MZLAUNCHER");
    root
}

/// 标准打包输入（输出目录 = `<root>/out`）。
fn inputs(root: &Path) -> PackInputs {
    PackInputs {
        repo_root: root.to_path_buf(),
        launcher_exe: root.join("launcher.exe"),
        dosbox_dir: root.join("tools/dosbox"),
        out_dir: root.join("out"),
    }
}

#[test]
fn 完整打包产物解析一致() {
    let root = fixture_repo("full");
    let outcome = capsule_pack::build_package(&inputs(&root)).expect("打包应成功");

    assert_eq!(outcome.exe, root.join("out/TESTGAME.exe"));
    assert!(outcome.readme.is_file(), "readme 应生成");
    assert!(
        outcome.exe_bytes
            > std::fs::metadata(root.join("launcher.exe"))
                .expect("大小")
                .len()
    );

    let mut reader = PayloadReader::open(&outcome.exe)
        .expect("解析")
        .expect("产物应识别为打包形态");
    assert!(
        reader.build_id().starts_with("0.1.0+"),
        "build_id 应为 <产品版本>+<unix秒>，实际 {}",
        reader.build_id()
    );
    let names = reader.entry_names();
    for expect in [
        "dosbox/bin/SDL2.dll",
        "dosbox/bin/dosbox.exe",
        "payload.json",
        "retro/GAMEDIR/DATA/ITEM.DAT",
        "retro/GAMEDIR/GAME.EXE",
        "retro/game.json",
        "retro/memory_map.json",
        "retro/patches/t.zip",
        "retro/test-guide.md",
    ] {
        assert!(
            names.contains(&expect.to_string()),
            "缺条目 {expect}，实际 {names:?}"
        );
    }
    assert!(
        !names.iter().any(|n| n.ends_with(".lib")),
        "构建中间产物不应入包: {names:?}"
    );
    let config: capsule_core::game_config::GameConfig = reader
        .read_entry_to_string("retro/game.json")
        .expect("读内嵌配置")
        .parse()
        .expect("内嵌配置应可解析");
    assert_eq!(config.name, "TESTGAME");

    let readme = std::fs::read_to_string(&outcome.readme).expect("读 readme");
    assert!(readme.contains("TESTGAME"), "readme 应注入游戏名");
    assert!(readme.contains("0.1.0"), "readme 应注入产品版本");
    assert!(!readme.contains("{{GAME_NAME}}"), "占位符应全部替换");
}

#[test]
fn 缺游戏目录报错() {
    let root = fixture_repo("no-gamedir");
    std::fs::remove_dir_all(root.join("retro/GAMEDIR")).expect("删游戏目录");
    let err = capsule_pack::build_package(&inputs(&root)).expect_err("应报错");
    assert!(err.to_string().contains("游戏目录"), "{err:#}");
}

#[test]
fn 缺攻略报错() {
    let root = fixture_repo("no-guide");
    std::fs::remove_file(root.join("retro/test-guide.md")).expect("删攻略");
    let err = capsule_pack::build_package(&inputs(&root)).expect_err("应报错");
    assert!(err.to_string().contains("攻略"), "{err:#}");
}

#[test]
fn 缺补丁包报错() {
    let root = fixture_repo("no-patch");
    std::fs::remove_file(root.join("retro/patches/t.zip")).expect("删补丁");
    let err = capsule_pack::build_package(&inputs(&root)).expect_err("应报错");
    assert!(err.to_string().contains("补丁包"), "{err:#}");
}

#[test]
fn 缺dosbox可执行报错() {
    let root = fixture_repo("no-dosbox");
    std::fs::remove_file(root.join("tools/dosbox/dosbox.exe")).expect("删 dosbox");
    let err = capsule_pack::build_package(&inputs(&root)).expect_err("应报错");
    assert!(err.to_string().contains("dosbox"), "{err:#}");
}

#[test]
fn 缺launcher报错() {
    let root = fixture_repo("no-launcher");
    std::fs::remove_file(root.join("launcher.exe")).expect("删 launcher");
    let err = capsule_pack::build_package(&inputs(&root)).expect_err("应报错");
    assert!(err.to_string().contains("launcher"), "{err:#}");
}

#[test]
fn 已打包的launcher拒绝再打包() {
    let root = fixture_repo("repack");
    let first = capsule_pack::build_package(&inputs(&root)).expect("首次打包应成功");
    let mut again = inputs(&root);
    again.launcher_exe = first.exe;
    let err = capsule_pack::build_package(&again).expect_err("应拒绝已含 payload 的 launcher");
    assert!(err.to_string().contains("已含 payload"), "{err:#}");
}
