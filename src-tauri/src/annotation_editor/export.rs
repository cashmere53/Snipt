//! "名前を付けて保存": the format from the chosen file name, and writing through a temporary
//! file in the same folder so a failed write never damages an existing file.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::envelope::PNG_SIGNATURE;
use super::error::EditorError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Png,
    Jpeg,
}

const JPEG_SIGNATURE: [u8; 3] = [0xFF, 0xD8, 0xFF];

impl ExportFormat {
    pub fn matches(self, bytes: &[u8]) -> bool {
        match self {
            Self::Png => bytes.starts_with(&PNG_SIGNATURE),
            Self::Jpeg => bytes.starts_with(&JPEG_SIGNATURE),
        }
    }
}

/// `.jpg` / `.jpeg` (any case) are JPEG; everything else is PNG, and a name without an
/// extension gets `.png`.
pub fn resolve_target(path: PathBuf) -> (PathBuf, ExportFormat) {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("jpg" | "jpeg") => (path, ExportFormat::Jpeg),
        Some(_) => (path, ExportFormat::Png),
        None => (path.with_extension("png"), ExportFormat::Png),
    }
}

fn io(error: std::io::Error) -> EditorError {
    EditorError::Io(error.to_string())
}

/// Writes `bytes` to a temporary file next to `path`, then replaces `path` with it.
pub fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), EditorError> {
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or_else(|| EditorError::Io("保存先のフォルダがありません".to_owned()))?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.snipt-tmp", uuid::Uuid::new_v4()));
    let written = (|| {
        let mut file = fs::File::create_new(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&tmp);
        return Err(io(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_format_from_the_extension() {
        let cases = [
            ("C:/shots/a.jpg", "C:/shots/a.jpg", ExportFormat::Jpeg),
            ("C:/shots/a.JPEG", "C:/shots/a.JPEG", ExportFormat::Jpeg),
            ("C:/shots/a.png", "C:/shots/a.png", ExportFormat::Png),
            ("C:/shots/a.PNG", "C:/shots/a.PNG", ExportFormat::Png),
            ("C:/shots/a.bmp", "C:/shots/a.bmp", ExportFormat::Png),
            ("C:/shots/画面", "C:/shots/画面.png", ExportFormat::Png),
            ("C:/shots/v1.2", "C:/shots/v1.2", ExportFormat::Png),
        ];
        for (chosen, path, format) in cases {
            assert_eq!(
                resolve_target(PathBuf::from(chosen)),
                (PathBuf::from(path), format),
                "{chosen}"
            );
        }
    }

    #[test]
    fn checks_the_bytes_against_the_format() {
        assert!(ExportFormat::Png.matches(&PNG_SIGNATURE));
        assert!(!ExportFormat::Png.matches(&JPEG_SIGNATURE));
        assert!(ExportFormat::Jpeg.matches(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!ExportFormat::Jpeg.matches(&PNG_SIGNATURE));
    }

    #[test]
    fn replaces_an_existing_file_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.png");
        fs::write(&path, b"old").unwrap();

        write_replace(&path, b"new").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_failed_write_keeps_the_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        // The target is a folder, so the final rename fails after the temporary file is written.
        let path = dir.path().join("taken.png");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("inside"), b"keep").unwrap();

        let err = write_replace(&path, b"new").unwrap_err();

        assert_eq!(err.kind(), "io");
        assert_eq!(fs::read(path.join("inside")).unwrap(), b"keep");
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["taken.png"], "no temporary file is left");
    }

    #[test]
    fn a_missing_folder_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = write_replace(&dir.path().join("missing/out.png"), b"x").unwrap_err();
        assert_eq!(err.kind(), "io");
        assert_eq!(
            write_replace(Path::new("out.png"), b"x")
                .unwrap_err()
                .kind(),
            "io"
        );
    }
}
