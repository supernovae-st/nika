// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-exec-runner` — the production shell-execution effect.
//!
//! This crate sits at **L1**: it implements the L0.5 `nika_kernel::process`
//! traits (`ShellRun` + `ShellCancel` — and the blanket `ShellExecutor`) via
//! `tokio::process`. The only production site spawning subprocesses; crates
//! that run commands inject the kernel traits and receive [`TokioShell`] in
//! production, a mock in tests (Invariant #27).
//!
//! [`TokioShell`] implements the `*Dyn` trait-variant companions (`Send`
//! futures · base traits via the blanket impl), same pattern as
//! `nika-fs`/`nika-http`/`nika-blob`.
//!
//! # Security — safe by default
//!
//! Workflows run attacker-influenced commands, so the MECHANISM is safe
//! before `nika-policy` (L1.5 · step 8) adds richer gating. Unless
//! `command.pre_validated`, the blocklist floor runs PER MODE:
//! - **`shell: true`** → the full string scan (`check_command` · NFKC +
//!   zero-width + quote/basename defenses · ~100 patterns) + `check_shell_mode`
//!   (alias/function + expansion/glob/substitution-char refusal · the TOCTOU
//!   floor for `sh -c`).
//! - **`shell: false` (argv)** → `check_argv`: the program IDENTITY
//!   (`DANGEROUS_PROGRAMS`) PLUS the structural re-exec class (interpreter
//!   inline-eval `-c`/`-e`, `env`, `nc -e`, `dd if=`) — symmetric with shell
//!   mode but per-argv-element, so a literal arg is never a false positive.
//!
//! `ShellError::Blocked` on a hit. `pre_validated` is the documented seam for
//! the policy layer that has already done intent-aware validation. The floor
//! is a TRIPWIRE, not a boundary — `permits.exec` + the sandbox are the real
//! gates (a glob/symlink in `$PATH` resolution is theirs to contain).
//!
//! # Process safety (kernel CANCEL SAFETY contract)
//!
//! - **`kill_on_drop(true)`** (INV-011) — dropping the `run()` future SIGKILLs
//!   the child. This IS the PRIMARY cancellation per ADR-016 (future-drop).
//! - **Concurrent stdout/stderr drain with `wait()`** via `tokio::try_join!`
//!   (INV-012) — a child writing past the OS pipe buffer would deadlock if we
//!   waited-then-read.
//! - **`cancel(id)`** is registry-backed kill-by-pid (ADR-016 · the OS kills) —
//!   `run()` registers each spawned child by its pid; `cancel(pid)` signals it;
//!   unknown/dead pid is an idempotent `Ok`.
//! - **Output cap** (`MAX_OUTPUT_BYTES` · NIKA-054) — each captured stream is
//!   bounded so an unbounded writer cannot OOM the host; the timeout bounds
//!   time, this bounds memory. See §3.5 of the crate spec.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod blocklist;

use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use nika_kernel::command_sandbox::{CommandSandbox, CommandSandboxError};
use nika_kernel::process::{ShellCancelDyn, ShellRunDyn};
use nika_kernel::{ShellCommand, ShellError, ShellResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::Notify;

/// Per-stream capture cap (stdout AND stderr each). A runaway writer
/// (`yes`, `cat /dev/zero`) would otherwise grow the capture buffer until
/// the host OOMs — the `timeout` bounds wall-clock, NOT memory. Fail-closed
/// at 64 MiB, mirroring the file-read cap precedent; the spawned child is
/// killed via `kill_on_drop` the instant the cap is hit. Commands needing
/// larger output redirect to a file in-command and read it back.
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// Marker error a bounded [`drain`] returns when a stream exceeds the cap.
/// Carried through `tokio::io::Error` so `try_join!` short-circuits (which
/// drops the child future → SIGKILL), then mapped to
/// [`ShellError::OutputTooLarge`] at the single exit site.
#[derive(Debug)]
struct OutputCapExceeded;

impl std::fmt::Display for OutputCapExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "captured output exceeded the per-stream cap")
    }
}

impl std::error::Error for OutputCapExceeded {}

