# -*- coding: utf-8 -*-
"""城镇/区域多图拼合渲染：整城分层方程式（与官方整城图 1:1 定案，2026-09-14）。

- 分量规划：xeendata.plan_components（户内主图四向邻接 DFS 连通分量，成员 >= 2 的组）
- 资源上下文整组归锚点：TIL 环境集（wallKind）与 surfaceLUT 都取锚点图——
  ScummVM XeenMap 场景资源按 _mazeData[0] 加载（scummvm-map.cpp:954），
  塔层等 TOWN 成员随锚点环境集渲染（map-atlas.md 勘误第七/十三则）
- 每层先拼成整城完整画布（跨分图边界的点缀溢出不被裁掉），再逐层叠加：
  L1 地面 frame 0 全格 → L2 点缀（锚点 LUT，+5,+5）→ L3 墙条 → L4 空置格 frame 1 盖板
- 数据 (0,0) 左下 → 城市行 = 分量行 × 16 + (15 - 数据行)，与单图同一翻转约定
- 命名：{prefix}{anchor:03d}-city.png / -x6.png（锚点 = 分量最小图号）
"""

import os

from til import write_png
from xeendata import OUT_DIR, SIDES, load_inventory, plan_components, read_maze
from render_indoor import WALL_A, VOID, TW, TH, make_blit, make_frames_loader
from render_outdoor import scale_x6


def render_all(inventory=None):
    """全部拼合分量渲染；无返回值——index.json 的 city 段由 gen_index 依同一
    plan_components 纯函数重算（渲染与清单单一来源，A003 #18）。"""
    if inventory is None:
        inventory = load_inventory()
    os.makedirs(OUT_DIR, exist_ok=True)
    for side, prefix, pal_name in SIDES:
        frames_of = make_frames_loader(side, pal_name)
        for plan in plan_components(inventory[side]["primary_maps"]):
            anchor = plan["anchor"]
            anchor_raw = read_maze(side, anchor)
            anchor_lut = anchor_raw[798:814]
            frames = frames_of(anchor_raw[826])
            if frames is None:
                print(f"  [跳过] {prefix}{anchor:03d}: 缺锚点环境集 TIL")
                continue
            W = plan["tiles"][0] * TW
            H = plan["tiles"][1] * TH
            layers = [bytearray(W * H * 4) for _ in range(4)]
            blit = make_blit(frames, W, H)

            for mid in sorted(plan["placement"]):
                gx, gy = plan["placement"][mid]
                raw = read_maze(side, mid)
                bx, by = gx * 16, gy * 16
                for ty in range(16):
                    for tx in range(16):
                        t = ty * 16 + tx
                        v = raw[t * 2] | (raw[t * 2 + 1] << 8)
                        px = (bx + tx) * TW
                        py = (by + 15 - ty) * TH
                        # L1 地面：全格 frame 0
                        blit(layers[0], 0, px, py)
                        if v == VOID:
                            # L4 空置盖板：数据在层、最后叠
                            blit(layers[3], 1, px, py)
                            continue
                        # L2 点缀：锚点 LUT → +36，(+5,+5)；LUT=0 → 帧 36 水面
                        surf = raw[512 + t] & 7
                        if surf:
                            blit(layers[1], anchor_lut[surf] + 36,
                                 px + TW // 2, py + TH // 2 + 1)
                        # L3 墙条：西 n3 → WALL_A、北 n0 → WALL_A+1
                        n0, n3 = v & 0xF, (v >> 12) & 0xF
                        if n3 and n3 in WALL_A:
                            blit(layers[2], WALL_A[n3], px, py)
                        if n0 and n0 in WALL_A:
                            blit(layers[2], WALL_A[n0] + 1, px, py)
            # 逐层叠加
            canvas = bytearray(W * H * 4)
            for layer in layers:
                for i in range(0, len(canvas), 4):
                    if layer[i + 3]:
                        canvas[i: i + 4] = layer[i: i + 4]
            base = f"{prefix}{anchor:03d}-city"
            write_png(os.path.join(OUT_DIR, base + ".png"), W, H, bytes(canvas))
            w6, h6, rgba6 = scale_x6(W, H, bytes(canvas))
            write_png(os.path.join(OUT_DIR, base + "-x6.png"), w6, h6, rgba6)
            print(f"{side} 分量锚点 {anchor}: {len(plan['placement'])} 图 -> {base}.png ({W}x{H})")
