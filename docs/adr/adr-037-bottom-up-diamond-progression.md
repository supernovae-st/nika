---
id: ADR-037
title: "Bottom-up diamond progression (no feature-milestone defer)"
status: accepted
date: "2026-04-17"
phase: "Phase D — philosophy lock"
deciders: ["@ThibautMelen"]
tags: ["philosophy", "roadmap", "admission", "forever-v0x", "diamond"]
affects_crates: ["all"]
affects_layers: ["L0", "L0.5", "L1", "L2", "L3", "L4", "L5"]
supersedes: []
superseded_by: []
related: ["ADR-001", "ADR-002", "ADR-003", "ADR-004", "ADR-028"]
amends: ["ADR-028"]
requires: []
enables: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "Effective immediately on admission"
follow_ups:
  - "Restructure ROADMAP.md — remove v0.81/v0.90/v0.95/v0.100 section headers"
  - "Re-scope ADR-028 — distinguish API reservation from feature scheduling"
  - "Re-evaluate FCI-007 unstable-cortex — remove or repurpose as per-crate experimental marker"
  - "Update CLAUDE.md narrative to reflect bottom-up progression"
  - "Write dedicated session specs for nika-memory + nika-registry (out of scope here)"
---

# ADR-037: Bottom-up diamond progression

## Status

**Accepted** 2026-04-17 night post-brainstorm (Q-plan 1a/2a/3b/4b/5-both/6a). Amends ADR-028. Cascade: ROADMAP restructure, FCI-007 re-eval, crate count target revised 40-42 → 50-90 (cap 100 unchanged).

## Context

Nika Diamond shipped 6 admitted L0/L0.5 crates + 1 WIP (`nika-schema`) by 2026-04-17. Throughout the first 6 admissions, the ROADMAP carried arbitrary version gates:

- `v0.81` — Foundation
- `v0.90` — Ship (pck + providers)
- `v0.95` — Cortex (9 memory satellites)
- `v0.100` — Plugins (WASM host + sandbox)

These gates encoded a **feature-milestone scheduling** model: ship core, then Cortex later, then plugins later. In practice this produced:

1. **Stubs** that pretend to implement reserved traits (`MemoryStore` in `nika-kernel` with no satellite impl in sight until v0.95).
2. **Version-labelled drift** — docs + specs referenced "target design — arriving at v0.95" as if it were a commitment.
3. **Shadow zones** — the gap between "reserved" and "implemented" accumulated residue (invariants drift, type changes needed later).
4. **Fake ambition** — any mention of "we'll ship X at vN.M" is a promise we cannot keep with forever-v0.x.

User said 2026-04-17 night:
> "c'est une mauvaise chose de le faire en différé. on a pas la bonne logique v0.95 v0.100. je veux vraiment qu'on fasse toutes les features au fur et à mesure en partant du bas du diamant et on remonte."

Translation: **defer = bad logic. Build every feature as its layer is reached, from the bottom of the diamond upward.**

## Decision

Abandon feature-milestone scheduling. Adopt **bottom-up layer progression**:

```
L0    (~9 crates)     foundation types, pure, sync
  ↓
L0.5  (~2 crates)     kernel traits (sealed, ISP, async OK in defs)
  ↓
L1    (~20+ crates)   effect impls — fs, http, process, git, memory-*, provider-*
  ↓
L2    (~10+ crates)   verbs + domain services + memory orchestrator + pck
  ↓
L3    (~5 crates)     runtime + shield + wasm-host + sandbox
  ↓
L4    (~7 crates)     interfaces — cli, lsp, daemon, mcp, sdk, serve
  ↓
L5    (1 crate)       the `nika` binary
```

Every crate admits atomically with 12 gates. Every admission = 1 commit = 1 `v0.80.N` tag increment.

### What changes

