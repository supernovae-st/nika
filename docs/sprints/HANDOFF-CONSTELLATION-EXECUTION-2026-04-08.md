# Constellation Execution Handoff — 2026-04-08

> **Copy-paste this ENTIRE file as context for a fresh Claude Code session.**
> **It contains everything needed to execute the Constellation architecture refactor.**

---

## SITUATION

- **Version:** v0.79.0 (Shield Sprint 2 merged, PR #107)
- **Branch:** main
- **Last commit:** `99cba267b` chore(license): add SPDX headers to all 862 .rs source files
- **Tests:** 10,666+ (`cargo test --workspace --lib` from `tools/nika/`)
- **Binary:** 112 MB release
- **Crates:** 17 (target: ~38)
- **LOC:** ~555k Rust, 160k in nika-engine monolith
- **Launch:** May 5, 2026 (J-27)
- **Codename:** Constellation v2.1
- **Working directory:** `/Users/thibaut/dev/supernovae/nika/tools/nika`

### BLOCKER: nika-media Linker Error

`cargo test --workspace --lib` currently FAILS on nika-media (ARM64 linker error).
Multiple html5ever versions (0.36, 0.38, 0.39) conflict with `lto = "thin"`.

**FIX FIRST (1 commit):** Add to `tools/Cargo.toml`:
```toml
[profile.test]
opt-level = 1
lto = false
```

---

## MANDATORY READS (in order)

Read these files BEFORE writing any code:

1. `nika/CLAUDE.md` — project overview, 5 verbs, testing philosophy
2. `tools/nika/CLAUDE.md` — crate architecture, error codes, testing rules
3. `docs/plans/2026-04-08-constellation-v2-mega-plan.md` — THE PLAN (1820 lines)
   - Section 3: Target ~38 crate architecture
   - Section 5: 10 trait specs
   - Section 7: God file decomposition
   - Section 8: 20-phase roadmap
   - Section 18: v2.1 revisions (big-bang OK, bundles, TaskScope splinters)
4. `docs/sprints/CONSTELLATION-FINDINGS-2026-04-08.md` — AUDIT RESULTS from 11 agents
   - 4 P0 bugs, 5 P1 improvements, 6 quick wins with exact file:line locations
5. `dx/.claude/rules/architecture.md` — current + target crate layering

---

## SKILLS TO USE

Use the Skill tool to load these skills BEFORE starting work:

- `/spn-rust:rust` — Master Rust skill (routes to rust-core, rust-async)
- `/spn-powers:verification-before-completion` — MANDATORY before claiming any phase done
- `/spn-powers:test-driven-development` — TDD for new traits and mocks

For specific phases:
- `/spn-rust:rust-async` — Phase 1 (kernel traits), Phase 9 (runtime)
- `/spn-rust:rust-core` — Phase 4 (error domains), Phase 19 (type hardening)
- `/spn-powers:systematic-debugging` — If any test breaks during refactor
- `/spn-powers:requesting-code-review` — After completing each phase

---

## 4 P0 BUGS TO FIX FIRST

Fix these BEFORE starting any Constellation phase. 1 commit each.

### P0-1: nika-media Linker (UNBLOCKS ALL TESTS)
**File:** `tools/Cargo.toml`
**Fix:** Add `[profile.test]` with `lto = false`
**Commit:** `fix(build): disable LTO for test profile to fix ARM64 linker error`

### P0-2: Dead MPSC Receiver in infer.rs (2 instances)
**File:** `nika-engine/src/runtime/executor/infer.rs`
**Lines:** 1075, 1277 — `let (tx, _rx) = mpsc::channel::<StreamChunk>(1);`
**Bug:** Receiver dropped immediately. Provider writes to dead channel.
**Fix:** Either drain with spawned task, or use larger buffer (256) + document why.
**Commit:** `fix(runtime): wire stream receiver in non-streaming infer paths`

### P0-3: McpInvoke Params Written UNREDACTED to Traces
**File:** `nika-engine/src/runtime/executor/invoke.rs:116`
**Bug:** `params: resolved_params.clone().map(Arc::new)` — no redaction
**Fix:** `params: resolved_params.as_ref().map(|p| Arc::new(redact_value(p)))`
**Commit:** `fix(security): redact McpInvoke params before trace serialization`

### P0-4: Serve Mutex Held Across .await
**File:** `nika-serve/src/routes/workflows.rs:320-343`
**Bug:** `state.workers.lock().await.remove(&id)` holds lock through async storage call
**Fix:** Extract lock result to variable BEFORE the if-let block:
```rust
let handle = state.workers.lock().await.remove(&id);
if let Some(handle) = handle { ... }
// Lock released here, before .await
state.storage.update_state(...).await?;
```
**Commit:** `fix(serve): release workers lock before async storage update`

---

## 6 QUICK WINS (after P0 bugs, 1 commit each)

### QW-1: `#[must_use]` on 10 Runner Builder Methods
**File:** `nika-engine/src/runtime/runner/mod.rs`
**Lines:** 416, 427, 436, 447, 480, 506, 515, 522, 530, 540

### QW-2: `FxHashSet` in template.rs (6 sites)
**File:** `nika-engine/src/binding/template.rs`
**Lines:** 555, 641, 725, 1377, 1476, 1563
Replace `std::collections::HashSet<String>` with `FxHashSet<String>` (already imported line 76).

### QW-3: `mem::take` for StreamChunk::Done (2 sites)
**File:** `nika-engine/src/provider/rig/provider_streaming.rs`
**Lines:** 137, 306 — replace `response_buf.clone()` with `mem::take(&mut response_buf)`

### QW-4: `OnceLock` for RunContext workspace_root
**File:** `nika-engine/src/store/run_context.rs:289`
Replace `Arc<RwLock<PathBuf>>` with `OnceLock<PathBuf>` (write-once field).

### QW-5: Hoist `Arc::from(task_id)` Out of Streaming Loops (5 sites)
**File:** `nika-engine/src/runtime/rig_agent_loop/streaming.rs`
**Lines:** 168, 253, 271, 647, 682

### QW-6: Write ARCHITECTURE.md
**File:** `tools/nika-engine/ARCHITECTURE.md` (does not exist yet)
Matklad rule. Document the 20 modules, crate deps, invariants, historical scaffolding.

---

## CONSTELLATION PHASES — EXECUTION ORDER

After P0 bugs + quick wins, execute phases in this order.
**Big-bang cutovers OK** — v0, zero users, zero backward compat.

```
Phase 1:  Create nika-kernel crate + 10 trait defs        [L0.5, ~800 LOC]
Phase 2:  Create nika-kernel-mock + conformance tests      [dev-dep only]
Phase 3:  Create nika-macros crate (4 derives + transform!) [proc-macro2, syn 2, quote]
Phase 4:  Adopt rstest on transform.rs pilot               [tools/Cargo.toml:259 already has dep]
Phase 5:  Wire EventEmitter via blanket impl               [2 commits: blanket + flip 5 hot sites]
Phase 6:  Promote error_domains (1 big-bang commit)         [84 call sites, all From impls ready]
Phase 7:  Absorb nika-engine/lsp/ into nika-lsp-core       [2 commits, incremental NOT delete]
Phase 8:  God file mechanical splits                        [transform→12, template→9, runner→7]
Phase 9:  Extract nika-clock, nika-fs, nika-blob            [3 parallel extractions]
Phase 10: Extract nika-http, nika-exec-runner               [2 parallel]
Phase 11: Create nika-provider + Provider trait cutover     [big-bang, wraps RigProvider]
Phase 12: Create nika-builtin + #[builtin_tool] via linkme  [40+ tools, sealed]
Phase 13: Create nika-verb-* crates (5 verb crates)         [VerbExecutor trait + impls]
Phase 14: Create nika-runtime, decompose RunContext          [TaskScope splinters, bundles]
Phase 15: Extract nika-cache                                [trust-aware keys]
Phase 16: Migrate main.rs → nika-cli/verbs/                 [5527→<500 LOC, 1-3 big commits]
Phase 17: Split analyze.rs (11 files)
Phase 18: Split nika-tui + wire dead feature flags           [nika-tui-{widgets,core,views,app}]
Phase 19: Type system hardening                              [TaskId, SecretString, sealed traits]
Phase 20: Polish + validation                                [pub API curation, lint bumps]
```

**Parallel opportunities:** P1+P2+P3 | P5+P6 | P8 sub-files | P9 sub-crates | P10 sub-crates

---

## KEY NUMBERS FROM AUDIT

| Finding | Value | Location |
|---------|-------|----------|
| Dead receiver | 2 instances | infer.rs:1075, :1277 |
| Unredacted trace | 1 event type | invoke.rs:116 (McpInvoke.params) |
| Serve Mutex .await | 1 site | workflows.rs:320 |
| EventEmitter dead uses | 49 files, 351 emits, 27 structs | emitter.rs trait |
| error_domains ready | 84 call sites, 4 From impls | error_domains.rs (253 LOC) |
| IndexedDag unused | 878 LOC built, 0 Runner use | dag/indexed.rs |
| Interner fake | 55 callers, 0 dedup | util/interner.rs:18 |
| LSP overlap | ~7k LOC shared | engine/lsp/ vs nika-lsp-core/ |
| main.rs | 5,527 LOC (Ruff=78, Helix=160) | nika/src/main.rs |
| Unwrap hotspot | 399 in transform.rs, 206 in resolve.rs | binding/ |
| FxHashSet missing | 6 sites | template.rs:555,641,725,1377,1476,1563 |
| Runner #[must_use] | 10 builder methods | runner/mod.rs:416-540 |
| Arc hoist needed | 5 sites in loops | streaming.rs:168,253,271,647,682 |
| TUI features | 18 pass-through (by design) | nika-tui/Cargo.toml |
| rstest | declared, 0 usage | workspace Cargo.toml:259 |

---

## RULES — NON-NEGOTIABLE

### Build
- `cargo test --workspace --lib` after EVERY commit (from `tools/nika/`)
- `cargo clippy --workspace -- -D warnings` — zero warnings
- Never `cargo test` without `--lib` (triggers macOS Keychain popups)

### Git
- 1 logical change = 1 commit
- Format: `refactor(arch): <what>` or `fix(scope): <what>` or `feat(scope): <what>`
- Co-author ALWAYS: `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- Do NOT push unless explicitly asked
- Stage specific files, never `git add -A`

### Architecture
- **CONNECT, DO NOT DELETE.** Dead scaffolding gets wired, not removed.
- Big-bang cutovers OK (v0 = zero users)
- No crate > 15k LOC source (excluding tests)
- No file > 1500 LOC
- Every side effect behind a trait
- Tests move WITH their code
- Run `editors/sync-editors.sh --fix` if catalogs change

### What You May NOT Touch
- The 5 verbs (infer, exec, fetch, invoke, agent) — NEVER change
- Schema version `nika/workflow@0.12` — NEVER change
- AGPL license — NEVER change
- Shield files (merged, stable) — only if directly needed

---

## HOW TO ORGANIZE A SESSION

### Starting a phase
1. Read the mega-plan section for that phase
2. Read the findings doc for relevant bugs/details
3. Load relevant skill (`/spn-rust:rust`, etc.)
4. Create tasks with TaskCreate for each commit planned
5. Measure baseline: `cargo test --workspace --lib 2>&1 | grep "test result"`

### During a phase
1. Mark task in_progress before starting
2. Make the change
3. `cargo test --workspace --lib` (must pass)
4. `cargo clippy --workspace -- -D warnings` (must be clean)
5. Commit with proper format + co-author
6. Mark task completed
7. Repeat

### Finishing a phase
1. Use `/spn-powers:verification-before-completion`
2. Run full test suite
3. Count test delta (must not decrease from 10,666)
4. Use `/spn-powers:requesting-code-review` for review
5. Report: commits created, test delta, next phase readiness

---

## PRIORITY ORDER FOR EXECUTION

```
SESSION 1 — Unblock + Quick Wins
  1. Fix nika-media linker (P0-1)
  2. Fix dead receiver (P0-2)  
  3. Fix McpInvoke redaction (P0-3)
  4. Fix serve Mutex (P0-4)
  5. QW-1 through QW-6
  6. Write ARCHITECTURE.md
  → Target: 11 commits, tests green, baseline established

SESSION 2 — Kernel + Foundation (Phases 1-4)
  7. Create nika-kernel crate with 10 trait defs
  8. Create nika-kernel-mock with conformance tests
  9. Create nika-macros crate (4 derives)
  10. Adopt rstest on transform.rs pilot
  → Target: 4 new crates, traits defined, macros working

SESSION 3 — Wire Dead Scaffolding (Phases 5-7)
  11. EventEmitter blanket impl (2 commits)
  12. error_domains big-bang promotion (1 commit, 84 sites)
  13. LSP absorption (2 commits)
  → Target: 0 dead scaffolding remaining

SESSION 4 — God File Splits (Phase 8)
  14. transform.rs → 12 files
  15. template.rs → 9 files
  16. runner/mod.rs → 7 files + scheduler.rs
  → Target: No file > 1500 LOC

SESSION 5+ — Crate Extractions (Phases 9-16)
  17. Effect crates (clock, fs, blob, http, exec)
  18. nika-provider (Provider trait cutover)
  19. nika-builtin (#[builtin_tool] + linkme)
  20. nika-verb-* (5 verb crates)
  21. nika-runtime (RunContext → TaskScope splinters)
  22. nika-cache
  23. main.rs → nika-cli/verbs/
  → Target: nika-engine eliminated, ~38 crates

SESSION 6 — Polish (Phases 17-20)
  24. analyze.rs split
  25. nika-tui split + wire features
  26. Type system hardening
  27. Public API curation
  → Target: v0.80.0, May 5 launch ready
```

---

## CONTEXT FILES FOR REFERENCE

These don't need reading upfront but are useful during specific phases:

- `SECURITY.md` — threat model (for Shield-related changes)
- `docs/plans/2026-04-07-nika-shield-mega-plan.md` — Shield architecture
- `~/.claude/rules/nika-bugs-and-patterns.md` — workflow engine patterns
- `~/.claude/rules/nika-project-structure.md` — nika.toml + .nika/ conventions
- `~/.claude/rules/nika.md` — full workflow syntax reference
- Memory: `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md`

---

## VERIFICATION COMMAND

After every commit:
```bash
cargo test --workspace --lib 2>&1 | grep "test result" && cargo clippy --workspace -- -D warnings 2>&1 | tail -5
```

Expected baseline (post P0-1 fix):
```
test result: ok. 10666 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

**Start with SESSION 1. Fix the 4 P0 bugs, then quick wins.**
**Make Nika the most elegant Rust workflow engine in existence.**
