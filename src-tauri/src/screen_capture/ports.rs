//! The two ports of the coordinator (screen-capture 3.1): `CapturePlatform` reaches the OS
//! (`platform::windows` implements it) and `OverlayPresenter` the screen (`overlay`
//! implements it). Tests use the recording fakes in `fake`.

use std::time::Duration;

use super::compose::{CapturedImage, FrozenFrame};
use super::error::CaptureError;
use super::geometry::{PhysicalPoint, PhysicalRect};
use super::session::SessionId;
use super::topology::{MonitorIndex, MonitorTopology};
use super::window_pick::WindowCandidate;

pub trait CapturePlatform: Send + Sync {
    fn topology(&self) -> Result<MonitorTopology, CaptureError>;
    /// Global physical pixels.
    fn cursor_position(&self) -> PhysicalPoint;
    /// One frame per monitor of `topology`, in its order. The cursor is not drawn.
    fn grab_monitors(&self, topology: &MonitorTopology) -> Result<Vec<FrozenFrame>, CaptureError>;
    /// The capturable windows, topmost first, without the `exclude` windows (Snipt's own).
    fn list_windows(&self, exclude: &[isize]) -> Vec<WindowCandidate>;
    /// One window's image, giving up after `timeout`.
    fn capture_window(&self, hwnd: isize, timeout: Duration)
        -> Result<CapturedImage, CaptureError>;
}

pub trait OverlayPresenter: Send + Sync {
    /// Builds an overlay per monitor of `topology` unless they already match it.
    fn ensure_built(&self, topology: &MonitorTopology) -> Result<(), CaptureError>;
    /// Shows the frozen screens under the overlays, for selecting a region; `focus` gets the
    /// keyboard.
    fn show_region(
        &self,
        session: SessionId,
        frames: &[FrozenFrame],
        focus: MonitorIndex,
    ) -> Result<(), CaptureError>;
    /// Shows the overlays over the live screen, for picking a window.
    fn show_window_picker(
        &self,
        session: SessionId,
        focus: MonitorIndex,
    ) -> Result<(), CaptureError>;
    /// The frame of the window under the pointer (global), or none.
    fn highlight(&self, session: SessionId, rect: Option<PhysicalRect>);
    fn hide_all(&self);
    /// The overlay and backdrop windows, which window mode must not offer.
    fn own_hwnds(&self) -> Vec<isize>;
}

#[cfg(test)]
pub mod fake {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::screen_capture::topology::testing::mixed_dpi;

    /// Calls in order, as text: `"grab_monitors"`, `"show_region 1 focus 0"`…
    pub type CallLog = Arc<Mutex<Vec<String>>>;

    fn record(log: &CallLog, call: impl Into<String>) {
        log.lock().unwrap().push(call.into());
    }

    /// A gray frame of `rect`'s size whose pixels are BGRA `[index, 0x80, 0x80, 0]`.
    pub fn solid_frame(index: MonitorIndex, rect: PhysicalRect) -> FrozenFrame {
        let stride = rect.width as usize * 4;
        let pixel = [index.0 as u8, 0x80, 0x80, 0];
        FrozenFrame {
            monitor: index,
            rect,
            stride,
            bgra: pixel
                .repeat(rect.width as usize * rect.height as usize)
                .into(),
        }
    }

    /// The OS: answers from its fields, records every call, fails when told to.
    pub struct FakePlatform {
        pub calls: CallLog,
        pub topology: Mutex<Result<MonitorTopology, CaptureError>>,
        pub cursor: Mutex<PhysicalPoint>,
        pub grab_error: Mutex<Option<CaptureError>>,
        pub windows: Mutex<Vec<WindowCandidate>>,
        pub window_image: Mutex<Result<CapturedImage, CaptureError>>,
    }

    impl Default for FakePlatform {
        /// The mixed-DPI layout, the cursor on the primary monitor, no windows, and a 2 × 1
        /// window image.
        fn default() -> Self {
            Self {
                calls: CallLog::default(),
                topology: Mutex::new(Ok(mixed_dpi())),
                cursor: Mutex::new(PhysicalPoint::new(100, 100)),
                grab_error: Mutex::new(None),
                windows: Mutex::new(Vec::new()),
                window_image: Mutex::new(Ok(CapturedImage {
                    width: 2,
                    height: 1,
                    rgba: vec![1, 2, 3, 255, 4, 5, 6, 255],
                })),
            }
        }
    }

