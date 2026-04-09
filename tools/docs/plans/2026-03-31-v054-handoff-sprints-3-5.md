# v0.54 Handoff — Sprints 3-5 + Research Prompts

> **Date:** 2026-03-31 | **From:** v0.54.0 release (Sprint 1+2 done) | **For:** Next session(s)
> **Status:** Sprint 1 (P0) + Sprint 2 (P1) = DONE. Sprints 3-5 remain.
> **Tests:** 9,038 green | **Codebase:** 356K LOC Rust | 12 crates

---

## What's Done (v0.54.0)

| Sprint | Items | Status |
|--------|-------|--------|
| 1. Security P0 | SEC-1, SEC-2, EXEC-1, FETCH-2, FETCH-1 | DONE |
| 2. Agent + Provider P1 | AGENT-1, SEC-AGENT-01, MCP-1 | DONE |
| 3. Runner Robustness P1 | RUNNER-1..4 | TODO |
| 4. Quality + Perf P2 | 7 items | TODO |
| 5. DX + Parser P2 | 3 items | TODO |

---

## Sprint 3: Runner Robustness (P1, ~6h)

### RUNNER-1: O(n^2) get_ready_tasks()

**File:** `nika-engine/src/runtime/runner.rs:408`
**Problem:** Each DAG loop iteration scans all n tasks x d deps to find ready ones.
**Fix:** Cache task readiness state. When a task completes, mark its dependents as "potentially ready" instead of rescanning everything.
**Effort:** 3h | **Risk:** Medium (hot path)

### RUNNER-2: Semaphore permit released early

**File:** `nika-engine/src/runtime/runner.rs:2281`
**Problem:** Concurrency semaphore permit is released before task execution completes, allowing more tasks than `concurrency` limit.
**Fix:** Hold the `OwnedSemaphorePermit` through the entire task execution span, dropping it only after the task result is stored.
**Effort:** 2h | **Risk:** Medium

### RUNNER-3: fail_fast=false skips cancelled items

**File:** `nika-engine/src/runtime/runner.rs:2594`
**Problem:** When `fail_fast: false`, cancelled for_each items are silently dropped from error summaries.
**Fix:** Track cancelled items separately and include them in the final error report.
**Effort:** 1h | **Risk:** Low

### RUNNER-4: NIKA-026 error code reused

**File:** `nika-engine/src/runtime/runner.rs:2298`
**Problem:** NIKA-026 used for both "dependency chain failed" and "semaphore acquisition failed".
**Fix:** Assign a new error code (NIKA-027?) for semaphore failures.
**Effort:** 15m | **Risk:** Zero

---

## Sprint 4: Quality + Perf (P2, ~6h)

### TRANSFORM-1: Pipe parser quote tracking only inside parens

**File:** `nika-engine/src/binding/transform.rs:152`
**Problem:** Quote tracking in pipe parser only works inside parenthesized args. `filter(it's)` breaks.
**Fix:** Track quote depth globally across the entire transform expression, not just inside parens.
**Effort:** 2h | **Risk:** Medium (parser change)

### TRANSFORM-2: Shell transform doesn't error on null

**File:** `nika-engine/src/binding/transform.rs:537`
**Problem:** `{{with.data | shell}}` on null input returns the string `'null'` instead of erroring.
**Fix:** Return error or null propagation when input is null.
**Effort:** 15m | **Risk:** Low

### PERF-1: EventLog O(n) drain()

**File:** `nika-event/src/log.rs:1186`
**Problem:** `drain()` is O(n) on the full event buffer. For workflows with thousands of events, this blocks.
**Fix:** Replace Vec with a ring buffer or use `drain(..)` range.
**Effort:** 2h | **Risk:** Low

### PERF-2: Template resolution unbounded String allocations

**File:** `nika-engine/src/binding/template.rs:370`
**Problem:** Each `{{...}}` resolution creates a new String allocation. Many templates = many allocs.
**Fix:** Pre-allocate output String with estimated capacity based on template length.
**Effort:** 30m | **Risk:** Zero

### PERF-3: Broadcast channel capacity 1024 too low

