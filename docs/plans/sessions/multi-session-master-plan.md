# Nika v0.53+ Multi-Session Master Plan

> 4 phases, 8 sessions, ~32h total. Each session is one `claude` invocation.
> Each phase has a verification checkpoint that must pass before proceeding.

---

## STATE AT START

```
Version:    v0.52.0 + 8 commits (untagged)
Tests:      8,970 lib + 40 E2E = 9,010 total
Clippy:     0 warnings (--all-targets --all-features)
Providers:  OpenAI OK, xAI OK, Gemini RATE LIMITED, Anthropic NO CREDITS
            Groq OK (daemon), Mistral OK (daemon), DeepSeek OK (daemon)
Codebase:   353k LOC Rust, 12 crates, 502 example workflows
Directory:  /Users/thibaut/dev/supernovae/nika/tools
```

## KNOWN BUGS (12 from 18-agent audit)

```
B1  [HIGH]   20+ hardcoded model strings (ModelResolver not wired in executor)
B2  [HIGH]   confidence_target ignored in P-ORCHESTRATE
B3  [MEDIUM] Orchestrator events never emitted
B4  [HIGH]   OOM on >100MB task output (no size limit)
B5  [HIGH]   No global workflow timeout
B6  [MEDIUM] Token overflow u64 in agent loop
B7  [MEDIUM] No MCP reconnection on server crash
B8  [LOW]    concurrency:0 silently becomes 1
B9  [LOW]    HTTPS→HTTP redirect downgrade allowed
B10 [DOC]    Nested for_each: no auto-flatten (limitation)
B11 [LOW]    stop_sequences provider name not normalized
B12 [MEDIUM] Chat overlay 5 hardcoded models
```

## SECURITY CONFIRMED (no fix needed)

```
Template injection 3-pass   SAFE    15+ tests
$env in LLM output          SAFE    Not template syntax
SSRF (all vectors)           SAFE    Defense-in-depth
CRLF headers                 SAFE    Rejected
Path traversal               SAFE    sanitize_for_path + canonicalize
CSS ReDoS                    SAFE    Linear-time parser
CAS BLAKE3                   SAFE    256-bit
Cost tracking                CORRECT All 7 providers + cached discounts
Fetch size limits            SAFE    50MB/100MB streaming check
```

---

# PHASE 1: "STOP BREAKING" (2 sessions, ~8h)

> Goal: Fix all HIGH bugs, wire ModelResolver everywhere, add safety limits.
> After this phase: zero hardcoded models, zero panics, zero OOM risk.

## Session 1A: ModelResolver Complete Wire (4h)

```
VERIFICATION INITIALE:
  cd /Users/thibaut/dev/supernovae/nika/tools
  git log --oneline -5
  cargo test --workspace --lib 2>&1 | tail -5
  cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3

TASKS (9 commits):
  1. Wire ModelResolver in executor/infer.rs (7 sites)
     - Replace all model.unwrap_or_else(|| provider.default_model())
     - Single ModelResolver::resolve() call at function top
     - Use resolved.model_id everywhere (cost, events, structured)
     TDD: test that ProviderCalled event reports correct model name
     COMMIT: refactor(infer): wire ModelResolver — 7 model fallback sites eliminated

  2. Wire ModelResolver in executor/agent.rs
     - Replace resolved_model.or_else(|| self.default_model...)
     TDD: test agent uses workflow default model when task model is None
     COMMIT: refactor(agent): wire ModelResolver for agent model resolution

  3. Wire ModelResolver in executor_compressor.rs (lines 76-78)
     - Replace hardcoded "claude-haiku-4-5", "gpt-4.1-mini"
     TDD: test compressor uses provider default from catalog
     COMMIT: refactor(compressor): wire ModelResolver — no hardcoded model names

  4. Wire ModelResolver in tui/app/mod.rs (lines 616-620)
     TDD: N/A (display only)
     COMMIT: refactor(tui): app uses default_model_for_provider

  5. Wire ModelResolver in tui/app/lifecycle.rs (lines 66-72)
     COMMIT: refactor(tui): lifecycle uses default_model_for_provider

  6. Wire ModelResolver in tui/state/chat_overlay.rs (lines 84, 86)
     COMMIT: refactor(tui): chat_overlay uses default_model_for_provider

  7. Wire ModelResolver in tui/views/chat/mod.rs (lines 331-367)
     COMMIT: refactor(tui): chat view uses default_model_for_provider

  8. Token overflow fix — saturating_add in providers.rs:364-366
     TDD: test with u64::MAX - 10 + 100 does not panic
     COMMIT: fix(agent): saturating_add for token accumulation

  9. for_each concurrency:0 validation in analyzer
     TDD: test that concurrency: 0 returns AnalyzerError
     COMMIT: fix(analyzer): reject concurrency: 0 with clear error

VERIFICATION POST-SESSION:
  cargo test --workspace --lib     # ALL pass
  cargo clippy --all-targets       # 0 warnings
  grep -rn 'unwrap_or.*default_model\|"gpt-4o"\|"claude-sonnet' \
    nika-engine/src/ nika-tui/src/ --include="*.rs" | \
    grep -v test | grep -v target  # Should return ZERO lines
  git push
```

