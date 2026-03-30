# Nika v0.53 — Mega Handoff v2 (10-Agent Enriched)

> Handoff from Phase 1-2 session (27 commits). Enriched by 10 parallel audit agents:
> rust-pro (×3), rust-security, rust-async-expert, rust-perf, rust-architect,
> code-reviewer, Explore (×2). **82 findings total, 9 CRITICAL, 18 HIGH.**

---

## STATE AT HANDOFF (2026-03-31)

```
Version:     v0.52.0 + 27 commits (untagged)
Tests:       9,009 lib (0 fail) + 47 E2E (41 pass, 6 fail)
Clippy:      0 warnings (--all-targets --all-features)
LOC:         356K Rust, 12 crates
Branch:      main
Directory:   /Users/thibaut/dev/supernovae/nika/tools
Last commit: cf3e5cef9 docs: mega handoff Phase 3-4
```

### Provider Status
| Provider | Status | E2E Validated? |
|----------|--------|----------------|
| OpenAI | **OK** | YES (gpt-4.1-nano structured) |
| xAI | **OK** | YES (grok-3-fast structured) |
| Gemini | RATE LIMITED | NO |
| Anthropic | NO CREDITS | NO |
| Groq | OK (daemon) | Not in E2E |
| Mistral | OK (daemon) | Not in E2E |
| DeepSeek | OK (daemon) | Not in E2E |
| Mock | **OK** | YES (12 tests pass) |

---

## CRITICAL BUGS (9) — Fix before v0.53.0

