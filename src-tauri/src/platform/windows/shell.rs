//! Shell operations: the recycle bin, revealing a file in Explorer and hidden folders.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_FLAGS_AND_ATTRIBUTES,
    INVALID_FILE_ATTRIBUTES,
};

/// Moves every path to the recycle bin in one operation.
pub fn recycle(paths: &[PathBuf]) -> Result<(), String> {
    trash::delete_all(paths).map_err(|e| e.to_string())
}

/// Opens Explorer with `path` selected.
pub fn reveal(path: &Path) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// Adds the hidden attribute, keeping the others.
pub fn set_hidden(path: &Path) -> Result<(), String> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let name = PCWSTR(wide.as_ptr());
    // SAFETY: `wide` is NUL-terminated and outlives both calls.
    unsafe {
        let current = GetFileAttributesW(name);
        if current == INVALID_FILE_ATTRIBUTES {
            return Err(windows::core::Error::from_thread().message());
        }
        SetFileAttributesW(
            name,
            FILE_FLAGS_AND_ATTRIBUTES(current | FILE_ATTRIBUTE_HIDDEN.0),
        )
        .map_err(|e| e.message())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hidden(path: &Path) -> bool {
        use std::os::windows::fs::MetadataExt;
        std::fs::metadata(path).unwrap().file_attributes() & FILE_ATTRIBUTE_HIDDEN.0 != 0
    }

    #[test]
    fn set_hidden_marks_a_folder_hidden() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(".snipt");
        std::fs::create_dir(&dir).unwrap();
        assert!(!is_hidden(&dir));
        set_hidden(&dir).unwrap();
        assert!(is_hidden(&dir));
        assert!(dir.is_dir());
    }

    #[test]
    fn set_hidden_reports_a_missing_path() {
        let root = tempfile::tempdir().unwrap();
        assert!(set_hidden(&root.path().join("missing")).is_err());
    }
}
