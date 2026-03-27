# Research Report: LLM Inference Servers & Routing Proxies (2025-2026)

## Summary

The LLM inference landscape in 2025-2026 has consolidated around four major open-source serving engines (vLLM, SGLang, TGI, Ollama) with distinct strengths, complemented by routing/gateway layers (LiteLLM, Portkey, OpenRouter, Martian) that abstract provider complexity. SGLang has emerged as a throughput leader on single-GPU setups (+29% over vLLM on Llama 8B), while vLLM dominates production deployments with its mature ecosystem and Kubernetes-native production stack. The routing proxy space has matured significantly, with LiteLLM providing the most practical open-source option and Portkey/OpenRouter leading the managed gateway category.

---

## 1. vLLM (v0.8 through v0.16)

**Status**: Industry standard for production LLM serving. Stripe reported 73% inference cost reduction via vLLM migration (50M daily API calls on 1/3 GPU fleet, December 2025).

### Key Features (2025-2026)

| Feature | Status | Notes |
|---------|--------|-------|
| Multi-model serving | Stable | OpenAI-compatible API, prefix-aware load balancing |
| Disaggregated prefill | V1 (maturing) | `--enable-chunked-prefill`, Q3 2025 matured wide-E parallelism |
| Speculative decoding | Stable | ngram-based in V1, Eagle/MTP for Qwen3.5 in v0.16 |
| FP8 quantization | Stable | SM100/SM120 kernels, MXFP8; FP8 KV cache pending |
| Structured output | V1 | xgrammar backend (no_fallback mode) |
| Prefix caching | Default on | Automatic, enabled by default |
| Data parallel | Stable | v0.16+ `--performance-mode throughput` or `interactivity` |

### Practical Configuration

**Single model, FP8 + prefix caching + chunked prefill:**
```bash
vllm serve meta-llama/Llama-3.3-70B-Instruct \
  --quantization fp8 \
  --enable-prefix-caching \
  --enable-chunked-prefill \
  --tensor-parallel-size 2 \
  --gpu-memory-utilization 0.85 \
  --max-model-len 4096 \
  --safetensors-only
```

**Docker multi-GPU production deployment:**
```bash
docker run --runtime nvidia --gpus all \
  --name vllm_llama70b \
  -v ~/.cache/huggingface:/root/.cache/huggingface \
  -p 8001:8000 --ipc=host \
  --env "HUGGING_FACE_HUB_TOKEN=$HF_TOKEN" \
  vllm/vllm-openai:latest \
  --model meta-llama/Llama-3.3-70B-Instruct \
  --tensor-parallel-size 2 \
  --safetensors-only \
  --max-model-len 4096 \
  --gpu-memory-utilization 0.85 \
  --enable-chunked-prefill
```

**Structured output (API-side):**
```python
response = client.chat.completions.create(
    model="meta-llama/Llama-3.1-8B-Instruct",
    messages=[{"role": "user", "content": "Extract data..."}],
    response_format={
        "type": "json_schema",
        "json_schema": {
            "name": "extraction",
            "schema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "price": {"type": "number"}
                },
                "required": ["name", "price"]
            }
        }
    }
)
```

**v0.16+ performance mode:**
```bash
vllm serve model_name --performance-mode throughput   # batch optimized
vllm serve model_name --performance-mode interactivity  # low latency
```

### Production Stack (Kubernetes)

vLLM provides a Kubernetes-native production stack with Helm:
- Scaling from single to distributed instances without code changes
- Web dashboard monitoring
- Request routing and KV cache offloading
- Multi-cloud (AWS, GCP, OCI)
- Semantic router for prefix-aware load balancing

**Performance guidance:**
- Batch size tuning: target 50-100ms TTFT SLAs
- Speculative decoding: 2-3x acceleration for predictable outputs
- Cross-instance KV cache sharing: 3-10x latency reduction for repetitive workloads

