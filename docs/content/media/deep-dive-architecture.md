# Deep Dive: Inside Nika -- 10 Crates, 1.56M Lines, Zero Compromises

> 20-minute technical deep dive
> Target audience: Rust developers, systems engineers, compiler/runtime enthusiasts
> Format: split-screen code walkthrough + animated architecture diagrams
> Resolution: 4K preferred, terminal font JetBrains Mono 14pt

---

## Opening Sequence (0:00 - 1:00)

[ANIMATION] A single `.nika.yaml` file icon appears at the top of the screen. Below it, the full architecture diagram builds itself in real time -- AST phases, DAG, executor branches, all animated with flowing data particles.

**Voice-over:**
A YAML file goes in. Structured, validated, parallel AI execution comes out. In between: a compiler-grade pipeline, an immutable DAG, a concurrent runtime, and ten carefully bounded crates. This is the inside of Nika -- a semantic workflow engine that takes ideas from rustc, Tokio, and content-addressable storage systems and applies them to AI workflow execution. Let's trace every step.

[TITLE CARD] "Inside Nika -- 10 Crates, 1.56M Lines, Zero Compromises"

---

## CHAPTER 1: The Workspace (1:00 - 3:30)

### Scene 1.1 -- Ten Crates, Ten Responsibilities (1:00 - 2:00)

[SLIDE] Animated crate dependency graph:

```
                    nika (CLI binary, 2k)
                   /    |    \
                  /     |     \
     nika-cli (8k)  nika-tui (92k)  nika-lsp (2.5k)
          |              |              |
     nika-engine (134k)            nika-lsp-core (9k)
      /    |    \                       |
     /     |     \                 nika-core (23k)
nika-mcp  nika-media  nika-event
 (9k)      (3.5k)      (4k)
```

[ANIMATION] Each crate node pulses as it is mentioned. Dependency arrows draw themselves.

**Voice-over:**
Nika is a Cargo workspace with twelve crates. Not because we like splitting things -- because each boundary enforces an architectural invariant.

`nika-core` at the bottom: zero I/O. Pure types, AST definitions, transform catalogs. It compiles without touching a file system or network. This is the foundation everything else builds on.

`nika-engine` in the middle: the embeddable runtime. One hundred thirty-four thousand lines. This is where YAML becomes execution. You can embed this in any Rust application -- it is not tied to the CLI.

`nika-tui` on the side: ninety-two thousand lines of ratatui terminal UI. It is the largest crate by feature count, but it is completely optional. The engine does not know the TUI exists.

`nika-mcp`: MCP client wrapping rmcp 0.16. `nika-media`: content-addressable storage. `nika-event`: event log and NDJSON tracing. Each one: a clear responsibility, a clean API, a testable boundary.

### Scene 1.2 -- Cargo.toml Walkthrough (2:00 - 2:45)

[SCREEN] Split screen. Left: `tools/Cargo.toml`. Right: key dependency highlights.

[ZOOM] On workspace dependencies:

```toml
[workspace.dependencies]
tokio = { version = "1.49", features = ["rt-multi-thread", ...] }
rig-core = { version = "0.32", features = ["rmcp"] }
rmcp = { version = "0.16", features = ["client", "transport-child-process"] }
ratatui = "0.30"
petgraph = { version = "0.6", features = ["serde-1"] }
blake3 = { version = "1.8", features = ["mmap"] }
serde-saphyr = "0.0.20"
```

**Voice-over:**
The dependency choices are deliberate. Tokio for async -- multi-threaded runtime, not single-threaded. rig-core 0.32 for LLM provider abstraction -- it gives us eight providers through one API. rmcp 0.16 for MCP client communication. petgraph for the TUI's StableGraph DAG visualization. blake3 for content-addressable hashing -- with mmap support for large files. And serde-saphyr for YAML parsing with YAML bomb protection built in. Not serde-yaml. serde-saphyr. Because YAML billion-laugh attacks are real.

### Scene 1.3 -- The Build Profile (2:45 - 3:30)

[ZOOM] On release profile:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = true

