//! State aplikasi, message loop, dan penghubung antara UI dan platform.
//!
//! Semua jendela Orb dibuat di thread UI yang sama. State disimpan di
//! thread-local agar window procedure tidak perlu menyimpan pointer mentah.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_DOWN, VK_ESCAPE, VK_LBUTTON, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MA_NOACTIVATE, WM_DISPLAYCHANGE, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE,
    WM_MOUSEMOVE, WM_RBUTTONUP, WM_TIMER,
};

use crate::actions;
use crate::config::{Action, Config, HotkeyMode, Position};
use crate::platform::{hotkey, input, startup, tray, window};
use crate::ui::ball::{self, Ball};
use crate::ui::radial::{MenuItem, RadialMenu};
use crate::ui::{self, settings};

/// Nama kelas jendela milik Orb.
pub const BALL_CLASS: &str = "OrbBallWindow";
pub const MENU_CLASS: &str = "OrbMenuWindow";
pub const TRAY_CLASS: &str = "OrbTrayWindow";

/// Interval timer.
const MENU_TICK_MS: u32 = 40;
const WATCH_TICK_MS: u32 = 100;
const ANIM_TICK_MS: u32 = 16;
/// Durasi animasi snap (PRD target < 150 ms).
const ANIM_DURATION_SECS: f32 = 0.16;
const PULSE_DURATION: Duration = Duration::from_millis(1_500);
/// Jeda singkat sebelum klik-di-luar dianggap menutup menu, agar klik yang
/// membuka menu tidak langsung menutupnya.
const MENU_GRACE: Duration = Duration::from_millis(200);
/// Jarak kursor (px logis) yang membuat bola menjadi jelas.
const NEAR_CURSOR_LOGICAL: f32 = 140.0;
/// Langkah perubahan opacity per tick timer.
const OPACITY_STEP: f32 = 0.1;

/// Aksi yang perlu dijalankan di luar pinjaman state aplikasi (dialog, quit).
enum Effect {
    RunAction(Action),
    TrayMenu(HWND),
    Settings,
    Notice(String),
    Error(String),
    Quit,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static EFFECTS: RefCell<VecDeque<Effect>> = const { RefCell::new(VecDeque::new()) };
    static PROCESSING: Cell<bool> = const { Cell::new(false) };
}

/// Menjalankan closure dengan state aplikasi bila tersedia.
///
/// Memakai `try_borrow_mut` agar pesan yang masuk saat state sedang dipinjam
/// (misalnya dari dialog modal) tidak menyebabkan panic.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| {
        let mut borrow = cell.try_borrow_mut().ok()?;
        borrow.as_mut().map(f)
    })
}

fn push_effect(effect: Effect) {
    EFFECTS.with(|queue| queue.borrow_mut().push_back(effect));
}

fn next_effect() -> Option<Effect> {
    EFFECTS.with(|queue| queue.borrow_mut().pop_front())
}

fn run_effects() {
    if PROCESSING.with(Cell::get) {
        return;
    }
    PROCESSING.with(|flag| flag.set(true));
    while let Some(effect) = next_effect() {
        match effect {
            Effect::RunAction(action) => {
                if let Err(err) = actions::run(&action) {
                    log::warn!("aksi '{}' gagal: {err}", action.describe());
                    show_error(format!(
                        "Aksi untuk \"{}\" gagal.\n\n{err}",
                        action.describe()
                    ));
                }
            }
            Effect::TrayMenu(hwnd) => {
                if let Some(command) = show_tray_menu(hwnd) {
                    let _ = with_app(|app| app.tray_command(command));
                }
            }
            Effect::Settings => {
                let context = with_app(|app| (app.config_path.clone(), app.ball.hwnd()));
                if let Some((path, ball)) = context
                    && let Err(err) = settings::open(path, ball)
                {
                    log::warn!("form pengaturan gagal dibuka: {err:#}");
                    show_error(format!("Pengaturan tidak bisa dibuka.\n\n{err:#}"));
                }
            }
            Effect::Notice(message) => {
                window::message_box(None, "Orb", &message, false);
            }
            Effect::Error(message) => show_error(message),
            Effect::Quit => {
                let app = APP.with(|cell| cell.borrow_mut().take());
                drop(app);
                window::quit(0);
                PROCESSING.with(|flag| flag.set(false));
                return;
            }
        }
    }
    PROCESSING.with(|flag| flag.set(false));
}

