# -*- coding: utf-8 -*-
"""世界整图缝合：户外主图按区域名网格拼接（原生 + ×6）。

网格语义 = 列字母 A–F（西→东）× 行数字 1–4（北→南），世界 6 列 × 4 行；
判定依据：A1 邻接 N=0/W=0（西北角）、A4 S=0（南缘）、F 列全 E=0（东缘），
并经官方 in-game world map 参考图比对证实（map-atlas.md 勘误第八则）。
天路（名字含"天路"前缀）与地面分组各自缝合。
"""

import os
import re

from til import write_png
from xeendata import OUT_DIR, SIDES, load_inventory, read_maze
from render_outdoor import load_frames, render_map, scale_x6


def grid_of(name):
    """区域名 → (列字母, 行数字)；非网格图返回 None。"""
    m = re.match(r"(?:天路 )?([A-F])([1-4])", name or "")
    return (m.group(1), int(m.group(2))) if m else None


def stitch(maps_by_id, frames, ids_by_cell):
    """按 (列字母, 行数字) 键缝合世界图；缺位网格显式报错，拒绝产出缺块图。"""
    tw = max(w for w, _, _ in frames)
    th = max(h for _, h, _ in frames)
    cols = sorted({letter for letter, _ in ids_by_cell})
    rows = sorted({number for _, number in ids_by_cell})
    for letter in cols:
        for number in rows:
            if (letter, number) not in ids_by_cell:
                raise ValueError(
                    f"world 网格缺位 {letter}{number}——区域名解析缺图，"
                    "请核查区域名表与户外图覆盖后再缝合"
                )
    W = len(cols) * 16 * tw
    H = len(rows) * 16 * th
    canvas = bytearray(W * H * 4)
    for (letter, number), mid in ids_by_cell.items():
        raw = maps_by_id[mid]
        w, h, rgba = render_map(raw, frames)
        ox = (ord(letter) - ord("A")) * 16 * tw
        oy = (number - 1) * 16 * th
        for y in range(h):
            di = (oy + y) * W + ox
            si = y * w * 4
            canvas[di * 4: di * 4 + w * 4] = rgba[si: si + w * 4]
    return W, H, bytes(canvas)


def render_all(inventory=None):
    """两侧世界整图缝合（地面 + 天路，有则出）。"""
    if inventory is None:
        inventory = load_inventory()
    os.makedirs(OUT_DIR, exist_ok=True)
    for side, prefix, pal_name in SIDES:
        frames = load_frames(side, pal_name)
        maps_by_id = {}
        grids = {}
        for m in inventory[side]["primary_maps"]:
            if not m["outdoors"]:
                continue
            maps_by_id[m["file_id"]] = read_maze(side, m["file_id"])
            cell = grid_of(m["area_name"])
            if cell:
                key = "sky" if "天路" in (m["area_name"] or "") else "world"
                grids.setdefault(key, {})[cell] = m["file_id"]
        for key, label in (("world", "大陆"), ("sky", "天路")):
            if key not in grids:
                continue
            w, h, rgba = stitch(maps_by_id, frames, grids[key])
            base = f"world-{prefix}" if key == "world" else f"world-{prefix}-sky"
            write_png(os.path.join(OUT_DIR, base + ".png"), w, h, rgba)
            w6, h6, rgba6 = scale_x6(w, h, rgba)
            write_png(os.path.join(OUT_DIR, base + "-x6.png"), w6, h6, rgba6)
            print(f"{side} {label}: {base}.png ({w}x{h}) + -x6 ({w6}x{h6})")
