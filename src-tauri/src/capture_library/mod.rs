//! capture-library: the store of captures and annotations, and the history window's backend.
//!
//! Depends only on app-shell (through `crate::shared`); screen-capture and annotation-editor
//! call into it.

pub mod atomic_fs;
pub mod events;
pub mod layout;
pub mod naming;
pub mod png_meta;
pub mod ports;
pub mod types;

use tauri::{App, Runtime};

pub use events::{LibraryEvent, RemovedReason};
pub use types::{
    CaptureImage, CaptureItem, DeleteFailure, DeleteOutcome, EditSession, LibraryError, NewCapture,
    SaveCaptureError,
};

pub fn setup<R: Runtime>(_app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: capture-library");
    Ok(())
}
