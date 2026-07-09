// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reproduce comparison (Proof Arc P3) — a RECORDED journal against
//! a FRESH one of the same workflow, every recorded task classified by
//! WHY it did or did not reproduce. The ADR-099 stamps make the
//! taxonomy total:
//!
//! - `reproduced` — def and inputs match · output identical
//! - `nondeterministic` — def and inputs match · output DIFFERS (the
//!   interesting one: a model call · a clock · an unpinned tool)
//! - `authored` — `def_hash` differs: the task itself changed
//! - `environment` — inputs differ (upstream outputs · vars · env)
//! - `status-changed` — settled differently (completed → failed · …)
//! - `unverifiable` — no stamps to compare (skips · cancels · fails)
//! - `missing` — absent from the fresh run
//! - `added` — a fresh task the recorded run never had (informational:
//!   the recorded journal is the reference frame, but silence would
//!   hide half the authorship story)
//!
//! Unverifiable tasks never fail the verdict — stated, never guessed.
//!
//! Descended from `nika-cli`'s `trace_reproduce` verb (2026-07-09 · the
//! W0 trace descent); the CLI keeps the file plumbing, the renderer and
//! the exit mapping as a shim over this comparison.

use std::collections::BTreeMap;

use nika_event::{Event, EventKind};

/// The workflow this journal records — LAST `workflow_started` wins,
/// consistent with the attestation fold (a run+resume file tells its
/// final story). The caller's identity guard: reproduce pairs two runs
/// of the SAME workflow — task ids pair by name, so a cross-workflow
/// compare must refuse before it classifies.
#[must_use]
pub fn workflow_of(events: &[Event]) -> Option<&str> {
    events
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .and_then(|e| field_str(e, "workflow"))
}

/// One recorded task's reproduction verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// def and inputs match · output identical.
    Reproduced,
    /// def and inputs match · output differs.
    Nondeterministic,
    /// `def_hash` differs: the task itself changed.
    Authored,
    /// Inputs differ (upstream outputs · vars · env).
    Environment,
    /// Settled differently on the two sides.
    #[non_exhaustive]
    StatusChanged {
        /// The recorded side's settle status word.
        recorded: String,
        /// The fresh side's settle status word.
        fresh: String,
    },
    /// No stamps to compare (skips · cancels · fails).
    Unverifiable,
    /// Absent from the fresh run.
    Missing,
    /// A fresh task the recorded run never had.
    Added,
}

/// One task row of the comparison report.
#[derive(Debug)]
#[non_exhaustive]
pub struct Row {
    /// The task id (as recorded — an adversarial id is the RENDERER's
    /// problem, never silently rewritten here).
    pub task: String,
    /// The task's reproduction verdict.
    pub verdict: Verdict,
}

impl Row {
    /// Assemble a row (invariant #19: every `#[non_exhaustive]` struct
    /// constructs through `new`, never a literal).
    #[must_use]
    pub fn new(task: String, verdict: Verdict) -> Self {
        Self { task, verdict }
    }
}

/// The whole comparison — rows in recorded order, then the fresh-only
/// tasks, plus the per-side engine attestations.
#[derive(Debug)]
#[non_exhaustive]
pub struct Report {
    /// One row per recorded task, then the `Added` rows.
    pub rows: Vec<Row>,
    /// `engine_version/platform` for the recorded side, when attested (#235).
    pub recorded_env: Option<String>,
    /// `engine_version/platform` for the fresh side, when attested.
    pub fresh_env: Option<String>,
}

impl Report {
    /// Assemble a report (invariant #19).
    #[must_use]
    pub fn new(rows: Vec<Row>, recorded_env: Option<String>, fresh_env: Option<String>) -> Self {
        Self {
            rows,
            recorded_env,
            fresh_env,
        }
    }

    /// Every row Unverifiable (or no rows at all): the compare proved
    /// NOTHING — an exit lane must not say « reproduced ».
    #[must_use]
    pub fn nothing_verified(&self) -> bool {
        self.rows
            .iter()
            .all(|r| matches!(r.verdict, Verdict::Unverifiable))
    }

    /// Any comparable row that did not reproduce.
    #[must_use]
    pub fn diverged(&self) -> bool {
        // ADDED diverges too: the definition set itself changed.
        self.rows
            .iter()
            .any(|r| !matches!(r.verdict, Verdict::Reproduced | Verdict::Unverifiable))
    }
}

/// One task's comparable settle facts.
#[derive(Default, Clone)]
struct Settle {
    status: Option<&'static str>,
    def_hash: Option<String>,
    input_hash: Option<String>,
    output: Option<String>,
}

