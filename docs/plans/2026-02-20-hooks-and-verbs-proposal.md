# Nika v0.6 Proposal: Hooks and New Verbs

**Date:** 2026-02-20
**Status:** Draft
**Author:** Claude + Thibaut

---

## Executive Summary

This document proposes two major enhancements for Nika v0.6:

1. **Hooks System** - Workflow-level event handlers (pre/post task, pre/post workflow)
2. **New Verbs** - `think:`, `browse:`, `test:` for specialized task types

---

## Part 1: Hooks System

### Motivation

Claude Code's hook system allows users to inject custom behavior at key points in the workflow. Nika should support similar capabilities for:

- **Security checks** before sensitive operations
- **Logging/auditing** after each task
- **Validation gates** before deployment tasks
- **Resource cleanup** after workflow completion

### Proposed YAML Syntax

```yaml
schema: "nika/workflow@0.6"
workflow: secure-deployment

hooks:
  # Run before any task starts
  pre_task:
    - matcher: "exec:*deploy*"        # Task ID pattern (glob)
      command: "./scripts/security-check.sh"
      fail_on_error: true             # Block task if hook fails

    - matcher: "*"                    # All tasks
      log: true                       # Built-in logging

  # Run after any task completes
  post_task:
    - matcher: "infer:*"              # All infer tasks
      command: "./scripts/log-llm-usage.sh {{task.id}} {{task.tokens}}"

    - matcher: "agent:*"              # All agent tasks
      validate:
        max_tokens: 100000            # Fail if exceeded
        require_stop_condition: true  # Must hit stop condition

  # Run once before workflow starts
  pre_workflow:
    - command: "./scripts/validate-env.sh"
      fail_on_error: true

  # Run once after workflow completes (success or failure)
  post_workflow:
    - command: "./scripts/cleanup.sh"
      always_run: true                # Run even on failure

tasks:
  - id: deploy_prod
    exec: "kubectl apply -f deployment.yaml"
```

### Hook Types

| Hook | When | Use Case |
|------|------|----------|
| `pre_task` | Before each task | Security checks, validation gates |
| `post_task` | After each task | Logging, metrics, cleanup |
| `pre_workflow` | Before first task | Environment validation |
| `post_workflow` | After last task | Cleanup, notifications |

### Hook Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `matcher` | string | `"*"` | Glob pattern for task IDs or verb:pattern |
| `command` | string | - | Shell command to execute |
| `log` | boolean | false | Built-in event logging |
| `validate` | object | - | Validation rules |
| `fail_on_error` | boolean | false | Block task on hook failure |
| `always_run` | boolean | false | Run even if workflow fails |
| `timeout` | integer | 30000 | Timeout in milliseconds |

### Template Variables

Hooks can access task context via templates:

| Variable | Description |
|----------|-------------|
| `{{task.id}}` | Current task ID |
| `{{task.verb}}` | Task verb (infer, exec, etc.) |
| `{{task.status}}` | Task status (for post_task) |
| `{{task.tokens}}` | Token usage (for infer/agent tasks) |
| `{{task.duration_ms}}` | Execution time |
| `{{workflow.id}}` | Workflow ID |
| `{{workflow.status}}` | Workflow status (for post_workflow) |

### Rust Implementation Sketch

```rust
// ast/hooks.rs
#[derive(Debug, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub pre_task: Vec<HookSpec>,
    #[serde(default)]
    pub post_task: Vec<HookSpec>,
    #[serde(default)]
    pub pre_workflow: Vec<HookSpec>,
    #[serde(default)]
    pub post_workflow: Vec<HookSpec>,
}

#[derive(Debug, Deserialize)]
pub struct HookSpec {
    #[serde(default = "default_matcher")]
    pub matcher: String,
    pub command: Option<String>,
    #[serde(default)]
    pub log: bool,
    pub validate: Option<ValidationSpec>,
    #[serde(default)]
    pub fail_on_error: bool,
    #[serde(default)]
    pub always_run: bool,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

// In Workflow struct
pub struct Workflow {
    // ... existing fields ...
    pub hooks: Option<HooksConfig>,
}
```

### JSON Schema Addition

