// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]
// This test SPAWNS bwrap directly (std::process::Command) to PROVE the
// generated jail confines — the one legitimate place outside the runner that
// spawns, because verifying the jail requires actually entering it.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
// The ADVERSARIAL jail proof — actually runs bwrap and verifies the generated
// namespace jail CONFINES the child (not just that the argv builds).
// Linux-only (bwrap); the macOS counterpart lives in nika-sandbox-seatbelt.
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::Command;

use nika_kernel::command_sandbox::CommandSandbox;
use nika_kernel::process::{SandboxSpec, ShellCommand};
use nika_sandbox_landlock::LandlockSandbox;

/// Confine `program args…` under `spec` and actually run it via bwrap.
fn run(spec: &SandboxSpec, program: &str, args: &[&str]) -> std::process::Output {
    let mut cmd = ShellCommand::new(program);
    cmd.args = args.iter().map(|s| (*s).to_string()).collect();
    let w = LandlockSandbox::new().confine(spec, cmd).expect("confine");
    assert_eq!(w.program, "/usr/bin/bwrap");
    Command::new(&w.program)
        .args(&w.args)
        .output()
        .expect("spawn bwrap")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME set"))
}

/// THE HEADLINE: a confined command cannot read a SENSITIVE file in the home
/// dir (the `~/.ssh/id_rsa` class) — `$HOME` is bound nowhere in the jail, so
/// the file is absent and the read fails (deny-default by absence).
#[test]
fn confined_cannot_read_a_sensitive_home_file() {
    if !LandlockSandbox::available() {
        return; // no bwrap on this runner — skip, not fail (matches the sibling)
    }
    let secret = home().join(format!(".nika-sbx-secret-{}", std::process::id()));
    std::fs::write(&secret, "TOPSECRET-KEY-MATERIAL").expect("write secret");

    // Empty spec = maximally confined (no declared reads, no network).
    let out = run(&SandboxSpec::new(), "/bin/cat", &[secret.to_str().unwrap()]);
    let _ = std::fs::remove_file(&secret);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Anti-vacuity: if bwrap ITSELF fails (e.g. an AppArmor profile denying
    // CAP_NET_ADMIN in the userns), the child never runs and "no secret +
    // non-zero exit" would pass without proving anything. A launcher-level
    // error makes the proof void, so it fails loudly instead.
    assert!(
        !stderr.contains("bwrap:"),
        "the launcher itself failed — the proof is vacuous · stderr {stderr:?}"
    );
    assert!(
        !stdout.contains("TOPSECRET"),
        "the jail must NOT expose a sensitive home file · stdout was {stdout:?}"
    );
    assert!(
        !out.status.success(),
        "reading an unbound file must fail · stderr {stderr:?}"
    );
}

/// A confined command can read a DECLARED path (the jail admits the granted reach).
#[test]
fn confined_can_read_a_declared_path() {
    if !LandlockSandbox::available() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("nika-sbx-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("in.txt");
    std::fs::write(&file, "DECLARED-READABLE").expect("write");

    let mut spec = SandboxSpec::new();
    spec.fs_read = vec![format!("{}/**", dir.display())];
    let out = run(&spec, "/bin/cat", &[file.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        String::from_utf8_lossy(&out.stdout).contains("DECLARED-READABLE"),
        "a declared read path must be inside the jail · status {:?} · stderr {:?}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
}

/// WRITE-DENY: a dir granted for READ is bound read-only — a write inside
/// it fails, even though the same path is perfectly writable on the host
/// (the read grant must never smuggle a write).
#[test]
fn confined_cannot_write_into_a_read_only_grant() {
    if !LandlockSandbox::available() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("nika-sbx-ro-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");

    let mut spec = SandboxSpec::new();
    spec.fs_read = vec![format!("{}/**", dir.display())];
    let target = dir.join("smuggle.txt");
    let out = run(
        &spec,
        "/bin/sh",
        &["-c", &format!("echo smuggled > {}", target.display())],
    );
    // Anti-vacuity (the sibling proofs' guard): a launcher-level failure
    // voids the proof — the child never ran.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("bwrap:"),
        "the launcher itself failed — the proof is vacuous · stderr {stderr:?}"
    );
    assert!(
        !out.status.success(),
        "a write inside a read-only bind must fail · stderr {stderr:?}"
    );
    assert!(
        !target.exists(),
        "the host file must NOT appear · the ro-bind held"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// NETWORK-DENY: without an explicit lift the net namespace is UNSHARED —
/// a connection that would succeed on the host fails inside the jail.
/// 1.1.1.1:80 is near-universally reachable, so the failure can only come
/// from the missing namespace, never from the target.
#[test]
fn confined_has_no_network_without_an_explicit_lift() {
    if !LandlockSandbox::available() {
        return;
    }
    let out = run(
        &SandboxSpec::new(),
        "/bin/bash",
        &["-c", "echo probe > /dev/tcp/1.1.1.1/80"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("bwrap:"),
        "the launcher itself failed — the proof is vacuous · stderr {stderr:?}"
    );
    assert!(
        !out.status.success(),
        "an unshared netns must refuse the connection · stderr {stderr:?}"
    );
}
