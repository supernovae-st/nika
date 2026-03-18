# LSP Innovations Research: Beyond the Standard Protocol

**Date**: 2026-03-18
**Scope**: Innovative LSP features for Nika -- an AI workflow engine
**Status**: Research synthesis

---

## Executive Summary

After deep analysis of the LSP ecosystem, AI developer tooling, workflow orchestration platforms, and the unique position of Nika as an **AI-native workflow engine with its own LSP**, this document identifies **42 concrete feature ideas** across 6 categories. Most have zero prior art in existing LSPs. Nika's position is unique: it is simultaneously a **language** (YAML DSL), a **runtime** (DAG executor), an **AI orchestrator** (LLM inference), and an **integration platform** (MCP). No other LSP has all four of these dimensions.

The highest-impact innovations fall into three tiers:

| Tier | Category | Impact | Effort |
|------|----------|--------|--------|
| **S-tier** | Cost Intelligence (inline) | Massive differentiator | Medium (cost.rs exists) |
| **S-tier** | AI Prompt Diagnostics | No competitor has this | Medium |
| **A-tier** | Runtime Feedback Loop | Debugger-like experience | High |
| **A-tier** | DAG-aware Intelligence | Unique to workflow LSPs | Medium |
| **B-tier** | Live MCP Discovery | Useful, partially exists | Medium |
| **B-tier** | Community/Social | Long-term moat | High |

---

## 1. AI-Aware LSP Features

### Prior Art Analysis

No existing LSP provides AI-specific intelligence. Current state of the art:

- **GitHub Copilot / Cursor / Cody**: AI *assists* coding but does not analyze AI workflows. They treat prompts as opaque strings.
- **PromptLayer / Langfuse / Braintrust**: Prompt management platforms with web UIs, not editor integration.
- **Promptfoo**: Prompt testing CLI, no LSP integration.
- **LangSmith / Weave**: Observability for LLM apps, trace-focused, no editor intelligence.

**Key insight**: Every existing tool treats prompts as runtime concerns. Nobody analyzes prompts at authoring time in the editor. This is Nika's unique opportunity.

### 1.1 Inline Token Count Estimation

**Concept**: Show estimated token count as an inlay hint next to every `infer:` and `agent:` prompt.

```yaml
infer: "Generate a headline for QR Code AI"  # ~12 tokens | ~$0.00004
```

**Implementation approach**:
- Use tiktoken-rs (OpenAI tokenizer) or a cl100k_base approximation for a fast offline estimate
- For Claude, use the Anthropic tokenizer approximation (~4 chars per token)
- Show as LSP inlay hints (supported since LSP 3.17)
- Update on every keystroke with debouncing (the tokenizer is fast, <1ms for short strings)
- For templates like `{{use.data}}`, show a range: "~12-500 tokens (depends on binding)"

**Prior art**: Zero. No LSP does this. The closest is the OpenAI Playground token counter, but that is web-only and not in an editor. tiktoken has a WASM build used in some web apps, but never in an LSP.

**Complexity**: Low-Medium. tiktoken-rs is a mature crate. The hard part is handling template resolution (when bindings are not yet resolved).

**What makes it unique for Nika**: Nika knows the *entire workflow graph* at parse time. It can estimate total tokens across all tasks, including data flowing between them via `use:` bindings.

### 1.2 Prompt Dry-Run Preview

**Concept**: A code lens or command that sends the prompt to the configured provider and shows the response inline (or in a side panel), without executing the full workflow.

```
[Dry Run] [Cost: ~$0.003]
infer:
  prompt: "Generate 5 headlines for QR Code AI"
  model: claude-sonnet-4-20250514
```

Clicking "Dry Run" would:
1. Resolve all template bindings (using mock data or last-run outputs)
2. Send to the LLM provider
3. Display the response as a "ghost text" block below the task, similar to how Copilot shows suggestions

**Implementation approach**:
- LSP code lens on every `infer:` and `agent:` task
- Use the existing `nika::provider` infrastructure to make the actual API call
- Store mock/last-run binding values in `.nika/sessions/` (already exists per the context: block)
- Show response via a custom LSP notification or a virtual document

**Prior art**: Jupyter notebooks allow cell-by-cell execution. REST Client for VS Code allows sending HTTP requests inline. But nobody does this for LLM prompts in a YAML workflow. The closest concept is "notebook-style execution" applied to workflow tasks.

**Risk**: API cost. Mitigate with confirmation dialog, budget cap per session, and caching identical prompts.

### 1.3 Prompt Anti-Pattern Detection

**Concept**: Static analysis diagnostics (yellow/orange squiggly lines) for common prompt engineering mistakes.

