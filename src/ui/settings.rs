//! Form pengaturan native (F9): ukuran, opacity, hotkey, tema, dan isi slot.
//!
//! Form memakai kontrol Win32 asli, tetapi diwarnai manual (background, label,
//! tombol, dan checkbox digambar sendiri) agar mengikuti tema gelap/terang dan
//! visual styles modern.

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::PathBuf;

use anyhow::Result;
use windows::Win32::Foundation::{COLORREF, HDC, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DRAW_TEXT_FORMAT, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, DeleteObject, DrawTextW, FillRect, GetClientRect, HBRUSH, HFONT, HGDIOBJ, LineTo,
    MoveToEx, PS_NULL, PS_SOLID, RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    TRANSPARENT,
};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_OWNERDRAW, CBS_DROPDOWNLIST, DestroyWindow, ES_AUTOHSCROLL, GetClassNameW, PostMessageW,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND, WS_BORDER, WS_TABSTOP, WS_VSCROLL,
};

use crate::config::{
    Action, ActionKind, Config, MAX_BALL_SIZE, MIN_BALL_SIZE, SLOT_COUNT, Slot, Theme,
};
use crate::platform::theme;
use crate::platform::window::{self, WM_ORB_SETTINGS_SAVED};

pub const CLASS_NAME: &str = "OrbSettingsWindow";
const TITLE: &str = "Orb - Pengaturan";

const ID_SIZE: i32 = 100;
const ID_OPACITY: i32 = 101;
const ID_HOTKEY: i32 = 102;
const ID_THEME: i32 = 103;
const ID_SNAP: i32 = 104;
const ID_AUTOSTART: i32 = 105;
const ID_SAVE: i32 = 200;
const ID_CANCEL: i32 = 201;
const ID_SLOT_BASE: i32 = 300;

thread_local! {
    static FORM: RefCell<Option<FormContext>> = const { RefCell::new(None) };
}

struct FormContext {
    path: PathBuf,
    ball: HWND,
}

/// Membuka form pengaturan, atau memfokuskan yang sudah terbuka.
pub fn open(path: PathBuf, ball: HWND) -> Result<()> {
    if let Some(existing) = window::find_window(CLASS_NAME) {
        window::focus(existing);
        return Ok(());
    }

    let (config, warnings) = Config::load(&path);
    for warning in warnings {
        log::warn!("{warning}");
    }

    let scale = window::system_scale();
    let width = (520.0 * scale).round() as i32;
    let height = (486.0 * scale).round() as i32;
    let hwnd = window::create_app_window(CLASS_NAME, TITLE, width, height)?;
    build_form(hwnd, &config, scale)?;
    window::center_on_cursor(hwnd);

    FORM.with(|cell| {
        *cell.borrow_mut() = Some(FormContext {
            path: path.clone(),
            ball,
        });
    });

    window::focus(hwnd);
    Ok(())
}

fn scaled(value: f32, scale: f32) -> i32 {
    (value * scale).round() as i32
}

fn add_label(parent: HWND, text: &str, x: f32, y: f32, width: f32, scale: f32) -> Result<()> {
    let control = window::create_child(
        parent,
        "STATIC",
        text,
        WINDOW_STYLE(0),
        (
            scaled(x, scale),
            scaled(y, scale),
            scaled(width, scale),
            scaled(18.0, scale),
        ),
        0,
    )?;
    window::apply_font(control);
    Ok(())
}

fn add_edit(
    parent: HWND,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    id: i32,
    scale: f32,
) -> Result<()> {
    let style = WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32) | WS_TABSTOP;
    let control = window::create_child(
        parent,
        "EDIT",
        text,
        style,
        (
            scaled(x, scale),
            scaled(y, scale),
            scaled(width, scale),
            scaled(24.0, scale),
        ),
        id,
    )?;
    window::apply_font(control);
    Ok(())
}

fn add_combo(
    parent: HWND,
    items: &[&str],
    selected: i32,
    position: (f32, f32),
    width: f32,
    id: i32,
    scale: f32,
) -> Result<()> {
    let (x, y) = position;
    let style = WINDOW_STYLE(CBS_DROPDOWNLIST as u32) | WS_VSCROLL | WS_TABSTOP;
    let control = window::create_child(
        parent,
        "COMBOBOX",
        "",
        style,
        (
            scaled(x, scale),
            scaled(y, scale),
            scaled(width, scale),
            scaled(160.0, scale),
        ),
        id,
    )?;
    window::apply_font(control);
    for item in items {
        window::combo_add(parent, id, item);
    }
    window::set_combo_index(parent, id, selected);
    Ok(())
}