fn show_error(message: String) {
    let owner = with_app(|app| app.ball.hwnd());
    window::message_box(owner, "Orb", &message, true);
}

fn show_tray_menu(hwnd: HWND) -> Option<usize> {
    window::popup_menu(
        hwnd,
        &[
            (1, "Tampilkan bola"),
            (2, "Reset posisi"),
            (3, "Pengaturan"),
            (0, ""),
            (4, "Keluar"),
        ],
    )
}

/// Menjalankan Orb: siapkan jendela, state, lalu masuk ke message loop.
pub fn run() -> Result<()> {
    window::init_process_dpi();

    if window::find_window(BALL_CLASS).is_some() {
        window::message_box(
            None,
            "Orb",
            "Orb sudah berjalan. Gunakan ikonnya di system tray.",
            false,
        );
        return Ok(());
    }

    window::register_class(BALL_CLASS, Some(ball_proc))?;
    window::register_class(MENU_CLASS, Some(menu_proc))?;
    window::register_class(TRAY_CLASS, Some(tray_proc))?;
    window::register_class(settings::CLASS_NAME, Some(settings::wndproc))?;

    let config_path = crate::config::default_path();
    let (config, warnings) = Config::load(&config_path);
    for warning in &warnings {
        log::warn!("{warning}");
    }
    if !warnings.is_empty() {
        window::message_box(
            None,
            "Orb",
            &format!(
                "Konfigurasi dibaca dengan penyesuaian:\n\n{}",
                warnings.join("\n")
            ),
            false,
        );
    }

    let scale = window::system_scale();
    let initial = Ball::scaled_size(config.ball.size, scale);
    let ball_hwnd = window::create_layered_window(BALL_CLASS, "Orb", initial, initial)?;
    let menu_hwnd = window::create_layered_window(MENU_CLASS, "Orb Menu", 8, 8)?;
    let tray_hwnd = window::create_hidden_window(TRAY_CLASS, "Orb Tray")?;

    APP.with(|cell| {
        *cell.borrow_mut() = Some(App::new(
            config,
            config_path,
            ball_hwnd,
            menu_hwnd,
            tray_hwnd,
        ));
    });

    let _ = with_app(|app| app.start());
    run_effects();

    window::run_message_loop(|message, wparam| {
        if message == WM_HOTKEY {
            let _ = with_app(|app| app.on_hotkey(wparam as i32));
            run_effects();
        }
    })?;

    let _ = APP.with(|cell| cell.borrow_mut().take());
    Ok(())
}

/// Penjaga status tombol agar satu tekanan hanya memicu satu aksi.
struct KeyWatcher {
    down: [bool; 256],
}

impl KeyWatcher {
    fn new() -> Self {
        Self { down: [false; 256] }
    }

    fn pressed(&mut self, vk: u16) -> bool {
        let index = usize::from(vk);
        if index >= self.down.len() {
            return false;
        }
        let now = input::is_key_down(vk);
        let was = self.down[index];
        self.down[index] = now;
        now && !was
    }
}

/// Status drag bola yang sedang berjalan.
struct Drag {
    origin_cursor: POINT,
    origin_position: (i32, i32),
}

/// Animasi snap ke tepi layar.
struct Snap {
    from: (i32, i32),
    to: (i32, i32),
    started: Instant,
}