```json
{
  "hooks": {
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "pre_task": {
        "type": "array",
        "items": { "$ref": "#/$defs/HookSpec" }
      },
      "post_task": {
        "type": "array",
        "items": { "$ref": "#/$defs/HookSpec" }
      },
      "pre_workflow": {
        "type": "array",
        "items": { "$ref": "#/$defs/HookSpec" }
      },
      "post_workflow": {
        "type": "array",
        "items": { "$ref": "#/$defs/HookSpec" }
      }
    }
  },
  "$defs": {
    "HookSpec": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "matcher": { "type": "string", "default": "*" },
        "command": { "type": "string" },
        "log": { "type": "boolean", "default": false },
        "validate": { "$ref": "#/$defs/ValidationSpec" },
        "fail_on_error": { "type": "boolean", "default": false },
        "always_run": { "type": "boolean", "default": false },
        "timeout": { "type": "integer", "default": 30000 }
      }
    }
  }
}
```

---

## Part 2: New Verbs

### Current State (5 Verbs)

| Verb | Purpose |
|------|---------|
| `infer:` | One-shot LLM text generation |
| `exec:` | Shell command execution |
| `fetch:` | HTTP requests |
| `invoke:` | MCP tool calls |
| `agent:` | Multi-turn agentic loops |

### Proposed New Verbs

#### 1. `think:` - Sequential Reasoning

**Purpose:** Structured step-by-step reasoning using sequential-thinking MCP.

**Why a new verb?**
- `infer:` is single-shot, no iterative thinking
- `agent:` is overkill for pure reasoning (no tool calls needed)
- `think:` is optimized for chain-of-thought without external tools

**Proposed Syntax:**

```yaml
- id: analyze_trade_offs
  think:
    prompt: "Analyze trade-offs between Redis and PostgreSQL for caching"
    steps: 5                          # Estimated reasoning steps
    style: analytical                 # analytical | creative | critical
    mcp: sequential-thinking          # MCP server to use
    output:
      format: json
      schema: analysis.schema.json
```

**How it differs from `infer:`:**

| Aspect | `infer:` | `think:` |
|--------|----------|----------|
| Reasoning | Single-shot | Multi-step chain |
| MCP | None | sequential-thinking |
| Thought visibility | Hidden | Captured in events |
| Use case | Generation | Analysis, planning |

**Implementation:**
- Uses `sequentialthinking` tool from `@modelcontextprotocol/server-sequential-thinking`
- Emits `ThinkingStep` events for each reasoning step
- Captures intermediate thoughts in trace

**Example Use Cases:**
- Architecture decisions
- Code review analysis
- Bug root cause analysis
- Trade-off evaluation

---

#### 2. `browse:` - Browser Automation

**Purpose:** Headless browser automation via Playwright MCP.

**Why a new verb?**
- `fetch:` can't execute JavaScript
- `invoke:` requires manual tool orchestration
- `browse:` provides declarative browser actions

**Proposed Syntax:**

```yaml
- id: capture_landing_page
  browse:
    url: "https://example.com"
    viewport:
      width: 1280
      height: 720
    mobile: false
    actions:
      - wait: 2000                    # Wait ms
      - click: "#accept-cookies"      # Click selector
      - scroll: "bottom"              # Scroll direction
      - fill:
          selector: "#email"
          value: "test@example.com"
      - screenshot: "landing.png"     # Take screenshot
      - evaluate: "document.title"    # Run JS
    mcp: playwright                   # MCP server to use
```

**Actions DSL:**

| Action | Description | Example |
|--------|-------------|---------|
| `wait` | Wait milliseconds | `wait: 2000` |
| `click` | Click element | `click: "#button"` |
| `fill` | Fill input | `fill: { selector: "#input", value: "text" }` |
| `scroll` | Scroll page | `scroll: "bottom"` or `scroll: 500` |
| `screenshot` | Capture screen | `screenshot: "output.png"` |
| `evaluate` | Run JavaScript | `evaluate: "document.title"` |
| `assert` | Assert condition | `assert: { selector: ".success", visible: true }` |

**Implementation:**
- Uses Playwright MCP (`@playwright/mcp@latest`)
- Translates DSL to `browser_*` tool calls
- Emits `BrowserAction` events for observability

**Example Use Cases:**
- E2E testing
- Screenshot capture for QA
- Form submission testing
- JavaScript-rendered page scraping

---

#### 3. `test:` - Automated Testing

