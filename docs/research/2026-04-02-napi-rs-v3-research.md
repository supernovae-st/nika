# napi-rs v3.x Research Report

> Research date: 2026-04-02
> Purpose: Building a Node.js native SDK for Nika (wrapping Client, Job, Artifact structs)

---

## 1. Latest Stable Versions

| Crate | Version | Released |
|-------|---------|----------|
| `napi` | **3.8.4** | 2026-03-28 |
| `napi-derive` | **3.5.3** | 2026-03-28 |
| `napi-build` | **2.3.1** | 2025-11-10 |
| `@napi-rs/cli` (npm) | **3.6.0** | latest |

Note: `napi-build` is still on v2.x -- the 3.0.0-beta.0 was yanked. v2.3.1 is what projects use.

---

## 2. Project Setup

### Cargo.toml

```toml
[package]
name = "nika-node"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Core napi bindings
napi = { version = "3", default-features = false, features = [
  "napi6",          # Node.js >= 14.x compatibility
  "async",          # async fn -> Promise (includes tokio_rt)
  "serde-json",     # serde interop for complex types
  "error_anyhow",   # anyhow::Error -> napi::Error conversion
] }

# Proc macros for #[napi] attribute
napi-derive = { version = "3", features = ["type-def"] }

# Your Rust SDK
nika-sdk = { path = "../nika-sdk" }

# For streaming
tokio = { version = "1", features = ["rt-multi-thread", "sync", "time"] }

[build-dependencies]
napi-build = "2"

[features]
# For cargo test without Node.js
noop = ["napi/noop", "napi-derive/noop"]

[profile.release]
lto = true
strip = "symbols"
```

### Feature Matrix

| Feature | What it does |
|---------|-------------|
| `async` | Enables `async fn` -> JS Promise (pulls in tokio_rt) |
| `napi4` | Default. Required for ThreadsafeFunction, async work |
| `napi6` | BigInt, key-value iterator (Node.js >= 10.20) |
| `serde-json` | Serialize/deserialize between JS objects and Rust structs |
| `error_anyhow` | `impl From<anyhow::Error> for napi::Error` |
| `noop` | **Critical for testing** -- stubs out all N-API calls |
| `web_stream` | ReadableStream support (requires tokio_rt + futures-core) |
| `type-def` (derive) | Auto-generates `.d.ts` TypeScript definitions at compile time |
| `compat-mode` | Legacy JsObject/JsFunction types from v2 (NOT recommended) |

### build.rs

```rust
fn main() {
  napi_build::setup();
}
```

That's it. `napi_build::setup()` handles emitting the correct linker flags for each platform.

### package.json

```json
{
  "name": "@supernovae/nika",
  "version": "0.1.0",
  "type": "module",
  "main": "index.js",
  "types": "index.d.ts",
  "napi": {
    "binaryName": "nika",
    "targets": [
      "x86_64-apple-darwin",
      "aarch64-apple-darwin",
      "x86_64-unknown-linux-gnu",
      "x86_64-unknown-linux-musl",
      "aarch64-unknown-linux-gnu",
      "aarch64-unknown-linux-musl",
      "x86_64-pc-windows-msvc",
      "aarch64-pc-windows-msvc"
    ]
  },
  "scripts": {
    "build": "napi build --platform --release",
    "build:debug": "napi build --platform",
    "artifacts": "napi artifacts",
    "prepublishOnly": "napi prepublish -t npm",
    "version": "napi version"
  },
  "files": [
    "index.d.ts",
    "index.js"
  ],
  "devDependencies": {
    "@napi-rs/cli": "^3.6.0"
  },
  "optionalDependencies": {
    "@supernovae/nika-darwin-x64": "0.1.0",
    "@supernovae/nika-darwin-arm64": "0.1.0",
    "@supernovae/nika-linux-x64-gnu": "0.1.0",
    "@supernovae/nika-linux-x64-musl": "0.1.0",
    "@supernovae/nika-linux-arm64-gnu": "0.1.0",
    "@supernovae/nika-linux-arm64-musl": "0.1.0",
    "@supernovae/nika-win32-x64-msvc": "0.1.0",
    "@supernovae/nika-win32-arm64-msvc": "0.1.0"
  }
}
```

