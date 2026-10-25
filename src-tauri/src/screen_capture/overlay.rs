//! `OverlayManager`: the prebuilt overlay windows, one per monitor (screen-capture 6.1, 6.2).
//! An overlay is a transparent WebView over its monitor; in region mode a native backdrop
//! window under it shows the frozen screen. Both are built ahead of time and only shown and
//! hidden during a capture.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use super::compose::FrozenFrame;
use super::error::CaptureError;
use super::geometry::PhysicalRect;
use super::ports::OverlayPresenter;
use super::session::SessionId;
use super::topology::{MonitorIndex, MonitorInfo, MonitorTopology, TopologyFingerprint};

/// `ACTIVATE_EVENT` and the others in `src/shared/ipc/screen-capture.ts`.
pub const ACTIVATE_EVENT: &str = "overlay://activate";
pub const HIGHLIGHT_EVENT: &str = "overlay://highlight";
pub const DEACTIVATE_EVENT: &str = "overlay://deactivate";

const LABEL_PREFIX: &str = "overlay-";
/// How long a capture waits for overlays still loading.
const READY_TIMEOUT: Duration = Duration::from_secs(1);

/// `overlay-<build>-<monitor>`: each build has new labels, so that a rebuild never waits for
/// the old windows to be gone.
pub fn overlay_label(build: u32, monitor: MonitorIndex) -> String {
    format!("{LABEL_PREFIX}{build}-{}", monitor.0)
}

pub fn is_overlay_label(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OverlayMode {
    Region,
    Window,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayMonitor {
    pub index: MonitorIndex,
    /// Global physical pixels.
    pub rect: PhysicalRect,
    pub scale: f64,
}

/// `OverlayActivate` in `src/shared/ipc/screen-capture.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayActivate {
    pub session_id: SessionId,
    pub mode: OverlayMode,
    pub monitor: OverlayMonitor,
}

/// `OverlayHighlight` in `src/shared/ipc/screen-capture.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayHighlight {
    pub session_id: SessionId,
    /// Global physical pixels; each overlay draws its part.
    pub rect: Option<PhysicalRect>,
}

/// `OverlayDeactivate` in `src/shared/ipc/screen-capture.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayDeactivate {
    pub session_id: SessionId,
}

/// The native side of the overlays: Win32 calls on their windows, and the backdrops.
pub trait OverlayNative: Send + Sync {
    /// Once per new overlay window (square corners).
    fn prepare_overlay(&self, hwnd: isize);
    /// Shows the overlay topmost over `rect`, without activating it.
    fn place_topmost(&self, hwnd: isize, rect: PhysicalRect);
    /// Hides an overlay shown by `place_topmost`. Tauri never saw it shown, so its own
    /// `hide` would do nothing.
    fn hide_overlay(&self, hwnd: isize);
    /// One hidden backdrop per monitor of `topology`, replacing the old ones.
    fn rebuild_backdrops(&self, topology: &MonitorTopology) -> Result<(), CaptureError>;
    /// Shows each frame on its monitor's backdrop.
    fn show_backdrops(&self, frames: &[FrozenFrame]) -> Result<(), CaptureError>;
    /// Hides the backdrops and lets go of their frames.
    fn hide_backdrops(&self);
    fn backdrop_hwnds(&self) -> Vec<isize>;
}

/// Which overlays have loaded their page; shared with the `capture_overlay_ready` command.
#[derive(Default)]
pub struct OverlayReadiness {
    labels: Mutex<Vec<(String, bool)>>,
    changed: Condvar,
}

impl OverlayReadiness {
    fn lock(&self) -> MutexGuard<'_, Vec<(String, bool)>> {
        self.labels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts tracking `labels`, none loaded yet.
    pub(super) fn track(&self, labels: &[String]) {
        *self.lock() = labels.iter().map(|l| (l.clone(), false)).collect();
    }

    /// The overlay `label` has loaded. Unknown labels (an earlier build) are ignored.
    pub fn mark_ready(&self, label: &str) {
        let mut labels = self.lock();
        if let Some(entry) = labels.iter_mut().find(|(l, _)| l == label) {
            entry.1 = true;
            self.changed.notify_all();
        }
    }

    /// Waits up to `timeout` for every overlay; returns whether all are ready.
    pub(super) fn wait_all(&self, timeout: Duration) -> bool {
        let labels = self.lock();
        let (labels, _) = self
            .changed
            .wait_timeout_while(labels, timeout, |labels| labels.iter().any(|(_, r)| !r))
            .unwrap_or_else(PoisonError::into_inner);
        labels.iter().all(|(_, ready)| *ready)
    }
}

struct Overlay {
    label: String,
    monitor: MonitorInfo,
}

