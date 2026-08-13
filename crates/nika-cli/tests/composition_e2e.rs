// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite's WHOLE JOB is to execute the real `nika-cli` binary
// (CARGO_BIN_EXE) — the bin_smoke carve-out class.
#![allow(clippy::disallowed_types)]

//! Composition end-to-end — REAL child execution through the REAL
//! binary (spec `14-composition.md` · the wave's demonstration):
//!
//! - a parent `invoke: workflow:` runs the child as a NESTED run over
//!   real files + a real subprocess; the child's typed `outputs:`
//!   remount as the parent task's value (law 2, both halves)
//! - the trace FOREST exists on disk: two journal files, two hash
//!   chains, each intact (law 8) — and the PARENT's chained frame
//!   embeds the CHILD's chain head, so the parent's receipt commits to
//!   the child's (law 9 · verified with an independent re-walk, not the
//!   engine's own verifier)
//! - a static cycle is refused at check (`NIKA-COMP-003` · law 7)
//! - a child effect outside the parent's declared boundary is refused
//!   at check (`NIKA-COMP-002` · laws 3/4)
//! - a templated target is refused at check (`NIKA-COMP-001` · law 1)
//! - a missing required child input is refused at check
//!   (`NIKA-COMP-004` · law 2)
//! - an ACYCLIC chain deeper than the run-recursion bound fails CLOSED
//!   at run (`NIKA-SEC-003` — the backstop static acyclicity cannot
//!   cover, demonstrated on real nesting)
//! - the parent task's `timeout:` bounds the child run (law 6, time
//!   half — the child future is dropped at the deadline)

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

fn write_fixture(dir: &std::path::Path, name: &str, yaml: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("fixture file");
    f.write_all(yaml.as_bytes()).expect("fixture body");
    path
}

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp");
    let dir = base.join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

