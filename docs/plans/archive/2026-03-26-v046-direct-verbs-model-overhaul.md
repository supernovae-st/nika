# Plan: v0.46.0 — Direct Verbs + Model Overhaul + CLI Polish

> The "nika as a superpower" release.
> Run verbs from CLI. See all models + pricing. Smart auto-detection.

## Vision

```bash
# ═══════════════════════════════════════════════════════════════
# DIRECT VERBS — no YAML needed
# ═══════════════════════════════════════════════════════════════

nika infer "Explain quantum computing"
cat article.txt | nika infer "Summarize this"
nika infer "Extract" --from-example '{"names":[""]}'
nika fetch https://blog.com --extract article
nika invoke nika:dimensions photo.jpg
nika agent "Research AI" --tool web_search --turns 5

# Compose with pipes
nika fetch https://news.com --extract article | nika infer "TL;DR"

# ═══════════════════════════════════════════════════════════════
# MODEL DISCOVERY — see everything, with pricing
# ═══════════════════════════════════════════════════════════════

nika model list                    # ALL models (cloud + local + pricing)
nika model list --provider openai  # Filter by provider
nika model info claude-sonnet-4-6  # Details + pricing + context window
nika model recommend               # Smart suggestion based on your keys

# ═══════════════════════════════════════════════════════════════
# COMPOSITE MODEL IDENTIFIER — one flag, not two
# ═══════════════════════════════════════════════════════════════

nika infer "hello" -m anthropic/claude-sonnet-4-6
nika infer "hello" -m openai/gpt-4o-mini
nika infer "hello" -m claude-sonnet   # auto-detect provider
```

---

## Architecture

### Command Taxonomy (26 → 30 commands)

```
VERBS (one-shot execution)         WORKFLOWS (orchestrate)
├── nika infer   [i]  NEW          ├── nika run     [r]
├── nika fetch   [f]  NEW          ├── nika check   [v]
├── nika invoke       NEW          ├── nika new     [n]
└── nika agent   [a]  NEW          └── nika workflow [w]

CONFIG (manage resources)           INTERACTIVE (use nika)
├── nika provider                  ├── nika ui / chat / studio
├── nika model   OVERHAULED        ├── nika course  [learn]
├── nika mcp                       ├── nika doctor  [d]
├── nika config                    ├── nika trace
├── nika media                     ├── nika showcase
└── nika pkg     [p]               └── nika completion
```

### Key Design Decisions

**1. invoke vs mcp — EXECUTE vs CONFIGURE**

| Command | Purpose | Example |
|---------|---------|---------|
| `nika invoke` | **EXECUTE** a tool | `nika invoke nika:thumbnail img.jpg` |
| `nika mcp` | **CONFIGURE** servers | `nika mcp add novanet --global` |

**2. provider vs model — CREDENTIALS vs DISCOVERY**

| Command | Purpose | Example |
|---------|---------|---------|
| `nika provider` | "Do I have access?" | `nika provider list` → ✓/✗ per provider |
| `nika model` | "What can I use?" | `nika model list` → all models + pricing |

Like `gh auth status` vs "what repos can I access?"

**3. Composite model identifier (LiteLLM pattern)**

```
-m anthropic/claude-sonnet-4-6   → explicit provider + model
-m claude-sonnet-4-6             → auto-detect provider from model name
-m gpt-4o                       → auto-detect → openai
```

One flag. Zero ambiguity. Parser: split on first `/`. If no `/`, scan all
provider pricing tables for the model name.

**4. model list OUT of native-inference feature gate**

Critical bug: `nika model` is `#[cfg(feature = "native-inference")]`.
Cloud-only users (99%) can't access ANY model command.

Fix: split into always-available (list/info/recommend) and gated (pull/delete/vision/status).

**5. TTY-aware output**

