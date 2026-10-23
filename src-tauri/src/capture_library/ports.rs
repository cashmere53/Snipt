//! `DesktopOps`: the recycle bin, Explorer and the clipboard behind one trait, so the library
//! can be tested with a recording fake.

use std::path::{Path, PathBuf};

use super::types::LibraryError;
pub use crate::platform::image_formats::ClipboardImage;

pub trait DesktopOps: Send + Sync + 'static {
    /// Moves every path to the recycle bin in one operation.
    fn recycle(&self, paths: &[PathBuf]) -> Result<(), LibraryError>;
    /// Opens Explorer with `path` selected.
    fn reveal(&self, path: &Path) -> Result<(), LibraryError>;
    fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), LibraryError>;
}

fn shell_error(message: String) -> LibraryError {
    LibraryError::Shell { message }
}

/// Production implementation: the Windows adapters in `platform::windows`.
pub struct SystemDesktop;

#[cfg(windows)]
impl DesktopOps for SystemDesktop {
    fn recycle(&self, paths: &[PathBuf]) -> Result<(), LibraryError> {
        crate::platform::windows::shell::recycle(paths).map_err(shell_error)
    }

    fn reveal(&self, path: &Path) -> Result<(), LibraryError> {
        crate::platform::windows::shell::reveal(path).map_err(shell_error)
    }

    fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), LibraryError> {
        use crate::platform::windows::clipboard::{copy_image, ClipboardError};
        copy_image(image).map_err(|e| LibraryError::Clipboard {
            message: match e {
                ClipboardError::Busy => "ほかのアプリがクリップボードを使用中です".to_owned(),
                ClipboardError::Win32(message) => message,
            },
        })
    }
}

/// Snipt targets Windows only; elsewhere the crate builds for tests and these calls fail.
#[cfg(not(windows))]
impl DesktopOps for SystemDesktop {
    fn recycle(&self, _paths: &[PathBuf]) -> Result<(), LibraryError> {
        Err(shell_error("unsupported platform".to_owned()))
    }

    fn reveal(&self, _path: &Path) -> Result<(), LibraryError> {
        Err(shell_error("unsupported platform".to_owned()))
    }

    fn copy_image(&self, _image: ClipboardImage<'_>) -> Result<(), LibraryError> {
        Err(LibraryError::Clipboard {
            message: "unsupported platform".to_owned(),
        })
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::sync::Mutex;

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum DesktopCall {
        Recycle(Vec<PathBuf>),
        Reveal(PathBuf),
        Copy {
            width: u32,
            height: u32,
            rgba: Vec<u8>,
            png: Option<Vec<u8>>,
        },
    }

    /// Records every call; `fail_with` makes the next calls fail.
    #[derive(Default)]
    pub struct RecordingDesktop {
        calls: Mutex<Vec<DesktopCall>>,
        failure: Mutex<Option<LibraryError>>,
    }

    impl RecordingDesktop {
        pub fn calls(&self) -> Vec<DesktopCall> {
            self.calls.lock().unwrap().clone()
        }

        pub fn fail_with(&self, error: Option<LibraryError>) {
            *self.failure.lock().unwrap() = error;
        }

        fn record(&self, call: DesktopCall) -> Result<(), LibraryError> {
            self.calls.lock().unwrap().push(call);
            match self.failure.lock().unwrap().clone() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
    }

    impl DesktopOps for RecordingDesktop {
        fn recycle(&self, paths: &[PathBuf]) -> Result<(), LibraryError> {
            self.record(DesktopCall::Recycle(paths.to_vec()))
        }

        fn reveal(&self, path: &Path) -> Result<(), LibraryError> {
            self.record(DesktopCall::Reveal(path.to_owned()))
        }

        fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), LibraryError> {
            self.record(DesktopCall::Copy {
                width: image.width,
                height: image.height,
                rgba: image.rgba.to_vec(),
                png: image.png.map(<[u8]>::to_vec),
            })
        }
    }

    /// Lets a test keep a handle to the fake the library owns.
    impl DesktopOps for std::sync::Arc<RecordingDesktop> {
        fn recycle(&self, paths: &[PathBuf]) -> Result<(), LibraryError> {
            RecordingDesktop::recycle(self, paths)
        }

        fn reveal(&self, path: &Path) -> Result<(), LibraryError> {
            RecordingDesktop::reveal(self, path)
        }

        fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), LibraryError> {
            RecordingDesktop::copy_image(self, image)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::fake::{DesktopCall, RecordingDesktop};
    use super::*;

    #[test]
    fn fake_records_calls_through_the_trait() {
        let fake = Arc::new(RecordingDesktop::default());
        let ops: Box<dyn DesktopOps> = Box::new(fake.clone());

        ops.recycle(&[PathBuf::from("a.png"), PathBuf::from("b.png")])
            .unwrap();
        ops.reveal(Path::new("a.png")).unwrap();
        ops.copy_image(ClipboardImage {
            width: 1,
            height: 1,
            rgba: &[1, 2, 3, 4],
            png: Some(&[9]),
        })
        .unwrap();

        assert_eq!(
            fake.calls(),
            vec![
                DesktopCall::Recycle(vec![PathBuf::from("a.png"), PathBuf::from("b.png")]),
                DesktopCall::Reveal(PathBuf::from("a.png")),
                DesktopCall::Copy {
                    width: 1,
                    height: 1,
                    rgba: vec![1, 2, 3, 4],
                    png: Some(vec![9]),
                },
            ]
        );
    }

    #[test]
    fn fake_failures_are_returned() {
        let fake = RecordingDesktop::default();
        fake.fail_with(Some(LibraryError::Shell {
            message: "in use".to_owned(),
        }));
        assert_eq!(fake.reveal(Path::new("a.png")).unwrap_err().kind(), "shell");
    }
}
