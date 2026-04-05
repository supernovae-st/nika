# v0.51 Mega Handoff — Master Plan + Agent Prompt

**Date**: 2026-03-28
**Author**: 30-agent E2E audit + manual fixes
**State**: main branch, 8,598 tests pass, 0 clippy warnings, 7 commits pushed this session
**Commit**: `3f86c4e67` (HEAD)

---

## What Was Done This Session (7 commits)

| # | Commit | Type | Impact |
|---|--------|------|--------|
| 1 | `94dec6a54` | `feat(ast)` | for_each object form — parser + schema accept both syntaxes |
| 2 | `254f01b89` | `fix(runtime)` | artifact `source:` binding — JSON string parsing + MediaRef construction |
| 3 | `01ea6e07d` | `fix(binding)` | transform type mismatch error — suggests `to_string \| trim` |
| 4 | `3c14f3a3e` | `docs(plans)` | E2E bug report — 12 workflows, 7 bugs documented |
| 5 | `ad8a3857f` | `fix(provider)` | vision token estimation — was 0, now 85+ |
| 6 | `a632b5184` | `fix(security)` | template injection — context path allowlist from original template |
| 7 | `3f86c4e67` | `fix(fetch)` | strip CSS/JS from markdown extraction |

## What 30 Agents Found

**350+ workflows executed, 800+ tasks, ~$0.10 in API costs.**

### Related Documents

- `docs/plans/2026-03-28-e2e-bug-report.md` — Initial 12-workflow E2E findings
- `docs/plans/2026-03-28-v051-bugfix-refactor-plan.md` — Wave 1-4 refactor plan (mostly done)
- `docs/plans/2026-03-28-v051-mega-handoff-prompt.md` — Previous session handoff
- `docs/plans/2026-03-28-split-rig-rs-master-plan.md` — rig.rs split plan (Wave 4.1)
- `docs/plans/2026-03-28-media-handoff.md` — Media pipeline findings
- `docs/plans/2026-03-28-session-handoff.md` — Security session findings

---

## CONSOLIDATED BUG LIST — 55 Issues, Prioritized

### Wave 0: CRITICAL (6 bugs — fix first, each is a potential crash/security hole)

| ID | Bug | File:Line | Fix | Test |
|----|-----|-----------|-----|------|
| C1 | `type: llm` guardrails silently skipped — `run_guardrails_async` never implemented | `nika-core/src/ast/guardrails.rs:823` | Implement async LLM judge OR emit NIKA-112 error when `type: llm` is used (minimum viable) | Add test: `agent + guardrails: [{ type: llm }]` should error or execute judge |
| C2 | Vision cloud streaming has NO overall timeout — slow-drip stream runs forever | `nika-engine/src/provider/rig.rs:744` (`VISION_STREAM_TIMEOUT` dead_code) | Apply `VISION_STREAM_TIMEOUT` (300s) to the `vision_stream!` macro cloud path (lines 800-842) like `infer_stream` uses `STREAM_TOTAL_TIMEOUT` at line 1461 | Add test: vision stream exceeding timeout returns error |
| C3 | Agent streaming has NO overall timeout — same slow-drip vector | `nika-engine/src/runtime/rig_agent_loop/streaming.rs:39-197` | Wrap `stream_completion_with_tokens` and `stream_with_tools_streaming` in `tokio::time::timeout(STREAM_TOTAL_TIMEOUT, ...)` | Verify existing agent tests still pass |
| C4 | `nika agent` CLI panics on startup — clap short option conflict `-t` | `nika/src/main.rs:320,332` | Line 332: change `#[arg(short, long)]` to `#[arg(short = 'T', long)]` for temperature (or remove `short` entirely) | Run `nika agent --help` without panic |
| C5 | `fmt_structured_output_attempt` panics on multi-byte UTF-8 | `nika-engine/src/display/format_event.rs:420` | Change `&e[..197]` to `&e[..colors::floor_char_boundary(e, 197)]` (same pattern as lines 323, 369) | Add test with CJK characters in error message |
| C6 | Security blocklist is 100% Unix — zero Windows commands blocked | `nika-engine/src/runtime/security.rs:28-97` | Add Windows patterns: `del /f`, `format C:`, `rd /s /q`, `shutdown /s`, `powershell -c`, `cmd /c` in a `#[cfg(windows)]` block | Add Windows-specific blocklist tests |

