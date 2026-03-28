# Research Report: Fine-Tuning LLMs for Workflow Orchestration

**Date**: 2026-03-27
**Purpose**: Inform the strategy for fine-tuning a model that natively understands the Nika workflow schema and can generate valid `.nika.yaml` files from natural language.

---

## Summary

Fine-tuning an LLM for Nika workflow generation is feasible and well-supported by existing research. The recommended path is: (1) generate 5,000-10,000 synthetic Nika workflow examples using Claude as teacher, (2) SFT a Qwen3-8B or 27B base with LoRA using LLaMA-Factory, (3) refine with DPO/SimPO using preference pairs of valid vs. invalid workflows. The Hermes and xLAM projects provide direct templates for how to train tool-use capabilities into open models. The entire pipeline can be executed on a single 80GB GPU (A100/H100) with LoRA in under 48 hours.

---

## 1. Function Calling Datasets

### 1.1 Glaive Function Calling v2

- **Source**: https://huggingface.co/datasets/glaiveai/glaive-function-calling-v2
- **Size**: 113,000 rows
- **Format**: System prompt with JSON function signatures, user query, assistant response (either tool call or refusal)
- **Quality**: Synthetically generated. Good diversity of function signatures. Each example includes the system prompt with available tools, a user message, and the model's response.
- **Relevance to Nika**: Medium. The function-calling format maps well to the `invoke:` verb, but doesn't cover YAML workflow generation, DAG composition, or multi-step orchestration.

### 1.2 Salesforce xLAM Function Calling 60k (APIGen)

