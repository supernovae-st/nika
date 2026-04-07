# Model Resilience — Hardening Plan v2

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate the class of bugs where a new model release breaks Nika at runtime, fix stale defaults, and make the system resilient to future model launches.

**Architecture:** Centralize all model metadata into a `ModelCapabilities` catalog in nika-core. Split "is reasoning" into orthogonal concerns (token param, temperature, thinking). Update stale defaults. Harden the SDK.

**Tech Stack:** Rust (nika-core, nika-engine), TypeScript (nika-client)

---

## Root Cause Analysis

The gpt-5.2 incident + deep research exposed **11 bugs** and **5 systemic weaknesses**.

### Bugs Found

| # | Bug | Severity | Status |
|---|-----|----------|--------|
| B1 | `is_reasoning_model` misses `gpt-5.x` (dot notation) | CRITICAL | FIXED |
| B2 | `is_openai_reasoning` in infer.rs hardcodes o-series only | CRITICAL | FIXED |
| B3 | `raw_openai_compat_infer` always sends `max_tokens` | CRITICAL | FIXED |
| B4 | Missing gpt-5.2 pricing in catalog | MEDIUM | FIXED |
| B5 | Agent streaming paths send `max_tokens` via rig-core for ALL models | HIGH | OPEN |
| B6 | **OpenAI default model is `gpt-4o` — RETIRED April 2026 (returns 404)** | **CRITICAL** | OPEN |
| B7 | `is_reasoning_model` treats DeepSeek Reasoner same as OpenAI (wrong token param) | MEDIUM | OPEN |
| B8 | Missing gpt-5.4, gpt-5.4-mini, gpt-5.4-nano pricing | MEDIUM | OPEN |
| B9 | No grok-4 in pricing or model lists | LOW | OPEN |
| B10 | Gemini default is `gemini-2.0-flash` — old | LOW | OPEN |
| B11 | rig-core `additional_params` uses `#[serde(flatten)]` — CANNOT override `max_tokens`, both fields sent | HIGH | OPEN |

### Systemic Weaknesses

| # | Weakness | Impact |
|---|----------|--------|
| S1 | Model capabilities scattered across 6+ files | Every new model = N files to update |
| S2 | `is_reasoning_model` conflates 3 orthogonal concerns | Wrong behavior for DeepSeek, future models |
| S3 | No contract test for API parameter correctness | Silent 400s in production |
| S4 | SDK crashes with unhelpful error on missing config | Developer confusion |
| S5 | Stale defaults not caught by CI | Users hit 404 on first use |

### Research Findings (April 2026 landscape)

**Provider-specific quirks discovered:**

| Provider | Model | `max_completion_tokens` required? | Rejects `temperature`? | Other quirks |
|----------|-------|-----------------------------------|------------------------|--------------|
| OpenAI | o1, o3, o3-mini, o3-pro, o4-mini | YES | Conflicting reports (safer: strip) | `reasoning_effort` supported |
| OpenAI | gpt-5, gpt-5.2, gpt-5.4 | YES | NO (accepts temperature) | Newer models |
| OpenAI | gpt-4o, gpt-4.1 | N/A | N/A | **RETIRED April 2026 (404)** |
| DeepSeek | deepseek-reasoner | **NO** (uses `max_tokens`) | Unclear (safer: strip) | `reasoning_content` in response |
| xAI | grok-4 | NO | NO | **Rejects `stop` sequences** |
| Mistral | magistral-* | NO | NO | Always-on reasoning, `reasoning_effort` |
| Gemini | gemini-3-* | NO | NO | `thinking_level` (LOW/MEDIUM/HIGH) |

**Key insight:** `is_reasoning_model` currently conflates THREE independent concerns:
1. Needs `max_completion_tokens` instead of `max_tokens` → **OpenAI only**
2. Rejects `temperature` parameter → **o-series only** (gpt-5.x accepts it!)
3. Has reasoning/thinking capabilities → **All providers, different mechanisms**

