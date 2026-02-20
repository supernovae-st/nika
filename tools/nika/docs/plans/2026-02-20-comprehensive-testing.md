# Nika Comprehensive Testing Plan

> **For Claude:** Execute tests in parallel using multiple agents

**Goal:** Verify all Nika features work correctly with real API calls and MCP connections

**Date:** 2026-02-20

---

## Test Categories

### Phase 1: Extended Thinking (Deep Seeking)
- Test `extended_thinking: true` with Claude
- Verify thinking field captured in AgentTurn events
- Check token counting works correctly

### Phase 2: Agent Verb with Tools
- Test multi-turn agent loops
- Verify tool calling works
- Test stop_conditions
- Test max_turns limit

### Phase 3: All 5 Verbs Live
- infer: Claude text generation
- exec: Shell commands
- fetch: HTTP requests
- invoke: MCP tool calls (mock)
- agent: Multi-turn loops

### Phase 4: Performance Benchmarks
- Run existing benchmarks
- Test concurrent execution
- Measure latency

### Phase 5: MCP Integration
- Test MCP client connection
- Test tool listing
- Test tool calling
- Test parameter validation

### Phase 6: Spawn Agent (Nested)
- Test nested agent spawning
- Verify depth limits
- Check event emission

### Phase 7: for_each Parallelism
- Test concurrent iteration
- Test fail_fast behavior
- Test result ordering

### Phase 8: Feature Completeness Audit
- Check all features are wired
- Verify no dead code paths
- Check schema validation

---

## Execution Plan

1. Launch rust-perf agent for benchmarks
2. Launch integration tests for verbs
3. Launch MCP tests
4. Test extended thinking live
5. Compile results