**File:** `nika-event/src/log.rs:1120`
**Problem:** Heavy for_each workflows can overflow the 1024-message broadcast channel.
**Fix:** Increase to 4096 or make configurable.
**Effort:** 5m | **Risk:** Zero

### RUNNER-5: Artifact path collisions not detected

**File:** `nika-engine/src/runtime/artifact_processor.rs`
**Problem:** Two tasks writing to the same artifact path silently overwrite.
**Fix:** Track artifact paths in a set and error on collision.
**Effort:** 1h | **Risk:** Low

### DOC-1: Dockerfile VERSION=0.52.0

**File:** `Dockerfile:56`
**Problem:** Stale version string.
**Fix:** Update to 0.54.0.
**Effort:** 5m | **Risk:** Zero

---

## Sprint 5: DX + Parser (P2, ~2h)

### AST-1: `use:` keyword silently ignored

**File:** `nika-core/src/ast/raw/parser.rs`
**Problem:** Users write `use:` (common typo for `with:`) and it's silently ignored.
**Fix:** Detect `use:` in task YAML and emit a helpful error: "did you mean `with:`?"
**Effort:** 30m | **Risk:** Zero

### AST-2: `max_retries:` at task level silently ignored

**File:** `nika-core/src/ast/raw/parser.rs`
**Problem:** `max_retries:` is only valid inside `structured:` block, but at task level it's silently dropped.
**Fix:** Detect and suggest: "did you mean `retry: { max_attempts: N }`?"
**Effort:** 30m | **Risk:** Zero

### SECRET-RE: Incomplete regex

**File:** `nika-engine/src/runtime/executor/verbs.rs:111`
**Problem:** Missing patterns for Stripe, Twilio, DB URIs.
**Status:** DONE in Sprint 1 (moved to `util::redact_secrets()`). Verify coverage is complete.
**Effort:** Already done. Just verify.

### TUI-1: Provider strings not migrated

**File:** `nika-engine/src/display/lifecycle.rs:66`
**Problem:** Some TUI code still uses raw strings instead of `ProviderName` enum.
**Fix:** Complete the migration.
**Effort:** 2h | **Risk:** Low

---

## Detailed Session Prompts

### Prompt 1: Sprint 3 — Runner Robustness

```
# Sprint 3: Runner Robustness (4 items, ~6h)

Context: Nika workflow engine v0.54.0, Rust, 12 crates, 9038 tests.
Working directory: <path>

## Task

Fix 4 runner bugs from the 20-agent deep audit:

### RUNNER-1: O(n^2) get_ready_tasks() — runner.rs:408
- Read `nika-engine/src/runtime/runner.rs` around line 408
- The current `get_ready_tasks()` scans ALL tasks every loop iteration
- Implement a "ready set" cache: when a task completes, check its dependents
- Only add newly-ready tasks to the ready set
- Must not break: concurrency limits, fail_fast, cancellation, for_each

### RUNNER-2: Semaphore permit released early — runner.rs:2281
- The OwnedSemaphorePermit is dropped before task execution completes
- Hold it through the entire task span using `let _permit = semaphore.acquire()`
- Verify with a test: launch N tasks with concurrency=1, assert serial execution

### RUNNER-3: fail_fast=false skips cancelled items — runner.rs:2594
- When fail_fast=false, cancelled for_each items are not in error summaries
- Track cancelled items and include in the NIKA-026 error message
- Add test: for_each with 5 items, first fails, fail_fast=false, verify all 5 reported

### RUNNER-4: NIKA-026 reused — runner.rs:2298
- Assign NIKA-027 for semaphore acquisition failures
- Update error_code() match arm and error code documentation

## Rules
- `cargo test --workspace --lib` after each fix (always --lib, no keychain)
- 1 fix = 1 commit with `fix(runtime): description`
- Co-authors: Claude + Nika
- Run `cargo fmt --all` and `cargo clippy` before each commit
```

### Prompt 2: Sprint 4 — Quality + Perf

