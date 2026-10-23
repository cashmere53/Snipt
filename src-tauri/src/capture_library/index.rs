//! `LibraryIndex`: every capture of the current save folder in memory, cached in
//! `.snipt/index.json` so startup does not have to open every PNG.
//!
//! The cache is reconciled with the folder on load (file names only): originals missing from
//! the cache are read from their PNG metadata, entries whose original is gone are dropped, and
//! the annotated PNG / annotation document / thumbnail flags follow the files. A missing,
//! corrupt or unknown-version cache is rebuilt from the files.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{Instant, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use super::atomic_fs::write_replace;
use super::layout::LibraryLayout;
use super::naming::{base_name, classify, SniptFile};
use super::png_meta::read_header;
use super::types::{CaptureItem, LibraryError};
use crate::shared::{CaptureId, CaptureMode};

pub const INDEX_SCHEMA_VERSION: u32 = 1;

/// `.snipt/index.json`. Paths are not stored: they follow from `base` and the folder, so the
/// folder can be moved as a whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexFileV1 {
    pub schema_version: u32,
    pub items: Vec<IndexEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub id: CaptureId,
    /// `Snipt_<id>`, the original's file name without the extension.
    pub base: String,
    pub captured_at: DateTime<FixedOffset>,
    pub mode: CaptureMode,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub window_title: Option<String>,
    pub has_annotated_png: bool,
    pub has_annotations: bool,
    /// 0 when there is no thumbnail.
    pub thumbnail_version: u64,
}

impl IndexFileV1 {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("the index always serializes")
    }

    /// `None` for anything but a well-formed version 1 file.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        if value.get("schemaVersion")?.as_u64()? != u64::from(INDEX_SCHEMA_VERSION) {
            return None;
        }
        serde_json::from_value(value).ok()
    }
}

/// All captures of one save folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryIndex {
    items: HashMap<CaptureId, CaptureItem>,
}

/// How a load went, for logging and for deciding whether to rewrite the cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// The cache was missing, corrupt or of an unknown version.
    pub rebuilt: bool,
    /// Originals found on disk but not in the cache.
    pub imported: usize,
    /// Cache entries whose original is gone.
    pub dropped: usize,
    /// Cache entries whose annotation or thumbnail flags were corrected.
    pub corrected: usize,
}

impl LoadReport {
    pub fn changed(&self) -> bool {
        self.rebuilt || self.imported > 0 || self.dropped > 0 || self.corrected > 0
    }
}

/// The Snipt files of a save folder, by name only.
#[derive(Default)]
struct FolderListing {
    originals: HashSet<CaptureId>,
    annotated: HashSet<CaptureId>,
    annotations: HashSet<CaptureId>,
    thumbnails: HashSet<CaptureId>,
}

impl FolderListing {
    fn read(layout: &LibraryLayout) -> Self {
        let mut listing = Self::default();
        for name in file_names(layout.root()) {
            match classify(&name) {
                Some(SniptFile::Original(id)) => listing.originals.insert(id),
                Some(SniptFile::Annotated(id)) => listing.annotated.insert(id),
                None => false,
            };
        }
        listing.annotations = ids_with_extension(&layout.annotations_dir(), ".json");
        listing.thumbnails = ids_with_extension(&layout.thumbs_dir(), ".png");
        listing
    }
}

fn file_names(dir: &Path) -> Vec<String> {
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect(),
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("library: could not list {}: {e}", dir.display());
            }
            Vec::new()
        }
    }
}

/// `<id><ext>` files of a `.snipt` subfolder.
fn ids_with_extension(dir: &Path, ext: &str) -> HashSet<CaptureId> {
    file_names(dir)
        .iter()
        .filter_map(|name| CaptureId::parse(name.strip_suffix(ext)?).ok())
        .collect()
}

/// Milliseconds since the epoch of the file's modification time, at least 1.
fn modified_millis(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(1, |d| (d.as_millis() as u64).max(1))
}

