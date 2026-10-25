//! After a capture (screen-capture 3.2): copy the image to the clipboard, save it, then open
//! it in the editor. A failed copy is reported and the rest goes on; a failed save is reported
//! by capture-library, and the editor opens the unsaved capture it keeps.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};

use super::compose::CapturedImage;
use crate::capture_library::{
    CaptureImage, CaptureItem, CaptureLibrary, NewCapture, SaveCaptureError,
};
use crate::shared::{CaptureId, CaptureMode, EditorLauncher, Notice, NoticeLevel, Notifier};

/// From the commit to the editor request; slower ones are logged as warnings.
const SLOW_POST_CAPTURE: Duration = Duration::from_secs(1);
const COPY_FAILED_TITLE: &str = "クリップボードへのコピーに失敗しました";

/// A capture the user committed.
pub struct CaptureOutcome {
    pub image: CapturedImage,
    pub mode: CaptureMode,
    /// When the screen was frozen (region, fullscreen) or the window captured.
    pub captured_at: DateTime<Local>,
    /// Window captures only.
    pub window_title: Option<String>,
    pub committed_at: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PostCaptureReport {
    pub copied: bool,
    /// `None` when saving failed (the editor opens the unsaved capture).
    pub saved: Option<CaptureId>,
    pub opened: CaptureId,
    pub elapsed: Duration,
}

/// The clipboard, replaceable in tests.
pub trait CaptureClipboard: Send + Sync {
    /// The error as text for the notice.
    fn copy(&self, image: &CapturedImage) -> Result<(), String>;
}

/// The capture store, replaceable in tests.
pub trait CaptureStore: Send + Sync {
    fn save_capture(&self, capture: NewCapture) -> Result<CaptureItem, SaveCaptureError>;
}

impl CaptureStore for CaptureLibrary {
    fn save_capture(&self, capture: NewCapture) -> Result<CaptureItem, SaveCaptureError> {
        CaptureLibrary::save_capture(self, capture)
    }
}

/// The Windows clipboard, as bitmap and PNG (made from the pixels).
pub struct SystemClipboard;

impl CaptureClipboard for SystemClipboard {
    #[cfg(windows)]
    fn copy(&self, image: &CapturedImage) -> Result<(), String> {
        use crate::platform::windows::clipboard::{copy_image, ClipboardImage};
        copy_image(ClipboardImage {
            width: image.width,
            height: image.height,
            rgba: &image.rgba,
            png: None,
        })
        .map_err(|e| e.to_string())
    }

    /// Snipt targets Windows only; elsewhere the crate builds for tests and copying fails.
    #[cfg(not(windows))]
    fn copy(&self, _image: &CapturedImage) -> Result<(), String> {
        Err("unsupported platform".to_owned())
    }
}

pub struct PostCaptureFlow {
    clipboard: Arc<dyn CaptureClipboard>,
    store: Arc<dyn CaptureStore>,
    editor: Arc<dyn EditorLauncher>,
    notifier: Arc<dyn Notifier>,
}

impl PostCaptureFlow {
    pub fn new(
        clipboard: Arc<dyn CaptureClipboard>,
        store: Arc<dyn CaptureStore>,
        editor: Arc<dyn EditorLauncher>,
        notifier: Arc<dyn Notifier>,
    ) -> Self {
        Self {
            clipboard,
            store,
            editor,
            notifier,
        }
    }

