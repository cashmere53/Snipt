//! Exit handling: the before-exit hook registry used by downstream specs, the quit flow with
//! its unsaved-data confirmation, and the second-instance notice.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

use super::hotkeys::HotkeyManager;
use crate::shared::{Notice, NoticeLevel, Notifier};

/// What a hook could not save. Each entry is a short user-facing description, e.g.
/// `"注釈: 20261022_173512"`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitCheck {
    pub unsaved: Vec<String>,
}

type Hook = Arc<dyn Fn() -> ExitCheck + Send + Sync + 'static>;

/// Managed as `Arc<BeforeExitHooks>`. Downstream specs register in their own `setup`.
#[derive(Default)]
pub struct BeforeExitHooks {
    hooks: Mutex<Vec<(&'static str, Hook)>>,
}

impl BeforeExitHooks {
    /// `hook` runs on a worker thread when the user quits. It tries to save pending data and
    /// returns what is still unsaved. It may block but must time out on its own (about 3 s).
    /// It runs again each time the user picks "retry", so it must be idempotent.
    pub fn register(
        &self,
        name: &'static str,
        hook: Box<dyn Fn() -> ExitCheck + Send + Sync + 'static>,
    ) {
        self.hooks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((name, Arc::from(hook)));
    }

    /// Runs every hook in registration order (without holding the registry lock) and merges
    /// what remains unsaved, in the same order.
    pub(crate) fn run_all(&self) -> ExitCheck {
        let hooks: Vec<(&'static str, Hook)> =
            self.hooks.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut result = ExitCheck::default();
        for (name, hook) in hooks {
            let check = hook();
            if !check.unsaved.is_empty() {
                log::warn!("exit: {name} left {} unsaved item(s)", check.unsaved.len());
            }
            result.unsaved.extend(check.unsaved);
        }
        result
    }
}

/// The user's answer when unsaved data remains at exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitChoice {
    Retry,
    Discard,
    Cancel,
}

/// Asks what to do about unsaved data. Called on the quit worker thread, so it may block.
pub trait ExitConfirm: Send + Sync + 'static {
    fn ask(&self, unsaved: &[String]) -> ExitChoice;
}

/// The final exit steps, in the order `Lifecycle` calls them.
pub trait ExitActions: Send + Sync + 'static {
    fn release_hotkeys(&self);
    fn destroy_windows(&self);
    fn exit(&self);
}

/// What `Lifecycle::quit` ended with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitOutcome {
    Exited,
    Cancelled,
    /// Another quit was already running.
    AlreadyQuitting,
}

const SECOND_INSTANCE_TITLE: &str = "Snipt";
const SECOND_INSTANCE_BODY: &str =
    "Snipt は既に起動しています。通知領域のアイコンから操作できます。";

pub struct Lifecycle {
    hooks: Arc<BeforeExitHooks>,
    confirm: Box<dyn ExitConfirm>,
    actions: Box<dyn ExitActions>,
    notifier: Arc<dyn Notifier>,
    quitting: AtomicBool,
}

impl Lifecycle {
    pub fn new(
        hooks: Arc<BeforeExitHooks>,
        confirm: Box<dyn ExitConfirm>,
        actions: Box<dyn ExitActions>,
        notifier: Arc<dyn Notifier>,
    ) -> Self {
        Self {
            hooks,
            confirm,
            actions,
            notifier,
            quitting: AtomicBool::new(false),
        }
    }

    /// Starts quitting on a worker thread, because hooks and the confirmation block.
    /// Ignored while a quit is already running.
    pub fn quit(self: &Arc<Self>) {
        let this = self.clone();
        std::thread::Builder::new()
            .name("snipt-quit".to_owned())
            .spawn(move || {
                let outcome = this.quit_blocking();
                log::info!("exit: {outcome:?}");
            })
            .map(|_| ())
            .unwrap_or_else(|e| log::error!("exit: could not start the quit thread: {e}"));
    }

