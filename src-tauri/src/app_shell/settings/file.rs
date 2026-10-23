//! `settings.json`: loading with defaults and corruption recovery, and atomic saving.
//!
//! Format: `{ "schemaVersion": 1, ...Settings }`. Missing fields take their default values.
//! Invalid JSON, wrong types, an unknown `schemaVersion`, and invalid or duplicate accelerators
//! count as corruption: the file is moved to `settings.corrupt-<unix seconds>.json` and the
//! defaults are saved in its place.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use super::validate::validate_hotkeys;
use crate::shared::Settings;

pub const SCHEMA_VERSION: u64 = 1;
const SCHEMA_VERSION_KEY: &str = "schemaVersion";

/// How the settings were obtained at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    Loaded,
    /// No settings file existed; the defaults were saved.
    FirstRun,
    /// The file was unreadable; it was moved to `backup` (if that succeeded) and the defaults
    /// were saved.
    RecoveredFromCorruption {
        backup: Option<PathBuf>,
    },
}

/// Writing `settings.json` failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistError {
    pub message: String,
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to save settings: {}", self.message)
    }
}

impl std::error::Error for PersistError {}

pub struct SettingsFile {
    path: PathBuf,
    defaults: Settings,
}

impl SettingsFile {
    /// `path` is `settings.json`; `picture_dir` decides the default save folder.
    pub fn new(path: PathBuf, picture_dir: &Path) -> Self {
        Self {
            path,
            defaults: Settings::default_with(picture_dir),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn defaults(&self) -> &Settings {
        &self.defaults
    }

    /// Never fails: on any problem it falls back to the defaults. Write failures while saving
    /// the defaults are only logged.
    pub fn load(&self) -> (Settings, LoadOutcome) {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return self.first_run(),
            Err(e) => return self.recover(&format!("read failed: {e}")),
        };
        match self.parse(&text) {
            Ok(settings) => (settings, LoadOutcome::Loaded),
            Err(reason) => self.recover(&reason),
        }
    }

    /// Writes to `settings.json.tmp` and renames it over `settings.json` (which replaces the
    /// existing file on Windows too).
    pub fn save(&self, settings: &Settings) -> Result<(), PersistError> {
        let to_err = |e: io::Error| PersistError {
            message: e.to_string(),
        };
        let mut value = serde_json::to_value(settings).map_err(|e| PersistError {
            message: e.to_string(),
        })?;
        let object = value
            .as_object_mut()
            .expect("Settings serializes to an object");
        let mut with_version = Map::new();
        with_version.insert(SCHEMA_VERSION_KEY.to_owned(), SCHEMA_VERSION.into());
        with_version.append(object);
        let bytes =
            serde_json::to_vec_pretty(&Value::Object(with_version)).map_err(|e| PersistError {
                message: e.to_string(),
            })?;

        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(to_err)?;
        }
        let tmp = self.sibling("settings.json.tmp");
        let result = (|| {
            let mut file = File::create(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result.map_err(to_err)
    }

    fn parse(&self, text: &str) -> Result<Settings, String> {
        let Value::Object(mut stored) =
            serde_json::from_str::<Value>(text).map_err(|e| format!("invalid JSON: {e}"))?
        else {
            return Err("not a JSON object".to_owned());
        };
        match stored.remove(SCHEMA_VERSION_KEY) {
            Some(Value::Number(n)) if n.as_u64() == Some(SCHEMA_VERSION) => {}
            other => return Err(format!("unsupported schemaVersion: {other:?}")),
        }

        let Value::Object(mut merged) =
            serde_json::to_value(&self.defaults).expect("Settings serializes")
        else {
            unreachable!("Settings serializes to an object");
        };
        merge_known_fields(&mut merged, stored);
        let mut settings: Settings = serde_json::from_value(Value::Object(merged))
            .map_err(|e| format!("wrong type: {e}"))?;
        settings.hotkeys =
            validate_hotkeys(&settings.hotkeys).map_err(|e| format!("invalid hotkeys: {e:?}"))?;
        Ok(settings)
    }

    fn first_run(&self) -> (Settings, LoadOutcome) {
        if let Err(e) = self.save(&self.defaults) {
            log::warn!("settings: could not save defaults on first run: {e}");
        }
        if let Err(e) = fs::create_dir_all(&self.defaults.save_dir) {
            log::warn!(
                "settings: could not create default save folder {}: {e}",
                self.defaults.save_dir.display()
            );
        }
        (self.defaults.clone(), LoadOutcome::FirstRun)
    }

    fn recover(&self, reason: &str) -> (Settings, LoadOutcome) {
        log::warn!(
            "settings: {} is corrupt ({reason}); resetting to defaults",
            self.path.display()
        );
        let backup = self.backup_path();
        let backup = match fs::rename(&self.path, &backup) {
            Ok(()) => Some(backup),
            Err(e) => {
                log::warn!("settings: could not back up corrupt file: {e}");
                None
            }
        };
        if let Err(e) = self.save(&self.defaults) {
            log::warn!("settings: could not save defaults after recovery: {e}");
        }
        (
            self.defaults.clone(),
            LoadOutcome::RecoveredFromCorruption { backup },
        )
    }

    fn backup_path(&self) -> PathBuf {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let mut candidate = self.sibling(&format!("settings.corrupt-{secs}.json"));
        let mut n = 1;
        while candidate.exists() {
            candidate = self.sibling(&format!("settings.corrupt-{secs}-{n}.json"));
            n += 1;
        }
        candidate
    }

    fn sibling(&self, name: &str) -> PathBuf {
        self.path.with_file_name(name)
    }
}

/// Overwrites the default fields with stored ones. `hotkeys` is merged per key so that a file
/// missing one hotkey keeps the other two. Unknown fields are ignored.
fn merge_known_fields(defaults: &mut Map<String, Value>, stored: Map<String, Value>) {
    for (key, value) in stored {
        match (defaults.get_mut(&key), value) {
            (Some(Value::Object(default_hotkeys)), Value::Object(stored_hotkeys))
                if key == "hotkeys" =>
            {
                for (mode, accelerator) in stored_hotkeys {
                    if default_hotkeys.contains_key(&mode) {
                        default_hotkeys.insert(mode, accelerator);
                    }
                }
            }
            (Some(slot), value) => *slot = value,
            (None, _) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{CaptureMode, FullscreenTarget};
    use serde_json::json;

    struct Fixture {
        _root: tempfile::TempDir,
        config: PathBuf,
        pictures: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let config = root.path().join("config");
            let pictures = root.path().join("Pictures");
            Self {
                _root: root,
                config,
                pictures,
            }
        }

        fn file(&self) -> SettingsFile {
            SettingsFile::new(self.config.join("settings.json"), &self.pictures)
        }

        fn write(&self, text: &str) {
            fs::create_dir_all(&self.config).unwrap();
            fs::write(self.config.join("settings.json"), text).unwrap();
        }

        fn stored(&self) -> Value {
            serde_json::from_str(&fs::read_to_string(self.config.join("settings.json")).unwrap())
                .unwrap()
        }

        fn backups(&self) -> Vec<PathBuf> {
            fs::read_dir(&self.config)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| {
                    p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("settings.corrupt-")
                })
                .collect()
        }
    }

    #[test]
    fn first_run_saves_defaults_and_creates_save_folder() {
        let fx = Fixture::new();
        let (settings, outcome) = fx.file().load();

        assert_eq!(outcome, LoadOutcome::FirstRun);
        assert_eq!(settings, Settings::default_with(&fx.pictures));
        assert_eq!(settings.save_dir, fx.pictures.join("Snipt"));
        assert_eq!(
            settings.hotkeys.get(CaptureMode::Region).as_str(),
            "PrintScreen"
        );
        assert!(settings.autostart);
        assert!(fx.pictures.join("Snipt").is_dir());
        assert_eq!(fx.stored()["schemaVersion"], json!(1));
        assert!(!fx.config.join("settings.json.tmp").exists());
    }

    #[test]
    fn save_then_load_round_trips() {
        let fx = Fixture::new();
        let file = fx.file();
        let mut settings = Settings::default_with(&fx.pictures);
        settings.fullscreen_target = FullscreenTarget::AllMonitors;
        settings.autostart = false;
        settings.save_dir = fx.pictures.join("Elsewhere");
        settings.hotkeys.region =
            crate::app_shell::hotkeys::accelerator::parse("Ctrl+Shift+KeyS").unwrap();

        file.save(&settings).unwrap();
        // Overwriting an existing file must work as well.
        file.save(&settings).unwrap();

        assert_eq!(file.load(), (settings, LoadOutcome::Loaded));
    }

    #[test]
    fn missing_fields_take_defaults() {
        let fx = Fixture::new();
        fx.write(r#"{ "schemaVersion": 1, "autostart": false, "hotkeys": { "region": "Ctrl+KeyR" }, "future": 1 }"#);
        let (settings, outcome) = fx.file().load();

        assert_eq!(outcome, LoadOutcome::Loaded);
        assert!(!settings.autostart);
        assert_eq!(settings.hotkeys.region.as_str(), "Ctrl+KeyR");
        assert_eq!(settings.hotkeys.window.as_str(), "Ctrl+PrintScreen");
        assert_eq!(settings.save_dir, fx.pictures.join("Snipt"));
    }

    #[test]
    fn stored_accelerators_are_normalized() {
        let fx = Fixture::new();
        fx.write(r#"{ "schemaVersion": 1, "hotkeys": { "region": "shift+ctrl+KeyR" } }"#);
        let (settings, _) = fx.file().load();
        assert_eq!(settings.hotkeys.region.as_str(), "Ctrl+Shift+KeyR");
    }

    fn assert_recovers(text: &str) {
        let fx = Fixture::new();
        fx.write(text);
        let (settings, outcome) = fx.file().load();

        assert_eq!(settings, Settings::default_with(&fx.pictures));
        let backups = fx.backups();
        assert_eq!(backups.len(), 1, "one backup for {text}");
        assert_eq!(
            outcome,
            LoadOutcome::RecoveredFromCorruption {
                backup: Some(backups[0].clone())
            }
        );
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), text);
        assert_eq!(fx.stored()["hotkeys"]["region"], json!("PrintScreen"));
    }

    #[test]
    fn invalid_json_is_recovered() {
        assert_recovers("{ not json");
    }

    #[test]
    fn duplicate_hotkeys_are_recovered() {
        assert_recovers(
            r#"{ "schemaVersion": 1, "hotkeys": { "region": "PrintScreen", "window": "PrintScreen" } }"#,
        );
    }

    #[test]
    fn invalid_hotkey_is_recovered() {
        assert_recovers(r#"{ "schemaVersion": 1, "hotkeys": { "window": "KeyA" } }"#);
    }

    #[test]
    fn wrong_type_is_recovered() {
        assert_recovers(r#"{ "schemaVersion": 1, "autostart": "yes" }"#);
    }

    #[test]
    fn unknown_or_missing_schema_version_is_recovered() {
        assert_recovers(r#"{ "schemaVersion": 2 }"#);
        assert_recovers(r#"{ "autostart": true }"#);
        assert_recovers("[]");
    }

    #[test]
    fn repeated_recovery_keeps_every_backup() {
        let fx = Fixture::new();
        fx.write("broken 1");
        fx.file().load();
        fx.write("broken 2");
        fx.file().load();
        assert_eq!(fx.backups().len(), 2);
    }

    #[test]
    fn save_reports_failure_when_folder_cannot_be_created() {
        let fx = Fixture::new();
        fs::create_dir_all(fx.config.parent().unwrap()).unwrap();
        fs::write(&fx.config, b"a file, not a folder").unwrap();
        let err = fx
            .file()
            .save(&Settings::default_with(&fx.pictures))
            .unwrap_err();
        assert!(!err.message.is_empty());
    }
}
