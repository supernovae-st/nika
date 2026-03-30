# Nika Phase 1.2: P-RECORD -- Record Compression Engine (v0.52)

## Detailed Implementation Plan

**Date**: 2026-03-28
**Author**: Architecture review
**Status**: Plan
**Depends on**: Phase 1.1 (P-MODEL -- agent presets)
**Duration**: 3 weeks (Week 5-7 of master plan)
**Target**: v0.52

---

### Executive Summary

P-RECORD introduces compressed representations of task outputs, generated at the natural completion boundary. When a task has `record: { compress: true }`, the engine uses a cheap "summary" agent to compress the raw output into a structured `Record` containing a summary, key findings, and confidence score. Downstream tasks that bind to this task via `with:` receive the compressed Record instead of raw output. This keeps context growth logarithmic instead of linear, avoiding the "dumb zone" where LLM performance degrades past 8K tokens of accumulated context.

The implementation spans 7 crates: `nika-core` (AST + parsing), `nika-engine` (runtime + bindings + executor + builtin tools), and `nika-event` (events). Estimated delta: approximately 1,800-2,200 lines of new code and 40-60 new tests.

---

### Part 1: Record Data Structure

**New file**: `tools/nika-engine/src/runtime/record.rs`
**LOC estimate**: 120 lines

#### Design

```rust
use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Compressed representation of a task's execution output.
///
/// Generated at the natural completion boundary when `record.compress = true`.
/// Downstream tasks receive the `summary` via bindings, not raw output.
/// The raw output is retained only for debug/trace purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Task that produced this record
    pub task_id: Arc<str>,
    /// LLM-compressed summary of the task output
    pub summary: String,
    /// Extracted key points (from `retain:` fields or auto-extracted)
    pub key_findings: Vec<String>,
    /// Original raw output (debug only -- never passed downstream)
    pub raw_output: Option<String>,
    /// Self-assessed confidence score (0.0-1.0)
    pub confidence: f64,
    /// Tokens consumed by the original task
    pub tokens_original: u64,
    /// Tokens in the compressed summary
    pub tokens_compressed: u64,
    /// Model used for compression
    pub compression_model: String,
    /// Cost of the compression call in USD
    pub compression_cost_usd: f64,
    /// Duration of the compression call
    pub compression_duration: Duration,
}

impl Record {
    /// Get the compression ratio (compressed / original)
    pub fn compression_ratio(&self) -> f64 {
        if self.tokens_original == 0 {
            return 1.0;
        }
        self.tokens_compressed as f64 / self.tokens_original as f64
    }

    /// Check if the record meets a confidence threshold
    pub fn meets_threshold(&self, threshold: f64) -> bool {
        self.confidence >= threshold
    }

    /// Convert to a JSON Value for binding resolution
    pub fn to_binding_value(&self) -> serde_json::Value {
        serde_json::json!({
            "summary": self.summary,
            "key_findings": self.key_findings,
            "confidence": self.confidence,
            "compression_ratio": self.compression_ratio(),
        })
    }
}
```

#### Integration with TaskResult

The `Record` is stored alongside the `TaskResult` in `RunContext`, not as a replacement. The key design decision: `TaskResult` remains unchanged (backward compatible), and `Record` is an optional secondary artifact.

**File to modify**: `tools/nika-engine/src/store/run_context.rs`
**LOC estimate**: 40 lines added

Changes to `RunContext`:
- Add a new `DashMap<Arc<str>, Record, FxBuildHasher>` field named `records` for lock-free concurrent access.
- Add methods: `set_record(&self, task_id: &Arc<str>, record: Record)`, `get_record(&self, task_id: &str) -> Option<Record>`, `has_record(&self, task_id: &str) -> bool`, `iter_records(&self) -> Vec<(Arc<str>, Record)>`.
- The `Default::default()` impl adds `records: Arc::new(DashMap::with_hasher(FxBuildHasher))`.

The separation is critical: `TaskResult.output` remains the raw output for backward compatibility, schema validation, artifact generation, etc. The `Record` is a derived, compressed view used exclusively for downstream binding resolution.

#### Verification

