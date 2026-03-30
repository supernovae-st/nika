# Nika v0.53 — Mega Handoff: Phase 3-4 (Sessions 3A-4B)

> Handoff from Phase 1-2 session (26 commits). Copy-paste the appropriate session prompt below.

---

## STATE AT HANDOFF (2026-03-31)

```
Version:     v0.52.0 + 26 commits (untagged)
Tests:       9,009 lib (0 fail) + 47 E2E (41 pass, 6 fail)
Clippy:      0 warnings (--all-targets --all-features)
Branch:      main
Directory:   /Users/thibaut/dev/supernovae/nika/tools
Last commit: 7c77f8779 fix(runtime): hoist sleep future, depth-limit size estimate, harden mock
```

### Provider Status (2026-03-31)
| Provider | Status | E2E Validated? |
|----------|--------|----------------|
| OpenAI | **OK** | YES (gpt-4.1-nano structured) |
| xAI | **OK** | YES (grok-3-fast structured) |
| Gemini | **RATE LIMITED** (free tier) | NO |
| Anthropic | **NO CREDITS** | NO |
| Groq | OK (daemon) | Not in E2E |
| Mistral | OK (daemon) | Not in E2E |
| DeepSeek | OK (daemon) | Not in E2E |
| Mock | **OK** | YES (12 mock tests pass) |

### 6 Failing E2E Tests (all known reasons)
| Test | Reason | Fix |
|------|--------|-----|
| `e2e_real_anthropic_haiku` | No Anthropic credits | Skip or fund |
| `e2e_structured_anthropic` | No Anthropic credits | Skip or fund |
| `e2e_real_gemini_flash` | Gemini rate limit | Skip or upgrade tier |
| `e2e_structured_gemini` | Gemini rate limit | Skip or upgrade tier |
| `e2e_real_research_pipeline` | Uses Anthropic (no credits) | Skip or fund |
| `e2e_adversarial_structured_additional_properties_false` | **BUG**: mock now generates schema-conforming JSON (no extra fields), so `additionalProperties: false` passes instead of failing. Test needs update — see BUG-AP1 below. |

---

## WHAT WAS DONE (Phases 1-2, 26 commits)

### Phase 1: "STOP BREAKING" — 15 commits
- **ModelResolver wired everywhere** — 9 infer.rs sites + agent.rs + compressor + 4 TUI files
- **PROVIDER_CHEAP_MODELS catalog** — cheap_model_for_provider() for compression/repair
- **saturating_add** — 21 sites across 6 files (providers, cost, renderer, introspect)
- **concurrency:0 rejected** — Analyzer validation + 3 tests (standalone + embedded for_each)
- **50MB output limit** — TaskResult::truncate_if_oversized() in runner.rs
- **50MB MCP result limit** — content_size_bytes() check in invoke.rs
- **600s structured timeout** — validate → validate_inner + tokio::time::timeout
- **Global workflow timeout** — max_duration_secs field (NIKA-038), sleep_until pinned in select!
- **DNS TOCTOU closed** — resolve_and_pin_ssrf() → reqwest .resolve() pinning
- **Stream channel buffer** — 64→1 for discarded receivers
- **Provider ID fix** — p.id (canonical) instead of p.aliases.first()
- **Fallback position fix** — agent.rs tracks chain index for ModelResolver

### Phase 2: "PROVE IT WORKS" — 11 commits
- **Mock structured output** — generate_mock_json() from JSON Schema
- **Mock failure simulation** — NIKA_MOCK_FAIL_COUNT env var + AtomicU32
- **7 new E2E tests** — fallback, artifact, for_each+structured, guardrails, OpenAI, xAI
- **Guardrail fix** — mock path now runs guardrails before early return
- **P-ORCHESTRATE events** — OrchestratorStarted + OrchestratorCompleted emitted
- **Review fixes** — DNS pinning wired, sleep hoisted, depth limit, pub(crate), mock hardened

---

## KNOWN BUGS (10 items — fix before v0.53.0)

