# PL015 · 游玩界面实验场 交接文档

（文档文件名沿用 PL012-game-ui-handoff.md；所属任务组改号史：2026-10-06 改号 PL013，2026-10-07 让位金框按钮动效顺延 PL014，同日职业徽章拆出独立立项 PL014、本组顺延为 PL015 游玩 UI 重构。）

- 产物：`assets/design/pl012-游玩界面实验场.html`（自包含单文件，浏览器直开）
- 素材：`assets/design/pl012-gameview-sample.png`（CRT 屏内实机画面占位）
- 设计基准：1920×1080（内容 1850×1000，上下左右对称留白）
- 规则档案：`retro/map-atlas.md`（渲染/坐标/历法依据）· `retro/memory_map.json`（内存寄存器）

---

## 一、实验过程简述

PL015 目标 = 重构游戏运行时 UI：中央为 CRT 显示器包裹的游戏画面，左侧日历 + 队伍面板，右侧地图 + 任务面板。全程以单文件 HTML 实验场迭代（静态假数据，浏览器直开），用户逐轮目测提意见、当日收敛，共四轮大版本（**2026-10-08 追加：队伍槽职业与人种徽章已落地**——独立立项 PL014，队伍行尾 [人种][职业] 双胶囊徽章，人种中性灰金、职业五组配色金绿蓝紫红，文字 SVG 化双向居中 + `dy="-1"` 雅黑墨迹视觉修正，详见 x.progress 已完成区与 z.plan 附录 PL014）：

1. **v1–v2 布局与部件**：三栏骨架（左信息 / 中 CRT / 右地图+攻略）；CRT 塑料壳 + 电源灯 + 旋钮 + 品牌；下方控制条三开关（电源 / 扫描线 / 弧面反光）；队伍 6 槽（真头像 + HP 四色 + MP 恒蓝 + 空槽占位）；日历（魔年 + 第 N 天/100 + 十天一周 + ☀️/🌙 昼夜）；资源卡（金币 / 宝石 / 食物≈天数）；任务卡（链清单 + 最近事件）。
2. **v3 尺寸与结构定稿**：舞台固定 1850×1000；面板高 1000；屏幕改直角；字号整体放大 25% 后回调为 +10%（累计 ≈ ×1.375）；滑杆替换按钮；图钉移到人名前。
3. **v4 任务链系统**：任务从"零散条目"改为**链结构**（每链多阶段，跨地图链演示"未执行到"）；排序规则（置顶 > 已执行到本地区近→远 > 未执行到）；点击展开所在链清单（每条三行：人物 / 位置 / 任务信息）；标签单发贴右（置顶任务 > 最近任务 > 可执行/未准备）。
4. **v4.5 头像与页脚**：头像比例锁定 0.85（100:118，object-fit: fill）；状态占位（昏迷 / 死亡框式占位，待真头像替换）；页脚精简为版权行。

关键机制定案：

- **地图联动**：小地图取景 9×9 格（单格 30×24，原生 10×8 × 3）；中心与朝向固定，xy 移动表现为地图整体平移，贴边钳制；坐标含试验偏移 `TEST_OFFSET_X = -5`（已定格）。
- **中键按住**：在地图上按住中键 = 朝向持续移动（200ms 步进），贴边自动停。
- **世界过滤**：任务分 xeen（星云界）/ dark（暗黑界）两个世界；"全部任务"只显示当前世界。
- **历法**：一年 100 天、十天一周（ScummVM `party.cpp addTime`：`_day` 满 100 归零并 `_year++`；星期 = `_day % 10`；夜晚 = `_minutes < 5:00 或 ≥ 21:00`）。

## 二、前后端对接注意事项

