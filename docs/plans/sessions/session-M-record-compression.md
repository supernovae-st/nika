# Session M: P-RECORD -- Record Compression Engine (~6-8h, split across 2-3 sittings)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-28-phase1-record.md` -- READ IT FIRST.
Master plan: `docs/plans/2026-03-28-v1-master-plan.md` for Phase 1.2 scope.
Dev reference: `tools/nika/CLAUDE.md` for conventions.

Depends on: Session L (P-MODEL complete) for agent preset resolution (compression uses a
cheap "summary" agent preset).

Key files:
- `tools/nika-engine/src/store/run_context.rs` (1593 LOC) -- RunContext, task result storage
- `tools/nika-engine/src/binding/resolve.rs` (3046 LOC) -- binding resolution
- `tools/nika-engine/src/runtime/runner.rs` (6524 LOC) -- completion boundary
- `tools/nika-core/src/ast/raw/parser.rs` (3372 LOC) -- YAML parsing
- `tools/nika-core/src/ast/raw/task.rs` (209 LOC) -- RawTask struct
- `tools/nika-event/src/log.rs` (3961 LOC) -- event definitions
- `tools/nika-engine/src/runtime/builtin/router.rs` (560 LOC) -- tool registration

## Mission: Add `record:` field for output compression, Record-aware bindings, nika:records tool

P-RECORD introduces compressed representations of task outputs. When a task has
`record: { compress: true }`, the engine uses a cheap LLM to compress raw output into a
structured `Record` (summary, key_findings, confidence). Downstream tasks receive the
compressed Record via bindings instead of raw 10K+ token output. This keeps context growth
logarithmic, avoiding the "dumb zone" past 8K accumulated tokens.

### Methodology
For EVERY change: read code -> write failing test -> fix -> verify -> commit.
`cargo test --workspace --lib` (always --lib). 1 fix = 1 commit.

---

## PART 1: Record Data Structure

### Task 1: Create Record struct

**File**: `tools/nika-engine/src/runtime/record.rs` (NEW, ~120 LOC)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub task_id: Arc<str>,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub raw_output: Option<String>,
    pub confidence: f64,
    pub tokens_original: u64,
    pub tokens_compressed: u64,
    pub compression_model: String,
    pub compression_cost_usd: f64,
    pub compression_duration: Duration,
}
```

Methods: `compression_ratio()`, `meets_threshold()`, `to_binding_value()`.

**Tests** (8):
- Record construction
- `compression_ratio()` with zero tokens (returns 1.0)
- `compression_ratio()` with real values
- `meets_threshold()` above threshold
- `meets_threshold()` below threshold
- `to_binding_value()` serialization (JSON object with summary, key_findings, confidence)
- Default field values
- Serialization round-trip

**Estimated LOC**: ~120 (struct) + ~80 (tests)
**Commit**: `feat(runtime): add Record struct for compressed task outputs`

### Task 2: Add records storage to RunContext

**File**: `tools/nika-engine/src/store/run_context.rs` (1593 LOC)

Add `records: DashMap<Arc<str>, Record, FxBuildHasher>` field.
Methods: `set_record()`, `get_record()`, `has_record()`, `iter_records()`.

**Tests** (4):
- `set_record` / `get_record` round-trip
- `has_record` true/false
- Concurrent access via DashMap
- `iter_records` returns all records

**Estimated LOC**: ~40
**Commit**: `feat(store): add Record storage to RunContext`

---

## PART 2: RecordSpec AST Type

### Task 3: Create RecordSpec in nika-core

**File**: `tools/nika-core/src/ast/record.rs` (NEW, ~50 LOC)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSpec {
    #[serde(default)]
    pub compress: bool,
    #[serde(default)]
    pub retain: Vec<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,  // default: 500
    #[serde(default)]
    pub confidence_threshold: f64,
}
```

**Commit**: `feat(ast): add RecordSpec type for record compression config`

### Task 4: Parse record: field from YAML

**File**: `tools/nika-core/src/ast/raw/task.rs` (209 LOC)
Add `pub record: Option<Spanned<serde_json::Value>>` to RawTask.

**File**: `tools/nika-core/src/ast/raw/parser.rs` (3372 LOC)
- Add `"record"` to KNOWN_TASK_KEYS
- Parse `record:` field (support both `record: true` shorthand and full mapping)
- Add to RawTask construction

**File**: `tools/nika-core/src/ast/analyzed/task.rs` (599 LOC)
Add `pub record: Option<RecordSpec>` to AnalyzedTask.

**File**: `tools/nika-core/src/ast/analyzer/analyze.rs`
- Handle shorthand: `record: true` -> `RecordSpec { compress: true, ..default }`
- Validate: `max_tokens > 0 && max_tokens <= 4096`
- Validate: `confidence_threshold` in 0.0..=1.0
- Warn: `compress: true` on `exec:` tasks

