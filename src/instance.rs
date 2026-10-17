//! Single-instance locks and the few messages the processes send each other.
//!
//! The engine (tray + scrolling) and the settings window are two copies of the
//! same exe. They find each other by window class name.

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, PostMessageW, SW_RESTORE, SetForegroundWindow, ShowWindow, WM_APP, WM_CLOSE,
};

use crate::target::{pid_of, process_path};
use crate::win::{exe_path, wide};

pub const ENGINE_MUTEX: &str = "Local\\SilkScroll.Engine";
pub const SETTINGS_MUTEX: &str = "Local\\SilkScroll.Settings";
pub const TRAY_CLASS: &str = "SilkScroll.Tray";
pub const SETTINGS_CLASS: &str = "SilkScroll.Settings";

/// Sent to the tray window: the config file changed, reload it now.
pub const WM_APP_RELOAD: u32 = WM_APP + 10;
/// Sent to the tray window: exit the engine.
pub const WM_APP_QUIT: u32 = WM_APP + 11;

/// Holds a named mutex for as long as it lives.
pub struct Guard(HANDLE);

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Takes the named lock, or returns `None` if another process holds it.
pub fn acquire(name: &str) -> Option<Guard> {
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, wide(name).as_ptr());
        if handle.is_null() {
            return None;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(handle);
            return None;
        }
        Some(Guard(handle))
    }
}

/// Like [`acquire`], but keeps trying for up to `timeout` while another
/// process is shutting down.
pub fn acquire_within(name: &str, timeout: Duration) -> Option<Guard> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(guard) = acquire(name) {
            return Some(guard);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn is_running(name: &str) -> bool {
    unsafe {
        let handle = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, wide(name).as_ptr());
        if handle.is_null() {
            false
        } else {
            CloseHandle(handle);
            true
        }
    }
}

/// Waits until nobody holds the named lock any more.
fn wait_until_free(name: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while is_running(name) {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

pub fn notify_engine(msg: u32) -> bool {
    unsafe {
        let hwnd = FindWindowW(wide(TRAY_CLASS).as_ptr(), std::ptr::null());
        !hwnd.is_null() && PostMessageW(hwnd, msg, 0, 0) != 0
    }
}

/// True when the running engine is a different exe than this one, for
/// example an older version saved in another folder or under another name.
pub fn engine_is_other_copy() -> bool {
    let hwnd = unsafe { FindWindowW(wide(TRAY_CLASS).as_ptr(), std::ptr::null()) };
    if hwnd.is_null() {
        return false;
    }
    let Some(running) = process_path(pid_of(hwnd)) else { return false };
    !running.eq_ignore_ascii_case(&exe_path().to_string_lossy())
}

/// Stops every running SilkScroll process (engine and settings window) and
/// waits for them to exit, so a new version or an installer can take over.
pub fn quit_all(timeout: Duration) -> bool {
    notify_engine(WM_APP_QUIT);
    unsafe {
        let settings = FindWindowW(wide(SETTINGS_CLASS).as_ptr(), std::ptr::null());
        if !settings.is_null() {
            PostMessageW(settings, WM_CLOSE, 0, 0);
        }
    }
    let engine_gone = wait_until_free(ENGINE_MUTEX, timeout);
    let settings_gone = wait_until_free(SETTINGS_MUTEX, timeout);
    engine_gone && settings_gone
}

/// Brings an already open settings window to the front.
pub fn focus_settings() -> bool {
    unsafe {
        let hwnd = FindWindowW(wide(SETTINGS_CLASS).as_ptr(), std::ptr::null());
        if hwnd.is_null() {
            return false;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
        true
    }
}
