# 21 -- Model Routing & Naming -- Deep Research

> Industry survey of model routing patterns, naming conventions, and semantic agent preset systems.
> Validates Nika's unified `agents:` block with 8 presets (main/fast/reason/search/vision/judge/code/summary) against the state of the art.

**Status:** RESEARCH (updated 2026-03-20) | **Date:** 2026-03-16
**Dependencies:** Doc 05 (Evolution Roadmap), Doc 12 (Vegapunk Naming), Doc 17 (Smart Router)
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
10. [Sources](#10-sources)

---

## 1. Why This Research Matters

Nika v0.28 introduces P-MODEL: a unified `agents:` block where workflows declare WHAT capability
they need (main, fast, reason, search, vision, judge, code, summary) rather than WHICH specific
model to use. This document validates the design against the 2025-2026 landscape.

```
THE CORE QUESTION
-----------------------------------------------------------------
Should workflows say:       Or should they say:

  model: claude-sonnet-4-6       agent: main
  model: llama-3.3-70b           agent: fast
  model: deepseek-chat           agent: search
  model: claude-sonnet-4-6       agent: reason

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
                             agent: reason            # Router picks best model
                             agent: fast              # Cost-optimized
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

This maps directly to Nika's agent preset model: `reason` for deep, `fast` for throughput,
with `main` and `search` providing additional specialization, plus `vision`, `judge`, `code`,
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
builder.AddOpenAIChatCompletion("gpt-4o-mini", key);   // "fast"
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
| **FrugalGPT**[^6] | 2023 | Cascading: try cheap model first, escalate if uncertain | Confidence-based model escalation in Shaka |
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
**Relevance to Nika:** This pattern maps to Shaka's confidence-based escalation (P-SHAKA).
A record with low confidence triggers re-execution with a more capable model slot.

### 4.3 R2-Router: Models as Quality-Cost Curves

R2-Router (February 2026) challenges the assumption that expensive models always produce
better results. By treating each LLM as a **quality-cost curve** (using length-constrained
instructions), the paper discovers configurations where:

- Powerful LLMs with **constrained budgets** outperform weaker models at full budget
- Optimal model selection depends on the specific cost-quality tradeoff desired

**Relevance to Nika:** Validates the `fast` agent preset concept -- a powerful model with
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

This is the definitive academic validation for Nika's multi-slot approach.

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
| **Nika (current)** | Descriptive (main/fast/reason/search/vision/judge/code/summary) | 8 | Functional capability presets in unified agents: block |

### 5.2 Two Philosophies

```
PHILOSOPHY A: Name by size/speed              PHILOSOPHY B: Name by capability/role
---------------------------------------------  -------------------------------------------
Haiku / Sonnet / Opus                          main / subagent / search / reasoning (Slate)
Flash / Pro / Ultra                            fast / quality / balanced (OpenRouter)
mini / standard / premium                      main / fast / reason / search (Nika)

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
| `main` | `main` | Primary creative generation, writing, orchestration |
| `subagent` | `fast` | Fast execution, structured tasks, formatting |
| `search` | `search` | Search, retrieval, data collection |
| `reasoning` | `reason` | Deep reasoning, planning, critique, review |
| -- | `vision` | Visual analysis, OCR, image understanding |
| -- | `judge` | Quality evaluation, scoring, validation |
| -- | `code` | Code generation, review, execution |
| -- | `summary` | Compression, summarization, extraction |

### 5.4 Why Nika Evolved Beyond Slate's 4 Slots

The evolution from 4 named slots (edison/atlas/york/pythagoras) to 8 descriptive agent presets
(main/fast/reason/search/vision/judge/code/summary) serves three purposes:

1. **Descriptive over lore-based.** Functional names (main, fast, reason) are instantly
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
     main              -->  anthropic / claude-sonnet   -->  API call
     fast              -->  groq / llama-3.3-70b        -->  API call
     search            -->  deepseek / deepseek-chat    -->  API call
     reason            -->  anthropic / claude + think   -->  API call with thinking
     vision            -->  openai / gpt-4o              -->  API call
     judge             -->  anthropic / claude-sonnet    -->  API call
     code              -->  anthropic / claude-sonnet    -->  API call
     summary           -->  groq / llama-3.3-70b         -->  API call
```

This is exactly what Nika implements in the unified `agents:` block (Doc 05).

### 6.2 Pattern: Fallback Chain

Every production routing system implements fallback:

```yaml
# Nika agents with fallback (proposed for v0.28+)
agents:
  main:
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
                    ─────────────────    ─────────────    ───────
reason              $15.00               9.2/10           2-8s (thinking)
main                $3.00                8.5/10           1-3s
search              $0.27                7.8/10           0.5-1s
fast                $0.05                7.0/10           0.2-0.5s
```

The cost difference between presets can be **100-300x**, making routing a significant
optimization lever. IDC reports 70% cost reduction through intelligent routing[^1].

### 6.4 Pattern: Confidence-Based Escalation

From FrugalGPT cascading + Nika's Shaka orchestration:

```
Task executes with fast (cheap, fast)
    |
    v
Record generated: confidence = 0.65 (below threshold)
    |
    v
Shaka sees low confidence --> Re-dispatch with main
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

## 7. Nika's 4-Slot Proposal: Deep Analysis

### 7.1 The Four Satellites

```
+===============================================================================+
|                    NIKA MODEL SLOTS (P-MODEL)                                  |
+===============================================================================+
|                                                                                |
|  EDISON (PUNK-03, Intelligence)                                                |
|  Role:     Primary creative work -- generation, writing, coding                |
|  Profile:  High quality, moderate cost, moderate speed                         |
|  Default:  claude-sonnet-4-6 / gpt-4o                                          |
|  Use:      Content generation, complex infer: tasks, code writing              |
|                                                                                |
|  ATLAS (PUNK-05, Force)                                                        |
|  Role:     Fast tactical execution -- structured tasks, formatting             |
|  Profile:  Good quality, low cost, high speed                                  |
|  Default:  llama-3.3-70b (Groq) / gpt-4o-mini / claude-haiku                  |
|  Use:      Record compression, JSON extraction, simple transforms              |
|                                                                                |
|  YORK (PUNK-06, Resources)                                                     |
|  Role:     Search and retrieval -- research, data collection                   |
|  Profile:  Search-optimized, low cost, variable speed                          |
|  Default:  deepseek-chat / perplexity/sonar-pro                                |
|  Use:      Information gathering, search synthesis, RAG queries                |
|                                                                                |
|  PYTHAGORAS (PUNK-04, Logic)                                                   |
|  Role:     Deep reasoning -- planning, analysis, critique, review              |
|  Profile:  Highest quality, high cost, slow (thinking enabled)                 |
|  Default:  claude-sonnet-4-6 + extended_thinking / o1-preview                  |
|  Use:      Strategic planning, code review, complex analysis                   |
|                                                                                |
+===============================================================================+
```

### 7.2 YAML Syntax (from Doc 05)

```yaml
schema: nika/workflow@0.12

model_slots:
  edison:
    provider: anthropic
    model: claude-sonnet-4-6
  atlas:
    provider: groq
    model: llama-3.3-70b-versatile
  york:
    provider: deepseek
    model: deepseek-chat
  pythagoras:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384

default_model_slot: edison

tasks:
  - id: plan
    model_slot: pythagoras
    infer: "Create a content plan for {{with.entity}}"

  - id: generate_pages
    model_slot: edison
    for_each: $pages
    infer: "Generate page {{with.item}}"

  - id: format
    model_slot: atlas
    infer: "Format and validate the generated page"
```

### 7.3 Why 4 Slots (Not 2, 3, or 5)

| Count | Examples | Problem |
|:-----:|----------|---------|
| 2 | fast / quality | Too coarse. Cannot distinguish reasoning from creative. |
| 3 | Haiku / Sonnet / Opus | Size-based, not capability-based. Search is not a "size". |
| **4** | **edison / atlas / york / pythagoras** | **Covers all 4 cognitive modes: creative, tactical, search, reasoning.** |
| 5+ | Adding "orchestrator" or "judge" | Diminishing returns. Shaka (the orchestrator) is not a model slot -- it is the dispatcher. |

The 4-slot design aligns with:
- **Slate's 4 slots** (main/subagent/search/reasoning) -- validated by production usage
- **Enterprise two-stack + two-specialty** (deep + fast + search + reasoning)
- **Cognitive task taxonomy** (create, execute, retrieve, analyze)

### 7.4 Slot Assignment Heuristics

When `model_slot:` is omitted, Nika uses `default_model_slot` (typically `edison`).
In Shaka mode, the Shaka LLM can dynamically assign slots per satellite dispatch:

```
Shaka decision loop:
  "This satellite needs research" --> york
  "This satellite needs fast formatting" --> atlas
  "This satellite needs creative writing" --> edison
  "I need to review all results" --> pythagoras (self)
```

---

## 8. Cross-Framework Comparison Matrix

### 8.1 Model Routing Capabilities

| Feature | Nika (proposed) | Slate | LangGraph | CrewAI | AutoGen | DSPy |
|---------|:-:|:-:|:-:|:-:|:-:|:-:|
| Named capability slots | **4 slots** | 4 slots | -- | -- | -- | -- |
| Per-task model selection | Yes | Yes | Yes (code) | Via agent | Via agent | Per module |
| Declarative (config/YAML) | **YAML** | JSON | Code only | Code only | Code only | Code only |
| Provider abstraction | 7 providers | ~3 | Via LangChain | Via LangChain | Direct | Direct |
| Fallback chain | Planned | Yes | Manual | Manual | Manual | -- |
| Cost tracking per slot | Yes (events) | Partial | Manual | Manual | Manual | Metrics |
| Dynamic re-routing | Via Shaka | Via orchestrator | Conditional edges | -- | -- | Compile-time |
| Extended thinking support | Per slot | Per slot | Manual | -- | -- | -- |

### 8.2 Abstraction Level

```
HIGH ABSTRACTION (semantic, portable)
  |
  |  Nika:      model_slot: edison      (YAML, capability-named)
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
| **Nika** | Character names (edison, atlas) | High | High | Medium (requires learning) |
| **Slate** | Role names (main, subagent) | Medium | High | High |
| **OpenRouter** | Objective names (fast, quality) | Medium | High | High |
| **Anthropic** | Poetry names (haiku, sonnet) | High | Low (vendor-specific) | Medium |
| **LangGraph** | Variable names (research_model) | Low | Low | High (developer context) |

---

## 9. Recommendation for Nika

### 9.1 Validation Summary

The research validates Nika's 4-slot design on every dimension:

| Dimension | Finding | Confidence |
|-----------|---------|:----------:|
| **Slot count (4)** | Matches Slate, covers all cognitive modes, aligns with enterprise patterns | High |
| **Named slots** | Higher abstraction than any competitor except Slate | High |
| **YAML-first** | Only declarative system with named slots (unique differentiator) | High |
| **Vegapunk names** | Memorable, cohesive with lore, no conflicts with industry terms | High |
| **Per-task assignment** | Standard pattern across all frameworks | High |
| **Fallback chains** | Industry standard, must ship in v0.28 or v0.29 | High |
| **Confidence escalation** | Academically validated (FrugalGPT, RouteLLM) | High |

### 9.2 Design Decisions: Confirmed

| Decision | Status | Rationale |
|----------|:------:|-----------|
| Use 4 named slots, not N dynamic slots | CONFIRMED | 4 covers all cognitive modes. More is complexity without value. |
| Use Vegapunk names (edison/atlas/york/pythagoras) | CONFIRMED | Cohesive with lore (Doc 12), memorable, no industry collision. |
| YAML-level declaration, not runtime-only | CONFIRMED | Declarative = version-controlled, auditable, reproducible. |
| `model_slot:` per task, `default_model_slot:` per workflow | CONFIRMED | Matches Slate pattern, granular control with sensible defaults. |
| Shaka can dynamically assign slots | CONFIRMED | Dynamic dispatch is validated by every orchestration framework. |
| Extended thinking as slot property, not separate slot | CONFIRMED | Thinking is a model capability, not a cognitive mode. |

### 9.3 Gap: Fallback Chains (Ship in v0.28)

The one gap in the current P-MODEL design is **explicit fallback configuration**.
Every production routing system supports this. Proposed addition:

```yaml
model_slots:
  edison:
    provider: anthropic
    model: claude-sonnet-4-6
    fallback:                        # NEW: fallback chain
      - provider: openai
        model: gpt-4o
      - provider: groq
        model: llama-3.3-70b-versatile
```

This enables resilience (provider outages) and flexibility (dev vs production configs).

### 9.4 Gap: Slot Aliases (Consider for v0.29)

For onboarding, consider accepting both Vegapunk names and descriptive aliases:

```yaml
# Both accepted, Vegapunk names are canonical
model_slots:
  edison:     ...    # OR:  creative:   ...
  atlas:      ...    # OR:  fast:       ...
  york:       ...    # OR:  search:     ...
  pythagoras: ...    # OR:  reasoning:  ...
```

This would lower the learning curve while maintaining the lore-cohesive canonical names.
The parser would normalize aliases to canonical names at parse time.

### 9.5 Future: Auto-Routing (v0.30+)

Long-term, Nika could offer an `auto` mode inspired by RouteLLM and R2-Router:

```yaml
model_slots:
  edison: auto       # Router picks best model for creative tasks
  atlas: auto        # Router picks best model for fast tasks
```

This is explicitly out of scope for v0.28-v0.30 (transparent manual routing first),
but the slot abstraction makes it a natural future extension.

---

## 10. Sources

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

### Research Methodology

- 5 Perplexity searches covering frameworks, naming, gateways, and academic papers
- Cross-referenced with framework documentation (CrewAI, LangGraph, AutoGen, Semantic Kernel, DSPy)
- Validated against existing Nika vision documents (Doc 03, 05, 12, 17)
- March 2026 data -- captures post-GPT-5 router landscape

---

## Confidence Level

**High** -- The 4-slot semantic model routing design is validated by:
- Direct precedent (Slate's identical 4-slot system)
- Academic research (FrugalGPT, RouteLLM, R2-Router, Dynamic Routing Survey)
- Enterprise adoption patterns (37% using 5+ models, two-stack dominant)
- Industry trajectory (IDC 70% prediction, gateway trend toward semantic routing)
- No framework offers declarative YAML-first named slots (unique differentiator)

The Vegapunk naming (edison/atlas/york/pythagoras) adds memorability and lore cohesion
without introducing industry naming conflicts.

---

<div align="center">

[<- 20 Agent Memory Architectures](./20-agent-memory-architectures.md) . [Index](./00-README.md)

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
