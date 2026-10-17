//! Wheel probe: a diagnostic window that reports every wheel event it gets.
//!
//! Run it while SilkScroll is running and scroll over the window, or let it
//! simulate the scrolling:
//!
//! ```text
//! cargo run --example wheel_probe -- --inject 1 --seconds 2
//! ```
//!
//! `--inject N` moves the mouse pointer into the probe window and sends N
//! ordinary wheel notches, as if you had turned the wheel. Without
//! SilkScroll the window receives N events of 120; with it, each notch arrives
//! as many small events that still add up to 120.

use std::cell::{Cell, RefCell};
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

thread_local! {
    /// Every wheel event received, as (milliseconds since start, delta).
    static EVENTS: RefCell<Vec<(f64, i32)>> = const { RefCell::new(Vec::new()) };
    static START: Cell<Option<Instant>> = const { Cell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_MOUSEWHEEL {
        let delta = ((wparam >> 16) & 0xFFFF) as u16 as i16 as i32;
        let at = START.get().map(|s| s.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
        EVENTS.with_borrow_mut(|events| events.push((at, delta)));
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn arg(name: &str, default: f64) -> f64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn send_notch() {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 120, dwFlags: MOUSEEVENTF_WHEEL, time: 0, dwExtraInfo: 0 },
        },
    };
    unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
}

fn main() {
    let inject = arg("--inject", 0.0) as u32;
    let seconds = arg("--seconds", 10.0);

    unsafe {
        let class = wide("SilkScrollWheelProbe");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: GetModuleHandleW(std::ptr::null()),
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            class.as_ptr(),
            wide("SilkScroll wheel probe - scroll here").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200,
            200,
            480,
            320,
            null_mut(),
            null_mut(),
            wc.hInstance,
            std::ptr::null(),
        );
        SetForegroundWindow(hwnd);

        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect);
        START.set(Some(Instant::now()));
        let deadline = Instant::now() + Duration::from_secs_f64(seconds);
        let mut injected = 0;
        let mut next_inject = Instant::now() + Duration::from_millis(300);

        let mut msg: MSG = std::mem::zeroed();
        while Instant::now() < deadline {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if injected < inject && Instant::now() >= next_inject {
                SetCursorPos((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
                send_notch();
                injected += 1;
                next_inject = Instant::now() + Duration::from_millis(600);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        DestroyWindow(hwnd);

        let events = EVENTS.take();
        let total: i32 = events.iter().map(|e| e.1).sum();
        let span = match (events.first(), events.last()) {
            (Some(a), Some(b)) => b.0 - a.0,
            _ => 0.0,
        };
        println!("events={} total_delta={} span_ms={:.0}", events.len(), total, span);
        let preview: Vec<String> = events.iter().take(40).map(|e| e.1.to_string()).collect();
        println!("first deltas: {}", preview.join(" "));
    }
}
