# -*- coding: utf-8 -*-
"""地图清单 index.json 生成：全量主图条目 + 城市拼合分量段，输出到提取工作区。

- sides：每张主图的文件 / 侧属区域名 / 户外 / 黑暗 / 环境集 / 四向邻接
- city_maps：整城分层拼合分量清单——placement 由 xeendata.plan_components 同源
  规划，与 city_stitch 渲染完全一致，无需渲染即可重建清单
- 名字：侧属区域名（暗面 DARK 表；云端 XEEN 表优先、无名回落 DARK 表，
  map-atlas.md 勘误第七则）
- 输出为缩进 JSON；复制入 retro 后按仓库门禁过 prettier 归一（README 流程）
"""

import json
import os
import struct
from pathlib import Path

from til import guarded
from xeendata import OUT_DIR, SIDES, TERRAIN_TYPES, load_inventory, plan_components

NOTE_CITY = (
    "城镇/区域为多张 MAZE 沿 SurrMazes 拼合的连续空间（整城分层方程式定案 "
    "2026-09-14）；members = 图号 → 相对锚点的图块坐标（列,行）"
)
NOTE_FALLBACK = "name 为 null 的地图在区域名表中无名（多为未用或内部图），显示端可用地图号回落"
NOTE_CAVEAT = (
    "区域名两侧各自成表（暗面 DARKxxxx.TXT / 云端 XEENxxxx.TXT）；云端无 XEEN 名"
    "的图回落暗面表名字，可能与实际地点不符（map-atlas.md 勘误第七则）"
)
WORLDS = [
    {"file": "world-dark.png", "label": "暗面大陆（地面 A1–F4 缝合）",
     "grid": "6x4（列 A–F 西→东 × 行 1–4 北→南）"},
    {"file": "world-dark-sky.png", "label": "暗面天路（A1–F4 缝合）", "grid": "6x4"},
    {"file": "world-cloud.png", "label": "云端大陆（地面 A1–F4 缝合）", "grid": "6x4"},
]


def png_size(path):
    """读 PNG IHDR 实际像素尺寸（标准库实现；产物为真源，清单不手抄尺寸——A003 #27）。"""
    with open(path, "rb") as f:
        head = f.read(24)
    if head[:8] != b"\x89PNG\r\n\x1a\n" or head[12:16] != b"IHDR":
        raise ValueError(f"非 PNG 文件或头部异常: {path}")
    return list(struct.unpack_from(">II", head, 16))


def build_index(inventory):
    """组装完整 index 字典（sides + city_maps 全段落）。"""
    index = {
        "game": "Might and Magic: World of Xeen（汉化版）",
        "source": {
            "maze_data": "retro/XEEN/DARK.CUR 与 retro/XEEN/XEEN.CUR（TOC 同 CC 方案，数据区无 XOR）",
            "area_names": "DARK.CC 资源 DARKxxxx.TXT 与 XEENxxxx.TXT（Big5，两侧各自成表；云端无名回落 DARK 表）",
            "format_refs": "OpenXeen WOXMazeFile.loadMaze 与 ScummVM mm/xeen interface_minimap.cpp",
            "render_rules": "retro/map-atlas.md（户内四层方程式 / 户外 Tab 四层 / 世界网格语义）",
        },
        "coordinate_system": {
            "tiles": "16x16",
            "origin": "数据 (0,0) 在左下角，图上需垂直翻转",
            "pixel_mapping": "image_px = ( x * S_w, (15 - y) * S_h )",
            "tile_size": {
                "native": {"tile": [10, 8], "map": [160, 128]},
                "x6": {"tile": [60, 48], "map": [960, 768], "note": "最近邻整数放大"},
            },
        },
        "variants_note": "CUR 中 2xx/3xx 段文件为基图状态副本（内嵌 id 回指基图），未单独渲染",
        "name_fallback": NOTE_FALLBACK,
        "name_caveat": NOTE_CAVEAT,
        "terrain_types": list(TERRAIN_TYPES),
        "worlds": [
            {**w, "size": png_size(os.path.join(OUT_DIR, w["file"]))}
            for w in WORLDS
        ],
        "sides": {},
    }
    for side, prefix, _pal in SIDES:
        maps_out = []
        for m in inventory[side]["primary_maps"]:
            env = None
            if not m["outdoors"]:
                env = (TERRAIN_TYPES[m["wall_kind"]]
                       if m["wall_kind"] < len(TERRAIN_TYPES) else f"unknown({m['wall_kind']})")
            base = f"{prefix}{m['file_id']:03d}"
            n, e, s, w = m["surr_nesw"]
            maps_out.append(
                {
                    "id": m["file_id"],
                    "file": f"{base}.png",
                    "file_x6": f"{base}-x6.png",
                    "name": m["area_name"],
                    "outdoors": m["outdoors"],
                    "dark": m["dark"],
                    "env": env,
                    "surr": {"n": n, "e": e, "s": s, "w": w},
                }
            )
        index["sides"][side] = {
            "label": "暗面（Darkside of Xeen）" if side == "dark" else "云端（Clouds of Xeen）",
            "container": "DARK.CUR" if side == "dark" else "XEEN.CUR",
            "file_prefix": prefix,
            "map_count": len(maps_out),
            "maps": maps_out,
        }
    city_list = []
    for side, prefix, _pal in SIDES:
        names = {m["file_id"]: m["area_name"] for m in inventory[side]["primary_maps"]}
        for plan in plan_components(inventory[side]["primary_maps"]):
            anchor = plan["anchor"]
            city_list.append(
                {
                    "side": side,
                    "file": f"{prefix}{anchor:03d}-city.png",
                    "file_x6": f"{prefix}{anchor:03d}-city-x6.png",
                    "anchor": anchor,
                    "anchor_name": names.get(anchor),
                    "members": {
                        str(mid): {"x": x, "y": y}
                        for mid, (x, y) in sorted(plan["placement"].items())
                    },
                    "size": [plan["tiles"][0] * 10, plan["tiles"][1] * 8],
                }
            )
    index["city_maps"] = {"note": NOTE_CITY, "list": city_list}
    return index


def render_all(inventory=None):
    """生成 index.json 到提取工作区，返回输出路径。"""
    if inventory is None:
        inventory = load_inventory()
    out = guarded(os.path.join(OUT_DIR, "index.json"))
    Path(out).write_text(
        json.dumps(build_index(inventory), ensure_ascii=False, indent=1) + "\n",
        encoding="utf-8",
    )
    print(f"index.json -> {out}")
    return out
