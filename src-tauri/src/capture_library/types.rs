//! Data types of the capture library, shared with screen-capture, annotation-editor and the
//! history window (`src/shared/ipc/capture-library.ts`).

use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Local};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use crate::shared::{CaptureId, CaptureMode};

/// One capture in the history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureItem {
    pub id: CaptureId,
    /// Serialized as RFC 3339 with the offset.
    pub captured_at: DateTime<FixedOffset>,
    pub mode: CaptureMode,
    /// Physical pixels.
    pub width: u32,
    pub height: u32,
    /// Only for window captures.
    pub window_title: Option<String>,
    pub original_path: PathBuf,
    /// Set while the annotated PNG exists.
    pub annotated_path: Option<PathBuf>,
    /// Set while the annotation document exists.
    pub has_annotations: bool,
    /// Set once the thumbnail has been generated.
    pub thumbnail_path: Option<PathBuf>,
    /// Changes whenever the thumbnail is regenerated (milliseconds), to bust caches.
    pub thumbnail_version: u64,
}

impl CaptureItem {
    /// The image to copy, reveal and thumbnail: the annotated PNG if any, else the original.
    pub fn display_path(&self) -> &Path {
        self.annotated_path
            .as_deref()
            .unwrap_or(&self.original_path)
    }
}

/// The pixels of a new capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureImage {
    /// Straight (not premultiplied) RGBA8, rows packed (stride = width * 4).
    Rgba {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
    /// An already encoded PNG; capture-library writes its own metadata into it.
    Png(Vec<u8>),
}

/// What screen-capture hands to `CaptureLibrary::save_capture`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCapture {
    pub image: CaptureImage,
    pub mode: CaptureMode,
    pub captured_at: DateTime<Local>,
    /// Only for window captures.
    pub window_title: Option<String>,
}

/// What the editor needs to open a capture.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSession {
    pub item: CaptureItem,
    /// May not exist for an unsaved capture; pixels come from `read_original_png`.
    pub original_path: PathBuf,
    /// Opaque to capture-library apart from its integer top-level `schemaVersion`.
    pub annotations: Option<serde_json::Value>,
    /// A message for the user (unreadable annotations, unsaved capture).
    pub warning: Option<String>,
    /// The capture is not in the save folder yet.
    pub unsaved: bool,
}

/// Why a library operation failed. Serialized as `{ kind, ...fields, message }` where `message`
/// is the user-facing text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LibraryError {
    #[error("キャプチャが見つかりません({id})")]
    NotFound { id: CaptureId },
    #[error("保存先フォルダを使用できません: {message}")]
    SaveDirUnavailable { path: PathBuf, message: String },
    #[error("保存先の空き容量が足りません: {message}")]
    DiskFull { message: String },
    #[error("保存先に書き込む権限がありません: {message}")]
    PermissionDenied { path: PathBuf, message: String },
    #[error("画像が正しくありません: {message}")]
    InvalidImage { message: String },
    #[error("注釈データが正しくありません: {message}")]
    InvalidAnnotationDocument { message: String },
    #[error("クリップボードにコピーできませんでした: {message}")]
    Clipboard { message: String },
    #[error("エクスプローラーまたはごみ箱の操作に失敗しました: {message}")]
    Shell { message: String },
    #[error("ファイルの読み書きに失敗しました: {message}")]
    Io { message: String },
}

impl LibraryError {
    /// The `kind` tag, matching `LibraryErrorKind` in TypeScript.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "notFound",
            Self::SaveDirUnavailable { .. } => "saveDirUnavailable",
            Self::DiskFull { .. } => "diskFull",
            Self::PermissionDenied { .. } => "permissionDenied",
            Self::InvalidImage { .. } => "invalidImage",
            Self::InvalidAnnotationDocument { .. } => "invalidAnnotationDocument",
            Self::Clipboard { .. } => "clipboard",
            Self::Shell { .. } => "shell",
            Self::Io { .. } => "io",
        }
    }
}

impl Serialize for LibraryError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", self.kind())?;
        match self {
            Self::NotFound { id } => map.serialize_entry("id", id)?,
            Self::SaveDirUnavailable { path, .. } | Self::PermissionDenied { path, .. } => {
                map.serialize_entry("path", path)?
            }
            _ => {}
        }
        map.serialize_entry("message", &self.to_string())?;
        map.end()
    }
}

/// `save_capture` failed; the capture is kept in memory so the editor can still open it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{reason}")]
pub struct SaveCaptureError {
    pub reason: LibraryError,
    /// Kept in the unsaved store; open the editor with this id.
    pub unsaved: CaptureItem,
}

