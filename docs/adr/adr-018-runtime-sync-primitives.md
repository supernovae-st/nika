---
id: ADR-018
title: "Runtime + sync primitives: single Tokio rt, parking_lot default"
status: accepted
date: "2026-04-16"
phase: "Phase D -- Session S1-E (Wave 2)"
deciders: ["@ThibautMelen"]
tags: ["runtime", "tokio", "parking-lot", "concurrency", "lints"]
affects_crates: ["nika-kernel", "nika-kernel-mock"]
affects_layers: ["L0.5", "L1", "L2", "L3"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-013", "ADR-016", "ADR-017", "ADR-019"]
requires: []
enables: []
amends: []
fci: []
inv: ["INV-016", "INV-029"]
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.1, Wave 2 / Session S1-E"
follow_ups:
  - "Elevate clippy::await_holding_lock from suspicious=warn to deny in workspace lints"
  - "Add nika-lints::no_std_sync_mutex custom lint when nika-lints ships"
  - "Document runtime spawning rules for daemon vs CLI in agent docs"
---

# ADR-018: Runtime + sync primitives: single Tokio rt, parking_lot default

## Context

The workspace has an opinionated lint policy
(`Cargo.toml [workspace.lints.clippy]`) that already denies `unwrap_used`,
`expect_used`, `panic`, and the full `correctness` group. The `suspicious`
group, which contains `await_holding_lock`, is currently set to `warn` not
`deny`.

The kernel's `Clock` trait (`crates/nika-kernel/src/io/clock.rs`) demonstrates the
hybrid sync/async rule: `Clock::now()` is sync, `Clock::sleep(Duration)` is
async. Async appears only where it is mandatory (timer wakeups). Everything
else is sync.

The kernel's process module
(`crates/nika-kernel/src/io/process.rs:9-11`) explicitly states design decision
#9: cancel is `fn cancel(&self, id)`, not a `CancellationToken` field. The
intent is "tokio-util out of nika-kernel".

All kernel collections use `BTreeMap` (per ADR-006) for deterministic
iteration. `HashMap` is allowed only in private internals where iteration
order does not leak.

INV-016 reminds: `parking_lot::RwLockReadGuard` is `!Send`. Holding a guard
across `.await` is unsound; INV-029 adds the Loom test requirement (ADR-013)
for any crate with concurrent primitives.

## Decision

**One runtime, one synchronous lock library, one rule for guards across
`.await`.**

- **Runtime: a single Tokio multi-threaded runtime per process.** Daemons
  (nika-daemon) spawn the runtime in `main`. The CLI uses
  `#[tokio::main(flavor = "multi_thread")]`. No `new_current_thread` in
  production code paths. Tests may use either flavor.
- **Synchronous locks: `parking_lot::Mutex` / `parking_lot::RwLock`** by
  default. `std::sync::Mutex` is banned in production code (future
  `nika-lints` custom rule will enforce).
- **`clippy::await_holding_lock` will be elevated to `deny`** in
  `[workspace.lints.clippy]`. Currently caught by `suspicious = "warn"`;
  the upgrade lands in a separate "lint hardening" commit so this ADR can
  ship docs-only.
- **Async sync primitives:**
  - `tokio::sync::Semaphore` for concurrency limiting (e.g., `for_each`
    parallelism cap, max parallel provider calls).
  - `tokio::sync::oneshot` for single-value async handoff.
  - `tokio::sync::watch` for configuration hot-reload broadcasts.
  - `tokio::sync::Notify` for wake-up coordination without a value.
  - `tokio::sync::Mutex` only when the critical section spans `.await` —
    otherwise `parking_lot::Mutex` (no async overhead).
- **No third-party runtimes.** `async-std`, `smol`, `glommio`: not in the
  dep tree, not allowed.
- **Lock-across-`.await` is forbidden.** INV-016 is the day-to-day rule.
  Guards live in tight scopes; if data is needed across yield points, clone
  it out, drop the guard, then `.await`.

## Consequences

### Positive

- Single runtime ⇒ predictable scheduling, no cross-runtime deadlocks, one
  set of metrics to monitor.
- `parking_lot` locks are faster under contention than stdlib (no
  poisoning, smaller guard size, faster uncontended path).
- Banning `std::sync::Mutex` removes the poisoning footgun and aligns the
  whole workspace on one lock library.
- Elevating `await_holding_lock` to `deny` makes INV-016 a compile-time
  error, not a runtime hazard.
- Small async-sync surface (Semaphore / oneshot / watch / Notify) is easy
  to teach.

### Negative

- New contributors familiar with `std::sync::Mutex` will hit the lint and
  need orientation. Mitigated by the workspace-level deny + module-level
  docs.
- `tokio::sync::Mutex` is intentionally rare; teams used to it as a default
  must adjust. Documented in module headers.
- Single runtime means the daemon and the CLI share a tokio version. Forces
  workspace dep alignment but that is already enforced by Cargo.

### Neutral

- Loom tests (ADR-013, INV-029) verify the lock discipline for crates with
  concurrent primitives. ADR-018 does not change the Loom strategy, just
  the lock library on top.

## Evidence / Affected code

- `Cargo.toml [workspace.lints.clippy]` — current lint policy
  (`unwrap_used = "deny"`, `panic = "deny"`, `correctness = { level = "deny" }`).
- `Cargo.toml [workspace.dependencies] parking_lot` — already pinned.
- `crates/nika-kernel/src/io/clock.rs` — sync/async hybrid pattern.
- `crates/nika-kernel/src/io/process.rs:9-11` — design decision #9 (no
  tokio-util in kernel).
