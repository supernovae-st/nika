# Stdlib v0.1 · Providers

> The canonical <!-- canon:providers -->14<!-- /canon --> providers shipped with v0.1-compliant engines. Each
> provider implements the same interface (LLM chat completion + optional
> streaming + vision + structured output) against a different backend.
> You select one with a single `model: <provider>/<name>` field.

---

## Model selection · ONE field · `model: <provider>/<name>`

You select an LLM with a **single `model:` field** in the form
`<provider>/<model-name>`, the de-facto standard convention (LiteLLM ·
OpenRouter · Vercel AI SDK · PydanticAI all converged on it). There is **no
separate `provider:` field** · the provider is the prefix.

```yaml
infer:
  model: ollama/llama3.1                 # local · no key
  prompt: "..."

infer:
  model: anthropic/claude-sonnet-4-6     # cloud · same shape
  prompt: "..."
```

**Why one field, not two** · a model belongs to a provider · two separate
fields let you write the silent-nonsense combination `provider: anthropic` +
`model: gpt-4o`. One `<provider>/<name>` string is atomic, self-documenting,
trivially swappable, and the industry standard. The same open model served by
different backends disambiguates cleanly · `groq/llama-3.1-70b` vs
`ollama/llama-3.1-70b`.

**Parameterize it** · combine with typed inputs to run one workflow
against any backend ·

```yaml
vars:
  model: { type: string, default: "ollama/llama3.1" }
tasks:
  - id: x
    infer: { model: "${{ vars.model }}", prompt: "..." }
# nika run flow.yaml --var model=mistral/mistral-large   ← same workflow, cloud
```

---

## The 14 canonical providers

| Provider | Backend | Local? | Auth |
|---|---|---|---|
| `ollama` | Ollama daemon (`localhost:11434`) | **local** | none |
| `lmstudio` | LM Studio (`localhost:1234/v1`) | **local** | none |
| `llamacpp` | llama.cpp `llama-server` (`localhost:8080/v1`) | **local** | none |
| `localai` | LocalAI (`localhost:8080/v1` · OpenAI drop-in · multi-backend) | **local** | none |
| `vllm` | vLLM OpenAI server (`localhost:8000/v1` · high-throughput · self-hosted) | **local** | none |
| `mistral` | Mistral AI API (EU · sovereign-leaning) | cloud | `${{ secrets.* }}` |
| `anthropic` | Anthropic Claude API | cloud | `${{ secrets.* }}` |
| `openai` | OpenAI API (+ the universal OpenAI-compat escape hatch · see below) | cloud | `${{ secrets.* }}` |
| `openrouter` | OpenRouter gateway (one key · every major model · cross-vendor fallback) | cloud | `${{ secrets.* }}` |
| `groq` | Groq Cloud (fastest open-weight) | cloud | `${{ secrets.* }}` |
| `deepseek` | DeepSeek API (reasoning · cost-efficient) | cloud | `${{ secrets.* }}` |
| `gemini` | Google Gemini API (long context · multimodal) | cloud | `${{ secrets.* }}` |
| `xai` | xAI Grok API | cloud | `${{ secrets.* }}` |
| `mock` | deterministic test fixture · no LLM call | test | none |

A Stdlib v0.1-compliant engine MUST ship all **14** (5 local · 8 cloud · 1 test).
Any *other* OpenAI-compatible local server (Jan · llamafile · KoboldCpp ·
text-generation-webui · a custom one) routes through the **`openai` escape
hatch** below (no new provider name needed).

> **2026-06-10 · `openrouter` promoted from escape hatch to named provider**
> (D-2026-06-10-N2). Earlier revisions routed OpenRouter through
> `openai`+`base_url`. That override **hijacks the `openai` prefix** (you
> cannot reach vanilla OpenAI and OpenRouter from the same engine config),
> and the largest model-aggregation gateway deserves first-class one-field
> selection. Together · Fireworks · custom gateways still use the escape hatch.

## Local vs cloud · the prefix decides

The **provider prefix IS the local/cloud signal** (no separate `local:` flag,
no hidden config to read):