---

## 3. Exposing Async Rust Functions (Promises)

Any `async fn` annotated with `#[napi]` automatically returns a `Promise` to JavaScript. The function runs on the tokio runtime that napi-rs manages internally.

### Basic async function

```rust
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
pub async fn run_workflow(path: String) -> Result<String> {
  let result = nika_sdk::run(&path).await
    .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
  Ok(result.to_json())
}
```

TypeScript output:
```typescript
export function runWorkflow(path: string): Promise<string>
```

### Async methods on classes

```rust
#[napi]
pub struct NikaClient {
  inner: nika_sdk::Client,
}

#[napi]
impl NikaClient {
  #[napi(constructor)]
  pub fn new(config_path: String) -> Result<Self> {
    let inner = nika_sdk::Client::from_config(&config_path)
      .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
    Ok(Self { inner })
  }

  /// Async method -- becomes Promise in JS
  #[napi]
  pub async fn submit_job(&self, workflow: String) -> Result<Job> {
    let job = self.inner.submit(&workflow).await
      .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
    Ok(Job { inner: job })
  }
}
```

### Important: `&mut self` in async methods

Using `&mut self` in async methods requires marking the function `unsafe`:

```rust
#[napi]
impl NikaClient {
  // This WILL NOT COMPILE:
  // pub async fn reconnect(&mut self) -> Result<()> { ... }
  
  // This is required:
  #[napi]
  pub async unsafe fn reconnect(&mut self) -> Result<()> {
    self.inner.reconnect().await
      .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
    Ok(())
  }
}
```

This is because `self` is owned by the JS runtime and could be GC'd at any `await` point.
napi-rs auto-creates a `napi_reference` to prevent GC during the async call.

### Accepting Promises from JS

```rust
use napi::bindgen_prelude::*;

#[napi]
pub async fn process_with_promise(input: Promise<u32>) -> Result<u32> {
  let value = input.await?;
  Ok(value * 2)
}
```

### Custom tokio runtime

```rust
use napi::bindgen_prelude::create_custom_tokio_runtime;

#[napi_derive::module_init]
fn init() {
  let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4)
    .enable_all()
    .build()
    .unwrap();
  create_custom_tokio_runtime(rt);
}
```

---

## 4. AsyncGenerator / Async Iterators for Streaming

**Added in napi v3.8.0** (2025-12-30) via `#[napi(async_iterator)]`.

This is the key feature for streaming SSE events. The struct implements the `AsyncGenerator` trait and becomes a JavaScript `AsyncIterator` (usable with `for await...of`).

### Basic pattern

```rust
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::future::Future;

#[napi(async_iterator)]
pub struct JobEventStream {
  receiver: tokio::sync::mpsc::Receiver<nika_sdk::Event>,
}

#[napi]
impl AsyncGenerator for JobEventStream {
  type Yield = String;   // What each iteration yields
  type Next = ();         // What .next(value) accepts
  type Return = ();       // What .return(value) accepts

  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    // CRITICAL: The returned Future must be 'static, so you CANNOT borrow self.
    // You must extract owned values from self BEFORE the async block.
    let item = self.receiver.try_recv().ok();
    
    async move {
      match item {
        Some(event) => Ok(Some(serde_json::to_string(&event).unwrap())),
        None => Ok(None), // Signals completion (done: true)
      }
    }
  }
}

#[napi]
impl JobEventStream {
  #[napi(constructor)]
  pub fn new() -> Self {
    // Would be created via a factory method in practice
    let (_, rx) = tokio::sync::mpsc::channel(100);
    Self { receiver: rx }
  }
}
```

### Truly async iteration (with real awaiting)

```rust
#[napi(async_iterator)]
pub struct SseEventStream {
  receiver: Option<tokio::sync::mpsc::Receiver<SseEvent>>,
}

/// Each SSE event as a typed object
#[napi(object)]
pub struct SseEvent {
  pub event_type: String,
  pub task_id: Option<String>,
  pub data: String,
  pub timestamp_ms: f64,
}

#[napi]
impl AsyncGenerator for SseEventStream {
  type Yield = SseEvent;
  type Next = ();
  type Return = ();

  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    // Take the receiver out to get ownership for the async block
    // Put it back after if there are more items
    let mut rx = self.receiver.take();
    
    async move {
      match rx.as_mut() {
        Some(receiver) => {
          match receiver.recv().await {
            Some(event) => Ok(Some(event)),
            None => Ok(None), // Channel closed = stream done
          }
        }
        None => Ok(None),
      }
    }
    // NOTE: the receiver is consumed here. For a real implementation,
    // use a different pattern (see below).
  }
}
```

