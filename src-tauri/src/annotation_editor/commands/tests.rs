//! Command tests over the mock runtime's IPC, with a library on a temporary folder and a
//! recording clipboard and dialog.

use std::path::PathBuf;

use chrono::{Local, TimeZone};
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

use super::*;
use crate::annotation_editor::envelope;
use crate::annotation_editor::ports::fake::RecordingDesktop;
use crate::capture_library::ports::fake::RecordingDesktop as LibraryDesktop;
use crate::capture_library::{CaptureImage, CaptureItem, LibraryDeps, NewCapture};
use crate::platform::image_formats::png_from_rgba;
use crate::shared::{CaptureMode, Notice, Notifier};

struct Quiet;

impl Notifier for Quiet {
    fn notify(&self, _notice: Notice) {}
}

struct Fixture {
    dir: tempfile::TempDir,
    library: Arc<CaptureLibrary>,
    desktop: Arc<RecordingDesktop>,
    item: CaptureItem,
    app: tauri::App<MockRuntime>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let library = CaptureLibrary::open(LibraryDeps {
            save_dir: dir.path().join("Snipt"),
            desktop: Box::new(Arc::new(LibraryDesktop::default())),
            notifier: Arc::new(Quiet),
            events: Box::new(|_| {}),
            editor: Box::new(|| None),
            serve_thumbnails: Box::new(|_| {}),
        });
        let item = library
            .save_capture(NewCapture {
                image: CaptureImage::Rgba {
                    width: 4,
                    height: 3,
                    pixels: [10, 20, 30, 255].repeat(12),
                },
                mode: CaptureMode::Window,
                captured_at: Local.with_ymd_and_hms(2026, 10, 22, 17, 41, 50).unwrap(),
                window_title: Some("無題 - メモ帳".to_owned()),
            })
            .unwrap();
        let desktop = Arc::new(RecordingDesktop::default());
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                editor_open_session,
                editor_read_original,
                editor_save,
                editor_copy,
                editor_pick_export_path,
                editor_write_export,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        app.manage(library.clone());
        app.manage(Arc::new(desktop.clone()) as Arc<dyn EditorDesktop>);
        app.manage(Arc::new(EditorState::default()));
        Self {
            dir,
            library,
            desktop,
            item,
            app,
        }
    }

    fn window(&self, label: &str) -> WebviewWindow<MockRuntime> {
        self.app.get_webview_window(label).unwrap_or_else(|| {
            WebviewWindowBuilder::new(&self.app, label, Default::default())
                .build()
                .unwrap()
        })
    }

    fn editor(&self) -> WebviewWindow<MockRuntime> {
        self.window(&format!("editor-{}", self.item.id))
    }

    fn invoke(
        &self,
        window: &WebviewWindow<MockRuntime>,
        cmd: &str,
        body: InvokeBody,
        headers: &[(&'static str, &str)],
    ) -> Result<InvokeResponseBody, Value> {
        let mut map = tauri::http::HeaderMap::new();
        for (name, value) in headers {
            map.insert(*name, value.parse().unwrap());
        }
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
            body,
            headers: map,
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(window, request)
    }

    fn json(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        self.invoke(&self.editor(), cmd, InvokeBody::Json(args), &[])
            .map(|body| body.deserialize::<Value>().unwrap())
    }

    fn raw(
        &self,
        cmd: &str,
        bytes: Vec<u8>,
        headers: &[(&'static str, &str)],
    ) -> Result<Value, Value> {
        self.invoke(&self.editor(), cmd, InvokeBody::Raw(bytes), headers)
            .map(|body| body.deserialize::<Value>().unwrap())
    }
}

fn composite() -> Vec<u8> {
    png_from_rgba(4, 3, &[200, 0, 0, 255].repeat(12)).unwrap()
}

fn document() -> Value {
    json!({ "schemaVersion": 1, "image": { "width": 4, "height": 3 }, "crop": null, "shapes": [] })
}

#[test]
fn the_session_describes_the_capture_of_the_calling_window() {
    let f = Fixture::new();
    let session = f.json("editor_open_session", json!({})).unwrap();
    assert_eq!(
        session,
        json!({
            "captureId": f.item.id,
            "width": 4,
            "height": 3,
            "capturedAt": f.item.captured_at.to_rfc3339(),
            "windowTitle": "無題 - メモ帳",
            "annotations": null,
            "warning": null,
            "unsaved": false,
        })
    );

    let original = f
        .invoke(
            &f.editor(),
            "editor_read_original",
            InvokeBody::Json(json!({})),
            &[],
        )
        .unwrap();
    let InvokeResponseBody::Raw(bytes) = original else {
        panic!("the original is sent as raw bytes");
    };
    assert_eq!(bytes, std::fs::read(&f.item.original_path).unwrap());
}

#[test]
fn other_windows_cannot_call_the_commands() {
    let f = Fixture::new();
    for label in ["history", "settings", "editor-20261022_000000_01"] {
        let window = f.window(label);
        for cmd in COMMANDS {
            let body = if cmd == "editor_pick_export_path" {
                InvokeBody::Json(json!({ "suggestedName": "x" }))
            } else {
                InvokeBody::Raw(composite())
            };
            let err = f.invoke(&window, cmd, body, &[]).unwrap_err();
            assert_eq!(err["kind"], "invalidCaptureId", "{label} {cmd}: {err}");
        }
    }
    assert!(f.desktop.copies.lock().unwrap().is_empty());
    assert!(f.desktop.dialogs.lock().unwrap().is_empty());
}

#[test]
fn an_unknown_capture_is_not_found() {
    let f = Fixture::new();
    let window = f.window("editor-20261022_000000");
    let err = f
        .invoke(
            &window,
            "editor_open_session",
            InvokeBody::Json(json!({})),
            &[],
        )
        .unwrap_err();
    assert_eq!(err["kind"], "notFound");
    assert_eq!(err["message"], "キャプチャが見つかりません");
}

#[test]
fn saving_hands_the_document_and_the_composite_to_the_library() {
    let f = Fixture::new();
    let body = envelope::encode(&serde_json::to_vec(&document()).unwrap(), &composite());
    assert_eq!(f.raw("editor_save", body, &[]).unwrap(), Value::Null);

    let session = f.library.load_for_edit(&f.item.id).unwrap();
    assert_eq!(session.annotations, Some(document()));
    assert_eq!(
        std::fs::read(session.item.annotated_path.unwrap()).unwrap(),
        composite()
    );
    assert_eq!(
        f.json("editor_open_session", json!({})).unwrap()["annotations"],
        document()
    );
}

#[test]
fn invalid_saves_are_rejected_before_the_library() {
    let f = Fixture::new();
    let not_png = envelope::encode(&serde_json::to_vec(&document()).unwrap(), b"GIF89a");
    let err = f.raw("editor_save", not_png, &[]).unwrap_err();
    assert_eq!(err["kind"], "invalidPayload");

    let err = f.json("editor_save", json!({ "doc": 1 })).unwrap_err();
    assert_eq!(err["kind"], "invalidPayload");

    assert_eq!(
        f.library.load_for_edit(&f.item.id).unwrap().annotations,
        None
    );
}

#[test]
fn copying_passes_the_pixels_and_the_png() {
    let f = Fixture::new();
    assert_eq!(f.raw("editor_copy", composite(), &[]).unwrap(), Value::Null);
    let copies = f.desktop.copies.lock().unwrap().clone();
    assert_eq!(copies.len(), 1);
    assert_eq!((copies[0].width, copies[0].height), (4, 3));
    assert_eq!(copies[0].rgba, [200, 0, 0, 255].repeat(12));
    assert_eq!(copies[0].png.as_deref(), Some(&composite()[..]));

    let err = f
        .raw("editor_copy", b"not a png".to_vec(), &[])
        .unwrap_err();
    assert_eq!(err["kind"], "invalidPayload");
    assert_eq!(f.desktop.copies.lock().unwrap().len(), 1);
}

#[test]
fn exports_go_only_to_the_path_chosen_in_the_dialog() {
    let f = Fixture::new();
    let chosen = f.dir.path().join("共有用");
    *f.desktop.dialog_answer.lock().unwrap() = Some(chosen.clone());

    let target = f
        .json(
            "editor_pick_export_path",
            json!({ "suggestedName": "Snipt_20261022_174150" }),
        )
        .unwrap();
    assert_eq!(target["format"], "png");
    assert_eq!(target["fileName"], "共有用.png");
    assert_eq!(
        f.desktop.dialogs.lock().unwrap().clone(),
        [(
            format!("editor-{}", f.item.id),
            "Snipt_20261022_174150".to_owned()
        )]
    );
    let token = target["token"].as_str().unwrap().to_owned();

    let written = f.raw(
        "editor_write_export",
        composite(),
        &[(EXPORT_TOKEN_HEADER, token.as_str())],
    );
    assert_eq!(written.unwrap(), Value::Null);
    assert_eq!(
        std::fs::read(chosen.with_extension("png")).unwrap(),
        composite()
    );

    // A JPEG body does not match the PNG target.
    let err = f
        .raw(
            "editor_write_export",
            vec![0xFF, 0xD8, 0xFF, 0xE0],
            &[(EXPORT_TOKEN_HEADER, token.as_str())],
        )
        .unwrap_err();
    assert_eq!(err["kind"], "invalidPayload");
}

#[test]
fn writing_needs_a_known_token() {
    let f = Fixture::new();
    for headers in [&[][..], &[(EXPORT_TOKEN_HEADER, "made-up")][..]] {
        let err = f
            .raw("editor_write_export", composite(), headers)
            .unwrap_err();
        assert_eq!(err["kind"], "unknownToken");
    }
    let leftovers: Vec<PathBuf> = std::fs::read_dir(f.dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(leftovers, [f.dir.path().join("Snipt")]);
}

#[test]
fn a_cancelled_dialog_returns_null() {
    let f = Fixture::new();
    assert_eq!(
        f.json("editor_pick_export_path", json!({ "suggestedName": "x" }))
            .unwrap(),
        Value::Null
    );
}
