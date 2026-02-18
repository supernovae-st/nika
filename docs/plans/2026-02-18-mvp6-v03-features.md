# MVP 6: v0.3 Features - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete Nika v0.3 with parallel task execution, multi-crate workspace, comprehensive documentation, and NovaNet enhancements.

**Architecture:** Workspace split into focused crates (nika-core, nika-mcp, nika-tui), parallel task execution via tokio::JoinSet, user-facing documentation.

**Tech Stack:** Cargo workspace, tokio::JoinSet, mdbook for docs

**Prerequisites:** MVP 5 completed (production hardening done)

---

## Task 1: Design for_each Parallelism

**Files:**
- Create: `nika-dev/docs/adr/005-foreach-parallelism.md`
- Create: `nika-dev/tools/nika/src/ast/foreach.rs`

**Step 1: Write ADR**

```markdown
# ADR-005: for_each Parallel Task Execution

## Status
Accepted

## Context
Nika workflows need to process lists of items in parallel. Current execution is sequential.

## Decision
Add `for_each:` verb that:
1. Takes a list of items
2. Executes a template task for each item in parallel
3. Collects results into a list

### Syntax

```yaml
tasks:
  generate_all:
    for_each:
      items: $locales  # List from previous task or inline
      as: locale
      max_parallel: 5  # Concurrency limit
      task:
        invoke: novanet_generate
        params:
          entity: "qr-code"
          locale: $locale
```

### Execution Model

1. Parse items list
2. Create JoinSet with max_parallel semaphore
3. Spawn tasks, each with item variable substituted
4. Collect results preserving order
5. Store as list in context

## Consequences

- Parallel execution improves throughput
- Memory usage proportional to items × task size
- Error handling: fail-fast or collect-all configurable
```

**Step 2: Create ForEachParams struct**

```rust
// src/ast/foreach.rs
use serde::{Deserialize, Serialize};
use super::action::TaskAction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForEachParams {
    /// Source of items (context reference or inline list)
    pub items: ItemsSource,
    /// Variable name for current item
    #[serde(rename = "as")]
    pub as_var: String,
    /// Maximum parallel executions
    #[serde(default = "default_max_parallel")]
    pub max_parallel: usize,
    /// Task template to execute for each item
    pub task: Box<TaskAction>,
    /// Error handling strategy
    #[serde(default)]
    pub on_error: OnErrorStrategy,
}

fn default_max_parallel() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemsSource {
    /// Reference to context variable
    Reference(String),
    /// Inline list
    Inline(Vec<serde_json::Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnErrorStrategy {
    /// Stop all tasks on first error
    #[default]
    FailFast,
    /// Continue and collect all errors
    CollectErrors,
    /// Skip failed items
    Skip,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_foreach() {
        let yaml = r#"
items: $locales
as: locale
max_parallel: 5
task:
  invoke: novanet_generate
  params:
    entity: qr-code
    locale: $locale
"#;

        let params: ForEachParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.as_var, "locale");
        assert_eq!(params.max_parallel, 5);
    }

    #[test]
    fn test_inline_items() {
        let yaml = r#"
items:
  - "en-US"
  - "fr-FR"
  - "de-DE"
as: locale
task:
  exec: "echo $locale"
"#;

        let params: ForEachParams = serde_yaml::from_str(yaml).unwrap();
        if let ItemsSource::Inline(items) = params.items {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected inline items");
        }
    }
}
```

**Step 3: Run tests**

Run: `cargo test test_parse_foreach`
Expected: PASS

**Step 4: Commit**