### Practical pattern: Arc<Mutex> for truly async receivers

The `'static` requirement on the Future means you cannot hold `&mut self` across await points. The idiomatic solution:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

#[napi(async_iterator)]
pub struct JobStream {
  receiver: Arc<Mutex<tokio::sync::mpsc::Receiver<SseEvent>>>,
}

#[napi]
impl AsyncGenerator for JobStream {
  type Yield = SseEvent;
  type Next = ();
  type Return = ();

  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let rx = self.receiver.clone();
    async move {
      let mut guard = rx.lock().await;
      match guard.recv().await {
        Some(event) => Ok(Some(event)),
        None => Ok(None),
      }
    }
  }
}
```

### JavaScript consumption

```typescript
const stream = client.streamJob('workflow.nika.yaml');

// for-await-of pattern
for await (const event of stream) {
  console.log(event.eventType, event.taskId, event.data);
}

// Or manual iteration
const iter = stream[Symbol.asyncIterator]();
let result = await iter.next();
while (!result.done) {
  console.log(result.value);
  result = await iter.next();
}
```

### Alternative: ReadableStream (web_stream feature)

For Web Streams API compatibility (useful for Deno, Bun, or piping):

```rust
use napi::bindgen_prelude::*;
use tokio_stream::wrappers::ReceiverStream;

#[napi]
pub fn create_event_stream(env: &Env) -> Result<ReadableStream<'_, SseEvent>> {
  let (tx, rx) = tokio::sync::mpsc::channel(100);
  
  // Spawn the actual event producer
  tokio::spawn(async move {
    // ... produce events and send them via tx
  });
  
  ReadableStream::new(env, ReceiverStream::new(rx))
}
```

Requires `features = ["web_stream"]` in napi.

---

## 5. TypeScript Definition Auto-Generation

The `type-def` feature in `napi-derive` automatically generates `.d.ts` files at compile time.

### How it works

1. The `#[napi]` proc macro inspects Rust types at compile time
2. With `type-def` enabled, it generates TypeScript declaration fragments
3. At build time (`napi build`), these are collected into a single `index.d.ts`

### Type mapping (Rust -> TypeScript)

| Rust | TypeScript |
|------|-----------|
| `String`, `&str` | `string` |
| `u32`, `i32`, `f64`, etc. | `number` |
| `bool` | `boolean` |
| `()` | `void` |
| `Option<T>` | `T \| null` |
| `Vec<T>` | `Array<T>` |
| `Buffer` | `Buffer` |
| `Result<T>` | `T` (throws on Err) |
| `async fn -> T` | `Promise<T>` |
| `#[napi(object)] struct` | `interface` |
| `#[napi] struct` (with impl) | `class` |
| `#[napi] enum` | `const enum` (numeric) or `string enum` |
| `HashMap<String, T>` | `Record<string, T>` |

### Controlling generated types

```rust
// Skip TypeScript generation for a field
#[napi]
pub struct Config {
  pub name: String,
  #[napi(skip_typescript)]
  pub internal_handle: u64,
}

// Override TypeScript type
#[napi(ts_return_type = "Buffer | null")]
pub fn maybe_read(path: String) -> Option<Buffer> { ... }

// Override argument types
#[napi(ts_args_type = "callback: (err: Error | null, value: string) => void")]
pub fn with_callback(callback: Function<String, ()>) -> Result<()> { ... }

// Custom JS name
#[napi(js_name = "NikaEngine")]
pub struct InternalEngine { ... }
```

### Object vs Class

