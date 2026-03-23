//! Machine-level auto-setup for Nika.
//!
//! Detects installed editors/AI tools and configures them automatically.
//! Tracks setup state via `~/.nika/machine.toml` marker file.
//! Called by `nika init` before the project wizard (Phase 1).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;

/// Result of a single setup action.
#[derive(Debug)]
pub struct SetupResult {
    pub name: String,
    pub success: bool,
    #[allow(dead_code)]
    pub message: String,
}

/// Distinguishes "never set up" from "version mismatch" from "ready".
#[derive(Debug, PartialEq)]
pub enum MachineStatus {
    /// Never set up (no marker file)
    NeverSetup,
    /// Set up but version doesn't match (user upgraded)
    NeedsUpdate,
    /// Fully set up and current
    Ready,
}

/// Return the machine setup status: NeverSetup, NeedsUpdate, or Ready.
pub fn machine_setup_status() -> MachineStatus {
    let marker = machine_toml_path();
    let content = match std::fs::read_to_string(&marker) {
        Ok(c) => c,
        Err(_) => return MachineStatus::NeverSetup,
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
            let version = rest.trim_matches('"');
            if version == env!("CARGO_PKG_VERSION") {
                return MachineStatus::Ready;
            } else {
                return MachineStatus::NeedsUpdate;
            }
        }
    }
    MachineStatus::NeedsUpdate
}

/// Check if machine setup has been done (marker file exists + version current).
pub fn is_machine_setup() -> bool {
    machine_setup_status() == MachineStatus::Ready
}

/// Returns true in CI/CD or automated environments where machine setup must not run.
pub fn is_ci() -> bool {
    use std::env;
    if env::var("NIKA_NO_SETUP").map(|v| v == "1").unwrap_or(false) {
        return true;
    }
    if env::var("CI").is_ok() {
        return true;
    }
    let ci_vars = [
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "CIRCLECI",
        "JENKINS_URL",
        "BUILDKITE",
        "TRAVIS",
        "CODEBUILD_BUILD_ID",
        "TF_BUILD",
    ];
    if ci_vars.iter().any(|v| env::var(v).is_ok()) {
        return true;
    }
    if dirs::home_dir().is_none() {
        return true;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let dumb = env::var("TERM").map(|t| t == "dumb").unwrap_or(false);
        let no_display = env::var("DISPLAY").is_err() && env::var("WAYLAND_DISPLAY").is_err();
        if dumb && no_display {
            return true;
        }
    }
    false
}

/// Path to the machine marker file.
fn machine_toml_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".nika")
        .join("machine.toml")
}

// ─── Editor Detection ────────────────────────────────────────────────────────

/// Detect which AI-capable editors are installed on this machine.
///
/// Returns a list of editor IDs (lowercase slugs) used as keys for rule
/// installation and tracking in machine.toml.
fn detect_editors() -> Vec<&'static str> {
    let mut editors = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // VS Code
    if which::which("code").is_ok() || check_macos_app("VS Code") {
        editors.push("vscode");
    }
    // Cursor
    if which::which("cursor").is_ok()
        || home.join(".cursor").exists()
        || check_macos_app("Cursor")
    {
        editors.push("cursor");
    }
    // Claude Code
    if which::which("claude").is_ok() || home.join(".claude").exists() {
        editors.push("claude");
    }
    // Windsurf
    if which::which("windsurf").is_ok() || check_macos_app("Windsurf") {
        editors.push("windsurf");
    }
    // Roo Code
    if home.join(".roo").exists() {
        editors.push("roo");
    }
    // Copilot (via GitHub CLI or VS Code)
    if which::which("gh").is_ok() || editors.contains(&"vscode") {
        editors.push("copilot");
    }

    editors
}

/// Run the full machine auto-setup (Phase 1).
///
/// Detects editors and AI tools, installs extensions/rules/completions.
/// Prints progress as it goes. Writes marker file on success.
/// This is SILENT by design — no questions asked, just detect and install.
pub fn run_machine_setup() -> Vec<SetupResult> {
    let mut results = Vec::new();

    println!();
    println!("  {}", "Machine setup".bold().underline());

    // 1. Editors: detect + install extension
    results.extend(setup_editors());

    // 2. AI tools: detect + install rules
    results.extend(setup_ai_rules());

    // 3. Shell completions
    results.push(setup_completions());

    // Write marker file
    write_marker(&results);

    // Summary line
    let ok = results.iter().filter(|r| r.success).count();
    let total = results.len();
    println!();
    println!(
        "  {} Machine ready ({}/{} configured)",
        "\u{2713}".green(),
        ok,
        total
    );

    results
}