#[derive(Default)]
struct Built {
    build: u32,
    fingerprint: Option<TopologyFingerprint>,
    overlays: Vec<Overlay>,
    /// The session shown, for `overlay://deactivate`.
    active: Option<SessionId>,
}

pub struct OverlayManager<R: Runtime> {
    app: AppHandle<R>,
    native: Arc<dyn OverlayNative>,
    readiness: Arc<OverlayReadiness>,
    built: Mutex<Built>,
}

impl<R: Runtime> OverlayManager<R> {
    pub fn new(
        app: AppHandle<R>,
        native: Arc<dyn OverlayNative>,
        readiness: Arc<OverlayReadiness>,
    ) -> Self {
        Self {
            app,
            native,
            readiness,
            built: Mutex::new(Built::default()),
        }
    }

    fn built(&self) -> MutexGuard<'_, Built> {
        self.built.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn build_overlay(&self, label: &str, monitor: &MonitorInfo) -> tauri::Result<WebviewWindow<R>> {
        let window =
            WebviewWindowBuilder::new(&self.app, label, WebviewUrl::App("index.html".into()))
                .title("Snipt")
                .visible(false)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .shadow(false)
                .focused(false)
                .build()?;
        // Position first: the size is then taken at the target monitor's DPI.
        let rect = monitor.rect;
        window.set_position(PhysicalPosition::new(rect.x, rect.y))?;
        window.set_size(PhysicalSize::new(rect.width, rect.height))?;
        if let Some(hwnd) = hwnd_of(&window) {
            self.native.prepare_overlay(hwnd);
        }
        Ok(window)
    }

    /// The overlay windows of the current build, waiting a little for those still loading.
    fn ready_windows(&self) -> Result<Vec<(WebviewWindow<R>, MonitorInfo)>, CaptureError> {
        let started = Instant::now();
        if !self.readiness.wait_all(READY_TIMEOUT) {
            log::warn!(
                "capture: overlays not loaded after {:?}, showing them anyway",
                started.elapsed()
            );
        }
        self.built()
            .overlays
            .iter()
            .map(|overlay| {
                let window = self
                    .app
                    .get_webview_window(&overlay.label)
                    .ok_or_else(|| unavailable(format!("{} is gone", overlay.label)))?;
                Ok((window, overlay.monitor.clone()))
            })
            .collect()
    }

    fn show_overlays(
        &self,
        session: SessionId,
        mode: OverlayMode,
        focus: MonitorIndex,
    ) -> Result<(), CaptureError> {
        let windows = self.ready_windows()?;
        if windows.is_empty() {
            return Err(unavailable("no overlay is built".to_owned()));
        }
        self.built().active = Some(session);
        let mut focused = None;
        for (window, monitor) in &windows {
            match hwnd_of(window) {
                Some(hwnd) => self.native.place_topmost(hwnd, monitor.rect),
                None => window.show().map_err(tauri_error)?,
            }
            let payload = OverlayActivate {
                session_id: session,
                mode,
                monitor: OverlayMonitor {
                    index: monitor.index,
                    rect: monitor.rect,
                    scale: monitor.scale,
                },
            };
            self.app
                .emit_to(
                    EventTarget::webview_window(window.label()),
                    ACTIVATE_EVENT,
                    payload,
                )
                .map_err(tauri_error)?;
            if monitor.index == focus {
                focused = Some(window);
            }
        }
        // The keyboard (Esc) goes to the cursor's monitor.
        if let Some(window) = focused.or(windows.first().map(|(w, _)| w)) {
            if let Err(error) = window.set_focus() {
                log::warn!("capture: could not focus {}: {error}", window.label());
            }
        }
        Ok(())
    }
}

impl<R: Runtime> OverlayPresenter for OverlayManager<R> {
    fn ensure_built(&self, topology: &MonitorTopology) -> Result<(), CaptureError> {
        let fingerprint = topology.fingerprint();
        let (build, old) = {
            let mut built = self.built();
            if built.fingerprint == Some(fingerprint)
                && built.overlays.len() == topology.monitors.len()
            {
                return Ok(());
            }
            built.build += 1;
            built.fingerprint = None;
            (built.build, std::mem::take(&mut built.overlays))
        };
        for overlay in old {
            if let Some(window) = self.app.get_webview_window(&overlay.label) {
                if let Err(error) = window.destroy() {
                    log::warn!("capture: could not destroy {}: {error}", overlay.label);
                }
            }
        }
        self.native.rebuild_backdrops(topology)?;
        let overlays: Vec<Overlay> = topology
            .monitors
            .iter()
            .map(|monitor| Overlay {
                label: overlay_label(build, monitor.index),
                monitor: monitor.clone(),
            })
            .collect();
        let labels: Vec<String> = overlays.iter().map(|o| o.label.clone()).collect();
        self.readiness.track(&labels);
        for overlay in &overlays {
            self.build_overlay(&overlay.label, &overlay.monitor)
                .map_err(tauri_error)?;
        }
        log::info!(
            "capture: built {} overlays ({})",
            overlays.len(),
            labels.join(", ")
        );
        let mut built = self.built();
        built.overlays = overlays;
        built.fingerprint = Some(fingerprint);
        Ok(())
    }

