//! Model konfigurasi Orb dan baca/tulis berkas TOML.
//!
//! Berkas disimpan di `%APPDATA%\Orb\config.toml`. Tidak ada data yang dikirim
//! keluar dari komputer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Jumlah slot menu radial (PRD F4).
pub const SLOT_COUNT: usize = 6;
/// Ukuran bola minimum dalam px logis (PRD §7.1).
pub const MIN_BALL_SIZE: u32 = 32;
/// Ukuran bola maksimum dalam px logis (PRD §7.1).
pub const MAX_BALL_SIZE: u32 = 96;

/// Lokasi berkas konfigurasi default.
pub fn default_path() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(dir) => PathBuf::from(dir).join("Orb").join("config.toml"),
        None => PathBuf::from("config.toml"),
    }
}

/// Konfigurasi lengkap Orb.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub ball: BallConfig,
    pub hotkey: HotkeyConfig,
    pub theme: Theme,
    pub autostart: bool,
    pub profile: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BallConfig {
    pub size: u32,
    pub min_opacity: f32,
    pub snap_to_edge: bool,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HotkeyConfig {
    pub open_menu: String,
    pub find_ball: String,
    pub mode: HotkeyMode,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HotkeyMode {
    #[default]
    Toggle,
    Hold,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub name: String,
    pub slot: Vec<Slot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Slot {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub action: Action,
}

/// Aksi yang bisa dipicu dari slot menu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Action {
    App { path: String },
    Folder { path: String },
    Url { url: String },
    Keys { combo: String },
    System { name: SystemAction },
}

/// Aksi sistem yang didukung (PRD F18, sebagian).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemAction {
    VolumeUp,
    VolumeDown,
    Mute,
    Lock,
    Screenshot,
}

/// Jenis aksi untuk form pengaturan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    App,
    Folder,
    Url,
    Keys,
    System,
}

impl ActionKind {
    pub const ALL: [ActionKind; 5] = [
        ActionKind::App,
        ActionKind::Folder,
        ActionKind::Url,
        ActionKind::Keys,
        ActionKind::System,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::App => "Aplikasi",
            ActionKind::Folder => "Folder",
            ActionKind::Url => "URL",
            ActionKind::Keys => "Pintasan",
            ActionKind::System => "Sistem",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }

    pub fn from_index(index: i32) -> ActionKind {
        usize::try_from(index)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or(ActionKind::App)
    }
}

impl Action {
    pub fn kind(&self) -> ActionKind {
        match self {
            Action::App { .. } => ActionKind::App,
            Action::Folder { .. } => ActionKind::Folder,
            Action::Url { .. } => ActionKind::Url,
            Action::Keys { .. } => ActionKind::Keys,
            Action::System { .. } => ActionKind::System,
        }
    }

    /// Nilai teks yang ditampilkan di kolom parameter form pengaturan.
    pub fn parameter(&self) -> String {
        match self {
            Action::App { path } | Action::Folder { path } => path.clone(),
            Action::Url { url } => url.clone(),
            Action::Keys { combo } => combo.clone(),
            Action::System { name } => name.as_str().to_string(),
        }
    }

    /// Membangun aksi dari pilihan jenis + parameter teks.
    pub fn from_parts(kind: ActionKind, parameter: &str) -> Result<Action, String> {
        let value = parameter.trim().to_string();
        if value.is_empty() {
            return Err("Parameter aksi belum diisi.".to_string());
        }
        match kind {
            ActionKind::App => Ok(Action::App { path: value }),
            ActionKind::Folder => Ok(Action::Folder { path: value }),
            ActionKind::Url => {
                if !(value.starts_with("http://")
                    || value.starts_with("https://")
                    || value.starts_with("ms-settings:"))
                {
                    return Err("URL harus diawali http:// atau https://.".to_string());
                }
                Ok(Action::Url { url: value })
            }
            ActionKind::Keys => Ok(Action::Keys { combo: value }),
            ActionKind::System => SystemAction::parse(&value)
                .map(|name| Action::System { name })
                .ok_or_else(|| {
                    "Nama aksi sistem tidak dikenal. Pilihan: volume_up, volume_down, mute, lock, \
                     screenshot."
                        .to_string()
                }),
        }
    }

    /// Label pendek untuk pesan error dan log.
    pub fn describe(&self) -> String {
        match self {
            Action::App { path } => format!("aplikasi {path}"),
            Action::Folder { path } => format!("folder {path}"),
            Action::Url { url } => format!("URL {url}"),
            Action::Keys { combo } => format!("pintasan {combo}"),
            Action::System { name } => format!("aksi sistem {}", name.as_str()),
        }
    }
}