Sources: [vLLM docs](https://docs.vllm.ai/en/stable/), [vLLM releases](https://github.com/vllm-project/vllm/releases), [Introl production guide](https://introl.com/blog/vllm-production-deployment-inference-serving-architecture)

---

## 2. SGLang

**Status**: High-performance serving framework, joined PyTorch ecosystem (March 2025). Strongest at structured generation and multi-turn workloads.

### Core Innovation: RadixAttention

RadixAttention maintains KV cache in a **radix tree** (compact prefix tree), enabling automatic reuse across multiple LLM calls. Unlike vLLM's PagedAttention which focuses on memory efficiency, RadixAttention optimizes for cross-request cache sharing with LRU eviction. It is **enabled by default**.

### Features

| Feature | Status | Notes |
|---------|--------|-------|
| RadixAttention | Default on | `--disable-radix-cache` to turn off |
| Multi-GPU (TP/DP) | Stable | `--tensor-parallel-size N --data-parallel-size M` |
| Structured output | Stable | xgrammar backend via `--grammar-backend xgrammar` |
| Speculative decoding | In progress | Q3 2025 roadmap |
| LoRA + RadixAttention | In progress | Compatibility work ongoing |
| Hybrid models | Supported | Mamba/SSM + attention (PyTorch blog, Dec 2025) |

### Practical Configuration

**Basic server launch:**
```bash
python3 -m sglang.launch_server \
  --model-path meta-llama/Llama-3.1-8B-Instruct \
  --host 0.0.0.0 \
  --port 30000 \
  --log-level warning
```

**Multi-GPU with structured output:**
```bash
python3 -m sglang.launch_server \
  --model-path deepseek-ai/DeepSeek-R1-Distill-Qwen-32B \
  --tensor-parallel-size 4 \
  --mem-fraction-static 0.7 \
  --grammar-backend xgrammar \
  --port 30000 \
  --trust-remote-code \
  --host 0.0.0.0
```

**Docker (AMD MI300X):**
```bash
docker run --gpus all --shm-size 32g -p 3000:3000 \
  -v ~/.cache/huggingface:/root/.cache/huggingface \
  lmsysorg/sglang:v0.4.5.post3-rocm630 \
  python3 -m sglang.launch_server \
  --model deepseek-ai/DeepSeek-R1-Distill-Qwen-32B \
  --port 3000 --trust-remote-code
```

**Key tuning parameters:**
- `--mem-fraction-static 0.7` -- KV cache memory allocation (70%)
- `--disable-radix-cache` -- Disable RadixAttention when memory constrained
- `--tensor-parallel-size N` -- Multi-GPU tensor parallelism
- `--data-parallel-size M` -- Data parallelism
- `--grammar-backend xgrammar` -- Structured output

### SGLang vs vLLM: When to Use Which

| Scenario | Winner | Why |
|----------|--------|-----|
| Multi-turn / chatbot | SGLang | RadixAttention reuses conversation cache |
| Structured generation (JSON) | SGLang | Native compressed FSM, 116% faster output throughput |
| High-concurrency batch | vLLM | PagedAttention more memory-efficient at scale |
| Production K8s deployment | vLLM | Mature production stack, Helm charts |
| Long-context (200k+) | TGI | 13x faster than vLLM on 200k+ tokens |
| LoRA multi-tenant | vLLM/LoRAX | SGLang LoRA+RadixAttention compatibility WIP |

Sources: [SGLang GitHub](https://github.com/sgl-project/sglang), [LMSYS blog](https://lmsys.org/blog/2024-01-17-sglang/), [PyTorch blog](https://pytorch.org/blog/sglang-joins-pytorch/)

---

## 3. TGI v3 (Text Generation Inference)

**Status**: HuggingFace's production inference server. Powers HuggingChat and HF Inference API. Strongest at zero-config deployment and long-context workloads.

### Key Numbers

| Metric | TGI v3 | Comparison |
|--------|--------|------------|
| Long prompt speed (200k+) | ~2s response | 13x faster than vLLM (27.5s) |
| Token capacity | 30k tokens | 3x more than v2 (on 24GB L4 GPU) |
| KV cache lookup | ~5 microseconds | Optimized data structures |
| Configuration | Zero-config | Auto-detects hardware + model |

### Features

- **Continuous batching**: Dynamic request grouping for GPU utilization
- **Flash Attention v2**: Enabled by default
- **Tensor parallelism**: `--num-shard N` for multi-GPU
- **Quantization**: GPTQ, AWQ, GGML supported
- **KV cache optimization**: Conversation history caching with 5us lookup
- **Auto-tuning**: Analyzes hardware + model, sets optimal MAX_BATCH_TOTAL_TOKENS

### Docker Launch

**Minimal (zero-config):**
```bash
docker run --gpus all -p 8080:80 \
  ghcr.io/huggingface/text-generation-inference:3.0 \
  --model-id meta-llama/Llama-3.1-8B
```

**Production with quantization and sharding:**
```bash
docker run --gpus all -p 8080:80 --shm-size 1g \
  -e MAX_CONCURRENT_REQUESTS=128 \
  -e MAX_BATCH_TOTAL_TOKENS=16384 \
  ghcr.io/huggingface/text-generation-inference:3.0 \
  --model-id meta-llama/Llama-3.1-70B-Instruct \
  --num-shard 2 \
  --quantize gptq
```

### TGI v3 Limitations

- No public documentation for TensorRT-LLM backend integration (mentioned in roadmaps but not confirmed in v3)
- Structured output grammar support not documented for v3
- Speculative decoding not confirmed in v3 release notes
- FP8 quantization: not documented (GPTQ/AWQ only)
- Primarily optimized for single-model serving

Sources: [TGI GitHub](https://github.com/huggingface/text-generation-inference), [TGI v3 announcement](https://oneboard.framer.website/blog/hugging-face-text-generation-inference-(tgi)-v3-0-released-13x-faster-than-vllm-on-long-prompts)

---

## 4. Ollama

**Status**: Developer-friendly local inference. Now uses ggml directly (no longer wraps llama.cpp as of 2025). Supports 100+ models including vision.

### Key Features (2025-2026)

| Feature | Status | Notes |
|---------|--------|-------|
| Multi-model | Stable | Multiple models via REST API |
| GPU sharing | Stable | `OLLAMA_SCHED_SPREAD=1` for even distribution |
| Multi-GPU | Stable | Up to 15 GPUs + 1 CPU (16 devices max) |
| Structured output / JSON | Stable | Logit shaping + GBNF from ggml backend |
| Vision/multimodal | Stable | New engine (May 2025) |
| Go library | Stable | `github.com/ollama/ollama/api` |
| OpenAI-compatible API | Stable | `/v1/chat/completions` endpoint |

### Multi-GPU Configuration

**Spread model across GPUs:**
```bash
OLLAMA_SCHED_SPREAD=1 ollama serve
```

**Parallel instances on separate GPUs:**
```bash
# Instance 1 on GPU 0
CUDA_VISIBLE_DEVICES=0 OLLAMA_HOST=0.0.0.0:11434 ollama serve

# Instance 2 on GPU 1
CUDA_VISIBLE_DEVICES=1 OLLAMA_HOST=0.0.0.0:11435 ollama serve
```

### Go Library Usage

```go
package main

import (
    "context"
    "fmt"
    "github.com/ollama/ollama/api"
)

func main() {
    client, _ := api.ClientFromEnvironment()
    req := &api.GenerateRequest{
        Model:     "llama3.2",
        Prompt:    "Explain quantum computing",
        KeepAlive: &api.Duration{Duration: 5 * time.Minute},
    }
    // Stream or non-stream generation
    client.Generate(context.Background(), req, func(resp api.GenerateResponse) error {
        fmt.Print(resp.Response)
        return nil
    })
}
```

### JSON Structured Output

```bash
curl http://localhost:11434/api/generate -d '{
  "model": "llama3.2",
  "prompt": "Extract: name, price from this text...",
  "format": "json",
  "options": { "temperature": 0 }
}'
```

### Ollama Limitations for Production

- Sequential request queue by default (parallel instances needed for concurrency)
- No continuous batching (vs vLLM/SGLang/TGI)
- No speculative decoding
- No FP8 quantization (uses GGML quantization: Q4_0, Q4_K_M, Q8_0, etc.)
- No disaggregated prefill
- Best for development, prototyping, and small-scale local deployment

Sources: [Ollama GitHub](https://github.com/ollama/ollama), [Ollama blog](https://ollama.com/blog/multimodal-models)

---

## 5. LiteLLM Proxy

**Status**: The most popular open-source LLM gateway/proxy. Unified API for 100+ providers. Production-ready with PostgreSQL + Redis.

### Architecture

LiteLLM acts as a drop-in OpenAI-compatible proxy that routes to any backend (vLLM, Ollama, Anthropic, OpenAI, Bedrock, Vertex, etc.) via a single `config.yaml`.

### Complete Production Config (3x vLLM + cloud fallback)

```yaml
model_list:
  # 3 vLLM instances for load balancing
  - model_name: llama-70b
    litellm_params:
      model: openai/llama3.1-405b
      api_base: http://vllm-pod-1:8000/v1
      api_key: "EMPTY"
    rate_limits:
      rpm: 100

  - model_name: llama-70b
    litellm_params:
      model: openai/llama3.1-405b
      api_base: http://vllm-pod-2:8000/v1
      api_key: "EMPTY"

  - model_name: llama-70b
    litellm_params:
      model: openai/llama3.1-405b
      api_base: http://vllm-pod-3:8000/v1
      api_key: "EMPTY"

  # Cloud fallback
  - model_name: claude-sonnet
    litellm_params:
      model: bedrock/anthropic.claude-3-5-sonnet-20240620-v1:0
      aws_region_name: us-east-1

  - model_name: gpt-4o
    litellm_params:
      model: gpt-4o
      api_key: os.environ/OPENAI_API_KEY

router_settings:
  routing_strategy: least-busy      # Options: least-busy, simple-shuffle, latency-based-routing
  fallbacks:
    - llama-70b: ["claude-sonnet", "gpt-4o"]   # vLLM down -> Anthropic -> OpenAI
    - gpt-4o: ["claude-sonnet", "llama-70b"]
  redis_host: redis
  redis_port: 6379

litellm_settings:
  database_url: postgresql://litellm:pass@db:5432/litellm
  caching: true
  redis_host: redis
  redis_port: 6379
  request_timeout: 600
  global_rate_limit: 500
  global_rate_limit_freq: 60

general_settings:
  master_key: sk-2025-proxy-admin    # Must start with sk-
  alerting: ["slack"]
```

**Deploy:**
```bash
litellm --config config.yaml --port 4000
# Or Docker:
docker run -p 4000:4000 -v ./config.yaml:/app/config.yaml \
  ghcr.io/berriai/litellm:main-latest \
  --config /app/config.yaml
```

### Routing Strategies

| Strategy | Behavior |
|----------|----------|
| `least-busy` | Routes to instance with fewest in-flight requests |
| `simple-shuffle` | Random distribution |
| `latency-based-routing` | Routes to fastest responding instance |
| `fallback-loop` | Tries fallbacks in order, loops if all fail |

### Key Features

- **Per-key rate limiting**: RPM limits per API key via PostgreSQL
- **Semantic caching**: Redis-based, deduplicates similar prompts
- **Team/org budgets**: Spending limits per team with tracking
- **Usage analytics**: Real-time tracking per API key/organization
- **Swagger UI**: Full API docs at `<proxy-url>/#/config.yaml`

Sources: [LiteLLM docs](https://docs.litellm.ai/docs/simple_proxy), [LiteLLM config](https://docs.litellm.ai/docs/proxy/configs)

---

## 6. Portkey.ai

**Status**: Enterprise AI gateway. 1,600+ LLMs. SOC 2 compliant. Managed + self-hosted options.

### Core Concepts

Portkey uses **Gateway Configs** -- JSON objects that define routing rules applied per-request via SDK or HTTP header.

### Config Examples

**Fallback chain (OpenAI -> Anthropic):**
```json
{
  "strategy": {
    "mode": "fallback",
    "on_status_codes": [429, 500, 502, 503]
  },
  "targets": [
    { "provider": "openai", "api_key": "sk-..." },
    { "provider": "anthropic", "api_key": "sk-ant-..." }
  ]
}
```

**Weighted load balancing:**
```json
{
  "strategy": { "mode": "loadbalance" },
  "targets": [
    { "provider": "openai", "api_key": "sk-1", "weight": 0.6 },
    { "provider": "anthropic", "api_key": "sk-ant-1", "weight": 0.4 }
  ]
}
```

**Conditional routing (by metadata):**
```json
{
  "strategy": {
    "mode": "conditional",
    "conditions": [
      {
        "query": { "metadata.user_plan": { "$eq": "paid" } },
        "then": "premium_target"
      }
    ],
    "default": "budget_target"
  },
  "targets": [
    { "id": "premium_target", "provider": "openai", "api_key": "sk-..." },
    { "id": "budget_target", "provider": "groq", "api_key": "gsk_..." }
  ]
}
```

**Caching:**
```json
{
  "cache": {
    "enabled": true,
    "ttl": 3600,
    "namespace": "chat-responses"
  }
}
```

### SDK Usage

```javascript
import { Portkey } from 'portkey-ai';

const portkey = new Portkey({
  apiKey: 'pk-your-portkey-key',
  virtualKey: 'vk-openai-proxy',    // Secure key proxy
  config: 'pc-routing-config-id'     // Or inline JSON
});

const response = await portkey.chat.completions.create({
  model: 'gpt-4',
  messages: [{ role: 'user', content: 'Hello' }]
});
```

### Portkey Key Differentiators (vs LiteLLM)

| Feature | Portkey | LiteLLM |
|---------|---------|---------|
| Virtual keys (key vault) | Yes, centralized | No native vault |
| Conditional routing | JSON path queries | Fallback chains only |
| Guardrails | Yes (Prisma AIRS integration) | No native |
| Observability | Built-in OpenTelemetry | Basic logging |
| Self-hosted | Yes (open-source gateway) | Yes |
| Pricing | Free tier + enterprise | Open source |

Sources: [Portkey docs](https://docs.portkey.ai/docs/product/ai-gateway/configs), [Portkey gateway](https://github.com/Portkey-ai/gateway)

---

## 7. OpenRouter

**Status**: Multi-provider API gateway processing 8.4T tokens/month across 2.5M users (2025).

### Routing Algorithm

OpenRouter's default provider selection follows this priority:

1. **Exclude providers with outages** in the last 30 seconds
2. **Select lowest-cost provider**, weighted by inverse square of price (strongly favoring cheaper)
3. **Fallback** to remaining providers if primary fails

### Customization via API

```python
response = client.chat.completions.create(
    model="meta-llama/llama-3.1-70b-instruct",
    messages=[...],
    extra_body={
        "provider": {
            # Sort by price, ignoring throughput partitions
            "sort": { "by": "price", "partition": "none" },
            # Only route to providers with >= 50 tok/s at p90
            "preferredMinThroughput": { "p90": 50 },
            # Or use model variants:
            # ":nitro" = fastest, ":floor" = cheapest, ":online" = RAG
        }
    }
)
```

### Key Features

| Feature | Details |
|---------|---------|
| Overhead | ~25ms routing latency |
| Fallback | Automatic across providers for same model |
| Auto Exacto | Re-evaluates tool-calling providers every 5 min |
| Pricing | Pass-through (no markup beyond minimal overhead) |
| Normalization | Returns consistent OpenAI-style JSON regardless of provider |
| Data flywheel | 8.4T tokens/month feeds back into routing decisions |

Sources: [OpenRouter provider selection docs](https://openrouter.ai/docs/guides/routing/provider-selection), [Sacra research](https://sacra.com/research/openrouter/)

---

## 8. Martian (withmartian.com)

**Status**: Research-driven smart router using mechanistic interpretability. Self-described "first LLM router."

### How It Works

Martian routes per-query (not static) using mechanistic interpretability to understand model internals:

1. **Judge models** evaluate specialist models across accuracy, alignment, verifiability, reliability
2. **Router** matches queries to specialists based on evaluations
3. **Expert orchestration** decomposes complex tasks across multiple models

### Reported Results

| Deployment | Result |
|------------|--------|
| Help chat | 52.4% error rate reduction, 92% cost drop |
| RAG (vs GPT-4) | +20% quality, 80x cost reduction |
| User-facing system | 79.2% user preference, 300x cheaper than GPT-4, 8.7x faster |
| Code generation | Outperforms any single model (Code Router product) |

### Approach Differentiator

Unlike OpenRouter (price/latency heuristics) or LiteLLM (rule-based routing), Martian uses **model mapping** -- understanding what makes models succeed or fail at the mechanistic level to make precise routing decisions. This is more "intelligence-aware" routing than load balancing.

Sources: [withmartian.com](https://withmartian.com), [Martian Code Router](https://withmartian.com/code), [Martian blog](https://withmartian.com/blog)

---

## 9. LoRAX (Predibase)

**Status**: Open-source multi-LoRA inference server. Serves thousands of fine-tuned adapters on a single GPU.

### Architecture

LoRAX keeps **one base model loaded** and dynamically swaps LoRA adapters per-request:

1. **Dynamic adapter loading**: LoRA weights loaded just-in-time, no base model reload
2. **Tiered weight caching**: GPU -> CPU -> disk offloading for inactive adapters
3. **SGMV batching**: Sparse Grouped Matrix Multiplication batches multiple LoRA requests in parallel

### Performance

- 100+ adapters on a single A100 GPU efficiently
- ICML 2025 method: 80% of single-LoRA throughput for 1,000+ adapters via shared-basis compression
- Near-zero adapter swap overhead after initial base model load

### LoRAX vs vLLM LoRA Support

| Aspect | LoRAX | vLLM |
|--------|-------|------|
| Design goal | Multi-tenant (thousands of adapters) | General inference with LoRA support |
| Adapter handling | JIT loading, tiered offload, multi-batching | PagedAttention, less optimized for thousands |
| Scaling | 100+ on A100, 1000+ with compression | Viable for fewer adapters |
| Use case | Per-customer fine-tunes, enterprise | General inference with occasional LoRA |

Sources: [LoRAX GitHub](https://github.com/predibase/lorax), [ICML 2025 poster](https://icml.cc/virtual/2025/poster/46530)

---

## 10. Benchmark Comparisons (2025-2026)

### SGLang vs vLLM -- Llama 3.1 8B on H100

| Metric | SGLang | vLLM | Delta |
|--------|--------|------|-------|
| Total throughput | 16,215 tok/s | 12,553 tok/s | **SGLang +29%** |
| Output token throughput | 893.82 tok/s | 412.99 tok/s | **SGLang +116%** |
| Time to first token (TTFT) | 79.42 ms | 102.65 ms | **SGLang faster** |
| Inter-token latency | 6.03 ms | 7.14 ms | **SGLang faster** |
| High-concurrency stability | 30-31 tok/s | Drops 22 -> 16 tok/s | **SGLang more stable** |

Source: [localaimaster.com comparison](https://localaimaster.com/blog/sglang-vs-vllm-comparison)

### General Throughput Ranges on H100 (2026)

| Engine | Throughput Range |
|--------|-----------------|
| vLLM v0.7+ | 1,000 - 2,000 tok/s |
| TGI v3 | 800 - 1,500 tok/s |
| TensorRT-LLM 0.17 | 35-50% higher than vLLM on same NVIDIA hardware |
| SGLang v0.4 | Highest for structured/multi-turn workloads |

### vLLM vs TGI -- Long Context (200k+ tokens)

| Engine | Response Time | Notes |
|--------|--------------|-------|
| TGI v3 | ~2 seconds | KV cache with 5us lookup |
| vLLM | ~27.5 seconds | 13x slower on long prompts |

### vLLM vs TGI -- High Concurrency

A 2025 arXiv study found **up to 24x higher throughput for vLLM under high concurrency**, though TGI showed competitive latency at low concurrency.

### Cost Efficiency (50M tokens/day)

| Engine | Throughput | GPU-hours/day |
|--------|-----------|---------------|
| vLLM | ~3,400 tok/s | 4.08 |
| TGI | ~2,900 tok/s | 4.79 |

### Decision Matrix

| Priority | Best Choice | Runner-up |
|----------|------------|-----------|
| Raw throughput (batch) | vLLM | TensorRT-LLM |
| Multi-turn / structured gen | SGLang | vLLM (V1) |
| Long context (200k+) | TGI v3 | vLLM + chunked prefill |
| Zero-config deployment | TGI v3 | Ollama |
| Local dev / prototyping | Ollama | SGLang |
| Multi-LoRA serving | LoRAX | vLLM |
| NVIDIA optimization | TensorRT-LLM | vLLM |
| Production K8s | vLLM (production stack) | TGI |
| Multi-provider routing | LiteLLM | Portkey |
| Smart model selection | Martian | OpenRouter |

---

## Routing Layer Comparison

| Feature | LiteLLM | Portkey | OpenRouter | Martian |
|---------|---------|---------|------------|---------|
| Open source | Yes | Gateway only | No | Partial |
| Self-hosted | Yes | Yes | No | No |
| Providers | 100+ | 1,600+ | Many | Many |
| Routing logic | Rule-based (YAML) | JSON configs | Price/latency heuristic | Mechanistic interpretability |
| Fallbacks | YAML fallback chains | Status-code based | Automatic | Per-query |
| Load balancing | least-busy, latency-based | Weighted targets | Inverse-square pricing | N/A (smart routing) |
| Caching | Redis semantic | TTL + namespace | N/A | N/A |
| Rate limiting | Per-key/team (PostgreSQL) | Per virtual key | Per API key | N/A |
| Cost tracking | Yes (DB) | Yes (dashboard) | Yes (dashboard) | N/A |
| Best for | Self-hosted proxy | Enterprise governance | Developer convenience | Quality optimization |

---

## Sources

1. [vLLM docs](https://docs.vllm.ai/en/stable/) -- Features, configuration, V1 guide
2. [vLLM production stack](https://docs.vllm.ai/projects/production-stack) -- K8s deployment
3. [SGLang GitHub](https://github.com/sgl-project/sglang) -- Server launch, features
4. [SGLang blog (LMSYS)](https://lmsys.org/blog/2024-01-17-sglang/) -- RadixAttention details
5. [TGI v3 announcement](https://oneboard.framer.website/blog/hugging-face-text-generation-inference-(tgi)-v3-0-released-13x-faster-than-vllm-on-long-prompts) -- Performance numbers
6. [TGI GitHub](https://github.com/huggingface/text-generation-inference) -- Docker config
7. [Ollama GitHub](https://github.com/ollama/ollama) -- Multi-GPU, API
8. [LiteLLM docs](https://docs.litellm.ai/docs/proxy/configs) -- YAML config reference
9. [Portkey docs](https://docs.portkey.ai/docs/product/ai-gateway/configs) -- Gateway configs
10. [OpenRouter provider selection](https://openrouter.ai/docs/guides/routing/provider-selection) -- Routing algorithm
11. [Martian blog](https://withmartian.com/blog) -- Smart routing approach
12. [LoRAX GitHub](https://github.com/predibase/lorax) -- Multi-adapter serving
13. [Yotta Labs comparison](https://www.yottalabs.ai/post/best-llm-inference-engines-in-2026-vllm-tensorrt-llm-tgi-and-sglang-compared) -- Engine comparison
14. [localaimaster SGLang vs vLLM](https://localaimaster.com/blog/sglang-vs-vllm-comparison) -- Benchmark numbers
15. [Spheron benchmarks](https://www.spheron.network/blog/vllm-vs-tensorrt-llm-vs-sglang-benchmarks) -- H100 benchmarks
16. [Introl vLLM production](https://introl.com/blog/vllm-production-deployment-inference-serving-architecture) -- Stripe case study

## Methodology

- Tools used: Perplexity AI search (14 queries across 2 rounds)
- Sources analyzed: 80+ web pages, GitHub repos, documentation sites, blog posts
- Time period covered: 2024 Q4 through 2026 Q1
- Cross-referenced claims across multiple independent sources

## Confidence Level

**Medium-High** -- Performance numbers come from third-party blogs and comparisons rather than peer-reviewed papers. The Llama 8B SGLang vs vLLM benchmark from localaimaster.com is the most detailed head-to-head available. TGI v3's "13x faster" claim is from HuggingFace's own announcement. Production deployment patterns (vLLM, LiteLLM configs) are well-documented in official docs. Martian's claimed improvements are self-reported.

## Further Research Suggestions

- Run custom benchmarks on target hardware with GuideLLM or MLPerf
- Evaluate TensorRT-LLM (35-50% faster than vLLM on NVIDIA but rigid compilation)
- Test vLLM V1 disaggregated prefill once stable
- Compare SGLang SGL Router with vLLM production stack for cluster-wide deployment
- Evaluate Martian's Code Router for multi-model code generation quality
- Benchmark FP8 vs FP16 on specific models for quality/speed tradeoff
