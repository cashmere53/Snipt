//! Settings: validation, the `settings.json` file and the committed-settings store.

pub mod file;
pub mod store;
pub mod validate;

pub use file::{LoadOutcome, PersistError};
pub use store::SettingsStore;