1. **帧流接入点**：`#game-view` 内的 `<img>` 占位（现为实机截图）整体替换为 GameCanvas 帧流画布，CRT 玻璃 / 扫描线图层叠加在其上，层级 z-index 勿动。
2. **数据注入**：实验场假数据集中在脚本顶部常量（`PARTY` / `CALENDAR` / `RES` / `MAPS` / `CHAINS`），字段语义与内存寄存器一一对应，Launcher 按同名字段注入即可，UI 逻辑无需改动。
3. **换算公式（已定案，勿改）**：
   - 格子 → 图上像素：`image_px = (x × S_w, (15 − y) × S_h)`（数据 (0,0) 在左下，垂直翻转）；
   - 城内多图：按 `index.json → city_maps[].members` 的锚点偏移 + 图内偏移换算整城连续坐标；
   - 世界前缀：`world === "xeen" ? 星云界 : 暗黑界`。
4. **地图资源**：小地图图片取 `retro/artwork/maps/`（单图 `cloudNNN.png` / 整城 `cloudNNN-city.png`），尺寸与 9×9 取景窗口的适配关系见 `#minimap` 的 `TILE_W/TILE_H` 常量。
5. **中键移动节奏**：实验场为 200ms 定时步进；真机应对齐游戏实际移动间隔，且移动前需确认引擎允许移动（事件格 / 墙体在真机由引擎判定，实验场只做边缘钳制）。
6. **状态存储**：任务勾选 / 置顶目前存 `localStorage["pl012-chains"]`（演示用）；正式版按既定方向改 **json 或 sqlite**，结构 = `{ 链id: { done: 阶段数, pinned: bool } }`。
7. **缩放方案**：整体 `transform: scale()` 等比缩放（`fitScale()`），Launcher 内嵌 WebView 同样适用；若 Tauri 窗口尺寸可控，可锁定 1920×1080 免缩放。

## 三、后端待补功能（对接时发现缺失）

| #   | 缺口                     | 说明                                                                              | 建议                             |
| --- | ------------------------ | --------------------------------------------------------------------------------- | -------------------------------- |
| 1   | **等级 LV 寄存器无映射** | 队伍槽要显示 LV.x；memory_map 109 条中无 level 条目                               | 破解地址后补 memory_map + 白名单 |
| 2   | **条件寄存器写权限**     | 复活 = 清 `death/unc/weak` 三寄存器（见下），monitor 白名单需放开这三个写入       | 沿用 PL007 write_memory 闸       |
| 3   | **HP/MP 作弊写入钳制**   | 写入值 = `min(当前 + 100, 200)`，200 为引擎安全阈值（防污染最大值，用户实测口径） | Launcher 侧做钳制后再写          |
| 4   | **中毒状态内存值**       | 状态头像规划含"中毒"，poison 条件寄存器未登记                                     | 破解后补 memory_map              |
| 5   | **状态头像套图提取**     | 昏迷 / 中毒 / 石化 / 死亡各一套头像未提取（现用框式占位）                         | 走 PL008 提取链路（CC 素材）     |
| 6   | **任务链数据源**         | 任务链（npc/xy/阶段/文案）需攻略 MD 结构化产出                                    | 与 xeen-guide.md 改造同批        |

## 四、前端数据来源清单（注入对照表）

