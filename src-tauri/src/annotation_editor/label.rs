//! Window labels: the editor of capture `<id>` is the window `editor-<id>`, so a command knows
//! its capture from the calling window and never takes an id argument.

use super::error::EditorError;
use crate::shared::CaptureId;

const PREFIX: &str = "editor-";

/// Tauri accepts only these characters in labels.
fn label_safe(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'/' | b':' | b'_'))
}

pub fn editor_label(id: &CaptureId) -> Result<String, EditorError> {
    if label_safe(id.as_str()) {
        Ok(format!("{PREFIX}{id}"))
    } else {
        Err(EditorError::InvalidCaptureId)
    }
}

/// The capture of an editor window; other windows' labels are rejected.
pub fn capture_id_of(label: &str) -> Result<CaptureId, EditorError> {
    label
        .strip_prefix(PREFIX)
        .and_then(|id| CaptureId::parse(id).ok())
        .ok_or(EditorError::InvalidCaptureId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_both_id_forms() {
        for raw in ["20261022_174150", "20261022_174150_02"] {
            let id = CaptureId::parse(raw).unwrap();
            let label = editor_label(&id).unwrap();
            assert_eq!(label, format!("editor-{raw}"));
            assert_eq!(capture_id_of(&label).unwrap(), id);
        }
    }

    #[test]
    fn rejects_other_windows_and_bad_ids() {
        for label in [
            "history",
            "settings",
            "overlay-0",
            "editor-",
            "editor-abc",
            "editor-20261022_174150_01",
            "editor-../20261022_174150",
            "Editor-20261022_174150",
            "editor-20261022_174150 ",
        ] {
            assert_eq!(
                capture_id_of(label),
                Err(EditorError::InvalidCaptureId),
                "{label}"
            );
        }
    }

    #[test]
    fn checks_label_characters() {
        assert!(label_safe("20261022_174150"));
        assert!(!label_safe("a b"));
        assert!(!label_safe("あ"));
        assert!(!label_safe(""));
    }
}