struct App {
    config: Config,
    config_path: PathBuf,
    ball: Ball,
    menu: RadialMenu,
    menu_hwnd: HWND,
    tray_hwnd: HWND,
    tray: Option<tray::Tray>,
    hook: Option<HWINEVENTHOOK>,
    hotkey_menu: Option<hotkey::Registration>,
    hotkey_find: Option<hotkey::Registration>,
    drag: Option<Drag>,
    drag_moved: bool,
    snap: Option<Snap>,
    keys: KeyWatcher,
    pulse_until: Option<Instant>,
    menu_opened: Instant,
    hidden_for_fullscreen: bool,
    watch_ticks: u32,
}

impl App {
    fn new(
        config: Config,
        config_path: PathBuf,
        ball_hwnd: HWND,
        menu_hwnd: HWND,
        tray_hwnd: HWND,
    ) -> Self {
        let palette = ui::palette(config.theme);
        Self {
            config,
            config_path,
            ball: Ball::new(ball_hwnd, palette),
            menu: RadialMenu::new(menu_hwnd, palette),
            menu_hwnd,
            tray_hwnd,
            tray: None,
            hook: None,
            hotkey_menu: None,
            hotkey_find: None,
            drag: None,
            drag_moved: false,
            snap: None,
            keys: KeyWatcher::new(),
            pulse_until: None,
            menu_opened: Instant::now(),
            hidden_for_fullscreen: false,
            watch_ticks: 0,
        }
    }

    fn start(&mut self) {
        self.apply_config();
        if let Err(err) = self.ball.show_at(self.ball.position()) {
            push_effect(Effect::Error(format!(
                "Bola tidak bisa ditampilkan.\n\n{err:#}"
            )));
        }
        let _ = self.ball.set_opacity(1.0);

        self.install_tray();
        self.update_tray_tooltip();
        match window::install_foreground_hook(self.ball.hwnd()) {
            Ok(hook) => self.hook = Some(hook),
            Err(err) => log::warn!("hook foreground tidak bisa dipasang: {err:#}"),
        }
        window::set_timer(self.ball.hwnd(), window::TIMER_WATCH, WATCH_TICK_MS);

        if self.config.autostart
            && let Err(err) = startup::set_enabled(true)
        {
            log::warn!("entri autostart tidak bisa dipastikan: {err:#}");
        }
        log::info!("Orb siap dijalankan");
    }

    fn install_tray(&mut self) {
        let size = 32;
        let pixels = match self.ball.icon_pixels(size) {
            Ok(pixels) => pixels,
            Err(err) => {
                log::warn!("ikon tray tidak bisa digambar: {err:#}");
                return;
            }
        };
        let icon = match tray::create_icon_from_bgra(size, size, &pixels) {
            Ok(icon) => icon,
            Err(err) => {
                log::warn!("ikon tray tidak bisa dibuat: {err:#}");
                return;
            }
        };
        match tray::Tray::install(self.tray_hwnd, icon, "Orb - klik untuk menu") {
            Ok(tray) => self.tray = Some(tray),
            Err(err) => log::warn!("ikon tray tidak bisa dipasang: {err:#}"),
        }
    }

    fn apply_config(&mut self) {
        let palette = ui::palette(self.config.theme);
        self.ball.set_palette(palette);
        self.menu.set_palette(palette);

        let scale = window::dpi_scale(self.ball.hwnd());
        if let Err(err) = self.ball.set_size(self.config.ball.size, scale) {
            push_effect(Effect::Error(format!(
                "Ukuran bola tidak bisa diterapkan.\n\n{err:#}"
            )));
        }
        self.reload_slots(scale);
        self.place_ball();
        self.register_hotkeys();
    }

    fn reload_slots(&mut self, scale: f32) {
        let items: Vec<MenuItem> = self
            .config
            .slots()
            .iter()
            .map(|slot| MenuItem {
                label: slot.label.clone(),
                action: slot.action.clone(),
                icon: slot.icon.clone(),
            })
            .collect();

        if let Err(err) = self.menu.configure(scale) {
            push_effect(Effect::Error(format!(
                "Menu radial tidak bisa disiapkan.\n\n{err:#}"
            )));
            return;
        }
        if let Err(err) = self.menu.set_items(items, scale) {
            push_effect(Effect::Error(format!(
                "Isi menu tidak bisa dimuat.\n\n{err:#}"
            )));
        }
    }

