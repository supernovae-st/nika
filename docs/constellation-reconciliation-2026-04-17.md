---
title: "Constellation reconciliation — 2026-04-17"
status: active
phase: "Phase B.5 prep"
authored_by: "rust-architect subagent (Phase B.5 prep, MEGA_HANDOFF_FOUNDATION_LOCK_V081 §reconciliation)"
head: "393fdefa8"
workspace: "v0.80.0"
supersedes: []
follow_up_adrs: ["ADR-022", "ADR-006 amendment", "ADR-004"]
---

# Constellation reconciliation — 2026-04-17

Five open items surfaced during Phase B.5 prep. Each is a localised drift
between the lockstep sources of truth (ADRs 001-015, 021-035) and the
narrative registries (`docs/architecture/crate-layer-registry.md`,
`ROADMAP.md`, `.claude/rules/nika-invariants.md`). This document resolves
each with a concrete delta action + PR sketch so the Phase D envelope
refactor starts from a consistent baseline.

**Sources of truth (priority order)**:

1. ADRs with `status: accepted` in `docs/adr/` (ground truth for
   architectural decisions).
2. `docs/architecture/forward-compat-invariants.md` (LOCKED, FCI-001..FCI-035).
3. `docs/architecture/crate-layer-registry.md` (illustrative map; flagged as
   "planned for v0.81 enforcement"; MUST track ADRs).
4. `ROADMAP.md` (forever-v0.x plan).
5. `.claude/rules/nika-invariants.md` (session-level enforcement).

## Summary

| # | Item | Verdict | Delta action | Follow-up ADR edits |
|---|---|---|---|---|
| 1 | `nika-http` naming | CONSISTENT (L1 `nika-http-client` + L4 `nika-http` lib) | no-op code, one doc clarification | 0 |
| 2 | `nika-runtime` vs `nika-engine` | NO CONFLICT (`nika-engine` is legacy yank-only) | no-op | 0 |
| 3 | `nika-pck-manifest` vs `nika-envelope` | DUPLICATE (ADR-022 merges pck-manifest → envelope) | update 3 files (registry + ROADMAP + invariants) | 0 (ADR-022 already says so) |
| 4 | Kernel 4-way split | STALE in registry (ADR-006 amendment 2026-04-16 rescinded the split) | update registry map + table | 0 (amendment already landed in ADR-006) |
| 5 | Memory subsystem count | DRIFTED (ROADMAP has 7 old names, invariants/registry have 8 correct names) | DEFERRED to dedicated nika-memory session per ADR-037 | 0 (source in private design docs) |

**Total**: 0 ADR edits required. 5 narrative-doc edits. All five items are
drift **away from** ADRs that already ruled — the ADRs win, the registry +
ROADMAP follow.

---

## Item 1 — `nika-http` naming

### Current state

- `docs/architecture/crate-layer-registry.md` (L1, line 85) lists
  `nika-http-client [axes: net-egress]`.
- Same registry (L4, line 110) lists `nika-http (lib)` alongside
  `nika-daemon (lib)` and `nika-mcp (lib)` as interface-surface libraries.
- Neither `nika-http-client` nor `nika-http` appears in the v0.80 workspace
  (`crates/` has 7 crates: types, error, catalog, catalog-verify, kernel,
  kernel-mock, schema).
- ADR-017 (streaming policy) and ADR-019 (retry + timeout ownership) both
  reference `nika-http` as the canonical L1 HTTP transport name — not
  `nika-http-client`. ADR-014 line 29 lists "nika-http" as an L1 effect
  crate. `docs/crate-specs/nika-kernel.md` line 22 also says `nika-http`.

### Expected state

`nika-http-client` is the registry's one outlier name. Every ADR uses
`nika-http`. The task brief states: *"L1 client effect only, server surface
inlined into `nika-serve` L4 binary."* No `nika-serve` crate exists in the
registry today — the L4 line lists `nika-http (lib)` which is ambiguous.

Correct shape, per task brief + ADR naming:

- **L1** `nika-http` — client effect, `rustls` only, net-egress axis.
- **L4** `nika-serve` (or inlined into `nika-cli`) — server binary surface.
  The L4 "nika-http (lib)" entry is the stale residue of a pre-inlining
  plan.

### Delta action

- **Rename** `nika-http-client` → `nika-http` in registry L1 line 85 and
  line 137 table row.
- **Drop** the L4 `nika-http (lib)` mention at line 110 and line 140 table
  row. The server surface lives inside `nika-serve` (future L4 binary) or
  `nika-cli`.
- No ADR rewrites — ADRs 014/017/019 already use the right name.

