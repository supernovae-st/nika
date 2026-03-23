---
name: nika-structured
description: >-
  Structured output expert for Nika YAML workflows (.nika.yaml). Design JSON
  schemas for the structured: field, validate LLM output against schemas,
  configure max_retries, and extract typed data from infer: and agent: tasks.
  Use when adding structured JSON output to Nika workflows
  (schema nika/workflow@0.12).
---

# Nika Structured Output Expert

Force LLM output to conform to a JSON schema using the `structured:` field.

## Basic Syntax

```yaml
- id: extract
  infer: "Extract the person's name and age from: {{with.text}}"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        age: { type: number }
      required: [name, age]
```

The LLM output is validated against the schema. If it does not match, Nika retries automatically (up to `max_retries`).

## Schema Types

### Primitive Types

```yaml
schema:
  type: object
  properties:
    name: { type: string }
    count: { type: number }
    active: { type: boolean }
  required: [name, count, active]
```

### Arrays

```yaml
schema:
  type: object
  properties:
    tags:
      type: array
      items: { type: string }
    scores:
      type: array
      items: { type: number }
  required: [tags, scores]
```

### Nested Objects

```yaml
schema:
  type: object
  properties:
    user:
      type: object
      properties:
        name: { type: string }
        address:
          type: object
          properties:
            city: { type: string }
            country: { type: string }
          required: [city, country]
      required: [name, address]
  required: [user]
```

### Enums

```yaml
schema:
  type: object
  properties:
    sentiment:
      type: string
      enum: [positive, negative, neutral]
    priority:
      type: number
      enum: [1, 2, 3, 4, 5]
  required: [sentiment, priority]
```

### Array of Objects

```yaml
schema:
  type: object
  properties:
    items:
      type: array
      items:
        type: object
        properties:
          title: { type: string }
          score: { type: number }
          tags:
            type: array
            items: { type: string }
        required: [title, score]
  required: [items]
```

## Patterns

### Entity Extraction

```yaml
- id: extract
  infer: |
    Extract all entities from this text:
    {{with.article}}
  structured:
    schema:
      type: object
      properties:
        people:
          type: array
          items: { type: string }
        organizations:
          type: array
          items: { type: string }
        locations:
          type: array
          items: { type: string }
        dates:
          type: array
          items: { type: string }
      required: [people, organizations, locations, dates]
  temperature: 0.0
```

### Classification

```yaml
- id: classify
  infer: "Classify this support ticket: {{with.ticket}}"
  structured:
    schema:
      type: object
      properties:
        category:
          type: string
          enum: [billing, technical, feature_request, bug_report, other]
        urgency:
          type: string
          enum: [low, medium, high, critical]
        summary: { type: string }
      required: [category, urgency, summary]
  temperature: 0.0
```

### Scoring / Rating

```yaml
- id: score
  infer: "Rate this code review on quality, readability, and performance (1-10 each)"
  structured:
    schema:
      type: object
      properties:
        quality: { type: number }
        readability: { type: number }
        performance: { type: number }
        overall: { type: number }
        feedback: { type: string }
      required: [quality, readability, performance, overall, feedback]
```

### Structured + for_each

```yaml
- id: analyze
  for_each: ["intro.md", "chapter1.md", "conclusion.md"]
  as: file
  infer: "Analyze writing quality of: {{with.file_content}}"
  structured:
    schema:
      type: object
      properties:
        word_count: { type: number }
        reading_level: { type: string }
        tone: { type: string }
      required: [word_count, reading_level, tone]
```

### Structured + Artifact

```yaml
- id: extract
  infer: "Extract product data from: {{with.page}}"
  structured:
    schema:
      type: object
      properties:
        products:
          type: array
          items:
            type: object
            properties:
              name: { type: string }
              price: { type: number }
              currency: { type: string }
            required: [name, price]
      required: [products]
  artifact:
    path: products.json
    format: json
```

### Structured + Agent

```yaml
- id: agent_analyze
  agent:
    prompt: "Analyze the codebase and return structured findings"
    tools: [nika_glob, nika_read, nika_complete]
    max_turns: 10
    provider: openai
    model: gpt-4.1
  structured:
    schema:
      type: object
      properties:
        files_count: { type: number }
        issues:
          type: array
          items:
            type: object
            properties:
              severity: { type: string }
              message: { type: string }
            required: [severity, message]
      required: [files_count, issues]
```

## Retry on Validation Failure

When the LLM output does not match the schema:

```yaml
- id: extract
  infer: "Extract data"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
      required: [name]
  retry:
    max_attempts: 3               # Retry up to 3 times on schema mismatch
```

## Accessing Structured Output Downstream

The output is valid JSON, accessible via JSONPath in bindings:

```yaml
- id: extract
  infer: "Extract name and city"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        city: { type: string }
      required: [name, city]

- id: use
  depends_on: [extract]
  with:
    person_name: $extract.name      # JSONPath access
    person_city: $extract.city
  exec: "echo '{{with.person_name}} lives in {{with.person_city}}'"
```

## Provider Notes

- **OpenAI**: Strict JSON mode. `required:` must list ALL properties.
- **Claude**: Flexible structured output via prompting.
- **Groq**: Fast but may need retries on complex schemas.
- **Gemini**: Good structured output support.
- **DeepSeek**: Best with simple schemas.

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| Missing `required:` list | Always list required properties |
| `required:` with subset of properties | For OpenAI strict mode, list ALL properties |
| Schema without `type: object` at root | Root must be `type: object` |
| Very complex nested schema | Keep schemas simple; split into multiple tasks |
| Missing `retry:` for unreliable models | Add `retry: { max_attempts: 3 }` |
| `temperature: 1.0` with structured | Use `temperature: 0.0` for consistent structure |

## Error Codes

| Code | Error | Fix |
|------|-------|-----|
| NIKA-300 | Invalid JSON schema definition | Fix the schema YAML |
| NIKA-301 | LLM output failed validation | Simplify schema, add retry, improve prompt |

## Validation

```bash
nika check workflow.nika.yaml    # Validates schema definition
nika run workflow.nika.yaml      # Tests actual LLM output against schema
```
