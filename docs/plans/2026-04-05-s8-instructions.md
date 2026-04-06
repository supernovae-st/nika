# S8 Instructions — nika keys Phase 2 (Polish, Wire, Clean) + v0.72 CHANGELOG

> **Baseline**: 10,123 tests GREEN, 534K+ LOC, 17 crates, 86 EventKind variants, HEAD = `4d030b368`
> **Previous since S6**: on_error DONE (391 LOC), vault hardening, nika keys DONE (2,218 LOC), auto-infer provider, NIKA-163, dead code removal (json_query, enable_extractor, RoutingStrategy), provider array syntax
> **Goal**: Polish nika keys (8 tasks), delete vault.rs (-693 LOC), wire setup, v0.72 CHANGELOG
> **Skills**: `/spn-rust:rust-core`, `/spn-powers:verification-before-completion`, `/spn-powers:test-driven-development`

---

## SITUATION REPORT

### What's DONE (since S6)

| Feature | Commit | Status |
|---------|--------|--------|
| on_error: fallback routing | `35256db` | SHIPPED — ignore, retry_with_provider, fallback |
| Vault hardening | `2b720ef` | SHIPPED — atomic writes, custom keys, try_open_vault |
| nika keys (5 commands) | `b2a0cf7` | SHIPPED — list, set, remove, check, sync (2,218 LOC) |
| Scheduling design docs | 5 commits | DESIGNED — mega prompt + UX bible + blueprint ready |
| Auto-infer provider from model | `8fbbdbb` | SHIPPED — e.g. model: gpt-4o → provider: openai |
| NIKA-163 misplaced fields | `125d2030a` | SHIPPED — reject task fields inside verb blocks |
| Provider array → fallback chain | `372fb75` | SHIPPED — wire `provider: [anthropic, openai]` |
| Remove deprecated json_query | `604c126` | SHIPPED — nika:jq is the replacement |
| Remove enable_extractor shim | `c83e39f` | SHIPPED — L1 was never implemented |
| Remove dead RoutingStrategy | `d98fff3` | SHIPPED — never read by engine |
| Warn output:json without schema | `4d030b3` | SHIPPED — analyzer warning |

### What's IN PROGRESS (other sessions)

| Feature | Session | Status |
|---------|---------|--------|
| Scheduling/cron (`nika every`) | Another agent | DESIGN DONE, implementation in progress |
| Keys Phase 2 (polish) | **THIS SESSION** | Handoff ready at `SESSION-KEYS-PHASE2-HANDOFF.md` |

### What's BLOCKED

| Feature | Blocked By | Notes |
|---------|-----------|-------|
| Multi-tenant auth (V6) | Scheduling (V5) | Schema migration ordering |
| PostgreSQL store | Auth (V6) | Depends on final schema |

---

## Pre-Session Research

```bash
cd tools

# 1. Confirm test baseline
cargo test --workspace --lib 2>&1 | grep "^test result"
# Expected: 10,123 total, 0 failures

# 2. Read the complete Phase 2 handoff (8 tasks, verification checklist)
cat ../docs/sprints/SESSION-KEYS-PHASE2-HANDOFF.md

# 3. Check keys.rs current state
wc -l nika-cli/src/keys.rs          # Expected: 2,218 LOC
wc -l nika-cli/src/vault.rs         # Expected: 692 LOC (to be deleted)

# 4. Check stale command references
grep -rn "nika vault set\|nika provider set\|nika provider delete" . --include="*.rs" | grep -v target
# Expected: 2 hits (vault.rs:153 + fallback.rs:125)

# 5. Check vault.rs imports (to verify safe deletion)
grep -r "crate::vault\|super::vault\|cli::vault" nika-cli/src/ --include="*.rs" | grep -v vault.rs

# 6. Check if onboarding still calls vault directly
grep -n "vault\.set\|vault\.get" nika-cli/src/onboarding.rs
```

---

## 8 TASKS (from SESSION-KEYS-PHASE2-HANDOFF.md)

### Task 1: Wire `nika setup` to use `keys set` (30 min)

