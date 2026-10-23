//! `TrayController`: the notification-area icon, its menu and its tooltip.

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use super::dispatch::CaptureDispatch;
use super::hotkeys::{accelerator, HotkeyManager, HotkeyStatus};
use super::lifecycle::Lifecycle;
use super::settings::SettingsStore;
use super::windows::WindowManager;
use crate::shared::{CaptureMode, Settings};

pub const TRAY_ID: &str = "snipt-tray";

/// What a menu item or tray click asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Capture(CaptureMode),
    OpenHistory,
    OpenSettings,
    Quit,
}

/// One menu row: an item with its id, or a separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item {
        id: &'static str,
        label: &'static str,
        action: TrayAction,
    },
    Separator,
}

/// The tray menu, top to bottom (requirement 1.2).
pub const MENU: [MenuEntry; 8] = [
    MenuEntry::Item {
        id: "capture-region",
        label: "範囲キャプチャ",
        action: TrayAction::Capture(CaptureMode::Region),
    },
    MenuEntry::Item {
        id: "capture-window",
        label: "ウィンドウキャプチャ",
        action: TrayAction::Capture(CaptureMode::Window),
    },
    MenuEntry::Item {
        id: "capture-fullscreen",
        label: "全画面キャプチャ",
        action: TrayAction::Capture(CaptureMode::Fullscreen),
    },
    MenuEntry::Separator,
    MenuEntry::Item {
        id: "open-history",
        label: "履歴を開く",
        action: TrayAction::OpenHistory,
    },
    MenuEntry::Item {
        id: "open-settings",
        label: "設定",
        action: TrayAction::OpenSettings,
    },
    MenuEntry::Separator,
    MenuEntry::Item {
        id: "quit",
        label: "終了",
        action: TrayAction::Quit,
    },
];

/// The action for a menu item id.
pub fn action_for_menu(id: &str) -> Option<TrayAction> {
    MENU.iter().find_map(|entry| match entry {
        MenuEntry::Item {
            id: item, action, ..
        } if *item == id => Some(*action),
        _ => None,
    })
}

/// The action for a tray icon event: a left double click opens the history (1.4).
pub fn action_for_icon(event: &TrayIconEvent) -> Option<TrayAction> {
    match event {
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => Some(TrayAction::OpenHistory),
        _ => None,
    }
}

/// `"Snipt - 範囲キャプチャ: Ctrl + PrintScreen"`, or `"…: 未登録"` when the region hotkey
/// is not registered (requirement 1.5).
pub fn tooltip_text(settings: &Settings, statuses: &[HotkeyStatus]) -> String {
    let registered = statuses
        .iter()
        .any(|s| s.mode == CaptureMode::Region && s.registered);
    let key = if registered {
        accelerator::display(&settings.hotkeys.region)
    } else {
        "未登録".to_owned()
    };
    format!("Snipt - 範囲キャプチャ: {key}")
}

/// Where tray actions go.
pub trait TrayTargets: Send + Sync + 'static {
    fn capture(&self, mode: CaptureMode);
    fn open_history(&self);
    fn open_settings(&self);
    fn quit(&self);
}

pub fn perform(targets: &dyn TrayTargets, action: TrayAction) {
    match action {
        TrayAction::Capture(mode) => targets.capture(mode),
        TrayAction::OpenHistory => targets.open_history(),
        TrayAction::OpenSettings => targets.open_settings(),
        TrayAction::Quit => targets.quit(),
    }
}

/// Production targets: `CaptureDispatch`, `WindowManager` and `Lifecycle`.
pub struct AppTrayTargets<R: Runtime> {
    pub dispatch: CaptureDispatch<R>,
    pub windows: WindowManager<R>,
    pub lifecycle: Arc<Lifecycle>,
}

impl<R: Runtime> TrayTargets for AppTrayTargets<R> {
    fn capture(&self, mode: CaptureMode) {
        self.dispatch.request(mode);
    }

    fn open_history(&self) {
        if let Err(e) = self.windows.show_history() {
            log::error!("tray: could not open history: {e}");
        }
    }

    fn open_settings(&self) {
        if let Err(e) = self.windows.show_settings() {
            log::error!("tray: could not open settings: {e}");
        }
    }

    fn quit(&self) {
        self.lifecycle.quit();
    }
}

