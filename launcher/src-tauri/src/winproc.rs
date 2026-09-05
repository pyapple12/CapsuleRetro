//! Win32 进程创建封装（CreateProcessW 直写，替代 `std::process::Command`）。
//!
//! 设计理由：`CREATE_SUSPENDED` 创建 → 挂 Job → `ResumeThread` 恢复，天然消除
//! "子进程在挂 Job 前繁衍孙进程"的竞态；stdio 重定向走可继承日志句柄
//! （`STARTF_USESTDHANDLES`）。安全边界：命令行参数全部来自仓库内
//! `retro/game.json` 与 u16 端口探测值，无外部输入参与。

use std::path::Path;

use windows::core::{Result as WinResult, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_TIMEOUT,
};
use windows::Win32::System::Threading::{
    CreateProcessW, ResumeThread, WaitForSingleObject, CREATE_SUSPENDED, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOW,
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
        let mut cmdline_text = quoted(&exe.to_string_lossy());
        for arg in args {
            cmdline_text.push(' ');
            cmdline_text.push_str(arg);
        }
        let mut cmdline: Vec<u16> = cmdline_text
            .chars()
            .map(|c| c as u16)
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
                CREATE_SUSPENDED,
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
    pub fn resume(&self) -> WinResult<()> {
        unsafe {
            ResumeThread(self.thread.0);
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