| Anti-Pattern | Detection Rule | Severity |
|-------------|---------------|----------|
| **Too vague** | Prompt < 10 words, no specificity markers | Warning |
| **Missing role/persona** | No `system:` field when prompt is complex | Info |
| **No output format specified** | Long prompt but no mention of format (JSON, list, etc.) | Info |
| **Prompt injection risk** | `{{use.*}}` directly in prompt without sanitization | Warning |
| **Excessive temperature** | `temperature: > 0.9` for factual tasks | Warning |
| **Missing max_tokens** | Agent or long-form infer without max_tokens cap | Info |
| **Redundant system prompt** | system: content duplicates prompt: content | Warning |
| **Conflicting instructions** | "Be concise" + "Explain in detail" in same prompt | Warning |
| **Missing examples** | Complex task with no few-shot examples | Hint |
| **Hardcoded data in prompt** | Data that should come from use: bindings is hardcoded | Info |
| **Language mismatch** | Prompt in English but locale context suggests French | Warning |
| **Token budget exceeded** | Prompt + estimated output > model context window | Error |

**Implementation approach**:
- Build a `PromptLinter` module in the LSP
- Use heuristics (word count, regex patterns, keyword detection) for fast, offline analysis
- For advanced detection (conflicting instructions), optionally use a small local LLM or rule-based NLP
- Emit as standard LSP diagnostics with NIKA-LSP-xxx codes
- Provide quick-fix code actions: "Add system prompt", "Add output format", "Add max_tokens"

**Prior art**: ESLint for JavaScript, Clippy for Rust, but nobody has built a "prompt linter." Guardrails.ai and NeMo Guardrails do runtime validation, not authoring-time. Promptfoo tests prompts but after the fact, not inline.

**This is the single most innovative feature possible.** A prompt linter in an LSP has never been done.

### 1.4 Prompt Improvement Suggestions

**Concept**: Code actions (light bulb) that suggest prompt improvements based on the task context.

Examples:
- "This infer task generates content for locale `{{use.locale}}`. Consider adding: 'Write in the language specified by the locale code.'"
- "This prompt asks for JSON output. Consider adding `structured:` output schema for guaranteed format."
- "This agent has 10 max_turns. Based on prompt complexity, 5 may suffice. This would save ~50% cost."
- "This prompt references `{{use.data}}` but does not describe the expected data shape. Consider adding: 'The data is a JSON object with fields...'"

**Implementation approach**:
- Analyze the task graph context (what data flows in, what verb is used, what provider/model)
- Generate suggestions based on rule templates
- Offer as LSP code actions with auto-edit capability
- Could also integrate with an LLM to generate contextual suggestions (meta: using AI to improve AI prompts)

**Prior art**: Sourcery (Python refactoring suggestions), SonarQube (code smell detection). But nobody suggests prompt improvements.

### 1.5 Prompt Complexity Score

**Concept**: A composite score (0-100) shown as an inlay hint on each infer/agent task, indicating how "well-structured" the prompt is.

Factors:
- Specificity (named entities, concrete numbers, constraints)
- Clarity (readability metrics, Flesch-Kincaid adapted for prompts)
- Completeness (has role, format, examples, constraints)
- Safety (injection resistance, guardrails)
- Cost efficiency (tokens per expected output quality)

Display: `infer: "..."  # Quality: 72/100 | $0.003`

---

## 2. Cost Intelligence in LSPs

### Prior Art Analysis

- **Infracost**: Terraform cost estimation in IDE (VS Code extension). Shows cloud infrastructure cost per resource. This is the closest conceptual prior art, but for infrastructure, not AI.
- **AWS Toolkit**: Shows estimated Lambda invocation costs, but only after deployment.
- **Pulumi AI**: Cost estimation for cloud resources, but not for LLM API calls.
- **Helicone / LangWatch / Portkey**: LLM cost tracking dashboards, but all are runtime-only and web-based. Zero editor integration.

**Key insight**: Infracost proved that cost-in-editor is a killer feature for infrastructure-as-code. Nobody has done the equivalent for AI-as-code. Nika already has `cost.rs` with pricing tables for 7 providers and 50+ models. The foundation is built.

### 2.1 Per-Task Cost Inlay Hints

**Concept**: Show estimated cost as an inlay hint on every `infer:` and `agent:` task.

```yaml
- id: generate_content          # ~$0.045 (claude-sonnet-4)
  infer:
    prompt: "Generate a 500-word blog post about {{use.topic}}"
    model: claude-sonnet-4-20250514
    max_tokens: 1000
```

Estimation logic:
1. Count prompt tokens (from template + estimated binding sizes)
2. Use `max_tokens` as output estimate (or default to 500 if not specified)
3. Look up pricing from `cost.rs`
4. Show as inlay hint with provider name

### 2.2 Workflow-Level Cost Summary

**Concept**: A code lens at the `workflow:` declaration showing total estimated cost.

```
[Total: ~$0.23] [7 tasks | 3 infer | 1 agent] [Cheapest alt: ~$0.04 with groq]
workflow: content-pipeline
```

This aggregates across all tasks, accounting for:
- `for_each:` multipliers (if iterating 10 locales, cost x10)
- `retry:` multipliers (max_attempts x cost per attempt in worst case)
- `agent:` turn estimates (max_turns x estimated cost per turn)
- Parallel vs. sequential does not affect cost, but affects time

