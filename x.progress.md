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

（暂无）
