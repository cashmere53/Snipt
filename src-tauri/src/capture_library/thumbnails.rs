//! `ThumbnailWorker`: makes `.snipt/thumbs/<id>.png` (longest side at most 320px, aspect ratio
//! and transparency kept) on a background thread.
//!
//! Requests for the same id are merged (the latest source wins), and the newest capture is
//! made first so the top of the history fills in before the rest. Generation overwrites, so
//! repeating a request is harmless.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset};
use image::RgbaImage;

use super::atomic_fs::write_replace;
use super::types::LibraryError;
use crate::platform::image_formats::png_from_rgba;
use crate::shared::CaptureId;

pub const THUMBNAIL_MAX_SIDE: u32 = 320;

/// Where the pixels come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThumbnailSource {
    /// Straight RGBA8 still in memory right after a save.
    Rgba {
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
    },
    /// PNG bytes still in memory (an annotated PNG right after it was saved).
    Png(Arc<Vec<u8>>),
    /// The item's `display_path()`.
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThumbnailJob {
    pub id: CaptureId,
    /// Orders the queue, newest first.
    pub captured_at: DateTime<FixedOffset>,
    pub source: ThumbnailSource,
    pub dest: PathBuf,
    /// `.snipt/tmp` of the same folder, for the atomic write.
    pub tmp_dir: PathBuf,
}

/// A finished job: the new `thumbnail_version` (milliseconds) or why it failed.
pub type ThumbnailResult = Result<u64, LibraryError>;
pub type OnThumbnailDone = Box<dyn Fn(&ThumbnailJob, ThumbnailResult) + Send + Sync>;

/// Pending jobs by id. Pure, so the order and merging can be tested without a thread.
#[derive(Debug, Default)]
pub struct ThumbnailQueue {
    jobs: HashMap<CaptureId, ThumbnailJob>,
}

impl ThumbnailQueue {
    /// Adds a job, replacing a pending one for the same id.
    pub fn push(&mut self, job: ThumbnailJob) {
        self.jobs.insert(job.id.clone(), job);
    }

    /// The newest pending job (`captured_at`, then id, descending).
    pub fn pop_newest(&mut self) -> Option<ThumbnailJob> {
        let newest = self
            .jobs
            .values()
            .max_by(|a, b| {
                a.captured_at
                    .cmp(&b.captured_at)
                    .then_with(|| a.id.cmp(&b.id))
            })?
            .id
            .clone();
        self.jobs.remove(&newest)
    }

