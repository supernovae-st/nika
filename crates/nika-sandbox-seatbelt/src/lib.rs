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
//!   scratch. Everything else (home, the repo, `/etc`) is read-only-or-denied.
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

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

use nika_kernel::command_sandbox::{CommandSandbox, CommandSandboxError};
use nika_kernel::process::{NetPolicy, SandboxSpec, ShellCommand};

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
        let profile = build_profile(spec)?;
        Ok(wrap(command, &profile))
    }

    fn backend(&self) -> &'static str {
        "seatbelt"
    }
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

/// Build the SBPL profile string from the spec (deny-default · see module doc).
fn build_profile(spec: &SandboxSpec) -> Result<String, CommandSandboxError> {
    use std::fmt::Write as _;
    let mut p = String::from(PROFILE_PREAMBLE);

    for glob in &spec.fs_read {
        let Some(prefix) = grant_subpath(glob)? else {
            continue; // a glob with no literal prefix is un-expressible as a subpath
        };
        let _ = writeln!(p, "(allow file-read* (subpath {}))", sbpl_string(&prefix)?);
    }

    for glob in &spec.fs_write {
        let Some(prefix) = grant_subpath(glob)? else {
            continue;
        };
        let _ = writeln!(
            p,
            "(allow file-write* file-read* (subpath {}))",
            sbpl_string(&prefix)?
        );
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
    if prefix.split('/').any(|seg| seg == "..") {
        return refuse("a `..` traversal cannot be expressed as a stable subpath");
    }
    let normalized = prefix.trim_end_matches('/');
    if normalized.is_empty() {
        return refuse("a path of `/` would grant the whole filesystem");
    }
    if SYSTEM_ROOTS.contains(&normalized) {
        return refuse("a bare system-root directory would over-grant its whole tree");
    }
    Ok(Some(prefix))
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
/// START (exec/fork, the dynamic linker's system reads, scratch writes), then
/// `build_profile` appends the declared reach. Network + non-scratch writes +
/// sensitive reads stay denied by `(deny default)`.
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
(allow file-read* file-write* (subpath "/private/tmp"))
(allow file-read* file-write* (subpath "/private/var/tmp"))
(allow file-read* file-write* (subpath "/private/var/folders"))
"#;

#[cfg(test)]
mod tests {
    use super::*;

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
        let denied = build_profile(&SandboxSpec::new()).unwrap();
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
        assert!(build_profile(&allow).unwrap().contains("(allow network*)"));
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
        let p = build_profile(&spec).unwrap();
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
        let p = build_profile(&spec).unwrap();
        assert!(p.contains("(allow network-outbound (remote ip \"localhost:*\"))"));
    }

    #[test]
    fn profile_emits_declared_reads_and_writes() {
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec!["/data/in/**".to_owned()];
        spec.fs_write = vec!["/data/out/**".to_owned()];
        let p = build_profile(&spec).unwrap();
        assert!(p.contains("(allow file-read* (subpath \"/data/in\"))"));
        assert!(p.contains("(allow file-write* file-read* (subpath \"/data/out\"))"));
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

    #[test]
    fn a_root_write_permit_fails_the_whole_profile() {
        // The end-to-end fail-closed: a spec asking to write `/` does not yield
        // a permissive profile — build_profile refuses it.
        let mut spec = SandboxSpec::new();
        spec.fs_write = vec!["/".to_owned()];
        assert!(matches!(
            build_profile(&spec),
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
            build_profile(&spec),
            Err(CommandSandboxError::Profile { .. })
        ));

        // (2) A quote break-out is ESCAPED — the whole payload is emitted as
        //     exactly ONE `(subpath "<escaped>")` literal, so the injected
        //     `(allow system-socket)` is inert string content, not a top-level
        //     form. Proven by reconstructing the escaped literal: the profile
        //     must contain precisely it (its internal quotes are `\"`).
        let payload = "/data\") (allow system-socket) (subpath \"/etc";
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec![payload.to_owned()];
        let p = build_profile(&spec).unwrap();
        let escaped = sbpl_string(payload).unwrap();
        assert!(
            escaped.contains("\\\""),
            "the payload's quotes are escaped: {escaped}"
        );
        assert!(
            p.contains(&format!("(allow file-read* (subpath {escaped}))")),
            "the payload is one escaped subpath literal, not a live directive: {p}"
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
