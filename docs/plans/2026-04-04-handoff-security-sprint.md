# Handoff Prompt — Security & Stabilization Sprint

**Date**: 2026-04-04 | **From**: Mega-audit session (12 agents, 392K LOC analyzed)
**Version**: v0.68.0 (feature freeze) | **Target**: Pre-launch hardening

---

## Context

A 7-agent parallel audit found 1 CRITICAL + 4 HIGH + 12 MEDIUM issues across 18 crates.
A 5-agent review pass then corrected the plans — 2 findings were already fixed, 1 was
based on a factual error. This handoff contains the CORRECTED priorities.

**Already done this session** (do NOT redo):
- README footer: v0.68.0, 18 crates, 9,800+ tests
- README tool count: "61 Builtin Tools" at lines 305 and 493
- project-info.json: v0.68.0, 18 crates (at `dx/.claude/project-info.json`)
- PATH blocked in exec env vars (`runtime/security.rs:587`)
- FxHashMap in `dag/flow.rs` (replaced std::HashMap + fixed unwrap → if let)
- `display/header.rs` HashMap type aligned

**Remaining stale ref**: `README.md:121` still says `TOOLS["45+ Tools + MCP"]` in a Mermaid diagram — fix to `TOOLS["61 Tools + MCP"]`.

---

## CRITICAL — C-1: Replace `unsafe set_var` with Thread-Safe SecretStore

**Risk**: UB in `nika serve --embedded` multi-threaded mode. Data corruption + secret leak.

**File**: `tools/nika-engine/src/secrets/mod.rs:34-37`

**What to do (6 steps)**:

### Step 1: Create `tools/nika-engine/src/secrets/store.rs`

```rust
use std::sync::LazyLock;  // NOT once_cell — std since Rust 1.80
use dashmap::DashMap;
use secrecy::{ExposeSecret, SecretString};

static STORE: LazyLock<DashMap<String, SecretString>> = LazyLock::new(DashMap::new);

pub fn set_secret(key: &str, value: &str) {
    STORE.insert(key.to_string(), SecretString::from(value.to_string()));
}

pub fn get_secret(key: &str) -> Option<String> {
    STORE.get(key).map(|s| s.expose_secret().to_string())
}

pub fn has_secret(key: &str) -> bool {
    STORE.contains_key(key)
}

/// Store-first, env-var fallback. Primary lookup for $env.VAR bindings.
pub fn resolve_env(key: &str) -> Option<String> {
    if let Some(val) = get_secret(key) {
        return Some(val);
    }
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
pub fn clear() { STORE.clear(); }
```

### Step 2: Update `secrets/mod.rs`

Add `pub mod store;` to module declarations. Replace `inject_secret_to_env` body:
```rust
pub fn inject_secret_to_env(env_var: &str, value: &str) {
    store::set_secret(env_var, value);
}
```
Remove the `unsafe` block entirely.

### Step 3: Update reads in `secrets/fallback.rs`

**CRITICAL** — 3 `std::env::var` reads in this file bypass the store:
- Line 73: `std::env::var(env_var)` in `load_from_daemon_or_fallback()`
- Line 134: `std::env::var(env_var)` in `get_secret()`
- Line 184: `std::env::var(env_var)` in `has_secret()`

Replace all 3 with `super::store::resolve_env(env_var)`. The store falls back to
env vars internally, so behavior is preserved.

### Step 4: Update `$env.VAR` binding resolution

Search: `rg "std::env::var" tools/nika-engine/src/binding/`
Replace the env-binding path with `crate::secrets::store::resolve_env(name)`.

### Step 5: Update provider key checks

In `tools/nika-engine/src/provider/rig/mod.rs` — find `has_env_key()` or equivalent
that calls `std::env::var` for API key existence. Replace with `store::resolve_env()`.

### Step 6: Tests + verify

```bash
cargo test --workspace --lib -p nika-engine -- secrets
rg "unsafe.*set_var" tools/ --glob '*.rs' --glob '!*test*'  # must be 0
```

**Commit**: `fix(runtime): replace unsafe set_var with thread-safe SecretStore [C-1]`

**Note**: 5 CLI call sites in `nika-cli/` (verbs.rs:61,799, provider.rs:524,593,
onboarding.rs:205) also call `inject_secret_to_env` — they are transparently fixed
because the function body changes. No CLI code changes needed.

---

## HIGH — H-2: SSRF-Safe Client for FetchTool + TUI Chat

**TWO vulnerable clients** (not one — review found a second):
1. `tools/nika-engine/src/runtime/builtin/fetch_tool.rs:37-42` — `Policy::limited(10)`
2. `tools/nika-tui/src/chat_agent/mod.rs:142` — `Policy::limited(5)`

**Fix**: Extract shared SSRF-safe redirect policy. The existing `policy::is_ssrf_blocked`
function already does the IP check — reuse it in the redirect closure.

**IMPORTANT**: The shared builder MUST accept `allowed_hosts: &[String]` parameter to
avoid regressing explicitly-allowed private hosts. The initial URL check already exists
in FetchTool (line 121) — the gap is redirect targets only.

```bash
rg "redirect::Policy::limited" tools/ --glob '*.rs'  # must be 0 after fix
```

