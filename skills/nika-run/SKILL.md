---
name: nika-run
description: >-
  Execute Nika YAML workflows (.nika.yaml) and explain results. Runs `nika run`,
  parses output, explains task execution order, binding resolution, artifact
  generation, and diagnoses runtime failures. Use when users want to run, execute,
  or test a .nika.yaml workflow with the Nika engine.
---

# Run Nika Workflows

Execute `.nika.yaml` workflows and interpret results.

## Commands

```bash
nika run workflow.nika.yaml                           # Basic run
nika run workflow.nika.yaml --input topic="AI Safety"  # With input overrides
nika run workflow.nika.yaml --quiet                    # Minimal output
nika run workflow.nika.yaml --trace trace.ndjson       # Save execution trace
```

## Pre-Run Checklist

1. **Validate first**: `nika check workflow.nika.yaml`
2. **Check provider**: Ensure API key is set for the provider used
3. **Check dependencies**: If `exec:` uses external commands, verify they exist
4. **Check MCP servers**: If `invoke:` uses MCP, ensure servers are configured

## Understanding Output

### Task Execution Order

Nika builds a DAG from `depends_on:` and executes:
- Tasks without dependencies run first (in parallel if possible)
- Tasks with dependencies wait for all deps to complete
- `for_each:` tasks expand into parallel iterations

### Output Format

Each task prints:
```
[1/3] task_id .................. OK (0.5s)
[2/3] task_id .................. OK (1.2s)
[3/3] task_id .................. OK (0.3s)
```

For `for_each:` tasks:
```
[2/3] task_id [1/4] ........... OK (0.5s)
[2/3] task_id [2/4] ........... OK (0.5s)
[2/3] task_id [3/4] ........... OK (0.5s)
[2/3] task_id [4/4] ........... OK (0.5s)
```

### Artifacts

If tasks define `artifact:`, output files appear in the artifacts directory:
```
./artifacts/              # Default directory
./output/                 # Or custom dir from artifacts.dir
```

## Runtime Troubleshooting

### Provider / API Key Issues

```
NIKA-032: Missing API key for provider 'openai'
```

Fix: Export the required environment variable:
```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
export MISTRAL_API_KEY="..."
export GROQ_API_KEY="gsk_..."
export DEEPSEEK_API_KEY="sk-..."
export GEMINI_API_KEY="..."
export XAI_API_KEY="xai-..."
```

### Timeout Issues

```
NIKA-121: Task 'slow_task' timed out after 30s
```

Fix: Increase timeout in the task:
```yaml
- id: slow_task
  timeout: 120              # 120 seconds
  fetch:
    url: "https://slow-api.example.com"
```

### Exec Command Not Found

```
NIKA-050: Invalid path syntax — Command 'jq' not found
```

Fix: Install the command or use a different approach:
```bash
which jq || brew install jq
```

### Binding Resolution Failures

```
NIKA-041: Template resolution error — 'with.data' not found
```

Fix: Ensure the `with:` block defines the alias:
```yaml
- id: step2
  depends_on: [step1]
  with:
    data: $step1              # This defines with.data
  exec: "echo {{with.data}}"
```

### Structured Output Validation Failure

```
NIKA-301: Output validation failed
```

Fix: The LLM output did not match the JSON schema. Options:
1. Simplify the schema
2. Add retry: `retry: { max_attempts: 3 }`
3. Make the prompt more explicit about the expected format
4. Use a more capable model

## Example: Full Run Cycle

```bash
# 1. Create workflow
cat > pipeline.nika.yaml << 'EOF'
schema: nika/workflow@0.12
workflow: demo
model: gpt-4.1-mini
tasks:
  - id: greet
    exec: "echo 'Hello from Nika'"
  - id: enhance
    depends_on: [greet]
    with:
      msg: $greet
    infer: "Make this greeting more creative: {{with.msg}}"
    max_tokens: 100
EOF

# 2. Validate
nika check pipeline.nika.yaml

# 3. Run
nika run pipeline.nika.yaml

# 4. Check artifacts (if any)
ls ./artifacts/
```

## Trace Analysis

For debugging, use trace output:

```bash
nika run workflow.nika.yaml --trace trace.ndjson
```

The trace file contains NDJSON events:
- `workflow_started` -- Workflow begins
- `task_started` -- Task begins execution
- `task_completed` -- Task finishes (includes output)
- `task_failed` -- Task failed (includes error)
- `workflow_completed` -- All tasks done

## Input Overrides

Override `inputs:` defaults from the command line:

```yaml
# workflow.nika.yaml
inputs:
  topic:
    default: "AI"
  language:
    default: "English"
```

```bash
nika run workflow.nika.yaml --input topic="Rust" --input language="French"
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Running without API key | Export `PROVIDER_API_KEY` env var |
| Expecting ms for timeout | `timeout: 30` means 30 seconds |
| Not checking exit code | `nika run` returns non-zero on failure |
| Forgetting `nika check` first | Always validate before running |
| Large LLM outputs without max_tokens | Set `max_tokens:` to avoid excessive costs |
