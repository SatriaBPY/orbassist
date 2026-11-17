//! Ikon system tray (Shell_NotifyIcon).

use std::mem::size_of;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject, HGDIOBJ,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONDATAW_0, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO};

use crate::platform::window;

/// Ikon tray milik Orb. Ikon dilepas saat `Tray` di-drop.
pub struct Tray {
    hwnd: HWND,
    icon: HICON,
}

const TRAY_ID: u32 = 1;

impl Tray {
    pub fn install(hwnd: HWND, icon: HICON, tooltip: &str) -> Result<Self> {
        let mut data = shell_data(hwnd, icon);
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = window::WM_ORB_TRAY;
        copy_tip(&mut data, tooltip);

        // SAFETY: struct NOTIFYICONDATAW valid dengan cbSize yang benar.
        if !unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
            // Ikon gagal dipasang: tetap lepas ikon agar tidak bocor.
            // SAFETY: ikon milik pemanggil.
            unsafe {
                let _ = DestroyIcon(icon);
            }
            bail!("Shell_NotifyIconW(NIM_ADD) gagal");
        }

        data.Anonymous = NOTIFYICONDATAW_0 {
            uVersion: NOTIFYICON_VERSION_4,
        };
        // SAFETY: data yang sama masih berlaku.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
        }

        Ok(Self { hwnd, icon })
    }

    pub fn set_tooltip(&self, tooltip: &str) {
        let mut data = shell_data(self.hwnd, self.icon);
        data.uFlags = NIF_TIP | NIF_SHOWTIP;
        copy_tip(&mut data, tooltip);
        // SAFETY: struct valid untuk NIM_MODIFY.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let data = shell_data(self.hwnd, self.icon);
        // SAFETY: menghapus ikon yang kita pasang; DestroyIcon untuk ikon kita.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            let _ = DestroyIcon(self.icon);
        }
    }
}

fn shell_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        hIcon: icon,
        ..Default::default()
    }
}

fn copy_tip(data: &mut NOTIFYICONDATAW, tooltip: &str) {
    for (target, source) in data.szTip.iter_mut().zip(tooltip.encode_utf16().take(127)) {
        *target = source;
    }
}

/// Membuat `HICON` dari piksel BGRA (straight alpha, baris atas dulu).
pub fn create_icon_from_bgra(width: i32, height: i32, pixels: &[u8]) -> Result<HICON> {
    let expected = (width * height * 4) as usize;
    if width <= 0 || height <= 0 || pixels.len() < expected {
        bail!("data ikon tidak sesuai ukuran {width}x{height}");
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: bmi valid; bits menerima pointer memori gambar.
    let color = unsafe { CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) }
        .context("CreateDIBSection untuk ikon gagal")?;
    if bits.is_null() {
        // SAFETY: bitmap baru dibuat.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        bail!("pointer piksel ikon null");
    }

    // SAFETY: memori bitmap cukup untuk width*height*4 byte.
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), expected);
    }

    // SAFETY: membuat bitmap mask 1-bit; isi NULL berarti semua piksel terlihat.
    let mask = unsafe { CreateBitmap(width, height, 1, 1, None) };

    let info = ICONINFO {
        fIcon: windows::core::BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };

    // SAFETY: ICONINFO berisi bitmap valid; Windows menyalin isinya.
    let icon = unsafe { CreateIconIndirect(&info) };

    // SAFETY: bitmap tidak dipakai lagi setelah CreateIconIndirect menyalinnya.
    unsafe {
        let _ = DeleteObject(HGDIOBJ(mask.0));
        let _ = DeleteObject(HGDIOBJ(color.0));
    }

    icon.context("CreateIconIndirect gagal")
}
