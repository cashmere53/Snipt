//! A hidden window that reports monitor layout changes (screen-capture 5.7). Broadcasts do
//! not reach message-only windows, so it is a real top-level window that is never shown.

use std::cell::Cell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetWindowLongPtrW, RegisterClassExW, SetWindowLongPtrW,
    GWLP_USERDATA, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE, WNDCLASSEXW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

use super::monitors::topology;
use crate::screen_capture::topology::TopologyFingerprint;

const CLASS_NAME: PCWSTR = w!("SniptDisplayWatch");

struct Watch {
    on_change: Box<dyn Fn()>,
    /// The layout last seen, to tell real changes among setting changes.
    last: Cell<Option<TopologyFingerprint>>,
}

impl Watch {
    /// Whether the layout differs from the one last seen; remembers the new one.
    fn layout_changed(&self) -> bool {
        let now = topology().ok().map(|t| t.fingerprint());
        now != self.last.replace(now)
    }
}

/// Calls `on_change` on every display or DPI change, and on setting changes that moved a
/// monitor. Must run on a thread with a message loop (Tauri's main thread); the window lives
/// until the process exits.
pub fn watch(on_change: impl Fn() + 'static) -> Result<(), String> {
    // SAFETY: plain call; `None` is this executable.
    let instance =
        unsafe { GetModuleHandleW(None) }.map_err(|e| format!("GetModuleHandleW: {e}"))?;
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: CLASS_NAME,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is filled in; registering twice fails harmlessly.
    if unsafe { RegisterClassExW(&class) } == 0 {
        return Err(format!(
            "RegisterClassExW: {}",
            windows::core::Error::from_thread()
        ));
    }
    // SAFETY: the class is registered; the window is never shown.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS_NAME,
            w!("Snipt display watch"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
    .map_err(|e| format!("CreateWindowExW: {e}"))?;
    let watch = Box::new(Watch {
        on_change: Box::new(on_change),
        last: Cell::new(topology().ok().map(|t| t.fingerprint())),
    });
    // SAFETY: the window keeps the pointer for the life of the process; it is never freed.
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(watch) as isize) };
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: 0 until `watch` stores its `Watch`, which is never freed.
    let watch = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Watch).as_ref() };
    if let Some(watch) = watch {
        let changed = match msg {
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                watch.layout_changed();
                true
            }
            // Sent for any setting; only a moved or rescaled monitor counts.
            WM_SETTINGCHANGE => watch.layout_changed(),
            _ => false,
        };
        if changed {
            log::info!("capture: display change (message {msg:#x})");
            (watch.on_change)();
        }
    }
    // SAFETY: the default handling of every message.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    use super::*;
    use crate::platform::windows::monitors::use_per_monitor_v2_on_this_thread;

    /// On real hardware: run with `--ignored --nocapture`, then within 30 seconds change the
    /// display scale and unplug a monitor; each must print a change.
    #[test]
    #[ignore = "needs real monitors and a person"]
    fn reports_real_layout_changes() {
        use_per_monitor_v2_on_this_thread();
        let changes = Rc::new(Cell::new(0));
        let seen = changes.clone();
        watch(move || {
            seen.set(seen.get() + 1);
            println!("layout changed: {:?}", topology().map(|t| t.monitors));
        })
        .unwrap();
        let until = Instant::now() + Duration::from_secs(30);
        let mut msg = MSG::default();
        while Instant::now() < until {
            // SAFETY: the message loop of this thread.
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(changes.get() > 0, "no layout change was reported");
    }
}
