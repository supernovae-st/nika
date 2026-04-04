# Grand Nettoyage S4+ — Provider Architecture & Stabilization Plan

> Generated 2026-04-05 from 7-agent deep research (OpenAiCompat dedup, ModelResolver,
> LSP diagnostics, find_project_root, provider landscape, Rust architecture, infer.rs tests).

---

## Executive Summary

The provider layer is Nika's biggest architectural bottleneck. `rig/mod.rs` is 2146 lines
with 17 match blocks × 10 arms = 170+ match arms. Adding one provider means touching 25+
locations. The fix: collapse to 3 enum variants using a `dispatch_rig!` macro, unify the
agent loop, and make OpenAI-compatible providers config-driven (zero code).

**Impact**: 2146 → ~700 lines in mod.rs, 170+ → ~40 match arms, new OpenAI-compat providers
need zero Rust code (config only).

---

## Phase 1: OpenAiCompat HTTP Dedup (1 commit, LOW risk)

### Current State
- **Copy A**: `raw_openai_compat_infer()` (mod.rs:537-613) — text completion, extracts
  `choices[0].message.content` + token usage. Used by `infer()` and `infer_with_options()`.
- **Copy B**: Inline in `infer_with_tools()` (mod.rs:1104-1203) — tool completion, extracts
  `choices[0].message.tool_calls[0].function.arguments` with content fallback. **Missing
  token usage tracking.**

### Shared (~35 lines identical)
- Build POST request with bearer auth
- Send request with timeout/network error mapping
- Read body text
- Non-2xx error with 500-char truncation
- JSON parse

### Different
| Aspect | Copy A | Copy B |
|--------|--------|--------|
| Body fields | `model, messages, max_tokens, temperature` | adds `tools, tool_choice: "required"` |
| Response extraction | `choices[0].message.content` | `tool_calls[0].function.arguments` + content fallback |
| Token usage | extracted, returned | **MISSING** (silent zero telemetry) |
| Return type | `(String, u64, u64)` | `String` |

### Fix: Extract `raw_chat_completion()`

```rust
/// Shared low-level POST to /chat/completions.
/// Returns parsed JSON response body.
async fn raw_chat_completion(
    http_client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    body: serde_json::Value,
    timeout: Duration,
) -> Result<(serde_json::Value, u64, u64), RigInferError>
```

Each caller builds its own body and extracts its own response field. Token usage tracked
in both paths. ~35 lines eliminated, bug fixed (tools path gets telemetry).

### Files
- `nika-engine/src/provider/rig/mod.rs` — extract helper, refactor both call sites

---

## Phase 2: ModelResolver Wiring (1-2 commits, LOW risk)

### Bypass Locations Found

| Location | Severity | Fix |
|----------|----------|-----|
| `providers.rs:225-289` — 5 `run_*()` methods with hardcoded defaults | **P1** | Replace `unwrap_or_else("hardcoded")` with `ok_or_else(NikaError)` to match `run_claude/run_openai` |
| `verbs.rs:74-85` — local `default_model_for_provider()` | **P2** | Replace with `nika_core::catalogs::default_model_for_provider()` |
| `spawn.rs:289-307` — child agent inherits parent model | **P3** | Add ModelResolver check when child provider differs from parent |

### Why It's Safe
The executor (agent.rs:232-239) ALWAYS pre-populates `params.model` via ModelResolver
before calling the agent loop. The hardcoded defaults in `providers.rs` are dead code in
the normal workflow path. Making them error instead of silently defaulting enforces the
contract and prevents future drift.

### Files
- `nika-engine/src/runtime/rig_agent_loop/providers.rs` — 5 methods
- `nika-cli/src/verbs.rs` — 1 function replacement
- `nika-engine/src/runtime/spawn.rs` — optional compatibility check

---

## Phase 3: Provider Enum Collapse (3-5 commits, MEDIUM risk)

### Architecture Decision: Keep Enum, Collapse to 3 Variants

**Why NOT trait objects:**
1. rig-core uses generics (`C: CompletionClient`), not trait objects. Type erasure would
   lose monomorphization (perf hit on hot path — every token chunk).
2. Zero backward compat means we can restructure freely.
3. The "many providers" problem is an illusion — there are only 3 behavioral categories.

### Target State

```rust
pub enum RigProvider {
    /// Any rig-core CompletionClient (Anthropic, OpenAI, Mistral, etc.)
    RigCore(RigCoreProvider),
    /// OpenAI-compatible HTTP endpoint (vLLM, OpenRouter, Together, etc.)
    OpenAiCompat(OpenAiCompatProvider),
    /// Deterministic mock (responses in executor)
    Mock,
    /// Local GGUF inference (feature-gated)
    #[cfg(feature = "native-inference")]
    Native(NativeRuntime),
}
```

### Inner Type Erasure with Macro

