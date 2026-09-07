# Crate spec — `nika-kernel-mock`

| | |
|---|---|
| Status | **ADMITTED 2026-04-13** (`ef8804371`) · companion test-double crate to `nika-kernel` |
| Layer | L0.5 (pure-memory test doubles, zero I/O, zero network) |
| Design | Hand-written mocks with builder pattern + call recording |
| LOC budget | <=2,000 src (target ~1,600, alarm at 2,000) |
| File cap | <=1,500 LOC each |
| Function cap | <=100 lines each |
| Source on `main` (reference) | `tools/nika-kernel-mock/src/` (12 files, ~2,267 LOC) |
| Crate version | tracks workspace (bumped to `0.90.0-alpha.1` at Phase 1 close) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-kernel-mock` provides **deterministic, in-memory test doubles** for
every trait defined in `nika-kernel`. It is the workspace's single source
of mock implementations, used as a `dev-dependency` by every crate from
L1 upward.

### Design principles

- **Zero I/O, zero network** — all mocks operate purely in memory.
  No tokio runtime required for construction; `async` methods resolve
  immediately or from canned data.
- **Builder pattern** — every mock is constructed via `MockX::new()`
  then configured with `.with_*()` and `.enqueue_*()` chainable methods.
- **Call recording** — every mock records calls it receives so tests can
  assert on what was called, in what order, with what arguments.
- **Send + Sync** — all mocks use `Arc<parking_lot::Mutex<...>>` or
  `Arc<parking_lot::RwLock<...>>` for interior mutability, making them
  safe to use in async test contexts and across `tokio::spawn` boundaries.
- **FIFO queues** — mocks that return canned responses (`MockProvider`,
  `MockShell`, `MockHttp`) consume from a `VecDeque` in FIFO order.
  When the queue is empty, they return a descriptive error (never panic).
- **No `nika-core` dependency** — unlike legacy, diamond `nika-kernel-mock`
  depends only on `nika-error` + `nika-kernel`. The legacy `nika-core`
  dependency (needed for `mcp::ContentBlock`) is eliminated by having
  `nika-kernel` re-export all necessary types.

### What this crate is NOT

- NOT a mocking framework (no `mockall`, no `mockito`). Hand-written
  mocks provide clarity, debuggability, and compile-time safety.
- NOT a provider simulator. The in-crate mock provider inside
  `nika-providers` (L1.5) provides a higher-fidelity deterministic
  LLM for integration tests.
  `MockProvider` here is a low-level trait double for unit tests.

### Legacy reference

The legacy crate at `main:tools/nika-kernel-mock/` (12 files, ~2,267 LOC)
provides the behavioral reference. Diamond rewrite eliminates:
- `nika-core` dependency (triangle break)
- `builtin.rs` `MockBuiltinRouter` (moves to `nika-kernel` scope as
  `ToolExecutor` trait replaces `BuiltinRouter`)
- `mcp.rs` `MockMcpPool` (absorbed into `MockToolExecutor`)
- `policy.rs` `MockPolicyChecker` (policy becomes a module in `nika-runtime`)
- `media.rs` `MockMediaContext` (moves to L2 `nika-media-cas` scope)
- `record.rs` `MockRecordStore` (moves to L2 scope)

Diamond adds:
- `NullMemoryStore` + `NullEmbeddingProvider` (the Connectome hooks, Phase 0 decision)
- `NullToolExecutor` + `MockToolExecutor` (agent-v2 hooks)
- `NullContextCompressor` (agent-v2 context compression hook)

---

## 2. Public API surface

### 2.1 MockClock — deterministic time

Implements: `nika_kernel::Clock`

```rust
#[derive(Clone)]
pub struct MockClock { /* Arc<Mutex<Inner>> */ }

impl MockClock {
    pub fn new() -> Self;                         // starts at real Instant::now(), offset 0
    pub fn advance(&self, duration: Duration);    // manually advance time
    pub fn elapsed_total(&self) -> Duration;      // total offset since creation
}