- **Source**: https://huggingface.co/datasets/Salesforce/xlam-function-calling-60k
- **Paper**: APIGen (arXiv:2406.18518)
- **Size**: 60,000 verified examples across 3,673 executable APIs in 21 categories
- **Generation**: Created by DeepSeek-V2-Chat (33,659 entries) and Mixtral-8x22B-Inst (remainder)
- **Verification**: Three-stage validation -- format checking, actual function execution, and semantic verification. Human evaluation shows 95%+ correctness rate.
- **Key insight**: The APIGen pipeline is directly applicable to Nika. You could build an analogous pipeline: define Nika tool schemas, generate candidate workflows with Claude, then validate them with `nika check --strict` and `nika run --dry-run`.
- **Models trained on this**: xLAM-1b-fc-r (#25 on BFCL), xLAM-7b-fc-r (#3 on BFCL) -- proving that even 1B parameter models can achieve competitive function-calling with high-quality data.

### 1.3 Berkeley Function Calling Leaderboard (BFCL)

- **Source**: https://gorilla.cs.berkeley.edu/leaderboard.html
- **Role**: The primary benchmark for evaluating function-calling models
- **Categories evaluated**: Simple function calls, multiple function calls, parallel function calls, Java/JavaScript/Python execution, REST API calls, relevance detection (knowing when NOT to call a function)
- **Key observation**: Models trained on small but high-quality function-calling datasets (like xLAM-1b) can outperform GPT-3.5-Turbo. Quality > quantity.

### 1.4 NousResearch Hermes Function Calling v1

- **Source**: https://huggingface.co/datasets/NousResearch/hermes-function-calling-v1
- **Size**: ~1,890 rows (single-turn) across 5 subsets
- **Format**: ChatML with `<tools>` XML tags for function definitions and `<tool_call>` for invocations
- **Categories**: 63 categories with fine-grained subcategories
- **Key insight**: Hermes achieves excellent tool-use with a relatively small dataset (~17M tokens for the tool use portion). This suggests Nika can achieve good results with a focused dataset.

### 1.5 YAML-Specific Training Data

- **No existing YAML workflow datasets exist for fine-tuning.** This is a gap and an opportunity.
- Closest analogs: GitHub Actions YAML, Kubernetes manifests, Ansible playbooks, Terraform HCL -- all have been seen in pretraining data but none match the `nika/workflow@0.12` schema.
- **Recommendation**: This is the highest-value data to create. Nika already has 226+ course workflows and 115 showcase workflows that can seed the training set.

---

## 2. Hermes Fine-Tuning Approach (NousResearch)

### 2.1 Architecture

- **Base models**: Llama 3.1 8B, 70B, and 405B
- **Training method**: Two-phase -- SFT followed by DPO
- **Tool use format**: ChatML with XML tags (`<tools>`, `<tool_call>`, `<tool_response>`, `<scratch_pad>`)
- **Total dataset**: ~390 million tokens across 8 categories

### 2.2 Data Mixture (from Hermes 3 Technical Report, arXiv:2408.11857)

| Category | Proportion | Tokens |
|----------|-----------|--------|
| General Instructions | 60.6% | 236M |
| Domain Expert | 12.8% | 50M |
| Math | 6.7% | 26M |
| Roleplaying | 6.1% | 24M |
| Coding | 4.5% | 18M |
| **Tool Use, Agentic, and RAG** | **4.3%** | **17M** |
| Content Generation | 3.0% | 12M |
| Steering and Alignment | 2.5% | 10M |

**Critical finding**: Tool use represents only 4.3% of the total training mix (~17M tokens) yet Hermes is one of the best open-source models for function calling. This proves that a focused, high-quality tool-use dataset is sufficient -- you don't need millions of examples.

### 2.3 Training Recipe

- **Optimizer**: AdamW with weight decay 0.01
- **Learning rate**: 7e-6 (selected via hyperparameter sweep on 8B models)
- **Schedule**: Cosine decay with 300-step warmup
- **Epochs**: 4
- **Sequence length**: 8192 tokens
- **Packing**: Flash Attention 2 variable-length packing at 96% efficiency
- **Loss masking**: Instruction and tool output tokens are masked (ignore value -100). Only response and tool-call tokens contribute to the loss. This is critical for Nika -- the model should learn to generate workflows, not memorize the schema descriptions.
- **DPO phase**: Applied after SFT for preference alignment

### 2.4 Agentic Capabilities

Hermes uses special tokens for structured reasoning:
- `<SCRATCHPAD>`, `<REASONING>`, `<INNER_MONOLOGUE>`, `<PLAN>`, `<EXECUTION>`, `<REFLECTION>`, `<THINKING>`, `<SOLUTION>`, `<EXPLANATION>`, `<UNIT_TEST>`

**Nika parallel**: The agent verb's `completion: explicit` mode (requiring `nika:complete` to stop) maps directly to this structured output approach. A fine-tuned Nika model could use similar tags for workflow planning before generation.

---

## 3. Fine-Tuning Qwen for Nika Workflows

### 3.1 Why Qwen

- Qwen2.5 series has strong instruction following and structured output capabilities
- Native JSON generation support (important for `structured:` blocks)
- 128K context window (important for complex multi-task workflows)
- Qwen3 (if available, expected 2025-2026) further improves tool use natively
- Open weights under permissive license (Apache 2.0 for most variants)
- Nika already supports Qwen via custom endpoints and native GGUF

### 3.2 LoRA vs. Full Fine-Tune

| Aspect | LoRA | Full Fine-Tune |
|--------|------|----------------|
| **VRAM for 27B** | ~24-48 GB (4-bit QLoRA) | ~160+ GB (bf16) |
| **Training time** | 12-24 hours (1x A100) | 2-5 days (8x A100) |
| **Data needed** | 1,000-10,000 examples | 10,000-100,000+ examples |
| **Risk of catastrophic forgetting** | Low (base weights frozen) | High without careful mixing |
| **Performance ceiling** | ~90-95% of full FT | 100% |
| **Merge & deploy** | Merge adapters for inference | Direct deployment |
| **Recommended for Nika** | **Yes -- Phase 1** | Phase 2 if needed |

**Recommendation**: Start with QLoRA (4-bit quantization + LoRA rank 64-128). For a domain-specific task like Nika workflow generation, LoRA should capture 90%+ of the possible improvement. Full fine-tune only makes sense once you have 50,000+ validated examples and need the last few percent.

**LoRA configuration for Qwen2.5:**
```python
peft_config = LoraConfig(
    r=64,                          # Rank (64-128 for tool use)
    lora_alpha=128,                # Scaling factor (usually 2x rank)
    lora_dropout=0.05,
    target_modules=["q_proj", "k_proj", "v_proj", "o_proj",
                     "gate_proj", "up_proj", "down_proj"],
    bias="none",
    task_type="CAUSAL_LM",
)
```

### 3.3 How Many Examples?

Based on the xLAM and Hermes precedents:

| Example Count | Expected Capability |
|--------------|-------------------|
| 500 | Basic schema understanding, simple single-task workflows |
| 1,000-2,000 | Reliable single-verb workflows, correct `with:` bindings |
| 5,000 | Multi-task DAGs, `for_each`, correct `depends_on:` chains |
| 10,000 | Complex agent workflows, proper error handling, `retry:` |
| 50,000+ | Near-expert level, edge cases, creative compositions |

**Sweet spot for Nika**: 5,000-10,000 high-quality validated examples. This is achievable with synthetic generation in 1-2 weeks.

### 3.4 Toolchain: LLaMA-Factory

- **Repository**: https://github.com/hiyouga/LLaMA-Factory (95k+ stars, ACL 2024)
- **Supports**: Qwen2.5 natively, LoRA/QLoRA, DPO/ORPO/SimPO, all in one package
- **Data format**: ShareGPT-style JSON or Alpaca format
- **Key advantage**: Web UI for training configuration, built-in evaluation, and one-command training
- **Alternatives**: Axolotl, TRL (HuggingFace), unsloth (for speed)

---

## 4. Synthetic Data Generation for Workflow Training

### 4.1 Data Generation Pipeline

```
Phase 1: Seed Collection
  - Nika's 226 course workflows
  - 115 showcase workflows
  - Any user-created workflows from internal use
  - Total seed: ~350 real examples

Phase 2: Synthetic Expansion (using Claude as teacher)
  - Input: Natural language task description
  - Output: Valid .nika.yaml workflow
  - Variations: different verbs, different providers,
    different complexity levels
  - Target: 5,000-10,000 pairs

Phase 3: Validation
  - nika check workflow.nika.yaml (schema validation)
  - nika check workflow.nika.yaml --strict (MCP validation)
  - nika run workflow.nika.yaml --dry-run (execution validation)
  - nika run workflow.nika.yaml --provider mock (mock execution)
  - Discard any example that fails validation

Phase 4: Quality Filtering
  - Deduplication (semantic similarity)
  - Complexity scoring (reward longer, multi-step workflows)
  - Correctness scoring (penalize common mistakes from the spec)
```

### 4.2 Training Data Format

Each training example should be a conversation:

```json
{
  "conversations": [
    {
      "from": "system",
      "value": "You are Nika, a YAML workflow engine assistant. You generate valid nika/workflow@0.12 workflows from natural language descriptions. [Nika schema reference here]"
    },
    {
      "from": "human",
      "value": "Create a workflow that fetches the latest Hacker News front page, extracts the article titles, then summarizes them with Claude."
    },
    {
      "from": "gpt",
      "value": "```yaml\nschema: \"nika/workflow@0.12\"\nworkflow: hn-summarizer\ndescription: \"Fetch and summarize Hacker News front page\"\nprovider: anthropic\nmodel: claude-sonnet-4-20250514\n\ntasks:\n  - id: fetch_hn\n    fetch:\n      url: \"https://news.ycombinator.com\"\n      extract: text\n      selector: \".titleline > a\"\n\n  - id: summarize\n    depends_on: [fetch_hn]\n    with:\n      titles: $fetch_hn\n    infer:\n      prompt: |\n        Here are today's Hacker News headlines:\n        {{with.titles}}\n        \n        Summarize the main themes and trends.\n      temperature: 0.3\n```"
    }
  ]
}
```

### 4.3 Evol-Instruct for Workflow Complexity

Inspired by WizardCoder's Evol-Instruct technique, evolve simple workflows into complex ones:

1. **Add constraints**: "Now make it work with rate limiting" -> adds `retry:` blocks
2. **Increase depth**: "Add an intermediate processing step" -> more DAG nodes
3. **Add parallelism**: "Process all URLs concurrently" -> `for_each` with `concurrency:`
4. **Add error handling**: "Handle failures gracefully" -> `fail_fast: false`, retry policies
5. **Mix verbs**: "Also save results to disk" -> adds `exec:` or artifact blocks
6. **Add conditionals**: "Only summarize if there are more than 5 articles" -> template logic

### 4.4 Negative Examples (Critical for DPO)

Generate deliberately incorrect workflows for preference training:

| Common Mistake | Incorrect | Correct |
|---------------|-----------|---------|
| Missing $ prefix | `with: { data: step1 }` | `with: { data: $step1 }` |
| Wrong template | `{{data}}` | `{{with.data}}` |
| Wrong extension | `.yaml` | `.nika.yaml` |
| Timeout in ms | `timeout: 30000` | `timeout: 30` |
| Missing schema | (no schema line) | `schema: "nika/workflow@0.12"` |
| Wrong separator | `tool: "server/tool"` | `tool: "server::tool"` |
| Wrong depends_on | `depends_on: task_id` | `depends_on: [task_id]` |

---

## 5. RLHF/DPO for Agent Behavior

### 5.1 Preference Optimization Landscape (2024-2026)

| Method | Reference Model | Data Format | Key Advantage |
|--------|----------------|-------------|---------------|
| **DPO** | Required | Paired preferences | Most robust, well-understood |
| **IPO** | Required | Paired preferences | Regularized, avoids overfitting |
| **KTO** | Required | Binary (good/bad) | No paired data needed |
| **ORPO** | Not required | Paired preferences | Single phase (SFT + alignment combined) |
| **SimPO** | Not required | Paired preferences | Length-normalized, memory efficient |

### 5.2 Recommended Approach for Nika: SimPO

**Why SimPO** (arXiv:2405.14734, NeurIPS 2024):
- No reference model needed = half the memory during training
- Uses average log probability as implicit reward (better for structured output)
- Consistently outperforms DPO by 6.4 points on AlpacaEval 2
- Length normalization prevents the model from gaming the reward by generating verbose YAML
- Best-performing method on Gemma-2-9B-it (72.4% LC win rate on AlpacaEval 2)

**Why NOT ORPO for Nika**:
- ORPO combines SFT and alignment in one phase, which is elegant but gives less control
- For a domain-specific task like Nika, you want to separately validate the SFT checkpoint before alignment

### 5.3 Reward Signals for Workflow Correctness

Define a multi-dimensional reward function:

```
R(workflow) = w1 * schema_valid(workflow)      # 0/1: passes nika check
            + w2 * dag_valid(workflow)          # 0/1: no cycles, valid deps
            + w3 * binding_valid(workflow)      # 0/1: all {{with.*}} resolve
            + w4 * execution_valid(workflow)    # 0/1: passes --dry-run
            + w5 * semantic_match(query, wf)    # 0-1: does it match intent?
            + w6 * efficiency_score(workflow)   # 0-1: minimal, no redundancy
