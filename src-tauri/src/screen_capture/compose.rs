//! Cutting the captured image out of the frozen monitors. Pixels are copied one to one, never
//! scaled: a monitor at 150% gives 1.5 times as many pixels as one at 100% for the same area.
//! Region, single-monitor and all-monitor captures all come down to `compose_region`.

use std::sync::Arc;

use super::geometry::{PhysicalPoint, PhysicalRect};
use super::topology::MonitorIndex;

/// One monitor's frozen screen: top-down BGRA rows of `stride` bytes (at least `width * 4`).
#[derive(Clone, Debug)]
pub struct FrozenFrame {
    pub monitor: MonitorIndex,
    /// Global physical pixels: the monitor's rectangle when it was frozen.
    pub rect: PhysicalRect,
    pub stride: usize,
    pub bgra: Arc<[u8]>,
}

/// A captured image: RGBA rows without padding, straight (not premultiplied) alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The pixels around the cursor, for the magnifier.
pub type PixelPatch = CapturedImage;

/// The largest magnifier radius: a patch is at most 33 × 33 pixels.
pub const MAX_PATCH_RADIUS: u32 = 16;

/// The pixels of `global` from `frames`: opaque where a monitor is, fully transparent where
/// none is (between monitors of different sizes, for example).
pub fn compose_region(frames: &[FrozenFrame], global: PhysicalRect) -> CapturedImage {
    let width = global.width as usize;
    let mut rgba = vec![0u8; width * global.height as usize * 4];
    for frame in frames {
        let Some(part) = frame.rect.intersection(&global) else {
            continue;
        };
        // `part` lies inside both rectangles, so these offsets are not negative.
        let src_x = (part.left() - frame.rect.left()) as usize;
        let src_y = (part.top() - frame.rect.top()) as usize;
        let dst_x = (part.left() - global.left()) as usize;
        let dst_y = (part.top() - global.top()) as usize;
        let row_bytes = part.width as usize * 4;
        for row in 0..part.height as usize {
            let src_start = (src_y + row) * frame.stride + src_x * 4;
            // A short frame leaves the rest transparent rather than panicking.
            let Some(src) = frame.bgra.get(src_start..src_start + row_bytes) else {
                break;
            };
            let dst_start = ((dst_y + row) * width + dst_x) * 4;
            let dst = &mut rgba[dst_start..dst_start + row_bytes];
            for (to, from) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                to.copy_from_slice(&[from[2], from[1], from[0], 255]);
            }
        }
    }
    CapturedImage {
        width: global.width,
        height: global.height,
        rgba,
    }
}

