# Session Review Findings — Final Audit of v0.51 Quality Overhaul

**Date**: 2026-03-28
**Reviewer**: Opus 4.6 deep code audit
**State**: main @ 1e87d5111, 8598+ tests, 0 clippy warnings

---

## 1. VERIFICATION: Fixes Claimed as "Done"

### 1.1 CONFIRMED FIXED

| ID | Bug | Evidence |
|----|-----|----------|
| C1 (partial) | LLM guardrails silently skipped | `has_llm_guardrails()` check in `thinking.rs:58` emits `GuardrailFailed` event and returns `FailedImmediate`. However, async LLM judge is NOT implemented -- only the silent skip was fixed. |
| C2 | Vision stream no timeout | `VISION_STREAM_TIMEOUT` (300s) applied at `rig.rs:770,827`. No `dead_code` attribute. Wraps both native and cloud paths. |
| C3 | Agent stream no timeout | `AGENT_STREAM_TOTAL_TIMEOUT` (600s) at `streaming.rs:22`, applied at lines 102, 309, 464 across all three streaming paths. |
| C4 | `nika agent` clap `-t` conflict | Line 332: `#[arg(short = 'T', long)]` for temperature. |
| C5 | `fmt_structured_output_attempt` UTF-8 panic | Line 420: `colors::floor_char_boundary(e, 197)` used correctly. Test at line 723 with CJK characters. |
| C6 | No Windows commands in blocklist | `del /f`, `format c:`, `cmd /c`, `powershell -c`, `runas` all present in BLOCKLIST. Tests at lines 627-654. |
| H1 | `calculate_cost_with_cache` not used | 5 callsites in `infer.rs` (lines 474, 629, 698, 845, 1189) + 8 in `providers.rs`. All using `_with_cache` variant. |
| H10 | Context shorthand | `Shorthand: context.<alias>` implemented at `run_context.rs:483`. Tests at lines 1065-1071. |
| H13 | Trailing newline in shell commands | `cmd.trim_end().contains('\n')` at `security.rs:435`. |
| M3 | `lower_invoke` hardcodes `resource: None` | Fixed: `resource: invoke.resource` at `lower.rs:255`. |

### 1.2 NOT FIXED (Claimed or Expected to Be Done)

| ID | Bug | Status | Evidence |
|----|-----|--------|----------|
| S1+S2 | Block `bash -c`, `zsh -c`, `python3 -c` generically | **NOT FIXED** | Blocklist only has `"python -c \"import socket"` and `"python3 -c \"import socket"`. No `bash -c`, `zsh -c`, `sh -c`, `dash -c` entries. `python3 -c "import os"` bypasses. |
| SF1 | DNS failure defaults to ALLOW | **NOT FIXED** | `policy.rs:106-112`: Both `Ok(Err(e))` and `Err(_)` (timeout) return `false` (allow). Log level is still `debug!`, not `warn!`. This is fail-open SSRF. |
| SF5 | `jsonschema::validator_for().ok()` silently disables validation | **NOT FIXED** | `runner.rs:656`: Still `.ok()` -- invalid schema silently becomes None, disabling structured output validation for the entire retry loop. |
| SF6 | EventLog drops trace writes with `let _ =` | **UNVERIFIED** | Not checked in detail, plan says to fix. |
| CR1 | SchemaGuardrail only checks `required` | **NOT FIXED** | `guardrails.rs:332-381`: `check()` method only validates required field presence. No type checking, pattern matching, enum validation, or numeric ranges. Comment at line 331 explicitly says "For now, we just verify required fields". |
| M-sec1 | `xargs`, `find -exec` not blocked | **NOT FIXED** | Not in blocklist. |
| M-sec4 | `redact_for_event` doesn't redact API keys | **NOT FIXED** | `verbs.rs:95-106`: Only truncates at 200 bytes. No `sk-*` or `Bearer *` pattern redaction. |
| EV2 | Chat path never emits `ProviderResponded` | **NOT FIXED** | Zero matches for `ProviderResponded` in `chat.rs`. |
| SF9 | `token_budget` never enforced | **NOT FIXED** | Zero references to `token_budget` in entire `rig_agent_loop/` directory. Parsed and stored in `AgentParams` (agent.rs:119), `effective_token_budget()` exists (agent.rs:240), but never wired into `LimitTracker`. |
| M1 | Temperature not validated per-provider | **NOT FIXED** | No `max_temperature` function. Temperature validation exists generically (0.0-2.0 range in action.rs) but not per-provider (Anthropic max 1.0). |
| M5 | `manifest: true` never writes `artifacts.json` | **NOT FIXED** | Zero code paths write an `artifacts.json` file. Templates reference `manifest: true` but it's dead config. |
| M11 | `{{for_each.index}}` unavailable in artifact paths | **NOT FIXED** | Zero matches for `for_each.index` or `for_each_index` in engine source. |
| M4 | `routing:` parsed but dead code | **NOT FIXED** | `nika-core/src/ast/routing.rs` has `#[allow(dead_code)]` on line 38. Parsed but never used at runtime. |
| H14 | 1200 LOC duplicated in providers.rs | **NOT FIXED** | `providers.rs` is still exactly 1505 lines. Three near-identical methods remain. |

