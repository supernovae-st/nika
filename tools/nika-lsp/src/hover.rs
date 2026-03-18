//! Hover documentation provider for Nika workflows.
//!
//! Provides inline documentation when hovering over:
//! - Verbs (infer, exec, fetch, invoke, agent)
//! - Task IDs
//! - Schema versions
//! - Parameters

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::document::DocumentState;

/// Get hover documentation for the given position.
pub fn get_hover(doc: &DocumentState, position: Position) -> Option<Hover> {
    let content = doc.content();
    let lines: Vec<&str> = content.lines().collect();

    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let col = position.character as usize;

    // Extract the word at the cursor position
    let word = extract_word_at_position(line, col)?;

    // Get documentation for the word
    let docs = get_word_documentation(word, line)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: docs,
        }),
        range: None,
    })
}

/// Extract the word at a given position in a line.
fn extract_word_at_position(line: &str, col: usize) -> Option<&str> {
    if col >= line.len() {
        return None;
    }

    let chars: Vec<char> = line.chars().collect();

    // Find word boundaries
    let mut start = col;
    while start > 0 && is_word_char(chars.get(start - 1).copied()?) {
        start -= 1;
    }

    let mut end = col;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    // Convert char indices to byte indices
    let byte_start: usize = line.chars().take(start).map(|c| c.len_utf8()).sum();
    let byte_end: usize = line.chars().take(end).map(|c| c.len_utf8()).sum();

    Some(&line[byte_start..byte_end])
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '@' || c == '/' || c == '.'
}

/// Get documentation for a word based on context.
fn get_word_documentation(word: &str, line: &str) -> Option<String> {
    // Check for verb keywords
    match word {
        "infer" => Some(INFER_DOCS.to_string()),
        "exec" => Some(EXEC_DOCS.to_string()),
        "fetch" => Some(FETCH_DOCS.to_string()),
        "invoke" => Some(INVOKE_DOCS.to_string()),
        "agent" => Some(AGENT_DOCS.to_string()),
        "schema" if line.contains("schema:") => Some(SCHEMA_DOCS.to_string()),
        "workflow" if line.contains("workflow:") => Some(WORKFLOW_DOCS.to_string()),
        "tasks" => Some(TASKS_DOCS.to_string()),
        "flows" => Some(FLOWS_DOCS.to_string()),
        "use" => Some(USE_DOCS.to_string()),
        "for_each" => Some(FOR_EACH_DOCS.to_string()),
        "retry" => Some(RETRY_DOCS.to_string()),
        "mcp" => Some(MCP_DOCS.to_string()),
        "context" => Some(CONTEXT_DOCS.to_string()),
        "include" => Some(INCLUDE_DOCS.to_string()),
        _ if word.starts_with("nika/workflow@") => Some(format!(
            "## Schema Version\n\n`{}`\n\nNika workflow schema version.",
            word
        )),
        _ => None,
    }
}

const INFER_DOCS: &str = r#"## infer: verb

**LLM text generation**

Generate text using a language model.

### Shorthand
```yaml
infer: "Generate a headline for QR Code AI"
```

### Full form
```yaml
infer:
  prompt: "Generate content"
  temperature: 0.7      # 0.0-1.0 (creativity)
  system: "You are a marketing expert"
  max_tokens: 100
  model: claude-sonnet-4-20250514
```

### Extended Thinking (Claude only)
```yaml
infer:
  prompt: "Analyze this complex problem"
  extended_thinking: true
  thinking_budget: 16384
```

**Error codes:** NIKA-030 (provider error), NIKA-031 (model not found)"#;

const EXEC_DOCS: &str = r#"## exec: verb

**Shell command execution**

Execute a shell command.

### Shorthand
```yaml
exec: "npm run build"
```

### Full form
```yaml
exec:
  command: "npm run build"
  shell: true    # Required for pipes/redirects
  timeout: 30    # Seconds
```

### Security
- Default: `shell: false` (safe, no injection)
- Set `shell: true` only for pipes/redirects
- Blocked commands: `rm -rf /`, `sudo`, etc.

**Error codes:** NIKA-050 (exec failed), NIKA-053 (blocked command)"#;

const FETCH_DOCS: &str = r#"## fetch: verb

**HTTP request**

Make an HTTP request to an external URL.

### Example
```yaml
fetch:
  url: "https://api.example.com/data"
  method: GET              # GET, POST, PUT, DELETE, PATCH, HEAD
  headers:
    Authorization: "Bearer $TOKEN"
  body: '{"query": "test"}'
  timeout: 30
```

### Response
The response body is available in the task output.

**Error codes:** NIKA-070 (fetch failed), NIKA-071 (timeout)"#;

const INVOKE_DOCS: &str = r#"## invoke: verb

**MCP tool call**

Call a tool from an MCP server.

### Example
```yaml
invoke:
  mcp: novanet           # MCP server name
  tool: novanet_generate # Tool name
  params:
    entity: "qr-code"
    locale: "fr-FR"
```

### Requires
Define MCP servers in the workflow:
```yaml
mcp:
  servers:
    novanet:
      command: "node"
      args: ["dist/index.js"]
```

**Error codes:** NIKA-100 (MCP not connected), NIKA-101 (tool not found)"#;

const AGENT_DOCS: &str = r#"## agent: verb

**Multi-turn agentic loop**

Run an autonomous agent that can use tools.

