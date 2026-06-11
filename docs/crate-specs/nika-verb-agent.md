# Crate spec — `nika-verb-agent`

| | |
|---|---|
| Status | **SPEC** (Gate 1 · authored 2026-06-11 · announce-ladder step s12 · night arc · the 4th and LAST verb · **impl BLOCKED on a `ToolDefinitionProvider` seam** — see §8 ⛔ · not just deferred for time) |
| Layer | **L2** — verb crate · domain executor for the `agent` verb (4th of the 4 verbs · D-2026-05-22-N18) |
| Design | the multi-turn agentic loop · consumes `nika-providers` (L1.5 · inference) **+ `nika-verb-invoke`** (L2 · tool dispatch · same-layer dep — layering-legal, §0.5) · drives infer → tool-calls → infer until a terminal condition |
| LOC budget | ≤4k src (the most complex verb · brouillon agent loop was the largest verb) · caps ≤1500/file · ≤100/fn · ≤15k/crate |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L2 verb crate |
| NIKA codes | **NIKA_460–469** in the Verb range 430–479 (infer 430-439 · exec 440-449 · invoke 450-459) · maps to spec `NIKA-AGENT-001/002` (`spec/05-errors.md:95-96`) |

---

## §0 · Architecture — the seam (verified 2026-06-11)

1. **The agent kernel DTOs are L0.5-complete.** `nika-kernel-runtime/src/agent.rs`
   ships `AgentLoopConfig` (max_turns · planning · parallel_tools · reflection ·
   compression · tool_error_policy · session_id · inject_records) ·
   `AgentOutcome` (output · stop_reason · turns · total_tokens · cost ·
   checkpoint) · `AgentStopReason` (Completed · ExplicitCompletion · TurnsLimit
   · TokensLimit · CostLimit · DurationLimit · Failed) · `PlanningStrategy`
   (React default · ReWoo) · `ToolErrorPolicy` (ReportToLlm default ·
   RetryTransient · FailFast) · `ReflectionConfig` · all `#[non_exhaustive]`.
   **There is NO agent-loop EXECUTOR trait** — these are config/outcome value
   types; the loop logic lives in THIS crate.
2. **Inference rides `nika-providers`** (NOT `nika-verb-infer`) — the agent
   builds its own `InferRequest`s each turn (it needs `tools` on the request
   and tool-use blocks in the response, which the one-shot infer verb fences
   out). Same registry resolution as `verb-infer`, no verb→verb infer dep.
3. **Tool dispatch rides `nika-verb-invoke`** — each tool-use block the model
   emits becomes an `InvokeInput`; the agent reuses invoke's closed-namespace
   validation + dispatch + result mapping rather than re-implementing it.
4. **The injected effects** — `Arc<ProviderRegistry<H>>` + `Arc<InvokeVerb<T>>`
   (or the raw `Arc<T: ToolExecuteDyn>` the invoke verb wraps). Tests inject
   the mock provider + a mock tool executor.

## §0.5 · The L2→L2 dependency (RESOLVED · layering-legal)

`verb-agent → verb-invoke` is a SAME-layer dependency. Verified against
`scripts/ci/check-layering.sh:115` — the gate blocks only STRICTLY-upward
deps (`dep_rank > current_rank`); same-layer is explicitly allowed (« L0.5
mock can use L0.5 kernel, etc. »). The blueprint (`crate-admission-order.md`
step 15 + `NIKA_ROADMAP` S5) prescribes exactly this. **No architectural
blocker.** The alternative (duplicating invoke's tool-ref validation +
dispatch inside agent) would violate DRY for no layering gain.

```text
   future L3 nika-engine ── schedules ──┐
                                        v
   L2  nika-verb-agent   run(AgentInput) → AgentOutput
         ├── Arc<ProviderRegistry>  (infer each turn · tools on request)
         └── Arc<InvokeVerb> / Arc<T: ToolExecuteDyn>  (dispatch tool-use blocks)
         v
   L1.5 nika-providers   L2 nika-verb-invoke   L0.5 nika-kernel-runtime (agent DTOs)
```

