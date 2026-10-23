//! Checks a settings draft before it is committed: hotkey validity and duplicates, and whether
//! the save folder is writable.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app_shell::hotkeys::accelerator::{self, InvalidAccelerator};
use crate::shared::{Accelerator, CaptureMode, HotkeySettings};

/// Why a set of hotkeys was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    /// One mode's accelerator is not valid.
    Invalid {
        mode: CaptureMode,
        reason: InvalidAccelerator,
    },
    /// The same accelerator is assigned to two or more modes (in `CaptureMode::ALL` order).
    Duplicate {
        accelerator: Accelerator,
        modes: Vec<CaptureMode>,
    },
}

/// Re-validates every accelerator and returns the hotkeys in normalized form.
///
/// Invalid accelerators are reported first (in `CaptureMode::ALL` order), then the first
/// duplicate. Duplicates are compared after normalization, so `"ctrl+KeyA"` and `"Ctrl+KeyA"`
/// collide.
pub fn validate_hotkeys(hotkeys: &HotkeySettings) -> Result<HotkeySettings, HotkeyError> {
    let mut normalized = Vec::with_capacity(CaptureMode::ALL.len());
    for mode in CaptureMode::ALL {
        let accelerator = accelerator::parse(hotkeys.get(mode).as_str())
            .map_err(|reason| HotkeyError::Invalid { mode, reason })?;
        normalized.push((mode, accelerator));
    }

    for (i, (_, accelerator)) in normalized.iter().enumerate() {
        let modes: Vec<CaptureMode> = normalized
            .iter()
            .filter(|(_, other)| other == accelerator)
            .map(|(mode, _)| *mode)
            .collect();
        // Report each duplicate once, at its first occurrence.
        if modes.len() > 1 && normalized[i].0 == modes[0] {
            return Err(HotkeyError::Duplicate {
                accelerator: accelerator.clone(),
                modes,
            });
        }
    }

    let mut keys = normalized.into_iter().map(|(_, accelerator)| accelerator);
    Ok(HotkeySettings {
        region: keys.next().expect("three modes"),
        window: keys.next().expect("three modes"),
        fullscreen: keys.next().expect("three modes"),
    })
}

/// The save folder cannot be used. `path` is the folder as the user chose it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveDirNotWritable {
    pub path: PathBuf,
    pub message: String,
}

/// Makes `path` absolute, creates it if missing, and checks that a uniquely named file can be
/// created and removed in it. Returns the absolute path.
pub fn check_save_dir(path: &Path) -> Result<PathBuf, SaveDirNotWritable> {
    let fail = |message: String| SaveDirNotWritable {
        path: path.to_path_buf(),
        message,
    };
    if path.as_os_str().is_empty() {
        return Err(fail("フォルダが指定されていません".to_owned()));
    }
    let dir = std::path::absolute(path).map_err(|e| fail(e.to_string()))?;
    fs::create_dir_all(&dir).map_err(|e| fail(e.to_string()))?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let probe = dir.join(format!(
        ".snipt-write-check-{}-{nanos}.tmp",
        std::process::id()
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|e| fail(e.to_string()))?;
    fs::remove_file(&probe).map_err(|e| fail(e.to_string()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hotkeys(region: &str, window: &str, fullscreen: &str) -> HotkeySettings {
        let key = |s: &str| Accelerator::from_normalized(s.to_owned());
        HotkeySettings {
            region: key(region),
            window: key(window),
            fullscreen: key(fullscreen),
        }
    }

    #[test]
    fn accepts_and_normalizes_distinct_hotkeys() {
        let result = validate_hotkeys(&hotkeys("PrintScreen", "ctrl+PrintScreen", "Shift+KeyA"));
        assert_eq!(
            result,
            Ok(hotkeys("PrintScreen", "Ctrl+PrintScreen", "Shift+KeyA"))
        );
    }

    #[test]
    fn detects_duplicate_with_all_involved_modes() {
        let result = validate_hotkeys(&hotkeys("Ctrl+KeyA", "PrintScreen", "control+KeyA"));
        assert_eq!(
            result,
            Err(HotkeyError::Duplicate {
                accelerator: Accelerator::from_normalized("Ctrl+KeyA".to_owned()),
                modes: vec![CaptureMode::Region, CaptureMode::Fullscreen],
            })
        );
    }

    #[test]
    fn detects_triple_duplicate() {
        let result = validate_hotkeys(&hotkeys("PrintScreen", "PrintScreen", "PrintScreen"));
        assert_eq!(
            result,
            Err(HotkeyError::Duplicate {
                accelerator: Accelerator::from_normalized("PrintScreen".to_owned()),
                modes: CaptureMode::ALL.to_vec(),
            })
        );
    }

    #[test]
    fn reports_invalid_hotkey_per_mode() {
        let result = validate_hotkeys(&hotkeys("PrintScreen", "KeyA", "Shift+PrintScreen"));
        assert_eq!(
            result,
            Err(HotkeyError::Invalid {
                mode: CaptureMode::Window,
                reason: InvalidAccelerator::ModifierRequired,
            })
        );
        let result = validate_hotkeys(&hotkeys("PrintScreen", "Ctrl+PrintScreen", "Ctrl"));
        assert_eq!(
            result,
            Err(HotkeyError::Invalid {
                mode: CaptureMode::Fullscreen,
                reason: InvalidAccelerator::NoKey,
            })
        );
    }

    #[test]
    fn writable_dir_is_created_and_left_clean() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("nested").join("Snipt");
        let checked = check_save_dir(&target).unwrap();
        assert!(checked.is_absolute());
        assert!(target.is_dir());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    }

    #[test]
    fn dir_that_cannot_be_created_is_not_writable() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("not-a-folder");
        fs::write(&file, b"x").unwrap();
        let target = file.join("Snipt");
        let err = check_save_dir(&target).unwrap_err();
        assert_eq!(err.path, target);
        assert!(!err.message.is_empty());
    }

    #[test]
    fn existing_file_is_not_a_writable_dir() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file.png");
        fs::write(&file, b"x").unwrap();
        assert!(check_save_dir(&file).is_err());
    }

    #[test]
    fn empty_path_is_rejected() {
        assert!(check_save_dir(Path::new("")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn read_only_dir_is_not_writable() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("readonly");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        // Root ignores permission bits, so only assert when the probe really is blocked.
        let blocked = fs::write(dir.join("probe"), b"x").is_err();
        let result = check_save_dir(&dir);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(result.is_err(), blocked);
    }
}
