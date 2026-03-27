# v0.49 Fixes & Polish — Complete Handoff Plan

## Context

v0.49 UX mega session added 7 features (daemon auth, cli_format, provider rewrite, onboarding, model metadata, model catalog, clap styling). A pre-commit hook reverted some test modules and partially reverted security fixes. Three E2E audit agents verified the full system and found additional gaps.

**Artifacts:**
- Plan: `docs/plans/2026-03-27-v049-fixes-handoff.md` (this file)
- Memory: `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_v049_session.md`
- Security review: `~/.claude/plans/moonlit-spinning-ritchie.md`

---

## Phase 1: Missing Tests (CRITICAL — do first)

### 1.1 — cost.rs model metadata tests (10 tests)

**File:** `tools/nika-engine/src/provider/cost.rs`
**What:** `ModelTag`, `ModelMeta`, `get_model_meta()`, `format_context_window()` exist but test module was lost by hook.

**Tests to append** inside existing `#[cfg(test)] mod tests`:
```rust
#[test]
fn model_meta_claude_opus() {
    let meta = get_model_meta("claude-opus-4-20250514").unwrap();
    assert!(meta.tags.contains(&ModelTag::Reasoning));
    assert_eq!(meta.context_window, 200_000);
}
#[test]
fn model_meta_claude_sonnet() {
    let meta = get_model_meta("claude-sonnet-4-6").unwrap();
    assert!(meta.tags.contains(&ModelTag::Balanced));
    assert!(meta.tags.contains(&ModelTag::Code));
}
#[test]
fn model_meta_gpt4o() {
    let meta = get_model_meta("gpt-4o").unwrap();
    assert!(meta.tags.contains(&ModelTag::Vision));
}
#[test]
fn model_meta_o3() {
    let meta = get_model_meta("o3").unwrap();
    assert!(meta.tags.contains(&ModelTag::Reasoning));
}
#[test]
fn model_meta_gemini_pro() {
    let meta = get_model_meta("gemini-2.5-pro-preview").unwrap();
    assert_eq!(meta.context_window, 1_000_000);
}
#[test]
fn model_meta_unknown_returns_none() {
    assert!(get_model_meta("nonexistent-model-xyz").is_none());
}
#[test]
fn format_context_window_million() { assert_eq!(format_context_window(1_000_000), "1M"); }
#[test]
fn format_context_window_thousand() {
    assert_eq!(format_context_window(200_000), "200K");
    assert_eq!(format_context_window(128_000), "128K");
}
#[test]
fn format_context_window_small() { assert_eq!(format_context_window(512), "512"); }
#[test]
fn model_tag_labels() {
    assert_eq!(ModelTag::Reasoning.label(), "reasoning");
    assert_eq!(ModelTag::Code.label(), "code");
    assert_eq!(ModelTag::Fast.label(), "fast");
    assert_eq!(ModelTag::Vision.label(), "vision");
    assert_eq!(ModelTag::Balanced.label(), "balanced");
}
```
**Verify:** `cargo test -p nika-engine --lib -- provider::cost::tests::model`

### 1.2 — model_cloud.rs tests (6 tests)

**File:** `tools/nika-cli/src/model_cloud.rs`
**What:** Rewritten file has zero `#[cfg(test)]` module.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_providers_has_seven() { assert_eq!(CLOUD_PROVIDERS.len(), 7); }
    #[test]
    fn cloud_providers_all_have_best_for() {
        for p in CLOUD_PROVIDERS { assert!(!p.best_for.is_empty(), "{} missing best_for", p.name); }
    }
    #[test]
    fn format_tags_known_model() {
        let tags = format_tags("claude-sonnet-4-6");
        assert!(tags.contains("balanced") || tags.contains("code"));
    }
    #[test]
    fn format_tags_unknown_model() { assert!(format_tags("nonexistent-model").is_empty()); }
    #[test]
    fn print_cloud_models_doesnt_panic() { let _ = print_cloud_models(Some("nonexistent")); }
    #[test]
    fn print_model_info_not_found() { assert!(print_model_info("nonexistent-model-xyz").is_err()); }
}
```
**Verify:** `cargo test -p nika-cli --lib -- model_cloud`

---

## Phase 2: Security Verification (HIGH)

### 2.1 — Verify write_auth_token uses atomic 0o600

**Check:** `grep "OpenOptionsExt\|\.mode(0o600)" tools/nika-daemon/src/server.rs`
Must find `OpenOptionsExt` + `.mode(0o600)`. If only `tokio::fs::write` + `set_permissions` → TOCTOU vulnerability, replace with:
```rust
use std::os::unix::fs::OpenOptionsExt;
std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?;
```

### 2.2 — Verify CSPRNG token generation

**Check:** `grep "Uuid::new_v4" tools/nika-daemon/src/server.rs`
Must match. If `blake3::hash(seed)` found instead → predictable token, replace with UUID v4.

### 2.3 — Verify DaemonRequest custom Debug

**Check:** `grep -B1 "pub enum DaemonRequest" tools/nika-daemon/src/protocol.rs`
Must show `#[derive(Clone, Serialize, Deserialize, PartialEq)]` WITHOUT `Debug`. Custom `impl Debug` must exist that redacts `SetSecret.key` and `auth_token`.

