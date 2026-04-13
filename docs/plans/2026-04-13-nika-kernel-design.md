# nika-kernel + nika-kernel-mock -- Design

Brainstorm session 2026-04-13. Validated by user.

## Decisions

| # | Decision | Rationale |
|---|---|---|
| 1 | Types co-located with traits (B) | Cohesion by domain, no god-file |
| 2 | Clock SYNC, everything else ASYNC | YAGNI on network time, simpler hot paths |
| 3 | trait_variant on all async traits | Zero boxing static path, Rust 1.91 native |
| 4 | ISP max (20 atomic + 6 super) | Consumers bind minimum needed |
| 5 | Errors per subsystem | No god-enum, each domain owns its codes |
| 6 | Provider super = Infer + Stream + Meta | All providers MUST stream (even mock) |
| 7 | ProviderEmbed + ProviderVision opt-in | Not all providers have these |
| 8 | No tokio dep in kernel | Pure trait definitions, L0.5 |
| 9 | Cancel as fn param, not in struct | Keeps ShellCommand free of tokio-util |
| 10 | BTreeMap over HashMap | Deterministic, no hasher dependency |

## File layout (10 files)

```
tools/nika-kernel/src/
  lib.rs           (~40 LOC -- re-exports)
  clock.rs         (~30 LOC -- Clock sync, Instant/SystemTime)
  fs.rs            (~120 LOC -- FsRead/FsWrite/FsMeta/FsList + Fs)
  http.rs          (~100 LOC -- HttpGet/HttpPost + HttpClient)
  process.rs       (~150 LOC -- ShellCommand/Result/Error + ShellRun/Cancel + ShellExecutor)
  blob.rs          (~60 LOC -- BlobStore, BlobHash)
  provider.rs      (~350 LOC -- Message/Role/ContentBlock/InferReq/Resp/Event/Error + ISP traits)
  tool_executor.rs (~120 LOC -- ToolCall/Result/Error + ToolExecute/Batch + ToolExecutor)
  memory.rs        (~200 LOC -- MemoryFrame/Hit/Query/Directive/Level/Id/Error + ISP traits)
  agent.rs         (~180 LOC -- Checkpoint/Config/Outcome + ContextCompressor)

TOTAL: ~1,350 LOC
```

```
tools/nika-kernel-mock/src/
  lib.rs
  mock_clock.rs    (MockClock -- controllable Instant)
  mock_fs.rs       (InMemoryFs -- HashMap<PathBuf, Bytes>)
  mock_http.rs     (MockHttp -- request/response recordings)
  mock_shell.rs    (MockShell -- scripted stdout/exit codes)
  mock_blob.rs     (InMemoryBlob -- HashMap<BlobHash, Bytes>)
  mock_provider.rs (MockProvider -- deterministic responses)
  mock_tool.rs     (MockToolExecutor -- Vec<(call, result)>)
  null_memory.rs   (NullMemoryStore -- no-op)

TOTAL: ~800 LOC
```

## Trait hierarchy

### Sync

```
Clock
  fn now(&self) -> Instant
  fn system_now(&self) -> SystemTime
```

### Async (all with trait_variant for Dyn companion)

```
Fs
  FsRead   -- read, read_to_string, exists
  FsWrite  -- write, create_dir_all, remove_file
  FsMeta   -- metadata, canonicalize
  FsList   -- glob, list_dir

HttpClient
  HttpGet  -- get(url) -> Response
  HttpPost -- post(url, body) -> Response

ShellExecutor
  ShellRun    -- run(cmd) -> ShellResult
  ShellCancel -- cancel(handle) -> ()

BlobStore (no split -- atomic ops)
  store, load, exists, hash

Provider (super = Infer + Stream + Meta)
  ProviderInfer  -- name(), infer(req) -> InferResponse
  ProviderStream -- infer_stream(req) -> InferEventStream
  ProviderMeta   -- capabilities(), supports_response_format()
  ProviderEmbed  -- embed(text) -> Vec<f32>  [OPT-IN]
  ProviderVision -- supports_vision() -> bool [OPT-IN]

ToolExecutor
  ToolExecute -- execute(ToolCall) -> ToolResult
  ToolBatch   -- execute_batch(Vec<ToolCall>) [default: sequential]

MemoryStore
  MemoryRemember -- remember(MemoryFrame) -> MemoryId
  MemoryRecall   -- recall(RecallQuery) -> Vec<MemoryHit>
  MemoryForget   -- forget(MemoryId) -> ()

EmbeddingProvider
  embed(text) -> Vec<f32>
  dimension() -> usize

ContextCompressor
  compress(messages, policy) -> Option<CompressedContext>
```

## Error types (per subsystem)

| Error | Variants | Codes |
|---|---|---|
| ProviderError | Api, ModelNotFound, RateLimited, Auth, Other | NIKA 380-429 |
| ShellError | NotFound, Timeout, Cancelled, Blocked, Other | NIKA 050-099 |
| ToolExecError | NotFound, InvalidParams, Timeout, Other | NIKA 430-479 |
| MemoryError | Unavailable, NotFound, EmbeddingFailed, Storage | NIKA 600-649 |

Fs, Http, Blob use `std::io::Error` directly (no wrapper needed).
Clock is infallible (sync, no error).

## Dependencies

```toml
# nika-kernel
[dependencies]
nika-error    = { path = "../nika-error" }
trait-variant = { workspace = true }
bytes         = { workspace = true }
futures-core  = { workspace = true }    # Stream trait
thiserror     = { workspace = true }
miette        = { workspace = true }
serde         = { workspace = true, optional = true }
serde_json    = { workspace = true }    # Value in ToolCall

# NO tokio, NO tokio-util, NO async-trait, NO nika-catalog
```

## Key differences vs legacy

| Legacy | Diamond |
|---|---|
| async_trait (boxing) | trait_variant (zero-cost static) |
| 15 files | 10 files (scoped down) |
| 1 Provider monolith | 5 ISP traits |
| 0 memory hooks | Full Cortex hooks |
| 0 agent types | Full v2 types |
| CancellationToken in struct | fn param (no tokio in kernel) |
| HashMap | BTreeMap (deterministic) |
| Fs 2 splinters | Fs 4 ISP |

## Stream type

```rust
// trait_variant does NOT handle Stream return types.
// Solution: boxed type alias (single allocation per LLM call).
pub type InferEventStream =
    Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>>;
```
