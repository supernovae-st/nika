# Nika CLI — Claude Code Context

## Overview

Nika is a DAG workflow runner for AI tasks with MCP integration. It's the "body" of the spn-agi architecture, executing workflows that leverage NovaNet's knowledge graph "brain".

**Current version:** v0.16.3 | DX Consolidation + Registry + Skill Merging | 3,358 tests | Zero clippy warnings

## Architecture

```
tools/nika/src/
├── main.rs           # CLI entry point
├── lib.rs            # Public API
├── error.rs          # NikaError with codes
├── ast/              # YAML → Rust structs
│   ├── workflow.rs   # Workflow, Task
│   ├── action.rs     # TaskAction (5 variants)
│   ├── context.rs    # ✅ ContextSpec (v0.14.3 - file loading)
│   ├── include.rs    # ✅ IncludeSpec (v0.14.3 - DAG fusion)
│   ├── include_loader.rs # Include resolution + prefix + skill merging
│   ├── skill_def.rs  # ✅ SkillDef struct (v0.15.1 - skill merging)
│   ├── pkg_resolver.rs # ✅ pkg: URI resolution (v0.15.1)
│   ├── decompose.rs  # ✅ DecomposeSpec (v0.5 MVP 8)
│   └── output.rs     # OutputSpec
├── dag/              # DAG validation
├── runtime/          # Execution engine
│   ├── executor.rs   # Task dispatch + decompose expansion
│   ├── runner.rs     # Workflow orchestration
│   ├── output.rs     # Output format handling
│   ├── spawn.rs      # ✅ SpawnAgentTool (v0.5 MVP 8)
│   └── rig_agent_loop.rs # ✅ rig-core AgentBuilder (v0.4+)
├── mcp/              # MCP client (rmcp v0.16)
├── event/            # Event sourcing
│   ├── log.rs        # EventLog (22 variants)
│   └── trace.rs      # NDJSON writer
├── tui/              # Terminal UI (feature-gated)
├── binding/          # Data flow ({{use.alias}}) + lazy bindings
│   ├── entry.rs      # UseEntry with lazy flag (v0.5)
│   └── resolve.rs    # LazyBinding enum (v0.5)
├── provider/         # LLM providers (rig-core only)
│   └── rig.rs        # ✅ RigProvider + NikaMcpTool (rig-core v0.31)
└── store/            # DataStore
```

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
workflow.nika.yaml     ✅ Correct
workflow.yaml          ❌ Wrong (ambiguous)
workflow.nika          ❌ Wrong (not YAML)
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

### yamllint

YAML linting uses `.yamllint.yaml` configuration:

```bash
yamllint -c .yamllint.yaml **/*.nika.yaml
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

### Context Paths

Context file paths are relative to **project root** (where `nika run` is executed), not to the workflow file:

```yaml
# Workflow at: workflows/my-workflow.nika.yaml
context:
  files:
    data: ./context/data.json     # ✅ Correct - relative to project root
    # data: ../context/data.json  # ❌ Wrong - relative to workflow file
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
    for_each: ["item1", "item2", "item3"]  # ✅ Array
    # for_each: "$items"                   # ✅ Binding ref
    # for_each: "{{context.files.items}}"  # ❌ Invalid
    as: item
    concurrency: 3
    infer: "Process {{use.item}}"
```

## v0.15.1 Changes (Skill Merging Through DAG Fusion)

Workflow-level skills propagate through `include:` DAG fusion:

```yaml
schema: nika/workflow@0.9

skills:
  - path: ./skills/writing.md
    alias: writing
  - path: pkg:@spn/core@1.0.0/skills/coding.md
    alias: coding

include:
  - path: ./partials/setup.nika.yaml
    # Skills from parent workflow are merged with included workflow skills
```

**Key features:**
- `SkillDef` AST type with path and optional alias
- `merge_skills()` function with deduplication and circular detection
- Local paths and `pkg:` URIs supported
- 11 tests for skill merging

**Implementation:**
- `src/ast/skill_def.rs` - SkillDef struct and parsing
- `src/ast/pkg_resolver.rs` - pkg: URI resolution
- `src/ast/include_loader.rs` - Skill merging during DAG fusion

## v0.15.0 Changes (Security + Infer LLM Control + Gemini)

### Security Hardening: Shell-Free Execution

**BREAKING:** `exec:` now defaults to `shell: false` for security:

```yaml
# Default: shell-free (v0.15.0) - uses shlex parsing
- id: safe_exec
  exec:
    command: "echo 'Hello World'"
    shell: false  # default, can be omitted

# Opt-in shell mode for pipes/redirects
- id: pipeline
  exec:
    command: "cat file.txt | grep pattern"
    shell: true  # required for shell features
