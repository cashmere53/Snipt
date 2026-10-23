//! Tauri commands for the history window. Allowed only for the `history` window by
//! `capabilities/history.json`.
//!
//! Ids arrive as `CaptureId`, so a malformed id is rejected while the arguments are read and
//! never becomes a path. Every command runs on a blocking worker: listing waits for the index,
//! copying may retry the clipboard and deleting waits for open editors to close.

use std::sync::Arc;

use tauri::State;

use super::service::CaptureLibrary;
use super::types::{CaptureItem, DeleteOutcome, LibraryError};
use crate::shared::CaptureId;

async fn blocking<T: Send + 'static>(
    library: &State<'_, Arc<CaptureLibrary>>,
    f: impl FnOnce(&CaptureLibrary) -> T + Send + 'static,
) -> Result<T, LibraryError> {
    let library = library.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(&library))
        .await
        .map_err(|e| LibraryError::Io {
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn library_list(
    library: State<'_, Arc<CaptureLibrary>>,
) -> Result<Vec<CaptureItem>, LibraryError> {
    blocking(&library, CaptureLibrary::list).await
}

#[tauri::command]
pub async fn library_open_in_editor(
    library: State<'_, Arc<CaptureLibrary>>,
    id: CaptureId,
) -> Result<(), LibraryError> {
    blocking(&library, move |library| library.open_in_editor(&id)).await?
}

#[tauri::command]
pub async fn library_copy(
    library: State<'_, Arc<CaptureLibrary>>,
    id: CaptureId,
) -> Result<(), LibraryError> {
    blocking(&library, move |library| library.copy_to_clipboard(&id)).await?
}

#[tauri::command]
pub async fn library_reveal(
    library: State<'_, Arc<CaptureLibrary>>,
    id: CaptureId,
) -> Result<(), LibraryError> {
    blocking(&library, move |library| library.reveal_in_explorer(&id)).await?
}

#[tauri::command]
pub async fn library_delete(
    library: State<'_, Arc<CaptureLibrary>>,
    ids: Vec<CaptureId>,
) -> Result<DeleteOutcome, LibraryError> {
    blocking(&library, move |library| library.delete(&ids)).await?
}

#[tauri::command]
pub async fn library_prune_missing(
    library: State<'_, Arc<CaptureLibrary>>,
    ids: Vec<CaptureId>,
) -> Result<Vec<CaptureId>, LibraryError> {
    blocking(&library, move |library| library.prune_missing(&ids)).await
}

/// Command names, in the order listed in `build.rs` (`AppManifest::commands`).
pub const COMMANDS: [&str; 6] = [
    "library_list",
    "library_open_in_editor",
    "library_copy",
    "library_reveal",
    "library_delete",
    "library_prune_missing",
];

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};
    use serde_json::{json, Value};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

    use super::*;
    use crate::capture_library::ports::fake::{DesktopCall, RecordingDesktop};
    use crate::capture_library::service::LibraryDeps;
    use crate::capture_library::types::{CaptureImage, NewCapture};
    use crate::shared::{CaptureMode, Notice, Notifier};

    struct Quiet;

    impl Notifier for Quiet {
        fn notify(&self, _notice: Notice) {}
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        library: Arc<CaptureLibrary>,
        desktop: Arc<RecordingDesktop>,
        webview: WebviewWindow<tauri::test::MockRuntime>,
        _app: tauri::App<tauri::test::MockRuntime>,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let desktop = Arc::new(RecordingDesktop::default());
        let library = CaptureLibrary::open(LibraryDeps {
            save_dir: dir.path().join("Snipt"),
            desktop: Box::new(desktop.clone()),
            notifier: Arc::new(Quiet),
            events: Box::new(|_| {}),
            editor: Box::new(|| None),
            serve_thumbnails: Box::new(|_| {}),
        });
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                library_list,
                library_open_in_editor,
                library_copy,
                library_reveal,
                library_delete,
                library_prune_missing,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        app.manage(library.clone());
        let webview = WebviewWindowBuilder::new(&app, "history", Default::default())
            .build()
            .unwrap();
        Fixture {
            _dir: dir,
            library,
            desktop,
            webview,
            _app: app,
        }
    }

    fn invoke(f: &Fixture, cmd: &str, args: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&f.webview, request).map(|body| body.deserialize::<Value>().unwrap())
    }

    fn save(f: &Fixture, second: u32) -> CaptureItem {
        f.library
            .save_capture(NewCapture {
                image: CaptureImage::Rgba {
                    width: 2,
                    height: 2,
                    pixels: vec![255; 16],
                },
                mode: CaptureMode::Region,
                captured_at: Local
                    .with_ymd_and_hms(2026, 10, 22, 17, 41, second)
                    .unwrap(),
                window_title: None,
            })
            .unwrap()
    }

    #[test]
    fn valid_ids_reach_the_service() {
        let f = fixture();
        let older = save(&f, 50);
        let newer = save(&f, 51);

        let list = invoke(&f, "library_list", json!({})).unwrap();
        assert_eq!(list[0]["id"], newer.id.as_str());
        assert_eq!(list[1]["id"], older.id.as_str());
        assert_eq!(list[0]["mode"], "region");

        invoke(&f, "library_copy", json!({ "id": older.id })).unwrap();
        invoke(&f, "library_reveal", json!({ "id": older.id })).unwrap();
        invoke(&f, "library_open_in_editor", json!({ "id": older.id })).unwrap();
        let calls = f.desktop.calls();
        assert!(matches!(
            calls[0],
            DesktopCall::Copy {
                width: 2,
                height: 2,
                ..
            }
        ));
        assert_eq!(calls[1], DesktopCall::Reveal(older.original_path.clone()));

        std::fs::remove_file(&newer.original_path).unwrap();
        let pruned = invoke(
            &f,
            "library_prune_missing",
            json!({ "ids": [older.id, newer.id] }),
        )
        .unwrap();
        assert_eq!(pruned, json!([newer.id]));

        let outcome = invoke(&f, "library_delete", json!({ "ids": [older.id] })).unwrap();
        assert_eq!(outcome, json!({ "removed": [older.id], "failed": [] }));
        assert_eq!(invoke(&f, "library_list", json!({})).unwrap(), json!([]));
    }

    #[test]
    fn unknown_ids_are_not_found_and_malformed_ids_are_rejected() {
        let f = fixture();
        for cmd in ["library_copy", "library_reveal", "library_open_in_editor"] {
            let err = invoke(&f, cmd, json!({ "id": "20261022_000000" })).unwrap_err();
            assert_eq!(err["kind"], "notFound", "{cmd}");
            assert_eq!(err["id"], "20261022_000000");
            assert!(err["message"].is_string());

            for bad in ["..\\..\\Windows", "20261022_174150_01", ""] {
                let err = invoke(&f, cmd, json!({ "id": bad })).unwrap_err();
                assert!(err.is_string(), "{cmd} {bad}: {err}");
            }
        }
        let err = invoke(&f, "library_delete", json!({ "ids": ["../x"] })).unwrap_err();
        assert!(err.is_string());

        let outcome = invoke(&f, "library_delete", json!({ "ids": ["20261022_000000"] })).unwrap();
        assert_eq!(outcome["removed"], json!([]));
        assert_eq!(outcome["failed"][0]["error"]["kind"], "notFound");
        assert!(f.desktop.calls().is_empty());
    }

    #[test]
    fn errors_from_the_desktop_are_passed_on() {
        let f = fixture();
        let item = save(&f, 50);
        f.desktop.fail_with(Some(LibraryError::Clipboard {
            message: "busy".to_owned(),
        }));
        let err = invoke(&f, "library_copy", json!({ "id": item.id })).unwrap_err();
        assert_eq!(err["kind"], "clipboard");
    }

    #[test]
    fn capability_allows_only_the_history_window() {
        let capability: Value =
            serde_json::from_str(include_str!("../../capabilities/history.json")).unwrap();
        assert_eq!(capability["windows"], json!(["history"]));
        let permissions: Vec<&str> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        for command in COMMANDS {
            let permission = format!("allow-{}", command.replace('_', "-"));
            assert!(permissions.contains(&permission.as_str()), "{permission}");
        }
    }
}
