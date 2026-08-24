// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-sandbox-seatbelt` — the macOS command sandbox (the `CommandSandbox`
//! seam · spec 01 §permits · ADR-095 Layer 6).
//!
//! Confines the `exec` verb's CHILD process by wrapping it in the OS-shipped
//! `sandbox-exec` launcher with an SBPL profile generated from the workflow's
//! [`SandboxSpec`] (derived from `permits.fs` / `permits.net`). The wrapper
//! model (the same one Claude Code / Codex / Cursor use on macOS) needs NO
//! `unsafe` and NO FFI — this crate only builds a profile string and the
//! launcher argv; the runner spawns the result.
//!
//! ## What the profile enforces (deny-default)
//!
//! - **Network** — the [`NetPolicy`] tri-state (the Anthropic sandbox-runtime
//!   seatbelt model, verified live against `sandbox-exec`): `Deny` admits
//!   loopback outbound ONLY (`(allow network-outbound (remote ip
//!   "localhost:*"))` — no syscall goes beyond loopback); `Allow` emits
//!   `(allow network*)` (the explicit escape hatch); `Allowlist` admits
//!   outbound loopback scoped to the per-run egress proxy's PORT
//!   (`(remote ip "localhost:PORT")`) — the proxy (in `nika-exec-runner`)
//!   serves exactly the declared `permits.net.http` set and the child gets
//!   its env contract, because a Seatbelt host rule is TLS-blind: the
//!   profile fences the CHANNEL, the proxy fences the HOSTS.
//! - **Writes** — allowed ONLY under the declared `fs_write` prefixes plus
//!   the per-spawn private scratch the runner creates and grants (the child's
//!   `TMPDIR` — issue 754: the SHARED host tmp trees are no blanket grant
//!   anymore, they bypassed every declared boundary). Everything else (home,
//!   the repo, `/etc`, `/private/tmp`) is read-only-or-denied.
//! - **Reads** — the system paths every binary + the dynamic linker need are
//!   always allowed (else nothing runs); the declared `fs_read` prefixes are
//!   added; SENSITIVE user paths (`~/.ssh`, `~/.aws`, arbitrary home files)
//!   are NOT in any allow rule, so their CONTENTS are denied (deny-default).
//!
//! ## Coarseness (honest limits)
//!
//! `permits.fs` globs are gitignore-style; an SBPL `subpath` is a literal
//! prefix. This crate uses the glob's literal prefix as a `subpath` — a
//! COARSENING (the sandbox allows at least the declared reach). The precise
//! glob check is `permits_fit`'s static job; the sandbox is the OS FLOOR (no
//! network · no out-of-bounds writes · no sensitive reads), path-prefix
//! granularity, not per-file.
//!
//! The ONE same-directory extension: an EXACT-file grant (no glob
//! metacharacter) also admits its `SQLite` journal family — `<db>-wal`,
//! `<db>-shm` (WAL mode) and `<db>-journal` (rollback mode) — as three
//! exact-path `literal` filters on the same rule, access class inherited
//! (the `write_journal_sidecars` helper). `SQLite`'s atomicity model creates, locks,
//! mmaps and unlinks these same-stem siblings on every write, so a grant
//! naming only the main file dies with `SQLITE_CANTOPEN` (14) the moment a
//! journal materializes — verified live 2026-07-29 (macOS 15.6.1 · sqlite
//! 3.43.2: the bare file grant fails, the three literals pass WAL, rollback
//! and reopen modes, and an `ATTACH`ed database outside the grant stays
//! refused). The extension is bounded by construction — three exact literals
//! in the file's own directory, dead letters for a non-database file; the
//! directory itself is NOT granted, so no other sibling becomes reachable.
//!
//! And the walk behind it: a confined child must be able to canonicalize its
//! OWN location. Every relative open resolves through the process cwd, and
//! the libc `getcwd`/`realpath` path reads directory ENTRIES on the way —
//! under deny-default that read dies (`file-read-data` on the cwd, then on
//! the opened file's parent — the kernel denial log behind the same
//! finding). So the profile lists the child's cwd and every exact-file
//! grant's parent as `file-read-data` literals (directory LISTINGS only,
//! never file contents): names in those two dirs stop being the sandbox's
//! false positive, everything else stays denied.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

use nika_kernel::command_sandbox::{
    CommandSandbox, CommandSandboxError, fold_sandbox_prefix, names_system_root,
    stderr_signals_confinement_denial,
};
use nika_kernel::process::{NetPolicy, SandboxSpec, ShellAdapterOutcome, ShellCommand};

/// The OS-shipped Seatbelt launcher. A fixed absolute path (not `$PATH`) so a
/// hijacked `PATH` cannot point the sandbox at an impostor launcher.
const LAUNCHER: &str = "/usr/bin/sandbox-exec";

/// The macOS command sandbox (`sandbox-exec` + a generated SBPL profile).
#[derive(Debug, Clone, Copy, Default)]
pub struct SeatbeltSandbox;

impl SeatbeltSandbox {
    /// Construct the macOS sandbox.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Whether the Seatbelt launcher is present (macOS + the binary exists).
    /// On any non-macOS target this is `false` (fail-closed).
    #[must_use]
    pub fn available() -> bool {
        available_given(cfg!(target_os = "macos"), Path::new(LAUNCHER).exists())
    }
}

