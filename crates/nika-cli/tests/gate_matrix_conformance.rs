// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as bin_smoke: this suite's WHOLE JOB is the real binary,
// and NIKA_SPEC_DIR is harness plumbing (same read as the schema tiers).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

//! The gate-v2 observation matrix, LIVE — every spec fixture under
//! `conformance/tests/runtime/gates/**` runs at the REAL binary and its
//! observed statuses must match `expected-run.json` (whose statuses were
//! authored by the reference model — spec `scripts/gen-gate-matrix.py`).
//!
//! This is the engine half of the runtime conformance tier for the
//! gates area: the model defines (35 matrix cells + the always-pattern
//! fixture) · the engine follows · this suite is the ratchet that keeps
//! them agreeing on every commit. The spec dir resolves like the static
//! tiers (`$NIKA_SPEC_DIR` or the sibling checkout) and the suite
//! HARD-FAILS when missing — a silently-skipped conformance gate is the
//! guard-blind class.

use std::path::{Path, PathBuf};
use std::process::Command;

fn spec_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    // sibling checkout — <engine>/../spec (same fallback as nika-schema's
    // conformance harness)
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .parent()
        .expect("engine parent")
        .join("spec")
}

/// Observed terminal statuses from one real run (`run --json` event
/// stream — the same projection reference/differential.py reads) plus
/// the run's verdict bit.
fn run_observed(workflow: &Path) -> (std::collections::BTreeMap<String, String>, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_nika"))
        .arg("run")
        .arg(workflow)
        .arg("--json")
        .output()
        .expect("binary runs");
    let mut statuses = std::collections::BTreeMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let status = match ev["kind"].as_str() {
            Some("task_completed") => "success",
            Some("task_failed") => "failure",
            Some("task_skipped") => "skipped",
            Some("task_cancelled") => "cancelled",
            _ => continue,
        };
        let Some(fields) = ev["fields"].as_array() else {
            continue;
        };
        let task = fields
            .iter()
            .find(|f| f["key"] == "task")
            .and_then(|f| f["value"].as_str());
        if let Some(task) = task {
            statuses.insert(task.to_owned(), status.to_owned());
        }
        // an output assert rides the same event (task_completed only)
        if status == "success"
            && let Some(task) = fields
                .iter()
                .find(|f| f["key"] == "task")
                .and_then(|f| f["value"].as_str())
            && let Some(output) = fields
                .iter()
                .find(|f| f["key"] == "output")
                .and_then(|f| f["value"].as_str())
        {
            statuses.insert(format!("{task}\u{0}output"), output.to_owned());
        }
    }
    (statuses, out.status.success())
}

#[test]
fn gate_matrix_cells_match_the_model_authored_expectations() {
    let gates = spec_dir().join("conformance/tests/runtime/gates");
    assert!(
        gates.is_dir(),
        "runtime gates tier missing: {} — set NIKA_SPEC_DIR",
        gates.display()
    );

    let mut total = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&gates)
        .expect("read gates dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("input.nika.yaml").is_file())
        .collect();
    dirs.sort();

    for dir in dirs {
        total += 1;
        let name = dir
            .file_name()
            .expect("dir name")
            .to_string_lossy()
            .to_string();
        let input = dir.join("input.nika.yaml");
        let (observed, run_ok) = run_observed(&input);

        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("expected-run.json")).expect("expected-run.json"),
        )
        .expect("expected parses");

        // workflow verdict · rc==0 ⟺ expected workflow_state == success
        let want_ok = expected["workflow_state"] == "success";
        if run_ok != want_ok {
            failures.push(format!(
                "{name}: workflow verdict — expected {} · binary rc says {}",
                expected["workflow_state"], run_ok
            ));
        }
        // per-task terminal statuses (the model's half of the contract)
        for (task, spec) in expected["tasks"].as_object().expect("tasks map") {
            let want = spec["status"].as_str().expect("status");
            match observed.get(task) {
                Some(got) if got == want => {}
                got => failures.push(format!(
                    "{name}: task `{task}` — expected {want} · observed {got:?}"
                )),
            }
            if let Some(needle) = spec["output_contains"].as_str() {
                let key = format!("{task}\u{0}output");
                let hit = observed.get(&key).is_some_and(|o| o.contains(needle));
                if !hit {
                    failures.push(format!(
                        "{name}: task `{task}` output missing `{needle}` (got {:?})",
                        observed.get(&key)
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} matrix cells diverged ·\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The matrix floor: 35 cells + the always-pattern fixture.
    assert!(total >= 36, "only {total} cells walked — layout drift?");
}