**Tests** (10):
- `record: true` shorthand
- `record: { compress: true, max_tokens: 300 }`
- `record: { compress: true, retain: [stats, findings] }`
- Invalid max_tokens -> error
- Invalid confidence_threshold -> error
- Missing record -> None (backward compat)
- `record:` does not trigger NIKA-163 unknown key
- Exec task with record -> warning
- Full spec round-trip
- Shorthand false -> None

**Estimated LOC**: ~80
**Commit**: `feat(parser): parse record: field with shorthand and full form`

### Task 5: Propagate through lower.rs

**File**: `tools/nika-engine/src/ast/workflow.rs` (1021 LOC)
Add `pub record: Option<RecordSpec>` to Task struct.

**File**: `tools/nika-engine/src/ast/lower.rs` (2716 LOC)
Propagate `record` through lowering.

**Estimated LOC**: ~10
**Commit**: `feat(ast): propagate record spec through lower`

---

## PART 3: RecordCompressor

### Task 6: Create RecordCompressor

**File**: `tools/nika-engine/src/runtime/record_compress.rs` (NEW, ~250 LOC)

The compressor takes raw output and produces a Record using a cheap LLM.

**Compression prompt** (embedded const):
```
You are a precise summarizer. Given a task's raw output, produce JSON:
1. "summary": concise summary (max {max_tokens} tokens)
2. "key_findings": array of 3-7 key points
3. "confidence": float 0.0-1.0 assessing summary quality
```

**Fallback strategy** (compression failure is NON-FATAL):
1. Retry with simplified prompt
2. Truncate raw output to max_tokens, confidence = 0.0, emit RecordSkipped event
3. NEVER fail the task itself

**Tests** (10-12):
- Mock provider returns valid JSON -> Record created
- Mock returns invalid JSON -> fallback to truncation
- Mock returns error -> fallback to truncation
- Empty output -> minimal Record
- Cost calculation
- Duration tracking
- `retain` fields extracted correctly
- Confidence below threshold -> RecordSkipped event
- Non-fatal: task succeeds even when compression fails

**Estimated LOC**: ~250 (impl) + ~150 (tests)
**Commit**: `feat(runtime): add RecordCompressor with fallback strategy`

---

## PART 4: Runner Integration -- Compression at Completion Boundary

### Task 7: Wire compression into runner

**File**: `tools/nika-engine/src/runtime/runner.rs` (6524 LOC)

After task result is built and confirmed successful (around the area where artifact
processing begins), add the compression step:

```rust
if task_result.is_success() {
    if let Some(ref record_spec) = task.record {
        if record_spec.compress {
            let raw_output = task_result.output_str().into_owned();
            let compressor = RecordCompressor::new(event_log.clone());
            let (provider, provider_name, model) = resolve_compression_provider(
                &self.resolved_assets, &self.default_provider
            );
            let record = compressor.compress(&task_id, &raw_output, ...).await;
            datastore.set_record(&task_id, record);
        }
    }
}
```

**Compression provider resolution**:
1. Look for agent named `summary` in resolved agents
2. If found, use its provider + model
3. Fallback: workflow default provider with cheapest model heuristic

**Tests** (6-8):
- Workflow with `record: { compress: true }` produces Record
- Compression runs after success, not failure
- For-each aggregated output gets single Record
- Compression failure does not fail the task

**Estimated LOC**: ~80
**Commit**: `feat(runtime): wire record compression at task completion boundary`

---

## PART 5: Record-Aware Bindings

### Task 8: Modify binding resolution for Records

**File**: `tools/nika-engine/src/binding/resolve.rs` (3046 LOC)

In `resolve_binding_path()`, when resolving `BindingSource::Task(id)`:
1. Check `datastore.has_record(task_id)`
2. If Record exists AND no further path segments: return `Value::String(record.summary)`
3. If Record exists AND path segments present: return `record.to_binding_value()` for navigation
4. If no Record: return raw output (backward compat)

This means:
- `{{with.data}}` -> summary string (most common)
- `{{with.data.confidence}}` -> float
- `{{with.data.key_findings}}` -> array

**Tests** (8-10):
- Binding WITH Record returns summary
- Binding WITHOUT Record returns raw output (backward compat)
- `{{with.data.confidence}}` navigates into Record
- `{{with.data}}` in template produces summary string, not JSON
- `{{with.data.key_findings | first}}` works with pipe transforms
- No Record + no record: spec -> raw output unchanged

**Estimated LOC**: ~60
**Commit**: `feat(binding): Record-aware binding resolution`

---

## PART 6: Events + Display

### Task 9: Add RecordCreated and RecordSkipped events

