# Fine-Tuning LLMs for Tool Use and Structured YAML Output

**Research Date:** March 27, 2026
**Target Model:** Qwen3.5-27B
**Target Task:** Generate valid Nika workflow YAML from natural language descriptions

---

## Executive Summary

Fine-tuning a 27B-parameter model for domain-specific YAML generation is both practical and affordable in 2026. The recommended approach is **QLoRA SFT** with **2,000-5,000 curated examples**, using **Unsloth + LLaMA-Factory** for prototyping and **Axolotl** for production. Training takes approximately 4-15 hours on 1-4x H100 GPUs at a cost of $50-$360. Post-SFT, optional **SimPO or GRPO** alignment can improve structured output adherence. The Qwen3.5-27B model is natively supported by all major frameworks and represents the best quality/cost ratio for this task.

---

## 1. QLoRA vs Full Fine-Tune vs LoRA

### When to Use Which

| Method | Performance vs Full FT | VRAM (27B model) | Training Speed | When to Use |
|--------|----------------------|-------------------|----------------|-------------|
| **Full Fine-Tuning** | 100% (baseline) | 160-200 GB (8x H100) | Slowest (baseline) | Only with very large datasets (50k+) and abundant compute |
| **LoRA** | 95-99% | 48-80 GB (2-4x H100) | 2-5x faster | Multi-GPU access, need highest quality |
| **QLoRA** | 95-98% | 24-48 GB (1-2x H100) | 2-5x faster | **Default choice.** Single/dual GPU, best value |

### Key Findings (2026)

- **LoRA introduces "intruder dimensions"** -- new high-rank singular vectors that cause structural differences from full fine-tuning. However, LoRA actually **forgets less pre-training knowledge** than full FT.
- **QLoRA (4-bit)** achieves near-equivalent accuracy to LoRA on structured tasks, with half the memory. The quality gap is typically less than 1-2%.
- **For Qwen3.5-27B specifically**: QLoRA fits on a single 48GB GPU (A6000/A100-40GB) or comfortably on 80GB H100. This is the recommended approach.
- **Performance on structured output** (JSON/YAML): all three methods perform comparably, since the task relies more on instruction following than deep knowledge. QLoRA is the clear winner for cost/quality.

### Recommendation for Nika

**Use QLoRA (4-bit).** The structured YAML generation task does not require the model to learn fundamentally new knowledge -- it needs to learn a specific output format and vocabulary. QLoRA is perfectly suited for this.

---

## 2. LLaMA-Factory

### Overview

LLaMA-Factory (68.4K GitHub stars, most popular framework) provides a **unified, GUI-driven** approach to fine-tuning 100+ models including the full Qwen3/3.5 family.

### Key Features (March 2026)

- **Web UI**: Zero-code fine-tuning via `llamafactory-cli webui`
- **Model support**: Qwen3, Qwen3.5 (all variants), LLaMA, Mistral, DeepSeek, Gemma
- **Training methods**: SFT, DPO, KTO, ORPO, PPO, reward modeling
- **Efficiency**: LoRA, QLoRA (2-8 bit), DoRA, LongLoRA, PiSSA, Unsloth backend
- **Acceleration**: FlashAttention-2, Unsloth integration (2-5x speed), GaLore

### Dataset Format for Tool-Use (ShareGPT)

```json
{
  "conversations": [
    {
      "role": "user",
      "content": "Create a workflow that researches a topic and summarizes findings"
    },
    {
      "role": "assistant",
      "content": "schema: \"nika/workflow@0.12\"\nworkflow: research-and-summarize\nprovider: anthropic\nmodel: claude-sonnet-4-20250514\n\ninputs:\n  topic: \"AI workflow engines\"\n\ntasks:\n  - id: research\n    infer:\n      prompt: |\n        Research the following topic: {{inputs.topic}}\n      temperature: 0.7\n\n  - id: summarize\n    depends_on: [research]\n    with:\n      data: $research\n    infer:\n      prompt: |\n        Create a concise summary from: {{with.data}}\n      max_tokens: 500"
    }
  ]
}
```

