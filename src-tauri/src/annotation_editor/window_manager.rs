//! `EditorWindowManager`: implements `shared::EditorLauncher` with one `editor-<id>` window
//! per capture, the flush-then-close used before deleting a capture, and the flush on quit.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, PhysicalPosition,
    PhysicalSize, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use super::label::{capture_id_of, editor_label};
use crate::capture_library::CaptureLibrary;
use crate::shared::{CaptureId, EditorLauncher, ExitCheck, Notice, NoticeLevel, Notifier};

/// `CLOSE_REQUEST_EVENT` in `src/shared/ipc/annotation-editor.ts`.
pub const CLOSE_REQUEST_EVENT: &str = "editor://close-request";
/// `FLUSH_REQUEST_EVENT` in `src/shared/ipc/annotation-editor.ts`.
pub const FLUSH_REQUEST_EVENT: &str = "editor://flush-request";

/// Space around the image for the toolbar, the style panel and the status bar (logical px).
const UI_MARGIN: (f64, f64) = (48.0, 120.0);
const MIN_SIZE: (f64, f64) = (800.0, 600.0);
const MAX_SHARE_OF_WORK_AREA: f64 = 0.9;

/// The usable part of a monitor (without the taskbar), in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkArea {
    pub position: PhysicalPosition<i32>,
    pub size: PhysicalSize<u32>,
    pub scale: f64,
}

impl Default for WorkArea {
    fn default() -> Self {
        Self {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1040),
            scale: 1.0,
        }
    }
}

/// The image at 1:1 physical pixels plus the UI, at most 90% of the work area and at least
/// 800×600 (unless the work area is smaller).
pub fn initial_size(image: PhysicalSize<u32>, area: &WorkArea) -> LogicalSize<f64> {
    let max_w = f64::from(area.size.width) / area.scale * MAX_SHARE_OF_WORK_AREA;
    let max_h = f64::from(area.size.height) / area.scale * MAX_SHARE_OF_WORK_AREA;
    let want_w = f64::from(image.width) / area.scale + UI_MARGIN.0;
    let want_h = f64::from(image.height) / area.scale + UI_MARGIN.1;
    LogicalSize::new(
        want_w.max(MIN_SIZE.0).min(max_w).round(),
        want_h.max(MIN_SIZE.1).min(max_h).round(),
    )
}

/// Centers `size` in the work area.
pub fn centered(size: LogicalSize<f64>, area: &WorkArea) -> LogicalPosition<f64> {
    let left = f64::from(area.position.x) / area.scale;
    let top = f64::from(area.position.y) / area.scale;
    let width = f64::from(area.size.width) / area.scale;
    let height = f64::from(area.size.height) / area.scale;
    LogicalPosition::new(
        (left + (width - size.width) / 2.0).round(),
        (top + (height - size.height) / 2.0).round(),
    )
}

/// `Snipt - 2026/10/22 17:41:50`, with ` (2)` for later captures of the same second.
pub fn window_title(id: &CaptureId) -> String {
    let s = id.as_str();
    let mut title = format!(
        "Snipt - {}/{}/{} {}:{}:{}",
        &s[0..4],
        &s[4..6],
        &s[6..8],
        &s[9..11],
        &s[11..13],
        &s[13..15]
    );
    if let Some(seq) = s.get(16..) {
        title.push_str(&format!(" ({})", seq.trim_start_matches('0')));
    }
    title
}

/// Waits for `closed` until `timeout`; calls `force` if it did not come. Returns whether the
/// window closed by itself.
pub fn wait_or_force(closed: &Receiver<()>, timeout: Duration, force: impl FnOnce()) -> bool {
    // A disconnect means the session was dropped, which also means closed.
    match closed.recv_timeout(timeout) {
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => true,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            force();
            false
        }
    }
}

/// Waits (all within one `timeout`) for each editor's flush answer; the editors that failed or
/// did not answer are returned as `注釈: <id>`.
pub fn unsaved_after(pending: Vec<(CaptureId, Receiver<bool>)>, timeout: Duration) -> Vec<String> {
    let deadline = Instant::now() + timeout;
    pending
        .into_iter()
        .filter_map(|(id, answer)| {
            let left = deadline.saturating_duration_since(Instant::now());
            match answer.recv_timeout(left) {
                Ok(true) => None,
                Ok(false) | Err(_) => Some(format!("注釈: {id}")),
            }
        })
        .collect()
}

pub type WorkAreaAtCursor = Box<dyn Fn() -> Option<WorkArea> + Send + Sync>;

/// The work area of the monitor under the mouse cursor.
pub fn work_area_at_cursor<R: Runtime>(app: AppHandle<R>) -> WorkAreaAtCursor {
    Box::new(move || {
        let cursor = app.cursor_position().ok()?;
        let monitor = app
            .monitor_from_point(cursor.x, cursor.y)
            .ok()
            .flatten()
            .or_else(|| app.primary_monitor().ok().flatten())?;
        let area = monitor.work_area();
        Some(WorkArea {
            position: area.position,
            size: area.size,
            scale: monitor.scale_factor(),
        })
    })
}

pub struct EditorWindowManager<R: Runtime> {
    app: AppHandle<R>,
    state: Arc<super::session::EditorState>,
    notifier: Arc<dyn Notifier>,
    work_area: WorkAreaAtCursor,
    /// Keeps a check-then-create from racing with another open of the same capture.
    opening: Mutex<()>,
    pub close_timeout: Duration,
    pub flush_timeout: Duration,
}

