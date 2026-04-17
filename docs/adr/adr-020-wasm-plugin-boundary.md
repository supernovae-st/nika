---
id: ADR-020
title: "WASM plugin boundary + Sandbox: trait stubs now, dual timeout later"
status: accepted
date: "2026-04-16"
phase: "Phase D -- Session S1-E (Wave 2)"
deciders: ["@ThibautMelen"]
tags: ["wasm", "plugin", "sandbox", "capability", "forward-compat"]
affects_crates: ["nika-kernel", "nika-kernel-mock"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-007", "ADR-014", "ADR-016", "ADR-034"]
requires: ["ADR-007", "ADR-014"]
enables: []
amends: []
fci: ["pattern-1-kernel-traits-upfront"]
inv: []
shadow_zones: []
nika_codes: ["NIKA-380", "NIKA-381", "NIKA-384"]
timeline: "v0.81.0-alpha.1 (stubs); v0.100 (impls)"
follow_ups:
  - "Decide MessagePack vs CBOR for plugin payload format (v0.100)"
  - "Pick wasmtime vs wasmer (v0.100, benchmark-driven)"
  - "Implement nika-sandbox-landlock (Linux) + nika-sandbox-seatbelt (macOS) (v0.100)"
  - "Implement dual-timeout (wall-clock + fuel) inside nika-wasm-host (v0.100)"
---

# ADR-020: WASM plugin boundary + Sandbox: trait stubs now, dual timeout later

## Context

`forward-compat-invariants.md` Pattern 1 lists `WasmPluginHost`, `Sandbox`,
and `ObservabilitySink` as locked stub traits for v0.81. The intent: define
the trait shape now so future implementations slot in with zero breaking
change to the public API (Pattern 2: `#[non_exhaustive]` + opt-in fields).

The kernel before Wave 2 (`crates/nika-kernel/src/lib.rs:48-60`) defined ~20
atomic traits across 13 modules (agent, blob, checkpoint, clock, context,
errors, fs, http, memory, process, provider, tool_executor, types). None of
them addressed plugin sandboxing.

S1-B (Wave 2 commit `b68e58d4b`) added 4 new modules:

- `crates/nika-kernel/src/cancel.rs` — `CancelCtx` (real impl).
- `crates/nika-kernel/src/observability.rs` — `ObservabilitySink` (stub).
- `crates/nika-kernel/src/plugin/wasm.rs` — `WasmPluginHost`, `PluginFs`,
  `PluginHttp`, `WasmPluginError` (stubs).
- `crates/nika-kernel/src/plugin/sandbox.rs` — `Sandbox`, `Capability`,
  `SandboxError` (stubs).

ADR-014 sealing policy distinguishes "sealed" traits (community cannot
implement, must use the pck plugin protocol) from "open" traits (community
may implement alternative backends). The choice is per-trait.

The kernel `Cargo.toml` (`crates/nika-kernel/Cargo.toml:18-26`) has no WASM
deps — the stubs are trait definitions only, zero implementation code, zero
new transitive deps.

Shield error codes NIKA-380 (CapabilityDenied), NIKA-381 (TrustViolation),
NIKA-384 (SpotlightRequired) already exist for runtime capability checks at
the workflow layer. The Sandbox trait gives those codes a kernel-level
substrate to enforce on.

## Decision

**Define the WASM plugin and sandbox traits at v0.81 with zero
implementation. Pick implementation details when v0.100 is in scope.**

### v0.81 (now, shipped in S1-B)

- `WasmPluginHost` (async, sealed = NO / **open**) — community may ship
  alternative WASM hosts (wasmtime, wasmer, or future WASI preview3
  runtimes).
- `PluginFs` (sync, **open**) — sandboxed file access for guest code.
- `PluginHttp` (sync, **open**) — sandboxed HTTP for guest code.
- `WasmPluginError` enum: `NotFound`, `ExecutionFailed`, `SandboxViolation`,
  `Timeout`. All variants `#[non_exhaustive]`.
- `Sandbox` trait (async, **open**) — community may ship alternative sandbox
  backends (Landlock on Linux, seatbelt on macOS, AppArmor, Bubblewrap).
- `Capability` enum (`#[non_exhaustive]`): `FsRead`, `FsWrite`, `Network`,
  `ProcessSpawn`, `EnvRead`. **Default-deny** allowlist model.
- `SandboxError` enum: `Unavailable`, `CapabilityDenied`, `SetupFailed`.
- Mocks: `NullWasmPluginHost` (always returns `NotFound`), `NullSandbox`
  (allow-all for tests).

### v0.100 (deferred)

- **Implementation choice between wasmtime and wasmer** — benchmark when
  v0.100 is in scope. Both are viable; the trait shape is engine-agnostic.
- **Dual timeout**: wall-clock (kills the WASM instance) AND fuel-based
  (instruction budget, platform-agnostic). Both required because wall-clock
  alone misses runaway pure-compute, and fuel alone misses I/O-bound stalls.
- **Plugin payload format**: MessagePack or CBOR for compactness; decide at
  v0.100 with a benchmark on representative payloads.
- **Sandbox backends**:
  - `nika-sandbox-landlock` (Linux 5.13+).
  - `nika-sandbox-seatbelt` (macOS).
  - `nika-sandbox-noop` (Windows / unsupported, hard-fails on any
    capability check by default).

### Sealing rationale (ADR-014 application)

| Trait | Sealed? | Why |
|---|---|---|
| `Provider`, `MemoryStore`, `EventSink`, `BillingSink` | **sealed** (per ADR-034) | Workspace controls implementations; community uses the pck protocol. |
| `WasmPluginHost`, `PluginFs`, `PluginHttp` | **open** | Multiple WASM runtimes are legitimate; we should not block alternative hosts. |
| `Sandbox` | **open** | Platform-specific backends; users should be free to add new OS backends. |

## Consequences

### Positive

- Future v0.100 implementations are **strictly additive** — no breaking
  change to any consumer that imports the trait today.
- Capability model is default-deny — security posture is conservative by
  construction.
- Open traits invite community contributions (Bubblewrap, Firecracker
  micro-VMs) without forking the kernel.
- The mocks (`NullWasmPluginHost`, `NullSandbox`) let downstream crates
  satisfy trait bounds today even though no real impl exists.

### Negative

- Stub traits add ~250 LOC to the kernel that returns errors today. Cost
  is small; the alternative (adding traits at v0.100) is a breaking
  change for every `Provider` impl that wants to live alongside.
- "Open" classification means we cannot later add a non-trivial method to
  `Sandbox` without a major version bump, even at v0.x. Mitigated by
  `#[non_exhaustive]` on the trait's associated enums (Capability,
  SandboxError) and by adding new methods with `default` impls.
