# CapsuleRetro — Agent Guide

老 DOS 游戏现代包装系统：把任意 DOS 游戏打包成"看起来像移植的现代游戏"，三平台（Windows / macOS / Linux）通用。产物 = 执行文件 + save 文件 + readme.md 三个文件走天下。总体规划见 `CapsuleRetro_plan.md`。

**当前状态**：PL001–PL003 主线与 FIX001 审计修复完结（2026-09-05 / 07 / 08），PL004 capsule-pack 打包器 + 打包形态落地完结（2026-09-09，产品三文件闭环达成、dist 可产出，见 z.plan.md 附录 PL004）——`capsule-core`（monitor 观察库 + game.json 解析 + save/patch/paths 生命周期三模块）+ `xmonitor` 诊断 CLI + `launcher`（Tauri 2 + Vue 3；统一窗口形态 + save 闭环 + hack 切换 + 攻略渲染，见 z.plan.md 附录 PL002/PL003）。已有素材：`retro/XEEN/`（《魔法门 XEEN》DOS 版，含 CHINFONT 汉化字体与 CMM4.PAT 补丁）、`retro/` 根适配层（game.json v2、xeen-guide.md 攻略、zhuyin-keyboard.md 注音对照、memory_map.json 内存档案、patches/test-hack.zip 测试补丁包）、`.archived/`（mm4/mm5 原版 zip）、`.agents/skills/`（项目自建 skill）、独立模拟器仓 `DosboxCapsule`（`E:\GameMission\DosboxCapsule`，v0.83.0 + 5 代码补丁，维护套件见 `tools/dosbox-fork/`）。遗留与远期项登记 `y.problems.md`。PL005 会话体验硬化已完结（2026-09-09，帧流 chunked 直推降延迟 + dosbox 静默启动，见 z.plan.md 附录 PL005；体验硬化阶梯与回退锚点 V0.1.0.6 见 CapsuleRetro_plan.md §10）；FIX002 第 2 轮审计修复已完结（2026-09-10，39 条 + 快速重启竞态连带修复，见 z.plan.md 附录 A002）；PL006 UI 第一遍打磨已完结并经用户目测判收（2026-09-10，v3 黑金典藏版落地：待机页/会话态/攻略面板按稿移植 + cover/hero 封面资产链 + 金框卡带图标与窗口标题，见 z.plan.md 附录 PL006）。PL007 会话内数值修改器（金手指第一歩）已完结（2026-09-11——实况修正：上游 vanilla v0.83.0 webserver 本就内置内存写端点，fork 零改动；落地为 monitor 白名单闸 write_memory/adjust_field + launcher cheat_adjust + 会话态「金币/宝石 +10000」按钮，live 双向闭环实证，见 z.plan.md 附录 PL007）。PL008 CC 素材提取链路已完结（2026-09-12——实况修正：ScummVM 无 xeen 引擎，肉眼基准用 DosboxCapsule 截图、格式参考用 OpenXeen；落地为 capsule-core cc_archive 模块 + xeen-extractor list/extract CLI + `.xeen-extract/` 素材库（2457 原始资源 + 1059 PNG + 素材墙 + 161 头像清单），见 z.plan.md 附录 PL008）。下一步：**UI 持续迭代期（2026-09-10 用户定调）**——第一遍仅是起点，后续以连续小热更新方式打磨 UI，最终形态待用户深入考虑后定案；期间不进发布工程立项（GitHub Actions / DosboxCapsule Release / 杀软实测在 UI 定案后另行立项），0.2.0 三端适配为远期目标。PL009 角色头像上屏已完结（2026-09-12——face 机制破解：面容文件号 = 名册槽位 + 1（第 N 槽用 CHAR(N+1).FAC），live 面容数组 0x5A7C:0x2；24 张头像随包 `retro/artwork/faces/` + HUD 头像缩略与 HP 着色（≥60% 绿 / 20–60% 黄 / <20% 红 / 负值蓝，会话滚动基线）+ 空槽占位；机制档案 `retro/face-map.md`，见 z.plan.md 附录 PL009）。PL010 XEEN 地图底图自渲染已完结（2026-09-13，311 主图 + 96 变体盘点、892B 布局 Rust 化 `maze.rs` + `mazes` 校验命令、76 户外 + 235 户内双分辨率共 622 图交付、index.json 与 `retro/map-atlas.md` 转正，见 z.plan.md 附录 PL010）；PL011 队伍位置读取已完结（2026-09-13，四连字节 `0x5A7CA` + 侧标志 `0x5A7CE` 锁定、区域名双文件族 `XEEN/DARKxxxx.TXT` 按侧破解、HUD 五要素 live 判收"HUD 对的"，见 z.plan.md 附录 PL011）；**PL012 老板键已完结**（2026-10-07 收口 8/8 经用户判收，V0.1.0.26：实验场 `#boss-screen` overlay——两轮探针取色规格 + Ctrl+F10 即时切换 + 操作栏「老板」按钮 + 壁纸 waybar + fastfetch/真源码编辑器/cargo 日志流三终端，机制待 Launcher 对接期平移，见 z.plan.md 附录 PL012）；**PL013 金框按钮动效顺滑化已完结**（2026-10-07 收口 5/5 经用户判收：双曲线按压 + 流光 1:1 迁合成器（环边像素对照视觉不变）+ 十颗按钮文字 SVG 化（ghost 垫片保自适应宽），机制待 Launcher 对接期平移，见 z.plan.md 附录 PL013）；**PL015 小地图拖拽已完结**（2026-10-08 收口 3/3 经用户判收：中键拖拽=拖动地图查看、箭头移入地图层随图一体联动可出画、钳制对三者之和整体生效永不露黑底、走路/切图回中，见 z.plan.md 附录 PL015）；**PL016 游玩 UI 重构进行中**（2026-09-15 起实验场阶段：自包含 HTML 四轮迭代至 v4.5，含左栏操作栏与浮窗、背景图按世界切换与毛玻璃滑杆、Uiverse 六组件接入、CRT 特效六项已实装基本设置（默认全关，默认值待用户调参定稿）；**对接 Launcher 未开始**，交接文档 `assets/design/PL012-game-ui-handoff.md` 含对接要点与后端六项缺口，第六节七项已全勾）。下一步：PL016 补后端缺口 → 按交接文档对接 Launcher；期间仍不进发布工程立项。

