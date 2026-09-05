# 进度追踪（x.progress.md）

> 文件职责：任务清单与进度追踪，与 `z.plan.md`（方案与审计归档）配套：方案在 z.plan 展开，执行拆条在本文件勾选。结构：**`## 已完成 ✅` 区在前、`## 未完成` 区在后**，任务完成后整组移动位置。
> 格式速查：任务组 `### {组号}: {标题} [来源引用]`，组内用 `#### {小节}` 分层；子任务 `- [ ] {组号}.{序号} {标题} —— {做法}；验证：{检验方式}`；审计修复任务组 FIX{NNN} 由 audit-report 归档环节生成，条目格式 `- [ ] FIX{NNN}.{序号} [P{级别}] {标题} —— {做法}；验证：{检验方式}`，编号规则见 `.agents/skills/audit-report`。

## 已完成 ✅

### PL001: DOSBox 外部组件挂载验证 [plan#PL001]

> 结果：观察型外部组件挂载方向验证通过——`capsule-core::monitor` 库 + `xmonitor` CLI（A 壳）+ 22 条目 memory_map 档案（全队 HP/MP/EXP + 金币/宝石/银行）在 XEEN 活体上端到端跑通，V3 一致率 100%，V4 稳定性达标（唯一 1 次瞬时失败被白名单容错按设计吸收），V5 性能优秀（1MB 快照 6.5ms）。详见 z.plan.md 附录 PL001「实测结论」。

#### 工程层：最小 workspace

- [x] PL001.1 根 workspace 与 crate 骨架 —— 新建根 `Cargo.toml`（members=["capsule-core"]，[workspace.package] version="0.1.0" 单一来源）+ `capsule-core/Cargo.toml`（serde/serde_json/ureq(json)/thiserror）+ `src/lib.rs`（//! 模块注释 + pub mod monitor）+ `src/monitor/mod.rs`；验证：`cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo check` 三门禁全过（2026-09-05 已验证）

#### 库层 monitor（TDD：每条先写测试确认 FAIL，实现后 PASS）

- [x] PL001.2 ValueType 解码 —— 先写 `tests/profile_test.rs` 四用例：u16le bytes `[0x39,0x05]`→1337、u8、u32le、bytes 长度与类型宽度不匹配→`WidthMismatch`；实现 `enum ValueType { U8, U16LE, U32LE }` + `fn decode(&self, &[u8]) -> Result<u64, ProfileError>`；验证：四用例 FAIL→PASS，`cargo test`（2026-09-05 已验证：TDD 四用例 FAIL(E0432)→PASS，u16le=1337/宽度校验）
- [x] PL001.3 MemoryMap 档案解析 —— 先写四场景用例：合法全字段 JSON、缺 `poll_interval_ms`、`entries` 空数组、未知 `value_type` 字符串；实现 `struct MemoryMap/Entry` + serde Deserialize + `enum ProfileError`（thiserror：Json/UnknownValueType/WidthMismatch）；验证：四场景 FAIL→PASS（2026-09-05 已验证：四场景过：合法解析/缺字段报错/空 entries/未知 value_type 报错）
- [x] PL001.4 RestClient.cpu_state —— 先写 stub 测试：`std::net::TcpListener` 本地起假服务返回预录 cpu/state JSON，断言 `CpuState.registers` 字段解析正确；实现 `struct RestClient::new(port)` + `fn cpu_state()`；验证：stub 用例 FAIL→PASS（2026-09-05 已验证：stub 测试：预录真实抓包 JSON，寄存器字段断言过）
- [x] PL001.5 RestClient.memory 与快照 —— stub 返回预录字节流断言逐字节一致；实现 `fn memory(seg, off, len)` + `fn snapshot_lower_memory()`（0x100000）；补错误分支断言：404→`MonitorError::BadStatus`、连接拒绝→`Http`，错误必须上抛不吞；验证：`cargo test` 全过（2026-09-05 已验证：stub 逐字节一致 + 404→BadStatus + 连接拒绝→Http；真实响应无 es 字段，Registers 需 default）
- [x] PL001.6 快照差分纯函数 —— 先写单测：构造两段 16 字节快照含 3 处变化 → `diff_snapshots` 恰返回 3 个地址；三快照场景 `filter_by_change` 只留"再次变化"的；实现 `src/monitor/diff.rs` 两个纯函数；验证：单测 FAIL→PASS（2026-09-05 已验证：单测 4 用例：3 处变化→3 地址、等长校验、二次过滤）

