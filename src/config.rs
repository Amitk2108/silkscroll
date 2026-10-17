//! User settings, stored as JSON in `%APPDATA%\SilkScroll\config.json`.
//!
//! The engine and the settings window are separate processes; this file is the
//! only thing they share. The settings window writes it, the engine notices
//! the new modification time and reloads.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::easing::Easing;

/// How one scroll profile feels. The defaults apply to every app; individual
/// apps can carry a full profile of their own.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub enabled: bool,
    /// Distance of one wheel notch. 100 px is the distance of a plain notch
    /// in a browser; the default of 120 scrolls a little further.
    pub step_size_px: f64,
    /// How long one notch keeps moving.
    pub animation_time_ms: f64,
    /// Notches arriving closer together than this speed the scroll up.
    pub acceleration_delta_ms: f64,
    /// Upper bound for that speed-up, as a multiple of the step size.
    pub acceleration_max: f64,
    pub easing: Easing,
    /// Pulse only: how much longer the glide is than the push.
    pub tail_to_head_ratio: f64,
    pub horizontal_smoothing: bool,
    /// Shift + wheel scrolls sideways, even in apps that don't support it.
    pub shift_for_horizontal: bool,
    pub reverse_direction: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            enabled: true,
            step_size_px: 120.0,
            animation_time_ms: 360.0,
            acceleration_delta_ms: 70.0,
            acceleration_max: 3.0,
            easing: Easing::Pulse,
            tail_to_head_ratio: 3.0,
            horizontal_smoothing: true,
            shift_for_horizontal: false,
            reverse_direction: false,
        }
    }
}

pub const LIMITS_STEP_SIZE: (f64, f64) = (10.0, 1000.0);
pub const LIMITS_ANIMATION_TIME: (f64, f64) = (50.0, 2000.0);
pub const LIMITS_ACCELERATION_DELTA: (f64, f64) = (0.0, 500.0);
pub const LIMITS_ACCELERATION_MAX: (f64, f64) = (1.0, 20.0);
pub const LIMITS_TAIL_RATIO: (f64, f64) = (1.0, 20.0);

impl Profile {
    /// Clamps every value into a range the engine can work with, so a
    /// hand-edited config file can never produce a runaway scroll.
    pub fn sanitized(mut self) -> Self {
        fn fix(v: f64, (lo, hi): (f64, f64), default: f64) -> f64 {
            if v.is_finite() { v.clamp(lo, hi) } else { default }
        }
        let d = Profile::default();
        self.step_size_px = fix(self.step_size_px, LIMITS_STEP_SIZE, d.step_size_px);
        self.animation_time_ms = fix(self.animation_time_ms, LIMITS_ANIMATION_TIME, d.animation_time_ms);
        self.acceleration_delta_ms =
            fix(self.acceleration_delta_ms, LIMITS_ACCELERATION_DELTA, d.acceleration_delta_ms);
        self.acceleration_max = fix(self.acceleration_max, LIMITS_ACCELERATION_MAX, d.acceleration_max);
        self.tail_to_head_ratio = fix(self.tail_to_head_ratio, LIMITS_TAIL_RATIO, d.tail_to_head_ratio);
        self
    }
}

/// Format version of the config file. Files written before this field existed
/// read as 0. Bump it whenever the default profile changes, and add the old
/// defaults to [`PREVIOUS_DEFAULTS`].
pub const CONFIG_VERSION: u32 = 1;

