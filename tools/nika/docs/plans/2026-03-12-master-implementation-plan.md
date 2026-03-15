# Nika Master Implementation Plan — 2026-03-12

**Status**: ACTIVE EXECUTION
**Author**: Claude Opus 4.5 + 54-Agent Research Swarm
**Current Version**: v0.27.0 | 6,157 tests passing
**Target**: Comprehensive autonomous work through all remaining items

---

## Executive Summary

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  MASTER IMPLEMENTATION PLAN — 6 PHASES                                        ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Phase 1: Bug Fixes + TUI Polish               ~4.5 hours    [PRIORITY 1]    ║
║  ├── BUG-003: use: implicit depends_on         ~1.5h                         ║
║  ├── BUG-004: Wrong terminal task selection    ~1h                           ║
║  ├── BUG-005: for_each $items resolution       ~0.5h (verify after BUG-003)  ║
║  └── TUI Gap polish                            ~1.5h                         ║
║                                                                               ║
║  Phase 2: Chat MCP Integration                 ~12 hours     [PRIORITY 2]    ║
║  ├── ChatInvoke: /invoke command in Chat       ~4h                           ║
║  ├── ChatAgent: /agent command in Chat         ~6h                           ║
║  └── McpClient pool management                 ~2h                           ║
║                                                                               ║
║  Phase 3: MCP Hardening (5 bugs)               ~10 hours     [PRIORITY 3]    ║
║  ├── NIKA-101: Server spawn race               ~2h                           ║
║  ├── NIKA-102: Stdin/stdout mutex conflicts    ~2h                           ║
║  ├── NIKA-103: Reconnect loop on disconnect    ~2h                           ║
║  ├── NIKA-104: Tool list caching stale         ~2h                           ║
║  └── NIKA-105: Timeout not propagating         ~2h                           ║
║                                                                               ║
║  Phase 4: Agent Completion v2.0                ~32 hours     [PRIORITY 4]    ║
║  ├── Phase 2: Chat Integration                 ~8-10h                        ║
║  ├── Phase 3: TUI Integration                  ~10-12h                       ║
║  ├── Phase 4: Error Handling                   ~6-8h                         ║
║  └── Phase 5: Testing                          ~8-10h                        ║
║                                                                               ║
║  Phase 5: v0.28 Workspace Restructure          ~80-100 hours [PRIORITY 5]    ║
║  ├── Phase 0: Pre-Migration Setup              ~4h                           ║
║  ├── Phase 1: Extract nika-core                ~20h                          ║
║  ├── Phase 2: Extract nika-runtime             ~35h                          ║
║  ├── Phase 3: Extract nika-tui                 ~20h                          ║
║  ├── Phase 4: CLI Restructure                  ~15h                          ║
║  └── Phase 5: Polish + Release                 ~10h                          ║
║                                                                               ║
║  Phase 6: Roadmap v0.9-v0.12 Train             ~58 hours     [FUTURE]        ║
║  ├── v0.9 Chat-as-DAG                          ~18h (251 tests)              ║
║  ├── v0.10 TaskBox                             ~12h (75 tests)               ║
║  ├── v0.11 Six Views                           ~15h (90 tests)               ║
║  └── v0.12 Providers                           ~9h (45 tests)                ║
║                                                                               ║
║  GRAND TOTAL                                   ~150-170 hours                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Table of Contents

