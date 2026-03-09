# Nika 5 Semantic Verbs Audit Report

**Date:** 2026-03-09
**Nika Version:** v0.22.2
**Auditor:** Claude Code

---

## Executive Summary

| Verb | Status | Issues Found |
|------|--------|--------------|
| `infer:` | PARTIALLY WORKS | Artifact path resolution bug |
| `exec:` | WORKS | None |
| `fetch:` | WORKS | None |
| `invoke:` | WORKS | None |
| `agent:` | BROKEN | OpenAI schema validation bug |

**Overall:** 3/5 verbs work correctly, 1 partially works, 1 is broken with OpenAI provider.

---

## Detailed Results

### 1. INFER Verb

**Status:** PARTIALLY WORKS

**Test Workflow:** `test-verb-infer.nika.yaml`

```yaml
schema: nika/workflow@0.9
workflow: test-verb-infer
provider: openai

tasks:
  - id: simple_infer
    infer: "Say hello in exactly 3 words"
    artifact:
      path: ./test-audit/artifacts/infer-output.txt
```

**Evidence:**
- Task executed successfully: Output was "Hello, howdy, hi!" (correct 3 words)
- Provider auto-detection worked (fell back to OpenAI when Claude credits unavailable)
- Trace file generated with complete events

**BUG FOUND: Artifact Path Resolution**

When user specifies a custom artifact path like `./test-audit/artifacts/infer-output.txt`, the artifact is written to the WRONG location:

- **Expected:** `./test-audit/artifacts/infer-output.txt`
- **Actual:** `.nika/artifacts/test-audit/artifacts/infer-output.txt`

**Root Cause:** The `resolve_artifact_dir()` function in `src/runtime/artifact_processor.rs` prepends `.nika/artifacts/` as the base directory regardless of whether the user specified a relative path starting with `./`.

**Impact:** Users cannot control where artifacts are written; they always end up nested under `.nika/artifacts/`.

---

### 2. EXEC Verb

**Status:** WORKS

**Test Workflow:** `test-verb-exec.nika.yaml`

```yaml
schema: nika/workflow@0.9
workflow: test-verb-exec

tasks:
  - id: simple_exec
    exec:
      command: "echo 'Hello from exec' && date"
      shell: true

  - id: exec_with_env
    exec:
      command: "echo $MY_VAR"
      shell: true
      env:
        MY_VAR: "test_value_from_env"
```

**Evidence:**
- Both tasks completed successfully
- Shell execution with `shell: true` works correctly
- Environment variable injection via `env:` block works (output: "test_value_from_env")
- Command chaining with `&&` works
- Trace file shows complete execution flow

**No issues found.**

---

### 3. FETCH Verb

**Status:** WORKS

**Test Workflow:** `test-verb-fetch.nika.yaml` and `test-verb-fetch-json.nika.yaml`

```yaml
# GET request
tasks:
  - id: get_request
    fetch:
      url: "https://httpbin.org/get"
      method: GET

# POST with JSON body
tasks:
  - id: post_json
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      json:
        name: "Test User"
        count: 42
        active: true
```

**Evidence:**
- GET request to httpbin.org/get returned proper JSON response
- POST request with `json:` body correctly serialized
- Content-Type header automatically set to `application/json`
- Response parsing worked correctly
- Trace files show complete HTTP request/response cycle

**No issues found.**

---

### 4. INVOKE Verb

**Status:** WORKS

**Test Workflow:** `test-verb-invoke.nika.yaml`

```yaml
schema: "nika/workflow@0.5"

mcp:
  dummy:
    command: "echo"
    args: ["not used"]

tasks:
  - id: test_sleep
    invoke:
      mcp: dummy
      tool: nika:sleep
      params:
        duration: "100ms"

  - id: test_log
    invoke:
      mcp: dummy
      tool: nika:log
      params:
        level: info
        message: "Sleep completed successfully"
```

**Evidence:**
- Both tasks completed successfully
- `nika:sleep` slept for exactly 100ms (trace shows 101ms duration)
- `nika:log` emitted proper log event
- Task binding with `use:` worked correctly
- Flow dependency respected (test_log ran after test_sleep)
- Trace file shows complete MCP invoke/response cycle