**File**: `tools/nika-cli/src/onboarding.rs`
- Current: `vault.set(&provider, &api_key)` (line ~192)
- Target: `keys::handle_keys_set(Some(name), false, false, false).await`
- At end: offer `keys::handle_keys_sync(None, false, false).await`

**Commit**: `feat(cli): wire nika setup to use keys set`

### Task 2: "Did you mean?" for old commands (30 min)

**Files**: `tools/nika-cli/src/provider.rs`, `tools/nika/src/main.rs`
- Add hidden `#[command(hide = true)]` variants for Set/Get/Delete/Migrate/VaultReset in ProviderAction
- Add hidden `Vault` command in Commands enum
- Handler: `eprintln!("  ✗ Did you mean? nika keys set <name>")`

**Commit**: `feat(cli): add did-you-mean errors for old provider/vault commands`

### Task 3: Real connection test in `keys check` (30 min)

**File**: `tools/nika-cli/src/keys.rs` — `test_provider_connection()`
- Currently stubbed (only checks env var)
- Use existing `provider::run_provider_test()` from provider.rs:402
- Make it public, import in keys.rs

**Commit**: `feat(cli): keys check uses real provider connection test`

### Task 4: `keys set` env detection — UX Helper #9 (20 min)

**File**: `tools/nika-cli/src/keys.rs` — `set_known_provider()`
- Detect existing env var before prompting
- Show: `💡 Found OPENAI_API_KEY in environment: sk-••••a3b9`
- Offer: `Save to vault for persistence? (Y/n)`

**Commit**: `feat(cli): keys set detects existing env var — UX helper #9`

### Task 5: Delete `vault.rs` module (-693 LOC) (10 min)

**Files**:
- DELETE `tools/nika-cli/src/vault.rs`
- EDIT `tools/nika-cli/src/lib.rs` — remove `pub mod vault;`

**Pre-check**: verify no imports from vault.rs elsewhere

**Commit**: `refactor(cli): delete vault.rs — vault commands replaced by nika keys`

### Task 6: Fix 4 flaky `secrets::tests` (20 min)

**File**: `tools/nika-engine/src/secrets/mod.rs`
- 4 tests manipulate global env vars, fail in workspace runs
- Pragmatic fix: document isolation requirement

**Commit**: `fix(test): document env var isolation in secrets tests`

### Task 7: Update stale "vault" references (10 min)

**File**: `tools/nika-engine/src/secrets/fallback.rs:125`
- `"nika vault set custom:..."` → `"nika keys set ..."`

**Commit**: `fix(docs): update stale vault references to nika keys`

### Task 8: Update docs — CLAUDE.md, AGENTS.md (15 min)

Replace all `nika provider set` / `nika vault set` with `nika keys set` in:
- `CLAUDE.md`
- `AGENTS.md`
- `README.md` (if needed)

**Commit**: `docs: update command references — nika keys replaces provider/vault`

---

## Phase 2: v0.72 CHANGELOG Entry (15 min, 1 commit)

Insert AFTER the `## [0.71.0]` block. NOTE: ASCII banner (`╔═`) required in first 50 lines.

```markdown
---

## [0.72.0] — 2026-04-05

### Added

- **`on_error:` fallback routing** — 3 recovery strategies when task fails after retries: `ignore` (null output), `retry_with_provider` (different provider), `fallback` (execute another task's action). NIKA-290 for unknown fallback. Depth limit 1.
- **`nika keys`** — Unified API key management. 5 commands: `keys` (categorized list), `keys set` (smart detection + cliclack), `keys remove`, `keys check` (latency bars), `keys sync` (GitHub Actions via gh)
- **Auto-infer provider from model** — `model: gpt-4o` auto-resolves to `provider: openai`
- **Provider array syntax** — `provider: [anthropic, openai]` wired to fallback chain
- **NIKA-163** — Reject misplaced task-level fields inside verb blocks
- **`output: { format: json }` warning** — Warns when used without `structured:` schema on LLM verbs

### Architecture

- **Vault hardening** — Atomic writes (tmp+rename), custom vault keys, `try_open_vault()` helper

### Removed

- **`nika:json_query`** — Deprecated, use `nika:jq` instead
- **`enable_extractor` shim** — Layer 1 was never implemented
- **`RoutingStrategy` enum** — Dead code, never read by engine
- **`vault.rs` module** — Vault commands replaced by `nika keys`
- **`ProviderAction::{Set, Get, Delete, Migrate, VaultReset}`** — Replaced by `nika keys`
```