```rust
enum RigClient {
    Anthropic(anthropic::Client),
    OpenAI(openai::Client),
    Mistral(mistral::Client),
    Groq(groq::Client),
    DeepSeek(deepseek::Client),
    Gemini(gemini::Client),
    XAi(xai::Client),
}

struct RigCoreProvider {
    client: RigClient,
    flags: &'static ProviderFlags,
}

struct ProviderFlags {
    id: &'static str,
    is_anthropic: bool,       // stream parsing differs
    supports_vision: bool,     // DeepSeek = false
    supports_native_structured: bool,
    cost_kind: ProviderKind,
}

macro_rules! dispatch_rig {
    ($self:expr, |$client:ident| $body:expr) => {
        match &$self.client {
            RigClient::Anthropic($client) => $body,
            RigClient::OpenAI($client) => $body,
            RigClient::Mistral($client) => $body,
            RigClient::Groq($client) => $body,
            RigClient::DeepSeek($client) => $body,
            RigClient::Gemini($client) => $body,
            RigClient::XAi($client) => $body,
        }
    };
}
```

### Impact
| Metric | Before | After |
|--------|--------|-------|
| `rig/mod.rs` lines | 2146 | ~700 |
| Match arms total | 170+ | ~40 |
| Adding new rig-core provider | 25+ locations | 15 lines, 3 files |
| Adding new OpenAI-compat | Code changes | Zero (config only) |

### Migration: 5 Commits

1. **Extract `OpenAiCompatProvider` struct** — move raw HTTP to own struct, unify text/tools
2. **Extract `RigCoreProvider` + `dispatch_rig!`** — collapse 7 variants into 1
3. **Unify agent loop** — delete 7 `run_*()` methods, add single `run(provider)` entry
4. **Consolidate enums** — `ProviderName::to_cost_id()`, flags use `ProviderName`
5. **Structured HTTP errors** — `RigInferError::HttpError { endpoint, status, body }`

---

## Phase 4: New Providers (1-2 commits, LOW risk)

### Provider Landscape Research Results

| Provider | OpenAI-Compat | Base URL | Env Var | Key Prefix | Priority |
|----------|---------------|----------|---------|------------|----------|
| **OpenRouter** | YES | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` | `sk-or-v1-` | **P0** |
| **Together AI** | YES | `https://api.together.xyz/v1` | `TOGETHER_API_KEY` | — | **P1** |
| **Fireworks AI** | YES | `https://api.fireworks.ai/inference/v1` | `FIREWORKS_API_KEY` | `fw_` | **P1** |
| **Cerebras** | YES | `https://api.cerebras.ai/v1` | `CEREBRAS_API_KEY` | `csk-` | **P1** |
| **SambaNova** | YES | `https://api.sambanova.ai/v1` | `SAMBANOVA_API_KEY` | — | **P2** |
| **Perplexity** | YES | `https://api.perplexity.ai` | `PERPLEXITY_API_KEY` | `pplx-` | **P2** |
| **Cohere** | YES (compat) | `https://api.cohere.com/compatibility/v1` | `COHERE_API_KEY` | — | **P3** |
| **AI21 Labs** | YES | `https://api.ai21.com/studio/v1` | `AI21_API_KEY` | — | **P3** |
| **Replicate** | PARTIAL | `https://openai.replicate.com/v1` | `REPLICATE_API_TOKEN` | `r8_` | **P4** |

### OpenRouter (P0 — meta-aggregator)
- Proxies 200+ models through single key
- Model names: `provider/model` format (e.g., `anthropic/claude-sonnet-4-20250514`)
- Supports streaming, vision, structured output (proxied to upstream)
- Optional `HTTP-Referer` + `X-Title` headers for ranking
- Built-in model fallback (`route: "fallback"`)

### Cerebras (P1 — speed differentiator)
- **Fastest inference: 2000+ tok/sec on Llama 3.3 70B** (10x GPU providers)
- Wafer-Scale Engine (WSE-3) custom silicon
- Limited model selection (only their optimized models)
- No vision

### Each Addition = ~15 Lines
1. `Provider { ... }` entry in `KNOWN_PROVIDERS` (10 lines)
2. Match arm in `RigProvider::from_name()` → `Self::openai_compat(...)` (8 lines)
3. Default model in `PROVIDER_DEFAULTS` (1 line)
4. Tests (copy-paste pattern)

### Already Works Today (via endpoints)
```toml
# nika.toml — zero code
[endpoints.openrouter]
base_url = "https://openrouter.ai/api/v1"
api_key = "$env.OPENROUTER_API_KEY"
model = "anthropic/claude-sonnet-4-6"
```

First-class catalog entries give: `nika provider list` shows them, `provider: openrouter`
works in YAML, aliases work, key validation works.

---

## Phase 5: find_project_root Dedup (1 commit, LOW risk)

### 3 Current Implementations

| Impl | Location | Markers | Return | Tests |
|------|----------|---------|--------|-------|
| nika-tui | `lib.rs:505` | Cargo.toml, .git | `Option<PathBuf>` | 0 |
| nika-engine | `mcp_config.rs:155` | .nika, .git, nika.yaml | `Option<PathBuf>` | 0 |
| **nika-cli** | `config.rs:35` | nika.toml, .nika (2-pass) | `Result<ProjectRoot>` | **4 tests** |