**rig-core limitation:** `additional_params` uses `#[serde(flatten)]` which ADDS fields but does NOT remove `max_tokens`. For the streaming agent path, we cannot just inject `max_completion_tokens` — we must avoid calling `.max_tokens()` entirely and build the body manually.

---

## Phase 0: URGENT — Fix Stale Defaults (B6, B10)

### Task 0.1: Update retired OpenAI default model

**Files:**
- Modify: `tools/nika-core/src/catalogs/resolver.rs:16`

**Step 1: Write failing test**

```rust
#[test]
fn openai_default_model_is_not_retired() {
    let (_, model) = PROVIDER_DEFAULTS.iter().find(|(p, _)| *p == "openai").unwrap();
    // gpt-4o was retired April 2026 — ensure we don't default to it
    assert_ne!(*model, "gpt-4o", "gpt-4o is retired — update default");
    assert_ne!(*model, "gpt-4o-mini", "gpt-4o-mini is retired — update default");
    assert_ne!(*model, "gpt-4.1", "gpt-4.1 is retired — update default");
}
```

**Step 2: Fix defaults**

```rust
pub static PROVIDER_DEFAULTS: &[(&str, &str)] = &[
    ("anthropic", "claude-sonnet-4-6"),
    ("openai", "gpt-5.2"),              // WAS: gpt-4o (RETIRED)
    ("mistral", "mistral-large-latest"),
    ("groq", "llama-3.3-70b-versatile"),
    ("deepseek", "deepseek-chat"),
    ("gemini", "gemini-2.5-flash"),     // WAS: gemini-2.0-flash
    ("xai", "grok-3-fast"),
    // ... rest unchanged
];
```

Also update cheap model for OpenAI:

```rust
pub static PROVIDER_CHEAP_MODELS: &[(&str, &str)] = &[
    ("anthropic", "claude-haiku-4-5"),
    ("openai", "gpt-5.2"),   // WAS: gpt-4.1-mini (RETIRED)
    // ...
];
```

**Step 3: Run tests**

```bash
cargo test -p nika-core --lib resolver -v
```

**Step 4: Commit**

```
fix(core): update stale defaults — gpt-4o/gpt-4.1 retired April 2026
```

### Task 0.2: Update TUI + CLI model lists

**Files:**
- Modify: `tools/nika-cli/src/keys.rs:413` — update OpenAI top models
- Modify: `tools/nika-tui/src/widgets/provider_modal/tabs/cloud.rs` — update model lists

**Step 1: Update keys.rs**

```rust
"openai" => vec!["gpt-5.2".into(), "gpt-5.4".into(), "o4-mini".into()],
```

**Step 2: Commit**

```
chore(cli): update model lists with current models (gpt-5.2, gpt-5.4)
```

---

## Phase 1: Fix Agent Streaming (B5, B11)

### Task 1.1: Investigate rig-core serialization

**Context:** rig-core 0.33.0 always serializes `.max_tokens()` as `"max_tokens"` in the JSON body. `additional_params` uses `#[serde(flatten)]` which ADDS fields but does NOT remove existing ones. So we CANNOT use additional_params to override max_tokens — both fields would be sent.

**Step 1: Verify the flatten behavior**

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
grep -n "flatten" ~/.cargo/registry/src/*/rig-core-0.33.*/src/completion/mod.rs 2>/dev/null | head
```

**Step 2: Determine fix strategy**

Two options:
- **A:** For reasoning models, skip the rig-core builder entirely and use `raw_openai_compat_infer` (manual HTTP)
- **B:** Build the rig-core request, serialize it, mutate the JSON (remove `max_tokens`, add `max_completion_tokens`), then send raw

Option A is cleaner. The `raw_openai_compat_infer` path already handles reasoning models correctly.

### Task 1.2: Route reasoning model agents through raw HTTP path

**Files:**
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs:60-93`
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs:265-295`

**Step 1: In the completion stream path (~line 82)**

```rust
let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;
let model_id = self.params.model.as_deref().unwrap_or("");

