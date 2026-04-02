# PyO3 Research Report: Building a Python SDK from Rust

> Date: 2026-04-02
> Target: Wrapping Nika Rust SDK (Client, Job, Artifact) for Python
> PyO3 version: **0.28.2** (latest stable, 2026-02-18)
> Maturin version: **1.12.6** (latest stable)
> Sources: PyO3 GitHub main branch, crates.io, official guide

---

## 1. Latest Stable Version

**PyO3 0.28.2** (released 2026-02-18). NOT 0.24 as initially assumed.

Version timeline:
- 0.24.0 -- 2025-03-09
- 0.25.0 -- 2025-05-14
- 0.26.0 -- 2025-08-29
- 0.27.0 -- 2025-10-19
- 0.28.0 -- 2026-02-01 (MSRV: Rust 1.83)
- 0.28.2 -- 2026-02-18 (latest)

Each minor version has substantial breaking changes. The migration from 0.23
to 0.28 is significant -- see section 10.

---

## 2. Project Setup with Maturin

### Cargo.toml

```toml
[package]
name = "nika-python"
version = "0.1.0"
edition = "2021"

[lib]
name = "nika_python"
crate-type = ["cdylib"]

[dependencies]
pyo3 = { version = "0.28", features = [
  "abi3-py39",            # Universal wheel, min Python 3.9
  "experimental-async",   # Native async fn support
  "experimental-inspect", # .pyi stub generation
] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
# Your internal SDK crate:
nika-sdk = { path = "../nika-sdk" }
```

### pyproject.toml

```toml
[build-system]
requires = ["maturin>=1.12,<2"]
build-backend = "maturin"

[project]
name = "nika"
description = "Nika workflow engine - Python SDK"
requires-python = ">=3.9"
license = { text = "AGPL-3.0-or-later" }
classifiers = [
  "Programming Language :: Rust",
  "Programming Language :: Python :: Implementation :: CPython",
  "Programming Language :: Python :: 3.9",
  "Programming Language :: Python :: 3.10",
  "Programming Language :: Python :: 3.11",
  "Programming Language :: Python :: 3.12",
  "Programming Language :: Python :: 3.13",
  "Programming Language :: Python :: 3.14",
  "Typing :: Typed",
]
dynamic = ["version"]

[tool.maturin]
features = ["pyo3/abi3-py39"]
python-source = "python"
module-name = "nika._nika"
```

### Project layout (mixed Rust/Python)

```
nika-python/
+-- Cargo.toml
+-- pyproject.toml
+-- python/
|   +-- nika/
|       +-- __init__.py       # Re-exports from _nika
|       +-- _nika.pyi         # Type stubs for Rust module
|       +-- py.typed          # PEP 561 marker (empty file)
+-- src/
    +-- lib.rs                # #[pymodule]
    +-- client.rs             # Client pyclass
    +-- job.rs                # Job pyclass
    +-- artifact.rs           # Artifact pyclass
    +-- error.rs              # Exception hierarchy
    +-- runtime.rs            # Tokio runtime management
```

### python/nika/__init__.py

```python
from nika._nika import (
    Client,
    Job,
    Artifact,
    NikaError,
    NikaConnectionError,
    NikaTimeoutError,
    NikaValidationError,
    EventStream,
)

__all__ = [
    "Client",
    "Job",
    "Artifact",
    "NikaError",
    "NikaConnectionError",
    "NikaTimeoutError",
    "NikaValidationError",
    "EventStream",
]
```

---

## 3. Native Async in PyO3 0.22+ (No pyo3-asyncio Needed)

Since PyO3 0.22, native `async fn` is supported directly in `#[pyfunction]`
and `#[pymethods]` via the `experimental-async` feature flag. The old
`pyo3-asyncio` crate is **no longer needed**.

### How it works

- `async fn` in `#[pymethods]` returns a PyO3 `Coroutine` that implements
  the Python coroutine protocol
- Each `coroutine.send()` call maps to a `Future::poll()` call
- The future must be `Send + 'static`
- Awaitable only in `asyncio` context (other Python runtimes not yet supported)

