# Nika v0.23 - Master Audit Plan

**Date:** 2026-03-10
**Version:** v0.22.4 → v0.23.0
**Auditor:** Claude Opus 4.5 (15 parallel agents)
**Method:** Ultrathink + TDD + Ralph Wiggum Loop

---

## Executive Summary

Comprehensive audit of Nika v0.22.4 to verify all features work correctly before v0.23.0 release.

### Audit Scope

| Area | Components | Agent |
|------|------------|-------|
| **AST** | Raw + Analyzed, 2-phase parsing | ast-explorer |
| **Runtime** | Executor, Runner, for_each | runtime-explorer |
| **MCP** | Client, servers, tool invocation | mcp-explorer |
| **TUI** | 8 views, widgets, state | tui-explorer |
| **Providers** | 7 LLM providers, streaming | provider-explorer |
| **Bindings** | use:, templates, implicit deps | binding-tester |
| **Control Flow** | for_each, depends_on, flows | control-flow-tester |
| **Artifacts** | Output files, atomic writes | artifact-tester |
| **MCP Tools** | invoke:, server connections | mcp-tester |
| **Provider APIs** | Real API calls, all 7 | provider-tester |
| **Traces** | NDJSON, 22 event types | trace-analyzer |
| **Errors** | NIKA-000 to NIKA-289 | error-analyzer |
| **Performance** | Benchmarks, parsing, execution | perf-analyzer |
| **AST Improvements** | Better validation, spans | ast-improver |
| **DX Improvements** | CLI, TUI, Studio | dx-improver |

---

## Phase 1: Exploration (5 Agents)

### Agent 1: ast-explorer
```
MISSION: Map complete AST type system
SCOPE:
- src/ast/raw/*.rs (RawWorkflow, RawTask, RawTaskAction)
- src/ast/analyzed/*.rs (AnalyzedWorkflow, AnalyzedTask)
- src/ast/analyzer/*.rs (analyze(), feature gating)
- Schema versions v0.1 - v0.10

OUTPUT:
- Complete type inventory
- Parser coverage
- Validation rules
- Edge cases
```

### Agent 2: runtime-explorer
```
MISSION: Map execution paths
SCOPE:
- src/runtime/executor.rs (task dispatch)
- src/runtime/runner.rs (workflow orchestration)
- src/runtime/rig_agent_loop.rs (agent execution)
- for_each expansion, concurrency

OUTPUT:
- Execution flow diagrams
- State machine analysis
- Error handling paths
- Performance bottlenecks
```

### Agent 3: mcp-explorer
```
MISSION: Map MCP integration
SCOPE:
- src/mcp/client.rs (McpClient)
- src/mcp/types.rs (McpErrorCode)
- Server lifecycle management
- Tool invocation protocol

OUTPUT:
- Protocol compliance check
- Error code mapping
- Timeout handling
- Connection pooling
```

### Agent 4: tui-explorer
```
MISSION: Map TUI architecture
SCOPE:
- src/tui/views/*.rs (8 views)
- src/tui/widgets/*.rs (all widgets)
- src/tui/state.rs (AppState)
- Keyboard shortcuts

OUTPUT:
- View hierarchy
- Widget tree
- State transitions
- Key binding matrix
```

### Agent 5: provider-explorer
```
MISSION: Map LLM provider integration
SCOPE:
- src/provider/rig.rs (RigProvider)
- 7 providers: Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, Ollama
- Streaming implementation
- Token tracking

OUTPUT:
- Provider matrix
- API compatibility
- Streaming status
- Error handling
```

---

## Phase 2: Testing (5 Agents)

### Test Workflow Categories

| Category | Count | Agent |
|----------|-------|-------|
| Binding tests | 20 | binding-tester |
| Control flow tests | 25 | control-flow-tester |
| Artifact tests | 15 | artifact-tester |
| MCP tests | 20 | mcp-tester |
| Provider tests | 20 | provider-tester |
| **Total** | **100** | - |

### Agent 6: binding-tester
```
WORKFLOWS TO CREATE:
1. use: basic binding
2. use: with field path (task.field)
3. use: multiple deps
4. use: implicit depends_on (BUG-003 fix)
5. use: with context.files.*
6. use: with inputs.*
7. {{use.alias}} template resolution
8. $alias shorthand
9. Nested path access
10. Default values (?? syntax)
...
```

