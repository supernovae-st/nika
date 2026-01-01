# Task: Clean up Nika v0.1-PRO Spec

## Executive Summary

The current SPEC-v0.1-PRO.md has structural issues, redundant content, and Rust code that doesn't align with the actual codebase. This exploration identifies problems and proposes a cleaner architecture.

---

## Current Spec Issues

### 1. Structural Problems

| Issue | Location | Impact |
|-------|----------|--------|
| Too many ASCII diagrams | 5+ boxes throughout | Overwhelming, hard to scan |
| Redundant examples | use: forms shown 3+ times | Verbose, repetitive |
| Missing TL;DR | Top of doc | Users can't quickly understand |
| Rust code orphaned | Line 401-552 | Not aligned with codebase |

### 2. Rust Code Misalignment

**Spec uses:**
```rust
SmartString         # NOT in Cargo.toml
UseBlock            # Undefined struct
PathBuf             # Wrong - should be String path
```

**Codebase uses:**
```rust
String              # Standard strings
Arc<RwLock<...>>    # Thread-safe patterns
thiserror           # Error handling
```

### 3. Content Redundancy

- `use:` Form 1 example: 4x (lines 117-124, 137-144, 169-188, 354-360)
- Decision tree: 2x (lines 85-100, in summary)
- Error tables: 2x (lines 221-226, 306-310)

---

## Codebase Context

### Key Files

| File | Lines | Purpose | v0.1-PRO Impact |
|------|-------|---------|-----------------|
| `src/workflow.rs` | 59 | Workflow/Task structs | Add `use`, `output` fields |
| `src/task.rs` | 37 | InferDef, ExecDef, FetchDef | No changes |
| `src/template.rs` | 106 | Template resolution | **MAJOR**: New regex |
| `src/datastore.rs` | 80 | TaskData storage | **MAJOR**: Add Value support |
| `src/error.rs` | 38 | NikaError enum | Add NIKA-050+ codes |
| `src/runner.rs` | 349 | DAG execution | Add use block resolution |

### Current Template Pattern

```rust
// src/template.rs:8-11
static TEMPLATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\{\{\s*(\w+)\.output\s*\}\}").unwrap()
});
```

**v0.1-PRO requires:**
```rust
Regex::new(r"\{\{\s*use\.(\w+(?:\.\w+)*)\s*\}\}")
```

### Current DataStore

```rust
// src/datastore.rs:39-43
pub struct DataStore {
    data: Arc<RwLock<HashMap<String, TaskData>>>,
}
```

**v0.1-PRO requires:**
```rust
pub struct DataStore {
    outputs: HashMap<String, Value>,
    resolved_inputs: HashMap<String, HashMap<String, Value>>,
}
```

---

## Proposed Spec Structure

### Before (Current - 643 lines)

```
1. Overview
2. Core Concepts (big diagram)
3. DataStore v2 Structure
4. Optional Fields
5. The use: Block (3 sections)
6. Path Resolution
7. Template Syntax
8. The output: Policy
9. Complete Workflow Example (huge)
10. Rust Implementation (150 lines)
11. Validation Rules
12. Error Codes
13. Migration from v7.0
14. Summary
```

**Problems:**
- Rust code in middle of spec
- No quick reference
- Examples scattered everywhere

### After (Proposed - ~350 lines)

```
1. TL;DR (1 page quick reference)
2. Concepts (flows/use/output triangle)
3. use: Block (3 forms, compact)
4. output: Policy (compact)
5. Examples (one good one)
6. Appendix A: Rust Types (optional)
7. Appendix B: Error Codes (optional)
```

**Improvements:**
- TL;DR at top
- Reference appendices (not inline)
- Single comprehensive example
- ~50% shorter

---

## Rust Type Alignment

### Types to Define (Aligned with Codebase)

```rust
// NEW: src/use_block.rs

use serde::Deserialize;
use serde_json::Value;

/// Use block entries - serde handles 3 forms
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UseEntry {
    /// "alias: task.path"
    Single(String),

    /// "task.path: [field1, field2]"
    Batch(Vec<String>),

    /// "alias: { task.path: { pick, default } }"
    Advanced {
        #[serde(flatten)]
        config: UseConfig,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct UseConfig {
    pub pick: Option<Vec<String>>,
    #[serde(default)]
    pub default: Option<Value>,
}

/// Output policy
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutputPolicy {
    #[serde(default)]
    pub format: OutputFormat,
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}
```

### Key Differences from Current Spec

| Spec Says | Codebase Should Use | Why |
|-----------|---------------------|-----|
| `SmartString` | `String` | Not in Cargo.toml yet |
| `PathBuf` | `String` | Simpler, YAML path |
| `ParsedPath` struct | Parse at runtime | Less complexity |
| `HashMap<String, Value>` | `serde_json::Value` | Direct JSON access |

---

## Web Research Insights

### Industry Patterns (from agents)

**Argo Workflows:**
```yaml
parameters:
  - name: message
    value: "{{steps.step1.outputs.parameters.result}}"
```

**GitHub Actions:**
```yaml
needs: [job1, job2]
steps:
  - run: echo "${{ needs.job1.outputs.result }}"
```

**Prefect:**
```python
@task
def my_task(input_data):
    return result  # Automatic data passing
```

### Key Takeaways

1. **Dot notation is universal** - All systems use `task.field.subfield`
2. **Explicit > Implicit** - Argo/GHA declare outputs explicitly
3. **Minimal syntax wins** - Prefect's decorator approach is cleanest
4. **Batch extraction is unique to Nika** - Our `task.path: [fields]` form

---

## Patterns to Follow

### From Codebase

1. **serde untagged enums** for auto-detection (workflow.rs:30-36)
2. **thiserror for errors** with fix suggestions (error.rs:10-38)
3. **Arc<RwLock<...>>** for thread-safe shared state (datastore.rs:42)
4. **Lazy<Regex>** for compiled regex (template.rs:9)

### From Industry

1. **TL;DR section** at top (GitHub docs pattern)
2. **Reference appendices** for details (Temporal pattern)
3. **Single canonical example** (Argo pattern)
4. **Progressive disclosure** - basics first, details later

---

## Implementation Priority

### Phase 1: Spec Cleanup (This Task)

1. Restructure to TL;DR + Concepts + Examples + Appendices
2. Remove redundant ASCII diagrams
3. Consolidate examples
4. Move Rust code to appendix
5. Align Rust types with actual codebase patterns

### Phase 2: Code Implementation (Next)

1. Add UseEntry, OutputPolicy types to src/
2. Update Task struct with `use`, `output` fields
3. Migrate DataStore to JSON Value support
4. Update template regex for `{{use.alias}}`
5. Add NIKA-050+ error codes

---

## Dependencies

- **serde_json**: Already in Cargo.toml
- **thiserror**: Already in Cargo.toml
- **regex**: Already in Cargo.toml
- **SmartString**: NOT in Cargo.toml (optional optimization)

---

## Next Step

Run `/epct:plan 01-cleanup-v01-pro-spec` to create implementation plan for the cleaner spec.
