// SPDX-License-Identifier: GPL-3.0-only

//! `config.toml` (SPECIFICATION §5.4, §9). Unknown keys are kept as they are and
//! written back, so a file from a newer version or edited by hand loses nothing.
//! Writes are atomic and the file is readable only by the user (0600).

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Errors while reading or writing the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file exists but could not be read.
    #[error("reading {path}: {source}")]
    Read {
        /// The configuration file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file could not be written.
    #[error("writing {path}: {source}")]
    Write {
        /// The configuration file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is not valid TOML or has values of the wrong type. Only the
    /// parser's message and the line are kept: the parser's own text quotes the
    /// line, which may hold an API key (PRV-002).
    #[error("{path}, line {line}: {message}")]
    Parse {
        /// The configuration file.
        path: PathBuf,
        /// 1-based line of the error (0 if unknown).
        line: usize,
        /// The parser's message, without the quoted line.
        message: String,
    },
    /// The configuration could not be serialized (should not happen).
    #[error("serializing the configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `[general]`.
    pub general: General,
    /// `[player]`.
    pub player: PlayerConfig,
    /// `[subtitles]`.
    pub subtitles: SubtitlesConfig,
    /// `[lookup]`.
    pub lookup: LookupConfig,
    /// `[popup]`.
    pub popup: PopupConfig,
    /// `[paths]`.
    pub paths: PathsConfig,
    /// `[shortcuts]`: action → keys, only for the bindings changed by the user
    /// (KEY-001). The defaults live in the UI.
    pub shortcuts: BTreeMap<String, Vec<String>>,
    /// Sections this version does not know, kept as they are.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `[general]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// `"system"`, `"it"`, `"en"`, …
    pub ui_language: String,
    /// `"light"`, `"dark"` or `"system"`.
    pub theme: String,
    /// Language being studied (BCP 47), e.g. `"ja"`.
    pub study_language: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `[player]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    /// mpv `hwdec` (PLY-006): `auto-safe`, `auto-copy-safe` or `no`.
    pub hwdec: String,
    /// `"default"` or `"high-quality"` (PLY-006).
    pub quality_profile: String,
    /// Seek step in seconds, 1–30 (PLY-001).
    pub seek_seconds: u8,
    /// Start the next episode at the end (LIB-009).
    pub auto_next: bool,
    /// Raw mpv options, `key=value` per line, applied last (PLY-006, PLY-017).
    pub raw_mpv_options: String,
    /// Last loaded or saved preset, `mpv/<name>.conf` (PLY-016).
    pub mpv_preset: String,
    /// mpv options set from the UI or loaded from a preset (PLY-016, PLY-018).
    pub mpv_options: BTreeMap<String, String>,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `[subtitles]` (SUB-008, SUB-010, SUB-014).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitlesConfig {
    /// Languages of the subtitles chosen automatically, in order (SUB-008).
    pub preferred_languages: Vec<String>,
    /// Font size in pixels for a video 1080 px high (SUB-010).
    pub font_size: f64,
    /// Use the colours, fonts and positions of ASS files (SUB-010).
    pub respect_ass_styles: bool,
    /// `"none"`, `"blur"` or `"hidden"` (SUB-014).
    pub obscure: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for SubtitlesConfig {
    fn default() -> Self {
        Self {
            preferred_languages: vec!["ja".into(), "jpn".into()],
            font_size: 36.0,
            respect_ass_styles: false,
            obscure: "none".into(),
            other: toml::Table::new(),
        }
    }
}

/// `[lookup]` (LKP-035, POP-003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LookupConfig {
    /// Headword groups in a popup (LKP-035), 1–50.
    pub max_results: usize,
    /// Look words up on hover without holding Shift (POP-003).
    pub hover_without_modifier: bool,
    /// Close a popup opened on hover, and resume, when the pointer leaves
    /// the subtitles and the popup (POP-003).
    pub resume_on_leave: bool,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for LookupConfig {
    fn default() -> Self {
        Self {
            max_results: 10,
            hover_without_modifier: false,
            resume_on_leave: true,
            other: toml::Table::new(),
        }
    }
}

/// `[popup]` (POP-006): the largest dictionary popup, in points. A popup is
/// this wide and as tall as its content up to this height; the window can make
/// it smaller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PopupConfig {
    /// Width, within [`PopupConfig::WIDTHS`].
    pub max_width: f64,
    /// Height, within [`PopupConfig::HEIGHTS`].
    pub max_height: f64,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl PopupConfig {
    /// The widths a popup can have: narrower, a glossary line holds a few
    /// words; no screen is wider.
    pub const WIDTHS: RangeInclusive<f64> = 240.0..=4096.0;
    /// The heights a popup can have: lower, it shows less than a headword
    /// and its first line.
    pub const HEIGHTS: RangeInclusive<f64> = 120.0..=4096.0;

    /// A size brought within [`Self::WIDTHS`] and [`Self::HEIGHTS`]; a
    /// value that is not a number takes the default.
    pub fn bounded(width: f64, height: f64) -> (f64, f64) {
        let defaults = Self::default();
        let fit = |value: f64, range: &RangeInclusive<f64>, default: f64| {
            if value.is_finite() {
                value.clamp(*range.start(), *range.end())
            } else {
                default
            }
        };
        (
            fit(width, &Self::WIDTHS, defaults.max_width),
            fit(height, &Self::HEIGHTS, defaults.max_height),
        )
    }
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            max_width: 560.0,
            max_height: 500.0,
            other: toml::Table::new(),
        }
    }
}

/// `[paths]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    /// The ffmpeg executable, or its folder; empty = `NIKOMIRU_FFMPEG`, then
    /// `PATH` (EXT-001). ffprobe is looked for next to it.
    pub ffmpeg: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for General {
    fn default() -> Self {
        Self {
            ui_language: "system".into(),
            theme: "system".into(),
            study_language: "ja".into(),
            other: toml::Table::new(),
        }
    }
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            hwdec: "auto-safe".into(),
            quality_profile: "default".into(),
            seek_seconds: 3,
            auto_next: true,
            raw_mpv_options: String::new(),
            mpv_preset: String::new(),
            mpv_options: BTreeMap::new(),
            other: toml::Table::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            player: PlayerConfig::default(),
            subtitles: SubtitlesConfig::default(),
            lookup: LookupConfig::default(),
            popup: PopupConfig::default(),
            paths: PathsConfig::default(),
            shortcuts: BTreeMap::new(),
            other: toml::Table::new(),
        }
    }
}

