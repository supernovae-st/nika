# Implementation Plan: Phase 1.4-1.5 -- Context Budgets + Introspection + Local Memory + Self-Improvement

### Architecture Context

From codebase exploration, the critical integration points are:

**AST pipeline**: `YAML -> RawTask (parser.rs) -> AnalyzedTask (analyzer/) -> lower_action() -> TaskExecutor`

**Binding resolution**: `WithSpec -> ResolvedBindings::from_with_spec() / from_with_spec_traced()` in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/binding/resolve.rs`

**Builtin tools**: `BuiltinTool` trait (name, description, parameters_schema, call) registered in `BuiltinToolRouter` via `FxHashMap<&'static str, Arc<dyn BuiltinTool>>` in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/router.rs`

**Events**: 43+ `EventKind` variants in `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs`, append-only `EventLog` with `parking_lot::RwLock`

**Token estimation already exists**: `estimate_tokens(char_len: usize) -> u64` in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/verbs.rs` (line 12), plus `json_value_size_estimate()` for JSON values

**Trace writing already exists**: `TraceWriter` writes NDJSON to `.nika/traces/` in `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/trace.rs`

**SQLite pattern already exists**: Daemon storage in `/Users/thibaut/dev/supernovae/nika/tools/nika-daemon/src/storage.rs` uses dedicated DB thread + mpsc channel + `rusqlite` with WAL mode

**Valid task keys**: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs` line 1683 has `KNOWN_TASK_KEYS` that must be extended

**Post-workflow hook point**: After `WorkflowCompleted` event (line 2332 of runner.rs) and before `write_trace()` (line 2361) -- this is where record persistence goes

---

## P-CONTEXT (v0.54, Week 9-10)

### Part 1: `context_budget:` Field in AST

**RawTask** (`/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/task.rs`):
- Add `pub context_budget: Option<Spanned<u32>>` field to `RawTask` struct

**Parser** (`/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs`):
- Add `"context_budget"` to `KNOWN_TASK_KEYS` array (line 1683)
- Parse using `get_u32_field(file_id, map, "context_budget")?` in `parse_task()` (around line 1820)
- Assign to `RawTask` struct construction (line 1827)

**AnalyzedTask** (`/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzed/task.rs`):
- Add `pub context_budget: Option<u32>` field to `AnalyzedTask` struct (after line 89)

**Analyzer** (in `nika-core/src/ast/analyzer/`):
- Validate during analysis: `context_budget > 0 && context_budget <= 200_000`
- Error code: `NIKA-152` (next available in AST analysis range 140-151)
- Pass through to `AnalyzedTask`

**Schema stays @0.12** -- additive field, no breaking change.

**Estimated LOC**: ~40 (parser) + ~15 (raw task) + ~10 (analyzed task) + ~25 (analyzer validation) = **~90 LOC**

**Tests**: 6 tests
- Parse `context_budget: 4000` from YAML
- Parse missing `context_budget` (None)
- Reject `context_budget: 0` (NIKA-152)
- Reject `context_budget: 250000` (exceeds 200K limit)
- Analyzer passes valid budget through
- Unknown key detection still works with new field

### Part 2: Token Counting Utilities

**Location**: New file `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/binding/token_budget.rs`

The existing `estimate_tokens` in `verbs.rs` only takes `char_len: usize`. We need a richer module.

```rust
/// Estimate tokens for a string (language-aware approximation)
pub fn estimate_tokens_str(text: &str) -> u64 {
    // CJK detection: characters in CJK Unified Ideographs range use ~2 chars/token
    let (cjk_chars, latin_chars) = text.chars().fold((0u64, 0u64), |(cjk, lat), c| {
        if ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3040}'..='\u{309F}').contains(&c)  // Hiragana
            || ('\u{30A0}'..='\u{30FF}').contains(&c)  // Katakana
        {
            (cjk + 1, lat)
        } else {
            (cjk, lat + 1)
        }
    });
    // CJK: ~2 chars/token, Latin: ~4 chars/token
    (cjk_chars.div_ceil(2) + latin_chars.div_ceil(4)).max(1)
}

/// Estimate tokens for a serde_json::Value (recursive, no allocation)
pub fn estimate_tokens_value(value: &serde_json::Value) -> u64 {
    let size = json_value_size_estimate(value);  // re-use from verbs.rs
    estimate_tokens(size)  // re-use existing fn
}

/// Estimate total tokens in resolved bindings
pub fn estimate_bindings_tokens(bindings: &ResolvedBindings) -> u64 { ... }
```

