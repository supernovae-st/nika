# Overnight Mega Plan v3 — Full Autonomy Edition

> **Budget:** $20-30 API | **Providers:** OpenAI, xAI, Gemini (Anthropic: no credits)
> **Mode:** Full autonomy, 20h+ | **Enriched by:** 10 parallel agents + Socratic review
> **Codebase:** 356K LOC Rust, 12 crates, 9015+ tests, v0.53.0 + 9 post-release commits
> **Philosophy:** Question everything. Verify everything. Trust nothing.

---

## Autonomous Operation Protocol

> These rules govern HOW the agent works — not WHAT it works on.

### Build & Test Loop (MANDATORY after every code change)
```bash
# STEP 1: Compile check (fast, catches syntax)
cd /Users/thibaut/dev/supernovae/nika/tools && cargo check -p nika-engine 2>&1 | tail -20

# STEP 2: Full test suite (ALWAYS --lib, NEVER without)
cargo test --workspace --lib 2>&1 | tail -10

# STEP 3: If tests fail — STOP. Do NOT commit broken code.
# Read the error. Fix it. Re-run tests. Only then continue.
```

### Commit Protocol
```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
- **1 fix = 1 commit** — never batch unrelated fixes
- **git add <specific files>** — never `git add .` or `git add -A`
- Push after every 3-5 commits (checkpoint)

### Recovery Procedures
- **Compilation fails after fix:** Read error carefully. Most likely a missing import, wrong scope, or type mismatch. Fix in same commit.
- **Tests fail after fix:** Run the specific failing test with `-- --nocapture` to see output. Your fix likely changed behavior another test depends on. Understand WHY the test fails before changing it.
- **Provider returns 429:** Wait 60s, retry. If persistent, switch to different provider for remaining workflows. Track which provider hit limits.
- **Provider returns 401:** API key issue. Skip that provider's workflows, document the failure, move on.
- **A fix breaks 50+ tests:** STOP. `git stash`. Think about whether the fix is correct. Maybe the tests are right and the fix is wrong. Only if you're CERTAIN the fix is correct, update the tests.
- **Context window getting full:** Commit all current work. Push. Create a handoff note in `docs/plans/` with exact state. Start new session from handoff.

### Socratic Verification Loops

**After EACH code fix, ask yourself:**
1. Did I verify the fix with a test that FAILS before and PASSES after?
2. Did I check that the fix doesn't change behavior in OTHER code paths?
3. Did I grep for similar patterns elsewhere that need the same fix?
4. Would a user notice this fix? If yes, does it need CHANGELOG entry?
5. Is there a workflow I can run to verify this works end-to-end, not just in unit tests?

**After EACH workflow run, ask yourself:**
1. Did the workflow succeed for the RIGHT reason, or did it silently skip the thing I'm testing?
2. Is the output LOGICALLY correct? (Not just "non-empty" but actually meaningful)
3. Did the expected events appear in the output? (Check NDJSON or --no-live output)
4. If it failed — is the error message helpful? Does it point to the right line?
5. Would this workflow catch a regression if someone changed the code tomorrow?

**After EACH phase completion:**
1. Run the FULL test suite: `cargo test --workspace --lib`
2. Run ALL security workflows (G01-G05) — they must ALL fail
3. Run 3 random non-security workflows — they must ALL pass
4. Push to remote
5. Update the checklist at the bottom of this file

### NDJSON Verification Protocol
```bash
# Run workflow and capture NDJSON events
./tools/target/debug/nika run workflow.nika.yaml --no-live 2>&1 | tee /tmp/run.log

# Check for specific event types in output
grep -i "event" /tmp/run.log | head -20

# Check for leaked secrets (should find NONE)
grep -iE "(sk-|AKIA|ghp_|password|secret)" /tmp/run.log

