# CapsuleRetro — Agent Guide

老 DOS 游戏现代包装系统：把任意 DOS 游戏打包成"看起来像移植的现代游戏"，三平台（Windows / macOS / Linux）通用。产物 = 执行文件 + save 文件两个文件走天下。总体规划见 `CapsuleRetro_plan.md`。

**当前状态**：Phase 0 进行中——PL001 已完成（`capsule-core` crate + `capsule-core::monitor` REST 观察库 + `xmonitor` 诊断 CLI + XEEN 适配档案 `retro/memory_map.json`，见 z.plan.md 附录 PL001）。已有素材：`retro/XEEN/`（《魔法门 XEEN》DOS 版，含 CHINFONT 汉化字体与 CMM4.PAT 补丁）、`retro/` 根适配层（xeen-guide.md 攻略、zhuyin-keyboard.md 注音对照、memory_map.json 内存档案）、`.archived/`（mm4/mm5 原版 zip）、`.agents/skills/`（项目自建 skill）。launcher / capsule-pack 未建。下文"启动命令"与"架构要点"按计划书预写，代码落地后如与实现不一致，以实现为准并回改本文件。

## 技术栈

| 组件       | 选型                           |
| ---------- | ------------------------------ |
| 启动器外壳 | Tauri 2（Rust 后端 + WebView） |
| 前端 UI    | Vue 3 + TypeScript + Vite      |
| 模拟器     | DOSBox Staging（sidecar）      |
| 打包器     | Rust CLI（capsule-pack）       |
| 核心库     | Rust crate（capsule-core）     |

## 启动命令（规划）

```bash
cargo run -p launcher                            # 开发运行启动器（或 cargo tauri dev）
npm run dev                                      # 前端热更开发
cargo run -p capsule-pack -- game.json -o dist/  # 打包器：game.json → 游戏包
cargo test --workspace                           # 全量测试
npm run build                                    # 前端构建校验
```

依赖：Rust toolchain（cargo 1.96+）、Node.js（26+）、DOSBox Staging 二进制（sidecar，随包分发）。

## 架构要点（按计划书预写）

- **两个文件架构**：`GameName.exe/.app/.AppImage`（只读：DOSBox + 游戏 + 启动器 + 攻略 + 补丁）+ `GameName.save`（唯一用户数据：存档 + 设置 + hack 状态，zip 格式）
- **capsule-core**：save 打包/解包（原子替换）、补丁应用、路径管理——launcher 与 packer 共用，逻辑只写这一份
- **launcher**（Tauri 2 + Vue 3）：UI、DOSBox 进程管理（sidecar）、全屏游戏会话、攻略 Markdown 渲染
- **packer**（capsule-pack CLI）：读 game.json → 校验 → 资源内嵌 → 生成三平台产物
- **save 生命周期**：启动解压到临时工作区 → 运行 → 退出原子回写（防损坏，丢失可自愈）
- **补丁纪律（产品核心，代码与 Agent 操作双重遵守）**：hack 版 = 原版文件副本 + 运行时应用补丁；**原版游戏文件永不被修改**，补丁只作用于释放出来的工作副本
- **4:3 填充**：Windows 用 SetParent 嵌入，macOS/Linux 用覆盖层点击穿透窗口（最大技术风险，见计划书 §8）
- **运行时目录**：系统缓存目录（如 `%LOCALAPPDATA%/CapsuleRetro/<GameName>/`）存放释放的 game/work/dosbox，丢失可自愈重新释放

## 目录规划

```
Cargo.toml       # workspace 根（version 单一来源；当前 members=["capsule-core"]）
capsule-core/    # Rust crate：save 打包/解包、补丁应用、配置、路径管理
                 #   src/monitor/      REST 观察库（PL001）
                 #   src/bin/xmonitor  诊断 CLI（watch/search/bench）
                 #   tests/            集成测试（cargo test 自动发现）
launcher/        # Tauri 2 应用（src-tauri/ Rust 后端 + src/ Vue 前端）——未建
capsule-pack/    # Rust CLI 打包器——未建
assets/dosbox/   # DOSBox Staging v0.83.0 sidecar（锁版本，进 git 的是 zip；
                 #   解压目录 dosbox-staging/ 为派生产物，已 gitignore）
retro/           # 根 = 适配层（我们编写的文件）：game.json（待建）、
                 #   memory_map.json（内存档案）、xeen-guide.md（攻略）、
                 #   zhuyin-keyboard.md（注音对照）
                 # 子目录（如 retro/XEEN/）= 纯原版游戏文件，字面意义只读
.agents/skills/  # 项目自建 skill（audit-project / audit-report / progress-task），ZCode workspace 级扫描目录
.archived/       # 归档素材（mm4.zip / mm5.zip，只读）
.temp/           # 临时脚本与文件（gitignore）；探针、快照、验证记录放这里
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
- **版本双轨**：机器版本只存根 `Cargo.toml` 的 `[workspace.package] version`（三段式 X.Y.Z，Cargo 强制 semver）；对外正式文本（commit 标题、README 徽章、发布说明）一律写四段式 `VX.Y.Z.R`——前三段与 workspace version 一致，第四段 R 为修订号，只随提交历史记录、不入任何配置文件（递增规则见"Commit 提交规范"）；`tauri.conf.json`、`capsule-pack` 产物、`game.json` 的 version 均从 workspace version 派生或注入，禁止第二处硬编码
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
- **流程**：每次提交前 AI 必须 `git status` + `git diff`（含 `--stat`）核对改动范围无误，再草拟完整 commit 内容（add 清单 + 标题 + 正文）交用户确认；用户审阅后自行执行 `git add`/`git commit`/`git push`（AI 不执行任何 git 写操作）

## 错误策略

主线：**严格抛错**——配置缺失/非法输入直接返回错误（Rust 返回 `Result` 并定义明确错误类型；TS 抛出具体 Error），不静默兜底、不降级假成功。

容错白名单（初始为空；仅产品明确允许的边界可容错，**新增容错须先在此登记**，写明场景、降级行为与理由）：

- **watch 轮询瞬时失败**（PL001 登记，2026-09-05）：场景——`xmonitor watch` 长驻轮询 DOSBox webserver 时的单次瞬时失败（连接拒绝/超时）；降级行为——单次失败打印警告并计数、跳过本轮继续轮询，连续 3 次失败以退出码 1 终止；理由——轮询型伴生进程必须对瞬时抖动免疫，但失败路径最终显式退出、不吞错，符合"严格抛错 + 登记制"精神。

## 素材与环境陷阱

- **retro/ 原版游戏文件只读**：打包、测试、调试一律不得写入 `retro/`；补丁与存档只作用于运行时释放的工作副本（对应"补丁纪律"）
- **.archived/ 只读**：mm4/mm5 的 zip 为原始素材，解包到临时目录操作，不原地修改
- **DOSBox conf 逐游戏调优**：不同 DOS 游戏配置差异大，game.json 暴露 dosbox_conf，调优只改 conf 不改模拟器调用方式
- **Windows 杀软误报**：开发期单文件 exe 可能被误报，调试用文件夹绿色版形态；发布对策见计划书 §8
- **Tauri sidecar 路径**：DOSBox 二进制随包分发，开发与打包形态的 sidecar 解析路径不同，改动路径逻辑时两种形态都要验证
- **PowerShell 中文编码**：见"代码约定"同名额
