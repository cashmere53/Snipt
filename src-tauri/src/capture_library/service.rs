//! `CaptureLibrary`: the single entry point for saving, re-editing, listing, copying, revealing
//! and deleting captures. Managed as `Arc<CaptureLibrary>`.
//!
//! Writes (saves, annotation saves, deletes) are serialized by the state's write lock. Every
//! file is written to `.snipt/tmp` and renamed into place; originals are never changed after
//! they are created. The index is loaded on a background thread at startup and every call
//! waits for it.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};

use super::atomic_fs::{fs_error, write_replace};
use super::events::{LibraryEvent, RemovedReason};
use super::index::{write_snapshot, LibraryIndex};
use super::layout::LibraryLayout;
use super::naming::{base_name, next_name, FILE_PREFIX};
use super::png_meta::{self, SniptMetaV1};
use super::ports::{ClipboardImage, DesktopOps};
use super::thumbnails::{ThumbnailJob, ThumbnailResult, ThumbnailSource, ThumbnailWorker};
use super::types::{
    CaptureImage, CaptureItem, DeleteFailure, DeleteOutcome, EditSession, LibraryError, NewCapture,
    SaveCaptureError,
};
use super::unsaved::{UnsavedCapture, UnsavedStore};
use crate::shared::{CaptureId, EditorLauncher, ExitCheck, Notice, NoticeLevel, Notifier};

/// Index writes requested within this window are merged into one.
const PERSIST_DELAY: Duration = Duration::from_millis(300);

const SAVE_FAILED_TITLE: &str = "キャプチャを保存できませんでした";
const SAVE_FAILED_HINT: &str =
    "画像はアプリを終了するまで保持されます。編集画面で注釈を保存すると、もう一度保存を試みます。";
const UNSAVED_WARNING: &str =
    "このキャプチャはまだ保存先に保存されていません。注釈を保存すると、もう一度保存を試みます。";
const ANNOTATIONS_UNREADABLE: &str = "注釈を読み込めませんでした。元画像のみ開きます。";

pub type EventSink = Box<dyn Fn(LibraryEvent) + Send + Sync>;
/// Resolves the editor launcher when it is needed: annotation-editor sets up after this spec.
pub type EditorResolver = Box<dyn Fn() -> Option<Arc<dyn EditorLauncher>> + Send + Sync>;

/// What the library is built from; production wiring is in `capture_library::setup`.
pub struct LibraryDeps {
    pub save_dir: PathBuf,
    pub desktop: Box<dyn DesktopOps>,
    pub notifier: Arc<dyn Notifier>,
    pub events: EventSink,
    pub editor: EditorResolver,
}

struct LibraryState {
    layout: LibraryLayout,
    index: LibraryIndex,
    /// Captures opened for editing, so their annotations keep going to the folder they came
    /// from after the save folder changes.
    sessions: HashMap<CaptureId, CaptureItem>,
    /// Deleted ids: an editor that is still saving must not bring their files back.
    deleted: HashSet<CaptureId>,
}

impl LibraryState {
    /// An item of the current folder.
    fn current(&self, id: &CaptureId) -> Result<&CaptureItem, LibraryError> {
        self.index
            .get(id)
            .filter(|_| !self.deleted.contains(id))
            .ok_or_else(|| not_found(id))
    }

    /// An item being edited, else one of the current folder.
    fn find(&self, id: &CaptureId) -> Result<&CaptureItem, LibraryError> {
        if self.deleted.contains(id) {
            return Err(not_found(id));
        }
        self.sessions
            .get(id)
            .or_else(|| self.index.get(id))
            .ok_or_else(|| not_found(id))
    }
}

fn not_found(id: &CaptureId) -> LibraryError {
    LibraryError::NotFound { id: id.clone() }
}

/// The folder an item lives in (it may not be the current one).
fn layout_of(item: &CaptureItem) -> LibraryLayout {
    LibraryLayout::new(
        item.original_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_owned(),
    )
}

