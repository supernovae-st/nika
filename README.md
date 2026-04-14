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
The legacy `main` branch (138,724 LOC engine monolith) stays intact as a
read-only reference while we rebuild Nika as **40-42 clean crates**, each
≤15k LOC, passing 12 strict admission gates.

**Why** : every crate fits entirely in an AI context window → zero
hallucination when refactoring → sustainable velocity, forever.

Read the full argument: [`docs/architecture/ai-velocity.md`](docs/architecture/ai-velocity.md).
Read the origin story: [`docs/MANIFESTO.md`](docs/MANIFESTO.md).

## Before Diamond / After Diamond

| Metric | Legacy (v0.79) | Diamond (target v0.90) |
|---|---|---|
| Largest crate | 138,724 LOC | 15,000 LOC (cap) |
| Largest file | 7,243 LOC (`template/mod.rs`) | 1,500 LOC (cap) |
| Unwrap calls in src/ | 1,276 | 0 (CI-enforced) |
| Files >1,500 LOC | 47 | 0 (CI-enforced) |
| AI context fit (per crate) | 75% of 1M | 7% of 1M |
| Error type | 1 god-enum | Per-subsystem `NikaErrorCode` trait |
| Async traits | `async_trait` (boxing) | `trait_variant` (zero cost) |
| Catalog source | Hardcoded Rust arrays | TOML + `build.rs` + `phf_codegen` |

## Current status

```
Phase 0  [SCAFFOLD]     ✅ DONE   workspace + CI + .claude/ rules
L0/L0.5  [SPLIT CORE]    🔄 NOW   splitting legacy nika-core monolith
  ✅ nika-error           admitted v0.80.0-alpha.1 (44 tests, 100% mutation)
  ✅ nika-catalog         admitted v0.80.0-alpha.2+ (154 tests, 42-tag vocab, TOML capabilities, 9 feature subsets)
  ✅ nika-kernel + mock   admitted v0.80.0-alpha.3 (99 + 88 tests)
  ✅ nika-catalog-verify  admitted v0.80.0-alpha.4 (9 tests, online registry verifier)
  🔄 nika-catalog         Phase D — Session 2a ✅ (TOML rules, api_dialect, inv #19 full, Gate 8 green) · Session 2b next
  ⏳ nika-schema          next (AST + DAG + taint, ~13k LOC)
  ⏳ nika-binding         after schema (65 transforms, ~13k LOC)
L1       [EFFECTS]        ⏸        ~11 effect crates
L2       [DOMAIN]         ⏸        ~20 crates (verbs + providers + media + builtins)
L3/L4    [RUNTIME]        ⏸        ~8 crates (runtime, daemon, cli, lsp, serve, init, lints, binary)
         [PARITY]         ⏸        7 shadow zones + canary tests
         [MERGE]          ⏸        cutover → tag v0.90
```

**Timeline** : no deadline. Quality > speed. Forever v0.x model —
[see ROADMAP.md](ROADMAP.md) for the full vision (v0.80 → v0.90 → v0.95 → v0.100 → v0.110+).

## Architecture target (40-42 crates)

```
L5   nika (binary <500 LOC)
L4   cli · lsp · serve · init · lints · pck
L3   runtime · daemon
L2   verb-{exec,fetch,invoke,infer,agent}
     provider-{rig,native,mock}
     builtin · builtin-{github,cloud,workspace} · mcp · display · tool
     media-{cas,image,pdf,document,provenance}
     pck-{manifest,registry,store}
L1   shield · event · clock · fs · http · blob · process
     extract · security · git · vault
L0.5 kernel · kernel-mock
L0   error · catalog · catalog-verify · schema · binding · pck-manifest
```

Full ecosystem map : [`ROADMAP.md`](ROADMAP.md) · legacy migration checklist :
[`docs/migration/legacy-features-checklist.md`](docs/migration/legacy-features-checklist.md).

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
11. REVIEW SWARM — 3 agents parallel
12. ATOMIC COMMIT

## CI ratchets

`scripts/ci/` + `.github/workflows/` + `scripts/hygiene/check-all.sh` (15 vectors)
enforce diamond rules on every push :

| Check | Rule |
|---|---|
| `check-loc-limits.sh` | file ≤ 1,500 LOC |
| `check-crate-size.sh` | crate ≤ 15,000 LOC |
| `check-fn-length.sh` | fn ≤ 100 lines |
| `check-unwrap.sh` | 0 `.unwrap()` in src/ |
| `check-expect.sh` | 0 `.expect(` in src/ |
| `check-dead-code.sh` | 0 `#[allow(dead_code)]` |
| `check-clippy.sh` | clippy -D warnings |
| `check-tests.sh` | cargo test --lib |
| `check-no-default-features.sh` | no-default-features compiles |
| `hygiene/check-all.sh` | 15 drift vectors (MEMORY / ROADMAP / Linear / GitHub sync) |
| `forward-compat.yml` | cargo-public-api + cargo-semver-checks on PR |
| `hygiene-nightly.yml` | nightly drift issue on RED |

## Philosophy

This is **not an extraction**. This is **craft**.

Each crate is rewritten from scratch, guided by the legacy code on `main`
(via `git show main:path`) but never copy-pasted. Every `.unwrap()` becomes
`?` propagation. Every file >1500 LOC gets split into modules. Every
function >100 lines gets decomposed.

The user learns Rust in parallel with building. Each crate = a chapter
of their Rust education.

### Why Rust

Single static binary (no runtime to install). Zero garbage collector — ownership
gives workflow determinism. Strong type system catches integration errors at
compile time, not at 2am. Mature async ecosystem (`tokio`, `rig`, `rmcp`).
Cross-platform via `cargo-dist` with signed releases.

### Why AGPL-3.0

AGPL closes the SaaS loophole that MIT leaves open. If you modify Nika and
run it as a hosted service, users of that service get the source. This is the
right default for infrastructure we want to stay open. For organizations that
can't accept AGPL's network clause, a commercial license is available
(Grafana model). Contact: contact@supernovae.studio.

### Why forever v0.x

We don't ship half-features behind "coming in v2". Every release is diamond-grade
for its declared scope. v1.0 isn't a target; quality is. SQLite stayed on 3.x
for 20 years while growing WAL, FTS, JSON1, window functions. Each 3.x release
was complete AT THAT RELEASE. That's the model.

## Post-diamond (v0.95+)

- **nika-memory / Cortex** — persistent agent memory (Oxigraph + FSRS-6 + OWL 2) — 9-10 crates
- **Agent v2** — parallel tools, ReWOO planning, reflection, resume, compression
- **pck full** — git-repo registry, 8 package types (gold + beta + experimental)
- **WASM plugins** (v0.100) — wasmtime + extism sandbox for third-party builtins
- **Keys subsystem** (v0.100) — Keychain/keyring/OAuth device flow, 10 CLI subcommands
- **Nika Cloud** (v0.110+) — hosted runner, Tailscale model (optional, self-host remains primary)
- **Nika Enterprise** (v0.110+) — on-prem + SSO + web UI (demand-driven)

## License

AGPL-3.0-or-later · Commercial CLA (Grafana model)

© 2024-2026 [SuperNovae Studio](https://supernovae.studio)

🦋 Nika — the butterfly on the SuperNovae flag.
