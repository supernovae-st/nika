// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-sandbox-landlock` — the Linux command sandbox (the `CommandSandbox`
//! seam · spec 01 §permits · ADR-095 Layer 6).
//!
//! Confines the `exec` verb's CHILD process by wrapping it in the `bwrap`
//! (bubblewrap) launcher with a mount + namespace jail generated from the
//! workflow's [`SandboxSpec`] (derived from `permits.fs` / `permits.net`). The
//! wrapper model (the same one Claude Code / Codex / Cursor use, and the exact
//! model the macOS sibling `nika-sandbox-seatbelt` uses) needs NO `unsafe` and
//! NO FFI — this crate only builds the launcher argv; the runner spawns the
//! result. (Landlock LSM hardening — the unprivileged in-kernel path-filter —
//! is the named follow-on; bwrap's namespaces are the shipping floor and share
//! this crate's spec-to-grant logic.)
//!
//! ## What the jail enforces (deny-default)
//!
//! - **Network** — the [`NetPolicy`] tri-state: `Deny` keeps the network
//!   namespace unshared (`--unshare-all` — loopback included, the stricter
//!   reading of the loopback-only floor: bwrap cannot bring `lo` up alone);
//!   `Allow` re-shares it (`--share-net`, the explicit escape hatch);
//!   `Allowlist` re-shares it too and the child gets the egress proxy's env
//!   contract (the Anthropic sandbox-runtime model — the proxy in
//!   `nika-exec-runner` serves exactly the declared `permits.net.http` set).
//!   HONEST LIMIT, stated loudly: bwrap cannot express a loopback-only
//!   fence, so on Linux the allowlist arm's OS floor is the env contract —
//!   an env-honoring client (curl · wget · node · python) is confined to
//!   the allowlist, while a client that strips its proxy env reaches
//!   anything (unlike macOS, where the Seatbelt port fence closes that).
//!   srt's own answer — `--unshare-net` + a socat bridge over a
//!   bind-mounted unix socket pair — is the named follow-on that brings
//!   Linux to the same strength.
//! - **Writes** — allowed ONLY under the declared `fs_write` prefixes plus
//!   scratch (`--bind`). Everything else is read-only or absent.
//! - **Reads** — the system paths every binary + the dynamic linker need are
//!   bound read-only (`--ro-bind`, else nothing runs); the declared `fs_read`
//!   prefixes are added read-only; SENSITIVE user paths (`~/.ssh`, `~/.aws`,
//!   arbitrary `$HOME` files) are bound NOWHERE, so they are absent from the
//!   jail's mount namespace (deny-default · the strongest form of read-deny).
//!
//! ## Coarseness (honest limits · identical to the sibling)
//!
//! `permits.fs` globs are gitignore-style; a bwrap `--bind` is a literal path.
//! This crate binds the glob's literal prefix — a COARSENING (the jail admits
//! at least the declared reach). The precise glob check is `permits_fit`'s
//! static job; the sandbox is the OS FLOOR (no network · no out-of-bounds
//! writes · no sensitive reads), path-prefix granularity, not per-file.
//!
//! ONE ASYMMETRY with the macOS sibling, stated loudly: an EXACT-file grant
//! binds exactly that file, so a `SQLite` database granted by its file path
//! CANNOT run WAL here — the `-wal`/`-shm`/`-journal` sidecars are
//! same-stem siblings created AT RUNTIME, and bwrap has no future-file bind
//! (`--bind-try` skips what does not exist yet; binding the parent directory
//! would over-grant the whole tree). The Seatbelt backend expresses the
//! family precisely as three exact-path `literal` filters; bwrap cannot.
//! The honest Linux contract: **a database grant names its directory**
//! (`/data/db-dir/**`), never the bare file.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

use nika_kernel::command_sandbox::{
    CommandSandbox, CommandSandboxError, fold_sandbox_prefix, names_system_root,
};
use nika_kernel::process::{NetPolicy, SandboxSpec, ShellAdapterOutcome, ShellCommand};

