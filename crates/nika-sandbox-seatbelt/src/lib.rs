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
//! metacharacter, a regular file or a cleanly absent path) also admits
//! its `SQLite` journal family — `<db>-wal`,
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
        ShellAdapterOutcome::authority_refusal(confinement_refusal_reason(status, stderr))
    } else {
        ShellAdapterOutcome::process()
    }
}

/// Name what the jail denied when stderr identifies it. Unknown operations
/// stay unknown: never recommend `permits.fs.read` unless the line is a
/// read denial.
fn confinement_refusal_reason(status: i32, stderr: &str) -> String {
    match classify_denial(stderr) {
        Denial::SandboxRead(path) => format!(
            "seatbelt refused the confined process (status {status}): stderr reports a read denial of `{path}` — check `permits.fs.read` and OS access"
        ),
        Denial::SandboxWrite(path) => format!(
            "seatbelt refused the confined process (status {status}): stderr reports a write denial of `{path}` — check `permits.fs.write` and OS access"
        ),
        Denial::Socket(path) => format!(
            "seatbelt refused the confined process (status {status}): `{path}` is a unix socket — the profile admits no socket"
        ),
        Denial::Named(path) => format!(
            "seatbelt refused the confined process (status {status}): stderr reports `{path}` was denied"
        ),
        Denial::Unknown => format!("seatbelt refused the confined process (status {status})"),
    }
}

enum Denial<'a> {
    /// `sandbox-exec` named a `file-read*` filter.
    SandboxRead(&'a str),
    /// `sandbox-exec` named a `file-write*` filter.
    SandboxWrite(&'a str),
    Socket(&'a str),
    /// A `prog: path: EPERM` line — class unknown (`cp` source vs dest).
    Named(&'a str),
    Unknown,
}

fn classify_denial(stderr: &str) -> Denial<'_> {
    for line in stderr.lines().map(str::trim_start) {
        if let Some(path) = unix_socket_on_denial_line(line) {
            return Denial::Socket(path);
        }
        if let Some(denial) = sandbox_exec_denial(line) {
            return denial;
        }
        if let Some(path) = unix_colon_path(line) {
            return Denial::Named(path);
        }
        if line.to_ascii_lowercase().contains("read-only file system") {
            return Denial::Unknown;
        }
    }
    Denial::Unknown
}

/// A `unix://` path on a line that is itself a confinement denial.
/// A URI in help text or an unrelated `.sock` filename is not a socket deny.
fn unix_socket_on_denial_line(line: &str) -> Option<&str> {
    if !line_is_denial(line) {
        return None;
    }
    let rest = line.split_once("unix://")?.1;
    let path = rest
        .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ')'))
        .next()
        .unwrap_or("");
    (path.starts_with('/') && path.contains('/')).then_some(path)
}

fn line_is_denial(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("operation not permitted")
        || lower.contains("permission denied")
        || lower.contains("sandbox-exec:")
        || lower.contains("file system sandbox blocked")
}

fn unix_colon_path(line: &str) -> Option<&str> {
    let lower = line.to_ascii_lowercase();
    let marker = [": operation not permitted", ": permission denied"]
        .into_iter()
        .find(|m| lower.ends_with(m))?;
    let rest = &line[..line.len() - marker.len()];
    let (_, path) = rest.split_once(": ")?;
    if path.is_empty() || path.contains('\0') {
        None
    } else {
        Some(path)
    }
}