```
ollama/…  lmstudio/…  llamacpp/…  localai/…  vllm/…   → LOCAL · localhost · no key · sovereign
anthropic/…  openai/…  groq/…  mistral/…  deepseek/…   → CLOUD · remote API · key via ${{ secrets.* }}
  …  gemini/…  xai/…  openrouter/…
mock/…                                                 → TEST  · deterministic fixture
```

Sovereignty · **local-first** · nothing leaves the machine unless a
cloud provider is *explicitly* selected. `ollama/llama3.1` makes a sovereign,
zero-cloud run trivial.

All 5 local providers are external **HTTP servers** (OpenAI-compatible API · the
engine talks to them over localhost). They are NOT the in-process GGUF runtime
`native`, which was DEFERRED (mistral.rs crashed
the host). It re-enters stdlib v0.x when a candle/llama.cpp binding stabilizes +
30-day crash-free cohort + cross-platform conformance. **The named local
providers are ergonomic shortcuts; the long tail uses the escape hatch.**

## The `openai` escape hatch · any OpenAI-compatible server

Most local servers (and many cloud gateways · Together · Fireworks · etc.)
speak the **OpenAI chat-completions protocol**. Rather than mint a provider
name for every one, the `openai` provider accepts a `base_url` override in
**engine config** (never in the workflow) and routes there:

```
model: openai/<model-name>          # workflow · unchanged · selects the model
OPENAI_BASE_URL=http://localhost:1337/v1   # engine config · points at Jan
```

This is the LiteLLM pattern: **named providers for the popular backends ·
`openai`+base_url for everything else.** It is how Jan · llamafile ·
KoboldCpp · text-generation-webui · and any custom OpenAI-compatible server
run today: zero spec change, the stdlib stays curated, the long tail is
covered. Adding a *new named* provider later (its own prefix) is an
additive stdlib bump: `openrouter` (2026-06-10 · D-2026-06-10-N2) is the
first such promotion.

## Provider config lives OUTSIDE the workflow

A workflow only *selects* (`model: <provider>/<name>`). It never inlines
`base_url` or keys. The engine resolves each provider's endpoint + auth from
**engine/provider config** (cloud → `${{ secrets.* }}` · local → localhost,
no key). This keeps workflows portable + secrets masked.

---

## Common contract · all providers

Every provider supports ·

```yaml
infer:
  prompt: "..."                # required
  system: "..."                # optional
  model: <provider>/<name>     # one field · e.g. ollama/llama3.1
  temperature: 0.0 to 2.0      # optional
  max_tokens: <int>            # optional
  schema: { ... }              # optional · structured output
```

The engine MUST route the request to the provider's API, format the response, and return it as task output.

Errors map to `NIKA-PROVIDER-NNN` codes with the provider-specific status.

---

## Provider-by-provider

### `ollama` · **local-first sovereign**

```yaml
infer:
  model: ollama/llama3.1                 # or any pulled Ollama model
  prompt: "..."
```

**Backend** · the Ollama daemon (`http://localhost:11434`) · OpenAI-compatible API · the engine talks to it over HTTP.

**Models** · any model pulled into Ollama (`llama3.1` · `qwen2.5` · `mistral` · `gemma2` · etc.) · pass-through.

**Auth** · none (localhost).

**Features** · 100% local · zero cloud egress · GPU-accelerated by Ollama. The sovereign default: `model: ollama/<x>` runs offline / air-gapped, zero vendor lock-in. (NOT the in-process `native` GGUF runtime, which is deferred: Ollama is an external server, stable.)

---

### `lmstudio` · **local-first sovereign**

```yaml
infer:
  model: lmstudio/qwen2.5-14b-instruct
  prompt: "..."
```

**Backend** · LM Studio's local server (`http://localhost:1234/v1`) · OpenAI-compatible API.

**Models** · whatever you load in LM Studio · pass-through.

**Auth** · none (localhost · dummy key tolerated).

**Features** · 100% local · zero cloud egress · GUI model management.

---

### `llamacpp` · **local-first sovereign**

```yaml
infer:
  model: llamacpp/qwen2.5-7b-instruct
  prompt: "..."
```