// For reasoning models, DON'T use .max_tokens() — rig-core sends the wrong field.
// Instead, inject max_completion_tokens via additional_params and set max_tokens to None.
if is_reasoning_model(model_id) {
    let request = request_builder
        .additional_params(serde_json::json!({
            "max_completion_tokens": effective_max_tokens
        }))
        .build();
    // max_tokens is Option<u64> = None by default, so it won't be serialized
} else {
    let request = request_builder.max_tokens(effective_max_tokens).build();
}
```

Wait — this depends on whether rig-core serializes max_tokens:None as absent. Need to verify.

**Alternative approach:** If rig-core always includes max_tokens (even as null), we need the raw HTTP path.

**Step 2: Test with reasoning model**

Create a unit test that verifies the request body:

```rust
#[test]
fn reasoning_model_agent_sends_max_completion_tokens() {
    // Build request for o3 model
    // Verify JSON body contains "max_completion_tokens" and NOT "max_tokens"
}
```

**Step 3: Commit**

```
fix(agent): use max_completion_tokens for reasoning models in streaming path
```

---

## Phase 2: ModelCapabilities Catalog (S1, S2)

### Task 2.1: Define `ModelCapabilities` with orthogonal concerns

**Files:**
- Create: `tools/nika-core/src/catalogs/capabilities.rs`
- Modify: `tools/nika-core/src/catalogs/mod.rs`

**Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ── OpenAI o-series: max_completion_tokens + no temperature ──

    #[test]
    fn o3_needs_max_completion_tokens_and_no_temperature() {
        let caps = model_capabilities("openai", "o3");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(!caps.supports_temperature);
    }

    #[test]
    fn o4_mini_needs_max_completion_tokens() {
        let caps = model_capabilities("openai", "o4-mini");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    // ── OpenAI gpt-5.x: max_completion_tokens BUT supports temperature ──

    #[test]
    fn gpt52_needs_max_completion_tokens_but_supports_temperature() {
        let caps = model_capabilities("openai", "gpt-5.2");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(caps.supports_temperature); // gpt-5.x DOES support temperature
    }

    #[test]
    fn gpt54_needs_max_completion_tokens() {
        let caps = model_capabilities("openai", "gpt-5.4");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn gpt54_mini_and_nano() {
        for model in ["gpt-5.4-mini", "gpt-5.4-nano"] {
            let caps = model_capabilities("openai", model);
            assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens,
                "{model} should use max_completion_tokens");
        }
    }

    // ── DeepSeek Reasoner: max_tokens (NOT max_completion_tokens) ──

    #[test]
    fn deepseek_reasoner_uses_max_tokens() {
        let caps = model_capabilities("deepseek", "deepseek-reasoner");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(!caps.supports_temperature); // strip to be safe
    }

    // ── Standard models: everything normal ──

    #[test]
    fn gpt4o_is_standard() {
        let caps = model_capabilities("openai", "gpt-4o");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
        assert!(!caps.supports_thinking);
    }

    #[test]
    fn claude_supports_thinking() {
        let caps = model_capabilities("anthropic", "claude-sonnet-4-6");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
        assert!(caps.supports_thinking);
    }

    // ── xAI grok-4: rejects stop sequences ──

    #[test]
    fn grok4_rejects_stop_sequences() {
        let caps = model_capabilities("xai", "grok-4");
        assert!(!caps.supports_stop_sequences);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn grok3_supports_stop_sequences() {
        let caps = model_capabilities("xai", "grok-3");
        assert!(caps.supports_stop_sequences);
    }

    // ── Custom endpoint: safe defaults ──

    #[test]
    fn unknown_model_gets_safe_defaults() {
        let caps = model_capabilities("openai", "future-model-99");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
        assert!(caps.supports_stop_sequences);
    }

    // ── Edge cases ──

    #[test]
    fn gpt5_dot_variants_all_match() {
        for model in ["gpt-5.2", "gpt-5.2-pro", "gpt-5.2-2025-12-11",
                       "gpt-5.3", "gpt-5.4", "gpt-5.10-turbo"] {
            let caps = model_capabilities("openai", model);
            assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens,
                "{model} should use max_completion_tokens");
        }
    }

    #[test]
    fn gpt41_is_not_reasoning() {
        // gpt-4.1 is NOT a reasoning model (different from gpt-5.x)
        let caps = model_capabilities("openai", "gpt-4.1");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
    }

    #[test]
    fn case_insensitive() {
        let caps = model_capabilities("openai", "GPT-5.2");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn custom_endpoint_qwen_on_vllm() {
        // Custom endpoint model should get standard behavior
        // even if model name looks like something else
        let caps = model_capabilities("h100", "Qwen/Qwen3-8B");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn custom_endpoint_reasoning_model_name() {
        // Edge case: o3-finetune running on vLLM via custom endpoint
        // Should NOT get max_completion_tokens because the endpoint isn't OpenAI
        let caps = model_capabilities("h100", "o3-finetune");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens,
            "Custom endpoints should use max_tokens even for o3-like names");
    }
}
```