impl CommandSandbox for SeatbeltSandbox {
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
        let profile = build_profile(spec, confined_cwd(&command).as_deref())?;
        Ok(wrap(command, &profile))
    }

    fn backend(&self) -> &'static str {
        "seatbelt"
    }

    fn classify_outcome(&self, status: i32, stderr: &str) -> ShellAdapterOutcome {
        classify_terminal_outcome(status, stderr)
    }
}

/// Seatbelt's wrapper-status table. Status 0 is always the inner process.
/// Status 126 is reserved fail-closed (launcher/exec refusal, indistinguishable
/// from an inner 126). A `sandbox-exec:` line or an inner kernel EPERM/EACCES
/// (#1068 · `cat` denied by the jail) is authority at any other non-zero.
/// Remaining non-zero statuses stay authored process outcomes so
/// `capture: structured` can still branch on a program's own failure.
fn classify_terminal_outcome(status: i32, stderr: &str) -> ShellAdapterOutcome {
    if status == 0 {
        return ShellAdapterOutcome::process();
    }
    let launcher_diagnostic = stderr
        .lines()
        .map(str::trim_start)
        .any(|line| line.starts_with("sandbox-exec:"));
    if status == 126 || launcher_diagnostic || stderr_signals_confinement_denial(stderr) {
        ShellAdapterOutcome::authority_refusal(format!(
            "seatbelt refused the confined process (status {status})"
        ))
    } else {
        ShellAdapterOutcome::process()
    }
}

/// The working directory the confined child will actually run in: the
/// command's own `cwd` when set, else the runner's (spawn-inherit semantics
/// — a `None` cwd means the child inherits the spawning process's). The
/// profile must be able to list THAT directory (`file-read-data`), or every
/// relative open in the child dies on the `getcwd` walk (module doc
/// §Coarseness · the 2026-07-29 finding).
fn confined_cwd(command: &ShellCommand) -> Option<PathBuf> {
    command.cwd.clone().or_else(|| std::env::current_dir().ok())
}

/// The availability DECISION, pure — macOS AND the launcher binary both
/// present, never either alone (fail-closed). Split from [`SeatbeltSandbox::available`]
/// so the truth table is testable on EVERY platform: the binder reads the
/// real world (cfg! + fs), this owns the logic — Gate 5's surviving
/// mutants (`-> true` · `-> false` · `&& → ||`) all lived in the fused
/// form, unkillable on any single host.
fn available_given(is_macos: bool, launcher_exists: bool) -> bool {
    is_macos && launcher_exists
}

/// Build the SBPL profile string from the spec (deny-default · see module
/// doc). `cwd` is the directory the confined child will run in ([`confined_cwd`])
/// — listed as a `file-read-data` literal so relative opens survive the
/// `getcwd` walk.
fn build_profile(spec: &SandboxSpec, cwd: Option<&Path>) -> Result<String, CommandSandboxError> {
    use std::fmt::Write as _;
    let mut p = String::from(PROFILE_PREAMBLE);
    // Directory listings the child legitimately needs beyond its grants:
    // its own cwd (the getcwd walk) + each exact-file grant's parent (the
    // opened file's home — same-directory tooling scans it). LISTINGS only
    // (`file-read-data` on exact literals), never file contents.
    let mut listings = std::collections::BTreeSet::new();

    for glob in &spec.fs_read {
        let Some(prefix) = grant_subpath(glob)? else {
            continue; // a glob with no literal prefix is un-expressible as a subpath
        };
        let _ = write!(p, "(allow file-read* (subpath {})", sbpl_string(&prefix)?);
        write_journal_sidecars(&mut p, glob, &prefix, &mut listings)?;
        p.push_str(")\n");
    }

    for glob in &spec.fs_write {
        let Some(prefix) = grant_subpath(glob)? else {
            continue;
        };
        let _ = write!(
            p,
            "(allow file-write* file-read* (subpath {})",
            sbpl_string(&prefix)?
        );
        write_journal_sidecars(&mut p, glob, &prefix, &mut listings)?;
        p.push_str(")\n");
    }

    if let Some(dir) = cwd
        && dir != Path::new("/")
    {
        listings.insert(dir.to_string_lossy().into_owned());
    }

    if !listings.is_empty() {
        p.push_str("(allow file-read-data");
        for dir in &listings {
            let _ = write!(p, " (literal {})", sbpl_string(dir)?);
        }
        p.push_str(")\n");
    }

    // The network arms (the Anthropic sandbox-runtime seatbelt model —
    // verified live against sandbox-exec on macOS):
    //
    // - Allow (the explicit escape hatch): unrestricted — `(allow network*)`.
    // - Allowlist: outbound loopback ONLY, scoped to the egress proxy's port
    //   when the runner has started it (`proxy_port` — always filled by the
    //   runner; `None` only for a spec that never passed one). The allowlist
    //   itself is the proxy's job: a Seatbelt host rule is TLS-blind, so the
    //   profile fences the CHANNEL and the proxy fences the HOSTS.
    // - Deny (and, by the #[non_exhaustive] law, any future arm): loopback
    //   outbound only — no network syscall goes BEYOND loopback (the
    //   sandbox-runtime posture: local services stay reachable, egress is
    //   refused). Fail-closed, one rule, no exceptions.
    match &spec.net {
        NetPolicy::Allow => p.push_str("(allow network*)\n"),
        NetPolicy::Allowlist(allowlist) => {
            let scope = match allowlist.proxy_port {
                Some(port) => port.to_string(),
                None => "*".to_owned(),
            };
            let _ = writeln!(
                p,
                "(allow network-outbound (remote ip \"localhost:{scope}\"))"
            );
        }
        _ => p.push_str("(allow network-outbound (remote ip \"localhost:*\"))\n"),
    }

    Ok(p)
}