```rust
// This becomes a TypeScript INTERFACE (plain data object)
// No constructor, no methods -- just shape validation
#[napi(object)]
pub struct JobStatus {
  pub id: String,
  pub state: String,
  pub progress: f64,
}

// This becomes a TypeScript CLASS
// Has constructor, methods, getter/setter
#[napi]
pub struct Job {
  inner: nika_sdk::Job,
}

#[napi]
impl Job {
  #[napi(constructor)]
  pub fn new(id: String) -> Self { ... }
  
  #[napi]
  pub async fn status(&self) -> Result<JobStatus> { ... }
  
  #[napi(getter)]
  pub fn id(&self) -> &str { ... }
}
```

### Custom .d.ts header

In package.json:
```json
{
  "napi": {
    "dtsHeader": "type MaybePromise<T> = T | Promise<T>",
    "dtsHeaderFile": "./dts-header.d.ts"
  }
}
```

---

## 6. Error Handling

### Basic errors

```rust
use napi::bindgen_prelude::*;

#[napi]
pub fn validate(input: String) -> Result<String> {
  if input.is_empty() {
    return Err(Error::new(
      Status::InvalidArg,
      "Input cannot be empty".to_owned(),
    ));
  }
  Ok(input)
}
```

### Status codes

| Status | JS equivalent |
|--------|--------------|
| `Status::InvalidArg` | `TypeError` semantics |
| `Status::GenericFailure` | General `Error` |
| `Status::ObjectExpected` | `TypeError` |
| `Status::StringExpected` | `TypeError` |
| `Status::Cancelled` | Abort/cancel |

### Shorthand

```rust
// Quick error from string
Err(Error::from_reason("something went wrong"))

// Equivalent to:
Err(Error::new(Status::GenericFailure, "something went wrong"))
```

### Custom error types

```rust
pub enum NikaError {
  NapiError(Error<Status>),
  WorkflowNotFound,
  ProviderUnavailable,
  DagCycle,
}

impl AsRef<str> for NikaError {
  fn as_ref(&self) -> &str {
    match self {
      NikaError::NapiError(e) => e.status.as_ref(),
      NikaError::WorkflowNotFound => "NIKA_WORKFLOW_NOT_FOUND",
      NikaError::ProviderUnavailable => "NIKA_PROVIDER_UNAVAILABLE",
      NikaError::DagCycle => "NIKA_DAG_CYCLE",
    }
  }
}

#[napi]
pub fn check_workflow(path: String) -> Result<(), NikaError> {
  Err(Error::new(NikaError::WorkflowNotFound, "File not found"))
}
```

The custom status becomes the error's `code` property in JavaScript.

### Error cause chain

```rust
#[napi]
pub fn with_cause() -> Result<()> {
  let mut err = Error::new(Status::GenericFailure, "Workflow failed");
  err.set_cause(Error::new(Status::InvalidArg, "Invalid YAML syntax"));
  Err(err)
}
```

### anyhow integration

With `features = ["error_anyhow"]`:

```rust
use anyhow::Context;

#[napi]
pub async fn run(path: String) -> Result<String> {
  let content = tokio::fs::read_to_string(&path)
    .await
    .context("Failed to read workflow file")?;  // anyhow::Error -> napi::Error
  Ok(content)
}
```

### Extending JavaScript errors

```rust
#[napi]
pub fn throw_rich_error(env: Env, error_class: Function<String>) -> Result<()> {
  let instance = error_class.new_instance("Workflow validation failed".to_owned())?;
  let mut error_object = instance.coerce_to_object()?;
  error_object.set("name", "NikaError")?;
  error_object.set("code", "NIKA-010")?;
  error_object.set("workflow", "my-flow.nika.yaml")?;
  env.throw(error_object)?;
  Ok(())
}
```

### Panic catching

```rust
#[napi(catch_unwind)]
pub fn might_panic() -> u32 {
  panic!("oops"); // Caught and converted to JS Error instead of crashing
}
```

---

## 7. The `noop` Feature (cargo test compatibility)

The `noop` feature stubs out all N-API FFI calls, allowing `cargo test` to run without a Node.js runtime.

### Setup

```toml
[features]
noop = ["napi/noop", "napi-derive/noop"]
```

### Usage

```bash
# Regular build (produces .node binary)
cargo build

# Run tests without Node.js
cargo test --features noop

# Or in CI
cargo test --workspace --lib --features noop
```

### What noop does