/// Default profiles shipped by earlier versions as `(step size, animation
/// time, acceleration delta, acceleration max, tail-to-head ratio)`. A config
/// whose defaults still equal one of these was never customized, so it is
/// moved to the current defaults.
const PREVIOUS_DEFAULTS: [(f64, f64, f64, f64, f64); 2] = [
    (100.0, 400.0, 50.0, 3.0, 4.0), // 0.1
    (120.0, 360.0, 70.0, 7.0, 3.0), // 0.2
];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    /// See [`CONFIG_VERSION`]. Missing in old files, which then read as 0.
    #[serde(default)]
    pub config_version: u32,
    /// Master switch.
    pub enabled: bool,
    pub start_with_windows: bool,
    pub show_tray_icon: bool,
    /// Leave touchpads and free-spinning high-resolution wheels alone; they
    /// already scroll smoothly.
    pub ignore_touchpad: bool,
    /// Don't touch scrolling inside fullscreen apps, which are mostly games
    /// that treat every wheel event as a whole notch.
    pub skip_fullscreen_apps: bool,
    pub defaults: Profile,
    /// Per-app profiles keyed by lowercase exe name, e.g. `chrome.exe`.
    pub apps: BTreeMap<String, Profile>,
    /// Where each app's exe lives, so the settings window can show its icon.
    pub app_paths: BTreeMap<String, String>,
    pub welcome_shown: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            config_version: CONFIG_VERSION,
            enabled: true,
            // On for release builds only, so `cargo run` never registers a
            // development exe to start at sign-in.
            start_with_windows: cfg!(not(debug_assertions)),
            show_tray_icon: true,
            ignore_touchpad: true,
            skip_fullscreen_apps: true,
            defaults: Profile::default(),
            apps: BTreeMap::new(),
            app_paths: BTreeMap::new(),
            welcome_shown: false,
        }
    }
}

impl Config {
    pub fn profile_for(&self, exe: &str) -> &Profile {
        self.apps.get(&exe.to_ascii_lowercase()).unwrap_or(&self.defaults)
    }

    /// Adds a profile for `exe` copied from the defaults, unless it has one.
    pub fn add_app(&mut self, exe: &str, path: Option<&str>) -> String {
        let key = exe.to_ascii_lowercase();
        let defaults = self.defaults;
        self.apps.entry(key.clone()).or_insert(defaults);
        if let Some(path) = path {
            self.app_paths.insert(key.clone(), path.to_string());
        }
        key
    }

    pub fn remove_app(&mut self, exe: &str) {
        let key = exe.to_ascii_lowercase();
        self.apps.remove(&key);
        self.app_paths.remove(&key);
    }

    /// Turns smoothing on or off for one app, creating its profile from the
    /// defaults if it doesn't have one yet.
    pub fn set_app_enabled(&mut self, exe: &str, enabled: bool) {
        let key = self.add_app(exe, None);
        self.apps.get_mut(&key).expect("just added").enabled = enabled;
    }

    pub fn sanitized(mut self) -> Self {
        self.defaults = self.defaults.sanitized();
        self.apps = self.apps.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v.sanitized())).collect();
        self.app_paths = self.app_paths.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v)).collect();
        self
    }

    /// Brings a config written by an older version up to date. Returns
    /// whether anything changed, so the file can be saved again.
    fn upgrade(&mut self) -> bool {
        if self.config_version >= CONFIG_VERSION {
            return false;
        }
        let d = &self.defaults;
        let untouched = !d.shift_for_horizontal
            && !d.reverse_direction
            && d.horizontal_smoothing
            && d.easing == Easing::Pulse
            && PREVIOUS_DEFAULTS.contains(&(
                d.step_size_px,
                d.animation_time_ms,
                d.acceleration_delta_ms,
                d.acceleration_max,
                d.tail_to_head_ratio,
            ));
        if untouched {
            self.defaults = Profile { enabled: self.defaults.enabled, ..Profile::default() };
        }
        self.config_version = CONFIG_VERSION;
        true
    }

    #[cfg(test)]
    pub fn from_json(text: &str) -> serde_json::Result<Config> {
        parse(text).map(|(cfg, _)| cfg)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("config always serializes")
    }
}

