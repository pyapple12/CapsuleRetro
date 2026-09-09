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

> 实况修订（2026-09-09）：产品形态定为三文件——执行文件 + save + `readme.md`（随包说明：使用方法 / 改名规则 / SmartScreen 指引）；save 名从 game.json `name` 派生，执行文件改名无感；执行文件资源内嵌方式 = 尾部追加 zip payload。详见 z.plan.md 附录 PL004。

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

### Phase 0 — POC 技术验证（约 1.5 周）✅ 完成（2026-09-08 收口）

- [x] Tauri 2 项目骨架（三平台可跑通）——已完成（PL002.4；Windows 实测通过）
- [x] DOSBox Staging sidecar 集成：进程启动、界面隐藏、独立 conf ——已完成（PL002 阶段 B + 后期主线；"界面隐藏"以 Windows 离屏托管实现）
- [x] **4:3 填充验证**：Windows SetParent 嵌入 + macOS 覆盖层窗口，两者至少其一跑通 ——已完成（方案变更：SetParent 判死，由 fork 帧流 + 画布整数缩放达成，见下方实况备注）
- [x] save zip 打包/解包/原子替换（随 PL003 落地）——已完成（capsule-core save/patch/paths 三模块 + 15 集成测试）
- [x] 验收：Windows 上跑通一个示例游戏，全屏填充图正确显示，存档能回写 save 文件（原 macOS 验收移 Phase 5）——已完成（PL003 VA1 live：存档 → 停止回写 → 读档续玩；画面由帧流画布承担）

> 实况备注（2026-09-07）：前三项已由 PL002 完成（界面隐藏 = DosboxCapsule 离屏托管；**4:3 填充方案变更**——SetParent 判死（VC0），改由 fork 帧流 + launcher 画布整数缩放达成，跨平台缺口仅剩 macOS/Linux 窗口隐藏，见 y.problems #10）；save 闭环未做，随 PL003 落地。
> 实况收口（2026-09-08）：save 闭环已随 PL003 落地（打包/解包/原子替换 + 损坏降级 + 补丁纪律代码化），live 验收全过（z.plan 附录 PL003 实测结论）。

### Phase 1 — 启动器核心（约 2.5 周，Windows）✅ 主体完成（余项归 PL005）

- [ ] 单游戏 UI：封面页、原版/hack 入口、攻略页（MD → HTML 渲染）——大半完成（原版/hack 入口 + save 状态 + 攻略渲染已落地：PL003.12/13 live 验证；封面页未做，归 PL005 UI 打磨）
- [ ] 全屏游戏会话：填充图、4:3 区域计算、热键呼出/隐藏启动器 ——方案已被取代（PL002 统一窗口形态：帧流 + 画布整数缩放，4:3 正确显示 live 验证；填充图随 SetParent 一并淘汰；热键呼出/隐藏未做，PL005 评估）
- [x] DOSBox 生命周期管理：启动、退出、崩溃检测与恢复 ——已完成（PL002 会话链路 + FIX001.5 exit-watcher：退出/被杀检测 → session-ended 事件 + save 回写；恢复 = 重新启动会话，存档由原子回写保障）
- [ ] 设置项：音量、填充图切换、分辨率 ——未做（save meta 预留 settings 字段；填充图切换随方案变更失去对象；归 PL005 评估取舍）
- [x] 验收：完整玩一局游戏，存档保存/恢复正确，界面无模拟器痕迹 ——已完成（PL003 VA1 live 验证；统一窗口无模拟器痕迹）

### Phase 2 — 补丁与 save 闭环（约 1.5 周，Windows）✅ 完成（2026-09-08，PL003）

- [x] patch 格式定义 + 应用器（文件级替换）——已完成（capsule-core patch.rs：manifest replace/add，7 种路径穿越全拒）
- [x] save 完整闭环：启动解压 → 运行 → 退出回写（原子、防损坏）——已完成（save.rs + paths.rs + exit-watcher/stop_game 双挂点；损坏白名单降级）
- [x] hack 标记持久化（meta.json）——已完成（meta.hack 随 save 持久化，UI 选择为权威源）
- [x] 验收：hack 版与原版切换互不污染，save 文件损坏时能恢复 ——已完成（VA2：retro/ 21 文件 MD5 三比照全等；VA3：垃圾字节 → .corrupt 备份 + 降级启动）

### Phase 3 — 打包器 CLI（约 2 周，Windows 产物）✅ 完成（2026-09-09，PL004）

