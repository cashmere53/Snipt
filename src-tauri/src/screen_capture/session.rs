//! The capture session, kept in memory only (screen-capture 3.1). Its frozen screens and
//! window list go away with it; nothing is written to disk.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::compose::FrozenFrame;
use super::topology::{MonitorTopology, TopologyFingerprint};
use super::window_pick::WindowCandidate;
use crate::shared::CaptureMode;

/// Identifies one capture, so that late messages from an earlier one are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub u64);

/// Hands out increasing session ids, starting at 1.
#[derive(Debug, Default)]
pub struct SessionIds(AtomicU64);

impl SessionIds {
    pub fn next(&self) -> SessionId {
        SessionId(self.0.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

#[derive(Debug)]
pub struct CaptureSession {
    pub id: SessionId,
    pub mode: CaptureMode,
    pub topology: MonitorTopology,
    pub fingerprint: TopologyFingerprint,
    /// Every monitor, frozen (region and fullscreen); each frame matches its monitor in
    /// `topology`.
    pub frozen: Option<Vec<FrozenFrame>>,
    /// The windows to pick from (window mode), topmost first.
    pub windows: Option<Vec<WindowCandidate>>,
    /// Index into `windows` of the window under the pointer.
    pub highlighted: Option<usize>,
    pub started_at: Instant,
    /// When the screen was frozen: the capture time of region and fullscreen captures.
    pub frozen_at: DateTime<Local>,
}

impl CaptureSession {
    pub fn new(
        id: SessionId,
        mode: CaptureMode,
        topology: MonitorTopology,
        started_at: Instant,
        frozen_at: DateTime<Local>,
    ) -> Self {
        let fingerprint = topology.fingerprint();
        Self {
            id,
            mode,
            topology,
            fingerprint,
            frozen: None,
            windows: None,
            highlighted: None,
            started_at,
            frozen_at,
        }
    }

    pub fn highlighted_window(&self) -> Option<&WindowCandidate> {
        self.windows.as_ref()?.get(self.highlighted?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_capture::geometry::PhysicalRect;
    use crate::screen_capture::topology::testing::mixed_dpi;

    #[test]
    fn session_ids_increase_from_one() {
        let ids = SessionIds::default();
        assert_eq!(ids.next(), SessionId(1));
        assert_eq!(ids.next(), SessionId(2));
        assert!(ids.next() > SessionId(2));
    }

    #[test]
    fn a_session_keeps_its_layout_fingerprint_and_highlighted_window() {
        let topology = mixed_dpi();
        let mut session = CaptureSession::new(
            SessionId(1),
            CaptureMode::Window,
            topology.clone(),
            Instant::now(),
            Local::now(),
        );
        assert_eq!(session.fingerprint, topology.fingerprint());
        assert!(session.highlighted_window().is_none());

        let window = WindowCandidate {
            hwnd: 7,
            rect: PhysicalRect::new(0, 0, 10, 10),
            title: "メモ帳".to_owned(),
            z: 0,
        };
        session.windows = Some(vec![window.clone()]);
        session.highlighted = Some(0);
        assert_eq!(session.highlighted_window(), Some(&window));
        session.highlighted = Some(3);
        assert!(session.highlighted_window().is_none());
    }
}
