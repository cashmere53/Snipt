//! `FolderWatcher`: notices Snipt PNGs removed or renamed outside Snipt (requirement 3.7).
//!
//! Only the save folder itself is watched (not `.snipt`). Events are collected until the folder
//! has been quiet for 300ms and reduced to the capture ids whose files were touched; the
//! library then checks which files are really gone, so stray or duplicate events do no harm.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::naming::{classify, SniptFile};
use crate::shared::CaptureId;

pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// Captures whose files may have disappeared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderChanges {
    pub originals: BTreeSet<CaptureId>,
    pub annotated: BTreeSet<CaptureId>,
}

impl FolderChanges {
    pub fn is_empty(&self) -> bool {
        self.originals.is_empty() && self.annotated.is_empty()
    }

    /// Adds the Snipt files among `paths`; other names are ignored.
    pub fn add_paths<'a>(&mut self, paths: impl IntoIterator<Item = &'a PathBuf>) {
        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            match classify(name) {
                Some(SniptFile::Original(id)) => self.originals.insert(id),
                Some(SniptFile::Annotated(id)) => self.annotated.insert(id),
                None => false,
            };
        }
    }
}

/// Whether an event can mean a file went away. Renames and removals are reported differently
/// per platform; anything unclassified is checked too.
fn may_remove(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Any
            | EventKind::Other
    )
}

pub type OnChanges = Box<dyn Fn(&Path, FolderChanges) + Send + Sync>;

/// Watches one folder until dropped.
pub struct FolderWatcher {
    root: PathBuf,
    _watcher: RecommendedWatcher,
}

impl FolderWatcher {
    /// Starts watching `root` (which must exist) and calls `on_changes` from a background
    /// thread after each quiet period.
    pub fn start(root: &Path, on_changes: OnChanges) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(tx)?;
        watcher.watch(root, RecursiveMode::NonRecursive)?;
        let thread_root = root.to_owned();
        std::thread::Builder::new()
            .name("snipt-folder-watcher".to_owned())
            .spawn(move || debounce_loop(&thread_root, &rx, &on_changes))
            .expect("spawning the folder watcher");
        Ok(Self {
            root: root.to_owned(),
            _watcher: watcher,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Ends when the watcher is dropped (its sender goes away).
fn debounce_loop(root: &Path, rx: &mpsc::Receiver<notify::Result<Event>>, on_changes: &OnChanges) {
    let mut pending = FolderChanges::default();
    loop {
        let received = if pending.is_empty() {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(DEBOUNCE)
        };
        match received {
            Ok(Ok(event)) if may_remove(&event.kind) => pending.add_paths(&event.paths),
            Ok(Ok(_)) => {}
            Ok(Err(e)) => log::warn!("library: watching {} failed: {e}", root.display()),
            Err(RecvTimeoutError::Timeout) => on_changes(root, std::mem::take(&mut pending)),
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use super::*;
    use crate::capture_library::types::fixtures::id;

    #[test]
    fn keeps_only_snipt_files() {
        let mut changes = FolderChanges::default();
        changes.add_paths(&[
            PathBuf::from("D:/shots/Snipt_20261022_174150.png"),
            PathBuf::from("D:/shots/Snipt_20261022_174151_annotated.png"),
            PathBuf::from("D:/shots/Snipt_20261022_174150.png"),
            PathBuf::from("D:/shots/other.png"),
            PathBuf::from("D:/shots/.snipt"),
        ]);
        assert_eq!(
            changes,
            FolderChanges {
                originals: [id("20261022_174150")].into(),
                annotated: [id("20261022_174151")].into(),
            }
        );
    }

    #[test]
    fn only_removal_like_events_count() {
        use notify::event::{CreateKind, RemoveKind, RenameMode};
        assert!(may_remove(&EventKind::Remove(RemoveKind::File)));
        assert!(may_remove(&EventKind::Modify(ModifyKind::Name(
            RenameMode::From
        ))));
        assert!(!may_remove(&EventKind::Create(CreateKind::File)));
    }

    #[test]
    fn reports_removed_files_after_a_quiet_period() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("Snipt_20261022_174150.png");
        let annotated = dir.path().join("Snipt_20261022_174150_annotated.png");
        std::fs::write(&original, b"x").unwrap();
        std::fs::write(&annotated, b"x").unwrap();
        let batches = Arc::new(Mutex::new(Vec::new()));
        let watcher = FolderWatcher::start(
            dir.path(),
            Box::new({
                let batches = batches.clone();
                move |root, changes| batches.lock().unwrap().push((root.to_owned(), changes))
            }),
        )
        .unwrap();
        assert_eq!(watcher.root(), dir.path());

        std::fs::remove_file(&original).unwrap();
        std::fs::remove_file(&annotated).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while batches.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let batches = batches.lock().unwrap().clone();
        assert_eq!(batches.len(), 1, "one debounced batch: {batches:?}");
        assert_eq!(batches[0].0, dir.path());
        assert_eq!(batches[0].1.originals, [id("20261022_174150")].into());
        assert_eq!(batches[0].1.annotated, [id("20261022_174150")].into());
    }
}