**File**: `tools/nika-event/src/log.rs` (3961 LOC)

```rust
RecordCreated {
    task_id: Arc<str>,
    summary_tokens: u64,
    confidence: f64,
    compression_cost_usd: f64,
    compression_ratio: f64,
    model: String,
},
RecordSkipped {
    task_id: Arc<str>,
    reason: String,
},
```

**File**: `tools/nika-engine/src/display/format_event.rs` (739 LOC)
Add formatting for both events.

**Tests** (4): Serialization, task_id() accessor, display formatting.

**Estimated LOC**: ~60
**Commit**: `feat(event): add RecordCreated and RecordSkipped events`

---

## PART 7: nika:records Introspection Tool

### Task 10: Create RecordsTool builtin

**File**: `tools/nika-engine/src/runtime/builtin/records.rs` (NEW, ~80 LOC)

Params: `{ task_id?: string, min_confidence?: float }`
Returns: JSON array of record summaries, filtered by optional params.

Needs `Arc<RunContext>` reference (same pattern as CostTool from Session L).

**File**: `tools/nika-engine/src/runtime/builtin/router.rs` (560 LOC)
Register `nika:records` alongside `nika:cost`.

**File**: `tools/nika-engine/src/runtime/builtin/mod.rs` (156 LOC)
`mod records; pub use records::RecordsTool;`

**Tests** (4):
- Empty records -> empty array
- Records present -> returns summaries
- task_id filter
- min_confidence filter

**Estimated LOC**: ~90 (impl) + ~50 (tests)
**Commit**: `feat(builtin): add nika:records introspection tool`

---

## PART 8: Error Codes + Module Registration

### Task 11: Reserve NIKA-320-329 for Record errors

**File**: `tools/nika-engine/src/error.rs` / `error_domains.rs`

| Code | Meaning |
|------|---------|
| NIKA-320 | Record compression failed (fallback to truncation) |
| NIKA-321 | Record compression produced invalid JSON |
| NIKA-322 | Record confidence below threshold |
| NIKA-323 | Record max_tokens exceeded |
| NIKA-324 | No compression provider available |

**File**: `tools/nika-engine/src/runtime/mod.rs`
Add `pub mod record; pub mod record_compress;`

**Estimated LOC**: ~30
**Commit**: `feat(error): add NIKA-320-324 record error codes`

---

## E2E Verification Workflows

### test-record-compression.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-record-compression
description: "E2E: record compression passes summary to downstream task"
provider: mock

agents:
  summary:
    provider: mock
    model: mock-summary

tasks:
  - id: research
    infer: "Research QR code trends 2026 in France. Include statistics and market data."
    record:
      compress: true
      retain: [statistics, trends]
      max_tokens: 300

  - id: write
    depends_on: [research]
    with: { findings: $research }
    infer: |
      Write an article using these findings: {{with.findings}}
      # Expected: {{with.findings}} contains compressed summary, NOT raw 10K output
```
**Run**: `nika run test-record-compression.nika.yaml`

### test-record-backward-compat.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-record-backward-compat
description: "E2E: no record: field, raw output passed as before"
provider: mock

tasks:
  - id: step1
    infer: "Hello world"
    # No record: field

  - id: step2
    depends_on: [step1]
    with: { data: $step1 }
    infer: "Process: {{with.data}}"
    # Expected: {{with.data}} is raw output from step1
```
**Run**: `nika run test-record-backward-compat.nika.yaml`

### test-record-shorthand.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-record-shorthand
provider: mock

tasks:
  - id: generate
    infer: "Generate long content"
    record: true
    # Expected: shorthand for record: { compress: true }
```
**Run**: `nika check test-record-shorthand.nika.yaml` -> valid

---

## After All Fixes

```bash
cd tools && cargo test --workspace --lib       # All pass, 48+ new tests
cd tools && cargo clippy --workspace -- -D warnings  # Zero warnings
nika check test-record-compression.nika.yaml   # Valid
nika run test-record-backward-compat.nika.yaml # Raw output still works
```

---

## Commit Strategy (11 commits)

```
# Part 1-2: Data structures
feat(runtime): add Record struct for compressed task outputs
feat(store): add Record storage to RunContext
feat(ast): add RecordSpec type for record compression config
feat(parser): parse record: field with shorthand and full form
feat(ast): propagate record spec through lower

# Part 3-4: Compression
feat(runtime): add RecordCompressor with fallback strategy
feat(runtime): wire record compression at task completion boundary

# Part 5: Bindings
feat(binding): Record-aware binding resolution

# Part 6-8: Events + tools + registration
feat(event): add RecordCreated and RecordSkipped events
feat(builtin): add nika:records introspection tool
feat(error): add NIKA-320-324 record error codes
```
