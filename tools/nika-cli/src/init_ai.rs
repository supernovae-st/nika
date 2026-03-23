//! AI coding assistant file generation for `nika init`
//!
//! Generates per-tool AI rules, AGENTS.md, and git hooks
//! during project initialization.

use colored::Colorize;
use std::fs;
use std::path::Path;

/// Generate all AI integration files for the project
pub fn generate_ai_files(project_dir: &Path) -> Result<(), nika_engine::NikaError> {
    println!("\n  {}", "AI Integration".bold().underline());

    let mut count = 0;

    // AGENTS.md (universal, 20+ tools)
    count += write_if_absent(
        &project_dir.join("AGENTS.md"),
        AGENTS_MD_CONTENT,
        "AGENTS.md",
    );

    // CLAUDE.md symlink
    let claude_md = project_dir.join("CLAUDE.md");
    if !claude_md.exists() {
        #[cfg(unix)]
        {
            match std::os::unix::fs::symlink("AGENTS.md", &claude_md) {
                Ok(()) => {
                    println!("  {} CLAUDE.md → AGENTS.md (symlink)", "✓".green());
                    count += 1;
                }
                Err(e) => {
                    println!("  {} CLAUDE.md symlink failed: {}", "⚠".yellow(), e);
                }
            }
        }
    }

    // Cursor rule
    count += write_if_absent_with_dir(
        project_dir,
        ".cursor/rules/nika-workflows.mdc",
        CURSOR_RULE,
        "Cursor rule",
    );

    // Copilot instructions
    count += write_if_absent_with_dir(
        project_dir,
        ".github/copilot/nika.instructions.md",
        COPILOT_INSTRUCTIONS,
        "Copilot instructions",
    );

    // Windsurf rule
    count += write_if_absent_with_dir(
        project_dir,
        ".windsurf/rules/nika.md",
        WINDSURF_RULE,
        "Windsurf rule",
    );

    // Roo Code rule
    count += write_if_absent_with_dir(project_dir, ".roo/rules/nika.md", ROO_RULE, "Roo Code rule");

    // Roo Code .roomodes
    count += write_if_absent(
        &project_dir.join(".roomodes"),
        ROOMODES,
        ".roomodes (Roo Code mode)",
    );

    // VS Code extensions.json
    count += write_if_absent_with_dir(
        project_dir,
        ".vscode/extensions.json",
        VSCODE_EXTENSIONS,
        "VS Code recommendations",
    );

    // Git co-author hook
    let git_dir = project_dir.join(".git");
    if git_dir.exists() {
        let hook_path = git_dir.join("hooks/prepare-commit-msg");
        if !hook_path.exists() {
            fs::create_dir_all(git_dir.join("hooks")).ok();
            match fs::write(&hook_path, GIT_HOOK) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).ok();
                    }
                    println!("  {} Git co-author hook", "✓".green());
                    count += 1;
                }
                Err(e) => {
                    println!("  {} Git co-author hook failed: {}", "⚠".yellow(), e);
                }
            }
        }
    }

    println!(
        "\n  {} {} AI integration file(s) created",
        "✓".green(),
        count
    );

    Ok(())
}

