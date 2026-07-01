# Crate spec — `nika-verb-exec`

| | |
|---|---|
| Status | **SPEC** (Gate 1 · authored 2026-06-11 · announce-ladder step s10 · night arc · follows s9 `nika-verb-infer`) |
| Layer | **L2** — verb crate · domain executor for the `exec` verb (4 verbs locked forever per D-2026-05-22-N18) |
| Design | consumes the L0.5 kernel `io::process` seam (`ShellRunDyn` injected · the wiring layer hands it `nika-exec-runner::TokioShell`) · zero subprocess code of its own · `pre_validated = false` ALWAYS (the s7 effect-layer blocklist is the floor · the verb never bypasses it) |
| LOC budget | ≤2k src (brouillon reference ~600 LOC lib.rs + error) · caps ≤1500/file · ≤15k/crate |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L2 verb crate |
| NIKA codes | **NIKA_440–449** claimed inside the Verb range 430–479 (s9 took 430–439) · maps to spec `NIKA-EXEC-001/002` (`spec/05-errors.md:91-92`) |

---

## §0 · Architecture — the seam (verified 2026-06-11)

1. **The shell contract is L0.5-complete.** `nika-kernel-core/src/io/process.rs`
   ships `ShellCommand` (program · args · env · env_remove · cwd · timeout ·
   stdin · shell · pre_validated · all `#[non_exhaustive]`) · `ShellResult`
   (status · stdout · stderr · duration) · `ShellRunDyn` / `ShellCancelDyn`
   atomic traits (+ `ShellExecutor` blanket). CANCEL SAFETY rides INV-011
   `kill_on_drop` (the s7 runner sets it).
2. **The effect is injected, never owned.** The verb takes `Arc<S>` with
   `S: ShellRunDyn` — production wiring injects `nika-exec-runner::TokioShell`
   (s7 · admitted · NFKC/zero-width/quote-bypass blocklist + concurrent drain
   + hard-kill timeout); tests inject `nika-kernel-mock::MockShell` (scripted
   results · call recording). NO Cargo dep on `nika-exec-runner` — L2 reaches
   the L1 effect through the kernel trait only (same inversion as every
   effect consumer).
3. **The language contract** — `spec/02-verbs.md §exec`: required `command`
   (CEL-resolved upstream) · optional `cwd` / `env` (OS env of THIS subprocess
   · NOT the envelope `env:`) / `stdin` / `capture` (stdout default · stderr ·
   combined · structured). `timeout` is task-level (03-dag) — the engine
   resolves it and passes it down as an input param; the runner enforces the
   hard kill.
4. **The one-obvious-way split (spec §exec conformance)** · default capture
   modes FAIL the task on non-zero exit (`NIKA-EXEC-001`) · `capture:
   structured` returns `{ stdout, stderr, exit_code }` as DATA — non-zero is
   the workflow's to branch on, the task succeeds.

```text
   future L3 nika-engine ── schedules ──┐ (timeout resolved task-level)
                                        v
   L2  nika-verb-exec    run(ExecInput) → ExecOutput
         │ Arc<S: ShellRunDyn>  (pre_validated = false · blocklist floor holds)
         v
   L1  nika-exec-runner::TokioShell (prod) · nika-kernel-mock::MockShell (test)
   L0.5 nika-kernel-core io::process  ShellCommand / ShellResult / ShellError
```

## §1 · Public API (admission shape)

```rust
pub struct ExecVerb<S> { shell: Arc<S> }

#[non_exhaustive]
pub struct ExecInput {
    pub command: String,                      // required · CEL-resolved
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,        // subprocess OS env
    pub stdin: Option<String>,
    pub capture: CaptureMode,                 // default Stdout
    pub timeout: Option<Duration>,            // task-level · engine-resolved
}

#[non_exhaustive]
pub enum CaptureMode { Stdout, Stderr, Combined, Structured }

#[non_exhaustive]
pub enum ExecValue {
    Text(String),                             // stdout | stderr | combined
    Structured { stdout: String, stderr: String, exit_code: i32 },
}

#[non_exhaustive]
pub struct ExecOutput {
    pub output: ExecValue,
    pub duration: Duration,                   // from ShellResult
}

impl<S: ShellRunDyn + Send + Sync + 'static> ExecVerb<S> {
    pub fn new(shell: Arc<S>) -> Self;
    /// CANCEL SAFETY: cancel-safe via the runner's kill_on_drop (INV-011).
    pub async fn run(&self, input: ExecInput) -> Result<ExecOutput, VerbExecError>;
}
```

