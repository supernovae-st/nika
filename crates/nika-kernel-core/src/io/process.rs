// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shell executor traits — ISP decomposition of process management.
//!
//! 2 atomic traits: `ShellRun`, `ShellCancel`.
//! 1 super-trait: `ShellExecutor` (blanket for both).
//!
//! Design decision #9: cancel is a `fn cancel(&self, id)` method,
//! not a `CancellationToken` field on `ShellCommand`. This keeps
//! tokio-util out of nika-kernel.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

/// The OS-confinement boundary for a spawned command (spec 01 §permits ·
/// derived from `permits.fs` / `permits.net`). A `CommandSandbox` (the L1
/// `nika-sandbox-{seatbelt,landlock}` crates) translates this into a
/// platform sandbox (Seatbelt profile · Landlock ruleset) that confines the
/// child to the declared filesystem reach and the declared network arm.
///
/// Empty spec = maximally confined (no reads/writes beyond the always-allowed
/// system paths · network denied) — `permits: {}` pure compute. Absent (the
/// `ShellCommand.sandbox` field is `None`) = today's behavior, unconfined
/// (the blocklist floor is the only gate).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SandboxSpec {
    /// Readable path prefixes (beyond the always-allowed system paths the
    /// dynamic linker + the program binary need to start).
    pub fs_read: Vec<String>,
    /// Writable path prefixes (the ONLY paths the child may write). The
    /// runner adds the per-spawn private scratch it mints (the child's
    /// `TMPDIR`) as one of these — the SHARED host tmp trees are no
    /// ambient grant (issue 754: they bypassed every declared boundary).
    pub fs_write: Vec<String>,
    /// The network arm (the tri-state below). [`NetPolicy::Deny`] is the
    /// default under a declared boundary — fail-closed.
    pub net: NetPolicy,
}

impl SandboxSpec {
    /// A maximally-confined spec — no extra reads/writes, no network.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The network arm of the confinement boundary — the tri-state derived from
/// `permits.net` (ADR-095 Layer 6 · the Anthropic sandbox-runtime model:
/// host-granular egress needs a loopback proxy, a Seatbelt host rule is
/// TLS-blind).
///
/// Derivation (`nika-runtime`'s `permits:` → [`SandboxSpec`] pass):
///
/// - a declared `net.http` host list maps to [`NetPolicy::Allowlist`] —
///   egress confined to exactly that set, enforced by the per-run loopback
///   egress proxy (the child gets proxy env vars · the OS fence admits
///   loopback only). Until the proxy lands, this arm confines EXACTLY as
///   [`NetPolicy::Allow`] (the pre-tri-state `allow_network = true`
///   behavior — zero regression, never a silent partial).
/// - an absent `net:` block or an empty `net.http` maps to
///   [`NetPolicy::Deny`] — the default, fail-closed.
/// - the bare `*` host entry (`net: { http: ["*"] }`) maps to
///   [`NetPolicy::Allow`] — unrestricted egress, reachable ONLY from that
///   explicit in-file declaration, never by default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetPolicy {
    /// No outbound network (the default under a declared boundary ·
    /// fail-closed).
    #[default]
    Deny,
    /// Unrestricted outbound network — the explicit escape hatch, reached
    /// only by declaring the bare `*` host in `permits.net.http`.
    Allow,
    /// Egress confined to the declared `permits.net.http` host set, via the
    /// loopback egress proxy (see the enum doc for the transitional mapping).
    Allowlist(EgressAllowlist),
}

/// The declared `permits.net.http` host set carried by
/// [`NetPolicy::Allowlist`]. Matched with the ONE host matcher
/// (`nika_types::net::host_glob_matches`) so the sandbox decision can never
/// drift from the static check or the http effect's boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EgressAllowlist {
    /// Allowed hosts (exact names or leading-`*.` subdomain globs — the same
    /// language the author writes in `permits.net.http`).
    pub hosts: Vec<String>,
    /// The loopback port the per-run egress proxy listens on — filled by the
    /// runner when it starts the proxy (the Seatbelt fence scopes outbound to
    /// exactly this port). `None` at derivation time.
    pub proxy_port: Option<u16>,
}

impl EgressAllowlist {
    /// The derivation-time allowlist — the declared hosts, no proxy yet.
    #[must_use]
    pub fn new(hosts: Vec<String>) -> Self {
        Self {
            hosts,
            proxy_port: None,
        }
    }
}

