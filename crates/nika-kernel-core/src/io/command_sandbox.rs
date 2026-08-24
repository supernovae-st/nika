// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command sandbox seam — OS confinement for the `exec` verb's CHILD process.
//!
//! Distinct from the reserved capability `Sandbox` (`plugin::sandbox` · WASM
//! / MCP · the `enter()` self-restriction model · v0.100). This trait
//! confines a SPAWNED subprocess to a declared filesystem + network boundary
//! ([`super::process::SandboxSpec`], derived from `permits`).
//!
//! ## The wrapper model (no `unsafe`)
//!
//! Rather than an in-process `pre_exec` closure (which needs `unsafe`, banned
//! workspace-wide), an impl TRANSFORMS the command into its confined form by
//! wrapping it in the platform sandbox launcher — `sandbox-exec -p <profile>`
//! on macOS (`nika-sandbox-seatbelt`), `bwrap …` / a Landlock helper on Linux
//! (`nika-sandbox-landlock`). The same model every coding-agent CLI uses
//! (Claude Code · Codex · Cursor). [`CommandSandbox::confine`] is a pure,
//! synchronous transform: `ShellCommand` → wrapped `ShellCommand`.
//!
//! ## Ordering (load-bearing)
//!
//! The runner applies the blocklist floor to the ORIGINAL command, THEN
//! confines, THEN spawns — so the floor always inspects the real command, and
//! the wrapped form (`sandbox-exec -p … -- <real command>`) is spawned
//! directly. Confining first would hide the real command behind the launcher.

use super::process::{SandboxSpec, ShellAdapterOutcome, ShellCommand};

/// Confine a spawned command to an OS sandbox derived from the spec.
///
/// Object-safe (used as `Arc<dyn CommandSandbox>`). Synchronous — the
/// transform is pure (build the launcher argv + profile); mechanism
/// availability is the impl's own concern (returned as [`CommandSandboxError::
/// Unavailable`] · fail-closed · the caller decides whether to refuse or run
/// unconfined).
pub trait CommandSandbox: Send + Sync {
    /// Wrap `command` in the platform sandbox launcher per `spec`, returning
    /// the confined command to spawn. The result runs the SAME program/args,
    /// confined to the declared filesystem + network reach.
    ///
    /// # Errors
    ///
    /// [`CommandSandboxError::Unavailable`] when the platform sandbox
    /// mechanism is not present (fail-closed) · [`CommandSandboxError::
    /// Profile`] when the spec cannot be expressed as a platform profile.
    fn confine(
        &self,
        spec: &SandboxSpec,
        command: ShellCommand,
    ) -> Result<ShellCommand, CommandSandboxError>;

    /// A short, stable name of the backend (`"seatbelt"` · `"landlock"` ·
    /// `"noop"`) for diagnostics + the capability report.
    fn backend(&self) -> &'static str;

    /// Classify a drained launcher result before capture policy sees it.
    ///
    /// A sandbox wrapper and its inner process share one OS exit-status
    /// channel. Backends therefore own the conservative table that
    /// distinguishes a launcher/authority refusal from a business-process
    /// exit. Unknown backends fail closed on non-zero rather than allowing
    /// `capture: structured` to reinterpret an unclassified refusal.
    fn classify_outcome(&self, status: i32, _stderr: &str) -> ShellAdapterOutcome {
        if status == 0 {
            ShellAdapterOutcome::process()
        } else {
            ShellAdapterOutcome::authority_refusal(format!(
                "unclassified `{}` sandbox outcome (status {status}) was refused",
                self.backend()
            ))
        }
    }
}

/// True when drained stderr is a kernel/sandbox denial, not authored output.
///
/// `capture: structured` turns a business non-zero into data. A confinement
/// EPERM/EACCES is not a business non-zero (#1068): `cat` denied by the jail
/// prints `Operation not permitted` / `Permission denied` and exits 1, which
/// the structured path used to render as ✔. Conservative on purpose — a
/// program that prints these phrases under a jail is treated as authority.
/// Status 0 never uses this helper; backends keep a zero as process.
#[must_use]
pub fn stderr_signals_confinement_denial(stderr: &str) -> bool {
    stderr.lines().map(str::trim_start).any(|line| {
        line.contains("Operation not permitted")
            || line.contains("Permission denied")
            || line.contains("file system sandbox blocked")
            || line.contains("Read-only file system")
    })
}

/// Errors from confining a command.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum CommandSandboxError {
    /// The platform sandbox mechanism is unavailable (missing launcher · an
    /// unsupported OS · an old kernel). Fail-closed: the caller decides
    /// whether to refuse the command or run it unconfined.
    #[error("command sandbox unavailable: {reason}")]
    Unavailable {
        /// Why the sandbox could not be applied.
        reason: String,
    },

    /// The spec cannot be expressed as a platform sandbox profile.
    #[error("command sandbox profile error: {reason}")]
    Profile {
        /// What about the spec could not be expressed.
        reason: String,
    },
}