fn write_if_absent(path: &Path, content: &str, label: &str) -> usize {
    if path.exists() {
        return 0;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    match fs::write(path, content) {
        Ok(()) => {
            println!("  {} {}", "✓".green(), label);
            1
        }
        Err(e) => {
            println!("  {} {} — {}", "✗".red(), label, e);
            0
        }
    }
}

fn write_if_absent_with_dir(base: &Path, rel_path: &str, content: &str, label: &str) -> usize {
    let path = base.join(rel_path);
    write_if_absent(&path, content, label)
}

// ─── Embedded Content ─────────────────────────────────────────────────────────

const CURSOR_RULE: &str = r#"---
description: "Nika workflow syntax for .nika.yaml files"
globs: "**/*.nika.yaml"
alwaysApply: false
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
schema: "@0.12"
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
schema: "@0.12"               # Required. Always "@0.12"
workflow: my-workflow          # Optional. Defaults to filename
description: "What it does"   # Optional
provider: anthropic            # Default LLM provider for all tasks
model: claude-sonnet-4-20250514  # Default model for all tasks

inputs:                        # Workflow parameters
  topic: "default value"
  count: 5

context:                       # File context bindings
  files:
    readme: ./README.md

imports:                       # Import external workflows
  - path: ./partials/setup.nika.yaml
    prefix: setup_

skills:                        # Prompt augmentation files
  writing: ./skills/writing.md

agents:                        # Reusable agent definitions
  researcher:
    system: "You are a research specialist"
    tools: [perplexity/search]
    max_turns: 15

artifacts:                     # Persist outputs to files
  dir: ./output
  format: markdown
```

## Providers (7 LLM + 1 Local)

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

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | uppercase | trim}}`
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
    guardrails:                        # Output validation
      - type: length
        min_words: 50
        max_words: 500
```

**Short form**: `infer: "Your prompt here"` (string = prompt only)

## Exec Verb (Full Form)

```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true                        # Run via sh -c (default: false)
    cwd: "./frontend"                  # Working directory
    timeout: 60                        # Seconds
    env:
      NODE_ENV: production
      API_URL: "{{inputs.api_url}}"
```

**Short form**: `exec: "echo hello"` (string = command only)

## Fetch Verb (Full Form + Extract)

```yaml
- id: scrape
  fetch:
    url: "https://example.com/article"
    method: GET                        # GET, POST, PUT, DELETE, PATCH
    headers:
      Authorization: "Bearer {{inputs.token}}"
      Content-Type: application/json
    body: '{"query": "test"}'          # String body
    json:                              # JSON body (alternative to body)
      query: "test"
    extract: markdown                  # Post-processing mode
    selector: "main"                   # CSS selector (for extract: text/selector)
    response: full                     # full, binary, or omit for raw body
    follow_redirects: true
    timeout: 30
```

**Short form**: `fetch: "https://api.example.com/data"` (string = URL only)

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

### Response Modes

| Mode | Output |
|------|--------|
| `full` | JSON: `{ status, headers, body, final_url }` |
| `binary` | CAS hash (for media pipeline) |
| *(omit)* | Raw body text (default) |

## Invoke Verb (MCP + Builtin Tools)

```yaml
# MCP tool call (external server)
- id: search
  invoke:
    tool: "novanet/novanet_search"     # server/tool_name format
    params:
      query: "{{with.topic}}"
      limit: 10
    timeout: 30

# Alternative: explicit mcp field
- id: search2
  invoke:
    mcp: novanet                       # MCP server name
    tool: novanet_search               # Tool name only
    params:
      query: "test"

# MCP resource read
- id: read-resource
  invoke:
    resource: "novanet://nodes/latest"

# Builtin nika: tools (no MCP server needed)
- id: resize
  invoke:
    tool: "nika:thumbnail"
    params:
      input: "{{with.photo.media[0].hash}}"
      width: 300
      height: 200
```

**Short form**: `invoke: "nika:dimensions"` (for tools needing no params)

### 24 Builtin Tools (nika:*)

**Always-on**: `nika:import`, `nika:dimensions`, `nika:thumbhash`, `nika:dominant_color`, `nika:pipeline`
**Media core**: `nika:thumbnail`, `nika:convert`, `nika:strip`, `nika:metadata`, `nika:optimize`, `nika:svg_render`
**Opt-in**: `nika:phash`, `nika:compare`, `nika:pdf_extract`, `nika:chart`, `nika:provenance`, `nika:verify`, `nika:qr_validate`, `nika:quality`, `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`

## Agent Verb (Multi-Turn)

```yaml
- id: assistant
  agent:
    system: "You are a research assistant"
    prompt: "Find and analyze {{inputs.topic}}"
    model: claude-sonnet-4-20250514    # Agent-level override
    provider: anthropic                # Agent-level override
    tools:                             # Available tools
      - novanet/novanet_search
      - novanet/novanet_context
    mcp:                               # MCP servers for tool access
      - novanet
      - filesystem
    max_turns: 10                      # Max agentic loop iterations
    max_tokens: 4000                   # Max tokens per response
    temperature: 0.5
    token_budget: 50000                # Total token budget
    extended_thinking: true
    thinking_budget: 5000
    tool_choice: auto                  # auto, required, none
    scope: full                        # full, minimal, debug
    depth_limit: 3                     # Max spawn_agent recursion
    skills:                            # Inject skill prompts
      - writing
    from: researcher                   # Reference agents: definition
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
      - type: regex
        pattern: "^## "
        message: "Must start with heading"
      - type: llm
        judge_prompt: "Is this accurate? Reply PASS or FAIL."
        pass_pattern: "^PASS"
        on_failure: escalate
    completion:                        # Completion behavior
      mode: explicit                   # auto, explicit, pattern
    limits:                            # Cost control
      max_turns: 20
      max_tokens: 100000
      max_cost_usd: 1.0
      max_duration_secs: 300
```

### Guardrail Types

| Type | Description | Key Fields |
|------|-------------|------------|
| `length` | Word/character count bounds | `min_words`, `max_words` |
| `schema` | JSON Schema validation | `json_schema: { ... }` |
| `regex` | Pattern matching | `pattern`, `message` |
| `llm` | Secondary LLM judge | `judge_prompt`, `pass_pattern` |

**on_failure**: `retry` (default), `escalate`, `fail`

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"             # Array expression
    as: item                           # Loop variable (default: "item")
    concurrency: 3                     # Max parallel (default: unlimited)
    fail_fast: true                    # Stop on first error (default: true)
  infer: "Process: {{with.item}}"
```

Access loop variable via `with:` prefix: `{{with.item}}` (same as all bindings)

## Structured Output (JSON Schema)

```yaml
- id: extract
  infer:
    prompt: "Extract entities from: {{with.text}}"
  structured:
    schema:
      type: object
      properties:
        entities:
          type: array
          items:
            type: object
            properties:
              name: { type: string }
              type: { type: string }
            required: [name, type]
      required: [entities]
```

Multi-layer enforcement: tool injection -> rig Extractor -> regex parse -> retry with feedback.

## Vision (Multimodal Content)

```yaml
- id: analyze
  infer:
    content:
      - type: image
        source: "{{with.photo.media[0].hash}}"  # CAS hash -> base64 auto
        detail: high                              # high, low, auto
      - type: text
        text: "Describe this image in detail"
    prompt: "Optional additional prompt"         # Prepended if present
```

Supported: Claude, OpenAI, Mistral, Groq, Gemini, xAI. Not supported: DeepSeek.

## Output Configuration

```yaml
- id: formatted
  infer: "Generate a report"
  output:
    format: json                       # text, json, yaml
    schema:                            # JSON Schema for validation
      type: object
      properties:
        report: { type: string }
    max_retries: 3                     # Retries on validation failure
```

## Retry Configuration

```yaml
- id: flaky-api
  fetch: "https://api.example.com/data"
  retry:
    max_attempts: 3                    # Total attempts
    delay: 2                           # Seconds between retries
    backoff: 2.0                       # Exponential multiplier
```

## Artifact Configuration

```yaml
- id: report
  infer: "Write a report"
  artifact:
    path: "./output/report.md"
    format: markdown                   # markdown, json, text, binary
```

## MCP Server Configuration

```yaml
schema: "@0.12"
workflow: with-mcp

mcp:
  novanet:
    command: cargo
    args: ["run", "-p", "novanet-mcp"]
    env:
      NEO4J_PASSWORD: "{{$env.NEO4J_PASSWORD}}"
    cwd: "../novanet"

  external-api:
    url: "http://localhost:8080/sse"
    transport: sse

  filesystem:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "./data"]

tasks:
  - id: query
    invoke:
      tool: "novanet/novanet_search"
      params:
        query: "test"
```

## Task-Level Overrides

Any task can override workflow-level `provider:` and `model:`:

```yaml
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: cheap-task
    provider: groq
    model: llama-4-maverick
    infer: "Quick classification"

  - id: smart-task
    model: claude-opus-4-20250514
    infer: "Complex reasoning"
```

## Pipeline Patterns

### Two-Task Chain

```yaml
tasks:
  - id: research
    infer: "Research {{inputs.topic}}"

  - id: summarize
    with:
      data: $research
    infer: "Summarize: {{with.data}}"
```

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

### Conditional with Exec

```yaml
tasks:
  - id: check
    exec: "test -f config.json && echo exists || echo missing"

  - id: handle
    with:
      status: $check
    infer: "Config status is: {{with.status}}. What should we do?"
```

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `use: { data: step1 }` | `with: { data: $step1 }` ($ prefix required) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `retry: 3` | `retry: { max_attempts: 3, delay: 2 }` |
| `.yaml` extension | `.nika.yaml` extension |
| Direct Cypher/SQL | Use `invoke:` with MCP tools |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `shell: bash` | `shell: true` (boolean, not shell name) |
| Missing `schema:` line | Always start with `schema: "@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (always array) |

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
```
"#;

const COPILOT_INSTRUCTIONS: &str = r#"---
applyTo: "**/*.nika.yaml"
---

# Nika Workflow Conventions

- Extension: `.nika.yaml` | Schema: `nika/workflow@0.12`
- 5 verbs: infer (LLM), exec (shell), fetch (HTTP), invoke (MCP), agent (loop)
- Bindings: `with: { alias: $task_id }` → `{{with.alias}}`
- Dependencies: `depends_on: [task_id]`
- Timeout values are in seconds
- Validate: `nika check <file>`
"#;

const WINDSURF_RULE: &str = r#"---
trigger: glob
globs: "**/*.nika.yaml"
description: "Nika YAML workflow engine rules"
---

# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs
- `infer:` — LLM generation
- `exec:` — Shell command
- `fetch:` — HTTP request (9 extract modes)
- `invoke:` — MCP tool call (24 builtin nika:* tools)
- `agent:` — Multi-turn autonomous loop

## Syntax
- Bindings: `with: { alias: $task_id }` → `{{with.alias}}`
- DAG: `depends_on: [task_id]`
- Parallel: `for_each: [items]` + `concurrency: N`
- Validate: `nika check <file>`
"#;

const ROO_RULE: &str = r#"---
description: "Nika YAML workflow engine syntax and patterns"
globs: ["*.nika.yaml"]
---

# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs
- `infer:` — LLM generation
- `exec:` — Shell command
- `fetch:` — HTTP request
- `invoke:` — MCP tool call
- `agent:` — Multi-turn loop

## Key Rules
- Bindings: `with: { alias: $task_id }` → `{{with.alias}}`
- DAG ordering: `depends_on: [task_id]`
- Timeout: in seconds
- Zero Cypher: use invoke: for NovaNet, never raw Cypher
- Validate: `nika check <file>`
"#;

const VSCODE_EXTENSIONS: &str = r#"{
  "recommendations": [
    "supernovae-studio.nika-lang",
    "redhat.vscode-yaml"
  ]
}
"#;