### Basic async method

```rust
use pyo3::prelude::*;

#[pyclass]
struct Client {
    inner: nika_sdk::Client,
}

#[pymethods]
impl Client {
    /// Submit a workflow and return a Job.
    /// In Python: `job = await client.submit("workflow.nika.yaml")`
    async fn submit(&self, workflow_path: String) -> PyResult<Job> {
        // self is borrowed for the duration of the future
        // Use &self (not &mut self) to avoid borrow conflicts
        let inner_job = self.inner
            .submit(&workflow_path)
            .await
            .map_err(|e| NikaError::new_err(e.to_string()))?;
        Ok(Job { inner: inner_job })
    }

    /// Wait for job completion with timeout.
    async fn wait(&self, job_id: String, timeout_secs: Option<f64>) -> PyResult<Job> {
        let timeout = timeout_secs.map(std::time::Duration::from_secs_f64);
        let result = self.inner
            .wait(&job_id, timeout)
            .await
            .map_err(|e| NikaError::new_err(e.to_string()))?;
        Ok(Job { inner: result })
    }
}
```

### Cancellation support

```rust
use pyo3::prelude::*;
use pyo3::coroutine::CancelHandle;
use futures::FutureExt;

#[pymethods]
impl Client {
    /// Cancellable long-running operation.
    /// Python: `task.cancel()` will trigger the cancel handle.
    async fn run_workflow(
        &self,
        path: String,
        #[pyo3(cancel_handle)] mut cancel: CancelHandle,
    ) -> PyResult<Job> {
        futures::select! {
            result = self.inner.run(&path).fuse() => {
                result.map(|j| Job { inner: j })
                    .map_err(|e| NikaError::new_err(e.to_string()))
            }
            _ = cancel.cancelled().fuse() => {
                Err(pyo3::exceptions::PyRuntimeError::new_err("Operation cancelled"))
            }
        }
    }
}
```

### Releasing the GIL during .await (critical for performance)

By default, the GIL is NOT released during `.await`. You MUST use the
`AllowThreads` wrapper to release it, otherwise Python is blocked during
all Rust I/O:

```rust
use std::future::Future;
use std::pin::{Pin, pin};
use std::task::{Context, Poll};
use pyo3::prelude::*;

/// Wrapper that releases the GIL while polling a Rust future.
struct AllowThreads<F>(F);

impl<F> Future for AllowThreads<F>
where
    F: Future + Unpin + Send,
    F::Output: Send,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let waker = cx.waker();
        Python::attach(|py| {
            py.detach(|| pin!(&mut self.0).poll(&mut Context::from_waker(waker)))
        })
    }
}

// Usage in async methods:
#[pymethods]
impl Client {
    async fn submit(&self, path: String) -> PyResult<Job> {
        let client = self.inner.clone(); // Clone to avoid self borrow issues
        let result = AllowThreads(async move {
            client.submit(&path).await
        }).await;
        result.map(|j| Job { inner: j })
            .map_err(|e| NikaError::new_err(e.to_string()))
    }
}
```

### Key constraints

1. **`Send + 'static`**: The future and all captured values must be
   `Send + 'static`. You cannot capture `Bound<'py, T>` or `&'py PyAny`.
2. **Prefer `&self` over `&mut self`**: `&mut self` borrows exclusively for
   the entire future lifetime, blocking other methods. Use interior mutability
   (`Arc<Mutex<T>>` or `tokio::sync::Mutex<T>`) instead.
3. **Method receivers are an exception**: `&self` / `&mut self` can be used
   even though they are not `'static`, but the class is borrowed until the
   future completes.

---

## 4. Async Generators (__aiter__ / __anext__) for Streaming Events

PyO3 does NOT have built-in async generator support. You must implement
`__aiter__` and `__anext__` manually using a channel-based pattern:

