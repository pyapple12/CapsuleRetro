//! PL003 save / patch / paths 三模块集成测试。
//!
//! 覆盖：save 打包解包往返、损坏检测与降级、原子替换；补丁合法应用与
//! 各非法形态（越界路径 / 目标冲突 / 未知 action / 缺 source）；工作副本
//! 释放重建与生命周期编排；补丁纪律反向验收（全程断言原版目录零写入）。

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use capsule_core::patch;
use capsule_core::paths;
use capsule_core::save::{self, SaveMeta};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

static SEQ: AtomicU32 = AtomicU32::new(0);

/// 建立唯一的临时目录（测试间互不干扰，测试内自行清理与否无碍断言）。
fn temp_dir(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("pl003-{}-{}-{}", tag, std::process::id(), n));
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

/// 构造 zip 文件（条目名 + 内容字节）。
fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).expect("创建 zip 文件");
    let mut zw = ZipWriter::new(file);
    for (name, bytes) in entries {
        zw.start_file(name.to_string(), SimpleFileOptions::default())
            .expect("start_file");
        zw.write_all(bytes).expect("写条目");
    }
    zw.finish().expect("finish zip");
}

/// 列出目录下全部相对路径（递归、排序），用于文件集断言。
fn dir_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).expect("read_dir") {
            let p = entry.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(
                    p.strip_prefix(dir)
                        .expect("prefix")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}

/// 读文件字节。
fn read(dir: &Path, name: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    std::fs::File::open(dir.join(name))
        .expect("打开文件")
        .read_to_end(&mut buf)
        .expect("读文件");
    buf
}

/// 写文件字节（自动建父目录）。
fn write(dir: &Path, name: &str, bytes: &[u8]) {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).expect("建父目录");
    }
    std::fs::write(&p, bytes).expect("写文件");
}

/// 采集目录全部文件内容快照（补丁纪律断言用：原版前后必须一致）。
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    dir_files(dir)
        .into_iter()
        .map(|name| (name.clone(), read(dir, &name)))
        .collect()
}

fn meta(hack: Option<&str>) -> SaveMeta {
    SaveMeta {
        version: "1".to_string(),
        hack: hack.map(str::to_string),
        settings: serde_json::json!({}),
    }
}

// ---------- save 模块 ----------

/// PL003.4 验证：pack → unpack 往返，清单命中文件进包、未命中不进、meta 一致。
#[test]
fn save打包解包往返一致() {
    let work = temp_dir("pack-work");
    let out = temp_dir("pack-out");
    write(&work, "XEEN01.SAV", b"slot1");
    write(&work, "DARK02.SAV", b"slot2");
    write(&work, "XEEN.DAT", b"roster");
    write(&work, "XEEN.EXE", b"game binary");

    let patterns = ["*.SAV".to_string(), "XEEN.DAT".to_string()];
    let packed = save::pack(
        &work,
        &patterns,
        &meta(Some("test-hack")),
        &out.join("s.save"),
    )
    .expect("pack");
    assert_eq!(packed, 3, "应打包 3 个命中文件");

    let restore = temp_dir("pack-restore");
    let got = save::unpack(&out.join("s.save"), &restore).expect("unpack");
    assert_eq!(got, meta(Some("test-hack")));
    assert_eq!(
        dir_files(&restore),
        vec!["DARK02.SAV", "XEEN.DAT", "XEEN01.SAV"],
        "未命中的 XEEN.EXE 不应进包"
    );
    assert_eq!(read(&restore, "XEEN01.SAV"), b"slot1");
    assert_eq!(read(&restore, "XEEN.DAT"), b"roster");
}

/// PL003.4 验证：损坏 zip（非 zip 字节）报 Corrupted。
#[test]
fn 损坏save报损坏错误() {
    let dir = temp_dir("corrupt");
    write(&dir, "s.save", b"this is not a zip file at all");
    let err = save::unpack(&dir.join("s.save"), &temp_dir("corrupt-work")).unwrap_err();
    assert!(
        matches!(err, save::SaveError::Corrupted(_)),
        "实际: {err:?}"
    );
}

/// PL003.4 验证：缺 meta.json 的 zip 报 Corrupted（结构不完整）。
#[test]
fn 缺meta的save报损坏错误() {
    let dir = temp_dir("nometa");
    make_zip(&dir.join("s.save"), &[("save/XEEN01.SAV", b"x")]);
    let err = save::unpack(&dir.join("s.save"), &temp_dir("nometa-work")).unwrap_err();
    assert!(
        matches!(err, save::SaveError::Corrupted(_)),
        "实际: {err:?}"
    );
}