impl LibraryIndex {
    /// Reads the cache of `layout` and reconciles it with the folder, rebuilding it from the
    /// files when it is unusable. The cache is rewritten when anything changed; a failure to
    /// write it is only logged.
    pub fn load(layout: &LibraryLayout) -> (Self, LoadReport) {
        let started = Instant::now();
        let listing = FolderListing::read(layout);
        let cached = fs::read(layout.index_file())
            .ok()
            .and_then(|bytes| IndexFileV1::parse(&bytes));

        let mut report = LoadReport {
            rebuilt: cached.is_none(),
            ..LoadReport::default()
        };
        let mut index = Self::default();
        for entry in cached.map(|file| file.items).unwrap_or_default() {
            if index.items.contains_key(&entry.id) || entry.base != base_name(&entry.id) {
                report.corrected += 1;
                continue;
            }
            if !listing.originals.contains(&entry.id) {
                report.dropped += 1;
                continue;
            }
            let (item, corrected) = item_from_entry(layout, &listing, entry);
            report.corrected += usize::from(corrected);
            index.items.insert(item.id.clone(), item);
        }
        for id in &listing.originals {
            if index.items.contains_key(id) {
                continue;
            }
            if let Some(item) = item_from_file(layout, &listing, id) {
                if !report.rebuilt {
                    report.imported += 1;
                }
                index.items.insert(id.clone(), item);
            }
        }

        // A folder that does not exist yet gets its cache with the first save.
        if report.changed() && layout.tmp_dir().is_dir() {
            if let Err(e) = index.persist(layout) {
                log::warn!("library: could not write the index: {e}");
            }
        }
        log::info!(
            "library: loaded {} captures from {} in {:?} (rebuilt: {}, imported: {}, dropped: {}, corrected: {})",
            index.items.len(),
            layout.root().display(),
            started.elapsed(),
            report.rebuilt,
            report.imported,
            report.dropped,
            report.corrected,
        );
        (index, report)
    }

