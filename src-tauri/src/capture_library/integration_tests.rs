//! End-to-end scenarios across the library's parts (task 10.1): saving, annotating, re-editing,
//! deleting, keeping failed saves, and switching save folders, on temporary folders with a
//! recycle bin, an editor and an event sink that are fakes.
//!
//! Each scenario reopens the library where it matters, so the index written in the background
//! is part of what is checked.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Local, TimeZone};
use serde_json::{json, Value};

use super::layout::LibraryLayout;
use super::ports::{ClipboardImage, DesktopOps};
use super::*;
use crate::platform::image_formats::png_from_rgba;
use crate::shared::{CaptureId, CaptureMode, Notice, Notifier};

/// A recycle bin that really moves the files away, so they can be restored later.
struct TrashDesktop {
    trash: PathBuf,
    moved: Mutex<Vec<(PathBuf, PathBuf)>>,
    /// The PNG handed to the clipboard by each copy.
    copies: Mutex<Vec<Option<Vec<u8>>>>,
}

impl TrashDesktop {
    fn new(trash: PathBuf) -> Self {
        fs::create_dir_all(&trash).unwrap();
        Self {
            trash,
            moved: Mutex::new(Vec::new()),
            copies: Mutex::new(Vec::new()),
        }
    }

    /// Puts every recycled file back where it was ("元に戻す" in the recycle bin).
    fn restore_all(&self) {
        for (from, to) in self.moved.lock().unwrap().drain(..) {
            fs::rename(to, from).unwrap();
        }
    }
}

impl DesktopOps for Arc<TrashDesktop> {
    fn recycle(&self, paths: &[PathBuf]) -> Result<(), LibraryError> {
        let mut moved = self.moved.lock().unwrap();
        for path in paths {
            let to = self.trash.join(format!(
                "{}-{}",
                moved.len(),
                path.file_name().unwrap().to_string_lossy()
            ));
            fs::rename(path, &to).unwrap();
            moved.push((path.clone(), to));
        }
        Ok(())
    }

    fn reveal(&self, _path: &Path) -> Result<(), LibraryError> {
        Ok(())
    }

    fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), LibraryError> {
        self.copies
            .lock()
            .unwrap()
            .push(image.png.map(<[u8]>::to_vec));
        Ok(())
    }
}

#[derive(Default)]
struct Editors(Mutex<Vec<String>>);

impl EditorLauncher for Editors {
    fn open_editor(&self, id: &CaptureId) {
        self.0.lock().unwrap().push(format!("open {id}"));
    }

    fn close_editor(&self, id: &CaptureId) {
        self.0.lock().unwrap().push(format!("close {id}"));
    }
}

#[derive(Default)]
struct Notices(Mutex<Vec<Notice>>);

impl Notifier for Notices {
    fn notify(&self, notice: Notice) {
        self.0.lock().unwrap().push(notice);
    }
}

