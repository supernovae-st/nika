# 01 -- What Is Nika

## Executive Summary

Nika is a semantic YAML workflow engine purpose-built for AI tasks. Written in Rust (~451K lines across 12 crates), it orchestrates LLM inference, shell commands, HTTP requests, MCP tool calls, and autonomous agent loops through declarative YAML files. Nika follows the schema `nika/workflow@0.12` and enforces a strict five-verb paradigm: `infer:`, `exec:`, `fetch:`, `invoke:`, and `agent:`.

At its core, Nika is the **body** in a brain/body architecture. Its counterpart, **NovaNet**, is the brain -- a knowledge graph with NodeClasses and ArcClasses. The two communicate exclusively via the Model Context Protocol (MCP). Nika never touches Neo4j or Cypher directly; all knowledge graph operations go through `invoke:` verbs that call MCP tools exposed by NovaNet.

Nika is licensed under AGPL-3.0-or-later and maintained by SuperNovae Studio. The current version is **v0.49.0**.

---

## Philosophy

### Terminal-First, Human-Optional

Nika is designed as a terminal-first tool. Simple workflows run headlessly via `nika run workflow.nika.yaml`. Complex interactions happen in a ratatui-based TUI with three views: Studio (file browser + YAML editor + DAG preview), Command (execution monitoring + chat), and Control (provider config, theme, preferences). There is no web UI, no Electron wrapper. The terminal is the interface.

### Declarative Over Imperative

Workflows are YAML files, not code. A developer declares **what** should happen -- "infer a summary", "fetch this URL", "invoke this MCP tool" -- and Nika handles **how**: provider resolution, connection pooling, retry logic, DAG ordering, binding resolution, and structured output validation. The YAML schema is the contract.

### Five Verbs, Nothing More

Every task in a Nika workflow performs exactly one of five verbs:

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM text generation | Send a prompt to Claude, GPT-4, Gemini |
| `exec:` | Shell command execution | Run `curl`, `jq`, `python` scripts |
| `fetch:` | HTTP requests | GET/POST with extraction (markdown, article, metadata) |
| `invoke:` | MCP tool calls | Call tools on MCP servers or 24 builtin `nika:*` tools |
| `agent:` | Multi-turn autonomous loops | Give an LLM agent tools and let it work |

This constraint is intentional. Any AI task can be decomposed into these five primitives. The simplicity makes workflows readable, portable, and auditable.

### DAG-First Execution

Tasks form a Directed Acyclic Graph. Dependencies are declared via `depends_on:` (explicit ordering edges) and `with:` blocks (data flow edges). Nika's runtime resolves the DAG topologically and executes independent tasks concurrently using tokio's JoinSet. There are no sequential pipelines unless the data flow requires it.

### Zero Cypher Rule

Nika workflows **never** contain raw Cypher queries. All interactions with NovaNet's knowledge graph go through MCP `invoke:` calls. This rule is enforced architecturally: Nika has no Neo4j driver dependency. The separation ensures that Nika and NovaNet can evolve independently.

---

## Core Concepts

### Workflows

A workflow is a `.nika.yaml` file containing a schema declaration, optional metadata, and a list of tasks. The minimal valid workflow:

```yaml
schema: "nika/workflow@0.12"
workflow: hello
tasks:
  - id: greet
    exec:
      command: echo "Hello from Nika"
```

### Tasks

Each task has a unique `id`, exactly one verb (action), and optional metadata: `description`, `with:` bindings, `depends_on:` ordering, `output:` policy, `retry:` config, `for_each:` iteration, `artifact:` persistence, and `structured:` output enforcement.

### Bindings

The `with:` block creates named data references between tasks. When task B declares `with: { summary: $task_a }`, it gets access to task A's output via `{{with.summary}}` templates. Bindings support JSONPath traversal, default values with `??`, and 27 pipe transforms like `upper`, `trim`, `sort`, `first(N)`, and `join(",")`.

