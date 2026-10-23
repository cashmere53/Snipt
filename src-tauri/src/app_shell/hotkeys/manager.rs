//! `HotkeyManager`: registers the three capture hotkeys, keeps their status, rolls back failed
//! changes, and resolves key presses to capture modes.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;

use super::accelerator;
use super::backend::ShortcutBackend;
use crate::shared::{Accelerator, CaptureMode, HotkeySettings};

/// Why a hotkey could not be registered. Serialized as
/// `"conflict" | "printScreenReservedByWindows"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyFailureKind {
    /// Another app (or Windows feature) already owns the key.
    Conflict,
    /// Plain PrintScreen while Windows may reserve it for the Snipping Tool.
    PrintScreenReservedByWindows,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub mode: CaptureMode,
    pub accelerator: Accelerator,
    pub registered: bool,
    pub failure: Option<HotkeyFailureKind>,
}

/// Reads whether Windows reserves PrintScreen (see
/// `platform::windows::keyboard_settings::print_screen_opens_snipping`).
pub type ReservationProbe = Box<dyn Fn() -> Option<bool> + Send + Sync>;

struct State {
    current: HotkeySettings,
    /// Per mode, in `CaptureMode::ALL` order: `None` when registered (or not attempted while
    /// suspended), `Some(kind)` when registration failed.
    failures: [Option<HotkeyFailureKind>; 3],
    registered: [bool; 3],
    suspended: bool,
}

pub struct HotkeyManager {
    backend: Box<dyn ShortcutBackend>,
    probe: ReservationProbe,
    state: Mutex<State>,
    /// Shortcut id -> mode. Kept apart from `state` because the plugin calls `mode_for` on the
    /// main thread, while `state` is held during backend calls that wait for the main thread.
    shortcuts: RwLock<HashMap<u32, CaptureMode>>,
}

fn index(mode: CaptureMode) -> usize {
    match mode {
        CaptureMode::Region => 0,
        CaptureMode::Window => 1,
        CaptureMode::Fullscreen => 2,
    }
}

impl HotkeyManager {
    /// Starts with `initial` recorded but nothing registered; call `register_initial`.
    pub fn new(
        backend: Box<dyn ShortcutBackend>,
        probe: ReservationProbe,
        initial: HotkeySettings,
    ) -> Self {
        Self {
            backend,
            probe,
            state: Mutex::new(State {
                current: initial,
                failures: [None; 3],
                registered: [false; 3],
                suspended: false,
            }),
            shortcuts: RwLock::new(HashMap::new()),
        }
    }

    /// Production probe: the Windows registry on Windows, unknown elsewhere.
    pub fn windows_probe() -> ReservationProbe {
        #[cfg(windows)]
        {
            Box::new(crate::platform::windows::keyboard_settings::print_screen_opens_snipping)
        }
        #[cfg(not(windows))]
        {
            Box::new(|| None)
        }
    }

    /// Registers all three hotkeys of `hotkeys` and returns their statuses.
    pub fn register_initial(&self, hotkeys: &HotkeySettings) -> Vec<HotkeyStatus> {
        let mut state = self.lock();
        for mode in CaptureMode::ALL {
            let old = state.current.get(mode).clone();
            if state.registered[index(mode)] {
                self.unregister_logged(&old);
                state.registered[index(mode)] = false;
            }
        }
        state.current = hotkeys.clone();
        state.suspended = false;
        for mode in CaptureMode::ALL {
            self.register_into(&mut state, mode);
        }
        self.rebuild_shortcuts(&state);
        Self::statuses_of(&state)
    }

