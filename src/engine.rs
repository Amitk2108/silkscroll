//! The scrolling engine.
//!
//! A low-level mouse hook sees every wheel event before any app does. When an
//! event should be smoothed, the hook swallows it and passes the distance to
//! the animator thread, which replays it as a quick series of small wheel
//! events through `SendInput`. Those injected events carry
//! [`INJECTED_MARK`] so the hook lets them through untouched.

use std::cell::RefCell;
use std::ptr::null_mut;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HANDLE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW};
use windows_sys::Win32::Media::timeBeginPeriod;
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, GetCurrentThread, GetCurrentThreadId, INFINITE,
    SetThreadPriority, SetWaitableTimer, THREAD_PRIORITY_HIGHEST, THREAD_PRIORITY_TIME_CRITICAL, TIMER_ALL_ACCESS,
    WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VK_CONTROL, VK_LWIN,
    VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, IsHungAppWindow, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL,
    WM_QUIT, WM_USER,
};

use crate::animator::{AxisAnimator, units_for};
use crate::config::{Config, Profile};
use crate::target::{AppCache, is_fullscreen, pid_of, root_window_at};
use crate::win::{hinstance, key_down};

/// Tags the wheel events SilkScroll injects ("SILK").
pub const INJECTED_MARK: usize = 0x5349_4C4B;

const WM_APP_REHOOK: u32 = WM_APP + 1;

#[derive(Clone, Copy)]
enum Axis {
    Vertical,
    Horizontal,
}

enum Command {
    Scroll { axis: Axis, units: f64, profile: Profile },
    Stop,
}

static CONFIG: RwLock<Option<Arc<Config>>> = RwLock::new(None);
static COMMANDS: OnceLock<Sender<Command>> = OnceLock::new();

/// Makes the hook use `cfg` from the next wheel event on.
pub fn set_config(cfg: &Config) {
    if let Ok(mut slot) = CONFIG.write() {
        *slot = Some(Arc::new(cfg.clone()));
    }
    if !cfg.enabled {
        send(Command::Stop);
    }
}

fn current_config() -> Option<Arc<Config>> {
    CONFIG.read().ok()?.clone()
}

fn send(command: Command) -> bool {
    COMMANDS.get().is_some_and(|tx| tx.send(command).is_ok())
}

pub struct Engine {
    hook_thread: u32,
}

/// Starts the animator and the mouse hook on their own threads.
pub fn start(cfg: &Config) -> Engine {
    set_config(cfg);

    let (tx, rx) = mpsc::channel();
    let _ = COMMANDS.set(tx);
    thread::Builder::new()
        .name("animator".into())
        .spawn(move || animator_loop(rx))
        .expect("failed to start the animator thread");

    let (ready_tx, ready_rx) = mpsc::channel();
    thread::Builder::new()
        .name("mouse-hook".into())
        .spawn(move || hook_loop(ready_tx))
        .expect("failed to start the mouse hook thread");

    Engine { hook_thread: ready_rx.recv().unwrap_or(0) }
}

impl Engine {
    /// Installs the hook again. Windows quietly removes a low-level hook that
    /// once answers too slowly (say, while the PC is swapping), so the tray
    /// calls this every few minutes and after unlock or resume.
    pub fn rehook(&self) {
        unsafe { PostThreadMessageW(self.hook_thread, WM_APP_REHOOK, 0, 0) };
    }

    pub fn shutdown(&self) {
        send(Command::Stop);
        unsafe { PostThreadMessageW(self.hook_thread, WM_QUIT, 0, 0) };
    }
}

// ---------------------------------------------------------------------------
// Mouse hook

thread_local! {
    static APPS: RefCell<AppCache> = RefCell::new(AppCache::default());
}

fn hook_loop(ready: Sender<u32>) {
    unsafe {
        // The hook runs on this thread's message loop; any delay here is a
        // delay for every mouse event on the system.
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        let mut msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut msg, null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());

        let mut hook = install_hook();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_APP_REHOOK {
                if !hook.is_null() {
                    UnhookWindowsHookEx(hook);
                }
                hook = install_hook();
            }
        }
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
        }
    }
}

unsafe fn install_hook() -> HHOOK {
    unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), hinstance(), 0) }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let message = wparam as u32;
        if message == WM_MOUSEWHEEL || message == WM_MOUSEHWHEEL {
            let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            if info.dwExtraInfo != INJECTED_MARK {
                let horizontal = message == WM_MOUSEHWHEEL;
                let swallowed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| intercept(horizontal, info)))
                    .unwrap_or(false);
                if swallowed {
                    return 1;
                }
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

