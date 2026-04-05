# Plan: Direct Verb Execution + CLI Coherence (v0.46.0)

> Run any Nika verb directly from the terminal. No YAML needed.
> Redesign help text. Make all 26 commands coherent.

## Vision

```bash
# LLM in one line — provider auto-detected from API keys
nika infer "Explain quantum computing in 3 sentences"

# Pipe-friendly — cat | nika infer works like jq/curl
cat article.txt | nika infer "Summarize this in 5 bullet points"

# Structured output — from_example, no schema needed
nika infer "Extract people" --from-example '{"names":[""], "ages":[0]}'

# Fetch + extract — 9 extraction modes
nika fetch https://blog.com --extract article
nika fetch https://api.github.com/repos/x/y --extract jsonpath --selector ".stargazers_count"

# Builtin tools — 24 nika:* tools directly from CLI
nika invoke nika:dimensions photo.jpg
nika invoke nika:thumbnail photo.jpg --params '{"width":200}'

# MCP tools — call any configured server tool
nika invoke novanet::search --params '{"query":"AI workflows"}'

# Multi-turn agent with tools
nika agent "Research Rust async patterns" --tool web_search --turns 5

# Everything composes
curl -s https://api.x.com/data | nika infer "Analyze this JSON" --json
nika fetch https://news.com --extract article | nika infer "TL;DR" -m gpt-4o
```

## Command Taxonomy (after v0.46.0)

```
VERBS (execute one-shot)          WORKFLOWS (orchestrate)
├── nika infer   [i]              ├── nika run     [r]
├── nika fetch   [f]              ├── nika check   [v]
├── nika invoke                   ├── nika new     [n]
└── nika agent   [a]              └── nika workflow [w]

CONFIG (manage resources)          INTERACTIVE (use nika)
├── nika provider                 ├── nika ui / chat / studio
├── nika model   [m]              ├── nika course  [learn]
├── nika mcp                      ├── nika doctor  [d]
├── nika config                   ├── nika trace
├── nika media                    ├── nika showcase
└── nika pkg     [p]              └── nika completion
```

## Key Design Decisions

### invoke vs mcp — they are DIFFERENT

| Command | Purpose | Example |
|---------|---------|---------|
| `nika invoke` | **EXECUTE** a tool (builtin or MCP) | `nika invoke nika:thumbnail img.jpg` |
| `nika mcp` | **CONFIGURE** MCP servers | `nika mcp add novanet --global` |

`invoke` calls tools. `mcp` manages server configs. Never confused.

### Provider auto-detection priority

1. `-p` flag (explicit override)
2. `.nika/config.toml` `default_provider` (project default)
3. Keychain (via `nika keys set`)
4. Environment variables (`ANTHROPIC_API_KEY`, etc.)
5. Error: "No provider found. Run `nika keys set <name>` or set API key env var"

### TTY-aware output

| Context | Behavior |
|---------|----------|
| Terminal (TTY) | Spinner + colors + cost footer |
| Pipe (`\| jq`) | Raw output only (no ANSI, no footer) |
| `--json` flag | Force JSON output regardless of TTY |
| `--quiet` flag | Suppress all non-output text |

---

## Task 1: Commands enum — 4 new verb subcommands

**File:** `nika/src/main.rs`

Add after `Run` and before `Check`:

```rust
/// Call an LLM directly (no workflow needed)
#[command(visible_alias = "i")]
Infer {
    /// Prompt text (use "-" to read from stdin)
    prompt: String,

    /// Provider (auto-detected if omitted)
    #[arg(short, long)]
    provider: Option<String>,

    /// Model name
    #[arg(short, long)]
    model: Option<String>,

    /// System prompt
    #[arg(short, long)]
    system: Option<String>,

    /// Temperature (0.0 - 2.0)
    #[arg(short, long)]
    temperature: Option<f64>,

    /// Max output tokens
    #[arg(long)]
    max_tokens: Option<u32>,

    /// Force JSON output format
    #[arg(long)]
    json: bool,

    /// Structured output from example (inline JSON or file path)
    #[arg(long, value_name = "EXAMPLE")]
    from_example: Option<String>,

    /// Read additional context from stdin (prepended to prompt)
    #[arg(long)]
    stdin: bool,
},

/// Fetch a URL with smart extraction (9 modes)
#[command(visible_alias = "f")]
Fetch {
    /// URL to fetch
    url: String,

    /// Extraction mode
    #[arg(short, long, value_parser = ["markdown", "article", "text",
        "selector", "metadata", "links", "jsonpath", "feed", "llm_txt"])]
    extract: Option<String>,

    /// CSS selector or JSONPath expression
    #[arg(long)]
    selector: Option<String>,

    /// HTTP method (default: GET)
    #[arg(short = 'X', long)]
    method: Option<String>,

    /// HTTP header (repeatable): -H "Key: Value"
    #[arg(short = 'H', long = "header", value_name = "KEY:VALUE")]
    headers: Vec<String>,

    /// Request body
    #[arg(long)]
    body: Option<String>,

    /// JSON body (auto Content-Type)
    #[arg(long, value_name = "JSON")]
    json_body: Option<String>,

    /// Response mode: full (with headers) or binary (CAS hash)
    #[arg(long, value_parser = ["full", "binary"])]
    response: Option<String>,

    /// Timeout in seconds (default: 30)
    #[arg(long)]
    timeout: Option<u64>,
},

/// Call a builtin nika:* tool or MCP server tool
Invoke {
    /// Tool name: nika:thumbnail, nika:dimensions, server::tool_name
    tool: String,

    /// Positional file argument (shortcut for common tools)
    #[arg(value_name = "FILE")]
    file: Option<String>,

    /// Tool parameters as JSON
    #[arg(long, value_name = "JSON")]
    params: Option<String>,

    /// MCP server name (required for non-builtin tools)
    #[arg(long)]
    mcp: Option<String>,

    /// Timeout in seconds
    #[arg(long)]
    timeout: Option<u64>,
},

/// Run a multi-turn AI agent with tools
#[command(visible_alias = "a")]
Agent {
    /// Agent objective / prompt
    prompt: String,

    /// Provider
    #[arg(short, long)]
    provider: Option<String>,

    /// Model
    #[arg(short, long)]
    model: Option<String>,

    /// System prompt
    #[arg(short, long)]
    system: Option<String>,

    /// Available tool (repeatable)
    #[arg(short, long = "tool")]
    tools: Vec<String>,

    /// MCP server to connect (repeatable)
    #[arg(long = "mcp")]
    mcp_servers: Vec<String>,

    /// Max conversation turns (default: 10)
    #[arg(long, default_value = "10")]
    turns: u32,

    /// Max tokens per turn
    #[arg(long)]
    max_tokens: Option<u32>,

    /// Temperature
    #[arg(short, long)]
    temperature: Option<f64>,

    /// Read context from stdin
    #[arg(long)]
    stdin: bool,
},
```

---

## Task 2: Verb handlers — `nika-cli/src/verbs.rs` (~350 LOC)

### Shared infrastructure

```rust
use std::io::{IsTerminal, Read};
use std::sync::Arc;
use std::time::Instant;
use colored::Colorize;

/// Auto-detect provider from config, keychain, or env vars.
fn detect_provider() -> Option<String> {
    // 1. Check config.toml default_provider
    // 2. Check env vars in priority order
    for (var, provider) in [
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("OPENAI_API_KEY", "openai"),
        ("MISTRAL_API_KEY", "mistral"),
        ("GROQ_API_KEY", "groq"),
        ("DEEPSEEK_API_KEY", "deepseek"),
        ("GEMINI_API_KEY", "gemini"),
        ("XAI_API_KEY", "xai"),
    ] {
        if std::env::var(var).is_ok() {
            return Some(provider.to_string());
        }
    }
    None
}

/// Read stdin content (for --stdin flag or prompt="-").
fn read_stdin() -> Result<String, NikaError> { ... }

/// Create a one-shot TaskExecutor with minimal setup.
async fn one_shot_executor(
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<(TaskExecutor, EventLog), NikaError> { ... }

/// Display cost footer (TTY only).
fn print_cost_footer(events: &[Event], elapsed: Duration) { ... }
```