struct World {
    dir: tempfile::TempDir,
    desktop: Arc<TrashDesktop>,
    editors: Arc<Editors>,
    notices: Arc<Notices>,
    events: Arc<Mutex<Vec<LibraryEvent>>>,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let desktop = Arc::new(TrashDesktop::new(dir.path().join("RecycleBin")));
        Self {
            dir,
            desktop,
            editors: Arc::default(),
            notices: Arc::default(),
            events: Arc::default(),
        }
    }

    fn folder(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Opens a library on `<tmp>/<name>`, as the app does at startup.
    fn open(&self, name: &str) -> Arc<CaptureLibrary> {
        let editors = self.editors.clone();
        let events = self.events.clone();
        CaptureLibrary::open(LibraryDeps {
            save_dir: self.folder(name),
            desktop: Box::new(self.desktop.clone()),
            notifier: self.notices.clone(),
            events: Box::new(move |event| events.lock().unwrap().push(event)),
            editor: Box::new(move || Some(editors.clone() as Arc<dyn EditorLauncher>)),
            serve_thumbnails: Box::new(|_| {}),
        })
    }

    fn take_events(&self) -> Vec<LibraryEvent> {
        std::mem::take(&mut self.events.lock().unwrap())
    }

    /// Waits until the thumbnail of `id` has been announced.
    fn wait_for_thumbnail(&self, id: &CaptureId) -> CaptureItem {
        for _ in 0..500 {
            let found = self
                .events
                .lock()
                .unwrap()
                .iter()
                .find_map(|event| match event {
                    LibraryEvent::ItemUpdated(item)
                        if item.id == *id && item.thumbnail_path.is_some() =>
                    {
                        Some(item.clone())
                    }
                    _ => None,
                });
            if let Some(item) = found {
                return item;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no thumbnail for {id}: {:?}", self.events.lock().unwrap());
    }
}

fn capture(second: u32) -> NewCapture {
    NewCapture {
        image: CaptureImage::Rgba {
            width: 4,
            height: 3,
            pixels: [10, 20, 30, 255].repeat(12),
        },
        mode: CaptureMode::Region,
        captured_at: Local
            .with_ymd_and_hms(2026, 10, 22, 17, 41, second)
            .unwrap(),
        window_title: None,
    }
}

/// What the editor sends: the annotation document and the flattened image, tinted `red`.
fn annotation(label: &str, red: u8) -> (Value, Vec<u8>) {
    (
        json!({ "schemaVersion": 1, "shapes": [{ "type": "text", "text": label }] }),
        png_from_rgba(4, 3, &[red, 0, 0, 255].repeat(12)).unwrap(),
    )
}

fn ids(library: &CaptureLibrary) -> Vec<String> {
    library
        .list()
        .iter()
        .map(|item| item.id.as_str().to_owned())
        .collect()
}

/// Closes the library the way quitting does: the hook runs, then the index is written.
fn quit(library: Arc<CaptureLibrary>) -> Vec<String> {
    let check = library.before_exit();
    library.flush_index();
    drop(library);
    check.unsaved
}

#[test]
fn save_annotate_reopen_reedit_and_delete() {
    let world = World::new();
    let library = world.open("Snipt");

    // Capture: saved, listed first and announced, with a thumbnail shortly after.
    let first = library.save_capture(capture(50)).unwrap();
    let item = library.save_capture(capture(51)).unwrap();
    assert_eq!(ids(&library), [item.id.as_str(), first.id.as_str()]);
    assert!(world
        .take_events()
        .contains(&LibraryEvent::ItemAdded(item.clone())));
    let with_thumbnail = world.wait_for_thumbnail(&item.id);
    assert!(with_thumbnail.thumbnail_path.unwrap().is_file());

    // Edit: nothing to restore yet, then the editor saves twice.
    library.open_in_editor(&item.id).unwrap();
    let session = library.load_for_edit(&item.id).unwrap();
    assert_eq!((session.annotations, session.unsaved), (None, false));
    let original = fs::read(&item.original_path).unwrap();
    let (draft, draft_png) = annotation("下書き", 100);
    library
        .save_annotations(&item.id, draft, draft_png)
        .unwrap();
    let (document, flattened) = annotation("完成", 200);
    let saved = library
        .save_annotations(&item.id, document.clone(), flattened.clone())
        .unwrap();
    assert_eq!(fs::read(&item.original_path).unwrap(), original);
    assert_eq!(
        fs::read(saved.annotated_path.as_ref().unwrap()).unwrap(),
        flattened
    );

    // Copying from the history takes the annotated image.
    library.copy_to_clipboard(&item.id).unwrap();
    assert_eq!(
        world
            .desktop
            .copies
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .as_deref(),
        Some(&flattened[..])
    );

    // After a restart the annotations come back for re-editing.
    assert!(quit(library).is_empty());
    let library = world.open("Snipt");
    let listed = library.list();
    assert_eq!(listed[0].id, item.id);
    assert!(listed[0].has_annotations);
    assert_eq!(listed[0].annotated_path, saved.annotated_path);
    let session = library.load_for_edit(&item.id).unwrap();
    assert_eq!(session.annotations, Some(document));
    assert_eq!(session.warning, None);

    // Delete: the editor is closed first, every file goes to the recycle bin, once.
    world.take_events();
    let outcome = library.delete(std::slice::from_ref(&item.id)).unwrap();
    assert_eq!(outcome.removed, vec![item.id.clone()]);
    assert!(outcome.failed.is_empty());
    assert_eq!(
        world.editors.0.lock().unwrap().clone(),
        [format!("open {}", item.id), format!("close {}", item.id)]
    );
    let layout = LibraryLayout::new(world.folder("Snipt"));
    for path in [
        layout.original(&item.id),
        layout.annotated(&item.id),
        layout.annotations(&item.id),
        layout.thumbnail(&item.id),
    ] {
        assert!(!path.exists(), "{}", path.display());
    }
    assert_eq!(ids(&library), [first.id.as_str()]);
    // The folder watcher sees the files go but must not report them a second time.
    std::thread::sleep(super::watcher::DEBOUNCE * 2);
    let removals: Vec<_> = world
        .take_events()
        .into_iter()
        .filter(|event| matches!(event, LibraryEvent::ItemsRemoved { .. }))
        .collect();
    assert_eq!(
        removals,
        [LibraryEvent::ItemsRemoved {
            ids: vec![item.id.clone()],
            reason: RemovedReason::Deleted,
        }]
    );

    // Still gone after a restart; restored from the recycle bin, it is back with its
    // annotations.
    assert!(quit(library).is_empty());
    let library = world.open("Snipt");
    assert_eq!(ids(&library), [first.id.as_str()]);
    drop(library);
    world.desktop.restore_all();
    let library = world.open("Snipt");
    assert_eq!(ids(&library), [item.id.as_str(), first.id.as_str()]);
    assert!(library.list()[0].has_annotations);
    assert_eq!(
        library.load_for_edit(&item.id).unwrap().annotations,
        Some(annotation("完成", 200).0)
    );
}

#[test]
fn a_failed_save_stays_editable_and_is_saved_later() {
    let world = World::new();
    // The save folder cannot be created while its parent is a file.
    fs::write(world.folder("Blocked"), b"not a folder").unwrap();
    let library = world.open("Blocked/Snipt");

    let err = library.save_capture(capture(50)).unwrap_err();
    let id = err.unsaved.id.clone();
    assert_eq!(err.reason.kind(), "saveDirUnavailable");
    assert_eq!(world.notices.0.lock().unwrap().len(), 1);
    assert!(library.list().is_empty());

    // The editor still opens it from memory, and saving annotations fails the same way.
    let session = library.load_for_edit(&id).unwrap();
    assert!(session.unsaved);
    assert!(session.warning.is_some());
    assert!(!library.read_original_png(&id).unwrap().is_empty());
    let (document, flattened) = annotation("保存待ち", 50);
    assert_eq!(
        library
            .save_annotations(&id, document.clone(), flattened.clone())
            .unwrap_err()
            .kind(),
        "saveDirUnavailable"
    );
    // Quitting now would lose it, so the exit check names it.
    assert_eq!(library.before_exit().unsaved, [format!("撮影画像: {id}")]);

    // Once the folder can be created, the next save writes the capture with its id kept.
    fs::remove_file(world.folder("Blocked")).unwrap();
    world.take_events();
    let saved = library
        .save_annotations(&id, document.clone(), flattened)
        .unwrap();
    assert_eq!(saved.id, id);
    assert!(saved.original_path.is_file());
    assert!(saved.has_annotations);
    assert!(matches!(
        world.take_events().first(),
        Some(LibraryEvent::ItemAdded(item)) if item.id == id
    ));
    assert!(!library.load_for_edit(&id).unwrap().unsaved);
    assert!(quit(library).is_empty());

    let library = world.open("Blocked/Snipt");
    assert_eq!(ids(&library), [id.as_str()]);
    assert_eq!(
        library.load_for_edit(&id).unwrap().annotations,
        Some(document)
    );
}

#[test]
fn unsaved_captures_are_written_by_the_exit_hook() {
    let world = World::new();
    fs::write(world.folder("Blocked"), b"not a folder").unwrap();
    let library = world.open("Blocked/Snipt");
    let id = library.save_capture(capture(50)).unwrap_err().unsaved.id;

    fs::remove_file(world.folder("Blocked")).unwrap();
    assert!(quit(library).is_empty());

    let library = world.open("Blocked/Snipt");
    assert_eq!(ids(&library), [id.as_str()]);
}

#[test]
fn switching_folders_and_back_keeps_each_history() {
    let world = World::new();
    let library = world.open("A");
    let in_a = library.save_capture(capture(50)).unwrap();
    let (document, flattened) = annotation("A の注釈", 150);
    library
        .save_annotations(&in_a.id, document.clone(), flattened)
        .unwrap();

    // Switch: the new folder starts empty and receives new captures; nothing moves.
    world.take_events();
    library.switch_save_dir(world.folder("B"));
    assert!(world.take_events().contains(&LibraryEvent::Reloaded {
        save_dir: world.folder("B"),
    }));
    assert!(library.list().is_empty());
    let in_b = library.save_capture(capture(51)).unwrap();
    assert!(in_b.original_path.starts_with(world.folder("B")));
    assert!(in_a.original_path.is_file());
    assert_eq!(ids(&library), [in_b.id.as_str()]);

    // Switching back shows A's history again, annotations included.
    library.switch_save_dir(world.folder("A"));
    assert_eq!(ids(&library), [in_a.id.as_str()]);
    assert!(library.list()[0].has_annotations);
    assert_eq!(
        library.load_for_edit(&in_a.id).unwrap().annotations,
        Some(document)
    );
    // A's watcher is active again: a file removed outside Snipt leaves the list.
    world.take_events();
    fs::remove_file(&in_a.original_path).unwrap();
    let removed = LibraryEvent::ItemsRemoved {
        ids: vec![in_a.id.clone()],
        reason: RemovedReason::MissingOnDisk,
    };
    for _ in 0..500 {
        if world.events.lock().unwrap().contains(&removed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(world.take_events().contains(&removed));
    assert!(library.list().is_empty());

    // B still has its own capture, also after a restart.
    library.switch_save_dir(world.folder("B"));
    assert_eq!(ids(&library), [in_b.id.as_str()]);
    assert!(quit(library).is_empty());
    let library = world.open("B");
    assert_eq!(ids(&library), [in_b.id.as_str()]);
}