# Check for error events (understand each one)
grep -i "error\|failed\|warning" /tmp/run.log
```

---

## Phase 0: Stabilize Uncommitted Changes (~15m)

> 4 files have uncommitted changes. Compilation is BROKEN. Fix this first.

### 0.1 Fix MAX_MCP_RESULT_SIZE Scope
**File:** `nika-engine/src/runtime/executor/invoke.rs`
**Bug:** `MAX_MCP_RESULT_SIZE` is defined inside the `if let Some(tool)` branch (line 212) but referenced in the `else if let Some(resource)` branch (line 351).
**Fix:** Move the const to the outer scope (before the if/else if), remove the duplicate.
**Verify:** `cargo check -p nika-engine`

### 0.2 Verify Other Uncommitted Changes
**Files:** `nika-core/src/ast/analyzer/analyze.rs`, `nika-core/src/ast/orchestrate.rs`, `nika-engine/src/runtime/orchestrate.rs`
**Action:** Read each diff. Verify the changes are correct. If yes, commit them individually:
- `fix(ast): validate orchestrate confidence_target bounds`
- `fix(runtime): add YAML syntax examples to orchestrator system prompt`
- `fix(security): enforce 50MB limit on MCP resource reads`

### 0.3 Verify Green Build
```bash
cargo test --workspace --lib  # Must pass
cargo check -p nika           # Must compile
```

**GATE:** Do NOT proceed to Phase 1 until build is green.

---

## Phase 1: CRITICAL Security (~2h)

> Fix the bugs that block everything else. Each is a separate commit.
> **Priority:** Security bugs FIRST because they affect what we can test later.

### 1.1 Newline Injection in Shell Exec
**File:** `nika-engine/src/runtime/executor/exec.rs:284-285`
**Source:** Security Agent (CRITICAL)
**Bug:** `validate_command_string()` allows `\n` in `shell: true` mode. Template-resolved values with embedded newlines execute as separate shell commands.
**Attack vector:** LLM output → `with:` binding → exec command → shell injection via newlines
**Fix:** When `shell: true`, reject commands containing `\n` (literal newline, not `\\n` escape). Add to dangerous char check alongside existing `$(`, backtick checks.
**Test (TDD):**
```rust
#[test]
fn test_newline_injection_blocked_in_shell_mode() {
    // "echo hello\necho injected" must be rejected with NIKA-053
    let cmd = "echo hello\necho injected";
    let result = validate_command_string(cmd, /*shell=*/true);
    assert!(result.is_err());
    // Verify error code is NIKA-053
}
```
**Verify:** `cargo test --workspace --lib -p nika-engine -- exec`
**Socratic check:** Does this also affect `\r\n`? Check for `\r` too.

### 1.2 IPv6 `::` SSRF Bypass
**File:** `nika-engine/src/runtime/policy.rs:46-74`
**Source:** Security Agent (CRITICAL)
**Bug:** IPv6 unspecified address `::` not blocked. Equivalent to `0.0.0.0` in IPv4.
**Fix:** Add `Ipv6Addr::UNSPECIFIED` check in the V6 match arm.
**Test (TDD):**
```rust
#[test]
fn test_ipv6_unspecified_blocked() {
    let addr = "http://[::]:8080/admin";
    let result = check_ssrf(addr);
    assert!(result.is_err()); // NIKA-045
}
```
**Verify:** `cargo test --workspace --lib -p nika-engine -- policy`
**Socratic check:** What about `[::1]` (loopback)? Is it already blocked? Verify.

### 1.3 SECRET_RE Expansion
**File:** `nika-engine/src/util/mod.rs:30-35`
**Source:** Security Agent (CRITICAL) + prior audit
**Bug:** Missing patterns for: AWS ASIA tokens, GitHub OAuth (ghu_, ghr_, ghd_), SendGrid (SG\.), Stripe (sk_live_, sk_test_), DB connection strings, JWT tokens.
**Fix:** Extend regex with all patterns. Use `(?:...)` non-capturing groups for performance.
**Test (TDD):** One test per pattern type — each secret must be redacted.
```rust
#[test]
fn test_redact_aws_temp_creds() {
    let input = "key: ASIAVYXYZEXAMPLE12345";
    assert!(redact_secrets(input).contains("[REDACTED]"));
    assert!(!redact_secrets(input).contains("ASIAVYXYZ"));
}

#[test]
fn test_redact_github_user_token() {
    let input = "token: ghu_16C7e42F292c6912E7710c838347Ae178B4a";
    assert!(redact_secrets(input).contains("[REDACTED]"));
}

#[test]
fn test_redact_db_connection_string() {
    let input = "postgres://admin:s3cret@db.host:5432/mydb";
    assert!(redact_secrets(input).contains("[REDACTED]"));
}

#[test]
fn test_redact_jwt() {
    let input = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abc123def456";
    assert!(redact_secrets(input).contains("[REDACTED]"));
}
```
**Verify:** `cargo test --workspace --lib -p nika-engine -- redact`
**Socratic check:** Will the JWT regex match base64-encoded strings that AREN'T JWTs? Test with a non-JWT base64 string to verify false positive rate is acceptable.

### 1.4 MCP Error Response Redaction
**File:** `nika-engine/src/runtime/executor/invoke.rs:159-167`
**Source:** Security Agent (HIGH)
**Bug:** `McpResponse` event stores raw tool response. Secrets in MCP error messages leak.
**Fix:** Apply `crate::util::redact_secrets()` to response value before creating event.
**Test:** Mock MCP response with embedded API key — verify redacted in event.
**Verify:** `cargo test --workspace --lib -p nika-engine -- invoke`

### 1.5 Recursive JSON Redaction
**File:** `nika-engine/src/runtime/executor/verbs.rs` (or `resolve.rs:458`)
**Source:** Security Agent (CRITICAL) + Binding Agent
**Bug:** `to_value_redacted()` is flat — only redacts top-level string values.
**Fix:** Recursive traversal:
```rust
fn redact_value_recursive(value: &mut serde_json::Value, re: &Regex) {
    match value {
        Value::String(s) => {
            *s = re.replace_all(s, "[REDACTED]").to_string();
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_value_recursive(item, re);
            }
        }
        Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                redact_value_recursive(v, re);
            }
        }
        _ => {}
    }
}
```
**Test (TDD):** Nested object with secret at depth 3 must be redacted.
**Verify:** `cargo test --workspace --lib -p nika-engine -- redact`
**Socratic check:** Does this handle circular references? serde_json::Value can't be circular, so no.

### 1.6 JSON Schema — 9 Missing Fields
**File:** `tools/nika-engine/schemas/nika-workflow.schema.json` + `tools/nika/schemas/`
**Source:** Prior audit
**Add:** Workflow-level: `max_duration_secs` (integer), `pkg` (object). Task-level: `timeout` (integer), `max_tokens` (integer), `temperature` (number, min 0.0, max 2.0), `system` (string), `extended_thinking` (boolean), `thinking_budget` (integer), `response_format` (string enum: text/json/markdown).
**Verify:** Build binary, then `nika check` on a workflow using ALL 9 new fields.
**Socratic check:** Are there OTHER fields missing? Grep the AST parser for all field names, cross-reference with schema. Common misses: `concurrency`, `fail_fast`, `as` (for_each alias).

### 1.7 BINDING_RE Missing `{{context.*}}`
**File:** `nika-engine/src/runtime/executor/exec.rs:46`
**Source:** Prior audit
**Fix:** `\{\{(with|inputs|context)\.[^}]+\}\}`
**Test:** Exec command with `{{context.readme}}` must trigger the shell warning (not silently pass).
**Verify:** `cargo test --workspace --lib -p nika-engine -- exec`

### 1.8 Secret Redaction in tracing::warn (3 sites)
**Files:** `exec.rs:83`, `security.rs:258`, `security.rs:377`
**Source:** Prior audit
**Fix:** Replace `command = %cmd` with `command = %crate::util::redact_secrets(&cmd)` or use `redact_for_event()`.
**Verify:** `cargo test --workspace --lib -p nika-engine -- exec security`
**Socratic check:** Are there OTHER tracing::warn/error calls that log sensitive data? Grep for `tracing::warn.*command\|tracing::error.*command` to find all.

### 1.9 Quoted Pattern Bypass in Command Blocklist
**File:** `nika-engine/src/runtime/security.rs:250-270`
**Source:** Security Agent (HIGH)
**Bug:** Backtick detection uses `contains_unquoted()` but pipe-to-shell (`| bash`, `| sh`) uses simple `.contains()`. Inconsistent.
**Fix:** Apply `contains_unquoted()` to ALL blocklist pattern checks, not just backticks.
**Test:** `echo data | 'bash'` should still be blocked (or not — decide semantic).
**Verify:** `cargo test --workspace --lib -p nika-engine -- security`

**GATE after Phase 1:**
```bash
cargo test --workspace --lib          # ALL pass
# Run security workflows G01-G05     # ALL must fail correctly
```

---

## Phase 2: Silent Bug Fixes (~1.5h)

> Code that LOOKS correct but produces wrong results.

### 2.1 `unwrap_or_default()` Hiding Serialization Failures
**File:** `nika-core/src/binding/transform.rs`
**Source:** Silent Bugs Agent (CRITICAL)
**Bug (a):** Line ~267 (FirstN on Object): `serde_json::to_string(value).unwrap_or_default()` silently returns `""`.
**Bug (b):** Line ~415 (ToJson): Same pattern.
**Fix:** Replace with `serde_json::to_string(value).map_err(|e| TransformError::SerializationFailed { details: e.to_string() })?`
**Test:** Existing tests should still pass. Add test with valid object to confirm no regression.
**Verify:** `cargo test --workspace --lib -p nika-core -- transform`
**Socratic check:** Is `serde_json::to_string()` actually fallible for valid `serde_json::Value`? It shouldn't be — serde_json::Value is always serializable. So `unwrap_or_default()` is technically safe here. BUT: it's bad practice and hides the issue if Value ever changes. Fix anyway for correctness hygiene.

### 2.2 String "null" → JSON null Coercion
**File:** `nika-engine/src/runtime/executor/verbs.rs:70-91`
**Source:** Silent Bugs Agent (HIGH)
**Bug:** `coerce_json_types()` converts `"null"` → `Value::Null`. Creates ambiguity.
**Fix:** Remove the `"null"` case. Keep `"true"`/`"false"` and numeric coercion.
**Test (TDD):**
```rust
#[test]
fn coerce_preserves_null_string() {
    let mut v = json!("null");
    coerce_json_types(&mut v);
    assert_eq!(v, json!("null")); // NOT json!(null)
}
```
**Verify:** `cargo test --workspace --lib -p nika-engine -- coerce`
**Socratic check:** Are there workflows that DEPEND on `"null"` → null coercion? Grep test fixtures for the string "null" in structured output. If found, this is a breaking change — document in CHANGELOG.

### 2.3 Transform Null Propagation Documentation
**File:** `nika-core/src/binding/transform.rs`
**Source:** Silent Bugs Agent (HIGH)
**Issue:** `length`, `keys`, `to_string`, `to_json`, `type_of` return `null` on null input. Others (`upper`, `trim`, `first`) return NIKA-153 error. Inconsistent.
**Decision:** These are INTENTIONALLY propagating (marked with `// propagating` comments). Document clearly but don't change behavior — it would break existing workflows.
**Action:** Add comment block at top of `apply()` explaining the two null-handling strategies:
```rust
// Null handling: Two strategies exist:
// 1. PROPAGATING: length, keys, to_string, to_json, type_of → return null on null input
// 2. STRICT: upper, trim, first, last, etc → return NIKA-153 error on null input
// Users should use `| default("fallback")` before strict transforms if null is possible.
```
**Verify:** No code changes needed for existing tests.