/// Creates the tray icon. Left click does not open the menu.
pub fn create<R: Runtime>(
    app: &AppHandle<R>,
    targets: Arc<dyn TrayTargets>,
    tooltip: &str,
) -> tauri::Result<TrayIcon<R>> {
    let items: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = MENU
        .iter()
        .map(
            |entry| -> tauri::Result<Box<dyn tauri::menu::IsMenuItem<R>>> {
                Ok(match entry {
                    MenuEntry::Item { id, label, .. } => {
                        Box::new(MenuItem::with_id(app, *id, *label, true, None::<&str>)?)
                    }
                    MenuEntry::Separator => Box::new(PredefinedMenuItem::separator(app)?),
                })
            },
        )
        .collect::<tauri::Result<_>>()?;
    let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = items.iter().map(|i| i.as_ref()).collect();
    let menu = Menu::with_items(app, &refs)?;

    let menu_targets = targets.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(tooltip)
        .on_menu_event(move |_, event| match action_for_menu(event.id().as_ref()) {
            Some(action) => perform(menu_targets.as_ref(), action),
            None => log::warn!("tray: unknown menu id {:?}", event.id()),
        })
        .on_tray_icon_event(move |_, event| {
            if let Some(action) = action_for_icon(&event) {
                perform(targets.as_ref(), action);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)
}

/// Updates the tooltip after a settings commit or a hotkey retry.
pub fn refresh_tooltip<R: Runtime>(tray: &TrayIcon<R>, text: &str) {
    if let Err(e) = tray.set_tooltip(Some(text)) {
        log::warn!("tray: could not update tooltip: {e}");
    }
}

/// The notification-area icon as startup sees it, so tests can run startup without a desktop.
pub trait TrayHost: Send + Sync + 'static {
    fn create(&self, targets: Arc<dyn TrayTargets>, tooltip: &str) -> tauri::Result<()>;
    /// Does nothing before `create`.
    fn set_tooltip(&self, text: &str);
}

/// Production host: the Tauri tray icon with id `TRAY_ID`.
pub struct AppTray<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> AppTray<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> TrayHost for AppTray<R> {
    fn create(&self, targets: Arc<dyn TrayTargets>, tooltip: &str) -> tauri::Result<()> {
        create(&self.app, targets, tooltip).map(|_| ())
    }

    fn set_tooltip(&self, text: &str) {
        if let Some(tray) = self.app.tray_by_id(TRAY_ID) {
            refresh_tooltip(&tray, text);
        }
    }
}

/// Rebuilds the tooltip from the managed `SettingsStore` and `HotkeyManager`. Does nothing
/// before stage 1 has run.
pub fn refresh_from_state<R: Runtime>(app: &AppHandle<R>) {
    let (Some(tray), Some(store), Some(hotkeys)) = (
        app.try_state::<Arc<dyn TrayHost>>(),
        app.try_state::<Arc<SettingsStore>>(),
        app.try_state::<Arc<HotkeyManager>>(),
    ) else {
        return;
    };
    tray.set_tooltip(&tooltip_text(&store.get(), &hotkeys.statuses()));
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Mutex;

    use tauri::tray::{MouseButtonState, TrayIconId};
    use tauri::{PhysicalPosition, Rect};

    use super::*;
    use crate::app_shell::hotkeys::HotkeyFailureKind;

    #[derive(Default)]
    struct Recorded(Mutex<Vec<TrayAction>>);

    impl TrayTargets for Recorded {
        fn capture(&self, mode: CaptureMode) {
            self.0.lock().unwrap().push(TrayAction::Capture(mode));
        }
        fn open_history(&self) {
            self.0.lock().unwrap().push(TrayAction::OpenHistory);
        }
        fn open_settings(&self) {
            self.0.lock().unwrap().push(TrayAction::OpenSettings);
        }
        fn quit(&self) {
            self.0.lock().unwrap().push(TrayAction::Quit);
        }
    }

    #[test]
    fn menu_has_six_items_in_order() {
        let labels: Vec<&str> = MENU
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item { label, .. } => Some(*label),
                MenuEntry::Separator => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                "範囲キャプチャ",
                "ウィンドウキャプチャ",
                "全画面キャプチャ",
                "履歴を開く",
                "設定",
                "終了"
            ]
        );
        assert_eq!(MENU[3], MenuEntry::Separator);
        assert_eq!(MENU[6], MenuEntry::Separator);
    }

    #[test]
    fn menu_ids_reach_their_targets() {
        let cases = [
            ("capture-region", TrayAction::Capture(CaptureMode::Region)),
            ("capture-window", TrayAction::Capture(CaptureMode::Window)),
            (
                "capture-fullscreen",
                TrayAction::Capture(CaptureMode::Fullscreen),
            ),
            ("open-history", TrayAction::OpenHistory),
            ("open-settings", TrayAction::OpenSettings),
            ("quit", TrayAction::Quit),
        ];
        let targets = Recorded::default();
        for (id, expected) in cases {
            let action = action_for_menu(id).unwrap();
            assert_eq!(action, expected, "{id}");
            perform(&targets, action);
        }
        assert_eq!(
            *targets.0.lock().unwrap(),
            cases.map(|(_, action)| action).to_vec()
        );
        assert_eq!(action_for_menu("unknown"), None);
    }

    fn double_click(button: MouseButton) -> TrayIconEvent {
        TrayIconEvent::DoubleClick {
            id: TrayIconId::new(TRAY_ID),
            position: PhysicalPosition::default(),
            rect: Rect::default(),
            button,
        }
    }

    #[test]
    fn left_double_click_opens_history() {
        assert_eq!(
            action_for_icon(&double_click(MouseButton::Left)),
            Some(TrayAction::OpenHistory)
        );
        assert_eq!(action_for_icon(&double_click(MouseButton::Right)), None);
        let click = TrayIconEvent::Click {
            id: TrayIconId::new(TRAY_ID),
            position: PhysicalPosition::default(),
            rect: Rect::default(),
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
        };
        assert_eq!(action_for_icon(&click), None);
    }

    fn status(registered: bool) -> HotkeyStatus {
        HotkeyStatus {
            mode: CaptureMode::Region,
            accelerator: accelerator::parse("Ctrl+PrintScreen").unwrap(),
            registered,
            failure: (!registered).then_some(HotkeyFailureKind::Conflict),
        }
    }

    #[test]
    fn tooltip_shows_region_hotkey_or_unregistered() {
        let mut settings = Settings::default_with(Path::new("/pictures"));
        settings.hotkeys.region = accelerator::parse("Ctrl+PrintScreen").unwrap();

        assert_eq!(
            tooltip_text(&settings, &[status(true)]),
            "Snipt - 範囲キャプチャ: Ctrl + PrintScreen"
        );
        assert_eq!(
            tooltip_text(&settings, &[status(false)]),
            "Snipt - 範囲キャプチャ: 未登録"
        );
    }
}
