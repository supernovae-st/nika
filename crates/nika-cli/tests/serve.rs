// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This suite's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// arm_fire.rs / bin_smoke.rs — and the SIGTERM test spawns one watchdog
// thread so a broken loop fails LOUD instead of hanging the suite.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

//! `nika serve` end-to-end (W5 · LE TIREUR RÉSIDENT): a tempdir project
//! (a `nika.yaml` registry + a `workflows/` shelf), the real binary, and
//! the hidden replay hooks (`--now`/`--until`, D5) making every pass
//! deterministic — the scripted waits ADVANCE the clock instead of
//! sleeping, so a loop spanning two slots still runs in milliseconds.

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
    // Nothing serve runs may ever ask a terminal: stdin stays closed.
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// A tempdir project: the registry + the workflow shelf.
fn project(tag: &str, registry: &str, workflows: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-serve-{tag}-{}", std::process::id()));
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

/// The beat whose run REWRITES the registry (the reload test's own
/// mover — the edit lands mid-loop, driven by the fire itself).
const REWRITER: &str = concat!(
    "nika: armed-rewrite\n",
    // The write is DECLARED — the sandboxed exec (seatbelt) honors the
    // fs boundary, so the registry move names its target.
    "permits:\n",
    "  exec: true\n",
    "  fs:\n",
    "    read: [\"./nika-v2.yaml\"]\n",
    "    write: [\"./nika.yaml\"]\n",
    "tasks:\n",
    "  ok:\n",
    "    exec: { shell: \"cp nika-v2.yaml nika.yaml\" }\n",
);

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

/// Every file under the project, repo-relative, sorted.
fn tree(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).expect("read dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(
                    path.strip_prefix(dir)
                        .expect("under the root")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    out.sort_unstable();
    out
}

/// W5 gate: `--once` fires what is due and exits zero.
#[test]
fn serve_once_fires_what_is_due_and_exits_zero() {
    let dir = project("once", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args(["serve", "--once", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), 1, "ONE decision line: «{stdout}»");
    let line = stdout.lines().next().expect("the one line");
    assert!(
        line.starts_with("fired doctor · slot 2026-08-19T03:00:00Z · exit 0 · trace .nika/traces/"),
        "{line}"
    );
    // The record stands — and the firer's lock is RELEASED.
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "fired");
    assert_eq!(last["slot"], "2026-08-19T03:00:00Z");
    assert!(
        !dir.join(".nika/arm/doctor/lock").exists(),
        "the lock never outlives the fire"
    );
}

/// The loop: two beats, two slots, fired in slot order — the scripted
/// clock spans the hour in milliseconds.
#[test]
fn serve_loop_fires_two_beats_in_slot_order() {
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
        "loop",
        registry,
        &[("doctor.nika.yaml", TRUE), ("nightly.nika.yaml", TRUE)],
    );
    let out = bin()
        .args([
            "serve",
            "--now",
            "2026-08-19T02:59:55Z",
            "--until",
            "2026-08-19T04:06:00Z",
        ])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "two slots, two fires: «{stdout}»");
    assert!(
        lines[0].starts_with("fired doctor · slot 2026-08-19T03:00:00Z"),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("fired nightly · slot 2026-08-19T04:00:00Z"),
        "{}",
        lines[1]
    );
    // Both locks released; the bound was honored (stderr stop line).
    assert!(!dir.join(".nika/arm/doctor/lock").exists());
    assert!(!dir.join(".nika/arm/nightly/lock").exists());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("until reached"), "{stderr}");
}

/// The file proposes, re-read: a beat that is NOT in the boot registry
/// fires once the moved file lands it — the edit is driven by the first
/// fire itself (`cp nika-v2.yaml nika.yaml`).
#[test]
fn serve_reloads_the_registry_when_the_file_changes() {
    let v1 = DAILY_3AM;
    let v2 = concat!(
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
        "reload",
        v1,
        &[("doctor.nika.yaml", REWRITER), ("nightly.nika.yaml", TRUE)],
    );
    std::fs::write(dir.join("nika-v2.yaml"), v2).expect("the staged v2");
    let out = bin()
        .args([
            "serve",
            "--now",
            "2026-08-19T02:59:55Z",
            "--until",
            "2026-08-19T04:06:00Z",
        ])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "the boot beat, then the reloaded one: «{stdout}»"
    );
    assert!(
        lines[0].starts_with("fired doctor · slot 2026-08-19T03:00:00Z"),
        "{}",
        lines[0]
    );
    // nightly was NOT armed at boot — its fire PROVES the re-read.
    assert!(
        lines[1].starts_with("fired nightly · slot 2026-08-19T04:00:00Z"),
        "{}",
        lines[1]
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("re-read"), "the reload is said: {stderr}");
    // … and the served file at rest is the v2 the first fire installed.
    let served = std::fs::read_to_string(dir.join("nika.yaml")).expect("the file at rest");
    assert!(served.contains("nightly"), "{served}");
}

