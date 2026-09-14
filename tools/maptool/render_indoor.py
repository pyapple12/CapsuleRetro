# -*- coding: utf-8 -*-
"""户内迷宫渲染器：四层方程式（与官方全城图 1:1 定案，2026-09-14 用户验收）。

规则细节与实证记录见地图档案 retro/map-atlas.md「户内渲染规则」；要点：
- 地面：全部格子铺 frame 0（空置格由第 4 遍盖板最终覆盖）
- 点缀：cellFlag 低 3 位 = surfaceId，非 0 即画 surfaceLUT[surf]+36 于 (+5,+5)；
  LUT 映 0 → 帧号 36 = 水面（ScummVM interface_minimap.cpp:151 同源条件）
- 墙：nibble3（西）查 WALL_A、nibble0（北）查 WALL_A+1；作用于原始 nibble；
  空置格跳过
- 空置盖板：wallData == 0x8888（INVALID_CELL）最后整格画 frame 1
- 画序 = 地面 → 点缀 → 墙 → 盖板（层级错画即现门贴压墙 / 地面盖墙缺陷）
- 城镇拼合（city_stitch）：资源上下文（TIL 环境集 + surfaceLUT）整组取锚点图
- 数据 (0,0) 左下 → 画布垂直翻转
"""

import os

from til import EXTRACT, decode_sprite, load_palette, write_png
from render_outdoor import scale_x6
from xeendata import (OUT_DIR, SIDES, TERRAIN_TYPES, load_inventory,  # noqa: F401
                      read_maze)

# 西墙（nibble3）帧表；12 = 动画下水道栅栏（取静态第 0 帧 → 4）；北墙 = 同表 +1
WALL_A = {1: 18, 2: 34, 3: 22, 4: 16, 5: 2, 6: 30, 7: 32, 8: 2, 9: 20, 10: 28,
          11: 14, 12: 4, 13: 16, 14: 24, 15: 26}
VOID = 0x8888  # ScummVM INVALID_CELL：全 nibble=8 的实心格
TW, TH = 10, 8


def render_indoor(raw, frames, lut=None):
    """户内单图渲染。lut = 16 字节 surfaceLUT 覆盖（城镇拼合传锚点 LUT），缺省用自身。"""
    if lut is None:
        lut = raw[798:814]
    W = 16 * TW
    H = 16 * TH
    canvas = bytearray(W * H * 4)

    def blit(fi, px, py):
        # 像素自带帧内绝对偏移，只按画布边界裁剪；透明像素不覆盖（与引擎绘制语义一致）
        if fi >= len(frames):
            return
        _, _, pixels = frames[fi]
        for (x, y), rgba in pixels.items():
            if not rgba[3]:
                continue
            xx = px + x
            yy = py + y
            if 0 <= xx < W and 0 <= yy < H:
                i = (yy * W + xx) * 4
                canvas[i: i + 4] = bytes(rgba)

    # 遍 1：地面——全格 frame 0 铺满（空置格由遍 4 盖板最终覆盖）
    for t in range(256):
        blit(0, (t % 16) * TW, (15 - t // 16) * TH)
    # 遍 2：地表点缀（surfaceId 非 0 即画，LUT=0 → 帧 36 水面；空置格跳过；+5,+5）
    for t in range(256):
        v = raw[t * 2] | (raw[t * 2 + 1] << 8)
        if v == VOID:
            continue
        surf = raw[512 + t] & 7
        if surf:
            blit(lut[surf] + 36, (t % 16) * TW + TW // 2, (15 - t // 16) * TH + TH // 2 + 1)
    # 遍 3：墙（西 = n3 → WALL_A；北 = n0 → WALL_A+1 奇数帧；空置格无墙条）
    for t in range(256):
        v = raw[t * 2] | (raw[t * 2 + 1] << 8)
        if v == VOID:
            continue
        n0, n3 = v & 0xF, (v >> 12) & 0xF
        px = (t % 16) * TW
        py = (15 - t // 16) * TH
        if n3 and n3 in WALL_A:
            blit(WALL_A[n3], px, py)
        if n0 and n0 in WALL_A:
            blit(WALL_A[n0] + 1, px, py)
    # 遍 4：空置实心格 frame 1 盖板——最后画，盖掉点缀/墙条的越界部分
    for t in range(256):
        v = raw[t * 2] | (raw[t * 2 + 1] << 8)
        if v == VOID:
            blit(1, (t % 16) * TW, (15 - t // 16) * TH)
    return W, H, bytes(canvas)


def make_frames_loader(side, pal_name):
    """按侧构建帧表加载器：wallKind(int) → TIL 帧表（环境集缓存；缺文件返回 None）。"""
    pal = load_palette(os.path.join(EXTRACT, side, pal_name))
    frames_cache = {}

    def frames_of(wall_kind):
        tt = TERRAIN_TYPES[wall_kind] if wall_kind < len(TERRAIN_TYPES) else None
        if tt is None:
            return None
        if tt not in frames_cache:
            path = os.path.join(EXTRACT, side, tt + ".TIL")
            if not os.path.isfile(path):
                frames_cache[tt] = None
            else:
                with open(path, "rb") as f:
                    frames_cache[tt] = decode_sprite(f.read(), pal)
        return frames_cache[tt]

    return frames_of


def render_all(inventory=None):
    """户内主图全量渲染（原生 160×128 + ×6），单图保留自身 LUT 与自身环境集。"""
    if inventory is None:
        inventory = load_inventory()
    for side, prefix, pal_name in SIDES:
        frames_of = make_frames_loader(side, pal_name)
        out_side = os.path.join(OUT_DIR, prefix)
        os.makedirs(out_side, exist_ok=True)
        n = 0
        for m in inventory[side]["primary_maps"]:
            if m["outdoors"]:
                continue
            mid = m["file_id"]
            raw = read_maze(side, mid)
            frames = frames_of(raw[826])
            if frames is None:
                print(f"  [跳过] {prefix}{mid:03d}: 缺 {TERRAIN_TYPES[raw[826]]}.TIL")
                continue
            w, h, rgba = render_indoor(raw, frames)
            base = f"{prefix}{mid:03d}"
            write_png(os.path.join(out_side, base + ".png"), w, h, rgba)
            w6, h6, rgba6 = scale_x6(w, h, rgba)
            write_png(os.path.join(out_side, base + "-x6.png"), w6, h6, rgba6)
            n += 1
        print(f"{side}: 户内渲染 {n} 张（含 ×6 双份）→ {out_side}")