### Wave 1: HIGH — Cost/Token Accuracy (5 bugs — money is wrong)

| ID | Bug | File:Line | Fix |
|----|-----|-----------|-----|
| H1 | Cost calculation ignores `cache_read_tokens` — never applies discount | `nika-engine/src/runtime/executor/infer.rs:474,628,696,842,1185` | Replace all 5 `calculate_cost(pk, model, in, out)` with `calculate_cost_with_cache(pk, model, in, out, cached)` |
| H2 | `infer_stream_with_options` native path reports 0 tokens | `nika-engine/src/provider/rig.rs:1917` | Add chars/4 heuristic like `infer_stream_inner` at lines 1667-1672 |
| H3 | Streaming without `Final` event produces 0/0/0 tokens | `nika-engine/src/provider/rig.rs:1415-1422` | After `consume_rig_stream()`, if tokens are 0, fallback to `estimate_tokens()` |
| H4 | Thinking tokens priced at output rate (Sonnet 3x undercount) | `nika-engine/src/provider/cost.rs:110-115` | Add `thinking_per_million: Option<f64>` to `ModelPricing`, set for Sonnet |
| H5 | nika-core catalog: `haiku-3.5` pattern doesn't match `haiku-4-5` | `nika-core/src/catalogs/cost.rs:46` | Change `model_pattern: "haiku-3.5"` to `"haiku-4"` |

### Wave 2: HIGH — Agent + Runtime (5 bugs)

| ID | Bug | File:Line | Fix |
|----|-----|-----------|-----|
| H6 | `token_budget` on agent verb never enforced | `nika-engine/src/runtime/rig_agent_loop/mod.rs` (never calls `effective_token_budget()`) | Wire `token_budget` into `LimitTracker` or add check in provider loop |
| H7 | `extended_thinking` agent path: single-turn, no tools, no retry | `nika-engine/src/runtime/rig_agent_loop/thinking.rs:314-512` | Allow multi-turn with tools in thinking mode, or validate as error at analyzer |
| H8 | Custom endpoints NOT wired for `nika infer`/`nika agent` CLI | `nika-cli/src/verbs.rs:112-118` (`one_shot_executor` passes None for custom_endpoints) | Load config and pass `resolved_endpoints` to `TaskExecutor::new()` |
| H9 | Reconnect storm: concurrent for_each MCP calls all reconnect simultaneously | `nika-mcp/src/client.rs:974-1001` | Add `AtomicBool` reconnect guard — only one task reconnects, others wait |
| H10 | `context:` files fail at runtime (NIKA-041) despite passing `nika check` | `nika-engine/src/runtime/runner.rs` (context resolution) | Debug context loading path — verify `RunContext.context_files` is populated |

### Wave 3: HIGH — CLI + Showcase + DX (4 bugs)

| ID | Bug | File:Line | Fix |
|----|-----|-----------|-----|
| H11 | `nika new` generates invalid workflow name from path | `nika-cli/src/init.rs` or wherever `nika new` lives | Extract basename only, strip invalid chars |
| H12 | Help text references nonexistent `schema list` subcommand | `nika/src/main.rs` help annotations | Remove `schema list` from help, or implement it |
| H13 | 45 showcase workflows fail: `command: \|` + `shell: true` + trailing newline | `nika-engine/src/runtime/security.rs:409` | Change `cmd.contains('\n')` to `cmd.trim_end().contains('\n')` — tolerate trailing newline from YAML `\|` |
| H14 | ~1200 LOC duplicated across `run_claude`/`run_openai`/`run_generic` | `nika-engine/src/runtime/rig_agent_loop/providers.rs` | Extract shared retry+guardrail loop into a generic method |