**Commit**: `fix(runtime): SSRF-safe redirect policy for FetchTool and TUI chat [H-2]`

---

## HIGH — H-1: YAML Size Limit (use serde-saphyr Budget)

**Better approach than plan originally proposed**: serde-saphyr v0.0.20+ has a built-in
`Budget` struct with 13 configurable fields including `max_reader_input_bytes`,
`max_aliases`, `max_anchors`, `max_depth`, `enforce_alias_anchor_ratio`.

Instead of a manual size check, use `serde_saphyr::from_str_with_options()` with budget:
```rust
let options = serde_saphyr::options! {
    budget: serde_saphyr::budget! {
        max_reader_input_bytes: Some(1_048_576),  // 1 MiB
        max_aliases: 1000,
        max_anchors: 100,
        max_depth: 64,
        enforce_alias_anchor_ratio: true,
    },
};
```

Apply to ALL 6+ `serde_yaml::from_str` call sites on file-loaded content:
- `ast/loader.rs:212` (skill/agent YAML)
- `ast/loader.rs:239` (markdown frontmatter)
- `ast/schema_validator.rs:69` (workflow YAML)
- `runtime/context_loader.rs:201` (context files)
- `core/mcp_config.rs:224` (MCP config)
- `runtime/resolver.rs:373` (included partials)
- `registry/operations.rs:157,251` (registry index/manifest)

**Commit**: `fix(ast): enforce serde-saphyr budget limits on all YAML parsing [H-1]`

---

## REMOVED FROM SPRINT (review corrections)

| ID | Original | Correction | Action |
|----|----------|-----------|--------|
| H-3 | Symlink escape in artifacts | Already fixed in `io/writer.rs` (SEC-3/4 checks) | WONTFIX |
| H-4 | Vault KDF too weak (64KB) | `1<<16` KiB = **64 MiB** not 64 KB. Already EXCEEDS OWASP. | WONTFIX |
| M-5 | PATH not blocked | Already done this session | DONE |

---

## MEDIUM fixes (after C-1, H-1, H-2)

**M-6**: Daemon `ListSecrets` auth — add `auth_token` field to `DaemonRequest::ListSecrets`
in `nika-daemon/src/protocol.rs:51`, validate in `server.rs:468`.

**M-7**: Vault zeroize — add `zeroize = { workspace = true, features = ["derive"] }` to
`nika-vault/Cargo.toml`. Derive `#[derive(Zeroize)] #[zeroize(drop)]` on `VaultPayload`.
Also zeroize the raw `plaintext` Vec in `read_payload()` immediately after deserialization.

**QA-1 (blocking I/O)**: Priority fix — `structured_output.rs:1137` uses `std::fs::read_to_string`
in an `async fn` hot path. Also `nika:inject` (data/io.rs:94,132). Use `tokio::fs`.

---

## Architecture Quick Wins (if time permits)

1. **ARCH-6**: `pub` → `pub(crate)` on runtime re-exports (`runtime/mod.rs`).
   Keep `validate_exec_command_with_shell` as `pub` (nika-tui uses it).
   Make `pub(crate)`: `check_blocklist`, `check_shell_data_injection`, `process_task_artifacts`,
   `validate_structured_output`, `LimitTracker`, `DynamicSubmitTool`, `PartialCheckpoint`.
   Also candidates for `pub(crate)`: `SpawnAgentTool`, `SkillInjector`, `make_task_result`,
   `WorkflowMeta`, `load_context`. Run `cargo check --workspace` to verify.

2. **ARCH-8**: Rename `engine/src/core/` → `engine/src/catalog/` (naming collision with nika-core crate).

3. **DOC-5**: Fill CHANGELOG v0.67 + v0.68 (`git log v0.66.0..v0.68.0 --oneline`).

---

## Key Facts (from review agents)

- **unwrap() in prod**: Only **33** (not 2,583 — plan had counting error including #[cfg(test)])
- **dead_code**: Only **54** real annotations (not 119 — 66 were in generated build artifacts)
- **orion KDF**: Uses Argon2**i** not Argon2id. Current params (6 iter, 64 MiB) already strong.
- **serde-saphyr**: Has built-in `Budget` struct — use it instead of manual size checks
- **ARCH-2 (NikaError)**: 28 orphan variants not covered by proposed domains. Realistic: 16-20h.
- **ARCH-3 (provider extraction)**: 10 internal deps (not 4). Needs McpClient, PolicyEnforcer.

---

## Invariants

```bash
cargo test --workspace --lib    # 9800+ tests, always --lib (no keychain)
cargo clippy --workspace -- -D warnings
```

- No new `unsafe` without review
- No new `pub` on `runtime/mod.rs`
- AGPL-3.0-or-later on all crates
- Commits: `type(scope): description` with both co-authors

---

## Plan Documents (full details with code blocks)

- `docs/plans/2026-04-04-plan1-security-critical-fixes.md` — Original plan (read with corrections above)
- `docs/plans/2026-04-04-plan2-architecture-decomposition.md` — ARCH-1..9
- `docs/plans/2026-04-04-plan3-code-quality-rust-idioms.md` — QA-1..10
- `docs/plans/2026-04-04-plan4-dx-documentation-sync.md` — DOC-1..9
- `docs/plans/2026-04-04-mega-audit-master-plan.md` — Master timeline