**Purpose:** Run test suites with structured result reporting.

**Why a new verb?**
- `exec:` returns raw output, not structured test results
- `test:` parses test output into pass/fail/skip counts
- Native retry and flaky test handling

**Proposed Syntax:**

```yaml
- id: run_unit_tests
  test:
    runner: cargo                     # cargo | npm | pytest | playwright
    command: "cargo nextest run"      # Full command
    pattern: "tests/**/*.rs"          # Test file pattern
    parallel: true                    # Run tests in parallel
    timeout: 300000                   # 5 min timeout
    retry:
      attempts: 2                     # Retry failing tests
      flaky_threshold: 0.5            # Mark flaky if fails >50%
    coverage:
      enabled: true
      threshold: 80                   # Fail if coverage < 80%
```

**Output Structure:**

```json
{
  "runner": "cargo",
  "status": "passed",
  "duration_ms": 12345,
  "summary": {
    "total": 100,
    "passed": 98,
    "failed": 1,
    "skipped": 1,
    "flaky": 0
  },
  "failures": [
    {
      "name": "test_edge_case",
      "file": "src/lib.rs",
      "line": 42,
      "message": "assertion failed: expected true, got false"
    }
  ],
  "coverage": {
    "lines": 82.5,
    "branches": 75.0
  }
}
```

**Implementation:**
- Runner-specific output parsers (JUnit XML, JSON, TAP)
- Emits `TestSuiteStarted`, `TestPassed`, `TestFailed`, `TestSuiteCompleted` events
- Integrates with coverage tools (llvm-cov, c8, coverage.py)

**Example Use Cases:**
- CI/CD pipelines
- TDD workflows (RED → GREEN)
- Regression testing
- Coverage enforcement

---

## Part 3: Comparison Matrix

| Aspect | `infer:` | `think:` | `agent:` | `browse:` | `test:` |
|--------|----------|----------|----------|-----------|---------|
| LLM | Yes | Yes | Yes | No | No |
| Tools | No | MCP (thinking) | MCP (any) | MCP (playwright) | No |
| Multi-turn | No | Yes (steps) | Yes (turns) | No | No |
| Deterministic | No | No | No | Yes | Yes |
| Best for | Generation | Analysis | Automation | Browser | Testing |

---

## Part 4: Implementation Roadmap

### Phase 1: Hooks (v0.6.0)
- [ ] Add `HooksConfig` to `ast/workflow.rs`
- [ ] Update JSON schema with hooks
- [ ] Implement hook execution in runner
- [ ] Add template variable substitution
- [ ] Add `HookExecuted` event type
- [ ] Write tests for matcher patterns

### Phase 2: `think:` Verb (v0.6.1)
- [ ] Add `ThinkParams` to `ast/action.rs`
- [ ] Implement sequential-thinking MCP integration
- [ ] Add `ThinkingStep` event type
- [ ] Update JSON schema
- [ ] Create example workflows

### Phase 3: `browse:` Verb (v0.6.2)
- [ ] Add `BrowseParams` with actions DSL
- [ ] Implement Playwright MCP integration
- [ ] Add action DSL parser
- [ ] Add `BrowserAction` event type
- [ ] Create example workflows

### Phase 4: `test:` Verb (v0.6.3)
- [ ] Add `TestParams` to `ast/action.rs`
- [ ] Implement runner output parsers
- [ ] Add coverage integration
- [ ] Add test event types
- [ ] Create example workflows

---

## Part 5: Open Questions

1. **Should hooks be able to modify task parameters?**
   - Pro: Enables dynamic configuration
   - Con: Complicates execution model

2. **Should `think:` support branching (explore multiple paths)?**
   - Consider: `strategy: linear | branching | beam_search`

3. **Should `browse:` support parallel actions?**
   - Consider: `parallel: true` for independent actions

4. **Should `test:` support custom parsers via plugins?**
   - Consider: `parser: ./custom-parser.js`

---

## References

- [Claude Code Hooks](~/.claude-code-docs/docs/hooks.md)
- [Playwright MCP](https://github.com/playwright-community/playwright-mcp)
- [Sequential Thinking MCP](https://github.com/modelcontextprotocol/server-sequential-thinking)
- [Nika ADR-001](../.claude/rules/adr/adr-001-5-semantic-verbs.md)
