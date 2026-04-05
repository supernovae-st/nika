# Security Sprint Bug Report — 2026-04-04

## Pre-existing bugs found during sprint

### BUG-PRE-1: nika-cli verbs tests fail when vault has real keys (10-13 tests)

**Files**: `tools/nika-cli/src/verbs.rs` (tests: `detect_provider_*`, `resolve_*`)

**Root cause**: Tests call `clear_provider_env()` which removes env vars, but `detect_provider()` also checks the NikaVault at `~/.nika/secrets/vault.enc`. On machines with stored vault keys (via `nika keys set`), the vault check finds the key and returns a provider even though the test expects `None`.

**Impact**: 10-13 tests fail on any developer machine with vault-stored API keys.

**Fix needed**: `clear_provider_env()` should either:
1. Set `NIKA_HOME` to a tempdir (isolating from real vault), or
2. Set `NIKA_NO_DAEMON=1` + override the vault path

**Severity**: LOW (tests only, no prod impact)

---

### BUG-PRE-2: SecretStore cross-test contamination in mod.rs tests

**File**: `tools/nika-engine/src/secrets/mod.rs`, test `test_has_secret_false_when_env_empty`

**Root cause**: The DashMap-based SecretStore is process-global (`LazyLock`). When tests run in sequence, values set by one test persist into the next. The `test_has_secret_false_when_env_empty` test sets `XAI_API_KEY=""` in env but didn't clear the store entry from a prior test.

**Fix applied**: Added `store::remove_secret(key)` call in the test. Also added `remove_secret()` API to the store module.

**Severity**: FIXED this sprint

---

### BUG-PRE-3: nika-py link errors (Python symbols missing)

**File**: `tools/nika-py/`

**Root cause**: PyO3 link symbols not found when Python isn't available. `cargo test --workspace` fails to link nika-py.

**Workaround**: Always exclude nika-py from workspace tests: `--exclude nika-py --exclude nika-napi`

**Severity**: LOW (CI likely has Python installed; local dev needs exclusion)

---

## Improvements identified but not implemented

### IMP-1: has_env_key() in nika-core is now stale

**File**: `tools/nika-core/src/catalogs/providers.rs:56`

`Provider::has_env_key()` still checks only `std::env::var`. All prod call sites have been migrated to `nika_engine::secrets::has_provider_key()`, but the method remains for backward compat. Consider:
1. Deprecating it with `#[deprecated]`
2. Or moving the SecretStore to nika-core for a self-contained fix

### IMP-2: SSRF redirect policy already existed in executor — could DRY up

The SSRF-safe redirect closure was duplicated in 3 places before this sprint:
- `runtime/executor/mod.rs:156`
- `runtime/executor/fetch.rs:254`
- `runtime/executor/infer.rs:1567`

Now `runtime/policy::ssrf_safe_redirect_policy()` is the shared version (used by FetchTool + ChatAgent). The 3 existing duplicates could be refactored to use it too.

### IMP-3: parse_yaml_budgeted doesn't cover nika-core parser

The main workflow parser in nika-core uses `marked-yaml` (not serde-saphyr). Budget limits only apply to serde-saphyr call sites. If marked-yaml supports budgets, those should be added too.

### IMP-4: mcp_config.rs reads file synchronously

`core/mcp_config.rs:219` uses `std::fs::read_to_string` (blocking). It's called during boot (single-threaded), so not as critical as the structured_output hot path, but could be made async for consistency.

---

## Summary of fixes applied this sprint

| ID | Type | Description | Files |
|----|------|-------------|-------|
| C-1 | CRITICAL | Replace unsafe set_var with thread-safe DashMap SecretStore | 14 files |
| H-2 | HIGH | SSRF-safe redirect policy for FetchTool + TUI chat | 3 files |
| H-1 | HIGH | serde-saphyr Budget limits on 10 YAML parsing sites | 10 files |
| M-6 | MEDIUM | Auth token for daemon ListSecrets | 2 files |
| M-7 | MEDIUM | Zeroize VaultPayload + plaintext Vec | 2 files |
| QA-1 | QUALITY | Replace blocking I/O in async hot paths (structured_output + nika:inject) | 2 files |
| DOC | DOC | Fix stale "45+ Tools" in README Mermaid diagram | 1 file |
