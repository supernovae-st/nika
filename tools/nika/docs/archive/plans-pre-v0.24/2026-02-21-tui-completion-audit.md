# TUI Completion Audit v2 — 10-Agent Deep Analysis

**Date:** 2026-02-21
**Version:** v0.5.2
**Audit Method:** 10 parallel Haiku agents + Context7 rig-core docs + Perplexity crate research
**Status:** Comprehensive analysis complete

---

## Executive Summary

```
┌─────────────────────────────────────────────────────────────────────────┐
│  AUDIT RESULTS — 10 PARALLEL AGENTS                                     │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  TUI WIDGETS          ████████████████████████████████████ 100% ✅     │
│  TUI VIEWS            ████████████████████████████████████ 100% ✅     │
│  WORKFLOW ENGINE      ████████████████████████████████████ 100% ✅     │
│  USER JOURNEYS        ████████████████████████████████████ 100% ✅     │
│  rig-core USAGE       ████████████████████░░░░░░░░░░░░░░░░  60% ⚠️    │
│  rmcp USAGE           ██████████████████████████░░░░░░░░░░  70% ⚠️    │
│                                                                         │
│  Tests: 1,697 passing | TODOs: 2 (both LOW) | Mock data: test-only     │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Bottom line:** The TUI and workflow engine are **production-ready**. Focus shifts to **unlocking rig-core capabilities** and **hardening MCP integration**.

---

## Part 1: What's Complete (No Action Needed)

### TUI Views — All 4 Production Ready

| View | Status | Key Features |
|------|--------|--------------|
| **ChatView** | ✅ Complete | 10-step user journey verified, streaming, MCP inline, agent turns |
| **HomeView** | ✅ Complete | Welcome screen, file browser, history, quick actions |
| **StudioView** | ✅ Complete | YAML editor, 2-phase validation, DAG preview (minimal/expanded) |
| **Monitor** | ✅ Complete | 4-panel layout, real-time events, all panels implemented |

### Chat User Journey — 10/10 Steps Working

| Step | Component | Status |
|------|-----------|--------|
| 1 | ChatView init | ✅ |
| 2 | Input handling (vim keys, clipboard) | ✅ |
| 3 | Message submit + command parse | ✅ |
| 4 | Provider dispatch (8 command types) | ✅ |
| 5 | Streaming tokens (real-time) | ✅ |
| 6 | MCP inline display | ✅ |
| 7 | Agent turns + history | ✅ |
| 8 | Session context (tokens, cost, MCP) | ✅ |
| 9 | Command palette (Cmd+K) | ✅ |
| 10 | Error handling (categorized) | ✅ |

### Workflow Engine — All Components Complete

| Component | Status | Tests |
|-----------|--------|-------|
| YAML parsing (5 schema versions) | ✅ | 45+ |
| DAG validation (cycle detection) | ✅ | 14+ |
| `infer:` (shorthand + full) | ✅ | 20+ |
| `exec:` (shorthand + full) | ✅ | 15+ |
| `fetch:` (GET/POST/PUT/DELETE) | ✅ | 10+ |
| `invoke:` (tool + resource) | ✅ | 25+ |
| `agent:` (rig-core + spawn) | ✅ | 40+ |
| `for_each` parallelism | ✅ | 14 |
| Lazy bindings (v0.5) | ✅ | 14 |
| Decompose modifier (v0.5) | ✅ | 10+ |
| Event emission (20 types) | ✅ | 15+ |

### Mock Data Audit — All Test-Only

| Location | Purpose | Production Impact |
|----------|---------|-------------------|
| `mcp/client.rs` | MCP mock for unit tests | None (test-only) |
| `provider/rig.rs` | Fake API keys for tests | None (test-only) |
| `runtime/executor.rs` | Mock injection | None (#[cfg(test)]) |
| `tests/fixtures` | Test workflows | None (test harness) |

### TODOs Found — Only 2 (Both LOW Priority)

| File | Line | TODO | Priority |
|------|------|------|----------|
| `tui/views/studio.rs` | 17 | tui-textarea ratatui 0.30 support | LOW (blocked) |
| `tui/widgets/dag.rs` | 311 | Variable height DAG nodes | LOW (cosmetic) |

---

## Part 2: CRITICAL Issue (Fix in v0.5.3)

### MCP Tool Call Timeout Missing

**Location:** `src/mcp/rmcp_adapter.rs:698-750`

**Problem:** `service.call_tool()` can hang forever — no timeout wrapper.

**Impact:** Entire workflow blocks if MCP server is unresponsive.

**Fix:**
```rust
use tokio::time::{timeout, Duration};

