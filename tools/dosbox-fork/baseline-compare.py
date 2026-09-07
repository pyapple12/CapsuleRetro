# -*- coding: utf-8 -*-
"""DosboxCapsule 基线判定·比对（FB1，升级 rebase 后必跑，见同目录 PATCHES.md）。

读四份 1MB 快照（.temp/fb1-snap-{s1a,s1b,s2a,s2b}.bin，s1=官方发布版参照、
s2=候选版），按 memory_map 档案 92 条目切片对照：自对照（每版 a vs b，间隔
2 秒）识别动态条目并剔除；交叉对照中静态条目字节级零偏差 → 基线通过
（退出码 0），否则列偏差表（退出码 1）。纯文件离线处理，无网络、无路径拼接。
"""

import json
import sys
from pathlib import Path

TEMP = Path(r"E:/GameMission/CapsuleRetro/.temp")
PROFILE = Path(r"E:/GameMission/CapsuleRetro/retro/memory_map.json")


def load(tag: str) -> bytes:
    return (TEMP / f"fb1-snap-{tag}.bin").read_bytes()


def slice_at(mem: bytes, seg: str, off: str, length: int) -> bytes:
    """按实模式 seg:off 计算线性地址并切片（seg/off 为档案中的 '0x..' 字符串）。

    坐标越界直接抛错：静默截断会让四快照同切空串恒等，条目形同未比对仍 GO。
    """
    addr = (int(seg, 16) << 4) + int(off, 16)
    if addr < 0 or addr + length > len(mem):
        raise ValueError(f"切片越界: {seg}:{off}+{length}B 超出内存 {len(mem)}B")
    return mem[addr : addr + length]


def main() -> int:
    s1a, s1b = load("s1a"), load("s1b")
    s2a, s2b = load("s2a"), load("s2b")
    for name, m in (("s1a", s1a), ("s1b", s1b), ("s2a", s2a), ("s2b", s2b)):
        if len(m) != 0x100000:
            print(f"快照 {name} 大小异常: {len(m)}B（应 1048576）")
            return 2

    entries = json.loads(PROFILE.read_text(encoding="utf-8"))["entries"]
    static, dynamic, unstable = [], [], []
    for i, e in enumerate(entries):
        v1a = slice_at(s1a, e["segment"], e["offset"], e["len"])
        v1b = slice_at(s1b, e["segment"], e["offset"], e["len"])
        v2a = slice_at(s2a, e["segment"], e["offset"], e["len"])
        v2b = slice_at(s2b, e["segment"], e["offset"], e["len"])
        s1_stable, s2_stable = v1a == v1b, v2a == v2b
        if s1_stable and s2_stable:
            static.append((i, v1a, v2a))
        elif s1_stable != s2_stable:
            # 单侧不稳 = 参照版稳定而候选版自身漂移（或反之）——rebase 引入的
            # 非确定性本身就是回归信号，不得静默归入 dynamic 剔除
            unstable.append((i, v1a, v1b, v2a, v2b))
        else:
            dynamic.append((i, v1a, v1b, v2a, v2b))

    mismatches = [(i, a, b) for i, a, b in static if a != b]
    print(
        f"条目总数 {len(entries)}：静态 {len(static)} / 动态 {len(dynamic)}"
        f" / 单侧不稳 {len(unstable)} / 静态偏差 {len(mismatches)}"
    )
    if dynamic:
        print("动态条目（双侧自对照均漂移，剔出比对集）:")
        for i, v1a, v1b, v2a, v2b in dynamic:
            print(
                f"  {entries[i]['name']}: s1a={v1a.hex()} s1b={v1b.hex()}"
                f" s2a={v2a.hex()} s2b={v2b.hex()}"
            )
    if unstable:
        print("单侧不稳（一侧自对照漂移——候选版与参照版稳定性不一致，判非 GO）:")
        for i, v1a, v1b, v2a, v2b in unstable:
            print(
                f"  {entries[i]['name']}: s1a={v1a.hex()} s1b={v1b.hex()}"
                f" s2a={v2a.hex()} s2b={v2b.hex()}"
            )
    for i, a, b in mismatches:
        print(
            f"  偏差: {entries[i]['name']} @ {entries[i]['segment']}:{entries[i]['offset']}"
            f" 官方={a.hex()} 构建={b.hex()}"
        )

    report = {
        "entries_total": len(entries),
        "static_count": len(static),
        "dynamic_count": len(dynamic),
        "dynamic_names": [entries[i]["name"] for i, *_ in dynamic],
        "unstable_count": len(unstable),
        "unstable_names": [entries[i]["name"] for i, *_ in unstable],
        "static_mismatch_count": len(mismatches),
        "static_mismatches": [
            {"name": entries[i]["name"], "official": a.hex(), "built": b.hex()}
            for i, a, b in mismatches
        ],
    }
    (TEMP / "fb1-report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    print(f"报告 -> {TEMP / 'fb1-report.json'}")
    # 判定三闸：静态零偏差；无单侧不稳（假 GO 路径一）；静态集不塌缩
    # （低于总数一半说明比对内容趋近空，空真 GO 路径二）
    if mismatches or unstable:
        return 1
    if len(static) * 2 < len(entries):
        print("静态条目不足总数一半，比对集塌缩，判非 GO")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
