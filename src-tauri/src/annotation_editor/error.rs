//! `EditorError`: every failure the editor's commands report. Serialized as
//! `{ kind, message }` (`EditorErrorDto` in `src/shared/ipc/annotation-editor.ts`), where
//! `message` is the user-facing text.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::capture_library::LibraryError;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditorError {
    #[error("キャプチャが見つかりません")]
    NotFound,
    #[error("編集画面のキャプチャ ID が正しくありません")]
    InvalidCaptureId,
    #[error("編集内容を読み取れませんでした: {0}")]
    InvalidPayload(String),
    /// Carries capture-library's own user-facing message.
    #[error("{0}")]
    Library(String),
    #[error("クリップボードにコピーできませんでした: {0}")]
    Clipboard(String),
    #[error("保存ダイアログを開けませんでした: {0}")]
    Dialog(String),
    #[error("ファイルを書き込めませんでした: {0}")]
    Io(String),
    #[error("保存先が見つかりません。もう一度保存先を選んでください")]
    UnknownToken,
}

impl EditorError {
    /// The `EditorErrorKind` string.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound => "notFound",
            Self::InvalidCaptureId => "invalidCaptureId",
            Self::InvalidPayload(_) => "invalidPayload",
            Self::Library(_) => "library",
            Self::Clipboard(_) => "clipboard",
            Self::Dialog(_) => "dialog",
            Self::Io(_) => "io",
            Self::UnknownToken => "unknownToken",
        }
    }
}

impl Serialize for EditorError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut dto = serializer.serialize_struct("EditorError", 2)?;
        dto.serialize_field("kind", self.kind())?;
        dto.serialize_field("message", &self.to_string())?;
        dto.end()
    }
}

impl From<LibraryError> for EditorError {
    fn from(error: LibraryError) -> Self {
        match error {
            LibraryError::NotFound { .. } => Self::NotFound,
            LibraryError::Clipboard { message } => Self::Clipboard(message),
            other => Self::Library(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::shared::CaptureId;

    #[test]
    fn serializes_as_kind_and_message() {
        assert_eq!(
            serde_json::to_value(EditorError::Io("ディスクがいっぱいです".to_owned())).unwrap(),
            json!({
                "kind": "io",
                "message": "ファイルを書き込めませんでした: ディスクがいっぱいです",
            })
        );
    }

    #[test]
    fn has_the_eight_kinds_of_the_ipc_contract() {
        // The same list, in the same order, as EDITOR_ERROR_KINDS in the TS contract.
        let all = [
            EditorError::NotFound,
            EditorError::InvalidCaptureId,
            EditorError::InvalidPayload(String::new()),
            EditorError::Library(String::new()),
            EditorError::Clipboard(String::new()),
            EditorError::Dialog(String::new()),
            EditorError::Io(String::new()),
            EditorError::UnknownToken,
        ];
        let kinds: Vec<_> = all.iter().map(EditorError::kind).collect();
        assert_eq!(
            kinds,
            [
                "notFound",
                "invalidCaptureId",
                "invalidPayload",
                "library",
                "clipboard",
                "dialog",
                "io",
                "unknownToken"
            ]
        );
        let contract = include_str!("../../../src/shared/ipc/annotation-editor.ts");
        for kind in kinds {
            assert!(contract.contains(&format!("\"{kind}\",")), "{kind}");
        }
    }

    #[test]
    fn library_errors_keep_their_message() {
        let id = CaptureId::parse("20261022_174150").unwrap();
        assert_eq!(
            EditorError::from(LibraryError::NotFound { id }),
            EditorError::NotFound
        );
        let disk_full = LibraryError::DiskFull {
            message: "残り 0 バイト".to_owned(),
        };
        assert_eq!(
            EditorError::from(disk_full.clone()),
            EditorError::Library(disk_full.to_string())
        );
        assert_eq!(
            EditorError::from(LibraryError::Clipboard {
                message: "使用中".to_owned()
            })
            .kind(),
            "clipboard"
        );
    }
}
