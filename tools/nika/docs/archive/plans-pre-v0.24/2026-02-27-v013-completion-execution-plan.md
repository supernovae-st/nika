# v0.13 Completion Execution Plan

**Date:** 2026-02-27
**Author:** Claude Opus 4.5
**Status:** EXECUTING
**Target:** v0.13.0 release-ready

---

## Executive Summary

Based on 5 parallel agent analysis, v0.13 is **more complete than planned**:

| Feature | Expected Status | Actual Status |
|---------|-----------------|---------------|
| rig-core integration | Unknown | ✅ COMPLETE (6 providers, streaming) |
| ChatView ↔ ChatWorkflow | Missing | ✅ ALREADY WIRED |
| to_yaml() export | Missing | ✅ IMPLEMENTED (14 tests) |
| Memory bindings | Partial | ✅ COMPLETE |
| MCP stability | Unknown | ⚠️ ISSUES FOUND |
| CLI commands | Unknown | ⚠️ GAPS FOUND |

**Main work remaining:**
1. Verify all features work E2E
2. Fix MCP stability issues
3. Create complex test workflows
4. Run full test suite with ZERO failures

---

## Phase 0: Pre-Testing Verification (1 hour)

### Task 0.1: Run Full Test Suite
```bash
cd /Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika
cargo nextest run --no-fail-fast 2>&1 | tee /tmp/nika-test-results.txt
```
Expected: 2,997+ tests pass

### Task 0.2: Check Clippy Warnings
```bash
cargo clippy -- -D warnings 2>&1 | tee /tmp/nika-clippy.txt
```
Expected: Zero warnings

### Task 0.3: Verify All CLI Commands Work
```bash
nika --help
nika chat --help
nika studio --help
nika run --help
nika check --help
nika init --help
nika trace --help
nika provider --help
nika mcp --help
```

---

## Phase 1: Test rig-core Implementation (30 min)

### Task 1.1: Test Auto-Detection
```bash
# With ANTHROPIC_API_KEY set
nika run examples/test-real-claude.nika.yaml
```

### Task 1.2: Test Each Provider
- Claude: `examples/test-claude-provider.nika.yaml`
- OpenAI: `examples/test-openai-provider.nika.yaml`
- Check streaming output in TUI

### Task 1.3: Test Extended Thinking
```bash
nika run examples/test-extended-thinking.nika.yaml
```
Verify: Token tracking shows non-zero values

---

## Phase 2: Test Workflow Execution (1 hour)

### Task 2.1: Simple Workflows
```bash
nika check examples/simple-infer-save.nika.yaml
nika run examples/simple-infer-save.nika.yaml
```

### Task 2.2: Complex DAG Workflows
```bash
nika check examples/test-dag-complex-dependencies.nika.yaml
nika run examples/test-dag-complex-dependencies.nika.yaml
```

### Task 2.3: Parallel Workflows
```bash
nika check examples/test-parallel-stress.nika.yaml
nika run examples/test-parallel-stress.nika.yaml
```

### Task 2.4: Context Propagation
```bash
nika run examples/test-context-propagation.nika.yaml
nika run examples/test-deep-context-chain.nika.yaml
```

---

## Phase 3: Test MCP Integration (1 hour)

### Task 3.1: Test MCP CLI Commands
```bash
nika mcp list --workflow examples/test-novanet-mcp.nika.yaml
nika mcp tools examples/test-novanet-mcp.nika.yaml novanet
```

### Task 3.2: Test Perplexity MCP
```bash
nika run examples/test-perplexity-mcp.nika.yaml
```

### Task 3.3: Test Multi-Server MCP
```bash
nika run examples/test-mcp-multi-server.nika.yaml
```

### Task 3.4: Test Agent with MCP Tools
```bash
nika run examples/test-multi-mcp-agent.nika.yaml
```

---

## Phase 4: Test Chat System (30 min)

### Task 4.1: Launch Chat TUI
```bash
nika chat
```
Test:
- Type message, verify response
- Use @N mention, verify DAG edge created
- Type `/export yaml`, verify file created

### Task 4.2: Verify Exported YAML
```bash
cat .nika/exports/*.nika.yaml
nika check .nika/exports/*.nika.yaml
```

### Task 4.3: Run Exported Workflow
```bash
nika run .nika/exports/*.nika.yaml
```

---

## Phase 5: Create Complex Test Workflows (2 hours)

### Task 5.1: Create Parallel Subagent Workflow
Create workflow with:
- 3 parallel agents using for_each
- Each agent has different MCP server
- Results aggregated via use: bindings

### Task 5.2: Create Memory + Agent Workflow
Create workflow using:
- memory.files.* for context
- Agent with spawn_agent
- Deep context chain (6+ levels)

### Task 5.3: Create E2E API Workflow
Create workflow with:
- fetch: to real API
- infer: to process response
- exec: to save to file
- Verify entire pipeline

---

## Phase 6: Fix MCP Stability Issues (2 hours)

Based on agent analysis, fix:

### Issue 1: Exponential Backoff
File: `src/mcp/client.rs`
Change fixed 100ms sleep to exponential backoff with jitter.

### Issue 2: Increase CONNECT_TIMEOUT
File: `src/mcp/constants.rs`
Increase from 20s to 30s for cold starts.

### Issue 3: Add Health Check
Add optional periodic ping during workflow execution.

---

## Phase 7: Final Test Suite (30 min)

### Task 7.1: Run ALL Tests
```bash
cargo nextest run --no-fail-fast
```
Expected: ALL tests pass, ZERO failures

### Task 7.2: Run Integration Tests
```bash
cargo test --test '*' -- --nocapture
```

### Task 7.3: Verify Test Count
Expected: 2,997+ tests (per v0.12.1 baseline)

---

## Success Criteria

- [ ] All 2,997+ tests pass
- [ ] Zero clippy warnings
- [ ] All CLI commands work
- [ ] rig-core works with 6 providers
- [ ] Streaming works for all providers
- [ ] MCP connection stable (with timeout)
- [ ] Chat exports valid YAML
- [ ] Exported YAML runs successfully
- [ ] Complex workflows execute without errors
- [ ] Memory bindings resolve correctly

---

## Files Created/Modified

| File | Action |
|------|--------|
| `src/mcp/client.rs` | Fix exponential backoff |
| `src/mcp/constants.rs` | Increase CONNECT_TIMEOUT |
| `examples/test-v013-*.nika.yaml` | New test workflows |
| `docs/plans/2026-02-27-v013-completion-execution-plan.md` | This plan |

---

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>