    /// Runs the hooks; while something stays unsaved, asks to retry, discard or cancel.
    /// Cancel leaves hotkeys and windows untouched.
    pub fn quit_blocking(&self) -> QuitOutcome {
        if self.quitting.swap(true, Ordering::SeqCst) {
            return QuitOutcome::AlreadyQuitting;
        }
        loop {
            let check = self.hooks.run_all();
            if check.unsaved.is_empty() {
                break;
            }
            match self.confirm.ask(&check.unsaved) {
                ExitChoice::Retry => continue,
                ExitChoice::Discard => break,
                ExitChoice::Cancel => {
                    self.quitting.store(false, Ordering::SeqCst);
                    return QuitOutcome::Cancelled;
                }
            }
        }
        self.actions.release_hotkeys();
        self.actions.destroy_windows();
        self.actions.exit();
        QuitOutcome::Exited
    }

    /// Called in the running instance when a second process starts (requirement 6.1).
    pub fn on_second_instance(&self) {
        self.notifier.notify(Notice {
            level: NoticeLevel::Info,
            title: SECOND_INSTANCE_TITLE.to_owned(),
            body: SECOND_INSTANCE_BODY.to_owned(),
        });
    }

    /// For `RunEvent::ExitRequested`: closing the last window requests an exit without a code,
    /// which must be prevented so Snipt stays in the tray. Explicit exits carry a code.
    pub fn should_prevent_exit(code: Option<i32>) -> bool {
        code.is_none()
    }
}

const RETRY: &str = "再試行";
const DISCARD: &str = "破棄して終了";
const CANCEL: &str = "キャンセル";

fn confirm_message(unsaved: &[String]) -> String {
    let items: Vec<String> = unsaved.iter().map(|item| format!("・{item}")).collect();
    format!(
        "保存できていないデータが {} 件あります。\n\n{}\n\n「{RETRY}」でもう一度保存を試します。",
        unsaved.len(),
        items.join("\n")
    )
}

fn choice_from(result: MessageDialogResult) -> ExitChoice {
    match result {
        MessageDialogResult::Yes => ExitChoice::Retry,
        MessageDialogResult::No => ExitChoice::Discard,
        MessageDialogResult::Custom(label) if label == RETRY => ExitChoice::Retry,
        MessageDialogResult::Custom(label) if label == DISCARD => ExitChoice::Discard,
        _ => ExitChoice::Cancel,
    }
}

/// Production confirmation: a native message box from the dialog plugin.
pub struct DialogExitConfirm<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> DialogExitConfirm<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> ExitConfirm for DialogExitConfirm<R> {
    fn ask(&self, unsaved: &[String]) -> ExitChoice {
        let result = self
            .app
            .dialog()
            .message(confirm_message(unsaved))
            .title("Snipt を終了")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::YesNoCancelCustom(
                RETRY.to_owned(),
                DISCARD.to_owned(),
                CANCEL.to_owned(),
            ))
            .blocking_show_with_result();
        choice_from(result)
    }
}

/// Production exit steps: release hotkeys, destroy every window on the main thread, exit.
pub struct AppExitActions<R: Runtime> {
    app: AppHandle<R>,
    hotkeys: Arc<HotkeyManager>,
}

impl<R: Runtime> AppExitActions<R> {
    pub fn new(app: AppHandle<R>, hotkeys: Arc<HotkeyManager>) -> Self {
        Self { app, hotkeys }
    }
}

impl<R: Runtime> ExitActions for AppExitActions<R> {
    fn release_hotkeys(&self) {
        self.hotkeys.release_all();
    }

    fn destroy_windows(&self) {
        let (done, wait) = mpsc::channel();
        let app = self.app.clone();
        let scheduled = self.app.run_on_main_thread(move || {
            for (label, window) in app.webview_windows() {
                if let Err(e) = window.destroy() {
                    log::warn!("exit: could not destroy {label}: {e}");
                }
            }
            let _ = done.send(());
        });
        match scheduled {
            Ok(()) => {
                let _ = wait.recv();
            }
            Err(e) => log::warn!("exit: could not reach the main thread: {e}"),
        }
    }

