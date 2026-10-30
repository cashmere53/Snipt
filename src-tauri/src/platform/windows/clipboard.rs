//! Image clipboard: `CF_DIBV5` with alpha plus the registered `"PNG"` format, set in one
//! clipboard session. Used by capture-library, screen-capture and annotation-editor.

use std::thread;
use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_DIBV5;

pub use crate::platform::image_formats::ClipboardImage;
use crate::platform::image_formats::{dibv5_from_rgba, png_from_rgba};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// Another process kept the clipboard open through every retry.
    #[error("clipboard busy")]
    Busy,
    #[error("{0}")]
    Win32(String),
}

const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(20);

/// Callable from any thread; no window handle is needed.
pub fn copy_image(image: ClipboardImage<'_>) -> Result<(), ClipboardError> {
    let dib = dibv5_from_rgba(image.width, image.height, image.rgba);
    let made_png;
    let png = match image.png {
        Some(png) => png,
        None => {
            made_png = png_from_rgba(image.width, image.height, image.rgba)
                .map_err(|e| ClipboardError::Win32(format!("PNG encoding failed: {e}")))?;
            &made_png
        }
    };

    set_image_formats(&dib, png)
}

/// Replaces the clipboard contents with the built `CF_DIBV5` and PNG bytes, in one session.
pub fn set_image_formats(dib: &[u8], png: &[u8]) -> Result<(), ClipboardError> {
    let _session = ClipboardSession::open()?;
    // SAFETY: the clipboard is open on this thread for the lifetime of `_session`.
    unsafe {
        EmptyClipboard().map_err(win32)?;
        set_bytes(u32::from(CF_DIBV5.0), dib)?;
        let png_format = RegisterClipboardFormatW(w!("PNG"));
        if png_format == 0 {
            return Err(win32(windows::core::Error::from_thread()));
        }
        set_bytes(png_format, png)?;
    }
    Ok(())
}

fn win32(e: windows::core::Error) -> ClipboardError {
    ClipboardError::Win32(e.message())
}

/// Closes the clipboard when dropped.
struct ClipboardSession;

impl ClipboardSession {
    fn open() -> Result<Self, ClipboardError> {
        for attempt in 1..=OPEN_ATTEMPTS {
            // SAFETY: no owner window; paired with CloseClipboard in `drop`.
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            if attempt < OPEN_ATTEMPTS {
                thread::sleep(OPEN_RETRY_INTERVAL);
            }
        }
        Err(ClipboardError::Busy)
    }
}

impl Drop for ClipboardSession {
    fn drop(&mut self) {
        // SAFETY: opened by `ClipboardSession::open`.
        if let Err(e) = unsafe { CloseClipboard() } {
            log::warn!("clipboard: could not close: {e}");
        }
    }
}

/// Copies `bytes` into movable global memory and hands it to the clipboard, which then owns it.
///
/// # Safety
/// The clipboard must be open on this thread.
unsafe fn set_bytes(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
    // SAFETY: the allocation is `bytes.len()` long, locked while written, and freed here unless
    // the clipboard took ownership.
    unsafe {
        let memory: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(win32)?;
        let target = GlobalLock(memory);
        if target.is_null() {
            let error = windows::core::Error::from_thread();
            let _ = GlobalFree(Some(memory));
            return Err(win32(error));
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        // GlobalUnlock reports an "error" with code 0 once the lock count reaches zero.
        let _ = GlobalUnlock(memory);
        if let Err(e) = SetClipboardData(format, Some(HANDLE(memory.0))) {
            let _ = GlobalFree(Some(memory));
            return Err(win32(e));
        }
    }
    Ok(())
}