## §1 · Public API (admission shape — DRAFT)

```rust
pub struct AgentVerb<H, T> {
    registry: Arc<ProviderRegistry<H>>,
    invoke: Arc<InvokeVerb<T>>,
    defaults: AgentDefaults, // default model · max_turns(10) · max_tokens
}

#[non_exhaustive]
pub struct AgentInput {
    pub prompt: String,                    // required (spec §agent · same as infer)
    pub system: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<String>,                // whitelist · DEFAULT-DENY (empty = no tools)
    pub max_turns: Option<u32>,            // default 10
    pub max_tokens_total: Option<u64>,
    pub temperature: Option<f32>,
    pub schema: Option<serde_json::Value>, // validates the FINAL message
}

#[non_exhaustive]
pub enum AgentValue { Text(String), Structured(serde_json::Value) }

#[non_exhaustive]
pub struct AgentOutput {
    pub output: AgentValue,
    pub stop_reason: AgentStopReason,
    pub turns: u32,
    pub total_tokens: u64,
}

impl<H, T> AgentVerb<H, T> {
    pub async fn run(&self, input: AgentInput) -> Result<AgentOutput, VerbAgentError>;
}
```

## §2 · Loop semantics (spec §agent · normative)

The loop: model response → if tool-use blocks present, dispatch each via
invoke, feed results back as tool-result messages → repeat. Terminates on:

1. **Model returns no tool calls** → `Completed` · `success`.
2. **`nika:done` sentinel** (a tool in the whitelist) → `ExplicitCompletion`
   · `success`. Optional `result:` arg → `.output` is that JSON value
   (schema validates IT); absent → `.output` is the final assistant text.
3. **`max_turns` reached** → `TurnsLimit` · **`status: failure`** ·
   `NIKA_460` (spec NIKA-AGENT-001 · budget_error) · last assistant message
   preserved at `error.details.partial_output`.
4. **`max_tokens_total` exhausted** → `TokensLimit` · failure · `NIKA_461`
   (spec NIKA-AGENT-002).

**Tool errors are fed back, not fatal** (the agentic convention): a failing
tool call returns its typed error to the model as the tool result; the loop
continues against its budgets. The ONE exception: a **whitelist violation**
(`security_error`) fails the task IMMEDIATELY → `NIKA_462` — security
boundaries are not model-negotiable.

## §3 · The tool whitelist (default-deny · glob)

`tools:` is gitignore-style globs (`nika:read` exact · `mcp:browser/*` ·
`nika:*`). DEFAULT-DENY: absent/empty → the agent gets NO tools (pure
conversation · least-privilege). Each model tool-use block is matched against
the whitelist BEFORE dispatch; a non-match is the immediate-fail
`security_error` (NIKA_462), NOT a fed-back tool error. `nika:done` is valid
ONLY inside an agent whitelist (the loop sentinel).

## §4 · Error model (DRAFT · NIKA_460-469)

| Code | Variant | Spec | transient |
|---|---|---|---|
| NIKA_460 | `MaxTurns { turns, partial_output }` | NIKA-AGENT-001 | `false` |
| NIKA_461 | `MaxTokens { total_tokens, partial_output }` | NIKA-AGENT-002 | `false` |
| NIKA_462 | `WhitelistViolation { tool }` | security_error (NIKA-SEC-002 family) | `false` |
| NIKA_463 | `Inference` (wraps provider error mid-loop) | — | inherited |
| NIKA_464 | `SchemaValidation` (final message vs schema) | — | `false` |
| NIKA_465 | `InvalidParam` (empty prompt · temp range) | — | `false` |

## §5 · Scope fences

- **Reflection / compression / ReWOO** — `AgentLoopConfig` exposes them, but
  v0.1 ships **ReAct only**, no reflection, no compression (the DTOs are
  forward-compat seams · spec §agent v0.1 is the basic loop). Gate them off.
