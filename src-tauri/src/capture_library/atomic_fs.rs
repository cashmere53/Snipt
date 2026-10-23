//! Atomic file replacement: write to `.snipt/tmp`, flush, then rename over the destination
//! (same volume, so readers never see a half-written file).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::types::LibraryError;

/// `ERROR_DISK_FULL` and `ERROR_HANDLE_DISK_FULL`.
const WINDOWS_DISK_FULL: [i32; 2] = [112, 39];

/// Maps a file system error at `path` to the reason shown to the user (requirement 1.6).
pub fn fs_error(path: &Path, error: &io::Error) -> LibraryError {
    let message = error.to_string();
    let disk_full = error.kind() == io::ErrorKind::StorageFull
        || (cfg!(windows) && WINDOWS_DISK_FULL.contains(&error.raw_os_error().unwrap_or(0)));
    if disk_full {
        LibraryError::DiskFull { message }
    } else if error.kind() == io::ErrorKind::PermissionDenied {
        LibraryError::PermissionDenied {
            path: path.to_owned(),
            message,
        }
    } else {
        LibraryError::Io { message }
    }
}

static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

fn tmp_path(tmp_dir: &Path, dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let n = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
    tmp_dir.join(format!("{name}.{}.{n}.tmp", std::process::id()))
}

/// Replaces `dest` with `bytes` atomically. `tmp_dir` must be on the same volume.
pub fn write_replace(tmp_dir: &Path, dest: &Path, bytes: &[u8]) -> Result<(), LibraryError> {
    let tmp = tmp_path(tmp_dir, dest);
    let written = File::create(&tmp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(fs_error(dest, &e));
    }
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        fs_error(dest, &e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let tmp = root.path().join("tmp");
        fs::create_dir(&tmp).unwrap();
        (root, tmp)
    }

    #[test]
    fn replaces_the_contents_and_leaves_no_temporary_file() {
        let (root, tmp) = setup();
        let dest = root.path().join("a.png");

        write_replace(&tmp, &dest, b"first").unwrap();
        write_replace(&tmp, &dest, b"second").unwrap();

        assert_eq!(fs::read(&dest).unwrap(), b"second");
        assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
    }

    #[test]
    fn maps_error_kinds_to_reasons() {
        let path = Path::new("D:/shots/a.png");
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(fs_error(path, &denied).kind(), "permissionDenied");
        let full = io::Error::from(io::ErrorKind::StorageFull);
        assert_eq!(fs_error(path, &full).kind(), "diskFull");
        let other = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(fs_error(path, &other).kind(), "io");
        #[cfg(windows)]
        assert_eq!(
            fs_error(path, &io::Error::from_raw_os_error(112)).kind(),
            "diskFull"
        );
    }

    /// Replacing a read-only file is refused on Windows.
    #[cfg(windows)]
    #[test]
    fn read_only_destination_is_permission_denied() {
        let (root, tmp) = setup();
        let dest = root.path().join("a.png");
        fs::write(&dest, b"old").unwrap();
        let mut permissions = fs::metadata(&dest).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&dest, permissions).unwrap();

        let err = write_replace(&tmp, &dest, b"new").unwrap_err();

        assert_eq!(err.kind(), "permissionDenied", "{err:?}");
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
    }

    /// A read-only folder refuses the rename on Unix (root ignores permission bits).
    #[cfg(unix)]
    #[test]
    fn read_only_folder_is_permission_denied() {
        use std::os::unix::fs::PermissionsExt;
        let (root, tmp) = setup();
        let dir = root.path().join("locked");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();

        let result = write_replace(&tmp, &dir.join("a.png"), b"new");
        let running_as_root = fs::write(dir.join("probe"), b"").is_ok();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

        if running_as_root {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.unwrap_err().kind(), "permissionDenied");
            assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
        }
    }
}