fn sandbox_exec_denial(line: &str) -> Option<Denial<'_>> {
    let rest = line.strip_prefix("sandbox-exec:")?;
    let path = rest
        .split_whitespace()
        .find(|token| token.starts_with('/') || token.starts_with("./"))?;
    let lower = rest.to_ascii_lowercase();
    if lower.contains("file-read") {
        Some(Denial::SandboxRead(path))
    } else if lower.contains("file-write") {
        Some(Denial::SandboxWrite(path))
    } else {
        Some(Denial::Unknown)
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
/// (`glob == prefix`), no directory marker and a file-compatible identity
/// (an existing directory never gains siblings) — append `<file>-wal`,
/// `<file>-shm` and `<file>-journal` as exact-path `literal` filters, so the
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
    // Exactness AND directory-intent are properties of the ORIGINAL
    // GLOB, preserved through absolutization (KR-03): a directory
    // grant — any metacharacter (the literal prefix trimmed back to a
    // directory boundary), a trailing `/`, or a terminal `/.` / `/..` — emits
    // NEITHER the three journal siblings (they live OUTSIDE the
    // directory's subtree) NOR the parent listing. SandboxSpec preserves
    // that intent when absolutizing relative grants.
    // Testing the already-folded prefix cannot preserve it (the fold
    // erases both the trailing slash and the `/.`). An exact FILE
    // keeps its whole family on the canonical spelling.
    if literal_prefix(glob) != glob
        || glob.ends_with('/')
        || glob.ends_with("/.")
        || glob.ends_with("/..")
    {
        return Ok(());
    }
    if !journal_file_prefix(prefix).map_err(|reason| CommandSandboxError::Profile {
        reason: format!("permits path {glob:?} cannot grant journal siblings: {reason}"),
    })? {
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

/// A bare directory is still a directory. Never follow the FINAL component
/// to decide this extension: a final symlink must not borrow its target's
/// file type or gain siblings. Ancestor aliases were resolved by the grant
/// judge; an exact new file is legal only under the same healthy parent.
fn journal_file_prefix(prefix: &str) -> Result<bool, String> {
    match std::fs::symlink_metadata(prefix) {
        // seam-bypass-ok: profile-build-time final-entry judgment, without following it
        Ok(metadata) if metadata.is_dir() => Ok(false),
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(format!(
            "an exact grant must name a regular file or directory, not a final symlink or special entry: {prefix}"
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // ENOENT alone can hide an unresolved ancestor. Re-judge the
            // parent and refuse if its effective identity changed, instead
            // of silently treating a dangling/error path as a new file.
            let rechecked = effective_seatbelt_prefix(prefix)?;
            if rechecked != prefix {
                return Err(format!("the grant parent identity changed: {prefix}"));
            }
            Ok(true)
        }
        Err(error) => Err(format!("cannot inspect exact grant {prefix}: {error}")),
    }
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
    let Some(lexical) = fold_sandbox_prefix(&prefix) else {
        return refuse("this path cannot be expressed as a stable subpath");
    };
    // KIMI-SEC-02 · NEP-0009 law 2 on the seatbelt arm: macOS seatbelt
    // matches paths CANONICALLY, so a rule spelled through a symlinked
    // ancestor (the system `/tmp` → `/private/tmp` link) never fires —
    // the identity judge tolerates a legitimately-symlinked ancestor and
    // the child resolves it too, so the confined write died on every
    // access (the check≡run≡jail parity break). Spell the rule in the
    // SAME effective form the judge compares against: longest EXISTING
    // ancestor canonicalized, final component lexical (a planted symlink
    // AT the prefix keeps its own name — access through it resolves
    // outside the rule and the floor holds), the genuinely-absent tail
    // folded lexically (NEP-0009 law 5 · KR-04: only independently
    // established ABSENCE may fold — every other resolution failure
    // refuses through the fallible profile boundary). Seatbelt-only:
    // the bwrap mount projection keeps the author's spelling.
    let folded =
        effective_seatbelt_prefix(&lexical).map_err(|reason| CommandSandboxError::Profile {
            reason: format!("permits path {glob:?} cannot be confined: {reason}"),
        })?;
    // KR-02 · the system-root guard reads the lexical form, the
    // effective form, AND the canonical identities of the protected
    // roots themselves: `/etc` IS `/private/etc` on macOS, so an alias
    // reaching `…/alias/etc` must refuse exactly like the literal
    // `/etc` — exact-match membership over all three sets, never a
    // widened grant, and the pre-existing direct-`/private/etc` gap
    // closes with it.
    if names_system_root(&lexical, SYSTEM_ROOTS)
        || names_system_root(&folded, SYSTEM_ROOTS)
        || canonical_system_roots()
            .iter()
            .any(|root| folded.eq_ignore_ascii_case(root))
    {
        return refuse("a bare system-root directory would over-grant its whole tree");
    }
    Ok(Some(folded))
}

/// The spelling macOS seatbelt can actually match: the parent chain's
/// longest EXISTING ancestor canonicalized (symlinked ancestors
/// absorbed), the FINAL component lexical (NEVER followed — a planted
/// symlink at the prefix keeps its own name, so access through it
/// resolves outside the rule; the derivation-side identity judge owns
/// the hard refusal), the genuinely-absent tail folded lexically
/// (KR-04 · see the helper for the absence-vs-error discrimination).
fn effective_seatbelt_prefix(prefix: &str) -> Result<String, String> {
    let path = Path::new(prefix);
    let Some(name) = path.file_name() else {
        // A bare root has no final component to protect.
        let root = std::fs::canonicalize(path) // seam-bypass-ok: profile-build-time fs judgment (the resolve_effective precedent)
            .map_err(|error| format!("cannot resolve grant root {prefix}: {error}"))?;
        return exact_seatbelt_path(root);
    };
    let mut effective_parent = effective_existing_ancestor(path.parent().unwrap_or(path))?;
    effective_parent.push(name);
    exact_seatbelt_path(effective_parent)
}

/// Authority cannot substitute replacement characters for a canonical identity.
/// The caller maps this failure to the existing fail-closed Profile boundary.
fn exact_seatbelt_path(path: PathBuf) -> Result<String, String> {
    path.into_os_string()
        .into_string()
        .map_err(|_| "canonical grant path is not representable as UTF-8".to_owned())
}

/// The longest existing ancestor of `dir`, canonicalized, with the
/// genuinely-absent tail folded back lexically. KR-04: resolution
/// failures are NOT absence — per level, `symlink_metadata` (never
/// follows the final component) discriminates a clean ENOENT (the
/// entry does not exist at all · the legal new-write tail, NEP-0009
/// law 5) from an entry that EXISTS but cannot be resolved (dangling
/// link · ELOOP · ENOTDIR · EACCES · anything else) — the latter
/// refuses through the fallible profile boundary, fail-closed.
fn effective_existing_ancestor(dir: &Path) -> Result<PathBuf, String> {
    let mut trailing: Vec<&std::ffi::OsStr> = Vec::new();
    let mut cur = dir;
    loop {
        match std::fs::canonicalize(cur) {
            // seam-bypass-ok: profile-build-time fs judgment (the resolve_effective precedent)
            Ok(canon) => {
                // This helper resolves a PARENT: even an empty `trailing`
                // has the grant's final component appended by the caller.
                // canonicalize succeeds on regular files too (KR-04).
                let metadata = std::fs::metadata(&canon) // seam-bypass-ok: same profile-build-time ancestor judgment
                    .map_err(|error| {
                        format!("cannot inspect ancestor {}: {error}", canon.display())
                    })?;
                if !metadata.is_dir() {
                    return Err(format!(
                        "grant ancestor is not a directory: {}",
                        canon.display()
                    ));
                }
                let mut out = canon;
                for name in trailing.iter().rev() {
                    out.push(name);
                }
                return Ok(out);
            }
            Err(resolve_err) => match std::fs::symlink_metadata(cur) {
                // seam-bypass-ok: same judgment, never following the final component
                Ok(_) => {
                    return Err(format!(
                        "a path component exists but cannot be resolved (dangling link, loop, or access failure): {}",
                        cur.display()
                    ));
                }
                Err(meta_err)
                    if resolve_err.kind() == std::io::ErrorKind::NotFound
                        && meta_err.kind() == std::io::ErrorKind::NotFound =>
                {
                    match cur.parent() {
                        Some(parent) if parent != cur => {
                            if let Some(name) = cur.file_name() {
                                trailing.push(name);
                            }
                            cur = parent;
                        }
                        _ => {
                            return Err(format!(
                                "no resolvable directory ancestor: {}",
                                dir.display()
                            ));
                        }
                    }
                }
                Err(meta_err) => {
                    return Err(format!(
                        "cannot resolve or inspect the grant prefix: {} · {resolve_err} · {meta_err}",
                        cur.display()
                    ));
                }
            },
        }
    }
}

/// The protected roots in their canonical identities (computed once):
/// on macOS `/etc` IS `/private/etc`, so matching only the literal
/// list lets an alias-spelled or canonically-spelled root through
/// (KR-02). A root that fails to resolve keeps its raw entry (the
/// guard is never widened by a resolution failure).
fn canonical_system_roots() -> &'static Vec<String> {
    static ROOTS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    ROOTS.get_or_init(|| {
        SYSTEM_ROOTS
            .iter()
            .map(|r| {
                std::fs::canonicalize(r) // seam-bypass-ok: profile-build-time fs judgment of the protected roots themselves
                    .map_or_else(|_| (*r).to_owned(), |c| c.to_string_lossy().into_owned())
            })
            .collect()
    })
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
mod tests;
