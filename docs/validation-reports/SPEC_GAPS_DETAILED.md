# Nika Spec - Detailed Gap Analysis

## Missing Verb #1: `invoke:` (MCP Tool Calls)

**Status in spec:** Not mentioned
**Status in code:** Full implementation (v0.2+, 150+ LOC)
**Introduced:** 2026-02-18 (v0.2)

### What the spec should document:

```yaml
# invoke: - Call MCP server tools

invoke:
  server: novanet              # MCP server name
  tool: novanet_generate       # Tool name
  params:
    entity: "qr-code"
    locale: "fr-FR"
    forms: ["text", "title"]
```

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `server` | string | Yes | MCP server name (must be defined in `mcp:` section) |
| `tool` | string | Yes | Tool name on the MCP server |
| `params` | object | Yes | Tool parameters (validated against schema) |
| `timeout_secs` | integer | No | Timeout for tool call (default: 30s) |

### Example: Using NovaNet MCP

```yaml
schema: nika/workflow@0.9
provider: claude

mcp:
  servers:
    novanet:
      command: "novanet-mcp"
      env:
        NEO4J_URI: bolt://localhost:7687

tasks:
  - id: get_entity
    invoke:
      server: novanet
      tool: novanet_describe
      params:
        entity: "entity:qr-code"
        depth: 2

  - id: generate_content
    use:
      ctx: get_entity
    invoke:
      server: novanet
      tool: novanet_generate
      params:
        focus_key: "{{use.ctx.key}}"
        locale: "fr-FR"
```

### Error Codes

| Code | Error | Cause |
|------|-------|-------|
| NIKA-100 | MCP server not connected | Server not running |
| NIKA-101 | MCP server failed to start | Wrong command/args |
| NIKA-102 | MCP tool call failed | Invalid params, tool error |
| NIKA-103 | MCP resource not found | URI doesn't exist |
| NIKA-104 | MCP protocol error | JSON-RPC issue |
| NIKA-105 | MCP not configured | Server not in `mcp:` section |
| NIKA-106 | MCP invalid response | Response format wrong |
| NIKA-107 | MCP validation failed | Missing required params |
| NIKA-108 | MCP schema error | Tool schema mismatch |
| NIKA-109 | MCP timeout | Operation exceeded 30s |

---

## Missing Verb #2: `agent:` (Multi-Turn Agentic Loops)

**Status in spec:** Not mentioned
**Status in code:** Full implementation (v0.2+, 1,500+ LOC)
**Introduced:** 2026-02-18 (v0.2)

### What the spec should document:

```yaml
# agent: - Multi-turn agentic loop with tool use

agent:
  prompt: "Decompose and solve this problem"
  mcp: [novanet, web_search]          # Available tools
  max_turns: 10                        # Prevent infinite loops
  depth_limit: 3                       # Nested agent protection
  thinking: true                       # Extended thinking (Claude)
  stop_conditions: ["DONE", "ERROR"]   # When to stop
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `prompt` | string | required | Agent goal/instruction |
| `mcp` | array | [] | MCP server names to expose as tools |
| `max_turns` | integer | 10 | Max conversation turns |
| `depth_limit` | integer | 3 | Max nested agents (spawn_agent) |
| `thinking` | boolean | false | Extended thinking (Claude only) |
| `stop_conditions` | array | [] | Strings that trigger completion |
| `temperature` | float | default | LLM creativity (0.0-1.0) |
| `max_tokens` | integer | default | Output length limit |

### Example: Research Agent with MCP

```yaml
schema: nika/workflow@0.9
provider: claude

mcp:
  servers:
    web_search:
      command: "npx @anthropic/mcp-server-perplexity"
      env:
        PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"
    filesystem:
      command: "npx @anthropic/mcp-filesystem"
      env:
        ALLOWED_PATHS: "./output"

tasks:
  - id: research_agent
    agent:
      prompt: |
        Research "AI Safety in 2024" and create a report:
        1. Find 5 recent papers
        2. Extract key findings
        3. Identify gaps
        4. Save report to disk

      mcp: [web_search, filesystem]
      max_turns: 15
      depth_limit: 3
      thinking: true
      stop_conditions: ["RESEARCH_COMPLETE"]