### 2.4 Artifact Write Failure Visibility
**File:** `nika-engine/src/runtime/artifact_processor.rs:164-180`
**Source:** Silent Bugs Agent (MEDIUM)
**Bug:** Artifact failures logged as warnings but task succeeds.
**Action:** Verify `ArtifactFailed` event IS emitted (it exists in the event model). If not emitted, wire it. If emitted, the issue is that the TASK doesn't fail — this is intentional design (artifact is optional). Document.
**Verify:** `cargo test --workspace --lib -p nika-engine -- artifact`

---

## Phase 3: Telemetry & Observability (~2h)

> Add missing events and tracing. Each new event = separate commit.

### 3.1 ForEachItem Events (3 new variants)
**Files:** `nika-event/src/log.rs` + `nika-engine/src/runtime/runner.rs`
**Source:** Telemetry Agent (CRITICAL) + Event System Agent
**Action:**
1. Add to EventKind enum: `ForEachItemStarted`, `ForEachItemCompleted`, `ForEachItemFailed`
2. Add serde serialization
3. Emit in runner.rs for_each loop (before/after each iteration spawn)
4. Handle in TUI event handler (at minimum, update progress counter)
**Test:** Workflow with `for_each: [1,2,3]` must produce 3 ItemStarted + 3 ItemCompleted events.
**Verify:** `cargo test --workspace --lib` (check no event handling breaks)

