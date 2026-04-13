# Intermediate Track -- Levels 5 through 8

> Master data transformation, structured output, builtin tools, and autonomous agents.

---

## Level 5 -- Shapeshifter

> *"Chaos is just structure that has not met you yet."*

### What You Will Learn

Level 5 dives deep into structured output, artifacts, and schema-driven validation. You will learn to force LLMs to return exactly the data shape you need, save output to files, and automatically retry when validation fails.

### Concept: Structured Output

LLMs are powerful but chaotic. Without constraints, you get prose when you need JSON. The `output:` field tames them:

```yaml
tasks:
  - id: extract
    infer:
      prompt: "Extract the name and age from: John Smith, 34 years old"
      output:
        format: json_schema
        schema:
          type: object
          properties:
            name:
              type: string
            age:
              type: integer
          required: [name, age]
```

The LLM must return valid JSON matching that schema. If it does not, Nika catches the error.

Three output formats are available:

| Format | What You Get |
|--------|-------------|
| `json` | Any valid JSON |
| `json_schema` | JSON matching your exact schema |
| (omitted) | Raw text (the default) |

**Why this matters**: Without structured output, LLM pipelines are fragile. One malformed response breaks everything downstream. With schema validation, you get guarantees: real, parseable, type-safe output.

### Concept: Artifacts

Artifacts save task output to files. They provide a structured way to capture workflow results:

```yaml
# Workflow-level artifact configuration
artifacts:
  dir: ./output/reports
  format: text
  manifest: true       # Generate a manifest.json listing all artifacts

tasks:
  - id: report
    infer: "Generate a comprehensive status report."
    artifact:
      path: output/status-report.md
```

The `manifest: true` option creates a JSON file listing all generated artifacts, useful for downstream processing.

### Concept: Schema Retry

When an LLM fails to produce valid output on the first attempt, Nika can automatically retry with feedback:

```yaml
output:
  format: json_schema
  schema:
    type: object
    properties:
      items: { type: array, items: { type: string } }
    required: [items]
  enable_retry: true        # Enable automatic retry
  max_retry_attempts: 3     # Up to 3 attempts
```

On failure, Nika feeds the validation errors back to the LLM and asks it to fix the output. This dramatically improves reliability without any manual intervention.

### Exercise 1: Structured Output

**Objective**: Force an LLM to return a specific JSON structure.

Create a workflow where an `infer:` task must return a JSON object with `title` (string), `summary` (string), and `tags` (array of strings). Use `output: format: json_schema`.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: structured-output
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: extract_info
    infer:
      prompt: |
        Analyze this text and extract structured information:
        "Nika is a semantic YAML workflow engine for AI tasks.
        It supports 5 verbs, DAG execution, and 22+ LLM providers."
      output:
        format: json_schema
        schema:
          type: object
          properties:
            title:
              type: string
              description: "A concise title for the content"
            summary:
              type: string
              description: "A 1-2 sentence summary"
            tags:
              type: array
              items:
                type: string
              description: "Relevant topic tags"
          required: [title, summary, tags]
```
</details>

**Key takeaway**: `output: format: json_schema` with a `schema:` definition guarantees structured output from any LLM.

### Exercise 2: Artifacts

**Objective**: Save LLM output to a file using artifacts.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: artifacts-demo
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output
  format: text
  manifest: true

tasks:
  - id: generate
    infer:
      prompt: "Write a haiku about workflow automation."
    artifact:
      path: output/haiku.txt
```
</details>

**Key takeaway**: `artifact: path:` writes task output to a file. `artifacts:` at the workflow level configures the output directory and manifest generation.

### Exercise 3: Schema Retry

**Objective**: Enable automatic retry when schema validation fails.

Create a task with a strict schema and `enable_retry: true`. The LLM must produce a JSON object with `sentiment` (enum: positive, negative, neutral) and `confidence` (number between 0 and 1).

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: schema-retry
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: classify
    infer:
      prompt: |
        Classify the sentiment of this review:
        "The product works great but shipping took forever."
      output:
        format: json_schema
        schema:
          type: object
          properties:
            sentiment:
              type: string
              enum: [positive, negative, neutral]
            confidence:
              type: number
              minimum: 0
              maximum: 1
          required: [sentiment, confidence]
        enable_retry: true
        max_retry_attempts: 3