const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

let result = timeout(MCP_CALL_TIMEOUT, service.call_tool(request))
    .await
    .map_err(|_| NikaError::Timeout {
        operation: format!("MCP tool: {}", name),
        timeout_ms: 30000,
    })??;
```

**Effort:** 30 minutes | **Priority:** CRITICAL

---

## Part 3: HIGH Priority (v0.5.4)

### 1. MCP Error Codes Discarded

**Current:** `e.to_string()` loses JSON-RPC error codes.

**Fix:** Preserve `-32600` (INVALID_REQUEST), `-32602` (INVALID_PARAMS), etc.

**Effort:** 1 hour

### 2. MCP Resource Listing Missing

**Current:** Only `read_resource(uri)` implemented.
**Missing:** `list_resources()` for dynamic discovery.

**Effort:** 2 hours

### 3. rig-core Chat History Not Used

**Current:** `agent.prompt()` (stateless single-turn).
**Available:** `agent.chat()` (maintains conversation history).

**Benefit:** Better context retention in multi-turn agents.

**Effort:** 2 hours

### 4. Prompt Caching Tracked But Not Reported

**Current:** `cache_read_tokens` extracted but not in events.
**Fix:** Add to `ProviderResponded` event.

**Effort:** 30 minutes

---

## Part 4: MEDIUM Priority (v0.6)

### 1. Embeddings/RAG with rig-core

**Available:**
- `EmbeddingsBuilder` for document embeddings
- `InMemoryVectorStore` or `rig-lancedb`
- `dynamic_context(n, index)` for RAG agents

**Use case:** Semantic entity search in NovaNet context assembly.

**Effort:** 1 day

### 2. Additional LLM Providers

**Currently:** Claude, OpenAI
**Available:** Mistral, Cohere, Ollama, Google, Azure, Together...

**Benefits:**
- Cost optimization (Mistral cheaper)
- Offline execution (Ollama)
- Redundancy

**Effort:** 4 hours per provider

### 3. Token Counting Pre-API

**Crate:** `tiktoken-rs` (4.3M downloads)

**Use case:** Estimate tokens before API call, prevent budget overrun.

**Effort:** 2 hours

### 4. Structured JSON Output

**rig-core:** `response_format: "json_schema"`

**Use case:** Validated task decomposition responses.

**Effort:** 3 hours

### 5. MCP Prompts API

**Missing:** `list_prompts()`, `get_prompt()`

**Use case:** Server-provided prompt templates.

**Effort:** 3 hours

---

## Part 5: LOW Priority (v0.7+)

| Issue | Description | Effort |
|-------|-------------|--------|
| tui-textarea | Blocked on ratatui 0.30 | Track upstream |
| DAG variable height | Cosmetic improvement | 2 hours |
| MCP health checking | Periodic ping on idle | 2 hours |
| Vision/multimodal | rig-core has ContentBlock::Image | 1 day |
| Circuit breaker | Fail-fast on cascading errors | 3 hours |

---

## Part 6: Crate Recommendations

### Keep Using (Excellent Choices)

| Crate | Version | Rating |
|-------|---------|--------|
| **rig-core** | 0.31 | ⭐⭐⭐⭐⭐ |
| **rmcp** | 0.16 | ⭐⭐⭐⭐⭐ |
| **ratatui** | 0.30 | ⭐⭐⭐⭐⭐ |
| **tokio** | latest | ⭐⭐⭐⭐⭐ |

### Consider Adding

| Crate | Purpose | Downloads |
|-------|---------|-----------|
| **tiktoken-rs** | Token counting | 4.3M |
| **llm_json** | JSON repair | 13K |
| **ollama-rs** | Local LLM | 231K |
| **rig-lancedb** | Vector store | — |

### Not Needed

| Crate | Reason |
|-------|--------|
| **async-openai** | rig-core wraps it |
| **llm-chain** | Abandoned (2023) |
| **claudius** | Less mature than rig |

---

## Part 7: Implementation Roadmap

### v0.5.3 (Stability) — This Week

- [ ] **CRITICAL:** Add MCP timeout enforcement
- [ ] Preserve MCP error codes
- [ ] Report cached tokens in events

### v0.5.4 (Reliability) — Next Week

- [ ] Add MCP resource listing
- [ ] Implement provider retry logic
- [ ] Enhanced AgentTurnMetadata

### v0.6 (Features) — 2 Weeks

- [ ] rig-core chat history in agent loop
- [ ] Additional providers (Mistral, Ollama)
- [ ] Token counting with tiktoken-rs
- [ ] Structured JSON output mode
- [ ] Embeddings/RAG foundation

### v0.7 (Advanced) — Future

- [ ] RAG with rig-lancedb + NovaNet
- [ ] MCP prompts API
- [ ] Vision/multimodal workflows

---

## Part 8: rig-core Capability Matrix

| Capability | Status | Priority |
|------------|--------|----------|
| Provider abstraction | ✅ Using | — |
| AgentBuilder + tools | ✅ Using | — |
| Extended thinking | ✅ Using | — |
| Streaming + tokens | ✅ Using | — |
| Chat history | ❌ Not using | HIGH |
| Embeddings/RAG | ❌ Not using | MEDIUM |
| Vision/multimodal | ❌ Not using | LOW |
| Multiple providers (20+) | ⚠️ Partial (2/20) | MEDIUM |
| Structured output | ⚠️ Partial | MEDIUM |
| Prompt caching | ⚠️ Tracked only | LOW |

---

## Part 9: rmcp Capability Matrix

| Capability | Status | Priority |
|------------|--------|----------|
| Tool calling | ✅ Using | — |
| Tool discovery | ✅ Using | — |
| Resource reading | ✅ Using | — |
| Connection management | ✅ Using | — |
| **Timeout enforcement** | ❌ MISSING | **CRITICAL** |
| Error code preservation | ❌ Missing | HIGH |
| Resource listing | ❌ Missing | HIGH |
| Prompts API | ❌ Missing | MEDIUM |
| Health checking | ❌ Missing | LOW |
| Streaming responses | ❌ Missing | LOW |

---

## Test Coverage

| Category | Count | Status |
|----------|-------|--------|
| Unit tests (lib) | 1,133 | ✅ Pass |
| Integration tests | 564 | ✅ Pass |
| Snapshot tests | 6 | ✅ Pass |
| **Total** | **1,697** | ✅ |

---

## Conclusion

The 10-agent audit confirms:

1. **TUI is production-ready** — All 4 views, 10-step chat journey, 7 widgets complete
2. **Workflow engine is complete** — All 5 verbs, for_each, lazy bindings, decompose
3. **1 CRITICAL gap** — MCP timeout (fix immediately)
4. **4 HIGH gaps** — MCP errors, resource listing, chat history, cache reporting
5. **rig-core at 60%** — Major opportunities in embeddings/RAG, providers, structured output

**Next action:** Fix MCP timeout in v0.5.3, then unlock rig-core capabilities in v0.6.
