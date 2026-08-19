// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This suite's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// ascii_contract.rs / bin_smoke.rs.
#![allow(clippy::disallowed_types)]

//! `nika arm fire <label>` end-to-end (W2 · LE TIREUR): a tempdir
//! project (a `nika.yaml` registry + a `workflows/` shelf), the real
//! binary, and the injected clock (`--now`, D5) making every branch
//! deterministic. D8 is pinned on EVERY branch: exactly one stdout
//! line, whatever happened.
//!
//! The workflows are `exec: { shell: "true" }` (exit 0, zero provider,
//! zero Keychain) and a default-less `nika:prompt` gate for the pause
//! (stdin is /dev/null — the terminal ask never fires, the run parks).

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
    // A pause must PARK, never ask (the TTY ask would block a developer
    // machine): stdin stays closed, so the gate goes durable.
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// A tempdir project: the registry + the workflow shelf.
fn project(tag: &str, registry: &str, workflows: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-arm-fire-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("workflows")).expect("workflows dir");
    let mut f = std::fs::File::create(dir.join("nika.yaml")).expect("registry file");
    f.write_all(registry.as_bytes()).expect("registry body");
    for (name, body) in workflows {
        std::fs::write(dir.join("workflows").join(name), body).expect("workflow file");
    }
    dir
}

/// The trivial beat — exits 0, no provider, no key.
const TRUE: &str =
    "nika: armed-true\npermits: { exec: true }\ntasks:\n  ok:\n    exec: { shell: \"true\" }\n";

/// The gated beat — a default-less `nika:prompt` pauses a
/// non-interactive run (exit 4).
const GATED: &str = r#"
nika: armed-gate
permits: { tools: ["nika:prompt"] }
tasks:
  approve:
    invoke:
      tool: "nika:prompt"
      args: { mode: "input", message: "ship it?" }
"#;

/// Daily 03:00 UTC, skip the misses.
const DAILY_3AM: &str = concat!(
    "nika: v1\n",
    "arm:\n",
    "  - workflow: workflows/doctor.nika.yaml\n",
    "    cadence: \"TZ=UTC 0 3 * * *\"\n",
    "    plafond: 0.05\n",
    "    manqué: sauter\n",
);

/// One beat's parsed `last.json` (`None` when absent).
fn last_json(dir: &std::path::Path, label: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join(".nika/arm").join(label).join("last.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The history's raw text (`""` when absent).
fn history(dir: &std::path::Path, label: &str) -> String {
    std::fs::read_to_string(dir.join(".nika/arm").join(label).join("history.ndjson"))
        .unwrap_or_default()
}

/// A pre-seeded last.json (a beat that fired a past slot).
fn seed_last(dir: &std::path::Path, label: &str, slot: &str) {
    let dir = dir.join(".nika/arm").join(label);
    std::fs::create_dir_all(&dir).expect("sidecar dir");
    std::fs::write(
        dir.join("last.json"),
        format!(
            "{{\"slot\":\"{slot}\",\"fired_at\":\"{slot}\",\"trace\":null,\"exit\":0,\"kind\":\"fired\"}}\n"
        ),
    )
    .expect("seed last.json");
}

/// A lock held by a LIVE owner (this test process).
fn seed_lock(dir: &std::path::Path, label: &str) {
    let dir = dir.join(".nika/arm").join(label);
    std::fs::create_dir_all(&dir).expect("sidecar dir");
    std::fs::write(
        dir.join("lock"),
        format!(
            "{{\"pid\":{},\"started_at\":\"2026-08-19T03:00:00Z\"}}\n",
            std::process::id()
        ),
    )
    .expect("seed lock");
}

/// The traces under the project.
fn traces(dir: &std::path::Path) -> Vec<String> {
    let path = dir.join(".nika/traces");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return Vec::new();
    };
    entries
        .filter_map(std::result::Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".ndjson"))
        .collect()
}

/// D8: stdout is EXACTLY one line, always.
fn assert_one_line(what: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&what.stdout);
    assert_eq!(
        stdout.lines().count(),
        1,
        "D8 — exactly one stdout line, got: «{stdout}» (stderr: {})",
        String::from_utf8_lossy(&what.stderr)
    );
    stdout.lines().next().expect("the one line").to_owned()
}

#[test]
fn fire_runs_a_due_beat_and_records_it() {
    let dir = project("due", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = assert_one_line(&out);
    assert!(
        line.starts_with("fired doctor · slot 2026-08-19T03:00:00Z · exit 0 · trace .nika/traces/"),
        "{line}"
    );
    // The record: last.json fired · exit 0 · the slot — and the trace
    // the line cites really exists (law: every fire leaves one).
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "fired");
    assert_eq!(last["exit"], 0);
    assert_eq!(last["slot"], "2026-08-19T03:00:00Z");
    let trace = last["trace"].as_str().expect("a trace path");
    assert!(dir.join(trace).exists(), "{trace}");
    assert_eq!(history(&dir, "doctor").lines().count(), 1);
    assert_eq!(traces(&dir).len(), 1, "one fresh run = one trace (N2)");
}