```
</details>

**Key takeaway**: `enable_retry: true` makes schema validation self-healing. The LLM gets feedback on failures and fixes its output automatically.

---

## Level 6 -- Pay-Per-Dream

> *"7 providers. 0 lock-in. Their worst nightmare."*

### What You Will Learn

Level 6 teaches multi-provider workflows, local model execution, and advanced LLM configuration. You will switch between providers in a single workflow and learn to optimize for cost, speed, and capability.

### Concept: Multi-Provider Workflows

Different tasks have different requirements. A fast, cheap model handles simple tasks. A powerful, expensive model handles complex analysis. Nika lets you mix them in one workflow:

```yaml
schema: "nika/workflow@0.12"
workflow: multi-provider

tasks:
  - id: quick_draft
    provider: groq
    model: llama-3.3-70b-versatile
    infer: "List 5 blog post ideas about YAML."

  - id: deep_analysis
    depends_on: [quick_draft]
    provider: anthropic
    model: claude-sonnet-4-20250514
    with:
      ideas: $quick_draft
    infer:
      prompt: "Rank these ideas by potential impact:\n{{with.ideas}}"
      temperature: 0.2
```

The workflow-level `provider:` and `model:` serve as defaults. Task-level settings override them.

### Concept: Provider Landscape

Nika supports 9 providers. Here are the most common:

| Provider | Models | Best For |
|----------|--------|----------|
| Anthropic | Claude Sonnet/Opus/Haiku | Reasoning, analysis, coding |
| OpenAI | GPT-4o, GPT-4o-mini | General purpose |
| Google (Gemini) | Gemini 2.5 Flash | Fast, multimodal |
| Groq | Llama 4 Maverick | Speed (inference) |
| Mistral | Small/Large/Codestral | European, multilingual |
| DeepSeek | DeepSeek-V3, R1 | Cost-efficient reasoning |
| xAI | Grok | Real-time knowledge |
| Native | Any GGUF model | Local, private, free |

### Concept: Native Local Models

The `native` provider runs GGUF models locally. No API key, no network, no cost per token:

```yaml
provider: native
model: "~/.nika/models/mistral-7b.gguf"

tasks:
  - id: local_infer
    infer: "Explain the benefits of local inference."
```

For vision tasks, use HuggingFace models with ISQ quantization:
```bash
nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K
```

### Concept: System Prompts and Temperature

Fine-tune LLM behavior with `system:` prompts and `temperature:`:

```yaml
infer:
  system: "You are a senior Rust developer. Be concise and precise."
  prompt: "Review this code snippet."
  temperature: 0.1       # Very deterministic
  max_tokens: 500        # Cap response length
```

Temperature guide:
- **0.0-0.2**: Factual, deterministic (code review, data extraction)
- **0.3-0.5**: Balanced (general tasks)
- **0.7-1.0**: Creative (writing, brainstorming)

### Exercise 1: Multi-Provider

**Objective**: Use two different providers in one workflow.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: multi-provider
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: brainstorm
    provider: groq
    model: llama-3.3-70b-versatile
    infer: "Generate 5 creative project names for a workflow engine."

  - id: evaluate
    depends_on: [brainstorm]
    with:
      names: $brainstorm
    infer:
      prompt: "Evaluate each name for memorability and clarity:\n{{with.names}}"
      temperature: 0.2
```
</details>

### Exercise 2: Native Local

**Objective**: Configure a workflow for local model execution.

### Exercise 3: System Prompts

**Objective**: Use system prompts and temperature to control LLM behavior.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: system-prompts
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: formal
    infer:
      system: "You are a formal British butler. Speak with utmost propriety."
      prompt: "Describe a sunset."
      temperature: 0.3

  - id: casual
    infer:
      system: "You are a surfer dude from California. Keep it chill."
      prompt: "Describe a sunset."
      temperature: 0.9

  - id: compare
    depends_on: [formal, casual]
    with:
      butler: $formal
      surfer: $casual
    infer:
      prompt: |
        Compare these two descriptions:
        BUTLER: {{with.butler}}
        SURFER: {{with.surfer}}
        Which is more vivid? Why?
