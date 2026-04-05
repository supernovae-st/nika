# nika keys — Implementation Plan

> v0 philosophy: zero backward compat, zero aliases, zero dead code.
> `provider set/delete` → REMOVED. `keys` replaces everything.

## Architecture

```
nika keys set anthropic        ← smart: detects provider, validates, shows models
nika keys set MY_CUSTOM_KEY    ← generic: name + value
nika keys list                 ← tree display: LLM / OpenAI-compat / Custom / Always Available
nika keys remove anthropic     ← delete from vault
nika keys check                ← test all configured keys with latency
nika keys sync --github        ← push to GitHub Actions secrets via gh CLI

nika provider list             ← READ-ONLY: catalog, models, pricing (stays)
nika provider test anthropic   ← validate key works (stays)
nika provider recommend        ← suggest model (stays)
```

## Phase 1: Create `keys.rs` + clap structure

### Task 1.1: Add `KeysAction` enum to `nika-cli/src/keys.rs`

```rust
// tools/nika-cli/src/keys.rs

use clap::Subcommand;

#[derive(Subcommand)]
pub enum KeysAction {
    /// Store an API key (smart for known providers, generic for custom)
    Set {
        /// Provider name or custom key name
        name: String,
        /// API key value (use --stdin for automation)
        #[arg(hide = true)]
        key: Option<String>,
        /// Read key from stdin (safe for CI/scripts)
        #[arg(long)]
        stdin: bool,
        /// Read key from environment variable
        #[arg(long)]
        key_env: Option<String>,
        /// Skip connection test after setting
        #[arg(long)]
        no_test: bool,
        /// Skip GitHub sync prompt
        #[arg(long)]
        no_sync: bool,
    },

    /// Show all configured keys with source provenance
    #[command(alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Show all details (env var names, last test, vault info)
        #[arg(long, short)]
        verbose: bool,
    },

    /// Remove a key from vault
    #[command(alias = "rm")]
    Remove {
        /// Provider name or custom key name
        name: String,
    },

    /// Test all configured keys (connection + latency)
    Check {
        /// Test only this provider
        provider: Option<String>,
        /// Suppress output, exit code only
        #[arg(long, short)]
        quiet: bool,
    },

    /// Sync keys to external services
    Sync {
        /// Push to GitHub Actions secrets
        #[arg(long)]
        github: bool,
        /// Target repository (default: auto-detect from git remote)
        #[arg(long)]
        repo: Option<String>,
        /// Preview without pushing
        #[arg(long)]
        dry_run: bool,
    },

    /// Import keys from env vars, .env file, or Doppler
    Import {
        /// Source: env, dotenv, doppler
        #[arg(long, default_value = "env")]
        from: String,
        /// Path to .env file (for --from dotenv)
        #[arg(long)]
        file: Option<String>,
    },
}
```

### Task 1.2: Wire into main.rs

In `tools/nika/src/main.rs`, add `Keys` variant to `Commands` enum:

```rust
/// Manage API keys and secrets
#[command(visible_alias = "k")]
Keys {
    #[command(subcommand)]
    action: cli::keys::KeysAction,
},
```

And in the match dispatch:

```rust
Commands::Keys { action } => cli::keys::handle_keys_command(action).await,
```

### Task 1.3: Add module to lib.rs

```rust
// tools/nika-cli/src/lib.rs
pub mod keys;
```

### Task 1.4: Tests

```rust
#[cfg(test)]
mod tests {
    // Test classify_name() detects known providers
    // Test classify_name() handles aliases (claude → anthropic)
    // Test classify_name() treats unknown names as custom
    // Test mask_key() format (prefix + last 4)
    // Test env_var_for_name() mapping
}
```

**Verify**: `cargo test -p nika-cli --lib`

---

## Phase 2: `keys set` implementation

### Task 2.1: Smart provider detection

```rust
fn classify_name(name: &str) -> KeyKind {
    if let Some(provider) = find_provider(name) {
        KeyKind::Provider(provider)
    } else {
        KeyKind::Custom(name.to_uppercase())
    }
}

enum KeyKind {
    Provider(&'static Provider),  // Known LLM/MCP provider
    Custom(String),                // Generic key name
}
```

