// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The local UX metrics journal (W8 · audit UX 2026-07-30, P1).
//!
//! One append-only JSONL file at `~/.nika/metrics.ndjson` — the raw
//! material the TTFP/TTFRV/TTSR folds read later. Three laws, each
//! enforced by construction, not by discipline:
//!
//! 1. **Local only** — telemetry-canon §0: zero cloud, zero network.
//!    The journal sits beside the registry cache under `~/.nika/` and
//!    never leaves the machine.
//! 2. **Off by default** — spec H11 « zero telemetry by default »:
//!    nothing is written unless the operator opts in with
//!    `NIKA_METRICS=1` (`true`/`on` accepted). Without the opt-in the
//!    whole module is a no-op and no directory is even created.
//! 3. **Content-free by construction** — the facts are a CLOSED
//!    whitelist: enums, bools and counters only. There is no string
//!    field an event could smuggle a prompt, a path, a file's content,
//!    a secret, PII, a query-bearing URL or model output through — the
//!    type forbids it, so the audit reads the type, not every call
//!    site.
//!
//! Metrics are diagnostic, never on the data path: a failed append is
//! swallowed (`record_if_enabled`), the verb's bytes and exit code
//! never depend on the journal.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// The closed event vocabulary — `snake_case` on the wire. Every kind the
/// W8 audit named; the ones with no CLI surface yet (a selection needs
/// an interactive picker · a backtrack needs a TUI) are reserved for
/// the surfaces that can OBSERVE them — an event is only ever recorded
/// where it truly happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A call-to-action was shown (the concierge's start moves).
    CtaImpression,
    /// A call-to-action was picked. Reserved: no CLI surface selects
    /// today (the operator types the next command themselves).
    CtaSelected,
    /// The session context resolved (welcome's envelope).
    ContextResolved,
    /// The guided flow understood a plain-words intent. Reserved.
    IntentUnderstood,
    /// A preview rendered. Reserved.
    PreviewReached,
    /// `new`/guided wrote a draft on disk.
    DraftCreated,
    /// The check ladder came back green.
    CheckPassed,
    /// The machine proved real readiness. Reserved.
    RealReadinessProven,
    /// The run was handed back to the human (guard allow · welcome's
    /// run CTA).
    HumanRunHandoff,
    /// The first real value landed. Reserved.
    FirstRealValue,
    /// The second success (the habit signal). Reserved.
    SecondSuccess,
    /// The operator walked a choice back. Reserved.
    UserBacktracked,
    /// The operator refused a suggestion. Reserved.
    UserDeclined,
    /// A suggestion was withheld on purpose. Reserved.
    SuggestionSuppressed,
    /// An unexpected effect was reported. Reserved.
    UnexpectedEffectReport,
}

/// The concierge's CTA classes (`cta_impression` · `cta_selected`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cta {
    /// Found something — `init` · `new`.
    Create,
    /// Go see — `examples` · `welcome --deep`.
    Discover,
    /// Keep going on the work at hand — `run` · `check`.
    Continue,
}

/// Where the run came back to human hands (`human_run_handoff`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Handoff {
    /// The hook's judge allowed the run (`nika guard`).
    GuardAllow,
    /// The concierge led with a `run` CTA on an audited-clean file.
    WelcomeCta,
}

/// Which surface wrote the draft (`draft_created`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftSource {
    /// `nika new --from <template|example>` — direct instantiation.
    New,
    /// The guided conversation materialized its wizard.
    Guided,
}

/// The resolved session shape (`context_resolved`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Session {
    /// No reliable project evidence — the chat-only envelope.
    ChatOnly,
    /// A workspace resolved (the root itself never rides the event).
    Workspace,
}

