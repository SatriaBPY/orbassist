//! Pembungkus aman untuk jendela dan utilitas Win32 yang dipakai Orb.

use std::mem::size_of;
use std::sync::atomic::{AtomicIsize, Ordering};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, BM_GETCHECK, BM_SETCHECK, CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CS_HREDRAW,
    CS_VREDRAW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetDlgItem, GetMessageW, GetWindowRect, GetWindowTextW, HCURSOR, HMENU, HWND_TOPMOST,
    IDC_ARROW, IsWindowVisible, LoadCursorW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK,
    MENU_ITEM_FLAGS, MESSAGEBOX_STYLE, MF_SEPARATOR, MSG, MessageBoxW, PostMessageW,
    PostQuitMessage, RegisterClassExW, SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_SETFONT, WNDCLASSEXW, WNDPROC,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WS_VISIBLE,
};
use windows::core::{HSTRING, PCWSTR};

/// Pesan jendela khusus Orb.
pub const WM_ORB_FOREGROUND: u32 = WM_APP + 1;
pub const WM_ORB_TRAY: u32 = WM_APP + 2;
pub const WM_ORB_SETTINGS_SAVED: u32 = WM_APP + 3;

/// ID timer jendela bola.
pub const TIMER_ANIM: usize = 1;
pub const TIMER_WATCH: usize = 2;
pub const TIMER_MENU: usize = 3;

/// ID hotkey global.
pub const HOTKEY_MENU_ID: i32 = 1;
pub const HOTKEY_FIND_ID: i32 = 2;

/// Status `BM_GETCHECK` untuk checkbox tercentang (`BST_CHECKED`).
const BST_CHECKED: isize = 1;

/// Mengaktifkan DPI awareness per-monitor agar ukuran bola mengikuti layar.
pub fn init_process_dpi() {
    // SAFETY: dipanggil sebelum jendela pertama dibuat.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_err()
    {
        log::warn!("DPI awareness per-monitor tidak tersedia; memakai skala sistem");
    }
}

fn module_handle() -> Result<windows::Win32::Foundation::HINSTANCE> {
    // SAFETY: meminta handle modul executable saat ini.
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW gagal")?;
    Ok(windows::Win32::Foundation::HINSTANCE(module.0))
}

/// Mendaftarkan kelas jendela.
pub fn register_class(name: &str, proc: WNDPROC) -> Result<()> {
    let class_name = HSTRING::from(name);
    let instance = module_handle()?;
    // SAFETY: memuat kursor standar.
    let cursor: HCURSOR = unsafe { LoadCursorW(None, IDC_ARROW) }.context("LoadCursorW gagal")?;

    let window_class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
        lpfnWndProc: proc,
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };

    // SAFETY: WNDCLASSEXW terisi lengkap dan nama kelas hidup selama panggilan.
    let atom = unsafe { RegisterClassExW(&window_class) };
    if atom == 0 {
        bail!("RegisterClassExW gagal untuk kelas {name}");
    }
    Ok(())
}