Uses `find_provider()` from `nika-core::catalogs::providers` which handles aliases (claude → anthropic, gpt → openai).

### Task 2.2: Interactive set flow

For known providers:
1. Show console URL: `"Get your key at https://console.anthropic.com/settings/keys"`
2. Read key via `cliclack::password()` (or --stdin/--key-env)
3. Validate prefix: `validate_key_format(provider, &key)`
4. Store in vault: `vault.set(provider.id, &key)`
5. Inject to env: `inject_secret_to_env(provider.env_var, &key)`
6. Show models: `"✓ Saved · Claude Sonnet 4, Haiku available"`
7. Offer test: `"Test connection? (Y/n)"`
8. Offer sync: `"Sync to GitHub CI? (Y/n)"`

For custom keys:
1. Read value via `cliclack::password()`
2. Store with `custom:` prefix: `vault.set(&format!("custom:{name}"), &value)`
3. Show confirmation: `"✓ Saved"`

### Task 2.3: Console URL registry

```rust
const CONSOLE_URLS: &[(&str, &str)] = &[
    ("anthropic", "https://console.anthropic.com/settings/keys"),
    ("openai", "https://platform.openai.com/api-keys"),
    ("gemini", "https://aistudio.google.com/apikey"),
    ("groq", "https://console.groq.com/keys"),
    ("mistral", "https://console.mistral.ai/api-keys"),
    ("deepseek", "https://platform.deepseek.com/api_keys"),
    ("xai", "https://console.x.ai/"),
    ("openrouter", "https://openrouter.ai/settings/keys"),
    ("together", "https://api.together.xyz/settings/api-keys"),
    ("fireworks", "https://fireworks.ai/api-keys"),
    ("cerebras", "https://cloud.cerebras.ai/platform"),
    ("cohere", "https://dashboard.cohere.com/api-keys"),
];
```

### Task 2.4: Tests

```rust
// Test provider set stores in vault correctly
// Test custom key set uses custom: prefix
// Test --stdin reads from pipe
// Test --key-env reads from env
// Test prefix validation rejects wrong format
// Test alias resolution (claude → anthropic)
```

**Verify**: `cargo test -p nika-cli --lib`

---

## Phase 3: `keys list` implementation (tree display)

### Task 3.1: Gather all keys from all sources

```rust
struct ResolvedKey {
    name: String,
    kind: KeyKind,
    source: KeySource,
    masked_value: Option<String>,  // sk-ant-••••7f2k
    models: Vec<String>,            // For LLM providers only
}

enum KeySource {
    Vault,    // NikaVault encrypted file
    Env,      // Environment variable (ephemeral)
    Daemon,   // Daemon IPC cache
    Config,   // config.toml custom endpoint
    None,     // Not configured
}
```

Resolution order (matches existing engine behavior):
1. Check env var: `std::env::var(provider.env_var)`
2. Check daemon: `DaemonClient::has_secret()` (Unix only)
3. Check vault: `vault.get(name)`
4. Custom keys: `vault.get(&format!("custom:{name}"))`

### Task 3.2: Tree display renderer

```
  🔑 Keys                                          5 of 7 configured

  LLM Providers
  ├── ✓ anthropic     sk-ant-••••7f2k    vault     Claude Sonnet 4, Haiku
  ├── ✓ openai        sk-••••a3b9        env       GPT-4.1, o4-mini
  │                                       ⚠ env only — run: nika keys set openai
  ├── ✓ groq          gsk_••••mN3p       vault     Llama 3.3-70b
  ├── ✗ deepseek                                    nika keys set deepseek
  ├── ✓ gemini        AIza••••wQ7x       vault     Gemini 2.5 Pro
  └── ✗ xai                                         nika keys set xai

  OpenAI-Compatible
  ├── ✓ openrouter    sk-or-••••         vault     Any model via gateway
  └── ✓ cerebras      csk-••••           vault     Llama 70B (fastest)

  Custom
  └── ✓ ELEVENLABS    ••••••••           vault

  Always Available
  ├── ✓ mock          (no key needed)              deterministic responses
  └── ○ native        (no model loaded)            nika model pull <name>

  💡 nika keys set <name>  ·  nika keys check  ·  nika keys sync --github
```

