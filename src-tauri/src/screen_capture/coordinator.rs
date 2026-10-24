//! `CaptureCoordinator`: the capture state machine (screen-capture 4.x). It alone moves
//! between Idle, Preparing, Selecting and Finalizing. The slow work runs on the worker, and
//! every way out of a capture (cancel, failure, done, a panic) ends in Idle.

use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::Local;

use super::compose::{compose_region, patch, PixelPatch};
use super::error::{CaptureError, CommitError};
use super::geometry::{clamp_to_monitor, PhysicalPoint, PhysicalRect};
use super::ports::{CapturePlatform, OverlayPresenter};
use super::post_capture::{CaptureOutcome, PostCaptureSink};
use super::session::{CaptureSession, SessionId, SessionIds};
use super::topology::{MonitorIndex, MonitorTopology, TopologyFingerprint};
use super::window_pick::hit_test;
use super::worker::CaptureWorker;
use crate::shared::{
    CaptureMode, CaptureRequester, FullscreenTarget, Notice, NoticeLevel, Notifier,
};

/// From the request to an overlay on screen; slower ones are logged as warnings.
const OVERLAY_TARGET: Duration = Duration::from_millis(200);
/// How long the layout must stay still before the overlays are rebuilt.
const REBUILD_DEBOUNCE: Duration = Duration::from_millis(500);
const WINDOW_CAPTURE_TIMEOUT: Duration = Duration::from_secs(1);
const START_FAILED_TITLE: &str = "撮影を開始できませんでした";
const WINDOW_FAILED_TITLE: &str = "ウィンドウを撮影できませんでした";
const CAPTURE_FAILED_TITLE: &str = "撮影に失敗しました";

/// A magnifier request: the pixels around a local point of a monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagnifierQuery {
    pub session: SessionId,
    pub monitor: MonitorIndex,
    /// Local physical pixels.
    pub x: i32,
    pub y: i32,
    pub radius: u32,
}

/// Reads the fullscreen target setting at each fullscreen capture.
pub type FullscreenTargetSource = Arc<dyn Fn() -> FullscreenTarget + Send + Sync>;

pub struct CoordinatorPorts {
    pub platform: Arc<dyn CapturePlatform>,
    pub presenter: Arc<dyn OverlayPresenter>,
    pub post: Arc<dyn PostCaptureSink>,
    pub notifier: Arc<dyn Notifier>,
    pub fullscreen_target: FullscreenTargetSource,
    pub worker: Arc<dyn CaptureWorker>,
}

enum State {
    Idle,
    Preparing {
        id: SessionId,
    },
    Selecting(Box<CaptureSession>),
    /// Composing, capturing the window or running the post-capture flow.
    Finalizing {
        id: SessionId,
    },
}

impl State {
    fn session_id(&self) -> Option<SessionId> {
        match self {
            State::Idle => None,
            State::Preparing { id } | State::Finalizing { id } => Some(*id),
            State::Selecting(session) => Some(session.id),
        }
    }

    /// The selection of session `id`, if it is going on.
    fn selecting(&mut self, id: SessionId) -> Option<&mut CaptureSession> {
        match self {
            State::Selecting(session) if session.id == id => Some(session),
            _ => None,
        }
    }
}

struct Shared {
    state: State,
    /// The layout the overlays were built for.
    built: Option<TopologyFingerprint>,
    /// The layout changed since the overlays were built.
    rebuild_pending: bool,
    /// Counts layout changes, so that only the last scheduled rebuild runs.
    generation: u64,
}

struct Inner {
    ports: CoordinatorPorts,
    ids: SessionIds,
    shared: Mutex<Shared>,
}

/// Cheap to clone: the clones share one state machine.
#[derive(Clone)]
pub struct CaptureCoordinator {
    inner: Arc<Inner>,
}

impl CaptureRequester for CaptureCoordinator {
    fn request_capture(&self, mode: CaptureMode) {
        let id = {
            let mut shared = self.shared();
            if !matches!(shared.state, State::Idle) {
                log::info!("capture: {mode:?} request ignored, a capture is in progress");
                return;
            }
            let id = self.inner.ids.next();
            shared.state = State::Preparing { id };
            id
        };
        let requested_at = Instant::now();
        self.spawn(Some(id), move |this| this.prepare(id, mode, requested_at));
    }
}

