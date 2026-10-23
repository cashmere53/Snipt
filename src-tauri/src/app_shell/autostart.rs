//! `AutostartSync`: keeps the OS "start at login" entry in line with the settings.

use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// The OS side of autostart (the autostart plugin in production, a fake in tests).
pub trait AutostartBackend: Send + Sync + 'static {
    fn is_enabled(&self) -> Result<bool, String>;
    fn enable(&self) -> Result<(), String>;
    fn disable(&self) -> Result<(), String>;
}

pub struct PluginAutostart<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> PluginAutostart<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> AutostartBackend for PluginAutostart<R> {
    fn is_enabled(&self) -> Result<bool, String> {
        self.app
            .autolaunch()
            .is_enabled()
            .map_err(|e| e.to_string())
    }

    fn enable(&self) -> Result<(), String> {
        self.app.autolaunch().enable().map_err(|e| e.to_string())
    }

    fn disable(&self) -> Result<(), String> {
        self.app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

pub struct AutostartSync {
    backend: Box<dyn AutostartBackend>,
    change_os: bool,
}

impl AutostartSync {
    /// Debug builds never touch the OS entry, so `pnpm tauri dev` does not register the
    /// development binary for login.
    pub fn new(backend: Box<dyn AutostartBackend>) -> Self {
        Self::with_os_changes(backend, !cfg!(debug_assertions))
    }

    fn with_os_changes(backend: Box<dyn AutostartBackend>, change_os: bool) -> Self {
        Self { backend, change_os }
    }

    /// Enables or disables the OS entry only when it differs from `enabled`.
    pub fn apply(&self, enabled: bool) -> Result<(), String> {
        if !self.change_os {
            log::info!("autostart: debug build, not changing the OS entry (wanted {enabled})");
            return Ok(());
        }
        if self.backend.is_enabled()? == enabled {
            return Ok(());
        }
        if enabled {
            self.backend.enable()
        } else {
            self.backend.disable()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct Fake {
        enabled: Mutex<bool>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl AutostartBackend for Arc<Fake> {
        fn is_enabled(&self) -> Result<bool, String> {
            Ok(*self.enabled.lock().unwrap())
        }
        fn enable(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("enable");
            *self.enabled.lock().unwrap() = true;
            Ok(())
        }
        fn disable(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("disable");
            *self.enabled.lock().unwrap() = false;
            Ok(())
        }
    }

    fn sync(os_enabled: bool) -> (AutostartSync, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        *fake.enabled.lock().unwrap() = os_enabled;
        (
            AutostartSync::with_os_changes(Box::new(fake.clone()), true),
            fake,
        )
    }

    fn calls(fake: &Fake) -> Vec<&'static str> {
        fake.calls.lock().unwrap().clone()
    }

    #[test]
    fn disables_when_wanted_off() {
        let (sync, fake) = sync(true);
        sync.apply(false).unwrap();
        assert_eq!(calls(&fake), ["disable"]);
    }

    #[test]
    fn enables_when_os_is_off() {
        let (sync, fake) = sync(false);
        sync.apply(true).unwrap();
        assert_eq!(calls(&fake), ["enable"]);
    }

    #[test]
    fn does_nothing_when_already_in_sync() {
        for state in [true, false] {
            let (sync, fake) = sync(state);
            sync.apply(state).unwrap();
            assert!(calls(&fake).is_empty());
        }
    }

    #[test]
    fn debug_mode_never_touches_the_os() {
        let fake = Arc::new(Fake::default());
        let sync = AutostartSync::with_os_changes(Box::new(fake.clone()), false);
        sync.apply(true).unwrap();
        assert!(calls(&fake).is_empty());
    }

    #[test]
    fn reports_backend_errors() {
        struct Broken;
        impl AutostartBackend for Broken {
            fn is_enabled(&self) -> Result<bool, String> {
                Err("registry unavailable".to_owned())
            }
            fn enable(&self) -> Result<(), String> {
                unreachable!()
            }
            fn disable(&self) -> Result<(), String> {
                unreachable!()
            }
        }
        let sync = AutostartSync::with_os_changes(Box::new(Broken), true);
        assert_eq!(sync.apply(true), Err("registry unavailable".to_owned()));
    }
}
