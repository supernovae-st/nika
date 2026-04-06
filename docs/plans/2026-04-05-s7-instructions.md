# S7 Instructions — nika keys Implementation + v0.72 CHANGELOG

> **Baseline**: 10,102 tests GREEN, 534K+ LOC, 17 crates, 86 EventKind variants, HEAD = `35256dbbd`
> **Previous**: S6 (9 commits: deps, docs, telemetry) + on_error DONE (391 LOC, 20 files) + vault hardening
> **Goal**: Implement `nika keys` — unified API key management command (5 subcommands)
> **Launch**: May 5, 2026 (30 days)
> **Skills**: `/spn-rust:rust-core`, `/spn-powers:verification-before-completion`, `/spn-powers:test-driven-development`, `/spn-powers:brainstorming`

---

## WHAT HAPPENED SINCE S6

7 commits (by Thibaut) after our S6 push:

| Commit | What | Impact |
|--------|------|--------|
| `da76329` | CLI refs: provider set → keys set | docs only |
| `2b720ef` | **vault**: atomic writes, custom keys, try_open_vault | 30 files, vault hardening |
| `456b2c3` | nika keys mega plan + keys.rs stub (1,102 LOC) | Design doc + code scaffold |
| `3be8a09` | nika keys mega handoff (386 lines) | Implementation guide |
| `4b23c75` | Enrich keys handoff (565 lines) | Fully autonomous handoff |
| `ba03b29` | Align handoff — exact v0 nuke targets | Provider/Vault enum cleanup |
| `35256db` | **on_error: fallback routing** (391 LOC, 20 files) | FEATURE DONE, 10,102 tests |

**on_error: is DONE** — ignore, retry_with_provider, fallback all implemented with NIKA-290, TaskFallbackTriggered event, depth limit 1. Tests pass.

**LSP diagnostics: ALREADY DONE** — `nika-lsp/src/diagnostics.rs` (379 lines, 5-phase validation, wired into did_open/did_change). NOT a gap.

---

## Pre-Session Research

```bash
cd tools

# 1. Confirm test baseline
cargo test --workspace --lib 2>&1 | grep "^test result"
# Expected: 10,102 total, 0 failures

# 2. Read the mega plan (ALL design decisions)
cat ../docs/plans/2026-04-05-nika-keys-mega-plan.md

# 3. Read the session handoff (implementation guide)
cat ../docs/sprints/SESSION-KEYS-HANDOFF.md

# 4. Check the keys.rs stub (1,102 LOC already scaffolded)
wc -l nika-cli/src/keys.rs
head -50 nika-cli/src/keys.rs

# 5. Check what Provider/Vault commands exist (to be nuked)
grep -n "ProviderAction\|VaultAction" nika-cli/src/lib.rs nika/src/main.rs
grep -n "pub mod vault\|pub mod provider" nika-cli/src/lib.rs

# 6. Check vault API (try_open_vault, custom keys)
grep -n "pub fn\|pub async fn" nika-vault/src/lib.rs | head -15

# 7. Check cliclack availability
grep "cliclack" Cargo.toml nika-cli/Cargo.toml nika/Cargo.toml
```

---

## DESIGN DECISIONS (Non-Negotiable — from 40+ agent brainstorm)

### Name: `keys` (research: 30/35 pts, precedent: Simon Willison's `llm keys set openai`)

### 5 Commands
```
nika keys              ← bare = list (rich display, categorized)
nika keys set <name>   ← add key (smart provider detection + cliclack)
nika keys remove <name> ← remove from vault
nika keys check        ← test all keys with latency bars
nika keys sync         ← push to GitHub Actions via gh CLI
```

### 4 Categories (hidden if empty)
```
🧠 Inference   — anthropic, openai, mistral, groq, deepseek, gemini, xai
🔍 Search      — serper, tavily, firecrawl
🔧 Custom      — user-defined keys via nika keys set custom:MY_KEY
◎ Local        — native (GGUF), mock
```

### Icons
```
● configured (green)   · not set (dimmed)   ◎ system (no key needed)
○ offline              ⚠ env-only (yellow warning: not in vault)
```

### v0 Philosophy — NUKE old commands
```
DELETE: ProviderAction::{Set, Get, Delete, Migrate, VaultReset}
DELETE: VaultAction entirely (Set, List, Check, Export, Import)
KEEP:   ProviderAction::{List, Test}
```
No deprecation hints. No aliases. BUT: "Did you mean?" when user types old commands.

### 15 Smart UX Moments (see mega plan for full list)
- `nika provider set X` → "Did you mean? `nika keys set X`"
- `nika keys set claude` → auto-resolve alias: claude → anthropic
- `nika keys set antrhopic` → "Did you mean? anthropic" (Levenshtein)
- `nika keys set sk-ant-abc123` → "That's a key, not a name → anthropic"
- `nika keys set` (bare) → interactive picker (cliclack::select)
- ... (10 more in mega plan)

---

## IMPLEMENTATION PLAN (7 phases from mega plan)

### Phase 0: P0 Bug Fixes (3 bugs — check if already fixed in vault commit)

```bash
# Check if custom vault keys are injected at runtime
grep -rn "custom:" nika-engine/src/secrets/ --include="*.rs" | head -10
```

1. **P0-1**: Custom vault keys dead at runtime → inject `custom:*` from vault.list()
2. **P0-2**: Provider validation phantom key → clear env on Set before re-check
3. **P0-3**: Vault init atomicity → (DONE in `2b720ef` — atomic writes)

### Phase 1: Create `nika keys` Command Structure

