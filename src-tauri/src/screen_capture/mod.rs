//! screen-capture

use tauri::{App, Runtime};

pub fn setup<R: Runtime>(_app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: screen-capture");
    Ok(())
}
