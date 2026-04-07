## Advanced Features

### agent: verb (full form)

```yaml
- id: assistant
  agent:
    system: "You are a research assistant"
    prompt: "Find and analyze {{inputs.topic}}"
    tools: [novanet::novanet_search, novanet::fetch_node]
    max_turns: 10
    token_budget: 50000
    tool_choice: auto
    completion:
      mode: explicit               # explicit | natural | pattern
    guardrails:
      - type: length
        min_words: 100
        max_words: 2000
        on_failure: retry
      - type: schema
        json_schema:
          type: object
          properties:
            findings: { type: array }
          required: [findings]
        on_failure: escalate
    limits:
      max_cost_usd: 2.0
      max_duration_secs: 120
```

### on_error: error handling

```yaml
- id: generate
  on_error: ignore                 # Skip failures silently
  infer: "Generate content"

- id: translate
  on_error:
    strategy: retry_with_provider
    fallback_provider: openai      # Try another provider on failure
  infer: "Translate: {{with.text}}"

- id: critical
  on_error:
    strategy: fallback
    fallback_task: manual_review   # Jump to fallback task
  infer: "Critical operation"
```

### when: conditional execution

```yaml
- id: translate
  when: "{{inputs.locale != 'en'}}"
  infer: "Translate to {{inputs.locale}}"
```

### Artifacts

```yaml
artifacts:                         # Workflow-level defaults
  dir: ./output
  format: markdown
  mode: overwrite

# Task-level:
- id: report
  infer: "Generate report"
  artifact:
    path: report.md
    format: markdown
```

### Vision / multimodal

```yaml
- id: analyze
  infer:
    content:
      - type: image
        source: "{{with.photo_hash}}"   # CAS hash from nika:import
        detail: high
      - type: text
        text: "Describe this image"
```

`source:` must be a CAS hash — NEVER a file path. Use `nika:import` or `nika:decode` first.

### routing: provider fallback chain

```yaml
- id: resilient
  routing:
    fallback: [anthropic, openai, groq]
  infer: "Generate content with automatic provider fallback"
```
