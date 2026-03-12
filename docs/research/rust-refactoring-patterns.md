# Rust Refactoring Patterns for Nika v0.28.0

**Research Date**: 2026-03-12
**Context**: Large-scale code restructuring for Nika v0.28.0
**Sources**: Context7 (rust-unofficial/patterns, google/comprehensive-rust, tokio, refactoringguru/design-patterns-rust)

---

## Executive Summary

This document provides research-backed patterns and strategies for refactoring Nika's codebase:

| Target | Current | Goal | Primary Pattern |
|--------|---------|------|-----------------|
| `main.rs` | 6,967 LOC | `cli/` module | Module Extraction |
| `rig_agent_loop.rs` | 3,786 LOC | `agent/` submodules | Struct Decomposition |
| `App` struct | 50+ fields | Subsystems | Facade + Mediator |
| `std::fs` calls | ~140 | `tokio::fs` | spawn_blocking Migration |

---

## Table of Contents

1. [Struct Decomposition Patterns](#1-struct-decomposition-patterns)
2. [Module Extraction Strategies](#2-module-extraction-strategies)
3. [Facade Pattern for Subsystems](#3-facade-pattern-for-subsystems)
4. [Mediator Pattern for Coordination](#4-mediator-pattern-for-coordination)
5. [Async Migration Patterns](#5-async-migration-patterns)
6. [Testing Strategies During Refactoring](#6-testing-strategies-during-refactoring)
7. [Rollback Strategies](#7-rollback-strategies)
8. [Implementation Checklist](#8-implementation-checklist)

---

## 1. Struct Decomposition Patterns

### Problem: God Object (App with 50+ fields)

Large structs cause borrow checker conflicts and make code hard to maintain.

### Solution: Decompose into Smaller Structs

**Pattern from rust-unofficial/patterns:**

```rust
// BEFORE: Monolithic struct with many fields
struct App {
    // UI state (15+ fields)
    current_view: View,
    show_modal: bool,
    scroll_offset: u16,
    cursor_position: (u16, u16),
    // ... more UI fields

    // Provider state (10+ fields)
    active_provider: Provider,
    api_keys: HashMap<String, String>,
    // ... more provider fields

    // Runtime state (15+ fields)
    tasks: Vec<Task>,
    dag: Dag,
    event_log: EventLog,
    // ... more runtime fields

    // Editor state (10+ fields)
    open_files: Vec<PathBuf>,
    edit_history: EditHistory,
    // ... more editor fields
}

// AFTER: Decomposed into subsystems
struct App {
    ui: UiSubsystem,
    providers: ProviderSubsystem,
    runtime: RuntimeSubsystem,
    editor: EditorSubsystem,
}

// Each subsystem is independently borrowable
#[derive(Debug, Default)]
struct UiSubsystem {
    current_view: View,
    show_modal: bool,
    scroll_offset: u16,
    cursor_position: (u16, u16),
}

#[derive(Debug)]
struct ProviderSubsystem {
    active_provider: Provider,
    api_keys: HashMap<String, String>,
    connection_status: ConnectionStatus,
}

#[derive(Debug)]
struct RuntimeSubsystem {
    tasks: Vec<Task>,
    dag: Dag,
    event_log: EventLog,
    executor: TaskExecutor,
}

#[derive(Debug)]
struct EditorSubsystem {
    open_files: Vec<PathBuf>,
    edit_history: EditHistory,
    sessions: SessionManager,
}
```

### Advantages of Struct Decomposition

1. **Independent Borrowing**: Each subsystem can be borrowed independently
2. **Cleaner APIs**: Functions take only the subsystem they need
3. **Better Testing**: Subsystems can be tested in isolation
4. **Reduced Cognitive Load**: Smaller, focused modules

### Implementation Steps

```rust
// Step 1: Group related fields
impl App {
    // Identify field clusters by access patterns
    fn analyze_field_access(&self) {
        // Fields accessed together go in same subsystem
    }
}

// Step 2: Create subsystem structs with impl blocks
impl UiSubsystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_view(&mut self, view: View) {
        self.current_view = view;
    }

    pub fn toggle_modal(&mut self) {
        self.show_modal = !self.show_modal;
    }
}

// Step 3: Update method signatures
// BEFORE
fn render_view(app: &App) { ... }

// AFTER
fn render_view(ui: &UiSubsystem, runtime: &RuntimeSubsystem) { ... }

// Step 4: Use delegation in App
impl App {
    pub fn set_view(&mut self, view: View) {
        self.ui.set_view(view);
    }
}
```

---

## 2. Module Extraction Strategies

### Problem: Large Files (main.rs at 6,967 LOC)

Large files are hard to navigate, test, and maintain.

### Solution: Systematic Module Extraction

**Pattern from google/comprehensive-rust:**

```rust
// BEFORE: Everything in main.rs
// main.rs (6,967 lines)
fn main() { ... }
fn parse_args() { ... }
fn run_command() { ... }
fn handle_workflow() { ... }
fn handle_chat() { ... }
fn handle_studio() { ... }
// ... hundreds more functions

// AFTER: Modular structure
// main.rs (50 lines)
mod cli;

fn main() -> anyhow::Result<()> {
    cli::run()
}

// src/cli/mod.rs
mod args;
mod commands;
mod output;

pub use args::Args;
pub use commands::run;

// src/cli/args.rs
use clap::Parser;

#[derive(Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Run(RunArgs),
    Chat(ChatArgs),
    Studio(StudioArgs),
    Provider(ProviderArgs),
    // ...
}

// src/cli/commands/mod.rs
mod run;
mod chat;
mod studio;
mod provider;

pub use run::execute as run;

pub fn dispatch(cmd: Command) -> Result<()> {
    match cmd {
        Command::Run(args) => run::execute(args),
        Command::Chat(args) => chat::execute(args),
        Command::Studio(args) => studio::execute(args),
        Command::Provider(args) => provider::execute(args),
    }
}
```

### Module Extraction Checklist

```
[ ] 1. Identify logical boundaries
    - Group by feature (chat, studio, provider)
    - Group by layer (args, handlers, output)
    - Group by domain (workflow, agent, mcp)

[ ] 2. Create module structure
    src/cli/
    ├── mod.rs           # Public API + re-exports
    ├── args.rs          # CLI argument definitions
    ├── commands/
    │   ├── mod.rs       # Command dispatcher
    │   ├── run.rs       # `nika run` command
    │   ├── chat.rs      # `nika chat` command
    │   ├── studio.rs    # `nika studio` command
    │   └── provider.rs  # `nika provider` subcommands
    └── output.rs        # Output formatting

[ ] 3. Move code incrementally
    - Move ONE function/struct at a time
    - Run tests after each move
    - Update imports immediately

[ ] 4. Maintain public API
    - Use `pub use` for re-exports
    - Keep backward compatibility in lib.rs
```

### Filesystem to Module Mapping

```
// Option A: mod.rs style (traditional)
src/cli/mod.rs           -> mod cli { ... }
src/cli/args.rs          -> mod cli::args { ... }

// Option B: filename.rs style (modern, preferred)
src/cli.rs               -> mod cli { ... }
src/cli/args.rs          -> mod cli::args { ... }
```

### Visibility Control

```rust
// Control what's public at each level
// src/cli/mod.rs
pub mod args;        // Public module
mod internal;        // Private module

pub use args::Args;  // Re-export specific items
pub(crate) use internal::helper;  // Crate-internal
```

---

## 3. Facade Pattern for Subsystems

### Problem: Complex Internal APIs

External code shouldn't need to know subsystem internals.

### Solution: Facade Provides Simplified Interface

**Pattern from refactoringguru/design-patterns-rust:**

```rust
/// Facade for the Nika workflow engine
/// Hides complexity of runtime, MCP, and provider subsystems
pub struct NikaFacade {
    runtime: RuntimeSubsystem,
    mcp: McpSubsystem,
    providers: ProviderSubsystem,
    events: EventSubsystem,
}

impl NikaFacade {
    pub fn new(config: Config) -> Result<Self> {
        println!("Initializing Nika engine...");
        Ok(Self {
            runtime: RuntimeSubsystem::new(&config)?,
            mcp: McpSubsystem::new(&config)?,
            providers: ProviderSubsystem::new(&config)?,
            events: EventSubsystem::new(),
        })
    }

    /// Execute a workflow - simplified API
    pub async fn run_workflow(&mut self, path: &Path) -> Result<WorkflowResult> {
        // Internally orchestrates multiple subsystems
        let workflow = self.runtime.parse_workflow(path)?;
        self.runtime.validate_dag(&workflow)?;

        // Start MCP servers
        for server in &workflow.mcp_servers {
            self.mcp.start_server(server).await?;
        }

        // Initialize provider
        let provider = self.providers.get_or_create(&workflow.provider)?;

        // Execute with event tracking
        let result = self.runtime.execute(
            &workflow,
            &provider,
            &mut self.events,
        ).await?;

        // Cleanup
        self.mcp.stop_all_servers().await?;

        Ok(result)
    }

    /// Start chat session - simplified API
    pub async fn start_chat(&mut self, opts: ChatOptions) -> Result<ChatSession> {
        let provider = self.providers.get_or_create(&opts.provider)?;
        Ok(ChatSession::new(provider, self.mcp.clone(), opts))
    }
}

// Usage is simple despite internal complexity
fn main() -> Result<()> {
    let mut nika = NikaFacade::new(Config::load()?)?;

    // Simple API hides 5+ subsystem interactions
    let result = nika.run_workflow(Path::new("workflow.nika.yaml")).await?;
    println!("Result: {:?}", result);

    Ok(())
}
```

### Facade Benefits for Nika

| Aspect | Without Facade | With Facade |
|--------|---------------|-------------|
| API Surface | 50+ public methods | 5-10 entry points |
| Learning Curve | High | Low |
| Coupling | Tight to internals | Loose, interface-based |
| Refactoring | Breaks external code | Internal changes hidden |

---

## 4. Mediator Pattern for Coordination

### Problem: Components Need to Communicate

Subsystems need to coordinate without tight coupling.

### Solution: Mediator Centralizes Communication

**Pattern from refactoringguru/design-patterns-rust:**

```rust
/// Mediator trait defines communication protocol
pub trait AppMediator {
    fn notify(&mut self, sender: &str, event: AppEvent);
}

/// Events that flow through the mediator
pub enum AppEvent {
    ViewChanged(View),
    TaskCompleted(TaskId, TaskResult),
    ProviderConnected(ProviderId),
    EditorFileOpened(PathBuf),
    McpToolCalled(String, serde_json::Value),
}

/// App acts as the mediator
impl AppMediator for App {
    fn notify(&mut self, sender: &str, event: AppEvent) {
        match event {
            AppEvent::ViewChanged(view) => {
                // Update UI subsystem
                self.ui.set_view(view);
                // Maybe also update status bar
                self.ui.update_status(&format!("Switched to {:?}", view));
            }

            AppEvent::TaskCompleted(task_id, result) => {
                // Update runtime state
                self.runtime.mark_completed(task_id, &result);
                // Update UI to show completion
                self.ui.refresh_task_list(&self.runtime.tasks);
                // Log event
                self.events.emit(Event::TaskCompleted { task_id, result });
            }

            AppEvent::ProviderConnected(provider_id) => {
                // Update provider subsystem
                self.providers.set_connected(provider_id);
                // Update UI status
                self.ui.show_provider_status(&self.providers);
            }

            AppEvent::EditorFileOpened(path) => {
                // Update editor subsystem
                self.editor.open_file(&path);
                // Update UI
                self.ui.show_editor_tabs(&self.editor.open_files);
            }

            AppEvent::McpToolCalled(tool, params) => {
                // Log for observability
                self.events.emit(Event::McpToolInvoked { tool, params });
                // Update UI activity indicator
                self.ui.show_activity("MCP tool call...");
            }
        }
    }
}

/// Subsystems use mediator for coordination
impl RuntimeSubsystem {
    pub async fn execute_task(
        &mut self,
        task: &Task,
        mediator: &mut impl AppMediator,
    ) -> Result<TaskResult> {
        let result = self.executor.run(task).await?;

        // Notify via mediator instead of direct coupling
        mediator.notify("runtime", AppEvent::TaskCompleted(task.id, result.clone()));

        Ok(result)
    }
}
```

### Top-Down Ownership (Rust-Specific)

Rust's ownership rules require a specific mediator variant:

```rust
/// Mediator owns all components (top-down ownership)
struct AppCoordinator {
    // Owned subsystems
    ui: UiSubsystem,
    runtime: RuntimeSubsystem,
    providers: ProviderSubsystem,
    editor: EditorSubsystem,
}

impl AppCoordinator {
    /// Components receive mediator reference via method calls
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::TaskCompleted(id, result) => {
                // Mediator coordinates between owned components
                self.runtime.mark_completed(id, &result);
                self.ui.refresh_task_list(&self.runtime.tasks);
            }
            // ...
        }
    }

    /// Subsystems don't hold permanent reference to mediator
    pub fn execute_workflow(&mut self, workflow: &Workflow) -> Result<()> {
        // Pass mediator reference only when needed
        for task in &workflow.tasks {
            let result = self.runtime.execute_task(task)?;
            // Coordinator handles cross-cutting concerns
            self.handle_event(AppEvent::TaskCompleted(task.id, result));
        }
        Ok(())
    }
}
```

---

## 5. Async Migration Patterns

### Problem: 140 std::fs Calls Need Async

Blocking file I/O impacts async runtime performance.

### Solution: Gradual Migration with spawn_blocking

**Pattern from tokio documentation:**

### Strategy 1: spawn_blocking Wrapper (Quick Migration)

```rust
use tokio::task::spawn_blocking;
use std::path::Path;
use std::io;

/// Wrapper that makes std::fs async-compatible
pub mod async_fs {
    use super::*;

    pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
        let path = path.as_ref().to_owned();
        spawn_blocking(move || std::fs::read_to_string(path))
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
    }

    pub async fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
        let path = path.as_ref().to_owned();
        let contents = contents.as_ref().to_vec();
        spawn_blocking(move || std::fs::write(path, contents))
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
    }

    pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref().to_owned();
        spawn_blocking(move || std::fs::create_dir_all(path))
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
    }

    pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref().to_owned();
        spawn_blocking(move || std::fs::remove_file(path))
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
    }

    pub async fn exists(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref().to_owned();
        spawn_blocking(move || path.exists())
            .await
            .unwrap_or(false)
    }

    pub async fn metadata(path: impl AsRef<Path>) -> io::Result<std::fs::Metadata> {
        let path = path.as_ref().to_owned();
        spawn_blocking(move || std::fs::metadata(path))
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
    }
}
```

### Strategy 2: Direct tokio::fs (Full Migration)

```rust
use tokio::fs;
use std::path::Path;

// BEFORE (blocking)
fn load_workflow(path: &Path) -> Result<Workflow> {
    let content = std::fs::read_to_string(path)?;
    serde_yaml::from_str(&content).map_err(Into::into)
}

// AFTER (async)
async fn load_workflow(path: &Path) -> Result<Workflow> {
    let content = fs::read_to_string(path).await?;
    serde_yaml::from_str(&content).map_err(Into::into)
}
```

### Migration Checklist

```
Phase 1: Audit (identify all std::fs calls)
[ ] grep -r "std::fs::" src/
[ ] grep -r "fs::read" src/
[ ] grep -r "fs::write" src/
[ ] grep -r "File::create" src/
[ ] grep -r "File::open" src/
[ ] Document all 140 call sites

Phase 2: Create async_fs wrapper module
[ ] Create src/io/async_fs.rs
[ ] Implement spawn_blocking wrappers
[ ] Add tests for wrapper functions

Phase 3: Migrate by module (one at a time)
[ ] src/ast/ - YAML loading
[ ] src/runtime/ - Artifact writing
[ ] src/tui/session.rs - Session persistence
[ ] src/tui/config.rs - Config loading
[ ] src/event/trace.rs - Trace writing

Phase 4: Replace wrapper with tokio::fs
[ ] Update imports: use tokio::fs instead of async_fs
[ ] Remove async_fs wrapper module
[ ] Verify no spawn_blocking overhead remains
```

### When to Use Each Strategy

| Scenario | Strategy | Rationale |
|----------|----------|-----------|
| Quick migration | `spawn_blocking` wrapper | Minimal code changes |
| Performance critical | Direct `tokio::fs` | No thread pool overhead |
| Complex file operations | `spawn_blocking` | Better for multi-step ops |
| Simple read/write | Direct `tokio::fs` | Cleaner code |

---

## 6. Testing Strategies During Refactoring

### Principle: Tests Are Vital During Refactoring

**From rust-unofficial/patterns:**
> "Tests are of vital importance during refactoring."

### Strategy 1: Characterization Tests (Before Refactoring)

```rust
// Capture current behavior BEFORE changing code
#[cfg(test)]
mod characterization_tests {
    use super::*;

    #[test]
    fn capture_app_initialization_behavior() {
        // Record current behavior
        let app = App::new(Config::default()).unwrap();

        // Assert current state (even if it seems obvious)
        assert!(app.current_view == View::Home);
        assert!(app.tasks.is_empty());
        assert!(app.mcp_clients.is_empty());

        // These tests document existing behavior
        // They'll catch unintended changes during refactor
    }

    #[test]
    fn capture_workflow_parsing_behavior() {
        let yaml = include_str!("../fixtures/simple.nika.yaml");
        let workflow = parse_workflow(yaml).unwrap();

        // Snapshot test - captures exact output
        insta::assert_yaml_snapshot!(workflow);
    }
}
```

### Strategy 2: Parallel Testing Structure

```rust
// Tests mirror production structure
// src/cli/commands/run.rs
pub fn execute(args: RunArgs) -> Result<()> { ... }

// tests/cli/commands/run_tests.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_with_valid_workflow() { ... }

    #[test]
    fn test_run_with_missing_file() { ... }

    #[test]
    fn test_run_with_invalid_yaml() { ... }
}
```

### Strategy 3: Integration Tests for Public API

```rust
// tests/api_stability.rs
// These tests ensure public API doesn't break

#[test]
fn test_nika_facade_api_stability() {
    // Test that public API still works after refactoring
    let config = Config::default();
    let nika = NikaFacade::new(config).unwrap();

    // Public methods must remain callable
    assert!(nika.list_providers().len() > 0);
    assert!(nika.get_version().starts_with("0."));
}

#[test]
fn test_workflow_parsing_api_stability() {
    // Parser API must remain stable
    let yaml = include_str!("fixtures/basic.nika.yaml");
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

    // Public fields must exist
    assert!(!workflow.tasks.is_empty());
    assert!(workflow.schema.starts_with("nika/workflow@"));
}
```

### Strategy 4: Mutation Testing Validation

```bash
# After refactoring, verify tests still catch bugs
cargo mutants --package nika

# If mutation score drops, tests are weaker
# Target: >80% mutation score for critical paths
```

### Test Organization After Refactoring

```
tests/
├── api_stability.rs      # Public API contract tests
├── cli/
│   ├── mod.rs
│   ├── run_tests.rs
│   ├── chat_tests.rs
│   └── provider_tests.rs
├── runtime/
│   ├── mod.rs
│   ├── executor_tests.rs
│   └── agent_tests.rs
├── integration/
│   ├── workflow_e2e.rs
│   └── mcp_e2e.rs
└── fixtures/
    ├── workflows/
    └── snapshots/
```

---

## 7. Rollback Strategies

### Git-Based Rollback

```bash
# Strategy 1: Feature branch per refactoring phase
git checkout -b refactor/cli-extraction
# ... do work ...
git checkout -b refactor/app-subsystems
# ... do work ...

# If phase fails, just delete branch
git branch -D refactor/app-subsystems

# Strategy 2: Incremental commits with clear messages
git commit -m "refactor(cli): extract args module from main.rs"
git commit -m "refactor(cli): extract run command handler"
git commit -m "refactor(cli): extract chat command handler"

# Rollback to specific point
git revert HEAD~2..HEAD  # Revert last 2 commits

# Strategy 3: Tags for milestones
git tag refactor-m1-cli-extracted
git tag refactor-m2-subsystems-created
git tag refactor-m3-async-migration

# Rollback to milestone
git checkout refactor-m2-subsystems-created
```

### Code-Level Rollback

```rust
// Strategy 1: Feature flags for gradual rollout
#[cfg(feature = "new-cli")]
mod cli_v2;

#[cfg(not(feature = "new-cli"))]
mod cli;  // Original implementation

// Strategy 2: Parallel implementations during migration
mod app {
    pub mod v1;  // Original monolithic App
    pub mod v2;  // New subsystem-based App
}

// Switch at runtime
fn create_app(use_v2: bool) -> Box<dyn AppTrait> {
    if use_v2 {
        Box::new(app::v2::App::new())
    } else {
        Box::new(app::v1::App::new())
    }
}

// Strategy 3: Deprecation with fallback
#[deprecated(since = "0.28.0", note = "Use NikaFacade instead")]
pub struct App {
    inner: NikaFacade,
}

impl App {
    // Forward to new implementation
    pub fn run_workflow(&mut self, path: &Path) -> Result<()> {
        self.inner.run_workflow(path)
    }
}
```

### Rollback Decision Matrix

| Symptom | Action | Command |
|---------|--------|---------|
| Tests fail after commit | Revert commit | `git revert HEAD` |
| Build breaks | Checkout last green | `git checkout HEAD~1` |
| Performance regression | Feature flag off | `--no-default-features` |
| API breaks external code | Keep parallel impl | Use `v1` module |
| Multiple commits bad | Interactive rebase | `git rebase -i HEAD~N` |

---

## 8. Implementation Checklist

### Phase 1: Preparation (Week 1)

```
[ ] Create characterization tests for current behavior
[ ] Document all public API entry points
[ ] Set up feature flags for gradual rollout
[ ] Create refactor branches:
    - refactor/cli-extraction
    - refactor/app-subsystems
    - refactor/async-migration
    - refactor/agent-decomposition
```

### Phase 2: CLI Extraction (Week 2)

```
[ ] Create src/cli/mod.rs
[ ] Extract Args struct to src/cli/args.rs
[ ] Create src/cli/commands/ directory
[ ] Move run command to src/cli/commands/run.rs
[ ] Move chat command to src/cli/commands/chat.rs
[ ] Move studio command to src/cli/commands/studio.rs
[ ] Move provider commands to src/cli/commands/provider.rs
[ ] Update main.rs to use cli::run()
[ ] Run tests after each extraction
[ ] Tag milestone: refactor-m1-cli-extracted
```

### Phase 3: App Subsystems (Week 3)

```
[ ] Create src/tui/subsystems/ directory
[ ] Extract UiSubsystem to src/tui/subsystems/ui.rs
[ ] Extract ProviderSubsystem to src/tui/subsystems/providers.rs
[ ] Extract RuntimeSubsystem to src/tui/subsystems/runtime.rs
[ ] Extract EditorSubsystem to src/tui/subsystems/editor.rs
[ ] Update App struct to compose subsystems
[ ] Implement AppMediator trait for coordination
[ ] Update all App method signatures
[ ] Run tests after each extraction
[ ] Tag milestone: refactor-m2-subsystems-created
```

### Phase 4: Agent Decomposition (Week 4)

```
[ ] Create src/runtime/agent/ directory
[ ] Extract agent loop to src/runtime/agent/loop.rs
[ ] Extract tool handling to src/runtime/agent/tools.rs
[ ] Extract provider integration to src/runtime/agent/providers.rs
[ ] Extract history management to src/runtime/agent/history.rs
[ ] Update RigAgentLoop to use new modules
[ ] Run tests after each extraction
[ ] Tag milestone: refactor-m3-agent-decomposed
```

### Phase 5: Async Migration (Week 5)

```
[ ] Create src/io/async_fs.rs wrapper module
[ ] Replace std::fs calls in src/ast/
[ ] Replace std::fs calls in src/runtime/
[ ] Replace std::fs calls in src/tui/session.rs
[ ] Replace std::fs calls in src/tui/config.rs
[ ] Replace std::fs calls in src/event/trace.rs
[ ] Run benchmarks to verify no regression
[ ] Replace wrapper with direct tokio::fs
[ ] Tag milestone: refactor-m4-async-complete
```

### Phase 6: Verification (Week 6)

```
[ ] Run full test suite (cargo nextest run)
[ ] Run clippy with all warnings (cargo clippy -- -D warnings)
[ ] Run benchmarks (cargo bench)
[ ] Run mutation tests (cargo mutants)
[ ] Verify public API compatibility
[ ] Update documentation
[ ] Remove deprecated code paths
[ ] Tag release: v0.28.0
```

---

## Summary

### Pattern Selection Guide

| Problem | Pattern | Apply To |
|---------|---------|----------|
| Large struct (50+ fields) | Struct Decomposition | App struct |
| Large file (3000+ LOC) | Module Extraction | main.rs, rig_agent_loop.rs |
| Complex internal APIs | Facade | NikaFacade for external use |
| Component communication | Mediator | App coordinates subsystems |
| Blocking I/O in async | spawn_blocking | 140 std::fs calls |
| Type safety at boundaries | Newtype | TaskId, ProviderId, etc. |

### Key Principles

1. **Incremental Changes**: One extraction at a time
2. **Test After Each Change**: Catch regressions immediately
3. **Maintain Public API**: Use re-exports and deprecation
4. **Document Decisions**: Update CLAUDE.md and ADRs
5. **Tag Milestones**: Enable easy rollback

### Expected Outcomes

| Metric | Before | After |
|--------|--------|-------|
| main.rs LOC | 6,967 | ~100 |
| rig_agent_loop.rs LOC | 3,786 | ~500 |
| App struct fields | 50+ | 4 subsystems |
| std::fs calls | ~140 | 0 (all async) |
| Test coverage | 4,433 | 4,800+ |

---

## References

- [rust-unofficial/patterns - Struct Decomposition](https://github.com/rust-unofficial/patterns)
- [google/comprehensive-rust - Modules](https://github.com/google/comprehensive-rust)
- [tokio::fs documentation](https://docs.rs/tokio/latest/tokio/fs/)
- [refactoringguru/design-patterns-rust - Facade](https://github.com/refactoringguru/design-patterns-rust)
- [refactoringguru/design-patterns-rust - Mediator](https://github.com/refactoringguru/design-patterns-rust)