/// PL003.4 验证：save 外的条目或越界路径报 UnsafeEntry（严格报错不降级）。
#[test]
fn 越界条目报路径非法() {
    let dir = temp_dir("unsafe");
    make_zip(
        &dir.join("s.save"),
        &[
            ("meta.json", br#"{"version":"1","hack":null,"settings":{}}"#),
            ("evil.txt", b"x"),
        ],
    );
    let err = save::unpack(&dir.join("s.save"), &temp_dir("unsafe-work")).unwrap_err();
    assert!(
        matches!(err, save::SaveError::UnsafeEntry(_)),
        "实际: {err:?}"
    );

    make_zip(
        &dir.join("s2.save"),
        &[
            ("meta.json", br#"{"version":"1","hack":null,"settings":{}}"#),
            ("save/../evil", b"x"),
        ],
    );
    let err = save::unpack(&dir.join("s2.save"), &temp_dir("unsafe-work2")).unwrap_err();
    assert!(
        matches!(err, save::SaveError::UnsafeEntry(_)),
        "实际: {err:?}"
    );
}

/// PL003.5 验证：load_or_recover 三态——完好解包 / 损坏降级备份 / 缺失空档。
#[test]
fn load_or_recover三态() {
    let dir = temp_dir("recover");
    let work = temp_dir("recover-work");
    let save_path = dir.join("s.save");

    // 缺失：返回默认 meta，不落盘
    let (got, recovered) =
        save::load_or_recover(&save_path, &work, meta(None)).expect("缺失态应成功");
    assert!(!recovered);
    assert_eq!(got, meta(None));
    assert!(!save_path.exists(), "缺失态不应创建档案文件");

    // 完好：解包 + meta 返回
    make_zip(
        &save_path,
        &[
            (
                "meta.json",
                br#"{"version":"1","hack":"test-hack","settings":{}}"#,
            ),
            ("save/XEEN01.SAV", b"data"),
        ],
    );
    let (got, recovered) =
        save::load_or_recover(&save_path, &work, meta(None)).expect("完好态应成功");
    assert!(!recovered);
    assert_eq!(got.hack.as_deref(), Some("test-hack"));
    assert_eq!(read(&work, "XEEN01.SAV"), b"data");

    // 损坏：降级——备份 .corrupt + 返回默认 meta
    std::fs::write(&save_path, b"garbage not zip").expect("写坏档");
    let (got, recovered) = save::load_or_recover(&save_path, &work, meta(Some("test-hack")))
        .expect("损坏态应降级成功");
    assert!(recovered, "损坏应报告 recovered");
    assert_eq!(got, meta(Some("test-hack")));
    let backup = dir.join("s.save.corrupt");
    assert!(backup.is_file(), "损坏档案应备份为 .corrupt");
    assert_eq!(std::fs::read(&backup).expect("读备份"), b"garbage not zip");
}

/// PL003.5 验证：原子替换覆盖已存在目标，替换后临时文件消失。
#[test]
fn 原子替换覆盖旧档() {
    let dir = temp_dir("atomic");
    let target = dir.join("s.save");
    std::fs::write(&target, b"old").expect("写旧档");
    let tmp = dir.join("s.save.tmp");
    std::fs::write(&tmp, b"new").expect("写新档");
    save::replace_atomically(&tmp, &target).expect("替换");
    assert_eq!(std::fs::read(&target).expect("读"), b"new");
    assert!(!tmp.exists(), "临时文件应随替换消失");
}

/// PL003.4 验证：通配符语义——仅 '*'、大小写不敏感、精确名匹配。
#[test]
fn 通配符匹配语义() {
    assert!(save::match_pattern("XEEN01.SAV", "*.SAV"));
    assert!(save::match_pattern("xeen01.sav", "*.SAV"), "大小写不敏感");
    assert!(save::match_pattern("XEEN.DAT", "XEEN.DAT"));
    assert!(save::match_pattern("AxB", "A*B"));
    assert!(save::match_pattern("AB", "A*B"), "'*' 可匹配空段");
    assert!(!save::match_pattern("XEEN.EXE", "*.SAV"));
    assert!(
        !save::match_pattern("XEEN.DATX", "XEEN.DAT"),
        "精确名不前缀匹配"
    );
    assert!(!save::match_pattern("aXB", "A*C"));
}

// ---------- patch 模块 ----------

/// PL003.6 验证：合法补丁 replace=覆盖已存在、add=新增，返回条目数。
#[test]
fn 补丁合法应用() {
    let work = temp_dir("patch-work");
    let zip_dir = temp_dir("patch-zip");
    write(&work, "PLAY.BAT", b"XEEN\r\n");
    make_zip(
        &zip_dir.join("p.zip"),
        &[
            (
                "manifest.json",
                br#"[{"action":"replace","path":"PLAY.BAT","source":"files/PLAY.BAT"},
                    {"action":"add","path":"PATCHED.TXT","source":"files/PATCHED.TXT"}]"#,
            ),
            ("files/PLAY.BAT", b"XEEN\r\nrem patched\r\n"),
            ("files/PATCHED.TXT", b"applied"),
        ],
    );
    let n = patch::apply(&zip_dir.join("p.zip"), &work).expect("apply");
    assert_eq!(n, 2);
    assert_eq!(read(&work, "PLAY.BAT"), b"XEEN\r\nrem patched\r\n");
    assert_eq!(read(&work, "PATCHED.TXT"), b"applied");
}

/// PL003.6 验证：replace 目标不存在（manifest 写错或换了释放源）显式报错。
#[test]
fn replace目标缺失报错() {
    let work = temp_dir("patch-miss");
    let zip_dir = temp_dir("patch-miss-zip");
    make_zip(
        &zip_dir.join("p.zip"),
        &[
            (
                "manifest.json",
                br#"[{"action":"replace","path":"NOPE.BAT","source":"files/N"}]"#,
            ),
            ("files/N", b"x"),
        ],
    );
    let err = patch::apply(&zip_dir.join("p.zip"), &work).unwrap_err();
    assert!(
        matches!(err, patch::PatchError::MissingTarget { .. }),
        "实际: {err:?}"
    );
}

/// PL003.6 验证：add 目标已存在（污染的释放源或重复 add）显式报错。
#[test]
fn add目标已存在报错() {
    let work = temp_dir("patch-dup");
    let zip_dir = temp_dir("patch-dup-zip");
    write(&work, "MARK.TXT", b"old");
    make_zip(
        &zip_dir.join("p.zip"),
        &[
            (
                "manifest.json",
                br#"[{"action":"add","path":"MARK.TXT","source":"files/M"}]"#,
            ),
            ("files/M", b"new"),
        ],
    );
    let err = patch::apply(&zip_dir.join("p.zip"), &work).unwrap_err();
    assert!(
        matches!(err, patch::PatchError::TargetExists { .. }),
        "实际: {err:?}"
    );
}

/// PL003.6 验证：manifest path 越界（zip slip 家族）全部拒绝。
#[test]
fn 越界路径拒绝() {
    let bad_paths = [
        "../evil",
        "a/../../evil",
        "/abs",
        r"\\server\share",
        "C:/evil",
        "",
        "sub/../evil",
    ];
    for p in bad_paths {
        let work = temp_dir("patch-slip");
        let zip_dir = temp_dir("patch-slip-zip");
        // serde_json 构造保证任意 path 字符串的 JSON 转义正确
        let manifest = serde_json::to_string(&serde_json::json!([
            {"action": "add", "path": p, "source": "files/M"}
        ]))
        .expect("构造 manifest");
        make_zip(
            &zip_dir.join("p.zip"),
            &[("manifest.json", manifest.as_bytes()), ("files/M", b"x")],
        );
        let err = patch::apply(&zip_dir.join("p.zip"), &work).unwrap_err();
        assert!(
            matches!(err, patch::PatchError::UnsafePath(_)),
            "path {p:?} 应判 UnsafePath，实际: {err:?}"
        );
    }
}

/// PL003.6 验证：未知 action 与缺失 source 显式报错。
#[test]
fn 未知action与缺source报错() {
    let work = temp_dir("patch-bad");
    let zip_dir = temp_dir("patch-bad-zip");
    make_zip(
        &zip_dir.join("p.zip"),
        &[(
            "manifest.json",
            br#"[{"action":"delete","path":"X","source":"files/X"}]"#,
        )],
    );
    let err = patch::apply(&zip_dir.join("p.zip"), &work).unwrap_err();
    assert!(
        matches!(err, patch::PatchError::UnknownAction(_)),
        "实际: {err:?}"
    );

    make_zip(
        &zip_dir.join("p2.zip"),
        &[(
            "manifest.json",
            br#"[{"action":"add","path":"X","source":"files/MISSING"}]"#,
        )],
    );
    let err = patch::apply(&zip_dir.join("p2.zip"), &work).unwrap_err();
    assert!(
        matches!(err, patch::PatchError::MissingSource(_)),
        "实际: {err:?}"
    );
}

// ---------- paths 模块 ----------

/// PL003.7 验证：release_work 清空重建——上次运行残留被清除（自愈 + 防污染）。
#[test]
fn 释放重建清除残留() {
    let original = temp_dir("rel-orig");
    let work = temp_dir("rel-work");
    write(&original, "XEEN.EXE", b"game");
    write(&original, "DARK.CC", b"big file");

    paths::release_work(&original, &work).expect("首次释放");
    assert_eq!(dir_files(&work), vec!["DARK.CC", "XEEN.EXE"]);

    // 模拟上次会话残留：新文件 + 原文件被改写
    write(&work, "PATCHED.TXT", b"stale patch");
    write(&work, "XEEN.EXE", b"modified");
    paths::release_work(&original, &work).expect("重建");
    assert_eq!(
        dir_files(&work),
        vec!["DARK.CC", "XEEN.EXE"],
        "残留文件应被清除"
    );
    assert_eq!(
        read(&work, "XEEN.EXE"),
        b"game",
        "被改写的文件应还原为原版内容"
    );
}

/// PL003.7 验证：prepare_session 全链路（释放 → 补丁 → 存档解包），且原版
/// 目录字节级零改动（补丁纪律反向验收）。
#[test]
fn 启动前置序列全链路且原版零写入() {
    let original = temp_dir("lif-orig");
    let work = temp_dir("lif-work");
    let zips = temp_dir("lif-zips");
    write(&original, "PLAY.BAT", b"XEEN\r\n");
    write(&original, "XEEN.EXE", b"game");
    let before = snapshot(&original);

    make_zip(
        &zips.join("p.zip"),
        &[
            (
                "manifest.json",
                br#"[{"action":"replace","path":"PLAY.BAT","source":"files/PLAY.BAT"}]"#,
            ),
            ("files/PLAY.BAT", b"XEEN\r\nrem patched\r\n"),
        ],
    );
    make_zip(
        &zips.join("s.save"),
        &[
            (
                "meta.json",
                br#"{"version":"1","hack":"test-hack","settings":{}}"#,
            ),
            ("save/XEEN01.SAV", b"slot"),
        ],
    );

    let (got, recovered) = paths::prepare_session(
        &original,
        &work,
        Some(&zips.join("p.zip")),
        &zips.join("s.save"),
        meta(None),
    )
    .expect("前置序列");
    assert!(!recovered);
    assert_eq!(got.hack.as_deref(), Some("test-hack"));
    assert_eq!(
        read(&work, "PLAY.BAT"),
        b"XEEN\r\nrem patched\r\n",
        "补丁应覆盖工作副本"
    );
    assert_eq!(read(&work, "XEEN01.SAV"), b"slot", "存档应解包到工作副本");
    assert_eq!(read(&work, "XEEN.EXE"), b"game");

    assert_eq!(
        snapshot(&original),
        before,
        "原版目录必须字节级不变（补丁纪律）"
    );
}

/// PL003.7 验证：writeback 两轮打包替换，后写胜出、目标始终完好。
#[test]
fn 回写序列原子替换() {
    let work = temp_dir("wb-work");
    let out = temp_dir("wb-out");
    let save_path = out.join("s.save");
    let patterns = ["*.SAV".to_string()];

    write(&work, "XEEN01.SAV", b"round1");
    let n = paths::writeback(&work, &save_path, &patterns, &meta(None)).expect("第一轮回写");
    assert_eq!(n, 1);
    assert!(
        !out.join("s.save.tmp").exists(),
        "第一轮回写后不应残留临时文件"
    );
    assert!(save_path.is_file());

    write(&work, "XEEN01.SAV", b"round2");
    paths::writeback(&work, &save_path, &patterns, &meta(Some("test-hack"))).expect("第二轮回写");
    let restore = temp_dir("wb-restore");
    let got = save::unpack(&save_path, &restore).expect("回写结果应可解包");
    assert_eq!(got.hack.as_deref(), Some("test-hack"), "meta 应随回写更新");
    assert_eq!(read(&restore, "XEEN01.SAV"), b"round2", "后写胜出");
    assert!(!out.join("s.save.tmp").exists(), "回写后不应残留临时文件");
}