**Backend** · llama.cpp's `llama-server` (`http://localhost:8080/v1`) · OpenAI-compatible API · the reference local inference server.

**Models** · whatever GGUF you serve with `llama-server` · pass-through.

**Auth** · none (localhost).

**Features** · 100% local · zero cloud egress · GPU + CPU · the leanest local server.

---

### `localai` · **local-first sovereign**

```yaml
infer:
  model: localai/llama-3.1-8b-instruct
  prompt: "..."
```

**Backend** · LocalAI (`http://localhost:8080/v1`) · OpenAI drop-in · multi-backend (llama.cpp · transformers · diffusers · etc.).

**Models** · whatever LocalAI has configured · pass-through.

**Auth** · none (localhost · optional API key).

**Features** · 100% local · zero cloud egress · multi-modal · OpenAI API surface for the whole local stack.

---

### `vllm` · **local / self-hosted**

```yaml
infer:
  model: vllm/meta-llama-3.1-70b-instruct
  prompt: "..."
```

**Backend** · vLLM's OpenAI server (`http://localhost:8000/v1`) · high-throughput batched serving.

**Models** · whatever vLLM is serving · pass-through.

**Auth** · none locally (optional token if the deployment sets one).

**Features** · highest throughput for open-weight models · local OR self-hosted on your own GPU box (sovereign as long as it's your infra).

---

### `mistral`

```yaml
infer:
  model: mistral/mistral-large-latest
  prompt: "..."
```

**Models** · `mistral-large-latest` · `mistral-medium-latest` · `mistral-small-latest` · `codestral-latest` · `pixtral-large-latest` (vision) · (pass-through).

**Auth** · `MISTRAL_API_KEY` env var.

**Features** · tool use · vision (pixtral) · structured output.

---

### `anthropic`

```yaml
infer:
  model: anthropic/claude-3-5-sonnet
  prompt: "..."
```

**Models** · `claude-3-5-sonnet` · `claude-3-5-haiku` · `claude-3-opus` · (and any newer · model name pass-through).

**Auth** · `ANTHROPIC_API_KEY` env var · OR engine config.

**Features** · tool use · vision · extended thinking · structured output via JSON Schema.

**Specific fields** ·
```yaml
infer:
  model: anthropic/claude-sonnet-4-6
  thinking:
    enabled: true
    budget_tokens: 4000
```

---

### `openai`

```yaml
infer:
  model: openai/gpt-4o
  prompt: "..."
```

**Models** · `gpt-4o` · `gpt-4o-mini` · `gpt-4-turbo` · `o1` · (pass-through).

**Auth** · `OPENAI_API_KEY` env var.

**Features** · tool use · vision · structured output (JSON mode).

**Escape hatch** · the openai provider routes ANY OpenAI-compatible endpoint via the `OPENAI_BASE_URL` engine-config override (see « The `openai` escape hatch » above). Covers the local servers without their own named provider (**Jan · llamafile · KoboldCpp · text-generation-webui**) plus cloud gateways (**Together · Fireworks**) and custom servers. Providers with their own named prefix (`openrouter` · `ollama` · `lmstudio` · `llamacpp` · `localai` · `vllm`) don't need this.

---

### `openrouter`

```yaml
infer:
  model: openrouter/meta-llama/llama-3.1-70b-instruct
  prompt: "..."
```

The cross-vendor **gateway** · one API key reaches every major model
(Anthropic · OpenAI · Meta · Mistral · Google · open-weight). Promoted from
the `openai` escape hatch 2026-06-10 (D-2026-06-10-N2): a named prefix means
OpenRouter and vanilla `openai` coexist in one engine config.

**Models** · OpenRouter ids are themselves `vendor/model`: the workflow form
is `openrouter/<vendor>/<model>` (everything after the first `/` passes
through verbatim). E.g. `openrouter/anthropic/claude-sonnet-4-6` ·
`openrouter/meta-llama/llama-3.1-70b-instruct` · `openrouter/deepseek/deepseek-r1`.

**Auth** · `OPENROUTER_API_KEY` env var.

**Features** · OpenAI-compatible chat completions · streaming · tool use ·
structured output · provider-side model fallback/routing.

**When to prefer it** · cross-vendor benchmarking from ONE workflow
(`--var model=openrouter/...`) · models with no native named provider ·
provider-side failover. For a vendor's flagship via its own API (latency ·
native features · billing) prefer the direct provider (`anthropic/…` etc.).