**Decision**: Do NOT add `tiktoken-rs` as a dependency. It pulls in ~3MB of BPE data and a Python FFI layer. The char-based heuristic is sufficient for budget enforcement (we are truncating, not billing). If more precision is needed later, this module is the single place to upgrade.

**Estimated LOC**: ~80 (module) + ~60 (tests) = **~140 LOC**

**Tests**: 5 tests
- English text: "hello world" -> ~3 tokens
- CJK text: proportionally higher tokens
- JSON value estimation
- Empty string -> 1 token (minimum)
- Mixed CJK/Latin text

### Part 3: Budget Enforcement

**Location**: Modify `from_with_spec_traced()` in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/binding/resolve.rs` (line 237)

**Approach**: After resolving all bindings (line 228-231 pattern), add a post-resolution budget check. This is the integration point where the caller (runner.rs) passes `context_budget: Option<u32>`.

New function in `token_budget.rs`:

```rust
/// Enforce context budget on resolved bindings.
///
/// If total tokens exceed budget, truncates longest values proportionally.
/// Returns events describing what happened.
pub fn enforce_budget(
    bindings: &mut ResolvedBindings,
    budget: u32,
    task_id: &Arc<str>,
) -> Vec<EventKind> {
    let actual = estimate_bindings_tokens(bindings);
    if actual <= budget as u64 {
        return vec![EventKind::BudgetOk {
            task_id: task_id.clone(),
            budget,
            actual: actual as u32,
        }];
    }

    // Proportional truncation strategy:
    // 1. Sort bindings by estimated token count (descending)
    // 2. Truncate each binding proportionally to fit budget
    // 3. Prefer truncating string values over structured JSON
    let truncated_fields = truncate_proportional(bindings, budget);

    vec![EventKind::BudgetExceeded {
        task_id: task_id.clone(),
        budget,
        actual: actual as u32,
        truncated_fields,
    }]
}
```

**Integration in runner.rs**: In the task execution path (around line 2200-2300 area), after `ResolvedBindings::from_with_spec_traced()` succeeds, call `enforce_budget()` if the task has `context_budget`.

**Truncation strategy**:
1. Calculate total token estimate across all resolved bindings
2. If under budget: emit `BudgetOk`, done
3. If over budget: sort bindings by size descending, truncate largest values to fit
4. For string values: truncate at word boundary
5. For JSON values: truncate string fields within the JSON
6. Never truncate to empty -- minimum 50 tokens per binding

**Estimated LOC**: ~120 (enforcement logic) + ~40 (runner integration) = **~160 LOC**

**Tests**: 8 tests
- Bindings under budget: no truncation
- Bindings over budget: truncation applied, BudgetExceeded event
- Single large binding truncated
- Multiple bindings: proportional truncation
- Minimum 50 tokens preserved per binding
- Budget = 0 rejected by analyzer (Part 1)
- JSON value truncation
- Word-boundary truncation for strings

### Part 4: New EventKind Variants

**Location**: `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs`

Add to the `EventKind` enum:

```rust
// ═══════════════════════════════════════════
// CONTEXT BUDGET
// ═══════════════════════════════════════════
/// Context budget check passed (within limits)
BudgetOk {
    task_id: Arc<str>,
    budget: u32,
    actual: u32,
},
/// Context budget exceeded, bindings truncated
BudgetExceeded {
    task_id: Arc<str>,
    budget: u32,
    actual: u32,
    truncated_fields: Vec<String>,
},
```

**Display integration**: Add format functions in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/display/format_event.rs` for both variants.

**TUI integration**: Handle in TUI event handlers for live display of budget warnings.

