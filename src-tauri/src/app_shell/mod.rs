//! app-shell: tray, global hotkeys, settings, autostart, single instance and exit.
//!
//! app-shell must not depend on downstream modules (capture_library, screen_capture,
//! annotation_editor); downstream specs plug in through traits in `crate::shared`.

use tauri::{App, Runtime};

/// Stage 1: load settings and manage shared services before downstream setup runs.
pub fn setup<R: Runtime>(_app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: app-shell (stage 1)");
    Ok(())
}

/// Stage 2: create the tray, register hotkeys and sync autostart.
pub fn start<R: Runtime>(_app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: app-shell (stage 2)");
    Ok(())
}
