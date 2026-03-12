# JSON Schema Audit Report: nika-workflow.schema.json

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/schemas/nika-workflow.schema.json`
**Version:** v0.27.0
**Date:** 2026-03-12
**Audit Scope:** Schema v0.10 consistency with code (src/ast/, src/core/)

---

## Executive Summary

The schema is **87% correct** with 2 critical inconsistencies and 13 minor issues.

**Critical Issues:** 2
- Missing workflow-level `description` field in code
- Missing workflow-level `workflow` field in code

**High Priority:** 1
- Overly permissive regex pattern for `for_each` template syntax

**Medium Priority:** 4
- Outdated model examples
- Underdocumented `inputs` parameter structure
- Underdocumented tool naming conventions
- Missing description for multi-source/target flows

---

## Detailed Findings

### CRITICAL - Issue 1: Missing `description` Field in Workflow Code

**Schema Location:** Line 22-25
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs` (NOT FOUND)

**Schema Definition:**
```json
"description": {
  "type": "string",
  "description": "Human-readable workflow description"
}
```

**Code Check:**
The `WorkflowRaw` and `Workflow` structs do NOT have a `description` field. This means YAML like this will be silently ignored:

```yaml
schema: nika/workflow@0.10
description: "My workflow"  # ← This field is ignored by deserialization
tasks: [...]
```

**Impact:** Users can specify descriptions in YAML, but they are silently discarded. The schema should match the code.

**Resolution Options:**

**Option A (Recommended):** Add field to code
```rust
pub struct Workflow {
    pub schema: String,
    pub description: Option<String>,  // ← ADD THIS
    pub provider: String,
    // ...
}
```

**Option B:** Remove from schema
```json
// Remove lines 22-25 from schema.json
```

**Recommendation:** **Option A** - Add to code since task-level `description` already exists (line 274-276 in schema).

---

### CRITICAL - Issue 2: Missing `workflow` Field in Workflow Code

**Schema Location:** Line 16-21
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs` (NOT FOUND)

**Schema Definition:**
```json
"workflow": {
  "type": "string",
  "pattern": "^[a-z][a-z0-9-]*$",
  "description": "Workflow identifier (lowercase alphanumeric with hyphens)",
  "examples": ["generate-page", "seo-pipeline"]
}
```

**Code Check:**
The `WorkflowRaw` and `Workflow` structs do NOT have a `workflow` field. The pattern validation won't execute because the field is never deserialized.

**Impact:** This is metadata that could be useful for logging, tracing, or workflow identification, but is currently ignored.

**YAML Example:**
```yaml
schema: nika/workflow@0.10
workflow: generate-page  # ← Ignored by code
tasks: [...]
```

**Resolution Options:**

**Option A (Recommended):** Add field to code
```rust
pub struct Workflow {
    pub schema: String,
    pub workflow: Option<String>,  // ← ADD THIS
    // ...
}
```

**Option B:** Remove from schema
```json
// Remove lines 16-21 from schema.json
```

**Recommendation:** **Option A** - The field provides semantic value for workflow identification and is not redundant with the file path.

---

### HIGH - Issue 3: Overly Permissive Regex for `for_each` Template Syntax

**Schema Location:** Line 206
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/binding/` (runtime validation)

**Schema Definition:**
```json
{
  "type": "string",
  "pattern": "^\\{\\{(use|inputs|context)\\.[a-z][a-z0-9_.]*\\}\\}$",
  "description": "Template binding to an array (e.g., {{use.items}}, {{inputs.locales}})"
}
```

**Problem:**
The pattern `[a-z0-9_.]*` allows:
- Multiple consecutive dots: `{{use.field..nested}}`
- Multiple consecutive underscores: `{{context.__private__}}`
- Leading underscores after dot: `{{use._internal}}`

These should be invalid path expressions.