/// Decides whether to smooth one wheel event. Returns true if the event was
/// handed to the animator and must not reach the app.
fn intercept(horizontal: bool, info: &MSLLHOOKSTRUCT) -> bool {
    let Some(cfg) = current_config() else { return false };
    if !cfg.enabled {
        return false;
    }

    let delta = (info.mouseData >> 16) as u16 as i16 as i32;
    if delta == 0 {
        return false;
    }
    // Touchpads and free-spinning wheels report fractions of a notch and
    // already scroll smoothly.
    if cfg.ignore_touchpad && delta % 120 != 0 {
        return false;
    }
    // Ctrl/Alt/Win + wheel mean zoom and other shortcuts, not scrolling.
    if [VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].into_iter().any(key_down) {
        return false;
    }

    let Some(root) = root_window_at(info.pt) else { return false };
    if unsafe { IsHungAppWindow(root) } != 0 {
        return false;
    }
    let app = APPS.with(|cache| cache.borrow_mut().get(pid_of(root)));
    if !app.can_inject {
        return false;
    }
    if cfg.skip_fullscreen_apps && is_fullscreen(root) {
        return false;
    }
    let profile = *cfg.profile_for(&app.exe);
    if !profile.enabled {
        return false;
    }

    let (axis, mut delta) = if horizontal {
        if !profile.horizontal_smoothing {
            return false;
        }
        (Axis::Horizontal, delta)
    } else if profile.shift_for_horizontal && key_down(VK_SHIFT) {
        // Wheel up scrolls left, like Shift + wheel in browsers.
        (Axis::Horizontal, -delta)
    } else {
        (Axis::Vertical, delta)
    };
    if profile.reverse_direction {
        delta = -delta;
    }

    send(Command::Scroll { axis, units: units_for(delta, &profile), profile })
}

// ---------------------------------------------------------------------------
// Animator

fn animator_loop(commands: Receiver<Command>) {
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) };
    let timer = FrameTimer::new();
    let clock = Instant::now();
    let now_ms = || clock.elapsed().as_secs_f64() * 1000.0;

    let mut vertical = AxisAnimator::default();
    let mut horizontal = AxisAnimator::default();
    let mut frame = refresh_interval();

    loop {
        if vertical.is_idle() && horizontal.is_idle() {
            // Nothing moving: sleep until the next wheel event.
            let Ok(command) = commands.recv() else { return };
            frame = refresh_interval();
            apply(command, now_ms(), &mut vertical, &mut horizontal);
        }
        while let Ok(command) = commands.try_recv() {
            apply(command, now_ms(), &mut vertical, &mut horizontal);
        }

        timer.sleep(frame);
        let now = now_ms();
        inject(vertical.tick(now), horizontal.tick(now));
    }
}

fn apply(command: Command, now: f64, vertical: &mut AxisAnimator, horizontal: &mut AxisAnimator) {
    match command {
        Command::Scroll { axis: Axis::Vertical, units, profile } => vertical.push(now, units, &profile),
        Command::Scroll { axis: Axis::Horizontal, units, profile } => horizontal.push(now, units, &profile),
        Command::Stop => {
            vertical.stop();
            horizontal.stop();
        }
    }
}

fn inject(vertical: i32, horizontal: i32) {
    let mut inputs = [INPUT::default(); 2];
    let mut count = 0;
    for (amount, flag) in [(vertical, MOUSEEVENTF_WHEEL), (horizontal, MOUSEEVENTF_HWHEEL)] {
        if amount != 0 {
            inputs[count] = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: amount as u32,
                        dwFlags: flag,
                        time: 0,
                        dwExtraInfo: INJECTED_MARK,
                    },
                },
            };
            count += 1;
        }
    }
    if count > 0 {
        unsafe { SendInput(count as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    }
}

/// One frame of the main display, so the scroll updates as often as the
/// screen can show them.
fn refresh_interval() -> Duration {
    let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
    mode.dmSize = size_of::<DEVMODEW>() as u16;
    let ok = unsafe { EnumDisplaySettingsW(std::ptr::null(), ENUM_CURRENT_SETTINGS, &mut mode) } != 0;
    let hz = if ok && mode.dmDisplayFrequency > 1 { mode.dmDisplayFrequency } else { 60 };
    Duration::from_secs_f64(1.0 / hz.clamp(60, 360) as f64)
}

/// Sleeps with sub-millisecond precision where Windows supports it.
struct FrameTimer(HANDLE);

impl FrameTimer {
    fn new() -> Self {
        let handle = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        if handle.is_null() {
            // Older Windows: make thread::sleep accurate to about 1 ms.
            unsafe { timeBeginPeriod(1) };
        }
        FrameTimer(handle)
    }

    fn sleep(&self, duration: Duration) {
        if self.0.is_null() {
            thread::sleep(duration);
            return;
        }
        let due = -((duration.as_nanos() / 100) as i64);
        unsafe {
            SetWaitableTimer(self.0, &due, 0, None, std::ptr::null(), 0);
            WaitForSingleObject(self.0, INFINITE);
        }
    }
}
