// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-exec-runner` — the production shell-execution effect.
//!
//! This crate sits at **L1**: it implements the L0.5 `nika_kernel::process`
//! traits (`ShellRun` + `ShellCancel` — and the blanket `ShellExecutor`) via
//! `tokio::process`. The only production site spawning PLAIN subprocesses —
//! one deliberate second site exists: `nika-mcp`'s stdio MCP client (a
//! persistent bidirectional JSON-RPC pipe session, a shape the one-shot
//! `ShellRunDyn` cannot express, in a crate tokio cannot reach by deny.toml
//! law). Crates that run commands inject the kernel traits and receive
//! [`TokioShell`] in production, a mock in tests (Invariant #27).
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
//!   (dangerous basenames) PLUS the structural re-exec class (interpreter
//!   inline-eval `-c`/`-e`, `env`, `nc -e`, `dd if=`) — symmetric with shell
//!   mode but per-argv-element, so a literal arg is never a false positive.
//!   The predicate itself is [`nika_types::exec::argv_floor_refusal`] — the
//!   ONE the static `nika check` finding judges with too (#605 · check ≡
//!   run by construction, no mirrored table).
//!
//! `ShellError::Blocked` on a hit. `pre_validated` is the documented seam for
//! the policy layer that has already done intent-aware validation. The floor
//! is a TRIPWIRE, not a boundary — `permits.exec` + the sandbox are the real
//! gates (a glob/symlink in `$PATH` resolution is theirs to contain).
//!
//! # Egress proxy (the sandbox `allowlist` arm · ADR-095 Layer 6)
//!
//! The `egress` module is the per-run loopback proxy that gives the
//! sandbox's network tri-state its host granularity (the Anthropic
//! sandbox-runtime model): when a confined command carries
//! `NetPolicy::Allowlist`, the runner starts (or reuses) the ONE proxy,
//! injects the srt-mirrored env contract (`HTTP(S)_PROXY` / `ALL_PROXY` /
//! `NO_PROXY` … on the muxed CONNECT+SOCKS5 port), and lets the OS fence
//! admit loopback only. Every target is evaluated against the ONE host
//! matcher and every event journalised — the allow/refuse verdict, then
//! the relayed tunnel's byte counters at close (the F-P5 metering · see
//! the `egress` module doc).
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
mod egress;
/// The `permits:` → `SandboxSpec` derivation (ADR-095 Layer 6 · descended
/// from `nika-runtime::dispatch` at the 15k wall, ADR-110 · #889) — `pub`
/// because the runtime's dispatch still judges through it (L3→L1).
pub mod sandbox_spec;
mod scratch;

pub use egress::{EgressDecision, EgressEvent, EgressObserver};

use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use nika_kernel::command_sandbox::{CommandSandbox, CommandSandboxError};
use nika_kernel::process::{DANGEROUS_ENV_VARS, ShellCancelDyn, ShellRunDyn, compose_child_env};
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
/// Clones of one `TokioShell` share ONE loopback egress proxy (started lazily
/// on the first allowlisted exec, dying with the last clone — per-run).
#[derive(Clone)]
pub struct TokioShell {
    registry: Registry,
    /// The OS-confinement backend (injected by the wiring layer · `None` =
    /// no sandbox available, today's behavior). Applied AFTER the blocklist.
    sandbox: Option<Arc<dyn CommandSandbox>>,
    /// The per-run loopback egress proxy (the `NetPolicy::Allowlist` arm's
    /// enforcement half) — `Arc`-shared so every clone of this shell serves
    /// the SAME boundary on the SAME port; started lazily, stopped when the
    /// last clone drops (no orphan listener outlives the run).
    egress_proxy: Arc<Mutex<Option<egress::EgressProxy>>>,
    /// The egress-event journal sink (every proxy verdict — a refused
    /// host is a security event — plus each relayed tunnel's byte
    /// counters at close, F-P5). Default: the namespaced stderr line (see
    /// the `egress` module doc for the FCI-009 seam rationale).
    egress_observer: EgressObserver,
}

impl std::fmt::Debug for TokioShell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokioShell")
            .field("sandbox", &self.sandbox.as_ref().map(|s| s.backend()))
            .finish_non_exhaustive()
    }
}

