# Nika CLI — Claude Code Context

## Overview

Nika is a DAG workflow runner for AI tasks with MCP integration. It's the "body" of the spn-agi architecture, executing workflows that leverage NovaNet's knowledge graph "brain".

**Current version:** v0.27.0 | spn→nika Feature Fusion | 5,054 tests | Zero clippy warnings

**v0.27.0 Changes (spn→nika Fusion):**
- **Unified CLI** — All spn features now available via `nika` commands
- **New commands:** provider, model, mcp, sync, setup, daemon, jobs, backup
- **Core module** — `src/core/` with zero-dep provider/model/MCP definitions
- **spn deprecation** — `spn` CLI now shows deprecation warnings

## Architecture

```
tools/nika/src/
├── main.rs           # CLI entry point
├── lib.rs            # Public API
├── error.rs          # NikaError with codes
├── core/             # Zero-dep provider/model/MCP definitions (v0.27)
│   ├── mod.rs        # Re-exports for KNOWN_PROVIDERS, KNOWN_MODELS, MCP_ALIASES
│   ├── providers.rs  # KNOWN_PROVIDERS (6 LLM + 11 MCP + 1 Local = 18)
│   ├── models.rs     # KNOWN_MODELS (16+ curated models for native inference)
│   ├── mcp_aliases.rs # MCP_ALIASES (48 server aliases)
│   └── mcp_config.rs # McpConfig, McpServer, global/project config loading
├── secrets/          # Unified secrets management (v0.27)
│   ├── mod.rs        # KeychainResolver, DaemonClient integration
│   └── resolve.rs    # Resolution chain: daemon → keychain → env
├── ast/              # Two-phase YAML parsing (v0.20)
│   ├── raw/          # Phase 1: YAML → Raw AST (with spans)
│   │   ├── parser.rs     # marked_yaml parser
│   │   ├── workflow.rs   # RawWorkflow
│   │   ├── task.rs       # RawTask, RawForEach, RawRetry
│   │   ├── action.rs     # RawTaskAction (5 verbs)
│   │   └── mcp.rs        # RawMcpConfig, RawMcpServer
│   ├── analyzed/     # Phase 2: Raw → Analyzed (validated)
│   │   ├── workflow.rs   # AnalyzedWorkflow with TaskTable
│   │   ├── task.rs       # AnalyzedTask with TaskId
│   │   ├── action.rs     # AnalyzedTaskAction (typed)
│   │   └── mcp.rs        # AnalyzedMcpServer
│   ├── analyzer/     # Validation + transformation
│   │   ├── analyze.rs    # Main analyze() function
│   │   ├── errors.rs     # AnalyzeError (NIKA-140-149)
│   │   └── feature_gate.rs # Schema version gating
│   ├── context.rs    # ContextSpec (v0.14.3 - file loading)
│   ├── include.rs    # IncludeSpec (v0.14.3 - DAG fusion)
│   ├── include_loader.rs # Include resolution + prefix + skill merging
│   ├── skill_def.rs  # SkillDef struct (v0.15.1 - skill merging)
│   ├── pkg_resolver.rs # pkg: URI resolution (v0.15.1)
│   ├── decompose.rs  # DecomposeSpec (v0.5 MVP 8)
│   └── output.rs     # OutputSpec
├── dag/              # DAG validation
├── runtime/          # Execution engine
│   ├── executor.rs   # Task dispatch + decompose expansion
│   ├── runner.rs     # Workflow orchestration
│   ├── output.rs     # Output format handling
│   ├── spawn.rs      # SpawnAgentTool (v0.5 MVP 8)
│   └── rig_agent_loop.rs # rig-core AgentBuilder (v0.4+)
├── mcp/              # MCP client (rmcp v0.16)
├── event/            # Event sourcing
│   ├── log.rs        # EventLog (22 variants)
│   └── trace.rs      # NDJSON writer
├── tui/              # Terminal UI (feature-gated)
├── binding/          # Data flow ({{use.alias}}) + lazy bindings
│   ├── entry.rs      # UseEntry with lazy flag (v0.5)
│   └── resolve.rs    # LazyBinding enum (v0.5)
├── provider/         # LLM providers (rig-core + native)
│   ├── rig.rs        # RigProvider + NikaMcpTool (rig-core v0.32)
│   └── native/       # NativeRuntime (mistral.rs, v0.26.0)
└── store/            # RunContext (task results + context + inputs)
```

## Unified Secrets Management (v0.20.1)