- Unit tests: Record construction, `compression_ratio()`, `meets_threshold()`, `to_binding_value()` serialization.
- RunContext tests: `set_record` / `get_record` round-trip, concurrent access via DashMap.
- Estimated: 8 tests.

---

### Part 2: RecordCompressor

**New file**: `tools/nika-engine/src/runtime/record_compress.rs`
**LOC estimate**: 250-300 lines

#### Design

The `RecordCompressor` takes raw task output and produces a `Record`. It uses the agent preset system (P-MODEL from Phase 1.1) to select a cheap model for compression. The system prompt is engineered for structured summary extraction.

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::provider::rig::RigProvider;
use crate::provider::cost::{calculate_cost, ProviderKind};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};

/// Configuration for record compression (from `record:` AST field)
#[derive(Debug, Clone)]
pub struct RecordConfig {
    pub compress: bool,
    pub retain: Vec<String>,
    pub max_tokens: u32,
    pub confidence_threshold: f64,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
            compress: false,
            retain: vec![],
            max_tokens: 500,
            confidence_threshold: 0.0,
        }
    }
}

/// Compresses task output into a Record using a cheap LLM.
pub struct RecordCompressor {
    event_log: EventLog,
}
```

#### Compression Prompt Engineering

The system prompt for compression will be a const embedded in the module:

```text
You are a precise summarizer. Given a task's raw output, produce a JSON response with:
1. "summary": A concise summary of the key information (max {max_tokens} tokens)
2. "key_findings": An array of 3-7 key points extracted from the output
3. "confidence": A float 0.0-1.0 assessing how well the summary captures the original
{retain_instruction}

Respond ONLY with valid JSON. No markdown fences. No additional text.
```

When `retain` fields are specified, append: `4. "retained": Extract these specific fields from the output: {fields}`.

#### Compression Flow

1. Build compression prompt with raw output embedded.
2. Get or create a `RigProvider` for the summary agent preset (falls back to Groq `llama-3.3-70b-versatile` or workflow default if no summary agent).
3. Call the provider with the compression prompt.
4. Parse the JSON response to extract summary, key_findings, confidence.
5. Calculate token counts and cost via `estimate_tokens()` and `calculate_cost()`.
6. Build and return the `Record`.

#### Fallback Strategy

If compression fails (provider error, invalid JSON, timeout):
1. Attempt 1: Retry with simplified prompt ("Summarize this in {max_tokens} words: {output}").
2. Attempt 2: Truncate raw output to `max_tokens` characters, set confidence to 0.0, emit `RecordSkipped` event.
3. Never fail the task itself -- compression failure is non-fatal.

#### Agent Preset Resolution

The compressor needs access to agent presets from Phase 1.1. The method signature:

```rust
pub async fn compress(
    &self,
    task_id: &Arc<str>,
    raw_output: &str,
    config: &RecordConfig,
    provider: &RigProvider,
    provider_name: &str,
    model: Option<&str>,
) -> Record { ... }
```

The caller (in `runner.rs`) resolves the "summary" agent preset to a provider+model before calling compress. If no summary agent is defined in the workflow, use the workflow default provider with a cheap model heuristic.

#### Cost Tracking

The compression call itself has cost. Track:
- `compression_cost_usd`: from `calculate_cost()` using the ProviderResponded event data.
- `tokens_compressed`: actual summary token count from `estimate_tokens()`.
- `compression_duration`: wall-clock time for the LLM call.

#### Verification

- Unit tests with mock provider returning known JSON.
- Fallback tests: provider returns error, returns invalid JSON, returns empty.
- Truncation fallback test.
- Cost calculation test.
- Estimated: 10-12 tests.

---

### Part 3: AST Changes

Three files in `nika-core` need changes, plus the analyzer.

#### 3a. Raw AST: `record:` field on RawTask

**File to modify**: `tools/nika-core/src/ast/raw/task.rs`
**LOC estimate**: 5 lines added

Add to `RawTask`:
```rust
/// Record compression configuration
pub record: Option<Spanned<serde_json::Value>>,
```

#### 3b. New RecordSpec type

**New file**: `tools/nika-core/src/ast/record.rs`
**LOC estimate**: 50 lines

```rust
use serde::{Deserialize, Serialize};