impl Default for TokioShell {
    fn default() -> Self {
        Self {
            registry: Registry::default(),
            sandbox: None,
            egress_proxy: Arc::new(Mutex::new(None)),
            egress_observer: egress::stderr_journal(),
        }
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

    /// Replace the egress-event journal sink (the allowlist arm's
    /// allow/refuse verdicts + the relayed tunnels' byte counters). The
    /// default is the namespaced stderr line — the honest out-of-band
    /// channel (FCI-009 · see the `egress` module doc); tests and
    /// embedders wire a collecting probe here. Chainable.
    #[must_use]
    pub fn with_egress_observer(mut self, observer: EgressObserver) -> Self {
        self.egress_observer = observer;
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
        // launcher wrapper. `scratch` is the per-spawn private TMPDIR the
        // seatbelt arm minted (issue 754) — removed when the spawn settles.
        let (command, scratch, sandbox_classifier) = self.apply_sandbox(command)?;

        let start = Instant::now();
        let mut cmd = build_command(&command);
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

        // Register for out-of-band cancel-by-pid (ADR-016).
        let pid = child.id();
        let notify = pid.map(|p| self.register(p));

        // Take stdin before `child` moves into `wait_drain_feed`, which drains
        // stdout/stderr AND feeds this stdin CONCURRENTLY under the timeout
        // `select!` below. The pre-fix code wrote stdin SEQUENTIALLY, before
        // the timeout was armed, so a child echoing a larger-than-pipe-buffer
        // stdin (`cat`) deadlocked forever (review · HIGH).
        let stdin_feed = command
            .stdin
            .clone()
            .and_then(|data| child.stdin.take().map(|si| (si, data)));
        let child_fut = wait_drain_feed(child, stdin_feed);

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

        // The per-spawn scratch dies with the spawn (issue 754 · best-effort:
        // a leftover under the user temp is the OS reaper's to sweep, never
        // a correctness problem — the next spawn mints a fresh one).
        if let Some(dir) = scratch {
            let _ = std::fs::remove_dir_all(&dir);
        }

        outcome_to_result(outcome, pid, start, sandbox_classifier.as_deref())
    }
}

/// Wait for the child while draining BOTH output streams (bounded · INV-012)
/// AND feeding its stdin, all concurrently. Feeding must NOT precede the drain:
/// a child that echoes a stdin larger than the OS pipe buffer fills its stdout
/// and stops reading, so a sequential `write_all` deadlocks — and doing it
/// before the caller arms the timeout makes that hang unbreakable (review ·
/// HIGH). The feed never surfaces an error (a write to an early-exited child is
/// benign · its status rides `wait()`), so it cannot cancel the join; the
/// 4-tuple maps back to the 3-tuple the [`Outcome::Done`] contract expects.
async fn wait_drain_feed(
    mut child: tokio::process::Child,
    stdin_feed: Option<(tokio::process::ChildStdin, String)>,
) -> std::io::Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    let out = child.stdout.take();
    let err = child.stderr.take();
    let feed = async move {
        if let Some((mut stdin, data)) = stdin_feed {
            let _ = stdin.write_all(data.as_bytes()).await;
            // `stdin` drops at this scope end → EOF for the child.
        }
        Ok::<(), std::io::Error>(())
    };
    let (status, out_bytes, err_bytes, ()) = tokio::try_join!(
        child.wait(),
        drain(out, MAX_OUTPUT_BYTES),
        drain(err, MAX_OUTPUT_BYTES),
        feed,
    )?;
    Ok((status, out_bytes, err_bytes))
}