1. [Research Agent Findings](#1-research-agent-findings)
2. [Phase 1: Bug Fixes + TUI Polish](#2-phase-1-bug-fixes--tui-polish)
3. [Phase 2: Chat MCP Integration](#3-phase-2-chat-mcp-integration)
4. [Phase 3: MCP Hardening](#4-phase-3-mcp-hardening)
5. [Phase 4: Agent Completion v2.0](#5-phase-4-agent-completion-v20)
6. [Phase 5: v0.28 Workspace Restructure](#6-phase-5-v028-workspace-restructure)
7. [Quality Gates & Checkpoints](#7-quality-gates--checkpoints)
8. [Execution Workflow](#8-execution-workflow)

---

## 1. Research Agent Findings

### 1.1 Context7: Rust Workspace Patterns

**Best practices for Cargo workspace restructuring:**

```toml
# Root Cargo.toml pattern
[workspace]
members = [
    "crates/nika-core",
    "crates/nika-ast",
    "crates/nika-dag",
    "crates/nika-mcp",
    "crates/nika-runtime",
    "crates/nika-tui",
    "crates/nika-cli",
]
resolver = "2"

[workspace.package]
version = "0.28.0"
edition = "2021"
authors = ["Thibaut MÉLEN <thibaut@supernovae.studio>"]
license = "AGPL-3.0-or-later"
repository = "https://github.com/supernovae-st/nika"

[workspace.dependencies]
# Shared dependencies with versions
tokio = { version = "1.49", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"
```

**Dependency management principles:**
1. Use `workspace.dependencies` for all shared deps
2. Crate-specific deps in `[dependencies]` with `version.workspace = true`
3. Feature flags at workspace level for conditional compilation
4. Test dependencies separate from main deps

### 1.2 Rust Pro: God Object Decomposition

**RigAgentLoop decomposition (3,786 LoC → 13 files):**

```
src/runtime/agent/
├── mod.rs              # 200 LoC - Re-exports, RigAgentLoop struct
├── types.rs            # 150 LoC - RigAgentStatus, RigAgentLoopResult
├── builder.rs          # 200 LoC - new(), with_stream_tx(), with_skills()
├── history.rs          # 150 LoC - Chat history management
├── tools.rs            # 250 LoC - build_tools(), tool registration
├── streaming.rs        # 200 LoC - stream_completion_with_tokens()
├── providers/
│   ├── mod.rs          # 100 LoC - Provider trait + run_auto()
│   ├── claude.rs       # 400 LoC - run_claude(), run_claude_with_thinking()
│   ├── openai.rs       # 200 LoC - run_openai()
│   ├── generic.rs      # 300 LoC - run_generic_provider_impl()
│   └── mock.rs         # 150 LoC - run_mock()
├── guardrails.rs       # 200 LoC - check_guardrails()
├── completion.rs       # 300 LoC - determine_status(), check_stop_conditions()
├── routing.rs          # 350 LoC - apply_routing(), route_action_to_status()
└── skills.rs           # 180 LoC - inject_skills_into_prompt()
```

**Extension trait pattern for decomposition:**

```rust
// Step 1: Extract trait
pub trait HistoryManagement {
    fn add_to_history(&mut self, user: &str, assistant: &str);
    fn clear_history(&mut self);
    fn history_len(&self) -> usize;
}

// Step 2: Implement for main struct
impl HistoryManagement for RigAgentLoop {
    fn add_to_history(&mut self, user: &str, assistant: &str) {
        // Implementation moved from god object
    }
    // ...
}
```

**main.rs decomposition (6,967 LoC → 15+ files):**

```
src/cli/
├── mod.rs              # 100 LoC - Re-exports
├── app.rs              # 200 LoC - Clap app definition
├── handlers/
│   ├── mod.rs          # 50 LoC - Handler trait
│   ├── run.rs          # 300 LoC - `nika run` handler
│   ├── check.rs        # 200 LoC - `nika check` handler
│   ├── chat.rs         # 250 LoC - `nika chat` handler
│   ├── studio.rs       # 200 LoC - `nika studio` handler
│   ├── tui.rs          # 250 LoC - TUI launcher
│   ├── provider.rs     # 400 LoC - `nika provider` commands
│   ├── model.rs        # 350 LoC - `nika model` commands
│   ├── mcp.rs          # 400 LoC - `nika mcp` commands
│   ├── sync.rs         # 300 LoC - `nika sync` commands
│   ├── setup.rs        # 400 LoC - `nika setup` wizard
│   ├── daemon.rs       # 200 LoC - `nika daemon` commands
│   ├── jobs.rs         # 300 LoC - `nika jobs` commands
│   ├── backup.rs       # 250 LoC - `nika backup` commands
│   └── trace.rs        # 200 LoC - `nika trace` commands
└── helpers/
    ├── output.rs       # 150 LoC - Output formatting
    ├── progress.rs     # 100 LoC - Progress bars
    └── prompts.rs      # 150 LoC - Interactive prompts
```

**TUI App decomposition (34 fields → 5 subsystems):**

```rust
pub struct App {
    // 5 subsystems instead of 34 flat fields
    terminal: TerminalSubsystem,      // Terminal state, backend
    navigation: NavigationSubsystem,   // View switching, focus
    views: ViewSubsystem,              // All view instances
    agent: AgentSubsystem,             // LLM/MCP integration
    persistence: PersistenceSubsystem, // Session/config state
}

pub struct TerminalSubsystem {
    backend: CrosstermBackend<Stdout>,
    size: Rect,
    should_quit: bool,
}

pub struct NavigationSubsystem {
    current_view: View,
    previous_view: Option<View>,
    focus_state: FocusState,
    modal: Option<ModalKind>,
}

pub struct ViewSubsystem {
    studio: StudioView,
    runner: RunnerView,
    chat: ChatView,
    settings: SettingsView,
}

pub struct AgentSubsystem {
    provider: Option<RigProvider>,
    mcp_clients: DashMap<String, McpClient>,
    event_log: EventLog,
}

pub struct PersistenceSubsystem {
    config: TuiSettings,
    session: SessionManager,
    datastore: RunContext,
}
```

### 1.3 Perplexity: Refactoring Best Practices

**4-Phase refactoring approach:**

```
Phase 1: Foundation (Week 1) - Extract nika-core
├── Create workspace structure
├── Move zero-dep types (error.rs, models.rs, providers.rs)
├── Establish compilation baseline
└── Test: cargo build --workspace

Phase 2: AST Layer (Week 2) - Extract nika-ast, nika-dag
├── Extract ast/ module to nika-ast crate
├── Extract dag/ module to nika-dag crate
├── Update imports in main crate
└── Test: all existing tests pass

Phase 3: Execution Layer (Week 3) - Extract runtime components
├── Extract mcp/ to nika-mcp crate
├── Extract provider/ to nika-provider crate
├── Extract runtime/ to nika-runtime crate
└── Test: workflow execution unchanged

Phase 4: UI Layer (Week 4) - Extract TUI, create CLI
├── Extract tui/ to nika-tui crate
├── Create nika-cli with handler pattern
├── Main binary becomes thin wrapper
└── Test: TUI and CLI functionality preserved
```

**Common pitfalls to avoid:**
1. **Over-splitting**: Don't create too many tiny crates (7-10 optimal)
2. **Circular deps**: Plan dependency graph before starting
3. **Test fragmentation**: Keep integration tests at workspace level
4. **Feature flag explosion**: Consolidate features at workspace level
5. **Documentation drift**: Update docs alongside code moves

---

## 2. Phase 1: Bug Fixes + TUI Polish

### 2.1 BUG-003: use: implicit depends_on

**Location**: `src/dag/flow.rs:40-118`

**Problem**: When a task has `use: { data: step1 }`, it should automatically depend on `step1`. Currently requires explicit `depends_on: [step1]`.

**TDD Steps**:

1. Write failing test:
```rust
#[test]
fn test_use_wiring_creates_implicit_dependency() {
    let yaml = r#"
schema: nika/workflow@0.9
workflow: test_implicit_dep
tasks:
  - id: step1
    infer: "Generate data"
  - id: step2
    use:
      data: step1
    infer: "Process: {{use.data}}"
"#;
    let workflow = parse_workflow(yaml).unwrap();
    let dag = Dag::from_workflow(&workflow);

    let deps = dag.get_dependencies("step2");
    assert!(deps.iter().any(|d| d.as_ref() == "step1"));
}
```

2. Implement fix in `Dag::from_workflow()`:
```rust
// After line ~110 (after task.flow loop)
for task in &workflow.tasks {
    if let Some(ref wiring) = task.use_wiring {
        let tgt_arc = task_set.get(task.id.as_str()).cloned()
            .unwrap_or_else(|| intern(&task.id));

        for (_alias, entry) in wiring {
            let dep_task_id = entry.task_id();
            if dep_task_id == task.id || !task_set.contains(dep_task_id) {
                continue;
            }

            let src_arc = task_set.get(dep_task_id).cloned()
                .unwrap_or_else(|| intern(dep_task_id));

            let adj_entry = adjacency.entry(Arc::clone(&src_arc)).or_default();
            if !adj_entry.contains(&tgt_arc) {
                adj_entry.push(Arc::clone(&tgt_arc));
            }

            let pred_entry = predecessors.entry(Arc::clone(&tgt_arc)).or_default();
            if !pred_entry.contains(&src_arc) {
                pred_entry.push(src_arc);
            }
        }
    }
}
```

3. Verify tests pass

### 2.2 BUG-004: Wrong Terminal Task Selection

**Location**: `src/dag/flow.rs` + `src/runtime/runner.rs`

**Problem**: Multiple terminal nodes pick arbitrarily instead of deepest.

**TDD Steps**:

1. Add `compute_depths()` and `get_deepest_final_task()` to `Dag`
2. Modify `get_final_output()` in runner to use deepest task
3. Add tests for chain, branching, and parallel terminal scenarios

### 2.3 BUG-005: Verify After BUG-003

**Hypothesis**: BUG-005 is symptom of BUG-003. After fix:

```bash
cargo run -- run test-audit/phase3-control-flow/03-for-each-binding.nika.yaml
```

If passes → No separate fix needed.
If fails → Check `execute_task_iteration()` binding injection.

### 2.4 TUI Gap Polish (~1.5h)

| Gap | Location | Fix |
|-----|----------|-----|
| RunnerView mission tab wiring | `src/tui/views/runner.rs` | Wire state.mission_tab |
| RunnerView dag tab | `src/tui/views/runner.rs` | Wire state.dag_tab |
| OpenInStudio error status | `src/tui/app/routing.rs` | Show error on file load failure |

---

## 3. Phase 2: Chat MCP Integration

### 3.1 ChatInvoke Implementation (~4h)

**Location**: `src/tui/app/routing.rs:339-345`

**Current**:
```rust
ViewAction::ChatInvoke(tool, server, _params) => {
    self.chat_view.add_user_message(format!("/invoke {} {:?}", tool, server));
    self.set_status("Invoke requires MCP integration (not yet implemented)");
}
```

**Implementation steps**:

1. Create `chat_invoke_handler()` method in App
2. Reuse existing `McpClient` pool from agent subsystem
3. Call `mcp_client.call_tool()` with parameters
4. Stream result to chat view
5. Add proper error handling for MCP failures

### 3.2 ChatAgent Implementation (~6h)

**Location**: `src/tui/app/routing.rs:346-350`

**Implementation steps**:

1. Create `chat_agent_handler()` method
2. Instantiate `RigAgentLoop` with chat context
3. Wire MCP tools from configured servers
4. Run agent loop with streaming to chat view
5. Handle agent completion and error states

### 3.3 McpClient Pool Management (~2h)

**Pattern**:
```rust
pub struct McpClientPool {
    clients: DashMap<String, McpClient>,
    spawn_mutex: Mutex<()>,
}

impl McpClientPool {
    pub async fn get_or_create(&self, server: &str) -> Result<&McpClient> {
        // Check cache
        if let Some(client) = self.clients.get(server) {
            return Ok(client);
        }

        // Spawn with mutex to prevent race
        let _guard = self.spawn_mutex.lock().await;
        // Double-check after acquiring mutex
        if let Some(client) = self.clients.get(server) {
            return Ok(client);
        }

        // Create new client
        let client = McpClient::spawn(server).await?;
        self.clients.insert(server.to_string(), client);
        Ok(self.clients.get(server).unwrap())
    }
}
```

---

## 4. Phase 3: MCP Hardening

### 4.1 NIKA-101: Server Spawn Race (~2h)

**Problem**: Multiple tasks spawning same MCP server simultaneously.

**Fix**: Add spawn mutex per server (see McpClientPool above).

### 4.2 NIKA-102: Stdin/Stdout Mutex Conflicts (~2h)

**Problem**: Concurrent writes to MCP server stdin cause corruption.

**Fix**: RAII guard pattern for stdio operations:

```rust
pub struct IoLock {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<ChildStdout>,
}

impl IoLock {
    pub async fn call(&self, request: &[u8]) -> Result<Vec<u8>> {
        let mut stdin = self.stdin.lock().await;
        let mut stdout = self.stdout.lock().await;

        stdin.write_all(request).await?;
        stdin.flush().await?;

        let mut response = Vec::new();
        // Read response...
        Ok(response)
    }
}
```

### 4.3 NIKA-103: Reconnect Loop on Disconnect (~2h)

**Problem**: Single disconnect causes infinite reconnect attempts.

**Fix**: Exponential backoff with max retries:

```rust
pub async fn reconnect(&mut self) -> Result<()> {
    let backoff = backon::ExponentialBuilder::default()
        .with_max_times(5)
        .with_jitter();

    let op = || async {
        self.spawn_server().await
    };

    op.retry(&backoff).await
}
```

### 4.4 NIKA-104: Tool List Caching Stale (~2h)

**Problem**: Tool list cached forever, doesn't reflect server updates.

**Fix**: TTL-based cache with manual invalidation:

```rust
pub struct ToolCache {
    tools: Vec<Tool>,
    cached_at: Instant,
    ttl: Duration,
}

impl ToolCache {
    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }

    pub fn invalidate(&mut self) {
        self.cached_at = Instant::now() - self.ttl;
    }
}
```

### 4.5 NIKA-105: Timeout Not Propagating (~2h)

**Problem**: Task timeout doesn't cancel MCP operations.

**Fix**: Pass `CancellationToken` to all MCP calls:

```rust
pub async fn call_tool(&self, request: CallTool, cancel: CancellationToken) -> Result<Value> {
    tokio::select! {
        result = self.inner_call(request) => result,
        _ = cancel.cancelled() => Err(NikaError::Cancelled),
    }
}
```

---

## 5. Phase 4: Agent Completion v2.0

### 5.1 Current Status

Phase 1 (Foundation) ~60% complete:
- [x] AgentTurn metadata structure
- [x] Token tracking
- [x] Basic streaming
- [ ] Thinking capture integration
- [ ] Tool result handling

### 5.2 Remaining Work

**Phase 2: Chat Integration (~8-10h)**
- Wire RigAgentLoop to ChatView
- Multi-turn conversation support
- History management in chat context

**Phase 3: TUI Integration (~10-12h)**
- AgentActivity widget in runner view
- Streaming progress indicators
- Tool call visualization

**Phase 4: Error Handling (~6-8h)**
- Structured error recovery
- User-facing error messages
- Retry mechanisms

**Phase 5: Testing (~8-10h)**
- Unit tests for all agent methods
- Integration tests with mock providers
- E2E tests with real APIs

---

## 6. Phase 5: v0.28 Workspace Restructure

### 6.1 Crate Dependency Graph

```
nika-core (zero-dep types)
    ↑
nika-ast (parsing)
    ↑
nika-dag (validation)
    ↑
nika-mcp (MCP client)
    ↑
nika-runtime (execution)
    ↑        ↑
nika-tui    nika-cli
    ↑        ↑
      nika (binary)
```

### 6.2 File Movement Plan

| Current | Target Crate | Priority |
|---------|--------------|----------|
| `src/core/` | nika-core | P0 |
| `src/error.rs` | nika-core | P0 |
| `src/ast/` | nika-ast | P1 |
| `src/dag/` | nika-dag | P1 |
| `src/mcp/` | nika-mcp | P2 |
| `src/provider/` | nika-runtime | P2 |
| `src/runtime/` | nika-runtime | P2 |
| `src/binding/` | nika-runtime | P2 |
| `src/event/` | nika-runtime | P2 |
| `src/tui/` | nika-tui | P3 |
| `src/commands/` | nika-cli | P4 |
| `src/main.rs` | nika-cli | P4 |

### 6.3 Migration Script

```bash
#!/bin/bash
# v0.28 workspace migration script

# Phase 0: Create structure
mkdir -p crates/{nika-core,nika-ast,nika-dag,nika-mcp,nika-runtime,nika-tui,nika-cli}/src

# Phase 1: nika-core
mv src/core/* crates/nika-core/src/
mv src/error.rs crates/nika-core/src/

# Generate Cargo.toml for each crate...
```

---

## 7. Quality Gates & Checkpoints

### 7.1 Per-Phase Checkpoints

| Phase | Gate | Action |
|-------|------|--------|
| After Phase 1 | All tests pass, bugs verified fixed | Code review with rust-pro agent |
| After Phase 2 | Chat /invoke and /agent work E2E | Manual TUI testing |
| After Phase 3 | MCP stress test passes | Code review with rust-security agent |
| After Phase 4 | Agent completion tests pass | Full regression suite |
| After Phase 5 | Workspace builds, all tests pass | Code review with rust-architect agent |

### 7.2 Continuous Quality

```bash
# Run after each significant change
cargo clippy -- -D warnings
cargo test
cargo fmt -- --check
```

### 7.3 ARMADA CI Stations

All changes must pass 10-station ARMADA:

1. Format (cargo fmt)
2. Lint (clippy)
3. Tests (cargo test)
4. Coverage (>80%)
5. Docs (cargo doc)
6. Security (audit)
7. Schema Validation
8. Claude AI review
9. Conventional Commits
10. Version Lock

---

## 8. Execution Workflow

### 8.1 TDD Cycle

```
For each bug/feature:
1. Write failing test(s)
2. Run tests (confirm failure)
3. Implement minimal fix
4. Run tests (confirm pass)
5. Refactor if needed
6. Commit with conventional message
```

### 8.2 Granular Commits

Following dx/.claude/rules/git-workflow.md:

```bash
# 1 fix = 1 commit
git commit -m "fix(dag): add implicit depends_on from use: wiring

BUG-003: use: block now creates implicit dependency edges.
References to context.* and inputs.* correctly ignored.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

### 8.3 Progress Tracking

Use TodoWrite for real-time progress:

```
[ ] Phase 1 Bug Fixes
  [x] BUG-003: Write tests
  [x] BUG-003: Implement fix
  [ ] BUG-004: Write tests
  [ ] BUG-004: Implement fix
  [ ] BUG-005: Verify
  [ ] TUI Gap: mission tab
  [ ] TUI Gap: dag tab
  [ ] TUI Gap: error status
```

---

## Appendix A: Test Counts by Module

| Module | v0.27.0 | Target v0.28.0 |
|--------|---------|----------------|
| ast/ | 847 | 900 |
| dag/ | 234 | 280 |
| runtime/ | 1,245 | 1,400 |
| mcp/ | 567 | 650 |
| tui/ | 1,423 | 1,600 |
| provider/ | 312 | 350 |
| binding/ | 289 | 320 |
| event/ | 156 | 180 |
| core/ | 84 | 150 |
| **Total** | **6,157** | **6,830+** |

---

## Appendix B: Version Timeline

| Version | Target Date | Scope |
|---------|-------------|-------|
| v0.27.1 | 2026-03-12 | Bug fixes (BUG-003/004/005) + TUI polish |
| v0.27.2 | 2026-03-14 | Chat MCP integration |
| v0.27.3 | 2026-03-16 | MCP hardening |
| v0.27.4 | 2026-03-20 | Agent Completion v2.0 |
| v0.28.0 | 2026-04-15 | Workspace restructure |

---

**End of Master Implementation Plan**
