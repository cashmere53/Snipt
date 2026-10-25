//! The capturable windows in z order, with their DWM frame bounds and titles
//! (screen-capture 5.3). The OS state is read here; `window_pick` decides.

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindowVisible,
};

use super::monitors::physical_rect;
use crate::screen_capture::window_pick::{candidates, TopLevelWindow, WindowCandidate};

/// The capturable top-level windows, topmost first, without `exclude`.
pub fn list_windows(exclude: &[isize]) -> Vec<WindowCandidate> {
    candidates(top_level_windows(), exclude)
}

/// Every top-level window in z order (topmost first), as the OS reports it.
fn top_level_windows() -> Vec<TopLevelWindow> {
    let mut handles: Vec<HWND> = Vec::new();
    // SAFETY: `collect` only runs during this call, while `handles` is alive and not
    // otherwise borrowed.
    let listed = unsafe {
        EnumWindows(
            Some(collect),
            LPARAM(&mut handles as *mut Vec<HWND> as isize),
        )
    };
    if let Err(error) = listed {
        // The windows found so far are still worth offering.
        log::warn!("capture: EnumWindows stopped early: {error}");
    }
    handles.into_iter().map(read).collect()
}

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `Vec` that `top_level_windows` passed.
    let handles = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
    handles.push(hwnd);
    true.into()
}

fn read(hwnd: HWND) -> TopLevelWindow {
    // SAFETY: plain queries on a window handle; a window closed meanwhile only makes them
    // fail.
    let (visible, minimized) =
        unsafe { (IsWindowVisible(hwnd).as_bool(), IsIconic(hwnd).as_bool()) };
    let mut cloaked: u32 = 0;
    // SAFETY: `cloaked` is a DWORD, as DWMWA_CLOAKED needs.
    let cloaked =
        unsafe { dwm_attribute(hwnd, DWMWA_CLOAKED, &mut cloaked) }.is_ok() && cloaked != 0;
    let mut frame = RECT::default();
    // SAFETY: `frame` is a RECT, as DWMWA_EXTENDED_FRAME_BOUNDS needs.
    let rect = match unsafe { dwm_attribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut frame) } {
        Ok(()) => physical_rect(&frame),
        // An empty rectangle: never a candidate.
        Err(_) => Default::default(),
    };
    TopLevelWindow {
        hwnd: hwnd.0 as isize,
        rect,
        title: window_text(hwnd),
        class_name: class_name(hwnd),
        visible,
        minimized,
        cloaked,
    }
}

/// # Safety
/// `T` must be the type `attribute` writes.
unsafe fn dwm_attribute<T>(
    hwnd: HWND,
    attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
    value: &mut T,
) -> windows::core::Result<()> {
    // SAFETY: `value` is valid for `size_of::<T>()` bytes, per the caller.
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (value as *mut T).cast::<c_void>(),
            size_of::<T>() as u32,
        )
    }
}

fn window_text(hwnd: HWND) -> String {
    // SAFETY: plain query.
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut text = vec![0u16; len as usize + 1];
    // SAFETY: `text` is a writable buffer of the length passed.
    let copied = unsafe { GetWindowTextW(hwnd, &mut text) };
    String::from_utf16_lossy(&text[..copied.max(0) as usize])
}

fn class_name(hwnd: HWND) -> String {
    // Class names are at most 256 characters.
    let mut name = [0u16; 257];
    // SAFETY: `name` is a writable buffer of the length passed.
    let copied = unsafe { GetClassNameW(hwnd, &mut name) };
    String::from_utf16_lossy(&name[..copied.max(0) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::windows::monitors::use_per_monitor_v2_on_this_thread;

    /// On real hardware: run with `--ignored` with a minimized window and a window on another
    /// virtual desktop, and check that neither is listed.
    #[test]
    #[ignore = "needs a real desktop"]
    fn lists_the_real_windows() {
        use_per_monitor_v2_on_this_thread();
        for window in list_windows(&[]) {
            println!("{:>3} {:?} {:?}", window.z, window.rect, window.title);
            assert!(!window.rect.is_empty());
        }
    }
}
