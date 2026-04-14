# Nika Diamond -- Feature Roadmap (post research, 2026-04-13)

Integrates competitive intelligence research findings into the diamond
timeline. These are not future nice-to-haves. They are part of the plan.

Last updated: 2026-04-13

---

## Tier 1 -- Core Diamond (already planned)

These are in the existing plan. Confirmed covered.

| Feature | Phase | Crate(s) | Status |
|---|---|---|---|
| Single binary, 4ms startup, brew install | Phase 6 (cutover) | nika (L5 binary) | Planned |
| YAML declarative + `nika check` validation | Phase 1 (nika-schema) | nika-schema | In progress |
| 5 verbs (exec, fetch, invoke, infer, agent) | Phase 3 | nika-verb-* | Planned |
| 9+ providers via rig-core | Phase 3 | nika-provider-rig, nika-provider-native, nika-provider-mock | Planned |
| 63 builtin transforms | Phase 3 | nika-builtin | Planned |
| MCP native (113 aliases, invoke/agent) | Phase 3 | nika-mcp + nika-catalog | Planned |
| Structured output 3-phase: Enforce/Extract/Recover | Phase 3 | nika-verb-infer | Planned |
| Shield 6-layer security | Phase 4 | nika-runtime (nika-policy module) | Planned |
| Agent v2 + Cortex memory | Post v0.90, v0.95 | nika-memory-*, nika-agent-v2 | Planned |

No changes needed. These are locked.

---

## Tier 2 -- High Impact, Integrated Into Diamond

These get added to specific phases now.

### Cost-Per-Step Tracking

**Phase:** 3-4
**Crates:** nika-runtime, nika-event, nika-catalog

Every task step reports granular cost data:

- `tokens_in`, `tokens_out`, `cost_usd`, `duration_ms`, `provider`, `model`
- OpenTelemetry GenAI semantic conventions: `gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `gen_ai.usage.total_tokens`
- Aggregated in workflow result: `total_cost_usd`, `per_step_costs` array
- `nika cost workflow.nika.yaml` CLI command for cost estimation before running

**Integration points:**

- nika-event: `EventKind` gains `CostReport` variant (tokens, cost, provider, model, duration)
- nika-catalog: pricing data per provider/model (updated via catalog refresh)
- nika-runtime: aggregates per-step costs, emits summary event at workflow end
- nika-cli: `nika cost` subcommand reads catalog pricing, estimates from workflow shape

**Dependencies:** opentelemetry 0.27, opentelemetry-otlp 0.27

### Budget Enforcement

**Phase:** 4
**Crates:** nika-runtime (policy module)

Workflow and task level cost caps:

- Workflow-level: `max_cost_usd: 5.0` in YAML header
- Task-level: `max_cost_usd: 1.0` per task
- At 80% budget: auto-fallback to `provider.cheap_model` (configurable threshold)
- At 100% budget: hard stop, return partial results + `CostExceeded` error
- Uses nika-catalog pricing data for real-time estimation

**New error codes:**

- `NIKA-530` CostExceeded -- workflow or task exceeded budget, hard stop
- `NIKA-531` BudgetWarning -- approaching budget limit, fallback activated

**Integration points:**

- nika-schema: validates `max_cost_usd` field in workflow/task YAML
- nika-runtime: budget tracker middleware, checks after each provider call
- nika-error: new error codes registered in NikaCode enum
- nika-catalog: provides model pricing for estimation

### MCP Metadata Enrichment

**Phase:** 3
**Crates:** nika-mcp, nika-catalog

Consume structured metadata from MCP ecosystem:

- Consume MCPB Manifest v0.3 `inputs` field for required env vars
- `nika mcp add stripe` fetches manifest, checks `STRIPE_API_KEY` present
- `nika mcp list --pricing` shows Free/Freemium/Paid from catalog
- `nika mcp check` validates all configured servers have required keys

**Enriched McpAlias struct fields:**

- `required_env_vars: Vec<String>`
- `auth_type: AuthType` (enum: None, ApiKey, OAuth, Custom)
- `manifest_url: Option<String>`
- `pricing_tier: Option<PricingTier>` (enum: Free, Freemium, Paid)

**Future:** consume MCP Server Cards (SEP-1649) for `.well-known` discovery
when spec stabilizes.

### Ariadne-Quality YAML Diagnostics

**Phase:** 1, Step 4 (nika-schema)
**Crates:** nika-schema

Compiler-quality error messages for YAML validation:

- Use `ariadne` or `codespan-reporting` for parse error display
- Point to exact YAML line with colored snippet
- Levenshtein suggestions: `line 14: unknown provider "antrhopic" -- did you mean "anthropic"?`
- nika-schema parser wraps errors with source spans (byte offset + line/col)
- `nika check` output looks like Rust compiler errors (cargo-style)

**Example output:**

```
error[NIKA-201]: unknown provider
  --> workflow.nika.yaml:14:15
   |
