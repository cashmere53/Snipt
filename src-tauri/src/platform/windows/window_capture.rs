//! One window's image: Windows.Graphics.Capture, with PrintWindow as the fallback
//! (screen-capture 5.4, 5.5). WGC reads the window's own surface, so windows on top of it
//! do not show.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{factory, Error, Interface, HRESULT, HSTRING};
use windows::Foundation::Metadata::ApiInformation;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind,
    GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
use windows::Win32::Foundation::{ERROR_TIMEOUT, E_FAIL, E_POINTER, HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PW_RENDERFULLCONTENT};

use super::monitors::{physical_rect, use_per_monitor_v2_on_this_thread};
use super::screen_grab::{Canvas, ScreenDc};
use crate::screen_capture::compose::{
    compose_region, from_premultiplied_bgra, CapturedImage, FrozenFrame,
};
use crate::screen_capture::error::CaptureError;
use crate::screen_capture::topology::MonitorIndex;

const SESSION_TYPE: &str = "Windows.Graphics.Capture.GraphicsCaptureSession";
/// How often the frame pool is checked for the first frame.
const POLL: Duration = Duration::from_millis(5);

/// The D3D11 device WGC renders with, kept between captures.
#[derive(Clone)]
struct Devices {
    d3d: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
}

// SAFETY: the device is free threaded; the immediate context and the WinRT wrapper are only
// used by one capture at a time (`WindowCapture::devices` is behind a mutex and captures run
// on the one capture worker).
unsafe impl Send for Devices {}

pub struct WindowCapture {
    devices: Mutex<Option<Devices>>,
    /// The user allowed captures without the yellow border (Windows 11).
    borderless: AtomicBool,
}

impl Default for WindowCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowCapture {
    pub fn new() -> Self {
        Self {
            devices: Mutex::new(None),
            borderless: AtomicBool::new(false),
        }
    }

    /// Asks once for captures without the yellow border, where Windows supports it, and
    /// logs the answer. Blocks: call it on the capture worker, not the UI thread.
    pub fn request_borderless(&self) {
        if !property_present("IsBorderRequired") {
            log::info!("capture: this Windows always draws the capture border");
            return;
        }
        let status =
            GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
                .and_then(|operation| operation.join());
        match status {
            Ok(status) if status == AppCapabilityAccessStatus::Allowed => {
                self.borderless.store(true, Ordering::Relaxed);
                log::info!("capture: borderless window capture allowed");
            }
            Ok(status) => log::info!(
                "capture: borderless window capture not allowed ({})",
                status.0
            ),
            Err(error) => log::warn!("capture: could not ask for borderless capture: {error}"),
        }
    }

    /// The window's image without what covers it; PrintWindow when WGC fails. Gives up after
    /// `timeout` in all: WGC gets two thirds of it, PrintWindow what is left.
    pub fn capture(&self, hwnd: isize, timeout: Duration) -> Result<CapturedImage, CaptureError> {
        let started = Instant::now();
        let deadline = started + timeout;
        let wgc = match self.capture_wgc(hwnd, started + timeout * 2 / 3) {
            Ok(image) => return Ok(image),
            Err(error) => error,
        };
        log::warn!("capture: Windows.Graphics.Capture failed, trying PrintWindow: {wgc}");
        let left = deadline.saturating_duration_since(Instant::now());
        print_window(hwnd, left).map_err(|print| CaptureError::WindowCaptureFailed {
            reason: format!("{wgc}; PrintWindow: {print}"),
        })
    }

    fn capture_wgc(&self, hwnd: isize, deadline: Instant) -> Result<CapturedImage, Error> {
        let devices = self.devices()?;
        match first_frame(
            &devices,
            hwnd,
            deadline,
            self.borderless.load(Ordering::Relaxed),
        ) {
            Err(error) if is_device_lost(&error) => {
                log::warn!("capture: the D3D11 device was lost, recreating it: {error}");
                *self.lock() = None;
                let devices = self.devices()?;
                first_frame(
                    &devices,
                    hwnd,
                    deadline,
                    self.borderless.load(Ordering::Relaxed),
                )
            }
            result => result,
        }
    }