[profile.test]
opt-level = 1
```

**Voice-over:**
The release profile: thin LTO for link-time optimization, single codegen unit for maximum optimization, symbols stripped for binary size. The test profile runs at opt-level 1 -- not zero. Why? Because seven thousand eight hundred tests at zero optimization is painfully slow, but full optimization makes compile times unbearable. Opt-level 1 is the sweet spot.

The result: a release binary under 30 megabytes that starts in under 10 milliseconds. Rust edition 2021. Minimum supported Rust version: 1.86. License: AGPL-3.0-or-later.

---

## CHAPTER 2: The Three-Phase AST (3:30 - 8:00)

### Scene 2.1 -- Why Three Phases (3:30 - 4:30)

[ANIMATION] Three boxes appear in sequence, connected by arrows:

```
Phase 1: Raw AST          Phase 2: Analyzed AST      Phase 3: Lowered Types
(YAML -> spans)            (validated, resolved)       (runtime-ready)
```

[ANIMATION] Each box expands to show its contents. Phase 1 fills with YAML tokens. Phase 2 shows validation checkmarks appearing. Phase 3 shows runtime structs forming.

**Voice-over:**
The AST pipeline is inspired by rustc's multi-phase compilation. Three phases, each with strict guarantees.

Phase 1 is raw parsing. YAML goes in, a raw AST comes out with source spans attached to every node. Every field is Optional. We make no assumptions about validity. If the YAML parses, we produce a Raw AST. The source spans are critical -- they let us point error messages to exact lines in the original file.

Phase 2 is analysis. The raw AST goes through the Analyzer -- a set of validation and transformation passes. Task IDs get interned into Arc strings. Bindings get resolved. Dependencies get checked. Model references get validated. If anything is wrong, the error includes the source span from Phase 1. After analysis, every field that should exist does exist. No more Options.

Phase 3 is lowering. Analyzed types get converted to runtime types. InferParams, ExecParams, FetchParams, InvokeParams, AgentParams -- concrete structs optimized for execution, not parsing. After lowering, no validation ever runs again. The runtime trusts the types completely.

### Scene 2.2 -- Phase 1: Raw Parsing (4:30 - 5:30)

[SCREEN] Split screen. Left: YAML source. Right: Raw AST structs.

[ZOOM] On `tools/nika-core/src/ast/raw/parser.rs`

```rust
pub struct RawTask {
    pub id: Option<Spanned<String>>,
    pub infer: Option<Spanned<RawInfer>>,
    pub fetch: Option<Spanned<RawFetch>>,
    pub invoke: Option<Spanned<RawInvoke>>,
    pub agent: Option<Spanned<RawAgent>>,
    pub depends_on: Option<Vec<Spanned<String>>>,
    pub with: Option<Map<String, Spanned<String>>>,
    // ... all Optional
}
```

[ANIMATION] Arrows from YAML fields to corresponding struct fields. Red "Optional" badges on every field.

**Voice-over:**
Look at the Raw AST. Everything is `Option`. The parser's job is simple: turn YAML into Rust structs without judging. Is the `id` missing? Fine -- it is None. Are there two verbs on one task? Fine -- both are Some. We do not validate here. We parse. The `Spanned` wrapper carries source location information: line number, column, byte offset. This is how error messages later can say "line 14, column 3: missing task ID."

The parser uses `marked-yaml` under the hood, with YAML bomb protection from `serde-saphyr`. Billion-laugh attacks, entity expansion bombs, deep nesting attacks -- all handled at the parser level.

### Scene 2.3 -- Phase 2: The Analyzer (5:30 - 6:45)

[SCREEN] Code walkthrough of `tools/nika-core/src/ast/analyzer/`

[ZOOM] On validation passes:

```rust
// Pseudo-structure of analyzer passes:
// 1. Validate schema version
// 2. Intern task IDs (String -> Arc<str>)
// 3. Check each task has exactly one verb
// 4. Validate depends_on references exist
// 5. Check with: bindings reference valid tasks
// 6. Validate model references against provider catalog
// 7. Check for duplicate task IDs
// 8. Validate output schemas
// 9. Security policy checks
```

[ANIMATION] A checklist appearing item by item, each with a green checkmark animation.

**Voice-over:**
The analyzer runs nine categories of validation passes. Schema version check -- is this `@0.12`? Task ID interning -- every string task ID becomes an `Arc<str>` through an interner, so comparisons are pointer-equal instead of string-equal. Verb exclusivity -- each task must have exactly one verb. Dependency resolution -- every `depends_on` target must be a real task ID. Binding validation -- every `$task_id` in a `with:` block must reference a valid upstream task.

The output is an `AnalyzedWorkflow` where nothing is Optional anymore. If you have an `AnalyzedTask`, it has an ID. It has exactly one verb. Its dependencies are resolved. Its bindings are valid. The type system encodes the validation -- downstream code cannot receive an invalid workflow.

### Scene 2.4 -- Phase 3: Lowering (6:45 - 7:30)

[SCREEN] Code walkthrough of `tools/nika-engine/src/ast/lower.rs`

[ZOOM] On the `lower_action` function:

```rust
pub fn lower_action(action: &AnalyzedTaskAction) -> Result<TaskAction, NikaError> {
    match action {
        AnalyzedTaskAction::Infer(params) => {
            Ok(TaskAction::Infer(InferParams {
                model: params.model.clone(),
                prompt: params.prompt.clone(),
                system: params.system.clone(),
                temperature: params.temperature,
                max_tokens: params.max_tokens,
                content: params.content.as_ref().map(lower_content),
                // ... runtime-optimized fields
            }))
        }
        // ... other verbs
    }
}
```

**Voice-over:**
Lowering converts analyzed types to runtime types. This is where the abstraction shifts from "what the user wrote" to "what the engine executes." `AnalyzedInfer` becomes `InferParams`. The types are flattened, defaults applied, and optional fields resolved to concrete values.

After lowering, the runtime never validates again. It trusts the types. This is the compiler guarantee: if it lowered, it is valid. No runtime checks, no assertion sprinkle, no "should never happen" branches. The phase boundaries are the validation boundaries.

### Scene 2.5 -- Why This Matters (7:30 - 8:00)

[SLIDE] Comparison with other frameworks:

```
LangChain:  Parse -> Execute  (runtime errors)
CrewAI:     Parse -> Execute  (runtime errors)
Nika:       Parse -> Analyze -> Lower -> Execute  (compile-time errors)
```

[ANIMATION] Error indicators: red explosions on the "Execute" step for LangChain/CrewAI. Green shield on the "Analyze" step for Nika.

**Voice-over:**
Most AI frameworks have two phases: parse the configuration and run it. If something is wrong, you find out at runtime -- maybe after burning API tokens, maybe after ten minutes of work. Nika catches errors before any task runs. The three-phase pipeline means validation errors come with source spans, before you spend a single API dollar. It is the same philosophy as a compiler: reject bad programs early, with good error messages, at the right level of abstraction.

---

## CHAPTER 3: The DAG (8:00 - 11:00)

### Scene 3.1 -- DAG Construction (8:00 - 9:00)

[ANIMATION] A workflow with six tasks appears. Lines draw between dependent tasks, forming a DAG. Nodes arrange themselves into topological layers.

```
[fetch_a]  [fetch_b]        <-- Layer 0 (parallel)
    \       /     \
  [merge]    [transform]    <-- Layer 1 (parallel)
      \       /
     [analyze]              <-- Layer 2
        |
     [report]               <-- Layer 3