Command dispatch: `ShellCommand::new(command)` with `shell = true` (spec:
« run via the OS shell ») · `pre_validated = false` forever — the runner's
blocklist + NFKC defenses stay armed; a future `nika-policy` layer composes
ABOVE without this crate changing.

## §2 · Error model (one-voice · vector 37)

| Code | Variant | Spec mapping | transient |
|---|---|---|---|
| NIKA_440 | `NonZeroExit { status, stderr_tail }` (default capture modes only) | NIKA-EXEC-001 | `false` |
| NIKA_441 | `Shell` (wraps `ShellError` · spawn/blocklist/timeout) | NIKA-EXEC-002 | inherited from `ShellError` |
| NIKA_442 | `InvalidParam` (empty command) | upstream-reject class | `false` |

NIKA_443–449 reserved (future: stdin-pipe failures · output caps).

## §3 · Scope fences (what this crate is NOT)

- **NOT the blocklist** — security validation lives in the s7 runner (floor)
  and the future `nika-policy` (s8 · design locked). `pre_validated` is never
  set by this crate.
- **NOT `${{ }}` resolution** — upstream binding.
- **NOT retry/timeout policy** — task-level (engine); the verb passes the
  resolved timeout down to `ShellCommand`.
- **NOT output truncation** — the brouillon `max_stdout` knob is NOT in the
  v0.1 spec; the runner's own caps apply. Seam note: NIKA_443+ reserved.
- **NOT cancel-by-id** — `ShellCancel` is the engine/daemon surface.
- **NOT `env_remove`** — the brouillon's sensitive-env stripping moves to the
  policy layer (engine-wide concern, not per-task spec surface).

## §4 · Testing strategy (Gates 2–7)

- **TDD** mock-first (`MockShell` scripted): command passthrough shaping ·
  capture-mode matrix (stdout/stderr/combined/structured) · non-zero exit →
  NIKA_440 in default modes BUT data in structured mode (the one-obvious-way
  split — THE load-bearing behavior) · empty command → NIKA_442 zero calls ·
  ShellError passthrough → NIKA_441 · env/cwd/stdin/timeout land on
  ShellCommand · `pre_validated` stays false (pinned).
- **Property** (Gate 6): capture-mode total function over arbitrary
  status/stdout/stderr triples · combined = stdout⧺stderr invariant.
- **Mutation** (Gate 5): ≥90 %.
- **Parity** (Gate 10): pinned vs brouillon `tools/nika-verb-exec` behaviors
  (delegate-to-kernel-trait · stdout default · structured exit-code shape).
- **Canary** (Gate 9): N/A — no L3 runner (lands step 17).
- **Benchmarks** (Gate 7): N/A — subprocess-bound.

## §5 · Wiring pass

L2 row exists since s9 (rank · refresh-status · roadmap). Remaining:
`.gitignore` lift (`/crates/nika-verb-exec/`) · `Cargo.toml` members +
`layers.nika-verb-exec = "L2"` + wip · `deny.toml` tokio wrapper (dev-dep
`#[tokio::test]`).

## §6 · Dependencies

```toml
[dependencies]
nika-error  = { path = "../nika-error",  version = "0.90.0" }
nika-kernel = { path = "../nika-kernel", version = "0.90.0" }
miette · thiserror
[dev-dependencies]
nika-kernel-mock · proptest · tokio (test rt)
```

(No `serde_json` — `Structured` is a typed variant, the engine serializes.)

## §7 · Update log

```
2026-06-11  v0.1 — Gate 1 SPEC authored (night arc · s10 · post-s9 template) ·
              seam verified empirically (kernel io::process · s7 runner ·
              MockShell · spec §exec one-obvious-way split · brouillon
              read-only reference).
```