```

This is far better than generic RLHF because Nika has **deterministic validators** -- you can automatically generate preference pairs by running `nika check` on candidate workflows.

### 5.4 Training Pipeline

```
Stage 1: SFT on validated (query, workflow) pairs
  -> Produces a model that can generate plausible workflows

Stage 2: Generate N candidate workflows per query
  -> Sample with temperature > 0 to get diversity

Stage 3: Score candidates with nika check + semantic matching
  -> Creates (chosen, rejected) pairs automatically

Stage 4: SimPO/DPO training on preference pairs
  -> Model learns to prefer valid, efficient workflows
```

---

## 6. Distillation from Claude to Smaller Models

### 6.1 Teacher-Student Approach

The core idea: use Claude Opus/Sonnet to generate high-quality Nika workflows, then train a smaller model (Qwen3-8B or 27B) to replicate that behavior.

**Precedents**:
- **Orca** (Microsoft): Distilled GPT-4 reasoning into 13B model, matching ChatGPT-level performance
- **Zephyr** (HuggingFace): Distilled GPT-4 preferences into Mistral-7B via DPO, creating one of the best 7B chat models
- **xLAM** (Salesforce): 1B parameter model trained on 60k synthetic examples ranked #25 on BFCL, outperforming GPT-3.5-Turbo at function calling
- **WizardCoder**: Used Evol-Instruct (complexity escalation) to distill code generation from GPT-4 into StarCoder

### 6.2 What's Realistic with 1,000-10,000 Examples

| Scale | What You Get | Confidence |
|-------|-------------|------------|
| 1,000 | Model understands Nika schema basics, generates simple valid workflows 70-80% of the time | Medium |
| 2,500 | Reliable single-verb and two-step workflows, correct bindings 85-90% | Medium-High |
| 5,000 | Multi-task DAGs, for_each, artifacts -- 90%+ valid on first try | High |
| 10,000 | Handles edge cases, complex agent loops, structured output -- expert-level | Very High |

**The xLAM precedent is the strongest evidence**: Salesforce trained a 1B parameter model on 60,000 examples and it outperformed GPT-3.5-Turbo at function calling. For Nika's constrained domain (5 verbs, 1 schema), 5,000-10,000 examples should be more than sufficient for a 8B-27B model.

### 6.3 Practical Distillation Pipeline for Nika

```
Step 1: Define the seed tasks (500 natural language descriptions)
  Examples:
  - "Scrape a blog and summarize it"
  - "Run a shell command and format the output"
  - "Call an MCP tool and process the result"
  - "Create an agent that researches a topic using 3 tools"

