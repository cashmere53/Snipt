//! capture-library: the store of captures and annotations, and the history window's backend.
//!
//! Depends only on app-shell (through `crate::shared`); screen-capture and annotation-editor
//! call into it.

pub mod atomic_fs;
pub mod events;
pub mod index;
pub mod layout;
pub mod naming;
pub mod png_meta;
pub mod ports;
pub mod service;
pub mod thumbnails;
pub mod types;
pub mod unsaved;

use std::sync::Arc;

use tauri::{App, Manager, Runtime};

pub use events::{LibraryEvent, RemovedReason};
pub use service::{CaptureLibrary, LibraryDeps};
pub use types::{
    CaptureImage, CaptureItem, DeleteFailure, DeleteOutcome, EditSession, LibraryError, NewCapture,
    SaveCaptureError,
};

use crate::shared::{BeforeExitHooks, EditorLauncher, Notifier, SettingsStore};

/// Manages `Arc<CaptureLibrary>` on the configured save folder (its index loads in the
/// background), lets the asset protocol serve its thumbnails and registers the before-exit
/// hook that saves unsaved captures.
pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: capture-library");
    let settings = app.state::<Arc<SettingsStore>>().inner().clone();
    let notifier = app.state::<Arc<dyn Notifier>>().inner().clone();
    let hooks = app.state::<Arc<BeforeExitHooks>>().inner().clone();
    let events_app = app.handle().clone();
    let editor_app = app.handle().clone();
    let save_dir = settings.get().save_dir.clone();

    let library = CaptureLibrary::open(LibraryDeps {
        save_dir: save_dir.clone(),
        desktop: Box::new(ports::SystemDesktop),
        notifier,
        events: Box::new(move |event: LibraryEvent| event.emit(&events_app)),
        editor: Box::new(move || {
            editor_app
                .try_state::<Arc<dyn EditorLauncher>>()
                .map(|launcher| launcher.inner().clone())
        }),
    });

    let thumbs = layout::LibraryLayout::new(save_dir).thumbs_dir();
    if let Err(e) = app.asset_protocol_scope().allow_directory(&thumbs, false) {
        log::warn!("library: could not serve {}: {e}", thumbs.display());
    }
    let hook_library = library.clone();
    hooks.register(
        "capture-library",
        Box::new(move || hook_library.before_exit()),
    );
    app.manage(library);
    Ok(())
}
