# Documentation Fixes Required

This file shows the exact changes needed to fix all audit findings.

## Fix 1: Update README.md Version Badge

**File:** `tools/nika/README.md`
**Line:** 4

**Change From:**
```markdown
[![Version](https://img.shields.io/badge/version-0.16.1-blue?logo=rust&logoColor=white)](Cargo.toml)
```

**Change To:**
```markdown
[![Version](https://img.shields.io/badge/version-0.19.5-blue?logo=rust&logoColor=white)](Cargo.toml)
```

**Also Update Line 6:**
```markdown
[![Tests](https://img.shields.io/badge/tests-3358%20passing-brightgreen)](src/)
```

**Change To (after verifying actual count):**
```markdown
[![Tests](https://img.shields.io/badge/tests-3562%20passing-brightgreen)](src/)
```

**Time:** 2 minutes

---

## Fix 2: Add Two-Phase IR Architecture Section to CLAUDE.md

**File:** `tools/nika/CLAUDE.md`
**Insert After:** Line 44 (after "## Key Concepts" section)
**Before:** Line 46

**Insert This Content:**

```markdown
## v0.19.x Architecture: Two-Phase IR System

### Overview

Nika v0.19 introduces a **two-phase compilation architecture** for robust validation and better error diagnostics.

```
Phase 1 (Parsing)        Phase 2 (Validation)      Phase 3 (Execution)
┌──────────────────────┐  ┌──────────────────────┐  ┌──────────────────────┐
│  YAML File           │  │  Unvalidated IR      │  │  Validated IR        │
│  ↓                   │  │  ↓                   │  │  ↓                   │
│  marked-yaml parse   │→ │  Schema validation   │→ │  Task dispatch       │
│  (with Span track)   │  │  Semantic checks     │  │  DAG execution       │
│  ↓                   │  │  Error collection    │  │  Result collection   │
│  IR (source locs)    │  │  ↓                   │  │  ↓                   │
│                      │  │  IR (validated)      │  │  Output              │
└──────────────────────┘  └──────────────────────┘  └──────────────────────┘
```

### Phase 1: YAML Parsing

**Goal:** Parse YAML and preserve source location metadata

**Dependencies:**
- `marked-yaml` (v0.8) - YAML parser with Span (line:col) tracking
- `serde-saphyr` (v0.0.20) - Safe YAML deserialization (replaces deprecated serde_yaml)

**Output:** Unvalidated IR with source locations

**Benefits:**
- Precise error messages: `[NIKA-001] Parse error at line 42, column 15`
- "Did you mean?" suggestions via `strsim` (Levenshtein distance)
- Better debugging: exact location of problems in YAML

### Phase 2: Semantic Validation

**Goal:** Validate workflow semantics and enforce constraints

**Dependencies:**
- `jsonschema` (v0.26) - JSON schema validation
- `indexmap` (v2.7) - Preserves YAML key order
- Custom validators in `src/ast/schema_validator.rs`

**Validation Steps:**
1. Schema validation - Ensure structure matches `nika-workflow.schema.json`
2. Semantic validation - Check business rules
   - DAG cycle detection
   - Task dependencies resolution
   - Binding reference validation
3. Error aggregation - Collect all errors (not just first)

**Output:** Validated IR ready for execution

**Error Codes (v0.19):**
- NIKA-005: Schema validation failed (with error details)
- NIKA-060: Structured output validation failed
- NIKA-061: JSON schema mismatch in response

### Structured Output Enforcement (v0.19.5)

Tasks with `response_format: json` get automatic:
1. **Schema injection:** `$schema` field added to task prompt
2. **Validation retry:** Up to 3 retries if LLM doesn't return valid JSON
3. **Template variables:** Access workflow inputs with `{{inputs.field_name}}`

**Example:**
```yaml
schema: nika/workflow@0.9
inputs:
  brand_name: "SuperNovae"
  target_audience: "Developers"

tasks:
  - id: generate_content
    infer:
      prompt: "Generate JSON for {{inputs.brand_name}} targeting {{inputs.target_audience}}"
      response_format: json
      # Automatically gets:
      # - JSON schema validation
      # - Retry loop (max 3 attempts)
      # - Structured output error codes
```

**Error Codes:**
- NIKA-060: JSON validation failed (user input error)
- NIKA-061: JSON schema mismatch (provider error)

### Reference

See `docs/plans/2026-03-04-v0.19-foundation-implementation.md` for detailed implementation plan.

---
```

