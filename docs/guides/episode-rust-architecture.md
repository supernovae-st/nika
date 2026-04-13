# Episode 3: 451K Lines of Rust -- The Architecture That Makes It Work

## Metadata

| Field | Value |
|-------|-------|
| **Series** | Building Nika -- A Rust AI Engine from Scratch |
| **Episode** | 03 |
| **Duration** | ~30 minutes |
| **Topics** | 10-crate workspace, three-phase AST, DAG execution, zero-I/O core, provider abstraction |
| **Guest Suggestions** | A Rust systems architect, a compiler engineer, a Tokio contributor |
| **Audience** | Developers interested in systems design, Rust architecture, compiler-like pipelines |
| **Prerequisites** | Episodes 1-2 or familiarity with Nika's 5 verbs |

---

## Cold Open (30 seconds)

[MUSIC: Technical, precise, building layers]

**Host:** Most AI tools are Python scripts with a nice API. Nika is 451 thousand lines of Rust organized into 12 crates with a three-phase compiler pipeline, a cache-friendly DAG executor using Kahn's topological sort, a zero-I/O core library, and 8,300+ tests.

[PAUSE]

This is not over-engineering. This is what happens when you apply compiler design principles to workflow execution. And today, I am going to show you why every one of those architectural decisions exists, and what they buy you.

[MUSIC FADES]

---

## Intro (1 minute)

**Host:** Welcome to Episode 3 of "Building Nika." If you have been following along, you know what Nika does -- five verbs, YAML workflows, AI orchestration. Today we are talking about how it does it. The Rust architecture underneath.

This episode is for anyone who has ever wondered: what does a production-grade Rust application actually look like at scale? How do you organize 451K lines across 12 crates? How do you make a workflow engine that is both correct and fast?

Let us start with the big picture.

---

## Segment 1: The 12-Crate Workspace Design (8 minutes)

**Host:** Nika is a Cargo workspace with 12 crates. And the split is not arbitrary -- each crate has a specific role, specific dependency rules, and a specific reason for existing.

[CODE EXAMPLE]
```
tools/
  nika/           (2K lines)    CLI binary -- the entry point
  nika-engine/    (134K lines)  Execution engine -- the heart
  nika-core/      (23K lines)   AST, types, catalogs -- zero I/O
  nika-daemon/    (5K lines)    Background daemon -- secrets, jobs
  nika-init/      (21K lines)   Project scaffolding -- init, course
  nika-event/     (4K lines)    EventLog, TraceWriter
  nika-mcp/       (9K lines)    MCP client, connection pool
  nika-media/     (3.5K lines)  CAS store, media processor
  nika-cli/       (8K lines)    CLI subcommands
  nika-tui/       (92K lines)   Terminal UI
  nika-lsp-core/  (9K lines)    LSP intelligence
  nika-lsp/       (2.5K lines)  LSP binary
```

Let me explain why this split exists.

**nika-core: The Zero-I/O Foundation**

[EMPHASIS] This is the most architecturally important crate, and it is the smallest runtime crate. nika-core contains all the type definitions -- the AST types (Raw and Analyzed), the schema types, the provider catalog, the model catalog, the MCP alias catalog, and the binding transform catalog.

The critical property: zero I/O. This crate does not read files. It does not make network calls. It does not access the file system. It is pure data transformation. Why does this matter?

First, it is testable without mocking. Every test in nika-core is a pure function test: given these inputs, produce these outputs. No test fixtures, no temporary directories, no network stubs.

Second, it defines the language boundary. The AST types are the contract between the parser (which reads YAML) and the engine (which executes tasks). By putting these types in a separate crate, you guarantee that the parser and engine agree on what a workflow means.

Third, it enables embedding. If someone wants to build a different frontend for Nika -- say, a GUI editor or a web-based workflow designer -- they can depend on nika-core for the types without pulling in the entire engine.

[CODE EXAMPLE]
```
nika-core/src/
  core/          providers.rs, models.rs, mcp_aliases.rs
  ast/
    raw/         Phase 1 types (everything Optional)
    analyzed/    Phase 2 types (validated, resolved)
    analyzer/    Phase 2 validation logic
    schema.rs    Schema version types
  binding/       Transform catalog (31 operations)
```

**nika-engine: The Heart**

At 134K lines, nika-engine is the largest crate. It contains everything needed to execute a workflow:

- The AST lowering phase (Analyzed to Runtime types)
- The DAG builder and executor
- The runtime (runner, executor, verb dispatch)
- The agent loop (per-provider)
- All 24+ built-in tools
- The binding resolution engine
- The provider abstraction
- The security module
- The init/course/showcase system
- Error types (NikaError with NIKA-XXX codes)