**Estimated LOC**: ~20 (event variants) + ~30 (format/display) = **~50 LOC**

**Tests**: 3 tests
- BudgetOk serialization/deserialization
- BudgetExceeded serialization/deserialization
- Display formatting

---

## P-INTROSPECT (v0.54, parallel with P-CONTEXT)

### Part 5: 4 New Builtin Tools

**Location**: New files in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/`

All introspection tools need read access to runtime state. The current `BuiltinTool` trait takes only `args: String`. We need a way to inject `RunContext` and `EventLog`. Two options:

**Option A (recommended)**: Store `Arc<RunContext>` and `EventLog` inside each introspection tool struct (like `FileToolAdapter` stores `Arc<ToolContext>`). The tools are constructed with these references and registered into the router.

**Option B**: Extend `BuiltinTool::call()` to take an optional context parameter. This changes the trait signature and breaks all existing tools.

Going with Option A. Create an `IntrospectionToolAdapter` pattern similar to `FileToolAdapter`.

#### Tool 1: `nika:dag_info`

**File**: `builtin/introspect_dag.rs`

```rust
pub struct DagInfoTool {
    dag: Arc<Dag>,
}

// Params: {} (no params)
// Response:
{
    "task_count": 5,
    "edge_count": 7,
    "critical_path": ["research", "write", "review"],
    "parallel_groups": [["research"], ["write_hero", "write_features"], ["review"]],
    "max_depth": 3
}
```

**Implementation**: Walk the `Dag` (from `dag/flow.rs`) to compute:
- `task_count`: number of nodes
- `edge_count`: number of edges
- `critical_path`: longest path through DAG (topological order)
- `parallel_groups`: tasks at each depth level
- `max_depth`: depth of DAG

**Estimated LOC**: ~80

#### Tool 2: `nika:task_status`

**File**: `builtin/introspect_task.rs`

```rust
pub struct TaskStatusTool {
    datastore: RunContext,
    event_log: EventLog,
}

// Params: { "task_id": "research" }
// Response:
{
    "task_id": "research",
    "status": "completed",  // pending|running|completed|failed|skipped
    "duration_ms": 1234,
    "tokens": { "input": 500, "output": 200 },
    "cost_usd": 0.003,
    "has_record": false,
    "output_preview": "QR code adoption in France..."  // first 200 chars
}
```

**Implementation**: Query `RunContext.get_output(task_id)` for result, scan `EventLog` for `ProviderResponded` events matching `task_id` to get token/cost data.

**Estimated LOC**: ~100

#### Tool 3: `nika:threads`

**File**: `builtin/introspect_threads.rs`

```rust
pub struct ThreadsTool {
    event_log: EventLog,
}

// Params: {} or { "status": "running" }
// Response:
[
    { "task_id": "research", "status": "completed", "verb": "infer", "started_at_ms": 100, "duration_ms": 500 },
    { "task_id": "write", "status": "running", "verb": "infer", "started_at_ms": 600, "duration_ms": null }
]
```

**Implementation**: Scan `EventLog` for `TaskStarted`/`TaskCompleted`/`TaskFailed` events and build the thread list. Filter by optional `status` param.

**Estimated LOC**: ~90

#### Tool 4: `nika:orchestrate`

**File**: `builtin/introspect_orchestrate.rs`

```rust
pub struct OrchestrateTool {
    // Will be wired to Orchestrator state in Phase 1.3
    // For now, returns basic workflow-level stats from EventLog
    event_log: EventLog,
    datastore: RunContext,
}

// Params: {}
// Response:
{
    "round": 1,
    "max_rounds": 5,
    "records_count": 3,
    "budget_used_tokens": 4500,
    "budget_limit_tokens": 32000,
    "goal": "Generate a French landing page..."
}
```

**Note**: This tool will be fully wired when P-ORCHESTRATE (v0.53) ships. For now it returns a stub or basic EventLog aggregations.

**Estimated LOC**: ~70

### Part 6: Registration

**Location**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/mod.rs` and `router.rs`

Add new module declarations:

