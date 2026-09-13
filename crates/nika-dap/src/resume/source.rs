// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resume's SOURCE judgment (#1586) — is the workflow this resume
//! runs the workflow the journal recorded? Two questions, two voices:
//!
//! - the **`nika:` id** · a different id is a FOREIGN journal, refused
//!   naming both workflows and both content hashes — a journal continues
//!   the workflow that wrote it, never another (the lab measured « file
//!   CHANGED » on exactly this case: the operator went hunting an edit
//!   that never happened while every task ran live);
//! - the **bytes** · the same id with changed content is a CHANGE, said
//!   as a notice: the current file stays the source of truth (ADR-099 ·
//!   an edit re-runs, it never serves a stale output).
//!
//! Descended from `resume.rs` at its 1500-line wall (2026-09-13); the
//! comparator [`source_drifted`] keeps its `resume::` path for the replay
//! session (one comparator, two callers).

use nika_event::{Event, EventKind};

use super::{short, str_field};

/// The boot-manifest field naming the workflow the run executed (the
/// `nika:` id — the runtime's `workflow_started` frame).
const WORKFLOW_FIELD: &str = "workflow";

/// Whether this resume runs the workflow the journal recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceVerdict {
    /// The journal names no content hash (an older engine wrote it) and
    /// no id contradicts the current one: no claim, never a guess.
    Unbound,
    /// The same `nika:` id and the same content.
    Same,
    /// The same `nika:` id, different content — the file changed since
    /// the journal recorded it (the current bytes are what runs).
    Changed,
    /// Another `nika:` id — a FOREIGN journal, refused with the teaching
    /// (both names · both content hashes · the two honest ways out).
    Foreign(String),
}

/// The recorded workflow id — the `workflow_started` frame's `workflow`
/// field (`None` when the journal never recorded one · torn at birth).
#[must_use]
pub fn trace_workflow(events: &[Event]) -> Option<&str> {
    let started = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::WorkflowStarted))?;
    str_field(started, WORKFLOW_FIELD)
}

/// Judge the current workflow (its `nika:` id · its bytes) against the
/// journal's boot manifest. The id is judged FIRST: a different id is
/// foreign whatever the bytes say. An absent id on either side is no
/// claim on the id (the bytes still speak); an absent recorded hash is
/// no claim on the bytes.
#[must_use]
pub fn judge_source(events: &[Event], current_id: Option<&str>, yaml: &str) -> SourceVerdict {
    if let (Some(recorded), Some(current)) = (trace_workflow(events), current_id)
        && recorded != current
    {
        let recorded_sha = events
            .iter()
            .find(|e| matches!(e.kind, EventKind::WorkflowStarted))
            .and_then(|e| str_field(e, "workflow_sha256"))
            .map_or_else(|| "unrecorded".to_owned(), short);
        let current_sha = short(&nika_event::source_id::sha256_hex(yaml.as_bytes()));
        return SourceVerdict::Foreign(format!(
            "journal is workflow `{recorded}` (sha256 {recorded_sha}…) · you asked to run \
             `{current}` (sha256 {current_sha}…) — a journal continues the workflow that \
             wrote it, never another: resume it with `{recorded}`, or run `{current}` \
             afresh (without --resume)"
        ));
    }
    match source_drifted(yaml, events) {
        None => SourceVerdict::Unbound,
        Some(false) => SourceVerdict::Same,
        Some(true) => SourceVerdict::Changed,
    }
}

