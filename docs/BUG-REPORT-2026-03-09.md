# Nika Workflow Bug Report — 2026-03-09

Systematic testing of `nika new` templates (30 workflows, tier1-6).

## Summary

| ID | Bug | Severity | Status |
|----|-----|----------|--------|
| #1 | Path validation rejects `/tmp/` | Medium | Documented |
| #2 | `exec:` without `shell: true` fails with shell operators | High | Documented |
| #3 | morning-briefing uses wrong MCP tool name | High | Documented |
| #4 | Builtin tools require absolute paths | Medium | Documented |
| #5 | Templates use `$targets` instead of `$inputs.targets` | High | Documented |
| #6 | Artifacts reject absolute paths | Low | Design Choice |
| #8 | `output.schema` silently ignored | Critical | Documented |

## Features Verified Working

| Feature | Status |
|---------|--------|
| `for_each` with literal arrays | ✅ Works |
| `for_each` with complex objects | ✅ Works |
| `for_each: $inputs.xxx` binding | ✅ Works |
| `{{inputs.name}}` template | ✅ Works |
| `context:` file loading | ✅ Works |
| Artifact templates `{{date}}` | ✅ Works |
| Agent builtin tools | ✅ Works (with retry) |

---

## Bug Details

### BUG #1: Path Validation Rejects `/tmp/`

**Error:**
```
[NIKA-204] Path '/tmp/nika-test/file.txt' is outside working directory
```

**Impact:** Cannot write files outside project directory, even to standard temp locations.

**Affected:**
- `invoke: nika:write` with absolute paths
- `invoke: nika:read` with absolute paths

**Reproduction:**
```yaml
tasks:
  - id: write
    invoke:
      tool: nika:write
      params:
        file_path: /tmp/test.txt
        content: "Hello"
```

**Root Cause:** `src/core/security.rs` validates paths against working directory.

---

### BUG #2: `exec:` Without `shell: true` Fails With Shell Operators

**Behavior:** Commands with `>`, `&&`, `|` print literally instead of being interpreted.

**Output:**
```
Workflow executed at: 2026-03-09 00:11:10 > /tmp/nika-output.txt && echo Saved
```

**Expected:** File written, "Saved" echoed.

**Affected:** All tier1+ templates using `exec:` without explicit `shell: true`.

**Root Cause:** v0.15.0 security change - `shell: false` is default.

**Fix Required:** Update templates to add `shell: true` where operators are used:
```yaml
exec:
  command: "date > output.txt && echo Done"
  shell: true  # REQUIRED for shell operators
```

---

### BUG #3: Wrong MCP Tool Names in Templates

**Error:**
```
[NIKA-102] MCP tool 'perplexity_search_web' call failed: Tool perplexity_search_web not found
```

**Correct tool name:** `perplexity_search`

**Affected:** `WORKFLOW_21_MORNING_BRIEFING` in `tier6.rs`

**Fix:** Change `perplexity_search_web` → `perplexity_search`

---

### BUG #4: Builtin Tools Require Absolute Paths Within Workdir

**Error:**
```
[NIKA-211] Path must be absolute: ./test-output.txt
```

**Then:**
```
[NIKA-204] Path '/test-output.txt' is outside working directory
```

**Required format:** `/full/path/to/workdir/file.txt`

**Impact:** Templates using relative paths don't work.

**Affected:**
- `WORKFLOW_9_TEST_INVOKE_BUILTIN` uses `./test-output.txt`
- Any workflow using relative paths with builtins

**Fix:** Convert relative to absolute in templates OR fix runtime to accept relative paths.

---

### BUG #5: Templates Use Wrong Binding Syntax

**Error:** Deadlock when using `for_each: $targets` with `inputs:`

**Correct syntax:** `for_each: $inputs.targets`

**Affected:** Multiple tier5 templates:
- `WORKFLOW_12_CONTENT_LOCALIZATION`
- `WORKFLOW_17_PR_REVIEW_BOT`
- `WORKFLOW_23_COMPETITOR_ANALYSIS`
- `WORKFLOW_30_NEWSLETTER_CURATOR`

**Fix:** Change `$targets` → `$inputs.targets` in all affected templates.

---

### BUG #6: Artifacts Reject Absolute Paths

**Error:**
```
[NIKA-280] Artifact path error: Absolute paths are not allowed in artifact output
```

**Status:** Design choice for sandboxing.

**Behavior:** Artifacts written to `.nika/artifacts/` prefix automatically.

**Documentation:** Update templates to use relative paths for artifacts.

---

### BUG #8: `output.schema` Silently Ignored

**Behavior:** Schema defined but:
1. LLM not instructed to return JSON
2. Output not validated against schema
3. No error when output doesn't match

**Expected:**
- LLM prompted to return JSON matching schema
- Output parsed and validated
- Retry loop on validation failure (per v0.19.x spec)

**Affected:** All templates with `output.schema`:
- `WORKFLOW_11_CODE_REVIEW_PIPELINE`
- `WORKFLOW_13_SEO_CONTENT_GENERATOR`
- `WORKFLOW_18_MEETING_PROCESSOR`
- `WORKFLOW_20_KNOWLEDGE_EXTRACTOR`
- `WORKFLOW_22_SOCIAL_MEDIA_PLANNER`
- `WORKFLOW_25_MEAL_PLANNER`

**Root Cause:** `output.schema` parsed by AST but not wired in runtime.

**Fix Required:** Wire schema validation in `executor.rs` or `rig_agent_loop.rs`.

---

## Priority Fixes

### Critical
1. **BUG #8** — `output.schema` not implemented despite being documented

### High
2. **BUG #2** — Update templates with `shell: true`
3. **BUG #3** — Fix MCP tool names
4. **BUG #5** — Fix `$targets` → `$inputs.targets`

### Medium
5. **BUG #1, #4** — Path validation improvements (accept `/tmp/` or relative paths)

---

## Test Files

All test workflows available at: `/tmp/nika-test/`

```
foreach-literal.nika.yaml       # Works
foreach-verify-binding.nika.yaml # Works
inputs-foreach-objects.nika.yaml # Works
context-test.nika.yaml          # Works
artifact-relative.nika.yaml     # Works
agent-builtins.nika.yaml        # Works (with retries)
output-schema.nika.yaml         # BUG #8 demo
```
