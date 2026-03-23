---
name: nika-wizard
description: Interactive workflow creation wizard. Asks questions about your goal, designs a DAG, generates a .nika.yaml file, validates it, and optionally runs it. Use when the user wants to create a new Nika workflow from scratch.
user-invocable: true
allowed-tools: Bash, Read, Write, Edit, Glob, Grep
argument-hint: "[goal description or workflow name]"
---

# Nika Workflow Wizard

> Build production-ready .nika.yaml workflows through guided conversation.

## Process

Follow these steps IN ORDER. Do not skip steps.

### Step 1: Understand the Goal

Ask the user what they want to achieve. Gather:

1. **What** — What is the end goal? (e.g., "translate a blog post into 5 languages")
2. **Inputs** — What data or files does the workflow start with?
3. **Outputs** — What should the final result look like? (files, JSON, text)
4. **Providers** — Which LLM providers should be used? (claude, openai, etc.)
5. **Integrations** — Does it need MCP servers, fetch calls, shell commands?

If the user provided a goal description as an argument, extract answers from it and confirm.

### Step 2: Design the DAG

Based on the answers, design a task graph:

1. Break the goal into discrete tasks (aim for 3-8 tasks)
2. Identify which verb each task needs:
   - `infer:` — LLM generation (text, structured output)
   - `exec:` — Shell commands (file manipulation, builds)
   - `fetch:` — HTTP requests (APIs, web scraping)
   - `invoke:` — MCP tool calls (NovaNet, databases)
   - `agent:` — Multi-turn autonomous loops
3. Map data dependencies between tasks (`with:` bindings)
4. Identify parallelism opportunities (`for_each:`)

Present the DAG as ASCII art:

```
  [fetch_data] ──► [transform] ──► [generate_content]
                                        │
                                   [write_output]
```

Ask the user to confirm or adjust the design.

### Step 3: Generate the Workflow

Create the .nika.yaml file following these rules:

- Schema: `nika/workflow@0.12` (always current)
- File extension: `.nika.yaml` (never `.yaml` alone)
- Task IDs: `snake_case` matching `^[a-z][a-z0-9_]*$`
- Bindings: `with: { alias: source_task }` + `{{with.alias}}`
- for_each: FLAT format only (never nested)
- Provider: explicit if user specified, omit for auto-detect
- Security: `shell: false` default for exec, env vars for secrets

Include these sections as needed:

```yaml
schema: nika/workflow@0.12
workflow: descriptive-name
description: "Clear description of what this does"
provider: claude  # if specified

# Optional sections
context:
  files:
    data: ./context/data.md
inputs:
  param: { default: "value" }
mcp:
  server_name:
    command: "..."
    args: ["..."]

tasks:
  - id: first_task
    # verb + configuration
```

### Step 4: Validate

Run validation immediately after writing the file:

```bash
nika check <workflow-file> 2>&1
```

If validation fails:
1. Read the NIKA-XXX error code
2. Fix the issue in the workflow
3. Re-validate until clean

### Step 5: Offer to Run

Ask the user if they want to run the workflow:

```bash
# Dry run (validate + show DAG)
nika check <workflow-file> --strict

# Execute
nika run <workflow-file>
```

## Reference: Common Patterns

### Linear Pipeline

```yaml
tasks:
  - id: step_a
    infer: "Generate outline"
  - id: step_b
    with: { outline: $step_a }
    infer: "Expand: {{with.outline}}"
  - id: step_c
    with: { content: $step_b }
    exec: "echo '{{with.content}}' > output.md"
```

### Fan-out / Fan-in

```yaml
tasks:
  - id: get_items
    exec: 'echo ''["a","b","c"]'''
  - id: process_each
    for_each: "$get_items"
    as: item
    concurrency: 3
    infer: "Process {{with.item}}"
  - id: aggregate
    with: { results: $process_each }
    infer: "Summarize: {{with.results}}"
```

### Fetch + Transform

```yaml
tasks:
  - id: fetch_api
    fetch:
      url: "https://api.example.com/data"
      method: GET
      headers:
        Authorization: "Bearer {{inputs.api_key}}"
  - id: transform
    with: { data: $fetch_api }
    infer: "Extract key insights from: {{with.data}}"
```

### Agent Loop

```yaml
tasks:
  - id: research
    agent:
      prompt: "Research and write a comprehensive report on the topic"
      mcp: [perplexity]
      max_turns: 10
      extended_thinking: true
```

## Rules

- NEVER skip validation (Step 4)
- NEVER use `flows:` (removed in @0.10)
- NEVER nest for_each properties
- ALWAYS use `.nika.yaml` extension
- ALWAYS ask before running (Step 5)
- PREFER explicit `with:` over `depends_on:` for data flow
- KEEP task count manageable (3-8 per workflow)
