# Nika Workflow Language Reference

> **Version**: 0.19 Foundation
> **Schema**: `nika/workflow@0.10`

This document is the authoritative reference for the Nika workflow language.

## Table of Contents

1. [Overview](#overview)
2. [Schema Version](#schema-version)
3. [Workflow Structure](#workflow-structure)
4. [Tasks](#tasks)
5. [Verbs (Actions)](#verbs-actions)
6. [Dependencies](#dependencies)
7. [Output Configuration](#output-configuration)
8. [MCP Integration](#mcp-integration)
9. [Template Expressions](#template-expressions)
10. [Error Messages](#error-messages)

---

## Overview

Nika is a semantic YAML workflow engine for multi-step AI workflows. Workflows are declarative specifications that define:

- **What** tasks to execute (verbs)
- **How** tasks depend on each other (flow, use)
- **Where** data flows between tasks (templates)

### Minimal Example

```yaml
schema: "nika/workflow@0.10"
workflow: hello-world

tasks:
  - id: greet
    infer: "Say hello to the world"
```

---

## Schema Version

Every workflow must declare its schema version:

```yaml
schema: "nika/workflow@0.10"
```

### Valid Schema Versions

| Version | Status | Key Features |
|---------|--------|--------------|
| `nika/workflow@0.1` | Deprecated | Basic workflows |
| `nika/workflow@0.2` | Deprecated | MCP integration |
| `nika/workflow@0.3` | Deprecated | Agent verb |
| `nika/workflow@0.4` | Deprecated | Output validation |
| `nika/workflow@0.5` | Deprecated | Decompose |
| `nika/workflow@0.6` | Deprecated | Skills, agents |
| `nika/workflow@0.7` | Deprecated | Retry config |
| `nika/workflow@0.8` | Deprecated | Flow endpoints |
| `nika/workflow@0.9` | Deprecated | Context files |
| `nika/workflow@0.10` | **Current** | Full feature set |

### Schema Validation

Invalid schema versions produce an error with suggestions:

```
error[E003]: invalid schema version 'nika/workfow@0.10'
  --> workflow.yaml:1:9
   |
 1 | schema: "nika/workfow@0.10"
   |         ^^^^^^^^^^^^^^^^^^^
   |
   = help: did you mean 'nika/workflow@0.10'?
```

---

## Workflow Structure

### Top-Level Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schema` | string | **Yes** | Schema version |
| `workflow` | string | No | Workflow name (defaults to filename) |
| `description` | string | No | Human-readable description |
| `provider` | string | No | Default LLM provider |
| `model` | string | No | Default model |
| `mcp` | object | No | MCP server configurations |
| `context` | object | No | Context file loading |
| `inputs` | object | No | Input parameters with defaults |
| `tasks` | array | **Yes** | Task definitions |
| `flows` | array | No | Explicit flow definitions |

### Full Example

```yaml
schema: "nika/workflow@0.10"
workflow: code-review
description: "Automated code review workflow"
provider: claude
model: claude-sonnet-4-6

mcp:
  servers:
    novanet:
      command: "cargo run -p novanet-mcp"

context:
  files:
    code: "./src/**/*.rs"

inputs:
  max_issues:
    default: 10
    type: number

tasks:
  - id: analyze
    infer: "Analyze the code for issues"

  - id: report
    infer: "Generate a report of {{use.analyze}}"
    flow: analyze
```

---

## Tasks

Tasks are the fundamental unit of execution.

### Task Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | **Yes** | Unique identifier |
| `description` | string | No | Human-readable description |
| `provider` | string | No | Override workflow provider |
| `model` | string | No | Override workflow model |
| `use` | object | No | Data dependencies |
| `flow` | string/array | No | Execution dependencies |
| `output` | object | No | Output configuration |
| `retry` | object | No | Retry configuration |

### Task ID Rules

- Must be unique within the workflow
- Valid characters: `a-z`, `A-Z`, `0-9`, `-`, `_`
- Cannot start with a number
- Case-sensitive

```yaml
# Valid IDs
- id: my-task
- id: task_1
- id: analyzeCode

# Invalid IDs
- id: 1-task     # starts with number
- id: my task    # contains space
```

### Duplicate Task Detection

```
error[E002]: duplicate task id 'process'
  --> workflow.yaml:15:7
   |
15 |   - id: process
   |         ^^^^^^^
   |
   = note: first defined at workflow.yaml:8:7
```

---

## Verbs (Actions)

Each task performs exactly one action using a "verb". There are 5 verbs:

### 1. `infer` - LLM Inference

Send a prompt to an LLM and get a response.

```yaml
# Shorthand
- id: simple
  infer: "Summarize this document"

# Full form
- id: detailed
  infer:
    prompt: "Summarize this document"
    system: "You are a technical writer"
    temperature: 0.7
    max_tokens: 1000
    thinking: true
    thinking_budget: 4096
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `prompt` | string | **Required** | The prompt text |
| `system` | string | None | System prompt override |
| `temperature` | float | 1.0 | Sampling temperature (0.0-2.0) |
| `max_tokens` | int | None | Maximum response tokens |
| `thinking` | bool | false | Enable extended thinking |
| `thinking_budget` | int | None | Thinking token budget |

### 2. `exec` - Shell Command

Execute a shell command.

```yaml
# Shorthand
- id: build
  exec: "cargo build --release"

# Full form
- id: test
  exec:
    command: "npm test"
    shell: true
    working_dir: "./frontend"
    env:
      NODE_ENV: test
    timeout_ms: 60000
    capture_stdout: true
    capture_stderr: true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `command` | string | **Required** | Command to execute |
| `shell` | bool | false | Run through shell |
| `working_dir` | string | None | Working directory |
| `env` | object | None | Environment variables |
| `timeout_ms` | int | None | Timeout in milliseconds |
| `capture_stdout` | bool | true | Capture stdout |
| `capture_stderr` | bool | true | Capture stderr |

### 3. `fetch` - HTTP Request

Make an HTTP request.

```yaml
# Shorthand (GET)
- id: get-data
  fetch: "https://api.example.com/data"

# Full form
- id: post-data
  fetch:
    url: "https://api.example.com/submit"
    method: POST
    headers:
      Authorization: "Bearer {{env.API_KEY}}"
      Content-Type: application/json
    json:
      name: "{{inputs.name}}"
    timeout_ms: 30000
    follow_redirects: true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `url` | string | **Required** | URL to fetch |
| `method` | string | GET | HTTP method |
| `headers` | object | None | HTTP headers |
| `body` | string | None | Request body (text) |
| `json` | object | None | Request body (JSON) |
| `timeout_ms` | int | None | Timeout in milliseconds |
| `follow_redirects` | bool | true | Follow redirects |

### 4. `invoke` - MCP Tool

Call an MCP (Model Context Protocol) tool.

```yaml
# Shorthand
- id: query
  invoke: novanet::query

# With parameters
- id: search
  invoke:
    tool: novanet::search
    params:
      query: "{{use.keywords}}"
      limit: 10
    timeout_ms: 5000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tool` | string | **Required** | Tool name (`server::tool` or `tool`) |
| `params` | object | None | Tool parameters |
| `mcp` | string | None | MCP server (if not in tool name) |
| `timeout_ms` | int | None | Timeout in milliseconds |

### 5. `agent` - Autonomous Agent

Run an autonomous agent with tools.

```yaml
- id: researcher
  agent:
    goal: "Research the topic and compile findings"
    tools:
      - web_search
      - read_file
      - write_file
    max_iterations: 10
    max_tokens: 4096
    skills:
      - research-methodology
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `goal` | string | **Required** | Agent's objective |
| `tools` | array | None | Available tools |
| `max_iterations` | int | None | Maximum iterations |
| `max_tokens` | int | None | Max tokens per response |
| `from` | string | None | Agent definition reference |
| `skills` | array | None | Skills to inject |

---

## Dependencies

Tasks can depend on other tasks in two ways:

### `use:` - Data Dependencies

Access output from another task:

```yaml
tasks:
  - id: analyze
    infer: "Analyze this code"

  - id: report
    use:
      analysis: analyze
    infer: "Create report from {{use.analysis}}"
```

With JSONPath extraction:

```yaml
- id: extract
  use:
    items:
      task: parse-json
      path: "$.data.items"
  infer: "Process these items: {{use.items}}"
```

### `flow:` - Execution Dependencies

Ensure a task runs after another (without using its output):

```yaml
tasks:
  - id: setup
    exec: "npm install"

  - id: build
    flow: setup
    exec: "npm run build"

  - id: test
    flow: [setup, build]
    exec: "npm test"
```

### Dependency Validation

Unknown task references produce helpful errors:

```
error[E001]: unknown task 'analize'
  --> workflow.yaml:12:14
   |
12 |       data: analize
   |             ^^^^^^^
   |
   = help: did you mean 'analyze'?
```

### Cycle Detection

Circular dependencies are detected:

```
error[E004]: cyclic dependency detected: task1 → task2 → task3 → task1
  --> workflow.yaml:8:5
   |
 8 |   - id: task1
   |     ^^^^^^^^^
```

---

## Output Configuration

Control task output format and validation:

```yaml
- id: structured
  infer: "Generate a list of items"
  output:
    format: json
    schema:
      type: array
      items:
        type: object
        properties:
          name: { type: string }
          score: { type: number }
        required: [name, score]
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `format` | string | text | Output format: `text`, `json`, `yaml` |
| `schema` | object | None | JSON Schema for validation |
| `schema_ref` | string | None | Path to schema file |

---

## MCP Integration

Configure MCP servers for tool invocation:

```yaml
mcp:
  servers:
    novanet:
      command: "cargo run -p novanet-mcp"
      args: ["--verbose"]
      env:
        NEO4J_URI: "bolt://localhost:7687"
      cwd: "../novanet"

    external:
      url: "http://localhost:8080"
      transport: sse
```

### Server Configuration

| Field | Type | Description |
|-------|------|-------------|
| `command` | string | Command to spawn (stdio) |
| `args` | array | Command arguments |
| `env` | object | Environment variables |
| `cwd` | string | Working directory |
| `url` | string | Server URL (SSE) |
| `transport` | string | Transport type: `stdio` (default) or `sse` |

---

## Template Expressions

Templates use `{{...}}` syntax for variable interpolation:

### Available Variables

| Variable | Description |
|----------|-------------|
| `{{use.alias}}` | Output from a `use:` dependency |
| `{{inputs.name}}` | Input parameter value |
| `{{env.VAR}}` | Environment variable |
| `{{context.alias}}` | Loaded context file |

### Examples

```yaml
- id: process
  use:
    data: fetch-data
  infer: |
    Process this data: {{use.data}}

    Configuration:
    - API Key: {{env.API_KEY}}
    - Max items: {{inputs.max_items}}
```

---

## Error Messages

Nika provides detailed error messages with:

- **Error code** (E001-E009)
- **Source location** (file:line:column)
- **Contextual suggestions** ("did you mean?")
- **Related notes** (first definition location, etc.)

### Error Codes

| Code | Error | Description |
|------|-------|-------------|
| E001 | UnknownTask | Reference to undefined task |
| E002 | DuplicateTask | Task ID defined multiple times |
| E003 | InvalidSchema | Invalid schema version |
| E004 | CyclicDependency | Circular dependency detected |
| E005 | InvalidValue | Invalid field value |
| E006 | MissingField | Required field missing |
| E007 | InvalidTemplate | Template syntax error |
| E008 | UnknownFlow | Reference to undefined flow |
| E009 | UnknownMcpServer | Reference to undefined MCP server |

---

## Appendix: Grammar Summary

```
workflow      ::= schema [metadata] tasks [flows]
schema        ::= "schema:" string
metadata      ::= [workflow] [description] [provider] [model] [mcp] [context] [inputs]
tasks         ::= "tasks:" task+
task          ::= "- id:" id [description] verb [use] [flow] [output] [retry]
verb          ::= infer | exec | fetch | invoke | agent
infer         ::= "infer:" (string | infer_params)
exec          ::= "exec:" (string | exec_params)
fetch         ::= "fetch:" (string | fetch_params)
invoke        ::= "invoke:" (string | invoke_params)
agent         ::= "agent:" agent_params
use           ::= "use:" { alias: target }+
target        ::= id | { task: id, path: jsonpath }
flow          ::= "flow:" (id | id+)
output        ::= "output:" { format: format [schema: schema] }
```

---

*Last updated: v0.19 Foundation*
