//! app-shell: tray, global hotkeys, settings, autostart, single instance and exit.
//!
//! app-shell must not depend on downstream modules (capture_library, screen_capture,
//! annotation_editor); downstream specs plug in through traits in `crate::shared`.

pub mod autostart;
pub mod commands;
pub mod dispatch;
pub mod hotkeys;
pub mod lifecycle;
pub mod notifier;
pub mod service;
pub mod settings;
pub mod startup;
pub mod tray;
pub mod windows;

pub use startup::{on_second_instance, on_shortcut, setup, start};
