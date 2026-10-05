# -*- coding: utf-8 -*-
"""XEEN 迷宫数据访问层：CUR/CC 容器解析、资源读取、区域名、主图清单与连通分量规划。

容器布局（TOC 与 .CC 同方案：rol2 + 滚动计数 0xAC/+0x67；数据区无 XOR；区域名
资源经 XOR 0x35 还原）与迷宫 892 字节布局依据 OpenXeen WOXMazeFile.loadMaze +
ScummVM mm/xeen 双源，字段级考证见地图档案 retro/map-atlas.md。
区域名两侧各自成表：暗面 DARKxxxx.TXT、云端 XEENxxxx.TXT（同存于 DARK.CC），
云端无 XEEN 名的图回落暗面表（map-atlas.md 勘误第七则）。
"""

import json
import os
import struct
from pathlib import Path

from til import EXTRACT, REPO_ROOT

CUR_PATHS = {
    "dark": os.path.join(REPO_ROOT, "retro", "XEEN", "DARK.CUR"),
    "xeen": os.path.join(REPO_ROOT, "retro", "XEEN", "XEEN.CUR"),
}
CC_PATHS = {
    "dark": os.path.join(REPO_ROOT, "retro", "XEEN", "DARK.CC"),
    "xeen": os.path.join(REPO_ROOT, "retro", "XEEN", "XEEN.CC"),
}
# (数据键, 文件名前缀, 调色板)：dark 侧文件 darkNNN.png（DARK.PAL），xeen 侧 cloudNNN.png（MM4.PAL）
SIDES = (("dark", "dark", "DARK.PAL"), ("xeen", "cloud", "MM4.PAL"))
TERRAIN_TYPES = ("TOWN", "CAVE", "TOWR", "CSTL", "DUNG", "SCFI")
DELTA = {"n": (0, -1), "e": (1, 0), "s": (0, 1), "w": (-1, 0)}
OUT_DIR = os.path.join(EXTRACT, "png", "maps")
_CUR_CACHE = {}


def cur_container(side):
    """按侧取 CUR 容器（inventory 与 read_maze 共用同一缓存入口，A003 #24）。

    同一命令内同侧容器只解析一次；返回 (数据体, 条目表, xor 标记)。
    """
    if side not in _CUR_CACHE:
        _CUR_CACHE[side] = parse_cc(CUR_PATHS[side], xor_data=False)
    return _CUR_CACHE[side]


def read_maze(side, mid):
    """读单张迷宫原始数据（892 字节；容器按侧缓存）。"""
    cur_data, cur_entries, cur_xor = cur_container(side)
    return read_res(cur_data, cur_entries, rot7_hash(maze_name(mid, "MAZE", "DAT")), cur_xor)


def rot7_hash(name):
    """CC 资源名 → 16 位 rot7 哈希（TOC 键）。"""
    h = 0
    for ch in name:
        h = ((h >> 7) | (h << 9)) & 0xFFFF
        h = (h + ord(ch)) & 0xFFFF
    return h


def parse_cc(path, xor_data):
    """解析 CC/CUR 容器 → (数据体, {资源哈希: (偏移, 长度)}, xor 标记)。"""
    data = Path(path).read_bytes()
    count = struct.unpack_from("<H", data, 0)[0]
    toc_raw = data[2: 2 + count * 8]
    out = bytearray()
    cnt = 0xAC
    for b in toc_raw:
        out.append(((((b << 2) & 0xFF) | (b >> 6)) + cnt) & 0xFF)
        cnt = (cnt + 0x67) & 0xFF
    entries = {}
    for i in range(count):
        chunk = out[i * 8: i * 8 + 8]
        rid = struct.unpack_from("<H", chunk, 0)[0]
        offset = chunk[2] | (chunk[3] << 8) | (chunk[4] << 16)
        length = struct.unpack_from("<H", chunk, 5)[0]
        entries[rid] = (offset, length)
    return data, entries, xor_data


def read_res(data, entries, rid, xor_data):
    """按资源哈希取资源体（区域名类资源需 XOR 0x35 还原）。"""
    off, length = entries[rid]
    raw = data[off: off + length]
    return bytes(b ^ 0x35 for b in raw) if xor_data else raw


def maze_name(mid, stem, ext):
    """地图号 → 资源文件名（4 位补零，>=100 收缩为 X 前缀 3 位）。"""
    s = ("00000" + str(mid))[-4:]
    if mid >= 100:
        s = "X" + s[1:]
    return f"{stem}{s}.{ext}"


