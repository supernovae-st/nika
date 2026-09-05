# nika-types

Foundation value types for the Nika diamond — pure, zero I/O, zero async.

Layer **L0** crate. Ships the 18 cross-cutting value types every higher
layer composes on : `Cost`, `Trust`, `Budget`, `Baggage`, `Cancel`,
`Checkpoint`, `Compression`, `Embedding`, `Hash`, `Id`, `Memory`,
`Resource`, `Retry`, `Role`, `Schema`, `Timestamp`, `TokenUsage`, plus
their builder shims. No I/O, no async, no `nika-*` siblings — these are
the leaves of the dependency graph.

## Usage

```rust
use nika_types::{Cost, TokenUsage, Trust, Timestamp};

let cost = Cost::new()
    .input_usd(0.003)
    .output_usd(0.015);

let tokens = TokenUsage::new()
    .input(1_240)
    .output(380);

let now = Timestamp::now_utc();
let trust = Trust::Untrusted;
```

Every public struct is `#[non_exhaustive]` and constructed via
`Type::new()` (Invariant #19), so adding fields in v0.x is non-breaking.

## Features

| feature | default | what it gates |
|---|---|---|
| `std` | yes | `std::`-only paths (seam for WASM v0.100, ADR-028) |
| `serde` | yes | `serde` + `serde_json` + `uuid` for round-trip |

`no_std`/`no_serde` builds compile against `core` + `alloc` only — kept
working so the WASM seam stays open.

## Loom

Concurrency-sensitive types ship Loom interleaving tests. Run with :

```
RUSTFLAGS="--cfg loom" cargo test --locked -p nika-types --lib loom_cancel
```

`loom` is a `cfg(loom)` dep only — invisible to the normal workflow.

Diamond CI's tests leg also runs `bash scripts/ci/check-loom-cancel.sh`,
which refuses an absent payload model. The model checks visibility of a
separate relaxed payload before joining the cancelling thread; weakening
either cancellation ordering to Relaxed fails it. This proves the modeled
publication edge, not scheduler progress or external-effect cancellation.

## MSRV

Rust 1.91+.

## License

AGPL-3.0-or-later. Co-author `Nika 🦋 <nika@supernovae.studio>`.

## Related

- `docs/architecture/forward-compat-invariants.md` — Invariant #19 (`new()` constructors), `#[non_exhaustive]` ratchet
- `docs/architecture/crate-layer-registry.md` — L0 contract (zero I/O, zero async)
- `docs/adr/adr-006-monolithic-kernel.md` — sibling crate (`nika-kernel`) consumes these types
- `docs/adr/adr-028-wasm-feature-seams.md` — `std`/`serde` feature design
- `BLUEPRINT_2036.md` — collapse-vs-publish principle (foundation crates stay collapsed)