```rust
use pyo3::prelude::*;
use pyo3::exceptions::PyStopAsyncIteration;
use tokio::sync::mpsc;

/// A streaming event from a running workflow.
#[pyclass]
#[derive(Clone)]
struct Event {
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    task_id: String,
    #[pyo3(get)]
    data: String,
}

/// Async iterator over workflow events.
/// Python usage:
///   async for event in client.stream("job-123"):
///       print(event.kind, event.task_id)
#[pyclass]
struct EventStream {
    receiver: Arc<tokio::sync::Mutex<mpsc::Receiver<nika_sdk::Event>>>,
}

use std::sync::Arc;

#[pymethods]
impl EventStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    /// Each call to __anext__ returns a coroutine that resolves to the next event.
    fn __anext__(&self) -> PyResult<Option<Event>> {
        // For sync version -- see async version below
        Err(PyStopAsyncIteration::new_err(()))
    }
}
```

### Full async __anext__ implementation

The trick is to return a Python-awaitable coroutine from `__anext__`.
Since `__anext__` itself cannot be `async fn` directly in all PyO3 versions,
use the approach of making it return a coroutine:

```rust
use pyo3::prelude::*;
use pyo3::exceptions::PyStopAsyncIteration;
use std::sync::Arc;
use tokio::sync::mpsc;

#[pyclass]
struct EventStream {
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<nika_sdk::Event>>>,
}

#[pymethods]
impl EventStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    // With experimental-async, __anext__ can be async:
    async fn __anext__(&self) -> PyResult<Event> {
        let mut rx = self.rx.lock().await;
        match rx.recv().await {
            Some(event) => Ok(Event {
                kind: event.kind.to_string(),
                task_id: event.task_id.clone(),
                data: serde_json::to_string(&event.data).unwrap_or_default(),
            }),
            None => Err(PyStopAsyncIteration::new_err(())),
        }
    }
}

// On the Client side:
#[pymethods]
impl Client {
    /// Stream events for a running job.
    /// Returns an async iterator.
    fn stream(&self, job_id: String) -> PyResult<EventStream> {
        let (tx, rx) = mpsc::channel(256);

        // Spawn the event forwarding on the Tokio runtime
        let client = self.inner.clone();
        get_runtime().spawn(async move {
            let mut stream = client.events(&job_id).await.unwrap();
            while let Some(event) = stream.next().await {
                if tx.send(event).await.is_err() {
                    break; // receiver dropped
                }
            }
        });

        Ok(EventStream {
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
        })
    }
}
```

### Python usage

```python
import asyncio
from nika import Client

async def main():
    client = Client("http://localhost:3000")
    job = await client.submit("workflow.nika.yaml")

    # Async iteration over events
    async for event in client.stream(job.id):
        print(f"[{event.kind}] {event.task_id}: {event.data}")

asyncio.run(main())
```

---

## 5. Custom Python Exception Hierarchies

### Using create_exception! macro

```rust
use pyo3::prelude::*;
use pyo3::create_exception;
use pyo3::exceptions::PyException;

// Base exception
create_exception!(nika, NikaError, PyException);

// Hierarchy under NikaError
create_exception!(nika, NikaConnectionError, NikaError);
create_exception!(nika, NikaTimeoutError, NikaError);
create_exception!(nika, NikaValidationError, NikaError);
create_exception!(nika, NikaAuthError, NikaError);
create_exception!(nika, NikaWorkflowError, NikaError);

// Register in the module
#[pymodule]
mod _nika {
    use pyo3::prelude::*;

    #[pymodule_export]
    use super::NikaError;
    #[pymodule_export]
    use super::NikaConnectionError;
    #[pymodule_export]
    use super::NikaTimeoutError;
    #[pymodule_export]
    use super::NikaValidationError;
    #[pymodule_export]
    use super::NikaAuthError;
    #[pymodule_export]
    use super::NikaWorkflowError;

    // ... classes, functions ...
}
```

### Rich exceptions with data fields (requires Python 3.12+ with abi3)

