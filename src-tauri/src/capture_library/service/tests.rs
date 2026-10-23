//! Service tests on temporary folders with a recording desktop, notifier, editor and event
//! sink.

use std::sync::OnceLock;
use std::time::Duration;

use chrono::TimeZone;
use serde_json::json;

use super::*;
use crate::capture_library::index::IndexFileV1;
use crate::capture_library::ports::fake::{DesktopCall, RecordingDesktop};
use crate::capture_library::types::fixtures::id;
use crate::platform::image_formats::png_from_rgba;
use crate::shared::CaptureMode;

#[derive(Default)]
struct Notices(Mutex<Vec<Notice>>);

impl Notifier for Notices {
    fn notify(&self, notice: Notice) {
        self.0.lock().unwrap().push(notice);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EditorCall {
    Open(CaptureId),
    /// Whether the library's state lock was free when the editor was closed.
    Close {
        id: CaptureId,
        lock_free: bool,
    },
}

#[derive(Default)]
struct FakeEditor {
    calls: Mutex<Vec<EditorCall>>,
    library: OnceLock<Weak<CaptureLibrary>>,
}

impl EditorLauncher for FakeEditor {
    fn open_editor(&self, id: &CaptureId) {
        self.calls
            .lock()
            .unwrap()
            .push(EditorCall::Open(id.clone()));
    }

    fn close_editor(&self, id: &CaptureId) {
        let library = self.library.get().and_then(Weak::upgrade).unwrap();
        let lock_free = library.state.try_write().is_ok();
        self.calls.lock().unwrap().push(EditorCall::Close {
            id: id.clone(),
            lock_free,
        });
    }
}

struct Harness {
    _dir: tempfile::TempDir,
    root: PathBuf,
    library: Arc<CaptureLibrary>,
    desktop: Arc<RecordingDesktop>,
    notices: Arc<Notices>,
    events: Arc<Mutex<Vec<LibraryEvent>>>,
    editor: Arc<FakeEditor>,
}

impl Harness {
    /// A library on `<tmp>/<relative>`, created lazily by the first save.
    fn at(relative: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(relative);
        let desktop = Arc::new(RecordingDesktop::default());
        let notices = Arc::new(Notices::default());
        let events = Arc::new(Mutex::new(Vec::new()));
        let editor = Arc::new(FakeEditor::default());
        let library = CaptureLibrary::open(LibraryDeps {
            save_dir: root.clone(),
            desktop: Box::new(desktop.clone()),
            notifier: notices.clone(),
            events: Box::new({
                let events = events.clone();
                move |event| events.lock().unwrap().push(event)
            }),
            editor: Box::new({
                let editor = editor.clone();
                move || Some(editor.clone() as Arc<dyn EditorLauncher>)
            }),
        });
        editor.library.set(Arc::downgrade(&library)).unwrap();
        Self {
            _dir: dir,
            root,
            library,
            desktop,
            notices,
            events,
            editor,
        }
    }

    fn new() -> Self {
        Self::at("Pictures/Snipt")
    }

    fn layout(&self) -> LibraryLayout {
        LibraryLayout::new(self.root.clone())
    }

    fn events(&self) -> Vec<LibraryEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Waits for an event matching `f` (thumbnails arrive on another thread).
    fn wait_for_event(&self, f: impl Fn(&LibraryEvent) -> bool) -> LibraryEvent {
        for _ in 0..500 {
            if let Some(event) = self.events().into_iter().find(&f) {
                return event;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no matching event in {:?}", self.events());
    }

    fn save(&self, second: u32) -> CaptureItem {
        self.library.save_capture(capture(second)).unwrap()
    }
}

fn at(second: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, 22, 17, 41, second)
        .unwrap()
}

fn capture(second: u32) -> NewCapture {
    NewCapture {
        image: CaptureImage::Rgba {
            width: 4,
            height: 3,
            pixels: [10, 20, 30, 255].repeat(12),
        },
        mode: CaptureMode::Window,
        captured_at: at(second),
        window_title: Some("無題 - メモ帳".to_owned()),
    }
}

fn flattened(pixel: [u8; 4]) -> Vec<u8> {
    png_from_rgba(4, 3, &pixel.repeat(12)).unwrap()
}

fn document() -> serde_json::Value {
    json!({ "schemaVersion": 1, "shapes": [{ "type": "arrow" }] })
}

fn ids(items: &[CaptureItem]) -> Vec<&str> {
    items.iter().map(|item| item.id.as_str()).collect()
}

/// Makes the save folder unusable: its parent becomes a file.
fn block(h: &Harness) {
    fs::create_dir_all(h.root.parent().unwrap().parent().unwrap()).unwrap();
    fs::write(h.root.parent().unwrap(), b"not a folder").unwrap();
}

fn unblock(h: &Harness) {
    fs::remove_file(h.root.parent().unwrap()).unwrap();
}

// --- 5.1 / 5.2 ---

#[test]
fn lists_an_existing_folder_newest_first() {
    let h = Harness::new();
    h.save(50);
    h.save(52);
    h.save(51);
    h.library.flush_index();

    let reopened = CaptureLibrary::open(LibraryDeps {
        save_dir: h.root.clone(),
        desktop: Box::new(RecordingDesktop::default()),
        notifier: Arc::new(Notices::default()),
        events: Box::new(|_| {}),
        editor: Box::new(|| None),
    });

    assert_eq!(
        ids(&reopened.list()),
        ["20261022_174152", "20261022_174151", "20261022_174150"]
    );
    assert_eq!(reopened.save_dir(), h.root);
}

#[test]
fn save_creates_the_folder_writes_the_original_and_announces_it() {
    let h = Harness::new();
    assert!(!h.root.exists());

    let item = h.save(50);

    assert_eq!(item.id.as_str(), "20261022_174150");
    assert_eq!(item.original_path, h.root.join("Snipt_20261022_174150.png"));
    assert_eq!((item.width, item.height), (4, 3));
    assert_eq!(item.mode, CaptureMode::Window);
    assert_eq!(item.window_title.as_deref(), Some("無題 - メモ帳"));
    let (width, height, meta) = png_meta::read_header(&item.original_path).unwrap();
    assert_eq!((width, height), (4, 3));
    let meta = meta.unwrap();
    assert_eq!(meta.id, item.id);
    assert_eq!(meta.captured_at, at(50).fixed_offset());
    assert!(h.layout().tmp_dir().is_dir());
    assert_eq!(h.library.list(), vec![item.clone()]);
    assert_eq!(h.events()[0], LibraryEvent::ItemAdded(item.clone()));

    let LibraryEvent::ItemUpdated(updated) =
        h.wait_for_event(|e| matches!(e, LibraryEvent::ItemUpdated(_)))
    else {
        unreachable!()
    };
    assert_eq!(updated.thumbnail_path, Some(h.layout().thumbnail(&item.id)));
    assert!(updated.thumbnail_version > 0);
    assert!(h.layout().thumbnail(&item.id).is_file());

    h.library.flush_index();
    let cache = IndexFileV1::parse(&fs::read(h.layout().index_file()).unwrap()).unwrap();
    assert_eq!(cache.items.len(), 1);
    assert_eq!(cache.items[0].id, item.id);
}

#[test]
fn same_second_saves_get_sequence_numbers_with_matching_metadata() {
    let h = Harness::new();
    h.save(50);
    let second = h.save(50);

    assert_eq!(second.id.as_str(), "20261022_174150_02");
    let (_, _, meta) = png_meta::read_header(&second.original_path).unwrap();
    assert_eq!(meta.unwrap().id, second.id);
}

#[test]
fn the_index_is_written_in_the_background() {
    let h = Harness::new();
    h.save(50);
    for _ in 0..200 {
        if let Ok(bytes) = fs::read(h.layout().index_file()) {
            if IndexFileV1::parse(&bytes).is_some_and(|f| f.items.len() == 1) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("index.json was not written");
}

// --- 5.3 ---

#[test]
fn a_failed_save_keeps_the_capture_and_notifies_once() {
    let h = Harness::new();
    block(&h);

    let err = h.library.save_capture(capture(50)).unwrap_err();

    assert_eq!(err.reason.kind(), "saveDirUnavailable", "{:?}", err.reason);
    assert_eq!(err.unsaved.id.as_str(), "20261022_174150");
    let notices = h.notices.0.lock().unwrap().clone();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].level, NoticeLevel::Error);
    assert!(notices[0].body.contains("保存先フォルダを使用できません"));
    assert!(h.library.list().is_empty());
    assert!(h.events().is_empty());

    let session = h.library.load_for_edit(&err.unsaved.id).unwrap();
    assert!(session.unsaved);
    assert!(session.warning.is_some());
    let png = h.library.read_original_png(&err.unsaved.id).unwrap();
    let (_, _, meta) = png_meta::read_header_from(Cursor::new(&png)).unwrap();
    assert_eq!(meta.unwrap().id, err.unsaved.id);
}

#[test]
fn unsaved_captures_keep_their_ids_apart() {
    let h = Harness::new();
    block(&h);
    let first = h.library.save_capture(capture(50)).unwrap_err();
    let second = h.library.save_capture(capture(50)).unwrap_err();
    assert_eq!(first.unsaved.id.as_str(), "20261022_174150");
    assert_eq!(second.unsaved.id.as_str(), "20261022_174150_02");
}

#[test]
fn invalid_input_is_reported_without_keeping_anything() {
    let h = Harness::new();
    let mut bad = capture(50);
    bad.image = CaptureImage::Rgba {
        width: 0,
        height: 3,
        pixels: Vec::new(),
    };

    let err = h.library.save_capture(bad).unwrap_err();

    assert_eq!(err.reason.kind(), "invalidImage");
    assert_eq!(h.notices.0.lock().unwrap().len(), 1);
    assert_eq!(
        h.library.load_for_edit(&err.unsaved.id).unwrap_err().kind(),
        "notFound"
    );
}

// --- 5.4 ---

#[test]
fn load_for_edit_returns_annotations_when_usable() {
    let h = Harness::new();
    let item = h.save(50);
    let annotations = h.layout().annotations(&item.id);

    let plain = h.library.load_for_edit(&item.id).unwrap();
    assert_eq!(
        (plain.annotations, plain.warning, plain.unsaved),
        (None, None, false)
    );
    assert_eq!(plain.original_path, item.original_path);

    fs::write(&annotations, serde_json::to_vec(&document()).unwrap()).unwrap();
    let annotated = h.library.load_for_edit(&item.id).unwrap();
    assert_eq!(annotated.annotations, Some(document()));
    assert_eq!(annotated.warning, None);

    for broken in [
        &b"{broken"[..],
        b"[]",
        b"{\"schemaVersion\":\"1\"}",
        b"{\"schemaVersion\":1.5}",
    ] {
        fs::write(&annotations, broken).unwrap();
        let session = h.library.load_for_edit(&item.id).unwrap();
        assert_eq!(session.annotations, None);
        assert_eq!(session.warning.as_deref(), Some(ANNOTATIONS_UNREADABLE));
    }

    assert_eq!(
        h.library.read_original_png(&item.id).unwrap(),
        fs::read(&item.original_path).unwrap()
    );
    let unknown = id("20261022_000000");
    assert_eq!(
        h.library.load_for_edit(&unknown).unwrap_err().kind(),
        "notFound"
    );
    assert_eq!(
        h.library.read_original_png(&unknown).unwrap_err().kind(),
        "notFound"
    );
}

// --- 5.5 ---

#[test]
fn saving_annotations_replaces_the_annotated_png_and_keeps_the_original() {
    let h = Harness::new();
    let item = h.save(50);
    let original = fs::read(&item.original_path).unwrap();

    h.library
        .save_annotations(&item.id, document(), flattened([1, 1, 1, 255]))
        .unwrap();
    let saved = h
        .library
        .save_annotations(&item.id, document(), flattened([2, 2, 2, 255]))
        .unwrap();

    assert_eq!(fs::read(&item.original_path).unwrap(), original);
    assert_eq!(saved.annotated_path, Some(h.layout().annotated(&item.id)));
    assert!(saved.has_annotations);
    assert_eq!(
        fs::read(h.layout().annotated(&item.id)).unwrap(),
        flattened([2, 2, 2, 255])
    );
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(h.layout().annotations(&item.id)).unwrap()).unwrap();
    assert_eq!(stored, document());
    assert_eq!(h.library.list()[0].annotated_path, saved.annotated_path);
    assert!(h
        .events()
        .contains(&LibraryEvent::ItemUpdated(saved.clone())));
    assert_eq!(
        h.library.load_for_edit(&item.id).unwrap().annotations,
        Some(document())
    );
}

#[test]
fn invalid_annotation_saves_are_rejected() {
    let h = Harness::new();
    let item = h.save(50);
    for bad in [
        json!({ "schemaVersion": 0 }),
        json!({ "schemaVersion": "1" }),
        json!({ "shapes": [] }),
        json!([1]),
    ] {
        let err = h
            .library
            .save_annotations(&item.id, bad, flattened([0; 4]))
            .unwrap_err();
        assert_eq!(err.kind(), "invalidAnnotationDocument");
    }
    let err = h
        .library
        .save_annotations(&item.id, document(), b"nope".to_vec())
        .unwrap_err();
    assert_eq!(err.kind(), "invalidImage");
    assert!(!h.layout().annotations(&item.id).exists());
    assert!(!h.layout().annotated(&item.id).exists());
}

#[test]
fn saving_annotations_of_an_unsaved_capture_saves_it_first() {
    let h = Harness::new();
    block(&h);
    let unsaved = h.library.save_capture(capture(50)).unwrap_err().unsaved;
    h.library.load_for_edit(&unsaved.id).unwrap();

    let err = h
        .library
        .save_annotations(&unsaved.id, document(), flattened([0; 4]))
        .unwrap_err();
    assert_eq!(err.kind(), "saveDirUnavailable");

    unblock(&h);
    let saved = h
        .library
        .save_annotations(&unsaved.id, document(), flattened([0; 4]))
        .unwrap();

    assert_eq!(saved.id, unsaved.id);
    assert!(saved.original_path.is_file());
    assert!(saved.has_annotations);
    assert_eq!(ids(&h.library.list()), ["20261022_174150"]);
    assert!(matches!(h.events()[0], LibraryEvent::ItemAdded(_)));
    assert!(!h.library.load_for_edit(&unsaved.id).unwrap().unsaved);
}

#[test]
fn deleted_captures_do_not_come_back_from_a_late_annotation_save() {
    let h = Harness::new();
    let item = h.save(50);
    h.library.delete(std::slice::from_ref(&item.id)).unwrap();

    let err = h
        .library
        .save_annotations(&item.id, document(), flattened([0; 4]))
        .unwrap_err();

    assert_eq!(err.kind(), "notFound");
    assert!(!h.layout().annotations(&item.id).exists());
    assert!(!h.layout().annotated(&item.id).exists());
}

// --- 5.6 ---

#[test]
fn copy_and_reveal_use_the_annotated_png_when_there_is_one() {
    let h = Harness::new();
    let plain = h.save(50);
    let annotated = h.save(51);
    h.library
        .save_annotations(&annotated.id, document(), flattened([9, 8, 7, 128]))
        .unwrap();

    h.library.copy_to_clipboard(&plain.id).unwrap();
    h.library.copy_to_clipboard(&annotated.id).unwrap();
    h.library.reveal_in_explorer(&plain.id).unwrap();
    h.library.reveal_in_explorer(&annotated.id).unwrap();

    let calls = h.desktop.calls();
    assert_eq!(
        calls[0],
        DesktopCall::Copy {
            width: 4,
            height: 3,
            rgba: [10, 20, 30, 255].repeat(12),
            png: Some(fs::read(&plain.original_path).unwrap()),
        }
    );
    assert_eq!(
        calls[1],
        DesktopCall::Copy {
            width: 4,
            height: 3,
            rgba: [9, 8, 7, 128].repeat(12),
            png: Some(flattened([9, 8, 7, 128])),
        }
    );
    assert_eq!(calls[2], DesktopCall::Reveal(plain.original_path.clone()));
    assert_eq!(
        calls[3],
        DesktopCall::Reveal(h.layout().annotated(&annotated.id))
    );

    let unknown = id("20261022_000000");
    assert_eq!(
        h.library.copy_to_clipboard(&unknown).unwrap_err().kind(),
        "notFound"
    );
    assert_eq!(
        h.library.reveal_in_explorer(&unknown).unwrap_err().kind(),
        "notFound"
    );
}

#[test]
fn prune_missing_drops_only_missing_originals() {
    let h = Harness::new();
    let kept = h.save(50);
    let gone = h.save(51);
    fs::remove_file(&gone.original_path).unwrap();

    let removed =
        h.library
            .prune_missing(&[kept.id.clone(), gone.id.clone(), id("20261022_000000")]);

    assert_eq!(removed, vec![gone.id.clone()]);
    assert_eq!(ids(&h.library.list()), [kept.id.as_str()]);
    assert!(h.events().contains(&LibraryEvent::ItemsRemoved {
        ids: vec![gone.id],
        reason: RemovedReason::MissingOnDisk,
    }));
    assert!(h.library.prune_missing(&[kept.id]).is_empty());
}

// --- 5.7 ---

#[test]
fn open_in_editor_asks_the_launcher() {
    let h = Harness::new();
    let item = h.save(50);
    h.library.open_in_editor(&item.id).unwrap();
    assert_eq!(
        h.editor.calls.lock().unwrap().clone(),
        [EditorCall::Open(item.id)]
    );
    assert_eq!(
        h.library
            .open_in_editor(&id("20261022_000000"))
            .unwrap_err()
            .kind(),
        "notFound"
    );
}

#[test]
fn delete_closes_editors_outside_the_lock_then_recycles_all_files() {
    let h = Harness::new();
    let item = h.save(50);
    let other = h.save(51);
    h.library
        .save_annotations(&item.id, document(), flattened([0; 4]))
        .unwrap();

    let outcome = h
        .library
        .delete(&[item.id.clone(), item.id.clone()])
        .unwrap();

    assert_eq!(outcome.removed, vec![item.id.clone()]);
    assert!(outcome.failed.is_empty());
    assert_eq!(
        h.editor.calls.lock().unwrap().clone(),
        [EditorCall::Close {
            id: item.id.clone(),
            lock_free: true,
        }]
    );
    assert_eq!(
        h.desktop.calls().last(),
        Some(&DesktopCall::Recycle(vec![
            item.original_path.clone(),
            h.layout().annotated(&item.id),
            h.layout().annotations(&item.id),
        ]))
    );
    assert_eq!(ids(&h.library.list()), [other.id.as_str()]);
    assert!(!h.layout().thumbnail(&item.id).exists());
    assert!(h.events().contains(&LibraryEvent::ItemsRemoved {
        ids: vec![item.id.clone()],
        reason: RemovedReason::Deleted,
    }));
    assert_eq!(
        h.library.load_for_edit(&item.id).unwrap_err().kind(),
        "notFound"
    );
}

#[test]
fn a_failed_recycle_keeps_the_captures() {
    let h = Harness::new();
    let item = h.save(50);
    h.desktop.fail_with(Some(LibraryError::Shell {
        message: "in use".to_owned(),
    }));

    let outcome = h
        .library
        .delete(&[item.id.clone(), id("20261022_000000")])
        .unwrap();

    assert!(outcome.removed.is_empty());
    let failed: Vec<_> = outcome
        .failed
        .iter()
        .map(|f| (f.id.as_str(), f.error.kind()))
        .collect();
    assert_eq!(
        failed,
        [
            ("20261022_000000", "notFound"),
            ("20261022_174150", "shell")
        ]
    );
    assert_eq!(ids(&h.library.list()), [item.id.as_str()]);
    assert!(!h
        .events()
        .iter()
        .any(|e| matches!(e, LibraryEvent::ItemsRemoved { .. })));
    // Still usable after the failed delete.
    h.library
        .save_annotations(&item.id, document(), flattened([0; 4]))
        .unwrap();
}

#[test]
fn deleting_an_unsaved_capture_forgets_it() {
    let h = Harness::new();
    block(&h);
    let unsaved = h.library.save_capture(capture(50)).unwrap_err().unsaved;

    let outcome = h.library.delete(std::slice::from_ref(&unsaved.id)).unwrap();

    assert_eq!(outcome.removed, vec![unsaved.id.clone()]);
    assert!(h.desktop.calls().is_empty());
    assert_eq!(
        h.library.read_original_png(&unsaved.id).unwrap_err().kind(),
        "notFound"
    );
    assert!(h.library.before_exit().unsaved.is_empty());
}

// --- 5.8 ---

#[test]
fn before_exit_saves_what_it_can_and_reports_the_rest() {
    let h = Harness::new();
    block(&h);
    let unsaved = h.library.save_capture(capture(50)).unwrap_err().unsaved;

    assert_eq!(
        h.library.before_exit().unsaved,
        ["撮影画像: 20261022_174150"]
    );
    assert_eq!(
        h.library.before_exit().unsaved,
        ["撮影画像: 20261022_174150"]
    );

    unblock(&h);
    assert!(h.library.before_exit().unsaved.is_empty());
    assert!(h.library.before_exit().unsaved.is_empty());
    assert_eq!(ids(&h.library.list()), [unsaved.id.as_str()]);
    assert!(unsaved.original_path.is_file());
    assert_eq!(
        h.events()
            .iter()
            .filter(|e| matches!(e, LibraryEvent::ItemAdded(_)))
            .count(),
        1
    );
}