**Step 2: Implement**

```rust
//! Model capabilities catalog — single source of truth for model behavior.
//!
//! Every model-specific conditional in the codebase should read from here.
//! Three orthogonal concerns, NOT a single "is_reasoning" flag:
//! 1. Token limit parameter name (max_tokens vs max_completion_tokens)
//! 2. Temperature support
//! 3. Thinking/reasoning mechanism

/// How to send the token limit to the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenLimitParam {
    /// Standard: `"max_tokens"` in JSON body
    MaxTokens,
    /// OpenAI reasoning models: `"max_completion_tokens"`
    MaxCompletionTokens,
}

/// Capabilities of a specific model on a specific provider.
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    pub token_limit_param: TokenLimitParam,
    pub supports_temperature: bool,
    pub supports_stop_sequences: bool,
    pub supports_thinking: bool,
    pub supports_vision: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            token_limit_param: TokenLimitParam::MaxTokens,
            supports_temperature: true,
            supports_stop_sequences: true,
            supports_thinking: false,
            supports_vision: true,
        }
    }
}

/// Resolve capabilities for a given provider + model combination.
///
/// Provider-aware: `o3` on OpenAI gets `max_completion_tokens`,
/// but `o3-finetune` on a custom vLLM endpoint gets `max_tokens`.
pub fn model_capabilities(provider: &str, model: &str) -> ModelCapabilities {
    let lower = model.to_lowercase();
    let provider_lower = provider.to_lowercase();

    // Only apply OpenAI-specific reasoning rules for OpenAI provider
    let is_openai = matches!(provider_lower.as_str(),
        "openai" | "gpt" | "openrouter");

    if is_openai {
        // OpenAI o-series: max_completion_tokens + NO temperature
        let is_o_series = lower == "o1" || lower.starts_with("o1-")
            || lower == "o3" || lower.starts_with("o3-")
            || lower == "o4" || lower.starts_with("o4-");
        if is_o_series {
            return ModelCapabilities {
                token_limit_param: TokenLimitParam::MaxCompletionTokens,
                supports_temperature: false,
                ..Default::default()
            };
        }

        // OpenAI gpt-5.x: max_completion_tokens + YES temperature
        let is_gpt5 = lower == "gpt-5"
            || lower.starts_with("gpt-5-")
            || lower.starts_with("gpt-5.");
        if is_gpt5 {
            return ModelCapabilities {
                token_limit_param: TokenLimitParam::MaxCompletionTokens,
                supports_temperature: true,
                ..Default::default()
            };
        }
    }

    // Anthropic Claude: supports thinking, standard params
    if matches!(provider_lower.as_str(), "anthropic" | "claude")
        || lower.starts_with("claude")
    {
        return ModelCapabilities {
            supports_thinking: true,
            ..Default::default()
        };
    }

    // DeepSeek Reasoner: standard max_tokens, strip temperature (safety)
    if provider_lower == "deepseek" && lower == "deepseek-reasoner" {
        return ModelCapabilities {
            supports_temperature: false,
            supports_vision: false,
            ..Default::default()
        };
    }

    // DeepSeek Chat: no vision
    if provider_lower == "deepseek" {
        return ModelCapabilities {
            supports_vision: false,
            ..Default::default()
        };
    }

    // xAI grok-4: rejects stop sequences
    if matches!(provider_lower.as_str(), "xai" | "grok") && lower == "grok-4" {
        return ModelCapabilities {
            supports_stop_sequences: false,
            ..Default::default()
        };
    }

    // Everything else: safe defaults
    ModelCapabilities::default()
}
```