```

[SCREEN] Code from `tools/nika-engine/src/dag/`

**Voice-over:**
After the AST is lowered, the dependency graph gets built. Every `depends_on:` creates a directed edge. The resulting DAG is validated: no cycles allowed. Nika uses Kahn's algorithm for topological sorting -- an iterative approach that naturally detects cycles by checking if all nodes were visited.

The DAG has three implementations in the codebase. `flow.rs`: an immutable HashMap-based DAG for the runtime -- once built, it cannot be modified, making concurrent access safe without locks. `indexed.rs`: a Vec-based adjacency list with Kahn's algorithm for the topological sort. `stable.rs`: a petgraph StableGraph for the TUI, where nodes can be added and removed for visualization purposes.

### Scene 3.2 -- Parallel Execution Strategy (9:00 - 10:00)

[ANIMATION] The DAG layers light up one by one. Layer 0 tasks launch simultaneously (two task boxes light up at once). When both complete, Layer 1 launches (two more). Then Layer 2 (one task). Then Layer 3 (one task).

[SCREEN] Code from `runner.rs`:

```rust
// Simplified execution loop
let mut join_set = JoinSet::new();

for task in ready_tasks {
    let executor_clone = executor.clone();
    join_set.spawn(async move {
        executor_clone.run(task).await
    });
}