// Trait impl: Clock
// - now() → base + offset (deterministic)
// - sleep(_) → returns immediately (no actual wait)
// - elapsed(since) → default impl from Clock trait
```

### 2.2 MockFs — in-memory filesystem

Implements the `FsReadDyn`, `FsWriteDyn`, `FsMetaDyn` and `FsListDyn`
companions from `nika_kernel::fs`; their blanket implementations provide the
base traits and the `Fs` umbrella.

```rust
#[derive(Clone, Default)]
pub struct MockFs { /* Arc<RwLock<HashMap<PathBuf, Vec<u8>>>> */ }

impl MockFs {
    pub fn new() -> Self;
    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Self;
    pub fn seed(&self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>);
    pub fn files(&self) -> Vec<PathBuf>;          // list all stored paths
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}

// Trait impl: FsRead
// - read(path) → Ok(Bytes) or NotFound
// - read_to_string(path) → Ok(String) or NotFound/InvalidData
// - metadata(path) → FileMetadata { len, is_file, is_dir }
// - exists(path) → bool (checks files + implicit directories)
// - glob(root, pattern) → Vec<PathBuf> (suffix matching)
// - canonicalize(path) → Ok(path) (no-op, in-memory has no symlinks)

// Trait impl: FsWriteDyn
// - write_new(path, contents) → Ok(()) or FsError::AlreadyExists
// - write(path, contents) → Ok(()) (upsert)
// - create_dir_all(path) → Ok(()) (no-op, dirs are implicit)
// - remove_file(path) → Ok(()) or NotFound
```

Filesystem write methods return `Result<(), nika_kernel::fs::FsError>`.
See the [kernel backend migration contract](nika-kernel-core.md#4-filesystem-backend-migration)
for the provided method and wrapper obligations.

### 2.3 MockHttp — canned HTTP responses

Implements: `nika_kernel::HttpClient`

```rust
#[derive(Clone, Default)]
pub struct MockHttp { /* Arc<Mutex<VecDeque<Response>>> + Arc<Mutex<Vec<Request>>> */ }

impl MockHttp {
    pub fn new() -> Self;
    pub fn enqueue_ok(self, status: u16, body: impl Into<Bytes>) -> Self;
    pub fn enqueue_json(self, status: u16, json: &serde_json::Value) -> Self;
    pub fn enqueue_err(self, error: HttpError) -> Self;
    pub fn sent_requests(&self) -> Vec<HttpRequest>;   // recorded calls
    pub fn remaining(&self) -> usize;                   // queued responses left
}

// Trait impl: HttpClient
// - send(request) → records request, pops front of queue (or error if empty)
// - send_streaming(request) → HttpError::Unsupported (streaming not mocked at L0.5)
```

### 2.4 MockShell — recorded shell commands

Implements: `nika_kernel::ShellExecutor`

```rust
#[derive(Clone, Default)]
pub struct MockShell { /* Arc<Mutex<VecDeque<Result>>> + Arc<Mutex<Vec<ShellCommand>>> */ }

impl MockShell {
    pub fn new() -> Self;
    pub fn enqueue_ok(self, stdout: impl Into<String>) -> Self;
    pub fn enqueue_fail(self, status: i32, stderr: impl Into<String>) -> Self;
    pub fn enqueue_err(self, error: ShellError) -> Self;
    pub fn executed_commands(&self) -> Vec<ShellCommand>;  // recorded calls
}

// Trait impl: ShellExecutor
// - run(command) → records command, pops front of queue (or error if empty)
```

### 2.5 MockBlob — in-memory content-addressable store

Implements: `nika_kernel::BlobStore`

```rust
#[derive(Clone, Default)]
pub struct MockBlob { /* Arc<RwLock<HashMap<String, (Bytes, String)>>> */ }

impl MockBlob {
    pub fn new() -> Self;
    pub fn with_blob(self, hash: &str, data: impl Into<Bytes>, mime: &str) -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}

// Trait impl: BlobStore
// - put(data, mime) → Ok(BlobMetadata) with mock hash
// - get(hash) → Ok(Bytes) or NotFound
// - exists(hash) → bool
// - stat(hash) → Ok(BlobMetadata) or NotFound
// - delete(hash) → Ok(()) or NotFound
```

### 2.6 MockProvider — deterministic LLM responses

Implements: `nika_kernel::Provider`

```rust
#[derive(Clone, Default)]
pub struct MockProvider { /* name + Arc<Mutex<VecDeque<Response>>> + Arc<Mutex<Vec<Request>>> */ }

