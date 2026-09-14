# -*- coding: utf-8 -*-
"""户外地图渲染器：游戏 Tab 小地图同款四层算法（原生 + ×6）。

- 每格依次叠画（0 = 跳过）：surfaceLUT[n0] → wallLUT[n1]+16 → n2+32 → n3+48；
  帧来自 OUTDOOR.TIL（主体 10×8）
- 数据 (0,0) 左下 → 画布垂直翻转
- 世界整图缝合（区域名网格）在 world_stitch.py
"""

import os

from til import EXTRACT, decode_sprite, load_palette, write_png
from xeendata import OUT_DIR, SIDES, load_inventory, read_maze

SCALE = 6


def load_frames(side, pal_name):
    """加载户外 TIL 帧表（OUTDOOR.TIL + 侧调色板）。"""
    pal = load_palette(os.path.join(EXTRACT, side, pal_name))
    with open(os.path.join(EXTRACT, side, "OUTDOOR.TIL"), "rb") as f:
        return decode_sprite(f.read(), pal)


def render_map(raw, frames):
    """四层算法渲染 16×16 格 → (W, H, RGBA)。"""
    tw = max(w for w, _, _ in frames)
    th = max(h for _, h, _ in frames)
    wall_lut = raw[782:798]
    surf_lut = raw[798:814]
    W = 16 * tw
    H = 16 * th
    canvas = bytearray(W * H * 4)
    for ty in range(16):
        for tx in range(16):
            t = ty * 16 + tx
            v = raw[t * 2] | (raw[t * 2 + 1] << 8)
            n0, n1, n2, n3 = v & 0xF, (v >> 4) & 0xF, (v >> 8) & 0xF, (v >> 12) & 0xF
            layer_frames = [surf_lut[n0]]
            if n1:
                layer_frames.append(wall_lut[n1] + 16)
            if n2:
                layer_frames.append(n2 + 32)
            if n3:
                layer_frames.append(n3 + 48)
            for fi in layer_frames:
                if fi >= len(frames):
                    continue
                fw, fh, pixels = frames[fi]
                ox = tx * tw
                oy = (15 - ty) * th
                for (x, y), rgba in pixels.items():
                    if 0 <= x < fw and 0 <= y < fh and rgba[3]:
                        px = (oy + y) * W + ox + x
                        canvas[px * 4: px * 4 + 4] = bytes(rgba)
    return W, H, bytes(canvas)


def scale_x6(w, h, rgba):
    """最近邻整数放大（逐像素 ×6）。"""
    W, H = w * SCALE, h * SCALE
    out = bytearray(W * H * 4)
    for y in range(H):
        sy = y // SCALE
        row = rgba[sy * w * 4: (sy + 1) * w * 4]
        di = y * W * 4
        for x in range(W):
            sx = x // SCALE
            out[di + x * 4: di + x * 4 + 4] = row[sx * 4: sx * 4 + 4]
    return W, H, bytes(out)


def render_all(inventory=None):
    """户外主图全量渲染（原生 + ×6 双份）。"""
    if inventory is None:
        inventory = load_inventory()
    for side, prefix, pal_name in SIDES:
        frames = load_frames(side, pal_name)
        out_side = os.path.join(OUT_DIR, prefix)
        os.makedirs(out_side, exist_ok=True)
        n = 0
        for m in inventory[side]["primary_maps"]:
            if not m["outdoors"]:
                continue
            raw = read_maze(side, m["file_id"])
            w, h, rgba = render_map(raw, frames)
            base = f"{prefix}{m['file_id']:03d}"
            write_png(os.path.join(out_side, base + ".png"), w, h, rgba)
            w6, h6, rgba6 = scale_x6(w, h, rgba)
            write_png(os.path.join(out_side, base + "-x6.png"), w6, h6, rgba6)
            n += 1
        print(f"{side}: 户外渲染 {n} 张（含 ×6 双份）→ {out_side}")
