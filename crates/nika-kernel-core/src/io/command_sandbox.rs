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

use super::process::{SandboxSpec, ShellCommand};

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
