# GPU Cloud Landscape for AI Inference (2025-2026)

Comprehensive compatibility matrix for self-hosted and cloud-based AI inference.

**Last updated**: 2026-03-27
**Sources**: Scaleway, Lambda Labs, RunPod, Vast.ai, Together.ai, Groq, NVIDIA official specs, technical.city

---

## 1. NVIDIA Data Center GPU Specifications

| GPU | Architecture | VRAM | Memory Type | Bandwidth | FP16/BF16 TFLOPS | FP32 TFLOPS | TDP | NVLink | Year |
|-----|-------------|------|-------------|-----------|-------------------|-------------|-----|--------|------|
| **L4** | Ada Lovelace | 24 GB | GDDR6 | 300 GB/s | 242 (w/ sparsity) | 30.3 | 72W | No | 2023 |
| **L40** | Ada Lovelace | 48 GB | GDDR6 | 864 GB/s | 362 (w/ sparsity) | 90.5 | 300W | No | 2023 |
| **L40S** | Ada Lovelace | 48 GB | GDDR6 | 864 GB/s | 362 (w/ sparsity) | 91.6 | 350W | No | 2023 |
| **A100 PCIe** | Ampere | 80 GB | HBM2e | 2,039 GB/s | 312 | 19.5 | 300W | No | 2020 |
| **A100 SXM** | Ampere | 80 GB | HBM2e | 2,039 GB/s | 312 | 19.5 | 400W | 600 GB/s | 2020 |
| **H100 PCIe** | Hopper | 80 GB | HBM3 | 2,039 GB/s | 1,513 (w/ sparsity) | 51.2 | 350W | No | 2023 |
| **H100 SXM** | Hopper | 80 GB | HBM3 | 3,350 GB/s | 1,979 (w/ sparsity) | 66.9 | 700W | 900 GB/s | 2023 |
| **H100 NVL** | Hopper | 94 GB | HBM3 | 3,938 GB/s | 1,979 (w/ sparsity) | 66.9 | 400W | 600 GB/s | 2023 |
| **H200 SXM** | Hopper | 141 GB | HBM3e | 4,800 GB/s | 1,979 (w/ sparsity) | 66.9 | 700W | 900 GB/s | 2024 |
| **B200** | Blackwell | 192 GB | HBM3e | 8,000 GB/s | 4,500 (w/ sparsity) | 90 | 1,000W | 1,800 GB/s | 2025 |
| **B300** | Blackwell | 288 GB | HBM3e | 12,000 GB/s | ~9,000 (w/ sparsity) | ~125 | 1,400W | 1,800 GB/s | 2025 |

**Key differences**:
- **L4 vs L40/L40S**: L4 is low-power (72W) inference-optimized; L40/L40S are workstation-class with 2x VRAM and 3x bandwidth
- **L40 vs L40S**: L40S has slightly higher TDP (350W vs 300W) and better FP8 Transformer Engine support
- **A100 vs H100**: H100 has ~3-4x FP16 TFLOPS via Transformer Engine, 1.6x memory bandwidth (SXM)
- **H100 vs H200**: Same compute, but H200 has 76% more VRAM (141 vs 80 GB) and 43% more bandwidth (4,800 vs 3,350 GB/s)
- **H200 vs B200**: B200 doubles compute TFLOPS, 36% more VRAM, 67% more bandwidth

---

## 2. Consumer GPU Specifications (Local Inference)

| GPU | VRAM | Memory Type | Bandwidth | FP16 TFLOPS | FP32 TFLOPS | TDP | MSRP | Year |
|-----|------|-------------|-----------|-------------|-------------|-----|------|------|
| **RTX 3090** | 24 GB | GDDR6X | 936 GB/s | 71 | 35.6 | 350W | $1,499 | 2020 |
| **RTX 4090** | 24 GB | GDDR6X | 1,008 GB/s | 165 (w/ sparsity) | 82.6 | 450W | $1,599 | 2022 |
| **RTX 5090** | 32 GB | GDDR7 | 1,792 GB/s | 210 (w/ sparsity) | 104.8 | 575W | $1,999 | 2025 |

### Apple Silicon (Unified Memory)

