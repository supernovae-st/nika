# Session Handoff — Model Resilience (2026-04-07)

## TL;DR

Nicolas tried `gpt-5.2` in a workflow → Nika crashed. We fixed 9 bugs, created a `ModelCapabilities` catalog as single source of truth, updated stale defaults (gpt-4o RETIRED), and documented everything. Templatable refactor landed in a parallel session. **2 items deferred** — B5 agent streaming + god file split.

## Current State (post-session)

```
Branch: main
HEAD:   85ac6fa54  (Templatable coverage complete)
Our:    b72568884  (model resilience — 6 commits behind HEAD)
Tests:  1261 pass (nika-core --lib)
Build:  cargo check --workspace CLEAN
WIP:    9 unstaged files (Templatable extras + package.json versions)
```

### Commits This Day (oldest → newest)

```
b72568884  feat(core): model resilience — ModelCapabilities catalog + stale defaults fix  ← THIS SESSION
d23a27102  fix(clippy): resolve pre-existing warnings across workspace                    ← other session
c2a10d495  feat(core): propagate Templatable<T> through AST pipeline                      ← other session
107cdf811  feat(engine): runtime template resolution for typed fields                     ← other session
34abd473c  feat(schema): allow template expressions in all 65 typed fields                ← other session
cf1929991  fix(engine): resolve ALL Templatable fields                                    ← other session
85ac6fa54  fix(engine): complete Templatable coverage — zero silent drops                  ← other session
```

## What This Session Did

### Created: `nika-core/src/catalogs/capabilities.rs` (351 lines, 23 tests)

Single source of truth for model-specific API behavior. 5 orthogonal concerns:

```rust
pub struct ModelCapabilities {
    pub token_limit_param: TokenLimitParam,  // MaxTokens | MaxCompletionTokens
    pub supports_temperature: bool,          // o-series: false, gpt-5.x: true
    pub supports_stop_sequences: bool,       // grok-4: false
    pub supports_thinking: bool,             // Claude, OpenAI reasoning: true
    pub supports_vision: bool,               // DeepSeek: false
}
```

**Provider-aware:** Same model name → different behavior based on provider:
- `o3` on OpenAI → `MaxCompletionTokens` + no temperature
- `o3-finetune` on custom vLLM → `MaxTokens` + temperature OK (safe default)
- `gpt-5.2` on OpenAI → `MaxCompletionTokens` + temperature OK (key insight!)
- `deepseek-reasoner` → `MaxTokens` (NOT `MaxCompletionTokens` like we were sending)

### Fixed: 9 Bugs

| # | Bug | Impact | Fix |
|---|-----|--------|-----|
| B1 | `is_reasoning_model` misses `gpt-5.x` (dot notation) | gpt-5.2 crash | `starts_with("gpt-5.")` in catalog |
| B2 | `is_openai_reasoning` hardcodes o-series in infer.rs | gpt-5.x extended_thinking broken | Uses `model_capabilities()` |
| B3 | `raw_openai_compat_infer` always sends `max_tokens` | HTTP 400 for reasoning models | Sends `max_completion_tokens` based on caps |
| B4 | Missing gpt-5.2 pricing | "Unknown model" warning | Added $1.75/$14.00 |
| B6 | **OpenAI default `gpt-4o` RETIRED** | **404 for every new user** | → `gpt-5.2` |
| B7 | DeepSeek Reasoner sent `max_completion_tokens` | DeepSeek API might reject | Fixed to `max_tokens` |
| B8 | Missing gpt-5.4/mini/nano pricing | Warning spam | Added 3 entries |
| B9 | Missing grok-4 pricing | Warning | Added $3/$15 |
| B10 | Gemini default `gemini-2.0-flash` stale | Old model | → `gemini-2.5-flash` |

### Fixed: Extended Thinking

- `temperature=1.0` was forced for ALL providers when `extended_thinking: true`. Now only for Anthropic.
- gpt-5.x gets `reasoning_effort: "high"` when thinking enabled (was only o-series)
- Agent verb silently overrode provider to Claude — now documented, routing improved

### Refactored

- `is_reasoning_model()` → 3-line wrapper delegating to `model_capabilities()`
- `effective_temperature_for_model()` → eliminated 2x code duplication (was 13 lines x2)
- `raw_openai_compat_infer()` → provider-aware via base_url detection

### Added: 27 tests, 2 plans

- 23 capability tests covering all providers + edge cases
- Canary test: fails when default model is in retired list
- Contract test: every known reasoning model must have pricing + correct capabilities
- `docs/plans/2026-04-07-model-resilience.md` — full plan with critical questions
- `docs/plans/2026-04-07-deferred-provider-refactor.md` — B5 + god file split

### SDK Changes (local only, nika-client repo)

