# CapsuleRetro —— 老 DOS 游戏的现代包装系统

[![Version](https://img.shields.io/badge/Version-0.1.0.30-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.96-orange.svg)](https://www.rust-lang.org)
[![Phase](https://img.shields.io/badge/Phase-PL006%E2%80%93PL011%E5%AE%8C%E6%88%90-brightgreen.svg)](z.plan.md)

把任意 DOS 游戏打包成"看起来像移植的现代游戏"：玩家拿到 **三个文件**，双击即玩，无安装、无模拟器痕迹。当前以 Windows 为交付目标，macOS / Linux 为 0.2.0 目标。

## 产品形态（三文件走天下）

| 文件            | 内容                                             | 说明                                       |
| --------------- | ------------------------------------------------ | ------------------------------------------ |
| `GameName.exe`  | 只读：DOSBox fork + 游戏 + 启动器 + 攻略 + 补丁  | 资源以 zip payload 内嵌 exe 尾部，可改名   |
| `GameName.save` | 唯一用户数据：存档 + hack 状态（zip）            | 名字从 game.json `name` 派生，exe 改名无感 |
| `readme.md`     | 随包说明：使用方法 / 改名规则 / SmartScreen 指引 | 打包器从模板生成，注入游戏名与版本         |

运行时资源释放到系统缓存目录（`%LOCALAPPDATA%/CapsuleRetro/<game>/`），丢失可自愈重新释放，用户无需关心。

| 卖点         | 说明                                                                   |
| ------------ | ---------------------------------------------------------------------- |
| 零痕迹模拟   | fork DOSBox 窗口创建即隐藏（静默启动），画面只活在启动器画布里         |
| 流畅帧流     | 渲染帧经 HTTP chunked 长连接每帧即推，无轮询陈旧度（帧龄 p50 10–12ms） |
| 现代启动器   | 待机页原版 / hack 切换、攻略 Markdown 渲染、实时 HUD 状态条            |
| 补丁应用     | hack 版 = 原版副本 + 运行时应用补丁，**原版游戏文件永不被修改**        |
| 一条命令出包 | `capsule-pack` 读 game.json → 校验 → 资源内嵌 → dist 三文件            |
| 伴生观察     | REST 原子读取模拟器内存，109 条目内存档案实时驱动 HUD 与诊断 CLI       |

> 当前状态：**PL001–PL011 全部完工 + FIX001/FIX002 两轮审计收口**（V0.1.0.22）——产品三文件闭环、静默启动、帧流直推、HUD（头像缩略 / HP 四色 / 位置五要素）、会话内金手指、地图与头像资产链（622 张地图 + 24 张头像）、攻略渲染均已 live 验收。**PL012 老板键已完结**（2026-10-07，omarchy 桌面复刻 + Ctrl+F10）；**PL013 金框按钮动效顺滑化已完结**（2026-10-07，双曲线按压 + 流光迁合成器 + 文字 SVG 化）；**PL014 职业与人种徽章已完结**（2026-10-08，五组配色胶囊 + SVG 居中）；**PL015 小地图拖拽已完结**（2026-10-08，箭头入地图层随图联动 + 边界钳制）；**PL016 游玩 UI 重构**（实验场与 CRT 特效已实装，对接未开始，见 `assets/design/game-ui-handoff.md`）。后续：PL016 补后端缺口并对接 Launcher → 发布工程立项（GitHub Actions / fork Release / 杀软实测）；0.2.0 三端适配登记不动。方案与实测结论见 `z.plan.md` 附录，体验硬化阶梯见 `CapsuleRetro_plan.md` §10。

## 技术栈

| 层     | 选型                                                    | 说明                                                                            |
| ------ | ------------------------------------------------------- | ------------------------------------------------------------------------------- |
| 外壳   | Tauri 2（Rust + 系统 WebView）                          | 会话编排、进程管理（Job Object）、Tauri 命令层                                  |
| 业务   | Rust：`capsule-core` + `capsule-pack`                   | save/补丁/payload/路径生命周期与打包编排，launcher 与 packer 共用，逻辑只写一份 |
| 前端   | Vue 3 + TypeScript + Vite                               | 画布 / HUD / 攻略展示与命令转发，业务逻辑零含量                                 |
| 模拟器 | DOSBox Staging v0.83.0 fork（**DosboxCapsule** 独立仓） | 5 个代码补丁：帧捕获 / 输入注入 / 错误映射 400 / 帧流直推 / 窗口隐藏            |
| 帧传输 | HTTP chunked 流式直推（vendored cpp-httplib）           | 端口锁死 127.0.0.1:18086，画布消费 pre-shader 帧自行 4:3 整数缩放               |
| 输入   | 注入端点（Bridge + mapper，nomouse 形态）               | dosbox 永不抓宿主光标，宿主光标归画布 Pointer Lock、DOS 光标归注入原语          |

## 快速开始

### 环境要求

- Rust toolchain（cargo 1.96+）、Node.js（26+）
- Windows 10/11（当前开发与验证实机；macOS/Linux 适配为 0.2.0）
- 帧流画面依赖 **DosboxCapsule** fork 构建产物（官方原版可跑会话但无帧流画面）

### 构建与测试

```bash
cargo test --workspace        # 全量测试（核心库 / launcher / 打包器）
npm install                   # 前端依赖（tauri CLI 随 devDep 就位）
npm run build                 # 前端构建校验（含 vue-tsc）
cargo run -p launcher         # 开发运行启动器（或 npm run tauri dev）
```

### 打包出产品（三文件）

```bash
cargo build --release -p launcher
cargo run -p capsule-pack -- retro/game.json --launcher target/release/launcher.exe -o dist
# → dist/XEEN.exe + dist/readme.md（save 首次会话自动生成）；拷到任意目录双击即玩
```

### 观察组件（xmonitor 诊断 CLI）

```bash
cargo run -p capsule-core --bin xmonitor -- --game-json retro/game.json watch
# 实时显示：全队 HP/MP/EXP、随身金币/宝石、银行存款（按 memory_map.json 档案）

cargo run -p capsule-core --bin xmonitor -- search   # 差分搜索内存偏移（适配新游戏用）
cargo run -p capsule-core --bin xmonitor -- bench    # REST 读取性能基准
```

## 架构一句话

fork DOSBox 把渲染帧经 chunked 长连接直推 Tauri 画布（每帧即推、SDL 窗口创建即隐藏）、键盘鼠标经注入端点回灌；save 打包/解包/补丁应用/payload 内嵌全部收敛在 `capsule-core` 并可 `cargo test` 直测——launcher 与 packer 只做编排，业务逻辑零重复。

## DosboxCapsule（fork 仓）

- 独立仓 `DosboxCapsule`：v0.83.0 tag 锚点 + 5 个主题化代码补丁（单 main 分支，`git diff v0.83.0..main` 即全部足迹）
- 补丁纪律：最小 diff、PATCHES.md 登记、清洁重建（`tools/dosbox-fork/rebuild.bat`）、每次升级必跑 FB1 基线判定（109 条目内存字节级对照，假 GO 双路径已封堵）
- fork 基于 GPLv3，分发须遵守开源义务

## 项目结构

```
CapsuleRetro/
├── Cargo.toml               # workspace 根（版本单一来源：[workspace.package] version）
├── capsule-core/            # 核心 crate：save/patch/paths 生命周期 + payload 内嵌格式
│   │                        #   + monitor REST 观察库 + game.json 解析（TDD 全覆盖）
│   └── src/bin/xmonitor.rs  # 诊断 CLI（watch / search / bench）
├── launcher/                # Tauri 2 应用：src-tauri（会话编排 / conf / HUD / 进程）
│   │                        #   + src（App / GameCanvas 帧流画布 / HudBar）
├── capsule-pack/            # 打包器 CLI：校验 → 资源收集 → payload 拼装 → 三文件产物
├── assets/dosbox/           # 官方 v0.83.0 sidecar zip（未打补丁原版，入库）
├── assets/design/           # UI 视觉稿库与 PL012 实验场（自包含 HTML + 背景候选图）
├── tools/dosbox-fork/       # DosboxCapsule 维护套件（PATCHES.md / rebuild.bat / FB1 基线）
├── tools/maptool/           # XEEN 地图资产管线（til / xeendata / render_* / city_stitch / CLI）
├── retro/                   # 根 = 适配层：game.json（v2）、xeen-guide.md（攻略）、xeen 机制档案、
│   │                        #   memory_map.json（109 条目内存档案）、patches/
│   │                        #   face-map.md（面容）/ map-atlas.md（地图）/ artwork/（封面·头像·地图）
│   └── XEEN/                # 纯原版游戏文件（字面意义只读）
├── .archived/               # mm4/mm5 原始素材（只读）
├── .reference/              # 外部参考图（本地留存，gitignore）
├── .xeen-extract/           # CC 素材工坊（gitignore）
├── AGENTS.md                # 项目规范（AI 协作必读）
├── CapsuleRetro_plan.md     # 总体规划（Phase 0-5、版本路线、体验硬化阶梯）
├── z.plan.md                # 方案与审计归档（PL001–PL014 / A001–A004 / 豁免定案清单）
├── x.progress.md            # 任务清单
├── y.problems.md / w.study.md
└── .agents/skills/          # 项目自建 skill（audit-project / audit-report / progress-task）
```

## 文档地图

- `CapsuleRetro_plan.md`：总体规划与 Phase 划分、版本路线（0.1.0 Windows 可玩可发布 / 0.2.0 三端适配）、风险对策
- `AGENTS.md`：工程原则、代码规范、错误策略（容错白名单）、提交规范（AI 协作必读）
- `z.plan.md`：专题方案与审计归档（附录 PL = 方案实测结论，附录 A = 审计报告）
- `x.progress.md`：任务清单与进度
- `y.problems.md`：问题与远期改进备忘录
- `retro/zhuyin-keyboard.md`：注音输入对照（汉化版游戏内打字用）