```bash
git add docs/adr/005-foreach-parallelism.md src/ast/foreach.rs
git commit -m "feat(ast): add for_each params structure

- ForEachParams with items, as, max_parallel, task
- ItemsSource: Reference or Inline
- OnErrorStrategy: FailFast, CollectErrors, Skip
- ADR-005 documenting design

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Implement Parallel Task Execution

**Files:**
- Create: `nika-dev/tools/nika/src/runtime/foreach.rs`
- Modify: `nika-dev/tools/nika/src/runtime/executor.rs`

**Step 1: Write failing test**

```rust
// src/runtime/foreach.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_foreach_parallel_execution() {
        let items = vec!["a", "b", "c", "d", "e"];
        let executor = ForEachExecutor::new(ForEachConfig {
            max_parallel: 3,
            on_error: OnErrorStrategy::FailFast,
        });

        let start = std::time::Instant::now();
        let results = executor.execute(
            items,
            |item| async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                Ok::<_, String>(format!("processed_{}", item))
            }
        ).await.unwrap();

        let duration = start.elapsed();

        assert_eq!(results.len(), 5);
        // Should be ~200ms (2 batches of 3, then 2), not 500ms (sequential)
        assert!(duration < std::time::Duration::from_millis(300));
    }

    #[tokio::test]
    async fn test_foreach_fail_fast() {
        let items = vec![1, 2, 3, 4, 5];
        let executor = ForEachExecutor::new(ForEachConfig {
            max_parallel: 5,
            on_error: OnErrorStrategy::FailFast,
        });

        let results = executor.execute(
            items,
            |item| async move {
                if item == 3 {
                    Err("item 3 failed".to_string())
                } else {
                    Ok(item * 2)
                }
            }
        ).await;

        assert!(results.is_err());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_foreach`
Expected: FAIL

**Step 3: Implement ForEachExecutor**

```rust
// src/runtime/foreach.rs
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use crate::ast::foreach::OnErrorStrategy;

#[derive(Debug, Clone)]
pub struct ForEachConfig {
    pub max_parallel: usize,
    pub on_error: OnErrorStrategy,
}

impl Default for ForEachConfig {
    fn default() -> Self {
        Self {
            max_parallel: 10,
            on_error: OnErrorStrategy::FailFast,
        }
    }
}

pub struct ForEachExecutor {
    config: ForEachConfig,
}

impl ForEachExecutor {
    pub fn new(config: ForEachConfig) -> Self {
        Self { config }
    }

    pub async fn execute<T, R, E, F, Fut>(
        &self,
        items: Vec<T>,
        operation: F,
    ) -> Result<Vec<R>, ForEachError<E>>
    where
        T: Send + 'static,
        R: Send + 'static,
        E: Send + 'static + std::fmt::Debug,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<R, E>> + Send + 'static,
    {
        let semaphore = Arc::new(Semaphore::new(self.config.max_parallel));
        let operation = Arc::new(operation);
        let mut join_set = JoinSet::new();

        // Track original order
        let total = items.len();
        let mut results: Vec<Option<R>> = (0..total).map(|_| None).collect();
        let mut errors: Vec<(usize, E)> = Vec::new();

        for (index, item) in items.into_iter().enumerate() {
            let sem = semaphore.clone();
            let op = operation.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let result = op(item).await;
                (index, result)
            });
        }

        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((index, Ok(result))) => {
                    results[index] = Some(result);
                }
                Ok((index, Err(e))) => {
                    match self.config.on_error {
                        OnErrorStrategy::FailFast => {
                            join_set.abort_all();
                            return Err(ForEachError::ItemFailed { index, error: e });
                        }
                        OnErrorStrategy::CollectErrors => {
                            errors.push((index, e));
                        }
                        OnErrorStrategy::Skip => {
                            // Just don't add to results
                        }
                    }
                }
                Err(join_error) => {
                    return Err(ForEachError::JoinError(join_error.to_string()));
                }
            }
        }

        if !errors.is_empty() {
            return Err(ForEachError::MultipleFailures(
                errors.into_iter().map(|(i, e)| format!("[{}] {:?}", i, e)).collect()
            ));
        }

        // Collect results, filtering out None (skipped)
        let final_results: Vec<R> = results.into_iter().flatten().collect();
        Ok(final_results)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ForEachError<E> {
    #[error("Item {index} failed: {error:?}")]
    ItemFailed { index: usize, error: E },

    #[error("Multiple failures: {0:?}")]
    MultipleFailures(Vec<String>),

    #[error("Join error: {0}")]
    JoinError(String),
}
```

**Step 4: Integrate into executor**

```rust
// src/runtime/executor.rs - add TaskAction::ForEach handling
TaskAction::ForEach(params) => {
    let items = self.resolve_items(&params.items, context)?;
    let executor = ForEachExecutor::new(ForEachConfig {
        max_parallel: params.max_parallel,
        on_error: params.on_error.clone(),
    });

    let results = executor.execute(
        items,
        |item| {
            let mut task_context = context.clone();
            task_context.insert(params.as_var.clone(), item);
            let task = params.task.clone();
            async move {
                self.execute_action(&task, &task_context).await
            }
        }
    ).await?;

    Ok(TaskOutput {
        output: serde_json::to_string(&results)?,
        success: true,
        ..Default::default()
    })
}
```

**Step 5: Run tests**

Run: `cargo test test_foreach`
Expected: PASS

**Step 6: Commit**

```bash
git add src/runtime/foreach.rs src/runtime/executor.rs
git commit -m "feat(runtime): implement parallel for_each execution