/// Record compression specification (from `record:` YAML field).
///
/// Controls whether task output is compressed at the completion boundary.
/// When `compress: true`, a cheap LLM summarizes the output into a Record
/// that is passed to downstream tasks instead of raw output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSpec {
    /// Whether to compress this task's output (default: false)
    #[serde(default)]
    pub compress: bool,

    /// Fields to explicitly retain/extract in the record
    #[serde(default)]
    pub retain: Vec<String>,

    /// Maximum tokens for the compressed summary (default: 500)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Minimum confidence threshold (default: 0.0 = accept any)
    #[serde(default)]
    pub confidence_threshold: f64,
}

fn default_max_tokens() -> u32 { 500 }

impl Default for RecordSpec {
    fn default() -> Self {
        Self {
            compress: false,
            retain: vec![],
            max_tokens: 500,
            confidence_threshold: 0.0,
        }
    }
}
```

Register in `tools/nika-core/src/lib.rs` or the appropriate `mod.rs`.

#### 3c. Parser changes

**File to modify**: `tools/nika-core/src/ast/raw/parser.rs`
**LOC estimate**: 25 lines added

1. Add `"record"` to `KNOWN_TASK_KEYS` in `validate_task_keys()` (line ~1683).
2. Add `"record"` to the known task keys list around line ~467.
3. Parse the `record:` field after artifact/log parsing (around line ~1795):
```rust
// Parse record: config (task-level record compression)
let record = match map.get_node("record") {
    Some(node) => {
        let span = node_to_span(file_id, node);
        let value = node_to_json(node);
        Some(Spanned::new(value, span))
    }
    None => None,
};
```
4. Add `record` to the `RawTask` construction in `parse_task()`.

Also handle the shorthand `record: true` (boolean) as sugar for `record: { compress: true }`.

#### 3d. Analyzed AST changes

**File to modify**: `tools/nika-core/src/ast/analyzed/task.rs`
**LOC estimate**: 5 lines added

Add to `AnalyzedTask`:
```rust
/// Record compression specification
pub record: Option<RecordSpec>,
```

#### 3e. Analyzer changes

**File to modify**: `tools/nika-core/src/ast/analyzer/analyze.rs`
**LOC estimate**: 20 lines added

In `analyze_task()` (around line ~668), add record parsing:
```rust
record: raw.record.as_ref().and_then(|s| {
    // Handle shorthand: record: true → RecordSpec { compress: true, ..default }
    if s.value.is_boolean() {
        if s.value.as_bool() == Some(true) {
            return Some(RecordSpec { compress: true, ..Default::default() });
        }
        return None;
    }
    match serde_json::from_value(s.value.clone()) {
        Ok(spec) => Some(spec),
        Err(e) => {
            ctx.error(AnalyzerError {
                kind: AnalyzerErrorKind::InvalidFieldValue,
                span: s.span,
                message: format!("Invalid record config: {}", e),
            });
            None
        }
    }
}),
```

#### 3f. Analyzer Validation

Add validation rules:
- `max_tokens` must be > 0 and <= 4096.
- `confidence_threshold` must be 0.0..=1.0.
- `retain` field names should be valid identifiers (warn, not error).
- `compress: true` on `exec:` tasks should emit a warning (exec outputs are often unpredictable).

#### Verification

- Parser tests: `record: true`, `record: { compress: true, max_tokens: 300 }`, `record: { compress: true, retain: [stats, findings] }`.
- Analyzer validation tests: invalid max_tokens, invalid confidence_threshold.
- Unknown field test: ensure `record:` does not trigger NIKA-163.
- Estimated: 8-10 tests.

---

### Part 4: Record-Aware Bindings

This is the core integration point. When a downstream task binds to a task that has a Record, the binding resolution must transparently return the Record summary instead of raw output.

#### 4a. Binding Resolution Changes

**File to modify**: `tools/nika-engine/src/binding/resolve.rs`
**LOC estimate**: 60 lines added

The key change is in the `resolve_binding_path()` and `resolve_with_entry()` functions. Currently, when resolving `$research`, the system calls `datastore.get_output(task_id)` which returns the raw `Arc<Value>`. The modification:

1. Before returning the raw output, check `datastore.has_record(task_id)`.
2. If a Record exists, return `record.to_binding_value()` as the resolved value.
3. If no Record exists, return raw output (backward compat).

The change goes into the `resolve_binding_path()` function where `BindingSource::Task(id)` is dispatched. This is the single point where task-to-task data flows.

For the traced variant `resolve_binding_path_traced()`, emit a new event `BindingRecordUsed { task_id, alias, summary_tokens }` when a Record is used instead of raw output.

#### 4b. Template Access to Record Fields

When a Record is used, `{{with.data}}` returns the summary string. But users need access to sub-fields:
- `{{with.data}}` -- the summary string (most common use case)
- `{{with.data.confidence}}` -- the confidence score
- `{{with.data.key_findings}}` -- the key findings array
- `{{with.data.compression_ratio}}` -- the compression ratio

This works automatically because `Record::to_binding_value()` returns a JSON object. The existing JSONPath navigation in `resolve_path()` handles nested access. The summary is the top-level string representation when the value is used as a string in templates (via `Value::to_string()` coercion).

However, there is a subtlety: when `{{with.data}}` is used in a prompt template, the user expects the summary text, not a JSON object. Solution: in `resolve_binding_path()`, when a Record exists and no further path segments are present, return `Value::String(record.summary.clone())` -- the summary as a plain string. When path segments ARE present (e.g., `.confidence`), return the full binding value for navigation.

#### 4c. Raw Output Escape Hatch

For cases where the user explicitly needs raw output despite having a Record:
- `{{with.data.raw}}` or a `record: { compress: true, pass_raw: true }` option.
- Phase 1 approach: omit escape hatch. If users need raw output, they omit `record:`.

#### Verification

- Test: binding resolution WITH Record returns summary.
- Test: binding resolution WITHOUT Record returns raw output (backward compat).
- Test: `{{with.data.confidence}}` navigates into Record binding value.
- Test: `{{with.data}}` in template produces summary string, not JSON object.
- Test: traced variant emits `BindingRecordUsed` event.
- Estimated: 8-10 tests.

---

### Part 5: Runner Integration -- Compression at Completion Boundary

The Record compression happens AFTER task execution succeeds, BEFORE the result is stored in the datastore. This is the natural completion boundary.

**File to modify**: `tools/nika-engine/src/runtime/runner.rs`
**LOC estimate**: 80 lines added

#### Insertion Point

In `execute_task_iteration()` (line ~865), after the task result is built (around line ~1063 where artifact processing begins), add the compression step:

```rust
// After task_result is built and confirmed successful:
if task_result.is_success() {
    // Check if task has record config with compress: true
    if let Some(ref record_spec) = task.record {
        if record_spec.compress {
            let raw_output = task_result.output_str().into_owned();
            let compressor = RecordCompressor::new(event_log.clone());
            
            // Resolve summary agent (from workflow agents: block or default)
            let (provider, provider_name, model) = resolve_compression_provider(...);
            
            let record = compressor.compress(
                &task_id,
                &raw_output,
                &RecordConfig::from(record_spec),
                &provider,
                &provider_name,
                model.as_deref(),
            ).await;
            
            datastore.set_record(&task_id, record);
        }
    }
}
```

The compression call is async and runs after the task's own execution completes. It does NOT block other tasks that do not depend on this task.

#### Compression Provider Resolution

The compression provider is resolved from the workflow's `agents:` block:
1. Look for an agent named `summary` in the workflow's resolved agents.
2. If found, use its provider + model.
3. If not found, use the workflow's default provider with a cheap model fallback (Groq `llama-3.3-70b-versatile` if available, else workflow default).

This requires passing `resolved_assets` (or a reference to the summary agent config) into `execute_task_iteration()`. Since `resolved_assets` is already available in the `Runner` struct, thread it through via the closure or as a cloned Arc.

#### For-Each Task Records

For `for_each:` tasks, individual iterations do NOT get their own Records. The aggregated parent task output gets a Record if configured. This avoids N compression calls for N iterations.

#### Verification

- Integration test: workflow with `record: { compress: true }` produces Record in datastore.
- Test: compression runs after task success, not after failure.
- Test: for_each aggregated output gets single Record.
- Test: compression failure does not fail the task.
- Estimated: 6-8 tests.

---

### Part 6: Events

**File to modify**: `tools/nika-event/src/log.rs`
**LOC estimate**: 40 lines added

Add two new EventKind variants in the events section (after the STRUCTURED OUTPUT EVENTS block, around line ~543):

```rust
// ═══════════════════════════════════════════
// RECORD EVENTS
// ═══════════════════════════════════════════

