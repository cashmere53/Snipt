//! The native window per monitor that shows the frozen screen under the overlay
//! (screen-capture 5.6), and the Win32 side of the overlay windows: `WinOverlayNative`.
//!
//! Backdrops are made on the main thread (it pumps their messages) and shown or hidden from
//! the capture worker; their frames live in `FRAMES` only while they are shown. No lock is
//! held across a call that waits for the main thread.

use std::ffi::c_void;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use windows::core::{w, Error, PCWSTR};
use windows::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, RedrawWindow, SetDIBitsToDevice, DIB_RGB_COLORS, PAINTSTRUCT,
    RDW_INVALIDATE, RDW_UPDATENOW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, SetWindowPos, ShowWindow,
    HWND_TOPMOST, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_ERASEBKGND,
    WM_MOUSEACTIVATE, WM_PAINT, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

use super::screen_grab::top_down_bgra;
use crate::screen_capture::compose::FrozenFrame;
use crate::screen_capture::error::CaptureError;
use crate::screen_capture::geometry::PhysicalRect;
use crate::screen_capture::overlay::OverlayNative;
use crate::screen_capture::topology::{MonitorIndex, MonitorTopology};

const CLASS_NAME: PCWSTR = w!("SniptBackdrop");
/// How long building the backdrops may wait for the main thread.
const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs a closure on the main thread (`AppHandle::run_on_main_thread`).
pub type MainThread = Arc<dyn Fn(Box<dyn FnOnce() + Send>) -> Result<(), String> + Send + Sync>;

/// The frame each shown backdrop paints, by window handle.
static FRAMES: Mutex<Vec<(isize, FrozenFrame)>> = Mutex::new(Vec::new());

#[derive(Clone, Copy)]
struct Backdrop {
    hwnd: isize,
    monitor: MonitorIndex,
    rect: PhysicalRect,
}

pub struct WinOverlayNative {
    main: MainThread,
    backdrops: Mutex<Vec<Backdrop>>,
}

impl WinOverlayNative {
    pub fn new(main: MainThread) -> Self {
        Self {
            main,
            backdrops: Mutex::new(Vec::new()),
        }
    }

    fn backdrops(&self) -> Vec<Backdrop> {
        self.backdrops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Runs `job` on the main thread and waits for its answer.
    fn on_main<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        (self.main)(Box::new(move || {
            let _ = tx.send(job());
        }))?;
        rx.recv_timeout(MAIN_THREAD_TIMEOUT)
            .map_err(|_| "the main thread did not answer".to_owned())
    }
}