/// The bubblewrap launcher. A fixed absolute path (not `$PATH`) so a hijacked
/// `PATH` cannot point the sandbox at an impostor launcher.
const LAUNCHER: &str = "/usr/bin/bwrap";

/// System trees every dynamically-linked binary needs bound read-only to even
/// start (the linker, shared libs, the shell). Absent trees are silently
/// skipped at wrap time (a minimal container may lack `/sbin`), so binding a
/// non-existent path is never an error.
const SYSTEM_RO_BINDS: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/etc/alternatives",
    "/etc/ld.so.cache",
];

/// The Linux command sandbox (`bwrap` + a generated namespace jail).
#[derive(Debug, Clone, Copy, Default)]
pub struct LandlockSandbox;

impl LandlockSandbox {
    /// Construct the Linux sandbox.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Whether the bwrap launcher is present (Linux + the binary exists).
    /// On any non-Linux target this is `false` (fail-closed).
    #[must_use]
    pub fn available() -> bool {
        available_given(cfg!(target_os = "linux"), Path::new(LAUNCHER).exists())
    }
}

impl CommandSandbox for LandlockSandbox {
    fn confine(
        &self,
        spec: &SandboxSpec,
        command: ShellCommand,
    ) -> Result<ShellCommand, CommandSandboxError> {
        if !Self::available() {
            return Err(CommandSandboxError::Unavailable {
                reason: format!("{LAUNCHER} not available on this host"),
            });
        }
        let jail = build_jail_args(spec)?;
        Ok(wrap(command, jail))
    }

    fn backend(&self) -> &'static str {
        "landlock"
    }

    fn classify_outcome(&self, status: i32, stderr: &str) -> ShellAdapterOutcome {
        classify_terminal_outcome(status, stderr)
    }
}

/// Bubblewrap's wrapper-status table, kept pure so every host can verify the
/// Linux decision without pretending to execute a Linux jail locally.
fn classify_terminal_outcome(status: i32, stderr: &str) -> ShellAdapterOutcome {
    let launcher_diagnostic = stderr
        .lines()
        .map(str::trim_start)
        .any(|line| line.starts_with("bwrap:"));
    if status == 126 || launcher_diagnostic {
        ShellAdapterOutcome::authority_refusal(format!(
            "landlock/bwrap refused the confined process (status {status})"
        ))
    } else {
        ShellAdapterOutcome::process()
    }
}

/// The availability DECISION, pure — Linux AND the launcher binary both
/// present, never either alone (fail-closed). Split from
/// [`LandlockSandbox::available`] so the truth table is testable on EVERY
/// platform (the same Gate-5 discipline as the macOS sibling).
fn available_given(is_linux: bool, launcher_exists: bool) -> bool {
    is_linux && launcher_exists
}

