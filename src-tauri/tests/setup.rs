//! Runs app-shell's stage 1 and the downstream setups against Tauri's mock runtime.
//!
//! Stage 2 (`app_shell::start`) creates the tray and registers OS hotkeys, so it is checked by
//! a development run instead.

use std::sync::Arc;

use snipt_lib::capture_library::CaptureLibrary;
use snipt_lib::shared::{BeforeExitHooks, Notifier, SettingsStore};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Manager;

#[test]
fn stage_one_and_downstream_setup_succeed_on_mock_runtime() {
    let root = tempfile::tempdir().unwrap();
    let mut app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app");

    snipt_lib::app_shell::startup::setup_at(
        app.handle(),
        root.path().join("settings.json"),
        &root.path().join("Pictures"),
        snipt_lib::app_shell::startup::Platform::production(app.handle()),
    );
    snipt_lib::setup_downstream(&mut app).expect("downstream setup failed");

    assert!(app.try_state::<Arc<SettingsStore>>().is_some());
    assert!(app.try_state::<Arc<dyn Notifier>>().is_some());
    assert!(app.try_state::<Arc<BeforeExitHooks>>().is_some());
    let library = app.state::<Arc<CaptureLibrary>>();
    assert!(library.list().is_empty());
    assert_eq!(
        library.save_dir(),
        root.path().join("Pictures").join("Snipt")
    );
}