impl OverlayNative for WinOverlayNative {
    fn prepare_overlay(&self, hwnd: isize) {
        let preference = DWMWCP_DONOTROUND;
        // SAFETY: the attribute takes a DWM_WINDOW_CORNER_PREFERENCE.
        let set = unsafe {
            DwmSetWindowAttribute(
                handle(hwnd),
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&preference as *const DWM_WINDOW_CORNER_PREFERENCE).cast::<c_void>(),
                size_of_val(&preference) as u32,
            )
        };
        // Windows 10 has no rounded corners and refuses the attribute.
        if let Err(error) = set {
            log::debug!("capture: corner preference not set: {error}");
        }
    }

    fn place_topmost(&self, hwnd: isize, rect: PhysicalRect) {
        if let Err(error) = place(hwnd, rect) {
            log::warn!("capture: could not place an overlay: {error}");
        }
    }

    fn hide_overlay(&self, hwnd: isize) {
        // SAFETY: hiding a window is allowed from any thread.
        let _ = unsafe { ShowWindow(handle(hwnd), SW_HIDE) };
    }

    fn rebuild_backdrops(&self, topology: &MonitorTopology) -> Result<(), CaptureError> {
        let old: Vec<isize> = self.backdrops().iter().map(|b| b.hwnd).collect();
        let monitors: Vec<(MonitorIndex, PhysicalRect)> = topology
            .monitors
            .iter()
            .map(|m| (m.index, m.rect))
            .collect();
        let built = self
            .on_main(move || {
                for hwnd in old {
                    // SAFETY: a backdrop made on this thread.
                    let _ = unsafe { DestroyWindow(handle(hwnd)) };
                }
                monitors
                    .into_iter()
                    .map(|(monitor, rect)| {
                        create_backdrop(rect).map(|hwnd| Backdrop {
                            hwnd,
                            monitor,
                            rect,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()
                    .map_err(|e| e.to_string())
            })
            .and_then(|built| built)
            .map_err(|reason| CaptureError::OverlayUnavailable { reason })?;
        *self
            .backdrops
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = built;
        Ok(())
    }

    fn show_backdrops(&self, frames: &[FrozenFrame]) -> Result<(), CaptureError> {
        let backdrops = self.backdrops();
        let shown: Vec<(Backdrop, FrozenFrame)> = backdrops
            .iter()
            .filter_map(|b| {
                let frame = frames.iter().find(|f| f.monitor == b.monitor)?;
                Some((*b, frame.clone()))
            })
            .collect();
        *FRAMES.lock().unwrap_or_else(PoisonError::into_inner) = shown
            .iter()
            .map(|(b, frame)| (b.hwnd, frame.clone()))
            .collect();
        for (backdrop, _) in &shown {
            place(backdrop.hwnd, backdrop.rect).map_err(|e| CaptureError::OverlayUnavailable {
                reason: format!("backdrop: {e}"),
            })?;
            // SAFETY: paints now, so that the overlay never shows over an empty backdrop.
            let _ = unsafe {
                RedrawWindow(
                    Some(handle(backdrop.hwnd)),
                    None,
                    None,
                    RDW_INVALIDATE | RDW_UPDATENOW,
                )
            };
        }
        Ok(())
    }

    fn hide_backdrops(&self) {
        for backdrop in self.backdrops() {
            // SAFETY: hiding a window is allowed from any thread.
            let _ = unsafe { ShowWindow(handle(backdrop.hwnd), SW_HIDE) };
        }
        FRAMES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    fn backdrop_hwnds(&self) -> Vec<isize> {
        self.backdrops().iter().map(|b| b.hwnd).collect()
    }
}

fn handle(hwnd: isize) -> HWND {
    HWND(hwnd as *mut c_void)
}

/// Topmost over `rect`, shown, not activated.
fn place(hwnd: isize, rect: PhysicalRect) -> windows::core::Result<()> {
    // SAFETY: plain call on a window handle.
    unsafe {
        SetWindowPos(
            handle(hwnd),
            Some(HWND_TOPMOST),
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            SWP_SHOWWINDOW | SWP_NOACTIVATE,
        )
    }
}

/// A hidden backdrop over `rect`. On the main thread.
fn create_backdrop(rect: PhysicalRect) -> Result<isize, Error> {
    // SAFETY: plain call; `None` is this executable.
    let instance = unsafe { GetModuleHandleW(None) }?;
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(backdrop_proc),
        hInstance: instance.into(),
        lpszClassName: CLASS_NAME,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is filled in.
    if unsafe { RegisterClassExW(&class) } == 0 {
        let error = Error::from_thread();
        if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
            return Err(error);
        }
    }
    // SAFETY: the class is registered; the window stays hidden until shown.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS_NAME,
            w!("Snipt backdrop"),
            WS_POPUP,
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }?;
    Ok(hwnd.0 as isize)
}

unsafe extern "system" fn backdrop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        // The frame covers everything: no background to erase (no flash).
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        // SAFETY: the default handling of every other message.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Draws the backdrop's frame 1:1 from its top-left corner.
fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    // SAFETY: inside WM_PAINT of this window; ended below.
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    let frame = FRAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(h, _)| *h == hwnd.0 as isize)
        .map(|(_, frame)| frame.clone());
    if let Some(frame) = frame {
        let (width, height) = (frame.rect.width, frame.rect.height);
        // Rows of `stride` bytes are a bitmap `stride / 4` pixels wide; only `width` show.
        let info = top_down_bgra((frame.stride / 4) as u32, height);
        // SAFETY: `frame.bgra` holds `height` rows of `stride` bytes, as `info` says.
        unsafe {
            SetDIBitsToDevice(
                dc,
                0,
                0,
                width,
                height,
                0,
                0,
                0,
                height,
                frame.bgra.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
            );
        }
    }
    // SAFETY: pairs with BeginPaint above.
    let _ = unsafe { EndPaint(hwnd, &paint) };
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    use super::*;
    use crate::platform::windows::monitors::{topology, use_per_monitor_v2_on_this_thread};
    use crate::platform::windows::screen_grab::grab_monitors;

    fn pump_for(duration: Duration) {
        let until = Instant::now() + duration;
        let mut msg = MSG::default();
        while Instant::now() < until {
            // SAFETY: the message loop of this thread.
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// On real hardware: run with `--ignored --nocapture`. The screen freezes for two seconds
    /// (nothing moves, no shift), then comes back; the time to show is printed.
    #[test]
    #[ignore = "needs real monitors and a person"]
    fn shows_the_frozen_screen_without_a_shift() {
        use_per_monitor_v2_on_this_thread();
        // This test thread is the "main thread": jobs run at once.
        let native = WinOverlayNative::new(Arc::new(|job| {
            job();
            Ok(())
        }));
        let topology = topology().unwrap();
        native.rebuild_backdrops(&topology).unwrap();
        let frames = grab_monitors(&topology).unwrap();
        let started = Instant::now();
        native.show_backdrops(&frames).unwrap();
        println!("shown in {:?}", started.elapsed());
        pump_for(Duration::from_secs(2));
        native.hide_backdrops();
        pump_for(Duration::from_millis(100));
    }
}