/// Record successfully compressed from task output
RecordCreated {
    /// Task that produced this record
    task_id: Arc<str>,
    /// Token count of the summary
    summary_tokens: u64,
    /// Self-assessed confidence score
    confidence: f64,
    /// Cost of the compression call in USD
    compression_cost_usd: f64,
    /// Compression ratio (compressed / original)
    compression_ratio: f64,
    /// Model used for compression
    model: String,
},

/// Record compression was skipped or fell back to truncation
RecordSkipped {
    /// Task that was supposed to produce a record
    task_id: Arc<str>,
    /// Why compression was skipped
    reason: String,
},
```

Update `EventKind::task_id()` method to return `Some` for both new variants.

Update `EventKind::is_workflow_event()` to return `false` for both.

#### Display Integration

**File to modify**: `tools/nika-engine/src/display/format_event.rs`
**LOC estimate**: 20 lines added

Add formatting for the two new event variants in the event formatter:
- `RecordCreated`: display task_id, compression ratio, confidence, cost.
- `RecordSkipped`: display task_id and reason.

#### Verification

- Test: RecordCreated event emission on successful compression.
- Test: RecordSkipped event emission on fallback/failure.
- Test: task_id() returns correct value for both variants.
- Estimated: 4 tests.

---

### Part 7: `nika:records` Introspection Tool

**New file**: `tools/nika-engine/src/runtime/builtin/records.rs`
**LOC estimate**: 80 lines

A new builtin tool that agents can call to inspect accumulated records in the current run. This is critical for the future P-ORCHESTRATE phase where the orchestrator reviews records to make decisions.

#### Design

```rust
use super::BuiltinTool;
use crate::error::NikaError;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Deserialize)]
struct RecordsParams {
    /// Optional: filter by task_id
    #[serde(default)]
    task_id: Option<String>,
    /// Optional: filter by minimum confidence
    #[serde(default)]
    min_confidence: Option<f64>,
}

