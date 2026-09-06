@echo off
rem DosboxCapsule 清洁重建（升级 rebase 后必跑，见同目录 PATCHES.md 升级配方）。
rem 清空 build 后全新配置编译，证明源码树自包含、重建可重复（FB2）。
rem 机器相关路径：vcpkg 在 E:\GameMission\tools\vcpkg（换机改 VCPKG_ROOT）；
rem VS 实例钉死 BuildTools——本机存在 Insiders 多实例，不钉会被 CMake 误选
rem （Insiders 的 MSBuild 缺 VC 工具集，报 MSB8020）。
rem PYTHONUTF8 清空：本会话环境变量若为 1，python 子进程走 UTF-8 严格解码，
rem 中文 Windows 的 MSVC 工具 GBK 输出会炸（历史教训，留作环境疫苗）。
set PYTHONUTF8=
set HTTP_PROXY=socks5://127.0.0.1:10808
set HTTPS_PROXY=socks5://127.0.0.1:10808
set ALL_PROXY=socks5://127.0.0.1:10808
set UseMultiToolTask=true
set EnforceProcessCountAcrossBuilds=true
set VCPKG_ROOT=E:\GameMission\tools\vcpkg
if exist E:\GameMission\DosboxCapsule\build rmdir /s /q E:\GameMission\DosboxCapsule\build
cd /d E:\GameMission\DosboxCapsule
cmake -S . -B build/release-windows -G "Visual Studio 18 2026" -A x64 ^
  -DCMAKE_GENERATOR_TOOLSET=ClangCL ^
  "-DCMAKE_GENERATOR_INSTANCE=C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools" ^
  -DCMAKE_TOOLCHAIN_FILE=%VCPKG_ROOT%\scripts\buildsystems\vcpkg.cmake ^
  -DCMAKE_FIND_PACKAGE_PREFER_CONFIG=ON ^
  -DCMAKE_BUILD_TYPE=Release
if errorlevel 1 exit /b 1
cmake --build build/release-windows --config Release
echo REBUILD-EXIT:%ERRORLEVEL%