### 1.3 PARTIALLY FIXED

| ID | Bug | Status |
|----|-----|--------|
| H1 | Cache discount in cost | **5/8 callsites fixed.** `structured_output.rs:206`, `thinking.rs:520`, `chat.rs:336` still use `calculate_cost()` without cache parameter. |
| C1 | LLM guardrails | Silently-skipping behavior replaced with explicit error + event emission. But the actual LLM judge feature is not implemented. Acceptable as minimum viable. |

---

## 2. BUGS IN NONE OF THE PLANS

### 2.1 `unsafe` Blocks (30+ occurrences, none in plans)

The codebase has 30+ `unsafe` blocks across `nika-daemon`, `nika-cli`, and `nika-lsp`:

- **`nika-daemon/src/services/secrets.rs`**: `unsafe { std::env::remove_var(...) }` (3 occurrences)
- **`nika-daemon/src/server.rs`**: `unsafe { std::env::remove_var("NIKA_HOME") }` (6 occurrences in tests)
- **`nika-cli/src/onboarding.rs:132`**: `unsafe { std::env::set_var(env_var, &api_key) }`
- **`nika-cli/src/provider.rs`**: `unsafe { std::env::set_var(...) }` (2 occurrences)
- **`nika-cli/src/verbs.rs`**: `unsafe { std::env::set_var/remove_var }` (7 occurrences including tests)
- **`nika-lsp/tests/e2e_harness.rs`**: `unsafe { libc::fcntl(...) }` (4 occurrences)

These are `unsafe` because `std::env::set_var`/`remove_var` became unsafe in Rust 1.66+. While
the tests are mostly serial, the production code in `onboarding.rs` and `provider.rs` calls
`set_var` from potentially concurrent contexts. This is undefined behavior per the Rust safety
model.

**Fix**: Use `std::sync::OnceLock` or collect env vars at startup. For tests, use `serial_test` crate.

**None of the 6 session plans mention this.**

### 2.2 `#[allow(dead_code)]` Hiding Real Issues (42 occurrences)

Notable non-test instances in production code:

| File | Line | Comment/Justification |
|------|------|----------------------|
| `nika-core/src/ast/routing.rs:38` | Full struct | `#[allow(dead_code)]` on routing config -- confirms M4 (dead code) |
| `nika-engine/src/runtime/runner.rs:148` | Field | Dead field in production struct |
| `nika-engine/src/runtime/rig_agent_loop/mod.rs:86` | Field | "Will be used when run_claude is fully implemented" -- stale promise |
| `nika-mcp/src/rmcp_adapter.rs:154` | Field | Unexplained |
| `nika-lsp/src/position.rs` | 7 items | Entire file mostly dead |
| `nika-lsp/src/ast_integration.rs` | 4 items | Mostly dead code |
| `nika-lsp/src/daemon_bridge.rs` | 3 items | "Used incrementally" -- but not yet |

