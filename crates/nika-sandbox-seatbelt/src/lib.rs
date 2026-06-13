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
//! - **Network** — denied entirely unless `spec.allow_network` (Seatbelt host
//!   filtering is TLS-blind · host-granular needs a proxy · follow-on).
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
use nika_kernel::process::{SandboxSpec, ShellCommand};

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
        cfg!(target_os = "macos") && Path::new(LAUNCHER).exists()
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

/// Build the SBPL profile string from the spec (deny-default · see module doc).
fn build_profile(spec: &SandboxSpec) -> Result<String, CommandSandboxError> {
    use std::fmt::Write as _;
    let mut p = String::from(PROFILE_PREAMBLE);

    for glob in &spec.fs_read {
        let prefix = literal_prefix(glob);
        if prefix.is_empty() {
            continue; // a glob with no literal prefix is un-expressible as a subpath
        }
        let _ = writeln!(p, "(allow file-read* (subpath {}))", sbpl_string(&prefix)?);
    }

    for glob in &spec.fs_write {
        let prefix = literal_prefix(glob);
        if prefix.is_empty() {
            continue;
        }
        let _ = writeln!(
            p,
            "(allow file-write* file-read* (subpath {}))",
            sbpl_string(&prefix)?
        );
    }

    if spec.allow_network {
        p.push_str("(allow network*)\n");
    }
    // else: network stays denied by `(deny default)` — the load-bearing line.

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
    wrapped.env_remove = command.env_remove;
    wrapped.stdin = command.stdin;
    wrapped.timeout = command.timeout;
    wrapped.pre_validated = true; // the original already passed the floor; the launcher is benign
    wrapped.sandbox = None; // already confined
    wrapped
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
            "network denied by default"
        );

        let mut allow = SandboxSpec::new();
        allow.allow_network = true;
        assert!(build_profile(&allow).unwrap().contains("(allow network*)"));
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
