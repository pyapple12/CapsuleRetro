//! PL004.4/5 集成测试：exe 尾部 payload（zip + trailer）的拼装、解析与 extract-once 释放。
//!
//! 格式定案见 z.plan 附录 PL004「数据格式」：trailer 28 字节
//! （magic "CRPAYLD1" + format u32 LE + payload_len u64 LE + payload_offset u64 LE），
//! payload 布局镜像仓库（retro/ 前缀），`payload.json` 兼作 extract-once 版本标记。

use std::io::{Seek as _, Write as _};
use std::path::{Path, PathBuf};

use capsule_core::payload::{self, PayloadError, PayloadReader};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// 独立临时目录（按 tag 隔离，先清残留）。
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cr-pl004-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

/// 构造 payload zip（条目名 → 字节），返回 zip 文件路径。
fn make_payload_zip(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join("payload.zip");
    let file = std::fs::File::create(&path).expect("创建 payload zip");
    let mut writer = ZipWriter::new(file);
    for (name, bytes) in entries {
        writer
            .start_file(name.to_string(), SimpleFileOptions::default())
            .expect("写入 zip 条目");
        writer.write_all(bytes).expect("写入条目字节");
    }
    writer.finish().expect("收尾 zip");
    path
}

/// 构造伪 launcher exe（内容无关紧要，只要是个文件）。
fn make_dummy_exe(dir: &Path) -> PathBuf {
    let path = dir.join("launcher.exe");
    std::fs::write(&path, b"MZFAKE-LAUNCHER-BYTES").expect("写伪 exe");
    path
}