/// Build the bwrap jail argv (deny-default · see module doc) from the spec.
///
/// The base is a fully unshared namespace with a fresh `/proc` + `/tmp`, the
/// system trees bound read-only, network denied. Declared reads add `--ro-bind`,
/// declared writes add `--bind`; `$HOME` is never bound, so sensitive home files
/// are simply absent from the jail.
fn build_jail_args(spec: &SandboxSpec) -> Result<Vec<String>, CommandSandboxError> {
    // Fresh namespaces · deny-default. `--unshare-all` drops net/ipc/pid/uts/
    // cgroup/user; `--die-with-parent` ties the child's lifetime to the runner;
    // a minimal virtual filesystem gets a fresh proc + dev + a private tmp.
    let mut a: Vec<String> = [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();

    // System trees read-only (skip any that do not exist on this host).
    for tree in SYSTEM_RO_BINDS {
        if Path::new(tree).exists() {
            a.push("--ro-bind".to_owned());
            a.push((*tree).to_owned());
            a.push((*tree).to_owned());
        }
    }

    // Declared reads · read-only binds under the validated literal prefix.
    for glob in &spec.fs_read {
        let Some(prefix) = grant_subpath(glob)? else {
            continue; // a glob with no literal prefix is un-expressible as a bind
        };
        a.push("--ro-bind-try".to_owned());
        a.push(prefix.clone());
        a.push(prefix);
    }

    // Declared writes · read-write binds under the validated literal prefix.
    for glob in &spec.fs_write {
        let Some(prefix) = grant_subpath(glob)? else {
            continue;
        };
        a.push("--bind-try".to_owned());
        a.push(prefix.clone());
        a.push(prefix);
    }

    // Network: `--unshare-all` already dropped the net namespace. The ALLOW
    // arms re-share it and bind the resolver config read-only — Allow (the
    // explicit escape hatch) and Allowlist, which shares the namespace so
    // the loopback egress proxy is reachable and serves the declared hosts
    // via the env contract (the srt model · bwrap cannot fence loopback-only
    // — the socat bridge is the named follow-on · module doc). Every other
    // arm — Deny and, by the #[non_exhaustive] law, any future variant —
    // fails CLOSED: the namespace stays unshared, no network physically.
    if matches!(spec.net, NetPolicy::Allow | NetPolicy::Allowlist(_)) {
        a.push("--share-net".to_owned());
        if Path::new("/etc/resolv.conf").exists() {
            a.push("--ro-bind".to_owned());
            a.push("/etc/resolv.conf".to_owned());
            a.push("/etc/resolv.conf".to_owned());
        }
    }

    Ok(a)
}

/// Wrap a command in `bwrap <jail args> -- <inner argv>`.
///
/// The inner invocation is reconstructed faithfully: the shell form becomes
/// `/bin/sh -c <line>` (jailed), the argv form runs the program directly.
/// `cwd` / `env` / `stdin` / `timeout` ride on the OUTER command so they apply
/// to the launcher and are inherited by the confined child. `pre_validated` is
/// set (the blocklist floor already ran on the ORIGINAL command; the wrapped
/// launcher argv must not be re-scanned).
fn wrap(command: ShellCommand, jail: Vec<String>) -> ShellCommand {
    let inner: Vec<String> = if command.shell {
        let line = if command.args.is_empty() {
            command.program.clone()
        } else {
            format!("{} {}", command.program, command.args.join(" "))
        };
        vec!["/bin/sh".to_owned(), "-c".to_owned(), line]
    } else {
        let mut v = Vec::with_capacity(1 + command.args.len());
        v.push(command.program.clone());
        v.extend(command.args.iter().cloned());
        v
    };

    let mut wrapped = ShellCommand::new(LAUNCHER);
    let mut args = Vec::with_capacity(jail.len() + 1 + inner.len());
    args.extend(jail);
    args.push("--".to_owned());
    args.extend(inner);
    wrapped.args = args;
    wrapped.shell = false;
    wrapped.cwd = command.cwd;
    wrapped.env = command.env;
    wrapped.env_passthrough = command.env_passthrough;
    wrapped.stdin = command.stdin;
    wrapped.timeout = command.timeout;
    wrapped.pre_validated = true; // the original already passed the floor; the launcher is benign
    wrapped.sandbox = None; // already confined
    wrapped
}

/// The grant subpath for a glob: its literal prefix, VALIDATED so the floor
/// holds even against a hostile or wrong permit (the sandbox's whole job). The
/// transform that turns a declared glob into a real OS grant must never be able
/// to express a whole-filesystem or system-root grant. (Identical soundness
/// contract to the macOS sibling's `grant_subpath` — the two backends validate
/// permits the same way.)
///
/// - `Ok(None)` — the glob has no literal prefix (`**/x` · `*`) — skipped.
/// - `Ok(Some(p))` — a safe absolute path.
/// - `Err(Profile)` — the prefix would over-grant or is non-canonical: root
///   `/`, a non-absolute / `~` / `$`-bearing path, a `..` traversal, or a bare
///   system-root directory (`/etc`, `/usr`, `/home`, … — a filename glob that
///   trims to one of these would bind the whole tree). Fail-closed: the caller
///   maps this to a refusal to spawn.
fn grant_subpath(glob: &str) -> Result<Option<String>, CommandSandboxError> {
    let prefix = literal_prefix(glob);
    if prefix.is_empty() {
        return Ok(None);
    }
    let refuse = |why: &str| {
        Err(CommandSandboxError::Profile {
            reason: format!("permits path {glob:?} cannot be confined: {why}"),
        })
    };
    if !prefix.starts_with('/') {
        // rejects relative (`./out`, `data/`), `~/…`, and `$VAR/…` — bwrap has
        // no shell expansion, so these would not match the canonical path.
        return refuse("a sandbox path must be absolute (canonicalize it first)");
    }
    if prefix.contains('\0') {
        return refuse("a NUL byte cannot be expressed in a bwrap bind argument");
    }
    // Fold to what the KERNEL will see before comparing. Trimming
    // trailing slashes is not a fixed point: `/root/./x*` trimmed to
    // `/root/.` and `//etc/passwd*` to `//etc`, neither of which is a
    // string in SYSTEM_ROOTS — and both of which the kernel resolves
    // straight back to the root, binding it read-write into the jail
    // (2026-08-02). One fold, shared with the seatbelt sibling, so the
    // two cannot drift apart on the answer.
    let Some(folded) = fold_sandbox_prefix(&prefix) else {
        return refuse("this path cannot be expressed as a stable bind");
    };
    if names_system_root(&folded, SYSTEM_ROOTS) {
        return refuse("a bare system-root directory would over-bind its whole tree");
    }
    Ok(Some(folded))
}

/// The literal directory prefix of a gitignore-style glob — everything before
/// the first glob metacharacter, trimmed back to the last path separator so a
/// directory boundary is kept. `./output/**` -> `./output`; `/data/lo*` ->
/// `/data`; `/data/x.txt` -> `/data/x.txt`; `**/y` -> empty (no literal prefix).
/// (Byte-identical to the sibling's `literal_prefix`.)
fn literal_prefix(glob: &str) -> String {
    let cut = glob.find(['*', '?', '[']).unwrap_or(glob.len());
    let head = &glob[..cut];
    match head.rfind('/') {
        Some(slash) if cut < glob.len() => head[..slash].to_owned(),
        _ => head.to_owned(),
    }
}

/// Bare system-root directories a permit must NOT bind (binding the whole tree
/// would defeat the jail). A filename glob that trims to one of these
/// (`/etc/passwd*` -> `/etc`) is refused; the author must declare a more
/// specific subpath (`/etc/myapp/**`). The Linux root set (the macOS sibling
/// carries the Darwin equivalents).
const SYSTEM_ROOTS: &[&str] = &[
    "/etc", "/usr", "/bin", "/sbin", "/lib", "/lib64", "/var", "/opt", "/root", "/home", "/dev",
    "/proc", "/sys", "/run", "/tmp", "/boot", "/srv", "/mnt", "/media",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(outcome: ShellAdapterOutcome) -> Result<(), nika_kernel::ShellError> {
        nika_kernel::ShellResult::new(126, r#"{"ok":true}"#, "", std::time::Duration::ZERO)
            .with_adapter_outcome(outcome)
            .into_process_result()
            .map(|_| ())
    }

    #[test]
    fn terminal_outcome_table_is_platform_independent_and_fail_closed() {
        assert!(matches!(
            apply(classify_terminal_outcome(126, r#"{"ok":true}"#)),
            Err(nika_kernel::ShellError::Blocked { .. })
        ));
        assert!(matches!(
            apply(classify_terminal_outcome(
                1,
                "bwrap: setting up uid map: Permission denied"
            )),
            Err(nika_kernel::ShellError::Blocked { .. })
        ));
        assert!(apply(classify_terminal_outcome(7, "business validation failed")).is_ok());
    }

    /// The availability truth table — all four rows, platform-free.
    /// Kills Gate 5's three survivors: `-> true` (row 4 fails), `-> false`
    /// (row 1 fails), `&& → ||` (rows 2+3 fail).
    #[test]
    fn available_given_is_the_and_of_both_facts() {
        assert!(available_given(true, true));
        assert!(
            !available_given(true, false),
            "Linux without bwrap is UNAVAILABLE"
        );
        assert!(
            !available_given(false, true),
            "a launcher path on non-Linux is UNAVAILABLE"
        );
        assert!(!available_given(false, false));
    }

    /// A glob with no literal prefix yields no bind (not an error).
    #[test]
    fn no_literal_prefix_is_skipped() {
        assert_eq!(grant_subpath("**/x").unwrap(), None);
        assert_eq!(grant_subpath("*").unwrap(), None);
    }

    /// A declared absolute subpath becomes a bind target.
    #[test]
    fn absolute_prefix_is_granted() {
        assert_eq!(
            grant_subpath("/data/project/**").unwrap(),
            Some("/data/project".to_owned())
        );
        assert_eq!(
            grant_subpath("/data/x.txt").unwrap(),
            Some("/data/x.txt".to_owned())
        );
    }

    /// The soundness refusals — a permit can never bind the whole tree.
    #[test]
    fn over_grant_is_refused() {
        for hostile in [
            "/",
            "//",
            "/etc",
            "/usr",
            "/home",
            "/etc/passwd*",
            "relative/x",
            "~/s",
            "$HOME/x",
            "/a/../..",
            // macOS ships a case-insensitive filesystem: `/ETC` and
            // `/etc` are the SAME inode, so an exact-match root check
            // let these through the guard that exists to stop them
            // (2026-08-02). Linux is case-sensitive, where refusing
            // these can only ever refuse a path that does not exist.
            "/ETC/passwd*",
            "/ROOT/x*",
            "/Root/./x*",
            "/HOME//x*",
            // The escape of 2026-08-02. Every one of these named a
            // system root and was GRANTED: the check trimmed trailing
            // slashes and compared text, while the kernel folds `.` and
            // collapses `//`. `/root/./x*` became a read-write bind of
            // the host's `/root` inside the jail.
            "/root/./x*",
            "//etc/passwd*",
            "/home/./x*",
            "/root/.//x*",
            "/./usr/x*",
            "///var///x*",
        ] {
            assert!(
                grant_subpath(hostile).is_err(),
                "{hostile:?} must be refused"
            );
        }
    }

    /// The list is the contract — every entry, not the three the doc
    /// comment happens to name as examples. The old test tried exactly
    /// the prose's `/etc`, `/usr`, `/home`, so deleting any other root
    /// from the const left every test green.
    #[test]
    fn every_system_root_is_refused_in_every_spelling() {
        for root in SYSTEM_ROOTS {
            for spelled in [
                (*root).to_owned(),
                format!("{root}/"),
                format!("/{root}"),
                format!("{root}/./x*"),
                format!("{root}//x*"),
                format!("{root}/.//."),
                root.to_uppercase(),
                format!("{}/x*", root.to_uppercase()),
            ] {
                assert!(
                    grant_subpath(&spelled).is_err(),
                    "{spelled:?} reaches system root {root:?} and must be refused"
                );
            }
        }
    }

    #[test]
    fn literal_prefix_trims_to_directory_boundary() {
        assert_eq!(literal_prefix("./output/**"), "./output");
        assert_eq!(literal_prefix("/data/lo*"), "/data");
        assert_eq!(literal_prefix("/data/x.txt"), "/data/x.txt");
        assert_eq!(literal_prefix("**/y"), "");
    }

    /// The jail denies network by default and re-shares only on request.
    #[test]
    fn network_is_deny_default() {
        let base = build_jail_args(&SandboxSpec::new()).unwrap();
        assert!(
            base.iter().any(|a| a == "--unshare-all"),
            "base unshares everything"
        );
        assert!(
            !base.iter().any(|a| a == "--share-net"),
            "no network by default"
        );

        let mut net = SandboxSpec::new();
        net.net = NetPolicy::Allow;
        let with_net = build_jail_args(&net).unwrap();
        assert!(
            with_net.iter().any(|a| a == "--share-net"),
            "network re-shared on request"
        );
    }

    /// The allowlist arm rides the shared namespace + the proxy's env
    /// contract (the srt model — bwrap cannot fence loopback-only, so the
    /// env contract IS the Linux floor; the socat bridge is the named
    /// follow-on · module doc).
    #[test]
    fn allowlist_rides_the_shared_namespace_and_the_env_contract() {
        let mut spec = SandboxSpec::new();
        spec.net = NetPolicy::Allowlist(nika_kernel::process::EgressAllowlist::new(vec![
            "api.example.com".to_owned(),
        ]));
        let args = build_jail_args(&spec).unwrap();
        assert!(
            args.iter().any(|a| a == "--share-net"),
            "the allowlist arm shares the namespace so the loopback proxy is reachable"
        );
    }

    /// A declared write becomes a read-write bind; a declared read a ro-bind.
    #[test]
    fn declared_paths_become_binds() {
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec!["/data/in/**".to_owned()];
        spec.fs_write = vec!["/data/out/**".to_owned()];
        let args = build_jail_args(&spec).unwrap();
        let joined = args.join(" ");
        assert!(
            joined.contains("--ro-bind-try /data/in /data/in"),
            "read is a ro-bind: {joined}"
        );
        assert!(
            joined.contains("--bind-try /data/out /data/out"),
            "write is a rw-bind: {joined}"
        );
    }

    /// The wrap transform: argv form runs the program directly under bwrap.
    #[test]
    fn wrap_argv_runs_program_under_launcher() {
        let mut cmd = ShellCommand::new("/bin/echo");
        cmd.args = vec!["hi".to_owned()];
        let jail = build_jail_args(&SandboxSpec::new()).unwrap();
        let w = wrap(cmd, jail);
        assert_eq!(w.program, LAUNCHER);
        assert!(w.pre_validated, "the launcher argv must not be re-scanned");
        assert!(w.sandbox.is_none(), "already confined");
        // the inner program appears after the `--` separator
        let sep = w
            .args
            .iter()
            .position(|a| a == "--")
            .expect("a -- separator");
        assert_eq!(w.args[sep + 1], "/bin/echo");
        assert_eq!(w.args[sep + 2], "hi");
    }

    /// The shell form becomes `/bin/sh -c <line>` under the jail.
    #[test]
    fn wrap_shell_form_becomes_sh_c() {
        let mut cmd = ShellCommand::new("echo hi > f");
        cmd.shell = true;
        let w = wrap(cmd, build_jail_args(&SandboxSpec::new()).unwrap());
        let sep = w.args.iter().position(|a| a == "--").unwrap();
        assert_eq!(w.args[sep + 1], "/bin/sh");
        assert_eq!(w.args[sep + 2], "-c");
        assert_eq!(w.args[sep + 3], "echo hi > f");
    }

    #[test]
    fn backend_name_is_stable() {
        assert_eq!(LandlockSandbox::new().backend(), "landlock");
    }

    /// A path with an unescapable NUL is refused (the injection boundary).
    #[test]
    fn nul_in_path_is_refused() {
        assert!(grant_subpath("/data/\0evil").is_err());
    }
}
