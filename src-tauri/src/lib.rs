//! Composition root: plugin registration order, two-stage setup and command registration.
//!
//! Each spec owns a commented section below and only appends to it.

pub mod annotation_editor;
pub mod app_shell;
pub mod capture_library;
pub mod platform;
pub mod screen_capture;
pub mod shared;

use tauri::{App, AppHandle, RunEvent, Runtime};
use tauri_plugin_log::{Target, TargetKind};

/// Runs every spec's `setup` in dependency order, then app-shell's second stage.
///
/// Stage 1 (`app_shell::setup`) puts shared services into managed state so that
/// downstream specs can resolve them; stage 2 (`app_shell::start`) creates the tray
/// and registers hotkeys once every downstream trait implementation is in place.
pub fn setup_specs<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    app_shell::setup(app)?;
    setup_downstream(app)?;
    app_shell::start(app)?;
    // tauri-driver attaches to the first webview; Snipt otherwise starts with none.
    #[cfg(feature = "e2e")]
    app_shell::windows::WindowManager::new(app.handle().clone()).show_history()?;
    Ok(())
}

/// The downstream specs' `setup`, in dependency order, between app-shell's two stages.
pub fn setup_downstream<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    capture_library::setup(app)?;
    annotation_editor::setup(app)?;
    screen_capture::setup(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        // --- app-shell --- (single-instance must be registered first)
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            app_shell::on_second_instance(app, &argv);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(app_shell::on_shortcut)
                .build(),
        )
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                // %LOCALAPPDATA%\app.snipt\logs on Windows
                .targets([
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Stdout),
                ])
                .level(log::LevelFilter::Info)
                .build(),
        )
        // --- capture-library ---
        .plugin(tauri_plugin_opener::init());
    // --- screen-capture ---
    // --- annotation-editor ---

    builder
        .setup(|app| setup_specs(app).inspect_err(|e| log::error!("startup failed: {e}")))
        .invoke_handler(tauri::generate_handler![
            // --- app-shell ---
            app_shell::commands::settings_get,
            app_shell::commands::settings_update,
            app_shell::commands::settings_check_save_dir,
            app_shell::commands::hotkeys_status,
            app_shell::commands::hotkeys_retry,
            app_shell::commands::hotkeys_suspend,
            app_shell::commands::hotkeys_resume,
            // --- capture-library ---
            capture_library::commands::library_list,
            capture_library::commands::library_open_in_editor,
            capture_library::commands::library_copy,
            capture_library::commands::library_reveal,
            capture_library::commands::library_delete,
            capture_library::commands::library_prune_missing,
            // --- screen-capture ---
            screen_capture::commands::capture_overlay_ready,
            screen_capture::commands::capture_overlay_shown,
            screen_capture::commands::capture_overlay_magnifier,
            screen_capture::commands::capture_overlay_pointer,
            screen_capture::commands::capture_overlay_commit_region,
            screen_capture::commands::capture_overlay_commit_window,
            screen_capture::commands::capture_overlay_cancel,
            // --- annotation-editor ---
            annotation_editor::commands::editor_open_session,
            annotation_editor::commands::editor_read_original,
            annotation_editor::commands::editor_save,
            annotation_editor::commands::editor_copy,
            annotation_editor::commands::editor_pick_export_path,
            annotation_editor::commands::editor_write_export,
            annotation_editor::commands::editor_flush_result,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Snipt")
        .run(on_run_event);
}

fn on_run_event<R: Runtime>(_app: &AppHandle<R>, event: RunEvent) {
    // --- app-shell --- (closing the last window must not end the tray app)
    if let RunEvent::ExitRequested { code, api, .. } = &event {
        if app_shell::lifecycle::Lifecycle::should_prevent_exit(*code) {
            api.prevent_exit();
        }
    }
    // --- capture-library ---
    // --- screen-capture ---
    // --- annotation-editor ---
}
