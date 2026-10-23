//! The editor window's commands. Each one works on the capture of the calling `editor-<id>`
//! window (see `label`); no command takes a capture id or a path from the WebView.
//!
//! Work that waits (the index, file writes, the clipboard, the dialog) runs on a blocking
//! worker, so the main thread is never held.

use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{Runtime, State, Webview};

use super::envelope;
use super::error::EditorError;
use super::export::{resolve_target, write_replace, ExportFormat};
use super::label::capture_id_of;
use super::ports::{ClipboardImage, EditorDesktop};
use super::session::{EditorState, ExportTarget};
use crate::capture_library::{CaptureLibrary, EditSession};
use crate::shared::CaptureId;

/// `EditorSessionDto` in `src/shared/ipc/annotation-editor.ts`.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSessionDto {
    pub capture_id: CaptureId,
    pub width: u32,
    pub height: u32,
    /// RFC 3339 with the offset.
    pub captured_at: String,
    pub window_title: Option<String>,
    pub annotations: Option<serde_json::Value>,
    pub warning: Option<String>,
    pub unsaved: bool,
}

impl From<EditSession> for EditorSessionDto {
    fn from(session: EditSession) -> Self {
        let item = session.item;
        Self {
            capture_id: item.id,
            width: item.width,
            height: item.height,
            captured_at: item.captured_at.to_rfc3339(),
            window_title: item.window_title,
            annotations: session.annotations,
            warning: session.warning,
            unsaved: session.unsaved,
        }
    }
}

/// `ExportTargetDto` in `src/shared/ipc/annotation-editor.ts`.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTargetDto {
    pub token: String,
    pub format: ExportFormat,
    pub file_name: String,
}

pub const EXPORT_TOKEN_HEADER: &str = "x-snipt-export-token";

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, EditorError> + Send + 'static,
) -> Result<T, EditorError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| EditorError::Io(e.to_string()))?
}

fn caller<R: Runtime>(webview: &Webview<R>) -> Result<CaptureId, EditorError> {
    capture_id_of(webview.label()).inspect_err(|_| {
        log::warn!(
            "editor: command from a non-editor window {}",
            webview.label()
        );
    })
}

fn raw_body(request: &Request<'_>) -> Result<Vec<u8>, EditorError> {
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        InvokeBody::Json(_) => Err(EditorError::InvalidPayload(
            "バイナリで送る必要があります".to_owned(),
        )),
    }
}

fn logged<T>(
    command: &str,
    id: &CaptureId,
    result: Result<T, EditorError>,
) -> Result<T, EditorError> {
    if let Err(e) = &result {
        log::warn!("editor: {command} for {id} failed: {}", e.kind());
    }
    result
}

#[tauri::command]
pub async fn editor_open_session<R: Runtime>(
    webview: Webview<R>,
    library: State<'_, Arc<CaptureLibrary>>,
) -> Result<EditorSessionDto, EditorError> {
    let id = caller(&webview)?;
    let library = library.inner().clone();
    let result = blocking({
        let id = id.clone();
        move || Ok(library.load_for_edit(&id)?.into())
    })
    .await;
    logged("open_session", &id, result)
}

#[tauri::command]
pub async fn editor_read_original<R: Runtime>(
    webview: Webview<R>,
    library: State<'_, Arc<CaptureLibrary>>,
) -> Result<Response, EditorError> {
    let id = caller(&webview)?;
    let library = library.inner().clone();
    let result = blocking({
        let id = id.clone();
        move || Ok(Response::new(library.read_original_png(&id)?))
    })
    .await;
    logged("read_original", &id, result)
}

#[tauri::command]
pub async fn editor_save<R: Runtime>(
    webview: Webview<R>,
    request: Request<'_>,
    library: State<'_, Arc<CaptureLibrary>>,
) -> Result<(), EditorError> {
    let id = caller(&webview)?;
    let body = raw_body(&request)?;
    let library = library.inner().clone();
    let result = blocking({
        let id = id.clone();
        move || {
            let envelope = envelope::decode(&body)?;
            library.save_annotations(&id, envelope.annotations, envelope.png)?;
            Ok(())
        }
    })
    .await;
    logged("save", &id, result)
}