Nika integrates with the `spn daemon` for secure credential management via Unix socket IPC.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  spn-daemon INTEGRATION (--features spn-daemon)                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Without daemon:              With daemon:                                      │
│  Nika → Keychain (popup)      Nika → spn-client → ~/.spn/daemon.sock            │
│  MCP1 → Keychain (popup)                          ↓                             │
│  MCP2 → Keychain (popup)                    OS Keychain                         │
│                                           (one accessor, no popups)             │
│                                                                                 │
│  Resolution priority:                                                           │
│  1. spn daemon (IPC)  ─┐                                                        │
│  2. OS Keychain        ├─ KNOWN_PROVIDERS from spn-core (13 providers)          │
│  3. Environment vars  ─┘                                                        │
│                                                                                 │
│  Providers:                                                                     │
│  ├── LLM: anthropic, openai, mistral, groq, deepseek, gemini (v0.27: -Ollama)  │
│  ├── Native: local GGUF inference via mistral.rs (v0.27)                       │
│  └── MCP: neo4j, github, slack, perplexity, firecrawl, supadata                │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Problem Solved:** macOS Keychain repeatedly prompts "allow access?" for each process.
With Nika spawning multiple MCP servers, this was unbearable.

**Solution:** The `spn daemon` is the SOLE keychain accessor. Nika and all MCP servers
connect via Unix socket IPC. One auth prompt at daemon start, then silence.

**Feature flag:** `cargo build --features spn-daemon` (enabled by default)

**Implementation:** `src/secrets.rs` uses `spn_client::KNOWN_PROVIDERS` as the single
source of truth for provider definitions (env var names, key prefixes, validation).

## Key Concepts

- **Workflow:** YAML file with tasks and flows
- **Task:** Single unit of work (infer, exec, fetch, invoke, agent)
- **Flow:** Dependency edge between tasks
- **Verb:** Action type (infer:, exec:, fetch:, invoke:, agent:)
- **Binding:** Data passing via `use:` block and `{{use.alias}}`

## File Conventions

### Workflow File Extension

All Nika workflow files **MUST** use the `.nika.yaml` extension:

```
workflow.nika.yaml     Correct
workflow.yaml          Wrong (ambiguous)
workflow.nika          Wrong (not YAML)
```

### JSON Schema Validation

Workflows are validated against `schemas/nika-workflow.schema.json`:

```bash
# Validate single file
cargo run -- validate workflow.nika.yaml

# Validate directory
cargo run -- validate examples/
```

### VS Code Integration

Schema auto-completion is enabled via `.vscode/settings.json`:

```json
{
  "yaml.schemas": {
    "./schemas/nika-workflow.schema.json": "*.nika.yaml"
  }
}
```

## Schema Versions

- `nika/workflow@0.1`: infer, exec, fetch verbs
- `nika/workflow@0.2`: +invoke, +agent verbs, +mcp config
- `nika/workflow@0.3`: +for_each parallelism, rig-core integration
- `nika/workflow@0.5`: +decompose, +lazy bindings, +spawn_agent (MVP 8)
- `nika/workflow@0.6`: +multi-provider support (6 providers)
- `nika/workflow@0.7`: +full streaming for all providers
- `nika/workflow@0.8`: +Studio DX (edit history, sessions, themes, config)
- `nika/workflow@0.9`: +context: file loading, +include: DAG fusion (v0.14.3)
- `nika/workflow@0.10`: +two-phase AST, +analyzer validation (v0.20)
- `nika/workflow@0.11`: +native inference (provider: native, mistral.rs, v0.26.0)

## Two-Phase AST Architecture (v0.20)

The AST module uses a two-phase parsing architecture for IDE integration:

```
YAML Source → [Phase 1: Parser] → RawWorkflow → [Phase 2: Analyzer] → AnalyzedWorkflow
                  ↓                    ↓                                    ↓
             marked_yaml         Spans preserved              TaskId interning
                                 All fields Optional          Semantic validation
                                 No validation                Feature gating
```

### Phase 1: Raw AST (`ast::raw`)

Parses YAML with full source position tracking via `marked_yaml`:

```rust
use nika::ast::raw::{parse, RawWorkflow};
use nika::source::FileId;

let raw: RawWorkflow = parse(yaml_content, FileId(0))?;
// raw.schema.span → precise location in source
```

**Key types:** `RawWorkflow`, `RawTask`, `RawTaskAction`, `RawMcpConfig`

### Phase 2: Analyzed AST (`ast::analyzed`)

Validates and transforms the raw AST:

```rust
use nika::ast::analyzer::analyze;
use nika::ast::analyzed::AnalyzedWorkflow;

let result = analyze(raw);
if result.is_ok() {
    let workflow: AnalyzedWorkflow = result.value.unwrap();
    // workflow.get_task_by_name("step1") → O(1) lookup via TaskTable
}
```