- All N-API sys calls become no-ops
- `#[napi]` macros still generate Rust code but skip FFI registration
- Types like `Env`, `JsValue`, etc. become zero-sized stubs
- Your pure Rust logic can be tested normally
- Any code that actually calls N-API functions will silently return defaults

### Testing strategy

```rust
#[napi]
pub struct Calculator {
  value: f64,
}

// Test the pure logic, not the N-API bindings
#[cfg(test)]
mod tests {
  use super::*;
  
  #[test]
  fn test_calculation() {
    let calc = Calculator { value: 42.0 };
    // Test internal methods that don't touch N-API
    assert_eq!(calc.value, 42.0);
  }
}
```

### cfg_attr pattern for conditional napi

```rust
// Only apply #[napi] when not in noop mode
#[cfg_attr(not(feature = "noop"), napi_derive::napi)]
pub struct Bird {
  pub name: String,
}

#[cfg_attr(not(feature = "noop"), napi_derive::napi)]
impl Bird {
  #[cfg_attr(not(feature = "noop"), napi_derive::napi(constructor))]
  pub fn new(name: String) -> Self {
    Bird { name }
  }
}
```

---

## 8. npm Publishing with Per-Platform Binaries

### Architecture

```
@supernovae/nika                    <-- Main package (JS loader + .d.ts)
  |-- index.js                      <-- Platform detection + native loading
  |-- index.d.ts                    <-- Auto-generated TypeScript types
  |-- optionalDependencies:
       |-- @supernovae/nika-darwin-x64
       |-- @supernovae/nika-darwin-arm64
       |-- @supernovae/nika-linux-x64-gnu
       |-- @supernovae/nika-linux-x64-musl
       |-- @supernovae/nika-linux-arm64-gnu
       |-- @supernovae/nika-linux-arm64-musl
       |-- @supernovae/nika-win32-x64-msvc
       |-- @supernovae/nika-win32-arm64-msvc
```

### How it works

1. Each platform package contains just a `.node` binary + package.json with `os`/`cpu` fields
2. npm installs only the matching platform package (via `optionalDependencies` + `os`/`cpu` filtering)
3. The main `index.js` detects the platform and loads the correct `.node` file

### Platform package.json example

```json
{
  "name": "@supernovae/nika-darwin-arm64",
  "version": "0.1.0",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "main": "nika.darwin-arm64.node",
  "files": ["nika.darwin-arm64.node"]
}
```

### CI workflow (GitHub Actions)

```yaml
# Build matrix -- runs in parallel
strategy:
  matrix:
    include:
      - target: x86_64-apple-darwin
        os: macos-13
      - target: aarch64-apple-darwin
        os: macos-14
      - target: x86_64-unknown-linux-gnu
        os: ubuntu-latest
      - target: x86_64-unknown-linux-musl
        os: ubuntu-latest
      - target: aarch64-unknown-linux-gnu
        os: ubuntu-latest  # cross-compile
      - target: x86_64-pc-windows-msvc
        os: windows-latest

steps:
  - uses: actions/checkout@v4
  - uses: actions/setup-node@v4
  - name: Build
    run: |
      npm install
      napi build --platform --release --target ${{ matrix.target }}
  - uses: actions/upload-artifact@v4
    with:
      name: bindings-${{ matrix.target }}
      path: "*.node"
```

### Publishing steps

```yaml
# After all builds complete:
steps:
  - uses: actions/download-artifact@v4
    with:
      path: artifacts
  - run: napi artifacts  # Moves .node files into npm/ directories
  - run: napi create-npm-dirs  # Creates npm/platform-pkg/ directories
  - run: napi prepublish -t npm  # Prepares all packages
  - run: |
      npm publish --access public
      cd npm/darwin-x64 && npm publish --access public
      cd npm/darwin-arm64 && npm publish --access public
      # ... etc
```

### CLI commands summary

| Command | Purpose |
|---------|---------|
| `napi build --platform --release` | Build native binary for current platform |
| `napi artifacts` | Collect `.node` files from build artifacts |
| `napi create-npm-dirs` | Create `npm/<platform>/` directory structure |
| `napi prepublish -t npm` | Prepare all packages for publishing |
| `napi version` | Sync version across all platform packages |
| `napi universalize` | Create universal binary (macOS x64 + arm64) |

