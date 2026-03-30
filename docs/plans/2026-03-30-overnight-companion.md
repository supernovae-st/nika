# Overnight Companion — Deep Knowledge Base

> Companion to `2026-03-30-overnight-mega-plan.md` (v3).
> Contains all detailed findings from 16 audit agents (10 + 6 follow-up).
> Reference this for exact file:line, code snippets, and attack payloads.

---

## 1. Concrete Rust Tests to Add (from Rust Pro Agent)

### 1.1 Redaction Idempotency
```rust
#[test]
fn redact_secrets_is_idempotent() {
    let inputs = [
        "key=sk-proj-abc123def456ghi789jkl",
        "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij",
        "postgres://user:pass@host:5432/db",
        "AKIAIOSFODNN7EXAMPLE",
    ];
    for input in inputs {
        let once = redact_secrets(input);
        let twice = redact_secrets(&once);
        assert_eq!(once, twice, "NOT idempotent for: {input:?}");
    }
}
```

### 1.2 Secrets Inside JSON Values
```rust
#[test]
fn redact_secrets_inside_json_values() {
    let json = r#"{"headers":{"Authorization":"Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abc"}}"#;
    let result = redact_secrets(json);
    assert!(result.contains("[REDACTED]"));
    assert!(!result.contains("eyJhbGci"));
}
```

### 1.3 SSRF RFC1918 Boundary Exhaustive
```rust
#[test]
fn ssrf_blocks_all_rfc1918_boundaries() {
    let blocked = ["10.0.0.0", "10.255.255.255", "172.16.0.0", "172.31.255.255",
                   "192.168.0.0", "192.168.255.255", "169.254.0.1", "100.64.0.1"];
    for ip in blocked { assert!(is_ssrf_blocked(ip), "Should block: {ip}"); }

    let allowed = ["11.0.0.1", "172.15.0.1", "172.32.0.1", "192.167.0.1", "100.63.0.1"];
    for ip in allowed { assert!(!is_ssrf_blocked(ip), "Should allow: {ip}"); }
}
```

### 1.4 Transform Chain Edge Cases
```rust
#[test]
fn chain_default_split_sort_join() {
    let expr = TransformExpr::parse("default('c,a,b') | split(',') | sort | join(',')").unwrap();
    assert_eq!(expr.apply(&Value::Null).unwrap(), json!("a,b,c"));
    assert_eq!(expr.apply(&json!("z,x,y")).unwrap(), json!("x,y,z"));
}

#[test]
fn chain_flatten_compact_unique_length() {
    let expr = TransformExpr::parse("flatten | compact | unique | length").unwrap();
    let input = json!([["a", "b", null], ["b", "c", ""], ["a", null]]);
    assert_eq!(expr.apply(&input).unwrap(), json!(3));
}

#[test]
fn chain_on_empty_array_no_panic() {
    for chain in ["sort | unique | join(',')", "first", "last", "flatten | compact", "length"] {
        let expr = TransformExpr::parse(chain).unwrap();
        let _ = expr.apply(&json!([])); // Must not panic
    }
}
```

### 1.5 Unicode Security Bypass
```rust
#[test]
fn blocklist_rejects_fullwidth_unicode_sudo() {
    let fullwidth = "\u{FF53}\u{FF55}\u{FF44}\u{FF4F} rm -rf /tmp"; // ｓｕｄｏ
    assert!(check_blocklist(fullwidth).is_err());
}

#[test]
fn blocklist_rejects_zero_width_space_sudo() {
    let zwsp = "s\u{200B}u\u{200B}d\u{200B}o rm -rf /tmp";
    assert!(check_blocklist(zwsp).is_err());
}
```

### 1.6 for_each Output Array Regression
```rust
#[tokio::test]
async fn for_each_single_item_still_array() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: single
    for_each: ["only_one"]
    as: item
    exec: "echo {{with.item}}"
"#;
    let runner = run_yaml(yaml).await;
    let result = runner.datastore().get("single").unwrap();
    let parsed: Value = serde_json::from_str(result.output_str()).unwrap_or(json!(result.output_str()));
    assert!(parsed.is_array(), "Single-item for_each must produce array, got: {parsed:?}");
}
```

### 1.7 Event Log Assertions (new macro)
```rust
macro_rules! assert_has_event {
    ($events:expr, $pattern:pat) => {
        assert!(
            $events.iter().any(|e| matches!(&e.kind, $pattern)),
            "Expected event matching {} but none found in {} events",
            stringify!($pattern), $events.len()
        );
    };
}

// Usage:
assert_has_event!(events, EventKind::TaskCompleted { task_id, .. } if task_id.as_ref() == "extract");
assert_has_event!(events, EventKind::WorkflowStarted { .. });
```