### Training Command

```bash
llamafactory-cli train \
  --stage sft \
  --model_name_or_path Qwen/Qwen3.5-27B \
  --dataset nika_workflows \
  --template qwen \
  --finetuning_type lora \
  --lora_target all \
  --quantization_bit 4 \
  --use_unsloth true \
  --output_dir qwen35_nika_lora \
  --per_device_train_batch_size 2 \
  --gradient_accumulation_steps 4 \
  --num_train_epochs 3 \
  --learning_rate 2e-4 \
  --fp16 true
```

### Best For

Prototyping and quick iteration. The GUI makes it accessible to non-ML engineers.

---

## 3. Unsloth

### Overview

Unsloth (53.9K stars) is the **speed king** of fine-tuning frameworks, with custom CUDA/Triton kernels that deliver 2-5x speedups and 70%+ VRAM savings.

### Qwen3.5 Support

**Yes, fully supported.** Unsloth explicitly supports:
- Qwen3.5-0.8B, 2B, 4B, 9B (small series)
- **Qwen3.5-27B** (dense)
- Qwen3.5-35B-A3B (MoE)
- Qwen3.5-122B-A10B, 397B-A17B (large MoE)

### 2026 Performance Claims (Validated)

| Optimization | Speedup | VRAM Savings | Context Gain |
|-------------|---------|-------------|--------------|
| **Dense models** | 2-5x | 70%+ | Standard |
| **MoE training** | 12x | >35% | ~6x |
| **Embeddings** | 2x | 20% | 2x |
| **RL (GRPO)** | Standard | Significant | 7-12x longer |

### Qwen3.5-27B on Unsloth

- **VRAM**: ~22 GB with 4-bit QLoRA (fits single RTX 4090 or A100-40GB)
- **Speed**: 2-3x faster than baseline HuggingFace Transformers
- **Export**: Direct GGUF export for local inference via llama.cpp/mistral.rs

### Best For

Single-GPU training, rapid iteration, budget-constrained teams.

---

## 4. Axolotl

### Overview

Axolotl (11.4K stars, v0.15.0) is the **production workhorse** -- maximum flexibility via YAML configuration, full multi-GPU/multi-node support, and bleeding-edge technique adoption.

### Key Features (March 2026)

- **Qwen3.5 support**: Confirmed for all variants including MoE
- **Training methods**: Full FT, LoRA, QLoRA, GPTQ, QAT, DPO/IPO, GRPO, GDPO, RLHF
- **Multi-GPU**: FSDP2, DeepSpeed, sequence parallelism
- **Advanced**: MoE expert quantization, ScatterMoE LoRA, sparse finetuning
- **Multimodal**: LLaMA-Vision, Qwen2-VL, Pixtral, text-to-speech
- **Unsloth backend**: Integrates Unsloth kernels for speed

### Configuration Example

```yaml
base_model: Qwen/Qwen3.5-27B
model_type: AutoModelForCausalLM
tokenizer_type: AutoTokenizer

load_in_4bit: true
adapter: qlora
lora_model_dir:

sequence_len: 4096
sample_packing: true
pad_to_sequence_len: true

lora_r: 32
lora_alpha: 64
lora_dropout: 0.05
lora_target_linear: true

datasets:
  - path: ./data/nika_workflows.jsonl
    type: sharegpt

val_set_size: 0.05
output_dir: ./qlora-out

gradient_accumulation_steps: 4
micro_batch_size: 2
num_epochs: 3
optimizer: adamw_bnb_8bit
lr_scheduler: cosine
learning_rate: 2e-4
warmup_steps: 100

bf16: auto
flash_attention: true

wandb_project: nika-finetune
```

### Best For

Production pipelines, team-scale reproducible training, multi-GPU clusters.

---

