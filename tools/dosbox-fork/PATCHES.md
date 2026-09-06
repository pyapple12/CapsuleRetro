# DosboxCapsule 补丁登记与升级配方

> CapsuleRetro 维护档案：DosboxCapsule（独立公开仓，承载 dosbox-staging 代码基）
> 的补丁清单、提交纪律与升级操作规程。仓库 = `pyapple12/DosboxCapsule`，
> 本地工作树 = `E:\GameMission\DosboxCapsule`，upstream remote 指官方（拉 tag 升级用）。

DosboxCapsule 承载 [dosbox-staging](https://github.com/dosbox-staging/dosbox-staging)
的代码基线（**v0.83.0** 起步），作为 CapsuleRetro（老 DOS 游戏现代包装系统）的模拟器
组件独立演进。源码遵循上游许可证 **GPL-2.0-or-later**，分发修改版时随包提供完整源码。
上游血统与许可声明见仓库根 `FORK.md`。

## 分支模型

独立单分支模型，基线锚点用 **tag** 而非分支：

| 引用          | 内容                            | 用途                                                |
| ------------- | ------------------------------- | --------------------------------------------------- |
| `main`        | **v0.83.0 + CapsuleRetro 补丁** | 唯一分支、默认分支，只收代码补丁，逐主题提交        |
| tag `v0.83.0` | 上游发布基线（7b40053b7）       | 基线锚点：`git diff v0.83.0..main` 即 fork 全部足迹 |

基线永远是上游发布 tag（当前 **v0.83.0**），不追上游 main。文档与工具集中在
本目录（CapsuleRetro 仓），不进 DosboxCapsule——main 的 diff 保持 100% 代码补丁。

## 提交纪律

- **一个 commit 一个主题**：每条补丁只做一件事，标题 `主题: 动作`（英文，
  对齐上游惯例），diff 最小化
- **补丁必须可独立 rebase**：升级 = `main` rebase 到新 tag，逐提交重放、
  逐冲突解决；大杂烩提交是升级毒药，禁止
- **每次升级后重跑基线判定**（FB1：92 条目内存档案与官方发布版零偏差），
  基线不过不发版

## 升级配方（依样画葫芦）

1. `git fetch upstream --tags` → 确认新版本 tag
2. `git rebase --onto vX.Y.Z v0.83.0 main`（基线换轴，补丁逐个重放）
3. 清洁重建：本目录 `rebuild.bat`（删 build 全新配置编译）
4. 重跑基线判定：本目录 `baseline-capture.sh`（官方版/候选版各采两份 1MB
   快照）+ `baseline-compare.py`（92 条目零偏差判定）
5. 更新本文件补丁清单与基线 tag，结果登记 z.plan 实测结论

## 清洁重建

`rebuild.bat`：清空 `build/` 后全新配置编译（构建链 = VS Build Tools 2026
ClangCL 工具集 + vcpkg manifest，与上游官方 Windows CI 同路线；vcpkg 位于
`E:\GameMission\tools\vcpkg`，机器相关路径见脚本内注释）。核心命令：

```bash
cmake -S . -B build/release-windows -G "Visual Studio 18 2026" -A x64 \
  -DCMAKE_GENERATOR_TOOLSET=ClangCL \
  "-DCMAKE_GENERATOR_INSTANCE=C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools" \
  -DCMAKE_TOOLCHAIN_FILE=<VCPKG_ROOT>/scripts/buildsystems/vcpkg.cmake \
  -DCMAKE_FIND_PACKAGE_PREFER_CONFIG=ON -DCMAKE_BUILD_TYPE=Release
cmake --build build/release-windows --config Release
```

产物：`build/release-windows/Release/dosbox.exe`

## 补丁清单

基线：**v0.83.0**（7b40053b7）｜状态：**3 条补丁**

| #   | Commit 主题                                                   | 文件                                                                                                                                                      | 用途                                                                                                                       | 原因                                                                            |
| --- | ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| 1   | `a8e395cbf` webserver: Add /api/v2/frame stream endpoint      | `src/gui/capsule_frame.{h,cpp}`、`src/gui/render/{sdl,opengl}_renderer.cpp`、`src/webserver/frame.{h,cpp}`、`src/webserver/webserver.cpp`、两模块构建文件 | 帧流端点：渲染 EndFrame 捕获最新 DOS 帧 → `GET /api/v2/frame`（header 元数据 + BGRX body，503 先于首帧）                   | 统一窗口主线：launcher 画布吃帧流成像，替代游戏窗口可见性（PL002.23，FF1 已过） |
| 2   | `5019a886a` input: Add /api/v2/input/event injection endpoint | `src/webserver/input.{h,cpp}`、`src/webserver/webserver.cpp`、两模块构建文件                                                                              | 输入注入端点：键盘经 Bridge+MAPPER_CheckEvent（模拟线程，与真实事件泵同路），鼠标经 SDL_PushEvent；键名表 62 键 + 6 修饰键 | 统一窗口画布的输入转发通道（PL002.25，I1/I2 已过；协议事实：组合键须时序注入）  |
| 3   | `adc68e3a2` webserver: Map client errors to HTTP 400          | `src/webserver/webserver.cpp`                                                                                                                             | error_handler 将 `invalid_argument`/`out_of_range` 映射 400，区分客户端错误与服务端 500                                    | 轮询方据状态码决定重试策略；非法地址/键名不崩溃不误报（PL002.26，M1/M3 已过）   |