/// pid → cancel signal, shared across `run()`/`cancel()` calls.
type Registry = Arc<Mutex<BTreeMap<u32, Arc<Notify>>>>;

/// Production shell executor backed by `tokio::process::Command`.
///
/// Cheap to clone (the cancel registry is `Arc`-shared). The blocklist is
/// the safe-by-default floor; `kill_on_drop` + the registry give cancellation.
/// An optional injected [`CommandSandbox`] (the `nika-sandbox-{seatbelt,
/// landlock}` backends) confines a command that carries a `SandboxSpec`.
#[derive(Clone, Default)]
pub struct TokioShell {
    registry: Registry,
    /// The OS-confinement backend (injected by the wiring layer · `None` =
    /// no sandbox available, today's behavior). Applied AFTER the blocklist.
    sandbox: Option<Arc<dyn CommandSandbox>>,
    /// Env-var NAMES the child must NOT inherit from the ENGINE's ambient
    /// environment — the provider API keys `config_from_env` loaded for the
    /// engine's OWN inference calls (ADR-095 Layer 3 · ambient-secret scrub).
    /// Injected by the composition root from the provider catalog. A workflow
    /// that genuinely needs a key in its child still sets it EXPLICITLY in
    /// `env:` (explicit intent wins over the ambient scrub · see `run`). Empty
    /// = today's behavior (inherit all). `Arc<[…]>` for a cheap `Clone`.
    ambient_secret_env: Arc<[String]>,
}

impl std::fmt::Debug for TokioShell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokioShell")
            .field("sandbox", &self.sandbox.as_ref().map(|s| s.backend()))
            .finish_non_exhaustive()
    }
}

impl TokioShell {
    /// Create a new shell executor with an empty cancel registry and NO
    /// sandbox backend (unconfined · today's behavior · the blocklist floor
    /// is the only gate).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a shell executor with an injected OS-confinement backend. A
    /// command carrying a `SandboxSpec` is confined by it (after the blocklist,
    /// before the spawn); a command with no spec runs unconfined as before.
    #[must_use]
    pub fn with_sandbox(sandbox: Arc<dyn CommandSandbox>) -> Self {
        Self {
            sandbox: Some(sandbox),
            ..Self::default()
        }
    }

    /// Scrub the ENGINE's ambient provider API keys from every child's
    /// environment (ADR-095 Layer 3 · least-privilege by default). `names` are
    /// the provider env-var names the engine reads for its OWN inference calls
    /// (from the provider catalog · injected by the composition root); an exec
    /// child has no legitimate need for them via ambient inheritance, and
    /// leaving them in lets an untrusted command exfiltrate them (`printenv`,
    /// `cat /proc/self/environ`). A workflow that DOES need a key in its child
    /// sets it explicitly in `env:` — that wins (only the ambient copy is
    /// stripped · see `run`). Chainable with [`Self::with_sandbox`].
    #[must_use]
    pub fn with_ambient_secret_env(mut self, names: impl Into<Arc<[String]>>) -> Self {
        self.ambient_secret_env = names.into();
        self
    }

    /// Register a pid's cancel signal; returns the shared `Notify`.
    fn register(&self, pid: u32) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        if let Ok(mut reg) = self.registry.lock() {
            reg.insert(pid, Arc::clone(&notify));
        }
        notify
    }

    /// Deregister a pid (on natural exit). Best-effort.
    fn deregister(&self, pid: u32) {
        if let Ok(mut reg) = self.registry.lock() {
            reg.remove(&pid);
        }
    }

    /// Apply the injected OS sandbox to a command that requests one. No spec =
    /// unchanged (today's behavior); spec + backend = confined; spec but NO
    /// backend = fail-closed (refuse rather than run an asked-for-confined
    /// command unconfined).
    fn apply_sandbox(&self, command: ShellCommand) -> Result<ShellCommand, ShellError> {
        let Some(spec) = command.sandbox.clone() else {
            return Ok(command);
        };
        match &self.sandbox {
            Some(backend) => backend
                .confine(&spec, command)
                .map_err(|e| map_sandbox_error(&e)),
            None => Err(ShellError::Blocked {
                reason: "command requires an OS sandbox but no backend is wired \
                         (refusing to run unconfined)"
                    .to_string(),
            }),
        }
    }
}

