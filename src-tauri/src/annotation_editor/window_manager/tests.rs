//! Pure sizing and waiting rules, and the manager over the mock runtime (whose monitor APIs
//! are not implemented, so the work area is injected).

use std::sync::mpsc;

use chrono::{Local, TimeZone};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::Listener;

use super::*;
use crate::annotation_editor::session::EditorState;
use crate::capture_library::ports::fake::RecordingDesktop as LibraryDesktop;
use crate::capture_library::{CaptureImage, CaptureItem, LibraryDeps, NewCapture};
use crate::shared::CaptureMode;

fn id(s: &str) -> CaptureId {
    CaptureId::parse(s).unwrap()
}

fn area(width: u32, height: u32, scale: f64) -> WorkArea {
    WorkArea {
        position: PhysicalPosition::new(0, 0),
        size: PhysicalSize::new(width, height),
        scale,
    }
}

#[test]
fn small_images_get_the_minimum_size() {
    let size = initial_size(PhysicalSize::new(200, 100), &area(1920, 1040, 1.0));
    assert_eq!(size, LogicalSize::new(800.0, 600.0));
}

#[test]
fn medium_images_fit_with_the_ui_margin() {
    let size = initial_size(PhysicalSize::new(1000, 700), &area(1920, 1040, 1.0));
    assert_eq!(size, LogicalSize::new(1048.0, 820.0));
    // At 150%, the image is shown at 1:1 physical pixels: fewer logical pixels.
    let size = initial_size(PhysicalSize::new(1500, 900), &area(2880, 1560, 1.5));
    assert_eq!(size, LogicalSize::new(1048.0, 720.0));
}

#[test]
fn large_images_are_capped_at_90_percent_of_the_work_area() {
    let size = initial_size(PhysicalSize::new(7680, 2160), &area(1920, 1040, 1.0));
    assert_eq!(size, LogicalSize::new(1728.0, 936.0));
    // A work area smaller than the minimum wins over the minimum.
    let size = initial_size(PhysicalSize::new(100, 100), &area(800, 600, 1.0));
    assert_eq!(size, LogicalSize::new(720.0, 540.0));
}

#[test]
fn windows_are_centered_in_the_work_area() {
    let second_monitor = WorkArea {
        position: PhysicalPosition::new(1920, 0),
        ..area(3840, 2080, 2.0)
    };
    assert_eq!(
        centered(LogicalSize::new(800.0, 600.0), &second_monitor),
        LogicalPosition::new(960.0 + 560.0, 220.0)
    );
}

#[test]
fn titles_show_the_capture_time() {
    assert_eq!(
        window_title(&id("20261022_174150")),
        "Snipt - 2026/10/22 17:41:50"
    );
    assert_eq!(
        window_title(&id("20261022_174150_02")),
        "Snipt - 2026/10/22 17:41:50 (2)"
    );
}

#[test]
fn a_window_that_closes_is_not_forced() {
    let (tx, rx) = mpsc::channel();
    tx.send(()).unwrap();
    assert!(wait_or_force(&rx, Duration::from_secs(1), || panic!(
        "forced"
    )));
    let (tx, rx) = mpsc::channel::<()>();
    drop(tx);
    assert!(wait_or_force(&rx, Duration::from_secs(1), || panic!(
        "forced"
    )));
}

#[test]
fn a_silent_window_is_forced_after_the_timeout() {
    let (_tx, rx) = mpsc::channel::<()>();
    let mut forced = false;
    let started = Instant::now();
    assert!(!wait_or_force(&rx, Duration::from_millis(50), || {
        forced = true
    }));
    assert!(forced);
    assert!(started.elapsed() >= Duration::from_millis(50));
}

#[test]
fn failed_and_silent_editors_are_unsaved() {
    let (ok_tx, ok) = mpsc::channel();
    let (failed_tx, failed) = mpsc::channel();
    let (_silent_tx, silent) = mpsc::channel();
    let (gone_tx, gone) = mpsc::channel::<bool>();
    ok_tx.send(true).unwrap();
    failed_tx.send(false).unwrap();
    drop(gone_tx);

    let started = Instant::now();
    let unsaved = unsaved_after(
        vec![
            (id("20261022_174150"), ok),
            (id("20261022_174151"), failed),
            (id("20261022_174152"), silent),
            (id("20261022_174153"), gone),
        ],
        Duration::from_millis(100),
    );
    assert_eq!(
        unsaved,
        [
            "注釈: 20261022_174151",
            "注釈: 20261022_174152",
            "注釈: 20261022_174153"
        ]
    );
    // One timeout for all, not one per editor.
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(
        unsaved_after(Vec::new(), Duration::from_secs(5)),
        Vec::<String>::new()
    );
}

#[derive(Default)]
struct Notices(Mutex<Vec<Notice>>);

impl Notifier for Notices {
    fn notify(&self, notice: Notice) {
        self.0.lock().unwrap().push(notice);
    }
}