```

**Security features:**
- `shell: false` (default) - Command parsed via shlex, no shell injection
- Command blocklist prevents dangerous binaries (`rm -rf`, `sudo`, etc.)
- New error code: `NIKA-053 BlockedCommand`
- Implementation: `src/core/security.rs`

### Infer LLM Control Parity

`infer:` now supports temperature, system prompt, and max_tokens:

```yaml
- id: creative_output
  infer:
    prompt: "Generate a tagline"
    temperature: 0.9      # 0.0-1.0, higher = more creative
    system: "You are a marketing expert"
    max_tokens: 100       # Limit output length

- id: precise_output
  infer:
    prompt: "Technical summary"
    temperature: 0.1      # Lower = more deterministic
```

**Implementation:**
- `InferParams` struct: `temperature: Option<f64>`, `system: Option<String>`, `max_tokens: Option<u32>`
- `InferOptions` struct in `provider/rig.rs` for passing to LLM
- `infer_with_options()` method in `RigProvider`

### Gemini Provider (7th provider)

Google's Gemini is now available via rig-core:

```yaml
# Set GEMINI_API_KEY environment variable
schema: "nika/workflow@0.9"
provider: gemini

tasks:
  - id: generate
    infer:
      prompt: "Hello Gemini!"
      model: gemini-2.0-flash
```

**Auto-detection priority (updated):**
1. ANTHROPIC_API_KEY → Claude
2. OPENAI_API_KEY → OpenAI
3. MISTRAL_API_KEY → Mistral
4. GROQ_API_KEY → Groq
5. DEEPSEEK_API_KEY → DeepSeek
6. **GEMINI_API_KEY → Gemini** (NEW)
7. OLLAMA_API_BASE_URL → Ollama

**Methods added:**
- `RigProvider::gemini()` constructor
- `RigAgentLoop::run_gemini()` for agent mode
- Full streaming support with token tracking

### Builtin Tools (11 total - v0.15.1)

Nika provides 11 builtin tools via `BuiltinToolRouter`:

**Core tools (6):**
| Tool | Description | Example |
|------|-------------|---------|
| `nika:sleep` | Pause execution | `{"duration":"1s"}` |
| `nika:log` | Emit log event | `{"level":"info","message":"..."}` |
| `nika:emit` | Custom event | `{"name":"event","payload":{}}` |
| `nika:assert` | Validate condition | `{"condition":true}` |
| `nika:prompt` | HITL user input | `{"message":"Continue?"}` |
| `nika:run` | Execute sub-workflow | `{"workflow":"sub.nika.yaml"}` |

**File tools (5) - NEW in v0.15.1:**
| Tool | Description | Example |
|------|-------------|---------|
| `nika:read` | Read file | `{"file_path":"./file.txt"}` |
| `nika:write` | Create/overwrite file | `{"file_path":"./out.txt","content":"..."}` |
| `nika:edit` | Modify file | `{"file_path":"./f.txt","old_string":"a","new_string":"b"}` |
| `nika:glob` | Find files by pattern | `{"pattern":"*.yaml","path":"./"}` |
| `nika:grep` | Search content | `{"pattern":"TODO","path":"./src"}` |

**Usage:**

```rust
use nika::runtime::builtin::BuiltinToolRouter;
use nika::tools::{ToolContext, PermissionMode};
use std::sync::Arc;

// Core tools only (6)
let router = BuiltinToolRouter::new();

// All 11 tools (core + file)
let ctx = Arc::new(ToolContext::new(
    std::env::current_dir().unwrap(),
    PermissionMode::YoloMode,
));
let router = BuiltinToolRouter::with_file_tools(ctx);

