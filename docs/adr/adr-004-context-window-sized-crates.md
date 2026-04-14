# ADR-004: Context-window-sized crate architecture

**Status:** Accepted
**Date:** 2026-04-13 (updated 2026-04-14 with expansion to 40–42)
**Phase:** Phase 0 (sizing)
**Deciders:** @ThibautMelen

## Context

The fundamental premise of the Diamond rewrite (ADR-001) is that each crate must fit in an AI assistant's context window to prevent hallucinated refactors. The math: 15k LOC ≈ 70k tokens ≈ 7% of a 1M-token context window, leaving 93% for tests, kernel traits, and working space. Legacy `nika-core` was a 138k-LOC monolith — effectively invisible to any AI reviewer.

Post-audit (2026-04-13) counted crates by natural boundary using the "≥2 distinct consumers" rule. The initial draft (32–34 crates at v0.90) was expanded on 2026-04-14 after the pck + native-adapter brainstorm to **40–42 crates at v0.90**.

## Decision

Hard caps enforced by CI (see ADR-009):

- **Per-crate LOC cap:** 15,000 lines
- **Per-file LOC cap:** 1,500 lines
- **Per-function line cap:** 100 lines
- **Total crate count cap:** 100 (hard limit; admissions past this require gate-review)

Crates with fewer than 2 distinct consumers are merged (examples: `nika-macros` inlined into `nika-event`; `nika-lsp-core` merged into `nika-lsp`; `nika-policy` is a module of `nika-runtime`, not a crate).

Heavy-dep isolation justifies aggressive splits: `nika-provider` becomes 3 crates (`nika-provider-rig`, `nika-provider-native`, `nika-provider-mock`) because `mistral.rs` CUDA/Metal features are incompatible with `rig-core`'s cloud dep graph. `nika-media` becomes 5 crates (pdfium, c2pa, resvg, image, CAS) to isolate C-FFI and large binary deps.

Phased target table:

| Phase | Target count | Running total |
|---|---|---|
| v0.80.x (current) | 5 | 5 |
| v0.90 | 32–34 core + 8 new (pck + native adapters) | **40–42** |
| v0.95 | +14–15 (Cortex + media + natives) | ~63–65 |
| v0.100 | +4–6 (observability + LSP + WASM) | ~72–75 |
| v0.110+ | incremental | cap **100** |

## Consequences

### Positive
- An AI reviewer working on `nika-catalog` (4,690 LOC) holds the entire crate + tests + kernel traits simultaneously. No blind spots.
- 12-gate protocol (ADR-003) is feasible at this scale; it would be impractical on 138k-LOC crates.
- Dep isolation from provider/media splits reduces binary size and compile time for downstream consumers (CLI users don't pull PDFium unless they opt in).

### Negative
- 40-42 crates creates substantial `Cargo.toml` ceremony and cross-crate import discipline overhead.
- The merge/split decisions require ongoing gate-review (≥2-consumers rule applied but judgment-loaded on new crates).
- Admission overhead at 40+ crates is a major driver of the 11–12 month v0.90 timeline.

### Neutral
- Workspace-level dependency deduplication (one pin per external crate in `[workspace.dependencies]`) mitigates the compile-time concern.

## Evidence

- `Cargo.toml` lines 1–17 — 5 admitted members, 32 excluded
- `ROADMAP.md` lines 17–35 — "Why Diamond" context-window math
- `scripts/ci/check-crate-size.sh` — enforces 15k LOC cap per crate
- `scripts/ci/check-loc-limits.sh` — enforces 1,500 LOC cap per file
- `scripts/ci/check-fn-length.sh` — enforces 100-line cap per function
- memory: `POST_AUDIT_REVISIONS.md` lines 18–64 — phased crate count table, merge/split decisions
- memory: `RUST_ENFORCEMENT.md` — `check_layers.rs` pattern for layer-level ordering enforcement (see ADR-006)

## Alternatives considered

### Alt A — 20–25 crates, looser caps (40k LOC per crate)
Closer to Nushell (43 crates) or DataFusion (47). Rejected because 40k LOC = 180k tokens = 18% of context, cutting reasoning headroom in half. Shadow zones found historically in files >1,500 LOC (legacy `resolve.rs` at 2,945 LOC); enforcing the file cap is empirically valuable.

### Alt B — 10–12 mega-crates, no caps
Rejected immediately — recreates the monolith problem.

### Alt C — Unbounded crate count, organic splits
Rejected because 100+ crates would violate the "≥2 consumers" rule and produce coordination overhead.

## Related

- ADR-001 — orphan rewrite (the thesis that produced this sizing)
- ADR-003 — 12-gate admission (the process the sizing enables)
- ADR-006 — layered kernel + ISP traits (why even 15k-LOC crates have clear internal structure)

## Notes

Revisit the 100-crate cap after v0.100. If the ecosystem grows beyond what 100 crates can express, consider spinning satellite workspaces rather than expanding the cap — 100 is a humility check, not a hard engineering limit.