```
# Sprint 4: Quality + Perf (7 items, ~6h)

Context: Nika workflow engine v0.54.0, Rust, 12 crates, 9038 tests.
Working directory: <path>

## Task

Fix 7 quality/perf items from the 20-agent audit:

### TRANSFORM-1: Quote tracking — transform.rs:152
- Pipe parser only tracks quotes inside parens
- `filter(it's)` breaks the parser
- Fix: track quote depth globally across the expression
- Test: `"hello it's world | upper"` should not split on `'`

### TRANSFORM-2: Shell null — transform.rs:537
- `{{with.data | shell}}` on null returns `'null'` string
- Fix: return NikaError or propagate null
- Test: null input → error, not string `'null'`

### PERF-1: EventLog drain — log.rs:1186
- O(n) drain on full buffer
- Fix: ring buffer or VecDeque

### PERF-2: Template allocations — template.rs:370
- Pre-allocate String with capacity estimate

### PERF-3: Broadcast channel — log.rs:1120
- Increase from 1024 to 4096

### RUNNER-5: Artifact collisions — artifact_processor.rs
- Track paths in HashSet, error on collision

### DOC-1: Dockerfile VERSION
- Update to 0.54.0

## Rules
- Same as Sprint 3 (test, fmt, clippy, 1 fix = 1 commit)
```

### Prompt 3: Sprint 5 — DX + Parser

```
# Sprint 5: DX + Parser (3 items, ~2h)

Context: Nika workflow engine v0.54.0, Rust, 12 crates, 9038 tests.
Working directory: <path>

## Task

3 DX improvements from the audit:

### AST-1: Reject `use:` keyword
- `nika-core/src/ast/raw/parser.rs` or `nika-core/src/ast/analyzer/`
- Detect `use:` in task YAML → emit NIKA-010 with "did you mean `with:`?"
- Test: workflow with `use: { data: $step1 }` → clear error

### AST-2: Reject `max_retries:` at task level
- Same files
- Detect `max_retries:` at task level → suggest `retry: { max_attempts: N }`
- Test: workflow with `max_retries: 3` at task level → clear error

### TUI-1: Complete ProviderName migration
- `nika-engine/src/display/lifecycle.rs:66` and related display files
- Replace remaining raw provider strings with ProviderName enum
- Grep for `"anthropic"\|"openai"\|"claude"` in display/ code

## Rules
- Same as Sprint 3 (test, fmt, clippy, 1 fix = 1 commit)
```

---

## Research Prompts

### Prompt 4: Competitive Analysis — Workflow Engines

```
# Competitive Research: AI Workflow Engines (2026)

Use web search (Perplexity, Firecrawl) to research these competitors
and produce a structured comparison.

## Engines to Research

1. **LangGraph** (LangChain) — Python, graph-based agent orchestration
2. **CrewAI** — Python, multi-agent framework
3. **Dify** — No-code/low-code LLM app builder
4. **Flowise** — Open-source UI for LangChain flows
5. **n8n** — Workflow automation with AI nodes
6. **Windmill** — OSS alternative to Retool with scripts
7. **Temporal** — Workflow orchestration (not AI-specific)
8. **Prefect** — Data workflow orchestration
9. **Rivet** (Ironclad) — Visual AI agent builder
10. **Haystack** (deepset) — NLP pipeline framework

## For Each Engine, Find:

1. **Positioning** — One-line tagline, target audience
2. **Pricing** — Free tier, paid tiers, enterprise
3. **Architecture** — How workflows are defined (code, YAML, visual, hybrid)
4. **LLM Support** — Which providers, how many, local model support?
5. **Differentiators** — What makes it unique?
6. **Weaknesses** — Known limitations, complaints
7. **GitHub Stars** — As proxy for adoption
8. **Latest Release** — Date and version
9. **Community** — Discord/Slack size, contributor count

## Nika's Position

After researching all competitors, answer:
1. Where does Nika fit in the landscape?
2. What are Nika's TRUE differentiators vs each?
3. Which competitor is the closest threat?
4. What features should Nika steal?
5. What positioning angle is UNIQUE to Nika?

## Output Format

Markdown table + written analysis. Save to:
`docs/research/2026-03-31-competitive-landscape.md`
```

### Prompt 5: Library Documentation Deep Dive

```
# Library Documentation Research

Use Context7 (ctx7) and web search to fetch up-to-date docs
for these critical Nika dependencies.

## Libraries to Research

### 1. rig-core 0.33.0
- Context7: `ctx7 library "rig-core"` → `ctx7 docs <id> "agent builder streaming tools"`
- Focus: AgentBuilder API, ToolDyn trait, streaming, completion model
- Questions:
  - How to add pre-call hooks for tools? (needed for SEC-AGENT-01 improvements)
  - Does rig-core support tool_choice: required?
  - How does streaming work with CompletionModel vs Agent?
  - What's the correct way to set stop_sequences?

### 2. rmcp 0.16.0
- Context7: `ctx7 library "rmcp"` → `ctx7 docs <id> "resource read tool call"`
- Focus: MCP protocol, resource reads, tool calls, server lifecycle
- Questions:
  - How to limit resource read size at the protocol level?
  - What transport options are supported (stdio, HTTP, WebSocket)?
  - How to handle MCP server reconnection?
  - What's the correct way to list available resources?

### 3. tokio 1.50 + tokio-util
- Focus: CancellationToken best practices, semaphore patterns
- Questions:
  - Best pattern for holding semaphore permits across async boundaries?
  - CancellationToken vs select! for cooperative shutdown?
  - How to implement graceful drain of in-flight tasks?

### 4. serde_json_path 0.7
- Focus: JSONPath implementation, edge cases
- Questions:
  - Does it support recursive descent (`$..field`)?
  - How does it handle null values in paths?
  - Performance characteristics for large JSON documents?

### 5. jsonschema 0.26
- Focus: JSON Schema validation, custom formats, error reporting
- Questions:
  - How to get human-readable validation error messages?
  - Does it support `$ref` resolution?
  - Performance for repeated validation with same schema?

## Output Format

For each library:
1. Current version + latest version (check if we're behind)
2. Key API patterns relevant to Nika
3. Gotchas and known issues
4. Answers to the specific questions above

Save to: `docs/research/2026-03-31-library-docs.md`
```

### Prompt 6: Overnight Autonomous Session

```
# Overnight Autonomous: Sprints 3-5 + Research

You are starting a long autonomous session on Nika v0.54.0.
Working directory: <path>

## Phase 1: Sprint 3 — Runner Robustness (~6h)
[Paste Prompt 1 content here]

## Phase 2: Sprint 4 — Quality + Perf (~6h)
[Paste Prompt 2 content here]

## Phase 3: Sprint 5 — DX + Parser (~2h)
[Paste Prompt 3 content here]

## Phase 4: Research (~2h)
[Paste Prompt 4 + 5 content here]

## Phase 5: Finalize
- Update CHANGELOG.md with all fixes
- Run full test suite: `cargo test --workspace --lib`
- Run clippy: `cargo clippy --all-targets --all-features`
- Run fmt: `cargo fmt --all`
- Commit each fix individually (1 fix = 1 commit)
- Write session handoff to `docs/plans/2026-04-01-handoff.md`

## Rules
- ALWAYS `cargo test --workspace --lib` (--lib to avoid keychain)
- 1 fix = 1 commit, conventional commits: `fix(scope): description`
- Co-authors on every commit:
  Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
- Read files before editing. Understand before modifying.
- If a fix is risky or unclear, skip it and document why in the handoff.
- Push after each sprint completes.
```

---

## Codebase Health (v0.54.0)

```
Version:           v0.54.0
Total LOC:         356K (Rust)
Test count:        9,038 (all green)
Crates:            12
Workflows:         644 (.nika.yaml)
Error codes:       158 (NIKA-000 to NIKA-324)
Providers:         7 cloud + 1 native + 1 mock
Builtin tools:     30+ (nika:*)
Dead code:         ~50 LOC
CI pipelines:      8
Release targets:   7
```

## Remaining from 20-Agent Audit

| Priority | Done | Remaining |
|----------|------|-----------|
| P0 (5)   | 5/5  | 0         |
| P1 (13)  | 6/13 | 7         |
| P2 (12)  | 1/12 | 11        |
| P3 (7)   | 0/7  | 7 (defer) |
| **Total**| **12/37** | **25** |