/// SIGTERM: the resident loop stops clean — exit 0 (a signal death
/// would read `None`), the fire in flight finished first, no lock left.
#[cfg(unix)]
#[test]
fn serve_stops_cleanly_on_sigterm() {
    use std::io::BufRead as _;
    let dir = project("sigterm", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    let mut child = bin()
        .args(["serve"])
        .current_dir(&dir)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn serve");
    // The watchdog: a loop that ignores SIGTERM must fail LOUD, never
    // hang the suite (detached — it reaps itself once `done` lands).
    let pid = child.id();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watched = std::sync::Arc::clone(&done);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if !watched.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        }
    });
    // Synchronize on the startup line: the loop is armed once it
    // prints. The reader stays ALIVE past the read — dropping it closes
    // the pipe, and the child's stop line would die EPIPE (the pipe
    // guard's honest 141), which is not the signal path being judged.
    let stderr = child.stderr.take().expect("piped stderr");
    let mut lines = std::io::BufReader::new(stderr);
    let mut boot = String::new();
    lines.read_line(&mut boot).expect("the startup line");
    assert!(boot.contains("serve ·"), "{boot}");
    let status_kill = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("kill -TERM");
    assert!(status_kill.success());
    let status = child.wait().expect("wait");
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    drop(lines);
    assert_eq!(
        status.code(),
        Some(0),
        "SIGTERM = a clean exit 0, never a signal death"
    );
    assert!(!dir.join(".nika/arm/doctor/lock").exists());
}

/// A cloud beat is the planner's own refusal — serve never fires it,
/// never records it, never builds its sidecar.
#[test]
fn serve_never_fires_a_cloud_beat() {
    let registry = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "    où: cloud\n",
    );
    let dir = project("cloud", registry, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args(["serve", "--once", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.is_empty(), "no fire, no line: «{stdout}»");
    assert!(!dir.join(".nika").exists(), "no sidecar, no trace dir");
}

/// Gate 1 (P0): serve has NO input but the registry and its own
/// sidecar — the public surface carries no port, no socket, no token,
/// no external file door; and a run touches only `nika.yaml` (read),
/// `.nika/arm/` and `.nika/traces/` (its own state) and the workflow
/// shelf (read).
#[test]
fn serve_has_no_input_but_the_registry_and_its_state() {
    // (a) The surface: `--help` names the two public flags and NOTHING
    // else — no port/bind/token/url/file door exists to name.
    let out = bin()
        .args(["serve", "--help"])
        .output()
        .expect("spawn serve --help");
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--once"), "{help}");
    assert!(help.contains("--dry"), "{help}");
    for banned in ["--port", "--bind", "--token", "--url", "--socket", "--addr"] {
        assert!(!help.contains(banned), "serve has no {banned} door: {help}");
    }

    // (b) The tree: after a full scripted loop, every file under the
    // project is the registry, the workflow shelf, or serve's own
    // sidecar/traces — nothing else was read, nothing else was written.
    let dir = project("trust", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args([
            "serve",
            "--now",
            "2026-08-19T02:59:55Z",
            "--until",
            "2026-08-19T03:06:00Z",
        ])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(out.status.code(), Some(0));
    for path in tree(&dir) {
        let known = path == "nika.yaml"
            || path.starts_with("workflows/")
            || path.starts_with(".nika/arm/")
            || path.starts_with(".nika/traces/");
        assert!(known, "serve touched an input it must not: {path}");
    }
    assert_eq!(traces(&dir).len(), 1, "the one fire left its one trace");
}

/// A scripted loop without a bound refuses at the edge — it would spin,
/// never serve.
#[test]
fn a_scripted_loop_without_a_bound_refuses() {
    let dir = project("boundless", DAILY_3AM, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args(["serve", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(out.status.code(), Some(1), "the server convention: 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("needs a bound"), "{stderr}");
    assert!(traces(&dir).is_empty(), "nothing ran");
}

/// A registry that refuses at BOOT stops serve before any pass — the
/// two judges (grammar · law) read the file before any firing.
#[test]
fn a_registry_that_refuses_at_boot_exits_one_with_the_refusal() {
    let registry = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 3 * * *\"\n",
        "    plafond: 0.05\n",
        // manqué: absent — REQUIRED by the law, no default.
    );
    let dir = project("refusal", registry, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args(["serve", "--once", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("refusal"), "the refusal is said: {stderr}");
    assert!(traces(&dir).is_empty(), "nothing ran");
    assert!(last_json(&dir, "doctor").is_none(), "nothing recorded");
}
