# Mega Audit Master Plan — v0.68.0 Stabilization

**Date**: 2026-04-04
**Source**: 7-agent parallel audit (Rust Architect, Rust Pro, Rust Security, Rust Async Expert, Code Explorer x2, DX Explorer)
**Scope**: 392K LOC, 18 crates, 9,800+ tests

---

## Audit Results Summary

| Agent | Focus | Key Findings | Rating |
|-------|-------|-------------|--------|
| Rust Architect | Crate topology, boundaries, traits | runner.rs monolith, NikaError 103 variants, 46 pub re-exports | GOOD |
| Rust Pro | Idioms, patterns, dead code | 119 dead_code, 335 is_empty assertions, blocking I/O | GOOD |
| Code Explorer | Daemon/Vault/Secrets flow | UB in set_var, NikaConfig dead letter, vault path duplication | SOLID |
| Crate Explorer | Workspace dependencies | 18 crates, clean DAG, async-stream inconsistency | EXCELLENT |
| DX Explorer | Docs, config, tests | version drift, tool count wrong, schema consistent | 7.5/10 |
| Rust Security | Injection, SSRF, crypto, path traversal | 1 CRITICAL + 4 HIGH + 4 MEDIUM | STRONG |
| Rust Async Expert | Cancellation, blocking, streaming | 2 MEDIUM + 3 LOW, architecture textbook | EXCELLENT |

---

## All Findings (Priority-Sorted)

### CRITICAL (1)

| ID | Finding | Plan | Risk |
|----|---------|------|------|
| C-1 | `unsafe set_var()` UB in nika serve embedded | Plan 1 | Data corruption, secret leak |

### HIGH (6)

| ID | Finding | Plan | Risk |
|----|---------|------|------|
| H-1 | YAML anchor bomb on skill/agent files | Plan 1 | OOM/DoS |
| H-2 | Agent FetchTool SSRF bypass on redirects | Plan 1 | Metadata exfiltration |
| H-3 | Symlink escape in artifact path validation | Plan 1 | Arbitrary file write |
| H-4 | Vault KDF too weak (6 iter, 64KB) | Plan 1 | Offline brute-force |
| H-5 | runner.rs 8,252 lines, 3,274-line impl | Plan 2 | Maintainability crisis |
| H-6 | NikaError 103 flat variants, triple-dispatch | Plan 2 | Extension friction |

### MEDIUM (12)

| ID | Finding | Plan |
|----|---------|------|
| M-1 | Blocking I/O in async (inject, artifacts, trace) | Plan 3 (QA-1) |
| M-2 | 119 `#[allow(dead_code)]` annotations | Plan 3 (QA-4) |
| M-3 | `working_dir_mode: Option<String>` | Plan 3 (QA-2) |
| M-4 | 335 `assert!(!is_empty())` anti-patterns | Plan 3 (QA-5) |
| M-5 | PATH not blocked in exec env | Plan 1 |
| M-6 | Daemon ListSecrets unauthenticated | Plan 1 |
| M-7 | Vault plaintext not zeroized | Plan 1 |
| M-8 | nika-engine monolith 168K LOC | Plan 2 (ARCH-3) |
| M-9 | NikaConfig stores API keys plaintext | Plan 2 (ARCH-7) |
| M-10 | nika-media → nika-mcp inversion | Plan 2 (ARCH-9) |
| M-11 | Dual LSP implementations (24K LOC) | Plan 2 (ARCH-4) |
| M-12 | RunContext 50 pub methods god object | Plan 2 (ARCH-5) |

### QUICK WINS (10)

| # | Action | Plan | Time |
|---|--------|------|------|
| 1 | project-info.json v0.52.0 → v0.68.0 | Plan 4 | 5m |
| 2 | README footer v0.65.1/17 crates → v0.68.0/18 | Plan 4 | 5m |
| 3 | Tool count 45+ → 61 | Plan 4 | 10m |
| 4 | pub → pub(crate) on 46 runtime re-exports | Plan 2 | 30m |
| 5 | FxHashMap in dag/flow.rs | Plan 3 | 15m |
| 6 | serde_saphyr alias consolidation | Plan 3 | 10m |
| 7 | async-stream/futures workspace refs | Plan 4 | 10m |
| 8 | format!().as_str() cleanup | Plan 3 | 15m |
| 9 | serde-saphyr version unification | Plan 4 | 10m |
| 10 | WorkingDirMode enum | Plan 3 | 30m |

