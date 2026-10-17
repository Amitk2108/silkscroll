//! Finds out which app a wheel event is aimed at, and whether we may touch it.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromWindow};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOT, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW, GetDesktopWindow,
    GetForegroundWindow, GetShellWindow, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW,
    GetWindowThreadProcessId, IsWindowVisible, IsZoomed, WS_CAPTION, WS_EX_TOOLWINDOW, WindowFromPoint,
};

use crate::win::exe_path;

pub struct AppInfo {
    /// Lowercase file name such as `chrome.exe`; empty if unknown.
    pub exe: String,
    /// False when Windows would silently drop the scroll events we inject,
    /// which happens for apps running as administrator while we are not.
    pub can_inject: bool,
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn open_process(pid: u32) -> Option<ProcessHandle> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    (!handle.is_null()).then_some(ProcessHandle(handle))
}

fn image_path(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let token = ProcessHandle(token);
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0;
        let ok = GetTokenInformation(
            token.0,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        (ok != 0).then_some(elevation.TokenIsElevated != 0)
    }
}

pub fn self_elevated() -> bool {
    static ELEVATED: OnceLock<bool> = OnceLock::new();
    *ELEVATED.get_or_init(|| token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false))
}

pub fn exe_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase()
}

fn own_exe() -> &'static str {
    static OWN: OnceLock<String> = OnceLock::new();
    OWN.get_or_init(|| exe_name(&exe_path().to_string_lossy()))
}

pub fn process_path(pid: u32) -> Option<String> {
    open_process(pid).and_then(|p| image_path(p.0))
}

pub fn app_info(pid: u32) -> AppInfo {
    let Some(process) = open_process(pid) else {
        return AppInfo { exe: String::new(), can_inject: self_elevated() };
    };
    let exe = image_path(process.0).map(|p| exe_name(&p)).unwrap_or_default();
    let can_inject = self_elevated() || token_elevated(process.0) == Some(false);
    AppInfo { exe, can_inject }
}

pub fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

/// The top-level window under a screen point.
pub fn root_window_at(pt: POINT) -> Option<HWND> {
    unsafe {
        let hwnd = WindowFromPoint(pt);
        if hwnd.is_null() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        Some(if root.is_null() { hwnd } else { root })
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

/// A borderless window covering its whole monitor: almost always a game or
/// a video player rather than a document you'd scroll through.
pub fn is_fullscreen(hwnd: HWND) -> bool {
    unsafe {
        if hwnd == GetShellWindow() || hwnd == GetDesktopWindow() || IsZoomed(hwnd) != 0 {
            return false;
        }
        if matches!(class_name(hwnd).as_str(), "WorkerW" | "Progman") {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if style & WS_CAPTION == WS_CAPTION {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return false;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
        if monitor.is_null() {
            return false;
        }
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return false;
        }
        let m = info.rcMonitor;
        rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
    }
}

/// The app the user is working in, as `(exe, full path)`, ignoring the
/// taskbar, desktop, menus and SilkScroll itself. The tray menu uses it for
/// "Disable for <app>".
pub fn foreground_app() -> Option<(String, String)> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    let class = class_name(hwnd);
    if matches!(
        class.as_str(),
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "Progman"
            | "WorkerW"
            | "#32768"
    ) {
        return None;
    }
    let path = process_path(pid_of(hwnd))?;
    let exe = exe_name(&path);
    (exe != own_exe()).then_some((exe, path))
}

/// Apps with a visible window right now, as `(exe, full path)`, sorted.
pub fn running_apps() -> Vec<(String, String)> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
        let windows = unsafe { &mut *(lparam as *mut Vec<HWND>) };
        windows.push(hwnd);
        1
    }
    let mut windows: Vec<HWND> = Vec::new();
    unsafe { EnumWindows(Some(collect), &mut windows as *mut Vec<HWND> as LPARAM) };

    let mut apps: Vec<(String, String)> = Vec::new();
    for hwnd in windows {
        unsafe {
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            if IsWindowVisible(hwnd) == 0
                || GetWindowTextLengthW(hwnd) == 0
                || !GetWindow(hwnd, GW_OWNER).is_null()
                || ex_style & WS_EX_TOOLWINDOW != 0
            {
                continue;
            }
        }
        let Some(path) = process_path(pid_of(hwnd)) else { continue };
        let exe = exe_name(&path);
        if exe != own_exe() && !apps.iter().any(|(e, _)| *e == exe) {
            apps.push((exe, path));
        }
    }
    apps.sort();
    apps
}

/// Remembers process lookups for a few seconds; the mouse hook runs for every
/// wheel event and must stay fast.
#[derive(Default)]
pub struct AppCache {
    entries: HashMap<u32, (Rc<AppInfo>, Instant)>,
}

impl AppCache {
    const TTL: Duration = Duration::from_secs(5);

    pub fn get(&mut self, pid: u32) -> Rc<AppInfo> {
        let now = Instant::now();
        if let Some((info, at)) = self.entries.get(&pid) {
            if now.duration_since(*at) < Self::TTL {
                return info.clone();
            }
        }
        if self.entries.len() > 256 {
            self.entries.clear();
        }
        let info = Rc::new(app_info(pid));
        self.entries.insert(pid, (info.clone(), now));
        info
    }
}
