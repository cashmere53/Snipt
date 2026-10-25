//! The overlay windows' commands (screen-capture 6.3). Each one checks that it comes from an
//! `overlay-*` window, then hands over to the coordinator. Coordinates are local physical
//! pixels of the calling overlay's monitor.

use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{Runtime, State, Webview};

use super::coordinator::{CaptureCoordinator, MagnifierQuery};
use super::error::{CaptureError, CommitError};
use super::geometry::{PhysicalPoint, PhysicalRect};
use super::overlay::{is_overlay_label, OverlayReadiness};
use super::session::SessionId;
use super::topology::MonitorIndex;

/// `OverlayCommandError` in `src/shared/ipc/screen-capture.ts`: `{ "kind": "tooSmall" }` and
/// so on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OverlayCommandError {
    #[error("the selection is smaller than one pixel")]
    TooSmall,
    #[error("no window is highlighted")]
    NoWindowHighlighted,
    #[error("the capture session has ended")]
    StaleSession,
    #[error("the monitor layout changed")]
    TopologyChanged,
    #[error("the point is outside the frozen screens")]
    OutOfRange,
    /// The caller is not an overlay window.
    #[error("not an overlay window")]
    NotAnOverlay,
}

impl From<CommitError> for OverlayCommandError {
    fn from(error: CommitError) -> Self {
        match error {
            CommitError::TooSmall => Self::TooSmall,
            CommitError::NoWindowHighlighted => Self::NoWindowHighlighted,
            CommitError::StaleSession => Self::StaleSession,
            CommitError::TopologyChanged => Self::TopologyChanged,
        }
    }
}

impl From<CaptureError> for OverlayCommandError {
    fn from(error: CaptureError) -> Self {
        match error {
            CaptureError::OutOfRange => Self::OutOfRange,
            CaptureError::TopologyChanged => Self::TopologyChanged,
            // The selection-time requests fail only for an ended session or a point outside.
            _ => Self::StaleSession,
        }
    }
}

fn caller<R: Runtime>(webview: &Webview<R>) -> Result<(), OverlayCommandError> {
    if is_overlay_label(webview.label()) {
        Ok(())
    } else {
        log::warn!("capture: overlay command from {}", webview.label());
        Err(OverlayCommandError::NotAnOverlay)
    }
}

/// The overlay has loaded its page.
#[tauri::command]
pub async fn capture_overlay_ready<R: Runtime>(
    webview: Webview<R>,
    readiness: State<'_, Arc<OverlayReadiness>>,
) -> Result<(), OverlayCommandError> {
    caller(&webview)?;
    readiness.mark_ready(webview.label());
    Ok(())
}

/// The overlay has drawn its first frame of the session; logs the latency.
#[tauri::command]
pub async fn capture_overlay_shown<R: Runtime>(
    webview: Webview<R>,
    coordinator: State<'_, CaptureCoordinator>,
    session_id: SessionId,
    monitor_index: MonitorIndex,
) -> Result<(), OverlayCommandError> {
    caller(&webview)?;
    coordinator.on_overlay_shown(session_id, monitor_index);
    Ok(())
}

/// The frozen pixels around a point, as raw RGBA bytes (`(2r + 1)²` pixels).
#[tauri::command]
pub async fn capture_overlay_magnifier<R: Runtime>(
    webview: Webview<R>,
    coordinator: State<'_, CaptureCoordinator>,
    session_id: SessionId,
    monitor_index: MonitorIndex,
    x: i32,
    y: i32,
    radius: u32,
) -> Result<Response, OverlayCommandError> {
    caller(&webview)?;
    let patch = coordinator.magnifier_patch(MagnifierQuery {
        session: session_id,
        monitor: monitor_index,
        x,
        y,
        radius,
    })?;
    Ok(Response::new(patch.rgba))
}

/// Window mode: the pointer moved; the highlight comes back as `overlay://highlight`.
#[tauri::command]
pub async fn capture_overlay_pointer<R: Runtime>(
    webview: Webview<R>,
    coordinator: State<'_, CaptureCoordinator>,
    session_id: SessionId,
    monitor_index: MonitorIndex,
    x: i32,
    y: i32,
) -> Result<(), OverlayCommandError> {
    caller(&webview)?;
    coordinator.pointer_on_monitor(session_id, monitor_index, PhysicalPoint::new(x, y))?;
    Ok(())
}

#[tauri::command]
pub async fn capture_overlay_commit_region<R: Runtime>(
    webview: Webview<R>,
    coordinator: State<'_, CaptureCoordinator>,
    session_id: SessionId,
    monitor_index: MonitorIndex,
    rect: PhysicalRect,
) -> Result<(), OverlayCommandError> {
    caller(&webview)?;
    coordinator.commit_region(session_id, monitor_index, rect)?;
    Ok(())
}

#[tauri::command]
pub async fn capture_overlay_commit_window<R: Runtime>(
    webview: Webview<R>,
    coordinator: State<'_, CaptureCoordinator>,
    session_id: SessionId,
) -> Result<(), OverlayCommandError> {
    caller(&webview)?;
    coordinator.commit_window(session_id)?;
    Ok(())
}

/// Esc or a right click.
#[tauri::command]
pub async fn capture_overlay_cancel<R: Runtime>(
    webview: Webview<R>,
    coordinator: State<'_, CaptureCoordinator>,
    session_id: SessionId,
) -> Result<(), OverlayCommandError> {
    caller(&webview)?;
    coordinator.cancel(session_id);
    Ok(())
}

/// Every command here, as `build.rs` and `capabilities/overlay.json` list them.
pub const COMMANDS: [&str; 7] = [
    "capture_overlay_ready",
    "capture_overlay_shown",
    "capture_overlay_magnifier",
    "capture_overlay_pointer",
    "capture_overlay_commit_region",
    "capture_overlay_commit_window",
    "capture_overlay_cancel",
];

#[cfg(test)]
mod tests;