impl SystemAction {
    pub fn as_str(self) -> &'static str {
        match self {
            SystemAction::VolumeUp => "volume_up",
            SystemAction::VolumeDown => "volume_down",
            SystemAction::Mute => "mute",
            SystemAction::Lock => "lock",
            SystemAction::Screenshot => "screenshot",
        }
    }

    pub fn parse(value: &str) -> Option<SystemAction> {
        match value.trim().to_ascii_lowercase().as_str() {
            "volume_up" => Some(SystemAction::VolumeUp),
            "volume_down" => Some(SystemAction::VolumeDown),
            "mute" => Some(SystemAction::Mute),
            "lock" => Some(SystemAction::Lock),
            "screenshot" => Some(SystemAction::Screenshot),
            _ => None,
        }
    }
}

impl Default for BallConfig {
    fn default() -> Self {
        Self {
            size: 48,
            min_opacity: 0.25,
            snap_to_edge: true,
            position: None,
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            open_menu: "Ctrl+Alt+Space".to_string(),
            find_ball: "Ctrl+Alt+B".to_string(),
            mode: HotkeyMode::Toggle,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ball: BallConfig::default(),
            hotkey: HotkeyConfig::default(),
            theme: Theme::Dark,
            autostart: false,
            profile: vec![Profile::default()],
        }
    }
}

impl Default for Profile {
    fn default() -> Self {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string());
        Self {
            name: "Default".to_string(),
            slot: vec![
                Slot {
                    label: "Files".to_string(),
                    icon: None,
                    action: Action::Folder { path: home },
                },
                Slot {
                    label: "Screenshot".to_string(),
                    icon: None,
                    action: Action::Keys {
                        combo: "Win+Shift+S".to_string(),
                    },
                },
                Slot {
                    label: "Notepad".to_string(),
                    icon: None,
                    action: Action::App {
                        path: "notepad.exe".to_string(),
                    },
                },
                Slot {
                    label: "Task Manager".to_string(),
                    icon: None,
                    action: Action::Keys {
                        combo: "Ctrl+Shift+Esc".to_string(),
                    },
                },
                Slot {
                    label: "Calculator".to_string(),
                    icon: None,
                    action: Action::App {
                        path: "calc.exe".to_string(),
                    },
                },
                Slot {
                    label: "Settings".to_string(),
                    icon: None,
                    action: Action::Keys {
                        combo: "Win+I".to_string(),
                    },
                },
            ],
        }
    }
}

