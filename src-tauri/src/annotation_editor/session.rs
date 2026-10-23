//! `EditorState`: per-window state of the open editors. Created on first use, removed when the
//! window is destroyed (which also wakes everyone waiting for it to close).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use super::error::EditorError;
use super::export::ExportFormat;
use crate::shared::CaptureId;

/// Where `editor_write_export` may write, chosen in the save dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportTarget {
    pub path: PathBuf,
    pub format: ExportFormat,
}

#[derive(Default)]
struct WindowSession {
    export_targets: HashMap<String, ExportTarget>,
    close_waiters: Vec<Sender<()>>,
}

#[derive(Default)]
pub struct EditorState {
    sessions: Mutex<HashMap<CaptureId, WindowSession>>,
    /// Flush requests sent on quit, by request id: the editor asked and where to answer.
    flushes: Mutex<HashMap<String, (CaptureId, Sender<bool>)>>,
}

impl EditorState {
    fn with<T>(&self, id: &CaptureId, f: impl FnOnce(&mut WindowSession) -> T) -> T {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        f(sessions.entry(id.clone()).or_default())
    }

    /// Remembers `target` for this editor and returns the token that names it.
    pub fn add_export_target(&self, id: &CaptureId, target: ExportTarget) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        self.with(id, |session| {
            session.export_targets.insert(token.clone(), target)
        });
        token
    }

    /// The target of `token`, only for the editor it was chosen in.
    pub fn export_target(&self, id: &CaptureId, token: &str) -> Result<ExportTarget, EditorError> {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions
            .get(id)
            .and_then(|session| session.export_targets.get(token))
            .cloned()
            .ok_or(EditorError::UnknownToken)
    }

    /// Receives `()` (or a disconnect) once the editor's session is removed.
    pub fn wait_for_close(&self, id: &CaptureId) -> Receiver<()> {
        let (tx, rx) = mpsc::channel();
        self.with(id, |session| session.close_waiters.push(tx));
        rx
    }

    /// Registers a flush request to `id`'s editor; the receiver gets its answer.
    pub fn expect_flush(&self, id: &CaptureId) -> (String, Receiver<bool>) {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel();
        self.flushes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(request_id.clone(), (id.clone(), tx));
        (request_id, rx)
    }

    /// Delivers an editor's answer; only the editor that was asked can answer. Returns whether
    /// the request was known.
    pub fn answer_flush(&self, id: &CaptureId, request_id: &str, saved: bool) -> bool {
        let mut flushes = self.flushes.lock().unwrap_or_else(|e| e.into_inner());
        match flushes.get(request_id) {
            Some((asked, _)) if asked == id => {
                let (_, answer) = flushes.remove(request_id).expect("just found");
                let _ = answer.send(saved);
                true
            }
            _ => false,
        }
    }

    pub fn forget_flushes(&self, request_ids: &[String]) {
        let mut flushes = self.flushes.lock().unwrap_or_else(|e| e.into_inner());
        for request_id in request_ids {
            flushes.remove(request_id);
        }
    }

    /// Forgets the editor and wakes its close waiters.
    pub fn remove(&self, id: &CaptureId) {
        let session = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        for waiter in session.into_iter().flat_map(|s| s.close_waiters) {
            let _ = waiter.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn id(s: &str) -> CaptureId {
        CaptureId::parse(s).unwrap()
    }

    fn target(name: &str) -> ExportTarget {
        ExportTarget {
            path: PathBuf::from(name),
            format: ExportFormat::Png,
        }
    }

    #[test]
    fn tokens_name_targets_of_their_own_editor() {
        let state = EditorState::default();
        let a = id("20261022_174150");
        let b = id("20261022_174151");
        let token = state.add_export_target(&a, target("a.png"));
        let other = state.add_export_target(&a, target("b.png"));
        assert_ne!(token, other);

        assert_eq!(state.export_target(&a, &token), Ok(target("a.png")));
        assert_eq!(state.export_target(&a, &other), Ok(target("b.png")));
        assert_eq!(
            state.export_target(&b, &token),
            Err(EditorError::UnknownToken)
        );
        assert_eq!(
            state.export_target(&a, "made-up"),
            Err(EditorError::UnknownToken)
        );
    }

    #[test]
    fn removing_wakes_the_waiters_and_forgets_the_tokens() {
        let state = EditorState::default();
        let a = id("20261022_174150");
        let token = state.add_export_target(&a, target("a.png"));
        let first = state.wait_for_close(&a);
        let second = state.wait_for_close(&a);
        assert!(first.recv_timeout(Duration::from_millis(10)).is_err());

        state.remove(&a);

        assert_eq!(first.recv_timeout(Duration::from_secs(1)), Ok(()));
        assert_eq!(second.recv_timeout(Duration::from_secs(1)), Ok(()));
        assert_eq!(
            state.export_target(&a, &token),
            Err(EditorError::UnknownToken)
        );
        state.remove(&a);
    }

    #[test]
    fn only_the_asked_editor_answers_a_flush() {
        let state = EditorState::default();
        let a = id("20261022_174150");
        let b = id("20261022_174151");
        let (request, answer) = state.expect_flush(&a);

        assert!(!state.answer_flush(&b, &request, true));
        assert!(!state.answer_flush(&a, "made-up", true));
        assert!(answer.recv_timeout(Duration::from_millis(10)).is_err());

        assert!(state.answer_flush(&a, &request, false));
        assert_eq!(answer.recv_timeout(Duration::from_secs(1)), Ok(false));
        assert!(!state.answer_flush(&a, &request, true), "answered once");
    }

    #[test]
    fn forgotten_flushes_cannot_be_answered() {
        let state = EditorState::default();
        let a = id("20261022_174150");
        let (request, _answer) = state.expect_flush(&a);
        state.forget_flushes(std::slice::from_ref(&request));
        assert!(!state.answer_flush(&a, &request, true));
    }
}