fn create_window(
    class: &str,
    title: &str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    width: i32,
    height: i32,
) -> Result<HWND> {
    let class = HSTRING::from(class);
    let title = HSTRING::from(title);
    let instance = module_handle()?;
    // SAFETY: kelas sudah didaftarkan; semua argumen valid.
    unsafe {
        CreateWindowExW(
            ex_style,
            &class,
            &title,
            style,
            0,
            0,
            width,
            height,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .context("CreateWindowExW gagal")
}

/// Membuat jendela berlapis yang tidak pernah mengambil fokus.
pub fn create_layered_window(class: &str, title: &str, width: i32, height: i32) -> Result<HWND> {
    let ex_style = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE;
    create_window(class, title, WS_POPUP, ex_style, width, height)
}

/// Jendela aplikasi biasa (dipakai form pengaturan).
pub fn create_app_window(class: &str, title: &str, width: i32, height: i32) -> Result<HWND> {
    let style = windows::Win32::UI::WindowsAndMessaging::WS_OVERLAPPED
        | windows::Win32::UI::WindowsAndMessaging::WS_CAPTION
        | windows::Win32::UI::WindowsAndMessaging::WS_SYSMENU;
    create_window(class, title, style, WINDOW_EX_STYLE(0), width, height)
}

/// Menampilkan jendela dan memberinya fokus.
pub fn focus(hwnd: HWND) {
    // SAFETY: hwnd valid milik proses ini.
    unsafe {
        let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Skala DPI sistem saat jendela belum dibuat.
pub fn system_scale() -> f32 {
    // SAFETY: tidak memerlukan handle.
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

/// Memusatkan jendela di monitor tempat kursor berada.
pub fn center_on_cursor(hwnd: HWND) {
    let rect = window_rect(hwnd);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let area = work_area(cursor_pos());
    let x = area.left + ((area.right - area.left - width) / 2).max(0);
    let y = area.top + ((area.bottom - area.top - height) / 2).max(0);
    move_to(hwnd, x, y);
}

/// Jendela tersembunyi (dipakai untuk ikon tray dan pengaturan).
pub fn create_hidden_window(class: &str, title: &str) -> Result<HWND> {
    create_window(class, title, WS_POPUP, WINDOW_EX_STYLE(0), 0, 0)
}

/// Membuat kontrol anak (EDIT, BUTTON, COMBOBOX, STATIC).
pub fn create_child(
    parent: HWND,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    rect: (i32, i32, i32, i32),
    id: i32,
) -> Result<HWND> {
    let class = HSTRING::from(class);
    let text = HSTRING::from(text);
    let instance = module_handle()?;
    let (x, y, width, height) = rect;
    // ID kontrol dikirim lewat parameter menu, sesuai cara `GetDlgItem` bekerja.
    let control_id = HMENU(id as usize as *mut core::ffi::c_void);
    // SAFETY: parent valid; semua argumen milik pemanggil dan hidup selama panggilan.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            &class,
            &text,
            style | WS_CHILD | WS_VISIBLE,
            x,
            y,
            width,
            height,
            Some(parent),
            Some(control_id),
            Some(instance),
            None,
        )
    }
    .context("membuat kontrol anak gagal")
}

/// Menampilkan jendela tanpa mengambil fokus.
pub fn show_no_activate(hwnd: HWND) {
    // SAFETY: hwnd valid milik proses ini.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNA);
    }
    raise(hwnd);
}

/// Menyembunyikan jendela.
pub fn hide(hwnd: HWND) {
    // SAFETY: hwnd valid milik proses ini.
    unsafe {
        let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    // SAFETY: hwnd valid milik proses ini.
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

/// Menaikkan jendela ke z-order paling atas tanpa mengubah fokus.
pub fn raise(hwnd: HWND) {
    // SAFETY: hwnd valid; SWP_NOACTIVATE menjaga fokus aplikasi lain.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Memindahkan jendela berlapis; ukuran tidak berubah.
pub fn move_to(hwnd: HWND, x: i32, y: i32) {
    // SAFETY: hwnd valid; ukuran dan z-order tidak diubah.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

/// Posisi kursor dalam koordinat layar fisik.
pub fn cursor_pos() -> POINT {
    let mut point = POINT::default();
    // SAFETY: point menunjuk ke variabel stack yang valid.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point);
    }
    point
}

/// Area kerja (tanpa taskbar) dari monitor tempat `point` berada.
pub fn work_area(point: POINT) -> RECT {
    // SAFETY: monitor dan info diisi oleh Windows.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: info memiliki cbSize yang benar.
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        info.rcWork
    } else {
        RECT {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 720,
        }
    }
}

/// Skala DPI jendela (1.0 = 96 DPI).
pub fn dpi_scale(hwnd: HWND) -> f32 {
    // SAFETY: hwnd valid milik proses ini.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

pub fn window_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    // SAFETY: rect menunjuk ke variabel stack yang valid.
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    rect
}

pub fn set_timer(hwnd: HWND, id: usize, milliseconds: u32) {
    // SAFETY: hwnd valid; TIMERPROC NULL berarti WM_TIMER dikirim ke jendela.
    unsafe {
        let _ = SetTimer(Some(hwnd), id, milliseconds, None);
    }
}

pub fn kill_timer(hwnd: HWND, id: usize) {
    // SAFETY: hwnd valid.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), id);
    }
}

/// Menangkap mouse agar drag tetap berjalan di luar jendela.
pub fn capture_mouse(hwnd: HWND) {
    // SAFETY: hwnd valid; capture dilepas pada mouse up.
    unsafe {
        SetCapture(hwnd);
    }
}

pub fn release_mouse() {
    // SAFETY: melepas capture milik thread ini jika ada.
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
    }
}

static BALL_TARGET: AtomicIsize = AtomicIsize::new(0);

/// Memasang hook `EVENT_SYSTEM_FOREGROUND` sekali per proses.
pub fn install_foreground_hook(ball_window: HWND) -> Result<HWINEVENTHOOK> {
    BALL_TARGET.store(ball_window.0 as isize, Ordering::Relaxed);
    // SAFETY: callback adalah fungsi `extern "system"` statis yang tidak menangkap
    // state Rust; callback hanya mengirim pesan.
    let hook = unsafe {
        SetWinEventHook(
            windows::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_FOREGROUND,
            windows::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_callback),
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::WINEVENT_OUTOFCONTEXT
                | windows::Win32::UI::WindowsAndMessaging::WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.is_invalid() {
        bail!("SetWinEventHook gagal");
    }
    Ok(hook)
}

pub fn uninstall_hook(hook: HWINEVENTHOOK) {
    // SAFETY: hook berasal dari `install_foreground_hook` dan belum dilepas.
    unsafe {
        let _ = UnhookWinEvent(hook);
    }
}

unsafe extern "system" fn foreground_callback(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let raw = BALL_TARGET.load(Ordering::Relaxed);
    if raw == 0 {
        return;
    }
    // SAFETY: HWND disimpan saat hook dipasang dan jendela hidup sampai aplikasi
    // keluar; PostMessageW aman dipanggil dari callback.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(raw as *mut core::ffi::c_void)),
            WM_ORB_FOREGROUND,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

/// Menampilkan pesan ke pengguna.
pub fn message_box(owner: Option<HWND>, title: &str, text: &str, is_error: bool) {
    let style: MESSAGEBOX_STYLE = if is_error {
        MB_OK | MB_ICONERROR
    } else {
        MB_OK | MB_ICONINFORMATION
    };
    let title = HSTRING::from(title);
    let text = HSTRING::from(text);
    // SAFETY: string hidup selama panggilan; MessageBox milik thread UI.
    unsafe {
        MessageBoxW(owner, &text, &title, style);
    }
}

/// Popup menu sederhana; mengembalikan id item yang dipilih.
pub fn popup_menu(hwnd: HWND, items: &[(usize, &str)]) -> Option<usize> {
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    for (index, item) in items {
        let text = HSTRING::from(*item);
        if item.is_empty() {
            // SAFETY: menu valid.
            unsafe {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            }
        } else {
            // SAFETY: menu dan string hidup selama panggilan.
            unsafe {
                let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0), *index, &text);
            }
        }
    }

    let point = cursor_pos();
    // SAFETY: menu valid; TPM_RETURNCMD mengembalikan id item langsung.
    let command = unsafe {
        let _ = SetForegroundWindow(hwnd);
        let result = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), 0, WPARAM(0), LPARAM(0));
        result.0
    };
    // SAFETY: menu tidak dipakai lagi.
    unsafe {
        let _ = DestroyMenu(menu);
    }
    if command == 0 {
        None
    } else {
        Some(command as usize)
    }
}