    /// Switches to `next`. Old keys of the changed modes are all released before any new key is
    /// registered, so swapping two keys works. If any new key fails, every new key is released,
    /// the old keys are registered again, and the attempted statuses are returned.
    ///
    /// While suspended, only records `next`; `resume` registers it.
    pub fn apply(&self, next: &HotkeySettings) -> Result<(), Vec<HotkeyStatus>> {
        let mut state = self.lock();
        let changed: Vec<CaptureMode> = CaptureMode::ALL
            .into_iter()
            .filter(|&mode| state.current.get(mode) != next.get(mode))
            .collect();
        if changed.is_empty() {
            return Ok(());
        }
        if state.suspended {
            state.current = next.clone();
            for &mode in &changed {
                state.failures[index(mode)] = None;
            }
            return Ok(());
        }

        let was_registered = state.registered;
        for &mode in &changed {
            if was_registered[index(mode)] {
                self.unregister_logged(state.current.get(mode));
                state.registered[index(mode)] = false;
            }
        }

        let mut attempted = Vec::with_capacity(changed.len());
        for &mode in &changed {
            let accelerator = next.get(mode);
            let failure = self.backend.register(accelerator).err().map(|e| {
                log::warn!("hotkeys: could not register {}: {e}", accelerator.as_str());
                self.classify(accelerator)
            });
            attempted.push((mode, failure));
        }

        if attempted.iter().all(|(_, failure)| failure.is_none()) {
            state.current = next.clone();
            for &mode in &changed {
                state.registered[index(mode)] = true;
                state.failures[index(mode)] = None;
            }
            self.rebuild_shortcuts(&state);
            return Ok(());
        }

        // Roll back: release what was registered, then restore the old keys.
        for (mode, failure) in &attempted {
            if failure.is_none() {
                self.unregister_logged(next.get(*mode));
            }
        }
        for &mode in &changed {
            if was_registered[index(mode)] {
                self.register_into(&mut state, mode);
            }
        }
        self.rebuild_shortcuts(&state);

        Err(CaptureMode::ALL
            .into_iter()
            .map(|mode| match attempted.iter().find(|(m, _)| *m == mode) {
                Some((_, failure)) => HotkeyStatus {
                    mode,
                    accelerator: next.get(mode).clone(),
                    registered: false,
                    failure: *failure,
                },
                None => Self::status_of(&state, mode),
            })
            .collect())
    }

    /// Registers again only the hotkeys whose registration failed.
    pub fn retry_failed(&self) -> Vec<HotkeyStatus> {
        let mut state = self.lock();
        if !state.suspended {
            for mode in CaptureMode::ALL {
                if state.failures[index(mode)].is_some() {
                    self.register_into(&mut state, mode);
                }
            }
            self.rebuild_shortcuts(&state);
        }
        Self::statuses_of(&state)
    }

    pub fn statuses(&self) -> Vec<HotkeyStatus> {
        Self::statuses_of(&self.lock())
    }

