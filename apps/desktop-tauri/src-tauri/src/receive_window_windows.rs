//! Native popup attributes must be reasserted even when Tao's cache says true.
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, WS_EX_NOACTIVATE,
    },
};

pub fn configure(window: &tauri::WebviewWindow) {
    if let Ok(handle) = window.hwnd() {
        unsafe {
            let style = GetWindowLongPtrW(handle.0, GWL_EXSTYLE);
            SetWindowLongPtrW(handle.0, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
        }
    }
}

pub fn show(window: &tauri::WebviewWindow) -> Result<(), String> {
    let handle = window.hwnd().map_err(|e| e.to_string())?;
    unsafe { show_native(handle.0) }
}

unsafe fn show_native(hwnd: HWND) -> Result<(), String> {
    if SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
    ) == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetForegroundWindow, ShowWindow, HWND_NOTOPMOST, SW_HIDE,
        WS_EX_TOPMOST, WS_POPUP,
    };

    #[test]
    fn topmost_is_restored_after_native_attribute_loss_without_taking_focus() {
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                50,
                50,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            let foreground = GetForegroundWindow();
            for _ in 0..30 {
                ShowWindow(hwnd, SW_HIDE);
                assert_ne!(
                    SetWindowPos(
                        hwnd,
                        HWND_NOTOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE
                    ),
                    0
                );
                assert_eq!(
                    GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST as isize,
                    0
                );
                show_native(hwnd).unwrap();
                assert_ne!(
                    GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST as isize,
                    0
                );
                assert_ne!(
                    GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_NOACTIVATE as isize,
                    0
                );
                assert_eq!(GetForegroundWindow(), foreground);
            }
            DestroyWindow(hwnd);
        }
    }
}
