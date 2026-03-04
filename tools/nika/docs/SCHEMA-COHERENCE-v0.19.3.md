# Schema/Code Coherence Report - Nika v0.19.3

**Date:** 2026-03-04
**Status:** All gaps fixed, 3596 tests passing

## Executive Summary

Comprehensive audit of JSON schema (`schemas/nika-workflow.schema.json`) vs Rust AST code (`src/ast/*.rs`) for Nika v0.19.3. All 8 identified gaps have been fixed with TDD methodology (tests run after each fix).

## Gaps Fixed

### Gap 1: ExecParams Missing Fields

**Location:** `src/ast/action.rs` (ExecParams struct)

**Issue:** JSON schema had `timeout` and `cwd` fields, Rust did not.

**Fix:**
```rust
pub struct ExecParams {
    pub command: String,
    pub shell: Option<bool>,
    pub timeout: Option<u64>,  // ADDED: Timeout in milliseconds
    pub cwd: Option<String>,   // ADDED: Working directory
}
```

**Files Updated:**
- `src/ast/action.rs` - Added fields + custom deserializer
- `src/runtime/executor.rs` - Updated 20+ test cases
- `src/runtime/runner.rs` - Updated test cases

### Gap 2: InferParams Missing response_format

**Location:** `src/ast/action.rs` (InferParams struct)

**Issue:** JSON schema had `response_format` enum, Rust did not.

**Fix:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    #[default]
    Text,
    Json,
    Markdown,
}

pub struct InferParams {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub response_format: Option<ResponseFormat>,  // ADDED
}
```

### Gap 3: FetchParams Missing retry in JSON Schema

**Location:** `schemas/nika-workflow.schema.json`

**Issue:** Rust had `retry: Option<RetryConfig>`, JSON schema did not.

**Fix:** Added to JSON schema:
```json
"RetryConfig": {
  "type": "object",
  "properties": {
    "max_attempts": { "type": "integer", "minimum": 1, "default": 3 },
    "backoff_ms": { "type": "integer", "minimum": 0, "default": 1000 },
    "multiplier": { "type": "number", "minimum": 1.0, "default": 2.0 }
  }
}
```

### Gap 4: AgentParams Missing Fields

**Location:** `src/ast/agent.rs` + JSON schema

**Issue:** Rust missing `tools`, `stop_sequences`; JSON schema missing `scope`.

**Fix (Rust):**
```rust
pub struct AgentParams {
    // ... existing fields ...
    pub tools: Vec<String>,           // ADDED: Builtin tools
    pub stop_sequences: Vec<String>,  // ADDED: Stop sequences
}
```

**Fix (JSON schema):**
```json
"scope": {
  "type": "string",
  "enum": ["full", "minimal", "debug"],
  "default": "full",
  "description": "Agent execution scope"
}
```

### Gap 5: DecomposeSpec Missing Fields in JSON Schema

**Location:** `schemas/nika-workflow.schema.json`

**Issue:** JSON schema missing `mcp_server` and `max_depth` fields.

**Fix:**
```json
"mcp_server": {
  "type": "string",
  "default": "novanet",
  "description": "MCP server name for traversal"
},
"max_depth": {
  "type": "integer",
  "minimum": 1,
  "maximum": 10,
  "default": 3,
  "description": "Maximum traversal depth"
}
```

### Gap 6: OutputFormat Missing Variants

**Location:** `src/ast/output.rs`

**Issue:** JSON schema had `yaml` and `markdown` formats, Rust only had `text` and `json`.

**Fix:**
```rust
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Yaml,      // ADDED
    Markdown,  // ADDED
}
```

### Gap 7: LogConfig Coherence

**Location:** `src/ast/logging.rs` + JSON schema

**Issue:**
- Rust missing `format` field (text/json)
- JSON schema missing `console` field
- Rust missing `Trace` log level

**Fix (Rust):**
```rust
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

pub struct LogConfig {
    pub level: LogLevel,
    pub format: LogFormat,  // ADDED
    pub console: bool,
    pub file: Option<String>,
}

pub enum LogLevel {
    Trace,  // ADDED: Most verbose
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}
```

**Fix (JSON schema):**
```json
"console": {
  "type": "boolean",
  "default": true,
  "description": "Show logs in console output"
}
```

### Gap 8: Workflow Skills Type Mismatch

**Location:** `schemas/nika-workflow.schema.json`

**Issue:** JSON schema defined `skills` as array of `SkillDef` objects; Rust uses `FxHashMap<String, String>` (object format).

**Fix:** Updated JSON schema to match Rust:
```json
"skills": {
  "type": "object",
  "description": "Workflow-level skills for prompt augmentation (v0.6+). Map of alias -> skill file path.",
  "additionalProperties": {
    "type": "string",
    "description": "Path to skill file (local or pkg: URI)"
  }
}
```

Also removed orphaned `SkillDef` definition.

## Test Results

| Stage | Tests | Status |
|-------|-------|--------|
| After Gap 1 | 3592 | PASS |
| After Gap 2 | 3592 | PASS |
| After Gap 3 | 3592 | PASS |
| After Gap 4 | 3592 | PASS |
| After Gap 5 | 3592 | PASS |
| After Gap 6 | 3592 | PASS |
| After Gap 7 | 3596 | PASS |
| After Gap 8 | 3596 | PASS |

**Final:** 3596 passed, 0 failed, 2 ignored

## Files Modified

### Rust Source Files

| File | Changes |
|------|---------|
| `src/ast/action.rs` | Added ExecParams fields, ResponseFormat enum, InferParams field |
| `src/ast/agent.rs` | Added tools, stop_sequences fields |
| `src/ast/output.rs` | Added Yaml, Markdown variants to OutputFormat |
| `src/ast/logging.rs` | Added LogFormat enum, format field to LogConfig, Trace level |
| `src/runtime/executor.rs` | Updated 20+ test cases for new fields |
| `src/runtime/runner.rs` | Updated test cases for new fields |

### JSON Schema

| File | Changes |
|------|---------|
| `schemas/nika-workflow.schema.json` | Added RetryConfig, DecomposeSpec fields, AgentParams scope, LogConfig console, fixed skills type |

## Verification Commands

```bash
# Run all lib tests
cd /Users/thibaut/dev/supernovae/nika/tools/nika
cargo test --lib

# Run specific module tests
cargo test --lib logging
cargo test --lib action
cargo test --lib agent
cargo test --lib output
cargo test --lib workflow
```

## Schema Version

- JSON Schema: `nika/workflow@0.10` (current)
- Rust Code: v0.19.3
- Coherence: 100%

---

*Report generated by Schema Architect audit on 2026-03-04*
