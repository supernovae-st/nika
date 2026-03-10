# Nika Comprehensive Audit Plan

**Date:** 2026-02-21
**Version:** v0.7.0
**Current State:** 1635 tests, 75.77% line coverage

## Audit Objectives

1. **Test Coverage Completeness** - Verify all features have meaningful tests
2. **Feature Implementation** - Confirm all documented features work correctly
3. **Code-Doc Alignment** - Ensure CLAUDE.md matches actual implementation
4. **DX Consistency** - Check CLI commands, error messages, help text
5. **Integration Validity** - Test real workflow execution paths

## Current Gaps Identified

### Low Coverage Modules (< 50%)

| Module | Coverage | Priority |
|--------|----------|----------|
| tui/mod.rs | 0% | HIGH |
| tui/panels/context.rs | 12% | HIGH |
| tui/panels/progress.rs | 13% | HIGH |
| runtime/rig_agent_loop.rs | 14% | CRITICAL |
| tui/app.rs | 26% | HIGH |
| tui/widgets/mcp_log.rs | 27% | MEDIUM |
| tui/widgets/header.rs | 28% | MEDIUM |
| tui/widgets/timeline.rs | 36% | MEDIUM |
| tui/widgets/infer_stream_box.rs | 42% | MEDIUM |
| tui/widgets/activity_stack.rs | 45% | MEDIUM |

### Features to Verify

1. **5 Semantic Verbs**: infer, exec, fetch, invoke, agent
2. **6 LLM Providers**: Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama
3. **MCP Integration**: Tool calls, resource access, server lifecycle
4. **Streaming**: All providers streaming correctly
5. **TUI Views**: Chat, Home, Studio (4-view architecture)
6. **Bindings**: use: block, lazy bindings, {{use.alias}} templates
7. **DAG Execution**: for_each, concurrency, fail_fast
8. **Event System**: 22 event variants, NDJSON traces

## Sniper Agent Assignments

### Agent 1: rig_agent_loop.rs Deep Audit
- Verify all 6 provider methods work
- Check streaming implementation
- Validate token tracking
- Test spawn_agent integration

### Agent 2: TUI App/Mod Integration
- Test tui/mod.rs exports
- Verify tui/app.rs event loop
- Check view transitions
- Validate keybinding consistency

### Agent 3: TUI Panels Coverage
- context.rs panel tests
- progress.rs panel tests
- reasoning.rs panel tests
- graph.rs panel tests

### Agent 4: TUI Widgets Low Coverage
- header.rs tests
- mcp_log.rs tests
- timeline.rs tests
- sparkline.rs tests

### Agent 5: Provider/MCP Integration
- RigProvider all methods
- NikaMcpTool implementation
- MCP client lifecycle
- Error handling paths

### Agent 6: CLI DX Verification
- All CLI commands work
- Help text accuracy
- Error messages helpful
- Exit codes correct

### Agent 7: CLAUDE.md Alignment
- Doc matches implementation
- Version numbers correct
- Code examples work
- No outdated info

### Agent 8: Integration Tests
- Real workflow execution
- Multi-task DAG
- MCP tool invocation
- Event emission

## Success Criteria

- [ ] All documented features have tests
- [ ] Coverage > 80% for critical modules
- [ ] Zero broken integrations
- [ ] CLI DX consistent and helpful
- [ ] CLAUDE.md 100% accurate
