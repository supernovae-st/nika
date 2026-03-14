# 01 — Current Features Inventory

> Exhaustive map of Nika v0.27.0 + NovaNet v0.20.0 capabilities.
> Date: 2026-03-14

---

## Nika v0.27.0 — The Body

**Stats:** 371 Rust source files | ~219K lines | 6,157+ tests | Zero clippy warnings

### Core Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  NIKA ARCHITECTURE (v0.27.0)                                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  YAML Source                                                        │
│      │                                                              │
│      ▼                                                              │
│  ┌─────────────────────────────────┐                                │
│  │  TWO-PHASE AST                  │                                │
│  │  Phase 1: Raw (marked_yaml)     │  ← spans, all Optional        │
│  │  Phase 2: Analyzed (validated)  │  ← TaskId interning, gating   │
│  └─────────────┬───────────────────┘                                │
│                │                                                    │
│                ▼                                                    │
│  ┌─────────────────────────────────┐                                │
│  │  DAG VALIDATION                 │                                │
│  │  Cycle detection, dep resolution│                                │
│  └─────────────┬───────────────────┘                                │
│                │                                                    │
│                ▼                                                    │
│  ┌─────────────────────────────────┐   ┌─────────────────────────┐ │
│  │  RUNTIME EXECUTOR               │──▶│  EVENT SOURCING         │ │
│  │  tokio tasks, JoinSet,          │   │  34 EventKind variants  │ │
│  │  CancellationToken, fail_fast   │   │  NDJSON trace writer    │ │
│  └─────────────────────────────────┘   └─────────────────────────┘ │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Module Breakdown

| Module | Files | Lines | Role |
|--------|------:|------:|------|
| **tui** | 164 | 91.5K | Terminal UI (4 views: Studio, Runner, Chat, Settings) |
| **runtime** | 39 | 24.8K | Execution engine, agent loop, spawn, transforms |
| **ast** | 37 | 21.8K | Two-phase YAML parser (Raw → Analyzed) |
| **binding** | 9 | 10.9K | Data flow, lazy bindings, transform engine |
| **init** | 10 | 8.4K | Project initialization |
| **mcp** | 12 | 7.9K | MCP client (rmcp v0.16) |
| **lsp** | 13 | 6.2K | Language Server Protocol for YAML |
| **provider** | 7 | 4.4K | rig-core v0.32 + mistral.rs native |
| **core** | 8 | 4.1K | Zero-dep provider/model/MCP definitions |
| **new** | 3 | 4.1K | Workflow creation wizard |
| **jobs** | 8 | 3.9K | Background job execution |
| **tools** | 9 | 3.6K | 11 builtin tools (6 core + 5 file) |
| **dag** | 4 | 2.7K | DAG construction and validation |
| **event** | 4 | 2.6K | Event log with 34 variants |
| **registry** | 6 | 2.5K | Package registry |
| **io** | 5 | 1.8K | Atomic writes, security, templates |
| **sync** | 4 | 1.7K | Editor sync (Claude Code, Cursor, etc.) |
| **daemon** | 5 | 1.4K | Background service management |
| **store** | 2 | 1.0K | DashMap-based concurrent data store |
| **source** | 3 | 0.7K | Source file tracking |
| **secrets** | 4 | 0.6K | Keychain + daemon IPC |
| **setup** | 2 | 0.5K | Onboarding wizard |
| **backup** | 2 | 0.5K | Data backup/restore |

### 1. Five Semantic Verbs

| Verb | Icon | Purpose | Implementation |
|------|------|---------|----------------|
| `infer:` | ⚡ | LLM generation | rig-core `CompletionModel`, 6 cloud + 1 native provider |
| `exec:` | 📟 | Shell commands | shlex parsing, `shell: false` default, command blocklist |
| `fetch:` | 🛰️ | HTTP requests | reqwest, auto-JSON body via `json:` field |
| `invoke:` | 🔌 | MCP tool calls | rmcp v0.16, timeout enforcement (30s) |
| `agent:` | 🐔 | Multi-turn agentic loop | rig-core `AgentBuilder`, spawning, depth limits |