    fn place_ball(&mut self) {
        let size = self.ball.size();
        let current_area = self.ball_area();
        let cursor_area = window::work_area(window::cursor_pos());
        let position = match self.config.ball.position {
            Some(saved)
                if ball::is_within_any((saved.x, saved.y), size, &[current_area, cursor_area]) =>
            {
                (saved.x, saved.y)
            }
            _ => ball::default_position(size, cursor_area),
        };
        let area = window::work_area(POINT {
            x: position.0 + size / 2,
            y: position.1 + size / 2,
        });
        let position = ball::clamp_to_area(position, size, area);
        self.ball.move_to(position);
        if let Err(err) = self.ball.redraw() {
            push_effect(Effect::Error(format!(
                "Bola tidak bisa digambar.\n\n{err:#}"
            )));
        }
    }

    fn register_hotkeys(&mut self) {
        self.hotkey_menu = None;
        self.hotkey_find = None;

        let spec = self.config.hotkey.open_menu.trim().to_string();
        if spec.is_empty() {
            log::info!("hotkey menu dikosongkan; menu hanya bisa dibuka lewat bola atau tray");
        } else {
            match register_with_fallback(window::HOTKEY_MENU_ID, &spec) {
                Ok((registration, used)) => {
                    if used != spec {
                        push_effect(Effect::Notice(format!(
                            "Hotkey \"{spec}\" sudah dipakai aplikasi lain, jadi Orb memakai \
                             \"{used}\". Ubah hotkey lewat menu Pengaturan."
                        )));
                    }
                    self.hotkey_menu = Some(registration);
                }
                Err(err) => push_effect(Effect::Error(format!(
                    "Hotkey \"{spec}\" dan semua alternatifnya gagal didaftarkan.\n\n{err:#}\n\n\
                     Menu tetap bisa dibuka dengan klik bola atau lewat ikon tray, dan hotkey \
                     bisa diubah di Pengaturan."
                ))),
            }
        }

        let spec = self.config.hotkey.find_ball.trim().to_string();
        if !spec.is_empty() {
            match hotkey::parse(&spec).and_then(|combo| {
                hotkey::Registration::register(window::HOTKEY_FIND_ID, combo, &spec)
            }) {
                Ok(registration) => self.hotkey_find = Some(registration),
                Err(err) => log::warn!("hotkey temukan bola \"{spec}\" tidak tersedia: {err}"),
            }
        }

        if self.config.hotkey.mode == HotkeyMode::Hold {
            log::warn!("mode hotkey \"hold\" belum didukung; Orb memakai mode toggle");
        }
        self.update_tray_tooltip();
    }

    fn update_tray_tooltip(&self) {
        let Some(tray) = &self.tray else {
            return;
        };
        if self.hotkey_menu.is_some() {
            tray.set_tooltip(&format!(
                "Orb - {} untuk menu",
                self.config.hotkey.open_menu.trim()
            ));
        } else {
            tray.set_tooltip("Orb - klik untuk menu");
        }
    }

    fn save_config(&mut self) {
        if let Err(err) = self.config.save(&self.config_path) {
            log::warn!("konfigurasi tidak bisa disimpan: {err:#}");
            return;
        }
        log::info!("konfigurasi disimpan ke {}", self.config_path.display());
    }

    fn persist_position(&mut self) {
        let (x, y) = self.ball.position();
        self.config.ball.position = Some(Position { x, y });
        self.save_config();
    }

