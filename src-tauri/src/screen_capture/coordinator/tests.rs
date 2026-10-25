use std::sync::{Arc, Mutex};

use super::*;
use crate::screen_capture::ports::fake::{CallLog, FakePlatform, FakePresenter};
use crate::screen_capture::topology::testing::{mixed_dpi, monitor};
use crate::screen_capture::window_pick::WindowCandidate;
use crate::screen_capture::worker::fake::InlineWorker;

#[derive(Default)]
struct FakePost(Mutex<Vec<CaptureOutcome>>);

impl PostCaptureSink for FakePost {
    fn deliver(&self, outcome: CaptureOutcome) {
        self.0.lock().unwrap().push(outcome);
    }
}

#[derive(Default)]
struct FakeNotifier(Mutex<Vec<Notice>>);

impl Notifier for FakeNotifier {
    fn notify(&self, notice: Notice) {
        self.0.lock().unwrap().push(notice);
    }
}

struct Fixture {
    /// The platform's and the presenter's calls, in one log.
    calls: CallLog,
    platform: Arc<FakePlatform>,
    presenter: Arc<FakePresenter>,
    post: Arc<FakePost>,
    notifier: Arc<FakeNotifier>,
    worker: Arc<InlineWorker>,
    target: Arc<Mutex<FullscreenTarget>>,
    coordinator: CaptureCoordinator,
}

impl Fixture {
    fn new() -> Self {
        Self::with_worker(InlineWorker::default())
    }

    fn with_worker(worker: InlineWorker) -> Self {
        let calls = CallLog::default();
        let platform = Arc::new(FakePlatform {
            calls: calls.clone(),
            ..FakePlatform::default()
        });
        let presenter = Arc::new(FakePresenter {
            calls: calls.clone(),
            own: vec![5, 6],
            ..FakePresenter::default()
        });
        let post = Arc::new(FakePost::default());
        let notifier = Arc::new(FakeNotifier::default());
        let worker = Arc::new(worker);
        let target = Arc::new(Mutex::new(FullscreenTarget::CursorMonitor));
        let read_target = target.clone();
        let coordinator = CaptureCoordinator::new(CoordinatorPorts {
            platform: platform.clone(),
            presenter: presenter.clone(),
            post: post.clone(),
            notifier: notifier.clone(),
            fullscreen_target: Arc::new(move || *read_target.lock().unwrap()),
            worker: worker.clone(),
        });
        Self {
            calls,
            platform,
            presenter,
            post,
            notifier,
            worker,
            target,
            coordinator,
        }
    }

    /// The calls since the last time, emptying the log.
    fn take_calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }

    fn state(&self) -> &'static str {
        self.coordinator.state_name()
    }

    fn delivered(&self) -> usize {
        self.post.0.lock().unwrap().len()
    }

    fn notices(&self) -> Vec<String> {
        let notices = self.notifier.0.lock().unwrap();
        notices.iter().map(|n| n.title.clone()).collect()
    }

    fn set_topology(&self, topology: MonitorTopology) {
        *self.platform.topology.lock().unwrap() = Ok(topology);
    }

    fn set_windows(&self) {
        *self.platform.windows.lock().unwrap() = vec![
            window(11, PhysicalRect::new(0, 0, 800, 600), "メモ帳", 0),
            window(
                12,
                PhysicalRect::new(0, 0, 1920, 1080),
                "エクスプローラー",
                1,
            ),
        ];
    }
}

fn window(hwnd: isize, rect: PhysicalRect, title: &str, z: u32) -> WindowCandidate {
    WindowCandidate {
        hwnd,
        rect,
        title: title.to_owned(),
        z,
    }
}

const FIRST: SessionId = SessionId(1);
/// The 1080p primary monitor of `mixed_dpi` (the 4K one, left of it, is 0).
const PRIMARY: MonitorIndex = MonitorIndex(1);

/// The RGBA pixel at `(x, y)`.
fn pixel(image: &PixelPatch, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * image.width + x) * 4) as usize;
    image.rgba[at..at + 4].try_into().unwrap()
}