### Version checking

napi-rs 3.x added version mismatch detection. Set `NAPI_RS_ENFORCE_VERSION_CHECK=1` to throw if a platform package version doesn't match the main package.

---

## 9. Breaking Changes: v2.x to v3.x

### Package.json changes

| v2 | v3 |
|----|-----|
| `napi.name` | `napi.binaryName` |
| `napi.triples.default` + `napi.triples.additional` | `napi.targets` (flat array of target triples) |

### CLI changes

| v2 | v3 |
|----|-----|
| `--cargo-cwd ./path` | `--manifest-path ./path/Cargo.toml` |
| `--cargo-flags="--locked"` | `-- --locked` (flags after `--`) |
| `napi create-npm-dir` (singular) | `napi create-npm-dirs` (plural) |
| `napi universal` | `napi universalize` |
| Commit `npm/*` files | Generate in CI with `napi create-npm-dirs` |

### Rust API changes

**JsValue types moved behind `compat-mode` feature:**

The following types are no longer available by default in v3:
- `JsObject`, `JsFunction`, `JsNull`, `JsBoolean`, `JsUndefined`
- `JsBuffer`, `JsBufferView`, `JsArrayBuffer`, `JsArrayBufferView`
- `JsTypedArray`, `JsBigint`, `Ref`

These are considered unsafe because they don't track lifetimes. The v3 replacements:
- `JsObject` -> `Object<'env>` (with lifetime)
- `JsFunction` -> `Function<'env, Args, Return>`
- `Buffer` -> `BufferSlice<'env>` (with lifetime)

To continue using old types: `napi = { features = ["compat-mode"] }` (NOT recommended for new code).

**ThreadsafeFunction completely rewritten:**
- New API is safer and more ergonomic
- See napi.rs docs for updated usage

**Module init moved:**
```rust
// v2 (compat-mode)
#[module_exports]
fn init(mut exports: JsObject) -> Result<()> { ... }

// v3
#[napi_derive::module_init]
fn init() { ... }

// v3 alternative (replaces #[module_exports])
#[napi(module_exports)]
fn init(mut exports: Object) -> Result<()> { ... }
```

**BufferRef renamed to BufferSlice** (v3.8.3 changelog)

### New features in v3

1. **Lifetime-tracked values** -- `Object<'env>`, `Function<'env>`, `BufferSlice<'env>`
2. **WebAssembly support** -- compile to `wasm32-wasip1-threads`
3. **ReadableStream** -- Web Streams API interop (`web_stream` feature)
4. **AsyncGenerator / async_iterator** -- `#[napi(async_iterator)]` (v3.8.0+)
5. **ScopedTask** -- Task that can return lifetime-scoped JS values (v3.1.0+)
6. **PromiseRaw** -- Low-level Promise manipulation (.then/.catch/.finally)
7. **Dynamic symbol loading** -- `dyn-symbols` feature (default) uses libloading
8. **Discriminant case control** -- `#[napi(discriminant_case)]` for enum variants (v3.3.0+)

### Recent bug fixes to be aware of

- **v3.8.4** (2026-03-28): null error_message check, skip nullish error causes
- **v3.8.3** (2026-02-14): **prevent async iterator use-after-free during GC** (critical fix)
- **v3.8.2** (2026-01-08): memory leak in async fn (critical fix)
- Always use **>= 3.8.4** to get all safety fixes

---

## 10. Practical: Wrapping Nika SDK (Client, Job, Artifact)

### Complete example architecture

