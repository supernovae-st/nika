// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tests of the retention config (an ADR-110 host-member module),
//! living cli-side where the `trace::store` test fixtures are.

use crate::trace::retention::*;
use crate::trace::store::tests::{ndjson, run_events, stage_trace, temp_store};
use nika_event::EventKind;
use std::path::Path;
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