/// What ended the wait — kept out of the `select!` so the pid is always
/// deregistered (no early `return` inside the arms).
enum Outcome {
    Done(std::io::Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)>),
    Cancelled,
    TimedOut(u64),
}

impl ShellRunDyn for TokioShell {
    /// Run a command: blocklist (unless `pre_validated`) → spawn
    /// (`kill_on_drop`) → concurrent drain + wait with timeout + cancel.
    ///
    /// CANCEL SAFETY: cancel-safe — `kill_on_drop(true)` means dropping this
    /// future SIGKILLs the child (no orphan). The PRIMARY cancellation path.
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        pre_validate(&command)?;

        // OS confinement (spec 01 §permits · ADR-095 Layer 6) — applied AFTER
        // the blocklist so the floor inspected the REAL command, not the
        // launcher wrapper.
        let command = self.apply_sandbox(command)?;

        let start = Instant::now();
        let mut cmd = build_command(&command);
        // Ambient-secret scrub (ADR-095 Layer 3): the engine loaded provider
        // API keys from ITS env for its own inference calls; strip them from
        // the child UNLESS the workflow set the name EXPLICITLY in `env:`
        // (explicit intent wins · only the ambient copy is removed). Runs after
        // build_command applied the explicit `env:` map, so an explicit set is
        // preserved; env_remove of an absent key is a harmless no-op.
        for name in self.ambient_secret_env.iter() {
            if !command.env.contains_key(name) {
                cmd.env_remove(name);
            }
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        // INV-011: kill the child when its handle drops (cancel/timeout/panic).
        cmd.kill_on_drop(true);

        let mut child = spawn_classified(&mut cmd, &command.program)?;

        if let Some(data) = &command.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            let _ = stdin.write_all(data.as_bytes()).await;
            drop(stdin); // close → EOF for the child
        }

        // Register for out-of-band cancel-by-pid (ADR-016).
        let pid = child.id();
        let notify = pid.map(|p| self.register(p));

        // INV-012: drain stdout+stderr CONCURRENTLY with wait() — a child that
        // writes past the OS pipe buffer would deadlock on wait-then-read.
        let child_fut = async move {
            let out = child.stdout.take();
            let err = child.stderr.take();
            tokio::try_join!(
                child.wait(),
                drain(out, MAX_OUTPUT_BYTES),
                drain(err, MAX_OUTPUT_BYTES)
            )
        };

        let timeout_fut = async {
            match command.timeout {
                Some(t) => tokio::time::sleep(t).await,
                None => std::future::pending::<()>().await,
            }
        };

        let outcome = match &notify {
            Some(n) => {
                tokio::select! {
                    biased;
                    () = n.notified() => Outcome::Cancelled,
                    () = timeout_fut => Outcome::TimedOut(
                        command.timeout.map_or(0, dur_ms)),
                    r = child_fut => Outcome::Done(r),
                }
            }
            None => {
                tokio::select! {
                    biased;
                    () = timeout_fut => Outcome::TimedOut(
                        command.timeout.map_or(0, dur_ms)),
                    r = child_fut => Outcome::Done(r),
                }
            }
        };

        if let Some(p) = pid {
            self.deregister(p);
        }

        outcome_to_result(outcome, pid, start).await
    }
}

/// Map the `select!` [`Outcome`] to the public `Result` — the cancel/timeout
/// arms first SIGTERM→SIGKILL the whole process group (detached grandchildren
/// die too · `terminate_group`) before reporting; a clean exit becomes a
/// [`ShellResult`] with the captured stdout/stderr.
async fn outcome_to_result(
    outcome: Outcome,
    pid: Option<u32>,
    start: Instant,
) -> Result<ShellResult, ShellError> {
    match outcome {
        Outcome::Cancelled => {
            // Kill the WHOLE process group (not just the direct child),
            // so detached grandchildren die too — before reporting.
            if let Some(p) = pid {
                terminate_group(p).await;
            }
            Err(ShellError::Cancelled {
                id: pid.map_or_else(|| "?".to_string(), |p| p.to_string()),
            })
        }
        Outcome::TimedOut(ms) => {
            if let Some(p) = pid {
                terminate_group(p).await;
            }
            Err(ShellError::Timeout { duration_ms: ms })
        }
        Outcome::Done(Err(e)) => Err(classify_drain_error(&e)),
        Outcome::Done(Ok((status, stdout, stderr))) => Ok(ShellResult::new(
            status.code().unwrap_or(-1),
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
            start.elapsed(),
        )),
    }
}

