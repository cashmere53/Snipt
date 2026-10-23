//! Library events sent to every window. Receivers handle them idempotently by id.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{Emitter, Runtime};

use super::types::CaptureItem;
use crate::shared::CaptureId;

pub const ITEM_ADDED: &str = "library://item-added";
pub const ITEM_UPDATED: &str = "library://item-updated";
pub const ITEMS_REMOVED: &str = "library://items-removed";
pub const RELOADED: &str = "library://reloaded";

/// Why items left the history. Serialized as `"deleted" | "missingOnDisk"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemovedReason {
    Deleted,
    MissingOnDisk,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryEvent {
    /// A new capture, an unsaved capture saved later, or a Snipt PNG found in the folder.
    ItemAdded(CaptureItem),
    /// Annotations saved, thumbnail generated, or the annotated PNG removed outside Snipt.
    ItemUpdated(CaptureItem),
    ItemsRemoved {
        ids: Vec<CaptureId>,
        reason: RemovedReason,
    },
    /// The save folder changed and its history has been loaded.
    Reloaded { save_dir: PathBuf },
}

#[derive(Serialize)]
struct ItemPayload<'a> {
    item: &'a CaptureItem,
}

#[derive(Serialize)]
struct RemovedPayload<'a> {
    ids: &'a [CaptureId],
    reason: RemovedReason,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReloadedPayload<'a> {
    save_dir: &'a PathBuf,
}

impl LibraryEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ItemAdded(_) => ITEM_ADDED,
            Self::ItemUpdated(_) => ITEM_UPDATED,
            Self::ItemsRemoved { .. } => ITEMS_REMOVED,
            Self::Reloaded { .. } => RELOADED,
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        let value = match self {
            Self::ItemAdded(item) | Self::ItemUpdated(item) => {
                serde_json::to_value(ItemPayload { item })
            }
            Self::ItemsRemoved { ids, reason } => serde_json::to_value(RemovedPayload {
                ids,
                reason: *reason,
            }),
            Self::Reloaded { save_dir } => serde_json::to_value(ReloadedPayload { save_dir }),
        };
        value.expect("library event payloads always serialize")
    }

    /// Sends the event to every window. Failures are logged.
    pub fn emit<R: Runtime>(&self, emitter: &impl Emitter<R>) {
        if let Err(e) = emitter.emit(self.name(), self.payload()) {
            log::warn!("library: could not emit {}: {e}", self.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tauri::Listener;

    use super::*;
    use crate::capture_library::types::fixtures::{id, item};

    #[test]
    fn payload_shapes() {
        let added = LibraryEvent::ItemAdded(item("20261022_174150"));
        assert_eq!(added.name(), "library://item-added");
        assert_eq!(added.payload()["item"]["id"], "20261022_174150");

        let updated = LibraryEvent::ItemUpdated(item("20261022_174150"));
        assert_eq!(updated.name(), "library://item-updated");
        assert_eq!(updated.payload()["item"]["width"], 1920);

        let removed = LibraryEvent::ItemsRemoved {
            ids: vec![id("20261022_174150"), id("20261022_174150_02")],
            reason: RemovedReason::MissingOnDisk,
        };
        assert_eq!(removed.name(), "library://items-removed");
        assert_eq!(
            removed.payload(),
            json!({ "ids": ["20261022_174150", "20261022_174150_02"], "reason": "missingOnDisk" })
        );
        let deleted = LibraryEvent::ItemsRemoved {
            ids: vec![],
            reason: RemovedReason::Deleted,
        };
        assert_eq!(deleted.payload()["reason"], "deleted");

        let reloaded = LibraryEvent::Reloaded {
            save_dir: PathBuf::from("D:/shots"),
        };
        assert_eq!(reloaded.name(), "library://reloaded");
        assert_eq!(reloaded.payload(), json!({ "saveDir": "D:/shots" }));
    }

    #[test]
    fn emit_reaches_listeners() {
        let app = tauri::test::mock_app();
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = received.clone();
        app.listen_any(ITEMS_REMOVED, move |event| {
            sink.lock().unwrap().push(event.payload().to_owned());
        });

        LibraryEvent::ItemsRemoved {
            ids: vec![id("20261022_174150")],
            reason: RemovedReason::Deleted,
        }
        .emit(app.handle());

        let received = received.lock().unwrap();
        assert_eq!(received.len(), 1);
        let payload: serde_json::Value = serde_json::from_str(&received[0]).unwrap();
        assert_eq!(payload["ids"], json!(["20261022_174150"]));
    }
}
