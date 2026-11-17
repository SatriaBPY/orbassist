//! Menu radial 6 slot yang muncul di atas bola atau di posisi kursor.

use anyhow::Result;
use windows::Win32::Foundation::{HWND, POINT, RECT};

use crate::config::Action;
use crate::platform::render::{Image, Surface};
use crate::platform::window;
use crate::ui::Palette;

/// Jari-jari lingkaran tempat slot diletakkan (px logis).
const RADIUS_LOGICAL: f32 = 78.0;
/// Jari-jari tiap slot (px logis).
const SLOT_LOGICAL: f32 = 27.0;
/// Ruang tambahan untuk label dan bayangan (px logis).
const PAD_LOGICAL: f32 = 26.0;

/// Satu item menu radial.
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub action: Action,
    pub icon: Option<String>,
}

/// Menu radial yang tidak pernah mengambil fokus.
pub struct RadialMenu {
    hwnd: HWND,
    surface: Option<Surface>,
    items: Vec<MenuItem>,
    images: Vec<Option<Image>>,
    palette: Palette,
    center: (i32, i32),
    radius: i32,
    slot_radius: i32,
    size: i32,
    hovered: Option<usize>,
    selected: Option<usize>,
    visible: bool,
}

impl RadialMenu {
    pub fn new(hwnd: HWND, palette: Palette) -> Self {
        Self {
            hwnd,
            surface: None,
            items: Vec::new(),
            images: Vec::new(),
            palette,
            center: (0, 0),
            radius: 78,
            slot_radius: 27,
            size: 0,
            hovered: None,
            selected: None,
            visible: false,
        }
    }

    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    /// Mengatur ukuran menu mengikuti skala DPI.
    pub fn configure(&mut self, scale: f32) -> Result<()> {
        self.radius = (RADIUS_LOGICAL * scale).round() as i32;
        self.slot_radius = (SLOT_LOGICAL * scale).round() as i32;
        let half = self.radius + self.slot_radius + (PAD_LOGICAL * scale).round() as i32;
        self.size = half * 2;
        self.surface = Some(Surface::new(self.size, self.size)?);
        self.redraw()
    }

    /// Mengganti isi menu dan memuat ikonnya.
    pub fn set_items(&mut self, items: Vec<MenuItem>, scale: f32) -> Result<()> {
        self.images = items
            .iter()
            .map(|item| match &item.icon {
                Some(path) if !path.trim().is_empty() => {
                    match Image::load(std::path::Path::new(path)) {
                        Ok(image) => Some(image),
                        Err(err) => {
                            log::warn!("ikon slot '{}' gagal dimuat: {err}", item.label);
                            None
                        }
                    }
                }
                _ => None,
            })
            .collect();
        self.items = items;
        if self.size == 0 {
            self.configure(scale)?;
        }
        self.hovered = None;
        self.selected = None;
        self.redraw()
    }

    /// Menampilkan menu di sekitar `center`, dijepit ke area kerja.
    pub fn open(&mut self, center: (i32, i32), area: RECT) -> Result<()> {
        if self.surface.is_none() {
            self.configure(1.0)?;
        }
        let half = self.size / 2;
        let min_x = area.left + half;
        let max_x = (area.right - half).max(min_x);
        let min_y = area.top + half;
        let max_y = (area.bottom - half).max(min_y);
        self.center = (center.0.clamp(min_x, max_x), center.1.clamp(min_y, max_y));
        self.hovered = None;
        self.selected = None;
        self.visible = true;
        window::move_to(self.hwnd, self.center.0 - half, self.center.1 - half);
        window::show_no_activate(self.hwnd);
        self.redraw()
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.hovered = None;
        self.selected = None;
        window::hide(self.hwnd);
    }

    pub fn set_hovered(&mut self, index: Option<usize>) -> Result<bool> {
        if self.hovered == index {
            return Ok(false);
        }
        self.hovered = index;
        if self.visible {
            self.redraw()?;
        }
        Ok(true)
    }