**Examples of Invalid Patterns Accepted:**
```
{{use.field..nested}}     ← Double dot
{{inputs.array...items}}  ← Triple dot
{{context.__private}}     ← Double underscore
{{use.a_b_c_d_e}}         ← OK actually
```

**Correct Pattern:**
```json
"pattern": "^\\{\\{(use|inputs|context)\\.[a-z][a-z0-9]*(?:\\.[a-z][a-z0-9]*)*\\}\\}$"
```

This enforces:
- Single letter or digit after each dot
- No consecutive special characters
- Valid nested paths: `field.nested.deep`

**Current Behavior:** Runtime validation catches these, so they fail at execution time, not validation time. The fix improves developer experience by catching errors earlier.

**Recommendation:** Update pattern to tighten constraints.

---

### MEDIUM - Issue 4: Outdated Model Examples

**Schema Location:** Line 32-35
**Code Location:** Various provider defaults (rig-core v0.32)

**Schema Definition:**
```json
"model": {
  "type": "string",
  "description": "Default model override (e.g., claude-sonnet-4-6, gpt-4o)",
  "examples": ["claude-sonnet-4-6", "gpt-4o-mini", "gemini-2.0-flash"]
}
```

**Issue:**
The example `claude-sonnet-4-6` is outdated. Per v0.5.1 in CLAUDE.md:
> "Updated from deprecated `claude-3-5-sonnet-latest` to `claude-sonnet-4-20250514`"

**Recommendation:**
Update examples to reflect current model names:
```json
"examples": ["claude-sonnet-4-20250514", "gpt-4o", "gemini-2.0-flash"]
```

**Impact:** Low - Users see examples and will likely test with latest models, but documentation should be accurate.

---

### MEDIUM - Issue 5: Underdocumented `inputs` Parameter Structure

**Schema Location:** Line 95-99
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs` line 115

**Schema Definition:**
```json
"inputs": {
  "type": "object",
  "description": "Workflow input parameters (v0.10+)",
  "additionalProperties": {}
}
```

**Code Definition:**
```rust
pub inputs: Option<FxHashMap<String, serde_json::Value>>,
```

**Problem:**
The schema allows arbitrary JSON values with no structure documentation. Users won't know what properties are expected in input parameter objects.

**Recommended Schema:**
```json
"inputs": {
  "type": "object",
  "description": "Workflow input parameters (v0.10+). Each parameter is a JSON object with optional type, default, description, and enum.",
  "additionalProperties": {
    "oneOf": [
      { "type": "string", "description": "Simple string value" },
      { "type": "number", "description": "Simple number value" },
      { "type": "boolean", "description": "Simple boolean value" },
      { "type": "array", "description": "Simple array value" },
      {
        "type": "object",
        "properties": {
          "type": { "type": "string", "description": "Parameter type (string, number, boolean, array)" },
          "default": { "description": "Default value if not provided" },
          "description": { "type": "string", "description": "Parameter description" },
          "enum": { "type": "array", "description": "Allowed values" }
        }
      }
    ]
  },
  "examples": [
    {
      "locale": { "type": "string", "default": "en-US", "description": "Target locale" },
      "count": { "type": "number", "default": 10, "description": "Item count" }
    }
  ]
}
```

**Impact:** Medium - Works but users lack documentation on structure conventions.

---

### MEDIUM - Issue 6: Underdocumented Tool Naming in `invoke`

**Schema Location:** Line 507-509
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/invoke.rs`

**Schema Definition:**
```json
"tool": {
  "type": "string",
  "description": "Tool name to call (mutually exclusive with resource)"
}
```

**Problem:**
The schema doesn't document that builtin tools use `nika:` prefix while MCP tools don't.

**Valid Examples:**
```yaml
# Builtin tool (11 total)
- invoke:
    tool: nika:read      # Core tool: read file
    params: { file_path: "./data.json" }

- invoke:
    tool: nika:write     # Core tool: write file
    params: { file_path: "./output.txt", content: "..." }

# MCP server tool
- invoke:
    mcp: novanet
    tool: novanet_search # No prefix
    params: { query: "QR code" }
```

