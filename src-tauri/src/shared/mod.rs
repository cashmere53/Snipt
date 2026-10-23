//! Types and traits shared across specs. Each type is owned by one spec
//! (see the design documents) and re-exported from here.

// --- app-shell ---
mod capture;
mod notify;
mod settings;

pub use crate::app_shell::lifecycle::{BeforeExitHooks, ExitCheck};
pub use crate::app_shell::settings::SettingsStore;
pub use capture::{CaptureMode, CaptureRequester};
pub use notify::{Notice, NoticeLevel, Notifier};
pub use settings::{Accelerator, FullscreenTarget, HotkeySettings, Settings};
// --- capture-library ---
mod capture_id;

pub use capture_id::{CaptureId, InvalidCaptureId};
// --- annotation-editor ---
mod editor_launcher;

pub use editor_launcher::EditorLauncher;
