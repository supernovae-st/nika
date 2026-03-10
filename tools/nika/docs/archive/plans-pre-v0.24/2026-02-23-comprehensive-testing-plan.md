# Comprehensive Testing Plan for Nika

**Date:** 2026-02-23
**Version:** v0.7.2
**Goal:** Create exhaustive tests covering ALL Nika capabilities

## Overview

This plan creates a comprehensive test suite that challenges every aspect of Nika:
- All 5 verbs simultaneously
- All binding syntaxes (`{{use.var}}` and `$var`)
- All AgentParams fields
- Real MCP servers (NovaNet, Perplexity, Firecrawl)
- Real API providers (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama)
- Complex DAG patterns (diamond, fan-in, fan-out, deep chains)
- Large-scale stress tests (100+ tasks)
- Edge cases and error handling

## Phase 1: Agent Sniper Tests (CURRENT)

Create tests that explore ALL AgentParams possibilities:

```rust
// AgentParams fields to test:
- prompt: String              // Required - main agent goal
- system: Option<String>      // System prompt
- provider: Option<String>    // claude, openai, mistral, groq, deepseek, ollama
- model: Option<String>       // Specific model override
- mcp: Vec<String>            // MCP servers to use
- max_turns: Option<u32>      // Limit iterations
- token_budget: Option<u32>   // Token limit
- stop_conditions: Vec<String> // Early termination
- scope: Option<String>       // Agent scope
- extended_thinking: Option<bool> // Claude thinking capture
- thinking_budget: Option<u64> // Max thinking tokens
- depth_limit: Option<u32>    // Nested agent depth
```

### Test Files to Create:

1. **`examples/test-agent-sniper-complete.nika.yaml`**
   - Agent with ALL AgentParams fields populated
   - Multiple MCP servers
   - Extended thinking enabled
   - spawn_agent depth testing

2. **`examples/test-agent-providers.nika.yaml`**
   - One task per provider (6 providers)
   - Compare outputs
   - Verify provider switching

3. **`examples/test-agent-stop-conditions.nika.yaml`**
   - Various stop condition patterns
   - Early termination testing
   - Token budget enforcement

4. **`tests/comprehensive/agent_sniper_tests.rs`**
   - Unit tests for all AgentParams parsing
   - Integration tests for agent execution

## Phase 2: Binding Syntax Tests

Test BOTH binding syntaxes everywhere:

```yaml
# Mustache syntax
{{use.alias}}

# Dollar syntax
$alias
```

### Test Scenarios:

1. Bindings in `infer:` prompts
2. Bindings in `exec:` commands
3. Bindings in `fetch:` URLs
4. Bindings in `invoke:` params
5. Bindings in `agent:` prompts
6. Mixed syntaxes in same prompt
7. Nested bindings (JSON paths)
8. Array access via bindings
9. Default values for lazy bindings

## Phase 3: MCP Integration Tests

Real MCP server tests (require live servers):

1. **NovaNet MCP** (`examples/test-mcp-novanet-full.nika.yaml`)
   - All 8 tools: describe, traverse, generate, atoms, query, assemble, search, introspect
   - Entity context propagation
   - Locale-specific generation

2. **Perplexity MCP** (`examples/test-mcp-perplexity.nika.yaml`)
   - Web search integration
   - Result processing
   - Error handling

3. **Firecrawl MCP** (`examples/test-mcp-firecrawl.nika.yaml`)
   - URL scraping
   - Content extraction
   - Rate limiting

4. **Multi-MCP** (`examples/test-mcp-multi-server.nika.yaml`)
   - Multiple servers simultaneously
   - Cross-server data flow
   - Server priority

## Phase 4: DAG Edge Case Tests

Complex dependency patterns:

```
Diamond:     A → B
             ↓   ↓
             C ← D

Fan-out:     A → [B, C, D, E, F]

Fan-in:      [A, B, C, D] → E

Deep chain:  A → B → C → D → E → ... (10+ levels)

Wide:        100 parallel independent tasks
```

### Test Files:

1. **`examples/test-dag-diamond.nika.yaml`**
2. **`examples/test-dag-fanout-10.nika.yaml`**
3. **`examples/test-dag-fanin-10.nika.yaml`**
4. **`examples/test-dag-deep-chain-20.nika.yaml`**
5. **`tests/comprehensive/dag_edge_case_tests.rs`**

## Phase 5: Large-Scale Stress Tests

Stress testing with:

1. **100+ task workflows**
2. **Deep nesting (20+ levels)**
3. **Wide parallelism (50+ concurrent)**
4. **Large payloads (1MB+ context)**
5. **Long chains with binding propagation**

## Phase 6: Error Handling Tests

Test all error codes (NIKA-XXX):

1. **Parse errors** (NIKA-000-009)
2. **Task errors** (NIKA-010-019)
3. **DAG errors** (NIKA-020-029)
4. **Provider errors** (NIKA-030-039)
5. **Binding errors** (NIKA-040-049)
6. **MCP errors** (NIKA-100-109)
7. **Agent errors** (NIKA-110-119)

## Phase 7: Invalid Workflow Tests

Ensure proper rejection of:

1. **Cyclic dependencies**
2. **Missing task references**
3. **Invalid verb syntax**
4. **Unknown verbs**
5. **Invalid MCP config**
6. **Missing required fields**
7. **Type mismatches**

## Phase 8: CI/CD Pipeline Tests

Create GitHub Actions workflow:

1. **Smoke tests** (fast, no API keys)
2. **Unit tests** (cargo test)
3. **Integration tests** (real APIs, secrets required)
4. **Stress tests** (large workflows)

## Execution Order

1. ✅ Phase 1: Agent sniper tests (IN PROGRESS)
2. ⏳ Phase 2: Binding syntax tests
3. ⏳ Phase 3: MCP integration tests
4. ⏳ Phase 4: DAG edge case tests
5. ⏳ Phase 5: Large-scale stress tests
6. ⏳ Phase 6: Error handling tests
7. ⏳ Phase 7: Invalid workflow tests
8. ⏳ Phase 8: CI/CD pipeline tests

## Files Created So Far

- `examples/test-all-verbs-complex.nika.yaml` ✅
- `examples/test-edge-cases.nika.yaml` ✅
- `examples/test-stress-large-dag.nika.yaml` ✅
- `tests/comprehensive/mod.rs` ✅
- `tests/comprehensive/all_verbs_tests.rs` ✅
- `tests/comprehensive/edge_case_tests.rs` ✅

## Success Criteria

- All tests pass with `cargo test`
- All example workflows validate with `nika check`
- No compilation errors
- 80%+ code coverage
- All 5 verbs tested
- All binding syntaxes tested
- All providers tested
- All MCP tools tested