    fn exit(&self) {
        self.app.exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsaved(items: &[&str]) -> ExitCheck {
        ExitCheck {
            unsaved: items.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn no_hooks_means_nothing_unsaved() {
        assert_eq!(BeforeExitHooks::default().run_all(), ExitCheck::default());
    }

    #[test]
    fn merges_unsaved_items_in_registration_order() {
        let hooks = BeforeExitHooks::default();
        hooks.register(
            "capture-library",
            Box::new(|| unsaved(&["画像: 20261022_173500"])),
        );
        hooks.register("clean", Box::new(ExitCheck::default));
        hooks.register(
            "annotation-editor",
            Box::new(|| unsaved(&["注釈: 20261022_173512", "注釈: 20261022_173530"])),
        );

        assert_eq!(
            hooks.run_all(),
            unsaved(&[
                "画像: 20261022_173500",
                "注釈: 20261022_173512",
                "注釈: 20261022_173530",
            ])
        );
    }

    #[test]
    fn hooks_run_again_on_each_retry() {
        let hooks = Arc::new(BeforeExitHooks::default());
        let remaining = Arc::new(Mutex::new(2));
        let counter = remaining.clone();
        hooks.register(
            "flaky",
            Box::new(move || {
                let mut left = counter.lock().unwrap();
                *left -= 1;
                if *left > 0 {
                    unsaved(&["注釈: 1"])
                } else {
                    ExitCheck::default()
                }
            }),
        );

        assert_eq!(hooks.run_all().unsaved.len(), 1);
        assert_eq!(hooks.run_all(), ExitCheck::default());
    }

    #[derive(Default)]
    struct Recorder {
        steps: Mutex<Vec<String>>,
    }

    impl Recorder {
        fn steps(&self) -> Vec<String> {
            self.steps.lock().unwrap().clone()
        }
        fn push(&self, step: impl Into<String>) {
            self.steps.lock().unwrap().push(step.into());
        }
    }

    struct Actions(Arc<Recorder>);

    impl ExitActions for Actions {
        fn release_hotkeys(&self) {
            self.0.push("release_hotkeys");
        }
        fn destroy_windows(&self) {
            self.0.push("destroy_windows");
        }
        fn exit(&self) {
            self.0.push("exit");
        }
    }

    /// Answers with the given choices in order and records each question.
    struct Answers(Mutex<Vec<ExitChoice>>, Arc<Recorder>);

    impl ExitConfirm for Answers {
        fn ask(&self, unsaved: &[String]) -> ExitChoice {
            self.1.push(format!("ask {}", unsaved.join(",")));
            self.0.lock().unwrap().remove(0)
        }
    }

    struct Notices(Mutex<Vec<Notice>>);

    impl Notifier for Notices {
        fn notify(&self, notice: Notice) {
            self.0.lock().unwrap().push(notice);
        }
    }

    /// A hook that reports `items` as unsaved for its first `failing_runs` runs.
    fn hook(
        failing_runs: usize,
        items: &'static [&'static str],
    ) -> Box<dyn Fn() -> ExitCheck + Send + Sync> {
        let runs = Mutex::new(0);
        Box::new(move || {
            let mut runs = runs.lock().unwrap();
            *runs += 1;
            if *runs <= failing_runs {
                unsaved(items)
            } else {
                ExitCheck::default()
            }
        })
    }

    fn lifecycle(hooks: BeforeExitHooks, answers: Vec<ExitChoice>) -> (Lifecycle, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        let lifecycle = Lifecycle::new(
            Arc::new(hooks),
            Box::new(Answers(Mutex::new(answers), recorder.clone())),
            Box::new(Actions(recorder.clone())),
            Arc::new(Notices(Mutex::new(Vec::new()))),
        );
        (lifecycle, recorder)
    }

    const EXIT_STEPS: [&str; 3] = ["release_hotkeys", "destroy_windows", "exit"];

    #[test]
    fn nothing_unsaved_exits_without_asking() {
        let (lifecycle, recorder) = lifecycle(BeforeExitHooks::default(), vec![]);
        assert_eq!(lifecycle.quit_blocking(), QuitOutcome::Exited);
        assert_eq!(recorder.steps(), EXIT_STEPS);
    }

    #[test]
    fn retry_that_saves_everything_exits() {
        let hooks = BeforeExitHooks::default();
        hooks.register("editor", hook(1, &["注釈: 1"]));
        let (lifecycle, recorder) = lifecycle(hooks, vec![ExitChoice::Retry]);

        assert_eq!(lifecycle.quit_blocking(), QuitOutcome::Exited);

        let mut expected = vec!["ask 注釈: 1".to_owned()];
        expected.extend(EXIT_STEPS.map(String::from));
        assert_eq!(recorder.steps(), expected);
    }

    #[test]
    fn retry_that_still_fails_asks_again() {
        let hooks = BeforeExitHooks::default();
        hooks.register("editor", hook(2, &["注釈: 1"]));
        let (lifecycle, recorder) = lifecycle(hooks, vec![ExitChoice::Retry, ExitChoice::Discard]);

        assert_eq!(lifecycle.quit_blocking(), QuitOutcome::Exited);
        assert_eq!(recorder.steps()[..2], ["ask 注釈: 1", "ask 注釈: 1"]);
    }

    #[test]
    fn cancel_keeps_hotkeys_and_windows_and_allows_a_later_quit() {
        let hooks = BeforeExitHooks::default();
        hooks.register("editor", hook(1, &["注釈: 1"]));
        let (lifecycle, recorder) = lifecycle(hooks, vec![ExitChoice::Cancel]);

        assert_eq!(lifecycle.quit_blocking(), QuitOutcome::Cancelled);
        assert_eq!(recorder.steps(), ["ask 注釈: 1"]);

        // The hook has saved by now, so quitting again exits.
        assert_eq!(lifecycle.quit_blocking(), QuitOutcome::Exited);
    }

    #[test]
    fn a_second_quit_while_quitting_is_ignored() {
        let (lifecycle, _) = lifecycle(BeforeExitHooks::default(), vec![]);
        lifecycle.quitting.store(true, Ordering::SeqCst);
        assert_eq!(lifecycle.quit_blocking(), QuitOutcome::AlreadyQuitting);
    }

    #[test]
    fn second_instance_shows_already_running_notice() {
        let notices = Arc::new(Notices(Mutex::new(Vec::new())));
        let lifecycle = Lifecycle::new(
            Arc::new(BeforeExitHooks::default()),
            Box::new(Answers(Mutex::new(vec![]), Arc::default())),
            Box::new(Actions(Arc::default())),
            notices.clone(),
        );
        lifecycle.on_second_instance();
        let shown = notices.0.lock().unwrap();
        assert_eq!(shown.len(), 1);
        assert!(shown[0].body.starts_with("Snipt は既に起動しています。"));
    }

    #[test]
    fn only_implicit_exit_requests_are_prevented() {
        assert!(Lifecycle::should_prevent_exit(None));
        assert!(!Lifecycle::should_prevent_exit(Some(0)));
    }

    #[test]
    fn dialog_buttons_map_to_choices() {
        assert_eq!(
            choice_from(MessageDialogResult::Custom(RETRY.to_owned())),
            ExitChoice::Retry
        );
        assert_eq!(
            choice_from(MessageDialogResult::Custom(DISCARD.to_owned())),
            ExitChoice::Discard
        );
        assert_eq!(
            choice_from(MessageDialogResult::Custom(CANCEL.to_owned())),
            ExitChoice::Cancel
        );
        assert_eq!(choice_from(MessageDialogResult::Yes), ExitChoice::Retry);
        assert_eq!(choice_from(MessageDialogResult::No), ExitChoice::Discard);
        assert_eq!(choice_from(MessageDialogResult::Cancel), ExitChoice::Cancel);
    }

    #[test]
    fn confirm_message_lists_count_and_items() {
        let message = confirm_message(&["画像: a".to_owned(), "注釈: b".to_owned()]);
        assert!(message.starts_with("保存できていないデータが 2 件あります。"));
        assert!(message.contains("・画像: a\n・注釈: b"));
    }
}
