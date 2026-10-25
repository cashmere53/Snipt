//! Capture and commit errors (screen-capture 3.1).

use serde::Serialize;

use super::topology::MonitorIndex;

/// Why a capture could not start or finish.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    /// Monitors were added, removed, moved or rescaled during the capture.
    #[error("the monitor layout changed")]
    TopologyChanged,
    /// The OS error as text, so that this type stays OS independent.
    #[error("could not freeze monitor {}: {reason}", .monitor.0)]
    GrabFailed {
        monitor: MonitorIndex,
        reason: String,
    },
    #[error("could not capture the window: {reason}")]
    WindowCaptureFailed { reason: String },
    #[error("the overlay is unavailable: {reason}")]
    OverlayUnavailable { reason: String },
    /// A request for a session that has ended.
    #[error("the capture session has ended")]
    StaleSession,
    #[error("{0}")]
    Internal(String),
}

/// Why a selection was not taken. Serialized as `{ "kind": "tooSmall" }` and so on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommitError {
    /// Narrower or lower than one pixel; the selection goes on.
    #[error("the selection is smaller than one pixel")]
    TooSmall,
    #[error("no window is highlighted")]
    NoWindowHighlighted,
    #[error("the capture session has ended")]
    StaleSession,
    #[error("the monitor layout changed")]
    TopologyChanged,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn commit_errors_serialize_as_a_kind() {
        let kinds: Vec<_> = [
            CommitError::TooSmall,
            CommitError::NoWindowHighlighted,
            CommitError::StaleSession,
            CommitError::TopologyChanged,
        ]
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
        assert_eq!(
            kinds,
            [
                json!({ "kind": "tooSmall" }),
                json!({ "kind": "noWindowHighlighted" }),
                json!({ "kind": "staleSession" }),
                json!({ "kind": "topologyChanged" }),
            ]
        );
    }

    #[test]
    fn capture_errors_name_the_monitor() {
        let error = CaptureError::GrabFailed {
            monitor: MonitorIndex(1),
            reason: "BitBlt failed".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "could not freeze monitor 1: BitBlt failed"
        );
    }
}
