# nika keys Phase 2 — Polish, Wire, Clean

> Mega handoff for the next session. Copy-paste to continue.
> Previous session: implemented `nika keys` (5 subcommands, 34 tests, 2097 lines).
> This session: polish, wire setup, delete dead code, fix flaky tests.

## Context

`nika keys` is IMPLEMENTED and PUSHED. Commit `b2a0cf7e0`. The command works:
- `nika keys` — categorized list with source provenance
- `nika keys set anthropic` — cliclack interactive flow
- `nika keys set MY_CUSTOM --stdin` — non-interactive
- `nika keys remove` — vault + env cleanup
- `nika keys check` — latency bars (stub: needs real API test)
- `nika keys sync` — gh CLI push

**ProviderAction** stripped to `List` + `Test` only. **Vault command** removed from `main.rs` dispatch.

## 8 Tasks (in order)

### Task 1: Wire `nika setup` wizard to use `keys set`

**File:** `tools/nika-cli/src/onboarding.rs`

The setup wizard currently calls vault directly (line 192: `vault.set(&provider, &api_key)`). It should call `keys::set_known_provider` instead, which adds:
- Format validation
- Auto-test
- Console URL display
- Sync offer

**Current flow (onboarding.rs:101-240):**
```
run_onboarding_wizard():
  1. cliclack multi-select providers
  2. For each: cliclack::password → vault.set(provider, key) → inject_secret_to_env
  3. Optional: test connection
```

**Target flow:**
```
run_onboarding_wizard():
  1. cliclack multi-select providers
  2. For each: keys::handle_keys_set(Some(name), false, false, false).await
  3. At end: offer keys::handle_keys_sync(None, false, false).await
```

**Integration point — line 9:** `use super::provider::detect_provider_from_key;`
→ Change to `use super::keys::classify_name;` (or keep both — `detect_provider_from_key` is still used for prefix detection in onboarding.rs:173).

**Tests to update:** `onboarding::tests::onboarding_providers_has_eight` — verify count still matches.

**Commit:** `feat(cli): wire nika setup to use keys set`

---

### Task 2: "Did you mean?" errors for old commands

When user types `nika provider set X` or `nika vault set X`, clap will show the standard error because Set/Delete no longer exist. We want a SMART error instead.

**Approach:** Add a hidden `#[command(hide = true)]` fallback variant to ProviderAction:

```rust
#[derive(Subcommand)]
pub enum ProviderAction {
    List,
    Test { provider: String, #[arg(short, long)] quiet: bool },

    /// Moved to nika keys set
    #[command(hide = true)]
    Set {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },
    /// Moved to nika keys remove
    #[command(hide = true)]
    Delete {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },
    /// Moved to nika keys
    #[command(hide = true, name = "vault-reset")]
    VaultReset,
    /// Moved to nika keys
    #[command(hide = true)]
    Migrate,
    /// Moved to nika keys
    #[command(hide = true)]
    Get {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },
}
```

And in the handler:
```rust
ProviderAction::Set { .. } | ProviderAction::Delete { .. } |
ProviderAction::Get { .. } | ProviderAction::Migrate |
ProviderAction::VaultReset => {
    eprintln!("  {} Did you mean? {}", "✗".red().bold(), "nika keys set <name>".cyan());
    eprintln!("  Key management moved to: nika keys");
    std::process::exit(1);
}
```

Similarly for `nika vault *` — but vault subcommand is already removed from main.rs. If a user types `nika vault`, clap will show "error: unrecognized subcommand 'vault'". We could add a top-level hidden `Vault` command that shows the message:

```rust
// In Commands enum
/// (moved to nika keys)
#[command(hide = true)]
Vault {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    _args: Vec<String>,
},
```