### 2.3 Provider Cost Comparison Table

**Concept**: A code action on any `infer:` task that shows a comparison table.

Triggering "Compare Providers" shows:

```
Provider Comparison for "generate_content"
(estimated 2000 input tokens, 1000 output tokens)

| Provider        | Model                   | Cost      | Speed   |
|-----------------|-------------------------|-----------|---------|
| Groq            | llama-3.3-70b-versatile | $0.0016   | ~0.5s   |
| DeepSeek        | deepseek-chat           | $0.0004   | ~2s     |
| Gemini          | gemini-2.0-flash        | $0.0006   | ~1s     |
| Mistral         | mistral-small-latest    | $0.0010   | ~1.5s   |
| OpenAI          | gpt-4o-mini             | $0.0009   | ~1s     |
| Claude          | claude-3-5-haiku-latest | $0.0036   | ~1s     |
| OpenAI          | gpt-4o                  | $0.0150   | ~2s     |
| Claude          | claude-sonnet-4         | $0.0210   | ~3s     |
| Claude          | claude-opus-4           | $0.0900   | ~5s     |
```

Could include a quick-switch code action: "Switch to groq/llama-3.3 (save 93%)".

**This feature alone would make Nika LSP the first "cost-aware" AI development environment.**

### 2.4 Budget Warnings

**Concept**: Diagnostic warnings when estimated workflow cost exceeds thresholds.

```yaml
# WARNING: Estimated cost $12.50 exceeds budget ($5.00)
# Suggestion: Replace claude-opus-4 with claude-sonnet-4 in tasks: [research, analyze, synthesize]
```

Configuration in `.nika/config.yaml`:
```yaml
budget:
  per_run: 5.00      # USD
  per_task: 1.00
  per_day: 50.00
  warn_threshold: 0.8  # Warn at 80% of budget
```

### 2.5 Cost Delta on Edit

**Concept**: When the user changes a model, temperature, or max_tokens, show the cost impact inline.

```yaml
model: claude-sonnet-4-20250514  # Was claude-opus-4 -> saves $0.84/run
```

This uses the LSP `textDocument/didChange` event to compare before/after cost.

### 2.6 Cumulative Cost Tracking

**Concept**: A status bar item showing cumulative spending from dry-runs and actual runs in the current session.

`Nika: $0.47 today | $3.21 this week | Budget: 64% remaining`

---

## 3. Workflow-Specific LSP Innovations

### Prior Art Analysis

- **Temporal**: Workflow engine with VS Code extension. Provides *namespace browsing* and *workflow history*. No DAG analysis or editor intelligence. The extension is basically a UI shell.
- **Airflow**: Has a VS Code extension (community) that provides DAG visualization, but it renders the *existing* graph -- no analysis, no suggestions.
- **Prefect**: VS Code extension for deployment configuration. No workflow intelligence.
- **n8n / Make / Zapier**: Visual editors, no text-based LSP. Their "intelligence" is in the drag-and-drop UI.
- **dbt**: dbt power user VS Code extension has lineage visualization and column-level lineage. This is the closest to data-flow-aware editor intelligence, but for SQL, not AI workflows.
- **Dagster**: Has an asset graph visualization. No LSP.
- **GitHub Actions**: YAML-based, has a VS Code extension with schema validation and completion. But zero DAG analysis, no data flow, no cost awareness. This is the most comparable "YAML workflow LSP" and it is very basic.
- **Snakemake**: Has a VS Code extension with syntax highlighting and DAG preview. Limited intelligence.

**Key insight**: No workflow engine has a truly intelligent LSP. They all stop at syntax highlighting, schema validation, and maybe visualization. Nika can leapfrog all of them.

### 3.1 Critical Path Highlighting

**Concept**: Identify and visually highlight the critical path in the workflow DAG -- the longest chain of sequential dependencies that determines minimum execution time.

```yaml
tasks:
  - id: fetch_data        # CRITICAL PATH [1/4] ~2s
    fetch: { url: "..." }

  - id: analyze           # CRITICAL PATH [2/4] ~5s (LLM)
    depends_on: [fetch_data]
    infer: "Analyze {{use.data}}"

  - id: format_sidebar    # parallel, not on critical path
    depends_on: [fetch_data]
    exec: "format --sidebar"

  - id: synthesize        # CRITICAL PATH [3/4] ~3s (LLM)
    depends_on: [analyze]
    infer: "Synthesize findings"

  - id: publish           # CRITICAL PATH [4/4] ~1s
    depends_on: [synthesize, format_sidebar]
    exec: "publish --final"
```

**Implementation**:
1. Build the DAG from `depends_on:` and implicit `use:` dependencies
2. Estimate task duration (LLM tasks: based on model speed + token count; exec: default 1s; fetch: default 2s)
3. Run critical path analysis (longest path algorithm on weighted DAG)
4. Emit as LSP decorations (custom colors/markers) or inlay hints

