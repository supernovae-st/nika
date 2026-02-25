<div align="center">

<!-- Animated Header with Butterfly Logo -->
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/supernovae-st/nika/main/assets/nika-logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/supernovae-st/nika/main/assets/nika-logo.svg">
  <img src="https://raw.githubusercontent.com/supernovae-st/nika/main/assets/nika-logo.svg" alt="Nika Logo" width="160" height="160">
</picture>

# Nika

### Open-Source Agentic CLI

<sup>Transform YAML into intelligent AI workflows</sup>

<!-- Primary Badges -->
[![Version](https://img.shields.io/badge/v0.12.0-7c3aed?style=for-the-badge&logo=semver&logoColor=white)](CHANGELOG.md)
[![Rust](https://img.shields.io/badge/rust_1.86+-f97316?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/AGPL--3.0-22c55e?style=for-the-badge&logo=gnu&logoColor=white)](LICENSE)
[![Website](https://img.shields.io/badge/nika.sh-8b5cf6?style=for-the-badge&logo=safari&logoColor=white)](https://nika.sh)

<!-- GitHub Badges -->
[![CI](https://img.shields.io/github/actions/workflow/status/supernovae-st/nika/ci.yml?style=flat-square&logo=github&label=CI)](https://github.com/supernovae-st/nika/actions)
[![Stars](https://img.shields.io/github/stars/supernovae-st/nika?style=flat-square&logo=github&label=Stars)](https://github.com/supernovae-st/nika/stargazers)
[![Tests](https://img.shields.io/badge/tests-2,720+_passing-10b981?style=flat-square&logo=checkmarx)](https://github.com/supernovae-st/nika/actions)
[![LOC](https://img.shields.io/badge/LOC-106k-0ea5e9?style=flat-square&logo=codeclimate)](https://github.com/supernovae-st/nika)

<!-- Feature Badges -->
[![Providers](https://img.shields.io/badge/LLM_providers-6-ec4899?style=flat-square&logo=openai)](#-providers)
[![Views](https://img.shields.io/badge/TUI_views-6-f59e0b?style=flat-square&logo=windowsterminal)](#-studio-tui)
[![Widgets](https://img.shields.io/badge/widgets-39-06b6d4?style=flat-square&logo=react)](#-chat-dag-widgets)

<!-- Navigation -->
<p>
<a href="#-quick-start">Quick Start</a> &bull;
<a href="#-the-5-verbs">5 Verbs</a> &bull;
<a href="#-chat-dag-widgets">Chat DAG</a> &bull;
<a href="#-studio-tui">Studio TUI</a> &bull;
<a href="#-providers">Providers</a> &bull;
<a href="#-examples">Examples</a> &bull;
<a href="#-documentation">Docs</a>
</p>

---

**Nika** executes YAML-defined workflows as **directed acyclic graphs (DAGs)**.<br>
Connect LLMs, shell commands, HTTP APIs, and MCP tools in a single declarative file.

</div>

<br>

<!-- TUI Screenshot as ASCII Art -->
```
+-------------------------------------------------------------------------------------+
|  Nika Studio                                               v0.12.0  Ctrl+K ?   |
+-------------------------------------------------------------------------------------+
| +- Files ---------------+ +- Editor ------------------------------------------------+ |
| | > workflows/          | |  1 | schema: "nika/workflow@0.12"                      | |
| |     deploy.nika.yaml  | |  2 | provider: claude                                  | |
| |   > review.nika.yaml  | |  3 |                                                   | |
| |     test.nika.yaml    | |  4 | tasks:                                            | |
| +- DAG ------------------| |  5 |   - id: research                                 | |
| |                       | |  6 |     agent:                                        | |
| |  [research] ----+     | |  7 |       prompt: "Find AI papers"                    | |
| |        |        |     | |  8 |       mcp: [web_search]                           | |
| |   [analyze]  [eval]   | |  9 |                                                   | |
| |        |        |     | | 10 |   - id: analyze                                   | |
| |     [report]<--+      | | 11 |     use: { papers: @1 }                           | |
| +-----------------------+ +-----------------------------------------------------------+ |
| +- Chat DAG ---------------------------------------------------------------------+ |
| | @1 research --> @2 analyze --> @4 report                                       | |
| |       \                                                                        | |
| |        `-----> @3 evaluate ---'                                                | |
| +--------------------------------------------------------------------------------+ |
+-------------------------------------------------------------------------------------+
|  [a]Chat  [h]Home  [s]Studio  [m]Monitor  [,]Settings  [?]Help  |  claude  |  4 tasks |
+-------------------------------------------------------------------------------------+
```

<br>

## What's New in v0.12.0

<table>
<tr>
<td width="50%">

**Chat-as-DAG Architecture**

- **@mention Bindings** &mdash; Reference messages with `@1`, `@last`, `@all`
- **StableGraph** &mdash; Stable NodeIndex preserved after deletion
- **ChatWorkflow** &mdash; Thread-safe DAG wrapper for messages
- **4 Chat DAG Widgets** &mdash; Visual DAG in terminal

</td>
<td width="50%">

**6-Views TUI**

- **Settings View** &mdash; Provider config, themes, preferences
- **Help View** &mdash; Keyboard shortcuts, documentation
- **MonitorView** &mdash; Real-time execution monitoring
- **6 Builtin Tools** &mdash; `nika:sleep`, `nika:log`, `nika:assert`...

</td>
</tr>
</table>

> **Upgrade:** `cargo install --git https://github.com/supernovae-st/nika.git --force`

<br>

## Why Nika?

```mermaid
flowchart LR
    subgraph Problem["The Problem"]
        P1["LLM calls buried in code"]
        P2["Custom glue for each integration"]
        P3["No standard AI workflow format"]
        P4["Hard to debug pipelines"]
    end

    subgraph Solution["Nika Solution"]
        S1["YAML = version-controlled"]
        S2["5 verbs cover everything"]
        S3["Full NDJSON observability"]
        S4["Native MCP client"]
    end

    P1 -.->|"transforms into"| S1
    P2 -.->|"replaced by"| S2
    P3 -.->|"becomes"| S3
    P4 -.->|"solved with"| S4

    style Problem fill:#fecaca,stroke:#dc2626
    style Solution fill:#bbf7d0,stroke:#16a34a
```

<br>

### How Nika Compares

<div align="center">

| | **Nika** | LangChain | Prefect | Temporal |
|:---|:---:|:---:|:---:|:---:|
| **Config Format** | YAML | Python | Python | Code |
| **Learning Curve** | 5 min | Hours | Hours | Days |
| **LLM Native** | Built-in | Core | Add-on | Add-on |
| **MCP Support** | Native | No | No | No |
| **Observability** | NDJSON | LangSmith | UI | UI |
| **Self-hosted** | Single Binary | Yes | Cloud | Cloud |
| **Dependencies** | 0 | Many | Many | Many |
| **Chat-as-DAG** | Native | No | No | No |

</div>

> **TL;DR:** Nika = single binary, YAML config, LLM-first, zero dependencies.

<br>

## Quick Start

### Installation

```bash
# From source (recommended)
cargo install --git https://github.com/supernovae-st/nika.git

# Or clone and build
git clone https://github.com/supernovae-st/nika.git
cd nika && cargo install --path tools/nika
```

### Hello World

```yaml
# hello.nika.yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: greet
    infer: "Say hello in French, then in Japanese"
```

```bash
export ANTHROPIC_API_KEY=sk-ant-...
nika hello.nika.yaml
```

<details>
<summary><b>Output</b></summary>

```
Workflow completed in 1.4s

greet:
  Bonjour!

  konnichiha!
```

</details>

<br>

## Architecture

```mermaid
flowchart TB
    subgraph Input["Input Layer"]
        YAML[("workflow.nika.yaml")]
        Chat["Chat Messages"]
    end

    subgraph Core["Nika Engine"]
        direction TB
        Parser["AST Parser"]
        StableDag["StableGraph DAG"]
        Executor["Task Executor"]
        BuiltinTools["Builtin Tools"]

        Parser --> StableDag
        StableDag --> Executor
        Executor <--> BuiltinTools
    end

    subgraph Verbs["5 Semantic Verbs"]
        direction LR
        Infer["infer"]
        Exec["exec"]
        Fetch["fetch"]
        Invoke["invoke"]
        Agent["agent"]
    end

    subgraph Providers["6 LLM Providers"]
        Claude["Claude"]
        OpenAI["OpenAI"]
        Mistral["Mistral"]
        Groq["Groq"]
        DeepSeek["DeepSeek"]
        Ollama["Ollama"]
    end

    subgraph MCP["MCP Integration"]
        McpClient["MCP Client"]
        NovaNet["NovaNet"]
        Filesystem["Filesystem"]
        Custom["Custom Servers"]
    end

    subgraph Output["Output Layer"]
        Traces["NDJSON Traces"]
        Results["Task Results"]
        Events["22 Event Types"]
    end

    YAML --> Parser
    Chat --> StableDag
    Executor --> Verbs
    Infer --> Providers
    Agent --> Providers
    Invoke --> McpClient
    McpClient --> NovaNet
    McpClient --> Filesystem
    McpClient --> Custom
    Executor --> Output

    style Core fill:#7c3aed,color:#fff
    style YAML fill:#f97316,color:#fff
    style Chat fill:#ec4899,color:#fff
```

### Module Architecture

```mermaid
graph TD
    subgraph CLI["CLI Layer"]
        Main["main.rs"]
        Commands["commands/"]
    end

    subgraph AST["AST Layer"]
        Workflow["workflow.rs"]
        Action["action.rs<br/>5 Verbs"]
        Decompose["decompose.rs"]
    end

    subgraph Runtime["Runtime Layer"]
        Runner["runner.rs"]
        Executor["executor.rs"]
        RigAgent["rig_agent_loop.rs"]
        Spawn["spawn.rs"]
    end

    subgraph Data["Data Layer"]
        Binding["binding/"]
        Store["store/"]
        Event["event/"]
    end

    subgraph TUI["TUI Layer (73k LOC)"]
        Views["6 Views"]
        Widgets["39 Widgets"]
        Animation["Animation System"]
    end

    subgraph Integration["Integration Layer"]
        MCP["mcp/"]
        Provider["provider/"]
        Tools["tools/"]
    end

    Main --> Commands
    Commands --> Runner
    Runner --> AST
    Runner --> Executor
    Executor --> RigAgent
    Executor --> Spawn
    Executor --> Data
    RigAgent --> MCP
    RigAgent --> Provider
    Main --> TUI

    style TUI fill:#10b981,color:#fff
    style Runtime fill:#6366f1,color:#fff
```

<br>

## The 5 Verbs

```mermaid
mindmap
  root((5 Verbs))
    infer
      LLM Generation
      6 Providers
      Streaming
      Shorthand syntax
    exec
      Shell Commands
      Timeout control
      Template vars
    fetch
      HTTP Requests
      Retry logic
      JSON parsing
    invoke
      MCP Tool Calls
      Any MCP server
      30s timeout
    agent
      Multi-turn loops
      Tool calling
      spawn_agent
      Extended thinking
```

<details open>
<summary><b>infer</b> &mdash; LLM Generation</summary>

```yaml
# Shorthand (v0.5.1+)
- id: haiku
  infer: "Write a haiku about Rust"

# Full options
- id: analyze
  infer:
    prompt: "Analyze this code for bugs"
    provider: openai
    model: gpt-4o
    temperature: 0.7
    max_tokens: 2000
```

</details>

<details>
<summary><b>exec</b> &mdash; Shell Commands</summary>

```yaml
# Simple command
- id: build
  exec: "cargo build --release"

# With templating
- id: deploy
  use:
    env: staging
  exec:
    command: "kubectl apply -f {{use.env}}.yaml"
    timeout: 60
```

</details>

<details>
<summary><b>fetch</b> &mdash; HTTP Requests</summary>

```yaml
- id: get_users
  fetch:
    url: "https://api.example.com/users"
    method: GET
    headers:
      Authorization: "Bearer {{use.token}}"
  output:
    format: json
```

</details>

<details>
<summary><b>invoke</b> &mdash; MCP Tool Calls</summary>

```yaml
- id: generate_content
  invoke:
    mcp: novanet
    tool: novanet_generate
    params:
      focus_key: "entity:qr-code"
      locale: "fr-FR"
```

</details>

<details>
<summary><b>agent</b> &mdash; Agentic Loops</summary>

```yaml
- id: research
  agent:
    prompt: "Research recent AI papers and summarize findings"
    mcp: [filesystem, web_search]
    max_turns: 10
    thinking: true  # Extended thinking (Claude)
    depth_limit: 3  # Nested agent protection
```

</details>

<br>

## Chat DAG Widgets

<sup>New in v0.10+</sup>

Nika v0.10 introduces **Chat-as-DAG** architecture where every chat message is a DAG node with stable references.

```mermaid
flowchart LR
    subgraph ChatWorkflow["ChatWorkflow (StableDag)"]
        M1["@1 User: Research AI"]
        M2["@2 Assistant: Found papers..."]
        M3["@3 User: Summarize @1"]
        M4["@4 Assistant: Summary..."]
    end

    M1 -->|"auto-edge"| M2
    M2 -->|"auto-edge"| M3
    M3 -->|"@mention"| M1
    M3 -->|"auto-edge"| M4

    style M1 fill:#3b82f6,color:#fff
    style M2 fill:#10b981,color:#fff
    style M3 fill:#3b82f6,color:#fff
    style M4 fill:#10b981,color:#fff
```

### @mention Binding System

Reference previous messages in your chat:

| Syntax | Description | Example |
|:-------|:------------|:--------|
| `@N` | Reference message N | `Analyze @1` |
| `@last` | Previous message | `Continue @last` |
| `@all` | All messages | `Summarize @all` |
| `@N..M` | Range of messages | `Compare @1..3` |
| `//` | Parallel marker | `@1 // @2` (parallel edges) |

### Widget Components

| Widget | Purpose | Features |
|:-------|:--------|:---------|
| **ChatNodeBox** | Message as DAG node | 4 kinds, 4 states |
| **ChatEdgeLine** | @N reference edges | Bezier curves |
| **ChatTaskQueue** | Task execution queue | 5-verb icons |
| **ChatDagPanel** | Full DAG visualization | Nodes + edges combined |

<br>

## Builtin Tools

<sup>New in v0.9.3</sup>

6 `nika:*` prefixed tools for workflow control:

```mermaid
graph LR
    subgraph BuiltinTools["nika:* Builtin Tools"]
        Sleep["nika:sleep<br/>Delay execution"]
        Log["nika:log<br/>Emit log events"]
        Emit["nika:emit<br/>Custom events"]
        Assert["nika:assert<br/>Validate conditions"]
        Prompt["nika:prompt<br/>User interaction"]
        Run["nika:run<br/>Sub-workflows"]
    end

    Sleep --> |"ms precision"| Executor
    Log --> |"trace/debug/info/warn/error"| Tracing
    Emit --> |"payload"| EventSystem
    Assert --> |"custom message"| ErrorHandling
    Prompt --> |"HITL"| UserInput
    Run --> |"nested workflow"| Executor

    style BuiltinTools fill:#8b5cf6,color:#fff
```

```yaml
# Example: Using builtin tools in agent
- id: careful_agent
  agent:
    prompt: "Process data carefully"
    tools:
      - nika:log     # Emit debug logs
      - nika:assert  # Validate assumptions
      - nika:sleep   # Rate limiting
```

<br>

## Studio TUI

Launch the terminal UI with 6 views:

```bash
nika              # Home view (browse workflows)
nika chat         # Chat with AI
nika studio       # YAML editor
nika studio file.yaml  # Edit specific file
```

### 6-Views Architecture

```mermaid
stateDiagram-v2
    [*] --> Home: nika
    Home --> Chat: a
    Home --> Studio: s
    Home --> Monitor: m
    Chat --> Home: h
    Chat --> Studio: s
    Studio --> Home: h
    Studio --> Monitor: m
    Monitor --> Home: h

    Settings --> [*]: Esc
    Help --> [*]: Esc

    state "Views" as views {
        Home: Browse workflows
        Chat: Conversational AI
        Studio: YAML editor
        Monitor: Execution tracking
    }

    state "Overlays" as overlays {
        Settings: Ctrl+,
        Help: ?
    }
```

| Key | View | Description |
|:---:|:-----|:------------|
| `a` | **Chat** | Conversational AI agent with 5-verb commands |
| `h` | **Home** | Browse and launch `.nika.yaml` workflows |
| `s` | **Studio** | VS Code-like YAML editor with validation |
| `m` | **Monitor** | Real-time execution with 4-panel display |
| `,` | **Settings** | Provider config, themes, preferences |
| `?` | **Help** | Keyboard shortcuts, documentation |

### Keyboard Shortcuts

| Key | Action | | Key | Action |
|:---:|:-------|---|:---:|:-------|
| `Tab` | Switch views | | `Ctrl+Z` | Undo |
| `Ctrl+P` | Fuzzy search | | `Ctrl+Y` | Redo |
| `Ctrl+W` | Close tab | | `Ctrl+S` | Save |
| `Alt+`/`Alt+` | Navigate tabs | | `q` | Quit |

<br>

## Providers

<div align="center">

| Provider | Environment Variable | Default Model | Streaming | Thinking |
|:--------:|:---------------------|:--------------|:---------:|:--------:|
| **Claude** | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` | | |
| **OpenAI** | `OPENAI_API_KEY` | `gpt-4o` | | - |
| **Mistral** | `MISTRAL_API_KEY` | `mistral-large-latest` | | - |
| **Groq** | `GROQ_API_KEY` | `llama-3.3-70b-versatile` | | - |
| **DeepSeek** | `DEEPSEEK_API_KEY` | `deepseek-chat` | | - |
| **Ollama** | `OLLAMA_API_BASE_URL` | `llama3.2` | | - |

</div>

**Auto-detection priority:** Claude &rarr; OpenAI &rarr; Mistral &rarr; Groq &rarr; DeepSeek &rarr; Ollama

```mermaid
flowchart LR
    subgraph Detection["Provider Auto-Detection"]
        direction TB
        Check1{"ANTHROPIC_API_KEY?"}
        Check2{"OPENAI_API_KEY?"}
        Check3{"MISTRAL_API_KEY?"}
        Check4{"GROQ_API_KEY?"}
        Check5{"DEEPSEEK_API_KEY?"}
        Check6{"OLLAMA_API_BASE_URL?"}
        Error["Error: No provider"]

        Check1 -->|Yes| Claude
        Check1 -->|No| Check2
        Check2 -->|Yes| OpenAI
        Check2 -->|No| Check3
        Check3 -->|Yes| Mistral
        Check3 -->|No| Check4
        Check4 -->|Yes| Groq
        Check4 -->|No| Check5
        Check5 -->|Yes| DeepSeek
        Check5 -->|No| Check6
        Check6 -->|Yes| Ollama
        Check6 -->|No| Error
    end

    Claude["Claude"]
    OpenAI["OpenAI"]
    Mistral["Mistral"]
    Groq["Groq"]
    DeepSeek["DeepSeek"]
    Ollama["Ollama"]

    style Claude fill:#d97757
    style Error fill:#ef4444
```

<br>

## Examples

### Code Review Pipeline

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: get_diff
    exec: "git diff HEAD~1"

  - id: review
    use:
      diff: get_diff
    infer:
      prompt: |
        Review this code diff for:
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

### Multi-Locale Generation with for_each

```yaml
schema: "nika/workflow@0.12"

tasks:
  - id: translate
    for_each: ["en-US", "fr-FR", "de-DE", "ja-JP", "es-ES"]
    as: locale
    concurrency: 5
    infer:
      prompt: "Write a marketing tagline in {{use.locale}}"
```

### Diamond DAG Pattern

```mermaid
graph LR
    A["outline"] --> B["intro"]
    A --> C["conclusion"]
    B --> D["assemble"]
    C --> D

    style A fill:#7c3aed,color:#fff
    style B fill:#10b981,color:#fff
    style C fill:#10b981,color:#fff
    style D fill:#f97316,color:#fff
```

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: outline
    infer: "Create blog outline about AI agents"
    output:
      format: json

  - id: write_intro
    use: { title: outline.title }
    infer: "Write engaging intro for: {{use.title}}"

  - id: write_conclusion
    use: { title: outline.title }
    infer: "Write compelling conclusion for: {{use.title}}"

  - id: assemble
    use:
      intro: write_intro
      conclusion: write_conclusion
    exec: |
      echo "{{use.intro}}"
      echo -e "\n---\n"
      echo "{{use.conclusion}}"

flows:
  - source: outline
    target: [write_intro, write_conclusion]
  - source: [write_intro, write_conclusion]
    target: assemble
```

### Multi-Agent Research with spawn_agent

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: orchestrator
    agent:
      prompt: |
        Research AI safety papers from 2024.
        For each paper found, spawn a sub-agent to summarize it.
        Compile findings into a comprehensive report.
      mcp: [web_search, filesystem]
      max_turns: 15
      depth_limit: 3  # Allows nested agents up to depth 3
      thinking: true
```

<br>

## MCP Integration

Connect Nika to any [Model Context Protocol](https://modelcontextprotocol.io/) server:

```yaml
schema: "nika/workflow@0.12"

mcp:
  novanet:
    command: novanet-mcp
    env:
      NEO4J_URI: bolt://localhost:7687
  filesystem:
    command: npx
    args: ["-y", "@anthropic/mcp-filesystem"]
  web_search:
    command: npx
    args: ["-y", "@anthropic/mcp-web-search"]

tasks:
  - id: generate
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "qr-code"
        locale: "fr-FR"
```

```mermaid
flowchart LR
    Nika["Nika CLI"] -->|"invoke:"| McpClient["MCP Client"]

    subgraph Servers["MCP Servers"]
        NovaNet["NovaNet<br/>Knowledge Graph"]
        Filesystem["Filesystem<br/>File Operations"]
        WebSearch["Web Search<br/>Internet Queries"]
        Custom["Custom<br/>Your Tools"]
    end

    McpClient --> NovaNet
    McpClient --> Filesystem
    McpClient --> WebSearch
    McpClient --> Custom

    style Nika fill:#7c3aed,color:#fff
    style McpClient fill:#ec4899,color:#fff
```

<br>

## Project Stats

<div align="center">

```
+===================================================================+
|                       Nika v0.12.0                                |
+===================================================================+
|  Tests           |  2,720+ passing                                |
|  Lines of Code   |  106,000+ LOC                                  |
|  Clippy          |  0 warnings                                    |
|  Providers       |  6 (Claude, OpenAI, Mistral, Groq...)          |
|  Verbs           |  5 semantic actions                            |
|  Builtin Tools   |  6 nika:* tools                                |
|  TUI Views       |  6 (Chat, Home, Studio, Monitor...)            |
|  TUI Widgets     |  39 widgets                                    |
|  Event Types     |  22 event variants                             |
|  Rust Edition    |  2021                                          |
+===================================================================+
```

</div>

### Test Distribution

```mermaid
pie title Test Distribution by Module
    "TUI" : 1704
    "Binding" : 198
    "AST" : 171
    "Runtime" : 124
    "MCP" : 111
    "DAG" : 60
    "Event" : 46
    "Util" : 30
    "Provider" : 24
    "Store" : 19
    "Tools" : 13
```

<br>

## Powered By

<div align="center">

[![rig-core](https://img.shields.io/badge/rig--core-0.31-f97316?style=flat-square)](https://github.com/0xPlaygrounds/rig)
[![tokio](https://img.shields.io/badge/tokio-1.49-3b82f6?style=flat-square)](https://tokio.rs/)
[![ratatui](https://img.shields.io/badge/ratatui-0.30-10b981?style=flat-square)](https://ratatui.rs/)
[![rmcp](https://img.shields.io/badge/rmcp-0.16-8b5cf6?style=flat-square)](https://github.com/anthropics/anthropic-cookbook)
[![petgraph](https://img.shields.io/badge/petgraph-0.6-ec4899?style=flat-square)](https://docs.rs/petgraph)
[![serde](https://img.shields.io/badge/serde-1.0-0ea5e9?style=flat-square)](https://serde.rs/)

</div>

<br>

## IDE Setup

Get YAML autocompletion and validation in your editor:

<details>
<summary><b>VS Code</b></summary>

Install [YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml), then add to `.vscode/settings.json`:

```json
{
  "yaml.schemas": {
    "https://raw.githubusercontent.com/supernovae-st/nika/main/schemas/nika-workflow.schema.json": "*.nika.yaml"
  }
}
```

</details>

<details>
<summary><b>JetBrains (IntelliJ, WebStorm)</b></summary>

1. Go to **Settings &rarr; Languages &rarr; Schemas and DTDs &rarr; JSON Schema Mappings**
2. Add new mapping:
   - Schema URL: `https://raw.githubusercontent.com/supernovae-st/nika/main/schemas/nika-workflow.schema.json`
   - File pattern: `*.nika.yaml`

</details>

<details>
<summary><b>Neovim (with nvim-lspconfig)</b></summary>

```lua
require('lspconfig').yamlls.setup {
  settings = {
    yaml = {
      schemas = {
        ["https://raw.githubusercontent.com/supernovae-st/nika/main/schemas/nika-workflow.schema.json"] = "*.nika.yaml"
      }
    }
  }
}
```

</details>

<br>

## Documentation

| Resource | Description |
|:---------|:------------|
| [CHANGELOG.md](CHANGELOG.md) | Version history |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design |
| [tools/nika/CLAUDE.md](tools/nika/CLAUDE.md) | AI context |
| [examples/](tools/nika/examples/) | Sample workflows |
| [schemas/](schemas/) | JSON Schema for IDE |

<br>

## FAQ

<details>
<summary><b>How do I switch LLM providers?</b></summary>

Set the environment variable for your provider:

```bash
# Use Claude (default)
export ANTHROPIC_API_KEY=sk-ant-...

# Use OpenAI instead
export OPENAI_API_KEY=sk-...

# Or specify per-workflow
provider: openai  # in your YAML
```

</details>

<details>
<summary><b>How do I use @mentions in chat?</b></summary>

Reference previous messages by their number:

```
You: Research AI safety papers
Assistant: Found 5 papers...

You: Summarize @1 in bullet points
# ^^ References message 1 (your first message)

You: Compare @1 and @2
# ^^ References messages 1 and 2
```

</details>

<details>
<summary><b>What are builtin tools?</b></summary>

Nika provides 6 `nika:*` tools for workflow control:

- `nika:sleep` &mdash; Delay execution (ms precision)
- `nika:log` &mdash; Emit log events (trace/debug/info/warn/error)
- `nika:emit` &mdash; Custom event emission with payload
- `nika:assert` &mdash; Validate conditions with custom messages
- `nika:prompt` &mdash; Interactive user prompts (HITL)
- `nika:run` &mdash; Execute sub-workflows

</details>

<details>
<summary><b>What's the difference between infer and agent?</b></summary>

| | `infer` | `agent` |
|---|---|---|
| Turns | Single | Multi-turn loop |
| Tools | No | MCP tools |
| Nested agents | No | spawn_agent |
| Use for | Simple prompts | Complex reasoning |

</details>

<details>
<summary><b>How do I debug a failing workflow?</b></summary>

1. **Check traces:** `nika trace list` then `nika trace show <id>`
2. **Validate first:** `nika check workflow.yaml --strict`
3. **Use verbose mode:** `RUST_LOG=debug nika run workflow.yaml`

</details>

<br>

## Troubleshooting

<details>
<summary><b>NIKA-001: No API key found</b></summary>

```
Error: NIKA-001 - No LLM provider configured
```

**Fix:** Set at least one API key:
```bash
export ANTHROPIC_API_KEY=sk-ant-...
# or
export OPENAI_API_KEY=sk-...
```

</details>

<details>
<summary><b>NIKA-010: MCP server failed to start</b></summary>

```
Error: NIKA-010 - MCP server 'novanet' failed to connect
```

**Fix:**
1. Check the command path exists
2. Verify the server binary is executable
3. Check server logs: `nika trace show <id> | grep mcp`

</details>

<details>
<summary><b>NIKA-020: Cycle detected in DAG</b></summary>

```
Error: NIKA-020 - Cycle detected: task_a -> task_b -> task_a
```

**Fix:** Remove circular dependency. Use `flows:` to visualize.

</details>

<br>

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika

cargo build          # Build
cargo test           # Test (2,720+ tests)
cargo clippy         # Lint
cargo run -- --help  # Run
```

<br>

## License

**AGPL-3.0** &mdash; See [LICENSE](LICENSE) for details.

<br>

---

<div align="center">

## Part of the SuperNovae Ecosystem

<table>
<tr>
<td align="center">
<a href="https://github.com/supernovae-st/novanet">
<img src="https://img.shields.io/badge/NovaNet-Knowledge_Graph-0ea5e9?style=for-the-badge&logo=neo4j&logoColor=white" alt="NovaNet">
</a>
</td>
<td align="center">
<a href="https://github.com/supernovae-st/nika">
<img src="https://img.shields.io/badge/Nika-Workflow_Engine-7c3aed?style=for-the-badge&logo=yaml&logoColor=white" alt="Nika">
</a>
</td>
</tr>
</table>

<br>

<!-- SuperNovae Studio -->
<a href="https://supernovae.studio">
<img src="https://avatars.githubusercontent.com/u/33066282?s=200&v=4" width="80" height="80" alt="SuperNovae Studio">
</a>

**[SuperNovae Studio](https://supernovae.studio)**

*Building the future of AI workflows*

<br>

<!-- Team -->
<table>
<tr>
<td align="center">
<a href="https://github.com/ThibautMelen">
<img src="https://github.com/ThibautMelen.png" width="80" height="80" alt="Thibaut Melen"><br>
<sub><b>Thibaut Melen</b></sub>
</a>
</td>
<td align="center">
<a href="https://github.com/NicolasCELLA">
<img src="https://github.com/NicolasCELLA.png" width="80" height="80" alt="Nicolas Cella"><br>
<sub><b>Nicolas Cella</b></sub>
</a>
</td>
</tr>
</table>

<br>

<!-- Links -->
[![Website](https://img.shields.io/badge/nika.sh-7c3aed?style=for-the-badge&logo=safari&logoColor=white)](https://nika.sh)
[![SuperNovae](https://img.shields.io/badge/supernovae.studio-f97316?style=for-the-badge&logo=safari&logoColor=white)](https://supernovae.studio)
[![GitHub](https://img.shields.io/badge/supernovae--st-181717?style=for-the-badge&logo=github&logoColor=white)](https://github.com/supernovae-st)

<br>

[![Stars](https://img.shields.io/github/stars/supernovae-st/nika?style=social)](https://github.com/supernovae-st/nika)
&nbsp;&nbsp;
[![Forks](https://img.shields.io/github/forks/supernovae-st/nika?style=social)](https://github.com/supernovae-st/nika/fork)
&nbsp;&nbsp;
[![Watchers](https://img.shields.io/github/watchers/supernovae-st/nika?style=social)](https://github.com/supernovae-st/nika)

<br>

---

<sub>Made with love and Rust by SuperNovae Studio</sub>

</div>
