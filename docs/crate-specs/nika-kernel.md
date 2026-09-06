# Crate spec — `nika-kernel`

| | |
|---|---|
| Status | **ADMITTED 2026-04-13** (`ef8804371`) · facade + range-registry hub post 4-way split 2026-06-10 |
| Layer | L0.5 (TRAITS ONLY, zero I/O, zero impl, minimal deps) |
| Design | **ISP-layered** — ~20 atomic traits + ~6 super-traits + the Connectome/agent-v2 hooks |
| LOC budget | ≤3,500 src (target ~3,000, alarm at 3,500) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `main` (reference) | `tools/nika-kernel/src/` (15 files, ~2,741 LOC) |
| Crate version | tracks workspace (bumped to `0.90.0-alpha.1` at Phase 1 close) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-kernel` defines the **trait contracts for every side effect** in the
Nika diamond. It sits at L0.5 — above L0 pure types (nika-error, nika-catalog)
and below L1 effect implementations (nika-fs, nika-http, nika-exec-runner, etc.).

**Zero implementations live here.** This crate defines contracts only.
Implementations live in their respective crates; test mocks live in
`nika-kernel-mock` (L0.5 companion).

The design follows Interface Segregation Principle (ISP): ~20 fine-grained
atomic traits grouped into ~6 super-traits of convenience. Consumers depend
on the minimum trait surface they need (e.g., a context loader depends on
`FsRead` alone, not the full `Fs` umbrella). All async traits use
`trait_variant` (decision A1) for zero-overhead object-safe companions
via `#[trait_variant::make(XxxDyn: Send)]`.

