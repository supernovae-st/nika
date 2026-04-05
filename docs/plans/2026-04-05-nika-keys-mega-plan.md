# nika keys — Mega Implementation Plan

> v0 philosophy: zero backward compat, zero dead code.
> BUT: smart UX everywhere — did you mean, typo correction, alias resolution, guidance.
> 3 P0 bugs (DONE), then 7-phase implementation.

## Design Decisions (validated)

- **Name**: `keys` (research: 30/35 pts, precedent: Simon Willison's `llm keys`)
- **Commands**: 5 (bare list, set, remove, check, sync)
- **Categories**: 4 (🧠 Inference, 🔍 Search, 🔧 Custom, ◎ Local) — hidden if empty
- **Icons**: ● configured, · not set, ◎ system, ○ offline, ⚠ env-only
- **Source provenance**: vault (green) / env (yellow+warning) / daemon (cyan)
- **v0 philosophy**: `provider set/delete/get/migrate` and `vault set/get/delete/list` DELETED
- **UX helpers**: did-you-mean, typo correction, alias resolution, interactive picker, env→vault nudge

## Smart UX Moments (15 total)

| # | Trigger | Response |
|---|---------|----------|
| 1 | `nika provider set X` | ✗ "Did you mean? `nika keys set X`" |
| 2 | `nika vault set X` | ✗ "Did you mean? `nika keys set X`" |
| 3 | `nika keys set claude` | ✓ Auto-resolve alias: claude → anthropic |
| 4 | `nika keys set antrhopic` | 💡 "Did you mean? anthropic" (Levenshtein) |
| 5 | `nika keys set ANTHROPIC_API_KEY` | 💡 "Did you mean? `nika keys set anthropic`" |
| 6 | `nika keys set sk-ant-abc123` | 💡 "That's a key, not a name → anthropic" |
| 7 | `nika keys set` (no name) | 📋 Interactive picker (cliclack::select) |
| 8 | `nika keys set anthropic` (exists) | ⚠ "Update? Current: sk-ant-••••7f2k" |
| 9 | `nika keys set openai` (env exists) | 💡 "Found in env. Save to vault? (Y/n)" |
| 10 | `nika keys remove` (no name) | 📋 Pick from configured keys |
| 11 | `nika keys check` (zero keys) | 💡 "No keys. Run: nika keys set anthropic" |
| 12 | `nika keys sync` (no git remote) | 💡 "Not in a git repo. Use --repo owner/name" |
| 13 | `nika keys sync` (no gh CLI) | 💡 "Install: brew install gh" |
| 14 | `nika run` (missing key) | 💡 Fix command + configured alternatives |
| 15 | Wrong prefix (sk- for anthropic) | 💡 "Looks like OpenAI → nika keys set openai" |

## Pre-requisites: Fix 3 P0 Bugs

### P0-1: Custom vault keys are dead data at runtime
**File**: `tools/nika-engine/src/secrets/mod.rs` (or wherever `load_from_daemon_or_fallback` lives)
**Bug**: Boot loader only iterates `KNOWN_PROVIDERS`. Custom keys stored via vault are never injected into `SecretStore`.
**Fix**: After iterating providers, also iterate `vault.list()` and inject any `custom:*` entries.
```rust
// After provider loop, inject custom vault keys
if let Ok(keys) = vault.list() {
    for key in keys.iter().filter(|k| k.starts_with("custom:")) {
        let env_name = key.strip_prefix("custom:").unwrap();
        if let Ok(Some(secret)) = vault.get(key) {
            inject_secret_to_env(env_name, secret.expose_secret());
        }
    }
}
```
**Test**: Set custom key via vault, verify `$env.CUSTOM_KEY` resolves in workflow.
**Commit**: `fix(engine): inject custom vault keys into SecretStore at boot`

### P0-2: $vault bindings broken in production
**File**: `tools/nika-engine/src/runtime/runner.rs`
**Bug**: `run_context.set_vault()` is called in tests but never in production code.
**Fix**: Add vault initialization in `Runner::run()` before workflow execution.
```rust
// In Runner::run(), after creating RunContext
if let Some(vault) = NikaVault::try_load() {
    run_context.set_vault(vault);
}
```
**Test**: Workflow with `$vault.test.field` binding resolves correctly.
**Commit**: `fix(runtime): wire vault into RunContext for $vault bindings`

### P0-3: Atomic vault writes
**File**: `tools/nika-vault/src/lib.rs`
**Bug**: `std::fs::write()` directly to `vault.enc` — concurrent read can see truncated file.
**Fix**: Write to `.enc.tmp` then `std::fs::rename()` (atomic on POSIX).
```rust
let tmp_path = self.vault_path.with_extension("enc.tmp");
std::fs::write(&tmp_path, &encrypted)?;
std::fs::rename(&tmp_path, &self.vault_path)?;
```
**Test**: Concurrent read/write doesn't produce errors.
**Commit**: `fix(vault): atomic write via rename to prevent truncated reads`

---

## Phase 1: Create `keys.rs` skeleton + clap (1 commit)

### Files
| File | Action |
|------|--------|
| `tools/nika-cli/src/keys.rs` | CREATE (~50 LOC skeleton) |
| `tools/nika-cli/src/lib.rs` | ADD `pub mod keys` |
| `tools/nika/src/main.rs` | ADD `Keys` variant to `Commands` enum |

### Clap Structure
```rust
#[derive(Subcommand)]
pub enum KeysAction {
    /// Store an API key
    Set { name: String, key: Option<String>, #[arg(long)] stdin: bool,
          #[arg(long)] key_env: Option<String>, #[arg(long)] no_test: bool },
    /// Show all configured keys
    #[command(alias = "ls")]
    List { #[arg(long)] json: bool, #[arg(long, short)] verbose: bool,
           #[arg(long)] all: bool },
    /// Remove a key
    #[command(alias = "rm")]
    Remove { name: String },
    /// Test all configured keys
    Check { provider: Option<String>, #[arg(long, short)] quiet: bool },
    /// Sync keys to GitHub Actions
    Sync { #[arg(long)] github: bool, #[arg(long)] repo: Option<String>,
           #[arg(long)] dry_run: bool },
}
```

`nika keys` (bare) → defaults to `KeysAction::List` (not help).
`nika keys set` (no name) → interactive picker via cliclack::select.
`nika k` → visible alias for `nika keys`.

### Tests (5)
- classify_name("anthropic") → Provider
- classify_name("claude") → Provider (alias)
- classify_name("MY_CUSTOM") → Custom
- mask_key("sk-ant-abc123xyz") → "sk-ant-••••3xyz"
- env_var_for("anthropic") → "ANTHROPIC_API_KEY"

**Verify**: `cargo test -p nika-cli --lib -- keys`
**Commit**: `feat(cli): add nika keys command skeleton with clap structure`

---

## Phase 2: `keys set` — smart provider detection (1 commit)

### Smart Detection
```rust
fn classify_name(name: &str) -> KeyKind {
    // 1. Try known provider (handles aliases: claude→anthropic, gpt→openai)
    if let Some(provider) = find_provider(name) {
        return KeyKind::Provider(provider);
    }
    // 2. Try detecting from key prefix (if user pasted key as name by mistake)
    // 3. Default to custom
    KeyKind::Custom(name.to_uppercase())
}
```

### Interactive Flow (cliclack)
```
Provider:
  cliclack::intro("nika keys set — anthropic")
  cliclack::note("Get your key", "https://console.anthropic.com/settings/keys")
  key = cliclack::password("ANTHROPIC_API_KEY")
  validate prefix (sk-ant-)
  cliclack::spinner() → vault.set("anthropic", &key)
  cliclack::note("Models available", "Claude Sonnet 4, Claude Haiku")
  cliclack::confirm("Test connection?") → test with latency
  cliclack::confirm("Sync to GitHub CI?") → keys sync
  cliclack::outro("✓ anthropic configured")

Custom:
  cliclack::intro("nika keys set — MY_CUSTOM_KEY")
  key = cliclack::password("Value for MY_CUSTOM_KEY")
  vault.set("custom:MY_CUSTOM_KEY", &key)
  cliclack::outro("✓ MY_CUSTOM_KEY saved")
```

### Console URL Registry
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
    ("ai21", "https://studio.ai21.com/account/api-key"),
    ("sambanova", "https://cloud.sambanova.ai/apis"),
];
```

### Tests (6)
- set known provider stores in vault
- set custom key uses custom: prefix
- --stdin reads from pipe
- --key-env reads from env var
- prefix validation warns on mismatch
- alias resolution works (claude → anthropic)

**Commit**: `feat(cli): implement nika keys set — smart detection + interactive flow`

---

## Phase 3: `keys list` — the wow display (1 commit)

### Data Gathering
```rust
struct ResolvedKey {
    name: String,
    display_name: String,      // "anthropic" or "MY_CUSTOM_KEY"
    category: KeyCategory,     // Inference, Search, Data, Media, Tools, Services, Local
    status: KeyStatus,         // Configured, NotConfigured, EnvOnly, System, Offline
    source: KeySource,         // Vault, Env, Daemon, Config, Builtin, None
    masked_value: Option<String>,
    models: Vec<String>,       // For LLM providers
    env_var: Option<String>,   // ANTHROPIC_API_KEY
}