---

## 2. Complete Limits Inventory (from Stress Test Agent)

### Critical Constants

| Constant | Value | File | Impact |
|----------|-------|------|--------|
| MAX_EVENTS | 10,000 | nika-event/log.rs | Events silently dropped in long workflows |
| CHANNEL_CAPACITY | 256 | nika-daemon/events.rs | Too low for heavy for_each |
| MAX_OUTPUT_SIZE | 50 MB | run_context.rs | Task output truncated silently |
| AGGREGATE_TIMEOUT_SECS | 600 | structured_output.rs | 10min cap on structured validation |
| INVOKE_TASK_DEADLINE | 300s | constants.rs | 5min total per MCP task |
| EXEC_TIMEOUT | 60s | constants.rs | Shell command timeout |
| FETCH_TIMEOUT | 30s | constants.rs | HTTP request timeout |
| INFER_TIMEOUT | 120s | constants.rs | LLM call timeout |
| MAX_STORE_SIZE | 100 MB | store.rs | CAS file limit |
| MAX_IMAGE_DIM | 10,000 px | safety.rs | Image processing limit |
| MAX_LINKS | 5,000 | extract_links.rs | Links extraction cap |
| MAX_MATCHES | 1,000 | css_select.rs | CSS selector results cap |
| DEFAULT_MAX_TURNS | 10 | agent.rs | Agent loop default |
| MAX_ALLOWED_TURNS | 100 | agent.rs | Agent loop hard cap |
| DEFAULT_DEPTH_LIMIT | 3 | agent.rs | Sub-agent nesting |
| MAX_DEPTH_LIMIT | 10 | agent.rs | Sub-agent hard cap |
| MAX_INCLUDE_DEPTH | 10 | import_loader.rs | YAML include nesting |
| REDIRECT_LIMIT | 5 | constants.rs | HTTP redirect following |
| MAX_HTML_SIZE | 10 MB | tools/*.rs | HTML processing limit |
| MAX_BASE64_INPUT | 100 MB | processor.rs | Before base64 decode |

### MISSING Limits (Bugs!)
- **No for_each item count limit** — can OOM with 100K+ items
- **No template variable count limit** — unbounded memory
- **No template string size limit** — unbounded regex processing
- **Broadcast channel 256** — too low for heavy for_each (should be 4096+)

---

## 3. Validation Pipeline (from Validation Agent)

### Parse → Analyze → Execute Flow
```
YAML Input
  ↓
Phase 1: YAML Schema Validation (marked_yaml)
  → LoadError on syntax errors
  ↓
Phase 2: Raw AST Parsing (parser.rs)
  → NIKA-160-164 on field errors
  ↓
Phase 3: Analysis (analyze.rs)
  → NIKA-140-155 on semantic errors
  → DAG cycle detection (NIKA-143/020)
  → Binding validation (NIKA-150-155)
  → Model requirement (NIKA-034)
  ↓
Runtime Execution
  → Template resolution (NIKA-041-042)
  → Security checks (NIKA-053)
  → Provider validation (NIKA-030-039)
  → Guardrails (NIKA-112)
```

### Error Code Ranges
| Range | Module | Purpose |
|-------|--------|---------|
| 001-009 | Workflow | General workflow errors |
| 010-019 | Schema | Schema validation |
| 020-029 | DAG | Cycle detection, dependency |
| 030-039 | Provider | API keys, models, endpoints |
| 040-049 | Binding | Template/binding resolution |
| 050-059 | Security | Command injection, SSRF |
| 100-109 | Validation | Strict mode |
| 110-119 | Execution | Guardrails |
| 140-155 | Analysis | AST semantic validation |
| 160-164 | Parser | YAML syntax errors |
| 200+ | Tools | File, media, artifacts |

### Mock Provider Behavior
- Returns deterministic JSON: `{"mock": true, "task_id": "...", "name": "mock_value", ...}`
- **Structured output**: Uses `generate_mock_json(&schema)` — generates valid JSON from schema
- **Agent verb**: Works — returns mock JSON per turn
- **for_each**: Works — each iteration gets same mock response
- **Vision**: Returns `{"vision": true, "image_count": N}` (no processing)
- **Failure simulation**: `NIKA_MOCK_FAIL_COUNT=3` makes first 3 calls fail

### `nika check` vs `nika run --dry-run`
| Aspect | check | dry-run |
|--------|-------|---------|
| Syntax validation | Yes | Yes |
| DAG validation | Yes | Yes |
| Model required check | No | Yes |
| MCP tool signatures | Only with --strict | No |
| Template resolution | Parsed, not resolved | Partially resolved |
| Provider API keys | Not tested | Validated |
| Can pass but fail at runtime? | YES | Unlikely |

---

## 4. Advanced Feature Coverage (from Features Agent)

### include: (Partial Workflows)
- **Max depth**: 10 (MAX_INCLUDE_DEPTH)
- **Task prefix**: `prefix: "seo_"` + task `analyze` → `seo_analyze`
- **No input override** — included tasks inherit parent's provider/model
- **Test gap**: No transitive include test, no prefix collision test

### context: (File Bindings)
- **Load timing**: Parse time for glob, execution time for I/O
- **No max file size** — loaded entirely into memory (BUG!)
- **Binary files**: Will fail with UTF-8 decode error
- **Templates in context**: Not evaluated — loaded as-is
- **Test gap**: No binary file test, no large file test

### skills: (Prompt Augmentation)
- **Injection point**: Prepended BEFORE base system prompt
- **Max size**: No explicit limit (BUG!)
- **No transitive skills** — each must be self-contained
- **Test gap**: No pkg: URI test, no large skill file test

### orchestrate: (Multi-Workflow)
- **System prompt**: Includes goal, task list, YAML syntax, confidence target
- **Cost control**: max_cost_usd → agent limits.max_cost_usd
- **Rounds**: max_rounds × 3 → agent max_turns
- **Test gap**: No multi-round test, no cost enforcement test

### record: (Compression)
- **Cheap model**: claude-haiku-4-5 (Anthropic), gpt-4.1-mini (OpenAI)
- **Max tokens**: 500 default, 4096 max
- **Confidence**: 0.0-1.0 from compression LLM
- **Fallback**: Truncation if compression fails
- **Test gap**: No E2E compression test, no confidence filtering test

### response_format vs structured
| Aspect | response_format | structured |
|--------|----------------|-----------|
| Purpose | Format hint | Full validation |
| Schema | None | Required |
| Validation | No | 5-layer defense |
| Repair | No | Yes (Layer 4) |
| Use when | Simple JSON nudge | Strict schema |

---

## 5. Stress Test Specifications (from Stress Agent)

### S01: Transform Chain Bomb
- 10 chained transforms on 5KB string
- Tests: pipeline depth, memory, string processing
- Expected: No stack overflow, bounded output

### S02: Deep Nested Binding (50 levels)
- JSON nested 50 deep, access via $task.a.b.c...
- Tests: MAX_WALK_DEPTH (64), coalescing operator
- Expected: Success up to 64, error at 65+

### S03: for_each 200 items, concurrency=50
- Tests: Semaphore, output aggregation, timeout
- Expected: All items complete, proper concurrency limiting

### S04: Template 200+ Variables
- 200 unique {{with.*}} in one template
- Tests: Variable binding count, hash collisions
- Expected: All variables interpolate correctly

### S05: Diamond DAG 20 Tasks
- Complex cross-dependencies, conditional execution
- Tests: No duplicate execution, cycle detection
- Expected: Each task runs exactly once

### S06: Retry Storm (3 failures → success)
- Tests: Exponential backoff, INVOKE_TASK_DEADLINE
- Expected: Proper backoff timing, success on 4th attempt

### S07: Large Fetch (9MB HTML)
- Near MAX_HTML_SIZE, 5000+ links
- Tests: FETCH_TIMEOUT, processing limits
- Expected: Link cap at 5000, CSS cap at 1000

### S08: Concurrent Artifact Writes (50 parallel)
- for_each with 50 items writing to artifact dir
- Tests: File handle exhaustion, race conditions
- Expected: All 50 files created atomically

### S09: Agent 20 Turns with Tools
- max_turns=20, tool_choice=required, extended_thinking
- Tests: Turn limit, token budget, thinking budget
- Expected: Graceful stop at limit

### S10: Multi-Provider Fan-Out
- 3 providers in parallel, structured output, merge
- Tests: Provider fallback, schema parity, aggregation
- Expected: All 3 succeed, merge produces valid JSON

---

## 6. Autonomous Session Protocol (from Methodology Agent)

### Checkpointing Strategy
After every 3-5 commits, create a checkpoint file:
```bash
echo "Phase: X.Y | Tests: PASS | Commits: N | Time: $(date)" >> /tmp/overnight-checkpoint.log
```

### Recovery Without Human Intervention
1. **Test failure after fix**: Read error → understand → fix → re-test. Never blindly change test.
2. **Compilation failure**: Most likely missing import or scope issue. Read error message carefully.
3. **Provider 429**: Wait 60s, retry. Switch provider if persistent.
4. **50+ tests break**: `git stash` → rethink approach. Maybe fix is wrong.
5. **Context window full**: Commit all work, push, write handoff note.

### Extended Thinking Usage
- **USE for**: Architecture decisions, security fix design, multi-file refactoring planning
- **DON'T USE for**: Routine test writing, simple bug fixes, file edits

### Parallel Agent Strategy
- **Independent fixes**: Launch 3 worktree agents for unrelated bug fixes
- **Test + Fix**: One agent fixes, another runs tests continuously
- **Code review**: After each phase, launch review agent to catch issues

### Progress Tracking
Maintain `/tmp/overnight-progress.md`:
```markdown
## Phase 1: Security
- [x] 1.1 Newline injection — FIXED (commit abc123)
- [x] 1.2 IPv6 SSRF — FIXED (commit def456)
- [ ] 1.3 SECRET_RE — IN PROGRESS
```

---

## 7. Property-Based Testing Gaps (from Rust Pro)

### Missing proptest targets:
1. **Transform chain composition** — Random chain of 2-5 transforms on random values (must not panic)
2. **Secret redaction idempotency** — `redact(redact(x)) == redact(x)` for all strings
3. **SSRF IP classification** — Arbitrary IPv4/IPv6 addresses (must not panic, all RFC1918 blocked)
4. **Template injection** — Values containing `{{` must NOT be re-evaluated
5. **JSON schema validation** — Arbitrary schema + arbitrary JSON (must not panic)

### Fuzzing targets (cargo-fuzz):
1. Template parser (`template_resolve`)
2. Transform expression parser (`TransformExpr::parse`)
3. Security command validator (`validate_command_string`)
4. URL/SSRF checker (`is_ssrf_blocked`)
5. JSON schema validator (structured output)

---

## 8. New Workflow Categories (from Complex Workflow + Features Agents)

### Category S: Stress Tests (10 workflows)
Based on stress test specifications above. Use mock provider where possible.

### Category X: Advanced Features (5 workflows)
| ID | Test | Feature |
|----|------|---------|
| X01 | include: with prefix | Partial workflow import |
| X02 | context: file bindings + glob | File context |
| X03 | skills: injection in agent | Prompt augmentation |
| X04 | record: compression | Output compression |
| X05 | orchestrate: with mock | Multi-workflow orchestration |

### Category P: Property/Regression (5 workflows)
| ID | Test | Property |
|----|------|----------|
| P01 | Redaction idempotency | exec echo $SECRET → verify NDJSON |
| P02 | for_each always array | Single item → still array output |
| P03 | Transform null safety | All 33 transforms on null input |
| P04 | Binding from failed task | Downstream gets error string |
| P05 | Template injection | Value with {{}} not re-evaluated |

---

## 9. Feature Coverage Matrix (Updated)

| Feature | Current Coverage | After Plan | Gap |
|---------|-----------------|------------|-----|
| infer: | 100% | 100% | None |
| exec: | 100% | 100% | None |
| fetch: (9 modes) | 90% | 100% | llm_txt test |
| invoke: (builtins) | 80% | 95% | Media tools |
| agent: | 70% | 90% | LLM guardrails, presets |
| structured: | 90% | 95% | Repair model |
| for_each | 85% | 95% | Item limit, single-item |
| depends_on | 80% | 90% | Failed task binding |
| transforms (33) | 70% | 90% | Chain edge cases |
| artifacts | 60% | 80% | Concurrent writes |
| context: | 20% | 60% | Binary, large file |
| skills: | 20% | 50% | pkg: URI |
| include: | 10% | 50% | Transitive |
| orchestrate: | 10% | 40% | Multi-round |
| record: | 0% | 30% | Compression E2E |
| vision | 0% | 0% | No available provider |
| native provider | 0% | 0% | Needs GGUF model |
| response_format | 0% | 30% | vs structured |
| security | 80% | 95% | Unicode bypass |
| SSRF | 70% | 95% | RFC1918 boundaries |
| redaction | 50% | 90% | Idempotency, JSON |