def decode_name(raw):
    """区域名解码：Big5 优先（中文正文），latin-1 直返（单字节直映任意字节
    恒成功——保底即必达，无需第三级分支）。"""
    try:
        return raw.decode("big5").strip().strip("\x00").strip()
    except UnicodeDecodeError:
        return raw.decode("latin-1").strip().strip("\x00").strip()


def load_inventory():
    """全量主图清单：两侧 primary_maps（含侧属区域名）+ 变体文件数。"""
    name_data, name_entries, name_xor = parse_cc(CC_PATHS["dark"], xor_data=True)

    def area_name(mid, side):
        """侧属区域名：暗面查 DARK 表；云端查 XEEN 表，无名回落 DARK 表。"""
        stems = ("XEEN", "DARK") if side == "xeen" else ("DARK",)
        for stem in stems:
            rid = rot7_hash(maze_name(mid, stem, "TXT"))
            if rid in name_entries:
                name = decode_name(read_res(name_data, name_entries, rid, name_xor))
                if name:
                    return name
        return None

    inventory = {}
    for side, _prefix, _pal in SIDES:
        cur_data, cur_entries, cur_xor = cur_container(side)
        maps = []
        variants = 0
        for mid in range(1, 1000):
            rid = rot7_hash(maze_name(mid, "MAZE", "DAT"))
            if rid not in cur_entries:
                continue
            raw = read_res(cur_data, cur_entries, rid, cur_xor)
            if len(raw) != 892:
                variants += 1
                continue
            entry = {
                "file_id": mid,
                "primary": struct.unpack_from("<H", raw, 768)[0] == mid,
                "outdoors": bool(struct.unpack_from("<H", raw, 780)[0] & 0x8000),
                "dark": bool(struct.unpack_from("<H", raw, 780)[0] & 0x4000),
                "surr_nesw": list(struct.unpack_from("<4H", raw, 770)),
                "area_name": area_name(mid, side),
                "wall_kind": raw[826],
            }
            if entry["primary"]:
                maps.append(entry)
            else:
                variants += 1
        inventory[side] = {"primary_maps": maps, "variant_count": variants}
    return inventory


def plan_components(primary_maps):
    """户内主图沿四向邻接 DFS 规划连通分量（成员数 >= 2 的组才有拼合价值）。

    栈实现（list.pop）；连通分量划分与 placement 坐标只依赖父节点相对位置，
    与遍历序无关。返回 [{"anchor": 锚点图号, "placement": {图号: (列, 行)},
    "tiles": (宽, 高)}]，锚点 = 分量最小图号、位于 (0,0)；tiles 为图块数
    （×10×8 即像素尺寸）。
    """
    primary = {m["file_id"]: m for m in primary_maps if not m["outdoors"]}
    plans = []
    seen = set()
    for anchor in sorted(primary):
        if anchor in seen:
            continue
        comp = {anchor: (0, 0)}
        queue = [anchor]
        seen.add(anchor)
        while queue:
            cur = queue.pop()
            cx, cy = comp[cur]
            for dkey, (dx, dy) in DELTA.items():
                nb = primary[cur]["surr_nesw"]["nesw".index(dkey)]
                if nb in primary and nb not in seen:
                    seen.add(nb)
                    comp[nb] = (cx + dx, cy + dy)
                    queue.append(nb)
        if len(comp) < 2:
            continue
        xs = [x for x, _ in comp.values()]
        ys = [y for _, y in comp.values()]
        x0, y0 = min(xs), min(ys)
        plans.append(
            {
                "anchor": anchor,
                "placement": {mid: (gx - x0, gy - y0) for mid, (gx, gy) in comp.items()},
                "tiles": ((max(xs) - x0 + 1) * 16, (max(ys) - y0 + 1) * 16),
            }
        )
    return plans


def print_summary(inventory):
    """清单摘要（每侧总数/户外/黑暗/区域名覆盖），供人读核对。"""
    for side, _prefix, _pal in SIDES:
        maps = inventory[side]["primary_maps"]
        named = [m for m in maps if m["area_name"]]
        outdoors = [m["file_id"] for m in maps if m["outdoors"]]
        print(f"=== {side} ===")
        print(f"主图 {len(maps)} 张 + 状态副本 {inventory[side]['variant_count']} 个")
        print(f"户外 {len(outdoors)} 张: {outdoors}")
        print(f"区域名覆盖 {len(named)}/{len(maps)}（云端 XEEN 表优先，回落 DARK 表）")
        sample = [f"{m['file_id']}:{m['area_name']}" for m in maps[:8] if m["area_name"]]
        print(f"样例名: {sample}")
        print()