// The environment plane of the capability boundary lives in `nika-cap::env`
// (NEP-0005 · the permits vocabulary crate): the dangerous-name floor, the
// runner env floor, and the ONE composition law every spawn family runs.
// Re-exported here at the historical kernel path so every consumer import
// (`nika_kernel::process::DANGEROUS_ENV_VARS` · the spawn sites) resolves
// unchanged.
pub use nika_cap::env::{DANGEROUS_ENV_VARS, RUNNER_FLOOR_ENV_VARS, compose_child_env};

/// A shell command to execute.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShellCommand {
    /// Program to execute.
    pub program: String,
    /// Command arguments.
    pub args: Vec<String>,
    /// The AUTHORED environment entries (the task's `env:` map · values the
    /// file carries). The spawn site composes the child environment via
    /// [`compose_child_env`] — a CLEARED slate + the runner floor + the
    /// declared [`Self::env_passthrough`] + this map (which wins on a
    /// same-name collision), minus [`DANGEROUS_ENV_VARS`]. Nothing is ever
    /// inherited (NEP-0005).
    pub env: BTreeMap<String, String>,
    /// The declared `permits.env:` passthrough NAMES (NEP-0005) — resolved
    /// from the ENGINE's ambient environment at the spawn site, beneath the
    /// authored [`Self::env`] map and above the runner floor. Empty = floor
    /// + authored map only.
    pub env_passthrough: Vec<String>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
    /// Execution timeout.
    pub timeout: Option<Duration>,
    /// Data to send to stdin.
    pub stdin: Option<String>,
    /// Whether to run via `sh -c` (enables pipes, redirects).
    pub shell: bool,
    /// If `true`, skip the executor's default blocklist check.
    ///
    /// Set by callers that have already performed intelligent validation
    /// (e.g., the engine's `validate_exec_command_full`).
    pub pre_validated: bool,
    /// The OS-confinement boundary (derived from `permits`). `None` = today's
    /// behavior (unconfined · the blocklist floor is the only gate); `Some` =
    /// the runner wraps the spawn in the injected `CommandSandbox` (applied
    /// AFTER the blocklist, so the floor still sees the real command).
    pub sandbox: Option<SandboxSpec>,
}

impl ShellCommand {
    /// Create a new shell command with the given program.
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            cwd: None,
            timeout: None,
            stdin: None,
            shell: false,
            pre_validated: false,
            sandbox: None,
        }
    }

    /// Add an argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }
}

/// Result of a shell command execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShellResult {
    /// Process exit code.
    pub status: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Wall-clock duration of execution.
    pub duration: Duration,
    /// The RAW captured stdout octets, pre any lossy text decode —
    /// `Some` when the effect recorded them (the real runner does; a
    /// text-configured mock need not). The `decode:` pipeline (spec 09
    /// §decode · « raw bytes → decode → value, never bytes → lossy
    /// string → decode ») reads these through [`Self::stdout_octets`].
    pub stdout_raw: Option<Vec<u8>>,
    /// The RAW captured stderr octets (see `stdout_raw`).
    pub stderr_raw: Option<Vec<u8>>,
    failure: Option<ShellResultFailure>,
}

#[derive(Debug, Clone)]
enum ShellResultFailure {
    Authority(String),
    Transport(String),
}

/// Typed terminal receipt produced by a local sandbox or remote adapter.
///
/// Captured bytes alone never prove that a remote or wrapped process reached
/// a business exit. Adapters must attach one of these receipts; an absent or
/// unsupported receipt is a transport failure, never structured data.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShellAdapterOutcome {
    kind: ShellAdapterOutcomeKind,
}

#[derive(Debug, Clone)]
enum ShellAdapterOutcomeKind {
    Process,
    Authority(String),
    Transport(String),
}

impl ShellAdapterOutcome {
    /// The status belongs to the authored business process.
    #[must_use]
    pub fn process() -> Self {
        Self {
            kind: ShellAdapterOutcomeKind::Process,
        }
    }

    /// A permit, sandbox, or remote authority refused the operation.
    #[must_use]
    pub fn authority_refusal(reason: impl Into<String>) -> Self {
        Self {
            kind: ShellAdapterOutcomeKind::Authority(reason.into()),
        }
    }

    /// The adapter did not deliver a trustworthy process outcome.
    #[must_use]
    pub fn transport_failure(reason: impl Into<String>) -> Self {
        Self {
            kind: ShellAdapterOutcomeKind::Transport(reason.into()),
        }
    }

    /// A remote/adapter envelope carried bytes without a terminal receipt.
    #[must_use]
    pub fn missing_terminal_receipt(adapter: &'static str) -> Self {
        Self::transport_failure(format!(
            "{adapter} returned captured bytes without a terminal receipt"
        ))
    }
}

