<div align="center">

<!-- Animated Header -->
<img src="https://raw.githubusercontent.com/supernovae-st/nika/main/assets/nika-logo.svg" alt="Nika Logo" width="140" height="140">

# 🦋 Nika

### **Native Intelligence Kernel Agent**

<sup>Transform YAML into intelligent workflows</sup>

<!-- Primary Badges -->
[![Version](https://img.shields.io/badge/v0.8.0-7c3aed?style=for-the-badge&logo=semver&logoColor=white)](CHANGELOG.md)
[![Rust](https://img.shields.io/badge/rust_1.86+-f97316?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/AGPL--3.0-22c55e?style=for-the-badge&logo=gnu&logoColor=white)](LICENSE)

<!-- GitHub Badges -->
[![CI](https://img.shields.io/github/actions/workflow/status/supernovae-st/nika/ci.yml?style=flat-square&logo=github&label=CI)](https://github.com/supernovae-st/nika/actions)
[![Stars](https://img.shields.io/github/stars/supernovae-st/nika?style=flat-square&logo=github&label=Stars)](https://github.com/supernovae-st/nika/stargazers)
[![Tests](https://img.shields.io/badge/tests-1,902_passing-10b981?style=flat-square&logo=checkmarx)](https://github.com/supernovae-st/nika/actions)
[![Providers](https://img.shields.io/badge/LLM_providers-6-8b5cf6?style=flat-square&logo=openai)](#-providers)

<!-- Navigation -->
<p>
<a href="#-quick-start">Quick Start</a> •
<a href="#-how-nika-compares">Compare</a> •
<a href="#-features">Features</a> •
<a href="#-the-5-verbs">Verbs</a> •
<a href="#-examples">Examples</a> •
<a href="#-faq">FAQ</a> •
<a href="#-documentation">Docs</a>
</p>

---

**Nika** executes YAML-defined workflows as **directed acyclic graphs (DAGs)**.<br>
Connect LLMs, shell commands, HTTP APIs, and MCP tools in a single declarative file.

</div>

<br>

<!-- TUI Screenshot as ASCII Art -->
```
┌──────────────────────────────────────────────────────────────────────────────────┐
│  🦋 Nika Studio                                              v0.8.0  ⌘K ?  │
├──────────────────────────────────────────────────────────────────────────────────┤
│ ┌─ Files ────────────┐ ┌─ Editor ─────────────────────────────────────────────┐ │
│ │ 📁 workflows/      │ │  1 │ schema: "nika/workflow@0.8"                     │ │
│ │   📄 deploy.yaml   │ │  2 │ provider: claude                                │ │
│ │   📄 review.yaml   │ │  3 │                                                 │ │
│ │ ▸ 📄 hello.yaml ◀  │ │  4 │ tasks:                                          │ │
│ │   📄 test.yaml     │ │  5 │   - id: greet                                   │ │
│ │                    │ │  6 │     infer: "Say hello in French"                │ │
│ └────────────────────┘ │  7 │                                                 │ │
│ ┌─ DAG Preview ──────┐ │  8 │   - id: format                                  │ │
│ │                    │ │  9 │     use: { msg: greet }                         │ │
│ │   ┌───────┐        │ │ 10 │     exec: "echo '{{use.msg}}' | cowsay"         │ │
│ │   │ greet │        │ └───────────────────────────────────────────────────────┘ │
│ │   └───┬───┘        │ ┌─ Output ─────────────────────────────────────────────┐ │
│ │       │            │ │ ✅ greet completed (1.2s)                             │ │
│ │   ┌───▼───┐        │ │    Bonjour! Comment allez-vous?                      │ │
│ │   │format │        │ │ ⏳ format running...                                  │ │
│ │   └───────┘        │ │                                                       │ │
│ └────────────────────┘ └───────────────────────────────────────────────────────┘ │
├──────────────────────────────────────────────────────────────────────────────────┤
│  Chat[c] Home[h] Studio[s] Monitor[m]  │  claude-sonnet-4  │  2 tasks  │  1.2s  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

<br>

## ✨ What's New in v0.8.0

<table>
<tr>
<td>

**🎨 Studio DX Enhancement**

</td>
<td>

- **Edit History** — Ctrl+Z/Y with intelligent 500ms coalescing
- **Session Persistence** — Auto-save to `.nika/sessions/`
- **Solarized Theme** — Beautiful dark/light modes
- **Config System** — `.nika/config.toml` for preferences

</td>
</tr>
</table>

> 📦 **Upgrade:** `cargo install --git https://github.com/supernovae-st/nika.git --force`

<br>

## 🎯 Why Nika?

<table>
<tr>
<td width="50%">

### ❌ The Problem

```
LLM calls buried in code = untraceable
Custom glue code for each integration
No standard format for AI workflows
Hard to debug multi-step pipelines
```

</td>
<td width="50%">

### ✅ The Solution

```
YAML workflows = version-controlled
5 semantic verbs cover everything
Full observability via NDJSON traces
Native MCP client for tool calling
```

</td>
</tr>
</table>

<br>

### 🆚 How Nika Compares

<div align="center">

| | Nika | LangChain | Prefect | Temporal |
|:---|:---:|:---:|:---:|:---:|
| **Config Format** | YAML | Python | Python | Code |
| **Learning Curve** | 🟢 5 min | 🟡 Hours | 🟡 Hours | 🔴 Days |
| **LLM Native** | ✅ Built-in | ✅ Core | ❌ Add-on | ❌ Add-on |
| **MCP Support** | ✅ Native | ❌ No | ❌ No | ❌ No |
| **Observability** | ✅ NDJSON | 🟡 LangSmith | ✅ UI | ✅ UI |
| **Self-hosted** | ✅ Binary | ✅ Yes | 🟡 Cloud | 🟡 Cloud |
| **Dependencies** | 0 | Many | Many | Many |

</div>

> **TL;DR:** Nika = single binary, YAML config, LLM-first, zero dependencies.

<br>

## 🚀 Quick Start

### Installation

```bash
# From source (recommended)
cargo install --git https://github.com/supernovae-st/nika.git

# Or clone and build
git clone https://github.com/supernovae-st/nika.git
cd nika && cargo install --path .
```

### Hello World

```yaml
# hello.nika.yaml
schema: "nika/workflow@0.8"
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
<summary>📺 <b>Output</b></summary>

```
✅ Workflow completed in 1.4s

greet:
  Bonjour! 👋

  こんにちは! 🇯🇵
```

</details>

<br>

## 🎨 Features

<div align="center">

| | Feature | Description |
|:---:|:---|:---|
| 🧠 | **5 Semantic Verbs** | `infer` `exec` `fetch` `invoke` `agent` |
| ⚡ | **Parallel DAG** | Automatic dependency resolution & parallel execution |
| 🔄 | **for_each Loops** | Process arrays with configurable concurrency |
| 🔌 | **MCP Native** | Connect to any MCP server (NovaNet, filesystem, etc.) |
| 🤖 | **6 LLM Providers** | Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama |
| 🖥️ | **Studio TUI** | VS Code-like terminal interface |
| ↩️ | **Undo/Redo** | Ctrl+Z/Y with intelligent coalescing |
| 💾 | **Sessions** | Auto-save & restore your work |
| 🎨 | **Themes** | Default, Solarized, Custom |
| 📊 | **Traces** | NDJSON traces for full observability |

</div>

<br>

## 🏗️ Architecture

```mermaid
flowchart TB
    subgraph Input["📄 Input"]
        YAML[("workflow.nika.yaml")]
    end

    subgraph Engine["🦋 Nika Engine"]
        Parser["AST Parser"]
        DAG["DAG Builder"]
        Executor["Task Executor"]

        Parser --> DAG --> Executor
    end

    subgraph Verbs["⚡ Semantic Verbs"]
        direction LR
        Infer["🧠 infer"]
        Exec["⚙️ exec"]
        Fetch["🌐 fetch"]
        Invoke["🔌 invoke"]
        Agent["🤖 agent"]
    end

    subgraph Providers["🔮 LLM Providers"]
        direction LR
        Claude["Claude"]
        OpenAI["OpenAI"]
        Mistral["Mistral"]
        More["+ 3 more"]
    end

    subgraph Output["📊 Output"]
        Traces["NDJSON Traces"]
        Results["Task Results"]
    end

    YAML --> Parser
    Executor --> Verbs
    Infer --> Providers
    Agent --> Providers
    Executor --> Output

    style Engine fill:#7c3aed,color:#fff
    style YAML fill:#f97316,color:#fff
    style Infer fill:#10b981,color:#fff
    style Exec fill:#6366f1,color:#fff
    style Fetch fill:#0ea5e9,color:#fff
    style Invoke fill:#ec4899,color:#fff
    style Agent fill:#f59e0b,color:#fff
```

### Execution Flow

```mermaid
sequenceDiagram
    participant U as User
    participant N as Nika CLI
    participant P as Parser
    participant D as DAG
    participant E as Executor
    participant L as LLM Provider

    U->>N: nika workflow.yaml
    N->>P: Parse YAML
    P->>D: Build DAG
    D->>E: Execute tasks

    loop For each task
        E->>L: Call provider
        L-->>E: Response + tokens
        E->>E: Store result
    end

    E-->>N: Final results
    N-->>U: Output + traces
```

<br>

## 📖 The 5 Verbs

<details open>
<summary><b>🧠 infer</b> — LLM Generation</summary>

```yaml
# Shorthand
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
<summary><b>⚙️ exec</b> — Shell Commands</summary>

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
<summary><b>🌐 fetch</b> — HTTP Requests</summary>

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
<summary><b>🔌 invoke</b> — MCP Tool Calls</summary>

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
<summary><b>🤖 agent</b> — Agentic Loops</summary>

```yaml
- id: research
  agent:
    prompt: "Research recent AI papers and summarize findings"
    mcp: [filesystem, web_search]
    max_turns: 10
    thinking: true  # Enable extended thinking
```

</details>

<br>

## 🔮 Providers

<div align="center">

| Provider | Environment Variable | Default Model | Streaming |
|:--------:|:---------------------|:--------------|:---------:|
| <img src="https://www.anthropic.com/favicon.ico" width="16"> **Claude** | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` | ✅ |
| <img src="https://openai.com/favicon.ico" width="16"> **OpenAI** | `OPENAI_API_KEY` | `gpt-4o` | ✅ |
| <img src="https://mistral.ai/favicon.ico" width="16"> **Mistral** | `MISTRAL_API_KEY` | `mistral-large-latest` | ✅ |
| ⚡ **Groq** | `GROQ_API_KEY` | `llama-3.1-70b-versatile` | ✅ |
| 🌊 **DeepSeek** | `DEEPSEEK_API_KEY` | `deepseek-chat` | ✅ |
| 🦙 **Ollama** | `OLLAMA_API_BASE_URL` | `llama3.2` | ✅ |

</div>

**Auto-detection priority:** Claude → OpenAI → Mistral → Groq → DeepSeek → Ollama

<br>

## 💻 Studio TUI

Launch the terminal UI:

```bash
nika              # Home view (browse workflows)
nika chat         # Chat with AI
nika studio       # YAML editor
nika studio file.yaml  # Edit specific file
```

### Keyboard Shortcuts

| Key | Action | | Key | Action |
|:---:|:-------|---|:---:|:-------|
| `Tab` | Switch views | | `Ctrl+Z` | Undo |
| `Ctrl+P` | Fuzzy search | | `Ctrl+Y` | Redo |
| `Ctrl+W` | Close tab | | `Ctrl+S` | Save |
| `?` | Help overlay | | `q` | Quit |

<br>

## 📚 Examples

### 🔍 Code Review Pipeline

```yaml
schema: "nika/workflow@0.8"
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
        1. 🐛 Potential bugs
        2. 🔒 Security issues
        3. ✨ Improvements

        ```diff
        {{use.diff}}
        ```

flows:
  - source: get_diff
    target: review
```

### 🌍 Multi-Locale Generation

```yaml
schema: "nika/workflow@0.8"

tasks:
  - id: translate
    for_each: ["en-US", "fr-FR", "de-DE", "ja-JP", "es-ES"]
    as: locale
    concurrency: 5
    infer:
      prompt: "Write a marketing tagline in {{use.locale}}"
```

### 💎 Diamond DAG Pattern

```mermaid
graph LR
    A[📝 outline] --> B[✍️ intro]
    A --> C[✍️ conclusion]
    B --> D[📄 assemble]
    C --> D

    style A fill:#7c3aed,color:#fff
    style B fill:#10b981,color:#fff
    style C fill:#10b981,color:#fff
    style D fill:#f97316,color:#fff
```

```yaml
schema: "nika/workflow@0.8"
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

<br>

## 🎯 Use Cases

<table>
<tr>
<td width="33%" valign="top">

### 🔄 CI/CD Integration

```yaml
# Automated PR review
- exec: "git diff main"
- infer: "Review for bugs"
- exec: "gh pr comment"
```

**Teams using:** DevOps, Platform

</td>
<td width="33%" valign="top">

### 🌍 Content Generation

```yaml
# Multi-locale marketing
for_each: [en, fr, de, ja]
- invoke: novanet_generate
- infer: "Localize content"
```

**Teams using:** Marketing, SEO

</td>
<td width="33%" valign="top">

### 🤖 AI Agents

```yaml
# Research assistant
- agent:
    mcp: [web, files]
    prompt: "Research topic"
```

**Teams using:** Research, Analytics

</td>
</tr>
</table>

<details>
<summary><b>📋 More Use Cases</b></summary>

| Use Case | Verbs Used | Complexity |
|:---------|:-----------|:-----------|
| **Code Review Bot** | `exec` → `infer` | ⭐ Simple |
| **Doc Generator** | `infer` → `exec` | ⭐ Simple |
| **API Testing** | `fetch` → `infer` | ⭐⭐ Medium |
| **Data Pipeline** | `fetch` → `infer` → `exec` | ⭐⭐ Medium |
| **Multi-Agent Research** | `agent` + `invoke` | ⭐⭐⭐ Advanced |
| **Knowledge Graph Sync** | `invoke` + `for_each` | ⭐⭐⭐ Advanced |

</details>

<br>

## 🔌 MCP Integration

Connect Nika to any [Model Context Protocol](https://modelcontextprotocol.io/) server:

```yaml
schema: "nika/workflow@0.8"

mcp:
  novanet:
    command: novanet-mcp
    env:
      NEO4J_URI: bolt://localhost:7687
  filesystem:
    command: npx
    args: ["-y", "@anthropic/mcp-filesystem"]

tasks:
  - id: generate
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "qr-code"
        locale: "fr-FR"
```

<br>

## 📊 Project Stats

<div align="center">

```
╔═══════════════════════════════════════════════════════════════╗
║                      🦋 Nika v0.8.0                           ║
╠═══════════════════════════════════════════════════════════════╣
║  Tests           │  1,902 passing                             ║
║  Clippy          │  0 warnings                                ║
║  Providers       │  6 (Claude, OpenAI, Mistral, Groq...)      ║
║  Verbs           │  5 semantic actions                        ║
║  Schema          │  nika/workflow@0.8                         ║
║  TUI Views       │  4 (Chat, Home, Studio, Monitor)           ║
║  Rust Edition    │  2024                                      ║
╚═══════════════════════════════════════════════════════════════╝
```

</div>

<br>

## ⚡ Powered By

<div align="center">

[![rig-core](https://img.shields.io/badge/rig--core-0.31-f97316?style=flat-square)](https://github.com/0xPlaygrounds/rig)
[![tokio](https://img.shields.io/badge/tokio-1.49-3b82f6?style=flat-square)](https://tokio.rs/)
[![ratatui](https://img.shields.io/badge/ratatui-0.30-10b981?style=flat-square)](https://ratatui.rs/)
[![rmcp](https://img.shields.io/badge/rmcp-0.16-8b5cf6?style=flat-square)](https://github.com/anthropics/anthropic-cookbook)
[![serde](https://img.shields.io/badge/serde-1.0-ec4899?style=flat-square)](https://serde.rs/)

</div>

<br>

## 💡 IDE Setup

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

1. Go to **Settings → Languages → Schemas and DTDs → JSON Schema Mappings**
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

## 📖 Documentation

| Resource | Description |
|:---------|:------------|
| 📋 [CHANGELOG.md](CHANGELOG.md) | Version history |
| 🏗️ [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design |
| 🔄 [docs/MIGRATION-v0.8.0.md](docs/MIGRATION-v0.8.0.md) | Upgrade guide |
| 🤖 [tools/nika/CLAUDE.md](tools/nika/CLAUDE.md) | AI context |
| 📁 [examples/](tools/nika/examples/) | Sample workflows |
| 📐 [schemas/](schemas/) | JSON Schema for IDE |

<br>

## ❓ FAQ

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

Nika auto-detects in order: Claude → OpenAI → Mistral → Groq → DeepSeek → Ollama

</details>

<details>
<summary><b>Can I use local models with Ollama?</b></summary>

Yes! Start Ollama and set the base URL:

```bash
export OLLAMA_API_BASE_URL=http://localhost:11434
nika workflow.yaml  # Will use Ollama
```

</details>

<details>
<summary><b>How do I debug a failing workflow?</b></summary>

1. **Check traces:** `nika trace list` then `nika trace show <id>`
2. **Validate first:** `nika check workflow.yaml --strict`
3. **Use verbose mode:** `RUST_LOG=debug nika run workflow.yaml`

</details>

<details>
<summary><b>What's the difference between `infer` and `agent`?</b></summary>

| | `infer` | `agent` |
|---|---|---|
| Turns | Single | Multi-turn loop |
| Tools | ❌ No | ✅ MCP tools |
| Use for | Simple prompts | Complex reasoning |

</details>

<details>
<summary><b>How do I pass data between tasks?</b></summary>

Use the `use:` block with bindings:

```yaml
tasks:
  - id: step1
    infer: "Generate a title"

  - id: step2
    use:
      title: step1  # Reference previous task
    infer: "Write article about: {{use.title}}"
```

</details>

<br>

## 🔧 Troubleshooting

<details>
<summary><b>🔴 NIKA-001: No API key found</b></summary>

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
<summary><b>🔴 NIKA-010: MCP server failed to start</b></summary>

```
Error: NIKA-010 - MCP server 'novanet' failed to connect
```

**Fix:**
1. Check the command path exists
2. Verify the server binary is executable
3. Check server logs: `nika trace show <id> | grep mcp`

</details>

<details>
<summary><b>🔴 NIKA-020: Cycle detected in DAG</b></summary>

```
Error: NIKA-020 - Cycle detected: task_a → task_b → task_a
```

**Fix:** Remove circular dependency. Use `flows:` to visualize:
```yaml
flows:
  - source: task_a
    target: task_b  # task_b cannot reference task_a
```

</details>

<details>
<summary><b>🟡 Workflow runs slowly</b></summary>

**Tips:**
1. Use `for_each` with `concurrency: 5` for parallel tasks
2. Use `lazy: true` bindings to defer expensive lookups
3. Check if MCP servers are reconnecting (add `timeout: 30`)

</details>

<br>

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika

cargo build          # Build
cargo test           # Test (1,902 tests)
cargo clippy         # Lint
cargo run -- --help  # Run
```

<br>

## 📜 License

**AGPL-3.0** — See [LICENSE](LICENSE) for details.

<br>

---

<div align="center">

## 🌟 Part of the SuperNovae Ecosystem

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
[![Website](https://img.shields.io/badge/supernovae.studio-f97316?style=for-the-badge&logo=safari&logoColor=white)](https://supernovae.studio)
[![GitHub](https://img.shields.io/badge/supernovae--st-181717?style=for-the-badge&logo=github&logoColor=white)](https://github.com/supernovae-st)
[![Twitter](https://img.shields.io/badge/@supernovaestudio-1DA1F2?style=for-the-badge&logo=twitter&logoColor=white)](https://twitter.com/supernovaestudio)

<br>

[![Stars](https://img.shields.io/github/stars/supernovae-st/nika?style=social)](https://github.com/supernovae-st/nika)
&nbsp;&nbsp;
[![Forks](https://img.shields.io/github/forks/supernovae-st/nika?style=social)](https://github.com/supernovae-st/nika/fork)
&nbsp;&nbsp;
[![Watchers](https://img.shields.io/github/watchers/supernovae-st/nika?style=social)](https://github.com/supernovae-st/nika)

<br>

---

<sub>Made with 🦋 and 🦀 by SuperNovae Studio</sub>

</div>
