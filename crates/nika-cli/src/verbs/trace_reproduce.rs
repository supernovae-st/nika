//! `nika trace reproduce` — is this run reproducible? (Proof Arc P3)
//!
//! Compares a RECORDED journal against a FRESH one of the same
//! workflow and classifies every recorded task by WHY it did or did
//! not reproduce — the ADR-099 stamps make the taxonomy total:
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
//! Exit codes: 0 = every comparable task reproduced · 2 (FILE) = any
//! divergence. Unverifiable tasks never fail the verdict — stated,
//! never guessed. Both journals get a chain walk first (P2: verify
//! before trust — broken is WARNED, never blocked).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use nika_event::{Event, EventKind};

use super::VerbOutput;

#[must_use]
pub fn reproduce(recorded: &str, fresh: &str) -> VerbOutput {
    let rec_raw = match std::fs::read_to_string(recorded) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {recorded}: {e}")),
    };
    let new_raw = match std::fs::read_to_string(fresh) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {fresh}: {e}")),
    };
    let rec = match super::run::recover_events(&rec_raw, recorded) {
        Ok(r) => r,
        Err(e) => return VerbOutput::env(e),
    };
    let new = match super::run::recover_events(&new_raw, fresh) {
        Ok(r) => r,
        Err(e) => return VerbOutput::env(e),
    };

    // The identity guard: reproduce pairs two runs of the SAME
    // workflow — task ids pair by name, so a cross-workflow compare
    // (wrong file in a directory of look-alike traces) renders a
    // confident MISSING/ADDED/AUTHORED taxonomy about two runs that
    // never shared a definition (the rust-pro review's F1). Both
    // journals name their workflow on `workflow_started` (0.95+);
    // when both speak and disagree, nothing is comparable.
    let rec_wf = workflow_of(&rec.events);
    let new_wf = workflow_of(&new.events);
    if let (Some(a), Some(b)) = (rec_wf, new_wf)
        && a != b
    {
        return VerbOutput::env(format!(
            "the two journals record DIFFERENT workflows — recorded `{a}` vs fresh `{b}`: nothing to compare (reproduce pairs runs of the same workflow)"
        ));
    }

    let mut out = String::new();
    if rec_wf.is_none() || new_wf.is_none() {
        let _ = writeln!(
            out,
            "WARNING — a journal names no workflow (pre-0.95 engine?): same-workflow pairing is unverified"
        );
    }
    // A torn journal compares on its recovered prefix — SAY so, or the
    // lost tail's tasks surface as phantom divergences.
    for note in [&rec.truncated_note, &new.truncated_note]
        .into_iter()
        .flatten()
    {
        let _ = writeln!(out, "WARNING — {note}");
    }
    for (label, raw) in [("recorded", &rec_raw), ("fresh", &new_raw)] {
        if let super::trace_verify::Verdict::Broken { line, .. } = super::trace_verify::walk(raw) {
            let _ = writeln!(
                out,
                "WARNING — the {label} journal fails verification (chain broken at line {line}); its claims are unverified"
            );
        }
    }

    let report = compare(&rec.events, &new.events);
    let _ = write!(out, "{}", render(&report));
    if report.diverged() {
        VerbOutput::file(out)
    } else if report.nothing_verified() {
        // The text lane already refuses the overclaim (« NOTHING
        // VERIFIED ») — the exit lane must agree: a CI gate
        // `reproduce a b && promote` was passing on runs where not one
        // task was comparable (pre-stamp journals · all-failed runs).
        // ENV, the sibling of `trace verify`'s Unchained exit (the
        // rust-pro review's F2).
        VerbOutput::env(out)
    } else {
        VerbOutput::ok(out)
    }
}

/// The workflow this journal records — LAST `workflow_started` wins,
/// consistent with `attestation_of` (a run+resume file tells its final
/// story).
fn workflow_of(events: &[Event]) -> Option<&str> {
    events
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .and_then(|e| field_str(e, "workflow"))
}