```rust
mod introspect_dag;
mod introspect_task;
mod introspect_threads;
mod introspect_orchestrate;

pub use introspect_dag::DagInfoTool;
pub use introspect_task::TaskStatusTool;
pub use introspect_threads::ThreadsTool;
pub use introspect_orchestrate::OrchestrateTool;
```

Add new constructor to `BuiltinToolRouter`:

```rust
/// Create a router with introspection tools (requires runtime context).
pub fn with_introspection(
    &mut self,
    dag: Arc<Dag>,
    datastore: RunContext,
    event_log: EventLog,
) {
    self.register(DagInfoTool::new(dag));
    self.register(TaskStatusTool::new(datastore.clone(), event_log.clone()));
    self.register(ThreadsTool::new(event_log.clone()));
    self.register(OrchestrateTool::new(event_log, datastore));
}
```

**Wire in runner.rs**: After constructing the `BuiltinToolRouter` but before executing tasks, call `with_introspection()` to register the 4 tools.

**Estimated LOC**: ~30 (mod.rs exports) + ~25 (router registration) + ~15 (runner wiring) = **~70 LOC**

**Tests for all 4 tools**: 10 tests
- DagInfoTool: single task, linear chain, diamond DAG
- TaskStatusTool: completed task, pending task, invalid task_id
- ThreadsTool: no tasks, mixed status, filter by status
- OrchestrateTool: basic stats return

---

## P-MEMORY-LOCAL (v0.55, Week 11-12)

### Part 7: NDJSON Record Persistence

**Location**: New file `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/store/record_writer.rs`

After workflow completes (in runner.rs, between `WorkflowCompleted` event emission at line 2332 and `write_trace()` at line 2361), persist all task results as records.

**NDJSON format** (one JSON line per record):

```json
{"timestamp":"2026-03-28T10:30:00Z","workflow":"landing-page","task_id":"research","summary":"QR code trends...","confidence":0.9,"tokens_spent":1500,"cost_usd":0.005,"model":"deepseek-chat","duration_ms":3400}
```

**File naming**: `.nika/records/{workflow_name}_{ISO8601_compact}.ndjson`
- Example: `.nika/records/landing-page_20260328T103000Z.ndjson`

**Directory creation**: Create `.nika/records/` if not exists (same pattern as `.nika/traces/`)

**Retention**: Configurable TTL in `config.toml` with default 90 days. Cleanup logic in `nika doctor` command.

```rust
pub struct RecordWriter;

impl RecordWriter {
    pub fn write_records(
        workflow_name: &str,
        datastore: &RunContext,
        event_log: &EventLog,
    ) -> Result<PathBuf, NikaError> {
        let dir = Path::new(".nika/records");
        fs::create_dir_all(dir)?;

        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let filename = format!("{}_{}.ndjson", sanitize(workflow_name), timestamp);
        let path = dir.join(&filename);

        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);

        // Iterate all task results from RunContext
        for (task_id, result) in datastore.iter_results() {
            let record = build_record_line(&task_id, &result, event_log);
            serde_json::to_writer(&mut writer, &record)?;
            writer.write_all(b"\n")?;
        }

        writer.flush()?;
        Ok(path)
    }
}
```

**Integration**: Add `write_records()` call in `Runner::run()` after line 2332 (WorkflowCompleted). Only write if at least one task completed successfully.

**Estimated LOC**: ~120 (writer) + ~20 (runner integration) = **~140 LOC**

**Tests**: 5 tests
- Write records for a simple workflow
- File naming convention
- NDJSON format validation (each line is valid JSON)
- Empty workflow produces no file
- Sanitize workflow name for filesystem safety

### Part 8: SQLite FTS5 Index

**Location**: New file `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/store/record_index.rs`

Follow the existing daemon storage pattern from `/Users/thibaut/dev/supernovae/nika/tools/nika-daemon/src/storage.rs`: dedicated DB thread + mpsc channel.

**Dependencies**: Add `rusqlite = { version = "0.39", features = ["bundled"] }` to `nika-engine/Cargo.toml` (already used by `nika-daemon`, so the version is established).

**Schema**:

```sql
CREATE TABLE IF NOT EXISTS records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workflow_name TEXT NOT NULL,
    task_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    key_findings TEXT,  -- JSON array serialized as text
    confidence REAL,
    tokens_spent INTEGER,
    cost_usd REAL,
    model TEXT,
    duration_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
    summary,
    key_findings,
    workflow_name,
    task_id,
    content='records',
    content_rowid='id'
);

-- Triggers to keep FTS5 in sync
CREATE TRIGGER IF NOT EXISTS records_ai AFTER INSERT ON records BEGIN
    INSERT INTO records_fts(rowid, summary, key_findings, workflow_name, task_id)
    VALUES (new.id, new.summary, new.key_findings, new.workflow_name, new.task_id);
END;
```

**RecordIndex struct**:

```rust
pub struct RecordIndex {
    tx: mpsc::Sender<IndexCommand>,
}

enum IndexCommand {
    Insert { records: Vec<RecordRow>, reply: oneshot::Sender<Result<(), NikaError>> },
    Search { query: String, limit: usize, since: Option<String>, workflow: Option<String>,
             reply: oneshot::Sender<Result<Vec<SearchResult>, NikaError>> },
    Cleanup { retention_days: u32, reply: oneshot::Sender<Result<u64, NikaError>> },
}

pub struct SearchResult {
    pub workflow_name: String,
    pub task_id: String,
    pub summary: String,
    pub confidence: Option<f64>,
    pub created_at: String,
    pub rank: f64,  // FTS5 bm25 rank
}
```

**Integration with RecordWriter**: After writing NDJSON, also insert into FTS5 index. The index is the queryable view; NDJSON is the durable source of truth (append-only, easy to back up).

**Estimated LOC**: ~200 (index module) + ~30 (integration) = **~230 LOC**

**Tests**: 7 tests
- Insert records and search by keyword
- FTS5 ranking (BM25 relevance)
- Search with --since filter
- Search with --workflow filter
- Empty results
- Cleanup removes records older than retention
- Concurrent insert safety

### Part 9: `nika trace search` CLI Command

**Location**: Modify `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/trace.rs`

Add `Search` variant to the existing `TraceAction` enum:

```rust
#[derive(Subcommand)]
pub enum TraceAction {
    // ... existing variants (List, Show, Export, Clean) ...

    /// Search records across workflows using full-text search
    Search {
        /// Search query (FTS5 syntax supported)
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Only records since this date (ISO 8601)
        #[arg(long)]
        since: Option<String>,
        /// Filter by workflow name
        #[arg(short, long)]
        workflow: Option<String>,
    },
}
```

**Output format**:

```
 Records matching "QR code trends" (3 results)

  WORKFLOW                TASK         CONFIDENCE  DATE
  landing-page            research     0.90        2026-03-28
    QR code adoption in France grew 34% in 2025, with mobile payments...

  content-pipeline        analyze      0.85        2026-03-27
    Key findings: Dynamic QR codes outperform static by 3x in...

  seo-research            gather       0.78        2026-03-25
    QR code SEO impact: landing pages with QR integration show...
```

**Estimated LOC**: ~80 (CLI handler) + ~20 (formatting) = **~100 LOC**

**Tests**: 3 tests
- Search returns formatted results
- No results message
- Limit parameter respected

### Part 10: Frozen Snapshot Pattern

**Location**: Already partially implemented in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs` (lines 1171-1176)

The current code loads context files once at workflow start:

```rust
if !self.workflow.context_files.is_empty() {
    let loaded_context =
        load_context_analyzed(&self.workflow.context_files, &base_path).await?;
    self.datastore.set_context(loaded_context);
}
```

This is already "frozen" -- `LoadedContext` is set once in `RunContext` and never re-loaded. The `RunContext.context` field is behind `Arc<RwLock<LoadedContext>>` which is written once and only read thereafter.

**What's needed**:
1. **Document the pattern** explicitly in `context_loader.rs` and `RunContext` doc comments
2. **Prevent re-reads**: Add a `frozen: bool` flag to `LoadedContext` that prevents `set_context()` from being called twice
3. **Event**: Emit `ContextFrozen { file_count, total_tokens }` event after loading

**Estimated LOC**: ~25 (guard + event) + ~15 (doc comments) = **~40 LOC**

**Tests**: 2 tests
- Second `set_context()` call returns error when frozen
- `ContextFrozen` event emitted

### Part 11: File Locking (fcntl)

**Location**: New file `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/store/file_lock.rs`

Use `fs2` crate (well-maintained, cross-platform advisory locking) rather than raw `fcntl`.

**Add to Cargo.toml**: `fs2 = "0.4"`

```rust
use fs2::FileExt;