### Agent 7: control-flow-tester
```
WORKFLOWS TO CREATE:
1. depends_on: single
2. depends_on: multiple
3. depends_on: implicit from use:
4. for_each: literal array
5. for_each: $binding
6. for_each: with as:
7. for_each: concurrency
8. for_each: fail_fast
9. flows: section
10. Branching DAG (BUG-004 fix)
...
```

### Agent 8: artifact-tester
```
WORKFLOWS TO CREATE:
1. artifact: basic write
2. artifact: template path ({{task_id}})
3. artifact: date template
4. artifact: JSON output
5. artifact: YAML output
6. artifact: atomic write
7. artifact: security (path traversal)
8. output_schema: validation
9. output_schema: retry
10. Structured output enforcement
...
```

### Agent 9: mcp-tester
```
WORKFLOWS TO CREATE:
1. invoke: basic tool call
2. invoke: with params
3. invoke: timeout handling
4. invoke: error recovery
5. invoke: nested results
6. MCP server lifecycle
7. Multiple MCP servers
8. MCP tool discovery
9. Builtin tools (nika:*)
10. File tools (nika:read, nika:write)
...
```

### Agent 10: provider-tester
```
WORKFLOWS TO CREATE (per provider):
1. Basic infer:
2. infer: with temperature
3. infer: with system prompt
4. infer: with max_tokens
5. agent: multi-turn
6. agent: with tools
7. Streaming verification
8. Token tracking
9. Error handling
10. Extended thinking (Claude)
...
```

---

## Phase 3: Analysis (3 Agents)

### Agent 11: trace-analyzer
```
MISSION: Verify event logging
SCOPE:
- .nika/traces/*.ndjson
- 22 event types (EventKind)
- Event sequencing
- Token counts

CHECKS:
- All events serializable
- Timestamps accurate
- Task IDs consistent
- Error events captured
```

### Agent 12: error-analyzer
```
MISSION: Map error codes
SCOPE:
- src/error.rs (NikaError)
- NIKA-000 to NIKA-289
- Error recovery paths
- User messages

CHECKS:
- All codes documented
- Helpful error messages
- Correct exit codes
- miette formatting
```

### Agent 13: perf-analyzer
```
MISSION: Performance benchmarks
SCOPE:
- benches/*.rs (Criterion)
- YAML parsing speed
- DAG validation speed
- Binding resolution speed

TARGETS:
- 1 task workflow: <10ms
- 100 task workflow: <100ms
- for_each 100 items: <500ms
```

---

## Phase 4: Improvement (2 Agents)

### Agent 14: ast-improver
```
MISSION: Identify AST improvements
FOCUS:
- Better error spans
- Schema version gating
- Validation completeness
- Type safety

PROPOSALS:
- New validation rules
- Better error messages
- Performance optimizations
```

### Agent 15: dx-improver
```
MISSION: Identify DX improvements
FOCUS:
- CLI ergonomics
- TUI usability
- Studio features
- Documentation

PROPOSALS:
- New commands
- Keyboard shortcuts
- Help improvements
- Tutorial workflows
```

---

## Phase 5: Release

### Version Decision

Based on audit results:
- If no breaking changes: v0.22.5 (patch)
- If new features: v0.23.0 (minor)
- If breaking changes: v1.0.0 (major) - NOT ALLOWED

### Release Checklist

- [ ] All tests pass (4325+)
- [ ] Zero clippy warnings
- [ ] CHANGELOG updated
- [ ] README updated
- [ ] Cargo.toml version bumped
- [ ] Docker build successful
- [ ] Docker tests pass
- [ ] crates.io publish ready
- [ ] GitHub release drafted
- [ ] spn CLI integration verified

---

## Execution Timeline

| Phase | Duration | Agents |
|-------|----------|--------|
| 1. Exploration | 5 min | 5 parallel |
| 2. Testing | 10 min | 5 parallel |
| 3. Analysis | 5 min | 3 parallel |
| 4. Improvement | 5 min | 2 parallel |
| 5. Fix Issues | TBD | 1 (main) |
| 6. Release | 5 min | 1 (main) |

**Total estimated: 30 min + issue fixes**

---

## Success Criteria

1. **100% feature coverage** - All features tested
2. **Zero regressions** - All existing tests pass
3. **Zero new bugs** - No issues found in audit
4. **Performance maintained** - Benchmarks within targets
5. **Clean release** - Docker, crates, GitHub all working