fn setup_editors() -> Vec<SetupResult> {
    let mut results = Vec::new();

    let editors: &[(&str, &str, &str)] = &[
        ("VS Code", "code", "supernovae-studio.nika-lang"),
        ("Cursor", "cursor", "supernovae-studio.nika-lang"),
        ("Windsurf", "windsurf", "supernovae-studio.nika-lang"),
    ];

    for (name, binary, ext_id) in editors {
        // Check binary in PATH or macOS .app
        let has_binary = which::which(binary).is_ok() || check_macos_app(name);
        if !has_binary {
            continue;
        }

        // Check if extension already installed
        let has_ext = Command::new(binary)
            .args(["--list-extensions"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|list| list.lines().any(|l| l.eq_ignore_ascii_case(ext_id)))
            .unwrap_or(false);

        if has_ext {
            println!("  {} {} + nika-lang extension", "\u{2713}".green(), name);
            results.push(SetupResult {
                name: name.to_string(),
                success: true,
                message: "already installed".into(),
            });
            continue;
        }

        // Install extension
        print!("  {} {} — installing nika-lang...", "\u{25c7}".cyan(), name);
        let install = Command::new(binary)
            .args(["--install-extension", ext_id])
            .output();

        match install {
            Ok(output) if output.status.success() => {
                println!(
                    "\r  {} {} — nika-lang installed       ",
                    "\u{2713}".green(),
                    name
                );
                results.push(SetupResult {
                    name: name.to_string(),
                    success: true,
                    message: "installed".into(),
                });
            }
            _ => {
                println!(
                    "\r  {} {} — install failed          ",
                    "\u{2717}".red(),
                    name
                );
                results.push(SetupResult {
                    name: name.to_string(),
                    success: false,
                    message: format!("run: {} --install-extension {}", binary, ext_id),
                });
            }
        }
    }

    if results.is_empty() {
        println!(
            "  {} No editors detected (VS Code, Cursor, Windsurf)",
            "\u{25cb}".dimmed()
        );
    }

    results
}

#[cfg(target_os = "macos")]
fn check_macos_app(name: &str) -> bool {
    let app_names: &[&str] = match name {
        "VS Code" => &["Visual Studio Code"],
        "Cursor" => &["Cursor"],
        "Windsurf" => &["Windsurf"],
        _ => return false,
    };
    app_names.iter().any(|app| {
        std::path::Path::new(&format!("/Applications/{}.app", app)).exists()
            || dirs::home_dir()
                .map(|h| h.join(format!("Applications/{}.app", app)).exists())
                .unwrap_or(false)
    })
}

#[cfg(not(target_os = "macos"))]
fn check_macos_app(_name: &str) -> bool {
    false
}

/// Comprehensive Nika rules for Claude Code (~/.claude/rules/nika.md).
const CLAUDE_RULES_CONTENT: &str = r#"# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: "echo hello"` |
| `fetch:` | HTTP request | `fetch: "https://api.example.com"` |
| `invoke:` | MCP tool call | `invoke:` block with `tool:` + `params:` |
| `agent:` | Multi-turn loop | `agent:` block with `tools:` + `max_turns:` |

## Complete Workflow Example

```yaml
schema: "nika/workflow@0.12"
workflow: research-and-summarize
description: "Research a topic and create a summary"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: |
        Research the following topic thoroughly: {{inputs.topic}}
        Provide key findings, trends, and notable projects.
      temperature: 0.7

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer:
      prompt: |
        Create a concise executive summary from this research:
        {{with.data}}
      max_tokens: 500
```

## Workflow Header Fields

```yaml
schema: "nika/workflow@0.12"               # Required. Always "nika/workflow@0.12"
workflow: my-workflow          # Optional. Defaults to filename
description: "What it does"   # Optional
provider: anthropic            # Default LLM provider for all tasks
model: claude-sonnet-4-20250514  # Default model for all tasks

inputs:                        # Workflow parameters
  topic: "default value"

context:                       # File context bindings
  files:
    readme: ./README.md

skills:                        # Prompt augmentation files
  writing: ./skills/writing.md

artifacts:                     # Persist outputs to files
  dir: ./output
  format: markdown
```

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | upper | trim}}`
- **Dependencies**: `depends_on: [task_id]` for ordering without data
- **Inputs**: `{{inputs.param}}` for workflow parameters
- **Context files**: `{{context.readme}}` for loaded file content

## Pipe Transforms (31 available)

**String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `to_string`
**Array**: `first`, `last`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
**Numeric**: `to_number`, `round`, `abs`, `ceil`, `floor`
**Type**: `to_bool`, `to_json`, `parse_json`, `type_of`
**Parametric**: `join(", ")`, `split(",")`, `default("fallback")`
**System**: `shell` (execute as shell command)

Usage: `{{with.items | flatten | unique | join(", ")}}`

## Providers (7 Cloud + 1 Local)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514, claude-haiku-3.5 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest, mistral-small-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick, mixtral-8x7b |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
  infer: "Process: {{with.item}}"
```

Access loop variable via `with:` prefix: `{{with.item}}` (same as all bindings).

## 24 Builtin Tools (nika:*)

**Always-on**: `nika:import`, `nika:dimensions`, `nika:thumbhash`, `nika:dominant_color`, `nika:pipeline`
**Media core**: `nika:thumbnail`, `nika:convert`, `nika:strip`, `nika:metadata`, `nika:optimize`, `nika:svg_render`
**Opt-in**: `nika:phash`, `nika:compare`, `nika:pdf_extract`, `nika:chart`, `nika:provenance`, `nika:verify`, `nika:qr_validate`, `nika:quality`, `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`

## Fetch Extract Modes (9)