    pub fn move_selection(&mut self, forward: bool) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }
        let count = self.items.len();
        let current = self.selected.or(self.hovered);
        let next = match (current, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, true) => 0,
            (None, false) => count - 1,
        };
        self.selected = Some(next);
        self.redraw()
    }

    /// Slot di bawah titik layar (jika ada).
    pub fn hit_test(&self, point: POINT) -> Option<usize> {
        if !self.visible || self.items.is_empty() {
            return None;
        }
        let mut best: Option<(usize, i64)> = None;
        for index in 0..self.items.len() {
            let (x, y) = self.slot_center(index);
            let dx = i64::from(point.x - x);
            let dy = i64::from(point.y - y);
            let distance = dx * dx + dy * dy;
            let limit = i64::from(self.slot_radius + 6).pow(2);
            if distance <= limit && best.is_none_or(|(_, best_distance)| distance < best_distance) {
                best = Some((index, distance));
            }
        }
        best.map(|(index, _)| index)
    }

    pub fn window_rect(&self) -> RECT {
        let half = self.size / 2;
        RECT {
            left: self.center.0 - half,
            top: self.center.1 - half,
            right: self.center.0 + half,
            bottom: self.center.1 + half,
        }
    }

    fn slot_center(&self, index: usize) -> (i32, i32) {
        let count = self.items.len().max(1);
        let step = std::f32::consts::TAU / count as f32;
        let angle = -std::f32::consts::FRAC_PI_2 + step * index as f32;
        (
            self.center.0 + (self.radius as f32 * angle.cos()).round() as i32,
            self.center.1 + (self.radius as f32 * angle.sin()).round() as i32,
        )
    }

    /// Menggambar ulang seluruh menu.
    pub fn redraw(&mut self) -> Result<()> {
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        surface.clear()?;
        let local_center = (self.size as f32 / 2.0, self.size as f32 / 2.0);
        let outer = self.radius as f32 + self.slot_radius as f32 + 6.0;
        surface.fill_circle(local_center.0, local_center.1, outer, self.palette.backdrop)?;
        surface.stroke_circle(
            local_center.0,
            local_center.1,
            outer - 1.0,
            1.0,
            self.palette.ball_ring,
        )?;

        for index in 0..self.items.len() {
            self.draw_slot(index, surface, local_center)?;
        }

        if self.items.is_empty() {
            surface.text(
                "Belum ada slot",
                (0.0, local_center.1 - 8.0, self.size as f32, 20.0),
                12.0,
                self.palette.muted,
                true,
                false,
            )?;
        }

        surface.present(
            self.hwnd,
            self.center.0 - self.size / 2,
            self.center.1 - self.size / 2,
            1.0,
        )
    }

    fn draw_slot(&self, index: usize, surface: &Surface, _local_center: (f32, f32)) -> Result<()> {
        let (x, y) = self.slot_center(index);
        let active = self.hovered == Some(index) || self.selected == Some(index);
        let radius = self.slot_radius as f32 * if active { 1.09 } else { 1.0 };

        surface.fill_circle(x as f32, y as f32 + 1.5, radius, self.palette.shadow)?;
        surface.fill_circle(
            x as f32,
            y as f32,
            radius,
            if active {
                self.palette.accent
            } else {
                self.palette.slot
            },
        )?;
        if active {
            surface.stroke_circle(x as f32, y as f32, radius + 1.5, 1.2, self.palette.accent)?;
        }

        let glyph_size = radius * 1.05;
        match self.images.get(index).and_then(Option::as_ref) {
            Some(image) => {
                surface.draw_image(image, x as f32, y as f32 - radius * 0.12, glyph_size)?
            }
            None => {
                let letter = self
                    .items
                    .get(index)
                    .and_then(|item| item.label.chars().next())
                    .map(|ch| ch.to_uppercase().to_string())
                    .unwrap_or_default();
                surface.text(
                    &letter,
                    (
                        x as f32 - radius,
                        y as f32 - radius * 0.85,
                        radius * 2.0,
                        radius * 1.4,
                    ),
                    radius * 0.95,
                    self.palette.text,
                    true,
                    true,
                )?;
            }
        }

        // Label di bawah tiap slot.
        let label_width = 104.0;
        surface.text(
            &self.items[index].label,
            (
                x as f32 - label_width / 2.0,
                y as f32 + radius + 2.0,
                label_width,
                16.0,
            ),
            10.5,
            if active {
                self.palette.text
            } else {
                self.palette.muted
            },
            true,
            false,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> RadialMenu {
        let mut menu = RadialMenu::new(
            HWND(std::ptr::null_mut()),
            Palette {
                ball_base: 0,
                ball_gloss: 0,
                ball_ring: 0,
                accent: 0,
                text: 0,
                muted: 0,
                shadow: 0,
                backdrop: 0,
                slot: 0,
            },
        );
        menu.radius = 78;
        menu.slot_radius = 27;
        menu.size = 262;
        menu.items = vec![
            MenuItem {
                label: "A".to_string(),
                action: Action::Keys {
                    combo: "F5".to_string(),
                },
                icon: None,
            };
            6
        ];
        menu
    }

    fn point_at(center: (i32, i32)) -> POINT {
        POINT {
            x: center.0,
            y: center.1,
        }
    }

    #[test]
    fn six_slots_are_evenly_placed() {
        let menu = menu();
        let first = menu.slot_center(0);
        let second = menu.slot_center(1);
        assert_eq!(first.0, menu.center.0);
        assert!(first.1 < menu.center.1);
        assert!(second.0 > menu.center.0);
        assert!(second.1 < menu.center.1);
    }

    #[test]
    fn hit_test_finds_slot_center() {
        let mut menu = menu();
        menu.visible = true;
        assert_eq!(menu.hit_test(point_at(menu.slot_center(3))), Some(3));
    }

    #[test]
    fn hit_test_ignores_far_points() {
        let mut menu = menu();
        menu.visible = true;
        let point = point_at(menu.center);
        assert_eq!(menu.hit_test(point), None);
    }

    #[test]
    fn hit_test_is_inactive_when_hidden() {
        let menu = menu();
        assert_eq!(menu.hit_test(point_at(menu.slot_center(0))), None);
    }

    #[test]
    fn selection_wraps_around() {
        let mut menu = menu();
        menu.selected = Some(5);
        menu.move_selection(true).expect("gerak");
        assert_eq!(menu.selected, Some(0));
        menu.move_selection(false).expect("gerak");
        assert_eq!(menu.selected, Some(5));
    }
}