    pub fn run(&self, outcome: CaptureOutcome) -> PostCaptureReport {
        let CaptureOutcome {
            image,
            mode,
            captured_at,
            window_title,
            committed_at,
        } = outcome;
        let (width, height) = (image.width, image.height);

        let copied = match self.clipboard.copy(&image) {
            Ok(()) => true,
            Err(reason) => {
                log::warn!("capture: copy failed: {reason}");
                self.notifier.notify(Notice {
                    level: NoticeLevel::Warning,
                    title: COPY_FAILED_TITLE.to_owned(),
                    body: reason,
                });
                false
            }
        };
        let copied_at = committed_at.elapsed();

        // The pixels move into the store; they are not copied.
        let saved = self.store.save_capture(NewCapture {
            image: CaptureImage::Rgba {
                width,
                height,
                pixels: image.rgba,
            },
            mode,
            captured_at,
            window_title,
        });
        let saved_at = committed_at.elapsed();
        // capture-library has already told the user about a failed save.
        let (saved, opened) = match saved {
            Ok(item) => (Some(item.id.clone()), item.id),
            Err(error) => {
                log::warn!("capture: save failed, editing unsaved: {}", error.reason);
                (None, error.unsaved.id)
            }
        };

        self.editor.open_editor(&opened);
        let elapsed = committed_at.elapsed();
        // The window title stays out of the log.
        let timings = format!(
            "capture {opened} ({mode:?}, {width}x{height}): copied {copied_at:?}, \
             saved {saved_at:?}, editor requested {elapsed:?} after commit"
        );
        if elapsed > SLOW_POST_CAPTURE {
            log::warn!("{timings}");
        } else {
            log::info!("{timings}");
        }
        PostCaptureReport {
            copied,
            saved,
            opened,
            elapsed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::capture_library::types::fixtures::item;
    use crate::capture_library::LibraryError;

    type Log = Arc<Mutex<Vec<String>>>;

    struct FakeClipboard {
        log: Log,
        fail: bool,
    }

    impl CaptureClipboard for FakeClipboard {
        fn copy(&self, image: &CapturedImage) -> Result<(), String> {
            self.log
                .lock()
                .unwrap()
                .push(format!("copy {}x{}", image.width, image.height));
            if self.fail {
                Err("clipboard busy".to_owned())
            } else {
                Ok(())
            }
        }
    }

    struct FakeStore {
        log: Log,
        fail: bool,
        saved: Mutex<Vec<NewCapture>>,
    }

    impl CaptureStore for FakeStore {
        fn save_capture(&self, capture: NewCapture) -> Result<CaptureItem, SaveCaptureError> {
            self.log.lock().unwrap().push("save".to_owned());
            self.saved.lock().unwrap().push(capture);
            if self.fail {
                Err(SaveCaptureError {
                    reason: LibraryError::Io {
                        message: "disk full".to_owned(),
                    },
                    unsaved: Box::new(item("20261022_174151")),
                })
            } else {
                Ok(item("20261022_174150"))
            }
        }
    }

    struct FakeEditor(Log);

    impl EditorLauncher for FakeEditor {
        fn open_editor(&self, id: &CaptureId) {
            self.0.lock().unwrap().push(format!("open {id}"));
        }

        fn close_editor(&self, _id: &CaptureId) {}
    }

    struct FakeNotifier(Mutex<Vec<Notice>>);

    impl Notifier for FakeNotifier {
        fn notify(&self, notice: Notice) {
            self.0.lock().unwrap().push(notice);
        }
    }

    struct Fixture {
        log: Log,
        store: Arc<FakeStore>,
        notifier: Arc<FakeNotifier>,
        flow: PostCaptureFlow,
    }

    fn fixture(copy_fails: bool, save_fails: bool) -> Fixture {
        let log = Log::default();
        let store = Arc::new(FakeStore {
            log: log.clone(),
            fail: save_fails,
            saved: Mutex::default(),
        });
        let notifier = Arc::new(FakeNotifier(Mutex::default()));
        let flow = PostCaptureFlow::new(
            Arc::new(FakeClipboard {
                log: log.clone(),
                fail: copy_fails,
            }),
            store.clone(),
            Arc::new(FakeEditor(log.clone())),
            notifier.clone(),
        );
        Fixture {
            log,
            store,
            notifier,
            flow,
        }
    }

    fn outcome(mode: CaptureMode, title: Option<&str>) -> CaptureOutcome {
        CaptureOutcome {
            image: CapturedImage {
                width: 2,
                height: 1,
                rgba: vec![10, 20, 30, 255, 40, 50, 60, 255],
            },
            mode,
            captured_at: Local::now(),
            window_title: title.map(str::to_owned),
            committed_at: Instant::now(),
        }
    }

    #[test]
    fn copies_then_saves_then_opens_the_saved_capture() {
        let f = fixture(false, false);
        let report = f
            .flow
            .run(outcome(CaptureMode::Window, Some("メモ帳 - 無題")));
        assert_eq!(
            *f.log.lock().unwrap(),
            ["copy 2x1", "save", "open 20261022_174150"]
        );
        assert!(report.copied);
        assert_eq!(report.saved, Some(item("20261022_174150").id));
        assert_eq!(report.opened, item("20261022_174150").id);
        assert!(f.notifier.0.lock().unwrap().is_empty());

        let saved = f.store.saved.lock().unwrap();
        let capture = &saved[0];
        assert_eq!(capture.mode, CaptureMode::Window);
        assert_eq!(capture.window_title.as_deref(), Some("メモ帳 - 無題"));
        match &capture.image {
            CaptureImage::Rgba {
                width,
                height,
                pixels,
            } => {
                assert_eq!((*width, *height), (2, 1));
                assert_eq!(pixels, &[10, 20, 30, 255, 40, 50, 60, 255]);
            }
            CaptureImage::Png(_) => panic!("the pixels go to the store as RGBA"),
        }
    }

    #[test]
    fn a_failed_copy_is_reported_and_the_capture_still_saved_and_opened() {
        let f = fixture(true, false);
        let report = f.flow.run(outcome(CaptureMode::Region, None));
        assert_eq!(
            *f.log.lock().unwrap(),
            ["copy 2x1", "save", "open 20261022_174150"]
        );
        assert!(!report.copied);
        let notices = f.notifier.0.lock().unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].title, COPY_FAILED_TITLE);
        assert_eq!(notices[0].body, "clipboard busy");
    }

    #[test]
    fn a_failed_save_opens_the_unsaved_capture_without_a_second_notice() {
        let f = fixture(false, true);
        let report = f.flow.run(outcome(CaptureMode::Fullscreen, None));
        assert_eq!(
            *f.log.lock().unwrap(),
            ["copy 2x1", "save", "open 20261022_174151"]
        );
        assert_eq!(report.saved, None);
        assert_eq!(report.opened, item("20261022_174151").id);
        // capture-library notifies about the failed save.
        assert!(f.notifier.0.lock().unwrap().is_empty());
    }
}