impl Config {
    /// Parses a configuration and brings out-of-range values back into range.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        let mut c: Config = toml::from_str(text)?;
        c.player.seek_seconds = c.player.seek_seconds.clamp(1, 30);
        c.lookup.max_results = c.lookup.max_results.clamp(1, 50);
        if !(8.0..=200.0).contains(&c.subtitles.font_size) {
            c.subtitles.font_size = 36.0;
        }
        (c.popup.max_width, c.popup.max_height) =
            PopupConfig::bounded(c.popup.max_width, c.popup.max_height);
        Ok(c)
    }

    /// Reads `path`; a missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text).map_err(|e| ConfigError::Parse {
                path: path.to_path_buf(),
                line: e.span().map_or(0, |s| {
                    text[..s.start.min(text.len())].lines().count().max(1)
                }),
                message: mask_quoted(e.message().trim()),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Writes `path` atomically with permissions 0600.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        crate::fs::write_atomic(path, text.as_bytes(), 0o600).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Dotted names of the keys this version does not know (for a warning).
    pub fn unknown_keys(&self) -> Vec<String> {
        let mut out: Vec<String> = self.other.keys().cloned().collect();
        out.extend(self.general.other.keys().map(|k| format!("general.{k}")));
        out.extend(self.player.other.keys().map(|k| format!("player.{k}")));
        out.extend(self.lookup.other.keys().map(|k| format!("lookup.{k}")));
        out.extend(self.popup.other.keys().map(|k| format!("popup.{k}")));
        out.extend(self.paths.other.keys().map(|k| format!("paths.{k}")));
        out.extend(
            self.subtitles
                .other
                .keys()
                .map(|k| format!("subtitles.{k}")),
        );
        out.sort();
        out
    }
}

impl crate::Localized for ConfigError {
    fn message_id(&self) -> &'static str {
        match self {
            ConfigError::Read { .. } => "error-config-read",
            ConfigError::Write { .. } => "error-config-write",
            ConfigError::Parse { .. } => "error-config",
            ConfigError::Serialize(_) => "error-config-serialize",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            ConfigError::Read { path, source } | ConfigError::Write { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            ConfigError::Parse {
                path,
                line,
                message,
            } => vec![
                ("path", path.display().to_string()),
                ("line", line.to_string()),
                ("reason", message.clone()),
            ],
            ConfigError::Serialize(e) => vec![("reason", e.to_string())],
        }
    }
}

