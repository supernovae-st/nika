# Crate Dependency Graph

> How Nika's 32 crates relate to each other, what each one owns, and why the boundaries exist.
>
> **Last verified:** 2026-04-13 (post-W1 cleanup)
> **Workspace version:** 0.79.x

## Workspace Overview

Nika is organized as a Cargo workspace containing 32 crates at `tools/`. Every crate shares the workspace version (currently 0.79.x), edition 2021, and the AGPL-3.0-or-later license. The workspace uses the resolver v2 setting for correct feature unification.

The architecture follows a strict **L0 → L5 diamond layering**: each layer depends only on layers below it. L0 is pure data (zero I/O), L5 is the binary entry point, and every side effect (HTTP, FS, exec, LLM, MCP) flows through a kernel trait at L0.5.

```
tools/
├── Cargo.toml              # Workspace root (members, shared deps, profiles)
├── nika/                   # L5  — Binary crate, CLI entry point (~5.5k LOC)
├── nika-engine/            # L2  — Execution engine, Act 2 extraction target (~138k LOC)
├── nika-core/              # L0  — AST + types + catalogs, ZERO I/O (~23k LOC)
├── nika-event/             # L0  — Event sourcing, EventLog + EventEmitter (~4k LOC)
├── nika-kernel/            # L0.5 — 12 trait definitions only, ZERO impls (~3k LOC)
├── nika-kernel-mock/       # L0.5 — Mocks for every kernel trait
│
├── nika-clock/             # L1  — SystemClock implementation
├── nika-fs/                # L1  — TokioFs (FsRead + FsWrite splinter impls)
├── nika-blob/              # L1  — DiskBlobStore (blake3 CAS)
├── nika-http/              # L1  — ReqwestClient (with SSRF defense)
├── nika-exec-runner/       # L1  — TokioShell (kill_on_drop + try_join!)
├── nika-policy/            # L1  — PolicyEnforcer + SSRF helpers
├── nika-extract/           # L1  — Pure 9-mode fetch extraction
├── nika-security/          # L1  — Blocklist, injection detection, env hygiene
├── nika-lsp-core/          # L1  — LSP intelligence (pure functions)
│
├── nika-verb-exec/         # L2  — `exec:` verb behind ShellExecutor trait
├── nika-verb-fetch/        # L2  — `fetch:` verb behind HttpClient trait
├── nika-verb-invoke/       # L2  — `invoke:` verb via BuiltinRouter/McpPool
├── nika-verb-infer/        # L2  — `infer:` verb via Provider trait
├── nika-builtin/           # L2  — 63 builtin tools (37 always-on + 26 opt-in)
├── nika-display/           # L2  — CLI render output (~13k LOC)
├── nika-media/             # L2  — Media processing pipeline (~14k LOC)
├── nika-mcp/               # L2  — MCP client/server (~9k LOC)
├── nika-vault/             # L2  — XChaCha20+Argon2 credential store (~1.2k)
├── nika-storage/           # L2  — Storage abstraction (~3k LOC)
│
├── nika-daemon/            # L3  — Background daemon, cron, cache, IPC (~7k)
│
├── nika-cli/               # L4  — CLI subcommands (~8k LOC)
├── nika-serve/             # L4  — HTTP API + SDK bridge (~7k LOC)
├── nika-lsp/               # L4  — LSP server binary (~3.5k LOC)
├── nika-sdk/               # L4  — Rust SDK + remote transport (~3k LOC)
├── nika-init/              # L4  — Project scaffolding (~5k LOC)
│
└── nika-macros/            # Outside diamond — proc-macro support
```

**TUI was deleted in Month A (W1, 2026-04-13)**. It will be rebuilt clean against the diamond runtime in Act 4 (optional, may never ship).

## Layer responsibilities

### L0 — Pure data (zero I/O, zero async)

`nika-core` and `nika-event` define every type the rest of the workspace consumes. They never touch the filesystem, network, or shell. They never spawn tokio tasks. This makes them trivially testable and infinitely cacheable.

- **`nika-core`** owns: workflow AST (Raw → Analyzed → Lower), 27 provider catalog, 63 builtin catalog, 65 transform catalog, error hierarchy (`NikaError` + 100+ NIKA-XXX codes), util helpers (`redact_value` etc.), `PolicyConfig` + `SecurityPolicyConfig`, trust types.
- **`nika-event`** owns: `EventLog`, `EventKind` enum (40+ variants), `TraceWriter` (NDJSON serializer), `EventEmitter` blanket trait.

### L0.5 — Kernel traits (zero impls, just interfaces)

`nika-kernel` defines exactly twelve trait abstractions for every side effect Nika performs. Code at L2+ depends only on these traits — never on `tokio::*`, `reqwest::*`, or `std::process::*` directly.

The 12 traits: `Provider`, `HttpClient`, `FsRead`, `FsWrite`, `ShellExecutor`, `BlobStore`, `Clock`, `PolicyChecker`, `BuiltinRouter`, `McpPool`, `BindingStore`, `MemoryStore` (the last is forward declared for Phase D Act 3).

`nika-kernel-mock` provides one mock implementation per trait, enabling pure-memory testing across every L2+ crate.

### L1 — Effect crates (one per side-effect type)