### 2.3 Reachable `unreachable!()` (5 occurrences in production code)

| File | Line | Risk |
|------|------|------|
| `nika-engine/src/runtime/runner.rs:5612` | `unreachable!()` | Could panic on unexpected task state |
| `nika-engine/src/binding/template.rs:313` | `_ => unreachable!()` | Could panic on unexpected token type |
| `nika-engine/src/provider/rig.rs:721,723` | `unreachable!("DeepSeek/Native handled above")` | Relies on match ordering; refactoring could break |
| `nika-engine/src/provider/rig.rs:837,839` | Same pattern in vision path | Same risk |
| `nika-cli/src/model_cmd.rs:193` | `unreachable!("cloud actions handled before delegation")` | Assumes control flow |

### 2.4 `_ => {}` Without Logging (60 occurrences across 40 files)

The master plan Rule 2 says every `_ => {}` must at minimum `tracing::warn!()`. There are 60
instances across the codebase. Key production-path violations:

- `nika-engine/src/runtime/runner.rs:996`
- `nika-engine/src/runtime/executor/mod.rs:463`
- `nika-engine/src/provider/rig.rs:1437,2205`
- `nika-engine/src/runtime/rig_agent_loop/streaming.rs:362,613`
- `nika-engine/src/runtime/rig_agent_loop/mod.rs:572`
- `nika-engine/src/display/renderer.rs:303,1335`

### 2.5 `unwrap_or(0)` in Production Code (50+ occurrences)

The master plan Rule 1 says every `unwrap_or(0)` must be replaced with explicit logging.
There are 50+ instances. Most are in LSP/TUI/display code (acceptable for line numbers),
but several are in critical paths:

- `nika-daemon/src/services/jobs.rs:178`: `child.id().unwrap_or(0)` -- PID silently zero
- `nika-engine/src/display/summary.rs:448-449`: TTFT min/max silently zero
- `nika-core/src/binding/transform.rs:416`: Round decimals default to 0

### 2.6 Three `calculate_cost()` Callsites Still Missing Cache

| File | Line | Context |
|------|------|---------|
| `structured_output.rs:206` | `estimate_cost()` | Uses non-cache variant |
| `thinking.rs:520` | Extended thinking result | Hardcodes `ProviderKind::Claude`, no cache |
| `chat.rs:336` | Chat turn cost | Uses non-cache variant |

---

## 3. DOCUMENTATION vs REALITY

### 3.1 `nika-event/src/log.rs` Doc Mismatch

**Doc says**: "44 variants across 15 categories" (line 5)
**Reality**: 58 variants across 18+ categories

### 3.2 `nika/CLAUDE.md` and rules: Issues Found

1. **`routing:` documented as planned feature** -- it is parsed (nika-core) with `#[allow(dead_code)]` but never used at runtime. Not mentioned as "planned" or "not yet implemented" anywhere in user-facing docs.

2. **`manifest: true` documented in artifacts block** -- templates generate it, docs describe it, but it literally does nothing. Users who set `manifest: true` get zero feedback that nothing happened.

3. **`token_budget` documented as agent parameter** -- LSP snippets include it, course exercises reference it, `effective_token_budget()` exists, but it is NEVER enforced at runtime. Users setting `token_budget: 50000` are deceived.

4. **Error code table says NIKA-010 = "Schema validation error"** -- but there's no explicit NIKA-010 in the error enum. The actual schema errors use NIKA-160-164 (parse errors).

5. **`type: llm` guardrails documented as a guardrail type** -- but the async judge is not implemented. Setting `type: llm` will cause `FailedImmediate` with a message saying "not yet implemented". This is not documented.

6. **`response: binary` + `extract:` combination** -- docs suggest they're independent, but runtime behavior when both are set is undefined/untested.

### 3.3 Nika Rules (repo root `.claude/rules/nika.md`)

The nika rule file documents the workflow syntax correctly. However:

1. **`for_each` output is documented as JSON array** but the `for_each.index` template variable (documented as "unavailable" in the bug list) suggests users expect `{{for_each.index}}` to work. No documentation says this is not available.

