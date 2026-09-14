# -*- coding: utf-8 -*-
"""XEEN 地图资产管线单一入口：渲染、拼合、清单生成与发布。

子命令：
  inventory  打印主图清单摘要（每侧总数/户外/区域名覆盖）
  singles    户内 + 户外单图全量渲染（原生 + ×6）
  cities     城镇/区域整城分层拼合
  world      世界整图缝合（地面 + 天路）
  index      生成地图清单 index.json（含城市段）
  all        singles → cities → world → index 顺序执行
  publish    将提取工作区产物复制入 retro/artwork/maps（入库由 git 管理）
"""

import argparse
import os
import shutil

import city_stitch
import gen_index
import render_indoor
import render_outdoor
import world_stitch
import xeendata
from til import REPO_ROOT
from xeendata import OUT_DIR, load_inventory

RETRO_MAPS = os.path.join(REPO_ROOT, "retro", "artwork", "maps")


def cmd_publish():
    """发布：单图目录、城市拼合图、世界图与 index.json 复制入 retro（仅白名单）。"""
    copied = 0
    for sub in ("cloud", "dark"):
        src = os.path.join(OUT_DIR, sub)
        dst = os.path.join(RETRO_MAPS, sub)
        os.makedirs(dst, exist_ok=True)
        for name in sorted(os.listdir(src)):
            if name.endswith(".png"):
                shutil.copy2(os.path.join(src, name), os.path.join(dst, name))
                copied += 1
    for name in sorted(os.listdir(OUT_DIR)):
        if (
            name.endswith("-city.png")
            or name.endswith("-city-x6.png")
            or name.startswith("world-")
            or name == "index.json"
        ):
            shutil.copy2(os.path.join(OUT_DIR, name), os.path.join(RETRO_MAPS, name))
            copied += 1
    print(f"published {copied} files -> {RETRO_MAPS}")
    print("注意：retro 侧 index.json 按仓库门禁过 prettier 归一（npx prettier --write）")


def main():
    ap = argparse.ArgumentParser(description="XEEN 地图资产管线（渲染/拼合/清单/发布）")
    ap.add_argument(
        "command",
        choices=["inventory", "singles", "cities", "world", "index", "all", "publish"],
    )
    args = ap.parse_args()
    if args.command == "inventory":
        xeendata.print_summary(load_inventory())
    elif args.command == "singles":
        inv = load_inventory()
        render_indoor.render_all(inv)
        render_outdoor.render_all(inv)
    elif args.command == "cities":
        city_stitch.render_all(load_inventory())
    elif args.command == "world":
        world_stitch.render_all(load_inventory())
    elif args.command == "index":
        gen_index.render_all(load_inventory())
    elif args.command == "all":
        inv = load_inventory()
        render_indoor.render_all(inv)
        render_outdoor.render_all(inv)
        city_stitch.render_all(inv)
        world_stitch.render_all(inv)
        gen_index.render_all(inv)
        print("全部完成；确认产物后执行 publish 发布")
    elif args.command == "publish":
        cmd_publish()


if __name__ == "__main__":
    main()