- `PluginFs` and `PluginHttp` are sync (per design decision: WASM guests
  see synchronous calls), but real impls must internally bridge to async
  HTTP — cost of bridging happens once per call, accepted.

### Neutral

- `Sandbox` is orthogonal to WASM. It is also reusable for MCP server
  isolation when MCP lifecycle ships (Phase D Session 4).
- `WasmPluginError::Timeout { timeout_ms }` reserves the wall-clock
  timeout shape. The fuel-based timeout will surface as a separate
  variant (`OutOfFuel`) when v0.100 ships — additive.

## Evidence / Affected code

- `crates/nika-kernel/src/plugin/wasm.rs:11-22` — `WasmPluginHost` trait.
- `crates/nika-kernel/src/plugin/wasm.rs:24-39` — `PluginFs`, `PluginHttp`.
- `crates/nika-kernel/src/plugin/wasm.rs:50-83` — `WasmPluginError` enum.
- `crates/nika-kernel/src/plugin/sandbox.rs:18-26` — `Sandbox` trait.
- `crates/nika-kernel/src/plugin/sandbox.rs:28-54` — `Capability` enum.
- `crates/nika-kernel/src/plugin/sandbox.rs:78-99` — `SandboxError`.
- `crates/nika-kernel-mock/src/plugin.rs` — `NullWasmPluginHost`.
- `crates/nika-kernel-mock/src/sandbox.rs` — `NullSandbox`.
- `crates/nika-kernel/src/lib.rs:50,58,64-65` — module + re-exports.
- `crates/nika-kernel/Cargo.toml:18-26` — dep list (no WASM crates).
- `docs/architecture/forward-compat-invariants.md` — Pattern 1 (kernel
  traits upfront).