    fn reload_settings(&mut self) {
        let (config, warnings) = Config::load(&self.config_path);
        for warning in &warnings {
            log::warn!("{warning}");
        }
        let autostart_before = self.config.autostart;
        self.config = config;
        self.apply_config();

        if self.config.autostart != autostart_before
            && let Err(err) = startup::set_enabled(self.config.autostart)
        {
            push_effect(Effect::Error(format!(
                "Autostart tidak bisa diubah.\n\n{err:#}"
            )));
        }
        log::info!("pengaturan dimuat ulang");
    }

    fn ball_area(&self) -> RECT {
        let (x, y) = ball::center(self.ball.position(), self.ball.size());
        window::work_area(POINT { x, y })
    }

    // ---- Menu radial -----------------------------------------------------

    fn open_menu(&mut self, center: (i32, i32), area: RECT) {
        match self.menu.open(center, area) {
            Ok(()) => {
                self.menu_opened = Instant::now();
                window::set_timer(self.menu_hwnd, window::TIMER_MENU, MENU_TICK_MS);
                let _ = self.ball.set_opacity(1.0);
            }
            Err(err) => {
                log::warn!("menu tidak bisa dibuka: {err:#}");
                push_effect(Effect::Error(format!(
                    "Menu radial tidak bisa dibuka.\n\n{err:#}"
                )));
            }
        }
    }

    fn open_menu_at_ball(&mut self) {
        let (x, y) = ball::center(self.ball.position(), self.ball.size());
        let area = window::work_area(POINT { x, y });
        self.open_menu((x, y), area);
    }

    fn close_menu(&mut self) {
        window::kill_timer(self.menu_hwnd, window::TIMER_MENU);
        if self.menu.is_visible() {
            self.menu.close();
        }
    }

    fn activate(&mut self, index: usize) {
        let item = self.menu.items().get(index).cloned();
        self.close_menu();
        if let Some(item) = item {
            push_effect(Effect::RunAction(item.action));
        }
    }

    fn on_menu_timer(&mut self) {
        if !self.menu.is_visible() {
            window::kill_timer(self.menu_hwnd, window::TIMER_MENU);
            return;
        }

        let cursor = window::cursor_pos();
        let hovered = self.menu.hit_test(cursor);
        if let Err(err) = self.menu.set_hovered(hovered) {
            log::warn!("gambar menu gagal diperbarui: {err:#}");
        }

        // Selalu baca semua tombol setiap tick agar status tidak tertinggal.
        let escape = self.keys.pressed(VK_ESCAPE.0);
        let enter = self.keys.pressed(VK_RETURN.0);
        let previous = self.keys.pressed(VK_LEFT.0) || self.keys.pressed(VK_UP.0);
        let next = self.keys.pressed(VK_RIGHT.0) || self.keys.pressed(VK_DOWN.0);
        let left_click = self.keys.pressed(VK_LBUTTON.0);

        let mut chosen = None;
        let item_count = self.menu.item_count();
        for digit in 0..8u16 {
            let pressed = self.keys.pressed(0x31 + digit);
            if pressed && chosen.is_none() && usize::from(digit) < item_count {
                chosen = Some(usize::from(digit));
            }
        }

        if escape {
            self.close_menu();
            return;
        }
        if let Some(index) = chosen {
            self.activate(index);
            return;
        }
        if previous {
            let _ = self.menu.move_selection(false);
        }
        if next {
            let _ = self.menu.move_selection(true);
        }
        if enter {
            if let Some(index) = self.menu.selected() {
                self.activate(index);
            }
            return;
        }

        if self.menu_opened.elapsed() > MENU_GRACE {
            let inside = point_in_rect(cursor, self.menu.window_rect());
            if left_click && !inside {
                self.close_menu();
            }
        }
    }

    // ---- Drag, snap, dan klik bola ---------------------------------------

    fn begin_drag(&mut self) {
        if self.menu.is_visible() {
            self.close_menu();
        }
        if self.snap.is_some() {
            self.snap = None;
            window::kill_timer(self.ball.hwnd(), window::TIMER_ANIM);
        }
        window::capture_mouse(self.ball.hwnd());
        self.drag = Some(Drag {
            origin_cursor: window::cursor_pos(),
            origin_position: self.ball.position(),
        });
        self.drag_moved = false;
        let _ = self.ball.set_opacity(1.0);
    }

