//! `SettingsService`: commits a settings draft from the settings window all-or-nothing.
//!
//! Order: validate hotkeys -> check the save folder -> apply hotkeys -> sync autostart ->
//! persist and replace the stored value (subscribers run) -> emit `settings://changed`.
//! A failure undoes the hotkeys and autostart applied so far, so the persisted value always
//! equals the applied one.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use super::autostart::AutostartSync;
use super::hotkeys::accelerator::InvalidAccelerator;
use super::hotkeys::{HotkeyManager, HotkeyStatus};
use super::settings::validate::{self, HotkeyError, SaveDirNotWritable};
use super::settings::SettingsStore;
use crate::shared::{Accelerator, CaptureMode, Settings};

/// Tauri event sent to every window after a commit. Payload: `Settings`.
pub const SETTINGS_CHANGED_EVENT: &str = "settings://changed";

/// Why `settings_update` was rejected. Serialized with a `kind` tag, matching
/// `SettingsUpdateError` in `src/shared/ipc/app-shell.ts`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SettingsUpdateError {
    DuplicateHotkey {
        accelerator: Accelerator,
        modes: Vec<CaptureMode>,
    },
    InvalidHotkey {
        mode: CaptureMode,
        reason: InvalidAccelerator,
    },
    SaveDirNotWritable {
        path: PathBuf,
        message: String,
    },
    HotkeyRegistrationFailed {
        statuses: Vec<HotkeyStatus>,
    },
    AutostartFailed {
        message: String,
    },
    PersistFailed {
        message: String,
    },
}

impl From<HotkeyError> for SettingsUpdateError {
    fn from(e: HotkeyError) -> Self {
        match e {
            HotkeyError::Invalid { mode, reason } => Self::InvalidHotkey { mode, reason },
            HotkeyError::Duplicate { accelerator, modes } => {
                Self::DuplicateHotkey { accelerator, modes }
            }
        }
    }
}

impl From<SaveDirNotWritable> for SettingsUpdateError {
    fn from(e: SaveDirNotWritable) -> Self {
        Self::SaveDirNotWritable {
            path: e.path,
            message: e.message,
        }
    }
}

type Emit = Box<dyn Fn(&Settings) + Send + Sync>;

pub struct SettingsService {
    store: Arc<SettingsStore>,
    hotkeys: Arc<HotkeyManager>,
    autostart: Arc<AutostartSync>,
    /// Sends `settings://changed` to every window.
    emit: Emit,
    /// One commit at a time, so rollbacks never interleave.
    update_lock: Mutex<()>,
}

impl SettingsService {
    pub fn new(
        store: Arc<SettingsStore>,
        hotkeys: Arc<HotkeyManager>,
        autostart: Arc<AutostartSync>,
        emit: Emit,
    ) -> Self {
        Self {
            store,
            hotkeys,
            autostart,
            emit,
            update_lock: Mutex::new(()),
        }
    }

    pub fn get(&self) -> Arc<Settings> {
        self.store.get()
    }

    /// Checks that `path` can be used as the save folder (creating it if missing).
    pub fn check_save_dir(path: &Path) -> Result<(), SettingsUpdateError> {
        validate::check_save_dir(path)?;
        Ok(())
    }

    /// Commits `next` and returns the committed value (hotkeys normalized, save folder
    /// absolute). On error nothing has changed.
    pub fn update(&self, mut next: Settings) -> Result<Settings, SettingsUpdateError> {
        let _guard = self.update_lock.lock().unwrap_or_else(|e| e.into_inner());
        next.hotkeys = validate::validate_hotkeys(&next.hotkeys)?;
        next.save_dir = validate::check_save_dir(&next.save_dir)?;

        let prev = self.store.get();
        self.hotkeys
            .apply(&next.hotkeys)
            .map_err(|statuses| SettingsUpdateError::HotkeyRegistrationFailed { statuses })?;

        let autostart_changed = next.autostart != prev.autostart;
        if autostart_changed {
            if let Err(message) = self.autostart.apply(next.autostart) {
                self.restore_hotkeys(&prev);
                return Err(SettingsUpdateError::AutostartFailed { message });
            }
        }

        if let Err(e) = self.store.commit(next.clone()) {
            self.restore_hotkeys(&prev);
            if autostart_changed {
                if let Err(e) = self.autostart.apply(prev.autostart) {
                    log::error!("settings: could not restore autostart: {e}");
                }
            }
            return Err(SettingsUpdateError::PersistFailed { message: e.message });
        }

        (self.emit)(&next);
        Ok(next)
    }