### Wave 4: MEDIUM (20 bugs — quality + correctness)

| ID | Bug | Short description |
|----|-----|-------------------|
| M1 | Temperature not validated per-provider (Anthropic max 1.0, OpenAI max 2.0) |
| M2 | `for_each` ordering non-sequential even with `concurrency: 1` |
| M3 | `lower_invoke` hardcodes `resource: None` — MCP resource reads broken through lower path |
| M4 | Workflow-level `routing:` parsed but never used at runtime (dead code) |
| M5 | `manifest: true` parsed but never implemented (no `artifacts.json` written) |
| M6 | Duplicate "scheduled" display lines for every dependent task |
| M7 | `fetch:` short form (`fetch: "url"`) rejected by JSON schema |
| M8 | `infer_with_tools` (Layer 0b) has NO timeout |
| M9 | `format: markdown` rejected by JSON schema (missing from enum) |
| M10 | MCP task-level `timeout:` from YAML not enforced (hardcoded 60s) |
| M11 | `{{for_each.index}}` unavailable in artifact paths |
| M12 | `extract: llm_txt` returns raw HTML fallback instead of error |
| M13 | `extract: text` includes CSS from `<style>` tags (same fix as markdown) |
| M14 | `provider_initialized` event logs wrong (default) model |
| M15 | LSP hover for `as:` says `{{item}}` instead of `{{with.item}}` |
| M16 | LSP hover for guardrails uses stale syntax (`guard:` instead of `guardrails:`) |
| M17 | LSP transform hover lists phantom transforms not in runtime |
| M18 | Schema guardrail only checks `required` fields, no full JSON Schema validation |
| M19 | Unknown CAS framing flag returns data WITH 4-byte header (corruption) |
| M20 | Cache discount fallback returns 0.1 for providers without caching |

### Wave 5: LOW (20 bugs — polish)

Includes: `join()` param can't contain pipe `|`, `compact` doesn't filter empty strings, `round` returns float while `ceil`/`floor` return int, vision TTFT always null, `python3 -c` not in exec blocklist, DNS resolution failure defaults to allow, `no artifacts.json` manifest, duplicate scheduled events, summary box breaks at w=30, no Windows CI tests, stale comments in writer.rs, error code mismatches with docs, etc.

---

## MEGA PROMPT — Copy below into next session

