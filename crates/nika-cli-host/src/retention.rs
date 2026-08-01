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

pub use nika_dap::retention::{GcReport, Reason, RetentionConfig, collect, newest_per_workflow};

/// The EXACTLY-ONE stderr line (D2):
/// `trace gc · removed 3 (2 aged · 1 over-budget) · kept 12 · 41MB` —
/// the counts come from the policy; the bytes vocabulary is the
/// display fold's.
#[must_use]
pub fn gc_line(report: &GcReport) -> String {
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
#[must_use]
pub fn gc_at_run_start(dir: &Path, no_gc: bool, dry_run: bool) -> Option<String> {
    if no_gc || dry_run || !dir.is_dir() {
        return None;
    }
    let (cfg, _notes) = RetentionConfig::from_env();
    collect(dir, &cfg, std::time::SystemTime::now()).map(|report| gc_line(&report))
}

/// Bytes for the line + summaries (`41MB`) — the display fold's own
/// formatter, u64-widened at this one seam.
#[must_use]
pub fn fmt_bytes(bytes: u64) -> String {
    crate::display::shape::fmt_bytes(usize::try_from(bytes).unwrap_or(usize::MAX))
}