**Files**:
- `nika-cli/src/keys.rs` — Main implementation (stub exists, 1,102 LOC)
- `nika-cli/src/lib.rs` — Register module, add `Keys` variant to Commands enum
- `nika/src/main.rs` — Wire `Commands::Keys` dispatch

**Exact steps**:
1. Read the existing `keys.rs` stub — it has the full design system and screen layouts
2. Implement `KeysAction` enum: `{ List, Set { name: Option<String> }, Remove { name: Option<String> }, Check, Sync { repo: Option<String> } }`
3. Wire into main.rs dispatch

### Phase 2: `nika keys` (bare = list)

Rich display with 4 categories. Read vault + env + daemon. Show source provenance.

**Display format** (from design):
```
  🔑 API Keys
  ─────────────────────────────────────────

  🧠 Inference
  ● anthropic        sk-ant-••••7f2k    vault
  ⚠ openai           sk-••••3b1a        env     ← save to vault: nika keys set openai
  · mistral

  🔍 Search
  · serper
  · firecrawl

  ◎ Local
  ◎ native           system (no key needed)
  ◎ mock             test provider
```

### Phase 3: `nika keys set <name>`

Interactive cliclack-based key input with smart detection:
- Alias resolution (claude → anthropic)
- Typo correction (Levenshtein distance)
- Key prefix detection (sk-ant-* → anthropic)
- Env var name detection (ANTHROPIC_API_KEY → anthropic)
- Interactive picker when bare `nika keys set`

### Phase 4: `nika keys remove <name>`

Delete from vault. Interactive picker when bare.

### Phase 5: `nika keys check`

Test all configured keys with real API calls. Show latency bars.

### Phase 6: `nika keys sync`

Push secrets to GitHub Actions via `gh secret set`. Requires `gh` CLI.

### Phase 7: Nuke Old Commands + Did You Mean

Delete ProviderAction variants, delete VaultAction entirely, add "Did you mean?" hints.

---

## KEY FILES

| What | Path | Notes |
|------|------|-------|
| keys.rs stub | `tools/nika-cli/src/keys.rs` | 1,102 LOC scaffold |
| CLI entry | `tools/nika/src/main.rs` | Commands enum dispatch |
| CLI modules | `tools/nika-cli/src/lib.rs` | Module registration |
| Provider commands (NUKE) | `tools/nika-cli/src/provider.rs` | Delete Set/Get/Delete/Migrate/VaultReset |
| Vault commands (NUKE) | `tools/nika-cli/src/vault.rs` | Delete entire module |
| Vault crate | `tools/nika-vault/src/lib.rs` | try_open_vault, custom keys |
| Secrets engine | `tools/nika-engine/src/secrets/` | SecretStore, fallback.rs |
| Mega plan | `docs/plans/2026-04-05-nika-keys-mega-plan.md` | Full implementation spec |
| Session handoff | `docs/sprints/SESSION-KEYS-HANDOFF.md` | 565 lines, fully autonomous |
| Design system | (inside keys.rs) | Icons, colors, typography, dashed separators |

---

## ARCHITECTURE NOTES

### Crate Boundaries
- `nika-vault` — encrypted credential storage (XChaCha20 + Argon2i). AGPL. Pure crypto, no CLI.
- `nika-cli/src/keys.rs` — CLI UX layer. Uses cliclack for interactive prompts. Calls vault API.
- `nika-engine/src/secrets/` — Runtime secret resolution. Order: env → daemon → vault → None.

### Vault API (from `2b720ef`)
```rust
pub fn try_open_vault() -> Option<NikaVault>          // Non-panicking vault open
pub fn set(&self, key: &str, value: &str) -> Result   // Store encrypted
pub fn get(&self, key: &str) -> Result<Option<Secret>> // Retrieve
pub fn delete(&self, key: &str) -> Result              // Remove
pub fn list(&self) -> Result<Vec<String>>              // All key names
```

### Rust Quality Patterns
- `Arc<str>` for shared strings, not `String.clone()`
- `thiserror` for error types, not `anyhow`
- `cliclack` for interactive prompts (already in deps)
- `colored` for terminal output (already in deps)
- No `unwrap()` in production code — `?` operator everywhere
- AGPL-3.0-or-later header on new files

---

## VERIFICATION CHECKLIST

After ALL phases:
```bash
cd tools
cargo test --workspace --lib                    # ALL pass (10,102+)
cargo clippy --workspace                        # ZERO warnings
cargo check --workspace 2>&1 | grep warning     # ZERO warnings
git log --oneline -10                           # Clean commit history
```

---

## RULES

- `cargo test --workspace --lib` green after EVERY commit
- 1 fix = 1 commit (never batch unrelated changes)
- Co-author: ONLY `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- AGPL-3.0-or-later on new files
- v0 = ZERO backward compat. Delete old code, don't deprecate.
- Use `/spn-rust:rust-core` skill for Rust patterns
- Use `/spn-powers:verification-before-completion` before each commit
- Use `/spn-powers:test-driven-development` for each phase
- Read the FULL mega plan before writing any code
- Read the FULL session handoff before writing any code

---

## GRAND NETTOYAGE ROADMAP (updated)

```
S6   ✅  Deps + docs + telemetry (9 commits)
     ✅  on_error: fallback routing (391 LOC, 20 files)
     ✅  Vault hardening (atomic writes, custom keys)
S7   →   nika keys (5 commands, nuke old Provider/Vault)
S8       TUI polish + dedup
S9       Scheduling / cron (blueprint: 1,458 lines)
S10      Multi-tenant auth (blueprint: ~500 lines)
S11      PostgreSQL store (~1,500 LOC)
S12      CLI UX + final pre-launch polish
```