1. **ROADMAP.md restructure** — replace `v0.81/v0.90/v0.95/v0.100` section headers with layer-progression sections. No calendar targets. No "this ships at vN.M" language.
2. **ADR-028 re-scope** — distinguish:
   - **API type reservation** (non_exhaustive ratchet, reserved fields in InferRequest/InferResponse, kernel trait surface) — KEEP. Legitimate forward-compat work.
   - **Feature scheduling** (we'll ship Cortex at v0.95) — REMOVE. Bad diamond.
3. **FCI-007 `unstable-cortex` re-evaluate** — if Cortex ships layer-by-layer without defer, we don't need a feature flag gating it. Either drop or repurpose as per-crate experimental opt-in.
4. **Version tagging — Q-plan 3b LOCKED**: `v0.8X.Y` where X climbs per layer-phase completion, Y increments per admission within a phase.
   - `v0.80.N` → current baseline (L0 partial, L0.5 complete)
   - `v0.81` → L0 complete (9 crates admitted)
   - `v0.82` → L1a primitives complete (fs, process, http-client, keys-env)
   - `v0.83` → L1b deps-étage-1 (git, keys-keychain, catalog-sync)
   - `v0.84` → L1c providers (rig + openai-compat + mock)
   - `v0.85` → L1d pck backend (registry + store)
   - `v0.86` → L1e memory foundation (embed, store-fjall, autodesc, hnsw, bm25, rrf)
   - `v0.87` → L1f memory advanced (temporal, fsrs, rdfs-reasoner, graph-algos)
   - `v0.88` → L1g reserved satellites (as demand emerges: causal, episodic, working, meta, procedural, spatial)
   - `v0.89` → L2 complete (verbs, orchestrators, builtins)
   - `v0.90` → L3 complete (runtime, shield, wasm-host, sandbox). **NOT a "ship" tag** per Q-plan 6a.
   - `v0.91` → L4 complete (cli, daemon, serve, mcp, lsp, sdk)
   - `v0.92` → L5 binary first full-stack integration
   - `v0.93+` → polish, optimization, quality iterations
   - Forever-v0.x — no v1.0 target ever.
5. **Total crate count target** — Q-plan 1a LOCKED: 40-42 → **50-90 crates** (cap 100 unchanged). Growth driven by 14-crate memory subsystem, 4 storage backends, ~30 provider crates, own `nika-embed`, WASM host + 3 sandbox crates.
6. **Dedicated deep-dive sessions reserved** — Q-plan 5 LOCKED both:
   - **Session nika-memory** — most-ambitious B1-B5 validated 2026-04-17 night. Design 14 crates (8 core + 6 reserved). Runs FIRST (stabilizes kernel memory traits before L1 memory admission).
   - **Session nika-registry** — backend + publishing policy. Runs after memory.
7. **"v0.90 ship target" DROPPED** — Q-plan 6a. Versions 0.9X are just layer tags, no marketing ceremony. Forever-v0.x.

### What stays the same

- 12-gate admission model (unchanged).
- 28+ invariants (all still binding).
- 7 shadow zones (still pre-launch quality gates).
- `publish = false` on foundation crates (ADR-022 unchanged).
- Forever-v0.x (no v1.0 target).

### Ordering within a layer

Dependencies-first topological order. If two crates have no dep relation, tiebreaker:

1. **User-value-first** (which unlocks a demo or blocks downstream first).
2. **LOC-smallest-first** (easier wins early build momentum).
3. **Effect-axis-smallest-first** (fewer capability axes = lower review burden).

Example L1 candidates in topological order:
- `nika-fs` (L1, reads-fs rw-fs, no deps on other L1)
- `nika-process` (L1, exec-shell, no deps)
- `nika-http-client` (L1, net-egress, no deps)
- `nika-git` (L1, depends on `nika-fs` + `nika-process`)
- `nika-memory-*` (L1, each independent, 8 core + reserved)
- `nika-<provider>-*` (L1, depend on `nika-http-client`)

## Consequences

- ✅ Forever-v0.x integrity — no fake milestones, no version-labelled residue.
- ✅ Every admitted crate is REAL code, not stub. No "v0.95 target design" banners in docs.
- ✅ Linear ROADMAP — users see "what's admitted, what's coming up in this layer, what's above".
- ✅ Tagging discipline — `v0.80.N` monotonic, no backtracking, no retroactive milestone rewrites.
- ❌ Harder to communicate "when does Cortex ship" — answer is "when we reach L1/L2 memory satellites, and never before then".
- ❌ Existing docs (ROADMAP sections, spec drafts, adr-036 MSRV body) that reference `v0.90`/`v0.95`/`v0.100` need cascade edits.
- ⚠️ Some speculative scheduling (nika-sdk crates.io publishing, MSRV cadence) currently pinned to v0.90 — adr-036 must be reworded to say "at whichever tag crosses L4 interface completion" rather than "v0.90".

## Alternatives considered

1. **Keep version-milestone gates but clarify they're goals not deadlines** — fails because docs still communicate false scheduling.
2. **Hybrid: layer-progression as primary, version labels as secondary guidance** — fails because version labels re-encode the deferred-scheduling mindset.
3. **Skip ADR-028 revision, just restructure ROADMAP** — fails because ADR-028 still uses "reservation for v0.95 Cortex" language that contradicts this ADR.

## Cascade plan (post-acceptance)

1. Restructure ROADMAP.md (1 commit).
2. Re-scope ADR-028 (1 commit, Under Revision → Amended).
3. Re-evaluate FCI-007 (1 commit in `forward-compat-invariants.md`).
4. Update CLAUDE.md narrative (1 commit).
5. Reserve dedicated sessions for nika-memory + nika-registry (memory files, no commits).
6. Rework adr-036 MSRV body to remove v0.90 references (1 commit).
7. Constellation reconciliation report: Note Option A/B/C oxigraph rename deferred to nika-memory session (1 commit).

## See also

- ADR-001 — Diamond orphan branch.
- ADR-002 — Forever-v0.x.
- ADR-003 — 12-gate admission.
- ADR-004 — Context-window-sized crates.
- ADR-028 — Forward-compat reservation policy (amended by this ADR).
- `philosophy_bottom_up_diamond_2026_04_17.md` in memory — full brainstorm record.

🦋