| Chip | Max Unified Memory | Memory Bandwidth | GPU TFLOPS (FP16) | TDP (whole SoC) | Year |
|------|--------------------|--------------------|---------------------|-----------------|------|
| **M1** | 16 GB | 68 GB/s | 2.6 | 20-39W | 2020 |
| **M1 Max** | 64 GB | 400 GB/s | 10.4 | 60W | 2021 |
| **M1 Ultra** | 128 GB | 800 GB/s | 21.0 | 215W | 2022 |
| **M2** | 24 GB | 100 GB/s | 3.6 | 22W | 2022 |
| **M2 Max** | 96 GB | 400 GB/s | 13.6 | 75W | 2023 |
| **M2 Ultra** | 192 GB | 800 GB/s | 27.2 | 215W | 2023 |
| **M3** | 24 GB | 100 GB/s | 4.1 | 22W | 2023 |
| **M3 Max** | 128 GB | 400 GB/s | 14.2 | 80W | 2023 |
| **M3 Ultra** | 192 GB | 800 GB/s | 28.4 | 215W | 2024 |
| **M4** | 32 GB | 120 GB/s | 4.6 | 22W | 2024 |
| **M4 Pro** | 48 GB | 273 GB/s | 8.7 | 45W | 2024 |
| **M4 Max** | 128 GB | 546 GB/s | 17.4 | 90W | 2024 |
| **M4 Ultra** | 256 GB | 819 GB/s | 34.8 | 215W | 2025 |

**Apple Silicon advantages**: Unified memory means the entire pool is available as "VRAM". Extremely power-efficient (tokens/watt). M4 Ultra with 256 GB can load models that would need multiple GPUs on NVIDIA.

**Apple Silicon disadvantages**: Raw FP16 TFLOPS are 5-20x lower than discrete GPUs. No CUDA ecosystem (must use MLX or llama.cpp Metal backend). Bandwidth is 2-10x lower than HBM.

---

## 3. Cloud Provider Pricing

### 3.1 Scaleway (European Cloud, GDPR-compliant)

**Regions**: Paris (PAR1, PAR2), Warsaw (WAW2)

| Instance | GPU | VRAM/GPU | FP16 TFLOPS | Price/hr | ~Price/month |
|----------|-----|----------|-------------|----------|-------------|
| GPU-L4-1 | 1x L4 | 24 GB | 242 | EUR 0.75 | EUR 548 |
| GPU-L4-2 | 2x L4 | 24 GB ea. | 484 | EUR 1.50 | EUR 1,095 |
| GPU-L4-4 | 4x L4 | 24 GB ea. | 968 | EUR 3.00 | EUR 2,190 |
| GPU-L4-8 | 8x L4 | 24 GB ea. | 1,936 | EUR 6.00 | EUR 4,380 |
| GPU-L40S-1 | 1x L40S | 48 GB | 362 | EUR 1.40 | EUR 1,022 |
| GPU-L40S-2 | 2x L40S | 48 GB ea. | 724 | EUR 2.80 | EUR 2,044 |
| GPU-L40S-4 | 4x L40S | 48 GB ea. | 1,448 | EUR 5.60 | EUR 4,088 |
| GPU-L40S-8 | 8x L40S | 48 GB ea. | 2,896 | EUR 11.20 | EUR 8,176 |
| GPU-H100-1 | 1x H100 SXM | 80 GB | 1,513 | EUR 2.52 | EUR 1,840 |
| GPU-H100-2 | 2x H100 SXM | 80 GB ea. | 3,026 | EUR 5.04 | EUR 3,679 |
| GPU-H100-PCIe | 1x H100 PCIe | 80 GB | 1,513 | EUR 2.73 | EUR 1,992 |
| GPU-B300-8 | 8x B300 | 288 GB ea. | -- | EUR 60.00 | EUR 43,800 |
| RENDER (P100) | 1x P100 | 16 GB | 19 | EUR 1.24 | EUR 905 |

**Notes**: Scaleway is EU-sovereign, all data stays in Europe. H200 and Blackwell B300 available (B300 in preview). Good for GDPR-sensitive deployments. Pricing is in EUR.

### 3.2 Lambda Labs

**Pricing effective April 6, 2026.**

| GPU | VRAM | 1-GPU/hr | 4-GPU/hr | 8-GPU/hr |
|-----|------|----------|----------|----------|
| **B200 SXM6** | 180 GB | $6.99 | $6.79/GPU | $6.69/GPU |
| **H100 SXM** | 80 GB | $4.29 | $4.09/GPU | $3.99/GPU |
| **H100 PCIe** | 80 GB | $3.29 | -- | -- |
| **GH200** | 96 GB | $2.29 | -- | -- |
| **A100 SXM 80GB** | 80 GB | -- | -- | $2.79/GPU |
| **A100 SXM 40GB** | 40 GB | $1.99 | -- | $1.99/GPU |
| **A100 PCIe 40GB** | 40 GB | $1.99 | $1.99/GPU | -- |
| **A6000** | 48 GB | $1.09 | $1.09/GPU | -- |
| **A10** | 24 GB | $1.29 | -- | -- |
| **Quadro RTX 6000** | 24 GB | $0.69 | -- | -- |
| **Tesla V100** | 16 GB | -- | -- | $0.79/GPU |

