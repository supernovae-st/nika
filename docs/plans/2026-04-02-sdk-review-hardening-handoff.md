# v0.61.0 SDK Review Hardening — Handoff Prompt

> Date: 2026-04-02 | Previous commit: aafa0574d | Tests: 9,400+ | Branch: main
> Session focus: Fix remaining code review issues identified by 4 parallel review agents
> (Rust expert, code reviewer, CI reviewer, Context7/best-practices)

## Pre-Session Checklist

```bash
cd tools && cargo test --workspace --lib --exclude nika-py  # Must pass
cargo test -p nika-py --lib --no-default-features            # Must pass (separate: extension-module)
git log --oneline -5                                         # Verify aafa0574d is HEAD
```

---

## WHAT WAS DONE (this session)

### Commits (SDK v0.61.0 — 9 commits)

| Commit | Description |
|--------|-------------|
| `4cffd0a3e` | `nika-sdk` — Rust core (remote + embedded), 45 tests |
| `85907e8eb` | `nika-napi` — Node.js SDK (napi-rs 3.8), 6 tests |
| `a0cd6abe0` | `nika-py` — Python SDK (PyO3 0.24), 6 tests |
| `0b4156ffc` | Research docs (napi-rs + PyO3) |
| `3cd788e85` | Node AsyncGenerator streaming (`for await...of`) |
| `b6076898b` | Python EventStream + clippy fixes |
| `a701f3558` | CI: npm scope `@supernovae-st` + SDK publish jobs |
| `826358d09` | Docs: scope fix in README + CHANGELOG |
| `aafa0574d` | **Code review fixes**: 3 CRITICAL + 5 HIGH + 5 MEDIUM |

### Architecture

```
nika-sdk (Rust core — crates.io)
├── remote: reqwest HTTP + SSE streaming
├── embedded: in-process Runner + event broadcast forwarding
│
├── nika-napi (Node.js — @supernovae-st/nika-sdk on npm)
│   ├── Client, Job, Artifact as napi classes
│   ├── AsyncGenerator for `for await (event of job.events())`
│   └── client.run() one-liner
│
└── nika-py (Python — nika-sdk on PyPI)
    ├── Client, Job, JobInfo, JobResult, ArtifactInfo as pyclasses
    ├── Sync API: submit(), run_sync(), wait()
    ├── EventStream: blocking `for event in job.stream_events()`
    └── py.allow_threads on all blocking calls
```

---

## WHAT WAS FIXED (code review)

All 3 CRITICAL, 5 HIGH, and 5 MEDIUM issues from the 4-agent code review are resolved.

### Resolved

| ID | Fix |
|----|-----|
| C1 | Python EventStream: `__aiter__`/`__anext__` → `__iter__`/`__next__` + `py.allow_threads` around `recv()` |
| C2 | Python `events()`: removed `Python::with_gil` inside `py.allow_threads` — collect Rust data first, convert after |
| C3 | SSE parser: `strip_prefix(' ')` instead of `trim_start_matches(' ')` — per spec, only 1 space stripped |
| H1 | Embedded `cancel()`: `CancellationToken` stored in `JobState`, `cancel()` calls `.cancel()` |
| H2 | Embedded `events()`: real `TaskStart`/`TaskComplete` forwarded via broadcast receiver |
| H3 | CI `runs-on: ${{ matrix.os }}` instead of hardcoded `ubuntu-latest` |
| H4 | CI artifact actions v7/v8 (matching rest of workflow) |
| H5 | CI collect jobs: `needs.*.result == 'success'` instead of unconditional |
| M-format | `ArtifactInfo.format` exposed in both Node + Python bindings |
| M-token | `Client(url, token?)` — token now optional for `--no-auth` servers |
| M-version | `pyproject.toml` uses `dynamic = ["version"]` from Cargo.toml |
| M-timeout | `map_reqwest_error`: path-aware timeout estimate, distinct error for non-connect |
| M-path | Embedded: handle empty parent path for relative workflows |

---

## WHAT REMAINS (for next session)

### SHOULD DO (quality polish — no blockers)

1. **Python `json.dumps` roundtrip** (`nika-py/src/lib.rs:389-395`)
   - Currently: `py.import("json")?.call_method1("dumps", ...)` → `serde_json::from_str`
   - Better: add `pythonize` crate dep, use `depythonize(&dict)` for direct PyDict → serde_json::Value
   - Impact: performance only, functional correctness is fine