impl<R: Runtime> EditorWindowManager<R> {
    pub fn new(
        app: AppHandle<R>,
        state: Arc<super::session::EditorState>,
        notifier: Arc<dyn Notifier>,
        work_area: WorkAreaAtCursor,
    ) -> Self {
        Self {
            app,
            state,
            notifier,
            work_area,
            opening: Mutex::new(()),
            close_timeout: Duration::from_secs(3),
            flush_timeout: Duration::from_secs(3),
        }
    }

    fn report(&self, id: &CaptureId, error: &str) {
        log::error!("editor: could not open {id}: {error}");
        self.notifier.notify(Notice {
            level: NoticeLevel::Error,
            title: "編集画面を開けませんでした".to_owned(),
            body: error.to_owned(),
        });
    }

    /// Brings an existing editor to the front, or creates it. Runs off the main thread.
    pub fn show(&self, id: &CaptureId) {
        let label = match editor_label(id) {
            Ok(label) => label,
            Err(e) => return self.report(id, &e.to_string()),
        };
        let _opening = self.opening.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(window) = self.app.get_webview_window(&label) {
            bring_to_front(&window);
            return;
        }
        if let Err(e) = self.create(id, &label) {
            self.report(id, &e);
        }
    }

    fn create(&self, id: &CaptureId, label: &str) -> Result<(), String> {
        let image = self
            .app
            .try_state::<Arc<CaptureLibrary>>()
            .ok_or("キャプチャの保管庫が使えません")?
            .load_for_edit(id)
            .map_err(|e| e.to_string())?
            .item;
        let area = (self.work_area)().unwrap_or_default();
        let size = initial_size(PhysicalSize::new(image.width, image.height), &area);
        let position = centered(size, &area);
        let window =
            WebviewWindowBuilder::new(&self.app, label, WebviewUrl::App("index.html".into()))
                .title(window_title(id))
                .inner_size(size.width, size.height)
                .min_inner_size(MIN_SIZE.0.min(size.width), MIN_SIZE.1.min(size.height))
                .position(position.x, position.y)
                .focused(true)
                .build()
                .map_err(|e| e.to_string())?;
        let state = self.state.clone();
        let id = id.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::Destroyed = event {
                state.remove(&id);
            }
        });
        Ok(())
    }

    /// Asks the editor to flush and close, then waits for it; forces it closed after the
    /// timeout. Returns at once when it is not open. Blocks: never call on the main thread.
    pub fn close(&self, id: &CaptureId) {
        let Ok(label) = editor_label(id) else { return };
        if self.app.get_webview_window(&label).is_none() {
            return;
        }
        let closed = self.state.wait_for_close(id);
        // Destroyed between the check and the registration: nothing will wake us.
        let Some(window) = self.app.get_webview_window(&label) else {
            self.state.remove(id);
            return;
        };
        if let Err(e) =
            self.app
                .emit_to(EventTarget::webview_window(&label), CLOSE_REQUEST_EVENT, ())
        {
            log::warn!("editor: could not ask {id} to close: {e}");
        }
        let closed_by_itself = wait_or_force(&closed, self.close_timeout, || {
            log::warn!("editor: {id} did not close in time; destroying it");
            if let Err(e) = window.destroy() {
                log::warn!("editor: could not destroy {id}: {e}");
            }
        });
        if !closed_by_itself {
            // The mock runtime (and a crashed WebView) may not report the destruction.
            self.state.remove(id);
        }
    }

    /// The before-exit hook: asks every open editor to save without closing and reports those
    /// that failed or did not answer in time. Idempotent.
    pub fn flush_all(&self) -> ExitCheck {
        let pending: Vec<_> = self
            .app
            .webview_windows()
            .into_keys()
            .filter_map(|label| capture_id_of(&label).ok().map(|id| (label, id)))
            .map(|(label, id)| {
                let (request_id, answer) = self.state.expect_flush(&id);
                let payload = json!({ "requestId": request_id });
                if let Err(e) = self.app.emit_to(
                    EventTarget::webview_window(&label),
                    FLUSH_REQUEST_EVENT,
                    payload,
                ) {
                    log::warn!("editor: could not ask {id} to save: {e}");
                }
                (id, request_id, answer)
            })
            .collect();
        let request_ids: Vec<String> = pending.iter().map(|(_, r, _)| r.clone()).collect();
        let unsaved = unsaved_after(
            pending
                .into_iter()
                .map(|(id, _, answer)| (id, answer))
                .collect(),
            self.flush_timeout,
        );
        self.state.forget_flushes(&request_ids);
        ExitCheck { unsaved }
    }
}

fn bring_to_front<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.unminimize();
    let _ = window.show();
    if window.set_focus().is_err() {
        // Windows may refuse to move focus; flipping always-on-top brings it forward anyway.
        let _ = window.set_always_on_top(true);
        let _ = window.set_always_on_top(false);
        let _ = window.set_focus();
    }
}

/// The shared trait for screen-capture and capture-library, backed by the manager.
pub struct Launcher<R: Runtime>(pub Arc<EditorWindowManager<R>>);

impl<R: Runtime> EditorLauncher for Launcher<R> {
    fn open_editor(&self, id: &CaptureId) {
        let manager = self.0.clone();
        let id = id.clone();
        tauri::async_runtime::spawn_blocking(move || manager.show(&id));
    }

    fn close_editor(&self, id: &CaptureId) {
        self.0.close(id);
    }
}

#[cfg(test)]
mod tests;
