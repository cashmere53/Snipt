//! Startup, commit and quit across app-shell's services, on Tauri's mock runtime with fakes for
//! the OS parts (shortcuts, autostart, notifications, the exit confirmation and the tray).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::test::{mock_app, MockRuntime};
use tauri::{App, Listener, Manager};
use tauri_plugin_global_shortcut::{ShortcutEvent, ShortcutState};

use super::autostart::{AutostartBackend, AutostartSync};
use super::hotkeys::accelerator;
use super::hotkeys::backend::fake::FakeBackend;
use super::hotkeys::HotkeyManager;
use super::lifecycle::{ExitActions, ExitChoice, ExitConfirm, Lifecycle, QuitOutcome};
use super::service::{SettingsService, SETTINGS_CHANGED_EVENT};
use super::startup::{self, Platform};
use super::tray::{TrayHost, TrayTargets};
use crate::shared::{
    BeforeExitHooks, CaptureMode, CaptureRequester, ExitCheck, Notice, Notifier, Settings,
    SettingsStore,
};

type Log = Arc<Mutex<Vec<String>>>;

fn push(log: &Log, entry: impl Into<String>) {
    log.lock().unwrap().push(entry.into());
}

struct RecordingNotifier(Arc<Mutex<Vec<Notice>>>);

impl Notifier for RecordingNotifier {
    fn notify(&self, notice: Notice) {
        self.0.lock().unwrap().push(notice);
    }
}

/// Records the tooltip given at creation as `"create: …"` and later ones as `"tooltip: …"`.
struct RecordingTray(Log);

impl TrayHost for RecordingTray {
    fn create(&self, _targets: Arc<dyn TrayTargets>, tooltip: &str) -> tauri::Result<()> {
        push(&self.0, format!("create: {tooltip}"));
        Ok(())
    }

    fn set_tooltip(&self, text: &str) {
        push(&self.0, format!("tooltip: {text}"));
    }
}

struct FakeAutostart(Arc<Mutex<bool>>);

impl AutostartBackend for FakeAutostart {
    fn is_enabled(&self) -> Result<bool, String> {
        Ok(*self.0.lock().unwrap())
    }

    fn enable(&self) -> Result<(), String> {
        *self.0.lock().unwrap() = true;
        Ok(())
    }

    fn disable(&self) -> Result<(), String> {
        *self.0.lock().unwrap() = false;
        Ok(())
    }
}

struct DiscardOnExit;

impl ExitConfirm for DiscardOnExit {
    fn ask(&self, _unsaved: &[String]) -> ExitChoice {
        ExitChoice::Discard
    }
}

struct RecordingExit {
    hotkeys: Arc<HotkeyManager>,
    steps: Log,
}

impl ExitActions for RecordingExit {
    fn release_hotkeys(&self) {
        self.hotkeys.release_all();
        push(&self.steps, "release");
    }

    fn destroy_windows(&self) {
        push(&self.steps, "destroy");
    }

    fn exit(&self) {
        push(&self.steps, "exit");
    }
}

#[derive(Default)]
struct RecordingRequester(Mutex<Vec<CaptureMode>>);

impl CaptureRequester for RecordingRequester {
    fn request_capture(&self, mode: CaptureMode) {
        self.0.lock().unwrap().push(mode);
    }
}

/// A mock app after app-shell's stage 1, with handles to every fake.
struct Harness {
    app: App<MockRuntime>,
    shortcuts: Arc<FakeBackend>,
    autostart: Arc<Mutex<bool>>,
    notices: Arc<Mutex<Vec<Notice>>>,
    /// Tray calls, subscriber calls, emitted events and exit steps, in the order they happened.
    log: Log,
    _root: tempfile::TempDir,
}

impl Harness {
    /// `prepare` runs before stage 1, e.g. to write a settings file or make a key unavailable.
    fn stage_one(prepare: impl FnOnce(&Path, &FakeBackend)) -> Self {
        let root = tempfile::tempdir().unwrap();
        let settings_path = root.path().join("config").join("settings.json");
        let shortcuts = Arc::new(FakeBackend::default());
        prepare(&settings_path, &shortcuts);

        let autostart = Arc::new(Mutex::new(false));
        let notices = Arc::new(Mutex::new(Vec::new()));
        let log: Log = Arc::default();
        let exit_log = log.clone();
        let platform = Platform {
            shortcuts: Box::new(shortcuts.clone()),
            // As if Windows reserved plain PrintScreen for the Snipping Tool.
            reservation: Box::new(|| Some(true)),
            autostart: AutostartSync::with_os_changes(
                Box::new(FakeAutostart(autostart.clone())),
                true,
            ),
            notifier: Arc::new(RecordingNotifier(notices.clone())),
            exit_confirm: Box::new(DiscardOnExit),
            exit_actions: Box::new(move |hotkeys| {
                Box::new(RecordingExit {
                    hotkeys,
                    steps: exit_log,
                }) as Box<dyn ExitActions>
            }),
            tray: Arc::new(RecordingTray(log.clone())),
        };

        let app = mock_app();
        startup::setup_at(
            app.handle(),
            settings_path,
            &root.path().join("Pictures"),
            platform,
        );
        Self {
            app,
            shortcuts,
            autostart,
            notices,
            log,
            _root: root,
        }
    }