    fn update_drag(&mut self) {
        let Some(drag) = &self.drag else {
            return;
        };
        let origin_cursor = drag.origin_cursor;
        let origin_position = drag.origin_position;

        let cursor = window::cursor_pos();
        let dx = cursor.x - origin_cursor.x;
        let dy = cursor.y - origin_cursor.y;
        if dx * dx + dy * dy > 9 {
            self.drag_moved = true;
        }
        let target = (origin_position.0 + dx, origin_position.1 + dy);
        let area = window::work_area(cursor);
        let clamped = ball::clamp_to_area(target, self.ball.size(), area);
        self.ball.move_to(clamped);
    }

    fn end_drag(&mut self) {
        window::release_mouse();
        let Some(drag) = self.drag.take() else {
            return;
        };

        if !self.drag_moved {
            self.ball.move_to(drag.origin_position);
            self.open_menu_at_ball();
            return;
        }

        let area = window::work_area(window::cursor_pos());
        if self.config.ball.snap_to_edge {
            let target = ball::snap_target(self.ball.position(), self.ball.size(), area);
            self.start_snap(target);
        } else {
            let clamped = ball::clamp_to_area(self.ball.position(), self.ball.size(), area);
            self.ball.move_to(clamped);
            self.persist_position();
        }
    }

    fn start_snap(&mut self, target: (i32, i32)) {
        let from = self.ball.position();
        if from == target {
            self.persist_position();
            return;
        }
        self.snap = Some(Snap {
            from,
            to: target,
            started: Instant::now(),
        });
        window::set_timer(self.ball.hwnd(), window::TIMER_ANIM, ANIM_TICK_MS);
    }

    fn step_snap(&mut self) {
        let Some(snap) = &self.snap else {
            return;
        };
        let progress = (snap.started.elapsed().as_secs_f32() / ANIM_DURATION_SECS).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        let x = snap.from.0 + ((snap.to.0 - snap.from.0) as f32 * eased).round() as i32;
        let y = snap.from.1 + ((snap.to.1 - snap.from.1) as f32 * eased).round() as i32;
        self.ball.move_to((x, y));

        if progress >= 1.0 {
            let target = snap.to;
            self.snap = None;
            window::kill_timer(self.ball.hwnd(), window::TIMER_ANIM);
            self.ball.move_to(target);
            self.persist_position();
        }
    }

    // ---- Timer pemantau ------------------------------------------------

    fn watch(&mut self) {
        self.watch_ticks = self.watch_ticks.wrapping_add(1);
        if self.watch_ticks.is_multiple_of(5) {
            self.update_fullscreen();
        }
        self.update_opacity();
    }

    fn update_fullscreen(&mut self) {
        if self.drag.is_some() {
            return;
        }
        let fullscreen = input::should_hide_for_fullscreen();
        if fullscreen && !self.hidden_for_fullscreen {
            self.hidden_for_fullscreen = true;
            self.close_menu();
            self.ball.hide();
            log::debug!("bola disembunyikan karena aplikasi layar penuh");
        } else if !fullscreen && self.hidden_for_fullscreen {
            self.hidden_for_fullscreen = false;
            window::show_no_activate(self.ball.hwnd());
            log::debug!("bola ditampilkan kembali");
        }
    }

