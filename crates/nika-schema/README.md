# nika-schema

Workflow AST + parser — THE PARSER (its blueprint shape) — pure, zero I/O, zero async.

> **Status · ADMITTED** (2026-06-18 · all 12 gates · the last L0 crate).
> Parser-only since 2026-07-21: the analyzer and the ADR-092 `nika check`
> static ladder descended to **`nika-check`** at the 15k crate-size wall
> (the nika-graph/nika-dap precedents). Gate 5 closed in BUDGET mode
> (survivors ≤ 300); see the crate spec for the full gate table. Sibling
> crates depend on it freely.

Layer **L0**. Parses the canonical nine-key envelope (`nika: <id>` ·
`model` · `inputs` · `const` · `secrets` · `permits` · `run` · `tasks` ·
`outputs` · per [nika-spec]) into a typed AST and surfaces
diagnostics via [miette] so the CLI can render rich error spans. The
parser is a two-stage pipeline : `marked-yaml` (source-span preserving) →
`serde-saphyr` (typed deserialize). The analyzer (DAG checks · the edge
derivation) and the `check` ladder live in `nika-check` (L0 → this crate).

## Usage

```rust
use nika_schema::{FileId, ParseMode, RawWorkflow, SchemaError};

let yaml = std::fs::read_to_string("hello.nika.yaml")?;
let workflow: RawWorkflow = nika_schema::parse(&yaml, FileId::new(0), ParseMode::Strict)?;

// Inspect the DAG · the edges are declared, never restated:
// `with:` bindings are the data edges, `after:` the control edges.
for task in &workflow.tasks {
    println!("{} · after {:?} · with {:?}", task.id.value, task.after, task.with);
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

- `docs/adr/adr-021-yaml-envelope-convention.md` (superseded) · `docs/adr/adr-113-envelope-identity-on-nika.md` + `nika-spec` spec/01-envelope.md — the nine-key envelope · the identity rides on `nika:`
- `docs/adr/adr-003-12-gate-admission.md` — admission gates (this crate awaiting)
- `docs/architecture/forward-compat-invariants.md` — `#[non_exhaustive]` ratchet
- `docs/architecture/crate-layer-registry.md` — L0 contract

[nika-spec]: https://github.com/supernovae-st/nika-spec
[miette]: https://docs.rs/miette