**Commit**: `docs(changelog): add v0.72 entry — on_error, nika keys, dead code removal`

---

## Summary: 9 commits

| # | Commit | Time |
|---|--------|------|
| 1 | `feat(cli): wire nika setup to use keys set` | 30 min |
| 2 | `feat(cli): add did-you-mean errors for old provider/vault` | 30 min |
| 3 | `feat(cli): keys check uses real provider connection test` | 30 min |
| 4 | `feat(cli): keys set detects existing env var` | 20 min |
| 5 | `refactor(cli): delete vault.rs` | 10 min |
| 6 | `fix(test): document env var isolation in secrets tests` | 20 min |
| 7 | `fix(docs): update stale vault references to nika keys` | 10 min |
| 8 | `docs: update command references` | 15 min |
| 9 | `docs(changelog): add v0.72 entry` | 15 min |

**Total**: ~3h | **Net LOC**: -600 (vault.rs deletion) | **Expected tests**: 10,123+

---

## Verification Checklist

```bash
cd tools
cargo test --workspace --lib                    # ALL pass
cargo clippy --workspace                        # ZERO warnings
nika keys                                       # Categorized display
nika keys set anthropic                         # Interactive flow
nika keys check                                 # Real API latency bars
nika provider set anthropic 2>&1 | grep "mean"  # Did you mean?
nika vault 2>&1 | grep "mean"                   # Did you mean?
grep -r "nika vault set\|nika provider set" tools/ --include="*.rs" | grep -v target  # ZERO hits
ls tools/nika-cli/src/vault.rs 2>/dev/null       # Should NOT exist
```

---

## Rules

- `cargo test --workspace --lib` green after EVERY commit
- 1 fix = 1 commit (never batch unrelated changes)
- Co-author: ONLY `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- AGPL-3.0-or-later on new files
- v0 = ZERO backward compat. Delete old code, don't deprecate. BUT: smart "Did you mean?" UX.
- Use `/spn-rust:rust-core` for Rust patterns
- Use `/spn-powers:verification-before-completion` before each commit

---

## Key File Paths

| What | Path | LOC |
|------|------|-----|
| keys.rs (main impl) | `tools/nika-cli/src/keys.rs` | 2,218 |
| vault.rs (TO DELETE) | `tools/nika-cli/src/vault.rs` | 692 |
| provider.rs (add hidden variants) | `tools/nika-cli/src/provider.rs` | 476 |
| onboarding.rs (wire keys set) | `tools/nika-cli/src/onboarding.rs` | ~300 |
| main.rs (Commands enum) | `tools/nika/src/main.rs` | ~400 |
| lib.rs (module registration) | `tools/nika-cli/src/lib.rs` | ~50 |
| fallback.rs (stale ref) | `tools/nika-engine/src/secrets/fallback.rs` | line 125 |
| Phase 2 handoff | `docs/sprints/SESSION-KEYS-PHASE2-HANDOFF.md` | 338 lines |
| CHANGELOG | `tools/nika/CHANGELOG.md` | after v0.71 block |

---

## Roadmap

```
S6   ✅  Deps + docs + telemetry (9 commits)
     ✅  on_error: fallback routing (391 LOC)
     ✅  Vault hardening (atomic writes, custom keys)
     ✅  nika keys Phase 1 (2,218 LOC, 5 commands)
     ✅  Dead code removal (json_query, enable_extractor, RoutingStrategy)
     ✅  AST improvements (auto-infer, NIKA-163, provider array, output:json warn)
S8   →   nika keys Phase 2 (polish, wire, -693 LOC) + v0.72 CHANGELOG
S9       Scheduling / cron (in progress — another session)
S10      Multi-tenant auth (blocked by S9 V5 migration)
S11      PostgreSQL store (blocked by S10 V6 migration)
S12      Final pre-launch polish
```