**Notes**: No egress fees. Per-minute billing. Lambda Stack pre-installed (PyTorch, CUDA). 1-Click Clusters available for 16-2000+ GPU scale. Availability can be limited -- often sold out for H100.

### 3.3 RunPod

**Cloud GPUs (On-Demand Pods)**:

| GPU | VRAM | Community Cloud | Secure Cloud |
|-----|------|----------------|--------------|
| **B200** | 180 GB | ~$5.50/hr | ~$8.64/s-rate |
| **H200 SXM** | 141 GB | ~$3.50/hr | ~$4.31/hr |
| **H100 NVL** | 94 GB | ~$2.50/hr | ~$3.50/hr |
| **H100 SXM** | 80 GB | ~$2.49/hr | ~$3.29/hr |
| **H100 PCIe** | 80 GB | ~$2.19/hr | ~$2.89/hr |
| **A100 SXM** | 80 GB | ~$1.64/hr | ~$1.79/hr |
| **A100 PCIe** | 80 GB | ~$1.44/hr | ~$1.79/hr |
| **L40S** | 48 GB | ~$0.89/hr | ~$1.14/hr |
| **RTX 6000 Ada** | 48 GB | ~$0.69/hr | ~$0.99/hr |
| **L40** | 48 GB | ~$0.69/hr | ~$0.89/hr |
| **A40** | 48 GB | ~$0.49/hr | ~$0.69/hr |
| **RTX 5090** | 32 GB | ~$0.54/hr | ~$0.74/hr |
| **RTX 4090** | 24 GB | ~$0.39/hr | ~$0.69/hr |
| **L4** | 24 GB | ~$0.29/hr | ~$0.44/hr |
| **RTX 3090** | 24 GB | ~$0.22/hr | ~$0.29/hr |

**Serverless GPU pricing (per second)**:

| Tier | VRAM | Flex rate/s | Active rate/s |
|------|------|-------------|---------------|
| B200 | 180 GB | $8.64/s | $6.84/s |
| H200 | 141 GB | $5.58/s | $4.46/s |
| H100 | 80 GB | $4.18/s | $3.35/s |
| A100 | 80 GB | $2.72/s | $2.17/s |
| L40/L40S/6000 Ada | 48 GB | $1.90/s | $1.33/s |
| RTX 5090 | 32 GB | $1.58/s | $1.11/s |
| RTX 4090 | 24 GB | $1.10/s | $0.77/s |
| L4/A5000/3090 | 24 GB | $0.69/s | $0.48/s |

**Notes**: Community Cloud = shared infrastructure (cheaper, less reliable). Secure Cloud = dedicated T3/T4 data centers with SOC 2 compliance. Serverless rates are per-second of active GPU time. Instant Clusters available up to 64 GPUs.

### 3.4 Vast.ai (GPU Marketplace)

Live marketplace pricing (median on-demand, as of March 2026):

| GPU | VRAM | Median $/hr | Range $/hr | Availability |
|-----|------|-------------|------------|-------------|
| **B200** | 192 GB | $3.13 | $2.31 - $9.38 | High (120+) |
| **H200 NVL** | 141 GB | $2.22 | $1.87 - $3.20 | Medium |
| **H200** | 141 GB | $2.06 | $1.94 - $3.74 | Medium |
| **H100 SXM** | 80 GB | $1.53 | $0.93 - $2.33 | Medium |
| **H100 NVL** | 94 GB | $1.52 | $1.20 - $2.93 | Low |
| **RTX PRO 6000 S** | 48 GB | $1.07 | $0.67 - $1.60 | Medium |
| **A100 SXM4** | 80 GB | $0.74 | $0.21 - $1.27 | Low |
| **A100 PCIe** | 80 GB | $0.64 | $0.09 - $1.20 | Low |
| **L40S** | 48 GB | $0.47 | $0.45 - $1.20 | Medium |
| **RTX 6000 Ada** | 48 GB | $0.47 | $0.27 - $0.80 | Low |
| **RTX A6000** | 48 GB | $0.37 | $0.16 - $0.64 | Medium |
| **RTX 5090** | 32 GB | $0.37 | $0.11 - $13.33 | High (120+) |
| **A40** | 48 GB | $0.29 | $0.29 - $0.60 | Low |
| **RTX 4090** | 24 GB | $0.29 | $0.11 - $6.67 | High (120+) |
| **RTX 3090** | 24 GB | $0.13 | $0.03 - $1.49 | High (120+) |
| **RTX 3090 Ti** | 24 GB | $0.13 | $0.07 - $0.33 | Low |