**Time:** 15 minutes

---

## Fix 3: Update Error Code Ranges in CLAUDE.md

**File:** `tools/nika/CLAUDE.md`
**Lines:** 1350-1360 (entire Error Codes section)

**Replace This:**
```markdown
## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Task errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Binding errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |
```

**With This:**
```markdown
## Error Codes

| Range | Category | Notes |
|-------|----------|-------|
| NIKA-000-009 | Workflow errors | Parse, schema, validation |
| NIKA-010-019 | Schema/validation errors | Schema version, task errors |
| NIKA-020-029 | DAG errors | Cycles, missing dependencies |
| NIKA-030-039 | Provider errors | Missing API keys, config |
| NIKA-040-049 | Template/binding errors | Unresolved bindings |
| NIKA-050-059 | Path/task/security errors | v0.15+: NIKA-053 BlockedCommand |
| NIKA-060-069 | Output/structured output errors | v0.19: JSON validation, response format |
| NIKA-070-079 | Use block validation errors | Use specification validation |
| NIKA-080-089 | DAG validation errors | DAG construction and validation |
| NIKA-090-099 | JSONPath/IO errors | JSONPath parsing, file operations |
| NIKA-100-109 | MCP errors | Tool calls, server connection |
| NIKA-110-119 | Agent errors | Agent execution, tool dispatch |
| NIKA-120-129 | Resilience errors | (Deprecated in v0.4) |
| NIKA-130-139 | TUI errors | Terminal UI errors |
| NIKA-200-209 | Chat/Mention errors | v0.9.1+ features |
| NIKA-210-219 | Builtin tool errors | v0.9.3+ features |
| NIKA-220-229 | DAG Panel errors | v0.9.4+ features |
| NIKA-230-239 | Session persistence errors | v0.9.5+ features |
| NIKA-240-249 | Animation/Export errors | v0.9.5+ features |
| NIKA-280-289 | Artifact errors | v0.18+: path validation, writes |

For complete error definitions, see `src/error.rs`.
```

**Time:** 10 minutes

---

## Fix 4: Add v0.20 Roadmap Section to CLAUDE.md

**File:** `tools/nika/CLAUDE.md`
**Insert After:** v0.15.0 Changes section (after line 350)
**Before:** "v0.8.0 Changes" section

**Insert This Content:**

```markdown
## v0.20.0 Roadmap (In Planning)

Planned features for v0.20.0 are documented in implementation plans:

| Feature | Documentation | Status |
|---------|---|---------|
| Artifact Validation System | `docs/plans/2026-03-04-v0.20-artifact-validation-implementation.md` | Planning |
| Core Validation Framework | `docs/plans/2026-03-04-v0.20-core-validation-implementation.md` | Planning |
| Validation System Design | `docs/plans/2026-03-04-v0.20-validation-system-design.md` | Design |

**6-Views Architecture** (v0.20 target):
- Browse: Workflow file browser
- Editor: YAML editor with syntax highlighting
- Runner: Execution monitor with real-time output
- Chat: Conversational agent interface
- Scheduler: Job scheduling and cron management
- Settings: User preferences and configuration

See `ROADMAP.md` and `docs/plans/` directory for full details.

---
```

**Time:** 10 minutes

---

## Fix 5: Clarify Schema Version Extensibility

**File:** `tools/nika/CLAUDE.md`
**Lines:** 98-107

**Replace This:**
```markdown
## Schema Versions

- `nika/workflow@0.1`: infer, exec, fetch verbs
- `nika/workflow@0.2`: +invoke, +agent verbs, +mcp config
- `nika/workflow@0.3`: +for_each parallelism, rig-core integration
- `nika/workflow@0.5`: +decompose, +lazy bindings, +spawn_agent (MVP 8)
- `nika/workflow@0.6`: +multi-provider support (6 providers)
- `nika/workflow@0.7`: +full streaming for all providers
- `nika/workflow@0.8`: +Studio DX (edit history, sessions, themes, config)
- `nika/workflow@0.9`: +context: file loading, +include: DAG fusion (v0.14.3)
```