## 5. Hyperparameters for Tool-Use Fine-Tuning

### Recommended Starting Configuration

| Parameter | Value | Notes |
|-----------|-------|-------|
| **Learning rate** | 2e-4 | Standard for Qwen3.5 with AdamW-8bit |
| **Epochs** | 2-3 | Monitor eval loss; stop early if plateauing |
| **LoRA rank (r)** | 16-32 | 16 for small datasets (<2k), 32 for larger (5k+) |
| **LoRA alpha** | 2x rank (32-64) | Standard ratio |
| **LoRA target** | All linear layers | q_proj, k_proj, v_proj, o_proj minimum |
| **LoRA dropout** | 0.05 | Prevents overfitting on small datasets |
| **Batch size** | 2-4 per GPU | With gradient accumulation to effective 8-16 |
| **Warmup ratio** | 0.03-0.1 | Linear warmup prevents early divergence |
| **Optimizer** | AdamW-8bit | Saves memory vs full AdamW |
| **LR scheduler** | Cosine or linear | Cosine slightly better for longer runs |
| **Max sequence length** | 2048-4096 | Nika YAML workflows are typically 500-2000 tokens |
| **Precision** | BF16 | Preferred on Ampere+ GPUs |
| **Weight decay** | 0.01 | Standard regularization |

### Tips Specific to YAML Generation

- **Sample packing**: Enable to maximize GPU utilization (shorter YAML examples can be packed together)
- **Data mix**: 75% chain-of-thought examples (natural language -> thinking -> YAML) + 25% direct (natural language -> YAML)
- **Cutoff length**: Set to 4096 to handle complex multi-task workflows
- **Gradient checkpointing**: Enable to trade compute for memory on single GPU

---

## 6. SimPO vs DPO vs ORPO (Preference Alignment)

### Comparison for Structured Output

| Method | Best For | Quality vs DPO | Memory | Speed | Reference Model? |
|--------|---------|----------------|--------|-------|-----------------|
| **SimPO** | **Structured output (recommended)** | +6.4 AlpacaEval, +7.5 Arena-Hard | 10% less than DPO | 20% faster | No |
| **DPO** | General alignment baseline | Baseline | Higher | Slower | Yes (expensive) |
| **ORPO** | Single-stage SFT+alignment | ~DPO quality | Lower | Slowest convergence | No |
| **KTO** | Binary-only feedback | Below SimPO | Low | Fast | No |

### Newer Methods (2025-2026)

| Method | Description | Practical for Small Teams? |
|--------|-------------|---------------------------|
| **GRPO** | Group Relative Policy Optimization (DeepSeek-R1 style). Eliminates critic model, samples group of responses, computes relative advantages. | Yes, with verifiable rewards |
| **DAPO** | Decoupled GRPO variant. 50% fewer training steps. Better for long-chain reasoning. | Yes, open-sourced |
| **RLVR** | RL with Verifiable Rewards. Uses automated validators instead of human preferences. | **Best for YAML** -- can use schema validators as reward |
| **AlphaPO** | DPO variant with alpha-parameter for reward shaping. 7-10% over SimPO. | Emerging, limited tooling |

### Recommendation for Nika YAML Generation

**Phase 1: SFT only** (start here, likely sufficient)
**Phase 2: SimPO** if SFT quality is not enough -- requires preference pairs (good YAML vs bad YAML)
**Phase 3: RLVR + GRPO** for maximum quality -- use `nika check` as the verifiable reward signal (validates YAML syntax + DAG)

The RLVR approach is particularly compelling for Nika because we have a **perfect automated validator**: the `nika check` command. Any generated YAML that passes validation gets a positive reward; failures get a negative reward. This is exactly the setup that GRPO excels at.

---

## 7. Synthetic Data Generation

### Strategy for Nika Training Data

