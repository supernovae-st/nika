# Crate spec — `nika-exec-runner`

| | |
|---|---|
| Status | **ADMITTED 2026-06-10** (`7ba7b51d8`) · was **L1 admission target** (Phase-B slice step 7 · announce floor `exec` · per D-2026-06-10-N6) |
| Layer | L1 — effect crate · the only production site spawning PLAIN subprocesses (`tokio::process`) — one deliberate second site: `nika-mcp`’s stdio MCP client (a persistent pipe session the one-shot shell seam cannot express) |
| Design | `TokioShell` impl of the L0.5 `nika_kernel::process` traits (`ShellRun` + `ShellCancel`) via the `*Dyn` (`Send`) companions · SECURITY-SENSITIVE (command blocklist + injection defense) |
| LOC budget | well under the ≤1500/file + ≤15k/crate caps (enforced live by vectors 12+24) · live count · `scripts/crate-metrics.sh nika-exec-runner` |
| LOC (live) | ~3755 LOC src (live · `scripts/crate-metrics.sh nika-exec-runner`) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L1 effect crate |
| NIKA codes | none — speaks the kernel `ShellError` (`NotFound` · `Timeout` · `Cancelled` · `Blocked` · `Other`) |

---

## 1. Purpose

`nika-exec-runner` is the **production shell-execution effect**. It provides
`TokioShell`, the real-subprocess implementation of the L0.5 kernel traits
(`ShellRun` + `ShellCancel` — and therefore the blanket `ShellExecutor`)
backed by `tokio::process::Command`, with a **command blocklist in the effect
crate itself**: workflows run attacker-influenced commands, so the mechanism
must be safe-by-default even before `nika-policy` (L1.5 · step 8) adds richer
capability gating.

The only production site spawning PLAIN subprocesses (the second, deliberate site is `nika-mcp`’s stdio MCP client) — tests inject a mock.
Effect-crate discipline (Invariant #27).

It also hosts the **loopback egress proxy** (`src/egress.rs`) — the
`sandbox.net = allowlist` arm's enforcement half (ADR-095 Layer 6 · the
Anthropic sandbox-runtime model): a per-run `127.0.0.1:0` mux serving HTTP
CONNECT + SOCKS5 (protocol-sniffed), evaluating every target against the
ONE host matcher (`nika_types::net::host_in_allowlist`), injecting the
srt-mirrored env contract into the confined child, and journalising every
allow/refuse decision (the `EgressObserver` seam — default: the namespaced
stderr line).

## 2. Public API

```rust
pub struct TokioShell { /* Arc<Mutex<registry>> for cancel-by-pid */ }

impl TokioShell {
    pub fn new() -> Self;   // Clone (Arc-shared registry) · Default
    pub fn with_egress_observer(self, o: EgressObserver) -> Self;  // the journal seam
}
pub struct EgressDecision { pub host, pub port, pub allowed }  // one proxy verdict
pub type EgressObserver = Arc<dyn Fn(&EgressDecision) + Send + Sync>;
impl ShellRunDyn    for TokioShell { run }
impl ShellCancelDyn for TokioShell { cancel }
```

Targets the `*Dyn` trait-variant companions (`Send` futures · base traits +
`ShellExecutor` umbrella via the blanket impl · same pattern as
`nika-fs`/`nika-http`/`nika-blob`).

## 3. Security design (the heart of this crate)

### 3.1 Blocklist — in the mechanism (canon: `pre_validated` seam)