### BUG-AP1: `e2e_adversarial_structured_additional_properties_false` needs update
**File**: `nika/tests/e2e_workflow_test.rs:872`
**What**: Test expects mock to fail `additionalProperties: false` but mock now generates only schema fields.
**Fix**: Update test to verify mock passes (it's now correct behavior) OR test additionalProperties with a manually crafted non-conforming response.

### BUG-ROUND: OrchestratorRound events never emitted in production
**File**: `nika-engine/src/runtime/runner.rs:2662`
**What**: `OrchestratorCompleted.rounds` is always 0 because no code emits `OrchestratorRound`.
**Fix**: Emit OrchestratorRound in the agent loop (rig_agent_loop) when task_id == "__orchestrator__", or in the runner when a non-orchestrator task completes during orchestration.

### BUG-CONFIDENCE: confidence_target parsed but never enforced
**File**: `nika-core/src/ast/orchestrate.rs:17` (parsed), never read in execution
**What**: `confidence_target: 0.85` is deserialized but execution always hardcodes `confidence: 1.0`.
**Fix**: In RigAgentLoop, when `nika:complete` is called, extract confidence from params. Compare against config target. If below, inject retry prompt.

### BUG-COUNTER: MOCK_CALL_COUNTER is process-global static, never resets
**File**: `nika-engine/src/runtime/executor/infer.rs:286`
**What**: AtomicU32 shared across concurrent tests. Counter never resets between test runs.
**Fix**: Reset counter when env var is read: `MOCK_CALL_COUNTER.store(0, Ordering::SeqCst)` at the start of each check, or use per-Runner scoping.

### BUG-LOWER: max_duration_secs discarded in lower.rs
**File**: `nika-engine/src/ast/lower.rs:61` (destructured as `_`), line 692 (hardcoded 3600)
**What**: User's `max_duration_secs:` is lost on round-trip through lower/unlower.
**Fix**: Add field to lowered `Workflow` struct and propagate.

### BUG-SSRF-POST: Post-redirect SSRF uses old boolean check, not pinned
**File**: `nika-engine/src/runtime/executor/fetch.rs:~365`
**What**: Post-redirect check still calls `resolve_and_check_ssrf` (boolean), not the pinned version.
**Fix**: Use `resolve_and_pin_ssrf` for post-redirect check too (or accept as defense-in-depth layer).

### BUG-VISION-SSRF: Vision image URL SSRF check not pinned
**File**: `nika-engine/src/runtime/executor/infer.rs:~1363`
**What**: Vision `image_url` path uses `resolve_and_check_ssrf`, same TOCTOU gap.
**Fix**: Wire resolve_and_pin_ssrf for vision URLs too.

### DEBT-SCOPE: AgentParams.scope parsed but not implemented
**File**: `nika-engine/src/runtime/rig_agent_loop/mod.rs:285`
**What**: `scope: minimal` accepted but ignored — all tools always available.
**Fix**: Implement scope filtering or remove the parameter.

### DEBT-DOC: Doc comment corruption on generic_mock_json
**File**: `nika-engine/src/runtime/executor/infer.rs:~1470`
**What**: `generic_mock_json` has the doc comment that belongs to `check_infer_guardrails`.
**Fix**: Separate the doc comments.

### DEBT-ALLOF: generate_mock_json doesn't handle allOf/anyOf/oneOf/$ref
**File**: `nika-engine/src/runtime/mock_json.rs`
**What**: Composition keywords fall through to "string" default.
**Fix**: Handle allOf (merge schemas), anyOf/oneOf (pick first variant).

---

## PHASE 3: "MASS VALIDATION" — 2 sessions

### Session 3A: Performance Optimizations (4h)

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
```

### Session 3B: Mass Workflow Validation (4h)

```
PREPARATION:
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
      SKIP=$((SKIP + 1))
    else
      FAIL=$((FAIL + 1))
      ERRORS="$ERRORS\n$(basename $f): $last_line"
    fi
  done
  echo "PASS=$PASS FAIL=$FAIL SKIP=$SKIP"
  echo -e "$ERRORS"

TARGET: PASS >= 400, FAIL <= 50
```

---

## PHASE 4: "SHIP IT" — 2 sessions

### Session 4A: Code Review + Documentation (4h)

```
TASKS:
  1. Fix all 10 KNOWN BUGS above (BUG-AP1 through DEBT-ALLOF)
  2. Launch code-reviewer agent on ALL changed files since v0.52.0:
     git diff v0.52.0..HEAD --name-only | grep '\.rs$'
  3. Update CHANGELOG.md with ALL changes since v0.52.0
  4. Run cargo deny check (not || true)
  5. Run cargo machete (unused deps)
  6. Verify Dockerfile VERSION
```

### Session 4B: Final Validation + Release v0.53.0 (4h)

```
TASKS:
  1. Version bump: 0.52.0 → 0.53.0 in tools/Cargo.toml
  2. CHANGELOG entry for v0.53.0
  3. Final E2E on all available providers
  4. Mass workflow validation (target: 400+/502)
  5. Full test suite: target 9,300+ tests
  6. Final clippy: 0 warnings
  7. Tag and push: git tag v0.53.0 && git push && git push --tags

RELEASE GATE:
  [ ] 9,300+ lib tests pass
  [ ] 55+ E2E tests pass
  [ ] 400+/502 workflows validated
  [ ] 0 clippy warnings
  [ ] cargo deny check passes
  [ ] cargo machete clean
  [ ] CHANGELOG complete
  [ ] Tag v0.53.0 pushed
```

---

## SESSION HANDOFF PROMPTS

### Session 3A: Performance Optimizations

```
Tu es l'orchestrateur Nika. Session 3A/8: Performance Optimizations.

ETAT: v0.52.0+26 commits, 9009 tests, 0 clippy.
DIR: /Users/thibaut/dev/supernovae/nika/tools
PROVIDERS: OpenAI OK, xAI OK, Gemini RATE LIMITED, Anthropic NO CREDITS.

OBJECTIF: Optimisations perf — Value clone elimination, TransformExpr
pre-parsing, Kahn's algorithm for compute_depths, resolve_alias_path
zero-clone.

PLAN DETAILLE: docs/plans/sessions/handoff-phase3-v053.md (Section: Session 3A)
REFERENCE: docs/plans/sessions/multi-session-master-plan.md

REGLES: TDD, 1 fix = 1 commit, cargo test + clippy avant chaque commit.
Regression tests obligatoires. Mesurer avant/apres si possible.

GO: Verification initiale puis 4 commits.
```

### Session 3B: Mass Workflow Validation

```
Tu es l'orchestrateur Nika. Session 3B/8: Mass Workflow Validation.

ETAT: Post-Session 3A (verifier git log).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Executer les 502 example workflows avec mock provider.
Fixer chaque echec: bug YAML → fix .nika.yaml, bug engine → fix Rust.
Target: 400+ pass sur 502.

PLAN DETAILLE: docs/plans/sessions/handoff-phase3-v053.md (Section: Session 3B)

REGLES: cargo build --release d'abord. 1 fix = 1 commit.
Phase 3 checkpoint a la fin.

GO: Build release puis mass validation script.
```

### Session 4A: Bug Fixes + Code Review + Docs

```
Tu es l'orchestrateur Nika. Session 4A/8: Bug Fixes + Code Review + Documentation.

ETAT: Post-Phase 3 (verifier git log + Phase 3 checkpoint).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Fixer les 10 KNOWN BUGS listes dans le handoff, puis lancer
code-reviewer agent sur tous les fichiers changes depuis v0.52.0.
Documenter nouvelles features. cargo deny + machete.

KNOWN BUGS A FIXER (priorite):
  BUG-AP1:     e2e_adversarial test needs update (mock now schema-conforming)
  BUG-ROUND:   OrchestratorRound events never emitted in production
  BUG-CONFIDENCE: confidence_target never enforced (always 1.0)
  BUG-COUNTER: MOCK_CALL_COUNTER never resets between tests
  BUG-LOWER:   max_duration_secs lost in lower/unlower round-trip
  BUG-SSRF-POST: Post-redirect SSRF not pinned
  BUG-VISION-SSRF: Vision image URL SSRF not pinned
  DEBT-SCOPE:  AgentParams.scope parsed but not implemented
  DEBT-DOC:    Doc comment corruption on generic_mock_json
  DEBT-ALLOF:  generate_mock_json doesn't handle allOf/anyOf/oneOf

PLAN DETAILLE: docs/plans/sessions/handoff-phase3-v053.md (Section: Session 4A)

REGLES: Lancer code-reviewer agent. Fixer tout probleme trouve.
CHANGELOG obligatoire. Phase 4 checkpoint a la fin.

GO: Fix 10 bugs puis review puis documentation.
```

### Session 4B: Final Validation + Release v0.53.0

```
Tu es l'orchestrateur Nika. Session 4B/8: Final Validation + Release v0.53.0.

ETAT: Post-Session 4A (verifier git log).
DIR: /Users/thibaut/dev/supernovae/nika/tools

OBJECTIF: Version bump 0.53.0, CHANGELOG, final E2E sur tous providers,
mass validation finale, full test suite, tag + push.

PLAN DETAILLE: docs/plans/sessions/handoff-phase3-v053.md (Section: Session 4B)

RELEASE GATE:
  [ ] 9,300+ lib tests, 0 failures
  [ ] 55+ E2E tests pass
  [ ] 400+/502 example workflows pass
  [ ] 0 clippy warnings
  [ ] cargo deny check passes
  [ ] CHANGELOG complete
  [ ] Version bumped to 0.53.0
  [ ] Tag v0.53.0 pushed

REGLES: RELEASE GATE — tous les checkpoints doivent passer.

GO: Version bump puis validation finale puis tag.
```

---

## REVIEW AGENT FINDINGS (resolved in this session)

### Resolved CRITICALs (4 total, all fixed)
1. `fallback_position: 0` hardcoded in agent.rs → tracked chain index
2. Missed `saturating_add` in 3 files → fixed 12 sites
3. DNS pinned addresses discarded → wired to reqwest .resolve()
4. `p.aliases.first()` wrong provider_id → use `p.id` canonical

### Resolved HIGHs (5 total, all fixed)
1. `sleep_until` re-created per loop iteration → hoisted + pinned
2. `max_duration_secs` overflow → clamped to 604800
3. `estimate_value_size` unbounded recursion → 128-level depth limit
4. `mock_json` pub → pub(crate)
5. Dead branch + minItems clamp → removed + fixed

### Unresolved MEDIUMs (deferred to Session 4A)
1. Dead `model: Option<&str>` wrapper in infer.rs (works but semantically redundant)
2. `lifecycle.rs` still hardcodes provider list (not KNOWN_PROVIDERS)
3. `Cow<str>` opportunity in ModelResolver (acceptable for v0)
4. f64 cost accumulators not guarded (infinity acceptable)
5. `original_model` provenance gap in ModelResolver (rare edge case)

---

## METRICS TRACKING

| Metric | Start | Phase 1 | Phase 2 | Target Phase 3 | Target Phase 4 |
|--------|-------|---------|---------|----------------|----------------|
| Commits | 0 | 15 | 26 | 30+ | 40+ |
| Tests lib | 8,970 | 8,998 | 9,009 | 9,100+ | 9,300+ |
| Tests E2E | 40 | 42 | 47 | 47+ | 55+ |
| Hardcoded models | 20+ | 0 | 0 | 0 | 0 |
| OOM protection | none | 50MB | 50MB | 50MB | 50MB |
| Mock structured | NO | NO | YES | YES | YES |
| Providers validated | 2/7 | 2/7 | 4/7 | 4/7 | 5/7 |
| Workflows validated | 0/502 | 0/502 | 0/502 | 400+/502 | 400+/502 |
| Known bugs | 12 | 3 | 10 | 5 | 0 |
| Version | 0.52.0+ | 0.52.0+ | 0.52.0+ | 0.52.0+ | **0.53.0** |
