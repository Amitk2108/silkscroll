//! The engine process's tray icon, its menu, and config reloads.
//!
//! Left-click opens the settings window. Right-click shows the menu, which can
//! switch smoothing off for the app you were just using or for every app.

use std::cell::RefCell;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetMessageW, GetSystemMetrics, HICON, HMENU, MB_ICONINFORMATION, MB_OK, MF_CHECKED, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, MSG, PBT_APMRESUMEAUTOMATIC, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterWindowMessageW, SM_CXSMICON, SetForegroundWindow, SetMenuDefaultItem, SetTimer, TPM_BOTTOMALIGN,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY,
    WM_LBUTTONUP, WM_NULL, WM_POWERBROADCAST, WM_RBUTTONUP, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_OVERLAPPED,
};

use crate::config::{self, Config};
use crate::engine::{self, Engine};
use crate::instance::{TRAY_CLASS, WM_APP_QUIT, WM_APP_RELOAD};
use crate::win::{hinstance, make_icon, message_box, shell_open, spawn_self, wide};
use crate::{autostart, target};

const WM_TRAY: u32 = WM_APP + 1;
const TIMER_POLL: usize = 1;
const TIMER_REHOOK: usize = 2;
const WTS_SESSION_UNLOCK: usize = 0x8;

const ID_SETTINGS: usize = 10;
const ID_TOGGLE_APP: usize = 11;
const ID_TOGGLE_ALL: usize = 12;
const ID_AUTOSTART: usize = 13;
const ID_FOLDER: usize = 14;
const ID_ABOUT: usize = 15;
const ID_EXIT: usize = 16;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Tray {
    hwnd: HWND,
    engine: Engine,
    cfg: Config,
    cfg_time: Option<SystemTime>,
    icon_on: HICON,
    icon_off: HICON,
    icon_shown: bool,
    /// The last app the user worked in, as `(exe, path)`, for
    /// "Disable for <app>".
    last_app: Option<(String, String)>,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// Runs `f` on the tray state, unless it is already in use further up the
/// stack (a menu or message box pumping messages); then the message is
/// simply skipped.
fn with_tray<R>(f: impl FnOnce(&mut Tray) -> R) -> Option<R> {
    TRAY.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

pub fn run(engine: Engine, cfg: Config) {
    unsafe {
        let class = wide(TRAY_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance(),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);

        // A real (never shown) top-level window, because message-only
        // windows don't get the TaskbarCreated broadcast.
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("SilkScroll").as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinstance(),
            std::ptr::null(),
        );

        let icon_size = GetSystemMetrics(SM_CXSMICON).max(16) as u32;
        let first_run = !cfg.welcome_shown;
        let tray = Tray {
            hwnd,
            engine,
            cfg_time: config::modified_time(),
            cfg,
            icon_on: make_icon(icon_size, true),
            icon_off: make_icon(icon_size, false),
            icon_shown: false,
            last_app: None,
        };
        TRAY.with(|cell| *cell.borrow_mut() = Some(tray));
        with_tray(|t| t.apply());

        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        SetTimer(hwnd, TIMER_POLL, 500, None);
        SetTimer(hwnd, TIMER_REHOOK, 5 * 60 * 1000, None);

        if first_run {
            with_tray(|t| {
                t.cfg.welcome_shown = true;
                t.save();
                t.balloon("SilkScroll is running", "Click the tray icon any time to open the settings.");
            });
            spawn_self(Some("--settings"));
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

impl Tray {
    fn apply(&mut self) {
        engine::set_config(&self.cfg);
        autostart::sync(self.cfg.start_with_windows);
        self.update_icon();
    }

    fn reload(&mut self, force: bool) {
        let time = config::modified_time();
        if force || time != self.cfg_time {
            self.cfg_time = time;
            if let Ok(cfg) = config::read() {
                self.cfg = cfg;
                self.apply();
            }
        }
    }

    fn save(&mut self) {
        let _ = config::save(&self.cfg);
        self.cfg_time = config::modified_time();
        self.apply();
    }

    fn icon_data(&self) -> NOTIFYICONDATAW {
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        data.hIcon = if self.cfg.enabled { self.icon_on } else { self.icon_off };
        let tip = if self.cfg.enabled { "SilkScroll" } else { "SilkScroll (off for all apps)" };
        copy_into(&mut data.szTip, tip);
        data
    }

    fn update_icon(&mut self) {
        let data = self.icon_data();
        unsafe {
            if self.cfg.show_tray_icon {
                if self.icon_shown {
                    Shell_NotifyIconW(NIM_MODIFY, &data);
                } else {
                    self.icon_shown = Shell_NotifyIconW(NIM_ADD, &data) != 0;
                }
            } else if self.icon_shown {
                Shell_NotifyIconW(NIM_DELETE, &data);
                self.icon_shown = false;
            }
        }
    }

    fn remove_icon(&mut self) {
        if self.icon_shown {
            let data = self.icon_data();
            unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
            self.icon_shown = false;
        }
    }

    fn balloon(&self, title: &str, text: &str) {
        if !self.icon_shown {
            return;
        }
        let mut data = self.icon_data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_INFO;
        copy_into(&mut data.szInfoTitle, title);
        copy_into(&mut data.szInfo, text);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }

    fn track_foreground(&mut self) {
        if let Some(app) = target::foreground_app() {
            self.last_app = Some(app);
        }
    }

    fn build_menu(&self) -> HMENU {
        unsafe {
            let menu = CreatePopupMenu();
            let add = |flags: u32, id: usize, text: &str| {
                AppendMenuW(menu, flags, id, wide(text).as_ptr());
            };
            let check = |on: bool| if on { MF_CHECKED } else { 0 };

            add(MF_STRING, ID_SETTINGS, "Settings");
            add(MF_SEPARATOR, 0, "");
            match &self.last_app {
                Some((app, _)) => {
                    let disabled = !self.cfg.profile_for(app).enabled;
                    add(MF_STRING | check(disabled), ID_TOGGLE_APP, &format!("Disable for {app}"));
                }
                None => add(MF_STRING | MF_GRAYED, ID_TOGGLE_APP, "Disable for Active App"),
            }
            add(MF_STRING | check(!self.cfg.enabled), ID_TOGGLE_ALL, "Disable for All Apps");
            add(MF_SEPARATOR, 0, "");
            add(MF_STRING | check(self.cfg.start_with_windows), ID_AUTOSTART, "Start with Windows");
            add(MF_STRING, ID_FOLDER, "Open Settings Folder");
            add(MF_STRING, ID_ABOUT, "About SilkScroll");
            add(MF_SEPARATOR, 0, "");
            add(MF_STRING, ID_EXIT, "Exit");
            SetMenuDefaultItem(menu, ID_SETTINGS as u32, 0);
            menu
        }
    }

    fn on_menu(&mut self, id: usize) {
        match id {
            ID_TOGGLE_APP => {
                if let Some((app, path)) = self.last_app.clone() {
                    let enabled = self.cfg.profile_for(&app).enabled;
                    self.cfg.add_app(&app, Some(&path));
                    self.cfg.set_app_enabled(&app, !enabled);
                    self.save();
                }
            }
            ID_TOGGLE_ALL => {
                self.cfg.enabled = !self.cfg.enabled;
                self.save();
            }
            ID_AUTOSTART => {
                self.cfg.start_with_windows = !self.cfg.start_with_windows;
                self.save();
            }
            ID_FOLDER => shell_open(&config::config_dir().to_string_lossy()),
            _ => {}
        }
    }
}

fn copy_into(dst: &mut [u16], text: &str) {
    let src: Vec<u16> = text.encode_utf16().take(dst.len() - 1).collect();
    dst[..src.len()].copy_from_slice(&src);
    dst[src.len()] = 0;
}

pub fn about_text() -> String {
    format!(
        "SilkScroll {}\n\nSmooth scrolling for every Windows app.\n\nFree and open source under the MIT License.",
        env!("CARGO_PKG_VERSION")
    )
}

fn show_menu(hwnd: HWND) {
    let Some(menu) = with_tray(|t| t.build_menu()) else { return };
    let mut pt = POINT::default();
    let id = unsafe {
        GetCursorPos(&mut pt);
        // Without this the menu would not close when clicking elsewhere.
        SetForegroundWindow(hwnd);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        id as usize
    };
    match id {
        0 => {}
        ID_SETTINGS => spawn_self(Some("--settings")),
        ID_ABOUT => {
            message_box(hwnd, &about_text(), "About SilkScroll", MB_OK | MB_ICONINFORMATION);
        }
        ID_EXIT => exit(hwnd),
        other => {
            with_tray(|t| t.on_menu(other));
        }
    }
}

fn exit(hwnd: HWND) {
    with_tray(|t| {
        t.remove_icon();
        t.engine.shutdown();
    });
    unsafe { DestroyWindow(hwnd) };
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP => spawn_self(Some("--settings")),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_TIMER => {
            match wparam {
                TIMER_POLL => {
                    with_tray(|t| {
                        t.reload(false);
                        t.track_foreground();
                    });
                }
                TIMER_REHOOK => {
                    with_tray(|t| t.engine.rehook());
                }
                _ => {}
            }
            0
        }
        WM_APP_RELOAD => {
            with_tray(|t| t.reload(true));
            0
        }
        WM_APP_QUIT | WM_CLOSE => {
            exit(hwnd);
            0
        }
        WM_WTSSESSION_CHANGE => {
            if wparam == WTS_SESSION_UNLOCK {
                with_tray(|t| t.engine.rehook());
            }
            0
        }
        WM_POWERBROADCAST => {
            if wparam == PBT_APMRESUMEAUTOMATIC as usize {
                with_tray(|t| t.engine.rehook());
            }
            1
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            // Explorer restarted and forgot every tray icon.
            with_tray(|t| {
                t.icon_shown = false;
                t.update_icon();
            });
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