    impl CapturePlatform for FakePlatform {
        fn topology(&self) -> Result<MonitorTopology, CaptureError> {
            record(&self.calls, "topology");
            self.topology.lock().unwrap().clone()
        }

        fn cursor_position(&self) -> PhysicalPoint {
            *self.cursor.lock().unwrap()
        }

        fn grab_monitors(
            &self,
            topology: &MonitorTopology,
        ) -> Result<Vec<FrozenFrame>, CaptureError> {
            record(&self.calls, "grab_monitors");
            if let Some(error) = self.grab_error.lock().unwrap().clone() {
                return Err(error);
            }
            Ok(topology
                .monitors
                .iter()
                .map(|m| solid_frame(m.index, m.rect))
                .collect())
        }

        fn list_windows(&self, exclude: &[isize]) -> Vec<WindowCandidate> {
            record(&self.calls, format!("list_windows exclude {exclude:?}"));
            let windows = self.windows.lock().unwrap();
            windows
                .iter()
                .filter(|w| !exclude.contains(&w.hwnd))
                .cloned()
                .collect()
        }

        fn capture_window(
            &self,
            hwnd: isize,
            _timeout: Duration,
        ) -> Result<CapturedImage, CaptureError> {
            record(&self.calls, format!("capture_window {hwnd}"));
            self.window_image.lock().unwrap().clone()
        }
    }

    /// The screen: records every call; `show_*` fail while `fail_show` is set.
    #[derive(Default)]
    pub struct FakePresenter {
        pub calls: CallLog,
        pub fail_show: Mutex<Option<CaptureError>>,
        pub own: Vec<isize>,
    }

    impl FakePresenter {
        fn shown(&self, call: String) -> Result<(), CaptureError> {
            record(&self.calls, call);
            match self.fail_show.lock().unwrap().clone() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
    }

    impl OverlayPresenter for FakePresenter {
        fn ensure_built(&self, topology: &MonitorTopology) -> Result<(), CaptureError> {
            record(
                &self.calls,
                format!("ensure_built {}", topology.monitors.len()),
            );
            Ok(())
        }

        fn show_region(
            &self,
            session: SessionId,
            frames: &[FrozenFrame],
            focus: MonitorIndex,
        ) -> Result<(), CaptureError> {
            self.shown(format!(
                "show_region {} frames {} focus {}",
                session.0,
                frames.len(),
                focus.0
            ))
        }

        fn show_window_picker(
            &self,
            session: SessionId,
            focus: MonitorIndex,
        ) -> Result<(), CaptureError> {
            self.shown(format!(
                "show_window_picker {} focus {}",
                session.0, focus.0
            ))
        }

        fn highlight(&self, session: SessionId, rect: Option<PhysicalRect>) {
            record(&self.calls, format!("highlight {} {rect:?}", session.0));
        }

        fn hide_all(&self) {
            record(&self.calls, "hide_all");
        }

        fn own_hwnds(&self) -> Vec<isize> {
            self.own.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::fake::{FakePlatform, FakePresenter};
    use super::*;

    #[test]
    fn the_fakes_answer_record_and_fail_on_demand() {
        let platform = Arc::new(FakePlatform::default());
        let ports: Arc<dyn CapturePlatform> = platform.clone();
        let topology = ports.topology().unwrap();
        let frames = ports.grab_monitors(&topology).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].rect, topology.monitors[1].rect);
        *platform.grab_error.lock().unwrap() = Some(CaptureError::TopologyChanged);
        assert_eq!(
            ports.grab_monitors(&topology).unwrap_err(),
            CaptureError::TopologyChanged
        );
        assert_eq!(
            *platform.calls.lock().unwrap(),
            ["topology", "grab_monitors", "grab_monitors"]
        );

        let presenter = FakePresenter {
            own: vec![9],
            ..FakePresenter::default()
        };
        let screen: &dyn OverlayPresenter = &presenter;
        screen
            .show_region(SessionId(3), &frames, MonitorIndex(1))
            .unwrap();
        *presenter.fail_show.lock().unwrap() = Some(CaptureError::OverlayUnavailable {
            reason: "gone".to_owned(),
        });
        assert!(screen
            .show_window_picker(SessionId(4), MonitorIndex(0))
            .is_err());
        screen.hide_all();
        assert_eq!(screen.own_hwnds(), [9]);
        assert_eq!(
            *presenter.calls.lock().unwrap(),
            [
                "show_region 3 frames 2 focus 1",
                "show_window_picker 4 focus 0",
                "hide_all"
            ]
        );
    }
}