## Session 1B: Safety Limits + Security Hardening (4h)

```
TASKS (7 commits):
  1. Task output size limit — 50MB in runner.rs
     TDD: test that output > 50MB is truncated with warning
     COMMIT: fix(runtime): 50MB output size limit — prevent OOM

  2. Structured output aggregate timeout — 600s
     TDD: test timeout fires (mock engine that sleeps 2s, set timeout to 1s)
     COMMIT: fix(structured): 600s aggregate timeout on validation engine

  3. MCP tool result size limit — 50MB
     TDD: test result > 50MB returns error
     COMMIT: fix(security): 50MB size limit on MCP tool results

  4. DNS rebinding pin — reqwest .resolve()
     TDD: test fetch with pinned DNS (verify resolved addr used)
     COMMIT: fix(security): pin DNS resolution — prevent TOCTOU rebinding

  5. Workflow global timeout — max_duration_secs header
     - Add field to AnalyzedWorkflow
     - Parse from YAML (optional, default: 3600)
     - Wrap runner.run() in tokio::time::timeout
     TDD: test workflow with max_duration_secs: 1 and slow task → timeout error
     COMMIT: feat(runtime): global workflow timeout via max_duration_secs

  6. Abandoned stream channels — reduce buffer 64→1 or drop
     TDD: N/A (performance, no behavior change)
     COMMIT: perf(streaming): reduce abandoned channel buffer 64→1

  7. ProviderCalled event for L0b tool injection
     TDD: test L0b path emits ProviderCalled before API call
     COMMIT: fix(events): emit ProviderCalled for L0b tool injection path

VERIFICATION POST-SESSION:
  cargo test --workspace --lib
  cargo clippy --all-targets
  # Run real structured output test:
  cargo test --test e2e_workflow_test -- e2e_structured_openai
  git push

PHASE 1 CHECKPOINT:
  [ ] Zero hardcoded model strings in non-test code
  [ ] Zero panics in production code
  [ ] 50MB output limit enforced
  [ ] 600s structured output timeout
  [ ] 50MB MCP result limit
  [ ] DNS pinning active
  [ ] Global workflow timeout
  [ ] saturating_add on all token counters
  [ ] concurrency:0 rejected at parse time
```

---

# PHASE 2: "PROVE IT WORKS" (2 sessions, ~8h)

> Goal: Mock structured output, E2E tests with real providers, P-ORCHESTRATE complete.
> After this phase: every feature tested with real API calls.

## Session 2A: Mock Provider + Core E2E (4h)