### 2.4 — Document accepted risk: GetSecret without auth

Add comment above `DaemonRequest::GetSecret` handler in server.rs:
```rust
// SECURITY NOTE: GetSecret does not require auth token.
// Unix socket 0o600 is the access boundary (same-user only).
// Accepted risk for single-user dev machines.
```

### 2.5 — Gate Shutdown behind auth (MEDIUM)

Currently any local same-user process can `Shutdown` the daemon (DoS). Consider gating behind auth_token same as SetSecret/DeleteSecret.

---

## Phase 3: Onboarding Auto-Trigger (MEDIUM)

### 3.1 — Hook into MissingApiKey error path

**Files:**
- `tools/nika/src/main.rs` — `Commands::Run` error handler (~line 1559+)
- `tools/nika/src/main.rs` — `Commands::Infer` error handler
- `tools/nika/src/cli/onboarding.rs` — `run_onboarding_wizard()` exists
- `tools/nika-engine/src/error_domains.rs` — `ProviderError::MissingApiKey`

**Logic:**
```
when error matches ProviderError::MissingApiKey:
  if stdin.is_terminal() && !has_any_provider_key():
    run_onboarding_wizard().await
    if Ok(true) → retry the command
    else → show original error
```

### 3.2 — Add `nika setup` to AFTER_HELP

**File:** `tools/nika/src/main.rs` — AFTER_HELP constant (~line 35-115)
Under CONFIGURATION section add:
```
    nika setup                    Interactive API key setup wizard
```

---

## Phase 4: Daemon IPC for Provider Commands (MEDIUM)

### 4.1 — Add nika-daemon dep to nika binary

**File:** `tools/nika/Cargo.toml`
```toml
[target.'cfg(unix)'.dependencies]
nika-daemon = { workspace = true }
```

### 4.2 — Route provider set/delete through daemon

**File:** `tools/nika/src/cli/provider.rs`
In `ProviderAction::Set`, before `NikaKeyring::set()`:
```rust
#[cfg(unix)]
{
    let sock = nika::core::daemon_socket_path();
    if sock.exists() {
        let client = nika_daemon::DaemonClient::new(&sock);
        if client.set_secret(&provider, &api_key).await.is_ok() {
            println!("  {} stored via daemon", StatusIcon::Ok);
            return Ok(());
        }
    }
}
// Fallback: direct keyring
NikaKeyring::set(&provider, &api_key)?;
```

Same pattern for `ProviderAction::Delete`.

---

## Phase 5: Model Download Progress (MEDIUM)

### 5.1 — Replace eprint with indicatif ProgressBar

**File:** `tools/nika-cli/src/model.rs` — `ModelAction::Pull` (~line 270)
```rust
// Replace eprint!("\r Progress: {}%...") with:
use indicatif::{ProgressBar, ProgressStyle};
let pb = ProgressBar::new(total_bytes);
pb.set_style(ProgressStyle::default_bar()
    .template("  {spinner:.cyan} {bar:40.cyan/dim} {percent}% ({bytes}/{total_bytes}) {bytes_per_sec} ETA {eta}")
    .unwrap()
    .progress_chars("━╸─"));
// In callback: pb.set_position(progress.completed);
// After: pb.finish_with_message("Downloaded");
```

### 5.2 — Show ModelMeta in model info

**File:** `tools/nika-cli/src/model.rs` — `ModelAction::Info` (~line 300)
After showing quantizations, add:
```rust
if let Some(meta) = nika_engine::provider::cost::get_model_meta(&name) {
    let tags: Vec<&str> = meta.tags.iter().map(|t| t.label()).collect();
    println!("  Tags:         {}", tags.join(", "));
    println!("  Context:      {} tokens", format_context_window(meta.context_window));
}
```

---

## Phase 6: CLI Commands Polish (8 commands need cli_format adoption)

### Audit findings (from 3-agent E2E sweep):

| Command | File | LOC | cli_format? | Tests | Priority |
|---------|------|-----|-------------|-------|----------|
| doctor | nika-cli/doctor.rs | 1148 | No (18 raw Colorize) | 0 | **P1** |
| model | nika-cli/model.rs | 511 | No (35 raw Colorize) | 0 | **P1** |
| config | nika-cli/config.rs | 284 | No (5 raw Colorize) | 0 | **P1** |
| trace | nika-cli/trace.rs | 160 | No (0, plain text) | 0 | **P1** |
| daemon | nika-cli/daemon.rs | 343 | No (15 raw Colorize) | 4 | P2 |
| media | nika-cli/media.rs | 966 | No (30 raw Colorize) | 6 | P2 |
| provider | nika/cli/provider.rs | 453 | Yes (16 usages) | 9 | Done |
| model_cloud | nika-cli/model_cloud.rs | 266 | Yes (10 usages) | 0 | Done (needs tests) |