| Context | Behavior |
|---------|----------|
| Terminal (TTY) | Border box + colors + cost footer |
| Pipe (`\| jq`) | Raw output only (no ANSI, no footer) |
| `--json` flag | Force JSON regardless of TTY |

**6. Provider auto-detection priority**

1. `-m provider/model` or `-p` flag (explicit)
2. `.nika/config.toml` `default_provider` + `default_model`
3. Keychain (via `nika keys set`)
4. Environment variables (scan in order: anthropic, openai, ...)
5. Error with actionable fix suggestion

---

## SECTION A: Direct Verbs (Tasks 1-4)

### Task 1: Commands enum — 4 new subcommands

**File:** `nika/src/main.rs`

```rust
/// Call an LLM directly (no workflow needed)
#[command(visible_alias = "i")]
Infer {
    /// Prompt (use "-" for stdin)
    prompt: String,
    #[arg(short, long)]
    provider: Option<String>,
    /// Model (supports provider/model syntax: anthropic/claude-sonnet-4-6)
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long)]
    system: Option<String>,
    #[arg(short, long)]
    temperature: Option<f64>,
    #[arg(long)]
    max_tokens: Option<u32>,
    #[arg(long)]
    json: bool,
    #[arg(long, value_name = "EXAMPLE")]
    from_example: Option<String>,
    #[arg(long)]
    stdin: bool,
},

/// Fetch a URL with smart extraction
#[command(visible_alias = "f")]
Fetch {
    url: String,
    #[arg(short, long, value_parser = ["markdown", "article", "text",
        "selector", "metadata", "links", "jsonpath", "feed", "llm_txt"])]
    extract: Option<String>,
    #[arg(long)]
    selector: Option<String>,
    #[arg(short = 'X', long)]
    method: Option<String>,
    #[arg(short = 'H', long = "header", value_name = "KEY:VALUE")]
    headers: Vec<String>,
    #[arg(long)]
    body: Option<String>,
    #[arg(long, value_name = "JSON")]
    json_body: Option<String>,
    #[arg(long, value_parser = ["full", "binary"])]
    response: Option<String>,
    #[arg(long)]
    timeout: Option<u64>,
},

/// Call a builtin nika:* tool or MCP tool
Invoke {
    /// Tool: nika:thumbnail, nika:dimensions, server::tool_name
    tool: String,
    /// File argument (auto-mapped to "source" param)
    #[arg(value_name = "FILE")]
    file: Option<String>,
    #[arg(long, value_name = "JSON")]
    params: Option<String>,
    #[arg(long)]
    mcp: Option<String>,
    #[arg(long)]
    timeout: Option<u64>,
    /// List available builtin tools
    #[arg(long)]
    list: bool,
},

/// Multi-turn AI agent with tools
#[command(visible_alias = "a")]
Agent {
    prompt: String,
    #[arg(short, long)]
    provider: Option<String>,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long)]
    system: Option<String>,
    #[arg(short, long = "tool")]
    tools: Vec<String>,
    #[arg(long = "mcp")]
    mcp_servers: Vec<String>,
    #[arg(long, default_value = "10")]
    turns: u32,
    #[arg(long)]
    max_tokens: Option<u32>,
    #[arg(short, long)]
    temperature: Option<f64>,
    #[arg(long)]
    stdin: bool,
},
```

### Task 2: Verb handlers — `nika-cli/src/verbs.rs` (~350 LOC)

**New file** with 4 handlers + shared infrastructure:

- `detect_provider()` — config → keychain → env → error
- `parse_composite_model("anthropic/claude-sonnet")` → `(Some("anthropic"), "claude-sonnet")`
- `resolve_provider_for_model("gpt-4o")` — scan pricing tables → "openai"
- `read_stdin()` — for `--stdin` or `prompt == "-"`
- `one_shot_executor()` — create TaskExecutor for single use
- `print_verb_output()` — TTY-aware border box + cost footer
- `handle_infer()`, `handle_fetch()`, `handle_invoke()`, `handle_agent()`

