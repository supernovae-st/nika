# Crate Layer Registry

**Status**: planned for v0.81 enforcement. The layer discipline described
here is the ground truth that `[workspace.metadata.diamond.layers]` and
`scripts/ci/check-layering.sh` are built to enforce.

**Authority**: This document shows the layer model with illustrative
examples. For the authoritative crate manifest, see POST_AUDIT_REVISIONS.md
(supreme authority) and `l0-l05-architecture-decisions.md` (Q1-Q8 layer
decisions, 2026-04-16). CONSTELLATION.md (forthcoming) reconciles all.

**Last updated**: 2026-04-16 (Q6-Q8 brainstorm: nika-transform inlined
into nika-binding, nika-event scoped sub-enums, nika-kernel prelude hub).

## Why this document exists

A crate workspace without a layer contract converges on a kitchen-sink dependency
graph: everything imports everything, every crate pulls in tokio, every
change ripples through the workspace. Diamond discipline defines six
layers (L0 → L5, with an L0.5 for trait-only crates) and makes upward
dependencies a CI hard failure.

The goal is **mechanical sorting**: every new crate has an unambiguous
home, and "which crate should this go in" becomes a look-up instead of a
debate.

### Mechanical sort test (< 10 seconds)

Given a crate `nika-<role>`, ask these questions in order:

1. Is it the composition root that owns the `nika` executable? → **L5** (today the `nika` bin target is born in L4's `nika-cli` · ADR-135)
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

╭─ L0  (pure, sync, zero I/O) — 6 crates in workspace (+3 planned) ───────╮
│ nika-types                [axes: none]      Foundation value types (leaf) │
│                                             + timestamp module (Q9)       │
│                                             + hash::canonical (Q10, JCS)  │
│ nika-error                [axes: none]      Error infra (depends types)  │
│ nika-catalog              [axes: none]      Cow<'static, str>, phf lookup │
│ nika-catalog-codegen      [build-dep+lib]   TOML schema types + codegen  │
│ nika-schema               [axes: none]      Workflow AST + parser (THE  │
│                                             PARSER · blueprint shape)   │
│ nika-check                [axes: none]      Analyzer + the `nika check` │
│                                             ladder (schema unit's 4th   │
│                                             member · 15k split 07-21)   │
│ nika-source               [axes: none]      Spans + file registry (the   │
│                                             schema unit's 2nd member ·   │
│                                             size-cap split D-07-09-N1)   │
│ nika-vocab                [axes: none]      .nika.yaml config vocabulary │
│                                             (schema unit's 3rd member ·  │
│                                             size-cap split D-07-09-N1) + │
│                                             the nika.yaml PROJECT file   │
│                                             (D-2026-08-11-N5 ·           │
│                                             ceiling/traces/registry/arm) │
│ nika-migrate              [axes: none]      .nika.yaml fix migrations    │
│                                             (W1 map · W2 flow · from     │
│                                             nika-cli · size-cap D-07)    │
│ nika-event                [axes: none]      ~22 scoped sub-enums +        │
│                                             event_categories! macro      │
│ nika-binding              [axes: none]      Template resolution engine   │
│                                             (depends on nika-transform)  │
│ nika-transform            [axes: none]      65 transforms, 7 sub-modules │
│                                             (Q8 rev.2 — 2 consumers:    │
│                                              binding + builtin-*)        │
│ nika-pck-manifest         [axes: none]      Package manifest TOML types  │
│                                                                          │
│ REMOVED (per Q1, Q2 — see l0-l05-architecture-decisions.md):             │
│   nika-stdx         → nika-types IS the foundation; purpose-named if need│
│   nika-macros       → manual impl + macro_rules!; reopen at L2 builtins │
│   nika-macros-core  → same as above                                      │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲ (every upper-layer depends down only)
╭─ L0.5  (trait defs + companions, async OK) ──────────────────────────────╮
│ nika-kernel               facade + range-registry HUB (split 2026-06-10) │
│                           re-exports every sibling path + pub mod        │
│                           prelude re-exporting nika-error::prelude::* —  │
│                           L2+ verb crates depend on nika-kernel only     │
│ nika-kernel-mock          [axes: none]  1:1 mirror, pure-memory mocks    │
│                                                                          │
│ ✅ 4-way split EXECUTED 2026-06-10 (trigger hit · 50 pub traits) ·       │
│   census kernel-split-census-2026-06-10.md is the freeze · the facade    │
│   preserves every historical path (zero downstream break) · per-type    │
│   NikaErrorCode impls live with their types (orphan rule) ·             │
│   nika-kernel-core        io 19 traits + infra 7 + cancel/sealed/types  │
│   nika-kernel-ai          Provider* + Memory* + Vision + Compressor (14)│
│   nika-kernel-runtime     ToolExecutor traits (3) + agent type surfaces │
│   nika-kernel-plugin      WasmPluginHost + Sandbox (6 · ADR-020 open)   │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L1  (effect impls, async) ───────────────────────────────────────────────╮
│ nika-clock                [axes: none]         time source                │
│ nika-fs                   [axes: rw-fs]        atomic write (s4)          │
│ nika-http                 [axes: net-egress]   rustls only · 3-layer SSRF │
│ nika-blob                 [axes: rw-fs]        blake3 CAS (s6)            │
│ nika-exec-runner          [axes: exec-shell]   shell/process effect (s7)  │
│ nika-git                  [axes: rw-fs, net]   gix wrapper                │
│ nika-keys                 [axes: reads-env]    KeyProvider trait          │
│ nika-keys-env             L1                                              │
│ nika-keys-keychain        L2 feature-gated (never auto in tests)          │
│ nika-bm25                 [axes: none]         BM25 lexical (ADR-038) +   │
│   the Connectome satellites (hnsw · rrf · rerank · fsrs · rdfs-reasoner   │
│   · temporal · graph-algos · autodesc-minimal/full · Phase-M climb)       │
│ nika-store                [axes: rw-fs]        F-P8 signed memory (SMSR)  │
│   · `.nika/memory/<store>/` signed envelopes · key injected (custody L4)  │
│ nika-screen / ocr / a11y  [axes: computer-use read]  M2.1-M2.3 (ADR-081)  │
│ nika-input / browser      [axes: computer-use act]   M2.4-M2.5 (ADR-081)  │
│ nika-pck-registry         [axes: net-egress]                              │
│ nika-pck-store            [axes: rw-fs]                                   │
│ nika-catalog-sync         [axes: net-egress]   freshness pipeline         │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L1.5 (provider wire adapters) ───────────────────────────────────────────╮
│ nika-providers            [axes: via L1 http]  14/14 wire + in-crate mock │
│ nika-infer-local          [axes: rw-fs]        candle sidecar (ADR-091)   │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L2  (verbs + domain services) ──────────────────────────────────────────╮
│ nika-pck              nika-policy (s8)     nika-builtin-github            │
│ nika-verb-*           nika-connectome      nika-builtin-cloud             │
│ nika-observability                         nika-builtin-workspace         │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L3  (runtime + policy + sandbox) ────────────────────────────────────────╮
│ nika-runtime          nika-execution       nika-shield                    │
│                       owned world ↑ file · stdin · ARM                    │
│ nika-wasm-host (v0.100)                    nika-sandbox   (v0.100)        │
╰──────────────────────────────────────────────────────────────────────────╯
              ▲
╭─ L4  (interfaces — libraries) ───────────────────────────────────────────╮
│ nika-cli (lib)        nika-arm (lib)       nika-serve (lib)               │
│ nika-mcp (lib)        nika-lsp (lib)       nika-sdk · nika-dap (lib)      │
│ nika-catalog-verify   (build-only)  nika-check-wasm (browser · WIP)       │
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
| L0 | Pure types, lookup tables, sync-only APIs (live counts: refresh-status block) | none | (leaf) | `nika-types`, `nika-error`, `nika-catalog`, `nika-catalog-codegen`, `nika-schema`, `nika-check`, `nika-check-analyzer` (size-cap member of the nika-check unit · the analysis substrate · D-2026-07-09-N1 · ADR-115), `nika-source`, `nika-vocab` (workflow vocabulary + the `nika.yaml` project file · D-2026-08-11-N5), `nika-graph`, `nika-event`, `nika-pack`, `nika-migrate`, `nika-proof` (WIP · the spec-15 hash primitives + receipt · split from nika-runtime at the 15k wall 2026-07-29), `nika-cadence` (WIP · the `arm:`-registry grammar + the pure next-slot calculator · the 6 pre-freeze corrections of the arming plan §2unvicies · 2026-08-12) · *planned* · `nika-binding`, `nika-transform`, `nika-pck-manifest` |
| L0.5 | Kernel trait definitions + companions (mock) — async OK; prelude re-export hub | none (traits only) | L0 | `nika-kernel` (facade hub), `nika-kernel-core`, `nika-kernel-ai`, `nika-kernel-runtime`, `nika-kernel-plugin`, `nika-kernel-mock` |
| L1 | Effect implementations — async, per-crate capability axis | declared axes only (fs/net/exec/env · +computer-use axes from M2) | L0, L0.5 | `nika-clock`, `nika-fs`, `nika-http`, `nika-blob`, `nika-exec-runner`, `nika-screen`, `nika-ocr`, `nika-a11y`, `nika-input`, `nika-browser` (ADR-081 guards), `nika-bm25` ✅, `nika-store` (F-P8 signed memory · SMSR) + the Connectome satellites (`nika-hnsw`, `nika-rrf`, `nika-rerank`, …), `nika-git`, `nika-keys-*`, `nika-pck-registry`, `nika-pck-store`, `nika-catalog-sync` |
| L1.5 | Provider wire adapters — between effects and verbs | via L1 http seam | L0, L0.5, L1 | `nika-providers` (14/14 wire · in-crate mock), `nika-infer-local` (candle sidecar · ADR-091) |
| L2 | Verbs + domain services — orchestrates L1 impls behind kernel traits | via L1 traits only | L0, L0.5, L1, L1.5 | `nika-pck`, `nika-verb-*`, `nika-policy` (s8 · design locked), `nika-connectome` (the Connectome orchestrator), `nika-observability`, `nika-builtin-{github,cloud,workspace}`, `nika-registry-client` (size-cap descent from nika-cli · D-2026-07-09-N1) |
| L3 | Runtime + policy + sandbox — enforces execution contracts | via L2 | L0..L2 | `nika-runtime` (+ its member `nika-runtime-laws` · ADR-127), `nika-execution` (descriptor-rooted immutable-world admission; owned-root overlay for stdin/adapters), `nika-shield`, `nika-wasm-host` (v0.100), `nika-sandbox` (v0.100) |
| L4 | Interfaces — transport/UI surface, libraries only | via L3 | L0..L3 · plus **lateral L4→L4** edges (`check-layering.sh` refuses only an UPWARD rank; a same-rank edge is admitted so one interface crate can reuse another's pure seam instead of cloning it — `nika-mcp → nika-onboard` for the ONE routing door, `nika-mcp → nika-cli-host` for the ONE `check --fix` ladder (#1270); the edge must stay acyclic and the reused member must never depend back) | `nika-cli`, `nika-arm` (descriptor-rooted ARM custody + the one injected firer shared by CLI and Serve), `nika-cli-host` (size-cap member of the nika-cli unit · D-2026-07-09-N1 · ADR-110), `nika-trace` (size-cap member of the nika-cli unit · the flight-recorder reader · D-2026-07-09-N1 · 2026-08-11), `nika-dap`, `nika-display`, `nika-onboard`, `nika-models`, `nika-daemon`, `nika-serve`, `nika-mcp`, `nika-lsp`, `nika-sdk`, `nika-catalog-verify`, `nika-check-wasm` (WIP · ADR-107), `nika-session` (the native session · a host runtime over the installed engine · lateral `nika-session → nika-cli-host` for the ONE probe and the ONE oracle facade · ADR-125) |
| L5 | The composition root (future) — will own the `nika` bin target by moving it, never by renaming it (today the target is born `nika` in `nika-cli` · ADR-135) | via L4 | L0..L4 | `nika` (<500 LOC) |

### Forward compatibility

- **The Connectome** (v0.85 recall floor → v0.90 operational): 1 L2
  orchestrator (`nika-connectome`) + 10 L1 satellites (hnsw, bm25 ✅, rrf,
  rerank, fsrs, rdfs-reasoner, temporal, graph-algos, autodesc-minimal,
  autodesc-full). No renumber needed — they slot into L1/L2 naturally.
- **v0.100 WASM** plugin host + sandbox: L3 crates alongside `nika-runtime`.
  No renumber needed — L3 is "runtime + policy + sandbox" by design.

### Axis vocabulary (12 axes — extended swarm-3 audit 2026-04-16)

Every L1 crate declares the capabilities it exercises in
`docs/architecture/security-axes.toml` (landing v0.81). Reserving an axis
costs nothing today and forbids its silent later use. Vocabulary:

| Axis | Meaning |
|---|---|
| `reads-env` | Reads process environment variables |
| `reads-fs` | Reads filesystem (read-only) |
| `rw-fs` | Reads and writes filesystem |
| `exec-shell` | Spawns subprocesses |
| `net-egress` | Initiates outbound network connections |
| `net-ingress` | Accepts inbound connections |
| `spawns-thread` | Uses `std::thread::spawn` or `tokio::task::spawn_blocking` outside the runtime pool |
| `mutates-global` | Mutates process-global state (env, CWD, signals) |
| `panics-allowed` | Explicit opt-out from zero-panic discipline (rare — test helpers only) |
| `reads-secrets` | Reads `Secret<T>` / `SecretRef` material (KeyProvider, Keychain, env-token) |
| `time-mutation` | Manipulates wall-clock or monotonic clock for tests (`MockClock`, `tokio::time::pause`) |
| `allocator-sensitive` | Imposes allocator constraints (`bumpalo`, custom `GlobalAlloc`, `no_std` arenas) — pre-flight for v0.100 WASM |
| `thread-local-state` | Uses `thread_local!` or static `RefCell` (incompatible with thread-pool migration / WASM) |

A crate's declared axes are its capability budget. CI fails if a crate
exercises an axis it did not declare. Adding an axis to the vocabulary is
a non-breaking change; reclassifying an existing crate to a new axis is.

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

1. **Kitchen-sink crate.** "I'll just add this utility to `nika-error`"
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

## Kernel split (EXECUTED 2026-06-10)

The ADR-006 threshold (`LOC > 10k OR traits > 50`) was HIT at 50 pub
traits — the 4-way split executed per the locked sequence
(`docs/architecture/kernel-split-census-2026-06-10.md` is the census
freeze + bucket map). The split was additive as designed — `nika-kernel`
is now the facade + NIKA-NNN range-registry hub re-exporting every
sibling path (`nika_kernel::io::…` · `nika_kernel::Provider` · the flat
re-exports) · zero downstream import changed. Per-type `NikaErrorCode`
impls + constants moved into the sibling owning each type (orphan rule ·
discovered at execution · census amended in place). New kernel traits
land directly in their bucket sibling.

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
