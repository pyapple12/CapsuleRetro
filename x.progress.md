# 进度追踪（x.progress.md）

> 文件职责：任务清单与进度追踪，与 `z.plan.md`（方案与审计归档）配套：方案在 z.plan 展开，执行拆条在本文件勾选。结构：**`## 已完成 ✅` 区在前、`## 未完成` 区在后**，任务完成后整组移动位置。
> 格式速查：任务组 `### {组号}: {标题} [来源引用]`，组内用 `#### {小节}` 分层；子任务 `- [ ] {组号}.{序号} {标题} —— {做法}；验证：{检验方式}`。**做法必须含可执行的实现办法**（机制 / 文件 / 函数级，读到条目即知怎么下手，不能只写"要做什么"）；**验证必须是可断言的观测点**（具体检验动作 + 预期结果，"登记结论"类措辞须绑定客观判据）。审计修复任务组 FIX{NNN} 由 audit-report 归档环节生成，条目格式 `- [ ] FIX{NNN}.{序号} [P{级别}] {标题} —— {做法}；验证：{检验方式}`，编号规则见 `.agents/skills/audit-report`。

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

### PL002: Launcher 与 DOSBox 结合——前期准备 + 后期实施 [plan#PL002]

> 目标（v2 改造 2026-09-05）：launcher 与 DOSBox 结合——前期准备（launcher 线 + fork 线并行，fork 基线复现为 go/no-go 判定点）→ 后期实施（主线：fork 三端点 + 统一窗口；备线：独立窗口模式）。方案与验证表见 z.plan.md 附录 PL002；三线路判定与 T1 探针判定书见同附录实测结论（历史依据）。
>
> **终态（2026-09-07）**：fork 基线 FB1 判 go，后期主线 PL002.23–28 全部完结；备线 PL002.29/30 休眠不执行，登记 [problems#16]。

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

- [x] PL002.19 环境探测 —— 盘点 git / VS Build Tools / Meson / Ninja / CMake 在位与版本，输出缺口安装清单（微软官方/开源组件）交用户安装；验证：清单落 `.temp/p1-env.md` 且全部工具在位（2026-09-06 已验证：MSVC 侧 VS Build Tools 2026 v18.8.2 + cl 14.51 + SDK 10.0.26100 在位（最大件免装）；两项兼容雷预排——CMake 4×zlib-ng 2.1.5（3.5.1 在 CMake 4 保留线上，兼容）、Meson vsenv 自动激活 cl；缺口 meson/ninja/cmake 经 pip --user + scoop 装齐为 1.12.0 / 1.13.2 / 4.4.3，pip Scripts 目录补入用户 PATH（HKCU 追加 + WM_SETTINGCHANGE 广播，重开终端生效）；`.temp/p1-env.md` 为探测全记录）
- [x] PL002.20 源码基线 —— clone dosbox-staging **v0.83.0 tag** 到仓库外独立目录 `E:\GameMission\DosboxCapsule\`（独立 git 仓 **DosboxCapsule**，upstream remote 指官方）→ 未改一行构建出 dosbox.exe；验证：构建产物启动 XEEN 正常（2026-09-06 已验证：P1 tag/remote/仓库身份 ✓；P2 产物 `--version` = 0.83.0 (7b400)；P3 构建后 `git status --porcelain` = 0 行零改动实证；P4 启动 XEEN——模式 13h 进入、REST 66 秒三连 200、5/5 档案条目读出真实数值、用户目视确认画面 OK。**路线变更：Meson 判死**（tag 的 meson 构建自身损毁——misc 目录悬空 + 上游 fbef15885 已焚毁 meson）→ 正式改 **CMake+vcpkg**（官方 Windows CI 路径）：vcpkg@6283825b 34 包、ClangCL 22.1.3、VS18 2026 生成器 + CMAKE_GENERATOR_INSTANCE 钉死 BuildTools 实例；结论回写 z.plan 附录、构建脚本 `.temp/p1-cmake-build.bat`、探测全记录 `.temp/p1-env.md`）
- [x] PL002.21 基线复现判定（go/no-go）—— 构建产物 + webserver 跑 XEEN，`xmonitor watch` 92 条目读数与官方发布版零偏差；验证：FB1 全过 → 后期实施走主线；判负 → 走备线（结论登记本条目 + z.plan 实测结论）（2026-09-06 已验证 **FB1 = go**：`.temp/fb1-capture.sh`+`fb1-compare.py` 四快照对照——官方发布版与构建版串行启动 XEEN 各拍两份 1MB 快照，自对照确认双稳定（首轮暴露构建版 5 秒稳定期不足、判定口径空洞并修正为 12 秒重跑——探针自证），交叉对照 92/92 条目字节级零偏差（fb1-report.json）；ClangCL 构建产物 DOS 内存行为与官方发布版一致，决策走主线，判定书入 z.plan 附录）
- [x] PL002.22 补丁仓库结构 —— DosboxCapsule 的 capsule 补丁分支 + 主题化 commit 纪律（最小 diff、锁 v0.83.0）+ clean rebuild 脚本 + PATCHES.md（每补丁用途与原因）；验证：FB2——重建可重复、git 结构清晰（2026-09-06 已验证：capsule 分支基于 v0.83.0 创建（脱离游离 HEAD），origin 接线 pyapple12/DosboxCapsule 公开 fork（用户建仓+改名，upstream 关系保留）、双 remote（origin 推送/upstream 升级）；FB2-a clean rebuild 全绿——build 目录全新配置编译 BUILD-EXIT:0，产物 `--version` = 0.83.0 (7b400)；FB2-b 结构清晰——capsule diff 零起步（补丁清单空表，PL002.23 起逐条登记）；commit+push 命令草案交用户执行（fork 仓提交风格对齐上游英文 conventional，不套 CapsuleRetro 四段式）（2026-09-07 结构修订·用户拍板：维护套件集中 CapsuleRetro `tools/dosbox-fork/` 版本化——PATCHES.md + rebuild.bat + baseline-capture.sh/compare.py（FB1 脚本升级为常备升级门槛，迁出 .temp），一致性验证闭环（cmake 链逐行相等、capture 执行行 MD5 全等、compare 仅 docstring+错别字差异、新位置功能跑通 PASS）；capsule 分支 diff 保持 100% 代码补丁，fork 仓以 FORK.md 单提交声明用途）（2026-09-07 独立仓修订·用户拍板：删 fork 重建同名独立公开仓切断 fork 关系（陷阱横幅消失），本地 capsule 改名 main 推 main + v0.83.0 tag（默认分支自动 main），FORK.md 独立仓化、PATCHES.md 简化为单分支 + tag 锚点模型，上游 CI 噪音经仓库设置关闭）

#### 后期实施·主线：fork 三端点与统一窗口（基线通过后）

- [x] PL002.23 帧流端点 —— 定位渲染 present 点 → 双缓冲帧缓冲（RGBA+宽高+帧号，防撕裂）→ webserver 新增 `GET /api/v2/frame`（loopback 轮询 30–60fps）；验证：FF1——连续 300 帧帧号单调递增、单帧服务 <5ms、内容与 PrintWindow 抓帧对照一致（2026-09-07 已验证：挂钩点定为两渲染器 `EndFrame`（curr→last 拷贝完成点，scaler 输出帧，BGRA 32bit），`CapsuleFrame` 共享缓冲（gui 层新文件，webserver 关闭时 Capture 仅一次原子读）→ `GET /api/v2/frame`（header 元数据 + BGRX body，503 先于首帧），清洁重建全绿（commit a8e395cbf，11 文件 +194/-1）；FF1——活性 300 轮询帧号 3→670 零回退、单帧服务 200 连发 keep-alive 最慢 2.27ms（口径修正：curl 进程启动开销不计入端点成本）、尺寸自洽 640×200/512000B，对照物官方版探针 FAIL 闭环；内容对照：同刻采集 stream PNG（640×200 pre-shader）vs PrintWindow 窗口图（2000×1200 画布内 ~1067×800 post-shader），用户确认内容一致——分辨率差异系设计使然（画布消费 pre-shader 小帧自行做 4:3 缩放，shader 管线留在本地全屏体验）✓；遗留 PL002.27 关注：帧流暂未携带像素纵横比元数据，画布 4:3 拟合时需补充（后续小补丁或按已知模式处理））
- [x] PL002.24 帧流稳定性 —— 30fps×10 分钟连续运行；验证：FF2——工作集稳定无泄漏、CPU 增量记录（2026-09-07 已验证：PowerShell HttpClient keep-alive 30fps×600 秒，实发 12816 请求 **0 失败**；帧号 21 段采样零停滞、总增量 5747（9.1fps 内容变化帧，符合 XEEN 静态画面特征）；工作集 180.3→181.6MB **仅涨 1.3MB 无泄漏**；CPU 判定经对照实验——轮询期总量 20.32% vs 无轮询基线 21.07% = **轮询净增量 ≈0**（优于 VD3 HUD 轮询 <2% 口径）。两处阈值口径修正如实记录：①"帧增量≥9000"系混淆渲染帧与内容变化帧（帧号仅计 EndFrame 内容变化帧），正确断言=零停滞+持续产帧；② CPU 阈值从总量 15% 修正为净增量口径（FF2 原文即"CPU 增量"，对照实验定案）。`.temp/ff2-samples.csv` 存证）
- [x] PL002.25 输入注入端点 —— `POST /api/v2/input/event`：合成键盘与鼠标事件注入；验证：I1 注入 Alt+Enter → 全屏切换生效（复用 T1 客观信号）+ I2 注入方向键 → 步数 0x5AA22 递增（REST 读闭环）（2026-09-07 已验证：设计经实测修正——键盘走 Bridge+`MAPPER_CheckEvent`（与真实事件泵 sdl_gui.cpp:2617 同路，绕开 SDL 暂停清队列坑），鼠标走 `SDL_PushEvent`；键名表 62 键 + 6 修饰键，commit 随 25 号补丁。**I1 PASS**：窗口 869×678 → 1536×864 全屏切换往返；**协议事实**：mapper 组合键须时序注入（leftalt down → return down/up → leftalt up），单发带 mod 位事件不触发（mapper 匹配依赖自身修饰键按下状态）；**I2 PASS**：注入 3 次右移 → steps 0x0007 → 0x000e（+7，little-endian 解码），注入到游戏的完整闭环；负例 f13 → 400
- [x] PL002.26 内存写入端点 —— 内存写入 REST 通道（非法地址 400 不崩溃）；验证：M1 food=99 写→读→面板一致 + M3 非法地址报错（2026-09-07 已验证：设计修正——上游 v0.83 已自带 `PUT /api/v1/memory/:seg/:off`（If-Match 乐观锁 + 越界检查），**无需新写端点**；唯一补丁 = error_handler 将 `invalid_argument`/`out_of_range` 映射 400（区分客户端错误与服务端 500）。**M3 PASS**：PUT 越界 0x99999999 → 400 不崩溃；**M1 PASS**：游戏内状态 PUT food=99 → 读回 `63 00`，2 秒后仍持久（实测发现：标题屏下游戏活跃代码覆盖人物数据区，写入验证必须在游戏内状态）；HUD 面板与 M1 同 REST 同档案通道（VD1 已证双通道一致），面板目视留 PL002.27 集成复验
- [x] PL002.27 统一窗口集成 —— 游戏窗口离屏托管（winembed 找窗+移位）+ launcher 画布吃帧流（30–60fps）+ 输入转发（canvas 事件→REST 注入）；验证：UW1–UW7 人工清单（成像/鼠标/键盘/切态/焦点/任务栏/生命周期）（2026-09-07 已验证：实现 = winembed.rs（EnumWindows 按 PID → 移屏外 x=-3200 + WS_EX_TOOLWINDOW 摘任务栏）+ get_frame/inject_input 命令（ureq 透传，20 字节元数据头+二进制 body，零 CORS 零 fork 改动）+ GameCanvas.vue（30fps 循环+帧号去重+BGRX→RGBA+4:3 整数缩放+PointerLock 鼠标 30ms 合帧+键盘 code→键名映射）+ App.vue 双模式布局。**实测根因链三条**：① 画布黑屏 = game.json 仍指官方版（无帧流端点）→ 改指 DosboxCapsule 构建产物；② 注入光标不动 = 离屏窗口宿主光标恒"窗外"，cursor_is_outside 丢弃一切鼠标事件 → fork 补丁 #4 注入原语（MOUSE_InjectMoved/Button 绕判定 + cursor_x_abs/y_abs 随位移累进 clamp——nomouse 态驱动走 seamless 绝对路径必需）；③ 中键"脱离"反致宿主光标失效 = 非捕获态中键触发捕获、离屏窗口把宿主光标拖进屏外 → **nomouse 形态定案**（dosbox 永不抓宿主光标，鼠标输入全走注入）。**UW 清单**：UW1 ✓ 成像（用户目视）、UW2 ✓ 鼠标注入（用户确认）、UW3 ✓ 键盘（用户经画布键盘完成菜单导航进游戏）、UW4 ✓ 待机→标题→游戏切态走通（帧尺寸 header 动态）、UW5 ✓ 宿主光标异常已根治（20 次压测并入日常观察）、UW6 ✓ 客观（rect x=-2560（125% DPI 虚拟化）+ TOOLWINDOW 置位）、UW7 ✓（Job KILL_ON_JOB_CLOSE 回归 VB2））
- [x] PL002.28 HUD 与主线收尾 —— HudBar（REST 只读，画 4:3 区域外）+ VD1–VD3 + 全量门禁 + z.plan 实测结论回写；验证：验收标准逐条核对完成（2026-09-07 已验证：HudBar 进统一窗口顶栏（4:3 画布外，DOM 层不进画布、不吃画布输入）；VD1 双通道一致（阶段 D 已证，通道未变）、VD2 结构性满足（HUD 在 DOM 顶栏与 canvas 元素分离）、VD3 轮询 CPU 净增量≈0（FF2 对照实验）；全量门禁 = cargo fmt + clippy -D warnings + cargo test --workspace + vue-tsc + npm build 全绿；z.plan 后期实施开展结论回写完成）

---

### FIX001: 第1轮审计修复 [audit#A001]

> 来源：附录 A001（第 1 轮全量审计，2026-09-07）。43 条 P 级按 [P0]→[P3] 分层；观察项 23 条经用户确认全部保持参考级，未入清单。条目标题含（A001#n）回溯编号。
> **✅ 全组完成（2026-09-08）**：44/44 勾结。全量门禁全绿（fmt/clippy/test 36 过/doc 零警告/prettier/vue-tsc/npm build）；反向验收逐项记录见 `.temp/fix001-verification.md`。**live 复核已收口（2026-09-08 双轮实测）**：①会话退出感知 ✅ ②UI 无冻结 ✅ ③30fps 流畅+帧校验零误报 ✅ ⑤release 日志落盘 ✅（实测修复 1 bug：logging::init 补目录创建）；④小键盘注入由 y.problems #18 承接待实体键盘；FIX001.43 CSP 按条目原文随 PL003 执行；y.problems #12 保持挂账随 PL003 测试资产闭环。

- [x] FIX001.1 [P0] CreateProcessW 参数引号包裹（A001#1）—— winproc.rs:59-63 spawn_suspended 对 args 各段统一走 `quoted()` 包裹，同步修正 :49 文档注释；验证：新增单测构造含空格 conf 路径断言命令行含引号（cargo test -p launcher）
- [x] FIX001.2 [P0] rebuild.bat 构建失败退出码归零（A001#2）—— :25 build 步补 `if errorlevel 1 exit /b 1`（REBUILD-EXIT 输出保留在退出前）；验证：临时注入编译错误跑 rebuild.bat 断言退出码非 0，复原后再跑断言 0
- [x] FIX001.3 [P0] baseline-compare.py 假 GO 双路径（A001#3）—— :40-48 dynamic 分类细分为"仅 s1 不稳/仅 s2 不稳/双侧不稳"，单侧不稳列名并退出码非 0；:73 静态条目数低于总数一半时退出码非 0；验证：构造单侧不稳定与空静态集两组输入离线断言退出码
- [x] FIX001.4 [P1] 比较切片越界静默截断（A001#4）—— baseline-compare.py slice_at 补 `addr+length > len(mem)` 时 raise ValueError；验证：构造越界坐标条目离线断言报错
- [x] FIX001.5 [P1] 会话结束无感知（A001#5）—— commands.rs 增加进程退出监听线程（wait_exit + emit `session-ended` 事件并清槽），App.vue 监听置 running=false，GameCanvas 收事件停循环；验证：live 手杀 dosbox 进程 3 秒内 UI 回待机态
- [x] FIX001.6 [P1] 帧协议静默错值链（A001#6）—— commands.rs:100 超限截断改显式报错；GameCanvas.vue drawFrame 开头校验 `buffer.length ≥ HEADER+(height-1)*pitch+width*4` 且 `pitch ≥ width*4` 不符抛错走失败路径；验证：cargo test 畸形响应 stub + vue-tsc
- [x] FIX001.7 [P1] GameCanvas 空 catch 吞错（A001#7）—— :99-111 try 域缩小到 invoke 调用；catch 内 console.warn + 连续失败计数（超限显式置错误状态并停循环）；AGENTS.md 容错白名单登记帧轮询条目；验证：vue-tsc + 杀进程观察 UI 显式错误
- [x] FIX001.8 [P1] 同步 command 阻塞主线程（A001#8）—— get_frame/launch_game/inject_input 改 async fn（Tauri 2 异步 command），launch 锁外构造 Session 锁内仅 swap；验证：cargo clippy/test + live 界面拖动不冻结（需验证 Tauri 2 调度行为后定案）
- [x] FIX001.9 [P1] ResumeThread 返回值未检查（A001#9）—— winproc.rs:108-110 检查 `!= u32::MAX` 失败转 WinResult；验证：cargo test 编译路径 + 代码审查
- [x] FIX001.10 [P1] probe_port TOCTOU 窗口（A001#10）—— :136 文档标注竞态；launch 后增 webserver 就绪探测（复用 capsule-core::monitor REST 重试 N 秒）；验证：live 启动日志出现就绪确认
- [x] FIX001.11 [P1] Job assign 失败挂起僵尸（A001#11）—— commands.rs:184-190 失败路径先 TerminateProcess 再返回错误；验证：代码审查 + cargo test
- [x] FIX001.12 [P1] embed 固定 2s 竞态无重试（A001#12）—— commands.rs:232-240 改轮询重试（500ms × 10 次找窗）；验证：live 人工模拟慢启动场景
- [x] FIX001.13 [P1] work_dir 不校验（A001#13）—— launch 前校验 work_dir 存在且含游戏可执行文件，缺失严格报错提示释放流程未跑；验证：清空 work_dir 启动断言错误消息（live）
- [x] FIX001.14 [P1] rest memory() 短读静默（A001#17）—— rest.rs:118-135 读后校验 `buf.len() == len` 否则新增 ShortRead 错误变体，统一返回值文档；验证：cargo test 新增短读 stub 用例
- [x] FIX001.15 [P1] search 离线参数不成对（A001#18）—— xmonitor.rs clap 用 requires 强制 s1↔s2 成对，s3 单独出现显式报错退出码 2；验证：cargo test + 手工只传 --s1 断言报错
- [x] FIX001.16 [P1] 档案 len 宽度无加载期校验（A001#19）—— MemoryMap::load 校验 `len != 0 && len == value_type.width()`；poll_once 错误附 entry.name；验证：cargo test 非法档案用例
- [x] FIX001.17 [P1] 容错白名单登记制缺口（A001#20）—— AGENTS.md 白名单补四条登记（rest serde(default) 置 0 / xmonitor 快照存档降级 / embed 失败退化 / LOCALAPPDATA 兜底——末者建议改严格抛错）；验证：文档三要素核对 + cargo test
- [x] FIX001.18 [P1] capture 脚本全机杀 dosbox（A001#22）—— baseline-capture.sh:28,37 改按记录 PID 杀（PowerShell Stop-Process -Id），//IM 兜底删除或加警告；验证：并发第二个 dosbox 跑采集断言其存活
- [x] FIX001.19 [P1] rebuild.bat rmdir 失败静默（A001#23）—— :16 rmdir 后 `if exist ...build exit /b 1`；验证：占用 build 目录跑脚本断言非 0
- [x] FIX001.20 [P1] NumpadEnter 死条目与大小写（A001#24）—— GameCanvas.vue:32 Enter 特判提前于 Numpad 前缀分支；Numpad 分支统一小写；与 fork `/api/v2/input/event` 键名表核对 numpad\* 契约；验证：vue-tsc + 注入小键盘回车人工验证
- [x] FIX001.21 [P1] 仓库根解析依赖 cwd（A001#26）—— gameconf.rs resolve_repo_root 失败改显式报错并报告探测起点；exe 路径锚定留 Phase 3/4 打包形态一并处理；验证：cargo test + 非仓库根目录启动行为核对
- [x] FIX001.22 [P2] release 无日志通道（A001#14）—— 引入文件日志（自写 append 落 runtime_dir 或 log+fern）替换关键 eprintln!；验证：release 构建跑一轮查日志文件落盘
- [x] FIX001.23 [P2] 端口双处事实源（A001#15）—— xmonitor 增加 `--game-json` 从 GameConfig 派生端口或 help 注明与 game.json 同步关系；验证：cargo test + 帮助文本核对
- [x] FIX001.24 [P2] runtime_dir 游戏身份硬编码（A001#16）—— gameconf.rs runtime_dir(game) 改传参由 config.name 小写派生；验证：cargo test 改断言
- [x] FIX001.25 [P2] 版本第二处事实源（A001#21）—— game.json version 语义定案（游戏自身版本 → AGENTS.md 条目明示豁免；产品版本 → workspace 派生注入），package.json version 注明不参与发布；验证：文档核对
- [x] FIX001.26 [P2] 画布每帧重分配（A001#25）—— GameCanvas.vue offscreen 尺寸变化才赋值 + ImageData 按尺寸缓存复用；验证：vue-tsc + 30fps 目视流畅
- [x] FIX001.27 [P2] 魔法数未命名（A001#41）—— commands.rs:70 提 `POLL_INTERVAL_MS` 常量；GameCanvas.vue 提 `MOUSE_BATCH_MS`/`FPS_WINDOW_MS`；验证：clippy + vue-tsc
- [x] FIX001.28 [P2] GameConfig.name 无校验（A001#43）—— validate() 补非空与 `[A-Za-z0-9_-]` 合法字符校验；验证：cargo test 非法名用例
- [x] FIX001.29 [P3] frameFps 死代码（A001#27）—— GameCanvas.vue 删除 fps 统计机制或接入 UI 角标；验证：vue-tsc
- [x] FIX001.30 [P3] window_rect 死代码（A001#28）—— 删除 winembed.rs:76-94（探针需要时移 .temp）；验证：cargo build
- [x] FIX001.31 [P3] serde_json 未使用依赖（A001#29）—— 删 launcher/src-tauri/Cargo.toml:12；验证：cargo build
- [x] FIX001.32 [P3] Registers 字段缺文档（A001#30）—— rest.rs:40-66 补齐 `///` 注释；验证：cargo doc 无警告
- [x] FIX001.33 [P3] "无会话"返回语义不一致（A001#31）—— 统一三 command 返回约定并与前端核对；验证：cargo test + vue-tsc
- [x] FIX001.34 [P3] 端口探测测试重复（A001#32）—— 删 launcher_session_test.rs 副本保留 commands.rs 内联版；验证：cargo test
- [x] FIX001.35 [P3] use 位置违规（A001#33）—— launcher_session_test.rs:41 上移文件头 use 块；验证：cargo fmt
- [x] FIX001.36 [P3] 快照目录相对 CWD（A001#34）—— xmonitor.rs:156 向上搜索仓库根或加 `--out-dir`；验证：非仓库根运行核对落点
- [x] FIX001.37 [P3] HudBar listen 卸载竞态（A001#35）—— 加 disposed 标志守卫防晚 resolve 泄漏；验证：vue-tsc + HMR 手工启停
- [x] FIX001.38 [P3] 窗口样式 API 返回值未检查（A001#36）—— winembed.rs:56-57 SetWindowLongPtrW 失败报 from_win32；验证：代码审查
- [x] FIX001.39 [P3] 非 BMP UTF-16 截断（A001#37）—— winproc.rs:64-68 统一 `encode_utf16()`；验证：cargo test
- [x] FIX001.40 [P3] HUD stop 最坏阻塞 500ms（A001#38）—— poll_loop 改短步进 sleep 或注明接受；验证：cargo test
- [x] FIX001.41 [P3] capture CONF 无存在性检查（A001#39）—— baseline-capture.sh 启动前 `[ -f "$CONF" ]` 断言；验证：改错路径跑断言报错
- [x] FIX001.42 [P3] 显示 ctx 每帧获取（A001#40）—— GameCanvas.vue:88 首帧缓存 context；验证：vue-tsc
- [x] FIX001.43 [P3] 生产 CSP 为 null（A001#42）—— PL003 攻略渲染落地前设最小 CSP（本条随 PL003 执行，先登记）；验证：攻略渲染落地时核对
- [x] FIX001.44 [P2] FIX001 收尾门禁与状态回写 —— 全量门禁（cargo fmt --check + clippy -D warnings + test --workspace + vue-tsc + npm build）全绿；A001 状态行回写 ✅ 已修复；y.problems #12 闭环核对（随 FIX001.6/测试修复路径处理）；验证：门禁全绿 + z.plan 附录 A001 状态更新

---

### PL003: save 生命周期 + 补丁闭环 [plan#PL003]

> 来源：附录 PL003（2026-09-08 立项，方向定案三条：存档清单显式声明 / 攻略渲染并入 / 开发形态 save 落仓库根）。目标 = `GameName.save` 唯一用户数据端到端闭环（解压 → 运行 → 原子回写）+ 补丁纪律代码化 + 攻略渲染与 CSP 收尾；packer 不在本期。
> **✅ 全组完成（2026-09-08）**：15/15 勾结。全量门禁绿（fmt/clippy/test 54 过/doc 零警告/prettier/vue-tsc/npm build）；反向验收 14 项 + live VA1–VA4 全过，记录见 `.temp/pl003-verification.md`。live 实测修复 2 bug：① CSP 未放行 `ipc:` 二进制通道致帧流全灭（FIX001.43 回归，教训=安全配置必须 live 验证）；② meta.hack 被旧 save 覆盖（UI 选择改为权威源）。

#### 阶段 A：定案与实验先行（格式 + 事实收集）

- [x] PL003.1 存档落盘实验 —— 静态取证（.temp/xeen-work 用户游玩副本文件集）+ live 终验联合定案；验证：清单登记 retro/game.json `paths.save_files` 落盘，两次独立存档命中集合一致（2026-09-08 已验证：save_files = ["*.SAV","XEEN.DAT","XEEN.CUR","DARK.CUR","XEEN.CFG"]；VA1 live 用户存档 → zip 内 6 文件（XEEN01+DARK01.SAV 双面合体同槽双写）→ 读档续玩）
- [x] PL003.2 zip 选型与 save 结构定案 —— 评估 zip crate 版本（2.x/3.x API 与维护状态）锁定依赖；定案 save zip 结构（meta.json + save/ + config/）与 meta.json 字段（version/hack/settings）；验证：单测构造 → 解析最小 save 往返字段一致（2026-09-08 已验证：zip 8.6.0 仅 deflate 特性（cargo add 解析最新稳定）；SaveMeta serde 单一来源 + SaveMeta::new 构造；往返断言在 `save打包解包往返一致`）
- [x] PL003.3 patch 包结构与测试素材 —— 定案 manifest.json（action: replace|add, path, source 含 files/ 前缀）；用 retro/XEEN 现成文件（只读复制构造）做一个真实测试补丁包 + capsule-core 集成测试伪文件夹具；验证：素材落盘、manifest 可解析、retro/ 零写入（2026-09-08 已验证：`retro/patches/test-hack.zip`（构造脚本 .temp/make_test_patch.py）= replace PLAY.BAT + add PATCHED.TXT；选 PLAY.BAT 而非 CMM4.PAT——更小且 replace/add 双动作覆盖；真实包应用并入 VA2 live）
- [x] PL003.4 src/save.rs 解包与打包 —— unpack（meta + save/ 落工作区）、pack（按 save_files 收集 → 临时 zip）；损坏 zip 显式检测（区分损坏与缺失）；验证：集成测试用例——合法/损坏/缺 meta 三场景 FAIL→PASS（2026-09-08 已验证：E0432 FAIL → 15 集成测试 PASS；函数式 API 非 SaveArchive 结构体，KISS）
- [x] PL003.5 save 原子替换与降级 —— replace_atomically（临时文件 → rename 覆盖，失败保旧件）；损坏降级路径（备份 .corrupt + 新建空 save 继续）；AGENTS.md 容错白名单登记该条目（场景/降级/理由三要素）；验证：单测——替换失败保旧件、损坏降级产物正确（备份存在 + 新 save 可用）（2026-09-08 已验证：`原子替换覆盖旧档`/`load_or_recover三态` PASS；白名单已登记含 dosbox.log 轮转失败条目）
- [x] PL003.6 src/patch.rs 补丁应用 —— manifest 解析校验 + apply（replace=覆盖/add=新增到工作副本）；path 越界（zip slip/绝对路径/`..`）严格报错；验证：单测——合法应用/未知 action/越界 path FAIL→PASS（2026-09-08 已验证：7 种越界形态全拒；replace 目标缺失/add 目标已存在/source 缺失/未知 action 各自独立错误；校验顺序 action→目标→source 防错误语义混淆）
- [x] PL003.7 src/paths.rs 释放自愈与生命周期编排 —— release（原版 → 工作副本释放）；编排纯函数 prepare_session（释放 → 补丁 → 解包）与 writeback（收集 → 打包 → 替换）；验证：单测断言对 retro/ 零写入（2026-09-08 已验证：自愈策略定案为"每次启动全量重建工作副本"（残留天然自愈、hack 切换零污染，37MB 本地盘亚秒级），比计划原文的"首启释放+缺失重建"更强；`释放重建清除残留`/`启动前置序列全链路且原版零写入` PASS）
- [x] PL003.8 启动前置接线 —— launch_game 前置序列接入（释放 → 按 hack 补丁 → 解包 save），退役 `.temp/xeen-work` 依赖；验证：cargo test（2026-09-08 已验证：game.json v2 落盘（game_dir + save_files + patches）；**work_dir 字段退役而非改名**——释放目标由 runtime_dir 派生（runtime/work），避免死配置；hack id 不在 patches 中严格报错；冷启动 live 并入 VA1）
- [x] PL003.9 退出回写接线 —— exit-watcher 线程与 stop_game 双挂点走同一 writeback_save()（幂等：按 save_files 收集 → 打包 → 原子替换仓库根 XEEN.save）；.gitignore 登记 \*.save；关键节点落 launcher.log；验证：live VA1——游戏内存档 → 退出 → zip 内含存档文件 → 重进读档继续（2026-09-08 已验证：VA1 全过——用户存档后停止显示"save 已回写"，zip 内 6 文件，重启读档一致）
- [x] PL003.10 补丁纪律 live 验收（VA2）—— hack 版与原版各启动一轮交叉；验证：retro/ 哈希断言零改动、hack/原版互不污染（工作副本内容随切换重建）（2026-09-08 已验证：hack 轮 work 出现 PATCHED.TXT + PLAY.BAT 改写 + meta.hack=test-hack 持久化；原版轮 PATCHED.TXT 随重建清除 + meta 归 null；retro/ 21 文件 MD5 三比对照全等）
- [x] PL003.11 防损坏 live 验收（VA3）—— 手工破坏 XEEN.save 启动；验证：启动按白名单降级不炸（回写中断场景由原子替换单测覆盖：`原子替换覆盖旧档`）（2026-09-08 已验证：垃圾字节写 save → ERROR 日志 + .corrupt 备份 + 游戏正常启动 + 停止生成全新档；真实存档测试后已恢复）
- [x] PL003.12 待机页 hack 切换与 save 状态 —— App.vue 待机态：hack/原版切换（写 meta.hack，启动前生效）+ save 状态显示（"N 个数据文件"计数）；验证：vue-tsc + live 切换后启动会话实际按所选形态释放（2026-09-08 已验证：切换测试补丁启动 → work 实际打上补丁；save 状态行随回写刷新为"补丁 test-hack"/"原版"）
- [x] PL003.13 攻略 Markdown 渲染 + CSP —— 攻略视图（retro/xeen-guide.md 渲染）；选型 marked+DOMPurify（XSS 消毒 + CSP 兜底双层防护）；tauri.conf.json csp 最小白名单（FIX001.43 闭环）；验证：渲染可用、无脚本执行 + npm build（2026-09-08 已验证：用户目测攻略渲染正常；**live 揪出 CSP 回归**——connect-src 须放行 `ipc: http://ipc.localhost` 否则二进制 IPC 全灭，已修复并记录教训）

#### 阶段 C：连带清理与收尾

- [x] PL003.14 连带清理 —— dosbox.log 轮转（problems#13：>5MB 改名 .log.1 仅留一份）、gameconf 测试资产转正（problems#12：定案键内嵌测试）、帧流 640×200 纵横比复核（problems#9：全幅拉伸对 13h 即正确显示几何，非缺陷，y.problems 降级注记）；验证：三项各自验证 + 全量门禁（2026-09-08 已验证：两项闭环一项降级注记；门禁全绿）
- [x] PL003.15 文档回改与收尾 —— AGENTS.md 回改（当前状态/目录规划/白名单两条登记）；全量门禁（fmt/clippy/test/doc + prettier/vue-tsc/npm build）全绿；z.plan 实测结论回写 + x.progress 勾结；验证：门禁全绿 + 文档一致性核对（2026-09-08 已验证：全部完成；遗留移交——.temp/xeen-work 退役待用户批准删除，.temp/xeen-test.conf 已无代码引用）

---

### PL004: capsule-pack 打包器 + 打包形态落地 [plan#PL004]

> 来源：附录 PL004（2026-09-09 立项，六条定案：exe 尾部 zip payload + trailer / save 名从 game.json name 派生 / 产品三文件形态 / 窗口关闭兜底回写纳入 / UI 打磨不做动效（PL005）/ 0.2.0 三端登记 CapsuleRetro_plan.md 不动）。目标 = `GameName.exe`（资源全内嵌）+ `GameName.save` + `readme.md` 三文件产品闭环；0.1.0 = Windows 可玩可发布。
> **✅ 全组完成（2026-09-09）**：12/12 勾结。TDD 新增 14 集成测试全绿，全量门禁绿（fmt/clippy -D warnings/test --workspace/doc 零警告/prettier/npm build）；构建链从零跑通，打包形态 live 全过（帧流渲染 / HUD / 回写 / exe 改名无感 / 关窗兜底 / VA1 干净环境），live 揪出 dosbox shader 资源缺失 1 bug 已修；反向验收 11 项 + live 记录见 `.temp/pl004-verification.md`。

#### 阶段 A：定案与实验先行（格式 + 事实收集）

- [x] PL004.1 trailer 格式与 payload 布局打样定案 —— .temp 打样：手工拼装"exe + zip payload + trailer"并解析回来，定 trailer 字段字节布局（magic / 格式版本 / 偏移 / 长度）与 payload zip 内部布局（game.json / game_dir / dosbox.zip / 攻略 / 补丁）；验证：拼装 → 解析往返一致，定案回写附录 PL004 数据格式小节（2026-09-09 已验证：探针 .temp/pl004_probe.py 对 10.6MB 真实 launcher.exe 拼装 → 解析往返一致，magic/len/payload 三负例行为符合预期；trailer 28 字节（CRPAYLD1 + format u32 LE + len u64 LE + offset u64 LE）与镜像仓库 payload 布局定案回写附录）
- [x] PL004.2 双形态资源来源协议定案 —— launcher 侧判定规则（读自身 exe，trailer 命中 = 打包形态）与资源解析接口抽象（game.json / 游戏文件 / dosbox zip / 攻略 / 补丁五类来源）；save 落点确认（产品根：dev = 仓库根 / packed = exe 同目录，名字从 game.json name 派生）；验证：协议落附录小节，接口签名 cargo check 过（2026-09-09 已验证：协议落附录「双形态协议」小节；payload 布局镜像仓库使两形态仅"资源根"不同——实现 `resolve_resource_ctx` 返回 ResourceCtx，下游逻辑零分叉；workspace clippy/check 全绿）
- [x] PL004.3 readme 模板定案 —— 模板落盘（使用方法 / 两个数据文件角色 / 同目录要求 / 改名规则表 / SmartScreen 指引），占位符（游戏名 / 版本）注入方式定案；验证：模板 prettier 过，packer 输入清单含模板路径（2026-09-09 已验证：**文案用户审阅通过**；模板 include_str! 编译期内嵌（比运行期输入清单更强），占位符 {{GAME_NAME}} / {{VERSION}} 出包注入实测生效，prettier 过）

#### 阶段 B：capsule-core payload 模块（TDD）

- [x] PL004.4 src/payload.rs 拼装与解析 —— append_payload（exe + zip + trailer）与 open（读 exe → 定位 trailer → payload zip 只读读取）；缺失 / 非法 trailer 显式错误；验证：集成测试——往返一致 / 无 trailer 报错 / trailer 损坏报错 FAIL→PASS（2026-09-09 已验证：E0432 FAIL → 7 集成测试 PASS——往返一致 / 无 trailer None / format 与 len 篡改 BadTrailer / 重复拼装 AlreadyPacked；payload 区间用区间游标 Read+Seek，不整段载入内存）
- [x] PL004.5 payload 资源枚举与 extract-once 释放 —— 按协议枚举 payload 资源 + dosbox/game 释放到 runtime 目录（版本标记，已释放跳过，标记损坏或缺失重释放）；验证：集成测试——释放产物齐全 / 二次调用跳过 / 标记损坏重释放（2026-09-09 已验证：三态 + 越界条目 UnsafeEntry 全过；live 复现——第二次打包新 build_id 后启动自动重释放（launcher.log 21:25:19），释放目标 = runtime/<game>/resources/ 与 work/ 隔离）

#### 阶段 C：capsule-pack CLI 本体

- [x] PL004.6 crate 骨架与打包前校验 —— workspace members 加 capsule-pack；clap CLI（game.json 路径 + `--launcher <exe>` + `-o dist/`）；打包前校验（game_dir / patches / 攻略 / dosbox zip / readme 模板全部存在，缺一严格报错）；验证：缺失资源逐一报错用例 + 合法样例通过（2026-09-09 已验证：E0432 FAIL → 7 集成测试 PASS——缺游戏目录/攻略/补丁/dosbox/launcher 五用例逐一报错 + 已打包 launcher 拒绝再打包；readme 模板升级为 include_str! 编译期内嵌，无运行期路径可失）
- [x] PL004.7 产物拼装 —— 资源 zip 构造 → 复制 launcher exe → append_payload → dist/{name}.exe + readme.md（模板注入游戏名 / 版本）；验证：产物回读资源清单与输入一致 + 体积 40-60MB 量级合理（2026-09-09 已验证：产物回读条目/内嵌 game.json 真实解析一致；实测 dist/XEEN.exe = 37.6MB（413 条目，deflate 后低于预估——游戏与 dosbox 压缩率高，更小更好））

#### 阶段 D：launcher 打包形态改造

- [x] PL004.8 双形态资源接入 —— 启动读自身 exe 识别形态；打包形态：game.json / 攻略 / 补丁从 payload 读、dosbox/game 走 extract-once 释放；dev 形态行为不变；验证：集成测试双形态识别 + live get_game_info 双形态表现一致（2026-09-09 已验证：dev 单测（资源根=仓库根）+ live 打包形态——双击 dist/XEEN.exe 待机页（游戏名/补丁/存档状态）全部来自 payload 与 exe 同目录，会话启动帧流渲染 + HUD 全量正常；live 揪出 1 bug：dosbox shader 资源在构建树兄弟 Resources/ 目录，payload 布局改镜像构建树（dosbox/bin + dosbox/Resources）后修复）
- [x] PL004.9 save 落点定案落地 —— 打包形态 save = exe 同目录/{name}.save（name 从 game.json 派生，exe 改名无感）；dev 形态维持仓库根；验证：live 改 exe 名 → 启动 → 回写后 save 名不变（2026-09-09 已验证：live 复制 dist/XEEN.exe → 魔法门4.exe，待机页正确显示 dist/XEEN.save 状态（原版 · v1 · 4 文件），启动/停止回写仍落 XEEN.save、无 魔法门4.save 产生；测试副本已清理）
- [x] PL004.10 窗口关闭兜底回写 —— onCloseRequested 拦截：会话存活 → writeback → 退出；被杀 / 崩溃路径由 exit-watcher + Job Object 现状兜底；验证：live 游戏中直接关窗口 → 重进存档在 + 日志含回写记录（2026-09-09 已验证：代码（lib.rs on_window_event CloseRequested → commands::writeback_on_close：停会话 + 幂等 writeback_save 后放行，失败落日志不阻断关闭）；**live 用户通过——游戏内存档后直接关窗口，重进存档在**）

#### 阶段 E：连带收尾

- [x] PL004.11 构建链与文档回改 —— 构建链跑通并落文档（npm tauri build → capsule-pack → dist 三文件）；AGENTS.md（三文件架构 / capsule-pack 状态 / workspace members）；z.plan 实测结论回写 + x.progress 勾结；验证：从零跑完整构建链 + 门禁全绿 + 文档一致性（2026-09-09 已验证：构建链 npm build → cargo build --release -p launcher → capsule-pack → dist/XEEN.exe + readme.md 从零跑通；AGENTS.md（members/payload.rs/capsule-pack 行/启动命令）与 z.plan 状态行已回改；z.plan 实测结论待 live VA 收口后一并回写；门禁 fmt/clippy -D warnings/test 全 workspace/doc 零警告/npm build 全绿）
- [x] PL004.12 发布形态 live 验收（VA1/VA2）—— VA1：dist 产物拷到无 repo / 无工具链目录全链路（双击 → 玩 → 存档 → 回写 → 重进续档）；VA2：直接关窗口存档不丢 + exe 改名无感；验证：live 全链路 + 证据落 .temp（2026-09-09 已验证：**用户 live 全过**——VA1 干净环境全链路；VA2 关窗口存档不丢（随 PL004.10）+ exe 改名无感（随 PL004.9）；反向验收 11 项 + live 冒烟记录见 .temp/pl004-verification.md）

---

### PL005: 会话体验硬化——静默启动 + 帧流 WS 直推 [plan#PL005]

> 来源：附录 PL005（2026-09-09 立项；CapsuleRetro_plan.md §10 阶梯②）。目标量化 = 输入注入往返 <5ms、显示帧龄 <17ms、启动无 dosbox 窗口闪现；先打点基线再改；回退锚点 V0.1.0.6。
> **✅ 全组完成（2026-09-09）**：9/9 勾结。显示链延迟基线 42–53ms → 帧龄 p50 10–12ms / p95 19–29ms（降约 60–75%）；静默启动零窗口闪现（hidden + CREATE_NO_WINDOW）；winembed / get_frame / HTTP 轮询端点三链退役；FB1 基线 92 条目零偏差；cycles 5000→6500 实测定案；用户判收"过"（遗留：鼠标跟手残余差距 → 阶梯③动机）。门禁全绿。

#### 阶段 A：测量基线与探针先行

- [x] PL005.1 延迟基线打点 —— GameCanvas 注入调用前后 `performance.now()` 打点测输入 RTT（含 Tauri IPC + HTTP 注入全链，≥100 次按键采样取 p50/p95）；取帧调用前后打点测取帧 RTT 并统计轮询实际间隔分布；帧龄精确值待帧头时间戳落地（PL005.4）后同口径补测，基线期以"30fps 理论陈旧度 + 取帧 RTT"作对比口径；验证：基线表落 `.temp/pl005-baseline.md`（p50/p95 两级），缺表不动工（2026-09-09 已验证：取帧 RTT p50 9.1ms / p95 20ms / max 33.6ms（256×4 轮）；注入 fork 侧直连 p50 2.11ms / p95 2.86ms；真实 JS→IPC 注入路径因宿主环境抢占 CUA 前台未采——该路径改前改后不变，判收会话埋点自动补采）
- [x] PL005.2 SDL hidden 抓帧探针 —— DosboxCapsule 打样分支：`SDL_CreateWindow` 带 SDL_WINDOW_HIDDEN（或创建后 `SDL_HideWindow`），照常起 XEEN + webserver，与可见模式并行取帧对比；验证：hidden 下连续取帧 ≥60 秒无停帧、帧内容与可见时一致（截图逐帧比对）——结论登记本条目，不可行则阶梯②降级重评（2026-09-09 已验证：hidden 下会话帧流正常、画面持续推进开场动画（截图证据），SDL_WINDOW_HIDDEN 不影响渲染与抓帧——阶梯②成立）

#### 阶段 B：fork 改造（DosboxCapsule 仓：PATCHES.md 登记 + rebuild.bat 清洁重建 + baseline-compare 回归）

- [x] PL005.3 窗口创建即隐藏 —— fork 正式补丁：`webserver_enabled`（streaming）模式下 SDL 窗口以 SDL_WINDOW_HIDDEN 创建，env `CAPSULERETRO_SHOW_DOSBOX=1` 时保持可见（launcher spawn 时按需注入）；验证：dev 会话画面正常出帧 + 屏幕与任务栏无 dosbox 窗口 + env 开关可找回原生窗口 + baseline-compare 通过（FB 基线不受影响）（2026-09-09 已验证：单点补丁在 `get_sdl_window_flags()`（sdl_gui.cpp），两渲染器通吃；dev/packed 启动全程零 dosbox 窗口闪现；**FB1 基线判定 PASS——92 条目全静态零偏差**（官方 v0.83.0 vs 新候选，.temp/fb1-report.json）；补丁不触内存路径与判定一致）
- [x] PL005.4 /ws/frame 二进制推送端点 —— fork webserver 新增 WebSocket 端点：连接建立后每帧推送（帧号 / 宽高 / pitch / 捕获时间戳头），替代 HTTP 轮询端点；验证：WS 客户端收帧连续 + 帧头解析往返一致（2026-09-09 已验证：**机制微调——WS → HTTP chunked 流式直推**（vendored cpp-httplib 0.32 无 WS 支持，自研握手/封帧协议风险高且推送语义与延迟特征与 WS 一致）；帧头 20→28 字节（补捕获时间戳）、CapsuleFrame 增 WaitSnapshot 条件变量等待；stream 端点 200 + chunked + CORS ✓、4.1s 收 25 完整帧、头解析往返一致、服务端→客户端传输帧龄 p50 2ms）

#### 阶段 C：launcher 接入与退役

- [x] PL005.5 前端 WS 消费 —— GameCanvas 改 `new WebSocket("ws://127.0.0.1:18086/ws/frame")` 二进制消费（28 字节头解析 + rAF 回调绘制 + 帧龄统计），断线重连、会话结束关闭；tauri.conf.json CSP connect-src 加 ws://127.0.0.1:18086；验证：vue-tsc + npm build 过 + live 会话帧流 ≥5 分钟帧号连续 + 控制台无 CSP 拦截报错（CSP 变更 live 全链路验证，PL003 教训）（2026-09-09 已验证：GameCanvas 改 **fetch ReadableStream** 消费 chunked 流（读线程挑最新完整帧、rAF 绘制解耦、陈旧帧丢弃、断线重试超限置错）；CSP connect-src 放行 `http://127.0.0.1:18086`（锁死端口）；live 会话画面持续推进动画、帧号连续、无 CSP 拦截）
- [x] PL005.6 winembed 与 get_frame 退役 —— 删 `winembed.rs` + launch_game 托管重试线程 + `commands::get_frame` 与 `Session::get_frame`（invoke_handler 同步移除）；fork 侧 HTTP 帧端点随 WS 稳定退役；AGENTS.md 白名单"窗口离屏托管退化形态"条目注记退役（该错误路径随隐藏窗口不复存在）；验证：cargo clippy -D warnings + cargo test --workspace 全绿 + live 会话全链路（启动 / 画面 / HUD / 停止回写）无回归（2026-09-09 已验证：三链全退（winembed 模块 / 托管线程 / get_frame + fork 轮询端点）；clippy 0 error + workspace 测试全过 + live 回归全链无恙（停止回写 4 文件））
- [x] PL005.7 输入 RTT 对比 —— 复用 PL005.1 打点埋点，WS 落地后同口径采样 ≥100 次（输入链路本身未动：确认无回归 + 帧龄改善对整体手感的贡献单列）；验证：基线 vs 改后对比表（p50/p95）落 .temp + <5ms 判定结论（超标则登记"并入 WS 上行"增量）（2026-09-09 已验证：**显示链延迟 42–53ms → 帧龄 p50 10–12ms / p95 19–29ms（dev + packed 双测，256 样本/轮），降约 60–75%**；注入链路按定案未动——fork 侧 2.11ms 参考不变，真实路径 <5ms 复核随判收会话埋点自动产出）
- [x] PL005.8 VA1/VA2 量化与静默验收 —— VA1 测量对比表达标（帧龄 <17ms）；VA2 dev + packed 启动无 dosbox 窗口闪现；验证：数据 + 证据落 .temp（2026-09-09 已验证：**dev frame_age p50 12ms / p95 27–29ms；packed p50 10ms / p95 19ms**（各 256 样本）——中位数与 packed p95 达 <17ms 目标，dev p95 受开场动画高频帧影响；双形态启动全程无 dosbox 窗口；证据 = launcher.log PERF 行 + 本组截图）
- [x] PL005.9 用户实玩判收 + 文档收尾 —— 用户实玩 XEEN 判收（鼠标 / 键盘 / 流畅度三项主观结论）；全量门禁（fmt / clippy -D warnings / test / doc / prettier / npm build）；z.plan 实测结论回写 + x.progress 勾结 + 计划书 §10 阶梯②状态更新；验证：VA3 三项用户结论 + 门禁绿 + 文档一致性核对（2026-09-09 已验证：**用户判收"过"**——键盘差距缩小但可感、鼠标跟手有差距但可接受、流畅度提升可继续开发；**cycles 5000→6500 实测定案**（用户感知有效，写入 game.json）；注入真实路径实测 p50 9.1ms / p95 16.4ms（>5ms 目标 → 登记"输入并入上行通道"为后续增量，与阶梯③动机合并）；门禁全绿 fmt/doc 零警告/npm build；实测结论已回写 z.plan）

---

### FIX002: 第2轮审计修复 [audit#A002]（✅ 2026-09-10 收口，39/39；单测 43+9+24 全绿、live 探针全过，证据与连带修复见 `.temp/fix002-verification.md`）

#### P1（确定性复现）

- [x] FIX002.1 [P1] 端口回退与锁死 18086 契约矛盾 —— 做法：删除 `probe_port`（commands.rs:207-218）的 +1 回退——18086 `TcpListener::bind` 失败时直接返回携带原因的错误（"端口 18086 被其他程序占用，请释放后重试"），GameCanvas.vue:9 硬编码与 tauri.conf.json:19 CSP 的 18086 保持不动（锁死契约贯彻）；验证：预占 18086（临时监听脚本）后点启动 → launch 显式报错且消息含占用原因；释放后正常启动 → 帧流/HUD/注入 live 全链路无回归（2026-09-10 已验证：单测端口探测锁死无回退过 + live 占位 18086 报完整占用消息、释放后恢复；连带发现瞬时 WSAEADDRINUSE → probe_port 加短重试，不回退端口）
- [x] FIX002.2 [P1] 多实例零防护互毁工作副本与存档 —— 做法：launcher 引入 `tauri-plugin-single-instance`（src-tauri/Cargo.toml + lib.rs Builder 初始化，回调聚焦既有窗口），二次启动直接退出、不进 start_session 前置序列（双实例对 work_dir `remove_dir_all` 与 save 回写竞速的根因消除）；验证：打包 exe 双开 → 第二实例立即退出且日志无第二次 `prepare_session`；A 实例存档退出后再退 B → save 内容为 A 的存档（互不覆盖）（2026-09-10 已验证：dist 双开 live——实例2 PID 6732 秒退、实例1 PID 19740 存活）
- [x] FIX002.3 [P2] save 解包拒绝良性目录/未知前缀条目致启动永久阻断 —— 做法：save.rs unpack 循环（:128-156）加 `entry.is_dir()` 跳过（对齐 payload.rs extract_all 同类循环）；非 `save/` 前缀的非目录条目（config/ 预留节、`__MACOSX/` 噪声）改为忽略 + WARN 日志，`validate_rel` 穿越拒绝维持严格；验证：新增测试——含 meta.json + 目录条目 `save/` + `config/x` + `save/XEEN01.SAV` 的 zip 解包成功且存档落位；`../evil` 条目仍报 UnsafeEntry；手工用 7-Zip 重打包 XEEN.save 后启动正常（2026-09-10 已验证：目录条目与未知前缀良性忽略 + 越界条目仍 UnsafeEntry，7-Zip 手工重打包场景以合成 zip 等价覆盖）
- [x] FIX002.4 [P2] payload extract_once 静默容错未登记 —— 做法：AGENTS.md 容错白名单新增条目（场景：extract_once 读 payload.json 标记失败（被锁/权限/半写）；降级：清空重释放；理由：重释放幂等、自愈优于阻断），payload.rs:330-335 补注释指向该登记；验证：白名单条目三要素齐备且降级路径与登记描述一致（代码审查）（2026-09-10 已验证：白名单三要素登记 + marker_fresh 注释在位）
- [x] FIX002.5 [P2] 容错白名单 GameCanvas 条目机制过时 —— 做法：AGENTS.md 白名单"GameCanvas 帧轮询瞬时失败"条目整体改写为现行机制（chunked 流单次中断/解析失败 → 1s 退避重连；连续 5 次 → 显式置错停循环），条目标题同步；验证：条目描述与 GameCanvas.vue streamLoop 实现（MAX_STREAM_FAILURES/退避常量）逐点一致（2026-09-10 已验证：白名单条目改写为 chunked 流现行语义，与 streamLoop 实现一致）
- [x] FIX002.6 [P3] CpuState.registers 缺文档（A001 #30 漏改）—— 做法：rest.rs:91-92 `pub registers` 补 `///` 中文文档注释；验证：cargo doc --no-deps 无警告 + clippy -D warnings 过
- [x] FIX002.7 [P3] hud_test fn 内 use（A001 #33 漏网）—— 做法：launcher_hud_test.rs:74-75、120-121 两处 fn 内 use 上移至文件头 use 区；验证：clippy -D warnings 过 + cargo test --workspace 全绿
- [x] FIX002.8 [P3] trailer 畸形值加法溢出 panic —— 做法：payload.rs:217-225 第二段加法并入 checked_add 链（`checked_add(payload_len).and_then(|sum| sum.checked_add(TRAILER_SIZE))`）报 BadTrailer；验证：新增测试——offset/payload_len 构造 u64::MAX 附近值 → 解析返回 BadTrailer 而非 panic（debug 构建下断言）
- [x] FIX002.9 [P3] payload 部分丢失不可自愈 + "可自愈"文档失实 —— 做法：extract_once 命中标记后抽查关键条目存在性（dosbox/bin/<exe> 与 retro/game.json），缺失即清空重释放；AGENTS.md 运行时目录条目措辞收敛为与行为一致；验证：live——删 resources/dosbox/bin/dosbox.exe 后启动 → 自动重释放且会话正常；文档措辞与实际行为一致
- [x] FIX002.10 [P3] extract_all 丢弃 unix 权限（0.2.0 前置）—— 做法：payload.rs extract_all 循环读 `entry.unix_mode()`，非 0 时在 `#[cfg(unix)]` 分支 set_permissions（Windows 侧 cfg 隔离为 no-op）；验证：Windows 侧 clippy/test 过 + y.problems 登记为 0.2.0 三端适配前置项（实机验证随三端）
- [x] FIX002.11 [P3] append_payload 整读内存 —— 做法：payload.rs:81-84 改 `io::copy(&mut File::open(zip)?, &mut out)` 流式追加，payload_len 取复制字节数；验证：现有 payload 往返/extract 测试全过 + capsule-pack 打包 dist 冒烟（产物可启动）
- [x] FIX002.12 [P3] 截断档降级残留部分解包内容 —— 做法：save.rs load_or_recover 降级分支在 `.corrupt` 备份后对 work_dir 执行清空再返回全新 meta（对齐 prepare_session 全量重建语义）；验证：新增测试——前半有效后半截断的 zip → 走降级且 work_dir 无前半条目残留
- [x] FIX002.13 [P3] SaveMeta 版本字面量无单一来源 —— 做法：capsule-core save.rs 增 `pub const SAVE_META_VERSION: &str = "1"`，commands.rs:312 改引用常量；验证：grep 全仓 SaveMeta::new 构造点仅剩常量引用一处 + 测试过
- [x] FIX002.14 [P3] patch source 读取失败混淆 MissingSource —— 做法：patch.rs read_zip_file 区分：archive.by_name 查无 → None（MissingSource）；条目读取/解压 Err → 映射 PatchError::Corrupted（消息带 source 名）；验证：新增测试——manifest 指向不存在条目 → MissingSource；条目存在但数据损坏 → Corrupted
- [x] FIX002.15 [P3] game_dir 为空误报 InvalidPatches —— 做法：game_config.rs:159-164 新增 `GameConfigError::InvalidGameDir` 变体（文案"paths.game_dir 为空"）替换现有变体；验证：新增测试——game_dir 空串 → 错误变体为 InvalidGameDir
- [x] FIX002.16 [P3] capsule-core dev-dependencies zip 重复声明 —— 做法：删除 capsule-core/Cargo.toml:14-15 dev-deps 的 zip（集成测试直接用 [dependencies] 的 zip）；验证：cargo test --workspace 过（集成测试编译链接正常）
- [x] FIX002.17 [P3] xmonitor stdin/stdout 吞错 —— 做法：xmonitor.rs:216-217、271、273 三处 `.ok()` 改显式处理：读取/刷新失败 eprintln 提示后按"用户拒绝/EOF"处理并走正常退出路径；验证：`xmonitor search < /dev/null`（EOF 管道）→ 输出含 EOF 提示、退出码明确，无静默跳过
- [x] FIX002.18 [P3] launch 竞态败者 stop() 吞错未登记 —— 做法：commands.rs:525 `let _ = session.stop()` 改 `if let Err(e) = session.stop() { logging::error(...) }`；验证：代码审查确认日志路径在位 + clippy 过（该路径无可达测试）
- [x] FIX002.19 [P3] WindowsAndMessaging feature 死配置 —— 做法：删除 launcher/src-tauri/Cargo.toml:22 该 feature（winembed 退役残留）；验证：cargo build + clippy -D warnings + cargo test --workspace 全过（无缺符号编译失败）
- [x] FIX002.20 [P3] 注释引用已退役机制 —— 做法：commands.rs:532 注释改为实际理由（HTTP 注入等待不占主线程）；commands.rs:604 PerfStats.kind 注释删除 frame_fetch_rtt、只列现行三类（inject_rtt/frame_age/parse_draw）；验证：grep 全仓 get_frame/frame_fetch_rtt 零残留（.mimosa 与文档历史记录除外）
- [x] FIX002.21 [P3] NO_SESSION 死契约 —— 做法：删除 commands.rs:56-57 常量与 :13-14/:542 相关表述，inject_input 无会话错误消息改直白文案（"当前无游戏会话"）；验证：grep NO_SESSION 全仓零命中 + clippy/test 过 + live 无会话时注入返回错误消息正常
- [x] FIX002.22 [P3] 会话槽锁跨长 IO —— 做法：三处（inject_input :538-544 / stop_game :555-575 / writeback_on_close :584-597）补注释说明锁跨越边界与有界时长（KISS：先注释不改结构；先 take 再操作列为可选优化，如做须 live 回归）；验证：注释在位 + live 停止/关窗/注入三路径行为不变
- [x] FIX002.23 [P3] exit-watcher 锁窗口 + 陈旧 session-ended 拉回待机 —— 做法：commands.rs:447-458 is_same 检查与 take 合并为一次持锁；SessionState 增会话序号 u64，session-ended 事件载荷改 `{pid, seq}`，App.vue:36-40 比对当前序号一致才回待机；验证：live——A 会话崩溃后立即启动 B，A 的陈旧事件到达时 B 的画面与状态栏不回待机；cargo test 过
- [x] FIX002.24 [P3] launch 错误链口径不一 —— 做法：commands.rs:493 `map_err(|e| e.to_string())` 改 `format!("{e:#}")` 保留 anyhow 全链；验证：人为制造前置失败（临时改坏 game.json）→ 启动错误消息含底层 IO 原因
- [x] FIX002.25 [P3] guest 游戏 exe 名无配置字段 —— 做法：game.json paths 增可选 `guest_exe` 字段（GameConfig serde + 缺省回落 name 小写派生的现行为），gameconf.rs:58-59 与 commands.rs:340-346 改读该字段；retro/game.json 不动（XEEN 恰好成立）；验证：新增测试——guest_exe 显式配置时 conf 启动命令与完整性断言用其值；缺省时回落现行派生
- [x] FIX002.26 [P3] launcher.log 无轮转 + 注释失实 —— 做法：logging.rs:3-4 模块注释回改为如实的"无轮转"取舍声明（KISS：launcher.log 量级低），y.problems 登记远期轮转项；验证：注释与行为一致 + grep 无"届时 PL003"失实表述 + y.problems 新条目在位
- [x] FIX002.27 [P3] 测试 expect 消息不插值 —— 做法：launcher_session_test.rs:50-53 改 `unwrap_or_else(|e| panic!("第 {round} 轮启动应成功: {e}"))` 形态；验证：cargo test 全绿（消息插值以代码审查断言）
- [x] FIX002.28 [P3] 流累积缓冲无上界 —— 做法：GameCanvas.vue streamLoop 中 acc.byteLength 超上限常量（如 8MB，单帧合法最大值的数倍）即抛错进入既有失败计数路径；验证：vue-tsc + npm build 过 + 上限常量与判断分支在位（代码审查），live 长会话内存平稳
- [x] FIX002.29 [P3] 鼠标合帧残差无 flush —— 做法：GameCanvas.vue 合帧逻辑补 `setTimeout(MOUSE_BATCH_MS)` 定时器兜底：到期 flush pending 累积并清零（覆盖"移动后停手"场景）；验证：live——画布内移动鼠标急停 → 游戏内光标停在手指位置（目测）；vue-tsc/build 过
- [x] FIX002.30 [P3] "save 已回写"假成功文案 —— 做法：session-ended 事件载荷改 `{pid, writeback_ok}`（exit-watcher 回写失败置 false，commands.rs:465-474），App.vue 文案按载荷分支（成功"存档已保存" / 失败"存档回写失败，详见日志"）；验证：live——正常停止显示成功文案；临时锁住 save 文件制造回写失败 → 显示失败文案且 launcher.log 有 ERROR
- [x] FIX002.31 [P3] onMounted 死导入 —— 做法：App.vue:6 删除未使用导入；验证：vue-tsc --noEmit 过 + grep 零残留
- [x] FIX002.32 [P3] NumpadEnter map 死条目残留 —— 做法：GameCanvas.vue:100 删除该行（:87 特判先行永不可达）；验证：vue-tsc/build 过 + live 小键盘映射回归（Enter 仍映射 return）
- [x] FIX002.33 [P3] session-ended 后待机页数据陈旧 —— 做法：App.vue session-ended 处理器回待机后 `void loadLobby()`（事件由 exit-watcher 回写完成后发出，时序安全）；验证：live——首次会话存档退出 → 待机页显示最新 save 状态而非"尚无存档"
- [x] FIX002.34 [P3] GameConfig 双重加载 —— 做法：capsule-pack main.rs 解析得的 config 经 PackInputs 传引用进 build_package（签名增加量），lib.rs:51 二次 load 删除；验证：cargo test -p capsule-pack 过 + 打包 dist 冒烟成功
- [x] FIX002.35 [P3] readme 模板与实现失配 —— 做法：readme-template.md:18 按钮指引改"启动 {{GAME_NAME}}"；:40 运行时目录改注入小写值（packer 生成 readme 时对目录语境用 name.to_ascii_lowercase()，如新增 {{GAME_DIR}} 占位符）；验证：打包 dist → readme 文案与实际按钮一致、目录示例为小写 xeen
- [x] FIX002.36 [P3] pack_test 覆盖缺口（zip slip 写侧 + Resources 分支）—— 做法：补两用例：①game.json 字段含 `../` → build_package 报错（ensure_safe_rel 拒绝路径）；②fixture dosbox 目录加 Resources/ 子目录 → 产物 payload 含 `dosbox/Resources/**` 条目；验证：cargo test -p capsule-pack 两新用例过
- [x] FIX002.37 [P3] save 根不可写时明确报错（原观察项 C1，部署形态）—— 做法：start_session 前置序列（commands.rs，prepare_session 前）对 save_root 做可写探针（create_dir_all + 临时文件创建后删除），失败时返回明确错误（"游戏目录不可写（如 Program Files），请将 exe 移到普通可写目录"）；readme-template.md 增部署位置指引一段（勿放只读/需管理员目录）；验证：新增测试——探针函数传入"现有文件路径"（充当不可写目录）→ 返回错误；正常目录 → 通过；live 把 dist 拷入受限目录启动 → 报错文案明确
- [x] FIX002.38 [P3] packer 对 dosbox Resources 缺失静默跳过加 WARN（原观察项 C2）—— 做法：capsule-pack lib.rs 收集分支（Resources 目录 `filter(is_dir)` 处）在目录不存在时输出 WARN（"dosbox Resources 目录缺失，shader 等运行期资源将不随包，请确认构建树完整"），不阻断打包；验证：cargo test -p capsule-pack 新用例——fixture 无 Resources 目录 → 打包成功且告警出现；有 Resources → 无告警
- [x] FIX002.39 [P3] 验证收尾 —— 做法：全量门禁（cargo fmt --check + clippy -D warnings + cargo test --workspace + npm run build）+ 打包 dist 冒烟（产物启动/存档闭环）+ z.plan 附录 A002 状态改 ✅ + x.progress 本组勾结上移已完成区；验证：门禁全绿 + live 冒烟记录落 .temp（2026-09-10 已验证：fmt/clippy/test/doc + vue-tsc + npm build 全绿；dist 出包 413 条目零告警；live 探针全过——EOF 退出码 1、单实例双开、自愈抽查恢复 dosbox.exe、串行会话冒烟 2/2；连带修复 remove_dir_all 热会话 error 5 与 probe_port 10048 瞬时窗口，详见 .temp/fix002-verification.md）

---

### PL006: UI 第一遍打磨——黑金典藏版落地 [plan#PL006]（✅ 2026-09-10 收口，9/9；v3 稿 live 目测对照 + 反向验证全过，证据见 `.temp/pl006-verification.md`）

#### 阶段 A：资产与配置

- [x] PL006.1 assets/design 入库与定案标注 —— 做法：五稿 + README.md 定案说明（v3 黑金典藏版为 PL006 视觉定案，其余四版留作 theme 迭代参考）已自 .temp 迁入 assets/design/；验证：git 跟踪生效 + v3 稿浏览器打开真图（../../retro/artwork/ 相对引用）正常加载（✅ 2026-09-10：五稿 + README 在位、相对引用两图实存，随 PL 完成一并提交）
- [x] PL006.2 game.json 封面字段位与随包 —— 做法：retro/game.json assets 增 `"cover": "retro/artwork/xeen2x3.jpg"`；game_config.rs AssetsSection 增 `cover: Option<String>`；capsule-pack lib.rs build_package 对 cover 走 ensure_safe_rel + require_file 校验但缺失时仅 warnings 告警不阻断，命中时收集为 payload 条目；验证：pack_test 新用例——有 cover 打包 payload 含封面条目、无 cover 打包成功且 warnings 非空（✅ 2026-09-10：实作为 `Option<PathBuf>` 并同式附带 `hero` 可选主视觉（方向定案 3 随包）；TDD 先红后绿 + 真实打包 415 条目无告警；缺文件/越界仍严格报错）
- [x] PL006.3 hero 图压缩 —— 做法：.temp 脚本（PowerShell System.Drawing）把 xeed16x9.jpeg 2848×1600/7.7MB 压到 1920×1080 JPEG q≈80 覆盖回 retro/artwork/；验证：产物 ≤500KB 且 1920×1080、目测无可见劣化（✅ 2026-09-10：479KB/1920×1080 双断言过 + 目检无劣化；原件备份 `.temp/xeed16x9-original-backup.jpeg`）
- [x] PL006.4 封面路径下发与回落 —— 做法：commands.rs GameInfo 增 `cover: Option<String>`（get_game_info_impl 把 config.assets.cover 解析为资源根绝对路径），App.vue 待机页 img 绑定、None 时回落占位；验证：vue-tsc/npm build 过 + 临时删 cover 配置启动 → 待机页占位显示不报错（验后还原）（✅ 2026-09-10：实为内联 data URL 下发——复用 CSP img-src data:，不引入 asset protocol 暴露面；roundtrip 单测 + 删配置回落占位探针过，配置已还原）

#### 阶段 B：UI 第一遍（v3 施工）

- [x] PL006.5 待机页按 v3 稿重构 —— 做法：App.vue template/style 照 assets/design/v3 稿施工——品牌条（CAPSULERETRO 典藏系列 + HUD/版本 chip）、hero 区（16:9 keyart 渐变压暗 + 金框卡带封面 + 烫金渐变标题 + meta chips + 存档行）、形态切换与按钮行（金描边启动 / ghost 停止攻略）；色板收敛 :root 变量（--gold/--panel/--text-dim 系）；验证：与 v3 稿并排目测结构层级一致 + vue-tsc/npm build 过（✅ 2026-09-10：live 截图对照全要素在位；连带修复顶栏 flex auto-margin 收缩包裹→与 .stage 同宽对齐）
- [x] PL006.6 会话顶栏与画布容器 —— 做法：会话态顶栏（品牌 + HudBar + 停止按钮金边样式）与 GameCanvas 画布容器按 v3 稿改金边深底样式；验证：live 会话目测 + build 过（✅ 2026-09-10：live 启动/停止全通——金边画布直推 XEEN 画面、停止回待机且存档行刷新；连带修 `.btn` nowrap 防按钮文字换行）
- [x] PL006.7 攻略面板与 HudBar 对齐 —— 做法：HudBar.vue 数值条与攻略 overlay（guide-overlay/panel/body）按 v3 色板（深底金描边）对齐；验证：攻略开合目测 + build 过（✅ 2026-09-10：overlay 金标题/深面板/pre 深底目测过，HudBar 金值单元格会话同场验证）

#### 阶段 C：图标与产品名

- [x] PL006.8 exe 图标与产品名 —— 做法：金框卡带母题 PNG（1024²，可由封面合成）经 `npx tauri icon` 生成 launcher icons 全尺寸；tauri.conf.json 复核 productName/windowTitle；App.vue onMounted 设 `document.title = "CapsuleRetro · " + gameName`；验证：dist exe 文件图标显示、任务栏名与窗口标题正确（✅ 2026-09-10：System.Drawing 合成卡带源图 + 全尺寸 icons；debug/dist exe 图标探针均卡带；窗口标题 `CapsuleRetro · XEEN` a11y 实证；陷阱——icons 变更后 dev/release profile 各需 clean 才重嵌入）

#### 阶段 D：验收收尾

- [x] PL006.9 验收收尾 —— 做法：launcher 待机页/会话顶栏/攻略面板与 assets/design/v3 稿并排对照目测判收（VA1）；无 cover 回落占位与有 cover 显示双向验证（VA2）；图标/任务栏名/窗口标题正确（VA3）；全量门禁（fmt/clippy/test/doc + vue-tsc/npm build）绿；z.plan 实测结论回写 + 本组勾结上移；验证：VA1–VA3 全过 + 门禁绿（✅ 2026-09-10：VA1/VA2/VA3 全过 + dist 端到端首启（payload 含封面、释放目录两图在位）；门禁全绿——单测 59 + live 探针 3 串行全过；VA1 用户目测判收通过（2026-09-10））

### PL007: 会话内数值修改器——金手指第一歩 [plan#PL007]（✅ 2026-09-11 收口，4/4；live 双按钮闭环 + 反向验证全过，证据见 `.temp/pl007-verification.md`）

#### 阶段 A：上游写端点验证（fork 零改动）

- [x] PL007.1 上游写端点 live 探针 —— 做法：~~fork 新增写补丁~~（2026-09-11 实况修正：vanilla v0.83.0 webserver 已内置 `PUT /api/v1/memory/{segment}/{offset}`——octet-stream 原始字节 / JSON+base64 双形态、If-Match 乐观并发 412、越界 400，fork 补丁取消）；live 会话起后 curl 探针：合法地址写 → GET 读回一致、If-Match 不匹配 → 412、越界地址 → 400；验证：三探针全过（ DosboxCapsule 仓零改动）（✅ 2026-09-11：写读回一致（50c30000 @ 0x5AA3E）/ 412 附当前值 / 现读现写命中 200 / 越界 400 带明确错误；HUD 实证金币 50000）

#### 阶段 B：monitor 写 API + 白名单

- [x] PL007.2 monitor write_memory + adjust_field + 白名单 —— 做法：rest.rs 增 `write_memory(addr, bytes)`（POST，MonitorError 增写失败变体）；memory_map 条目区间白名单（写入区间须完整落某登记条目内，否则 `WriteDenied`）；`adjust_field(name, delta)` = GET → 类型感知饱和加法（u32le/i16le 封顶）→ 校验 → 写回；验证：单测先行红→绿——白名单拒绝（未登记/跨界地址）、u32 饱和封顶不回绕、条目边界命中判定（纯逻辑，无需 dosbox）（✅ 2026-09-11：3 单测红→绿全过（样例 = 真实 92 条档案）；write_memory 带 map 参数做白名单闸，裸 PUT 收敛为私有传输）

#### 阶段 C：launcher 与按钮

- [x] PL007.3 cheat_adjust 命令与会话态加值按钮 —— 做法：commands.rs 增 `cheat_adjust(field, delta)`（无会话显式报错、返回新值）；App.vue 会话顶栏 HUD 右侧加「+10000 金币」「+10000 宝石」ghost 按钮（v3 色板），点击后状态行反馈新值；CSP 零改动（写入经后端转发）；验证：cargo test + vue-tsc/npm build 过（✅ 2026-09-11：锁内取 port 锁外 HTTP；注册进 invoke_handler；构建全绿）

#### 阶段 D：验收收尾

- [x] PL007.4 验收收尾 —— 做法：live 会话点按钮 → HUD 半秒内跳值（金币/宝石双向）→ 停止回写 save → 读档确认持久化（VA1）；白名单拒绝与饱和封顶单测全过（VA2）；静默启动/帧流/输入注入 live 探针三件套无回归（VA3）；全量门禁（fmt/clippy/test/doc + vue-tsc/npm build）绿；z.plan 实测结论回写 + 本组勾结上移；验证：VA1–VA3 全过 + 门禁绿（✅ 2026-09-11：VA1 双向过——gold 5620+10000=15620、gems 10+10000=10010，后端返回值与 HUD 一致；持久化复合环节（游戏自身存档）不在控制面，见实测结论口径说明；VA2 单测过；VA3 三件套全过；门禁全绿）

### PL008: CC 素材提取链路——xeen-extractor [plan#PL008]（✅ 2026-09-12 收口，8/8；双锚点格式验证 + 2457 资源全量提取 + 1059 PNG 素材墙，证据见 `.temp/pl008-verification.md`）

#### 阶段 A：格式探针（.temp，先于转正）

- [x] PL008.1 CC 目录解密探针 —— 做法：.temp 脚本对 CC 副本（retro/ 只读纪律）解析：读 2 字节个数 → TOC 滚动解密（循环左移 2 位 + 计数，初值 0xAC 每字节 +0x67）→ 数据区 XOR 0x35 还原，打印条目清单；验证：**独立锚点**——DARK.CC 目录中解出 `DARK.PAL = 0x48AA`（Xeen Wiki 官方示例，与实现无关的第三方事实）（✅ 2026-09-12：`rol2+滚动计数` 为唯一过双判据组合；DARK.PAL 解出 len=768 调色板（灰阶渐变头部自证）；三容器 1172/990/295 全 contiguous）
- [x] PL008.2 文件名对照表落地 —— 做法：自 OpenXeen / ScummVM 源码抄 哈希↔文件名 数据表（事实数据非代码）；验证：对三容器统计命名覆盖率（有名字资源 / 总资源数），抽样核对若干已知名（_.PAL、_.MAP 类）（✅ 2026-09-12：哈希 = 16 位循环右移 7 位后加字符值，穷举变体命中 0x48AA 并与 OpenXeen 实现互证；内嵌表 1757 有名 ID，槽位覆盖 98%）

#### 阶段 B：工具转正

- [x] PL008.3 cc_archive 模块 + xeen-extractor bin —— 做法：capsule-core 新增 `cc_archive.rs`（目录解密 + 资源枚举 + 数据还原）与 `src/bin/xeen-extractor.rs`（clap）：`list <cc>`（ID/文件名/偏移/长度）、`extract <cc> --out <dir>`（全量导出，无名资源按 ID 十六进制命名），默认输出根 `.xeen-extract/`；验证：单测红→绿——TOC 解密往返、XOR 还原、DARK.PAL 锚点命中；实跑 `list DARK.CC` 列出 DARK.PAL（✅ 2026-09-12：4 单测红→绿全过；实跑 list 1172 资源有名 1160、PAL 族 len=768 自洽）
- [x] PL008.4 三容器全量提取 —— 做法：XEEN.CC/DARK.CC/INTRO.CC 副本各跑 extract，输出 `.xeen-extract/cc/{xeen,dark,intro}/`；验证：条目数与 list 一致、字节总量吻合、产出清单统计（资源总数/命名覆盖比例/类别粗分）（✅ 2026-09-12：990/1172/295 = 2457 文件与目录逐一吻合）

#### 阶段 C：素材库

- [x] PL008.5 图形资源转 PNG —— 做法：先解调色板资源（\*.PAL 类），再按 ScummVM xeen 引擎源码的格式参考把可看大类转 PNG 进 `.xeen-extract/png/`（按类别做，不追求全部资源）；验证：转出的 PNG 与 ScummVM 实跑画面素材肉眼一致（✅ 2026-09-12：实况修正——ScummVM 无 xeen 引擎，格式参考改为 OpenXeen WOXSpriteFile/WOXPallet/MaMRawImage；1059 张转换 9 张特殊变体失败；目检 CHAR01/CHAR05 头像与 THRONE1 320×200 场景完整清晰；跨扩展同名覆盖缺陷修复后 1059/1059 落盘）
- [x] PL008.6 素材墙 + 肉眼基准落位 —— 做法：browse.html（资源清单 + PNG 缩略图，可翻库）；下载 ScummVM 便携版落位 `.xeen-extract/tools/scummvm/` 并实跑游戏目录；验证：ScummVM 能读同一套 CC 文件进游戏（肉眼基准成立）（✅ 2026-09-12：实况修正——肉眼基准改用 DosboxCapsule 实跑截图（本仓模拟器即最强事实），OpenXeen 源码克隆于 .temp 作格式参考；browse.html 1059 卡片生成）
- [x] PL008.7 头像资源圈定 —— 做法：借 ScummVM 源码资源名在清单中定位 portrait/face 相关资源，产出清单（资源名/尺寸/张数）落 `.xeen-extract/`；验证：清单内资源转 PNG 后与游戏内建队头像选择界面比对吻合（✅ 2026-09-12：portraits-manifest.csv 落盘——161 个 .FAC 全部转出（CHAR01–25 × 两世界建队头像集 48 张 + NPC 头像），即 PL009 输入）（✅ 2026-09-12 用户多模态复核：两世界 CHAR 集 4 张字节差异目视相同，已收敛为单套 24 张 `char01–24.fac.png` 入 `retro/artwork/faces/`）

#### 阶段 D：验收收尾

- [x] PL008.8 验收收尾 —— 做法：提取结果与实跑画面交叉对照一致（VA1）；三容器全量落库 + 素材墙可翻 + 用户判收"参考素材库"价值（VA2）；工具单测 + 门禁全绿 + 头像资源清单产出（VA3）；AGENTS 目录规划登记 `.xeen-extract/`；z.plan 实测结论回写 + 本组勾结上移；验证：VA1–VA3 全过 + 用户判收（✅ 2026-09-12：VA1 对照游戏实画成立；VA3 门禁全绿（fmt/clippy/doc/19 套件/npm）；VA2 素材墙就绪待用户翻库判收）

### PL009: 角色头像上屏 [plan#PL009]（✅ 2026-09-12 收口，3/3；face 机制破解 + live 三重验证 + 用户判收"是一致的"，勘探全记录转正 `retro/face-map.md`）

#### 阶段 A：face 机制勘探（实况修正：双存档差分 → 交互式快照差分）

- [x] PL009.1 face index 勘探 —— 做法：~~双存档差分法~~（2026-09-12 实况修正：改**交互式快照差分**——用户配合换像/换人/换队五轮快照 S0–S5，live 定位面容数组 `0x5A7C:0x0002–0x0007`（u8×6，按队伍槽位存**名册槽位**），机制 = **面容文件号 = 名册槽位 + 1**（第 N 槽用 CHAR(N+1).FAC，与 ScummVM Roster `char{idx+1}.fac` 一致）；连带破解资源哈希（rot7 累加、对无扩展名 stem 计算，24 对全吻合）与 DARK01.SAV 名册布局（记录区起 0xE01、步长 354、名字在记录头）；登记 face1~~face6（u8）+ occ1~~occ6（占用信号 = 名字首字节，空槽为零）进 memory_map；验证：五快照 + live REST + 12 角色三重验证无一例外（✅ 2026-09-12：face1–6/occ1–6 已入 memory_map；勘探全记录转正 `retro/face-map.md`；旧"+0xA9 = face"假说证伪——那是预建角色模板号，自建角色恒 0）

#### 阶段 B：头像随包与 HUD 渲染（实况修正：血条 → HP 数字着色）

- [x] PL009.2 头像随包与 HUD 渲染 —— 做法：24 张头像 PNG 入 `retro/artwork/faces/` + game.json assets 增 `faces_dir` + gameconf `AssetsSection.faces_dir`（Option<PathBuf>）+ packer 目录收集；hud.rs WATCHED 扩至 21 条目，HudSnapshot 增 `face`/`occupied`；commands 增 `get_faces`（charNN.png → id + data_url，一次性取图）；HudBar 文字格升级"头像缩略 + HP 着色数字"——~~血条~~（2026-09-12 用户定案修正：HP 数字按 **≥60% 绿 / 20–60% 黄 / <20% 红 / 负值蓝** 着色，分母用**会话滚动基线**（按头像 id 绑定、换人重置； getMaxHP 公式复刻因无显示值可对照验证，登记为后续增强））；空槽位（解雇/换人瞬间）灰色占位不抓头像；验证：TDD faces_dir 可选解析 + 目录随包收集 2 用例红→绿，fmt/clippy/vue-tsc/npm build 全绿（✅ 2026-09-12：全过）

#### 阶段 C：验收

- [x] PL009.3 验收 —— 做法：HUD 头像与游戏内角色实际肖像一致（换角色/换存档自动跟随）；live 会话目测判收；验证：重启会话后 HUD 六头像与游戏队伍面板逐位一致，2 号位带伤显黄、满员显绿，用户判收"是一致的"（✅ 2026-09-12）

> **✅ 全组完成（2026-09-12）**：3/3 勾结。全量门禁绿（fmt/clippy -D warnings/test --workspace 16 套件全过/prettier/vue-tsc/npm build）；face 机制 = 名册槽位+1（五快照 + live REST + 12 角色三重验证）；live 用户判收"是一致的"。勘探全记录转正 `retro/face-map.md`；HP 着色基线为会话滚动最大值（getMaxHP 公式复刻列后续增强，另 fac 资源哈希破解已写入档案）。

### PL010: XEEN 地图底图自渲染 [plan#PL010]（✅ 2026-09-13 收口，11/11；路线 B 自渲染全程无兜底——CUR 容器破译 + ScummVM 标准答案 + 暗面五地标实证 + 622 图交付 + 用户判收"可以对的上"；机制全文转正 `retro/map-atlas.md`）

#### 阶段 A：盘点与解析

- [x] PL010.1 地图资源盘点 —— 做法：三轮探针（.temp/pl010_probe*.py）定位迷宫几何真实存放处——**不在主 CC**（无 MAZE 名；AAZE\*.TXT = 事件文字 Big5、AAZE\*.HED = 事件头，OpenXeen 作者注佐证），XEEN.DAT 为 MZD 头运行时状态文件亦排除；最终依 OpenXeen `loadMaps()` 指认 **DARK.CUR / XEEN.CUR（CUR = current side 非光标）**，TOC 与 .CC 同款（rol2 + 滚动计数），数据区**不做 XOR**；哈希实现以 OpenXeen 测试断言 `AAZE0002.TXT = 0x7CA1` 独立校验；产出全量清单 `.temp/pl010-inventory.json`；验证：清单与 TOC 哈希逐一可对 + 结构锚点（DAT 偏移 768 的 u16 == 地图号）**主图 311/311 全过**（dark 183 + xeen 128），布局 892B 与 OpenXeen `loadMaze` 逐段吻合（flags2@780：bit15 户外 / bit14 黑暗）；区域名直解——DARK.CC 的 `DARKxxxx.TXT` 两侧统一覆盖（Big5 实解：A1 區域/湖濱城/大北塔第三層/天路 C4/龍雲），名字覆盖 244/311（✅ 2026-09-13：主图 311 + 变体文件 96（2xx/3xx 段 id 字段回指基图）；户外 dark 52 + xeen 24、黑暗 dark 39 + xeen 36；名字源确定 = DARKxxxx.TXT，PL010.6 EVT 探测降级为可选验证）
- [x] PL010.2 DAT 解析模块 —— 做法：capsule-core 新增 `maze.rs`（892B 布局严格解析：WallData 512B = 16×16 u16 四半字节 → CellFlag 256B → 内嵌地图号@768 → SurrMazes 北/东/南/西@770 → flags1@778（b14 可休息/b15 可存盘）→ flags2@780（b14 黑暗/b15 户外，.1 实测校正原立项的 778 偏移）→ 墙/地表 LUT@782/798）；`maze_file_name` 拼名规则（MAZE%04d / id≥100 MAZEX%03d）；cc_archive 增 `read_raw`（CUR 明文读取）+ `slice` 收敛越界检查；xeen-extractor 增 `mazes <cur>` 全量校验命令；验证：单测 5 条红→绿（合成夹具按 OpenXeen 布局常量独立构造 + 尺寸不符严格报错 + 变体双号 + 命名规则），`mazes` 全量跑两 CUR 零解析错误且与 .1 Python 盘点交叉全同（dark 247 = 主 183/变体 64、xeen 160 = 128/32；户外 52/24、黑暗 39/36），fmt/clippy -D warnings/test --workspace 19 套件全绿（✅ 2026-09-13）
- [x] PL010.3 TIL 帧解码 —— 做法：TIL = 标准 MaMSprite（帧表 + 调色板索引），复用 PL008 `convert.py::decode_sprite`（PL008.5 已对实机验证）写 `.temp/pl010_til.py`：全 TIL 解帧 + 拼接图落 `.xeen-extract/png/til/`，并做**四层合成首验**（dark/xeen 各自 1 号图：surfaceLUT[n0] 恒画 + n1/n2/n3 各组 0 = 无叠加约定，数据 (0,0) 左下翻转）；验证：OUTDOOR.TIL 主体帧尺寸 10×8 与文档一致（dark 50 帧 / xeen 48 帧），合成图地形合理——暗面 A1 = 龟裂红土 + 岩脊 + 地点图标、云端 A1 = 沙地 + 斜贯山脉 + 城/塔标记 + 边缘空洞；过程中修正两处实现错误（瓦片偏移漏 ×16、零叠加组误画第 48 帧盖满全图）；逐张游戏内对照留 PL010.4 承接（✅ 2026-09-13：环境集 TIL 各 53 帧同格式解出，布局与 OpenXeen 注释一致：0-1 地板 / 2-33 墙 V·H / 34-35 柱 / 36-51 备用地板）

#### 阶段 B：渲染与名字

- [x] PL010.4 户外地图渲染 —— 做法：`.temp/pl010_render_outdoor.py` 全量渲染 76 张户外（dark 52 + xeen 24）双分辨率（原生 160×128 + 最近邻 ×6 960×768，命名 darkNNN/cloudNNN + -x6 后缀）落 `.xeen-extract/png/maps/`；验证：**三世界拼接预览接缝连续性**（地面 A1–F4 邻接表网格缝合）——暗面沙漠横跨四区块连续、雪原山脊跨行对齐、道路网络跨缝贯通、大湖跨边界；云端侧沙海/雪峰/中央大湖（湖滨城）/绿林/南境熔岩区全部成形且接缝连续；地形常识核验无 uniform 垃圾图（✅ 2026-09-13：76×2 = 152 文件；过程修正两处拼接脚本 bug——网格行列未去重、id/raw 混存；游戏内 Tab 逐张实测对照并入 PL010.10 验收，由用户对熟悉区块抽查）
- [x] PL010.5 户内地图渲染 —— 做法：主路线（游戏原始贴图，用户定案：不设兜底色块）——拿到**标准答案源码** ScummVM `interface_minimap.cpp`（drawIndoorsMinimap）：地面 = 每格 frame 0；地面点缀 = CellFlag 低 3 位 surfaceId（非 0）画 surfaceLUT[surf]+36 于格心 (+5,+5)；墙 = nibble3（西墙）查固定帧表 {1:18,2:34,3:22,4:16,5:2,6:30,7:32,8:2,9:20,10:28,11:14,12:4,13:16,14:24,15:26}、nibble0（北墙）查 {…+1 各奇数帧}（东/南墙由邻格表达，0 = 不画）；环境集选择 = maze 头 **offset 826 wallKind → [TOWN, CAVE, TOWR, CSTL, DUNG, SCFI]**（暗面聚类实证：0=五镇、1=下水道/矿坑、2=群塔、3=城堡、4=神庙地牢、5=逃生艇科幻）；布局偏移以 MazeData::synchronize 序列化序核对（floorType@814/difficulties@816–822/wallKind@826，总长 892 吻合）；修正两处实现错误（帧包围盒裁剪吞掉带偏移的墙条像素、force 透明覆盖）；验证：**暗面五地标渲染全对**——湖滨城=建筑群+水岸（TOWN）、卡琳卓城堡=蓝墙块+绿庭（CSTL）、大北塔 L1=八角塔楼层平面（TOWR）、丧魂地牢（DUNG）、逃生艇（SCFI）；235 张（dark 131 + xeen 104）全量渲染双分辨率零缺料（✅ 2026-09-13：云端侧个别图（如 cloud033）虚空占比高系数据本身如此，且区域名表 DARKxxxx.TXT 为两侧共用、云端名可能张冠李戴——登记 map-atlas 勘误；云端地标终审并入 PL010.10 用户验收）
- [x] PL010.6 EVT 名字探测（限时决策门，不阻塞主线）—— 做法：~~EVT 锚点反查~~ **注销（2026-09-13）**——PL010.1 盘点发现 DARK.CC 的 `DARKxxxx.TXT` 区域名文件（游戏自身地图显示名，两侧统一覆盖 244/311），名字源问题前置解决，EVT 探测失去必要性；验证：不适用（名字质量由 .7 抽查与 .10 用户验收承接）（✅ 2026-09-13 注销）
- [x] PL010.7 地图名对照表 —— 做法：名字源 = DARK.CC `DARKxxxx.TXT`（Big5 直解，游戏自身地图显示名，与 OpenXeen `getAreaNameFile` 同源）；无名图（多为未用/内部图）`name: null` 回落语义定案；名字内嵌 index.json 逐图字段；勘误登记：区域名表两侧共用，云端侧名字可能张冠李戴（OpenXeen 同有此 TODO 遗留）；验证：样例抽查语义吻合（A1 區域/湖濱城/大北塔第三層/天路 C4/龍雲/火元素平原 与游戏认知一致），244/311 覆盖，终审并入 .10 用户验收（✅ 2026-09-13）

#### 阶段 C：交付验收

- [x] PL010.8 index.json 对照表 —— 做法：生成 `retro/artwork/maps/index.json`：全量 311 图（id/文件名/名字/户外/黑暗/环境集/四向邻接）+ 坐标系说明（16×16、数据 (0,0) 左下需翻转、换算式 `image_px = (x × S_w, (15−y) × S_h)`、原生与 ×6 两套 tile 尺寸）+ 变体文件与名字回落语义说明；验证：JSON 解析通过、dark 183 + xeen 128 与盘点一致、样例字段核对无误（✅ 2026-09-13）
- [x] PL010.9 交付归档 —— 做法：622 个 PNG（311 × 双分辨率）落 `retro/artwork/maps/{dark,cloud}/` + index.json；机制档案 `retro/map-atlas.md` 转正（892B 布局表/户外与户内渲染规则含墙帧表/环境集表序/坐标换算/清单摘要/勘误七则：16×16 非 22×22、版权口径、ScummVM 支持勘误、AAZE 语义、CUR 容器、XEEN.DAT 性质、云端名共用表）；验证：文件齐套核对 dark 366 + cloud 256 + index.json，prettier 门禁过（✅ 2026-09-13）
- [x] PL010.10 用户验收（不进游戏）—— 做法：用户抽查套图与已知地图布局比对 + 指定格子让我按 index.json 在对应像素指认；验证：用户判收（✅ 2026-09-13 用户："可以对的上"——湖滨城/城堡/塔层/拼接预览目测通过；云端名共用表问题知悉，发现错名随报随修）
- [x] PL010.11 世界拼接图（收尾彩蛋）—— 做法：区域名网格缝合出整图（暗面大陆、暗面天路、云端大陆各原生 960×512 + ×6 5760×3072）落 `retro/artwork/maps/world-*.png`，index.json 增 worlds 段；~~初版按 4 列 × 6 行转置排布~~（2026-09-13 实况修正：用户提供官方 in-game world map 参考图发现拼错——区域名语义 = **字母为列（西→东）、数字为行（北→南）**，判据 = A1 N=0/W=0、A4 S=0、F 列全 E=0；修正后与参考图逐区吻合，勘误第八则登记）；验证：修正版与官方参考图逐区比对一致 + 六文件落位核对（✅ 2026-09-13）

> **✅ PL010 全组完成（2026-09-13）**：11/11 勾结。311 主图 + 96 变体盘点、892B 布局解析（Rust `maze.rs` + `mazes` 命令，双实现交叉全同）、76 户外 + 235 户内全量双分辨率渲染（622 文件）、区域名 244/311、index.json + `retro/map-atlas.md` 转正、世界整图三张。路线 B 全程未启用兜底；云端地标终审随验收持续（发现错名随报随修 index.json）。

### PL011: 队伍位置读取——X/Y 坐标与所在地图 [plan#PL011]（✅ 2026-09-13 收口，7/7；ScummVM party.cpp 签名四连字节 + 三轮快照差分锁定 0x5A7CA 链 + 区域名双文件族破解（XEEN/DARKxxxx.TXT 按侧）+ HUD 五要素 live 判收"HUD对的"；用户定案追加朝向要素）

#### 阶段 A：内存勘探（复用快照差分，需用户配合）

- [x] PL011.1 X/Y 坐标勘探 —— 做法：~~盲目三轮差分~~（实况升级：先取 ScummVM `party.cpp:286-289` 标准答案——**方向/X/Y/地图号四连字节**各 1 字节，签名扫描代替盲扫）；用户配合三轮快照（原地→走一步→原地转向），签名过滤 15752 命中 → 同偏移差分收敛 13 → **0x5A7CA 锁定**（走步 x 10→11 步长 1、转向 dir 1→3 仅此字节变、与 face 数组同段 0x5A7C 印证队伍状态块）；验证：走步 ±1 / 转向不动 / 跨图重置入口坐标三重语义全对（✅ 2026-09-13）
- [x] PL011.2 地图号勘探 —— 做法：map 字节 @0x5A7CD 跨图实测（户外 23 → 进佛提格市 28，坐标重置入口、朝向重置 0）；**侧标志 @0x5A7CE** 同步探得（1=云端星云面/0=暗面，经用户 live 确认——用户所在云端读数恒 1）；实况修正：坐标在城镇连续空间可超 16（佛提格市由 MAZE 28+109+110 邻接拼合，map 字节保持进城锚点号——已登记 y.problems#21 城镇拼合渲染缺陷）；验证：跨图枚举切换与游戏内 Tab 一致（✅ 2026-09-13）
- [x] PL011.3 双重验证 + memory_map 登记 —— 做法：dir/pos_x/pos_y/map_id/side 五条登记 memory_map.json（0x5A7C:0x000A–0x000E，108→109 条）；live REST 读值对照（x=11/y=12/map=23 户外、15,0/28 城内、16-18,4-8 城内连续空间）；验证：读数与游戏内 Tab 逐位一致（用户截图 X=15/Y=0 与 HUD 同帧吻合）（✅ 2026-09-13）

#### 阶段 B：HUD 五要素上屏（用户定案追加朝向）

- [x] PL011.4 快照链路扩字段 —— 做法：hud.rs WATCHED 21→26 条目（+dir/pos_x/pos_y/map_id/side）+ HudSnapshot 扩 7 字段（direction/pos_x/pos_y/map_id/side + 后端查表所得 map_name/map_outdoors）；HudPollEvent::Snapshot Box 化（clippy large_enum_variant）；验证：cargo fmt/clippy -D warnings/test --workspace 19 套件全绿（✅ 2026-09-13）
- [x] PL011.5 HUD 显示 + 名字查表 —— 做法：~~静态 JSON~~（实况修正：launcher 无 serde_json 依赖，改**内嵌 mapnames.csv** 手写解析，cc_archive_names.csv 同款先例，零新依赖）；HudBar 新增位置格（名字/户外户内标志/编号/X,Y/朝向北东南西）；**名字源两次实况修正**：①初版按 DARKxxxx.TXT 单表——用户 live 报错（佛提格市显示水元素平原）②live 内存 Big5 搜索"佛提格市"一发命中 **XEEN0028.TXT**——破解区域名双文件族：云端 = XEENxxxx.TXT（85 个）优先回落 DARK，暗面 = DARKxxxx.TXT，同号不同名；连带修复 memory_map 漏登 side 条目导致查表恒走暗面列的 bug；验证：mapnames.csv 重造（cloud 85+30 回落 / dark 183），佛提格市 HUD 显示与游戏 Tab 一致（✅ 2026-09-13）

#### 阶段 C：验收

- [x] PL011.6 live 正确性验证 —— 做法：live 走动/跨图/转向多轮对照；验证：坐标/朝向/地图号实时跟手，佛提格市名与游戏 Tab 吻合；坐标超 16 现象实证为城镇连续空间（引擎真值原样显示）（✅ 2026-09-13）
- [x] PL011.7 用户目测判收 + 归档 —— 做法：用户 live 目测判收；勘探记录转正 `retro/map-atlas.md`（勘误七更正为区域名双文件族 + 新增勘误九城镇连续空间）；验证：用户判收"HUD对的"（✅ 2026-09-13）

> **✅ PL011 全组完成（2026-09-13）**：7/7 勾结。五要素 = 地图名/户外户内/编号/X,Y/朝向（用户定案追加朝向）。位置四连字节 0x5A7CA–0x000D + 侧标志 0x5A7CE（1=云端）；区域名双文件族 XEEN/DARKxxxx.TXT 按侧破解（勘误 PL010"共用表"结论）。城镇多图拼合渲染缺陷登记 y.problems#21。

## 未完成

### PL012: 游玩 UI 重构 [plan#PL012]（2026-09-13 立项登记；2026-09-15 起实验场阶段实施中；消费 PL010 底图 + PL011 位置数据）

> 实况（2026-09-16）：实验场单文件 HTML `assets/design/pl012-游玩界面实验场.html` 已迭代四轮——v1–v2 三栏布局与部件、v3 尺寸结构定稿（舞台 1850×1000）、v4 任务链系统、v4.5 头像比例与页脚；并追加左栏操作栏与浮窗（魔法/注音/说明/设置）、背景图按世界切换与毛玻璃滑杆、Uiverse 六组件接入、CRT 特效归属定案（前端侧，DOSBox shader 弃用）。对接要点与后端缺口见 `assets/design/PL012-game-ui-handoff.md`。
> 后端缺口核对（2026-09-16 逐条对码）：六项中仅 ② 前提过期（`death1–6`/`unc1–6`/`weak1–6` 已登记 memory_map，白名单为"登记即放行"，无需放开）；①③④⑤⑥ 均未做。

- [ ] PL012.1 立项细化讨论 —— 做法：过四个决策点（信息层级 / 小地图交互形态：固定角标 vs 悬停展开 vs 全图切换 / 攻略入口形态 / 连续热更 vs 整版）；输入 = PL010 PNG + PL011 四要素 + assets/design 五稿 + PL012 实验场；验证：用户拍板设计方案与拆分
- [ ] PL012.2 实验场迭代收口 —— 做法：补完实验场未完成项——老板按钮（点击界面整体消失 + omarchy 伪装动画）、职业徽章（队伍槽职业图形化）、小地图中键拖拽联动（按住拖动全向平滑跟随 + 贴边停止）；验证：浏览器直开实验场逐项目测
- [ ] PL012.3 后端缺口补齐 —— 做法：①等级 LV 寄存器（升级事件前后全量快照差分定位，入 memory_map + 白名单）②复活命令与按钮（清 death/unc/weak，白名单已开放无需改闸）③HP/MP 写入钳制 `min(当前+100, 200)`（launcher 侧钳制后调 write_memory）④中毒状态寄存器破解入档 ⑤状态头像套图（先勘探来源——51 个 `.ICN` 多为界面按钮，需确认是叠加图标还是独立套图，再走 xeen-extractor 提取）⑥任务链数据源（`retro/xeen-guide.md` 结构化 → npc/x/y/stages/world）；验证：③ 单测（钳制边界）+ live 写读回；①④⑤⑥ 各自 live 或对照证据
- [ ] PL012.4 对接 Launcher —— 做法：按交接文档第二节施工——`#game-view` 内占位 img 换 GameCanvas 帧流画布（CRT 玻璃/扫描线图层 z-index 不动）、假数据常量换后端注入、换算公式 `image_px = (x × S_w, (15−y) × S_h)` 与城内连续坐标/世界前缀照定案、任务勾选状态改 json 或 sqlite；验证：live 会话全要素显示正确 + 用户目测
- [ ] PL012.5 验收收尾 —— 做法：用户 live 判收 + 全量门禁（fmt/clippy/test/doc + prettier/vue-tsc/npm build）+ z.plan 附录 PL012 实测结论回写 + 本组勾结上移；验证：判收 + 门禁绿

### FIX003: 第3轮审计修复 [audit#A003]

> 来源：A003（2026-10-05 三路并行审计 + 回归复核）。零节结论：A002 36 条中 35 条在位，FIX002.7 虚假勾结重报为 FIX003.14；19 条参考级观察项维持参考级（z.plan 附录 A003 第二节）。条目编号与 A003 清单 # 一一对应。

- [x] FIX003.1 [P2] save 嵌套条目往返静默丢失 —— 做法：save.rs unpack（:149-168）对 `save/` 前缀下深度 >1 的普通文件条目报 UnsafeEntry（组件计数 >1 即拒绝，对齐 save_files 只认顶层契约；目录条目与未知前缀良性忽略维持 FIX002.3 白名单不变）；验证：新增测试——含 `save/sub/X.SAV` 的 zip 解包报 UnsafeEntry、含 `save/XEEN01.SAV` 与目录条目的 zip 正常解包（2026-10-05 已验证：新测试过 + stash 回退对比确认修复前 FAIL 修复后 PASS + FIX002.3 回归锚过，见 .temp/fix003-verification.md）
- [x] FIX003.2 [P2] maptool 仓库根机器绝对路径 —— 做法：til.py:14 `REPO_ROOT` 改为 `os.path.dirname` 链上溯三级自 `__file__` 派生；baseline-compare.py:14-15 TEMP/PROFILE 顺带同改（与 capture.sh 配套设计一并评估，如 capture.sh 传参则保持接收方兼容）；验证：换工作目录执行 `python tools/maptool/maptool.py list` 与 `python tools/dosbox-fork/baseline-compare.py --help` 正常（路径不再依赖克隆位置）（2026-10-05 已验证：双 cwd 跑 maptool inventory 输出一致 + baseline-compare 换 cwd 真实跑通基线判定 109 条目 GO，见 .temp/fix003-verification.md）
- [x] FIX003.3 [P2] 打包校验配置与内嵌 game.json 错配 —— 做法：capsule-pack lib.rs build_package 入口（:55 附近）对 inputs.game_json 补 require_file 前置校验；main.rs（:27-32）repo_root 推导后断言 args.game_json 规范化路径 == repo_root/retro/game.json，不等即报错说明两文件用途；验证：新增测试——传入非规范路径 game_json 打包报错且消息含指引；正常 `retro/game.json` 打包不受影响（2026-10-05 已验证：新测试过 + CLI 冒烟双过——非规范路径报不一致带指引、规范路径放行走 launcher 校验，见 .temp/fix003-verification.md）
- [x] FIX003.4 [P3] patch by_name 错误误分类 MissingSource —— 做法：patch.rs read_zip_file（:135-138）by_name 错误匹配 `ZipError::FileNotFound` 才返回 Ok(None)，其余错误映射 Corrupted（消息带条目名）；验证：新增测试——加密/不支持压缩条目报 Corrupted 而非 MissingSource（先验证 zip 8.x 在 by_name 阶段的实际错误类型，据此定断言）（2026-10-05 已验证：探针实锤 zip 8.6 的 bzip2 错误在 read 腿抛出、by_name 可达错误仅 FileNotFound——收窄为防御性；bzip2 fixture 测试过 + 缺 source 回归过，见 .temp/fix003-verification.md）
- [x] FIX003.5 [P3] cc_archive 名字表 CSV 错行静默跳过 —— 做法：cc_archive.rs builtin_names（:182-195）filter_map 改 for + match：非注释行 split_once 或 hex 解析失败时 eprintln 警告（含行内容）后跳过；验证：新增测试——夹具 CSV 含坏行时解析成功且坏行警告出现（断言 warning 捕获或计数）（2026-10-05 已验证：提取 parse_names_table + unit test 坏行跳过不吞好行过；真实 CSV 1755 行零坏行不引入新警告、内置名字表锚点回归过）
- [x] FIX003.6 [P3] validate_rel 三份重复收敛 —— 做法：capsule-core 公共位置（如 lib.rs 或新 util）提取 `fn validate_rel_path(path: &str) -> Result<PathBuf, ()>`，save.rs:364-374 / patch.rs:148-157 / payload.rs:391-401 三处改调用 + map_err 到各自错误变体，消除两处 fn 内 use；验证：cargo test --workspace 全绿 + grep 确认三处校验逻辑仅存一份（2026-10-05 已验证：lib.rs `pub(crate) fn validate_rel_path -> Option<PathBuf>` 单份，三处调用点各自 map_err；grep 断言 Component::Normal 仅 lib.rs:32 一份；三处既有穿越拒绝测试全过）
- [x] FIX003.7 [P3] save meta 读取三处重复 —— 做法：save.rs 提取 `fn read_meta_from_archive(archive: &mut ZipArchive<File>) -> Result<SaveMeta, SaveError>`，unpack（:119-129）与 read_meta（:255-271）共用，错误文案统一；验证：cargo test -p capsule-core（save 相关用例全绿）（2026-10-05 已验证：提取函数双调用点收敛、文案统一，save 相关测试全绿）
- [x] FIX003.8 [P3] xeen-extractor 文档缺 mazes —— 做法：xeen-extractor.rs:1-8 模块 //! 头注与 :21 clap about 补 "mazes" 子命令；验证：`cargo run -p capsule-core --bin xeen-extractor -- --help` 顶层描述含 mazes + cargo doc 无警告（2026-10-05 已验证：--help 显示 "list/extract/mazes" + cargo doc 无警告）
- [x] FIX003.9 [P3] corrupt_backup_path 错误变体错位 —— 做法：save.rs:353-356 路径缺文件名改返回 `SaveError::Io`（`io::Error::new(InvalidInput, …)`），不再借用 Corrupted 降级语义；验证：cargo test -p capsule-core + grep 确认 Corrupted 仅用于档案损坏分类（2026-10-06 已验证：Corrupted 产生点收敛至真正的档案损坏五处、路径形状错误走 Io 严格上抛，见 .temp/fix003-verification.md）
- [x] FIX003.10 [P3] 会话入槽前死亡窗口 —— 做法：commands.rs launch_game（:594-615）入槽前补 `if !session.is_alive() { session.stop() 落日志; return Err("游戏进程启动后立即退出，请查看 dosbox.log"); }`；验证：代码审查 + cargo test（如可模拟）；live 短命进程场景日志与错误消息正确（2026-10-06 已验证：入槽前 is_alive 拦截 + stop 落日志 + 指引消息；活进程路径零影响——检查为纯读；clippy/test 全绿）
- [x] FIX003.11 [P3] 毒锁静默放弃 + 豁免清单失配维护 —— 做法：commands.rs exit-watcher（:517-520）`Err(_)` 分支补 logging::error 留痕后 return；z.plan §四 豁免清单 #6（expect 会话槽锁毒化）条目更新为现行语义（毒锁静默 return + 日志留痕）或删除；验证：代码审查确认日志路径在位 + 豁免清单与代码一致（2026-10-06 已验证：留痕 return 在位 + 豁免清单 #6 四列对齐现行语义，失配消除）
- [x] FIX003.12 [P3] HudSnapshot.side TS 漂移 —— 做法：hud.rs:69-70 side 字段加 `#[serde(skip_serializing)]`（后端查表内部用途保留）；验证：cargo test -p launcher + 前端 HudBar.vue 接口注释与实际载荷一致（hud 事件 JSON 无 side）（2026-10-06 已验证：skip_serializing 在位、内部用途保留、VD 双通道对照不受影响，cargo test -p launcher 全绿）
- [x] FIX003.13 [P3] wait_for_handle 死代码 —— 做法：删除 winproc.rs:180-182 wait_for_handle；验证：cargo build + clippy -D warnings + cargo test --workspace 全绿（2026-10-06 已验证：删除前确认 WAIT_TIMEOUT 另有 :139/:144 使用者 import 不失效；clippy/test 全绿）
- [x] FIX003.14 [P3] fn 内 use 三处（FIX002.7 虚假勾结重报）—— 做法：launcher_hud_test.rs:74-75（direct_read）、:120-121（cpu_seconds）与 hud.rs:319（map_names 内 `use std::sync::OnceLock`）上移至各文件头部 use 区（标准库 → 第三方 → 本地分组）；验证：grep 确认三文件无 fn 内 use + clippy/test 全绿（2026-10-06 已验证：grep 零残留 + OnceLock 并入 std 组——FIX002.7 虚假勾结遗留账就此清零）
- [x] FIX003.15 [P3] 缺 /// 文档注释三处 —— 做法：hud.rs:135 poll_loop、winproc.rs:212 cmdline_args_are_quoted、:224 cmdline_roundtrip_via_utf16 补 1-3 行中文 ///；验证：cargo doc --no-deps 无警告 + clippy 过（2026-10-06 已验证：三处补齐，doc 无警告 + clippy 过）
- [x] FIX003.16 [P3] hud.rs 注释双数字过期 —— 做法：hud.rs:150 注释"21 个条目 / 104 条目"改"26 个条目 / 109 条目全量"（与 :22-26 WATCHED 与 memory_map.json 实况一致）；验证：代码审查注释与事实一致（2026-10-06 已验证：动手前实数核对——WATCHED 数组字面 26 项、memory_map.json entries=109，注释与两处事实一致）
- [ ] FIX003.17 [P3] render_indoor 两种 None 混报 + 越界 IndexError —— 做法：frames_of（:89-95）区分"wall_kind ≥ 6 无 TERRAIN_TYPES"与"TIL 缺文件"两分支各自明确报错消息；:120 打印分支按成因分流并防 TERRAIN_TYPES[raw[826]] 越界（先判范围）；验证：构造 wall_kind 越界样例跑渲染 → 错误消息明确指因而非 traceback
- [ ] FIX003.18 [P3] city 元数据重复组装与死返回值 —— 做法：city_stitch.py stitch_city 返回的 city_index（:92-106）改为 gen_index.py 消费（all 流程传递），或删除 city_stitch 组装保留 gen_index 侧；二选一消除双源；验证：`python tools/maptool/maptool.py all` 产物 index.json 的 city_maps 段与改前逐字段一致
- [ ] FIX003.19 [P3] index.json 不感知渲染跳过 —— 做法：render_indoor/city_stitch 收集 skipped 集合并传入 gen_index（或 gen_index 生成时对 sides/city 逐条 png 存在性校验告警）；验证：临时移走一个 TIL 跑 all → index.json 不登记该图或输出告警（验证后还原）
- [ ] FIX003.20 [P3] xeendata BFS/DFS 注释失真 —— 做法：xeendata.py:141-164 docstring "BFS" 改 "DFS（栈实现，遍历序不影响结果）"；验证：代码审查注释与实现一致
- [ ] FIX003.21 [P3] blit 闭包跨文件重复 —— 做法：抽共享 blit 工厂（入参帧/宽高，返回写入函数）置于 til.py 或 xeendata.py 共享层，render_indoor.py:38-50 与 city_stitch.py:42-53 改调用；验证：`maptool.py all` 重渲产物与改前逐字节一致（或抽查若干图对照）
- [ ] FIX003.22 [P3] decode_name 不可达分支 —— 做法：删除 til.py:93 `return raw.hex()` 死代码（latin-1 解码任意字节均成功）；验证：代码审查 + `maptool.py list` 输出不变
- [ ] FIX003.23 [P3] til.py docstring 路径守卫过强 —— 做法：til.py:6 "所有文件读写经路径守卫"改"所有写操作经路径守卫"；验证：代码审查与实际覆盖一致
- [ ] FIX003.24 [P3] CUR 容器重复解析 —— 做法：xeendata.py load_inventory（:113）与 read_maze（:36-38）收敛到共享缓存入口（统一走 _CUR_CACHE 或新 parse_cc_cached）；验证：`maptool.py mazes` 输出不变 + 重复解析消除（计数或性能对比）
- [ ] FIX003.25 [P3] world_stitch 缺位警告后照产 —— 做法：world_stitch.py（:28-45）网格缺位改抛 ValueError（缺哪格指名道姓），移除"警告后继续"；如用户判定需容错则走 AGENTS.md 白名单登记三要素；验证：构造缺格网格跑 stitch → 显式报错不产图
- [ ] FIX003.26 [P3] gen_index import 顺序 —— 做法：gen_index.py:12-18 `from pathlib import Path` 上移至标准库组（本地模块组之前）；验证：代码审查符合 AGENTS.md 组序约定
- [ ] FIX003.27 [P3] WORLDS 硬编码第二来源 —— 做法：gen_index.py:29-34 WORLDS 清单与尺寸改为消费 world_stitch 产出元数据（与 FIX003.18 同一方案一并落地）；验证：`maptool.py all` 产物 index.json 的 worlds 段与改前一致
- [ ] FIX003.28 [P3] index.json 日期破坏可重现 —— 做法：gen_index.py:41 移除 generated 当日日期字段（或改为数据指纹）；验证：连续两日重跑 all（或伪造系统时间）→ index.json 无 diff
- [ ] FIX003.29 [P3] cmd_publish 裸 traceback —— 做法：maptool.py cmd_publish（:30-51）入口检查 OUT_DIR/cloud/dark 存在性，缺失时打印"先执行 all/singles"指引后 sys.exit(1)；验证：空目录跑 publish → 指引消息 + 退出码 1（非 traceback）
- [ ] FIX003.30 [P3] 头像链路健壮性三连 —— 做法：①commands.rs get_faces_impl（:858-876）对非 charNN.png 命名模式的文件跳过 + WARN（杂物容错走 AGENTS.md 白名单登记三要素）；②HudBar.vue:97-99 get_faces 失败 console.warn 保留并登记白名单（场景=头像目录读取失败，降级=槽位号占位）；③HudBar.vue:121-127 Big5 空 catch 补 console.warn 并登记白名单（场景=解码器能力探测，降级=utf-8 回落）；验证：faces 目录放 Thumbs.db → 头像正常加载 + 日志 WARN；单测/live 头像链路不回归
- [ ] FIX003.31 [P3] PackInputs.launcher_exe 注释反义 —— 做法：capsule-pack lib.rs:26 注释改"不应已含 payload（含者拒绝）"；验证：代码审查与 :59-68 实现一致
- [ ] FIX003.32 [P3] 基线判定文档条目数漂移 —— 做法：baseline-compare.py:5 docstring 与 PATCHES.md:31、:39 三处"92 条目"改"109 条目"；验证：grep 三文件无 92 残留
- [ ] FIX003.33 [P3] 验证收尾 —— 做法：全量门禁（cargo fmt --check + clippy -D warnings + cargo test --workspace + npm run build + cargo doc）+ maptool 回归（maptool.py all 产物与改前对照）+ x.progress 本组勾结上移 + z.plan 附录 A003 状态改 ✅；验证：门禁全绿 + 修复证据落 .temp
