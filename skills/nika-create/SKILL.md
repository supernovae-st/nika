---
name: nika-create
description: >-
  Create new Nika YAML workflows (.nika.yaml) from natural language descriptions.
  Generates valid schema nika/workflow@0.12 files with the right verbs (infer,
  exec, fetch, invoke, agent), bindings, DAG dependencies, and validates with
  `nika check`. Use when users want to build, scaffold, or generate a new
  .nika.yaml workflow file.
---

# Create Nika Workflow

Generate valid `.nika.yaml` workflows from natural language requirements.

## Process

1. **Clarify** the user's goal (what data flows in, what comes out)
2. **Choose verbs** for each step
3. **Design the DAG** (dependencies, parallelism)
4. **Generate YAML** with correct syntax
5. **Validate** with `nika check`

## Verb Selection Guide

| User wants to... | Verb |
|-------------------|------|
| Generate text / ask LLM | `infer:` |
| Run a shell command | `exec:` |
| Call an API / fetch a URL | `fetch:` |
| Call an MCP tool / nika builtin | `invoke:` |
| Autonomous multi-step agent | `agent:` |

## Questions to Ask

1. What is the end goal? (report, file, API call, transformed data)
2. What are the inputs? (files, URLs, user params, nothing)
3. Which LLM provider? (openai, claude, mistral, groq, deepseek, gemini, xai)
4. Should outputs be saved to files? (artifacts)
5. Any steps that can run in parallel?
6. Need structured JSON output?

## Template: Simple LLM Workflow

```yaml
schema: nika/workflow@0.12
workflow: summarize-text
model: gpt-4.1-mini

inputs:
  topic:
    default: "artificial intelligence"

tasks:
  - id: research
    exec: "echo 'Gathering info about {{inputs.topic}}'"

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer: "Summarize this research about {{inputs.topic}}: {{with.data}}"
    max_tokens: 500

  - id: format
    depends_on: [summarize]
    with:
      summary: $summarize
    exec: "echo '# Summary\n{{with.summary}}'"
```

## Template: Fetch + Process Pipeline

```yaml
schema: nika/workflow@0.12
workflow: api-pipeline
model: gpt-4.1-mini

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
      method: GET
      headers:
        Authorization: "Bearer {{$env.API_TOKEN}}"

  - id: analyze
    depends_on: [fetch_data]
    with:
      data: $fetch_data
    infer: "Analyze this data and extract key insights: {{with.data}}"
    structured:
      schema:
        type: object
        properties:
          insights:
            type: array
            items: { type: string }
          confidence: { type: number }
        required: [insights, confidence]

  - id: report
    depends_on: [analyze]
    with:
      result: $analyze
    exec: "echo '{{with.result | to_json}}'"
    artifact:
      path: report.json
      format: json
```

## Template: Parallel Fan-Out

```yaml
schema: nika/workflow@0.12
workflow: multi-language
model: gpt-4.1-mini

artifacts:
  dir: ./output

tasks:
  - id: draft
    infer: "Write a short product description for a smart watch"
    max_tokens: 200

  - id: translate
    depends_on: [draft]
    for_each: ["french", "spanish", "german", "japanese"]
    as: lang
    with:
      text: $draft
    infer: "Translate to {{with.lang}}: {{with.text}}"
    max_tokens: 300
    artifact:
      path: "{{with.lang}}.txt"
```

## Validation

Always validate after generating:

```bash
nika check workflow.nika.yaml
```

Common NIKA error codes during creation:

| Code | Issue | Fix |
|------|-------|-----|
| NIKA-001 | Failed to parse workflow | Fix YAML syntax errors |
| NIKA-002 | Invalid schema version | Add `schema: nika/workflow@0.12` |
| NIKA-004 | Workflow validation failed | Read the detailed validation message |
| NIKA-005 | Schema validation failed | Fix schema structure issues |
| NIKA-020 | Circular dependency | Check `depends_on:` for cycles |
| NIKA-021 | Missing dep reference | `depends_on:` references nonexistent task |
| NIKA-022 | Duplicate task ID | Each id must be unique |
| NIKA-041 | Template resolution error | Check `{{...}}` delimiters and binding sources |
| NIKA-071 | Unknown alias in with: block | `with:` value needs `$` prefix |
| NIKA-080 | Unknown task in with: reference | Referenced task id does not exist |

## Common Mistakes to Avoid

- Missing `schema: nika/workflow@0.12` as first line
- Using `.yaml` instead of `.nika.yaml` extension
- Forgetting `$` prefix in `with:` bindings: `data: $task_id`
- Adding two verbs to one task (only one verb per task)
- Missing `depends_on:` when using `with: { x: $upstream }`
- Using `for_each:` without `as:` (the iterator variable)
- Setting `timeout: 30` thinking it means milliseconds (it means seconds)

## Checklist Before Delivering

- [ ] File has `.nika.yaml` extension
- [ ] First line is `schema: nika/workflow@0.12`
- [ ] Every task has unique `id:`
- [ ] Every task has exactly one verb
- [ ] All `with:` references use `$` prefix
- [ ] All `with:` sources have matching `depends_on:`
- [ ] `for_each:` paired with `as:`
- [ ] `nika check` passes