Step 2: Use Claude to generate 10 workflow variants per task
  - Different complexity levels (simple/medium/complex)
  - Different providers (anthropic, openai, native, mock)
  - Different verb combinations
  -> 5,000 raw examples

Step 3: Validate all examples
  for each workflow:
    nika check workflow.nika.yaml
    nika run workflow.nika.yaml --dry-run
    nika run workflow.nika.yaml --provider mock
  -> Keep only passing examples (expect 80-90% pass rate)
  -> ~4,000-4,500 validated examples

Step 4: Evol-Instruct expansion
  Take validated examples, ask Claude to create harder versions:
  - Add for_each parallelism
  - Add retry policies
  - Add structured output validation
  - Add agent guardrails
  -> Double the dataset to ~8,000-9,000 examples

Step 5: Generate preference pairs
  For each query, generate 3-5 candidate workflows
  Score with nika check + semantic similarity
  Best = chosen, worst valid = rejected, invalid = hard negative
  -> ~5,000 preference pairs

Step 6: Train
  Phase A: SFT with QLoRA on Qwen3-8B/27B (4-8 hours on 1x A100)
  Phase B: SimPO on preference pairs (2-4 hours on 1x A100)

Step 7: Evaluate
  Hold out 500 test queries
  Metric 1: % of valid workflows (nika check passes)
  Metric 2: % of executable workflows (--dry-run passes)
  Metric 3: Semantic accuracy (does it match the intent?)
  Metric 4: Efficiency (minimal tasks, no redundancy)