- ForEachExecutor with semaphore-based concurrency
- Preserve result ordering
- FailFast, CollectErrors, Skip strategies
- Integrate into task executor

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Split into Multi-Crate Workspace

**Files:**
- Create: `nika-dev/Cargo.toml` (workspace root)
- Create: `nika-dev/crates/nika-core/Cargo.toml`
- Create: `nika-dev/crates/nika-mcp/Cargo.toml`
- Create: `nika-dev/crates/nika-tui/Cargo.toml`
- Modify: `nika-dev/tools/nika/Cargo.toml`

**Step 1: Create workspace root**

```toml
# nika-dev/Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/nika-core",
    "crates/nika-mcp",
    "crates/nika-tui",
    "tools/nika",
]

[workspace.package]
version = "0.3.0"
edition = "2021"
authors = ["SuperNovae Studio"]
license = "MIT"
repository = "https://github.com/SuperNovae-studio/nika"

[workspace.dependencies]
# Core
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
thiserror = "1.0"
anyhow = "1.0"
tracing = "0.1"

# Async
async-trait = "0.1"
parking_lot = "0.12"

# MCP
rmcp = "0.3"

# TUI (optional)
ratatui = "0.29"
crossterm = "0.28"

# Testing
tokio-test = "0.4"
```

**Step 2: Create nika-core crate**

```toml
# crates/nika-core/Cargo.toml
[package]
name = "nika-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
thiserror.workspace = true
tracing.workspace = true
async-trait.workspace = true
parking_lot.workspace = true
```

```rust
// crates/nika-core/src/lib.rs
//! Nika Core - AST, DAG, and execution primitives

pub mod ast;
pub mod dag;
pub mod error;
pub mod event;
pub mod runtime;
pub mod resilience;
pub mod metrics;

pub use error::NikaError;
pub use ast::Workflow;
pub use runtime::Runner;
```

**Step 3: Create nika-mcp crate**

```toml
# crates/nika-mcp/Cargo.toml
[package]
name = "nika-mcp"
version.workspace = true
edition.workspace = true

[dependencies]
nika-core = { path = "../nika-core" }
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
rmcp.workspace = true
```

```rust
// crates/nika-mcp/src/lib.rs
//! Nika MCP - Model Context Protocol client

pub mod client;
pub mod types;

pub use client::McpClient;
pub use types::{McpConfig, ToolCallResult};
```

**Step 4: Create nika-tui crate**

```toml
# crates/nika-tui/Cargo.toml
[package]
name = "nika-tui"
version.workspace = true
edition.workspace = true

[dependencies]
nika-core = { path = "../nika-core" }
tokio.workspace = true
ratatui.workspace = true
crossterm.workspace = true
```

```rust
// crates/nika-tui/src/lib.rs
//! Nika TUI - Terminal User Interface

pub mod app;
pub mod ui;
pub mod panels;
pub mod event_loop;

pub use app::App;
```

**Step 5: Update main CLI crate**

```toml
# tools/nika/Cargo.toml
[package]
name = "nika"
version.workspace = true
edition.workspace = true

[[bin]]
name = "nika"
path = "src/main.rs"

[dependencies]
nika-core = { path = "../../crates/nika-core" }
nika-mcp = { path = "../../crates/nika-mcp" }
nika-tui = { path = "../../crates/nika-tui", optional = true }

clap = { version = "4.4", features = ["derive"] }
tokio.workspace = true

[features]
default = []
tui = ["nika-tui"]
```