### `handle_infer()`

Core flow:
1. Read stdin if `--stdin` or `prompt == "-"`
2. Build `InferParams` from CLI flags
3. Build `OutputPolicy` if `--from-example` or `--json`
4. Create executor → `execute()` → get output string
5. Print output (raw if piped, colored if TTY)
6. Print cost footer (TTY only)

### `handle_fetch()`

Core flow:
1. Parse `-H` headers into map
2. Build `FetchParams` from CLI flags
3. Create executor → `execute()` → get output
4. Print output

Note: `fetch` does NOT need a provider. Use "mock" as provider for executor
(it only uses the HTTP client, not the LLM).

### `handle_invoke()`

Core flow:
1. Detect if builtin (`nika:*`) or MCP (`server::tool`)
2. For builtin: merge `file` positional arg into params if present
3. Build `InvokeParams`
4. Create executor → `execute()`
5. Print output

Smart file argument:
```bash
nika invoke nika:dimensions photo.jpg
# equivalent to:
nika invoke nika:dimensions --params '{"source":"photo.jpg"}'
```

For MCP tools: require `--mcp` flag or `server::tool` syntax.

### `handle_agent()`

Core flow:
1. Read stdin if `--stdin`
2. Build `AgentParams` with tools + MCP servers
3. Create executor → `execute()`
4. Print output with cost footer

---

## Task 3: Wire match arms + update other callers

Wire the 4 new commands in the main match. Update `run_workflow()` callers
that changed signature in v0.44-v0.45 to stay consistent.

---

## Task 4: Redesign AFTER_HELP text

Complete rewrite organized by taxonomy:

```
DIRECT VERBS (no YAML needed):
    nika infer "Explain X"                Call LLM
    nika infer "Summarize" --stdin        Pipe content to LLM
    nika infer "Extract" --from-example '{"names":[""]}' Structured output
    nika fetch URL --extract markdown     Fetch + extract
    nika fetch URL --extract jsonpath --selector ".data"
    nika invoke nika:dimensions img.jpg   Call builtin tool
    nika invoke nika:thumbnail img.jpg --params '{"width":200}'
    nika agent "Research X" --turns 5     Multi-turn agent

    Pipes: cat f.txt | nika infer "Summarize"
           nika fetch URL --extract article | nika infer "TL;DR"

WORKFLOW EXECUTION:
    nika run [file]                       Run workflow (auto-discover if omitted)
    nika run file -i key=value            Override inputs
    nika run file --dry-run               Preview execution plan
    nika run file --task step2            Run single task + deps
    nika run file --from step3            Run from task onwards
    nika run file -o results.json         Capture outputs

WORKFLOW MANAGEMENT:
    nika check <file> [--strict]          Validate syntax + DAG
    nika new <name> [--verb infer]        Create workflow scaffold
    nika workflow graph <file>            Show DAG visualization

INTERACTIVE:
    nika ui                               TUI (Studio view)
    nika chat                             TUI Chat (shortcut)
    nika studio [file]                    TUI Studio (shortcut)
    nika course                           Interactive learning

CONFIGURATION:
    nika provider list|set|test           Manage API keys
    nika model list|pull|delete           Manage local GGUF models
    nika mcp add|remove|list|test|tools   Manage MCP servers
    nika config list|get|set|edit         Settings

DIAGNOSTICS:
    nika doctor [--fix]                   System health check
    nika trace list|show                  Execution traces
    nika media list|stats|tools|clean     CAS media store

GLOBAL FLAGS:
    -v, -vv, -vvv                         Verbosity levels
    -q, --quiet                           Suppress non-output text
    --color auto|always|never             Color control
    --detail max|default|min|json         Output detail level

BUILTIN TOOLS (nika:*):
    nika:import nika:dimensions nika:thumbhash nika:dominant_color nika:pipeline
    nika:thumbnail nika:convert nika:strip nika:metadata nika:optimize nika:svg_render
    nika:phash nika:compare nika:pdf_extract nika:chart nika:provenance nika:verify
    nika:qr_validate nika:quality nika:html_to_md nika:css_select
    nika:extract_metadata nika:extract_links nika:readability

    Use: nika invoke nika:<tool> [file] [--params JSON]
    List: nika media tools
```

