//! Parsing dan registrasi hotkey global (`RegisterHotKey`).

use thiserror::Error;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};

/// Hotkey yang sudah diterjemahkan ke nilai Win32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    /// Kombinasi bit `MOD_*`.
    pub modifiers: u32,
    /// Kode tombol virtual (`VK_*`).
    pub vk: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HotkeyError {
    #[error("hotkey kosong")]
    Empty,
    #[error("bagian '{0}' bukan tombol atau modifier yang dikenal")]
    UnknownPart(String),
    #[error("hotkey '{0}' memiliki lebih dari satu tombol utama")]
    MultipleKeys(String),
    #[error("hotkey '{0}' membutuhkan minimal satu modifier (Ctrl/Alt/Shift/Win)")]
    MissingModifier(String),
    #[error("hotkey '{0}' sudah dipakai aplikasi lain")]
    AlreadyTaken(String),
}

/// Hotkey untuk didaftarkan ke Windows. Dilepas otomatis saat di-`drop`.
pub struct Registration {
    id: i32,
}

impl Registration {
    /// Mendaftarkan hotkey global. `id` harus unik di dalam aplikasi.
    pub fn register(id: i32, hotkey: Hotkey, spec: &str) -> Result<Self, HotkeyError> {
        let modifiers = HOT_KEY_MODIFIERS(hotkey.modifiers | MOD_NOREPEAT.0);
        // SAFETY: id unik, modifier dan VK sudah divalidasi oleh parser.
        unsafe { RegisterHotKey(None, id, modifiers, u32::from(hotkey.vk)) }
            .map_err(|_| HotkeyError::AlreadyTaken(spec.to_string()))?;
        Ok(Self { id })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: id ini didaftarkan oleh `register` dan belum dilepas.
        unsafe {
            let _ = UnregisterHotKey(None, self.id);
        }
    }
}

/// Parse hotkey global: wajib punya minimal satu modifier.
pub fn parse(spec: &str) -> Result<Hotkey, HotkeyError> {
    let hotkey = parse_combo(spec)?;
    if hotkey.modifiers == 0 {
        return Err(HotkeyError::MissingModifier(spec.trim().to_string()));
    }
    Ok(hotkey)
}

/// Parse kombinasi tombol untuk dikirim lewat `SendInput`; modifier opsional.
pub fn parse_combo(spec: &str) -> Result<Hotkey, HotkeyError> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(HotkeyError::Empty);
    }

    let mut modifiers = 0u32;
    let mut key: Option<u16> = None;

    for part in trimmed.split('+') {
        let part = part.trim();
        if part.is_empty() {
            return Err(HotkeyError::UnknownPart(trimmed.to_string()));
        }
        if let Some(flag) = modifier_flag(part) {
            modifiers |= flag;
            continue;
        }
        let vk = key_code(part).ok_or_else(|| HotkeyError::UnknownPart(part.to_string()))?;
        if key.replace(vk).is_some() {
            return Err(HotkeyError::MultipleKeys(trimmed.to_string()));
        }
    }

    match key {
        Some(vk) => Ok(Hotkey { modifiers, vk }),
        None => Err(HotkeyError::UnknownPart(trimmed.to_string())),
    }
}

fn modifier_flag(part: &str) -> Option<u32> {
    match part.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(MOD_CONTROL.0),
        "alt" => Some(MOD_ALT.0),
        "shift" => Some(MOD_SHIFT.0),
        "win" | "windows" | "meta" | "super" => Some(MOD_WIN.0),
        _ => None,
    }
}

/// Memetakan nama tombol ke kode tombol virtual Windows.
fn key_code(part: &str) -> Option<u16> {
    let upper = part.to_ascii_uppercase();
    if upper.len() == 1 {
        let ch = upper.as_bytes()[0];
        if ch.is_ascii_uppercase() || ch.is_ascii_digit() {
            // Kode ASCII huruf besar dan angka sama dengan kode VK.
            return Some(u16::from(ch));
        }
    }

    if let Some(rest) = upper.strip_prefix('F')
        && let Ok(number) = rest.parse::<u16>()
        && (1..=24).contains(&number)
    {
        return Some(0x70 + number - 1);
    }

    let code = match upper.as_str() {
        "SPACE" => 0x20,
        "ENTER" | "RETURN" => 0x0D,
        "ESC" | "ESCAPE" => 0x1B,
        "TAB" => 0x09,
        "BACKSPACE" => 0x08,
        "DELETE" | "DEL" => 0x2E,
        "INSERT" | "INS" => 0x2D,
        "HOME" => 0x24,
        "END" => 0x23,
        "PGUP" | "PAGEUP" => 0x21,
        "PGDN" | "PAGEDOWN" => 0x22,
        "LEFT" => 0x25,
        "UP" => 0x26,
        "RIGHT" => 0x27,
        "DOWN" => 0x28,
        "PLUS" => 0xBB,
        "MINUS" => 0xBD,
        "COMMA" => 0xBC,
        "PERIOD" => 0xBE,
        "SLASH" => 0xBF,
        "SEMICOLON" => 0xBA,
        "QUOTE" => 0xDE,
        "BACKTICK" => 0xC0,
        "PRINTSCREEN" => 0x2C,
        "PAUSE" => 0x13,
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ctrl_alt_space() {
        let hotkey = parse("Ctrl+Alt+Space").expect("valid");
        assert_eq!(hotkey.modifiers, MOD_CONTROL.0 | MOD_ALT.0);
        assert_eq!(hotkey.vk, 0x20);
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!(
            parse("ctrl+shift+f5").expect("valid"),
            Hotkey {
                modifiers: MOD_CONTROL.0 | MOD_SHIFT.0,
                vk: 0x74
            }
        );
    }

    #[test]
    fn rejects_hotkey_without_modifier() {
        assert_eq!(
            parse("Space"),
            Err(HotkeyError::MissingModifier("Space".to_string()))
        );
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(matches!(
            parse("Ctrl+Banana"),
            Err(HotkeyError::UnknownPart(_))
        ));
    }

    #[test]
    fn rejects_multiple_keys() {
        assert!(matches!(
            parse("Ctrl+A+B"),
            Err(HotkeyError::MultipleKeys(_))
        ));
    }

    #[test]
    fn combo_without_modifier_is_allowed() {
        let hotkey = parse_combo("F5").expect("valid");
        assert_eq!(hotkey.modifiers, 0);
        assert_eq!(hotkey.vk, 0x74);
    }

    #[test]
    fn win_key_is_supported() {
        let hotkey = parse("Win+I").expect("valid");
        assert_eq!(hotkey.modifiers, MOD_WIN.0);
        assert_eq!(hotkey.vk, 0x49);
    }

    #[test]
    fn empty_spec_is_rejected() {
        assert_eq!(parse("   "), Err(HotkeyError::Empty));
    }
}