impl CaptureCoordinator {
    pub fn new(ports: CoordinatorPorts) -> Self {
        Self {
            inner: Arc::new(Inner {
                ports,
                ids: SessionIds::default(),
                shared: Mutex::new(Shared {
                    state: State::Idle,
                    built: None,
                    rebuild_pending: false,
                    generation: 0,
                }),
            }),
        }
    }

    /// The capture flag: true from a request until the capture ends.
    pub fn is_capturing(&self) -> bool {
        !matches!(self.shared().state, State::Idle)
    }

    /// An overlay of session `session` is on screen; logs how long it took.
    pub fn on_overlay_shown(&self, session: SessionId, monitor: MonitorIndex) {
        let Some(elapsed) = self
            .shared()
            .state
            .selecting(session)
            .map(|s| s.started_at.elapsed())
        else {
            return;
        };
        let message = format!(
            "capture {}: overlay {} shown {elapsed:?} after the request",
            session.0, monitor.0
        );
        if elapsed > OVERLAY_TARGET {
            log::warn!("{message}");
        } else {
            log::info!("{message}");
        }
    }

    /// The frozen pixels around a point, for the magnifier.
    pub fn magnifier_patch(&self, q: MagnifierQuery) -> Result<PixelPatch, CaptureError> {
        let mut shared = self.shared();
        let session = shared
            .state
            .selecting(q.session)
            .ok_or(CaptureError::StaleSession)?;
        let monitor = session
            .topology
            .get(q.monitor)
            .ok_or(CaptureError::OutOfRange)?;
        let (width, height) = monitor.size();
        let local = PhysicalRect::new(q.x, q.y, 1, 1);
        if PhysicalRect::new(0, 0, width, height).intersection(&local) != Some(local) {
            return Err(CaptureError::OutOfRange);
        }
        let frame = session
            .frozen
            .iter()
            .flatten()
            .find(|frame| frame.monitor == q.monitor)
            .ok_or(CaptureError::OutOfRange)?;
        let center = monitor.to_global(local);
        Ok(patch(
            frame,
            PhysicalPoint::new(center.x, center.y),
            q.radius,
        ))
    }

    /// Window mode: highlights the window under the pointer.
    pub fn pointer_moved(
        &self,
        session: SessionId,
        global: PhysicalPoint,
    ) -> Result<(), CaptureError> {
        let rect = {
            let mut shared = self.shared();
            let selection = shared
                .state
                .selecting(session)
                .ok_or(CaptureError::StaleSession)?;
            let Some(windows) = &selection.windows else {
                return Ok(());
            };
            let hit = hit_test(windows, global)
                .and_then(|hit| windows.iter().position(|w| std::ptr::eq(w, hit)));
            if hit == selection.highlighted {
                return Ok(());
            }
            let rect = hit.map(|i| windows[i].rect);
            selection.highlighted = hit;
            rect
        };
        self.inner.ports.presenter.highlight(session, rect);
        Ok(())
    }

    /// `pointer_moved` for a point local to `monitor`, as the overlays report it.
    pub fn pointer_on_monitor(
        &self,
        session: SessionId,
        monitor: MonitorIndex,
        local: PhysicalPoint,
    ) -> Result<(), CaptureError> {
        let global = {
            let mut shared = self.shared();
            let selection = shared
                .state
                .selecting(session)
                .ok_or(CaptureError::StaleSession)?;
            let info = selection
                .topology
                .get(monitor)
                .ok_or(CaptureError::OutOfRange)?;
            info.to_global(PhysicalRect::new(local.x, local.y, 1, 1))
        };
        self.pointer_moved(session, PhysicalPoint::new(global.x, global.y))
    }

    /// Region mode: takes `local` (on `monitor`, kept inside it) as the capture.
    pub fn commit_region(
        &self,
        session: SessionId,
        monitor: MonitorIndex,
        local: PhysicalRect,
    ) -> Result<(), CommitError> {
        let (global, fingerprint) = {
            let mut shared = self.shared();
            let selection = shared
                .state
                .selecting(session)
                .filter(|s| s.frozen.is_some())
                .ok_or(CommitError::StaleSession)?;
            let info = selection
                .topology
                .get(monitor)
                .ok_or(CommitError::StaleSession)?;
            let clamped = clamp_to_monitor(local, info.size());
            if clamped.is_empty() {
                return Err(CommitError::TooSmall);
            }
            (info.to_global(clamped), selection.fingerprint)
        };
        self.check_layout(session, fingerprint)?;
        let selection = self
            .start_finalizing(session)
            .ok_or(CommitError::StaleSession)?;
        self.inner.ports.presenter.hide_all();
        let committed_at = Instant::now();
        self.spawn(Some(session), move |this| {
            let frames = selection.frozen.unwrap_or_default();
            let image = compose_region(&frames, global);
            this.deliver(
                session,
                CaptureOutcome {
                    image,
                    mode: CaptureMode::Region,
                    captured_at: selection.frozen_at,
                    window_title: None,
                    committed_at,
                },
            );
        });
        Ok(())
    }