    /// Downstream setup, then stage 2.
    fn start(&mut self) {
        crate::setup_downstream(&mut self.app).expect("downstream setup failed");
        startup::start(&mut self.app).expect("stage 2 failed");
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn notices(&self) -> Vec<Notice> {
        self.notices.lock().unwrap().clone()
    }
}

#[test]
fn starts_without_capture_requester_and_reports_a_reset_once() {
    let mut harness = Harness::stage_one(|settings_path, _| {
        std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        std::fs::write(settings_path, b"{ not json").unwrap();
    });
    assert!(harness
        .app
        .try_state::<Arc<dyn CaptureRequester>>()
        .is_none());

    harness.start();

    let notices = harness.notices();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].body.contains("初期設定に戻しました"));
    assert_eq!(
        harness.shortcuts.registered(),
        vec!["Ctrl+PrintScreen", "PrintScreen", "Shift+PrintScreen"]
    );
    assert_eq!(
        harness.log(),
        vec!["create: Snipt - 範囲キャプチャ: PrintScreen"]
    );
    assert!(*harness.autostart.lock().unwrap(), "autostart is synced on");

    // Requests without a requester are only logged.
    super::tray::perform(
        &super::tray::AppTrayTargets {
            dispatch: super::dispatch::CaptureDispatch::new(harness.app.handle().clone()),
            windows: super::windows::WindowManager::new(harness.app.handle().clone()),
            lifecycle: harness.app.state::<Arc<Lifecycle>>().inner().clone(),
        },
        super::tray::TrayAction::Capture(CaptureMode::Region),
    );
}

#[test]
fn failed_hotkeys_at_startup_give_one_notice() {
    let mut harness = Harness::stage_one(|_, shortcuts| {
        shortcuts.fail_on("PrintScreen");
        shortcuts.fail_on("Ctrl+PrintScreen");
    });

    harness.start();

    let notices = harness.notices();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].body.contains("範囲キャプチャ"));
    assert!(notices[0].body.contains("ウィンドウキャプチャ"));
    assert!(notices[0].body.contains("アクセシビリティ"));
    assert!(notices[0].body.contains("ほかのアプリ"));
    assert_eq!(
        harness.log(),
        vec!["create: Snipt - 範囲キャプチャ: 未登録"]
    );
}

#[test]
fn a_commit_reaches_subscribers_the_tooltip_and_the_event_once_in_order() {
    let mut harness = Harness::stage_one(|_, _| {});
    // A downstream spec subscribes in its own setup, before stage 2.
    let store = harness.app.state::<Arc<SettingsStore>>().inner().clone();
    let subscriber_log = harness.log.clone();
    store.subscribe(Box::new(move |s: &Settings| {
        push(
            &subscriber_log,
            format!("subscriber: {}", s.hotkeys.region.as_str()),
        )
    }));
    harness.start();
    let event_log = harness.log.clone();
    harness
        .app
        .listen_any(SETTINGS_CHANGED_EVENT, move |event| {
            let settings: Settings = serde_json::from_str(event.payload()).unwrap();
            push(
                &event_log,
                format!("event: {}", settings.hotkeys.region.as_str()),
            );
        });

    let service = harness.app.state::<Arc<SettingsService>>().inner().clone();
    let mut next = (*service.get()).clone();
    next.hotkeys.region = accelerator::parse("Ctrl+Shift+KeyA").unwrap();
    service.update(next).unwrap();

    let shown = accelerator::display(&accelerator::parse("Ctrl+Shift+KeyA").unwrap());
    assert_eq!(
        harness.log()[1..],
        [
            "subscriber: Ctrl+Shift+KeyA".to_owned(),
            format!("tooltip: Snipt - 範囲キャプチャ: {shown}"),
            "event: Ctrl+Shift+KeyA".to_owned(),
        ]
    );
    assert!(harness
        .shortcuts
        .registered()
        .contains(&"Ctrl+Shift+KeyA".to_owned()));
}

#[test]
fn quitting_runs_the_hooks_before_releasing_every_hotkey() {
    let mut harness = Harness::stage_one(|_, _| {});
    // A downstream spec registers its hook in its own setup.
    let hook_log = harness.log.clone();
    harness.app.state::<Arc<BeforeExitHooks>>().register(
        "test",
        Box::new(move || {
            push(&hook_log, "hook");
            ExitCheck::default()
        }),
    );
    harness.start();
    assert_eq!(harness.shortcuts.registered().len(), 3);

    let lifecycle = harness.app.state::<Arc<Lifecycle>>().inner().clone();
    assert_eq!(lifecycle.quit_blocking(), QuitOutcome::Exited);

    assert_eq!(harness.log()[1..], ["hook", "release", "destroy", "exit"]);
    assert!(harness.shortcuts.registered().is_empty());
}

/// 10.2: resolving a pressed hotkey and handing it to the requester stays well under 10 ms.
#[test]
fn a_pressed_hotkey_reaches_the_requester_quickly() {
    let mut harness = Harness::stage_one(|_, _| {});
    let requester = Arc::new(RecordingRequester::default());
    let managed: Arc<dyn CaptureRequester> = requester.clone();
    harness.app.manage(managed);
    harness.start();

    let shortcut = accelerator::to_shortcut(&accelerator::parse("PrintScreen").unwrap()).unwrap();
    let event = |state| ShortcutEvent {
        id: shortcut.id(),
        state,
    };
    startup::on_shortcut(
        harness.app.handle(),
        &shortcut,
        event(ShortcutState::Released),
    );
    assert!(requester.0.lock().unwrap().is_empty(), "release is ignored");

    let started = Instant::now();
    startup::on_shortcut(
        harness.app.handle(),
        &shortcut,
        event(ShortcutState::Pressed),
    );
    let elapsed = started.elapsed();

    assert_eq!(*requester.0.lock().unwrap(), vec![CaptureMode::Region]);
    assert!(elapsed < Duration::from_millis(10), "{elapsed:?}");
}