```rust
use pyo3::prelude::*;
use pyo3::exceptions::PyException;

// NOTE: subclassing native types (including PyException) with abi3
// requires Python 3.12+. On older Python, use create_exception! instead.
#[cfg(any(not(Py_LIMITED_API), Py_3_12))]
#[pyclass(extends=PyException)]
struct NikaDetailedError {
    #[pyo3(get)]
    code: String,

    #[pyo3(get)]
    message: String,

    #[pyo3(get)]
    task_id: Option<String>,
}

#[cfg(any(not(Py_LIMITED_API), Py_3_12))]
#[pymethods]
impl NikaDetailedError {
    #[new]
    fn new(code: String, message: String, task_id: Option<String>) -> Self {
        Self { code, message, task_id }
    }
}
```

### Converting Rust errors to Python exceptions

```rust
use pyo3::prelude::*;

// Blanket conversion from your SDK error type
impl From<nika_sdk::Error> for PyErr {
    fn from(err: nika_sdk::Error) -> PyErr {
        match err {
            nika_sdk::Error::Connection(msg) => NikaConnectionError::new_err(msg),
            nika_sdk::Error::Timeout(msg) => NikaTimeoutError::new_err(msg),
            nika_sdk::Error::Validation(msg) => NikaValidationError::new_err(msg),
            nika_sdk::Error::Auth(msg) => NikaAuthError::new_err(msg),
            _ => NikaError::new_err(err.to_string()),
        }
    }
}

// Then in methods, just use ?:
#[pymethods]
impl Client {
    async fn submit(&self, path: String) -> PyResult<Job> {
        let job = self.inner.submit(&path).await?; // auto-converts
        Ok(Job { inner: job })
    }
}
```

### Python-side usage

```python
from nika import Client, NikaError, NikaTimeoutError

try:
    job = await client.submit("bad.nika.yaml")
except NikaTimeoutError as e:
    print(f"Timed out: {e}")
except NikaError as e:
    print(f"Nika error: {e}")  # catches all Nika errors
```

---

## 6. Managing the Tokio Runtime (OnceLock Pattern)

Your Rust SDK uses async/await with Tokio. PyO3's `async fn` support
generates Python coroutines but does NOT provide a Tokio runtime. You must
create and manage one yourself.

### The OnceLock<Runtime> pattern

```rust
use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Global Tokio runtime, lazily initialized.
/// OnceLock ensures thread-safe single initialization.
fn get_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("nika-python-worker")
            .build()
            .expect("Failed to create Tokio runtime")
    })
}
```

### Using the runtime in sync methods

For methods that need to call async Rust code but are exposed as sync
Python functions:

```rust
#[pymethods]
impl Client {
    /// Synchronous version -- blocks until complete.
    fn submit_sync(&self, path: String) -> PyResult<Job> {
        // Block on the Tokio runtime, releasing the GIL while waiting
        Python::attach(|py| {
            py.detach(|| {
                get_runtime().block_on(async {
                    self.inner.submit(&path).await
                        .map(|j| Job { inner: j })
                        .map_err(|e| NikaError::new_err(e.to_string()))
                })
            })
        })
    }
}
```

### Using the runtime with async methods

For `async fn` in `#[pymethods]`, the future needs to be driven by a
runtime. The recommended approach is to spawn on Tokio and bridge:

```rust
#[pymethods]
impl Client {
    async fn submit(&self, path: String) -> PyResult<Job> {
        let client = self.inner.clone();
        // The future runs on Tokio's thread pool
        let handle = get_runtime().spawn(async move {
            client.submit(&path).await
        });
        let result = AllowThreads(async {
            handle.await.map_err(|e| NikaError::new_err(e.to_string()))?
        }).await;
        result.map(|j| Job { inner: j })
            .map_err(|e| NikaError::new_err(e.to_string()))
    }
}
```

### Important: Python 0.26+ renamed `with_gil` to `attach`

In PyO3 0.26+:
- `Python::with_gil(|py| ...)` --> `Python::attach(|py| ...)`
- `py.allow_threads(|| ...)` --> `py.detach(|| ...)`
- `prepare_freethreaded_python()` --> `Python::initialize()`

