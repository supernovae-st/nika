# Crate spec — `nika-cli-host-typesafe`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-cli-host`, itself a member of the admitted `nika-cli` unit · the ADR-110 amendment · D-2026-07-09-N1 · 2026-10-11) |
| Layer | L4 — beside its unit; lateral L4 edge `nika-cli-host → nika-cli-host-typesafe`, never back · its own edges: `nika-onboard` (L4, lateral), `nika-providers` (L1.5), `nika-http` (L1), `nika-kernel` (L0.5) |
| Design | the `TypeSafe` System One decision seat: one unary `POST /v1/systemone` per question or per batch request, the Session's journaling seat and the batch exchange, kept under its historical path `nika_cli_host::compile::typesafe` |
| IMPL | measured by `scripts/crate-metrics.sh nika-cli-host-typesafe` at each freeze; the crate carries what `nika-cli-host` held in `compile/typesafe.rs` and `compile/typesafe/` on 2026-10-11 (the gate's own counter: 935 prod LOC at the split · 48 tests, moved with their modules) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-cli` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-http` · `nika-kernel` · `nika-onboard` · `nika-providers` · `bytes`, `serde_json`, `url` · dev: `sha2`, `tokio` |
| NIKA codes | none minted here — a refused question is a `DecisionError` the compiler and the Session speak with their own codes |

## 1. Purpose

`nika-cli-host` reached 14,995 of its 15,000 production lines with the decision seat's next
changes still to land: the settled record's probabilities and wire order, the ordered criteria
serializer (B9), the writer's hooks and the reasoning-judge route beside the seat. The seat is
cohesive (its one in-crate caller, `compile/authoring.rs`, reaches it through
`TypesafeSeat::{from_env, key}`), so it moved here with git history and keeps its own budget.
`nika-cli-host` re-exports the crate whole as `compile::typesafe`: no consumer changes.

## 2. Surface

- `TypesafeSeat` (`new`, `with_base`, `from_env`, `model`, `key`, the exchanges), `seat`,
  `Delivery`, `TIMEOUT` and `Usage` (re-exported from `system_one`).
- `session` — the operator-selected seat with its observation journal (`DecisionSetup`,
  `DECISION_SCHEMA` and the records it writes).
- `batch` — a batch of independent questions, one request or its halves after a capacity
  refusal, under what the seat learned of the service's capacity.
- `key` became a documented public seam returning `&Secret` (redacted from `Debug`, zeroized on
  drop): `nika-cli-host`'s private capture policy exposes it only to withhold the key from
  captured Text. The seat still has no `Debug`, and nothing here prints the key.

## 3. Boundary

- One transport: the seat's own single-attempt `nika-http` client, no SDK retry loop, no
  fallback; the key rides only in the Authorization header.
- Never depends on `nika-cli-host` or `nika-cli` (the layer check refuses the edge back).

## 4. Tests

The seat's suites moved with their modules: the transport (`wire_tests`), its regressions, the
Session seat's batches, the batch exchange and its loopback live tests (48 in all).

## 5. Admission evidence

A member of the admitted `nika-cli` unit: code that moved under its gates, with its tests. The
public API snapshots are rendered on macOS and the Linux job confirms them.

## 6. Related

- `docs/crate-specs/nika-cli-host.md` — the member it descends from, and its pointer.
- `docs/adr/adr-110-nika-cli-host-member-split.md` — the unit's split, amended with this member.
- `docs/architecture/crate-layer-registry.md` — the L4 row.
