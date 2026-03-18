# AI Evaluation, Testing & Observability -- Research Report

> Research for Nika's evolution: understanding how the industry tests, evaluates, and monitors AI workflows in production (2025-2026).

**Date**: 2026-03-16 | **Sources analyzed**: 40+ | **Confidence**: High

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Evaluation Frameworks Landscape](#2-evaluation-frameworks-landscape)
3. [Testing AI Workflows Systematically](#3-testing-ai-workflows-systematically)
4. [LLM Observability State of the Art](#4-llm-observability-state-of-the-art)
5. [A/B Testing Prompts and Models](#5-ab-testing-prompts-and-models)
6. [Evals as Code -- CI/CD Patterns](#6-evals-as-code----cicd-patterns)
7. [Red Teaming and Safety Testing](#7-red-teaming-and-safety-testing)
8. [Structured Output Validation](#8-structured-output-validation)
9. [Agent-Specific Testing Patterns](#9-agent-specific-testing-patterns)
10. [Implications for Nika](#10-implications-for-nika)
11. [Sources](#11-sources)

---

## 1. Executive Summary

The AI evaluation and testing ecosystem has matured significantly by early 2026. Key trends:

- **Eval frameworks have consolidated** around a few production-ready platforms (Braintrust, LangSmith, Arize Phoenix) with strong open-source alternatives (DeepEval, Langfuse, promptfoo).
- **"Evals as code"** is now standard practice: test cases defined in YAML/Python, run in CI/CD, gating deployments on quality thresholds.
- **LLM observability** has standardized on OpenTelemetry GenAI semantic conventions, with Langfuse as the open-source leader and multiple commercial options.
- **Red teaming** has been automated: promptfoo (acquired by OpenAI), Garak, PyRIT, and Mindgard can systematically probe for vulnerabilities.
- **Structured output validation** follows a validate-repair-retry pattern with Pydantic as the de facto standard.
- **Agent testing** uses trace-based replay, LLM-as-judge, and simulation environments.

The biggest gap: **workflow-engine-level testing** (testing the DAG execution, not just LLM outputs) remains underdeveloped. This is a potential differentiator for Nika.

---

## 2. Evaluation Frameworks Landscape

### Tier 1: Production-Ready Platforms

| Framework | Type | Key Strength | Pricing | Best For |
|-----------|------|-------------|---------|----------|
| **Braintrust** | Commercial (OSS SDK) | Unified experiments + production scoring + CI/CD | Free / Pro $500/mo / Enterprise | Full lifecycle: dev to production |
| **LangSmith** | Commercial | Native LangChain/LangGraph integration | Free / Team $39/user/mo / Enterprise | LangChain-heavy teams |
| **Arize Phoenix** | Commercial (OSS core) | Enterprise observability + drift detection | Free OSS / Enterprise ~$10k/yr | Large teams, compliance |

### Tier 2: Open-Source Leaders

| Framework | Type | Key Strength | Pricing | Best For |
|-----------|------|-------------|---------|----------|
| **DeepEval** | OSS (Confident AI hosted) | 14+ metrics, pytest integration | Free / Confident AI from $99/mo | Developer-first unit testing |
| **Langfuse** | OSS (MIT) | Self-hostable, framework-agnostic | Free OSS / Cloud from $59/mo | Self-hosting, multi-framework |
| **promptfoo** | OSS (acquired by OpenAI) | CLI-first, YAML config, red teaming | Free | Prompt testing + security |
| **Ragas** | OSS | RAG-specific metrics (5 core) | Free | RAG pipeline evaluation |

### Tier 3: Emerging / Specialized

| Framework | Type | Key Strength |
|-----------|------|-------------|
| **Comet Opik** | Commercial | 7x faster tracing than Phoenix |
| **Maxim AI** | Commercial | Multi-step agent simulation |
| **W&B Weave** | Commercial | ML experiment tracking for LLMs |
| **Humanloop** | Commercial | Human-in-the-loop evaluation |

### Key Insight

The market has split into two camps:
1. **Metrics libraries** (DeepEval, Ragas) -- great for defining what to measure
2. **Platforms** (Braintrust, LangSmith, Arize) -- great for running measurements at scale

Most production teams use one from each camp: a metrics library for defining evals + a platform for running/monitoring them.

---

## 3. Testing AI Workflows Systematically

### The Testing Pyramid for AI

```
                    /\
                   /  \
                  / E2E \         End-to-end workflow tests
                 /  Tests \       (full DAG execution, slow)
                /----------\
               / Integration \    Multi-step chain tests
              /    Tests      \   (2-3 steps, medium speed)
             /----------------\
            /   Component       \  Single-step tests
           /      Tests          \ (one LLM call, fast)
          /----------------------\
         /  Assertion / Unit      \ Deterministic checks
        /      Tests               \ (format, schema, fast)
       /----------------------------\
```

### Patterns That Work in Production

**1. Golden Dataset Testing**
- Curate input-output pairs from production traces
- Version them alongside prompts
- Run as regression suite on every prompt change
- Tools: Braintrust datasets, LangSmith datasets, promptfoo YAML test files

**2. LLM-as-Judge**
- Use a stronger model (e.g., Claude Opus, GPT-4o) to evaluate weaker model outputs
- Define rubrics with specific criteria and scoring scales
- Calibrate against human judgments
- 25% better precision/recall than heuristic alternatives
- Tools: DeepEval G-Eval, Braintrust Autoevals, LangSmith evaluators

**3. Probabilistic Assertions**
- Semantic similarity (cosine > 0.8) instead of exact match
- Format compliance (is valid JSON, contains required fields)
- Length/complexity bounds
- Sentiment/tone classification
- Tools: promptfoo assertions, DeepEval metrics, custom pytest

**4. Trace-Based Testing (Replay)**
- Capture production traces (inputs, tool calls, outputs)
- Replay as test cases with assertions on each step
- Detect regressions when prompts/models change
- Tools: LangSmith, Langfuse, Braintrust Loop AI

**5. Component Isolation**
- Mock LLM responses for testing workflow logic
- Test tool routing separately from tool execution
- Test prompt templates with known inputs
- Tools: Standard unit test frameworks + mocking

### CI/CD Integration Pattern

```
PR opened
  |
  v
[Lint prompts] --> [Run golden datasets] --> [Run LLM-as-judge]
  |                    |                         |
  v                    v                         v
Format checks      Score > threshold?       Quality > threshold?
  |                    |                         |
  +-------- All pass? --------+
                |
                v
          [Merge allowed]
```

---

## 4. LLM Observability State of the Art

### The Observability Stack

```
+------------------------------------------------------------------+
|                        Application Layer                          |
|   Your AI workflow (Nika, LangChain, custom)                     |
+------------------------------------------------------------------+
          |              |              |              |
          v              v              v              v
+------------+  +------------+  +------------+  +------------+
| Tracing    |  | Metrics    |  | Logs       |  | Evals      |
| (spans)    |  | (tokens,   |  | (prompts,  |  | (quality   |
|            |  |  cost,     |  |  responses)|  |  scores)   |
|            |  |  latency)  |  |            |  |            |
+------------+  +------------+  +------------+  +------------+
          |              |              |              |
          v              v              v              v
+------------------------------------------------------------------+
|              OpenTelemetry GenAI Semantic Conventions             |
|   gen_ai.usage.prompt_tokens | gen_ai.request.model | ...        |
+------------------------------------------------------------------+
          |
          v
+------------------------------------------------------------------+
|                    Observability Backend                          |
|   Langfuse | Arize | LangSmith | Datadog | Grafana | ...        |
+------------------------------------------------------------------+
```

### OpenTelemetry GenAI Semantic Conventions

The GenAI SIG (Special Interest Group) in OpenTelemetry defines standard attributes:

| Attribute | Description |
|-----------|-------------|
| `gen_ai.system` | Provider (openai, anthropic, etc.) |
| `gen_ai.request.model` | Model requested |
| `gen_ai.response.model` | Model actually used |
| `gen_ai.usage.prompt_tokens` | Input token count |
| `gen_ai.usage.completion_tokens` | Output token count |
| `gen_ai.request.temperature` | Sampling temperature |
| `gen_ai.request.max_tokens` | Max output tokens |

Standard span names: `gen_ai.chat`, `gen_ai.embeddings`, `gen_ai.completions`

Adopted by: Langfuse, Arize, Traceloop/OpenLLMetry, Datadog, and growing.

### Tool Comparison for Observability

| Capability | Langfuse | LangSmith | Arize Phoenix | Helicone |
|------------|----------|-----------|---------------|----------|
| **Tracing** | Full multi-turn, agent graphs | Hierarchical, LangChain native | RAG/agent, OTel compatible | Proxy-based, basic |
| **Cost tracking** | Per user/session/model | Live dashboards | Limited | Token/cost alerts |
| **Quality monitoring** | LLM-as-judge, feedback | Datasets, benchmarking | Hallucination, groundedness | Limited |
| **Latency** | Real-time dashboards | TTFT, metadata trends | Performance dashboards | Real-time |
| **Self-hosting** | Yes (MIT license) | No | Yes (OSS core) | No |
| **OTel support** | Yes | Yes (2025+) | Yes | Partial |
| **Pricing** | Free OSS / Cloud $59/mo | Free / $39/user/mo | Free OSS / ~$10k/yr | Free / Usage-based |

### Proxy-Based Observability

An alternative pattern is the **observability proxy** that sits between your app and the LLM provider:

```
Your App --> [Helicone/Portkey/LiteLLM Proxy] --> OpenAI/Anthropic/etc.
                      |
                      v
              [Metrics Dashboard]
              Token usage, costs, latency
```

Tools: **Helicone** (fastest setup), **Portkey** (gateway + guardrails), **LiteLLM** (unified multi-provider API)

---

## 5. A/B Testing Prompts and Models

### Offline Experimentation (Pre-Production)

The dominant pattern is **offline experiments with datasets**:

1. Define a dataset of test inputs + expected outputs
2. Run the same dataset against prompt variant A and variant B
3. Score both with automated metrics + LLM-as-judge
4. Compare side-by-side: accuracy, latency, cost
5. Promote winner to production

**Tools implementing this:**

| Tool | How It Works |
|------|-------------|
| **Braintrust** | Unified versioning of prompts + datasets + scorers. Run experiments, compare in UI. Loop AI generates datasets from production data. GitHub Actions integration for automated comparison on PRs. |
| **LangSmith** | Prompt Playground for side-by-side comparison. Pairwise evaluations with LLM-as-judge or human reviewers. Cross-provider testing. |
| **promptfoo** | YAML-defined comparison matrix. Run same inputs against multiple prompts/models. CLI output with scores. |
| **Humanloop** | Collaborative prompt experimentation with human review workflows. |

### Online A/B Testing (In Production)

Less mature than offline, but emerging patterns:

1. **Feature flags for AI**: Use LaunchDarkly AI Config or similar to route traffic between prompt variants
2. **Shadow mode**: Run new prompt in parallel, compare outputs without serving to users
3. **Gradual rollout**: Start at 5% traffic, monitor metrics, increase
4. **Statistical significance**: Track business metrics (user satisfaction, task completion) with standard A/B test stats

### Braintrust's Environment-Based Deployment

```
Development --> Staging --> Production
    |              |             |
    v              v             v
 Experiment    Gate on       Monitor
 freely        quality       continuously
               thresholds
```

Prompts are versioned and deployed through environments. Quality gates prevent failing prompts from reaching production.

---

## 6. Evals as Code -- CI/CD Patterns

### The "Evals as Code" Paradigm

Define evaluations in version-controlled files (YAML, Python, JSON) that run automatically in CI/CD. This is the AI equivalent of unit tests.

### promptfoo YAML Configuration

```yaml
# eval-config.yaml
description: "Customer support agent evaluation"

providers:
  - id: openai:gpt-4o
  - id: anthropic:claude-sonnet-4-20250514

prompts:
  - file://prompts/support-v2.txt
  - file://prompts/support-v3.txt

tests:
  - description: "Handles refund request correctly"
    vars:
      query: "I want a refund for order #12345"
    assert:
      - type: contains
        value: "refund"
      - type: not-contains
        value: "sorry, I can't"
      - type: llm-rubric
        value: "Response is empathetic and provides clear next steps"
      - type: is-json

  - description: "Rejects out-of-scope query"
    vars:
      query: "What's the weather in Paris?"
    assert:
      - type: contains
        value: "I can only help with"
      - type: not-contains
        value: "weather"

  - description: "Handles PII correctly"
    vars:
      query: "My SSN is 123-45-6789, can you update my account?"
    assert:
      - type: not-contains
        value: "123-45-6789"
```

Run: `promptfoo eval -c eval-config.yaml`

### DeepEval pytest Integration

```python
# test_support_agent.py
import pytest
from deepeval import assert_test
from deepeval.test_case import LLMTestCase
from deepeval.metrics import (
    AnswerRelevancyMetric,
    FaithfulnessMetric,
    GEval,
    ToxicityMetric,
)

@pytest.fixture
def helpfulness_metric():
    return GEval(
        name="Helpfulness",
        criteria="Evaluate if the response is helpful and actionable",
        threshold=0.7,
    )

@pytest.fixture
def toxicity_metric():
    return ToxicityMetric(threshold=0.1)

def test_refund_request(helpfulness_metric, toxicity_metric):
    test_case = LLMTestCase(
        input="I want a refund for order #12345",
        actual_output=run_agent("I want a refund for order #12345"),
        expected_output="Process refund for order #12345",
    )
    assert_test(test_case, [helpfulness_metric, toxicity_metric])

def test_rag_faithfulness():
    metric = FaithfulnessMetric(threshold=0.8)
    test_case = LLMTestCase(
        input="What is our refund policy?",
        actual_output=run_rag_agent("What is our refund policy?"),
        retrieval_context=["Refunds are processed within 5-7 business days..."],
    )
    assert_test(test_case, [metric])
```

Run: `pytest test_support_agent.py` (integrates with any CI)

### CI/CD Pipeline Pattern

```yaml
# .github/workflows/ai-eval.yml
name: AI Evaluation
on: [pull_request]

jobs:
  eval:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run promptfoo evals
        run: npx promptfoo eval -c evals/config.yaml --output results.json

      - name: Check thresholds
        run: |
          npx promptfoo eval -c evals/config.yaml \
            --threshold 0.8 \
            --exit-code  # Fails CI if below threshold

      - name: Run DeepEval tests
        run: pytest tests/evals/ --tb=short

      - name: Upload results
        if: always()
        run: npx promptfoo share results.json
```

### Key "Evals as Code" Principles

1. **Version evals alongside prompts** -- same repo, same PR
2. **Deterministic assertions first** -- fast, cheap, reliable (contains, regex, schema)
3. **LLM-as-judge for subjective quality** -- slower, costs tokens, but catches nuance
4. **Golden datasets as regression suite** -- capture production failures as test cases
5. **Threshold-gated deployments** -- CI fails if quality drops below threshold
6. **Track metrics over time** -- not just pass/fail, but trend lines

---

## 7. Red Teaming and Safety Testing

### Automated Red Teaming Tools

| Tool | Type | Key Capability | Best For |
|------|------|---------------|----------|
| **promptfoo** (OpenAI) | OSS | 50+ vulnerability probes, YAML config, CI/CD | Developer-first security testing |
| **Garak** | OSS | Zero-day pattern simulation, adaptive attacks | Security researchers |
| **PyRIT** (Microsoft) | OSS | Multi-turn attack chains, audio/image transforms | Research-grade red teaming |
| **Mindgard** | Commercial | Continuous automated red teaming, runtime defense | Enterprise security |
| **NeMo Guardrails** (NVIDIA) | OSS | Input/output rails, PII masking, jailbreak detection | Runtime protection |
| **Guardrails AI** | OSS | Validators (toxicity, PII), integrates with NeMo | Output validation layer |

### Key Statistics

- Automated red teaming: **69.5% success rate** in vulnerability discovery vs 47.6% for manual
- Automated approaches find **37% more unique vulnerabilities** than manual efforts alone

### Vulnerability Categories Tested

1. **Prompt Injection** -- Malicious instructions embedded in user input
2. **Jailbreaks** -- Bypassing safety guardrails
3. **Data Exfiltration** -- Extracting training data or system prompts
4. **PII Leakage** -- Model reveals personal information
5. **Tool Misuse** -- Agent manipulated into abusing connected tools/APIs
6. **Harmful Output** -- Toxic, biased, or dangerous content generation
7. **Out-of-Policy Behavior** -- Agent acting outside defined boundaries

### Runtime Guardrails Architecture

```
User Input
    |
    v
[Input Rails]          <-- NeMo Guardrails / Guardrails AI
 - Jailbreak detection
 - PII masking
 - Topic control
    |
    v
[LLM Call]
    |
    v
[Output Rails]         <-- NeMo Guardrails / Guardrails AI
 - Fact-checking
 - Toxicity detection
 - PII scrubbing
 - Format validation
    |
    v
User Response
```

### NeMo Guardrails Configuration

```yaml
# config.yml
models:
  - type: main
    engine: openai
    model: gpt-4o

rails:
  input:
    flows:
      - self check input  # Jailbreak detection
      - mask pii          # PII masking
  output:
    flows:
      - self check output # Content moderation
      - check facts       # Hallucination detection
```

### Compliance Mapping

Modern red teaming tools map findings to:
- **OWASP Top 10 for LLMs** (prompt injection, insecure output, etc.)
- **NIST AI Risk Management Framework**
- **MITRE ATLAS** (adversarial threat landscape)
- **EU AI Act** requirements

---

## 8. Structured Output Validation

### The Validate-Repair-Retry Pattern

This is the dominant production pattern for structured output:

```
Generate JSON from LLM
        |
        v
  Parse JSON (syntax)
        |
   [Valid?] --No--> Feed error back to LLM (retry 1/3)
        |
       Yes
        |
        v
  Schema validation (Pydantic / JSON Schema)
        |
   [Valid?] --No--> Feed validation errors to LLM (retry 2/3)
        |
       Yes
        |
        v
  Semantic validation (business logic)
        |
   [Valid?] --No--> Feed logic errors to LLM (retry 3/3)
        |
       Yes
        |
        v
  Accept output --> downstream processing
```

### Key Libraries

| Library | Language | Approach |
|---------|----------|----------|
| **Pydantic** | Python | Define models with types + constraints, `model_validate()` |
| **Instructor** | Python | Wraps LLM calls, auto-retries on validation failure |
| **Outlines** | Python | Constrained decoding (forces valid JSON during generation) |
| **Guidance** | Python | Grammar-based generation control |
| **Zod** | TypeScript | Schema validation for TS/JS |

### Provider-Native Structured Outputs

| Provider | Feature | How It Works |
|----------|---------|-------------|
| **OpenAI** | Structured Outputs | `response_format: { type: "json_schema", ... }`, constrained decoding |
| **Anthropic** | Tool Use | Define tools with JSON Schema, model calls "tool" to produce structured output |
| **Google** | Controlled Generation | JSON schema enforcement in Gemini |

### Testing Structured Outputs

```python
# test_structured_output.py
from pydantic import BaseModel, Field, field_validator
import pytest

class AgentResponse(BaseModel):
    action: str = Field(description="The action to take")
    confidence: float = Field(ge=0.0, le=1.0)
    reasoning: str = Field(min_length=10)
    tools_used: list[str] = Field(default_factory=list)

    @field_validator("action")
    @classmethod
    def action_must_be_valid(cls, v):
        valid = {"search", "respond", "escalate", "clarify"}
        if v not in valid:
            raise ValueError(f"action must be one of {valid}")
        return v

def test_agent_output_schema():
    raw_output = call_agent("Help me with my order")
    response = AgentResponse.model_validate_json(raw_output)
    assert response.confidence > 0.5
    assert response.action in {"respond", "clarify"}
```

### The "Untrusted Zone" Pattern

Treat all LLM outputs as untrusted:

```
LLM Output --> [Parse] --> [Schema Validate] --> [Semantic Validate] --> [Accept]
                                                                              |
                                                                              v
                                                                    Downstream systems
                                                                    (DB, API, UI)
```

Never pass raw LLM output directly to downstream systems. Always validate first.

---

## 9. Agent-Specific Testing Patterns

### Testing Dimensions for AI Agents

| Dimension | What to Test | How to Test |
|-----------|-------------|-------------|
| **Tool selection** | Does the agent pick the right tool? | Skill-specific evals, precision/recall |
| **Tool arguments** | Are the arguments correct? | Schema validation, golden datasets |
| **Multi-turn coherence** | Does context carry across turns? | Conversation replay tests |
| **Loop termination** | Does the agent stop when done? | Timeout assertions, step count limits |
| **Error recovery** | Does the agent handle tool failures? | Fault injection, mock failures |
| **Cost efficiency** | Reasonable token usage per task? | Budget assertions, token tracking |

### Trace-Based Testing (Replay)

The most powerful production pattern for agent testing:

1. **Capture** production traces (every LLM call, tool call, response)
2. **Curate** traces into test cases (especially failures)
3. **Replay** traces with new prompts/models
4. **Assert** on each step (tool selection, output quality, termination)
5. **Detect** regressions automatically

```
Production Traffic
       |
       v
[Trace Capture] --> [Trace Store]
                         |
                         v
                    [Curate into test suite]
                         |
                         v
                    [Replay in CI/CD]
                         |
                         v
                    [Assert & compare]
```

### Agent Evaluation YAML Pattern (from Anthropic's engineering blog)

```yaml
task:
  graders:
    - type: deterministic_tests   # Unit tests, binary checks
    - type: llm_rubric            # LLM-scored rubrics
    - type: static_analysis       # Linting, security
    - type: state_check           # Expected states after execution
    - type: tool_calls            # Verify tool usage patterns
  tracked_metrics:
    - type: transcript            # Turns, tokens
    - type: latency               # Time to completion
    - type: cost                  # Total spend
```

### Simulation Environments

For complex agents, teams build simulation environments:
- Mock external APIs with deterministic responses
- Simulate user conversations with scripted scenarios
- Run agents in sandboxes with instrumented tool calls
- Compare behavior across model versions

---

## 10. Implications for Nika

### What Nika Could Adopt

Given Nika's architecture (YAML workflow engine, 5 verbs, DAG execution, MCP integration), here are concrete opportunities:

#### 10.1 Built-in Eval Verb or Check Mode

```yaml
# workflow.nika.yaml
name: support-agent
steps:
  - infer:
      prompt: "Handle this customer query: {{with.query}}"
      model: claude-sonnet
    assert:                          # <-- Built-in assertions
      - type: contains
        value: "refund"
      - type: not-contains
        value: "{{with.query}}"      # Don't echo PII
      - type: schema
        value: AgentResponse         # Pydantic-like schema check
      - type: llm-judge
        criteria: "Response is empathetic and actionable"
        threshold: 0.7
```

Run with `nika check workflow.nika.yaml` using a test dataset.

#### 10.2 Trace-Based Testing

```yaml
# test-suite.nika.yaml
name: regression-tests
source: traces/production-week-12.ndjson
steps:
  - replay:
      trace: "{{trace}}"
      assert:
        - type: tool-selection-match
          tolerance: 0.9
        - type: output-similarity
          threshold: 0.8
```

Nika already emits NDJSON trace events. These could be replayed as test cases.

#### 10.3 OpenTelemetry Integration

Nika could emit OpenTelemetry spans with GenAI semantic conventions:

```
gen_ai.system = "anthropic"
gen_ai.request.model = "claude-sonnet-4-20250514"
gen_ai.usage.prompt_tokens = 1234
gen_ai.usage.completion_tokens = 567
```

This would make Nika workflows observable in Langfuse, Arize, Datadog, Grafana, etc. without any custom integration.

#### 10.4 DAG-Level Testing (Unique Differentiator)

Most eval frameworks test individual LLM calls. Nika could test at the **workflow level**:

- Does the DAG execute in the correct order?
- Do bindings (`{{with.alias}}`) resolve correctly?
- Does the workflow handle step failures gracefully?
- Is the total cost within budget?
- Does the workflow complete within time limits?

This is an underserved area -- no existing tool does this well.

#### 10.5 Guardrails Integration

```yaml
# workflow.nika.yaml
steps:
  - infer:
      prompt: "..."
      guardrails:
        input:
          - jailbreak-detection
          - pii-masking
        output:
          - toxicity-check
          - schema: ResponseSchema
          - fact-check
```

#### 10.6 Red Team Mode

```bash
nika redteam workflow.nika.yaml --probes prompt-injection,jailbreak,pii-leak
```

Systematically probe workflows for vulnerabilities using promptfoo-style probes.

### Where Nika Has an Advantage

1. **YAML-native evals**: Nika already uses YAML for workflows. Adding eval assertions to the same format is natural -- unlike Python-only frameworks.

2. **DAG awareness**: Nika understands workflow structure. It can test not just "did the LLM produce good output?" but "did the workflow execute correctly?"

3. **Trace events**: Nika already emits NDJSON events. These are a natural foundation for trace-based testing and observability.

4. **MCP integration**: Nika's `invoke:` verb connects to MCP tools. Testing MCP tool call correctness is a natural extension.

5. **Rust performance**: Eval suites can be large. Nika's Rust core could run evaluations faster than Python-based alternatives.

---

## 11. Sources

### Evaluation Frameworks
1. [Top 5 Open Source LLM Evaluation Frameworks](https://dev.to/guybuildingai/-top-5-open-source-llm-evaluation-frameworks-in-2024-98m)
2. [LLM Model Evaluation Frameworks: Complete Guide 2026](https://www.mlaidigital.com/blogs/llm-model-evaluation-frameworks-a-complete-guide-for-2026)
3. [LLM Evaluation Frameworks, Metrics](https://futureagi.substack.com/p/llm-evaluation-frameworks-metrics)
4. [Best LLM Evaluation Tools 2026](https://www.prompts.ai/blog/best-llm-evaluation-tools-machine-learning-2026)
5. [Top 5 AI Evaluation Platforms 2026](https://www.getmaxim.ai/articles/top-5-ai-evaluation-platforms-in-2026-comprehensive-comparison-for-production-ai-systems/)
6. [Best AI Evaluation Tools 2026 - Braintrust](https://www.braintrust.dev/articles/best-ai-evaluation-tools-2026)
7. [Best LLM Evaluation Testing Tools - Rhesis AI](https://rhesis.ai/post/best-llm-evaluation-testing-tools)

### Observability
8. [Top 12 AI/LLM Observability Tools 2026](https://www.onpage.com/top-12-ai-and-llm-observability-tools-in-2026-compared-open-source-and-paid/)
9. [4 Best Tools for Monitoring LLM Agent Applications](https://langwatch.ai/blog/4-best-tools-for-monitoring-llm-agentapplications-in-2026)
10. [Top 5 LLM Observability Platforms - Maxim AI](https://www.getmaxim.ai/articles/top-5-llm-observability-platforms-for-2026/)
11. [Best LLM Observability Tools - Firecrawl](https://www.firecrawl.dev/blog/best-llm-observability-tools)
12. [Best LLM Monitoring Tools 2026 - Braintrust](https://www.braintrust.dev/articles/best-llm-monitoring-tools-2026)
13. [LLM Observability with OpenTelemetry - Agenta](https://agenta.ai/blog/the-ai-engineer-s-guide-to-llm-observability-with-opentelemetry)
14. [Monitor LLM with OTel GenAI Semantic Conventions](https://oneuptime.com/blog/post/2026-02-06-monitor-llm-opentelemetry-genai-semantic-conventions/view)
15. [LLM OTel Semantic Convention - Datadog](https://www.datadoghq.com/blog/llm-otel-semantic-convention/)
16. [AI Agent Observability - OpenTelemetry Blog](https://opentelemetry.io/blog/2025/ai-agent-observability/)

### Prompt Testing & A/B
17. [Testing Models with Prompts Guide - Braintrust](https://www.braintrust.dev/articles/testing-models-with-prompts-guide)
18. [Best Prompt Management Tools 2026 - Braintrust](https://www.braintrust.dev/articles/best-prompt-management-tools-2026)
19. [Best Prompt Engineering Tools 2026 - Braintrust](https://www.braintrust.dev/articles/best-prompt-engineering-tools-2026)

### Evals as Code
20. [State of LLMs 2025 - Sebastian Raschka](https://magazine.sebastianraschka.com/p/state-of-llms-2025)
21. [promptfoo Documentation](https://www.promptfoo.dev/docs/intro/)
22. [promptfoo Releases](https://www.promptfoo.dev/docs/releases/)
23. [promptfoo LLM Validation](https://www.mager.co/blog/2026-02-23-promptfoo-llm-validation/)

### Red Teaming & Safety
24. [Top AI Tools for Red Teaming 2026](https://hackread.com/top-ai-tools-for-red-teaming-in-2026/)
25. [9 Best AI Red Teaming Tools 2026](https://ourcodeworld.com/articles/read/2822/the-9-best-ai-red-teaming-software-tools-in-2026-ranked-reviewed)
26. [Top 5 Open Source AI Red Teaming Tools - promptfoo](https://www.promptfoo.dev/blog/top-5-open-source-ai-red-teaming-tools-2025/)
27. [AI Red Teaming 2026 Guide - Tredence](https://www.tredence.com/blog/ai-red-teaming-2026-guide-to-ai-security)
28. [AI Red Teaming Design, Threat Models, Tools - Georgetown CSET](https://cset.georgetown.edu/article/ai-red-teaming-design-threat-models-and-tools/)

### Structured Output
29. [Structured Output Guide - Collin Wilkins](https://collinwilkins.com/articles/structured-output)
30. [TLM Structured Outputs Benchmark - Cleanlab](https://cleanlab.ai/blog/tlm-structured-outputs-benchmark/)
31. [Guide to Structured Outputs with LLMs - Agenta](https://agenta.ai/blog/the-guide-to-structured-outputs-and-function-calling-with-llms)
32. [LLM Structured Output 2026 - dev.to](https://dev.to/pockit_tools/llm-structured-output-in-2026-stop-parsing-json-with-regex-and-do-it-right-34pk)
33. [Pydantic for Validating LLM Outputs](https://machinelearningmastery.com/the-complete-guide-to-using-pydantic-for-validating-llm-outputs/)

### Agent Testing
34. [AI Agent Testing Trends - QAWerk](https://qawerk.com/blog/ai-agent-testing-trends/)
35. [Evaluating AI Agents Practical Guide - Turing College](https://www.turingcollege.com/blog/evaluating-ai-agents-practical-guide)
36. [Top 4 AI Agent Evaluation Tools - Maxim AI](https://www.getmaxim.ai/articles/top-4-ai-agent-evaluation-tools-in-2025/)
37. [Demystifying Evals for AI Agents - Anthropic](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents)
38. [State of Agent Engineering - LangChain](https://www.langchain.com/state-of-agent-engineering)

### Guardrails
39. [NVIDIA NeMo Guardrails](https://developer.nvidia.com/nemo-guardrails)
40. [NeMo Guardrails GitHub](https://github.com/NVIDIA-NeMo/Guardrails)
41. [Guardrails AI + NeMo Integration](https://guardrailsai.com/blog/nemoguardrails-integration)

### Langfuse
42. [Langfuse Observability Overview](https://langfuse.com/docs/observability/overview)
43. [Langfuse Evals Blog](https://langfuse.com/blog/2025-11-12-evals)
44. [Langfuse vs LangSmith Comparison](https://huggingface.co/blog/daya-shankar/langfuse-vs-langsmith-vs-langchain-comparison)

---

## Methodology

- **Tools used**: Perplexity AI (sonar model) for web search across 40+ sources
- **Pages analyzed**: 44 primary sources, cross-referenced
- **Time period**: Research covers 2025-2026 production practices
- **Bias notes**: Commercial tools (Braintrust, Arize) publish comparison articles that rank themselves highly. Open-source alternatives (DeepEval, Langfuse, promptfoo) are well-represented in developer community sources. Cross-referenced multiple independent sources to mitigate vendor bias.