    fn update_opacity(&mut self) {
        let now = Instant::now();
        let pulsing = self.pulse_until.is_some_and(|until| until > now);
        if self.pulse_until.is_some_and(|until| until <= now) {
            self.pulse_until = None;
        }

        let target = if pulsing || self.menu.is_visible() || self.drag.is_some() {
            1.0
        } else {
            let cursor = window::cursor_pos();
            let (cx, cy) = ball::center(self.ball.position(), self.ball.size());
            let dx = f64::from(cursor.x - cx);
            let dy = f64::from(cursor.y - cy);
            let near = f64::from(NEAR_CURSOR_LOGICAL * window::dpi_scale(self.ball.hwnd()));
            if dx.hypot(dy) <= near {
                1.0
            } else {
                self.config.ball.min_opacity
            }
        };

        let current = self.ball.opacity();
        let delta = target - current;
        let next = if delta.abs() <= OPACITY_STEP {
            target
        } else {
            current + delta.signum() * OPACITY_STEP
        };
        if let Err(err) = self.ball.set_opacity(next) {
            log::warn!("opacity bola tidak bisa diubah: {err:#}");
        }
    }

    // ---- Hotkey, tray, dan pesan jendela --------------------------------

    fn on_hotkey(&mut self, id: i32) {
        match id {
            window::HOTKEY_MENU_ID => self.toggle_menu(),
            window::HOTKEY_FIND_ID => self.find_ball(),
            _ => {}
        }
    }

    fn toggle_menu(&mut self) {
        if self.menu.is_visible() {
            self.close_menu();
            return;
        }
        let cursor = window::cursor_pos();
        let area = window::work_area(cursor);
        self.open_menu((cursor.x, cursor.y), area);
    }

    fn find_ball(&mut self) {
        let cursor = window::cursor_pos();
        let area = window::work_area(cursor);
        let size = self.ball.size();

        if self.hidden_for_fullscreen {
            self.hidden_for_fullscreen = false;
            window::show_no_activate(self.ball.hwnd());
        } else if !ball::is_within_any(self.ball.position(), size, &[area]) {
            let target = ball::clamp_to_area((cursor.x + 24, cursor.y + 24), size, area);
            self.ball.move_to(target);
            window::show_no_activate(self.ball.hwnd());
            self.persist_position();
            log::info!("bola dipindahkan ke dekat kursor lewat hotkey temukan bola");
        }

        self.pulse_until = Some(Instant::now() + PULSE_DURATION);
        let _ = self.ball.set_opacity(1.0);
    }

    fn tray_command(&mut self, command: usize) {
        match command {
            1 => {
                self.hidden_for_fullscreen = false;
                self.pulse_until = Some(Instant::now() + PULSE_DURATION);
                window::show_no_activate(self.ball.hwnd());
                let _ = self.ball.set_opacity(1.0);
            }
            2 => {
                let area = window::work_area(window::cursor_pos());
                let position = ball::default_position(self.ball.size(), area);
                if let Err(err) = self.ball.show_at(position) {
                    push_effect(Effect::Error(format!(
                        "Posisi bola tidak bisa direset.\n\n{err:#}"
                    )));
                }
                self.persist_position();
                log::info!("posisi bola direset");
            }
            3 => push_effect(Effect::Settings),
            4 => push_effect(Effect::Quit),
            _ => {}
        }
    }

    fn on_foreground(&mut self) {
        if self.ball.is_visible() && !self.hidden_for_fullscreen {
            window::raise(self.ball.hwnd());
        }
        if self.menu.is_visible() {
            window::raise(self.menu_hwnd);
        }
    }

