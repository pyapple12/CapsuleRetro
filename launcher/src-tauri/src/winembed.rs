//! 窗口离屏托管（PL002.27）：把游戏窗口移出屏幕并摘除任务栏条目。
//!
//! 设计理由：统一窗口形态下，显示走帧流端点、输入走注入端点，游戏窗口本体
//! 只需保持"非最小化存在"（最小化会挂起 SDL 渲染路径）。移出屏幕保证用户
//! 看不见；`WS_EX_TOOLWINDOW` 摘除任务栏按钮（UW6：任务栏只有 launcher 一条）。
//! 客观判据：窗口 rect 的 x ≤ -3200 且扩展样式含 TOOLWINDOW。

use windows::core::Result as WinResult;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindowVisible, SetWindowLongPtrW,
    SetWindowPos, GWL_EXSTYLE, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    WS_EX_TOOLWINDOW,
};

/// 屏幕外横坐标（覆盖常见多屏负坐标区，-3200 足够深且不触发最小化）。
const OFFSCREEN_X: i32 = -3200;

/// 按 PID 收集可见顶层窗口的枚举上下文。
struct WindowFinder {
    pid: u32,
    found: Vec<HWND>,
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let finder = unsafe { &mut *(lparam.0 as *mut WindowFinder) };
    let mut window_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
    }
    if window_pid == finder.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
        finder.found.push(hwnd);
    }
    BOOL(1)
}

/// 把指定 PID 的全部可见顶层窗口移出屏幕并摘除任务栏条目。
///
/// # Errors
/// 枚举不到任何窗口、或任一窗口操作失败时返回 `windows::core::Error`。
pub fn embed_offscreen(pid: u32) -> WinResult<()> {
    let mut finder = WindowFinder {
        pid,
        found: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(enum_callback), LPARAM(&mut finder as *mut _ as isize))?;
    }
    if finder.found.is_empty() {
        // 窗口可能尚未创建完成；调用方在会话启动后稍作等待再调用
        return Err(windows::core::Error::from_win32());
    }
    for hwnd in &finder.found {
        unsafe {
            let ex_style = GetWindowLongPtrW(*hwnd, GWL_EXSTYLE);
            let prev =
                SetWindowLongPtrW(*hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW.0 as isize);
            // 返回值检查（FIX001.38）：失败返回 0 且置最后错误；先前样式恰为
            // 0 时 from_win32 的 code 也是 0，误报无害（样式本就为空）
            if prev == 0 {
                let err = windows::core::Error::from_win32();
                if err.code() != windows::core::HRESULT(0) {
                    return Err(err);
                }
            }
            SetWindowPos(
                *hwnd,
                Some(HWND_BOTTOM),
                OFFSCREEN_X,
                0,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
        }
    }
    Ok(())
}
