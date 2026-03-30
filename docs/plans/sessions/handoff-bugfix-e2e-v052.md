# Handoff — Bug Fixes + 40 E2E Tests (Post v0.52.0)

> Generated: 2026-03-30 | Base: v0.52.0 + 13 commits | Tests: 8,957 | 0 clippy
> Parent: `3c29d4220 docs: definitive master plan v2`
> Status: **UNCOMMITTED** — 8 files changed, ~1000 lines added

---

## Master Prompt

```bash
claude --model opus -p "$(cat docs/plans/sessions/handoff-bugfix-e2e-v052.md)"
```

You are continuing a session on the Nika workflow engine (Rust, `tools/` workspace).
This session fixed 5 bugs from the mega bug report and added 40 E2E workflow tests.

**All changes are uncommitted.** Your first task: review the diff, commit per git-workflow
rules (1 fix = 1 commit), then address the open items below.

### What was done

| # | Commit scope | Files | Summary |
|---|-------------|-------|---------|
| 1 | `fix(structured): wire infer_callback to L0a/L0b` | `executor/infer.rs` | Created `l0_infer_callback` before L0 block, chained `.with_infer_callback()` on both safety net engines. L3 (retry) + L4 (repair) now enabled in all 4 code paths. |
| 2 | `fix(transforms): default() triggers on empty strings` | `nika-core/binding/transform.rs` | Added `Value::String(s) if s.is_empty()` arm. +1 test. |
| 3 | `fix(output): harden JSON fence stripping` | `runtime/output.rs` | Extracted `try_extract_fenced_json()` helper. Added uppercase `JSON`, Windows `\r\n`, space-after-fence. +2 tests. |
| 4 | `fix(provider): clamp max_tokens to minimum 16` | `provider/rig/mod.rs` | `.max(16)` on all 5 user-facing unwrap sites. OpenAI rejects < 16. |
| 5 | `fix(examples): use inputs: instead of shell ${} syntax` | 2 YAML files | `competitive-analysis` + `content-audit` now use `inputs:` + `{{inputs.url}}`. |
| 6 | `test(e2e): 40 real workflow E2E tests` | `runtime/tests_e2e_workflow.rs` + `runtime/mod.rs` | 889 lines. 8 categories x 5 tests. Full YAML -> parse_analyzed -> Runner -> verify. |

### What was NOT fixed (deferred bugs)

| Bug | Severity | Reason deferred |
|-----|----------|-----------------|
| BUG-4: YAML anchors `&`/`*` not supported | LOW | `marked-yaml` parser limitation. Requires parser swap to `saphyr`. |
| BUG-6: NIKA-053 false positive on echo | LOW | Security filter too aggressive. Needs pattern analysis without weakening security. |
| BUG-8: Agent `LowConfidence(0.0)` | INFO | Cosmetic. Confidence calc ignores `nika:complete`. Zero user impact. |

---

## Architecture: Structured Output Pipeline (5-Layer Defense)

```
                        run_infer()
                            |
                  policy.is_structured()?
                       YES |
                            v
               +--- SCHEMA RESOLUTION ---+
               | policy.schema OR        |
               | policy.from_example     |
               +-------------------------+
                            |
            +-- l0_infer_callback created --+  <-- BUG-1 FIX
            |   (Arc<InferCallback>)        |
            +-------------------------------+
                            |
         +------------------+------------------+
         |                                     |
  supports_native_                     does NOT support
  structured_output()?                 (Claude, Gemini,
  (OpenAI, Groq, xAI)                  Mistral, Native)
         |                                     |
         v                                     v
  +-- LAYER 0a --+                      +-- LAYER 0b --+
  | response_    |                      | tool_        |
  | format       |                      | injection    |
  | (native)     |                      | (Dynamic-    |
  +--------------+                      | SubmitTool)  |
  infer_stream_                         +--------------+
  with_options()                        infer_with_tools()
         |                                     |
         v                                     v
  +----------------------------------------------------+
  |  StructuredOutputEngine.validate()                  |
  |  NOW WITH l0_infer_callback.clone()                 |
  |  -> L2: extract + validate (always)                 |
  |  -> L3: retry with feedback (needs infer_fn) [NEW]  |
  |  -> L4: LLM repair (needs infer_fn) [NEW]          |
  +----------------------------------------------------+
         |
  SUCCESS -> return validated JSON
  FAILURE -> fall through to streaming
         |
         v
  +-- STREAMING PATH --+
  | infer_stream_with_ |
  | options()          |
  +--------------------+
         |
         v
  +----------------------------------------------------+
  |  StructuredOutputEngine.validate()                  |
  |  WITH infer_callback (main path, always had it)     |
  |  WITH repair_callback (optional, for repair_model)  |
  |  -> L2 -> L3 -> L4 (full defense)                  |
  +----------------------------------------------------+
         |
         v
  +-- RUNNER FALLBACK (line 1145) --+
  |  NO InferCallback (defensive)   |
  |  L2 only. "shouldn't happen"    |
  +----------------------------------+
```

