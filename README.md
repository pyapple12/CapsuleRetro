# CapsuleRetro —— 老 DOS 游戏的现代包装系统

[![Version](https://img.shields.io/badge/Version-0.1.0-blue.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-1.96-orange.svg)](https://www.rust-lang.org)
[![Phase](https://img.shields.io/badge/Phase-0%20POC%20%E8%BF%9B%E8%A1%8C%E4%B8%AD-yellow.svg)](CapsuleRetro_plan.md)

把任意 DOS 游戏打包成"看起来像移植的现代游戏"，三平台（Windows / macOS / Linux）通用。产物 = 执行文件 + save 文件，两个文件走天下。

| 卖点                  | 说明                                                  |
| --------------------- | ----------------------------------------------------- |
| 现代启动器            | 封面、原版/hack 版入口、攻略阅读                      |
| 可隐藏界面的 DOS 模拟 | 全屏 4:3 保持比例，空白区自定义图片填充               |
| 外部组件伴生          | 实时读取游戏内存（REST），HUD/状态面板/攻略联动       |
| 补丁应用              | hack 版 = 原版副本 + 运行时打补丁，原版文件永不被修改 |
| 随身携带              | 存档 + 设置 + hack 状态打包为单个 save 文件           |

> 当前状态：Phase 0（POC 技术验证）进行中——PL001「外部组件挂载」已完成并实测通过，方案与数据见 `z.plan.md` 附录；任务进度见 `x.progress.md`。

## 技术栈

| 组件       | 选型                                                         |
| ---------- | ------------------------------------------------------------ |
| 启动器外壳 | Tauri 2（Rust 后端 + WebView）——未建                         |
| 前端 UI    | Vue 3 + TypeScript + Vite——未建                              |
| 模拟器     | DOSBox Staging v0.83.0（sidecar，随包分发）                  |
| 核心库     | Rust crate `capsule-core`（save/补丁/路径 + monitor 观察库） |
| 打包器     | Rust CLI `capsule-pack`——未建                                |

## 快速开始

### 环境要求

- Rust toolchain（cargo 1.96+）
- Windows 10/11（macOS / Linux 支持在计划内）
- DOSBox Staging 已随仓库分发（`assets/dosbox/`，解压 `zip` 即得运行时）

### 构建与测试

```bash
cargo test --workspace     # 全量测试（19 项）
cargo build --release      # 构建
```

### 运行游戏（开发态，挂工作副本不碰原版）

```bash
# 首次：解压 sidecar
#   assets/dosbox/dosbox-staging-windows-x64-v0.83.0.zip → assets/dosbox/dosbox-staging/

assets/dosbox/dosbox-staging/dosbox-staging-v0.83.0/dosbox.exe \
  --set webserver_enabled=on --set webserver_port=18086 \
  -conf .temp/xeen-test.conf
```

### 观察组件（xmonitor）

```bash
cargo run --bin xmonitor -- --profile retro/memory_map.json --port 18086 watch
# 实时显示：全队 HP/MP/EXP、随身金币/宝石、银行存款

cargo run --bin xmonitor -- search          # 差分搜索内存偏移（适配新游戏用）
cargo run --bin xmonitor -- bench           # REST 读取性能基准
```

## 项目结构

```
CapsuleRetro/
├── Cargo.toml               # workspace 根（版本号唯一来源：[workspace.package] version）
├── capsule-core/            # 核心 crate：monitor 观察库 + src/bin/xmonitor 诊断 CLI
│   └── tests/               # 集成测试（档案解析 / REST stub / 差分）
├── assets/dosbox/           # DOSBox Staging sidecar（入库 zip；解压目录已 gitignore）
├── retro/                   # 根 = 适配层：memory_map.json（22 条目内存档案）、
│   │                        #   xeen-guide.md（攻略）、zhuyin-keyboard.md（注音对照）
│   └── XEEN/                # 纯原版游戏文件（字面意义只读）
├── AGENTS.md                # 项目规范（AI 协作约定）
├── CapsuleRetro_plan.md     # 总体规划（Phase 0-4）
├── z.plan.md                # 方案与审计归档（PL001 实测结论在附录）
├── x.progress.md            # 任务清单
├── y.problems.md / w.study.md
└── .temp/                   # 临时文件（gitignore）
```

架构一句话：`capsule-core::monitor` 通过 DOSBox 内置 REST API 原子读取模拟器内存，按 `retro/memory_map.json` 档案解码为游戏数值；launcher（Tauri，未建）未来以同库渲染 HUD。

## 文档地图

- `CapsuleRetro_plan.md`：总体规划与 Phase 划分
- `AGENTS.md`：工程原则、代码规范、提交规范（AI 协作必读）
- `z.plan.md`：专题方案与审计归档（附录 PL001 = 外部组件挂载实测结论）
- `x.progress.md`：任务清单与进度
- `retro/zhuyin-keyboard.md`：注音输入对照（汉化版游戏内打字用）