    /// Drops pending jobs, e.g. for captures that were deleted or a folder that was left.
    pub fn retain(&mut self, keep: impl Fn(&ThumbnailJob) -> bool) {
        self.jobs.retain(|_, job| keep(job));
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[derive(Default)]
struct Shared {
    queue: ThumbnailQueue,
    closed: bool,
}

/// Owns the background thread; dropping the worker finishes the current job and stops it.
pub struct ThumbnailWorker {
    state: Arc<(Mutex<Shared>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl ThumbnailWorker {
    pub fn start(on_done: OnThumbnailDone) -> Self {
        let state = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let thread = std::thread::Builder::new()
            .name("snipt-thumbnails".to_owned())
            .spawn({
                let state = state.clone();
                move || run(&state, &on_done)
            })
            .expect("spawning the thumbnail thread");
        Self {
            state,
            thread: Some(thread),
        }
    }

    pub fn request(&self, job: ThumbnailJob) {
        let (lock, wake) = &*self.state;
        lock.lock().unwrap().queue.push(job);
        wake.notify_one();
    }

    /// Drops pending jobs that `keep` rejects; a job already running still finishes.
    pub fn retain(&self, keep: impl Fn(&ThumbnailJob) -> bool) {
        self.state.0.lock().unwrap().queue.retain(keep);
    }
}

impl Drop for ThumbnailWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.state;
        lock.lock().unwrap().closed = true;
        wake.notify_all();
        if let Some(thread) = self.thread.take() {
            // The last owner may be dropped from inside `on_done`; the thread then ends by
            // itself on its next turn.
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}

fn run(state: &(Mutex<Shared>, Condvar), on_done: &OnThumbnailDone) {
    let (lock, wake) = state;
    loop {
        let job = {
            let mut shared = lock.lock().unwrap();
            loop {
                if shared.closed {
                    return;
                }
                if let Some(job) = shared.queue.pop_newest() {
                    break job;
                }
                shared = wake.wait(shared).unwrap();
            }
        };
        let result = generate(&job);
        if let Err(e) = &result {
            log::warn!("library: thumbnail for {} failed: {e}", job.id);
        }
        on_done(&job, result);
    }
}

/// Makes and writes one thumbnail.
pub fn generate(job: &ThumbnailJob) -> ThumbnailResult {
    let image = decode(&job.source)?;
    let small = shrink(&image);
    let png = png_from_rgba(small.width(), small.height(), small.as_raw()).map_err(|e| {
        LibraryError::InvalidImage {
            message: e.to_string(),
        }
    })?;
    write_replace(&job.tmp_dir, &job.dest, &png)?;
    Ok(now_millis())
}

fn decode(source: &ThumbnailSource) -> Result<RgbaImage, LibraryError> {
    let invalid = |message: String| LibraryError::InvalidImage { message };
    match source {
        ThumbnailSource::Rgba {
            width,
            height,
            pixels,
        } => RgbaImage::from_raw(*width, *height, pixels.to_vec())
            .ok_or_else(|| invalid("画素データの長さが合いません".to_owned())),
        ThumbnailSource::Png(bytes) => {
            image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
                .map(|image| image.into_rgba8())
                .map_err(|e| invalid(e.to_string()))
        }
        ThumbnailSource::File(path) => {
            let bytes = std::fs::read(path).map_err(|e| super::atomic_fs::fs_error(path, &e))?;
            decode(&ThumbnailSource::Png(Arc::new(bytes)))
        }
    }
}

/// The size that fits in `THUMBNAIL_MAX_SIDE` with the aspect ratio kept. Never enlarges and
/// never goes below 1px.
pub fn thumbnail_size(width: u32, height: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= THUMBNAIL_MAX_SIDE {
        return (width, height);
    }
    let scale = |side: u32| {
        ((u64::from(side) * u64::from(THUMBNAIL_MAX_SIDE) + u64::from(longest) / 2)
            / u64::from(longest))
        .max(1) as u32
    };
    (scale(width), scale(height))
}

fn shrink(image: &RgbaImage) -> RgbaImage {
    let (width, height) = thumbnail_size(image.width(), image.height());
    if (width, height) == image.dimensions() {
        return image.clone();
    }
    image::imageops::thumbnail(image, width, height)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_millis() as u64)
        .max(1)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::capture_library::types::fixtures::id;

    fn at(rfc3339: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(rfc3339).unwrap()
    }

    fn job(
        id_str: &str,
        captured_at: &str,
        source: ThumbnailSource,
        dir: &std::path::Path,
    ) -> ThumbnailJob {
        ThumbnailJob {
            id: id(id_str),
            captured_at: at(captured_at),
            source,
            dest: dir.join(format!("{id_str}.png")),
            tmp_dir: dir.to_owned(),
        }
    }

    fn rgba(width: u32, height: u32, pixel: [u8; 4]) -> ThumbnailSource {
        ThumbnailSource::Rgba {
            width,
            height,
            pixels: Arc::new(pixel.repeat((width * height) as usize)),
        }
    }

    fn read(path: &std::path::Path) -> RgbaImage {
        image::open(path).unwrap().into_rgba8()
    }

    #[test]
    fn sizes_fit_the_longest_side() {
        assert_eq!(thumbnail_size(1920, 1080), (320, 180));
        assert_eq!(thumbnail_size(1080, 1920), (180, 320));
        assert_eq!(thumbnail_size(7680, 2160), (320, 90));
        assert_eq!(thumbnail_size(10_000, 1), (320, 1));
        assert_eq!(thumbnail_size(200, 100), (200, 100));
        assert_eq!(thumbnail_size(320, 320), (320, 320));
    }

    #[test]
    fn shrinks_landscape_and_portrait_images() {
        let dir = tempfile::tempdir().unwrap();
        let wide = job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            rgba(1600, 900, [10, 20, 30, 255]),
            dir.path(),
        );
        let tall = job(
            "20261022_174151",
            "2026-10-22T17:41:51+09:00",
            rgba(400, 1000, [10, 20, 30, 255]),
            dir.path(),
        );

        assert!(generate(&wide).unwrap() > 0);
        generate(&tall).unwrap();

        let wide = read(&wide.dest);
        assert_eq!(wide.dimensions(), (320, 180));
        assert_eq!(wide.get_pixel(5, 5).0, [10, 20, 30, 255]);
        assert_eq!(read(&tall.dest).dimensions(), (128, 320));
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            2,
            "no temporary files"
        );
    }

    #[test]
    fn keeps_transparency() {
        let dir = tempfile::tempdir().unwrap();
        let png = png_from_rgba(640, 640, &[200, 0, 0, 0].repeat(640 * 640)).unwrap();
        let from_png = job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            ThumbnailSource::Png(Arc::new(png.clone())),
            dir.path(),
        );
        let source = dir.path().join("source.png");
        std::fs::write(&source, &png).unwrap();
        let from_file = job(
            "20261022_174151",
            "2026-10-22T17:41:51+09:00",
            ThumbnailSource::File(source),
            dir.path(),
        );

        generate(&from_png).unwrap();
        generate(&from_file).unwrap();

        for thumb in [read(&from_png.dest), read(&from_file.dest)] {
            assert_eq!(thumb.dimensions(), (320, 320));
            assert!(thumb.pixels().all(|p| p.0[3] == 0));
        }
    }