/// The `SQLite` durability family (module doc §Coarseness): when a grant names
/// an EXACT file — no glob metacharacter, so `literal_prefix` kept it whole
/// (`glob == prefix`), and no trailing slash (a directory grant already
/// covers same-dir sidecars) — append `<file>-wal`, `<file>-shm` and
/// `<file>-journal` as exact-path `literal` filters on the same rule, so the
/// sidecars inherit the file's access class. `SQLite`'s atomicity model
/// creates, locks, mmaps and unlinks these same-stem siblings on every
/// write; without them the confined open dies with `SQLITE_CANTOPEN` (14).
/// The file's PARENT is recorded in `listings` (a `file-read-data` grant —
/// its name list, never sibling contents) so same-directory tooling that
/// scans the file's home stops false-denying. The suffixes are constants and
/// every path passes through `sbpl_string` exactly like the main path, so
/// the injection boundary is unchanged.
fn write_journal_sidecars(
    p: &mut String,
    glob: &str,
    prefix: &str,
    listings: &mut std::collections::BTreeSet<String>,
) -> Result<(), CommandSandboxError> {
    use std::fmt::Write as _;
    if glob != prefix || prefix.ends_with('/') {
        return Ok(());
    }
    for suffix in JOURNAL_SIDECAR_SUFFIXES {
        let _ = write!(
            p,
            " (literal {})",
            sbpl_string(&format!("{prefix}{suffix}"))?
        );
    }
    if let Some(parent) = Path::new(prefix).parent()
        && parent != Path::new("/")
    {
        listings.insert(parent.to_string_lossy().into_owned());
    }
    Ok(())
}

/// The single-database journal sidecars `SQLite` keeps next to the main file
/// (WAL's `-wal` + `-shm`, the rollback `-journal`). The multi-database
/// super-journal (`<db>-mj*`) is deliberately out: an `ATTACH`ed database
/// needs its own declared grant, so the transaction that would need one is
/// already fenced at the attach.
const JOURNAL_SIDECAR_SUFFIXES: &[&str] = &["-wal", "-shm", "-journal"];

/// Wrap a command in `sandbox-exec -p <profile> -- <inner argv>`.
///
/// The inner invocation is reconstructed faithfully: the shell form becomes
/// `/bin/sh -c <line>` (sandboxed), the argv form runs the program directly.
/// `cwd` / `env` / `stdin` / `timeout` ride on the OUTER command so they apply
/// to the launcher and are inherited by the confined child. `pre_validated` is
/// set (the blocklist floor already ran on the ORIGINAL command, and the
/// wrapped launcher argv must not be re-scanned).
fn wrap(command: ShellCommand, profile: &str) -> ShellCommand {
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
    let mut args = Vec::with_capacity(3 + inner.len());
    args.push("-p".to_owned());
    args.push(profile.to_owned());
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
/// to express a whole-filesystem or system-root grant (review P1-1/P1-2/P2-1).
///
/// - `Ok(None)` — the glob has no literal prefix (`**/x` · `*`) — skipped.
/// - `Ok(Some(p))` — a safe absolute subpath at least two segments deep.
/// - `Err(Profile)` — the prefix would over-grant or is non-canonical:
///   root `/`, a non-absolute / `~` / `$`-bearing path (SBPL does not expand
///   them · they would match unreliably), a `..` traversal, or a bare
///   system-root directory (`/etc`, `/usr`, `/Users`, … — a filename glob that
///   trims to one of these would grant the whole tree). Fail-closed: the
///   caller (the runner) maps this to a refusal to spawn.
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
        // rejects relative (`./out`, `data/`), `~/…`, and `$VAR/…` — SBPL has
        // no shell expansion, so these would not match the canonical path.
        return refuse("a sandbox path must be absolute (canonicalize it first)");
    }
    // Fold to what the KERNEL will see before comparing — see the
    // landlock sibling for the escape this closes. One fold, shared, so
    // the two backends cannot answer differently.
    let Some(folded) = fold_sandbox_prefix(&prefix) else {
        return refuse("this path cannot be expressed as a stable subpath");
    };
    if names_system_root(&folded, SYSTEM_ROOTS) {
        return refuse("a bare system-root directory would over-grant its whole tree");
    }
    Ok(Some(folded))
}