    /// Window mode: captures the highlighted window.
    pub fn commit_window(&self, session: SessionId) -> Result<(), CommitError> {
        let fingerprint = {
            let mut shared = self.shared();
            let selection = shared
                .state
                .selecting(session)
                .ok_or(CommitError::StaleSession)?;
            if selection.highlighted_window().is_none() {
                return Err(CommitError::NoWindowHighlighted);
            }
            selection.fingerprint
        };
        self.check_layout(session, fingerprint)?;
        let selection = self
            .start_finalizing(session)
            .ok_or(CommitError::StaleSession)?;
        // The pointer may have moved on during the check; it was over a window just before.
        let Some(window) = selection.highlighted_window().cloned() else {
            self.end_and_hide(session);
            return Err(CommitError::NoWindowHighlighted);
        };
        let committed_at = Instant::now();
        self.spawn(Some(session), move |this| {
            let result = this
                .inner
                .ports
                .platform
                .capture_window(window.hwnd, WINDOW_CAPTURE_TIMEOUT);
            let captured_at = Local::now();
            this.inner.ports.presenter.hide_all();
            match result {
                Ok(image) => this.deliver(
                    session,
                    CaptureOutcome {
                        image,
                        mode: CaptureMode::Window,
                        captured_at,
                        window_title: (!window.title.is_empty()).then_some(window.title),
                        committed_at,
                    },
                ),
                Err(error) => {
                    log::warn!("capture {}: {error}", session.0);
                    this.notify(WINDOW_FAILED_TITLE, error.to_string());
                    this.end(session);
                }
            }
        });
        Ok(())
    }

    /// Ends session `session` without capturing; later requests for it are stale.
    pub fn cancel(&self, session: SessionId) {
        let cancelled = {
            let mut shared = self.shared();
            let ours = matches!(
                shared.state,
                State::Preparing { id } if id == session
            ) || shared.state.selecting(session).is_some();
            if ours {
                shared.state = State::Idle;
            }
            ours
        };
        if cancelled {
            self.inner.ports.presenter.hide_all();
            self.after_idle();
        }
    }

    /// Monitors were added, removed, moved or rescaled: a capture being prepared or selected
    /// is cancelled without a notice, and the overlays are rebuilt once things settle.
    pub fn on_topology_changed(&self) {
        let cancelled = {
            let mut shared = self.shared();
            shared.generation += 1;
            shared.rebuild_pending = true;
            let cancelled = matches!(shared.state, State::Preparing { .. } | State::Selecting(_));
            if cancelled {
                shared.state = State::Idle;
            }
            cancelled
        };
        if cancelled {
            log::info!("capture: the monitor layout changed, capture cancelled");
            self.inner.ports.presenter.hide_all();
        }
        self.after_idle();
    }

    fn shared(&self) -> MutexGuard<'_, Shared> {
        self.inner
            .shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `job` on the worker. If it panics, session `owner` (if still current) ends.
    fn spawn(&self, owner: Option<SessionId>, job: impl FnOnce(&Self) + Send + 'static) {
        let this = self.clone();
        self.inner.ports.worker.run(Box::new(move || {
            if panic::catch_unwind(AssertUnwindSafe(|| job(&this))).is_ok() {
                return;
            }
            log::error!("capture: a capture job panicked");
            if let Some(id) = owner {
                if this.end_and_hide(id) {
                    this.notify(CAPTURE_FAILED_TITLE, "内部エラーが発生しました".to_owned());
                }
            }
        }));
    }

