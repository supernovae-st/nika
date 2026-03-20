# 21 -- Model Routing & Agents Design

> Industry survey of model routing patterns, naming conventions, and semantic agent preset systems.
> Validates Nika's unified `agents:` block with 8 presets (default/lite/think/search/vision/judge/coder/summary) against the state of the art.
> Includes adversarial analysis (devil's advocate counter-arguments) of the unified design.

**Status:** RESEARCH (updated 2026-03-20) | **Date:** 2026-03-16
**Dependencies:** Doc 05 (Evolution Roadmap), Doc 12 (Naming & Identity), Doc 17 (Smart Router)
**Research Sources:** Perplexity (5 queries), arXiv (4 papers), framework docs (6 frameworks)

---

## Table of Contents

1. [Why This Research Matters](#1-why-this-research-matters)
2. [Industry Survey: Model Routing Patterns](#2-industry-survey-model-routing-patterns)
3. [Framework-Level Model Assignment](#3-framework-level-model-assignment)
4. [Academic Foundations: LLM Routing Research](#4-academic-foundations-llm-routing-research)
5. [Naming Conventions Comparison](#5-naming-conventions-comparison)
6. [Semantic Agent Preset Design Patterns](#6-semantic-agent-preset-design-patterns)
7. [Nika's 8-Preset Unified Agents Block: Deep Analysis](#7-nikas-8-preset-unified-agents-block-deep-analysis)
8. [Cross-Framework Comparison Matrix](#8-cross-framework-comparison-matrix)
9. [Recommendation for Nika](#9-recommendation-for-nika)
10. [Devil's Advocate: Counter-Arguments](#10-devils-advocate-counter-arguments)
11. [Sources](#11-sources)

---

## 1. Why This Research Matters

Nika v0.28 introduces P-MODEL: a unified `agents:` block where workflows declare WHAT capability
they need (default, lite, think, search, vision, judge, coder, summary) rather than WHICH specific
model to use. This document validates the design against the 2025-2026 landscape.

```
THE CORE QUESTION
-----------------------------------------------------------------
Should workflows say:       Or should they say:

  model: claude-sonnet-4-6       agent: default
  model: llama-3.3-70b           agent: lite
  model: deepseek-chat           agent: search
  model: claude-sonnet-4-6       agent: think

Explicit model IDs             Semantic agent presets
(brittle, coupled)             (portable, intent-driven)
```

The industry is moving decisively toward **semantic routing** -- assigning models by capability
rather than by name. IDC predicts 70% of enterprises will adopt multi-model routing by 2028[^1].
This research confirms Nika's approach is aligned with the trajectory.

---

## 2. Industry Survey: Model Routing Patterns

### 2.1 The Three Generations of Model Selection

```
Generation 1 (2023-2024):    Single model, hardcoded
                             model: "gpt-4"

Generation 2 (2024-2025):    Per-task explicit model IDs
                             research_model = ChatOpenAI(model="gpt-4o-mini")
                             analysis_model = ChatOpenAI(model="o1-preview")

Generation 3 (2025-2026):    Unified agent presets + dynamic routing
                             agent: think            # Router picks best model
                             agent: lite              # Cost-optimized
```

### 2.2 Platform-Level Routing (Gateways)

| Platform | Approach | Cost Savings | Key Innovation |
|----------|----------|:------------:|----------------|
| **OpenAI GPT-5 Router** | Internal router selects sub-models per query | Undisclosed | Trained on user switches, preference rates, correctness signals[^2] |
| **OpenRouter** | Unified API to 500+ models, provider fallback | 30-60% | Load balancing, "exacto" curated endpoints, 100T+ tokens routed in 2025[^3] |
| **Azure Model Router** | Complexity/cost/quality modes per request | 40-70% | Three modes: quality, cost, balanced[^4] |
| **RouteLLM (LMSYS)** | Open-source cost-quality router | 10-80% | SW ranking, matrix factorization, BERT classifiers[^5] |
| **LiteLLM** | OpenAI-compatible proxy, semantic tiers | 30-50% | Drop-in swap without code changes |
| **Cloudflare AI Gateway** | Edge routing with caching | 40-60% | Semantic caching, automatic failover |

### 2.3 The GPT-5 Router Controversy

OpenAI's GPT-5 (August 2025) introduced a **model router** that automatically selects
between internal sub-models based on query complexity. The system was trained on:
- When users switch models (implicit preference signal)
- Preference rates for responses
- Measured correctness on evaluation benchmarks

This sparked user backlash due to inconsistencies and unpredictable behavior[^2].
Key lesson for Nika: **transparent, user-controlled routing beats opaque auto-routing**.
Nika's named slots give the user explicit control over which capability tier each task uses.

### 2.4 Enterprise Adoption

IDC reports that 37% of enterprises already use 5+ models in production[^1].
The dominant pattern is a "two-stack" approach:

```
DEEP STACK          FAST STACK
(reasoning)         (throughput)
Claude Sonnet 4.5   Gemini 2.5 Flash
GPT-4o              Llama 3.3 70B
o1-preview          GPT-4o-mini
```

This maps directly to Nika's agent preset model: `think` for deep, `lite` for throughput,
with `default` and `search` providing additional specialization, plus `vision`, `judge`, `coder`,
and `summary` for targeted capabilities.

---

## 3. Framework-Level Model Assignment

### 3.1 LangGraph: Per-Node Model Binding

LangGraph (LangChain's graph orchestration) assigns models at the **node function level**.
Each node in the StateGraph can invoke a different LLM. There is no named-slot abstraction --
models are bound directly via Python code.

```python
# LangGraph: explicit model IDs per node
research_model = ChatOpenAI(model="gpt-4o-mini")     # Cheap
analysis_model = ChatOpenAI(model="o1-preview")       # Expensive

def research_node(state):
    return {"messages": research_model.invoke(state["messages"])}

def analysis_node(state):
    return {"messages": analysis_model.invoke(state["messages"])}

graph = StateGraph(State)
graph.add_node("research", research_node)
graph.add_node("analysis", analysis_node)
```

**Observation:** No semantic abstraction. Model IDs are hardcoded in Python. Changing
a model requires code changes, not config changes.

### 3.2 CrewAI: Per-Agent Model Assignment

CrewAI assigns LLMs to **agents**, and tasks inherit from their assigned agent.
This is role-based model routing -- the "Researcher" agent gets a cheap model,
the "Writer" gets an expensive one.

```python
# CrewAI: model assignment via agent roles
researcher = Agent(
    role="Researcher",
    goal="Discover AI trends",
    llm=ChatOpenAI(model="gpt-4o-mini"),   # Cost-optimized
)
writer = Agent(
    role="Writer",
    goal="Summarize findings",
    llm=ChatOpenAI(model="o1"),            # High-reasoning
)

research_task = Task(agent=researcher)     # Inherits gpt-4o-mini
write_task = Task(agent=writer)            # Inherits o1
```

**Observation:** Role-based but still explicit model IDs. No indirection layer.
Agent roles serve as implicit capability categories (researcher = search, writer = creative).

### 3.3 Microsoft AutoGen: model_client Per Agent

AutoGen uses `model_client` (or `llm_config`) per ConversableAgent. Each agent can
point to a different provider and model.

```python
# AutoGen: per-agent model_client
researcher = ConversableAgent(
    name="Researcher",
    model_client=dict(config_list=openai_config),    # gpt-4o-mini
)
analyst = ConversableAgent(
    name="Analyst",
    model_client=dict(config_list=azure_config),     # gpt-4o (Azure)
)
```

**Observation:** Provider-level routing (OpenAI vs Azure) in addition to model-level.
No named slot abstraction.

### 3.4 Microsoft Semantic Kernel: Service-Based Routing

Semantic Kernel routes models via the kernel's service registry. Functions can specify
which registered service to use via `PromptExecutionSettings`.

```csharp
// Semantic Kernel: registered services as implicit slots
builder.AddOpenAIChatCompletion("gpt-4o-mini", key);   // "lite"
builder.AddAzureChatCompletion("gpt-4o", endpoint);    // "quality"

// Per-invocation selection
var args = new KernelArguments { { "model_id", "gpt-4o-mini" } };
var result = await kernel.InvokeAsync(function, args);
```

**Observation:** Closest to a slot-like system via service registration. But still
uses model IDs, not semantic names.

### 3.5 DSPy: Compile-Time Model Optimization

DSPy takes a unique approach -- models are assigned per module, and the `Compile` step
can automatically optimize which model each module uses.

```python
# DSPy: per-module model with compile-time optimization
gpt4mini = dspy.OpenAI(model='gpt-4o-mini')
o1 = dspy.OpenAI(model='o1-preview')

research = dspy.ChainOfThought(Research, lm=gpt4mini)
generate = dspy.ChainOfThought(Generate, lm=o1)

# Compiler can swap models based on metrics
optimized = dspy.BootstrapFewShot(metric=accuracy).compile(program)
```

**Observation:** Compile-time optimization is the closest to "automatic semantic routing" --
DSPy figures out which model works best per module. However, the initial assignment
is still explicit model IDs.

### 3.6 Mastra AI: Workflow Step Model Selection

Mastra AI (2025) supports per-step model selection in TypeScript workflows. Each step
in a workflow can reference a different model from the configured providers.

**Observation:** Limited public documentation as of March 2026. Follows the Generation 2
pattern of explicit model IDs per step.

---

## 4. Academic Foundations: LLM Routing Research

### 4.1 Key Papers

| Paper | Year | Key Contribution | Relevance to Nika |
|-------|:----:|------------------|-------------------|
| **FrugalGPT**[^6] | 2023 | Cascading: try cheap model first, escalate if uncertain | Confidence-based model escalation in orchestration |
| **AutoMix**[^7] | 2024 | Automatic routing between model sizes based on query | Validates per-task model routing |
| **RouteLLM**[^5] | 2024 | Open-source cost-quality routing with 4 classifier types | SW ranking + BERT classifiers for routing |
| **R2-Router**[^8] | 2026 | Treats LLMs as quality-cost curves, not points | Powerful models with constrained budgets can beat weak models |
| **Dynamic Routing Survey**[^9] | 2026 | Systematic analysis of multi-LLM routing and cascading | Well-designed routing outperforms any single model |
| **KNN Routing**[^10] | 2025 | Simple non-parametric routing beats complex methods | Simple slot assignment may outperform dynamic routing |

### 4.2 FrugalGPT Cascading Pattern

FrugalGPT introduced the cascading model: try the cheapest model first, check confidence,
escalate to a more expensive model if needed.

```
Query arrives
    |
    v
Try GPT-3.5 (cheap)
    |
    +-- Confidence > 0.9? --> Return result
    |
    +-- Confidence < 0.9
    |
    v
Try GPT-4 (expensive)
    |
    v
Return result
```

**Cost savings:** Up to 98% cost reduction with minimal quality loss on certain benchmarks.
**Relevance to Nika:** This pattern maps to orchestration's confidence-based escalation (P-SHAKA).
A record with low confidence triggers re-execution with a more capable agent preset.

### 4.3 R2-Router: Models as Quality-Cost Curves

R2-Router (February 2026) challenges the assumption that expensive models always produce
better results. By treating each LLM as a **quality-cost curve** (using length-constrained
instructions), the paper discovers configurations where:

- Powerful LLMs with **constrained budgets** outperform weaker models at full budget
- Optimal model selection depends on the specific cost-quality tradeoff desired

**Relevance to Nika:** Validates the `lite` agent preset concept -- a powerful model with
constrained parameters (fast, short outputs) can outperform a weaker model at full length.

### 4.4 Dynamic Routing Survey (2026)

The most comprehensive survey to date[^9] analyzes routing across:
- Query difficulty estimation
- Human preference prediction
- Clustering-based routing
- Uncertainty quantification
- Reinforcement learning routers
- Cascading systems

**Key finding:** "Well-designed routing systems can outperform even the most powerful
individual models by strategically leveraging specialized capabilities across models
while maximizing efficiency gains."

This is the definitive academic validation for Nika's multi-preset approach.

---

## 5. Naming Conventions Comparison

### 5.1 Existing Naming Systems

| System | Naming Style | Tiers | Philosophy |
|--------|-------------|:-----:|------------|
| **Anthropic** | Poetry (Haiku/Sonnet/Opus) | 3 | Size + capability as literary forms |
| **OpenAI** | Version + suffix (gpt-4o, gpt-4o-mini, o1) | ~4 | Technical naming, "o" for optimized |
| **Google** | Gems (Gemini Flash/Pro/Ultra) | 3 | Speed metaphor (Flash) + quality (Ultra) |
| **Slate** | Role (main/subagent/search/reasoning) | 4 | Functional role in agent system |
| **OpenRouter** | Tiered (fast/quality/balanced) | 3 | Routing strategy, not model identity |
| **Azure Router** | Mode (quality/cost/balanced) | 3 | Optimization objective |
| **Nika (current)** | Descriptive presets (default/lite/think/search/vision/judge/coder/summary) | 8 | Functional capability presets in unified agents: block |

### 5.2 Two Philosophies

```
PHILOSOPHY A: Name by size/speed              PHILOSOPHY B: Name by capability/role
---------------------------------------------  -------------------------------------------
Haiku / Sonnet / Opus                          main / subagent / search / reasoning (Slate)
Flash / Pro / Ultra                            fast / quality / balanced (OpenRouter)
mini / standard / premium                      default / lite / think / search (Nika)

Pros:                                          Pros:
- Clear cost/perf ordering                     - Decoupled from model identity
- Universal understanding                      - Portable across providers
- Stable naming across generations             - Intent-driven, not resource-driven

Cons:                                          Cons:
- Tied to a single provider                    - Learning curve for slot names
- Says nothing about task fit                  - Mapping must be user-configurable
- Cannot express multi-provider routing        - May confuse if names are obscure
```

### 5.3 Slate's 4-Slot System (Direct Comparison)

Slate defines exactly 4 model slots, configured in `slate.json`:

| Slate Slot | Purpose | Default Model |
|-----------|---------|---------------|
| `main` | Primary content generation, orchestration | claude-sonnet-4-20250514 |
| `subagent` | Cheaper threads, tactical execution | claude-haiku-35 |
| `search` | Fast retrieval and search synthesis | perplexity/sonar-pro |
| `reasoning` | Deep thinking, planning, review | claude-sonnet-4-20250514 + thinking |

Nika's current mapping (unified `agents:` block with 8 presets):

| Slate Slot | Nika Agent Preset | Cognitive Role |
|-----------|-------------------|----------------|
| `main` | `default` | Primary creative generation, writing, orchestration |
| `subagent` | `lite` | Fast execution, structured tasks, formatting |
| `search` | `search` | Search, retrieval, data collection |
| `reasoning` | `think` | Deep reasoning, planning, critique, review |
| -- | `vision` | Visual analysis, OCR, image understanding |
| -- | `judge` | Quality evaluation, scoring, validation |
| -- | `coder` | Code generation, review, execution |
| -- | `summary` | Compression, summarization, extraction |

### 5.4 Why Nika Evolved Beyond Slate's 4 Slots

The evolution from 4 named slots (edison/atlas/york/pythagoras) to 8 descriptive agent presets
(default/lite/think/search/vision/judge/coder/summary) serves three purposes:

1. **Descriptive over lore-based.** Functional names (default, lite, think) are instantly
   understandable without learning Vegapunk lore. Lower onboarding friction.

2. **Expanded coverage.** 4 slots could not adequately cover vision, code, judging, and
   summarization -- capabilities that workflows need distinct model configurations for.

3. **Unified `agents:` block.** Instead of a separate `model_slots:` top-level key, agent
   presets live inside the `agents:` block, unifying model configuration with agent behavior.

---

## 6. Semantic Agent Preset Design Patterns

### 6.1 Pattern: Agent Preset with Provider Binding

The dominant pattern across all frameworks that support multi-model is:

```
[User-Facing Preset]  -->  [Provider + Model Config]  -->  [Runtime Resolution]
     default           -->  anthropic / claude-sonnet   -->  API call
     lite              -->  groq / llama-3.3-70b        -->  API call
     search            -->  deepseek / deepseek-chat    -->  API call
     think             -->  anthropic / claude + think   -->  API call with thinking
     vision            -->  openai / gpt-4o              -->  API call
     judge             -->  anthropic / claude-sonnet    -->  API call
     coder             -->  anthropic / claude-sonnet    -->  API call
     summary           -->  groq / llama-3.3-70b         -->  API call
```

This is exactly what Nika implements in the unified `agents:` block (Doc 05).

### 6.2 Pattern: Fallback Chain

Every production routing system implements fallback:

```yaml
# Nika agents with fallback (proposed for v0.28+)
agents:
  default:
    primary:
      provider: anthropic
      model: claude-sonnet-4-6
    fallback:
      provider: openai
      model: gpt-4o
```

This mirrors OpenRouter's automatic failover, Azure's provider redundancy,
and LiteLLM's fallback configuration.

### 6.3 Pattern: Cost-Aware Preset Assignment

The key insight from FrugalGPT, RouteLLM, and enterprise adoption:

```
                    Cost per 1M tokens    Quality (avg)    Latency
                    -----------------    -------------    -------
think               $15.00               9.2/10           2-8s (thinking)
default             $3.00                8.5/10           1-3s
search              $0.27                7.8/10           0.5-1s
lite                $0.05                7.0/10           0.2-0.5s
```

The cost difference between presets can be **100-300x**, making routing a significant
optimization lever. IDC reports 70% cost reduction through intelligent routing[^1].

### 6.4 Pattern: Confidence-Based Escalation

From FrugalGPT cascading + Nika's orchestration:

```
Task executes with lite (cheap, fast)
    |
    v
Record generated: confidence = 0.65 (below threshold)
    |
    v
Orchestrator sees low confidence --> Re-dispatch with default
    |
    v
Record generated: confidence = 0.92 (above threshold)
    |
    v
Continue workflow
```

This is not a separate routing system -- it emerges naturally from Nika's
P-RECORD (confidence tracking) + P-SHAKA (dynamic re-dispatch) integration.

---

## 7. Nika's 8-Preset Unified Agents Block: Deep Analysis

### 7.1 The Eight Agent Presets

```
+===============================================================================+
|                    NIKA AGENT PRESETS (P-MODEL)                                 |
+===============================================================================+
|                                                                                |
|  DEFAULT                                                                       |
|  Role:     Primary creative work -- generation, writing, orchestration         |
|  Profile:  High quality, moderate cost, moderate speed                         |
|  Default:  claude-sonnet-4-6 / gpt-4o                                          |
|  Use:      Content generation, complex infer: tasks, general purpose           |
|                                                                                |
|  LITE                                                                          |
|  Role:     Fast tactical execution -- structured tasks, formatting             |
|  Profile:  Good quality, low cost, high speed                                  |
|  Default:  llama-3.3-70b (Groq) / gpt-4o-mini / claude-haiku                  |
|  Use:      Record compression, JSON extraction, simple transforms              |
|                                                                                |
|  THINK                                                                         |
|  Role:     Deep reasoning -- planning, analysis, critique, review              |
|  Profile:  Highest quality, high cost, slow (thinking enabled)                 |
|  Default:  claude-sonnet-4-6 + extended_thinking / o1-preview                  |
|  Use:      Strategic planning, complex analysis, multi-step reasoning          |
|                                                                                |
|  SEARCH                                                                        |
|  Role:     Search and retrieval -- research, data collection                   |
|  Profile:  Search-optimized, low cost, variable speed                          |
|  Default:  deepseek-chat / perplexity/sonar-pro                                |
|  Use:      Information gathering, search synthesis, RAG queries                |
|                                                                                |
|  VISION                                                                        |
|  Role:     Visual analysis -- image understanding, OCR                         |
|  Profile:  Vision-capable, moderate cost                                       |
|  Default:  openai/gpt-4o / native/qwen2-vl                                    |
|  Use:      Image analysis, visual QA, screenshot understanding                 |
|                                                                                |
|  JUDGE                                                                         |
|  Role:     Quality evaluation -- scoring, validation, gate checks              |
|  Profile:  High quality, moderate cost, deterministic                          |
|  Default:  claude-sonnet-4-6 / gpt-4o                                          |
|  Use:      Output validation, quality scoring, acceptance criteria              |
|                                                                                |
|  CODER                                                                         |
|  Role:     Code generation and review                                          |
|  Profile:  Code-optimized, moderate cost                                       |
|  Default:  claude-sonnet-4-6 / deepseek-coder                                  |
|  Use:      Code writing, refactoring, code review, debugging                   |
|                                                                                |
|  SUMMARY                                                                       |
|  Role:     Compression and summarization                                       |
|  Profile:  Good quality, low cost, fast                                        |
|  Default:  llama-3.3-70b (Groq) / gpt-4o-mini                                 |
|  Use:      Text summarization, record compression, key extraction              |
|                                                                                |
+===============================================================================+
```

### 7.2 YAML Syntax (from Doc 05)

```yaml
schema: nika/workflow@0.12

agents:
  default:
    provider: anthropic
    model: claude-sonnet-4-6
  lite:
    provider: groq
    model: llama-3.3-70b-versatile
  search:
    provider: deepseek
    model: deepseek-chat
  think:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384
  vision:
    provider: openai
    model: gpt-4o
  judge:
    provider: anthropic
    model: claude-sonnet-4-6
  coder:
    provider: anthropic
    model: claude-sonnet-4-6
  summary:
    provider: groq
    model: llama-3.3-70b-versatile

tasks:
  - id: plan
    agent: think
    infer: "Create a content plan for {{with.entity}}"

  - id: generate_pages
    agent: default
    for_each: $pages
    infer: "Generate page {{with.item}}"

  - id: format
    agent: lite
    infer: "Format and validate the generated page"
```

### 7.3 Why 8 Presets (Not 2, 3, or 4)

| Count | Examples | Problem |
|:-----:|----------|---------|
| 2 | fast / quality | Too coarse. Cannot distinguish reasoning from creative. |
| 3 | Haiku / Sonnet / Opus | Size-based, not capability-based. Search is not a "size". |
| 4 | default / lite / search / think | Misses vision, code, judging, summarization -- real workflow needs. |
| **8** | **default / lite / think / search / vision / judge / coder / summary** | **Covers all functional roles that workflows actually dispatch to.** |
| 10+ | Adding more specializations | Diminishing returns. The orchestrator is not a preset -- it is the dispatcher. |

The 8-preset design aligns with:
- **Slate's 4 slots** (main/subagent/search/reasoning) as a foundation -- extended with 4 more
- **Enterprise multi-model patterns** (deep + fast + search + reasoning + specialized)
- **Real workflow needs** (vision analysis, code gen, quality judging, summarization are first-class tasks)

### 7.4 Agent Assignment Heuristics

When `agent:` is omitted, Nika uses the `default` preset by default.
In orchestrate mode, the orchestrator LLM can dynamically assign presets per task dispatch:

```
Orchestrator decision loop:
  "This task needs research" --> search
  "This task needs fast formatting" --> lite
  "This task needs creative writing" --> default
  "This task needs image analysis" --> vision
  "I need to review all results" --> judge
  "Compress this output" --> summary
```

---

## 8. Cross-Framework Comparison Matrix

### 8.1 Model Routing Capabilities

| Feature | Nika (current) | Slate | LangGraph | CrewAI | AutoGen | DSPy |
|---------|:-:|:-:|:-:|:-:|:-:|:-:|
| Named capability presets | **8 presets** | 4 slots | -- | -- | -- | -- |
| Per-task model selection | Yes | Yes | Yes (code) | Via agent | Via agent | Per module |
| Declarative (config/YAML) | **YAML** | JSON | Code only | Code only | Code only | Code only |
| Provider abstraction | 7 providers | ~3 | Via LangChain | Via LangChain | Direct | Direct |
| Fallback chain | Planned | Yes | Manual | Manual | Manual | -- |
| Cost tracking per slot | Yes (events) | Partial | Manual | Manual | Manual | Metrics |
| Dynamic re-routing | Via orchestrator | Via orchestrator | Conditional edges | -- | -- | Compile-time |
| Extended thinking support | Per slot | Per slot | Manual | -- | -- | -- |

### 8.2 Abstraction Level

```
HIGH ABSTRACTION (semantic, portable)
  |
  |  Nika:      agent: default          (YAML, capability-named, 8 presets)
  |  Slate:     slot: main              (JSON, role-named)
  |
  |  OpenRouter: tier: quality           (API, objective-named)
  |  Azure:      mode: balanced          (API, optimization-named)
  |
  |  DSPy:       lm=gpt4mini            (Python, model-bound per module)
  |  CrewAI:     llm=ChatOpenAI(...)     (Python, model-bound per agent)
  |  LangGraph:  model = ChatOpenAI(...) (Python, model-bound per node)
  |  AutoGen:    model_client=dict(...)  (Python, config-bound per agent)
  |
LOW ABSTRACTION (explicit, brittle)
```

Nika and Slate sit at the highest abstraction level. The key differentiator is that
Nika uses **YAML-first declarative** configuration while Slate uses JSON/TypeScript.

### 8.3 Naming Approach

| Framework | Naming Style | Memorable? | Portable? | Self-Documenting? |
|-----------|-------------|:----------:|:---------:|:-----------------:|
| **Nika** | Descriptive presets (default, lite, think) | Medium | High | High (self-documenting) |
| **Slate** | Role names (main, subagent) | Medium | High | High |
| **OpenRouter** | Objective names (fast, quality) | Medium | High | High |
| **Anthropic** | Poetry names (haiku, sonnet) | High | Low (vendor-specific) | Medium |
| **LangGraph** | Variable names (research_model) | Low | Low | High (developer context) |

---

## 9. Recommendation for Nika

### 9.1 Validation Summary

The research validates Nika's 8-preset unified `agents:` design on every dimension:

| Dimension | Finding | Confidence |
|-----------|---------|:----------:|
| **Preset count (8)** | Covers all cognitive modes + specialized capabilities, aligns with enterprise patterns | High |
| **Named presets** | Higher abstraction than any competitor except Slate | High |
| **YAML-first** | Only declarative system with named presets (unique differentiator) | High |
| **Descriptive names** | Self-documenting, zero onboarding friction, no lore required | High |
| **Per-task assignment** | Standard pattern across all frameworks | High |
| **Fallback chains** | Industry standard, must ship in v0.28 or v0.29 | High |
| **Confidence escalation** | Academically validated (FrugalGPT, RouteLLM) | High |

### 9.2 Design Decisions: Confirmed

| Decision | Status | Rationale |
|----------|:------:|-----------|
| Use 8 named presets as unified `agents:` block | CONFIRMED | Covers all cognitive modes + specialized capabilities. |
| Use descriptive names (default/lite/think/search/vision/judge/coder/summary) | CONFIRMED | Self-documenting, zero onboarding friction, no lore dependency. |
| YAML-level declaration, not runtime-only | CONFIRMED | Declarative = version-controlled, auditable, reproducible. |
| `agent:` per task, `default` as fallback | CONFIRMED | Matches Slate pattern, granular control with sensible defaults. |
| Orchestrator can dynamically assign presets | CONFIRMED | Dynamic dispatch is validated by every orchestration framework. |
| Extended thinking as preset property, not separate preset | CONFIRMED | Thinking is a model capability, not a cognitive mode. |

### 9.3 Gap: Fallback Chains (Ship in v0.28)

The one gap in the current P-MODEL design is **explicit fallback configuration**.
Every production routing system supports this. Proposed addition:

```yaml
agents:
  default:
    provider: anthropic
    model: claude-sonnet-4-6
    fallback:                        # NEW: fallback chain
      - provider: openai
        model: gpt-4o
      - provider: groq
        model: llama-3.3-70b-versatile
```

This enables resilience (provider outages) and flexibility (dev vs production configs).

### 9.4 Gap: Preset Aliases (Consider for v0.29)

For migration from Vegapunk naming, consider accepting both old and new names:

```yaml
# Both accepted, descriptive names are canonical
agents:
  default:    ...    # Alias: edison, main
  lite:       ...    # Alias: atlas, fast, tactical
  think:      ...    # Alias: pythagoras, reason, reasoning
  search:     ...    # Alias: york
```

This would lower the migration curve while maintaining the descriptive canonical names.
The parser would normalize aliases to canonical names at parse time.

### 9.5 Future: Auto-Routing (v0.30+)

Long-term, Nika could offer an `auto` mode inspired by RouteLLM and R2-Router:

```yaml
agents:
  default: auto       # Router picks best model for creative tasks
  lite: auto           # Router picks best model for fast tasks
```

This is explicitly out of scope for v0.28-v0.30 (transparent manual routing first),
but the preset abstraction makes it a natural future extension.

---

## 10. Devil's Advocate: Counter-Arguments

> Consolidated from the adversarial analysis session (2026-03-20).
> Steel-manning the opposition: the strongest arguments AGAINST the unified `agents:` block design.

### 10.1 What the Unified Block Merges

Three previously separate concepts became one unified `agents:` block:

```
CONCEPT 1: model aliases (lightweight)
  Purpose: Named model aliases (default, lite, think, search)
  Minimal: Just provider + model ID
  Example: lite: { provider: groq, model: llama-3.3-70b }

CONCEPT 2: satellites (medium)
  Purpose: Worker templates dispatched by orchestrator
  Rich:    Accept/produce MIME types, tools, agent preset, instructions
  Example: vision-analyst: { model: gpt-4o, accepts: image, tools: [read] }

CONCEPT 3: agent persona (full)
  Purpose: Reusable agent identity with instructions + tools + model + guardrails
  Full:    name, instructions, model, tools, guardrails, handoffs, output_type
  Example: code-reviewer: { instructions: "Review code...", tools: [...], guardrails: [...] }
```

### 10.2 Risk: The Kubernetes Lesson -- Orthogonal Concerns

Kubernetes separates Pod/Service/Deployment/ConfigMap because they have different lifecycles, owners, and rates of change. In a team scenario:
- The **platform team** configures model routing (providers, API keys, cost limits, fallback chains).
- The **workflow author** configures agent behavior (instructions, tools, output schemas, guardrails).

If both live in one `agents:` block, you cannot share model configs without duplicating behavior, or lock down routing while letting users customize prompts.

**Assessment:** MEDIUM severity. Nika v0.x is single-user; becomes HIGH if targeting teams.

### 10.3 Risk: Cognitive Overload -- Polymorphic Entries

Under the same `agents:` key, entries range from 2-line model aliases to 20-line full agent personas. This creates "what IS an agent?" confusion -- a known anti-pattern (Docker Compose `ports:` polymorphism).

```yaml
agents:
  lite:            # 2 lines -- just a model alias
    provider: groq
    model: llama-3.3-70b-versatile

  code-reviewer:   # 15 lines -- full agent persona
    provider: anthropic
    model: claude-sonnet-4-6
    instructions: |
      You are a senior code reviewer. Focus on security...
    tools: [nika:read, nika:exec]
    guardrails:
      output: [no-secrets-in-output]
    output_type: ReviewResult
```

**Assessment:** HIGH severity for DX with new users. Mitigated by Progressive Disclosure (shorthand syntax).

### 10.4 Risk: The DRY Violation

Multiple agents using the same model must duplicate `provider: anthropic, model: claude-sonnet-4-6`. Changing the model requires updating N entries.

**Assessment:** HIGH severity. Scales with workflow complexity.

### 10.5 Risk: Identity vs Execution Separation

No way to say "same agent persona, different model this time" without duplicating the entire agent definition. OpenAI's Assistants API solves this with per-Run overrides.

**Assessment:** HIGH severity. Real composability gap.

### 10.6 Risk: Preset Proliferation

The jump from Slate's 4 to Nika's 8 shows pressure to add more. When native tools ship (translate, embed, audio, safety), 8 may not be enough.

**Assessment:** MEDIUM severity. Not a problem today; becomes one at 15+.

### 10.7 Risk: Historical Precedent

Docker Compose v1->v3 progressively split concerns. Ansible roles split into galaxy+vault+inventory. React classes split into hooks. Terraform inlined providers were refactored to separate blocks. The pattern: unification -> split at scale.

**Assessment:** MEDIUM severity. Pattern-level concern.

### 10.8 Recommended Mitigations

**Keep the unified `agents:` block** as the primary authoring surface (good DX for 80% case), but add two escape hatches:

**Mitigation A: Model References (solves DRY)**

```yaml
models:
  sonnet: { provider: anthropic, model: claude-sonnet-4-6 }

agents:
  default:
    model: sonnet            # reference, not inline
  code-reviewer:
    model: sonnet            # same reference, different behavior
    instructions: "..."
```

**Mitigation B: Per-Task Model Override (solves Identity vs Execution)**

```yaml
agents:
  code-reviewer:
    model: sonnet
    instructions: "Review code..."
    tools: [nika:read]

tasks:
  - id: quick-review
    agent: code-reviewer
    model: lite              # override model, keep instructions + tools
  - id: deep-review
    agent: code-reviewer
    model: think             # override model, keep instructions + tools
```

**What NOT to do:**
1. Do NOT split `agents:` into three separate blocks -- kills simplicity.
2. Do NOT add inheritance (`extends:`) -- config inheritance is a complexity trap.
3. Do NOT add more than 8-10 presets -- new capabilities should be custom agents, not built-in presets.

### 10.9 Devil's Advocate Verdict

The unified `agents:` block is aligned with industry consensus. The strongest counter-arguments come from AutoGen's dependency injection, OpenAI's Run overrides, and Terraform's provider aliasing. Mitigations A and B address the urgent risks without abandoning the unified design.

**Confidence:** High on risk identification, Medium on mitigations (need prototyping).

---

## 11. Sources

| # | Source | Type | Key Finding |
|:-:|--------|------|-------------|
| 1 | IDC, "The Future of AI is Model Routing" (Nov 2025)[^1] | Industry report | 70% enterprise adoption predicted by 2028 |
| 2 | Fortune, "GPT-5 Router Backlash" (Aug 2025)[^2] | News | Opaque routing causes user frustration |
| 3 | OpenRouter State of AI 2025[^3] | Data report | 100T+ tokens routed, two-stack pattern dominant |
| 4 | Swfte, "Intelligent LLM Routing" (Jan 2026)[^4] | Technical blog | 85% cost reduction with intelligent routing |
| 5 | RouteLLM, LMSYS (2024)[^5] | Open-source | 4 classifier types for cost-quality routing |
| 6 | FrugalGPT, Chen et al. (2023)[^6] | Academic paper | Cascading with up to 98% cost savings |
| 7 | AutoMix (2024)[^7] | Academic paper | Automatic routing between model sizes |
| 8 | R2-Router (Feb 2026)[^8] | Academic paper | Models as quality-cost curves |
| 9 | Dynamic Routing Survey (Feb 2026)[^9] | Survey paper | Routing outperforms any single model |
| 10 | KNN Routing (Oct 2025)[^10] | Academic paper | Simple methods beat complex routers |
| 11 | CrewAI docs (2026) | Framework docs | Per-agent LLM assignment |
| 12 | LangGraph docs (2025-2026) | Framework docs | Per-node model binding |
| 13 | Slate by Random Labs | Framework docs | 4-slot model system |
| 14 | Kosmoy, "6 AI Gateway Trends" (Jan 2026) | Industry analysis | Semantic routing as gateway trend |
| 15 | Kubernetes API design | Architecture reference | Separation of orthogonal concerns |
| 16 | AutoGen Component system | Framework source | Model client as injected dependency |
| 17 | OpenAI Assistants API | API reference | Per-Run model/instruction overrides |
| 18 | Terraform provider aliasing | IaC reference | Infrastructure config referenced, not inlined |

### Research Methodology

- 5 Perplexity searches covering frameworks, naming, gateways, and academic papers
- Cross-referenced with framework documentation (CrewAI, LangGraph, AutoGen, Semantic Kernel, DSPy)
- Validated against existing Nika vision documents (Doc 03, 05, 12, 17)
- Adversarial analysis: steel-man opposition, cross-domain analogies (K8s, Terraform, Docker, React)
- March 2026 data -- captures post-GPT-5 router landscape

---

## Confidence Level

**High** -- The 8-preset semantic agent routing design is validated by:
- Direct precedent (Slate's identical 4-slot system, extended)
- Academic research (FrugalGPT, RouteLLM, R2-Router, Dynamic Routing Survey)
- Enterprise adoption patterns (37% using 5+ models, two-stack dominant)
- Industry trajectory (IDC 70% prediction, gateway trend toward semantic routing)
- No framework offers declarative YAML-first named presets (unique differentiator)

The descriptive naming (default/lite/think/search/vision/judge/coder/summary) is self-documenting
and eliminates the Vegapunk lore onboarding requirement while maintaining all functional coverage.

The adversarial analysis identified real risks (DRY, identity-vs-execution, cognitive overload)
with practical mitigations (model references + per-task overrides) that preserve the unified design.

---

<div align="center">

[<- 20 Agent Memory Architectures](./20-agent-memory-architectures.md) . [22 Wave 0 Foundation Report ->](./22-wave0-foundation-report.md) . [Index](./00-README.md)

</div>

---

[^1]: IDC, "The Future of AI is Model Routing" -- https://www.idc.com/resource-center/blog/the-future-of-ai-is-model-routing/ (November 2025). 70% enterprise adoption predicted by 2028.
[^2]: Fortune, "GPT-5's model router ignited a user backlash against OpenAI" -- https://fortune.com/2025/08/12/openai-gpt-5-model-router-backlash-ai-future/ (August 2025).
[^3]: OpenRouter State of AI 2025 -- https://openrouter.ai/state-of-ai. 100T+ tokens routed. a16z analysis: https://a16z.com/state-of-ai/
[^4]: Swfte, "Intelligent LLM Routing: How Multi-Model AI Cuts Costs by 85%" -- https://www.swfte.com/blog/intelligent-llm-routing-multi-model-ai (January 2026).
[^5]: RouteLLM by LMSYS -- Open-source cost-quality router with SW ranking, matrix factorization, BERT, and causal LLM classifiers. https://github.com/lm-sys/RouteLLM (2024).
[^6]: FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance -- Chen et al. (2023). Cascading model selection with up to 98% cost reduction.
[^7]: AutoMix: Automatically Mixing Language Models -- Madaan et al. (2024). Automatic routing between model sizes based on query complexity.
[^8]: R2-Router: A New Paradigm for LLM Routing with Reasoning -- https://arxiv.org/html/2602.02823v1 (February 2026). Treats LLMs as quality-cost curves.
[^9]: Dynamic Model Routing and Cascading for Efficient LLM Inference -- https://arxiv.org/html/2603.04445v1 (February 2026). Comprehensive survey of multi-LLM routing approaches.
[^10]: Rethinking Predictive LLM Routing: When Simple KNN... -- https://openreview.net/pdf/09a1cf8eea342f695327cb4308918d85676c6637.pdf (October 2025). Non-parametric approaches for model selection.