2. **Pipe transforms**: The rule says 31 transforms available. The `compact` transform is documented but does NOT filter empty strings (L2 bug). The `join()` transform cannot have `|` in its parameter (L1 bug).

3. **Mock provider**: Documented as returning "deterministic test responses" but the actual mock implementation's behavior is not specified anywhere.

---

## 4. SESSION PLAN GAP ANALYSIS

### Session A: Security Hardening -- GAPS

| Plan Item | Covered? | Gap |
|-----------|----------|-----|
| S1+S2: Block shell -c variants | YES | -- |
| SF1: DNS fail-closed | YES | -- |
| S5+S6: Template injection | YES | -- |
| S3+S4: SSRF redirect DNS | YES (investigate) | -- |
| SF5: Schema .ok() silent disable | NO | Not listed in Session A despite being security-relevant |
| M-sec1: xargs/find -exec | NO | Listed in A as Bug 5, but also missing from actual plan text |
| M-sec4: redact API keys | YES (Bug 8) | -- |
| `unsafe` env var manipulation | NO | 30+ unsafe blocks not mentioned in any session |

**Missing from Session A**: SF5 is a security issue (invalid schema silently disables validation)
but is only mentioned in Session C. It should be in A because it's a validation bypass.

### Session B: Agent Refactor -- GAPS

| Plan Item | Covered? | Gap |
|-----------|----------|-----|
| AD1: Extract `run_agent_loop<C>` | YES | -- |
| SF9: Wire token_budget | YES | -- |
| SF10: Extended thinking integration | YES | -- |
| AD2+AD3: Agent execution tests | YES | -- |
| Streaming module timeout (all 3 paths) | NO | Session B only covers providers.rs. The streaming module (`streaming.rs`) has 3 separate streaming functions with duplicated timeout logic that should be unified. |
| chat.rs missing ProviderResponded (EV2) | NO | Chat is a separate agent path not covered. |

### Session C: Silent Failures -- GAPS

| Plan Item | Covered? | Gap |
|-----------|----------|-----|
| TaskEventGuard | YES | -- |
| SF2-SF4: Missing events | YES | -- |
| EV2: Chat ProviderResponded | YES | -- |
| EV5: MCP disconnect events | YES | -- |
| EV1: ContextAssembled hardcoded | YES | -- |
| SF6-SF8: Log levels | YES | -- |
| CR1: SchemaGuardrail | YES | -- |
| SF5: Schema .ok() | NO | Not in Session C's actual task list, only master plan |
| 28 untested event variants | NO | Session C Part 2 only fixes 5 missing events. The telemetry audit found 28 untested variants. Where are the other 23? |
| EV6-EV8: Estimated tokens in events | Part 3 covers these | -- |

**Critical gap**: Of the 30 untested EventKind variants listed in the master plan Part 6,
Session C only fixes 5 missing events (SF2-SF4 + EV2 + EV5). The remaining 25 untested
variants have NO session assignment.

### Session D: Quality Infrastructure -- GAPS

| Plan Item | Covered? | Gap |
|-----------|----------|-----|
| cargo-mutants on 5 files | YES | -- |
| proptest strategies | YES | -- |
| tracing-error | YES | -- |
| serial_test for env vars | YES | -- |
| RC6: workspace deps | YES | -- |
| RC4: merge pricing tables | YES | -- |
| unsafe code audit | NO | Not mentioned despite 30+ unsafe blocks |
| `_ => {}` sweep | NO | 60 instances, Rule 2 violation, no session covers this |
| `unwrap_or(0)` sweep | NO | 50+ instances, Rule 1 violation, no session covers this |

**Missing from Session D**: The master plan rules 1-2 are violated 110+ times.
No session is assigned to fix them. `cargo-mutants` will catch SOME of these,
but a manual sweep is needed for the `_ => {}` patterns that silently ignore
new enum variants.

### Session E: Test Hardening -- GAPS