/// Teks kontrol anak.
pub fn child_text(parent: HWND, id: i32) -> String {
    let Ok(child) = (unsafe { GetDlgItem(Some(parent), id) }) else {
        return String::new();
    };
    let mut buffer = [0u16; 512];
    // SAFETY: buffer milik pemanggil dengan panjang yang benar.
    let len = unsafe { GetWindowTextW(child, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

pub fn combo_index(parent: HWND, id: i32) -> i32 {
    let Ok(child) = (unsafe { GetDlgItem(Some(parent), id) }) else {
        return 0;
    };
    // SAFETY: child adalah COMBOBOX.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(child, CB_GETCURSEL, None, None).0
            as i32
    }
}

pub fn set_combo_index(parent: HWND, id: i32, index: i32) {
    let Ok(child) = (unsafe { GetDlgItem(Some(parent), id) }) else {
        return;
    };
    // SAFETY: child adalah COMBOBOX.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            child,
            CB_SETCURSEL,
            Some(WPARAM(index as usize)),
            None,
        );
    }
}

pub fn combo_add(parent: HWND, id: i32, text: &str) {
    let Ok(child) = (unsafe { GetDlgItem(Some(parent), id) }) else {
        return;
    };
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: child adalah COMBOBOX dan string hidup selama panggilan.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            child,
            CB_ADDSTRING,
            None,
            Some(LPARAM(wide.as_ptr() as isize)),
        );
    }
}