/// A no-op sandbox — returns the command UNCHANGED (unconfined).
///
/// The explicit, auditable "no confinement" choice for platforms without a
/// shipped backend, or where the operator has opted out. NEVER the silent
/// default: a caller selects it deliberately (the wiring layer logs it), so
/// "unconfined" is always a visible decision, not an accident.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopSandbox;

impl CommandSandbox for NoopSandbox {
    fn confine(
        &self,
        _spec: &SandboxSpec,
        command: ShellCommand,
    ) -> Result<ShellCommand, CommandSandboxError> {
        Ok(command)
    }

    fn backend(&self) -> &'static str {
        "noop"
    }

    fn classify_outcome(&self, _status: i32, _stderr: &str) -> ShellAdapterOutcome {
        // Reaching Noop is the composition layer's explicit
        // NIKA_SANDBOX=off waiver. It has no launcher whose status could be
        // an authority refusal, so every status is the process status.
        ShellAdapterOutcome::process()
    }
}

/// Fold a permit's literal path prefix to the form the KERNEL will see,
/// so a system-root check can compare against reality instead of text.
///
/// Returns `None` when the path can never be a stable confinement —
/// relative, empty, or carrying a `..` the backend cannot express.
///
/// Both sandbox backends used to trim trailing slashes and reject `..`,
/// then compare the raw string against their root list. Two spellings
/// survived that and named a system root anyway (2026-08-02):
///
/// ```text
///   /root/*      REFUSED          /root/./x*    granted as "/root/."
///   /etc/passwd* REFUSED          //etc/passwd* granted as "//etc"
/// ```
///
/// The kernel resolves `.` and collapses `//`, so both became a
/// read-write bind of the host's system root INTO the jail — the exact
/// thing each crate's doc comment promises is unrepresentable, and
/// reachable through any permit an author did not write themselves.
///
/// A normalizer that stops before a fixed point cannot feed an
/// exact-match check. This one reaches it: segments are rebuilt, empties
/// (from `//`) and `.` dropped, `..` refused outright.
///
/// # Symlinks are the kernel's job, and it does it
///
/// This fold is LEXICAL · it never resolves a link, which raises the
/// obvious question: does a permit naming `/tmp/link/**` grant whatever
/// the link points at, a system root included?
///
/// No. Measured on macOS 2026-08-02 against the real `sandbox-exec`
/// with this crate's own preamble:
///
/// ```text
///   grant the LINK    → reading through the link  REFUSED
///   grant the LINK    → reading the target        REFUSED
///   grant the TARGET  → reading the target        allowed
///   grant the TARGET  → reading through the link  allowed
/// ```
///
/// Seatbelt canonicalizes the path before matching a `subpath`, so a
/// grant follows the REAL location and a link is not a way to name
/// something the boundary refuses. The failure direction is the safe
/// one: naming a link grants nothing at all.
///
/// The Linux half (bwrap binds a literal path, which the mount
/// namespace then resolves) is unmeasured here · this machine has no
/// bwrap. Stated rather than assumed.
/// True when `folded` names one of `roots`, comparing the way the
/// KERNEL will.
///
/// The comparison is case-INSENSITIVE, and that is not defensive
/// styling. macOS ships a case-insensitive filesystem by default, where
/// `/ETC` and `/etc` are the same inode (verified: identical inode
/// numbers, both symlinks to `private/etc`). The root list is spelled
/// in lower case, so an exact match let `/ETC/passwd*` and `/ROOT/x*`
/// straight through the guard that exists to stop exactly that, on the
/// very platform the seatbelt backend serves (2026-08-02).
///
/// Linux is case-sensitive, so folding case there can only ever refuse
/// a path that does not exist. Refusing more on the stricter reading is
/// the safe direction for a boundary.
#[must_use]
pub fn names_system_root(folded: &str, roots: &[&str]) -> bool {
    roots.iter().any(|r| folded.eq_ignore_ascii_case(r))
}

