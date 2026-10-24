//! Command tests over the mock runtime's IPC, with a coordinator on the recording fakes.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

use super::*;
use crate::screen_capture::coordinator::CoordinatorPorts;
use crate::screen_capture::ports::fake::{FakePlatform, FakePresenter};
use crate::screen_capture::post_capture::{CaptureOutcome, PostCaptureSink};
use crate::screen_capture::window_pick::WindowCandidate;
use crate::screen_capture::worker::fake::InlineWorker;
use crate::shared::{CaptureMode, CaptureRequester, FullscreenTarget, Notice, Notifier};

#[derive(Default)]
struct Delivered(Mutex<Vec<(u32, u32)>>);

impl PostCaptureSink for Delivered {
    fn deliver(&self, outcome: CaptureOutcome) {
        let image = outcome.image;
        self.0.lock().unwrap().push((image.width, image.height));
    }
}

struct Quiet;

impl Notifier for Quiet {
    fn notify(&self, _notice: Notice) {}
}

struct Fixture {
    app: tauri::App<MockRuntime>,
    platform: Arc<FakePlatform>,
    presenter: Arc<FakePresenter>,
    delivered: Arc<Delivered>,
    readiness: Arc<OverlayReadiness>,
    coordinator: CaptureCoordinator,
}

impl Fixture {
    fn new() -> Self {
        let platform = Arc::new(FakePlatform::default());
        let presenter = Arc::new(FakePresenter::default());
        let delivered = Arc::new(Delivered::default());
        let coordinator = CaptureCoordinator::new(CoordinatorPorts {
            platform: platform.clone(),
            presenter: presenter.clone(),
            post: delivered.clone(),
            notifier: Arc::new(Quiet),
            fullscreen_target: Arc::new(|| FullscreenTarget::CursorMonitor),
            worker: Arc::new(InlineWorker::default()),
        });
        let readiness = Arc::new(OverlayReadiness::default());
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                capture_overlay_ready,
                capture_overlay_shown,
                capture_overlay_magnifier,
                capture_overlay_pointer,
                capture_overlay_commit_region,
                capture_overlay_commit_window,
                capture_overlay_cancel,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        app.manage(coordinator.clone());
        app.manage(readiness.clone());
        Self {
            app,
            platform,
            presenter,
            delivered,
            readiness,
            coordinator,
        }
    }

    fn window(&self, label: &str) -> WebviewWindow<MockRuntime> {
        self.app.get_webview_window(label).unwrap_or_else(|| {
            WebviewWindowBuilder::new(&self.app, label, Default::default())
                .build()
                .unwrap()
        })
    }

    fn invoke(&self, label: &str, cmd: &str, args: Value) -> Result<InvokeResponseBody, Value> {
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
        get_ipc_response(&self.window(label), request)
    }

    /// From the overlay of monitor 1 (the primary one).
    fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        self.invoke("overlay-1-1", cmd, args)
            .map(|body| body.deserialize::<Value>().unwrap())
    }

    fn calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.presenter.calls.lock().unwrap())
    }
}

/// Arguments every command accepts (extra ones are ignored).
fn any_args() -> Value {
    json!({
        "sessionId": 1,
        "monitorIndex": 1,
        "x": 0,
        "y": 0,
        "radius": 1,
        "rect": { "x": 0, "y": 0, "width": 10, "height": 10 },
    })
}

#[test]
fn other_windows_cannot_call_the_commands() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.calls();
    for label in ["main", "history", "editor-20261022_174150"] {
        for cmd in COMMANDS {
            let err = f.invoke(label, cmd, any_args()).unwrap_err();
            assert_eq!(err, json!({ "kind": "notAnOverlay" }), "{label} {cmd}");
        }
    }
    assert!(f.coordinator.is_capturing());
    assert!(f.calls().is_empty());
}

#[test]
fn ready_marks_the_calling_overlay() {
    let f = Fixture::new();
    f.readiness.track(&["overlay-1-1".to_owned()]);
    assert!(!f.readiness.wait_all(Duration::ZERO));
    assert_eq!(
        f.call("capture_overlay_ready", json!({})).unwrap(),
        Value::Null
    );
    assert!(f.readiness.wait_all(Duration::ZERO));
}