```typescript
// NEW: Nika.fromEnv() — reads NIKA_URL + NIKA_TOKEN
const nika = Nika.fromEnv();

// FIXED: helpful error instead of "Cannot read properties of undefined"
new Nika({ url: undefined, token: 'x' });
// → TypeError: NikaConfig.url is required — pass the nika serve URL.
//   Example: new Nika({ url: "http://localhost:3000", token: "..." })
//   Or use:  Nika.fromEnv() to read NIKA_URL and NIKA_TOKEN from environment
```

## What's NOT Done

### B5: Agent Streaming max_completion_tokens (HIGH)

**Impact:** `agent:` verb with reasoning models (o3, gpt-5.2) → HTTP 400. `infer:` verb works fine.

**Root cause:** rig-core 0.33.0 `.max_tokens()` always serializes as `"max_tokens"`. `additional_params` uses `#[serde(flatten)]` → both fields sent → OpenAI rejects.

**Fix strategy (3 affected paths):**
1. `streaming.rs:82` — completion stream builder
2. `streaming.rs:273` — agent tool-use builder  
3. `rig/mod.rs:1326` — infer_with_options builder

**Key research needed:** Does rig-core skip serializing `max_tokens` when not set via `.max_tokens()`? If yes → skip the call + inject via additional_params. If no → need raw HTTP fallback.

**Detailed plan:** `docs/plans/2026-04-07-deferred-provider-refactor.md` Part A (Tasks A.1-A.6, ~45min)

### P5: rig/mod.rs God File Split (MEDIUM)

**1964 lines, 37 public functions → 5 focused modules (~350 lines each)**

Target:
```
rig/mod.rs          ~350 lines   enum + macro + re-exports
rig/capabilities.rs ~150 lines   model checks, provider flags
rig/construction.rs ~300 lines   from_name, constructors, auto()
rig/compat.rs       ~150 lines   raw HTTP for OpenAI-compat
rig/inference.rs    ~500 lines   infer, vision, tools
rig/streaming.rs    ~400 lines   streaming (merge with stream.rs)
```

**Detailed plan:** `docs/plans/2026-04-07-deferred-provider-refactor.md` Part B (Tasks B.1-B.9, ~2h)

### nika-client SDK commit

Changes made locally in `/Users/thibaut/dev/supernovae/nika-client/src/index.ts`. Need to commit + push in that repo.

## WIP in Working Directory

```
 M packages/nika-darwin-arm64/package.json      ← version bumps (unrelated)
 M packages/nika-darwin-x64/package.json
 M packages/nika-linux-arm64/package.json
 M packages/nika-linux-x64/package.json
 M packages/nika-win32-x64/package.json
 M tools/nika-engine/src/ast/lower.rs           ← Templatable extras
 M tools/nika-engine/src/runtime/resolve_typed.rs
 M tools/nika-engine/src/runtime/structured_retry.rs
 M tools/nika-engine/src/runtime/task_dispatch.rs
?? docs/plans/2026-04-07-ai-rules-architecture.md
?? docs/sprints/SESSION-HANDOFF-EDITORS-ARCH-2026-04-07.md
?? docs/sprints/SESSION-MEGA-HANDOFF-2026-04-07.md
?? docs/sprints/SESSION-MODEL-RESILIENCE-HANDOFF.md
```

## Architecture Diagram

```
BEFORE (scattered, fragile)                    AFTER (centralized, resilient)
──────────────────────────                     ─────────────────────────────

rig/mod.rs:92                                  nika-core/catalogs/capabilities.rs
  is_reasoning_model("gpt-5.2") → false ✗       model_capabilities("openai","gpt-5.2")
                                                   .token_limit_param → MaxCompletionTokens ✓
infer.rs:844                                       .supports_temperature → true ✓
  is_openai_reasoning = "openai"                   .supports_thinking → true ✓
    && starts_with("o1"||"o3"||"o4") ✗
                                                 model_capabilities("deepseek","deepseek-reasoner")
streaming.rs:71                                    .token_limit_param → MaxTokens ✓ (was wrong!)
  is_reasoning_model(model) → wrong for
  custom endpoints                               model_capabilities("h100","o3-finetune")
                                                   .token_limit_param → MaxTokens ✓ (safe default)
cost.rs: no gpt-5.2, no gpt-5.4, no grok-4
resolver.rs: default = gpt-4o (RETIRED!)         All callers → model_capabilities(provider, model)
```

## Next Priorities

```
1. B5 streaming fix         ~45min   HIGH   (agent: verb broken for reasoning models)
2. God file split            ~2h     MEDIUM (1964 lines → 5 modules)
3. nika-client SDK commit    ~5min   LOW    (separate repo, changes ready)
4. Unstaged WIP cleanup      ~10min  LOW    (commit or discard Templatable extras)
```