| Mode | Description |
|------|-------------|
| `markdown` | Clean Markdown from HTML |
| `article` | Main article content (Readability) |
| `text` | Visible text, optionally filtered by `selector:` |
| `selector` | Raw HTML of matching elements (requires `selector:`) |
| `metadata` | OG, Twitter Cards, JSON-LD, SEO tags |
| `links` | Link classification (internal/external) |
| `jsonpath` | JSONPath query on JSON responses (requires `selector:` for path) |
| `feed` | RSS/Atom/JSON Feed parsing |
| `llm_txt` | AI content discovery (/llms.txt) |

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `use: { data: step1 }` | `with: { data: $step1 }` ($ prefix required) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `retry: 3` | `retry: { max_attempts: 3, delay: 2 }` |
| `.yaml` extension | `.nika.yaml` extension |
| Direct Cypher/SQL | Use `invoke:` with MCP tools |
| `shell: bash` | `shell: true` (boolean, not shell name) |
| Missing `schema:` line | Always start with `schema: "nika/workflow@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (always array) |

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
nika ui                                # TUI
nika provider list                     # API key status
```
"#;

/// Unified Nika rules for Cursor (~/.cursor/rules/nika.mdc).
///
/// Merges syntax, patterns, architecture, and security into one comprehensive
/// .mdc file triggered on *.nika.yaml files.
const CURSOR_NIKA_RULES: &str = r#"---
description: Nika workflow engine — complete syntax, patterns, architecture, and security reference
globs: "**/*.nika.yaml"
alwaysApply: false
---

# Nika Workflow Engine

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Short form | Full form key fields |
|------|---------|------------|----------------------|
| `infer:` | LLM generation | `infer: "prompt"` | `prompt`, `system`, `model`, `temperature`, `max_tokens` |
| `exec:` | Shell command | `exec: "command"` | `command`, `shell`, `cwd`, `timeout`, `env` |
| `fetch:` | HTTP request | `fetch: "url"` | `url`, `method`, `headers`, `body`, `extract`, `response` |
| `invoke:` | MCP / builtin tool | `invoke: "nika:dims"` | `tool`, `params`, `mcp`, `resource`, `timeout` |
| `agent:` | Multi-turn loop | *(no short form)* | `system`, `prompt`, `tools`, `max_turns`, `token_budget` |

## Complete Workflow Example

```yaml
schema: "nika/workflow@0.12"
workflow: research-and-summarize
description: "Research a topic and create a summary"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: |
        Research the following topic thoroughly: {{inputs.topic}}
        Provide key findings, trends, and notable projects.
      temperature: 0.7

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer:
      prompt: |
        Create a concise executive summary from this research:
        {{with.data}}
      max_tokens: 500
```

## Workflow Header Fields

```yaml
schema: "nika/workflow@0.12"     # Required. Always this exact string
workflow: my-workflow              # Optional. Defaults to filename
description: "What it does"       # Optional
provider: anthropic                # Default LLM provider for all tasks
model: claude-sonnet-4-20250514   # Default model for all tasks

inputs:                            # Workflow parameters
  topic: "default value"

context:                           # File context bindings
  files:
    readme: ./README.md

skills:                            # Prompt augmentation files
  writing: ./skills/writing.md

artifacts:                         # Persist outputs to files
  dir: ./output
  format: markdown
```

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | upper | trim}}`
- **Dependencies**: `depends_on: [task_id]` for ordering without data
- **Inputs**: `{{inputs.param}}` for workflow parameters
- **Context files**: `{{context.readme}}` for loaded file content

## Pipe Transforms (31 available)

**String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `to_string`
**Array**: `first`, `last`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
**Numeric**: `to_number`, `round`, `abs`, `ceil`, `floor`
**Type**: `to_bool`, `to_json`, `parse_json`, `type_of`
**Parametric**: `join(", ")`, `split(",")`, `default("fallback")`
**System**: `shell` (execute as shell command)

Usage: `{{with.items | flatten | unique | join(", ")}}`

## Providers (7 Cloud + 1 Local)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514, claude-haiku-3.5 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest, mistral-small-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick, mixtral-8x7b |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

## Infer Verb (Full Form)

```yaml
- id: generate
  infer:
    prompt: "Your prompt here"
    system: "You are a helpful assistant"
    model: claude-sonnet-4-20250514    # Task-level override
    temperature: 0.7                   # 0.0 - 2.0
    max_tokens: 1000                   # Max output tokens
    extended_thinking: true            # Claude extended thinking
    thinking_budget: 10000             # Thinking token budget
    response_format: json              # text, json, markdown
```

## Exec Verb (Full Form)

```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true                        # Run via sh -c (default: false)
    cwd: "./frontend"
    timeout: 60                        # Seconds
    env:
      NODE_ENV: production
```

## Fetch Verb (Full Form + Extract)

```yaml
- id: scrape
  fetch:
    url: "https://example.com/article"
    method: GET
    headers:
      Authorization: "Bearer {{inputs.token}}"
    extract: markdown                  # Post-processing mode
    response: full                     # full, binary, or omit for raw body
    timeout: 30
```

### 9 Extract Modes

| Mode | Description | Requires `selector:` |
|------|-------------|---------------------|
| `markdown` | Clean Markdown from HTML | No |
| `article` | Main article content (Readability) | No |
| `text` | Visible text, optionally filtered | Optional |
| `selector` | Raw HTML of matching elements | Yes |
| `metadata` | OG, Twitter Cards, JSON-LD, SEO | No |
| `links` | Link classification (internal/external) | No |
| `jsonpath` | JSONPath query on JSON responses | Yes (path) |
| `feed` | RSS/Atom/JSON Feed parsing | No |
| `llm_txt` | AI content discovery (/llms.txt) | No |

## Invoke Verb (MCP + Builtin Tools)

```yaml
- id: search
  invoke:
    tool: "novanet/novanet_search"     # server/tool_name format
    params:
      query: "{{with.topic}}"
      limit: 10
    timeout: 30