impl ShellResult {
    /// Create a new shell result.
    #[must_use]
    pub fn new(
        status: i32,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            status,
            stdout: stdout.into(),
            stderr: stderr.into(),
            duration,
            stdout_raw: None,
            stderr_raw: None,
            failure: None,
        }
    }

    /// Attach the raw captured byte streams (the real runner's duty —
    /// the text fields stay the lossy projections existing readers use).
    #[must_use]
    pub fn with_raw(mut self, stdout_raw: Vec<u8>, stderr_raw: Vec<u8>) -> Self {
        self.stdout_raw = Some(stdout_raw);
        self.stderr_raw = Some(stderr_raw);
        self
    }

    /// Mark a captured adapter result as an authority refusal.
    ///
    /// Some sandbox, permit, and remote-process adapters must drain output
    /// before they can classify the terminal status. This tag keeps that
    /// refusal typed so `capture: structured` cannot reinterpret the drained
    /// bytes as successful business data.
    #[must_use]
    pub fn with_authority_refusal(mut self, reason: impl Into<String>) -> Self {
        self.failure = Some(ShellResultFailure::Authority(reason.into()));
        self
    }

    /// Mark a captured adapter result as a transport failure.
    ///
    /// The output may have been drained for cleanup or diagnostics, but it is
    /// never a process outcome that capture policy may declassify.
    #[must_use]
    pub fn with_transport_failure(mut self, reason: impl Into<String>) -> Self {
        self.failure = Some(ShellResultFailure::Transport(reason.into()));
        self
    }

    /// Attach a typed adapter receipt to captured bytes.
    ///
    /// This is the production hand-off for both sandbox wrappers and future
    /// remote workers. `MissingTerminalReceipt` fails closed so a remote
    /// adapter cannot turn plausible output into a process result merely by
    /// omitting its terminal authority receipt.
    #[must_use]
    pub fn with_adapter_outcome(mut self, outcome: ShellAdapterOutcome) -> Self {
        self.failure = match outcome.kind {
            ShellAdapterOutcomeKind::Process => self.failure,
            ShellAdapterOutcomeKind::Authority(reason) => {
                Some(ShellResultFailure::Authority(reason))
            }
            ShellAdapterOutcomeKind::Transport(reason) => {
                Some(ShellResultFailure::Transport(reason))
            }
        };
        self
    }

    /// Consume the captured envelope and recover only a business-process
    /// result. Typed authority/transport failures take precedence.
    ///
    /// # Errors
    /// Returns [`ShellError::Blocked`] for an authority refusal and
    /// [`ShellError::Other`] for a transport failure.
    pub fn into_process_result(self) -> Result<Self, ShellError> {
        match self.failure {
            None => Ok(self),
            Some(ShellResultFailure::Authority(reason)) => Err(ShellError::Blocked { reason }),
            Some(ShellResultFailure::Transport(reason)) => Err(ShellError::Other { reason }),
        }
    }

    /// The exact captured stdout octets — the recorded raw stream when
    /// present, else the text field's bytes (exact for any effect that
    /// was configured with text, e.g. mocks).
    #[must_use]
    pub fn stdout_octets(&self) -> &[u8] {
        self.stdout_raw.as_deref().unwrap_or(self.stdout.as_bytes())
    }

    /// The exact captured stderr octets (see [`Self::stdout_octets`]).
    #[must_use]
    pub fn stderr_octets(&self) -> &[u8] {
        self.stderr_raw.as_deref().unwrap_or(self.stderr.as_bytes())
    }

    /// Whether the process exited successfully (status 0).
    #[must_use]
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Shell execution errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ShellError {
    /// Program not found.
    #[error("program not found: {program}")]
    NotFound {
        /// The program that was not found.
        program: String,
    },

    /// Execution timed out.
    #[error("execution timed out after {duration_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds.
        duration_ms: u64,
    },

    /// Execution was cancelled.
    #[error("execution cancelled: {id}")]
    Cancelled {
        /// The cancelled command identifier.
        id: String,
    },

    /// Command blocked by security policy.
    #[error("command blocked: {reason}")]
    Blocked {
        /// Why the command was blocked.
        reason: String,
    },

    /// Captured stdout or stderr exceeded the per-stream byte cap.
    ///
    /// Safe-by-default resource floor: an unbounded writer (`yes`,
    /// `cat /dev/zero`) cannot OOM the host. Fail-closed and aligned with
    /// the file-read cap precedent. Callers needing larger output redirect
    /// to a file inside the command and read it back.
    #[error("captured output exceeded {limit_bytes}-byte per-stream cap")]
    OutputTooLarge {
        /// The per-stream byte limit that was exceeded.
        limit_bytes: usize,
    },

    /// Other execution error.
    #[error("shell error: {reason}")]
    Other {
        /// Error description.
        reason: String,
    },
}

