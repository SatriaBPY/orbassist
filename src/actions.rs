//! Menjalankan aksi dari slot menu.

use std::path::Path;

use thiserror::Error;

use crate::config::{Action, SystemAction};
use crate::platform::hotkey;
use crate::platform::input::{self, InputError};

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("berkas atau folder tidak ditemukan: {0}")]
    NotFound(String),
    #[error("folder tidak bisa dibuka karena bukan folder: {0}")]
    NotFolder(String),
    #[error(transparent)]
    Hotkey(#[from] hotkey::HotkeyError),
    #[error(transparent)]
    Input(#[from] InputError),
}

/// Menjalankan satu aksi. Error dikembalikan agar bisa ditampilkan ke pengguna.
pub fn run(action: &Action) -> Result<(), ActionError> {
    match action {
        Action::App { path } => {
            let target = expand_env(path);
            if !is_launchable(&target) {
                return Err(ActionError::NotFound(target));
            }
            input::shell_open(&target, None)?;
        }
        Action::Folder { path } => {
            let target = expand_env(path);
            if !Path::new(&target).is_dir() {
                return Err(ActionError::NotFolder(target));
            }
            input::shell_open(&target, None)?;
        }
        Action::Url { url } => {
            input::shell_open(url, None)?;
        }
        Action::Keys { combo } => {
            let hotkey = hotkey::parse_combo(combo)?;
            input::send_combo(hotkey)?;
        }
        Action::System { name } => run_system(*name)?,
    }
    log::info!("aksi dijalankan: {}", action.describe());
    Ok(())
}

fn run_system(name: SystemAction) -> Result<(), ActionError> {
    const VK_VOLUME_MUTE: u16 = 0xAD;
    const VK_VOLUME_DOWN: u16 = 0xAE;
    const VK_VOLUME_UP: u16 = 0xAF;

    match name {
        SystemAction::VolumeUp => input::send_key(VK_VOLUME_UP)?,
        SystemAction::VolumeDown => input::send_key(VK_VOLUME_DOWN)?,
        SystemAction::Mute => input::send_key(VK_VOLUME_MUTE)?,
        SystemAction::Lock => input::lock_workstation()?,
        SystemAction::Screenshot => input::shell_open("ms-screenclip:", None)?,
    }
    Ok(())
}

/// Perintah tanpa path absolut (misalnya `notepad.exe`) dibiarkan dicari oleh
/// shell lewat PATH; path lain harus ada.
fn is_launchable(target: &str) -> bool {
    let has_separator = target.contains('\\') || target.contains('/');
    if has_separator {
        Path::new(target).exists()
    } else {
        true
    }
}

/// Mengganti `%VARIABEL%` dengan nilai environment. Variabel yang tidak ada
/// dibiarkan apa adanya agar pesan error tetap informatif.
fn expand_env(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) if !name.is_empty() => result.push_str(&value),
                    _ => {
                        result.push('%');
                        result.push_str(name);
                        result.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                result.push('%');
                result.push_str(after);
                rest = "";
            }
        }
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_variables_are_expanded() {
        // SAFETY (test): variabel unik hanya dipakai test ini.
        unsafe {
            std::env::set_var("ORB_TEST_DIR", "C:\\OrbTest");
        }
        assert_eq!(expand_env("%ORB_TEST_DIR%\\data"), "C:\\OrbTest\\data");
    }

    #[test]
    fn unknown_variables_are_left_alone() {
        assert_eq!(
            expand_env("%ORB_SURELY_MISSING%\\x"),
            "%ORB_SURELY_MISSING%\\x"
        );
        assert_eq!(expand_env("tanpa-variabel"), "tanpa-variabel");
        assert_eq!(expand_env("50%"), "50%");
    }

    #[test]
    fn paths_without_separator_are_launchable() {
        assert!(is_launchable("notepad.exe"));
        assert!(!is_launchable("C:\\tidak-ada\\file.exe"));
    }
}