### Task 3: Wire match arms

4 new arms in the main match + update other `run_workflow()` callers.

### Task 4: Composite model identifier parser

```rust
/// Parse "anthropic/claude-sonnet-4-6" → (Some("anthropic"), "claude-sonnet-4-6")
/// Parse "claude-sonnet-4-6" → (None, "claude-sonnet-4-6")
fn parse_composite_model(model: &str) -> (Option<&str>, &str) {
    match model.split_once('/') {
        Some((provider, model_name)) => (Some(provider), model_name),
        None => (None, model),
    }
}

/// Auto-detect provider from model name by scanning pricing tables.
fn resolve_provider_for_model(model: &str) -> Option<&'static str> {
    // Check each provider's pricing table for the model
    if CLAUDE_PRICING.contains_key(model) { return Some("anthropic"); }
    if OPENAI_PRICING.contains_key(model) { return Some("openai"); }
    if MISTRAL_PRICING.contains_key(model) { return Some("mistral"); }
    // ... etc for all providers
    None
}
```

---

## SECTION B: Model Overhaul (Tasks 5-8)

### Task 5: Move model list/info/recommend OUT of feature gate

**File:** `nika/src/main.rs`

Currently ALL model subcommands are behind `#[cfg(feature = "native-inference")]`.
Split the enum:

```rust
// ALWAYS available (cloud + local listing)
Model {
    #[command(subcommand)]
    action: ModelAction,
}

// ModelAction in nika-cli/src/model.rs:
pub enum ModelAction {
    /// List all available models (cloud + local) with pricing
    List {
        #[arg(long)]
        cloud: bool,        // cloud only
        #[arg(long)]
        local: bool,        // local GGUF only
        #[arg(short, long)]
        provider: Option<String>,  // filter by provider
        #[arg(long)]
        json: bool,
    },

    /// Show model details + pricing
    Info {
        name: String,
    },

    /// Smart model recommendation based on available API keys
    Recommend,

    // --- Below: gated behind native-inference ---

    /// Download a GGUF model from HuggingFace
    #[cfg(feature = "native-inference")]
    Pull { ... },

    /// Show loaded model status
    #[cfg(feature = "native-inference")]
    Status,

    /// Delete a downloaded model
    #[cfg(feature = "native-inference")]
    Delete { ... },

    /// Load HuggingFace vision model
    #[cfg(feature = "native-inference")]
    Vision { ... },
}
```

### Task 6: Enriched `nika model list` with cloud pricing

**File:** `nika-cli/src/model.rs` — rewrite `handle_list()`

Output design:

```
╭──────────────────────────────────────────────────────────────────────╮
│  Available Models                          input / output per M tok  │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ANTHROPIC (✓ key set)                                               │
│  ├── claude-opus-4-6             $15.00 / $75.00                    │
│  ├── claude-sonnet-4-6           $3.00 / $15.00   ★ default         │
│  ├── claude-sonnet-4-20250514    $3.00 / $15.00                     │
│  └── claude-haiku-4-5            $0.80 / $4.00                      │
│                                                                      │
│  OPENAI (✓ key set)                                                  │
│  ├── gpt-4o                      $2.50 / $10.00                     │
│  ├── gpt-4o-mini                $0.15 / $0.60                       │
│  ├── gpt-4.1                    $2.00 / $8.00                       │
│  ├── gpt-4.1-mini              $0.40 / $1.60                       │
│  └── o3                         $10.00 / $40.00                     │
│                                                                      │
│  MISTRAL (✗ no key → nika keys set mistral)                      │
│  GROQ (✗ no key)                                                     │
│  DEEPSEEK (✗ no key)                                                 │
│  GEMINI (✗ no key)                                                   │
│  XAI (✗ no key)                                                      │
│                                                                      │
│  LOCAL (native inference)                                            │
│  ├── qwen3:8b      Q4_K_M  5.2GB  [✓ downloaded]                   │
│  └── llama3:8b     Q4_K_M  4.8GB  [nika model pull]                │
│                                                                      │
│  Tip: nika infer "..." -m claude-sonnet-4-6                          │
│  Default: nika config set default_model claude-sonnet-4-6            │
│                                                                      │
╰──────────────────────────────────────────────────────────────────────╯
```