/// Parses, clamps and upgrades a config. The flag says whether an upgrade
/// happened.
fn parse(text: &str) -> serde_json::Result<(Config, bool)> {
    let mut cfg = serde_json::from_str::<Config>(text)?.sanitized();
    let upgraded = cfg.upgrade();
    Ok((cfg, upgraded))
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("SilkScroll")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

#[derive(Debug)]
pub enum ReadError {
    Missing,
    /// Not valid JSON, or caught halfway through being written.
    Invalid,
}

/// Reads the config file as it is right now, without fixing anything.
/// Reloads use this: if the user is in the middle of saving the file from an
/// editor, the running settings simply stay as they are until the next change.
pub fn read() -> Result<Config, ReadError> {
    read_file().map(|(cfg, _)| cfg)
}

fn read_file() -> Result<(Config, bool), ReadError> {
    match fs::read_to_string(config_path()) {
        Ok(text) => parse(&text).map_err(|_| ReadError::Invalid),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(ReadError::Missing),
        Err(_) => Err(ReadError::Invalid),
    }
}

/// Loads the config at startup, creating it with defaults on first run and
/// saving it back if it was upgraded from an older version. A file that still
/// can't be parsed after a few tries is moved aside to `config.bad.json`
/// instead of being lost.
pub fn load() -> Config {
    for attempt in 0..3 {
        match read_file() {
            Ok((cfg, upgraded)) => {
                if upgraded {
                    let _ = save(&cfg);
                }
                return cfg;
            }
            Err(ReadError::Missing) => break,
            Err(ReadError::Invalid) if attempt < 2 => std::thread::sleep(std::time::Duration::from_millis(150)),
            Err(ReadError::Invalid) => {
                let path = config_path();
                let _ = fs::rename(&path, path.with_file_name("config.bad.json"));
            }
        }
    }
    let cfg = Config::default();
    let _ = save(&cfg);
    cfg
}

/// Writes to a temporary file first and renames it over the old one, so the
/// engine never reads a half-written config.
pub fn save(cfg: &Config) -> io::Result<()> {
    let path = config_path();
    fs::create_dir_all(config_dir())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, cfg.to_json())?;
    fs::rename(&tmp, &path)
}

