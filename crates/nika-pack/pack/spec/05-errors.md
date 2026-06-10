# 05 · Errors

> Nika has a **typed error model**. Every error has a code · a category ·
> and structured details. Tasks may declare retry policies and fallback
> recovery. The workflow itself fails when an unrecovered terminal error
> reaches a task with no `on_error:` policy.

---

## Error structure

Every error is a typed structure ·

```json
{
  "code": "NIKA-INFER-001",
  "category": "provider_error",
  "message": "Anthropic API returned 503 service unavailable",
  "transient": true,
  "details": {
    "provider": "anthropic",
    "model": "claude-3-5-sonnet",
    "status_code": 503,
    "retry_after_secs": 30
  },
  "task_id": "research",
  "attempt": 2
}
```

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `code` | yes | string | `NIKA-<NAMESPACE>-<NNN>` · stable identifier |
| `category` | yes | enum | See category list below |
| `message` | yes | string | Human-readable description |
| `transient` | yes | boolean | True if retry might succeed (network · 503 · rate limit) |
| `details` | no | object | Category-specific structured fields |
| `task_id` | yes (runtime) | string | Which task this error occurred in |
| `attempt` | yes (runtime) | integer | Which attempt failed (1-indexed) |

---

## Error code namespaces

Error codes follow the format `NIKA-<NAMESPACE>-<NNN>` where namespace is 2-9 uppercase letters and NNN is a 3-digit zero-padded number. A code MAY add an **optional sub-namespace** for self-documentation · `NIKA-<NAMESPACE>-<SUB>-<NNN>` (4-segment) — used per-builtin (`NIKA-BUILTIN-WAIT-001` · each builtin owns its own 001-099) or per-field (`NIKA-PARSE-WHEN-001` · the `when:` field of a parse error). The canonical regex is `^NIKA-[A-Z]{2,9}(-[A-Z]{2,9})?-[0-9]{3}$` (also the `retry.on_codes` validation pattern).