### Providers

Nika supports 7 LLM providers (Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI) plus native local inference via mistral.rs. Providers are selected per-workflow or per-task. API keys come from environment variables, config files, or the system keychain.

### MCP Integration

The Model Context Protocol is Nika's extensibility mechanism. Any MCP server can be declared in the workflow's `mcp:` block and its tools become available to `invoke:` and `agent:` verbs. Nika also ships 12 core builtin tools and 24 media tools under the `nika:*` namespace.

---

## How Nika Fits with NovaNet

```
NovaNet (Brain)            MCP Protocol           Nika (Body)
+-----------------+    <================>    +------------------+
| Knowledge Graph |                          | YAML Workflows   |
| NodeClasses     |                          | 5 Verbs          |
| ArcClasses      |                          | DAG Execution    |
| MCP Tools       |                          | Inference Engine  |
+-----------------+                          +------------------+
```

NovaNet exposes MCP tools like `query_nodes`, `create_arc`, `search_knowledge`. Nika workflows call these via `invoke:` to read from and write to the knowledge graph. The two projects share folder structure conventions (`tools/<name>/src/{core,tui,commands}/`) but are separate git repositories with independent versioning. Nika stays at 0.x.x intentionally -- it is designed to evolve rapidly without backward compatibility constraints.

---

## The Nika Name

The name "Nika" comes from the Japanese pronunciation of the Greek goddess Nike (victory). In the SuperNovae universe, Nika is the butterfly on the crew's flag -- a symbol of transformation and freedom. The butterfly motif appears throughout the project: the TUI uses butterfly-themed icons, the course uses "Liberation" as its narrative theme, and the community rallying cry is about breaking free from closed AI systems.

---

## Target Users

Nika targets three audiences:

1. **AI Engineers** who need to orchestrate multi-step LLM workflows with structured output, tool use, and DAG-based parallelism.
2. **Developers** who want to automate tasks involving shell commands, HTTP APIs, and AI inference without writing Python scripts.
3. **AI Agents** (Claude Code, Cursor, Windsurf, etc.) that need a structured way to execute complex AI pipelines via YAML declarations.

The interactive course (12 levels, 44 exercises) onboards users from basic `exec:` commands to full MCP orchestration in a progressive "Liberation" narrative.

---

## What Nika Is NOT

- **Not a chatbot framework.** Nika's `agent:` verb enables agent loops, but Nika itself is not a chat application. The TUI's Command view includes a chat mode but Nika itself is not a chat application.
- **Not a LangChain alternative.** Nika is declarative YAML, not imperative Python. There are no chain abstractions, memory modules, or retrieval pipelines built in.
- **Not a CI/CD tool.** While `exec:` can run shell commands, Nika is not designed for build pipelines. It is designed for AI task orchestration.
- **Not a cloud service.** Nika runs locally as a CLI binary. There is no SaaS offering, no cloud dashboard, no managed service.

---

## Key Numbers (v0.49.0)

| Metric | Value |
|--------|-------|
| Total Rust code | ~451K lines |
| Workspace crates | 12 |
| Tests | 8,100+ (lib only, safe) |
| LLM providers | 7 cloud + 1 native |
| MCP aliases | 100 |
| Builtin tools | 12 core + 24 media |
| Showcase workflows | 115 |
| Course exercises | 44 across 12 levels |
| Error codes | NIKA-000 through NIKA-314 |
| Features (Cargo) | 30+ (most default) |
| Minimum Rust | 1.86 |

---

## Getting Started

```bash
# Install (when published to crates.io)
cargo install nika

# Initialize a project
nika init

# Run a workflow
nika run workflows/minimal/01-exec.nika.yaml

# Open the TUI
nika ui

# Start the interactive course
nika course next
```

See [02-architecture-overview.md](./02-architecture-overview.md) for the full crate architecture and [03-yaml-schema-reference.md](./03-yaml-schema-reference.md) for the complete YAML schema.