#### Step 1: Seed Examples (Manual, 100-200)
Create high-quality hand-written examples covering:
- All 5 verbs (infer, exec, fetch, invoke, agent)
- Simple to complex workflows (1 task to 10+ tasks)
- Data flow patterns (with/depends_on/for_each)
- Edge cases (nested templates, pipe transforms, structured output)

#### Step 2: Teacher Model Expansion (Claude, 2000-5000)
Use Claude to generate variations:

```
Given this Nika workflow schema reference and these seed examples,
generate 50 diverse training pairs. Each pair should have:
1. A natural language description of what the user wants
2. The correct Nika workflow YAML

Vary complexity from simple single-task to multi-task DAGs.
Include: different providers, tool invocations, fetch+extract,
for_each loops, agent blocks, artifacts, error handling.
```

#### Step 3: Quality Filtering
- **Schema validation**: Run `nika check` on every generated YAML (reject invalid)
- **LLM-as-Judge**: Score 0-5 for correctness, completeness, idiomatic style
- **Deduplication**: Embed with sentence-transformers, remove near-duplicates
- **Diversity selection**: Ensure coverage of all verbs, patterns, complexity levels

#### Step 4: Evol-Instruct (Optional, for 5000+)
Iteratively evolve instructions for complexity:
- Round 1: "Add error handling to this workflow"
- Round 2: "Make it use for_each with concurrency"
- Round 3: "Add structured output validation"
- Round 4: "Convert to use agent verb with guardrails"

### Practical Numbers

| Dataset Size | Quality | Training Time (QLoRA, 27B) | Expected Outcome |
|-------------|---------|---------------------------|------------------|
| 500 | Minimum viable | ~1 hour | Basic YAML generation, frequent errors |
| 1,000-2,000 | Good starting point | ~2-4 hours | Reliable simple workflows, some complex |
| 5,000 | **Recommended** | ~8-15 hours | Strong across all patterns |
| 10,000 | Diminishing returns | ~15-30 hours | Marginal improvement over 5k |

**Key insight: Quality > Quantity.** 2,000 perfectly curated examples outperform 10,000 noisy ones.

---

## 8. Dataset Size Guidance

### For Nika YAML Generation Specifically

The task has several properties that affect dataset size requirements:

**Factors reducing data needs:**
- YAML is a well-known format (pre-training exposure)
- The schema is constrained (5 verbs, limited fields)
- Output structure is predictable (schema, workflow, tasks array)

**Factors increasing data needs:**
- Domain-specific vocabulary (Nika-specific fields like `for_each`, `with:`, `$task_id`)
- Template syntax (`{{with.alias}}`, pipe transforms)
- Complex DAG patterns (depends_on, for_each, agent loops)
- Multiple extract modes, response formats, error handling

### Recommended Phased Approach

| Phase | Examples | Focus | Expected Quality |
|-------|----------|-------|-----------------|
| **Phase 1** | 500 | Core patterns: simple infer, exec, fetch | 70% valid YAML |
| **Phase 2** | 2,000 | All verbs, data flow, templates | 85% valid YAML |
| **Phase 3** | 5,000 | Edge cases, complex DAGs, artifacts | 92%+ valid YAML |
| **Phase 4** | 5,000 + SimPO/GRPO | Preference alignment | 95%+ valid YAML |

---

## 9. Evaluation Metrics

### Multi-Layer Evaluation Framework

#### Layer 1: Automated Syntax Validation
- **YAML parse rate**: Does the output parse as valid YAML? (target: 99%+)
- **Schema validation rate**: Does it pass `nika check`? (target: 95%+)
- **DAG validation**: Are dependencies valid, no cycles? (target: 98%+)

#### Layer 2: Structural Correctness
- **Field accuracy**: Correct field names (not `use:` instead of `with:`, not `timeout: 30000` instead of `timeout: 30`)
- **Template resolution**: Valid template syntax (`{{with.alias}}` not `{{alias}}`)
- **Verb correctness**: Appropriate verb for the task (not using `exec:` when `infer:` is needed)