### 3.2 TaskCancelled Event
**File:** `nika-event/src/log.rs` + `nika-engine/src/runtime/runner.rs:684-693`
**Source:** Telemetry Agent (HIGH)
**Action:** Add `TaskCancelled` variant. Emit instead of `TaskFailed` when cancellation detected.
**Test:** Cancel workflow mid-execution → verify TaskCancelled (not TaskFailed) in events.

### 3.3 FallbackChainExhausted Event
**File:** `nika-event/src/log.rs` + `nika-engine/src/runtime/executor/mod.rs:494-530`
**Source:** Telemetry Agent (HIGH)
**Action:** Add variant, emit before returning fallback exhaustion error.

### 3.4 Cost Calculation Warning
**File:** `nika-engine/src/runtime/executor/infer.rs`
**Source:** Telemetry Agent (MEDIUM)
**Action:** `tracing::warn!("No pricing data for model {model}, cost_usd will be 0")` when cost lookup returns None.

### 3.5 StructuredOutputTimeout Event
**File:** `nika-event/src/log.rs` + `nika-engine/src/runtime/structured_output.rs:200`
**Source:** Telemetry Agent (CRITICAL)
**Action:** Emit event with timeout_secs and current layer before returning timeout error.

### 3.6 MCP Reconnection Event
**File:** `nika-event/src/log.rs` + `nika-engine/src/runtime/executor/invoke.rs`
**Source:** Telemetry Agent (CRITICAL)
**Action:** Emit `McpReconnected` after successful retry of a failed MCP connection.

**GATE after Phase 3:**
```bash
cargo test --workspace --lib     # ALL pass
# Verify new events compile and serialize correctly
```

---

## Phase 4: Edge Case Hardening (~1.5h)

### 4.1 Cancellation in Binding Resolution
**File:** `nika-engine/src/runtime/runner.rs:1965-1993`
**Source:** Edge Cases Agent (CRITICAL)
**Fix:** Check `cancel_token.is_cancelled()` in path traversal loop.

### 4.2 timeout=0 Rejection
**File:** Parser-level validation (AST or action.rs)
**Source:** Edge Cases Agent (MEDIUM)
**Fix:** Reject `timeout: 0` at parse time with clear error: "timeout must be at least 1 second".

### 4.3 for_each Item Count Limit
**File:** `nika-engine/src/runtime/runner.rs:2225`
**Source:** Edge Cases Agent (HIGH)
**Fix:** `const MAX_FOR_EACH_ITEMS: usize = 10_000;` before spawning.

### 4.4 Binding from Failed Task Warning
**File:** `nika-engine/src/runtime/runner.rs:1936-1946`
**Source:** Edge Cases Agent (MEDIUM)
**Fix:** `tracing::warn!("Binding $task.field from FAILED task — value may be error message")` when source task status is Failed.

---

## Phase 5: Workflow Factory (~3h)

> Create `tests/e2e-overnight/` with 9 categories, 65+ workflows.
> **Providers:** Rotate OpenAI (gpt-4o-mini), xAI (grok-3-fast), Gemini (gemini-2.0-flash).
> **Cost estimate:** ~$5-8 total API spend.

### Workflow Design Rules
1. **NATURAL prompts** — never mention JSON, schema, format in infer prompts
2. **Deterministic where possible** — use `temperature: 0` for structured output
3. **Self-verifying** — each workflow should produce output that can be validated programmatically
4. **Provider rotation** — cycle through providers to catch parity bugs
5. **Small and focused** — each workflow tests ONE thing, not five
6. **Real URLs** — use actual public APIs/websites, not fictional ones

### Category A: Structured Output (10 workflows)
| ID | Test | Provider | Assertion |
|----|------|----------|-----------|
| A01 | Basic object (name, age, skills) | OpenAI | All required fields, correct types |
| A02 | Nested 3 levels (company.ceo.contact.email) | xAI | Deep path accessible |
| A03 | Array of objects with minItems/maxItems | Gemini | Array length within bounds |
| A04 | Enum field (status: draft/published/archived) | OpenAI | Value in enum set |
| A05 | from_example (inline JSON sample) | xAI | Structure matches example |
| A06 | Mixed types (bool + int + number + string) | Gemini | No type coercion errors |
| A07 | for_each + structured (3 items, same schema) | OpenAI | 3 valid JSON objects in array |
| A08 | Bad prompt → force L3 retry | xAI | StructuredOutputAttempt > 1 in events |
| A09 | repair_model: gpt-4o-mini for repairs | Gemini | Repair event visible |
| A10 | Parity test: same prompt+schema on ALL 3 | ALL | All 3 produce valid JSON |

### Category B: Agent Verb (8 workflows)
| ID | Test | Provider | Assertion |
|----|------|----------|-----------|
| B01 | nika:log + nika:complete (explicit) | OpenAI | Log event + AgentComplete |
| B02 | File tools (nika:read + nika:glob) | xAI | Tool calls visible in events |
| B03 | Length guardrail (min_words: 20) | Gemini | GuardrailPassed OR retry |
| B04 | max_turns: 3 limit | OpenAI | Stops at turn 3 |
| B05 | Completion: natural mode | xAI | NaturalCompletion stop reason |
| B06 | Completion: pattern "DONE" | Gemini | PatternMatch stop reason |
| B07 | token_budget: 2000 | OpenAI | TokenBudgetExceeded or completes |
| B08 | Regex guardrail (must start with "##") | xAI | GuardrailPassed |