```

### 6.4 Cost Estimate

| Item | Cost |
|------|------|
| Claude API for 10,000 workflow generations | ~$50-150 (Sonnet) |
| Claude API for preference pair generation | ~$30-80 |
| A100 80GB rental for training (24h) | ~$30-50 (Lambda, Vast.ai) |
| Evaluation and iteration (3 rounds) | ~$100-200 |
| **Total** | **~$200-500** |

---

## 7. Nika-Specific Architecture Decisions

### 7.1 System Prompt Strategy

The system prompt should encode the Nika schema compactly. Two approaches:

**Option A: Full schema in system prompt** (~2,000 tokens)
- Pros: Model always has reference
- Cons: Uses context window, may be ignored after fine-tuning

**Option B: Schema learned during fine-tuning, minimal system prompt**
- Pros: Efficient, schema knowledge is internalized
- Cons: Harder to update schema without retraining

**Recommendation**: Option B for the fine-tuned model, Option A for zero-shot use with base models. After SFT, the model should have the schema internalized.

### 7.2 Output Format

Train the model to output fenced YAML:
```
```yaml
schema: "nika/workflow@0.12"
...
```
```

This aligns with how users expect to copy-paste workflows and how the Nika TUI/CLI can detect and extract them.

### 7.3 Integration with Nika

The fine-tuned model could serve as:
1. **`provider: native`** -- Run locally via mistral.rs GGUF for workflow generation
2. **`nika new --ai "description"`** -- CLI command that generates a workflow from description
3. **LSP completions** -- Context-aware YAML completions in editor
4. **TUI Studio** -- Natural language to workflow in the Studio view

### 7.4 Evaluation Benchmark: NikaBench

Create a standardized benchmark:
- 200 natural language descriptions at 4 difficulty levels
- Gold-standard .nika.yaml files for each
- Automated scoring: `nika check` pass rate, AST similarity, execution success
- Human evaluation: 50 examples rated by Nika developers

---

## 8. Recommended Roadmap

### Phase 1: Data Collection (2 weeks)
- Catalog all existing Nika workflows (course + showcase + internal)
- Generate 5,000 synthetic training examples using Claude
- Validate with `nika check` and `nika run --provider mock`
- Create NikaBench evaluation set (200 examples)

### Phase 2: SFT Training (1 week)
- Base: Qwen2.5-32B-Instruct (or Qwen3 equivalent when available)
- Method: QLoRA (rank 64, alpha 128)
- Tool: LLaMA-Factory
- Hardware: 1x A100 80GB or 2x A6000 48GB
- Evaluate on NikaBench

### Phase 3: Preference Alignment (1 week)
- Generate 5,000 preference pairs (chosen vs. rejected workflows)
- Train with SimPO (reference-free, memory efficient)
- Evaluate improvement on NikaBench

### Phase 4: Deployment (1 week)
- Export to GGUF for `provider: native` usage
- Integrate into `nika new --ai` CLI command
- Publish model on HuggingFace under AGPL-compatible license
- Write documentation and blog post

### Total Timeline: 5-6 weeks
### Total Cost: ~$300-600 (API + compute)

---

## Sources

1. [Glaive Function Calling v2](https://huggingface.co/datasets/glaiveai/glaive-function-calling-v2) - 113k synthetic function calling examples
2. [Salesforce xLAM / APIGen](https://huggingface.co/datasets/Salesforce/xlam-function-calling-60k) - 60k verified function calling examples (arXiv:2406.18518)
3. [Berkeley Function Calling Leaderboard](https://gorilla.cs.berkeley.edu/leaderboard.html) - Primary benchmark for function calling
4. [Hermes 3 Technical Report](https://arxiv.org/abs/2408.11857) - NousResearch's training recipe for tool-use models
5. [NousResearch Hermes Function Calling v1](https://huggingface.co/datasets/NousResearch/hermes-function-calling-v1) - 1,890 function calling examples
6. [Hermes Function Calling Code](https://github.com/NousResearch/Hermes-Function-Calling) - Reference implementation for tool-use inference
7. [Qwen2.5-32B-Instruct](https://huggingface.co/Qwen/Qwen2.5-32B-Instruct) - Strong base model for fine-tuning
8. [LLaMA-Factory](https://github.com/hiyouga/LLaMA-Factory) - Unified fine-tuning framework (ACL 2024)
9. [DPO with TRL](https://huggingface.co/blog/dpo-trl) - Reference implementation for preference optimization
10. [SimPO](https://arxiv.org/abs/2405.14734) - Reference-free preference optimization (NeurIPS 2024)
11. [ORPO](https://arxiv.org/abs/2403.07691) - Monolithic preference optimization without reference model
12. [Preference Tuning Comparison](https://huggingface.co/blog/pref-tuning) - DPO vs. IPO vs. KTO empirical evaluation
13. [WizardCoder / Evol-Instruct](https://arxiv.org/abs/2306.08568) - Complexity escalation for synthetic data (ICLR 2024)

## Methodology

- **Tools used**: Jina Reader (web scraping), arxiv.org, HuggingFace Hub, GitHub
- **Pages analyzed**: 18 sources (model cards, papers, datasets, blog posts, repositories)
- **Time period covered**: 2023-2026 (DPO through SimPO era)

## Confidence Level

**High** - The approach is well-supported by multiple independent research efforts (Hermes, xLAM, Zephyr, Orca) that have successfully fine-tuned smaller models for tool use. Nika's constrained domain (5 verbs, 1 schema) makes the problem significantly easier than general-purpose function calling. The deterministic validation via `nika check` provides an unusually strong quality signal for both data filtering and reward modeling.

## Key Risk

The primary risk is not technical but strategic: maintaining the fine-tuned model as the Nika schema evolves. Each schema change (new verbs, new fields, new pipe transforms) requires retraining or at least supplemental fine-tuning. Mitigation: design the training pipeline to be fully automated and repeatable.

## Further Research Suggestions

1. **Investigate Qwen3 tool-use capabilities** when released -- it may have native workflow understanding that reduces the fine-tuning needed
2. **Explore constrained decoding** (grammar-guided generation) as a complement to fine-tuning -- enforce valid YAML at the token level
3. **Study ToolBench** (arXiv:2305.16504) for multi-step tool-use training data
4. **Consider RAFT** (Retrieval Augmented Fine-Tuning) to make the model schema-version-aware
5. **Benchmark against Claude/GPT-4 zero-shot** to measure the fine-tuning uplift objectively
