---
name: nika-infer
description: >-
  Expert at the Nika infer: verb for LLM generation in .nika.yaml workflows.
  Covers prompt engineering, system prompts, temperature tuning, max_tokens,
  vision/multimodal content, structured JSON output, provider selection, and
  model choice. Use when building or debugging infer: tasks in Nika YAML
  workflows (schema nika/workflow@0.12).
---

# Nika infer: Verb Expert

The `infer:` verb sends a prompt to an LLM and returns the generated text.

## Syntax

### Short Form (prompt only)

```yaml
- id: ask
  infer: "What is the capital of France?"
```

### Long Form (with options)

```yaml
- id: ask
  infer:
    prompt: "Explain quantum computing"
  system: "You are a physics professor"
  provider: openai
  model: gpt-4.1
  temperature: 0.3
  max_tokens: 500
```

### Vision / Multimodal

```yaml
- id: describe
  infer:
    content:
      - type: image
        source: "{{with.photo.media[0].hash}}"  # CAS hash
        detail: high                              # low | high | auto
      - type: text
        text: "Describe this image in detail"
  provider: openai
  model: gpt-4o
```

When `content:` is present, `prompt:` is optional. If both are given, the prompt is prepended as the first text part.

Vision-capable providers: claude, openai, mistral, groq, gemini, xai.
Not supported: deepseek (returns VisionNotSupported error).

## Provider & Model Selection

| Provider | Best Models | Strengths |
|----------|------------|-----------|
| `claude` | claude-sonnet-4-20250514 | Reasoning, long context, code |
| `openai` | gpt-4.1, gpt-4.1-mini | Structured output, vision, speed |
| `mistral` | mistral-large-latest | Multilingual, code |
| `groq` | llama-3.3-70b-versatile | Speed (fastest inference) |
| `deepseek` | deepseek-chat | Reasoning, cost-effective |
| `gemini` | gemini-2.5-flash | Long context (1M tokens) |
| `xai` | grok-3-mini | Real-time knowledge |
| `native` | (local GGUF file) | Privacy, offline |
| `mock` | (testing) | Deterministic testing |

Set default at workflow level or override per task:

```yaml
schema: nika/workflow@0.12
model: gpt-4.1-mini           # Default for all tasks
provider: openai               # Default for all tasks
tasks:
  - id: fast
    infer: "Quick answer"
    provider: groq             # Override for this task
    model: llama-3.3-70b-versatile
```

## Prompt Engineering Patterns

### Chain of Thought

```yaml
- id: think
  infer: |
    Think step by step about this problem:
    {{with.problem}}

    First, identify the key components.
    Then, analyze each one.
    Finally, synthesize your conclusion.
  temperature: 0.2
```

### Few-Shot Examples

```yaml
- id: classify
  system: |
    Classify sentiment as positive, negative, or neutral.
    Examples:
    Input: "Great product!" -> positive
    Input: "Terrible service" -> negative
    Input: "It arrived on time" -> neutral
  infer: "Classify: {{with.review}}"
  temperature: 0.0
```

### Structured Extraction

```yaml
- id: extract
  infer: "Extract entities from: {{with.text}}"
  structured:
    schema:
      type: object
      properties:
        people:
          type: array
          items: { type: string }
        locations:
          type: array
          items: { type: string }
        dates:
          type: array
          items: { type: string }
      required: [people, locations, dates]
  temperature: 0.0
```

### Multi-Step Refinement

```yaml
tasks:
  - id: draft
    infer: "Write a blog post about {{inputs.topic}}"
    max_tokens: 1000

  - id: critique
    depends_on: [draft]
    with:
      text: $draft
    infer: "Review this draft and list improvements: {{with.text}}"
    max_tokens: 500

  - id: final
    depends_on: [draft, critique]
    with:
      original: $draft
      feedback: $critique
    infer: |
      Rewrite this draft incorporating the feedback:
      DRAFT: {{with.original}}
      FEEDBACK: {{with.feedback}}
    max_tokens: 1500
```

## Temperature Guide

| Temperature | Use Case |
|-------------|----------|
| 0.0 | Extraction, classification, structured output |
| 0.2-0.3 | Analysis, summaries, technical writing |
| 0.5-0.7 | Creative writing, brainstorming |
| 1.0-1.5 | Highly creative, diverse outputs |

## Saving Output to Files

```yaml
- id: write
  infer: "Write a poem about the ocean"
  artifact:
    path: poem.txt
    format: text

# With template wrapping
- id: report
  infer: "Analyze this data: {{with.data}}"
  artifact:
    path: report.md
    format: markdown
    template: |
      # Analysis Report
      Generated: {{with.date}}

      {{output}}
```

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| `infer: { prompt: "..." }` without closing | Use short form `infer: "..."` for simple prompts |
| Vision with deepseek provider | DeepSeek does not support vision |
| Missing `max_tokens:` | Always set to control costs and output length |
| `temperature: 2.5` | Range is 0.0-2.0 |
| Structured output without `required:` | Always list required properties for reliable parsing |
| GGUF model for vision | GGUF is text-only; use HuggingFace models for native vision |

## Validation

```bash
nika check workflow.nika.yaml    # Catches schema, binding, DAG errors
nika run workflow.nika.yaml      # Test actual LLM generation
```