**Step 6: Move code to respective crates**

```bash
# Move files (conceptual - do manually)
# src/ast/ → crates/nika-core/src/ast/
# src/dag/ → crates/nika-core/src/dag/
# src/error.rs → crates/nika-core/src/error.rs
# src/event/ → crates/nika-core/src/event/
# src/runtime/ → crates/nika-core/src/runtime/
# src/resilience/ → crates/nika-core/src/resilience/
# src/metrics/ → crates/nika-core/src/metrics/
# src/mcp/ → crates/nika-mcp/src/
# src/tui/ → crates/nika-tui/src/
```

**Step 7: Verify compilation**

Run: `cargo build --workspace`
Expected: All crates compile

**Step 8: Commit**

```bash
git add Cargo.toml crates/ tools/nika/Cargo.toml
git commit -m "refactor: split into multi-crate workspace

- nika-core: AST, DAG, runtime, events
- nika-mcp: MCP client
- nika-tui: Terminal UI (optional)
- nika: CLI binary

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Write User Documentation (README)

**Files:**
- Create: `nika-dev/README.md`
- Create: `nika-dev/docs/getting-started.md`

**Step 1: Create main README**

```markdown
# Nika

> Semantic YAML workflow engine for AI applications

Nika executes multi-step AI workflows defined in YAML. It integrates with LLM providers (Claude, OpenAI) and external tools via Model Context Protocol (MCP).

## Features

- **5 Semantic Verbs**: `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`
- **DAG Execution**: Automatic dependency resolution and parallel execution
- **MCP Integration**: Connect to any MCP server (like NovaNet)
- **Observability**: NDJSON traces, real-time TUI, CLI trace commands
- **Resilience**: Retry, circuit breaker, rate limiting

## Installation

```bash
cargo install nika

# Or from source
git clone https://github.com/SuperNovae-studio/nika
cd nika && cargo install --path tools/nika
```

## Quick Start

```yaml
# hello.yaml
name: hello-world
version: "1.0"

tasks:
  greet:
    infer: "Write a friendly greeting"
    provider: claude
    model: claude-sonnet-4-20250514
```

```bash
nika run hello.yaml
```

## Workflow Syntax

### Basic Task

```yaml
tasks:
  my_task:
    infer: "Your prompt here"
    provider: claude
    model: claude-sonnet-4-20250514
```

### Dependencies

```yaml
tasks:
  fetch_data:
    fetch: "https://api.example.com/data"

  process:
    depends_on: [fetch_data]
    infer: "Process this data: $fetch_data.result"
```

### MCP Integration

```yaml
mcp:
  novanet:
    command: node
    args: ["path/to/novanet-mcp/dist/index.js"]

tasks:
  generate:
    invoke: novanet_generate
    params:
      entity: "qr-code"
      locale: "en-US"
```

### Agent Mode

```yaml
tasks:
  research:
    agent: "Research and summarize the topic"
    provider: claude
    tools:
      - novanet_describe
      - novanet_traverse
    max_turns: 10
```

### Parallel Execution

```yaml
tasks:
  generate_all:
    for_each:
      items: ["en-US", "fr-FR", "de-DE"]
      as: locale
      max_parallel: 3
      task:
        invoke: novanet_generate
        params:
          entity: "qr-code"
          locale: $locale
```

## CLI Commands

```bash
# Run a workflow
nika run workflow.yaml

# Run with TUI (requires --features tui)
nika tui workflow.yaml

# Validate syntax
nika validate workflow.yaml

# List traces
nika trace list

# Show trace details
nika trace show <trace-id>

# Export trace
nika trace export <trace-id> --format json
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Claude API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `NIKA_TRACE_DIR` | Trace output directory (default: `.nika/traces`) |

## Documentation

- [Getting Started](docs/getting-started.md)
- [Workflow Reference](docs/workflow-reference.md)
- [MCP Integration](docs/mcp-integration.md)
- [Observability](docs/observability.md)

## License

MIT
```

