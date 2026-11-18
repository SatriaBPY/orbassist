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
    pub assets: Assets,
    pub active_profile: usize,
    pub profile: Vec<Profile>,
}

/// Aset gambar opsional. Semua nilai kosong berarti memakai gambar bawaan.
///
/// Path relatif dihitung dari [`Assets::dir`]. Jika `ball`/`tray` kosong, Orb
/// mencari `ball.png`/`tray.png` (juga `.ico`, `.jpg`, `.jpeg`, `.bmp`) di
/// folder aset.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Assets {
    /// Folder aset. Kosong = `%APPDATA%\Orb\assets`.
    pub dir: String,
    /// Gambar bola. Kosong = bola bawaan (digambar Orb).
    pub ball: String,
    /// Ikon tray. Kosong = ikon dari bola.
    pub tray: String,
}

/// Ekstensi yang dicoba saat mencari `ball.*`/`tray.*` di folder aset.
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "ico", "jpg", "jpeg", "bmp"];

/// Folder aset default: `%APPDATA%\Orb\assets`.
pub fn default_assets_dir() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(dir) => PathBuf::from(dir).join("Orb").join("assets"),
        None => PathBuf::from("assets"),
    }
}

impl Assets {
    /// Folder aset efektif.
    pub fn dir(&self) -> PathBuf {
        let dir = self.dir.trim();
        if dir.is_empty() {
            default_assets_dir()
        } else {
            PathBuf::from(dir)
        }
    }

    /// Mengubah nilai konfigurasi menjadi path: absolut dipakai apa adanya,
    /// relatif dihitung dari folder aset, kosong berarti `None`.
    pub fn resolve(&self, value: &str) -> Option<PathBuf> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let path = Path::new(value);
        if path.is_absolute() {
            Some(path.to_path_buf())
        } else {
            Some(self.dir().join(path))
        }
    }

    /// Path eksplisit, atau `stem.<ext>` pertama yang ada di folder aset.
    fn image(&self, value: &str, stem: &str) -> Option<PathBuf> {
        if let Some(path) = self.resolve(value) {
            return Some(path);
        }
        let dir = self.dir();
        IMAGE_EXTENSIONS
            .iter()
            .map(|ext| dir.join(format!("{stem}.{ext}")))
            .find(|path| path.is_file())
    }

    /// Gambar bola kustom (jika ada).
    pub fn ball_image(&self) -> Option<PathBuf> {
        self.image(&self.ball, "ball")
    }

    /// Ikon tray kustom (jika ada).
    pub fn tray_image(&self) -> Option<PathBuf> {
        self.image(&self.tray, "tray")
    }

    /// Path ikon slot menu; `None` berarti pakai huruf awal label.
    pub fn slot_icon(&self, icon: Option<&str>) -> Option<PathBuf> {
        icon.and_then(|icon| self.resolve(icon))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BallConfig {
    pub size: u32,
    pub min_opacity: f32,
    pub snap_to_edge: bool,
    pub position: Option<Position>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub double_click: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flick_up: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flick_down: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flick_left: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flick_right: Option<Action>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    pub slot: Vec<Slot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    Submenu { items: Vec<Slot> },
}

/// Aksi sistem yang didukung (PRD F18).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemAction {
    VolumeUp,
    VolumeDown,
    Mute,
    MuteMic,
    MediaPlayPause,
    MediaNext,
    MediaPrev,
    BrightnessUp,
    BrightnessDown,
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
    Submenu,
}

impl ActionKind {
    pub const ALL: [ActionKind; 6] = [
        ActionKind::App,
        ActionKind::Folder,
        ActionKind::Url,
        ActionKind::Keys,
        ActionKind::System,
        ActionKind::Submenu,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::App => "Aplikasi",
            ActionKind::Folder => "Folder",
            ActionKind::Url => "URL",
            ActionKind::Keys => "Pintasan",
            ActionKind::System => "Sistem",
            ActionKind::Submenu => "Submenu",
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
            Action::Submenu { .. } => ActionKind::Submenu,
        }
    }

    /// Nilai teks yang ditampilkan di kolom parameter form pengaturan.
    pub fn parameter(&self) -> String {
        match self {
            Action::App { path } | Action::Folder { path } => path.clone(),
            Action::Url { url } => url.clone(),
            Action::Keys { combo } => combo.clone(),
            Action::System { name } => name.as_str().to_string(),
            Action::Submenu { items } => format!("{} item", items.len()),
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
                    "Nama aksi sistem tidak dikenal. Pilihan: volume_up, volume_down, mute, \
                     mute_mic, media_play_pause, media_next, media_prev, brightness_up, \
                     brightness_down, lock, screenshot."
                        .to_string()
                }),
            ActionKind::Submenu => Ok(Action::Submenu { items: Vec::new() }),
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
            Action::Submenu { items } => format!("submenu ({} item)", items.len()),
        }
    }
}