**Verb syntax:** `infer:` and `exec:` support shorthand string form (`infer: "prompt"`) plus full object form (`infer: { prompt, model, temperature }`).

### 2. LLM Provider Ecosystem

**6 cloud providers** (rig-core v0.32):

| Provider | Constructor | Default Model |
|----------|-------------|---------------|
| Claude | `RigProvider::claude()` | claude-sonnet-4-6 |
| OpenAI | `RigProvider::openai()` | gpt-4o |
| Mistral | `RigProvider::mistral()` | mistral-large-latest |
| Groq | `RigProvider::groq()` | llama-3.3-70b-versatile |
| DeepSeek | `RigProvider::deepseek()` | deepseek-chat |
| Gemini | `RigProvider::gemini()` | gemini-2.0-flash |

**1 local provider** (v0.26):
- `NativeRuntime` via mistral.rs — GGUF models, Metal/CUDA acceleration
- `provider: native` in workflows, streaming via `infer_stream()`

**Auto-detection:** Priority order checks env vars (ANTHROPIC → OPENAI → MISTRAL → GROQ → DEEPSEEK → GEMINI).

### 3. Agent Capabilities

- **Multi-turn loop:** `RigAgentLoop` with `AgentBuilder`, chat history via rig `Chat` trait
- **Extended thinking:** Claude-only, `thinking_budget: 1024-65536`, reasoning captured in `AgentTurnMetadata`
- **Spawn sub-agents:** `SpawnAgentTool` with `depth_limit` (default 3, max 10)
- **Stop conditions:** Configurable agent termination criteria
- **Chat history:** `add_to_history()`, `chat_continue()`, `with_history()`
- **Streaming:** All 7 providers support real-time token streaming

### 4. Builtin Tools (11)

**Core tools (6):**
`nika:sleep`, `nika:log`, `nika:emit`, `nika:assert`, `nika:prompt`, `nika:run`

**File tools (5, agent-only):**
`nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`

### 5. Data Flow & Bindings

- **`use:` block:** Bind upstream task outputs to aliases
- **`$task` implicit syntax:** `$step1` sugar for `step1` (v0.21)
- **`{{use.alias}}` templates:** Variable interpolation in prompts
- **`{{context.files.alias}}`:** Context file references
- **`{{inputs.*}}`:** Workflow input parameter access
- **Lazy bindings:** `lazy: true` defers resolution until access (v0.5)

### 6. Transform Engine (v0.28)

30+ chained operations via pipe syntax (`sort | unique | first(3)`):

