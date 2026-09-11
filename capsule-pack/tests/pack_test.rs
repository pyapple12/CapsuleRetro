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

/// 标准打包输入（输出目录 = `<root>/out`；配置由 GAME_JSON 解析——FIX002.34
/// 单次加载传入，库内不再二次读盘）。
fn inputs(root: &Path) -> PackInputs {
    PackInputs {
        repo_root: root.to_path_buf(),
        config: GAME_JSON.parse().expect("GAME_JSON 应可解析为 GameConfig"),
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
    assert!(
        readme.contains("CapsuleRetro\\testgame"),
        "运行时目录应注入小写派生值（FIX002.35）: {readme}"
    );
    assert!(readme.contains("启动 TESTGAME"), "按钮指引应与实际文案一致");
    assert!(!readme.contains("{{GAME_NAME}}"), "占位符应全部替换");
    assert!(!readme.contains("{{GAME_DIR}}"), "占位符应全部替换");
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

/// FIX002.36 验证：game_dir 含 `..`（zip slip 写侧唯一闸 ensure_safe_rel）
/// 在打包前校验即拒绝。
#[test]
fn game_dir越界拒绝() {
    let root = fixture_repo("evil-gamedir");
    let evil_text = GAME_JSON.replace(r#""game_dir": "GAMEDIR""#, r#""game_dir": "../evil""#);
    write(&root.join("retro/game.json"), evil_text.as_bytes());
    // 镜像真实 CLI 流程：config 从磁盘 game.json 加载后传入（FIX002.34）
    let mut evil_inputs = inputs(&root);
    evil_inputs.config = evil_text
        .parse()
        .expect("越界 game_dir 的配置应可解析（拒绝发生在打包校验层）");
    let err = capsule_pack::build_package(&evil_inputs).expect_err("越界 game_dir 应报错");
    assert!(err.to_string().contains("仓库相对路径"), "{err:#}");
}

/// PL006.2 验证：cover/hero 可选资产链——cover 缺省打包成功但告警（待机页
/// 回落占位的前置）；配置且文件在位则严格校验并收集为 payload 条目；文件
/// 缺失或路径越界仍严格报错（错误策略主线不因"可选"放宽已配置项）。
#[test]
fn 封面主视觉随包与缺失告警() {
    // 缺省：打包成功但带 cover 告警（hero 静默可选，不告警）
    let root = fixture_repo("no-cover");
    let outcome = capsule_pack::build_package(&inputs(&root)).expect("无 cover 应打包成功");
    assert!(
        outcome.warnings.iter().any(|w| w.contains("assets.cover")),
        "无 cover 应产生告警: {:?}",
        outcome.warnings
    );

    // 配置且在位：收集为 payload 条目且无 cover 告警
    let with_text = GAME_JSON.replace(
        r#""guide": "retro/test-guide.md" }"#,
        r#""guide": "retro/test-guide.md",
        "cover": "retro/artwork/xeen2x3.jpg", "hero": "retro/artwork/xeed16x9.jpeg" }"#,
    );
    write(&root.join("retro/game.json"), with_text.as_bytes());
    write(&root.join("retro/artwork/xeen2x3.jpg"), b"JPGCOVER");
    write(&root.join("retro/artwork/xeed16x9.jpeg"), b"JPGHERO");
    let mut with_inputs = inputs(&root);
    with_inputs.config = with_text.parse().expect("含 cover/hero 的配置应可解析");
    with_inputs.out_dir = root.join("out2");
    let outcome2 = capsule_pack::build_package(&with_inputs).expect("打包应成功");
    assert!(
        !outcome2.warnings.iter().any(|w| w.contains("assets.cover")),
        "已配置 cover 不应告警: {:?}",
        outcome2.warnings
    );
    let mut reader = PayloadReader::open(&outcome2.exe)
        .expect("解析")
        .expect("产物应识别为打包形态");
    let names = reader.entry_names();
    for expect in ["retro/artwork/xeen2x3.jpg", "retro/artwork/xeed16x9.jpeg"] {
        assert!(names.contains(&expect.to_string()), "缺条目 {expect}");
    }

    // 配置但文件缺失：严格报错不静默
    std::fs::remove_file(root.join("retro/artwork/xeen2x3.jpg")).expect("删封面");
    let err = capsule_pack::build_package(&with_inputs).expect_err("封面缺失应报错");
    assert!(err.to_string().contains("assets.cover"), "{err:#}");

    // 越界路径：写侧 ensure_safe_rel 拒绝
    let evil_text = with_text.replace(
        r#""cover": "retro/artwork/xeen2x3.jpg""#,
        r#""cover": "../evil.jpg""#,
    );
    write(&root.join("retro/artwork/xeen2x3.jpg"), b"JPGCOVER");
    let mut evil_inputs = inputs(&root);
    evil_inputs.config = evil_text
        .parse()
        .expect("越界 cover 的配置应可解析（拒绝发生在打包校验层）");
    evil_inputs.out_dir = root.join("out3");
    let err = capsule_pack::build_package(&evil_inputs).expect_err("越界 cover 应报错");
    assert!(err.to_string().contains("仓库相对路径"), "{err:#}");
}

/// PL009.2 验证：faces_dir 头像目录随包——目录内 charNN.png 全量收集为
/// payload 条目（HUD 面容取图源）；缺省不入包不告警（HUD 回落槽位号占位）。
#[test]
fn faces_dir头像目录随包() {
    // 缺省：打包成功且无 faces 条目
    let root = fixture_repo("no-faces");
    let outcome = capsule_pack::build_package(&inputs(&root)).expect("无 faces_dir 应打包成功");
    let names = PayloadReader::open(&outcome.exe)
        .expect("解析")
        .expect("产物应识别为打包形态")
        .entry_names();
    assert!(
        !names.iter().any(|n| n.contains("artwork/faces")),
        "缺省不应有头像条目: {names:?}"
    );

    // 配置且文件在位：目录内 PNG 全量收集
    let with_text = GAME_JSON.replace(
        r#""guide": "retro/test-guide.md" }"#,
        r#""guide": "retro/test-guide.md", "faces_dir": "retro/artwork/faces" }"#,
    );
    write(&root.join("retro/game.json"), with_text.as_bytes());
    write(&root.join("retro/artwork/faces/char01.png"), b"FACE01");
    write(&root.join("retro/artwork/faces/char02.png"), b"FACE02");
    let mut with_inputs = inputs(&root);
    with_inputs.config = with_text.parse().expect("含 faces_dir 的配置应可解析");
    with_inputs.out_dir = root.join("out-faces");
    let outcome2 = capsule_pack::build_package(&with_inputs).expect("打包应成功");
    let names2 = PayloadReader::open(&outcome2.exe)
        .expect("解析")
        .expect("产物应识别为打包形态")
        .entry_names();
    for expect in [
        "retro/artwork/faces/char01.png",
        "retro/artwork/faces/char02.png",
    ] {
        assert!(names2.contains(&expect.to_string()), "缺头像条目 {expect}");
    }
}

/// FIX002.36 / FIX002.38 验证：dosbox 兄弟 Resources 目录收集为
/// `dosbox/Resources/**` 条目（PL004 shader 教训的回归点）；缺失时打包
/// 成功但产出告警。
#[test]
fn dosbox_resources收集与缺失告警() {
    let root = fixture_repo("resources");

    // 缺失：打包成功但带告警（不阻断，产物不完整须显式可见）
    let outcome = capsule_pack::build_package(&inputs(&root)).expect("无 Resources 也应打包成功");
    assert!(
        outcome
            .warnings
            .iter()
            .any(|w| w.contains("Resources 目录缺失")),
        "应产生 Resources 缺失告警: {:?}",
        outcome.warnings
    );

    // 存在：收集为 dosbox/Resources/** 条目且无 Resources 告警
    //（无 cover 等可选资产缺省告警不算数——只看本条目口径）
    write(&root.join("tools/Resources/shaders/x.glsl"), b"shader");
    let mut with_res = inputs(&root);
    with_res.out_dir = root.join("out2");
    let outcome2 = capsule_pack::build_package(&with_res).expect("打包应成功");
    assert!(
        !outcome2.warnings.iter().any(|w| w.contains("Resources")),
        "有 Resources 不应告警: {:?}",
        outcome2.warnings
    );
    let mut reader = PayloadReader::open(&outcome2.exe)
        .expect("解析")
        .expect("产物应识别为打包形态");
    assert!(
        reader
            .entry_names()
            .contains(&"dosbox/Resources/shaders/x.glsl".to_string()),
        "Resources 树应随包部署"
    );
}
