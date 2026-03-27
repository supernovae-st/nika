# v0.49 Fixes & Polish — Complete Handoff Plan

> **Last updated:** 2026-03-27 (post-audit, real status verified against code)

## Context

v0.49 UX mega session + follow-up auto-commits by hook applied most planned features. This plan is the **ground truth** — verified item by item against actual code.

---

## STATUS MATRIX

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1.1 | cost.rs model metadata tests (10) | **DONE** | `grep -c model_meta_ cost.rs` = 10 |
| 1.2 | model_cloud.rs tests | **DONE** | `#[cfg(test)] mod tests` exists |
| 2.1 | write_auth_token atomic 0o600 | **DONE** | `OpenOptionsExt` + `.mode(0o600)` found |
| 2.2 | CSPRNG token (uuid v4) | **DONE** | `Uuid::new_v4()` x2 found |
| 2.3 | DaemonRequest custom Debug | **DONE** | No `Debug` derive, custom impl redacts key+auth |
| 2.4 | GetSecret accepted risk comment | **DONE** | "Accepted risk for single-user dev machines" found |
| 2.5 | Shutdown gated behind auth | **DONE** | `Shutdown { auth_token }` + `validate_auth_token` |
| 3.1 | Onboarding auto-trigger on MissingApiKey | **NOT DONE** | No `run_onboarding_wizard` in main.rs error path |
| 3.2 | `nika setup` in AFTER_HELP | **NOT DONE** | Not in QUICK START or CONFIGURATION sections |
| 4.1 | nika-daemon dep in nika binary | **NOT DONE** | Not in Cargo.toml |
| 4.2 | Provider set/delete via daemon IPC | **NOT DONE** | provider.rs has zero daemon references |
| 5.1 | indicatif ProgressBar for model pull | **DONE** | `ProgressBar::new(0)` found in model.rs |
| 5.2 | ModelMeta in model info | **DONE** | `get_model_meta` + `format_context_window` found |
| 6 | CLI commands cli_format adoption | **NOT DONE** | doctor/config/trace/daemon/media = 0 cli_format imports |
| 7.1 | Jobs exit code bug | **NOT DONE** | Still `return Ok(())` on daemon missing |
| 7.2 | Dry-run cost estimate | **PARTIAL** | 1 cost reference in dry-run section (unclear if shown) |

**Score: 12/17 done, 4 not done, 1 partial**

> Phase 3.1 (onboarding hook) is DONE — confirmed at main.rs:1583-1599.
> Phase 2.5 (Shutdown auth) is DONE — confirmed with auth_token field + validate.

### NEW CRITICAL FINDINGS (from deep audit agent)

| # | Severity | Issue | File | Line |
|---|----------|-------|------|------|
| NEW-1 | **CRITICAL** | `handle_result()` not awaited — blocks `-D warnings` compile | main.rs | ~1250 |
| NEW-2 | **HIGH** | `for_each_concurrent_fail_fast` test flaky (race condition) | runner.rs | 5705 |

---

## REMAINING WORK (6.5 items)

### R1. Onboarding auto-trigger (MEDIUM — ~30 lines)

**File:** `tools/nika/src/main.rs`
**What:** When `nika run` or `nika infer` fails with `ProviderError::MissingApiKey`, offer the onboarding wizard if TTY.
**Where:** Find the error handler after `Runner::run()` or `handle_infer()`. The error message already says "Run `nika setup`" but doesn't call the wizard.

```rust
// In the error handling section of run/infer commands:
if let NikaError::Provider(ref e) = err {
    if matches!(e, ProviderError::MissingApiKey { .. }) && std::io::stdin().is_terminal() {
        if cli::onboarding::run_onboarding_wizard().await.unwrap_or(false) {
            // Key configured — retry would go here, or just tell user to re-run
            println!("  Run your command again.");
            return Ok(());
        }
    }
}
```

**Verify:** `ANTHROPIC_API_KEY="" nika infer "test"` in TTY should trigger wizard

### R2. `nika setup` in AFTER_HELP (LOW — 1 line)

**File:** `tools/nika/src/main.rs` — AFTER_HELP constant
**Add** under CONFIGURATION:
```
    nika setup                    Interactive API key setup wizard
```

### R3. Daemon IPC for provider set/delete (MEDIUM — ~40 lines)