[PAUSE]

This crate is designed to be embeddable. You can depend on nika-engine in your own Rust application, parse a workflow YAML, and execute it programmatically. You do not need the CLI, the TUI, or the LSP. The engine is a library.

**nika-tui: The Developer Experience Layer**

92K lines for a terminal UI. Let that sink in.

The TUI is built on ratatui and provides three views:

- **Studio (1/s)** -- The main workflow editing view. DAG visualization, task details, binding inspection.
- **Command (2/c)** -- Command palette for running operations.
- **Control (3/x)** -- System status, provider health, event stream.

It has 40+ widgets, real-time streaming of agent thoughts, and a DAG renderer that shows task execution progress in real time. This is 44% of the total source files in the project -- a deliberate investment in developer experience.

**nika-mcp: The Protocol Layer**

9K lines for MCP client functionality:
- Connection management with pooling
- Retry with exponential backoff and jitter
- Automatic reconnection on disconnect
- Response caching with TTL
- Tool schema validation and caching
- rmcp v0.16 adapter

**nika-event: The Observability Layer**

4K lines for 41 event types and NDJSON trace writing. Every significant action in a workflow run produces an event: TaskStarted, TaskCompleted, TaskFailed, InferRequest, InferResponse, McpToolCall, GuardrailPassed, StructuredOutputAttempt, and 33 more. These events are written as NDJSON (newline-delimited JSON) for easy processing with standard tools.

**nika-media: The Content Pipeline**

3.5K lines for content-addressable storage. Files are stored by their blake3 hash with a sharded directory layout. Writes are atomic via O_EXCL. Deduplication is automatic. Files over 1MB get read-back verification. A per-run MediaBudget caps storage at 500MB with lock-free atomic tracking.

---

## Segment 2: The Three-Phase AST (8 minutes)

**Host:** This is the design decision that separates Nika from every other YAML-based workflow tool I have seen. Nika processes workflow files through a three-phase abstract syntax tree pipeline, just like a real compiler.

[EMPHASIS] Most tools parse YAML into a struct, maybe validate a few fields, and start executing. Nika does something fundamentally different.

**Phase 1: Raw AST**

The YAML is parsed into raw types where everything is `Option`. Every field is optional. Source spans are preserved for error reporting. This phase uses the `marked_yaml` crate for span tracking.

[CODE EXAMPLE]
```rust
// Phase 1: Everything is Option, preserving source spans
pub struct RawTask {
    pub id: Option<Spanned<String>>,
    pub infer: Option<RawInfer>,
    pub exec: Option<RawExec>,
    pub fetch: Option<RawFetch>,
    pub invoke: Option<RawInvoke>,
    pub agent: Option<RawAgent>,
    pub depends_on: Option<Vec<Spanned<String>>>,
    pub with: Option<HashMap<String, RawWithEntry>>,
    // ... everything Optional
}
```

The philosophy: accept anything that looks like a workflow. Defer validation to Phase 2.

**Phase 2: Analyzed AST**

This is where the magic happens. The analyzer takes the raw AST and produces a validated, resolved representation:

- Task IDs are interned into `TaskId` -- a newtype over `u32` -- for O(1) comparison and zero-copy sharing
- Dependencies are resolved from string names to `TaskId` references
- Provider configurations are validated against the catalog
- Binding paths are parsed and verified
- Circular dependencies are detected
- Unused tasks are flagged
- Schema version compatibility is checked

[CODE EXAMPLE]
```rust
// Phase 2: Validated and resolved, no more Option
pub struct AnalyzedTask {
    pub id: TaskId,              // Interned, O(1) comparison
    pub verb: AnalyzedVerb,      // Exactly one verb, validated
    pub depends_on: Vec<TaskId>, // Resolved references
    pub with: WithBlock,         // Parsed binding paths
    // ... all required fields present
}
```

[EMPHASIS] The key insight is that a Phase 2 `AnalyzedTask` is impossible to construct with invalid data. There is no `Option<TaskId>` -- it has an ID or it did not pass analysis. There is no ambiguity about which verb a task uses -- it is an enum with exactly one variant. The type system makes invalid states unrepresentable.

And when analysis fails, you get precise error messages with source locations:

```
NIKA-020: Circular dependency detected
  --> workflow.nika.yaml:15:5
   |
15 |   depends_on: [task_b]
   |                ^^^^^^ task_a depends on task_b
   |
  --> workflow.nika.yaml:22:5
   |
22 |   depends_on: [task_a]
   |                ^^^^^^ task_b depends on task_a
```

