# Nika Core Functionality Audit

**Date:** 2026-02-27 (Updated)
**Version:** v0.12.1
**Tests:** 3,146 passing

---

## Executive Summary

All core Nika functionality is working correctly in CLI mode.

### Features Tested ✓

| Feature | Status | Test Workflow |
|---------|--------|---------------|
| **5 Verbs** | | |
| infer: | ✓ Works | simple-infer-save.nika.yaml |
| exec: | ✓ Works | simple-exec-write.nika.yaml |
| fetch: | ✓ Works | simple-fetch-save.nika.yaml |
| invoke: (builtin) | ✓ Works | test-builtins.nika.yaml |
| agent: | ✓ Works | agent-simple.nika.yaml |
| **Parallelism** | | |
| for_each | ✓ Works | test-for-each-simple.nika.yaml |
| concurrency | ✓ Works | test-for-each-simple.nika.yaml |
| **DAG** | | |
| Diamond dependencies | ✓ Works | test-dag-complex.nika.yaml |
| Context propagation | ✓ Works | test-binding-quick.nika.yaml |
| **Providers** | | |
| Claude | ✓ Works | test-providers.nika.yaml |
| OpenAI | ✓ Works | test-providers.nika.yaml |
| Mock | ✓ Works | agent-simple.nika.yaml |
| **Builtins** | | |
| nika:sleep | ✓ Works | test-builtins.nika.yaml |
| nika:log | ✓ Works | test-builtins.nika.yaml |
| nika:emit | ✓ Works | test-builtins.nika.yaml |
| nika:assert | ✓ Works | test-builtins.nika.yaml |
| **CLI Commands** | | |
| nika provider list | ✓ Works | Tested manually |
| nika provider test | ✓ Works | Tested manually |
| nika mcp list | ✓ Works | Tested manually |
| nika trace list | ✓ Works | Tested manually |
| nika check | ✓ Works | Multiple workflows |
| nika init | ✓ Works | Tested manually |

---

## Issues Found and Fixed

### 1. Schema Validation for Builtins

**Problem:** Builtin tools (nika:*) require `mcp` field in schema even though they don't use MCP.

**Workaround:** Add dummy mcp configuration to workflows using builtins:
```yaml
mcp:
  dummy:
    command: "echo"
    args: ["not used"]
```

**Recommendation:** Update schema to make `mcp` optional for builtin tools.

### 2. Formatting Issues

**Problem:** `src/runtime/resolver.rs` had formatting issues.

**Fix:** Ran `cargo fmt`.

### 3. Flaky Startup Time Test

**Problem:** `test_startup_time_help` is flaky on macOS due to first-run quarantine scanning.

**Impact:** Low - only affects first run after compilation.

---

## CLI Feature Parity

### Implemented Commands

| Command | Description | Status |
|---------|-------------|--------|
| `nika provider list` | List all providers and status | ✓ |
| `nika provider set <name>` | Set API key for provider | ✓ |
| `nika provider test <name>` | Test connection to provider | ✓ |
| `nika provider migrate` | Migrate env vars to keychain | ✓ |
| `nika mcp list -w <file>` | List MCP servers in workflow | ✓ |
| `nika mcp test <file> <server>` | Test MCP connection | ✓ |
| `nika mcp tools <file> <server>` | List MCP tools | ✓ |
| `nika trace list` | List execution traces | ✓ |
| `nika trace show <id>` | Show trace details | ✓ |
| `nika check <file>` | Validate workflow | ✓ |
| `nika init` | Initialize project | ✓ |

---

## Test Workflows Created

1. **test-builtins.nika.yaml** - Tests all 4 main builtin tools
2. **test-dag-complex.nika.yaml** - Diamond DAG with 7 tasks
3. **test-for-each-simple.nika.yaml** - Parallel for_each execution
4. **test-providers.nika.yaml** - Multi-provider (Claude + OpenAI)

---

## Agent Loop & spawn_agent Testing

### Unit Tests (All Pass)

| Test Category | Count | Status |
|---------------|-------|--------|
| spawn_agent tests | 12 | ✓ Pass |
| rig_agent_loop tests | 7 | ✓ Pass |
| memory tests | 33 | ✓ Pass |
| agent_def tests | 5 | ✓ Pass |
| skill tests | 10 | ✓ Pass |

### spawn_agent Features Verified

- ✓ Depth limit enforcement (current >= max → blocked)
- ✓ Child depth calculation
- ✓ AgentSpawned event emission
- ✓ rig::ToolDyn implementation
- ✓ MCP client inheritance to child agents
- ✓ Three-level depth protection (root → child → grandchild)

### Agent Loop Features Verified

- ✓ Extended thinking capture
- ✓ Stop conditions matching
- ✓ System prompt injection
- ✓ RigAgentLoopResult with turns + tokens

---

## v0.6 Schema Features Testing

### memory: Field (33 tests pass)

| Feature | Status | Test |
|---------|--------|------|
| Single file loading (text) | ✓ Works | test_load_single_file_text |
| Single file loading (YAML) | ✓ Works | test_load_single_file_yaml |
| Glob pattern loading | ✓ Works | test_load_glob_files |
| Session JSON loading | ✓ Works | test_resolve_memory_path_session |
| Memory in template resolution | ✓ Works | resolve_memory_files_simple |
| Nested memory access | ✓ Works | resolve_memory_files_nested |

