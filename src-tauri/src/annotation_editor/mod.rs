//! annotation-editor

use tauri::{App, Runtime};

pub fn setup<R: Runtime>(_app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: annotation-editor");
    Ok(())
}