## 技术栈

| 组件       | 选型                           |
| ---------- | ------------------------------ |
| 启动器外壳 | Tauri 2（Rust 后端 + WebView） |
| 前端 UI    | Vue 3 + TypeScript + Vite      |
| 模拟器     | DOSBox Staging（sidecar）      |
| 打包器     | Rust CLI（capsule-pack）       |
| 核心库     | Rust crate（capsule-core）     |

## 启动命令

```bash
cargo run -p launcher                            # 开发运行启动器（或 cargo tauri dev）
npm run dev                                      # 前端热更开发
cargo run -p capsule-pack -- retro/game.json --launcher target/release/launcher.exe -o dist
                                                 # 打包器：→ dist/XEEN.exe + readme.md（save 首启生成）
cargo test --workspace                           # 全量测试
npm run build                                    # 前端构建校验
```

依赖：Rust toolchain（cargo 1.96+）、Node.js（26+）、DOSBox Staging 二进制（sidecar，随包分发）。

## 架构要点

- **三文件架构**：`GameName.exe/.app/.AppImage`（只读：DOSBox + 游戏 + 启动器 + 攻略 + 补丁，资源以 zip payload 内嵌 exe 尾部）+ `GameName.save`（唯一用户数据：存档 + 设置 + hack 状态，zip 格式；名字从 game.json `name` 派生，exe 改名无感）+ `readme.md`（随包说明：使用方法 / 改名规则 / SmartScreen 指引）
- **capsule-core**：save 打包/解包（原子替换）、补丁应用、路径管理——launcher 与 packer 共用，逻辑只写这一份
- **launcher**（Tauri 2 + Vue 3）：UI、DOSBox 进程管理（sidecar）、全屏游戏会话、攻略 Markdown 渲染
- **packer**（capsule-pack CLI）：读 game.json → 校验 → 资源内嵌 → 生成三平台产物
- **save 生命周期**：启动解压到临时工作区 → 运行 → 退出原子回写（防损坏，丢失可自愈）
- **补丁纪律（产品核心，代码与 Agent 操作双重遵守）**：hack 版 = 原版文件副本 + 运行时应用补丁；**原版游戏文件永不被修改**，补丁只作用于释放出来的工作副本
- **4:3 填充**：已由 fork 帧流方案实现——launcher 画布消费 pre-shader 帧自行整数缩放（SetParent 路线 VC0 判死，见 z.plan 附录 PL002）；PL005 起帧经 chunked 流式直推（每帧即推，无轮询）+ fork 窗口创建即隐藏（静默启动，winembed 退役）；nomouse 形态定案：dosbox 永不抓宿主光标，宿主光标归画布 PointerLock、DOS 光标归注入原语；macOS/Linux 窗口隐藏同用 SDL_WINDOW_HIDDEN（跨平台 flag），0.2.0 实机验证 [problems#10]
- **运行时目录**：系统缓存目录（如 `%LOCALAPPDATA%/CapsuleRetro/<GameName>/`）存放释放的 game/work/dosbox，丢失可自愈重新释放

## 目录规划

```
Cargo.toml       # workspace 根（version 单一来源；当前 members=["capsule-core", "launcher", "capsule-pack"]）
capsule-core/    # Rust crate：save 打包/解包、补丁应用、配置、路径管理
                 #   src/monitor/      REST 观察库（PL001）
                 #   src/save.rs       save 档案打包/解包/原子替换（PL003）
                 #   src/patch.rs      补丁包解析与应用（PL003）
                 #   src/paths.rs      运行时释放与生命周期编排（PL003）
                 #   src/payload.rs    exe 尾部 payload 拼装/解析/extract-once（PL004）
                 #   src/maze.rs       CUR 的 892B 迷宫布局严格解析（PL010）
                 #   src/cc_archive.rs CC 容器目录解密与资源还原（PL008）
                 #   src/bin/xmonitor  诊断 CLI（watch/search/bench）
                 #   src/bin/xeen-extractor  CC 素材提取 CLI（list/extract/mazes，PL008/PL010）
                 #   tests/            集成测试（cargo test 自动发现）
launcher/        # Tauri 2 应用（src-tauri/：commands / gameconf / hud / logging / jobctl / winproc
                 #   + mapnames.csv 区域名表；+ src/：App / GameCanvas / HudBar）
capsule-pack/    # Rust CLI 打包器（PL004）：game.json + launcher exe → dist 三文件
                 #   （GameName.exe 资源内嵌 + readme.md；save 由产品首启生成）
assets/dosbox/   # DOSBox Staging v0.83.0 sidecar（锁版本，进 git 的是 zip；
                 #   解压目录 dosbox-staging/ 为派生产物，已 gitignore）
assets/design/   # UI 视觉稿库与实验场（自包含 HTML，引用 retro/artwork/ 真图）：
                 #   v3 黑金典藏版 = PL006 视觉定案，其余四版留作 theme 迭代参考；
                 #   pl012-游玩界面实验场.html（PL012/PL016 评审媒介）+ PL012-game-ui-handoff.md
                 #   （PL016 对接要点与后端缺口）+ omarchy.png（PL012 老板键还原基准）；
                 #   背景候选三图（MM4-Clouds / MM5-Darkside / xeed16x9）
tools/dosbox-fork/ # DosboxCapsule fork 维护套件（版本化）：PATCHES.md（补丁登记
                 #   与升级配方）、rebuild.bat（清洁重建）、baseline-capture.sh /
                 #   baseline-compare.py（FB1 基线判定，升级后必跑）
tools/maptool/   # XEEN 地图资产管线（正式组件，Python 标准库零依赖）：til（解码+PNG）、
                 #   xeendata（CUR/CC 数据层+区域名双表+连通分量规划）、render_indoor
                 #   （户内四层方程式）/render_outdoor/city_stitch（整城分层，资源上下文
                 #   归锚点）/world_stitch/gen_index/maptool.py CLI（all/publish）；
                 #   渲染规则与验收基准见 retro/map-atlas.md 与 tools/maptool/README.md
retro/           # 根 = 适配层（我们编写的文件）：game.json（v2：game_dir/save_files/patches）、
                 #   patches/（测试补丁包）、memory_map.json（109 条目内存档案）、face-map.md（面容档案：
                 #   face 数组 0x5A7C:0x2、槽位+1=头像文件号、角色对照总账）、map-atlas.md（地图档案：
                 #   892B 布局、户内四层渲染定案、区域名双文件族、坐标换算）、xeen-guide.md（攻略）、
                 #   zhuyin-keyboard.md（注音对照）、artwork/（封面/主视觉/faces 头像 24 张/maps 地图资产）
                 # 子目录（如 retro/XEEN/）= 纯原版游戏文件，字面意义只读
.agents/skills/  # 项目自建 skill（audit-project / audit-report / progress-task），ZCode workspace 级扫描目录
.archived/       # 归档素材（mm4.zip / mm5.zip，只读）
.reference/      # 外部参考图（MM4/MM5 官方世界地图与主视觉，本地留存，gitignore 不入库）
.temp/           # 临时脚本与文件（gitignore）；探针、快照、验证记录放这里
.xeen-extract/   # CC 素材工坊（gitignore，版权素材只留本地）：xeen-extractor 的
                 #   提取输出（cc/）、转码素材（png/）、素材墙（browse.html）、
                 #   OpenXeen 参考克隆（tools/）；PL008–PL011 工作区
y.problems.md    # 问题与远期改进备忘录（只增不删、编号递增；任务清单以 [problems#N] 引用）
```

## 自动格式化与静态检查（硬性工作流，替代 LSP）

> ZCode 无自动 LSP / Formatter 挂载机制，本节即强制工作流：**改完必须跑，不通过不得宣称完成**；环境缺工具时先报告用户，不得静默跳过。

- **编辑任何 `.rs` 文件后**：依次运行 `cargo fmt`（改动所在 crate）、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo check`，三条全过才算完成
- **编辑前端文件（`.ts` `.tsx` `.vue` `.css` `.json`）后**：运行 `npx prettier --write <file>`；若改动含逻辑（非纯样式），还需 `npx vue-tsc --noEmit` 通过
- **编辑 `.md` 后**：运行 `npx prettier --write <file>`（本文件与计划书等均适用）
- **Rust 文档注释与签名变更后**：`cargo doc --no-deps` 确认无 broken intra-doc link（可与其他检查合并跑）
- **提交前全量门禁**：`cargo fmt --check` + `cargo clippy -D warnings` + `cargo test --workspace` + `npm run build`

## 工程原则（设计哲学，所有项目通用）

> 设计哲学总纲（18 条 / 5 大类，与用户级 instructions.md 同源）；下文"代码规范"为项目细则，两者冲突时以本项目边界为准。

### 核心思想

- 以第一性原理思考问题：理解需求背后的真实目标，而非直接套用已有模式或技术方案。
- 优先解决本质问题，避免为假设中的未来需求提前设计复杂系统。
- 在保证长期可维护性的前提下，选择当前最简单、可靠、清晰的实现方案。

### 简洁与设计

- 遵循 KISS：优先选择简单直接的实现，避免不必要的复杂度。
- 遵循 DRY：避免重复逻辑，但不要为了消除少量重复而创建过度抽象。
- 遵循 SOLID 思想：职责清晰、降低模块耦合，提高可维护性和扩展能力。

### 架构

- 不长期保留废弃方案：优先删除过时代码，而不是增加兼容层、fallback 或临时迁移逻辑。
- 不进行未经验证的架构设计：避免提前引入抽象、配置和间接层。
- 从最小可工作的版本开始逐步演进，每次修改建立在已有可运行系统之上。
- 永远不要用未来可能需要的复杂性，牺牲当前产品的可用性。

### 代码质量

- 保持模块职责明确，避免一个模块承担过多职责。
- 优先使用成熟、稳定、维护良好的第三方库，而不是重复造轮子。
- 使用项目已有依赖解决问题之前，不要随意新增依赖。
- 在引入新方案前，先检查已有代码、依赖、文档和能力。
- 避免为了"看起来更优雅"而增加实际复杂度。

### 工程决策

- 优先选择长期可维护的方案，而不是只能临时运行的解决方案。
- 代码应该服务于业务目标，而不是为了展示技术复杂度。
- 如果简单方案已经满足需求，不要主动升级为复杂方案。

## 函数注释规则（Rust / TypeScript）

- **Rust**：每个函数定义上方紧跟 `///` 文档注释（中文），说明用途和核心逻辑（1-3 行）；公开 API 的参数、返回值、panic 条件用 `/// # 参数` / `/// # 返回` / `/// # Panics` 小节补充；**禁止无文档注释的 `pub` 函数**
- **Rust 模块**：每个 `.rs` 文件顶部用 `//!` 模块注释说明文件职责、设计理由（为什么这样做）、关联的配置或外部依赖
- **TypeScript / Vue**：导出函数与组合式函数用 `/** */` 中文注释说明用途和核心逻辑（1-3 行）；复杂内部函数同样注释
- **禁止以注释替代类型**：能用类型签名表达的约束（`Option` / `Result` / 联合类型）不写进注释
- 单行 `//` 注释用于解释"为什么"，不复述代码本身做什么

## 代码约定（Rust + Vue/TS）

- **注释语言**：所有注释与文档注释使用中文；注释符号：Rust 用 `//` `///` `//!`，TS/Vue 用 `//` `/** */`，禁止其他语言注释符号
- **命名风格**：Rust 由编译器强制——函数/变量 `snake_case`、类型 `CamelCase`、常量/静态 `UPPER_CASE`；TS/Vue 变量函数 `camelCase`、类型/组件 `PascalCase`、常量 `UPPER_CASE`
- **私有性**：Rust 模块内部使用收敛可见性（默认私有，跨 crate 用 `pub(crate)`，慎用 `pub`）；TS 模块内部用 `_` 前缀表示内部私用，外部模块不应直接调用
- **入口**：每个可独立运行的二进制 crate 有 `fn main()`；CLI 参数用 clap 解析，不在 main 里手写 argv 解析
- **类型**：Rust 强类型天然覆盖；TS 禁止 `any`，优先显式标注，接口用 `interface`/`type`，跨进程数据结构用 `serde` / 类型定义单一来源
- **数据结构**：配置聚合优先用 Rust struct + `serde::Deserialize`（对应 game.json / patch manifest / save meta.json），不做散装 HashMap 取值
- **use / import 顺序**：Rust——标准库 → 第三方 crate → 本地 crate/模块，组间空行（rustfmt 默认分组）；TS——外部包 → 内部模块，组间空行
- **字符串格式化**：Rust 优先 `format!("{var}")` 内联变量写法；TS 优先模板字符串，避免 `+` 拼接
- **集合构建**：Rust 优先 iterator 适配器链（`map`/`filter`/`collect`）而非手写 for 循环构建集合
- **布尔判断**：用 `if x:` / `if not x:`（TS）/ `if x` / `if !x`，而非与字面量显式比较
- **空值判断**：Rust 用 `Option::is_none()` / `is_some()` 及 `match`/`if let`；TS 用 `x == null` / `x != null` 同时覆盖 null 与 undefined，禁止与 `undefined` 单独比较
- **错误处理**：Rust 禁止在业务代码散落 `unwrap()`/`expect()`——库 crate 用 `thiserror` 定义错误类型，应用 crate 顶层用 `anyhow`；错误必须向上传播或明确记录，禁止空 `catch`/`let _ =` 吞错；TS 避免 `catch {}`，至少记录日志
- **行长度**：每行尽量不超过 100 字符（rustfmt `max_width = 100` 默认值；超过时在运算符或逗号后换行）
- **格式化自动化**：空格、缩进、引号、尾逗号等排版细节**以 rustfmt 与 prettier 输出为准**，不手工对齐；普通字符串双引号（TS 由 prettier 配置保证）
- **路径处理**：Rust 强制使用 `std::path::Path` / `PathBuf`，禁止手拼字符串路径；TS 使用 `path.join` / `URL`，禁止字符串拼接路径
- **临时文件**：所有临时生成的脚本/文件必须写入项目根目录下的 `.temp/` 文件夹（gitignore），禁止散落在源码目录
- **文件修改必须用 edit 工具**：修改既有文件（.rs/.ts/.vue/.md/.json/.toml）一律用 edit 的精确 oldString/newString 替换，禁止用 python -c 或 PowerShell 脚本做内容替换（易踩引号/缩进坑）；新建 .temp 探针脚本不受限
- **版本双轨**：机器版本只存根 `Cargo.toml` 的 `[workspace.package] version`（三段式 X.Y.Z，Cargo 强制 semver）；对外正式文本（commit 标题、README 徽章、发布说明）一律写四段式 `VX.Y.Z.R`——前三段与 workspace version 一致，第四段 R 为修订号，只随提交历史记录、不入任何配置文件（递增规则见"Commit 提交规范"）；**README 徽章版本随提交同步推进**——每次 git 提交前必须把 `README.md` 顶部 Version 徽章更新为本次的四段式版本号，防止徽章滞后于代码；`tauri.conf.json`、`capsule-pack` 产物、`game.json` 的 version 均从 workspace version 派生或注入，禁止第二处硬编码。**豁免（FIX001.25 定案 2026-09-08）**：`retro/game.json` 的 `version` 语义 = 游戏自身版本（对外展示，非产品版本）；`launcher/package.json` 的 `version` 为 npm 生态必填字段、不参与发布——两者豁免于 workspace 单一来源，不派生不注入
- **Shell 命令**：执行命令前先检测当前 shell（`echo $SHELL` 或检查系统），本机 Windows 环境；避免使用 Linux-only 或 PowerShell-only 工具，优先跨 shell 兼容命令
- **PowerShell 中文编码**：bash 工具的 pwsh 会话带 `-NoProfile`，不加载 `$PROFILE`，输出中文前必须先设置编码：`[Console]::OutputEncoding = [System.Text.Encoding]::UTF8;`
- **Git 操作**：未经用户明确要求，不得擅自执行 `git add`、`git commit` 或任何其他 Git 写操作

## Commit 提交规范

- **标题行**：`<type>: V<版本>，<摘要>`——版本为四段式 `VX.Y.Z.R`（如 `V0.1.0.1`）：前三段与根 `Cargo.toml` 的 workspace `version` 一致，第四段 R 为修订号（不入配置文件，见下条）；摘要一句话概括核心
- **版本递增**：`feat`/`fix`/`refactor`/`perf` 提交前先 bump 第四段 R（R+1）；前三段在 `Cargo.toml` 中推进时 R 回到 1；`docs`/`test`/`style`/`chore` 不强制
- **type 全集**（conventional 风格）：`feat` 新功能 / `fix` 修复 / `refactor` 重构（行为不变）/ `perf` 性能 / `docs` 文档 / `test` 测试 / `style` 格式 / `build` 构建依赖 / `ci` CI / `chore` 杂项 / `revert` 回滚
- **正文**（默认必写，仅 docs/style 微调可省）：`- ` 列表逐条"具体做了什么"，每条一个功能块、一行自然中文、≤ 100 字符；体例＝`主题：内容（细节；细节）`，同主题多细节用 `；` 分隔。样例：`- capsule-core::monitor 观察库：DOSBox Staging 0.83 REST 客户端（cpu/memory/1MB 快照，错误归一化 BadStatus/Http）`
- **禁止项**：内部编号（A0.1/B1.5/FIX1.1 等任务编号）、验证/回归数字（如"全量回归 506 项通过"）、英文混排描述
- **提交范围**：一个版本的所有连带改动一次提交（源码 + 配置 + 文档同步）
- **GitHub 身份与远程**：远程 `git@takechance:pyapple12/CapsuleRetro.git`（SSH 别名 `takechance` = pyapple12 账号，密钥 id_b_takechance，经 socks5 127.0.0.1:10808）；仓库级身份 `git config user.name pyapple12` + `user.email takechance_bao@188.com`（覆盖全局 YiYi 身份，与 MrBoard 同款做法）
- **流程**：每次提交前 AI 必须 ①`git status` + `git diff`（含 `--stat`）核对改动范围无误；②核对 `README.md` 顶部 Version 徽章是否已推进到本次的四段式版本号（`docs`/`chore` 等不递增 R 的提交也要核对，不一致先改再提交）；③草拟完整 commit 内容（add 清单 + 标题 + 正文）交用户确认；用户审阅后自行执行 `git add`/`git commit`/`git push`（AI 不执行任何 git 写操作，除非用户当轮明确授权）

## 错误策略

主线：**严格抛错**——配置缺失/非法输入直接返回错误（Rust 返回 `Result` 并定义明确错误类型；TS 抛出具体 Error），不静默兜底、不降级假成功。

容错白名单（初始为空；仅产品明确允许的边界可容错，**新增容错须先在此登记**，写明场景、降级行为与理由）：

- **watch 轮询瞬时失败**（PL001 登记，2026-09-05）：场景——`xmonitor watch` 长驻轮询 DOSBox webserver 时的单次瞬时失败（连接拒绝/超时）；降级行为——单次失败打印警告并计数、跳过本轮继续轮询，连续 3 次失败以退出码 1 终止；理由——轮询型伴生进程必须对瞬时抖动免疫，但失败路径最终显式退出、不吞错，符合"严格抛错 + 登记制"精神。
- **HUD 轮询瞬时失败**（PL002 登记，2026-09-06）：场景——launcher 内 HUD 线程按 500ms 轮询 webserver 读取关注条目；降级行为——单次失败跳过本轮（前端保留上次快照），连续 5 次失败向 UI 发停滞事件并停止轮询（显式暴露、不吞错）；理由——与 watch 同类的伴生轮询，瞬时抖动不应打崩 UI，停滞最终显式呈现给用户。
- **REST Registers 缺字段兜 0**（FIX001.17 登记，2026-09-08）：场景——`cpu_state` 解析时 DOSBox 响应可能省略部分寄存器字段（实测无 `es`）；降级行为——`#[serde(default)]` 缺字段置 0，不报错；理由——上游 API 字段可选属事实契约，缺字段非错误状态。
- **xmonitor 快照存档降级**（FIX001.17 登记，2026-09-08）：场景——`search` 交互拍快照后的本地存档（目录创建失败/目录定位失败退化相对 `.temp`）；降级行为——存档失败仅警告、快照内存数据照常参与差分；理由——存档是离线回放的便利品，不是功能必需，拍取与差分本身不受影响。
- **窗口离屏托管退化形态**（FIX001.17 登记，2026-09-08；**PL005 退役（2026-09-09）**——fork 改为 SDL 窗口创建即隐藏，原生窗口不再存在，托管链路整体移除）：场景——`embed_offscreen` 轮询重试（500ms×10）仍找不到/改不动 SDL 窗口；降级行为——打日志，游戏保留本机顶层窗口形态继续运行；理由——托管是统一窗口形态的增强层，帧流显示与输入注入均不依赖窗口位置，会话功能完整。
- **GameCanvas 帧流瞬时失败**（FIX001.7/17 登记 2026-09-08；**FIX002.5 改写（2026-09-10）**——30fps 轮询已随 PL005 退役，机制改为 chunked 流消费，条目同步现行语义）：场景——前端 chunked 帧流消费循环中单次中断（fetch 失败/流异常关闭/解析失败/缓冲超限）；降级行为——console.warn 计数并按 1 秒退避重连，连续 5 次显式置错误态并停循环；理由——与 watch/HUD 同类的伴生长连接，瞬时抖动不应打崩画面，最终显式暴露。
- **文件日志写失败静默**（FIX001.17/22 登记，2026-09-08）：场景——`logging.rs` 追加写 launcher.log 失败（磁盘满/句柄失效）；降级行为——静默忽略该行，不影响业务流程；理由——日志组件不得反噬主流程；init 未调用时为空操作同此登记。
- **save 档案损坏降级**（PL003.5 登记，2026-09-08）：场景——启动解包 `GameName.save` 时检测损坏（非 zip / 缺 meta / meta 非法）；降级行为——损坏档案备份为 `<原名>.save.corrupt`（仅保留最新一份），以全新空 save 继续运行并 ERROR 级落日志；理由——存档损坏若严格报错会让游戏整体不可玩，降级保住"可继续游玩"，损失仅限已损坏数据；其余错误（IO/越界条目）仍严格上抛不走降级。
- **dosbox.log 轮转失败继续追加**（PL003.14 登记，2026-09-08）：场景——启动时 dosbox.log 超过 5MB 触发轮转但 rename 失败（文件被占用等）；降级行为——ERROR 级落日志，本轮放弃轮转继续追加；理由——轮转只影响日志体积不影响功能，下轮启动自动重试。
- **save 解包未知前缀条目忽略**（FIX002.3 登记，2026-09-10）：场景——save zip 内出现 `save/` 前缀之外的普通文件条目（资源管理器/7-Zip 重打包的显式目录条目、预留 `config/` 节、macOS `__MACOSX/` 噪声）；降级行为——跳过不落工作副本（显式目录条目同理），不报错；理由——良性结构噪声不应阻断启动，此前按 UnsafeEntry 严格拒绝会让重打包过的 save 永久无法启动；路径穿越形态仍严格拒绝不走本条。
- **save 解包中断残留清理失败静默**（FIX002.12 登记，2026-09-10）：场景——unpack 中途失败后删除已写入文件时 remove 失败（文件被占）；降级行为——eprintln 警告、不掩盖主错误照常上抛；理由——清理是错误路径上的次要动作，残留由每次启动全量重建自愈。
- **payload extract_once 标记读取失败重释放**（FIX002.4 登记，2026-09-10）：场景——读取/解析 `resources/payload.json` 标记失败（文件被锁/权限/半写）；降级行为——按"标记无效"清空重释放（自愈，秒级）；理由——重释放幂等且结果正确，优于阻断启动；配合条目落盘抽查（FIX002.9）不放大释放频率。
- **头像目录杂物文件跳过**（FIX003.30 登记，2026-10-06）：场景——faces 目录混入非 charNN.png 命名的杂物文件（Thumbs.db/.DS_Store 等，资源管理器浏览即生成）；降级行为——logging::warn 留痕后跳过该文件继续加载其余头像；理由——杂物不是头像数据错误，严格拒绝会让一个缓存文件废掉全部头像，目录由产品资产管控。
- **get_faces 失败前端占位**（FIX003.30 登记，2026-10-06）：场景——前端 invoke get_faces 失败（目录缺失/读取错误等）；降级行为——console.warn + 头像槽位回落"—"占位，不阻断 HUD 状态条；理由——头像是增强信息，缺失不应打崩 HUD，失败已在控制台可见。
- **Big5 解码器探测回落**（FIX003.30 登记，2026-10-06）：场景——运行环境 TextDecoder 不支持 big5（能力探测构造失败）；降级行为——console.warn 后姓名回落 utf-8 解码；理由——解码器可用性是环境能力而非数据错误，回落保证姓名仍可显示。

## 素材与环境陷阱

- **retro/ 原版游戏文件只读**：打包、测试、调试一律不得写入 `retro/`；补丁与存档只作用于运行时释放的工作副本（对应"补丁纪律"）
- **.archived/ 只读**：mm4/mm5 的 zip 为原始素材，解包到临时目录操作，不原地修改
- **DOSBox conf 逐游戏调优**：不同 DOS 游戏配置差异大，game.json 暴露 dosbox_conf，调优只改 conf 不改模拟器调用方式
- **Windows 杀软误报**：开发期单文件 exe 可能被误报，调试用文件夹绿色版形态；发布对策见计划书 §8
- **Tauri sidecar 路径**：DOSBox 二进制随包分发，开发与打包形态的 sidecar 解析路径不同，改动路径逻辑时两种形态都要验证
- **PowerShell 中文编码**：见"代码约定"同名额
