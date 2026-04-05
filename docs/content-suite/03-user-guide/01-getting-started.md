# Getting Started with Nika

Nika is a semantic YAML workflow engine for AI tasks. It lets you orchestrate LLM calls, shell commands, HTTP requests, MCP tool invocations, and multi-turn agent loops -- all from declarative YAML files. This guide takes you from zero to running your first workflow in under 10 minutes.

## What is Nika?

Nika provides five semantic verbs that cover every common AI automation pattern:

| Verb | Purpose | Example Use Case |
|------|---------|------------------|
| `infer:` | LLM text generation | Summarize text, generate code, answer questions |
| `exec:` | Shell command execution | Run scripts, process files, call system tools |
| `fetch:` | HTTP requests | Call APIs, scrape websites, download data |
| `invoke:` | MCP tool calls | Use builtin tools or connect to external MCP servers |
| `agent:` | Multi-turn agent loops | Autonomous research, code generation, complex reasoning |

Workflows are written in YAML with the `.nika.yaml` extension and follow the `nika/workflow@0.12` schema. Tasks form a Directed Acyclic Graph (DAG) -- they can run in parallel, pass data to each other through bindings, and be validated before execution.

## Installation

### Using Homebrew (macOS)

```bash
brew tap supernovae-st/tap
brew install nika
```

### From Source (Rust toolchain required)

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika
cargo install --path tools/nika
```

### Verify Installation

```bash
nika --version
```

You should see output like:

```
nika 0.42.0
```

## Your First 60 Seconds

### 1. Initialize a project

Create a new directory and initialize Nika:

```bash
mkdir my-nika-project
cd my-nika-project
nika init
```

This creates a `.nika/` directory with configuration files, and optionally generates example workflows.

### 2. Create your first workflow

Create a file called `hello.nika.yaml`:

```yaml
schema: nika/workflow@0.12
workflow: hello-world

tasks:
  - id: greet
    exec: "echo 'Hello from Nika!'"
```

### 3. Validate it

```bash
nika check hello.nika.yaml
```

Expected output:

```
  Nika Check — hello-world
  ─────────────────────────────────
  Schema:    nika/workflow@0.12 ✓
  Tasks:     1 task
  DAG:       Valid (no cycles)
  Bindings:  Valid

  ✓ All checks passed
```

### 4. Run it

```bash
nika run hello.nika.yaml
```

Or use the shorthand (just pass the file directly):

```bash
nika hello.nika.yaml
```

Expected output:

```
  ┌─ hello-world ─────────────────────────────────┐
  │ Schema: nika/workflow@0.12                     │
  │ Tasks:  1                                      │
  └────────────────────────────────────────────────┘

  ✓ greet ─── 0.02s
    Hello from Nika!

  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ✓ 1/1 tasks completed in 0.03s
```

Congratulations -- you just ran your first Nika workflow.

## Understanding What Happened

When you ran `nika hello.nika.yaml`, Nika performed these steps:

1. **Parsed** the YAML file and validated the `nika/workflow@0.12` schema
2. **Built the DAG** -- determined which tasks depend on which (in this case, just one task)
3. **Analyzed bindings** -- checked that all `{{template}}` references and `$task_id` references are valid
4. **Executed** each task in dependency order, capturing stdout as the output
5. **Reported** results with timing and status

This parse-validate-execute pipeline catches most errors before any work is done. That is why `nika check` is so useful -- it runs steps 1-3 without spending time or API credits on step 4.

## Adding a Second Task

Workflows become powerful when tasks depend on each other. Let's create a workflow where one task produces data and another consumes it:

```yaml
schema: nika/workflow@0.12
workflow: two-step

tasks:
  - id: generate
    exec: "echo 'The quick brown fox'"

  - id: transform
    depends_on: [generate]
    with:
      text: $generate
    exec: "echo '{{with.text}}' | tr '[:lower:]' '[:upper:]'"
```

Run it:

```bash
nika two-step.nika.yaml
```

Output:

```
  ✓ generate ─── 0.01s
    The quick brown fox

  ✓ transform ─── 0.02s
    THE QUICK BROWN FOX