### Category C: Fetch 9 Extract Modes (9 workflows)
| ID | Mode | URL | Assertion |
|----|------|-----|-----------|
| C01 | markdown | httpbin.org/html | Contains markdown headers |
| C02 | article | en.wikipedia.org/wiki/Rust_(programming_language) | Has title + content |
| C03 | metadata | github.com | og:title present |
| C04 | links | news.ycombinator.com | count > 0 |
| C05 | jsonpath `$[0].name` | jsonplaceholder.typicode.com/users | Returns string |
| C06 | feed | hnrss.org/frontpage | entries array non-empty |
| C07 | text + selector "h1" | example.com | Contains "Example Domain" |
| C08 | llm_txt | docs.anthropic.com | found field present |
| C09 | response: full | httpbin.org/get | status + headers + body |

### Category D: DAG + for_each (8 workflows)
| ID | Test | Assertion |
|----|------|-----------|
| D01 | Linear A→B→C (data flows through) | C has A's output |
| D02 | Diamond A→(B,C)→D | D merges B and C |
| D03 | for_each concurrency=3, 5 items | 5 results in array |
| D04 | for_each fail_fast=false, 1 bad item | Other items succeed |
| D05 | for_each + structured output | Each item valid JSON |
| D06 | Transforms: upper, trim, join, split | Correct values |
| D07 | for_each + artifact per item | N files created |
| D08 | context: file binding | File content in template |

### Category E: Exec + Invoke (8 workflows)
| ID | Test | Assertion |
|----|------|-----------|
| E01 | Shell pipe: `echo hello \| wc -c` | Numeric output |
| E02 | env + cwd: `pwd` with cwd set | Correct directory |
| E03 | exec → infer: command output analyzed by LLM | Structured analysis |
| E04 | nika:glob *.rs → count | File list |
| E05 | nika:log + nika:assert | Events emitted |
| E06 | nika:import + nika:dimensions | Width + height |
| E07 | exec + fetch + infer pipeline | End-to-end success |
| E08 | Timeout: `sleep 10` with timeout: 2 | Timeout error |

### Category F: Media Pipeline (5 workflows)
| ID | Test | Assertion |
|----|------|-----------|
| F01 | import → dimensions → dominant_color | Color hex output |
| F02 | nika:pipeline thumbnail + optimize | Smaller file |
| F03 | nika:chart bar from JSON | SVG artifact |
| F04 | exec echo → write file → import | CAS hash |
| F05 | glob → read → LLM analysis | Analysis output |

### Category G: Security (7 workflows — ALL MUST FAIL)
| ID | Test | Expected Error |
|----|------|---------------|
| G01 | SSRF: `http://169.254.169.254/` | NIKA-045 |
| G02 | Path traversal: `../../etc/passwd` | Path error |
| G03 | Command injection: `$(whoami)` | NIKA-053 |
| G04 | Shell blocklist: `sudo ls` | NIKA-053 |
| G05 | IPv6 SSRF: `http://[::]:8080/` | NIKA-045 |
| G06 | Newline injection: `echo\nrm -rf` | NIKA-053 |
| G07 | LD_PRELOAD env var injection | Blocked |

### Category H: Real-World Use Cases (7 workflows)
| ID | Test | Provider | Assertion |
|----|------|----------|-----------|
| H01 | Blog → summarize → structured | OpenAI | title + key_points |
| H02 | API JSON → analysis → artifact | xAI | Report file |
| H03 | 3 URLs → merge research | Gemini | References all 3 |
| H04 | Read code → LLM review | OpenAI | Issues array |
| H05 | SEO audit (fetch meta → analyze) | xAI | Score field |
| H06 | Content: outline → sections → merge | Gemini | All sections |
| H07 | JSON API → transform → artifact | OpenAI | Schema-valid |

### Category T: Telemetry + NDJSON Verification (5 workflows)
| ID | Test | Assertion |
|----|------|-----------|
| T01 | for_each 3 items → ForEachItem events | 3 started + 3 completed events |
| T02 | Structured retry → StructuredOutputAttempt | Attempt count > 1 |
| T03 | Provider fallback (env var trick) | ProviderFallback event |
| T04 | exec with $env.SECRET → verify redaction | No secret in output |
| T05 | Agent guardrails → GuardrailPassed | Event present |

### Category I: Include + Context + Skills (3 workflows — NEW)
| ID | Test | Assertion |
|----|------|-----------|
| I01 | include: partial workflow import | Merged tasks execute |
| I02 | context: files binding | File content accessible in template |
| I03 | skills: prompt augmentation | Skill content in system prompt |

### Category V: Verification Workflows (5 workflows — NEW, run AFTER fixes)
| ID | Test | Assertion |
|----|------|-----------|
| V01 | Redaction: workflow with all secret types | ZERO secrets in output |
| V02 | Cross-provider parity: 3 providers same prompt | All succeed |
| V03 | Error codes: 5 error scenarios → correct NIKA-XXX | Codes match |
| V04 | Artifact: write → read → verify content | Content matches |
| V05 | Cost tracking: verify cost > 0 in events | cost_usd field present |

### Category N: Native/Local Provider (5 workflows — REQUIRES MODEL DOWNLOAD)
> **Setup:** `nika model pull llama3.2:1b` (~1GB, smallest model for testing)
> **Fallback:** If native not compiled, skip with note. Check `nika features` first.

| ID | Test | Model | Assertion |
|----|------|-------|-----------|
| N01 | Basic infer with native | llama3.2:1b | Non-empty text response |
| N02 | Structured output with native | llama3.2:1b | Valid JSON matching schema |
| N03 | for_each with native (3 items) | llama3.2:1b | 3-element array output |
| N04 | exec → native infer chain | llama3.2:1b | Exec output in prompt |
| N05 | Native + cloud mixed pipeline | native + openai | Both providers succeed |