    fn on_display_change(&mut self) {
        self.close_menu();
        let area = self.ball_area();
        let clamped = ball::clamp_to_area(self.ball.position(), self.ball.size(), area);
        if clamped != self.ball.position() {
            self.ball.move_to(clamped);
            self.persist_position();
            log::info!("posisi bola disesuaikan setelah perubahan layar");
        }
        if let Err(err) = self.ball.redraw() {
            log::warn!("bola tidak bisa digambar ulang: {err:#}");
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(hook) = self.hook.take() {
            window::uninstall_hook(hook);
        }
        window::kill_timer(self.ball.hwnd(), window::TIMER_ANIM);
        window::kill_timer(self.ball.hwnd(), window::TIMER_WATCH);
        window::kill_timer(self.menu_hwnd, window::TIMER_MENU);
        log::info!("Orb berhenti");
    }
}

fn register_with_fallback(id: i32, spec: &str) -> Result<(hotkey::Registration, String)> {
    let mut candidates = vec![spec.to_string()];
    for alternative in ["Ctrl+Alt+O", "Ctrl+Shift+Space", "Ctrl+Alt+F10"] {
        if alternative != spec {
            candidates.push(alternative.to_string());
        }
    }

    let mut last_error = None;
    for candidate in candidates {
        let attempt = hotkey::parse(&candidate)
            .and_then(|combo| hotkey::Registration::register(id, combo, &candidate));
        match attempt {
            Ok(registration) => return Ok((registration, candidate)),
            Err(err) => {
                log::warn!("hotkey \"{candidate}\" gagal didaftarkan: {err}");
                last_error = Some(err);
            }
        }
    }

    match last_error {
        Some(err) => Err(err.into()),
        None => anyhow::bail!("tidak ada kandidat hotkey"),
    }
}

fn point_in_rect(point: POINT, rect: RECT) -> bool {
    point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom
}

// ---- Window procedure ---------------------------------------------------

unsafe extern "system" fn ball_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN => {
            let _ = with_app(|app| app.begin_drag());
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let _ = with_app(|app| app.update_drag());
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = with_app(App::end_drag);
            run_effects();
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            push_effect(Effect::TrayMenu(hwnd));
            run_effects();
            LRESULT(0)
        }
        WM_TIMER => {
            let id = wparam.0;
            let _ = with_app(|app| match id {
                window::TIMER_ANIM => app.step_snap(),
                window::TIMER_WATCH => app.watch(),
                _ => {}
            });
            run_effects();
            LRESULT(0)
        }
        window::WM_ORB_FOREGROUND => {
            let _ = with_app(App::on_foreground);
            LRESULT(0)
        }
        window::WM_ORB_SETTINGS_SAVED => {
            let _ = with_app(App::reload_settings);
            run_effects();
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            let _ = with_app(App::on_display_change);
            run_effects();
            LRESULT(0)
        }
        _ => LRESULT(window::default_proc(hwnd, msg, wparam, lparam)),
    }
}

unsafe extern "system" fn menu_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONUP => {
            let _ = with_app(|app| {
                let cursor = window::cursor_pos();
                match app.menu.hit_test(cursor) {
                    Some(index) => app.activate(index),
                    None => app.close_menu(),
                }
            });
            run_effects();
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let _ = with_app(App::close_menu);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == window::TIMER_MENU {
                let _ = with_app(App::on_menu_timer);
                run_effects();
            }
            LRESULT(0)
        }
        _ => LRESULT(window::default_proc(hwnd, msg, wparam, lparam)),
    }
}

unsafe extern "system" fn tray_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        window::WM_ORB_TRAY => {
            push_effect(Effect::TrayMenu(hwnd));
            run_effects();
            LRESULT(0)
        }
        _ => LRESULT(window::default_proc(hwnd, msg, wparam, lparam)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_watcher_detects_single_press() {
        let mut watcher = KeyWatcher::new();
        // Status tombol asli tidak bisa diatur dari test, jadi cukup pastikan
        // pembacaan tidak pernah panic dan status tersimpan.
        let _ = watcher.pressed(VK_ESCAPE.0);
        assert_eq!(
            watcher.down[usize::from(VK_ESCAPE.0)],
            input::is_key_down(VK_ESCAPE.0)
        );
    }

    #[test]
    fn point_in_rect_checks_bounds() {
        let rect = RECT {
            left: 10,
            top: 10,
            right: 20,
            bottom: 20,
        };
        assert!(point_in_rect(POINT { x: 10, y: 10 }, rect));
        assert!(point_in_rect(POINT { x: 20, y: 20 }, rect));
        assert!(!point_in_rect(POINT { x: 9, y: 15 }, rect));
        assert!(!point_in_rect(POINT { x: 15, y: 21 }, rect));
    }
}