**Instance types**: On-Demand (guaranteed uptime), Interruptible (can be preempted, cheaper), Reserved (long-term commitment, best rates). Marketplace pricing fluctuates with supply and demand. Wide price ranges reflect different host locations, connectivity, and reliability.

---

## 4. Serverless Inference Providers

### 4.1 Together.ai

**Per 1M tokens (Serverless Inference, March 2026)**:

| Model | Input | Output |
|-------|-------|--------|
| Llama 4 Maverick | $0.27 | $0.85 |
| MiniMax M2.5 | $0.30 ($0.06 cached) | $1.20 |
| Kimi K2.5 | $0.50 | $2.80 |
| GLM-5 | $1.00 | $3.20 |
| Llama 3.3 70B | $0.88 | $0.88 |
| Llama 3 8B Instruct Lite | $0.10 | $0.10 |
| DeepSeek-R1-0528 | $3.00 | $7.00 |
| DeepSeek-V3.1 | $0.60 | $1.70 |
| gpt-oss-120B | $0.15 | $0.60 |
| Qwen3-Next-80B-A3B | $0.15 | $1.50 |
| Qwen3 235B (FP8) | $0.20 | $0.60 |
| Qwen2.5 7B Turbo | $0.30 | $0.30 |
| Mistral Small 3 | $0.10 | $0.30 |
| Gemma 3n E4B | $0.02 | $0.04 |

**Also offers**: Batch API (50% discount), Dedicated Model Inference, GPU Clusters (H100, H200, B200, GB200, GB300), Fine-tuning, Sandbox environments.

### 4.2 Groq (LPU Inference)

**Per 1M tokens (March 2026)**:

| Model | Speed (TPS) | Input | Output |
|-------|-------------|-------|--------|
| GPT-OSS 20B | 1,000 | $0.075 | $0.30 |
| GPT-OSS 120B | 500 | $0.15 | $0.60 |
| Kimi K2 1T | 200 | $1.00 | $3.00 |
| Llama 4 Scout 17Bx16E | 594 | $0.11 | $0.34 |
| Qwen3 32B | 662 | $0.29 | $0.59 |
| Llama 3.3 70B | 394 | $0.59 | $0.79 |
| Llama 3.1 8B | 840 | $0.05 | $0.08 |

**Prompt caching**: 50% discount on cached input tokens.
**Built-in tools**: Web search ($5-8/1K requests), code execution ($0.18/hr), browser automation ($0.08/hr).
**Key advantage**: Custom LPU hardware delivers 200-1,000 tokens/second -- significantly faster than GPU-based inference.

---

## 5. Model VRAM Requirements

Practical VRAM needed to load and run popular models at various quantizations:

| Model | Parameters | FP16 | Q8 (8-bit) | Q4 (4-bit) | GGUF Q4_K_M |
|-------|-----------|------|------------|------------|-------------|
| Llama 3.1 8B | 8B | 16 GB | 9 GB | 5 GB | ~5 GB |
| Mistral 7B | 7B | 14 GB | 8 GB | 4.5 GB | ~4.5 GB |
| Gemma 3 4B | 4B | 8 GB | 5 GB | 3 GB | ~3 GB |
| Qwen 2.5 14B | 14B | 28 GB | 16 GB | 9 GB | ~9 GB |
| Llama 3.3 70B | 70B | 140 GB | 75 GB | 40 GB | ~40 GB |
| Qwen 2.5 72B | 72B | 144 GB | 77 GB | 42 GB | ~42 GB |
| Mixtral 8x22B | 141B (39B active) | 282 GB | 150 GB | 80 GB | ~80 GB |
| Llama 4 Maverick | 400B MoE | ~200 GB | ~110 GB | ~60 GB | ~55 GB |
| DeepSeek-V3 | 671B MoE (37B active) | ~1.3 TB | ~700 GB | ~350 GB | ~350 GB |
| DeepSeek-R1 | 671B MoE | ~1.3 TB | ~700 GB | ~350 GB | ~350 GB |
| Qwen3 235B A22B | 235B MoE | ~470 GB | ~250 GB | ~130 GB | ~130 GB |