```
TASKS (6 commits):
  1. Mock structured output — generate_mock_json() from schema
     - Handle: object, string, number, boolean, array
     - Respect: enum, minimum, minItems, required
     TDD RED:  e2e_mock_structured_output_valid_json (currently fails)
     TDD GREEN: implement generate_mock_json, wire into mock path
     COMMIT: feat(mock): schema-conforming JSON for structured output

  2. Mock failure simulation — NIKA_MOCK_FAIL_COUNT env var
     TDD RED:  e2e_mock_retry_after_failure (currently fails — no mock error)
     TDD GREEN: check env var in mock provider, return error for first N calls
     COMMIT: feat(mock): NIKA_MOCK_FAIL_COUNT for retry testing

  3. E2E: Retry with backoff (uses mock failure)
     TDD: workflow with retry max_attempts:3, NIKA_MOCK_FAIL_COUNT=2 → succeeds
     VERIFY: TaskRetry events emitted with backoff_ms increasing
     COMMIT: test(e2e): retry with exponential backoff via mock failure

  4. E2E: Provider fallback chain (real API)
     TDD: routing fallback: [nonexistent, openai] → succeeds via OpenAI
     VERIFY: FallbackTriggered event emitted
     COMMIT: test(e2e): provider fallback chain with real OpenAI API

  5. E2E: Artifact writing on disk
     TDD: workflow with artifact: {path: report.txt} → file exists after run
     VERIFY: file content matches task output
     COMMIT: test(e2e): artifact writing creates file on disk

  6. E2E: for_each + structured output combo
     TDD: for_each 3 items, each with structured schema → array of valid JSON
     VERIFY: each item in result array has correct fields
     COMMIT: test(e2e): for_each with structured output — array of valid JSON

VERIFICATION:
  cargo test --test e2e_workflow_test              # ALL E2E pass
  cargo test --test e2e_workflow_test -- e2e_mock  # Mock tests pass without API
```

## Session 2B: Real Provider Validation + P-ORCHESTRATE (4h)

```
TASKS (8 commits):
  1. E2E: Vision multimodal (real OpenAI)
     - Create tests/fixtures/test-image.png (1x1 pixel)
     - Workflow: nika:import → infer with content:[{type:image}]
     COMMIT: test(e2e): vision multimodal with real OpenAI

  2. E2E: Agent guardrails (mock)
     - Agent with length guardrail max_words:5
     - Mock returns long text → guardrail triggers
     COMMIT: test(e2e): agent guardrail length violation

  3. E2E: Multi-step pipeline with bindings (real provider)
     - 4-task pipeline: fetch → infer → structured → artifact
     - Real OpenAI call + real URL + real file output
     COMMIT: test(e2e): full pipeline fetch→infer→structured→artifact

  4. E2E: Structured output on ALL available providers
     - Same complex schema (nested object + enum + array + constraints)
     - Test each available provider (skip if no key)
     - Programmatic validation of EVERY field
     COMMIT: test(e2e): structured output complex schema — all providers

  5. P-ORCHESTRATE: Emit OrchestratorStarted event
     TDD: goal workflow → OrchestratorStarted event in log
     COMMIT: feat(orchestrate): emit OrchestratorStarted event

  6. P-ORCHESTRATE: Emit OrchestratorRound events
     TDD: goal workflow → OrchestratorRound events per agent turn
     COMMIT: feat(orchestrate): emit OrchestratorRound events

  7. P-ORCHESTRATE: confidence_target enforcement
     TDD: goal workflow with confidence_target:0.9 → agent retries if low
     COMMIT: feat(orchestrate): enforce confidence_target on completion

  8. P-ORCHESTRATE: OrchestratorCompleted event
     TDD: goal workflow → OrchestratorCompleted with rounds + cost
     COMMIT: feat(orchestrate): emit OrchestratorCompleted event

PHASE 2 CHECKPOINT:
  [ ] Mock structured output works (no API keys needed)
  [ ] Mock failure simulation works for retry testing
  [ ] Retry with backoff tested E2E
  [ ] Provider fallback tested E2E
  [ ] Artifact writing tested E2E
  [ ] Vision tested with real API
  [ ] Agent guardrails tested E2E
  [ ] Full pipeline tested E2E
  [ ] Structured output validated on 5+ providers
  [ ] P-ORCHESTRATE: events emitted
  [ ] P-ORCHESTRATE: confidence_target enforced
  [ ] cargo test --test e2e_workflow_test → 55+ tests, 0 failures
```