// Dispatch
let result = router.dispatch("nika:write", r#"{"file_path":"./test.txt","content":"Hello"}"#.to_string()).await?;
```

**In workflows:**

```yaml
- id: write_result
  agent:
    prompt: "Generate a report and save it"
    tools: [nika:write, nika:read]  # File tools available to agent
```

## v0.14.3 Changes (context: + include: DAG Fusion)

### Statistics
- **4,369 tests passing** (security path validation tests added)
- **Zero clippy warnings**
- **Path traversal protection** in include_loader.rs and context_loader.rs

### context: Field (NEW in v0.14.3)

Load files at workflow start, accessible via `{{context.files.alias}}` bindings:

```yaml
schema: nika/workflow@0.9
workflow: context-demo

context:
  files:
    brand: ./context/brand.md        # Markdown → string
    persona: ./context/persona.json  # JSON → parsed object
    examples: ./context/*.md         # Glob → array of strings
  session: .nika/sessions/prev.json  # Session restore

tasks:
  - id: generate
    infer: |
      Using brand guidelines: {{context.files.brand}}
      Generate content for our product.
```

**Implementation (`src/ast/context.rs` + `src/runtime/context_loader.rs`):**
- `ContextConfig` struct with files hashmap and optional session
- Automatic content type detection (markdown, json, yaml, glob)
- Path boundary validation prevents traversal attacks

### include: DAG Fusion (NEW in v0.14.3)

Merge tasks from external workflows into current DAG:

```yaml
schema: nika/workflow@0.9
workflow: main

include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_                    # Task ID prefix
  - path: ./partials/cleanup.nika.yaml
    prefix: cleanup_

tasks:
  - id: main_task
    infer: "Main workflow logic"

flows:
  - source: setup_init        # From included workflow
    target: main_task
  - source: main_task
    target: cleanup_finalize  # From included workflow
```

**Implementation (`src/ast/include.rs` + `src/ast/include_loader.rs`):**
- `IncludeSpec` struct with path and optional prefix
- Recursive include resolution with cycle detection
- Task ID prefixing for namespace isolation
- Path boundary validation prevents traversal attacks

### Path Traversal Security (NEW in v0.14.3)

Both include_loader.rs and context_loader.rs now validate paths:

```rust
fn validate_path_boundary(base_path: &Path, target_path: &Path) -> Result<(), NikaError> {
    let canonical_base = base_path.canonicalize()...;
    let canonical_target = target_path.canonicalize()...;

    if !canonical_target.starts_with(&canonical_base) {
        return Err(NikaError::ValidationError {
            reason: format!("Path traversal detected..."),
        });
    }
    Ok(())
}
```

- Prevents `../../../etc/passwd` style attacks
- Validates both single files and glob patterns
- Applies to session file paths as well

## v0.8.0 Changes (Studio DX Complete + Test Count Finalization)

### Statistics
- **4,369 tests passing** (v0.15.0 total - up from 2,997 in v0.12.0)
- **New modules:** `src/tui/edit_history.rs`, `src/tui/session.rs`, `src/tui/config.rs`
- **Studio view:** 5,400+ lines of code
- **Zero clippy warnings:** Full `-D warnings` compliance
- **Feature coverage:** 100% for v0.8 requirements

### Edit History (Undo/Redo) — NEW in v0.8.0
Real-time undo/redo for YAML editing in Studio view with intelligent coalescing:

| Action | Shortcut | Effect |
|--------|----------|--------|
| Undo | `Ctrl+Z` | Revert last edit |
| Redo | `Ctrl+Y` / `Ctrl+Shift+Z` | Restore undone edit |
| Clear history | Manual | Reset undo stack on file load |

**Technical Implementation (`src/tui/edit_history.rs`):**
- `EditHistory` struct with `Vec<String>` snapshots
- Intelligent coalescing: Groups rapid keystrokes within 500ms window
- Preserves user intent across multi-character edits
- Separate undo stack per open file
- 19 unit tests for edge cases, coalescing, and boundary conditions

**Key Features:**
- Undo stack depth: unlimited (memory-bounded per session)
- Redo support: Full history navigation
- Auto-clear: Stack resets on file reload
- Performance: O(1) undo/redo operations

### Session Persistence — NEW in v0.8.0
Auto-save editor state to `.nika/sessions/` with atomic writes and auto-cleanup:

```
.nika/sessions/
├── <session-id>.json  # Per-session state (max 50 sessions)
├── current_view.json  # Last opened view (chat/home/studio/monitor)
└── editor_metadata.json
```

**Session Data Structure:**
- `open_files`: List of workflow files with cursor positions
- `active_file`: Currently focused workflow
- `cursor_position`: Line and column in each file
- `scroll_offset`: Vertical scroll state
- `selection_state`: Highlight/selection range
- `timestamp`: Last modified timestamp

**Auto-recovery on startup:**
- Restores open files and cursor positions automatically
- Session survives app restart
- Incremental save during editing (500ms debounce)
- Atomic writes using temp + rename pattern (crash-safe)
- Auto-cleanup: Removes sessions older than 7 days
- Manual clear with `nika init --reset-sessions`

**Technical Implementation (`src/tui/session.rs`):**
- 13 unit tests for persistence, recovery, and cleanup
- `SessionManager` handles load/save lifecycle
- JSON serialization with serde for fast I/O
- Max 50 concurrent sessions (oldest auto-pruned)
- Error handling: Graceful degradation if .nika/sessions/ unavailable

### Solarized Theme — NEW in v0.8.0
Third theme option alongside Light and Dark with unified palette:

| Theme | Primary | Accent | Use Case |
|-------|---------|--------|----------|
| Light | #fdf6e3 | #268bd2 (blue) | Day mode (high contrast) |
| Dark | #002b36 | #268bd2 (blue) | Night mode (low strain) |
| Solarized | #fdf6e3 (light)/`#002b36` (dark) | #b58900 (warm) | WCAG AAA contrast, precision |

**Features:**
- Auto-detect based on system theme preference (macOS/Linux)
- Manual override via config or TUI settings
- Unified across all TUI views (Chat, Home, Studio, Monitor)
- Color palette based on Ethan Schoonover's Solarized project
- 12 unit tests for color correctness and contrast ratios

### Config System (.nika/config.toml) — NEW in v0.8.0
Persistent configuration for Nika with type-safe TOML serialization:

```toml
[editor]
theme = "solarized"           # light | dark | solarized
font_size = 12
auto_format = true            # Format YAML on save
indent_size = 2
line_numbers = true

[session]
auto_restore = true           # Restore editor state on startup
session_dir = ".nika/sessions"
max_sessions = 50             # Auto-cleanup when exceeded
session_ttl_days = 7          # Delete sessions older than N days

[providers]
default = "claude"            # Default LLM provider
timeout_secs = 30
auto_retry = true
max_retries = 3

[mcp]
auto_start_servers = true     # Auto-start MCP servers on workflow load
server_timeout_secs = 10
server_max_memory_mb = 512

[chat]
context_window = 4096         # Token limit for chat history
auto_save = true              # Auto-save chat sessions
```

**Auto-created on `nika init` with sensible defaults.**

**Technical Implementation (`src/tui/config.rs`):**
- 10 unit tests for parsing, validation, defaults
- `TuiSettings`, `ChatSettings`, `StudioSettings`, `PathSettings` structs
- Serde TOML serialization for human-readability
- Type-safe config with validation rules
- Backward compatibility: Old configs auto-upgraded

---

## v0.7.0 Changes (Full Streaming for All Providers)

All 6 providers now support **real-time streaming** in the TUI:

| Provider | Streaming | Token Tracking |
|----------|-----------|----------------|
| Claude | ✅ Full streaming + thinking capture | ✅ |
| OpenAI | ✅ Full streaming | ✅ |
| Mistral | ✅ Full streaming (v0.7) | ✅ |
| Groq | ✅ Full streaming (v0.7) | ✅ |
| DeepSeek | ✅ Full streaming (v0.7) | ✅ |
| Ollama | ✅ Full streaming (v0.7) | ✅ |

**Technical:** All providers use rig-core's `CompletionModel::stream()` API with `StreamedAssistantContent` for real-time token delivery.

**Zero TODOs remaining** — All streaming implementation is complete.

---

## v0.6.0 Changes (Multi-Provider + Chat History)

### 7 LLM Providers via rig-core (v0.15.0: +Gemini)

Nika now supports 7 providers natively via rig-core:

| Provider | Constructor | Env Var | Default Model |
|----------|-------------|---------|---------------|
| Claude | `RigProvider::claude()` | `ANTHROPIC_API_KEY` | claude-sonnet-4-6 |
| OpenAI | `RigProvider::openai()` | `OPENAI_API_KEY` | gpt-4o |
| Mistral | `RigProvider::mistral()` | `MISTRAL_API_KEY` | mistral-large-latest |
| Ollama | `RigProvider::ollama()` | `OLLAMA_API_BASE_URL` | llama3.2 |
| Groq | `RigProvider::groq()` | `GROQ_API_KEY` | llama-3.3-70b-versatile |
| DeepSeek | `RigProvider::deepseek()` | `DEEPSEEK_API_KEY` | deepseek-chat |
| **Gemini** | `RigProvider::gemini()` | `GEMINI_API_KEY` | gemini-2.0-flash |

**Auto-detection** (priority order):
```rust
// RigProvider::auto() checks env vars in order:
// 1. ANTHROPIC_API_KEY → Claude
// 2. OPENAI_API_KEY → OpenAI
// 3. MISTRAL_API_KEY → Mistral
// 4. GROQ_API_KEY → Groq
// 5. DEEPSEEK_API_KEY → DeepSeek
// 6. GEMINI_API_KEY → Gemini (v0.15.0)
// 7. OLLAMA_API_BASE_URL → Ollama (no key needed)

let provider = RigProvider::auto(); // Returns Option<RigProvider>
```

**Usage in RigAgentLoop:**
```rust
let mut agent = RigAgentLoop::new(task_id, params, log, mcp_clients)?;

// Auto-detect provider (recommended)
let result = agent.run_auto().await?;

// Or explicitly choose
let result = agent.run_mistral().await?;
let result = agent.run_ollama().await?;
let result = agent.run_groq().await?;
let result = agent.run_deepseek().await?;
let result = agent.run_gemini().await?;  // v0.15.0
```

### Chat History Support

Multi-turn conversations using rig's `Chat` trait:

```rust
use rig::message::Message;

let mut agent = RigAgentLoop::new(task_id, params, log, mcp_clients)?;

// First turn
let result1 = agent.run_claude().await?;

// History is automatically tracked
agent.add_to_history("What is Rust?", &result1.final_response);

// Continue conversation with context
let result2 = agent.chat_continue("Give me more examples").await?;

// Manual history management
agent.push_message(Message::user("Custom user message"));
agent.push_message(Message::assistant("Custom assistant response"));

// Initialize with existing history
let history = vec![
    Message::user("Previous question"),
    Message::assistant("Previous answer"),
];
let agent = RigAgentLoop::new(task_id, params, log, mcp)?
    .with_history(history);
```

**Chat History Methods:**
| Method | Description |
|--------|-------------|
| `add_to_history(user, assistant)` | Add user/assistant exchange |
| `push_message(msg)` | Add single message |
| `clear_history()` | Clear all history |
| `history_len()` | Get history length |
| `history()` | Get history slice |
| `with_history(vec)` | Builder pattern |
| `chat_continue(prompt)` | Continue with history |

## v0.5.3 Changes (MCP Stability)

### MCP Timeout Enforcement

All MCP operations now have timeout protection (30s default):

```rust
// Before (v0.5.2): Could hang indefinitely
let result = service.call_tool(request).await?;

// After (v0.5.3): Timeout after 30 seconds
let result = timeout(MCP_CALL_TIMEOUT, service.call_tool(request))
    .await
    .map_err(|_| NikaError::Timeout { ... })??;
```

**Affected operations:**
- `call_tool()` - MCP tool invocation
- `read_resource()` - MCP resource reading
- `list_tools()` - Tool discovery

### MCP Error Code Preservation

JSON-RPC error codes are now preserved from MCP servers:

```rust
pub enum McpErrorCode {
    ParseError,      // -32700
    InvalidRequest,  // -32600
    MethodNotFound,  // -32601
    InvalidParams,   // -32602
    InternalError,   // -32603
    ServerError(i32), // -32000 to -32099
    Unknown(i32),
}

// Error messages now include the code
// "[NIKA-102] MCP tool 'x' call failed (Invalid params (-32602)): ..."
```

**Usage:**
```rust
use nika::mcp::McpErrorCode;

let code = McpErrorCode::from_code(-32602);
assert!(code.is_client_error());  // InvalidParams is client-side
```

## Verb Shorthand Syntax (v0.5.1)

`infer:` and `exec:` support shorthand string syntax for simple cases:

```yaml
# Shorthand (v0.5.1+)
tasks:
  - id: generate
    infer: "Generate a headline for QR Code AI"

  - id: build
    exec: "npm run build"

# Full form (always supported)
tasks:
  - id: generate
    infer:
      prompt: "Generate a headline for QR Code AI"
      model: claude-sonnet-4-6

  - id: build
    exec:
      command: "npm run build"
```

| Verb | Shorthand | Full Form |
|------|-----------|-----------|
| `infer:` | `infer: "prompt"` | `infer: { prompt: "...", model: "..." }` |
| `exec:` | `exec: "command"` | `exec: { command: "..." }` |
| `fetch:` | ❌ No shorthand | `fetch: { url: "...", method: "..." }` |
| `invoke:` | ❌ No shorthand | `invoke: { tool: "...", server: "..." }` |
| `agent:` | ❌ No shorthand | `agent: { prompt: "...", mcp: [...] }` |

## rig-core Integration (v0.4+)

Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) v0.31 for all LLM providers.

| Component | Status | Implementation |
|-----------|--------|----------------|
| `agent:` verb | ✅ Done | `RigAgentLoop` uses rig's `AgentBuilder` |
| `infer:` verb | ✅ Done | `RigProvider.infer()` (6 providers) |
| MCP tools | ✅ Done | `NikaMcpTool` implements rig's `ToolDyn` |
| Chat history | ✅ v0.6 | `agent.chat(prompt, history)` via `Chat` trait |
| Multi-provider | ✅ v0.6 | Claude, OpenAI, Mistral, Ollama, Groq, DeepSeek |

### Using RigProvider (v0.6+)

```rust
use nika::provider::rig::RigProvider;

// Auto-detect provider from environment (recommended)
let provider = RigProvider::auto().expect("No API key found");

// Or explicitly choose
let provider = RigProvider::claude();    // ANTHROPIC_API_KEY
let provider = RigProvider::openai();    // OPENAI_API_KEY
let provider = RigProvider::mistral();   // MISTRAL_API_KEY
let provider = RigProvider::ollama();    // OLLAMA_API_BASE_URL
let provider = RigProvider::groq();      // GROQ_API_KEY
let provider = RigProvider::deepseek();  // DEEPSEEK_API_KEY

// Simple text completion via rig-core
let result = provider.infer("Summarize this text", None).await?;
```

### Using RigAgentLoop (Recommended for agent:)

```rust
use nika::runtime::RigAgentLoop;
use nika::ast::AgentParams;
use nika::event::EventLog;
use rig::message::Message;

let params = AgentParams {
    prompt: "Generate a landing page".to_string(),
    mcp: vec!["novanet".to_string()],
    max_turns: Some(5),
    ..Default::default()
};
let mut agent = RigAgentLoop::new("task-1".into(), params, EventLog::new(), mcp_clients)?;

// Production - auto-detects provider from env vars (checks 6 providers)
let result = agent.run_auto().await?;

// Or explicitly choose provider (6 available)
let result = agent.run_claude().await?;     // requires ANTHROPIC_API_KEY
let result = agent.run_openai().await?;     // requires OPENAI_API_KEY
let result = agent.run_mistral().await?;    // requires MISTRAL_API_KEY
let result = agent.run_ollama().await?;     // requires OLLAMA_API_BASE_URL
let result = agent.run_groq().await?;       // requires GROQ_API_KEY
let result = agent.run_deepseek().await?;   // requires DEEPSEEK_API_KEY
let result = agent.run_gemini().await?;     // requires GEMINI_API_KEY (v0.15.0)
let result = agent.run_mock().await?;       // for testing (no API key needed)

// Multi-turn with chat history (v0.6)
agent.add_to_history("First question", &result.final_response);
let result2 = agent.chat_continue("Follow-up question").await?;
```

## v0.4.1 Changes (Token Tracking Fix)

Token tracking in streaming mode (extended thinking) now works correctly:

| Before (v0.4.0) | After (v0.4.1) |
|-----------------|----------------|
| `input_tokens: 0` (always) | `input_tokens: <actual>` |
| `output_tokens: 0` (always) | `output_tokens: <actual>` |
| `total_tokens: 0` (always) | `total_tokens: <actual>` |

**Technical fix:** `run_claude_with_thinking()` now extracts token usage from `StreamedAssistantContent::Final` via rig's `GetTokenUsage` trait.

**Affected files:**
- `runtime/rig_agent_loop.rs` - Token extraction from streaming response
- `tests/thinking_capture_test.rs` - Integration tests for token capture

**AgentTurnMetadata** now contains accurate token counts when using extended thinking:

```rust
if let EventKind::AgentTurn { metadata: Some(metadata), .. } = event {
    println!("Input tokens: {}", metadata.input_tokens);   // Now > 0
    println!("Output tokens: {}", metadata.output_tokens); // Now > 0
    println!("Thinking: {:?}", metadata.thinking);         // Claude's reasoning
}
```

## v0.4 Changes (Removed Deprecated Code)

The following were **removed in v0.4**:

| Removed | Replacement | Notes |
|---------|-------------|-------|
| `ClaudeProvider` | `RigProvider::claude()` | Deleted `provider/claude.rs` |
| `OpenAIProvider` | `RigProvider::openai()` | Deleted `provider/openai.rs` |
| `provider::types` | `rig::completion::*` | Moved to minimal compat types in `mod.rs` |
| `AgentLoop` | `RigAgentLoop` | Deleted `runtime/agent_loop.rs` |
| `UseWiring` | `WiringSpec` | Alias removed |
| `from_use_wiring()` | `from_wiring_spec()` | Method removed |
| `resilience/` module | None | Entire module deleted (was never wired) |

## v0.5 MVP 8: RLM Enhancements

### Lazy Bindings

Defer binding resolution until first access:

```yaml
use:
  # Eager (default) - resolved immediately
  eager_val: task1.result

  # Lazy (v0.5) - resolved on access
  lazy_val:
    path: future_task.result
    lazy: true
    default: "fallback"
```

### Decompose Modifier

Runtime DAG expansion via MCP traversal:

```yaml
tasks:
  - id: expand_entities
    decompose:
      strategy: semantic    # semantic | static | nested
      traverse: HAS_CHILD   # Arc to follow
      source: $entity       # Starting node
      max_items: 10         # Optional limit
    infer: "Generate for {{use.item}}"
```

### Nested Agents (spawn_agent) ✅ IMPLEMENTED

Internal tool for recursive agent spawning with depth protection.
Implements `rig::ToolDyn` for seamless integration with `RigAgentLoop`.

**Usage in workflow:**
```yaml
tasks:
  - id: orchestrator
    agent:
      prompt: "Decompose and delegate sub-tasks"
      depth_limit: 3  # Prevents infinite recursion (default: 3, max: 10)
```

**spawn_agent tool parameters:**
```json
{
  "task_id": "subtask-1",      // Unique ID for child task
  "prompt": "Generate header", // Child agent goal
  "context": {"entity": "qr"}, // Optional context data
  "max_turns": 5               // Optional max turns (default: 10)
}
```

**Implementation:**
- `SpawnAgentTool` in `runtime/spawn.rs` (implements `rig::ToolDyn`)
- Automatically added to agents when `depth_limit > current_depth`
- Child agents inherit MCP clients from parent
- Emits `AgentSpawned` event for observability
- 13 unit tests + 4 ToolDyn integration tests

## for_each Parallelism (v0.3)

Parallel iteration over arrays with concurrency control.

### Configuration (Flat Format)

```yaml
tasks:
  - id: generate_pages
    for_each: ["fr-FR", "en-US", "de-DE"]  # Array or binding expression
    as: page                                # Loop variable name
    concurrency: 5                          # Max parallel tasks (default: 1)
    fail_fast: true                         # Stop on first error (default: true)
    infer: "Generate content for {{use.page}}"
    use.ctx: page_content
```

Binding expressions are also supported:
```yaml
    for_each: "{{use.items}}"  # Resolved at runtime
    for_each: "$items"         # Alternative binding syntax
```

### Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `for_each` | array/binding | required | Array or binding expression |
| `as` | string | "item" | Loop variable name |
| `concurrency` | integer | 1 | Max parallel executions |
| `fail_fast` | boolean | true | Stop all on first error |

### Implementation

Uses `tokio::spawn` with `JoinSet` for true concurrent execution:

```
concurrency=1:  [Task1] → [Task2] → [Task3]  (sequential)
concurrency=3:  [Task1]
                [Task2]  → (all in parallel)
                [Task3]
```

- Each iteration spawns as a separate tokio task
- `JoinSet` manages concurrent task lifecycle
- Results collected in original order
- `fail_fast: true` aborts remaining tasks on first error

## Benchmarks (v0.5.1)

Criterion benchmarks for performance testing:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench workflow_parsing
cargo bench --bench dag_validation
cargo bench --bench binding_resolution
cargo bench --bench task_execution
```

| Benchmark | Target | Measured |
|-----------|--------|----------|
| YAML parsing (1 task) | <10µs | ~4.6µs |
| YAML parsing (100 tasks) | <500µs | ~340µs |
| DAG validation (10 nodes) | <1µs | ~800ns |
| Binding resolution (3 entries) | <1µs | ~450ns |
| DataStore get | <10ns | ~6ns |

Benchmarks are in `benches/`:
- `workflow_parsing.rs` — YAML parsing, schema validation
- `dag_validation.rs` — Dag construction, cycle detection
- `binding_resolution.rs` — UseEntry parsing, lazy binding resolution
- `task_execution.rs` — DataStore operations, TaskResult creation

## TUI Enhancements (v0.5.1)

### Spinners

4 themed spinner types in `src/tui/widgets/spinner.rs`:

```rust
ROCKET_SPINNER: &[char] = &['🚀', '🔥', '✨', '💫', '⭐'];
STARS_SPINNER:  &[char] = &['✦', '✧', '★', '☆', '✵', '✶'];
ORBIT_SPINNER:  &[char] = &['◐', '◓', '◑', '◒'];
COSMIC_SPINNER: &[char] = &['🌑', '🌒', '🌓', '🌔', '🌕', '🌖', '🌗', '🌘'];
```

### Chat UX Widgets (v0.5.2)

- **SessionContextBar** — Token/cost/MCP status (full + compact modes)
- **McpCallBox** — Inline MCP call visualization with retry support
- **InferStreamBox** — Streaming LLM inference with progress bar
- **ActivityStack** — Hot/warm/queued task activity monitor
- **CommandPalette** — ⌘K fuzzy command search overlay
- **AgentTurns** — Agent turn history with verb icons

### Status Bar Enhancements

- Provider indicator: 🧠 Claude | 🤖 OpenAI | 🧪 Mock
- Token counter with smart formatting (K/M suffixes)
- MCP connection status

### DAG Visualization

Verb-specific icons (canonical):
- ⚡ `infer:` — LLM generation
- 📟 `exec:` — Shell command
- 🛰️ `fetch:` — HTTP request
- 🔌 `invoke:` — MCP tool
- 🐔 `agent:` — Agentic loop (parent)
- 🐤 subagent — Spawned via spawn_agent

## Commands (v0.5.2 CLI Refresh)

### Direct Execution

```bash
# Run workflow directly (simplest form)
nika workflow.nika.yaml

# Run with TUI observer (default, real-time execution)
nika tui workflow.nika.yaml

# Run without TUI (headless)
nika run workflow.nika.yaml
```

### Interactive Modes

```bash
# Home view — Browse and select .nika.yaml workflows
nika

# Chat view — Conversational agent with 5-verb support
nika chat

# Chat with specific provider (auto-detects from env by default)
nika chat --provider openai
nika chat --provider claude

# Studio view — YAML editor with live validation
nika studio

# Studio with file loaded
nika studio workflow.nika.yaml
```

### Workflow Management

```bash
# Validate syntax
nika check workflow.nika.yaml

# Strict validation (includes MCP connection check)
nika check flow.yaml --strict

# Initialize project (.nika/ directory with config)
nika init
```

### Traces & Observability

```bash
# List all execution traces
nika trace list

# Show trace details
nika trace show <id>

# Export trace (JSON/NDJSON)
nika trace export <id>

# Clean old traces
nika trace clean
```

### Provider Management (v0.12.1)

```bash
# List all LLM providers and their status
nika provider list

# Set API key for a provider (stores in system keychain)
nika provider set anthropic
nika provider set openai

# Test connection to a provider
nika provider test claude
nika provider test openai

# Migrate environment variables to system keychain
nika provider migrate
```

| Provider | Env Variable | Default Model |
|----------|--------------|---------------|
| Claude | `ANTHROPIC_API_KEY` | claude-sonnet-4-6 |
| OpenAI | `OPENAI_API_KEY` | gpt-4o |
| Mistral | `MISTRAL_API_KEY` | mistral-large-latest |
| Groq | `GROQ_API_KEY` | llama-3.3-70b-versatile |
| DeepSeek | `DEEPSEEK_API_KEY` | deepseek-chat |
| Ollama | `OLLAMA_API_BASE_URL` | llama3.2 |

### MCP Server Management (v0.12.1)

```bash
# List MCP servers defined in a workflow
nika mcp list --workflow flow.nika.yaml

# Test connection to an MCP server
nika mcp test flow.nika.yaml perplexity

# List tools available from an MCP server
nika mcp tools flow.nika.yaml perplexity
```

MCP servers are defined in workflow files:

```yaml
mcp:
  servers:
    perplexity:
      command: npx
      args: ["-y", "@anthropic/mcp-server-perplexity"]
      env:
        PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"
```

### Development & Testing

```bash
# Run tests (via cargo)
cargo nextest run

# Run with coverage
cargo llvm-cov nextest

# Run benchmarks
cargo bench
```

### TUI Views (Tab Navigation)

The TUI provides 3 interactive views:

| View | Key | Purpose |
|------|-----|---------|
| **Chat** | `a` | Conversational agent (supports infer:, exec:, fetch:, invoke:, agent:) |
| **Home** | `h` | Browse and launch .nika.yaml workflows from project |
| **Studio** | `s` | YAML editor with schema validation and syntax highlighting |

## Testing Strategy

- **Unit tests:** In-file `#[cfg(test)]` modules (1641 tests)
- **Integration tests:** `tests/` directory
- **Snapshot tests:** insta for YAML/JSON outputs
- **Property tests:** proptest for parser fuzzing
- **Real API tests:** `examples/test-*.nika.yaml` (require API keys)

### Real API Testing

Test workflows with live API calls:

```bash
# Set API keys
export ANTHROPIC_API_KEY=sk-ant-...
export PERPLEXITY_API_KEY=pplx-...

# Run real API tests
cargo run -- run examples/test-parallel-stress.nika.yaml
cargo run -- run examples/test-multi-mcp-agent.nika.yaml
cargo run -- run examples/test-context-propagation.nika.yaml
```

| Test | Features Validated |
|------|-------------------|
| `test-parallel-stress.nika.yaml` | 5 concurrent Claude API calls with `for_each` |
| `test-multi-mcp-agent.nika.yaml` | Agent with MCP tools, spawn_agent, stop_conditions |
| `test-deep-context-chain.nika.yaml` | 6-level context propagation with `use:` bindings |
| `test-agent-stop-conditions.nika.yaml` | Agent stop condition triggering |
| `test-perplexity-mcp.nika.yaml` | External MCP server integration |

## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Task errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Binding errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |

## Conventions

- **Imports:** Group by std, external, internal
- **Error handling:** Use `NikaError` with codes, not `anyhow`
- **Logging:** Use `tracing` macros (debug!, info!, warn!, error!)
- **Tests:** TDD - write failing test first
- **Commits:** Conventional commits with scope