/// The closed fact whitelist. Every field is optional and skipped when
/// absent, so an event carries exactly what it means — and NOTHING
/// else: there is deliberately no `String` field anywhere in this
/// struct. Enums · bools · counters, full stop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Facts {
    /// The CTA class (`cta_impression` · `cta_selected`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cta: Option<Cta>,
    /// The handoff surface (`human_run_handoff`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<Handoff>,
    /// The draft's source (`draft_created`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<DraftSource>,
    /// The resolved session shape (`context_resolved`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<Session>,
    /// A boolean fact (e.g. the context expanded from a subdir).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag: Option<bool>,
    /// A counter fact (e.g. the moves shown).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

impl Facts {
    /// The empty fact set — events that ARE their whole payload.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// True when every field is absent (the line then omits `facts`).
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One journal line — `v` versions the envelope (additive-only, the
/// same law every machine surface here obeys). `ts` is whole seconds
/// since the Unix epoch: ordering, never a fingerprint.
#[derive(Serialize)]
struct Line {
    v: u8,
    ts: u64,
    event: EventKind,
    #[serde(skip_serializing_if = "Facts::is_empty")]
    facts: Facts,
}

/// Seconds since the Unix epoch — `0` when the clock reads before 1970
/// (never a guess, never a panic).
fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Append one event to the journal at `path` (creating the parent dir).
/// The fallible, explicit form — tests and any future surface that
/// wants the error speak this; the verbs speak [`record_if_enabled`].
///
/// # Errors
///
/// The underlying filesystem error when the parent dir cannot be
/// created or the journal cannot be opened/appended (a serialization
/// failure is NOT one: this closed shape cannot produce one, and the
/// law is zero unwraps — it degrades to a skipped event instead).
pub fn record(path: &Path, event: EventKind, facts: Facts) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let line = Line {
        v: 1,
        ts: epoch_secs(),
        event,
        facts,
    };
    // serde_json::to_string on this closed shape cannot fail (no map
    // with non-string keys, no float) — but the law is zero unwraps:
    // a serialization failure degrades to a skipped event, the same
    // posture as a failed append.
    let Ok(mut body) = serde_json::to_string(&line) else {
        return Ok(());
    };
    body.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(body.as_bytes())
}

/// The opt-in gate, pure for tests: `NIKA_METRICS` must say `1`, `true`
/// or `on` — every other value (including set-but-empty) keeps the
/// journal off (spec H11 · zero telemetry by default).
fn enabled(var: Option<&str>) -> bool {
    matches!(var, Some("1" | "true" | "on"))
}

/// The journal's canonical home: `~/.nika/metrics.ndjson` — beside the
/// registry cache, the house's user-level store.
#[allow(clippy::disallowed_methods)] // the probe.rs home_dir precedent
fn journal_path() -> Option<PathBuf> {
    if !enabled(std::env::var("NIKA_METRICS").ok().as_deref()) {
        return None;
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".nika").join("metrics.ndjson"))
}