---

# PHASE 3: "MASS VALIDATION" (2 sessions, ~8h)

> Goal: Run all 502 example workflows, fix what breaks, performance optimizations.
> After this phase: 400+ workflows validated, binding pipeline optimized.

## Session 3A: Performance Optimizations (4h)

```
TASKS (4 commits):
  1. Value clone elimination — get_ref() for eager bindings
     File: binding/resolve.rs:328
     TDD: benchmark for_each 100 items × 3 templates — measure allocation count
     COMMIT: perf(binding): zero-clone for eager binding lookups

  2. TransformExpr pre-parsing in template AST
     File: binding/template.rs — TemplateExpr::Alias transforms
     TDD: verify transforms still work (regression) + measure parse count
     COMMIT: perf(template): pre-parse transform expressions in AST

  3. DAG compute_depths Kahn's algorithm O(V+E)
     File: dag/flow.rs:267-318
     TDD: verify same depths as before on test DAGs (regression)
     COMMIT: perf(dag): compute_depths uses Kahn's algorithm O(V+E)

  4. resolve_alias_path — avoid clone via borrowed reference
     File: binding/template.rs:284,325
     TDD: verify template resolution still works (regression)
     COMMIT: perf(template): eliminate Value clone in resolve_alias_path

VERIFICATION:
  cargo test --workspace --lib
  cargo bench (if benchmarks exist)
```

## Session 3B: Mass Workflow Validation (4h)

```
PREPARATION:
  # Build release binary for faster execution
  cargo build --bin nika --release

EXECUTION SCRIPT:
  #!/bin/bash
  PASS=0; FAIL=0; SKIP=0; ERRORS=""
  for f in examples/gates/feature/*.nika.yaml \
           examples/gates/complex/*.nika.yaml \
           examples/dag-patterns/*.nika.yaml \
           examples/use-cases/*.nika.yaml; do
    result=$(timeout 30 ./target/release/nika run "$f" --provider mock --no-live 2>&1)
    exit_code=$?
    last_line=$(echo "$result" | tail -1)
    if [ $exit_code -eq 0 ]; then
      PASS=$((PASS + 1))
    elif echo "$last_line" | grep -q "NIKA-032\|NIKA-030"; then
      SKIP=$((SKIP + 1))  # Missing API key — expected with mock
    else
      FAIL=$((FAIL + 1))
      ERRORS="$ERRORS\n$(basename $f): $last_line"
    fi
  done
  echo "PASS=$PASS FAIL=$FAIL SKIP=$SKIP"
  echo -e "$ERRORS"

FOR EACH FAILURE:
  1. Read the workflow YAML
  2. Determine root cause:
     a) YAML bug (wrong field name, missing schema:, etc.) → fix the .nika.yaml file
     b) Engine bug (parser/runtime error on valid YAML) → fix the Rust code
     c) Feature not available in mock → skip (document)
  3. Fix with TDD — test first, then fix
  4. COMMIT: fix(examples): <workflow-name> — <description>
     OR: fix(parser|runtime): <description> (if engine bug)

VERIFICATION:
  Re-run the mass validation script
  Target: PASS >= 400, FAIL <= 50 (rest are mock-incompatible)

PHASE 3 CHECKPOINT:
  [ ] Value clone elimination merged
  [ ] TransformExpr pre-parsing merged
  [ ] Kahn's algorithm for compute_depths merged
  [ ] 400+/502 example workflows pass with mock
  [ ] All engine bugs found during mass validation fixed
  [ ] All YAML bugs in examples fixed
```

---

# PHASE 4: "SHIP IT" (2 sessions, ~8h)

> Goal: Code review, final E2E validation on all providers, release v0.53.0.
> After this phase: production-ready release tagged and pushed.

## Session 4A: Code Review + Documentation (4h)