/// Replaces what serde quotes in its messages (`string "value"`, `` `value` ``)
/// with `…`: the value may be a secret typed into the wrong field.
fn mask_quoted(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut open: Option<char> = None;
    for c in message.chars() {
        match open {
            Some(q) if c == q => {
                out.push('…');
                out.push(c);
                open = None;
            }
            Some(_) => {}
            None => {
                out.push(c);
                if c == '"' || c == '`' {
                    open = Some(c);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.player.hwdec, "auto-safe");
        assert_eq!(c.player.seek_seconds, 3);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let text = r#"
future_section = { a = 1 }

[general]
theme = "dark"
new_flag = true

[player]
seek_seconds = 5
mpv_options = { deband = "yes" }
later = "x"

[anki]
deck = "Lapis"

[shortcuts]
play_pause = ["Space"]
"#;
        let c = Config::from_toml(text).unwrap();
        assert_eq!(c.general.theme, "dark");
        assert_eq!(c.player.seek_seconds, 5);
        assert_eq!(c.player.mpv_options["deband"], "yes");
        assert_eq!(c.shortcuts["play_pause"], ["Space"]);
        assert_eq!(
            c.unknown_keys(),
            ["anki", "future_section", "general.new_flag", "player.later"]
        );
        let again = Config::from_toml(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn lookup_settings_have_defaults_and_limits() {
        let c = Config::from_toml("[lookup]\nmax_results = 500\nhover_without_modifier = true")
            .unwrap();
        assert_eq!(c.lookup.max_results, 50);
        assert!(c.lookup.hover_without_modifier && c.lookup.resume_on_leave);
        assert_eq!(Config::default().lookup.max_results, 10);
    }

    // T-POP-006
    #[test]
    fn popup_sizes_have_defaults_and_bounds() {
        let size = |text: &str| {
            let c = Config::from_toml(text).unwrap();
            (c.popup.max_width, c.popup.max_height)
        };
        assert_eq!(size(""), (560.0, 500.0));
        // whole numbers, as people write them
        assert_eq!(
            size("[popup]\nmax_width = 700\nmax_height = 320"),
            (700.0, 320.0)
        );
        assert_eq!(
            size("[popup]\nmax_width = 10\nmax_height = 100000.5"),
            (240.0, 4096.0)
        );
        // TOML has floats that are not numbers
        assert_eq!(
            size("[popup]\nmax_width = nan\nmax_height = -inf"),
            (560.0, 500.0)
        );
        assert_eq!(PopupConfig::bounded(f64::INFINITY, 0.0), (560.0, 120.0));
        let c =
            Config::from_toml("[popup]\nmax_width = 600\nlater = 1\n[lookup]\nlater = 2").unwrap();
        assert_eq!(c.unknown_keys(), ["lookup.later", "popup.later"]);
        let again = Config::from_toml(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn seek_step_is_clamped() {
        assert_eq!(
            Config::from_toml("[player]\nseek_seconds = 0")
                .unwrap()
                .player
                .seek_seconds,
            1
        );
        assert_eq!(
            Config::from_toml("[player]\nseek_seconds = 99")
                .unwrap()
                .player
                .seek_seconds,
            30
        );
    }

    #[test]
    fn invalid_file_is_a_parse_error_with_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[anki]\napi_key = \"secret\"\n[player]\nseek_seconds = \"three\"\n",
        )
        .unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { line: 4, .. }), "{err}");
        let text = err.to_string();
        assert!(text.contains("config.toml"), "{text}");
        assert!(
            !text.contains("secret") && !text.contains("three"),
            "{text}"
        );
    }

    // T-SEC-008
    #[test]
    fn saved_file_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nikomiru/config.toml");
        Config::default().save(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        use serde::ser::Error as _;
        let io = || std::io::Error::other("x");
        let path = || PathBuf::from("/c/config.toml");
        let errors = [
            ConfigError::Read {
                path: path(),
                source: io(),
            },
            ConfigError::Write {
                path: path(),
                source: io(),
            },
            ConfigError::Parse {
                path: path(),
                line: 3,
                message: "x".into(),
            },
            ConfigError::Serialize(toml::ser::Error::custom("x")),
        ];
        let mut all: Vec<&dyn crate::Localized> = errors.iter().map(|e| e as _).collect();
        all.push(&crate::PathsError::NoHome);
        let problems = crate::error::message_problems(crate::error::I18N_DIR.as_ref(), &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