---

### `groq`

```yaml
infer:
  model: groq/llama-3.3-70b-versatile
  prompt: "..."
```

**Models** · open-weight models on Groq LPU (Llama · Mixtral · Gemma · etc.) · pass-through.

**Auth** · `GROQ_API_KEY` env var.

**Features** · fast inference · tool use (some models) · structured output (some models).

---

### `deepseek`

```yaml
infer:
  model: deepseek/deepseek-chat
  prompt: "..."
```

**Models** · `deepseek-chat` · `deepseek-reasoner` · (pass-through).

**Auth** · `DEEPSEEK_API_KEY` env var.

**Features** · reasoning model · structured output.

---

### `gemini`

```yaml
infer:
  model: gemini/gemini-2.0-flash
  prompt: "..."
```

**Models** · `gemini-2.0-flash` · `gemini-2.0-pro` · `gemini-1.5-pro` · (pass-through).

**Auth** · `GOOGLE_API_KEY` env var · OR ADC.

**Features** · long context (1M+ tokens) · multimodal native (text · image · audio · video) · structured output.

---

### `xai`

```yaml
infer:
  model: xai/grok-2-latest
  prompt: "..."
```

**Models** · `grok-2-latest` · `grok-vision-latest` · (pass-through).

**Auth** · `XAI_API_KEY` env var.

**Features** · vision · real-time data option.

---

### `mock`

```yaml
infer:
  model: mock/mock-deterministic
  prompt: "..."
```

**Backend** · deterministic test fixture · returns a configured response.

**Models** ·
- **`echo` · THE canonical test model** (`model: mock/echo`, what every
  canonical example uses) · returns the prompt text **verbatim** as the
  output · zero network · zero entropy (bit-identical across runs/engines).
  With a `schema:` declared · returns `{}` shaped to the schema's required
  scalar defaults (string `""` · number `0` · boolean `false` · array `[]` ·
  object recursed): deterministic · validates · carries no meaning (test
  the SHAPE of your DAG · not model quality).
- `mock-deterministic` · returns the prompt verbatim (echo's long-form alias)
- `mock-error` · returns a configured error
- `mock-streaming` · streams a configured response
- `mock-json` · returns structured JSON matching a configured schema

The configured-response forms (`mock-error` · `mock-streaming` ·
`mock-json`) read their fixture from **engine config** (NOT workflow YAML ·
the workflow stays portable). The behavioral conformance fixtures
(post-announce) pin their exact contract. `mock/echo` is fully normative
TODAY (the static + example gates rely on it).

**Auth** · none.

**Use case** · workflow tests · CI · conformance suite · zero LLM cost · deterministic.

---

## Cross-provider semantics

When using the same workflow with different providers ·

- The **prompt** is provider-agnostic (raw text)
- The **system message** is provider-agnostic
- The **temperature** is normalized to 0.0-2.0 (engine maps to provider-specific scale)
- The **structured output** uses JSON Schema (engine adapts to provider's native mechanism · JSON mode · function calling · etc.)
- The **vision input** is provider-agnostic in the workflow · the engine adapts

A workflow can switch providers with one line change (`model: ollama/llama3.1` → `model: mistral/mistral-large`, or any other `<provider>/<name>`) for most use cases.

---

## Forward-compat

New providers MAY enter stdlib v0.x. Provider-specific options that don't map to the common contract are passed as `provider_options:` (forward-compat extension point · not in v0.1 conformance).

```yaml
# Forward-compat (post-v0.1)
infer:
  model: anthropic/claude-3-5-sonnet
  provider_options:
    cache_control: { type: "ephemeral" }
    beta: ["custom-feature"]
```

The reference engine implements `provider_options:` as best-effort pass-through.

---

🦋 *<!-- canon:providers -->14<!-- /canon --> providers · 1 contract · sovereignty preserved.*