/// Reads a capture's file; a missing file means the capture is gone.
fn read_capture_file(id: &CaptureId, path: &Path) -> Result<Vec<u8>, LibraryError> {
    fs::read(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => not_found(id),
        _ => fs_error(path, &e),
    })
}

/// The annotation document must be an object with an integer `schemaVersion` of 1 or more.
fn validate_document(document: &serde_json::Value) -> Result<(), LibraryError> {
    let invalid = |message: &str| LibraryError::InvalidAnnotationDocument {
        message: message.to_owned(),
    };
    let object = document
        .as_object()
        .ok_or_else(|| invalid("オブジェクトではありません"))?;
    match object
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
    {
        Some(version) if version >= 1 => Ok(()),
        _ => Err(invalid("schemaVersion が 1 以上の整数ではありません")),
    }
}

/// The annotations next to `item`, or a warning when they cannot be used.
fn read_annotations(item: &CaptureItem) -> (Option<serde_json::Value>, Option<String>) {
    let path = layout_of(item).annotations(&item.id);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return (None, None),
        Err(e) => {
            log::warn!("library: could not read {}: {e}", path.display());
            return (None, Some(ANNOTATIONS_UNREADABLE.to_owned()));
        }
    };
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(document) if validate_document(&document).is_ok() => (Some(document), None),
        _ => {
            log::warn!("library: unusable annotations in {}", path.display());
            (None, Some(ANNOTATIONS_UNREADABLE.to_owned()))
        }
    }
}

/// A gate that opens once and stays open.
#[derive(Default)]
struct Latch {
    open: Mutex<bool>,
    opened: Condvar,
}

impl Latch {
    fn open(&self) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.opened.notify_all();
    }

    fn wait(&self) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        while !*open {
            open = self.opened.wait(open).unwrap_or_else(|e| e.into_inner());
        }
    }
}

#[derive(Default)]
struct PersistFlags {
    dirty: bool,
    closed: bool,
}

/// Requests to write `index.json`, merged by a background thread.
#[derive(Default)]
struct PersistQueue {
    flags: Mutex<PersistFlags>,
    wake: Condvar,
}

fn persist_loop(queue: &PersistQueue, library: &Weak<CaptureLibrary>) {
    loop {
        {
            let mut flags = queue.flags.lock().unwrap_or_else(|e| e.into_inner());
            while !flags.dirty && !flags.closed {
                flags = queue.wake.wait(flags).unwrap_or_else(|e| e.into_inner());
            }
            if flags.closed {
                return;
            }
        }
        std::thread::sleep(PERSIST_DELAY);
        queue.flags.lock().unwrap_or_else(|e| e.into_inner()).dirty = false;
        let Some(library) = library.upgrade() else {
            return;
        };
        library.flush_index();
    }
}

pub struct CaptureLibrary {
    state: RwLock<LibraryState>,
    ready: Latch,
    unsaved: UnsavedStore,
    desktop: Box<dyn DesktopOps>,
    notifier: Arc<dyn Notifier>,
    events: EventSink,
    editor: EditorResolver,
    thumbnails: ThumbnailWorker,
    persist: Arc<PersistQueue>,
}