| Plan Item | Covered? | Gap |
|-----------|----------|-----|
| CR2+CR3: Tautological tests | YES | -- |
| CR1: SchemaGuardrail | YES | -- |
| Top 50 `assert!(is_ok())` | YES | -- |
| Agent test coverage | YES | -- |
| Event emission tests | YES | -- |
| M-orig6: for_each.index | YES (Part 6) | -- |
| M-orig3: manifest | YES (Part 6) | -- |
| M-orig8: Temperature per-provider | YES (Part 6) | -- |
| 232 bare is_ok() assertions | Only top 50 | 182 remain unaddressed |

### Session F: Stringly-Typed -- GAPS

| Plan Item | Covered? | Gap |
|-----------|----------|-----|
| ExtractMode enum | YES | -- |
| ProviderName enum | YES | -- |
| EventKind grouping (RC7) | YES | -- |
| Doc comment fix | YES | -- |
| L2, L3, L5, L9 | YES | -- |
| L1: join() pipe escape | NO | Not listed in Session F |
| L4: Vision TTFT null | NO | Not listed in any session |
| L6: DNS fail-closed | Covered in Session A | -- |
| L7: Summary box width | NO | Not in any session |
| L8: Windows CI | NO | Not in any session |
| L10: Error code audit | NO | Not in any session |

---

## 5. ITEMS IN NO SESSION AT ALL

These bugs appear in the master plan or handoff but are NOT assigned to any session:

| ID | Bug | Source |
|----|-----|--------|
| H2 | `infer_stream_with_options` native path 0 tokens | Mega handoff Wave 1 |
| H3 | Streaming without `Final` event = 0/0/0 tokens | Mega handoff Wave 1 |
| H4 | Thinking tokens priced at output rate | Mega handoff Wave 1 + Honest handoff B1 |
| H5 | `haiku-3.5` pattern mismatch | Mega handoff Wave 1 |
| H8 | Custom endpoints not wired for CLI | Mega handoff Wave 2 |
| H9 | MCP reconnect storm | Mega handoff Wave 2 |
| H11 | `nika new` invalid workflow name | Mega handoff Wave 3 |
| H12 | Help text references nonexistent subcommand | Mega handoff Wave 3 |
| M2 | for_each ordering with concurrency: 1 | Honest handoff B5 |
| M6 | Duplicate "scheduled" display lines | Honest handoff B3 |
| M7 | fetch: short form rejected by schema | Honest handoff B8 |
| M8 | infer_with_tools (Layer 0b) no timeout | Mega handoff Wave 4 |
| M9 | format: markdown rejected by schema | Honest handoff B9 |
| M10 | MCP task-level timeout not enforced | Mega handoff Wave 4 |
| M12 | extract: llm_txt returns fallback JSON | Honest handoff B11 |
| M13 | extract: text includes CSS | Mega handoff Wave 4 |
| M14 | provider_initialized logs wrong model | Honest handoff B4 |
| M15-M17 | LSP hover issues | Mega handoff Wave 4 |
| M18 | Schema guardrail required-only | Also in Session E Part 2 |
| M19 | Unknown CAS framing flag corruption | Mega handoff Wave 4 |
| M20 | Cache discount fallback 0.1 | Mega handoff Wave 4 |
| L1 | join() pipe escape | Mega handoff Wave 5 |
| L4 | Vision TTFT null | Mega handoff Wave 5 |
| L7 | Summary box width | Mega handoff Wave 5 |
| L8 | Windows CI | Mega handoff Wave 5 |
| L10 | Error code audit | Mega handoff Wave 5 |

**That is 25 bugs from the original lists with NO session assignment.**

The session plans (A-F) total roughly 40-50 tasks. The master plan + mega handoff + honest
handoff list 100+ issues. The gap is ~50 items that either fell through the cracks or were
implicitly deferred without being called out.

---

## 6. E2E MEGA-WORKFLOW

The following workflow exercises all 5 verbs, for_each, context, artifacts, structured output,
multiple providers, guardrails, and error handling. Save as `e2e-mega-test.nika.yaml`.