pub struct FileLock {
    file: File,
    path: PathBuf,
}

impl FileLock {
    /// Acquire an exclusive lock (blocks until available, with timeout)
    pub fn acquire(path: &Path, timeout: Duration) -> Result<Self, NikaError> {
        let file = File::create(path.with_extension("lock"))?;
        // Try lock with timeout via polling (fs2 doesn't have native timeout)
        let deadline = Instant::now() + timeout;
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Self { file, path: path.to_path_buf() }),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(NikaError::FileLockTimeout { path: path.display().to_string(), source: e.to_string() }),
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        let _ = fs::remove_file(self.path.with_extension("lock"));
    }
}
```

**Usage**: Wrap `RecordWriter::write_records()` and `RecordIndex` inserts with `FileLock`.

**Estimated LOC**: ~60 (lock module) + ~15 (integration) = **~75 LOC**

**Tests**: 4 tests
- Acquire and release lock
- Second lock blocks until first released
- Timeout on contested lock
- RAII cleanup on drop

### Part 12: Security Scanning

**Location**: New file `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/output_scanner.rs`

Inspired by Hermes `skills_guard.py` patterns. Scan LLM outputs before they become inputs to downstream tasks.

**Patterns to detect**:

```rust
pub struct OutputScanner {
    patterns: Vec<ScanPattern>,
}

struct ScanPattern {
    name: &'static str,
    regex: Regex,
    severity: Severity, // Warning, Dangerous
}

enum Severity { Warning, Dangerous }

impl OutputScanner {
    pub fn default_patterns() -> Self {
        Self {
            patterns: vec![
                // Invisible Unicode (zero-width chars, directional overrides)
                ScanPattern::new("invisible_unicode",
                    r"[\u{200B}-\u{200F}\u{2028}-\u{202F}\u{2060}-\u{206F}\u{FEFF}]",
                    Severity::Dangerous),

                // Exfiltration: curl/wget with variable interpolation
                ScanPattern::new("exfiltration_curl",
                    r"(?i)(curl|wget|fetch)\s+.*\$\{?\w*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL)\w*\}?",
                    Severity::Dangerous),

                // Role hijack: attempts to override system prompt
                ScanPattern::new("role_hijack",
                    r"(?i)(you are now|ignore previous|system:\s*you)",
                    Severity::Warning),

                // Prompt injection: markdown/XML injection
                ScanPattern::new("prompt_injection",
                    r"(?i)(<\s*/?\s*system\s*>|```system|<\|im_start\|>system)",
                    Severity::Dangerous),

                // Data URI exfiltration
                ScanPattern::new("data_uri_exfil",
                    r"!\[.*\]\(https?://.*\?.*=\{\{",
                    Severity::Dangerous),
            ],
        }
    }

    pub fn scan(&self, text: &str) -> Vec<ScanFinding> { ... }
    pub fn sanitize(&self, text: &str) -> String { ... }  // Remove dangerous patterns
}
```

**Workflow-level config** (additive field on workflow):

```yaml
security:
  scan_outputs: true  # default: false in v0.54, true in v1.0