/// Record an event when the operator opted in — the verbs' form. Off
/// by default; a failed append is silent BY DESIGN (diagnostic, never
/// on the data path: the verb's bytes and exit code are already
/// decided when this runs).
pub fn record_if_enabled(event: EventKind, facts: Facts) {
    let Some(path) = journal_path() else {
        return;
    };
    let _ = record(&path, event, facts);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The journal is append-only JSONL: one line per event, every line
    /// a parseable envelope with the version stamp and the `snake_case`
    /// event name.
    #[test]
    fn record_appends_parseable_envelopes() {
        let dir = tempfile::tempdir().expect("scratch");
        let journal = dir.path().join("nested").join("metrics.ndjson");
        record(&journal, EventKind::CheckPassed, Facts::none()).expect("first append");
        record(
            &journal,
            EventKind::ContextResolved,
            Facts {
                session: Some(Session::Workspace),
                flag: Some(true),
                ..Facts::none()
            },
        )
        .expect("second append");
        let body = std::fs::read_to_string(&journal).expect("journal readable");
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2, "one line per event: {body}");
        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("line 1 is json");
        assert_eq!(first["v"], 1);
        assert_eq!(first["event"], "check_passed", "{first:#}");
        assert!(first["ts"].as_u64().is_some(), "a ts rides: {first:#}");
        assert!(
            first.get("facts").is_none(),
            "an empty fact set is omitted: {first:#}"
        );
        let second: serde_json::Value = serde_json::from_str(lines[1]).expect("line 2 is json");
        assert_eq!(second["event"], "context_resolved");
        assert_eq!(second["facts"]["session"], "workspace");
        assert_eq!(second["facts"]["flag"], true);
    }

    /// The whitelist is closed BY CONSTRUCTION: a fully-populated
    /// `Facts` serializes exactly the six known keys — any future field
    /// (a string would be the smuggle) turns this test red at review.
    #[test]
    fn the_fact_whitelist_stays_closed() {
        let facts = Facts {
            cta: Some(Cta::Create),
            handoff: Some(Handoff::GuardAllow),
            draft: Some(DraftSource::Guided),
            session: Some(Session::ChatOnly),
            flag: Some(false),
            count: Some(3),
        };
        let dir = tempfile::tempdir().expect("scratch");
        let journal = dir.path().join("metrics.ndjson");
        record(&journal, EventKind::CtaSelected, facts).expect("append");
        let body = std::fs::read_to_string(&journal).expect("journal readable");
        let line: serde_json::Value = serde_json::from_str(body.trim()).expect("json");
        let mut keys: Vec<&str> = line["facts"]
            .as_object()
            .expect("facts object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["count", "cta", "draft", "flag", "handoff", "session"],
            "the closed whitelist — no free string can exist: {line:#}"
        );
        assert_eq!(line["facts"]["cta"], "create");
        assert_eq!(line["facts"]["handoff"], "guard_allow");
        assert_eq!(line["facts"]["draft"], "guided");
        assert_eq!(line["facts"]["session"], "chat_only");
        assert_eq!(line["facts"]["count"], 3);
    }

    /// The opt-in gate: unset · empty · `0` · any other word keep the
    /// journal OFF; only the three documented truths switch it on.
    #[test]
    fn the_opt_in_gate_is_explicit() {
        for off in [None, Some(""), Some("0"), Some("yes"), Some("TRUE")] {
            assert!(!enabled(off), "off: {off:?}");
        }
        for on in [Some("1"), Some("true"), Some("on")] {
            assert!(enabled(on), "on: {on:?}");
        }
    }

    /// Every reserved kind still serializes its `snake_case` name — the
    /// vocabulary the W8 audit named, pinned so a rename is a choice,
    /// never a drift.
    #[test]
    fn the_event_vocabulary_is_the_audits_own() {
        let names = |k: EventKind| serde_json::to_string(&k).expect("name");
        assert_eq!(names(EventKind::CtaImpression), "\"cta_impression\"");
        assert_eq!(names(EventKind::CtaSelected), "\"cta_selected\"");
        assert_eq!(names(EventKind::ContextResolved), "\"context_resolved\"");
        assert_eq!(names(EventKind::IntentUnderstood), "\"intent_understood\"");
        assert_eq!(names(EventKind::PreviewReached), "\"preview_reached\"");
        assert_eq!(names(EventKind::DraftCreated), "\"draft_created\"");
        assert_eq!(names(EventKind::CheckPassed), "\"check_passed\"");
        assert_eq!(
            names(EventKind::RealReadinessProven),
            "\"real_readiness_proven\""
        );
        assert_eq!(names(EventKind::HumanRunHandoff), "\"human_run_handoff\"");
        assert_eq!(names(EventKind::FirstRealValue), "\"first_real_value\"");
        assert_eq!(names(EventKind::SecondSuccess), "\"second_success\"");
        assert_eq!(names(EventKind::UserBacktracked), "\"user_backtracked\"");
        assert_eq!(names(EventKind::UserDeclined), "\"user_declined\"");
        assert_eq!(
            names(EventKind::SuggestionSuppressed),
            "\"suggestion_suppressed\""
        );
        assert_eq!(
            names(EventKind::UnexpectedEffectReport),
            "\"unexpected_effect_report\""
        );
    }
}