#[test]
fn fire_skips_a_missed_slot_when_manque_is_sauter() {
    let dir = project("missed", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    seed_last(&dir, "doctor", "2026-08-18T03:00:00Z");
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T10:00:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(0));
    let line = assert_one_line(&out);
    assert!(
        line.starts_with("skipped doctor · missed:1 · slot 2026-08-19T03:00:00Z"),
        "{line}"
    );
    // The skip CONSUMES the slot: last.json moves to it, kind skipped.
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "skipped");
    assert_eq!(last["slot"], "2026-08-19T03:00:00Z");
    assert_eq!(history(&dir, "doctor").lines().count(), 1);
    // … and nothing ran (no trace, the workflow never went).
    assert!(traces(&dir).is_empty());
}

#[test]
fn fire_refuses_an_unknown_label_and_names_the_known_ones() {
    let registry = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "  - workflow: workflows/nightly.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 4 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
    );
    let dir = project(
        "unknown",
        registry,
        &[("doctor.nika.yaml", TRUE), ("nightly.nika.yaml", TRUE)],
    );
    let out = bin()
        .args(["arm", "fire", "bogus", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(2));
    let line = assert_one_line(&out);
    assert!(line.contains("unknown beat `bogus`"), "{line}");
    assert!(line.contains("doctor"), "the known labels: {line}");
    assert!(line.contains("nightly"), "the known labels: {line}");
}

#[test]
fn fire_skips_when_the_lock_is_held_by_a_living_owner() {
    let dir = project("locked", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    seed_lock(&dir, "doctor");
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(0));
    let line = assert_one_line(&out);
    assert!(
        line.starts_with("skipped doctor · overlap · pid "),
        "{line}"
    );
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "skipped");
    // Law ⑥ sauter: the running tick keeps its lock, nothing ran.
    assert!(dir.join(".nika/arm/doctor/lock").exists());
    assert!(traces(&dir).is_empty());
}

#[test]
fn fire_with_file_policy_times_out_at_the_next_slot() {
    let registry = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC * * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "    chevauchement: file\n",
    );
    let dir = project("queue", registry, &[("doctor.nika.yaml", TRUE)]);
    seed_lock(&dir, "doctor");
    // 03:02:59.9 — the 03:02 slot is 59.9s old (on time), the next one
    // lands in 100ms: the queue waits the 100ms, then gives up.
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:59.900Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(0));
    let line = assert_one_line(&out);
    assert!(
        line.starts_with("skipped doctor · overlap-timeout"),
        "{line}"
    );
    assert!(traces(&dir).is_empty(), "the queue never ran");
}

#[test]
fn fire_prints_exactly_one_stdout_line() {
    // The cheap branches, each pinning D8 (the run-bearing branches
    // assert the same in their own tests).
    let dir = project("oneline", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);

    // not-due: no state, the window long gone (N2 invents no backlog).
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T10:00:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(0));
    let line = assert_one_line(&out);
    assert!(line.starts_with("skipped doctor · not-due"), "{line}");
    assert!(last_json(&dir, "doctor").is_none(), "N2 writes nothing");

    // refusal: a bad --now teaches, one line, exit 2.
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "demain"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(2));
    assert_one_line(&out);
}

#[test]
fn fire_refuses_the_v0_unsupported_policies_with_teaching() {
    let registry = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "    chevauchement: remplacer\n",
    );
    let dir = project("refuse", registry, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(2));
    let line = assert_one_line(&out);
    assert!(line.contains("chevauchement: remplacer"), "{line}");
    assert!(line.contains("serve v0.2"), "names the version: {line}");
    assert!(traces(&dir).is_empty(), "a refusal never runs");
}

#[test]
fn a_paused_run_is_parked_never_answered() {
    let dir = project("paused", DAILY_3AM, &[("doctor.nika.yaml", GATED)]);
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(
        out.status.code(),
        Some(4),
        "the gate parks the run · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = assert_one_line(&out);
    assert!(
        line.starts_with("paused doctor · slot 2026-08-19T03:00:00Z"),
        "{line}"
    );
    assert!(
        line.contains("trace .nika/traces/"),
        "the trace is cited: {line}"
    );
    assert!(line.contains("garé"), "parked, never resumed: {line}");
    // Law 4 (N2): the park is recorded with its trace — and nothing
    // answered the gate (the run went ONCE, exit 4).
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "paused");
    assert_eq!(last["exit"], 4);
    let trace = last["trace"].as_str().expect("the parked trace");
    let body = std::fs::read_to_string(dir.join(trace)).expect("trace body");
    assert!(body.contains("workflow_paused"), "the pause is journaled");
}

#[test]
fn rattraper_une_fois_fires_one_run_for_the_whole_silence() {
    let registry = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: rattraper-une-fois\n",
    );
    let dir = project("catchup", registry, &[("doctor.nika.yaml", TRUE)]);
    seed_last(&dir, "doctor", "2026-08-17T03:00:00Z");
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(out.status.code(), Some(0));
    let line = assert_one_line(&out);
    assert!(
        line.starts_with("fired doctor · slot 2026-08-19T03:00:00Z · rattrapage ×2"),
        "{line}"
    );
    let hist = history(&dir, "doctor");
    assert!(hist.contains("\"slots\":2"), "the silence's count: {hist}");
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "fired");
    assert_eq!(last["slot"], "2026-08-19T03:00:00Z");
}
