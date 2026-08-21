// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This suite's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// arm_fire.rs / bin_smoke.rs.
#![allow(clippy::disallowed_types)]

//! `nika serve` end-to-end (W5 · LE TIREUR RÉSIDENT): a tempdir project,
//! the real binary, and the injected clock (`--now`/`--until`, D5) making
//! the loop deterministic — the harness advances the clock on each sleep,
//! so the loop never waits; the SIGTERM stop rides the REAL clock.

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
    // A pause must PARK, never ask (the arm_fire.rs precedent).
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

/// Daily 03:00 UTC, skip the misses.
const DAILY_3AM: &str = concat!(
    "nika: v1\n",
    "arm:\n",
    "  - workflow: workflows/doctor.nika.yaml\n",
    "    cadence: \"TZ=UTC 0 3 * * *\"\n",
    "    plafond: 0.05\n",
    "    manqué: sauter\n",
);

/// Every minute — the loop + signal tests' cadence.
const EVERY_MINUTE: &str = concat!(
    "nika: v1\n",
    "arm:\n",
    "  - workflow: workflows/doctor.nika.yaml\n",
    "    cadence: \"TZ=UTC * * * * *\"\n",
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
    assert!(
        stdout.contains("fired doctor · slot 2026-08-19T03:00:00Z · exit 0"),
        "{stdout}"
    );
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "fired");
    assert_eq!(last["slot"], "2026-08-19T03:00:00Z");
}

#[test]
fn serve_loop_fires_two_beats_in_slot_order() {
    let dir = project("loop", EVERY_MINUTE, &[("doctor.nika.yaml", TRUE)]);
    let out = bin()
        .args([
            "serve",
            "--now",
            "2026-08-19T03:02:00Z",
            "--until",
            "2026-08-19T03:03:30Z",
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
    assert_eq!(lines.len(), 2, "one line per beat: {stdout}");
    assert!(
        lines[0].contains("fired doctor · slot 2026-08-19T03:02:00Z"),
        "the first slot first: {stdout}"
    );
    assert!(
        lines[1].contains("fired doctor · slot 2026-08-19T03:03:00Z"),
        "the second slot second: {stdout}"
    );
    // Two fires, two lines each (W5-bis): the claim, then the receipt.
    assert_eq!(history(&dir, "doctor").lines().count(), 4);
}

#[test]
fn serve_never_fires_a_cloud_beat() {
    let registry = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC * * * * *\"\n",
        "    où: cloud\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
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
    assert!(
        !stdout.contains("fired"),
        "a cloud beat never fires: {stdout}"
    );
    assert!(
        !dir.join(".nika/arm/doctor").exists(),
        "the cloud's calendar stays the operator's — no sidecar is even opened"
    );
}

/// unix: SIGTERM while the resident idles between two slots — it exits
/// clean (0) and releases its kernel lease. The stable metadata inode remains.
#[cfg(unix)]
#[test]
fn serve_stops_cleanly_on_sigterm() {
    let dir = project("sigterm", EVERY_MINUTE, &[("doctor.nika.yaml", TRUE)]);
    let mut child = bin()
        .args(["serve"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn serve");
    // Wait for the first fire (the sidecar attests it), then TERM.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while last_json(&dir, "doctor").is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the first fire never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let pid = nix::unistd::Pid::from_raw(i32::try_from(child.id()).expect("pid"));
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).expect("kill -TERM");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("serve ignored SIGTERM");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(0), "SIGTERM = a clean stop");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join(".nika/arm/doctor/lock"))
        .expect("stable lock metadata");
    let lease = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .map_err(|(_, error)| error)
        .expect("the kernel lease is released");
    drop(lease);
}
