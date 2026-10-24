//! Freezing every monitor: BitBlt from the screen into a top-down DIB section per monitor
//! (screen-capture 5.2). `SRCCOPY` without `CAPTUREBLT`: the cursor is not drawn, and layered
//! windows still are on Windows 8 and later (DWM composes them into the screen).

use std::ffi::c_void;
use std::ptr;

use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    HGDIOBJ, SRCCOPY,
};

use crate::screen_capture::compose::FrozenFrame;
use crate::screen_capture::error::CaptureError;
use crate::screen_capture::topology::{MonitorInfo, MonitorTopology};

/// A 32 bpp top-down BGRA bitmap of `width × height`, rows of `width * 4` bytes.
pub fn top_down_bgra(width: u32, height: u32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // Negative: the first row is the top one.
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    }
}

/// One frame per monitor of `topology`, in its order.
pub fn grab_monitors(topology: &MonitorTopology) -> Result<Vec<FrozenFrame>, CaptureError> {
    let screen = ScreenDc::get()?;
    topology
        .monitors
        .iter()
        .map(|monitor| {
            grab(&screen, monitor).map_err(|reason| CaptureError::GrabFailed {
                monitor: monitor.index,
                reason,
            })
        })
        .collect()
}

fn grab(screen: &ScreenDc, monitor: &MonitorInfo) -> Result<FrozenFrame, String> {
    let rect = monitor.rect;
    let canvas = Canvas::new(screen.dc(), rect.width, rect.height)?;
    // SAFETY: both DCs are alive; the source rectangle is the monitor on the screen DC.
    unsafe {
        BitBlt(
            canvas.dc(),
            0,
            0,
            rect.width as i32,
            rect.height as i32,
            Some(screen.dc()),
            rect.x,
            rect.y,
            SRCCOPY,
        )
    }
    .map_err(|e| format!("BitBlt: {e}"))?;
    Ok(FrozenFrame {
        monitor: monitor.index,
        rect,
        stride: canvas.stride(),
        bgra: canvas.pixels().into(),
    })
}

/// The screen DC, released on drop.
pub(super) struct ScreenDc(HDC);

impl ScreenDc {
    pub(super) fn get() -> Result<Self, CaptureError> {
        // SAFETY: plain call; released on drop.
        let dc = unsafe { GetDC(None) };
        if dc.is_invalid() {
            return Err(CaptureError::Internal(
                "GetDC returned no screen DC".to_owned(),
            ));
        }
        Ok(Self(dc))
    }

    pub(super) fn dc(&self) -> HDC {
        self.0
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: the DC came from `GetDC(None)`.
        unsafe { ReleaseDC(None, self.0) };
    }
}

/// A memory DC with a top-down BGRA DIB section selected, to draw into and read back. Both
/// are freed on drop.
pub(super) struct Canvas {
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *const u8,
    width: u32,
    height: u32,
}

impl Canvas {
    /// A canvas of `width × height` compatible with `dc`.
    pub(super) fn new(dc: HDC, width: u32, height: u32) -> Result<Self, String> {
        // SAFETY: `dc` is alive.
        let memory = unsafe { CreateCompatibleDC(Some(dc)) };
        if memory.is_invalid() {
            return Err("CreateCompatibleDC failed".to_owned());
        }
        let info = top_down_bgra(width, height);
        let mut bits: *mut c_void = ptr::null_mut();
        // SAFETY: `info` describes a 32 bpp DIB and `bits` is valid for writes.
        let bitmap = match unsafe {
            CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        } {
            Ok(bitmap) if !bits.is_null() => bitmap,
            result => {
                // SAFETY: the DC and the bitmap (if any) are ours and selected nowhere.
                unsafe {
                    if let Ok(bitmap) = result {
                        let _ = DeleteObject(bitmap.into());
                    }
                    let _ = DeleteDC(memory);
                }
                return Err(format!(
                    "CreateDIBSection {width}x{height}: {}",
                    result
                        .err()
                        .map_or("no pixels".to_owned(), |e| e.to_string())
                ));
            }
        };
        // SAFETY: both are alive; the previous object is put back on drop.
        let previous = unsafe { SelectObject(memory, bitmap.into()) };
        Ok(Self {
            memory,
            bitmap,
            previous,
            bits: bits.cast(),
            width,
            height,
        })
    }

    pub(super) fn dc(&self) -> HDC {
        self.memory
    }

    pub(super) fn stride(&self) -> usize {
        self.width as usize * 4
    }

    /// The pixels drawn so far, top row first.
    pub(super) fn pixels(&self) -> &[u8] {
        // SAFETY: plain call; makes GDI finish drawing before the pixels are read.
        let _ = unsafe { GdiFlush() };
        // SAFETY: the DIB section holds `stride * height` bytes while `self` is alive.
        unsafe { std::slice::from_raw_parts(self.bits, self.stride() * self.height as usize) }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: the DC and the bitmap are ours; the bitmap is deselected before deletion.
        unsafe {
            SelectObject(self.memory, self.previous);
            let _ = DeleteDC(self.memory);
            let _ = DeleteObject(self.bitmap.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::windows::monitors::{topology, use_per_monitor_v2_on_this_thread};

    /// On real hardware: run with `--ignored`. Saves each frame as `target/grab-<n>.png`, to
    /// check by eye that the cursor is absent and the pixels are 1:1.
    #[test]
    #[ignore = "needs real monitors"]
    fn freezes_every_real_monitor_at_physical_size() {
        use_per_monitor_v2_on_this_thread();
        let topology = topology().unwrap();
        let frames = grab_monitors(&topology).unwrap();
        assert_eq!(frames.len(), topology.monitors.len());
        for (frame, monitor) in frames.iter().zip(&topology.monitors) {
            assert_eq!(frame.rect, monitor.rect);
            assert_eq!(
                frame.bgra.len(),
                frame.stride * monitor.rect.height as usize
            );
            let image = crate::screen_capture::compose::compose_region(
                std::slice::from_ref(frame),
                frame.rect,
            );
            let path = format!("target/grab-{}.png", monitor.index.0);
            image::save_buffer(
                &path,
                &image.rgba,
                image.width,
                image.height,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
            println!("{monitor:?} -> {path}");
        }
    }
}