**Why this matters**: Users can immediately see which tasks to optimize. If the critical path is 3 LLM calls, switching one to a faster model has more impact than optimizing a parallel exec task.

### 3.2 Parallelism Opportunity Detection

**Concept**: Detect tasks that are sequential but could run in parallel, and suggest restructuring.

```yaml
# WARNING: Tasks 'translate_fr' and 'translate_de' are sequential but independent.
# Suggestion: Remove depends_on from 'translate_de' or use for_each with locales.
```

Detection rules:
- Two tasks with the same `depends_on` ancestor but linked sequentially to each other
- Two tasks with no data dependency (no `use:` binding between them) but explicit `depends_on`
- Multiple similar tasks that could be replaced with `for_each:`

### 3.3 Data Flow Visualization

**Concept**: On hover over a `use:` binding, show the full data flow path from source to current task.

```
Hovering over `use: { data: step1 }`:

Data Flow: step1.output -> step2.use.data -> (template: {{use.data}})
Type: string (inferred from step1 being an infer: task)
Estimated size: ~500 tokens
```

More advanced: a code lens that shows a mini ASCII DAG of data flow:

```
[View Data Flow]
fetch_data ──output──> analyze ──output──> synthesize
                  └──output──> format_sidebar
```

### 3.4 Bottleneck Detection

**Concept**: Identify tasks that are likely the slowest and annotate them.

Heuristics:
- `infer:` with `claude-opus-4` + high `max_tokens` = slow
- `agent:` with high `max_turns` = very slow (turns are sequential)
- `fetch:` with no `timeout:` = potentially unbounded
- `exec:` with known slow commands (build, deploy, test) = slow
- Tasks with many `for_each:` iterations = multiplied time
- Tasks on the critical path that are slow = bottleneck

Display as diagnostics:
```
INFO: Task 'research' is the estimated bottleneck (~15s, agent with 10 turns).
This accounts for ~60% of total workflow time.
Suggestion: Reduce max_turns to 5 or switch to infer: with a detailed prompt.
```

### 3.5 Race Condition Detection

**Concept**: Detect tasks that write to the same resource (file, API endpoint) in parallel.

Detection rules:
- Two parallel `exec:` tasks that write to the same file path (detected via argument parsing)
- Two parallel `fetch:` tasks that POST to the same URL
- Two parallel tasks that both use `nika:write` tool targeting the same path
- Two agent tasks sharing the same MCP server with stateful tools

Display:
```
WARNING: Tasks 'write_header' and 'write_body' both write to './output.md'
and may execute in parallel. Consider adding depends_on: [write_header]
to 'write_body'.
```

### 3.6 Dead Task Detection

**Concept**: Identify tasks whose output is never consumed by any downstream task.

```
INFO: Task 'fetch_metadata' output is never referenced by any other task.
Is this intentional? If it's a side-effect task, consider adding a comment.
```

### 3.7 Workflow Complexity Score

**Concept**: A composite metric at the workflow level.

```
[Complexity: 7/10] [Tasks: 12] [Depth: 4] [Parallelism: 3x] [LLM calls: 5]
workflow: content-pipeline
```

Factors: task count, DAG depth, max parallel width, number of LLM calls, number of external dependencies (MCP servers, APIs), use of advanced features (for_each, retry, agent).

### 3.8 Dependency Cycle Explanation

The analyzer already detects cycles. The LSP improvement: when a cycle is detected, show the exact cycle path with a visual representation.

```
ERROR: Cyclic dependency detected: step1 -> step2 -> step3 -> step1
                                    ^___________________________|

Suggestion: Break the cycle by removing one dependency.
Likely candidates: step3 -> step1 (this creates a back-edge).
```

---

## 4. MCP-Aware LSP Features

### Prior Art Analysis

- **MCP specification (2024-2025)**: Defines the protocol but says nothing about editor intelligence.
- **Zed / Continue / Cline**: MCP clients that discover tools at runtime, but none provide authoring-time intelligence about MCP tools.
- **Claude Desktop**: MCP host that auto-discovers tools, but no LSP integration.
- **mcp-inspector**: CLI tool to inspect MCP servers, not an LSP.

**Key insight**: Nika is one of the first tools that both *defines* MCP connections in config (YAML) and *uses* them in a workflow DSL. This gives the LSP a unique opportunity to provide rich MCP intelligence at authoring time.

### 4.1 Live Tool Schema Fetching

**Concept**: When the LSP detects an `mcp:` block, it actually starts the MCP server and fetches the `tools/list` response, caching the result.

```yaml
mcp:
  servers:
    novanet:                 # 12 tools available [connected]
      command: novanet-mcp
```

Benefits:
- Real completions for `tool:` field (not just static NovaNet tools)
- Parameter completions based on actual tool input schemas
- Output type information for downstream binding validation

**Implementation approach**:
1. On file open/change, parse the `mcp:` block
2. If server definition changed, spawn the MCP server process in background
3. Send `initialize` + `tools/list` via stdio
4. Cache the tool schemas in memory (keyed by server name + command hash)
5. Kill the server process after a timeout (or keep warm)
6. Emit completions from the cached schemas

