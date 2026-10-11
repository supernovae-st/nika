# Crate spec — `nika-providers-access`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-providers`, an admitted crate · the ADR-122 member note · D-2026-07-09-N1 · 2026-10-10) |
| Layer | L1.5 — beside its unit; lateral L1.5 edge `nika-providers → nika-providers-access`, never back · its own edges go down only: `nika-types` and `nika-catalog`, and `nika-harness` behind `access-harness` |
| Design | how this machine reaches an intelligence, as values: the facts a provider probe states, the access plan resolved over them and the identity of the route a call takes, kept under their historical paths `nika_providers::{probe, resolve_access, route_identity}` and `nika_providers::profile::canonical_provider` |
| IMPL | measured by `scripts/crate-metrics.sh nika-providers-access` at each freeze; the crate carries what `nika-providers` held in `resolve_access.rs`, `route_identity.rs` and the data half of `probe.rs` and `probe/model_list.rs` on 2026-10-10 (the gate's own counter: 2,225 prod LOC at the split · 33 unit tests and the resolve_access property suite, moved with their module) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-providers` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-types` · `nika-catalog` (`providers`, `pricing`) · `nika-harness` (optional, `access-harness`) · `serde`, `serde_json`, `url` · dev: `proptest` |
| Features | `access-harness` (seats judged at their infer grade through `nika-harness` rows; `nika-providers`' own `access-harness` enables it, so the unit keeps one feature) |
| NIKA codes | none minted here — a refused access is a witness (`AccessRejection`, `PinRefusal`) the verbs and the CLI speak with their own codes |

## 1. Purpose

`nika-providers` reached 14,991 of its 15,000 production lines with the readiness of keyed cloud
routes still to land (the authenticated model listing that reads a refused key as « key
refused »). The facts a probe states, the access plan judged over them and the route identity a
call carries form one cluster of pure values and judgments: they name nothing else in the
provider layer, and the provider layer reaches them downward. They move here with git history;
`nika-providers` keeps every path as a re-export, so no consumer changes.

## 2. Surface

- `probe` — `PingState`, `ExecutionLocus` (with `classify`), `ProviderReadiness` (with the
  `key_auth` an authenticated listing learned and `key_refused()`), `ProviderProbe`, `KeyAuth`
  (`Forbidden` for a 403), `classify_key_value`, `classify_http_status`, `ModelListing` (`new`,
  `available`) and the `AccessClass` vocabulary. Probing stays in `nika-providers`:
  key presence, local pings, local liveness and the model-list GET with its parser.
- `resolve_access` — the access plan resolved over probe rows, its candidates, pins, witnesses
  and refusals. `judge`, `pin_matches` and `named_infer_grade_ready` became documented public
  seams: `nika-providers`' model choice and plan read them.
- `route_identity` — the one owner projection of a provider endpoint (`nika/route-identity@1`)
  and the durable projections (`durable_calls`, `durable_pricing`, `project_observation`).
  `checkpoint_text_safe` became a documented public seam, which the admission checkpoint
  reads. `InferenceReceipt::durable_observation` stays with its type in `nika-providers`.
- `canonical_provider` — the catalogue's canonical provider id for an alias.

## 3. Boundary

- No I/O, no runtime, no transport: nothing here dials, reads the environment or spawns.
- Never depends on `nika-providers` (the layer check refuses the edge back); a type the
  provider layer builds and this crate needs moves here, or stays out.
- `ModelListing`, `ProviderReadiness` and `ProviderProbe` are `#[non_exhaustive]`: the provider
  layer builds them through their constructors (INV-019) and sets the public fields.

## 4. Tests

- Here: the resolve_access unit and property suites, moved with their module.
- In `nika-providers`, beside what they need: the two route-identity suites (they build their
  observations with the admission account and the retry route), the probe-to-candidates bridge
  (it reads the registry's probe rows) and `tests/access_reexport.rs`, which compiles against the
  historical paths and proves they name this crate's very items.

## 5. Admission evidence

A member of the admitted `nika-providers` unit: code that moved under its gates, with its tests.
Mutation and property evidence for the moved modules are the unit's; the public API snapshots
are rendered on macOS and the Linux job confirms them.

## 6. Related

- `docs/crate-specs/nika-providers.md` — the unit, and its member note.
- `docs/adr/adr-122-execution-access-plan-authority.md` — the access plan's authority, amended
  with the member note.
- `docs/architecture/crate-layer-registry.md` — the L1.5 row.