### InferCallback Creation Map

| Location | Variable | Available To | Has repair_fn |
|----------|----------|-------------|---------------|
| `infer.rs:484` | `l0_infer_callback` | L0a + L0b safety nets | No |
| `infer.rs:1023` | `infer_callback` | Main streaming path | No |
| `infer.rs:1060` | `repair_callback` | Main path L4 only | Yes (cheaper model) |
| `runner.rs:1145` | NONE | Runner fallback | No |

---

## Architecture: Provider Abstraction (6 Infer Entry Points)

| Method | max_tokens | Clamped? | Used by |
|--------|-----------|----------|---------|
| `infer()` | Hardcoded 8192 | N/A (always >= 16) | InferCallback for L3/L4 retries |
| `infer_with_options()` | User opt, default 8192 | `.max(16)` YES | Main infer path |
| `infer_stream_with_options()` | User opt, default 8192 | `.max(16)` YES | Streaming path |
| `infer_with_tools()` | User opt, default 8192 | `.max(16)` YES | L0b tool injection |
| `infer_vision()` | User opt, default 8192 | `.max(16)` YES | Vision content |
| `infer_vision_stream()` | User opt, default 8192 | `.max(16)` YES | Vision streaming |

**Risk found**: `infer()` (used by InferCallback) hardcodes 8192. If a task sets `max_tokens: 16000` and L3/L4 retry fires, the retry is silently capped at 8192. Not critical for now (schemas rarely need >8K output) but worth noting.

---

## Architecture: Transform Pipeline (31 ops)

| Category | Transforms | Null handling |
|----------|-----------|---------------|
| String (5) | upper, lower, trim, trim_start, trim_end | NIKA-153 error |
| Collection (12) | length, first, last, keys, values, flatten, reverse, sort, unique, compact, first(N), last(N) | Mixed (length returns 0, others error) |
| Type (5) | to_string, to_number, to_bool, to_json, parse_json | Mixed |
| Numeric (4) | round(N), abs, ceil, floor | Error |
| Utility (5) | default(V), type_of, join(S), split(S), shell | default: null+empty->fallback |

**`default()` semantic change**: Now treats BOTH `null` AND `""` (empty string) as "needs fallback". No other transform conflates null and empty string. This is intentional for workflow ergonomics but should be documented.

---

## Architecture: JSON Extraction (Shared Codepath)

| Path | Trigger | Uses `extract_json()` | Schema validation | Retry |
|------|---------|----------------------|-------------------|-------|
| `make_task_result()` | `output: { format: json }` | YES | Optional | No |
| `StructuredOutputEngine` | `structured: { schema }` | YES (Layer 2) | Always | L3+L4 |

Both paths share `extract_json_from_output()` from `output.rs`. No divergence risk.

**Extraction strategies** (in order):
1. Direct `serde_json::from_str` (fast path)
2. `\`\`\`json` fence (case-insensitive: json, JSON)
3. `\`\`\`` fence (Unix `\n`, Windows `\r\n`, space)
4. Bracket matching `{...}` / `[...]` with depth tracking
5. Greedy fallback: first `{` to last `}`

---

## E2E Test Coverage (40 tests)

### What's covered

| Category | Tests | Verbs | Pattern |
|----------|-------|-------|---------|
| DAG patterns | 5 | exec | sequential, diamond, parallel, tree, wide fan-out/fan-in |
| Data flow | 5 | exec | with bindings, `??` fallback, inputs, multi-binding, chained |
| Exec verb | 5 | exec | basic, shell pipe, env vars, templates, multiline |
| Infer mock | 5 | infer | basic, template, chain, system prompt, temperature |
| for_each | 5 | exec | sequential, concurrent, as variable, upstream array, fail_fast |
| Transforms | 5 | exec | upper/trim, length, first/last, to_json, default |
| Error cases | 5 | mixed | cycle, missing schema, exec fail, missing dep, duplicate ids |
| Edge cases | 5 | mixed | unicode, empty output, single task, 10-task pipeline, events |

### What's NOT covered (gaps for next session)

| Gap | Priority | Why it matters |
|-----|----------|----------------|
| **fetch verb** | HIGH | 0 tests. 9 extract modes untested at E2E level. |
| **invoke verb** | HIGH | 0 tests. Builtin tools (nika:*) untested. |
| **agent verb** | MEDIUM | 0 tests. Multi-turn, guardrails, completion modes. |
| **structured output** | HIGH | 0 tests. The killer feature has zero E2E YAML coverage. |
| **retry with backoff** | MEDIUM | 0 tests. `retry: { max_attempts, delay_ms, backoff }` untested. |
| **artifacts** | LOW | 0 tests. File persistence not verified. |
| **error propagation** | HIGH | Upstream failure -> NIKA-026 not tested through YAML. |
| **for_each + infer** | MEDIUM | All for_each tests use exec, not infer. |
| **output: json** | MEDIUM | JSON output format directive not tested. |
| **env var bindings** | LOW | `$env.VAR` in with: blocks untested. |
| **parametric transforms** | LOW | `join(", ")`, `split(",")`, `default("X")` untested. |