impl SystemAction {
    pub const ALL: [SystemAction; 11] = [
        SystemAction::VolumeUp,
        SystemAction::VolumeDown,
        SystemAction::Mute,
        SystemAction::MuteMic,
        SystemAction::MediaPlayPause,
        SystemAction::MediaNext,
        SystemAction::MediaPrev,
        SystemAction::BrightnessUp,
        SystemAction::BrightnessDown,
        SystemAction::Lock,
        SystemAction::Screenshot,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SystemAction::VolumeUp => "Naikkan Volume",
            SystemAction::VolumeDown => "Turunkan Volume",
            SystemAction::Mute => "Bisukan Audio",
            SystemAction::MuteMic => "Bisukan Mikrofon",
            SystemAction::MediaPlayPause => "Play / Pause Media",
            SystemAction::MediaNext => "Lagu Berikutnya",
            SystemAction::MediaPrev => "Lagu Sebelumnya",
            SystemAction::BrightnessUp => "Naikkan Kecerahan",
            SystemAction::BrightnessDown => "Turunkan Kecerahan",
            SystemAction::Lock => "Kunci Layar",
            SystemAction::Screenshot => "Screenshot",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            SystemAction::VolumeUp => "Vol +",
            SystemAction::VolumeDown => "Vol -",
            SystemAction::Mute => "Mute",
            SystemAction::MuteMic => "Mute Mic",
            SystemAction::MediaPlayPause => "Play/Pause",
            SystemAction::MediaNext => "Next",
            SystemAction::MediaPrev => "Prev",
            SystemAction::BrightnessUp => "Terang +",
            SystemAction::BrightnessDown => "Terang -",
            SystemAction::Lock => "Lock",
            SystemAction::Screenshot => "Screenshot",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|item| *item == self).unwrap_or(0)
    }

    pub fn from_index(index: i32) -> SystemAction {
        usize::try_from(index)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or(SystemAction::Mute)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SystemAction::VolumeUp => "volume_up",
            SystemAction::VolumeDown => "volume_down",
            SystemAction::Mute => "mute",
            SystemAction::MuteMic => "mute_mic",
            SystemAction::MediaPlayPause => "media_play_pause",
            SystemAction::MediaNext => "media_next",
            SystemAction::MediaPrev => "media_prev",
            SystemAction::BrightnessUp => "brightness_up",
            SystemAction::BrightnessDown => "brightness_down",
            SystemAction::Lock => "lock",
            SystemAction::Screenshot => "screenshot",
        }
    }

    pub fn parse(value: &str) -> Option<SystemAction> {
        match value.trim().to_ascii_lowercase().as_str() {
            "volume_up" => Some(SystemAction::VolumeUp),
            "volume_down" => Some(SystemAction::VolumeDown),
            "mute" => Some(SystemAction::Mute),
            "mute_mic" => Some(SystemAction::MuteMic),
            "media_play_pause" | "play_pause" => Some(SystemAction::MediaPlayPause),
            "media_next" | "next_track" => Some(SystemAction::MediaNext),
            "media_prev" | "prev_track" => Some(SystemAction::MediaPrev),
            "brightness_up" => Some(SystemAction::BrightnessUp),
            "brightness_down" => Some(SystemAction::BrightnessDown),
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
            double_click: None,
            flick_up: None,
            flick_down: None,
            flick_left: None,
            flick_right: None,
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
            assets: Assets::default(),
            active_profile: 0,
            profile: vec![Profile::default(), Profile::coding_default()],
        }
    }
}

impl Profile {
    /// Profil bawaan untuk companion VS Code / Zed / Cursor.
    pub fn coding_default() -> Self {
        let key_slot = |label: &str, combo: &str| Slot {
            label: label.to_string(),
            icon: None,
            action: Action::Keys {
                combo: combo.to_string(),
            },
        };
        Self {
            name: "Coding (VS Code/Zed)".to_string(),
            apps: vec![
                "code.exe".to_string(),
                "zed.exe".to_string(),
                "cursor.exe".to_string(),
            ],
            slot: vec![
                key_slot("Run", "F5"),
                key_slot("Terminal", "Ctrl+Backtick"),
                key_slot("Command", "Ctrl+Shift+P"),
                key_slot("Files", "Ctrl+P"),
                key_slot("Search", "Ctrl+Shift+F"),
                key_slot("Format", "Shift+Alt+F"),
            ],
        }
    }
}