impl Config {
    /// Memuat konfigurasi. Tidak pernah gagal: jika berkas hilang/rusak,
    /// konfigurasi default dipakai dan peringatan dikembalikan.
    pub fn load(path: &Path) -> (Config, Vec<String>) {
        let mut warnings = Vec::new();

        if !path.exists() {
            let config = Config::default();
            if let Err(err) = config.save(path) {
                warnings.push(format!("Gagal membuat konfigurasi default: {err}"));
            }
            return (config, warnings);
        }

        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                warnings.push(format!(
                    "Gagal membaca {}: {err}. Memakai konfigurasi default.",
                    path.display()
                ));
                return (Config::default(), warnings);
            }
        };

        match toml::from_str::<Config>(&text) {
            Ok(mut config) => {
                config.normalize(&mut warnings);
                (config, warnings)
            }
            Err(err) => {
                warnings.push(format!(
                    "Konfigurasi di {} tidak valid: {err}. Memakai konfigurasi default.",
                    path.display()
                ));
                (Config::default(), warnings)
            }
        }
    }

    /// Menyimpan konfigurasi secara atomik (tulis sementara lalu rename).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        use anyhow::Context;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("gagal membuat folder {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("gagal membuat TOML")?;
        let temporary = path.with_extension("toml.tmp");
        std::fs::write(&temporary, text)
            .with_context(|| format!("gagal menulis {}", temporary.display()))?;
        std::fs::rename(&temporary, path)
            .with_context(|| format!("gagal mengganti {}", path.display()))?;
        Ok(())
    }

    /// Merapikan nilai yang tidak masuk akal dan mencatat peringatan.
    pub fn normalize(&mut self, warnings: &mut Vec<String>) {
        let size = self.ball.size.clamp(MIN_BALL_SIZE, MAX_BALL_SIZE);
        if size != self.ball.size {
            warnings.push(format!(
                "Ukuran bola {} px di luar batas, disesuaikan ke {} px.",
                self.ball.size, size
            ));
            self.ball.size = size;
        }

        if !self.ball.min_opacity.is_finite() {
            self.ball.min_opacity = BallConfig::default().min_opacity;
        }
        let min_opacity = self.ball.min_opacity.clamp(0.05, 1.0);
        if (min_opacity - self.ball.min_opacity).abs() > f32::EPSILON {
            warnings.push(format!(
                "Opasitas minimum {} di luar batas, disesuaikan ke {}.",
                self.ball.min_opacity, min_opacity
            ));
            self.ball.min_opacity = min_opacity;
        }

        if self.profile.is_empty() {
            warnings.push("Tidak ada profile, profile default dipakai.".to_string());
            self.profile.push(Profile::default());
        }
        if self.profile.len() > 1 {
            warnings.push(
                "Lebih dari satu profile ditemukan; versi ini hanya memakai profile pertama."
                    .to_string(),
            );
        }

        let profile = &mut self.profile[0];
        if profile.slot.len() > SLOT_COUNT {
            warnings.push(format!(
                "Profile memuat {} slot, hanya {} slot pertama yang dipakai.",
                profile.slot.len(),
                SLOT_COUNT
            ));
            profile.slot.truncate(SLOT_COUNT);
        }
        while profile.slot.len() < SLOT_COUNT {
            let label = format!("Slot {}", profile.slot.len() + 1);
            profile.slot.push(Slot {
                label,
                icon: None,
                action: Action::System {
                    name: SystemAction::Mute,
                },
            });
        }
        for slot in &mut profile.slot {
            slot.label = slot.label.trim().to_string();
            if slot.label.is_empty() {
                slot.label = "Tanpa nama".to_string();
            }
        }
    }

    /// Slot pada profile aktif (profile pertama).
    pub fn slots(&self) -> &[Slot] {
        self.profile
            .first()
            .map(|profile| profile.slot.as_slice())
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).expect("serialisasi default gagal");
        let parsed: Config = toml::from_str(&text).expect("deserialisasi default gagal");
        assert_eq!(config, parsed);
    }

    #[test]
    fn normalize_clamps_size_and_opacity() {
        let mut config = Config {
            ball: BallConfig {
                size: 500,
                min_opacity: -1.0,
                ..BallConfig::default()
            },
            ..Config::default()
        };
        let mut warnings = Vec::new();
        config.normalize(&mut warnings);
        assert_eq!(config.ball.size, MAX_BALL_SIZE);
        assert_eq!(config.ball.min_opacity, 0.05);
        assert!(!warnings.is_empty());
    }

    #[test]
    fn normalize_pads_and_truncates_slots() {
        let mut config = Config::default();
        config.profile[0].slot.truncate(2);
        let mut warnings = Vec::new();
        config.normalize(&mut warnings);
        assert_eq!(config.slots().len(), SLOT_COUNT);

        let mut config = Config::default();
        let extra = config.profile[0].slot[0].clone();
        for _ in 0..4 {
            config.profile[0].slot.push(extra.clone());
        }
        let mut warnings = Vec::new();
        config.normalize(&mut warnings);
        assert_eq!(config.slots().len(), SLOT_COUNT);
    }

    #[test]
    fn action_parts_round_trip() {
        let action = Action::from_parts(ActionKind::Keys, " Ctrl+Shift+Esc ").expect("keys");
        assert_eq!(
            action,
            Action::Keys {
                combo: "Ctrl+Shift+Esc".to_string()
            }
        );
        assert_eq!(action.kind(), ActionKind::Keys);
        assert_eq!(action.parameter(), "Ctrl+Shift+Esc");

        let system = Action::from_parts(ActionKind::System, "mute").expect("system");
        assert_eq!(
            system,
            Action::System {
                name: SystemAction::Mute
            }
        );

        assert!(Action::from_parts(ActionKind::Url, "example.com").is_err());
        assert!(Action::from_parts(ActionKind::System, "reboot").is_err());
        assert!(Action::from_parts(ActionKind::App, "   ").is_err());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("orb-config-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);

        let mut config = Config::default();
        config.ball.size = 64;
        config.theme = Theme::Light;
        config.ball.position = Some(Position { x: 10, y: 20 });
        config.save(&path).expect("simpan");

        let (loaded, warnings) = Config::load(&path);
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert_eq!(loaded.ball.size, 64);
        assert_eq!(loaded.theme, Theme::Light);
        assert_eq!(loaded.ball.position, Some(Position { x: 10, y: 20 }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_file_falls_back_to_default() {
        let dir = std::env::temp_dir().join(format!("orb-config-broken-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(&path, "ini bukan toml").expect("write");

        let (loaded, warnings) = Config::load(&path);
        assert_eq!(loaded, Config::default());
        assert_eq!(warnings.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