/// Decodes the composite PNG and hands both the pixels and the PNG to the clipboard (no
/// re-encoding).
fn copy_png(desktop: &dyn EditorDesktop, png: &[u8]) -> Result<(), EditorError> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| EditorError::InvalidPayload(format!("合成画像を読み取れません: {e}")))?
        .to_rgba8();
    desktop.copy_image(ClipboardImage {
        width: image.width(),
        height: image.height(),
        rgba: image.as_raw(),
        png: Some(png),
    })
}

#[tauri::command]
pub async fn editor_copy<R: Runtime>(
    webview: Webview<R>,
    request: Request<'_>,
    desktop: State<'_, Arc<dyn EditorDesktop>>,
) -> Result<(), EditorError> {
    let id = caller(&webview)?;
    let png = raw_body(&request)?;
    let desktop = desktop.inner().clone();
    let result = blocking(move || copy_png(desktop.as_ref(), &png)).await;
    logged("copy", &id, result)
}

#[tauri::command]
pub async fn editor_pick_export_path<R: Runtime>(
    webview: Webview<R>,
    suggested_name: String,
    desktop: State<'_, Arc<dyn EditorDesktop>>,
    state: State<'_, Arc<EditorState>>,
) -> Result<Option<ExportTargetDto>, EditorError> {
    let id = caller(&webview)?;
    let label = webview.label().to_owned();
    let desktop = desktop.inner().clone();
    let state = state.inner().clone();
    let result = blocking({
        let id = id.clone();
        move || {
            let Some(chosen) = desktop.pick_save_path(&label, &suggested_name)? else {
                return Ok(None);
            };
            let (path, format) = resolve_target(chosen);
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let token = state.add_export_target(&id, ExportTarget { path, format });
            Ok(Some(ExportTargetDto {
                token,
                format,
                file_name,
            }))
        }
    })
    .await;
    logged("pick_export_path", &id, result)
}

#[tauri::command]
pub async fn editor_write_export<R: Runtime>(
    webview: Webview<R>,
    request: Request<'_>,
    state: State<'_, Arc<EditorState>>,
) -> Result<(), EditorError> {
    let id = caller(&webview)?;
    let token = request
        .headers()
        .get(EXPORT_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or(EditorError::UnknownToken)?;
    let target = logged("write_export", &id, state.export_target(&id, token))?;
    let bytes = raw_body(&request)?;
    let result = blocking(move || {
        if !target.format.matches(&bytes) {
            return Err(EditorError::InvalidPayload(
                "画像の形式が保存先の拡張子と一致しません".to_owned(),
            ));
        }
        write_replace(&target.path, &bytes)
    })
    .await;
    logged("write_export", &id, result)
}

/// The answer to `editor://flush-request`. A late answer (after the exit hook stopped waiting)
/// or an answer for another editor's request is ignored.
#[tauri::command]
pub fn editor_flush_result<R: Runtime>(
    webview: Webview<R>,
    request_id: String,
    saved: bool,
    state: State<'_, Arc<EditorState>>,
) -> Result<(), EditorError> {
    let id = caller(&webview)?;
    if !state.answer_flush(&id, &request_id, saved) {
        log::warn!("editor: {id} answered an unknown flush request");
    }
    Ok(())
}

/// Command names, in the order listed in `build.rs` (`AppManifest::commands`).
pub const COMMANDS: [&str; 7] = [
    "editor_open_session",
    "editor_read_original",
    "editor_save",
    "editor_copy",
    "editor_pick_export_path",
    "editor_write_export",
    "editor_flush_result",
];

#[cfg(test)]
mod tests;
