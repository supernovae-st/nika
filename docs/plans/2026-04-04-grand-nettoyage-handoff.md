# Grand Nettoyage — Handoff Prompt

> Copy-paste this as your opening prompt for each session.
> Update the "CURRENT SESSION" section before each session.

---

## CURRENT SESSION: S2 — Engine Runner Decomposition

**Previous**: S1 Security Hardening completed (6 commits, 9,871 tests green).
**Next after this**: S3 — Provider Layer Cleanup

---

## Context

You are executing the **Grand Nettoyage** — a 13-session stabilization sprint for Nika (Rust workflow engine, 380k LOC, 18 crates, 9,847 tests) before the May 5, 2026 launch.

**Philosophy**: Zero dead code. Improve everything. Delete nothing. Elegant architecture. v0 = zero backward compat.

**Master plan**: `nika/docs/plans/2026-04-04-grand-nettoyage-master-plan.md`

---

## Key Files

| What | Path |
|------|------|
| **Master Plan** | `nika/docs/plans/2026-04-04-grand-nettoyage-master-plan.md` |
| **This Handoff** | `nika/docs/plans/2026-04-04-grand-nettoyage-handoff.md` |
| **SDK Audit** | `docs/plans/2026-04-04-sdk-api-mega-audit.md` |
| **SDK Consolidated** | `docs/plans/2026-04-04-sdk-audit-consolidated-report.md` |
| **Egghead Bible** | `nika/docs/plans/2026-03-31-egghead-design-bible.md` (NOT in scope) |
| **Runner Decomposition** | `nika/docs/plans/2026-04-01-arch2-runner-decomposition.md` (reference) |
| **CLI UX Plan** | `nika/docs/plans/2026-04-01-cli-ux-improvements.md` (reference) |

---

## Codebase Architecture (from 10-agent audit)

```
18 crates | 380,780 LOC | 5-tier layered architecture | Zero circular deps

Tier 0 (Pure Data):     nika-core (31k), nika-vault (1.3k), nika-init (21k), nika-storage (1.5k)
Tier 1 (Coordination):  nika-event (5.5k), nika-mcp (8.6k), nika-media (14k)
Tier 2 (Engine Hub):    nika-engine (168k) — 44% of codebase, THE target for decomposition
Tier 3 (Interfaces):    nika-cli (18k), nika-tui (88k), nika-serve (5k), nika-lsp (3.5k+12k core), nika-daemon (6k)
Tier 4 (Bindings):      nika-sdk (2.5k), nika-napi (deprecated), nika-py (deprecated)
```

### Critical Files

| File | LOC | What |
|------|-----|------|
| `nika-engine/src/runtime/runner.rs` | 8,252 | DAG executor — monolith to decompose |
| `nika-engine/src/binding/template.rs` | 4,676 | Template engine |
| `nika-engine/src/binding/resolve.rs` | 3,898 | Binding resolution |
| `nika-engine/src/runtime/executor/infer.rs` | 1,798 | Infer verb + L0 structured output |
| `nika-engine/src/runtime/structured_output.rs` | ~1,200 | L2/L3/L4 structured output |
| `nika-engine/src/runtime/security.rs` | ~800 | Exec blocklist, shell injection |
| `nika-engine/src/provider/rig/mod.rs` | 2,113 | Provider enum, all 9 variants |
| `nika-engine/src/error.rs` | 2,802 | NikaError, 127 error codes |
| `nika-vault/src/lib.rs` | 1,270 | XChaCha20Poly1305 vault |
| `nika-serve/src/auth.rs` | 58 | Bearer token auth — 0 TESTS |

---

## 13 Sessions Overview

### S1 — Security Hardening (P0) ← START HERE

Fix 1 HIGH + 5 MEDIUM security findings. All have specific file paths and fixes.

