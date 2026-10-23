use serde::{Deserialize, Serialize};

/// The three capture kinds. Serialized as `"region" | "window" | "fullscreen"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureMode {
    Region,
    Window,
    Fullscreen,
}

impl CaptureMode {
    pub const ALL: [CaptureMode; 3] = [
        CaptureMode::Region,
        CaptureMode::Window,
        CaptureMode::Fullscreen,
    ];
}

/// Implemented by screen-capture and managed as `Arc<dyn CaptureRequester>`.
pub trait CaptureRequester: Send + Sync + 'static {
    /// Must return immediately; it is called from hotkey and tray event threads.
    /// Requests that arrive while a capture is in progress are ignored by the implementation.
    fn request_capture(&self, mode: CaptureMode);
}
