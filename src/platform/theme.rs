//! Visual styles modern (comctl32 v6), chrome jendela, dan font UI.
//!
//! Tanpa *application manifest* tertanam, kontrol Win32 digambar dengan gaya
//! klasik Windows 95. Modul ini mengaktifkan visual styles lewat *activation
//! context* yang dibuat saat runtime dari berkas manifest kecil, lalu mengatur
//! title bar gelap dan sudut membulat lewat DWM.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;

use windows::core::{HSTRING, PCWSTR, w};
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND};
use windows::Win32::Graphics::Dwm::{
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, HFONT,
    OUT_DEFAULT_PRECIS,
};
use windows::Win32::System::ApplicationInstallationAndServicing::{
    ACTCTXW, ActivateActCtx, CreateActCtxW, ReleaseActCtx,
};
use windows::Win32::UI::Controls::SetWindowTheme;

/// Manifest minimal: cukup untuk memuat Common Controls v6.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="Orb.App" version="1.0.0.0" processorArchitecture="*"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
                        version="6.0.0.0" processorArchitecture="*"
                        publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

thread_local! {
    /// Handle activation context yang sengaja dibiarkan aktif sampai proses berakhir.
    static ACT_CTX: Cell<Option<(HANDLE, usize)>> = const { Cell::new(None) };
    /// Cache font per DPI agar tidak membuat font baru setiap kontrol.
    static FONTS: RefCell<Vec<(u32, HFONT)>> = const { RefCell::new(Vec::new()) };
}

/// Berkas manifest ditulis di folder data pengguna, bukan di folder instalasi
/// (yang belum tentu bisa ditulis).
fn manifest_path() -> PathBuf {
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Orb");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("orb.manifest")
}

/// Mengaktifkan visual styles. Aman dipanggil lebih dari sekali; kegagalan
/// hanya dicatat di log dan aplikasi tetap berjalan dengan gaya klasik.
pub fn enable_visual_styles() {
    if ACT_CTX.with(|cell| cell.get().is_some()) {
        return;
    }

    let path = manifest_path();
    if let Err(err) = std::fs::write(&path, MANIFEST) {
        log::warn!("manifest visual styles tidak bisa ditulis ke {}: {err}", path.display());
        return;
    }

    let wide = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
    let context = ACTCTXW {
        cbSize: size_of::<ACTCTXW>() as u32,
        lpSource: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };

    // SAFETY: ACTCTXW terisi dan string sumber hidup selama panggilan.
    let handle = match unsafe { CreateActCtxW(&context) } {
        Ok(handle) => handle,
        Err(err) => {
            log::warn!("activation context visual styles gagal dibuat: {err}");
            return;
        }
    };

    let mut cookie = 0usize;
    // SAFETY: handle valid dari CreateActCtxW dan cookie ditulis ke variabel stack.
    if let Err(err) = unsafe { ActivateActCtx(Some(handle), &mut cookie) } {
        log::warn!("activation context visual styles gagal diaktifkan: {err}");
        // SAFETY: handle belum dipakai lagi.
        unsafe { ReleaseActCtx(handle) };
        return;
    }

    ACT_CTX.with(|cell| cell.set(Some((handle, cookie))));
    log::info!("visual styles comctl32 v6 aktif");
}

/// Title bar gelap (Windows 10 1903+), sudut membulat (Windows 11), dan tema
/// kontrol gelap. Error DWM diabaikan karena Windows 10 lama tidak mendukung.
pub fn apply_window_chrome(hwnd: HWND, dark: bool) {
    let dark_flag: i32 = i32::from(dark);
    let corner: i32 = DWMWCP_ROUND.0;

    // SAFETY: pointer menunjuk nilai stack dengan ukuran yang dilaporkan benar.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_flag as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        );
    }

    if dark {
        set_dark_theme(hwnd);
    }
}

/// Meminta uxtheme memakai varian gelap untuk scrollbar dan kontrol.
pub fn set_dark_theme(hwnd: HWND) {
    // SAFETY: hwnd valid; string statis hidup selama proses.
    let _ = unsafe { SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null()) };
}

/// Font UI modern (Segoe UI) untuk DPI tertentu. Font di-cache per DPI dan
/// tidak dihapus selama proses hidup, sehingga aman dipakai banyak kontrol.
pub fn ui_font(dpi: u32) -> HFONT {
    let dpi = dpi.max(96);
    if let Some(font) = FONTS.with(|fonts| {
        fonts
            .borrow()
            .iter()
            .find(|(cached, _)| *cached == dpi)
            .map(|(_, font)| *font)
    }) {
        return font;
    }

    // 9 pt pada 96 DPI kira-kira 12 px.
    let height = -((12.0 * dpi as f32 / 96.0).round() as i32);
    // SAFETY: semua argumen valid dan nama font statis.
    let font = unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI"),
        )
    };

    FONTS.with(|fonts| fonts.borrow_mut().push((dpi, font)));
    font
}

/// Menggambar ulang seluruh jendela.
pub fn repaint(hwnd: HWND) {
    // SAFETY: hwnd valid milik proses ini.
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, true);
    }
}

/// Warna RGB ke COLORREF Win32 (0x00BBGGRR).
pub const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}
