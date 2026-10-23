//! `UnsavedStore`: captures that could not be written to the save folder, kept in memory so
//! the editor can still open them (requirement 1.6). Lost when the process ends, and never
//! part of the history list.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::types::{CaptureItem, LibraryError};
use crate::shared::CaptureId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsavedCapture {
    pub item: CaptureItem,
    /// The encoded original, metadata included, ready to be written again.
    pub png: Arc<Vec<u8>>,
    /// Why the save failed.
    pub reason: LibraryError,
}

#[derive(Debug, Default)]
pub struct UnsavedStore {
    captures: Mutex<HashMap<CaptureId, UnsavedCapture>>,
}

impl UnsavedStore {
    /// Keeps `capture`, replacing one with the same id.
    pub fn insert(&self, capture: UnsavedCapture) {
        self.captures
            .lock()
            .unwrap()
            .insert(capture.item.id.clone(), capture);
    }

    pub fn get(&self, id: &CaptureId) -> Option<UnsavedCapture> {
        self.captures.lock().unwrap().get(id).cloned()
    }

    pub fn contains(&self, id: &CaptureId) -> bool {
        self.captures.lock().unwrap().contains_key(id)
    }

    /// All kept captures, oldest id first.
    pub fn list(&self) -> Vec<UnsavedCapture> {
        let mut captures: Vec<_> = self.captures.lock().unwrap().values().cloned().collect();
        captures.sort_by(|a, b| a.item.id.cmp(&b.item.id));
        captures
    }

    /// Forgets a capture once it was saved after all, or deleted.
    pub fn remove(&self, id: &CaptureId) -> Option<UnsavedCapture> {
        self.captures.lock().unwrap().remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture_library::types::fixtures::{id, item};

    fn unsaved(id_str: &str) -> UnsavedCapture {
        UnsavedCapture {
            item: item(id_str),
            png: Arc::new(vec![1, 2, 3]),
            reason: LibraryError::DiskFull {
                message: "full".to_owned(),
            },
        }
    }

    #[test]
    fn keeps_gets_lists_and_removes() {
        let store = UnsavedStore::default();
        store.insert(unsaved("20261022_174151"));
        store.insert(unsaved("20261022_174150"));

        let a = id("20261022_174150");
        assert!(store.contains(&a));
        assert_eq!(store.get(&a), Some(unsaved("20261022_174150")));
        assert_eq!(
            store
                .list()
                .iter()
                .map(|c| c.item.id.as_str())
                .collect::<Vec<_>>(),
            ["20261022_174150", "20261022_174151"]
        );

        assert_eq!(store.remove(&a), Some(unsaved("20261022_174150")));
        assert_eq!(store.remove(&a), None);
        assert_eq!(store.get(&a), None);
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn insert_replaces_the_same_id() {
        let store = UnsavedStore::default();
        store.insert(unsaved("20261022_174150"));
        let mut again = unsaved("20261022_174150");
        again.reason = LibraryError::PermissionDenied {
            path: "D:/".into(),
            message: "denied".to_owned(),
        };
        store.insert(again.clone());
        assert_eq!(store.list(), [again]);
    }
}
