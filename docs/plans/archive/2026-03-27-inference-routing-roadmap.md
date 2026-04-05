# Inference Routing Roadmap — Levels 1→6

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan.
> Each level is a standalone deliverable. Execute one level at a time.

**Vision:** Transform Nika from a workflow engine into an **intelligent inference orchestrator** that routes every LLM call to the optimal backend — based on speed, cost, quality, and task requirements.

**Architecture:** 6 progressive levels, each building on the previous. Level 1 (custom endpoints) is the foundation. Level 6 (fleet management) is the ceiling. Each level is independently useful.

**Key insight:** Nika sees the **workflow DAG**. It knows which tasks are on the critical path, which are intermediate, which need structured output, which need vision. No proxy (LiteLLM, Portkey) has this context. This is the moat.

---

## Table of Contents

1. [Level 1: Custom Endpoints](#level-1-custom-endpoints) — Connect to any backend
2. [Level 2: nika bench](#level-2-nika-bench) — Measure and compare
3. [Level 3: Fallback Chains](#level-3-fallback-chains) — Resilient routing
4. [Level 4: Smart Routing](#level-4-smart-routing) — Per-task intelligence
5. [Level 5: Auto-Optimization](#level-5-auto-optimization) — Data-driven config
6. [Level 6: Fleet Management](#level-6-fleet-management) — Multi-GPU orchestration
7. [Daemon Integrations](#daemon-integrations) — Background intelligence
8. [TUI Routing Dashboard](#tui-routing-dashboard) — Live visualization
9. [Smart Combos](#smart-combos) — Cross-crate synergies
10. [CLI Output Design](#cli-output-design) — Nika aesthetic guide
11. [GPU Compatibility Matrix](#gpu-compatibility-matrix) — What fits where
12. [Deployment Architectures](#deployment-architectures) — 5 modes
13. [UX Philosophy](#ux-philosophy) — Progressive disclosure

---

## Level 1: Custom Endpoints

**Status:** Plan written at `docs/plans/2026-03-27-custom-endpoints.md` — being executed.

**Goal:** Connect Nika to any OpenAI-compatible inference server.

**Deliverables:**
- `base_url` field in YAML (workflow + task level)
- Named endpoints in `config.toml`
- `OPENAI_BASE_URL` env var (already works via rig-core)
- `RigProvider::OpenAiCompat` variant
- NIKA-035/036 error codes
- `nika provider list` shows endpoints

**See full plan:** `docs/plans/2026-03-27-custom-endpoints.md` (15 tasks, 7 phases)

---

## Level 2: nika bench

**Goal:** Run a workflow across multiple providers, collect metrics, display comparison.

**Depends on:** Level 1 (custom endpoints)

**Metrics source:** All from existing EventLog — zero new instrumentation needed.

```
RunStats (already exists):
├── ttft_values: Vec<u64>           → percentile ladder
├── total_input_tokens: u64         → token comparison
├── total_output_tokens: u64        → throughput calc
├── total_cost: f64                 → cost comparison
├── provider_calls: Vec<ProviderCallStat>  → per-task breakdown
└── task_timeline: Vec<(id, verb, start_ms, duration_ms)>  → Gantt
```

### CLI Output Design

```
╭──────────────────────────────────────────────────────────────────╮
│                                                                  │
│  N I K A   B E N C H                                     v0.50  │
│                                                                  │
│  research-pipeline.nika.yaml · 5 tasks · 3 iterations           │
│                                                                  │
╰──────────────────────────────────────────────────────────────────╯

  ── Speed ─────────────────────────────────────────────────────────

  Provider       TTFT [p50, p90, p99]        Tok/s     Total
  ⋈ anthropic     230ms  340ms  510ms         85/s      12.4s
  ⋈ h100          45ms   62ms   89ms        142/s       7.1s ✧ fastest
  ⋈ native         12ms   15ms   18ms         38/s      26.8s

  h100 ran 1.7× faster than anthropic, 3.8× faster than native

  ── Cost ──────────────────────────────────────────────────────────

  Provider       $/run     Input      Output     Cache
  ⋈ anthropic    $0.0234   1.2k tok   420 tok    200 tok
  ⋈ h100         ~$0.006   1.2k tok   380 tok    —
  ⋈ native       $0.000    1.2k tok   350 tok    —

  h100 saves 74% vs anthropic ($0.017/run)

  ── Profile ───────────────────────────────────────────────────────

  Task            anthropic    h100        native      Verb
  research        ████ 3.4s    ██ 1.2s     ████████ 8.1s   ✧ infer
  scrape[×10]     ███ 4.2s     ███ 4.2s    ███ 4.2s        ☄ fetch
  analyze         ██ 2.1s      █ 0.8s      █████ 5.2s      ✧ infer
  summarize       ██ 2.7s      █ 0.9s      ██████ 6.3s     ✧ infer
                  ─────────    ─────────   ─────────
  LLM only        8.2s         2.9s        19.6s
  Non-LLM         4.2s         4.2s        4.2s

  Bottleneck: scrape[×10] is 59% of h100 total (fetch, not LLM)
  → Tip: increase concurrency before switching provider

╭──────────────────────────────────────────────────────────────────╮
│                                                                  │
│  ✓  B E N C H   C O M P L E T E                          24.1s │
│                                                                  │
│  Verdict: h100 — best speed/cost ratio                          │
│  1.7× faster · 74% cheaper · similar quality                    │
│                                                                  │
╰──────────────────────────────────────────────────────────────────╯
```

### Architecture

```rust
// Bench runs N iterations × M providers
// Reuses Runner API exactly as tests do

async fn run_bench(yaml: &str, providers: &[String], iterations: usize) {
    let parsed = parse_workflow(yaml)?;
    let base = expand_includes(parsed, &base_path)?;

    for provider_name in providers {
        let mut iteration_stats = Vec::new();
        for i in 0..iterations {
            // Fresh workflow per iteration (override provider)
            let mut workflow = nika::ast::unlower(base.clone())?;
            workflow.provider = Some(provider_name.clone());

            // NOTE: If provider is a named endpoint (e.g., "h100"),
            // NikaConfig::resolve_endpoints() is called inside Runner::new()
            // which passes the CustomEndpointMap to TaskExecutor.
            // The provider name "h100" is resolved at runtime, not here.

            let event_log = EventLog::new();
            let mut runner = Runner::with_event_log(workflow, event_log.clone())?
                .quiet();
            runner.run().await?;

            // IMPORTANT: RunStats does NOT have from_events().
            // Must replay events manually:
            let mut stats = RunStats::default();
            event_log.with_events(|events| {
                for event in events {
                    stats.apply_event(event);
                }
            });
            iteration_stats.push(stats);
        }
        aggregate_and_store(provider_name, iteration_stats);
    }
    display_comparison_table(all_results);
}
```

### Known Constraints

- **`RunStats::apply_event()`** is incremental — no `from_events()` constructor exists. Replay events manually.
- **`ProviderCallStat` has no `provider_name` field** — must join `ProviderCalled.provider` + `ProviderResponded` by `task_id` to attribute metrics to providers. Either add a `provider` field to `ProviderCallStat` (preferred) or join at display time.
- **Token counts are provider-reported** — different providers count tokens differently (Anthropic includes special tokens, vLLM may not). Note this in bench output as a caveat.
- **Named endpoints need `hourly_rate`** for cost estimation — add `hourly_rate: Option<f64>` to `CustomEndpointConfig`.
- **Rate limits** — custom endpoints may return 429 differently from cloud providers. Bench should retry with exponential backoff on 429, immediate failure on 5xx.

### Key Tasks

**Phase 1 — CLI command + runner loop (6 tasks)**

| # | Task | Files |
|---|------|-------|
| 1 | Add `Bench` variant to CLI Commands enum | `tools/nika/src/main.rs` |
| 2 | Create `tools/nika-cli/src/bench.rs` with argument parsing | NEW file |
| 3 | Implement bench loop: parse → override → run → collect | `bench.rs` |
| 4 | Aggregate RunStats across iterations (mean, p50, p90, p99) | `bench.rs` |
| 5 | Display comparison table using existing display helpers | `bench.rs` |
| 6 | Add `--iterations`, `--providers`, `--profile`, `--json` flags | `main.rs` |

**Phase 2 — Rich display (4 tasks)**

| # | Task | Files |
|---|------|-------|
| 7 | Implement speed section (TTFT percentiles, tok/s, total) | `bench.rs` |
| 8 | Implement cost section ($/run, token breakdown) | `bench.rs` |
| 9 | Implement profile section (per-task Gantt bars, bottleneck) | `bench.rs` |
| 10 | Implement summary box with verdict + relative comparison | `bench.rs` |

**Phase 2b — Data model fixes (3 tasks)**

| # | Task | Files |
|---|------|-------|
| 10b | Add `provider: String` field to `ProviderCallStat` + populate from `ProviderCalled` event | `display/renderer.rs` |
| 10c | Add `hourly_rate: Option<f64>`, `currency: Option<String>` to `CustomEndpointConfig` | `provider/endpoints.rs`, `config.rs` |
| 10d | Persist bench results to `.nika/bench-cache/<workflow_hash>.json` (keyed by hash+provider) | `bench.rs` |

**Phase 3 — Quality evaluation (4 tasks)**

| # | Task | Files |
|---|------|-------|
| 11 | Add `--eval` flag, `--eval-model` option (default: `claude-haiku-4-5`) | `main.rs`, `bench.rs` |
| 12 | Implement LLM-as-judge (reuse guardrail `type: llm` pattern). Only eval final-output tasks, not intermediates (avoid latency/cost bloat) | `bench.rs` |
| 13 | Score comparison: 1-10 scale per provider, display inline | `bench.rs` |
| 14 | Quality section in output with bar charts | `bench.rs` |

**Phase 4 — Export + persistence (2 tasks)**

| # | Task | Files |
|---|------|-------|
| 15 | `--json` flag: export full bench results as JSON (for Level 5 auto-optimization) | `bench.rs` |
| 16 | Auto-persist results to `.nika/bench-cache/` with workflow hash key. Read cache for Level 4 bootstrap. | `bench.rs` |

**Estimated cost field for local providers:**

```toml
# config.toml
[endpoints.h100]
base_url = "http://10.0.1.42:8000/v1"
hourly_rate = 3.00    # EUR/h — used by nika bench for cost estimation
currency = "EUR"
```

```
estimated_cost = (workflow_duration_secs / 3600) × hourly_rate
```

---

## Level 3: Fallback Chains

**Goal:** Try cheap provider first, fall back to expensive on failure or low quality.

**Depends on:** Level 1 (endpoints), Level 2 (bench data for thresholds)

### YAML Syntax

```yaml
schema: "nika/workflow@0.12"
model: Qwen/Qwen2.5-72B-AWQ

routing:
  strategy: fallback
  chain: [h100, anthropic]     # Try h100 first, fallback to anthropic
  retry_on:
    - error                     # Network/timeout errors
    - structured_failure        # Structured output validation failed
    - quality_below: 7.0        # LLM-as-judge score < 7 (requires eval model)

tasks:
  - id: analyze
    infer: "Analyze {{inputs.data}}"
```

### Per-task override

```yaml
tasks:
  - id: critical_output
    routing:
      chain: [anthropic, openai]  # Cloud-only for critical tasks
    infer: "Final report"

  - id: bulk_classify
    routing:
      chain: [native, h100, groq]  # Cheap → medium → fast cloud
    for_each:
      items: "{{with.data}}"
      concurrency: 10
    infer: "Classify: {{with.item}}"
```

### CLI Output (fallback triggered)

```
  ✧ analyze
    ⋈ h100 ──────── ✗ timeout (5.2s)
    ↯ fallback → anthropic
    ⋈ anthropic ──── ✓ 3.1s · 420 tok · $0.012
```

### Architecture

**Routing lives at workflow/task level, NOT inside InferParams.**

```
// AST layer (nika-core) — routing config as strings only
RawWorkflow {
    routing: Option<Spanned<RawRoutingConfig>>,   // NEW
}
RawTask {
    routing: Option<Spanned<RawRoutingConfig>>,   // NEW (override)
}

AnalyzedWorkflow {
    routing: Option<RoutingConfig>,               // Validated
}
AnalyzedTask {
    routing: Option<RoutingConfig>,               // Override
}

// Runtime layer (nika-engine) — resolved at execution time
RoutingConfig {
    strategy: RoutingStrategy,         // Fallback | Smart | Fleet
    chain: Vec<String>,                // Provider names in order
    retry_on: Vec<RetryCondition>,     // When to try next
}

RetryCondition {
    Error,                              // Any provider error (5xx, connection)
    Timeout,                            // Timeout specifically
    RateLimit,                          // 429 — use exponential backoff before fallback
    StructuredFailure,                  // All 5 structured output layers failed
    QualityBelow(f64),                  // LLM-as-judge (opt-in, final tasks only)
}
```

**Resolution happens in the executor**, not the analyzer. Named endpoints (e.g., "h100") are resolved from `NikaConfig.endpoints` at runtime via `RigProvider::from_name_with_endpoints()`.

### Interaction with existing `retry:` config

The task-level `retry:` config (max_attempts, delay_ms, backoff) operates **inside** a single provider attempt. The fallback chain operates **across** providers:

```
Task retry (existing):     Provider A: attempt 1 → fail → attempt 2 → fail → attempt 3 → FAIL
Fallback chain (new):      Provider A FAIL → Provider B: attempt 1 → ... → Provider C: ...

Combined:
  Provider A: retry 3× → all fail → FallbackTriggered
  Provider B: retry 3× → success on attempt 2 → DONE
```

The existing `retry:` runs first (within one provider). Only after all retries are exhausted does the fallback chain advance to the next provider.

### StructuredFailure detection

The `StructuredOutputEngine` returns `NikaError::StructuredOutputAllLayersFailed` when all 5 layers fail. The fallback loop pattern-matches on this specific variant:

```rust
match result {
    Err(NikaError::StructuredOutputAllLayersFailed { .. }) if has_next_provider => {
        // Fallback to next provider in chain
    }
    Err(e) if is_retriable(&e) && has_next_provider => {
        // Generic error fallback
    }
    other => other, // Propagate success or final failure
}
```

### New Error Codes

| Code | Variant | Trigger |
|------|---------|---------|
| NIKA-037 | `FallbackChainExhausted` | All providers in chain failed |
| NIKA-038 | `RoutingBudgetExceeded` | Dollar budget used up (Level 4) |
| NIKA-039 | `NoCapableProvider` | No provider matches task requirements (Level 4) |

### Key Tasks (12 tasks)

| # | Task | What |
|---|------|------|
| 1 | Add `RoutingConfig`, `RetryCondition` to nika-core AST | Raw + Analyzed types |
| 2 | Parse `routing:` from YAML (workflow + task level) | parser.rs, known fields |
| 3 | Thread through analyzer (name strings only, no endpoint resolution) | analyze.rs |
| 4 | Thread routing config to executor via Runner (NOT in InferParams/AgentParams — pass separately) | runner.rs, executor/mod.rs |
| 5 | Implement fallback loop in `executor/infer.rs` wrapping existing `get_rig_provider()` call | Replace single provider call |
| 6 | Implement fallback loop in `executor/agent.rs` wrapping `RigAgentLoop` provider dispatch | Thread custom client into agent loop |
| 7 | Add `FallbackTriggered` event to EventLog | nika-event/src/log.rs |
| 8 | Pattern-match `StructuredOutputAllLayersFailed` for `RetryCondition::StructuredFailure` | executor/infer.rs |
| 9 | Implement `RetryCondition::RateLimit` with exponential backoff (jitter) before fallback | executor/infer.rs |
| 10 | Add NIKA-037 `FallbackChainExhausted` error code | error_domains.rs |
| 11 | Display fallback events in CLI renderer (live + classic) | display/live.rs, format_event.rs |
| 12 | Tests: mock provider chain, structured failure fallback, chain exhaustion, rate limit backoff | Integration tests with mock |

### New Events

```rust
EventKind::FallbackTriggered {
    task_id: Arc<str>,
    from_provider: String,
    to_provider: String,
    reason: String,           // "timeout", "structured_failure", "quality_below:6.2"
    attempt: u32,
}
```

---

## Level 4: Smart Routing

**Goal:** Nika automatically selects the best provider per task based on task characteristics.

**Depends on:** Level 1 + Level 3 (fallback as safety net)

### YAML Syntax

```yaml
schema: "nika/workflow@0.12"

routing:
  strategy: smart
  budget: 0.10                 # Max $0.10 per workflow run
  priority: cost               # cost | speed | quality | balanced
  providers:
    h100:
      capabilities: [text, json, fast]
      cost_per_1k_input: 0.001
      cost_per_1k_output: 0.004
    anthropic:
      capabilities: [text, json, vision, reasoning, long_context]
      # cost from built-in pricing table
    native:
      capabilities: [text, offline]
      # cost: 0

tasks:
  - id: classify
    infer: "Classify: {{inputs.text}}"
    # → smart routing picks: native (free, short prompt, no quality need)

  - id: extract
    structured: { schema: { type: object, ... } }
    infer: "Extract entities"
    # → smart routing picks: h100 (json capable, cheap)

  - id: reason
    infer: "Complex multi-doc analysis..."
    # → smart routing picks: anthropic (reasoning, critical path)
```

### Routing Algorithm

```
1. FILTER — Remove providers that can't handle the task:
   - Vision content? → only vision-capable providers
   - Structured output? → only json-capable providers
   - Agent verb? → only tool-calling-capable providers

2. SCORE — Weight remaining providers:
   score = w_cost × normalized_cost
         + w_speed × normalized_speed
         + w_quality × normalized_quality

   Weights from routing.priority:
   ┌───────────┬────────┬────────┬──────────┐
   │ Priority  │ w_cost │ w_speed│ w_quality│
   ├───────────┼────────┼────────┼──────────┤
   │ cost      │  0.6   │  0.2   │  0.2     │
   │ speed     │  0.2   │  0.6   │  0.2     │
   │ quality   │  0.1   │  0.1   │  0.8     │
   │ balanced  │  0.33  │  0.33  │  0.33    │
   └───────────┴────────┴────────┴──────────┘

3. CRITICAL PATH BOOST — If task is on the DAG critical path:
   w_quality *= 1.5  (final outputs matter more)

   NOTE: Critical path = longest weighted path from source to sink.
   Existing compute_depths() only counts hops, not weighted time.
   Need new Dag::critical_path_set() → HashSet<TaskId> that:
   a) Forward pass: compute earliest start time for each task
   b) Backward pass: compute latest start time from sinks
   c) Tasks where earliest == latest are on the critical path

4. BUDGET CHECK — Will this choice exceed remaining budget?
   If yes → force cheapest option

   Budget tracking uses AtomicU64 with micro-dollar fixed-point:
   $0.017 → 17_000 microdollars. Avoids f64 atomicity issues.
   Shared across concurrent for_each via Arc<AtomicU64>.

5. BOOTSTRAP — When no bench data exists for a provider:
   - Use static defaults: speed=medium, quality=medium
   - Cost from built-in pricing table (cloud) or hourly_rate (local)
   - First `nika bench` run populates the cache
   - Subsequent `nika run` reads cache for smart decisions

6. SELECT — Highest score wins
```

### CLI Output (smart routing)

```
  ── Smart Routing ─────────────────────────────────────────────────

  Task            Provider    Reason                         Cost
  ├── classify    native      free, short prompt             $0.000
  ├── scrape[×10] (fetch)     no LLM needed                 —
  ├── extract[×5] h100        json-capable, 5× cheaper      $0.003
  ├── analyze     anthropic   critical path, reasoning       $0.008
  └── summarize   anthropic   critical path, final output    $0.006

  Budget: $0.017 / $0.10 (17% used)
  Routing saved $0.014 vs all-anthropic (-45%)
```

### Key Tasks (15 tasks)

| # | Task | What |
|---|------|------|
| 1 | Define `SmartRoutingConfig` in AST (capabilities, budget, priority) | nika-core AST types |
| 2 | Parse `routing: { strategy: smart, providers: {...} }` from YAML | parser.rs |
| 3 | Define `ProviderCapability` enum: `Text, Json, Vision, Reasoning, ToolCalling, LongContext, Fast, Offline` | nika-core catalogs |
| 4 | Implement capability filtering (remove providers that can't handle task) | New `routing/filter.rs` in nika-engine |
| 5 | Implement `Dag::critical_path_set()` — forward+backward longest-path computation | dag/flow.rs |
| 6 | Implement scoring algorithm with configurable weights | `routing/scorer.rs` |
| 7 | Implement budget tracking with `Arc<AtomicU64>` micro-dollar fixed-point | `routing/budget.rs` |
| 8 | Read bench cache (`.nika/bench-cache/`) for speed/quality scores; fall back to static defaults | `routing/cache.rs` |
| 9 | Wire router into executor: call before `get_rig_provider()` for both `infer:` and `agent:` | `executor/infer.rs`, `executor/agent.rs` |
| 10 | Handle `agent:` verb routing — thread selected provider into `RigAgentLoop` (not just infer) | `executor/agent.rs`, `rig_agent_loop/` |
| 11 | Add `SmartRouteDecision` event to EventLog | nika-event |
| 12 | Add NIKA-038 `RoutingBudgetExceeded`, NIKA-039 `NoCapableProvider` error codes | error_domains.rs |
| 13 | Display routing decisions in live renderer | `display/live.rs` |
| 14 | `nika run --explain-routing` flag + routing summary in run summary | CLI flag, `display/summary.rs` |
| 15 | Tests: scoring, filtering, budget exhaustion, critical path set, agent routing, no bench data bootstrap | Unit + integration |

### New Events

```rust
EventKind::SmartRouteDecision {
    task_id: Arc<str>,
    selected_provider: String,
    reason: String,              // "cheapest with json capability"
    score: f64,                  // Winning score
    alternatives: Vec<(String, f64)>,  // Other providers + scores
    budget_remaining: f64,       // Budget left after this task
}
```

---

## Level 5: Auto-Optimization

**Goal:** `nika optimize` analyzes bench data and generates optimal routing config.

**Depends on:** Level 2 (bench) + Level 4 (smart routing)

### CLI

```bash
nika optimize workflow.nika.yaml \
  --providers anthropic,h100,native \
  --budget 0.05 \
  --min-quality 8.0 \
  --iterations 3
```

### Output

```
╭──────────────────────────────────────────────────────────────────╮
│                                                                  │
│  N I K A   O P T I M I Z E                               v0.52 │
│                                                                  │
│  research-pipeline.nika.yaml                                    │
│  Budget: $0.05/run · Min quality: 8.0/10                        │
│                                                                  │
╰──────────────────────────────────────────────────────────────────╯

  ── Benchmarking ──────────────────────────────────────────────────

  ⠹ Running 3 iterations × 3 providers...
  ⋈ anthropic  ✓✓✓  avg 12.4s · $0.023 · 9.2/10
  ⋈ h100       ✓✓✓  avg  7.1s · $0.006 · 8.5/10
  ⋈ native     ✓✓✓  avg 26.8s · $0.000 · 7.1/10

  ── Optimization ──────────────────────────────────────────────────

  Task            Current     Optimized    Saving    Quality
  ├── classify    anthropic → native       -$0.002   8.1→7.9 (ok)
  ├── extract[×5] anthropic → h100         -$0.008   9.0→8.6 (ok)
  ├── analyze     anthropic → anthropic     $0       9.4 (critical path)
  └── summarize   anthropic → anthropic     $0       9.3 (critical path)

  ── Result ────────────────────────────────────────────────────────

                 Before       After       Delta
  Cost           $0.023       $0.013      -43%
  Time           12.4s        7.8s        -37%
  Quality        9.2/10       8.9/10      -3%

  Critical path tasks stay on Claude (analyze, summarize).
  Bulk/intermediate tasks moved to cheaper providers.

  ── Generated Config ──────────────────────────────────────────────

  routing:
    strategy: smart
    budget: 0.05
    priority: cost
    rules:
      - match: { task: classify }
        provider: native
      - match: { structured: true, critical_path: false }
        provider: h100
      - default:
        provider: anthropic

  Apply this routing? [Y/n] █
```

### Architecture

```
nika optimize = nika bench (all providers)
              + quality evaluation (LLM-as-judge)
              + optimization solver
              + config generator
```

The solver is a simple greedy algorithm:
1. For each task, rank providers by `priority` (cost/speed/quality/balanced)
2. Critical path tasks: boost quality weight
3. Check budget constraint
4. Generate routing rules from assignments

### Bench Result Persistence

```
.nika/bench-cache/
├── 7f3a2b.json     # Keyed by workflow content hash
└── a1b2c3.json     # Each file stores results per provider
```

```json
{
  "workflow_hash": "7f3a2b",
  "timestamp": "2026-03-27T14:30:00Z",
  "results": {
    "anthropic": { "avg_duration_ms": 12400, "avg_cost": 0.023, "avg_quality": 9.2, "ttft_p50_ms": 230 },
    "h100":      { "avg_duration_ms": 7100, "avg_cost": 0.006, "avg_quality": 8.5, "ttft_p50_ms": 45 },
    "native":    { "avg_duration_ms": 26800, "avg_cost": 0.0, "avg_quality": 7.1, "ttft_p50_ms": 12 }
  }
}
```

### Routing Rules DSL

The optimizer generates `rules:` syntax (more expressive than Level 4's static capabilities):

```yaml
routing:
  strategy: smart
  rules:
    - match: { task: "classify*" }          # Glob on task ID
      provider: native
    - match: { structured: true }           # Task has structured: block
      provider: h100
    - match: { critical_path: true }        # On DAG critical path
      provider: anthropic
    - match: { vision: true }               # Has content: with images
      provider: anthropic
    - default:
      provider: h100
```

**NOTE:** The `rules:` DSL must be defined in Level 4's AST before Level 5 can generate it. Add `RoutingRule` struct to nika-core:

```rust
struct RoutingRule {
    match_condition: MatchCondition,
    provider: String,
}

enum MatchCondition {
    TaskGlob(String),         // "classify*"
    HasStructuredOutput,      // structured: block present
    OnCriticalPath,           // DAG analysis
    HasVision,                // content: with images
    Default,                  // Catch-all
}
```

### Key Tasks (10 tasks)

| # | Task | What |
|---|------|------|
| 1 | Add `Optimize` CLI command | `main.rs` |
| 2 | Define `RoutingRule` + `MatchCondition` in nika-core AST | AST types |
| 3 | Parse `rules:` syntax from YAML | parser.rs |
| 4 | Run bench internally (reuse Level 2 code, read cache if fresh) | `optimize.rs` |
| 5 | Run quality eval on final-output tasks only (reuse Level 2 --eval) | `optimize.rs` |
| 6 | Implement greedy solver (per-task assignment, budget constraint, critical path boost) | `optimize.rs` |
| 7 | Generate `routing.rules` config from solver output | `optimize.rs` |
| 8 | Interactive apply with cliclack (show before/after table, confirm) | `optimize.rs` |
| 9 | Write routing config to `.nika/routing.yaml` or inline into workflow | `optimize.rs` |
| 10 | `nika optimize --dry-run` to preview without applying | CLI flag |

---

## Level 6: Fleet Management

**Goal:** Manage multiple GPU servers, load balance, health check, auto-scale.

**Depends on:** Level 1 + Level 3 (fallback on unhealthy node)

### Config

```toml
# config.toml
[fleet]
strategy = "least_busy"     # round_robin | least_busy | latency_based
health_check_interval = 30  # seconds

[fleet.endpoints.gpu-1]
base_url = "http://10.0.1.41:8000/v1"
api_key = "sk-fleet"
model = "Qwen/Qwen2.5-72B-AWQ"

[fleet.endpoints.gpu-2]
base_url = "http://10.0.1.42:8000/v1"
api_key = "sk-fleet"
model = "Qwen/Qwen2.5-72B-AWQ"

[fleet.endpoints.gpu-3]
base_url = "http://10.0.1.43:8000/v1"
api_key = "sk-fleet"
model = "Qwen/Qwen2.5-72B-AWQ"
```

### YAML

```yaml
schema: "nika/workflow@0.12"

routing:
  strategy: fleet
  fleet: gpu-cluster      # References [fleet] in config.toml

tasks:
  - id: process
    for_each:
      items: "{{with.urls}}"
      concurrency: 30          # Distributed across 3 GPUs
    infer: "Process {{with.item}}"
```

### CLI: `nika fleet` (not `nika status` — avoids conflict with daemon status)

```
╭──────────────────────────────────────────────────────────────────╮
│                                                                  │
│  N I K A   F L E E T                                     v0.53  │
│                                                                  │
╰──────────────────────────────────────────────────────────────────╯

  ── gpu-cluster (3 nodes) ─────────────────────────────────────────

  Node        Status    Model              Queue   Tok/s   Latency
  ├── gpu-1   ✓ active  Qwen2.5-72B-AWQ    12 req  142/s   45ms
  ├── gpu-2   ✓ active  Qwen2.5-72B-AWQ     8 req  138/s   48ms
  └── gpu-3   ⚠ slow    Qwen2.5-72B-AWQ    28 req   91/s  112ms

  Aggregate: 371 tok/s · 48 queued · avg 68ms

  ── Health History (30s) ──────────────────────────────────────────

  gpu-1  ▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇  100% healthy
  gpu-2  ▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇  100% healthy
  gpu-3  ▇▇▇▇▇▇▇▅▅▃▃▂▂▂▃▃▅▅▇▇   78% (degraded 12s ago)
```

### Health Check

```rust
// Ping /v1/models every N seconds
async fn health_check(endpoint: &ResolvedEndpoint) -> HealthStatus {
    let url = format!("{}/models", endpoint.base_url);
    match reqwest::get(&url).timeout(Duration::from_secs(5)).await {
        Ok(resp) if resp.status().is_success() => HealthStatus::Healthy,
        Ok(resp) => HealthStatus::Degraded(resp.status().to_string()),
        Err(e) => HealthStatus::Unhealthy(e.to_string()),
    }
}
```

### Load Balancing Strategies

```rust
enum FleetStrategy {
    RoundRobin,       // Simple rotation
    LeastBusy,        // Track in-flight requests, pick lowest
    LatencyBased,     // Track response times, pick fastest
}
```

### Key Tasks (12 tasks)

| # | Task | What |
|---|------|------|
| 1 | Add `[fleet]` section to config.toml schema | `config.rs` |
| 2 | Implement `FleetManager` with endpoint pool | NEW `fleet.rs` |
| 3 | Background health checker (tokio interval) | `fleet.rs` |
| 4 | Round-robin strategy | `fleet.rs` |
| 5 | Least-busy strategy (AtomicU32 in-flight counter) | `fleet.rs` |
| 6 | Latency-based strategy (EMA of response times) | `fleet.rs` |
| 7 | Wire FleetManager into TaskExecutor | `executor/mod.rs` |
| 8 | Handle unhealthy nodes (remove from pool, re-add on recovery) | `fleet.rs` |
| 9 | Add `FleetHealthCheck` event to EventLog | nika-event |
| 10 | Implement `nika fleet` CLI command (subcommands: status, health, scale) | `main.rs`, `fleet_cli.rs` |
| 11 | Display fleet dashboard with sparklines + health history | `fleet_cli.rs` |
| 12 | Tests: round-robin, failover, health recovery | Integration tests |

---

## CLI Output Design

### Nika Aesthetic Rules

All CLI output must follow these conventions (from existing codebase):

**Box Drawing:**
- Rounded corners for top-level containers: `╭ ╮ ╰ ╯`
- Double-line for emphasis/DAG: `╔ ╗ ╚ ╝ ═ ║`
- Light for tables: `┌ ┐ └ ┘ ─ │`
- Tree connectors: `├──` (branch), `└──` (last), `│` (pipe)

**Icons (Cosmic Palette):**
- Verbs: `✧` infer (magenta), `⎈` exec (yellow), `☄` fetch (cyan), `⊛` invoke (green), `❋` agent (red)
- Status: `✓` success (green), `✗` failed (red), `○` pending (dim), `⊘` skipped (dim)
- Subsystems: `⋈` provider (blue), `↯` retry (yellow), `⊞` MCP (green)

**Colors (semantic, max 5 per view):**
- Green: success, fast, cheap
- Red: failure, slow, expensive
- Yellow: warning, moderate
- Cyan: info, fetch, structural
- Dim: secondary, metadata, borders

**Typography hierarchy:**
1. Bold + Color → titles, errors
2. Bold → section headers, key metrics
3. Color (normal) → categorized data
4. Normal → values
5. Dim → metadata, timestamps

**Sparklines:** `▁▂▃▄▅▆▇█` in blue (8 levels)

**Progress bars:** `━╸─` (filled, tip, empty) in cyan

**Duration thresholds:**
- `< 1s` → green
- `1-5s` → yellow
- `> 5s` → red

**Cost thresholds:**
- `< $0.001` → dim
- `< $0.01` → green
- `< $0.10` → yellow
- `>= $0.10` → red bold

**TTFT thresholds:**
- `< 200ms` → green
- `< 500ms` → yellow
- `>= 500ms` → red

**Token display:** `123` / `1.2k` / `42k` / `1.2M`

**Relative comparisons (hyperfine style):**
```
h100 ran 1.7× faster than anthropic
h100 saves 74% vs anthropic ($0.017/run)
```

**Percentile ladder (oha style):**
```
TTFT [p50, p90, p99]  45ms  62ms  89ms
```

**Gantt bars (existing pattern):**
```
  ✧ task_a       ████░░░░░░░░░░░░  0.5s
  ⎈ task_b       ░░░░████████░░░░  0.8s
                  0s         1.5s
```

---

## GPU Compatibility Matrix

### What Models Fit Where

| GPU | VRAM | FP16 Max | AWQ/Q4 Max | Example Models | Platform |
|-----|------|----------|------------|----------------|----------|
| RTX 4090 | 24 GB | ~12B | ~30B | Qwen3-8B, Llama-8B, Mistral-7B | Consumer |
| RTX 5090 | 32 GB | ~16B | ~40B | Qwen-14B, Phi-4 | Consumer |
| Mac M4 Pro | 24 GB | ~12B | ~30B | Same as 4090 but slower | Consumer |
| Mac M4 Max | 64 GB | ~32B | ~80B | Llama-70B Q4, Qwen-72B Q4 | Consumer |
| Mac M4 Ultra | 256 GB | ~128B | ~320B | DeepSeek-V3, Llama-405B Q4 | Consumer |
| L4 | 24 GB | ~12B | ~30B | Qwen3-8B, Mistral-7B | Cloud |
| L40S | 48 GB | ~24B | ~60B | Qwen-32B, Llama-70B Q4 | Cloud |
| A100 | 80 GB | ~40B | ~100B | Llama-70B FP16, Qwen-72B Q4 | Cloud |
| H100 | 80 GB | ~40B | ~100B | Same + faster, PagedAttention | Cloud |
| H200 | 141 GB | ~70B | ~180B | Llama-70B FP16 + room | Cloud |

### Multi-Model Combos (1× H100 80GB)

| Combo | Model A | Model B | VRAM | Use Case |
|-------|---------|---------|------|----------|
| 2× small | Mistral-7B FP16 | Qwen-8B FP16 | ~36 GB | Fast + diverse |
| Small + medium | Llama-8B FP16 | Qwen-14B FP16 | ~54 GB | General purpose |
| Big solo | Qwen-72B AWQ | — | ~48 GB | Best open-source |
| Big + small | Qwen-72B AWQ | Mistral-7B | ~66 GB | Smart + fast |
| 3× small | 7B + 7B + 7B | — | ~54 GB | Max diversity |

### Cloud Pricing (H100 SXM 80GB)

| Provider | Price/h | Notes |
|----------|---------|-------|
| vast.ai | ~$1.53 | Cheapest, marketplace, variable quality |
| Scaleway | ~€2.52 ($2.71) | EU sovereign, GDPR, fr-par-2 |
| RunPod Secure | ~$3.29 | Good UX, spot + on-demand |
| Lambda Labs | $3.99 | Best DX, no egress fees |

### Inference Server Comparison (2025)

| Feature | vLLM | SGLang | TGI v3 | Ollama |
|---------|------|--------|--------|--------|
| Throughput | Excellent | Best (+29%) | Very good | Basic |
| Long context (200k+) | Slow | Good | Best (13×) | N/A |
| Continuous batching | Yes | Yes | Yes | No |
| Multi-model | Multi-process | Multi-process | Multi-process | Built-in |
| Structured output | xgrammar | LMFE | Outlines | No |
| LoRA hot-swap | Yes (API) | Yes | No | No |
| PagedAttention | Inventor | RadixAttention | Adopted | No |
| Production maturity | Highest | Growing | High | Dev only |
| Protocol | OpenAI API | OpenAI API | OpenAI API* | OpenAI API |

**Recommendation by use case:**
- **Production multi-model**: vLLM + LiteLLM proxy
- **Max throughput**: SGLang (single GPU)
- **Long context (200k+)**: TGI v3
- **Dev/local**: Ollama or Nika native
- **Multi-LoRA**: LoRAX

---

## Deployment Architectures

### Mode A — VPS + Remote GPU

```
VPS (€5/mois)                    H100 Scaleway (€3/h)
├── nika binary                   ├── vLLM :8000 (Qwen-72B)
├── config.toml                   ├── vLLM :8001 (Mistral-7B)
│   └── [endpoints.h100]          └── LiteLLM :4000 (proxy)
├── workflows/
└── cron / daemon

Best for: Scale GPU up/down independently, always-on orchestration
```

### Mode B1 — Nika Native on GPU Machine

```
H100 / L40 / RTX 4090
├── nika binary (--features native-inference)
├── ~/.local/share/nika/models/
│   ├── qwen3-8b-q4_k_m.gguf
│   └── mistral-7b-q4_k_m.gguf
└── provider: native

Best for: Single user, dev, edge, offline, simplicity
```

### Mode B2 — Nika + vLLM on Same Machine

```
H100 / L40
├── vLLM :8000 (Qwen-72B)
├── vLLM :8001 (Mistral-7B)
├── nika binary
│   └── base_url: http://localhost:8000/v1
└── provider: openai

Best for: Single machine, multi-model, high throughput
```

### Mode C — Cloud APIs Only

```
Any machine
├── nika binary
├── ANTHROPIC_API_KEY
├── OPENAI_API_KEY
└── provider: anthropic

Best for: No GPU, quick start, frontier models (Claude, GPT-4o)
```

### Mode D — Hybrid (the endgame)

```
Laptop / VPS
├── nika binary + daemon (always-on)
├── routing:
│   strategy: smart
│   providers:
│     native:    [text, offline, free]          ← Mac GPU / CPU
│     h100:      [text, json, fast]             ← Scaleway H100
│     anthropic: [text, json, vision, reasoning] ← Cloud API
│
├── daemon services:
│   ├── cron: bench every morning at 6am
│   ├── cache: LLM response cache (blake3, 1h TTL)
│   ├── watch: auto-invalidate bench on workflow change
│   └── alerts: Telegram webhook at 80% budget
│
└── Nika routes each task to the optimal backend
    ├── classify      → native (free)
    ├── extract[×50]  → h100 (batching, cached)
    ├── analyze       → anthropic (reasoning, critical path)
    └── describe_img  → anthropic (vision, no alternative)

Best for: Maximum efficiency, production, cost optimization
```

### Mode E — Offline Edge

```
Raspberry Pi / Air-gapped machine / Laptop on a plane
├── nika binary (--features native-inference)
├── ~/.local/share/nika/models/
│   └── qwen3-1.7b-q4_k_m.gguf    (1.2 GB)
├── routing:
│   strategy: fallback
│   chain: [anthropic, native]      # Try cloud, fall back to local
│
└── When offline:
    anthropic → timeout → fallback → native → ✓ works

Best for: Edge computing, air-gapped, demos, offline development
```

---

## Timeline

```
Level 1: Custom Endpoints     ██████░░░░░░░░░░░░░░  ~1 session (in progress)
Level 2: nika bench           ░░░░░░██████░░░░░░░░  ~1 session
Level 3: Fallback Chains      ░░░░░░░░░░░░████░░░░  ~1 session
Level 4: Smart Routing        ░░░░░░░░░░░░░░░░████  ~2 sessions
Level 5: Auto-Optimization    ░░░░░░░░░░░░░░░░░░██  ~1 session (reuses L2+L4)
Level 6: Fleet Management     ░░░░░░░░░░░░░░░░░░░░  ~2 sessions
```

Each level is independently shippable. Ship early, ship often.

---

## Routing Module Architecture (Rust)

The routing system lives in a new `nika-engine/src/routing/` module. Trait-based, modular, zero-cost when unused.

### Module Tree

```
nika-engine/src/routing/
├── mod.rs              # Public API + ProviderSlot + BenchEntry
├── error.rs            # RoutingError (NIKA-320..325)
├── budget.rs           # BudgetTracker — AtomicU64 micro-dollars
├── capability.rs       # CapabilityFilter + bitflags Capabilities
├── bench_cache.rs      # BenchCache — .nika/bench-cache/ persistence
├── critical_path.rs    # CriticalPathAnalyzer — DAG longest-path
├── selector.rs         # ProviderSelector — entry point (filter → budget → strategy)
└── strategy/
    ├── mod.rs          # RoutingStrategy trait
    ├── direct.rs       # Direct — no routing, workflow default
    ├── fallback.rs     # Fallback — ordered priority list
    ├── smart.rs        # Smart — cost/latency optimization via bench data
    └── fleet.rs        # Fleet — multi-provider race dispatch
```

### Core Trait

```rust
pub trait RoutingStrategy: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;

    fn select(
        &self,
        task_id: &str,
        requirements: &TaskRequirements,
        candidates: &[ProviderSlot],
        bench_data: &[BenchEntry],
        is_critical: bool,
    ) -> Result<Vec<ProviderSlot>, RoutingError>;
}
```

### Key Design Decisions

- **`bitflags` for capabilities** — O(1) subset check: `task_caps & provider_caps == task_caps`
- **`AtomicU64` micro-dollars** — $0.017 = 17_000 microdollars. Lock-free CAS loop for concurrent for_each
- **`BenchCache` with EMA** — Exponential moving average (alpha=0.3) for smooth convergence
- **`CriticalPathAnalyzer`** — Forward + backward longest-path in O(V+E), identifies bottleneck tasks
- **`Option<Arc<ProviderSelector>>`** in TaskExecutor — zero cost when routing is not configured
- **No feature flag** — zero runtime cost when unused (single pointer check per task)
- **Dependency direction** — `executor → routing → dag + provider` (clean, no cycles)

### Error Codes (NIKA-320..325)

| Code | Variant | Trigger |
|------|---------|---------|
| NIKA-320 | `NoProviderAvailable` | All candidates filtered out |
| NIKA-321 | `BudgetExhausted` | Micro-dollar limit reached |
| NIKA-322 | `CapabilityMismatch` | Task needs vision, no provider supports it |
| NIKA-323 | `BenchCacheIo` | Disk read/write failure (non-fatal) |
| NIKA-324 | `InvalidStrategy` | Unknown strategy name in workflow config |
| NIKA-325 | `FleetAllFailed` | Fleet dispatch, all providers errored |

### Integration with TaskExecutor

```rust
// In TaskExecutor — routing intercedes before provider cache lookup
async fn resolve_provider_for_task(&self, task_id: &str, reqs: &TaskRequirements)
    -> Result<RigProvider, NikaError>
{
    if let Some(selector) = &self.routing_selector {
        let decision = selector.select(task_id, &reqs)?;
        let slot = decision.primary();
        return self.get_rig_provider(slot.provider.to_provider_id());
    }
    // Fallback: existing behavior
    self.get_rig_provider(&self.default_provider)
}
```

### Full Rust Code

Complete trait definitions, structs, and implementations for all 11 files are available in the rust-architect output. Key files:
- `routing/budget.rs` — 120 lines, AtomicU64 CAS loop, micro-dollar precision
- `routing/capability.rs` — 100 lines, bitflags, TaskRequirements::from_infer()
- `routing/bench_cache.rs` — 150 lines, DashMap + JSON disk persistence + EMA
- `routing/critical_path.rs` — 90 lines, forward+backward BFS
- `routing/selector.rs` — 130 lines, ProviderSelector composition root

---

## Bench Display Module (Rust — already written)

**File:** `tools/nika-engine/src/display/bench.rs` (1210 lines, 17 tests)

This file has been written by the rust-pro agent and is ready to compile. It contains:

### Data Types (4 structs)

```rust
pub struct BenchProviderResult {
    pub provider: String,
    pub model: String,
    pub ttft: Percentiles,           // p50, p90, p99
    pub tokens_per_sec: f64,
    pub total_duration: Duration,
    pub cost_per_run: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub task_timeline: Vec<BenchTaskTiming>,
    pub quality: Option<Vec<QualityScore>>,
}
```

### Display Functions (6 sections)

| Function | What it renders |
|----------|----------------|
| `format_bench_header()` | Rounded `╭╮╰╯` box, "N I K A   B E N C H", version, providers |
| `format_speed_section()` | TTFT percentiles, tok/s, total, "1.7× faster" comparison |
| `format_cost_section()` | $/run, token breakdown, savings % |
| `format_profile_section()` | Per-task Gantt bars with verb-colored `█░`, bottleneck detection |
| `format_quality_section()` | Inline `▓░` bar charts with green/yellow/red thresholds |
| `format_bench_summary()` | Verdict box, winner per category, run statistics |

### Color/Icon Consistency

All output uses existing Nika display helpers:
- `colors::duration()`, `colors::ttft()`, `colors::cost()`, `colors::tokens()`
- `icons::provider()` (`⋈` blue), `icons::verb()` (verb-colored), `icons::success()` (`✓` green)
- `stripped_len()` for ANSI-safe column alignment
- `cli_format::terminal_width()` capped at 72 chars

---

## Daemon Integrations

The routing system deeply integrates with nika-daemon's existing services. The daemon already has 31 IPC commands across 4 service domains. Routing adds 5 new ones.

### Existing Daemon Services → Routing Uses

| Daemon Service | Existing IPC | Routing Integration |
|----------------|-------------|---------------------|
| **Cache** (blake3 key, TTL) | `CacheGet`, `CacheSet`, `CacheStats` | **Cache-aware routing**: check cache BEFORE routing. Cache hit = skip provider entirely. $0.00, 0ms. |
| **Cron** (60s fire window) | `JobSubmit { cron }` | **Scheduled bench**: `nika bench --schedule "0 6 * * *"` keeps bench cache warm via cron job |
| **Watch** (FSEvents/inotify) | `WatchStart`, `WatchTriggered` | **Cache invalidation**: workflow file change → invalidate bench cache for that workflow hash |
| **Secrets** (keyring + env) | `GetSecret`, `SetSecret` | **Endpoint secrets**: custom endpoint API keys stored in same keyring |
| **Events** (broadcast bus) | `EventSubscribe` | **Routing events**: `SmartRouteDecision`, `FallbackTriggered`, `BudgetAlert` broadcast to TUI |

### New Daemon IPC Commands (5)

```rust
// Add to DaemonRequest enum in protocol.rs

/// Run bench in background, persist results to cache
BenchSubmit {
    workflow: String,
    providers: Vec<String>,
    iterations: u32,
    cron: Option<String>,          // Optional schedule
},

/// Get bench results for a workflow
BenchResults {
    workflow_hash: String,
},

/// Health check all configured endpoints
EndpointHealthCheck,

/// Get routing stats (budget spent, cache hits, decisions)
RoutingStats,

/// Set budget alert threshold
SetBudgetAlert {
    budget_usd: f64,
    alert_at_pct: u8,             // 0-100
    webhook_url: Option<String>,  // Telegram, Slack, etc.
},
```

### Cache-Aware Routing (detailed flow)

```
Task "classify" arrives at executor
    │
    ▼
┌─ Cache Check (daemon IPC) ───────────────┐
│                                           │
│  key = blake3(provider + model +          │
│              prompt + system +            │
│              temperature + max_tokens)    │
│                                           │
│  CacheGet { key } → CacheHit?            │
│                                           │
│  YES → return cached response             │
│        emit ProviderResponded {           │
│          cost_usd: 0.0,                   │
│          cache_read_tokens: N,            │
│          ttft_ms: 0                       │
│        }                                  │
│        ✓ Done (0ms, $0.00)               │
│                                           │
│  NO → proceed to routing                  │
└───────────────────────────────────────────┘
    │
    ▼
┌─ Routing (ProviderSelector) ─────────────┐
│  filter → budget → strategy → provider   │
└───────────────────────────────────────────┘
    │
    ▼
┌─ Provider Call ──────────────────────────┐
│  infer() → response                      │
│  CacheSet { key, response, ttl: 3600 }  │
│  ✓ Done                                  │
└───────────────────────────────────────────┘
```

**Impact on bench output:**

```
  ── Cache Impact ──────────────────────────────────────────────────

  Provider       Runs    Cache Hits   Effective $/run   Savings
  ⋈ anthropic    150     42 (28%)     $0.023 → $0.017   -26%
  ⋈ h100         150      0 (0%)      $0.006 → $0.006    0%

  Cache saved $0.90 over 150 runs · 42 hits × ~$0.021/hit
```

### Scheduled Bench Warmup (detailed flow)

```bash
# Submit bench as a cron job to the daemon
nika bench research-pipeline.nika.yaml \
  --providers anthropic,h100,native \
  --iterations 3 \
  --schedule "0 6 * * *"
```

```
Daemon receives BenchSubmit { cron: "0 6 * * *" }
    │
    ▼
Cron scheduler fires at 06:00 every day
    │
    ▼
Daemon spawns bench job (same as `nika bench` CLI)
    │
    ▼
Results written to .nika/bench-cache/<hash>.json
    │
    ▼
DaemonEvent::JobCompleted { job_id: "bench-xyz" } broadcast
    │
    ▼
Next `nika run` at 09:00 → smart routing reads fresh cache
    → Zero cold start, data is 3 hours old max
```

### Cost Alert Webhook

```toml
# config.toml
[routing]
budget_usd = 10.00

[routing.alerts]
threshold_pct = 80
webhook_url = "https://api.telegram.org/bot<TOKEN>/sendMessage?chat_id=<ID>"
webhook_body = '{"text": "Nika budget alert: {{spent}}/{{limit}} ({{pct}}%)"}'
```

```rust
// In BudgetTracker::try_spend() — after successful spend
if let Some(alert_config) = &self.alert_config {
    let pct = (new_total as f64 / limit as f64) * 100.0;
    if pct >= alert_config.threshold_pct as f64 && !self.alert_sent.swap(true, Ordering::Relaxed) {
        // Fire webhook (non-blocking, best-effort)
        tokio::spawn(send_webhook(alert_config, spent_usd, limit_usd, pct));
    }
}
```

### Key Tasks (daemon integration — 8 tasks)

| # | Task | What |
|---|------|------|
| 1 | Add `BenchSubmit`, `BenchResults` IPC to daemon protocol | `nika-daemon/src/protocol.rs` |
| 2 | Implement bench job execution in daemon (reuse Runner API) | `nika-daemon/src/services/bench.rs` NEW |
| 3 | Add `EndpointHealthCheck` IPC — ping `GET /v1/models` on all endpoints | `nika-daemon/src/services/health.rs` NEW |
| 4 | Wire cache check into executor BEFORE routing (cache-aware routing) | `nika-engine/src/runtime/executor/infer.rs` |
| 5 | Add `RoutingStats` IPC — budget spent, cache hit rate, decisions count | `nika-daemon/src/protocol.rs` |
| 6 | Implement `SetBudgetAlert` + webhook sender | `nika-daemon/src/services/alerts.rs` NEW |
| 7 | Wire `WatchTriggered` → invalidate bench cache for changed workflow | `nika-daemon/src/services/watch.rs` |
| 8 | Add `--schedule` flag to `nika bench` CLI → submits `BenchSubmit` to daemon | `nika-cli/src/bench.rs` |

---

## TUI Routing Dashboard

The TUI has 3 views: Studio [1/s], Command [2/c], Control [3/x]. Routing adds a new panel to Control view.

### Control View: Routing Panel

```
╭── Control [3/x] ─────────────────────────────────────────────────╮
│                                                                   │
│  [Providers]  [Routing]  [Theme]  [Settings]                     │
│               ────────                                            │
│                                                                   │
│  ── Strategy ─────────────────────────────────────────────────── │
│                                                                   │
│  Strategy: smart · Priority: cost                                │
│  Budget: $1.42 / $10.00  ━━━━━━━╸──────────────── (14%)         │
│                                                                   │
│  ── Endpoints ────────────────────────────────────────────────── │
│                                                                   │
│  ⋈ anthropic    ✓ healthy    claude-sonnet-4       cloud         │
│  ⋈ h100         ✓ healthy    Qwen2.5-72B-AWQ      10.0.1.42     │
│  ⋈ native       ✓ loaded     qwen3:8b              local        │
│  ⋈ ollama       ⚠ slow       llama3.2              localhost     │
│                                                                   │
│  ── Recent Decisions ─────────────────────────────────────────── │
│                                                                   │
│  14:32:01  ✧ classify     → native      free              $0.000 │
│  14:32:01  ☄ scrape[×10]  → (fetch)     —                 —     │
│  14:32:05  ✧ extract[×5]  → h100        json, cheap       $0.003 │
│  14:32:08  ✧ analyze      → anthropic   critical path     $0.008 │
│  14:32:11  ✧ summarize    → anthropic   critical path     $0.006 │
│                                                                   │
│  ── Bench Cache ──────────────────────────────────────────────── │
│                                                                   │
│  Last bench: 2h ago · 3 providers · cache fresh                  │
│  anthropic   9.2/10  ▇▇▇▇▇▇▇▇▇░  12.4s  $0.023                │
│  h100        8.5/10  ▇▇▇▇▇▇▇▇░░   7.1s  $0.006                │
│  native      7.1/10  ▇▇▇▇▇▇▇░░░  26.8s  $0.000                │
│                                                                   │
╰───────────────────────────────────────────────────────────────────╯
```

### Command View: Live Routing Events

During `nika run`, the Command view shows routing decisions inline with task execution:

```
  ✧ classify
    ⊛ route → native (free, short prompt)
    ⋈ native ──── ✓ 0.3s · 120 tok · $0.000

  ✧ extract[1/5]
    ⊛ route → h100 (json-capable, 5× cheaper than anthropic)
    ⋈ h100 ──── ✓ 0.8s · 380 tok · $0.001

  ✧ analyze
    ⊛ route → anthropic (critical path, best reasoning)
    ⋈ anthropic ──── ✓ 3.1s · 1.2k tok · $0.008

  Budget: $0.017 / $0.100 ━━╸──────────── (17%)
```

### New EventKind variants for TUI

```rust
// Add to nika-event/src/log.rs

EventKind::RouteDecision {
    task_id: Arc<str>,
    provider: String,
    reason: String,
    estimated_cost: f64,
    is_critical: bool,
    strategy: String,        // "smart", "fallback", "fleet"
}

EventKind::BudgetUpdate {
    spent_usd: f64,
    limit_usd: f64,
    pct: f32,
}

EventKind::EndpointHealth {
    endpoint: String,
    status: String,          // "healthy", "degraded", "unhealthy"
    latency_ms: u64,
    models: Vec<String>,
}
```

### Key Tasks (TUI — 6 tasks)

| # | Task | What |
|---|------|------|
| 1 | Add Routing tab to Control view | `nika-tui/src/views/control.rs` |
| 2 | Render strategy + budget bar in Routing panel | `nika-tui/src/widgets/routing.rs` NEW |
| 3 | Render endpoint health list with status icons | Same widget |
| 4 | Render recent routing decisions with cost | Same widget |
| 5 | Display inline route decision in Command view (⊛ route → provider) | `nika-tui/src/views/command.rs` |
| 6 | Display budget bar in Command view footer during execution | `nika-tui/src/views/command.rs` |

---

## Smart Combos

Cross-crate synergies that make the routing system more than the sum of its parts.

### Combo 1: Auto-Discover Providers

```bash
nika discover
```

Pings `GET /v1/models` on all configured endpoints + checks cloud API keys.

```
╭──────────────────────────────────────────────────────────────────╮
│                                                                  │
│  N I K A   D I S C O V E R                               v0.50 │
│                                                                  │
╰──────────────────────────────────────────────────────────────────╯

  ── Scanning endpoints ────────────────────────────────────────────

  ⋈ h100 (http://10.0.1.42:8000/v1)                          120ms
    ├── ✓ Qwen/Qwen2.5-72B-Instruct-AWQ     72B  AWQ  text,json
    └── ✓ mistralai/Mistral-7B-v0.3          7B   FP16 text,json

  ⋈ ollama (http://localhost:11434/v1)                         45ms
    ├── ✓ llama3.2:latest                    3B         text
    └── ✓ qwen2.5-coder:7b                  7B         text,json

  ── Scanning cloud providers ──────────────────────────────────────

  ⋈ anthropic     ✓ sk-ant-a...   claude-sonnet-4   text,json,vision,reasoning
  ⋈ openai        ✓ sk-proj...    gpt-4o            text,json,vision,reasoning
  ⋈ groq          ✗ no key        → nika keys set groq

  ── Auto-generated capabilities ───────────────────────────────────

  Updated config.toml with 4 active providers, 6 models

  → Tip: run nika bench to measure speed/cost across these providers
```

**How it works:**
1. Read `config.toml` endpoints
2. For each endpoint: `GET {base_url}/models` → parse model list
3. For each cloud provider: check `has_env_key()` from KNOWN_PROVIDERS catalog
4. Infer capabilities from model names (7B → text only, 72B → text+json+reasoning, etc.)
5. Write capabilities to `config.toml` under `[endpoints.*.capabilities]`

### Combo 2: A/B Testing Mode

```bash
nika bench workflow.nika.yaml \
  --providers anthropic,h100 \
  --ab-test \
  --eval
```

Runs the SAME workflow twice (one per provider), then shows output diff side-by-side:

```
  ── A/B Comparison — task "summarize" ─────────────────────────────

  ⋈ anthropic (9.2/10)              │  ⋈ h100 (8.5/10)
  ─────────────────────────────────  │  ─────────────────────────────
  The research identifies three      │  The research shows three main
  key trends in AI workflow          │  trends in AI workflows:
  orchestration:                     │
                                     │  1. Multi-model routing is
  1. **Multi-model routing** is      │     becoming standard
     becoming the standard           │  2. Cost optimization drives
     approach for production         │     adoption
     deployments.                    │  3. Open models approach
                                     │     frontier quality
  2. **Cost optimization** is the    │
     primary driver of adoption.     │  Overall, the field is moving
                                     │  toward hybrid approaches.
  3. **Open-weight models** are      │
     approaching frontier quality    │
     for specialized tasks.          │

  Judge: anthropic more detailed (+0.7), h100 more concise (-0.7)
  Verdict: anthropic wins for final reports, h100 fine for intermediates
```

### Combo 3: Task-Type Fingerprinting

The routing system can analyze task characteristics from the AST to auto-classify tasks:

```rust
// In routing/capability.rs
pub fn fingerprint_task(task: &AnalyzedTask) -> TaskFingerprint {
    TaskFingerprint {
        // From AST analysis
        has_vision: task.action.has_vision_content(),
        has_structured_output: task.structured.is_some(),
        has_extended_thinking: matches_infer_param(task, |p| p.extended_thinking == Some(true)),
        is_agent: matches!(task.action, AnalyzedTaskAction::Agent(_)),
        is_for_each: task.for_each.is_some(),

        // From prompt analysis (heuristic)
        estimated_complexity: estimate_prompt_complexity(&task.action),
        estimated_output_length: estimate_output_length(&task.action),
    }
}

pub enum TaskComplexity {
    Simple,     // Classification, yes/no, short answer → cheap model OK
    Medium,     // Extraction, summarization → balanced model
    Complex,    // Multi-step reasoning, analysis → best model
    Creative,   // Open-ended generation → quality matters
}
```

This lets smart routing make decisions WITHOUT bench data for new workflows:

```
  ✧ classify    → native    (TaskComplexity::Simple, no structured output)
  ✧ extract     → h100      (TaskComplexity::Medium, structured output required)
  ✧ analyze     → anthropic (TaskComplexity::Complex, critical path)
```

### Combo 4: Provider Warmup (daemon startup)

```rust
// In nika-daemon startup (server.rs)
async fn warmup_endpoints(config: &NikaConfig) {
    for (name, endpoint) in &config.endpoints {
        // Pre-connect: establish TCP + TLS
        let url = format!("{}/models", endpoint.base_url);
        match reqwest::get(&url).timeout(Duration::from_secs(5)).await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(endpoint = name, "Pre-connected, healthy");
            }
            Ok(resp) => {
                tracing::warn!(endpoint = name, status = %resp.status(), "Degraded");
            }
            Err(e) => {
                tracing::warn!(endpoint = name, error = %e, "Unreachable");
            }
        }
    }
}
```

**Result:** First `nika run` after daemon start has zero cold-start latency on TLS handshake. TTFT drops by ~50-200ms for HTTPS endpoints.

### Combo 5: Batch Coalescing for for_each

When `for_each` runs with `concurrency: 10` and smart routing sends all items to the same provider, the executor can coalesce items:

```
Without coalescing:
  for_each item 1 → HTTP connection 1 → vLLM
  for_each item 2 → HTTP connection 2 → vLLM
  for_each item 3 → HTTP connection 3 → vLLM
  ... (10 connections, 10 HTTP handshakes)

With coalescing:
  for_each items [1,2,3,...10] → 1 HTTP/2 connection → vLLM
  ... (1 connection, multiplexed, vLLM batches internally)
```

Nika already uses `reqwest::Client` with connection pooling. The coalescing happens naturally at the HTTP/2 level. What we ADD is:

```rust
// In executor: group for_each items by routed provider
let groups: HashMap<String, Vec<TaskId>> = for_each_items
    .iter()
    .map(|item| {
        let decision = selector.select(&item.task_id, &item.requirements)?;
        (decision.primary().provider.to_provider_id().to_string(), item.task_id.clone())
    })
    .into_group_map();

// Execute each group with its optimal concurrency
for (provider, items) in groups {
    let concurrency = match provider.as_str() {
        "native" => 1,              // Sequential (no GPU batching)
        _ => for_each.concurrency,  // Full concurrency (vLLM batches)
    };
    execute_group(provider, items, concurrency).await;
}
```

### Combo 6: Offline Fallback

```yaml
routing:
  strategy: fallback
  chain: [anthropic, h100, native]   # Cloud → remote GPU → local
  retry_on:
    - error
    - timeout
```

If internet is down:
1. `anthropic` → timeout → fallback
2. `h100` (VPC) → maybe also down → fallback
3. `native` → always available (GGUF in local memory)

**Output:**
```
  ✧ analyze
    ⋈ anthropic ──── ✗ connection timeout (5s)
    ↯ fallback → h100
    ⋈ h100 ──────── ✗ connection refused
    ↯ fallback → native
    ⋈ native ─────── ✓ 8.2s · 350 tok · $0.000

  ⚠ Running in offline mode (2 providers unreachable)
```

### Combo 7: Cost Dashboard

```bash
nika cost
```

```
╭──────────────────────────────────────────────────────────────────╮
│                                                                  │
│  N I K A   C O S T                                       v0.51  │
│                                                                  │
╰──────────────────────────────────────────────────────────────────╯

  ── Today ─────────────────────────────────────────────────────────

  Provider       Runs    Tokens      Cost        Trend (7d)
  ⋈ anthropic      12    42k in      $0.284      ▅▆▇▆▅▃▂
                          18k out
  ⋈ h100           45    120k in     $0.042      ▂▃▅▇▇▆▅
                          52k out
  ⋈ native         23    35k in      $0.000      ▃▃▃▃▃▃▃
                          15k out
  ─────────────────────────────────────────────────────────────────
  Total            80    197k in     $0.326
                          85k out

  ── This Week ─────────────────────────────────────────────────────

  Mon  $0.082  ██████░░░░░░░░░░░░░░░░░░░░░░░░░░
  Tue  $0.156  ████████████░░░░░░░░░░░░░░░░░░░░
  Wed  $0.234  ██████████████████░░░░░░░░░░░░░░
  Thu  $0.326  █████████████████████████░░░░░░░░  ← today
  Fri  ─────   (projected: $0.41 based on trend)

  Budget: $5.00/week · $0.326 spent · $4.674 remaining (93%)

  ── Savings from routing ──────────────────────────────────────────

  Without routing:   $0.612  (all anthropic)
  With routing:      $0.326
  Saved:             $0.286  (-47%)

  Without cache:     $0.326
  With cache:        $0.284  (12 cache hits saved $0.042)
```

**Data source:** Aggregated from `.nika/traces/` event logs (already persisted by TraceWriter).

### Summary of Combos

| # | Combo | Crates Used | Impact |
|---|-------|-------------|--------|
| 1 | Auto-discover providers | daemon + endpoints | Zero-config capability detection |
| 2 | A/B testing mode | bench + eval + display | Side-by-side output comparison |
| 3 | Task-type fingerprinting | nika-core AST + routing | Smart routing without bench data |
| 4 | Provider warmup | daemon startup | -50-200ms TTFT on first run |
| 5 | Batch coalescing | executor + routing | Better for_each throughput |
| 6 | Offline fallback | fallback chain + native | Always works, even offline |
| 7 | Cost dashboard | event traces + display | Daily/weekly cost visibility |

---

## UX Philosophy

### Progressive Disclosure — 4 levels of user complexity

```
Level 0: "It just works"
─────────────────────────
provider: anthropic
model: claude-sonnet-4-20250514
→ Zero config. One provider. Works out of the box.

Level 1: "I have a GPU"
────────────────────────
export OPENAI_BASE_URL="http://localhost:8000/v1"
→ One env var. Everything now hits your vLLM.

Level 2: "I want to choose per task"
─────────────────────────────────────
provider: h100          # Named endpoint in config.toml
tasks:
  - id: smart_task
    provider: anthropic  # Override for this task
→ Per-task provider selection. Still simple YAML.

Level 3: "Optimize everything"
──────────────────────────────
routing:
  strategy: smart
  budget: 0.10
→ Nika decides. Based on bench data, capabilities, cost, DAG.
```

**Rule: each level is additive.** A Level 0 workflow works unchanged at Level 3. No migration needed.

### CLI Design Principles

1. **Always show WHY** — Every routing decision includes a reason: "critical path", "json-capable", "cheapest"
2. **Always show COST** — Budget bar visible during execution. Cost per task in run summary.
3. **Always show SPEED** — Duration colored green/yellow/red. Relative comparison ("1.7× faster").
4. **Progressive detail** — `nika run` shows minimal routing info. `nika run --explain-routing` shows full decisions. `nika bench --profile` shows per-task breakdown.
5. **No surprises** — Routing decisions are deterministic given same bench data. `nika run --dry-run` shows what WOULD be routed.
6. **Escape hatch** — `provider: anthropic` on any task overrides all routing. User always has final say.

### Error Messages

Every routing error includes:
1. **What happened** — "No provider available for task 'analyze'"
2. **Why** — "Task requires vision capability, but no configured provider supports it"
3. **How to fix** — "Add a vision-capable provider: anthropic, openai, or gemini"

```
  ✗ [NIKA-322] Capability mismatch

  Task 'analyze_image' requires VISION capability.
  No configured provider supports it.

  ╭────────────────────────────────────────────╮
  │ Available providers:                        │
  │   ⋈ h100    → text, json                   │
  │   ⋈ native  → text                         │
  │                                             │
  │ Vision-capable providers (not configured):  │
  │   ⋈ anthropic  → nika keys set anthropic│
  │   ⋈ openai     → nika keys set openai   │
  │   ⋈ gemini     → nika keys set gemini   │
  ╰────────────────────────────────────────────╯
```

### Discoverability

```bash
nika --help
```

```
Commands:
  run        Run a workflow file
  bench      Compare providers on a workflow          ← NEW
  optimize   Auto-generate optimal routing config     ← NEW
  discover   Scan endpoints and detect models         ← NEW
  cost       Show cost breakdown by provider/day      ← NEW
  fleet      Manage GPU fleet (status, health, scale) ← NEW
  ...
```

Each new command has a `--help` with examples:

```bash
nika bench --help
```

```
Compare providers on a workflow

Usage: nika bench <FILE> [OPTIONS]

Arguments:
  <FILE>  Path to .nika.yaml file

Options:
  -p, --providers <LIST>     Providers to compare (comma-separated)
  -n, --iterations <N>       Number of iterations per provider [default: 3]
      --profile              Show per-task breakdown with Gantt bars
      --eval                 Evaluate output quality with LLM-as-judge
      --eval-model <MODEL>   Model for quality evaluation [default: claude-haiku-4-5]
      --schedule <CRON>      Schedule recurring bench via daemon
      --json                 Export results as JSON
  -o, --output <FILE>        Save results to file

Examples:
  nika bench workflow.nika.yaml --providers anthropic,h100,native
  nika bench workflow.nika.yaml --providers anthropic,h100 --eval --profile
  nika bench workflow.nika.yaml --schedule "0 6 * * *"
```

---

## Cross-Cutting Concerns

### Error Codes (all levels)

| Code | Level | Variant | Trigger |
|------|-------|---------|---------|
| NIKA-035 | L1 | `EndpointNotFound` | Named endpoint not in config |
| NIKA-036 | L1 | `EndpointConnectionFailed` | TCP/TLS connect to custom URL fails |
| NIKA-037 | L3 | `FallbackChainExhausted` | All providers in chain failed |
| NIKA-038 | L4 | `RoutingBudgetExceeded` | Dollar budget used up mid-workflow |
| NIKA-039 | L4 | `NoCapableProvider` | No provider matches task requirements |

### Streaming with Custom Endpoints

Custom endpoints via `RigProvider::OpenAiCompat` inherit SSE streaming from rig-core's OpenAI client. vLLM, SGLang, TGI all implement the same SSE protocol (`text/event-stream`). Streaming works out of the box for:
- `infer:` verb (text streaming via `infer_stream()`)
- `agent:` verb (tool-calling streaming via `run_openai()`)
- Vision (streaming via `infer_vision_stream()`)

**Known limitation:** Native provider uses mistral.rs streaming which has a different chunk protocol than rig-core's `StreamChunk`. This is pre-existing and unrelated to custom endpoints.

### Agent Verb (`agent:`) Routing

The `agent:` verb creates its own LLM client inside `RigAgentLoop` (`providers.rs:443`). Routing for agent tasks requires:
1. **Fallback (L3):** If agent loop fails (error/timeout), retry with next provider. The agent loop is single-attempt — no mid-loop provider switching.
2. **Smart routing (L4):** Select provider BEFORE creating `RigAgentLoop`. Thread selected provider + optional custom client into the agent loop constructor.
3. **Fleet (L6):** Same as L4 — select endpoint before loop creation.

Agent tasks are more expensive (multi-turn) so routing decisions have higher impact. Critical path boost is especially important for agent tasks.

### Config Backward Compatibility

All new config fields use `#[serde(default)]`:
- `NikaConfig.endpoints: IndexMap` — defaults to empty map
- `NikaConfig.fleet: Option<FleetConfig>` — defaults to None
- `CustomEndpointConfig.hourly_rate: Option<f64>` — defaults to None
- `CustomEndpointConfig.currency: Option<String>` — defaults to None

Old `config.toml` files without these fields continue to work unchanged.

### Token Counting Caveat

Different providers count tokens differently:
- **Anthropic:** Includes special tokens, prompt caching reports `cache_read_tokens`
- **OpenAI:** May not include system prompt tokens in some modes
- **vLLM/SGLang:** Reports `usage.prompt_tokens` and `usage.completion_tokens` — may differ from cloud
- **Native (mistral.rs):** Token counting is approximate (no tokenizer access post-generation)

`nika bench` should display a footnote: "Token counts are provider-reported and may not be directly comparable."
