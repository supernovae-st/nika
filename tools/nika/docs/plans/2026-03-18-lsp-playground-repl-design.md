# Nika LSP Playground / REPL Mode — Architecture Design

> Date: 2026-03-18
> Depends on: [LSP 11/10 Master Plan](./2026-03-18-lsp-11-out-of-10-master-plan.md) (Phase 2+)
> Inspiration: Quokka.js, Jupyter notebooks, Swift Playgrounds, Rust Playground
> Goal: Run individual tasks inline and see results in the editor — the missing link between "write" and "run"

---

## Executive Summary

The Nika REPL/Playground adds **live task execution** to the LSP. Users can run a single task, see its output as an inlay hint, iterate on their prompt, and diff successive outputs — all without leaving the editor. A scratch pad mode enables zero-boilerplate prompt experimentation. Mock mode validates DAG structure without spending money.

This is the feature that turns the Nika LSP from "a smart editor" into "a development environment."

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Single-Task Execution](#2-single-task-execution)
3. [Scratch Pad](#3-scratch-pad)
4. [Incremental Re-run with Diff](#4-incremental-re-run-with-diff)
5. [Mock Mode](#5-mock-mode)
6. [LSP-Runner Communication Architecture](#6-lsp-runner-communication-architecture)
7. [UX: How Results Appear](#7-ux-how-results-appear)
8. [Security Model](#8-security-model)
9. [Custom LSP Requests](#9-custom-lsp-requests)
10. [VS Code Extension Code Sketches](#10-vs-code-extension-code-sketches)
11. [Rust Implementation](#11-rust-implementation)
12. [Testing Strategy](#12-testing-strategy)
13. [Phased Rollout](#13-phased-rollout)

---

## 1. Architecture Overview

### The Key Insight

Nika already has every piece needed for single-task execution:
- `TaskExecutor::execute()` runs one action with bindings and a datastore
- `EventLog` with broadcast channels provides real-time event streaming
- `RunContext` is a thread-safe `DashMap` that can be pre-seeded
- `CancellationToken` enables abort-on-edit

The LSP does NOT need to spawn a full `Runner` (which builds a DAG). It needs a **lightweight harness** that creates a `TaskExecutor`, feeds it one task, and streams events back.

### Component Diagram

```
VS Code Extension                          Nika LSP Server
+----------------------------+             +------------------------------------+
|                            |   LSP/JSONRPC  |                                 |
|  Code Lens: "Run Task"    |──────────────>|  nika/playground/run              |
|  Scratch Pad Panel         |<──────────────|  nika/playground/result           |
|  Inline Result Decorations |   notify      |  nika/playground/stream           |
|  Output Diff View          |               |                                 |
+----------------------------+             +------------------------------------+
                                                         |
                                                         | in-process (shared library)
                                                         v
                                           +------------------------------------+
                                           |  PlaygroundRunner                  |
                                           |  ├── TaskExecutor (reused)         |
                                           |  ├── RunContext (pre-seeded)       |
                                           |  ├── EventLog (broadcast)          |
                                           |  ├── CancellationToken             |
                                           |  └── SecurityGate                  |
                                           +------------------------------------+
                                                         |
                                                   +-----------+
                                                   | Provider  |
                                                   | (real or  |
                                                   |  mock)    |
                                                   +-----------+
```

### Why In-Process (Not Subprocess)

| Approach | Pros | Cons |
|----------|------|------|
| **Spawn `nika run --task`** | Process isolation, simple | Cold start (~1s boot), no event streaming, no provider cache sharing |
| **Shared library (in-process)** | Zero cold start, reuse provider cache, event streaming via broadcast channel, cancel via token | Crash in executor could affect LSP |
| **IPC daemon** | Full isolation + warm cache | Massive complexity, new daemon process to manage |

**Decision: In-process with crash isolation.** The `TaskExecutor` already uses `tokio::task::spawn` which provides panic isolation. A panic in a spawned task does not crash the LSP process — it returns a `JoinError` that we catch and report as a task failure. For shell commands that might hang, we enforce timeouts via `CancellationToken`.

---

## 2. Single-Task Execution

### User Flow

1. User places cursor on a task (or clicks code lens "Run Task")
2. LSP resolves the task's dependencies from the `RunContext` cache:
   - If dependencies have cached results from previous runs, use those
   - If dependencies have no results, show "Missing dependency: research" and offer to run the dependency first
3. Task runs via `TaskExecutor::execute()`
4. Result appears as an inlay hint after the task block
5. Result is cached in the playground's `RunContext` for downstream tasks

### Dependency Resolution Strategy

```
Task A (no deps)  -->  Run directly
Task B (deps: A)  -->  Check playground cache for A
                       |-- A cached? Use cached result
                       |-- A not cached? Offer: "Run A first" or "Run A+B chain"
Task C (deps: A,B) --> Same logic, recursively
```

The playground maintains a **session-scoped `RunContext`** that persists across runs within the same editor session. This means:
- Run task A once, its output is cached
- Edit task B's prompt, re-run B: it uses A's cached output
- Edit task A, re-run A: cache is invalidated for A AND all downstream tasks

### Cache Invalidation

```rust
/// Track which tasks have been modified since their last playground run
struct PlaygroundCache {
    /// Task results from previous playground runs
    results: RunContext,
    /// Hash of task definition at the time of last run (for invalidation)
    task_hashes: DashMap<Arc<str>, u64>,
    /// Dependency graph for cascade invalidation
    deps: Dag,
}

impl PlaygroundCache {
    /// Invalidate a task and all its transitive dependents
    fn invalidate(&self, task_id: &str) {
        self.results.remove(task_id);
        self.task_hashes.remove(task_id);
        for dependent in self.deps.get_transitive_dependents(task_id) {
            self.results.remove(&dependent);
            self.task_hashes.remove(&dependent);
        }
    }
}
```

---

## 3. Scratch Pad

### Concept

A side panel where users can test a prompt with a model without creating a workflow task. Like a REPL for LLM calls.

### VS Code Implementation

The scratch pad is a **VS Code webview panel** (not a document) that sends `nika/playground/scratch` requests.

```
+------------------------------------------+
|  Nika Scratch Pad                    [x]  |
+------------------------------------------+
|  Model: [claude-sonnet-4-5-20250514 v]   |
|  Provider: [anthropic]                    |
+------------------------------------------+
|  Prompt:                                  |
|  +--------------------------------------+|
|  | Explain quantum computing in one     ||
|  | paragraph for a 10-year-old.         ||
|  +--------------------------------------+|
|                                          |
|  [Run]  [Run with Mock]                  |
+------------------------------------------+
|  Output:                                  |
|  +--------------------------------------+|
|  | Quantum computing is like having a   ||
|  | magic coin that can be both heads... ||
|  +--------------------------------------+|
|  Tokens: 45 in / 128 out | Cost: $0.002 |
|  Duration: 1.2s | Provider: anthropic     |
+------------------------------------------+
```

### What Makes It Different from ChatGPT

1. **Model switching**: Change provider/model in a dropdown, same prompt
2. **Cost tracking**: See exact cost per call (uses existing `cost.rs`)
3. **Template testing**: Use `{{variable}}` syntax, provide variables in a JSON panel
4. **Copy to workflow**: Button that generates a complete task block from the scratch pad state
5. **History**: Last 20 scratch pad runs are saved (in `.nika/playground/history.ndjson`)

### Scratch Pad as Inline Comment (Alternative UX)

For users who prefer staying in the YAML file, support a special comment syntax:

```yaml
tasks:
  - id: research
    infer:
      model: claude-sonnet-4-5-20250514
      prompt: |
        Explain quantum computing for a 10-year-old.

# nika:playground:result
# Quantum computing is like having a magic coin...
# [tokens: 45/128 | cost: $0.002 | 1.2s]
```

The LSP inserts/updates the comment block after the task. This is **not** a virtual document — it is real text in the file, clearly marked as generated. The user can delete it at any time.

**Decision: Support both.** Webview scratch pad for freeform experimentation. Inline comments for in-file iteration. The user chooses.

---

## 4. Incremental Re-run with Diff

### The Problem

After editing a prompt, the user wants to know: "Did my output improve?"

### Solution: Output Diff View

When a task is re-run, the playground stores both the previous and current output. The VS Code extension can show a diff:

```typescript
// VS Code command: nika.playground.showDiff
vscode.commands.executeCommand('vscode.diff',
  previousOutputUri,  // virtual document with previous output
  currentOutputUri,   // virtual document with current output
  `${taskId}: Run #${runNumber - 1} vs #${runNumber}`
);
```

### LSP-Side Storage

```rust
/// Per-task run history (ring buffer, last N runs)
struct TaskRunHistory {
    task_id: Arc<str>,
    runs: VecDeque<PlaygroundRun>,  // max 10 entries
}

struct PlaygroundRun {
    /// Run sequence number
    run_number: u32,
    /// The prompt that was used (after template resolution)
    resolved_prompt: String,
    /// The raw output
    output: String,
    /// Execution metadata
    metadata: RunMetadata,
    /// Timestamp
    timestamp: SystemTime,
}

struct RunMetadata {
    duration_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    provider: String,
    model: String,
}
```

### Diff Modes

| Mode | What | How |
|------|------|-----|
| **Side-by-side** | VS Code diff editor | `vscode.diff` command with virtual document URIs |
| **Inline delta** | Inlay hint shows "+12 chars, -3 chars" | Computed by LSP, shown as decoration |
| **Semantic diff** | For JSON output: show added/removed/changed keys | Custom differ in LSP, rendered in webview |

---

## 5. Mock Mode

### Purpose

Run the DAG to test structure, bindings, and templates WITHOUT making real API calls (no cost, no latency).

### Mock Provider Implementation

```rust
/// Mock provider that returns deterministic outputs
pub struct MockProvider {
    /// Strategy for generating mock outputs
    strategy: MockStrategy,
}

pub enum MockStrategy {
    /// Return the prompt itself (echo mode — useful for testing templates)
    Echo,
    /// Return a fixed string per verb type
    FixedPerVerb {
        infer: String,   // default: "[mock infer output for {task_id}]"
        exec_output: String,    // default: "[mock output]"
        fetch: String,   // default: '{"mock": true}'
        invoke: String,  // default: '{"result": "mock"}'
        agent: String,   // default: "[mock agent output after 1 turn]"
    },
    /// Return content from a `.mock.json` sidecar file
    /// e.g., `workflow.nika.yaml` looks for `workflow.mock.json`
    Sidecar(PathBuf),
    /// Return content from previous real runs (replay mode)
    Replay(PlaygroundCache),
}
```

### What Mock Mode Validates

1. **Template resolution**: `{{with.data}}` resolves correctly (mock outputs propagate through bindings)
2. **DAG structure**: Dependencies run in correct order
3. **Output schema**: If `output.format: json` with a schema, mock output can be pre-shaped to match
4. **for_each expansion**: Array outputs expand correctly
5. **Binding chains**: Multi-hop `with:` references resolve
6. **Artifact paths**: Template paths in `artifact:` resolve without writing real files

### What Mock Mode Does NOT Validate

1. Actual LLM output quality
2. Real HTTP responses from `fetch:`
3. MCP server availability
4. Shell command behavior (mocked to echo)

### Mock Mode UX

```
Code Lens: [Run Task] [Run Mock] [Run Workflow Mock]
                       ^^^^^^^^^
                       Uses MockProvider, shows:
                       "Mock: DAG valid, 5/5 tasks completed, 3 bindings resolved"
```

---

## 6. LSP-Runner Communication Architecture

### In-Process Design

The LSP server holds a long-lived `PlaygroundRunner` that shares the `TaskExecutor`'s provider cache. This means the second `infer:` call reuses the already-initialized provider — zero cold start.

```rust
/// Long-lived playground runner, held by NikaLanguageServer
pub struct PlaygroundRunner {
    /// Shared executor with provider cache
    executor: TaskExecutor,
    /// Per-session result cache
    cache: PlaygroundCache,
    /// Active runs (for cancellation)
    active_runs: DashMap<String, CancellationToken>,
    /// Run history per task
    history: DashMap<Arc<str>, TaskRunHistory>,
    /// Security gate
    security: PlaygroundSecurity,
    /// Event log with broadcast for streaming results
    event_log: EventLog,
}
```

### Lifecycle

```
LSP Initialize
    |
    v
PlaygroundRunner::new()  // lazy — created on first playground request
    |
    v
[idle — no resources consumed]
    |
    v  (user clicks "Run Task")
PlaygroundRunner::run_task(task_id, workflow, uri)
    |
    +--> Parse workflow (reuse LSP's cached AnalyzedWorkflow)
    +--> Resolve dependencies from cache
    +--> Lower single task to TaskAction
    +--> TaskExecutor::execute()
    |       |
    |       +--> Events stream via EventLog broadcast
    |       |       |
    |       |       +--> LSP sends nika/playground/stream notifications
    |       |
    |       +--> Result returned
    |
    +--> Cache result
    +--> Send nika/playground/result notification
    |
    v
[idle — executor + provider cache stay warm]
```

### Event Streaming

The `EventLog::new_with_broadcast()` pattern (already used by TUI) enables real-time progress reporting:

```rust
// In PlaygroundRunner::run_task
let (event_log, mut rx) = EventLog::new_with_broadcast();

// Spawn event forwarder
let client = self.lsp_client.clone();
let task_id = task_id.clone();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        // Forward as LSP notification
        client.send_notification::<PlaygroundStream>(PlaygroundStreamParams {
            task_id: task_id.clone(),
            event: event.kind.clone(),
        }).await;
    }
});
```

This means the VS Code extension receives events like `ProviderCalled`, `ProviderResponded`, `TemplateResolved` in real-time and can show a progress indicator.

---

## 7. UX: How Results Appear

### Option A: Inlay Hints (recommended for short outputs)

```yaml
tasks:
  - id: research
    infer:
      model: claude-sonnet-4-5-20250514
      prompt: |
        What is the capital of France?
                                          # <- inlay hint appears here
                                          # "Paris" [128 tokens, $0.001, 0.8s]
```

**Implementation**: The LSP returns inlay hints with `InlayHintKind::Parameter` at the end of the task block. The hint includes:
- First line of output (truncated to 80 chars)
- Token count, cost, duration
- Click to expand full output

**Limitation**: Inlay hints are text-only. No rich formatting. Good for single-line outputs, bad for multi-paragraph LLM responses.

### Option B: Virtual Documents (recommended for long outputs)

Register a `TextDocumentContentProvider` for `nika-playground:` URIs:

```
nika-playground:/workspace/workflow.nika.yaml/research?run=3
```

The virtual document shows the full output with metadata header:

```markdown
# Task: research (Run #3)
# Model: claude-sonnet-4-5-20250514 | Provider: anthropic
# Tokens: 45 in / 1,284 out | Cost: $0.019
# Duration: 2.3s | Status: success

Quantum computing is a type of computing that harnesses quantum mechanical
phenomena like superposition and entanglement...
```

### Option C: Decorations + Output Channel (pragmatic default)

Combine three mechanisms:
1. **Gutter icon**: Green check or red X next to the task `id:` line
2. **Status bar**: "Task: research | 1.2s | $0.002"
3. **Output channel**: `Nika Playground` output channel with full results

```typescript
// Create output channel
const output = vscode.window.createOutputChannel('Nika Playground', 'markdown');

// On result:
output.appendLine(`\n## ${taskId} (Run #${runNumber})`);
output.appendLine(`Model: ${model} | Tokens: ${inputTokens}/${outputTokens} | Cost: $${cost}`);
output.appendLine(`Duration: ${durationMs}ms\n`);
output.appendLine(result);
output.show(true); // reveal but don't steal focus
```

### Recommended Default

**Option C (decorations + output channel)** as the default. It is the least intrusive and works with every VS Code theme. Option B (virtual documents) available via "Show Full Output" action. Option A (inlay hints) available as a user setting for users who want maximum inline feedback.

User setting:

```json
{
  "nika.playground.resultDisplay": "output-channel" | "virtual-document" | "inlay-hint"
}
```

---

## 8. Security Model

### Threat Model

The playground runs arbitrary code from the editor. The threats:

| Threat | Verb | Severity | Mitigation |
|--------|------|----------|------------|
| Arbitrary shell execution | `exec:` | Critical | Require explicit confirmation per run |
| Data exfiltration via prompt | `infer:` | Medium | User is running their own prompts |
| MCP tool invocation | `invoke:` | Medium | Inherit workflow's MCP config |
| Uncontrolled HTTP requests | `fetch:` | Low | Standard fetch, user-visible URL |
| Resource exhaustion | `agent:` | Medium | Enforce max_turns=5 in playground |
| Cost runaway | `infer:`/`agent:` | Medium | Per-session cost cap, confirmation above threshold |

### Security Gates

```rust
pub struct PlaygroundSecurity {
    /// VS Code Workspace Trust state (sent by extension)
    workspace_trusted: bool,
    /// Per-session cost accumulator
    session_cost_usd: AtomicF64,
    /// Cost threshold before requiring confirmation (default: $1.00)
    cost_confirmation_threshold: f64,
    /// Verbs that require explicit confirmation
    confirmation_required: HashSet<String>,  // default: {"exec"}
    /// Max agent turns in playground mode
    max_agent_turns: u32,  // default: 5
}

impl PlaygroundSecurity {
    /// Check if a task can run without confirmation
    fn needs_confirmation(&self, action: &TaskAction) -> Option<ConfirmationReason> {
        // 1. Workspace Trust
        if !self.workspace_trusted {
            return Some(ConfirmationReason::UntrustedWorkspace);
        }

        // 2. exec: always requires confirmation (arbitrary shell)
        if matches!(action, TaskAction::Exec { .. }) {
            return Some(ConfirmationReason::ShellExecution);
        }

        // 3. Cost threshold exceeded
        let current_cost = self.session_cost_usd.load(Ordering::Relaxed);
        if current_cost > self.cost_confirmation_threshold {
            return Some(ConfirmationReason::CostThreshold {
                current: current_cost,
                threshold: self.cost_confirmation_threshold,
            });
        }

        None
    }
}
```

### The Shell Execution Problem

The existing `security.rs` module already blocks dangerous patterns (rm -rf, sudo, pipe-to-shell, etc.). The playground inherits all of these protections. Additionally:

1. **Shell commands require confirmation** — a VS Code dialog appears: "This task runs: `echo hello`. Allow?"
2. **Command preview** — the LSP resolves all templates before showing the confirmation, so the user sees the actual command
3. **In untrusted workspaces** — shell commands are completely blocked (no confirmation option)
4. **Sandbox option** — future: run shell commands in a container/namespace (not MVP)

### Agent Turn Limits

In playground mode, `agent:` tasks have a hard cap:
- `max_turns` is clamped to `min(task.max_turns, playground.max_agent_turns)`
- Default playground max: 5 turns (vs 50 in production)
- Configurable via `nika.playground.maxAgentTurns` setting

---

## 9. Custom LSP Requests

### 9.1 `nika/playground/run` (Request)

Run a single task from the current workflow.

```typescript
// Client -> Server
interface PlaygroundRunParams {
  /** URI of the workflow file */
  textDocument: TextDocumentIdentifier;
  /** Task ID to run */
  taskId: string;
  /** Execution mode */
  mode: 'real' | 'mock';
  /** Override provider/model (optional, for scratch pad) */
  provider?: string;
  model?: string;
  /** Mock strategy when mode='mock' */
  mockStrategy?: 'echo' | 'fixed' | 'replay';
  /** If true, also run unresolved dependencies first */
  runDependencies?: boolean;
}

// Server -> Client (response)
interface PlaygroundRunResult {
  /** Unique run ID for this execution */
  runId: string;
  /** Whether confirmation is needed before execution */
  needsConfirmation?: PlaygroundConfirmation;
}

interface PlaygroundConfirmation {
  reason: 'shell_execution' | 'cost_threshold' | 'untrusted_workspace';
  /** For shell_execution: the resolved command string */
  resolvedCommand?: string;
  /** For cost_threshold: current session cost */
  currentCost?: number;
}
```

### 9.2 `nika/playground/confirm` (Request)

User confirmed the security dialog.

```typescript
// Client -> Server
interface PlaygroundConfirmParams {
  runId: string;
  confirmed: boolean;
}
```

### 9.3 `nika/playground/cancel` (Request)

Cancel an in-progress playground run.

```typescript
// Client -> Server
interface PlaygroundCancelParams {
  runId: string;
}
```

### 9.4 `nika/playground/stream` (Notification, Server -> Client)

Real-time events during execution.

```typescript
// Server -> Client (notification)
interface PlaygroundStreamParams {
  runId: string;
  taskId: string;
  event: PlaygroundEvent;
}

type PlaygroundEvent =
  | { type: 'started'; verb: string }
  | { type: 'template_resolved'; template: string; result: string }
  | { type: 'provider_called'; provider: string; model: string; promptLen: number }
  | { type: 'provider_responded'; inputTokens: number; outputTokens: number;
      costUsd: number; durationMs: number }
  | { type: 'completed'; output: string; durationMs: number }
  | { type: 'failed'; error: string; durationMs: number }
  | { type: 'cancelled' };
```

### 9.5 `nika/playground/result` (Notification, Server -> Client)

Final result of a playground run (sent after completion/failure).

```typescript
// Server -> Client (notification)
interface PlaygroundResultParams {
  runId: string;
  taskId: string;
  status: 'success' | 'failed' | 'cancelled';
  /** Full output text */
  output?: string;
  /** Error message if failed */
  error?: string;
  /** Run metadata */
  metadata: {
    durationMs: number;
    inputTokens: number;
    outputTokens: number;
    costUsd: number;
    provider: string;
    model: string;
    runNumber: number;
  };
  /** Previous output for diffing (if run > 1) */
  previousOutput?: string;
}
```

### 9.6 `nika/playground/scratch` (Request)

Run a freeform prompt (not tied to a workflow task).

```typescript
// Client -> Server
interface PlaygroundScratchParams {
  /** The prompt text */
  prompt: string;
  /** Provider to use */
  provider: string;
  /** Model to use */
  model: string;
  /** Optional system prompt */
  system?: string;
  /** Optional template variables */
  variables?: Record<string, string>;
  /** Temperature */
  temperature?: number;
  /** Max tokens */
  maxTokens?: number;
}

// Server -> Client (response, same as PlaygroundRunResult)
```

### 9.7 `nika/playground/history` (Request)

Get run history for a task.

```typescript
// Client -> Server
interface PlaygroundHistoryParams {
  textDocument: TextDocumentIdentifier;
  taskId: string;
}

// Server -> Client
interface PlaygroundHistoryResult {
  runs: Array<{
    runNumber: number;
    output: string;
    metadata: RunMetadata;
    timestamp: string;
  }>;
}
```

### 9.8 `nika/playground/runChain` (Request)

Run a task and all its unresolved dependencies in topological order.

```typescript
// Client -> Server
interface PlaygroundRunChainParams {
  textDocument: TextDocumentIdentifier;
  /** Terminal task to run (dependencies auto-resolved) */
  taskId: string;
  mode: 'real' | 'mock';
  /** If true, re-run even if cached */
  force?: boolean;
}
```

---

## 10. VS Code Extension Code Sketches

### 10.1 Code Lens Provider

```typescript
import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';

export class NikaPlaygroundCodeLensProvider implements vscode.CodeLensProvider {
  constructor(private client: LanguageClient) {}

  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const lenses: vscode.CodeLens[] = [];
    const text = document.getText();

    // Find task ID lines (simple regex, LSP handles precision)
    const taskIdRegex = /^\s*-\s*id:\s*(\S+)/gm;
    let match;

    while ((match = taskIdRegex.exec(text)) !== null) {
      const line = document.positionAt(match.index).line;
      const range = new vscode.Range(line, 0, line, 0);
      const taskId = match[1];

      // "Run Task" lens
      lenses.push(new vscode.CodeLens(range, {
        title: '$(play) Run',
        command: 'nika.playground.runTask',
        arguments: [document.uri, taskId, 'real'],
      }));

      // "Mock" lens
      lenses.push(new vscode.CodeLens(range, {
        title: '$(beaker) Mock',
        command: 'nika.playground.runTask',
        arguments: [document.uri, taskId, 'mock'],
      }));

      // "Run Chain" lens (only if task has dependencies)
      lenses.push(new vscode.CodeLens(range, {
        title: '$(list-tree) Run Chain',
        command: 'nika.playground.runChain',
        arguments: [document.uri, taskId],
      }));
    }

    return lenses;
  }
}
```

### 10.2 Run Task Command

```typescript
async function runTask(
  client: LanguageClient,
  output: vscode.OutputChannel,
  uri: vscode.Uri,
  taskId: string,
  mode: 'real' | 'mock'
) {
  // Check workspace trust
  if (!vscode.workspace.isTrusted) {
    if (mode === 'real') {
      vscode.window.showWarningMessage(
        'Nika Playground requires a trusted workspace for real execution. '
        + 'Use Mock mode instead.'
      );
      return;
    }
  }

  // Send run request
  const result = await client.sendRequest<PlaygroundRunResult>(
    'nika/playground/run',
    {
      textDocument: { uri: uri.toString() },
      taskId,
      mode,
      runDependencies: true,
    }
  );

  // Handle confirmation dialog
  if (result.needsConfirmation) {
    const confirmed = await showConfirmationDialog(result.needsConfirmation);
    await client.sendRequest('nika/playground/confirm', {
      runId: result.runId,
      confirmed,
    });
    if (!confirmed) return;
  }

  // Show progress
  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `Running ${taskId}...`,
      cancellable: true,
    },
    async (progress, token) => {
      // Cancel forwarding
      token.onCancellationRequested(() => {
        client.sendRequest('nika/playground/cancel', { runId: result.runId });
      });

      // Progress updates come via nika/playground/stream notifications
      // (handled by the stream listener below)
      return new Promise<void>((resolve) => {
        const disposable = onPlaygroundResult(result.runId, (res) => {
          disposable.dispose();
          resolve();
        });
      });
    }
  );
}
```

### 10.3 Stream Event Handler

```typescript
function registerPlaygroundHandlers(
  client: LanguageClient,
  output: vscode.OutputChannel
) {
  // Gutter decorations
  const successDecoration = vscode.window.createTextEditorDecorationType({
    gutterIconPath: path.join(__dirname, 'icons', 'check-green.svg'),
    gutterIconSize: '80%',
  });
  const failDecoration = vscode.window.createTextEditorDecorationType({
    gutterIconPath: path.join(__dirname, 'icons', 'x-red.svg'),
    gutterIconSize: '80%',
  });
  const runningDecoration = vscode.window.createTextEditorDecorationType({
    gutterIconPath: path.join(__dirname, 'icons', 'sync-spin.svg'),
    gutterIconSize: '80%',
  });

  // Stream events (real-time progress)
  client.onNotification(
    'nika/playground/stream',
    (params: PlaygroundStreamParams) => {
      const { runId, taskId, event } = params;

      switch (event.type) {
        case 'started':
          setGutterDecoration(taskId, runningDecoration);
          output.appendLine(`[${taskId}] Started (${event.verb})`);
          break;
        case 'provider_called':
          output.appendLine(
            `[${taskId}] Calling ${event.provider}/${event.model}...`
          );
          break;
        case 'provider_responded':
          output.appendLine(
            `[${taskId}] Response: ` +
            `${event.inputTokens}/${event.outputTokens} tokens, ` +
            `$${event.costUsd.toFixed(4)}, ${event.durationMs}ms`
          );
          break;
      }
    }
  );

  // Final results
  client.onNotification(
    'nika/playground/result',
    (params: PlaygroundResultParams) => {
      const {
        taskId, status, output: taskOutput,
        metadata, previousOutput,
      } = params;

      if (status === 'success') {
        setGutterDecoration(taskId, successDecoration);
        output.appendLine(`\n## ${taskId} (Run #${metadata.runNumber}) - SUCCESS`);
        output.appendLine(
          `${metadata.provider}/${metadata.model} | ` +
          `${metadata.inputTokens}/${metadata.outputTokens} tokens | ` +
          `$${metadata.costUsd.toFixed(4)} | ${metadata.durationMs}ms`
        );
        output.appendLine('---');
        output.appendLine(taskOutput ?? '');

        // Offer diff if previous output exists
        if (previousOutput && previousOutput !== taskOutput) {
          vscode.window.showInformationMessage(
            `Output changed from Run #${metadata.runNumber - 1}`,
            'Show Diff'
          ).then((choice) => {
            if (choice === 'Show Diff') {
              showOutputDiff(
                taskId, previousOutput, taskOutput!, metadata.runNumber
              );
            }
          });
        }
      } else {
        setGutterDecoration(taskId, failDecoration);
        output.appendLine(`\n## ${taskId} - FAILED`);
        output.appendLine(params.error ?? 'Unknown error');
      }

      output.show(true);
    }
  );
}
```

### 10.4 Scratch Pad Webview

```typescript
function createScratchPadPanel(
  client: LanguageClient
): vscode.WebviewPanel {
  const panel = vscode.window.createWebviewPanel(
    'nikaScratchPad',
    'Nika Scratch Pad',
    vscode.ViewColumn.Beside,
    {
      enableScripts: true,
      retainContextWhenHidden: true,
    }
  );

  panel.webview.html = getScratchPadHtml();

  // Handle messages from webview
  panel.webview.onDidReceiveMessage(async (message) => {
    switch (message.command) {
      case 'run': {
        const result = await client.sendRequest<PlaygroundRunResult>(
          'nika/playground/scratch',
          {
            prompt: message.prompt,
            provider: message.provider,
            model: message.model,
            system: message.system,
            variables: message.variables,
            temperature: message.temperature,
            maxTokens: message.maxTokens,
          }
        );
        // Stream events update the webview
        break;
      }
      case 'copyToWorkflow': {
        // Generate a task YAML block from scratch pad state
        const yaml = generateTaskYaml(message);
        await vscode.env.clipboard.writeText(yaml);
        vscode.window.showInformationMessage(
          'Task YAML copied to clipboard'
        );
        break;
      }
    }
  });

  return panel;
}
```

---

## 11. Rust Implementation

### 11.1 PlaygroundRunner (New File: `src/lsp/playground.rs`)

```rust
//! LSP Playground Runner — single-task execution with event streaming
//!
//! Provides in-editor task execution for the Nika LSP.
//! Reuses TaskExecutor for zero cold-start, shares provider cache.

use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::ast::analyzed::{AnalyzedTask, AnalyzedWorkflow};
use crate::ast::lower::{lower_action, lower_mcp_servers, lower_output};
use crate::ast::output::OutputPolicy;
use crate::binding::{resolve_bindings, ResolvedBindings};
use crate::dag::Dag;
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::runtime::executor::TaskExecutor;
use crate::store::{RunContext, TaskResult};
use crate::util::intern;

/// Execution mode for playground runs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaygroundMode {
    Real,
    Mock,
}

/// Long-lived playground runner held by the LSP server
pub struct PlaygroundRunner {
    /// Shared task executor (reuses provider cache across runs)
    executor: Option<TaskExecutor>,
    /// Per-session result cache (persists across runs)
    cache: RunContext,
    /// DAG for dependency resolution
    dag: Option<Dag>,
    /// Active run cancellation tokens
    active_runs: DashMap<String, CancellationToken>,
    /// Run history per task (ring buffer)
    history: DashMap<Arc<str>, Vec<PlaygroundRun>>,
    /// Session cost accumulator (f64 bits stored as u64)
    session_cost_usd: std::sync::atomic::AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaygroundRun {
    pub run_number: u32,
    pub output: String,
    pub resolved_prompt: String,
    pub duration_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub provider: String,
    pub model: String,
    pub timestamp_ms: u64,
}

impl PlaygroundRunner {
    pub fn new() -> Self {
        Self {
            executor: None,
            cache: RunContext::new(),
            dag: None,
            active_runs: DashMap::new(),
            history: DashMap::new(),
            session_cost_usd: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Initialize or update executor from workflow analysis
    pub fn update_from_workflow(&mut self, workflow: &AnalyzedWorkflow) {
        let mcp_configs = lower_mcp_servers(workflow.mcp_servers.clone());
        let provider = workflow.provider.as_deref().unwrap_or("claude");
        let event_log = EventLog::new(); // replaced per-run

        self.executor = Some(TaskExecutor::new(
            provider,
            workflow.model.as_deref(),
            mcp_configs,
            event_log,
        ));

        // Rebuild DAG for dependency resolution
        if let Ok(dag) = Dag::from_analyzed(workflow) {
            self.dag = Some(dag);
        }
    }

    /// Run a single task, returning the result and an event log
    pub async fn run_task(
        &self,
        task: &AnalyzedTask,
        workflow: &AnalyzedWorkflow,
        mode: PlaygroundMode,
        cancel_token: CancellationToken,
    ) -> Result<(TaskResult, EventLog), NikaError> {
        let executor = self.executor.as_ref().ok_or_else(|| {
            NikaError::PlaygroundError {
                reason: "Playground not initialized. Open a workflow first."
                    .into(),
            }
        })?;

        // Create per-run event log with broadcast
        let (event_log, _rx) = EventLog::new_with_broadcast();

        let task_id = intern(&task.name);
        let start = Instant::now();

        // Resolve bindings from cache
        let bindings = self.resolve_task_bindings(task, workflow)?;

        // Lower the action
        let action = lower_action(&task.action);

        // Lower output policy
        let output_policy = task.output.as_ref().map(|o| lower_output(o));

        // Execute with the appropriate mode
        let result = match mode {
            PlaygroundMode::Real => {
                executor
                    .execute(
                        &task_id, &action, &bindings,
                        &self.cache, output_policy.as_ref(),
                    )
                    .await
            }
            PlaygroundMode::Mock => {
                Ok(format!(
                    "[mock] Task '{}' would run here",
                    task.name
                ))
            }
        };

        let duration = start.elapsed();

        let task_result = match result {
            Ok(output) => {
                event_log.emit(EventKind::TaskCompleted {
                    task_id: Arc::clone(&task_id),
                    output: Arc::new(Value::String(output.clone())),
                    duration_ms: duration.as_millis() as u64,
                });
                let tr = TaskResult::success_str(output, duration);
                // Cache for downstream tasks
                self.cache.insert(Arc::clone(&task_id), tr.clone());
                tr
            }
            Err(e) => {
                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(&task_id),
                    error: e.to_string(),
                    duration_ms: duration.as_millis() as u64,
                });
                TaskResult::failed(e.to_string(), duration)
            }
        };

        Ok((task_result, event_log))
    }

    /// Resolve bindings for a task using the playground cache
    fn resolve_task_bindings(
        &self,
        task: &AnalyzedTask,
        _workflow: &AnalyzedWorkflow,
    ) -> Result<ResolvedBindings, NikaError> {
        resolve_bindings(&task.bindings, &self.cache, &task.name)
    }

    /// Get unresolved dependencies for a task
    pub fn get_unresolved_deps(&self, task_id: &str) -> Vec<String> {
        let dag = match &self.dag {
            Some(d) => d,
            None => return vec![],
        };

        dag.get_dependencies(task_id)
            .iter()
            .filter(|dep| !self.cache.contains(dep.as_ref()))
            .map(|dep| dep.to_string())
            .collect()
    }

    /// Invalidate cache for a task and all its dependents
    pub fn invalidate(&self, task_id: &str) {
        self.cache.remove(task_id);
        if let Some(dag) = &self.dag {
            for dep in dag.get_dependents(task_id) {
                self.cache.remove(dep.as_ref());
            }
        }
    }

    /// Cancel an active run
    pub fn cancel(&self, run_id: &str) {
        if let Some((_, token)) = self.active_runs.remove(run_id) {
            token.cancel();
        }
    }

    /// Get run history for a task
    pub fn get_history(&self, task_id: &str) -> Vec<PlaygroundRun> {
        self.history
            .get(task_id)
            .map(|h| h.value().clone())
            .unwrap_or_default()
    }
}
```

### 11.2 New Error Codes

Add to `src/error.rs` in the 290-299 range:

```rust
// Error code range: 290-299 (Playground errors)
PlaygroundError { reason: String },            // NIKA-290
PlaygroundSecurityBlocked { reason: String },   // NIKA-291
PlaygroundDependencyMissing {                   // NIKA-292
    task_id: String,
    missing_deps: Vec<String>,
},
PlaygroundCostLimitExceeded {                   // NIKA-293
    current_cost: f64,
    limit: f64,
},
```

### 11.3 Integration with NikaLanguageServer

```rust
// In src/lsp/server.rs — add playground field

#[cfg(feature = "lsp")]
pub struct NikaLanguageServer {
    client: Client,
    documents: Arc<RwLock<DocumentStore>>,
    ast_index: AstIndex,
    /// Playground runner for single-task execution (lazy-initialized)
    playground: Arc<RwLock<PlaygroundRunner>>,
}
```

### 11.4 Mock Provider (New File: `src/runtime/mock_provider.rs`)

```rust
//! Mock provider for playground mode
//!
//! Returns deterministic outputs without making API calls.
//! Zero cost, zero latency, deterministic.

use crate::ast::TaskAction;
use crate::error::NikaError;

pub enum MockStrategy {
    /// Echo the prompt back (useful for testing template resolution)
    Echo,
    /// Return fixed strings per verb type
    Fixed,
    /// Return content from a sidecar .mock.json file
    Sidecar(std::path::PathBuf),
}

pub fn mock_task(
    task_id: &str,
    action: &TaskAction,
    strategy: &MockStrategy,
    resolved_prompt: &str,
) -> Result<String, NikaError> {
    match strategy {
        MockStrategy::Echo => Ok(format!(
            "[mock:echo] {resolved_prompt}"
        )),
        MockStrategy::Fixed => match action {
            TaskAction::Infer { .. } => Ok(format!(
                "[mock:infer] Output for task '{task_id}'"
            )),
            TaskAction::Exec { .. } => Ok(
                "[mock:exec] Command completed".to_string()
            ),
            TaskAction::Fetch { fetch } => Ok(format!(
                r#"{{"mock": true, "url": "{}"}}"#, fetch.url
            )),
            TaskAction::Invoke { invoke } => Ok(format!(
                r#"{{"mock": true, "tool": "{}"}}"#,
                invoke.tool.as_deref().unwrap_or("unknown")
            )),
            TaskAction::Agent { .. } => Ok(format!(
                "[mock:agent] Agent output for '{task_id}' after 1 turn"
            )),
        },
        MockStrategy::Sidecar(path) => {
            let content = std::fs::read_to_string(path).map_err(|e| {
                NikaError::PlaygroundError {
                    reason: format!(
                        "Failed to read mock file {}: {e}",
                        path.display()
                    ),
                }
            })?;
            let mocks: serde_json::Value =
                serde_json::from_str(&content).map_err(|e| {
                    NikaError::PlaygroundError {
                        reason: format!("Invalid mock JSON: {e}"),
                    }
                })?;
            mocks
                .get(task_id)
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| NikaError::PlaygroundError {
                    reason: format!(
                        "No mock data for task '{task_id}' in {}",
                        path.display()
                    ),
                })
        }
    }
}
```

---

## 12. Testing Strategy

### Unit Tests (in `src/lsp/playground.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playground_cache_invalidation() {
        // Seed cache with task_a, task_b
        // Invalidate task_a
        // Assert task_a removed AND dependents removed
    }

    #[test]
    fn test_mock_execute_echo() {
        // Verify echo mode returns prompt text
    }

    #[test]
    fn test_mock_execute_fixed_per_verb() {
        // Test each verb returns appropriate mock output
    }

    #[test]
    fn test_unresolved_deps_detection() {
        // Build a DAG: A -> B -> C
        // Only A is cached
        // get_unresolved_deps("C") should return ["B"]
    }

    #[test]
    fn test_security_exec_requires_confirmation() {
        // exec: verb must require confirmation
    }

    #[test]
    fn test_security_infer_no_confirmation() {
        // infer: verb should NOT require confirmation
    }

    #[test]
    fn test_run_history_ring_buffer() {
        // Verify history keeps max 10 entries
    }

    #[test]
    fn test_cost_accumulator() {
        // Verify session cost accumulates across runs
        // Verify threshold triggers confirmation
    }
}
```

### Integration Tests

```rust
#[cfg(test)]
mod integration_tests {
    /// Test running a real infer task in mock mode
    #[tokio::test]
    async fn test_playground_mock_infer() {
        // Parse -> Analyze -> Run in mock mode
        // Verify mock output returned, no API call made
    }

    /// Test dependency chain resolution
    #[tokio::test]
    async fn test_playground_dependency_chain() {
        // A -> B -> C
        // Run C: should detect A,B as unresolved
        // Run A, then B, then C: should use cached results
    }

    /// Test cache invalidation on edit
    #[tokio::test]
    async fn test_playground_edit_invalidates_cache() {
        // Run A (cached)
        // "Edit" A (change prompt hash)
        // A's cache should be invalidated
    }
}
```

### VS Code Extension Tests

```typescript
suite('Playground', () => {
  test('code lens appears on task IDs', async () => {
    // Open a workflow file
    // Verify code lens provider returns Run/Mock/Chain lenses
  });

  test('mock mode returns mock output', async () => {
    // Send nika/playground/run with mode: 'mock'
    // Verify response has mock output
  });

  test('shell commands require confirmation dialog', async () => {
    // Send run request for task with exec: verb
    // Verify needsConfirmation is returned
  });
});
```

---

## 13. Phased Rollout

### Phase A: Foundation (1 PR, ~1 week)

**PR: Playground infrastructure**

Files created:
- `src/lsp/playground.rs` — PlaygroundRunner, PlaygroundCache, PlaygroundSecurity
- `src/runtime/mock_provider.rs` — MockStrategy, mock_task

What works:
- `nika/playground/run` with mock mode only
- Cache invalidation on document edit
- Security gate (blocks shell commands in untrusted workspaces)

What does not work yet:
- No real execution (mock only)
- No event streaming
- No VS Code extension changes

### Phase B: Real Execution + Streaming (1 PR, ~1 week)

**PR: Live task execution**

What is added:
- Real mode execution via TaskExecutor
- Event streaming via `nika/playground/stream`
- Confirmation dialog for shell tasks
- Cost tracking per session

VS Code extension:
- Code lens: "Run Task" / "Mock" on each task
- Output channel: `Nika Playground`
- Gutter decorations (success/failure)
- Progress notification with cancel

### Phase C: Scratch Pad + History (1 PR, ~1 week)

**PR: Scratch pad and run history**

What is added:
- `nika/playground/scratch` request
- Run history per task (ring buffer)
- `nika/playground/history` request
- Output diff view

VS Code extension:
- Scratch Pad webview panel
- "Show Diff" action on changed outputs
- History view in scratch pad

### Phase D: Dependency Chain + Advanced Mock (1 PR, ~1 week)

**PR: Smart dependency resolution**

What is added:
- `nika/playground/runChain` — run task + all unresolved deps
- Sidecar mock files (`.mock.json`)
- Replay mock mode (use outputs from previous real runs)
- Agent turn limits in playground mode

---

## Design Decisions Summary

| Decision | Choice | Why |
|----------|--------|-----|
| LSP-Runner communication | In-process (shared library) | Zero cold start, shared provider cache, event streaming |
| Crash isolation | tokio::task::spawn + JoinError catch | Panic in executor does not crash LSP |
| Cache scope | Per-session (editor lifetime) | Natural for iterative development |
| Cache invalidation | Hash-based + transitive dependents | Avoids stale results without full re-run |
| Default result display | Output channel + gutter | Least intrusive, works everywhere |
| Security for shell commands | Confirmation dialog + blocklist | Arbitrary shell is the only real threat |
| Mock strategy | Echo/Fixed/Sidecar/Replay | Covers all use cases from testing to demo |
| Event streaming | EventLog broadcast (existing) | Reuses TUI pattern, zero new infrastructure |
| Scratch pad | Webview panel | Rich UI for model switching, variable editing |
| History storage | In-memory ring buffer (10 per task) | No persistence needed for dev iteration |
| Error code range | NIKA-290-299 | Follows existing convention, no conflicts |

---

## Open Questions

1. **Should the playground cache persist to disk?** Currently in-memory only. Disk persistence would survive VS Code restarts but adds complexity. Decision: defer to user feedback.

2. **Should mock mode support JSON Schema output shaping?** If a task has `output.schema`, the mock could generate conforming JSON. Decision: nice-to-have for Phase D.

3. **Should the scratch pad support agent: mode?** Multi-turn agents in a scratch pad is complex. Decision: start with infer: only, add agent: in v2.

4. **Multi-file playground?** If task A is in `lib/common.nika.yaml` and task B includes it, the playground needs cross-file support. Decision: Phase D, depends on LSP master plan PR 3 (handler migration with multi-file support).