- `crates/nika-kernel-mock/Cargo.toml:21` — `parking_lot = { workspace = true }`
  in mocks (validates the rule).
- `docs/architecture/forward-compat-invariants.md` — INV-016, INV-029.
- ADR-013 — Loom concurrency verification.

## Alternatives considered

### Alt A -- `std::sync::Mutex` everywhere
"It's in the standard library, no extra dep."
Rejected: stdlib `Mutex` poisons on panic (every `lock()` returns `Result`,
forcing `.unwrap()` patterns we have already banned). `parking_lot` is
faster, smaller, and panics-clean.

### Alt B -- `async-std` runtime
Single-file runtime, looks tempting.
Rejected: ecosystem consolidation. `rig-core`, `reqwest`, `axum`, `tokio-postgres`
all require Tokio. Adding `async-std` would force compatibility shims for
every L1 crate.

### Alt C -- Lock-free data structures everywhere
`crossbeam`, `dashmap`, atomics-only.
Rejected: overkill for kernel data. `parking_lot` is the right complexity
level for our access patterns. `DashMap` is fine in specific hotspots
(catalog lookup), but is not the default.

### Alt D -- `tokio::sync::Mutex` as default
"Async-aware, never blocks."
Rejected: every `lock().await` is a yield point; using it as default would
sprinkle yields in code that does not need them, and slows uncontended
paths. Reserve `tokio::sync::Mutex` for the (rare) case where the critical
section legitimately spans `.await`.

### Alt E -- Multiple runtimes per process
One runtime for I/O, one for compute (ML inference).
Rejected: cross-runtime channels are subtle and prone to deadlock; the
benefit (CPU-bound work isolation) is better served by `tokio::task::spawn_blocking`
or a dedicated `rayon` pool — neither requires a second runtime.

## Related

- ADR-006 — kernel ISP trait design. ADR-018 is the runtime substrate
  underneath ADR-006's traits.
- ADR-013 — Loom concurrency verification (INV-029). ADR-018 chooses the
  lock library; ADR-013 verifies it under concurrent stress.
- ADR-016 — Cancellation model. Both ADRs share the "no tokio-util in
  kernel" rule.
- ADR-017 — Streaming policy. Bounded mpsc(32) is the streaming face of
  this runtime choice.

## Notes

Follow-ups:

- Elevate `clippy::await_holding_lock` from `suspicious = "warn"` to
  `deny` in the workspace lints. Land in a separate "lint hardening"
  commit so that any preexisting violations are surfaced + fixed in
  isolation.
- Add `nika-lints::no_std_sync_mutex` custom lint when `nika-lints`
  ships (Phase 4+).
- Document daemon vs CLI runtime spawning rules in the agent self-serve
  docs (Phase E).
