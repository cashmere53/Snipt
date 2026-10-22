//! Composition root: plugin registration order, two-stage setup and command registration.
//!
//! Each spec owns a commented section below and only appends to it.

pub mod annotation_editor;
pub mod app_shell;
pub mod capture_library;
pub mod platform;
pub mod screen_capture;
pub mod shared;

use tauri::{App, Runtime};
use tauri_plugin_log::{Target, TargetKind};

/// Runs every spec's `setup` in dependency order, then app-shell's second stage.
///
/// Stage 1 (`app_shell::setup`) puts shared services into managed state so that
/// downstream specs can resolve them; stage 2 (`app_shell::start`) creates the tray
/// and registers hotkeys once every downstream trait implementation is in place.
pub fn setup_specs<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    app_shell::setup(app)?;
    capture_library::setup(app)?;
    annotation_editor::setup(app)?;
    screen_capture::setup(app)?;
    app_shell::start(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        // --- app-shell --- (single-instance must be registered first)
        .plugin(tauri_plugin_single_instance::init(|_app, argv, _cwd| {
            log::info!("second instance started: {argv:?}");
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
        );
    // --- capture-library ---
    // --- screen-capture ---
    // --- annotation-editor ---

    builder
        .setup(|app| setup_specs(app).inspect_err(|e| log::error!("startup failed: {e}")))
        .invoke_handler(tauri::generate_handler![
            // --- app-shell ---
            // --- capture-library ---
            // --- screen-capture ---
            // --- annotation-editor ---
        ])
        .run(tauri::generate_context!())
        .expect("error while running Snipt");
}
