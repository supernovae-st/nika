# Crate spec — `nika-clock`

| | |
|---|---|
| Status | **ADMITTED 2026-05-24** (`74a8ff483`) · was **L1 admission target** (first time-effect crate · pairs with nika-event) |
| Layer | L1 — effect crate · the only production site touching `tokio::time` + `std::time` |
| Design | `SystemClock` ZST impl of the L0.5 `nika_kernel::Clock` trait |
| LOC budget | ≤200 src (actual ~62) |
| Function cap | ≤100 lines each (all trivial wrappers) |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L1 effect crate |
| NIKA codes | none — `SystemClock` is infallible (no error enum) |

---

## 1. Purpose

`nika-clock` is the **production time effect**. It provides `SystemClock`,
the real-system-time implementation of the L0.5 `nika_kernel::Clock` trait
(monotonic `now`, wall `system_now`, async `sleep`, defaulted `elapsed`).

It is the **only** place `tokio::time` and `std::time::{Instant,
SystemTime}` are touched on the production path — pure crates (L0) and the
kernel (L0.5) stay clock-free; tests inject the kernel-mock clock. This is
the effect-crate discipline (Invariant #27 · Clock injection for test
hermeticity): one trait impl per crate.

---

## 2. Public API

```rust
/// Zero-size production clock. Copy + Default.
pub struct SystemClock;

impl nika_kernel::Clock for SystemClock {
    fn now(&self) -> Instant;            // Instant::now()
    fn system_now(&self) -> SystemTime;  // SystemTime::now()
    async fn sleep(&self, Duration);     // tokio::time::sleep
    // elapsed(&self, since) -> Duration  — trait default
}
```

Implements the `trait_variant`-generated `ClockDyn` companion (the
`Send`-future form); the base `Clock` arrives via the blanket impl.
`ClockDyn` is a generic bound, NOT a dyn-dispatch surface (RPITIT is
not object-safe — fan out via `Arc<SystemClock>`, not `Arc<dyn _>`).
*Corrected 2026-06-10: the original claim («object-safe · `&dyn ClockDyn`
fan-out works») was false on both counts — caught by the nika-fs
admission swarm.*

---

## 3. The 12 gates

| Gate | Status | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file |
| 2 TDD | ✅ | `tests/clock_contract.rs` · 11 integration (2 async) + 1 doctest |
| 3 IMPL | ✅ | ~62 LOC src · zero unwrap/expect in src |
| 4 CLIPPY 0 | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` GREEN |
| 5 MUTATION ≥90% | ✅ | `cargo mutants -p nika-clock` · 1/1 viable caught = 100% (2 unviable) |
| 6 PROPERTY | N/A | no parser/encoding/security surface — timing behaviour covered by async tests (justified) |
| 7 BENCH | N/A | trivial std/tokio wrappers, no hot path (justified) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc` 0 warnings |
| 9 CANARY | N/A | L1 effect, no `.nika.yaml` surface (justified) |
| 10 PARITY | ✅ | brouillon `tools/nika-clock` `SystemClock` exists; tests assert the SAME properties (monotonic `now` · `sleep` advances · `elapsed` non-negative). Diamond trait ADDS `system_now` (wall clock) + uses `trait_variant` native-async (brouillon used `async_trait`) — CRAFT-fresh against the evolved trait per ADR-001 |
| 11 REVIEW | ✅ | spn-rust:rust-pro + Foreman-direct (model-context-required fallback PE-5.1) |
| 12 ATOMIC | ✅ | 1 commit · Nika 🦋 trailer |

---

## 4. Consumers (downstream)

Every crate needing time injects `&impl Clock` (or `&dyn ClockDyn`) and
receives `SystemClock` in production, the kernel-mock in tests. First
consumers: L2 verb crates (timeouts, retry backoff), L3 runtime
(workflow scheduling, `nika-event` timestamp source at emit sites).