fn add_checkbox(
    parent: HWND,
    text: &str,
    checked: bool,
    position: (f32, f32),
    width: f32,
    id: i32,
    scale: f32,
) -> Result<()> {
    let (x, y) = position;
    let control = window::create_child(
        parent,
        "BUTTON",
        text,
        WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        (
            scaled(x, scale),
            scaled(y, scale),
            scaled(width, scale),
            scaled(24.0, scale),
        ),
        id,
    )?;
    window::apply_font(control);
    window::set_checked(parent, id, checked);
    Ok(())
}

fn add_button(
    parent: HWND,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    id: i32,
    scale: f32,
) -> Result<()> {
    let control = window::create_child(
        parent,
        "BUTTON",
        text,
        WINDOW_STYLE(BS_PUSHBUTTON as u32) | WS_TABSTOP,
        (
            scaled(x, scale),
            scaled(y, scale),
            scaled(width, scale),
            scaled(28.0, scale),
        ),
        id,
    )?;
    window::apply_font(control);
    Ok(())
}

fn build_form(hwnd: HWND, config: &Config, scale: f32) -> Result<()> {
    add_label(hwnd, "Ukuran bola (32-96 px)", 16.0, 18.0, 200.0, scale)?;
    add_edit(
        hwnd,
        &config.ball.size.to_string(),
        216.0,
        14.0,
        70.0,
        ID_SIZE,
        scale,
    )?;

    add_label(
        hwnd,
        "Opasitas minimum (0.05-1.0)",
        16.0,
        50.0,
        200.0,
        scale,
    )?;
    add_edit(
        hwnd,
        &format!("{:.2}", config.ball.min_opacity),
        216.0,
        46.0,
        70.0,
        ID_OPACITY,
        scale,
    )?;

    add_label(hwnd, "Hotkey menu", 16.0, 82.0, 200.0, scale)?;
    add_edit(
        hwnd,
        &config.hotkey.open_menu,
        216.0,
        78.0,
        180.0,
        ID_HOTKEY,
        scale,
    )?;

    add_label(hwnd, "Tema", 16.0, 114.0, 200.0, scale)?;
    let theme_index = match config.theme {
        Theme::Dark => 0,
        Theme::Light => 1,
    };
    add_combo(
        hwnd,
        &["Gelap", "Terang"],
        theme_index,
        (216.0, 110.0),
        120.0,
        ID_THEME,
        scale,
    )?;

    add_checkbox(
        hwnd,
        "Tempel ke tepi layar setelah digeser",
        config.ball.snap_to_edge,
        (16.0, 146.0),
        300.0,
        ID_SNAP,
        scale,
    )?;
    add_checkbox(
        hwnd,
        "Jalankan Orb saat Windows menyala",
        config.autostart,
        (16.0, 172.0),
        300.0,
        ID_AUTOSTART,
        scale,
    )?;

    add_label(hwnd, "Slot menu", 16.0, 208.0, 200.0, scale)?;
    add_label(
        hwnd,
        "Label                    Jenis                         Parameter",
        16.0,
        228.0,
        480.0,
        scale,
    )?;

    for index in 0..SLOT_COUNT {
        let slot = config.slots().get(index);
        let y = 250.0 + index as f32 * 32.0;
        add_edit(
            hwnd,
            slot.map(|slot| slot.label.as_str()).unwrap_or(""),
            16.0,
            y,
            120.0,
            ID_SLOT_BASE + index as i32 * 3,
            scale,
        )?;

        let labels: Vec<&str> = ActionKind::ALL.iter().map(|kind| kind.label()).collect();
        let kind_index = slot.map(|slot| slot.action.kind().index()).unwrap_or(0) as i32;
        add_combo(
            hwnd,
            &labels,
            kind_index,
            (148.0, y - 4.0),
            100.0,
            ID_SLOT_BASE + index as i32 * 3 + 1,
            scale,
        )?;

        add_edit(
            hwnd,
            slot.map(|slot| slot.action.parameter())
                .unwrap_or_default()
                .as_str(),
            258.0,
            y,
            246.0,
            ID_SLOT_BASE + index as i32 * 3 + 2,
            scale,
        )?;
    }

    add_button(hwnd, "Simpan", 16.0, 448.0, 120.0, ID_SAVE, scale)?;
    add_button(hwnd, "Batal", 148.0, 448.0, 120.0, ID_CANCEL, scale)?;
    Ok(())
}

fn current_context() -> Option<(PathBuf, HWND)> {
    FORM.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|context| (context.path.clone(), context.ball))
    })
}

