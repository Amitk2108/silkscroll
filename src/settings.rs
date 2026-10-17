//! The settings window, built from native Windows controls.
//!
//! Left: the list of profiles ("Default (All Applications)" plus one entry per
//! app) with add, remove and reset buttons. Right: the selected profile's
//! values. Bottom: options for SilkScroll itself. Every change is saved at
//! once and the engine is told to reload.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr::null_mut;
use std::time::SystemTime;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW,
    COLOR_WINDOWTEXT, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, DeleteObject, DrawFocusRect, DrawTextW, FW_NORMAL, FillRect, GetMonitorInfoW, GetSysColor,
    GetSysColorBrush, HDC, HFONT, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    OUT_DEFAULT_PRECIS, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::{
    BST_CHECKED, BST_UNCHECKED, DRAWITEMSTRUCT, EM_SETMARGINS, ICC_STANDARD_CLASSES, ICC_WIN95_CLASSES,
    INITCOMMONCONTROLSEX, InitCommonControlsEx, MEASUREITEMSTRUCT, ODS_FOCUS, ODS_SELECTED, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_SETDELAYTIME, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
};
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::Shell::{
    SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{
    self, Config, LIMITS_ACCELERATION_DELTA, LIMITS_ACCELERATION_MAX, LIMITS_ANIMATION_TIME, LIMITS_STEP_SIZE,
    LIMITS_TAIL_RATIO, Profile,
};
use crate::easing::Easing;
use crate::instance::{self, SETTINGS_CLASS, WM_APP_RELOAD};
use crate::presets::{self, PRESETS};
use crate::target::{exe_name, running_apps};
use crate::tray::about_text;
use crate::win::{from_wide, hinstance, make_icon, message_box, scale, spawn_self, wide};

// Control ids.
const IDC_LIST: u16 = 100;
const IDC_ADD: u16 = 101;
const IDC_REMOVE: u16 = 102;
const IDC_RESET_ALL: u16 = 103;
const IDC_PRESET: u16 = 105;
const IDC_ENABLE: u16 = 110;
const IDC_FIELD: u16 = 120; // + row
const IDC_FIELD_RESET: u16 = 140; // + row
const IDC_EASING: u16 = 160;
const IDC_HORIZONTAL: u16 = 161;
const IDC_SHIFT: u16 = 162;
const IDC_REVERSE: u16 = 163;
const IDC_AUTOSTART: u16 = 170;
const IDC_TRAY_ICON: u16 = 171;
const IDC_TOUCHPAD: u16 = 172;
const IDC_FULLSCREEN: u16 = 173;
const IDC_HELP: u16 = 180;
const IDC_ABOUT: u16 = 181;

const MENU_BROWSE: usize = 1;
const MENU_RUNNING: usize = 100;

const TIMER_SAVE: usize = 1;
const TIMER_WATCH: usize = 2;

// Static control styles (kept here to avoid pulling in all of SystemServices).
const SS_LEFT: u32 = 0x0;
const SS_ETCHEDHORZ: u32 = 0x10;
const SS_NOTIFY: u32 = 0x100;
const SS_CENTERIMAGE: u32 = 0x200;

const CLIENT_W: i32 = 600;
const CLIENT_H: i32 = 502;
const ROW_COUNT: usize = 5;
const TAIL_ROW: usize = 4;

struct Field {
    label: &'static str,
    tip: &'static str,
    limits: (f64, f64),
    get: fn(&Profile) -> f64,
    set: fn(&mut Profile, f64),
}

static FIELDS: [Field; ROW_COUNT] = [
    Field {
        label: "Step size [px]",
        tip: "How far one notch of the wheel scrolls.\n100 matches a normal notch in a browser.",
        limits: LIMITS_STEP_SIZE,
        get: |p| p.step_size_px,
        set: |p, v| p.step_size_px = v,
    },
    Field {
        label: "Animation time [ms]",
        tip: "How long each notch keeps gliding.\nHigher feels slower and silkier.",
        limits: LIMITS_ANIMATION_TIME,
        get: |p| p.animation_time_ms,
        set: |p, v| p.animation_time_ms = v,
    },
    Field {
        label: "Acceleration delta [ms]",
        tip: "Notches that come faster than this make scrolling speed up.\nSmaller means you have to spin faster. 0 turns acceleration off.",
        limits: LIMITS_ACCELERATION_DELTA,
        get: |p| p.acceleration_delta_ms,
        set: |p, v| p.acceleration_delta_ms = v,
    },
    Field {
        label: "Acceleration max [x]",
        tip: "The most fast spinning can speed things up,\nas a multiple of the step size.",
        limits: LIMITS_ACCELERATION_MAX,
        get: |p| p.acceleration_max,
        set: |p, v| p.acceleration_max = v,
    },
    Field {
        label: "Tail to head ratio [x]",
        tip: "How much longer the glide slows down than it speeds up.\n4 means the slow-down takes four times as long.",
        limits: LIMITS_TAIL_RATIO,
        get: |p| p.tail_to_head_ratio,
        set: |p, v| p.tail_to_head_ratio = v,
    },
];

const HELP_TEXT: &str = "SilkScroll catches each notch of your mouse wheel and replays it as a smooth glide.\n\n\
Step size: how far one notch scrolls (100 is a normal notch).\n\
Animation time: how long each glide lasts.\n\
Acceleration delta / max: spin the wheel quickly to scroll faster, up to the max.\n\
Tail to head ratio: how much longer the glide slows down than it speeds up.\n\n\
Click + to give an app its own settings, or untick \"Enable for this app\" to leave it alone. \
The tray icon menu can also turn SilkScroll off for the active app or for all apps.\n\n\
Hold Ctrl while scrolling to zoom as usual; Ctrl, Alt and Win + wheel are never smoothed.\n\n\
Settings are saved in %APPDATA%\\SilkScroll\\config.json.";

/// Window handles of every control.
struct Controls {
    list: HWND,
    add: HWND,
    remove: HWND,
    reset_all: HWND,
    enable: HWND,
    preset_label: HWND,
    preset: HWND,
    labels: [HWND; ROW_COUNT],
    edits: [HWND; ROW_COUNT],
    resets: [HWND; ROW_COUNT],
    easing: HWND,
    horizontal: HWND,
    shift: HWND,
    reverse: HWND,
    separator: HWND,
    autostart: HWND,
    tray_icon: HWND,
    touchpad: HWND,
    fullscreen: HWND,
    help: HWND,
    about: HWND,
}

struct State {
    hwnd: HWND,
    c: Controls,
    dpi: u32,
    font: HFONT,
    glyph_font: HFONT,
    cfg: Config,
    /// `None` is "Default (All Applications)".
    selected: Option<String>,
    items: Vec<Option<String>>,
    icons: HashMap<String, HICON>,
    app_icon: HICON,
    save_pending: bool,
    saved_time: Option<SystemTime>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Runs `f` on the window state unless it is already being used further up
/// the stack. That happens when changing a control from code makes Windows
/// send a notification straight back, which we want to ignore anyway.
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

pub fn run() {
    let Some(_lock) = instance::acquire(instance::SETTINGS_MUTEX) else {
        instance::focus_settings();
        return;
    };
    if !instance::is_running(instance::ENGINE_MUTEX) {
        spawn_self(None);
    }

    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES,
        };
        InitCommonControlsEx(&icc);

        let class = wide(SETTINGS_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            lpszClassName: class.as_ptr(),
            hIcon: make_icon(32, true),
            hIconSm: make_icon(16, true),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("SilkScroll Settings").as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CLIENT_W,
            CLIENT_H,
            null_mut(),
            null_mut(),
            hinstance(),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }
        let dpi = GetDpiForWindow(hwnd).max(96);
        center_window(hwnd, dpi);

        let state = State::new(hwnd, dpi);
        STATE.with(|cell| *cell.borrow_mut() = Some(state));
        with_state(|s| {
            s.layout();
            s.refresh_list();
            s.fill();
        });
        SetTimer(hwnd, TIMER_WATCH, 1000, None);

        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

/// Sizes the window for its client area at `dpi` and centers it on the
/// monitor it is on.
fn center_window(hwnd: HWND, dpi: u32) {
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let mut rect = RECT { left: 0, top: 0, right: scale(CLIENT_W, dpi), bottom: scale(CLIENT_H, dpi) };
        AdjustWindowRectExForDpi(&mut rect, style, 0, 0, dpi);
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info);
        let work = info.rcWork;
        let x = work.left + (work.right - work.left - w) / 2;
        let y = work.top + (work.bottom - work.top - h) / 2;
        SetWindowPos(hwnd, null_mut(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

fn create_child(parent: HWND, class: &str, text: &str, style: u32, ex_style: u32, id: u16) -> HWND {
    unsafe {
        CreateWindowExW(
            ex_style,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            parent,
            id as usize as HMENU,
            hinstance(),
            std::ptr::null(),
        )
    }
}

fn button(parent: HWND, text: &str, id: u16) -> HWND {
    create_child(parent, "BUTTON", text, (BS_PUSHBUTTON as u32) | WS_TABSTOP, 0, id)
}

fn checkbox(parent: HWND, text: &str, id: u16) -> HWND {
    create_child(parent, "BUTTON", text, (BS_AUTOCHECKBOX as u32) | WS_TABSTOP, 0, id)
}

fn label(parent: HWND, text: &str) -> HWND {
    create_child(parent, "STATIC", text, SS_LEFT | SS_CENTERIMAGE | SS_NOTIFY, 0, 0)
}

/// The preset drop-down: every preset plus "Custom" for hand-edited values.
fn preset_box(parent: HWND) -> HWND {
    let combo =
        create_child(parent, "COMBOBOX", "", (CBS_DROPDOWNLIST as u32) | WS_VSCROLL | WS_TABSTOP, 0, IDC_PRESET);
    for name in PRESETS.iter().map(|p| p.name).chain(["Custom"]) {
        unsafe { SendMessageW(combo, CB_ADDSTRING, 0, wide(name).as_ptr() as LPARAM) };
    }
    combo
}

fn make_font(face: &str, points: i32, dpi: u32) -> HFONT {
    unsafe {
        CreateFontW(
            -(points * dpi as i32 / 72),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide(face).as_ptr(),
        )
    }
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize }
}

fn set_checked(hwnd: HWND, on: bool) {
    let value = if on { BST_CHECKED } else { BST_UNCHECKED };
    unsafe { SendMessageW(hwnd, BM_SETCHECK, value as usize, 0) };
}

fn set_text(hwnd: HWND, text: &str) {
    unsafe { SetWindowTextW(hwnd, wide(text).as_ptr()) };
}

fn get_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn enable(hwnd: HWND, on: bool) {
    unsafe { EnableWindow(hwnd, on as i32) };
}

pub fn format_value(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{v:.0}")
    } else {
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// The small icon Windows shows for `path`, or the generic program icon.
fn file_icon(path: Option<&str>) -> HICON {
    unsafe {
        let mut info: SHFILEINFOW = std::mem::zeroed();
        if let Some(path) = path {
            let path = path.replace('/', "\\");
            let found = SHGetFileInfoW(
                wide(&path).as_ptr(),
                0,
                &mut info,
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_SMALLICON,
            );
            if found != 0 && !info.hIcon.is_null() {
                return info.hIcon;
            }
        }
        SHGetFileInfoW(
            wide("program.exe").as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut info,
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES,
        );
        info.hIcon
    }
}

impl State {
    fn new(hwnd: HWND, dpi: u32) -> State {
        let c = Controls {
            list: create_child(
                hwnd,
                "LISTBOX",
                "",
                (LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT) as u32
                    | WS_VSCROLL
                    | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
                IDC_LIST,
            ),
            add: button(hwnd, "+", IDC_ADD),
            remove: button(hwnd, "\u{2013}", IDC_REMOVE),
            reset_all: button(hwnd, "Reset All", IDC_RESET_ALL),
            enable: checkbox(hwnd, "Enable for all apps by default", IDC_ENABLE),
            preset_label: label(hwnd, "Preset"),
            preset: preset_box(hwnd),
            labels: std::array::from_fn(|i| label(hwnd, FIELDS[i].label)),
            edits: std::array::from_fn(|i| {
                create_child(
                    hwnd,
                    "EDIT",
                    "",
                    (ES_AUTOHSCROLL as u32) | WS_TABSTOP,
                    WS_EX_CLIENTEDGE,
                    IDC_FIELD + i as u16,
                )
            }),
            resets: std::array::from_fn(|i| button(hwnd, "\u{E72C}", IDC_FIELD_RESET + i as u16)),
            easing: checkbox(hwnd, "Animation easing", IDC_EASING),
            horizontal: checkbox(hwnd, "Horizontal smoothing", IDC_HORIZONTAL),
            shift: checkbox(hwnd, "Shift + wheel scrolls horizontally", IDC_SHIFT),
            reverse: checkbox(hwnd, "Reverse wheel direction", IDC_REVERSE),
            separator: create_child(hwnd, "STATIC", "", SS_ETCHEDHORZ, 0, 0),
            autostart: checkbox(hwnd, "Start with Windows", IDC_AUTOSTART),
            tray_icon: checkbox(hwnd, "Show tray icon", IDC_TRAY_ICON),
            touchpad: checkbox(hwnd, "Ignore touchpad", IDC_TOUCHPAD),
            fullscreen: checkbox(hwnd, "Skip fullscreen apps (games)", IDC_FULLSCREEN),
            help: button(hwnd, "Help", IDC_HELP),
            about: button(hwnd, "About", IDC_ABOUT),
        };

        let mut state = State {
            hwnd,
            c,
            dpi,
            font: make_font("Segoe UI", 9, dpi),
            glyph_font: make_font("Segoe MDL2 Assets", 9, dpi),
            app_icon: make_icon(32, true),
            cfg: config::load(),
            selected: None,
            items: Vec::new(),
            icons: HashMap::new(),
            save_pending: false,
            saved_time: config::modified_time(),
        };
        state.sync_icons();
        state.add_tooltips();
        state
    }

    /// Looks up the icon of every app that doesn't have one yet.
    fn sync_icons(&mut self) {
        for exe in self.cfg.apps.keys() {
            if !self.icons.contains_key(exe) {
                let icon = file_icon(self.cfg.app_paths.get(exe).map(String::as_str));
                self.icons.insert(exe.clone(), icon);
            }
        }
    }

    fn add_tooltips(&self) {
        unsafe {
            let tip = CreateWindowExW(
                WS_EX_TOPMOST,
                wide("tooltips_class32").as_ptr(),
                std::ptr::null(),
                WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                self.hwnd,
                null_mut(),
                hinstance(),
                std::ptr::null(),
            );
            SendMessageW(tip, TTM_SETMAXTIPWIDTH, 0, scale(360, self.dpi) as isize);
            SendMessageW(tip, TTM_SETDELAYTIME, 2 /* TTDT_AUTOPOP */, 30_000);

            let add = |control: HWND, text: &str| {
                let mut text = wide(text);
                let info = TTTOOLINFOW {
                    cbSize: size_of::<TTTOOLINFOW>() as u32,
                    uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                    hwnd: self.hwnd,
                    uId: control as usize,
                    lpszText: text.as_mut_ptr(),
                    ..std::mem::zeroed()
                };
                SendMessageW(tip, TTM_ADDTOOLW, 0, &info as *const TTTOOLINFOW as LPARAM);
            };

            let c = &self.c;
            for (i, field) in FIELDS.iter().enumerate() {
                add(c.labels[i], field.tip);
                add(c.edits[i], field.tip);
                add(c.resets[i], "Reset to default");
            }
            let preset_tip = "Ready-made scroll styles. Pick one to fill in the values below;\nchange any value afterwards and it becomes Custom.";
            add(c.preset_label, preset_tip);
            add(c.preset, preset_tip);
            add(c.add, "Add settings for an app");
            add(c.remove, "Remove the settings for this app");
            add(c.reset_all, "Reset every value of this profile");
            add(c.easing, "Ease each notch in and out\ninstead of moving at a constant speed.");
            add(c.horizontal, "Smooth sideways scrolling too\n(tilt wheels and horizontal wheels).");
            add(
                c.shift,
                "Hold Shift while turning the wheel to scroll sideways,\neven in apps that don't support it themselves.",
            );
            add(c.reverse, "Flip the scroll direction:\nup becomes down, left becomes right.");
            add(c.autostart, "Start SilkScroll automatically when you sign in.");
            add(c.tray_icon, "If hidden, run SilkScroll again to open this window.");
            add(
                c.touchpad,
                "Leave touchpads and high-resolution wheels alone;\nthey already scroll smoothly. (Default: on)",
            );
            add(
                c.fullscreen,
                "Don't smooth scrolling in borderless fullscreen apps,\nmostly games, which expect whole wheel notches.",
            );
        }
    }

    /// Positions every control for the current DPI.
    fn layout(&self) {
        let c = &self.c;
        let mut place: Vec<(HWND, i32, i32, i32, i32)> = vec![
            (c.list, 14, 14, 214, 354),
            (c.add, 14, 376, 36, 28),
            (c.remove, 54, 376, 36, 28),
            (c.reset_all, 148, 376, 80, 28),
            (c.enable, 244, 14, 342, 24),
            (c.preset_label, 244, 52, 186, 26),
            // A drop-down list's height includes its open list.
            (c.preset, 434, 52, 120, 220),
            (c.easing, 244, 272, 342, 24),
            (c.horizontal, 244, 300, 342, 24),
            (c.shift, 244, 328, 342, 24),
            (c.reverse, 244, 356, 342, 24),
            (c.separator, 14, 420, 572, 2),
            (c.autostart, 14, 434, 190, 24),
            (c.tray_icon, 214, 434, 190, 24),
            (c.touchpad, 14, 464, 190, 24),
            (c.fullscreen, 214, 464, 210, 24),
            (c.help, 430, 448, 74, 28),
            (c.about, 512, 448, 74, 28),
        ];
        for i in 0..ROW_COUNT {
            let y = 88 + i as i32 * 36;
            place.push((c.labels[i], 244, y, 186, 26));
            place.push((c.edits[i], 434, y, 84, 26));
            place.push((c.resets[i], 524, y, 30, 26));
        }

        unsafe {
            for (hwnd, x, y, w, h) in place {
                let s = |v| scale(v, self.dpi);
                SetWindowPos(hwnd, null_mut(), s(x), s(y), s(w), s(h), SWP_NOZORDER | SWP_NOACTIVATE);
                SendMessageW(hwnd, WM_SETFONT, self.font as usize, 0);
            }
            for reset in c.resets {
                SendMessageW(reset, WM_SETFONT, self.glyph_font as usize, 0);
            }
            SendMessageW(c.list, LB_SETITEMHEIGHT, 0, scale(26, self.dpi) as isize);
            // Make the closed drop-down as tall as the edit boxes.
            SendMessageW(c.preset, CB_SETITEMHEIGHT, usize::MAX, scale(20, self.dpi) as isize);
            // Push the text off the edit border a little.
            let margin = scale(4, self.dpi) as usize;
            for edit in c.edits {
                SendMessageW(
                    edit,
                    EM_SETMARGINS,
                    (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                    ((margin << 16) | margin) as isize,
                );
            }
        }
    }

    fn on_dpi_changed(&mut self, dpi: u32) {
        self.dpi = dpi;
        unsafe {
            DeleteObject(self.font);
            DeleteObject(self.glyph_font);
        }
        self.font = make_font("Segoe UI", 9, dpi);
        self.glyph_font = make_font("Segoe MDL2 Assets", 9, dpi);
        self.layout();
    }

    fn profile(&self) -> &Profile {
        match &self.selected {
            Some(exe) => self.cfg.apps.get(exe).unwrap_or(&self.cfg.defaults),
            None => &self.cfg.defaults,
        }
    }

    fn profile_mut(&mut self) -> &mut Profile {
        if let Some(exe) = &self.selected {
            if self.cfg.apps.contains_key(exe) {
                return self.cfg.apps.get_mut(exe).expect("checked above");
            }
        }
        &mut self.cfg.defaults
    }

    /// What "reset" goes back to: factory values for the defaults, the
    /// current defaults for an app.
    fn baseline(&self) -> Profile {
        match self.selected {
            Some(_) => self.cfg.defaults,
            None => Profile::default(),
        }
    }

    fn refresh_list(&mut self) {
        if let Some(exe) = &self.selected {
            if !self.cfg.apps.contains_key(exe) {
                self.selected = None;
            }
        }
        self.items = std::iter::once(None).chain(self.cfg.apps.keys().cloned().map(Some)).collect();
        unsafe {
            SendMessageW(self.c.list, LB_RESETCONTENT, 0, 0);
            for item in &self.items {
                let text = item.as_deref().unwrap_or("Default (All Applications)");
                SendMessageW(self.c.list, LB_ADDSTRING, 0, wide(text).as_ptr() as LPARAM);
            }
            let index = self.items.iter().position(|i| *i == self.selected).unwrap_or(0);
            SendMessageW(self.c.list, LB_SETCURSEL, index, 0);
            InvalidateRect(self.c.list, std::ptr::null(), 1);
        }
    }

    /// Shows the selected profile and the global options in the controls.
    fn fill(&self) {
        let c = &self.c;
        let p = *self.profile();
        set_text(
            c.enable,
            if self.selected.is_some() { "Enable for this app" } else { "Enable for all apps by default" },
        );
        set_checked(c.enable, p.enabled);
        for i in 0..ROW_COUNT {
            self.fill_field(i);
        }
        self.sync_preset();
        set_checked(c.easing, p.easing != Easing::Linear);
        set_checked(c.horizontal, p.horizontal_smoothing);
        set_checked(c.shift, p.shift_for_horizontal);
        set_checked(c.reverse, p.reverse_direction);
        let eased = p.easing != Easing::Linear;
        enable(c.labels[TAIL_ROW], eased);
        enable(c.edits[TAIL_ROW], eased);
        enable(c.resets[TAIL_ROW], eased);
        enable(c.remove, self.selected.is_some());

        set_checked(c.autostart, self.cfg.start_with_windows);
        set_checked(c.tray_icon, self.cfg.show_tray_icon);
        set_checked(c.touchpad, self.cfg.ignore_touchpad);
        set_checked(c.fullscreen, self.cfg.skip_fullscreen_apps);
    }

    fn fill_field(&self, i: usize) {
        set_text(self.c.edits[i], &format_value((FIELDS[i].get)(self.profile())));
    }

    /// Shows which preset the values match, or "Custom".
    fn sync_preset(&self) {
        let index = presets::matching(self.profile()).unwrap_or(PRESETS.len());
        unsafe { SendMessageW(self.c.preset, CB_SETCURSEL, index, 0) };
    }

    fn schedule_save(&mut self) {
        self.save_pending = true;
        unsafe { SetTimer(self.hwnd, TIMER_SAVE, 250, None) };
    }

    fn save_now(&mut self) {
        unsafe { KillTimer(self.hwnd, TIMER_SAVE) };
        if !self.save_pending {
            return;
        }
        self.save_pending = false;
        self.cfg = std::mem::take(&mut self.cfg).sanitized();
        if config::save(&self.cfg).is_ok() {
            self.saved_time = config::modified_time();
            instance::notify_engine(WM_APP_RELOAD);
        }
    }

    /// Picks up changes made elsewhere, such as the tray menu.
    fn watch_file(&mut self) {
        if self.save_pending {
            return;
        }
        let time = config::modified_time();
        if time != self.saved_time {
            self.saved_time = time;
            if let Ok(cfg) = config::read() {
                self.cfg = cfg;
                self.sync_icons();
                self.refresh_list();
                self.fill();
            }
        }
    }

    fn select_index(&mut self, index: usize) {
        if let Some(item) = self.items.get(index) {
            self.selected = item.clone();
            self.fill();
        }
    }

    fn add_app(&mut self, path: &str) {
        let exe = exe_name(path);
        let key = self.cfg.add_app(&exe, Some(path));
        self.icons.insert(key.clone(), file_icon(Some(path)));
        self.selected = Some(key);
        self.refresh_list();
        self.fill();
        self.schedule_save();
    }

    fn remove_selected(&mut self) {
        if let Some(exe) = self.selected.take() {
            self.cfg.remove_app(&exe);
            self.refresh_list();
            self.fill();
            self.schedule_save();
        }
    }

    fn reset_all(&mut self) {
        let baseline = self.baseline();
        *self.profile_mut() = baseline;
        self.fill();
        self.schedule_save();
    }

    fn on_control(&mut self, id: u16, code: u32) {
        let c = &self.c;
        match id {
            IDC_LIST if code == LBN_SELCHANGE => {
                let index = unsafe { SendMessageW(c.list, LB_GETCURSEL, 0, 0) };
                if index >= 0 {
                    self.select_index(index as usize);
                }
            }
            _ if (IDC_FIELD..IDC_FIELD + ROW_COUNT as u16).contains(&id) => {
                let i = (id - IDC_FIELD) as usize;
                if code == EN_CHANGE {
                    if let Ok(value) = get_text(c.edits[i]).trim().replace(',', ".").parse::<f64>() {
                        let (lo, hi) = FIELDS[i].limits;
                        if value.is_finite() {
                            (FIELDS[i].set)(self.profile_mut(), value.clamp(lo, hi));
                            self.sync_preset();
                            self.schedule_save();
                        }
                    }
                } else if code == EN_KILLFOCUS {
                    self.fill_field(i);
                }
            }
            _ if (IDC_FIELD_RESET..IDC_FIELD_RESET + ROW_COUNT as u16).contains(&id) => {
                let i = (id - IDC_FIELD_RESET) as usize;
                let value = (FIELDS[i].get)(&self.baseline());
                (FIELDS[i].set)(self.profile_mut(), value);
                self.fill_field(i);
                self.sync_preset();
                self.schedule_save();
            }
            IDC_PRESET if code == CBN_SELCHANGE => {
                let index = unsafe { SendMessageW(c.preset, CB_GETCURSEL, 0, 0) };
                // "Custom" is only a label for hand-edited values.
                if let Some(preset) = usize::try_from(index).ok().and_then(|i| PRESETS.get(i)) {
                    preset.apply(self.profile_mut());
                    self.fill();
                    self.schedule_save();
                } else {
                    self.sync_preset();
                }
            }
            _ if code != BN_CLICKED => {}
            IDC_ENABLE => {
                let on = is_checked(c.enable);
                self.profile_mut().enabled = on;
                unsafe { InvalidateRect(self.c.list, std::ptr::null(), 1) };
                self.schedule_save();
            }
            IDC_EASING => {
                let on = is_checked(c.easing);
                let p = self.profile_mut();
                p.easing = match (on, p.easing) {
                    (true, Easing::Linear) => Easing::Pulse,
                    (true, kept) => kept,
                    (false, _) => Easing::Linear,
                };
                self.fill();
                self.schedule_save();
            }
            IDC_HORIZONTAL => {
                let on = is_checked(c.horizontal);
                self.profile_mut().horizontal_smoothing = on;
                self.schedule_save();
            }
            IDC_SHIFT => {
                let on = is_checked(c.shift);
                self.profile_mut().shift_for_horizontal = on;
                self.schedule_save();
            }
            IDC_REVERSE => {
                let on = is_checked(c.reverse);
                self.profile_mut().reverse_direction = on;
                self.schedule_save();
            }
            IDC_AUTOSTART => {
                self.cfg.start_with_windows = is_checked(c.autostart);
                self.schedule_save();
            }
            IDC_TRAY_ICON => {
                self.cfg.show_tray_icon = is_checked(c.tray_icon);
                self.schedule_save();
            }
            IDC_TOUCHPAD => {
                self.cfg.ignore_touchpad = is_checked(c.touchpad);
                self.schedule_save();
            }
            IDC_FULLSCREEN => {
                self.cfg.skip_fullscreen_apps = is_checked(c.fullscreen);
                self.schedule_save();
            }
            _ => {}
        }
    }

    fn draw_item(&self, item: &DRAWITEMSTRUCT) {
        let index = item.itemID as usize;
        let Some(entry) = self.items.get(index) else { return };
        let hdc: HDC = item.hDC;
        let rect = item.rcItem;
        let selected = item.itemState & ODS_SELECTED != 0;
        let enabled = match entry {
            Some(exe) => self.cfg.apps.get(exe).is_none_or(|p| p.enabled),
            None => self.cfg.defaults.enabled,
        };

        unsafe {
            let (bg, fg) = if selected {
                (COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT)
            } else if enabled {
                (COLOR_WINDOW, COLOR_WINDOWTEXT)
            } else {
                (COLOR_WINDOW, COLOR_GRAYTEXT)
            };
            FillRect(hdc, &rect, GetSysColorBrush(bg));
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, GetSysColor(fg));

            let size = scale(16, self.dpi);
            let pad = scale(6, self.dpi);
            let icon = match entry {
                Some(exe) => self.icons.get(exe).copied().unwrap_or(null_mut()),
                None => self.app_icon,
            };
            if !icon.is_null() {
                let top = rect.top + (rect.bottom - rect.top - size) / 2;
                DrawIconEx(hdc, rect.left + pad, top, icon, size, size, 0, null_mut(), DI_NORMAL);
            }

            let mut text_rect = RECT { left: rect.left + pad * 2 + size, ..rect };
            text_rect.right -= pad;
            let mut text = match entry {
                Some(exe) => exe.clone(),
                None => "Default (All Applications)".to_string(),
            };
            if !enabled {
                text.push_str("  (off)");
            }
            let text = wide(&text);
            DrawTextW(
                hdc,
                text.as_ptr(),
                -1,
                &mut text_rect,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            if item.itemState & ODS_FOCUS != 0 {
                DrawFocusRect(hdc, &rect);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Dialogs and menus (run without holding the state, since they pump messages)

fn confirm(hwnd: HWND, text: &str, title: &str) -> bool {
    message_box(hwnd, text, title, MB_YESNO | MB_ICONQUESTION) == IDYES
}

/// Shows the "+" menu under the add button and returns the chosen exe path.
fn choose_app(hwnd: HWND) -> Option<String> {
    let apps = running_apps();
    let configured: Vec<String> = with_state(|s| s.cfg.apps.keys().cloned().collect()).unwrap_or_default();
    let add_button = with_state(|s| s.c.add)?;

    let choice = unsafe {
        let menu = CreatePopupMenu();
        let running = CreatePopupMenu();
        if apps.is_empty() {
            AppendMenuW(running, MF_STRING | MF_GRAYED, 0, wide("No apps with open windows").as_ptr());
        }
        for (i, (exe, _)) in apps.iter().enumerate() {
            let flags = if configured.contains(exe) { MF_STRING | MF_GRAYED | MF_CHECKED } else { MF_STRING };
            AppendMenuW(running, flags, MENU_RUNNING + i, wide(exe).as_ptr());
        }
        AppendMenuW(menu, MF_POPUP, running as usize, wide("Add settings for a running app").as_ptr());
        AppendMenuW(menu, MF_STRING, MENU_BROWSE, wide("Find an app (.exe)...").as_ptr());

        let mut rect = RECT::default();
        GetWindowRect(add_button, &mut rect);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN,
            rect.left,
            rect.bottom,
            0,
            hwnd,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        id as usize
    };

    match choice {
        0 => None,
        MENU_BROWSE => browse_for_exe(hwnd),
        n => apps.get(n - MENU_RUNNING).map(|(_, path)| path.clone()),
    }
}

fn browse_for_exe(hwnd: HWND) -> Option<String> {
    let mut file = [0u16; 1024];
    let filter: Vec<u16> = "Programs (*.exe)\0*.exe\0All files\0*.*\0\0".encode_utf16().collect();
    let title = wide("Find an app");
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_HIDEREADONLY;
    (unsafe { GetOpenFileNameW(&mut ofn) } != 0).then(|| from_wide(&file))
}

fn on_command(hwnd: HWND, id: u16, code: u32) {
    match id {
        IDC_ADD if code == BN_CLICKED => {
            if let Some(path) = choose_app(hwnd) {
                with_state(|s| s.add_app(&path));
            }
        }
        IDC_REMOVE if code == BN_CLICKED => {
            let Some(Some(exe)) = with_state(|s| s.selected.clone()) else { return };
            let text = format!("Remove the settings for {exe}?\nIt will use the default settings again.");
            if confirm(hwnd, &text, "Remove app settings") {
                with_state(|s| s.remove_selected());
            }
        }
        IDC_RESET_ALL if code == BN_CLICKED => {
            let Some(selected) = with_state(|s| s.selected.clone()) else { return };
            let text = match selected {
                Some(exe) => format!("Reset {exe} to the default settings?"),
                None => "Reset the default settings to their original values?".to_string(),
            };
            if confirm(hwnd, &text, "Reset All") {
                with_state(|s| s.reset_all());
            }
        }
        IDC_HELP if code == BN_CLICKED => {
            let mut help = format!("{HELP_TEXT}\n\nPresets:");
            for preset in &PRESETS {
                help.push_str(&format!("\n- {}: {}", preset.name, preset.description));
            }
            message_box(hwnd, &help, "SilkScroll Help", MB_OK | MB_ICONINFORMATION);
        }
        IDC_ABOUT if code == BN_CLICKED => {
            message_box(hwnd, &about_text(), "About SilkScroll", MB_OK | MB_ICONINFORMATION);
        }
        _ => {
            with_state(|s| s.on_control(id, code));
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            on_command(hwnd, (wparam & 0xFFFF) as u16, ((wparam >> 16) & 0xFFFF) as u32);
            0
        }
        WM_MEASUREITEM => {
            let item = unsafe { &mut *(lparam as *mut MEASUREITEMSTRUCT) };
            item.itemHeight = scale(26, unsafe { GetDpiForWindow(hwnd) }.max(96)) as u32;
            1
        }
        WM_DRAWITEM => {
            let item = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            if item.itemID != u32::MAX {
                STATE.with(|cell| {
                    if let Ok(guard) = cell.try_borrow() {
                        if let Some(state) = guard.as_ref() {
                            state.draw_item(item);
                        }
                    }
                });
            }
            1
        }
        WM_CTLCOLORSTATIC => {
            // Labels and checkboxes sit on the white window background.
            let hdc = wparam as HDC;
            unsafe {
                SetBkColor(hdc, GetSysColor(COLOR_WINDOW));
                GetSysColorBrush(COLOR_WINDOW) as LRESULT
            }
        }
        WM_TIMER => {
            match wparam {
                TIMER_SAVE => {
                    with_state(|s| s.save_now());
                }
                TIMER_WATCH => {
                    with_state(|s| s.watch_file());
                }
                _ => {}
            }
            0
        }
        WM_DPICHANGED => {
            let dpi = ((wparam >> 16) & 0xFFFF) as u32;
            let r = unsafe { &*(lparam as *const RECT) };
            unsafe {
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            with_state(|s| s.on_dpi_changed(dpi));
            0
        }
        WM_CLOSE => {
            with_state(|s| s.save_now());
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::format_value;

    #[test]
    fn values_are_shown_without_needless_decimals() {
        assert_eq!(format_value(100.0), "100");
        assert_eq!(format_value(2.5), "2.5");
        assert_eq!(format_value(3.25), "3.25");
        assert_eq!(format_value(1.0 / 3.0), "0.33");
    }
}
