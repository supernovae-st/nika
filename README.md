<div align="center">

# 🦀 Nika

**Native Intelligence Kernel Agent**

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0-green.svg)](CHANGELOG.md)

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
| **3 Actions** | `infer:` (LLM) • `exec:` (shell) • `fetch:` (HTTP) |
| **DAG Execution** | Parallel processing when dependencies allow |
| **Data Flow** | `use:` blocks + `{{use.alias}}` templates |
| **Providers** | Claude, OpenAI, Mock |

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

## Providers

| Provider | Env Variable | Models |
|----------|--------------|--------|
| `claude` | `ANTHROPIC_API_KEY` | claude-sonnet-4-*, claude-haiku-* |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4o-mini |
| `mock` | - | (testing) |

## Commands

```bash
nika run <workflow.yaml>      # Execute workflow
nika validate <workflow.yaml> # Validate only
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

## Documentation

See [spec/SPEC.md](spec/SPEC.md) for full specification.

---

<div align="center">

**[SuperNovae Studio](https://supernovae.studio)**

AGPL-3.0 License • Made with 🦀 in Rust

</div>
