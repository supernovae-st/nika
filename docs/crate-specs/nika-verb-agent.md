# Crate spec — `nika-verb-agent`

| | |
|---|---|
| Status | **SPEC** (Gate 1 · authored 2026-06-11 · announce-ladder step s12 · night arc · the 4th and LAST verb · **impl BLOCKED on a `ToolDefinitionProvider` seam** — see §8 ⛔ · not just deferred for time) |
| Layer | **L2** — verb crate · domain executor for the `agent` verb (4th of the 4 verbs · D-2026-05-22-N18) |
| Design | the multi-turn agentic loop · consumes `nika-providers` (L1.5 · inference) **+ `nika-verb-invoke`** (L2 · tool dispatch · same-layer dep — layering-legal, §0.5) · drives infer → tool-calls → infer until a terminal condition |
| LOC budget | ≤4k src (the most complex verb · brouillon agent loop was the largest verb) · caps ≤1500/file · ≤100/fn · ≤15k/crate |
| Crate version | tracks workspace (`0.90.0`) |
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

   > **Config-shape reconciliation (OPEN · 2026-06-13).** `AgentLoopConfig`
   > is a SPECULATIVE forward-compat DTO — grep-verified **consumed by
   > nobody** (defined in `kernel-runtime/src/agent.rs`, re-exported by the
   > `nika-kernel` facade, read by zero crates). The SHIPPED config is two
   > types it predates and overlaps: `AgentInput` (the public spec fields ·
   > `max_turns`/`max_tokens_total`/…) + `AgentConfig` (ADR-096/094
   > engine-internal tuning · router/guard/`max_parallel_tools`). So
   > `AgentLoopConfig::{parallel_tools, reflection}` now read as the OPPOSITE
   > of reality (parallel is on · reflection is the bounded nudge). When the
   > kernel DTO is finally wired (L3 composer), it must be reconciled with
   > the shipped pair — NOT layered on top as a third config vocabulary
   > (parallel-taxonomy trap). Tracked here rather than silently left to
   > drift.
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

Two of the original v0.1 fences were AMENDED by engine-internal ADRs after
admission (each byte-transparent to the public spec §agent · zero YAML
surface). The list below is the CURRENT reality, not the admission draft.

- **Reflection** — AMENDED by **ADR-096**: the loop ships ONE bounded,
  deterministic corrective nudge on detected no-progress (cycle / error
  streak · `GuardConfig::max_reflections`, default 1). NOT the open
  `ReflectionConfig` self-evaluation loop the kernel DTO sketches — that
  stays fenced.
- **Parallel tools** — AMENDED by **ADR-097**: one turn's batch resolves
  CONCURRENTLY (`AgentConfig::max_parallel_tools`, default 8), results
  folded in REQUEST order. The transcript, guard signature, and event
  stream are byte-identical to sequential, so the public §agent contract
  is unchanged; `max_parallel_tools: 1` restores strict sequencing. (The
  kernel `AgentLoopConfig::parallel_tools: bool` seam predates this — see
  §0 note.)
- **Compression / ReWOO planning** — STILL fenced. v0.1 never drops or
  summarizes transcript history (lossy · v0.2), and never plans the whole
  trajectory ahead (changes the author-observable ReAct contract). The
  kernel DTOs (`CompressionPolicy` · `PlanningStrategy::ReWoo`) are
  forward-compat seams · gate them off.
- **`nika:compose` self-check intrinsic** — RESOLVED (2026-06-13). The
  agent's draft→check→repair tool is a loop-only `nika:` builtin (the
  23rd · Introspection · sibling to `nika:done`), catalogued in
  `nika-catalog`, rejected standalone in `nika-builtin`
  (NIKA-BUILTIN-COMPOSE-001), and DOCUMENTED in the public spec
  (`stdlib/builtins-v0.1.md` · `02-verbs` §agent). It respects the closed
  `{nika:, mcp:}` namespace set; the earlier `agent:compose` (a third
  namespace) is gone, and the agent whitelist now enforces the closed set
  at parse (`validate_whitelist_namespace`).
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
2026-06-12  v0.2 — ADR-096 intelligence layer (engine-internal · ZERO new
              YAML · spec §agent untouched). Four deterministic mechanisms,
              each arXiv-grounded + property-proven through the REAL loop
              (tests/research_conformance.rs · 10 e2e + unit proptests):
              · guard.rs — windowed max-repeats cycle detection over
                action+OBSERVATION turn signatures (polling-proof) · ONE
                bounded Reflexion nudge (arXiv:2303.11366) then NIKA-467
                Stalled{period, repeats} (TRAIL failure class ·
                arXiv:2505.08638) · key-order-canonical JSON hashing.
              · router.rs — per-turn BM25 tool routing via nika-bm25
                (L2→L1 · MCP-Zero direction arXiv:2506.01056 · sovereign
                zero-LLM) · passthrough <24 defs · pinned intrinsics +
                sentinel + recency ≤2 turns · FAIL-OPEN on zero overlap.
              · intrinsic.rs — agent:compose loop-served gate: full
                nika-schema check verdict + AARA certificate fed back as
                JSON (PCE «generation is not permission» arXiv:2605.24462
                · CodeAct arXiv:2402.01030 · AWM direction arXiv:2409.07429)
                · 256 KiB draft cap · poison-shadow-proof (upstream agent:*
                defs dropped · loop synthesizes its own) · NEVER executes.
              · observe.rs — AgentObserver 4th seam (Arc<dyn> · telemetry
                off the data path) · 10 AgentEvent decision payloads → the
                5 nika-event agent_* kinds (AgentOps arXiv:2411.05285) ·
                ToolSource closed-namespace classifier (skill: forward seam).
              New deps nika-bm25 + nika-schema (downward · layering-legal) ·
              NIKA-467 registered · errors.rs Stalled terminal-not-transient ·
              config.rs AgentConfig{router, guard} embedder tuning · lib.rs
              1482 ≤ 1500 · run() 96 ≤ 100 · clippy 0 · 71 tests GREEN ·
              full record docs/adr/adr-096-agent-loop-intelligence.md.
