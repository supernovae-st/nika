# Crate Layer Registry

**Status**: planned for v0.81 enforcement. The layer discipline described
here is the ground truth that `[workspace.metadata.diamond.layers]` and
`scripts/ci/check-layering.sh` are built to enforce.

## Why this document exists

40-42 crates without a layer contract converge on a kitchen-sink dependency
graph: everything imports everything, every crate pulls in tokio, every
change ripples through the workspace. Diamond discipline defines six
layers (L0 → L5, with an L0.5 for trait-only crates) and makes upward
dependencies a CI hard failure.

The goal is **mechanical sorting**: every new crate has an unambiguous
home, and "which crate should this go in" becomes a look-up instead of a
debate.

### Mechanical sort test (< 10 seconds)

Given a crate `nika-<role>`, ask these questions in order:

1. Does it produce the `nika` binary? → **L5**
2. Does it expose a transport / UI surface (CLI, HTTP, MCP, LSP, SDK)? → **L4**
3. Does it enforce runtime policy, sandboxing, or orchestration? → **L3**
4. Does it implement a verb or domain workflow? → **L2**
5. Is it a primitive with I/O (fs, net, exec, env)? → **L1**
6. Otherwise → **L0**

## Architecture map

```
╔════════════════════════════════════════════════════════════════════════════╗
║                              crates/                                       ║
╚════════════════════════════════════════════════════════════════════════════╝

╭─ L0  (pure, sync, zero I/O) ──────────────────────────────────────────────╮
│ nika-error                [axes: none]      Arc<str>, zero-heap           │
│ nika-catalog              [axes: none]      Cow<'static, str>, phf lookup │
│ nika-catalog-codegen      [build-dep]       Extracted build.rs logic      │
│ nika-pck-manifest         [axes: none]      TOML types only               │
│ nika-event                [axes: none]      Event envelope types          │
│ nika-stdx                 [axes: none]      Cross-cutting helpers         │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲ (every upper-layer depends down only)
╭─ L0-proc  (proc-macro split) ─────────────────────────────────────────────╮
│ nika-macros-core          [logic]           Syn minimal features          │
│ nika-macros               [thin shim]       proc-macro = true             │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L0.5  (trait defs + companions, async OK) ──────────────────────────────╮
│ nika-kernel-core          Clock + Fs + Http + Process + Blob + Shell      │
│ nika-kernel-ai            Provider* + Memory* + Embedding + Compressor    │
│ nika-kernel-runtime       ToolExecutor + Agent + Checkpoint + Context     │
│ nika-kernel-plugin        WasmPluginHost + Sandbox + ObservabilitySink    │
│                           (split when kernel > 10k LOC OR traits > 50)    │
│ nika-kernel-mock          [axes: none]         1:1 mirror, auto_impl      │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L1  (effect impls, async) ───────────────────────────────────────────────╮
│ nika-fs                   [axes: rw-fs]                                   │
│ nika-http-client          [axes: net-egress]   rustls only, no openssl    │
│ nika-process              [axes: exec-shell]                              │
│ nika-git                  [axes: rw-fs, net]   gix wrapper                │
│ nika-keys                 [axes: reads-env]    KeyProvider trait          │
│ nika-keys-env             L1                                              │
│ nika-keys-keychain        L2 feature-gated (never auto in tests)          │
│ nika-memory-oxigraph      [axes: rw-fs]        Cortex impl v0.95          │
│ nika-pck-registry         [axes: net-egress]                              │
│ nika-pck-store            [axes: rw-fs]                                   │
│ nika-<provider>-*         [axes: net-egress]   1 crate per frontier prov. │
│ nika-catalog-sync         [axes: net-egress]   freshness pipeline         │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L2  (verbs + domain services) ──────────────────────────────────────────╮
│ nika-pck              nika-mcp             nika-builtin-github            │
│ nika-verb-*           nika-policy          nika-builtin-cloud             │
│ nika-observability    nika-memory          nika-builtin-workspace         │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L3  (runtime + policy + sandbox) ────────────────────────────────────────╮
│ nika-runtime          nika-shield          nika-wasm-host (v0.100)        │
│                       nika-observability   nika-sandbox   (v0.100)        │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L4  (interfaces — libraries) ───────────────────────────────────────────╮
│ nika-cli (lib)        nika-daemon (lib)    nika-http (lib)                │
│ nika-mcp (lib)        nika-lsp (lib)       nika-sdk                      │
│                                             └─ xtask (build-only)         │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L5  (the binary — sole [[bin]] composition root) ────────────────────────╮
│ nika                  <500 LOC, wires L4 interfaces                       │
╰──────────────────────────────────────────────────────────────────────────╯
```