### What fits on what GPU?

| GPU / Device | VRAM | Can run (FP16) | Can run (Q4/GGUF) |
|-------------|------|----------------|---------------------|
| **RTX 3090** | 24 GB | 7-14B models | Up to 30B |
| **RTX 4090** | 24 GB | 7-14B models | Up to 30B |
| **RTX 5090** | 32 GB | Up to 14B | Up to 40B |
| **L4** | 24 GB | 7-14B models | Up to 30B |
| **L40S** | 48 GB | Up to 24B | Up to 70B (tight) |
| **A100 80GB** | 80 GB | Up to 40B | 70B comfortably, ~140B MoE |
| **H100 80GB** | 80 GB | Up to 40B | 70B comfortably, ~140B MoE |
| **H200 141GB** | 141 GB | 70B comfortably | ~240B MoE |
| **B200 192GB** | 192 GB | 70B+ comfortably | ~350B MoE |
| **B300 288GB** | 288 GB | 140B+ models | ~500B MoE |
| **Mac M4 Pro** | 48 GB unified | Up to 24B | Up to 70B (slow) |
| **Mac M4 Max** | 128 GB unified | Up to 60B (slow) | 70B, light MoE |
| **Mac M4 Ultra** | 256 GB unified | Up to 120B (slow) | 200B+ MoE (slow) |

**Important notes**:
- VRAM requirements include ~1-2 GB overhead for KV cache and runtime
- Longer context windows significantly increase VRAM usage (KV cache scales linearly with context length)
- MoE models only activate a fraction of parameters per token, but still need full VRAM to load all experts
- Multi-GPU setups can pool VRAM via tensor parallelism (e.g., 2x H100 = 160 GB effective)
- Apple Silicon bandwidth (100-819 GB/s) is 3-30x lower than HBM, resulting in proportionally slower token generation

---

## 6. Power Efficiency Comparison

Approximate tokens/watt for inference (Llama 3.1 8B Q4, single GPU, steady state):

| Hardware | TDP | ~Tokens/sec | ~Tokens/Watt |
|----------|-----|-------------|-------------|
| **Mac M4** | 22W | 25-30 | ~1.3 |
| **Mac M4 Pro** | 45W | 35-45 | ~0.9 |
| **Mac M4 Max** | 90W | 50-70 | ~0.7 |
| **L4** | 72W | 40-60 | ~0.7 |
| **RTX 4090** | 450W | 100-150 | ~0.3 |
| **RTX 5090** | 575W | 130-180 | ~0.3 |
| **L40S** | 350W | 80-120 | ~0.3 |
| **A100 SXM** | 400W | 80-120 | ~0.25 |
| **H100 SXM** | 700W | 200-350 | ~0.4 |
| **H200 SXM** | 700W | 250-400 | ~0.5 |
| **B200** | 1000W | 500-800 | ~0.6 |

**For always-on inference servers**:
- **Best tokens/watt**: M4 chips and L4 dominate for small models. Extremely power-efficient for 7-14B inference.
- **Best absolute throughput**: H100/H200/B200 for large models. The HBM bandwidth advantage is decisive for large KV caches.
- **Sweet spot for indie/startup**: L4 or RTX 4090 for small models (7-14B). L40S or H100 for 70B models.
- **Electricity cost**: At $0.15/kWh, an H100 at full load costs ~$0.105/hr in electricity alone. An L4 costs ~$0.011/hr.

---

## 7. Provider Comparison Matrix

### Cost per GPU-hour (H100 SXM 80GB)

| Provider | $/hr | Model | Notes |
|----------|------|-------|-------|
| **Vast.ai** | $1.53 (median) | Marketplace | Variable quality, $0.93 low end |
| **Scaleway** | $2.71 (EUR 2.52) | Fixed | EU sovereign, GDPR |
| **RunPod** | ~$2.49-3.29 | Community/Secure | Per-second billing |
| **Lambda Labs** | $3.99-4.29 | Fixed | No egress fees |

### Cost per GPU-hour (A100 80GB)

| Provider | $/hr | Notes |
|----------|------|-------|
| **Vast.ai** | $0.64-0.74 | Marketplace |
| **RunPod** | ~$1.44-1.79 | Community/Secure |
| **Lambda Labs** | $2.79 | 8-GPU only |

