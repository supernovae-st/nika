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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    // A pause must PARK, never ask (the TTY ask would block a developer
    // machine): stdin stays closed, so the gate goes durable.
    cmd.stdin(std::process::Stdio::null());
    cmd.env("NIKA_KEYCHAIN", "off");
    cmd
}

#[cfg(unix)]
fn bin_with_stream_setup(setup: &str) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", setup, "nika-stdout-setup", env!("CARGO_BIN_EXE_nika")]);
    cmd.stdin(std::process::Stdio::null());
    cmd.env("NIKA_KEYCHAIN", "off");
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

const CLOSED_STDOUT: &str = "nika: closed-stdout\npermits:\n  exec: true\ntasks:\n  ok:\n    exec:\n      command: [\"true\"]\n";

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

/// The parent deliberately stays alive long enough for its declared source to
/// be replaced after the claim. Its relative child must still resolve beside
/// the original logical path while the pinned parent bytes remain authoritative.
const RELATIVE_CHILD_PARENT: &str = r#"
nika: pinned-parent
permits: { exec: true }
tasks:
  hold:
    exec: { shell: "sleep 1" }
  call:
    after: { hold: success }
    invoke: { workflow: "./child.nika.yaml" }
"#;

const RELATIVE_CHILD: &str = r#"
nika: relative-child
permits: { exec: true }
tasks:
  ok:
    exec: { shell: "true" }
"#;

const RELATIVE_SKILL: &str = r#"
nika: relative-skill
model: mock/echo
permits:
  fs:
    read: ["skill.md"]
tasks:
  answer:
    agent: { prompt: "apply the skill", skills: ["skill.md"] }
"#;

/// A dynamically-computed write target passes static audit but is refused at
/// the real fs boundary because it is outside the declared write set.
const FORBIDDEN_WRITE: &str = r#"
nika: forbidden-write
permits:
  fs:
    write: ["./allowed.txt"]
  tools: ["nika:jq", "nika:write"]
