use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::Listener;

use super::fake::FakeNative;
use super::*;
use crate::screen_capture::ports::fake::solid_frame;
use crate::screen_capture::topology::testing::{mixed_dpi, monitor};

struct Fixture {
    app: tauri::App<MockRuntime>,
    native: Arc<FakeNative>,
    readiness: Arc<OverlayReadiness>,
    overlays: OverlayManager<MockRuntime>,
}

impl Fixture {
    fn new() -> Self {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        let native = Arc::new(FakeNative::default());
        let readiness = Arc::new(OverlayReadiness::default());
        let overlays = OverlayManager::new(app.handle().clone(), native.clone(), readiness.clone());
        Self {
            app,
            native,
            readiness,
            overlays,
        }
    }

    /// Builds for `mixed_dpi` with every overlay loaded.
    fn built() -> Self {
        let f = Self::new();
        f.overlays.ensure_built(&mixed_dpi()).unwrap();
        f.readiness.mark_ready("overlay-1-0");
        f.readiness.mark_ready("overlay-1-1");
        f.native.calls.lock().unwrap().clear();
        f
    }

    fn native_calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.native.calls.lock().unwrap())
    }

    /// Records the payloads of `event` sent to the window `label`.
    fn received(&self, label: &str, event: &'static str) -> Arc<Mutex<Vec<Value>>> {
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = got.clone();
        self.app
            .get_webview_window(label)
            .unwrap()
            .listen(event, move |e| {
                sink.lock()
                    .unwrap()
                    .push(serde_json::from_str(e.payload()).unwrap());
            });
        got
    }
}

#[test]
fn labels_name_the_build_and_the_monitor() {
    assert_eq!(overlay_label(3, MonitorIndex(1)), "overlay-3-1");
    assert!(is_overlay_label("overlay-3-1"));
    assert!(!is_overlay_label("editor-20261022_174150"));
    assert!(!is_overlay_label("main"));
}

#[test]
fn builds_one_overlay_per_monitor_once_per_layout() {
    let f = Fixture::new();
    f.overlays.ensure_built(&mixed_dpi()).unwrap();
    assert!(f.app.get_webview_window("overlay-1-0").is_some());
    assert!(f.app.get_webview_window("overlay-1-1").is_some());
    assert_eq!(f.native_calls(), ["rebuild_backdrops 2"]);

    // The same layout: nothing to do.
    f.overlays.ensure_built(&mixed_dpi()).unwrap();
    assert!(f.native_calls().is_empty());

    // Another layout: new overlays under new labels.
    let single = MonitorTopology::new(vec![monitor("DISPLAY1", 0, 0, 1920, 1080, 1.0)]);
    f.overlays.ensure_built(&single).unwrap();
    assert!(f.app.get_webview_window("overlay-2-0").is_some());
    assert_eq!(f.native_calls(), ["rebuild_backdrops 1"]);
}

#[test]
fn readiness_waits_for_every_overlay_of_the_current_build() {
    let readiness = OverlayReadiness::default();
    readiness.track(&["overlay-1-0".to_owned(), "overlay-1-1".to_owned()]);
    readiness.mark_ready("overlay-1-0");
    // From an earlier build: ignored.
    readiness.mark_ready("overlay-0-1");
    assert!(!readiness.wait_all(Duration::from_millis(20)));
    readiness.mark_ready("overlay-1-1");
    assert!(readiness.wait_all(Duration::from_millis(20)));
    readiness.track(&["overlay-2-0".to_owned()]);
    assert!(!readiness.wait_all(Duration::ZERO));
}

#[test]
fn a_region_shows_the_backdrops_then_activates_every_overlay() {
    let f = Fixture::built();
    let left = f.received("overlay-1-0", ACTIVATE_EVENT);
    let right = f.received("overlay-1-1", ACTIVATE_EVENT);
    let frames: Vec<_> = mixed_dpi()
        .monitors
        .iter()
        .map(|m| solid_frame(m.index, m.rect))
        .collect();
    f.overlays
        .show_region(SessionId(7), &frames, MonitorIndex(1))
        .unwrap();

    assert_eq!(f.native_calls(), ["show_backdrops 2"]);
    assert_eq!(
        *left.lock().unwrap(),
        [json!({
            "sessionId": 7,
            "mode": "region",
            "monitor": {
                "index": 0,
                "rect": { "x": -3840, "y": 0, "width": 3840, "height": 2160 },
                "scale": 1.5
            }
        })]
    );
    assert_eq!(right.lock().unwrap()[0]["monitor"]["index"], 1);
}

#[test]
fn the_window_picker_shows_no_backdrop() {
    let f = Fixture::built();
    let got = f.received("overlay-1-0", ACTIVATE_EVENT);
    f.overlays
        .show_window_picker(SessionId(8), MonitorIndex(0))
        .unwrap();
    assert!(f.native_calls().is_empty());
    assert_eq!(got.lock().unwrap()[0]["mode"], "window");
}

#[test]
fn highlights_go_to_every_overlay() {
    let f = Fixture::built();
    let left = f.received("overlay-1-0", HIGHLIGHT_EVENT);
    let right = f.received("overlay-1-1", HIGHLIGHT_EVENT);
    let rect = PhysicalRect::new(-100, 10, 300, 200);
    f.overlays.highlight(SessionId(8), Some(rect));
    f.overlays.highlight(SessionId(8), None);
    let expected = [
        json!({ "sessionId": 8, "rect": { "x": -100, "y": 10, "width": 300, "height": 200 } }),
        json!({ "sessionId": 8, "rect": null }),
    ];
    assert_eq!(*left.lock().unwrap(), expected);
    assert_eq!(*right.lock().unwrap(), expected);
}

#[test]
fn hiding_deactivates_the_shown_session_and_hides_the_backdrops() {
    let f = Fixture::built();
    let got = f.received("overlay-1-1", DEACTIVATE_EVENT);
    f.overlays
        .show_window_picker(SessionId(9), MonitorIndex(1))
        .unwrap();
    f.overlays.hide_all();
    f.overlays.hide_all();
    assert_eq!(*got.lock().unwrap(), [json!({ "sessionId": 9 })]);
    assert_eq!(f.native_calls(), ["hide_backdrops", "hide_backdrops"]);
}

#[test]
fn showing_without_overlays_fails() {
    let f = Fixture::new();
    assert!(matches!(
        f.overlays.show_window_picker(SessionId(1), MonitorIndex(0)),
        Err(CaptureError::OverlayUnavailable { .. })
    ));
}

#[test]
fn own_windows_include_the_backdrops() {
    let f = Fixture::built();
    // The mock windows have no handle; the backdrops do.
    assert_eq!(f.overlays.own_hwnds(), [900, 901]);
}