**Step 3: Run tests**

```bash
cargo test -p nika-core --lib capabilities -v
```

**Step 4: Commit**

```
feat(core): ModelCapabilities catalog — orthogonal concerns for model behavior
```

### Task 2.2: Migrate all callers

**Files:**
- Modify: `tools/nika-engine/src/provider/rig/mod.rs` — `is_reasoning_model` becomes thin wrapper
- Modify: `tools/nika-engine/src/provider/rig/mod.rs:738` — use `caps.token_limit_param`
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs` — use `model_capabilities`
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs` — use `model_capabilities`
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/mod.rs:200-214` — stop sequences

**Step 1: In `raw_openai_compat_infer` (rig/mod.rs:738)**

Replace reasoning boolean with capabilities:

```rust
use nika_core::catalogs::capabilities::{model_capabilities, TokenLimitParam};

let caps = model_capabilities(/* provider context */, model);
let mut body = serde_json::json!({
    "model": model,
    "messages": messages,
});

match caps.token_limit_param {
    TokenLimitParam::MaxCompletionTokens => {
        body["max_completion_tokens"] = serde_json::json!(max_tokens);
    }
    TokenLimitParam::MaxTokens => {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }
}

if let Some(temp) = temperature {
    if caps.supports_temperature {
        body["temperature"] = serde_json::json!(temp);
    } else {
        tracing::warn!(model, "temperature stripped for model that doesn't support it");
    }
}
```

**Step 2: In stop sequences (rig_agent_loop/mod.rs:200)**

```rust
let caps = model_capabilities(provider, model_id);
if !caps.supports_stop_sequences {
    tracing::warn!(model = model_id, "stop sequences stripped — model doesn't support them");
    return None;
}
```

**Step 3: Run tests, commit**

```
refactor(engine): migrate all model checks to ModelCapabilities catalog
```

---

## Phase 3: Pricing & Cost Catalog (B8, B9)

### Task 3.1: Add missing model pricing

**Files:**
- Modify: `tools/nika-core/src/catalogs/cost.rs`

**Step 1: Add pricing entries**

```rust
// GPT-5.4 family
ModelPricing {
    provider: "OpenAI",
    model_pattern: "gpt-5.4-nano",
    input_per_million: 0.10,    // TODO: verify exact pricing
    output_per_million: 0.40,
},
ModelPricing {
    provider: "OpenAI",
    model_pattern: "gpt-5.4-mini",
    input_per_million: 0.40,
    output_per_million: 1.60,
},
ModelPricing {
    provider: "OpenAI",
    model_pattern: "gpt-5.4",
    input_per_million: 2.00,
    output_per_million: 8.00,
},