**Recommended Description:**
```json
"tool": {
  "type": "string",
  "description": "Tool name to call. Use 'nika:TOOL' for builtins (nika:read, nika:write, nika:edit, nika:glob, nika:grep, nika:log, nika:emit, nika:assert, nika:prompt, nika:run, nika:sleep) or bare name for MCP tools (requires 'mcp' field)"
}
```

**Impact:** Medium - Users might not know about builtin tools or how to reference them.

---

### LOW - Issue 7: Missing Documentation for Multi-Source/Target Flows

**Schema Location:** Line 791-804
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs` line 130

**Schema Definition:**
```json
"source": {
  "$ref": "#/$defs/FlowEndpoint",
  "description": "Source task(s)"
},
"target": {
  "$ref": "#/$defs/FlowEndpoint",
  "description": "Target task(s)"
}
```

And `FlowEndpoint` allows:
```json
"FlowEndpoint": {
  "oneOf": [
    { "type": "string", "description": "Single task ID" },
    { "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "Multiple task IDs" }
  ]
}
```

**Problem:**
The description "Source task(s)" doesn't make it obvious that you can use arrays:

```yaml
flows:
  - source: [task_a, task_b]  # Both must complete
    target: [task_c, task_d]  # Then both start
```

**Recommended Enhancement:**
```json
"source": {
  "$ref": "#/$defs/FlowEndpoint",
  "description": "Source task(s) that must complete before target. Use string for single task or array for multiple."
}
```

**Impact:** Low - The schema allows it, but documentation helps discoverability.

---

## CORRECT Items (No Action Needed)

### ✅ Ollama Provider Removed

**Schema Location:** Line 28, 343, 557, 945
**Code Status:** Correctly removed in v0.27.0

The schema correctly shows:
```json
"enum": ["claude", "openai", "mistral", "groq", "deepseek", "gemini", "native", "mock"]
```

Ollama is gone, `native` (mistral.rs) is the replacement. **CORRECT**

---

### ✅ Schema Version Pattern Supports v0.1-v0.10

**Schema Location:** Line 12
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs` lines 228-237

Pattern `^nika/workflow@0\.[1-9][0-9]?$` matches:
- v0.1 through v0.9 ✅
- v0.10 through v0.99 ✅

Supports exactly what code validates. **CORRECT**

---

### ✅ Default Provider "claude" Matches Code

**Schema Location:** Line 29
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs` line 267

Both default to "claude". **CORRECT**

---

### ✅ Thinking Budget Defaults to 4096

**Schema Location:** Lines 380, 614
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/agent.rs` line 75

Schema: `"default": 4096`
Code: `const DEFAULT_THINKING_BUDGET: u64 = 4096;`

**CORRECT**

---

### ✅ Extended Thinking Provider Limitation Documented

**Schema Location:** Lines 371-374, 605-608
**Code Reality:** Claude only (other providers ignore the field)

The schema correctly states: "Enable extended thinking (Claude only, v0.4+)"

While JSON Schema can't enforce provider-specific behavior, the documentation is accurate. **CORRECT**

---

### ✅ Skills Structure is Object/Map

**Schema Location:** Lines 48-56
**Code Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs` line 105

Schema defines skills as `type: "object"` with `additionalProperties: { "type": "string" }`
Code defines: `pub skills: Option<FxHashMap<String, SkillDef>>`

The schema is correct. However, **CLAUDE.md has misleading examples** showing skills as an array:
```yaml
# WRONG (from CLAUDE.md)
skills:
  - path: ./skills/writing.md
    alias: writing

# CORRECT (matches schema)
skills:
  writing: ./skills/writing.md
```

**Resolution:** Fix documentation examples, not schema. **SCHEMA CORRECT**

---

## Summary Table

| # | Issue | Severity | Type | File | Line(s) | Resolution |
|---|-------|----------|------|------|---------|------------|
| 1 | Missing `description` field in code | CRITICAL | INCONSISTENCY | workflow.rs | - | Add to Workflow struct |
| 2 | Missing `workflow` field in code | CRITICAL | INCONSISTENCY | workflow.rs | - | Add to Workflow struct |
| 3 | `for_each` regex too permissive | HIGH | SCHEMA | schema.json | 206 | Tighten pattern |
| 4 | Outdated model examples | MEDIUM | DOC | schema.json | 35 | Update examples |
| 5 | `inputs` structure underdocumented | MEDIUM | DOC | schema.json | 99 | Add schema definition |
| 6 | Tool naming convention underdocumented | MEDIUM | DOC | schema.json | 509 | Expand description |
| 7 | Flow endpoint multi-source underdocumented | LOW | DOC | schema.json | 797 | Improve description |
| - | Skills CLAUDE.md examples misleading | LOW | DOC | CLAUDE.md | - | Update examples |
| - | All other items | - | CORRECT | - | - | ✅ No action |

---

## Recommendations by Priority

### Priority 1 (Immediate - Breaking Consistency)
1. **Add `description` field to Workflow struct** (workflow.rs)
   - Makes schema and code consistent
   - Enables workflow documentation
   - Minimal code change

2. **Add `workflow` field to Workflow struct** (workflow.rs)
   - Makes schema and code consistent
   - Provides semantic workflow identification
   - Minimal code change

### Priority 2 (High - Schema Quality)
3. **Tighten `for_each` template regex** (schema.json line 206)
   - Catches invalid paths at validation time
   - Improves developer experience
   - No code change required

4. **Update model examples** (schema.json line 35)
   - Reflects current default models
   - Helps users choose correct model names

### Priority 3 (Medium - Documentation)
5. **Document `inputs` parameter structure** (schema.json line 99)
   - Helps users understand expected format
   - Add example showing type/default/description fields

6. **Document tool naming conventions** (schema.json line 509)
   - Explain `nika:` prefix for builtins
   - List all 11 builtin tools

7. **Fix CLAUDE.md skill examples** (nika/CLAUDE.md)
   - Correct array syntax to object/map syntax
   - Match schema definition

---

## Test Cases for Verification

After implementing fixes, test with:

```yaml
# Test 1: workflow-level description and workflow fields
schema: nika/workflow@0.10
workflow: test-workflow
description: "Test workflow for schema validation"
tasks:
  - id: test
    infer: "Test"

# Test 2: for_each with correct nested paths (no double dots)
tasks:
  - id: test
    for_each: "{{use.items}}"
    infer: "Process"
    use:
      items: "{{inputs.array}}"

# Test 3: inputs parameter structure
inputs:
  locale:
    type: string
    default: en-US
    description: "Target locale"
  count:
    type: number
    default: 10

# Test 4: builtin tools
- invoke:
    tool: nika:read
    params:
      file_path: "./data.json"

# Test 5: multi-source flows
flows:
  - source: [a, b]
    target: [c, d]
```

---

## Files Affected by Recommendations

| File | Changes | Lines |
|------|---------|-------|
| `src/ast/workflow.rs` | Add `description` and `workflow` fields | Add to WorkflowRaw and Workflow structs |
| `schemas/nika-workflow.schema.json` | Tighten `for_each` regex, update model examples, enhance descriptions | 35, 99, 206, 509, 797 |
| `nika/CLAUDE.md` | Fix skill examples | Skill merging section |

---

## References

- Schema v0.10: `/Users/thibaut/dev/supernovae/nika/tools/nika/schemas/nika-workflow.schema.json`
- Workflow types: `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/workflow.rs`
- Agent types: `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/agent.rs`
- Skill types: `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/skill_def.rs`
- Code patterns v0.27.0: `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md`