/// The pre-spawn blocklist floor (skipped when the caller set `pre_validated`).
///
/// The two command forms gate differently. The SHELL form goes to `sh -c`, so
/// shell-syntax metacharacters in any argument ARE parsed; the full blocklist
/// plus the expansion-char refusal apply. The ARGV form is `execve` (NO shell),
/// where a `;` or `| bash` inside an argument is a LITERAL character, so
/// scanning the joined line would false-positive on benign args. There the
/// floor uses `check_argv`, which gates the program identity AND the re-exec
/// class an interpreter / `env` re-introduces. Symmetric on the program-level
/// dangers (review P0).
fn pre_validate(command: &ShellCommand) -> Result<(), ShellError> {
    if !command.pre_validated {
        if command.shell {
            // Shell form: the whole line goes to `sh -c`, so shell-syntax
            // metacharacters in any argument ARE parsed — the full
            // blocklist (dangerous patterns) + the expansion-char refusal
            // are the tripwire. The battle-tested floor, unchanged.
            let full = format!("{} {}", command.program, command.args.join(" "));
            blocklist::check_command(&full)?;
            blocklist::check_shell_mode(&full)?;
        } else {
            // Argv form: `execve`, NO shell. The shell-SYNTAX patterns are
            // meaningless here — a `;` or `| bash` inside an argument is a
            // LITERAL character, so scanning the joined line would false-
            // positive on benign args (`["echo", "a; b"]`). But the floor
            // must stay SYMMETRIC with shell mode for the program-level
            // dangers (review P0): `check_argv` checks the program identity
            // AND, STRUCTURALLY, the re-exec class an interpreter / `env`
            // re-introduces (`["sh","-c",…]`, `["python","-c",…]`, `nc -e`,
            // `dd if=`). Richer gating is `permits.exec` + the sandbox.
            blocklist::check_argv(&command.program, &command.args)?;
        }
    }
    Ok(())
}

/// Spawn the child, classifying the io error (`NotFound` → typed
/// `ShellError::NotFound` with the program name · anything else → `Other`).
fn spawn_classified(
    cmd: &mut tokio::process::Command,
    program: &str,
) -> Result<tokio::process::Child, ShellError> {
    cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ShellError::NotFound {
                program: program.to_owned(),
            }
        } else {
            ShellError::Other {
                reason: e.to_string(),
            }
        }
    })
}

impl ShellCancelDyn for TokioShell {
    /// Cancel a running command by its OS pid (string). Unknown/dead pid is
    /// an idempotent `Ok` (per the kernel contract).
    ///
    /// CANCEL SAFETY: cancel-safe and idempotent.
    async fn cancel(&self, id: &str) -> Result<(), ShellError> {
        let Ok(pid) = id.parse::<u32>() else {
            // Non-numeric id can't match a registered pid — harmless no-op.
            return Ok(());
        };
        let notify = self
            .registry
            .lock()
            .ok()
            .and_then(|reg| reg.get(&pid).map(Arc::clone));
        if let Some(n) = notify {
            // `notify_one` RETAINS a permit if `cancel` races ahead of `run()`
            // first polling `notified()` (the register→select window) — so an
            // early cancel is not lost (review P3). Wakes run()'s `notified()`
            // arm → child drop → SIGKILL.
            n.notify_one();
        }
        Ok(())
    }
}