**With This:**
```markdown
## Schema Versions

Current version: **@0.9**. Schema is extensible to @0.99 via JSON schema pattern validation.

### Documented Versions

- `nika/workflow@0.1`: infer, exec, fetch verbs
- `nika/workflow@0.2`: +invoke, +agent verbs, +mcp config
- `nika/workflow@0.3`: +for_each parallelism, rig-core integration
- `nika/workflow@0.5`: +decompose, +lazy bindings, +spawn_agent (MVP 8)
- `nika/workflow@0.6`: +multi-provider support (6 providers)
- `nika/workflow@0.7`: +full streaming for all providers
- `nika/workflow@0.8`: +Studio DX (edit history, sessions, themes, config)
- `nika/workflow@0.9`: +context: file loading, +include: DAG fusion (v0.14.3)

### Future Versions

Versions @0.10 through @0.99 are reserved for future features. The JSON schema pattern validation allows any version in this range:

```yaml
schema: nika/workflow@0.10  # Valid (reserved for future features)
```

See `schemas/nika-workflow.schema.json` for pattern: `^nika/workflow@0\.[1-9][0-9]?$`
```

**Time:** 5 minutes

---

## Fix 6: Verify and Update Test Count

**File:** `tools/nika/README.md` and `tools/nika/CLAUDE.md`

**Step 1:** Verify actual test count
```bash
cd tools/nika && cargo test --lib 2>&1 | grep "test result"
```

**Step 2:** Update README.md badge
Current (Line 6):
```markdown
[![Tests](https://img.shields.io/badge/tests-3358%20passing-brightgreen)](src/)
```

Change to (use actual count from Step 1):
```markdown
[![Tests](https://img.shields.io/badge/tests-XXXX%20passing-brightgreen)](src/)
```

**Step 3:** Update CLAUDE.md
Current (Line 7):
```markdown
**Current version:** v0.19.5 | Structured Output + Artifacts + Security | 3,562 tests | Zero clippy warnings
```

Verify this matches actual count. Update if needed.

**Also update** Line 448-449:
```markdown
### Statistics
- **4,369 tests passing** (v0.15.0 total - up from 2,997 in v0.12.0)
```

Change to:
```markdown
### Statistics
- **3,562 tests passing** (v0.19.5 total - optimized from 4,369 in v0.15.0)
```

**Time:** 5 minutes

---

## Fix 7: Update Workspace-Level CLAUDE.md (Nice to Have)

**File:** `nika/.claude/CLAUDE.md`
**Line:** Around TUI documentation

**Update From:**
```markdown
| `tui/` | Terminal UI | ✓ (v0.16.2: 6-Views, Chat DAG, ARMADA) |
```

**Update To:**
```markdown
| `tui/` | Terminal UI | ✓ (v0.19.5: Studio DX, Two-Phase IR, Structured Output) |
```

**Time:** 2 minutes

---

## Summary of Changes

| Fix # | File | Lines | Time | Priority |
|-------|------|-------|------|----------|
| 1 | README.md | 4, 6 | 2 min | CRITICAL |
| 2 | CLAUDE.md | +80 | 15 min | CRITICAL |
| 3 | CLAUDE.md | 1350-1360 | 10 min | CRITICAL |
| 4 | CLAUDE.md | +20 | 10 min | HIGH |
| 5 | CLAUDE.md | 98-107 | 5 min | HIGH |
| 6 | README.md + CLAUDE.md | 6, 7, 448-449 | 5 min | HIGH |
| 7 | nika/.claude/CLAUDE.md | TUI section | 2 min | MEDIUM |

**Total Time:** ~49 minutes

---

## Verification Checklist

After applying fixes, verify:

- [ ] README.md shows v0.19.5
- [ ] Test count badge matches actual count
- [ ] Two-Phase IR section is clear and accurate
- [ ] All 20 error code ranges documented
- [ ] v0.20 roadmap is visible
- [ ] Schema extensibility note added
- [ ] No broken links in documentation
- [ ] CLAUDE.md passes spell check

---

## Next Steps

1. Create a branch: `git checkout -b docs/v0.19.5-alignment`
2. Apply fixes 1-7 in order
3. Verify documentation changes: `grep -r "v0.19.5" tools/nika/`
4. Run validation: `cargo test --doc`
5. Commit: `git commit -m "docs: align v0.19.5 documentation"`
6. Create PR for review

---

**Estimated Total Time:** ~60 minutes (with testing)
**Files Modified:** 3 main files + 1 workspace file
**Lines Added:** ~130 lines
**Impact:** Comprehensive documentation alignment with v0.19.5 features