/// A small layout for full-size images: 60 × 45 at 150% left of 40 × 30 at 100%.
fn small_layout() -> MonitorTopology {
    MonitorTopology::new(vec![
        monitor("A", 0, 0, 40, 30, 1.0),
        monitor("B", -60, 0, 60, 45, 1.5),
    ])
}

// --- 4.1 ---

#[test]
fn a_request_returns_at_once_and_the_work_runs_on_the_worker() {
    let f = Fixture::with_worker(InlineWorker::holding());
    f.coordinator.request_capture(CaptureMode::Region);
    assert_eq!(f.state(), "Preparing");
    assert!(f.coordinator.is_capturing());
    assert!(f.take_calls().is_empty());
    f.worker.run_held();
    assert_eq!(f.state(), "Selecting");
}

#[test]
fn a_request_during_a_capture_is_ignored() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.take_calls();
    f.coordinator.request_capture(CaptureMode::Fullscreen);
    f.coordinator.request_capture(CaptureMode::Window);
    assert!(f.take_calls().is_empty());
    assert_eq!(f.state(), "Selecting");
    assert_eq!(f.delivered(), 0);
}

#[test]
fn a_panic_on_the_worker_ends_in_idle_and_the_next_request_is_taken() {
    let f = Fixture::new();
    *f.platform.grab_panics.lock().unwrap() = true;
    f.coordinator.request_capture(CaptureMode::Region);
    assert_eq!(f.state(), "Idle");
    assert!(!f.coordinator.is_capturing());
    assert_eq!(f.take_calls().last().unwrap(), "hide_all");
    assert_eq!(f.notices(), [CAPTURE_FAILED_TITLE]);

    *f.platform.grab_panics.lock().unwrap() = false;
    f.coordinator.request_capture(CaptureMode::Region);
    assert_eq!(f.state(), "Selecting");
    assert!(f
        .take_calls()
        .contains(&"show_region 2 frames 2 focus 1".to_owned()));
}

// --- 4.2 ---

#[test]
fn a_region_request_freezes_before_showing_and_selects() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    assert_eq!(
        f.take_calls(),
        [
            "topology",
            "ensure_built 2",
            "grab_monitors",
            "show_region 1 frames 2 focus 1"
        ]
    );
    assert_eq!(f.state(), "Selecting");
    f.coordinator.on_overlay_shown(FIRST, PRIMARY);

    // The overlays match the layout now: the next capture does not rebuild them.
    f.coordinator.cancel(FIRST);
    f.take_calls();
    f.coordinator.request_capture(CaptureMode::Region);
    assert_eq!(
        f.take_calls(),
        [
            "topology",
            "grab_monitors",
            "show_region 2 frames 2 focus 1"
        ]
    );
}

#[test]
fn the_overlay_on_the_cursors_monitor_gets_the_focus() {
    let f = Fixture::new();
    *f.platform.cursor.lock().unwrap() = PhysicalPoint::new(-10, 2000);
    f.coordinator.request_capture(CaptureMode::Region);
    assert!(f
        .take_calls()
        .contains(&"show_region 1 frames 2 focus 0".to_owned()));
}

#[test]
fn the_magnifier_reads_the_frozen_pixels_of_the_session() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    let query = MagnifierQuery {
        session: FIRST,
        monitor: PRIMARY,
        x: 0,
        y: 0,
        radius: 1,
    };
    let patch = f.coordinator.magnifier_patch(query).unwrap();
    assert_eq!((patch.width, patch.height), (3, 3));
    // The primary monitor's frozen BGRA [1, 0x80, 0x80, 0], as opaque RGBA.
    assert_eq!(pixel(&patch, 1, 1), [0x80, 0x80, 1, 255]);
    // Above the monitor: no pixel.
    assert_eq!(pixel(&patch, 1, 0), [0, 0, 0, 0]);

    let on_4k = MagnifierQuery {
        monitor: MonitorIndex(0),
        x: 3839,
        y: 2159,
        ..query
    };
    let patch = f.coordinator.magnifier_patch(on_4k).unwrap();
    assert_eq!(pixel(&patch, 1, 1), [0x80, 0x80, 0, 255]);

    let error = |q| f.coordinator.magnifier_patch(q).unwrap_err();
    assert_eq!(
        error(MagnifierQuery {
            session: SessionId(9),
            ..query
        }),
        CaptureError::StaleSession
    );
    assert_eq!(
        error(MagnifierQuery { x: 1920, ..query }),
        CaptureError::OutOfRange
    );
    assert_eq!(
        error(MagnifierQuery { y: -1, ..query }),
        CaptureError::OutOfRange
    );
    assert_eq!(
        error(MagnifierQuery {
            monitor: MonitorIndex(2),
            ..query
        }),
        CaptureError::OutOfRange
    );
}