    fn prepare(&self, id: SessionId, mode: CaptureMode, requested_at: Instant) {
        let started = match mode {
            CaptureMode::Region => self.start_region(id, requested_at),
            CaptureMode::Window => self.start_window(id, requested_at),
            CaptureMode::Fullscreen => self.capture_fullscreen(id, requested_at),
        };
        if let Err(error) = started {
            log::warn!("capture {}: could not start: {error}", id.0);
            if self.end_and_hide(id) {
                self.notify(START_FAILED_TITLE, error.to_string());
            }
        }
    }

    /// Freezes every monitor before any window of Snipt shows, then shows the overlays.
    fn start_region(&self, id: SessionId, requested_at: Instant) -> Result<(), CaptureError> {
        let ports = &self.inner.ports;
        let topology = self.topology()?;
        self.ensure_overlays(id, &topology)?;
        let frames = ports.platform.grab_monitors(&topology)?;
        let frozen_at = Local::now();
        let focus = focus_monitor(&topology, ports.platform.cursor_position());
        let mut session =
            CaptureSession::new(id, CaptureMode::Region, topology, requested_at, frozen_at);
        session.frozen = Some(frames.clone());
        if !self.start_selecting(session) {
            return Ok(());
        }
        ports.presenter.show_region(id, &frames, focus)?;
        self.hide_if_ended(id);
        Ok(())
    }

    /// Lists the windows (without Snipt's own) and shows the overlays over the live screen.
    fn start_window(&self, id: SessionId, requested_at: Instant) -> Result<(), CaptureError> {
        let ports = &self.inner.ports;
        let topology = self.topology()?;
        self.ensure_overlays(id, &topology)?;
        let windows = ports.platform.list_windows(&ports.presenter.own_hwnds());
        let cursor = ports.platform.cursor_position();
        let focus = focus_monitor(&topology, cursor);
        let mut session = CaptureSession::new(
            id,
            CaptureMode::Window,
            topology,
            requested_at,
            Local::now(),
        );
        session.windows = Some(windows);
        if !self.start_selecting(session) {
            return Ok(());
        }
        ports.presenter.show_window_picker(id, focus)?;
        if !self.hide_if_ended(id) {
            // Highlight the window under the cursor before it moves.
            let _ = self.pointer_moved(id, cursor);
        }
        Ok(())
    }

    /// Freezes and captures the cursor's monitor or every monitor, without overlays.
    fn capture_fullscreen(&self, id: SessionId, requested_at: Instant) -> Result<(), CaptureError> {
        let ports = &self.inner.ports;
        let topology = self.topology()?;
        let frames = ports.platform.grab_monitors(&topology)?;
        let captured_at = Local::now();
        let target = match (ports.fullscreen_target)() {
            FullscreenTarget::CursorMonitor => topology
                .monitor_at(ports.platform.cursor_position())
                .map_or_else(|| topology.virtual_bounds(), |m| m.rect),
            FullscreenTarget::AllMonitors => topology.virtual_bounds(),
        };
        {
            let mut shared = self.shared();
            if !matches!(shared.state, State::Preparing { id: current } if current == id) {
                return Ok(());
            }
            shared.state = State::Finalizing { id };
        }
        let image = compose_region(&frames, target);
        self.deliver(
            id,
            CaptureOutcome {
                image,
                mode: CaptureMode::Fullscreen,
                captured_at,
                window_title: None,
                committed_at: requested_at,
            },
        );
        Ok(())
    }

    fn topology(&self) -> Result<MonitorTopology, CaptureError> {
        let topology = self.inner.ports.platform.topology()?;
        if topology.monitors.is_empty() {
            return Err(CaptureError::Internal("no monitor found".to_owned()));
        }
        Ok(topology)
    }

    /// Rebuilds the overlays when they were built for another layout (that capture may take
    /// longer than 200 ms to show).
    fn ensure_overlays(
        &self,
        id: SessionId,
        topology: &MonitorTopology,
    ) -> Result<(), CaptureError> {
        let fingerprint = topology.fingerprint();
        let current = {
            let shared = self.shared();
            !shared.rebuild_pending && shared.built == Some(fingerprint)
        };
        if current {
            return Ok(());
        }
        self.inner.ports.presenter.ensure_built(topology)?;
        let mut shared = self.shared();
        shared.built = Some(fingerprint);
        // A change during the build cancelled this capture and needs another build.
        if shared.state.session_id() == Some(id) {
            shared.rebuild_pending = false;
        }
        Ok(())
    }

