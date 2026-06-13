// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]
// This test SPAWNS sandbox-exec directly (std::process::Command) to PROVE the
// generated profile confines — the one legitimate place outside the runner
// that spawns, because verifying the jail requires actually entering it.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
// The ADVERSARIAL jail proof — actually runs sandbox-exec and verifies the
// generated SBPL profile CONFINES the child (not just that the string parses).
// macOS-only (Seatbelt); the Linux counterpart lives in nika-sandbox-landlock.
#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::Command;

use nika_kernel::command_sandbox::CommandSandbox;
use nika_kernel::process::{SandboxSpec, ShellCommand};
use nika_sandbox_seatbelt::SeatbeltSandbox;

/// Confine `program args…` under `spec` and actually run it via sandbox-exec.
fn run(spec: &SandboxSpec, program: &str, args: &[&str]) -> std::process::Output {
    let mut cmd = ShellCommand::new(program);
    cmd.args = args.iter().map(|s| (*s).to_string()).collect();
    let w = SeatbeltSandbox::new().confine(spec, cmd).expect("confine");
    assert_eq!(w.program, "/usr/bin/sandbox-exec");
    Command::new(&w.program)
        .args(&w.args)
        .output()
        .expect("spawn sandbox-exec")
}

/// Confine a shell line under `spec` and run it.
fn run_shell(spec: &SandboxSpec, line: &str) -> std::process::Output {
    let mut cmd = ShellCommand::new(line);
    cmd.shell = true;
    let w = SeatbeltSandbox::new().confine(spec, cmd).expect("confine");
    Command::new(&w.program)
        .args(&w.args)
        .output()
        .expect("spawn sandbox-exec")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME set"))
}

/// THE HEADLINE: a confined command cannot read a SENSITIVE file in the home
/// dir (the `~/.ssh/id_rsa` class) — it is in no allow rule, so deny-default
/// blocks the read.
#[test]
fn confined_cannot_read_a_sensitive_home_file() {
    if !SeatbeltSandbox::available() {
        return; // no launcher (CI without macOS sandbox) — skip, not fail
    }
    let secret = home().join(format!(".nika-sbx-secret-{}", std::process::id()));
    std::fs::write(&secret, "TOPSECRET-KEY-MATERIAL").expect("write secret");

    // Empty spec = maximally confined (no declared reads, no network).
    let out = run(&SandboxSpec::new(), "/bin/cat", &[secret.to_str().unwrap()]);

    let _ = std::fs::remove_file(&secret);
    assert!(
        !out.status.success(),
        "reading a home secret under an empty spec MUST be denied"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("TOPSECRET"),
        "the secret's CONTENTS must never reach stdout"
    );
}

/// A path GRANTED via `fs_read` becomes readable — the declared reach works
/// (no over-confinement of the workflow's own data).
#[test]
fn fs_read_grant_makes_a_path_readable() {
    if !SeatbeltSandbox::available() {
        return;
    }
    let dir = home().join(format!(".nika-sbx-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("data.txt");
    std::fs::write(&file, "DECLARED-DATA").expect("write");

    let mut spec = SandboxSpec::new();
    spec.fs_read = vec![format!("{}/**", dir.display())];
    let out = run(&spec, "/bin/cat", &[file.to_str().unwrap()]);

    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "a granted path must be readable: {out:?}"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("DECLARED-DATA"),
        "the granted file's contents are returned"
    );
}

/// A confined command cannot WRITE outside the declared `fs_write` (here: the
/// home dir, not granted) — deny-default blocks the write.
#[test]
fn confined_cannot_write_outside_the_allowlist() {
    if !SeatbeltSandbox::available() {
        return;
    }
    let target = home().join(format!(".nika-sbx-w-{}", std::process::id()));
    let _ = std::fs::remove_file(&target);

    // Empty spec → no writable paths beyond scratch. Writing to $HOME is denied.
    let out = run_shell(
        &SandboxSpec::new(),
        &format!("echo pwned > '{}'", target.display()),
    );

    let created = target.exists();
    let _ = std::fs::remove_file(&target);
    assert!(!created, "a write outside the allowlist MUST be denied");
    assert!(
        !out.status.success(),
        "the redirect should fail under the sandbox"
    );
}

/// Sanity: the sandbox does NOT break an ordinary command (no false-deny) —
/// `/usr/bin/true` runs, and reading scratch (`/private/tmp`) is allowed.
#[test]
fn ordinary_command_still_runs_under_the_sandbox() {
    if !SeatbeltSandbox::available() {
        return;
    }
    let out = run(&SandboxSpec::new(), "/usr/bin/true", &[]);
    assert!(
        out.status.success(),
        "a benign command must still run: {out:?}"
    );

    let scratch = PathBuf::from(format!("/private/tmp/.nika-sbx-ok-{}", std::process::id()));
    std::fs::write(&scratch, "SCRATCH-OK").expect("write scratch");
    let out = run(
        &SandboxSpec::new(),
        "/bin/cat",
        &[scratch.to_str().unwrap()],
    );
    let _ = std::fs::remove_file(&scratch);
    assert!(
        out.status.success(),
        "scratch (/private/tmp) is readable: {out:?}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("SCRATCH-OK"));
}