```

## Agent Verb (Full Reference)

```yaml
- id: assistant
  agent:
    system: "You are a research assistant"
    prompt: "Find and analyze {{inputs.topic}}"
    tools: [novanet/novanet_search]
    max_turns: 10
    token_budget: 50000
    temperature: 0.5
    guardrails:
      - type: length
        min_words: 100
        max_words: 1000
      - type: schema
        json_schema:
          type: object
          properties:
            findings: { type: array }
          required: [findings]
```

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
    fail_fast: true
  infer: "Process: {{with.item}}"
```

Access loop variable via `with:` prefix: `{{with.item}}` (same as all bindings).

## Pipeline Patterns

### Fan-Out / Fan-In

```yaml
tasks:
  - id: get-urls
    infer: "List 5 URLs about {{inputs.topic}}"
    structured:
      schema:
        type: object
        properties:
          urls: { type: array, items: { type: string } }

  - id: scrape-all
    with:
      urls: $get-urls
    for_each:
      items: "{{with.urls.urls}}"
      as: url
      concurrency: 5
    fetch:
      url: "{{with.url}}"
      extract: article

  - id: synthesize
    with:
      articles: $scrape-all
    infer: "Synthesize these articles: {{with.articles}}"
```

## 24 Builtin Tools (nika:*)

**Always-on**: `nika:import`, `nika:dimensions`, `nika:thumbhash`, `nika:dominant_color`, `nika:pipeline`
**Media core**: `nika:thumbnail`, `nika:convert`, `nika:strip`, `nika:metadata`, `nika:optimize`, `nika:svg_render`
**Opt-in**: `nika:phash`, `nika:compare`, `nika:pdf_extract`, `nika:chart`, `nika:provenance`, `nika:verify`, `nika:qr_validate`, `nika:quality`, `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `use: { data: step1 }` | `with: { data: $step1 }` ($ prefix required) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `retry: 3` | `retry: { max_attempts: 3, delay: 2 }` |
| `.yaml` extension | `.nika.yaml` extension |
| Direct Cypher/SQL | Use `invoke:` with MCP tools |
| `shell: bash` | `shell: true` (boolean, not shell name) |
| Missing `schema:` line | Always start with `schema: "nika/workflow@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (always array) |

## Architecture

- Nika connects to NovaNet via MCP protocol ONLY. No direct database access.
- Use `invoke:` verb for all MCP tool calls.
- Errors use `NikaError` with NIKA-XXX codes.
- Extensions: `.nika.yaml` for workflows.

## Security

- Validate all file paths against directory traversal attacks.
- API keys: env vars only. Never hardcode.
- `fetch:` verb validates URLs against SSRF (private IP ranges blocked).
- `exec:` verb has command blocklist (no rm -rf, no sudo, etc.).

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
```
"#;

/// Nika rules for Copilot (~/.github/copilot/nika.instructions.md).
const COPILOT_RULES: &str = r#"---
applyTo: "**/*.nika.yaml"
---

# Nika Workflow Syntax

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: "echo hello"` |
| `fetch:` | HTTP request | `fetch: "https://api.example.com"` |
| `invoke:` | MCP tool call | `invoke:` block with `tool:` + `params:` |
| `agent:` | Multi-turn loop | `agent:` block with `tools:` + `max_turns:` |

## Complete Workflow Example

```yaml
schema: "nika/workflow@0.12"
workflow: research-and-summarize
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: |
        Research the following topic: {{inputs.topic}}
        Provide key findings and trends.
      temperature: 0.7

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer:
      prompt: |
        Create a concise summary from this research:
        {{with.data}}
      max_tokens: 500
```

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | upper | trim}}`
- **Dependencies**: `depends_on: [task_id]` for ordering without data
- **Inputs**: `{{inputs.param}}` for workflow parameters

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
  infer: "Process: {{with.item}}"
```

Access loop variable via `with.` prefix: `{{with.item}}`

## Providers (7 Cloud + 1 Local)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `.yaml` extension | `.nika.yaml` extension |
| Missing `schema:` line | Always start with `schema: "nika/workflow@0.12"` |

## Key Error Codes

| Code | Meaning |
|------|---------|
| NIKA-010 | Schema validation error |
| NIKA-020 | DAG cycle detected |
| NIKA-034 | Provider/model mismatch |
| NIKA-040 | Template resolution error |
| NIKA-140 | AST analysis failure |

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
```
"#;

/// Nika rules for Windsurf (~/.windsurf/rules/nika.md).
const WINDSURF_RULES: &str = r#"---
trigger: glob
globs: "**/*.nika.yaml"
description: "Nika YAML workflow engine rules"
---

# Nika Workflow Syntax

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: "echo hello"` |
| `fetch:` | HTTP request | `fetch: "https://api.example.com"` |
| `invoke:` | MCP tool call | `invoke:` block with `tool:` + `params:` |
| `agent:` | Multi-turn loop | `agent:` block with `tools:` + `max_turns:` |

## Complete Workflow Example