/// Map the `select!` [`Outcome`] to the public `Result` — a clean exit
/// becomes a [`ShellResult`] with the captured stdout/stderr; the
/// cancel/timeout arms report the typed error and the spawned child dies
/// via `kill_on_drop` (INV-011) when the `run()` future drops. Detached
/// grandchildren (`sh -c "... &"`) are NOT group-killed today: the
/// process-group kill rode arms the engine never reaches (the task
/// `timeout:` budget is never assigned to the command, `cancel` is never
/// invoked), so it was removed rather than kept as a tested-but-dead
/// promise — it returns with the wave that wires the task deadline into
/// the command (`linger: false`), where it can actually fire.
fn outcome_to_result(
    outcome: Outcome,
    pid: Option<u32>,
    start: Instant,
    sandbox: Option<&dyn CommandSandbox>,
) -> Result<ShellResult, ShellError> {
    match outcome {
        Outcome::Cancelled => Err(ShellError::Cancelled {
            id: pid.map_or_else(|| "?".to_string(), |p| p.to_string()),
        }),
        Outcome::TimedOut(ms) => Err(ShellError::Timeout { duration_ms: ms }),
        Outcome::Done(Err(e)) => Err(classify_drain_error(&e)),
        Outcome::Done(Ok((status, stdout, stderr))) => {
            let status = status.code().unwrap_or(-1);
            let stderr_text = String::from_utf8_lossy(&stderr);
            let classification = sandbox.map(|backend| {
                // The backend owns this conservative table: the runner has
                // drained the real launcher result and keeps its classifier
                // alive until this exact point.
                backend.classify_outcome(status, &stderr_text)
            });
            let result = ShellResult::new(
                status,
                String::from_utf8_lossy(&stdout),
                stderr_text,
                start.elapsed(),
            )
            // The raw octets ride alongside the lossy text projections —
            // the `decode:` pipeline (spec 09 §decode) reads bytes, never
            // a lossy string.
            .with_raw(stdout, stderr);
            Ok(match classification {
                Some(receipt) => result.with_adapter_outcome(receipt),
                None => result,
            })
        }
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

// The dangerous-env floor lives in the ONE canonical list,
// `nika_kernel::process::DANGEROUS_ENV_VARS` (imported above): the injection
// vectors that grant code execution or library injection with no dangerous
// flag in the command itself (the "env-var injection" class). The strip here
// is independent of `pre_validated` and runs AFTER the composed `env` map
// (the floor wins — a workflow does not pass these). Since NEP-0005 the
// child environment is CLEAN-SLATE: `env_clear` first, then exactly the
// composed map (`ShellCommand::env` — the runner floor ∪ the declared
// `permits.env:` passthrough ∪ the task's authored entries, composed
// upstream by the dispatch), then the dangerous strip. Nothing is inherited.

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
    // NEP-0005 clean slate: nothing is inherited. The child environment is
    // COMPOSED — the runner floor ∪ the declared `permits.env:` passthrough
    // (both resolved from the operator's ambient values HERE: reading the
    // ambient env to re-admit a curated subset to the child is the spawn
    // site's duty, the MCP stdio client's import-site precedent) ∪ the
    // authored `env` map, minus the dangerous floor (inside the compose).
    cmd.env_clear();
    #[allow(clippy::disallowed_methods)] // the spawn-site ambient read (see above)
    let composed = compose_child_env(
        |name| std::env::var(name).ok(),
        &command.env_passthrough,
        &command.env,
    );
    for (k, v) in &composed {
        cmd.env(k, v);
    }
    // SECURITY belt (always-on · independent of `pre_validated`): the compose
    // already stripped the dangerous floor — strip it again LAST at the spawn
    // boundary so a future compose regression cannot re-open the injection
    // class (see [`DANGEROUS_ENV_VARS`]).
    for var in DANGEROUS_ENV_VARS {
        cmd.env_remove(var);
    }
    if let Some(cwd) = &command.cwd {
        cmd.current_dir(cwd);
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

    #[tokio::test]
    async fn large_stdin_with_echoing_child_does_not_deadlock() {
        // Regression (review · HIGH): `cat` echoes stdin to stdout. A stdin
        // LARGER than the OS pipe buffer (~64 KiB) fills cat's stdout while we
        // are still writing, so cat stops reading stdin — a SEQUENTIAL
        // `write_all` (pre-fix) blocked there, and because it ran BEFORE the
        // timeout `select!` was armed, nothing could break it: a hang forever.
        // The feed now runs concurrently with the drain under the timeout, so
        // this completes and echoes every byte back. The outer test timeout
        // guards the SUITE against a re-introduced hang.
        let payload = "x".repeat(256 * 1024); // 4× a typical pipe buffer
        let mut cmd = ShellCommand::new("cat");
        cmd.stdin = Some(payload.clone());
        cmd.timeout = Some(std::time::Duration::from_secs(10));
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            TokioShell::new().run(cmd),
        )
        .await
        .expect("must not deadlock — stdin is fed concurrently with the drain")
        .expect("cat succeeds");
        assert_eq!(
            result.stdout.len(),
            payload.len(),
            "cat echoes all of stdin back"
        );
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

    /// Real macOS production path: Seatbelt wraps the command, the child
    /// encounters an undeclared read, drains plausible structured output,
    /// and exits in the launcher/refusal class. The runner must retain the
    /// Seatbelt classifier until after drain so the JSON cannot become data.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn seatbelt_status_126_with_json_is_typed_authority() {
        use nika_kernel::process::SandboxSpec;
        use nika_sandbox_seatbelt::SeatbeltSandbox;

        assert!(SeatbeltSandbox::available(), "macOS must ship sandbox-exec");
        let denied =
            std::env::temp_dir().join(format!("nika-w02-seatbelt-denied-{}", std::process::id()));
        std::fs::write(&denied, b"authority boundary").expect("denied fixture");
        let quoted = denied.to_string_lossy().replace('\'', "'\\''");
        let line = format!(
            "if /bin/cat '{quoted}' >/dev/null 2>&1; then exit 42; \
             else /usr/bin/printf '{{\"ok\":true}}\\n'; exit 126; fi"
        );
        let mut command = ShellCommand::new(line);
        command.shell = true;
        command.pre_validated = true;
        command.sandbox = Some(SandboxSpec::new());
        let result = TokioShell::with_sandbox(Arc::new(SeatbeltSandbox::new()))
            .run(command)
            .await
            .expect("the drained refusal is represented as ShellResult");
        let _ = std::fs::remove_file(denied);

        assert_eq!(result.status, 126, "the fixture reached the refusal arm");
        assert_eq!(result.stdout.trim(), r#"{"ok":true}"#);
        assert!(matches!(
            result.into_process_result(),
            Err(ShellError::Blocked { .. })
        ));
    }

    #[tokio::test]
    async fn child_env_is_composed_never_inherited() {
        // NEP-0005 law 1 · the clean slate. CARGO_PKG_NAME is reliably
        // present in a `cargo test` process env and is NOT on the runner
        // floor — it stands in for an ambient credential without the
        // (forbidden) unsafe env::set_var.
        let shell = TokioShell::new();
        let out = shell
            .run(ShellCommand::new("/usr/bin/printenv").arg("CARGO_PKG_NAME"))
            .await
            .expect("printenv runs");
        assert!(
            out.stdout.trim().is_empty(),
            "an ambient non-floor variable must never reach the child, got {:?}",
            out.stdout
        );

        // The runner floor still crosses (PATH is floor · law 1's "at most").
        let floor = shell
            .run(ShellCommand::new("/usr/bin/printenv").arg("PATH"))
            .await
            .expect("printenv runs");
        assert!(
            !floor.stdout.trim().is_empty(),
            "the runner floor must compose PATH into the child"
        );

        // The declared passthrough passes exactly the named variable (law 2).
        let mut declared = ShellCommand::new("/usr/bin/printenv").arg("CARGO_PKG_NAME");
        declared.env_passthrough = vec!["CARGO_PKG_NAME".to_owned()];
        let out2 = shell.run(declared).await.expect("printenv runs");
        assert_eq!(
            out2.stdout.trim(),
            env!("CARGO_PKG_NAME"),
            "a declared name must pass the ambient value through"
        );

        // The authored map wins over the passthrough on the same name (law 6).
        let mut authored = ShellCommand::new("/usr/bin/printenv").arg("CARGO_PKG_NAME");
        authored.env_passthrough = vec!["CARGO_PKG_NAME".to_owned()];
        authored
            .env
            .insert("CARGO_PKG_NAME".to_owned(), "authored".to_owned());
        let out3 = shell.run(authored).await.expect("printenv runs");
        assert_eq!(out3.stdout.trim(), "authored");
    }

    #[tokio::test]
    async fn program_resolution_rides_the_composed_floor_path() {
        // The flip must not brick relative-program spawns: the floor PATH is
        // in the composed map and `Command` resolves against the CHILD env.
        let out = TokioShell::new()
            .run(ShellCommand::new("echo").arg("resolved"))
            .await
            .expect("echo resolves via the composed floor PATH");
        assert_eq!(out.stdout.trim(), "resolved");
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