    /// Preparing → Selecting, unless the capture was cancelled meanwhile.
    fn start_selecting(&self, session: CaptureSession) -> bool {
        let mut shared = self.shared();
        if !matches!(shared.state, State::Preparing { id } if id == session.id) {
            return false;
        }
        shared.state = State::Selecting(Box::new(session));
        true
    }

    /// Hides the overlays if session `id` ended while they were being shown.
    fn hide_if_ended(&self, id: SessionId) -> bool {
        let ended = self.shared().state.session_id() != Some(id);
        if ended {
            self.inner.ports.presenter.hide_all();
        }
        ended
    }

    /// Selecting → Finalizing, handing over the session.
    fn start_finalizing(&self, id: SessionId) -> Option<Box<CaptureSession>> {
        let mut shared = self.shared();
        match std::mem::replace(&mut shared.state, State::Idle) {
            State::Selecting(session) if session.id == id => {
                shared.state = State::Finalizing { id };
                Some(session)
            }
            other => {
                shared.state = other;
                None
            }
        }
    }

    /// Cancels session `id` if the layout no longer matches the one it started with.
    fn check_layout(
        &self,
        id: SessionId,
        fingerprint: TopologyFingerprint,
    ) -> Result<(), CommitError> {
        let current = self
            .inner
            .ports
            .platform
            .topology()
            .map(|t| t.fingerprint());
        if current == Ok(fingerprint) {
            return Ok(());
        }
        log::info!(
            "capture {}: the monitor layout changed before the commit",
            id.0
        );
        self.cancel(id);
        Err(CommitError::TopologyChanged)
    }

    fn deliver(&self, id: SessionId, outcome: CaptureOutcome) {
        self.inner.ports.post.deliver(outcome);
        self.end(id);
    }

    /// Session `id`, if current, ends: back to Idle. Returns whether it did.
    fn end(&self, id: SessionId) -> bool {
        let ended = {
            let mut shared = self.shared();
            let ours = shared.state.session_id() == Some(id);
            if ours {
                shared.state = State::Idle;
            }
            ours
        };
        if ended {
            self.after_idle();
        }
        ended
    }

    fn end_and_hide(&self, id: SessionId) -> bool {
        let ended = self.end(id);
        if ended {
            self.inner.ports.presenter.hide_all();
        }
        ended
    }

    /// Schedules the overlay rebuild a layout change asked for, now that no capture runs.
    fn after_idle(&self) {
        let generation = {
            let shared = self.shared();
            if !shared.rebuild_pending || !matches!(shared.state, State::Idle) {
                return;
            }
            shared.generation
        };
        let this = self.clone();
        self.inner.ports.worker.run_after(
            REBUILD_DEBOUNCE,
            Box::new(move || {
                if panic::catch_unwind(AssertUnwindSafe(|| this.rebuild(generation))).is_err() {
                    log::error!("capture: the overlay rebuild panicked");
                }
            }),
        );
    }

    /// Rebuilds the overlays unless the layout changed again or a capture started.
    fn rebuild(&self, generation: u64) {
        {
            let shared = self.shared();
            if shared.generation != generation
                || !shared.rebuild_pending
                || !matches!(shared.state, State::Idle)
            {
                return;
            }
        }
        let built = self.topology().and_then(|topology| {
            self.inner.ports.presenter.ensure_built(&topology)?;
            Ok(topology.fingerprint())
        });
        match built {
            Ok(fingerprint) => {
                let mut shared = self.shared();
                shared.built = Some(fingerprint);
                if shared.generation == generation {
                    shared.rebuild_pending = false;
                }
            }
            // The next capture tries again.
            Err(error) => log::warn!("capture: could not rebuild the overlays: {error}"),
        }
    }

    fn notify(&self, title: &str, body: String) {
        self.inner.ports.notifier.notify(Notice {
            level: NoticeLevel::Error,
            title: title.to_owned(),
            body,
        });
    }

    #[cfg(test)]
    fn state_name(&self) -> &'static str {
        match self.shared().state {
            State::Idle => "Idle",
            State::Preparing { .. } => "Preparing",
            State::Selecting(_) => "Selecting",
            State::Finalizing { .. } => "Finalizing",
        }
    }
}

/// The monitor under `cursor`, which gets the keyboard.
fn focus_monitor(topology: &MonitorTopology, cursor: PhysicalPoint) -> MonitorIndex {
    topology
        .monitor_at(cursor)
        .map_or(MonitorIndex(0), |m| m.index)
}

#[cfg(test)]
mod tests;