/// Run the binary in `dir` and return (exit code, stdout+stderr).
fn run_in(dir: &std::path::Path, args: &[&str]) -> (i32, String) {
    let out = bin()
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binary runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

const CHILD: &str = r#"
nika: greet-child
inputs:
  name: { type: string, required: true }
permits: { exec: ["echo"] }
tasks:
  greet:
    exec: { command: ["echo", "hello ${{ inputs.name }}"] }
outputs:
  greeting: { value: "${{ tasks.greet.output }}", type: string }
"#;

const PARENT: &str = r#"
nika: greet-parent
permits: { exec: ["echo"] }
tasks:
  call:
    invoke:
      workflow: "./child.nika.yaml"
      args: { name: "composition" }
    returns: { object: { greeting: string } }
outputs:
  echoed: { value: "${{ tasks.call.output.greeting }}", type: string }
"#;

// ─── the real-run demonstrations ─────────────────────────────────────────

/// LAW 2 (both halves) on a REAL nested run: the parent calls the child
/// (real file · real `echo` subprocess); the child's typed outputs
/// remount as the parent task's value and flow into the parent's own
/// `outputs:`.
#[test]
fn child_runs_for_real_and_typed_outputs_remount() {
    let dir = tmp_dir("comp-real-run");
    write_fixture(&dir, "child.nika.yaml", CHILD);
    let parent = write_fixture(&dir, "parent.nika.yaml", PARENT);
    let (code, text) = run_in(
        &dir,
        &["run", parent.to_str().expect("utf8"), "--output", "json"],
    );
    assert_eq!(code, 0, "the composed run settles green:\n{text}");
    // The parent's own outputs carry the CHILD-produced value.
    let json_line = text
        .lines()
        .find(|l| l.trim_start().starts_with('{'))
        .expect("--output json emits the outputs object");
    let v: serde_json::Value = serde_json::from_str(json_line.trim()).expect("outputs JSON");
    assert_eq!(
        v["echoed"], "hello composition",
        "child echo → child typed output → parent task value → parent output:\n{text}"
    );
}

// Independent chain walk: line N's `chain` field = sha256 hex of
// line N-1's exact bytes (genesis: sha256 of b"nika-trace-v1").
// Wire shape: `fields` is an array of `{key, value}` pairs.
fn wire_field(event: &serde_json::Value, key: &str) -> Option<String> {
    event["fields"].as_array()?.iter().find_map(|f| {
        (f["key"] == key).then(|| match &f["value"] {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    })
}
fn walk(path: &std::path::Path) -> (String, String, Vec<serde_json::Value>) {
    use sha2::{Digest as _, Sha256};
    let raw = std::fs::read_to_string(path).expect("journal reads");
    let mut prev = format!("{:x}", Sha256::digest(b"nika-trace-v1"));
    let mut events = Vec::new();
    let mut wf_id = String::new();
    for line in raw.lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("journal line is JSON");
        assert_eq!(
            v["chain"].as_str().expect("chain field"),
            prev,
            "chain intact at every line of {}",
            path.display()
        );
        prev = format!("{:x}", Sha256::digest(line.as_bytes()));
        if let Some(id) = wire_field(&v, "workflow") {
            wf_id = id;
        }
        events.push(v);
    }
    (prev, wf_id, events)
}

/// LAWS 8 + 9 on disk: two journal files (the FOREST — the child keeps
/// its OWN chain), each chain intact under an INDEPENDENT re-walk (this
/// test's own sha256, not the engine's verifier), and the parent's
/// hash-chained `task_completed` frame embeds the child's chain HEAD —
/// the Merkle commitment: tamper with the child's journal and the
/// parent's committed head no longer matches.
#[test]
fn trace_forest_two_chains_and_the_parent_commits_to_the_child() {
    let dir = tmp_dir("comp-forest");
    write_fixture(&dir, "child.nika.yaml", CHILD);
    let parent = write_fixture(&dir, "parent.nika.yaml", PARENT);
    let (code, text) = run_in(&dir, &["run", parent.to_str().expect("utf8")]);
    assert_eq!(code, 0, "{text}");

    let traces = dir.join(".nika").join("traces");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&traces)
        .expect("trace dir exists")
        .map(|e| e.expect("entry").path())
        .collect();
    files.sort();
    assert_eq!(
        files.len(),
        2,
        "a forest: parent + child journals: {files:?}"
    );

    let walked: Vec<(String, String, Vec<serde_json::Value>)> =
        files.iter().map(|p| walk(p)).collect();
    let parent_walk = walked
        .iter()
        .find(|(_, id, _)| id == "greet-parent")
        .expect("parent journal present");
    let child_walk = walked
        .iter()
        .find(|(_, id, _)| id == "greet-child")
        .expect("child journal present");

    // The parent's terminal frame for the call task carries the child row…
    let child_row = parent_walk
        .2
        .iter()
        .find_map(|e| wire_field(e, "child"))
        .expect("the parent frame records the child row");
    let row: serde_json::Value = serde_json::from_str(&child_row).expect("child row is JSON");
    assert_eq!(row["target"], "./child.nika.yaml");
    assert_eq!(row["outcome"], "success");
    // …whose chain head IS the child journal's independently-walked head
    // AT COMMIT TIME. The child journal gains its own terminal frames
    // AFTER the parent frame was cut, so the committed head must appear
    // as the `chain` field of one of the child's own lines (the head-at-
    // that-moment) — tampering with any earlier child line breaks this.
    let committed = row["chain_head"].as_str().expect("chain_head");
    let child_heads: Vec<&str> = child_walk
        .2
        .iter()
        .filter_map(|e| e["chain"].as_str())
        .collect();
    assert!(
        child_heads.contains(&committed) || committed == child_walk.0,
        "the parent's committed head is a real point of the child's chain \
         (law 9): committed {committed} · child heads {child_heads:?} · final {}",
        child_walk.0
    );
    // …and the child's definition hash rides too (the pre-W6 identity).
    assert!(
        row["def_hash"].as_str().is_some_and(|h| h.len() == 64),
        "{row}"
    );
}

/// LAW 7 at check: a two-file cycle is refused (`NIKA-COMP-003`) before
/// any run — over the REAL resolver (relative paths on disk).
#[test]
fn static_cycle_is_refused_at_check() {
    let dir = tmp_dir("comp-cycle");
    let a = r#"
nika: a
tasks:
  go:
    invoke: { workflow: "./b.nika.yaml" }
"#;
    let b = r#"
nika: b
tasks:
  back:
    invoke: { workflow: "./a.nika.yaml" }
"#;
    let pa = write_fixture(&dir, "a.nika.yaml", a);
    write_fixture(&dir, "b.nika.yaml", b);
    let (code, text) = run_in(&dir, &["check", pa.to_str().expect("utf8")]);
    assert_eq!(code, 2, "file findings:\n{text}");
    assert!(text.contains("NIKA-COMP-003"), "{text}");
    // …and `run` refuses through the SAME gate (check≡run).
    let (code, text) = run_in(&dir, &["run", pa.to_str().expect("utf8")]);
    assert_ne!(code, 0);
    assert!(text.contains("NIKA-COMP-003"), "{text}");
}

/// LAWS 3/4 at check: the child's inferred effect boundary must fit
/// inside the parent's DECLARED boundary (`NIKA-COMP-002`).
#[test]
fn child_effect_outside_the_parent_boundary_is_refused() {
    let dir = tmp_dir("comp-contain");
    let parent = r#"
nika: contained-parent
permits:
  net:
    http: ["api.example.com"]
tasks:
  call:
    invoke:
      workflow: "./child.nika.yaml"
      args: { name: "x" }
"#;
    write_fixture(&dir, "child.nika.yaml", CHILD); // child does EXEC
    let pp = write_fixture(&dir, "parent.nika.yaml", parent);
    let (code, text) = run_in(&dir, &["check", pp.to_str().expect("utf8")]);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("NIKA-COMP-002"), "{text}");
}