```
</details>

**Key takeaway**: System prompts define personality and constraints. Temperature controls creativity vs. determinism. Mix both for precise control.

---

## Level 7 -- Swiss Knife

> *"12 tools. No subscription. No terms of service."*

### What You Will Learn

Level 7 introduces the `invoke:` verb and the builtin tool ecosystem. You will use core tools for logging and assertions, file tools for reading and writing, and sub-workflows for composition.

### Concept: The `invoke:` Verb

The `invoke:` verb calls a tool by name with parameters. Builtin tools use the `nika:` namespace:

```yaml
tasks:
  - id: log_start
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Workflow started"
```

No network, no API key, no dependency. These tools run inside the Nika process.

### Concept: Core Builtin Tools

| Tool | Purpose | Key Parameters |
|------|---------|----------------|
| `nika:log` | Structured logging | `level`, `message` |
| `nika:emit` | Custom events | `name`, `payload` |
| `nika:assert` | Condition checks | `condition`, `message` |
| `nika:sleep` | Pause execution | `duration` (humantime: "1s", "500ms") |
| `nika:complete` | Signal agent completion | `result` |
| `nika:run` | Execute sub-workflow | `workflow` |

### Concept: File Tools

Five file tools provide cross-platform file operations:

| Tool | Purpose | Key Parameters |
|------|---------|----------------|
| `nika:read` | Read file contents | `file_path` |
| `nika:write` | Write/create files | `file_path`, `content` |
| `nika:edit` | Find-and-replace in files | `file_path`, `old_string`, `new_string` |
| `nika:glob` | Find files by pattern | `pattern`, `path` |
| `nika:grep` | Search file contents | `pattern`, `path` |

```yaml
tasks:
  - id: write_config
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/config.toml"
        content: |
          [server]
          port = 8080
          host = "localhost"

  - id: read_back
    depends_on: [write_config]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/config.toml"

  - id: update_port
    depends_on: [read_back]
    invoke:
      tool: "nika:edit"
      params:
        file_path: ".scratch/config.toml"
        old_string: "port = 8080"
        new_string: "port = 3000"
```

### Concept: Sub-Workflows

The `nika:run` tool executes another workflow file as a child. The parent waits for completion and receives the child's output:

```yaml
tasks:
  - id: run_child
    invoke:
      tool: "nika:run"
      params:
        workflow: "data-pipeline.nika.yaml"
```

This enables workflow composition: small, focused workflows combined into larger systems.

### Exercise 1: Core Builtins

**Objective**: Use `nika:sleep`, `nika:log`, `nika:emit`, and `nika:assert` in a sequential pipeline.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"

tasks:
  - id: pause
    invoke:
      tool: "nika:sleep"
      params:
        duration: "1s"

  - id: log_start
    depends_on: [pause]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Swiss Knife exercise started"

  - id: emit_progress
    depends_on: [log_start]
    invoke:
      tool: "nika:emit"
      params:
        name: "exercise_progress"
        payload:
          level: 7
          status: "in_progress"

  - id: check
    depends_on: [emit_progress]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "All core builtins executed successfully"
```
</details>

**Key takeaway**: Core builtins run inside the Nika process. No network, no cost, no dependencies.

### Exercise 2: File Tools

**Objective**: Chain all 5 file tools: write, read, edit, grep, glob.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"