    #[test]
    fn unreadable_sources_fail() {
        let dir = tempfile::tempdir().unwrap();
        let broken = job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            ThumbnailSource::Png(Arc::new(b"nope".to_vec())),
            dir.path(),
        );
        assert_eq!(generate(&broken).unwrap_err().kind(), "invalidImage");
        let missing = job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            ThumbnailSource::File(dir.path().join("missing.png")),
            dir.path(),
        );
        assert_eq!(generate(&missing).unwrap_err().kind(), "io");
        let short = job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            ThumbnailSource::Rgba {
                width: 2,
                height: 2,
                pixels: Arc::new(vec![0; 3]),
            },
            dir.path(),
        );
        assert_eq!(generate(&short).unwrap_err().kind(), "invalidImage");
    }

    #[test]
    fn queue_merges_requests_and_pops_newest_first() {
        let dir = std::path::Path::new("thumbs");
        let mut queue = ThumbnailQueue::default();
        queue.push(job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            rgba(1, 1, [0; 4]),
            dir,
        ));
        queue.push(job(
            "20261023_090000",
            "2026-10-23T09:00:00+09:00",
            rgba(1, 1, [0; 4]),
            dir,
        ));
        queue.push(job(
            "20261022_174150_02",
            "2026-10-22T17:41:50+09:00",
            rgba(1, 1, [0; 4]),
            dir,
        ));
        let latest = job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            rgba(2, 2, [0; 4]),
            dir,
        );
        queue.push(latest.clone());

        assert_eq!(queue.len(), 3);
        let order: Vec<_> = std::iter::from_fn(|| queue.pop_newest()).collect();
        assert_eq!(
            order.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(),
            ["20261023_090000", "20261022_174150_02", "20261022_174150"]
        );
        assert_eq!(order[2], latest, "the latest source wins");
        assert!(queue.is_empty());
    }

    #[test]
    fn retain_drops_pending_jobs() {
        let dir = std::path::Path::new("thumbs");
        let mut queue = ThumbnailQueue::default();
        queue.push(job(
            "20261022_174150",
            "2026-10-22T17:41:50+09:00",
            rgba(1, 1, [0; 4]),
            dir,
        ));
        queue.push(job(
            "20261022_174151",
            "2026-10-22T17:41:51+09:00",
            rgba(1, 1, [0; 4]),
            dir,
        ));
        queue.retain(|j| j.id.as_str() != "20261022_174151");
        assert_eq!(queue.pop_newest().unwrap().id.as_str(), "20261022_174150");
        assert!(queue.pop_newest().is_none());
    }

    #[test]
    fn worker_generates_merged_requests_once_and_reports_each() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let worker = ThumbnailWorker::start(Box::new({
            let gate = gate.clone();
            move |job, result| {
                tx.lock().unwrap().send((job.id.clone(), result)).unwrap();
                // Hold the thread after the first job so the next requests pile up.
                let (open, opened) = &*gate;
                let mut open = open.lock().unwrap();
                while !*open {
                    open = opened.wait(open).unwrap();
                }
            }
        }));
        let blocker = job(
            "20261022_000000",
            "2026-10-22T00:00:00+09:00",
            rgba(1, 1, [0; 4]),
            dir.path(),
        );
        worker.request(blocker);
        let (first, _) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first.as_str(), "20261022_000000");

        for _ in 0..3 {
            worker.request(job(
                "20261022_174150",
                "2026-10-22T17:41:50+09:00",
                rgba(640, 480, [1, 2, 3, 255]),
                dir.path(),
            ));
        }
        worker.request(job(
            "20261022_174151",
            "2026-10-22T17:41:51+09:00",
            rgba(10, 10, [0; 4]),
            dir.path(),
        ));
        {
            let (open, opened) = &*gate;
            *open.lock().unwrap() = true;
            opened.notify_all();
        }

        let mut done = Vec::new();
        for _ in 0..2 {
            let (id, result) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(result.unwrap() > 0);
            done.push(id);
        }
        drop(worker);
        assert!(rx.try_recv().is_err(), "merged requests run once");
        assert_eq!(
            done.iter().map(CaptureId::as_str).collect::<Vec<_>>(),
            ["20261022_174151", "20261022_174150"]
        );
        assert_eq!(
            read(&dir.path().join("20261022_174150.png")).dimensions(),
            (320, 240)
        );
    }
}