Each L1 crate implements exactly one kernel trait against its real-world target. Zero overlap between L1 crates. They are intentionally tiny (most ~500-1.5k LOC) so they can be reviewed in a single AI context window.

| Crate | Implements | Reason for existence |
|-------|-----------|----------------------|
| `nika-clock` | `Clock` | Tokio-backed time ticking (testable via MockClock) |
| `nika-fs` | `FsRead` + `FsWrite` | Tokio FS wrapper with splinter trait separation |
| `nika-blob` | `BlobStore` | Blake3-keyed CAS on disk |
| `nika-http` | `HttpClient` | Reqwest with SSRF interceptor + retry policy |
| `nika-exec-runner` | `ShellExecutor` | Tokio process with `kill_on_drop(true)` + try_join drain |
| `nika-policy` | `PolicyChecker` | Capability + SSRF + path policy enforcement |
| `nika-extract` | (consumed by verb-fetch) | Pure 9-mode HTML/JSON/jsonpath extraction |
| `nika-security` | (used by all verbs) | Command blocklist + injection detection + env hygiene |
| `nika-lsp-core` | (consumed by nika-lsp) | Pure LSP intelligence (no transport) |

### L2 — Domain crates (business logic)

- **Verb crates (`nika-verb-*`)**: One crate per of the 5 sacred verbs. Each consumes kernel traits, never concrete L1 types. Currently extracted: exec ✓, fetch ✓, invoke ✓ (partial), infer ✓ (15% fast path). Agent verb extraction is Act 2 P4.
- **`nika-builtin`**: 63 tools implementing the `BuiltinTool` trait. Consumed by `nika-verb-invoke`.
- **`nika-mcp`**: MCP client (rmcp 1.x) + server (Nika-as-MCP-server). Trust-aware tool routing.
- **`nika-media`**: Media pipeline (import, decode, dimensions, thumbhash, dominant_color, opt-in OCR/PDF/SVG).
- **`nika-display`**: Terminal output rendering, trace formatting, error display.
- **`nika-vault`**: XChaCha20Poly1305 + Argon2i encrypted credential storage.
- **`nika-storage`**: General storage abstraction (used by traces, cache, locks).
- **`nika-engine`**: Monolith (~138k LOC) being progressively dissolved across Act 2 P1-P5. Holds runtime/runner/dispatch + structured output L0 + agent loop. Extraction target: 0 LOC by end of Act 2 P5.

### L3 — Orchestration

- **`nika-daemon`**: Background daemon for Unix systems. Owns cron scheduling, response cache (LLM + HTTP), IPC for credentials, watch mode.

### L4 — Interfaces

- **`nika-cli`**: All CLI subcommand handlers. Wire CLI args → engine calls.
- **`nika-serve`**: HTTP API server (REST + SSE streams). SDK consumers connect here.
- **`nika-lsp`**: Language Server Protocol binary (`nika lsp --stdio`). VS Code extension consumes this.
- **`nika-sdk`**: Rust SDK with remote transport. Mirror of `nika-client` (TypeScript SDK).
- **`nika-init`**: Project scaffolding wizard (`nika init` interactive flow).

### L5 — Binary

- **`nika`**: The composition root. Wires every L4 interface together. Targets <900 LOC by end of Act 2.

### Outside diamond

- **`nika-macros`**: Proc-macro support crate (used by other crates at compile time, doesn't fit the runtime layering).

## Strict downward dependency rule

Every crate may only depend on crates in lower layers. Any upward dependency is a CI-blocking violation. The current state achieves this for all L1+ crates; `nika-engine` is the last remaining monolith with cross-cutting concerns being progressively extracted.

## Why these boundaries exist

1. **AI context window fit**: each crate is small enough (most ≤15k LOC, target ≤14k) for an AI agent to hold its entire source in context. This eliminates hallucination-driven refactor errors. See `~/.claude/projects/.../memory/project_ai_velocity_north_star.md`.

2. **Mock-purity for tests**: every I/O operation behind a kernel trait means tests run in pure memory using `nika-kernel-mock`. No real subprocess, no real network, no real disk required for unit tests.

3. **Independent evolution**: extracting a verb crate (e.g., `nika-verb-infer`) lets us version, release, and iterate on it independently of the monolith. Public crates.io packages become possible.

4. **Bus-factor mitigation**: 8 satellite crates from the upcoming `nika-memory` constellation are designed to be standalone-publishable on crates.io. If the project pauses, the ecosystem retains the satellites.

## Diamond progress (post-W2, 2026-04-13)

- 32 crates total (target post-Act 2: 28-30 after agent extraction + engine dissolution)
- 4/5 verbs extracted (exec ✓, fetch ✓, invoke ✓ partial, infer ✓ partial, agent ✗)
- `nika-engine` god-crate: still ~138k LOC, Act 2 target = 0 LOC
- Largest non-engine crate: `nika-media` (~14k LOC, within target)
- Strict downward deps: 95 % compliant (engine still has cross-cuts)

## See also

- `docs/concepts/ARCHITECTURE.md` — overall architecture explanation
- `docs/roadmap/constellation-session12-rework/` — Act 2 extraction plan
- `~/.claude/projects/.../memory/PLAN.md` — 4-act strategy with diamond gates
- `~/.claude/projects/.../memory/project_ai_velocity_north_star.md` — why ≤14k crate size