const GIT_HOOK: &str = r#"#!/bin/sh
# Nika co-author hook
COMMIT_MSG_FILE=$1
COMMIT_SOURCE=$2
case "$COMMIT_SOURCE" in merge|squash) exit 0 ;; esac
if grep -q "Co-Authored-By:" "$COMMIT_MSG_FILE" 2>/dev/null; then exit 0; fi
if git diff --cached --name-only | grep -q '\.nika\.yaml$'; then
    printf '\n\nCo-Authored-By: Nika 🦋 <nika@supernovae.studio>\n' >> "$COMMIT_MSG_FILE"
fi
"#;

const ROOMODES: &str = r#"{
  "customModes": [
    {
      "slug": "nika",
      "name": "Nika",
      "roleDefinition": "You are a Nika workflow assistant. You understand the 5 verbs (infer, exec, fetch, invoke, agent), schema nika/workflow@0.12, with: bindings, and DAG execution.",
      "description": "Write and debug Nika YAML workflows",
      "groups": [
        "read",
        ["edit", { "fileRegex": "\\.nika\\.yaml$", "description": "Nika workflow files only" }],
        "command",
        "mcp"
      ],
      "source": "project"
    }
  ]
}
"#;

const AGENTS_MD_CONTENT: &str = r#"# Nika