tasks:
  - id: cleanup
    exec:
      command: rm -f .scratch/swiss-knife-report.txt

  - id: write_file
    depends_on: [cleanup]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/swiss-knife-report.txt"
        content: |
          Swiss Knife Report
          ==================
          Status: draft
          Tools tested: write, read, edit, grep, glob

  - id: read_file
    depends_on: [write_file]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/swiss-knife-report.txt"

  - id: edit_file
    depends_on: [read_file]
    invoke:
      tool: "nika:edit"
      params:
        file_path: ".scratch/swiss-knife-report.txt"
        old_string: "Status: draft"
        new_string: "Status: verified by nika:edit"

  - id: grep_file
    depends_on: [edit_file]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "verified by nika:edit"
        path: ".scratch"

  - id: glob_files
    depends_on: [edit_file]
    invoke:
      tool: "nika:glob"
      params:
        pattern: "*.txt"
        path: ".scratch"
```
</details>

**Key takeaway**: File tools are cross-platform and safe. No shelling out to `cat` and `grep`.

### Exercise 3: Sub-Workflows

**Objective**: Use `nika:run` to compose workflows from other workflows.

---

## Level 8 -- Gone Rogue

> *"You gave an LLM tools and told it to figure it out. What could go wrong?"*

### What You Will Learn

Level 8 introduces the most powerful verb: `agent:`. You will build autonomous LLMs that loop, call tools, validate their own output, and chain with other agents.

### Concept: The Agent Loop

The `agent:` verb creates a multi-turn conversation loop:

1. The agent receives a prompt (its mission)
2. The LLM decides which tool to call (or responds with text)
3. The tool result is fed back to the LLM
4. The loop continues until: `nika:complete` is called, `max_turns` is reached, or `token_budget` is exhausted

```yaml
tasks:
  - id: researcher
    agent:
      prompt: |
        You are a research agent. Your mission:
        1. Log "Starting research" using nika_log
        2. Research 3 facts about YAML
        3. Call nika_complete with your findings
      tools: [builtin]         # All nika:* tools available
      max_turns: 10            # Safety limit
      max_tokens: 800          # Per-response limit
      token_budget: 8000       # Total budget
      tool_choice: auto        # LLM decides when to use tools
```

### Concept: Tool Lists

The `tools:` field specifies which tools the agent can use:

| Value | Meaning |
|-------|---------|
| `[builtin]` | All `nika:*` tools (log, emit, assert, sleep, complete, file tools) |
| `[nika:log, nika:complete]` | Only specific tools |
| `[builtin, mcp:server_name]` | Builtin + MCP server tools |

### Concept: Completion Modes

The `completion:` block controls how the agent signals it is done:

```yaml
completion:
  mode: explicit          # Must call nika:complete
  signal:
    tool: nika:complete
    fields:
      required: [result]  # nika:complete must include 'result'
      optional: [confidence]
```

Three modes:
- **explicit**: Agent must call `nika:complete` (recommended for complex tasks)
- **natural**: Completes when the LLM stops making tool calls
- **pattern**: Completes when output matches a regex

### Concept: Guardrails

Guardrails validate agent output before accepting it. Four types:

```yaml
guardrails:
  # Word count bounds
  - type: length
    min_words: 100
    max_words: 500
    on_failure: retry       # Ask agent to fix it

  # Pattern matching
  - type: regex
    pattern: "^## "
    message: "Must start with a markdown heading"
    on_failure: retry

  # JSON Schema validation
  - type: schema
    schema:
      type: object
      properties:
        title: { type: string }
      required: [title]
    on_failure: fail        # Stop the agent

  # LLM-based evaluation
  - type: llm
    prompt: "Is this output professional and accurate? Answer YES or NO."
    on_failure: escalate    # Flag for human review
```

The `on_failure:` action determines what happens when a guardrail fails:
- **retry**: Feed the failure back to the agent and ask it to fix the output
- **fail**: Stop the agent with an error
- **escalate**: Flag for human review

### Concept: Cost Control Limits

Prevent runaway agents from burning through tokens:

```yaml
limits:
  max_turns: 20            # Hard cap on loop iterations
  max_cost_usd: 0.50       # Dollar cost ceiling
  max_duration_secs: 120   # Wall-clock timeout
