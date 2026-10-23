//! capture-library: the store of captures and annotations, and the history window's backend.
//!
//! Depends only on app-shell (through `crate::shared`); screen-capture and annotation-editor
//! call into it.

pub mod atomic_fs;
pub mod commands;
pub mod events;
pub mod index;
#[cfg(test)]
mod integration_tests;
pub mod layout;
pub mod naming;
#[cfg(test)]
mod perf_tests;
pub mod png_meta;
pub mod ports;
pub mod service;
pub mod thumbnails;
pub mod types;
pub mod unsaved;
pub mod watcher;

use std::path::Path;
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
/// background), lets the asset protocol serve its thumbnails, follows save folder changes and
/// registers the before-exit hook that saves unsaved captures.
pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: capture-library");
    let settings = app.state::<Arc<SettingsStore>>().inner().clone();
    let notifier = app.state::<Arc<dyn Notifier>>().inner().clone();
    let hooks = app.state::<Arc<BeforeExitHooks>>().inner().clone();
    let events_app = app.handle().clone();
    let editor_app = app.handle().clone();
    let scope_app = app.handle().clone();

    let library = CaptureLibrary::open(LibraryDeps {
        save_dir: settings.get().save_dir.clone(),
        desktop: Box::new(ports::SystemDesktop),
        notifier,
        events: Box::new(move |event: LibraryEvent| event.emit(&events_app)),
        editor: Box::new(move || {
            editor_app
                .try_state::<Arc<dyn EditorLauncher>>()
                .map(|launcher| launcher.inner().clone())
        }),
        // Earlier folders stay allowed: a forbidden pattern could never be allowed again when
        // the user switches back.
        serve_thumbnails: Box::new(move |thumbs: &Path| {
            if let Err(e) = scope_app
                .asset_protocol_scope()
                .allow_directory(thumbs, false)
            {
                log::warn!("library: could not serve {}: {e}", thumbs.display());
            }
        }),
    });

    // Subscribers must return quickly; the switch loads the new folder's index.
    let switch_library = library.clone();
    let switch_settings = settings.clone();
    settings.subscribe(Box::new(move |_| {
        let library = switch_library.clone();
        let settings = switch_settings.clone();
        std::thread::spawn(move || library.switch_save_dir(settings.get().save_dir.clone()));
    }));

    let hook_library = library.clone();
    hooks.register(
        "capture-library",
        Box::new(move || hook_library.before_exit()),
    );
    app.manage(library);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::*;
    use crate::app_shell::startup::{setup_at, Platform};

    #[test]
    fn committing_a_new_save_folder_switches_the_library() {
        let root = tempfile::tempdir().unwrap();
        let mut app = mock_builder().build(mock_context(noop_assets())).unwrap();
        setup_at(
            app.handle(),
            root.path().join("settings.json"),
            &root.path().join("Pictures"),
            Platform::production(app.handle()),
        );
        setup(&mut app).unwrap();
        let library = app.state::<Arc<CaptureLibrary>>().inner().clone();
        let store = app.state::<Arc<SettingsStore>>().inner().clone();

        let mut next = (*store.get()).clone();
        next.save_dir = root.path().join("Elsewhere");
        store.commit(next).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while library.save_dir() != root.path().join("Elsewhere") {
            assert!(Instant::now() < deadline, "the library did not switch");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