Semantic YAML workflow engine for AI tasks. Schema `nika/workflow@0.12` | [QR Code AI](https://qrcode-ai.com)

## 5 Verbs

| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation |
| `exec:` | Shell command |
| `fetch:` | HTTP request |
| `invoke:` | MCP tool call |
| `agent:` | Multi-turn loop |

## Workflow Syntax

`with:` for bindings, `{{with.alias}}` for templates, `.nika.yaml` extension.

## Integration with NovaNet

Nika connects to NovaNet via MCP only (Zero Cypher rule). Use `invoke:` verb.

## TUI Views

`1/s` Studio | `2/c` Command | `3/x` Control

## Commands

```bash
nika check workflow.nika.yaml    # Validate
nika run workflow.nika.yaml      # Execute
nika ui                          # TUI
nika provider list               # API key status
nika init                        # Interactive project setup (wizard)
nika init --course               # Generate 12-level learning course (44 exercises)
nika init --minimal              # Minimal scaffold (5 workflows, 1 per verb)
nika course status               # Show constellation progress map
nika course next                 # Open next exercise
nika course check [level]        # Validate exercises
nika course hint [exercise]      # Progressive hints (3 tiers)
nika course run <exercise>       # Run a course exercise
nika course info [level]         # Show course/level details
nika course reset <level>        # Reset a level
nika course watch                # Auto-check on file save
nika showcase list               # Browse 200+ showcase workflows
nika showcase extract <name>     # Extract a showcase to current dir
```
"#;

// ═══════════════════════════════════════════════════════════════════════════
// Coherence tests — prevent content drift between AI rules
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// All AI rules MUST use {{with.item}} not {{item}} for for_each.
    #[test]
    fn no_bare_item_template_in_code_examples() {
        let rules: &[(&str, &str)] = &[
            ("CURSOR_RULE", CURSOR_RULE),
            ("COPILOT_INSTRUCTIONS", COPILOT_INSTRUCTIONS),
            ("WINDSURF_RULE", WINDSURF_RULE),
            ("ROO_RULE", ROO_RULE),
            ("AGENTS_MD_CONTENT", AGENTS_MD_CONTENT),
        ];
        for (name, content) in rules {
            for (i, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                // Allow {{item}} in "Wrong" column of mistake tables
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

    /// All main rules must reference schema @0.12.
    #[test]
    fn rules_reference_current_schema() {
        assert!(CURSOR_RULE.contains("@0.12"), "CURSOR_RULE missing @0.12");
        assert!(
            AGENTS_MD_CONTENT.contains("@0.12"),
            "AGENTS_MD missing @0.12"
        );
    }

    /// No rules should reference nonexistent models.
    #[test]
    fn no_nonexistent_models() {
        let rules: &[(&str, &str)] = &[
            ("CURSOR_RULE", CURSOR_RULE),
            ("WINDSURF_RULE", WINDSURF_RULE),
            ("ROO_RULE", ROO_RULE),
        ];
        for (name, content) in rules {
            assert!(
                !content.contains("grok-4"),
                "{} references nonexistent model grok-4",
                name
            );
        }
    }

    /// for_each examples must use {{with.item}} not {{item}}.
    #[test]
    fn for_each_uses_with_prefix() {
        if CURSOR_RULE.contains("as: item") {
            assert!(
                CURSOR_RULE.contains("{{with.item}}"),
                "CURSOR_RULE has for_each as:item but no {{with.item}}"
            );
        }
    }

    /// Tool count consistency (24 builtin tools, not 25 or 26).
    #[test]
    fn consistent_builtin_tool_count() {
        assert!(
            !CURSOR_RULE.contains("26 builtin") && !CURSOR_RULE.contains("25 builtin"),
            "CURSOR_RULE has wrong builtin tool count"
        );
        assert!(
            !WINDSURF_RULE.contains("26 builtin") && !WINDSURF_RULE.contains("25 builtin"),
            "WINDSURF_RULE has wrong builtin tool count"
        );
    }
}