```
TASKS:
  1. Launch code-reviewer agent on ALL changed files since v0.52.0
     git diff v0.52.0..HEAD --name-only | grep '\.rs$'
     Review each file for: correctness, security, error handling, tests
     Fix any issues found

  2. Document new features in CLAUDE.md / CHANGELOG.md:
     - ModelResolver usage
     - Global workflow timeout (max_duration_secs)
     - Mock structured output
     - Mock failure simulation (NIKA_MOCK_FAIL_COUNT)
     - P-ORCHESTRATE confidence_target + events

  3. Update error codes table if new NIKA-XXX codes were added

  4. Run cargo deny check (not || true)

  5. Run cargo machete (verify no unused deps)

  6. Verify Dockerfile VERSION matches new version

VERIFICATION:
  cargo deny check              # No advisories
  cargo machete                 # No unused deps
  cargo test --workspace --lib  # All pass
  cargo test --test e2e_workflow_test  # All E2E pass
```

## Session 4B: Final Validation + Release (4h)

```
TASKS:
  1. Version bump: 0.52.0 → 0.53.0 in tools/Cargo.toml

  2. CHANGELOG entry for v0.53.0 with ALL changes since v0.52.0

  3. Final E2E validation on ALL available providers:
     cargo test --test e2e_workflow_test -- e2e_real
     cargo test --test e2e_workflow_test -- e2e_structured
     Expected: 5/7 providers pass (Anthropic billing, Gemini quota)

  4. Run mass workflow validation one final time:
     Target: 400+/502 pass

  5. Full test suite:
     cargo test --workspace --lib
     Target: 9,300+ tests, 0 failures

  6. Final clippy:
     cargo clippy --workspace --all-targets --all-features -- -D warnings
     Target: 0 warnings

  7. Tag and push:
     git tag v0.53.0
     git push && git push --tags

PHASE 4 CHECKPOINT (RELEASE GATE):
  [ ] Code review agent found no critical issues
  [ ] CHANGELOG complete
  [ ] cargo deny check passes
  [ ] cargo machete clean
  [ ] Version bumped to 0.53.0
  [ ] 9,300+ lib tests pass
  [ ] 55+ E2E tests pass
  [ ] 5/7 providers validated
  [ ] 400+/502 example workflows pass
  [ ] 0 clippy warnings
  [ ] Tag v0.53.0 pushed
```

---

# SESSION HANDOFF COMMANDS

Each session gets its own prompt. Copy-paste the appropriate one:

```bash
# Session 1A: ModelResolver Complete Wire
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 1A/8: ModelResolver Complete Wire.

ETAT: v0.52.0+8 commits, 8970 tests, 0 clippy.
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Wire ModelResolver dans TOUS les sites restants (20+ hardcoded models).
Aussi: saturating_add token overflow, concurrency:0 validation.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 1A)
REFERENCE: docs/plans/sessions/mega-prompt-v053-final-v2.md

REGLES: TDD, 1 fix = 1 commit, cargo test + clippy avant chaque commit.
Push toutes les 2-3 commits. Co-authors obligatoires.

GO: Verification initiale puis 9 commits.
"

# Session 1B: Safety Limits + Security
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 1B/8: Safety Limits + Security.

ETAT: Post-Session 1A (verifier git log).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: 50MB output limit, 600s structured timeout, 50MB MCP limit,
DNS pinning, global workflow timeout, stream channel cleanup, L0b event.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 1B)

REGLES: TDD, 1 fix = 1 commit, verification avant commit.
Push toutes les 2-3 commits. Phase 1 checkpoint a la fin.

GO: Verification initiale puis 7 commits.
"

# Session 2A: Mock Provider + Core E2E
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 2A/8: Mock Provider + Core E2E.

ETAT: Post-Phase 1 (verifier git log + Phase 1 checkpoint).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Mock structured output (generate_mock_json), mock failure simulation,
E2E tests pour retry, fallback, artifact, for_each+structured.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 2A)

REGLES: TDD obligatoire — RED test d'abord puis GREEN.
Tester avec vrais providers quand API keys disponibles.

GO: Verification initiale puis 6 commits.
"

# Session 2B: Real Providers + P-ORCHESTRATE
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 2B/8: Real Providers + P-ORCHESTRATE.

ETAT: Post-Session 2A (verifier git log).
DIR: /Users/thibaut/dev/supernovae/nika/tools
PROVIDERS: OpenAI OK, xAI OK, Groq OK (daemon), Mistral OK (daemon), DeepSeek OK (daemon).

OBJECTIF: Vision E2E, guardrails E2E, full pipeline E2E, structured sur tous providers,
P-ORCHESTRATE events + confidence_target enforcement.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 2B)

REGLES: Tester avec vrais appels API. Validation programmatique du JSON.
Phase 2 checkpoint a la fin.

GO: Verification initiale puis 8 commits.
"

# Session 3A: Performance Optimizations
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 3A/8: Performance Optimizations.

ETAT: Post-Phase 2 (verifier git log + Phase 2 checkpoint).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Value clone elimination (get_ref), TransformExpr pre-parsing,
Kahn's algorithm for compute_depths, resolve_alias_path optimization.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 3A)
REFERENCE: docs/plans/master-plan-v2-definitive.md (Performance section)

REGLES: Regression tests obligatoires. Mesurer avant/apres si possible.

GO: Verification initiale puis 4 commits.
"

# Session 3B: Mass Workflow Validation
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 3B/8: Mass Workflow Validation.

ETAT: Post-Session 3A (verifier git log).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Executer les 502 example workflows avec mock provider.
Fixer chaque echec: bug YAML → fix .nika.yaml, bug engine → fix Rust.
Target: 400+ pass sur 502.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 3B)

REGLES: cargo build --release d'abord. 1 fix = 1 commit.
Phase 3 checkpoint a la fin.

GO: Build release puis mass validation script.
"

# Session 4A: Code Review + Documentation
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 4A/8: Code Review + Documentation.

ETAT: Post-Phase 3 (verifier git log + Phase 3 checkpoint).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Code review agent sur tous les fichiers changes depuis v0.52.0.
Documenter les nouvelles features. cargo deny + machete. Dockerfile version.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 4A)

REGLES: Lancer l'agent code-reviewer. Fixer tout probleme trouve.

GO: git diff v0.52.0..HEAD --name-only | grep '.rs$' puis review.
"

# Session 4B: Final Validation + Release v0.53.0
claude --dangerously-skip-permissions --model opus -p "
Tu es l'orchestrateur Nika. Session 4B/8: Final Validation + Release v0.53.0.

ETAT: Post-Session 4A (verifier git log).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Version bump 0.53.0, CHANGELOG, final E2E sur tous providers,
mass validation finale, full test suite, tag + push.

PLAN DETAILLE: docs/plans/sessions/multi-session-master-plan.md (Section: Session 4B)

REGLES: RELEASE GATE — tous les checkpoints doivent passer.
9300+ tests, 55+ E2E, 400+ workflows, 0 clippy.

GO: Version bump puis validation finale puis tag.
"
```

---

# METRICS TRACKING

| Metric | Start | Phase 1 | Phase 2 | Phase 3 | Phase 4 |
|--------|-------|---------|---------|---------|---------|
| Tests lib | 8,970 | 9,050+ | 9,150+ | 9,250+ | 9,300+ |
| Tests E2E | 40 | 42+ | 55+ | 55+ | 55+ |
| Hardcoded models | 20+ | 0 | 0 | 0 | 0 |
| Panics production | 0 | 0 | 0 | 0 | 0 |
| OOM protection | none | 50MB | 50MB | 50MB | 50MB |
| Mock structured | NO | NO | YES | YES | YES |
| Vision E2E | NO | NO | YES | YES | YES |
| Providers validated | 5/7 | 5/7 | 5/7 | 5/7 | 5/7 |
| Workflows validated | 0/502 | 0/502 | 0/502 | 400+/502 | 400+/502 |
| P-ORCHESTRATE | wired | wired | complete | complete | complete |
| Perf: Value clones | O(N*M) | O(N*M) | O(N*M) | O(lazy) | O(lazy) |
| Version | 0.52.0+ | 0.52.0+ | 0.52.0+ | 0.52.0+ | **0.53.0** |
