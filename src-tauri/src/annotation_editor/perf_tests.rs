//! Performance measurements for the editor's Ctrl+C (`editor_copy`). Ignored by default; run them
//! in release mode with `mise run perf:editor`, which prints one line per measurement.
//!
//! All of them use an 11520×2160 composite, the size of the E2E perf test. The copy is timed in
//! steps on that image (decode, DIB, clipboard), and once through `copy_png` into the real
//! clipboard. The clipboard is cleared afterwards, so the user's previous contents are lost.

use std::time::{Duration, Instant};

#[cfg(windows)]
use super::commands::copy_png;
use super::commands::decode_composite;
#[cfg(windows)]
use super::error::EditorError;
#[cfg(windows)]
use super::ports::{ClipboardImage, EditorDesktop};
use crate::platform::image_formats::{dibv5_from_rgba, png_from_rgba, BITMAPV5HEADER_SIZE};

const WIDTH: u32 = 11_520;
const HEIGHT: u32 = 2_160;
const RUNS: usize = 3;

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// The median of the sorted times, for an odd number of runs.
fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

/// A screen-like composite: a vertical gradient, a grid of UI-like blocks with text-like bars,
/// and a few filled shapes. Flat areas and edges give PNG real work, unlike a solid colour.
fn composite(width: u32, height: u32) -> Vec<u8> {
    const PALETTE: [[u8; 3]; 5] = [
        [250, 250, 250],
        [32, 96, 180],
        [240, 200, 80],
        [60, 160, 90],
        [200, 70, 70],
    ];
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for (y, row) in pixels.chunks_exact_mut(width as usize * 4).enumerate() {
        let y = y as u32;
        let gradient = (y * 200 / height) as u8;
        for (x, px) in row.chunks_exact_mut(4).enumerate() {
            let x = x as u32;
            let cell = ((x / 240) + (y / 120) * 7) as usize % PALETTE.len();
            let mut rgb = [gradient, 255 - gradient / 2, 240];
            if x % 240 > 16 && y % 120 > 16 {
                rgb = PALETTE[cell];
                // Text-like bars inside each block.
                if (y % 120) % 24 < 4 && (x * 7) % 13 < 6 {
                    rgb = [20, 20, 20];
                }
            }
            // A circle and a rectangle, drawn over the blocks.
            let (dx, dy) = (x as i64 - 3_840, y as i64 - 1_080);
            if dx * dx + dy * dy < 400 * 400 {
                rgb = [30, 30, 120];
            }
            if (7_000..7_600).contains(&x) && (400..1_700).contains(&y) {
                rgb = [255, 255, 255];
            }
            px.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    pixels
}

/// A composite as the WebView would send it: a PNG of the screen-like pixels.
fn composite_png() -> Vec<u8> {
    let png = png_from_rgba(WIDTH, HEIGHT, &composite(WIDTH, HEIGHT)).unwrap();
    println!(
        "annotation-editor perf: composite {WIDTH}x{HEIGHT} png = {} bytes ({:.1} MB raw RGBA)",
        png.len(),
        (WIDTH as usize * HEIGHT as usize * 4) as f64 / 1_000_000.0
    );
    png
}

#[test]
#[ignore = "performance measurement; run with `mise run perf:editor`"]
fn decode_11520x2160_composite() {
    let png = composite_png();
    let mut times = Vec::new();
    for _ in 0..RUNS {
        let started = Instant::now();
        let image = decode_composite(&png).unwrap();
        times.push(started.elapsed());
        assert_eq!((image.width(), image.height()), (WIDTH, HEIGHT));
    }
    println!(
        "annotation-editor perf: copy {WIDTH}x{HEIGHT} decode = {:.1} ms (median of {RUNS})",
        millis(median(times))
    );
}

#[test]
#[ignore = "performance measurement; run with `mise run perf:editor`"]
fn build_clipboard_payloads_11520x2160() {
    let png = composite_png();
    let image = decode_composite(&png).unwrap();
    let mut times = Vec::new();
    for _ in 0..RUNS {
        let started = Instant::now();
        let dib = dibv5_from_rgba(WIDTH, HEIGHT, image.as_raw());
        times.push(started.elapsed());
        assert_eq!(dib.len(), BITMAPV5HEADER_SIZE + image.as_raw().len());
    }
    // The PNG is not built on this path: the composite PNG is handed over as it is.
    println!(
        "annotation-editor perf: copy {WIDTH}x{HEIGHT} dib = {:.1} ms (median of {RUNS}, {} MB)",
        millis(median(times)),
        (BITMAPV5HEADER_SIZE + image.as_raw().len()) / 1_000_000
    );
}

/// The real clipboard behind the editor port, as `SystemEditorDesktop` uses it.
#[cfg(windows)]
struct WindowsClipboard;

#[cfg(windows)]
impl EditorDesktop for WindowsClipboard {
    fn copy_image(&self, image: ClipboardImage<'_>) -> Result<(), EditorError> {
        crate::platform::windows::clipboard::copy_image(image)
            .map_err(|e| EditorError::Clipboard(e.to_string()))
    }

    fn pick_save_path(
        &self,
        _parent_label: &str,
        _suggested_name: &str,
    ) -> Result<Option<std::path::PathBuf>, EditorError> {
        Err(EditorError::Dialog("not used by the copy".to_owned()))
    }
}

/// Empties the clipboard. Nothing is restored: the previous contents are not kept.
#[cfg(windows)]
fn clear_clipboard() {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
    // SAFETY: no owner window; the clipboard is closed again on the same thread.
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
    }
}

/// Clears the clipboard when dropped, so a panicking test does not leave the copy behind.
#[cfg(windows)]
struct ClearClipboardOnDrop;

#[cfg(windows)]
impl Drop for ClearClipboardOnDrop {
    fn drop(&mut self) {
        clear_clipboard();
    }
}

#[cfg(windows)]
#[test]
#[ignore = "performance measurement; run with `mise run perf:editor`"]
fn copy_11520x2160_to_clipboard() {
    use crate::platform::windows::clipboard::set_image_formats;

    let _clear = ClearClipboardOnDrop;
    let png = composite_png();
    let (mut decode, mut dib, mut clipboard) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..RUNS {
        let started = Instant::now();
        let image = decode_composite(&png).unwrap();
        decode.push(started.elapsed());

        let started = Instant::now();
        let payload = dibv5_from_rgba(WIDTH, HEIGHT, image.as_raw());
        dib.push(started.elapsed());

        let started = Instant::now();
        set_image_formats(&payload, &png).unwrap();
        clipboard.push(started.elapsed());
    }

    // The whole command path: what `editor_copy` runs on its blocking worker.
    let mut total = Vec::new();
    for _ in 0..RUNS {
        let started = Instant::now();
        copy_png(&WindowsClipboard, &png).unwrap();
        total.push(started.elapsed());
    }

    println!(
        "annotation-editor perf: copy {WIDTH}x{HEIGHT} decode = {:.1} ms, dib = {:.1} ms, clipboard = {:.1} ms, total = {:.1} ms (medians of {RUNS}; steps and total run separately)",
        millis(median(decode)),
        millis(median(dib)),
        millis(median(clipboard)),
        millis(median(total)),
    );
}