impl CaptureLibrary {
    /// Starts the library on `deps.save_dir` and loads its index in the background.
    pub fn open(deps: LibraryDeps) -> Arc<Self> {
        let layout = LibraryLayout::new(deps.save_dir);
        let library = Arc::new_cyclic(|weak: &Weak<Self>| {
            let on_done = {
                let weak = weak.clone();
                Box::new(move |job: &ThumbnailJob, result: ThumbnailResult| {
                    if let Some(library) = weak.upgrade() {
                        library.thumbnail_done(job, result);
                    }
                })
            };
            Self {
                state: RwLock::new(LibraryState {
                    layout: layout.clone(),
                    index: LibraryIndex::default(),
                    sessions: HashMap::new(),
                    deleted: HashSet::new(),
                }),
                ready: Latch::default(),
                unsaved: UnsavedStore::default(),
                desktop: deps.desktop,
                notifier: deps.notifier,
                events: deps.events,
                editor: deps.editor,
                thumbnails: ThumbnailWorker::start(on_done),
                persist: Arc::new(PersistQueue::default()),
            }
        });

        let queue = library.persist.clone();
        let weak = Arc::downgrade(&library);
        std::thread::Builder::new()
            .name("snipt-index-writer".to_owned())
            .spawn(move || persist_loop(&queue, &weak))
            .expect("spawning the index writer");

        let loader = library.clone();
        std::thread::Builder::new()
            .name("snipt-index-loader".to_owned())
            .spawn(move || {
                let (index, _) = LibraryIndex::load(&layout);
                loader.finish_loading(&layout, index);
            })
            .expect("spawning the index loader");
        library
    }

    /// The current save folder.
    pub fn save_dir(&self) -> PathBuf {
        self.read().layout.root().to_owned()
    }