/// The pure comparison — the RECORDED journal is the reference frame.
#[must_use]
pub fn compare(recorded: &[Event], fresh: &[Event]) -> Report {
    let rec = settles_of(recorded);
    let new = settles_of(fresh);

    let mut rows: Vec<Row> = rec
        .iter()
        .map(|(task, r)| {
            let verdict = match new.get(task) {
                None => Verdict::Missing,
                Some(n) => classify(r, n),
            };
            Row::new(task.clone(), verdict)
        })
        .collect();
    // Fresh tasks the recorded run never had — named, not silenced
    // (half the authorship story lives on the fresh side).
    rows.extend(
        new.keys()
            .filter(|t| !rec.contains_key(*t))
            .map(|t| Row::new(t.clone(), Verdict::Added)),
    );

    Report::new(rows, attestation_of(recorded), attestation_of(fresh))
}

fn classify(rec: &Settle, new: &Settle) -> Verdict {
    if rec.status != new.status {
        return Verdict::StatusChanged {
            recorded: rec.status.unwrap_or("?").to_owned(),
            fresh: new.status.unwrap_or("?").to_owned(),
        };
    }
    // Stamps ride completed frames only — anything else has nothing
    // to compare beyond its (matching) status.
    let (Some(rd), Some(nd)) = (&rec.def_hash, &new.def_hash) else {
        return Verdict::Unverifiable;
    };
    if rd != nd {
        return Verdict::Authored;
    }
    match (&rec.input_hash, &new.input_hash) {
        (Some(ri), Some(ni)) if ri != ni => return Verdict::Environment,
        // One-sided stamp: symmetry with the def_hash guard — a pair
        // with input evidence on only ONE side must not be labeled
        // « same inputs, different output » (NONDETERMINISTIC) on no
        // input evidence at all (hand-edited / field-stripped journals
        // are this comparison's audience — a chain break only WARNS).
        (Some(_), None) | (None, Some(_)) => return Verdict::Unverifiable,
        _ => {}
    }
    if outputs_equal(rec.output.as_deref(), new.output.as_deref()) {
        Verdict::Reproduced
    } else {
        Verdict::Nondeterministic
    }
}

/// Value equality, not byte equality: a future serializer drift (JCS ·
/// float formatting) across engine VERSIONS must not read as
/// nondeterminism when the values are equal. Unparseable falls back to
/// byte compare.
fn outputs_equal(rec: Option<&str>, new: Option<&str>) -> bool {
    match (rec, new) {
        (None, None) => true,
        (Some(r), Some(n)) => {
            match (
                serde_json::from_str::<serde_json::Value>(r),
                serde_json::from_str::<serde_json::Value>(n),
            ) {
                (Ok(rv), Ok(nv)) => rv == nv,
                _ => r == n,
            }
        }
        _ => false,
    }
}

/// Fold the terminal frame per task — LAST terminal wins, the resume
/// fold's own convention: a file carrying run + resume appended
/// compares on its FINAL story (an early failed attempt must not
/// shadow the later cache-hit settle).
fn settles_of(events: &[Event]) -> BTreeMap<String, Settle> {
    let mut out: BTreeMap<String, Settle> = BTreeMap::new();
    for e in events {
        let status = match e.kind {
            // A cache hit replays a completed settle byte-for-byte (the
            // ADR-099 trio rides both frames) — normalize so a --resume
            // journal never reads as a status flip.
            EventKind::TaskCompleted | EventKind::TaskCacheHit => "completed",
            EventKind::TaskFailed => "failed",
            EventKind::TaskSkipped => "skipped",
            EventKind::TaskCancelled => "cancelled",
            _ => continue,
        };
        let Some(task) = field_str(e, "task") else {
            continue;
        };
        // LAST terminal wins — the resume fold's own convention (a file
        // carrying run + resume appended tells its FINAL story).
        let entry = out.entry(task.to_owned()).or_default();
        entry.status = Some(status);
        entry.def_hash = field_str(e, "def_hash").map(ToOwned::to_owned);
        entry.input_hash = field_str(e, "input_hash").map(ToOwned::to_owned);
        entry.output = field_str(e, "output").map(ToOwned::to_owned);
    }
    out
}

fn attestation_of(events: &[Event]) -> Option<String> {
    // LAST started wins — a run+resume-appended journal attests the
    // engine that produced its final story (consistent with settles_of).
    let started = events
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::WorkflowStarted)?;
    let version = field_str(started, "engine_version")?;
    let platform = field_str(started, "platform").unwrap_or("?");
    Some(format!("{version}/{platform}"))
}

