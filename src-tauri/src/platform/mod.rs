//! Thin adapters over OS APIs. Feature modules depend on these, never on OS APIs directly.

#[cfg(windows)]
pub mod windows;
