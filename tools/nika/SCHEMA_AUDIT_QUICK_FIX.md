# Schema Audit - Quick Fix Guide

## Issues Found: 7 Actionable Items

---

## 1. ADD: `description` Field to Workflow (CRITICAL)

**File:** `src/ast/workflow.rs`
**Action:** Add field to both `WorkflowRaw` and `Workflow` structs

```rust
// In struct WorkflowRaw (line ~85)
pub description: Option<String>,

// In struct Workflow (line ~123)
pub description: Option<String>,

// Update deserialization (line ~176)
description: raw.description,
```

**Why:** Schema defines it but code ignores it. Enables workflow documentation.

---

## 2. ADD: `workflow` Field to Workflow (CRITICAL)

**File:** `src/ast/workflow.rs`
**Action:** Add field to both `WorkflowRaw` and `Workflow` structs

```rust
// In struct WorkflowRaw (line ~85)
pub workflow: Option<String>,

// In struct Workflow (line ~123)
pub workflow: Option<String>,

// Update deserialization (line ~176)
workflow: raw.workflow,
```

**Why:** Schema defines it but code ignores it. Provides semantic workflow identification.

---

## 3. FIX: `for_each` Template Regex (HIGH)

**File:** `schemas/nika-workflow.schema.json`
**Location:** Line 206
**Current:**
```json
"pattern": "^\\{\\{(use|inputs|context)\\.[a-z][a-z0-9_.]*\\}\\}$"
```

**Replace with:**
```json
"pattern": "^\\{\\{(use|inputs|context)\\.[a-z][a-z0-9]*(?:\\.[a-z][a-z0-9]*)*\\}\\}$"
```

**Why:** Current pattern allows invalid paths like `{{use.field..nested}}`. New pattern enforces proper nesting.

---

## 4. UPDATE: Model Examples (MEDIUM)

**File:** `schemas/nika-workflow.schema.json`
**Location:** Line 35 (in `"model"` definition)

**Current:**
```json
"examples": ["claude-sonnet-4-6", "gpt-4o-mini", "gemini-2.0-flash"]
```

**Replace with:**
```json
"examples": ["claude-sonnet-4-20250514", "gpt-4o", "gemini-2.0-flash"]
```

**Why:** `claude-sonnet-4-6` is deprecated. Latest is `claude-sonnet-4-20250514`.

---

## 5. ENHANCE: `inputs` Documentation (MEDIUM)

**File:** `schemas/nika-workflow.schema.json`
**Location:** Lines 95-99

**Current:**
```json
"inputs": {
  "type": "object",
  "description": "Workflow input parameters (v0.10+)",
  "additionalProperties": {}
}
```

**Replace with:**
```json
"inputs": {
  "type": "object",
  "description": "Workflow input parameters (v0.10+). Accessible via {{inputs.name}} in templates. Can be simple values or objects with type/default/description/enum properties.",
  "additionalProperties": {
    "oneOf": [
      { "type": "string" },
      { "type": "number" },
      { "type": "boolean" },
      { "type": "array" },
      {
        "type": "object",
        "properties": {
          "type": { "type": "string" },
          "default": {},
          "description": { "type": "string" },
          "enum": { "type": "array" }
        }
      }
    ]
  },
  "examples": [
    {
      "locale": { "type": "string", "default": "en-US", "description": "Target locale" },
      "count": { "type": "number", "default": 10 }
    }
  ]
}
```

---

## 6. CLARIFY: Tool Naming in `invoke` (MEDIUM)

**File:** `schemas/nika-workflow.schema.json`
**Location:** Lines 507-509

**Current:**
```json
"tool": {
  "type": "string",
  "description": "Tool name to call (mutually exclusive with resource)"
}
```

**Replace with:**
```json
"tool": {
  "type": "string",
  "description": "Tool name: use 'nika:X' for builtins (nika:read, nika:write, nika:edit, nika:glob, nika:grep, nika:log, nika:emit, nika:assert, nika:prompt, nika:run, nika:sleep) or bare tool name for MCP server tools"
}
```

---

## 7. CLARIFY: Flow Endpoints (LOW)

**File:** `schemas/nika-workflow.schema.json`
**Location:** Lines 797-802

**Current:**
```json
"source": {
  "$ref": "#/$defs/FlowEndpoint",
  "description": "Source task(s)"
}
```

**Replace with:**
```json
"source": {
  "$ref": "#/$defs/FlowEndpoint",
  "description": "Source task(s) that must complete. Use string for single task ID or array for multiple (e.g., [a, b])"
}
```

---

## 8. BONUS: Fix CLAUDE.md Examples (DOC)

**File:** `nika/CLAUDE.md`
**Location:** Skill Merging section (v0.15.1)

**Current (WRONG):**
```yaml
skills:
  - path: ./skills/writing.md
    alias: writing
  - path: pkg:@spn/core@1.0.0/skills/coding.md
    alias: coding
```

**Change to (CORRECT):**
```yaml
skills:
  writing: ./skills/writing.md
  coding: pkg:@spn/core@1.0.0/skills/coding.md
```

**Why:** Skills are an object/map, not an array. Schema defines this correctly; documentation was misleading.

---

## Testing

After fixes, validate with:

```bash
# Validate schema syntax
jsonschema -i workflow.nika.yaml schemas/nika-workflow.schema.json

# Test workflow with new fields
cat > test.nika.yaml << 'EOF'
schema: nika/workflow@0.10
workflow: test-schema-audit
description: "Testing schema audit fixes"
provider: claude
tasks:
  - id: test-builtin
    invoke:
      tool: nika:read
      params:
        file_path: "./data.json"
inputs:
  locale:
    type: string
    default: en-US
EOF

# Run workflow
cargo run -- test.nika.yaml
```

---

## Completion Checklist

- [ ] Add `description` field to Workflow struct
- [ ] Add `workflow` field to Workflow struct
- [ ] Fix `for_each` regex pattern
- [ ] Update model examples
- [ ] Enhance `inputs` documentation
- [ ] Clarify tool naming description
- [ ] Clarify flow endpoint description
- [ ] Fix CLAUDE.md skill examples
- [ ] Run tests: `cargo test`
- [ ] Validate schema syntax
- [ ] Create commit: `fix(schema): audit fixes for v0.27.0`

