# Crate spec — `nika-verb-invoke`

| | |
|---|---|
| Status | **SPEC** (Gate 1 · authored 2026-06-11 · announce-ladder step s11 · night arc · follows s10 `nika-verb-exec`) |
| Layer | **L2** — verb crate · domain executor for the `invoke` verb (3rd of the 4 verbs · D-2026-05-22-N18) |
| Design | consumes the L0.5 kernel `runtime::ToolExecuteDyn` seam (injected · the wiring layer hands it the engine's builtin+MCP dispatcher) · zero tool implementation of its own · validates the closed `nika:`/`mcp:` tool-ref grammar BEFORE dispatch |
| LOC budget | ≤1.5k src · caps ≤1500/file · ≤15k/crate |
| Crate version | tracks workspace (`0.90.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L2 verb crate |
| NIKA codes | **NIKA_450–459** claimed inside the Verb range 430–479 (s9 infer 430-439 · s10 exec 440-449) · maps to spec `NIKA-INVOKE-001/002` (`spec/05-errors.md:93-94`) |

---

## §0 · Architecture — the seam (verified 2026-06-11)

1. **The tool-exec contract is L0.5-complete.** `nika-kernel-runtime/src/tool_executor.rs`
   ships `ToolCall` (id · name · `input: serde_json::Value` · `#[non_exhaustive]`)
   · `ToolResult` (`tool_use_id` · `content: String` · `is_error: bool`) ·
   `ToolExecuteDyn` / `ToolBatchDyn` atomic traits (+ `ToolExecutor` blanket).
   `ToolExecError` (NotFound · Timeout · ExecutionFailed · NotAvailable ·
   NIKA-230..279).
2. **The dispatcher is injected, never owned.** The verb takes `Arc<T>` with
   `T: ToolExecuteDyn` — production wiring injects the engine's builtin+MCP
   dispatcher (resolves `nika:*` against the closed 22-builtin set + `mcp:*`
   against the configured server registry); tests inject a mock executor.
   NO Cargo dep on `nika-builtin` / `nika-mcp` — the verb reaches them through
   the kernel trait only.
3. **The language contract** — `spec/02-verbs.md §invoke`: required `tool`
   (`nika:<path>` OR `mcp:<server>/<tool>` · CEL-resolved upstream) · optional
   `args` (object · tool-specific schema). The **namespace set is CLOSED at
   v1** (`nika:` · `mcp:` only); a third namespace is rejected. `mcp:` REQUIRES
   the slash (`mcp:postgres` alone is a parse error).
4. **Two validation tiers.** Grammar shape (`NIKA-PARSE`) is the upstream
   `nika-schema` concern. This verb owns the **semantic** reject: an
   unresolvable tool id (unknown builtin · `mcp:` missing slash · closed-set
   violation reaching the verb) → `NIKA-450` (spec NIKA-INVOKE-001), and the
   dispatcher's own NotFound → the same. A tool that runs but returns
   `is_error: true` → `NIKA-451`.

```text
   future L3 nika-engine ── schedules ──┐
                                        v
   L2  nika-verb-invoke   run(InvokeInput) → InvokeOutput
         │ Arc<T: ToolExecuteDyn>   (tool-ref grammar validated here)
         v
   L0.5 nika-kernel-runtime  ToolCall / ToolResult / ToolExecError
         (impl = engine builtin+MCP dispatcher · injected at wiring)
```

## §1 · Public API (admission shape)

```rust
pub struct InvokeVerb<T> { executor: Arc<T> }

#[non_exhaustive]
pub struct InvokeInput {
    pub tool: String,                  // `nika:<path>` | `mcp:<server>/<tool>`
    pub args: serde_json::Value,       // default `{}` (object)
    pub call_id: Option<String>,       // engine-supplied ToolCall id (else derived)
}

#[non_exhaustive]
pub struct InvokeOutput {
    pub content: String,               // ToolResult.content
    pub tool: String,                  // the resolved tool id (echo)
}

impl<T: ToolExecuteDyn + Send + Sync> InvokeVerb<T> {
    pub fn new(executor: Arc<T>) -> Self;
    pub async fn run(&self, input: InvokeInput) -> Result<InvokeOutput, VerbInvokeError>;
}
```

## §2 · Error model (one-voice · vector 37)

| Code | Variant | Spec mapping | transient |
|---|---|---|---|
| NIKA_450 | `UnresolvableTool { tool, detail }` (bad namespace · mcp missing slash · dispatcher NotFound) | NIKA-INVOKE-001 | `false` |
| NIKA_451 | `ToolReportedError { tool, content_tail }` (`is_error: true`) | none (engine-internal · no spec row) | `false` |
| NIKA_452 | `Dispatch` (wraps `ToolExecError` Timeout/ExecutionFailed/NotAvailable) | tool_error | inherited |

NIKA_453–459 reserved (future: args-schema validation NIKA-INVOKE-002 when the
verb gains tool-schema awareness · today schema validation is the tool's own).

## §3 · Scope fences

- **NOT the tool implementations** — builtins + MCP live behind the injected
  dispatcher.
- **NOT grammar parsing** — `NIKA-PARSE` shape is upstream `nika-schema`; the
  verb does a lightweight namespace/slash semantic check only.
- **NOT args-schema validation** — the tool owns its schema (spec: « validate
  against tool's schema if known »); NIKA-INVOKE-002 is reserved until the
  verb is given catalog schema access.
- **NOT batch** — `ToolBatchDyn` is the agent/engine surface.

## §4 · Testing strategy

- **TDD** mock-first: `nika:read` happy path · `mcp:postgres/query` happy
  path · unknown `nika:ghost` → NIKA-450 · `mcp:postgres` (no slash) → NIKA-450
  zero dispatch · `custom:x` closed-namespace → NIKA-450 · `is_error: true` →
  NIKA-451 · dispatcher Timeout → NIKA-452 · args passthrough verbatim ·
  call_id supplied vs derived.
- **Property** (Gate 6): the tool-ref classifier is total over arbitrary
  strings (never panics · the closed-namespace + slash rules hold).
- **Mutation** (Gate 5): ≥90 %.
- **Parity** (Gate 10): pinned vs brouillon `tools/nika-verb-invoke` (delegate
  to kernel tool trait · namespace dispatch · result content passthrough).
- **Canary** (Gate 9): N/A — no L3 runner.
- **Benchmarks** (Gate 7): N/A.

## §5 · Wiring pass

L2 row exists. Remaining: `.gitignore` lift · `Cargo.toml` members +
`layers.nika-verb-invoke = "L2"` + wip · `deny.toml` tokio wrapper (dev-dep).

## §6 · Dependencies

```toml
[dependencies]
nika-error  = { path = "../nika-error",  version = "0.90.0" }
nika-kernel = { path = "../nika-kernel", version = "0.90.0" }
miette · thiserror · serde_json  # args is a JSON value
[dev-dependencies]
proptest · tokio (test rt)  # + an inline mock ToolExecute
```

## §7 · Update log

```
2026-06-11  v0.1 — Gate 1 SPEC authored (night arc · s11 · post-s10 template) ·
              seam verified (kernel runtime::ToolExecuteDyn · spec §invoke
              closed nika:/mcp: namespace · brouillon read-only reference).
```
