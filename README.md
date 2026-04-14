# Nika 💎 Diamond

The clean-architecture rewrite of [Nika](https://github.com/supernovae-st/nika),
the **"Inference as Code"** workflow engine.

```yaml
schema: "nika/workflow@0.12"
tasks:
  - id: summarize
    infer: "Summarize this article in 3 bullets"
```

## What is this branch ?

`nika-diamond` is an **orphan branch** — no shared history with `main`.
The legacy `main` branch (138,654 LOC engine monolith) stays intact as a
read-only reference while we rebuild Nika as **~40 clean crates**, each
≤15k LOC, passing 12 strict admission gates.

**Why** : every crate fits entirely in an AI context window → zero
hallucination when refactoring → sustainable velocity for 2+ years.

## Current status

```
Phase 0  [SCAFFOLD]     ✅ DONE   workspace + CI + .claude/ rules
Phase 1  [SPLIT CORE]   🔄 NOW   split nika-core (45k) → 6 sub-crates
  └─ nika-error           🔄 IN PROGRESS (Gate 1 SPEC done)
  └─ nika-catalog          ⏳ next
  └─ nika-kernel + mock    ⏳
  └─ nika-schema-ast       ⏳
  └─ nika-schema-analyze   ⏳
  └─ nika-binding          ⏳
Phase 2  [EFFECTS]       ⏸        9 L1 effect crates
Phase 3  [DOMAIN]        ⏸        17 L2 crates (verbs + providers + media)
Phase 4  [RUNTIME]       ⏸        orchestration + interfaces
Phase 5  [PARITY]        ⏸        shadow zones + canary tests
Phase 6  [MERGE]         ⏸        big-bang merge → main + tag v0.90
```

**Timeline** : 11-12 months honest. Quality > speed.

## Architecture target (~40 crates)

```
L5   nika (binary <500 LOC)
L4   cli · lsp · serve · init · lints
L3   runtime · daemon
L2   verb-{exec,fetch,invoke,infer,agent}
     provider-{rig,native,mock}
     builtin · mcp · vault · display
     media-{cas,image,pdf,document,provenance}
     memory (Cortex stub)
L1   shield · event · clock · fs · http · blob · process
     extract · security
L0.5 kernel · kernel-mock
L0   error · catalog · schema-ast · schema-analyze · binding
```

Full details : [`DIAMOND.md`](DIAMOND.md)

## Admission rules

Every crate passes **12 gates** before joining the workspace :

1. SPEC written (`docs/crate-specs/<name>.md`)
2. TDD — tests before implementation
3. IMPL — compiles, tests pass
4. CLIPPY 0 — `cargo clippy -- -D warnings`
5. MUTATION ≥90% — `cargo mutants`
6. PROPERTY — proptest if sensitive
7. BENCHMARKS — if hot path
8. DOCS — `cargo doc` 0 warnings
9. CANARY E2E — workflow test
10. PARITY — golden vs legacy
11. REVIEW SWARM — 3 agents
12. ATOMIC COMMIT

## CI ratchets

9 scripts in `scripts/ci/` enforce diamond rules on every push :

| Script | Rule |
|--------|------|
| `check-loc-limits.sh` | file ≤ 1500 LOC |
| `check-crate-size.sh` | crate ≤ 15k LOC |
| `check-fn-length.sh` | fn ≤ 100 lines |
| `check-unwrap.sh` | 0 `.unwrap()` in src/ |
| `check-expect.sh` | 0 `.expect(` in src/ |
| `check-dead-code.sh` | 0 `#[allow(dead_code)]` |
| `check-clippy.sh` | clippy -D warnings |
| `check-tests.sh` | cargo test --lib |
| `check-no-default-features.sh` | no-default-features compiles |

## Philosophy

This is **not an extraction**. This is **craft**.

Each crate is rewritten from scratch, guided by the legacy code on `main`
(via `git show main:path`) but never copy-pasted. Every `.unwrap()` becomes
`?` propagation. Every file >1500 LOC gets split into modules. Every
function >100 lines gets decomposed.

The user learns Rust in parallel with building. Each crate = a chapter
of their Rust education.

## Post-diamond (v0.95+)

- **nika-memory / Cortex** — persistent agent memory (Oxigraph + FSRS-6 + OWL 2)
- **Agent v2** — parallel tools, ReWOO planning, reflection, resume, compression
- **Nika Cloud** (v0.110+) — hosted runner, Tailscale model (optional, self-host remains primary)
- **Nika Enterprise** (v0.110+) — on-prem + SSO + web UI (demand-driven)

## License

AGPL-3.0-or-later · Commercial CLA (Grafana model)

© 2024-2026 [SuperNovae Studio](https://supernovae.studio)

🦋 Nika — the butterfly on the SuperNovae flag.
