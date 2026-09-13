// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Journal delivery is independent of execution. Only bounded, path-free
//! evidence metadata crosses the resident's public boundary.

use serde::{Deserialize, Serialize};

/// Why the resident's journal mirror stopped recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JournalFailure {
    /// Opening, writing or syncing the journal failed.
    WriteFailed,
    /// A journal record could not be admitted within the writer's bounds.
    RecordRefused,
}

/// Evidence lost by an observation leg, separate from its execution status.
/// Absence means no loss was reported, not proof that a journal exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum JournalEvidence {
    /// The journal mirror failed; the job's execution result still stands.
    #[non_exhaustive]
    MirrorLost {
        /// Coarse first-error classification, never OS text or a local path.
        reason: JournalFailure,
    },
}

impl JournalEvidence {
    /// Record the first mirror failure without asserting an execution failure.
    #[must_use]
    pub const fn mirror_lost(reason: JournalFailure) -> Self {
        Self::MirrorLost { reason }
    }

    pub(crate) fn from_error(error: &std::io::Error) -> Self {
        Self::mirror_lost(if error.kind() == std::io::ErrorKind::InvalidData {
            JournalFailure::RecordRefused
        } else {
            JournalFailure::WriteFailed
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_error_class_never_carries_os_text_or_paths() {
        for (kind, reason) in [
            (std::io::ErrorKind::PermissionDenied, "write_failed"),
            (std::io::ErrorKind::InvalidData, "record_refused"),
        ] {
            let error = std::io::Error::new(kind, "/private/SENTINEL: token=SENTINEL");
            let value = serde_json::to_value(JournalEvidence::from_error(&error)).expect("json");
            assert_eq!(
                value,
                serde_json::json!({"status": "mirror_lost", "reason": reason})
            );
        }
    }

    #[test]
    fn evidence_refuses_unknown_status_reason_and_payload_fields() {
        for value in [
            serde_json::json!({"status": "complete", "reason": "write_failed"}),
            serde_json::json!({"status": "mirror_lost", "reason": "SENTINEL"}),
            serde_json::json!({"status": "mirror_lost", "reason": "write_failed", "path": "SENTINEL"}),
        ] {
            assert!(serde_json::from_value::<JournalEvidence>(value).is_err());
        }
    }
}