14 |     provider: antrhopic
   |               ^^^^^^^^^ did you mean `anthropic`?
   |
```

**Dependency:** ariadne 0.4

---

## Tier 3 -- Visionary (v0.95-v1.0 timeframe)

These are planned for after v0.90 merge. Design work may start earlier
but implementation lands post-merge.

### A2A Protocol Support

Agent-to-Agent protocol (Google, now Linux Foundation).

- Nika workflows can discover and call other Nika agents
- Complementary to MCP: MCP = agent-to-tool, A2A = agent-to-agent
- Requires: agent discovery, capability negotiation, task delegation
- Timeline: evaluate spec stability H2 2026, prototype v0.95+

**Crate:** nika-a2a (new, L1 effect crate)

### WASM Plugin System

User-defined transforms compiled to WASM.

- Sandboxed execution via wasmtime
- `nika plugin install ./my-transform.wasm`
- Hot-reload without recompiling Nika
- Uses WIT (WebAssembly Interface Types) for interface definitions
- Plugin API: receive JSON input, return JSON output, access allowed host functions

**Crate:** nika-plugin-wasm (new, L1 effect crate)
**Dependency:** wasmtime (latest stable at time of implementation)

### Deterministic Replay

Capture and replay full workflow state.

- Record: full workflow state at each step (inputs, outputs, provider responses)
- Replay: any failure with exact same inputs, deterministic reproduction
- `nika replay --from-checkpoint workflow-run-id`
- Enables: debugging, regression testing, auditing, cost-free iteration
- Storage: local SQLite via nika-daemon-db, or export as JSON

**Crate:** nika-replay (new, L2 domain crate, depends on nika-daemon-db)

### Landlock Sandboxing (Linux) + macOS Sandbox

OS-level sandboxing for exec: verb.

- Linux: Landlock (5.13+) restricts filesystem and network per exec: task
- Filesystem: restrict to project dir + explicitly allowed paths
- Network: restrict to allowed hosts list
- macOS: sandbox-exec profiles or App Sandbox entitlements
- Graceful degradation: if kernel lacks Landlock, warn and proceed unsandboxed

**Crate:** nika-sandbox (new, L1 effect crate, platform-conditional)
**Dependency:** landlock 0.4 (Linux only, cfg-gated)

### Workflow Versioning

Semantic diff and versioning for workflow YAML.

- Git-like diff for workflow YAML changes
- `nika diff v1.yaml v2.yaml` -- semantic diff (understands structure, not just text)
- Version field in YAML schema, migration helpers between versions
- Audit trail: which version ran when, with what results

**Crate:** module in nika-schema (not a separate crate)

### MCP Server Cards Consumer

Auto-discovery of MCP servers via well-known URIs.

- Read `.well-known/mcp/server-card.json` for discovery
- Auto-discover capabilities before connecting
- Show in `nika mcp list` with server card metadata
- IETF draft-serra-mcp-discovery-uri-02 support
- Timeline: blocked on spec finalization

**Crate:** extension to nika-mcp (not a separate crate)

---

## Impact on Diamond Phases

Summary of what changes in each phase due to this roadmap.

### Phase 1 (current, 5-7 weeks)

- Add `ariadne = "0.4"` to workspace deps for nika-schema (Step 4)
- nika-schema parser produces source-span-annotated errors
- `nika check` gains compiler-style diagnostic output

### Phase 3 (12-15 weeks)

- nika-event: add `CostReport` variant to `EventKind`
- nika-catalog: add pricing data per provider/model, MCP manifest metadata
- nika-mcp: enriched `McpAlias` struct with env vars, auth type, pricing tier
- nika-runtime: cost tracking per step, aggregation

### Phase 4 (8-10 weeks)

- nika-runtime: budget enforcement middleware (policy module)
- nika-error: register NIKA-530, NIKA-531 error codes
- nika-cli: `nika cost` subcommand
- nika-schema: validate `max_cost_usd` field

### Phase 5 (4 weeks)

- Parity tests include cost estimation accuracy vs actual provider billing
- Budget enforcement integration tests with mock provider

### v0.95 (post v0.90 merge)

- A2A protocol: nika-a2a crate (pending spec stability)
- WASM plugins: nika-plugin-wasm crate
- Deterministic replay: nika-replay crate
- Landlock sandboxing: nika-sandbox crate
- Workflow versioning: module in nika-schema
- MCP Server Cards: extension to nika-mcp

---

## New Dependencies to Add (when phases land)

| Dependency | Version | Phase | Purpose | Platform |
|---|---|---|---|---|
| ariadne | 0.4 | Phase 1, Step 4 | YAML diagnostics with source spans | All |
| opentelemetry | 0.27 | Phase 3 | Cost tracking, GenAI semantic conventions | All |
| opentelemetry-otlp | 0.27 | Phase 3 | OTLP export for traces/metrics | All |
| landlock | 0.4 | v0.95 | OS-level sandboxing for exec: verb | Linux only |
| wasmtime | latest | v0.95 | WASM plugin runtime | All |

All dependencies are optional or feature-gated where platform-specific.
No dependency added before its phase lands.

---

## Crate Count Impact

**UPDATED 2026-04-14** per ROADMAP.md (forever v0.x model):

- v0.90 target: **40-42 crates** (32-34 core + 3 builtin bundles +
  4-5 pck infrastructure crates, from vision brainstorm 2026-04-14)
- v0.95: **~63-65 crates** (+ Cortex 9-10 + media 5 + 8 more natives)
- v0.100: **~72-75 crates** (+ WASM host + observability + LSP full)
- v0.110+: incremental additions
- **Hard cap: 100 crates ever**

Tier 2 features add zero new crates (integrated into existing crates).
Tier 3 features (nika-a2a, nika-plugin-wasm, nika-replay, nika-sandbox)
land at v0.100, included in the 72-75 ceiling above.

---

## Decision Log

| # | Decision | Rationale |
|---|---|---|
| D1 | Cost tracking in nika-event, not separate crate | Single event stream, no new dep boundary |
| D2 | Budget enforcement in policy module of nika-runtime | Collocated with other runtime policies (Shield) |
| D3 | MCP metadata in nika-catalog, not nika-mcp | Catalog is the metadata authority |
| D4 | Ariadne in Phase 1 | Error quality is a launch differentiator |
| D5 | WASM plugins post v0.90 | Wasmtime is heavy, core must ship first |
| D6 | Landlock post v0.90 | Platform-specific, not blocking core value |
| D7 | No separate nika-cost crate | Cost is a cross-cutting concern, not a domain |
| D8 | Tier 3 crates outside diamond count ceiling | Diamond count is for core engine |