| # | What | File | Fix |
|---|------|------|-----|
| 1.1 | Shell quote-breakout (M-2) | `nika-engine/src/runtime/executor/exec.rs` | Require `\| shell` for bindings in single-quote context when value contains `'`, OR validate resolved values |
| 1.2 | Exec blocklist gaps (H-1) | `nika-engine/src/runtime/security.rs` | Add `command `, `builtin `, `nohup `, `nice `, `timeout `, `strace ` to BLOCKLIST |
| 1.3 | KDF parameter upgrade (M-5) | `nika-vault/src/lib.rs:598` | `1 << 16` → `1 << 21` (2 MiB). Consider Argon2id. Auto-migrate on next write. |
| 1.4 | Serve percent-encoding (M-4) | `nika-serve/src/routes/workflows.rs` | Add `%2e%2e`, `%2f`, `%5c` checks matching `nika-sdk/src/remote.rs:validate_path_segment` |
| 1.5 | Artifact symlink (M-3) | `nika-engine/src/io/security.rs` | After parent dir creation, `validate_canonicalized_boundary()` before write |
| 1.6 | Shell injection patterns (L-1) | `nika-engine/src/runtime/security.rs` | Add `&&`, `\|\|`, `;`, `>`, `>>`, `\|` to SHELL_INJECTION_PATTERNS for single-quote-exempt bindings |

**Each fix**: write test FIRST (TDD), then implement, then `cargo test --workspace --lib`.
**Commit format**: `fix(security): <description>` with co-authors.

### S2 — Engine Runner Decomposition (P1)

Split runner.rs from 8,252 → <3,000 LOC.

| # | What | Target |
|---|------|--------|
| 2.1 | Extract for_each | New `runtime/for_each.rs` — `ForEachExpander`, unified item resolution |
| 2.2 | Extract scheduler | New `runtime/scheduler.rs` — `TaskScheduler`, `get_ready_tasks()`, bitset deps |
| 2.3 | Unify DAG | Single `IndexedDag` for runtime + TUI, remove `Dag` (HashMap) |
| 2.4 | Dead code | Remove `_completed`, `cookie_jar`, `fetch_cache` |

### S3 — Provider Layer Cleanup (P1)

| # | What |
|---|------|
| 3.1 | Mock → `RigProvider::Mock` variant (remove string sentinel) |
| 3.2 | Deduplicate OpenAiCompat raw HTTP (3x → 1x helper) |
| 3.3 | Wire ModelResolver into agent loop (remove hardcoded models) |
| 3.4 | Fix CLI explain cost estimation (use ModelPricing tables) |

### S4 — Structured Output Polish (P2)

| # | What |
|---|------|
| 4.1 | Remove ghost Layer 1 (`enable_extractor` → NIKA-010 error) |
| 4.2 | LRU cache for schema/validator (replace clear-all DashMap) |
| 4.3 | L0 fallback: transport errors → try L0b (not skip) |
| 4.4 | Accurate token counts for L0b (real provider counts) |
| 4.5 | Fix double file read in standalone validate path |

### S5 — TUI Polish & Dedup (P2)

| # | What |
|---|------|
| 5.1 | Extract `VerbColor::from_task_type()` (5 copies → 1) |
| 5.2 | Consolidate App constructor (`new_inner` builder) |
| 5.3 | Clean Action enum (remove unwired variants) |
| 5.4 | Remove dead ActivityStack widget |
| 5.5 | Deduplicate `format_duration_ms` |
| 5.6 | Monitor view visual polish + theme consistency |

### S6 — CLI UX Cohérence (P2)

| # | What |
|---|------|
| 6.1 | Fix help text: "29 transforms" → 50 (or dynamic count) |
| 6.2 | Complete help metadata for bench/explain/switch/vault/clean |
| 6.3 | Typed config management (TOML struct, not raw string) |
| 6.4 | Improve explain cost estimation (ModelPricing + token estimates) |
| 6.5 | Replace curl/tail -f with pure Rust (reqwest, notify) |
| 6.6 | Error message audit — every error has actionable suggestion |

### S7 — Builtin Tools Hardening (P2)

| # | What |
|---|------|
| 7.1 | JQ cache: `Mutex` → `parking_lot::RwLock` |
| 7.2 | nika:inject: `current_dir()` → `ToolContext.working_dir` |
| 7.3 | `#[serde(deny_unknown_fields)]` on all tool param structs |
| 7.4 | Delete deprecated `nika:json_query` entirely |

### S8 — Crate Architecture: Engine Decomposition (P1)

| # | What |
|---|------|
| 8.1 | Extract `nika-builtins` crate (61 tools, ~13k LOC from engine) |
| 8.2 | Extract `nika-display` crate (6 renderers, ~8k LOC from engine) |
| 8.3 | Update workspace Cargo.toml + feature forwarding |
| 8.4 | Update CI & release scripts |

### S9 — Rust Pro Optimization (P2)

