//! `CaptureDispatch`: forwards capture requests from hotkeys and the tray to screen-capture.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use crate::shared::{CaptureMode, CaptureRequester};

pub struct CaptureDispatch<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> CaptureDispatch<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    /// Passes `mode` to the managed `Arc<dyn CaptureRequester>`, or logs a warning when none is
    /// registered. Ignoring requests during a capture is the requester's job.
    pub fn request(&self, mode: CaptureMode) {
        match self.app.try_state::<Arc<dyn CaptureRequester>>() {
            Some(requester) => requester.request_capture(mode),
            None => log::warn!("capture: no CaptureRequester registered; ignoring {mode:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<CaptureMode>>);

    impl CaptureRequester for Recorder {
        fn request_capture(&self, mode: CaptureMode) {
            self.0.lock().unwrap().push(mode);
        }
    }

    #[test]
    fn without_requester_only_logs() {
        let app = tauri::test::mock_app();
        CaptureDispatch::new(app.handle().clone()).request(CaptureMode::Region);
    }

    #[test]
    fn forwards_the_mode_once() {
        let app = tauri::test::mock_app();
        let recorder = Arc::new(Recorder::default());
        let requester: Arc<dyn CaptureRequester> = recorder.clone();
        app.manage(requester);

        CaptureDispatch::new(app.handle().clone()).request(CaptureMode::Window);

        assert_eq!(*recorder.0.lock().unwrap(), vec![CaptureMode::Window]);
    }
}