pub fn is_checked(parent: HWND, id: i32) -> bool {
    let Ok(child) = (unsafe { GetDlgItem(Some(parent), id) }) else {
        return false;
    };
    // SAFETY: child adalah tombol checkbox.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(child, BM_GETCHECK, None, None).0
            == BST_CHECKED
    }
}

pub fn set_checked(parent: HWND, id: i32, checked: bool) {
    let Ok(child) = (unsafe { GetDlgItem(Some(parent), id) }) else {
        return;
    };
    let state = if checked { BST_CHECKED as usize } else { 0 };
    // SAFETY: child adalah tombol checkbox.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            child,
            BM_SETCHECK,
            Some(WPARAM(state)),
            None,
        );
    }
}

/// Menerapkan font UI default ke kontrol anak.
pub fn apply_font(control: HWND) {
    // SAFETY: mengambil font GUI sistem; tidak perlu dihapus pemanggil.
    let font = unsafe {
        windows::Win32::Graphics::Gdi::GetStockObject(
            windows::Win32::Graphics::Gdi::DEFAULT_GUI_FONT,
        )
    };
    // SAFETY: control valid.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            control,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

/// Mencari jendela yang sudah ada berdasarkan kelas.
pub fn find_window(class: &str) -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    let class = HSTRING::from(class);
    // SAFETY: nama kelas hidup selama panggilan.
    unsafe { FindWindowW(&class, PCWSTR::null()) }.ok()
}

/// Jalur pesan default untuk jendela yang tidak menangani pesan.
pub fn default_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    // SAFETY: meneruskan pesan apa adanya ke Windows.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam).0 }
}

/// Loop pesan utama. `on_thread_message` menerima pesan tanpa jendela
/// (misalnya `WM_HOTKEY`).
pub fn run_message_loop(mut on_thread_message: impl FnMut(u32, usize)) -> Result<()> {
    let mut message = MSG::default();
    loop {
        // SAFETY: message menunjuk ke variabel stack yang valid.
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 == -1 {
            bail!("GetMessageW mengembalikan error");
        }
        if result.0 == 0 {
            break;
        }
        if message.hwnd.is_invalid() {
            on_thread_message(message.message, message.wParam.0);
            continue;
        }
        // SAFETY: pesan berasal dari antrean thread ini.
        unsafe {
            let _ = TranslateMessage(&message);
            let _ = DispatchMessageW(&message);
        }
    }
    Ok(())
}

/// Mengakhiri loop pesan.
pub fn quit(code: i32) {
    // SAFETY: dipanggil dari thread UI.
    unsafe { PostQuitMessage(code) };
}
