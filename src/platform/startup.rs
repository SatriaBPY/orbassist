//! Mengaktifkan/mematikan Orb saat Windows menyala (registry `Run` milik HKCU).

use std::ptr::null_mut;

use anyhow::{Context, Result, bail};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ, RegCloseKey, RegDeleteValueW,
    RegOpenKeyExW, RegSetValueExW,
};
use windows::core::HSTRING;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Orb";

/// Menulis atau menghapus entri autostart.
pub fn set_enabled(enabled: bool) -> Result<()> {
    if enabled { enable() } else { disable() }
}

fn enable() -> Result<()> {
    let exe = std::env::current_exe().context("tidak bisa menemukan path orb.exe")?;
    let command = format!("\"{}\"", exe.display());
    let mut data: Vec<u8> = command.encode_utf16().flat_map(u16::to_le_bytes).collect();
    data.extend_from_slice(&[0, 0]);

    let key = open_key(KEY_SET_VALUE)?;
    let value_name = HSTRING::from(VALUE_NAME);
    // SAFETY: key dibuka dengan KEY_SET_VALUE dan data adalah REG_SZ yang valid.
    let result = unsafe { RegSetValueExW(key, &value_name, None, REG_SZ, Some(&data)) };
    // SAFETY: key milik pemanggil dan harus ditutup.
    unsafe {
        let _ = RegCloseKey(key);
    }
    check(result, "menulis autostart")
}

fn disable() -> Result<()> {
    let key = open_key(KEY_SET_VALUE)?;
    let value_name = HSTRING::from(VALUE_NAME);
    // SAFETY: key dibuka dengan KEY_SET_VALUE.
    let result = unsafe { RegDeleteValueW(key, &value_name) };
    // SAFETY: key milik pemanggil dan harus ditutup.
    unsafe {
        let _ = RegCloseKey(key);
    }
    // Menghapus nilai yang belum ada bukan kegagalan (ERROR_FILE_NOT_FOUND = 2).
    if result.is_ok() || result.0 == 2 {
        Ok(())
    } else {
        bail!("gagal menghapus autostart (kode {})", result.0)
    }
}

fn open_key(access: REG_SAM_FLAGS) -> Result<HKEY> {
    let subkey = HSTRING::from(RUN_KEY);
    let mut key = HKEY(null_mut());
    // SAFETY: subkey hidup selama panggilan dan hasil ditulis ke `key`.
    let result = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, access, &mut key) };
    if !result.is_ok() {
        bail!("gagal membuka kunci Run (kode {})", result.0);
    }
    Ok(key)
}

fn check(result: windows::Win32::Foundation::WIN32_ERROR, action: &str) -> Result<()> {
    if result.is_ok() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("gagal {action} (kode {})", result.0))
    }
}
