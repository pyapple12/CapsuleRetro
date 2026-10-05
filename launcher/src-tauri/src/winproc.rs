//! Win32 进程创建封装（CreateProcessW 直写，替代 `std::process::Command`）。
//!
//! 设计理由：`CREATE_SUSPENDED` 创建 → 挂 Job → `ResumeThread` 恢复，天然消除
//! "子进程在挂 Job 前繁衍孙进程"的竞态；stdio 重定向走可继承日志句柄
//! （`STARTF_USESTDHANDLES`）。安全边界：命令行参数全部来自仓库内
//! `retro/game.json` 与 u16 端口探测值，无外部输入参与。

use std::path::Path;

use windows::core::{Result as WinResult, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, SetHandleInformation, DUPLICATE_SAME_ACCESS, HANDLE,
    HANDLE_FLAG_INHERIT, WAIT_TIMEOUT,
};
use windows::Win32::System::Threading::{
    CreateProcessW, GetCurrentProcess, ResumeThread, TerminateProcess, WaitForSingleObject,
    CREATE_NO_WINDOW, CREATE_SUSPENDED, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
};

/// 可跨线程传递的 Win32 句柄包装。
///
/// # Safety
/// HANDLE 是进程级不透明 ID（非线程仿射），本封装只用于 wait/terminate/
/// Job 挂载等进程级操作，跨线程传递安全。
pub struct SendHandle(pub HANDLE);

// SAFETY：见类型注释。
unsafe impl Send for SendHandle {}
// SAFETY：见类型注释。
unsafe impl Sync for SendHandle {}

/// 已创建（初始挂起）的 Win32 进程句柄封装。
pub struct WinProcess {
    process: SendHandle,
    thread: SendHandle,
    pid: u32,
}

/// 构造以引号包裹的命令行段（参数与路径都不含内嵌引号，简单包裹即安全）。
fn quoted(s: &str) -> String {
    format!("\"{s}\"")
}

/// 拼接完整命令行：exe 与 args 各段统一引号包裹（空格路径否则被截断）。
fn build_cmdline(exe: &Path, args: &[String]) -> String {
    let mut cmdline_text = quoted(&exe.to_string_lossy());
    for arg in args {
        cmdline_text.push(' ');
        cmdline_text.push_str(&quoted(arg));
    }
    cmdline_text
}

impl WinProcess {
    /// 以挂起状态创建进程（`CREATE_SUSPENDED`），stdio 重定向到传入的可继承句柄。
    ///
    /// # 参数
    /// - `exe`：可执行文件路径
    /// - `args`：参数列表（各段自动引号包裹）
    /// - `stdout` / `stderr`：已设为可继承的日志句柄
    ///
    /// # Errors
    /// Win32 调用失败时返回 `windows::core::Error`。
    pub fn spawn_suspended(
        exe: &Path,
        args: &[String],
        stdout: HANDLE,
        stderr: HANDLE,
    ) -> WinResult<Self> {
        let mut cmdline: Vec<u16> = build_cmdline(exe, args)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let appname: Vec<u16> = exe
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain([0])
            .collect();

        let si = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            hStdInput: HANDLE::default(),
            hStdOutput: stdout,
            hStdError: stderr,
            dwFlags: STARTF_USESTDHANDLES,
            ..STARTUPINFOW::default()
        };