```

These limits are independent of guardrails. An agent can pass all guardrails but still be stopped by limits.

### Exercise 1: Basic Agent

**Objective**: Create your first autonomous agent with builtin tools.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: analyzer
    agent:
      prompt: |
        You are a project analysis agent. Your mission:
        1. Log "Starting analysis" using nika_log
        2. Analyze the Nika workflow engine architecture:
           - 5 verbs: infer, exec, fetch, invoke, agent
           - DAG scheduler for parallel execution
           - YAML DSL with schema validation
        3. Log your key findings
        4. Call nika_complete with a brief summary report
      tools: [builtin]
      max_turns: 8
      max_tokens: 800
      token_budget: 6000
      tool_choice: auto
    artifact:
      path: output/analysis-report.md
```
</details>

**Key takeaway**: `agent:` creates an autonomous loop. `tools: [builtin]` gives access to all `nika:*` tools. Safety limits prevent runaway behavior.

### Exercise 2: Agent Skills and Completion

**Objective**: Chain two agents with different completion modes.

Agent 1 researches use cases with `completion: mode: explicit`. Agent 2 receives Agent 1's output via `with:` bindings and refines it.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: explicit_agent
    agent:
      prompt: |
        Research 3 innovative use cases for YAML workflow engines.
        For each, provide a title and 2-sentence description.
        Log each use case with nika_log.
        When done, call nika_complete with your ranked list.
      tools: [builtin]
      max_turns: 6
      max_tokens: 800
      token_budget: 5000
      completion:
        mode: explicit

  - id: refine_agent
    depends_on: [explicit_agent]
    with:
      research: $explicit_agent
    agent:
      prompt: |
        Review these use cases and pick the single best one:
        {{with.research}}
        Expand it into a 200-word pitch.
        Call nika_complete with your final pitch.
      tools: [builtin]
      max_turns: 4
      max_tokens: 600
      token_budget: 3000
      completion:
        mode: explicit
        signal:
          tool: nika:complete
          fields:
            required: [result]
            optional: [confidence]
    artifact:
      path: output/best-use-case-pitch.md
```
</details>

**Key takeaway**: Agents chain through `with:` bindings. Completion modes control the exit condition. The `signal:` block enforces required fields.

### Exercise 3: Agent Guardrails

**Objective**: Add guardrails (length + regex) and cost control limits.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: writer_agent
    agent:
      prompt: |
        Write a technical article about declarative workflow engines.
        Requirements:
        - Between 200 and 400 words
        - Must start with "In the era of"
        - Must include at least one code example
        Log your progress, then call nika_complete with the article.
      tools: [builtin]
      max_turns: 8
      max_tokens: 1000
      token_budget: 8000
      guardrails:
        - type: length
          min_words: 200
          max_words: 400
          on_failure: retry
        - type: regex
          pattern: "^In the era of"
          message: "Article must start with 'In the era of'"
          on_failure: retry
      limits:
        max_turns: 8
        max_cost_usd: 0.10
        max_duration_secs: 60
      completion:
        mode: explicit
    artifact:
      path: output/declarative-engines-article.md
```
</details>

**Key takeaway**: Guardrails validate output quality. `on_failure: retry` makes the agent self-correcting. Cost limits prevent budget overruns.

---

## Phase 2 Checkpoint

After completing Levels 5-8, you should be able to:

- Force LLMs to return validated JSON with `output: format: json_schema`
- Save output to files with `artifact:`
- Enable automatic retry on schema validation failure
- Switch between providers and models per task
- Use all 12+ core builtin tools via `invoke:`
- Compose workflows from sub-workflows with `nika:run`
- Build autonomous agents with the `agent:` verb
- Chain agents through `with:` bindings
- Add guardrails (length, regex, schema, LLM) to validate agent output
- Control costs with `max_turns`, `token_budget`, and `max_cost_usd`

### Checkpoint Project: Code Review Assistant

Build a workflow that:
1. Captures the git diff with `exec: git diff HEAD~1`
2. Analyzes it with an agent that has guardrails (output must be valid JSON with `severity`, `category`, `description` fields)
3. Generates a structured review report with an artifact
4. Uses a second agent to write a summary for the PR description

---

*"Their low-code tools give you 3 transforms behind a $49/month paywall. You just got the full catalog."*