- **Parallel tools** — sequential dispatch at v0.1 (`parallel_tools = false`).
- **`${{ }}` resolution / glob compilation** — `tools:` globs are matched
  here, but `${{ }}` in prompt/system is upstream-resolved.
- **Cost/duration limits** — `CostLimit`/`DurationLimit` stop reasons exist in
  the kernel enum but are engine-scheduler concerns at v0.1.

## §6 · Testing strategy (when implemented)

Mock-first (mock provider scripted to emit tool-use then a final message ·
mock tool executor): single-turn no-tools → Completed · tool-use → dispatch →
feed-back → final → Completed · `nika:done` with/without `result:` →
ExplicitCompletion · max_turns → NIKA_460 + partial_output · max_tokens →
NIKA_461 · whitelist violation → immediate NIKA_462 (zero dispatch of the
denied tool) · tool error fed back (loop continues) · final-message schema
validation. Property: whitelist glob matcher totality. Mutation ≥90%. Parity
vs brouillon agent loop.

## §7 · Wiring pass (when admitted)

`.gitignore` lift · `Cargo.toml` members + `layers.nika-verb-agent = "L2"` +
wip · `deny.toml` tokio wrapper · NIKA_460-469 registered in
`nika-error/codes.rs` + `verb_help` · kernel hub doc row.

## §8 · Why implementation is DEFERRED (not skipped) — + the BLOCKER found

The agent verb is the most complex of the four — a stateful multi-turn loop
with budget tracking, the `nika:done` sentinel protocol, whitelist-glob
security gating, and feed-back-vs-fatal error semantics. Per Diamond
discipline (`diamond-discipline.md` Rule 6 · `session-discipline.md`
anti-patinage · quality > speed · « no rushing »), this crate is authored as
a complete Gate-1 SPEC and its implementation is left to a focused session.

### ⛔ The missing seam (found 2026-06-11 · verify-the-seam-first paid off)

A second-pass empirical check before coding surfaced a REAL upstream gap, not
just complexity: **there is no tool-definition source in the workspace.** To
*give* the model its whitelisted tools, the agent must build
`nika_kernel_ai::ToolDef { name, description, parameters: <JSON Schema> }`
(`provider.rs:104`) for each — the model needs the description + parameter
schema to call a tool. But the agent only holds tool **names** (the whitelist).
Resolving name → `ToolDef` is unsourced today:

- **Builtins** (`nika:*`) — schemas live in the spec/`nika-catalog`, but
  `nika-catalog` exposes NO `ToolDef`-shaped getter (grep-verified · it has
  provider/pricing rows, not builtin tool schemas).
- **MCP** (`mcp:server/*`) — schemas come from the live server's `tools/list`
  (a runtime MCP-client call), which needs an MCP client surface not yet
  admitted (`nika-mcp` is step 18, excluded).

So the agent loop has an unresolved dependency: a **`ToolDefinitionProvider`**
(name → `ToolDef`) seam — a NEW kernel trait OR a `nika-catalog` method +
`nika-mcp` `tools/list`. That is an **architecture decision** (ASK-class per
the Question-First doctrine), not a verb implementation. Building the loop now
would mean inventing or stubbing that seam — exactly the fragile shortcut the
deferral avoids.

**Resolution for the next session** · decide the `ToolDefinitionProvider`
shape FIRST (likely: a kernel trait the wiring layer implements over
`nika-catalog` for builtins + `nika-mcp` for MCP), admit it, THEN the agent
loop has a clean seam to consume. The loop logic itself (turns · sentinel ·
whitelist · budgets) is fully designed above and unblocked once tool-defs
resolve. The three single-shot verbs (infer · exec · invoke) needed no such
seam — they shipped this arc; the loop verb is gated on this one decision.

## §9 · Update log

```
2026-06-11  v0.1 — Gate 1 SPEC authored (night arc · s12) · seam verified
              (kernel agent.rs DTOs · spec §agent loop semantics · L2→L2
              dep resolved layering-legal · brouillon read-only reference) ·
              implementation deliberately deferred per Diamond §8.
```
