//! Monitor layout: EnumDisplayMonitors, GetMonitorInfoW and GetDpiForMonitor, and the
//! check that the process is per-monitor DPI aware (screen-capture 5.1). Every rectangle is in
//! physical pixels, which needs Per-Monitor v2 awareness (Tauri's manifest declares it).

use std::mem::size_of;

use windows::core::{w, BOOL};
use windows::Win32::Foundation::{HANDLE, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetDpiAwarenessContextForProcess, GetDpiForMonitor,
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, MONITORINFOF_PRIMARY};

use crate::screen_capture::error::CaptureError;
use crate::screen_capture::geometry::{PhysicalPoint, PhysicalRect};
use crate::screen_capture::topology::{MonitorIndex, MonitorInfo, MonitorTopology};

/// The DPI of a monitor at 100%.
const BASE_DPI: f64 = 96.0;

/// A Win32 `RECT` (right and bottom outside, as here) in physical pixels.
pub fn physical_rect(rect: &RECT) -> PhysicalRect {
    PhysicalRect::from_corners(
        PhysicalPoint::new(rect.left, rect.top),
        PhysicalPoint::new(rect.right, rect.bottom),
    )
}

/// Every monitor, ordered and numbered by `MonitorTopology::new`.
pub fn topology() -> Result<MonitorTopology, CaptureError> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: `collect` only runs during this call, while `handles` is alive and not
    // otherwise borrowed.
    let listed = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut handles as *mut Vec<HMONITOR> as isize),
        )
    };
    if !listed.as_bool() {
        return Err(internal(
            "EnumDisplayMonitors",
            windows::core::Error::from_thread(),
        ));
    }
    let monitors = handles
        .into_iter()
        .map(describe)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MonitorTopology::new(monitors))
}

unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `Vec` that `topology` passed.
    let handles = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    handles.push(monitor);
    true.into()
}

fn describe(handle: HMONITOR) -> Result<MonitorInfo, CaptureError> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `info` is a MONITORINFOEXW whose `cbSize` says so.
    unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo) }
        .ok()
        .map_err(|e| internal("GetMonitorInfoW", e))?;
    let (mut dpi, mut dpi_y) = (0, 0);
    // SAFETY: both pointers are valid for writes.
    unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) }
        .map_err(|e| internal("GetDpiForMonitor", e))?;
    let name_len = info
        .szDevice
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(info.szDevice.len());
    Ok(MonitorInfo {
        // `MonitorTopology::new` numbers the monitors.
        index: MonitorIndex(0),
        device_name: String::from_utf16_lossy(&info.szDevice[..name_len]),
        rect: physical_rect(&info.monitorInfo.rcMonitor),
        dpi,
        scale: f64::from(dpi) / BASE_DPI,
        is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

/// The cursor in global physical pixels; the origin if it cannot be read (a secure desktop).
pub fn cursor_position() -> PhysicalPoint {
    let mut point = POINT::default();
    // SAFETY: `point` is valid for writes.
    match unsafe { GetCursorPos(&mut point) } {
        Ok(()) => PhysicalPoint::new(point.x, point.y),
        Err(error) => {
            log::warn!("capture: could not read the cursor position: {error}");
            PhysicalPoint::default()
        }
    }
}

/// Makes the calling thread Per-Monitor v2 aware, whatever the process default is.
pub fn use_per_monitor_v2_on_this_thread() {
    // SAFETY: plain call; the previous context is not needed.
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        log::warn!("capture: could not make the capture thread Per-Monitor v2 aware");
    }
}

/// Logs the OS build and warns when the process is not Per-Monitor v2 aware (rectangles
/// and captures would then be scaled).
pub fn log_dpi_awareness_and_os_build() {
    // SAFETY: a null handle means the current process.
    let process = unsafe { GetDpiAwarenessContextForProcess(HANDLE::default()) };
    // SAFETY: both are DPI awareness contexts.
    let per_monitor_v2 = unsafe {
        AreDpiAwarenessContextsEqual(process, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    }
    .as_bool();
    let build = os_build().unwrap_or_else(|| "unknown".to_owned());
    if per_monitor_v2 {
        log::info!("capture: Windows build {build}, Per-Monitor v2 DPI aware");
    } else {
        log::warn!("capture: Windows build {build}, the process is not Per-Monitor v2 DPI aware");
    }
}

/// `CurrentBuildNumber.UBR`, such as `26100.2033`.
fn os_build() -> Option<String> {
    const KEY: windows::core::PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let mut text = [0u16; 32];
    let mut size = size_of::<[u16; 32]>() as u32;
    // SAFETY: `text` and `size` are valid for writes and `size` matches the buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            KEY,
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    let build = String::from_utf16_lossy(&text[..len]);

    let mut revision: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: as above.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            KEY,
            w!("UBR"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut revision as *mut u32).cast()),
            Some(&mut size),
        )
    };
    Some(if status.is_ok() {
        format!("{build}.{revision}")
    } else {
        build
    })
}

fn internal(call: &str, error: windows::core::Error) -> CaptureError {
    CaptureError::Internal(format!("{call}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_win32_rects() {
        let rect = RECT {
            left: -3840,
            top: 0,
            right: 0,
            bottom: 2160,
        };
        assert_eq!(
            physical_rect(&rect),
            PhysicalRect::new(-3840, 0, 3840, 2160)
        );
    }

    /// On real hardware: run with `--ignored` on a layout of a 100% and a 150% monitor.
    #[test]
    #[ignore = "needs real monitors"]
    fn lists_the_real_monitors() {
        use_per_monitor_v2_on_this_thread();
        log_dpi_awareness_and_os_build();
        let topology = topology().unwrap();
        for monitor in &topology.monitors {
            println!("{monitor:?}");
            assert!(!monitor.rect.is_empty());
            assert!(monitor.dpi >= 96);
        }
        assert_eq!(topology.monitors.iter().filter(|m| m.is_primary).count(), 1);
        println!("cursor {:?}, build {:?}", cursor_position(), os_build());
    }
}