impl MockProvider {
    pub fn new(name: impl Into<Arc<str>>) -> Self;
    pub fn enqueue_text(self, text: impl Into<String>) -> Self;
    pub fn enqueue_response(self, response: InferResponse) -> Self;
    pub fn enqueue_error(self, error: ProviderError) -> Self;
    pub fn with_response_format_support(self, enabled: bool) -> Self;
    pub fn captured_requests(&self) -> Vec<InferRequest>;  // recorded calls
}

// Trait impl: Provider
// - name() → &str
// - capabilities(model) → None (or configurable)
// - infer(request) → records request, pops queue (or error if empty)
// - infer_stream(request) → synthesized stream from canned response
//   (Delta per text block + Usage + Done with stop_reason + request_id)
// - supports_response_format() → configurable bool
```

### 2.7 NullMemoryStore — no-op memory (the Connectome stub)

Implements: `nika_kernel::MemoryStore`

```rust
#[derive(Clone, Debug, Default)]
pub struct NullMemoryStore;

impl NullMemoryStore {
    pub fn new() -> Self;
}

// Trait impl: MemoryStore
// - remember(frame) → Ok(MemoryId::nil())  — always succeeds, stores nothing
// - recall(query) → Ok(vec![])             — always returns empty
// - forget(id) → Ok(())                    — always succeeds
```

### 2.8 NullEmbeddingProvider — zero-vector embeddings

Implements: `nika_kernel::EmbeddingProvider`

```rust
#[derive(Clone, Debug)]
pub struct NullEmbeddingProvider {
    dim: usize,
}

impl NullEmbeddingProvider {
    pub fn new(dimension: usize) -> Self;
    pub fn dimension(&self) -> usize;
}

// Trait impl: EmbeddingProvider
// - embed(text) → Ok(vec![0.0; self.dim])  — returns zero vector
// - dimension() → self.dim
```

### 2.9 NullToolExecutor — tool-not-available stub

Implements: `nika_kernel::ToolExecutor`

```rust
#[derive(Clone, Debug, Default)]
pub struct NullToolExecutor;

impl NullToolExecutor {
    pub fn new() -> Self;
}

// Trait impl: ToolExecutor
// - execute(call) → Err(ToolExecError::NotAvailable { tool: call.name })
// - execute_batch(calls) → vec of NotAvailable errors (default sequential impl)
```

### 2.10 MockToolExecutor — recorded tool calls (for agent tests)

Implements: `nika_kernel::ToolExecutor`

```rust
#[derive(Clone, Default)]
pub struct MockToolExecutor { /* Arc<Mutex<VecDeque<Response>>> + Arc<Mutex<Vec<ToolCall>>> */ }

impl MockToolExecutor {
    pub fn new() -> Self;
    pub fn enqueue_ok(self, result: ToolResult) -> Self;
    pub fn enqueue_err(self, error: ToolExecError) -> Self;
    pub fn captured_calls(&self) -> Vec<ToolCall>;    // recorded calls
}

// Trait impl: ToolExecutor
// - execute(call) → records call, pops queue (or NotAvailable if empty)
// - execute_batch(calls) → sequential via default impl
```

### 2.11 NullContextCompressor — never compresses

Implements: `nika_kernel::ContextCompressor` (agent-v2 hook)

```rust
#[derive(Clone, Debug, Default)]
pub struct NullContextCompressor;

impl NullContextCompressor {
    pub fn new() -> Self;
}

// Trait impl: ContextCompressor
// - compress(messages) → Ok(None)   — never compresses, always returns None
// - should_compress(messages) → false
```

---

## 3. File layout

```
crates/nika-kernel-mock/
  Cargo.toml
  src/
    lib.rs                (~40 LOC — pub mod + re-exports of all 11 types)
    clock.rs              (~80 LOC — MockClock)
    filesystem.rs         (~170 LOC — MockFs: FsRead + FsWrite)
    http.rs               (~110 LOC — MockHttp: HttpClient)
    shell.rs              (~100 LOC — MockShell: ShellExecutor)
    blob.rs               (~120 LOC — MockBlob: BlobStore)
    provider.rs           (~200 LOC — MockProvider: Provider)
    memory.rs             (~60 LOC — NullMemoryStore + NullEmbeddingProvider)
    tool_executor.rs      (~120 LOC — NullToolExecutor + MockToolExecutor)
    compressor.rs         (~40 LOC — NullContextCompressor)
