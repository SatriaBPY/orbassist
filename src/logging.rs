//! File logger minimal untuk `log`.
//!
//! Log ditulis ke berkas di folder data Orb. Tidak ada jaringan, tidak ada
//! telemetri. Format: `YYYY-MM-DD HH:MM:SS LEVEL target - pesan`.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use log::{LevelFilter, Log, Metadata, Record};

/// Absolut path log default: `%LOCALAPPDATA%\Orb\orb.log`.
pub fn default_log_path() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(dir) => PathBuf::from(dir).join("Orb").join("orb.log"),
        None => PathBuf::from("orb.log"),
    }
}

/// Memasang logger global. Hanya boleh dipanggil sekali.
pub fn init(path: &Path, level: LevelFilter) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("gagal membuat folder log {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("gagal membuka log {}", path.display()))?;

    let logger = FileLogger {
        file: Mutex::new(Some(file)),
        level,
    };
    log::set_boxed_logger(Box::new(logger)).context("logger sudah terpasang")?;
    log::set_max_level(level);
    Ok(())
}

struct FileLogger {
    file: Mutex<Option<File>>,
    level: LevelFilter,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {} - {}\n",
            timestamp(SystemTime::now()),
            record.level(),
            record.target(),
            record.args()
        );
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            // Kegagalan menulis log tidak boleh menjatuhkan aplikasi.
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }

    fn flush(&self) {}
}

/// Timestamp UTC yang mudah dibaca, tanpa dependency tanggal.
fn timestamp(now: SystemTime) -> String {
    let secs = match now.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    };
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60
    )
}

/// Konversi hari sejak 1970-01-01 ke (tahun, bulan, hari) kalender proleptik.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn known_date_is_mapped_correctly() {
        // 2026-11-17 = 20774 hari setelah epoch.
        assert_eq!(civil_from_days(20_774), (2026, 11, 17));
    }

    #[test]
    fn timestamp_has_utc_suffix() {
        let stamp = timestamp(UNIX_EPOCH + std::time::Duration::from_secs(20_774 * 86_400 + 3_661));
        assert_eq!(stamp, "2026-11-17 01:01:01Z");
    }
}