/// The `(2r + 1)²` pixels centered on `center` (global), `r` at most `MAX_PATCH_RADIUS`;
/// transparent outside `frame`.
pub fn patch(frame: &FrozenFrame, center: PhysicalPoint, radius: u32) -> PixelPatch {
    let r = radius.min(MAX_PATCH_RADIUS);
    let side = 2 * r + 1;
    let r = r as i32;
    let around = PhysicalRect::new(
        center.x.saturating_sub(r),
        center.y.saturating_sub(r),
        side,
        side,
    );
    compose_region(std::slice::from_ref(frame), around)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame whose pixel at local `(x, y)` is BGRA `[x, y, index, 0]` (alpha as BitBlt
    /// leaves it), with `padding` extra bytes per row.
    fn frame(index: u32, rect: PhysicalRect, padding: usize) -> FrozenFrame {
        let stride = rect.width as usize * 4 + padding;
        let mut bgra = vec![0xEE; stride * rect.height as usize];
        for y in 0..rect.height as usize {
            for x in 0..rect.width as usize {
                let at = y * stride + x * 4;
                bgra[at..at + 4].copy_from_slice(&[x as u8, y as u8, index as u8, 0]);
            }
        }
        FrozenFrame {
            monitor: MonitorIndex(index),
            rect,
            stride,
            bgra: bgra.into(),
        }
    }

    /// The RGBA pixel at `(x, y)` of `image`.
    fn pixel(image: &CapturedImage, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        image.rgba[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn copies_a_region_pixel_for_pixel_as_opaque_rgba() {
        let frames = [frame(0, PhysicalRect::new(0, 0, 40, 30), 0)];
        let image = compose_region(&frames, PhysicalRect::new(10, 5, 20, 10));
        assert_eq!((image.width, image.height), (20, 10));
        assert_eq!(image.rgba.len(), 20 * 10 * 4);
        // BGRA [x, y, 0, _] becomes RGBA [0, y, x, 255].
        assert_eq!(pixel(&image, 0, 0), [0, 5, 10, 255]);
        assert_eq!(pixel(&image, 19, 9), [0, 14, 29, 255]);
    }

    #[test]
    fn keeps_physical_sizes_across_monitors_at_100_and_150_percent() {
        // A 150% monitor (60 physical px for 40 logical) left of a 100% one.
        let frames = [
            frame(0, PhysicalRect::new(-60, 0, 60, 45), 0),
            frame(1, PhysicalRect::new(0, 0, 40, 30), 0),
        ];
        let region = PhysicalRect::new(-60, 0, 60, 45);
        let left = compose_region(&frames, region);
        assert_eq!((left.width, left.height), (60, 45));
        assert_eq!(pixel(&left, 59, 44), [0, 44, 59, 255]);

        let across = compose_region(&frames, PhysicalRect::new(-5, 0, 10, 1));
        assert_eq!(pixel(&across, 4, 0), [0, 0, 59, 255]);
        assert_eq!(pixel(&across, 5, 0), [1, 0, 0, 255]);
    }

    #[test]
    fn leaves_the_area_without_monitors_transparent() {
        let frames = [
            frame(0, PhysicalRect::new(-60, 0, 60, 45), 0),
            frame(1, PhysicalRect::new(0, 0, 40, 30), 0),
        ];
        let all = PhysicalRect::new(-60, 0, 100, 45);
        let image = compose_region(&frames, all);
        assert_eq!((image.width, image.height), (100, 45));
        // Below the shorter monitor on the right.
        assert_eq!(pixel(&image, 70, 40), [0, 0, 0, 0]);
        assert_eq!(pixel(&image, 70, 29), [1, 29, 10, 255]);
        let transparent = image.rgba.chunks_exact(4).filter(|p| p[3] == 0).count();
        assert_eq!(transparent, 40 * 15);
    }

    #[test]
    fn reads_rows_with_a_padded_stride() {
        let frames = [frame(0, PhysicalRect::new(0, 0, 5, 4), 12)];
        let image = compose_region(&frames, PhysicalRect::new(0, 0, 5, 4));
        for y in 0..4 {
            for x in 0..5 {
                assert_eq!(pixel(&image, x, y), [0, y as u8, x as u8, 255]);
            }
        }
    }

    #[test]
    fn a_short_frame_leaves_its_missing_rows_transparent() {
        let mut short = frame(0, PhysicalRect::new(0, 0, 4, 4), 0);
        short.bgra = short.bgra[..4 * 4 * 2].into();
        let image = compose_region(&[short], PhysicalRect::new(0, 0, 4, 4));
        assert_eq!(pixel(&image, 3, 1), [0, 1, 3, 255]);
        assert_eq!(pixel(&image, 0, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn cuts_a_patch_around_a_point_transparent_past_the_edge() {
        let f = frame(0, PhysicalRect::new(100, 100, 50, 50), 0);
        let patch = patch(&f, PhysicalPoint::new(100, 120), 2);
        assert_eq!((patch.width, patch.height), (5, 5));
        // The center is local (0, 20); the two columns left of it are off the monitor.
        assert_eq!(pixel(&patch, 2, 2), [0, 20, 0, 255]);
        assert_eq!(pixel(&patch, 1, 2), [0, 0, 0, 0]);
        let largest = super::patch(&f, PhysicalPoint::new(125, 125), 100);
        assert_eq!(largest.width, 2 * MAX_PATCH_RADIUS + 1);
    }
}