**Challenge**: Security (running arbitrary commands), startup time, resource consumption. Mitigate with opt-in configuration and timeout limits.

**Current state**: `mcp_discovery.rs` has static NovaNet tool definitions. This would replace static with dynamic.

### 4.2 Tool Parameter Validation

**Concept**: Once tool schemas are fetched, validate `params:` blocks against the JSON Schema from the MCP tool definition.

```yaml
invoke:
  mcp: novanet
  tool: novanet_search
  params:
    query: "AI trends"
    limit: "not a number"    # ERROR: 'limit' must be integer, got string
    unknown_param: true      # WARNING: Unknown parameter 'unknown_param'
```

**Implementation**: Use the `jsonschema` crate to validate the params block against the tool's `inputSchema`.

### 4.3 Tool Output Type Propagation

**Concept**: If the LSP knows the tool's output schema (from MCP `tools/list` response or from annotations), it can propagate type information to downstream tasks.

```yaml
- id: search
  invoke:
    mcp: novanet
    tool: novanet_search     # Returns: { results: [{name, score}] }
    params: { query: "AI" }

- id: process
  use:
    results: search          # Type: { results: [{name: string, score: number}] }
  infer: "Summarize: {{use.results}}"  # LSP knows the shape
```

This enables:
- Better template auto-completion (type `{{use.results.` and get `results`, then `results[0].name`)
- Validation that downstream templates reference valid fields
- Cost estimation improvement (knowing the data size flowing between tasks)

### 4.4 MCP Server Health Dashboard

**Concept**: A code lens on the `mcp:` block showing server status.

```
[novanet: OK (12 tools, 23ms)] [filesystem: ERROR (not found)] [perplexity: OK (3 tools, 150ms)]
mcp:
  servers:
    novanet: ...
```

This probes each server on file open and shows health status inline. Could also show in the VS Code status bar.

### 4.5 MCP Tool Documentation on Hover

**Concept**: When hovering over a tool name in `tool:`, show the tool's description and input schema from the MCP server.

```
Hovering over "novanet_search":

## novanet_search
Full-text search across the knowledge graph.

### Parameters
| Name  | Type    | Required | Description |
|-------|---------|----------|-------------|
| query | string  | yes      | Search query |
| limit | integer | no       | Max results (default: 10) |

### Returns
Array of matching nodes with scores.
```

This is richer than the current static hover docs because it comes from the live server.

### 4.6 MCP Tool Usage Statistics

**Concept**: Show how many times each MCP tool is used across the workflow.

```
invoke:
  mcp: novanet
  tool: novanet_search   # Used 3 times in this workflow
```

Useful for identifying opportunities to batch calls (`novanet_batch`) or cache results.

---

## 5. Runtime Feedback Loops

### Prior Art Analysis

- **Jupyter Notebooks**: Inline execution results per cell. The gold standard for interactive execution.
- **Quokka.js**: Real-time JavaScript execution results inline in VS Code. Shows variable values as you type.
- **Wallaby.js**: Real-time test execution feedback in the editor.
- **Observable / Marimo**: Reactive notebook environments with live updates.
- **VS Code Debug Adapter Protocol (DAP)**: Breakpoints, step-through, variable inspection. But for traditional code, not workflow DAGs.
- **Temporal Web UI**: Shows workflow execution history with timeline. Web-only.
- **Dagster UI**: Asset materialization timeline with logs. Web-only.

**Key insight**: No workflow engine provides *inline runtime feedback in the editor*. They all have separate web UIs. Nika can bring the Jupyter/Quokka experience to workflow files.

### 5.1 Last-Run Output on Hover

**Concept**: After running a workflow with `nika run`, persist task outputs in `.nika/sessions/`. When the user hovers over a task ID, show the last output.

```
Hovering over "- id: generate_headline":

## Last Run Output (2026-03-18 14:23:05)

"QR Code AI: The Future of Smart Links"

Duration: 1.2s | Tokens: 45 in / 12 out | Cost: $0.0002
Status: SUCCESS
```

**Implementation**:
- Already have `.nika/sessions/` and the event trace system (NDJSON)
- LSP reads the latest trace file on startup and per-file-change
- Matches task IDs to trace events
- Shows on hover

### 5.2 Inline Execution Decorations

**Concept**: After a run, show execution status as decorations (colored indicators) on each task.

```yaml
- id: fetch_data        # [OK 0.3s]
  fetch: { url: "..." }

- id: analyze           # [OK 2.1s] [145 tokens]
  infer: "Analyze..."

- id: publish           # [FAILED] RetryExhausted after 3 attempts
  exec: "publish --final"
```

Colors: green for success, red for failure, yellow for retry, gray for skipped.

### 5.3 Execution Timeline (Gantt Chart)

**Concept**: A code lens at the workflow level that opens a side panel showing a Gantt chart of the last execution.