fn handle_save(hwnd: HWND) {
    let Some((path, ball)) = current_context() else {
        // SAFETY: hwnd valid milik proses ini.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    };

    let (base, _) = Config::load(&path);
    let mut config = match read_form(hwnd, &base) {
        Ok(config) => config,
        Err(message) => {
            window::message_box(Some(hwnd), TITLE, &message, true);
            return;
        }
    };
    let mut warnings = Vec::new();
    config.normalize(&mut warnings);
    for warning in &warnings {
        log::warn!("pengaturan: {warning}");
    }

    if let Err(err) = config.save(&path) {
        window::message_box(
            Some(hwnd),
            TITLE,
            &format!("Pengaturan tidak bisa disimpan.\n\n{err:#}"),
            true,
        );
        return;
    }

    log::info!("pengaturan disimpan ke {}", path.display());
    // SAFETY: ball adalah jendela milik proses ini.
    unsafe {
        let _ = PostMessageW(Some(ball), WM_ORB_SETTINGS_SAVED, WPARAM(0), LPARAM(0));
        // SAFETY: hwnd valid milik proses ini.
        let _ = DestroyWindow(hwnd);
    }
}

fn read_form(hwnd: HWND, base: &Config) -> Result<Config, String> {
    let mut config = base.clone();

    let size_text = window::child_text(hwnd, ID_SIZE);
    let size: u32 = size_text
        .trim()
        .parse()
        .map_err(|_| "Ukuran bola harus berupa angka.".to_string())?;
    if !(MIN_BALL_SIZE..=MAX_BALL_SIZE).contains(&size) {
        return Err(format!(
            "Ukuran bola harus di antara {MIN_BALL_SIZE} dan {MAX_BALL_SIZE} px."
        ));
    }
    config.ball.size = size;

    let opacity_text = window::child_text(hwnd, ID_OPACITY).replace(',', ".");
    let opacity: f32 = opacity_text
        .trim()
        .parse()
        .map_err(|_| "Opasitas minimum harus berupa angka (contoh 0.25).".to_string())?;
    if !(0.05..=1.0).contains(&opacity) {
        return Err("Opasitas minimum harus di antara 0.05 dan 1.0.".to_string());
    }
    config.ball.min_opacity = opacity;

    let hotkey = window::child_text(hwnd, ID_HOTKEY);
    let hotkey = hotkey.trim().to_string();
    if !hotkey.is_empty() && crate::platform::hotkey::parse(&hotkey).is_err() {
        return Err(
            "Hotkey tidak valid. Contoh yang benar: Ctrl+Alt+Space, Ctrl+Shift+F9.".to_string(),
        );
    }
    config.hotkey.open_menu = hotkey;

    config.theme = match window::combo_index(hwnd, ID_THEME) {
        1 => Theme::Light,
        _ => Theme::Dark,
    };
    config.ball.snap_to_edge = window::is_checked(hwnd, ID_SNAP);
    config.autostart = window::is_checked(hwnd, ID_AUTOSTART);

    let mut slots = Vec::with_capacity(SLOT_COUNT);
    for index in 0..SLOT_COUNT {
        let base_id = ID_SLOT_BASE + index as i32 * 3;
        let label = window::child_text(hwnd, base_id).trim().to_string();
        let kind = ActionKind::from_index(window::combo_index(hwnd, base_id + 1));
        let parameter = window::child_text(hwnd, base_id + 2);
        let action = Action::from_parts(kind, &parameter).map_err(|message| {
            format!(
                "Slot {} ({}): {message}",
                index + 1,
                if label.is_empty() {
                    "tanpa nama"
                } else {
                    &label
                }
            )
        })?;
        slots.push(Slot {
            label: if label.is_empty() {
                format!("Slot {}", index + 1)
            } else {
                label
            },
            icon: base.slots().get(index).and_then(|slot| slot.icon.clone()),
            action,
        });
    }
    config.profile[0].slot = slots;
    Ok(config)
}

/// Window procedure form pengaturan.
pub unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            match id {
                ID_SAVE => handle_save(hwnd),
                ID_CANCEL => {
                    // SAFETY: hwnd valid milik proses ini.
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: hwnd valid milik proses ini.
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_DESTROY => {
            FORM.with(|cell| {
                *cell.borrow_mut() = None;
            });
            LRESULT(0)
        }
        _ => LRESULT(window::default_proc(hwnd, msg, wparam, lparam)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_ids_do_not_overlap() {
        let mut ids = Vec::new();
        for index in 0..SLOT_COUNT {
            for offset in 0..3 {
                ids.push(ID_SLOT_BASE + index as i32 * 3 + offset);
            }
        }
        let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(!ids.contains(&ID_SAVE) && !ids.contains(&ID_CANCEL));
    }
}
