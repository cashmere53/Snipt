//! `WinCapturePlatform`: the adapters above behind `CapturePlatform`
//! (screen-capture 5.8).

use std::time::Duration;

use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use super::monitors::{self, use_per_monitor_v2_on_this_thread};
use super::window_capture::WindowCapture;
use super::{screen_grab, window_enum};
use crate::screen_capture::compose::{CapturedImage, FrozenFrame};
use crate::screen_capture::error::CaptureError;
use crate::screen_capture::geometry::PhysicalPoint;
use crate::screen_capture::ports::CapturePlatform;
use crate::screen_capture::topology::MonitorTopology;
use crate::screen_capture::window_pick::WindowCandidate;

/// Prepares the capture worker thread: Per-Monitor v2 coordinates and the multithreaded
/// apartment that WGC and D3D11 need. Pass it as the worker's `init`.
pub fn init_capture_thread() {
    use_per_monitor_v2_on_this_thread();
    // SAFETY: plain call; the thread stays in the MTA until it ends.
    if let Err(error) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        log::error!("capture: could not initialize WinRT on the capture thread: {error}");
    }
}

#[derive(Default)]
pub struct WinCapturePlatform {
    windows: WindowCapture,
}

impl WinCapturePlatform {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for captures without the yellow border; call it once on the capture worker.
    pub fn request_borderless(&self) {
        self.windows.request_borderless();
    }
}

impl CapturePlatform for WinCapturePlatform {
    fn topology(&self) -> Result<MonitorTopology, CaptureError> {
        monitors::topology()
    }

    fn cursor_position(&self) -> PhysicalPoint {
        monitors::cursor_position()
    }

    fn grab_monitors(&self, topology: &MonitorTopology) -> Result<Vec<FrozenFrame>, CaptureError> {
        screen_grab::grab_monitors(topology)
    }

    fn list_windows(&self, exclude: &[isize]) -> Vec<WindowCandidate> {
        window_enum::list_windows(exclude)
    }

    fn capture_window(
        &self,
        hwnd: isize,
        timeout: Duration,
    ) -> Result<CapturedImage, CaptureError> {
        self.windows.capture(hwnd, timeout)
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    use super::*;

    /// On real hardware: run with `--ignored --nocapture`; every call goes through once.
    #[test]
    #[ignore = "needs a real desktop"]
    fn every_call_works_on_a_real_desktop() {
        init_capture_thread();
        let platform = WinCapturePlatform::new();
        platform.request_borderless();
        let topology = platform.topology().unwrap();
        println!(
            "{} monitors, cursor at {:?}",
            topology.monitors.len(),
            platform.cursor_position()
        );
        let frames = platform.grab_monitors(&topology).unwrap();
        assert_eq!(frames.len(), topology.monitors.len());
        let windows = platform.list_windows(&[]);
        println!("{} windows", windows.len());
        // SAFETY: plain call.
        let hwnd = unsafe { GetForegroundWindow() }.0 as isize;
        let image = platform
            .capture_window(hwnd, Duration::from_secs(1))
            .unwrap();
        println!("foreground window {}x{}", image.width, image.height);
    }
}