The kernel `ShellCommand` carries `pre_validated: bool` ("skip the executor's
default blocklist check · set by callers that have already performed
intelligent validation") and `ShellError::Blocked` exists — so canon places
the **baseline blocklist in the executor**. Defense-in-depth: this crate is
the safe-by-default floor; `nika-policy` (step 8) is the sophisticated layer
that may set `pre_validated` after its own (intent-aware) validation. Mirrors
the SSRF-in-`nika-http` decision (mechanism safe-by-default · policy on top).

Unless `command.pre_validated`, `run()` checks the full command string
(program + args joined) against the blocklist BEFORE spawning; `shell: true`
additionally checks the shell-mode blocklist (`alias`/`function`/`declare -f`)
**and refuses shell expansion/substitution chars** (`$VAR` · `${IFS}` ·
`$(...)` · `$'\t'` · backticks). The latter closes a structural TOCTOU
(Gate-11 swarm P1): `sh -c` expands those AFTER a template-only check, so
`rm${IFS}-rf${IFS}/` would render to `rm -rf /` past the blocklist. A baseline
executor cannot safely predict the expansion → it refuses; `nika-policy` (which
sets `pre_validated`) is where validated `$VAR` use is allowed. Plain
pipes/redirects (no `$`/backtick) are unaffected.

### 3.2 Bypass defenses (CRAFT-preserved from the battle-tested brouillon · do NOT weaken)

The blocklist normalizes BEFORE matching — each layer closes a real bypass:
1. **NFKC normalization** — fullwidth/confusable Unicode → ASCII (`ｒｍ` → `rm`).
2. **Zero-width stripping** — 7 invisible chars (ZWSP, ZWNJ, ZWJ, BOM, soft
   hyphen, word-joiner, Mongolian vowel sep) that NFKC preserves (`r​m` → `rm`).
3. **Whitespace collapse** — runs of whitespace → single space.
4. **Quote dequoting** — strip `"`/`'`/`\` (`su""do` → `sudo`).
5. **Basename resolution of the first token** — `/usr/bin/sudo rm` → `sudo rm`.
6. **Full-string scan** — NO length cap (SEC-1: a 8000-char pad then `&& rm -rf /`
   must still match · the brouillon's 4KB-limit bug is not reproduced).
7. **Boundary sentinels** (Diamond hardening · found by the Gate-6 proptest) —
   `split_whitespace().join(" ")` TRIMS leading/trailing whitespace, so a
   boundary-space pattern (`"sudo "` · `"; rm "` · `" -exec "`) would miss when
   the command begins/ends AT the pattern (`foo; rm` · `find -exec`). Every
   haystack is wrapped ` {proj} ` before matching to restore those edges —
   strictly safe-side (only adds matches).
Match runs against all four projections (lower · basename · dequoted ·
basename-dequoted) so a pattern hidden behind any one transform is caught.

### 3.3 Process safety (kernel CANCEL SAFETY contract · INV-011/012)

- **`kill_on_drop(true)`** on the `tokio::process::Command` (INV-011) — dropping
  the `run()` future sends SIGKILL to the child · no orphan/zombie processes.
  This IS the PRIMARY cancellation per ADR-016 (future-drop · the common case).
- **Concurrent stdout/stderr drain with `wait()` via `tokio::try_join!`**
  (INV-012) — a child writing > the OS pipe buffer (~64 KB Linux / ~16 KB macOS)
  would deadlock if we waited-then-read · all three futures poll in parallel.
- **stdin** piped + written when `command.stdin` is set, else null.
- **env / env_remove / cwd** applied (env_remove after env so removal wins).
- **timeout** via `tokio::time::timeout` → `ShellError::Timeout`.
- program-not-found → `ShellError::NotFound`.

### 3.4 `cancel(id)` — registry-backed kill-by-pid (ADR-016)

ADR-016: `ShellCancel` is "kill-by-id for subprocesses · different problem
class · the OS kills." The id is the **OS pid** (string · matching the trait
doc "signalling an already dead pid is a harmless no-op"). `TokioShell` holds
an `Arc<Mutex<BTreeMap<pid, Notify>>>`; `run()` registers the spawned child's
pid after spawn and `select!`s its wait against the registered `Notify`;
`cancel(pid)` notifies it → the child is killed (kill_on_drop fires).
Unknown/dead pid → `Ok(())` (idempotent · trait-compliant). Deregisters on
exit. (`run()`'s future-drop remains the primary path; this is the explicit
out-of-band kill the daemon/engine will use.)

### 3.5 Output capping (resource safety · NIKA-054 · post-admission hardening)

`timeout` bounds wall-clock, NOT memory: a runaway writer (`yes`,
`cat /dev/zero`) at ~1 GB/s would grow the capture buffer to ~30 GB under a
30 s timeout and OOM the host before the timeout fires. Each stream (stdout
AND stderr) is therefore capped at **64 MiB** (`MAX_OUTPUT_BYTES`):

- `drain` reads at most `limit + 1` bytes via `AsyncReadExt::take` — the `+1`
  makes overflow detectable while never buffering more than one byte past the
  cap (no OOM even on an infinite stream).
- On overflow `drain` returns an `OutputCapExceeded` marker through
  `tokio::io::Error`; `try_join!` short-circuits, the child future drops, and
  `kill_on_drop` (INV-011) SIGKILLs the writer — so the bounded read does NOT
  reintroduce the INV-012 pipe-fill deadlock.
- The marker maps to `ShellError::OutputTooLarge { limit_bytes }` (`NIKA-054`)
  at the single exit site. Fail-closed, aligned with the `nika:read` 50 MB
  cap precedent. Commands needing larger output redirect to a file in-command
  and read it back (or go through `pre_validated` policy with its own limits).

## 4. The 12 gates

| Gate | Status | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file |
| 2 TDD | ✅ | `tests/exec_contract.rs` (14 subprocess contract) + blocklist/lib unit (25) authored first · RED → GREEN |
| 3 IMPL | ✅ | ~3755 LOC src (live · `scripts/crate-metrics.sh nika-exec-runner`) · zero unwrap/expect in src |
| 4 CLIPPY 0 | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` GREEN |
| 5 MUTATION ≥90% | ✅ | `cargo mutants -p nika-exec-runner` · 28 mutants · **23 caught / 25 viable = 92%** (3 unviable). 2 documented survivors, both non-security: (a) the `basename_normalized` OR-branch — an EQUIVALENT mutant (for the current patterns `basename(s).contains(p) ⇒ lower.contains(p)`, so it never uniquely matches · pure defense-in-depth redundancy that would only bite a future mid-path pattern); (b) the `-1` exit sentinel for signal-death (`status.code()==None`) — cosmetic, no control-flow/security impact. The killers added: dequoted-sole-matcher (`/d'e'v/tcp/`) + quote-bypass-via-dequoting + basename helper + shell-expansion regression set. |
| 6 PROPERTY | ✅ | security unit-battery: each normalization layer (NFKC/zero-width/quote/basename) blocked · shell-expansion bypasses ($IFS/$VAR/$()/backtick/fullwidth-$) refused · safe commands + plain pipes allowed |
| 7 BENCH | N/A | subprocess-bound, no algorithmic hot path (justified) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` 0 warnings · private-item rustdoc clean (vector 28) · per-method CANCEL SAFETY + §3 |
| 9 CANARY | N/A | L1 effect, no `.nika.yaml` surface until L2 verb-exec (step 11) (justified) |
| 10 PARITY | ✅ | all brouillon blocklist bypass vectors re-asserted (quote · NFKC · zero-width · absolute-path · full-string-scan · priv-esc · reverse-shell · Windows) · Diamond ADDS registry cancel-by-pid (drops brouillon's removed `command.cancel` token · ADR-016) + the shell-expansion-refusal P1 hardening |
| 11 REVIEW | ✅ | adversarial security review (spn-nika:code-reviewer) found **2 P1 blocklist bypasses** ($IFS-expansion + env-var-indirection in shell:true) → BOTH FIXED same-session (shell-expansion-char refusal) + regression-pinned. rust-pro + feature-dev hit the session reset; their dimensions self-verified (the security reviewer's notes cross-confirmed: std-Mutex never across await · kill_on_drop pre-spawn · try_join concurrent drain INV-012 · register-after-cancel window acceptable since kill_on_drop is primary). P2s documented §3 (over-blocks · pre_validated pub field · cancel-vs-completion race) — Round-2 ratchet candidates (all safe-side). |
| 12 ATOMIC | ✅ | 1 commit · Nika 🦋 trailer |

## 5. Consumers (downstream)

`nika-policy` (step 8 · wraps with capability gating · may set `pre_validated`),
`nika-verb-exec` (step 11 · the `exec:` verb · composes exec-runner + policy),
`nika-builtin` (step 16 · exec-backed builtins). The blocklist default-on
posture is what lets `exec:` be safe-by-construction at the 1.0 launch.

## 6. Dependencies

| dep | why | layer-legal |
|---|---|---|
| `nika-kernel` (path) | trait + type + error contracts | L0.5 ← L1 ✓ |
| `tokio` (`process` · `io-util` · `sync` · `time` · `macros` · `rt`) | subprocess + concurrent drain + registry Notify + timeout | L1+ ✓ |
| `unicode-normalization` | NFKC confusable-bypass defense (MIT OR Apache-2.0) | ✓ |
| dev: `proptest` | Gate 6 security properties | dev-only |

deny.toml `tokio` wrapper extended with nika-exec-runner. `unicode-normalization`
added to `[workspace.dependencies]` (RUST_ENFORCEMENT §2 pin-once).