    /// The kept devices, made on first use.
    fn devices(&self) -> Result<Devices, Error> {
        let mut kept = self.lock();
        if let Some(devices) = kept.as_ref() {
            return Ok(devices.clone());
        }
        let devices = create_devices()?;
        *kept = Some(devices.clone());
        Ok(devices)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Devices>> {
        self.devices.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn is_device_lost(error: &Error) -> bool {
    [DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET].contains(&error.code())
}

/// Whether `GraphicsCaptureSession` has `property` on this Windows.
fn property_present(property: &str) -> bool {
    ApiInformation::IsPropertyPresent(&HSTRING::from(SESSION_TYPE), &HSTRING::from(property))
        .unwrap_or(false)
}

/// `IsCursorCaptureEnabled` exists from Windows 10 2004; before it, the overlay hides the
/// cursor before committing.
fn cursor_capture_switchable() -> bool {
    static PRESENT: OnceLock<bool> = OnceLock::new();
    *PRESENT.get_or_init(|| property_present("IsCursorCaptureEnabled"))
}

/// A hardware device, or WARP where there is none (a remote desktop, a VM).
fn create_devices() -> Result<Devices, Error> {
    let device = create_device(D3D_DRIVER_TYPE_HARDWARE).or_else(|error| {
        log::warn!("capture: no hardware D3D11 device, using WARP: {error}");
        create_device(D3D_DRIVER_TYPE_WARP)
    })?;
    // WGC's free-threaded frame pool uses the device from its own threads while this one
    // copies from the immediate context: without the device's lock they race (and crash).
    let multithread: ID3D11Multithread = device.cast()?;
    // SAFETY: plain call on a live device; the result is the previous setting.
    let _ = unsafe { multithread.SetMultithreadProtected(true) };
    // SAFETY: plain call on a live device.
    let context = unsafe { device.GetImmediateContext() }?;
    let dxgi: IDXGIDevice = device.cast()?;
    // SAFETY: `dxgi` is the DXGI side of a live D3D11 device.
    let winrt: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()?;
    Ok(Devices {
        d3d: device,
        context,
        winrt,
    })
}

fn create_device(driver: D3D_DRIVER_TYPE) -> Result<ID3D11Device, Error> {
    let mut device = None;
    // SAFETY: `device` is valid for writes; the other outputs are not asked for.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    }?;
    device.ok_or_else(|| Error::from(E_POINTER))
}

/// Starts a capture session on the window, waits until `deadline` for one frame and reads it
/// back.
fn first_frame(
    devices: &Devices,
    hwnd: isize,
    deadline: Instant,
    borderless: bool,
) -> Result<CapturedImage, Error> {
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    // SAFETY: plain call; an invalid window only makes it fail.
    let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(HWND(hwnd as *mut c_void)) }?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &devices.winrt,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        item.Size()?,
    )?;
    let session = pool.CreateCaptureSession(&item)?;
    let _closing = Closing(&pool, &session);
    if cursor_capture_switchable() {
        session.SetIsCursorCaptureEnabled(false)?;
    }
    if borderless {
        session.SetIsBorderRequired(false)?;
    }
    session.StartCapture()?;

    let frame = loop {
        match pool.TryGetNextFrame() {
            Ok(frame) => break frame,
            // No frame yet comes back as an empty error.
            Err(error) if error.code() == HRESULT(0) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                ERROR_TIMEOUT.to_hresult(),
                "no frame arrived in time",
            ));
        }
        thread::sleep(POLL);
    };
    // One frame is enough: stop capturing before reading it back.
    session.Close()?;
    let content = frame.ContentSize()?;
    let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
    // SAFETY: the surface of a frame of a D3D11 frame pool is a 2D texture.
    let texture: ID3D11Texture2D = unsafe { access.GetInterface() }?;
    let image = read_back(devices, &texture, content.Width, content.Height);
    frame.Close()?;
    image
}

/// Closes the session and the pool however the capture ends.
struct Closing<'a>(&'a Direct3D11CaptureFramePool, &'a GraphicsCaptureSession);

impl Drop for Closing<'_> {
    fn drop(&mut self) {
        let _ = self.1.Close();
        let _ = self.0.Close();
    }
}

/// The top-left `width × height` of `texture` (the window's content; the texture can be
/// larger) as straight RGBA.
fn read_back(
    devices: &Devices,
    texture: &ID3D11Texture2D,
    width: i32,
    height: i32,
) -> Result<CapturedImage, Error> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: `desc` is valid for writes.
    unsafe { texture.GetDesc(&mut desc) };
    let width = u32::try_from(width).unwrap_or(0).min(desc.Width);
    let height = u32::try_from(height).unwrap_or(0).min(desc.Height);
    if width == 0 || height == 0 {
        return Err(Error::new(E_FAIL, "the window has no content"));
    }
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
        ..desc
    };
    let mut staging = None;
    // SAFETY: `staging_desc` describes a CPU-readable texture; `staging` is valid for writes.
    unsafe {
        devices
            .d3d
            .CreateTexture2D(&staging_desc, None, Some(&mut staging))
    }?;
    let staging = staging.ok_or_else(|| Error::from(E_POINTER))?;
    let region = D3D11_BOX {
        left: 0,
        top: 0,
        front: 0,
        right: width,
        bottom: height,
        back: 1,
    };
    let context = &devices.context;
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    // SAFETY: both textures are alive and have the same format; `region` lies inside both.
    unsafe {
        context.CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, 0, Some(&region));
        context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
    }
    let stride = mapped.RowPitch as usize;
    let len = stride * (height as usize - 1) + width as usize * 4;
    // SAFETY: the mapped texture holds `height` rows of `RowPitch` bytes until `Unmap`.
    let bytes = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), len) };
    let image = from_premultiplied_bgra(bytes, width, height, stride);
    // SAFETY: mapped above.
    unsafe { context.Unmap(&staging, 0) };
    Ok(image)
}