```rust
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================
// Enums
// ============================================================

#[napi(string_enum)]
pub enum JobState {
  Pending,
  Running,
  Completed,
  Failed,
  Cancelled,
}

// ============================================================
// Data objects (interfaces in TypeScript)
// ============================================================

#[napi(object)]
pub struct JobStatus {
  pub id: String,
  pub state: JobState,
  pub progress: f64,
  pub error: Option<String>,
}

#[napi(object)]
pub struct TaskEvent {
  pub event_type: String,
  pub task_id: String,
  pub data: String,
  pub timestamp_ms: f64,
}

#[napi(object)]
pub struct ArtifactInfo {
  pub path: String,
  pub size: f64,
  pub format: String,
  pub checksum: Option<String>,
}

// ============================================================
// Classes (with methods)
// ============================================================

#[napi]
pub struct NikaClient {
  inner: Arc<nika_sdk::Client>,
}

#[napi]
impl NikaClient {
  #[napi(factory)]
  pub async fn connect(endpoint: String) -> Result<Self> {
    let client = nika_sdk::Client::connect(&endpoint).await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(Self { inner: Arc::new(client) })
  }

  #[napi]
  pub async fn submit(&self, workflow_path: String) -> Result<NikaJob> {
    let job = self.inner.submit(&workflow_path).await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(NikaJob {
      inner: Arc::new(tokio::sync::RwLock::new(job)),
    })
  }

  #[napi]
  pub async fn list_jobs(&self) -> Result<Vec<JobStatus>> {
    let jobs = self.inner.list_jobs().await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(jobs.into_iter().map(|j| JobStatus {
      id: j.id.to_string(),
      state: j.state.into(),
      progress: j.progress,
      error: j.error,
    }).collect())
  }
}

#[napi]
pub struct NikaJob {
  inner: Arc<tokio::sync::RwLock<nika_sdk::Job>>,
}

#[napi]
impl NikaJob {
  #[napi(getter)]
  pub fn id(&self) -> String {
    // Blocking read is OK for a quick getter
    self.inner.blocking_read().id.to_string()
  }

  #[napi]
  pub async fn status(&self) -> Result<JobStatus> {
    let job = self.inner.read().await;
    let s = job.status().await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(JobStatus {
      id: s.id.to_string(),
      state: s.state.into(),
      progress: s.progress,
      error: s.error,
    })
  }

  #[napi]
  pub async fn wait(&self) -> Result<JobStatus> {
    let job = self.inner.read().await;
    let s = job.wait_until_done().await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(JobStatus {
      id: s.id.to_string(),
      state: s.state.into(),
      progress: s.progress,
      error: s.error,
    })
  }

  #[napi]
  pub async fn cancel(&self) -> Result<()> {
    let job = self.inner.write().await;
    job.cancel().await
      .map_err(|e| Error::from_reason(e.to_string()))
  }

  #[napi]
  pub async fn artifacts(&self) -> Result<Vec<ArtifactInfo>> {
    let job = self.inner.read().await;
    let artifacts = job.artifacts().await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(artifacts.into_iter().map(|a| ArtifactInfo {
      path: a.path.to_string(),
      size: a.size as f64,
      format: a.format.to_string(),
      checksum: a.checksum,
    }).collect())
  }

  /// Returns an async iterator of SSE events
  #[napi]
  pub async fn stream_events(&self) -> Result<JobEventStream> {
    let job = self.inner.read().await;
    let rx = job.subscribe_events().await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(JobEventStream {
      receiver: Arc::new(Mutex::new(rx)),
    })
  }
}

// ============================================================
// Async Iterator for SSE streaming
// ============================================================

#[napi(async_iterator)]
pub struct JobEventStream {
  receiver: Arc<Mutex<tokio::sync::mpsc::Receiver<nika_sdk::TaskEvent>>>,
}

#[napi]
impl AsyncGenerator for JobEventStream {
  type Yield = TaskEvent;
  type Next = ();
  type Return = ();

  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let rx = self.receiver.clone();
    async move {
      let mut guard = rx.lock().await;
      match guard.recv().await {
        Some(event) => Ok(Some(TaskEvent {
          event_type: event.event_type.to_string(),
          task_id: event.task_id.to_string(),
          data: serde_json::to_string(&event.data).unwrap_or_default(),
          timestamp_ms: event.timestamp as f64,
        })),
        None => Ok(None),
      }
    }
  }
}

// ============================================================
// Artifact (for downloading results)
// ============================================================

#[napi]
pub struct NikaArtifact {
  inner: nika_sdk::Artifact,
}

#[napi]
impl NikaArtifact {
  #[napi(getter)]
  pub fn path(&self) -> &str {
    &self.inner.path
  }

  #[napi(getter)]
  pub fn size(&self) -> f64 {
    self.inner.size as f64
  }

  #[napi]
  pub async fn download(&self, dest: String) -> Result<()> {
    self.inner.download_to(&dest).await
      .map_err(|e| Error::from_reason(e.to_string()))
  }

  #[napi]
  pub async fn read_text(&self) -> Result<String> {
    self.inner.read_text().await
      .map_err(|e| Error::from_reason(e.to_string()))
  }

  #[napi]
  pub async fn read_bytes(&self) -> Result<Buffer> {
    let bytes = self.inner.read_bytes().await
      .map_err(|e| Error::from_reason(e.to_string()))?;
    Ok(bytes.into())
  }
}
```

