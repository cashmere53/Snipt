//! Tauri commands for the settings window. Allowed only for the `settings` window by
//! `capabilities/app-shell.json`.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Runtime, State};

use super::hotkeys::{HotkeyManager, HotkeyStatus};
use super::service::{SettingsService, SettingsUpdateError};
use super::tray;
use crate::shared::Settings;

#[tauri::command]
pub fn settings_get(service: State<'_, Arc<SettingsService>>) -> Settings {
    (*service.get()).clone()
}

#[tauri::command]
pub fn settings_update(
    service: State<'_, Arc<SettingsService>>,
    next: Settings,
) -> Result<Settings, SettingsUpdateError> {
    service.update(next)
}

#[tauri::command]
pub fn settings_check_save_dir(path: PathBuf) -> Result<(), SettingsUpdateError> {
    SettingsService::check_save_dir(&path)
}

#[tauri::command]
pub fn hotkeys_status(hotkeys: State<'_, Arc<HotkeyManager>>) -> Vec<HotkeyStatus> {
    hotkeys.statuses()
}

/// Also updates the tray tooltip, since the region hotkey may now be registered.
#[tauri::command]
pub fn hotkeys_retry<R: Runtime>(
    app: AppHandle<R>,
    hotkeys: State<'_, Arc<HotkeyManager>>,
) -> Vec<HotkeyStatus> {
    let statuses = hotkeys.retry_failed();
    tray::refresh_from_state(&app);
    statuses
}

/// While the settings window captures a key combination.
#[tauri::command]
pub fn hotkeys_suspend(hotkeys: State<'_, Arc<HotkeyManager>>) {
    hotkeys.suspend();
}

/// Resuming registers the keys again, which may change what the tooltip shows.
#[tauri::command]
pub fn hotkeys_resume<R: Runtime>(app: AppHandle<R>, hotkeys: State<'_, Arc<HotkeyManager>>) {
    hotkeys.resume();
    tray::refresh_from_state(&app);
}

/// Command names, in the order listed in `build.rs` (`AppManifest::commands`).
pub const COMMANDS: [&str; 7] = [
    "settings_get",
    "settings_update",
    "settings_check_save_dir",
    "hotkeys_status",
    "hotkeys_retry",
    "hotkeys_suspend",
    "hotkeys_resume",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_save_dir_reports_contract_error() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        std::fs::write(&file, b"x").unwrap();

        let err = settings_check_save_dir(file.join("Snipt")).unwrap_err();
        let value = serde_json::to_value(err).unwrap();

        assert_eq!(value["kind"], "saveDirNotWritable");
        assert_eq!(value["path"], serde_json::json!(file.join("Snipt")));
        assert!(value["message"].is_string());
        assert!(settings_check_save_dir(root.path().join("ok")).is_ok());
    }

    #[test]
    fn capability_allows_only_the_settings_window() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../../capabilities/app-shell.json")).unwrap();
        assert_eq!(capability["windows"], serde_json::json!(["settings"]));

        let permissions: Vec<&str> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        for command in COMMANDS {
            let permission = format!("allow-{}", command.replace('_', "-"));
            assert!(permissions.contains(&permission.as_str()), "{permission}");
        }
        assert!(
            permissions
                .iter()
                .all(|p| !p.starts_with("global-shortcut:") && !p.starts_with("autostart:")),
            "the webview must not control shortcuts or autostart directly"
        );
    }
}
