# One-Pager -- Nika

> Executive summary for investors, partners, conference organizers, and press.
> One page. Everything they need to understand what Nika is and why it matters.

---

<!-- BEGIN ONE-PAGER -->

# Nika -- Semantic YAML Workflow Engine for AI Tasks

Nika is an open source workflow engine that lets developers orchestrate AI tasks using 5 declarative YAML verbs, replacing hundreds of lines of SDK boilerplate with readable, version-controllable workflow definitions. Written in 451K lines of Rust, it compiles to a single binary with zero runtime dependencies.

---

## Key Metrics

| Metric | Value |
|--------|-------|
| **Codebase** | 451K lines of Rust, 10 workspace crates |
| **Tests** | 8,100+ passing, zero clippy warnings, zero unsafe |
| **LLM Providers** | 22 (Claude, GPT-4o, Gemini, Mistral, Groq, DeepSeek, xAI, Perplexity, local GGUF) |
| **Media Tools** | 24 built-in (thumbnail, chart, PDF, C2PA, QR validation) |
| **Showcase Workflows** | 200+ ready-to-use examples |
| **Course Exercises** | 44 across 12 levels (interactive learning) |
| **Schema Version** | nika/workflow@0.12 |
| **License** | AGPL-3.0-or-later |
| **Deployment** | Single binary via `cargo install nika` |
| **MCP Aliases** | 100+ pre-configured tool connections |

---

## Value Proposition

**For developers:** Replace fragile Python SDK pipelines with readable YAML. Switch LLM providers by changing one line. Get structured output validation, agent guardrails, and a media pipeline -- all in one binary.

**For teams:** Workflow files ARE documentation. Put them in PRs, diff them, review them. Non-engineers can read what the pipeline does without knowing Python.

**For the ecosystem:** AGPL-3.0 ensures Nika stays open. No cloud provider can fork it and close the door. Contributions benefit everyone.

---

## The 5 Verbs

```
infer:  --> Call any LLM (22 providers, structured output, vision)
exec:   --> Run shell commands (28-pattern security blocklist)
fetch:  --> HTTP requests (9 extract modes: markdown, article, RSS...)
invoke: --> MCP tool calls (24 built-in + any MCP server)
agent:  --> Multi-turn loops (guardrails, cost limits, tool calling)
```

---

## Architecture

```
                     YAML Workflow (.nika.yaml)
                              |
                    +--------------------+
                    | 2-Phase AST        |
                    | Raw -> Analyzed    |
                    | (source spans,     |
                    |  semantic validation)|
                    +--------------------+
                              |
                    +--------------------+
                    | DAG Scheduler      |
                    | (parallel exec,    |
                    |  cycle detection)  |
                    +--------------------+
                              |
              +-------+-------+-------+-------+
              |       |       |       |       |
           infer:  exec:  fetch:  invoke:  agent:
              |       |       |       |       |
              +-------+-------+-------+-------+
                              |
                    +--------------------+
                    | Event Sourcing     |
                    | (39 event types,   |
                    |  NDJSON traces)    |
                    +--------------------+
```

**Brain + Body:** Nika (workflow engine) connects to NovaNet (knowledge graph) via MCP Protocol. NovaNet provides entity context, semantic relationships, and cross-session memory. Clean separation: NovaNet knows, Nika does.

---

## Use Cases

### 1. Multi-Model Content Pipeline
Fetch data with Groq (fast, cheap), analyze with Claude (quality), format with DeepSeek ($0.14/1M tokens). One workflow, three providers, 60% cost savings.

### 2. Image Processing Automation
Import -> thumbnail -> optimize -> thumbhash -> metadata extraction. All via built-in tools. Zero external services, zero API keys.

### 3. Intelligent Code Agent
Multi-turn agent with file system access (read, write, edit, grep). Guardrails prevent destructive actions. Cost limits cap spending at $1/run.

### 4. Web Scraping + AI Analysis
Fetch with article extraction -> structured entity extraction -> knowledge graph storage via MCP. Nine extract modes replace headless browsers.

### 5. Content Authenticity
C2PA credential signing for AI-generated content. EU AI Act compliance verification. Provenance chain from creation to distribution.

---

## Getting Started (3 Steps)

```bash
# Step 1: Install
cargo install nika

# Step 2: Create a workflow
cat > hello.nika.yaml << 'EOF'
schema: nika/workflow@0.12
tasks:
  - id: greet
    infer: "Write a haiku about open source"
EOF

# Step 3: Run it
nika run hello.nika.yaml
```

**Learn more:** `nika init --course` (44 interactive exercises)
**Browse examples:** `nika showcase list` (200+ workflows)
**Full TUI:** `nika ui` (live DAG, streaming, cost tracking)

---

## Technical Highlights

### 2-Phase AST with Source Spans

Nika doesn't just parse YAML and execute it. The engine builds a proper Abstract Syntax Tree in two phases. Phase 1 (Raw) uses `marked_yaml` to preserve source spans -- every element knows its file, line, and column. Phase 2 (Analyzed) performs semantic validation: TaskId interning, dependency resolution, cycle detection, provider validation, and template variable checking. Errors include exact source locations, not generic stack traces.

### DAG Scheduling with Automatic Parallelism

Tasks declare dependencies through `with:` data bindings. Nika constructs a directed acyclic graph, detects cycles at compile time, and runs independent tasks in parallel via tokio JoinSet with CancellationToken for fail-fast semantics. No explicit `parallel:` blocks or ordering directives.

### Content-Addressable Storage

All media operations go through a CAS layer where files are addressed by content hash, not file path. This prevents path traversal attacks, enables deduplication, and makes pipelines reproducible -- same input hash always produces the same output.

### Event Sourcing

Every workflow execution emits events (39 distinct types) in NDJSON format. Events cover task lifecycle, provider selection, token usage, cost tracking, error details, and media operations. This provides full observability without external monitoring tools.

### Security Enforcement

The PolicyEnforcer validates workflows against configurable security policies. A 28-pattern command blocklist prevents dangerous shell operations. Environment variables are validated but never logged. SVG inputs are sanitized before parsing. File imports are size-limited (50MB) and path-validated against traversal attacks.

### Embeddable Engine

The `nika-engine` crate (134K lines) is a standalone library. It can be integrated into any Rust application without the CLI, TUI, or LSP. This enables building custom tools, services, or platforms on top of Nika's execution engine.

---

## Competitive Position

```
                    Declarative
                        |
              Dify      |     * NIKA
                        |
    Simple -------------|-------------- Complex
                        |
              CrewAI    |     LangGraph
                        |
                    Imperative
```

**Unique combination:** YAML-first + knowledge graph integration + 5 semantic verbs + MCP-native + built-in media pipeline + interactive course. No competitor has all six.

---

## Team and Contact

**Creator:** Thibaut Melen (@ThibautMelen)
**Organization:** SuperNovae Studio (@SuperNovae-studio)
**Product:** QR Code AI (https://qrcode-ai.com)
**Website:** https://supernovae.studio
**GitHub:** https://github.com/supernovae-st/nika
**Email:** thibaut@supernovae.studio

---

*Nika: the Greek goddess of victory. Open source is our victory. AGPL-3.0.*

<!-- END ONE-PAGER -->