| Category | Operations |
|----------|-----------|
| String | `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `replace`, `slice`, `truncate`, `capitalize`, `camel_case`, `snake_case`, `kebab_case` |
| Collection | `length`, `first`, `last`, `nth`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `filter`, `map`, `group_by`, `zip` |
| Type | `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json`, `type_of` |
| Numeric | `round`, `abs`, `ceil`, `floor`, `min`, `max`, `sum`, `avg` |
| Utility | `default`, `join`, `split`, `shell`, `regex_match`, `regex_replace` |

### 7. DAG Execution

- **Parallel execution:** `for_each` with `concurrency` control and `JoinSet`
- **`fail_fast:`** `tokio::select!` cancellation of in-flight tasks
- **`DependencyFailed`/`DependencyChainFailed`:** Cascading failure propagation
- **Deadlock detection:** Distinguishes true cycles from chain failures
- **Decompose modifier:** Runtime DAG expansion via MCP traversal

### 8. Structured Output

- **JSON Schema enforcement:** 4-layer validation pipeline
  - Layer 1: JSON extraction from LLM response
  - Layer 2: Schema validation
  - Layer 3: Retry with feedback via `InferCallback`
  - Layer 4: LLM repair via `InferCallback`
- **Output policy:** Task-level schema injection for infer/agent prompts

### 9. MCP Client (rmcp v0.16)

- **Server management:** 48 pre-configured aliases via `MCP_ALIASES`
- **Timeout enforcement:** 30s default, 5min deadline for tasks
- **Error code preservation:** JSON-RPC error codes mapped to `McpErrorCode`
- **Cache invalidation:** Tool + response caches invalidated on disconnect
- **Connection lifecycle:** Auto-start, health check, reconnect

### 10. Workflow Composition

- **`context:`** File loading at workflow start (markdown, JSON, YAML, glob patterns)
- **`include:`** DAG fusion from external workflows with prefix namespacing
- **`skills:`** Skill definition merging through DAG fusion, `pkg:` URI resolution
- **Schema versions:** `nika/workflow@0.1` through `@0.11`

### 11. Security

- **Shell-free execution:** `exec:` defaults to `shell: false` (shlex parsing)
- **Command blocklist:** Dangerous binaries blocked (`rm -rf`, `sudo`, etc.)
- **Path traversal protection:** `validate_path_boundary()` for include/context
- **Template injection prevention:** Sanitized variable interpolation
- **TOCTOU mitigation:** Atomic writes with temp+fsync+rename

### 12. Observability

- **Event sourcing:** 34 `EventKind` variants across 11 categories
- **NDJSON traces:** Per-run trace files with full event replay
- **`AgentTurnMetadata`:** thinking, tokens, stop_reason, tool_calls, cache tokens
- **Broadcast channels:** Real-time event streaming to TUI

### 13. TUI (91.5K lines)

**4 Views:**
| View | Key | Description |
|------|-----|-------------|
| Studio | `1`/`s` | 3-panel: Browser + Editor + DAG Preview |
| Runner | `2`/`r` | Real-time workflow execution monitor |
| Chat | `3`/`c` | Conversational agent interface |
| Settings | `4`/`,` | Provider config and preferences |

**Studio features:** Edit history (undo/redo), session persistence, Solarized theme, syntax highlighting, schema validation, tab management, fuzzy file search, command palette.

### 14. CLI Commands (v0.27 spn fusion)

| Category | Commands |
|----------|----------|
| **Run** | `nika workflow.nika.yaml`, `nika check`, `nika tui`, `nika chat`, `nika studio` |
| **Trace** | `nika trace list/show/export/clean` |
| **Provider** | `nika provider list/set/get/test/migrate` |
| **Model** | `nika model list/pull/info/search` |
| **MCP** | `nika mcp add/remove/list/test/tools` |
| **Sync** | `nika sync --enable/--disable/--status` |
| **Jobs** | `nika jobs submit/cancel/output/list` |
| **Backup** | `nika backup create/restore/list/prune` |
| **Setup** | `nika setup nika/novanet/claude-code` |
| **Daemon** | `nika daemon start/stop/status` |

### 15. Error System

20+ error code categories spanning NIKA-000 through NIKA-429:

| Range | Category | Count |
|-------|----------|-------|
| 000-009 | Workflow | 10 |
| 010-019 | Schema | 10 |
| 020-029 | DAG | 10 |
| 030-039 | Provider | 10 |
| 040-049 | Binding | 10 |
| 050-059 | Security | 10 |
| 060-069 | Output/JSON | 10 |
| 070-079 | Use block | 10 |
| 080-089 | DAG validation | 10 |
| 090-099 | JSONPath/IO | 10 |
| 100-109 | MCP | 10 |
| 110-119 | Agent | 10 |
| 120-129 | Resilience | 10 |
| 130-139 | TUI | 10 |
| 140-149 | AST analysis | 10 |
| 200-209 | File tools | 10 |
| 210-219 | Builtin tools | 10 |
| 280-289 | Artifact | 10 |
| 300-309 | Structured output | 10 |
| 400-429 | Daemon/IO/Sync | 30 |

### 16. Artifact System (v0.18)

- **Atomic writes:** temp+fsync+rename pattern
- **Security:** Path validation, traversal prevention
- **Templates:** `{{task_id}}`, `{{date}}`, variable interpolation
- **Events:** `ArtifactWritten`, `ArtifactFailed`

### 17. LSP (Language Server Protocol)

13 files, 6.2K lines — IDE integration for `.nika.yaml` files with diagnostics and completion.

### 18. Secrets Management (v0.27)

- **Resolution chain:** daemon → keychain → environment variables
- **spn daemon IPC:** Unix socket at `~/.spn/daemon.sock`
- **18 known providers:** 6 LLM + 11 MCP + 1 Local

---

## NovaNet v0.20.0 — The Brain

### Schema

- **59 NodeClasses** across 2 realms (Shared: 36, Org: 23)
- **159 ArcClasses** in 5 families (ownership, localization, semantic, generation, mining)
- **5 layers per realm:** config, locale, geography, knowledge (Shared); config, foundation, structure, semantic, instruction, output (Org)

### MCP Server (14 → 8 tools in v0.20)

| Tool | Purpose |
|------|---------|
| `novanet_describe` | Bootstrap graph understanding |
| `novanet_introspect` | Schema inspection (classes, arcs) |
| `novanet_search` | Find nodes (fulltext, property, hybrid, walk, triggers) |
| `novanet_context` | Build LLM context (page, block, knowledge, assemble) |
| `novanet_write` | Create/update data (with `dry_run` validation) |
| `novanet_audit` | Quality checks + CSR metrics |
| `novanet_batch` | Parallel operations |
| `novanet_query` | Raw Cypher (last resort) |

### Knowledge Atoms

| Type | Purpose |
|------|---------|
| Term | Technical vocabulary with definitions |
| Expression | Idiomatic expressions per locale |
| Pattern | Text templates/patterns |
| CultureRef | Cultural references |
| Taboo | Things to avoid per locale |
| AudienceTrait | Audience characteristics |

### Key Patterns

- ***Native Pattern (ADR-029):** `EntityNative`, `PageNative`, `BlockNative` — unified suffix
- **Slug Ownership (ADR-030):** Page owns URL, Entity owns semantics
- **Denomination Forms (ADR-033):** text/title/abbrev/mixed/base/url
- **Inverse Arc Tiers (ADR-026):** 3-tier system (Required/Recommended/Optional)

### Neo4j Integration

- 1,210 tests
- Cypher as source of truth (ADR-021)
- Fulltext indexes for search
- APOC for schema inspection

---

## Nika ↔ NovaNet Integration

```yaml
# The integration pattern
workflow: generate-page
mcp:
  servers:
    novanet:
      command: node
      args: ["/path/to/novanet-mcp/dist/index.js"]

tasks:
  - id: get_context
    invoke: novanet_context
    params:
      focus_key: "homepage"
      locale: "fr-FR"
      mode: page

  - id: generate
    use:
      ctx: $get_context
    infer: |
      Generate landing page content:
      {{use.ctx}}
```

**Protocol:** MCP (Model Context Protocol) — Zero Cypher rule enforced (ADR-003).
Nika never queries Neo4j directly. All graph access flows through NovaNet MCP tools.

---

## Summary Statistics

| Metric | Nika | NovaNet | Combined |
|--------|------|---------|----------|
| Tests | 6,157 | 1,210 | 7,367 |
| Source files | 371 | ~200 | ~571 |
| Lines of Rust | 219K | ~50K | ~269K |
| Providers | 7 | 0 | 7 |
| MCP tools | 11 builtin | 8 exposed | 19 |
| Error codes | 20+ ranges | — | 20+ |
| CLI commands | 30+ | 10+ | 40+ |
