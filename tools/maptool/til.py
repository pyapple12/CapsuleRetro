# -*- coding: utf-8 -*-
"""XEEN 资源解码库：调色板加载、CC 精灵帧解码、PNG 写出。

格式依据 Xeen Wiki（CC / 精灵文件格式）与 OpenXeen（WOXSpriteFile / WOXPallet）
双源，解码逻辑与地图档案（retro/map-atlas.md）所用管线一致。
EXTRACT = .xeen-extract 提取工作区（gitignore）；所有写操作经路径守卫限制在该工作区内。
"""

import os
import struct
import zlib
from pathlib import Path

# 仓库根自脚本位置派生（本文件位于 <仓库根>/tools/maptool/ 下，上溯两级）——
# 不得写死机器绝对路径，否则换克隆位置整条管线失效（A003 #2）
REPO_ROOT = os.path.realpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
EXTRACT = os.path.realpath(os.path.join(REPO_ROOT, ".xeen-extract"))
ALLOWED = EXTRACT + os.sep


def guarded(path):
    """路径守卫：realpath 规范化并校验真实路径落在提取工作区内，越界即抛错。"""
    real = os.path.realpath(path)
    if not (real == EXTRACT or real.startswith(ALLOWED)):
        raise ValueError(f"path escapes workspace: {real}")
    return real


PATTERN_STEPS = (0, 1, 1, 1, 2, 2, 3, 3, 0, -1, -1, -1, -2, -2, -3, -3)


def u16(data, off):
    return data[off] | (data[off + 1] << 8)


def write_png(path, w, h, rgba):
    """RGBA 原始像素写 8-bit RGBA PNG（zlib 压缩，确定性输出便于产物比对）。"""

    def chunk(tag, payload):
        return (struct.pack(">I", len(payload)) + tag + payload
                + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))

    raw = b"".join(
        b"\x00" + rgba[y * w * 4:(y + 1) * w * 4] for y in range(h)
    )
    blob = b"\x89PNG\r\n\x1a\n"
    blob += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    blob += chunk(b"IDAT", zlib.compress(raw, 6))
    blob += chunk(b"IEND", b"")
    Path(guarded(path)).write_bytes(blob)


def load_palette(path):
    """VGA 256 色调色板（768 字节，6-bit 分量左移补满 8-bit）。"""
    data = Path(guarded(path)).read_bytes()
    if len(data) < 768:
        raise ValueError(f"palette too small: {len(data)}")
    return tuple(
        (data[i * 3] << 2, data[i * 3 + 1] << 2, data[i * 3 + 2] << 2)
        for i in range(256)
    )


def render_cell(data, dp, pal, transparent, pixels):
    """解码单个精灵 cell 到 pixels 字典（坐标含帧内绝对偏移；RLE 变体指令集）。"""
    x_off = u16(data, dp)
    y_off = u16(data, dp + 4)
    height = u16(data, dp + 6)
    dp += 8

    def put(x, y, color):
        r, g, b = pal[color & 0xFF]
        alpha = 0 if color == transparent else 255
        pixels[(x, y)] = (r, g, b, alpha)

    y = y_off
    byte_count = 0
    while y < y_off + height:
        line_len = data[dp]
        dp += 1
        if line_len > 0:
            x = data[dp] + x_off
            dp += 1
            byte_count = 1
            while byte_count < line_len:
                opcode = data[dp]
                dp += 1
                byte_count += 1
                ln = opcode & 0x1F
                cmd = opcode >> 5
                if cmd in (0, 1):
                    for _ in range(opcode + 1):
                        put(x, y, data[dp])
                        dp += 1
                        byte_count += 1
                        x += 1
                elif cmd == 2:
                    color = data[dp]
                    dp += 1
                    byte_count += 1
                    for _ in range(ln + 3):
                        put(x, y, color)
                        x += 1
                elif cmd == 3:
                    back = u16(data, dp)
                    dp += 2
                    byte_count += 2
                    for i in range(ln + 4):
                        put(x, y, data[dp - back + i])
                        x += 1
                elif cmd == 4:
                    c1 = data[dp]
                    c2 = data[dp + 1]
                    dp += 2
                    byte_count += 2
                    for _ in range(ln + 2):
                        put(x, y, c1)
                        put(x + 1, y, c2)
                        x += 2
                elif cmd == 5:
                    x += ln + 1
                else:
                    ln = opcode & 0x07
                    base = (opcode >> 2) & 0x0E
                    color = data[dp]
                    dp += 1
                    byte_count += 1
                    for i in range(ln + 3):
                        put(x, y, color)
                        x += 1
                        color += PATTERN_STEPS[(base + i % 2) % 16]
        else:
            y += data[dp]
            dp += 1
        y += 1


def decode_sprite(data, pal):
    """解码精灵文件全部帧 → [(w, h, {(x,y): rgba}), ...]；一帧可由多个 cell 拼合。"""
    num_frames = u16(data, 0)
    p = 2
    cells = {}
    frame_cells = []
    for _ in range(num_frames):
        c1 = u16(data, p)
        c2 = u16(data, p + 2)
        p += 4
        lst = []
        for off in (c1, c2):
            if off == 0:
                continue
            if off not in cells:
                w = u16(data, off + 2)
                h = u16(data, off + 6)
                if w >= 1024 or h >= 1024:
                    raise ValueError(f"bad cell dims {w}x{h}")
                cells[off] = (u16(data, off), u16(data, off + 4), w, h)
            lst.append(off)
        frame_cells.append(lst)

    results = []
    for offs in frame_cells:
        if not offs:
            results.append((1, 1, {(0, 0): (0, 0, 0, 0)}))
            continue
        w = max(cells[o][0] + cells[o][2] for o in offs)
        h = max(cells[o][1] + cells[o][3] for o in offs)
        pixels = {}
        for off in offs:
            render_cell(data, off, pal, 0, pixels)
        results.append((w, h, pixels))
    return results