### PR sketch

Files to edit (one atomic commit, `docs(architecture): reconcile nika-http naming to ADR-014/017/019`):

- `docs/architecture/crate-layer-registry.md` lines 85, 110, 137, 140

```text
- │ nika-http-client          [axes: net-egress]   rustls only, no openssl    │
+ │ nika-http                 [axes: net-egress]   rustls only, no openssl    │

- │ nika-cli (lib)        nika-daemon (lib)    nika-http (lib)                │
+ │ nika-cli (lib)        nika-daemon (lib)    nika-serve (lib)               │
```

---

## Item 2 — `nika-runtime` vs `nika-engine`

### Current state

- `nika-runtime` is the canonical L3 crate name in
  `docs/architecture/crate-layer-registry.md` line 105+139, ADR-012
  (typestate), ADR-013 (loom), ADR-017 (streaming), ADR-019 (retry),
  `ROADMAP.md` line 125+516.
- `nika-engine` appears only in two historical contexts:
  - `docs/architecture/ai-velocity.md` line 8 — describes legacy v0.79's
    138k-LOC monolithic `nika-engine` crate as the cautionary tale
    motivating Diamond's 40-42 crate split.
  - `docs/adr/adr-022-foundation-crate-layout-v081.md` follow-ups +
    `docs/adr/adr-025-per-crate-semver-release-plz.md` line 68 — both
    about yanking legacy `nika-engine@0.47.1` from crates.io.

### Expected state

No conflict exists. `nika-engine` is not a current or planned crate. It is
a legacy name slated for crates.io yank (per ADR-022 follow-up). `nika-runtime`
is the only L3 name.

### Delta action

**No-op.** The two names cohabit correctly — `nika-engine` as legacy
reference, `nika-runtime` as planned L3 crate. No doc contradicts this.

### PR sketch

None.

---

## Item 3 — `nika-pck-manifest` vs `nika-envelope`

### Current state

- ADR-022 (Accepted 2026-04-16) line 83-84 **explicitly merges**
  `nika-pck-manifest` into `nika-envelope`:
  > "`nika-pck-manifest` MERGES into `nika-envelope` — A pck manifest IS
  > an envelope document (`Envelope<PckSpec>`). Eliminates 1 crate,
  > conceptually cleaner."
- ADR-022 line 45 positions `nika-envelope` at L0-tier-1 with bracket
  `[NEW — apiVersion+Kind+Metadata+multi-doc+PckSpec]`.
- **Stale references** still pointing at a free-standing `nika-pck-manifest`:
  - `docs/architecture/crate-layer-registry.md` line 60 lists it as an L0
    crate: `│ nika-pck-manifest         [axes: none]      Package manifest TOML types  │`
  - Same registry line 135 table row.
  - `ROADMAP.md` line 468: `- nika-pck-manifest (L0, ~1.5k LOC) — TOML types + validation`.
  - `.claude/rules/nika-invariants.md` "Added in POST_AUDIT 2026-04-14 expansion" section line 70: `- nika-pck-manifest — L0, manifest types (~1.5k LOC)`.
  - `docs/adr/adr-028-forward-compat-reservation-policy.md` and
    `docs/adr/index.json` / `index.toml` list `nika-pck-manifest` in
    `affects_crates` — these are historical and OK to leave.

### Expected state

`nika-envelope` is the single L0-tier-1 crate. No free-standing
`nika-pck-manifest` anywhere. The `PckSpec` type lives inside
`nika-envelope` as a variant of `Envelope<T>`.

### Delta action

Update 3 narrative docs (NOT the frozen ADR index files). Do **not**
rewrite ADR-022 — it is already correct and Accepted.

### PR sketch

One atomic commit: `docs(architecture): remove nika-pck-manifest stragglers (merged into nika-envelope per ADR-022)`.

- `docs/architecture/crate-layer-registry.md` — drop line 60 bullet +
  adjust L0 crate count from "9 crates" to "8 crates" (line 45). Update
  table row at line 135 to remove `nika-pck-manifest`.
- `ROADMAP.md` line 468 — replace `nika-pck-manifest` bullet with a
  cross-reference to `nika-envelope`.
- `.claude/rules/nika-invariants.md` line 70 — delete the
  `nika-pck-manifest — L0, manifest types` bullet; note in the pck section
  that manifest types live in `nika-envelope`.

ADR-022 is the citation — nothing to edit inside ADR-022.

---

## Item 4 — Kernel 4-way split

### Current state

