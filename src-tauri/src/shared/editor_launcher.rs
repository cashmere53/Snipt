//! `EditorLauncher`: opens and closes annotation editor windows. Defined and implemented by
//! annotation-editor (managed as `Arc<dyn EditorLauncher>`); screen-capture and
//! capture-library call it.

use super::CaptureId;

pub trait EditorLauncher: Send + Sync + 'static {
    /// Brings the capture's editor to the front, or opens one. Does not block the caller.
    fn open_editor(&self, id: &CaptureId);
    /// Saves unsaved annotations and closes the capture's editor if it is open. Blocks until it
    /// is closed or 3 seconds have passed, so never call it from the main thread. Returns at
    /// once when no editor is open.
    fn close_editor(&self, id: &CaptureId);
}
