//! Freezing every monitor: BitBlt from the screen into a top-down DIB section per monitor
//! (screen-capture 5.2). `SRCCOPY` without `CAPTUREBLT`: the cursor is not drawn, and layered
//! windows still are on Windows 8 and later (DWM composes them into the screen).

use std::ffi::c_void;
use std::mem::size_of;
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
    let (width, height) = (rect.width as i32, rect.height as i32);
    let stride = rect.width as usize * 4;
    let memory = MemoryDc::compatible_with(screen.0)?;
    let info = top_down_bgra(rect.width, rect.height);
    let mut bits: *mut c_void = ptr::null_mut();
    // SAFETY: `info` describes a 32 bpp DIB and `bits` is valid for writes.
    let bitmap =
        unsafe { CreateDIBSection(Some(screen.0), &info, DIB_RGB_COLORS, &mut bits, None, 0) }
            .map_err(|e| format!("CreateDIBSection: {e}"))?;
    let bitmap = Bitmap(bitmap);
    if bits.is_null() {
        return Err("CreateDIBSection returned no pixels".to_owned());
    }
    // SAFETY: `memory` and `bitmap` are alive; the old object is put back below.
    let previous = unsafe { SelectObject(memory.0, bitmap.0.into()) };
    // SAFETY: both DCs are alive; the source rectangle is the monitor on the screen DC.
    let copied = unsafe {
        BitBlt(
            memory.0,
            0,
            0,
            width,
            height,
            Some(screen.0),
            rect.x,
            rect.y,
            SRCCOPY,
        )
    };
    // SAFETY: as above.
    unsafe {
        let _ = GdiFlush();
        SelectObject(memory.0, previous);
    }
    copied.map_err(|e| format!("BitBlt: {e}"))?;
    // SAFETY: the DIB section holds `stride * height` bytes until `bitmap` is dropped, and
    // GDI has finished writing (GdiFlush).
    let pixels =
        unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), stride * rect.height as usize) };
    Ok(FrozenFrame {
        monitor: monitor.index,
        rect,
        stride,
        bgra: pixels.into(),
    })
}

/// The screen DC, released on drop.
struct ScreenDc(HDC);

impl ScreenDc {
    fn get() -> Result<Self, CaptureError> {
        // SAFETY: plain call; released on drop.
        let dc = unsafe { GetDC(None) };
        if dc.is_invalid() {
            return Err(CaptureError::Internal(
                "GetDC returned no screen DC".to_owned(),
            ));
        }
        Ok(Self(dc))
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: the DC came from `GetDC(None)`.
        unsafe { ReleaseDC(None, self.0) };
    }
}

/// A memory DC, deleted on drop.
struct MemoryDc(HDC);

impl MemoryDc {
    fn compatible_with(dc: HDC) -> Result<Self, String> {
        // SAFETY: `dc` is alive.
        let memory = unsafe { CreateCompatibleDC(Some(dc)) };
        if memory.is_invalid() {
            return Err("CreateCompatibleDC failed".to_owned());
        }
        Ok(Self(memory))
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: the DC came from `CreateCompatibleDC` and has its own bitmap selected again.
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

/// A DIB section, deleted on drop.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: the bitmap is not selected into any DC any more.
        unsafe {
            let _ = DeleteObject(HGDIOBJ::from(self.0));
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