- **ADR-006 "Amendment 2026-04-16 — Kernel stays monolithic forever"**
  (lines 106-132 of `docs/adr/adr-006-layered-kernel-isp-traits.md`)
  **rescinds** the split trigger and frees the reserved crate names
  `nika-kernel-core`, `nika-kernel-ai`, `nika-kernel-runtime`,
  `nika-kernel-plugin`. Quote: *"nika-kernel stays as ONE crate forever,
  with 5 internal group modules (io/, ai/, runtime/, plugin/, infra/) that
  already provide the logical separation."*
- ADR-022 line 59 echoes this: `nika-kernel [40 traits, NO split forever per ADR-006-amend]`.
- **Stale references** still describing the 4-way split:
  - `docs/architecture/crate-layer-registry.md` lines 76-80 — five-line
    sub-block describing the split crate names.
  - Same registry lines 226-234 "## Reserved kernel split (threshold-gated)"
    — entire section is superseded.
  - Kernel module header comments at `crates/nika-kernel/src/ai/mod.rs:6`,
    `crates/nika-kernel/src/plugin/mod.rs:6`,
    `crates/nika-kernel/src/runtime/mod.rs:6` say `Future sub-crate:
    nika-kernel-* (when kernel exceeds 10k LOC or 50 traits)`. Task brief
    forbids `src/` edits — these are **not** touched in this PR; flag as
    a Phase D follow-up.

### Expected state

The registry presents `nika-kernel` as monolithic forever, with the 5
internal module groups as the logical split. The "Reserved kernel split"
section is removed (or reduced to a one-line pointer to ADR-006
amendment).

### Delta action

Update `docs/architecture/crate-layer-registry.md` only. Module doc
comments inside `crates/nika-kernel/src/` are left for Phase D (task
brief forbids `src/` edits in this reconciliation PR).

### PR sketch

One atomic commit: `docs(architecture): collapse kernel 4-way split per ADR-006 amendment`.

- `docs/architecture/crate-layer-registry.md` lines 76-80 — delete the
  5-line split preview block.
- Same registry lines 226-234 — replace "## Reserved kernel split
  (threshold-gated)" with a 2-line pointer to ADR-006 amendment.

```text
- ## Reserved kernel split (threshold-gated)
-
- `nika-kernel` currently ships as one L0.5 crate (4,868 LOC, 40 traits).
- Per ADR-006, it splits when either total LOC exceeds 10k OR pub trait
- count exceeds 50. Currently well below both thresholds.
-
- The split is documented in the L0 architecture map above. The split is
- additive — downstream imports change from `nika_kernel::X` to
- `nika_kernel_core::X` behind a facade re-export in `nika-kernel`.
+ ## Kernel monolithic forever
+
+ Per ADR-006 amendment (2026-04-16), `nika-kernel` stays monolithic; the
+ previous 4-way split trigger is rescinded. Internal grouping happens via
+ `src/{io,ai,runtime,plugin,infra}/` modules, not sibling crates.
```

Phase D follow-up (separate PR, unlocks `src/` edits): drop the "Future
sub-crate:" comments in the 3 kernel mod.rs files.

---

## Item 5 — Memory subsystem 8-vs-7

### Current state

Three numbers circulating:

- **8 satellites** — `.claude/rules/nika-invariants.md` line 5:
  *"9 memory crates at v0.95 Cortex (1 L2 orchestrator `nika-memory` + 8
  L1 satellites: hnsw, bm25, rrf, fsrs, rdfs-reasoner, temporal,
  graph-algos, autodesc)"*. Matches `docs/architecture/crate-layer-registry.md`
  line 145-147 (*"1 L2 orchestrator (`nika-memory`) + 8 L1 satellites"*).
- **9-10 crates** — `ROADMAP.md` line 532 §v0.95 Cortex heading: *"Cortex
  (9-10 new crates, ~30-40k LOC)"*. Line 613: *"Cortex (memory satellites
  × 9-10)"*.
- **7 crates with stale names** — `ROADMAP.md` lines 534-540 lists
  `nika-memory-{core, oxigraph, fsrs, owl2, embed, retrieval, reasoning}`.
  None of these names match the 8 satellites in invariants
  (hnsw, bm25, rrf, fsrs, rdfs-reasoner, temporal, graph-algos, autodesc).
  `fsrs` overlaps in both lists; everything else is different.

Ground truth: memory design ADR (authored in private design docs outside
this public submodule) lists **9 crates** (1 orchestrator + 8 satellites).
This matches `.claude/rules/nika-invariants.md` + `crate-layer-registry.md`.

### Expected state

`ROADMAP.md` §v0.95 Cortex lists:

