# Crate spec — `nika-cli-host`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-cli` unit · ADR-110 · D-2026-07-09-N1 · 2026-07-31) |
| Layer | L4 — interface member (one unit, two members; the bin + dispatch stay in `nika-cli`) |
| Design | the host-integration plane: probes · wire writers · doctor receipts · vendored client matrix · context envelope · retention · metrics · the output contract |
| IMPL | ~18241 LOC src (2026-09-03 live · `scripts/crate-metrics.sh nika-cli-host`) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal member of the `nika-cli` unit |
| NIKA codes | none minted here — findings ride the surfaces they serve |

---

## 1. Purpose

`nika-cli-host` carries the **integration control plane** of the operator
surface — everything that answers "where are we, what host is present,
what is wired, what is honestly active":

- `probe` — client/kit probes, capability levels, `HostCapabilityReceipt`
- `wire` — the writers (JSON · JSONC · TOML families), preview/detected,
  the clap `WireTarget` seam
- `doctor` — the per-component receipt surface
- `welcome` — the front-door glance + the checkable `SAMPLE`
- `clients_registry` + `data/clients.registry.yaml` — the vendored
  byte-copy of the agents-repo matrix (H6: one truth, machine-checked)
- `context_envelope` — the single workspace resolution (chat_only law)
- `retention` · `metrics` · `text` · `output` (spec §4 exit codes ·
  `VerbOutput` · the OSC-8 link seam)

This is the third 15k-wall descent (display 2026-07-10 · run composer
2026-07-22): compute descends, the operator crate keeps the bin, the
dispatch, and the run/guard authority path.

## 2. Surface law

`nika-cli` re-exports every public item at its historical path
(`verbs::{doctor, probe, welcome, wire}` · `verbs::{VerbOutput, exit}` ·
`metrics` · `verbs::trace::retention` · …). Downstream callers and the
bin never name this crate; its `public-api.txt` is the split's receipt.

## 3. Discipline

- No engine authority here: `guard`, `run`, permits, cost, secrets and
  traces stay where they were — this member reads hosts, it never
  executes workflows.
- The vendored matrix is engine-mirror material: corrections happen in
  the agents repo (`clients.yaml`), then re-vendor byte-exact.
- `local-infer` forwards from `nika-cli` (feature parity across the
  unit's members).
- Tests coupled to cli-side fixtures (retention×trace-store ·
  SAMPLE×check) live cli-side and exercise the re-exported paths.