---

## 7. The abi3-py39 Feature for Universal Wheels

### What it does

The `abi3` (stable ABI) feature compiles your extension against Python's
limited/stable API. A single compiled wheel works across ALL Python versions
from your minimum (3.9) through the latest (3.14+).

### Without abi3

You must build separate wheels:
```
nika-0.1.0-cp39-cp39-manylinux_x86_64.whl
nika-0.1.0-cp310-cp310-manylinux_x86_64.whl
nika-0.1.0-cp311-cp311-manylinux_x86_64.whl
nika-0.1.0-cp312-cp312-manylinux_x86_64.whl
nika-0.1.0-cp313-cp313-manylinux_x86_64.whl
nika-0.1.0-cp314-cp314-manylinux_x86_64.whl
```
= 6 wheels per platform per release.

### With abi3-py39

One wheel per platform:
```
nika-0.1.0-cp39-abi3-manylinux_x86_64.whl
nika-0.1.0-cp39-abi3-macosx_11_0_arm64.whl
nika-0.1.0-cp39-abi3-win_amd64.whl
```
= 3 wheels total for all Python 3.9+ on 3 platforms.

### Cargo.toml

```toml
[dependencies]
pyo3 = { version = "0.28", features = ["abi3-py39"] }
```

Available features: `abi3-py39`, `abi3-py310`, `abi3-py311`, `abi3-py312`,
`abi3-py313`.

### Limitations with abi3