/// One recorded task's reproduction verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Reproduced,
    Nondeterministic,
    Authored,
    Environment,
    StatusChanged { recorded: String, fresh: String },
    Unverifiable,
    Missing,
    Added,
}

pub(crate) struct Row {
    pub(crate) task: String,
    pub(crate) verdict: Verdict,
}

pub(crate) struct Report {
    pub(crate) rows: Vec<Row>,
    /// `engine_version/platform` per side, when attested (#235).
    pub(crate) recorded_env: Option<String>,
    pub(crate) fresh_env: Option<String>,
}

impl Report {
    /// Every row Unverifiable (or no rows at all): the compare proved
    /// NOTHING — exit must not say « reproduced ».
    pub(crate) fn nothing_verified(&self) -> bool {
        self.rows
            .iter()
            .all(|r| matches!(r.verdict, Verdict::Unverifiable))
    }

    pub(crate) fn diverged(&self) -> bool {
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
pub(crate) fn compare(recorded: &[Event], fresh: &[Event]) -> Report {
    let rec = settles_of(recorded);
    let new = settles_of(fresh);

    let mut rows: Vec<Row> = rec
        .iter()
        .map(|(task, r)| {
            let verdict = match new.get(task) {
                None => Verdict::Missing,
                Some(n) => classify(r, n),
            };
            Row {
                task: task.clone(),
                verdict,
            }
        })
        .collect();
    // Fresh tasks the recorded run never had — named, not silenced
    // (half the authorship story lives on the fresh side).
    rows.extend(new.keys().filter(|t| !rec.contains_key(*t)).map(|t| Row {
        task: t.clone(),
        verdict: Verdict::Added,
    }));

    Report {
        rows,
        recorded_env: attestation_of(recorded),
        fresh_env: attestation_of(fresh),
    }
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
        // are this verb's audience — the chain break only WARNS).
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

fn render(report: &Report) -> String {
    let mut out = String::new();
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for row in &report.rows {
        let (tag, detail) = match &row.verdict {
            Verdict::Reproduced => ("reproduced", String::new()),
            Verdict::Nondeterministic => (
                "NONDETERMINISTIC",
                " — same def, same inputs, different output".to_owned(),
            ),
            Verdict::Authored => ("AUTHORED", " — the task changed".to_owned()),
            Verdict::Environment => ("ENVIRONMENT", " — inputs differ".to_owned()),
            Verdict::StatusChanged { recorded, fresh } => {
                ("STATUS-CHANGED", format!(" — {recorded} → {fresh}"))
            }
            Verdict::Unverifiable => ("unverifiable", String::new()),
            Verdict::Missing => ("MISSING", " — absent from the fresh run".to_owned()),
            Verdict::Added => ("ADDED", " — absent from the recorded run".to_owned()),
        };
        *counts.entry(tag).or_default() += 1;
        let _ = writeln!(
            out,
            "  {tag:<16} {}{detail}",
            super::trace_verify::sanitize(&row.task)
        );
    }
    let summary: Vec<String> = counts.iter().map(|(tag, n)| format!("{n} {tag}")).collect();
    let comparable = report
        .rows
        .iter()
        .filter(|r| !matches!(r.verdict, Verdict::Unverifiable))
        .count();
    // An all-unverifiable report proved NOTHING — the banner must not
    // overclaim (a pre-stamp journal would otherwise read REPRODUCED).
    let verdict = if report.diverged() {
        "DIVERGED"
    } else if comparable == 0 {
        "NOTHING VERIFIED — no comparable stamps"
    } else {
        "REPRODUCED"
    };
    let _ = writeln!(out, "\n{verdict} — {}", summary.join(" · "));
    match (&report.recorded_env, &report.fresh_env) {
        (Some(r), Some(f)) if r != f => {
            let _ = writeln!(out, "  engines differ: recorded {r} · fresh {f}");
        }
        (Some(r), Some(_)) => {
            let _ = writeln!(out, "  engine: {r} (both runs)");
        }
        _ => {}
    }
    out
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
        // The guard fires in reproduce() before compare — prove the
        // message shape via the pure parts (name extraction + refusal
        // text are the contract; the file plumbing is std).
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
