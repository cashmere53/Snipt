//! OS shortcut registration behind a trait so `HotkeyManager` can be tested with a fake.

use std::fmt;

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use super::accelerator;
use crate::shared::Accelerator;

/// Registering or unregistering a shortcut failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError {
    pub message: String,
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BackendError {}

pub trait ShortcutBackend: Send + Sync + 'static {
    /// Fails when the accelerator is invalid or the OS refuses it (e.g. another app owns it).
    fn register(&self, accelerator: &Accelerator) -> Result<(), BackendError>;
    fn unregister(&self, accelerator: &Accelerator) -> Result<(), BackendError>;
    /// Unregisters every shortcut this app registered.
    fn unregister_all(&self) -> Result<(), BackendError>;
}

/// Production backend: the global-shortcut plugin. Key presses reach the handler given to the
/// plugin's `Builder::with_handler`.
pub struct GlobalShortcutBackend<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> GlobalShortcutBackend<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

fn backend_error(e: impl fmt::Display) -> BackendError {
    BackendError {
        message: e.to_string(),
    }
}

impl<R: Runtime> ShortcutBackend for GlobalShortcutBackend<R> {
    fn register(&self, accelerator: &Accelerator) -> Result<(), BackendError> {
        let shortcut =
            accelerator::to_shortcut(accelerator).map_err(|e| backend_error(format!("{e:?}")))?;
        self.app
            .global_shortcut()
            .register(shortcut)
            .map_err(backend_error)
    }

    fn unregister(&self, accelerator: &Accelerator) -> Result<(), BackendError> {
        let shortcut =
            accelerator::to_shortcut(accelerator).map_err(|e| backend_error(format!("{e:?}")))?;
        self.app
            .global_shortcut()
            .unregister(shortcut)
            .map_err(backend_error)
    }

    fn unregister_all(&self) -> Result<(), BackendError> {
        self.app
            .global_shortcut()
            .unregister_all()
            .map_err(backend_error)
    }
}

#[cfg(test)]
pub mod fake {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use super::*;

    /// Behaves like the OS: registering a key that is already registered (by this app or, via
    /// `fail_on`, by another app) fails.
    #[derive(Default)]
    pub struct FakeBackend {
        registered: Mutex<HashSet<String>>,
        taken_elsewhere: Mutex<HashSet<String>>,
    }

    impl FakeBackend {
        /// Makes registering `accelerator` fail as if another app owned it.
        pub fn fail_on(&self, accelerator: &str) {
            self.taken_elsewhere
                .lock()
                .unwrap()
                .insert(accelerator.to_owned());
        }

        /// Releases a key previously passed to `fail_on`.
        pub fn release_elsewhere(&self, accelerator: &str) {
            self.taken_elsewhere.lock().unwrap().remove(accelerator);
        }

        /// Registered accelerators, sorted.
        pub fn registered(&self) -> Vec<String> {
            let mut keys: Vec<String> = self.registered.lock().unwrap().iter().cloned().collect();
            keys.sort();
            keys
        }
    }

    impl ShortcutBackend for FakeBackend {
        fn register(&self, accelerator: &Accelerator) -> Result<(), BackendError> {
            let key = accelerator.as_str();
            if self.taken_elsewhere.lock().unwrap().contains(key) {
                return Err(backend_error(format!("{key} is used by another app")));
            }
            if !self.registered.lock().unwrap().insert(key.to_owned()) {
                return Err(backend_error(format!("{key} is already registered")));
            }
            Ok(())
        }

        fn unregister(&self, accelerator: &Accelerator) -> Result<(), BackendError> {
            if self.registered.lock().unwrap().remove(accelerator.as_str()) {
                Ok(())
            } else {
                Err(backend_error(format!(
                    "{} is not registered",
                    accelerator.as_str()
                )))
            }
        }

        fn unregister_all(&self) -> Result<(), BackendError> {
            self.registered.lock().unwrap().clear();
            Ok(())
        }
    }
}