### agents: Field (5 tests pass)

| Feature | Status | Test |
|---------|--------|------|
| External agent file reference | ✓ Works | test_agent_def_external |
| Inline agent definition | ✓ Works | test_agent_def_inline_full |
| Minimal inline agent | ✓ Works | test_agent_def_inline_minimal |

### skills: Field (10 tests pass)

| Feature | Status | Test |
|---------|--------|------|
| Skill file loading | ✓ Works | test_load_skill |
| Missing skill error | ✓ Works | test_load_skill_missing_file |
| Mixed agents + skills | ✓ Works | test_resolve_mixed_agents_and_skills |
| Multiple skill refs | ✓ Works | test_skill_ref_multiple |

---

## Comprehensive Test Workflows (104 total)

| Category | Count | Example |
|----------|-------|---------|
| Agent tests | 47 | test-agent-sniper-complete.nika.yaml (15 tests) |
| Verb tests | 12 | simple-infer-save.nika.yaml, simple-exec-write.nika.yaml |
| DAG tests | 8 | test-dag-complex.nika.yaml |
| Parallelism tests | 6 | test-for-each-simple.nika.yaml |
| Real workflows | 31 | blog-content-pipeline.nika.yaml, code-review-assistant.nika.yaml |

---

## rig-core Integration (via Context7)

### multi_turn() API

From official docs, rig-core provides `StreamingPromptRequest.multi_turn(depth)`:
- Controls max tool calls before text response
- Default: 0 (single tool round-trip)
- Exceeding limit returns `PromptError::MaxDepthError`

**Nika implementation:** `RigAgentLoop` uses `max_turns` from `AgentParams` to control this.

---

## MCP Integration Status

### NovaNet MCP

- Binary exists and compiles: ✓
- Neo4j running: ✓
- Connection timeout: Needs pre-built binary path (not cargo run)
- **Fix applied:** Use `/Users/.../target/debug/novanet-mcp` directly

### Perplexity MCP (from research)

- Available via `@anthropic/perplexity-mcp` npm package
- Supports web search with Perplexity AI
- Can be used with `agent:` verb for research tasks

---

## Recommendations

1. **Schema Update:** Make `mcp` optional in InvokeParams for builtin tools
2. **Test Infrastructure:** Mark startup time test as `#[ignore]` on macOS
3. **Documentation:** Add builtin tools usage examples to README
4. **MCP Config:** Use pre-built binary paths instead of cargo run in workflows
5. **Timeout Tuning:** Increase MCP connect timeout for slow compiles (>20s)

---

## Session 2: Nested Workflows & Code Quality (2026-02-27)

### Runtime Wiring Verified

| Component | Status | Notes |
|-----------|--------|-------|
| `memory_loader.rs` | ✓ WIRED | Loads `memory:` files at workflow start |
| `resolver.rs` | ✓ WIRED | Resolves `agents:` and `skills:` definitions |
| `ChatWorkflow` | ✓ CORRECT | DAG data structure, not execution engine |

### Nested Workflow Testing

**nika:run builtin** is fully functional for workflow composition:

| Test | Result |
|------|--------|
| 2-level nesting (parent → child) | ✓ Works |
| 3-level nesting (L1 → L2 → L3) | ✓ Works |
| Result propagation between levels | ✓ Works |
| Separate trace per workflow | ✓ Works |

**Test workflows created:**
- `nested-parent.nika.yaml` - Parent orchestrator
- `nested-child.nika.yaml` - Child workflow
- `nested-level1.nika.yaml` - Triple nesting test
- `nested-level2.nika.yaml` - Middle level
- `nested-level3.nika.yaml` - Deepest level

### Code Quality Audit

**unwrap()/expect() in critical paths:** ✓ CLEAN

| File | Production unwraps | Test unwraps |
|------|-------------------|--------------|
| `runner.rs` | 0 | 50+ |
| `executor.rs` | 0 | 30+ |
| `rig_agent_loop.rs` | 0 | 1 |
| `mcp/client.rs` | 0 | 10+ |

All `unwrap()` calls are in test code. Production paths use proper error handling.

### CLI Commands Verified

| Command | Status | Description |
|---------|--------|-------------|
| `nika` | ✓ | TUI Home view |
| `nika chat` | ✓ | Chat agent mode |
| `nika studio` | ✓ | YAML editor |
| `nika check <file>` | ✓ | Validate workflow |
| `nika run <file>` | ✓ | Execute workflow |
| `nika provider list` | ✓ | Show provider status |
| `nika mcp list -w <file>` | ✓ | List MCP servers |
| `nika trace list` | ✓ | Show execution traces |
| `nika trace show <id>` | ✓ | Display trace events |

### Bug Fixes Applied

1. **Runner mutability** - Changed `run(&self)` to `run(&mut self)` for asset loading
2. **Test files** - Fixed 30+ test files to use `mut runner`
3. **Env var pollution** - Fixed Ollama URL tests with proper isolation
4. **Formatting** - Ran `cargo fmt` for clean code style

---

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
