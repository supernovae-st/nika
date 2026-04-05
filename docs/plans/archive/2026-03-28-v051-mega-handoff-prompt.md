# v0.51 Mega Handoff — Master Prompt

**Copy everything below this line into a new Claude Code session.**

---

```
## Context

You are continuing work on Nika, a semantic YAML workflow engine for AI tasks.
Schema: nika/workflow@0.12 | Workspace: tools/ (Cargo workspace with 10 crates)

### Current State (2026-03-28)
- **Branch:** main, pushed to origin
- **Tests:** 8634 passed, 0 failures, 0 clippy warnings
- **Version:** v0.50.0 (NOT tagged yet — security fixes pending release)

### What was done this session (50+ commits)
1. **5 security fixes:** SVG xlink+href SSRF, api_key Debug masking, DNS rebinding pre-resolution, streaming response size limit, on_limit_reached.action wired
2. **IPv6 SSRF hardening:** fe80::/10 link-local + fc00::/7 ULA blocked
3. **10 preset unit tests** across parser/analyzer/runner
4. **Routing scaffold:** RoutingConfig wired through AST pipeline + fallback chain execution (auto-committed by hooks)
5. **Schema sync:** JSON schema aligned with parser (5 struct fixes)
6. **4 code reviews** (3 agents + 1 human review pass)

### Key Files
- Plan: `docs/plans/2026-03-28-v051-bugfix-refactor-plan.md` — READ THIS FIRST
- CLAUDE.md files: `tools/nika/CLAUDE.md` (dev reference), `nika/CLAUDE.md` (commands)
- Tests: `cargo test --workspace --lib` (ALWAYS use --lib to avoid keychain popups)

---

## YOUR MISSION: Execute the v0.51 plan in 4 waves

Read `docs/plans/2026-03-28-v051-bugfix-refactor-plan.md` first. It contains 14 tasks across 4 waves with exact file paths, line numbers, code snippets, and test requirements.

### Methodology: TDD + Verification + Bug Hunt

For EVERY task:
1. **Read the code** — understand the current state before changing anything
2. **Write the failing test FIRST** (TDD) — prove the bug exists
3. **Implement the minimal fix** — make the test pass
4. **Run tests** — `cargo test -p <crate> --lib` for the affected crate
5. **Run clippy** — `cargo clippy --workspace -- -D warnings`
6. **Commit** — 1 fix = 1 commit, conventional commits format
7. **Move to next task**

### Wave Execution Order

**Wave 1: Bug Fixes (HIGH priority, ~5h)**
- Task 1.1: MCP cache hit race (AtomicBool → ToolCallResult.was_cached)
- Task 1.2: MCP reconnect validator cache rebuild
- Task 1.3: Resource blob error tracking (invoke.rs)
- Task 1.4: Streaming timeout gap (infer_stream needs overall timeout)
- Task 1.5: Remove dead media-compression cfg guard

**Wave 2: Telemetry Hardening (~4.5h)**
- Task 2.1: Emit ProviderResponded on agent limit-exceeded paths (CRITICAL)
- Task 2.2: Emit McpRetry events in retry loop
- Task 2.3: Remove 5 dead event types from EventKind
- Task 2.4: Capture TTFT for agent verb turns

**Wave 3: Documentation & Warnings (~1.5h)**
- Task 3.1: Warn on unsupported LLM guardrails in analyzer
- Task 3.2: Warn when extended_thinking + tools conflict

**Wave 4: Refactoring (~12h)**
- Task 4.1: Split rig.rs (3,598 LOC) into 4 focused modules
- Task 4.2: Consolidate 5 provider runner methods
- Task 4.3: Merge duplicate executor test files

### After Each Wave
1. Run FULL test suite: `cd tools && cargo test --workspace --lib`
2. Run clippy: `cd tools && cargo clippy --workspace -- -D warnings`
3. Launch a code review agent to verify the wave's changes
4. Push: `git push`

---

## PARALLEL AGENT STRATEGY

Launch these agents in parallel at each wave:

### Per-Wave Agents (3 parallel)
1. **rust-pro agent** — implement tasks 1-2 of the wave
2. **rust-pro agent** — implement tasks 3-5 of the wave (if applicable)
3. **code-reviewer agent** — review completed tasks against the plan

### After All 4 Waves — Deep Verification (6 parallel)
1. **Bug hunter agent** — search for new bugs introduced by refactoring
2. **Security audit agent** — verify all SSRF/SSRF/XSS protections still work
3. **Telemetry audit agent** — verify event coverage (no orphaned events)
4. **Media pipeline E2E agent** — run real workflows with actual images
5. **Architecture review agent** — verify no circular deps, clean layering
6. **Performance audit agent** — check for hot loops, unnecessary allocations

---

## E2E WORKFLOW TESTING (CRITICAL)

After Wave 1-2, create and run REAL workflows to test media pipeline end-to-end.

### Test 1: Image Generation + Processing
```yaml
schema: "nika/workflow@0.12"
workflow: test-media-pipeline
provider: mock