#[test]
fn cancel_hides_the_overlays_and_neither_copies_nor_saves() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.take_calls();
    f.coordinator.cancel(FIRST);
    assert_eq!(f.take_calls(), ["hide_all"]);
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.delivered(), 0);
    assert!(f.notices().is_empty());
}

#[test]
fn requests_for_an_earlier_session_are_ignored() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.coordinator.cancel(FIRST);
    f.coordinator.request_capture(CaptureMode::Region);
    f.take_calls();

    f.coordinator.cancel(FIRST);
    assert_eq!(f.state(), "Selecting");
    assert_eq!(
        f.coordinator
            .commit_region(FIRST, PRIMARY, PhysicalRect::new(0, 0, 10, 10)),
        Err(CommitError::StaleSession)
    );
    assert_eq!(
        f.coordinator.pointer_moved(FIRST, PhysicalPoint::new(1, 1)),
        Err(CaptureError::StaleSession)
    );
    assert!(f.take_calls().is_empty());
    assert_eq!(f.delivered(), 0);
}

#[test]
fn a_failed_freeze_is_reported_and_ends_in_idle() {
    let f = Fixture::new();
    *f.platform.grab_error.lock().unwrap() = Some(CaptureError::GrabFailed {
        monitor: PRIMARY,
        reason: "BitBlt failed".to_owned(),
    });
    f.coordinator.request_capture(CaptureMode::Region);
    let calls = f.take_calls();
    assert!(!calls.iter().any(|c| c.starts_with("show_region")));
    assert_eq!(calls.last().unwrap(), "hide_all");
    assert_eq!(f.state(), "Idle");
    let notices = f.notifier.0.lock().unwrap();
    assert_eq!(notices[0].title, START_FAILED_TITLE);
    assert_eq!(notices[0].body, "could not freeze monitor 1: BitBlt failed");
}

#[test]
fn a_failed_show_is_reported_and_ends_in_idle() {
    let f = Fixture::new();
    *f.presenter.fail_show.lock().unwrap() = Some(CaptureError::OverlayUnavailable {
        reason: "gone".to_owned(),
    });
    f.coordinator.request_capture(CaptureMode::Region);
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.take_calls().last().unwrap(), "hide_all");
    assert_eq!(f.notices(), [START_FAILED_TITLE]);
}

// --- 4.3 ---

#[test]
fn a_selection_smaller_than_a_pixel_keeps_selecting() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.take_calls();
    for rect in [
        PhysicalRect::new(10, 10, 0, 50),
        PhysicalRect::new(10, 10, 50, 0),
        // Entirely right of the monitor.
        PhysicalRect::new(1920, 10, 50, 50),
    ] {
        assert_eq!(
            f.coordinator.commit_region(FIRST, PRIMARY, rect),
            Err(CommitError::TooSmall)
        );
    }
    assert_eq!(f.state(), "Selecting");
    assert!(f.take_calls().is_empty());
    assert_eq!(f.delivered(), 0);
}