**Test:** `nika provider set anthropic 2>&1 | grep "Did you mean"` (E2E test #7 from handoff)

**Commit:** `feat(cli): add did-you-mean errors for old provider/vault commands`

---

### Task 3: Real connection test in `keys check`

**File:** `tools/nika-cli/src/keys.rs`, function `test_provider_connection()`

Currently stubbed — only checks if env var exists. Need to make an actual API call.

**Use existing:** `provider::run_provider_test()` from `tools/nika-cli/src/provider.rs:402`.
This function already exists and works (creates a RigProvider, sends a minimal completion request).

**Steps:**
1. Make `run_provider_test` public in provider.rs
2. Import it in keys.rs
3. Replace the stub in `test_provider_connection()`:
```rust
async fn test_provider_connection(provider: &Provider) -> Result<u64, String> {
    let start = std::time::Instant::now();
    crate::provider::run_provider_test(provider.id).await?;
    Ok(start.elapsed().as_millis() as u64)
}
```

**Commit:** `feat(cli): keys check uses real provider connection test`

---

### Task 4: `keys set` env detection (UX Helper #9)

**File:** `tools/nika-cli/src/keys.rs`, function `set_known_provider()`

When the user runs `nika keys set openai` and `OPENAI_API_KEY` already exists in the environment, show:

```
  💡 Found OPENAI_API_KEY in environment: sk-••••a3b9
  Save to vault for persistence? (Y/n)
```

If yes, read from env and save to vault (no need to re-enter).
If no, skip.

**Insert after line ~1010 (after `let key_value: String;` block):**
```rust
// UX #9: detect existing env var
if is_tty && !stdin {
    if let Ok(env_val) = std::env::var(provider.env_var) {
        if !env_val.is_empty() {
            let masked = mask_key_pretty(&env_val);
            eprintln!(
                "  {} Found {} in environment: {}",
                "\u{1F4A1}".dimmed(),
                provider.env_var.bold(),
                masked.dimmed()
            );
            let save = cliclack::confirm("Save to vault for persistence?")
                .initial_value(true)
                .interact()
                .map_err(io_err)?;
            if save {
                key_value = env_val;
                // Skip the password prompt below
                // (need to restructure the flow)
            }
        }
    }
}
```

This requires restructuring the interactive flow to check env BEFORE prompting for input.

**Commit:** `feat(cli): keys set detects existing env var — UX helper #9`

---

### Task 5: Delete `vault.rs` module (zero dead code)

**Files:**
- `tools/nika-cli/src/vault.rs` — DELETE entirely (693 lines)
- `tools/nika-cli/src/lib.rs` — Remove `pub mod vault;` (line 45)

**Pre-check:** Verify no other module imports from vault.rs:
```bash
grep -r "crate::vault\|super::vault\|cli::vault\|use.*vault::" tools/nika-cli/src/ --include="*.rs"
# Result: only vault.rs itself references NikaVault from nika-vault crate
```

**Note:** `nika_vault::NikaVault` (the CRATE) stays — it's used by keys.rs, provider.rs, onboarding.rs, engine. Only the CLI MODULE is deleted.

**Commit:** `refactor(cli): delete vault.rs — vault commands replaced by nika keys`

---

### Task 6: Fix 4 flaky `secrets::tests` 

**File:** `tools/nika-engine/src/secrets/mod.rs`

4 tests fail when run in full workspace but pass individually:
- `test_doppler_fallback_to_local_on_error`
- `test_get_secret_returns_env_var_value`
- `test_secrets_fallback_clear_error_when_no_source`
- `test_secrets_from_env_without_daemon`

**Root cause:** All 4 tests already have `#[serial]` but they manipulate `ANTHROPIC_API_KEY`, `NIKA_HOME`, `NIKA_NO_DAEMON` env vars. In workspace-wide runs, OTHER crates' tests also read these vars concurrently.

**Fix options:**
1. **Best:** Use `temp_env` crate's `with_vars_unset` for atomic env manipulation
2. **Simple:** Add `#[serial]` to the test MODULE level (not just individual tests) — but `serial_test` doesn't support module-level serial across crates
3. **Pragmatic:** Use unique env var names per test (e.g. `NIKA_TEST_ANTHROPIC_KEY_42`) to avoid collisions

**Recommended:** Option 3. Create a test helper:
```rust
fn test_env_var(provider: &str) -> String {
    format!("NIKA_TEST_{}_{}", provider.to_uppercase(), std::process::id())
}
```

But this requires changing `get_secret()` to accept custom env var names, which is a bigger refactor.

**Alternative pragmatic fix:** Accept these are integration tests and add a comment:
```rust
// NOTE: These tests manipulate global env vars and may fail when run
// alongside other crates' tests. Run in isolation:
// cargo test -p nika-engine --lib -- secrets::tests
```

**Commit:** `fix(test): document env var isolation in secrets tests`

---

### Task 7: Update remaining "vault" references in error messages

**Files with stale references:**
```
nika-engine/src/secrets/fallback.rs:125  — "nika vault set custom:..."  → "nika keys set ..."
nika-vault/src/lib.rs:549               — "nika vault reset"            → remove or keep (vault crate)
nika-cli/src/vault.rs:153               — deleted in Task 5
```

Only `fallback.rs:125` needs updating. The vault crate reference is about the internal vault mechanism, not the CLI command.

**Commit:** `fix(docs): update stale vault references to nika keys`

---

### Task 8: Update docs — CLAUDE.md, AGENTS.md, README

**Files to update:**
- `CLAUDE.md` — Replace all `nika provider set` with `nika keys set` in command reference
- `AGENTS.md` — Same
- `README.md` — Same (if command examples exist)
- `tools/nika/CLAUDE.md` — Already updated (verified)

**Verify:** `grep -r "provider set\|provider delete\|vault set\|vault list" *.md docs/ --include="*.md"`

**Commit:** `docs: update command references — nika keys replaces provider/vault`

---

## Files Summary

| File | Action | LOC |
|------|--------|-----|
| `tools/nika-cli/src/onboarding.rs` | EDIT (wire keys set) | ~30 |
| `tools/nika-cli/src/provider.rs` | EDIT (hidden did-you-mean variants) | +30 |
| `tools/nika-cli/src/keys.rs` | EDIT (real test, env detection) | +40 |
| `tools/nika-cli/src/vault.rs` | DELETE | -693 |
| `tools/nika-cli/src/lib.rs` | EDIT (remove vault mod) | -1 |
| `tools/nika/src/main.rs` | EDIT (hidden Vault command) | +10 |
| `tools/nika-engine/src/secrets/mod.rs` | EDIT (test docs) | +5 |
| `tools/nika-engine/src/secrets/fallback.rs` | EDIT (1 string) | +1 |
| `CLAUDE.md`, `AGENTS.md` | EDIT (command references) | ~10 |

**Net: -600 LOC** (mostly vault.rs deletion)

---

## Rules (ABSOLUTE)

### Co-author
```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
NEVER Claude. NEVER Anthropic.

### Tests
```bash
cargo test -p nika-cli --lib -- keys     # keys tests (34)
cargo test -p nika-cli --lib             # full cli crate
cargo test --workspace --lib             # ALWAYS --lib (no keychain popups)
cargo clippy --workspace -- -D warnings  # zero warnings
```

### v0 philosophy
- Zero backward compat, zero aliases, zero dead code
- BUT: smart UX (did you mean, typo, guidance) = ALWAYS

### Do NOT touch
- `nika-vault` crate (the crypto library) — stays
- `nika-engine/src/secrets/` — stays (runtime secret resolution)
- Stashes — there are NONE (all dropped)
- Worktrees — there are NONE

---

## Verification Checklist

- [ ] `cargo test --workspace --lib` — all pass (4 secrets flaky = known, document)
- [ ] `cargo clippy --workspace -- -D warnings` — clean
- [ ] `nika keys` shows categorized display
- [ ] `nika keys set anthropic` interactive flow works
- [ ] `nika keys check` makes real API calls with latency
- [ ] `nika keys sync` pushes to GitHub
- [ ] `nika provider set` → "Did you mean? nika keys set"
- [ ] `nika vault` → "Did you mean? nika keys"
- [ ] `nika setup` calls keys set for each provider
- [ ] vault.rs deleted, no compilation errors
- [ ] `grep -r "nika vault set\|nika provider set" tools/` — zero hits
- [ ] All 15 UX helpers from SESSION-KEYS-HANDOFF.md work

## Git State

```
Branch: main (up to date with origin)
Stashes: 0
Worktrees: 0
Last commit: b2a0cf7e0 feat(cli): implement nika keys — unified API key management
PR #106: needs manual merge on GitHub (dependabot, workflow scope)
```

## Commit Plan (8 commits)

```
feat(cli): wire nika setup to use keys set
feat(cli): add did-you-mean errors for old provider/vault commands
feat(cli): keys check uses real provider connection test
feat(cli): keys set detects existing env var — UX helper #9
refactor(cli): delete vault.rs — vault commands replaced by nika keys
fix(test): document env var isolation in secrets tests
fix(docs): update stale vault references to nika keys
docs: update command references — nika keys replaces provider/vault
```
