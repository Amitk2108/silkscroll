//! "Start with Windows": a value under the current user's Run key.

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

use crate::win::{exe_path, from_wide, wide};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "SilkScroll";

/// Adds or removes the Run entry so it matches `enabled`, and keeps it
/// pointing at the exe that is running now.
pub fn sync(enabled: bool) {
    let wanted = format!("\"{}\"", exe_path().display());
    let current = read();
    if enabled {
        if current.as_deref() != Some(wanted.as_str()) {
            write(&wanted);
        }
    } else if current.is_some() {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(VALUE_NAME).as_ptr()) };
    }
}

fn read() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide(VALUE_NAME).as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then(|| from_wide(&buf))
}

fn write(command: &str) {
    let data = wide(command);
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide(VALUE_NAME).as_ptr(),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        );
    }
}