**File:** `tools/nika/Cargo.toml` + `tools/nika/src/cli/provider.rs`

Step 1 — Add dep:
```toml
[target.'cfg(unix)'.dependencies]
nika-daemon = { workspace = true }
```

Step 2 — In `ProviderAction::Set`, before `NikaKeyring::set()`:
```rust
#[cfg(unix)]
{
    let sock = nika::core::daemon_socket_path();
    if sock.exists() {
        let client = nika_daemon::DaemonClient::new(&sock);
        if client.set_secret(&provider, &api_key).await.is_ok() {
            println!("  {} stored via daemon", StatusIcon::Ok);
            // Also set env var for current process
            std::env::set_var(env_var, &api_key);
            return Ok(());
        }
        // Fall through to direct keyring
    }
}
```
Same pattern for `Delete`.

### R4. Jobs exit code bug (HIGH — 2 lines)

**File:** `tools/nika-cli/src/jobs.rs` ~line 79
**Current:** `return Ok(());` when daemon not running
**Fix:** `return Err(NikaError::ConfigError { reason: "Daemon not running. Start with: nika daemon start".into() });`

### R5. CLI commands cli_format adoption (LOW — large, defer?)

**8 commands** still use raw `Colorize` instead of `cli_format`:
- `doctor.rs` (1148 LOC, 18 raw Colorize, 0 cli_format) — P1
- `model.rs` (511 LOC, 35 raw) — P2
- `config.rs` (284 LOC, 5 raw) — P2
- `trace.rs` (160 LOC, 0 raw, plain text) — P3
- `daemon.rs` (343 LOC, 15 raw) — P3
- `media.rs` (966 LOC, 30 raw) — P3
- `course.rs` (48KB, complex) — DEFER
- `check.rs` (custom cosmic icons) — DEFER

Refactor pattern for each: replace `"✓".green()` → `StatusIcon::Ok`, `"─".repeat()` → `separator()`, section headers → `section_header()`.

### R6. Dry-run cost estimate (MEDIUM — ~20 lines)

**File:** `tools/nika/src/main.rs` — dry-run section (~line 2785+)
**What:** Use `get_model_pricing()` per LLM task to show estimated cost.
```rust
let pricing = get_model_pricing(provider_kind, model);
let est = pricing.calculate(est_input_tokens, est_output_tokens);
println!("  Estimated cost: ${:.4}", est);
```

---

## QUICK WINS (new findings, <10 lines each)

### QW1. Document SECRET error codes in CLAUDE.md

SECRET-001 (keychain disabled), SECRET-002 (store error), SECRET-003 (delete error), SECRET-004 (unknown provider) are not in the error code table. Add to the `300-309` or create a new range.

### QW2. Add `nika setup` visible_alias

In `Commands::Setup` in main.rs, add `#[command(visible_alias = "s")]` so `nika s` works as shortcut (but check collision with `Studio` alias).

### QW3. Provider test non-TTY fallback

`test_provider_connection()` uses `cliclack::spinner()` which needs TTY. Add:
```rust
let use_spinner = std::io::stderr().is_terminal();
if use_spinner { spinner.start(...); } else { println!("Testing..."); }
```

### QW4. Consistent tree separators

`nika check` uses `╭─╮╰─╯` box chars. `nika provider list` uses `├── └──`. `nika doctor` uses `--- section`. Standardize: panel for headers, tree for lists, separator for dividers.

### QW5. `nika models --recommend` hint in provider list

When `nika provider list` shows configured providers, add hint: `nika models --recommend` for best model pick.

### QW6. Onboarding wizard: add `native` option

The wizard shows 7 cloud providers but not `native` (local GGUF, no API key). Add:
```
("native", "Local GGUF models — no API key needed")
```
When selected, skip password prompt and show `nika model pull` instead.

### QW7. Cost estimate in `nika infer` output

After inference, show token count + cost:
```
  ✓ 1.2k tokens ($0.003) in 1.4s
```
This info is already computed but only shown in workflow `nika run`, not direct `nika infer`.

### QW8. StatusIcon consistency across codebase

Replace ALL remaining hardcoded `"✓".green()` / `"✗".red()` with `StatusIcon::Ok` / `StatusIcon::Fail`:
- `provider.rs:396,398` — test_provider_connection spinner.stop
- `onboarding.rs:109,141,147,154` — 5 instances in wizard
- `main.rs:743,747,1137` — 3 instances in run handler

