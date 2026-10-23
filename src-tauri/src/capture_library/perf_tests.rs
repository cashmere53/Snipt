//! Performance measurements for task 10.2 (requirement 3.8). Ignored by default; run them in
//! release mode with `mise run perf:library`, which prints one line per measurement.
//!
//! The history window's own rendering is not measured here (it needs the WebView): the first
//! measurement covers what the backend does before the window can show its first rows, which
//! is opening the library, listing it and reading the first rows' thumbnail files.

use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Local, TimeZone};

use super::index::LibraryIndex;
use super::layout::LibraryLayout;
use super::ports::fake::RecordingDesktop;
use super::*;
use crate::shared::{CaptureMode, Notice, Notifier};

const CAPTURES: usize = 1_000;
/// Tiles a history window of the default size shows at first (5 rows of 4), plus one row
/// rendered ahead.
const FIRST_TILES: usize = 24;

struct Quiet;

impl Notifier for Quiet {
    fn notify(&self, _notice: Notice) {}
}

fn open(root: &std::path::Path, events: Arc<Mutex<Vec<LibraryEvent>>>) -> Arc<CaptureLibrary> {
    CaptureLibrary::open(LibraryDeps {
        save_dir: root.to_owned(),
        desktop: Box::new(Arc::new(RecordingDesktop::default())),
        notifier: Arc::new(Quiet),
        events: Box::new(move |event| events.lock().unwrap().push(event)),
        editor: Box::new(|| None),
        serve_thumbnails: Box::new(|_| {}),
    })
}

/// A screen-like image: flat areas with a gradient band, so PNG compression has realistic work.
fn screen(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let band = (y / 40) % 3 == 0;
            let value = if band { (x % 256) as u8 } else { 240 };
            pixels.extend_from_slice(&[value, value / 2, 255 - value, 255]);
        }
    }
    pixels
}

fn capture(index: usize, width: u32, height: u32, pixels: &[u8]) -> NewCapture {
    let start = Local.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap();
    NewCapture {
        image: CaptureImage::Rgba {
            width,
            height,
            pixels: pixels.to_vec(),
        },
        mode: CaptureMode::Region,
        captured_at: start + chrono::Duration::seconds(index as i64),
        window_title: None,
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[test]
#[ignore = "performance measurement; run with `mise run perf:library`"]
fn history_of_1000_captures() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Snipt");
    let events = Arc::new(Mutex::new(Vec::new()));
    let library = open(&root, events.clone());
    let pixels = screen(1280, 720);
    for i in 0..CAPTURES {
        library
            .save_capture(capture(i, 1280, 720, &pixels))
            .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let thumbnails = events
            .lock()
            .unwrap()
            .iter()
            .filter(
                |e| matches!(e, LibraryEvent::ItemUpdated(item) if item.thumbnail_path.is_some()),
            )
            .count();
        if thumbnails >= CAPTURES {
            break;
        }
        assert!(Instant::now() < deadline, "thumbnails: {thumbnails}");
        std::thread::sleep(Duration::from_millis(50));
    }
    library.flush_index();
    drop(library);

    // 1. With the index: open, list, and read the first rows' thumbnails.
    let started = Instant::now();
    let library = open(&root, Arc::default());
    let items = library.list();
    let mut bytes = 0;
    for item in items.iter().take(FIRST_TILES) {
        bytes += fs::read(item.thumbnail_path.as_ref().unwrap())
            .unwrap()
            .len();
    }
    let first_rows = started.elapsed();
    assert_eq!(items.len(), CAPTURES);
    println!(
        "perf: open + list {CAPTURES} + first {FIRST_TILES} thumbnails ({bytes} bytes): {:.1} ms",
        millis(first_rows)
    );
    drop(library);

    // 2. Without the index: rebuilt from the PNG headers in the folder.
    let layout = LibraryLayout::new(root.clone());
    fs::remove_file(layout.index_file()).unwrap();
    let started = Instant::now();
    let (index, report) = LibraryIndex::load(&layout);
    let rebuild = started.elapsed();
    assert!(report.rebuilt);
    assert_eq!(index.items_newest_first().len(), CAPTURES);
    println!(
        "perf: rebuild index of {CAPTURES} captures: {:.1} ms",
        millis(rebuild)
    );

    assert!(first_rows < Duration::from_secs(1), "{first_rows:?}");
}

#[test]
#[ignore = "performance measurement; run with `mise run perf:library`"]
fn saving_large_captures() {
    for (width, height) in [(3840, 2160), (7680, 2160)] {
        let dir = tempfile::tempdir().unwrap();
        let library = open(&dir.path().join("Snipt"), Arc::default());
        let pixels = screen(width, height);
        // The first save also creates the folders.
        library
            .save_capture(capture(0, width, height, &pixels))
            .unwrap();
        let mut times = Vec::new();
        for i in 1..=5 {
            let new = capture(i, width, height, &pixels);
            let started = Instant::now();
            library.save_capture(new).unwrap();
            times.push(started.elapsed());
        }
        times.sort();
        println!(
            "perf: save {width}x{height}: median {:.1} ms (min {:.1}, max {:.1})",
            millis(times[2]),
            millis(times[0]),
            millis(times[4])
        );
    }
}
