//! Two-stage startup: stage 1 (`setup`) loads the settings and manages the shared services;
//! stage 2 (`start`) creates the tray, registers the hotkeys and syncs autostart once every
//! downstream spec has run its own `setup`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{App, AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{Shortcut, ShortcutEvent, ShortcutState};

use super::autostart::{AutostartSync, PluginAutostart};
use super::dispatch::CaptureDispatch;
use super::hotkeys::{
    accelerator, GlobalShortcutBackend, HotkeyFailureKind, HotkeyManager, HotkeyStatus,
};
use super::lifecycle::{AppExitActions, BeforeExitHooks, DialogExitConfirm, Lifecycle};
use super::notifier::ToastNotifier;
use super::service::{SettingsService, SETTINGS_CHANGED_EVENT};
use super::settings::{LoadOutcome, SettingsStore};
use super::tray::{self, AppTrayTargets};
use super::windows::WindowManager;
use crate::shared::{CaptureMode, Notice, NoticeLevel, Notifier};

const NOTICE_TITLE: &str = "Snipt";

/// How the settings were loaded, handed from stage 1 to stage 2.
struct PendingLoadOutcome(Mutex<Option<LoadOutcome>>);

pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: app-shell (stage 1)");
    let settings_path = app.path().app_config_dir()?.join("settings.json");
    let picture_dir = app.path().picture_dir()?;
    setup_at(app.handle(), settings_path, &picture_dir);
    Ok(())
}

/// Stage 1 with explicit paths (tests use temporary ones). Manages `Arc<SettingsStore>`, `Arc<dyn Notifier>`,
/// `Arc<BeforeExitHooks>`, `Arc<HotkeyManager>`, `Arc<Lifecycle>` and `Arc<SettingsService>`.
/// Nothing is registered with the OS yet.
pub fn setup_at<R: Runtime>(app: &AppHandle<R>, settings_path: PathBuf, picture_dir: &Path) {
    let (store, outcome) = SettingsStore::load(settings_path, picture_dir);
    log::info!("settings: {outcome:?}");
    let store = Arc::new(store);

    let notifier: Arc<dyn Notifier> = Arc::new(ToastNotifier::new(app.clone()));
    let hooks = Arc::new(BeforeExitHooks::default());
    let hotkeys = Arc::new(HotkeyManager::new(
        Box::new(GlobalShortcutBackend::new(app.clone())),
        HotkeyManager::windows_probe(),
        store.get().hotkeys.clone(),
    ));
    let lifecycle = Arc::new(Lifecycle::new(
        hooks.clone(),
        Box::new(DialogExitConfirm::new(app.clone())),
        Box::new(AppExitActions::new(app.clone(), hotkeys.clone())),
        notifier.clone(),
    ));
    let autostart = Arc::new(AutostartSync::new(Box::new(PluginAutostart::new(
        app.clone(),
    ))));
    let emitter = app.clone();
    let service = Arc::new(SettingsService::new(
        store.clone(),
        hotkeys.clone(),
        autostart.clone(),
        Box::new(move |settings| {
            if let Err(e) = emitter.emit(SETTINGS_CHANGED_EVENT, settings) {
                log::warn!("settings: could not emit {SETTINGS_CHANGED_EVENT}: {e}");
            }
        }),
    ));

    app.manage(store);
    app.manage(notifier);
    app.manage(hooks);
    app.manage(hotkeys);
    app.manage(lifecycle);
    app.manage(autostart);
    app.manage(service);
    app.manage(PendingLoadOutcome(Mutex::new(Some(outcome))));
}

pub fn start<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("setup: app-shell (stage 2)");
    let app = app.handle();
    let store = app.state::<Arc<SettingsStore>>().inner().clone();
    let hotkeys = app.state::<Arc<HotkeyManager>>().inner().clone();
    let notifier = app.state::<Arc<dyn Notifier>>().inner().clone();
    let settings = store.get();

    let statuses = hotkeys.register_initial(&settings.hotkeys);
    let targets = Arc::new(AppTrayTargets {
        dispatch: CaptureDispatch::new(app.clone()),
        windows: WindowManager::new(app.clone()),
        lifecycle: app.state::<Arc<Lifecycle>>().inner().clone(),
    });
    tray::create(app, targets, &tray::tooltip_text(&settings, &statuses))?;
    let tooltip_app = app.clone();
    store.subscribe(Box::new(move |_| tray::refresh_from_state(&tooltip_app)));

    let outcome = app
        .state::<PendingLoadOutcome>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    let autostart = app.state::<Arc<AutostartSync>>().apply(settings.autostart);
    for notice in startup_notices(outcome.as_ref(), &statuses, autostart.err()) {
        notifier.notify(notice);
    }
    Ok(())
}