#[derive(Debug, Serialize)]
struct RecordSummary {
    task_id: String,
    summary: String,
    confidence: f64,
    key_findings: Vec<String>,
    tokens_compressed: u64,
    compression_cost_usd: f64,
}

pub struct RecordsTool {
    // Needs access to RunContext -- will use Arc<RunContext> from executor
}
```

The tool returns a JSON array of RecordSummary objects, filtered by the optional params.

#### Registration

**File to modify**: `tools/nika-engine/src/runtime/builtin/router.rs`
**LOC estimate**: 5 lines added

Add `"records"` to the tool registration in `BuiltinToolRouter::new()` or `with_all_tools()`.

**File to modify**: `tools/nika-engine/src/runtime/builtin/mod.rs`
**LOC estimate**: 5 lines added

Add `mod records;` and `pub use records::RecordsTool;`.

#### Challenge: Access to RunContext

The `BuiltinTool::call()` signature takes only `args: String`. The tool needs access to `RunContext` to query records. Solution: the `RecordsTool` struct holds an `Arc<RunContext>` reference, similar to how `ToolContext` is shared with file tools via `FileToolAdapter`. The `RunContext` is passed at construction time in `BuiltinToolRouter::with_all_tools()`.

However, this changes the `with_all_tools()` signature to also accept `RunContext`. Alternatively, use the same pattern as file tools: create a `RecordsTool` that captures the `RunContext` Arc. Since `RunContext` is Clone (all fields are Arc), this is straightforward.

The cleaner approach: add `RunContext` as an optional field on `BuiltinToolRouter`, set it when the Runner creates the executor. Tools that need it (like `nika:records`) check for it at call time.

#### Verification

- Test: returns empty array when no records.
- Test: returns records after compression.
- Test: task_id filter.
- Test: min_confidence filter.
- Estimated: 4 tests.

---

### Part 8: Module Registration and Wiring

#### nika-engine runtime/mod.rs

**File to modify**: `tools/nika-engine/src/runtime/mod.rs`
**LOC estimate**: 5 lines added

Add:
```rust
pub mod record;
pub mod record_compress;
```

And re-export:
```rust
pub use record::Record;
pub use record_compress::{RecordCompressor, RecordConfig};
```

#### nika-core lib.rs

**File to modify**: `tools/nika-core/src/lib.rs` (or `ast/mod.rs`)
**LOC estimate**: 3 lines added

Add:
```rust
pub mod record;
pub use record::RecordSpec;
```

---

### Part 9: Test Plan

**Total estimated new tests**: 48-56

| Category | Tests | Location |
|----------|-------|----------|
| Record struct | 8 | `nika-engine/src/runtime/record.rs` |
| RecordCompressor | 12 | `nika-engine/src/runtime/record_compress.rs` |
| AST parsing | 10 | `nika-core/src/ast/raw/parser.rs` (existing test module) |
| Binding resolution | 10 | `nika-engine/src/binding/resolve.rs` (existing test module) |
| Runner integration | 8 | `nika-engine/src/runtime/runner.rs` (existing test module) |
| Events | 4 | `nika-event/src/log.rs` (existing test module) |
| nika:records tool | 4 | `nika-engine/src/runtime/builtin/records.rs` |
| Backward compat | 2 | `nika-engine/src/runtime/runner.rs` |

Key test scenarios:
1. **Happy path**: `record: { compress: true }` produces Record, downstream binding gets summary.
2. **Backward compat**: no `record:` block, downstream binding gets raw output.
3. **Compression failure**: provider error falls back to truncation, task still succeeds.
4. **Shorthand**: `record: true` parsed as `{ compress: true }`.
5. **Confidence threshold**: Record with confidence below threshold triggers re-compression or warning.
6. **Template access**: `{{with.data}}` returns summary, `{{with.data.confidence}}` returns float.
7. **For-each**: aggregated result gets single Record.
8. **Events**: `RecordCreated` and `RecordSkipped` emitted correctly.

---

### Part 10: Timeline

#### Week 1: Record struct + AST + basic compression

| Day | Task | Files | LOC |
|-----|------|-------|-----|
| Mon | Record struct + RunContext integration | `record.rs` (NEW), `run_context.rs` | 160 |
| Tue | RecordSpec AST type + parser changes | `record.rs` (core, NEW), `raw/task.rs`, `parser.rs` | 80 |
| Wed | Analyzer changes + validation | `analyzer/analyze.rs`, `analyzed/task.rs` | 30 |
| Thu | RecordCompressor core logic | `record_compress.rs` (NEW) | 200 |
| Fri | RecordCompressor fallback + tests | `record_compress.rs` | 100 |

**Week 1 exit**: `Record` struct exists, `record:` parses in YAML, compression works with mock provider. ~15 tests.

#### Week 2: Binding integration + events + runner wiring

| Day | Task | Files | LOC |
|-----|------|-------|-----|
| Mon | Runner integration (compression at completion boundary) | `runner.rs` | 80 |
| Tue | Binding resolution with Record awareness | `resolve.rs` | 60 |
| Wed | Template access to Record fields + tests | `resolve.rs`, `template.rs` | 40 |
| Thu | Events (RecordCreated, RecordSkipped) + display | `log.rs`, `format_event.rs` | 60 |
| Fri | nika:records builtin tool | `records.rs` (NEW), `router.rs`, `mod.rs` | 90 |

**Week 2 exit**: Full pipeline works end-to-end. Records compress, bindings use them, events fire. ~35 tests.

#### Week 3: Polish + edge cases + documentation

| Day | Task | Files | LOC |
|-----|------|-------|-----|
| Mon | For-each integration + edge cases | `runner.rs` | 30 |
| Tue | Backward compat tests + regression suite | Various test modules | 100 |
| Wed | Error codes (NIKA-320..329 range for Record errors) | `error.rs`, `error_domains.rs` | 30 |
| Thu | CLAUDE.md update + YAML examples + showcase | `CLAUDE.md`, examples | 50 |
| Fri | Final test pass + `cargo clippy --workspace` | All | 0 |

**Week 3 exit**: v0.52 release-ready. 48+ new tests. Zero clippy warnings. Backward compatible.

---

### Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Compression adds latency | MEDIUM | Slows task completion by 1-3s | Use cheapest model (Groq); async, non-blocking to peers |
| LLM produces bad summaries | MEDIUM | Downstream tasks get poor context | Confidence score + fallback to truncation |
| Cost overhead of compression | LOW | ~$0.001/compression at Groq rates | Track and display in events; optional |
| Breaking binding resolution | HIGH | Existing workflows break | Guard: only use Record when `record.compress = true` exists |
| Agent preset not available | MEDIUM | No summary agent configured | Fallback chain: summary agent -> workflow default -> truncation |

---

### Dependencies

**Hard dependencies (must exist before starting)**:
- Phase 1.1 P-MODEL: agent preset resolution in executor (for compression model selection).
- Specifically: the ability to resolve a named agent to a provider + model from the `agents:` block.

**Soft dependencies (nice to have)**:
- Cost tracking in `ProviderResponded` event (already exists).
- `estimate_tokens()` utility (already exists in `executor/verbs.rs`).

---

### Error Codes

Reserve the 320-329 range for Record errors:

| Code | Meaning |
|------|---------|
| NIKA-320 | Record compression failed (fallback to truncation) |
| NIKA-321 | Record compression produced invalid JSON |
| NIKA-322 | Record confidence below threshold |
| NIKA-323 | Record max_tokens exceeded |
| NIKA-324 | No compression provider available |

---

### File Summary

**New files (5)**:
| File | Crate | LOC | Purpose |
|------|-------|-----|---------|
| `tools/nika-engine/src/runtime/record.rs` | nika-engine | 120 | Record data structure |
| `tools/nika-engine/src/runtime/record_compress.rs` | nika-engine | 300 | RecordCompressor logic |
| `tools/nika-core/src/ast/record.rs` | nika-core | 50 | RecordSpec AST type |
| `tools/nika-engine/src/runtime/builtin/records.rs` | nika-engine | 80 | nika:records introspection tool |
| `docs/plans/2026-03-28-phase1-record.md` | docs | N/A | This plan |

**Modified files (12)**:
| File | Crate | LOC Added | Changes |
|------|-------|-----------|---------|
| `tools/nika-engine/src/store/run_context.rs` | nika-engine | 40 | records DashMap + accessors |
| `tools/nika-core/src/ast/raw/task.rs` | nika-core | 5 | record field on RawTask |
| `tools/nika-core/src/ast/raw/parser.rs` | nika-core | 25 | Parse record:, validate keys |
| `tools/nika-core/src/ast/analyzed/task.rs` | nika-core | 5 | record field on AnalyzedTask |
| `tools/nika-core/src/ast/analyzer/analyze.rs` | nika-core | 20 | Analyze record config |
| `tools/nika-engine/src/binding/resolve.rs` | nika-engine | 60 | Record-aware binding resolution |
| `tools/nika-engine/src/runtime/runner.rs` | nika-engine | 80 | Compression at completion boundary |
| `tools/nika-engine/src/runtime/mod.rs` | nika-engine | 5 | Module registration |
| `tools/nika-engine/src/runtime/builtin/router.rs` | nika-engine | 5 | Register nika:records |
| `tools/nika-engine/src/runtime/builtin/mod.rs` | nika-engine | 5 | Export RecordsTool |
| `tools/nika-event/src/log.rs` | nika-event | 40 | RecordCreated + RecordSkipped events |
| `tools/nika-engine/src/display/format_event.rs` | nika-engine | 20 | Format new events |

**Total**: approximately 860 new lines + tests. With tests (approximately 1,000 lines), grand total: approximately 1,800-2,200 LOC.

---

### Critical Files for Implementation
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs` (completion boundary -- where compression triggers)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/binding/resolve.rs` (Record-aware binding resolution -- the core integration)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/store/run_context.rs` (Record storage alongside TaskResult)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs` (parsing `record:` field from YAML)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzer/analyze.rs` (analyzing and validating RecordSpec)