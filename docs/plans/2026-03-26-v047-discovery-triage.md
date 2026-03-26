# v0.47 Discovery Triage — 9 Agent Swarm Results

> 2026-03-26 | 9 agents, 3 swarms | Post ARCH-1/ARCH-2 extraction

## CRITICAL (4 findings — fix immediately)

### C1. LSP write lock held across `.await`
- **File**: `nika-engine/src/lsp/server.rs:297-307`
- **Agent**: Async patterns
- **Issue**: `did_change` acquires write lock on `documents`, then awaits `client.log_message()` — blocks all concurrent LSP handlers
- **Fix**: Drop write guard before awaiting. Check-then-return before lock scope.

### C2. Daemon lock held across 19 IPC calls
- **File**: `nika-engine/src/secrets/daemon.rs:58-105`
- **Agent**: Async patterns
- **Issue**: `tokio::sync::Mutex` held for entire loop of 19 `get_secret().await` calls during boot
- **Fix**: Clone/Arc the client, drop the guard, iterate without lock

### C3. openssl vendored build (116s C compilation)
- **File**: `nika-tui/Cargo.toml:67`
- **Agent**: Build time
- **Issue**: `git2` + vendored openssl = 130s of C compilation for TUI git gutter marks (373 lines)
- **Fix**: Replace with `gix` (pure Rust) or shell out to `git`. Saves ~80-100s clean build.

### C4. image default-features bloat (92s CPU)
- **File**: `nika-media/Cargo.toml:80` (charts-rs)
- **Agent**: Build time
- **Issue**: charts-rs enables image defaults (AVIF encoder = rav1e 29s, EXR 13s, TIFF 8s)
- **Fix**: `charts-rs = { ..., default-features = false, features = ["image-encoder"] }`

## HIGH (10 findings — fix this session or next)

### H1. Runner `select!` lacks `biased` (2 locations)
- **Files**: `runner.rs:2121`, `runner.rs:1337`
- **Fix**: Add `biased;` — one-line fixes each

### H2. Unbounded EventLog Vec
- **File**: `nika-event/src/log.rs:902`
- **Fix**: Add configurable cap with tiered eviction

### H3. reqwest 0.12 + 0.13 duplication
- **Agent**: Dependency graph
- **Fix**: Upgrade to reqwest 0.13 to match rig-core

### H4. Dual TLS backends (aws-lc-sys 69s + ring 3s)
- **Fix**: Align on single crypto backend

### H5. `compute_layers` has ZERO tests
- **File**: `nika-engine/src/dag/flow.rs`
- **Fix**: Add 5 tests (empty, single, diamond, chain, unknown edges)

### H6. VALIDATOR_CACHE hash collision risk
- **File**: `nika-engine/src/runtime/output.rs:28-51`
- **Fix**: Store schema alongside hash for collision guard, or use blake3

### H7. Unbounded cache growth (SCHEMA_CACHE, VALIDATOR_CACHE)
- **Fix**: Add size cap (512 entries) with clear-on-overflow

### H8. TUI `truncate_str` always allocates
- **File**: `nika-tui/src/utils.rs:72`
- **Fix**: Return `Cow<'_, str>` — eliminates 20-40 allocs/frame

### H9. `serde_json::to_string` in TUI render loop
- **File**: `nika-tui/src/views/chat/messages/task_boxes.rs:200,226`
- **Fix**: Cache serialized string in InvokeBox struct

### H10. Runtime ContextAssembled serialization waste
- **File**: `nika-engine/src/runtime/executor/infer.rs:102-113`
- **Fix**: Estimate tokens without serializing to String

## MEDIUM (15+ findings — plan for follow-up sessions)

| ID | Category | Summary |
|----|----------|---------|
| M1 | Build | qrcode-ai-scanner-core → rxing 108s CPU (slim deps) |
| M2 | Build | Remove tracing-subscriber from nika-engine (zero usage) |
| M3 | Build | Feature-gate jsonschema behind structured-output |
| M4 | Build | Feature-gate indicatif+colored behind cli-display |
| M5 | Build | thiserror 1.x + 2.x duplication (migrate to 2.0) |
| M6 | Async | Broadcast channel 512 may lag under heavy for_each |
| M7 | Async | Native model RwLock semantics inconsistency |
| M8 | Perf TUI | format! storm in inline.rs (6-10 allocs per MCP call/frame) |
| M9 | Perf TUI | status_line() format every frame |
| M10 | Perf TUI | dynamic_separator alloc per message |
| M11 | Perf Runtime | is_failed clones full TaskResult for bool check |
| M12 | Perf Runtime | lower_action clones entire analyzed structs |
| M13 | Security | flow.rs in_degree decrement without saturating_sub |
| M14 | Security | SCHEMA_CACHE serves stale data after file modification |
| M15 | Arch | 806/1510 pub items over-exposed (53% should be pub(crate)) |

## LOW / INFO

- Binary size: 557KB embedded data — **keep as-is** (< 3% of binary)
- VecDeque conversions: all correct (verified by security agent)
- Kahn's algorithm: correct across all 3 implementations
- DashMap TOCTOU in VALIDATOR_CACHE: benign (thundering herd on miss)
- Star animation: comprehensive test coverage already
- Profile settings: already good (lto=thin, codegen-units=1, opt-level=1 for tests)

## Execution Priority

**Quick wins (1-line fixes):**
1. H1 — Add `biased;` to 2 select! blocks
2. H5 — Add 5 compute_layers tests
3. C4 — Fix charts-rs default-features

**Next batch (30 min each):**
4. C1 — Fix LSP write lock
5. C2 — Fix daemon lock
6. H8 — truncate_str → Cow
7. H9 — Cache serde_json in InvokeBox

**Larger items (dedicated sessions):**
8. C3 — Replace git2 with gix/git CLI
9. H3+H4 — reqwest 0.13 + TLS alignment
10. ARCH-3 full — NikaError split (90 variants → domain enums)
