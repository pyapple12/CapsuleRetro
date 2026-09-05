# 项目分析报告（w.study.md）

> 文件职责：对项目代码与外部依赖的系统性研究存档。结构：1 项目概述 → 2 目录结构与模块职责 → 3 核心设计模式（真实代码片段 + 要点列表）→ 4 代码风格观察 → 5 与其他项目对比。代码未落地的章节留占位，落地后按实际回改。

## 1. 项目概述

CapsuleRetro：把任意 DOS 游戏打包成"看起来像移植的现代游戏"，三平台通用，产物 = 执行文件 + save 文件。组件流水线：

```
[打包] retro/<Game>/ 原版素材 + 根目录适配层(game.json 等) → capsule-pack → GameName.exe + GameName.save
[运行] launcher 拉起 dosbox.exe (sidecar) → 游戏会话 → monitor 读 REST API → HUD/攻略 UI
[数据] capsule-core：save 打包/解包、补丁应用、路径管理（launcher / packer 共用，逻辑只写一份）
```

## 2. 目录结构与模块职责

（骨架为规划态，代码落地后按实际回改）

```
capsule-core/    # Rust crate：save 打包/解包、补丁应用、路径管理、monitor（REST 观察库）
capsule-pack/    # Rust CLI：game.json → 游戏包
launcher/        # Tauri 2：src-tauri/ 后端（进程编排）+ src/ Vue 前端（封面/攻略/HUD）
assets/dosbox/   # DOSBox Staging sidecar 二进制（锁版本 0.83.0，进 git）
retro/           # 根 = 适配层（game.json / memory_map.json / dosbox conf，我们写的）
  └── XEEN/      # 子目录 = 纯原版游戏文件（字面意义只读，一个字节不动）
```

## 3. 核心设计模式

### 3.1 DOSBox Staging 外部挂载 — HTTP REST API（实测 2026-09-05，v0.83.0）

启用方式（零文件改动，命令行覆盖即可）：

```bash
dosbox.exe --set "webserver_enabled=on" --set "webserver_port=18086" -conf game.conf
# 或写入 conf 文件的 [webserver] 节：webserver_enabled = on（默认 off，仅绑 127.0.0.1:8086）
```

端点清单（实测可用）：

| 端点                                    | 能力                             |
| --------------------------------------- | -------------------------------- |
| `GET /api/v1/cpu/state`                 | 全部 CPU 寄存器实时值            |
| `GET /api/v1/memory/:seg/:off/:len`     | 读任意内存（支持 `0x` 十六进制） |
| `GET /api/v1/dos/internals`             | DOS 内部结构指针（PSP / LoL 等） |
| `GET /api/v1/dosbox/info`               | 版本与配置路径                   |
| `POST /api/v1/memory/allocate` / `free` | 在 guest 中分配/释放 DOS 内存    |

实测输出示例（XEEN 运行中，`GET /api/v1/cpu/state`）：

```json
{
  "registers": {
    "cs": 4518,
    "ds": 36370,
    "eax": 2052,
    "ebp": 0,
    "ebx": 3,
    "ecx": 0,
    "edi": 32384,
    "edx": 986,
    "eip": 25905,
    "esi": 64003,
    "esp": 3948,
    "flags": 29254,
    "ss": 8572
  }
}
```

要点与边界：

- **原子性保证**（官方文档）：所有端点在模拟 CPU 的指令步进之间原子执行，永不落在 DOS 中断处理中间——任意时刻的内存快照内部一致，这是外部组件可信读取的基础
- **v1 只读**：没有内存写入、按键注入、运行时调速端点；控制型需求要走调试器构建或上游 feature request
- **实模式匹配**：API 主要针对 `core = normal` 16 位实模式应用测试，XEEN（1993 DOS 游戏）正是此类，匹配度最好
- **安全边界**：API 对 DOSBox 有完全控制权，默认仅绑定 127.0.0.1，永不可暴露到不可信主机
- **版本锁定**：0.83 新特性，sidecar 二进制必须锁版本，防止升级破坏行为

### 3.2 伴生组件双壳模式 — 一个 monitor 库，两个消费者

观察逻辑（REST 轮询 + 按档案偏移取值）写成 `capsule-core::monitor` 库模块，不含任何界面：

- **A 壳（CLI）**：capsule-core 的 bin，加载 `retro/memory_map.json` → 轮询 → 控制台打印。独立诊断工具，适配新游戏全靠它调偏移
- **B 壳（launcher）**：Tauri 后端调用同一库模块，数据经 Tauri 事件推给 Vue 前端渲染 HUD

融合路径：A→B 不是改写，是同一库增加第二个消费者。轮询逻辑从第一天起写进库函数，禁止在壳里实现。

### 3.3 其余候选挂载点盘点（未采用/备用）

- **串口 null-modem / IPX**：`serial1 = nullmodem` over TCP——适合"游戏内即与之通信"的组件（联机、外置机器人），单机增强用不上，备用
- **调试器构建**：`dosbox_with_debugger.exe` 可写内存、设断点，是控制型（金手指）唯一现成通道，定位调试工具，不作分发形态
- **plugins/ 目录**：仅 CLAP 音频插件（Nuked-SC55），非通用插件系统，排除
- **窗口层**：计划书 §4.3 的 SetParent 嵌入（Windows）/ 点击穿透覆盖层（macOS/Linux），属 UI 伴生通道，与数据通道正交

## 4. 代码风格观察

（暂无——代码未落地；首次审计后按实际记录）

## 5. 与其他项目对比

（暂无）