/// LAW 1 at check: a templated target is refused (`NIKA-COMP-001`) —
/// purely, no child file needed.
#[test]
fn templated_target_is_refused_at_check() {
    let dir = tmp_dir("comp-templated");
    let parent = r#"
nika: t
const:
  which: "a"
tasks:
  call:
    invoke: { workflow: "./sub-${{ const.which }}.nika.yaml" }
"#;
    let pp = write_fixture(&dir, "parent.nika.yaml", parent);
    let (code, text) = run_in(&dir, &["check", pp.to_str().expect("utf8")]);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("NIKA-COMP-001"), "{text}");
}

/// LAW 2 at check: a missing required child input is `NIKA-COMP-004`.
#[test]
fn missing_required_child_input_is_refused_at_check() {
    let dir = tmp_dir("comp-args");
    write_fixture(&dir, "child.nika.yaml", CHILD); // requires `name`
    let parent = r#"
nika: p
tasks:
  call:
    invoke: { workflow: "./child.nika.yaml" }
"#;
    let pp = write_fixture(&dir, "parent.nika.yaml", parent);
    let (code, text) = run_in(&dir, &["check", pp.to_str().expect("utf8")]);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("NIKA-COMP-004"), "{text}");
    assert!(text.contains("`name`"), "{text}");
}