while let Some(result) = join_set.join_next().await {
    // Mark task complete, check for newly ready tasks
    let completed = result??;
    for dependent in dag.dependents(completed.id) {
        if dag.all_deps_met(dependent) {
            join_set.spawn(/* ... */);
        }
    }
}
```

**Voice-over:**
Execution uses Tokio's JoinSet for maximum parallelism. All tasks with no unmet dependencies launch immediately as Tokio tasks. When a task completes, the runner checks which dependents now have all their dependencies met, and spawns those too. No thread pool sizing. No concurrency limits by default. No artificial bottlenecks. The DAG structure IS the concurrency control.

CancellationToken propagates cancellation across all tasks. If fail-fast mode is enabled and one task fails, all running tasks get cancelled. If a task hits its timeout, only that task cancels -- dependencies that already completed keep their results.

### Scene 3.3 -- The RunContext (10:00 - 11:00)

[SCREEN] Code from `store.rs`:

```rust
pub struct RunContext {
    results: DashMap<Arc<str>, TaskResult>,
    // ...
}

pub struct TaskResult {
    pub output: String,
    pub duration: Duration,
    pub token_usage: Option<TokenUsage>,
    pub cost: Option<f64>,
    pub media: Vec<MediaRef>,
    pub artifacts: Vec<ArtifactRef>,
}
```

[ANIMATION] Tasks writing results into the RunContext as they complete. Downstream tasks reading from it when they start. Arrows show the data flow.

**Voice-over:**
The RunContext is the runtime's shared state. It is a DashMap -- a concurrent, sharded HashMap. No global lock. Multiple tasks can write results simultaneously without blocking each other.

Each TaskResult stores the output string, duration, token usage, cost, and any media or artifact references. When a downstream task resolves its `with:` bindings, it reads from the RunContext. The `$task_id` reference in a binding becomes a key lookup into this map. Template resolution happens at execution time, not at DAG construction time -- so values are always fresh and ordering is guaranteed by the DAG edges.

---

## CHAPTER 4: The Executor (11:00 - 15:00)

### Scene 4.1 -- Verb Dispatch (11:00 - 12:00)

[SCREEN] Code from `executor/mod.rs` and `executor/verbs.rs`:

```rust
impl TaskExecutor {
    pub async fn dispatch(&self, task: &LoweredTask) -> Result<TaskResult, NikaError> {
        match &task.action {
            TaskAction::Infer(params) => self.run_infer(task_id, params, ...).await,
            TaskAction::Exec(params)  => self.run_exec(task_id, params, ...).await,
            TaskAction::Fetch(params) => self.run_fetch(task_id, params, ...).await,
            TaskAction::Invoke(params) => self.run_invoke(task_id, params, ...).await,
            TaskAction::Agent(params) => self.run_agent(task_id, params, ...).await,
        }
    }
}
```

[ANIMATION] Five paths branching from the match statement, each going to its verb implementation.

**Voice-over:**
The TaskExecutor is the nerve center. Each task dispatches to exactly one verb handler through a simple match. No trait objects, no dynamic dispatch, no plugin system. The five verbs are the five arms of a match expression. This is deliberate -- the verb set is closed. You do not extend it with plugins; you compose with the existing verbs.

The executor holds shared resources: a DashMap cache of initialized providers (so we do not create a new HTTP client for every Claude call), an MCP client pool, a CAS store reference, and the event log for tracing.

### Scene 4.2 -- infer: Under the Hood (12:00 - 13:00)

[SCREEN] Code walkthrough of `run_infer` in `verbs.rs`:

[ANIMATION] Flow diagram:
```
resolve templates
    |
    v
validate prompt (non-empty)
    |
    v
inject JSON schema instruction (if output policy)
    |
    v
build content parts (if vision)
    |
    v
get or create RigProvider
    |
    v
call provider.completion() or provider.stream()
    |
    v
apply structured output layers (0-4)
    |
    v
