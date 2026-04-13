# Frequently Asked Questions

## General

### What is Nika?

Nika is a semantic YAML workflow engine for AI tasks. It lets you define multi-step pipelines that orchestrate LLM calls, shell commands, HTTP requests, MCP tool invocations, and autonomous agent loops -- all from declarative YAML files with the `.nika.yaml` extension.

### What does "semantic" mean in this context?

Nika uses five named verbs (`infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`) instead of generic "step" or "action" types. Each verb has purpose-specific fields and validation. This makes workflows self-documenting and enables smarter error messages.

### What schema version should I use?

Always use `nika/workflow@0.12`. This is the current and only supported schema version. Put it as the first line of every workflow:

```yaml
schema: nika/workflow@0.12
```

### Do I need an API key to use Nika?

No. The `exec:` and `fetch:` verbs work without any API keys. You can build powerful workflows using just shell commands and HTTP requests. API keys are only needed for `infer:` and `agent:` tasks that call LLM providers.

### Which LLM providers does Nika support?

Seven cloud providers and one local option:

| Provider | Env Var |
|----------|---------|
| Anthropic (Claude) | `ANTHROPIC_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| Mistral | `MISTRAL_API_KEY` |
| Groq | `GROQ_API_KEY` |
| DeepSeek | `DEEPSEEK_API_KEY` |
| Google Gemini | `GEMINI_API_KEY` |
| xAI (Grok) | `XAI_API_KEY` |
| Native (local) | No key needed |

### Can I run models locally?

Yes. Use `provider: native` to run GGUF models locally via mistral.rs. Download models with `nika model pull <model>`. No API key needed.

### Is Nika open source?

Yes. Nika is licensed under AGPL-3.0-or-later.

## Workflows

### What file extension should I use?

Always `.nika.yaml`. Not `.yaml`, not `.yml`, not `.nika.yml`. Nika validates the extension.

### Can tasks run in parallel?

Yes, automatically. Tasks with no dependencies on each other run in parallel without any configuration. Just declare your dependencies correctly, and Nika figures out the optimal execution order.

### What is the difference between depends_on and with?

- `depends_on:` creates a pure **ordering** dependency. The downstream task waits for the upstream task to finish but does not automatically receive its output.
- `with:` creates a **data flow** dependency. It binds the upstream task's output to an alias that can be used in templates.

In practice, you often use both:

```yaml
- id: consumer
  depends_on: [producer]    # Wait for producer
  with:
    data: $producer         # Also bind its output
  exec: "echo '{{with.data}}'"
```

If you use `with:` with a `$task_id`, Nika automatically infers the ordering dependency, so `depends_on:` is technically redundant in that case. However, it makes the intent clearer.

### How do I pass data between tasks?

Use the `with:` block and `{{with.alias}}` templates:

```yaml
- id: producer
  exec: "echo 'Hello World'"

- id: consumer
  with:
    greeting: $producer | trim
  exec: "echo 'Received: {{with.greeting}}'"
```

The `$` prefix is required for task references. You can apply transforms with `|` pipes.

### Can I use environment variables in workflows?

Yes, in two ways:

```yaml
# In with: bindings
with:
  key: $env.API_KEY

# In MCP server config
mcp:
  server:
    env:
      TOKEN: "{{$env.MY_TOKEN}}"
```

### How do I iterate over a list?

Use `for_each:`:

```yaml
- id: items
  exec: 'echo ''["a", "b", "c"]'''
  output:
    format: json

- id: process
  depends_on: [items]
  for_each: $items
  concurrency: 3
  exec: "echo 'Processing: {{with.item}}'"
```

### How do I handle errors?

Add `retry:` for automatic retries:

```yaml
- id: flaky_api
  fetch:
    url: "https://api.example.com"
  retry:
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0
```

For `for_each`, use `fail_fast: false` to continue processing even if some iterations fail.

### Can I split a workflow into multiple files?

Yes, using `imports:`:

```yaml
imports:
  - path: ./modules/setup.nika.yaml
    prefix: setup_
```

Imported tasks are prefixed to avoid name collisions.

### How do I validate a workflow without running it?

```bash
nika check workflow.nika.yaml
```

This checks YAML syntax, schema version, DAG structure, and binding validity. Add `--strict` to also test MCP server connections.

## LLM and infer

### How do I control LLM creativity?

Use the `temperature` field. Lower values (0.0-0.3) produce deterministic, focused output. Higher values (0.7-1.0) produce creative, varied output.

```yaml
infer:
  prompt: "..."
  temperature: 0.3  # Focused and consistent
```

### How do I get JSON output from an LLM?

Use the `structured:` field with a JSON Schema:

```yaml
- id: extract
  infer:
    prompt: "Extract names and ages from: {{with.text}}"
  structured:
    schema:
      type: object
      properties:
        people:
          type: array
          items:
            type: object
            properties:
              name: { type: string }
              age: { type: integer }
            required: [name]
      required: [people]
    max_retries: 2
    enable_repair: true
```

### Can I send images to an LLM?

Yes, using the `content:` field with vision-capable providers (Claude, OpenAI, Gemini, Mistral, Groq, xAI):

```yaml
infer:
  content:
    - type: image
      source: "{{with.img.media[0].hash}}"
      detail: high
    - type: text
      text: "Describe this image."
```

Import images first with `nika:import` to get a CAS hash.

### Which providers support vision?

All except DeepSeek and native GGUF models. For local vision, use HuggingFace models with ISQ quantization via `nika model vision`.

### How do I use different models for different tasks?

Set `provider:` and `model:` at the task level:

```yaml
provider: anthropic  # Workflow default

