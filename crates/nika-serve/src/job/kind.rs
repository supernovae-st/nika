// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident's own event vocabulary (#1471 · ADR-130's rule): the
//! `kind` a durable job event carries, typed ONCE. `nika-event`'s
//! `EventKind` is the RUN's vocabulary; this is the resident's. Every
//! producer spells a kind through this enum, every reader parses it here,
//! and the `OpenAPI` `JobEvent.kind` enumerates [`JobEventKind::ALL`].
//!
//! The words are frozen on the wire (`execution.<word>`): a v3 store
//! written by an earlier resident reads back through [`JobEventKind::parse`]
//! unchanged.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One resident event kind — the `kind` field of a job event payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum JobEventKind {
    /// The job was admitted and waits for an executor.
    #[serde(rename = "execution.queued")]
    Queued,
    /// The resident claimed the job and readmitted its snapshot.
    #[serde(rename = "execution.started")]
    Started,
    /// A scheduled admission claimed its slot before its run started.
    #[serde(rename = "execution.prepared")]
    Prepared,
    /// A running job of a previous resident incarnation went back to the
    /// queue at restart (its restart schedule was durable).
    #[serde(rename = "execution.requeued")]
    Requeued,
    /// The run settled (succeeded · failed · paused): the settlement rides
    /// this event whole (ADR-128).
    #[serde(rename = "execution.settled")]
    Settled,
    /// The run was cancelled — before it started, or racing its settlement.
    #[serde(rename = "execution.cancelled")]
    Cancelled,
    /// Execution ownership was lost: settlement unknown (ADR-129).
    #[serde(rename = "execution.interrupted")]
    Interrupted,
    /// The queued world could not be readmitted; the job failed at admission.
    #[serde(rename = "execution.refused")]
    Refused,
    /// A prepared scheduled admission lost its ARM claim before running.
    #[serde(rename = "execution.aborted_before_claim")]
    AbortedBeforeClaim,
}

impl JobEventKind {
    /// Every kind, in lifecycle order — the `OpenAPI` enumeration.
    pub const ALL: [Self; 9] = [
        Self::Queued,
        Self::Started,
        Self::Prepared,
        Self::Requeued,
        Self::Settled,
        Self::Cancelled,
        Self::Interrupted,
        Self::Refused,
        Self::AbortedBeforeClaim,
    ];

    /// The wire word (`execution.<word>`), equal to the serde form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "execution.queued",
            Self::Started => "execution.started",
            Self::Prepared => "execution.prepared",
            Self::Requeued => "execution.requeued",
            Self::Settled => "execution.settled",
            Self::Cancelled => "execution.cancelled",
            Self::Interrupted => "execution.interrupted",
            Self::Refused => "execution.refused",
            Self::AbortedBeforeClaim => "execution.aborted_before_claim",
        }
    }

    /// The kind a wire word names, or `None` for a word this resident does
    /// not carry (an approval event · a legacy bare word · a future kind).
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == word)
    }

    /// The kind an event payload carries, when it carries one of ours.
    #[must_use]
    pub fn of(payload: &Value) -> Option<Self> {
        payload
            .get("kind")
            .and_then(Value::as_str)
            .and_then(Self::parse)
    }

    /// Whether the payload is this kind.
    #[must_use]
    pub fn is(self, payload: &Value) -> bool {
        Self::of(payload) == Some(self)
    }
}

impl fmt::Display for JobEventKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_serde_word_and_the_spelled_word_are_one() {
        for kind in JobEventKind::ALL {
            let wire = serde_json::to_value(kind).expect("serializes");
            assert_eq!(wire, json!(kind.as_str()), "{kind:?}");
            let back: JobEventKind = serde_json::from_value(wire).expect("deserializes");
            assert_eq!(back, kind);
            assert_eq!(JobEventKind::parse(kind.as_str()), Some(kind));
            assert_eq!(kind.to_string(), kind.as_str());
            assert!(kind.as_str().starts_with("execution."));
        }
    }

    #[test]
    fn a_foreign_word_is_not_a_resident_kind() {
        assert_eq!(
            JobEventKind::parse("interrupted"),
            None,
            "the legacy bare word"
        );
        assert_eq!(JobEventKind::parse("approval_decided"), None);
        assert_eq!(JobEventKind::parse("execution.finished"), None);
        assert_eq!(JobEventKind::of(&json!({"kind": 42})), None);
        assert_eq!(JobEventKind::of(&json!(null)), None);
        assert_eq!(
            JobEventKind::of(&json!({"kind": "execution.settled", "status": "paused"})),
            Some(JobEventKind::Settled)
        );
        assert!(JobEventKind::Settled.is(&json!({"kind": "execution.settled"})));
        assert!(!JobEventKind::Cancelled.is(&json!({"kind": "execution.settled"})));
    }

    #[test]
    fn the_json_macro_spells_a_kind_as_its_wire_word() {
        let payload = json!({"kind": JobEventKind::Refused, "status": "failed"});
        assert_eq!(payload["kind"], "execution.refused");
        assert_eq!(JobEventKind::of(&payload), Some(JobEventKind::Refused));
    }
}
