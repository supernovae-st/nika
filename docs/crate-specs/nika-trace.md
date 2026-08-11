# Crate spec — `nika-trace` (descended member)

| | |
|---|---|
| Status | **DESCENDED 2026-08-11** from `nika-cli` — NOT a fresh admission: a size-cap member split of an already-admitted unit per D-2026-07-09-N1 (one architectural unit · two workspace members · the ADR-110 `nika-cli-host` precedent). `nika-cli` measured 15,040 prod LOC at the vector-24 gate (cap 15,000); the trace-reading plane was the clean seam (every consumer reaches it through `verbs::trace*`, and the compute half — chain walk · anchor wire · recover · store scan — had already descended to `nika-dap` 2026-07-09). |
| Layer | **L4** — the operator surface's read half: renders + routes, every effect already below. |
| Design | The **flight-recorder reader** — every surface that READS `.nika/traces/` (the NDJSON journals a run records): `trace show\|replay\|outputs\|peek\|flow` (the fold's render), `trace ls\|rm` (store management · ADR-100), `trace verify` (tamper-evidence chain · minisign signature · anchor tiers), `trace anchor` (Rekor v2 · RFC 3161 notary), `trace reproduce`, `trace export` (OTel), the `evidence` pack, the `receipt` explainer, the learned-truth `forecast` behind `explain --forecast`, and the bin's `trace` dispatch arm (`dispatch.rs` — the replay loop + door routing descended verbatim from `main.rs`, which keeps a one-line arm). `nika-cli` re-exports every public item at its historical `verbs::` path — call sites, the 32 integration suites and the clap tree read unchanged. |
| Name | `nika-trace` — the plane it reads, named after the verb it serves. Descent precedent: `nika-cli-host` (2026-07-31). |
| LOC | ~6087 LOC src (`scripts/crate-metrics.sh --loc nika-trace` · ±15% band per vector 6) — ≈3.3k of it prod (the counter's cfg(test) scope); the descent lifted `nika-cli` from 15,040 to 11,851 prod. |
| Deps | `nika-dap` (the forensics compute), `nika-cli-host` (VerbOutput/exit · retention config), `nika-display` (Theme · RunView · frame), `nika-event`, `nika-types`, `clap`, `serde`, `serde_json`. dev: `uuid`. |
| Publish | `false` — internal member of the `nika-cli` unit (the binary ships, the lib doesn't). |

## 1 · Why this crate exists

Two reasons, one mechanism (the same two as every descent):

1. **The cap held.** `≤15k prod LOC/crate` is a hard law; the trust-experience
   arc's trace surface (verify tiers · anchor · evidence · forecast) tipped
   `nika-cli` over it. The sanctioned move is a member split, not an
   exemption — the unit (the operator surface) stays ONE architectural
   unit; the workspace gains one member.
2. **The seam was real.** The plane is read-only over the journals, its
   compute had already descended (`nika-dap` hosts chain/anchor/recover/
   store/stats/seal), and its only in-workspace consumer is `nika-cli`
   itself (re-export) — plus `explain --forecast`, which reads the
   descended `forecast` module through the same re-export. Nothing else in
   the workspace imports these paths.

## 2 · Known residue (owned)

- **The staged-journal test fixtures are duplicated cli-side.** The
  `#[cfg(test)]` fixture sets (`trace::store::tests` · `forecast::gather::
  tests`) cannot cross a crate boundary, and `explain_file`'s staged-history
  integration tests stay with their subject in `nika-cli`. They carry a
  local twin of the six fixture fns (`temp_store` · `stage_trace` · `ev` ·
  `run_body` · `task_done` · `done`) — the established per-crate fixture
  pattern (`nika-dap`'s store tests already twin the same helpers). If a
  third consumer ever needs them, the honest home is a `testkit` seam, not
  a third copy.
- **The `store`/`retention` shims narrowed, deliberately.** They were
  `pub(crate)` inside `nika-cli`; rather than widen them to `pub` through
  the re-export, the two remaining cli-side consumers (`run`'s start-GC ·
  `explain_file`'s census) now read the descended homes directly
  (`nika_dap::store` · `nika_cli_host::retention`). `nika-cli`'s public
  surface did not gain an item.