This crate also carries the **Connectome + agent-v2 kernel hooks** (decision D3
from POST_AUDIT_REVISIONS): memory traits, tool executor, agent config types,
and checkpoint types. These hooks land in Phase 1 to avoid breaking-change
cascades on `#[non_exhaustive]` structs with `::new()` constructors
(invariant #19, ROI 6.7x per POST_AUDIT decision 3).

Decision D4 (ISP = Interface Segregation Principle) splits each effect into
narrow sub-traits (e.g. FsRead + FsWrite + FsMeta + FsList → Fs) so consumers
only depend on the capability they actually use. Memory + agent-v2 hooks
reserved on kernel traits for the 2.0 Connectome work.

---

## 2. Public API surface

### 2.1 Effect traits (ISP-layered)

The filesystem signatures below use `nika_kernel::fs::FsError`. The
provided `write_new` method and backend/wrapper migration are defined in
[the kernel-core migration contract](nika-kernel-core.md#4-filesystem-backend-migration).
Its default body is omitted from this API summary.

```rust
// ── clock.rs (~40 LOC) ───────────────────────────────────────
#[trait_variant::make(ClockDyn: Send)]
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    async fn sleep(&self, duration: Duration);
    fn elapsed(&self, since: Instant) -> Duration { self.now().duration_since(since) }
}

// ── fs.rs (~120 LOC) ─────────────────────────────────────────
#[trait_variant::make(FsReadDyn: Send)]
pub trait FsRead: Send + Sync {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError>;
    async fn read_to_string(&self, path: &Path) -> Result<String, FsError>;
    async fn exists(&self, path: &Path) -> bool;
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError>;
}

#[trait_variant::make(FsWriteDyn: Send)]
pub trait FsWrite: Send + Sync {
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError>;
    // Provided method; default body omitted.
    fn write_new(
        &self,
        path: &Path,
        contents: &[u8],
    ) -> impl std::future::Future<Output = Result<(), FsError>> + Send;
    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError>;
    async fn remove_file(&self, path: &Path) -> Result<(), FsError>;
}

#[trait_variant::make(FsMetaDyn: Send)]
pub trait FsMeta: Send + Sync {
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError>;
}

#[trait_variant::make(FsListDyn: Send)]
pub trait FsList: Send + Sync {
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError>;
    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError>;
}

pub trait Fs: FsRead + FsWrite + FsMeta + FsList {}
impl<T: FsRead + FsWrite + FsMeta + FsList> Fs for T {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileMetadata { pub len: u64, pub is_file: bool, pub is_dir: bool }

// ── http.rs (~150 LOC) ───────────────────────────────────────
#[trait_variant::make(HttpGetDyn: Send)]
pub trait HttpGet: Send + Sync {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;
}

#[trait_variant::make(HttpPostDyn: Send)]
pub trait HttpPost: Send + Sync {
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;
    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError>;
}

pub trait HttpClient: HttpGet + HttpPost {}
impl<T: HttpGet + HttpPost> HttpClient for T {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod { Get, Post, Put, Patch, Delete, Head, Options }

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Bytes>,
    pub timeout: Option<Duration>,
    pub follow_redirects: bool,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Bytes,
    pub final_url: String,
}

// HttpStreamResponse (status, headers, final_url, content_length, body: Pin<Box<Stream>>)
// HttpError enum (Timeout, Connection, SsrfBlocked, TooLarge, Unsupported, Other)

// ── process.rs (~120 LOC) ────────────────────────────────────
#[trait_variant::make(ShellRunDyn: Send)]
pub trait ShellRun: Send + Sync {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError>;
}

#[trait_variant::make(ShellCancelDyn: Send)]
pub trait ShellCancel: Send + Sync {
    async fn cancel(&self, id: &str) -> Result<(), ShellError>;
}

pub trait ShellExecutor: ShellRun + ShellCancel {}
impl<T: ShellRun + ShellCancel> ShellExecutor for T {}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub env_remove: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Option<Duration>,
    pub stdin: Option<String>,
    pub shell: bool,
    pub pre_validated: bool,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShellResult { pub status: i32, pub stdout: String, pub stderr: String, pub duration: Duration }

// ShellError enum (NotFound, Timeout, Cancelled, Blocked, Other)

// ── blob.rs (~80 LOC) ────────────────────────────────────────
#[trait_variant::make(BlobStoreDyn: Send)]
pub trait BlobStore: Send + Sync {
    async fn put(&self, data: Bytes, mime_type: &str) -> Result<BlobMetadata, BlobError>;
    async fn get(&self, hash: &str) -> Result<Bytes, BlobError>;
    async fn exists(&self, hash: &str) -> bool;
    async fn stat(&self, hash: &str) -> Result<BlobMetadata, BlobError>;
    async fn delete(&self, hash: &str) -> Result<(), BlobError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlobMetadata { pub hash: String, pub mime_type: String, pub size: u64 }

// BlobError enum (NotFound, Io, TooLarge)
```

### 2.2 Provider traits + DTOs

```rust
// ── provider.rs (~350 LOC) ───────────────────────────────────

// --- Traits ---
#[trait_variant::make(ProviderInferDyn: Send)]
pub trait ProviderInfer: Send + Sync {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError>;
}

#[trait_variant::make(ProviderStreamDyn: Send)]
pub trait ProviderStream: Send + Sync {
    async fn infer_stream(&self, request: InferRequest) -> Result<InferEventStream, ProviderError>;
}

pub trait Provider: ProviderInfer + ProviderStream {
    fn name(&self) -> &str;
    fn supports_response_format(&self) -> bool { false }
}

// --- DTOs (all #[non_exhaustive] + ::new()) ---
pub struct Message { pub role: Role, pub content: Vec<ContentBlock> }

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role { System, User, Assistant, Tool }

#[derive(Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ContentBlock {
    Text { text: String },
    Image { source: String, detail: Option<String> },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    Thinking { text: String },
}

pub struct ToolDef { pub name: String, pub description: String, pub parameters: serde_json::Value }

#[derive(Default)]
pub enum ToolChoice { #[default] Auto, Required, None, Specific(String) }

#[derive(Default)]
pub enum ResponseFormat { #[default] Text, Json, JsonSchema(serde_json::Value) }

pub struct ProviderExtras { pub params: serde_json::Map<String, serde_json::Value> }

#[non_exhaustive]
pub struct InferRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDef>,
    pub tool_choice: ToolChoice,
    pub response_format: ResponseFormat,
    pub stop_sequences: Vec<String>,
    pub thinking_budget: Option<u32>,
    pub extra: ProviderExtras,
    pub memory: Option<MemoryDirective>,   // the Connectome hook (Phase 1)
}

#[non_exhaustive]
pub struct InferResponse {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    pub ttft_ms: Option<u64>,
    pub cached_tokens: Option<u32>,
    pub request_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub finish_reason_raw: Option<String>,
    pub memory_frames: Vec<MemoryFrameRef>, // the Connectome hook (Phase 1)
}

pub struct TokenUsage { pub input_tokens: u64, pub output_tokens: u64, pub cache_read_tokens: Option<u64>, pub cache_write_tokens: Option<u64> }

pub enum StopReason { EndTurn, MaxTokens, StopSequence, ToolUse, ContentFilter, Unknown(String) }

#[non_exhaustive]
pub enum InferEvent {
    Delta(String),
    ToolUseStart { id: String, name: String },
    ToolUseDelta { id: String, partial_json: String },
    Thinking(String),
    Usage(TokenUsage),
    Done { stop_reason: StopReason, request_id: Option<String>, finish_reason_raw: Option<String> },
}

pub type InferEventStream = Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>>;

// ProviderError enum (Api, ModelNotFound, RateLimited, AuthFailed, Other)
// impl NikaErrorCode for ProviderError (codes NIKA 380-429)
```

### 2.3 Memory traits + types (the Connectome hook)

```rust
// ── memory.rs (~200 LOC) ─────────────────────────────────────

// --- Atomic traits (ISP) ---
#[trait_variant::make(MemoryRememberDyn: Send)]
pub trait MemoryRemember: Send + Sync {
    async fn remember(&self, frame: MemoryFrame) -> Result<MemoryId, MemoryError>;
}

#[trait_variant::make(MemoryRecallDyn: Send)]
pub trait MemoryRecall: Send + Sync {
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;
}

#[trait_variant::make(MemoryForgetDyn: Send)]
pub trait MemoryForget: Send + Sync {
    async fn forget(&self, id: MemoryId) -> Result<(), MemoryError>;
}

pub trait MemoryStore: MemoryRemember + MemoryRecall + MemoryForget {}
impl<T: MemoryRemember + MemoryRecall + MemoryForget> MemoryStore for T {}

#[trait_variant::make(EmbeddingProviderDyn: Send)]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;
    fn dimension(&self) -> usize;
}

// --- Types (all #[non_exhaustive] + ::new()) ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoryId(u128);           // Display as "mem-{hex}"

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MemoryLevel { Working, Episodic, Semantic, Procedural, Reflective, Conceptual }

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MemoryFrame {
    pub content: String,
    pub level: MemoryLevel,
    pub tags: Vec<String>,
    pub metadata: BTreeMap<String, String>,
    pub source: Option<String>,
    pub observed_at: Option<u64>,     // unix epoch millis
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RecallQuery {
    pub text: String,
    pub levels: Option<Vec<MemoryLevel>>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<usize>,
    pub min_score: Option<f32>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MemoryHit {
    pub id: MemoryId,
    pub content: String,
    pub level: MemoryLevel,
    pub score: f32,
    pub tags: Vec<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MemoryDirective {
    #[default]
    Auto,
    RecallOnly,
    RememberOnly,
    Explicit { recall: Vec<String>, remember: Vec<String> },
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MemoryFrameRef {
    pub id: MemoryId,
    pub level: MemoryLevel,
    pub summary: String,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MemoryError {
    Unavailable { reason: String },
    NotFound { id: MemoryId },
    EmbeddingFailed { reason: String },
    Storage { reason: String },
}
// impl NikaErrorCode for MemoryError (codes NIKA 600-649)
```

### 2.4 Tool executor traits + types (agent-v2 hook)

```rust
// ── tool_executor.rs (~100 LOC) ──────────────────────────────

// --- Atomic traits (ISP) ---
#[trait_variant::make(ToolExecuteDyn: Send)]
pub trait ToolExecute: Send + Sync {
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError>;
}

#[trait_variant::make(ToolBatchDyn: Send)]
pub trait ToolBatch: ToolExecute {
    async fn execute_batch(&self, calls: Vec<ToolCall>) -> Vec<Result<ToolResult, ToolExecError>> {
        // Default: sequential. Agent-v2 overrides for parallel.
        let mut out = Vec::with_capacity(calls.len());
        for call in calls { out.push(self.execute(call).await); }
        out
    }
}

pub trait ToolExecutor: ToolExecute + ToolBatch {}
impl<T: ToolExecute + ToolBatch> ToolExecutor for T {}

// --- Types ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolCallId(pub String);

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolCall {
    pub id: ToolCallId,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolResult {
    pub tool_use_id: ToolCallId,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolExecError {
    NotFound { name: String },
    Timeout { name: String, duration_ms: u64 },
    ExecutionFailed { name: String, reason: String },
    NotAvailable { reason: String },
}
// impl NikaErrorCode for ToolExecError (codes NIKA 230-279 range, shared with MCP/tools)
```

### 2.5 Agent config types (agent-v2 hook)

```rust
// ── agent.rs (~120 LOC) ──────────────────────────────────────

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AgentLoopConfig {
    pub max_turns: u32,
    pub planning: PlanningStrategy,
    pub parallel_tools: bool,
    pub reflection: Option<ReflectionConfig>,
    pub compression: Option<CompressionPolicy>,
    pub tool_error_policy: ToolErrorPolicy,
    pub session_id: Option<String>,
    pub inject_records: Vec<ToolCallRecord>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AgentOutcome {
    pub output: String,
    pub stop_reason: AgentStopReason,
    pub turns: u32,
    pub total_tokens: u64,
    pub cost_usd: Option<f64>,
    pub checkpoint: Option<AgentCheckpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AgentStopReason {
    Completed,
    ExplicitCompletion,
    TurnsLimit,
    TokensLimit,
    CostLimit,
    DurationLimit,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PlanningStrategy {
    #[default]
    React,
    ReWoo,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolErrorPolicy {
    #[default]
    ReportToLlm,
    RetryTransient { max: u32 },
    FailFast,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ReflectionConfig {
    pub enabled: bool,
    pub threshold: f32,
    pub max_retries: u32,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompressionPolicy {
    pub token_threshold: u64,
    pub preserve_recent: u32,
    pub compress_tool_results: bool,
}
```

### 2.6 Checkpoint types (agent-v2 hook)

```rust
// ── checkpoint.rs (~100 LOC) ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AgentCheckpoint {
    pub version: u32,
    pub session_id: String,
    pub task_id: Option<TaskId>,
    pub messages: Vec<CheckpointMessage>,
    pub tool_records: Vec<ToolCallRecord>,
    pub turn_count: u32,
    pub total_tokens: u64,
    pub cost_usd: Option<f64>,
    pub provider: String,
    pub model: String,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CheckpointMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCallRecord {
    pub name: String,
    pub input: serde_json::Value,
    pub output: String,
    pub is_error: bool,
    pub duration_ms: u64,
}
```

### 2.7 Context compressor trait (agent-v2 hook)

```rust
// ── context.rs (~40 LOC) ─────────────────────────────────────

#[trait_variant::make(ContextCompressorDyn: Send)]
pub trait ContextCompressor: Send + Sync {
    async fn compress(
        &self,
        messages: &[CheckpointMessage],
        policy: &CompressionPolicy,
    ) -> Option<CompressedContext>;
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompressedContext {
    pub summary: String,
    pub messages_compressed: u32,
    pub tokens_before: u64,
    pub tokens_after: u64,
}
```

### 2.8 Shared cross-boundary types

```rust
// ── types.rs (~40 LOC) ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WorkflowMeta {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
}
```

---

## 3. Key design decisions

### 3.1 trait_variant over async_trait (decision A1)

Rust 1.91 + edition 2024 has native async-fn-in-trait (AFIT). The legacy
kernel uses `#[async_trait]` which boxes every future. Diamond uses
`trait_variant` (Rust async WG, Tyler Mandry) to generate object-safe
companion traits (`XxxDyn: Send`) without boxing on the static dispatch
path. Zero overhead when the concrete type is known at compile time.

### 3.2 ISP atomic + super-trait blanket (decision D4)

Every trait is granular. Super-traits (`Fs`, `HttpClient`, `ShellExecutor`,
`MemoryStore`, `ToolExecutor`) are blanket-implemented for any type
satisfying all atomics. Consumers bind the minimum surface. Implementors
get the super-trait for free.

### 3.3 No DI framework

Rust's type system IS dependency injection. The runtime constructs concrete
types and threads them through generics or trait objects:
```rust
pub struct Runtime<P: Provider, M: MemoryStore, S: ShellExecutor> { ... }
```
No shaku, no inject, no service locator.

### 3.4 Caps structs live in nika-kernel but expand in Phase 3+

The per-verb `XxxCaps<'a>` structs (ExecCaps, FetchCaps, InferCaps,
InvokeCaps, AgentCaps) from legacy are preserved in concept but their
exact shape depends on the verb crates admitted in Phase 3. Phase 1
includes only the trait definitions and shared types; the Caps structs
are deferred to Phase 3 when verb crates land and their actual capability
requirements are known.

### 3.5 Error types per subsystem

Each subsystem (provider, http, shell, blob, memory, tool_executor) defines
its own error enum with `#[non_exhaustive]` and `impl NikaErrorCode`.
These are scoped errors, NOT the cross-cutting `CoreError` from nika-error.
The blanket `From<E> for NikaError` gives free conversion at call sites.

### 3.6 Scope/task_local/policy/mcp/builtin/events/binding NOT in diamond kernel

The legacy kernel carries modules that belong elsewhere in diamond:

| Legacy module | Diamond destination | Rationale |
|---|---|---|
| `scope.rs` (395 LOC) | nika-runtime L3 | Runtime-specific scope/context, not a kernel contract |
| `task_local.rs` (250 LOC) | nika-runtime L3 | tokio task-local state, runtime concern |
| `caps.rs` (270 LOC) | nika-kernel Phase 3 | Deferred until verb crates land |
| `policy.rs` (93 LOC) | nika-runtime L3 module | Single consumer |
| `mcp.rs` (426 LOC) | nika-mcp L2 | MCP pool is a domain crate |
| `builtin.rs` (405 LOC) | nika-builtin L2 | Builtin router is a domain crate |
| `events.rs` (15 LOC) | nika-event L1 | Event types live with event system |
| `binding.rs` (12 LOC) | nika-binding L0 | Binding traits live with binding crate |

This reduces nika-kernel from legacy ~2,741 LOC of mixed contracts+runtime
to ~3,000 LOC of pure trait definitions + hook types, with the increase
coming from the memory/agent-v2 hooks (~500 LOC) that did not exist in legacy.

---

## 4. File layout

```
crates/nika-kernel/
  Cargo.toml
  src/
    lib.rs              (~60 LOC — pub mod + re-exports)
    clock.rs            (~40 LOC — Clock trait)
    fs.rs               (~120 LOC — FsRead, FsWrite, FsMeta, FsList, Fs + FileMetadata)
    http.rs             (~200 LOC — HttpGet, HttpPost, HttpClient + HttpRequest, HttpResponse,
                                    HttpStreamResponse, HttpMethod, HttpError)
    process.rs          (~150 LOC — ShellRun, ShellCancel, ShellExecutor +
                                    ShellCommand, ShellResult, ShellError)
    blob.rs             (~80 LOC — BlobStore + BlobMetadata, BlobError)
    provider.rs         (~400 LOC — ProviderInfer, ProviderStream, Provider +
                                    InferRequest, InferResponse, Message, ContentBlock,
                                    Role, ToolDef, ToolChoice, ResponseFormat,
                                    ProviderExtras, TokenUsage, StopReason,
                                    InferEvent, InferEventStream, ProviderError)
    memory.rs           (~200 LOC — MemoryRemember, MemoryRecall, MemoryForget, MemoryStore,
                                    EmbeddingProvider + MemoryId, MemoryLevel, MemoryFrame,
                                    RecallQuery, MemoryHit, MemoryDirective, MemoryFrameRef,
                                    MemoryError)
    tool_executor.rs    (~100 LOC — ToolExecute, ToolBatch, ToolExecutor +
                                    ToolCallId, ToolCall, ToolResult, ToolExecError)
    agent.rs            (~120 LOC — AgentLoopConfig, AgentOutcome, AgentStopReason,
                                    PlanningStrategy, ToolErrorPolicy, ReflectionConfig,
                                    CompressionPolicy)
    checkpoint.rs       (~100 LOC — AgentCheckpoint, CheckpointMessage, ToolCallRecord)
    context.rs          (~40 LOC — ContextCompressor, CompressedContext)
    types.rs            (~40 LOC — TaskId, WorkflowMeta)
    errors.rs           (~150 LOC — NikaErrorCode impls for ProviderError, HttpError,
                                    ShellError, BlobError, MemoryError, ToolExecError)
```

Target: ~1,800 LOC src, ~1,200 LOC tests = ~3,000 LOC total.

---

## 5. Dependencies

```toml
[dependencies]
nika-error     = { path = "../nika-error" }
trait-variant  = { workspace = true }
thiserror      = { workspace = true }
serde          = { workspace = true, features = ["derive"] }
serde_json     = { workspace = true }
bytes          = { workspace = true }
futures-core   = { workspace = true }

[features]
default = ["serde"]
serde = ["dep:serde", "serde_json"]

[dev-dependencies]
insta      = { workspace = true }
proptest   = { workspace = true }
rstest     = { workspace = true }
serde_json = { workspace = true }
```

### Dependency rationale

| Dep | Why |
|---|---|
| `nika-error` | NikaErrorCode trait, NikaCode consts for per-subsystem error impls |
| `trait-variant` | `#[trait_variant::make(XxxDyn: Send)]` for object-safe async companions (A1) |
| `thiserror` | Per-subsystem error enum derives |
| `serde` + `serde_json` | Serialize/Deserialize on DTOs (Message, ContentBlock, checkpoint types, agent config). serde_json for `Value` in ToolDef.parameters, InferRequest.extra, ToolCall.input |
| `bytes` | `Bytes` in FsRead::read, BlobStore::put/get, HttpResponse::body |
| `futures-core` | `Stream` trait for `InferEventStream` type alias |

### What is NOT a dependency

| Crate | Why excluded |
|---|---|
| `tokio` | L0.5 kernel has zero tokio dep. Traits use native AFIT via trait_variant. No task-local, no runtime features. |
| `tokio-util` | Legacy used CancellationToken. Diamond passes cancel tokens as concrete params in Caps structs (Phase 3), not in kernel traits. |
| `async-trait` | Replaced by trait_variant (A1). |
| `nika-core` | Legacy kernel depended on nika-core for `ModelCapabilities` re-export. Diamond kernel depends only on nika-error. Model capabilities live in nika-catalog (L0), which is a sibling, not a dependency. |

---

## 6. Test plan

| Test location | Scope | Count |
|---|---|---|
| `src/clock.rs` #[cfg(test)] | Clock trait default `elapsed()` impl | ~2 |
| `src/fs.rs` #[cfg(test)] | Fs super-trait blanket, FileMetadata construction | ~4 |
| `src/http.rs` #[cfg(test)] | HttpClient blanket, HttpRequest convenience ctors, HttpError Display | ~6 |
| `src/process.rs` #[cfg(test)] | ShellCommand::new defaults, ShellResult::success, ShellError Display | ~5 |
| `src/blob.rs` #[cfg(test)] | BlobMetadata construction, BlobError Display | ~3 |
| `src/provider.rs` #[cfg(test)] | InferRequest::new defaults (memory=None), InferResponse::new (memory_frames=empty), ToolChoice default, StopReason PartialEq, InferEvent exhaustive match | ~10 |
| `src/memory.rs` #[cfg(test)] | MemoryId Display "mem-{hex}", MemoryLevel roundtrip, MemoryDirective Default=Auto, MemoryFrame::new, RecallQuery::new | ~8 |
| `src/tool_executor.rs` #[cfg(test)] | ToolCallId eq/hash, ToolCall::new, ToolResult::new, ToolExecError Display | ~5 |
| `src/agent.rs` #[cfg(test)] | AgentLoopConfig::new defaults, PlanningStrategy default=React, AgentStopReason serde roundtrip, CompressionPolicy defaults | ~6 |
| `src/checkpoint.rs` #[cfg(test)] | AgentCheckpoint::new, serde roundtrip, ToolCallRecord::new | ~4 |
| `src/context.rs` #[cfg(test)] | CompressedContext::new | ~2 |
| `src/types.rs` #[cfg(test)] | TaskId Display, WorkflowMeta::new | ~3 |
| `src/errors.rs` #[cfg(test)] | NikaErrorCode impl for each error enum, code ranges, is_transient | ~8 |
| proptest in provider.rs | Role roundtrip, ContentBlock tag stability | ~2 |
| proptest in memory.rs | MemoryId Display/parse roundtrip, MemoryLevel exhaustive | ~2 |
| **Trait object-safety** | assert `dyn XxxDyn` compiles for every trait_variant companion | ~14 |
| **Send+Sync** | `assert_send::<T>()` / `assert_sync::<T>()` for all DTOs | ~10 |
| **Total** | | **~94** |

All unit tests inline (`#[cfg(test)] mod tests`), run with `cargo test --lib`.

### Trait compile-time verification pattern

Traits cannot be unit-tested directly (no impl in this crate). Instead:

1. **Object safety**: `fn _assert_object_safe(_: &dyn ClockDyn) {}` compiles.
2. **Send+Sync bounds**: `fn assert_send<T: Send>() {}` for every public struct.
3. **Blanket impls**: Dummy struct implementing all atomics, assert super-trait satisfied.
4. **Real impl testing**: Happens in nika-kernel-mock (companion crate, same admission).

---

## 7. Gate exemptions

- **Gate 7 (Benchmarks)**: Exempt. L0.5 trait definitions, no executable code,
  no hot path. All methods are unimplemented in this crate.
- **Gate 9 (Canary E2E)**: Exempt. No runtime yet. Canary lands Phase 3
  when verb crates exist.
- **Gate 10 (Parity Legacy)**: Partial. Parity verified via Display format
  on error types and serde format on DTOs (Message, ContentBlock, InferEvent).
  Full behavioral parity is verified in nika-kernel-mock + L1 impl crates.

---

## 8. Relationship to nika-kernel-mock

`nika-kernel-mock` is the **companion test crate** (L0.5, same layer).
It provides pure-memory mock implementations of every kernel trait:

| Mock | Trait | Behavior |
|---|---|---|
| `MockClock` | Clock | Deterministic `advance(Duration)` |
| `InMemoryFs` | Fs | `HashMap<PathBuf, Vec<u8>>` |
| `MockHttpClient` | HttpClient | Programmable response queue |
| `MockShell` | ShellExecutor | Scripted outputs |
| `MemoryBlobStore` | BlobStore | In-memory HashMap |
| `MockProvider` | Provider | Deterministic responses, token counting |
| `NullMemoryStore` | MemoryStore | remember→nil, recall→empty, forget→ok |
| `NullEmbeddingProvider` | EmbeddingProvider | Zero vectors |
| `NullToolExecutor` | ToolExecutor | Returns NotAvailable |
| `NullContextCompressor` | ContextCompressor | Returns None |

`nika-kernel-mock` is admitted in the **same Step 3** as nika-kernel.
Both crates share Gate 12 atomic commit. Estimated ~2,000 LOC.

Every downstream crate (L1+) uses `nika-kernel-mock` in `[dev-dependencies]`
for hermetic testing without real I/O.

---

## 9. Error code ranges reserved

| Range | Subsystem | Owner crate |
|---|---|---|
| 140-189 | Http/network | nika-kernel (HttpError) |
| 050-099 | Shell/exec | nika-kernel (ShellError) |
| 380-429 | Provider | nika-kernel (ProviderError) |
| 100-139 | File/IO (BlobError mapped here) | nika-kernel (BlobError) |
| 600-649 | Memory | nika-kernel (MemoryError) |
| 230-279 | MCP/tools (ToolExecError mapped here) | nika-kernel (ToolExecError) |

Codes are defined as `NikaCode` consts in `src/errors.rs` and registered
in `nika-error`'s `ALL` registry via the per-crate error pattern.

---

## 10. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 S3 | Initial spec — ISP-layered traits + the Connectome/agent-v2 hooks. |

🦋