/// The global-shortcut plugin handler: resolves a pressed hotkey to its capture mode.
pub fn on_shortcut<R: Runtime>(app: &AppHandle<R>, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let Some(hotkeys) = app.try_state::<Arc<HotkeyManager>>() else {
        return;
    };
    match hotkeys.mode_for(shortcut) {
        Some(mode) => CaptureDispatch::new(app.clone()).request(mode),
        None => log::warn!("hotkeys: pressed an unknown shortcut {shortcut:?}"),
    }
}

/// The single-instance plugin callback, run in the first instance.
pub fn on_second_instance<R: Runtime>(app: &AppHandle<R>, argv: &[String]) {
    log::info!("second instance started: {argv:?}");
    if let Some(lifecycle) = app.try_state::<Arc<Lifecycle>>() {
        lifecycle.on_second_instance();
    }
}

/// Notices shown once startup is done: settings reset (4.3), failed hotkeys merged into one
/// (2.3, 2.4) and an autostart sync failure.
fn startup_notices(
    outcome: Option<&LoadOutcome>,
    statuses: &[HotkeyStatus],
    autostart_error: Option<String>,
) -> Vec<Notice> {
    let mut notices = Vec::new();
    if let Some(LoadOutcome::RecoveredFromCorruption { backup }) = outcome {
        let kept = match backup.as_ref().and_then(|b| b.file_name()) {
            Some(name) => format!(
                "元のファイルは {} として残しています。",
                name.to_string_lossy()
            ),
            None => String::new(),
        };
        notices.push(Notice {
            level: NoticeLevel::Warning,
            title: NOTICE_TITLE.to_owned(),
            body: format!("設定を読み込めなかったため、初期設定に戻しました。{kept}"),
        });
    }
    if let Some(notice) = hotkey_failure_notice(statuses) {
        notices.push(notice);
    }
    if let Some(message) = autostart_error {
        notices.push(Notice {
            level: NoticeLevel::Warning,
            title: NOTICE_TITLE.to_owned(),
            body: format!("自動起動の設定を変更できませんでした: {message}"),
        });
    }
    notices
}

const CONFLICT_GUIDE: &str =
    "ほかのアプリが使用している可能性があります。トレイの[設定]から別のキーを割り当ててください。";
const RESERVED_GUIDE: &str = "Windows の「設定 > アクセシビリティ > キーボード」にある「Print Screen キーを使用して画面キャプチャを開く」をオフにしてから、Snipt の設定画面で[再登録]を押してください。";

fn mode_label(mode: CaptureMode) -> &'static str {
    match mode {
        CaptureMode::Region => "範囲キャプチャ",
        CaptureMode::Window => "ウィンドウキャプチャ",
        CaptureMode::Fullscreen => "全画面キャプチャ",
    }
}