**Phase 3: Lowered (Runtime Types)**

The analyzed AST is converted into types optimized for execution. This is the `lower.rs` module. The lowered representation strips away information needed for analysis but not for execution, and adds information needed for execution but not for analysis (like pre-computed provider client handles).

[PAUSE]

**Host:** Why three phases? Why not just validate-and-execute?

The answer is error quality and correctness guarantees.

In a single-pass system, you discover errors as you execute. The first task might succeed, the second might fail with a provider error, and the third might fail with a dependency error. You have wasted time and API tokens running tasks that were doomed from the start.

In Nika's three-phase system, ALL validation happens before ANY execution. If your workflow has a problem -- a typo in a task ID, a circular dependency, an invalid provider name, a malformed binding path -- you learn about it immediately, before a single API call is made, before a single token is burned.

This is the same principle that makes compiled languages catch bugs before runtime. Nika is essentially a compiler for AI workflows.

---

## Segment 3: DAG Execution -- Automatic Parallelism (8 minutes)

**Host:** Once the workflow passes all three phases, it is time to execute. And this is where the DAG comes in.

DAG stands for Directed Acyclic Graph. In Nika, tasks are nodes and `depends_on:` relationships are edges. The engine builds this graph, computes a topological order, and executes tasks in parallel wherever the graph allows it.

**The IndexedDag**

Nika uses a compact, cache-friendly DAG representation. Let me read you the module documentation:

[CODE EXAMPLE]
```rust
//! Vec-indexed DAG with Kahn's algorithm.
//!
//! A compact, cache-friendly DAG representation using
//! Vec<SmallVec<[TaskId; 4]>> adjacency lists indexed by TaskId.
//! Replaces HashMap-based DAG for runtime execution with O(1) lookups,
//! zero hashing, and pre-computed topological order.
```

The key design choices:

**DepVec = SmallVec<[TaskId; 4]>** -- 95% of workflow tasks have four or fewer dependencies. SmallVec stores up to four elements on the stack, avoiding heap allocation in the common case. This is a micro-optimization that adds up when you have hundreds of tasks.

**Vec-indexed, not HashMap-indexed** -- Task IDs are interned as sequential u32 values, so they can directly index into a Vec. This gives O(1) lookup with zero hashing overhead and excellent cache locality.

**Kahn's Algorithm** -- The topological sort uses Kahn's algorithm (BFS-based) rather than DFS-based topological sort. Kahn's algorithm has a useful property: it simultaneously detects cycles and computes depth information. Tasks at depth 0 have no dependencies and can run immediately. Tasks at depth 1 depend only on depth-0 tasks. And so on.

[PAUSE]

**The Executor**

Once topological order is computed, the executor runs tasks using Tokio:

1. Start all depth-0 tasks concurrently using `JoinSet`
2. As each task completes, check if any depth-1+ tasks now have all dependencies satisfied
3. Start newly ready tasks
4. Repeat until all tasks are done or a failure occurs

`fail_fast:` mode uses `tokio::select!` to cancel in-flight tasks when any task fails. This is important for cost control -- if your analysis task fails, you do not want to keep burning tokens on the report generation task.

When a task fails, its dependents are marked as `DependencyFailed`, and their dependents as `DependencyChainFailed`. This cascading failure propagation distinguishes between "this task failed because of a bug in its own logic" and "this task failed because something upstream failed."

[CODE EXAMPLE]
```yaml
# This workflow has natural parallelism
tasks:
  - id: scrape_a           # Depth 0: runs immediately
    fetch: { url: "..." }

  - id: scrape_b           # Depth 0: runs in parallel with scrape_a
    fetch: { url: "..." }

  - id: process            # Depth 0: runs in parallel with both
    exec: "process_data"

  - id: analyze             # Depth 1: waits for scrape_a + scrape_b
    depends_on: [scrape_a, scrape_b]
    infer: "Analyze {{with.a}} and {{with.b}}"

  - id: report              # Depth 2: waits for analyze + process
    depends_on: [analyze, process]
    infer: "Write report from {{with.analysis}} and {{with.processed}}"
```

In this workflow, tasks scrape_a, scrape_b, and process all run concurrently. Then analyze runs. Then report runs. Nika figures this out automatically from the dependency graph -- you never manually manage concurrency.

**for_each with Concurrency Control**

For fan-out patterns, Nika supports `for_each:` with configurable concurrency:

[CODE EXAMPLE]
```yaml
- id: process_all
  for_each: "{{with.urls}}"
  concurrency: 5
  fetch:
    url: "{{with.item}}"
    extract: markdown
```

