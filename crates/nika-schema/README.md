# nika-schema

Workflow AST, parser, analyzer, and DAG validation — pure, zero I/O, zero async.

> **Status · ADMITTED** (2026-06-18 · all 12 gates · the last L0 crate).
> The workflow AST, parser, analyzer, and the ADR-092 `nika check` static
> ladder. Gate 5 closed in BUDGET mode (survivors ≤ 300); see the crate
> spec for the full gate table. Sibling crates depend on it freely.

Layer **L0**. Parses the canonical `nika: v1` envelope
(per [nika-spec]) into a typed AST, validates the DAG of tasks, and surfaces
diagnostics via [miette] so the CLI can render rich error spans. The
parser is a two-stage pipeline : `marked-yaml` (source-span preserving) →
`serde-saphyr` (typed deserialize) → analyzer (DAG checks + guardrails).

## Usage (planned · subject to change pre-admission)

```rust
use nika_schema::{Workflow, ParseError};

let yaml = std::fs::read_to_string("hello.nika.yaml")?;
let workflow: Workflow = nika_schema::parse(&yaml)?;

// Inspect the DAG
for task in workflow.tasks() {
    println!("{} → {:?}", task.id(), task.depends_on());
}
```

## Modules

- `raw/` — `marked-yaml` source-span representation (Stage 1)
- `parser/` — typed deserialize via `serde-saphyr` (Stage 2)
- `types/` — workflow AST (`Workflow`, `Task`, `Step`, …)
- `source/` — span tracking for diagnostics
- `guardrails/` — DAG analyzer (cycle detection, reachability, depth limits)
- `trust.rs` — trust-level propagation across the DAG
- `error.rs` — `#[non_exhaustive]` error enum with miette `Diagnostic` derive

## MSRV

Rust 1.91+.

## License

AGPL-3.0-or-later. Co-author `Nika 🦋 <nika@supernovae.studio>`.

## Related

- `docs/adr/adr-021-yaml-envelope-convention.md` (superseded) + `nika-spec` spec/01-envelope.md — `nika: v1` envelope forever
- `docs/adr/adr-003-12-gate-admission.md` — admission gates (this crate awaiting)
- `docs/architecture/forward-compat-invariants.md` — `#[non_exhaustive]` ratchet
- `docs/architecture/crate-layer-registry.md` — L0 contract

[nika-spec]: https://github.com/supernovae-st/nika-spec
[miette]: https://docs.rs/miette
