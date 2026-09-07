# 项目方案与审计归档（z.plan.md）

> 文件职责：方案文档与审计归档。与 `x.progress.md`（任务清单）、`CapsuleRetro_plan.md`（总体规划）分工：总体规划不动，方案演进与审计记录都落在本文件。
> 结构：一、已完成 ✅ → 二、待完成 → 三、主题规划（按需）→ 四、审计观察项豁免定案清单 → 附录 A{NNN}（审计报告，由 audit-report 归档时生成）→ 附录 PL{NNN}（专题方案）。

## 一、已完成 ✅

- **PL001 观察库与内存档案**（2026-09-05 收口）→ 附录 PL001
- **PL002 launcher × DOSBox 结合**（2026-09-07 主线收口，备线休眠 [problems#16]）→ 附录 PL002
- **FIX001 第 1 轮全量审计修复**（2026-09-08 收口）→ 附录 A001
- **PL003 save 生命周期 + 补丁闭环**（2026-09-08 收口）→ 附录 PL003

## 二、待完成

- （暂无——packer（capsule-pack）与三平台打包是下一个大件，未立项）

## 三、主题规划

- （按需）跨附录主题出现时在此登记；远期与遗留项见 y.problems.md

## 四、审计观察项豁免定案清单

> 豁免唯一权威源：已定案项审计时（audit-project）不再重复报告。新定案条目由归档环节（audit-report）经用户确认后追加。分级：①**永久豁免**——不再报告不再讨论；②**条件豁免**——标注触发条件，条件变化时重新评估。

（暂无）

---

## 附录 PL001：DOSBox 外部组件挂载验证（2026-09-05）

> 背景：产品方向新增"外部组件与游戏会话伴生"。DOSBox Staging v0.83.0 内置 HTTP REST API（`[webserver]` 节，默认关闭，仅绑定 127.0.0.1），可在游戏运行中原子读取 CPU 寄存器与底层内存——已于 2026-09-05 在 XEEN 运行中实测可用（端点清单、实测输出与边界结论见 w.study.md §3.1）。官方标注该 API 主要针对 `core = normal` 的 16 位实模式应用测试，XEEN 正是 16 位实模式游戏，匹配度最好；v1 API 仅只读（无内存写入、无按键注入、无运行时调速）。
> 关键洞察：观察型组件（HUD/状态监控/翻译/金手指搜索）只需要"稳定读"——读通了，产品方向的地基就成立了；控制型（写内存）走调试器构建或上游 feature request，不阻塞本期。
> 目标：以最小代码量回答"游戏运行时，外部程序能否稳定看清游戏内存"，沉淀首个游戏适配档案（memory_map）与偏移查找方法论，产出可复用的独立诊断工具（A 壳 CLI）。
> 状态：✅ 已完成（2026-09-05；实测结论见下方附录；PL001 任务清单见 x.progress.md）

### 实现措施（按层拆解到文件/函数级）

#### 1. 工程层：最小 cargo workspace

- 根 `Cargo.toml`：`[workspace]` members = `["capsule-core"]`；`[workspace.package]` `version = "0.1.0"`（版本单一来源，edition 2021）
- `capsule-core/Cargo.toml`：依赖 `serde`（derive）、`serde_json`、`ureq`（开 `json` feature，同步轻量 HTTP，避免引入 tokio 全家桶）、`thiserror`；dev-dependencies 不引入测试框架以外的东西
- `capsule-core/src/lib.rs`：`//!` 模块注释（crate 职责：save 打包/解包、补丁应用、路径管理、monitor REST 观察库）+ `pub mod monitor;`
- 工程门禁：`cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo check` + `cargo test`（AGENTS.md 硬性工作流）

#### 2. 库层 `capsule-core::monitor`（文件级拆解）

- **`src/monitor/mod.rs`**：模块组织 + `//!` 说明（为什么：launcher 与 CLI 双壳共用的观察逻辑只写这一份）
- **`src/monitor/profile.rs`**：档案解析，定义与 `retro/memory_map.json` 一一对应：

  ```json
  {
    "game": "xeen",
    "poll_interval_ms": 500,
    "entries": [
      {
        "name": "hp",
        "segment": "0x1234",
        "offset": "0x5678",
        "len": 2,
        "value_type": "u16le"
      }
    ]
  }
  ```

  - `struct MemoryMap { game: String, poll_interval_ms: u64, entries: Vec<Entry> }`
  - `struct Entry { name: String, segment: u16, offset: u16, len: u8, value_type: ValueType }`
  - `enum ValueType { U8, U16LE, U32LE }` + `fn decode(&self, bytes: &[u8]) -> Result<u64, ProfileError>`（按宽度校验 bytes 长度，小端解码）
  - `#[derive(Error)] enum ProfileError`（thiserror）：Json(serde_json::Error)、UnknownValueType(String)、WidthMismatch { expected, actual }

- **`src/monitor/rest.rs`**：REST 客户端

  - `struct RestClient { base_url: String, timeout: Duration }`，`fn new(port: u16) -> Self`（base 固定 `http://127.0.0.1:{port}`）
  - `fn cpu_state(&self) -> Result<CpuState, MonitorError>`：GET `/api/v1/cpu/state`，`struct CpuState { registers: Registers }` 只取需要的字段
  - `fn memory(&self, segment: Option<u16>, offset: u32, len: u32) -> Result<Vec<u8>, MonitorError>`：GET `/api/v1/memory/{seg}/{off}/{len}`（seg 省略时走一参形式），响应为原始字节流
  - `fn snapshot_lower_memory(&self) -> Result<Vec<u8>, MonitorError>`：`memory(None, 0, 0x100000)` 全底层内存快照（1MB），差分搜索的原料
  - `#[derive(Error)] enum MonitorError`：Http(ureq::Error)、BadStatus(u16)、Io(std::io::Error)
  - **无重试**（严格抛错主线）：单次调用失败一律上抛，由 CLI 轮询层决定计数策略（见错误策略节）

- **`src/monitor/diff.rs`**：纯函数差分

  - `fn diff_snapshots(prev: &[u8], curr: &[u8]) -> Vec<u32>`：返回发生变化的地址（长度不等视为调用方错误，断言前置）
  - `fn filter_by_change(prev, curr, again: &[u8]) -> Vec<u32>`：三快照二次过滤（在首轮变化地址中挑"再次变化"的），供收敛用

#### 3. CLI 壳层 `capsule-core/src/bin/xmonitor.rs`

- clap derive 定义：`--port <u16>=18086`、`--profile <Path>`、`--interval <u64>`（毫秒，覆盖档案值），子命令：
  - `watch`：加载档案 → 循环（interval）取值 → 表格打印 `name | 原始字节 | 解码值`；每 20 轮输出统计行（已轮询次数 / 失败次数 / 平均耗时）；Ctrl+C 退出
  - `search`：差分搜索助手——回车拍快照 s1 → 提示用户在游戏内改变数值 → 回车拍 s2 → 打印 `diff(s1,s2)` 候选表 → 可选第三快照 s3 调 `filter_by_change` 收敛
  - `bench`：对 1KB / 64KB / 1MB 三档各请求 20 次取平均耗时输出（V5 原料）
- 退出码约定：0 正常；非 0 = 连续失败退出（见错误策略）

#### 4. 适配层：XEEN 内存偏移实测（差分搜索操作规程）

1. 启动：`dosbox.exe --set webserver_enabled=on --set webserver_port=18086 -conf .temp/xeen-test.conf`（挂载 `.temp/xeen-work/` 工作副本，retro/XEEN/ 不触碰）
2. HP 定位：进游戏有队伍后 `xmonitor search` → 拍 s1 → 游戏内受伤（数值下降）→ 拍 s2 → 候选表按 u16le 假设过滤 → 恢复/再受伤拍 s3 收敛 → 锁定唯一地址
3. 金币定位：同法，用买入/卖出两个方向的变化做双向交叉验证，确认地址独立于 HP
4. 锁定标准：**三组独立变化（至少一次方向相反）后候选收敛为 1 个地址**，且 `watch` 实时值与游戏画面数值肉眼一致
5. 落盘：地址写入 `retro/memory_map.json`（retro/ 根适配层；`retro/XEEN/` 原版目录一个字节不动），再跑 `watch` 复验一次

#### 5. 错误策略与容错登记（AGENTS.md 错误策略衔接）

- 主线严格抛错：解析错误、HTTP 错误、状态码非 200 一律 `Result` 上抛，不静默兜底
- **唯一新增容错边界（须在 AGENTS.md 容错白名单登记）**：`watch` 长轮询场景下单次瞬时失败（连接拒绝/超时）打印警告并计数，**连续 3 次失败以非零码退出**——场景：DOSBox 偶发慢响应不应杀死监控；降级行为：跳过本轮；理由：长驻进程对瞬时抖动免疫是产品需求，且失败最终仍显式退出、不吞错

### 验证方案（确保代码正确性，V1–V5 全部可执行、可断言）

| #   | 层级        | 检验内容                         | 具体手段与通过标准                                                                                                                                                                                                   |
| --- | ----------- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| V1  | 档案解析    | ValueType 解码、MemoryMap 解析   | TDD 先写测试再实现，用例：①u16le bytes `[0x39,0x05]` → 1337 ②u8/u32le 各一例 ③len 与 value_type 宽度不匹配 → WidthMismatch ④缺 `poll_interval_ms` → Json 错误 ⑤未知 value_type → UnknownValueType；`cargo test` 全过 |
| V2  | REST 客户端 | 响应解析正确、错误分支传播不吞错 | `tests/rest_stub.rs`：`std::net::TcpListener` 起本地 stub 返回预录 cpu/state JSON 与 memory 字节流，断言逐字节一致；另断言 404 → `BadStatus`、坏 JSON → `Http/Json` 错误变体                                         |
| V3  | 偏移正确性  | CLI 显示值 == 游戏内实际值       | 受伤/恢复 ≥3 轮 + 买入/卖出 ≥2 轮独立场景，每轮记录 `场景 \| 游戏值 \| CLI 值 \| 一致?` 到 `.temp/verify_pl001_v3.md`；**一致率 100%** 才通过                                                                        |
| V4  | 稳定性      | 长轮询零错误                     | `xmonitor watch` 连续运行 ≥10 分钟（interval 500ms，轮询次数 >1000）：失败计数 0、退出码 0                                                                                                                           |
| V5  | 性能        | 快照读取开销有数                 | `xmonitor bench`：1KB / 64KB / 1MB 各 20 次平均耗时，结果写入本附录"实测结论"                                                                                                                                        |
| —   | 工程门禁    | 静态检查与测试全绿               | `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo check` + `cargo test`                                                                                                                 |

**验收标准**：V3 一致率 100% 且 V4 零错误，方可宣布"外部组件挂载"方向验证通过；任一不达标，在结论中如实记录偏差与原因，不降级宣布通过。

### 拆分 todo

见 x.progress.md「PL001」任务组（15 条子任务，按 工程层/库层/CLI 壳层/适配层/验证收尾 五个 `####` 小节分层，每条含做法与验证方式）。

### 实测结论（2026-09-05，全部验证通过）

**核心结论：观察型外部组件挂载可行——REST → 档案 → 解码 → 实时显示的完整链路在 XEEN 活体上端到端跑通，22 条目读数与游戏画面零偏差。**

#### 锁定的内存地图（retro/memory_map.json，22 条目）

| 区域                                 | 字段               | 物理地址                | 类型                          | 实测轨迹                                 |
| ------------------------------------ | ------------------ | ----------------------- | ----------------------------- | ---------------------------------------- |
| 资源区（等距 4 字节 u32 结构体）     | gold 随身金币      | `0x5AA3E`（5AA3:000E）  | u32le                         | 800→700→750→650，3 变 4 态全对（含反向） |
|                                      | gems 随身宝石      | `0x5AA42`（5AA4:0002）  | u32le                         | 10→8→9→4                                 |
|                                      | bank_gold 银行金币 | `0x5AA46`（5AA4:0006）  | u32le                         | 50→150                                   |
|                                      | bank_gems 银行宝石 | `0x5AA4A`（5AA4:000A）  | u32le                         | 1→6                                      |
| 队员区（步长 0x162 的等距结构体 ×6） | hp{n} 生命         | `0x56FA8 + (n-1)×0x162` | **i16le（有符号，昏迷为负）** | 实测 `fc ff` = -4                        |
|                                      | mp{n} 法力         | 同上 +0x2               | u16le                         | 全队 2/0/2/0/7/9                         |
|                                      | exp{n} 经验        | 同上 **+0x6**           | u16le                         | 16→40/140/40/40/40/40                    |

#### 验证方案结果

| #              | 结果         | 数据                                                                                                                                                     |
| -------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| V1 档案解析    | ✅           | TDD： ValueType 四用例+MemoryMap 四场景先 FAIL 后 PASS；评审后新增裸字符串地址报错、i16le 负值两用例，共 10 项                                           |
| V2 REST 客户端 | ✅           | stub 5 项：预录真实抓包 JSON/字节流、404→BadStatus、连接拒绝→Http                                                                                        |
| V3 偏移正确性  | ✅ 100%      | 22 条目 × 多轮独立变化（含反向、跨区域联动）全部与用户屏幕读数一致；比对表 `.temp/verify_pl001_v3.md`                                                    |
| V4 稳定性      | ✅（有备注） | 10 分钟连续轮询 1120+ 轮，**仅 1 次瞬时失败**——由已登记容错白名单路径按设计吸收（警告+自动恢复），未中断轮询；"零失败"未达成但失败处理机制被实测证明有效 |
| V5 性能        | ✅ 优秀      | 1KB=1.84ms / 64KB=1.70ms / 1MB=6.52ms（20 次均值，零失败）；500ms 轮询 1MB 快照占空比约 1.3%                                                             |
| 门禁           | ✅           | fmt / clippy -D warnings / check / test 全绿（19 项测试）                                                                                                |

#### 方法论沉淀（适配下一款游戏时照此办理）

1. 差分搜索三步法：基线快照 → 游戏内改变数值（至少一轮反向）→ 值过滤 + 二次收敛；快照即事实源，用户口误（本轮实际发生两次）以快照数据排除
2. 定位后先验布局再定类型：等距排布暗示结构体（资源区 4 字节距→u32；队员区 0x162 步长→数组），HP 负值揭示有符号字段
3. 全员同值字段的定位技巧：用"某队员值不同"的对照成员（如 2 号 EXP=140）分辨真偏移与巧合
4. 常量字段无法差分（如未变化前的 EXP=16）：用"全员同值 + 同偏移"扫描法锁定候选，再由真实变化确认

#### code-reviewer 评审与修复记录

评审发现 3 Important + 2 Minor，全部修复并复验：①快照存档吞错且假报成功（改为 create_dir_all + 诚实警告）；②离线回放误入交互询问（改为仅交互模式询问，复测通过）；③bench 容忍失败与严格策略矛盾（改为任一失败即非零退出，复跑通过）；④本地文件错误误报为 HTTP 语境（离线读取单独报错文案）；⑤裸字符串地址静默按十六进制（改为严格报错 + 新增测试）。

#### 偏差记录

- 原计划 HP+金币两项，实际完成 金币/宝石/银行金币/银行宝石/全队 HP/MP/EXP 共 22 条目——覆盖超计划；HP 由"1 号位"扩展为全队六人
- V4 的"失败计数 0"未严格达成（1 次瞬时失败），如实记录，不降级宣布为"零错误"，结论为"容错机制实测有效、监控未中断"

---

## 附录 PL002：Launcher 与 DOSBox 结合——前期准备 + 后期实施（2026-09-05 立项，同日 v2 改造）

> 背景：PL001 已验证外部组件数据通道（REST → 档案 → 解码 → 实时显示端到端跑通，92 条目实测零偏差）。本附录 v1 为"窗口嵌合 POC"，三路线判定（VC0 打样 + T1 四轮探针）产出"画面可行、输入不可行"的结论后，经用户决策（2026-09-05）**T2 fork Staging 立项**，本附录改造为两段式：前期准备（两线并行）+ 后期实施（按 go/no-go 分支）。
> 关键洞察：① 嵌合三路线判死（SetParent 输入失效 / 几何定位形式被否 / libretro 丢 REST），但 T1 探针证明 PrintWindow 帧流完全可行——"看"在应用层已解决，"玩"只能在 Staging 源码内解决（SDL_PushEvent 进程内注入，无焦点依赖）；② fork 的红利是三端点（帧流/输入注入/内存写入）一次到位，顺带解锁内存写入历史问题。
> 目标：前期准备完成 launcher 骨架/编排（game.json v1 真实消费）与 fork 基线复现（go/no-go）；后期实施走主线（三端点 + 统一窗口）或备线（独立窗口），最终交付"launcher 内玩 XEEN：看 / 玩 / HUD"端到端。
> 状态：📋 方案 v2 定稿（2026-09-05）——前期准备待启动（两线并行）；VC0 与三线路判定书见本附录实测结论（历史依据，原文保留）；任务清单见 x.progress.md「PL002」30 条子任务

### 改造说明（v2，2026-09-05）

> v1 为"窗口嵌合 POC"（VC0 打样 + 三路线判定）。判定完结后按用户决策改造：**PL003（fork）不单独立项，并入本附录作为后期实施主线**；原阶段 C 由 fork 线取代（历史任务保留标注）；结构按依赖切分——前期准备两线并行互不阻塞，后期实施按 go/no-go 自动分支（判定权：AI 按树执行、出结论汇报）。

```
前期准备（并行两线）
├─ launcher 线：阶段 A 骨架 → 阶段 B 编排（game.json、conf 生成、Job Object、日志、端口）
└─ fork 线：构建链搭建 → v0.83.0 基线复现 ← ★ go/no-go 判定点
后期实施（按判定分支）
├─ 主线（基线通过）：帧流端点 → 输入注入端点 → 内存写入端点 → 统一窗口集成 + HUD
└─ 备线（构建链失败）：独立窗口模式集成 + HUD 贴边面板
```

### 实现措施（按层拆解到文件/函数级）

#### 1. 前置打样 VC0（✅ 已完成判定 2026-09-05，历史依据）

- `.temp/vc0-reparent.ps1`：PowerShell Add-Type P/Invoke——按 DOSBox 进程 PID 枚举顶层窗口（EnumWindows + GetWindowThreadProcessId，GetClassNameW 含 SDL 辅助确认）→ GetWindowLong 调样式（去 WS_POPUP 加 WS_CHILD）→ SetParent 到 WinForms 无边框宿主窗体 → SetWindowPos 对齐 4:3 区域
- 判定规则：渲染 / 鼠标 / 键盘三项人工验证（见验证表 VC0.1–0.3）——全过则 C 阶段主攻路线一；渲染或输入核心项失败则路线一直接降级为备案，主攻路线二
- 脚本入 `.temp/`（gitignore），结论回写本附录实测结论

#### 2. 前期准备·launcher 线：阶段 A Tauri 2 骨架

- 根 `Cargo.toml`：members 扩为 `["capsule-core", "launcher"]`
- `launcher/src-tauri/`：`Cargo.toml`（tauri 2 + tauri-build，插件只装必需）、`tauri.conf.json`（骨架期普通窗口，嵌合期才改无边框）、`src/main.rs`（tauri::Builder + invoke_handler）、`src/commands.rs`（`launch_game` 阶段 A 占位）
- `launcher/src/`（Vue3 + TS + Vite）：`App.vue`（标题 / "启动 XEEN" 按钮 / 状态行）、`package.json` devDeps 含 `@tauri-apps/cli`（`npx tauri dev`，避免 cargo install tauri-cli 的长编译）
- 工程门禁扩容：AGENTS.md 四件套（cargo fmt/clippy/test + npm build）首次覆盖 launcher；前端侧 prettier + vue-tsc 生效

#### 3. 前期准备·launcher 线：阶段 B sidecar 进程编排

- **`capsule-core/src/game_config.rs`**（新）：`struct GameConfig` + serde Deserialize——字段：`name`、`version`、`dosbox { exe_path, webserver_port, cpu_cycles }`、`assets { memory_map, guide }`、`paths { work_dir }`；`enum GameConfigError`（thiserror：Json / 字段校验）；严格抛错、无默认值兜底（hack 入口 / 多磁盘 mount 留字段位不实现——YAGNI 边界）
- **`retro/game.json`**（新，适配层聚合入口第一版）：XEEN 的上述字段值；launcher 从此不再消费 `.temp` 临时 conf
- **`launcher/src-tauri/src/gameconf.rs`**（新）：读 game.json → 生成 DOSBox conf 写运行时目录；模板内容 = xeen-test.conf 定案值全集（cpu_cycles=5000、core=auto、[mouse] 五键、aspect=auto、webserver、autoexec mount 工作副本）；逐键可断言
- **`launcher/src-tauri/src/commands.rs`**：`launch_game`——生成 conf → spawn DOSBox（`--set webserver_enabled=on --set webserver_port={port} -conf {generated}`）→ stdout/stderr 重定向 `dosbox.log`；`stop_game` + 窗口关闭钩子
- **`launcher/src-tauri/src/jobctl.rs`**（新，`#[cfg(windows)]`）：Job Object——CreateJobObjectW / AssignProcessToJobObject / JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE；保证 DOSBox 崩溃或 launcher 退出时进程树必死（windows crate 依赖：win32 Job/EnumWindows 无 std 替代，属必要新增）
- 端口策略：TcpListener bind 探测，18086 被占则 +1 重试、上限 5 次报错
- 运行时目录：`%LOCALAPPDATA%/CapsuleRetro/xeen/`（conf + log）；环境变量 `CAPSULERETRO_RUNTIME` 可覆盖到 `.temp/`（排障/探针用）

#### 4. 前期准备·fork 线：构建链与基线复现（★ go/no-go 判定点）

- 环境探测：盘点 git / VS Build Tools / Meson / Ninja 现状与版本，缺口安装清单（微软官方 / 开源组件）交用户安装
- 源码基线：clone dosbox-staging **v0.83.0 tag** 到仓库外独立目录 `E:\GameMission\DosboxCapsule\`（独立 git 仓 **DosboxCapsule**：upstream remote 指官方 + capsule 补丁分支）——未改一行构建出 dosbox.exe；关于页/版本串声明"DOSBox Capsule（based on DOSBox Staging v0.83.0, GPLv3）"
- 基线复现：构建产物 + webserver 跑 XEEN，`xmonitor watch` 92 条目读数与官方发布版零偏差
- 补丁纪律：最小 diff、分主题 commit、锁 v0.83.0；clean rebuild 脚本保证可重复
- **判死条件**：本机建不起构建链且用户不愿补装 → 后期实施走备线（独立窗口模式），前期 launcher 线成果不受影响

#### 5. 后期实施·主线：fork 三端点 + 统一窗口集成

- **帧流端点**：定位渲染 present 点 → 双缓冲帧缓冲（RGBA + 宽高 + 帧号，防撕裂）→ webserver 新增 `GET /api/v2/frame`（loopback 轮询 30–60fps；938x704×4≈2.6MB/帧，loopback 无压力，不足再上共享内存环）
- **输入注入端点**：`POST /api/v2/input/event`——进程内 `SDL_PushEvent` 合成键盘（vk/scan/down）与鼠标（相对 dx/dy + 按键）；无焦点依赖，绕开 T1 四轮失败的全部模式（fork 核心红利）
- **内存写入端点**：`POST /api/v2/memory/write` {seg/off/bytes}——MemBase 线性直写；非法地址返回 400 不崩溃；顺带解锁 y.problems 七项"待游戏事件"实验
- **统一窗口集成**：`winembed.rs` 找到 SDL 窗口 → 移至屏外托管（显示走帧流、输入走注入端点，均不依赖窗口可见性）；生命周期仍由 Job Object 管理；GPLv3——fork 分发开源义务，商业化授权结构 Phase 1 前决策

#### 6. 后期实施·备线：独立窗口模式（fork 基线判负时启用）

- launcher 控制窗口（正常尺寸：启动 / 攻略 / 存档管理）+ 游戏独立窗口（Job Object 生命周期）+ HUD 贴边面板
- 全部基于已验证机制（B 阶段编排 + PL001 REST），零新增技术风险；验证 BW1–BW4
- **⏸ 休眠（2026-09-07）**：FB1 判 go、主线已全结，本备线不执行；仅帧流/注入路线出现不可修复缺陷时重开——登记 [problems#16]

#### 7. 后期实施·HUD（两分支共用，形态随分支）

- **`launcher/src-tauri/src/hud.rs`**（新）：std::thread + `capsule_core::monitor` 500ms 轮询（profile 路径取自 game.json 的 assets.memory_map）→ tauri Emitter 推 `hud` event → 前端 `HudBar.vue` 渲染；派生公式按已定案（食物天数 = food//18、"第 X 天" = ((day-1) mod 10)+1）
- 形态：主线 = HUD 画在统一窗口内 4:3 区域外（与帧流同画布）；备线 = HUD 贴边面板（游戏窗口旁）；均不遮游戏区、不吃游戏输入

### 三条线路研究、决策与备案（✅ 已判定完结 2026-09-05，历史依据）

#### 路线一：SetParent 父子嵌合（主选）

- **研究现状**：Win32 SetParent 嵌异进程窗口是成熟做法，但 DOSBox Staging 窗口由 SDL2 创建——SDL2 窗口被改样式 / 改父后内部行为无可靠先例。四个具体风险点：① 游戏切换视频模式时 SDL 可能重建窗口导致脱嵌；② DPI 缩放坐标系错位；③ 嵌套窗口键盘焦点归属（XEEN 键盘为主，焦点必须可靠）；④ Alt+Tab / 最小化行为
- **做法**：见实现措施 §4；在 VC0 打样通过后全量投入
- **判定**：VC1–VC5 全过 → 成立；VC6/VC7 外围失败可修 → 记 workaround 继续；**核心项 VC2（鼠标）/ VC3（键盘）/ VC4（模式切换）任一失败 → 判死，转路线二**

#### 路线二：非嵌套几何定位（备案，预期最稳）

- **研究现状**：不做任何窗口关系变更——launcher 无边框全屏盖满桌面，DOSBox 保持独立顶层窗口，SetWindowPos 精确摆到 4:3 区域上方（owner / 置顶协调）。兼容性最好（零 SDL 内部假设），代价：Alt+Tab 双条目、启动瞬间可见摆放闪动、焦点协调自己做
- **做法**：复用路线一约 80% 代码（枚举 / 几何 / 监听），仅去掉 SetParent 与样式调整，补 owner / 置顶协调
- **启用条件与判定**：路线一判死即转；VC1–VC7 复跑，VC6 放宽为"两个条目但行为正确"

#### 路线三：渲染流集成（远备，本轮不实施）

- **研究现状**：3a——libretro DOSBox core：画面渲染进 launcher canvas，但脱离 Staging 主线、conf 体系完全不同（retroarch 式）、core 维护活跃度一般，架构冲突最大；3b——DOSBox-X 特性路线：同样偏离；3c——截屏流（共享内存 / 管道 → WebView）：延迟与复杂度不划算
- **备案价值与重新触发条件**：路线一、二双双失败，或 REST 通道被证明撑不起 HUD（概率极低）。保证最坏情况下产品仍成立（"独立窗口模式"先行交付，体验降级、功能完整），Phase 0 无死局

#### 决策树（判定权：AI 按树执行，出结论汇报）

VC0 打样 → 过：C 阶段主攻路线一 / 核心崩：直接主攻路线二 → 路线一判死 → 退路线二 → 路线二也败（低概率）→ 启动路线三重评估，同时 Phase 0 以独立窗口模式先行交付

【已执行完毕 2026-09-05：路线一判死（跨进程 SetParent 输入失效）→ 路线二判死（形式被用户否）→ 路线三重评估（T1 探针四轮：显示✅ / 输入❌）→ 用户决策 T2 fork 立项，本附录改造为 v2 两段式】

### 验证方案（全部可执行、可断言）

| #     | 层级        | 检验内容            | 具体手段与通过标准                                                                 |
| ----- | ----------- | ------------------- | ---------------------------------------------------------------------------------- |
| VC0.1 | 打样·渲染   | reparent 后画面正常 | ✅/❌ 已判定（历史，见实测结论）                                                   |
| VC0.2 | 打样·鼠标   | reparent 后鼠标有效 | ❌ 判负（历史）                                                                    |
| VC0.3 | 打样·键盘   | reparent 后键盘有效 | ❌ 判负（历史）                                                                    |
| VA1   | 骨架        | tauri dev 起窗口    | 窗口打开、按钮可见、控制台无错误                                                   |
| VA2   | 骨架        | 门禁扩容全量首跑    | cargo fmt/clippy/test + npm build（含 vue-tsc）全绿，记录耗时基线                  |
| VB1   | 编排        | UI 启动游戏         | 点按钮 → DOSBox 进程出现 → 游戏画面出现                                            |
| VB2   | 编排        | 进程树终结          | 关闭 launcher → DOSBox 进程同步消失（Job Object 生效，无孤儿）                     |
| VB3   | 编排        | 日志捕获            | dosbox.log 落盘且 CONFIG 告警为零（沿用死键审计手法）                              |
| VB4   | 编排        | 稳定性              | 连续启动/停止 5 轮：无残留进程、无端口冲突                                         |
| FB1   | fork·基线   | 构建可复现          | 未改源码构建产物跑 XEEN 正常、REST 92 条目读数与官方版零偏差（xmonitor 复核）      |
| FB2   | fork·仓库   | 补丁结构            | upstream + patch 分支、主题化 commit、clean rebuild 可重复                         |
| FF1   | 帧流        | 帧流可用            | 连续 300 帧帧号单调递增、单帧服务 <5ms、内容与 PrintWindow 抓帧对照一致            |
| FF2   | 帧流        | 长时稳定            | 30fps×10 分钟：工作集稳定无泄漏、CPU 增量记录                                      |
| FI1   | 注入·键盘   | 注入生效            | 注入 Alt+Enter → 全屏切换（复用 T1 客观信号）                                      |
| FI2   | 注入·闭环   | 输入到游戏          | 注入方向键 → 步数 0x5AA22 递增（REST 读闭环）                                      |
| FI3   | 注入·鼠标   | 鼠标注入            | 注入点击 → 菜单响应（帧差分 / 人工）                                               |
| FI4   | 注入·并发   | 与物理输入共存      | 注入与真实键鼠并发，事件无错乱                                                     |
| FM1   | 写入        | 写入生效与安全      | food=99 写→读→面板一致；非法地址 400 不崩溃                                        |
| UW1   | 统一窗·成像 | 画布显示流畅        | launcher 画布内游戏帧 ≥30fps 观感、无撕裂、内容与游戏一致                          |
| UW2   | 统一窗·鼠标 | 注入鼠标可操作      | 游戏内菜单点击 / 迷宫移动正常                                                      |
| UW3   | 统一窗·键盘 | 注入键盘可靠        | 方向键 + F1 连续 20 次无丢失                                                       |
| UW4   | 统一窗·切态 | 模式切换自适应      | 开场 → 主菜单 → 游戏内画面切换无异常                                               |
| UW5   | 统一窗·焦点 | 输入切换正常        | launcher UI 与游戏输入来回切换 20 次正常                                           |
| UW6   | 统一窗·系统 | 任务栏 / 窗口形态   | 任务栏只有 launcher 一个条目                                                       |
| UW7   | 统一窗·回归 | 生命周期            | 关 launcher 游戏死（回归 VB2）                                                     |
| BW1–4 | 备线窗口    | 独立窗口模式        | 启动 / 游玩 / HUD 贴边 / 生命周期四项人工清单                                      |
| VD1   | HUD·数据    | 数值正确            | HUD 显示与 `xmonitor watch` 双通道对照一致                                         |
| VD2   | HUD·人因    | 不干扰输入          | HUD 不覆盖游戏区、不吃游戏输入                                                     |
| VD3   | HUD·开销    | 轮询开销可忽略      | 500ms 轮询下 launcher 进程 CPU 增量 < 2%（任务管理器对照）                         |
| —     | 工程门禁    | 静态检查与测试全绿  | cargo fmt + clippy -D warnings + test + npm build（含 prettier --check / vue-tsc） |

### 验收标准

1. 前期准备：VA / VB / FB 全绿（launcher 线与 fork 线并行完成）
2. 后期实施按 go/no-go 分支二选一：主线 FF / FI / FM / UW 全绿，或备线 BW 全绿；分支判定有书面结论
3. `retro/game.json` v1 落地且 launcher 真实消费（`.temp` 临时 conf 退役）
4. workspace 全门禁绿（Rust 三件套 + 前端 prettier/vue-tsc/build）+ fork 仓 clean rebuild 可重复
5. 文档回写：z.plan 本附录实测结论、x.progress 勾选、w.study 收录 fork 构建链与三端点知识

### 拆分 todo

见 x.progress.md「PL002」任务组（30 条子任务：前置打样 / 阶段 A / 阶段 B / 阶段 C【已由 fork 线取代，历史保留】/ 阶段 D / 前期 fork 线 / 后期主线 / 后期备线，每条含做法与验证方式）。

### 实测结论（VC0 + T1 已判定 2026-09-05；方向决策：T2 fork 立项 → v2 两段式）

> 方向决策（2026-09-05，用户）：T2 fork 立项，本附录改造为"前期准备 + 后期实施"两段式；PL003 不再单独立项。

#### VC0 打样判定书（2026-09-05）

| 项               | 结果      | 证据                                                                                                                                                           |
| ---------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| PL002.1 打样执行 | ✅        | `.temp/vc0-reparent.ps1` 一次跑通：SDL_app 窗口识别（title=XEEN.EXE - 5000 cycles/ms）、reparent 后矩形与预期像素级一致（ALIGNED yes，日志 .temp/vc0-run.log） |
| VC0.1 渲染       | ⚠️ 未判定 | 输入先于一切失效，渲染存活与否未获用户判定                                                                                                                     |
| VC0.2 鼠标       | ❌ 判负   | 全部失去响应                                                                                                                                                   |
| VC0.3 键盘       | ❌ 判负   | 同上                                                                                                                                                           |

- **路线一判死**：跨进程 SetParent 的输入路由失效——子窗口输入挂靠宿主线程队列，SDL 焦点/捕获模型与 reparent 不兼容（Win32 跨进程嵌窗的已知危险面，本次实测证实）。按判定规则"核心项任一失败即判死"执行。
- **路线二判死（用户裁决：表现形式不可接受）**：机制本身工作正常（游戏保持正常顶层窗口、位置对齐 ALIGNED yes、探针回退正常），但全屏接管 + 悬浮窗口的表现形式被用户否定；三项复测未进行（用户在验证前即否定形式）。**注意区分：机制未死、形式被否**——若存在用户可接受的变体形式（如普通尺寸 launcher 窗口 + 游戏去边框贴内容区 + owner 关系），路线二有复活条件。
- **路线三触发重评估**：按决策树"一、二均败"条件成立。重评估关键代价：libretro core 不带 Staging webserver，**PL001 数据通道不兼容**（HUD 需另寻数据源），且核心基于老 DOSBox——展开与否待用户定方向。
- **备案激活**：独立窗口模式（launcher 控制窗口 + 游戏独立窗口 + Job Object 生命周期管理）按决策树作为先行交付形态，待用户确认。
- 分离回退链路实测有效：宿主窗体关闭 → 跨进程子窗口被正确分离恢复（VC0 路线一）／置顶与矩形还原（路线二），游戏进程存活。

#### T1 探针判定书（2026-09-05，四轮 `.temp/t1*.ps1`，全部客观信号取证）

| 项                     | 结果    | 数据与证据                                                                                                                              |
| ---------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| F1 显示：屏上抓帧      | ✅ 成立 | PrintWindow(PW_RENDERFULLCONTENT) 对 SDL_app 实时抓帧：10/10 成功、distinct=5（动画实时进帧）、单帧 29.6ms                              |
| F1 显示：离屏抓帧      | ✅ 成立 | 窗口移至 -2600 屏外同样可读：10/10、distinct=6、单帧 20.2ms、黑率与屏上一致（0.079，非黑屏非残影）                                      |
| F2 输入：PostMessage   | ❌      | WM_KEYDOWN/WM_SYSKEYDOWN 直接投递无响应                                                                                                 |
| F2 输入：伪造焦点      | ❌      | WM_ACTIVATE + WM_SETFOCUS 后再投递，无响应                                                                                              |
| F2 输入：前台+物理注入 | ❌      | SetForegroundWindow 已验证（GetForegroundWindow==child），keybd_event 物理级 Alt+Enter 无效果；Enter×2 帧差分零扰动（5 哈希完全无新增） |
| F2 输入：离屏+前台     | ❌      | 同条件离屏复测，无效果                                                                                                                  |

- **结论：T1 显示侧完全成立、输入侧判死**——Staging 的 SDL3 窗口不响应任何外部注入输入（四种机制全灭），只认真实物理输入；而物理输入要求窗口可见且聚焦，即回到独立窗口/路线二形式。
- **推论**："保持 Staging + 统一窗口"仅剩 **T2 fork Staging**（源码级加帧流端点 + 输入注入/写内存端点，代价 = 永久维护 fork，GPLv3 开源义务）；Staging 原生形态上限 = 独立窗口模式或路线二形式变体（需用户描述可接受形式）。
- 顺带实测：游戏 VGA 模式切换时窗口自适配（869x678 → 654x518），探针顺带记录。

#### 阶段 A/B 开展结论（2026-09-06，VB1–VB4 全部通过）

- **VB1/VB2**：UI 启动 → REST 10 秒内可达 → 进程存活；停止 → Job kill → 5 秒内退出；**用户 UI 实测两轮启动/停止正常**（含重复会话）。机制：`KILL_ON_JOB_CLOSE`——launcher 崩溃时进程树同样必死。
- **VB3**：dosbox.log 落盘（`%LOCALAPPDATA%/CapsuleRetro/xeen/`），断言零 `CONFIG: Deprecated` / `No config section` 告警。
- **VB4**：连续 5 轮启动/停止（live 测试 7.8s），端口每轮复用（无残留强断言）。
- **附带修复**：移除 tauri.conf 的 `devUrl` 使 debug exe 自包含（否则 WebView 连 5173 被拒=白屏）。
- **Mimosa 误报改道**：`std::process::Command` 被整文件判高危（三种安全形态均拦、连 import 命中）→ 改用 windows crate `CreateProcessW` 直写（winproc.rs），`CREATE_SUSPENDED` 先挂 Job 再恢复，行为等价且消除竞态。

#### 阶段 D 开展结论（2026-09-06，VD1–VD3 全部通过）

- **VD1 双通道对照**：HUD poller 快照与独立 RestClient 直读（同档案同字段）完全一致——数据链路（档案筛选/REST 读取/解码/派生公式）正确。
- **VD2 不干扰输入**：当前形态结构性满足——HUD 在 launcher 控制窗口内、游戏为独立顶层窗口，几何不相交；主线（统一画布）落地时按同判据复验。
- **VD3 CPU 开销**：GetProcessTimes 10 秒窗口采样，轮询线程 CPU 占用 < 2%（断言内建于 live 测试）。
- **容错登记**：HUD 轮询瞬时失败容错已按规则登记 AGENTS.md 白名单（跳过本轮 / 连续 5 次停滞事件 + 停机）。

#### 前期准备·fork 线开展结论（2026-09-06，PL002.19–20 完成）

- **PL002.19 环境探测**：VS Build Tools 2026 18.8.2（cl 14.51 + SDK 10.0.26100）在位；缺口 meson/ninja/cmake 经 pip --user + scoop 补齐（1.12.0 / 1.13.2 / 4.4.3），pip Scripts 目录补入用户 PATH；CMake 4×zlib-ng 2.1.5（要求 3.5.1）兼容核实；全记录 `.temp/p1-env.md`。
- **路线变更（Meson → CMake+vcpkg）**：v0.83.0 tag 的 `resources/shaders/meson.build` 引用已被移除的 misc 目录（configure_file cp 失败）——tag 自身 meson 构建损毁；上游 fbef15885 "Kill the Meson build system with fire" 已整体删除 meson。0.83 官方 Windows CI 路径 = CMake+vcpkg（VS 生成器 + ClangCL 工具集），fork 演进对齐上游，判死 meson。
- **Meson 探测沉淀**（复建/升级可复用）：本会话环境 `PYTHONUTF8=1` 使 meson 走 UTF-8 严格解码分支，cl 的 GBK 用法文本必崩（Popen_safe strict 分支）——构建须清该变量；`vcvarsall.bat x64` 显式环境可用（meson 见 VCINSTALLDIR 跳过自激活）。
- **PL002.20 基线构建（P1–P4 全过）**：vcpkg 锁 baseline `6283825b`（34 包 22 分钟，二进制缓存沉淀，重建秒级）；ClangCL 22.1.3（用户补装 VS Clang 两组件）；cmake 直调等效 preset——preset 硬编码 VS 17 生成器且本机有 Insiders 多实例，须用 VS18 2026 生成器 + `CMAKE_GENERATOR_INSTANCE` 钉死 BuildTools（Insiders 实例 MSBuild 缺工具集触发 MSB8020 的教训）。产物 dosbox.exe（0.83.0 / 7b400），构建后 `git status --porcelain` = 0 行（零改动实证）；启动 XEEN：模式 13h 进入、REST 66 秒三连 200、5/5 档案条目读出真实数值（hp1=12 / exp1=2014），用户目视确认画面 OK。构建脚本 `.temp/p1-cmake-build.bat`（含 ALL_PROXY socks 修正——libcurl 不认 http(s)_proxy 的 socks5 方案、PYTHONUTF8 清除）。
- **PL002.22 补丁仓库结构（FB2 全过）**：公开 fork `pyapple12/DosboxCapsule`（用户建仓改名，fork 关系保留）；本地 `capsule` 分支基于 v0.83.0、双 remote（origin=fork 推送 / upstream=官方升级）；FB2-a 清洁重建全绿（build 目录全清 → 全新配置编译 → 产物 0.83.0 (7b400)）；FB2-b 结构纯净（capsule diff 零起步）；fork 仓提交风格对齐上游英文 conventional，不套 CapsuleRetro 四段式。
- **维护套件集中化（2026-09-07 修订，用户拍板）**：PATCHES.md、清洁重建脚本、FB1 基线判定脚本统一收入 CapsuleRetro `tools/dosbox-fork/`（版本化，迁出 .temp）——capsule 分支 diff 保持 100% 代码补丁；fork 仓仅以 FORK.md 单提交声明用途。迁移一致性验证闭环（cmake 链逐行相等、capture 执行行 MD5 全等、compare 仅 docstring 与错别字差异、新位置功能跑通 PASS）。
- **切断 fork 转独立仓（2026-09-07，用户拍板）**：删除 fork 重建同名独立公开仓（仓库初生期零迁移成本；"193 commits behind / Discard" 陷阱横幅随 fork 关系消失），本地 capsule 分支改名 main、推 main + v0.83.0 tag（基线溯源，默认分支自动 = main）；FORK.md 改独立仓表述，PATCHES.md 分支模型简化为单分支 + tag 锚点（`git diff v0.83.0..main` = 全部足迹，升级配方 rebase --onto 换轴分支名同步）。GitHub Actions 因触发上游 CI（PVS-Studio 无许可必红）已关（用户操作，仓库设置级，零 diff）。**前期准备四连（PL002.19-22）全部完结，后期实施（PL002.23 帧流端点起）就绪。**

#### 后期实施·主线开展结论（PL002.23 起）

- **PL002.23 帧流端点（FF1 全过，commit a8e395cbf）**：研究修正——v0.83 webserver 已自带 Bridge 线程模型（webserver 线程投递命令、模拟主循环 `ProcessRequests` 执行，dosbox.cpp:120）与**内存写入端点**（`PUT /api/v1/memory/:seg/:off`，含 If-Match 乐观锁，PL002.26 因此降级为验证任务）；挂钩点定为两渲染器 `EndFrame`（DOS 帧内容完成点，scaler 输出尺寸、BGRX 32bit），帧流共享缓冲 `CapsuleFrame` 置于 gui 层（webserver→gui 依赖方向与上游一致；webserver 关闭时 Capture 仅一次原子读短路）；端点 `GET /api/v2/frame` 用响应头携带元数据 + 原始 BGRX body（30-60fps 轮询下避免 base64 膨胀 1/3 带宽）。FF1 实测：帧号 3→670 零回退、单帧服务最慢 2.27ms（keep-alive 口径，修正了把 curl 进程开销计入的错误口径）、尺寸自洽 640×200；内容对照用户确认一致（stream 640×200 pre-shader vs 窗口 post-shader，分辨率差异系设计——画布消费小帧自行 4:3 缩放）。**遗留 PL002.27**：帧流暂无像素纵横比元数据，画布 4:3 拟合需补充。
- **PL002.24 帧流稳定性（FF2 全过）**：PowerShell HttpClient keep-alive 30fps×600 秒——12816 请求 0 失败、帧号 21 段零停滞（总增量 5747，9.1fps 内容变化帧率符合静态画面特征）、工作集仅涨 1.3MB（无泄漏）、**CPU 净增量 ≈0**（轮询期 20.32% vs 无轮询基线 21.07%，对照实验分离游戏本体开销；净增量优于 VD3 HUD <2% 口径）。两处阈值口径修正如实记录：帧增量阈值撤回（渲染帧≠内容变化帧混淆）、CPU 判定改净增量口径。`.temp/ff2-samples.csv` 存证。
- **PL002.25 研究预收**：mapper 统一事件入口 `MAPPER_CheckEvent`（mapper.cpp:2640）+ 上游自带 `MAPPER_AutoType`/`MAPPER_TriggerEvent` 程序化触发机制；注入走 Bridge（模拟线程执行 mapper 分发）而非 SDL_PushEvent——SDL 队列会被暂停循环清空（sdl_gui.cpp:349），Bridge 直调绕坑且与 memory 端点模式一致。I1 客观信号复用 Alt+Enter 全屏切换（T1 探针判据）。
- **PL002.25/26 实施（I1/I2/M1/M3 全过）**：注入端点 `POST /api/v2/input/event`（input.{h,cpp}）——键盘 Bridge+`MAPPER_CheckEvent`（模拟线程，与真实事件泵同路），键名表 62 键+6 修饰键；I1 = 窗口 869×678→1536×864 全屏往返；**协议事实**：mapper 组合键须时序注入（leftalt down → return → up），单发带 mod 位事件不触发。26 设计修正：上游 v0.83 自带 `PUT /api/v1/memory/:seg/:off`（If-Match 乐观锁），唯一补丁 = error_handler 映射 400；M3 = 越界 400 不崩溃；M1 = 游戏内 PUT food=99 读回持久（**标题屏下人物数据区被游戏活跃代码覆盖——写入验证必须在游戏内状态**）。
- **PL002.27/28 统一窗口集成与收尾（UW1–UW7 全过，nomouse 形态定案）**：launcher 侧 = winembed.rs（离屏 + TOOLWINDOW 摘任务栏，UW6 客观 rect x=-2560/TOOLWINDOW 置位）+ get_frame/inject_input 命令（ureq 透传 + 20 字节元数据头，零 CORS）+ GameCanvas.vue（30fps + 帧号去重 + BGRX→RGBA + PointerLock + 键盘鼠标转发）+ 双模式布局；HudBar 进顶栏（VD1/VD2/VD3 通道未变分别引用既有验证）。**实测根因链三条（集成测试价值实证）**：① 黑屏 = game.json 指官方版无帧流端点 → 改指 DosboxCapsule 构建产物（launcher 首次消费 fork）；② 注入光标不动 = 离屏窗口 cursor_is_outside 恒真丢弃鼠标事件 → **fork 补丁 #4** 注入原语 MOUSE_InjectMoved/Button（绕判定 + cursor_x_abs/y_abs 累进 clamp，nomouse 态驱动走 seamless 绝对路径必需）；③ 中键致宿主光标失效 = 非捕获态中键触发捕获、离屏窗口拖宿主光标进屏外 → **nomouse 形态定案**（dosbox 永不抓宿主光标，鼠标输入全走注入，宿主光标归画布 Pointer Lock）。全量门禁绿。**PL002 前期准备与后期实施主线全部完结**；备线（PL002.29-30）仅在上游升级 rebase 失败时回炉。

#### PL002 主线完结结论（2026-09-07，收口）

- **主线 28 条全结**：fork 三端点（帧流 / 输入注入 / 内存写入映射 400）+ 统一窗口（离屏托管 + 画布吃帧流 + 注入转发）+ HUD 收尾全过；全量门禁绿（cargo fmt / clippy -D warnings / test + vue-tsc + npm build）。
- **nomouse 形态定案**：dosbox 永不抓宿主光标（capture/middle/热键全禁、int33 照常注册），宿主光标归画布 PointerLock、DOS 光标归注入原语；三根因链（错 exe / cursor_is_outside 丢事件 / 中键触发捕获拖光标进屏外）全部闭环。
- **双仓终态**：DosboxCapsule main = v0.83.0 + 4 代码补丁 + 3 运维提交（FORK.md、dependabot.yml 移除；单分支 + v0.83.0 tag 锚点）；CapsuleRetro main = V0.1.0.3。dependabot 事件根因清除（继承 yml 的 github-actions 调度 + 独立于 yml 的安全更新通道；PR 建不出回落为 issue 的形态观察）。
- **遗留登记**：帧流像素纵横比等 9 项挂账 [problems#9–17]；备线休眠 [problems#16]。

#### FB1 基线复现判定书（2026-09-06，go——后期实施走主线）

- **方法**：`.temp/fb1-capture.sh` + `fb1-compare.py`——官方发布版（assets 内 v0.83.0）与构建版（DosboxCapsule ClangCL 产物）串行启动 XEEN（同 conf 同工作副本，标题屏），各拍两份 1MB 全内存快照（间隔 2 秒），按 memory_map 档案 92 条目切片对照：自对照（a vs b）确认稳定、交叉对照（官方 vs 构建）字节级零偏差。
- **判定过程含一次探针自证**：首轮构建版 5 秒稳定期不足（s2a 全零被自对照识别），暴露判定口径的空洞——静态集为 0 时"零偏差"为空真；改 12 秒等待重跑，双稳定条件真实成立后判定有效。
- **结果**：92/92 条目静态、0 偏差（`fb1-report.json`）。构建版（ClangCL）与官方发布版在 DOS 内存行为上字节级一致，memory_map 档案在构建产物上完整有效。
- **决策**：FB1 全过 → **PL002.23 起后期实施走主线**（fork 三端点 + 统一窗口）；备线（PL002.29-30）仅在上游升级 rebase 失败时回炉。

---

## 附录 A001：全量代码审计报告（第1轮，2026-09-07）

> 范围：capsule-core + launcher 后端（src-tauri）+ Vue 前端 + tools/dosbox-fork 运维脚本 + retro/game.json（2026-09-07 修订版范围界定，首轮执行）；方式：三路并行子任务全文通读 + 主会话合并去重 + P1/关键 P2 锚点原文复核；规则版本 CR 2026-09-04；DosboxCapsule 独立仓在范围外（PATCHES.md + FB1 门槛自行约束）。
> 状态：✅ 已修复（2026-09-08，FIX001 全组 44 条勾结；反向验收与 live 待验清单见 x.progress.md FIX001 组注记及 `.temp/fix001-verification.md`。例外：#42 CSP 随 PL003 落地执行；#21 game.json/package.json version 按 FIX001.25 豁免定案，不改代码）

### 零、上轮修复复核清单

无上轮（首轮审计）。y.problems #12（gameconf 测试读取 gitignored 参考文件）由后端组独立再发现，与登记一致，未重复立条。

### 一、P0-P3 修复清单

#### P1（高——确定性复现，3 条）

| #   | 位置                                             | 类别 | 描述                                                                                                                                                              | 建议                                                         | 影响面                  |
| --- | ------------------------------------------------ | ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | ----------------------- |
| 1   | `winproc.rs:59-63`（文档 ：49）                  | 1    | dosbox args 未加引号，与文档"各段自动引号包裹"不符——只有 exe 走 `quoted()`，参数裸拼；conf 路径来自 `%LOCALAPPDATA%`，用户名含空格的机器上确定性截断，launch 必败 | args 统一走 `quoted()`；同步修正文档                         | DOSBox 会话             |
| 2   | `tools/dosbox-fork/rebuild.bat:25-26`            | 2/13 | 构建失败仍退出码 0——build 步只 echo REBUILD-EXIT，无 errorlevel 判断，FB2"重建可重复"门禁失败被当成功                                                             | 末尾补 `exit /b %ERRORLEVEL%`                                | 跨模块（fork 升级门禁） |
| 3   | `tools/dosbox-fork/baseline-compare.py:40-48,73` | 1    | FB1 假 GO 双路径：①"仅候选版自对照不稳定"被静默归入 dynamic 剔除（rebase 非确定性本身是回归信号）；② 静态集趋近空时空比对内容照常返回 GO                          | dynamic 细分稳定性，单侧不稳即非 0；静态条目数低于阈值判非 0 | 跨模块（FB1 判定）      |

#### P2（中，23 条）

| #   | 位置                                                                               | 类别 | 描述                                                                                                                          | 建议                                         | 影响面                |
| --- | ---------------------------------------------------------------------------------- | ---- | ----------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- | --------------------- |
| 4   | `baseline-compare.py:22-25`                                                        | 2    | 切片无边界校验，越界静默截断——档案坐标出错时四快照同切空串恒等，条目形同未比对仍 GO（需验证：触发需档案坐标错误）             | `slice_at` 断言 `addr+length <= len(mem)`    | 跨模块                |
| 5   | `commands.rs`（无退出监听）+ `App.vue:37-46`                                       | 8/10 | DOSBox 自退/崩溃后会话槽位不自清、前端无感知——`running` 恒真，再启恒报"已有会话"，画布 30fps 永久静默吃 503                   | 退出监听线程 + emit 事件清槽（跨组联动）     | 跨模块                |
| 6   | `commands.rs:100` + `GameCanvas.vue:58-85`                                         | 2    | 帧协议静默错值链：后端 `take(16MB)` 静默截断，前端不校验长度与 pitch，像素循环读 undefined 静默写 0 → 花屏不报错              | 后端截断报错；前端加长度/pitch 断言          | Vue 前端 + Tauri 后端 |
| 7   | `GameCanvas.vue:99-111`                                                            | 13/2 | 空 catch 域覆盖 drawFrame（解码异常被当瞬时抖动永久静默）；帧循环降级未登记容错白名单                                         | 缩小 try 域；警告 + 连续失败计数显式置错     | Vue 前端              |
| 8   | `commands.rs:207-244`                                                              | 8/9  | 同步 command 主线程执行：get_frame 含 3s 超时 HTTP，launch 持锁全程 spawn——UI 冻结最长 3s（需验证 Tauri 2 调度）              | async / spawn_blocking；锁外构造 Session     | Tauri 后端            |
| 9   | `winproc.rs:108-110`                                                               | 2    | ResumeThread 返回值未检查，失败仍 Ok → 进程永久挂起的静默假成功                                                               | 检查 `!= u32::MAX` 转错误                    | DOSBox 会话           |
| 10  | `commands.rs:136-147`                                                              | 8    | probe_port 绑定后立即释放至 DOSBox bind 之间为 TOCTOU 窗口，被抢后 webserver 起不来且无诊断指向根因                           | 文档标注；launch 后做 webserver 就绪探测     | DOSBox 会话           |
| 11  | `commands.rs:184-190`                                                              | 8    | Job assign 失败提前返回时挂起的 DOSBox 未进 Job、句柄 drop 不杀 → 无人认领僵尸                                                | 失败路径显式 TerminateProcess                | DOSBox 会话           |
| 12  | `commands.rs:232-240`                                                              | 8    | embed 固定 2s 竞态无重试——慢机器上离屏托管静默失效                                                                            | 轮询重试若干次                               | DOSBox 会话           |
| 13  | `commands.rs:169-171`                                                              | 2    | work_dir 直接 mount 但不校验存在性（释放逻辑属 PL003 未建）——缺失时用户只见黑窗                                               | launch 前校验，缺失严格抛错                  | DOSBox 会话           |
| 14  | `main.rs:3` + 全部 `eprintln!`                                                     | 10   | release 无控制台，全部诊断输出丢失，无日志文件方案                                                                            | 文件日志统一落 runtime_dir                   | Tauri 后端            |
| 15  | `retro/game.json` + `xmonitor.rs:30`                                               | 3    | 端口 18086 双处事实源无同步机制。修正：A 组"commands.rs 生产第三处硬编码"经核实不成立（:305/:310 为测试，生产 :168 正确派生） | xmonitor 加 `--game-json` 派生或注明同步关系 | 配置体系              |
| 16  | `gameconf.rs:19-21`                                                                | 3    | 运行时目录 `join("xeen")` 游戏身份第二处硬编码，与 config.name 脱钩                                                           | 传 config.name 小写派生                      | 配置体系              |
| 17  | `rest.rs:118-135`                                                                  | 2/6  | memory() 短读静默接受 + 返回值文档自相矛盾——短读污染快照后差分报 LengthMismatch 误导排查                                      | 读后校验长度报 ShortRead；统一文档           | DOSBox 会话           |
| 18  | `xmonitor.rs:179-196`                                                              | 2    | search 离线参数不成对时静默混入交互模式；--s3 单独传时三种来源混用                                                            | clap requires 成对或显式报错                 | DOSBox 会话           |
| 19  | `profile.rs:175-176` + `xmonitor.rs:129-130`                                       | 2/10 | 档案 len 与宽度一致性无加载期校验（len:0 必败）；WidthMismatch 错误不带条目名无法定位                                         | 加载期校验；错误附 entry.name                | DOSBox 会话           |
| 20  | `rest.rs:40-66`、`xmonitor.rs:157-168`、`commands.rs:232-240`、`gameconf.rs:19-21` | 13   | 容错白名单登记制执行缺口（四处合并）：serde(default) 置 0 / 快照存档降级 / embed 退化形态 / LOCALAPPDATA 兜底 temp——均未登记  | 逐条登记或改严格抛错                         | 跨模块                |
| 21  | `retro/game.json:3` + `launcher/package.json:4`                                    | 3/12 | 版本单一来源第二处事实：game.json "1.0" 硬编码（AGENTS.md 明文禁止且零消费）；package.json 版本同理                           | 派生或 AGENTS.md 条目明示豁免                | 配置体系              |
| 22  | `baseline-capture.sh:28,37`                                                        | 8    | taskkill //IM 全机杀 dosbox——本机其他实例会被误杀                                                                             | 改按 PID 杀；//IM 兜底改可选并警告           | 跨模块                |
| 23  | `tools/dosbox-fork/rebuild.bat:16`                                                 | 2    | rmdir 失败无检查，残留旧 build 继续构建——"清洁重建"被静默架空                                                                 | rmdir 后 `if exist ... exit /b 1`            | 跨模块                |
| 24  | `GameCanvas.vue:32`                                                                | 1/5  | NumpadEnter 死条目（Numpad 前缀分支先执行）；Numpad 分支大小写不统一（fork 键名契约需验证）                                   | 特判提前；统一大小写                         | Vue 前端              |
| 25  | `GameCanvas.vue:68-70`                                                             | 9    | 每帧无条件重设 offscreen 尺寸 + createImageData——30fps 重复分配 ~1.2MB/帧级 GC 压力                                           | 尺寸变化才赋值；ImageData 缓存               | Vue 前端              |
| 26  | `commands.rs:165` + `gameconf.rs:24-35`                                            | 1/10 | 仓库根解析依赖进程 cwd（打包形态必败），探测失败静默返回 start、根因被掩盖（需验证打包形态）                                  | exe 路径锚定；探测失败显式报错               | Tauri 后端            |

#### P3（低，17 条）

| #   | 位置                                                      | 类别 | 描述                                                                           | 影响面      |
| --- | --------------------------------------------------------- | ---- | ------------------------------------------------------------------------------ | ----------- |
| 27  | `GameCanvas.vue:13,102-108,177`                           | 5    | frameFps 及 fps 统计机制只写不读（死代码）                                     | Vue 前端    |
| 28  | `winembed.rs:76-94`                                       | 5    | window_rect 全仓零调用                                                         | Tauri 后端  |
| 29  | `launcher/src-tauri/Cargo.toml:12`                        | 5/12 | serde_json 零引用依赖                                                          | Tauri 后端  |
| 30  | `rest.rs:40-66`                                           | 6    | Registers/CpuState pub 字段缺 /// 文档                                         | DOSBox 会话 |
| 31  | `commands.rs:249-251,262-263,283`                         | 11   | "无会话"返回语义不一致（stop=Ok，其余=Err），错误为中文裸串无法程序化分类      | Tauri 后端  |
| 32  | `launcher_session_test.rs:61-67` vs `commands.rs:292-298` | 4    | 端口探测测试逐字重复两份                                                       | Tauri 后端  |
| 33  | `launcher_session_test.rs:41`                             | 6    | use 语句在文件中部                                                             | Tauri 后端  |
| 34  | `xmonitor.rs:156`                                         | 6    | 快照目录 `.temp` 相对 CWD                                                      | DOSBox 会话 |
| 35  | `HudBar.vue:18-35`                                        | 8    | listen 异步注册与卸载竞态，unlisten 可能泄漏（需验证触发路径）                 | Vue 前端    |
| 36  | `winembed.rs:56-57`                                       | 2    | GetWindowLongPtrW/SetWindowLongPtrW 返回值未检查                               | DOSBox 会话 |
| 37  | `winproc.rs:64-68`                                        | 1    | 命令行 UTF-16 用 `c as u16` 截断非 BMP 字符（与 appname 的 encode_utf16 漂移） | DOSBox 会话 |
| 38  | `hud.rs:99-143`                                           | 9    | poll_loop 轮末固定 sleep，stop 最坏阻塞 500ms                                  | DOSBox 会话 |
| 39  | `baseline-capture.sh:14,16`                               | 2    | CONF 无启动前存在性检查                                                        | 跨模块      |
| 40  | `GameCanvas.vue:88`                                       | 9    | 显示画布 ctx 每帧重复获取                                                      | Vue 前端    |
| 41  | `commands.rs:70` + `GameCanvas.vue:137,104`               | 3    | 魔法数未命名：HUD 500ms / 合帧 30ms / fps 窗口 1000ms                          | 跨模块      |
| 42  | `tauri.conf.json:18`                                      | 7    | 生产 csp null——PL003 攻略渲染前需设最小 CSP                                    | Tauri 后端  |
| 43  | `game_config.rs:43-44`                                    | 2    | name 承担启动命令来源但无空名/合法字符校验（需验证消费方式）                   | 配置体系    |

### 二、参考级观察项（23 条，2026-09-08 用户确认全部保持参考级）

| #   | 位置                                        | 摘要                    | 回落理由                   |
| --- | ------------------------------------------- | ----------------------- | -------------------------- |
| 1   | `xmonitor.rs:93`                            | interval 下限 50        | 单次使用常量               |
| 2   | `diff.rs:17-23,44-54`                       | 等长校验两处重复        | KISS 自洽，第三处再提取    |
| 3   | `diff.rs`                                   | 复用 rest 错误枚举      | 错误类型收敛优先           |
| 4   | `profile.rs:87-103`、`game_config.rs:82-87` | trait impl 缺文档首行   | rustdoc 不展示，惯例豁免   |
| 5   | `rest.rs:129`                               | with_capacity 理论 4GB  | 无可达路径（需验证）       |
| 6   | `rest.rs:78`                                | "默认 8086" 注释        | 需验证上游默认值           |
| 7   | `xmonitor.rs:127-134`                       | poll_once 编排留 bin    | 第三消费方再下沉           |
| 8   | `commands.rs:54`                            | wait_exit(5000)         | 单次使用魔数               |
| 9   | `winembed.rs:18`                            | OFFSCREEN_X=-3200       | 有注释几何参数             |
| 10  | `commands.rs:236`                           | embed 2s 数值           | 有注释（竞态部分已列 #12） |
| 11  | `hud.rs:176`                                | food/18                 | 定案公式有注释             |
| 12  | `jobctl.rs:15-18`                           | 句柄无 Drop             | 注释明示设计               |
| 13  | `commands.rs:174-179`                       | dosbox.log 无轮转       | PL003 一并治理             |
| 14  | `winproc.rs:59`                             | 含 .. 非规范路径        | 文件层正确解析，纯理论     |
| 15  | `hud.rs:22-24` vs 测试                      | WATCHED 双处            | 双通道对照设计             |
| 16  | `GameCanvas.vue:197-198`                    | 960×720 非整数拉伸      | y.problems #9 已挂账       |
| 17  | `GameCanvas.vue:7`                          | FRAME_INTERVAL_MS=33    | 有注释常量                 |
| 18  | `baseline-capture.sh`                       | 字面量路径/端口         | 文件头明示设计             |
| 19  | `rebuild.bat:5,15`                          | 机器路径/代理           | 文件头明示换机改法         |
| 20  | `GameCanvas.vue:59,69`                      | offCtx null             | 纯理论不可达               |
| 21  | `GameCanvas.vue:174-181`                    | active=false 分支不可达 | 双保险，无错误行为         |
| 22  | `vite-env.d.ts`                             | vue shim 冗余           | 惯例无行为影响             |
| 23  | `rest_stub.rs:24-25,61-62`                  | 测试 stub expect/吞错   | 测试惯例                   |

### 三、亮点

- 补丁纪律（最高优先级安全项）三组零违规：全仓无任何对 `retro/` 的写路径
- 帧协议两端一致：20 字节 LE 帧头与 BGRX→RGBA 映射逐字节核对无误；HudSnapshot TS↔Rust 无类型漂移
- 严格抛错主线贯彻：thiserror 枚举齐全，业务代码无散落 unwrap/expect；版本单一来源 workspace 侧合规
- winproc.rs 挂起→挂 Job→resume 时序注释清晰、Send/Sync 论证规范；hud.rs 回调注入可测性好
- 审计过程修正一处子任务误报（生产端口硬编码指控），锚点复核有效

---

## 附录 PL003：save 生命周期 + 补丁闭环（2026-09-08 立项）

> 背景：PL002 收口后 launcher 已能"看 / 玩 / HUD"，但形态仍是"仓库即游戏包"——工作副本靠 `.temp/xeen-work` 手工预置、无用户数据持久化、补丁纪律尚无代码承载（commands.rs 留有"释放逻辑属 PL003"注记）。本期把产品骨架补完：`GameName.save`（唯一用户数据）与补丁应用闭环落地，capsule-core 补齐 save / patch / paths 三模块（launcher 与未来 packer 共用，逻辑只写一份）。
> 关键洞察：① 回写挂点是现成的——FIX001.5 的 exit-watcher 线程已监听进程退出（含被杀），打包回写正好挂在该链路，无需新增生命周期机制；② 补丁纪律（原版只读、补丁只作用于释放的工作副本）第一次由代码结构承载——原版 = 释放源（只读），工作副本 = 补丁与存档的宿主；③ packer 不在本期，save / patch 接口按路径参数化设计，packer 到来只需喂资源。
> 目标：XEEN 端到端跑通"启动解压 save → 按补丁状态释放工作副本 → 运行 → 退出原子回写"完整生命周期；hack / 原版切换互不污染；损坏 save 自愈；攻略渲染 + 最小 CSP 随期落地。
> 状态：✅ 已完成（2026-09-08 收口；VA1–VA4 全过；任务清单见 x.progress.md「PL003」；实测结论见下）

### 方向定案（2026-09-08，用户拍板）

1. **存档界定 = game.json 显式声明清单**：`paths.save_files` 声明存档文件（glob，如 `*.SAV`），回写只打包清单命中文件——干净透明、save 体积小；备选"整个工作副本 diff 回写"弃用。XEEN 具体存档文件名由阶段 A 落盘实验确定。
2. **攻略渲染并入本期**：攻略视图 + Markdown 渲染 + 最小 CSP（审计遗留 FIX001.43 随之闭环）。
3. **开发形态 save 落点 = 仓库根** `XEEN.save`（与产品"exe 同目录"形态一致，packer 到来零迁移），登记 .gitignore。

### 数据格式（定案）

- **save zip**（`GameName.save`，zip 格式可校验结构透明）：

  ```
  meta.json        # { version, hack: "hack1" | null, settings: {} }
  save/            # 存档文件（按 game.json paths.save_files 从工作副本收集）
  config/          # 用户 dosbox.conf 覆盖片段（字段位预留，本期可空）
  ```

- **patch zip**（游戏包内嵌或 game.json 引用）：

  ```
  manifest.json    # [{ action: "replace" | "add", path: "...", source: "files/..." }]
  files/           # 实际替换文件
  ```

  应用方式：解包覆盖到工作副本（原版永不触碰）；manifest path 严格校验不得越出工作副本根（zip slip / 绝对路径 / `..` 防护）。

- **game.json v2 扩展**：`paths.save_files`（存档清单）、`patches`（v1 字段位激活，本期至少一条真实测试补丁）；`paths.work_dir` 语义变更——从"手工预置源"改为"释放目标"。

### 生命周期设计

```
启动（launch_game 前置序列）
  1. 运行时目录自愈校验（work 缺失/不完整 → 从原版重新释放；conf/dosbox 现状保留）
  2. 读 <name>.save → 损坏? → 白名单降级（备份 .corrupt + 新建空 save）→ 解包到 work 工作区
  3. 按 meta.hack 应用补丁到工作副本（hack = null 则纯原版释放）
运行（PL002 会话链路不变：离屏托管 / 帧流 / 注入 / HUD）
退出（exit-watcher 线程 + stop_game 双挂点，同一 writeback() 幂等函数）
  4. 按 save_files 清单从工作副本收集存档 → 打包临时 zip
  5. 原子替换 <name>.save（失败保旧件，可自愈）
```

### 实现措施（按层拆解到文件/函数级）

#### 1. capsule-core 新模块（TDD 先行）

- `src/paths.rs`：运行时目录释放与自愈——release（原版 → work 释放 + 完整性校验 + 缺失重建）；退役 `.temp/xeen-work` 手工预置
- `src/save.rs`：unpack（meta + save/ 落工作区）、pack（按清单收集打包临时 zip）、replace_atomically（rename 覆盖，失败保旧件）；meta.json serde 结构单一来源；损坏检测与降级路径
- `src/patch.rs`：manifest 解析校验 + apply（replace = 覆盖 / add = 新增到工作副本，path 越界报错）
- 依赖：新增 `zip` crate（评估 2.x / 3.x 后锁定），其余不新增
- 测试素材：`capsule-core/tests/fixtures/` 放伪文件构造的补丁包与 save 样本（不含真实游戏文件）；真实补丁包用 retro/XEEN 现成补丁文件（CMM4.PAT 等）只读复制构造，不触碰 retro/

#### 2. launcher 接线（阶段 B）

- `commands.rs`：launch_game 前置序列（释放 → 补丁 → 解包）+ 退出双挂点（exit-watcher 线程与 stop_game 走同一 `writeback()`，幂等）
- `gameconf.rs`：save 路径解析（仓库根 `<name>.save`，产品形态预留 exe 同目录）；测试资产转正（y.problems #12 闭环）
- `logging.rs`：生命周期关键节点落日志（释放 / 补丁 / 解包 / 回写 / 降级）
- 容错白名单登记：save 损坏降级（备份损坏件 + 新建空 save 继续运行）——AGENTS.md 错误策略新增条目（场景 / 降级 / 理由三要素）

#### 3. 前端（阶段 C）

- 待机页：hack / 原版切换（写 meta.hack，启动前生效）+ save 状态显示
- 攻略视图：Markdown 渲染（选型 marked + DOMPurify 或等效净化方案，XSS 防护定案后记录）；`tauri.conf.json` `csp` 从 null 改最小白名单（FIX001.43 闭环）

#### 4. 连带清理

- dosbox.log 轮转（y.problems #13）、gameconf 测试资产转正（y.problems #12）、帧流 640×200 像素纵横比画布修正（y.problems #9）、`.temp/xeen-work` 退役与 AGENTS.md 目录规划回改

### 验收标准

1. **VA1 存档闭环**：游戏内存档 → 退出 → save zip 内可见存档文件 → 重进可读档继续
2. **VA2 补丁纪律**：hack / 原版各启动一轮，retro/ 哈希断言零改动，两者互不污染（工作副本随切换重建）
3. **VA3 防损坏**：手工破坏 save → 启动按白名单降级不炸；回写中断（杀进程模拟）→ 旧 save 完整可用
4. **VA4 攻略渲染 + CSP**：攻略视图可用、csp 非空且功能无回归
5. **收尾**：全量门禁绿 + z.plan 实测结论回写 + x.progress 勾结

### 实测结论（2026-09-08 收口）

- **VA1–VA4 全过**：存档闭环（存档 → 停止回写 6 文件 → 读档续玩）；补丁纪律（retro/ 21 文件 MD5 三比照全等，工作副本随切换重建零污染）；防损坏（垃圾字节 → ERROR 日志 + `.corrupt` 备份 + 降级启动 + 全新档）；攻略渲染 + CSP 目视正常。反向验收 14 项 + live 记录见 `.temp/pl003-verification.md`。
- **落地形态**：capsule-core 新增 save / patch / paths 三模块（zip 8.6.0 仅 deflate；SaveMeta serde 单一来源；manifest replace/add 语义 + 7 种路径穿越全拒）；"每次启动全量重建工作副本"替代计划的"首启释放 + 增量自愈"（残留天然自愈，37MB 亚秒级）；`paths.work_dir` 字段退役（释放目标由 runtime_dir 派生）；game.json v2（game_dir / save_files / patches）。
- **live 揪出 2 个测试盖不住的 bug**：① CSP 未放行 `connect-src ipc: http://ipc.localhost` → 二进制帧通道全灭（FIX001.43 回归；JSON 命令走 postMessage 无恙，唯二进制被拦）——**安全配置回归必须 live 全链路验证**；② hack 选择被旧 save meta 覆盖 → UI 选择定为权威源。
- **连带闭环**：y.problems #12（gameconf 测试资产转正）、#9（纵横比复核为非缺陷降级）、FIX001.43（CSP）；dosbox.log 5MB 轮转；容错白名单新增 2 条（save 损坏降级 / 轮转失败继续追加）。
- **存档事实**：XEEN 为双面合体，存一次档同槽号双面各写一份（XEEN01.SAV + DARK01.SAV），`*.SAV` 通配符天然覆盖。
