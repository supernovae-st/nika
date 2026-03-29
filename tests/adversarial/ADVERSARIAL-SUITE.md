# Adversarial Test Suite — 30 Scenarios

QA tests designed to BREAK the Nika workflow engine. Each test targets a specific
code path discovered through codebase analysis.

## Categories

### DATA FLOW TRAPS (Tests 01–10)

| # | File | Status | Target Code | Expected |
|---|------|--------|-------------|----------|
| 01 | `01-circular-binding.nika.yaml` | SHOULD_FAIL | `dag/validate.rs:192` WithCircularDep | NIKA-020 or NIKA-081 |
| 02 | `02-nested-for-each.nika.yaml` | SHOULD_PASS | `runner.rs:2170` for_each output type | Value::Array(Array) |
| 03 | `03-structured-output-in-structured.nika.yaml` | SHOULD_PASS | `structured_output.rs` Layer 2 | Input JSON not confused with output |
| 04 | `04-env-var-template-injection.nika.yaml` | SHOULD_PASS | `template.rs:536` injection guard | Literal text, not expanded |
| 05 | `05-null-binding-in-template.nika.yaml` | SHOULD_FAIL | `resolve.rs:441` NullValue | NIKA-072 |
| 06 | `06-large-output-binding.nika.yaml` | SHOULD_PASS | `template.rs:88` MAX_TEMPLATE_VARS | No OOM, correct length |
| 07 | `07-for-each-empty-array.nika.yaml` | SHOULD_PASS | `runner.rs:2333` empty for_each | Empty array result |
| 08 | `08-for-each-single-item.nika.yaml` | SHOULD_PASS | `runner.rs:2211` single-item loop | Array(1), not scalar |
| 09 | `09-depends-on-self.nika.yaml` | SHOULD_PASS | `dag/flow.rs:1412` self-dep skipped | Runs normally |
| 10 | `10-binding-missing-dep.nika.yaml` | SHOULD_PASS | DAG implicit_deps from with: | task_a runs before task_b |

### STRUCTURED OUTPUT STRESS (Tests 11–20)

| # | File | Status | Target Code | Expected |
|---|------|--------|-------------|----------|
| 11 | `11-deep-nested-schema.nika.yaml` | SHOULD_PASS | `output.rs:52` get_or_compile_validator | No stack overflow |
| 12 | `12-large-schema-array.nika.yaml` | SHOULD_FAIL (mock) | `output.rs` minItems validation | NIKA-300 after retries |
| 13 | `13-conflicting-schema-constraints.nika.yaml` | SHOULD_FAIL | `output.rs` impossible schema | NIKA-300 — unsatisfiable |
| 14 | `14-empty-object-schema.nika.yaml` | SHOULD_PASS | `output.rs` empty schema | Any value accepted |
| 15 | `15-llm-resists-json.nika.yaml` | SHOULD_PASS | Layer 0 tool injection | JSON forced despite protest |
| 16 | `16-unicode-schema-field-names.nika.yaml` | SHOULD_PASS | `output.rs:42` blake3 hash | Unicode keys handled |
| 17 | `17-valid-json-wrong-schema.nika.yaml` | SHOULD_FAIL (mock) | Layer 2 type validation | NIKA-300 after retries |
| 18 | `18-additional-properties-false.nika.yaml` | SHOULD_PASS | Layer 2 additionalProperties | Extra fields rejected |
| 19 | `19-same-schema-three-providers.nika.yaml` | SHOULD_PASS | Provider-specific Layer 0 | All 3 produce valid output |
| 20 | `20-structured-output-with-for-each.nika.yaml` | SHOULD_PASS | structured: + for_each combo | Per-iteration validation |

### PROVIDER EDGE CASES (Tests 21–25)

| # | File | Status | Target Code | Expected |
|---|------|--------|-------------|----------|
| 21 | `21-provider-returns-empty-string.nika.yaml` | SHOULD_FAIL | `output.rs:171` empty + JSON | NIKA-300 |
| 22 | `22-very-long-prompt.nika.yaml` | SHOULD_PASS (mock) | Template MAX_TEMPLATE_VARS | No truncation with mock |
| 23 | `23-non-latin-structured-output.nika.yaml` | SHOULD_PASS | `output_scanner.rs:27` unicode | Unicode not flagged as dangerous |
| 24 | `24-task-level-model-override.nika.yaml` | SHOULD_PASS | Executor model resolution | Task model beats workflow model |
| 25 | `25-invalid-provider-fallback.nika.yaml` | SHOULD_FAIL | `error.rs:195` NIKA-030 | Clear per-task provider error |

### TIMING & CONCURRENCY (Tests 26–30)

| # | File | Status | Target Code | Expected |
|---|------|--------|-------------|----------|
| 26 | `26-for-each-concurrency-100-items-3.nika.yaml` | SHOULD_PASS | `runner.rs:2200` Semaphore::new | 3/3 complete, no deadlock |
| 27 | `27-diamond-parallel-execution.nika.yaml` | SHOULD_PASS | DAG parallel scheduling | Correct merge, thread-safe store |
| 28 | `28-exec-timeout-one-second.nika.yaml` | SHOULD_FAIL | `exec.rs:145` kill_on_drop | NIKA-096, no orphan processes |
| 29 | `29-fetch-timeout.nika.yaml` | SHOULD_FAIL | `error.rs` NIKA-045 | Timeout surfaced, SSRF blocked |
| 30 | `30-agent-max-turns-one.nika.yaml` | MIXED | `rig_agent_loop/mod.rs:232` | 1=pass, 0=NIKA-113, 101=NIKA-113 |

## Running the Suite

```bash
# Run all adversarial tests (mock provider — no API keys needed)
for f in tests/adversarial/*.nika.yaml; do
  echo "=== $f ==="
  nika check "$f" 2>&1 | tail -5
done

# Run a specific test
nika run tests/adversarial/01-circular-binding.nika.yaml

# Validate syntax only (faster)
nika check tests/adversarial/07-for-each-empty-array.nika.yaml

# Tests requiring real API keys (skip with mock)
nika run tests/adversarial/19-same-schema-three-providers.nika.yaml

# Network tests (require internet access)
nika run tests/adversarial/29-fetch-timeout.nika.yaml
```

## Key Bug Targets Discovered

1. **Self-dep silent skip** (test 09): `depends_on: [self]` is silently skipped, not rejected.
   Users may expect an error but get silent behavior.

2. **for_each empty array** (test 07): Empty for_each stores `[]` success — downstream
   code using `| first` without `| default("none")` will propagate null.

3. **Impossible schema exhausts retries** (test 13): `minimum: 10, maximum: 5` compiles
   successfully but all validations fail. Layer 3 uselessly retries max_retries times.

4. **for_each output type mismatch** (test 08): Single-item for_each produces `["result"]`
   not `"result"`. Downstream code treating it as scalar silently breaks.

5. **Template injection guard** (test 04): Env var containing `{{...}}` is blocked.
   This is a FEATURE but must be verified as working.

6. **concurrent semaphore over-capacity** (test 26): concurrency:100 with 3 items —
   semaphore has 97 unused permits; must verify no deadlock on JoinSet.await.

7. **Unicode false-positive in output_scanner** (test 23): `is_dangerous_unicode` may
   incorrectly flag legitimate CJK/Arabic content as security violations.