| UI 元素              | 字段/占位                                         | 真机来源                                            |
| -------------------- | ------------------------------------------------- | --------------------------------------------------- |
| 日历：年份 / 第 N 天 | `CALENDAR.year / .day`                            | 寄存器 `year` @0x5AA2:0x6、`day` @0x5AA2:0x4（u16） |
| 昼夜 + 时刻          | `CALENDAR.minutes`                                | 寄存器 `time` @0x5AA2:0x8（<5:00 或 ≥21:00 为夜）   |
| 金币 / 宝石          | `RES.gold / .gems`                                | 寄存器 `gold`、`gems`（u32）                        |
| 食物 + 可维持天数    | `RES.food`                                        | 寄存器 `food`（u16）；天数 = ⌊food ÷ 在编人数⌋      |
| 队伍头像             | `PARTY[].face` → `retro/artwork/faces/charNN.png` | 寄存器 `face1–6` @0x5A7C                            |
| HP / MP              | `PARTY[].hp/.mhp/.mp/.mmp`                        | 寄存器 `hpN/mpN` 槽位成对 @0x56FA 系（u16）         |
| 等级 LV              | `PARTY[].lv`                                      | **无映射，地址待找**                                |
| 死亡 / 昏迷占位      | hp < 0 → 昏迷；hp ≤ −10 → 死亡                    | 寄存器 `deathN/uncN/weakN`（真机以条件寄存器驱动）  |
| 地图名 / 世界前缀    | `MAPS[].name` + `world`                           | index.json `maps[].name`、`env`、`side`             |
| 小地图               | `retro/artwork/maps/*-city.png / cloudNNN.png`    | index.json `city_maps`（城内连续坐标换算）          |
| 坐标 / 朝向          | `state.x/y/facing`                                | 寄存器 `pos_x/pos_y/dir`（HUD 五要素同源）          |
| 任务链               | `CHAINS[]`（npc/x/y/title/info/seq/world）        | 攻略 MD 结构化（待做）                              |
| CRT 屏画面           | `pl012-gameview-sample.png` 占位                  | GameCanvas 帧流                                     |

## 五、已移除文案归档（按分类）

### 系统使用说明（归入"使用说明"栏，未来浮窗展示）

> 点击任务框展开所在链的清单；📌 置顶一条链后无视规则置顶显示；勾选状态本地记录。

### 开发备注（数据源对照，已并入第四节表格）

> 数据源对照：头像 face1–6 @0x5A7C · HP/MP 槽位成对 @0x56FA 系 · year/day/time/food/gold/gems @0x5AA2–0x5AA4 段（等级 LV 无映射，地址待找；**职业/人种同样无寄存器映射**——PL014 徽章现消费实验场假数据，寄存器破解挂 PL015.2 后端缺口①）

## 六、未完成事项（已对齐、待排期）