### Example
```yaml
agent:
  prompt: "Research AI trends and write a summary"
  mcp: [novanet, perplexity]
  max_turns: 10
  depth_limit: 3        # For nested agents
  tools: [nika:read, nika:write]
```

### Stop conditions
```yaml
agent:
  prompt: "Generate until complete"
  stop_conditions:
    - contains: "DONE"
    - max_tokens: 5000
```

**Error codes:** NIKA-110 (agent error), NIKA-111 (depth exceeded)"#;

const SCHEMA_DOCS: &str = r#"## schema

**Workflow schema version**

Declares the Nika workflow schema version.

### Example
```yaml
schema: "nika/workflow@0.12"
```

### Versions
| Version | Features |
|---------|----------|
| @0.12 | Latest - flows: removed, depends_on: replaces it |
| @0.10 | structured output, artifacts |
| @0.9 | context: + include: |
| @0.5 | decompose, lazy bindings |
| @0.3 | for_each parallelism |
| @0.2 | invoke: + agent: |
| @0.1 | infer, exec, fetch |

**Required field** - must be first line."#;

const WORKFLOW_DOCS: &str = r#"## workflow

**Workflow name**

Unique identifier for this workflow.

### Example
```yaml
workflow: deploy-production
```

Used in:
- Trace files
- Logging
- UI display"#;

const TASKS_DOCS: &str = r#"## tasks

**Task list**

Array of tasks to execute.

### Example
```yaml
tasks:
  - id: step1
    infer: "Generate content"

  - id: step2
    use:
      content: step1
    exec: "echo {{use.content}}"
```

Each task must have:
- `id`: Unique identifier
- One verb: `infer`, `exec`, `fetch`, `invoke`, or `agent`"#;

const FLOWS_DOCS: &str = r#"## flows (removed in @0.12)

**Deprecated** -- use `depends_on:` or `use:` bindings instead.

### Migration
```yaml
# Before (@0.11 and earlier)
flows:
  - source: step1
    target: step2

# After (@0.12)
- id: step2
  depends_on: [step1]    # Explicit dependency
  use:
    data: step1           # Implicit dependency via binding
```

Dependencies are now expressed directly on each task via
`depends_on:` (explicit ordering) or `use:` bindings
(implicit data dependency)."#;

const USE_DOCS: &str = r#"## use

**Data binding**

Reference outputs from other tasks.

### Example
```yaml
tasks:
  - id: step1
    infer: "Generate data"

  - id: step2
    use:
      data: step1           # Bind step1 output to 'data'
      config: step1.field   # JSONPath access
    infer: "Process {{use.data}}"
```

### Lazy bindings
```yaml
use:
  data:
    path: future_task
    lazy: true
    default: "{}"
```"#;

const FOR_EACH_DOCS: &str = r#"## for_each

**Parallel iteration**

Execute task for each item in an array.

### Example
```yaml
tasks:
  - id: generate_pages
    for_each: ["fr-FR", "en-US", "de-DE"]
    as: locale
    concurrency: 3
    fail_fast: true
    infer: "Generate for {{use.locale}}"
```

### Binding reference
```yaml
for_each: "$locales"          # Reference from use:
for_each: "{{use.items}}"     # Template syntax
```

**Requires:** schema @0.3+"#;

const RETRY_DOCS: &str = r#"## retry

**Retry configuration**

Retry failed tasks automatically.

### Example
```yaml
tasks:
  - id: api_call
    retry:
      max_attempts: 3
      delay: 1000           # ms
      backoff: exponential  # or linear
    fetch:
      url: "https://api.example.com"
```"#;

const MCP_DOCS: &str = r#"## mcp

**MCP server configuration**

Define MCP servers for invoke: and agent: tasks.

### Example
```yaml
mcp:
  servers:
    novanet:
      command: "node"
      args: ["dist/index.js"]
      env:
        API_KEY: "$NOVANET_API_KEY"

    perplexity:
      url: "http://localhost:3000/sse"
      transport: sse
```"#;

const CONTEXT_DOCS: &str = r#"## context

**Context file loading** (schema @0.9+)

Load files at workflow start.

### Example
```yaml
context:
  files:
    brand: ./context/brand.md
    config: ./context/config.json
    examples: ./context/*.md    # Glob pattern
  session: .nika/sessions/prev.json
```

Access via `{{context.files.brand}}`"#;

const INCLUDE_DOCS: &str = r#"## include

**DAG fusion** (schema @0.9+)

Merge tasks from external workflows.

### Example
```yaml
include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_

  - path: ./partials/teardown.nika.yaml
```

Tasks are prefixed to avoid ID collisions."#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_word() {
        assert_eq!(extract_word_at_position("hello world", 0), Some("hello"));
        assert_eq!(extract_word_at_position("hello world", 6), Some("world"));
        assert_eq!(extract_word_at_position("  infer:", 3), Some("infer"));
    }

    #[test]
    fn test_get_documentation() {
        assert!(get_word_documentation("infer", "infer:").is_some());
        assert!(get_word_documentation("exec", "exec: npm").is_some());
        assert!(get_word_documentation("random_word", "random").is_none());
    }

    #[test]
    fn test_hover_on_verb() {
        let doc = DocumentState::new("tasks:\n  - id: step1\n    infer: \"Hello\"".to_string(), 1);
        let hover = get_hover(
            &doc,
            Position {
                line: 2,
                character: 5,
            },
        );

        assert!(hover.is_some());
        if let HoverContents::Markup(markup) = hover.unwrap().contents {
            assert!(markup.value.contains("LLM text generation"));
        }
    }
}