return result + token usage + cost
```

**Voice-over:**
The infer path is the most complex verb. Template resolution first -- all `{{with.alias}}` placeholders get replaced with actual values. Then prompt validation: an empty prompt after resolution is an error, not a silent failure. Unless we are in vision mode, where content blocks carry the payload.

If the task has an output schema, Nika injects a JSON schema instruction into the prompt. Then it builds the completion request through rig-core. Provider caching means the first call to Claude creates the client, the second call reuses it. Streaming is supported for all providers -- the event log captures stream chunks for the TUI to display.

After the LLM responds, the structured output engine takes over. Five layers: provider-native, extraction, validation, retry, LLM repair. The engine escalates automatically if lower layers fail.

### Scene 4.3 -- Security in the Runtime (13:00 - 13:45)

[SCREEN] Code from `security.rs`:

```rust
// NFKC normalization prevents Unicode homoglyph attacks
fn validate_command(cmd: &str) -> Result<(), NikaError> {
    let normalized = cmd.nfkc().collect::<String>();
    for pattern in BLOCKED_COMMANDS {
        if normalized.contains(pattern) {
            return Err(NikaError::SecurityViolation { ... });
        }
    }
    Ok(())
}
```

**Voice-over:**
The runtime takes security seriously at multiple levels. Shell commands go through NFKC normalization before blocklist checking -- this prevents attacks where destructive commands are spelled with visually similar Unicode characters to bypass pattern matching. File imports validate paths against directory traversal. Media operations enforce size limits -- fifty megabytes by default. SVG files get sanitized before parsing to prevent XML entity expansion and script injection.

The PolicyEnforcer runs at boot time, before your first task starts. It validates environment variables, checks provider credentials, and enforces workspace-level security policies. Security is a compile-time and boot-time guarantee, not a runtime hope.

### Scene 4.4 -- fetch: Nine Extraction Modes (13:45 - 14:30)

[SLIDE] The nine extraction modes as a feature matrix:

```
Mode         Library           Output
------------ ----------------- --------------------------
markdown     htmd              Clean Markdown
article      dom_smoothie      Main article content
text         built-in          Visible text (+ selector)
selector     scraper           Raw HTML of CSS matches
metadata     built-in          OG/Twitter/JSON-LD/SEO
links        built-in          Classified link inventory
jsonpath     built-in          JSONPath query result
feed         feed-rs           Parsed RSS/Atom/JSON Feed
llm_txt      built-in          AI-era content discovery
```

[ANIMATION] An HTTP response enters from the left. It passes through a "mode selector" that routes it to one of nine processing paths. The cleaned output exits on the right.

**Voice-over:**
The fetch verb is not just an HTTP client. It is an HTTP client with a post-processing pipeline. Nine extraction modes, each backed by a purpose-built library. `markdown` uses htmd for HTML-to-Markdown conversion -- better than regex, better than stripping tags. `article` uses dom_smoothie, a Readability-style algorithm that extracts the main content from a web page. `jsonpath` queries JSON responses without any external dependency. `feed` parses RSS, Atom, and JSON Feed formats.

Plus response modes: `full` returns status, headers, body, and final URL as structured JSON. `binary` stores the response in CAS and returns a hash -- perfect for feeding into the media pipeline. Default returns raw body text.

### Scene 4.5 -- invoke: MCP Client (14:30 - 15:00)

[SCREEN] Code from `tools/nika-mcp/`:

```rust
pub struct McpClientPool {
    clients: DashMap<String, Arc<McpClient>>,
    // Connection pooling, retry, timeout
}
```

[ANIMATION] Nika sending an invoke request through MCP to an external tool server. The response flows back.

**Voice-over:**
The invoke verb calls MCP tools through rmcp 0.16. The MCP client pool manages connections -- each unique server address gets one client, reused across tasks. Schema validation happens before the call: Nika checks that the input you provide matches the tool's declared input schema. Retry is built in with exponential backoff. Connection timeout and task deadline are separate -- you can have a long-running tool call without worrying about connection drops.

This is how Nika talks to NovaNet, but it is not limited to NovaNet. Any MCP-compatible tool server works. GitHub, Slack, databases, custom services -- if it speaks MCP, Nika can invoke it.

---

## CHAPTER 5: Supporting Systems (15:00 - 18:30)

### Scene 5.1 -- Content-Addressable Storage (15:00 - 16:00)

[ANIMATION] File import flow:

```
file.png
    |
    v
blake3 hash --> "3a7f2b..."
    |
    v
zstd compress --> .nika/cas/3a/7f/3a7f2b...
    |
    v