/// Environment variables ALWAYS stripped from the child's environment — the
/// injection vectors that grant code execution or library injection with no
/// dangerous flag in the command itself (the "env-var injection" class). An
/// ambient `LD_PRELOAD` inherited from the engine's own environment is not
/// something a policy vouched for, so the strip is independent of
/// `pre_validated` and runs AFTER the explicit `env` map (the floor wins — a
/// workflow does not pass these). The stronger clean-slate posture (drop ALL
/// inherited env + a curated `PATH`) belongs to the sandbox, which knows the
/// confined workflow's declared needs.
const DANGEROUS_ENV_VARS: &[&str] = &[
    // Dynamic-linker library injection (run attacker code in any dynamically
    // linked child) — Linux + macOS (incl. the macOS fallback paths · P2).
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "GCONV_PATH", // glibc iconv module load → code execution (P2)
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "DYLD_FALLBACK_FRAMEWORK_PATH",
    // Shell startup-file sourcing on non-interactive `sh`/bash.
    "BASH_ENV",
    "ENV",
    // Tool-specific command hooks (RCE with no flags).
    "GIT_SSH_COMMAND",
    "GIT_SSH",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "GIT_PROXY_COMMAND", // config-driven git RCE (P2)
    "GIT_CONFIG_GLOBAL", // point git at an attacker config (core.pager/fsmonitor)
    "GIT_CONFIG_SYSTEM",
    "GIT_TEMPLATE_DIR", // attacker hooks copied into a new repo
    "LESSOPEN",         // pager-input-preprocess command injection (P2)
    "HOSTALIASES",      // attacker file read during hostname resolution (P2)
    "TERMINFO",         // load a crafted terminfo entry (P2)
    "TERMINFO_DIRS",    // terminfo search-path override (P2)
    "TERMCAP",          // crafted termcap string executed by some pagers (P2)
    // Interpreter pre-exec hooks.
    "PYTHONSTARTUP",
    "PYTHONPATH", // inject a module into any python that imports (P2)
    "PERL5OPT",
    "PERL5LIB",
    "RUBYOPT",
    "NODE_OPTIONS",
    // Field-splitting injection for shell-mode commands.
    "IFS",
];

/// Build the `tokio::process::Command` (program/args or `sh -c`, env, cwd).
fn build_command(command: &ShellCommand) -> Command {
    let mut cmd = if command.shell {
        let line = if command.args.is_empty() {
            command.program.clone()
        } else {
            format!("{} {}", command.program, command.args.join(" "))
        };
        let mut c = Command::new("sh");
        c.arg("-c").arg(line);
        c
    } else {
        let mut c = Command::new(&command.program);
        c.args(&command.args);
        c
    };
    for (k, v) in &command.env {
        cmd.env(k, v);
    }
    // After env so a removal wins over a set of the same key.
    for k in &command.env_remove {
        cmd.env_remove(k);
    }
    // SECURITY (always-on · independent of `pre_validated`): strip the
    // dangerous-env injection vectors LAST so the floor wins even over an
    // explicit set — these grant code/library injection with no dangerous
    // flag in the command (see [`DANGEROUS_ENV_VARS`]).
    for var in DANGEROUS_ENV_VARS {
        cmd.env_remove(var);
    }
    if let Some(cwd) = &command.cwd {
        cmd.current_dir(cwd);
    }
    // New process group (leader = the child · pgid = pid) so a timeout/cancel
    // can signal the WHOLE group — detached grandchildren (`sh -c "... &"`)
    // die with the parent instead of being orphaned to init. `process_group(0)`
    // is the SAFE std equivalent of setpgid(0,0) (stable since 1.64) — no
    // unsafe pre_exec, so this crate keeps `unsafe_code = forbid`.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.as_std_mut().process_group(0);
    }
    cmd
}