tasks:
  - id: import_test
    invoke:
      tool: "nika:import"
      params:
        path: "test-fixtures/sample.png"

  - id: dimensions
    depends_on: [import_test]
    with:
      img: $import_test
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"

  - id: thumbnail
    depends_on: [import_test]
    with:
      img: $import_test
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.hash}}"
        width: 200
```

### Test 2: Fetch Binary + Artifact
```yaml
schema: "nika/workflow@0.12"
workflow: test-fetch-binary
provider: mock

tasks:
  - id: fetch_image
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
    artifact:
      path: output/test.png
      format: binary
```

### Test 3: Agent with Limits
```yaml
schema: "nika/workflow@0.12"
workflow: test-agent-limits
provider: mock

tasks:
  - id: limited_agent
    agent:
      prompt: "Count to 10"
      max_turns: 2
      limits:
        max_turns: 1
        on_limit_reached:
          action: fail
```

### Test 4: SVG Sanitization
Write a test that verifies ALL SVG attack vectors are blocked:
- `<script>`, `<foreignObject>`, `javascript:`, `file://`
- `xlink:href="http://..."`, `href="http://..."`, `href=http://...`
- `data:image/svg+xml`, `data:text/html`
- Event handlers: `onload=`, `onclick=`, `onerror=`

---

## VERIFICATION CHECKLIST

Before declaring the session complete:

- [ ] All 14 plan tasks implemented
- [ ] `cargo test --workspace --lib` = 0 failures (expect 8650+)
- [ ] `cargo clippy --workspace -- -D warnings` = 0 warnings
- [ ] 4 code review agents completed with no CRITICAL findings
- [ ] All commits follow conventional format with co-author lines
- [ ] CHANGELOG.md updated for v0.51.0
- [ ] Memory updated with session findings

---

## KNOWN ISSUES (from 6-agent deep exploration)

These are confirmed bugs — not speculation. Exact file:line references in the plan.

| # | Bug | File | Line | Severity |
|---|-----|------|------|----------|
| 1 | MCP AtomicBool cache race | nika-mcp/client.rs | 447, 936, 947 | HIGH |
| 2 | MCP reconnect drops validator cache | nika-mcp/client.rs | 807-827 | HIGH |
| 3 | Resource blob errors silently swallowed | nika-engine/executor/invoke.rs | 384-391 | MEDIUM |
| 4 | infer_stream() no overall timeout | nika-engine/provider/rig.rs | 1468 | MEDIUM |
| 5 | Dead cfg guard (media-compression) | nika-engine/runtime/runner.rs | 558-585 | LOW |
| 6 | Agent limit-exceeded skips ProviderResponded | nika-engine/rig_agent_loop/providers.rs | 10 locations | HIGH |
| 7 | McpRetry event never emitted | nika-mcp/client.rs | retry loop | MEDIUM |
| 8 | 5 dead event types in EventKind | nika-event/log.rs | various | LOW |
| 9 | Agent TTFT always None | nika-engine/rig_agent_loop/providers.rs | ProviderResponded | MEDIUM |
| 10 | LLM guardrails silently skipped | nika-core/guardrails.rs | run_sync_guardrails | MEDIUM |
| 11 | extended_thinking + tools silently ignored | nika-engine/ast/agent.rs | validate() | MEDIUM |
| 12 | Flaky test: test_cost_provider_kind_standard_providers | nika-engine/provider/rig.rs | flaky in suite | LOW |

### Architecture Context (from exploration agents)
- **Media pipeline is solid** — no critical issues, well-architected
- **Architecture is clean** — no circular deps, good layering
- **0 unused deps**, all #[allow(dead_code)] justified
- **Task lifecycle complete** — no orphaned TaskStarted events
- **Cost tracking 95%** across providers (gap: agent limit paths)
- **Routing fallback chain** was auto-scaffolded by hooks (5 commits) — verify it works

---

## COMMIT STRATEGY

```
# Wave 1
fix(mcp): return cache_hit in ToolCallResult — eliminate AtomicBool race
fix(mcp): rebuild validator cache after reconnect
fix(runtime): track fatal errors in resource blob processing
fix(provider): add overall timeout to streaming inference paths
chore(runtime): remove dead media-compression cfg guard

# Wave 2
fix(telemetry): emit ProviderResponded on agent limit-exceeded paths
fix(telemetry): emit McpRetry events in MCP retry loop
chore(telemetry): remove 5 dead event types
fix(telemetry): capture TTFT for agent verb turns

# Wave 3
fix(ast): warn on unsupported LLM guardrails in analyzer
fix(ast): warn when extended_thinking + tools conflict

# Wave 4
refactor(provider): split rig.rs into focused modules
refactor(agent): consolidate provider runner methods
chore(test): merge duplicate executor test files
```

Each commit must end with:
```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## START

1. Read `docs/plans/2026-03-28-v051-bugfix-refactor-plan.md`
2. Run `cd tools && cargo test --workspace --lib` to establish baseline
3. Create tasks for Wave 1
4. Start implementing Task 1.1 (MCP cache hit race) with TDD
5. Launch parallel agents as described above

Go.
```