#[must_use]
pub fn fold_sandbox_prefix(prefix: &str) -> Option<String> {
    if !prefix.starts_with('/') || prefix.contains('\0') {
        return None;
    }
    let mut out = String::with_capacity(prefix.len());
    for seg in prefix.split('/') {
        match seg {
            "" | "." => {}
            ".." => return None,
            s => {
                out.push('/');
                out.push_str(s);
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::process::{ShellError, ShellResult};

    fn apply(outcome: ShellAdapterOutcome) -> Result<ShellResult, ShellError> {
        ShellResult::new(126, r#"{"ok":true}"#, "", std::time::Duration::ZERO)
            .with_adapter_outcome(outcome)
            .into_process_result()
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn types_are_send_sync_and_object_safe() {
        _assert_send_sync::<CommandSandboxError>();
        _assert_send_sync::<NoopSandbox>();
        let _: Box<dyn CommandSandbox> = Box::new(NoopSandbox);
    }

    #[test]
    fn noop_returns_the_command_unchanged() {
        let cmd = ShellCommand::new("echo").arg("hi");
        let out = NoopSandbox
            .confine(&SandboxSpec::new(), cmd.clone())
            .expect("noop never fails");
        assert_eq!(out.program, "echo");
        assert_eq!(out.args, vec!["hi".to_owned()]);
        assert_eq!(NoopSandbox.backend(), "noop");
        assert!(
            apply(NoopSandbox.classify_outcome(126, r#"{"ok":true}"#)).is_ok(),
            "the explicit waiver has no launcher status to reinterpret"
        );
    }

    #[derive(Debug)]
    struct UnsupportedSandbox;

    impl CommandSandbox for UnsupportedSandbox {
        fn confine(
            &self,
            _spec: &SandboxSpec,
            command: ShellCommand,
        ) -> Result<ShellCommand, CommandSandboxError> {
            Ok(command)
        }

        fn backend(&self) -> &'static str {
            "unsupported"
        }
    }

    #[test]
    fn an_unknown_sandbox_adapter_fails_closed_on_nonzero() {
        assert!(matches!(
            apply(UnsupportedSandbox.classify_outcome(126, r#"{"ok":true}"#)),
            Err(ShellError::Blocked { .. })
        ));
        let zero = ShellResult::new(0, "", "", std::time::Duration::ZERO)
            .with_adapter_outcome(UnsupportedSandbox.classify_outcome(0, ""))
            .into_process_result();
        assert!(zero.is_ok());
    }

    #[test]
    fn error_display_is_clear() {
        let e = CommandSandboxError::Unavailable {
            reason: "sandbox-exec not found".into(),
        };
        assert!(e.to_string().contains("unavailable"));
        let e = CommandSandboxError::Profile {
            reason: "bad path".into(),
        };
        assert!(e.to_string().contains("profile"));
    }

    /// The #1068 discriminant: inner `cat` EPERM is confinement; an authored
    /// non-zero is not. A mutant that matches every stderr, or none of it,
    /// dies here — this is the table `capture: structured` must consult.
    #[test]
    fn inner_eperm_stderr_is_confinement_and_business_stderr_is_not() {
        assert!(stderr_signals_confinement_denial(
            "cat: secret/key.txt: Operation not permitted\n"
        ));
        assert!(stderr_signals_confinement_denial(
            "cat: secret/key.txt: Permission denied\n"
        ));
        assert!(stderr_signals_confinement_denial(
            "touch: out.txt: Read-only file system\n"
        ));
        assert!(stderr_signals_confinement_denial(
            "dyld[1]: Library not loaded: /opt/homebrew/lib/x.dylib\n  Reason: file system sandbox blocked open()\n"
        ));
        assert!(
            !stderr_signals_confinement_denial("tests failed"),
            "an authored non-zero must stay data under capture: structured"
        );
        assert!(
            !stderr_signals_confinement_denial(""),
            "empty stderr is not a denial diagnostic"
        );
    }
}

#[cfg(test)]
mod fold_tests {
    use super::fold_sandbox_prefix;

    /// Every spelling that reaches a system root must FOLD to that root,
    /// so the callers' exact-match check sees it. The first two rows are
    /// the live escape of 2026-08-02.
    #[test]
    fn every_spelling_of_a_system_root_folds_to_it() {
        for (spelled, want) in [
            ("/root/.", "/root"),
            ("//etc", "/etc"),
            ("/home/.", "/home"),
            ("/root/.//.", "/root"),
            ("///usr///", "/usr"),
            ("/etc/", "/etc"),
            ("/./etc", "/etc"),
        ] {
            assert_eq!(
                fold_sandbox_prefix(spelled).as_deref(),
                Some(want),
                "{spelled} must fold to {want} or the root check cannot see it"
            );
        }
    }

    /// A legitimate subpath keeps its shape — folding must not widen or
    /// narrow what the author declared.
    #[test]
    fn a_real_subpath_survives_folding() {
        for (spelled, want) in [
            ("/etc/myapp", "/etc/myapp"),
            ("/home/me/project", "/home/me/project"),
            ("/data/./out", "/data/out"),
            ("/var/log/app/", "/var/log/app"),
        ] {
            assert_eq!(fold_sandbox_prefix(spelled).as_deref(), Some(want));
        }
    }

    /// What can never be a stable confinement.
    #[test]
    fn unconfinable_paths_fold_to_nothing() {
        for bad in [
            "relative/path",
            "./out",
            "~/x",
            "$VAR/x",
            "/",
            "//",
            "/././",
            "/etc/../root",
            "/a/../..",
            "/nul\0byte",
        ] {
            assert!(
                fold_sandbox_prefix(bad).is_none(),
                "{bad:?} must not fold to a confinable path"
            );
        }
    }
}