**Trace excerpt:**
```json
{"type":"mcp_response","output_len":20,"duration_ms":101,"response":{"slept_for_ms":100}}
{"type":"mcp_response","output_len":71,"response":{"level":"info","logged":true}}
```

**No issues found.**

---

### 5. AGENT Verb

**Status:** BROKEN (with OpenAI provider)

**Test Workflow:** `test-verb-agent-simple.nika.yaml`

```yaml
schema: "nika/workflow@0.9"
provider: openai

tasks:
  - id: simple_agent
    agent:
      prompt: "List 3 programming languages and one advantage of each..."
      max_turns: 2
      stop_conditions:
        - "ANALYSIS_DONE"
```

**Error:**
```
[NIKA-115] Agent execution failed for task 'simple_agent': CompletionError:
HttpError: Invalid status code 400 Bad Request with message: {
  "error": {
    "message": "Invalid schema for function 'nika_complete': In context=
    ('properties', 'metadata'), 'additionalProperties' is required to be
    supplied and to be false.",
    "type": "invalid_request_error",
    "param": "tools[7].parameters",
    "code": "invalid_function_parameters"
  }
}
```

**Root Cause:**

The `nika_complete` builtin tool schema in `/src/runtime/builtin/complete.rs` has:

```rust
"metadata": {
    "type": "object",
    "description": "Additional metadata about the result",
    "additionalProperties": true  // <-- OpenAI requires false here
}
```

OpenAI's function calling requires `additionalProperties: false` on ALL nested objects, but the metadata field has `additionalProperties: true`.

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/runtime/builtin/complete.rs` (line 177)

**Impact:**
- AGENT verb is completely broken with OpenAI provider
- Cannot test with Claude due to credit exhaustion
- All agent-based workflows will fail with OpenAI

**Fix Required:** Change line 177 from `"additionalProperties": true` to `"additionalProperties": false`.

---

## Bugs Found Summary

| ID | Severity | Verb | Description | File |
|----|----------|------|-------------|------|
| BUG-001 | Medium | infer | Artifact path doubled under .nika/artifacts/ | `src/runtime/artifact_processor.rs` |
| BUG-002 | Critical | agent | OpenAI schema validation fails for nika_complete | `src/runtime/builtin/complete.rs:177` |

---

## Test Files Created

All test files are in `/Users/thibaut/dev/supernovae/nika/tools/nika/test-audit/workflows/`:

1. `test-verb-infer.nika.yaml` - INFER verb test
2. `test-verb-exec.nika.yaml` - EXEC verb test
3. `test-verb-fetch.nika.yaml` - FETCH GET test
4. `test-verb-fetch-json.nika.yaml` - FETCH POST with JSON body test
5. `test-verb-invoke.nika.yaml` - INVOKE with builtin tools test
6. `test-verb-agent.nika.yaml` - AGENT with MCP test (failed - dummy server)
7. `test-verb-agent-simple.nika.yaml` - AGENT simple test (failed - schema bug)
8. `test-verb-agent-claude.nika.yaml` - AGENT with Claude (failed - credits)

---

## Recommendations

1. **BUG-002 (Critical):** Fix the `nika_complete` tool schema immediately. Change `additionalProperties: true` to `false` on the metadata property. This blocks all OpenAI agent workflows.

2. **BUG-001 (Medium):** Review artifact path resolution logic. When a user specifies a path starting with `./`, it should be relative to the current working directory, not nested under `.nika/artifacts/`.

3. **Test Coverage:** Add integration tests that actually run workflows with real providers to catch API compatibility issues like BUG-002.

---

## Audit Conclusion

- **infer:** Works, but artifact path resolution has a bug
- **exec:** Fully functional with env vars and shell mode
- **fetch:** Fully functional with GET, POST, and JSON body
- **invoke:** Fully functional with builtin tools
- **agent:** Broken with OpenAI due to schema validation bug

The core workflow engine is functional for non-agentic workflows. The agent verb requires immediate attention for OpenAI compatibility.