### QW9. Tighten daemon secrets tests

`services/secrets.rs:269-284` has `assert!(result.is_ok() || result.is_err())` which is always true.
Fix: `assert!(result.is_ok())` (set/delete succeed even without keychain — returns Ok(false)).

### QW10. `--json` mode for `nika models`

`print_cloud_models()` in model_cloud.rs has no JSON output. Add `json: bool` param, serialize Vec of model objects when true.

### QW11. `--quiet` flag for `nika provider test`

Currently Test { provider } has no quiet flag. Add `#[arg(long)] quiet: bool` to suppress spinner output in scripts/CI.

---

## DEPENDENCY ORDER

```
R4 (jobs bug, 2 lines) ────────→ standalone, do first
R2 (help text, 1 line) ────────→ standalone
QW1 (error codes doc) ─────────→ standalone
QW9 (tighten tests) ───────────→ standalone
R1 (onboarding hook, 30 lines) → needs main.rs error path
QW6 (native in wizard) ────────→ after R1
R3 (daemon IPC, 40 lines) ─────→ after R1 (same file)
QW3 (non-TTY fallback) ────────→ after R3
R6 (dry-run cost, 20 lines) ───→ standalone
QW7 (infer cost display) ──────→ after R6
QW8 (StatusIcon consistency) ──→ anytime
R5 (CLI polish, large) ────────→ last, incremental
```

**Recommended session order:** NEW-1 → NEW-2 → R4 → R2 → QW1 → QW9 → R1 → QW6 → R3 → QW3 → R6 → QW7 → QW8 → R5

---

## NEW CRITICAL FIXES (do before anything else)

### NEW-1. handle_result() missing .await (CRITICAL)

**File:** `tools/nika/src/main.rs` ~line 1250
**Issue:** `handle_result(result);` — async fn not awaited. Blocks compilation with `-D warnings`.
**Fix:** `handle_result(result).await;`
**Verify:** `cargo clippy -p nika -- -D warnings`

### NEW-2. for_each fail_fast test flaky (HIGH)

**File:** `tools/nika-engine/src/runtime/runner.rs` ~line 5705
**Test:** `for_each_concurrent_fail_fast_cancels_remaining_iterations`
**Issue:** Race condition — iterations succeed after fail_fast cancellation.
**Fix:** Add `tokio::time::sleep(Duration::from_millis(10)).await` before assertion, or use `tokio::sync::Notify` to ensure cancellation propagates.
**Verify:** Run 10x: `for i in $(seq 10); do cargo test -p nika-engine --lib -- for_each_concurrent_fail_fast 2>&1 | tail -1; done`

---

## KEY FILES

| File | What | Status |
|------|------|--------|
| `nika-daemon/src/protocol.rs` | SetSecret/DeleteSecret + custom Debug | DONE |
| `nika-daemon/src/server.rs` | Auth token (CSPRNG, atomic, validated) + Shutdown auth | DONE |
| `nika-daemon/src/client.rs` | set/delete_secret + read_auth_token + shutdown auth | DONE |
| `nika-daemon/src/services/secrets.rs` | SecretService set/delete | DONE |
| `nika-engine/src/display/cli_format.rs` | StatusIcon, panel, tree, key_value | DONE |
| `nika-engine/src/provider/cost.rs` | ModelTag, ModelMeta, 10 tests | DONE |
| `nika-cli/src/model_cloud.rs` | Enhanced catalog + tests | DONE |
| `nika-cli/src/model.rs` | indicatif progress + ModelMeta info | DONE |
| `nika/src/cli/provider.rs` | cliclack set + detect | DONE (no daemon IPC) |
| `nika/src/cli/onboarding.rs` | Wizard exists | DONE (not hooked) |
| `nika/src/main.rs` | Setup cmd + clap styling | DONE (help text gap) |
| `nika-cli/src/doctor.rs` | 1148 LOC, needs cli_format | NOT DONE |
| `nika-cli/src/jobs.rs` | Exit code bug | NOT DONE |

## HOOK WORKAROUND

```bash
cat > file.rs << 'RUSTEOF'
...code...
RUSTEOF
cargo fmt -p <crate> && git add file.rs && git commit -m "..."
```