tasks:
  - id: fast
    provider: groq
    model: llama-3.3-70b-versatile
    infer: { prompt: "Quick task" }

  - id: smart
    model: claude-opus-4
    infer: { prompt: "Complex task" }
```

## fetch

### How do I scrape a webpage?

Use `extract: markdown` for the best LLM-friendly output:

```yaml
fetch:
  url: "https://example.com/article"
  extract: markdown
```

For just the main article body: `extract: article`. For specific elements: `extract: selector` with `selector: "CSS selector"`.

### How do I parse a JSON API response?

Use `extract: jsonpath`:

```yaml
fetch:
  url: "https://api.example.com/users"
  extract: jsonpath
  selector: "$[*].name"
```

### How do I download binary files?

Use `response: binary`:

```yaml
fetch:
  url: "https://example.com/image.jpg"
  response: binary
```

This stores the file in the CAS and returns a hash for use with media tools.

## exec

### Why doesn't my pipe command work?

Shell features (pipes, `&&`, redirects, globs) require `shell: true`:

```yaml
exec:
  command: "cat file.txt | grep 'pattern' | wc -l"
  shell: true
```

### How do I capture JSON from a command?

Add `output: { format: json }`:

```yaml
- id: data
  exec: 'echo ''{"key": "value"}'''
  output:
    format: json

- id: use
  with:
    val: $data.key
  exec: "echo '{{with.val}}'"
```

## invoke and MCP

### What are builtin tools?

Tools prefixed with `nika:` that are compiled into Nika. They include media processing (`nika:thumbnail`, `nika:import`), logging (`nika:log`), assertions (`nika:assert`), and more. They work without any MCP server configuration.

### How do I check which tools are available?

```bash
# Compiled features (which tool tiers are available)
nika features

# Tools on an MCP server
nika mcp tools workflow.yaml server_name
```

### How do I connect to an MCP server?

Add an `mcp:` block at the workflow level:

```yaml
mcp:
  server_name:
    command: npx
    args: ["-y", "@package/server"]
    env:
      API_KEY: "{{$env.API_KEY}}"
```

Then use `invoke: { tool: "server_name::tool_name", params: {...} }`.

## agent

### When should I use agent instead of infer?

Use `agent:` when the task requires multiple rounds of tool use, decision-making, or iteration. Use `infer:` for single-shot generation (summarization, classification, translation, etc.).

### How do I stop an agent?

Multiple options:
- `max_turns: N` -- hard limit on iterations
- `completion: { on_tool: tool_name }` -- stop when a specific tool is called
- `stop_sequences: ["DONE"]` -- stop on a text pattern
- `limits: { max_cost_usd: 1.0 }` -- stop when cost limit reached
- `token_budget: N` -- stop when token budget exhausted

### Can agents call other agents?

Yes, through the `spawn_agent` tool. Use `depth_limit` to prevent infinite recursion:

```yaml
agent:
  prompt: "Manage this project."
  tools: [spawn_agent]
  depth_limit: 3
```

## CLI

### What is the difference between nika run and just nika file.yaml?

They are equivalent. `nika workflow.nika.yaml` is a shorthand for `nika run workflow.nika.yaml`.

### How do I suppress output?

Use `-q` (quiet) for errors only, or `--detail min` for minimal output:

```bash
nika -q run workflow.nika.yaml
nika --detail min run workflow.nika.yaml
```

### How do I get JSON output from the CLI?

```bash
nika --detail json run workflow.nika.yaml
```

### How do I set up shell completions?

```bash
# Manual
nika completion bash > ~/.local/share/bash-completion/completions/nika
nika completion zsh > ~/.zfunc/_nika
nika completion fish > ~/.config/fish/completions/nika.fish
```

## Configuration

### Where is the config file?

In your project: `.nika/config.toml`

Manage it with:

```bash
nika config list                   # Show all values
nika config get editor.theme       # Get a specific value
nika config set editor.theme dark  # Set a value
nika config edit                   # Open in $EDITOR
nika config path                   # Show file path
```

### How do I create a new project?

```bash
nika init              # Interactive wizard
nika init --course     # With learning course
```

### How do I create a new workflow from a template?

```bash
nika new                           # Interactive wizard
nika new --template blog-generator # From template
nika new --verb infer              # By primary verb
nika new --list                    # List available templates
```

## Performance

### How many tasks can a workflow have?

There is no hard limit. Workflows with 20+ tasks run efficiently. The DAG engine handles complex dependency graphs with minimal overhead.

### Are tasks truly parallel?

Yes. Tasks without dependencies execute concurrently using Tokio async runtime. You can control concurrency in `for_each` with the `concurrency` field.

### How do I debug slow workflows?

1. Run with verbose logging: `nika -vv run workflow.yaml`
2. Check trace files: `nika trace list && nika trace show <id>`
3. Look for sequential bottlenecks in the DAG
4. Add timeouts to prevent hung tasks
5. Consider using faster providers (Groq) for simple tasks

## Troubleshooting

### Why does nika check pass but nika run fails?

`nika check` validates structure (syntax, DAG, bindings) but does not execute tasks. Runtime errors (API failures, command errors, network issues) only appear during `nika run`. Use `--strict` to also check MCP connections.

### Why is my API key not detected?

Check:
1. The environment variable is exported (not just set): `export ANTHROPIC_API_KEY="..."`
2. The variable name is correct (run `nika provider list` to see expected names)
3. The key format matches the expected prefix (e.g., `sk-ant-` for Anthropic)
4. Your shell profile is loaded: `source ~/.zshrc`

### How do I test without using real API credits?

Use the mock provider:

```bash
nika run workflow.nika.yaml --provider mock
```

This returns predictable test responses without making any API calls.