- `nika-memory` (L2 orchestrator)
- `nika-memory-hnsw`, `nika-memory-bm25`, `nika-memory-rrf`,
  `nika-memory-fsrs`, `nika-memory-rdfs-reasoner`, `nika-memory-temporal`,
  `nika-memory-graph-algos`, `nika-memory-autodesc` (8 L1 satellites)

Total = 9 crates. The "9-10 new crates" ambiguity resolves to 9 once
`owl2`/`reasoning`/`embed`/`retrieval` are reconciled:

- `owl2` is subsumed by `rdfs-reasoner` (RDFS is the lighter sibling of
  OWL2; canonical choice per ADR-004).
- `reasoning` folds into `autodesc` + `rdfs-reasoner`.
- `embed` is now an `EmbeddingProvider` trait in the kernel (ADR-029
  `EmbeddingSpec`), not a satellite crate.
- `retrieval` folds into the orchestrator `nika-memory` + `rrf` satellite.

### Delta action

Update `ROADMAP.md` §v0.95 Cortex (lines 528-547) to match the 8-satellite
list. Keep the `~30-40k LOC` estimate. Note that `nika-memory-oxigraph`
(L1, `[axes: rw-fs]`) already appears in the L1 section of the registry
at line 91 as the RDF triple store — keep it as part of the `rdfs-reasoner`
satellite's implementation, not a separate crate. Either:

- **Option A (preferred)**: rename the registry L1 entry from
  `nika-memory-oxigraph` to `nika-memory-rdfs-reasoner` so the 9-crate
  invariant matches exactly. Oxigraph is the impl detail, not the public
  name.
- **Option B**: keep `nika-memory-oxigraph` in the registry and treat
  `rdfs-reasoner` in invariants as a role alias. Leaves two names per
  crate — worse.

**Option A is correct.**

### PR sketch

One atomic commit: `docs(roadmap): reconcile v0.95 Cortex memory subsystem to 9 crates (ADR-004)`.

- `ROADMAP.md` lines 532-540 — replace 7 legacy names with canonical 8
  satellites + 1 orchestrator.
- `docs/architecture/crate-layer-registry.md` line 91 — rename the L1
  entry `nika-memory-oxigraph` → `nika-memory-rdfs-reasoner` (keep the
  `[axes: rw-fs]` axis and "Cortex impl v0.95" note).
- `docs/architecture/forward-compat-invariants.md` line 178 — the
  `unstable-cortex = ["dep:nika-memory-oxigraph"]` feature flag inside
  FCI-007 references the now-obsolete name. Task brief forbids editing
  `forward-compat-invariants.md`. **Flag for a follow-up ADR** or amend
  the renaming as a separate Phase C ADR. (Do not edit now.)

**Blocker for full reconciliation**: the forward-compat-invariants.md
reference at line 178 cannot be touched per task constraint. Either:

- Phase C ADR adds an addendum to FCI-007 naming the satellites and
  de-references `nika-memory-oxigraph` to `nika-memory-rdfs-reasoner`.
- Or the v0.95 Cortex PR does the rename+invariants amendment in one
  approved atomic commit.

Recommendation: schedule the FCI-007 amendment for Phase C ADR sweep and
leave the invariants file untouched in this reconciliation PR.

---

## Verification checklist (post-PR)

After merging the 3 atomic commits above:

- [ ] `docs/architecture/crate-layer-registry.md` L0 block lists 8 crates
      (pck-manifest dropped, kernel-split block gone).
- [ ] Registry L1 block lists `nika-http` (not `-client`) and
      `nika-memory-rdfs-reasoner` (not `-oxigraph`).
- [ ] Registry L4 list does not include `nika-http (lib)`.
- [ ] `ROADMAP.md` §v0.95 Cortex lists 9 crates with canonical satellite
      names.
- [ ] `.claude/rules/nika-invariants.md` no longer lists
      `nika-pck-manifest` as a separate crate.
- [ ] No ADR file is rewritten (ADRs 001-015, 021-035 unchanged).
- [ ] `forward-compat-invariants.md` unchanged (canonical).
- [ ] No `crates/nika-*/src/` edits.

## Phase D handoff notes

- The `crates/nika-kernel/src/{ai,plugin,runtime}/mod.rs` "Future sub-crate"
  comments must be deleted in Phase D when `src/` edits unlock. Tracked
  here for visibility; not part of this reconciliation PR.
- The registry v0.95 naming reconciliation (`-oxigraph` → `-rdfs-reasoner`)
  ripples into `FCI-007` feature-flag list at
  `docs/architecture/forward-compat-invariants.md:178`. Schedule a Phase C
  ADR amendment, do not touch the canonical file ad hoc.

🦋