        let mut pi = PROCESS_INFORMATION::default();
        unsafe {
            CreateProcessW(
                PCWSTR(appname.as_ptr()),
                Some(PWSTR(cmdline.as_mut_ptr())),
                None,
                None,
                true,
                // CREATE_NO_WINDOW：dosbox 是控制台程序，不加此 flag 会弹出
                // 可见的 console 窗口（PL005 静默启动收尾）；stdout/stderr 仍
                // 经 STARTF_USESTDHANDLES 重定向到 dosbox.log
                CREATE_SUSPENDED | CREATE_NO_WINDOW,
                None,
                PCWSTR::null(),
                &si,
                &mut pi,
            )?;
        }
        Ok(Self {
            process: SendHandle(pi.hProcess),
            thread: SendHandle(pi.hThread),
            pid: pi.dwProcessId,
        })
    }

    /// 恢复挂起的进程主线程。
    ///
    /// # Errors
    /// `ResumeThread` 返回 `u32::MAX`（恢复失败，进程将永久挂起）时返回错误。
    pub fn resume(&self) -> WinResult<()> {
        let prev = unsafe { ResumeThread(self.thread.0) };
        if prev == u32::MAX {
            return Err(windows::core::Error::from_win32());
        }
        Ok(())
    }

    /// 进程句柄（供 Job Object 挂载）。
    pub fn handle(&self) -> HANDLE {
        self.process.0
    }

    /// 进程 PID。
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// 进程是否仍在运行（零超时探测）。
    pub fn is_running(&self) -> bool {
        unsafe { WaitForSingleObject(self.process.0, 0) == WAIT_TIMEOUT }
    }

    /// 等待进程退出；返回是否在超时前退出。
    pub fn wait_exit(&self, timeout_ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.process.0, timeout_ms) != WAIT_TIMEOUT }
    }

    /// 终止进程（退出码 1）；用于 Job 挂载失败等异常路径的显式清理，
    /// 防止挂起状态的进程残留成无人认领的僵尸。
    ///
    /// # Errors
    /// `TerminateProcess` 失败时返回 `windows::core::Error`。
    pub fn terminate(&self) -> WinResult<()> {
        unsafe { TerminateProcess(self.process.0, 1).map(|_| ()) }
    }

    /// 复制进程句柄（`DUPLICATE_SAME_ACCESS`），供退出监听线程独立持有；
    /// 副本由调用方负责 `CloseHandle` 释放。
    ///
    /// # Errors
    /// `DuplicateHandle` 失败时返回 `windows::core::Error`。
    pub fn duplicate_process_handle(&self) -> WinResult<HANDLE> {
        let mut dup = HANDLE::default();
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.process.0,
                GetCurrentProcess(),
                &mut dup,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )?;
        }
        Ok(dup)
    }
}

/// 监听线程专用：无限等待句柄触发（进程退出）后关闭句柄副本。
/// 以 `SendHandle` 整体传参，避免调用点精确捕获裸 HANDLE 字段破坏 Send。
pub fn wait_forever_and_close(handle: SendHandle) {
    unsafe {
        WaitForSingleObject(handle.0, u32::MAX);
        let _ = CloseHandle(handle.0);
    }
}

impl Drop for WinProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.thread.0);
            let _ = CloseHandle(self.process.0);
        }
    }
}

/// 把日志句柄标记为可继承（子进程 stdio 重定向的前提）。
pub fn make_inheritable(handle: HANDLE) -> WinResult<()> {
    unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证含空格的路径与参数在命令行中被引号包裹（Windows 空格语义）。
    #[test]
    fn cmdline_args_are_quoted() {
        // 含空格的 conf 路径（真实场景：C:\Users\<First Last>\AppData\...）
        let exe = Path::new("C:\\dosbox\\dosbox.exe");
        let args = vec![String::from("C:\\Users\\John Smith\\conf\\dosbox.conf")];
        let line = build_cmdline(exe, &args);
        assert!(
            line.contains("\"C:\\Users\\John Smith\\conf\\dosbox.conf\""),
            "参数段必须引号包裹，实际：{line}"
        );
    }

    /// 验证非 BMP 字符（emoji）经 UTF-16 编码往返不丢失（旧 u16 截断实现的回归锚）。
    #[test]
    fn cmdline_roundtrip_via_utf16() {
        // 非 BMP 字符（emoji）经 encode_utf16 不丢信息；旧实现 c as u16 会截断
        let exe = Path::new("C:\\游戏🎮\\dosbox.exe");
        let args: Vec<String> = vec![];
        let line = build_cmdline(exe, &args);
        let utf16: Vec<u16> = line.encode_utf16().collect();
        let decoded: String = String::from_utf16(&utf16).expect("UTF-16 必须合法");
        assert_eq!(decoded, line);
        assert!(decoded.contains('\u{1F3AE}'));
    }
}
