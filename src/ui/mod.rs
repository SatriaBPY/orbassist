//! Modul antarmuka Orb: bola, menu radial, dan form pengaturan.

pub mod ball;
pub mod radial;
pub mod settings;

use crate::config::Theme;
use crate::platform::render::{Color, argb};

/// Warna yang dipakai UI, diturunkan dari tema terang/gelap.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub ball_base: Color,
    pub ball_gloss: Color,
    pub ball_ring: Color,
    pub accent: Color,
    pub text: Color,
    pub muted: Color,
    pub shadow: Color,
    pub backdrop: Color,
    pub slot: Color,
}

/// Palet warna untuk tema yang dipilih pengguna.
pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Dark => Palette {
            ball_base: argb(30, 34, 41, 242),
            ball_gloss: argb(255, 255, 255, 26),
            ball_ring: argb(91, 157, 255, 120),
            accent: argb(91, 157, 255, 255),
            text: argb(240, 244, 250, 255),
            muted: argb(154, 164, 178, 255),
            shadow: argb(0, 0, 0, 110),
            backdrop: argb(14, 17, 22, 150),
            slot: argb(38, 44, 54, 246),
        },
        Theme::Light => Palette {
            ball_base: argb(255, 255, 255, 246),
            ball_gloss: argb(255, 255, 255, 210),
            ball_ring: argb(37, 99, 235, 150),
            accent: argb(37, 99, 235, 255),
            text: argb(17, 24, 39, 255),
            muted: argb(107, 114, 128, 255),
            shadow: argb(15, 23, 42, 70),
            backdrop: argb(244, 246, 250, 205),
            slot: argb(255, 255, 255, 250),
        },
    }
}
