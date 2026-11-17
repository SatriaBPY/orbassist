//! Jendela bola melayang: gambar, geometri snap, dan opacity.

use anyhow::Result;
use windows::Win32::Foundation::{HWND, RECT};

use crate::platform::render::Surface;
use crate::platform::window;
use crate::ui::Palette;

/// Jarak bola ke tepi layar saat menempel.
pub const SNAP_MARGIN: i32 = 4;
/// Jarak awal bola dari tepi area kerja saat pertama kali dijalankan.
const DEFAULT_INSET: i32 = 24;

/// Bola melayang yang tidak pernah mengambil fokus.
pub struct Ball {
    hwnd: HWND,
    surface: Option<Surface>,
    size: i32,
    opacity: f32,
    palette: Palette,
    position: (i32, i32),
}

impl Ball {
    pub fn new(hwnd: HWND, palette: Palette) -> Self {
        Self {
            hwnd,
            surface: None,
            size: 0,
            opacity: 1.0,
            palette,
            position: (0, 0),
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn position(&self) -> (i32, i32) {
        self.position
    }

    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    /// Ukuran fisik bola (px) untuk ukuran logis dan skala DPI tertentu.
    pub fn scaled_size(logical: u32, scale: f32) -> i32 {
        ((logical as f32 * scale).round() as i32).clamp(24, 200)
    }

    /// Mengatur ulang ukuran dan membuat permukaan gambar baru.
    pub fn set_size(&mut self, logical: u32, scale: f32) -> Result<()> {
        let size = Self::scaled_size(logical, scale);
        if self.size == size && self.surface.is_some() {
            return Ok(());
        }
        self.size = size;
        self.surface = Some(Surface::new(size, size)?);
        self.redraw()
    }

    /// Menampilkan bola di posisi tertentu.
    pub fn show_at(&mut self, position: (i32, i32)) -> Result<()> {
        self.position = position;
        self.redraw()?;
        window::show_no_activate(self.hwnd);
        Ok(())
    }

    /// Memindahkan bola tanpa menggambar ulang isi.
    pub fn move_to(&mut self, position: (i32, i32)) {
        self.position = position;
        window::move_to(self.hwnd, position.0, position.1);
    }

    pub fn hide(&self) {
        window::hide(self.hwnd);
    }

    pub fn is_visible(&self) -> bool {
        window::is_visible(self.hwnd)
    }

    /// Mengubah opacity jendela. Mengembalikan `true` bila gambar diperbarui.
    pub fn set_opacity(&mut self, opacity: f32) -> Result<bool> {
        let opacity = opacity.clamp(0.0, 1.0);
        if (opacity - self.opacity).abs() < 0.004 {
            return Ok(false);
        }
        self.opacity = opacity;
        self.present()?;
        Ok(true)
    }

    /// Menggambar ulang bola dan menampilkannya.
    pub fn redraw(&mut self) -> Result<()> {
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        draw_ball(surface, self.size, &self.palette)?;
        surface.present(self.hwnd, self.position.0, self.position.1, self.opacity)
    }

    fn present(&self) -> Result<()> {
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        surface.present(self.hwnd, self.position.0, self.position.1, self.opacity)
    }

    /// Piksel BGRA (alpha straight) untuk ikon tray.
    pub fn icon_pixels(&self, size: i32) -> Result<Vec<u8>> {
        let surface = Surface::new(size, size)?;
        let palette = self.palette;
        draw_ball(&surface, size, &palette)?;
        Ok(premultiplied_to_straight(surface.bits(), size))
    }
}

/// Menggambar tampilan bola: cakram, kilau, ring aksen, dan tiga titik.
fn draw_ball(surface: &Surface, size: i32, palette: &Palette) -> Result<()> {
    surface.clear()?;
    let center = size as f32 / 2.0;
    let radius = center - 1.0;

    // Bayangan lembut di bawah bola.
    surface.fill_circle(
        center,
        center + radius * 0.06,
        radius * 0.98,
        palette.shadow,
    )?;
    // Cakram utama dan kilau.
    surface.fill_circle(center, center, radius, palette.ball_base)?;
    surface.stroke_circle(
        center,
        center,
        radius - 0.8,
        (size as f32 * 0.018).max(1.0),
        palette.ball_ring,
    )?;
    surface.fill_circle(
        center - radius * 0.22,
        center - radius * 0.3,
        radius * 0.42,
        palette.ball_gloss,
    )?;

    // Tiga titik aksen sebagai identitas Orb.
    let dot = (size as f32 * 0.055).max(1.6);
    let gap = dot * 2.4;
    for offset in [-gap, 0.0, gap] {
        surface.fill_circle(center + offset, center, dot, palette.accent)?;
    }
    Ok(())
}

/// Mengubah piksel BGRA premultiplied menjadi BGRA straight untuk ikon.
fn premultiplied_to_straight(bits: *const u8, size: i32) -> Vec<u8> {
    let count = (size * size) as usize;
    let mut output = vec![0u8; count * 4];
    if bits.is_null() {
        return output;
    }
    // SAFETY: permukaan berukuran size*size piksel, 4 byte per piksel.
    let source = unsafe { std::slice::from_raw_parts(bits, count * 4) };
    for index in 0..count {
        let base = index * 4;
        let alpha = source[base + 3];
        for channel in 0..3 {
            output[base + channel] = if alpha == 0 {
                0
            } else {
                ((u16::from(source[base + channel]) * 255) / u16::from(alpha)).min(255) as u8
            };
        }
        output[base + 3] = alpha;
    }
    output
}

/// Titik tengah bola.
pub fn center(position: (i32, i32), size: i32) -> (i32, i32) {
    (position.0 + size / 2, position.1 + size / 2)
}

/// Menjepit posisi bola agar seluruh badan bola ada di dalam area kerja.
pub fn clamp_to_area(position: (i32, i32), size: i32, area: RECT) -> (i32, i32) {
    let max_x = (area.right - size).max(area.left);
    let max_y = (area.bottom - size).max(area.top);
    (
        position.0.clamp(area.left, max_x),
        position.1.clamp(area.top, max_y),
    )
}

/// Posisi menempel ke tepi terdekat area kerja.
pub fn snap_target(position: (i32, i32), size: i32, area: RECT) -> (i32, i32) {
    let clamped = clamp_to_area(position, size, area);
    let left = area.left + SNAP_MARGIN;
    let right = (area.right - size - SNAP_MARGIN).max(left);
    let top = area.top + SNAP_MARGIN;
    let bottom = (area.bottom - size - SNAP_MARGIN).max(top);

    let distances = [
        (clamped.0 - left).abs(),
        (clamped.0 - right).abs(),
        (clamped.1 - top).abs(),
        (clamped.1 - bottom).abs(),
    ];
    let mut nearest = 0;
    for (index, distance) in distances.iter().enumerate() {
        if *distance < distances[nearest] {
            nearest = index;
        }
    }

    match nearest {
        0 => (left, clamped.1),
        1 => (right, clamped.1),
        2 => (clamped.0, top),
        _ => (clamped.0, bottom),
    }
}

/// Posisi default: pojok kanan bawah area kerja.
pub fn default_position(size: i32, area: RECT) -> (i32, i32) {
    clamp_to_area(
        (
            area.right - size - DEFAULT_INSET,
            area.bottom - size - DEFAULT_INSET,
        ),
        size,
        area,
    )
}

/// Memastikan posisi lama masih terlihat di salah satu area kerja.
pub fn is_within_any(position: (i32, i32), size: i32, areas: &[RECT]) -> bool {
    let (cx, cy) = center(position, size);
    areas
        .iter()
        .any(|area| cx >= area.left && cx <= area.right && cy >= area.top && cy <= area.bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> RECT {
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }
    }

    #[test]
    fn clamp_keeps_ball_inside() {
        assert_eq!(clamp_to_area((1900, 1020), 48, area()), (1872, 992));
        assert_eq!(clamp_to_area((-50, -50), 48, area()), (0, 0));
    }

    #[test]
    fn snap_picks_nearest_horizontal_edge() {
        assert_eq!(snap_target((1800, 500), 48, area()), (1868, 500));
        assert_eq!(snap_target((30, 500), 48, area()), (4, 500));
    }

    #[test]
    fn snap_picks_nearest_vertical_edge() {
        assert_eq!(snap_target((900, 20), 48, area()), (900, 4));
        assert_eq!(snap_target((900, 980), 48, area()), (900, 988));
    }

    #[test]
    fn default_position_is_bottom_right() {
        let (x, y) = default_position(48, area());
        assert_eq!((x, y), (1848, 968));
    }

    #[test]
    fn validation_rejects_offscreen_positions() {
        let other = RECT {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1040,
        };
        assert!(is_within_any((100, 100), 48, &[area(), other]));
        assert!(is_within_any((2000, 100), 48, &[area(), other]));
        assert!(!is_within_any((5000, 5000), 48, &[area(), other]));
    }
}