#[test]
fn a_committed_region_is_cut_inside_its_monitor_and_handed_over() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.take_calls();
    // Dragged past the bottom-right corner of the primary monitor.
    let dragged = PhysicalRect::new(1800, 1000, 500, 500);
    f.coordinator
        .commit_region(FIRST, PRIMARY, dragged)
        .unwrap();
    assert_eq!(f.take_calls(), ["topology", "hide_all"]);
    assert_eq!(f.state(), "Idle");

    let delivered = f.post.0.lock().unwrap();
    let outcome = &delivered[0];
    assert_eq!(outcome.mode, CaptureMode::Region);
    assert_eq!((outcome.image.width, outcome.image.height), (120, 80));
    assert_eq!(pixel(&outcome.image, 119, 79), [0x80, 0x80, 1, 255]);
    assert_eq!(outcome.window_title, None);
    assert!(outcome.captured_at <= Local::now());
}

#[test]
fn a_region_on_the_150_percent_monitor_keeps_its_physical_size() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.coordinator
        .commit_region(
            FIRST,
            MonitorIndex(0),
            PhysicalRect::new(3000, 2000, 900, 300),
        )
        .unwrap();
    let delivered = f.post.0.lock().unwrap();
    let image = &delivered[0].image;
    assert_eq!((image.width, image.height), (840, 160));
    assert_eq!(pixel(image, 0, 0), [0x80, 0x80, 0, 255]);
}

#[test]
fn the_capture_time_is_when_the_screen_froze() {
    let f = Fixture::with_worker(InlineWorker::holding());
    f.coordinator.request_capture(CaptureMode::Region);
    f.worker.run_held();
    let after_freeze = Local::now();
    f.coordinator
        .commit_region(FIRST, PRIMARY, PhysicalRect::new(0, 0, 5, 5))
        .unwrap();
    assert_eq!(f.state(), "Finalizing");
    f.worker.run_held();
    assert_eq!(f.state(), "Idle");
    assert!(f.post.0.lock().unwrap()[0].captured_at <= after_freeze);
}

#[test]
fn a_layout_change_found_at_commit_cancels() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.set_topology(MonitorTopology::new(vec![monitor(
        "DISPLAY1", 0, 0, 1920, 1080, 1.0,
    )]));
    f.take_calls();
    assert_eq!(
        f.coordinator
            .commit_region(FIRST, PRIMARY, PhysicalRect::new(0, 0, 10, 10)),
        Err(CommitError::TopologyChanged)
    );
    assert_eq!(f.take_calls(), ["topology", "hide_all"]);
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.delivered(), 0);

    // The layout cannot be read either: the same.
    f.set_topology(mixed_dpi());
    f.coordinator.request_capture(CaptureMode::Region);
    *f.platform.topology.lock().unwrap() = Err(CaptureError::Internal("busy".to_owned()));
    assert_eq!(
        f.coordinator
            .commit_region(SessionId(2), PRIMARY, PhysicalRect::new(0, 0, 10, 10)),
        Err(CommitError::TopologyChanged)
    );
    assert_eq!(f.state(), "Idle");
}

// --- 4.4 ---

#[test]
fn window_mode_lists_windows_without_snipts_own_and_highlights_under_the_cursor() {
    let f = Fixture::new();
    f.set_windows();
    f.coordinator.request_capture(CaptureMode::Window);
    assert_eq!(
        f.take_calls(),
        [
            "topology",
            "ensure_built 2",
            "list_windows exclude [5, 6]",
            "show_window_picker 1 focus 1",
            "highlight 1 Some(PhysicalRect { x: 0, y: 0, width: 800, height: 600 })",
        ]
    );
    assert_eq!(f.state(), "Selecting");
}

#[test]
fn moving_the_pointer_updates_the_highlight() {
    let f = Fixture::new();
    f.set_windows();
    f.coordinator.request_capture(CaptureMode::Window);
    f.take_calls();
    let mv = |x, y| f.coordinator.pointer_moved(FIRST, PhysicalPoint::new(x, y));
    mv(1000, 900).unwrap();
    // Still over the same window: nothing to send.
    mv(1001, 900).unwrap();
    mv(-100, 50).unwrap();
    assert_eq!(
        f.take_calls(),
        [
            "highlight 1 Some(PhysicalRect { x: 0, y: 0, width: 1920, height: 1080 })",
            "highlight 1 None",
        ]
    );
}

