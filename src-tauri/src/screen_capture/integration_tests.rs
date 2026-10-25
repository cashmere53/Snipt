//! screen-capture 8.2: the real coordinator and post-capture flow, end to end, on fake OS
//! ports, a fake store, editor and notifier. No screen is needed.

use std::sync::{Arc, Mutex};

use super::compose::CapturedImage;
use super::coordinator::{CaptureCoordinator, CoordinatorPorts};
use super::error::CaptureError;
use super::geometry::PhysicalRect;
use super::ports::fake::{CallLog, FakePlatform, FakePresenter};
use super::post_capture::{CaptureClipboard, CaptureStore, PostCaptureFlow};
use super::session::SessionId;
use super::topology::MonitorIndex;
use super::window_pick::WindowCandidate;
use super::worker::fake::InlineWorker;
use crate::capture_library::types::fixtures::item;
use crate::capture_library::{CaptureItem, NewCapture, SaveCaptureError};
use crate::shared::{
    CaptureId, CaptureMode, CaptureRequester, EditorLauncher, FullscreenTarget, Notice, Notifier,
};

/// Copy, save and open, in one log.
type Log = Arc<Mutex<Vec<String>>>;

struct Clipboard(Log);

impl CaptureClipboard for Clipboard {
    fn copy(&self, image: &CapturedImage) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .push(format!("copy {}x{}", image.width, image.height));
        Ok(())
    }
}

struct Store(Log);

impl CaptureStore for Store {
    fn save_capture(&self, capture: NewCapture) -> Result<CaptureItem, SaveCaptureError> {
        let title = capture.window_title.unwrap_or_default();
        self.0.lock().unwrap().push(
            format!("save {:?} {title}", capture.mode)
                .trim_end()
                .to_owned(),
        );
        Ok(item("20261022_174150"))
    }
}

struct Editor(Log);

impl EditorLauncher for Editor {
    fn open_editor(&self, id: &CaptureId) {
        self.0.lock().unwrap().push(format!("open {id}"));
    }

    fn close_editor(&self, _id: &CaptureId) {}
}

#[derive(Default)]
struct Notices(Mutex<Vec<String>>);

impl Notifier for Notices {
    fn notify(&self, notice: Notice) {
        self.0.lock().unwrap().push(notice.title);
    }
}

struct App {
    log: Log,
    screen: CallLog,
    platform: Arc<FakePlatform>,
    notices: Arc<Notices>,
    capture: CaptureCoordinator,
}

impl App {
    fn new(fullscreen: FullscreenTarget) -> Self {
        let log = Log::default();
        let screen = CallLog::default();
        let platform = Arc::new(FakePlatform::default());
        let notices = Arc::new(Notices::default());
        let post = PostCaptureFlow::new(
            Arc::new(Clipboard(log.clone())),
            Arc::new(Store(log.clone())),
            Arc::new(Editor(log.clone())),
            notices.clone(),
        );
        let capture = CaptureCoordinator::new(CoordinatorPorts {
            platform: platform.clone(),
            presenter: Arc::new(FakePresenter {
                calls: screen.clone(),
                ..FakePresenter::default()
            }),
            post: Arc::new(post),
            notifier: notices.clone(),
            fullscreen_target: Arc::new(move || fullscreen),
            worker: Arc::new(InlineWorker::default()),
        });
        Self {
            log,
            screen,
            platform,
            notices,
            capture,
        }
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn screen(&self) -> Vec<String> {
        self.screen.lock().unwrap().clone()
    }
}

#[test]
fn a_region_is_copied_then_saved_then_opened() {
    let app = App::new(FullscreenTarget::CursorMonitor);
    app.capture.request_capture(CaptureMode::Region);
    app.capture
        .commit_region(
            SessionId(1),
            MonitorIndex(1),
            PhysicalRect::new(10, 20, 30, 40),
        )
        .unwrap();
    assert_eq!(
        app.log(),
        ["copy 30x40", "save Region", "open 20261022_174150"]
    );
    assert_eq!(
        app.screen(),
        [
            "ensure_built 2",
            "show_region 1 frames 2 focus 1",
            "hide_all"
        ]
    );
    assert!(!app.capture.is_capturing());
}

#[test]
fn a_cancelled_capture_leaves_nothing_behind() {
    let app = App::new(FullscreenTarget::CursorMonitor);
    app.capture.request_capture(CaptureMode::Region);
    app.capture.cancel(SessionId(1));
    assert!(app.log().is_empty());
    assert!(app.notices.0.lock().unwrap().is_empty());
    assert_eq!(app.screen().last().unwrap(), "hide_all");
}

#[test]
fn requests_during_a_capture_are_ignored() {
    let app = App::new(FullscreenTarget::CursorMonitor);
    app.capture.request_capture(CaptureMode::Region);
    app.capture.request_capture(CaptureMode::Fullscreen);
    app.capture.request_capture(CaptureMode::Region);
    let grabs = app
        .platform
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| *c == "grab_monitors")
        .count();
    assert_eq!(grabs, 1);
    assert!(app.log().is_empty());
}

