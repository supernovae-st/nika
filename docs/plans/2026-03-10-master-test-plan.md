# Master Test Plan: Nika v0.24.0 Comprehensive Verification

**Date:** 2026-03-10
**Version:** v0.24.0 (Native Inference + 8-View TUI)
**Scope:** Full functionality verification across all systems

---

## Executive Summary

This plan deploys 10 parallel Opus 4.5 agents to exhaustively verify Nika v0.24.0 functionality. Each agent focuses on a specific domain, running real workflows with real assertions.

---

## Agent Assignments

### Agent 1: YAML Parser & Workflow Validation
**Focus:** AST parsing, schema validation, error recovery
**Tests:**
- [ ] All 5 verbs parse correctly (infer, exec, fetch, invoke, agent)
- [ ] Binding syntax: `use.alias`, `use.ctx`, `for_each`
- [ ] Schema validation with output: directive
- [ ] Error messages are helpful and accurate
- [ ] Edge cases: empty workflows, circular deps, malformed YAML

### Agent 2: DAG Execution & Dependencies
**Focus:** Task ordering, parallel execution, data flow
**Tests:**
- [ ] Linear task chains execute in order
- [ ] Parallel tasks run concurrently (verify with timing)
- [ ] for_each creates correct task instances
- [ ] Binding resolution: {{use.alias.field}}
- [ ] Nested bindings work correctly
- [ ] Task failure propagation
- [ ] Retry logic (retry: 3)

### Agent 3: Provider Integration
**Focus:** All LLM providers work correctly
**Tests:**
- [ ] Claude (Anthropic) - streaming + non-streaming
- [ ] OpenAI - GPT-4o, o3-mini
- [ ] Mistral - mistral-large
- [ ] Groq - llama3-70b
- [ ] DeepSeek - deepseek-chat
- [ ] Gemini - gemini-2.0-flash
- [ ] Ollama - local models
- [ ] Native (NEW) - GGUF via mistral.rs
- [ ] Provider auto-detection (provider: auto)
- [ ] Model override (model: field)
- [ ] Temperature, max_tokens, system prompt

### Agent 4: MCP Client & Tool Calling
**Focus:** invoke: verb and tool integration
**Tests:**
- [ ] invoke: with novanet_* tools
- [ ] invoke: with custom MCP servers
- [ ] Tool parameter passing
- [ ] Tool result binding (use.result)
- [ ] Multiple tools in sequence
- [ ] Tool error handling
- [ ] MCP reconnection on failure

### Agent 5: Event System & Logging
**Focus:** Silent errors, event flow, debugging
**Tests:**
- [ ] TaskStarted, TaskCompleted, TaskFailed events
- [ ] AgentTurn events with thinking field
- [ ] StreamChunk events for streaming
- [ ] ToolCall events for agent: verb
- [ ] Log levels (debug, info, warn, error)
- [ ] No silent failures (all errors logged)
- [ ] Event ordering is correct
- [ ] Cost calculation accuracy

### Agent 6: TUI Functionality
**Focus:** All 8 views work correctly
**Tests:**
- [ ] HomeView - welcome screen, version display
- [ ] WorkflowView - file browser, workflow list
- [ ] ChatView - message history, input
- [ ] RunnerView - DAG visualization, mission/dag tabs
- [ ] StudioView - YAML editor, live preview
- [ ] LogView - event streaming, filtering
- [ ] SettingsView - provider config, secrets
- [ ] HelpView - keybindings, documentation
- [ ] Keyboard shortcuts (Ctrl+Z, Tab, etc.)
- [ ] Mouse support
- [ ] Screen resize handling

### Agent 7: Output Schemas & Structured Data
**Focus:** JSON-LD, schema validation, type safety
**Tests:**
- [ ] output: directive with JSON schema
- [ ] Structured output parsing
- [ ] Schema validation errors are clear
- [ ] JSON-LD context injection
- [ ] Complex nested schemas
- [ ] Array schemas
- [ ] Optional fields handling
- [ ] Default values

### Agent 8: Secrets & Configuration
**Focus:** spn-daemon integration, env resolution
**Tests:**
- [ ] Keychain key retrieval via daemon
- [ ] Environment variable fallback
- [ ] .env file loading
- [ ] ${spn:provider} syntax in MCP configs
- [ ] Secret masking in logs
- [ ] No secrets leaked in errors
- [ ] Config file merging (global/team/local)

### Agent 9: Agent Verb & Agentic Loops
**Focus:** agent: verb functionality
**Tests:**
- [ ] Basic agent loop execution
- [ ] max_turns limit respected
- [ ] Tool use within agent loop
- [ ] Thinking/reasoning capture
- [ ] Agent abort on failure
- [ ] Multi-step reasoning chains
- [ ] Context preservation across turns
- [ ] Memory/state management

### Agent 10: Documentation & ADR Compliance
**Focus:** Docs accuracy, ADR adherence
**Tests:**
- [ ] CLAUDE.md reflects current code
- [ ] README has correct examples
- [ ] CHANGELOG is complete for v0.24
- [ ] All ADRs are followed in code
- [ ] API documentation is accurate
- [ ] Code comments match behavior
- [ ] Version numbers are consistent

---

## Test Workflow Categories

### Category A: Basic Workflows (20)
1. hello-world.yaml - Single infer
2. echo.yaml - exec: with output
3. fetch-json.yaml - HTTP GET
4. invoke-tool.yaml - MCP tool call
5. agent-simple.yaml - Basic agent loop
... (15 more)

### Category B: Data Flow (20)
1. binding-chain.yaml - A→B→C data flow
2. foreach-list.yaml - Iterate over list
3. foreach-nested.yaml - Nested iteration
4. parallel-tasks.yaml - Concurrent execution
5. conditional-exec.yaml - if: conditions
... (15 more)

### Category C: Provider Tests (20)
1. claude-streaming.yaml - Stream response
2. openai-json-mode.yaml - Structured output
3. mistral-tools.yaml - Tool calling
4. groq-fast.yaml - Speed test
5. native-local.yaml - GGUF inference
... (15 more)

### Category D: Error Handling (20)
1. missing-key.yaml - API key not set
2. invalid-yaml.yaml - Parse error
3. timeout.yaml - Request timeout
4. retry-success.yaml - Retry mechanism
5. circular-dep.yaml - Circular dependency
... (15 more)

### Category E: Integration (20)
1. novanet-generate.yaml - Full NovaNet flow
2. multi-provider.yaml - Multiple providers
3. complex-dag.yaml - 10+ tasks
4. real-workflow.yaml - Production workflow
5. stress-test.yaml - 50 concurrent tasks
... (15 more)

---

## Success Criteria

- [ ] All 100 workflows execute without silent errors
- [ ] All assertions pass
- [ ] No memory leaks detected
- [ ] No panics or crashes
- [ ] All providers respond correctly
- [ ] Documentation matches implementation
- [ ] ADRs are followed

---

## Execution Plan

1. **Phase 1:** Run all unit tests (cargo test)
2. **Phase 2:** Run integration tests (cargo test --features integration)
3. **Phase 3:** Execute workflow suites per agent
4. **Phase 4:** Cross-validate findings
5. **Phase 5:** Generate consolidated report

---

## Output

Each agent produces:
- Summary of tests run
- List of failures/bugs found
- Recommendations for fixes
- Code snippets for issues