enum KeyCategory {
    Inference,  // 🧠 LLM providers (14)
    Search,     // 🔍 Perplexity, Firecrawl, Ahrefs, DataForSEO
    Data,       // 💾 Neo4j, Postgres
    Media,      // 🎨 Supadata, ElevenLabs (future)
    Tools,      // 🔧 GitHub, Slack (MCP)
    Services,   // 🌐 Catch-all third party
    Local,      // ◎ Mock, Native
}
```

### Resolution (per key)
```rust
fn resolve_key(name: &str, provider: Option<&Provider>) -> ResolvedKey {
    // 1. Check env var
    if let Some(env_var) = provider.map(|p| p.env_var) {
        if let Ok(val) = std::env::var(env_var) {
            return ResolvedKey { source: KeySource::Env, ... };
        }
    }
    // 2. Check daemon (Unix)
    #[cfg(unix)]
    if daemon_has_secret(name) { return ... }
    // 3. Check vault
    if let Ok(Some(val)) = vault.get(name) { return ... }
    // 4. Check custom vault
    if let Ok(Some(val)) = vault.get(&format!("custom:{name}")) { return ... }
    // 5. Not configured
    ResolvedKey { status: KeyStatus::NotConfigured, source: KeySource::None, ... }
}
```

### Display Format — see PREVIEW below

### MCP Visibility
Parse `.mcp.json` for `env:` fields to discover MCP key requirements:
```rust
fn discover_mcp_keys(project_root: &Path) -> Vec<McpKeyRequirement> {
    let mcp_path = project_root.join(".mcp.json");
    // Parse and extract env var requirements from MCP server configs
}
```

### Empty State
When zero keys configured, show welcoming onboarding (not error).

### --json output
```json
{
  "keys": [...],
  "summary": { "configured": 5, "total": 14 },
  "categories": { "inference": { "configured": 5, "total": 7 }, ... }
}
```

### Tests (8)
- tree display with mix of configured/unconfigured
- empty state shows onboarding
- --json output structure
- env-only warning appears
- custom keys in separate section
- models shown for LLM providers
- MCP keys discovered from .mcp.json
- --verbose shows full details

**Commit**: `feat(cli): implement nika keys list — tree display with source provenance`

---

## Phase 4: Remove `provider set/delete` + update error messages (1 commit)

### Remove from ProviderAction enum
- DELETE: `Set`, `Delete`, `Migrate`, `VaultReset`
- KEEP: `List` (read-only catalog), `Get`, `Test`

### Update 38 error message locations
Search and replace all "nika keys set" → "nika keys set" in:
- `tools/nika-engine/src/error.rs` (NIKA-032 help text)
- `tools/nika/src/main.rs` (help texts, onboarding)
- `tools/nika-cli/src/provider.rs` (remaining hints)
- `tools/nika-init/src/` (course templates, init wizard)
- All `.md` files referencing the old command

### Move shared helpers
- `get_vault()` → `keys::get_vault()`
- `mask_api_key()` → `keys::mask_key()`
- `inject_secret_to_env()` → `keys::inject_to_env()`
- `has_key_env_or_vault()` → `keys::resolve_key()`

### Tests
- Existing provider list tests still pass
- Existing provider test tests still pass
- Error messages mention "nika keys set"

**Commit**: `refactor(cli): remove provider set/delete — v0 no backward compat`

---

## Phase 5: `keys remove` + `keys check` (1 commit)

### keys remove
```rust
fn remove_key(name: &str) -> Result<()> {
    let vault = get_vault();
    // Try provider name first, then custom: prefix
    let deleted = vault.delete(name)? || vault.delete(&format!("custom:{name}"))?;
    // Also try daemon
    #[cfg(unix)]
    if let Ok(client) = DaemonClient::new(&sock) {
        let _ = client.delete_secret(name).await;
    }
    // Clear from in-process SecretStore
    SecretStore::remove(name);
}
```

### keys check
```rust
fn check_keys(provider: Option<&str>) -> Result<()> {
    let keys = gather_all_keys();
    let configured: Vec<_> = keys.iter().filter(|k| k.status.is_configured()).collect();

    for key in &configured {
        // Show spinner
        let start = Instant::now();
        match test_key_connection(&key.name).await {
            Ok(response) => {
                let latency = start.elapsed();
                println!("  ● {}     {}    {}", key.name.bold(), format_latency(latency).green(), response.dimmed());
            }
            Err(e) => {
                println!("  ✗ {}     {}    {}", key.name.bold(), "failed".red(), e.to_string().dimmed());
            }
        }
    }
    // Summary
    println!("\n  {}/{} passed", passed, configured.len());
}
```

### Tests (4)
- remove deletes from vault
- remove clears SecretStore
- check reports pass/fail per key
- check --quiet returns exit code only

**Commit**: `feat(cli): implement nika keys remove + check`

---

## Phase 6: `keys sync --github` (1 commit)

### Implementation
```rust
fn sync_to_github(repo: &str, dry_run: bool) -> Result<()> {
    // 1. Check gh CLI
    let gh_version = Command::new("gh").arg("--version").output()?;

    // 2. Check auth
    let auth = Command::new("gh").args(["auth", "status"]).output()?;

    // 3. Get current GitHub secrets
    let existing = Command::new("gh")
        .args(["secret", "list", "--repo", repo])
        .output()?;
    let existing_names: HashSet<String> = parse_secret_names(&existing.stdout);

    // 4. Gather vault keys
    let keys = gather_all_keys().into_iter()
        .filter(|k| k.status.is_configured() && k.source != KeySource::Builtin)
        .collect::<Vec<_>>();

    // 5. Show preview
    for key in &keys {
        let env_name = key.env_var.as_deref().unwrap_or(&key.name);
        let status = if existing_names.contains(env_name) { "=" } else { "+" };
        println!("  {} {}", status, env_name);
    }

    // 6. Confirm and push (values via stdin, NEVER in args)
    if !dry_run {
        for key in &keys {
            let env_name = key.env_var.as_deref().unwrap_or(&key.name);
            let value = vault.get(&key.name)?.unwrap();
            let mut child = Command::new("gh")
                .args(["secret", "set", env_name, "--repo", repo])
                .stdin(Stdio::piped())
                .spawn()?;
            child.stdin.take().unwrap().write_all(value.expose_secret().as_bytes())?;
            child.wait()?;
        }
    }
}