/// One notice for every hotkey that failed to register, or `None` when all succeeded.
fn hotkey_failure_notice(statuses: &[HotkeyStatus]) -> Option<Notice> {
    let failed: Vec<&HotkeyStatus> = statuses.iter().filter(|s| !s.registered).collect();
    if failed.is_empty() {
        return None;
    }
    let keys: Vec<String> = failed
        .iter()
        .map(|s| {
            format!(
                "{}({})",
                mode_label(s.mode),
                accelerator::display(&s.accelerator)
            )
        })
        .collect();
    let mut body = format!("{}のホットキーを登録できませんでした。", keys.join("、"));
    let has = |kind| failed.iter().any(|s| s.failure == Some(kind));
    if has(HotkeyFailureKind::PrintScreenReservedByWindows) {
        body.push_str(RESERVED_GUIDE);
    }
    if has(HotkeyFailureKind::Conflict) {
        body.push_str(CONFLICT_GUIDE);
    }
    Some(Notice {
        level: NoticeLevel::Warning,
        title: NOTICE_TITLE.to_owned(),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::Accelerator;

    fn status(mode: CaptureMode, key: &str, failure: Option<HotkeyFailureKind>) -> HotkeyStatus {
        HotkeyStatus {
            mode,
            accelerator: Accelerator::from_normalized(key.to_owned()),
            registered: failure.is_none(),
            failure,
        }
    }

    fn all_registered() -> Vec<HotkeyStatus> {
        vec![
            status(CaptureMode::Region, "PrintScreen", None),
            status(CaptureMode::Window, "Ctrl+PrintScreen", None),
            status(CaptureMode::Fullscreen, "Shift+PrintScreen", None),
        ]
    }

    #[test]
    fn a_normal_start_shows_nothing() {
        assert!(startup_notices(Some(&LoadOutcome::Loaded), &all_registered(), None).is_empty());
        assert!(startup_notices(Some(&LoadOutcome::FirstRun), &all_registered(), None).is_empty());
    }

    #[test]
    fn failed_hotkeys_are_merged_into_one_notice() {
        let statuses = vec![
            status(
                CaptureMode::Region,
                "PrintScreen",
                Some(HotkeyFailureKind::PrintScreenReservedByWindows),
            ),
            status(
                CaptureMode::Window,
                "Ctrl+PrintScreen",
                Some(HotkeyFailureKind::Conflict),
            ),
            status(CaptureMode::Fullscreen, "Shift+PrintScreen", None),
        ];

        let notices = startup_notices(Some(&LoadOutcome::Loaded), &statuses, None);

        assert_eq!(notices.len(), 1);
        let body = &notices[0].body;
        assert_eq!(notices[0].level, NoticeLevel::Warning);
        assert!(body.starts_with(
            "範囲キャプチャ(PrintScreen)、ウィンドウキャプチャ(Ctrl + PrintScreen)のホットキーを登録できませんでした。"
        ), "{body}");
        assert!(body.contains(RESERVED_GUIDE));
        assert!(body.contains(CONFLICT_GUIDE));
        assert!(!body.contains("全画面"));
    }

    #[test]
    fn a_conflict_alone_gets_only_the_conflict_guide() {
        let mut statuses = all_registered();
        statuses[1] = status(
            CaptureMode::Window,
            "Ctrl+PrintScreen",
            Some(HotkeyFailureKind::Conflict),
        );
        let body = hotkey_failure_notice(&statuses).unwrap().body;
        assert!(body.contains(CONFLICT_GUIDE));
        assert!(!body.contains(RESERVED_GUIDE));
    }

    #[test]
    fn recovery_and_autostart_failures_are_reported() {
        let outcome = LoadOutcome::RecoveredFromCorruption {
            backup: Some(PathBuf::from("C:/cfg/settings.corrupt-1700000000.json")),
        };
        let notices = startup_notices(
            Some(&outcome),
            &all_registered(),
            Some("access denied".to_owned()),
        );

        assert_eq!(notices.len(), 2);
        assert!(notices[0].body.contains("初期設定に戻しました"));
        assert!(notices[0].body.contains("settings.corrupt-1700000000.json"));
        assert!(notices[1].body.contains("自動起動"));
        assert!(notices[1].body.contains("access denied"));
    }

    #[test]
    fn stage_one_manages_the_shared_services() {
        let root = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_app();
        setup_at(
            app.handle(),
            root.path().join("config").join("settings.json"),
            &root.path().join("Pictures"),
        );

        let store = app.state::<Arc<SettingsStore>>();
        assert_eq!(
            store.get().save_dir,
            root.path().join("Pictures").join("Snipt")
        );
        assert!(app.try_state::<Arc<dyn Notifier>>().is_some());
        assert!(app.try_state::<Arc<BeforeExitHooks>>().is_some());
        assert!(app.try_state::<Arc<HotkeyManager>>().is_some());
        assert!(app.try_state::<Arc<Lifecycle>>().is_some());
        assert!(app.try_state::<Arc<SettingsService>>().is_some());
        let pending = app.state::<PendingLoadOutcome>();
        assert_eq!(*pending.0.lock().unwrap(), Some(LoadOutcome::FirstRun));
    }

    #[test]
    fn a_second_instance_before_setup_does_not_panic() {
        let app = tauri::test::mock_app();
        on_second_instance(app.handle(), &["snipt.exe".to_owned()]);
    }
}