**Key features:**
- **TaskId interning:** O(1) task comparison and lookup
- **Schema version gating:** Features like `for_each` require v0.3+
- **Error collection:** Multiple errors reported, not just first
- **Span preservation:** Errors point to exact source locations

### Analyzer Error Codes (NIKA-140-149)

| Code | Kind | Description |
|------|------|-------------|
| NIKA-140 | UnknownTask | Referenced task doesn't exist |
| NIKA-141 | DuplicateTask | Task ID defined multiple times |
| NIKA-142 | InvalidSchema | Invalid schema version string |
| NIKA-143 | CyclicDependency | Tasks form a dependency cycle |
| NIKA-144 | InvalidValue | Field has invalid value |
| NIKA-145 | MissingField | Required field not provided |
| NIKA-146 | InvalidTemplate | Template expression is malformed |
| NIKA-147 | UnknownFlow | Flow references unknown task |
| NIKA-148 | UnknownMcpServer | MCP server not configured |
| NIKA-149 | UnsupportedFeature | Feature not available in schema version |

## Workflow Syntax Quick Reference

### Correct Task Binding Pattern

Use `use:` block on dependent tasks to reference outputs from upstream tasks:

```yaml
tasks:
  - id: step1
    infer: "Generate something"

  - id: step2
    use:
      result: step1           # Bind step1's output to 'result' alias
    infer: "Process: {{use.result}}"
```

**WRONG patterns to avoid:**
- `output: use.xxx: result` - This syntax does not exist
- `flow:` inside tasks - Use `flows:` at workflow level instead

### Implicit Output Syntax (v0.21)

Reference task outputs without explicit `.output` suffix using the `$` prefix:

```yaml
tasks:
  - id: step1
    infer: "Generate a title"

  - id: step2
    use:
      title: $step1           # Shorthand - same as "step1"
    infer: "Expand on: {{use.title}}"
```

### Context Paths

Context file paths are relative to **project root** (where `nika run` is executed), not to the workflow file:

```yaml
context:
  files:
    data: ./context/data.json     # Correct - relative to project root
```

### Builtin Tools via invoke:

Core builtin tools (6) work in `invoke:` tasks with `mcp: dummy`:

```yaml
mcp:
  dummy:
    command: "echo"
    args: ["not used"]

tasks:
  - id: log_it
    invoke:
      mcp: dummy
      tool: nika:log
      params:
        level: info
        message: "Hello!"
```

**Available core tools:** `nika:sleep`, `nika:log`, `nika:emit`, `nika:assert`, `nika:prompt`, `nika:run`

**File tools** (`nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`) are **only available inside `agent:` tasks**, not in `invoke:` tasks.

### flows: Section

Define task dependencies at workflow level, not inside tasks:

```yaml
tasks:
  - id: a
    infer: "Step A"
  - id: b
    infer: "Step B"

flows:
  - source: a
    target: b
```

### for_each Parallelism

`for_each` accepts arrays or `$binding` references (not `{{context.}}` syntax):

```yaml
tasks:
  - id: parallel_task
    for_each: ["item1", "item2", "item3"]  # Array
    # for_each: "$items"                   # Binding ref
    as: item
    concurrency: 3
    infer: "Process {{use.item}}"
```

## Version History

For detailed version changelogs, see `CHANGELOG.md`. Key milestones:

| Version | Highlight |
|---------|-----------|
| v0.27.0 | spn→nika Feature Fusion, Unified CLI |
| v0.26.0 | Native Inference (mistral.rs, ADR-008) |
| v0.24.0 | Control Flow (fail_fast, DependencyFailed) |
| v0.20.0 | Two-Phase AST, 4-View TUI |
| v0.15.0 | Security hardening, Gemini, 6 providers |
| v0.14.3 | context: + include: DAG fusion |
| v0.8.0 | Studio DX (edit history, sessions, themes) |
| v0.5.0 | MVP 8 (spawn_agent, decompose, lazy bindings) |

## Testing Strategy

- **Unit tests:** In-file `#[cfg(test)]` modules
- **Integration tests:** `tests/` directory
- **Snapshot tests:** insta for YAML/JSON outputs
- **Property tests:** proptest for parser fuzzing
- **Real API tests:** `examples/test-*.nika.yaml` (require API keys)

## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Task errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Binding errors |
| NIKA-050-059 | Security errors |
| NIKA-060-069 | JSON validation errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |
| NIKA-140-149 | AST analysis errors (v0.20) |

## Conventions

- **Imports:** Group by std, external, internal
- **Error handling:** Use `NikaError` with codes, not `anyhow`
- **Logging:** Use `tracing` macros (debug!, info!, warn!, error!)
- **Tests:** TDD - write failing test first
- **Commits:** Conventional commits with scope