| # | What |
|---|------|
| 9.1 | Allocation audit (hot paths, unnecessary clone/to_string) |
| 9.2 | Async pattern review (spawn_blocking, select!, DashMap) |
| 9.3 | Error handling elegance (NikaError grouping, unwrap audit) |
| 9.4 | TaskExecutor split (Shared arc-wrapped + Local per-task) |
| 9.5 | get_ready_tasks → incremental ready_set (reverse adjacency) |

### S10 — Test Quality Upgrade (P1 for auth.rs)

| # | What | Priority |
|---|------|----------|
| 10.1 | auth.rs test suite (0→10+ tests) | **P0** |
| 10.2 | Serve integration tests (start server, POST, SSE) | P0 |
| 10.3 | nika-storage test coverage (18 fns, 2 tested) | P1 |
| 10.4 | Security tests for S1 fixes + proptest template/YAML | P1 |
| 10.5 | Cross-provider structured output tests | P1 |
| 10.6 | Weak assertion cleanup (~200 is_ok → value checks) | P2 |
| 10.7 | Display renderer tests (1,593 LOC, 0 tests → insta) | P2 |
| 10.8 | CLI subcommand tests (workflow/mcp/pkg/schema) | P3 |

### S11 — Dead Code & Legacy Purge (P2)

37 findings from audit. Key actions:

| # | What |
|---|------|
| 11.1 | Remove nika-napi/nika-py from workspace, drop 4 unused deps, delete json_query |
| 11.2 | Remove .nika/config.toml legacy fallback (5 files) |
| 11.3 | LSP dead code cleanup (DaemonBridge, 6 position fns, dead fields) |
| 11.4 | Engine dead code (cookie_jar, fetch_cache, mcp_clients, Action enum) |
| 11.5 | Consolidate 3x find_project_root → nika-core |
| 11.6 | Fix incorrect #[allow(dead_code)] annotations |
| 11.7 | TODO/FIXME triage (2 remaining) |
| 11.8 | SDK dead fields cleanup |

### S12 — Integration & Smoke Testing (P1)

Full pass: 10 workflows E2E, every CLI command, TUI walkthrough, nika serve.

### S13 — Final Polish & Documentation (P1)

CHANGELOG, README, error codes, help system, tag v0.70.0-rc1.

---

## Workflow Per Session

```
1. Read the master plan section for this session
2. For each fix:
   a. Read the target file(s)
   b. Write test FIRST (TDD)
   c. Run test → verify it FAILS
   d. Implement the fix
   e. Run test → verify it PASSES
   f. cargo test --workspace --lib
   g. git add <specific files>
   h. git commit -m "type(scope): description"
3. After all fixes: cargo clippy --workspace
4. Update handoff: change CURRENT SESSION to next
```

---

## Commit Format

```
type(scope): concise description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `fix`, `refactor`, `test`, `chore`, `perf`
Scopes: `security`, `engine`, `provider`, `tui`, `cli`, `tools`, `vault`, `serve`, `sdk`

---

## Rules

- **Test BEFORE commit.** No exceptions.
- **1 fix = 1 commit.** No batching unrelated fixes.
- **cargo test --workspace --lib** — always `--lib` to avoid keychain popups.
- **Zero backward compat.** v0 = 0 users. Break freely.
- **Don't ask cleanup questions.** Just do what's best architecturally.
- **Egghead (memory) is NOT in scope.** Post-stabilization only.
- **All crates AGPL-3.0-or-later**, not MIT.

---

## Schedule

```
Week 1 (Apr 7-11):    S1 Security      → S2 Engine Decomposition
Week 2 (Apr 14-18):   S3 Provider      → S4 Structured Output    → S5 TUI
Week 3 (Apr 21-25):   S6 CLI UX        → S7 Builtin Tools        → S8 Crate Extraction
Week 4 (Apr 28-May 2): S9 Rust Pro     → S10 Test Quality        → S11 Dead Code
Week 5 (May 2-5):     S12 Integration  → S13 Final Polish        → 🚀 LAUNCH
```

---

## Metrics

| Metric | Before | Target |
|--------|--------|--------|
| nika-engine LOC | 168,000 | <120,000 |
| runner.rs LOC | 8,252 | <3,000 |
| Crate count | 18 | 20 |
| Security findings | 1H+5M | 0 |
| auth.rs tests | 0 | 10+ |
| Dead code findings | 37 | 0 |
| Tests | 9,847 | 10,000+ |
| Help text accuracy | ~80% | 100% |
| Unused deps | 4 | 0 |
| TODO/FIXME | 2 | 0 |