**Data sources (all exist in codebase):**
- `nika-engine/src/provider/cost.rs` — pricing per model per provider
- `nika-core/src/catalogs/providers.rs` — provider metadata + env var names
- `nika-core/src/catalogs/models.rs` — GGUF model catalog

### Task 7: `nika model info <model>` for cloud models

Currently only works for local GGUF. Extend to cloud:

```bash
$ nika model info claude-sonnet-4-6

  claude-sonnet-4-6 (Anthropic Claude)
  ─────────────────────────────────────
  Provider:    anthropic
  Pricing:     $3.00 input / $15.00 output per M tokens
  Context:     200K tokens
  Features:    vision, extended_thinking, tool_use
  Aliases:     claude-sonnet-4, claude-sonnet-4-6-20250514
  Status:      ✓ API key available

  Use: nika infer "..." -m claude-sonnet-4-6
```

**Data source:** Combine pricing from `cost.rs` + known aliases from pricing table keys.

### Task 8: `nika model recommend`

```bash
$ nika model recommend

  Based on your API keys (anthropic ✓, openai ✓):

  ★ Balanced:    claude-sonnet-4-6    $3.00/$15.00   [anthropic]
  ⚡ Budget:     gpt-4o-mini          $0.15/$0.60    [openai]
  🏆 Quality:    claude-opus-4-6      $15.00/$75.00  [anthropic]
  🏠 Free:       qwen3:8b             local          [native]

  Set default: nika config set default_model claude-sonnet-4-6
```

Logic:
- Scan available API keys
- Pick cheapest, most expensive, and best value from available models
- Include local if native-inference compiled + model downloaded
- Show actionable `config set` command

---

## SECTION C: CLI Polish (Tasks 9-11)

### Task 9: Redesign AFTER_HELP text

Complete rewrite with verbs first:

```
DIRECT VERBS (no YAML needed):
    nika infer "Explain X"                Call LLM directly
    nika infer "Summarize" --stdin        Pipe: cat file | nika infer "..."
    nika infer -m gpt-4o-mini "..."       Specify model (auto-detect provider)
    nika fetch URL --extract article      Fetch + extract content
    nika invoke nika:dimensions img.jpg   Call builtin tool
    nika agent "Research X" --turns 5     Multi-turn agent

MODELS & PROVIDERS:
    nika model list                       All models with pricing
    nika model list --provider anthropic  Filter by provider
    nika model info claude-sonnet-4-6     Model details + cost
    nika model recommend                  Smart suggestion
    nika provider list                    API key status
    nika keys set anthropic           Store key in keychain

WORKFLOWS:
    nika run [file]                       Run workflow (auto-discover)
    nika run file -i key=value            Override inputs
    nika run file --dry-run               Preview plan
    nika run file --task step2            Run single task + deps
    nika check <file> [--strict]          Validate
    nika new <name> [--verb infer]        Create scaffold

INTERACTIVE:
    nika ui / chat / studio               TUI modes
    nika course                           Learn Nika

CONFIG & DIAGNOSTICS:
    nika config list|set|edit             Settings
    nika mcp add|list|test|tools          MCP servers
    nika doctor [--fix]                   System health
    nika media list|tools|clean           CAS store

GLOBAL FLAGS:
    -v/-vv/-vvv  Verbosity    -q  Quiet    --color auto|always|never
```

### Task 10: Visual output for verb commands

Border box with model header + cost footer:

```rust
fn print_verb_header(model: &str, provider: &str, is_tty: bool) {
    if is_tty {
        let header = format!("{} via {}", model.cyan(), provider.dimmed());
        eprintln!("  ┌─ {} {}", header, "─".repeat(60 - header.len()).dimmed());
    }
}

fn print_verb_footer(elapsed: Duration, tokens: u64, cost: f64, is_tty: bool) {
    if is_tty {
        eprintln!(
            "  └{} {}",
            "─".repeat(40).dimmed(),
            format!("{}ms · {} tokens · ${:.4}", elapsed.as_millis(), tokens, cost).dimmed()
        );
    }
}
```

### Task 11: Config defaults (default_provider, default_model)

Read from `.nika/config.toml`:

```toml
[defaults]
provider = "anthropic"
model = "claude-sonnet-4-6"
```

Used in `detect_provider()` as second priority (after explicit flags).

---

## SECTION D: Testing + Release (Tasks 12-13)

### Task 12: E2E verification

**Infer:**
```bash
nika infer "Say hello" -p mock                    # basic
echo "hi" | nika infer "Reply" -p mock --stdin     # stdin pipe
echo "What is 2+2?" | nika infer - -p mock         # prompt from stdin
nika infer "JSON" -p mock --json                   # json mode
nika infer "Extract" -p mock --from-example '{"name":""}' # structured
nika infer -m anthropic/claude-sonnet-4-6 "test"   # composite model
```

**Fetch:**
```bash
nika fetch https://example.com                     # basic
nika fetch https://example.com --extract markdown   # extract
nika fetch https://httpbin.org/post -X POST --json-body '{"x":1}' # POST
```

**Invoke:**
```bash
nika invoke --list                                  # list tools
nika invoke nika:dimensions test-image.jpg          # builtin + file
nika invoke nika:nonexistent 2>&1                   # error path
```

**Model:**
```bash
nika model list                                     # cloud + local
nika model list --provider anthropic                # filter
nika model info claude-sonnet-4-6                   # cloud info
nika model recommend                                # recommendation
nika model list --json                              # machine-readable
```

**Help:**
```bash
nika --help | grep -c "infer\|fetch\|invoke\|agent" # all 4 listed
nika infer --help                                    # flag docs
nika model --help                                    # subcommand docs
```

### Task 13: Version bump + CHANGELOG + release

Bump to v0.46.0. Tag. GitHub release with ASCII art.

---

## Ordering

| Section | Task | LOC est. | Description |
|---------|------|----------|-------------|
| A | T1: Commands enum | ~100 | 4 new subcommands with all flags |
| A | T2: Verb handlers | ~350 | verbs.rs: infer + fetch + invoke + agent |
| A | T3: Match arms | ~30 | Wire 4 new commands |
| A | T4: Composite model | ~40 | Parse provider/model, auto-detect provider |
| B | T5: Feature gate fix | ~30 | Move model list/info/recommend out of gate |
| B | T6: Model list enriched | ~120 | Cloud pricing + local status display |
| B | T7: Model info cloud | ~60 | Cloud model details + pricing |
| B | T8: Model recommend | ~60 | Smart suggestion based on keys |
| C | T9: AFTER_HELP | ~60 | Complete help text redesign |
| C | T10: Visual output | ~40 | Border box + cost footer |
| C | T11: Config defaults | ~30 | default_provider + default_model |
| D | T12: E2E tests | ~40 | All verb + model tests |
| D | T13: Release | ~10 | Bump + tag + GitHub release |
| **Total** | | **~970** | |

---

## Non-Goals (V2 — v0.47.0)

- Streaming token-by-token output (requires direct provider API, not execute())
- Vision from CLI (`nika infer --image photo.jpg "Describe"`)
- MCP server auto-start for non-builtin invoke
- Agent with file edit tools (security implications)
- `nika exec` command (bash exists)
- Tab completion for tool names
- Model comparison table (`nika model compare gpt-4o claude-sonnet`)
