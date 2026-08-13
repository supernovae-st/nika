// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite's WHOLE JOB is to execute the real `nika-cli` binary
// (CARGO_BIN_EXE) — the bin_smoke carve-out class.
#![allow(clippy::disallowed_types)]

//! The ENERGY rung, end to end (NEP-0018 · nika-spec `governance/
//! nep-0018-energy-honesty.md`) — cost honesty transposed to watt-hours,
//! proven at the shipped surface:
//!
//! - a capped task on a MEASURED model renders a `≤ … Wh` worst-case
//!   OUTPUT ceiling with both axes named (provenance · scope) — two
//!   honest numbers stay comparable;
//! - a capped task on an unmeasured model renders `unpriced` — and the
//!   string `0.0 Wh` appears NOWHERE (a zero would claim free
//!   inference · « unknown stays unknown »);
//! - a LOCAL model says whose watts they are;
//! - an uncapped task yields NO total ceiling, counted at ENERGY and
//!   named at COST (one voice, no double list).
//!
//! The measured fixture asserts SHAPE, never the figure: energy rows
//! rot with hardware and runtime generations (the catalog pins each to
//! a source + month), so a hardcoded Wh here would fail on every honest
//! refresh.

use std::io::Write as _;
use std::process::Command;

fn check(yaml: &str) -> (i32, String) {
    // Tests run in-parallel in ONE process: the dir must be unique per
    // CALL, not per pid, or the fixtures clobber each other mid-test.
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp");
    let dir = base.join(format!("energy-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("wf.nika.yaml");
    let mut f = std::fs::File::create(&path).expect("fixture file");
    f.write_all(yaml.as_bytes()).expect("fixture body");
    let out = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .current_dir(&dir)
        .args(["check", path.to_str().expect("utf8")])
        .output()
        .expect("binary runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

/// The one row the catalog has carried since the first sourced figure
/// entered (2026-07-29 · ml.energy) — the measured-model fixture.
const MEASURED_MODEL: &str = "groq/qwen/qwen3-32b";

fn wf(model: &str, max_tokens: Option<u32>) -> String {
    let cap = max_tokens.map_or(String::new(), |n| format!(", max_tokens: {n}"));
    format!(
        "nika: e\nmodel: {model}\ntasks:\n  \
         brief:\n    infer: {{ prompt: \"hi\"{cap} }}\n"
    )
}

#[test]
fn measured_model_renders_a_ceiling_with_both_axes() {
    let (code, text) = check(&wf(MEASURED_MODEL, Some(1000)));
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("ENERGY") && text.contains("Wh worst-case OUTPUT ceiling"),
        "the headline claims a ceiling:\n{text}"
    );
    assert!(
        text.contains("1 of 1 tasks measured"),
        "the narrowing is counted:\n{text}"
    );
    // Both axes ride the row — without them two truthful numbers are
    // silently incomparable (gpu ≈ half of fleet for the same model).
    let row = text
        .lines()
        .find(|l| l.contains(MEASURED_MODEL) && l.contains("Wh"))
        .expect("a measured per-task row");
    for axis in ["measured", "gpu", "≤"] {
        assert!(row.contains(axis), "axis `{axis}` missing: {row}");
    }
}

#[test]
fn unmeasured_model_is_unpriced_and_zero_appears_nowhere() {
    let (code, text) = check(&wf("mock/echo", Some(1000)));
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("ENERGY") && text.contains("unpriced"),
        "no figure → unpriced, stated:\n{text}"
    );
    // The banned forms are the CLAIM shapes (`≤ 0 Wh` · a zero total),
    // not the words — the rung's own teaching line says « never 0 Wh »
    // and must keep saying it.
    for lie in ["0.0 Wh", "0.000 Wh", "≤ 0 Wh"] {
        assert!(
            !text.contains(lie),
            "`{lie}` is the free-inference lie:\n{text}"
        );
    }
}

#[test]
fn local_model_names_whose_watts() {
    let (code, text) = check(&wf("ollama/qwen3.5:4b", Some(800)));
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("a local model draws your watts"),
        "local is unpriced, never free:\n{text}"
    );
}

#[test]
fn uncapped_task_yields_no_total_ceiling() {
    let (code, text) = check(&wf(MEASURED_MODEL, None));
    assert_eq!(code, 0, "unbounded energy is a warning posture:\n{text}");
    assert!(
        text.contains("no total energy ceiling") && text.contains("1 uncapped"),
        "no cap → no claimed total, counted:\n{text}"
    );
    assert!(
        !text.contains("Wh worst-case OUTPUT ceiling"),
        "an unbounded run must not print a total ceiling:\n{text}"
    );
}

/// The one-voice pin, from the probe that found the defect: a
/// `for_each` over a literal EMPTY collection provably never executes,
/// so there is nothing to bound. Before the repair an
/// `iterations.max(1)` guard invented one iteration and this rung
/// printed `≤ 0.087 Wh` for a task COST priced at `$0.0000` — two
/// adjacent rungs disagreeing about the same task.
#[test]
fn an_empty_for_each_claims_no_energy_and_agrees_with_cost() {
    let (code, text) = check(
        "nika: zero\nmodel: groq/qwen/qwen3-32b\n\
         const:\n  nothing: []\ntasks:\n  brief:\n    \
         for_each: { items: \"${{ const.nothing }}\" }\n    \
         infer: { prompt: \"hi\", max_tokens: 1000 }\n",
    );
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("no task can run (empty for_each)"),
        "a provable zero is stated, never priced:\n{text}"
    );
    assert!(
        !text.contains("Wh worst-case OUTPUT ceiling"),
        "a task that never runs must not carry a ceiling:\n{text}"
    );
    // One voice: COST prices the same task at zero, and neither rung
    // invents a number the other denies.
    assert!(
        text.contains("$0.0000"),
        "the COST rung's own zero rides beside it:\n{text}"
    );
}

#[test]
fn no_inference_tasks_renders_no_energy_rung() {
    let (code, text) = check(
        "nika: x\npermits: { exec: [\"echo\"] }\n\
         tasks:\n  probe:\n    exec: { command: [\"echo\", \"hi\"] }\n",
    );
    assert_eq!(code, 0, "{text}");
    assert!(
        !text.contains("ENERGY"),
        "the ladder says so at COST already:\n{text}"
    );
}