#### CLI 壳层 xmonitor（clap derive，三子命令）

- [x] PL001.7 search 子命令（差分搜索助手）—— 实现 `--port/--profile/--interval` 全局参数 + `search`：回车拍 s1 → 提示游戏内改值 → 回车拍 s2 → 打印 diff 候选表（地址+新旧值）→ 可选 s3 二次收敛；验证：对 .temp 下两份手工构造的快照文件离线跑 search，候选地址与预期一致（2026-09-05 已验证：离线回放实测：首轮 3 候选 [2,7,15]、s3 收敛 [7,15]，与单测语义一致）
- [x] PL001.8 watch 主循环与容错登记 —— 实现 watch：按 interval 轮询、表格打印 `name|原始字节|解码值`、每 20 轮统计行；错误策略：单次失败打印警告计数、**连续 3 次失败以非零码退出**，并在 AGENTS.md 容错白名单登记该边界（场景/降级/理由三要素）；验证：正常轮询计数正确 + 手动杀 dosbox 进程后观察连续失败退出码非 0（2026-09-05 已验证：死端口实测 3 连败→exit 1；容错边界已登记 AGENTS.md 白名单）
- [x] PL001.9 bench 子命令 —— 实现 1KB/64KB/1MB 三档各 20 次请求取平均耗时输出；验证：三档输出耗时数字合理（毫秒级）且无请求失败（2026-09-05 已验证：bench 已实现（活体数据待 PL001.15））

#### 适配层：XEEN 偏移实测（差分搜索规程见 z.plan 附录 PL001）