/// Run shell commands.
#[trait_variant::make(ShellRunDyn: Send)]
pub trait ShellRun: Send + Sync {
    /// Execute a shell command and return the result.
    ///
    /// CANCEL SAFETY: cancel-safe IF the impl sets `kill_on_drop(true)`
    /// on its `tokio::process::Command` (INV-011). Dropping the future
    /// then sends SIGKILL to the child on drop — no orphan processes,
    /// no resource leak. Impls that do NOT set `kill_on_drop` are UNSAFE.
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError>;
}

/// Cancel running shell commands.
#[trait_variant::make(ShellCancelDyn: Send)]
pub trait ShellCancel: Send + Sync {
    /// Cancel a running command by its identifier.
    ///
    /// CANCEL SAFETY: cancel-safe and idempotent — signalling an already
    /// dead pid is a harmless no-op. Callers may race cancel against
    /// natural exit without corruption.
    async fn cancel(&self, id: &str) -> Result<(), ShellError>;
}

/// Full shell executor — blanket super-trait.
pub trait ShellExecutor: ShellRun + ShellCancel {}
impl<T: ShellRun + ShellCancel> ShellExecutor for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_new_defaults() {
        let cmd = ShellCommand::new("echo");
        assert_eq!(cmd.program, "echo");
        assert!(cmd.args.is_empty());
        assert!(cmd.env.is_empty());
        assert!(cmd.cwd.is_none());
        assert!(cmd.timeout.is_none());
        assert!(cmd.stdin.is_none());
        assert!(!cmd.shell);
        assert!(!cmd.pre_validated);
    }

    #[test]
    fn shell_command_builder() {
        let cmd = ShellCommand::new("ls").arg("-la").arg("/tmp");
        assert_eq!(cmd.args, vec!["-la", "/tmp"]);
    }

    #[test]
    fn shell_result_success() {
        let result = ShellResult::new(0, "output", "", Duration::from_millis(100));
        assert!(result.success());
        assert_eq!(result.stdout, "output");
    }

    #[test]
    fn shell_result_failure() {
        let result = ShellResult::new(1, "", "error", Duration::from_millis(50));
        assert!(!result.success());
    }

    #[test]
    fn shell_result_typed_failures_precede_process_interpretation() {
        let authority = ShellResult::new(126, "plausible", "", Duration::ZERO)
            .with_authority_refusal("permit refused")
            .into_process_result();
        assert!(matches!(authority, Err(ShellError::Blocked { .. })));
        let transport = ShellResult::new(1, "partial", "", Duration::ZERO)
            .with_transport_failure("remote disconnected")
            .into_process_result();
        assert!(matches!(transport, Err(ShellError::Other { .. })));
    }

    #[test]
    fn remote_terminal_receipt_table_is_fail_closed() {
        let process = ShellResult::new(7, r#"{"ok":true}"#, "", Duration::ZERO)
            .with_adapter_outcome(ShellAdapterOutcome::process())
            .into_process_result();
        assert!(matches!(process, Ok(result) if result.status == 7));

        let authority = ShellResult::new(126, r#"{"ok":true}"#, "", Duration::ZERO)
            .with_adapter_outcome(ShellAdapterOutcome::authority_refusal(
                "remote permit refused",
            ))
            .into_process_result();
        assert!(matches!(authority, Err(ShellError::Blocked { .. })));

        let disconnected = ShellResult::new(0, "partial", "", Duration::ZERO)
            .with_adapter_outcome(ShellAdapterOutcome::transport_failure(
                "remote stream disconnected",
            ))
            .into_process_result();
        assert!(matches!(disconnected, Err(ShellError::Other { .. })));

        let unsupported = ShellResult::new(0, r#"{"ok":true}"#, "", Duration::ZERO)
            .with_adapter_outcome(ShellAdapterOutcome::missing_terminal_receipt("remote"))
            .into_process_result();
        assert!(matches!(unsupported, Err(ShellError::Other { .. })));
    }

    #[test]
    fn shell_error_not_found_display() {
        let err = ShellError::NotFound {
            program: "git".into(),
        };
        assert_eq!(err.to_string(), "program not found: git");
    }

    #[test]
    fn shell_error_timeout_display() {
        let err = ShellError::Timeout { duration_ms: 30000 };
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn shell_error_blocked_display() {
        let err = ShellError::Blocked {
            reason: "rm -rf /".into(),
        };
        assert!(err.to_string().contains("blocked"));
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn shell_types_send_sync() {
        _assert_send_sync::<ShellCommand>();
        _assert_send_sync::<ShellResult>();
    }
}