### Weak assertions to strengthen

| Test | Issue |
|------|-------|
| `e2e_infer_mock_temperature` | `!output.is_empty()` proves nothing about temperature propagation |
| `e2e_for_each_with_as_variable` | `contains("one")` is trivially true, doesn't verify `as:` worked |
| `e2e_transform_upper_lower_trim` | `\|\|` fallback could mask transform failure |
| `e2e_for_each_fail_fast_false` | All items succeed, so `fail_fast: false` is never exercised |

---

## Issues Discovered During Review

### Code Issues (found by rust-pro + rust-architect)

| # | Severity | Location | Issue |
|---|----------|----------|-------|
| 1 | HIGH | `infer.rs` callback | `infer()` hardcodes max_tokens=8192 for L3/L4 retries. Task's `max_tokens:` not propagated. |
| 2 | MEDIUM | `infer.rs` | `run_infer()` is ~1100 lines. L0a/L0b should be extracted to methods. |
| 3 | MEDIUM | `infer.rs` | InferCallback construction duplicated 3 times (l0, main, repair). Needs factory. |
| 4 | MEDIUM | `infer.rs` | L0 schema resolution duplicates `StructuredOutputEngine.load_schema()`. `strict` mode diverges. |
| 5 | LOW | `rig/mod.rs:810` | Style inconsistency: `v.max(16) as u64` vs `v.max(16)).map(u64::from)` elsewhere. |
| 6 | LOW | `transform.rs` | Missing test: `default("X")` on whitespace-only `" "` (should NOT trigger). |
| 7 | LOW | `output.rs` | Missing test: fence-with-space variant `"\`\`\` "`. |
| 8 | INFO | `content-audit.nika.yaml` | Duplicate comment header (cosmetic). |
| 9 | INFO | Both example YAMLs | `max_tokens: 200` too low for multi-section prompts. Missing `extract:` on fetch. |

### Architectural Risks

| Risk | Impact | Mitigation |
|------|--------|-----------|
| `infer.rs` God Method | Every change risks side effects | Extract L0a, L0b, streaming into methods |
| 7+ match blocks per provider | Adding a provider = 10+ coordinated changes | Consider trait-based dispatch |
| Vision bypasses structured output | No structured+vision combo possible | Document limitation or implement |
| Runner fallback has no InferCallback | L3/L4 dead code in fallback path | Intentional, but add comment |

---

## Suggested Next Steps (Priority Order)

### Immediate (this session's leftovers)

1. **Commit** the 8 files as 3-4 logical commits per git-workflow rules
2. **Fix** content-audit.nika.yaml duplicate header + add `extract: article` to both fetch tasks
3. **Add** missing edge case tests (whitespace default, fence-with-space)

### Next Session

4. **10 more E2E tests** covering structured output, fetch, invoke, retry, error propagation
5. **Refactor** InferCallback into a factory method on TaskExecutor
6. **Extract** L0a/L0b into separate methods in `infer.rs`
7. **Re-test** structured output with real providers (OpenAI/xAI) to confirm BUG-1 fix

### Backlog

8. **BUG-4**: Evaluate `saphyr` as YAML parser replacement for anchor support
9. **BUG-6**: Audit NIKA-053 patterns — whitelist echo or use AST-aware matching
10. **Propagate max_tokens** through InferCallback for L3/L4 retries

---

## File Map

```
MODIFIED:
  tools/nika-engine/src/runtime/executor/infer.rs     +30 lines (BUG-1: l0_infer_callback)
  tools/nika-core/src/binding/transform.rs            +10 lines (BUG-2: empty string default)
  tools/nika-engine/src/runtime/output.rs             +35 lines (BUG-3: fence stripping)
  tools/nika-engine/src/provider/rig/mod.rs           +5 lines  (BUG-5: max_tokens clamp)
  tools/nika/examples/use-cases/competitive-analysis.nika.yaml  (BUG-7: inputs:)
  tools/nika/examples/use-cases/content-audit.nika.yaml         (BUG-7: inputs:)
  tools/nika-engine/src/runtime/mod.rs                +2 lines  (register test module)

NEW:
  tools/nika-engine/src/runtime/tests_e2e_workflow.rs  889 lines (40 E2E tests)
```

---

## Known State

```
Branch:   main (uncommitted changes)
Tests:    8,957 passed (8,917 before session)
Clippy:   0 warnings
Build:    clean
Providers working: openai, xai, gemini (rate limited)
Providers broken:  anthropic (billing), mistral/groq/deepseek (no keys)
```