fn detect_github_repo() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"]).output().ok()?;
    let url = String::from_utf8_lossy(&output.stdout);
    // Parse git@github.com:owner/repo.git or https://github.com/owner/repo.git
    parse_github_repo(&url)
}
```

### Tests (3)
- sync --dry-run shows preview
- detect_github_repo parses SSH URL
- detect_github_repo parses HTTPS URL

**Commit**: `feat(cli): implement nika keys sync --github`

---

## Phase 7: `nika setup` integration (1 commit)

### Wire setup wizard to use keys
```rust
// In wizard flow
let providers = cliclack::multiselect("Which providers do you use?")
    .items(&provider_choices)
    .interact()?;

for name in &providers {
    handle_keys_set(name, SetOptions::interactive()).await?;
}

// Offer sync
if let Some(repo) = detect_github_repo() {
    if cliclack::confirm("Sync keys to GitHub CI?").interact()? {
        sync_to_github(&repo, false)?;
    }
}
```

### Update just-in-time error messages
When `nika run` fails due to missing key:
```
  ✗ Provider 'anthropic' needs an API key
    run: nika keys set anthropic
```

**Commit**: `feat(cli): wire nika setup to use keys, update error messages`

---

## Commit Summary

```
fix(engine): inject custom vault keys into SecretStore at boot          ← P0-1
fix(runtime): wire vault into RunContext for $vault bindings            ← P0-2
fix(vault): atomic write via rename to prevent truncated reads          ← P0-3
feat(cli): add nika keys command skeleton with clap structure           ← Phase 1
feat(cli): implement nika keys set — smart detection + interactive flow ← Phase 2
feat(cli): implement nika keys list — tree display with source provenance ← Phase 3
refactor(cli): remove provider set/delete — v0 no backward compat      ← Phase 4
feat(cli): implement nika keys remove + check                          ← Phase 5
feat(cli): implement nika keys sync --github                           ← Phase 6
feat(cli): wire nika setup to use keys, update error messages           ← Phase 7
```

**10 commits. ~600 LOC added, ~300 LOC removed. 26 tests.**