/// PrintWindow on its own thread, given up after `timeout` (a hung window never answers; its
/// thread is then left behind).
fn print_window(hwnd: isize, timeout: Duration) -> Result<CapturedImage, String> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("capture-printwindow".to_owned())
        .spawn(move || {
            // Physical pixels, like the capture worker.
            use_per_monitor_v2_on_this_thread();
            let _ = tx.send(print_window_now(hwnd));
        })
        .map_err(|e| format!("could not start a thread: {e}"))?;
    rx.recv_timeout(timeout)
        .map_err(|_| format!("no answer in {timeout:?}"))?
}

fn print_window_now(hwnd: isize) -> Result<CapturedImage, String> {
    let hwnd = HWND(hwnd as *mut c_void);
    let mut window = RECT::default();
    // SAFETY: `window` is valid for writes.
    unsafe { GetWindowRect(hwnd, &mut window) }.map_err(|e| format!("GetWindowRect: {e}"))?;
    let outer = physical_rect(&window);
    if outer.is_empty() {
        return Err("the window has no size".to_owned());
    }
    // Without the invisible resize borders, like the highlight.
    let mut frame = RECT::default();
    // SAFETY: `frame` is a RECT, as DWMWA_EXTENDED_FRAME_BOUNDS needs.
    let visible = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut frame as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
    }
    .ok()
    .and_then(|()| physical_rect(&frame).intersection(&outer))
    .unwrap_or(outer);

    let screen = ScreenDc::get().map_err(|e| e.to_string())?;
    let canvas = Canvas::new(screen.dc(), outer.width, outer.height)?;
    // SAFETY: the canvas DC is alive; PW_RENDERFULLCONTENT also draws DirectComposition
    // content.
    let printed =
        unsafe { PrintWindow(hwnd, canvas.dc(), PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)) };
    if !printed.as_bool() {
        return Err(format!("PrintWindow: {}", Error::from_thread()));
    }
    // GDI leaves alpha undefined: `compose_region` makes the pixels opaque.
    let drawn = FrozenFrame {
        monitor: MonitorIndex(0),
        rect: outer,
        stride: canvas.stride(),
        bgra: canvas.pixels().into(),
    };
    Ok(compose_region(std::slice::from_ref(&drawn), visible))
}

#[cfg(test)]
mod tests {
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    use super::*;

    fn init() {
        use_per_monitor_v2_on_this_thread();
        // SAFETY: plain call; this test thread stays in the MTA.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    }

    fn save(image: &CapturedImage, name: &str) {
        let path = format!("target/{name}.png");
        image::save_buffer(
            &path,
            &image.rgba,
            image.width,
            image.height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        println!("{}x{} -> {path}", image.width, image.height);
    }

    /// On real hardware: run with `--ignored --nocapture` while half of the terminal is
    /// covered by another window; `target/wgc.png` must show the whole terminal.
    #[test]
    #[ignore = "needs a real desktop"]
    fn captures_a_covered_window_with_wgc() {
        init();
        let capture = WindowCapture::new();
        capture.request_borderless();
        // Where it stops tells which half failed, should the process crash.
        println!("borderless asked");
        // SAFETY: plain call.
        let hwnd = unsafe { GetForegroundWindow() }.0 as isize;
        let image = capture
            .capture_wgc(hwnd, Instant::now() + Duration::from_secs(1))
            .unwrap();
        save(&image, "wgc");
    }

    /// As above, with PrintWindow alone: `target/printwindow.png`.
    #[test]
    #[ignore = "needs a real desktop"]
    fn captures_a_window_with_print_window() {
        init();
        // SAFETY: plain call.
        let hwnd = unsafe { GetForegroundWindow() }.0 as isize;
        let image = print_window(hwnd, Duration::from_secs(1)).unwrap();
        save(&image, "printwindow");
    }

    #[test]
    #[ignore = "needs a real desktop"]
    fn a_window_that_does_not_exist_fails() {
        init();
        let started = Instant::now();
        let error = WindowCapture::new()
            .capture(0x7fff_fff0, Duration::from_millis(500))
            .unwrap_err();
        println!("{error}");
        // Both attempts share the one deadline.
        assert!(started.elapsed() < Duration::from_millis(600));
        assert!(matches!(error, CaptureError::WindowCaptureFailed { .. }));
    }
}