// xAI Grok-4
ModelPricing {
    provider: "xAI",
    model_pattern: "grok-4",
    input_per_million: 3.00,    // TODO: verify exact pricing
    output_per_million: 15.00,
},
```

**Step 2: Add tests, commit**

```
feat(core): add gpt-5.4 and grok-4 pricing to cost catalog
```

---

## Phase 4: Contract Tests (S3, S5)

### Task 4.1: Verify defaults are not retired

**Files:**
- Modify: `tools/nika-core/src/catalogs/resolver.rs` (tests section)

```rust
/// Canary test — fails when a default model gets retired.
/// Update defaults BEFORE the retirement date.
#[test]
fn default_models_are_not_retired() {
    let retired = [
        "gpt-4o", "gpt-4o-mini", "gpt-4.1", "gpt-4.1-mini", "gpt-4.1-nano",
        "gpt-4-turbo", "gpt-3.5-turbo",
        "gemini-1.5-pro", "gemini-1.5-flash", "gemini-2.0-flash",
    ];
    for (provider, model) in PROVIDER_DEFAULTS.iter() {
        assert!(
            !retired.contains(model),
            "Default model for '{provider}' is '{model}' which is RETIRED — update PROVIDER_DEFAULTS"
        );
    }
}
```

### Task 4.2: Contract test for token parameter correctness

**Files:**
- Create or modify: `tools/nika-core/src/catalogs/capabilities.rs` (tests section)

```rust
/// Every OpenAI model that needs max_completion_tokens must be detected.
/// If this test fails, a new model was added to pricing but not capabilities.
#[test]
fn all_openai_reasoning_models_in_catalog() {
    use crate::catalogs::cost::find_pricing;
    let openai_reasoning = [
        "o1", "o3", "o3-mini", "o3-pro", "o4-mini",
        "gpt-5", "gpt-5.2", "gpt-5.2-pro", "gpt-5.4", "gpt-5.4-mini", "gpt-5.4-nano",
    ];
    for model in openai_reasoning {
        let caps = model_capabilities("openai", model);
        assert_eq!(
            caps.token_limit_param, TokenLimitParam::MaxCompletionTokens,
            "OpenAI model '{model}' should use max_completion_tokens"
        );
        // Also verify pricing exists
        assert!(find_pricing(model).is_some(), "Missing pricing for '{model}'");
    }
}
```

**Commit:**

```
test(core): contract tests — retired defaults, reasoning model coverage
```

---

## Phase 5: SDK Hardening (S4)

### Task 5.1: Defensive config validation

**Files:**
- Modify: `/Users/thibaut/dev/supernovae/nika-client/src/index.ts:16-21`

```typescript
constructor(config: NikaConfig) {
  if (!config.url) {
    throw new TypeError(
      'NikaConfig.url is required — pass the nika serve URL.\n'
      + '  Example: new Nika({ url: "http://localhost:3000", token: "..." })\n'
      + '  Or use: new Nika({ url: process.env.NIKA_URL!, token: process.env.NIKA_TOKEN! })'
    );
  }
  if (!config.url.startsWith('http://') && !config.url.startsWith('https://')) {
    throw new TypeError(
      `NikaConfig.url must start with http:// or https://, got: "${config.url}"`
    );
  }
  if (!config.token) {
    throw new TypeError(
      'NikaConfig.token is required — set NIKA_TOKEN env var or pass token in config.'
    );
  }
  // ... rest unchanged
}
```

### Task 5.2: Add `Nika.fromEnv()` factory

```typescript
/**
 * Create a Nika client from environment variables.
 * Reads NIKA_URL and NIKA_TOKEN. Throws if missing.
 */