2026-06-12  v0.3 — 2-lens review fold (rust-pro + rust-security · 2×P1 +
              1×P1-sec + 6×P2, all folded same-arc):
              · REACH INVARIANT (P1) — window 16 made period-4 cycles
                structurally unstoppable (floor(16/4) < stall_after 5):
                default window → 25 (= stall·5) + Guard::new clamps
                window ≥ stall_after·5 (a misconfigured window can never
                disarm detection) · period-4-stalls + tiny-window-clamp
                tests · proptest widened to period 1..=5;
              · wire-shape (P1) — the nudge was a SECOND adjacent
                Role::User message (the anthropic wire serializes verbatim
                → non-alternating-roles rejection class): the nudge now
                rides as a trailing ContentBlock::Text INSIDE the same
                tool-results user message (legal on every wire);
              · certificate amplification (P1-sec) — compose feedback
                embedded the FULL RunCertificate (derivation = one row PER
                TASK → 256 KiB draft ⇒ ~MB feedback re-riding every
                transcript clone): certificate_summary() emits bounded
                scalars only (same .len() discipline as sibling fields);
              · error-streak excludes intrinsics (a compose `invalid` is
                the EXPECTED repair feedback, not a tool fault — it no
                longer spends the reflection budget) · max_turns gates
                BEFORE the last batch dispatches (mirrors the token gate's
                no-wasted-side-effects) · routing_query budgets per
                component (a long prompt can't evict the live tail) ·
                run_compose under spawn_blocking (the ocr/jq precedent) ·
                build_request consumes defs (double-clone removed) ·
                digest space-joined (no phantom seam tokens) · observer
                doc: bracketing NOT guaranteed on cancellation.
              run() 94 ≤ 100 (infer_turn extracted) · 77 tests GREEN ·
              clippy 0 · the non-issues (SipHash collisions valueless ·
              router has zero security authority · classify_turn order ·
              cancel-safety) verified + documented by the review.
2026-06-12  v0.4 — run_observed(input, &dyn AgentObserver): the L3 wiring
              seam (additive · run() delegates to the stored observer).
              A wave dispatches CONCURRENT agent tasks through ONE shared
              verb — a verb-wide observer interleaves their decision
              streams; the per-call observer keeps each run attributable.
              Consumed by nika-runtime's agent_events adapter (per-dispatch
              BufferingObserver → Dispatched → RanTask → settle drain onto
              the canonical stream · e2e-proven in the runtime's
              tests/agent_telemetry.rs). API baseline +1, zero removals.
2026-06-12  v0.5 — ADR-097 parallel intra-turn dispatch (amends the spec
              §5 « sequential dispatch » fence · engine-internal · zero
              YAML). run_batch = two phases: CONCURRENT resolve
              (buffered(max_parallel_tools) · yields in INPUT order ·
              pure: no observer, no router) then SEQUENTIAL fold (request
              order: telemetry · recency ledger · guard signature).
              Transcript + signature + event stream byte-identical to
              sequential; max_parallel_tools: 1 restores it exactly;
              cancel = drop per seam contracts. Grounded: LLMCompiler
              (Kim et al. 2023 · arXiv:2312.04511) · ReWOO (Xu et al.
              2023 · arXiv:2305.18323). Proof: rendezvous executor
              (every call waits until 2 in flight — sequential dispatch
              DEADLOCKS there · 5s timeout makes the hang loud) + fed-back
              blocks AND ToolCompleted events asserted in request order.
              with_observer doc honesty fix (review F4): run_observed
              callers — the runtime included — REPLACE it, no tee.
2026-06-13  v0.6 — agent:compose → nika:compose (the compose intrinsic
              becomes the 23rd `nika:` builtin · operator decision · the
              spec's tool-namespace set is CLOSED at {nika:, mcp:} and a
              third `agent:` namespace violated it, only "working" through
              a gap in the agent whitelist's namespace check). The
              `nika:done` precedent dictated the shape: a loop-served tool
              is a `nika:` builtin marked loop-only. Done across the
              catalog stack: nika-catalog ALL_BUILTINS 22→23 (compose ·
              Introspection · the static sibling of inspect) · nika-builtin
              core_tools::compose() standalone-rejection (NIKA-BUILTIN-
              COMPOSE-001) + defs.rs model-facing def · nika-schema codegen
              enum 22→23 (auto-mirrors) + the parser now enforces the
              closed namespace set on agent whitelists (validate_whitelist_
              namespace · catches a stray agent:compose with a pointer to
              nika:compose) · nika-verb-agent COMPOSE_TOOL + is_loop_owned
              {done, compose} + ToolSource collapsed to {Builtin, Mcp,
              Other} (Skill/Intrinsic removed · they anticipated namespaces
              the spec lacks). Public spec updated in lockstep (canon.yaml
              · builtins-v0.1.md · workflow.schema.json enum · 02-verbs §agent
              · CHANGELOG). 100+ tests GREEN across the 5 engine crates ·
              the §5 OPEN question is RESOLVED.
```