#[test]
fn a_committed_window_is_captured_with_its_title() {
    let f = Fixture::new();
    f.set_windows();
    f.coordinator.request_capture(CaptureMode::Window);
    f.take_calls();
    f.coordinator.commit_window(FIRST).unwrap();
    assert_eq!(
        f.take_calls(),
        ["topology", "capture_window 11", "hide_all"]
    );
    assert_eq!(f.state(), "Idle");
    let delivered = f.post.0.lock().unwrap();
    let outcome = &delivered[0];
    assert_eq!(outcome.mode, CaptureMode::Window);
    assert_eq!(outcome.window_title.as_deref(), Some("メモ帳"));
    assert_eq!((outcome.image.width, outcome.image.height), (2, 1));
}

#[test]
fn a_window_commit_needs_a_highlighted_window() {
    let f = Fixture::new();
    f.set_windows();
    f.coordinator.request_capture(CaptureMode::Window);
    f.coordinator
        .pointer_moved(FIRST, PhysicalPoint::new(-100, 50))
        .unwrap();
    f.take_calls();
    assert_eq!(
        f.coordinator.commit_window(FIRST),
        Err(CommitError::NoWindowHighlighted)
    );
    assert_eq!(f.state(), "Selecting");
    assert!(f.take_calls().is_empty());
}

#[test]
fn a_failed_window_capture_is_reported_and_neither_copied_nor_saved() {
    let f = Fixture::new();
    f.set_windows();
    *f.platform.window_image.lock().unwrap() = Err(CaptureError::WindowCaptureFailed {
        reason: "timed out".to_owned(),
    });
    f.coordinator.request_capture(CaptureMode::Window);
    f.coordinator.commit_window(FIRST).unwrap();
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.delivered(), 0);
    assert_eq!(f.take_calls().last().unwrap(), "hide_all");
    let notices = f.notifier.0.lock().unwrap();
    assert_eq!(notices[0].title, WINDOW_FAILED_TITLE);
    assert_eq!(notices[0].body, "could not capture the window: timed out");
}

#[test]
fn cancelling_window_mode_captures_nothing() {
    let f = Fixture::new();
    f.set_windows();
    f.coordinator.request_capture(CaptureMode::Window);
    f.take_calls();
    f.coordinator.cancel(FIRST);
    assert_eq!(f.take_calls(), ["hide_all"]);
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.delivered(), 0);
}

// --- 4.5 ---

#[test]
fn fullscreen_captures_the_cursors_monitor_without_overlays() {
    let f = Fixture::new();
    f.set_topology(small_layout());
    *f.platform.cursor.lock().unwrap() = PhysicalPoint::new(5, 5);
    f.coordinator.request_capture(CaptureMode::Fullscreen);
    assert_eq!(f.take_calls(), ["topology", "grab_monitors"]);
    assert_eq!(f.state(), "Idle");

    *f.platform.cursor.lock().unwrap() = PhysicalPoint::new(-5, 40);
    f.coordinator.request_capture(CaptureMode::Fullscreen);
    let delivered = f.post.0.lock().unwrap();
    let sizes: Vec<_> = delivered
        .iter()
        .map(|o| (o.mode, o.image.width, o.image.height))
        .collect();
    assert_eq!(
        sizes,
        [
            (CaptureMode::Fullscreen, 40, 30),
            (CaptureMode::Fullscreen, 60, 45)
        ]
    );
}

#[test]
fn fullscreen_of_all_monitors_keeps_the_layout_and_leaves_gaps_transparent() {
    let f = Fixture::new();
    f.set_topology(small_layout());
    *f.target.lock().unwrap() = FullscreenTarget::AllMonitors;
    f.coordinator.request_capture(CaptureMode::Fullscreen);
    assert!(!f.take_calls().iter().any(|c| c.starts_with("show")));
    let delivered = f.post.0.lock().unwrap();
    let image = &delivered[0].image;
    assert_eq!((image.width, image.height), (100, 45));
    assert_eq!(pixel(image, 0, 44), [0x80, 0x80, 0, 255]);
    assert_eq!(pixel(image, 99, 29), [0x80, 0x80, 1, 255]);
    // Below the shorter monitor on the right.
    assert_eq!(pixel(image, 99, 30), [0, 0, 0, 0]);
    let transparent = image.rgba.chunks_exact(4).filter(|p| p[3] == 0).count();
    assert_eq!(transparent, 40 * 15);
}