/// One capture that `delete` could not move to the recycle bin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeleteFailure {
    pub id: CaptureId,
    pub error: LibraryError,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DeleteOutcome {
    pub removed: Vec<CaptureId>,
    pub failed: Vec<DeleteFailure>,
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn id(s: &str) -> CaptureId {
        CaptureId::parse(s).unwrap()
    }

    pub fn item(id_str: &str) -> CaptureItem {
        CaptureItem {
            id: id(id_str),
            captured_at: DateTime::parse_from_rfc3339("2026-10-22T17:41:50.123+09:00").unwrap(),
            mode: CaptureMode::Region,
            width: 1920,
            height: 1080,
            window_title: None,
            original_path: PathBuf::from(format!("C:/shots/Snipt_{id_str}.png")),
            annotated_path: None,
            has_annotations: false,
            thumbnail_path: None,
            thumbnail_version: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{id, item};
    use super::*;

    #[test]
    fn item_serializes_in_camel_case_with_rfc3339_and_null_title() {
        let mut item = item("20261022_174150");
        item.mode = CaptureMode::Fullscreen;
        let value = serde_json::to_value(&item).unwrap();
        assert_eq!(
            value,
            json!({
                "id": "20261022_174150",
                "capturedAt": "2026-10-22T17:41:50.123+09:00",
                "mode": "fullscreen",
                "width": 1920,
                "height": 1080,
                "windowTitle": null,
                "originalPath": "C:/shots/Snipt_20261022_174150.png",
                "annotatedPath": null,
                "hasAnnotations": false,
                "thumbnailPath": null,
                "thumbnailVersion": 0,
            })
        );
    }

    #[test]
    fn window_title_and_mode_spelling() {
        let mut item = item("20261022_174150");
        item.mode = CaptureMode::Window;
        item.window_title = Some("メモ帳".to_owned());
        let value = serde_json::to_value(&item).unwrap();
        assert_eq!(value["mode"], "window");
        assert_eq!(value["windowTitle"], "メモ帳");
    }

    #[test]
    fn display_path_prefers_the_annotated_png() {
        let mut item = item("20261022_174150");
        assert_eq!(item.display_path(), item.original_path);
        item.annotated_path = Some(PathBuf::from(
            "C:/shots/Snipt_20261022_174150_annotated.png",
        ));
        assert_eq!(
            item.display_path(),
            Path::new("C:/shots/Snipt_20261022_174150_annotated.png")
        );
    }

    #[test]
    fn errors_carry_kind_fields_and_message() {
        assert_eq!(
            serde_json::to_value(LibraryError::NotFound {
                id: id("20261022_174150")
            })
            .unwrap(),
            json!({
                "kind": "notFound",
                "id": "20261022_174150",
                "message": "キャプチャが見つかりません(20261022_174150)",
            })
        );
        let denied = serde_json::to_value(LibraryError::PermissionDenied {
            path: PathBuf::from("D:/locked"),
            message: "access denied".to_owned(),
        })
        .unwrap();
        assert_eq!(denied["kind"], "permissionDenied");
        assert_eq!(denied["path"], "D:/locked");
        assert_eq!(
            denied["message"],
            "保存先に書き込む権限がありません: access denied"
        );
        for (error, kind) in [
            (
                LibraryError::DiskFull {
                    message: String::new(),
                },
                "diskFull",
            ),
            (
                LibraryError::InvalidAnnotationDocument {
                    message: String::new(),
                },
                "invalidAnnotationDocument",
            ),
            (
                LibraryError::Clipboard {
                    message: String::new(),
                },
                "clipboard",
            ),
        ] {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], kind);
            assert!(value["message"].is_string());
        }
    }

    #[test]
    fn save_error_and_session_shapes() {
        let err = SaveCaptureError {
            reason: LibraryError::DiskFull {
                message: "no space".to_owned(),
            },
            unsaved: item("20261022_174150"),
        };
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["reason"]["kind"], "diskFull");
        assert_eq!(value["unsaved"]["id"], "20261022_174150");

        let session = EditSession {
            item: item("20261022_174150"),
            original_path: PathBuf::from("C:/shots/Snipt_20261022_174150.png"),
            annotations: None,
            warning: None,
            unsaved: true,
        };
        let value = serde_json::to_value(&session).unwrap();
        assert_eq!(value["unsaved"], true);
        assert!(value["annotations"].is_null());
        assert!(value.get("originalPath").is_some());
    }

    #[test]
    fn delete_outcome_shape() {
        let outcome = DeleteOutcome {
            removed: vec![id("20261022_174150")],
            failed: vec![DeleteFailure {
                id: id("20261022_174151"),
                error: LibraryError::Shell {
                    message: "in use".to_owned(),
                },
            }],
        };
        let value = serde_json::to_value(&outcome).unwrap();
        assert_eq!(value["removed"], json!(["20261022_174150"]));
        assert_eq!(value["failed"][0]["id"], "20261022_174151");
        assert_eq!(value["failed"][0]["error"]["kind"], "shell");
    }
}