    /// Newest first: `captured_at` descending, then id descending.
    pub fn items_newest_first(&self) -> Vec<CaptureItem> {
        let mut items: Vec<CaptureItem> = self.items.values().cloned().collect();
        items.sort_by(|a, b| {
            b.captured_at
                .cmp(&a.captured_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        items
    }

    pub fn get(&self, id: &CaptureId) -> Option<&CaptureItem> {
        self.items.get(id)
    }

    pub fn contains(&self, id: &CaptureId) -> bool {
        self.items.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Adds or replaces the item with the same id.
    pub fn upsert(&mut self, item: CaptureItem) {
        self.items.insert(item.id.clone(), item);
    }

    /// Removes the ids and returns those that were present, in the given order.
    pub fn remove(&mut self, ids: &[CaptureId]) -> Vec<CaptureId> {
        ids.iter()
            .filter(|id| self.items.remove(id).is_some())
            .cloned()
            .collect()
    }

    /// The cache contents, newest first.
    pub fn snapshot_for_persist(&self) -> IndexFileV1 {
        IndexFileV1 {
            schema_version: INDEX_SCHEMA_VERSION,
            items: self
                .items_newest_first()
                .into_iter()
                .map(|item| IndexEntry {
                    base: base_name(&item.id),
                    has_annotated_png: item.annotated_path.is_some(),
                    thumbnail_version: if item.thumbnail_path.is_some() {
                        item.thumbnail_version.max(1)
                    } else {
                        0
                    },
                    id: item.id,
                    captured_at: item.captured_at,
                    mode: item.mode,
                    width: item.width,
                    height: item.height,
                    window_title: item.window_title,
                    has_annotations: item.has_annotations,
                })
                .collect(),
        }
    }

    /// Writes the cache atomically. `.snipt/tmp` must exist.
    pub fn persist(&self, layout: &LibraryLayout) -> Result<(), LibraryError> {
        write_snapshot(layout, &self.snapshot_for_persist())
    }
}

/// Writes a snapshot taken under a lock, so the write itself can happen outside it.
pub fn write_snapshot(layout: &LibraryLayout, snapshot: &IndexFileV1) -> Result<(), LibraryError> {
    write_replace(
        &layout.tmp_dir(),
        &layout.index_file(),
        &snapshot.to_bytes(),
    )
}

/// An item from a cache entry whose original exists, with its flags corrected from the
/// listing. The second value tells whether anything was corrected.
fn item_from_entry(
    layout: &LibraryLayout,
    listing: &FolderListing,
    entry: IndexEntry,
) -> (CaptureItem, bool) {
    let id = entry.id;
    let has_annotated = listing.annotated.contains(&id);
    let has_annotations = listing.annotations.contains(&id);
    let has_thumbnail = listing.thumbnails.contains(&id);
    let thumbnail_version = match (has_thumbnail, entry.thumbnail_version) {
        (false, _) => 0,
        (true, 0) => modified_millis(&layout.thumbnail(&id)),
        (true, version) => version,
    };
    let corrected = has_annotated != entry.has_annotated_png
        || has_annotations != entry.has_annotations
        || has_thumbnail != (entry.thumbnail_version > 0);
    let item = CaptureItem {
        original_path: layout.original(&id),
        annotated_path: has_annotated.then(|| layout.annotated(&id)),
        has_annotations,
        thumbnail_path: has_thumbnail.then(|| layout.thumbnail(&id)),
        thumbnail_version,
        captured_at: entry.captured_at,
        mode: entry.mode,
        width: entry.width,
        height: entry.height,
        window_title: entry.window_title,
        id,
    };
    (item, corrected)
}

/// An item from the original's PNG header. `None` for PNGs without Snipt metadata or that
/// cannot be read.
fn item_from_file(
    layout: &LibraryLayout,
    listing: &FolderListing,
    id: &CaptureId,
) -> Option<CaptureItem> {
    let path = layout.original(id);
    let (width, height, meta) = match read_header(&path) {
        Ok(header) => header,
        Err(e) => {
            log::warn!("library: skipping {}: {e}", path.display());
            return None;
        }
    };
    let Some(meta) = meta else {
        log::info!(
            "library: skipping {} without Snipt metadata",
            path.display()
        );
        return None;
    };
    let has_thumbnail = listing.thumbnails.contains(id);
    Some(CaptureItem {
        id: id.clone(),
        captured_at: meta.captured_at,
        mode: meta.mode,
        width,
        height,
        window_title: meta.window_title,
        original_path: path,
        annotated_path: listing.annotated.contains(id).then(|| layout.annotated(id)),
        has_annotations: listing.annotations.contains(id),
        thumbnail_path: has_thumbnail.then(|| layout.thumbnail(id)),
        thumbnail_version: if has_thumbnail {
            modified_millis(&layout.thumbnail(id))
        } else {
            0
        },
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::capture_library::png_meta::{encode, SniptMetaV1};
    use crate::capture_library::types::fixtures::{id, item};
    use crate::capture_library::types::CaptureImage;
    use crate::platform::image_formats::png_from_rgba;

    fn at(rfc3339: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(rfc3339).unwrap()
    }

    fn item_at(id_str: &str, captured_at: &str) -> CaptureItem {
        CaptureItem {
            captured_at: at(captured_at),
            ..item(id_str)
        }
    }

    fn ids(items: &[CaptureItem]) -> Vec<&str> {
        items.iter().map(|item| item.id.as_str()).collect()
    }

    #[test]
    fn items_are_newest_first_with_ties_broken_by_id() {
        let mut index = LibraryIndex::default();
        index.upsert(item_at("20261022_174150", "2026-10-22T17:41:50+09:00"));
        index.upsert(item_at("20261022_174150_02", "2026-10-22T17:41:50+09:00"));
        index.upsert(item_at("20261023_090000", "2026-10-23T09:00:00+09:00"));
        // Earlier instant despite the later wall-clock time.
        index.upsert(item_at("20261022_180000", "2026-10-22T18:00:00+12:00"));

        assert_eq!(
            ids(&index.items_newest_first()),
            [
                "20261023_090000",
                "20261022_174150_02",
                "20261022_174150",
                "20261022_180000",
            ]
        );
    }

    #[test]
    fn upsert_replaces_and_remove_reports_what_was_present() {
        let mut index = LibraryIndex::default();
        index.upsert(item("20261022_174150"));
        index.upsert(item("20261022_174151"));
        let mut updated = item("20261022_174150");
        updated.has_annotations = true;
        index.upsert(updated.clone());

        assert_eq!(index.len(), 2);
        assert_eq!(index.get(&id("20261022_174150")), Some(&updated));
        let removed = index.remove(&[id("20261022_174151"), id("20261022_235959")]);
        assert_eq!(removed, [id("20261022_174151")]);
        assert!(!index.contains(&id("20261022_174151")));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn index_file_round_trips_without_paths() {
        let mut index = LibraryIndex::default();
        let mut annotated = item_at("20261022_174150", "2026-10-22T17:41:50.123+09:00");
        annotated.window_title = Some("無題 - メモ帳".to_owned());
        annotated.annotated_path = Some(PathBuf::from("x"));
        annotated.has_annotations = true;
        annotated.thumbnail_path = Some(PathBuf::from("y"));
        annotated.thumbnail_version = 42;
        index.upsert(annotated);
        index.upsert(item_at("20261022_174151", "2026-10-22T17:41:51+09:00"));

        let snapshot = index.snapshot_for_persist();
        let bytes = snapshot.to_bytes();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(!text.contains("shots"), "no absolute paths: {text}");
        assert_eq!(IndexFileV1::parse(&bytes), Some(snapshot.clone()));

        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["items"][0]["id"], "20261022_174151");
        assert_eq!(value["items"][1]["base"], "Snipt_20261022_174150");
        assert_eq!(value["items"][1]["hasAnnotatedPng"], true);
        assert_eq!(value["items"][1]["thumbnailVersion"], 42);
        assert_eq!(value["items"][0]["thumbnailVersion"], 0);
    }

    #[test]
    fn unknown_versions_and_garbage_do_not_parse() {
        assert_eq!(
            IndexFileV1::parse(b"{\"schemaVersion\":2,\"items\":[]}"),
            None
        );
        assert_eq!(IndexFileV1::parse(b"{\"items\":[]}"), None);
        assert_eq!(IndexFileV1::parse(b"{\"schemaVersion\":1"), None);
        assert_eq!(
            IndexFileV1::parse(b"{\"schemaVersion\":1,\"items\":[{\"id\":\"..\\\\x\"}]}"),
            None
        );
        assert!(IndexFileV1::parse(b"{\"schemaVersion\":1,\"items\":[]}").is_some());
    }

    // --- load / reconcile ---

    struct Folder {
        _dir: tempfile::TempDir,
        layout: LibraryLayout,
    }

    fn folder() -> Folder {
        let dir = tempfile::tempdir().unwrap();
        let layout = LibraryLayout::new(dir.path().join("Snipt"));
        layout.ensure_dirs().unwrap();
        Folder { _dir: dir, layout }
    }

    fn meta(id_str: &str, captured_at: &str) -> SniptMetaV1 {
        SniptMetaV1 {
            v: 1,
            id: id(id_str),
            captured_at: at(captured_at),
            mode: CaptureMode::Window,
            window_title: Some("タイトル".to_owned()),
        }
    }

    /// Writes `Snipt_<id>.png` (3x2) with metadata.
    fn write_original(layout: &LibraryLayout, id_str: &str, captured_at: &str) {
        let image = CaptureImage::Rgba {
            width: 3,
            height: 2,
            pixels: vec![255; 24],
        };
        let png = encode(&image, &meta(id_str, captured_at)).unwrap();
        fs::write(layout.original(&id(id_str)), png).unwrap();
    }

    fn read_cache(layout: &LibraryLayout) -> IndexFileV1 {
        IndexFileV1::parse(&fs::read(layout.index_file()).unwrap()).unwrap()
    }

    #[test]
    fn rebuilds_from_the_files_when_there_is_no_cache() {
        let f = folder();
        write_original(&f.layout, "20261022_174150", "2026-10-22T17:41:50+09:00");
        write_original(&f.layout, "20261022_174151", "2026-10-22T17:41:51+09:00");
        let a = id("20261022_174150");
        fs::write(f.layout.annotated(&a), b"png").unwrap();
        fs::write(f.layout.annotations(&a), b"{}").unwrap();
        fs::write(f.layout.thumbnail(&a), b"png").unwrap();
        // Ignored: no metadata, not a Snipt name, an orphan annotated PNG.
        fs::write(
            f.layout.original(&id("20261022_000000")),
            png_from_rgba(1, 1, &[0; 4]).unwrap(),
        )
        .unwrap();
        fs::write(f.layout.root().join("other.png"), b"x").unwrap();
        fs::write(f.layout.annotated(&id("20261022_111111")), b"x").unwrap();

        let (index, report) = LibraryIndex::load(&f.layout);

        assert!(report.rebuilt);
        let items = index.items_newest_first();
        assert_eq!(ids(&items), ["20261022_174151", "20261022_174150"]);
        let first = &items[1];
        assert_eq!((first.width, first.height), (3, 2));
        assert_eq!(first.mode, CaptureMode::Window);
        assert_eq!(first.window_title.as_deref(), Some("タイトル"));
        assert_eq!(first.original_path, f.layout.original(&a));
        assert_eq!(first.annotated_path, Some(f.layout.annotated(&a)));
        assert!(first.has_annotations);
        assert_eq!(first.thumbnail_path, Some(f.layout.thumbnail(&a)));
        assert!(first.thumbnail_version > 0);
        assert_eq!(items[0].annotated_path, None);
        assert_eq!(items[0].thumbnail_version, 0);
        assert_eq!(read_cache(&f.layout), index.snapshot_for_persist());
    }

    #[test]
    fn rebuilds_a_corrupt_or_unknown_version_cache() {
        for cache in [&b"{not json"[..], b"{\"schemaVersion\":99,\"items\":[]}"] {
            let f = folder();
            write_original(&f.layout, "20261022_174150", "2026-10-22T17:41:50+09:00");
            fs::write(f.layout.index_file(), cache).unwrap();

            let (index, report) = LibraryIndex::load(&f.layout);

            assert!(report.rebuilt);
            assert_eq!(ids(&index.items_newest_first()), ["20261022_174150"]);
            assert_eq!(read_cache(&f.layout).items.len(), 1);
        }
    }

    #[test]
    fn imports_new_originals_and_drops_missing_ones() {
        let f = folder();
        write_original(&f.layout, "20261022_174150", "2026-10-22T17:41:50+09:00");
        write_original(&f.layout, "20261022_174151", "2026-10-22T17:41:51+09:00");
        let (_, first) = LibraryIndex::load(&f.layout);
        assert!(first.rebuilt);

        // An unchanged folder loads from the cache without rewriting it.
        let written = fs::metadata(f.layout.index_file())
            .unwrap()
            .modified()
            .unwrap();
        let (_, again) = LibraryIndex::load(&f.layout);
        assert_eq!(again, LoadReport::default());
        assert_eq!(
            fs::metadata(f.layout.index_file())
                .unwrap()
                .modified()
                .unwrap(),
            written
        );

        fs::remove_file(f.layout.original(&id("20261022_174150"))).unwrap();
        write_original(&f.layout, "20261022_174152", "2026-10-22T17:41:52+09:00");
        let (index, report) = LibraryIndex::load(&f.layout);

        assert_eq!(
            report,
            LoadReport {
                rebuilt: false,
                imported: 1,
                dropped: 1,
                corrected: 0,
            }
        );
        assert_eq!(
            ids(&index.items_newest_first()),
            ["20261022_174152", "20261022_174151"]
        );
        assert_eq!(read_cache(&f.layout).items.len(), 2);
    }

    #[test]
    fn corrects_flags_from_the_files() {
        let f = folder();
        write_original(&f.layout, "20261022_174150", "2026-10-22T17:41:50+09:00");
        let a = id("20261022_174150");
        fs::write(f.layout.annotated(&a), b"png").unwrap();
        fs::write(f.layout.annotations(&a), b"{}").unwrap();
        LibraryIndex::load(&f.layout);

        fs::remove_file(f.layout.annotated(&a)).unwrap();
        fs::remove_file(f.layout.annotations(&a)).unwrap();
        fs::write(f.layout.thumbnail(&a), b"png").unwrap();
        let (index, report) = LibraryIndex::load(&f.layout);

        assert_eq!(report.corrected, 1);
        let item = index.get(&a).unwrap();
        assert_eq!(item.annotated_path, None);
        assert!(!item.has_annotations);
        assert_eq!(item.thumbnail_path, Some(f.layout.thumbnail(&a)));
        assert!(read_cache(&f.layout).items[0].thumbnail_version > 0);
    }

    #[test]
    fn cache_entries_with_a_foreign_base_are_ignored() {
        let f = folder();
        write_original(&f.layout, "20261022_174150", "2026-10-22T17:41:50+09:00");
        let mut snapshot = LibraryIndex::load(&f.layout).0.snapshot_for_persist();
        snapshot.items[0].base = "../elsewhere".to_owned();
        snapshot.items[0].width = 999;
        fs::write(f.layout.index_file(), snapshot.to_bytes()).unwrap();

        let (index, report) = LibraryIndex::load(&f.layout);

        assert_eq!(report.corrected, 1);
        assert_eq!(report.imported, 1);
        assert_eq!(index.get(&id("20261022_174150")).unwrap().width, 3);
    }

    #[test]
    fn switching_back_to_an_old_folder_restores_its_history() {
        let old = folder();
        let new = folder();
        write_original(&old.layout, "20261022_174150", "2026-10-22T17:41:50+09:00");
        write_original(&new.layout, "20261023_090000", "2026-10-23T09:00:00+09:00");

        let (first, _) = LibraryIndex::load(&old.layout);
        let (switched, _) = LibraryIndex::load(&new.layout);
        let (back, report) = LibraryIndex::load(&old.layout);

        assert_eq!(ids(&switched.items_newest_first()), ["20261023_090000"]);
        assert_eq!(back, first);
        assert!(!report.changed());
    }

    #[test]
    fn a_missing_folder_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let layout = LibraryLayout::new(dir.path().join("missing"));
        let (index, report) = LibraryIndex::load(&layout);
        assert!(index.is_empty());
        assert!(report.rebuilt);
    }
}