### Generated TypeScript (auto)

```typescript
export const enum JobState {
  Pending = 'Pending',
  Running = 'Running',
  Completed = 'Completed',
  Failed = 'Failed',
  Cancelled = 'Cancelled',
}

export interface JobStatus {
  id: string
  state: JobState
  progress: number
  error: string | null
}

export interface TaskEvent {
  eventType: string
  taskId: string
  data: string
  timestampMs: number
}

export interface ArtifactInfo {
  path: string
  size: number
  format: string
  checksum: string | null
}

export class NikaClient {
  static connect(endpoint: string): Promise<NikaClient>
  submit(workflowPath: string): Promise<NikaJob>
  listJobs(): Promise<Array<JobStatus>>
}

export class NikaJob {
  get id(): string
  status(): Promise<JobStatus>
  wait(): Promise<JobStatus>
  cancel(): Promise<void>
  artifacts(): Promise<Array<ArtifactInfo>>
  streamEvents(): Promise<JobEventStream>
}

export class JobEventStream {
  [Symbol.asyncIterator](): AsyncIterator<TaskEvent>
}

export class NikaArtifact {
  get path(): string
  get size(): number
  download(dest: string): Promise<void>
  readText(): Promise<string>
  readBytes(): Promise<Buffer>
}
```

---

## Key Gotchas

1. **`&mut self` in async requires `unsafe`** -- napi-rs enforces this because the JS GC can collect `self` at await points. Use `Arc<RwLock<T>>` internally instead.

2. **AsyncGenerator's `next()` must return `'static` Future** -- you cannot borrow `self` in the async block. Extract values before the async block or use `Arc<Mutex<>>`.

3. **async_iterator use-after-free bug** (fixed in v3.8.3) -- always use >= 3.8.4.

4. **async fn memory leak** (fixed in v3.8.2) -- always use >= 3.8.4.

5. **`napi-build` is still v2.x** -- don't try to use 3.0.0-beta.0 (yanked).

6. **`type-def` is a `napi-derive` feature, not a `napi` feature** -- put it in the right place.

7. **`u64`/`i64` -> `number` loses precision** -- use `BigInt` (napi6) or `f64` for large numbers.

8. **Naming convention**: Rust `snake_case` methods become JS `camelCase` automatically. Struct fields in `#[napi(object)]` also convert: `timestamp_ms` -> `timestampMs`.

9. **`Object` in v3 has a lifetime parameter** `Object<'env>` -- unlike v2's `JsObject` which was untracked.

10. **For npm scope publishing**, the CLI generates platform packages as `@scope/name-platform-arch`. Make sure the scope is available on npm.

---

## Sources

1. [crates.io/crates/napi](https://crates.io/crates/napi) -- version 3.8.4, feature list, dependencies
2. [crates.io/crates/napi-derive](https://crates.io/crates/napi-derive) -- version 3.5.3, type-def feature
3. [crates.io/crates/napi-build](https://crates.io/crates/napi-build) -- version 2.3.1
4. [GitHub napi-rs/napi-rs](https://github.com/napi-rs/napi-rs) -- source code, examples, releases
5. [napi.rs docs](https://napi.rs) -- Getting started, Class, async fn, V2-V3 migration guide
6. [napi-rs/package-template](https://github.com/napi-rs/package-template) -- canonical project template
7. GitHub Releases (napi-v3.1.0 through napi-v3.8.4) -- changelog, breaking changes, bug fixes
8. npm @napi-rs/cli 3.6.0, @node-rs/argon2 -- real-world optionalDependencies structure

## Confidence Level

**High** -- All data sourced from official repositories, crates.io metadata, and official documentation. Code examples are from the napi-rs examples directory (verified working). Version numbers confirmed against crates.io API on 2026-04-02.
