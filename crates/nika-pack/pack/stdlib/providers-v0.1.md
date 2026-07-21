# Stdlib v0.1 · Providers

> The canonical <!-- canon:providers -->17<!-- /canon --> providers shipped with v0.1-compliant engines. Each
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
  model: ollama/qwen3.5:9b                 # local · no key
  prompt: "..."

infer:
  model: anthropic/claude-sonnet-4-6     # cloud · same shape
  prompt: "..."
```

**Why one field, not two** · a model belongs to a provider · two separate
fields let you write the silent-nonsense combination `provider: anthropic` +
`model: gpt-4o`. One `<provider>/<name>` string is atomic, self-documenting,
trivially swappable, and the industry standard. The same open model served by
different backends disambiguates cleanly · `groq/qwen3.5-32b` vs
`ollama/qwen3.5:27b`.

**Parameterize it** · combine with typed inputs to run one workflow
against any backend ·

```yaml
inputs:
  model: { type: string, default: "ollama/qwen3.5:9b" }
tasks:
  x:
    infer: { model: "${{ inputs.model }}", prompt: "..." }
# nika run flow.yaml --var model=mistral/mistral-large   ← same workflow, cloud
```

---

## The 17 canonical providers

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
| `huggingface` | HF Inference Providers router (100+ open-weights · 18 backends · zero markup) | cloud | `${{ secrets.* }}` |
| `nvidia` | NVIDIA API (Nemotron 3 · Open Model License · NIM self-hostable) | cloud | `${{ secrets.* }}` |
| `moonshot` | Moonshot AI API (Kimi K3 · frontier open-weight · thinking model · CN) | cloud | `${{ secrets.* }}` |
| `groq` | Groq Cloud (fastest open-weight) | cloud | `${{ secrets.* }}` |
| `deepseek` | DeepSeek API (reasoning · cost-efficient) | cloud | `${{ secrets.* }}` |
| `gemini` | Google Gemini API (long context · multimodal) | cloud | `${{ secrets.* }}` |
| `xai` | xAI Grok API | cloud | `${{ secrets.* }}` |
| `mock` | deterministic test fixture · no LLM call | test | none |

A Stdlib v0.1-compliant engine MUST ship all **17** (5 local · 11 cloud · 1 test).
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
  …  gemini/…  xai/…  openrouter/…  huggingface/…  nvidia/…  moonshot/…
mock/…                                                 → TEST  · deterministic fixture
```

Sovereignty · **local-first** · nothing leaves the machine unless a
cloud provider is *explicitly* selected. `ollama/qwen3.5:9b` makes a sovereign,
zero-cloud run trivial.

All 5 local providers are external **HTTP servers** (OpenAI-compatible API · the
engine talks to them over localhost). They are NOT the in-process GGUF runtime
`native`, which was DEFERRED (mistral.rs crashed
the host). It re-enters stdlib v0.x when a candle/llama.cpp binding stabilizes +
30-day crash-free cohort + cross-platform conformance. **The named local
providers are ergonomic shortcuts; the long tail uses the escape hatch.**

## Transport deadline · the task `timeout:` governs the provider call