    fn show_region(
        &self,
        session: SessionId,
        frames: &[FrozenFrame],
        focus: MonitorIndex,
    ) -> Result<(), CaptureError> {
        // The frozen screen first, then the overlays above it.
        self.native.show_backdrops(frames)?;
        self.show_overlays(session, OverlayMode::Region, focus)
    }

    fn show_window_picker(
        &self,
        session: SessionId,
        focus: MonitorIndex,
    ) -> Result<(), CaptureError> {
        self.show_overlays(session, OverlayMode::Window, focus)
    }

    fn highlight(&self, session: SessionId, rect: Option<PhysicalRect>) {
        let labels: Vec<String> = self
            .built()
            .overlays
            .iter()
            .map(|o| o.label.clone())
            .collect();
        for label in labels {
            let payload = OverlayHighlight {
                session_id: session,
                rect,
            };
            if let Err(error) = self.app.emit_to(
                EventTarget::webview_window(&label),
                HIGHLIGHT_EVENT,
                payload,
            ) {
                log::warn!("capture: could not send the highlight to {label}: {error}");
            }
        }
    }

    fn hide_all(&self) {
        let (labels, active) = {
            let mut built = self.built();
            let labels: Vec<String> = built.overlays.iter().map(|o| o.label.clone()).collect();
            (labels, built.active.take())
        };
        for label in &labels {
            if let Some(session) = active {
                let payload = OverlayDeactivate {
                    session_id: session,
                };
                if let Err(error) = self.app.emit_to(
                    EventTarget::webview_window(label),
                    DEACTIVATE_EVENT,
                    payload,
                ) {
                    log::warn!("capture: could not deactivate {label}: {error}");
                }
            }
            if let Some(window) = self.app.get_webview_window(label) {
                match hwnd_of(&window) {
                    Some(hwnd) => self.native.hide_overlay(hwnd),
                    None => {
                        if let Err(error) = window.hide() {
                            log::warn!("capture: could not hide {label}: {error}");
                        }
                    }
                }
            }
        }
        self.native.hide_backdrops();
    }

    fn own_hwnds(&self) -> Vec<isize> {
        let labels: Vec<String> = self
            .built()
            .overlays
            .iter()
            .map(|o| o.label.clone())
            .collect();
        let mut hwnds: Vec<isize> = labels
            .iter()
            .filter_map(|label| self.app.get_webview_window(label))
            .filter_map(|window| hwnd_of(&window))
            .collect();
        hwnds.extend(self.native.backdrop_hwnds());
        hwnds
    }
}

#[cfg(all(windows, not(test)))]
fn hwnd_of<R: Runtime>(window: &WebviewWindow<R>) -> Option<isize> {
    window.hwnd().ok().map(|hwnd| hwnd.0 as isize)
}

/// Snipt targets Windows only; elsewhere, and in tests (where the mock runtime hands out a
/// made-up handle), windows have no handle.
#[cfg(any(not(windows), test))]
fn hwnd_of<R: Runtime>(_window: &WebviewWindow<R>) -> Option<isize> {
    None
}

fn unavailable(reason: String) -> CaptureError {
    CaptureError::OverlayUnavailable { reason }
}

fn tauri_error(error: tauri::Error) -> CaptureError {
    unavailable(error.to_string())
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use crate::screen_capture::ports::fake::CallLog;

    /// Records the backdrop calls.
    #[derive(Default)]
    pub struct FakeNative {
        pub calls: CallLog,
    }

    impl OverlayNative for FakeNative {
        fn prepare_overlay(&self, hwnd: isize) {
            self.calls.lock().unwrap().push(format!("prepare {hwnd}"));
        }

        fn place_topmost(&self, hwnd: isize, rect: PhysicalRect) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("place {hwnd} {rect:?}"));
        }

        fn hide_overlay(&self, hwnd: isize) {
            self.calls.lock().unwrap().push(format!("hide {hwnd}"));
        }

        fn rebuild_backdrops(&self, topology: &MonitorTopology) -> Result<(), CaptureError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("rebuild_backdrops {}", topology.monitors.len()));
            Ok(())
        }

        fn show_backdrops(&self, frames: &[FrozenFrame]) -> Result<(), CaptureError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("show_backdrops {}", frames.len()));
            Ok(())
        }

        fn hide_backdrops(&self) {
            self.calls.lock().unwrap().push("hide_backdrops".to_owned());
        }

        fn backdrop_hwnds(&self) -> Vec<isize> {
            vec![900, 901]
        }
    }
}

#[cfg(test)]
mod tests;