```

**Integration point**: In `runner.rs`, after a task completes but before storing its output in `RunContext`, run the scanner. On `Dangerous` findings: sanitize + emit `SecurityScanFinding` event. On `Warning`: emit event only.

**New EventKind variant**:

```rust
SecurityScanFinding {
    task_id: Arc<str>,
    pattern: String,
    severity: String,  // "warning" | "dangerous"
    sanitized: bool,
}
```

**Estimated LOC**: ~150 (scanner) + ~20 (event) + ~30 (integration) = **~200 LOC**

**Tests**: 8 tests
- Invisible Unicode detection
- Curl exfiltration detection
- Role hijack detection
- Clean text passes
- Sanitize removes dangerous patterns
- Warning-only patterns not sanitized
- Event emitted on finding
- Scan disabled when `scan_outputs: false`

### Part 13: Background Nudge System (Optional)

**Location**: New file `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/nudge.rs`

This is explicitly marked as **optional** and **best-effort**. Inspired by the Hermes background review (Layer 3).

**Design**:

```rust
pub struct NudgeSystem {
    interval: u32,  // Check every N workflow completions
    counter: AtomicU32,
}

impl NudgeSystem {
    /// Called after workflow completion. May spawn a review agent.
    pub async fn maybe_nudge(
        &self,
        workflow_name: &str,
        event_log: &EventLog,
        datastore: &RunContext,
    ) {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        if count % self.interval != 0 {
            return;  // Not time yet
        }

        // Spawn background review (non-blocking)
        tokio::spawn(async move {
            if let Err(e) = run_nudge_review(workflow_name, event_log, datastore).await {
                tracing::debug!(error = %e, "Nudge review failed (best-effort)");
            }
        });
    }
}
```

**Nudge storage**: `.nika/nudges/{workflow_name}.md`

```markdown
# Nudge for landing-page (2026-03-28)

## Suggestion: Add context_budget to research task
- **Reason**: research task output was 12,000 tokens, but generate only needed 3,000
- **Confidence**: 0.7
- **Fix**: Add `context_budget: 4000` to the research task

## Suggestion: Use record compression
- **Reason**: Direct output passing caused 40% token waste
- **Confidence**: 0.8
- **Fix**: Add `record: { compress: true, max_tokens: 500 }` to research
```

**CLI**: `nika nudge list` and `nika nudge apply <workflow>` (read-only in v0.55; apply in future version).

**Configuration**: `nudge: { enabled: false, interval: 10 }` in config.toml. Off by default in v0.55.

**Estimated LOC**: ~120 (nudge system) + ~60 (storage/CLI) = **~180 LOC**

**Tests**: 4 tests
- Counter increments, nudge fires at interval
- Nudge file written to correct path
- Failure is silent (best-effort)
- Disabled when `enabled: false`

---

## Part 14: Test Summary

| Area | Test Count |
|------|-----------|
| Part 1: context_budget AST | 6 |
| Part 2: Token counting | 5 |
| Part 3: Budget enforcement | 8 |
| Part 4: Events | 3 |
| Part 5: Introspection tools | 10 |
| Part 6: Registration | 2 |
| Part 7: NDJSON persistence | 5 |
| Part 8: SQLite FTS5 | 7 |
| Part 9: trace search CLI | 3 |
| Part 10: Frozen snapshot | 2 |
| Part 11: File locking | 4 |
| Part 12: Security scanning | 8 |
| Part 13: Nudge system | 4 |
| **Total** | **67** |

Note: This exceeds the initial 40-50 estimate but reflects the actual scope. All tests use `#[test]` or `#[tokio::test]` with `--lib` safety (no keychain, no network).

---

## Part 15: Timeline

```
Week 9 (Mon-Fri):
  Day 1-2: Part 1 (context_budget field) + Part 2 (token counting)
  Day 3-4: Part 3 (budget enforcement) + Part 4 (events)
  Day 5:   Integration testing, Part 10 (frozen snapshot)

Week 10 (Mon-Fri):
  Day 1-2: Part 5 (dag_info + task_status tools)
  Day 3-4: Part 5 continued (threads + orchestrate tools) + Part 6 (registration)
  Day 5:   Part 12 (security scanning)

Week 11 (Mon-Fri):
  Day 1-2: Part 7 (NDJSON record persistence)
  Day 3-4: Part 8 (SQLite FTS5 index)
  Day 5:   Part 9 (nika trace search CLI)

Week 12 (Mon-Fri):
  Day 1:   Part 11 (file locking)
  Day 2-3: Part 13 (nudge system — optional, can defer)
  Day 4-5: Integration testing, edge cases, cargo clippy clean
```