The task-level [`timeout:`](../spec/03-dag.md#timeout--optional--task-level-timeout-go-duration-string)
(Go-duration string) **governs the provider HTTP deadline**: a task
declaring `timeout: "7m"` gives the provider round-trip those 7 minutes.
A fixed internal HTTP default MUST NOT undercut the declared budget.

When NO `timeout:` is declared, the default deadline is per provider
**class** ·

```
LOCAL  (ollama · lmstudio · llamacpp · localai · vllm)               ≥ 300s
CLOUD  (mistral · anthropic · openai · openrouter · huggingface ·      30s
        nvidia · groq · deepseek · gemini · xai · moonshot)
```

A local model routinely needs minutes for ONE completion on consumer
hardware — a 14B model cannot answer a real prompt in 30s, and a
30s-everywhere default silently kills every serious local-first workflow
(408 before the model finishes thinking). Local-first only works when the
defaults respect local reality. The class is keyed on the **canonical
provider id** (the table above) · a `base_url` override never flips it.

Two honest bounds (reference-engine values · pinned by its wire tests) ·

- **600s transport ceiling on a fully-silent connection** · a
  non-streaming completion delivers ZERO bytes while the model computes,
  so the transport cannot tell *thinking* from *dead*. A connection that
  has delivered nothing is reaped at 600s: a `timeout:` longer than the
  ceiling still bounds the task, but only a connection that starts
  delivering can use it.
- **Streaming carries only an EXPLICIT budget** · an SSE generation
  legitimately outlives any fixed total deadline. A declared `timeout:`
  rides the streaming request; when none is declared, the idle-read guard
  reaps a STALLED stream instead of capping a healthy one.

The task clock stays authoritative: `timeout:` bounds the **whole task**
(retries + backoff included · [03-dag](../spec/03-dag.md#timeout--optional--task-level-timeout-go-duration-string)) ·
this section defines what the provider leg of that budget does.

## The `openai` escape hatch · any OpenAI-compatible server

Most local servers (and many cloud gateways · Together · Fireworks · etc.)
speak the **OpenAI chat-completions protocol**. Rather than mint a provider
name for every one, the `openai` provider accepts a `base_url` override in
**engine config** (never in the workflow) and routes there:

```
model: openai/<model-name>                                       # workflow · unchanged · selects the model
NIKA_OPENAI_BASE_URL=http://localhost:1337/v1/chat/completions   # engine config · the COMPLETE endpoint, verbatim (points at Jan)
NIKA_OPENAI_API_KEY=<key>                                        # engine config · the scoped key (omit for keyless local)
```

The override is `NIKA_<ID>_BASE_URL` · the canonical provider id upper-cased
(here `NIKA_OPENAI_BASE_URL`), and its value is the **complete endpoint,
verbatim** · the engine calls exactly the URL you write and appends nothing.
A cloud endpoint carries its full path (`https://api.moonshot.ai/v1/chat/completions`
is the shape moonshot's own adapter targets · a custom OpenAI-compatible host
reads the same way), and a local one keeps its `/v1/chat/completions` too. It
pairs with the scoped `NIKA_<ID>_API_KEY`. This explicit per-provider pair is
the minimal trust surface by design · no implicit base-plus-suffix assembly,
no bare `OPENAI_BASE_URL` silently shared across providers.

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
  model: <provider>/<name>     # one field · e.g. ollama/qwen3.5:9b
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
  model: ollama/qwen3.5:9b                 # or any pulled Ollama model
  prompt: "..."
```

**Backend** · the Ollama daemon (`http://localhost:11434`) · OpenAI-compatible API · the engine talks to it over HTTP.

**Models** · any model pulled into Ollama (`qwen3.5:9b` · `qwen2.5` · `mistral` · `gemma2` · etc.) · pass-through.

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
  model: openai/gpt-5.2
  prompt: "..."
```

**Models** · `gpt-5.2` · `gpt-5-mini` · `gpt-5-nano` · the o-series · (pass-through — any id the live API serves works verbatim, `gpt-4o` included).

**Auth** · `OPENAI_API_KEY` env var.

**Features** · tool use · vision · structured output (JSON mode).

**Escape hatch** · the openai provider routes ANY OpenAI-compatible endpoint via the `NIKA_OPENAI_BASE_URL` engine-config override (the complete endpoint, verbatim · paired with `NIKA_OPENAI_API_KEY` · see « The `openai` escape hatch » above). Covers the local servers without their own named provider (**Jan · llamafile · KoboldCpp · text-generation-webui**) plus cloud gateways (**Together · Fireworks**) and custom servers. Providers with their own named prefix (`openrouter` · `ollama` · `lmstudio` · `llamacpp` · `localai` · `vllm`) don't need this.

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

### `huggingface`

```yaml
infer:
  model: huggingface/Qwen/Qwen3.5-9B:cheapest
  prompt: "..."
```

The **open-weight house** · one `HF_TOKEN` reaches 100+ top open models
across 18 backend providers (Groq · Cerebras · Together · Fireworks ·
Scaleway · OVHcloud · …) at provider passthrough prices (zero markup).
Promoted 2026-07-05 (ADR-104 · the openrouter precedent applied to the
hub-router access category).

**Models** · Hub ids are `org/model` with an optional ROUTING suffix: the
workflow form is `huggingface/<org>/<model>[:<provider>|:fastest|:cheapest]`
(everything after the first `/` passes through verbatim — inner slash AND
colon included). E.g. `huggingface/Qwen/Qwen3.5-397B-A17B:fastest` ·
`huggingface/openai/gpt-oss-120b:groq` ·
`huggingface/deepseek-ai/DeepSeek-V4-Flash`.

**Auth** · `HF_TOKEN` env var (fine-grained token · « Make calls to
Inference Providers » permission).

**Features** · OpenAI-compatible chat completions · streaming · tool use ·
structured output (per backend) · `:fastest`/`:cheapest` policy routing ·
org billing.

**When to prefer it** · running OPEN-WEIGHT models serverless without
picking a backend vendor · cost/latency policy routing · the sovereignty
ladder's middle rung (open weights · swappable backends · EU providers
available). For a fully local run prefer `ollama/…`; the same GGUFs pull
locally via `ollama run hf.co/<org>/<repo>`.

---

### `nvidia`

```yaml
infer:
  model: nvidia/nvidia/nemotron-3-super-120b-a12b
  prompt: "..."
```

The **NVIDIA API** (`integrate.api.nvidia.com`) · the Nemotron 3 family
(Nano 30B-A3B · Super 120B-A12B · Ultra 550B-A55B · NVIDIA Open Model
License · agentic-first) plus hosted open models (121 live). Promoted
2026-07-05 (ADR-104): self-hosted **NIM containers expose the exact same
OpenAI-compatible surface**, so one provider name covers cloud AND
sovereign self-host (engine-config `base_url` override points at your
NIM).

**Models** · catalog ids are `org/model`: the workflow form is
`nvidia/<org>/<model>`. E.g. `nvidia/nvidia/nemotron-3-super-120b-a12b` ·
`nvidia/nvidia/nemotron-3-nano-30b-a3b` · `nvidia/deepseek-ai/deepseek-r1`.

**Auth** · `NVIDIA_API_KEY` env var (`nvapi-…` from build.nvidia.com ·
free developer tier · ~40 RPM baseline).

**Features** · OpenAI-compatible chat completions · streaming · tool use ·
JSON mode · NVFP4-served flagships.

**When to prefer it** · the Nemotron family at full size · enterprise GPU
serving with a self-host path (NIM) that keeps workflows byte-identical
between cloud and sovereign deployments. Nemotron Nano GGUFs also run
fully local via `ollama/…`.

---

### `moonshot`

```yaml
infer:
  model: moonshot/kimi-k3
  prompt: "..."
```

The **Moonshot AI API** (`api.moonshot.ai/v1`) · the Kimi family (Kimi K3 ·
frontier · 1M-token context · a THINKING model · plus `kimi-k2.7-code` ·
`kimi-k2.7-code-highspeed` · `kimi-k2.6`). OpenAI-compatible chat
completions. The weights open on the same J+10 window (frontier open-weight ·
a self-host path is first-class · see the sovereignty note below). Added
2026-07-17 (ADR-105 · the frontier open-weight access category · the
openrouter and ADR-104 precedent applied).

**Models** · `moonshot/kimi-k3` (default · thinking · frontier) ·
`moonshot/kimi-k2.6` (the cheaper seat) · `moonshot/kimi-k2.7-code` /
`moonshot/kimi-k2.7-code-highspeed` (coding) · the model name passes through
verbatim.

**Auth** · `MOONSHOT_API_KEY` env var (`sk-…`).

**Features** · OpenAI-compatible chat completions · streaming · tool use ·
vision (base64 image parts · never a public URL) · structured output.

**Caveats · thinking model** ·
- **Reasoning spends tokens** · `kimi-k3` reasons before it answers and the
  reasoning draws from the SAME `max_tokens` budget · a tight `max_tokens: 64`
  can return an EMPTY completion (the whole budget went to thinking). Give it
  room · 512 is the practical floor for a real answer.
- **Temperature is server-fixed at 1.0** for `kimi-k3` · a workflow
  `temperature:` is accepted but the k3 seat overrides it server-side (the
  engine does not fight the server) · the non-thinking Kimi seats honor it.

**When to prefer it** · a frontier open-weight reasoning model with a long
context and a same-window self-host path. For a fully sovereign run, serve the
open weights yourself (vLLM · MXFP4) and reach them via `vllm/…` or the
`openai` escape hatch · the API-hosted seat is the convenience rung, not the
sovereign one (the hosted endpoint is a third-country surface · see the
sovereignty posture in ADR-105).

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

A workflow can switch providers with one line change (`model: ollama/qwen3.5:9b` → `model: mistral/mistral-large`, or any other `<provider>/<name>`) for most use cases.

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

🦋 *<!-- canon:providers -->17<!-- /canon --> providers · 1 contract · sovereignty preserved.*