---

## Task 5: Visual output design

### Infer output (TTY)

```
  ┌─ claude-sonnet-4-20250514 via anthropic ─────────────────────────┐

  Quantum computing uses qubits that can exist in superposition,
  allowing them to process multiple possibilities simultaneously.
  This enables exponential speedup for certain problems like
  factoring large numbers and simulating molecular behavior.

  └─────────────────────────────── 1.2s · 127 tokens · $0.0019 ─┘
```

### Infer output (piped)

```
Quantum computing uses qubits that can exist in superposition...
```

### Fetch output (TTY)

```
  ┌─ GET https://blog.com ──── 200 OK ── article ───────────────────┐

  # Article Title
  By Author Name · March 25, 2026

  Article content here...

  └──────────────────────────────── 0.8s · 4,231 bytes ──────────┘
```

### Invoke output (TTY)

```
  ┌─ nika:dimensions ───────────────────────────────────────────────┐

  {"width": 1920, "height": 1080, "format": "jpeg"}

  └──────────────────────────────────────────── 0.1s ────────────┘
```

### Error output (all modes)

```
  ✗ Provider error: ANTHROPIC_API_KEY not set

  Fix: nika keys set anthropic
       or: export ANTHROPIC_API_KEY=sk-ant-...
```

---

## Task 6: E2E verification plan

### Infer E2E

```bash
# Basic inference (mock provider)
nika infer "Say hello" -p mock
# Expected: some mock response, no error

# Stdin pipe
echo "Hello World" | nika infer "Uppercase this" -p mock --stdin
# Expected: mock response, no stdin read error

# Prompt from stdin
echo "What is 2+2?" | nika infer - -p mock
# Expected: mock response using stdin as prompt

# JSON mode
nika infer "Return a JSON object" -p mock --json
# Expected: response (mock doesn't enforce JSON, but flag parsed)

# Structured output
nika infer "Extract" -p mock --from-example '{"name":"","age":0}'
# Expected: structured output with schema validation

# Provider auto-detect (needs real API key)
nika infer "Say OK" 2>&1
# Expected: either response or "No provider found" error

# Error: no provider
ANTHROPIC_API_KEY= OPENAI_API_KEY= nika infer "test" 2>&1
# Expected: error with fix suggestion
```

### Fetch E2E

```bash
# Basic fetch
nika fetch https://example.com
# Expected: HTML content

# Extract markdown
nika fetch https://example.com --extract markdown
# Expected: clean markdown

# Extract metadata
nika fetch https://github.com --extract metadata
# Expected: JSON with OG tags

# POST with JSON body
nika fetch https://httpbin.org/post -X POST --json-body '{"test":true}'
# Expected: echo response with Content-Type: application/json

# Custom headers
nika fetch https://httpbin.org/headers -H "X-Custom: test"
# Expected: custom header in response

# Invalid URL
nika fetch not-a-url 2>&1
# Expected: clear error message
```

### Invoke E2E