**Parallelizable tracks**:
- P-CONTEXT (Parts 1-4) and P-INTROSPECT (Parts 5-6) can run in parallel on different branches
- Part 12 (security scanning) is independent of everything else
- Part 13 (nudge) is fully optional and can be deferred to v0.56

---

## Part 16: File Summary with LOC Estimates

### New Files

| File | LOC | Purpose |
|------|-----|---------|
| `nika-engine/src/binding/token_budget.rs` | ~220 | Token estimation + budget enforcement |
| `nika-engine/src/runtime/builtin/introspect_dag.rs` | ~80 | nika:dag_info tool |
| `nika-engine/src/runtime/builtin/introspect_task.rs` | ~100 | nika:task_status tool |
| `nika-engine/src/runtime/builtin/introspect_threads.rs` | ~90 | nika:threads tool |
| `nika-engine/src/runtime/builtin/introspect_orchestrate.rs` | ~70 | nika:orchestrate tool |
| `nika-engine/src/store/record_writer.rs` | ~140 | NDJSON record persistence |
| `nika-engine/src/store/record_index.rs` | ~230 | SQLite FTS5 index |
| `nika-engine/src/store/file_lock.rs` | ~75 | Advisory file locking (fs2) |
| `nika-engine/src/runtime/output_scanner.rs` | ~200 | Security scanning for LLM outputs |
| `nika-engine/src/runtime/nudge.rs` | ~180 | Background review + nudge system |

### Modified Files

| File | Changes | LOC Delta |
|------|---------|-----------|
| `nika-core/src/ast/raw/task.rs` | Add `context_budget` field | +5 |
| `nika-core/src/ast/raw/parser.rs` | Parse `context_budget`, add to KNOWN_TASK_KEYS | +15 |
| `nika-core/src/ast/analyzed/task.rs` | Add `context_budget` field | +5 |
| `nika-core/src/ast/analyzer/` | Validate context_budget range | +25 |
| `nika-event/src/log.rs` | Add BudgetOk, BudgetExceeded, SecurityScanFinding, ContextFrozen events | +40 |
| `nika-engine/src/binding/resolve.rs` | Budget check after resolution | +20 |
| `nika-engine/src/runtime/runner.rs` | Wire budget enforcement, record persistence, security scanning, nudge | +60 |
| `nika-engine/src/runtime/builtin/mod.rs` | Export introspection tools | +15 |
| `nika-engine/src/runtime/builtin/router.rs` | `with_introspection()` constructor | +25 |
| `nika-engine/src/runtime/context_loader.rs` | Frozen guard on LoadedContext | +15 |
| `nika-engine/src/display/format_event.rs` | Format new event kinds | +30 |
| `nika-cli/src/trace.rs` | Add Search subcommand | +80 |
| `nika-engine/Cargo.toml` | Add rusqlite, fs2, regex deps | +5 |

### Total

| Category | LOC |
|----------|-----|
| New files | ~1,385 |
| Modified files | ~340 |
| Tests (est. 67 tests) | ~800 |
| **Grand total** | **~2,525 LOC** |

### Dependencies to Add

| Crate | Version | Used In | Purpose |
|-------|---------|---------|---------|
| `rusqlite` | 0.39 | nika-engine | FTS5 record index (already in nika-daemon) |
| `fs2` | 0.4 | nika-engine | Advisory file locking |
| `regex` | (already dep) | nika-engine | Security scan patterns |
| `chrono` | (already dep) | nika-engine | NDJSON timestamps |

### Critical Files for Implementation

- `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs` -- parse `context_budget`, extend `KNOWN_TASK_KEYS`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/binding/resolve.rs` -- budget enforcement integration point after `from_with_spec_traced()`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/router.rs` -- register 4 introspection tools
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs` -- post-workflow record persistence hook (line 2332-2361), security scanning, nudge wiring
- `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs` -- new EventKind variants for budget, security, and context events