#[test]
fn fullscreen_goes_straight_to_the_editor_without_overlays() {
    let app = App::new(FullscreenTarget::AllMonitors);
    app.capture.request_capture(CaptureMode::Fullscreen);
    // The 4K monitor left of the 1080p one: 5760 × 2160.
    assert_eq!(
        app.log(),
        ["copy 5760x2160", "save Fullscreen", "open 20261022_174150"]
    );
    assert!(app.screen().is_empty());
}

#[test]
fn a_window_is_saved_with_its_title() {
    let app = App::new(FullscreenTarget::CursorMonitor);
    *app.platform.windows.lock().unwrap() = vec![WindowCandidate {
        hwnd: 11,
        rect: PhysicalRect::new(0, 0, 800, 600),
        title: "メモ帳".to_owned(),
        z: 0,
    }];
    app.capture.request_capture(CaptureMode::Window);
    app.capture.commit_window(SessionId(1)).unwrap();
    assert_eq!(
        app.log(),
        ["copy 2x1", "save Window メモ帳", "open 20261022_174150"]
    );
}

#[test]
fn a_failed_window_capture_only_tells_the_user() {
    let app = App::new(FullscreenTarget::CursorMonitor);
    *app.platform.windows.lock().unwrap() = vec![WindowCandidate {
        hwnd: 11,
        rect: PhysicalRect::new(0, 0, 800, 600),
        title: "メモ帳".to_owned(),
        z: 0,
    }];
    *app.platform.window_image.lock().unwrap() = Err(CaptureError::WindowCaptureFailed {
        reason: "access denied".to_owned(),
    });
    app.capture.request_capture(CaptureMode::Window);
    app.capture.commit_window(SessionId(1)).unwrap();
    assert!(app.log().is_empty());
    assert_eq!(
        *app.notices.0.lock().unwrap(),
        ["ウィンドウを撮影できませんでした"]
    );
    assert!(!app.capture.is_capturing());
}

#[test]
fn setup_puts_the_requester_and_the_overlay_state_in_place() {
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    use super::overlay::OverlayReadiness;
    use crate::app_shell::startup::{setup_at, Platform};

    let root = tempfile::tempdir().unwrap();
    let mut app = mock_builder().build(mock_context(noop_assets())).unwrap();
    setup_at(
        app.handle(),
        root.path().join("settings.json"),
        &root.path().join("Pictures"),
        Platform::production(app.handle()),
    );
    crate::setup_downstream(&mut app).unwrap();

    assert!(app.try_state::<Arc<dyn CaptureRequester>>().is_some());
    assert!(app.try_state::<CaptureCoordinator>().is_some());
    assert!(app.try_state::<Arc<OverlayReadiness>>().is_some());
}
