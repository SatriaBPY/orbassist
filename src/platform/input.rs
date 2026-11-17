//! Input sintetis dan deteksi status layar penuh.

use std::mem::size_of;

use thiserror::Error;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, SendInput, VIRTUAL_KEY, VK_CONTROL,
    VK_LWIN, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
    ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowThreadProcessId,
    SW_SHOWNORMAL,
};

use crate::platform::hotkey::Hotkey;
use crate::platform::window;

#[derive(Debug, Error)]
pub enum InputError {
    #[error("Windows menolak membuka '{target}' (kode {code})")]
    ShellExecute { target: String, code: isize },
    #[error("input keyboard ditolak Windows (mungkin diblokir aplikasi elevated)")]
    SendInputRejected,
    #[error("gagal mengunci layar: {0}")]
    LockFailed(String),
}

/// Membuka berkas/folder/URL memakai shell Windows.
pub fn shell_open(target: &str, parameters: Option<&str>) -> Result<(), InputError> {
    let operation = windows::core::HSTRING::from("open");
    let file = windows::core::HSTRING::from(target);
    let params = parameters.map(windows::core::HSTRING::from);
    let empty = windows::core::HSTRING::from("");

    // SAFETY: semua string hidup selama panggilan; ShellExecuteW aman dipanggil
    // dari thread UI.
    let result = unsafe {
        match params {
            Some(params) => ShellExecuteW(None, &operation, &file, &params, &empty, SW_SHOWNORMAL),
            None => ShellExecuteW(
                None,
                &operation,
                &file,
                windows::core::PCWSTR::null(),
                &empty,
                SW_SHOWNORMAL,
            ),
        }
    };

    let code = result.0 as isize;
    if code <= 32 {
        return Err(InputError::ShellExecute {
            target: target.to_string(),
            code,
        });
    }
    Ok(())
}

fn key_event(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Mengirim kombinasi tombol (tanpa menahan tombol setelahnya).
pub fn send_combo(hotkey: Hotkey) -> Result<(), InputError> {
    let modifiers = [
        (MOD_CONTROL.0, VK_CONTROL.0),
        (MOD_ALT.0, VK_MENU.0),
        (MOD_SHIFT.0, VK_SHIFT.0),
        (MOD_WIN.0, VK_LWIN.0),
    ];

    let mut pressed = Vec::new();
    for (flag, vk) in modifiers {
        if hotkey.modifiers & flag != 0 {
            pressed.push(vk);
        }
    }

    let mut inputs = Vec::with_capacity(pressed.len() * 2 + 2);
    for vk in &pressed {
        inputs.push(key_event(*vk, false));
    }
    inputs.push(key_event(hotkey.vk, false));
    inputs.push(key_event(hotkey.vk, true));
    for vk in pressed.iter().rev() {
        inputs.push(key_event(*vk, true));
    }

    // SAFETY: slice INPUT hidup selama panggilan dan ukurannya benar.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(InputError::SendInputRejected);
    }
    Ok(())
}

/// Mengirim satu tombol virtual (tekan lalu lepas).
pub fn send_key(vk: u16) -> Result<(), InputError> {
    let inputs = [key_event(vk, false), key_event(vk, true)];
    // SAFETY: slice hidup selama panggilan.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(InputError::SendInputRejected);
    }
    Ok(())
}

/// Mengunci workstation.
pub fn lock_workstation() -> Result<(), InputError> {
    // SAFETY: LockWorkStation tidak memerlukan argumen.
    unsafe { windows::Win32::System::Shutdown::LockWorkStation() }
        .map_err(|err| InputError::LockFailed(format!("{err}")))
}

/// Status tombol (true jika sedang ditekan).
pub fn is_key_down(vk: u16) -> bool {
    // SAFETY: GetAsyncKeyState tidak menyentuh memori pemanggil.
    let state = unsafe { GetAsyncKeyState(i32::from(vk)) };
    (state as u16 & 0x8000) != 0
}

/// Apakah Orb sebaiknya bersembunyi karena presentasi/video layar penuh.
pub fn should_hide_for_fullscreen() -> bool {
    // SAFETY: SHQueryUserNotificationState tidak memerlukan argumen.
    if let Ok(state) = unsafe { SHQueryUserNotificationState() }
        && (state == QUNS_BUSY
            || state == QUNS_PRESENTATION_MODE
            || state == QUNS_RUNNING_D3D_FULL_SCREEN)
    {
        return true;
    }
    foreground_covers_monitor()
}

fn foreground_covers_monitor() -> bool {
    // SAFETY: fungsi pengambil handle tidak menyentuh memori pemanggil.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_invalid() {
        return false;
    }
    // SAFETY: membandingkan handle dengan jendela shell/desktop.
    let shell = unsafe { GetShellWindow() };
    // SAFETY: GetDesktopWindow tidak memerlukan argumen.
    let desktop = unsafe { GetDesktopWindow() };
    if foreground == shell || foreground == desktop {
        return false;
    }

    // Jendela Orb sendiri (misalnya form pengaturan) tidak dihitung fullscreen.
    let mut process_id = 0u32;
    // SAFETY: process_id menunjuk ke variabel stack yang valid.
    unsafe {
        let _ = GetWindowThreadProcessId(foreground, Some(&mut process_id));
    }
    // SAFETY: GetCurrentProcessId tidak memerlukan argumen.
    if process_id == unsafe { GetCurrentProcessId() } {
        return false;
    }

    let class = window_class_name(foreground);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
        return false;
    }

    let rect = window::window_rect(foreground);
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return false;
    }

    // SAFETY: monitor dihitung dari jendela yang valid.
    let monitor = unsafe { MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: info memiliki cbSize benar.
    if !unsafe { windows::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return false;
    }

    let monitor_rect = info.rcMonitor;
    rect.left <= monitor_rect.left
        && rect.top <= monitor_rect.top
        && rect.right >= monitor_rect.right
        && rect.bottom >= monitor_rect.bottom
}

fn window_class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    // SAFETY: buffer milik pemanggil dengan panjang yang benar.
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let len = usize::try_from(len.max(0)).unwrap_or(0);
    String::from_utf16_lossy(&buffer[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_event_sets_keyup_flag() {
        let down = key_event(0x41, false);
        let up = key_event(0x41, true);
        // SAFETY: membaca union yang baru diisi oleh `key_event`.
        unsafe {
            assert_eq!(down.Anonymous.ki.dwFlags.0, 0);
            assert_eq!(up.Anonymous.ki.dwFlags.0, KEYEVENTF_KEYUP.0);
            assert_eq!(down.Anonymous.ki.wVk.0, 0x41);
        }
    }
}