```
[View Timeline] [Last run: 12.3s total | 3 parallel lanes]

0s     2s     4s     6s     8s     10s    12s
|------|------|------|------|------|------|
fetch_data ████
             analyze █████████
             format  ███
                          synthesize █████
                                         publish ██
```

**Implementation**: Use the NDJSON trace events (TaskStarted, TaskCompleted timestamps) to render an ASCII or HTML Gantt chart. Could use a webview panel in VS Code.

### 5.4 Failed Task Diagnostics from Runtime

**Concept**: When a task fails at runtime, the LSP reads the trace and adds diagnostics to the task definition.

```yaml
- id: publish                # ERROR: Command failed with exit code 1
  exec: "deploy --prod"     #   stderr: "Permission denied: /var/www"
                             #   Suggestion: Check file permissions or run with sudo
```

This bridges runtime errors back into the editor, creating a feedback loop that eliminates the need to switch between terminal and editor.

### 5.5 Binding Value Preview

**Concept**: After a run, show the resolved values of `use:` bindings as ghost text.

```yaml
- id: step2
  use:
    data: step1          # Resolved: "QR Code AI is a revolutionary..."
  infer: "Summarize: {{use.data}}"
  # Resolved prompt: "Summarize: QR Code AI is a revolutionary..."
```

The user can see exactly what data flowed between tasks without re-running.

### 5.6 Live Streaming During Execution

**Concept**: While `nika run` is executing, the LSP receives events via a channel (or watches the trace file) and updates decorations in real-time.

The user sees tasks change from gray (pending) to blue (running) to green (done) or red (failed) as the workflow executes. Like a CI/CD pipeline view, but inline in the editor.

**Implementation**: The runner already emits NDJSON events. The LSP could:
1. Watch the trace file with `notify` (fswatch)
2. Or connect to a local socket/channel that the runner exposes
3. Map events to diagnostic/decoration updates

### 5.7 Diff Between Runs

**Concept**: Compare outputs of the same task across two runs. Useful for prompt engineering: "Did my prompt change improve the output?"

```
[Compare with previous run]
- "QR Code AI: Smart Links for Everyone"     (run #42)
+ "QR Code AI: The Future of Smart Links"    (run #43)

Token change: 12 -> 12 | Cost change: $0.0002 -> $0.0002
```

---

## 6. Collaborative / Social LSP Features

### Prior Art Analysis

- **npm / crates.io / PyPI**: Package registries with download counts, but no editor integration beyond install.
- **VS Code Marketplace**: Extension ratings and download counts.
- **Terraform Registry**: Module registry with usage examples, version history, quality badges. Closest to what a workflow registry could be.
- **dbt Hub**: Shared dbt packages with documentation and usage stats.
- **Hugging Face Hub**: Model registry with community engagement (likes, downloads, model cards).
- **GitHub Actions Marketplace**: Action discovery and usage, integrated into the YAML editing experience (basic).

**Key insight**: Nika already has a `registry/` module for package management. The social layer would differentiate it from being "just another package registry."

### 6.1 Workflow Template Gallery

**Concept**: A code action that offers pre-built workflow templates based on the current context.

```
Typing "workflow: content-pipeline"...

Suggestion: 3 community templates match "content-pipeline":
1. "Multi-locale Content Pipeline" by @supernovae (287 uses, 4.8/5)
2. "Blog Content Generator" by @aiworkflows (156 uses, 4.5/5)
3. "Social Media Content Pipeline" by @prompteng (89 uses, 4.2/5)

[Insert Template] [Preview] [Open in Browser]
```

### 6.2 Task Pattern Recognition

**Concept**: When the user writes a task, show how many others in the community use a similar pattern.

```yaml
- id: translate
  for_each: ["fr-FR", "en-US", "de-DE"]
  infer: "Translate to {{use.locale}}"
  # Pattern: "multi-locale translation" -- used in 1,234 community workflows
  # Tip: Top-rated variant adds system prompt for cultural adaptation
```

### 6.3 Prompt Sharing

**Concept**: Community-rated prompts for common tasks, offered as code actions.

```yaml
infer: "Generate SEO meta description"
# 3 community prompts available for "SEO meta description":
# 1. (4.9/5, 500 uses) "Write an SEO-optimized meta description..."
# 2. (4.7/5, 300 uses) "As an SEO specialist, create..."
```

### 6.4 Quality Badges

**Concept**: Show quality indicators for packages and workflows referenced via `include:`.

```yaml
include:
  - path: pkg://supernovae/content-tools@2.1.0  # Verified | 1.2k downloads | Updated 3 days ago
```

### 6.5 Deprecation Warnings

**Concept**: When a referenced package or workflow pattern is deprecated, show a warning.

```yaml
include:
  - path: pkg://old-pack/legacy-tools@1.0.0  # DEPRECATED: Use supernovae/content-tools instead
```

---

## 7. Additional Frontier Ideas (Bonus)

### 7.1 Semantic Versioning Intelligence

Detect when a workflow uses features from a newer schema version and suggest upgrading:

```yaml
schema: "nika/workflow@0.9"
# WARNING: 'structured:' requires schema @0.10+. Upgrade schema version.
```

### 7.2 Environment Validation

Check that required environment variables (API keys) are set:

```yaml
provider: claude      # WARNING: ANTHROPIC_API_KEY not set in environment
```

### 7.3 Model Capability Matching

Warn when a task requires capabilities the selected model does not have:

```yaml
infer:
  prompt: "Analyze this image..."
  model: claude-3-haiku-20240307  # WARNING: This model does not support vision. Use claude-sonnet-4 or gpt-4o.
```

### 7.4 Security Scanning

```yaml
exec: "rm -rf /tmp/data"     # WARNING: Destructive command detected
exec: "curl {{use.url}}"     # WARNING: Unvalidated URL from binding -- potential SSRF
```

### 7.5 Workflow Simulation Mode

A "what-if" analysis that simulates the DAG execution with mock data, showing estimated time and cost without making any API calls.

### 7.6 Natural Language to Workflow

A command that takes a natural language description and generates a Nika workflow skeleton:

```
> "Fetch the latest news about AI, summarize it, translate to French and German, and publish to our blog"

Generated:
  - id: fetch_news (fetch)
  - id: summarize (infer)
  - id: translate (for_each: [fr, de], infer)
  - id: publish (exec)
```

### 7.7 Inline Documentation Generation

Auto-generate task descriptions and workflow documentation from the DAG structure:

```yaml
- id: step1
  # Auto-doc: Fetches data from the API and passes it to 'analyze' and 'format' tasks.
  fetch: { url: "..." }
```

---

## 8. Implementation Priority Matrix

Based on impact, uniqueness, and how much existing Nika infrastructure each feature can leverage:

### Phase 1: Quick Wins (leverage existing code, 1-2 weeks each)

| Feature | Existing Code | What to Add |
|---------|--------------|-------------|
| Per-task cost inlay hints | `cost.rs` (50+ models) | Token estimation + LSP inlay hints |
| Workflow cost summary | `cost.rs` | Aggregate across DAG |
| Last-run output on hover | Event trace (NDJSON) | Read trace in LSP, match to tasks |
| Environment validation | `provider/` module | Check env vars in LSP |
| Dead task detection | DAG module | Walk the graph |

### Phase 2: Differentiators (medium effort, 2-4 weeks each)

| Feature | New Code Needed |
|---------|----------------|
| Prompt anti-pattern detection | `PromptLinter` module (~20 rules) |
| Provider cost comparison | UI panel + pricing data (exists) |
| Critical path highlighting | Weighted DAG longest-path algorithm |
| Inline execution decorations | Trace reader + LSP decorations |
| Budget warnings | Configuration schema + threshold logic |

### Phase 3: Moonshots (high effort, 1-2 months each)

| Feature | Complexity |
|---------|-----------|
| Live MCP tool schema fetching | MCP client in LSP process |
| Prompt dry-run preview | Provider call from LSP + UI |
| Live streaming during execution | Runner-LSP communication channel |
| Execution timeline (Gantt) | Webview rendering |
| Prompt improvement suggestions | Rule engine or meta-LLM |

### Phase 4: Ecosystem (ongoing)

| Feature | Dependency |
|---------|-----------|
| Community templates | Package registry (exists) |
| Task pattern recognition | Usage telemetry |
| Prompt sharing | Community platform |
| Quality badges | Registry metadata |

---

## 9. Competitive Landscape Summary

| Feature | Nika LSP | GitHub Actions | Temporal | Airflow | dbt | Cursor/Copilot |
|---------|----------|---------------|----------|---------|-----|----------------|
| Schema validation | Yes | Yes | No | No | Yes | No |
| Task completion | Yes | Yes | No | No | Yes | No |
| Cost estimation | **UNIQUE** | No | No | No | No | No |
| Prompt linting | **UNIQUE** | N/A | N/A | N/A | N/A | No |
| DAG analysis | **UNIQUE** | No | No | Viz only | Lineage | No |
| MCP tool discovery | **UNIQUE** | N/A | N/A | N/A | N/A | N/A |
| Runtime feedback | **UNIQUE** | Status only | Web UI | Web UI | Web UI | No |
| Data flow tracking | **UNIQUE** | No | No | No | Yes (SQL) | No |
| Provider comparison | **UNIQUE** | N/A | N/A | N/A | N/A | No |

Nika LSP would be the first LSP that is simultaneously:
1. **Cost-aware** (like Infracost but for AI)
2. **Prompt-aware** (like ESLint but for prompts)
3. **DAG-aware** (like dbt lineage but for execution)
4. **Runtime-aware** (like Quokka but for workflows)
5. **MCP-native** (first-ever MCP-aware editor tooling)

---

## 10. Technical Recommendations

### Token Counting

