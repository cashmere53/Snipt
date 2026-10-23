//! `LibraryLayout`: where each file of a save folder lives.
//!
//! ```text
//! <root>/Snipt_<id>.png, Snipt_<id>_annotated.png
//! <root>/.snipt/index.json, annotations/<id>.json, thumbs/<id>.png, tmp/
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use super::atomic_fs::fs_error;
use super::naming::base_name;
use super::types::LibraryError;
use crate::shared::CaptureId;

const SNIPT_DIR: &str = ".snipt";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryLayout {
    root: PathBuf,
}

impl LibraryLayout {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn original(&self, id: &CaptureId) -> PathBuf {
        self.root.join(format!("{}.png", base_name(id)))
    }

    pub fn annotated(&self, id: &CaptureId) -> PathBuf {
        self.root.join(format!("{}_annotated.png", base_name(id)))
    }

    pub fn snipt_dir(&self) -> PathBuf {
        self.root.join(SNIPT_DIR)
    }

    pub fn annotations_dir(&self) -> PathBuf {
        self.snipt_dir().join("annotations")
    }

    pub fn annotations(&self, id: &CaptureId) -> PathBuf {
        self.annotations_dir().join(format!("{id}.json"))
    }

    pub fn thumbs_dir(&self) -> PathBuf {
        self.snipt_dir().join("thumbs")
    }

    pub fn thumbnail(&self, id: &CaptureId) -> PathBuf {
        self.thumbs_dir().join(format!("{id}.png"))
    }

    pub fn index_file(&self) -> PathBuf {
        self.snipt_dir().join("index.json")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.snipt_dir().join("tmp")
    }

    /// Creates the root and `.snipt` with its subfolders when missing, and hides `.snipt`
    /// (requirement 1.5). A failure to hide is only logged.
    pub fn ensure_dirs(&self) -> Result<(), LibraryError> {
        for dir in [self.annotations_dir(), self.thumbs_dir(), self.tmp_dir()] {
            fs::create_dir_all(&dir).map_err(|e| match fs_error(&dir, &e) {
                LibraryError::Io { message } => LibraryError::SaveDirUnavailable {
                    path: self.root.clone(),
                    message,
                },
                other => other,
            })?;
        }
        self.hide_snipt_dir();
        Ok(())
    }

    #[cfg(windows)]
    fn hide_snipt_dir(&self) {
        if let Err(e) = crate::platform::windows::shell::set_hidden(&self.snipt_dir()) {
            log::warn!(
                "library: could not hide {}: {e}",
                self.snipt_dir().display()
            );
        }
    }

    #[cfg(not(windows))]
    fn hide_snipt_dir(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> CaptureId {
        CaptureId::parse("20261022_174150_02").unwrap()
    }

    #[test]
    fn derives_every_path_from_the_root() {
        let layout = LibraryLayout::new(PathBuf::from("D:/shots"));
        let root = Path::new("D:/shots");
        assert_eq!(
            layout.original(&id()),
            root.join("Snipt_20261022_174150_02.png")
        );
        assert_eq!(
            layout.annotated(&id()),
            root.join("Snipt_20261022_174150_02_annotated.png")
        );
        assert_eq!(
            layout.annotations(&id()),
            root.join(".snipt/annotations/20261022_174150_02.json")
        );
        assert_eq!(
            layout.thumbnail(&id()),
            root.join(".snipt/thumbs/20261022_174150_02.png")
        );
        assert_eq!(layout.index_file(), root.join(".snipt/index.json"));
        assert_eq!(layout.tmp_dir(), root.join(".snipt/tmp"));
    }

    #[test]
    fn ensure_dirs_creates_a_missing_save_folder() {
        let parent = tempfile::tempdir().unwrap();
        let layout = LibraryLayout::new(parent.path().join("Pictures").join("Snipt"));

        layout.ensure_dirs().unwrap();
        layout.ensure_dirs().unwrap();

        for dir in [
            layout.root().to_owned(),
            layout.annotations_dir(),
            layout.thumbs_dir(),
            layout.tmp_dir(),
        ] {
            assert!(dir.is_dir(), "{}", dir.display());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            let attributes = fs::metadata(layout.snipt_dir()).unwrap().file_attributes();
            assert_ne!(attributes & 0x2, 0, "FILE_ATTRIBUTE_HIDDEN");
        }
    }

    #[test]
    fn ensure_dirs_reports_an_unusable_root() {
        let parent = tempfile::tempdir().unwrap();
        let file = parent.path().join("file");
        fs::write(&file, b"x").unwrap();
        let layout = LibraryLayout::new(file.join("Snipt"));

        let err = layout.ensure_dirs().unwrap_err();
        assert!(
            matches!(
                err,
                LibraryError::SaveDirUnavailable { .. } | LibraryError::PermissionDenied { .. }
            ),
            "{err:?}"
        );
    }
}