pub fn modified_time() -> Option<SystemTime> {
    fs::metadata(config_path()).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_defaults() {
        let p = Profile::default();
        assert!(p.enabled);
        assert_eq!(p.step_size_px, 120.0);
        assert_eq!(p.animation_time_ms, 360.0);
        assert_eq!(p.acceleration_delta_ms, 70.0);
        assert_eq!(p.acceleration_max, 3.0);
        assert_eq!(p.tail_to_head_ratio, 3.0);
        assert_eq!(p.easing, Easing::Pulse);
        assert!(!p.shift_for_horizontal);
        assert!(p.horizontal_smoothing);
        assert!(!p.reverse_direction);

        let cfg = Config::default();
        assert!(cfg.enabled);
        // Release builds start with Windows by default; debug builds don't,
        // so `cargo run` never registers a target\debug exe at sign-in.
        assert_eq!(cfg.start_with_windows, cfg!(not(debug_assertions)));
    }

    /// A config saved by an older version, with its untouched defaults.
    fn old_config(step: f64, time: f64, delta: f64, max: f64, ratio: f64) -> String {
        format!(
            r#"{{ "defaults": {{ "enabled": true, "step_size_px": {step}, "animation_time_ms": {time},
                "acceleration_delta_ms": {delta}, "acceleration_max": {max}, "easing": "pulse",
                "tail_to_head_ratio": {ratio}, "horizontal_smoothing": true,
                "shift_for_horizontal": false, "reverse_direction": false }},
                "apps": {{ "code.exe": {{ "step_size_px": 80 }} }} }}"#
        )
    }

    #[test]
    fn untouched_defaults_from_older_versions_are_upgraded() {
        for (version, text) in
            [("0.1", old_config(100.0, 400.0, 50.0, 3.0, 4.0)), ("0.2", old_config(120.0, 360.0, 70.0, 7.0, 3.0))]
        {
            let (cfg, upgraded) = parse(&text).unwrap();
            assert!(upgraded, "{version}");
            assert_eq!(cfg.defaults, Profile::default(), "{version}");
            assert_eq!(cfg.config_version, CONFIG_VERSION);
            // App profiles are the user's own choices and stay as they are.
            assert_eq!(cfg.profile_for("code.exe").step_size_px, 80.0);
        }
    }

    #[test]
    fn customized_defaults_are_kept() {
        let (cfg, upgraded) = parse(&old_config(150.0, 400.0, 50.0, 3.0, 4.0)).unwrap();
        assert!(upgraded, "the file is still marked as upgraded");
        assert_eq!(cfg.defaults.step_size_px, 150.0);
        assert_eq!(cfg.defaults.animation_time_ms, 400.0);
    }

    #[test]
    fn current_configs_are_never_rewritten() {
        // Someone on the current version who picks the old values on purpose
        // keeps them.
        let text = old_config(100.0, 400.0, 50.0, 3.0, 4.0).replacen(
            '{',
            &format!("{{ \"config_version\": {CONFIG_VERSION},"),
            1,
        );
        let (cfg, upgraded) = parse(&text).unwrap();
        assert!(!upgraded);
        assert_eq!(cfg.defaults.step_size_px, 100.0);
        assert_eq!(Config::default().config_version, CONFIG_VERSION);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let cfg = Config::from_json(r#"{ "enabled": false, "defaults": { "step_size_px": 150 } }"#).unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.defaults.step_size_px, 150.0);
        assert_eq!(cfg.defaults.animation_time_ms, Profile::default().animation_time_ms);
        assert!(cfg.show_tray_icon);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let cfg = Config::from_json(
            r#"{ "defaults": { "step_size_px": 99999, "animation_time_ms": -5, "acceleration_max": 0 } }"#,
        )
        .unwrap();
        assert_eq!(cfg.defaults.step_size_px, LIMITS_STEP_SIZE.1);
        assert_eq!(cfg.defaults.animation_time_ms, LIMITS_ANIMATION_TIME.0);
        assert_eq!(cfg.defaults.acceleration_max, LIMITS_ACCELERATION_MAX.0);
    }

    #[test]
    fn app_lookup_ignores_case_and_falls_back_to_defaults() {
        let mut cfg = Config::default();
        cfg.set_app_enabled("Chrome.EXE", false);
        assert!(!cfg.profile_for("chrome.exe").enabled);
        assert!(!cfg.profile_for("CHROME.exe").enabled);
        assert!(cfg.profile_for("code.exe").enabled);
    }

    #[test]
    fn hand_written_config_with_partial_app_profiles_loads() {
        let text = r#"{
          "welcome_shown": true,
          "apps": {
            "notepad.exe": { "step_size_px": 80 },
            "explorer.exe": { "enabled": false }
          },
          "app_paths": { "notepad.exe": "C:/Windows/notepad.exe" }
        }"#;
        let cfg = Config::from_json(text).unwrap();
        assert_eq!(cfg.profile_for("notepad.exe").step_size_px, 80.0);
        assert!(!cfg.profile_for("explorer.exe").enabled);
    }

    #[test]
    fn app_keys_are_normalized_when_loading() {
        let cfg = Config::from_json(r#"{ "apps": { "Notepad.exe": { "enabled": false } } }"#).unwrap();
        assert!(cfg.apps.contains_key("notepad.exe"));
        assert!(!cfg.profile_for("notepad.exe").enabled);
    }

    #[test]
    fn added_apps_start_from_the_defaults_and_can_be_removed() {
        let mut cfg = Config::default();
        cfg.defaults.step_size_px = 180.0;
        let key = cfg.add_app("Code.exe", Some(r"C:\Apps\Code.exe"));
        assert_eq!(key, "code.exe");
        assert_eq!(cfg.profile_for("code.exe").step_size_px, 180.0);
        assert_eq!(cfg.app_paths["code.exe"], r"C:\Apps\Code.exe");

        // Adding again keeps the app's own tweaks.
        cfg.apps.get_mut("code.exe").unwrap().step_size_px = 60.0;
        cfg.add_app("code.exe", None);
        assert_eq!(cfg.profile_for("code.exe").step_size_px, 60.0);

        cfg.remove_app("CODE.EXE");
        assert!(!cfg.apps.contains_key("code.exe"));
        assert!(!cfg.app_paths.contains_key("code.exe"));
        assert_eq!(cfg.profile_for("code.exe").step_size_px, 180.0);
    }

    #[test]
    fn json_round_trip_is_lossless() {
        let mut cfg = Config::default();
        cfg.defaults.easing = Easing::EaseOut;
        cfg.set_app_enabled("game.exe", false);
        assert_eq!(Config::from_json(&cfg.to_json()).unwrap(), cfg);
    }
}