/// Read an optional async handle to end, bounded at `limit` bytes.
///
/// Reads at most `limit + 1` (the `+1` makes overflow DETECTABLE without
/// ever buffering more than one byte past the cap), then returns an
/// [`OutputCapExceeded`] marker error. `read_to_end` over the `Take`
/// adapter stops at the cap on its own, so a child blocked writing past a
/// full pipe never deadlocks `wait()` — `try_join!` short-circuits on the
/// marker and the child future drops (SIGKILL · INV-011).
async fn drain<R: tokio::io::AsyncRead + Unpin>(
    handle: Option<R>,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let Some(h) = handle else {
        return Ok(Vec::new());
    };
    let mut buf = Vec::new();
    // `limit as u64 + 1` cannot overflow: limit is a small const (64 MiB).
    h.take(limit as u64 + 1).read_to_end(&mut buf).await?;
    if buf.len() > limit {
        return Err(std::io::Error::other(OutputCapExceeded));
    }
    Ok(buf)
}

/// Map a drained-stream error to the public [`ShellError`]: the cap marker
/// becomes [`ShellError::OutputTooLarge`], everything else is `Other`.
fn classify_drain_error(e: &std::io::Error) -> ShellError {
    if let Some(inner) = e.get_ref()
        && inner.is::<OutputCapExceeded>()
    {
        return ShellError::OutputTooLarge {
            limit_bytes: MAX_OUTPUT_BYTES,
        };
    }
    ShellError::Other {
        reason: e.to_string(),
    }
}

/// Saturating `Duration` → milliseconds (kernel `Timeout.duration_ms` is u64).
fn dur_ms(t: std::time::Duration) -> u64 {
    u64::try_from(t.as_millis()).unwrap_or(u64::MAX)
}

/// Map a [`CommandSandboxError`] to [`ShellError`] — both an unavailable
/// backend and an un-expressible profile are a fail-closed refusal to spawn,
/// surfaced as `Blocked` (the command was not run).
fn map_sandbox_error(e: &CommandSandboxError) -> ShellError {
    ShellError::Blocked {
        reason: format!("sandbox could not confine the command: {e}"),
    }
}

/// Grace between SIGTERM and SIGKILL when terminating a timed-out / cancelled
/// process group — lets the members flush and exit cleanly before the hard kill.
#[cfg(unix)]
const GROUP_TERM_GRACE: std::time::Duration = std::time::Duration::from_millis(100);

/// Terminate the child's whole process group on timeout/cancel: SIGTERM, a
/// short [`GROUP_TERM_GRACE`], then SIGKILL — so detached grandchildren (in the
/// same group via `process_group(0)`) die too, not just the direct child.
///
/// The child is its own group leader, so its pid IS the pgid. Best-effort: a
/// racing natural exit (or an already-empty group) makes the signals harmless
/// `ESRCH` no-ops. `kill_on_drop` still SIGKILLs the direct child when the
/// future drops; this catches the rest of the tree.
#[cfg(unix)]
async fn terminate_group(pid: u32) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    let Ok(raw) = i32::try_from(pid) else {
        return;
    };
    // The child is its own group leader, so its pid IS the pgid.
    let group = Pid::from_raw(raw);
    let _ = killpg(group, Signal::SIGTERM);
    tokio::time::sleep(GROUP_TERM_GRACE).await;
    let _ = killpg(group, Signal::SIGKILL);
}