fn field_str<'e>(event: &'e Event, key: &str) -> Option<&'e str> {
    event.fields.iter().find_map(|f| match (&f.key, &f.value) {
        (k, nika_types::resource::Value::String(s)) if k == key => Some(s.as_str()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value as FieldValue};
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    use super::*;

    fn ev(seed: u8, kind: EventKind, fields: &[(&str, &str)]) -> Event {
        let mut e = Event::new(
            EventId::new(Uuid::from_bytes([seed; 16])),
            Timestamp::from_unix_ns(i64::from(seed) * 1_000),
            kind,
        );
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, FieldValue::String((*v).to_owned())));
        }
        e
    }

    fn completed(seed: u8, task: &str, def: &str, input: &str, output: &str) -> Event {
        ev(
            seed,
            EventKind::TaskCompleted,
            &[
                ("task", task),
                ("def_hash", def),
                ("input_hash", input),
                ("output", output),
            ],
        )
    }

    #[test]
    fn cross_workflow_compare_is_refused_not_classified() {
        // F1: task ids pair by NAME — two unrelated workflows sharing
        // `fetch`/`summarize` rendered a confident taxonomy about runs
        // that never shared a definition. Both sides name their
        // workflow; disagreement is a refusal (ENV), never a report.
        let a = vec![ev(
            1,
            EventKind::WorkflowStarted,
            &[("workflow", "site-audit")],
        )];
        let b = vec![ev(
            2,
            EventKind::WorkflowStarted,
            &[("workflow", "blog-run")],
        )];
        assert_eq!(workflow_of(&a), Some("site-audit"));
        assert_eq!(workflow_of(&b), Some("blog-run"));
        // The guard fires in the CLI's reproduce() before compare —
        // prove the name extraction here (the refusal text + the file
        // plumbing are the shim's contract).
    }

    #[test]
    fn all_unverifiable_is_not_reproduced() {
        // F2: two pre-stamp journals (no trio) — the text says NOTHING
        // VERIFIED; the exit lane must agree (nothing_verified → ENV).
        let rec = vec![
            ev(1, EventKind::WorkflowStarted, &[("workflow", "w")]),
            ev(2, EventKind::TaskCompleted, &[("task", "a")]),
        ];
        let new = vec![
            ev(3, EventKind::WorkflowStarted, &[("workflow", "w")]),
            ev(4, EventKind::TaskCompleted, &[("task", "a")]),
        ];
        let report = compare(&rec, &new);
        assert!(report.nothing_verified(), "no stamps on either side");
        assert!(!report.diverged());
    }

    #[test]
    fn last_terminal_wins_within_one_journal() {
        // F4: the fn doc used to claim first-wins while the code (and
        // the resume fold) are last-wins — pin the behavior so a
        // doc-driven « fix » cannot flip it (an early failed attempt
        // must not shadow the later cache-hit settle).
        let events = vec![
            ev(1, EventKind::TaskFailed, &[("task", "a")]),
            completed(2, "a", "d1", "i1", "\"ok\""),
        ];
        let settles = settles_of(&events);
        assert_eq!(settles.get("a").and_then(|s| s.status), Some("completed"));
    }

    #[test]
    fn one_sided_input_stamp_is_unverifiable_not_nondeterministic() {
        // F7: « same def, same inputs, different output » requires
        // input evidence on BOTH sides — symmetry with the def guard.
        let rec = Settle {
            status: Some("completed"),
            def_hash: Some("d".into()),
            input_hash: Some("i".into()),
            output: Some("\"a\"".into()),
        };
        let new = Settle {
            status: Some("completed"),
            def_hash: Some("d".into()),
            input_hash: None,
            output: Some("\"b\"".into()),
        };
        assert!(matches!(classify(&rec, &new), Verdict::Unverifiable));
    }

    #[test]
    fn the_taxonomy_names_why() {
        let recorded = vec![
            ev(
                1,
                EventKind::WorkflowStarted,
                &[
                    ("workflow", "w"),
                    ("engine_version", "0.95.0"),
                    ("platform", "macos/aarch64"),
                ],
            ),
            completed(2, "same", "d1", "i1", "\"out\""),
            completed(3, "flaky", "d2", "i2", "\"a\""),
            completed(4, "edited", "d3", "i3", "\"x\""),
            completed(5, "shifted", "d4", "i4", "\"x\""),
            ev(6, EventKind::TaskSkipped, &[("task", "gated")]),
            completed(7, "gone", "d5", "i5", "\"x\""),
        ];
        let fresh = vec![
            ev(
                8,
                EventKind::WorkflowStarted,
                &[
                    ("workflow", "w"),
                    ("engine_version", "0.96.0"),
                    ("platform", "macos/aarch64"),
                ],
            ),
            completed(9, "same", "d1", "i1", "\"out\""),
            completed(10, "flaky", "d2", "i2", "\"b\""),
            completed(11, "edited", "dX", "i3", "\"x\""),
            completed(12, "shifted", "d4", "iX", "\"x\""),
            ev(13, EventKind::TaskSkipped, &[("task", "gated")]),
        ];
        let report = compare(&recorded, &fresh);
        let verdict = |task: &str| {
            report
                .rows
                .iter()
                .find(|r| r.task == task)
                .expect("row")
                .verdict
                .clone()
        };
        assert_eq!(verdict("same"), Verdict::Reproduced);
        assert_eq!(verdict("flaky"), Verdict::Nondeterministic);
        assert_eq!(verdict("edited"), Verdict::Authored);
        assert_eq!(verdict("shifted"), Verdict::Environment);
        assert_eq!(
            verdict("gated"),
            Verdict::Unverifiable,
            "skips have no stamps"
        );
        assert_eq!(verdict("gone"), Verdict::Missing);
        assert!(
            !report.rows.iter().any(|r| r.verdict == Verdict::Added),
            "no phantom ADDED rows on this fixture"
        );
        assert!(report.diverged());
        // The attestation comparison rides (#235 closing another loop).
        assert_eq!(report.recorded_env.as_deref(), Some("0.95.0/macos/aarch64"));
        assert_eq!(report.fresh_env.as_deref(), Some("0.96.0/macos/aarch64"));
    }

    #[test]
    fn a_fresh_task_the_recorded_run_never_had_is_named() {
        let recorded = vec![completed(1, "a", "d", "i", "\"x\"")];
        let fresh = vec![
            completed(2, "a", "d", "i", "\"x\""),
            completed(3, "brand_new", "d9", "i9", "\"y\""),
        ];
        let report = compare(&recorded, &fresh);
        let added = report
            .rows
            .iter()
            .find(|r| r.task == "brand_new")
            .expect("named, not silenced");
        assert_eq!(added.verdict, Verdict::Added);
        assert!(report.diverged(), "the definition set changed");
    }

    #[test]
    fn a_status_flip_names_both_sides() {
        let recorded = vec![completed(1, "a", "d", "i", "\"x\"")];
        let fresh = vec![ev(
            2,
            EventKind::TaskFailed,
            &[("task", "a"), ("detail", "boom")],
        )];
        let report = compare(&recorded, &fresh);
        assert_eq!(
            report.rows[0].verdict,
            Verdict::StatusChanged {
                recorded: "completed".to_owned(),
                fresh: "failed".to_owned()
            }
        );
    }

    #[test]
    fn a_resume_cache_hit_is_the_same_completed_settle() {
        // H2: a --resume journal rehydrates via task_cache_hit carrying
        // the SAME ADR-099 trio — never a status flip.
        let recorded = vec![completed(1, "a", "d", "i", "\"x\"")];
        let fresh = vec![ev(
            2,
            EventKind::TaskCacheHit,
            &[
                ("task", "a"),
                ("def_hash", "d"),
                ("input_hash", "i"),
                ("output", "\"x\""),
            ],
        )];
        let report = compare(&recorded, &fresh);
        assert_eq!(report.rows[0].verdict, Verdict::Reproduced);
    }

    #[test]
    fn value_equal_outputs_reproduce_across_serializer_drift() {
        // M2: key order is presentation, not value — cross-version
        // serializer drift must not read as nondeterminism.
        let recorded = vec![completed(1, "a", "d", "i", "{\"x\":1,\"y\":2}")];
        let fresh = vec![completed(2, "a", "d", "i", "{\"y\":2,\"x\":1}")];
        let report = compare(&recorded, &fresh);
        assert_eq!(report.rows[0].verdict, Verdict::Reproduced);
    }

    #[test]
    fn identical_runs_reproduce_with_exit_zero_semantics() {
        let run = vec![
            completed(1, "a", "d1", "i1", "\"x\""),
            ev(2, EventKind::TaskSkipped, &[("task", "g")]),
        ];
        let report = compare(&run, &run);
        assert!(!report.diverged(), "unverifiable never fails the verdict");
    }
}