```yaml
schema: "nika/workflow@0.12"
workflow: research-and-summarize
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: |
        Research the following topic: {{inputs.topic}}
        Provide key findings and trends.
      temperature: 0.7

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer:
      prompt: |
        Create a concise summary from this research:
        {{with.data}}
      max_tokens: 500
```

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | upper | trim}}`
- **Dependencies**: `depends_on: [task_id]` for ordering without data
- **Inputs**: `{{inputs.param}}` for workflow parameters

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
  infer: "Process: {{with.item}}"
```

Access loop variable via `with.` prefix: `{{with.item}}`

## Providers (7 Cloud + 1 Local)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `.yaml` extension | `.nika.yaml` extension |
| Missing `schema:` line | Always start with `schema: "nika/workflow@0.12"` |

## Key Error Codes

| Code | Meaning |
|------|---------|
| NIKA-010 | Schema validation error |
| NIKA-020 | DAG cycle detected |
| NIKA-034 | Provider/model mismatch |
| NIKA-040 | Template resolution error |
| NIKA-140 | AST analysis failure |

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
```
"#;

/// Nika rules for Roo Code (~/.roo/rules/nika.md).
const ROO_RULES: &str = r#"---
description: "Nika YAML workflow engine syntax and patterns"
globs: ["*.nika.yaml"]
---

# Nika Workflow Syntax

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: "echo hello"` |
| `fetch:` | HTTP request | `fetch: "https://api.example.com"` |
| `invoke:` | MCP tool call | `invoke:` block with `tool:` + `params:` |
| `agent:` | Multi-turn loop | `agent:` block with `tools:` + `max_turns:` |

## Complete Workflow Example

```yaml
schema: "nika/workflow@0.12"
workflow: research-and-summarize
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: |
        Research the following topic: {{inputs.topic}}
        Provide key findings and trends.
      temperature: 0.7

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer:
      prompt: |
        Create a concise summary from this research:
        {{with.data}}
      max_tokens: 500
```

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | upper | trim}}`
- **Dependencies**: `depends_on: [task_id]` for ordering without data
- **Inputs**: `{{inputs.param}}` for workflow parameters

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
  infer: "Process: {{with.item}}"
```

Access loop variable via `with.` prefix: `{{with.item}}`

## Providers (7 Cloud + 1 Local)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `.yaml` extension | `.nika.yaml` extension |
| Missing `schema:` line | Always start with `schema: "nika/workflow@0.12"` |

## Key Error Codes