- `docs/adr/adr-014-sealed-kernel-traits.md` — sealing policy.

## Alternatives considered

### Alt A -- No WASM support; extend MCP only
Lean on MCP servers for all extensibility.
Rejected: MCP requires a separate process per server, with stdio or
TCP overhead per call. WASM allows in-process plugins with microsecond
startup and shared memory. Both will coexist at v0.100; WASM is
strictly faster for short-lived, high-frequency plugins.

### Alt B -- WASI-only, no custom traits
Adopt the WASI Preview2 component model directly.
Rejected: WASI Preview2 doesn't cover all Nika capabilities (e.g., LLM
inference calls *from* plugins to providers). Custom traits give us
control of the host-side API; we can still target WASI Preview2/3 as
the guest ABI inside `nika-wasm-host`.

### Alt C -- Define full WASM types now (memory layout, instance handles)
Land everything at v0.81.
Rejected: premature. The trait shape is enough to satisfy Pattern 1.
Memory layout, instance lifecycle, and host-function bindings depend
on wasmtime/wasmer API evolution; defining them now risks rework when
v0.100 arrives.

### Alt D -- Seal `WasmPluginHost` and `Sandbox`
Apply the same sealing policy as `Provider`.
Rejected: unlike `Provider` (where workspace owns 25 implementations),
WASM hosts and sandbox backends are platform-specific and benefit
from community alternatives. Sealing them would block legitimate
implementations (Bubblewrap, Firecracker, AppArmor, future WASI runtimes).

### Alt E -- Single timeout (wall-clock only) at v0.100
Skip fuel-based timeouts.
Rejected for design intent: pure-compute WASM (no I/O) can spin without
ever yielding to the host. Wall-clock requires a separate watchdog
thread; fuel-based is cooperative and platform-agnostic. Both are
needed; choosing one is a regression we should not bake in.

## Related

- ADR-006 — kernel ISP trait design. ADR-020 adds traits to the kernel
  list but does not change ADR-006's existing trait set.
- ADR-007 — forward-compat invariants (Pattern 1: kernel traits upfront,
  Pattern 2: `#[non_exhaustive]` on every public enum/struct).
- ADR-014 — sealed kernel traits. ADR-020 explicitly classifies
  `WasmPluginHost`, `PluginFs`, `PluginHttp`, `Sandbox` as **open**
  (not sealed).
- ADR-016 — Cancellation. WASM plugin calls should accept a
  `CancelCtx` once v0.95 ships, so a plugin call respects parent
  cancellation.

## Notes

Follow-ups (v0.100 territory):

- Pick wasmtime vs wasmer with a benchmark on representative payloads
  (large LLM responses passed into plugin, plugin returning embeddings).
- Implement dual-timeout (wall-clock + fuel) inside `nika-wasm-host`.
- Decide payload format: MessagePack (smaller) vs CBOR (more standardized).
- Implement `nika-sandbox-landlock`, `nika-sandbox-seatbelt`, and a
  `nika-sandbox-noop` for unsupported platforms.
- Add new variants to `Capability` enum as platform support widens
  (e.g., `Clock` for time access, `RandomBytes` for entropy).