```

### Spawning Nested Agents

The `spawn_agent` tool is automatically available in agents:

```rust
// Agent can use this tool:
{
  "tool": "spawn_agent",
  "params": {
    "task_id": "subtask-1",
    "prompt": "Analyze this paper",
    "context": {"paper": "..."},
    "max_turns": 5
  }
}
```

### Error Codes

| Code | Error | Cause |
|------|-------|-------|
| NIKA-110 | Agent max turns exceeded | Loop didn't finish |
| NIKA-111 | Stop condition not met | Condition never triggered |
| NIKA-112 | Invalid tool name | Tool doesn't exist |
| NIKA-113 | Agent validation failed | Config error |
| NIKA-115 | Agent execution failed | Runtime error |
| NIKA-116 | Thinking capture failed | Extended thinking error |
| NIKA-117 | Thinking not supported | Provider doesn't support it |

---

## Missing Feature #1: `for_each` Parallelism (v0.3+)

**Status in spec:** Not mentioned
**Status in code:** Full implementation (500+ LOC)
**Introduced:** 2026-02-18 (v0.3)

### What the spec should document:

```yaml
tasks:
  - id: generate_pages
    for_each: ["fr-FR", "en-US", "de-DE"]  # Array or binding
    as: locale                              # Loop variable
    concurrency: 3                          # Max parallel tasks
    fail_fast: true                         # Stop on error
    infer: "Generate page for {{use.locale}}"
```

### Configuration

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `for_each` | array/string | required | Array to iterate over |
| `as` | string | "item" | Loop variable name |
| `concurrency` | integer | 1 | Max parallel executions |
| `fail_fast` | boolean | true | Stop all on first error |

### Example: Parallel Locale Generation

```yaml
schema: nika/workflow@0.9

tasks:
  - id: generate_all_locales
    for_each: ["en-US", "fr-FR", "de-DE", "es-ES", "ja-JP"]
    as: locale
    concurrency: 5  # All 5 in parallel
    infer:
      prompt: "Generate landing page for locale: {{use.locale}}"

  - id: validate_all
    use:
      pages: generate_all_locales  # Array of results
    infer: "Validate all {{use.pages}} for quality"
```

### Error Codes

- NIKA-040 (Binding error) — Array not found
- NIKA-041 (Template error) — {{use.item}} syntax

---

## Missing Feature #2: `context:` File Loading (v0.14.2+)

**Status in spec:** Not mentioned
**Status in code:** Full implementation (200+ LOC)
**Introduced:** 2026-02-28 (v0.14.2)

### What the spec should document:

```yaml
schema: nika/workflow@0.9