### Category M: Multi-Provider Parity (5 workflows — $$ API SPEND)
> **Purpose:** Same prompt + schema on ALL available providers. Catch parity bugs.
> **Budget:** ~$2-3 for all 5 workflows across 4 providers.

| ID | Test | Providers | Assertion |
|----|------|-----------|-----------|
| M01 | Simple infer parity | OpenAI + xAI + Gemini + Native | All return non-empty text |
| M02 | Structured output parity (object) | OpenAI + xAI + Gemini | All return valid JSON with same schema |
| M03 | Structured output parity (array) | OpenAI + xAI + Gemini | All return array matching schema |
| M04 | Temperature=0 determinism | OpenAI + xAI + Gemini | Similar outputs (not identical, but structurally same) |
| M05 | Fan-out 4 providers → merge | ALL | All 4 results merged in final task |

### Category S: Stress Tests (5 workflows — mock provider, no API cost)
> **Purpose:** Push Nika to its limits. Find edge cases that unit tests miss.

| ID | Test | Limit Tested | Assertion |
|----|------|-------------|-----------|
| S01 | 10 chained transforms on 5KB string | Pipeline depth | No panic, bounded output |
| S02 | for_each 100 items, concurrency=10 | Semaphore, aggregation | All items in output array |
| S03 | Diamond DAG 15 tasks | Task scheduling | Each task runs exactly once |
| S04 | Template with 100+ variables | Variable binding count | All interpolated correctly |
| S05 | Deep binding path (30 levels) | Path traversal depth | Resolves or clear error |

**Total: 90 workflows** across 14 categories.

### Native Provider Setup Protocol
```bash
# Step 1: Check if native feature is compiled
nika features | grep native

# Step 2: Download smallest model for testing
nika model pull llama3.2:1b    # ~1GB, 4GB RAM minimum

# Step 3: Verify model works
nika infer "Say hello" --provider native --model llama3.2:1b

# Step 4: If above works, run Category N workflows
# If native not available, SKIP Category N with note in handoff
```

---

## Phase 6: Execute & Hunt (~3h)

> Run ALL 75 workflows. Fix bugs as found.

### Execution Order (optimized for dependency)
1. **G01-G07** (security) — verify protections FIRST, no API cost
2. **E01-E08** (exec/invoke) — no API cost (local execution)
3. **F01-F05** (media) — no API cost (local tools)
4. **C01-C09** (fetch) — no API cost (HTTP only)
5. **A01-A10** (structured output) — API cost, most important
6. **D01-D08** (DAG/for_each) — API cost, DAG patterns
7. **B01-B08** (agent) — API cost, most expensive per workflow
8. **H01-H07** (real-world) — API cost, integration tests
9. **T01-T05** (telemetry) — verify new events work
10. **I01-I03** (include/context/skills) — feature coverage
11. **V01-V05** (verification) — final validation

### Bug Hunt Protocol
```
For each workflow:
  1. nika check → must pass (or explain why not)
  2. nika run --no-live → capture full output
  3. Check exit code (0 for success workflows, non-0 for security)
  4. Verify output is LOGICALLY correct (not just non-empty)
  5. Check for leaked secrets in output
  6. Check for expected events in output
  7. If failure:
     a. Read error message carefully
     b. Trace to source code (file:line)
     c. Write failing unit test
     d. Fix code
     e. Run cargo test --workspace --lib
     f. Re-run workflow
     g. Commit: fix(scope): description
  8. Log result: workflow, provider, status, duration, cost, bugs
```

### Bug Tracker
Keep a running log in `/tmp/overnight-bugs.md`:
```markdown
| # | Workflow | Bug | File:Line | Status |
|---|---------|-----|-----------|--------|
| 1 | A03     | Schema validation ignores minItems | structured_output.rs:340 | FIXED |
```

---

## Phase 7: Provider Parity Analysis (~1h)

> After running all workflows, analyze cross-provider differences.

### 7.1 Compare A10 Results
All 3 providers ran same prompt + schema. Compare:
- Output structure identical? Types match?
- Token counts reasonable? Cost differences?
- Any provider-specific quirks?

### 7.2 Temperature Validation Audit
**File:** `nika-engine/src/provider/cost.rs:103-110` + `rig/mod.rs`
**Source:** Provider Parity Agent
**Bug:** Temperature ranges defined in cost.rs but NOT enforced in inference path.
**Action:** Document which providers accept which ranges. Consider adding clamp/warning.

### 7.3 max_tokens(8192) Inventory
**Source:** Provider Parity Agent — 8 instances (not 22)
**Action:** List all 8 locations. Create handoff with exact fix plan (per-provider defaults).

---

## Phase 8: Clippy + Final Verification (~30m)

```bash
# Clippy (zero warnings)
cd tools && cargo clippy --workspace -- -D warnings

# Full test suite
cargo test --workspace --lib

# Run 5 random workflows as smoke test
for w in A01 C05 D02 E07 H03; do
    ./tools/target/debug/nika run tests/e2e-overnight/${w}.nika.yaml --no-live
done

# Verify ALL security workflows still fail
for w in G01 G02 G03 G04 G05 G06 G07; do
    ./tools/target/debug/nika run tests/e2e-overnight/${w}.nika.yaml --no-live
    [ $? -ne 0 ] && echo "${w}: PASS (correctly failed)" || echo "${w}: SECURITY BUG"
done
```

---

## Phase 9: Compile Mega Handoffs (~1h)

> 5 session-ready handoff prompts. Each must be SELF-CONTAINED — copyable into a new Claude Code session with zero context loss.

### Handoff A: `docs/plans/handoff-sprint-security.md` (~4h)
All remaining security items with exact file:line, attack vectors, test commands.