### 6.1 — Doctor refactor

**File:** `tools/nika-cli/src/doctor.rs`
Replace:
- Line 984: `"---".dimmed() + section.bold().cyan()` → `section_header(section)`
- Line 987-991: Manual icon coloring → `StatusIcon::Ok/Warn/Fail`
- Line 993: `check.name.bold()` → `status_line(icon, &format!("{} {}", name, message))`
- Line 996: `"->".cyan()` → `StatusIcon::Hint`

Add 8-10 tests: section grouping, pass/warn/fail counts, JSON output mode.

### 6.2 — Config, trace, model, daemon, media

Same pattern: replace raw `Colorize` calls with `cli_format` utilities.
Each needs 3-5 tests minimum.

---

## Phase 7: Workflow & Jobs Gaps (from audit)

### 7.1 — Job missing daemon returns Ok(()) (BUG)

**File:** `tools/nika-cli/src/jobs.rs` line 74-79
Returns `Ok(())` with error message when daemon not running → exit code 0 masks failure.
**Fix:** Return `Err(NikaError::ConfigError { reason: "daemon not running" })`

### 7.2 — Dry-run missing cost estimate

**File:** `tools/nika/src/main.rs` lines 2785-2932
Shows DAG layers and task details but NO cost projection.
**Fix:** Use `get_model_pricing()` to estimate per-task cost and show total.

### 7.3 — Spinners for network commands

- `nika check --strict` — MCP connection tests
- `nika daemon status` — daemon ping
- `nika provider test` — already done (cliclack spinner)

Use `cliclack::spinner()` (already a dep).

### 7.4 — Provider test non-interactive fallback

`test_provider_connection()` uses `cliclack::spinner()` which requires TTY. Add:
```rust
if !std::io::stderr().is_terminal() {
    println!("Testing {provider}...");
    // use plain println instead of spinner
}
```

---

## Verification Checklist

```bash
# Phase 1: Tests
cargo test -p nika-engine --lib -- provider::cost::tests::model
cargo test -p nika-cli --lib -- model_cloud

# Phase 2: Security
grep "OpenOptionsExt" tools/nika-daemon/src/server.rs          # Must match
grep "Uuid::new_v4" tools/nika-daemon/src/server.rs             # Must match
grep -B1 "pub enum DaemonRequest" tools/nika-daemon/src/protocol.rs  # No Debug derive

# Phase 3: Onboarding
ANTHROPIC_API_KEY="" nika infer "test"   # Should offer wizard in TTY

# Phase 5: Progress
nika model pull qwen3:8b  # Should show indicatif bar

# Full suite
cargo test --workspace --lib   # 8300+ tests
cargo clippy --workspace -- -D warnings
```

---

## Dependencies

```
Phase 1 (tests) ──────────────→ Phase 3 (onboarding hook)
Phase 2 (security verify) ────→ Phase 4 (daemon IPC)
Phase 5 (progress) ───────────→ independent
Phase 6 (CLI polish) ─────────→ after all others
Phase 7 (workflow gaps) ──────→ independent
```

**Recommended order:** 1 → 2 → 5 → 3 → 7.1 → 4 → 6 → 7.2+

---

## Key File Reference

| File | What | LOC |
|------|------|-----|
| `nika-daemon/src/protocol.rs` | SetSecret/DeleteSecret + custom Debug | ~170 |
| `nika-daemon/src/server.rs` | Auth token gen/write/validate + routing | ~1150 |
| `nika-daemon/src/client.rs` | set_secret/delete_secret + read_auth_token | ~550 |
| `nika-daemon/src/services/secrets.rs` | SecretService set/delete | ~280 |
| `nika-engine/src/display/cli_format.rs` | StatusIcon, panel, tree, key_value | ~360 |
| `nika-engine/src/provider/cost.rs` | ModelTag, ModelMeta, get_model_meta | ~900 |
| `nika-cli/src/model_cloud.rs` | Enhanced model catalog display | ~160 |
| `nika-cli/src/doctor.rs` | Doctor diagnostic (needs refactor) | ~1148 |
| `nika-cli/src/model.rs` | Local model commands (needs indicatif) | ~511 |
| `nika-cli/src/jobs.rs` | Job commands (exit code bug) | ~290 |
| `nika/src/cli/provider.rs` | Provider set with cliclack | ~300 |
| `nika/src/cli/onboarding.rs` | First-run wizard | ~135 |
| `nika/src/main.rs` | Setup command + clap styling + run handler | ~3200 |

---

## Gotcha: Pre-commit Hook

A pre-commit hook runs `cargo fmt` + `clippy` and has a file-restore mechanism. **Workaround:**
```bash
cat > file.rs << 'RUSTEOF'
...code...
RUSTEOF
cargo fmt -p <crate> && git add file.rs && git commit -m "..."
```