### STRENGTHS (Preserve)

- Async architecture: dual semaphore, CancellationToken, biased select!
- Exec security: NFKC normalization, shell transform enforcement
- SSRF main path: DNS pinning, per-hop redirect check
- Memory perf: Arc<str> interning, Cow<str> templates, SmallVec, DashMap
- Workspace topology: clean DAG, feature forwarding
- Schema consistency: 744 workflows on @0.12
- Secret redaction: comprehensive regex + value-based

---

## Execution Timeline

```
┌─────────────────────────────────────────────────────────────┐
│                    PRE-LAUNCH (Now → May 5)                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Week 1: Security Sprint                                    │
│  ├── C-1  SecretStore (DashMap) .............. 2h  [CRITICAL]│
│  ├── H-2  SSRF-safe FetchTool ............... 1h  [HIGH]    │
│  ├── M-5  Block PATH env .................... 15m [MEDIUM]  │
│  ├── H-1  YAML size limit ................... 30m [HIGH]    │
│  ├── H-3  Symlink artifact check ............ 1h  [HIGH]    │
│  ├── Quick Wins 1-3 (version sync) .......... 20m           │
│  └── Quick Wins 5-10 (code quality) ......... 1.5h          │
│                                                             │
│  Week 2: Architecture Quick Wins                            │
│  ├── H-4  Vault KDF upgrade ................. 45m [HIGH]    │
│  ├── M-6  ListSecrets auth .................. 30m [MEDIUM]  │
│  ├── M-7  Zeroize vault .................... 30m [MEDIUM]   │
│  ├── ARCH-6  pub(crate) cleanup ............. 1h            │
│  ├── ARCH-8  Rename core/ → catalog/ ........ 30m           │
│  └── ARCH-9  Move ContentBlock .............. 30m           │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                    POST-LAUNCH (May 5+)                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Sprint A: Decomposition (1 week)                           │
│  ├── ARCH-1  Split runner.rs ................ 4h            │
│  ├── ARCH-2  NikaError domain migration ..... 6h            │
│  └── QA-1    Fix blocking I/O ............... 30m           │
│                                                             │
│  Sprint B: Extraction (1 week)                              │
│  ├── ARCH-3  Extract nika-provider crate .... 8h            │
│  ├── ARCH-5  Decompose RunContext ........... 4h            │
│  └── ARCH-7  Move NikaConfig ............... 2h             │
│                                                             │
│  Sprint C: Cleanup (1 week)                                 │
│  ├── ARCH-4  Unify LSP (delete 12K LOC) ..... 6h           │
│  ├── QA-4    dead_code audit ................ 4h            │
│  ├── QA-5    is_empty assertions ............ 4h            │
│  └── QA-7    strum derives .................. 1h            │
│                                                             │
│  Ongoing:                                                   │
│  └── QA-10   unwrap() audit (module by module)              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Detailed Plans

| Plan | File | Focus | Items |
|------|------|-------|-------|
| **Plan 1** | `2026-04-04-plan1-security-critical-fixes.md` | C-1 + H-1..H-4 + M-5..M-7 | 8 fixes |
| **Plan 2** | `2026-04-04-plan2-architecture-decomposition.md` | ARCH-1..ARCH-9 | 9 refactors |
| **Plan 3** | `2026-04-04-plan3-code-quality-rust-idioms.md` | QA-1..QA-10 | 10 improvements |
| **Plan 4** | `2026-04-04-plan4-dx-documentation-sync.md` | DOC-1..DOC-9 | 9 doc fixes |

---

## Invariants (Hold at All Times)

1. `cargo test --workspace --lib` — 9800+ tests pass
2. `cargo clippy --workspace -- -D warnings` — clean
3. No new `unsafe` blocks without security review
4. No new `unwrap()` in production code paths
5. No new `pub` exports from `runtime/mod.rs`
6. No new `NikaError` variants — use domain enums
7. Schema stays at `nika/workflow@0.12`
8. Zero backward compatibility hacks (0 users = 0 compat)
9. AGPL-3.0-or-later on all crates
10. Co-author lines on all commits
