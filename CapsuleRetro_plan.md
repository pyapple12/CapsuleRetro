# CapsuleRetro — 项目计划书

> 老 DOS 游戏现代包装系统：把任意 DOS 游戏打包成"看起来像移植的现代游戏"，三平台（Windows / macOS / Linux）通用。

## 1. 项目概述

CapsuleRetro 是一套通用工具 + 单一游戏包的产物形态：

- **打包器（工具）**：输入游戏文件夹、攻略、封面、补丁，输出一个"游戏包"
- **游戏包（产物）**：用户拿到的是 **两个文件** —— 执行文件 + save 文件，随身携带，到任何机器解压即玩

### 核心卖点

| 卖点                  | 说明                                                    |
| --------------------- | ------------------------------------------------------- |
| 现代启动器            | 封面、原版 / hack 版入口、攻略阅读，现代游戏质感        |
| 可隐藏界面的 DOS 模拟 | 全屏 4:3 保持比例，空白区域自定义图片填充，无模拟器痕迹 |
| 随身携带              | 执行文件 + save 文件两个文件走天下                      |
| 补丁应用              | hack 版 = 原版 + 运行时应用补丁，原版文件永不被破坏     |
| 攻略内置              | 本地 Markdown 渲染成 HTML，离线可查                     |

## 2. 核心概念

### 2.1 两个文件架构

```
MyGame.exe / MyGame.app / MyGame.AppImage   ← 只读：DOSBox + 游戏 + 启动器 + 攻略 + 补丁
MyGame.save                                ← 唯一用户数据：存档 + 设置 + hack 状态（zip）
```

系统缓存目录（`%LOCALAPPDATA%` / `~/.cache` / `~/Library/Application Support`）存放释放的运行时与游戏文件，**丢失可自愈（重新释放）**，用户无需关心。

### 2.2 save 文件生命周期

1. 启动：解压 `MyGame.save` 到临时工作区
2. 运行：按 hack 标记决定是否应用补丁，启动 DOSBox
3. 退出：工作区重新打包回 `MyGame.save`（原子替换，防损坏）

### 2.3 hack 版 = 补丁应用

hack 版运行时对**游戏文件副本**应用补丁包，不改动原版。改补丁只需重新打包一次，无需维护多份游戏版本。

## 3. 技术栈

| 组件       | 选型                                    | 理由                                                         |
| ---------- | --------------------------------------- | ------------------------------------------------------------ |
| 启动器外壳 | **Tauri 2**（Rust 后端 + 系统 WebView） | 三平台原生支持，包体 10-20MB（Electron 需 200MB+，不可接受） |
| 前端 UI    | Vue 3 + TypeScript + Vite               | Tauri 官方模板生态，攻略 HTML 渲染天然支持                   |
| 模拟器     | **DOSBox Staging**（sidecar 二进制）    | 现代维护、SDL2、高 DPI、每个游戏独立 conf                    |
| 打包器     | Rust CLI（`capsule-pack`）              | 与 Tauri 同语言、跨平台、可嵌入 CI                           |
| 核心库     | Rust crate `capsule-core`               | save 打包/解包、补丁应用、路径管理，启动器与打包器共用       |
| 补丁格式   | 自定义文件级替换清单（zip）             | v1 简单可靠，未来可升级字节级 diff                           |
| save 格式  | zip                                     | 可压缩、可校验、结构透明                                     |

## 4. 架构

### 4.1 组件

```
┌─────────────────────────────────────────────────┐
│               capsule-core (Rust)               │
│  save 打包/解包 · 补丁应用 · 配置 · 路径管理      │
└──────────────┬──────────────────┬───────────────┘
               │                  │
┌──────────────▼───────┐  ┌──────▼───────────────┐
│   launcher (Tauri)   │  │  packer (Rust CLI)   │
│  UI · 进程管理       │  │  game.json → 游戏包  │
│  全屏会话 · 窗口管理  │  │  资源内嵌 · 产物生成  │
└──────────────┬───────┘  └──────────────────────┘
               │
┌──────────────▼───────┐
│  DOSBox Staging      │
│  (sidecar，界面隐藏)  │
└──────────────────────┘
```

### 4.2 启动器 UI 结构

- **主界面**：游戏封面、两个入口（原版 / hack 版）、攻略入口、设置（音量/分辨率/填充图选择）
- **攻略页**：侧边目录 + Markdown 渲染内容，WebView 内离线渲染
- **游戏会话**：全屏无边框，DOSBox 渲染区保持 4:3，四周填充自定义图片；热键呼出/隐藏启动器

### 4.3 4:3 填充方案（按平台）