- Subclassing native Python types (dict, list, **exceptions via #[pyclass(extends=PyException)]**) requires Python 3.12+ (newly enabled in PyO3 0.28.0)
- Some FFI functions are not available in the limited API
- `#[pyclass]` types cannot use `__dict__` or `__weakref__` slots on older Python
- datetime types now work with abi3 since PyO3 0.25.0

### Recommendation for the Nika SDK

Use `abi3-py39` and prefer `create_exception!` over `#[pyclass(extends=PyException)]`
for exception hierarchies, since `create_exception!` works on all Python versions
with abi3. If you need rich exception data fields, add them via `args` tuple
rather than custom pyclass fields.

---

## 8. Type Stubs (.pyi) and py.typed (PEP 561)

### Manual stubs (recommended for now)

Create `python/nika/_nika.pyi`:

```python
"""Nika Python SDK - Rust native bindings."""

from typing import AsyncIterator, Optional

class NikaError(Exception): ...
class NikaConnectionError(NikaError): ...
class NikaTimeoutError(NikaError): ...
class NikaValidationError(NikaError): ...

class Event:
    kind: str
    task_id: str
    data: str

class EventStream(AsyncIterator[Event]):
    def __aiter__(self) -> "EventStream": ...
    async def __anext__(self) -> Event: ...

class Artifact:
    @property
    def path(self) -> str: ...
    @property
    def format(self) -> str: ...
    @property
    def size(self) -> int: ...
    async def read(self) -> bytes: ...
    async def save(self, path: str) -> None: ...

class Job:
    @property
    def id(self) -> str: ...
    @property
    def status(self) -> str: ...
    @property
    def workflow(self) -> str: ...
    async def wait(self, timeout: Optional[float] = None) -> "Job": ...
    async def cancel(self) -> None: ...
    async def artifacts(self) -> list[Artifact]: ...
    def stream(self) -> EventStream: ...

class Client:
    def __init__(self, base_url: str, *, api_key: Optional[str] = None) -> None: ...
    async def submit(self, workflow_path: str, **inputs: str) -> Job: ...
    async def get_job(self, job_id: str) -> Job: ...
    async def list_jobs(self) -> list[Job]: ...
    def stream(self, job_id: str) -> EventStream: ...
```

### py.typed marker

Create an empty file at `python/nika/py.typed` (PEP 561 marker). This tells
type checkers (mypy, pyright) that the package ships inline types.

```bash
touch python/nika/py.typed
```

### Auto-generated stubs (experimental-inspect feature)

PyO3 0.25+ added `experimental-inspect` which can auto-generate `.pyi` stubs
at build time. In 0.28, this feature:
- Emits base classes
- Emits `@typing.final` on final classes
- Generates nested classes for complex enums
- Emits `async` keyword for async functions
- Fills annotations for all natively supported PyO3 types
- Uses `_typeshed.Incomplete` instead of `typing.Any` for incomplete types

To enable:
```toml
# Cargo.toml
pyo3 = { version = "0.28", features = ["experimental-inspect"] }
```

Then use `maturin`'s stub generation (if supported) or the
`pyo3-introspection` crate (published since PyO3 0.26) to extract stubs
from compiled binaries. Manual stubs are still recommended for production
as the auto-generation is experimental.

---

## 9. Maturin Publishing (sdist + wheels)

### Development workflow

```bash
# Create venv and install in development mode
python -m venv .venv
source .venv/bin/activate
maturin develop               # Build + install in current venv
maturin develop --release     # Optimized build

# Run tests
pytest tests/
```

### Building wheels

```bash
# Build wheel for current platform
maturin build --release

# Build sdist (source distribution)
maturin build --release --sdist

# Build manylinux wheel (Linux, uses Docker)
maturin build --release --manylinux auto

# Build universal2 wheel (macOS, both x86_64 + arm64)
maturin build --release --target universal2-apple-darwin
```

### Publishing to PyPI

```bash
# Publish to PyPI (builds + uploads)
maturin publish --username __token__ --password $PYPI_TOKEN

# Publish to TestPyPI first
maturin publish --repository testpypi
```

### CI matrix (GitHub Actions)

```yaml
name: Release
on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
          - os: macos-latest
            target: x86_64-apple-darwin
          - os: macos-latest
            target: aarch64-apple-darwin
          - os: windows-latest
            target: x86_64-pc-windows-msvc
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: PyO3/maturin-action@v1
        with:
          target: ${{ matrix.target }}
          args: --release --out dist
          manylinux: auto
      - uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.target }}
          path: dist

  publish:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          merge-multiple: true
          path: dist
      - uses: PyO3/maturin-action@v1
        with:
          command: upload
          args: --non-interactive dist/*
```

### Output structure

```
dist/
+-- nika-0.1.0.tar.gz                            # sdist
+-- nika-0.1.0-cp39-abi3-manylinux_2_17_x86_64.whl
+-- nika-0.1.0-cp39-abi3-manylinux_2_17_aarch64.whl
+-- nika-0.1.0-cp39-abi3-macosx_10_12_x86_64.whl
+-- nika-0.1.0-cp39-abi3-macosx_11_0_arm64.whl
+-- nika-0.1.0-cp39-abi3-win_amd64.whl
```

---

## 10. Breaking Changes: PyO3 0.23 through 0.28

### 0.23 to 0.24 (2025-03-09)

- **Removed `Deref` for `PyAny`** and other native types
- **Removed implicit default for trailing optional args** (must use `#[pyo3(signature)]`)
- **Removed deprecated implicit eq fallback** for simple enums
- **`#[pyo3(from_py_with)]` now takes a path**, not a string literal
- `PathBuf`/`Path` now convert to Python `pathlib.Path` instead of `PyString`
- `PyAnyMethods::call` now requires `PyCallArgs` for positional args

### 0.24 to 0.25 (2025-05-14)

- **Removed `AsPyPointer` trait** entirely
- **Removed deprecated `IntoPy` and `ToPyObject` traits** -- use `IntoPyObject` instead
- Added initial `.pyi` stub generation (`experimental-inspect`)
- Added `#[pyclass(generic)]` for runtime generic typing
- `datetime` types now work with `abi3` feature

### 0.25 to 0.26 (2025-08-29)

- **`Python::with_gil` renamed to `Python::attach`** (major rename)
- **`Python::allow_threads` renamed to `Python::detach`**
- **`prepare_freethreaded_python` renamed to `Python::initialize`**
- **`GILOnceCell` deprecated** in favor of `PyOnceLock`
- **`GILProtected` deprecated** -- use `std::sync::Mutex` with `MutexExt`
- **`PyObject` type alias deprecated** -- use `Py<PyAny>` directly
- `Bound::cast` family replaces `PyAnyMethods::downcast`
- `PYO3_BUILD_EXTENSION_MODULE` env var replaces `extension-module` feature

### 0.26 to 0.27 (2025-10-19)

- **`FromPyObject` reworked** with second lifetime `'a` + `Error` associated type
- **`extract_bound` replaced with `extract`** taking `Borrowed<'a, 'py, PyAny>`
- `downcast()` / `DowncastError` replaced with `cast()` / `CastError`
- `PyTypeCheck` is now an `unsafe trait`
- Dropped support for PyPy 3.9 and 3.10

### 0.27 to 0.28 (2026-02-01) -- CURRENT

- **MSRV bumped to Rust 1.83**
- **Free-threaded Python support is now opt-out** (`#[pymodule(gil_used = true)]`
  to opt out, default is `gil_used = false`)
- **Deprecated automatic `FromPyObject` for `#[pyclass] + Clone`** -- must use
  `#[pyclass(from_py_object)]` to opt in or `#[pyclass(skip_from_py_object)]`
- **Multi-phase initialization** (PEP 489) for `#[pymodule]`
- **`__init__` support** in `#[pymethods]` (finally!)
- `#[pyclass(new = "from_fields")]` option added
- `async` pymethods now borrow `self` only during awaiting, not entire call
- Subclassing native types (dict, list, exceptions) now works with abi3 on
  Python 3.12+
- `#[new]` can return arbitrary Python objects
- `#[deleter]` attribute for property deleters
- `py_format!` macro and `PyString::from_fmt`

---

## Complete Example: src/lib.rs

```rust
use pyo3::prelude::*;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;

// --- Runtime ---

fn get_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

// --- Exceptions ---

create_exception!(_nika, NikaError, PyException);
create_exception!(_nika, NikaConnectionError, NikaError);
create_exception!(_nika, NikaTimeoutError, NikaError);
create_exception!(_nika, NikaValidationError, NikaError);

// --- Classes ---

#[pyclass]
struct Client {
    inner: Arc<nika_sdk::Client>,
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (base_url, *, api_key=None))]
    fn new(base_url: String, api_key: Option<String>) -> PyResult<Self> {
        let inner = nika_sdk::Client::new(&base_url, api_key.as_deref())
            .map_err(|e| NikaConnectionError::new_err(e.to_string()))?;
        Ok(Self { inner: Arc::new(inner) })
    }

    async fn submit(&self, workflow_path: String) -> PyResult<Job> {
        let client = self.inner.clone();
        let result = get_runtime()
            .spawn(async move { client.submit(&workflow_path).await })
            .await
            .map_err(|e| NikaError::new_err(e.to_string()))?
            .map_err(|e| NikaError::new_err(e.to_string()))?;
        Ok(Job { inner: Arc::new(result) })
    }

    fn stream(&self, job_id: String) -> PyResult<EventStream> {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        let client = self.inner.clone();
        get_runtime().spawn(async move {
            if let Ok(mut stream) = client.events(&job_id).await {
                while let Some(event) = stream.next().await {
                    if tx.send(event).await.is_err() { break; }
                }
            }
        });
        Ok(EventStream {
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
        })
    }
}

#[pyclass]
struct Job {
    inner: Arc<nika_sdk::Job>,
}

#[pymethods]
impl Job {
    #[getter]
    fn id(&self) -> &str { &self.inner.id }

    #[getter]
    fn status(&self) -> &str { self.inner.status.as_str() }

    async fn wait(&self, timeout: Option<f64>) -> PyResult<Job> {
        let job = self.inner.clone();
        let t = timeout.map(std::time::Duration::from_secs_f64);
        let result = get_runtime()
            .spawn(async move { job.wait(t).await })
            .await
            .map_err(|e| NikaError::new_err(e.to_string()))?
            .map_err(|e| NikaTimeoutError::new_err(e.to_string()))?;
        Ok(Job { inner: Arc::new(result) })
    }
}

#[pyclass]
#[derive(Clone)]
struct Event {
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    task_id: String,
    #[pyo3(get)]
    data: String,
}

#[pyclass]
struct EventStream {
    rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<nika_sdk::Event>>>,
}

#[pymethods]
impl EventStream {
    fn __aiter__(slf: Py<Self>) -> Py<Self> { slf }

    async fn __anext__(&self) -> PyResult<Event> {
        let mut rx = self.rx.lock().await;
        match rx.recv().await {
            Some(e) => Ok(Event {
                kind: e.kind.to_string(),
                task_id: e.task_id,
                data: serde_json::to_string(&e.data).unwrap_or_default(),
            }),
            None => Err(pyo3::exceptions::PyStopAsyncIteration::new_err(())),
        }
    }
}

#[pyclass]
struct Artifact {
    inner: Arc<nika_sdk::Artifact>,
}

#[pymethods]
impl Artifact {
    #[getter]
    fn path(&self) -> &str { &self.inner.path }

    #[getter]
    fn format(&self) -> &str { &self.inner.format }

    #[getter]
    fn size(&self) -> u64 { self.inner.size }

    async fn read(&self) -> PyResult<Vec<u8>> {
        let artifact = self.inner.clone();
        get_runtime()
            .spawn(async move { artifact.read().await })
            .await
            .map_err(|e| NikaError::new_err(e.to_string()))?
            .map_err(|e| NikaError::new_err(e.to_string()))
    }
}

// --- Module ---

#[pymodule]
mod _nika {
    #[pymodule_export]
    use super::NikaError;
    #[pymodule_export]
    use super::NikaConnectionError;
    #[pymodule_export]
    use super::NikaTimeoutError;
    #[pymodule_export]
    use super::NikaValidationError;
    #[pymodule_export]
    use super::Client;
    #[pymodule_export]
    use super::Job;
    #[pymodule_export]
    use super::Event;
    #[pymodule_export]
    use super::EventStream;
    #[pymodule_export]
    use super::Artifact;
}
```

---

## Key Decisions Summary

| Decision | Recommendation | Why |
|----------|---------------|-----|
| PyO3 version | 0.28.2 | Latest stable, `__init__` support, abi3+exceptions on 3.12+ |
| Min Python | 3.9 | Broadest compatibility, still supported |
| abi3 | Yes (`abi3-py39`) | 1 wheel per platform instead of 6 |
| Async | `experimental-async` | Native, no pyo3-asyncio dependency |
| Runtime | `OnceLock<Runtime>` | Thread-safe singleton, lazy init |
| Exceptions | `create_exception!` | Works everywhere with abi3 (no 3.12 restriction) |
| Stubs | Manual `.pyi` | `experimental-inspect` still experimental |
| Builder | maturin 1.12 | Best-in-class for pure Rust -> Python |
| GIL release | `AllowThreads` wrapper | Critical for concurrent Python + Rust I/O |

---

## Sources

1. PyO3 CHANGELOG.md (main branch) -- version history, breaking changes
2. PyO3 guide: async-await.md -- native async documentation
3. PyO3 guide: exception.md -- create_exception!, rich exceptions
4. PyO3 guide: building-and-distribution.md -- abi3, wheels, manual builds
5. PyO3 guide: module.md -- #[pymodule] declarative syntax
6. PyO3 migration.md -- 0.24->0.25->0.26->0.27->0.28 migration steps
7. Maturin guide: project_layout.md -- mixed Rust/Python layout, py.typed
8. crates.io/crates/pyo3 -- version 0.28.2 confirmed
9. crates.io/crates/maturin -- version 1.12.6 confirmed

## Methodology

- Tools: crates.io API, GitHub raw content, PyO3 official guide (main branch)
- Pages analyzed: 8 documentation pages + full CHANGELOG (500+ entries)
- Confidence: HIGH -- all information from primary sources (PyO3 repo)
