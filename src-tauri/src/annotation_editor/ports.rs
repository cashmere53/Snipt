//! `EditorDesktop`: the clipboard and the save dialog behind one trait, so the commands can be
//! tested with a recording fake.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

use super::error::EditorError;
pub use crate::platform::image_formats::ClipboardImage;

pub trait EditorDesktop: Send + Sync + 'static {
    fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), EditorError>;
    /// Shows the save dialog over the window `parent_label`; `None` when cancelled. Blocks, so
    /// never call it on the main thread.
    fn pick_save_path(
        &self,
        parent_label: &str,
        suggested_name: &str,
    ) -> Result<Option<PathBuf>, EditorError>;
}

/// Production implementation: the Windows clipboard adapter and tauri-plugin-dialog.
pub struct SystemEditorDesktop<R: Runtime> {
    pub app: AppHandle<R>,
}

impl<R: Runtime> EditorDesktop for SystemEditorDesktop<R> {
    #[cfg(windows)]
    fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), EditorError> {
        use crate::platform::windows::clipboard::{copy_image, ClipboardError};
        copy_image(image).map_err(|e| {
            EditorError::Clipboard(match e {
                ClipboardError::Busy => "ほかのアプリがクリップボードを使用中です".to_owned(),
                ClipboardError::Win32(message) => message,
            })
        })
    }

    /// Snipt targets Windows only; elsewhere the crate builds for tests and copying fails.
    #[cfg(not(windows))]
    fn copy_image(&self, _image: ClipboardImage<'_>) -> Result<(), EditorError> {
        Err(EditorError::Clipboard("unsupported platform".to_owned()))
    }

    fn pick_save_path(
        &self,
        parent_label: &str,
        suggested_name: &str,
    ) -> Result<Option<PathBuf>, EditorError> {
        let mut dialog = self
            .app
            .dialog()
            .file()
            .set_title("名前を付けて保存")
            .set_file_name(suggested_name)
            .add_filter("PNG 画像", &["png"])
            .add_filter("JPEG 画像", &["jpg", "jpeg"]);
        if let Some(parent) = self.app.get_webview_window(parent_label) {
            dialog = dialog.set_parent(&parent);
        }
        dialog
            .blocking_save_file()
            .map(|path| {
                path.into_path()
                    .map_err(|e| EditorError::Dialog(e.to_string()))
            })
            .transpose()
    }
}

#[cfg(test)]
pub mod fake {
    use std::sync::Mutex;

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct CopiedImage {
        pub width: u32,
        pub height: u32,
        pub rgba: Vec<u8>,
        pub png: Option<Vec<u8>>,
    }

    /// Records copies; the dialog answers with `dialog_answer`.
    #[derive(Default)]
    pub struct RecordingDesktop {
        pub copies: Mutex<Vec<CopiedImage>>,
        pub dialogs: Mutex<Vec<(String, String)>>,
        pub dialog_answer: Mutex<Option<PathBuf>>,
    }

    impl EditorDesktop for std::sync::Arc<RecordingDesktop> {
        fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), EditorError> {
            self.copies.lock().unwrap().push(CopiedImage {
                width: image.width,
                height: image.height,
                rgba: image.rgba.to_vec(),
                png: image.png.map(<[u8]>::to_vec),
            });
            Ok(())
        }

        fn pick_save_path(
            &self,
            parent_label: &str,
            suggested_name: &str,
        ) -> Result<Option<PathBuf>, EditorError> {
            self.dialogs
                .lock()
                .unwrap()
                .push((parent_label.to_owned(), suggested_name.to_owned()));
            Ok(self.dialog_answer.lock().unwrap().clone())
        }
    }
}