#### Layer 3: Semantic Quality
- **Task completeness**: Does the workflow accomplish what the user asked?
- **Idiomatic style**: Does it follow Nika best practices (proper depends_on, not redundant bindings)?
- **LLM-as-Judge score**: Human-aligned quality rating (1-5)

#### Practical Metrics

| Metric | What It Measures | Target | How to Compute |
|--------|-----------------|--------|----------------|
| **Parse Rate** | Valid YAML syntax | >99% | `yaml.safe_load()` on output |
| **Schema Pass Rate** | Passes `nika check` | >95% | Run validator on output |
| **Exact Match (EM)** | Identical to reference | >30% | Token-level comparison |
| **ROUGE-L** | Structural similarity | >0.7 | n-gram overlap |
| **Field Accuracy** | Correct field names | >98% | Custom field-level checker |
| **Template Accuracy** | Valid template syntax | >97% | Regex + parser check |
| **Functional Correctness** | Workflow runs (mock) | >90% | `nika run --provider mock` |

### BFCL (Berkeley Function Calling Leaderboard) Relevance

BFCL measures tool-use accuracy for function calling. While not directly applicable to YAML generation, its **AST evaluation** methodology is relevant -- comparing the structural tree of generated output against reference. A similar approach can evaluate Nika workflow structure.

### Custom Evaluation Pipeline

```python
def evaluate_nika_yaml(generated: str, reference: str) -> dict:
    scores = {}

    # Layer 1: Syntax
    scores['yaml_valid'] = yaml_parses(generated)
    scores['schema_valid'] = nika_check(generated)

    # Layer 2: Structure
    scores['field_accuracy'] = compare_fields(generated, reference)
    scores['template_accuracy'] = validate_templates(generated)
    scores['verb_correct'] = check_verb_usage(generated, reference)

    # Layer 3: Semantic
    scores['rouge_l'] = compute_rouge_l(generated, reference)
    scores['functional'] = nika_run_mock(generated)

    return scores
```

---

## 10. Cost of Fine-Tuning on H100

### Qwen3.5-27B QLoRA Training Estimates

| Configuration | GPUs | VRAM Used | Training Time (5k examples) | Cloud Cost |
|--------------|------|-----------|---------------------------|------------|
| **QLoRA 4-bit** (recommended) | 1x H100 80GB | ~40 GB | 8-12 hours | $25-$35 |
| **QLoRA 4-bit** | 2x H100 80GB | ~25 GB each | 4-6 hours | $25-$35 |
| **QLoRA 4-bit** | 4x H100 80GB | ~15 GB each | 2-3 hours | $30-$45 |
| **LoRA BF16** | 2x H100 80GB | ~70 GB each | 6-10 hours | $35-$60 |
| **Full FT** | 8x H100 80GB | ~200 GB total | 15-25 hours | $240-$600 |

### Cloud GPU Pricing (March 2026)

| Provider | H100 80GB (per hour) | A100 80GB (per hour) | Notes |
|----------|---------------------|---------------------|-------|
| **Jarvislabs** | $2.69 | $1.50 | Cheapest major provider |
| **Lambda Labs** | $2.99 | $1.99 | Reliable, good availability |
| **RunPod** | $2.50-$2.99 | $1.64 | Spot instances available |
| **Fluence** | $1.50 | N/A | Decentralized, variable quality |
| **AWS** | $2.80-$9.00 | $2.10-$4.10 | p5 instances, highest reliability |

### Total Project Cost Estimate

| Phase | Task | GPU Hours | Cost |
|-------|------|-----------|------|
| **Data generation** | Claude API for 5000 examples | N/A | ~$50-100 (API costs) |
| **Phase 1 SFT** | QLoRA training, 3 epochs | 10h x 1 H100 | ~$30 |
| **Evaluation** | Validation + mock runs | 2h x 1 H100 | ~$6 |
| **Phase 2 iteration** | Hyperparameter tuning (3 runs) | 30h x 1 H100 | ~$90 |
| **Phase 3 SimPO** (optional) | Preference alignment | 15h x 1 H100 | ~$45 |
| **GGUF export** | Quantize for deployment | 1h x 1 H100 | ~$3 |
| **Total** | | ~58h | **~$225-$375** |