**Step 2: Create getting-started guide**

```markdown
# Getting Started with Nika

This guide walks you through your first Nika workflow.

## Prerequisites

- Rust 1.75+ (for building from source)
- An API key for Claude or OpenAI
- (Optional) Node.js for MCP servers

## Installation

### From Cargo

```bash
cargo install nika
```

### From Source

```bash
git clone https://github.com/SuperNovae-studio/nika
cd nika
cargo install --path tools/nika

# With TUI support
cargo install --path tools/nika --features tui
```

## Your First Workflow

Create `hello.yaml`:

```yaml
name: hello-world
version: "1.0"

tasks:
  greet:
    infer: |
      Write a friendly greeting for someone
      learning to use Nika for the first time.
    provider: claude
    model: claude-sonnet-4-20250514
```

Run it:

```bash
export ANTHROPIC_API_KEY=sk-...
nika run hello.yaml
```

## Adding Dependencies

Tasks can depend on each other:

```yaml
name: chained-tasks
version: "1.0"

tasks:
  fetch_joke:
    fetch: "https://official-joke-api.appspot.com/random_joke"

  explain:
    depends_on: [fetch_joke]
    infer: |
      Explain why this joke is funny:
      $fetch_joke.result
    provider: claude
```

## Using Context

Pass data between tasks:

```yaml
tasks:
  get_topic:
    exec: "echo 'quantum computing'"

  research:
    depends_on: [get_topic]
    context: $get_topic.result
    infer: "Explain $context to a 10 year old"
```

## Next Steps

- Learn about [MCP Integration](mcp-integration.md)
- Explore [Agent Mode](agent-mode.md)
- Set up [Observability](observability.md)
```

**Step 3: Commit**

```bash
git add README.md docs/
git commit -m "docs: add comprehensive user documentation

- README with installation, quick start, examples
- Getting started guide
- CLI reference

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Create examples/ Directory with Guides

**Files:**
- Create: `nika-dev/examples/README.md`
- Create: `nika-dev/examples/01-hello-world.yaml`
- Create: `nika-dev/examples/02-chained-tasks.yaml`
- Create: `nika-dev/examples/03-mcp-integration.yaml`
- Create: `nika-dev/examples/04-agent-mode.yaml`
- Create: `nika-dev/examples/05-parallel-execution.yaml`

**Step 1: Create examples README**

```markdown
# Nika Examples

This directory contains example workflows demonstrating Nika features.

## Examples

| # | File | Description |
|---|------|-------------|
| 1 | `01-hello-world.yaml` | Basic inference task |
| 2 | `02-chained-tasks.yaml` | Task dependencies |
| 3 | `03-mcp-integration.yaml` | Using MCP tools (NovaNet) |
| 4 | `04-agent-mode.yaml` | Multi-turn agent with tools |
| 5 | `05-parallel-execution.yaml` | for_each parallelism |

## Running Examples

```bash
# Run any example
nika run examples/01-hello-world.yaml

# Run with TUI
nika tui examples/04-agent-mode.yaml

# Validate before running
nika validate examples/*.yaml
```

## Prerequisites

- Examples 1-2: Just need `ANTHROPIC_API_KEY`
- Examples 3-5: Need NovaNet MCP server running
```

**Step 2: Create example workflows**

```yaml
# examples/01-hello-world.yaml
name: hello-world
version: "1.0"
description: Basic inference example

tasks:
  greet:
    infer: Write a haiku about coding
    provider: claude
    model: claude-sonnet-4-20250514
```

```yaml
# examples/02-chained-tasks.yaml
name: chained-tasks
version: "1.0"
description: Demonstrates task dependencies

tasks:
  fetch_data:
    fetch: "https://api.github.com/repos/anthropics/anthropic-sdk-python"

  summarize:
    depends_on: [fetch_data]
    infer: |
      Summarize this GitHub repository:
      $fetch_data.result
    provider: claude

  translate:
    depends_on: [summarize]
    infer: |
      Translate this to French:
      $summarize.result
    provider: claude
```

```yaml
# examples/03-mcp-integration.yaml
name: mcp-integration
version: "1.0"
description: Using NovaNet MCP tools