    /// The capture mode bound to a pressed shortcut.
    pub fn mode_for(&self, shortcut: &Shortcut) -> Option<CaptureMode> {
        self.shortcuts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&shortcut.id())
            .copied()
    }

    /// Releases every hotkey so the settings window can capture key presses. Idempotent.
    pub fn suspend(&self) {
        let mut state = self.lock();
        if state.suspended {
            return;
        }
        for mode in CaptureMode::ALL {
            if state.registered[index(mode)] {
                self.unregister_logged(state.current.get(mode));
                state.registered[index(mode)] = false;
            }
        }
        state.suspended = true;
        self.rebuild_shortcuts(&state);
    }

    /// Registers the current hotkeys again after `suspend`. Idempotent.
    pub fn resume(&self) {
        let mut state = self.lock();
        if !state.suspended {
            return;
        }
        state.suspended = false;
        for mode in CaptureMode::ALL {
            self.register_into(&mut state, mode);
        }
        self.rebuild_shortcuts(&state);
    }

    /// Releases every hotkey before the app exits.
    pub fn release_all(&self) {
        let mut state = self.lock();
        if let Err(e) = self.backend.unregister_all() {
            log::warn!("hotkeys: could not release all hotkeys: {e}");
        }
        state.registered = [false; 3];
        state.suspended = true;
        self.rebuild_shortcuts(&state);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn register_into(&self, state: &mut State, mode: CaptureMode) {
        let accelerator = state.current.get(mode).clone();
        match self.backend.register(&accelerator) {
            Ok(()) => {
                state.registered[index(mode)] = true;
                state.failures[index(mode)] = None;
            }
            Err(e) => {
                log::warn!("hotkeys: could not register {}: {e}", accelerator.as_str());
                state.registered[index(mode)] = false;
                state.failures[index(mode)] = Some(self.classify(&accelerator));
            }
        }
    }

    fn unregister_logged(&self, accelerator: &Accelerator) {
        if let Err(e) = self.backend.unregister(accelerator) {
            log::warn!(
                "hotkeys: could not unregister {}: {e}",
                accelerator.as_str()
            );
        }
    }

    /// Plain PrintScreen fails as "reserved by Windows" unless the Windows setting is known to
    /// be off; everything else is a conflict.
    fn classify(&self, accelerator: &Accelerator) -> HotkeyFailureKind {
        if accelerator.as_str() == "PrintScreen" && (self.probe)() != Some(false) {
            HotkeyFailureKind::PrintScreenReservedByWindows
        } else {
            HotkeyFailureKind::Conflict
        }
    }

    fn rebuild_shortcuts(&self, state: &State) {
        let map = CaptureMode::ALL
            .into_iter()
            .filter(|&mode| state.registered[index(mode)])
            .filter_map(|mode| {
                accelerator::to_shortcut(state.current.get(mode))
                    .ok()
                    .map(|shortcut| (shortcut.id(), mode))
            })
            .collect();
        *self.shortcuts.write().unwrap_or_else(|e| e.into_inner()) = map;
    }

    fn status_of(state: &State, mode: CaptureMode) -> HotkeyStatus {
        HotkeyStatus {
            mode,
            accelerator: state.current.get(mode).clone(),
            registered: state.registered[index(mode)],
            failure: state.failures[index(mode)],
        }
    }

    fn statuses_of(state: &State) -> Vec<HotkeyStatus> {
        CaptureMode::ALL
            .into_iter()
            .map(|mode| Self::status_of(state, mode))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::super::backend::fake::FakeBackend;
    use super::*;
    use crate::shared::Settings;

    /// Lets a test keep a handle to the fake that the manager owns.
    struct Shared(Arc<FakeBackend>);

    impl ShortcutBackend for Shared {
        fn register(&self, a: &Accelerator) -> Result<(), super::super::backend::BackendError> {
            self.0.register(a)
        }
        fn unregister(&self, a: &Accelerator) -> Result<(), super::super::backend::BackendError> {
            self.0.unregister(a)
        }
        fn unregister_all(&self) -> Result<(), super::super::backend::BackendError> {
            self.0.unregister_all()
        }
    }

    fn defaults() -> HotkeySettings {
        Settings::default_with(Path::new("/pictures")).hotkeys
    }

    fn hotkeys(region: &str, window: &str, fullscreen: &str) -> HotkeySettings {
        let key = |s: &str| accelerator::parse(s).unwrap();
        HotkeySettings {
            region: key(region),
            window: key(window),
            fullscreen: key(fullscreen),
        }
    }

    fn manager_with(probe: Option<bool>) -> (HotkeyManager, Arc<FakeBackend>) {
        let fake = Arc::new(FakeBackend::default());
        let manager = HotkeyManager::new(
            Box::new(Shared(fake.clone())),
            Box::new(move || probe),
            defaults(),
        );
        (manager, fake)
    }

    fn manager() -> (HotkeyManager, Arc<FakeBackend>) {
        manager_with(Some(false))
    }

    fn sorted(keys: &[&str]) -> Vec<String> {
        let mut keys: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
        keys.sort();
        keys
    }

    fn failures(statuses: &[HotkeyStatus]) -> Vec<(CaptureMode, Option<HotkeyFailureKind>)> {
        statuses.iter().map(|s| (s.mode, s.failure)).collect()
    }

    #[test]
    fn registers_all_three_initially() {
        let (manager, fake) = manager();
        let statuses = manager.register_initial(&defaults());

        assert!(statuses.iter().all(|s| s.registered && s.failure.is_none()));
        assert_eq!(
            fake.registered(),
            sorted(&["PrintScreen", "Ctrl+PrintScreen", "Shift+PrintScreen"])
        );
    }

    #[test]
    fn initial_failure_is_kept_per_mode() {
        let (manager, fake) = manager();
        fake.fail_on("Ctrl+PrintScreen");
        let statuses = manager.register_initial(&defaults());

        assert_eq!(
            failures(&statuses),
            vec![
                (CaptureMode::Region, None),
                (CaptureMode::Window, Some(HotkeyFailureKind::Conflict)),
                (CaptureMode::Fullscreen, None),
            ]
        );
        assert_eq!(manager.statuses(), statuses);
    }

    #[test]
    fn print_screen_failure_is_classified_by_windows_setting() {
        // (registry value, expected kind)
        let cases = [
            (Some(true), HotkeyFailureKind::PrintScreenReservedByWindows),
            (None, HotkeyFailureKind::PrintScreenReservedByWindows),
            (Some(false), HotkeyFailureKind::Conflict),
        ];
        for (probe, expected) in cases {
            let (manager, fake) = manager_with(probe);
            fake.fail_on("PrintScreen");
            fake.fail_on("Ctrl+PrintScreen");
            let statuses = manager.register_initial(&defaults());
            assert_eq!(statuses[0].failure, Some(expected), "probe {probe:?}");
            // A key with a modifier is always a conflict.
            assert_eq!(statuses[1].failure, Some(HotkeyFailureKind::Conflict));
        }
    }

    #[test]
    fn apply_switches_changed_keys() {
        let (manager, fake) = manager();
        manager.register_initial(&defaults());
        let next = hotkeys("PrintScreen", "Ctrl+Shift+KeyW", "Shift+PrintScreen");

        assert_eq!(manager.apply(&next), Ok(()));
        assert_eq!(
            fake.registered(),
            sorted(&["PrintScreen", "Ctrl+Shift+KeyW", "Shift+PrintScreen"])
        );
        assert_eq!(manager.statuses()[1].accelerator, next.window);
    }

    #[test]
    fn apply_can_swap_two_keys() {
        let (manager, fake) = manager();
        manager.register_initial(&defaults());
        let swapped = hotkeys("Ctrl+PrintScreen", "PrintScreen", "Shift+PrintScreen");

        assert_eq!(manager.apply(&swapped), Ok(()));
        assert_eq!(
            fake.registered(),
            sorted(&["PrintScreen", "Ctrl+PrintScreen", "Shift+PrintScreen"])
        );
        let print_screen = accelerator::to_shortcut(&swapped.window).unwrap();
        assert_eq!(manager.mode_for(&print_screen), Some(CaptureMode::Window));
    }

    #[test]
    fn apply_rolls_back_completely_when_second_key_fails() {
        let (manager, fake) = manager();
        manager.register_initial(&defaults());
        fake.fail_on("Ctrl+Shift+KeyF");
        let next = hotkeys("PrintScreen", "Ctrl+Shift+KeyW", "Ctrl+Shift+KeyF");

        let statuses = manager.apply(&next).unwrap_err();

        assert_eq!(
            failures(&statuses),
            vec![
                (CaptureMode::Region, None),
                (CaptureMode::Window, None),
                (CaptureMode::Fullscreen, Some(HotkeyFailureKind::Conflict)),
            ]
        );
        assert_eq!(statuses[2].accelerator, next.fullscreen);
        assert!(!statuses[2].registered);
        // The old keys are back and the manager still reports them.
        assert_eq!(
            fake.registered(),
            sorted(&["PrintScreen", "Ctrl+PrintScreen", "Shift+PrintScreen"])
        );
        let after = manager.statuses();
        assert!(after.iter().all(|s| s.registered && s.failure.is_none()));
        let accelerators: Vec<&str> = after.iter().map(|s| s.accelerator.as_str()).collect();
        assert_eq!(
            accelerators,
            ["PrintScreen", "Ctrl+PrintScreen", "Shift+PrintScreen"]
        );
    }

    #[test]
    fn retry_registers_only_failed_keys() {
        let (manager, fake) = manager_with(Some(true));
        fake.fail_on("PrintScreen");
        manager.register_initial(&defaults());
        fake.release_elsewhere("PrintScreen");

        let statuses = manager.retry_failed();

        assert!(statuses.iter().all(|s| s.registered && s.failure.is_none()));
        assert_eq!(fake.registered().len(), 3);
    }

    #[test]
    fn suspend_resume_and_release() {
        let (manager, fake) = manager();
        manager.register_initial(&defaults());

        manager.suspend();
        manager.suspend();
        assert!(fake.registered().is_empty());
        let print_screen = accelerator::to_shortcut(&defaults().region).unwrap();
        assert_eq!(manager.mode_for(&print_screen), None);

        manager.resume();
        manager.resume();
        assert_eq!(fake.registered().len(), 3);
        assert!(manager.statuses().iter().all(|s| s.registered));

        manager.release_all();
        assert!(fake.registered().is_empty());
        assert!(manager.statuses().iter().all(|s| !s.registered));
    }

    #[test]
    fn apply_while_suspended_takes_effect_on_resume() {
        let (manager, fake) = manager();
        manager.register_initial(&defaults());
        manager.suspend();
        let next = hotkeys("Ctrl+Shift+KeyR", "Ctrl+PrintScreen", "Shift+PrintScreen");

        assert_eq!(manager.apply(&next), Ok(()));
        assert!(fake.registered().is_empty());
        manager.resume();
        assert_eq!(
            fake.registered(),
            sorted(&["Ctrl+Shift+KeyR", "Ctrl+PrintScreen", "Shift+PrintScreen"])
        );
    }

    #[test]
    fn shortcuts_resolve_to_their_modes() {
        let (manager, _) = manager();
        let keys = defaults();
        manager.register_initial(&keys);
        for mode in CaptureMode::ALL {
            let shortcut = accelerator::to_shortcut(keys.get(mode)).unwrap();
            assert_eq!(manager.mode_for(&shortcut), Some(mode));
        }
        let unrelated =
            accelerator::to_shortcut(&accelerator::parse("Ctrl+KeyZ").unwrap()).unwrap();
        assert_eq!(manager.mode_for(&unrelated), None);
    }

    #[test]
    fn status_serializes_for_the_frontend() {
        let status = HotkeyStatus {
            mode: CaptureMode::Region,
            accelerator: accelerator::parse("PrintScreen").unwrap(),
            registered: false,
            failure: Some(HotkeyFailureKind::PrintScreenReservedByWindows),
        };
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({
                "mode": "region",
                "accelerator": "PrintScreen",
                "registered": false,
                "failure": "printScreenReservedByWindows"
            })
        );
    }
}
