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

    let mut out = String::new();
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
    } else {
        VerbOutput::ok(out)
    }
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
        _ => {}
    }
    if rec.output == new.output {
        Verdict::Reproduced
    } else {
        Verdict::Nondeterministic
    }
}

/// Fold the terminal frame per task (first terminal wins — reruns
/// within one journal are the resume path's business, not ours).
fn settles_of(events: &[Event]) -> BTreeMap<String, Settle> {
    let mut out: BTreeMap<String, Settle> = BTreeMap::new();
    for e in events {
        let status = match e.kind {
            EventKind::TaskCompleted => "completed",
            EventKind::TaskFailed => "failed",
            EventKind::TaskSkipped => "skipped",
            EventKind::TaskCancelled => "cancelled",
            EventKind::TaskCacheHit => "cache-hit",
            _ => continue,
        };
        let Some(task) = field_str(e, "task") else {
            continue;
        };
        let entry = out.entry(task.to_owned()).or_default();
        if entry.status.is_some() {
            continue;
        }
        entry.status = Some(status);
        entry.def_hash = field_str(e, "def_hash").map(ToOwned::to_owned);
        entry.input_hash = field_str(e, "input_hash").map(ToOwned::to_owned);
        entry.output = field_str(e, "output").map(ToOwned::to_owned);
    }
    out
}

fn attestation_of(events: &[Event]) -> Option<String> {
    let started = events
        .iter()
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
        let _ = writeln!(out, "  {tag:<16} {}{detail}", row.task);
    }
    let summary: Vec<String> = counts.iter().map(|(tag, n)| format!("{n} {tag}")).collect();
    let verdict = if report.diverged() {
        "DIVERGED"
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
    fn identical_runs_reproduce_with_exit_zero_semantics() {
        let run = vec![
            completed(1, "a", "d1", "i1", "\"x\""),
            ev(2, EventKind::TaskSkipped, &[("task", "g")]),
        ];
        let report = compare(&run, &run);
        assert!(!report.diverged(), "unverifiable never fails the verdict");
    }
}
