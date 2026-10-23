//! `SettingsStore`: the single holder of the committed settings.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use super::file::{LoadOutcome, PersistError, SettingsFile};
use crate::shared::Settings;

type Subscriber = Arc<dyn Fn(&Settings) + Send + Sync + 'static>;

/// Managed as `Arc<SettingsStore>`. Downstream specs read it; only app-shell commits.
pub struct SettingsStore {
    current: RwLock<Arc<Settings>>,
    subscribers: Mutex<Vec<Subscriber>>,
    file: SettingsFile,
    /// Serializes commits so subscribers see them in commit order.
    commit_lock: Mutex<()>,
}

impl SettingsStore {
    /// A snapshot of the committed settings.
    pub fn get(&self) -> Arc<Settings> {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// `f` is called synchronously with the new settings after every commit, in registration
    /// order and without the settings lock held (so it may call `get`). Move heavy work to
    /// another thread. It must not commit.
    pub fn subscribe(&self, f: Box<dyn Fn(&Settings) + Send + Sync + 'static>) {
        self.subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::from(f));
    }

    /// Loads `settings.json` at `path`, falling back to the defaults derived from `picture_dir`.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "wired into stage-1 setup in task 9.1")
    )]
    pub(crate) fn load(path: PathBuf, picture_dir: &Path) -> (SettingsStore, LoadOutcome) {
        let file = SettingsFile::new(path, picture_dir);
        let (settings, outcome) = file.load();
        (Self::with_file(file, settings), outcome)
    }

    fn with_file(file: SettingsFile, settings: Settings) -> Self {
        Self {
            current: RwLock::new(Arc::new(settings)),
            subscribers: Mutex::new(Vec::new()),
            file,
            commit_lock: Mutex::new(()),
        }
    }

    /// Persists `next`, then replaces the current settings and notifies subscribers. On a
    /// persistence failure nothing changes and no subscriber is called.
    pub(crate) fn commit(&self, next: Settings) -> Result<(), PersistError> {
        let _guard = self.commit_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.file.save(&next)?;
        let next = Arc::new(next);
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = next.clone();

        let subscribers = self
            .subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for subscriber in subscribers {
            subscriber(&next);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::FullscreenTarget;
    use std::fs;

    fn store(root: &Path) -> SettingsStore {
        SettingsStore::load(
            root.join("config").join("settings.json"),
            &root.join("Pictures"),
        )
        .0
    }

    fn record(store: &SettingsStore) -> Arc<Mutex<Vec<Settings>>> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let sink = calls.clone();
        store.subscribe(Box::new(move |s| sink.lock().unwrap().push(s.clone())));
        calls
    }

    #[test]
    fn commit_persists_replaces_and_notifies_once() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        let calls = record(&store);

        let mut next = (*store.get()).clone();
        next.fullscreen_target = FullscreenTarget::AllMonitors;
        store.commit(next.clone()).unwrap();

        assert_eq!(*calls.lock().unwrap(), vec![next.clone()]);
        assert_eq!(*store.get(), next);
        let reloaded = SettingsStore::load(
            root.path().join("config").join("settings.json"),
            &root.path().join("Pictures"),
        );
        assert_eq!(reloaded.1, LoadOutcome::Loaded);
        assert_eq!(*reloaded.0.get(), next);
    }

    #[test]
    fn subscribers_run_in_order_and_can_read_the_new_value() {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(store(root.path()));
        let order = Arc::new(Mutex::new(Vec::new()));
        for name in ["first", "second"] {
            let order = order.clone();
            let reader = Arc::downgrade(&store);
            store.subscribe(Box::new(move |s| {
                let seen = reader.upgrade().unwrap().get();
                assert_eq!(*seen, *s);
                order.lock().unwrap().push(name);
            }));
        }

        let mut next = (*store.get()).clone();
        next.autostart = false;
        store.commit(next).unwrap();

        assert_eq!(*order.lock().unwrap(), vec!["first", "second"]);
    }

    #[test]
    fn failed_persist_keeps_value_and_skips_subscribers() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        let calls = record(&store);
        let before = store.get();

        // Replace the config folder with a file so the next save cannot write.
        let config = root.path().join("config");
        fs::remove_dir_all(&config).unwrap();
        fs::write(&config, b"not a folder").unwrap();

        let mut next = (*before).clone();
        next.autostart = false;
        assert!(store.commit(next).is_err());

        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(store.get(), before);
    }
}
