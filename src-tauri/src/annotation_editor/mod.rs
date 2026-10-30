//! annotation-editor: the editor windows, their commands, and the annotations' save envelope.
//! The annotation schema itself lives in the WebView (`src/features/annotation-editor`).

pub mod commands;
pub mod envelope;
pub mod error;
pub mod export;
pub mod label;
pub mod ports;
pub mod session;
pub mod window_manager;

#[cfg(test)]
mod perf_tests;

use std::sync::Arc;

use tauri::{App, Manager, Runtime};

use self::ports::{EditorDesktop, SystemEditorDesktop};
use self::session::EditorState;
use self::window_manager::{work_area_at_cursor, EditorWindowManager, Launcher};
use crate::shared::{BeforeExitHooks, EditorLauncher, Notifier};

/// Manages `Arc<EditorState>`, `Arc<dyn EditorDesktop>` and `Arc<dyn EditorLauncher>` (which
/// capture-library resolves lazily and screen-capture uses after a capture), and registers the
/// before-exit hook that flushes every open editor.
pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: annotation-editor");
    let notifier = app.state::<Arc<dyn Notifier>>().inner().clone();
    let hooks = app.state::<Arc<BeforeExitHooks>>().inner().clone();
    let state = Arc::new(EditorState::default());
    let manager = Arc::new(EditorWindowManager::new(
        app.handle().clone(),
        state.clone(),
        notifier,
        work_area_at_cursor(app.handle().clone()),
    ));

    let hook_manager = manager.clone();
    hooks.register(
        "annotation-editor",
        Box::new(move || hook_manager.flush_all()),
    );
    app.manage(state);
    app.manage(Arc::new(SystemEditorDesktop {
        app: app.handle().clone(),
    }) as Arc<dyn EditorDesktop>);
    app.manage(Arc::new(Launcher(manager)) as Arc<dyn EditorLauncher>);
    Ok(())
}

#[cfg(test)]
mod tests {
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::*;
    use crate::app_shell::startup::{setup_at, Platform};
    use crate::shared::ExitCheck;

    #[test]
    fn the_launcher_is_available_to_the_other_specs() {
        let root = tempfile::tempdir().unwrap();
        let mut app = mock_builder().build(mock_context(noop_assets())).unwrap();
        setup_at(
            app.handle(),
            root.path().join("settings.json"),
            &root.path().join("Pictures"),
            Platform::production(app.handle()),
        );
        crate::setup_downstream(&mut app).unwrap();

        assert!(app.try_state::<Arc<dyn EditorLauncher>>().is_some());
        assert!(app.try_state::<Arc<dyn EditorDesktop>>().is_some());
        assert!(app.try_state::<Arc<EditorState>>().is_some());
        // No editor is open: the hooks have nothing to report.
        let hooks = app.state::<Arc<BeforeExitHooks>>().inner().clone();
        assert_eq!(hooks.run_all(), ExitCheck::default());
    }
}
