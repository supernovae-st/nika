<div align="center">

# 🦀 Nika

**Native Intelligence Kernel Agent**

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.7.1-green.svg)](CHANGELOG.md)

*DAG workflow runner for AI tasks*

[Installation](#installation) • [Quick Start](#quick-start) • [Tutorial](#tutorial) • [Documentation](#documentation)

</div>

---

Nika executes YAML-defined workflows as directed acyclic graphs (DAGs). Supports LLM inference, shell commands, and HTTP requests with data flow between tasks.

## Installation

```bash
cargo install --path .
```

## Quick Start

```yaml
# hello.nika.yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: greet
    infer:
      prompt: "Say hello in French"
```

```bash
export ANTHROPIC_API_KEY=your-key
nika run hello.nika.yaml
```

## Features

| Feature | Description |
|---------|-------------|
| **5 Actions** | `infer:` (LLM) • `exec:` (shell) • `fetch:` (HTTP) • `invoke:` (MCP) • `agent:` (agentic) |
| **DAG Execution** | Parallel processing when dependencies allow |
| **Data Flow** | `use:` blocks + `{{use.alias}}` templates |
| **for_each** | Parallel iteration over arrays (v0.3) |
| **MCP Integration** | Connect to MCP servers for tool calling (v0.2) |
| **Providers** | rig-core (Claude, OpenAI, 20+) via `RigProvider` (v0.4) |
| **TUI** | Real-time workflow visualization (v0.3) |

## Example

```yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: weather
    infer:
      prompt: "Get Paris weather as JSON: {summary, temp}"
    output:
      format: json

  - id: recommend
    use:
      forecast: weather.summary
      temp: weather.temp ?? 20
    infer:
      prompt: |
        Weather: {{use.forecast}} at {{use.temp}}C
        Suggest an activity.

flows:
  - source: weather
    target: recommend
```

## Actions

### infer (LLM)

```yaml
infer:
  prompt: "Your prompt"
  provider: openai   # optional
  model: gpt-4o-mini # optional
```

### exec (shell)

```yaml
exec:
  command: "npm run build"
```

### fetch (HTTP)

```yaml
fetch:
  url: "https://api.example.com"
  method: POST
  headers:
    Authorization: "Bearer {{use.token}}"
```

### invoke (MCP) — v0.2

Call tools from MCP servers.

```yaml
invoke:
  mcp: novanet
  tool: novanet_generate
  params:
    entity: "qr-code"
    locale: "fr-FR"
```

### agent (Agentic Loop) — v0.2

Run an agentic loop with tool access.

```yaml
agent:
  prompt: "Research and summarize recent AI papers"
  tools:
    - web_search
    - read_file
  max_iterations: 10
```

## MCP Configuration — v0.2

Define MCP servers inline in your workflow:

```yaml
schema: "nika/workflow@0.2"

# Run from nika-dev/tools/nika/ directory
mcp:
  novanet:
    command: cargo
    args:
      - run
      - --manifest-path
      - ../../../novanet-dev/tools/novanet-mcp/Cargo.toml
    env:
      NOVANET_MCP_NEO4J_URI: bolt://localhost:7687
      NOVANET_MCP_NEO4J_USER: neo4j
      NOVANET_MCP_NEO4J_PASSWORD: novanetpassword

tasks:
  - id: generate
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "landing-page"
```

## for_each Parallelism — v0.3

Execute a task once per item in an array, all in parallel:

```yaml
schema: "nika/workflow@0.3"

tasks:
  - id: process_locales
    for_each: ["en-US", "fr-FR", "de-DE", "es-ES"]
    as: locale
    exec:
      command: "echo Processing {{use.locale}}"
```

- `for_each:` — Array of values to iterate over
- `as:` — Variable name (defaults to `item` if omitted)
- Access via `{{use.<var>}}` in templates

### for_each with MCP

```yaml
tasks:
  - id: generate_content
    for_each: ["en-US", "fr-FR", "de-DE"]
    as: locale
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "qr-code"
        locale: "{{use.locale}}"
```

## Providers (v0.4 — rig-core)

Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) for LLM providers. All providers are accessed via `RigProvider`.

| Provider | Env Variable | Models |
|----------|--------------|--------|
| `claude` | `ANTHROPIC_API_KEY` | claude-sonnet-4-*, claude-haiku-* |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4o-mini |
| `mock` | - | (testing) |