mcp:
  novanet:
    command: node
    args:
      - ../novanet-dev/tools/novanet-mcp/dist/index.js
    env:
      NEO4J_URI: bolt://localhost:7687

tasks:
  generate_content:
    invoke: novanet_generate
    params:
      entity: "qr-code"
      locale: "en-US"
      forms:
        - text
        - title
        - denomination_forms
```

```yaml
# examples/04-agent-mode.yaml
name: agent-mode
version: "1.0"
description: Multi-turn agent with tool use

mcp:
  novanet:
    command: node
    args:
      - ../novanet-dev/tools/novanet-mcp/dist/index.js

tasks:
  research_entity:
    agent: |
      Research the "qr-code" entity in our knowledge graph.
      Find out:
      1. What related entities exist
      2. What content has been generated for different locales
      3. Any SEO recommendations

      Use the available tools to explore the graph.
    provider: claude
    model: claude-sonnet-4-20250514
    tools:
      - novanet_describe
      - novanet_traverse
      - novanet_generate
    max_turns: 10
```

```yaml
# examples/05-parallel-execution.yaml
name: parallel-execution
version: "1.0"
description: Parallel task execution with for_each

mcp:
  novanet:
    command: node
    args:
      - ../novanet-dev/tools/novanet-mcp/dist/index.js

tasks:
  # Generate content for multiple locales in parallel
  generate_all_locales:
    for_each:
      items:
        - "en-US"
        - "fr-FR"
        - "de-DE"
        - "es-ES"
        - "ja-JP"
      as: locale
      max_parallel: 3
      task:
        invoke: novanet_generate
        params:
          entity: "qr-code"
          locale: $locale
          forms:
            - text
            - title

  # Summarize all generated content
  summarize:
    depends_on: [generate_all_locales]
    infer: |
      Here is content generated for multiple locales:
      $generate_all_locales.result

      Create a summary comparing the content across locales.
    provider: claude