1. **左栏底部操作栏**（✅ 已实装 2026-09-15）：队伍卡下方新增一行高操作栏——三个文字按钮 **魔法 / 注音 / 说明**，最右一个**设置齿轮**；点击任意按钮弹出**浮窗**（尺寸恰好盖住 CRT 显示器，JS 实时取 `#monitor` 盒定位），点击浮窗外任意位置关闭；齿轮打开的是**基本设置**浮窗（电源 / 扫描线 / 弧面反光与控制条镜像联动，状态真源 = body class；「背景跟随世界」为待实装占位）。注音数据与 `retro/zhuyin-keyboard.md` 同源；说明浮窗收录第五节归档文案。
2. **背景图按世界切换**（✅ 已完成 2026-09-16）：新增房间背景图层 `#bg-scene`（fixed、inset -40px 抵消模糊边缘、cover + 暗色晕影压字）。三幅候选：MM4-Clouds（星云界）/ MM5-Darkside（暗黑界）/ xeed16x9（XEEN 主视觉，三图均置于 assets/design）。基本设置浮窗内三图缩略一排 + 图下街机开关（[Nawsome · empty-snail-69](https://uiverse.io/Nawsome/empty-snail-69)，改类名 `.arc-switch` 避让翻牌开关，zoom 0.4）互斥可选、可全关；全关 = 跟随世界（xeen → MM4，dark → MM5，applyMap 时自动切换）。毛玻璃模糊滑杆 0–20px（默认 6px）实时可调。
3. **CRT 特效调研与实装**（✅ 调研完成 2026-09-16，归属定案：**前端侧，DOSBox 侧弃用**；✅ **实装完成 2026-10-06——六效果进实验场基本设置，默认全关，待用户目测定稿默认值**）。定案依据——读 fork 源码证实：帧流捕获点在 `opengl_renderer.cpp EndFrame()` 的 `CapsuleFrame::Capture(last_framebuf)`，即 DOS 输入帧缓冲（着色前、原始分辨率），而 DOSBox Staging 的 `[render] glshader` CRT shader 只作用于自身 OpenGL 窗口的最终绘制——**shader 对帧流完全无效**，且与模拟器静默后台化冲突，弃用（DOSBox 侧 glshader 实机对照验证按用户决定放弃，源码定案维持）。前端侧可选效果：暗角 vignette（radial-gradient 叠层）、RGB 荫罩（repeating-linear-gradient 三色竖条）、噪点（SVG feTurbulence 动画）、闪烁（亮度关键帧）、色偏（hue-rotate / 分层混合）、光晕 bloom 近似（画布副本 blur + screen 混合）；真 bloom / 桶形畸变需 WebGL 单 pass（参考 [crt-fx](https://github.com/stefanlegg/crt-fx)、[crt-royale](https://docs.libretro.com/shader/crt_royale/)），列为后续升级路径。升级路径授权核实（2026-09-16）：**crt-fx = MIT，可直接嵌入**帧流画布（WebGL 纹理逐帧渲染），为首选库；crt-royale = **GPL-2.0+**，闭源发行产品不可搬用其代码（仅作视觉调参参考），且其 libretro 桌面 GLSL 多 pass 管线与浏览器 WebGL 不直接兼容。调研来源：[DOSBox Staging Shaders 手册](https://www.dosbox-staging.org/0.83/manual/graphics/rendering/shaders/)、[dosbox-shaders 仓库](https://github.com/dosbox-staging/dosbox-shaders)。
   **实装记录（2026-10-06）**：调参试验台 `.temp/crt-playground.html`（四预设 + 按住对比，gitignore 不入库）目测调参后，六效果 + 扫描线间距参数化已实装进实验场「基本设置」浮窗——每行一行内 = 翻牌开关 + 效果名 + 滑杆 + 数值；**默认全关**，滑杆预填试验台标准档（暗角 45 / 荫罩 4px / 噪点 18 / 闪烁 35 / 色偏 1px / 泛光 35，扫描线间距默认 3px），开即见标准强度。状态真源 = body class（`vig-on` / `mask-on` / `noise-on` / `flicker-on` / `chroma-on` / `bloom-on`）+ CSS 变量（`--fx-vig-a` / `--fx-mask-p` / `--fx-noise-a` / `--fx-flicker-a` / `--fx-bloom-o` / `--fx-scan-p`）；层 z 序：泛光 z2（垫 loading z3 之下防开机透出模糊画面）、其余 z4 与扫描线同层按 DOM 序、玻璃 z5 最顶；色偏 = SVG 通道分离滤镜（`feOffset` dx 由滑杆驱动，R 右移 / B 左移）。**待办**：用户目测定稿各项默认值后回填本节并设为实验场默认。
4. **老板按钮（Boss Key）**（✅ 已完成 2026-10-07，独立立项实装）：原设想"播放 omarchy 正在 vibe coding 模拟动画"升级为 **CSS 复刻 omarchy 真机桌面**——`#boss-screen` 全屏 overlay（壁纸纯 CSS 含中央光柱、waybar 工作区+真实时钟+托盘、三窗平铺：fastfetch / launcher 真源码编辑器 / cargo 日志滚动流），**Ctrl+F10 同键切入切出**（零过渡即时切），操作栏「说明」旁新增**「老板」按钮**同款切换逻辑，点击桌面空白退出；色板与规格经参考图两轮探针实测（`.temp/pl012-spec.md`）。已收口并经用户判收，详见 x.progress 已完成区与 z.plan 附录 PL012；机制待 Launcher 对接期平移（Tauri WebView 同款 JS 热键）。
5. **UI 套件调研**（✅ 已完成 2026-09-16）：选定 [uiverse-io/galaxy](https://github.com/uiverse-io/galaxy)（12.1k star，**MIT**）为组件来源，纯 CSS 组件按"摘取 + 换肤"引入，Tailwind 版排除（不为此加依赖），组件头部注释登记来源与改色。已落位六个组件：**① Toggle-switches**（[r7chardgh · dry-rabbit-69](https://uiverse.io/r7chardgh/dry-rabbit-69)）——电源/扫描线/弧面反光与设置浮窗镜像，改色仅文字 off 红 / on 绿；**② Tooltip**（[TanimMahbub · white-fly-43](https://uiverse.io/TanimMahbub/white-fly-43) 改款，黑金配色）——全局 JS 单例 `#js-tip` 挂舞台内随 fitScale 缩放，mouseover 委托、换目标强制重播滑入，气泡距目标 7px、箭头嵌 3px 且经 `--ax/--ay` 指向目标中心、左右按所在框钳制，动效 = 滑入淡入 + 箭头 45°→0° 转正（A 方案）；铺满操作栏四按钮、六个演示按钮与任务链图钉（图钉文案随置顶切换 📌 置顶该链 / 📍 取消置顶）；**③ Checkbox**（[Gautammsharma · massive-rabbit-40](https://uiverse.io/Gautammsharma/massive-rabbit-40)）——任务勾选，打勾绿 #8fc46f，仍走 `.checked` class 与 data-stage 委托点击；**④ Button**（[vinodjangid07 · pink-cheetah-68](https://uiverse.io/vinodjangid07/pink-cheetah-68)）——操作栏三按钮与全部演示按钮，金渐变流光 + 按压缩放，暗色内芯以 z-index:-1 的 ::before 实现（按钮 isolation: isolate）；**动效升级（✅ 2026-10-07 收口，用户判收）**：流光渐变整体迁 ::after（z -2 压暗芯下），hover `translateX(-60%)` 与原 background-position 逐像素等价、动画走合成器线程；按压双曲线（:active 0.08s 快收 / 松手 0.25s 弹簧回弹 / scale 0.96）；标签文字 SVG 化（`svg.lbl` + `lbl-ghost` 隐形垫片保自适应宽，灰度 AA 恒定、对合成层升降免疫）+ `aria-label`；范围 = 操作栏四文字按钮（魔法/注音/说明/老板）+ cheats 六颗，齿轮款不涉及**⑤ Button 齿轮款**（[TroyRandall · tall-wolverine-43](https://uiverse.io/TroyRandall/tall-wolverine-43)，缩至 32×32）——基本设置，自带滑出式气泡（悬停按钮与气泡同转金渐变）；**⑥ Toggle switch 任务范围**（[santhosh_2608 · itchy-stingray-87](https://uiverse.io/santhosh_2608/itchy-stingray-87)，整体验缩至 0.5）——文字 MAP（当前地图）/ WORLD（同世界全部），checkbox 原生 checked 驱动 `state.view`；**⑦ Loaders 组合**（[BlackisPlay · sweet-frog-67](https://uiverse.io/BlackisPlay/sweet-frog-67) 吃豆人幽灵 + [f754699 · witty-duck-39](https://uiverse.io/f754699/witty-duck-39) LOADING 故障字）——CRT 启动 loading，`#loading-screen` 置于 `#screen-wrap` 内（z-index 3，盖过帧流、垫在扫描线/玻璃下），flex 居中，`.show` 不淡入防闪帧，电源开机触发 2 秒。候选留档：个人卡片式 tooltip（[vikas7754 · evil-mule-52](https://uiverse.io/vikas7754/evil-mule-52)）作队伍成员 hover 角色卡。
6. **职业徽章**：队伍槽的职业现为纯文字，改为徽章式样（图形 + 底色）提升观感。
7. **小地图移动增强**：中键**按住并拖动**时，队伍位置图标要随地图整体联动（全向丝滑移动，非仅南向步进）；地图与取景框贴边后该方向停止移动（钳制已有，需补拖拽联动与平滑插值）。

---

_交接完毕。实验场继续作为 PL015 的评审媒介使用：改样式 → 刷新目测 → 定稿后按本文档对接 Launcher。_