MediaRef { hash: "3a7f2b...", mime: "image/png", size: 48210 }
```

[SCREEN] Show the CAS directory structure.

**Voice-over:**
The media system uses content-addressable storage. Import a file, get a blake3 hash. The file is compressed with zstd and stored by hash. Import the same file twice? Same hash, no duplicate storage. Twenty-four media tools operate on these hashes -- thumbnail, convert, optimize, chart, provenance. The `nika:pipeline` tool chains operations in memory with zero intermediate files. Reflink-copy on supported file systems means even extracting a file does not duplicate bytes on disk.

### Scene 5.2 -- Event Sourcing (16:00 - 17:00)

[SCREEN] Code from `tools/nika-event/src/`:

```rust
pub enum EventKind {
    WorkflowStarted,
    TaskStarted { task_id: Arc<str> },
    TaskCompleted { task_id: Arc<str>, duration: Duration },
    InferStream { task_id: Arc<str>, chunk: String },
    FetchComplete { task_id: Arc<str>, status: u16 },
    AgentTurn { task_id: Arc<str>, turn: u32 },
    GuardrailEvaluated { task_id: Arc<str>, passed: bool },
    // ... 41 total variants
}
```

[ANIMATION] Events flowing into an append-only log. A NDJSON file growing line by line. A replay arrow pointing backward from the log to a workflow visualization.

**Voice-over:**
Every significant action emits an event. Forty-one event kinds covering the entire lifecycle: workflow start, task start, task completion, inference streaming, fetch results, agent turns, guardrail evaluations, errors, cancellations. The event log is append-only -- events are never modified or deleted.

NDJSON trace files capture everything for offline analysis. The TraceWriter handles concurrent writes safely. The TUI subscribes to the event stream for real-time updates. And because events are structured -- not log strings -- you can filter, query, and replay them programmatically.

### Scene 5.3 -- The Error System (17:00 - 17:45)

[SCREEN] Code from `error.rs`:

```rust
pub enum NikaError {
    // NIKA-000 to NIKA-009: Workflow
    WorkflowNotFound { path: PathBuf },
    WorkflowParseError { source: String, span: Option<SourceSpan> },

    // NIKA-010 to NIKA-019: Schema
    SchemaVersionMismatch { expected: String, found: String },

    // NIKA-020 to NIKA-029: DAG
    CycleDetected { cycle: Vec<String> },

    // ... 320 error codes total
}
```

[ANIMATION] Error code spectrum visualization -- a horizontal bar with colored segments for each range.

**Voice-over:**
Nika has over three hundred distinct error codes organized into categories. NIKA-000 through 009 for workflow-level errors. 010 through 019 for schema validation. 020 through 029 for DAG issues. Every error carries a source span when applicable -- the exact line in your YAML. Every error has a unique code for searchability. No "something went wrong." No generic error types. When Nika fails, it tells you what failed, where it failed, and what the error code is.

The error type is `NikaError`, not `anyhow::Error`. This is enforced by convention and code review. Anyhow erases type information. NikaError preserves it. Every error path in the codebase produces a specific, categorized, span-annotated error.

### Scene 5.4 -- The LSP (17:45 - 18:30)

[SCREEN] Split screen. Left: VS Code with a `.nika.yaml` file. Right: LSP crate structure.

```
nika-lsp-core (9k lines)    -- Protocol-agnostic intelligence
    |
    v
