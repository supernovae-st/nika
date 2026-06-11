# Crate specs — the Gate-1 artifacts

One file per crate, named `nika-<crate>.md`. A spec is **Gate 1 of the
12-gate admission** (ADR-003): it exists *before* the implementation and
records the contract the crate is admitted against — purpose, layer, public
API shape, security posture, test strategy, and the gate table once
admission completes.

## Status row semantics

The `| Status |` row in each spec's header table is the spec's lifecycle
marker, in order of progression:

| Marker | Meaning |
|---|---|
| `SPEC` / `proposal` (Gate 1) | contract authored · implementation not started |
| `DESIGN LOCKED` | architecture decided (forks resolved) · impl sequenced/gated |
| `WIP` / `PROPOSED` | implementation in flight · pre-gate-pass |
| `ADMITTED YYYY-MM-DD (sha)` | all 12 gates passed · the SHA is the admission commit |

When a crate is admitted, flip the row to `ADMITTED` with the date and
commit — the prior status is kept inline (`· was …`) as the audit trail.

## What specs do NOT carry

Per the projection-derive discipline, specs never hand-maintain live
numbers that drift:

- **Live LOC / test counts** → `scripts/crate-metrics.sh <crate>`
  (vector 6 flags any spec LOC anchor that drifts >15% from reality)
- **Workspace-wide counts** (crates admitted · lib tests · clippy) →
  the auto-generated block in `.claude/CLAUDE.md` via
  `scripts/refresh-status.sh` (vector 23 parity-enforced)
- **Admitted-vs-WIP split** → `[workspace.metadata.diamond]` in the
  root `Cargo.toml` (the single source; `scripts/crate-metrics.sh`
  projects it)

## Related

- `docs/adr/adr-003-12-gate-admission.md` — the full gate spec
- `docs/architecture/crate-layer-registry.md` — L0→L5 layer discipline
- `docs/architecture/forward-compat-invariants.md` — Gate-12 checklist
- `ROADMAP.md` — the admission ladder and slice ordering