```yaml
schema: "nika/workflow@0.12"
workflow: e2e-mega-test
description: "Exercises all 5 verbs, for_each, context, artifacts, structured output, guardrails"
provider: mock
model: mock-model

inputs:
  topic: "Workflow engine testing"
  max_items: 3

context:
  files:
    readme: ./README.md

artifacts:
  dir: ./output/e2e-test
  format: text
  mode: overwrite

tasks:
  # ═══════════════════════════════════════════
  # VERB 1: exec — gather system info
  # ═══════════════════════════════════════════
  - id: system_info
    exec:
      command: "echo '{\"os\": \"test\", \"arch\": \"x86_64\", \"timestamp\": \"2026-03-28\"}'"
      shell: true
      timeout: 10

  # ═══════════════════════════════════════════
  # VERB 2: fetch — HTTP request with extract
  # ═══════════════════════════════════════════
  - id: fetch_data
    fetch:
      url: "https://httpbin.org/json"
      method: GET
      extract: jsonpath
      selector: "$.slideshow.title"
      timeout: 15

  # ═══════════════════════════════════════════
  # VERB 3: infer — LLM with context files
  # ═══════════════════════════════════════════
  - id: analyze_context
    depends_on: [system_info]
    with:
      sys: $system_info
    infer:
      prompt: |
        Given this system context: {{with.sys}}
        And this readme excerpt: {{context.readme | default("no readme") | trim}}
        Topic: {{inputs.topic}}
        Generate a JSON array of exactly {{inputs.max_items}} test scenarios.
      temperature: 0.3
      max_tokens: 500

  # ═══════════════════════════════════════════
  # VERB 3b: infer with structured output
  # ═══════════════════════════════════════════
  - id: structured_extract
    depends_on: [analyze_context]
    with:
      raw: $analyze_context
    infer:
      prompt: |
        Extract structured data from: {{with.raw}}
        Return a JSON object with fields: title, items (array), score (number).
    structured:
      schema:
        type: object
        properties:
          title:
            type: string
          items:
            type: array
            items:
              type: string
          score:
            type: number
            minimum: 0
            maximum: 100
        required: [title, items, score]
      enable_repair: true
      max_retries: 2
    artifact:
      path: structured-output.json
      format: json

  # ═══════════════════════════════════════════
  # for_each with concurrency
  # ═══════════════════════════════════════════
  - id: process_items
    depends_on: [structured_extract]
    with:
      data: $structured_extract
    for_each:
      items: "{{with.data.items | default(\"[\\\"item1\\\", \\\"item2\\\", \\\"item3\\\"]\") | parse_json}}"
      as: item
      concurrency: 2
      fail_fast: false
    infer: "Process item: {{with.item}} — provide a one-sentence analysis."

  # ═══════════════════════════════════════════
  # VERB 4: invoke — builtin tool
  # ═══════════════════════════════════════════
  - id: measure_output
    depends_on: [process_items]
    with:
      results: $process_items
    exec:
      command: "echo '{{with.results | length}} items processed successfully'"
      shell: true

  # ═══════════════════════════════════════════
  # VERB 5: agent — multi-turn with guardrails
  # ═══════════════════════════════════════════
  - id: review_agent
    depends_on: [structured_extract, measure_output]
    with:
      structured: $structured_extract
      count: $measure_output
    agent:
      system: "You are a quality reviewer. Be concise."
      prompt: |
        Review this structured output: {{with.structured}}
        Items processed: {{with.count}}
        Provide a quality assessment with exactly 3 bullet points.
      max_turns: 3
      temperature: 0.2
      completion:
        mode: natural
      guardrails:
        - type: length
          min_words: 10
          max_words: 500
          on_failure: retry
        - type: regex
          pattern: "^[\\s\\S]*\\*[\\s\\S]*$"
          message: "Response must contain bullet points"
          on_failure: retry

  # ═══════════════════════════════════════════
  # Error handling: on_error continue
  # ═══════════════════════════════════════════
  - id: risky_fetch
    on_error: continue
    fetch:
      url: "https://this-domain-does-not-exist-nika-test.invalid/api"
      timeout: 5

  # ═══════════════════════════════════════════
  # Final synthesis
  # ═══════════════════════════════════════════
  - id: final_report
    depends_on: [review_agent, fetch_data, risky_fetch]
    with:
      review: $review_agent
      api_data: $fetch_data
      risky: $risky_fetch
    infer:
      prompt: |
        Create a final report combining:
        - Review: {{with.review | default("review not available")}}
        - API data: {{with.api_data | default("api data not available") | trim}}
        - Risky fetch: {{with.risky | default("fetch failed as expected")}}

        Include a summary section.
      max_tokens: 800
    artifact:
      path: final-report.md
      format: markdown
```