```
## Context

You are continuing work on Nika, a semantic YAML workflow engine.
Schema: nika/workflow@0.12 | Workspace: tools/ (Cargo workspace with 10 crates)

### Current State (2026-03-28)
- **Branch:** main, pushed to origin, commit 3f86c4e67
- **Tests:** 8,598 passed, 0 failures, 0 clippy warnings
- **Version:** v0.50.0

### Previous Session Results
- 7 commits pushed (for_each, artifact source, vision tokens, security, CSS, error messages)
- 30 agents deployed, 350+ workflows executed, 55 bugs found
- Full bug list: `docs/plans/2026-03-28-mega-handoff-master.md` — READ THIS FIRST

### Key Files
- Bug list: `docs/plans/2026-03-28-mega-handoff-master.md`
- Dev reference: `tools/nika/CLAUDE.md`
- Commands: `nika/CLAUDE.md`
- Tests: `cargo test --workspace --lib` (ALWAYS use --lib to avoid keychain popups)

### API Keys
- ANTHROPIC_API_KEY=<set from env or keychain>
- OPENAI_API_KEY already in env
- Always set NIKA_SKIP_KEYCHAIN=1

---

## YOUR MISSION: Execute Waves 0-4 from the master plan

Read `docs/plans/2026-03-28-mega-handoff-master.md` FIRST. It has exact file:line references for every bug.

### Methodology: TDD + Verify + Commit

For EVERY bug:
1. **Read the code** at the exact file:line referenced
2. **Write the failing test FIRST** (TDD) — prove the bug exists
3. **Implement the minimal fix**
4. **Run crate tests** — `cargo test -p <crate> --lib`
5. **Run clippy** — `cargo clippy --workspace -- -D warnings`
6. **Commit** — 1 fix = 1 commit, conventional commits, co-authors
7. **Push** — `git push` after each commit

### Wave Order

**Wave 0 (CRITICAL — 6 bugs, ~3h)**
C1-C6: crashes, security holes, dead features. Do these first.

**Wave 1 (Cost accuracy — 5 bugs, ~2h)**
H1-H5: money is wrong. Fix all calculate_cost callsites.

**Wave 2 (Agent + Runtime — 5 bugs, ~4h)**
H6-H10: agent verb, endpoints, MCP, context files.

**Wave 3 (CLI + DX — 4 bugs, ~2h)**
H11-H14: CLI, showcases, code duplication.

**Wave 4 (Medium — 20 bugs, ~6h)**
M1-M20: correctness, polish, LSP.

### After Each Wave
1. `cd tools && cargo test --workspace --lib` — ALL must pass
2. `cd tools && cargo clippy --workspace -- -D warnings` — 0 warnings
3. `git push`
4. Launch a code review agent to verify

### Parallel Agent Strategy

For each wave, launch 3 parallel agents:
1. **rust-pro** — implement first half of wave
2. **rust-pro** — implement second half
3. **code-reviewer** — review completed fixes

After all 5 waves, launch 6 deep verification agents:
1. Bug hunter — search for new bugs from refactoring
2. Security audit — verify SSRF/injection protections still work
3. Telemetry audit — verify event coverage
4. E2E workflow — run real workflows with Anthropic + OpenAI
5. Architecture review — verify no circular deps
6. Performance audit — check for hot loops

### E2E Verification (after all waves)

Create and run these REAL workflows (not mock):
1. Vision pipeline: import image → OpenAI vision → structured → artifact
2. Media chain: fetch binary → import → thumbnail → convert → export
3. Research pipeline: fetch 3 URLs → extract → AI analysis → report
4. Agent loop: agent with tools → completion: natural → guardrails
5. Complex DAG: 20+ tasks, for_each, mixed verbs, mixed providers

Verify ALL artifacts, ALL token counts, ALL costs.

---

## VERIFICATION CHECKLIST

Before declaring session complete:
- [ ] All Wave 0-4 bugs fixed (55 total)
- [ ] `cargo test --workspace --lib` = 0 failures (expect 8700+)
- [ ] `cargo clippy --workspace -- -D warnings` = 0 warnings
- [ ] 6 verification agents completed
- [ ] 5 real E2E workflows passed
- [ ] All commits follow conventional format with co-authors
- [ ] Memory updated with session findings
```

---

## Architecture Quick Reference

### Workspace Crates

```
tools/
├── nika/           Binary (CLI entry) — main.rs
├── nika-engine/    Execution engine — 135k LOC
├── nika-core/      AST, types, catalogs — 23k LOC
├── nika-daemon/    Background daemon — 5k LOC
├── nika-init/      Scaffolding + course — 21k LOC
├── nika-event/     EventLog, TraceWriter — 4k LOC
├── nika-mcp/       MCP client — 9k LOC
├── nika-media/     CAS store, processor — 13k LOC
├── nika-cli/       CLI subcommands — 8k LOC
├── nika-tui/       Terminal UI — 86k LOC
├── nika-lsp-core/  LSP intelligence — 9k LOC
└── nika-lsp/       LSP binary — 2.5k LOC
```

### Test Commands

```bash
cargo test --workspace --lib             # Full suite (8598 tests, safe)
cargo test -p nika-engine --lib          # Engine only (4012 tests)
cargo test -p nika-core --lib            # Core only (757 tests)
cargo test -p nika-tui --lib             # TUI only (2153 tests)
cargo clippy --workspace -- -D warnings  # Zero warnings policy
```

### Commit Format

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
