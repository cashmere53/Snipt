//! Thin adapters over OS APIs. Feature modules depend on these, never on OS APIs directly.

// --- capture-library ---
pub mod image_formats;

#[cfg(windows)]
pub mod windows;
