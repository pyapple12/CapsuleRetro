//! Windows Job Object：把 DOSBox 子进程挂进 Job，`KILL_ON_JOB_CLOSE` 保证
//! launcher 进程退出（含崩溃）时游戏进程树必死——VB2 的机制保障。
//! 关联：windows crate（Win32_System_JobObjects），句柄随 launcher 存续不主动关闭。

use windows::core::{Result as WinResult, PCWSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

use crate::winproc::SendHandle;

/// 游戏 Job 句柄（RAII：launcher 存续期内不关闭句柄，进程退出时 OS 回收并杀树）。
pub struct JobHandle {
    handle: SendHandle,
}

impl JobHandle {
    /// 创建启用 `KILL_ON_JOB_CLOSE` 的 Job。
    ///
    /// # Errors
    /// Job 对象创建或扩展限制信息设置失败时返回 Windows 错误。
    pub fn create() -> WinResult<Self> {
        unsafe {
            let handle = CreateJobObjectW(None, PCWSTR::null())?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            Ok(Self {
                handle: SendHandle(handle),
            })
        }
    }

    /// 把进程句柄挂进 Job（配合挂起创建：先挂 Job 再恢复执行）。
    ///
    /// # Errors
    /// 进程未挂进 Job（句柄非法/权限不足等）时返回 Windows 错误。
    pub fn assign(&self, process: HANDLE) -> WinResult<()> {
        unsafe { AssignProcessToJobObject(self.handle.0, process) }
    }

    /// 终止 Job 内全部进程（stop_game 用）。
    ///
    /// # Errors
    /// 终止调用失败（Job 句柄无效等）时返回 Windows 错误——调用方须留痕，
    /// 此时崩溃兜底杀树未生效（FIX005.9）。
    pub fn kill(&self) -> WinResult<()> {
        unsafe { TerminateJobObject(self.handle.0, 0) }
    }
}