```

Here is what happened:

1. **`generate`** ran first and produced "The quick brown fox"
2. **`transform`** declared a dependency on `generate` via `depends_on:`
3. The `with:` block bound the output of `generate` to the alias `text` using `$generate`
4. The template `{{with.text}}` was replaced with the actual output at runtime
5. The piped shell command converted it to uppercase

This is the core pattern: produce data, bind it, use it in templates.

## Using an LLM (infer)

To use LLM generation, you need an API key for at least one provider. Set it as an environment variable:

```bash
# Pick one (or several)
export ANTHROPIC_API_KEY="sk-ant-..."     # Claude
export OPENAI_API_KEY="sk-..."            # OpenAI
export MISTRAL_API_KEY="..."              # Mistral
export GROQ_API_KEY="gsk_..."             # Groq
export GEMINI_API_KEY="..."               # Google Gemini
```

Now create a workflow that calls an LLM:

```yaml
schema: nika/workflow@0.12
workflow: first-infer
provider: anthropic

tasks:
  - id: haiku
    infer:
      prompt: "Write a haiku about programming."
      temperature: 0.8
```

Run it:

```bash
nika first-infer.nika.yaml
```

Example output:

```
  ✓ haiku ─── 1.23s
    Loops within the code
    Bugs that hide in shadowed lines
    Compiles — pure delight
```

You can override the provider at runtime:

```bash
nika run first-infer.nika.yaml --provider openai
```

### Working with Transforms

You can transform bound values using pipe operators. This is one of Nika's most powerful features:

```yaml
schema: nika/workflow@0.12
workflow: transform-demo

tasks:
  - id: source
    exec: "echo '  Hello, World!  '"

  - id: clean
    depends_on: [source]
    with:
      raw: $source | trim           # Remove whitespace
      upper: $source | trim | upper # Trim, then uppercase
      lower: $source | trim | lower # Trim, then lowercase
      length: $source | trim | length # Count characters
    exec:
      command: |
        echo "Raw:    [{{with.raw}}]"
        echo "Upper:  [{{with.upper}}]"
        echo "Lower:  [{{with.lower}}]"
        echo "Length: {{with.length}}"
      shell: true
```

Output:

```
  ✓ clean ─── 0.02s
    Raw:    [Hello, World!]
    Upper:  [HELLO, WORLD!]
    Lower:  [hello, world!]
    Length: 13
```

Nika includes 30+ transforms for strings, arrays, objects, numbers, and type conversions. See [Workflow Patterns](04-workflow-patterns.md) for the complete catalog.

## Fetching Data from the Web

The `fetch:` verb makes HTTP requests and can extract structured content:

```yaml
schema: nika/workflow@0.12
workflow: web-fetch

tasks:
  - id: get_page
    fetch:
      url: "https://example.com"
      extract: markdown

  - id: show_length
    depends_on: [get_page]
    with:
      page: $get_page
    exec: "echo 'Fetched {{with.page | length}} characters of markdown'"
```

This fetches a webpage, converts it to clean Markdown, and reports the content length.

## Project Structure

After running `nika init`, your project looks like this:

```
my-project/
├── .nika/
│   ├── config.toml        # Project configuration
│   ├── traces/             # Execution traces
│   └── media/              # Content-addressable store
├── hello.nika.yaml         # Your workflows
└── ...
```

Key conventions:
- Workflow files use the `.nika.yaml` extension
- The `.nika/` directory stores project configuration and runtime data
- Traces are saved after each run for debugging and auditing

## Essential CLI Commands

Here is a quick reference of the commands you will use most often:

### Running Workflows

```bash
nika workflow.nika.yaml                # Run directly (shorthand)
nika run workflow.nika.yaml            # Explicit run command
nika run workflow.nika.yaml -p openai  # Override provider
nika run workflow.nika.yaml -m gpt-4o  # Override model
```

### Validation

```bash
nika check workflow.nika.yaml          # Validate syntax, DAG, bindings
nika check workflow.nika.yaml --strict # Also test MCP connections
```

### Interactive TUI

```bash
nika ui                                # Launch terminal UI
nika chat                              # Chat mode (shortcut)
nika studio workflow.nika.yaml         # Studio editor (shortcut)
```

### Provider Management

```bash
nika provider list                     # Show providers and API key status
nika keys set anthropic            # Store key in system keychain
nika provider test openai              # Test provider connection
```

### Project Initialization

```bash
nika init                              # Interactive wizard
nika init --course                     # Generate learning course
```

### System Health

```bash
nika doctor                            # Quick health check
nika doctor --full                     # Full diagnostics
nika features                          # Show compiled features
```

## Understanding Workflow Output

When you run a workflow, Nika displays:

1. **Header** -- Workflow name, schema version, task count
2. **Task results** -- Each task shows status, duration, and output
3. **Summary** -- Total tasks completed and wall-clock time

Status icons:
- `✓` -- Task completed successfully
- `✗` -- Task failed (error details shown)
- `○` -- Task skipped (dependency failed)
- `⟳` -- Task is retrying

## Global Flags

These flags work with any command:

```bash
nika -v workflow.nika.yaml         # Verbose (info level)
nika -vv workflow.nika.yaml        # Debug level
nika -vvv workflow.nika.yaml       # Trace level (very detailed)
nika -q workflow.nika.yaml         # Quiet (errors only)
nika --color never check file.yaml # Disable colors
nika --detail min run file.yaml    # Minimal output
nika --detail json run file.yaml   # JSON output
```

## What to Learn Next

Now that you can create and run workflows, here is the recommended learning path:

1. **[Your First Workflow](02-your-first-workflow.md)** -- Build a real multi-task workflow step by step
2. **[Provider Setup Guide](03-provider-setup-guide.md)** -- Configure all 7 LLM providers
3. **[Workflow Patterns](04-workflow-patterns.md)** -- Master chaining, parallelism, and data flow
4. **[The Course](10-course-guide.md)** -- Run `nika init --course` for 44 hands-on exercises

Or jump directly to the verb you need:
- **[infer: Guide](05-infer-verb-guide.md)** -- LLM generation, vision, structured output
- **[fetch: Guide](06-fetch-verb-guide.md)** -- HTTP requests and content extraction
- **[exec/invoke/agent Guide](07-exec-invoke-agent-guide.md)** -- Shell, tools, and agents

If something goes wrong, check the **[Troubleshooting Guide](12-troubleshooting.md)** or run `nika doctor` for automated diagnostics.

## The nika new Command

Quickly scaffold new workflows from templates or an interactive wizard:

### Interactive Wizard

```bash
nika new
```

The wizard asks you for:
1. Workflow name
2. Primary verb (infer, exec, fetch, invoke, agent)
3. Provider preference
4. Output format
5. Optional features (MCP, artifacts, includes)

### From Template

```bash
# List available templates
nika new --list