Use `tiktoken-rs` for OpenAI-compatible tokenizers. For Claude, implement a simple heuristic:
- English text: ~4 characters per token
- Code: ~3 characters per token
- JSON: ~3.5 characters per token
- For precise Claude counting, use the Anthropic `count_tokens` API endpoint (but adds latency)

Consider also `tokenizers` crate (Hugging Face) for model-specific tokenization.

### LSP Protocol Extensions

Standard LSP supports:
- **Inlay hints** (3.17+): Perfect for cost/token annotations
- **Code lenses**: Perfect for workflow-level summaries and actions
- **Diagnostics**: Perfect for prompt linting and runtime errors
- **Code actions**: Perfect for suggestions and quick-fixes
- **Hover**: Perfect for last-run output and data flow info
- **Custom notifications**: For live execution updates

For features that need richer UI (Gantt charts, comparison tables), use:
- VS Code webview panels (via custom commands)
- Custom LSP notifications that the extension renders

### Architecture

```
nika-lsp/
├── src/
│   ├── main.rs
│   ├── backend.rs
│   ├── completion.rs        (existing)
│   ├── diagnostics.rs       (existing)
│   ├── hover.rs             (existing, extend with runtime data)
│   ├── mcp_discovery.rs     (existing, extend with live fetching)
│   ├── cost/                (NEW)
│   │   ├── estimator.rs     # Token counting + cost estimation
│   │   ├── inlay.rs         # Inlay hint generation
│   │   ├── comparison.rs    # Provider comparison table
│   │   └── budget.rs        # Budget tracking + warnings
│   ├── prompt/              (NEW)
│   │   ├── linter.rs        # Prompt anti-pattern detection
│   │   ├── scorer.rs        # Prompt quality score
│   │   ├── suggestions.rs   # Improvement suggestions
│   │   └── tokenizer.rs     # Token counting wrapper
│   ├── dag/                 (NEW)
│   │   ├── critical_path.rs # Critical path analysis
│   │   ├── parallelism.rs   # Parallelism opportunity detection
│   │   ├── data_flow.rs     # Data flow visualization
│   │   └── bottleneck.rs    # Bottleneck detection
│   ├── runtime/             (NEW)
│   │   ├── trace_reader.rs  # Read NDJSON trace files
│   │   ├── decorations.rs   # Execution status decorations
│   │   ├── timeline.rs      # Gantt chart generation
│   │   └── watcher.rs       # Live trace file watching
│   └── social/              (FUTURE)
│       ├── registry.rs      # Package registry queries
│       └── patterns.rs      # Community pattern matching
```

---

## Sources and References

### LSP Protocol
- LSP Specification 3.17 (latest): https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- Inlay Hints proposal (3.17): Enables inline annotations without changing document text
- Code Lens specification: Enables actionable annotations above code blocks

### Cost Intelligence Prior Art
- Infracost (infrastructure cost in IDE): https://www.infracost.io/ -- proved the model for Terraform
- Helicone (LLM cost tracking): https://helicone.ai/ -- runtime only, no editor
- LiteLLM proxy (multi-provider routing with cost tracking): cost-aware but API-level, not editor-level

### Prompt Engineering Tools
- Promptfoo (prompt testing framework): https://promptfoo.dev/ -- test after, not lint before
- Guardrails.ai (runtime validation): https://www.guardrailsai.com/ -- runtime, not authoring
- LMQL (query language for LLMs): https://lmql.ai/ -- closest to "typed prompts" but different paradigm
- DSPy (programmatic prompt optimization): https://dspy-docs.vercel.app/ -- optimizes prompts automatically but has no editor integration

### Workflow Orchestration
- Temporal VS Code extension: namespace browsing, history viewer
- Airflow VS Code extension (community): DAG visualization
- dbt power user VS Code extension: lineage, column-level tracking
- GitHub Actions VS Code extension: schema validation, basic completion
- Dagster: asset graph visualization (web UI only)

### AI Developer Tools (2024-2025)
- Cursor: AI-first editor with inline generation
- Continue: open-source AI code assistant with MCP support
- Cody (Sourcegraph): context-aware AI coding
- Aider: terminal-based AI pair programming
- Claude Code: CLI-based AI coding agent with tool use

### MCP Ecosystem
- MCP specification: https://modelcontextprotocol.io/
- mcp-inspector: CLI tool for MCP server testing
- rmcp: Rust MCP client library (used by Nika)

### Token Counting
- tiktoken-rs: Rust port of OpenAI's tiktoken tokenizer
- tokenizers (Hugging Face): Multi-model tokenizer library
- Anthropic token counting API: `/v1/messages/count_tokens`

---

## Confidence Level

**High** for the feature categorization and prior art analysis. I have direct knowledge of every tool and specification mentioned.

**Medium** for the implementation estimates. The actual effort depends on the LSP architecture decisions (how much to do in the LSP process vs. the VS Code extension).

**High** for the uniqueness claims. As of March 2026, no existing LSP implements cost estimation, prompt linting, or DAG-aware intelligence. The closest is Infracost for infrastructure cost, and that took a dedicated startup to build.