```

**Step 3: Run validation**

Run: `cargo run -- validate examples/*.yaml`
Expected: All examples valid

**Step 4: Commit**

```bash
git add examples/
git commit -m "docs: add example workflows

- 01: Basic hello world
- 02: Chained task dependencies
- 03: MCP integration with NovaNet
- 04: Agent mode with tools
- 05: Parallel execution with for_each

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: NovaNet: Add context_build_log to Generate

**Files:**
- Modify: `novanet-dev/tools/novanet-mcp/src/tools/generate.ts`
- Modify: `novanet-dev/tools/novanet-mcp/src/types.ts`

> **Note:** This task is in NovaNet, not Nika. Execute in novanet-dev directory.

**Step 1: Add context_build_log to response type**

```typescript
// src/types.ts - add to GenerateResult
export interface GenerateResult {
  entity: string;
  locale: string;
  generated: Record<string, any>;
  context_build_log?: ContextBuildLog;
}

export interface ContextBuildLog {
  steps: ContextBuildStep[];
  total_tokens_estimated: number;
  sources_used: string[];
}

export interface ContextBuildStep {
  step: string;
  description: string;
  data_fetched?: string[];
  tokens_estimated?: number;
}
```

**Step 2: Implement context logging in generate**

```typescript
// src/tools/generate.ts
export async function generate(params: GenerateParams): Promise<GenerateResult> {
  const log: ContextBuildStep[] = [];

  // Step 1: Fetch entity
  log.push({
    step: "fetch_entity",
    description: `Fetching entity: ${params.entity}`,
    data_fetched: ["entity_node", "entity_natives"],
  });

  const entity = await fetchEntity(params.entity);

  // Step 2: Fetch locale context
  log.push({
    step: "fetch_locale",
    description: `Fetching locale context: ${params.locale}`,
    data_fetched: ["locale_config", "locale_natives"],
  });

  const localeContext = await fetchLocaleContext(params.locale);

  // Step 3: Build generation prompt
  log.push({
    step: "build_prompt",
    description: "Assembling generation prompt from context",
    tokens_estimated: estimateTokens(entity, localeContext),
  });

  // Step 4: Generate content
  const generated = await generateContent(entity, localeContext, params.forms);

  return {
    entity: params.entity,
    locale: params.locale,
    generated,
    context_build_log: {
      steps: log,
      total_tokens_estimated: log.reduce((sum, s) => sum + (s.tokens_estimated || 0), 0),
      sources_used: [
        `entity:${params.entity}`,
        `locale:${params.locale}`,
        ...params.forms.map(f => `form:${f}`),
      ],
    },
  };
}
```

**Step 3: Test**

Run: `npm test -- --grep "generate"`
Expected: PASS

**Step 4: Commit**

```bash
git add src/tools/generate.ts src/types.ts
git commit -m "feat(mcp): add context_build_log to generate response

- ContextBuildLog with steps, tokens, sources
- Each step documents data fetched
- Enables Nika observability integration

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: Final Integration Validation

**Files:**
- Create: `nika-dev/tools/nika/tests/e2e/v03_features.rs`

**Step 1: Create comprehensive e2e test**

```rust
// tests/e2e/v03_features.rs

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_v03_full_workflow() {
    // This tests all v0.3 features together

    let workflow = r#"
name: v03-integration-test
version: "1.0"

mcp:
  novanet:
    command: node
    args:
      - ../novanet-dev/tools/novanet-mcp/dist/index.js

tasks:
  # Test for_each parallelism
  generate_batch:
    for_each:
      items: ["en-US", "fr-FR"]
      as: locale
      max_parallel: 2
      task:
        invoke: novanet_generate
        params:
          entity: "qr-code"
          locale: $locale

  # Test agent with new tools
  analyze:
    depends_on: [generate_batch]
    agent: |
      Analyze the generated content for quality.
      Compare the English and French versions.
    provider: claude
    tools:
      - novanet_describe
    max_turns: 5

  # Test context_build_log visibility
  verify_logs:
    depends_on: [generate_batch]
    exec: |
      echo "Checking context_build_log in results"
"#;

    let result = nika::run_workflow_from_str(workflow).await;
    assert!(result.is_ok(), "v0.3 workflow should succeed: {:?}", result.err());

    let output = result.unwrap();

    // Verify for_each results
    assert!(output.tasks.contains_key("generate_batch"));
    let batch_output: Vec<serde_json::Value> = serde_json::from_str(
        &output.tasks["generate_batch"].output
    ).unwrap();
    assert_eq!(batch_output.len(), 2, "Should have 2 locale results");

    // Verify agent completed
    assert!(output.tasks.contains_key("analyze"));
    assert!(output.tasks["analyze"].success);

    // Verify metrics
    assert!(output.metrics.task_count >= 3);
    assert!(output.metrics.mcp_calls >= 2);
}

#[tokio::test]
async fn test_workspace_crates_compile() {
    // Simple test that all crates compile and link correctly
    use nika_core::Workflow;
    use nika_mcp::McpClient;
    // nika_tui only if feature enabled

    // Just verify we can reference types from all crates
    let _: Option<Workflow> = None;
    let _: Option<McpClient> = None;
}
```

**Step 2: Run e2e tests**

Run: `cargo test --features integration e2e`
Expected: PASS

**Step 3: Create v0.3 release tag**

```bash
# Update version in workspace Cargo.toml
# version = "0.3.0"

git add Cargo.toml
git commit -m "chore: bump version to 0.3.0"
git tag -a v0.3.0 -m "Nika v0.3.0 - Full feature release

Features:
- for_each parallel execution
- Multi-crate workspace
- Comprehensive documentation
- context_build_log in NovaNet

Breaking changes: None
"
```

**Step 4: Commit**

```bash
git add tests/e2e/
git commit -m "test(e2e): add v0.3 integration validation

- Test for_each, agent, context_build_log together
- Verify workspace crates link correctly
- Create v0.3.0 tag

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing MVP 6, you will have:

- `for_each:` parallel task execution
- Multi-crate workspace (nika-core, nika-mcp, nika-tui)
- Comprehensive user documentation
- 5 example workflows
- context_build_log in NovaNet responses
- v0.3.0 release tag

**Total tasks:** 7
**Nika version:** 0.3.0
