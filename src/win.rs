//! Small helpers shared by the Win32 modules.

use std::path::PathBuf;
use std::process::Command;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, HICON, LR_DEFAULTCOLOR, MessageBoxW, SW_SHOWNORMAL,
};

use crate::icon;

/// NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads a UTF-16 buffer up to its first NUL.
pub fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

pub fn hinstance() -> HINSTANCE {
    unsafe { GetModuleHandleW(std::ptr::null()) }
}

pub fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

pub fn message_box(owner: HWND, text: &str, title: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(owner, wide(text).as_ptr(), wide(title).as_ptr(), flags) }
}

/// Opens a folder, file or URL with its default handler.
pub fn shell_open(target: &str) {
    unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            wide(target).as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("silkscroll.exe"))
}

/// Starts another copy of this exe, e.g. with `--settings`.
pub fn spawn_self(arg: Option<&str>) {
    let mut cmd = Command::new(exe_path());
    if let Some(arg) = arg {
        cmd.arg(arg);
    }
    let _ = cmd.spawn();
}

/// The app icon rendered at exactly `size` pixels.
pub fn make_icon(size: u32, active: bool) -> HICON {
    let bits = icon::dib(size, active);
    unsafe {
        CreateIconFromResourceEx(
            bits.as_ptr(),
            bits.len() as u32,
            1,
            0x0003_0000,
            size as i32,
            size as i32,
            LR_DEFAULTCOLOR,
        )
    }
}

/// Scales a length designed at 96 DPI to `dpi`.
pub fn scale(value: i32, dpi: u32) -> i32 {
    (value as i64 * dpi as i64 / 96) as i32
}