/// Non-unix fallback: process groups are a POSIX concept; `kill_on_drop`
/// remains the cancellation path. (The crate's contract tests are unix-only.)
#[cfg(not(unix))]
async fn terminate_group(_pid: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokioshell_is_clone_default() {
        fn assert_traits<T: Clone + Default>() {}
        assert_traits::<TokioShell>();
    }

    #[test]
    fn dur_ms_saturates() {
        assert_eq!(dur_ms(std::time::Duration::from_millis(1500)), 1500);
        // Duration::MAX has > u64::MAX milliseconds → saturates, never panics.
        assert_eq!(dur_ms(std::time::Duration::MAX), u64::MAX);
    }

    #[test]
    fn build_command_both_modes_do_not_panic() {
        let _ = build_command(&ShellCommand::new("echo").arg("hi"));
        let mut sh = ShellCommand::new("echo");
        sh.shell = true;
        sh.args = vec!["a".into(), "b".into()];
        let _ = build_command(&sh);
    }

    // ── output cap (NIKA-054): unbounded capture is an OOM vector ──

    #[tokio::test]
    async fn drain_reads_all_under_limit() {
        let data = b"hello world";
        let out = drain(Some(&data[..]), 1024).await.expect("under limit ok");
        assert_eq!(out, b"hello world");
    }

    #[tokio::test]
    async fn drain_none_is_empty() {
        let out = drain(None::<&[u8]>, 100).await.expect("none ok");
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn drain_over_limit_errors_with_cap_marker() {
        // 10_000 bytes against a 100-byte injected limit · the bounded read
        // stops one past the cap (never buffers the full input → no OOM) and
        // the marker maps to the public OutputTooLarge (reporting the
        // PRODUCTION cap · drain is always wired to MAX_OUTPUT_BYTES). This
        // unit exercises the exact code path run() takes at 64 MiB.
        let data = vec![b'x'; 10_000];
        let err = drain(Some(&data[..]), 100)
            .await
            .expect_err("over limit must error");
        assert!(matches!(
            classify_drain_error(&err),
            ShellError::OutputTooLarge { limit_bytes } if limit_bytes == MAX_OUTPUT_BYTES
        ));
    }

    #[test]
    fn classify_passes_through_non_cap_errors() {
        let e = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe gone");
        assert!(matches!(classify_drain_error(&e), ShellError::Other { .. }));
    }

    #[tokio::test]
    async fn run_still_captures_normal_output() {
        // Regression: the cap must not break ordinary (sub-cap) capture.
        let shell = TokioShell::new();
        let res = shell
            .run(ShellCommand::new("echo").arg("hello"))
            .await
            .expect("echo runs");
        assert_eq!(res.stdout.trim(), "hello");
        assert!(res.success());
    }

    #[tokio::test]
    async fn ambient_secret_env_scrubbed_unless_set_explicitly() {
        // ADR-095 Layer 3. PATH is reliably present in the test process env
        // AND unneeded to run an ABSOLUTE-path program — so it stands in for an
        // ambient provider secret without the (forbidden) unsafe env::set_var.
        let scrub = TokioShell::new().with_ambient_secret_env(vec!["PATH".to_owned()]);

        // Scrubbed + not set explicitly → the child's PATH is gone (printenv
        // prints nothing · exits non-zero, which `run` surfaces as data).
        let out = scrub
            .run(ShellCommand::new("/usr/bin/printenv").arg("PATH"))
            .await
            .expect("printenv runs");
        assert!(
            out.stdout.trim().is_empty(),
            "ambient PATH must be scrubbed from the child, got {:?}",
            out.stdout
        );

        // Explicit `env:` wins over the scrub — the child sees the declared value.
        let mut explicit = ShellCommand::new("/usr/bin/printenv").arg("PATH");
        explicit.env.insert("PATH".to_owned(), "/decl".to_owned());
        let out2 = scrub.run(explicit).await.expect("printenv runs");
        assert_eq!(
            out2.stdout.trim(),
            "/decl",
            "an explicit env entry must win over the ambient scrub"
        );

        // Default (no scrub set) inherits the ambient PATH as before.
        let out3 = TokioShell::new()
            .run(ShellCommand::new("/usr/bin/printenv").arg("PATH"))
            .await
            .expect("printenv runs");
        assert!(
            !out3.stdout.trim().is_empty(),
            "default TokioShell must still inherit the ambient PATH"
        );
    }

    #[tokio::test]
    async fn cancel_registry_register_notify_deregister() {
        let shell = TokioShell::new();
        let notify = shell.register(4242);
        // cancel(pid) must wake a task awaiting the registered Notify.
        let waiter = {
            let n = Arc::clone(&notify);
            tokio::spawn(async move { n.notified().await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        shell.cancel("4242").await.expect("cancel ok");
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("cancel must wake the waiter")
            .expect("waiter task ok");
        // deregister REMOVES the entry (not a no-op) — a fresh waiter on a
        // re-registered Notify must NOT be woken by the stale pid's cancel.
        shell.deregister(4242);
        assert!(
            shell.registry.lock().unwrap().get(&4242).is_none(),
            "deregister must remove the pid from the registry"
        );
        assert!(shell.cancel("4242").await.is_ok()); // now a harmless no-op
    }
}
