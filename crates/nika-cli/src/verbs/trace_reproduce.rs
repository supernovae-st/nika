//! `nika trace reproduce` — is this run reproducible? (Proof Arc P3)
//!
//! Compares a RECORDED journal against a FRESH one of the same
//! workflow and classifies every recorded task by WHY it did or did
//! not reproduce — the ADR-099 stamps make the taxonomy total (the
//! eight verdict classes live with the comparison in the forensics
//! crate: `nika_dap::reproduce` · the 2026-07-09 W0 trace descent).
//! This file is the SHIM (the `trace_verify` pattern): the file
//! plumbing, the renderer and the exit mapping.
//!
//! Exit codes: 0 = every comparable task reproduced · 2 (FILE) = any
//! divergence. Unverifiable tasks never fail the verdict — stated,
//! never guessed. Both journals get a chain walk first (P2: verify
//! before trust — broken is WARNED, never blocked).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use nika_dap::reproduce::{Report, Verdict};

use super::VerbOutput;

#[must_use]
pub fn reproduce(recorded: &str, fresh: &str) -> VerbOutput {
    // The plumbing (read · recover · the identity guard · the honesty
    // warnings · the compare) lives in the forensics crate
    // (nika_dap::reproduce::compare_journals — the 15k descent); this
    // shim keeps the renderer and the exit mapping.
    let compared = match nika_dap::reproduce::compare_journals(recorded, fresh) {
        Ok(compared) => compared,
        Err(nika_dap::reproduce::CompareRefusal::Unreadable(msg)) => {
            return VerbOutput::env(msg);
        }
        Err(nika_dap::reproduce::CompareRefusal::DifferentWorkflows { recorded, fresh }) => {
            return VerbOutput::env(format!(
                "the two journals record DIFFERENT workflows — recorded `{recorded}` vs fresh `{fresh}`: nothing to compare (reproduce pairs runs of the same workflow)"
            ));
        }
        // The refusal is #[non_exhaustive] — a newer class is an era
        // answer, never a guessed divergence.
        Err(_) => {
            return VerbOutput::env(
                "the reproduce path cannot classify this pair — the forensics library is newer than this CLI"
                    .to_owned(),
            );
        }
    };
    let mut out = String::new();
    for warning in &compared.warnings {
        use std::fmt::Write as _;
        let _ = writeln!(out, "{warning}");
    }
    let report = &compared.report;
    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{}", render(report)));
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
            Verdict::StatusChanged {
                recorded, fresh, ..
            } => ("STATUS-CHANGED", format!(" — {recorded} → {fresh}")),
            Verdict::Unverifiable => ("unverifiable", String::new()),
            Verdict::Missing => ("MISSING", " — absent from the fresh run".to_owned()),
            Verdict::Added => ("ADDED", " — absent from the recorded run".to_owned()),
            // The verdict is #[non_exhaustive]: a NEWER forensics crate
            // may learn classes this CLI cannot name — say so, never
            // mis-file one (it still counts as divergence dap-side).
            _ => (
                "UNKNOWN",
                " — a verdict class newer than this CLI".to_owned(),
            ),
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

#[cfg(test)]
mod tests {
    use nika_dap::reproduce::compare;
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};

    use super::*;
    use crate::demo;

    fn completed(task: &str, def: &str, input: &str, output: &str, ms: u64) -> nika_event::Event {
        let s = |v: &str| Value::String(v.to_owned());
        demo::bare_event(EventKind::TaskCompleted, ms)
            .with_field(KeyValue::new("task", s(task)))
            .with_field(KeyValue::new("def_hash", s(def)))
            .with_field(KeyValue::new("input_hash", s(input)))
            .with_field(KeyValue::new("output", s(output)))
    }

    /// The renderer's contract: one row per verdict with the taxonomy's
    /// words, the counted banner, and adversarial task ids SANITIZED
    /// (a journal field must never drive the operator's terminal).
    #[test]
    fn render_names_the_taxonomy_counts_the_banner_and_sanitizes() {
        let recorded = vec![
            completed("same", "d1", "i1", "\"out\"", 0),
            completed("fla\u{1b}[31mky", "d2", "i2", "\"a\"", 10),
        ];
        let fresh = vec![
            completed("same", "d1", "i1", "\"out\"", 20),
            completed("fla\u{1b}[31mky", "d2", "i2", "\"b\"", 30),
        ];
        let text = render(&compare(&recorded, &fresh));
        assert!(
            text.contains("reproduced       same"),
            "the calm row: {text}"
        );
        assert!(
            text.contains("NONDETERMINISTIC fla[31mky — same def, same inputs, different output"),
            "the loud row, escape-stripped: {text}"
        );
        assert!(
            !text.contains('\u{1b}'),
            "no raw escapes reach the terminal"
        );
        assert!(
            text.contains("DIVERGED — 1 NONDETERMINISTIC · 1 reproduced"),
            "the counted banner: {text}"
        );
    }

    /// The all-unverifiable banner refuses the overclaim and the
    /// engine attestation line rides when both sides attest.
    #[test]
    fn render_refuses_the_overclaim_and_speaks_the_attestations() {
        let started = |ms: u64, version: &str| {
            demo::bare_event(EventKind::WorkflowStarted, ms)
                .with_field(KeyValue::new("workflow", Value::String("w".to_owned())))
                .with_field(KeyValue::new(
                    "engine_version",
                    Value::String(version.to_owned()),
                ))
                .with_field(KeyValue::new(
                    "platform",
                    Value::String("macos/aarch64".to_owned()),
                ))
        };
        let bare = |ms: u64| {
            demo::bare_event(EventKind::TaskCompleted, ms)
                .with_field(KeyValue::new("task", Value::String("a".to_owned())))
        };
        let recorded = vec![started(0, "0.95.0"), bare(10)];
        let fresh = vec![started(20, "0.96.0"), bare(30)];
        let text = render(&compare(&recorded, &fresh));
        assert!(
            text.contains("NOTHING VERIFIED — no comparable stamps"),
            "{text}"
        );
        assert!(
            text.contains(
                "engines differ: recorded 0.95.0/macos/aarch64 · fresh 0.96.0/macos/aarch64"
            ),
            "{text}"
        );
    }
}