/// The literal directory prefix of a gitignore-style glob — everything before
/// the first glob metacharacter, trimmed back to the last path separator so a
/// directory boundary is kept. `./output/**` -> `./output`; `/data/lo*` ->
/// `/data`; `/data/x.txt` -> `/data/x.txt`; `**/y` -> empty (no literal prefix).
fn literal_prefix(glob: &str) -> String {
    let cut = glob.find(['*', '?', '[']).unwrap_or(glob.len());
    let head = &glob[..cut];
    match head.rfind('/') {
        Some(slash) if cut < glob.len() => head[..slash].to_owned(),
        _ => head.to_owned(),
    }
}

/// Bare system-root directories a permit must NOT grant as a subpath (granting
/// the whole tree would defeat the jail). A filename glob that trims to one of
/// these (`/etc/passwd*` -> `/etc`) is refused (review P2-1); the author must
/// declare a more specific subpath (`/etc/myapp/**`).
const SYSTEM_ROOTS: &[&str] = &[
    "/etc",
    "/usr",
    "/bin",
    "/sbin",
    "/var",
    "/private",
    "/System",
    "/Library",
    "/Users",
    "/opt",
    "/root",
    "/home",
    "/dev",
    "/tmp",
    "/Applications",
    "/Volumes",
    "/cores",
    "/net",
];

/// Quote a path as an SBPL string literal, escaping the two metacharacters
/// (`\` and `"`) that could otherwise BREAK OUT of the string and inject
/// profile directives (the profile-injection boundary). A control character
/// cannot be safely escaped in an SBPL string, so it is REFUSED.
fn sbpl_string(path: &str) -> Result<String, CommandSandboxError> {
    if path.chars().any(char::is_control) {
        return Err(CommandSandboxError::Profile {
            reason: "a sandbox path contains a control character (cannot be expressed in SBPL)"
                .to_owned(),
        });
    }
    let mut out = String::with_capacity(path.len() + 2);
    out.push('"');
    for c in path.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Ok(out)
}

/// The fixed deny-default preamble: allow the minimum every program needs to
/// START (exec/fork, the dynamic linker's system reads, the device-file
/// sinks), then `build_profile` appends the declared reach. Network + writes
/// + sensitive reads stay denied by `(deny default)`.
///
/// The shared host tmp trees (`/private/tmp`, `/private/var/tmp` and
/// `/private/var/folders`) are NOT here (issue 754): a blanket grant on them
/// bypassed every declared `permits.fs` boundary — the runner now hands each
/// confined spawn its OWN per-spawn scratch (the child's `TMPDIR`, granted
/// via `fs_write` like any other prefix), and an author who genuinely wants
/// the shared `/tmp` declares it.
///
/// `/opt/homebrew` IS here (2026-08-18): it is the interpreter's home on
/// Apple-Silicon Macs the way `/usr/local` is on Intel Macs, and `(subpath
/// "/usr")` already covered the latter — one architecture had program space,
/// the other did not. Without it a Homebrew `bash`/`node`/`python3` first
/// on PATH aborts at dyld (`Library not loaded … libreadline.8.dylib · file
/// system sandbox blocked open()`) under ANY `permits:` block, and a
/// `capture: structured` leg renders that abort as a ✔ with `exit_code:
/// -1` — measured on the studio's own daily ledger (12 runs · 36 legs · zero
/// measured). READ only, like `/usr`: the child still cannot read the
/// workspace, write anywhere, or reach the network without a declared
/// grant. Other package-manager prefixes (`/nix/store` · `MacPorts`
/// `/opt/local`) are the same class and are NOT granted here — unmeasured,
/// they wait for their own probe rather than ride this one.
const PROFILE_PREAMBLE: &str = r#"(version 1)
(deny default)
(allow process-exec*)
(allow process-fork)
(allow signal (target self))
(allow sysctl-read)
(allow mach-lookup)
(allow file-read-metadata)
(allow file-read* file-read-metadata
    (subpath "/usr")
    (subpath "/opt/homebrew")
    (subpath "/bin")
    (subpath "/sbin")
    (subpath "/System")
    (subpath "/Library")
    (subpath "/private/var/db/dyld")
    (subpath "/private/var/db/timezone")
    (subpath "/private/etc")
    (subpath "/dev")
    (literal "/"))
(allow file-write-data
    (literal "/dev/null")
    (literal "/dev/zero")
    (literal "/dev/stdout")
    (literal "/dev/stderr")
    (literal "/dev/dtracehelper")
    (literal "/dev/tty"))