### Community overlays are NOT a layer

Community-contributed catalog overlays (e.g., `nika-catalog-cn`,
`nika-catalog-eu`) and `unstable-*` features live inside their natural
layer (typically L1 or L2) behind `pck` admission gates and feature
flags. They obey the same 12-gate admission as any other crate.

`unstable-*` is a distribution flag, not a layer:
`[workspace.metadata.diamond.unstable = true]` + `#![doc(cfg(unstable))]`.

## Layer table

| Layer | Role | Allowed I/O | Allowed deps | Example crates |
|---|---|---|---|---|
| L0 | Pure types, lookup tables, sync-only APIs | none | (leaf) | `nika-error`, `nika-catalog`, `nika-pck-manifest`, `nika-stdx`, `nika-event` |
| L0-proc | Proc-macro crates (kept separate for compile-time cost) | none (build-host only) | L0 | `nika-macros`, `nika-macros-core` |
| L0.5 | Kernel trait definitions + companions (mock) — async OK | none (traits only) | L0 | `nika-kernel`, `nika-kernel-mock` |
| L1 | Effect implementations — async, per-crate capability axis | declared axes only (fs/net/exec/env) | L0, L0.5 | `nika-fs`, `nika-http-client`, `nika-process`, `nika-git`, `nika-keys-*`, `nika-memory-oxigraph`, `nika-pck-registry`, `nika-pck-store`, `nika-<provider>-*`, `nika-catalog-sync` |
| L2 | Verbs + domain services — orchestrates L1 impls behind kernel traits | via L1 traits only | L0, L0.5, L1 | `nika-pck`, `nika-mcp`, `nika-verb-*`, `nika-memory`, `nika-observability`, `nika-builtin-{github,cloud,workspace}` |
| L3 | Runtime + policy + sandbox — enforces execution contracts | via L2 | L0..L2 | `nika-runtime`, `nika-shield`, `nika-wasm-host` (v0.100), `nika-sandbox` (v0.100) |
| L4 | Interfaces — transport/UI surface, libraries only | via L3 | L0..L3 | `nika-cli`, `nika-daemon`, `nika-http`, `nika-mcp`, `nika-lsp`, `nika-sdk` |
| L5 | The binary — sole `[[bin]]` composition root | via L4 | L0..L4 | `nika` (<500 LOC) |

### Forward compatibility

- **v0.95 Cortex** memory crates: 1 L2 orchestrator (`nika-memory`) + 8 L1
  satellites (hnsw, bm25, rrf, fsrs, rdfs-reasoner, temporal, graph-algos,
  autodesc). No renumber needed — they slot into L1/L2 naturally.
- **v0.100 WASM** plugin host + sandbox: L3 crates alongside `nika-runtime`.
  No renumber needed — L3 is "runtime + policy + sandbox" by design.

### Axis vocabulary

Every L1 crate declares the capabilities it exercises in
`docs/architecture/security-axes.toml` (landing v0.81). The vocabulary:

| Axis | Meaning |
|---|---|
| `reads-env` | Reads process environment variables |
| `reads-fs` | Reads filesystem |
| `rw-fs` | Reads and writes filesystem |
| `exec-shell` | Spawns subprocesses |
| `net-egress` | Initiates outbound network connections |
| `net-ingress` | Accepts inbound connections |
| `spawns-thread` | Uses `std::thread::spawn` or `tokio::task::spawn_blocking` outside the runtime pool |
| `mutates-global` | Mutates process-global state (env, CWD, signals) |
| `panics-allowed` | Explicit opt-out from zero-panic discipline (rare — test helpers only) |

A crate's declared axes are its capability budget. CI fails if a crate
exercises an axis it did not declare.

## Enforcement