context:
  files:
    brand: ./context/brand.md
    config: ./context/settings.json
    examples: ./context/*.md
  session: .nika/sessions/prev.json

tasks:
  - id: generate
    infer: "Use brand: {{context.files.brand}}"
```

### Configuration

| Key | Type | Description |
|-----|------|-------------|
| `context.files.*` | object | Map of alias → file path |
| `context.session` | string | Session file to restore |

### Features

- Load markdown, JSON, YAML, glob patterns
- Accessible via `{{context.files.alias}}`
- Session restore on workflow start
- Path traversal protection (security)

### Error Codes

| Code | Error | Cause |
|------|-------|-------|
| NIKA-250 | Context load error | File not found/unreadable |

---

## Missing Feature #3: `include:` DAG Fusion (v0.14.2+)

**Status in spec:** Not mentioned
**Status in code:** Full implementation (300+ LOC)
**Introduced:** 2026-02-28 (v0.14.2)

### What the spec should document:

```yaml
schema: nika/workflow@0.9

include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
  - path: ./partials/cleanup.nika.yaml

tasks:
  - id: main
    infer: "Main logic"

flows:
  - source: setup_init
    target: main
  - source: main
    target: cleanup_finalize
```

### Configuration

| Property | Type | Description |
|----------|------|-------------|
| `path` | string | Path to workflow file |
| `prefix` | string | Prefix for included task IDs |

### Features

- Merge external workflows into DAG
- Task ID prefixing prevents collisions
- Recursive include with cycle detection
- Path traversal protection

---

## Missing Feature #4: `decompose:` Runtime DAG Expansion (v0.5+)

**Status in spec:** Not mentioned
**Status in code:** Full implementation (250+ LOC)
**Introduced:** 2026-02-20 (v0.5)

### What the spec should document:

```yaml
tasks:
  - id: generate_all_entities
    decompose:
      strategy: semantic    # semantic | static | nested
      traverse: HAS_CHILD   # Arc type to follow
      source: $entity       # Starting node
      max_items: 10         # Limit items
    infer: "Generate for {{use.item}}"
```

---

## Missing Feature #5: Lazy Bindings with `lazy: true` (v0.5+)

**Status in spec:** Not mentioned
**Status in code:** Full implementation (200+ LOC)
**Introduced:** 2026-02-20 (v0.5)

### What the spec should document:

```yaml
use:
  # Eager (default) - resolved at task start
  eager_val: task1.result

  # Lazy (v0.5) - resolved on first access
  lazy_val:
    path: future_task.result
    lazy: true
    default: "fallback"
```

---

## Missing Feature #6: Security - Shell-Free Execution (v0.15.0+)

**Status in spec:** Not mentioned
**Status in code:** Full implementation
**Introduced:** 2026-02-28 (v0.15.0)

### What the spec should document:

```yaml
tasks:
  # Default (v0.15.0): Shell-free, safe
  - id: safe_exec
    exec:
      command: "echo 'Hello World'"
      shell: false  # default, can omit

  # Opt-in shell mode (for pipes/redirects)
  - id: pipeline
    exec:
      command: "cat file.txt | grep pattern"
      shell: true  # required for shell features
```

### Security Features

- `shell: false` (default) — Parsed via shlex
- Command blocklist prevents `rm -rf`, `sudo`, etc.
- NIKA-053 error code for blocked commands

### Error Codes

| Code | Error |
|------|-------|
| NIKA-053 | BlockedCommand — dangerous command rejected |

---

## Missing Providers (v0.6+, v0.15.0+)

**Status in spec:** 3 providers only
**Status in code:** 7 providers total

### Missing Providers

| Provider | Added | Env Var | Default Model |
|----------|-------|---------|---------------|
| Mistral | v0.6 | `MISTRAL_API_KEY` | mistral-large-latest |
| Groq | v0.6 | `GROQ_API_KEY` | llama-3.3-70b-versatile |
| DeepSeek | v0.6 | `DEEPSEEK_API_KEY` | deepseek-chat |
| Ollama | v0.6 | `OLLAMA_API_BASE_URL` | llama3.2 |
| Gemini | v0.15.0 | `GEMINI_API_KEY` | gemini-2.0-flash |

### Auto-Detection Priority

```rust
1. ANTHROPIC_API_KEY → Claude
2. OPENAI_API_KEY → OpenAI
3. MISTRAL_API_KEY → Mistral
4. GROQ_API_KEY → Groq
5. DEEPSEEK_API_KEY → DeepSeek
6. GEMINI_API_KEY → Gemini
7. OLLAMA_API_BASE_URL → Ollama
```

---

## Error Code Ranges Not Documented

### Ranges 100-259 (157 codes missing from spec)

| Range | Category | Count | Added |
|-------|----------|-------|-------|
| 000-009 | Workflow errors | 5 | v0.1 |
| 010-019 | Task/schema errors | 4 | v0.1 |
| 020-029 | DAG errors | 2 | v0.1 |
| 030-039 | Provider errors | 4 | v0.1 |
| 040-049 | Binding/template errors | 6 | v0.1 |
| **100-109** | **MCP errors** | **10** | **v0.2** |
| **110-119** | **Agent errors** | **8** | **v0.2** |
| **120-129** | **Resilience errors** | **3** | **v0.2** |
| **130-139** | **TUI errors** | **1** | **v0.2** |
| **140-149** | **Config errors** | **1** | **v0.5** |
| **150-159** | **Startup errors** | **1** | **v0.8** |
| **160-169** | **Policy errors** | **2** | **v0.13** |
| **170-179** | **Runtime errors** | **1** | **v0.14** |
| **210-219** | **Builtin tool errors** | **4** | **v0.9** |
| **250-259** | **Context errors** | **1** | **v0.14.2** |
| **260-269** | **pkg: URI errors** | **1** | **v0.15.2** |

---

## Summary of Missing Content

Total pages needed to document all gaps: **40+ pages**

- **Verbs:** 2 complete verb specs (invoke, agent)
- **Features:** 6 feature sections (for_each, context, include, decompose, lazy, spawn_agent)
- **Security:** 1 security section
- **Providers:** 1 provider reference table
- **Error codes:** 5 pages of organized error references
- **Examples:** 8 new example workflows
- **Types:** Updated Rust type signatures

