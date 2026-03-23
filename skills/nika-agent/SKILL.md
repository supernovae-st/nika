---
name: nika-agent
description: >-
  Expert at the Nika agent: verb for multi-turn autonomous loops in .nika.yaml
  workflows. Covers tool selection (nika_read, nika_write, nika_glob, nika_grep,
  nika_complete), guardrails, max_turns, completion signals, provider config,
  and multi-agent patterns. Use when building agent: tasks in Nika YAML
  workflows (schema nika/workflow@0.12).
---

# Nika agent: Verb Expert

The `agent:` verb creates a multi-turn autonomous loop where an LLM iteratively calls tools until a completion signal.

## Basic Syntax

```yaml
- id: researcher
  agent:
    prompt: "Research topic X and write a summary"
    tools: [nika_read, nika_glob, nika_complete]
    max_turns: 10
    provider: openai
    model: gpt-4.1
    max_tokens: 2000
```

## Full Field Reference

```yaml
- id: agent_task
  agent:
    prompt: "Detailed instructions for the agent"
    tools:                          # List of available tools
      - nika_read                   # Read files
      - nika_write                  # Write files
      - nika_glob                   # Find files by pattern
      - nika_grep                   # Search file contents
      - nika_edit                   # Edit files
      - nika_complete               # Signal completion (required)
    max_turns: 10                   # Max tool-call rounds
    provider: openai                # LLM provider
    model: gpt-4.1                  # Model
    max_tokens: 2000                # Max tokens per response
    system: "You are a senior engineer" # System prompt
    temperature: 0.3
    guardrails:
      blocked_tools: [nika_write]   # Prevent specific tools
    completion:
      signal: nika_complete         # Tool that ends the loop
  timeout: 120                      # Total timeout in seconds
```

## Built-in Tools

### Core Tools (12)

| Tool | Description |
|------|-------------|
| `nika_complete` | Signal task completion (always include this) |
| `nika_read` | Read file contents |
| `nika_write` | Write file contents |
| `nika_edit` | Edit file (find & replace) |
| `nika_glob` | Find files by glob pattern |
| `nika_grep` | Search file contents with regex |
| `nika_log` | Log a message |
| `nika_import` | Import file into CAS |
| `nika_dimensions` | Get image dimensions |
| `nika_thumbhash` | Generate image placeholder |
| `nika_dominant_color` | Extract color palette |
| `nika_pipeline` | Chain media operations |

### Media Tools (available with features)

`nika_thumbnail`, `nika_convert`, `nika_strip`, `nika_metadata`, `nika_optimize`, `nika_svg_render`, `nika_phash`, `nika_compare`, `nika_pdf_extract`, `nika_chart`, `nika_provenance`, `nika_verify`, `nika_qr_validate`, `nika_quality`

### MCP Tools

When `mcp:` servers are configured, their tools are also available:

```yaml
mcp:
  novanet:
    command: novanet
    args: ["mcp", "serve"]
tasks:
  - id: agent
    agent:
      prompt: "Query the knowledge graph"
      tools: [nika_complete, novanet_search]   # MCP tools auto-prefixed
      max_turns: 5
```

## Patterns

### Research Agent

```yaml
- id: research
  agent:
    prompt: |
      Research the topic "{{inputs.topic}}" by:
      1. Find relevant files with nika_glob
      2. Read key files with nika_read
      3. Search for specific patterns with nika_grep
      4. When you have enough information, call nika_complete with a summary
    tools: [nika_glob, nika_read, nika_grep, nika_complete]
    max_turns: 15
    provider: openai
    model: gpt-4.1
```

### Code Review Agent

```yaml
- id: review
  agent:
    prompt: |
      Review the code in the current directory:
      1. Find source files with nika_glob
      2. Read each file with nika_read
      3. Search for common issues with nika_grep
      4. Call nika_complete with your review
    tools: [nika_glob, nika_read, nika_grep, nika_complete]
    max_turns: 20
    provider: claude
    model: claude-sonnet-4-20250514
    max_tokens: 4000
    system: "You are a senior code reviewer. Be thorough but constructive."
```

### File Processing Agent

```yaml
- id: processor
  agent:
    prompt: |
      Process all .json files in ./data/:
      1. Glob for *.json files
      2. Read each file
      3. Transform the data
      4. Write results to ./output/
      5. Call nika_complete when done
    tools: [nika_glob, nika_read, nika_write, nika_complete]
    max_turns: 30
    guardrails:
      blocked_tools: []              # Allow all tools
```

### Agent with Structured Output

```yaml
- id: analyze
  agent:
    prompt: "Analyze the codebase and return structured findings"
    tools: [nika_glob, nika_read, nika_grep, nika_complete]
    max_turns: 15
    provider: openai
    model: gpt-4.1
  structured:
    schema:
      type: object
      properties:
        files_analyzed: { type: number }
        issues:
          type: array
          items:
            type: object
            properties:
              file: { type: string }
              line: { type: number }
              severity: { type: string }
              message: { type: string }
            required: [file, severity, message]
      required: [files_analyzed, issues]
```

### Multi-Agent Pipeline

```yaml
tasks:
  - id: scout
    agent:
      prompt: "Find all TODO items in the codebase"
      tools: [nika_glob, nika_grep, nika_complete]
      max_turns: 10
      provider: openai
      model: gpt-4.1-mini

  - id: analyzer
    depends_on: [scout]
    with:
      todos: $scout
    agent:
      prompt: |
        Prioritize these TODOs and write a plan:
        {{with.todos}}
      tools: [nika_read, nika_write, nika_complete]
      max_turns: 10
      provider: claude
      model: claude-sonnet-4-20250514
```

## Completion Signal

The agent loop ends when:
1. The LLM calls `nika_complete` (preferred)
2. `max_turns` is reached (fallback)
3. `timeout` is exceeded (safety)

Always include `nika_complete` in the tools list. The agent's final output is the value passed to `nika_complete`.

## Guardrails

Restrict what the agent can do:

```yaml
guardrails:
  blocked_tools: [nika_write, nika_edit]   # Read-only agent
```

NIKA-112 error is raised if the agent attempts a blocked tool.

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| Missing `nika_complete` in tools | Always include it as the completion signal |
| `max_turns: 1` | Too low; agent needs multiple turns to work |
| No `max_turns:` at all | Always set a limit to prevent runaway loops |
| No `timeout:` | Set timeout for safety (default: 300s) |
| Provider in wrong location | `provider:` goes inside `agent:` block |
| Tools as string | `tools:` must be a YAML list: `[tool1, tool2]` |
| Expecting `tools: [builtin]` to include file tools | List each tool explicitly |

## Validation

```bash
nika check workflow.nika.yaml    # Validates agent config
nika run workflow.nika.yaml      # Test with real LLM
```