nika-lsp (2.5k lines)       -- tower-lsp-server binding
```

[ANIMATION] LSP completions appearing in the editor. Hover documentation showing. Diagnostic squiggles appearing for errors.

**Voice-over:**
The LSP is split into two crates. `nika-lsp-core` contains all the intelligence: completion providers, diagnostic analyzers, hover documentation, go-to-definition, semantic tokens. It is protocol-agnostic -- it knows about Nika workflows but not about the Language Server Protocol itself.

`nika-lsp` is the thin shell that binds the intelligence to tower-lsp-server. Twelve handlers cover the LSP feature surface: completions, diagnostics, hover, document symbols, semantic tokens, code actions. The split means you could use the intelligence layer for other purposes -- a web editor, a validation API, a linting tool -- without pulling in the LSP protocol machinery.

---

## CHAPTER 6: Design Decisions (18:30 - 20:00)

### Scene 6.1 -- Why YAML, Not Python (18:30 - 19:00)

[SLIDE] Two columns:

```
YAML-first                          Code-first
- Declarative                       - Imperative
- Validatable before run            - Errors at runtime
- Serializable (store, share)       - Tied to runtime
- LSP-powered (completions)         - IDE support varies
- DAG is explicit                   - Control flow is implicit
```

**Voice-over:**
Why YAML instead of Python? Because declarative workflows are validatable before execution. Because YAML is serializable -- you can store, share, version-control, and diff workflows without worrying about code semantics. Because a schema-backed YAML file can power an LSP with completions and diagnostics. Because the DAG is explicit in the structure, not hidden in imperative control flow.

The five verbs are the escape hatch. If you need arbitrary logic, the `exec` verb runs any shell command. If you need an LLM to decide what to do, `agent:` provides multi-turn autonomy. YAML for structure, verbs for power.

### Scene 6.2 -- Why Rust, Not Node/Python (19:00 - 19:30)

[SLIDE] Performance comparison as animated bar chart:

```
                Nika (Rust)    Python frameworks
Startup:        <10ms          ~2s
Memory:         ~8MB           ~120MB
Concurrency:    Native tokio   asyncio/threads
Type safety:    Compile-time   Runtime
Binary:         Single static  venv + deps
```

**Voice-over:**
Why Rust? Startup under ten milliseconds versus two seconds. Eight megabytes of memory versus one hundred and twenty. Native concurrency through Tokio versus asyncio's cooperative multitasking. Compile-time type safety versus runtime assertions. A single static binary versus a virtual environment with two hundred transitive dependencies.

But it is not just performance. Rust's type system lets us encode the AST phase guarantees at the type level. A `Raw` AST and an `Analyzed` AST are different types -- you cannot accidentally pass a raw, unvalidated workflow to the executor. The compiler enforces the pipeline.

### Scene 6.3 -- Why AGPL, Not MIT (19:30 - 19:45)

[SLIDE] Single statement, large text:

```
AGPL-3.0-or-later
"If you benefit from the commons, contribute to the commons."
```

**Voice-over:**
AGPL, not MIT. Not because we are anti-business -- because we are pro-commons. MIT lets cloud providers wrap open source in proprietary services. AGPL requires that if you modify Nika and offer it as a service, you share your modifications. The freedom to use comes with the responsibility to share.

### Scene 6.4 -- Closing (19:45 - 20:00)

[ANIMATION] The full architecture diagram from the opening, but now with data particles flowing through every connection. The diagram pulses with activity.

**Voice-over:**
Twelve crates. One point five six million lines. Eight thousand three hundred plus tests. Three AST phases. An immutable DAG. Five semantic verbs. Forty-one event types. Three hundred twenty error codes. Nine LLM providers. Twenty-four media tools. One terminal UI with forty-plus widgets.

This is not a prototype. This is a production workflow engine built with compiler-grade engineering for the AI era. This is Nika.

[TITLE CARD] "github.com/supernovae-st/nika"

---

## Visual Asset Requirements

| Asset | Type | Description |
|-------|------|-------------|
| Crate dependency graph | Animated SVG | 10 nodes, dependency arrows, pulsing |
| AST phase diagram | Three-stage animation | Data flowing through Raw -> Analyzed -> Lowered |
| DAG execution animation | Task graph | Nodes lighting up in topological order |
| CAS flow diagram | Linear animation | File -> hash -> compress -> store |
| Event stream visualization | Scrolling log | NDJSON lines appearing in real-time |
| Error code spectrum | Horizontal bar | Color-coded ranges 000-319 |
| Performance comparison | Bar chart | Nika vs Python frameworks, animated |

## Timing Summary

| Chapter | Duration | Content |
|---------|----------|---------|
| Opening | 1:00 | Architecture animation |
| Workspace | 2:30 | 10 crates, Cargo.toml |
| AST Pipeline | 4:30 | Three phases deep dive |
| DAG | 3:00 | Construction, execution, RunContext |
| Executor | 4:00 | Five verbs, security, extraction |
| Supporting | 3:30 | CAS, events, errors, LSP |
| Design Decisions | 1:30 | YAML, Rust, AGPL |
| **Total** | **~20:00** | |