struct World {
    _dir: tempfile::TempDir,
    app: tauri::App<MockRuntime>,
    state: Arc<EditorState>,
    notices: Arc<Notices>,
    manager: Arc<EditorWindowManager<MockRuntime>>,
    item: CaptureItem,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let library = CaptureLibrary::open(LibraryDeps {
            save_dir: dir.path().join("Snipt"),
            desktop: Box::new(Arc::new(LibraryDesktop::default())),
            notifier: Arc::new(Notices::default()),
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
                mode: CaptureMode::Region,
                captured_at: Local.with_ymd_and_hms(2026, 10, 22, 17, 41, 50).unwrap(),
                window_title: None,
            })
            .unwrap();
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage(library);
        let state = Arc::new(EditorState::default());
        let notices = Arc::new(Notices::default());
        let mut manager = EditorWindowManager::new(
            app.handle().clone(),
            state.clone(),
            notices.clone(),
            Box::new(|| Some(WorkArea::default())),
        );
        manager.close_timeout = Duration::from_millis(100);
        manager.flush_timeout = Duration::from_millis(200);
        Self {
            _dir: dir,
            app,
            state,
            notices,
            manager: Arc::new(manager),
            item,
        }
    }

    fn label(&self) -> String {
        editor_label(&self.item.id).unwrap()
    }

    fn editors(&self) -> Vec<String> {
        let mut labels: Vec<String> = self.app.webview_windows().into_keys().collect();
        labels.retain(|label| label.starts_with("editor-"));
        labels.sort();
        labels
    }
}

#[test]
fn opening_twice_keeps_one_window() {
    let world = World::new();
    world.manager.show(&world.item.id);
    world.manager.show(&world.item.id);
    assert_eq!(world.editors(), [world.label()]);
    assert!(world.notices.0.lock().unwrap().is_empty());
}

#[test]
fn the_launcher_opens_off_the_calling_thread() {
    let world = World::new();
    let launcher = Launcher(world.manager.clone());
    launcher.open_editor(&world.item.id);
    let deadline = Instant::now() + Duration::from_secs(5);
    while world.editors().is_empty() {
        assert!(Instant::now() < deadline, "no editor opened");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn unknown_captures_are_reported_without_a_window() {
    let world = World::new();
    world.manager.show(&id("20261022_000000"));
    assert!(world.editors().is_empty());
    let notices = world.notices.0.lock().unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].level, NoticeLevel::Error);
}

#[test]
fn closing_an_editor_that_is_not_open_returns_at_once() {
    let world = World::new();
    let started = Instant::now();
    world.manager.close(&world.item.id);
    assert!(started.elapsed() < world.manager.close_timeout);
}

#[test]
fn an_editor_that_closes_itself_is_not_forced() {
    let world = World::new();
    world.manager.show(&world.item.id);
    let state = world.state.clone();
    let id = world.item.id.clone();
    world
        .app
        .listen_any(CLOSE_REQUEST_EVENT, move |_| state.remove(&id));

    let started = Instant::now();
    world.manager.close(&world.item.id);
    assert!(started.elapsed() < world.manager.close_timeout);
}

#[test]
fn a_silent_editor_is_destroyed_after_the_timeout() {
    let world = World::new();
    world.manager.show(&world.item.id);
    let asked = Arc::new(Mutex::new(0));
    let counter = asked.clone();
    world
        .app
        .listen_any(CLOSE_REQUEST_EVENT, move |_| *counter.lock().unwrap() += 1);

    // The mock runtime neither removes destroyed windows nor reports their destruction, so
    // the session's removal (which wakes the waiters) is what shows the forced close.
    let other_waiter = world.state.wait_for_close(&world.item.id);

    let started = Instant::now();
    world.manager.close(&world.item.id);
    assert!(started.elapsed() >= world.manager.close_timeout);
    assert_eq!(*asked.lock().unwrap(), 1);
    assert_eq!(other_waiter.try_recv(), Ok(()));
}

#[test]
fn flushing_reports_the_editors_that_did_not_save_and_keeps_them_open() {
    let world = World::new();
    world.manager.show(&world.item.id);
    let second = world
        .app
        .state::<Arc<CaptureLibrary>>()
        .save_capture(NewCapture {
            image: CaptureImage::Rgba {
                width: 2,
                height: 2,
                pixels: [0, 0, 0, 255].repeat(4),
            },
            mode: CaptureMode::Region,
            captured_at: Local.with_ymd_and_hms(2026, 10, 22, 17, 42, 0).unwrap(),
            window_title: None,
        })
        .unwrap();
    world.manager.show(&second.id);

    // The first editor saves; the second fails on the first round and saves on the retry.
    let rounds = Arc::new(Mutex::new(0));
    let state = world.state.clone();
    let first = world.item.id.clone();
    let second_id = second.id.clone();
    let counter = rounds.clone();
    world.app.listen_any(FLUSH_REQUEST_EVENT, move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
        let request = payload["requestId"].as_str().unwrap().to_owned();
        let mut rounds = counter.lock().unwrap();
        *rounds += 1;
        // Two requests per round; the second editor's first answer fails.
        let saved_second = *rounds > 2;
        if !state.answer_flush(&first, &request, true) {
            assert!(state.answer_flush(&second_id, &request, saved_second));
        }
    });

    let check = world.manager.flush_all();
    assert_eq!(check.unsaved, [format!("注釈: {}", second.id)]);
    let check = world.manager.flush_all();
    assert_eq!(check, ExitCheck::default());
    assert_eq!(world.editors().len(), 2, "the windows stay open");
}

#[test]
fn an_editor_that_does_not_answer_is_unsaved() {
    let world = World::new();
    world.manager.show(&world.item.id);
    let check = world.manager.flush_all();
    assert_eq!(check.unsaved, [format!("注釈: {}", world.item.id)]);
    // The late answer is not taken.
    assert_eq!(world.editors(), [world.label()]);
}
