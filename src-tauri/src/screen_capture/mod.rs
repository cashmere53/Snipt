//! screen-capture: region, window and fullscreen captures, from the hotkey to the editor.
//!
//! The pure modules (`geometry`, `topology`, `compose`, `window_pick`) import neither Tauri nor
//! the `windows` crate; the OS is reached through `ports`, implemented in
//! `platform::windows`.

pub mod commands;
pub mod compose;
pub mod coordinator;
pub mod error;
pub mod geometry;
pub mod overlay;
pub mod ports;
pub mod post_capture;
pub mod session;
pub mod topology;
pub mod window_pick;
pub mod worker;

use std::sync::Arc;

use tauri::{App, AppHandle, Manager, Runtime};

use self::coordinator::{CaptureCoordinator, CoordinatorPorts};
use self::overlay::{OverlayManager, OverlayNative, OverlayReadiness};
use self::ports::CapturePlatform;
use self::post_capture::{PostCaptureFlow, SystemClipboard};
use self::worker::CaptureWorker;
use crate::capture_library::CaptureLibrary;
use crate::shared::{CaptureRequester, EditorLauncher, Notifier, SettingsStore};

/// The OS side of screen-capture; tests pass fakes.
pub struct SetupPorts {
    pub platform: Arc<dyn CapturePlatform>,
    pub native: Arc<dyn OverlayNative>,
    pub worker: Arc<dyn CaptureWorker>,
}

/// Manages `CaptureCoordinator` (also as `Arc<dyn CaptureRequester>` for app-shell's
/// hotkeys and tray) and `Arc<OverlayReadiness>` for the overlay commands, then builds the
/// overlays on the capture worker and starts watching the monitor layout.
pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: screen-capture");
    let ports = production_ports(app.handle())?;
    let coordinator = setup_with(app.handle(), ports);
    watch_displays(&coordinator);
    coordinator.prebuild_overlays();
    Ok(())
}

/// `setup` with the given OS side; returns the managed coordinator.
pub fn setup_with<R: Runtime>(app: &AppHandle<R>, ports: SetupPorts) -> CaptureCoordinator {
    let settings = app.state::<Arc<SettingsStore>>().inner().clone();
    let notifier = app.state::<Arc<dyn Notifier>>().inner().clone();
    let library = app.state::<Arc<CaptureLibrary>>().inner().clone();
    let editor = app.state::<Arc<dyn EditorLauncher>>().inner().clone();
    let readiness = Arc::new(OverlayReadiness::default());
    let overlays = OverlayManager::new(app.clone(), ports.native, readiness.clone());
    let post = PostCaptureFlow::new(Arc::new(SystemClipboard), library, editor, notifier.clone());
    let coordinator = CaptureCoordinator::new(CoordinatorPorts {
        platform: ports.platform,
        presenter: Arc::new(overlays),
        post: Arc::new(post),
        notifier,
        // Read at each fullscreen capture, so that a changed setting applies at once.
        fullscreen_target: Arc::new(move || settings.get().fullscreen_target),
        worker: ports.worker,
    });
    app.manage(coordinator.clone());
    app.manage(readiness);
    app.manage(Arc::new(coordinator.clone()) as Arc<dyn CaptureRequester>);
    coordinator
}

#[cfg(all(windows, not(test)))]
fn production_ports<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<SetupPorts, Box<dyn std::error::Error>> {
    use self::worker::ThreadWorker;
    use crate::platform::windows::backdrop::WinOverlayNative;
    use crate::platform::windows::capture_platform::{init_capture_thread, WinCapturePlatform};
    use crate::platform::windows::monitors::log_dpi_awareness_and_os_build;

    let worker = Arc::new(ThreadWorker::spawn(init_capture_thread)?);
    let platform = Arc::new(WinCapturePlatform::new());
    let startup_platform = platform.clone();
    worker.run(Box::new(move || {
        log_dpi_awareness_and_os_build();
        startup_platform.request_borderless();
    }));
    let main = app.clone();
    let native = WinOverlayNative::new(Arc::new(move |job| {
        main.run_on_main_thread(job).map_err(|e| e.to_string())
    }));
    Ok(SetupPorts {
        platform,
        native: Arc::new(native),
        worker,
    })
}

/// Snipt targets Windows only; elsewhere the crate builds for tests and captures fail. Tests
/// (which run `setup` on the mock runtime) never touch the real screen either.
#[cfg(any(not(windows), test))]
fn production_ports<R: Runtime>(
    _app: &AppHandle<R>,
) -> Result<SetupPorts, Box<dyn std::error::Error>> {
    Ok(SetupPorts {
        platform: Arc::new(unsupported::Platform),
        native: Arc::new(unsupported::Native),
        worker: Arc::new(worker::ThreadWorker::spawn(|| {})?),
    })
}

/// On the main thread: the coordinator hears of monitor layout changes.
#[cfg(all(windows, not(test)))]
fn watch_displays(coordinator: &CaptureCoordinator) {
    let coordinator = coordinator.clone();
    if let Err(error) =
        crate::platform::windows::display_watch::watch(move || coordinator.on_topology_changed())
    {
        log::error!("capture: cannot watch the monitor layout: {error}");
    }
}

#[cfg(any(not(windows), test))]
fn watch_displays(_coordinator: &CaptureCoordinator) {}

#[cfg(any(not(windows), test))]
mod unsupported {
    use std::time::Duration;

    use super::compose::{CapturedImage, FrozenFrame};
    use super::error::CaptureError;
    use super::geometry::{PhysicalPoint, PhysicalRect};
    use super::overlay::OverlayNative;
    use super::ports::CapturePlatform;
    use super::topology::MonitorTopology;
    use super::window_pick::WindowCandidate;

    fn unsupported() -> CaptureError {
        CaptureError::Internal("screen capture needs Windows".to_owned())
    }

    pub struct Platform;

    impl CapturePlatform for Platform {
        fn topology(&self) -> Result<MonitorTopology, CaptureError> {
            Err(unsupported())
        }

        fn cursor_position(&self) -> PhysicalPoint {
            PhysicalPoint::default()
        }

        fn grab_monitors(&self, _: &MonitorTopology) -> Result<Vec<FrozenFrame>, CaptureError> {
            Err(unsupported())
        }

        fn list_windows(&self, _: &[isize]) -> Vec<WindowCandidate> {
            Vec::new()
        }

        fn capture_window(&self, _: isize, _: Duration) -> Result<CapturedImage, CaptureError> {
            Err(unsupported())
        }
    }

    pub struct Native;

    impl OverlayNative for Native {
        fn prepare_overlay(&self, _: isize) {}

        fn place_topmost(&self, _: isize, _: PhysicalRect) {}

        fn rebuild_backdrops(&self, _: &MonitorTopology) -> Result<(), CaptureError> {
            Ok(())
        }

        fn show_backdrops(&self, _: &[FrozenFrame]) -> Result<(), CaptureError> {
            Ok(())
        }

        fn hide_backdrops(&self) {}

        fn backdrop_hwnds(&self) -> Vec<isize> {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod integration_tests;