/// The `NIKA-SEC-003` backstop on REAL nesting: a 10-deep ACYCLIC chain
/// passes the static check (no cycle to refuse) but the run refuses
/// FAIL-CLOSED at the recursion bound — the deepest refusal's code
/// propagates up the failure chain verbatim (one voice).
#[test]
fn acyclic_chain_beyond_the_depth_bound_fails_closed_at_run() {
    let dir = tmp_dir("comp-depth");
    // f0 → f1 → … → f9 → (echo)
    for i in 0..10 {
        let body = if i == 9 {
            "\
nika: f9
permits: { exec: [\"echo\"] }
tasks:
  leaf:
    exec: { command: [\"echo\", \"bottom\"] }
"
            .to_owned()
        } else {
            format!(
                "\
nika: f{i}
permits: {{ exec: [\"echo\"] }}
tasks:
  descend:
    invoke: {{ workflow: \"./f{}.nika.yaml\" }}
",
                i + 1
            )
        };
        write_fixture(&dir, &format!("f{i}.nika.yaml"), &body);
    }
    let root = dir.join("f0.nika.yaml");
    // Static check: ACYCLIC — green (the cycle law has nothing to say).
    let (code, text) = run_in(&dir, &["check", root.to_str().expect("utf8")]);
    assert_eq!(code, 0, "an acyclic chain checks clean:\n{text}");
    // Run: the 9th nesting exceeds MAX_RUN_DEPTH=8 — refused fail-closed.
    let (code, text) = run_in(&dir, &["run", root.to_str().expect("utf8")]);
    assert_eq!(code, 1, "a workflow failure, not a crash:\n{text}");
    assert!(text.contains("NIKA-SEC-003"), "{text}");
}

/// LAW 6, COST half — and law 5 (resources summed) is how it is
/// enforced: a parent carrying NO priced task of its own is refused
/// BEFORE IT STARTS because the CHILD's floor exceeds the parent's
/// `--max-cost-usd`. Block-before-spend, not a mid-run cut: nothing
/// executes, no trace is written, no provider is touched (hermetic —
/// the floor is static, so no key and no network are needed).
///
/// Three faces, because one would not distinguish the mechanisms:
/// (a) the composed refusal names a floor the parent alone cannot
/// explain; (b) the child ALONE under the same budget is refused with
/// the SAME number to the cent — so that floor is the child's own,
/// summed into the parent (law 5); (c) an UNPRICED child under the same
/// tiny budget runs green — the gate reads the FLOOR, and composition
/// never refuses spuriously.
#[test]
fn the_child_floor_bounds_the_parent_budget_before_any_token() {
    let dir = tmp_dir("comp-budget");
    let priced = "\
nika: spender
model: groq/qwen/qwen3-32b
tasks:
  think:
    infer: { prompt: \"hi\", max_tokens: 60000 }
";
    let unpriced = priced.replace("groq/qwen/qwen3-32b", "mock/echo");
    let parent_of = |child: &str| {
        format!(
            "nika: thrifty\ntasks:\n  call:\n    \
             invoke: {{ workflow: \"./{child}\" }}\n"
        )
    };
    write_fixture(&dir, "priced-child.nika.yaml", priced);
    write_fixture(&dir, "unpriced-child.nika.yaml", &unpriced);
    let p_priced = write_fixture(&dir, "p1.nika.yaml", &parent_of("priced-child.nika.yaml"));
    let p_unpriced = write_fixture(&dir, "p2.nika.yaml", &parent_of("unpriced-child.nika.yaml"));

    // (a) the composed floor refuses the run before it starts.
    let (code, text) = run_in(
        &dir,
        &[
            "run",
            p_priced.to_str().expect("utf8"),
            "--max-cost-usd",
            "0.0001",
        ],
    );
    assert_eq!(code, 2, "a refusal to START, not a failure:\n{text}");
    assert!(
        text.contains("refusing to start") && text.contains("cost floor"),
        "the refusal names the floor:\n{text}"
    );
    let floor = text
        .split("cost floor ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .expect("the floor figure")
        .to_owned();
    // Nothing ran: no journal, so not one token could have been spent.
    assert!(
        !dir.join(".nika").join("traces").exists()
            || std::fs::read_dir(dir.join(".nika").join("traces"))
                .into_iter()
                .flatten()
                .count()
                == 0,
        "block-before-spend writes no trace: {floor}"
    );

    // (b) the child ALONE, same budget → the SAME floor to the cent.
    let (code, alone) = run_in(
        &dir,
        &[
            "run",
            dir.join("priced-child.nika.yaml").to_str().expect("utf8"),
            "--max-cost-usd",
            "0.0001",
        ],
    );
    assert_eq!(code, 2, "{alone}");
    assert!(
        alone.contains(&floor),
        "the parent's floor IS the child's own (law 5 · summed): parent said \
         {floor}, child alone said:\n{alone}"
    );

    // (c) an unpriced child under the same tiny budget RUNS — the gate
    // reads the floor, and composition never refuses spuriously.
    let (code, cheap) = run_in(
        &dir,
        &[
            "run",
            p_unpriced.to_str().expect("utf8"),
            "--max-cost-usd",
            "0.0001",
        ],
    );
    assert_eq!(code, 0, "an unpriced child costs no dollars:\n{cheap}");
    assert!(
        !cheap.contains("refusing to start"),
        "no spurious refusal:\n{cheap}"
    );
}

/// LAW 6, time half, on a REAL child: the parent task's `timeout:`
/// bounds the whole nested run — the child (which would sleep 5s) is
/// dropped at the deadline and the parent settles fast.
#[test]
fn parent_timeout_bounds_the_real_child() {
    let dir = tmp_dir("comp-deadline");
    let child = r#"
nika: sleeper
permits: { exec: ["sleep"] }
tasks:
  nap:
    exec: { command: ["sleep", "5"] }
"#;
    let parent = r#"
nika: impatient
permits: { exec: ["sleep"] }
tasks:
  call:
    invoke: { workflow: "./child.nika.yaml" }
    timeout: 1s
"#;
    write_fixture(&dir, "child.nika.yaml", child);
    let pp = write_fixture(&dir, "parent.nika.yaml", parent);
    let started = std::time::Instant::now();
    let (code, text) = run_in(&dir, &["run", pp.to_str().expect("utf8")]);
    let elapsed = started.elapsed();
    assert_eq!(code, 1, "the parent settles a task failure:\n{text}");
    assert!(
        elapsed < std::time::Duration::from_secs(4),
        "the child cannot outlive its caller (law 6): took {elapsed:?}"
    );
}