    fn restore_hotkeys(&self, prev: &Settings) {
        if let Err(statuses) = self.hotkeys.apply(&prev.hotkeys) {
            log::error!("settings: could not restore hotkeys: {statuses:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;

    use super::*;
    use crate::app_shell::autostart::AutostartBackend;
    use crate::app_shell::hotkeys::accelerator;
    use crate::app_shell::hotkeys::backend::fake::FakeBackend;

    #[derive(Default)]
    struct FakeAutostart {
        enabled: Mutex<bool>,
        fail: Mutex<bool>,
    }

    impl AutostartBackend for Arc<FakeAutostart> {
        fn is_enabled(&self) -> Result<bool, String> {
            Ok(*self.enabled.lock().unwrap())
        }
        fn enable(&self) -> Result<(), String> {
            self.set(true)
        }
        fn disable(&self) -> Result<(), String> {
            self.set(false)
        }
    }

    impl FakeAutostart {
        fn set(&self, value: bool) -> Result<(), String> {
            if *self.fail.lock().unwrap() {
                return Err("access denied".to_owned());
            }
            *self.enabled.lock().unwrap() = value;
            Ok(())
        }
    }

    struct Fixture {
        root: tempfile::TempDir,
        service: SettingsService,
        store: Arc<SettingsStore>,
        keys: Arc<FakeBackend>,
        autostart: Arc<FakeAutostart>,
        notified: Arc<Mutex<Vec<Settings>>>,
        emitted: Arc<Mutex<Vec<Settings>>>,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let (store, _) = SettingsStore::load(
                root.path().join("config").join("settings.json"),
                &root.path().join("Pictures"),
            );
            let store = Arc::new(store);
            let notified = Arc::new(Mutex::new(Vec::new()));
            let sink = notified.clone();
            store.subscribe(Box::new(move |s| sink.lock().unwrap().push(s.clone())));

            let keys = Arc::new(FakeBackend::default());
            let hotkeys = Arc::new(HotkeyManager::new(
                Box::new(keys.clone()),
                Box::new(|| Some(false)),
                store.get().hotkeys.clone(),
            ));
            hotkeys.register_initial(&store.get().hotkeys);

            let autostart = Arc::new(FakeAutostart::default());
            *autostart.enabled.lock().unwrap() = store.get().autostart;
            let sync = Arc::new(AutostartSync::with_os_changes(
                Box::new(autostart.clone()),
                true,
            ));

            let emitted = Arc::new(Mutex::new(Vec::new()));
            let sink = emitted.clone();
            let service = SettingsService::new(
                store.clone(),
                hotkeys,
                sync,
                Box::new(move |s| sink.lock().unwrap().push(s.clone())),
            );
            Self {
                root,
                service,
                store,
                keys,
                autostart,
                notified,
                emitted,
            }
        }

        fn draft(&self) -> Settings {
            (*self.store.get()).clone()
        }

        fn settings_file(&self) -> String {
            fs::read_to_string(self.root.path().join("config").join("settings.json")).unwrap()
        }

        fn break_persistence(&self) {
            let config = self.root.path().join("config");
            fs::remove_dir_all(&config).unwrap();
            fs::write(&config, b"not a folder").unwrap();
        }

        fn assert_unchanged(&self, before: &Settings) {
            assert_eq!(*self.store.get(), *before);
            assert!(self.notified.lock().unwrap().is_empty());
            assert!(self.emitted.lock().unwrap().is_empty());
            let mut keys = vec![
                before.hotkeys.region.as_str().to_owned(),
                before.hotkeys.window.as_str().to_owned(),
                before.hotkeys.fullscreen.as_str().to_owned(),
            ];
            keys.sort();
            assert_eq!(self.keys.registered(), keys);
            assert_eq!(*self.autostart.enabled.lock().unwrap(), before.autostart);
        }
    }

    fn key(s: &str) -> Accelerator {
        accelerator::parse(s).unwrap()
    }

    #[test]
    fn success_applies_persists_notifies_once_and_emits() {
        let fx = Fixture::new();
        let mut next = fx.draft();
        next.hotkeys.window = key("Ctrl+Shift+KeyW");
        next.autostart = false;
        next.save_dir = fx.root.path().join("Shots");

        let committed = fx.service.update(next.clone()).unwrap();

        assert_eq!(committed, next);
        assert_eq!(*fx.store.get(), next);
        assert_eq!(*fx.notified.lock().unwrap(), vec![next.clone()]);
        assert_eq!(*fx.emitted.lock().unwrap(), vec![next.clone()]);
        assert!(fx.settings_file().contains("Ctrl+Shift+KeyW"));
        assert!(fx.keys.registered().contains(&"Ctrl+Shift+KeyW".to_owned()));
        assert!(!*fx.autostart.enabled.lock().unwrap());
        assert!(next.save_dir.is_dir());
    }

    #[test]
    fn committed_value_is_normalized() {
        let fx = Fixture::new();
        let mut next = fx.draft();
        next.hotkeys.region = Accelerator::from_normalized("shift+ctrl+KeyR".to_owned());

        let committed = fx.service.update(next).unwrap();

        assert_eq!(committed.hotkeys.region.as_str(), "Ctrl+Shift+KeyR");
    }

    #[test]
    fn duplicate_hotkeys_change_nothing() {
        let fx = Fixture::new();
        let before = fx.draft();
        let mut next = fx.draft();
        next.hotkeys.window = next.hotkeys.region.clone();

        let err = fx.service.update(next).unwrap_err();

        assert!(matches!(err, SettingsUpdateError::DuplicateHotkey { .. }));
        fx.assert_unchanged(&before);
    }

    #[test]
    fn unwritable_save_dir_changes_nothing() {
        let fx = Fixture::new();
        let before = fx.draft();
        let file = fx.root.path().join("file");
        fs::write(&file, b"x").unwrap();
        let mut next = fx.draft();
        next.save_dir = file.join("Snipt");

        let err = fx.service.update(next).unwrap_err();

        assert!(matches!(
            err,
            SettingsUpdateError::SaveDirNotWritable { .. }
        ));
        fx.assert_unchanged(&before);
    }

    #[test]
    fn hotkey_registration_failure_keeps_old_keys() {
        let fx = Fixture::new();
        let before = fx.draft();
        fx.keys.fail_on("Ctrl+Shift+KeyW");
        let mut next = fx.draft();
        next.hotkeys.window = key("Ctrl+Shift+KeyW");
        next.autostart = false;

        let err = fx.service.update(next).unwrap_err();

        assert!(matches!(
            err,
            SettingsUpdateError::HotkeyRegistrationFailed { .. }
        ));
        fx.assert_unchanged(&before);
    }

    #[test]
    fn autostart_failure_restores_hotkeys() {
        let fx = Fixture::new();
        let before = fx.draft();
        *fx.autostart.fail.lock().unwrap() = true;
        let mut next = fx.draft();
        next.hotkeys.window = key("Ctrl+Shift+KeyW");
        next.autostart = false;

        let err = fx.service.update(next).unwrap_err();

        assert_eq!(
            err,
            SettingsUpdateError::AutostartFailed {
                message: "access denied".to_owned()
            }
        );
        fx.assert_unchanged(&before);
    }

    #[test]
    fn persist_failure_restores_hotkeys_and_autostart() {
        let fx = Fixture::new();
        let before = fx.draft();
        fx.break_persistence();
        let mut next = fx.draft();
        next.hotkeys.window = key("Ctrl+Shift+KeyW");
        next.autostart = false;

        let err = fx.service.update(next).unwrap_err();

        assert!(matches!(err, SettingsUpdateError::PersistFailed { .. }));
        fx.assert_unchanged(&before);
    }

    #[test]
    fn errors_serialize_like_the_ipc_contract() {
        use serde_json::json;

        let cases = [
            (
                SettingsUpdateError::DuplicateHotkey {
                    accelerator: key("PrintScreen"),
                    modes: vec![CaptureMode::Region, CaptureMode::Window],
                },
                json!({ "kind": "duplicateHotkey", "accelerator": "PrintScreen", "modes": ["region", "window"] }),
            ),
            (
                SettingsUpdateError::InvalidHotkey {
                    mode: CaptureMode::Fullscreen,
                    reason: InvalidAccelerator::ModifierRequired,
                },
                json!({ "kind": "invalidHotkey", "mode": "fullscreen", "reason": "modifierRequired" }),
            ),
            (
                SettingsUpdateError::SaveDirNotWritable {
                    path: PathBuf::from("/x"),
                    message: "denied".to_owned(),
                },
                json!({ "kind": "saveDirNotWritable", "path": "/x", "message": "denied" }),
            ),
            (
                SettingsUpdateError::HotkeyRegistrationFailed { statuses: vec![] },
                json!({ "kind": "hotkeyRegistrationFailed", "statuses": [] }),
            ),
            (
                SettingsUpdateError::AutostartFailed {
                    message: "m".to_owned(),
                },
                json!({ "kind": "autostartFailed", "message": "m" }),
            ),
            (
                SettingsUpdateError::PersistFailed {
                    message: "m".to_owned(),
                },
                json!({ "kind": "persistFailed", "message": "m" }),
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(serde_json::to_value(&error).unwrap(), expected);
        }
    }
}