    fn finish_loading(&self, layout: &LibraryLayout, index: LibraryIndex) {
        let missing: Vec<CaptureItem> = {
            let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
            if state.layout != *layout {
                drop(state);
                self.ready.open();
                return;
            }
            state.index = index;
            state
                .index
                .items_newest_first()
                .into_iter()
                .filter(|item| item.thumbnail_path.is_none())
                .collect()
        };
        self.ready.open();
        for item in missing {
            let source = ThumbnailSource::File(item.display_path().to_owned());
            self.request_thumbnail(&item, layout, source);
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, LibraryState> {
        self.ready.wait();
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, LibraryState> {
        self.ready.wait();
        self.state.write().unwrap_or_else(|e| e.into_inner())
    }

    fn emit(&self, event: LibraryEvent) {
        (self.events)(event);
    }

    fn schedule_persist(&self) {
        self.persist
            .flags
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .dirty = true;
        self.persist.wake.notify_one();
    }

    /// Writes `index.json` now. A failure is only logged: the next load reconciles.
    pub fn flush_index(&self) {
        let (layout, snapshot) = {
            let state = self.read();
            (state.layout.clone(), state.index.snapshot_for_persist())
        };
        if let Err(e) = write_snapshot(&layout, &snapshot) {
            log::warn!("library: could not write the index: {e}");
        }
    }

    fn request_thumbnail(
        &self,
        item: &CaptureItem,
        layout: &LibraryLayout,
        source: ThumbnailSource,
    ) {
        self.thumbnails.request(ThumbnailJob {
            id: item.id.clone(),
            captured_at: item.captured_at,
            source,
            dest: layout.thumbnail(&item.id),
            tmp_dir: layout.tmp_dir(),
        });
    }

    fn thumbnail_done(&self, job: &ThumbnailJob, result: ThumbnailResult) {
        let Ok(version) = result else {
            return;
        };
        let updated = {
            let mut state = self.write();
            if let Some(item) = state.sessions.get_mut(&job.id) {
                if layout_of(item).thumbnail(&job.id) == job.dest {
                    item.thumbnail_path = Some(job.dest.clone());
                    item.thumbnail_version = version;
                }
            }
            let current = state
                .index
                .get(&job.id)
                .filter(|_| state.layout.thumbnail(&job.id) == job.dest)
                .cloned();
            current.map(|mut item| {
                item.thumbnail_path = Some(job.dest.clone());
                item.thumbnail_version = version;
                state.index.upsert(item.clone());
                item
            })
        };
        if let Some(item) = updated {
            self.schedule_persist();
            self.emit(LibraryEvent::ItemUpdated(item));
        }
    }

    /// Whether a new capture may not take `base`.
    fn name_taken(&self, state: &LibraryState, base: &str) -> bool {
        let Some(id) = base
            .strip_prefix(FILE_PREFIX)
            .and_then(|id| CaptureId::parse(id).ok())
        else {
            return true;
        };
        state.index.contains(&id)
            || state.sessions.contains_key(&id)
            || state.deleted.contains(&id)
            || self.unsaved.contains(&id)
            || state.layout.original(&id).exists()
    }

    /// Saves a new capture as `Snipt_<id>.png` in the current folder and adds it to the
    /// history. On failure the capture is kept in memory (see `SaveCaptureError`) and the user
    /// is notified.
    pub fn save_capture(&self, capture: NewCapture) -> Result<CaptureItem, SaveCaptureError> {
        let started = Instant::now();
        let NewCapture {
            image,
            mode,
            captured_at,
            window_title,
        } = capture;
        let meta = |id: &CaptureId| SniptMetaV1 {
            v: 1,
            id: id.clone(),
            captured_at: captured_at.fixed_offset(),
            mode,
            window_title: window_title.clone(),
        };
        let item = |id: CaptureId, width: u32, height: u32, layout: &LibraryLayout| CaptureItem {
            original_path: layout.original(&id),
            id,
            captured_at: captured_at.fixed_offset(),
            mode,
            width,
            height,
            window_title: window_title.clone(),
            annotated_path: None,
            has_annotations: false,
            thumbnail_path: None,
            thumbnail_version: 0,
        };

        // Encode before taking the lock; the metadata is cheap to rewrite once the id is known.
        let provisional = provisional_id(captured_at);
        let png = match png_meta::encode(&image, &meta(&provisional)) {
            Ok(png) => png,
            Err(reason) => {
                // A caller bug: screen-capture always hands over pixels.
                log::error!("library: invalid capture: {reason}");
                let (width, height) = match &image {
                    CaptureImage::Rgba { width, height, .. } => (*width, *height),
                    CaptureImage::Png(_) => (0, 0),
                };
                let unsaved = item(provisional, width, height, &self.read().layout);
                self.notify_save_failure(&reason);
                return Err(SaveCaptureError {
                    reason,
                    unsaved: Box::new(unsaved),
                });
            }
        };
        let (width, height, _) =
            png_meta::read_header_from(Cursor::new(&png)).expect("encode produces a readable PNG");

        let mut state = self.write();
        let layout = state.layout.clone();
        let id = next_name(captured_at, |base| self.name_taken(&state, base)).map(|(id, _)| id);
        let written = id
            .clone()
            .ok_or_else(|| LibraryError::Io {
                message: "同じ秒のキャプチャが多すぎます".to_owned(),
            })
            .and_then(|id| {
                let png = if id == provisional {
                    png.clone()
                } else {
                    png_meta::encode(&CaptureImage::Png(png.clone()), &meta(&id))?
                };
                layout.ensure_dirs()?;
                write_replace(&layout.tmp_dir(), &layout.original(&id), &png)?;
                Ok((id, png))
            });

        match written {
            Ok((id, png)) => {
                let item = item(id, width, height, &layout);
                state.index.upsert(item.clone());
                drop(state);
                self.schedule_persist();
                self.emit(LibraryEvent::ItemAdded(item.clone()));
                let source = match image {
                    CaptureImage::Rgba {
                        width,
                        height,
                        pixels,
                    } => ThumbnailSource::Rgba {
                        width,
                        height,
                        pixels: Arc::new(pixels),
                    },
                    CaptureImage::Png(_) => ThumbnailSource::Png(Arc::new(png)),
                };
                self.request_thumbnail(&item, &layout, source);
                log::info!("library: saved {} in {:?}", item.id, started.elapsed());
                Ok(item)
            }
            Err(reason) => {
                let id = id.unwrap_or(provisional);
                let png = png_meta::encode(&CaptureImage::Png(png), &meta(&id)).unwrap_or_default();
                let unsaved = item(id, width, height, &layout);
                drop(state);
                log::warn!("library: could not save {}: {reason}", unsaved.id);
                self.keep_unsaved(&layout, unsaved.clone(), png, reason.clone());
                self.notify_save_failure(&reason);
                Err(SaveCaptureError {
                    reason,
                    unsaved: Box::new(unsaved),
                })
            }
        }
    }

    fn keep_unsaved(
        &self,
        layout: &LibraryLayout,
        item: CaptureItem,
        png: Vec<u8>,
        reason: LibraryError,
    ) {
        // Best effort: a copy in `.snipt/tmp` survives a crash.
        if layout.tmp_dir().is_dir() {
            let copy = layout
                .tmp_dir()
                .join(format!("{}.png", base_name(&item.id)));
            if let Err(e) = fs::write(&copy, &png) {
                log::warn!("library: could not keep a copy in {}: {e}", copy.display());
            }
        }
        self.unsaved.insert(UnsavedCapture {
            item,
            png: Arc::new(png),
            reason,
        });
    }

    fn notify_save_failure(&self, reason: &LibraryError) {
        self.notifier.notify(Notice {
            level: NoticeLevel::Error,
            title: SAVE_FAILED_TITLE.to_owned(),
            body: format!("{reason}\n{SAVE_FAILED_HINT}"),
        });
    }

    /// Writes an unsaved capture to the current folder and makes it a regular item.
    fn resave_unsaved(&self, id: &CaptureId) -> Result<CaptureItem, LibraryError> {
        let mut state = self.write();
        let Some(unsaved) = self.unsaved.get(id) else {
            // Saved or deleted meanwhile.
            return state.find(id).cloned();
        };
        let layout = state.layout.clone();
        let path = layout.original(id);
        if state.index.contains(id) || path.exists() {
            return Err(LibraryError::Io {
                message: format!("{} は既に存在します", path.display()),
            });
        }
        layout.ensure_dirs()?;
        write_replace(&layout.tmp_dir(), &path, &unsaved.png)?;
        let item = CaptureItem {
            original_path: path,
            ..unsaved.item
        };
        state.index.upsert(item.clone());
        if state.sessions.contains_key(id) {
            state.sessions.insert(id.clone(), item.clone());
        }
        self.unsaved.remove(id);
        drop(state);

        let _ = fs::remove_file(layout.tmp_dir().join(format!("{}.png", base_name(id))));
        self.schedule_persist();
        self.emit(LibraryEvent::ItemAdded(item.clone()));
        self.request_thumbnail(&item, &layout, ThumbnailSource::Png(unsaved.png));
        log::info!("library: saved {id} after an earlier failure");
        Ok(item)
    }

    /// The item and its annotations for the editor. Unreadable annotations open the original
    /// alone with a warning.
    pub fn load_for_edit(&self, id: &CaptureId) -> Result<EditSession, LibraryError> {
        if let Some(unsaved) = self.unsaved.get(id) {
            self.write()
                .sessions
                .insert(id.clone(), unsaved.item.clone());
            return Ok(EditSession {
                original_path: unsaved.item.original_path.clone(),
                item: unsaved.item,
                annotations: None,
                warning: Some(UNSAVED_WARNING.to_owned()),
                unsaved: true,
            });
        }
        let item = {
            let mut state = self.write();
            let item = state.find(id)?.clone();
            state.sessions.insert(id.clone(), item.clone());
            item
        };
        let (annotations, warning) = read_annotations(&item);
        Ok(EditSession {
            original_path: item.original_path.clone(),
            item,
            annotations,
            warning,
            unsaved: false,
        })
    }

    /// The original PNG, from memory for an unsaved capture. The editor's only pixel source.
    pub fn read_original_png(&self, id: &CaptureId) -> Result<Vec<u8>, LibraryError> {
        if let Some(unsaved) = self.unsaved.get(id) {
            return Ok(unsaved.png.to_vec());
        }
        let path = self.read().find(id)?.original_path.clone();
        read_capture_file(id, &path)
    }

    /// Replaces the annotation document and the annotated PNG; the original is untouched. An
    /// unsaved capture is saved first. Captures being edited keep their folder after the save
    /// folder changes.
    pub fn save_annotations(
        &self,
        id: &CaptureId,
        document: serde_json::Value,
        flattened_png: Vec<u8>,
    ) -> Result<CaptureItem, LibraryError> {
        validate_document(&document)?;
        png_meta::read_header_from(Cursor::new(&flattened_png))?;
        let json = serde_json::to_vec(&document).expect("JSON values always serialize");
        if self.unsaved.contains(id) {
            self.resave_unsaved(id)?;
        }

        let (item, layout, current) = {
            let mut state = self.write();
            let mut item = state.find(id)?.clone();
            let layout = layout_of(&item);
            layout.ensure_dirs()?;
            write_replace(&layout.tmp_dir(), &layout.annotations(id), &json)?;
            write_replace(&layout.tmp_dir(), &layout.annotated(id), &flattened_png)?;
            item.annotated_path = Some(layout.annotated(id));
            item.has_annotations = true;
            if state.sessions.contains_key(id) {
                state.sessions.insert(id.clone(), item.clone());
            }
            let current = state
                .index
                .get(id)
                .is_some_and(|indexed| indexed.original_path == item.original_path);
            if current {
                state.index.upsert(item.clone());
            }
            (item, layout, current)
        };
        if current {
            self.schedule_persist();
            self.emit(LibraryEvent::ItemUpdated(item.clone()));
        }
        self.request_thumbnail(
            &item,
            &layout,
            ThumbnailSource::Png(Arc::new(flattened_png)),
        );
        Ok(item)
    }

    /// The history, newest first.
    pub fn list(&self) -> Vec<CaptureItem> {
        self.read().index.items_newest_first()
    }

    /// Copies the annotated PNG, or the original, as an image.
    pub fn copy_to_clipboard(&self, id: &CaptureId) -> Result<(), LibraryError> {
        let path = self.read().current(id)?.display_path().to_owned();
        let png = read_capture_file(id, &path)?;
        let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|e| LibraryError::InvalidImage {
                message: e.to_string(),
            })?
            .into_rgba8();
        self.desktop.copy_image(ClipboardImage {
            width: image.width(),
            height: image.height(),
            rgba: image.as_raw(),
            png: Some(&png),
        })
    }

    /// Opens Explorer with the annotated PNG, or the original, selected.
    pub fn reveal_in_explorer(&self, id: &CaptureId) -> Result<(), LibraryError> {
        let path = self.read().current(id)?.display_path().to_owned();
        self.desktop.reveal(&path)
    }

    /// Opens (or brings to the front) the capture's editor.
    pub fn open_in_editor(&self, id: &CaptureId) -> Result<(), LibraryError> {
        if !self.unsaved.contains(id) {
            self.read().current(id)?;
        }
        match (self.editor)() {
            Some(editor) => editor.open_editor(id),
            None => log::warn!("library: no editor to open {id}"),
        }
        Ok(())
    }

    /// Drops the ids whose original no longer exists, without an error. Returns those dropped.
    pub fn prune_missing(&self, ids: &[CaptureId]) -> Vec<CaptureId> {
        let removed = {
            let mut state = self.write();
            let missing: Vec<CaptureId> = ids
                .iter()
                .filter(|id| {
                    state
                        .index
                        .get(id)
                        .is_some_and(|item| !item.original_path.exists())
                })
                .cloned()
                .collect();
            let removed = state.index.remove(&missing);
            for id in &removed {
                let _ = fs::remove_file(state.layout.thumbnail(id));
            }
            removed
        };
        if !removed.is_empty() {
            log::info!("library: {} capture(s) missing on disk", removed.len());
            self.forget_thumbnails(&removed);
            self.schedule_persist();
            self.emit(LibraryEvent::ItemsRemoved {
                ids: removed.clone(),
                reason: RemovedReason::MissingOnDisk,
            });
        }
        removed
    }

    fn forget_thumbnails(&self, ids: &[CaptureId]) {
        let ids: HashSet<&CaptureId> = ids.iter().collect();
        self.thumbnails.retain(|job| !ids.contains(&job.id));
    }

    /// Closes the captures' editors (saving their annotations), then moves the original,
    /// annotated PNG and annotations to the recycle bin in one operation. Captures that could
    /// not be moved stay in the history and are listed in `failed`.
    pub fn delete(&self, ids: &[CaptureId]) -> Result<DeleteOutcome, LibraryError> {
        let mut seen = HashSet::new();
        let ids: Vec<CaptureId> = ids.iter().filter(|id| seen.insert(*id)).cloned().collect();
        // Outside the lock: flushing an editor calls `save_annotations`.
        if let Some(editor) = (self.editor)() {
            for id in &ids {
                editor.close_editor(id);
            }
        }

        let mut outcome = DeleteOutcome::default();
        let mut state = self.write();
        let mut targets = Vec::new();
        let mut files = Vec::new();
        for id in ids {
            if self.unsaved.remove(&id).is_some() {
                state.sessions.remove(&id);
                state.deleted.insert(id.clone());
                outcome.removed.push(id);
                continue;
            }
            let Ok(item) = state.current(&id) else {
                outcome.failed.push(DeleteFailure {
                    error: not_found(&id),
                    id,
                });
                continue;
            };
            let layout = layout_of(item);
            let original = item.original_path.clone();
            files.extend(
                [
                    original.clone(),
                    layout.annotated(&id),
                    layout.annotations(&id),
                ]
                .into_iter()
                .filter(|path| path.exists()),
            );
            state.deleted.insert(id.clone());
            targets.push((id, original));
        }
        let failure = if files.is_empty() {
            None
        } else {
            self.desktop.recycle(&files).err()
        };
        let mut removed = Vec::new();
        for (id, original) in targets {
            match &failure {
                Some(error) if original.exists() => {
                    state.deleted.remove(&id);
                    outcome.failed.push(DeleteFailure {
                        id,
                        error: error.clone(),
                    });
                }
                _ => removed.push(id),
            }
        }
        for id in &removed {
            let _ = fs::remove_file(state.layout.thumbnail(id));
            state.sessions.remove(id);
        }
        state.index.remove(&removed);
        drop(state);

        if let Some(error) = &failure {
            log::warn!("library: recycle failed: {error}");
        }
        outcome.removed.extend(removed);
        if !outcome.removed.is_empty() {
            self.forget_thumbnails(&outcome.removed);
            self.schedule_persist();
            self.emit(LibraryEvent::ItemsRemoved {
                ids: outcome.removed.clone(),
                reason: RemovedReason::Deleted,
            });
        }
        Ok(outcome)
    }

    /// The before-exit hook: saves every unsaved capture to the current folder and returns
    /// those that still cannot be saved. Idempotent.
    pub fn before_exit(&self) -> ExitCheck {
        let mut check = ExitCheck::default();
        for capture in self.unsaved.list() {
            let id = capture.item.id;
            if let Err(e) = self.resave_unsaved(&id) {
                log::warn!("library: {id} is still unsaved: {e}");
                check.unsaved.push(format!("撮影画像: {id}"));
            }
        }
        check
    }
}

impl Drop for CaptureLibrary {
    fn drop(&mut self) {
        let dirty = {
            let mut flags = self.persist.flags.lock().unwrap_or_else(|e| e.into_inner());
            flags.closed = true;
            std::mem::take(&mut flags.dirty)
        };
        self.persist.wake.notify_all();
        if dirty {
            let state = self.state.get_mut().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = write_snapshot(&state.layout, &state.index.snapshot_for_persist()) {
                log::warn!("library: could not write the index: {e}");
            }
        }
    }
}

/// The id a capture at `at` gets when nothing else was taken in that second.
fn provisional_id(at: DateTime<Local>) -> CaptureId {
    next_name(at, |_| false).expect("a free name exists").0
}

#[cfg(test)]
mod tests;