static fromEnv(overrides?: Partial<NikaConfig>): Nika {
  const url = overrides?.url ?? process.env.NIKA_URL;
  const token = overrides?.token ?? process.env.NIKA_TOKEN;
  if (!url) {
    throw new TypeError('NIKA_URL environment variable is not set');
  }
  if (!token) {
    throw new TypeError('NIKA_TOKEN environment variable is not set');
  }
  return new Nika({ ...overrides, url, token });
}
```

**Commit:**

```
feat(sdk): defensive validation + Nika.fromEnv() factory for zero-config usage
```

---

## Critical Questions

### Q1: Should `model_capabilities` be provider-aware for custom endpoints?

**Current plan:** YES — `o3` on OpenAI gets `max_completion_tokens`, but `o3-finetune` on a custom vLLM endpoint gets `max_tokens` (safe default).

**Risk:** A user running o3 via vLLM will get `max_tokens` instead of `max_completion_tokens`. vLLM supports both, so this is safe. Ollama does NOT support `max_completion_tokens`, so the safe default is correct.

**Decision needed:** Should custom endpoints inherit OpenAI rules if the base URL is openai-compatible? Or always use safe defaults?

### Q2: What about OpenRouter?

OpenRouter proxies to any model. A request for `o3` via OpenRouter should get `max_completion_tokens`. Our current plan marks `openrouter` as OpenAI-equivalent in `model_capabilities()`.

**Risk:** OpenRouter models from non-OpenAI providers (e.g., `anthropic/claude-3-opus`) would incorrectly get `max_completion_tokens` if the model name starts with `o3` — unlikely but possible.

### Q3: Temperature and o-series — strip or pass?

Research shows conflicting reports:
- o1 definitely rejects temperature != 1
- o3/o4-mini reportedly accept temperature now
- Safest: strip with warning (current behavior)
- Alternative: pass and handle 400 gracefully

**Recommendation:** Keep stripping with WARN. The cost of a silent 400 is higher than the cost of a lost temperature setting.

### Q4: Gemini thinking support?

Gemini 2.5/3 have thinking capabilities (`thinkingBudget` / `thinking_level`). Nika currently only supports Claude thinking. Adding Gemini thinking is a separate feature but should be planned.

**Recommendation:** Track as separate task post-launch. The `ModelCapabilities` catalog already has `supports_thinking` which can be extended to include `ThinkingMechanism::GeminiLevel` later.

### Q5: Should `nika model check <model>` exist?

A CLI command that verifies: model exists in catalog, pricing known, capabilities resolved, API key present, optionally sends a test request.

**Recommendation:** YES. High value, quick to implement. Would have caught the gpt-4o retirement instantly.

### Q6: Stale pricing — process for keeping current?

No CI catches stale pricing. Options:
- **A:** Manual update per release (current — fragile)
- **B:** `nika model list --check-remote` that queries provider APIs and diffs
- **C:** Weekly CI job that flags outdated defaults

**Recommendation:** B for now. C post-launch.

### Q7: Should `is_reasoning_model` be REMOVED?

Once `ModelCapabilities` is in place, the standalone `is_reasoning_model` function is a trap — callers might use it instead of the catalog. Should it be deprecated?

**Recommendation:** Keep as a thin wrapper that delegates to `model_capabilities().token_limit_param == MaxCompletionTokens || !supports_temperature`. Mark internal. Remove public export.

---

## Execution Order

```
Phase 0 (defaults)         ~10 min   URGENT — users hit 404 on first use
Phase 1 (streaming fix)    ~30 min   HIGH — agent path broken for reasoning models
Phase 2 (catalog)          ~60 min   HIGH — eliminate scattered hardcoding forever
Phase 3 (pricing)          ~15 min   MEDIUM — cosmetic but professional
Phase 4 (contract tests)   ~20 min   MEDIUM — prevent regression
Phase 5 (SDK)              ~20 min   MEDIUM — prevent Nicolas-class crashes
```

**Total:** ~2.5h of focused work.

---

## Verification Criteria

- [ ] `cargo test -p nika-core --lib` passes (defaults, capabilities, pricing, contract tests)
- [ ] `cargo check -p nika-engine` passes
- [ ] `model_capabilities("openai", "gpt-5.2").token_limit_param == MaxCompletionTokens`
- [ ] `model_capabilities("openai", "gpt-5.2").supports_temperature == true`
- [ ] `model_capabilities("openai", "o3").supports_temperature == false`
- [ ] `model_capabilities("deepseek", "deepseek-reasoner").token_limit_param == MaxTokens`
- [ ] `model_capabilities("xai", "grok-4").supports_stop_sequences == false`
- [ ] `model_capabilities("h100", "o3-finetune").token_limit_param == MaxTokens` (custom endpoint safe)
- [ ] Default OpenAI model is NOT `gpt-4o` (retired)
- [ ] `Nika({ url: undefined, token: 'x' })` throws helpful error
- [ ] `Nika.fromEnv()` reads NIKA_URL + NIKA_TOKEN
- [ ] No `starts_with("o1") || starts_with("o3")` outside ModelCapabilities