### CRIT-1: FallbackChainExhausted mapped to wrong error variant
**File**: `error_domains.rs:80-88`
**Agent**: error-handling
**What**: `ProviderError::FallbackChainExhausted` mapped to `NikaError::ProviderApiError` (NIKA-031) instead of `NikaError::FallbackChainExhausted` (NIKA-037). Wrong code, wrong retry behavior (retried when it shouldn't be), wrong fix suggestion, `providers` field lost.
**Fix**: Map to `NikaError::FallbackChainExhausted { providers, last_error }`.

### CRIT-2: FinishReason::Stop hardcoded in 6 places
**File**: `executor/infer.rs:727,896,963,1100,1169,1462`
**Agent**: telemetry
**What**: `StreamResult` doesn't carry `finish_reason` from LLM. All `ProviderResponded` events report `Stop` regardless of actual completion reason (max_tokens, tool_use, content_filter). Telemetry data is misleading.
**Fix**: Add `finish_reason: Option<FinishReason>` to `StreamResult`, capture from provider response, propagate through.

### CRIT-3: 3 event variants never emitted in production
**File**: `nika-event/src/log.rs:313,931,945`
**Agent**: telemetry
**What**: `McpConnected`, `OrchestratorSubWorkflow`, `OrchestratorFailed` are defined but zero `emit()` calls in production code. Dead telemetry.
**Fix**: Emit at appropriate points or remove variants.

### CRIT-4: OrchestratorRound events never emitted → rounds always 0
**File**: `runner.rs:2662`
**Agent**: explore + telemetry
**What**: `OrchestratorCompleted.rounds` counts `OrchestratorRound` events, but none are ever emitted outside tests. Orchestrator metrics incomplete.
**Fix**: Emit `OrchestratorRound` in agent loop when task_id == "__orchestrator__".

### CRIT-5: confidence_target parsed but never enforced
**File**: `nika-core/src/ast/orchestrate.rs:17`
**Agent**: explore
**What**: `confidence_target: 0.85` deserialized but execution hardcodes `confidence: 1.0`. Orchestrator never retries below-confidence completions.
**Fix**: In RigAgentLoop, compare nika:complete confidence against config target.

### CRIT-6: Security tests assert nothing (4 tests)
**File**: `tests_security.rs:211,300`, `tests_paranoid.rs:225,105`
**Agent**: test-quality
**What**: `svg_attack_entity_expansion_billion_laughs`, `svg_attack_css_import_external`, duplicate billion laughs, `params_thumbnail_width_as_float` — all discard results with `let _ = result`. Security vulnerabilities would not be caught.
**Fix**: Assert `result.is_err()` or explicit behavior check.

### CRIT-7: e2e_adversarial_structured_additional_properties_false broken
**File**: `e2e_workflow_test.rs:872`
**Agent**: pre-existing
**What**: Mock now generates schema-conforming JSON, so `additionalProperties: false` passes instead of failing. Test expects failure.
**Fix**: Update test to validate that mock + additionalProperties: false passes correctly.

### CRIT-8: exec/fetch verbs ignore CancellationToken
**File**: `executor/exec.rs:140`, `executor/fetch.rs` (entire file)
**Agent**: async-expert
**What**: exec and fetch don't race against `cancel_token.cancelled()`. Cancelled workflows continue running exec/fetch until their own timeout. Invoke and vision correctly use `tokio::select!`.
**Fix**: Add `tokio::select!` with `cancel_token.cancelled()` in both verbs.

### CRIT-9: Agent verb creates disconnected CancellationToken
**File**: `rig_agent_loop/mod.rs:256`
**Agent**: async-expert
**What**: `RigAgentLoop::new()` creates a fresh `CancellationToken::new()` not derived from the executor's token. Workflow cancellation only works through JoinSet abort (not cooperative). MCP servers spawned by agent may not shut down.
**Fix**: Pass `executor.cancel_token.child_token()` into RigAgentLoop.

---

## HIGH BUGS (18)

### HIGH-1: Secret leakage in traces
**File**: `nika-event/src/trace.rs:72`, `log.rs:197`
**Agent**: security
**What**: Trace NDJSON contains full `TaskCompleted.output` and `TaskStarted.inputs` — LLM responses, API results, potentially PII. Redaction only applied to `TemplateResolved`.
**Fix**: Apply `redact_for_event()` to all string fields in trace serialization, or add `--redact-traces` flag.

### HIGH-2: $env unrestricted access to all environment variables
**File**: `binding/resolve.rs:697-710`
**Agent**: security
**What**: `$env.AWS_SECRET_ACCESS_KEY` readable by workflow YAML and sent to LLM prompts. Only a `debug!` log for secret-pattern vars.
**Fix**: Upgrade to `warn!` event. Consider `env_allowlist` option.

### HIGH-3: exec backtick bypass in resolved LLM data
**File**: `executor/exec.rs:38`
**Agent**: security
**What**: LLM output containing `` `whoami` `` in `with:` values gets executed by `sh -c` when template uses `{{with.name}}` without `|shell`. By design but undocumented risk.
**Fix**: AST-phase warning when `shell: true` + `{{with.*}}` without `|shell`.

### HIGH-4: MOCK_CALL_COUNTER never resets between tests
**File**: `executor/infer.rs:286`
**Agent**: security + rust-pro
**What**: Process-global static AtomicU32 shared across concurrent tests. Counter persists, causing flaky test behavior.
**Fix**: Reset counter when env var is read, or scope per-Runner.

### HIGH-5: ProviderApiError always retried (401/403 should be permanent)
**File**: `runner.rs:868`
**Agent**: error-handling
**What**: All `ProviderApiError` classified as retryable. Auth errors (401), invalid model (404) waste retry budget.
**Fix**: Add status code field, only retry 429/5xx.

### HIGH-6: ExecError always retried (command not found is permanent)
**File**: `runner.rs:869`
**Agent**: error-handling
**What**: Permanent exec failures (No such file, bad cwd) classified as retryable.
**Fix**: Split into ExecError (permanent) and ExecTimeout (retryable).

### HIGH-7: WorkflowTimeout missing [NIKA-038] prefix
**File**: `error.rs:248`
**Agent**: logic
**What**: Error message `"Workflow timed out after..."` missing `[NIKA-038]` unlike all other variants.
**Fix**: Add `[NIKA-038]` to `#[error()]` attribute.

### HIGH-8: ProviderCalled event emits alias not canonical name
**File**: `executor/infer.rs:432`
**Agent**: logic
**What**: `provider_name` is raw YAML string ("claude"), not canonical ("anthropic"). ProviderInitialized uses canonical. Inconsistent telemetry.
**Fix**: Normalize via `find_provider(name).map(|p| p.id)`.

### HIGH-9: RigProvider::name() returns "claude" (alias) not "anthropic"
**File**: `provider/rig/mod.rs:403`
**Agent**: logic
**What**: Inconsistent with all other variants that return canonical names. Affects verify output.
**Fix**: Return "anthropic" for Claude variant.

### HIGH-10: retry: silently ignored with old output+structured combo
**File**: `runner.rs:1034-1056`
**Agent**: logic
**What**: When task has both old `output: { schema }` and `retry:`, the old path takes priority and task-level retry is silently dropped.
**Fix**: Emit warning or apply retry as outer loop.

### HIGH-11: max_duration_secs discarded in lower.rs
**File**: `ast/lower.rs:61,692`
**Agent**: explore
**What**: User's timeout lost on round-trip through lower/unlower. Hardcoded to 3600.
**Fix**: Propagate through lowered Workflow struct.

### HIGH-12: Post-redirect SSRF not pinned
**File**: `executor/fetch.rs:~365`
**Agent**: security
**What**: Post-redirect check uses old boolean `resolve_and_check_ssrf`, not pinned version.
**Fix**: Use `resolve_and_pin_ssrf` or accept as defense-in-depth.

### HIGH-13: Vision image URL SSRF not pinned
**File**: `executor/infer.rs:~1363`
**Agent**: security
**What**: Vision `image_url` path uses `resolve_and_check_ssrf`, same TOCTOU gap.
**Fix**: Wire resolve_and_pin_ssrf for vision URLs.

### HIGH-14: 4 unreachable!() calls depend on distant early returns
**File**: `provider/rig/mod.rs:640,642,756,758`
**Agent**: error-handling
**What**: `unreachable!("DeepSeek/Native handled above")` — 40+ lines from the guard. Fragile to refactoring.
**Fix**: Replace with proper error returns.

### HIGH-15: Global string interner never evicts (daemon memory leak)
**File**: `util/interner.rs:16`
**Agent**: async-expert
**What**: Every unique task ID interned forever. For_each generates N permanent entries. Slow leak in daemon mode.
**Fix**: Scoped interner per-workflow or eviction on completion.

### HIGH-16: God files need splitting
**Agent**: architect
**What**: runner.rs (7174 lines), template.rs (4386), parser.rs (3840), analyze.rs (3464), error.rs (2773, 102 variants)
**Fix**: Split runner into runner/mod.rs + for_each.rs + lifecycle.rs + trace.rs.

### HIGH-17: 81 AnalyzedTask construction sites (shotgun surgery)
**Agent**: architect
**What**: Every new field on AnalyzedTask requires touching 81 sites. No Default impl on AnalyzedTask.
**Fix**: Add `Default` impl + `AnalyzedTask::test(id, name, action)` builder.

### HIGH-18: Doc comment corruption on generic_mock_json
**File**: `executor/infer.rs:~1470`
**Agent**: rust-pro
**What**: generic_mock_json has check_infer_guardrails' doc comment.
**Fix**: Separate doc comments.

---

## MEDIUM FINDINGS (28 — selected highlights)

| # | Finding | Agent | File |
|---|---------|-------|------|
| M-1 | generate_mock_json no allOf/anyOf/oneOf/$ref | explore | mock_json.rs |
| M-2 | AgentParams.scope parsed but not implemented | explore | rig_agent_loop/mod.rs:285 |
| M-3 | Agent tool poisoning — MCP results unsanitized in LLM context | security | tool.rs:166 |
| M-4 | NaN propagation in f64 cost accumulators | security | cost.rs:108 |
| M-5 | JSONPath errors silently return None | error | run_context.rs:459 |
| M-6 | resolve_alias_path clones Value in hot path | perf | template.rs:284,325 |
| M-7 | bindings.to_value() full deep copy on every TaskStarted | perf | runner.rs:963 |
| M-8 | for_each aggregation deep-clones all outputs | perf | runner.rs:2574 |
| M-9 | redact_for_event regex runs on every prompt (no fast-path) | perf | verbs.rs:117 |
| M-10 | is_in_json_context O(N*M) scan | perf | template.rs:1651 |
| M-11 | SkillInjector benign double-load on concurrent access | async | skill_injector.rs:84 |
| M-12 | JoinSet abort may leak MCP server processes | async | runner.rs:2429 |
| M-13 | 9-arg functions in artifact_processor (feature envy) | architect | artifact_processor.rs |
| M-14 | DAG operates on strings not TaskId | architect | dag/flow.rs |
| M-15 | 38 error types across workspace (proliferation) | architect | error.rs |
| M-16 | 40+ ad-hoc std::env::var reads (config sprawl) | architect | scattered |
| M-17 | Silent test: e2e_infer_mock_temperature only checks !is_empty | test | tests_e2e_workflow.rs:443 |
| M-18 | 6× assert!(!data.is_empty()) in media chart tests | test | tests_pr3b_tools.rs |

---

## LOW FINDINGS (27) — deferred

Includes: base64 size accuracy, f64 infinity acceptable, compile-only tests, behavioral TUI change (claude→anthropic), stale wrapper function, mock array clamp edge case, Cow<str> opportunities, etc.

---

## PERFORMANCE QUICK WINS (from rust-perf agent)

| Priority | Fix | Impact | Complexity |
|----------|-----|--------|------------|
| **P1** | Gate `bindings.to_value()` behind trace subscriber check | Skip largest allocation per task | LOW |
| **P1** | Avoid `base.clone()` in `resolve_alias_path` — use `Cow<Value>` | Every `{{with.x}}` reference | MEDIUM |
| **P1** | Use `Arc::clone(&r.output)` in for_each aggregation | N deep clones → N Arc clones | LOW |
| **P2** | Fast pre-check in `redact_for_event()` (skip regex when no secret prefixes) | Every task execution | LOW |
| **P2** | Pre-Arc `AnalyzedTask` at workflow construction | Skip clone per task spawn | MEDIUM |
| **P3** | Cache `TransformExpr::parse()` per template | Repeated for_each transforms | HIGH |

---

## SECURITY MODEL DOCUMENTATION NEEDED

1. **Trace files** contain full task I/O — never commit `.nika/traces/`
2. **$env** provides unrestricted access to ALL env vars from workflow YAML
3. **MCP tool results** injected unsanitized into LLM conversation history
4. **exec with shell:true** — LLM output in `{{with.*}}` can contain backticks (use `|shell`)
5. **YAML bomb** protection: comprehensive Budget system (✓ already safe)
6. **Template injection**: 3-pass isolation prevents recursive expansion (✓ safe)
7. **CAS BLAKE3**: 256-bit hash, atomic writes, integrity verification (✓ safe)
8. **Symlink attacks**: double canonicalize + fail-closed on artifacts (✓ safe)
9. **Fetch streaming size**: enforced DURING streaming, not after (✓ safe)

---

## SESSION HANDOFF PROMPTS

### Session 3A/4A Combined: Bug Fixes + Performance (recommended)

```
Tu es l'orchestrateur Nika. Session combinée: Bug Fixes + Performance.

ETAT: v0.52.0+27 commits, 9009 tests, 0 clippy.
DIR: /Users/thibaut/dev/supernovae/nika/tools

HANDOFF: docs/plans/sessions/handoff-phase3-v053.md (v2, 10-agent enriched)

OBJECTIF PRINCIPAL: Fixer les 9 CRITICALs + les 18 HIGHs les plus impactants.

PRIORITE 1 — CRITICALs (9 bugs, tous doivent être fixés):
  CRIT-1: FallbackChainExhausted wrong error mapping (error_domains.rs:83)
  CRIT-2: FinishReason::Stop hardcoded 6 places (infer.rs)
  CRIT-3: 3 event variants never emitted (McpConnected, OrchestratorFailed, OrchestratorSubWorkflow)
  CRIT-4: OrchestratorRound never emitted → rounds always 0
  CRIT-5: confidence_target never enforced (always 1.0)
  CRIT-6: 4 security tests assert nothing (tests_security.rs, tests_paranoid.rs)
  CRIT-7: e2e_adversarial_structured test broken by mock improvement
  CRIT-8: exec/fetch verbs ignore CancellationToken
  CRIT-9: Agent CancellationToken disconnected from workflow

PRIORITE 2 — HIGHs les plus impactants:
  HIGH-1: Secret leakage in traces
  HIGH-5: ProviderApiError always retried (401 should be permanent)
  HIGH-6: ExecError always retried
  HIGH-7: WorkflowTimeout missing [NIKA-038]
  HIGH-8: ProviderCalled emits alias not canonical
  HIGH-9: RigProvider::name() returns "claude" not "anthropic"
  HIGH-14: 4 unreachable!() depend on distant early returns

PRIORITE 3 — Performance quick wins:
  P1: Gate bindings.to_value() behind subscriber check
  P1: Cow<Value> in resolve_alias_path
  P1: Arc::clone in for_each aggregation

REGLES: TDD, 1 fix = 1 commit, cargo test + clippy avant chaque commit.
Push toutes les 3-4 commits.

GO: Verification initiale puis fix CRITICALs d'abord.
```

### Session 4B: Final Validation + Release v0.53.0

```
Tu es l'orchestrateur Nika. Session 4B: Final Validation + Release v0.53.0.

ETAT: Post bug-fix session (verifier git log).
DIR: /Users/thibaut/dev/supernovae/nika/tools

HANDOFF: docs/plans/sessions/handoff-phase3-v053.md

OBJECTIF: Version bump, CHANGELOG, final E2E, mass validation, tag + push.

PRE-REQUIS: Tous les CRITICALs doivent être fixés (verifier dans git log).

RELEASE GATE:
  [ ] 0 CRITICAL bugs remaining
  [ ] 9,300+ lib tests, 0 failures
  [ ] 50+ E2E tests pass (mock)
  [ ] 0 clippy warnings
  [ ] cargo deny check passes
  [ ] CHANGELOG complete (27+ commits since v0.52.0)
  [ ] Version bumped to 0.53.0
  [ ] Tag v0.53.0 pushed

GO: Verification initiale puis version bump.
```

---

## METRICS

| Metric | v0.52.0 | Now | Target v0.53.0 |
|--------|---------|-----|----------------|
| Commits | 0 | 27 | 45+ |
| Lib tests | 8,970 | 9,009 | 9,300+ |
| E2E tests | 40 | 47 | 55+ |
| Hardcoded models | 20+ | 0 | 0 |
| CRITICALs | 12 | 9 | **0** |
| HIGHs | - | 18 | <5 |
| Clippy | 0 | 0 | 0 |
| OOM protection | none | 50MB | 50MB |
| DNS TOCTOU | open | pinned | pinned |

---

## AUDIT AGENT RESULTS SUMMARY

| Agent | Focus | CRITs | HIGHs | MEDs |
|-------|-------|-------|-------|------|
| test-quality (rust-pro) | Silent tests | 4 | 9 | 11 |
| security (rust-security) | Attack surfaces | 0 | 1 | 4 |
| telemetry (Explore) | Events | 2 | 0 | 0 |
| async-expert | Race conditions | 2 | 1 | 1 |
| error-handling (rust-pro) | Error quality | 1 | 3 | 3 |
| logic (code-reviewer) | Inconsistencies | 0 | 3 | 0 |
| perf (rust-perf) | Bottlenecks | 0 | 0 | 6 |
| architect | Structure | 0 | 2 | 5 |
| unfinished (Explore) | TODOs/stubs | 0 | 0 | 3 |
| dead-code (rust-pro) | Unused code | pending | - | - |
| **TOTAL** | | **9** | **18+** | **28+** |