Use existing display primitives:
- `tree_connector(is_last)` from `nika_display`
- `StatusIcon::Ok` / `StatusIcon::Fail` for ✓/✗
- `hint()` for dimmed suggestions
- `separator()` for section dividers

### Task 3.3: Empty state (zero keys)

```
  🔑 Keys

  No keys configured yet. Get started in 30 seconds:

  1. Pick a provider:
     nika keys set anthropic     Best quality (Claude Sonnet 4)
     nika keys set groq          Free tier, no credit card needed

  2. Paste your API key when prompted

  3. Run your first workflow:
     nika run hello.nika.yaml

  💡 nika setup   Full interactive wizard
     nika keys set <name>   Add a single key
```

### Task 3.4: `--json` output

```json
{
  "providers": [
    {
      "name": "anthropic",
      "status": "configured",
      "source": "vault",
      "key_preview": "sk-ant-••••7f2k",
      "env_var": "ANTHROPIC_API_KEY",
      "models": ["claude-sonnet-4-6", "claude-haiku-4-5"]
    }
  ],
  "custom": [
    { "name": "ELEVENLABS", "status": "configured", "source": "vault" }
  ],
  "summary": { "configured": 5, "total": 7 }
}
```

### Task 3.5: Tests

```rust
// Test tree display with mix of configured/unconfigured
// Test empty state rendering
// Test --json output structure
// Test env-only warning appears
// Test custom keys appear in separate section
// Test models shown for LLM providers
```

**Verify**: `cargo test -p nika-cli --lib`

---

## Phase 4: `keys remove`, `keys check`, `keys sync`

### Task 4.1: `keys remove`

Simple: delete from vault + daemon.
```rust
vault.delete(&name)?;
// Also try daemon
#[cfg(unix)]
if let Ok(client) = DaemonClient::new(&sock) {
    let _ = client.delete_secret(&name).await;
}
```

### Task 4.2: `keys check` — test all keys

For each configured key:
1. Resolve the key value
2. For LLM providers: run a quick inference test with timing
3. For MCP providers: check connection
4. Display results:

```
  🔑 Keys Check

  ✓ anthropic     247ms    Claude Sonnet 4 responded
  ✓ openai        312ms    GPT-4.1 responded
  ✗ groq          timeout  connection timed out after 10s
  ✓ gemini        189ms    Gemini 2.5 Pro responded
  ─ mock          0ms      deterministic (no network)

  4/5 passed · 1 failed
```

### Task 4.3: `keys sync --github`

```rust
fn sync_to_github(repo: &str, keys: &[ResolvedKey], dry_run: bool) -> Result<()> {
    // 1. Check `gh` CLI is installed
    // 2. Check `gh auth status` is authenticated
    // 3. For each configured key:
    //    - Map name to env var (anthropic → ANTHROPIC_API_KEY, custom:X → X)
    //    - Check if already set on GitHub: `gh secret list --repo X`
    //    - Show diff: "3 new, 1 updated, 2 already synced"
    // 4. If not dry_run, push each:
    //    - echo "$value" | gh secret set ENV_VAR --repo X
    //    - NEVER pass value in process args (visible in ps)
}
```

Auto-detect repo:
```rust
fn detect_github_repo() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output().ok()?;
    // Parse: git@github.com:owner/repo.git → owner/repo
    // Parse: https://github.com/owner/repo.git → owner/repo
}
```

### Task 4.4: Tests

```rust
// Test remove deletes from vault
// Test check reports pass/fail per provider
// Test sync --dry-run shows preview without pushing
// Test sync detects repo from git remote
// Test sync maps provider names to env vars
```

**Verify**: `cargo test -p nika-cli --lib`

---

## Phase 5: Remove `provider set/delete` (v0 = no backward compat)

### Task 5.1: Strip write operations from provider.rs

Remove from `ProviderAction` enum:
- `Set { ... }` — DELETED
- `Delete { ... }` — DELETED
- `Migrate` — DELETED (replaced by `keys import`)
- `VaultReset` — move to `keys reset` or delete