```

Target: ~1,040 LOC src, ~600 LOC tests = ~1,640 LOC total.

---

## 4. Dependencies

```toml
[dependencies]
nika-error  = { path = "../nika-error" }
nika-kernel = { path = "../nika-kernel" }
parking_lot = { workspace = true }
bytes       = { workspace = true }
serde_json  = { workspace = true }

[dev-dependencies]
tokio    = { workspace = true, features = ["rt", "macros"] }
rstest   = { workspace = true }
proptest = { workspace = true }

[lints]
workspace = true
```

### Dependency rationale

- **`nika-error`** — for `NikaResult` return types and error propagation.
- **`nika-kernel`** — the traits this crate implements (sole raison d'etre).
- **`parking_lot`** — `Mutex` and `RwLock` for interior mutability in
  `Send + Sync` mocks. Preferred over `std::sync::Mutex` for non-poisoning
  semantics and `!Send` read guards (invariant #16).
- **`bytes`** — `Bytes` type used in `BlobStore`, `HttpClient`, `FsRead` traits.
- **`serde_json`** — `Value` type used in `InferRequest`, `ToolCall`, etc.

### Notable NON-dependencies

- **No `async-trait`** — diamond uses `trait_variant` (decision A1) or
  native async fn in traits. No `#[async_trait]` macro needed.
- **No `tokio`** in `[dependencies]` — mocks do not need a runtime to
  construct or operate. Only `dev-dependencies` for `#[tokio::test]`.
- **No `futures-util`** — stream synthesis in `MockProvider::infer_stream`
  uses `futures_core::stream::iter` (already available via `nika-kernel`
  re-export) or a minimal hand-rolled stream.
- **No `nika-core`** — the legacy triangle `nika-kernel-mock → nika-core`
  is broken. All types flow through `nika-kernel` re-exports.

---

## 5. Test plan

### Strategy: test the mocks themselves

Mocks are test infrastructure, but buggy mocks produce false test results.
Each mock gets its own `#[cfg(test)] mod tests` verifying:

1. **Basic contract** — the mock honors the trait contract (e.g., `FsRead::read`
   returns `NotFound` for missing files, not panic).
2. **Builder roundtrip** — `.with_*()` / `.enqueue_*()` methods produce
   the expected state.
3. **Call recording** — `.calls()` / `.captured_*()` accurately reflects
   what was invoked.
4. **FIFO ordering** — queued responses come out in insertion order.
5. **Empty queue** — when the queue is exhausted, a descriptive error is
   returned (never panic, never `unwrap`).
6. **Clone shares state** — `Arc`-backed mocks share state across clones
   (important for multi-task test scenarios).
7. **Send + Sync** — compile-time assertion via `fn assert_send_sync<T: Send + Sync>()`.

### Test count per module

| Test location | Scope | Count |
|---|---|---|
| `src/clock.rs` #[cfg(test)] | advance, elapsed, sleep-is-instant, clone-shares-state | ~5 |
| `src/filesystem.rs` #[cfg(test)] | read/write roundtrip, missing file, seed, metadata, glob, remove | ~7 |
| `src/http.rs` #[cfg(test)] | enqueue/send, json, error, tracking, empty queue | ~5 |
| `src/shell.rs` #[cfg(test)] | enqueue/run, fail, error, command tracking | ~4 |
| `src/blob.rs` #[cfg(test)] | put/get roundtrip, missing, exists, stat, delete | ~5 |
| `src/provider.rs` #[cfg(test)] | text response, request recording, stream synthesis, empty queue | ~5 |
| `src/memory.rs` #[cfg(test)] | null remember/recall/forget, null embed zero-vector | ~4 |
| `src/tool_executor.rs` #[cfg(test)] | null not-available, mock enqueue/execute, recording | ~5 |
| `src/compressor.rs` #[cfg(test)] | null returns None, should_compress false | ~2 |
| **Total** | | **~42** |