| Namespace | Scope | Reserved range |
|---|---|---|
| `NIKA-PARSE` | YAML parse + envelope validation | 001-099 |
| `NIKA-DAG` | DAG topology · cycles · invalid deps | 001-099 |
| `NIKA-VAR` | Variable resolution failures | 001-099 |
| `NIKA-INFER` | `infer:` verb errors | 001-099 |
| `NIKA-EXEC` | `exec:` verb errors | 001-099 |
| `NIKA-INVOKE` | `invoke:` verb errors | 001-099 |
| `NIKA-AGENT` | `agent:` verb errors | 001-099 |
| `NIKA-PROVIDER` | Provider adapter errors | 001-099 per provider |
| `NIKA-BUILTIN-<BUILTIN>` | Builtin tool errors · per-builtin sub-namespace (`NIKA-BUILTIN-WAIT-001` · `NIKA-BUILTIN-NOTIFY-001` · `NIKA-BUILTIN-INSPECT-001` · `NIKA-BUILTIN-FETCH-001` — `nika:fetch`'s network/extraction errors, whose instances carry `category: network_error` though the namespace is the uniform `NIKA-BUILTIN`) | 001-099 per builtin |
| `NIKA-MCP` | MCP client errors | 001-099 |
| `NIKA-SEC` | Security policy violations (SSRF · blocklist) | 001-099 |
| `NIKA-TIMEOUT` | Task or step timeouts | 001-099 |
| `NIKA-CANCEL` | Task or workflow cancellation | 001-099 |
| `NIKA-IMPL` | Engine internal errors | 001-099 |

A v0.1-compliant engine MUST use these namespaces for the canonical categories. New error codes MAY be added in minor bumps (additive · never repurposed).

### Taxonomy ownership · the spec table is normative · engines derive

**This table — not any engine's source code — owns the taxonomy.** A
conformant engine (the Rust reference included) *derives* its error types
from this section: every spec-relevant error it emits MUST carry a code
matching the canonical regex, in the namespace this table assigns to the
failure's scope, with the category semantics of §Categories. An engine MAY
keep richer internal error machinery (the reference engine's internal
diagnostics codes, subsystem-specific numbering, extra metadata) — internal
codes are **not** spec surface and MUST NOT leak into workflow-visible
errors (`tasks.X.error` · run reports · conformance output) in place of the
canonical form. Two consequences ·

1. **A second engine can be error-conformant from this file alone** — the
   conformance suite matches on `code` OR `namespace`+`category`
   ([07](./07-conformance.md)) · nothing requires reading the reference
   implementation.
2. **Drift direction is defined** · if the reference engine and this table
   disagree, the table wins and the engine fixes (same rule as the published
   JSON Schema · the prose is normative on conflict per [07](./07-conformance.md)).

---

## Categories

The `category` field is a closed enum at v1 ·

| Category | Meaning | `transient` default |
|---|---|---|
| `parse_error` | Workflow YAML is malformed or invalid | false |
| `validation_error` | Workflow violates a spec rule (cycle · unknown field · etc.) | false |
| `variable_error` | Reference to undefined variable or invalid path | false |
| `provider_error` | LLM provider returned an error | true (engine assesses) |
| `network_error` | Network failure (DNS · TCP · TLS · timeout) | true |
| `tool_error` | Builtin or MCP tool returned an error | depends |
| `security_error` | SSRF · blocklist · capability denied | false |
| `timeout_error` | Task or step exceeded its timeout | false |
| `cancelled` | Workflow or task cancelled | false |
| `internal_error` | Engine bug · unexpected state | false |

---

## Retry policy

A task MAY declare a `retry:` block. Retries apply to **transient** errors only (`error.transient == true`).

### Syntax

```yaml
- id: flaky_api
  invoke:
    tool: "nika:fetch"
    args:
      url: "https://flaky.example.com/data"
  retry:
    max_attempts: 5              # default 1 (no retry)
    backoff_ms: 1000             # initial backoff
    backoff_strategy: exponential  # fixed | linear | exponential
    backoff_max_ms: 30000        # cap on backoff (default 60000)
    jitter: true                 # randomize backoff (default true · anti-thundering-herd)
    on_codes:                    # optional · whitelist of codes to retry
      - NIKA-BUILTIN-FETCH-001
      - NIKA-PROVIDER-001
```

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `max_attempts` | yes | integer ≥ 1 | Total attempts (including first try) |
| `backoff_ms` | no | integer | Initial backoff · default 1000 |
| `backoff_strategy` | no | enum | `fixed` · `linear` · `exponential` (default `exponential`) |
| `backoff_max_ms` | no | integer | Cap · default 60000 (1 min) |
| `jitter` | no | boolean | Randomize the computed backoff to avoid thundering-herd · **default true** · engines SHOULD use a full-jitter / equal-jitter family (AWS « exponential backoff and jitter ») |
| `on_codes` | no | array | If present · only retry on listed `NIKA-<NS>-<NNN>` codes · else retry all transient |

### Backoff strategies

- `fixed` · `backoff_ms` between every attempt
- `linear` · `backoff_ms * attempt` between attempts (1s · 2s · 3s · …)
- `exponential` · `backoff_ms * 2^(attempt-1)` between attempts (1s · 2s · 4s · 8s · …) · capped at `backoff_max_ms`

With `jitter: true` (the default) the computed delay is randomized (full-jitter
or equal-jitter family · per AWS « exponential backoff and jitter ») so many
tasks retrying the same upstream do not synchronize into a thundering herd.
`on_codes` lists canonical `NIKA-<NS>-<NNN>` codes (e.g. `NIKA-BUILTIN-FETCH-001`) — not
HTTP status numbers.

### Conformance

A v0.1-compliant engine MUST ·

- Honor `max_attempts` strictly
- Use the configured backoff between attempts
- Only retry transient errors (`error.transient == true`) unless `on_codes` is configured
- Surface the LAST error if all retries fail

---

## Error recovery · `on_error:`

A task MAY declare an `on_error:` block to recover from non-transient errors (or retried-and-still-failing transient errors).

### Syntax

```yaml
- id: api_call
  invoke:
    tool: "nika:fetch"
    args:
      url: "https://api.example.com/data"
  retry: { max_attempts: 3 }
  on_error:
    recover: ${{ tasks.cached_data.output }}    # recovery output · a ${{ }} ref OR a literal
    # OR
    # skip: true                          # skip · downstream sees status = skipped
    # OR
    # fail_workflow: true                 # explicit · same as no on_error
```

### Fields (mutually exclusive)

| Field | Effect | Downstream sees |
|---|---|---|
| `recover: <value>` | Use a recovery output — a `${{ }}` ref (e.g. another task's output) OR a literal | `status: success` · output = recover value |
| `skip: true` | Skip this task on error | `status: skipped` |
| `fail_workflow: true` | Fail the whole workflow (default behavior) | n/a (workflow fails) |

`recover:` merges the former `fallback:` (ref) + `value:` (literal) into one field
(`${{ }}` resolves to values either way · 4 modes → 3 · one way).

### `recover:` reference resolution (normative)

A `recover: ${{ tasks.X.output }}` reference is **NOT an execution-order
edge** (the canonical example references a fallback source with no
`depends_on` · [03 §referencing carve-out](./03-dag.md#referencing-a-task-requires-an-explicit-depends_on)).
Resolution happens at **recovery time** ·

1. The failing task exhausts `retry:` · `on_error.recover` fires.
2. If the referenced task is already **terminal**, its value resolves
   immediately.
3. If it is still `pending`/`running`, the engine **awaits its terminal
   state** before resolving (deterministic · never a race · the DAG is
   finite so the await always terminates).
4. If it terminated without a usable value (`failure` · `cancelled` ·
   `skipped`), the reference is unresolved → `NIKA-VAR-001` → the recovery
   itself fails → the task fails as if `on_error:` were absent.

Authors SHOULD keep recovery sources cheap and independent (the
fetch-chain pattern · a local `nika:read` beside a live fetch).

### Examples

```yaml
# Use cached data on API failure
- id: api_call
  invoke: { tool: "nika:fetch", args: { url: "https://api.example.com/data" } }
  on_error:
    recover: ${{ tasks.cached_data.output }}   # a ${{ }} ref

# Use a default on error
- id: get_count
  invoke:
    tool: "mcp:db/count_users"
  on_error:
    recover: 0                                 # a literal

# Skip on error · downstream may handle
- id: optional_step
  exec: { command: "./optional.sh" }
  on_error:
    skip: true

- id: next
  depends_on: [optional_step]
  when: ${{ tasks.optional_step.status == 'success' }}   # only run if not skipped
  exec: { command: "..." }
```

---

## Structured output validation

The `infer:` and `agent:` verbs may declare a JSON Schema for structured output. If the model returns invalid JSON or fails schema validation, an error of category `validation_error` is raised.

The engine MAY auto-retry validation failures internally (transparent to the workflow) before surfacing the error. This behavior is engine-configurable.

```yaml
- id: extract
  infer:
    prompt: "Extract entities from · ${{ vars.text }}"
    schema:
      type: object
      required: [entities]
      properties:
        entities:
          type: array
          items:
            type: object
            properties:
              name: { type: string }
              type: { type: string, enum: [person, place, organization] }
  retry:
    max_attempts: 3            # retry on transient errors
  # validation failures may be retried internally · engine choice
```

---

## Workflow-level error semantics

If a task fails with no `on_error:` recovery · the **whole workflow is marked failed** and remaining tasks are cancelled (status `cancelled`).

A workflow's final state is one of ·

| State | Meaning |
|---|---|
| `success` | All tasks reached terminal state · no unrecovered failures |
| `failure` | At least one task failed with no recovery |
| `cancelled` | The workflow was cancelled (Ctrl+C · API call · etc.) |

The engine MUST emit a typed completion event with this state.

---

## Forward-compat

The error structure (fields · categories · namespaces · retry shape · on_error shape) is locked at v1. Additional categories MAY be added in minor bumps (additive only · existing categories never repurposed). Additional retry strategies MAY be added.

Out of scope for v0.1 · structured retry conditions (e.g. `retry_when: ${{ error.details.status_code == 503 }}`) · global on_error handlers · workflow-level circuit breakers. See [08-out-of-scope.md](./08-out-of-scope.md).

---

🦋 *Next · [06 · Stdlib contract](./06-stdlib-contract.md)*
