//! annotation-editor: the editor windows, their commands, and the annotations' save envelope.
//! The annotation schema itself lives in the WebView (`src/features/annotation-editor`).

pub mod commands;
pub mod envelope;
pub mod error;
pub mod export;
pub mod label;
pub mod ports;
pub mod session;

use tauri::{App, Runtime};

pub fn setup<R: Runtime>(_app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: annotation-editor");
    Ok(())
}
