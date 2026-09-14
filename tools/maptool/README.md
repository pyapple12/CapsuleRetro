# tools/maptool —— XEEN 地图资产管线

从游戏原始资源（`retro/XEEN/DARK.CUR` / `XEEN.CUR` + `.xeen-extract` 提取工作区）
渲染全量地图资产：户内单图、户外单图、世界整图、城镇整城分层拼合图与清单
`index.json`。渲染规则的数据级考证与实证记录统一维护在
[retro/map-atlas.md](../../retro/map-atlas.md)，本 README 只讲用法与流程。

## 依赖

- Python 3.12+（仅标准库，无第三方包；PNG 为内置 zlib 手写编码器）

## 用法

```bash
python tools/maptool/maptool.py inventory   # 清单摘要（人读核对）
python tools/maptool/maptool.py all         # singles → cities → world → index
python tools/maptool/maptool.py publish     # 产物复制入 retro/artwork/maps
```

子命令可单独执行：`singles`（户内+户外单图）、`cities`（37 组整城分层拼合）、
`world`（世界整图）、`index`（清单）。产物先落在 `.xeen-extract/png/maps/`
（gitignore 工作区），**确认后**再 `publish` 复制入 `retro/artwork/maps/`
（版本化）；发布后按仓库门禁对 index.json 过 `npx prettier --write`。

## 模块

| 模块                | 职责                                                          |
| ------------------- | ------------------------------------------------------------- |
| `til.py`            | 调色板 / CC 精灵帧解码 / PNG 写出（路径守卫限制在提取工作区） |
| `xeendata.py`       | CUR/CC 解析、资源读取、侧属区域名、主图清单、连通分量规划     |
| `render_indoor.py`  | 户内四层方程式（地面→点缀→墙→空置盖板）+ 单图批处理           |
| `render_outdoor.py` | 户外 Tab 四层算法 + 最近邻 ×6 放大                            |
| `city_stitch.py`    | 城镇整城分层拼合（资源上下文归锚点；层先拼后叠）              |
| `world_stitch.py`   | 世界整图缝合（区域名网格 A1–F4；天路单独一组）                |
| `gen_index.py`      | `index.json` 生成（含 city_maps 段）                          |
| `maptool.py`        | CLI 入口（子命令见上）                                        |

## 验收基准（回归锚点）

对照官方实机截图（外部材料，不入库）逐像素误差，重渲后应与下列数值一致：

| 基准                              | 官方参照     | err  |
| --------------------------------- | ------------ | ---- |
| `cloud028-city.png` 佛提格市      | 717×574 截图 | 86.8 |
| `cloud035-city.png` 內可玻利斯    | 796×325 截图 | 93.5 |
| `dark124-city.png` 亡者地牢第四层 | 712×572 截图 | 57.1 |

偏移/画序/层级等任何规则改动都会破坏这些数值；规则级修改必须同步
`retro/map-atlas.md`（含勘误编号）。
