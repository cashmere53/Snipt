//! screen-capture: region, window and fullscreen captures, from the hotkey to the editor.
//!
//! The pure modules (`geometry`, `topology`, `compose`, `window_pick`) import neither Tauri nor
//! the `windows` crate; the OS is reached through `ports`, implemented in
//! `platform::windows`.

pub mod commands;
pub mod compose;
pub mod coordinator;
pub mod error;
pub mod geometry;
pub mod overlay;
pub mod ports;
pub mod post_capture;
pub mod session;
pub mod topology;
pub mod window_pick;
pub mod worker;

use tauri::{App, Runtime};

pub fn setup<R: Runtime>(_app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: screen-capture");
    Ok(())
}