### Serverless: Cost per 1M output tokens (Llama 3.3 70B equivalent)

| Provider | $/1M output | Speed | Notes |
|----------|-------------|-------|-------|
| **Groq** | $0.79 | 394 TPS | LPU custom hardware |
| **Together.ai** | $0.88 | ~100-200 TPS | GPU-based |

---

## 8. Decision Framework

### Self-hosted inference (you manage the GPU)

| Use Case | Recommended | Why |
|----------|-------------|-----|
| Dev/prototyping (7B models) | RTX 4090 local or Vast.ai RTX 3090 | Cheapest, 24 GB sufficient |
| Production 7-14B | L4 on Scaleway or RunPod | Low power, good throughput, cheap |
| Production 70B | 1x H100 or 2x L40S | H100 for latency, L40S pair for cost |
| Production 70B (EU) | Scaleway H100 | GDPR sovereign, EUR 2.52/hr |
| Large MoE (Mixtral, Llama 4) | H200 or 2x H100 | Need 141+ GB VRAM |
| Budget development | Mac M4 Pro/Max local | Silent, efficient, good for iteration |
| Maximum efficiency | Mac M4 or L4 | Best tokens/watt for small models |

### Serverless inference (API-based, no GPU management)

| Use Case | Recommended | Why |
|----------|-------------|-----|
| Lowest latency | Groq | 200-1000 TPS on LPU hardware |
| Lowest cost (small models) | Together.ai | Gemma 3n at $0.02/$0.04 per 1M tokens |
| Lowest cost (large models) | Together.ai Batch API | 50% discount on async workloads |
| Open source models | Together.ai or Groq | Widest model selection |

### GPU marketplace (cheapest, least reliable)

| Use Case | Recommended | Why |
|----------|-------------|-----|
| Batch processing | Vast.ai interruptible | Cheapest GPUs available |
| Training runs | Vast.ai or RunPod Community | Can restart on interruption |
| Production (uptime matters) | Lambda Labs or Scaleway | Fixed pricing, guaranteed availability |

---

## 9. Scaleway-Specific Notes (EU Focus)

Scaleway is the **only major EU-sovereign GPU cloud** with:
- Data residency in France and Poland
- GDPR compliance by design
- HDS (Health Data Hosting) certification
- No US CLOUD Act exposure

**Available GPUs**: L4, L40S, H100 (PCIe + SXM), B300 (preview)
**Not yet available**: H200 (pre-register), consumer GPUs (RTX series)
**Mac Mini hosting**: M1, M2, M2 Pro, M4, M4 Pro available (useful for MLX inference)
**Generative APIs**: Managed inference endpoints for popular models (pay per token, EU-hosted)

---

## Sources

1. [Scaleway GPU Instances](https://www.scaleway.com/en/gpu-instances/) -- Instance types, pricing, regions
2. [Lambda Labs Cloud](https://lambdalabs.com/cloud) -- GPU pricing effective April 2026
3. [RunPod Pricing](https://www.runpod.io/pricing) -- Pod and serverless pricing
4. [Vast.ai Pricing](https://vast.ai/pricing) -- Live marketplace rates (March 2026)
5. [Together.ai Pricing](https://www.together.ai/pricing) -- Serverless inference per-token pricing
6. [Groq Pricing](https://groq.com/pricing/) -- LPU inference pricing
7. [NVIDIA H100 Datasheet](https://resources.nvidia.com/en-us-hopper-architecture/nvidia-tensor-core-gpu-datasheet) -- Official specs
8. [Technical.city RTX 5090 vs 4090](https://technical.city/en/video/GeForce-RTX-5090-vs-GeForce-RTX-4090) -- Consumer GPU specs

---

## Methodology

- Cloud pricing: Direct from provider websites (March 2026 snapshots)
- Vast.ai: Median on-demand pricing, subject to marketplace fluctuation
- RunPod: Approximate pricing (JS-rendered, cross-referenced with community reports)
- NVIDIA specs: Official datasheets + Hopper/Blackwell architecture whitepapers
- VRAM requirements: Based on parameter count x bytes per parameter + overhead
- Tokens/watt: Estimated from published benchmarks and TDP specs (varies significantly by model, quantization, batch size, and context length)
- All USD prices unless noted (Scaleway in EUR)

**Confidence Level**: High for specs and fixed-price providers. Medium for marketplace pricing (Vast.ai, RunPod Community) which fluctuates. Tokens/watt estimates are approximate and workload-dependent.