#[test]
fn a_failed_fullscreen_freeze_is_reported() {
    let f = Fixture::new();
    *f.platform.grab_error.lock().unwrap() = Some(CaptureError::GrabFailed {
        monitor: PRIMARY,
        reason: "BitBlt failed".to_owned(),
    });
    f.coordinator.request_capture(CaptureMode::Fullscreen);
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.delivered(), 0);
    assert_eq!(f.notices(), [START_FAILED_TITLE]);
}

// --- 4.6 ---

#[test]
fn a_layout_change_while_selecting_cancels_silently_then_rebuilds() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.take_calls();
    f.coordinator.on_topology_changed();
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.take_calls(), ["hide_all"]);
    assert!(f.notices().is_empty());
    assert_eq!(f.worker.delays(), [REBUILD_DEBOUNCE]);
    f.worker.run_delayed();
    assert_eq!(f.take_calls(), ["topology", "ensure_built 2"]);
}

#[test]
fn a_layout_change_while_preparing_cancels_before_anything_shows() {
    let f = Fixture::with_worker(InlineWorker::holding());
    f.coordinator.request_capture(CaptureMode::Region);
    f.coordinator.on_topology_changed();
    assert_eq!(f.state(), "Idle");
    f.worker.run_held();
    assert!(!f.take_calls().iter().any(|c| c.starts_with("show")));
    assert_eq!(f.state(), "Idle");
    assert!(f.notices().is_empty());
}

#[test]
fn successive_layout_changes_rebuild_once() {
    let f = Fixture::new();
    for _ in 0..3 {
        f.coordinator.on_topology_changed();
    }
    f.worker.run_delayed();
    assert_eq!(f.take_calls(), ["topology", "ensure_built 2"]);
    // Nothing left to rebuild.
    f.coordinator.request_capture(CaptureMode::Region);
    assert!(!f.take_calls().iter().any(|c| c.starts_with("ensure_built")));
}

#[test]
fn the_rebuild_waits_until_the_capture_ends() {
    let f = Fixture::with_worker(InlineWorker::holding());
    f.coordinator.request_capture(CaptureMode::Region);
    f.worker.run_held();
    f.coordinator
        .commit_region(FIRST, PRIMARY, PhysicalRect::new(0, 0, 5, 5))
        .unwrap();
    f.coordinator.on_topology_changed();
    assert_eq!(f.state(), "Finalizing");
    assert!(f.worker.delays().is_empty());
    f.worker.run_held();
    assert_eq!(f.state(), "Idle");
    assert_eq!(f.delivered(), 1);
    assert_eq!(f.worker.delays(), [REBUILD_DEBOUNCE]);
}

#[test]
fn the_next_capture_uses_the_new_layout() {
    let f = Fixture::new();
    f.coordinator.request_capture(CaptureMode::Region);
    f.coordinator.cancel(FIRST);
    f.set_topology(MonitorTopology::new(vec![monitor(
        "DISPLAY1", 0, 0, 1920, 1080, 1.0,
    )]));
    f.coordinator.on_topology_changed();
    f.take_calls();
    // Before the debounced rebuild has run.
    f.coordinator.request_capture(CaptureMode::Region);
    assert_eq!(
        f.take_calls(),
        [
            "topology",
            "ensure_built 1",
            "grab_monitors",
            "show_region 2 frames 1 focus 0"
        ]
    );
    // The rebuild scheduled earlier finds nothing left to do.
    f.coordinator.cancel(SessionId(2));
    f.take_calls();
    f.worker.run_delayed();
    assert!(f.take_calls().is_empty());
}
