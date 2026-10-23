//! Global hotkeys: accelerator handling and registration.

pub mod accelerator;
pub mod backend;
pub mod manager;

pub use backend::{BackendError, GlobalShortcutBackend, ShortcutBackend};
pub use manager::{HotkeyFailureKind, HotkeyManager, HotkeyStatus};