tasks:
  target:
    invoke:
      tool: "nika:jq"
      args: { input: {}, expression: '"./forbidden.txt"' }
  write:
    with: { path: "${{ tasks.target.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "${{ with.path }}", content: "must-not-land" }
"#;

const CAPTURED_WORLD_PARENT: &str = r#"
nika: captured-world-parent
model: mock/echo
permits:
  exec: true
  fs:
    read: ["skill.md"]
tasks:
  hold:
    exec: { shell: "sleep 1" }
  child:
    after: { hold: success }
    invoke: { workflow: "./child.nika.yaml" }
  review:
    after: { child: success }
    agent: { prompt: "apply captured guidance", skills: ["skill.md"] }
"#;

/// Daily 03:00 UTC, skip the misses.
const DAILY_3AM: &str = concat!(
    "nika: proj\n",
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

/// Fire a past slot through the real binary so the seed carries the
/// claim, receipt, durable head, projection, and trace of genuine truth.
fn seed_fire(dir: &std::path::Path, label: &str, now: &str) {
    let out = bin()
        .args(["arm", "fire", label, "--now", now])
        .current_dir(dir)
        .output()
        .expect("spawn seed fire");
    assert_eq!(
        out.status.code(),
        Some(0),
        "seed stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = assert_one_line(&out);
    assert!(
        line.starts_with(&format!("fired {label} · slot ")),
        "{line}"
    );
}

/// A kernel lease held by a LIVE owner (this test process).
fn seed_lock(dir: &std::path::Path, label: &str) -> nix::fcntl::Flock<std::fs::File> {
    let dir = dir.join(".nika/arm").join(label);
    std::fs::create_dir_all(&dir).expect("sidecar dir");
    let mut file = std::fs::File::create(dir.join("lock")).expect("lock file");
    writeln!(
        file,
        "{{\"pid\":{},\"started_at\":\"2026-08-19T03:00:00Z\"}}",
        std::process::id()
    )
    .expect("lock metadata");
    file.sync_all().expect("lock metadata sync");
    nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .map_err(|(_, error)| error)
        .expect("kernel lock")
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

fn traced_workflow(dir: &std::path::Path, trace: &str) -> String {
    let raw = std::fs::read_to_string(dir.join(trace)).expect("trace journal");
    let recovered = nika_dap::recover::recover_events(&raw, trace).expect("typed trace events");
    recovered
        .events
        .iter()
        .find(|event| event.kind == nika_event::EventKind::WorkflowStarted)
        .and_then(|event| {
            event.fields.iter().find_map(|field| {
                if field.key == "workflow" {
                    match &field.value {
                        nika_types::resource::Value::String(value) => Some(value.clone()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        })
        .expect("workflow_started.workflow")
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
    // The ledger (W5-bis): the claim precedes the run, the receipt
    // settles it by fencing.
    let hist = history(&dir, "doctor");
    let lines: Vec<&str> = hist.lines().collect();
    assert_eq!(lines.len(), 2, "the claim, then the receipt: {hist}");
    let claim: serde_json::Value = serde_json::from_str(lines[0]).expect("claim json");
    let receipt: serde_json::Value = serde_json::from_str(lines[1]).expect("receipt json");
    assert_eq!(claim["kind"], "claimed", "{hist}");
    assert_eq!(claim["payload"]["fencing"], claim["seq"], "{hist}");
    assert_eq!(receipt["kind"], "fired", "{hist}");
    assert_eq!(
        receipt["payload"]["fencing"], claim["seq"],
        "the receipt fences the claim's seq: {hist}"
    );
    assert_eq!(receipt["slot_id"], claim["slot_id"], "{hist}");
    let execution = claim["payload"]["execution_id"]
        .as_str()
        .expect("claim execution id");
    let trace_id = claim["payload"]["trace_id"]
        .as_str()
        .expect("claim trace id");
    assert_eq!(receipt["payload"]["execution_id"], execution);
    assert_eq!(receipt["payload"]["trace_id"], trace_id);
    let raw = std::fs::read_to_string(dir.join(trace)).expect("physical journal");
    let recovered = nika_dap::recover::recover_events(&raw, trace).expect("typed trace events");
    let first_execution = recovered.events[0]
        .execution
        .expect("the first root event carries service execution identity");
    assert_eq!(first_execution.to_string(), execution);
    let execution_uuid = execution
        .strip_prefix("exe-")
        .expect("typed execution prefix");
    assert_eq!(trace_id, execution_uuid.replace('-', ""));
    let trace_suffix = &trace_id[trace_id.len() - 4..];
    assert!(
        trace.ends_with(&format!("-{trace_suffix}.ndjson")),
        "the typed trace ID addresses the physical journal: {trace}"
    );
    assert!(
        recovered
            .events
            .iter()
            .all(|event| event.execution == Some(first_execution)),
        "every root event carries the admitted execution identity"
    );
    assert_eq!(traces(&dir).len(), 1, "one fresh run = one trace (N2)");
}

#[test]
fn direct_cli_projects_one_execution_identity_to_json_and_physical_trace() {
    let dir = project(
        "direct-execution-identity",
        DAILY_3AM,
        &[("doctor.nika.yaml", TRUE)],
    );
    let out = bin()
        .args([
            "run",
            "workflows/doctor.nika.yaml",
            "--json",
            "--color",
            "never",
        ])
        .current_dir(&dir)
        .output()
        .expect("spawn direct run");
    assert_eq!(
        out.status.code(),
        Some(0),
        "direct run: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frames = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json event"))
        .collect::<Vec<_>>();
    let projected = frames[0]["execution"].clone();
    assert!(
        !projected.is_null(),
        "the direct CLI projects execution identity"
    );
    assert!(
        frames.iter().all(|frame| frame["execution"] == projected),
        "every JSON projection carries the same execution"
    );

    let journals = traces(&dir);
    assert_eq!(journals.len(), 1);
    let trace = format!(".nika/traces/{}", journals[0]);
    let raw = std::fs::read_to_string(dir.join(&trace)).expect("physical trace");
    let recovered = nika_dap::recover::recover_events(&raw, &trace).expect("typed trace");
    let execution = recovered.events[0].execution.expect("journal execution");
    assert!(
        recovered
            .events
            .iter()
            .all(|event| event.execution == Some(execution))
    );
    let simple = execution.uuid.as_simple().to_string();
    assert!(trace.ends_with(&format!("-{}.ndjson", &simple[simple.len() - 4..])));
}

#[test]
fn forbidden_effect_fails_without_side_effect_and_keeps_exact_trace_identity() {
    let dir = project(
        "forbidden-effect",
        DAILY_3AM,
        &[("doctor.nika.yaml", FORBIDDEN_WRITE)],
    );
    let out = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn forbidden fire");
    assert_eq!(out.status.code(), Some(1), "runtime denial maps to exit 1");
    let line = assert_one_line(&out);
    assert!(line.starts_with("failed doctor ·"), "{line}");
    assert!(
        !dir.join("forbidden.txt").exists(),
        "the denied effect leaves zero bytes"
    );

    let hist = history(&dir, "doctor");
    let docs = hist
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("ledger json"))
        .collect::<Vec<_>>();
    assert_eq!(docs.len(), 2, "durable claim then terminal receipt: {hist}");
    assert_eq!(docs[1]["kind"], "failed");
    assert_eq!(docs[1]["payload"]["exit"], 1);
    assert_eq!(
        docs[1]["payload"]["execution_id"],
        docs[0]["payload"]["execution_id"]
    );
    assert_eq!(
        docs[1]["payload"]["trace_id"],
        docs[0]["payload"]["trace_id"]
    );
    let trace = docs[1]["payload"]["trace"]
        .as_str()
        .expect("failed receipt trace");
    let raw = std::fs::read_to_string(dir.join(trace)).expect("failed trace exists");
    let recovered = nika_dap::recover::recover_events(&raw, trace).expect("typed failed trace");
    let execution = docs[0]["payload"]["execution_id"]
        .as_str()
        .expect("claim execution");
    assert!(recovered.events.iter().all(|event| {
        event
            .execution
            .is_some_and(|id| id.to_string() == execution)
    }));
    assert!(
        recovered.events.iter().any(|event| {
            event.kind == nika_event::EventKind::PermitChecked
                && event.fields.iter().any(|field| {
                    field.key == "decision"
                        && field.value == nika_types::resource::Value::String("deny".to_owned())
                })
        }),
        "the real denial is journaled"
    );
}

#[cfg(unix)]
#[test]
fn direct_run_with_broken_output_pipe_returns_141_with_finalized_trace() {
    for (tag, extra) in [
        ("plain", &[][..]),
        ("ndjson", &["--json"][..]),
        ("output-json", &["--output", "json"][..]),
    ] {
        let dir =
            std::env::temp_dir().join(format!("nika-direct-pipe-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("workflows")).expect("workflows dir");
        std::fs::write(dir.join("workflows/doctor.nika.yaml"), CLOSED_STDOUT)
            .expect("workflow file");
        let mut cmd = bin();
        cmd.args(["run", "workflows/doctor.nika.yaml"])
            .args(extra)
            .current_dir(&dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().expect("spawn direct run with output pipe");
        drop(child.stdout.take());
        let out = child.wait_with_output().expect("broken-pipe run settles");
        assert_eq!(out.status.code(), Some(141), "{tag}: BrokenPipe stays 141");
        if tag == "ndjson" {
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                stderr.contains("nika run: stream write failed:"),
                "the runtime write owns the diagnostic: {stderr}"
            );
            assert!(
                !stderr.contains("nika run: settlement write failed:"),
                "a prior runtime write is never blamed on settlement: {stderr}"
            );
        }
        let journals = traces(&dir);
        assert_eq!(journals.len(), 1, "{tag}: finalized trace survives");
        let trace = format!(".nika/traces/{}", journals[0]);
        let raw = std::fs::read_to_string(dir.join(&trace)).expect("trace journal");
        nika_dap::recover::recover_events(&raw, &trace).expect("finalized typed trace");
    }
}

#[cfg(unix)]
#[test]
fn closed_stdout_never_corrupts_trace_or_orphans_claim() {
    let dir = project(
        "closed-stdout-preflight",
        DAILY_3AM,
        &[("doctor.nika.yaml", CLOSED_STDOUT)],
    );
    let out = bin_with_stream_setup("exec 1>&-; exec \"$@\"")
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn arm fire with stdout closed");
    assert_ne!(out.status.code(), Some(101), "closed stdout never panics");
    let hist = history(&dir, "doctor");
    if !hist.is_empty() {
        assert_eq!(
            nika_cadence::ledger::unsettled(&hist)
                .expect("valid ledger")
                .count(),
            0,
            "no orphan claim: {hist}"
        );
    }
    for trace in traces(&dir) {
        let path = format!(".nika/traces/{trace}");
        let raw = std::fs::read_to_string(dir.join(&path)).expect("trace journal");
        nika_dap::recover::recover_events(&raw, &path).expect("uncorrupted trace");
    }
}

#[cfg(unix)]
#[test]
fn arm_fire_with_broken_run_pipe_settles_exact_trace() {
    let dir = project(
        "arm-broken-run-pipe",
        DAILY_3AM,
        &[("doctor.nika.yaml", CLOSED_STDOUT)],
    );
    let mut child = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn arm fire with diagnostic pipe");
    drop(child.stderr.take());
    let out = child.wait_with_output().expect("broken-pipe fire settles");
    assert_eq!(
        out.status.code(),
        Some(141),
        "the BrokenPipe receipt settles honestly: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let hist = history(&dir, "doctor");
    let lines: Vec<&str> = hist.lines().collect();
    assert_eq!(lines.len(), 2, "claim then terminal receipt: {hist}");
    let claim: serde_json::Value = serde_json::from_str(lines[0]).expect("claim json");
    let receipt: serde_json::Value = serde_json::from_str(lines[1]).expect("receipt json");
    assert_eq!(claim["kind"], "claimed", "{hist}");
    assert_eq!(receipt["kind"], "failed", "{hist}");
    assert_eq!(receipt["payload"]["exit"], 141, "{hist}");
    assert_eq!(receipt["payload"]["fencing"], claim["seq"], "{hist}");
    let trace = receipt["payload"]["trace"]
        .as_str()
        .expect("receipt retains the exact trace");
    assert!(dir.join(trace).exists(), "receipt trace exists: {trace}");
    assert_eq!(
        nika_cadence::ledger::unsettled(&hist)
            .expect("valid ledger")
            .count(),
        0,
        "the claim is terminally settled"
    );
}

#[test]
fn fire_keeps_pinned_parent_bytes_and_their_original_relative_child_base() {
    let registry = DAILY_3AM.replace("doctor.nika.yaml", "parent.nika.yaml");
    let dir = project(
        "relative-child-source-replacement",
        &registry,
        &[
            ("parent.nika.yaml", RELATIVE_CHILD_PARENT),
            ("child.nika.yaml", RELATIVE_CHILD),
        ],
    );
    let child = bin()
        .args(["arm", "fire", "parent", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn fire");
    let history_path = dir.join(".nika/arm/parent/history.ndjson");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let claimed = std::fs::read_to_string(&history_path)
            .is_ok_and(|text| text.contains("\"kind\":\"claimed\""));
        if claimed {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "claim did not land");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let replacement = "nika: replacement\npermits: { exec: true }\ntasks:\n  fail:\n    exec: { shell: \"false\" }\n";
    std::fs::write(dir.join("workflows/parent.nika.yaml"), replacement)
        .expect("replace source after claim");
    let out = child.wait_with_output().expect("fire settles");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the pinned parent and relative child run: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = assert_one_line(&out);
    assert!(line.starts_with("fired parent ·"), "{line}");
    assert_eq!(
        std::fs::read_to_string(dir.join("workflows/parent.nika.yaml")).expect("replacement"),
        replacement,
        "the successful run came from the captured bytes, not a second file read"
    );
}

#[test]
fn actual_arm_runner_uses_captured_child_and_skill_after_durable_claim() {
    let registry = DAILY_3AM.replace("doctor.nika.yaml", "parent.nika.yaml");
    let dir = project(
        "captured-child-skill-mutation",
        &registry,
        &[
            ("parent.nika.yaml", CAPTURED_WORLD_PARENT),
            ("child.nika.yaml", RELATIVE_CHILD),
            (
                "skill.md",
                "---\nname: captured\ndescription: captured guidance\n---\nOriginal.\n",
            ),
        ],
    );
    let child = bin()
        .args(["arm", "fire", "parent", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn captured-world fire");
    let history_path = dir.join(".nika/arm/parent/history.ndjson");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !std::fs::read_to_string(&history_path)
        .is_ok_and(|text| text.contains("\"kind\":\"claimed\""))
    {
        assert!(std::time::Instant::now() < deadline, "claim did not land");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    std::fs::write(
        dir.join("workflows/child.nika.yaml"),
        "not: a valid nika workflow\n",
    )
    .expect("mutate child after claim");
    std::fs::write(dir.join("workflows/skill.md"), "invalid replacement")
        .expect("mutate skill after claim");

    let out = child
        .wait_with_output()
        .expect("captured-world fire settles");
    assert_eq!(
        out.status.code(),
        Some(0),
        "only the admitted child+skill bytes may run: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = assert_one_line(&out);
    assert!(line.starts_with("fired parent ·"), "{line}");
}

#[test]
fn fire_resolves_relative_skills_from_the_declared_workflow_path() {
    let registry = DAILY_3AM.replace("doctor.nika.yaml", "skilled.nika.yaml");
    let dir = project(
        "relative-skill",
        &registry,
        &[("skilled.nika.yaml", RELATIVE_SKILL)],
    );
    std::fs::write(
        dir.join("workflows/skill.md"),
        "---\nname: careful\ndescription: relative fixture\n---\nBe exact.\n",
    )
    .expect("relative skill");
    let out = bin()
        .args(["arm", "fire", "skilled", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn fire");
    assert_eq!(
        out.status.code(),
        Some(0),
        "relative skill resolves: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = assert_one_line(&out);
    assert!(line.starts_with("fired skilled ·"), "{line}");
}

#[test]
fn concurrent_labels_record_their_own_exact_trace_paths() {
    let registry = concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/alpha.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "  - workflow: workflows/beta.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
    );
    let alpha = "nika: alpha-sleeper\npermits: { exec: true }\ntasks:\n  wait:\n    exec: { shell: \"sleep 1\" }\n";
    let beta = "nika: beta-sleeper\npermits: { exec: true }\ntasks:\n  wait:\n    exec: { shell: \"sleep 1\" }\n";
    let dir = project(
        "concurrent-trace-identity",
        registry,
        &[("alpha.nika.yaml", alpha), ("beta.nika.yaml", beta)],
    );
    let spawn = |label: &str| {
        bin()
            .args(["arm", "fire", label, "--now", "2026-08-19T03:02:00Z"])
            .current_dir(&dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn concurrent fire")
    };
    let alpha = spawn("alpha");
    let beta = spawn("beta");
    let alpha = alpha.wait_with_output().expect("alpha settles");
    let beta = beta.wait_with_output().expect("beta settles");
    assert_eq!(alpha.status.code(), Some(0));
    assert_eq!(beta.status.code(), Some(0));
    assert_one_line(&alpha);
    assert_one_line(&beta);
    let alpha_trace = last_json(&dir, "alpha")
        .and_then(|doc| doc["trace"].as_str().map(str::to_owned))
        .expect("alpha trace");
    let beta_trace = last_json(&dir, "beta")
        .and_then(|doc| doc["trace"].as_str().map(str::to_owned))
        .expect("beta trace");
    assert_ne!(
        alpha_trace, beta_trace,
        "concurrent receipts must not select one global newest trace"
    );
    assert!(dir.join(&alpha_trace).exists());
    assert!(dir.join(&beta_trace).exists());
    assert_eq!(traced_workflow(&dir, &alpha_trace), "alpha-sleeper");
    assert_eq!(traced_workflow(&dir, &beta_trace), "beta-sleeper");
}

#[test]
fn fire_skips_a_missed_slot_when_manque_is_sauter() {
    let dir = project("missed", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    seed_fire(&dir, "doctor", "2026-08-18T03:02:00Z");
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
    assert_eq!(history(&dir, "doctor").lines().count(), 3);
    // … and the skip ran nothing: only the seed fire left a trace.
    assert_eq!(traces(&dir).len(), 1);
}

#[test]
fn fire_refuses_an_unknown_label_and_names_the_known_ones() {
    let registry = concat!(
        "nika: proj\n",
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
    let _lock = seed_lock(&dir, "doctor");
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
    assert!(
        line.ends_with("· slot 2026-08-19T03:00:00Z"),
        "the slot rides the line (D8's consistency): {line}"
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
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC * * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "    chevauchement: file\n",
    );
    let dir = project("queue", registry, &[("doctor.nika.yaml", TRUE)]);
    let _lock = seed_lock(&dir, "doctor");
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
    assert!(
        line.ends_with("· slot 2026-08-19T03:02:00Z"),
        "the slot rides the line (D8's consistency): {line}"
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
        "nika: proj\n",
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
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: rattraper-une-fois\n",
    );
    let dir = project("catchup", registry, &[("doctor.nika.yaml", TRUE)]);
    seed_fire(&dir, "doctor", "2026-08-17T03:02:00Z");
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