This processes up to 5 URLs concurrently, queuing the rest. The concurrency limit prevents overwhelming APIs with too many simultaneous requests.

---

## Segment 4: The Provider Abstraction (4 minutes)

**Host:** Nika supports 9 LLM providers through a layered abstraction.

The foundation is rig-core -- a Rust LLM framework that provides a unified interface for cloud providers. On top of that, Nika adds:

- Auto-detection: checking environment variables in priority order (ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, GEMINI_API_KEY)
- Native inference via mistral.rs for local GGUF models with Metal/CUDA acceleration
- Provider-specific features like Claude's extended thinking
- Streaming support across all providers
- Cost tracking per task

[CODE EXAMPLE]
```rust
// Provider resolution: environment-based auto-detection
let provider = RigProvider::auto()?;

// Or explicit construction
let provider = RigProvider::claude()?;
let provider = RigProvider::openai()?;
let provider = RigProvider::mistral()?;
let provider = RigProvider::groq()?;
let provider = RigProvider::deepseek()?;
let provider = RigProvider::gemini()?;
```

[EMPHASIS] The provider abstraction means your workflow YAML is provider-agnostic by default. If you write `infer: "Do something"`, Nika uses whatever API key it finds. If you want to be explicit, you specify `provider: anthropic`. If you want to switch providers, you change one line in your YAML -- not your entire codebase.

For native (local) inference, Nika uses mistral.rs with support for:
- Text models via GGUF format with Metal and CUDA acceleration
- Vision models via HuggingFace safetensors with Integer-Scaled Quantization (ISQ)
- Streaming via spawn_stream_task with mpsc channels
- Models as small as Gemma 3 4B running in about 3 GB of VRAM

---

## Wrap-up & Preview (2 minutes)

**Host:** Let me summarize the architecture.

Nika is 10 Rust crates with clear responsibilities. nika-core provides zero-I/O types that define the workflow language. nika-engine provides the execution runtime with a three-phase compiler pipeline. nika-mcp handles protocol communication. nika-tui provides 92K lines of developer experience. And the remaining crates handle events, media, CLI, and LSP.

The three-phase AST -- Raw, Analyzed, Lowered -- catches all errors before execution starts, using Rust's type system to make invalid states unrepresentable.

The IndexedDag with Kahn's algorithm provides automatic parallelism with cache-friendly data structures and zero-overhead dependency resolution.

And the provider abstraction makes workflows portable across 9 LLM providers.

[PAUSE]

Next episode, we are diving into the media pipeline. 24 tools, content-addressable storage with blake3 hashing, SIMD-accelerated image processing, C2PA content provenance for EU AI Act compliance, and the three-tier feature system that keeps your binary small. It is Episode 4: "24 Media Tools, One Pipeline."

[MUSIC: Outro theme]

---

## Show Notes

### Architecture Concepts
- **Zero-I/O Core** -- nika-core has no file system or network access
- **Three-Phase AST** -- Raw (permissive), Analyzed (validated), Lowered (optimized)
- **IndexedDag** -- Vec-based adjacency lists with SmallVec<[TaskId; 4]>
- **Kahn's Algorithm** -- BFS topological sort with cycle detection + depth computation
- **TaskId Interning** -- u32 newtype for O(1) comparison
- **Fail-Fast** -- tokio::select! cancellation of in-flight tasks

### Crate Sizes
| Crate | Lines | Role |
|-------|------:|------|
| nika-engine | 134K | Execution engine |
| nika-tui | 92K | Terminal UI |
| nika-init | 21K | Project scaffolding |
| nika-core | 23K | AST types (zero I/O) |
| nika-mcp | 9K | MCP client |
| nika-lsp-core | 9K | LSP intelligence |
| nika-cli | 8K | CLI subcommands |
| nika-daemon | 5K | Background daemon |
| nika-event | 4K | Event system |
| nika-media | 3.5K | CAS store |
| nika-lsp | 2.5K | LSP binary |
| nika | 2K | CLI entry point |

### Key Dependencies
- **tokio** -- Async runtime
- **rig-core** v0.32 -- LLM provider abstraction
- **rmcp** v0.16 -- MCP protocol
- **ratatui** -- Terminal UI framework
- **smallvec** -- Stack-allocated small vectors
- **rustc_hash** -- Fast hashing (FxHashMap/FxHashSet)
- **blake3** -- Content-addressable hashing
- **marked_yaml** -- YAML parsing with source spans
- **insta** -- Snapshot testing
- **dashmap** -- Concurrent HashMap (RunContext)
