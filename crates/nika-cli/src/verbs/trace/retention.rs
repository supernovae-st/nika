// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trace-retention seam (ADR-100) — the policy (the three knobs ·
//! the pure plan · the fail-open collection) lives in the forensics
//! crate (`nika_dap::retention` · the 2026-07-09 W0 trace descent),
//! re-exported here so every `trace::retention::` consumer reads
//! unchanged (the `trace_verify` pattern). This file keeps the RENDER
//! half: the bytes vocabulary (the display fold's formatter), the
//! exactly-one stderr line a successful collection speaks (D2 — silent
//! deletion is forbidden), and the `nika run` hook.

use std::path::Path;

pub(crate) use nika_dap::retention::{
    GcReport, Reason, RetentionConfig, collect, newest_per_workflow,
};

/// The EXACTLY-ONE stderr line (D2):
/// `trace gc · removed 3 (2 aged · 1 over-budget) · kept 12 · 41MB` —
/// the counts come from the policy; the bytes vocabulary is the
/// display fold's.
pub(crate) fn gc_line(report: &GcReport) -> String {
    let removed = report.rotated + report.aged + report.over_budget;
    let mut groups = Vec::new();
    for (n, reason) in [
        (report.rotated, Reason::Rotated),
        (report.aged, Reason::Aged),
        (report.over_budget, Reason::OverBudget),
    ] {
        if n > 0 {
            groups.push(format!("{n} {}", reason.as_str()));
        }
    }
    format!(
        "trace gc · removed {removed} ({}) · kept {} · {}",
        groups.join(" · "),
        report.kept,
        fmt_bytes(report.kept_bytes),
    )
}

/// The `nika run` hook (D2): opportunistic collection before the run.
/// `--no-gc` skips it; `--dry-run` never collects (plan only · ZERO
/// effects); a missing dir is a no-op. Returns the ONE line to print
/// on stderr when anything was removed.
pub(crate) fn gc_at_run_start(dir: &Path, no_gc: bool, dry_run: bool) -> Option<String> {
    if no_gc || dry_run || !dir.is_dir() {
        return None;
    }
    let (cfg, _notes) = RetentionConfig::from_env();
    collect(dir, &cfg, std::time::SystemTime::now()).map(|report| gc_line(&report))
}

/// Bytes for the line + summaries (`41MB`) — the display fold's own
/// formatter, u64-widened at this one seam.
pub(crate) fn fmt_bytes(bytes: u64) -> String {
    crate::display::shape::fmt_bytes(usize::try_from(bytes).unwrap_or(usize::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::trace::store::tests::{ndjson, run_events, stage_trace, temp_store};
    use nika_event::EventKind;
    use std::time::Duration;

    /// The D2 line: one line, reasons grouped in a stable order, the
    /// display fold's bytes vocabulary.
    #[test]
    fn the_gc_line_groups_reasons_in_the_d2_form() {
        let line = gc_line(&GcReport::new(1, 2, 1, 12, 41_000_000));
        assert!(!line.contains('\n'), "EXACTLY one line: {line:?}");
        assert_eq!(
            line,
            "trace gc · removed 4 (1 rotated · 2 aged · 1 over-budget) · kept 12 · 41.0MB"
        );
        // A single-reason pass names only what fired.
        assert_eq!(
            gc_line(&GcReport::new(2, 0, 0, 10, 999)),
            "trace gc · removed 2 (2 rotated) · kept 10 · 999B"
        );
    }

    /// Fixture 3's `--no-gc` leg (+ the dry-run law): the run hook
    /// leaves the store INTACT and says nothing.
    #[test]
    fn fixture_no_gc_and_dry_run_leave_the_store_intact() {
        let dir = temp_store("no-gc");
        let body = ndjson(&run_events("veille", Some(EventKind::WorkflowCompleted)));
        for i in 0..12u64 {
            stage_trace(
                &dir,
                &format!("run-{i:02}.ndjson"),
                &body,
                Duration::from_secs(i * 60),
            );
        }
        assert_eq!(gc_at_run_start(&dir, true, false), None, "--no-gc skips");
        assert_eq!(gc_at_run_start(&dir, false, true), None, "--dry-run skips");
        for i in 0..12u64 {
            assert!(
                dir.join(format!("run-{i:02}.ndjson")).exists(),
                "run-{i} intact"
            );
        }
        // A missing dir is a no-op, never an error.
        assert_eq!(
            gc_at_run_start(Path::new("/nonexistent/traces"), false, false),
            None
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