/// Whether the CURRENT source differs IN CONTENT from the bytes the
/// recorded run executed — `None` when the trace predates
/// `workflow_sha256` (no claim, never a guess).
///
/// Content, not bytes: an editor re-encoding CRLF↔LF or adding a BOM
/// moved nothing an author would call a change, and the 0.96.0 review
/// proved a raw compare cries wolf on exactly that. Raw match first,
/// then the LF normal forms (against the recorded raw for LF-recorded
/// files, against `workflow_sha256_lf` for CRLF-recorded ones) — only a
/// real content change survives all three.
///
/// ONE comparator, two callers: the replay session's #210 identity
/// check (a drifted file moves breakpoint lines) and the resume path,
/// which owes the operator a word when the file changed under a paused
/// run — the envelope `model:` can be edited between the pause and the
/// resume, and the seat swaps with it (measured 2026-08-03).
#[must_use]
pub fn source_drifted(yaml: &str, events: &[Event]) -> Option<bool> {
    let started = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::WorkflowStarted))?;
    let recorded = str_field(started, "workflow_sha256")?;
    if nika_event::source_id::sha256_hex(yaml.as_bytes()) == recorded {
        return Some(false);
    }
    let lf_sha =
        nika_event::source_id::sha256_hex(nika_event::source_id::lf_normal_form(yaml).as_bytes());
    if lf_sha == recorded || str_field(started, "workflow_sha256_lf") == Some(lf_sha.as_str()) {
        return Some(false);
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value as FieldValue};
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    /// A `workflow_started` boot manifest naming (or not) the workflow
    /// id and the content hash of `recorded_yaml`.
    fn started(id: Option<&str>, recorded_yaml: Option<&str>) -> Event {
        let mut e = Event::new(
            EventId::new(Uuid::nil()),
            Timestamp::from_unix_ms(0),
            EventKind::WorkflowStarted,
        );
        if let Some(id) = id {
            e = e.with_field(KeyValue::new(
                WORKFLOW_FIELD,
                FieldValue::String(id.to_owned()),
            ));
        }
        if let Some(yaml) = recorded_yaml {
            e = e.with_field(KeyValue::new(
                "workflow_sha256",
                FieldValue::String(nika_event::source_id::sha256_hex(yaml.as_bytes())),
            ));
        }
        e
    }

    const BRIEF: &str = "nika: compose-brief\ntasks: {}\n";
    const HELLO: &str = "nika: hello\ntasks: {}\n";

    /// #1586 · the lab's case: a journal written by `compose-brief`
    /// handed to `hello` is FOREIGN — refused naming both ids and both
    /// content hashes, never « file CHANGED ».
    #[test]
    fn a_foreign_journal_is_refused_naming_both_workflows() {
        let events = vec![started(Some("compose-brief"), Some(BRIEF))];
        let SourceVerdict::Foreign(message) = judge_source(&events, Some("hello"), HELLO) else {
            panic!("another `nika:` id is a foreign journal");
        };
        assert!(
            message.contains("journal is workflow `compose-brief`"),
            "names the recorded workflow: {message}"
        );
        assert!(
            message.contains("you asked to run `hello`"),
            "names the requested workflow: {message}"
        );
        let recorded_sha = short(&nika_event::source_id::sha256_hex(BRIEF.as_bytes()));
        let current_sha = short(&nika_event::source_id::sha256_hex(HELLO.as_bytes()));
        assert!(
            message.contains(&recorded_sha) && message.contains(&current_sha),
            "both content hashes are said: {message}"
        );
        assert!(
            !message.contains("CHANGED"),
            "a foreign journal is not an edited file: {message}"
        );
        assert_eq!(trace_workflow(&events), Some("compose-brief"));
    }

    /// The same id with different bytes is a CHANGE (the current file
    /// runs · the notice), and the same bytes are simply the same.
    #[test]
    fn the_same_id_judges_the_bytes() {
        let events = vec![started(Some("hello"), Some(HELLO))];
        assert_eq!(
            judge_source(&events, Some("hello"), HELLO),
            SourceVerdict::Same
        );
        assert_eq!(
            judge_source(&events, Some("hello"), "nika: hello\ntasks: {}\n# edited\n"),
            SourceVerdict::Changed
        );
    }

    /// A journal that recorded no hash is unrecorded on the bytes; a
    /// foreign id still refuses (the hash then reads `unrecorded`).
    #[test]
    fn an_unrecorded_hash_is_no_claim_on_the_bytes_but_the_id_still_speaks() {
        let hashless = vec![started(Some("hello"), None)];
        assert_eq!(
            judge_source(&hashless, Some("hello"), HELLO),
            SourceVerdict::Unbound
        );
        assert!(matches!(
            judge_source(&hashless, Some("other"), HELLO),
            SourceVerdict::Foreign(ref m) if m.contains("sha256 unrecorded")
        ));
    }

    /// An absent id on EITHER side never refuses — the bytes decide.
    /// An empty journal is no claim at all.
    #[test]
    fn an_absent_id_never_refuses() {
        let idless = vec![started(None, Some(HELLO))];
        assert_eq!(
            judge_source(&idless, Some("hello"), HELLO),
            SourceVerdict::Same
        );
        assert_eq!(
            judge_source(&idless, Some("hello"), BRIEF),
            SourceVerdict::Changed
        );
        let named = vec![started(Some("compose-brief"), Some(BRIEF))];
        assert_eq!(judge_source(&named, None, BRIEF), SourceVerdict::Same);
        assert_eq!(
            judge_source(&[], Some("hello"), HELLO),
            SourceVerdict::Unbound
        );
        assert_eq!(trace_workflow(&idless), None);
    }

    /// The comparator is content-aware: a CRLF re-encode of the recorded
    /// LF file is NOT a change (the 0.96.0 review's false positive).
    #[test]
    fn a_crlf_reencode_is_not_a_change() {
        let events = vec![started(Some("hello"), Some(HELLO))];
        let crlf = HELLO.replace('\n', "\r\n");
        assert_eq!(source_drifted(&crlf, &events), Some(false));
        assert_eq!(source_drifted("nika: hello\n", &events), Some(true));
        assert_eq!(source_drifted(HELLO, &[]), None);
    }
}