| 平台          | 方案                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------ |
| Windows       | 启动器全屏窗口 + `SetParent` 嵌入 DOSBox 渲染窗口（首选，无焦点问题）                      |
| macOS / Linux | 覆盖层窗口方案：DOSBox 无边框窗口居中运行，启动器全屏"点击穿透 + 透明背景"窗口绘制填充图片 |

> macOS 无 Windows 式窗口嵌入 API，是最大技术风险点，覆盖层方案为备选兜底，POC 阶段必须验证。
>
> 实况修订（2026-09-07）：上表方案已被 T2 fork 线取代——两平台统一改为"DOSBox 帧流（REST）+ launcher 画布整数缩放"，无窗口嵌入需求；Windows 侧 SetParent 经 VC0 判死。本表保留作历史方案，风险讨论见 §8 与 y.problems #10。

### 4.4 运行时目录（Windows 示例）

```
%LOCALAPPDATA%/CapsuleRetro/<GameName>/
  game/          # 释放的游戏只读文件（缓存，可自愈）
  work/          # 可写工作区（存档、hack 副本）
  dosbox/        # sidecar 二进制
  dosbox.conf    # 由启动器生成的配置
```

## 5. 产物规格

| 平台    | 产物                                      | 说明                                             |
| ------- | ----------------------------------------- | ------------------------------------------------ |
| Windows | `GameName.exe`（单文件）+ `GameName.save` | 内嵌全部资源，首启释放；风险：杀软误报（见 §8）  |
| macOS   | `GameName.app` + `GameName.save`          | Finder 中呈现为单图标；免费 Gatekeeper 方案见 §8 |
| Linux   | `GameName.AppImage` + `GameName.save`     | 依赖 FUSE；备选：单 ELF + 缓存释放               |

体积预估：包体 40-60MB（DOSBox Staging ~15-30MB + 启动器 ~10-20MB + 游戏文件压缩存储）。

## 6. 数据格式

### 6.1 game.json（打包配置，打包器输入）

```json
{
  "id": "game-id",
  "name": "游戏名",
  "version": "1.0.0",
  "game_dir": "./game",
  "cover": "./cover.png",
  "fill_images": { "left": "./fill.png", "right": "./fill.png" },
  "walkthrough": "./攻略.md",
  "dosbox_conf": "./dosbox.conf",
  "patches": [{ "id": "hack1", "name": "Hack 版", "patch": "./hack1.zip" }]
}
```

### 6.2 patch 包（zip）

```
manifest.json    # [{action: "replace"|"add", path: "...", source: "files/..."}]
files/           # 实际替换文件
```

应用方式：将文件复制到工作副本（不触碰原版文件）。

### 6.3 save 文件（zip）

```
meta.json        # { hack: "hack1" | null, settings: {...}, version }
save/            # 游戏存档目录（对应 DOSBox mount）
config/          # 用户对 dosbox.conf 的覆盖片段
```

## 7. 实现路径

> **排期原则（2026-09-07 用户决策）：Windows 优先**——Phase 0–4 全部以 Windows 为唯一目标平台交付，功能全量完成后再进入 Phase 5 做 macOS/Linux 适配；三平台产品的最终目标不变。跨平台问题在主线期间只登记 y.problems.md，不阻塞开发。

### Phase 0 — POC 技术验证（约 1.5 周）✅ 主体完成

- [x] Tauri 2 项目骨架（三平台可跑通）——已完成（PL002.4；Windows 实测通过）
- [x] DOSBox Staging sidecar 集成：进程启动、界面隐藏、独立 conf ——已完成（PL002 阶段 B + 后期主线；"界面隐藏"以 Windows 离屏托管实现）
- [x] **4:3 填充验证**：Windows SetParent 嵌入 + macOS 覆盖层窗口，两者至少其一跑通 ——已完成（方案变更：SetParent 判死，由 fork 帧流 + 画布整数缩放达成，见下方实况备注）
- [ ] save zip 打包/解包/原子替换（随 PL003 落地）
- [ ] 验收：Windows 上跑通一个示例游戏，全屏填充图正确显示，存档能回写 save 文件（原 macOS 验收移 Phase 5）

> 实况备注（2026-09-07）：前三项已由 PL002 完成（界面隐藏 = DosboxCapsule 离屏托管；**4:3 填充方案变更**——SetParent 判死（VC0），改由 fork 帧流 + launcher 画布整数缩放达成，跨平台缺口仅剩 macOS/Linux 窗口隐藏，见 y.problems #10）；save 闭环未做，随 PL003 落地。

### Phase 1 — 启动器核心（约 2.5 周，Windows）