### Unified Design (in nika-core)

```rust
// nika-core/src/project.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRootSource {
    NikaToml,        // highest priority
    DotNika,         // legacy
    GitRepository,   // fallback for non-Nika projects
    CargoWorkspace,  // monorepo development
    Fallback,        // start dir (nothing found)
}

pub struct ProjectRoot {
    pub root: PathBuf,
    pub source: ProjectRootSource,
}

/// Priority: nika.toml > .nika/ > .git/ > Cargo.toml > start_dir
pub fn find_project_root(start: Option<&Path>) -> ProjectRoot
```

### Migration
1. Define in nika-core (based on nika-cli's tested implementation)
2. Replace 3 impls with calls to the shared function
3. Port nika-cli's 4 tests to nika-core

---

## Phase 6: LSP Diagnostics (P2, LOW priority for launch)

### Key Finding: nika-lsp ALREADY HAS Full Diagnostics

`nika-lsp/src/diagnostics.rs` (380 lines, 14 tests) implements a **5-phase pipeline**:
1. `raw::parse()` → ParseError with Span
2. `analyze()` → AnalyzeResult with errors + warnings
3. Template reference validation (`{{with.alias}}` checks)
4. Empty tasks guard (NIKA-145)
5. Provider key availability check

This is wired into `backend.rs` via `validate_document()` → `client.publish_diagnostics()`.

### The Actual Gap (P2, NOT P0)
Only `nika-lsp-core` is missing diagnostics — specifically, converting `PartialWorkflow.error_ranges`
(tree-sitter ERROR node byte ranges) to LSP Diagnostic entries. The converter infrastructure
exists (`LineIndex.to_lsp_position()` in `position.rs:80`), but no `handlers/diagnostics.rs`
wires them together.

**Scope**: tree-sitter syntax errors only (not semantic). Semantic diagnostics already work
via the nika-lsp path. This is a nice-to-have for the shared LSP core library, not a ship
blocker.

---

## Phase 7: infer.rs Test Coverage (2-3 commits, LOW risk)

### Current State: 6 unit tests (from S3) + 9 E2E tests + 10 mock_json tests

### What CAN Be Tested Without API Keys

| Function | Priority | Feasible | Why |
|----------|----------|----------|-----|
| `check_infer_guardrails` | **HIGH** | YES (mock TaskExecutor) | Emits events, raises GuardrailViolation |
| Provider chain fallback (lines 250-309) | **HIGH** | YES (mock) | NIKA_MOCK_FAIL_COUNT for retry testing |
| Empty prompt after template resolution | **MED** | YES (mock) | Lines 136-143 |
| `response_format` injection into system prompt | **MED** | YES (mock) | Lines 115-131 |
| Vision content validation (image count, size) | **LOW** | HARD — mock short-circuits before validation | Would need to bypass mock early return |
| Structured output layers (L0a, L0b) | **LOW** | Wiremock feasible but complex | Fake LLM endpoint returning JSON |

### What's Already Well-Covered
- `mock_json.rs` — 10 tests, good shape
- `verbs.rs` helpers (`strip_think_tags`, `detect_image_media_type`, etc.) — tested
- `build_json_schema_instruction` — 8 tests
- E2E via `tests_e2e_workflow.rs` — 9 infer-specific tests

### Test Helper Exists
`runtime/executor/tests.rs` has a `build_test_executor()` helper (creates TaskExecutor with
mock EventLog, empty RunContext). Used by 252+ existing tests.

---

## Execution Priority for May 5 Launch

| Phase | Risk | Impact | Effort | Priority |
|-------|------|--------|--------|----------|
| **P1: HTTP dedup** | LOW | Fixes tools token telemetry bug | 1 commit | **DO FIRST** |
| **P2: ModelResolver** | LOW | Fixes silent model default drift | 1-2 commits | **DO SECOND** |
| **P4: New providers** | LOW | OpenRouter = huge user value | 1-2 commits | **DO THIRD** |
| **P5: find_project_root** | LOW | Clean dedup, 3→1 impls | 1 commit | **DO FOURTH** |
| **P7: infer.rs tests** | LOW | Coverage for critical file | 2-3 commits | **DO FIFTH** |
| **P3: Enum collapse** | MEDIUM | Massive code reduction but risky | 3-5 commits | **POST-LAUNCH** |
| **P6: LSP diagnostics** | LOW | Nice-to-have, not ship blocker | 1 commit | **POST-LAUNCH** |

---

## Rules (same as always)

- 1 fix = 1 commit
- Test BEFORE commit
- `cargo test --workspace --lib` (no more --exclude nika-py!)
- Zero backward compat
- AGPL-3.0-or-later
- `dispatch_rig!` macro approach for Phase 3 (NOT trait objects)
- Config-driven for new OpenAI-compat providers
