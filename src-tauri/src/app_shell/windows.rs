//! `WindowManager`: opens or brings forward the `settings` and `history` windows.

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

struct WindowSpec {
    label: &'static str,
    title: &'static str,
    width: f64,
    height: f64,
    resizable: bool,
}

const SETTINGS: WindowSpec = WindowSpec {
    label: "settings",
    title: "Snipt 設定",
    width: 560.0,
    height: 640.0,
    resizable: false,
};

const HISTORY: WindowSpec = WindowSpec {
    label: "history",
    title: "Snipt 履歴",
    width: 960.0,
    height: 640.0,
    resizable: true,
};

/// Closing either window destroys it (Tauri's default); the app keeps running in the tray
/// because `Lifecycle::should_prevent_exit` stops the implicit exit.
pub struct WindowManager<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> WindowManager<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    pub fn show_settings(&self) -> tauri::Result<()> {
        self.show(&SETTINGS)
    }

    pub fn show_history(&self) -> tauri::Result<()> {
        self.show(&HISTORY)
    }

    fn show(&self, spec: &WindowSpec) -> tauri::Result<()> {
        if let Some(window) = self.app.get_webview_window(spec.label) {
            window.unminimize()?;
            window.show()?;
            return window.set_focus();
        }
        WebviewWindowBuilder::new(&self.app, spec.label, WebviewUrl::App("index.html".into()))
            .title(spec.title)
            .inner_size(spec.width, spec.height)
            .resizable(spec.resizable)
            .center()
            .focused(true)
            .build()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
        let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
        labels.sort();
        labels
    }

    #[test]
    fn opening_settings_twice_keeps_one_window() {
        let app = tauri::test::mock_app();
        let windows = WindowManager::new(app.handle().clone());

        windows.show_settings().unwrap();
        windows.show_settings().unwrap();

        assert_eq!(labels(app.handle()), ["settings"]);
    }

    #[test]
    fn opening_history_creates_the_history_window() {
        let app = tauri::test::mock_app();
        let windows = WindowManager::new(app.handle().clone());

        windows.show_history().unwrap();
        windows.show_settings().unwrap();

        assert_eq!(labels(app.handle()), ["history", "settings"]);
    }
}