- [ ] 单游戏 UI：封面页、原版/hack 入口、攻略页（MD → HTML 渲染）
- [ ] 全屏游戏会话：填充图、4:3 区域计算、热键呼出/隐藏启动器
- [ ] DOSBox 生命周期管理：启动、退出、崩溃检测与恢复
- [ ] 设置项：音量、填充图切换、分辨率
- [ ] 验收：完整玩一局游戏，存档保存/恢复正确，界面无模拟器痕迹

### Phase 2 — 补丁与 save 闭环（约 1.5 周，Windows）

- [ ] patch 格式定义 + 应用器（文件级替换）
- [ ] save 完整闭环：启动解压 → 运行 → 退出回写（原子、防损坏）
- [ ] hack 标记持久化（meta.json）
- [ ] 验收：hack 版与原版切换互不污染，save 文件损坏时能恢复

### Phase 3 — 打包器 CLI（约 2 周，Windows 产物）

- [ ] game.json 校验与解析
- [ ] `capsule-pack`：Windows 单 exe 产物生成（macOS .app / Linux AppImage 移 Phase 5）
- [ ] 资源内嵌 + 首次运行释放（含进度提示）
- [ ] 游戏文件压缩存储
- [ ] 验收：用一款真实 DOS 游戏跑通 Windows 打包 → 运行全流程

### Phase 4 — Windows 打磨与发布（约 1.5 周）

- [ ] Windows 杀软误报实测与对策（签名或降级方案）
- [ ] GitHub Actions Windows 构建与产物分发流水线
- [ ] UI 打磨（封面、填充图效果、攻略排版）
- [ ] 验收：Windows 产物可从零运行，新机器首次启动无人工干预

### Phase 5 — macOS / Linux 适配与发布（约 1.5 周，排期最后）

- [ ] 窗口隐藏等价手段：macOS NSWindow / Linux 窗管方案验证（y.problems #10）
- [ ] `capsule-pack`：macOS .app / Linux AppImage 产物生成
- [ ] macOS Gatekeeper 用户指引（右键打开 / `xattr -cr` / 社区分发规避）；Linux webkit2gtk / AppImage FUSE 依赖文档
- [ ] GitHub Actions 三平台构建矩阵
- [ ] 验收：macOS/Linux 上跑通一个示例游戏全流程（原 Phase 0 顺延项）；三平台产物可从零运行，新机器首次启动无人工干预

## 8. 技术风险与降级方案

| 风险                                  | 等级 | 应对 / 降级                                                  |
| ------------------------------------- | ---- | ------------------------------------------------------------ |
| macOS 无窗口嵌入 API                  | 高   | 覆盖层点击穿透方案；最坏情况 macOS 用 DOSBox 原生全屏 + 黑边 |
| Windows 单文件 exe 杀软误报           | 高   | 实测；降级为"文件夹绿色版"（游戏目录 + save），体验损失小    |
| Linux webkit2gtk 依赖 / AppImage FUSE | 中   | 文档写明依赖；备选单 ELF + 缓存释放                          |
| Tauri 单二进制离开 .app 无法运行      | 中   | macOS 已定为 .app 形态，不做单二进制                         |
| 不同 DOS 游戏配置差异大               | 中   | game.json 暴露 dosbox_conf，逐游戏调优配置                   |
| 首次运行释放缓存耗时                  | 低   | 释放时显示进度；增量释放（版本号比对）                       |

> 实况备注（2026-09-07）：第 1 行风险已随 §4.3 方案变更转移——统一窗口形态下 Windows 无窗口嵌入需求，macOS/Linux 剩"窗口隐藏"缺口（y.problems #10）；该缺口已随排期调整（Windows 优先）移至 Phase 5 处理。

## 9. 时间预估

**全职投入：约 10 周**（2026-09-07 排期调整：Phase 0–4 Windows 全量优先约 9 周 + Phase 5 macOS/Linux 适配约 1.5 周）

| Phase   | 内容             | 时间   |
| ------- | ---------------- | ------ |
| Phase 0 | POC 技术验证     | 1.5 周 |
| Phase 1 | 启动器核心       | 2.5 周 |
| Phase 2 | 补丁与 save 闭环 | 1.5 周 |
| Phase 3 | 打包器 CLI       | 2 周   |
| Phase 4 | 打磨与发布       | 2 周   |

**兼职投入：约 3-4 个月**（按每天 2-3 小时估算）

关键里程碑：Phase 0 结束 = 技术可行性确认（最大风险已排除）；Phase 2 结束 = 产品闭环可用；Phase 4 结束 = 可发布。