impl Default for Profile {
    fn default() -> Self {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string());
        Self {
            name: "Default".to_string(),
            apps: Vec::new(),
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
        if self.active_profile >= self.profile.len() {
            self.active_profile = 0;
        }

        for profile in &mut self.profile {
            profile.name = profile.name.trim().to_string();
            if profile.name.is_empty() {
                profile.name = "Default".to_string();
            }
            profile.apps = profile
                .apps
                .iter()
                .map(|app| app.trim().to_ascii_lowercase())
                .filter(|app| !app.is_empty())
                .collect();

            if profile.slot.len() > SLOT_COUNT {
                warnings.push(format!(
                    "Profile '{}' memuat {} slot, hanya {} slot pertama yang dipakai.",
                    profile.name,
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

        self.assets.dir = self.assets.dir.trim().to_string();
        self.assets.ball = self.assets.ball.trim().to_string();
        self.assets.tray = self.assets.tray.trim().to_string();
    }

    /// Slot pada profile aktif.
    pub fn slots(&self) -> &[Slot] {
        self.slots_for(self.active_profile)
    }

    /// Slot pada indeks profile tertentu.
    pub fn slots_for(&self, index: usize) -> &[Slot] {
        self.profile
            .get(index)
            .or_else(|| self.profile.first())
            .map(|profile| profile.slot.as_slice())
            .unwrap_or(&[])
    }

    /// Memilih indeks profile yang cocok dengan proses aktif, atau `active_profile`.
    pub fn profile_for_process(&self, process_name: Option<&str>) -> usize {
        if let Some(proc) = process_name
            && let Some(index) = self.profile.iter().position(|profile| {
                profile
                    .apps
                    .iter()
                    .any(|app| app.eq_ignore_ascii_case(proc))
            })
        {
            return index;
        }
        self.active_profile
            .min(self.profile.len().saturating_sub(1))
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
    fn assets_resolve_relative_and_absolute_paths() {
        let assets = Assets {
            dir: r"C:\Orb Assets".to_string(),
            ball: "custom.png".to_string(),
            tray: String::new(),
        };
        assert_eq!(
            assets.ball_image(),
            Some(PathBuf::from(r"C:\Orb Assets\custom.png"))
        );
        assert_eq!(
            assets.slot_icon(Some("chrome.png")),
            Some(PathBuf::from(r"C:\Orb Assets\chrome.png"))
        );
        assert_eq!(
            assets.slot_icon(Some(r"D:\icons\calc.ico")),
            Some(PathBuf::from(r"D:\icons\calc.ico"))
        );
        assert_eq!(assets.slot_icon(None), None);
        assert!(assets.tray_image().is_none());
    }

    #[test]
    fn assets_find_named_files_when_config_is_empty() {
        let dir = std::env::temp_dir().join(format!("orb-assets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder uji");
        let ball = dir.join("ball.png");
        std::fs::write(&ball, b"bukan gambar sungguhan").expect("tulis berkas uji");

        let assets = Assets {
            dir: dir.to_string_lossy().into_owned(),
            ..Assets::default()
        };
        assert_eq!(assets.ball_image(), Some(ball));
        assert_eq!(assets.tray_image(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_assets_keep_strings_empty_after_normalize() {
        let mut config = Config::default();
        config.assets.ball = "  ball.png  ".to_string();
        config.assets.dir = "  ".to_string();
        let mut warnings = Vec::new();
        config.normalize(&mut warnings);
        assert_eq!(config.assets.ball, "ball.png");
        assert_eq!(config.assets.dir, "");
        assert_eq!(config.assets.tray, "");
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

    #[test]
    fn media_system_actions_parse_and_round_trip() {
        assert_eq!(
            SystemAction::parse("media_play_pause"),
            Some(SystemAction::MediaPlayPause)
        );
        assert_eq!(
            SystemAction::parse("media_next"),
            Some(SystemAction::MediaNext)
        );
        assert_eq!(
            SystemAction::parse("media_prev"),
            Some(SystemAction::MediaPrev)
        );
    }

    #[test]
    fn profile_for_process_matches_case_insensitively_or_falls_back() {
        let mut config = Config::default();
        config.profile.push(Profile {
            name: "Coding".to_string(),
            apps: vec!["code.exe".to_string()],
            slot: Profile::default().slot,
        });
        assert_eq!(config.profile_for_process(Some("Code.exe")), 1);
        assert_eq!(config.profile_for_process(Some("notepad.exe")), 0);
        assert_eq!(config.profile_for_process(None), 0);
    }
}