### What This Exercises

| Feature | Task(s) |
|---------|---------|
| `exec:` verb | `system_info`, `measure_output` |
| `fetch:` verb | `fetch_data`, `risky_fetch` |
| `infer:` verb | `analyze_context`, `structured_extract`, `process_items`, `final_report` |
| `invoke:` verb | Would need MCP server running; use `measure_output` as proxy |
| `agent:` verb | `review_agent` |
| `for_each` + concurrency | `process_items` (concurrency: 2) |
| Context files | `analyze_context` uses `{{context.readme}}` |
| Artifacts | `structured_extract` and `final_report` write artifacts |
| Structured output | `structured_extract` with full schema + repair |
| Multiple providers | Workflow uses `mock`; tasks could override |
| Guardrails | `review_agent` has length + regex guardrails |
| Error handling | `risky_fetch` with `on_error: continue` |
| Pipe transforms | `default()`, `trim`, `length`, `parse_json` |
| `with:` bindings | Every task from `analyze_context` onward |
| `depends_on` DAG | Diamond dependency pattern |
| `inputs:` params | `topic`, `max_items` |
| `$` task references | Throughout |
| Default fallback `??` | Via `default()` transform |

### Known Limitations

1. `invoke:` not directly tested (requires MCP server). Could add `invoke: "nika:dimensions"` with a test image if available.
2. `provider: mock` means no real LLM calls. For real E2E: change to `provider: anthropic`.
3. `for_each.index` in artifact paths not tested (feature not implemented).
4. `manifest: true` not tested (feature not implemented).
5. Vision/multimodal not tested (requires image file + cloud provider).

---

## 7. PRIORITY RECOMMENDATIONS

### Immediate (Before Any Session)

1. **SF1**: Change DNS failure from `false` to `true` in `policy.rs:106-112`. 2-line fix, critical security.
2. **SF5**: Change `.ok()` to `.map_err(|e| NikaError::ValidationError { ... })` in `runner.rs:656`. 3-line fix.
3. **S1+S2**: Add `"bash -c"`, `"zsh -c"`, `"sh -c"`, `"python3 -c"` to BLOCKLIST. 5-line fix.

### Session A Addendum

- Add SF5 (schema .ok())
- Add M-sec1 (xargs, find -exec)
- Add unsafe env var audit (at minimum document the risk)

### Session B Addendum

- Add chat.rs ProviderResponded (EV2)
- Add streaming module dedup (3 identical timeout patterns)

### Session C Addendum

- Assign 23 remaining untested event variants to test tasks
- Add structured_output.rs + thinking.rs + chat.rs cost migration to `_with_cache`

### New Session G: Sweep Tasks (Not in Any Session)

Create a session for the 25 unassigned bugs listed in section 5, plus:
- `_ => {}` sweep (60 instances)
- `unwrap_or(0)` audit (50+ instances)
- `unsafe` code audit (30+ instances)
- `#[allow(dead_code)]` cleanup (42 instances)

### Documentation Fixes (Zero Code)

1. `log.rs:5`: Change "44 variants" to "58 variants"
2. Document `token_budget` as "parsed but not yet enforced"
3. Document `manifest: true` as "parsed but not yet implemented"
4. Document `routing:` as "reserved, not yet implemented"
5. Document `type: llm` guardrails as "not yet implemented, will error"
6. Error code table: verify NIKA-010 actually exists in code

---

*This review was conducted by reading all plan documents and then verifying every claim
against the actual codebase with grep/read. No claims were taken at face value.*