2. **Node.js discriminated union types** (`nika-napi`)
   - `NikaEvent` is a flat struct with all optional fields → weak TypeScript types
   - Better: provide handwritten `.d.ts` augmentation with proper `type NikaEvent = StartedEvent | TaskCompleteEvent | ...`
   - Impact: DX for TypeScript users

3. **Python `__eq__` on frozen classes** (`nika-py`)
   - `JobInfo`, `JobResult`, `ArtifactInfo` are `#[pyclass(frozen)]` but lack `__eq__`
   - Users can't `assert result == expected_result` in tests
   - Fix: add `#[pyo3(eq)]` or manual `__eq__` impl

4. **Rust SDK: `JobStatus` enum** (`nika-sdk/src/types.rs:94`)
   - `status: String` → `JobStatus` enum with `#[serde(rename_all = "snake_case")]`
   - Would catch protocol drift at deserialization time

5. **Rust SDK: `SdkError::Cancelled`** (`nika-sdk/src/client.rs:158`)
   - `Job::wait()` maps `Cancelled` event to `SdkError::Engine` — imprecise
   - Add dedicated `SdkError::Cancelled` variant

6. **Rust SDK: unbounded job map in embedded** (`nika-sdk/src/embedded.rs:27`)
   - `HashMap<String, JobState>` never reaps completed jobs
   - Add TTL-based cleanup or max capacity

7. **CI: aarch64-unknown-linux-gnu napi build** (removed from matrix for now)
   - Cross-compiling napi-rs for ARM Linux on x64 needs `cross` or `zig`
   - Currently 4 targets (macOS arm64/x64, Linux x64, Windows x64)
   - To add back: use `napi build --zig` or separate ARM runner

8. **CI: npm version sync** before publish
   - `nika-napi/package.json` version is hardcoded `0.58.1`
   - Release workflow should `npm version "$VERSION"` before `npm publish`
   - Same pattern as the CLI npm-publish job (line 770)

9. **SSE: CRLF handling** (`nika-sdk/src/sse.rs`)
   - `buffer.find("\n\n")` in `remote.rs` misses `\r\n\r\n` from CRLF servers
   - Low risk (nika-serve uses `\n`) but worth fixing for interop

10. **SSE: buffer size limit** (`nika-sdk/src/remote.rs`)
    - SSE buffer grows unbounded on malformed streams
    - Add max buffer (e.g. 1 MiB) with `SdkError::EventParse` on overflow

### NICE TO HAVE (can defer to v0.62)

- Python async API (true coroutines with `pyo3-async-runtimes`)
- Python TypedDict for event dicts
- WASM bindings (`@supernovae-st/nika-sdk-wasm`)
- OpenAPI spec via `utoipa`
- Rust SDK: `Event::Unknown` catch-all for forward-compat
- Rust SDK: `Debug` impl on `Client`, `Job`, `Artifact`
- Rust SDK: `Clone` on `Artifact`
- Rust SDK: `#[must_use]` on `submit()` and `wait()`

---

## KEY FILES

| What | File |
|------|------|
| Rust SDK | `tools/nika-sdk/src/{lib,client,transport,types,error,sse,remote,embedded,mock}.rs` |
| Node SDK | `tools/nika-napi/src/lib.rs`, `package.json`, `build.rs` |
| Python SDK | `tools/nika-py/src/lib.rs`, `pyproject.toml`, `python/nika_sdk/{__init__.py,__init__.pyi,py.typed}` |
| CI | `.github/workflows/release.yml` (jobs: sdk-npm-publish, sdk-npm-collect, sdk-pypi-publish, sdk-pypi-collect) |
| Workspace | `tools/Cargo.toml` (20 members including nika-sdk, nika-napi, nika-py) |

## MANUAL ACTIONS (for Thibaut)

```
1. npm login                              # if not already done
2. Add PYPI_TOKEN to GitHub Secrets       # pypi.org/manage/account/token
3. Verify: npm org ls supernovae-st       # confirm scope access
```

## CONSTRAINTS

1. **AGPL-3.0-or-later** — All crates
2. **cargo test --lib** — Always `--lib` to avoid keychain popups
3. **nika-py tests** — Run with `--no-default-features` (extension-module blocks linking)
4. **Zero backward compat** — SDK is new, no legacy users
5. **npm scope** — `@supernovae-st` (not `@supernovae` or `@nika`)