### H100 vs A100 Comparison for This Task

| Metric | H100 80GB | A100 80GB |
|--------|-----------|-----------|
| Training speed (27B QLoRA) | 1x (baseline) | 0.5-0.6x |
| VRAM | 80 GB | 80 GB |
| Cost per hour | $2.50-3.00 | $1.50-2.00 |
| Total cost (same job) | Lower (fewer hours) | Similar (more hours) |
| Recommendation | **Preferred** for <24h jobs | Fine for budget-constrained |

---

## Framework Comparison Summary

### The EVAL #003 Verdict (March 2026)

| Framework | GitHub Stars | Best For | Qwen3.5 Support | Speed | Learning Curve |
|-----------|-------------|---------|-----------------|-------|---------------|
| **LLaMA-Factory** | 68.4K | GUI-first prototyping | Full | 1-2x (Unsloth backend) | Low |
| **Unsloth** | 53.9K | Single-GPU speed | Full | 2-5x (12x MoE) | Low-Medium |
| **TRL** | 17.6K | RLHF/GRPO alignment | HF ecosystem | 1x baseline | Medium-High |
| **Axolotl** | 11.4K | Production pipelines | Full | 1x + multi-GPU | Medium |

### Recommendation for Nika Team

**Phase 1 (Prototype):** LLaMA-Factory with Unsloth backend
- Quick iteration with web UI
- Test dataset quality before committing to long runs

**Phase 2 (Training):** Unsloth directly
- Maximum single-GPU speed
- Direct GGUF export for Nika's `provider: native`