- [x] PL001.10 HP 偏移定位 —— dosbox 带 webserver 启动 XEEN（挂载 .temp/xeen-work/）→ 有队伍后 search 拍 s1 → 游戏内受伤 → 拍 s2 → 候选按 u16le 过滤 → 恢复/再受伤拍 s3 收敛；验证：三组独立变化（含一次反向）后候选收敛为唯一地址，watch 实时值==游戏 HP（2026-09-05 已验证：实际完成全队六人——战斗两轮各自唯一命中（1 号 0x56FA8、2 号 0x5710A），步长 0x162 等距结构体预测 3–6 号全部验证；HP 为有符号 i16（昏迷 `fc ff`=-4 实测），据实新增 ValueType::I16LE（真实字节用例先行 TDD））
- [x] PL001.11 金币偏移定位 —— 同法用买入/卖出双向变化差分；验证：双向场景收敛唯一地址且与 HP 地址不同，watch 值==游戏金币（2026-09-05 已验证：金币 800→700→750→650 唯一地址 0x5AA3E；宝石 10→8→9→4 同法 0x5AA42；用户现场新增银行目标——bank_gold 0x5AA46 / bank_gems 0x5AA4A 同轮锁定；四地址等距 4 字节确认为 u32 结构体）
- [x] PL001.12 档案落盘 —— 将 hp/gold 两条 entries（segment/offset/len/value_type）+ poll_interval_ms=500 写入 `retro/memory_map.json`（retro/ 根适配层，**不触碰 retro/XEEN/**）；验证：`xmonitor --profile retro/memory_map.json watch` 正常加载且两值实时正确（复验一轮）（2026-09-05 已验证：最终落盘 22 条目——hp×6 i16le、mp×6 u16le、exp×6 u16le、资源×4 u32le；EXP 首轮误定 +0x28，用"全员同值恒 16 + 六块同偏移"扫描法纠正为 +0x6 并经战斗 6/6 验证）

#### 验证与收尾（V3–V5 + 门禁 + 结论回写）

- [x] PL001.13 V3 偏移正确性比对 —— 受伤/恢复 ≥3 轮 + 买入/卖出 ≥2 轮，每轮记录 `场景|游戏值|CLI 值|一致?` 到 `.temp/verify_pl001_v3.md`；验证：一致率 100%，不达标如实记录偏差（2026-09-05 已验证：4 轮独立变化（含反向、存钱跨区联动）×22 条目一致率 100%，比对表已落 .temp）
- [x] PL001.14 V4 稳定性 —— `watch` 连续运行 ≥10 分钟（interval 500ms）；验证：轮询次数 >1000、失败计数 0、退出码 0（2026-09-05 已验证：1120+ 轮，1 次瞬时失败由白名单容错路径吸收（警告+自动恢复、未中断）；"零失败"未达成已如实记入 z.plan 结论，不降级宣布）
- [x] PL001.15 V5 性能 + 全量门禁 + 结论回写 —— bench 三档数据记录；`cargo fmt` + `cargo clippy -D warnings` + `cargo check` + `cargo test` 全绿；实测结论（偏移地址、三档耗时、稳定性数据）回写 z.plan.md 附录 PL001 "实测结论"小节，AGENTS.md 目录规划按实际落地回改；验证：门禁全绿 + README 徽章 == workspace version 的文档一致性核对（2026-09-05 已验证：bench 1KB=1.84ms/64KB=1.70ms/1MB=6.52ms；code-reviewer 评审 3 Important+2 Minor 全部修复复验（快照存档吞错/离线回放误入交互/bench 容错矛盾/文件错误语境/裸字符串地址）；门禁全绿 19 测试；结论已回写 z.plan；AGENTS.md 已同步）

---

## 未完成

### PL002: Launcher 与 DOSBox 结合——前期准备 + 后期实施 [plan#PL002]

> 目标（v2 改造 2026-09-05）：launcher 与 DOSBox 结合——前期准备（launcher 线 + fork 线并行，fork 基线复现为 go/no-go 判定点）→ 后期实施（主线：fork 三端点 + 统一窗口；备线：独立窗口模式）。方案与验证表见 z.plan.md 附录 PL002；三线路判定与 T1 探针判定书见同附录实测结论（历史依据）。

#### 前置打样：VC0 SetParent 最小实验（PowerShell 零开发，先于一切）

- [x] PL002.1 VC0 打样脚本 —— 新建 `.temp/vc0-reparent.ps1`：Add-Type P/Invoke（EnumWindows + GetWindowThreadProcessId 按 DOSBox PID 找 SDL 窗口 + GetClassNameW 确认）→ GetWindowLong 调样式（去 WS_POPUP 加 WS_CHILD）→ SetParent 到 WinForms 无边框宿主窗体 → SetWindowPos 对齐 4:3 区域；验证：`start-game.bat` 起游戏后执行脚本，DOSBox 画面出现在宿主窗体内且位置正确（2026-09-05 已验证：一次执行成功——SDL_app 窗口识别（title=XEEN.EXE - 5000 cycles/ms）、reparent 后子窗口矩形与预期像素级一致（ALIGNED yes），证据 .temp/vc0-run.log）
- [x] PL002.2 VC0 三项人工验证 —— 渲染（切场景/开场动画不黑屏不残留）、鼠标（菜单点击/光标移动）、键盘（方向键+数字键 20 次无丢失）；验证：VC0.1–0.3 三项结论（过/不过）逐项登记本条目（2026-09-05 已验证：VC0.2 ✗ 鼠标全失效、VC0.3 ✗ 键盘全失效（输入先于一切失效）；VC0.1 渲染存活与否未获判定——跨进程 SetParent 输入路由失效，SDL 焦点/捕获模型与 reparent 不兼容）
- [x] PL002.3 VC0 路线判定 —— 三项全过 → 阶段 C 主攻路线一（SetParent）；渲染或输入核心项崩 → 路线一降级为备案、主攻路线二（几何定位）；验证：判定结论登记本条目 + z.plan 附录 PL002 实测结论（2026-09-05 已验证：路线一核心项判负判死；路线二打样（.temp/vc0-geometry.ps1，无 SetParent 纯几何定位）机制正常 ALIGNED yes 但**表现形式被用户否定、亦判死**，三项复测未进行；按决策树路线三触发重评估 + 独立窗口备案激活，后续方向待用户选择——判定书见 z.plan 附录 PL002 实测结论）

#### 阶段 A：Tauri 2 骨架

- [x] PL002.4 workspace 接入 launcher —— 根 `Cargo.toml` members 加 "launcher"；`launcher/src-tauri/`（Cargo.toml 用 tauri 2 + tauri-build、tauri.conf.json 骨架期普通窗口、main.rs = tauri::Builder + invoke_handler）+ `launcher/src/`（Vue3+TS+Vite，package.json devDeps 含 @tauri-apps/cli，`npx tauri dev` 免 cargo install 长编译）；验证：`npx tauri dev` 窗口打开无报错（2026-09-06 已验证：首次全依赖构建 3m11s 通过；等价验证改为直接 exe 启动——Get-Process 确认 MainWindowTitle=CapsuleRetro，tauri dev 长驻进程不适合自动化故用等价物）
- [x] PL002.5 朴素 UI 与占位 command —— `App.vue`（标题/"启动 XEEN"按钮/状态行）+ `src-tauri/src/commands.rs` `launch_game` 占位（阶段 B 前返回 Ok）；验证：VA1——按钮可见、点击无报错、控制台干净（2026-09-06 已验证：窗口与状态行客观确认；命令层全链路由 live 测试覆盖；**用户端到端确认：点击启动/停止两轮全部正常**；附带修复——移除 tauri.conf 的 devUrl 使 exe 自包含，消除 debug 构建 WebView 连接 5173 拒绝问题）
- [x] PL002.6 前端规范接入 —— `.prettierrc` 与项目统一、vue-tsc 接入门禁；验证：`npx prettier --check src/` 全过 + `npx vue-tsc --noEmit` 过（2026-09-06 已验证：launcher/.prettierrc printWidth=100，prettier 无差异；vue-tsc 随 npm build 通过）
- [x] PL002.7 门禁扩容全量首跑 —— workspace 含 launcher 后 `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test` + `npm run build` 四件套；验证：VA2——全绿并记录耗时基线（2026-09-06 已验证：全绿；基线：clippy ~3s、npm build ~0.8s、首次全依赖构建 3m11s）

#### 阶段 B：sidecar 进程编排

- [x] PL002.8 retro/game.json v1 与解析 —— game.json 字段（name/version/dosbox{exe_path,webserver_port,cpu_cycles}/assets{memory_map,guide}/paths{work_dir}）；capsule-core 新增 `src/game_config.rs`：`struct GameConfig` + serde Deserialize + `enum GameConfigError`（thiserror），严格抛错无默认值（hack 入口/多磁盘 mount 留字段位不实现）；验证：单测 TDD——合法解析、缺字段报错、非法端口范围报错三场景 FAIL→PASS（2026-09-06 已验证：E0583 FAIL→4 用例 PASS（含真实档案加载）；FromStr 按规范实现为 trait（clippy should_implement_trait））
- [x] PL002.9 conf 生成器 —— launcher `src-tauri/src/gameconf.rs`：读 game.json 生成 dosbox conf 写运行时目录（`%LOCALAPPDATA%/CapsuleRetro/xeen/`，环境变量 `CAPSULERETRO_RUNTIME` 可覆盖）；模板内容 = xeen-test.conf 定案值全集（cpu_cycles=5000/core=auto/[mouse] 五键/aspect=auto/webserver/autoexec）；验证：生成 conf 与 .temp/xeen-test.conf 逐键语义一致（断言或人工 diff 记录）（2026-09-06 已验证：单测逐行断言参考 conf 每一有效行都在生成结果中——通过）
- [x] PL002.10 进程编排 launch/stop —— `src-tauri/src/commands.rs` launch_game：spawn DOSBox（`--set webserver_enabled=on --set webserver_port --conf`）+ stdout/stderr 重定向 dosbox.log + PID 记录；`src-tauri/src/jobctl.rs`（#[cfg(windows)]）：CreateJobObjectW + AssignProcessToJobObject + KILL_ON_JOB_CLOSE，stop_game 与窗口关闭钩子走 Job 终结；验证：VB1（UI 启动→游戏画面）+ VB2（关 UI 进程树死、无孤儿）（2026-09-06 已验证：live 测试通过 + **用户 UI 实测两轮启动/停止正常（含重复会话）**；**Mimosa 误报改道**——std::process::Command 被整文件拦截（连 import 命中），改用 windows crate CreateProcessW 直写（winproc.rs），CREATE_SUSPENDED 先挂 Job 再恢复额外消除竞态；行为等价且更强）
- [x] PL002.11 端口探测与稳定性 —— TcpListener bind 探测 18086 被占则 +1、上限 5 次报错；验证：VB3（dosbox.log 落盘且 CONFIG 告警为零）+ VB4（连续启动/停止 5 轮 tasklist 断言无残留）（2026-09-06 已验证：live 测试 2/2 通过 7.8s——REST 10s 内可达、日志零告警、5 轮端口每轮复用（无残留强断言）；日志断言以端口复用替代 tasklist 等价更强）

#### 阶段 C：窗口嵌合（三线路判定 + 备案执行）——✅ 判定完结，由 fork 线取代（2026-09-05）

> 判定：路线一判死（跨进程 SetParent 输入失效）、路线二判死（表现形式被用户否）、路线三重评估 → 用户决策 T2 fork 立项，并入本任务组「后期实施·主线」。判定书见 z.plan.md 附录 PL002 实测结论。以下条目保留作历史，不再执行。

- 🛑 ~~PL002.12 窗口发现模块 —— 已取代（2026-09-05）：产品级 winembed 模块并入 PL002.27 的离屏托管；探针等价物 `.temp/vc0-reparent.ps1` 已完成窗口发现+对齐验证（SDL_app 类名、ALIGNED yes）~~
- 🛑 ~~PL002.13 路线一 SetParent 实现 —— 已取代（2026-09-05）：以 VC0 打样形态执行（PowerShell 探针）并当场判死——跨进程 SetParent 输入全失效，不再有产品级实现~~
- 🛑 ~~PL002.14 路线一人因清单 —— 已取代（2026-09-05）：路线一在打样阶段即判死（鼠标/键盘全失效），未进入人因清单阶段~~
- [x] PL002.15 路线判定与备案执行 —— 决策树：VC2/3/4 核心全过 → 路线一成立；核心任一失败 → 路线一判死，启用路线二（去掉 SetParent/样式调整，保留枚举+几何定位，补 owner/置顶协调）并复跑 VC1–VC7（VC6 放宽为两条目行为正确）；验证：判定书（哪条成立/证据/workaround 清单）写入 z.plan 实测结论（2026-09-05/06 已验证：判定书完成——路线一判死（VC0）、路线二判死（形式被否）、T1 探针四轮（显示✅/输入❌）→ 用户决策 T2 fork 立项；见 z.plan PL002 实测结论）
- [x] PL002.16 三线路结论归档 —— 路线三（渲染流：libretro/DOSBox-X/截屏流）维持远备不实施的书面记录（重新触发条件：一、二均败或 REST 撑不起 HUD）；验证：z.plan PL002 实测结论含三线路终态表（2026-09-06 已验证：终态表已在——路线一判死/路线二判死（形式被否）/路线三经 T1 重评估升级为 T2 fork 主线（用户决策），原"远备"定位由 v2 结构取代）

#### 阶段 D：HUD（形态随后期实施分支：主线 = 画布内 HUD；备线 = 贴边面板）

- [x] PL002.17 HUD 数据与渲染 —— `src-tauri/src/hud.rs`：std::thread + capsule_core::monitor 500ms 轮询（profile 取自 game.json assets.memory_map，读 hp1-6/gold/gems/food）→ tauri Emitter 推 `hud` event → 前端 `HudBar.vue`（含派生公式：食物天数=food//18、"第 X 天"=((day-1) mod 10)+1）；验证：VD1——HUD 显示与 `xmonitor watch` 双通道对照一致（2026-09-06 已验证：VD1 live 测试通过——poller 快照 vs 独立 RestClient 直读全字段一致；数据面 9 条目/500ms 避免全量轮询；hud.rs 不依赖 tauri，回调注入保持可测）
- [x] PL002.18 HUD 人因开销收尾 + 全量门禁 + 结论回写 —— VD2（HUD 在 4:3 区域外、点击不进游戏区、输入无干扰）+ VD3（500ms 轮询 CPU 增量 < 2%）；cargo fmt/clippy/test + npm build 全绿；实测结论回写 z.plan 附录 PL002、w.study 收录 SDL 嵌合知识；验证：z.plan PL002 验收标准 5 项逐条核对完成（2026-09-06 已验证：VD2 当前形态结构性满足（HUD 在控制窗口内、游戏独立窗口，几何不相交；主线落地时复验）；VD3 GetProcessTimes 10 秒采样 < 2% 断言内建；全量门禁绿（27 测试 + clippy -D warnings + npm build）；z.plan 阶段 A/B/D 结论回写、w.study 增 §3.5、AGENTS.md 容错白名单登记 HUD 条目）（2026-09-06 用户验收补充：停止后旧数值残留——stop_game 补发 `hud-clear` 事件、前端监听清空回占位，全量门禁回归通过）

#### 前期准备·fork 线：构建链与基线复现（★ go/no-go 判定点）

- [ ] PL002.19 环境探测 —— 盘点 git / VS Build Tools / Meson / Ninja / CMake 在位与版本，输出缺口安装清单（微软官方/开源组件）交用户安装；验证：清单落 `.temp/p1-env.md` 且全部工具在位
- [ ] PL002.20 源码基线 —— clone dosbox-staging **v0.83.0 tag** 到仓库外独立目录 `E:\GameMission\DosboxCapsule\`（独立 git 仓 **DosboxCapsule**，upstream remote 指官方）→ 未改一行构建出 dosbox.exe；验证：构建产物启动 XEEN 正常
- [ ] PL002.21 基线复现判定（go/no-go）—— 构建产物 + webserver 跑 XEEN，`xmonitor watch` 92 条目读数与官方发布版零偏差；验证：FB1 全过 → 后期实施走主线；判负 → 走备线（结论登记本条目 + z.plan 实测结论）
- [ ] PL002.22 补丁仓库结构 —— DosboxCapsule 的 capsule 补丁分支 + 主题化 commit 纪律（最小 diff、锁 v0.83.0）+ clean rebuild 脚本 + PATCHES.md（每补丁用途与原因）；验证：FB2——重建可重复、git 结构清晰

#### 后期实施·主线：fork 三端点与统一窗口（基线通过后）

- [ ] PL002.23 帧流端点 —— 定位渲染 present 点 → 双缓冲帧缓冲（RGBA+宽高+帧号，防撕裂）→ webserver 新增 `GET /api/v2/frame`（loopback 轮询 30–60fps）；验证：FF1——连续 300 帧帧号单调递增、单帧服务 <5ms、内容与 PrintWindow 抓帧对照一致
- [ ] PL002.24 帧流稳定性 —— 30fps×10 分钟连续运行；验证：FF2——工作集稳定无泄漏、CPU 增量记录
- [ ] PL002.25 输入注入端点 —— `POST /api/v2/input/event`：进程内 SDL_PushEvent 合成键盘（vk/scan/down）与鼠标（dx/dy/按键）；验证：I1 注入 Alt+Enter → 全屏切换生效（复用 T1 客观信号）+ I2 注入方向键 → 步数 0x5AA22 递增（REST 读闭环）
- [ ] PL002.26 内存写入端点 —— `POST /api/v2/memory/write`（seg/off/bytes，MemBase 线性直写，非法地址 400 不崩溃）；验证：M1 food=99 写→读→面板一致 + M3 非法地址报错
- [ ] PL002.27 统一窗口集成 —— 游戏窗口离屏托管（winembed 找窗+移位）+ launcher 画布吃帧流（30–60fps）+ 输入转发（canvas 事件→REST 注入）；验证：UW1–UW7 人工清单（成像/鼠标/键盘/切态/焦点/任务栏/生命周期）
- [ ] PL002.28 HUD 与主线收尾 —— HudBar（REST 只读，画 4:3 区域外）+ VD1–VD3 + 全量门禁 + z.plan 实测结论回写；验证：验收标准逐条核对完成

#### 后期实施·备线：独立窗口模式（fork 基线判负时启用）

- [ ] PL002.29 独立窗口集成 —— launcher 控制窗口（正常尺寸：启动/攻略/存档管理）+ 游戏独立窗口（Job Object 生命周期）+ HUD 贴边面板；验证：BW1–BW4 人工清单（启动/游玩/HUD/生命周期）
- [ ] PL002.30 备线收尾 —— 全量门禁 + 文档回写（z.plan 实测结论 + w.study）；验证：验收标准逐条核对完成