- [x] game.json 校验与解析 ——已完成（capsule-core game_config.rs：v2 结构 game_dir/save_files/patches + 校验）
- [x] `capsule-pack`：Windows 单 exe 产物生成（macOS .app / Linux AppImage 移 Phase 5）——已完成（exe 尾部追加 zip payload + trailer；dist/XEEN.exe 37.6MB / 413 条目）
- [x] 资源内嵌 + 首次运行释放（含进度提示）——已完成（payload extract-once 释放 runtime/resources，版本标记自愈；进度提示不必要——实测秒级）
- [x] 游戏文件压缩存储 ——已完成（payload zip deflate）
- [x] 验收：用一款真实 DOS 游戏跑通 Windows 打包 → 运行全流程 ——已完成（VA1 干净环境用户 live 通过）

### Phase 4 — Windows 打磨与发布（约 1.5 周）→ 发布打磨（待 PL005 体验硬化后立项，编号届时定）

- [ ] Windows 杀软误报实测与对策（签名或降级方案）——0.1.0 倾向 zip 分发 + readme SmartScreen 指引，代码签名后续评估
- [ ] GitHub Actions Windows 构建与产物分发流水线
- [ ] UI 打磨（封面、填充图效果、攻略排版）——不做动效（2026-09-09 定案）；填充图效果随方案变更由统一窗口画布承接
- [ ] 验收：Windows 产物可从零运行，新机器首次启动无人工干预

### Phase 5 — macOS / Linux 适配与发布（约 1.5 周，排期最后）→ 0.2.0（登记不动，见 §9 版本路线）

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
| 帧流转录延迟 / 输入延迟 不达预期      | 中   | 体验硬化阶梯（§10）：WS 直推 → 进程内 core → 真窗口嵌入      |

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

版本路线（2026-09-09 用户定案）：

- **0.1.0 = Windows 可玩可发布**：对应 Phase 0–4（PL004 打包器落地 + 体验硬化（PL005）与发布打磨收口，打 V0.1.0.R）
- **0.2.0 = 三端适配（登记不动）**：macOS / Linux，对应 Phase 5；0.1.x 稳定后再细化立项（最大风险项：Linux Wayland 输入注入）

## 10. 体验硬化阶梯（2026-09-09 定案）

> 回退锚点：V0.1.0.6（commit 0d0931d）——后续所有会话体验优化方案失败时的最终回退对象。
> 现状：会话画面为 fork 帧流转录（launcher 30fps 轮询 + Rust/IPC 中转），输入经
> JS → Tauri IPC → HTTP 注入多跳；输入与画面延迟感明显，dosbox SDL 窗口启动期可见。

阶梯按序推进，每档设进入判据，禁止凭感觉换线：

| 档  | 方案                                                                                                                                     | 判据                                                                                                                |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| ②   | 隐藏窗口 + WebSocket 直推：SDL 窗口创建即隐藏（静默启动，winembed 退役）；fork 帧经 WS 直推 WebView 画布（rAF 对齐），输入暂留 HTTP 注入 | 目标：输入注入往返 <5ms、显示帧龄 <17ms、无窗口闪现；先打点基线再改、改后对比；不达标且数据证明瓶颈在传输层 → 评估③ |
| ③   | 进程内 core（libretro 式）：fork 编译为库，launcher 直调 run_frame(surface) / inject_input()                                             | 进入前先做可行性打样（最小接口 + 构建改动评估）；打样失败或工作量超预算 → ①                                         |
| ①   | 真窗口嵌入复活版：fork 内自 SetParent + AttachThreadInput 共享输入队列                                                                   | 最后手段，③不可行且出现"必须有真嵌入"的场景才启动                                                                   |

> 连带约束：端口锁死 18086，CSP connect-src 精确放行 http://127.0.0.1:18086（安全配置变更必须
> live 全链路验证，见 PL003 教训）；输入暂留 HTTP、超标再并上行通道；HUD / memory_map /
> save 生命周期不动，本阶梯只动显示与输入的传输层；winembed 整链退役，留
> CAPSULERETRO_SHOW_DOSBOX 调试开关。

> 阶梯②落地收口（2026-09-09，PL005）：机制落地为 **HTTP chunked 流式直推**（vendored
> cpp-httplib 无 WS 支持，推送语义一致），帧龄 p50 10–12ms（基线口径 42–53ms，降约
> 60–75%），静默启动零窗口闪现；注入真实路径 p50 9.1ms 超 5ms 目标 + 鼠标跟手残余
> 差距——合并登记为阶梯③的动机（未启动）。
