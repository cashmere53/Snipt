//! `ToastNotifier`: the `Notifier` implementation that shows Windows toast notifications.

use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::shared::{Notice, NoticeLevel, Notifier};

type Show = Box<dyn Fn(&Notice) -> Result<(), String> + Send + Sync>;

/// Shows each notice as an OS toast. Guidance must fit in the body text because clicking the
/// toast does nothing.
pub struct ToastNotifier {
    show: Show,
}

impl ToastNotifier {
    /// Shows toasts through the notification plugin.
    pub fn new<R: Runtime>(app: AppHandle<R>) -> Self {
        Self::with_show(Box::new(move |notice| {
            app.notification()
                .builder()
                .title(&notice.title)
                .body(&notice.body)
                .show()
                .map_err(|e| e.to_string())
        }))
    }

    fn with_show(show: Show) -> Self {
        Self { show }
    }
}

impl Notifier for ToastNotifier {
    fn notify(&self, notice: Notice) {
        let level = match notice.level {
            NoticeLevel::Info => log::Level::Info,
            NoticeLevel::Warning => log::Level::Warn,
            NoticeLevel::Error => log::Level::Error,
        };
        log::log!(level, "notice: {}: {}", notice.title, notice.body);
        if let Err(e) = (self.show)(&notice) {
            log::warn!("notice: could not show toast: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn notice() -> Notice {
        Notice {
            level: NoticeLevel::Warning,
            title: "Snipt".to_owned(),
            body: "ホットキーを登録できませんでした".to_owned(),
        }
    }

    #[test]
    fn passes_title_and_body_to_the_toast() {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let sink = shown.clone();
        let notifier = ToastNotifier::with_show(Box::new(move |n| {
            sink.lock().unwrap().push(n.clone());
            Ok(())
        }));
        notifier.notify(notice());
        assert_eq!(*shown.lock().unwrap(), vec![notice()]);
    }

    #[test]
    fn display_failure_returns_normally() {
        let notifier = ToastNotifier::with_show(Box::new(|_| Err("no toast".to_owned())));
        notifier.notify(notice());
    }

    #[test]
    fn can_be_managed_as_dyn_notifier() {
        let app = tauri::test::mock_app();
        let notifier: Arc<dyn Notifier> = Arc::new(ToastNotifier::new(app.handle().clone()));
        assert!(tauri::Manager::manage(&app, notifier));
    }
}