```bash
# Builtin tool with file arg
nika invoke nika:dimensions test-image.jpg
# Expected: {"width":..., "height":...}

# Builtin tool with params
nika invoke nika:thumbhash --params '{"source":"test-image.jpg"}'
# Expected: base64 thumbhash

# List available tools
nika media tools
# Expected: formatted table of 24 tools

# Unknown tool error
nika invoke nika:nonexistent 2>&1
# Expected: "Unknown tool 'nika:nonexistent'"

# MCP tool without server
nika invoke unknown::tool 2>&1
# Expected: "MCP server 'unknown' not configured"
```

### Agent E2E

```bash
# Basic agent (mock)
nika agent "Say hello" -p mock --turns 1
# Expected: single-turn response

# With tools
nika agent "List files" -p mock --tool read_file --turns 2
# Expected: agent attempt (mock provider may not support tools)
```

### Help E2E

```bash
# Main help shows new verbs
nika --help 2>&1 | grep -E "infer|fetch|invoke|agent"
# Expected: all 4 verbs listed

# Individual verb help
nika infer --help 2>&1 | grep -E "stdin|from-example|json|provider"
# Expected: all flags documented

nika fetch --help 2>&1 | grep -E "extract|selector|header"
# Expected: all flags documented

# Aliases work
nika i "test" -p mock 2>&1
nika f https://example.com 2>&1
nika a "test" -p mock --turns 1 2>&1
```

---

## Task 7: Gate workflows

```yaml
# nika/examples/gates/feature/cli-direct-infer.nika.yaml
schema: "nika/workflow@0.12"
workflow: test-cli-infer
provider: mock
model: mock
tasks:
  - id: test_infer
    description: "Verify nika infer command works"
    exec: |
      output=$(nika infer "Say hello" -p mock 2>&1)
      if [ $? -eq 0 ]; then echo "PASS"; else echo "FAIL: $output"; fi
```

---

## Task 8: Version bump + CHANGELOG + release

Bump to v0.46.0. Create GitHub release with ASCII art.

CHANGELOG entry:

```markdown
## [0.46.0] — 2026-03-25

### Added
- **Direct verb execution** — run LLM, fetch, tools, agents without YAML
  - `nika infer "prompt"` — one-shot LLM call
  - `nika fetch URL --extract article` — smart HTTP fetch
  - `nika invoke nika:thumbnail img.jpg` — builtin tool call
  - `nika agent "objective" --tool web_search` — multi-turn agent
- **Provider auto-detection** — scans API keys, no `-p` needed
- **Pipe-friendly** — `cat file | nika infer "Summarize"`
- **Structured output** — `nika infer --from-example '{"names":[]}'`
- **Cost footer** — shows tokens + cost after LLM calls (TTY only)
- **Redesigned help** — organized by taxonomy (verbs/workflows/config/interactive)
```

---

## Ordering

| Task | Priority | LOC | Description |
|------|----------|-----|-------------|
| 1 | HIGH | ~100 | Commands enum (4 variants + flags) |
| 2 | HIGH | ~350 | Verb handlers (verbs.rs) |
| 3 | HIGH | ~30 | Match arm wiring |
| 4 | MEDIUM | ~30 | Provider auto-detection |
| 5 | MEDIUM | ~80 | AFTER_HELP redesign |
| 6 | MEDIUM | ~60 | Visual output (border box + cost footer) |
| 7 | LOW | ~40 | E2E tests + gate workflow |
| 8 | LOW | ~10 | Version bump + release |
| **Total** | | **~700** | |

---

## Non-Goals (V2)

- Streaming token-by-token output (requires direct provider access, not execute())
- Vision from CLI (`nika infer --image photo.jpg "Describe"`)
- MCP server auto-start for `invoke` non-builtin tools
- Agent with file edit tools (security audit needed)
- `nika exec` command (bash already exists)
- Tab completion for tool names in `nika invoke`
