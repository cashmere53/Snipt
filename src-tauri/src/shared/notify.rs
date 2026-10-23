#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub level: NoticeLevel,
    pub title: String,
    pub body: String,
}

/// Implemented by app-shell and managed as `Arc<dyn Notifier>`; usable from every spec.
pub trait Notifier: Send + Sync + 'static {
    /// Best effort: failures are logged, never panic.
    fn notify(&self, notice: Notice);
}
