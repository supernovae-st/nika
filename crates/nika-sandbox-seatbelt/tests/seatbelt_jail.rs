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
/// `/usr/bin/true` runs. And the issue-754 closure holds: the SHARED host
/// tmp trees are NOT an ambient grant anymore — an ungranted `/private/tmp`
/// write is DENIED (it bypassed every declared boundary before), while a
/// DECLARED `fs_write` grant on a tmp subpath still works (the boundary is
/// the file, not the tree).
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

    // Issue 754 · the bypass is closed: no grant → no /private/tmp write.
    let probe = format!("/private/tmp/.nika-sbx-754-{}", std::process::id());
    let out = run_shell(
        &SandboxSpec::new(),
        &format!("echo NIKA-754 > {probe} && cat {probe}"),
    );
    let leaked = std::path::Path::new(&probe).exists();
    let _ = std::fs::remove_file(&probe);
    assert!(
        !out.status.success() && !leaked,
        "an ungranted /private/tmp write must be DENIED (issue 754): {out:?}"
    );

    // …and a DECLARED tmp grant still writes (the boundary is honest).
    let granted_dir = format!("/private/tmp/.nika-sbx-754-ok-{}", std::process::id());
    std::fs::create_dir_all(&granted_dir).expect("mk granted dir");
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{granted_dir}/**")];
    let out = run_shell(
        &spec,
        &format!("echo GRANTED-OK > {granted_dir}/f && cat {granted_dir}/f"),
    );
    let ok = out.status.success() && String::from_utf8_lossy(&out.stdout).contains("GRANTED-OK");
    let _ = std::fs::remove_dir_all(&granted_dir);
    assert!(ok, "a declared tmp grant must still write: {out:?}");
}

/// THE SQLITE DURABILITY PROOF (the 2026-07-29 finding, closed live): a
/// database granted by its EXACT file path — no `**` directory hatch — must
/// run WAL mode under the jail. Before the journal-sidecar literals this
/// died with `SQLITE_CANTOPEN` (14) the moment WAL created `-shm`/`-wal`
/// (bisected on macOS 15.6.1 · sqlite 3.43.2). Also proves the extension
/// does NOT leak: an arbitrary sibling in the same directory stays denied.
#[test]
fn an_exact_file_db_grant_supports_wal_mode_without_granting_the_directory() {
    if !SeatbeltSandbox::available() || !PathBuf::from("/usr/bin/sqlite3").exists() {
        return; // no launcher / no OS sqlite3 (CI) — skip, not fail
    }
    let dir = home().join(format!(".nika-sbx-sqlite-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db = dir.join("state.db");
    let db = db.to_str().unwrap().to_owned();

    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![db.clone()]; // the EXACT file — the narrow grant

    // WAL open + write + read across two connections (the second exercises
    // sidecar reopen + checkpoint + close-time unlink).
    let out = run(
        &spec,
        "/usr/bin/sqlite3",
        &[
            &db,
            "PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS t(x); INSERT INTO t VALUES (1);",
        ],
    );
    let out2 = run(
        &spec,
        "/usr/bin/sqlite3",
        &[&db, "INSERT INTO t VALUES (2); SELECT count(*) FROM t;"],
    );

    // The fence half: the same spec must still refuse an arbitrary sibling
    // write in the database's own directory (the sidecars are exact
    // literals, not a directory grant).
    let sibling = dir.join("evil.txt");
    let out3 = run_shell(&spec, &format!("echo pwned > '{}'", sibling.display()));
    let sibling_created = sibling.exists();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "WAL under an exact-file grant must work (the CANTOPEN class): {out:?}"
    );
    assert!(
        out2.status.success() && String::from_utf8_lossy(&out2.stdout).contains('2'),
        "sidecar reopen + checkpoint must work: {out2:?}"
    );
    assert!(
        !sibling_created && !out3.status.success(),
        "the journal-sidecar extension must never grant the directory"
    );
}

/// THE RELATIVE-PATH HALF of the finding (the qrsmart case, closed live):
/// a workflow's `exec` child gets the paths the author wrote — RELATIVE —
/// and the libc `getcwd` walk behind every relative open reads the cwd's
/// directory entries. Under deny-default that read died (`file-read-data`
/// on the cwd, per the kernel denial log), so even a fully-granted db
/// CANTOPEN'd. The profile must list the child's cwd — proven here by
/// opening a relative db under an exact-file grant with `cwd` confined.
#[test]
fn a_relative_db_path_survives_the_getcwd_walk() {
    if !SeatbeltSandbox::available() || !PathBuf::from("/usr/bin/sqlite3").exists() {
        return; // no launcher / no OS sqlite3 (CI) — skip, not fail
    }
    let dir = home().join(format!(".nika-sbx-rel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db_abs = dir.join("state.db");

    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![db_abs.to_str().unwrap().to_owned()]; // EXACT file

    let mut cmd = ShellCommand::new("/usr/bin/sqlite3");
    cmd.args = vec![
        "state.db".to_owned(), // RELATIVE — the finding's exact shape
        "PRAGMA journal_mode=WAL; CREATE TABLE t(x); INSERT INTO t VALUES (1); SELECT count(*) FROM t;".to_owned(),
    ];
    cmd.cwd = Some(dir.clone());
    let w = SeatbeltSandbox::new().confine(&spec, cmd).expect("confine");
    let mut spawn = Command::new(&w.program);
    spawn.args(&w.args);
    if let Some(cwd) = &w.cwd {
        spawn.current_dir(cwd); // the runner applies it; the jail proof must too
    }
    let out = spawn.output().expect("spawn sandbox-exec");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        out.status.success() && String::from_utf8_lossy(&out.stdout).contains('1'),
        "a relative db path must open under the jail (the getcwd walk): {out:?}"
    );
}

/// THE EGRESS FENCE (the sandbox-runtime seatbelt model, live): under the
/// `allowlist` arm with the proxy port filled, a confined child CAN reach
/// loopback on exactly the proxy's port and is EPERM-refused on every other
/// port — the channel is fenced, so the loopback proxy (which lives OUTSIDE
/// the jail) is the child's only way out. And under `deny`, loopback is
/// reachable but nothing beyond it (the mission's carve-out).
#[test]
fn allowlist_fences_loopback_to_the_proxy_port() {
    use std::net::{Ipv4Addr, TcpListener};

    if !SeatbeltSandbox::available() {
        return;
    }
    // The "proxy" listener and an "other service" listener, both loopback.
    let proxy = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind proxy");
    let proxy_port = proxy.local_addr().expect("addr").port();
    let other = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind other");
    let other_port = other.local_addr().expect("addr").port();
    // drain both so a successful connect never hangs nc
    for l in [proxy, other] {
        std::thread::spawn(move || {
            while let Ok((s, _)) = l.accept() {
                drop(s);
            }
        });
    }

    let mut spec = SandboxSpec::new();
    let mut allowlist = nika_kernel::process::EgressAllowlist::new(vec!["api.example.com".into()]);
    allowlist.proxy_port = Some(proxy_port);
    spec.net = nika_kernel::process::NetPolicy::Allowlist(allowlist);

    // 1. loopback on the PROXY port: reachable from inside the jail.
    let out = run(
        &spec,
        "/usr/bin/nc",
        &["-w", "2", "127.0.0.1", &proxy_port.to_string()],
    );
    assert!(
        out.status.success(),
        "the proxy port must be reachable under allowlist: {out:?}"
    );

    // 2. loopback on any OTHER port: EPERM (the fence) — not a TCP-level
    //    "Connection refused" (a listener is present), the sandbox verdict.
    //    `-v` is load-bearing: Apple nc (macOS 15.6 verified) prints the
    //    connect error ONLY in verbose mode — without it the EPERM is a
    //    silent exit 1 and the stderr assertion below cannot read it.
    let out = run(
        &spec,
        "/usr/bin/nc",
        &["-v", "-w", "2", "127.0.0.1", &other_port.to_string()],
    );
    assert!(!out.status.success(), "another port must be fenced");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Operation not permitted"),
        "the refusal is the seatbelt EPERM, not a refused TCP: {out:?}"
    );

    // 3. the DENY arm: loopback outbound is allowed (the carve-out) — the
    //    same connect succeeds where pre-tri-state deny admitted nothing.
    let out = run(
        &SandboxSpec::new(),
        "/usr/bin/nc",
        &["-w", "2", "127.0.0.1", &other_port.to_string()],
    );
    assert!(
        out.status.success(),
        "deny admits loopback outbound (the srt posture): {out:?}"
    );
}