**Layering vector** — `scripts/ci/check-layering.sh` (hygiene vector 11,
P0, fail on violation):

1. Reads layer membership from `[workspace.metadata.diamond.layers]` in
   the workspace `Cargo.toml`.
2. For every crate, parses its `[dependencies]` and verifies every
   workspace-local dependency targets an equal or lower layer.
3. Emits actionable output: `nika-foo (L1) depends on nika-bar (L2)
   → violates downward-only rule`.

**Security-axes vector** — `scripts/ci/check-security-axes.sh` (hygiene
vector 12, P1):

1. Reads `docs/architecture/security-axes.toml`.
2. Cross-checks declared axes against static analysis of the crate's
   source tree.
3. Fails when a crate performs, e.g., `net::TcpStream::connect` without
   declaring `net-egress`.

**No-async-in-L0 vector** — `scripts/ci/check-no-async-in-l0.sh`
(hygiene vector 16, P1): greps `async fn` and `async {` in L0 crate
source trees, fails on any match.

## Anti-patterns

1. **Kitchen-sink crate.** "I'll just add this utility to `nika-stdx`"
   leads to a 10k-LOC bag-of-tricks that every crate transitively pulls
   in. If a helper has one caller, it stays in that caller.
2. **Facade mega-crate.** "Let's re-export everything from `nika` for
   convenience" collapses the layer contract and tanks compile time.
   Public re-exports cross at most one layer (e.g., L5 → L4 for the
   binary's main.rs), never the whole stack.
3. **Upward re-exports.** An L1 crate that `pub use nika_pck::...` turns
   its API surface into an L2 consumer — breaks mechanical sorting. Each
   crate re-exports only from strictly lower layers.
4. **`async` in L0.** Async in a pure-types crate forces a runtime
   choice on every L1/L2 consumer and makes the crate impossible to use
   in build scripts. L0 is sync-only. Async traits live in L0.5.
5. **`anyhow::Error` in library code.** Per ADR-005, Nika uses
   `thiserror` + a typed error hierarchy. `anyhow` is allowed only in
   L5 binary `nika` and `xtask`, never in L0..L4 library surfaces.
6. **`Box<dyn Error>` in public API.** Hygiene vector 19 bans
   `Box<dyn Error>` on public signatures — swap to the typed error
   hierarchy.
7. **Runtime imports in L0.** `tokio::spawn` in `nika-error` is a layer
   violation even if it compiles. The `no-async-in-L0` checker catches it.

## Reserved v0.81 kernel split

`nika-kernel` currently ships as one L0.5 crate. Per ADR-006 (layered
kernel + ISP traits), it splits when either:

- total LOC exceeds 10k, **or**
- pub trait count exceeds 50.

Planned split into four sibling crates:

| Crate | Surface |
|---|---|
| `nika-kernel-core` | `Clock`, `Fs`, `Http`, `Process`, `Blob`, `Shell` |
| `nika-kernel-ai` | `Provider`, `ProviderStream`, `MemoryStore`, `EmbeddingProvider`, `ContextCompressor` |
| `nika-kernel-runtime` | `ToolExecutor`, `Agent`, `Checkpoint`, `Context` |
| `nika-kernel-plugin` | `WasmPluginHost`, `Sandbox`, `ObservabilitySink` |

Until the split trips one of the thresholds above, all traits live in the
single `nika-kernel` crate. The split is additive — downstream imports
change from `nika_kernel::X` to `nika_kernel_ai::X` behind a facade
re-export kept in `nika-kernel` for the v0.8x tail.

## See also

- `docs/architecture/forward-compat-invariants.md` — 8 patterns + 10 rules
  + `#[non_exhaustive]` ratchet
- `docs/architecture/ai-velocity.md` — why context-window-sized crates
- `docs/adr/adr-004-context-window-sized-crates.md` — Accepted
- `docs/adr/adr-006-layered-kernel-isp-traits.md` — Accepted
- `docs/adr/adr-014-sealed-kernel-traits.md` — Accepted
- ADR-016 — Cancellation model (Accepted)
- ADR-018 — Runtime + sync primitives (Accepted)
- ADR-020 — WASM plugin boundary + Sandbox (Accepted)

🦋