See [rig-core docs](https://docs.rs/rig-core) for 20+ additional providers.

## File Conventions

All Nika workflow files **MUST** use the `.nika.yaml` extension:

| Pattern | Status |
|---------|--------|
| `workflow.nika.yaml` | ✅ Correct |
| `workflow.yaml` | ❌ Wrong (ambiguous) |
| `workflow.nika` | ❌ Wrong (not YAML) |

### Schema Validation

Workflows are validated against `schemas/nika-workflow.schema.json`:

```bash
# Validate single file
nika validate workflow.nika.yaml

# Validate all examples
nika validate examples/
```

### IDE Support

VS Code users get automatic schema validation and autocompletion via `.vscode/settings.json`. Install the [YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml).

### Linting

```bash
yamllint -c .yamllint.yaml **/*.nika.yaml
```

## Commands

```bash
nika run <workflow.nika.yaml>      # Execute workflow
nika validate <workflow.nika.yaml> # Validate only
nika tui <workflow.nika.yaml>      # Interactive TUI (v0.3)
nika trace list                    # List traces
nika trace show <id>               # Show trace details
```

## Tutorial

### Use Case 1: Code Review Automation

Analyze git changes and generate a code review with AI.

```yaml
# code-review.nika.yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: get_diff
    exec:
      command: "git diff HEAD~1"

  - id: review
    use:
      diff: get_diff
    infer:
      prompt: |
        Review this code diff. List:
        1. Potential bugs
        2. Security issues
        3. Improvements

        ```diff
        {{use.diff}}
        ```

flows:
  - source: get_diff
    target: review
```

```bash
nika run code-review.nika.yaml
```

### Use Case 2: API Data Pipeline

Fetch data from an API, process it with AI, and save results.

```yaml
# api-pipeline.nika.yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: fetch_users
    fetch:
      url: "https://jsonplaceholder.typicode.com/users"
    output:
      format: json

  - id: analyze
    use:
      users: fetch_users
    infer:
      prompt: |
        Analyze these users and return JSON with:
        {"total": N, "cities": ["city1", ...], "summary": "..."}

        Data: {{use.users}}
    output:
      format: json

  - id: save
    use:
      report: analyze
    exec:
      command: "echo '{{use.report}}' > report.json"

flows:
  - source: fetch_users
    target: analyze
  - source: analyze
    target: save
```

### Use Case 3: Parallel DevOps Tasks

Run multiple checks in parallel, then aggregate results.

```yaml
# devops-check.nika.yaml
schema: "nika/workflow@0.1"

tasks:
  - id: check_disk
    exec:
      command: "df -h / | tail -1 | awk '{print $5}'"

  - id: check_memory
    exec:
      command: "top -l 1 | grep PhysMem | awk '{print $2}'"

  - id: check_docker
    exec:
      command: "docker ps --format '{{.Names}}' | wc -l | tr -d ' '"

  - id: report
    use:
      disk: check_disk
      mem: check_memory
      containers: check_docker
    exec:
      command: |
        echo "=== System Report ==="
        echo "Disk usage: {{use.disk}}"
        echo "Memory used: {{use.mem}}"
        echo "Docker containers: {{use.containers}}"

flows:
  - source: [check_disk, check_memory, check_docker]
    target: report
```

### Use Case 4: Content Generation Pipeline

Generate content with multiple AI steps.

```yaml
# content-gen.nika.yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: outline
    infer:
      prompt: |
        Create a blog post outline about "Rust for AI".
        Return JSON: {"title": "...", "sections": ["...", "..."]}
    output:
      format: json

  - id: write_intro
    use:
      title: outline.title
    infer:
      prompt: "Write a 2-sentence intro for: {{use.title}}"

  - id: write_conclusion
    use:
      title: outline.title
    infer:
      prompt: "Write a 2-sentence conclusion for: {{use.title}}"

  - id: assemble
    use:
      title: outline.title
      intro: write_intro
      conclusion: write_conclusion
    exec:
      command: |
        echo "# {{use.title}}"
        echo ""
        echo "{{use.intro}}"
        echo ""
        echo "[... content ...]"
        echo ""
        echo "{{use.conclusion}}"

flows:
  - source: outline
    target: [write_intro, write_conclusion]
  - source: [write_intro, write_conclusion]
    target: assemble
```

This creates a diamond DAG: `outline` → parallel `write_intro` + `write_conclusion` → `assemble`.

### Use Case 5: Multi-Locale Content Generation (v0.3)

Generate content for multiple locales in parallel using `for_each`.

```yaml
# multi-locale.nika.yaml
schema: "nika/workflow@0.3"
provider: claude

# Run from nika-dev/tools/nika/ directory
mcp:
  novanet:
    command: cargo
    args:
      - run
      - --manifest-path
      - ../../../novanet-dev/tools/novanet-mcp/Cargo.toml
    env:
      NOVANET_MCP_NEO4J_URI: bolt://localhost:7687

tasks:
  - id: generate_pages
    for_each: ["en-US", "fr-FR", "de-DE", "es-ES", "ja-JP"]
    as: locale
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "landing-page"
        locale: "{{use.locale}}"
        forms: ["title", "description", "cta"]
```

All 5 locales process in parallel, each calling NovaNet's MCP server.

## Documentation

See [spec/SPEC.md](spec/SPEC.md) for full specification.

---

<div align="center">

**[SuperNovae Studio](https://supernovae.studio)**

AGPL-3.0 License • Made with 🦀 in Rust

</div>