#[test]
fn payload拼装解析往返一致() {
    let dir = temp_dir("roundtrip");
    let exe = make_dummy_exe(&dir);
    let payload = make_payload_zip(
        &dir,
        &[
            ("payload.json", br#"{"build":"0.1.0+42"}"#),
            ("retro/game.json", br#"{"name":"XEEN"}"#),
            ("retro/XEEN/XEEN.EXE", b"GAME-EXE"),
            ("dosbox/dosbox.exe", b"DOSBOX-EXE"),
        ],
    );

    payload::append_payload(&exe, &payload).expect("拼装应成功");
    let mut reader = PayloadReader::open(&exe)
        .expect("解析不应报错")
        .expect("应识别为打包形态");

    assert_eq!(reader.build_id(), "0.1.0+42");
    assert_eq!(
        reader.entry_names(),
        vec![
            "dosbox/dosbox.exe",
            "payload.json",
            "retro/XEEN/XEEN.EXE",
            "retro/game.json",
        ]
    );
    assert_eq!(
        reader
            .read_entry_to_string("retro/game.json")
            .expect("读内嵌条目"),
        r#"{"name":"XEEN"}"#
    );
}

#[test]
fn 无trailer识别为非打包形态() {
    let dir = temp_dir("none");
    let exe = make_dummy_exe(&dir);
    let opened = PayloadReader::open(&exe).expect("不应报错");
    assert!(opened.is_none(), "裸 exe 应识别为非打包形态");
}

#[test]
fn trailer字段非法报错() {
    let dir = temp_dir("badtrailer");
    let exe = make_dummy_exe(&dir);
    let payload = make_payload_zip(
        &dir,
        &[("payload.json", br#"{"build":"b"}"#), ("retro/a.txt", b"A")],
    );
    payload::append_payload(&exe, &payload).expect("拼装应成功");
    let size = std::fs::metadata(&exe).expect("读大小").len();

    // 负例 1：格式版本未知 → BadTrailer
    let tampered = dir.join("bad-format.exe");
    std::fs::copy(&exe, &tampered).expect("复制");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(&tampered)
        .expect("打开");
    f.seek(std::io::SeekFrom::Start(size - 28 + 8))
        .expect("seek");
    f.write_all(&2u32.to_le_bytes()).expect("篡改 format");
    drop(f);
    match PayloadReader::open(&tampered) {
        Err(PayloadError::BadTrailer { .. }) => {}
        other => panic!("格式版本 2 应报 BadTrailer，实际 {other:?}"),
    }

    // 负例 2：payload_len 篡改 → 与文件大小矛盾 → BadTrailer
    let tampered = dir.join("bad-len.exe");
    std::fs::copy(&exe, &tampered).expect("复制");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(&tampered)
        .expect("打开");
    f.seek(std::io::SeekFrom::Start(size - 28 + 12))
        .expect("seek");
    f.write_all(&9999u64.to_le_bytes()).expect("篡改 len");
    drop(f);
    match PayloadReader::open(&tampered) {
        Err(PayloadError::BadTrailer { .. }) => {}
        other => panic!("len 矛盾应报 BadTrailer，实际 {other:?}"),
    }
}

#[test]
fn 重复拼装拒绝() {
    let dir = temp_dir("twice");
    let exe = make_dummy_exe(&dir);
    let payload = make_payload_zip(&dir, &[("payload.json", br#"{"build":"b"}"#)]);
    payload::append_payload(&exe, &payload).expect("首次拼装应成功");
    match payload::append_payload(&exe, &payload) {
        Err(PayloadError::AlreadyPacked(_)) => {}
        other => panic!("重复拼装应报 AlreadyPacked，实际 {other:?}"),
    }
}

#[test]
fn extract_all释放产物齐全() {
    let dir = temp_dir("extract");
    let exe = make_dummy_exe(&dir);
    let payload = make_payload_zip(
        &dir,
        &[
            ("payload.json", br#"{"build":"b7"}"#),
            ("retro/game.json", b"{}"),
            ("retro/XEEN/SUB/DEEP.TXT", b"deep"),
        ],
    );
    payload::append_payload(&exe, &payload).expect("拼装应成功");
    let mut reader = PayloadReader::open(&exe).expect("解析").expect("Some");
    let dest = dir.join("resources");
    reader.extract_all(&dest).expect("释放应成功");
    assert_eq!(
        std::fs::read_to_string(dest.join("retro/XEEN/SUB/DEEP.TXT")).expect("深文件"),
        "deep"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("payload.json")).expect("标记文件"),
        r#"{"build":"b7"}"#
    );
}

#[test]
fn extract_all越界条目拒绝() {
    let dir = temp_dir("unsafe");
    let exe = make_dummy_exe(&dir);
    let payload = make_payload_zip(
        &dir,
        &[
            ("payload.json", br#"{"build":"b"}"#),
            ("retro/../evil.txt", b"evil"),
        ],
    );
    payload::append_payload(&exe, &payload).expect("拼装应成功");
    let mut reader = PayloadReader::open(&exe).expect("解析").expect("Some");
    match reader.extract_all(&dir.join("resources")) {
        Err(PayloadError::UnsafeEntry(name)) => assert!(name.contains("evil"), "{name}"),
        other => panic!("路径穿越条目应报 UnsafeEntry，实际 {other:?}"),
    }
}

#[test]
fn extract_once三态与标记损坏重释放() {
    let dir = temp_dir("once");
    let exe = make_dummy_exe(&dir);
    let payload = make_payload_zip(
        &dir,
        &[
            ("payload.json", br#"{"build":"v1"}"#),
            ("retro/game.json", b"{}"),
        ],
    );
    payload::append_payload(&exe, &payload).expect("拼装应成功");
    let mut reader = PayloadReader::open(&exe).expect("解析").expect("Some");
    let dest = dir.join("resources");

    // 首次：释放（true）
    assert!(reader.extract_once(&dest).expect("首次"));
    // 二次：标记相符跳过（false）
    assert!(!reader.extract_once(&dest).expect("二次"));
    // 标记损坏 + 目标内残留脏文件 → 清空重释放（true），脏文件消失、标记恢复
    std::fs::write(dest.join("payload.json"), b"not-json").expect("写坏标记");
    std::fs::write(dest.join("stale.txt"), b"stale").expect("写残留");
    assert!(reader.extract_once(&dest).expect("标记损坏应重释放"));
    assert!(!dest.join("stale.txt").exists(), "残留应随清空移除");
    assert!(dest.join("retro/game.json").is_file(), "资源应恢复");
}