### Handoff B: `docs/plans/handoff-sprint-agent.md` (~8h)
max_tokens defaults, LLM guardrails, scope wiring, presets, temperature validation.

### Handoff C: `docs/plans/handoff-sprint-runner.md` (~6h)
O(n^2) fix, semaphore fix, fail_fast fix, cancellation gaps, EventLog ring buffer.

### Handoff D: `docs/plans/handoff-sprint-telemetry.md` (~4h)
All new events from Phase 3 + StreamingDelta + broadcast capacity + cost tracking.

### Handoff E: `docs/plans/handoff-sprint-polish.md` (~4h)
TUI migration, Dockerfile, missing tests, documentation, CHANGELOG.

**Each handoff MUST include:**
- Context: what was done, what remains
- Exact file:line for every item
- Test commands to verify each fix
- Success criteria (how to know it's done)
- Estimated effort per item
- Priority ordering
- Dependencies (what must be done first)

---

## Master Bug & Improvement Registry

> Complete inventory of ALL known issues, from all 10 agents + Socratic analysis.

### CRITICAL (must fix before v0.54)
| # | Bug | File | Source | Status |
|---|-----|------|--------|--------|
| C01 | Newline injection in shell exec | exec.rs:284 | Security Agent | Phase 1.1 |
| C02 | IPv6 `::` SSRF bypass | policy.rs:46 | Security Agent | Phase 1.2 |
| C03 | AWS ASIA / GitHub OAuth token leak | util/mod.rs:30 | Security Agent | Phase 1.3 |
| C04 | MCP error response leaks secrets | invoke.rs:159 | Security Agent | Phase 1.4 |
| C05 | to_value_redacted() not recursive | verbs.rs/resolve.rs | Security+Binding | Phase 1.5 |
| C06 | JSON schema missing 9 fields | schemas/*.json | Prior audit | Phase 1.6 |
| C07 | BINDING_RE misses {{context.*}} | exec.rs:46 | Prior audit | Phase 1.7 |
| C08 | Secrets in tracing::warn (3 sites) | exec.rs, security.rs | Prior audit | Phase 1.8 |
| C09 | Quoted pattern bypass in blocklist | security.rs:250 | Security Agent | Phase 1.9 |
| C10 | MAX_MCP_RESULT_SIZE scope error | invoke.rs:212/351 | Compilation | Phase 0.1 |

### HIGH (fix in v0.54 sprint)
| # | Bug | File | Source | Status |
|---|-----|------|--------|--------|
| H01 | unwrap_or_default() in transforms | transform.rs:267,415 | Silent Bugs | Phase 2.1 |
| H02 | "null" string → null coercion | verbs.rs:70 | Silent Bugs | Phase 2.2 |
| H03 | ForEachItem events missing | log.rs + runner.rs | Telemetry | Phase 3.1 |
| H04 | TaskCancelled vs TaskFailed | runner.rs:684 | Telemetry | Phase 3.2 |
| H05 | Cancellation in binding resolution | runner.rs:1965 | Edge Cases | Phase 4.1 |
| H06 | for_each no item count limit | runner.rs:2225 | Edge Cases | Phase 4.3 |
| H07 | max_tokens(8192) hardcoded x8 | rig/mod.rs | Provider Parity | Handoff B |
| H08 | O(n^2) get_ready_tasks() | runner.rs:408 | Prior audit | Handoff C |
| H09 | Semaphore permit released early | runner.rs:2281 | Prior audit | Handoff C |
| H10 | Agent scope parsed not wired | mod.rs:285 | Agent Verb | Handoff B |
| H11 | LLM guardrails not implemented | thinking.rs:57 | Agent Verb | Handoff B |
| H12 | FallbackChainExhausted event | executor/mod.rs:494 | Telemetry | Phase 3.3 |

### MEDIUM (v0.54 nice-to-have)
| # | Bug | File | Source | Status |
|---|-----|------|--------|--------|
| M01 | Artifact write failures silent | artifact_processor.rs:164 | Silent Bugs | Phase 2.4 |
| M02 | timeout=0 accepted | exec.rs:96 | Edge Cases | Phase 4.2 |
| M03 | Binding from failed task silent | runner.rs:1936 | Edge Cases | Phase 4.4 |
| M04 | Cancellation in artifact write | artifact_processor.rs | Edge Cases | Phase 4.2 |
| M05 | Cost calculation silent zero | infer.rs | Telemetry | Phase 3.4 |
| M06 | StructuredOutputTimeout event | structured_output.rs:200 | Telemetry | Phase 3.5 |
| M07 | MCP reconnection event | client.rs + invoke.rs | Telemetry | Phase 3.6 |
| M08 | Temperature validation per provider | cost.rs + rig/mod.rs | Provider Parity | Handoff B |
| M09 | fail_fast=false skips cancelled | runner.rs:2594 | Prior audit | Handoff C |
| M10 | TUI ProviderName migration | lifecycle.rs:66 | Prior audit | Handoff E |
| M11 | Dockerfile VERSION=0.52.0 | Dockerfile:56 | Prior audit | Handoff E |
| M12 | EventLog O(n) drain | log.rs:1186 | Prior audit | Handoff C |
| M13 | Broadcast channel 1024 too low | log.rs:1120 | Event System | Handoff D |
| M14 | Sort/unique objects use stringify | transform.rs:323 | Transform | Document |
| M15 | Agent presets (from:) not found | Unknown | Agent Verb | Handoff B |
| M16 | Default completion mode is explicit | completion.rs:299 | Agent Verb | Document |
| M17 | Missing tests: llm_txt extract | extract.rs | Fetch/Media | Handoff E |
| M18 | Missing tests: response:full | fetch.rs | Fetch/Media | Handoff E |
| M19 | Missing tests: response:binary | fetch.rs | Fetch/Media | Handoff E |
| M20 | Context window overflow not prevented | cost.rs + rig/mod.rs | Provider Parity | Handoff B |

### LOW (v0.55+ or won't fix)
| # | Bug | File | Source |
|---|-----|------|--------|
| L01 | MaxTurnsReached dead variant | types.rs | Prior audit |
| L02 | TOCTOU symlink race in file tools | context.rs:272 | Prior audit |
| L03 | SSRF redirect DNS re-pinning | fetch.rs:378 | Prior audit |
| L04 | Null-to-empty-string in templates | template.rs:232 | Silent Bugs |
| L05 | Transform null inconsistency | transform.rs | Silent Bugs |
| L06 | Parse_json markdown edge cases | transform.rs | Transform |
| L07 | Native model discovery TODO | loader.rs:147 | Dead Code |
| L08 | LSP parse recovery TODO | recovery.rs:31 | Dead Code |
| L09 | Backoff overflow edge case | fetch.rs:446 | Silent Bugs |
| L10 | Artifact path collisions | artifact_processor.rs | Prior audit |
| L11 | Circular with: bindings (indirect) | validate.rs:184 | Prior audit |
| L12 | Mock provider doesn't load schemas | infer.rs:353 | Prior audit |

---

## Feature Coverage Matrix

> What percentage of Nika features do the workflows test?

| Feature | Workflows Testing It | Coverage |
|---------|---------------------|----------|
| infer: verb | A01-A10, B01-B08, D01-D08, H01-H07 | 100% |
| exec: verb | E01-E08, F04 | 100% |
| fetch: verb (9 modes) | C01-C09 | 100% |
| invoke: verb (builtins) | E04-E06, F01-F05 | 90% |
| agent: verb | B01-B08 | 85% |
| structured: output | A01-A10, D05, H04, H07 | 95% |
| for_each | D03-D05, D07, A07 | 90% |
| depends_on | D01-D02, D06 | 80% |
| with: bindings | All categories | 100% |
| transforms (33) | D06 | 30% (gap!) |
| artifacts | D07, H02, F03, V04 | 70% |
| context: files | D08, I02 | 50% |
| skills: | B05, I03 | 50% |
| include: | I01 | 50% |
| security | G01-G07 | 90% |
| vision/multimodal | None | 0% (gap!) |
| native provider | N01-N05 | 60% | Needs GGUF download |
| mock provider | S01-S05, Phase 0 | 50% | Stress tests |
| multi-provider parity | M01-M05 | 70% | Cross-provider comparison |
| response: binary | None directly | 0% (gap!) |
| orchestrate: | X05, Phase 0 | 20% (gap!) |
| cache system | None | 0% |
| daemon | None | 0% |

**Gaps to address in future sprints:** vision, response:binary, orchestrate E2E, cache, daemon.

---

## Success Criteria

### Phase Gates (each must pass before next phase)
- [ ] Phase 0: Build green, uncommitted changes committed
- [ ] Phase 1: 10 security fixes committed, G01-G05 all fail correctly
- [ ] Phase 2: 4 silent bug fixes committed
- [ ] Phase 3: 6 new events committed, existing tests pass
- [ ] Phase 4: 4 edge case fixes committed
- [ ] Phase 5: 90 workflows created, all pass `nika check`
- [ ] Phase 5b: Native model downloaded and tested (or skipped with note)
- [ ] Phase 6: 70+ workflows run successfully, bugs fixed
- [ ] Phase 7: Cross-provider analysis documented
- [ ] Phase 8: Clippy clean, smoke tests pass
- [ ] Phase 9: 5 handoff prompts written

### Final Verification
- [ ] `cargo test --workspace --lib` passes (9000+ tests)
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] ALL security workflows (G01-G07) fail correctly
- [ ] 5 random non-security workflows pass
- [ ] No secrets in ANY workflow output
- [ ] All fixes committed with proper messages + co-authors
- [ ] Pushed to main
- [ ] CHANGELOG updated

---

## 10-Agent Audit Sources

1. **Telemetry Gaps** — 20 findings (4 CRITICAL, 5 HIGH, 11 MEDIUM)
2. **Silent Bug Detection** — 10 bugs (3 CRITICAL, 3 HIGH, 2 MEDIUM)
3. **Event System Completeness** — 71 events inventoried, 100% emitted, 3 key gaps
4. **Security Deep Scan** — 3 CRITICAL, 3 HIGH, 6 MEDIUM findings
5. **Provider Parity** — Feature matrix, 8 hardcoded max_tokens, temperature gaps
6. **Workflow Edge Cases** — Cancellation, timeout, for_each limits, binding safety
7. **Dead Code Audit** — Codebase CLEAN, 3 active TODOs
8. **Transform & Template** — 33 transforms, shell escaping secure, 2 medium gaps
9. **Agent Verb** — LLM guardrails stubbed, scope not wired, limits enforced
10. **Fetch & Media Pipeline** — All 9 modes complete, CAS blake3, test gaps

## Socratic Review Notes

**Questions that shaped v3:**
1. The plan had NO recovery procedures → added Autonomous Operation Protocol
2. No verification loops → added Socratic Verification after each step
3. Missing features: include, context, skills, vision → added Categories I, V
4. Phase ordering wrong (should fix compilation first) → added Phase 0
5. No NDJSON verification → added verification protocol
6. No feature coverage matrix → added to track gaps
7. No bug registry → added Master Bug Registry (42 items)
8. Security workflows should run FIRST (no API cost) → reordered execution
9. Plan should be self-contained for autonomous operation → added all protocols
10. Handoffs need structure requirements → added template