# Create from a specific template
nika new --template blog-generator my-blog

# Create with a specific verb
nika new --verb fetch my-scraper

# Create with options
nika new --verb infer --provider openai --with-artifacts my-analysis
```

Templates provide a complete, working starting point that you can customize.

### Output to a Specific Directory

```bash
nika new --template agent-research -d ./workflows/ my-research
```

## System Health with nika doctor

The `nika doctor` command runs a comprehensive health check:

```bash
nika doctor
```

```
  Nika Doctor — System Check
  ─────────────────────────────

  ✓ Project       .nika/ directory found
  ✓ Config        config.toml valid
  ✓ Anthropic     ANTHROPIC_API_KEY set (sk-ant-...)
  ✓ OpenAI        OPENAI_API_KEY set (sk-...)
  ⚠ Mistral       MISTRAL_API_KEY not set
  ✓ Version       Nika v0.42.0
  ✓ Traces        4 traces, 1.2 MB

  3 checks passed, 1 warning
```

For full diagnostics including MCP server connectivity:

```bash
nika doctor --full
```

## Execution Traces

Every workflow run produces a trace file that records detailed execution data:

```bash
# List recent traces
nika trace list

# Show trace details
nika trace show <trace-id>

# Export for analysis
nika trace export <trace-id> --format json
nika trace export <trace-id> --format yaml
```

Traces include:
- Task execution order and timing
- Input/output values for each task
- LLM token usage and cost estimates
- Error messages and retry attempts
- DAG execution path

This is invaluable for debugging, cost tracking, and workflow optimization.

## Quick Reference Card

```
WORKFLOW STRUCTURE:
  schema: nika/workflow@0.12     (required)
  workflow: name                 (optional, defaults to filename)
  provider: anthropic            (default LLM provider)
  model: claude-sonnet-4-6    (default model)
  tasks: [...]                   (required, ordered task list)

TASK STRUCTURE:
  - id: unique-name              (required)
    <verb>: ...                  (one of: infer, exec, fetch, invoke, agent)
    with: { alias: $task }       (bind data from other tasks)
    depends_on: [task_id]        (pure ordering dependency)

DATA FLOW:
  with: { data: $source }       → binds output of "source" task
  {{with.data}}                  → template reference in strings
  $source | trim | upper         → pipe transforms

FIVE VERBS:
  exec: "echo hello"             → shell command
  fetch: { url: "..." }          → HTTP request
  infer: { prompt: "..." }       → LLM generation
  invoke: "nika:dimensions"      → MCP tool call
  agent: { prompt: "..." }       → multi-turn loop
```