"#;

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
    fn terminal_outcome_table_keeps_authority_ahead_of_capture() {
        assert!(matches!(
            apply(classify_terminal_outcome(126, r#"{"ok":true}"#)),
            Err(nika_kernel::ShellError::Blocked { .. })
        ));
        assert!(matches!(
            apply(classify_terminal_outcome(
                1,
                "sandbox-exec: deny(1) file-read-data"
            )),
            Err(nika_kernel::ShellError::Blocked { .. })
        ));
        assert!(
            matches!(
                apply(classify_terminal_outcome(
                    1,
                    "cat: secret/key.txt: Operation not permitted\n"
                )),
                Err(nika_kernel::ShellError::Blocked { .. })
            ),
            "#1068: inner cat EPERM at status 1 is confinement, not structured data"
        );
        assert!(
            apply(classify_terminal_outcome(7, "business validation failed")).is_ok(),
            "an ordinary non-zero remains business data"
        );
        assert!(
            apply(classify_terminal_outcome(0, "Operation not permitted")).is_ok(),
            "status 0 is the inner process even if stderr mentions EPERM"
        );
    }

    /// The availability truth table — all four rows, platform-free.
    /// Kills Gate 5's three survivors: `-> true` (row 4 fails), `->
    /// false` (row 1 fails), `&& → ||` (rows 2+3 fail).
    #[test]
    fn available_given_is_the_and_of_both_facts() {
        assert!(available_given(true, true));
        assert!(
            !available_given(true, false),
            "macOS without the launcher is UNAVAILABLE"
        );
        assert!(
            !available_given(false, true),
            "a launcher path on non-macOS is UNAVAILABLE"
        );
        assert!(!available_given(false, false));
    }

    /// The binder reflects THIS platform's truth (belt over the seam).
    #[test]
    fn available_binder_matches_the_real_world() {
        let expected = cfg!(target_os = "macos") && Path::new(LAUNCHER).exists();
        assert_eq!(SeatbeltSandbox::available(), expected);
    }

    /// On a macOS host with the launcher, confine PROCEEDS — the wrapped
    /// command execs the launcher, not the original program. Kills the
    /// `delete !` mutant (which would return Unavailable exactly here).
    #[test]
    fn confine_proceeds_when_available() {
        if !SeatbeltSandbox::available() {
            return; // linux CI: the truth-table test carries the logic
        }
        let spec = SandboxSpec::default();
        let cmd = ShellCommand::new("/usr/bin/true");
        let wrapped = SeatbeltSandbox::new()
            .confine(&spec, cmd)
            .expect("available host confines");
        assert_eq!(wrapped.program, LAUNCHER);
    }

    #[test]
    fn literal_prefix_extracts_the_directory_head() {
        assert_eq!(literal_prefix("/data/**"), "/data");
        assert_eq!(literal_prefix("/data/out/*.txt"), "/data/out");
        assert_eq!(literal_prefix("/data/x.txt"), "/data/x.txt");
        assert_eq!(literal_prefix("/data/lo*"), "/data");
        assert_eq!(literal_prefix("**/y"), "");
        assert_eq!(literal_prefix("./output/**"), "./output");
    }

    #[test]
    fn sbpl_string_escapes_quote_and_backslash() {
        assert_eq!(sbpl_string("/a/b").unwrap(), "\"/a/b\"");
        assert_eq!(sbpl_string("/a\"b").unwrap(), "\"/a\\\"b\"");
        assert_eq!(sbpl_string("/a\\b").unwrap(), "\"/a\\\\b\"");
    }

    #[test]
    fn sbpl_string_refuses_a_control_char_path() {
        assert!(matches!(
            sbpl_string("/a\nb"),
            Err(CommandSandboxError::Profile { .. })
        ));
        assert!(matches!(
            sbpl_string("/a\0b"),
            Err(CommandSandboxError::Profile { .. })
        ));
    }

    #[test]
    fn profile_denies_network_by_default_and_allows_when_granted() {
        let denied = build_profile(&SandboxSpec::new(), None).unwrap();
        assert!(denied.contains("(deny default)"));
        assert!(
            !denied.contains("(allow network*)"),
            "no unrestricted network by default"
        );
        assert!(
            denied.contains("(allow network-outbound (remote ip \"localhost:*\"))"),
            "the deny arm admits loopback outbound only (the srt posture)"
        );

        let mut allow = SandboxSpec::new();
        allow.net = NetPolicy::Allow;
        assert!(
            build_profile(&allow, None)
                .unwrap()
                .contains("(allow network*)")
        );
    }

    #[test]
    fn allowlist_fences_outbound_loopback_to_the_proxy_port() {
        // The srt seatbelt line: the channel is fenced to the proxy's port;
        // the proxy (not the profile) fences the hosts — a Seatbelt host
        // rule is TLS-blind.
        let mut spec = SandboxSpec::new();
        let mut allowlist =
            nika_kernel::process::EgressAllowlist::new(vec!["api.example.com".to_owned()]);
        allowlist.proxy_port = Some(60080);
        spec.net = NetPolicy::Allowlist(allowlist);
        let p = build_profile(&spec, None).unwrap();
        assert!(
            p.contains("(allow network-outbound (remote ip \"localhost:60080\"))"),
            "port-scoped fence: {p}"
        );
        assert!(!p.contains("(allow network*)"), "never unrestricted");

        // A spec that never passed the runner (no proxy yet) degrades to
        // loopback-any — fail-closed, the allowlist simply cannot be served.
        let mut spec = SandboxSpec::new();
        spec.net = NetPolicy::Allowlist(nika_kernel::process::EgressAllowlist::new(vec![
            "api.example.com".to_owned(),
        ]));
        let p = build_profile(&spec, None).unwrap();
        assert!(p.contains("(allow network-outbound (remote ip \"localhost:*\"))"));
    }

    #[test]
    fn profile_emits_declared_reads_and_writes() {
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec!["/data/in/**".to_owned()];
        spec.fs_write = vec!["/data/out/**".to_owned()];
        let p = build_profile(&spec, None).unwrap();
        assert!(p.contains("(allow file-read* (subpath \"/data/in\"))"));
        assert!(p.contains("(allow file-write* file-read* (subpath \"/data/out\"))"));
    }

    /// The `SQLite` durability family (the 2026-07-29 finding, closed): an
    /// EXACT-file write grant carries `-wal` / `-shm` / `-journal` as exact
    /// literals on its own rule — without them a confined WAL open dies with
    /// `SQLITE_CANTOPEN` (14).
    #[test]
    fn an_exact_file_write_grant_carries_its_journal_sidecars() {
        let mut spec = SandboxSpec::new();
        spec.fs_write = vec!["/data/state.db".to_owned()];
        let p = build_profile(&spec, None).unwrap();
        assert!(
            p.contains(
                "(allow file-write* file-read* (subpath \"/data/state.db\") \
                 (literal \"/data/state.db-wal\") (literal \"/data/state.db-shm\") \
                 (literal \"/data/state.db-journal\"))"
            ),
            "the durability family rides the file's own rule: {p}"
        );
    }

    /// The read side inherits the family as READ-ONLY literals — the access
    /// class follows the grant, never widened by the sidecars.
    #[test]
    fn an_exact_file_read_grant_carries_read_only_sidecars() {
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec!["/data/state.db".to_owned()];
        let p = build_profile(&spec, None).unwrap();
        assert!(
            p.contains(
                "(allow file-read* (subpath \"/data/state.db\") \
                 (literal \"/data/state.db-wal\") (literal \"/data/state.db-shm\") \
                 (literal \"/data/state.db-journal\"))"
            ),
            "read-only sidecars on the read rule: {p}"
        );
        assert!(
            !p.contains("file-write* file-read* (subpath \"/data/state.db\""),
            "the sidecars never smuggle a write into a read grant: {p}"
        );
    }

    /// A directory-shaped grant (a `**` glob · a trailing-slash path) already
    /// covers same-dir sidecars — NO literal is added (no profile bloat, and
    /// the exact-file extension stays the only same-directory reach).
    #[test]
    fn directory_grants_add_no_sidecar_literals() {
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec!["/data/in/**".to_owned()];
        spec.fs_write = vec!["/data/out/".to_owned()];
        let p = build_profile(&spec, None).unwrap();
        assert!(
            !p.contains("-wal") && !p.contains("-journal"),
            "directory grants already cover their sidecars: {p}"
        );
    }

    /// The sidecar literals cross the same injection boundary as any path:
    /// a quote-bearing base is emitted ESCAPED, suffix included — the three
    /// literals stay inert string content, never live directives.
    #[test]
    fn sidecar_literals_are_escaped_like_any_path() {
        let mut spec = SandboxSpec::new();
        spec.fs_write = vec!["/data/x\"y.db".to_owned()];
        let p = build_profile(&spec, None).unwrap();
        let escaped_wal = sbpl_string("/data/x\"y.db-wal").unwrap();
        assert!(
            p.contains(&format!("(literal {escaped_wal})")),
            "the sidecar literal is one escaped string: {p}"
        );
    }

    /// The getcwd walk (the finding's second half): the child's OWN cwd is
    /// listed as a `file-read-data` literal — a directory LISTING, never file
    /// contents — so a relative open in the child stops dying on the walk.
    #[test]
    fn the_child_cwd_is_listed_as_read_data_only() {
        let p = build_profile(&SandboxSpec::new(), Some(Path::new("/data/project"))).unwrap();
        assert!(
            p.contains("(allow file-read-data (literal \"/data/project\"))"),
            "the cwd listing is emitted: {p}"
        );
        assert!(
            !p.contains("file-read* (subpath \"/data/project\")")
                && !p.contains("file-read* file-read-metadata\n    (subpath \"/data/project\")"),
            "the listing never widens to contents: {p}"
        );
        // No cwd → no listing rule at all (the profile stays minimal).
        let p = build_profile(&SandboxSpec::new(), None).unwrap();
        assert!(
            !p.contains("(allow file-read-data"),
            "no cwd, no listing: {p}"
        );
        // A root cwd is already the preamble's `(literal "/")` — not re-emitted.
        let p = build_profile(&SandboxSpec::new(), Some(Path::new("/"))).unwrap();
        assert!(
            !p.contains("(allow file-read-data"),
            "the root listing is the preamble's own: {p}"
        );
    }

    /// An exact-file grant lists its PARENT (names only — same-directory
    /// tooling scans the opened file's home); a directory grant adds nothing
    /// (its subpath already covers the listing). Two files in one directory
    /// emit the parent ONCE.
    #[test]
    fn an_exact_file_grant_lists_its_parent_directory_once() {
        let mut spec = SandboxSpec::new();
        spec.fs_write = vec!["/data/state.db".to_owned(), "/data/other.db".to_owned()];
        let p = build_profile(&spec, None).unwrap();
        let needle = "(literal \"/data\")";
        assert_eq!(
            p.matches(needle).count(),
            1,
            "the shared parent is listed exactly once: {p}"
        );
        assert!(
            p.contains("(allow file-read-data (literal \"/data\"))"),
            "as read-data only, never contents: {p}"
        );

        let mut spec = SandboxSpec::new();
        spec.fs_write = vec!["/data/out/**".to_owned()];
        let p = build_profile(&spec, None).unwrap();
        assert!(
            !p.contains("(allow file-read-data"),
            "a directory grant's subpath already covers its listing: {p}"
        );
    }

    /// The confined child runs in the command's own `cwd` when set, else
    /// inherits the runner's — the listing follows the SAME directory the
    /// child's relative opens will resolve against.
    #[test]
    fn confined_cwd_prefers_the_command_then_the_runner() {
        let mut cmd = ShellCommand::new("/usr/bin/true");
        cmd.cwd = Some(PathBuf::from("/data/project"));
        assert_eq!(confined_cwd(&cmd), Some(PathBuf::from("/data/project")));
        let cmd = ShellCommand::new("/usr/bin/true");
        assert_eq!(
            confined_cwd(&cmd),
            std::env::current_dir().ok(),
            "a None cwd inherits the runner's own directory"
        );
    }

    #[test]
    fn grant_subpath_refuses_an_over_granting_or_non_canonical_permit() {
        // The floor must make a whole-filesystem / system-root / non-canonical
        // grant IMPOSSIBLE to express — even from a hostile or wrong permit
        // (review P1-1/P1-2/P2-1). Each of these is fail-closed (Profile error).
        for over in [
            "/",                  // whole filesystem (P1-1)
            "//",                 // root, trailing-slash form
            "/etc/passwd*",       // trims to the /etc system root (P2-1)
            "/Users/*",           // trims to /Users (every home)
            "/usr/**",            // bare system root
            "./output/**",        // relative — SBPL can't canonicalize (P1-2)
            "../shared/**",       // parent traversal
            "~/.aws/**",          // ~ is not expanded by SBPL (P3-1)
            "$HOME/secrets/**",   // $VAR is not expanded by SBPL
            "/data/../../etc/x*", // a `..` escape
        ] {
            assert!(
                matches!(
                    grant_subpath(over),
                    Err(CommandSandboxError::Profile { .. })
                ),
                "permit {over:?} must be refused (fail-closed), not granted"
            );
        }
    }

    #[test]
    fn grant_subpath_allows_a_specific_absolute_permit() {
        // A genuinely-scoped permit (≥2 absolute segments, not a system root)
        // is granted as its directory subpath.
        assert_eq!(
            grant_subpath("/data/project/in/**").unwrap(),
            Some("/data/project/in".to_owned())
        );
        assert_eq!(
            grant_subpath("/srv/app/cache/x.txt").unwrap(),
            Some("/srv/app/cache/x.txt".to_owned())
        );
        // A glob with no literal prefix is SKIPPED (safe · no grant emitted),
        // not an error — `**/y` and root-level globs like `/*` `/**`.
        assert_eq!(grant_subpath("**/y").unwrap(), None);
        assert_eq!(grant_subpath("/*").unwrap(), None);
        assert_eq!(grant_subpath("/**").unwrap(), None);
    }

    /// Issue 754 — the blanket `(allow file-read* file-write* (subpath
    /// "/private/tmp"))` family bypassed every declared `permits.fs`
    /// boundary. The empty spec's profile must not name the shared tmp
    /// trees at all; a DECLARED grant re-admits exactly its own subpath.
    #[test]
    fn the_shared_host_tmp_is_no_ambient_grant() {
        let p = build_profile(&SandboxSpec::new(), None).expect("profile");
        for tree in ["/private/tmp", "/private/var/tmp", "/private/var/folders"] {
            assert!(
                !p.contains(tree),
                "the empty profile must not grant {tree}:\n{p}"
            );
        }
        let mut spec = SandboxSpec::new();
        spec.fs_write = vec!["/private/tmp/nika-x/**".to_owned()];
        let p = build_profile(&spec, None).expect("profile");
        assert!(
            p.contains("(subpath \"/private/tmp/nika-x\")"),
            "a declared tmp subpath is granted exactly:\n{p}"
        );
    }

    /// The interpreter's home is PROGRAM space, read-only, on both Mac
    /// architectures. `(subpath "/usr")` already covers `/usr/local` (the
    /// Intel Homebrew prefix); `/opt/homebrew` (the Apple-Silicon prefix)
    /// was not in the preamble, so a Homebrew `bash`/`node`/`python3` first
    /// on PATH died at dyld (`Library not loaded … libreadline.8.dylib ·
    /// file system sandbox blocked open()`) under any `permits:` block —
    /// measured 2026-08-18: 12 daily runs of the studio's own ledger rendered
    /// 36 ✔ legs whose captured `exit_code` was -1, and the same leg exits 0
    /// once the prefix is readable. Read only · never a write · never a
    /// declared-grant substitute (the child still cannot read the workspace
    /// without `fs.read`).
    #[test]
    fn the_apple_silicon_homebrew_prefix_is_program_space_like_usr_local() {
        let p = build_profile(&SandboxSpec::new(), None).expect("profile");
        let preamble_reads = p
            .split("(allow file-write-data")
            .next()
            .expect("the read block precedes the write block");
        assert!(
            preamble_reads.contains("(subpath \"/opt/homebrew\")"),
            "the Apple-Silicon Homebrew prefix is readable program space:\n{p}"
        );
        assert!(
            preamble_reads.contains("(subpath \"/usr\")"),
            "the Intel prefix (/usr/local) stays covered by /usr:\n{p}"
        );
        assert!(
            !p.contains("file-write* file-read* (subpath \"/opt/homebrew\")"),
            "program space is never writable:\n{p}"
        );
    }

    /// Live, macOS-only, and honest about its own applicability: if a
    /// Homebrew bash is installed at the Apple-Silicon prefix and the
    /// launcher exists, a confined `bash -c true` must START (exit 0). Where
    /// either is absent the test says so and passes vacuously — a skip that
    /// names itself, never a green that looked at nothing.
    #[test]
    #[cfg(target_os = "macos")]
    // The launcher IS the seam under test: this test spawns `sandbox-exec`
    // itself to prove the profile lets a real interpreter start · the
    // kernel `ShellExecutor` seam sits ABOVE this crate and would hide the
    // very thing being measured (the nika-cli-host git.rs precedent).
    #[allow(clippy::disallowed_types)]
    fn a_homebrew_interpreter_starts_under_the_seatbelt() {
        let brew_bash = Path::new("/opt/homebrew/bin/bash");
        if !brew_bash.exists() || !SeatbeltSandbox::available() {
            // a skip that names itself · never a green that looked at nothing
            return;
        }
        let profile = build_profile(&SandboxSpec::new(), None).expect("profile");
        let status = std::process::Command::new(LAUNCHER)
            .arg("-p")
            .arg(&profile)
            .arg("--")
            .arg(brew_bash)
            .arg("-c")
            .arg("true")
            .status()
            .expect("the launcher spawns");
        assert!(
            status.success(),
            "a Homebrew bash must start under the empty profile · status {status}"
        );
    }

    #[test]
    fn a_root_write_permit_fails_the_whole_profile() {
        // The end-to-end fail-closed: a spec asking to write `/` does not yield
        // a permissive profile — build_profile refuses it.
        let mut spec = SandboxSpec::new();
        spec.fs_write = vec!["/".to_owned()];
        assert!(matches!(
            build_profile(&spec, None),
            Err(CommandSandboxError::Profile { .. })
        ));
    }

    #[test]
    fn profile_injection_via_a_malicious_path_is_refused() {
        // Two independent boundaries defend the profile against a path crafted
        // to inject an SBPL directive (no glob metachar in these payloads, so
        // `literal_prefix` returns them whole and the escaping/refusal runs).

        // (1) A newline cannot be a valid path char in an SBPL string and is
        //     refused at the control-char boundary → Profile error.
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec!["/data\n(allow system-socket)".to_owned()];
        assert!(matches!(
            build_profile(&spec, None),
            Err(CommandSandboxError::Profile { .. })
        ));

        // (2) A quote break-out is ESCAPED — the whole payload is emitted as
        //     escaped string content inside ONE `(subpath "<escaped>")` filter
        //     (plus, this being an exact-file grant, its three escaped journal
        //     sidecars), so the injected `(allow system-socket)` is inert
        //     string content, not a top-level form. Proven by reconstructing
        //     the escaped literals: the profile must contain precisely them
        //     (their internal quotes are `\"`).
        let payload = "/data\") (allow system-socket) (subpath \"/etc";
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec![payload.to_owned()];
        let p = build_profile(&spec, None).unwrap();
        let escaped = sbpl_string(payload).unwrap();
        assert!(
            escaped.contains("\\\""),
            "the payload's quotes are escaped: {escaped}"
        );
        let escaped_wal = sbpl_string(&format!("{payload}-wal")).unwrap();
        assert!(
            p.contains(&format!(
                "(allow file-read* (subpath {escaped}) (literal {escaped_wal})"
            )),
            "the payload and its sidecars are escaped string content, not live directives: {p}"
        );
    }

    #[test]
    fn wrap_argv_form_runs_program_directly() {
        let cmd = ShellCommand::new("cat").arg("/data/x");
        let w = wrap(cmd, "(version 1)(deny default)");
        assert_eq!(w.program, LAUNCHER);
        assert!(!w.shell);
        assert!(w.pre_validated, "the launcher argv must not be re-scanned");
        assert!(w.sandbox.is_none(), "already confined");
        assert_eq!(w.args[0], "-p");
        assert_eq!(w.args[2], "--");
        assert_eq!(w.args[3], "cat");
        assert_eq!(w.args[4], "/data/x");
    }

    #[test]
    fn wrap_shell_form_reconstructs_sh_dash_c() {
        let mut cmd = ShellCommand::new("echo hi | wc -l");
        cmd.shell = true;
        let w = wrap(cmd, "(version 1)");
        assert_eq!(w.args[3], "/bin/sh");
        assert_eq!(w.args[4], "-c");
        assert_eq!(w.args[5], "echo hi | wc -l");
        assert!(!w.shell, "the OUTER command runs the launcher directly");
    }

    #[test]
    fn backend_name_is_stable() {
        assert_eq!(SeatbeltSandbox::new().backend(), "seatbelt");
    }
}
