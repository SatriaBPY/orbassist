//! Orb - bola melayang untuk Windows.
//!
//! Entry point: siapkan logging, jalankan aplikasi, dan tampilkan pesan bila
//! startup gagal total.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod config;
mod logging;
mod platform;
mod ui;

use log::LevelFilter;

fn main() {
    let log_path = logging::default_log_path();
    let level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let logging_ready = logging::init(&log_path, level);

    if let Err(err) = app::run() {
        if logging_ready.is_ok() {
            log::error!("Orb berhenti karena kesalahan fatal: {err:#}");
        }
        platform::window::message_box(
            None,
            "Orb",
            &format!("Orb tidak bisa dijalankan.\n\n{err:#}"),
            true,
        );
        std::process::exit(1);
    }
}