**Phase 3 (Production):** Axolotl
- Reproducible YAML configs (fits Nika's YAML-first philosophy)
- Multi-GPU scaling if needed

**Phase 4 (Alignment):** TRL + Unsloth
- GRPO with `nika check` as verifiable reward
- SimPO for preference alignment

---

## Qwen3.5-27B Model Details

### Architecture

- **Released**: February 25, 2026
- **Type**: Dense transformer (not MoE)
- **Parameters**: 27B
- **Context**: 256K tokens
- **License**: Apache 2.0
- **Multimodal**: Native text + vision + UI understanding
- **Languages**: 201 languages
- **HuggingFace ID**: `Qwen/Qwen3.5-27B`

### Why Qwen3.5-27B for Nika

1. **Size/quality ratio**: 27B is the sweet spot -- large enough for complex YAML, small enough for single-GPU QLoRA
2. **Native multimodal**: Future potential for vision-based workflow generation
3. **256K context**: Handles very long workflow specifications
4. **Apache 2.0**: Compatible with AGPL-3.0 (Nika's license)
5. **Framework support**: Full support in Unsloth, LLaMA-Factory, Axolotl
6. **GGUF export**: Direct path to `provider: native` in Nika
7. **MoE alternative**: Qwen3.5-35B-A3B offers 35B quality with only 3B active parameters (17.5 GB VRAM)

### MoE Alternative: Qwen3.5-35B-A3B

The MoE variant is worth serious consideration:
- Only 3B parameters active at inference (10x cheaper to serve)
- 17.5 GB VRAM with Unsloth QLoRA
- Near-27B quality on structured tasks
- 12x faster training with Unsloth's MoE kernels
- Ideal for deployment on consumer GPUs

---

## Actionable Next Steps

### Week 1: Data Preparation
1. Write 100-200 seed examples covering all Nika verbs and patterns
2. Use Claude to generate 2,000-3,000 variations with Evol-Instruct
3. Validate all examples with `nika check --strict`
4. Filter to 2,000 high-quality examples

### Week 2: Initial Training
1. Set up Unsloth on a single H100 (or RTX 4090 for testing)
2. Train QLoRA (r=16, alpha=32, lr=2e-4, 3 epochs)
3. Evaluate parse rate, schema pass rate, functional correctness
4. Iterate on hyperparameters (3-5 runs)

### Week 3: Refinement
1. Expand dataset to 5,000 examples based on failure analysis
2. Train with optimized hyperparameters (r=32, alpha=64)
3. Optional: SimPO alignment with preference pairs
4. Export to GGUF for local inference testing

### Week 4: Integration
1. Test GGUF model with `provider: native` in Nika
2. Benchmark against Claude/GPT-4 on Nika workflow generation
3. Set up GRPO with `nika check` as reward (if quality not sufficient)
4. Document results and publish model

---

## Sources

1. [QLoRA vs LoRA comparison](https://www.newline.co/@Dipen/qlora-vs-lora-which-finetuning-wins--683ca660) - Newline, Jan 2026
2. [LoRA vs Full FT: An Illusion of Equivalence](https://arxiv.org/html/2410.21228v3) - arXiv, updated Mar 2026
3. [LLaMA-Factory GitHub](https://github.com/hiyouga/LlamaFactory) - Updated Jan 2026
4. [Unsloth 2026 Update](https://unslothai.substack.com/p/unsloth-2026-update-faster-moe) - Feb 2026
5. [Unsloth Qwen3.5 Docs](https://unsloth.ai/docs/models/qwen3.5) - Mar 2026
6. [EVAL #003: Fine-Tuning in 2026](https://dev.to/ultraduneai/eval-003-fine-tuning-in-2026-axolotl-vs-unsloth-vs-trl-vs-llama-factory-2ohg) - Mar 2026
7. [Axolotl v0.15.0 Release](https://github.com/axolotl-ai-cloud/axolotl/releases) - 2026
8. [Post-Training in 2026: GRPO, DAPO, RLVR](https://llm-stats.com/blog/research/post-training-techniques-2026) - Mar 2026
9. [SimPO NeurIPS Paper](https://neurips.cc/virtual/2024/poster/96741) - Updated Mar 2026
10. [Qwen3.5 DataCamp Overview](https://www.datacamp.com/es/blog/qwen3-5) - Feb 2026
11. [H100 Pricing Guide 2026](https://docs.jarvislabs.ai/blog/h100-price) - Mar 2026
12. [Spheron Fine-Tuning Guide 2026](https://www.spheron.network/blog/how-to-fine-tune-llm-2026/) - Mar 2026
13. [NVIDIA Qwen3.5-35B-A3B on DGX Spark](https://forums.developer.nvidia.com/t/bf16-lora-fine-tuning-of-qwen3-5-35b-a3b-on-dgx-spark-no-quantization-required/363268) - Mar 2026
14. [Qwen3 Hyperparameters - Unsloth](https://unsloth.ai/docs/models/qwen3-how-to-run-and-fine-tune) - Mar 2026
15. [Synthetic Data - Eugene Yan](https://eugeneyan.com/writing/synthetic/) - Updated Mar 2026
16. [SDG Hub for Domain-Specific LLMs](https://developers.redhat.com/articles/2025/11/25/building-domain-specific-llms-synthetic-data-and-sdg-hub) - Nov 2025
17. [AWS Multi-Agent Fine-Tuning](https://aws.amazon.com/blogs/machine-learning/advanced-fine-tuning-techniques-for-multi-agent-orchestration-patterns-from-amazon-at-scale/) - Jan 2026
18. [GRPO Tricks](https://cameronrwolfe.substack.com/p/grpo-tricks) - Jan 2026
19. [Qwen3.5 API Pricing](https://pricepertoken.com/pricing-page/model/qwen3.5-27b) - Mar 2026
20. [Axolotl vs Unsloth vs TorchTune](https://www.spheron.network/blog/axolotl-vs-unsloth-vs-torchtune/) - Mar 2026