| Code | Meaning |
|------|---------|
| NIKA-010 | Schema validation error |
| NIKA-020 | DAG cycle detected |
| NIKA-034 | Provider/model mismatch |
| NIKA-040 | Template resolution error |
| NIKA-140 | AST analysis failure |

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
```
"#;

// ─── Content Hash Fingerprinting ─────────────────────────────────────────────

fn hash_content(content: &str) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(content.as_bytes()))
}

fn load_rule_hashes() -> HashMap<String, String> {
    let content = match std::fs::read_to_string(machine_toml_path()) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut in_section = false;
    let mut map = HashMap::new();
    for line in content.lines() {
        let t = line.trim();
        if t == "[rule_hashes]" {
            in_section = true;
            continue;
        }
        if t.starts_with('[') {
            in_section = false;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').to_string();
            if !key.is_empty() && !val.is_empty() {
                map.insert(key, val);
            }
        }
    }
    map
}

fn update_rule_hash(editor_key: &str, hash: &str) {
    let path = machine_toml_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_line = format!("{} = \"{}\"", editor_key, hash);
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let section_idx = lines.iter().position(|l| l.trim() == "[rule_hashes]");
    match section_idx {
        Some(idx) => {
            // Find existing key in section
            let key_idx = lines[idx + 1..]
                .iter()
                .position(|l| {
                    let t = l.trim();
                    t.starts_with(editor_key)
                        && t[editor_key.len()..].trim_start().starts_with('=')
                })
                .map(|i| i + idx + 1);
            match key_idx {
                Some(ki) => lines[ki] = new_line,
                None => lines.insert(idx + 1, new_line),
            }
        }
        None => {
            lines.push(String::new());
            lines.push("[rule_hashes]".to_string());
            lines.push(new_line);
        }
    }
    std::fs::write(&path, lines.join("\n") + "\n").ok();
}

fn setup_ai_rules() -> Vec<SetupResult> {
    let home = dirs::home_dir();
    if home.is_none() {
        return vec![SetupResult {
            name: "AI Rules".into(),
            success: false,
            message: "cannot determine home directory".into(),
        }];
    }
    let home = home.unwrap();
    let mut results = Vec::new();
    let editors = detect_editors();
    let hashes = load_rule_hashes();

    // Claude Code
    if editors.contains(&"claude") {
        install_rule(
            &home.join(".claude/rules/nika.md"),
            CLAUDE_RULES_CONTENT,
            "Claude Code",
            "claude",
            &hashes,
            &mut results,
        );
    }

    // Cursor
    if editors.contains(&"cursor") {
        install_rule(
            &home.join(".cursor/rules/nika.mdc"),
            CURSOR_NIKA_RULES,
            "Cursor",
            "cursor",
            &hashes,
            &mut results,
        );
    }

    // Copilot
    if editors.contains(&"copilot") {
        install_rule(
            &home.join(".github/copilot/nika.instructions.md"),
            COPILOT_RULES,
            "Copilot",
            "copilot",
            &hashes,
            &mut results,
        );
    }

    // Windsurf
    if editors.contains(&"windsurf") {
        install_rule(
            &home.join(".windsurf/rules/nika.md"),
            WINDSURF_RULES,
            "Windsurf",
            "windsurf",
            &hashes,
            &mut results,
        );
    }

    // Roo Code
    if editors.contains(&"roo") {
        install_rule(
            &home.join(".roo/rules/nika.md"),
            ROO_RULES,
            "Roo Code",
            "roo",
            &hashes,
            &mut results,
        );
    }

    // Agent Skills: install to ~/.agents/skills/
    let skills_dir = home.join(".agents/skills");
    let has_skills = skills_dir.join("nika-workflow-syntax").exists();
    if !has_skills {
        let skill_dir = skills_dir.join("nika-workflow-syntax");
        std::fs::create_dir_all(&skill_dir).ok();
        let skill_content = concat!(
            "# Nika Workflow Syntax\n\n",
            "Refer to AGENTS.md in any Nika project ",
            "for the complete workflow syntax reference.\n"
        );
        if std::fs::write(skill_dir.join("SKILL.md"), skill_content).is_ok() {
            println!(
                "  {} Agent Skills installed [~/.agents/skills/]",
                "\u{2713}".green()
            );
            results.push(SetupResult {
                name: "Agent Skills".into(),
                success: true,
                message: "installed".into(),
            });
        }
    } else {
        println!("  {} Agent Skills [~/.agents/skills/]", "\u{2713}".green());
        results.push(SetupResult {
            name: "Agent Skills".into(),
            success: true,
            message: "present".into(),
        });
    }

    results
}

/// Install a rule file for an editor with content-hash protection.
///
/// - If file exists and content matches expected -> skip ("up to date")
/// - If file exists and disk hash doesn't match stored hash -> skip with warning
///   ("user-customized")
/// - Otherwise -> write + update hash
fn install_rule(
    path: &Path,
    content: &str,
    name: &str,
    editor_key: &str,
    hashes: &HashMap<String, String>,
    results: &mut Vec<SetupResult>,
) {
    let expected_hash = hash_content(content);

    // If file exists, check hashes before overwriting
    if path.exists() {
        if let Ok(disk_content) = std::fs::read_to_string(path) {
            let disk_hash = hash_content(&disk_content);

            // Content already matches — nothing to do
            if disk_hash == expected_hash {
                println!("  {} {} — up to date", "\u{2713}".green(), name);
                results.push(SetupResult {
                    name: name.into(),
                    success: true,
                    message: "up to date".into(),
                });
                return;
            }

            // File differs from expected. Was it customized by the user?
            if let Some(stored_hash) = hashes.get(editor_key) {
                if disk_hash != *stored_hash {
                    // User modified the file — don't overwrite
                    println!(
                        "  {} {} — user-customized, skipping",
                        "\u{26a0}".yellow(),
                        name
                    );
                    results.push(SetupResult {
                        name: name.into(),
                        success: true,
                        message: "user-customized, preserved".into(),
                    });
                    return;
                }
            }
            // stored_hash matches disk (or no stored hash yet) — safe to overwrite
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    match std::fs::write(path, content) {
        Ok(()) => {
            update_rule_hash(editor_key, &expected_hash);
            println!("  {} {} — Nika rules installed", "\u{2713}".green(), name);
            results.push(SetupResult {
                name: name.into(),
                success: true,
                message: "installed".into(),
            });
        }
        Err(e) => {
            println!(
                "  {} {} — write failed: {}",
                "\u{2717}".red(),
                name,
                e
            );
            results.push(SetupResult {
                name: name.into(),
                success: false,
                message: format!("write failed: {}", e),
            });
        }
    }
}

/// Silently install a rule file (used by quick_editor_scan — no terminal output).
/// Respects content-hash protection: skips user-customized files.
fn install_rule_silent(
    path: &Path,
    content: &str,
    editor_key: &str,
    hashes: &HashMap<String, String>,
) -> bool {
    let expected_hash = hash_content(content);

    if path.exists() {
        if let Ok(disk_content) = std::fs::read_to_string(path) {
            let disk_hash = hash_content(&disk_content);
            // Already up to date
            if disk_hash == expected_hash {
                return true;
            }
            // User-customized — don't overwrite
            if let Some(stored_hash) = hashes.get(editor_key) {
                if disk_hash != *stored_hash {
                    return false;
                }
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if std::fs::write(path, content).is_ok() {
        update_rule_hash(editor_key, &expected_hash);
        true
    } else {
        false
    }
}

fn setup_completions() -> SetupResult {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            return SetupResult {
                name: "Completions".into(),
                success: false,
                message: "cannot determine home directory".into(),
            };
        }
    };
    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_name = if shell.contains("zsh") {
        "zsh"
    } else if shell.contains("bash") {
        "bash"
    } else if shell.contains("fish") {
        "fish"
    } else {
        return SetupResult {
            name: "Completions".into(),
            success: false,
            message: "unknown shell".into(),
        };
    };

    // Check if nika completion command exists
    let output = Command::new("nika")
        .args(["completion", shell_name])
        .output();

    match output {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            // Write completions to appropriate location
            let target = match shell_name {
                "zsh" => {
                    let zfunc = home.join(".zfunc");
                    std::fs::create_dir_all(&zfunc).ok();
                    Some(zfunc.join("_nika"))
                }
                "bash" => {
                    let dir = home.join(".local/share/bash-completion/completions");
                    std::fs::create_dir_all(&dir).ok();
                    Some(dir.join("nika"))
                }
                "fish" => {
                    let dir = dirs::config_dir()
                        .unwrap_or_default()
                        .join("fish/completions");
                    std::fs::create_dir_all(&dir).ok();
                    Some(dir.join("nika.fish"))
                }
                _ => None,
            };

            if let Some(target) = target {
                if std::fs::write(&target, &o.stdout).is_ok() {
                    println!(
                        "  {} {} completions installed",
                        "\u{2713}".green(),
                        shell_name
                    );
                    return SetupResult {
                        name: "Completions".into(),
                        success: true,
                        message: format!("{} completions at {}", shell_name, target.display()),
                    };
                }
            }

            SetupResult {
                name: "Completions".into(),
                success: false,
                message: "could not write completions".into(),
            }
        }
        _ => {
            println!(
                "  {} {} completions (nika completion not available)",
                "\u{25cb}".dimmed(),
                shell_name
            );
            SetupResult {
                name: "Completions".into(),
                success: false,
                message: "nika completion command not available".into(),
            }
        }
    }
}

fn write_marker(results: &[SetupResult]) {
    let editors = detect_editors();
    write_marker_with_editors(results, &editors);
}

fn write_marker_with_editors(results: &[SetupResult], editors: &[&str]) {
    let marker_path = machine_toml_path();
    let Some(dir) = marker_path.parent() else { return; };
    std::fs::create_dir_all(dir).ok();

    let ai_tools: Vec<&str> = results
        .iter()
        .filter(|r| {
            r.success
                && [
                    "Claude Code",
                    "Cursor",
                    "Copilot",
                    "Windsurf",
                    "Roo Code",
                    "Agent Skills",
                ]
                .contains(&r.name.as_str())
        })
        .map(|r| r.name.as_str())
        .collect();

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Format editors as TOML array: editors = ["vscode", "claude", "cursor"]
    let editors_toml: Vec<String> = editors.iter().map(|e| format!("\"{}\"", e)).collect();

    let content = format!(
        "[machine]\nsetup_at = \"{}\"\nversion = \"{}\"\neditors = [{}]\nai_tools = {:?}\n",
        secs,
        env!("CARGO_PKG_VERSION"),
        editors_toml.join(", "),
        ai_tools,
    );

    // Don't fail init if marker write fails
    std::fs::write(&marker_path, content).ok();
}

// ─── Quick Editor Re-Scan ────────────────────────────────────────────────────

/// Lightweight scan for newly installed editors. Called on every nika command
/// when machine_setup_status() == Ready. If a new editor is detected that
/// wasn't in machine.toml, install rules silently and update the stored list.
///
/// Adds ~5ms to every command. Acceptable.
pub fn quick_editor_scan() {
    let current = detect_editors();
    let stored = read_stored_editors();

    let new_editors: Vec<&&str> = current
        .iter()
        .filter(|e| !stored.iter().any(|s| s == *e))
        .collect();

    if new_editors.is_empty() {
        return;
    }

    let home = dirs::home_dir().unwrap_or_default();
    let hashes = load_rule_hashes();
    for editor in &new_editors {
        match **editor {
            "claude" => {
                install_rule_silent(
                    &home.join(".claude/rules/nika.md"),
                    CLAUDE_RULES_CONTENT,
                    "claude",
                    &hashes,
                );
            }
            "cursor" => {
                install_rule_silent(
                    &home.join(".cursor/rules/nika.mdc"),
                    CURSOR_NIKA_RULES,
                    "cursor",
                    &hashes,
                );
            }
            "copilot" => {
                install_rule_silent(
                    &home.join(".github/copilot/nika.instructions.md"),
                    COPILOT_RULES,
                    "copilot",
                    &hashes,
                );
            }
            "windsurf" => {
                install_rule_silent(
                    &home.join(".windsurf/rules/nika.md"),
                    WINDSURF_RULES,
                    "windsurf",
                    &hashes,
                );
            }
            "roo" => {
                install_rule_silent(
                    &home.join(".roo/rules/nika.md"),
                    ROO_RULES,
                    "roo",
                    &hashes,
                );
            }
            _ => {}
        }
    }

    // Update machine.toml with new editors list
    update_machine_toml_editors(&current);
}

/// Read the editors list from machine.toml.
fn read_stored_editors() -> Vec<String> {
    let marker = machine_toml_path();
    let content = match std::fs::read_to_string(&marker) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("editors") {
            let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
            // Parse TOML array: ["vscode", "claude", "cursor"]
            let inner = rest
                .trim_start_matches('[')
                .trim_end_matches(']');
            return inner
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    Vec::new()
}

/// Update only the editors field in machine.toml, preserving version and
/// setup_at.
fn update_machine_toml_editors(editors: &[&str]) {
    let marker_path = machine_toml_path();
    let content = match std::fs::read_to_string(&marker_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let editors_toml: Vec<String> = editors.iter().map(|e| format!("\"{}\"", e)).collect();
    let new_editors_line = format!("editors = [{}]", editors_toml.join(", "));

    let mut updated = String::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("editors") {
            updated.push_str(&new_editors_line);
            found = true;
        } else {
            updated.push_str(line);
        }
        updated.push('\n');
    }
    if !found {
        updated.push_str(&new_editors_line);
        updated.push('\n');
    }

    std::fs::write(&marker_path, updated).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_toml_path_is_in_home() {
        let path = machine_toml_path();
        assert!(path.to_string_lossy().contains(".nika"));
        assert!(path.to_string_lossy().ends_with("machine.toml"));
    }

    /// All rule constants must use {{with.item}} not bare {{item}} in code
    /// examples (the "Wrong" column in mistake tables is exempt).
    #[test]
    fn all_rules_use_with_item_not_bare_item() {
        let rules: &[(&str, &str)] = &[
            ("CLAUDE_RULES_CONTENT", CLAUDE_RULES_CONTENT),
            ("CURSOR_NIKA_RULES", CURSOR_NIKA_RULES),
            ("COPILOT_RULES", COPILOT_RULES),
            ("WINDSURF_RULES", WINDSURF_RULES),
            ("ROO_RULES", ROO_RULES),
        ];
        for (name, content) in rules {
            for (i, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.contains("{{item}}")
                    && !trimmed.starts_with('|')
                    && !trimmed.contains("Wrong")
                {
                    panic!(
                        "{} line {} has bare {{{{item}}}} outside mistakes table: {}",
                        name,
                        i + 1,
                        trimmed
                    );
                }
            }
        }
    }

    /// All rule constants must reference schema @0.12.
    #[test]
    fn all_rules_reference_current_schema() {
        let rules: &[(&str, &str)] = &[
            ("CLAUDE_RULES_CONTENT", CLAUDE_RULES_CONTENT),
            ("CURSOR_NIKA_RULES", CURSOR_NIKA_RULES),
            ("COPILOT_RULES", COPILOT_RULES),
            ("WINDSURF_RULES", WINDSURF_RULES),
            ("ROO_RULES", ROO_RULES),
        ];
        for (name, content) in rules {
            assert!(
                content.contains("@0.12"),
                "{} missing schema @0.12",
                name
            );
        }
    }

    /// No rule constants should reference nonexistent models.
    #[test]
    fn all_rules_no_nonexistent_models() {
        let rules: &[(&str, &str)] = &[
            ("CLAUDE_RULES_CONTENT", CLAUDE_RULES_CONTENT),
            ("CURSOR_NIKA_RULES", CURSOR_NIKA_RULES),
            ("COPILOT_RULES", COPILOT_RULES),
            ("WINDSURF_RULES", WINDSURF_RULES),
            ("ROO_RULES", ROO_RULES),
        ];
        for (name, content) in rules {
            assert!(
                !content.contains("grok-4"),
                "{} references nonexistent model grok-4",
                name
            );
        }
    }

    /// detect_editors returns a Vec (may be empty in CI, but must not panic).
    #[test]
    fn detect_editors_does_not_panic() {
        let editors = detect_editors();
        // Just ensure it returns without panicking; contents depend on machine
        assert!(editors.len() <= 10, "unexpectedly many editors detected");
    }

    /// read_stored_editors parses a TOML editors array correctly.
    #[test]
    fn read_stored_editors_parses_toml_array() {
        let tmpdir = tempfile::tempdir().unwrap();
        let marker = tmpdir.path().join("machine.toml");
        std::fs::write(
            &marker,
            "[machine]\nversion = \"0.41.3\"\neditors = [\"vscode\", \"claude\", \"cursor\"]\n",
        )
        .unwrap();

        // read_stored_editors uses machine_toml_path() which reads from ~/.nika/,
        // so we test the parsing logic directly
        let content = std::fs::read_to_string(&marker).unwrap();
        let mut parsed = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("editors") {
                let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
                let inner = rest.trim_start_matches('[').trim_end_matches(']');
                parsed = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        assert_eq!(parsed, vec!["vscode", "claude", "cursor"]);
    }

    /// Cursor rules .mdc must have valid frontmatter.
    #[test]
    fn cursor_rules_have_mdc_frontmatter() {
        assert!(
            CURSOR_NIKA_RULES.starts_with("---\n"),
            "CURSOR_NIKA_RULES must start with YAML frontmatter"
        );
        assert!(
            CURSOR_NIKA_RULES.contains("globs:"),
            "CURSOR_NIKA_RULES must have globs: in frontmatter"
        );
        assert!(
            CURSOR_NIKA_RULES.contains("alwaysApply:"),
            "CURSOR_NIKA_RULES must have alwaysApply: in frontmatter"
        );
    }

    /// Copilot rules must have applyTo frontmatter.
    #[test]
    fn copilot_rules_have_apply_to() {
        assert!(
            COPILOT_RULES.contains("applyTo:"),
            "COPILOT_RULES must have applyTo: frontmatter"
        );
    }

    /// Windsurf rules must have trigger frontmatter.
    #[test]
    fn windsurf_rules_have_trigger() {
        assert!(
            WINDSURF_RULES.contains("trigger:"),
            "WINDSURF_RULES must have trigger: frontmatter"
        );
    }

    /// install_rule_silent writes to disk.
    #[test]
    fn install_rule_silent_writes_file() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("sub/dir/rule.md");
        let hashes = HashMap::new();
        let ok = install_rule_silent(&path, "# test rule\n", "test", &hashes);
        assert!(ok);
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# test rule\n");
    }
}