Keep:
- `List` — read-only catalog view (models, pricing)
- `Get { provider }` — read-only masked key display
- `Test { provider, quiet }` — connection test

### Task 5.2: Move shared helpers to keys.rs

Move from provider.rs to keys.rs:
- `get_vault()` → `keys::get_vault()`
- `has_key_env_or_vault()` → `keys::resolve_key()`
- `mask_api_key()` → `keys::mask_key()`
- `KEY_PREFIXES` → keep in providers catalog
- `inject_secret_to_env()` → `keys::inject_to_env()`

### Task 5.3: Update provider.rs to use keys.rs helpers

`provider list` now calls `keys::resolve_key()` for status.
`provider test` now calls `keys::resolve_key()` for the key value.

### Task 5.4: Delete dead code

Remove all set/delete/migrate logic from provider.rs.
Remove unused imports, functions, constants.

**Verify**: `cargo test --workspace --lib` — all tests pass, zero warnings

---

## Phase 6: `nika setup` integration

### Task 6.1: Setup wizard calls keys

```rust
// In nika-init/src/wizard.rs or onboarding.rs

// Step: Provider setup
let providers = cliclack::multiselect("Which providers?")
    .items(&[("anthropic", "Anthropic Claude"), ("openai", "OpenAI"), ...])
    .interact()?;

for provider in providers {
    keys::set_key(provider, SetOptions::interactive()).await?;
}

// Step: GitHub sync
if let Some(repo) = keys::detect_github_repo() {
    if cliclack::confirm("Sync keys to GitHub CI?").interact()? {
        keys::sync_to_github(&repo, &configured_keys, false)?;
    }
}
```

### Task 6.2: Just-in-time setup in `nika run`

When a workflow needs a provider key that's missing:
```rust
// In nika-engine runtime, before provider init
if !has_key_for_provider(provider_name) {
    eprintln!("  ⚠ {provider_name} needs an API key");
    eprintln!("  run: nika keys set {provider_name}");
    return Err(NikaError::ProviderNotConfigured { ... });
}
```

No inline setup during run — just a clear error with the command to fix it.

---

## File Summary

| File | Action | LOC Change |
|------|--------|------------|
| `tools/nika-cli/src/keys.rs` | **CREATE** | +400 |
| `tools/nika-cli/src/lib.rs` | EDIT | +1 |
| `tools/nika/src/main.rs` | EDIT | +5, -2 |
| `tools/nika-cli/src/provider.rs` | EDIT (strip set/delete) | -250 |
| `tools/nika-init/src/wizard.rs` | EDIT (call keys) | +20 |

**Net**: ~+175 LOC (add 400, remove 225 dead code)

## Test Plan

| Phase | Tests | Command |
|-------|-------|---------|
| 1 | 5 (classify, mask, map) | `cargo test -p nika-cli --lib -- keys` |
| 2 | 6 (set, stdin, env, prefix) | `cargo test -p nika-cli --lib -- keys` |
| 3 | 6 (list, empty, json, tree) | `cargo test -p nika-cli --lib -- keys` |
| 4 | 5 (remove, check, sync) | `cargo test -p nika-cli --lib -- keys` |
| 5 | 0 (existing tests must pass) | `cargo test --workspace --lib` |
| 6 | 2 (setup, just-in-time) | `cargo test -p nika-init --lib` |
| **Total** | **24 new tests** | |

## Execution Order

```
Phase 1 → Phase 2 → Phase 3 → Phase 5 → Phase 4 → Phase 6
(struct)   (set)     (list)    (remove    (sync)    (setup)
                               provider)
```

Phase 5 (remove provider set) comes AFTER list works, so we can verify the display is correct before removing the old code.

## Commit Plan (1 fix = 1 commit)

```
feat(cli): add nika keys command skeleton with clap structure
feat(cli): implement nika keys set — smart provider detection + custom keys
feat(cli): implement nika keys list — tree display with source provenance
refactor(cli): remove provider set/delete — v0 no backward compat
feat(cli): implement nika keys remove + check (test all keys)
feat(cli): implement nika keys sync --github
feat(cli): wire nika setup to use keys instead of provider
```
