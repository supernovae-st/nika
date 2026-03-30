# Mega Handoff — Post v0.52.0 Remaining Work

> Generated: 2026-03-30 | Base: v0.52.0 (tag) | 8,938 tests | 0 clippy

---

## STATUS: What v0.52.0 Did

| Wave | Status | Commits | Detail |
|------|--------|---------|--------|
| 1A Security | DONE | 1 | IPv6 SSRF, path blocklist, symlink fail-closed |
| 1B Runtime | DONE | 1 | cancel_token, HashMap, exit_code, size_bytes |
| 1C Errors | DONE | 1 | error accumulation, warn levels, template warn |
| 1D Dead code | DONE | 1 | retain, artifact_paths, RetryCondition removed |
| 2A+B E2E Tests | DONE | 1 | 28 tests: 12 mock + 7 provider + 7 structured + 2 integration |
| 4 Release | DONE | 2 | Version bump + CHANGELOG + tag v0.52.0 |

**SKIPPED (all below = remaining work):**

---

## PRIORITY 1: QUICK WINS (30min total, 2 commits)

### 1.1 — Dead workspace dependencies (10min, 1 commit)

6 workspace deps in `tools/Cargo.toml` are declared but ZERO member crates use them:

```
nutype = { version = "0.6", features = ["serde"] }
static_assertions = "1.1"
strum = { version = "0.27", features = ["derive"] }
derive_more = { version = "2.0", features = ["display", "from", "error"] }
tracing-error = "0.2"
console = "0.16"
```

**Fix:** Delete all 6 lines from `[workspace.dependencies]` in `tools/Cargo.toml`. Run `cargo check --workspace` to confirm.

**Commit:** `chore(deps): remove 6 dead workspace dependencies`

### 1.2 — CI cargo-deny hard fail (20min, 1 commit)

`ci.yml:248` runs `cargo deny check 2>&1 || true` — failures are silently ignored.
Same for `cargo machete` at line 252.

**Fix:**
1. Remove `|| true` from cargo deny (line 248)
2. Remove `|| true` from cargo machete (line 252)
3. Run `cargo deny check` locally first — fix any advisories/bans
4. Run `cargo machete` locally — it should confirm the 6 dead deps above

**Commit:** `ci: cargo-deny and machete hard fail (remove || true)`

---

## PRIORITY 2: PROVIDERNAME ENGINE MIGRATION (3-4h, 1 commit)

The `ProviderName` typed enum exists in `nika-core`. The AST layer (analyzer) already uses it. But the **engine** still uses `Option<String>` for provider fields in ~18 files.

### Scope

| Area | Files | Edits | Complexity |
|------|-------|-------|-----------|
| InferParams.provider | 5 | ~30 | Low |
| AgentParams.provider | 3 | ~20 | Low |
| ExecContext.provider | 2 | ~10 | Low |
| config/boot defaults | 3 | ~15 | Low |
| Tests | 8 | ~50 | Low (string→enum) |
| Runner DAG/display | 3 | ~15 | Low |
| **Total** | **~18** | **~140** | **Low** |

### Strategy: 4 parallel agents

```
Agent 1: InferParams.provider + executor default_provider
  Files: executor/verbs.rs, executor/mod.rs, config.rs, lower.rs, runner.rs
  Pattern: Option<String> → Option<ProviderName>
  .as_deref() → .as_ref().map(|p| p.as_str()) or direct match

Agent 2: AgentParams.provider + spawn.parent_provider
  Files: rig_agent_loop/mod.rs, spawn.rs, executor/agent.rs
  Pattern: Same as Agent 1

Agent 3: config.provider + partial.provider + context.provider
  Files: config.rs, boot.rs, run_context.rs
  Pattern: Same

Agent 4: All tests
  Files: all test modules in the above files
  Pattern: Some("anthropic") → Some(ProviderName::Anthropic)
           Some("mock") → Some(ProviderName::Mock)
```

**Key function:** `ProviderName::parse("claude")` → `ProviderName::Anthropic` (alias resolution).
**Key method:** `provider.as_str()` → `"anthropic"` (canonical string).

**Commit:** `refactor(engine): complete ProviderName migration — 18 files, Option<String> → Option<ProviderName>`

---

## PRIORITY 3: STREAMING try_send LOGGING (30min, 1 commit)

10 instances of `let _ = tx.try_send(...)` in `nika-engine/src/runtime/rig_agent_loop/streaming.rs` silently discard channel send failures.

**Fix approach:** Create a helper macro at the top of streaming.rs:

```rust
/// Send a stream chunk to TUI, logging at debug if the channel is full/closed.
macro_rules! stream_send {
    ($tx:expr, $chunk:expr) => {
        if let Err(e) = $tx.try_send($chunk) {
            tracing::debug!("Stream send dropped: {e}");
        }
    };
}
```

Then replace all 10 instances:
```rust
// Before:
let _ = tx.try_send(StreamChunk::Token(text.clone()));

// After:
stream_send!(tx, StreamChunk::Token(text.clone()));
```

Lines: 128, 148, 160, 176, 481, 504, 536, 569, 593, 606

**Commit:** `fix(streaming): log try_send failures at debug level (10 sites)`

---

## PRIORITY 4: DAEMON + SECRETS CLEANUP (2h, 3 commits)

### 4.1 — Verify daemon auto-start tests

The daemon auto-starts but the test coverage is unclear. Write 4 tests:

```rust
#[tokio::test]
async fn test_daemon_auto_starts_when_missing()
#[tokio::test]
async fn test_daemon_not_started_if_nika_no_daemon()
#[tokio::test]
async fn test_secrets_from_env_without_daemon()
#[tokio::test]
async fn test_secrets_fallback_clear_error()
```

Location: `nika-daemon/src/lib.rs` or `nika-engine/src/secrets/` test module.

**Commit:** `test(daemon): 4 auto-start + secrets resolution tests`

### 4.2 — Clean provider list output

`nika provider list` should show "(env)" or "(daemon)" next to each provider.
Never touch keychain directly.

Check: `nika-cli/src/provider.rs` or `nika/src/commands/provider.rs`

**Commit:** `refactor(secrets): provider list shows source — env or daemon`

### 4.3 — Remove legacy keychain references

Search for and remove:
- `NIKA_SKIP_KEYCHAIN` env var references
- `NIKA_KEYCHAIN_BOOT` references
- Any direct keyring access that bypasses daemon

**Commit:** `docs: update secrets docs — env vars or daemon only`

---

## PRIORITY 5: E2E WORKFLOW EXECUTION (4h, 2-3 commits)

### 5.1 — Execute existing .nika.yaml files

There are **502 workflow files** in `tools/nika/examples/`. The most important to test:

```bash
# Gate tests (should all pass with mock)
nika run examples/gates/feature/gate-001-*.nika.yaml --provider mock
nika run examples/gates/complex/gate-*.nika.yaml --provider mock

# DAG pattern tests
nika run examples/dag-patterns/*.nika.yaml --provider mock

# Use case examples (may need real providers)
nika run examples/use-cases/*.nika.yaml --provider mock
```

For each failure: determine if it's a YAML bug or engine bug, fix accordingly.

### 5.2 — Adversarial tests (from original prompt)

Add to `nika/tests/e2e_workflow_test.rs`:

```
DATA FLOW TRAPS:
  1. Structured output → binding → ANOTHER structured output (JSON in JSON)
  2. for_each with 0 items ✅ (already tested in v0.52.0)
  3. for_each with 1 item (degenerate case)
  4. for_each output → another for_each (nested iteration)
  5. Task output 50K+ chars → binding in next task
  6. with: { data: $nonexistent } → NIKA-071 error, not panic
  7. Non-Latin prompt (Chinese/Arabic) → structured output works
  8. $env.VAR containing {{template}} → no injection

STRUCTURED OUTPUT STRESS:
  9. 10 levels of nesting
  10. additionalProperties: false → zero extra fields
  11. Contradictory constraints (min:10, max:5) → clear error
  12. Empty schema {} → behavior?
  13. "Never respond in JSON" prompt → layers still win
  14. Structured + for_each: EACH iteration valid JSON

CONCURRENCY:
  15. for_each concurrency:50 with 3 items → no deadlock
  16. 2 tasks depending on same parent → parallel execution
  17. Agent max_turns:1 → graceful completion
```

### 5.3 — Multi-provider comparison workflow

Create `tests/multi-provider-comparison.nika.yaml`:
```yaml
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: openai_test
    provider: openai
    model: gpt-4.1-mini
    infer: "Describe a chef named Marco"
    structured:
      schema: &chef
        type: object
        properties:
          name: { type: string }
          country: { type: string }
          specialties: { type: array, items: { type: string }, minItems: 2 }
        required: [name, country, specialties]

  - id: gemini_test
    provider: gemini
    model: gemini-2.5-flash
    infer: "Describe a chef named Marco"
    structured:
      schema: *chef
  # ... repeat for each provider
```

---

## PRIORITY 6: ANTHROPIC API KEY (5min)

The Anthropic API key has **insufficient credits**:
```
invalid_request_error: Your credit balance is too low to access the Anthropic API
```

**Fix:** Add credits at https://console.anthropic.com/settings/billing
This will unlock:
- `e2e_real_anthropic_haiku` test
- `e2e_structured_anthropic` test
- `e2e_real_research_pipeline` test

---

## PRIORITY 7: CI POLISH (1h, 2 commits)

### 7.1 — Add dependabot.yml

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: cargo
    directory: /tools
    schedule:
      interval: weekly
    open-pull-requests-limit: 5
    labels:
      - dependencies
      - rust
```

### 7.2 — Dockerfile version

Check if a Dockerfile exists with hardcoded version and update to 0.52.0.

---

## EXECUTION ORDER (optimal)

```
Session 1 (30min): Priority 1 — dead deps + CI deny
Session 2 (3-4h):  Priority 2 — ProviderName migration (4 parallel agents)
Session 3 (2h):    Priority 3+4 — streaming + daemon cleanup
Session 4 (4h):    Priority 5 — E2E execution + adversarial tests
Session 5 (1h):    Priority 7 — CI polish + dependabot
```

Priority 6 (Anthropic billing) is a manual action, not code.

---

## KNOWN STATE

```
Branch: main
Tag: v0.52.0
Tests: 8,938 (8,910 lib + 28 E2E integration)
Clippy: 0 warnings (--all-targets --all-features)
Providers working: openai, gemini, groq, mistral, deepseek, xai (6/7)
Provider broken: anthropic (billing, not code)
Dead workspace deps: 6 confirmed unused
ProviderName: AST migrated, engine NOT migrated (~18 files)
Streaming try_send: 10 sites with let _ = (display-only, non-critical)
```

## CONTEXT WINDOW HANDOFF

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-handoff-v052-remaining.md)"
```
