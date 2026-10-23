use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::CaptureMode;

/// A normalized accelerator string: modifiers in `Ctrl+Alt+Shift+Win+` order followed by one
/// W3C `KeyboardEvent.code` name, e.g. `"PrintScreen"`, `"Ctrl+Shift+KeyA"`.
///
/// Only `app_shell::hotkeys::accelerator::parse` creates validated values. Values read from
/// disk are re-validated by the settings loader.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Accelerator(String);

impl Accelerator {
    /// For the accelerator parser only; does not validate.
    pub(crate) fn from_normalized(normalized: String) -> Self {
        Self(normalized)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeySettings {
    pub region: Accelerator,
    pub window: Accelerator,
    pub fullscreen: Accelerator,
}

impl HotkeySettings {
    pub fn get(&self, mode: CaptureMode) -> &Accelerator {
        match mode {
            CaptureMode::Region => &self.region,
            CaptureMode::Window => &self.window,
            CaptureMode::Fullscreen => &self.fullscreen,
        }
    }
}

/// Serialized as `"cursorMonitor" | "allMonitors"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FullscreenTarget {
    CursorMonitor,
    AllMonitors,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub hotkeys: HotkeySettings,
    pub fullscreen_target: FullscreenTarget,
    pub save_dir: PathBuf,
    pub autostart: bool,
}

impl Settings {
    /// Defaults: PrintScreen / Ctrl+PrintScreen / Shift+PrintScreen, the cursor's monitor,
    /// `<picture_dir>\Snipt` and autostart enabled.
    pub fn default_with(picture_dir: &Path) -> Settings {
        let key = |s: &str| Accelerator::from_normalized(s.to_owned());
        Settings {
            hotkeys: HotkeySettings {
                region: key("PrintScreen"),
                window: key("Ctrl+PrintScreen"),
                fullscreen: key("Shift+PrintScreen"),
            },
            fullscreen_target: FullscreenTarget::CursorMonitor,
            save_dir: picture_dir.join("Snipt"),
            autostart: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_requirements() {
        let s = Settings::default_with(Path::new("/pictures"));
        assert_eq!(s.hotkeys.get(CaptureMode::Region).as_str(), "PrintScreen");
        assert_eq!(
            s.hotkeys.get(CaptureMode::Window).as_str(),
            "Ctrl+PrintScreen"
        );
        assert_eq!(
            s.hotkeys.get(CaptureMode::Fullscreen).as_str(),
            "Shift+PrintScreen"
        );
        assert_eq!(s.fullscreen_target, FullscreenTarget::CursorMonitor);
        assert_eq!(s.save_dir, Path::new("/pictures").join("Snipt"));
        assert!(s.autostart);
    }

    #[test]
    fn serializes_with_persisted_key_names() {
        let s = Settings::default_with(Path::new("/pictures"));
        let value = serde_json::to_value(&s).unwrap();
        assert_eq!(
            value,
            json!({
                "hotkeys": {
                    "region": "PrintScreen",
                    "window": "Ctrl+PrintScreen",
                    "fullscreen": "Shift+PrintScreen"
                },
                "fullscreenTarget": "cursorMonitor",
                "saveDir": Path::new("/pictures").join("Snipt"),
                "autostart": true
            })
        );
        assert_eq!(serde_json::from_value::<Settings>(value).unwrap(), s);
    }

    #[test]
    fn capture_mode_and_target_use_camel_case() {
        assert_eq!(
            serde_json::to_value(CaptureMode::Fullscreen).unwrap(),
            json!("fullscreen")
        );
        assert_eq!(
            serde_json::to_value(FullscreenTarget::AllMonitors).unwrap(),
            json!("allMonitors")
        );
    }
}