All tests inline (`#[cfg(test)] mod tests`), run with `cargo test --lib`.

---

## 6. Parity with legacy

The legacy crate on main has 12 source files. Diamond coverage:

| Legacy module | Diamond equivalent | Notes |
|---|---|---|
| `clock.rs` (110 LOC) | `clock.rs` (~80 LOC) | Rewrite propre, same semantics |
| `filesystem.rs` (182 LOC) | `filesystem.rs` (~170 LOC) | Add builder `.with_file()` |
| `http.rs` (126 LOC) | `http.rs` (~110 LOC) | Rename `MockHttpClient` to `MockHttp` |
| `shell.rs` (142 LOC) | `shell.rs` (~100 LOC) | Same semantics |
| `store.rs` (148 LOC) | `blob.rs` (~120 LOC) | Rename `MemoryBlobStore` to `MockBlob` |
| `provider.rs` (290 LOC) | `provider.rs` (~200 LOC) | Leaner, no `futures-util` dep |
| `builtin.rs` (236 LOC) | `tool_executor.rs` (~120 LOC) | `BuiltinRouter` replaced by `ToolExecutor` |
| `mcp.rs` (438 LOC) | absorbed into `tool_executor.rs` | MCP calls go through `ToolExecutor` |
| `policy.rs` (94 LOC) | DROPPED | Policy is a module in `nika-runtime` |
| `media.rs` (274 LOC) | DROPPED (moves to L2 scope) | `nika-media-cas` owns its mock |
| `record.rs` (192 LOC) | DROPPED (moves to L2 scope) | L2 crate owns its mock |
| NEW | `memory.rs` (~60 LOC) | the Connectome hooks: NullMemoryStore + NullEmbeddingProvider |
| NEW | `compressor.rs` (~40 LOC) | Agent-v2 hook: NullContextCompressor |

**Net**: 12 files (legacy) to 10 files (diamond). ~2,267 LOC to ~1,040 LOC (-54%).

---

## 7. Naming conventions

All mock types follow a consistent naming pattern:

- **`Mock*`** — programmable doubles with FIFO queues and call recording.
  Used when the test needs to control responses and assert on calls.
  Examples: `MockClock`, `MockFs`, `MockHttp`, `MockShell`, `MockBlob`,
  `MockProvider`, `MockToolExecutor`.

- **`Null*`** — zero-behavior stubs that accept all input and return
  neutral values. Used when the test does not exercise this trait but
  needs a value to satisfy type requirements. Examples: `NullMemoryStore`,
  `NullEmbeddingProvider`, `NullToolExecutor`, `NullContextCompressor`.

---

## 8. Gate exemptions

- **Gate 7 (Benchmarks)**: Exempt. L0.5 test doubles, no hot path.
  Mocks are never called in production code paths.
- **Gate 9 (Canary E2E)**: Exempt. No runtime yet. Mocks are consumed
  as dev-dependencies by other crates' tests, not by workflows.
- **Gate 10 (Parity Legacy)**: Partially exempt. Legacy `MockMcpPool`,
  `MockMediaContext`, `MockRecordStore`, and `MockPolicyChecker` are
  dropped (responsibilities moved to L2 crate scopes). Parity is verified
  for the 7 mocks that have direct legacy counterparts (clock, filesystem,
  http, shell, store/blob, provider, builtin/tool_executor).

---

## 9. Architecture constraints

- **Zero async runtime in construction**: `MockX::new()` must work
  without a tokio runtime. Only trait methods (which are `async`) need
  a runtime, and they resolve immediately.
- **No conditional compilation**: all mocks are always compiled. No
  `#[cfg(feature = "...")]` gates. The crate exists solely for testing.
- **No re-export of `nika-kernel` types**: consumers depend on
  `nika-kernel` directly for trait types. `nika-kernel-mock` exports
  only mock implementations.
- **Invariant #16**: `parking_lot::RwLockReadGuard` is `!Send`. All
  `RwLock` usage in mocks must drop guards before any `.await` point.
  Since mock trait methods resolve immediately (no real `.await`), this
  is naturally satisfied, but tests must verify it.

---

## 10. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 S3 | Initial spec — 11 mock types for diamond kernel traits. |

🦋