#[test]
fn the_magnifier_answers_raw_rgba_bytes() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    let body = f
        .invoke("overlay-1-1", "capture_overlay_magnifier", any_args())
        .unwrap();
    let InvokeResponseBody::Raw(bytes) = body else {
        panic!("the patch is sent as raw bytes");
    };
    assert_eq!(bytes.len(), 3 * 3 * 4);
    // The center: the primary monitor's frozen gray, opaque.
    assert_eq!(bytes[16..20], [0x80, 0x80, 1, 255]);

    let err = f
        .call(
            "capture_overlay_magnifier",
            json!({ "sessionId": 1, "monitorIndex": 1, "x": 5000, "y": 0, "radius": 1 }),
        )
        .unwrap_err();
    assert_eq!(err, json!({ "kind": "outOfRange" }));
}

#[test]
fn a_region_commit_answers_by_kind_and_ends_the_capture() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.call(
        "capture_overlay_shown",
        json!({ "sessionId": 1, "monitorIndex": 1 }),
    )
    .unwrap();
    let small = json!({
        "sessionId": 1,
        "monitorIndex": 1,
        "rect": { "x": 5, "y": 5, "width": 0, "height": 10 },
    });
    assert_eq!(
        f.call("capture_overlay_commit_region", small).unwrap_err(),
        json!({ "kind": "tooSmall" })
    );
    f.call(
        "capture_overlay_commit_region",
        json!({
            "sessionId": 1,
            "monitorIndex": 1,
            "rect": { "x": 10, "y": 20, "width": 30, "height": 40 },
        }),
    )
    .unwrap();
    assert_eq!(*f.delivered.0.lock().unwrap(), [(30, 40)]);
    assert!(!f.coordinator.is_capturing());
    // The session has ended: cancelling does nothing, committing is stale.
    f.call("capture_overlay_cancel", json!({ "sessionId": 1 }))
        .unwrap();
    assert_eq!(
        f.call("capture_overlay_commit_window", json!({ "sessionId": 1 }))
            .unwrap_err(),
        json!({ "kind": "staleSession" })
    );
}

#[test]
fn window_mode_moves_the_highlight_and_commits() {
    let f = Fixture::new();
    *f.platform.windows.lock().unwrap() = vec![WindowCandidate {
        hwnd: 11,
        rect: PhysicalRect::new(0, 0, 800, 600),
        title: "メモ帳".to_owned(),
        z: 0,
    }];
    *f.platform.cursor.lock().unwrap() = PhysicalPoint::new(1500, 1000);
    f.coordinator.request_capture(CaptureMode::Window);
    assert_eq!(
        f.call("capture_overlay_commit_window", json!({ "sessionId": 1 }))
            .unwrap_err(),
        json!({ "kind": "noWindowHighlighted" })
    );
    f.calls();
    f.call(
        "capture_overlay_pointer",
        json!({ "sessionId": 1, "monitorIndex": 1, "x": 100, "y": 100 }),
    )
    .unwrap();
    assert_eq!(
        f.calls(),
        ["highlight 1 Some(PhysicalRect { x: 0, y: 0, width: 800, height: 600 })"]
    );
    f.call("capture_overlay_commit_window", json!({ "sessionId": 1 }))
        .unwrap();
    assert_eq!(*f.delivered.0.lock().unwrap(), [(2, 1)]);
}

#[test]
fn a_layout_change_before_the_commit_is_reported() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    *f.platform.topology.lock().unwrap() = Err(CaptureError::TopologyChanged);
    let err = f
        .call(
            "capture_overlay_commit_region",
            json!({
                "sessionId": 1,
                "monitorIndex": 1,
                "rect": { "x": 0, "y": 0, "width": 10, "height": 10 },
            }),
        )
        .unwrap_err();
    assert_eq!(err, json!({ "kind": "topologyChanged" }));
    assert!(!f.coordinator.is_capturing());
}

#[test]
fn cancel_ends_the_capture() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.calls();
    f.call("capture_overlay_cancel", json!({ "sessionId": 1 }))
        .unwrap();
    assert_eq!(f.calls(), ["hide_all"]);
    assert!(!f.coordinator.is_capturing());
}

#[test]
fn capability_allows_only_the_overlay_windows() {
    let capability: Value =
        serde_json::from_str(include_str!("../../../capabilities/overlay.json")).unwrap();
    assert_eq!(capability["windows"], json!(["overlay-*"]));
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
    for other in [
        include_str!("../../../capabilities/app-shell.json"),
        include_str!("../../../capabilities/history.json"),
        include_str!("../../../capabilities/editor.json"),
    ] {
        assert!(!other.contains("allow-capture-overlay-"));
    }
}
