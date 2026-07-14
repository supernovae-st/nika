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

Error codes follow the format `NIKA-<NAMESPACE>-<NNN>` where namespace is 2-9 uppercase letters and NNN is a 3-digit zero-padded number. A code MAY add an **optional sub-namespace** for self-documentation · `NIKA-<NAMESPACE>-<SUB>-<NNN>` (4-segment), used per-builtin (`NIKA-BUILTIN-WAIT-001` · each builtin owns its own 001-099) or per-field (`NIKA-PARSE-WHEN-001` · the `when:` field of a parse error). The canonical regex is `^NIKA-[A-Z]{2,9}(-[A-Z][A-Z0-9_]{1,15})?-[0-9]{3}$` (also the `retry.on_codes` / `on_error.on_codes` validation pattern). The sub-namespace segment admits underscores so underscore-named builtins encode cleanly (`NIKA-BUILTIN-JSON_MERGE_PATCH-001`).

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
| `NIKA-BUILTIN-<BUILTIN>` | Builtin tool errors · per-builtin sub-namespace (`NIKA-BUILTIN-WAIT-001` · `NIKA-BUILTIN-NOTIFY-001` · `NIKA-BUILTIN-INSPECT-001` · `NIKA-BUILTIN-IMAGE_GENERATE-001..007` + `NIKA-BUILTIN-IMAGE_FX-001..006` — the §Media builtins' planes, stdlib page normative · `NIKA-BUILTIN-FETCH-001` — `nika:fetch`'s network/extraction errors, whose instances carry `category: network_error` though the namespace is the uniform `NIKA-BUILTIN`) | 001-099 per builtin |
| `NIKA-MCP` | MCP client errors | 001-099 |
| `NIKA-SEC` | Security policy violations (SSRF · blocklist) | 001-099 |
| `NIKA-TIMEOUT` | Task or step timeouts | 001-099 |
| `NIKA-TYPE` | Type core · contracts · lowering ([09-types.md](./09-types.md)) | 001-199 (001-099 static · 101+ runtime) |
| `NIKA-CANCEL` | Task or workflow cancellation | 001-099 |
| `NIKA-IMPL` | Engine internal errors | 001-099 |

A v0.1-compliant engine MUST use these namespaces for the canonical categories. New error codes MAY be added in minor bumps (additive · never repurposed).

### Concrete v0.1 codes · the normative floor

The codes below are **allocated**: a conformant engine emits exactly these
codes for these failures (it MAY add more within a namespace's range · never
repurpose). This closes the « placeholder » gap: a second engine matches
these from this file alone.

| Code | Failure | Category | `transient` |
|---|---|---|---|
| `NIKA-PARSE-001` | the YAML itself does not parse (syntax error) | `parse_error` | false |
| `NIKA-PARSE-002` | missing envelope field (`nika:` / `workflow:` / non-empty `tasks:`) | `validation_error` | false |
| `NIKA-PARSE-003` | `nika:` version marker is not exactly `v1` | `parse_error` | false |
| `NIKA-PARSE-004` | `workflow:` id violates `^[a-z][a-z0-9-]*$` | `validation_error` | false |
| `NIKA-PARSE-005` | unknown field — strict mode rejects anything outside the closed v1 set | `validation_error` | false |
| `NIKA-PARSE-006` | task id violates `^[a-z][a-z0-9_]*$` (snake_case · CEL-safe · no hyphens) | `validation_error` | false |
| `NIKA-PARSE-007` | duplicate task id within the workflow | `validation_error` | false |
| `NIKA-PARSE-008` | task declares no verb — exactly one of `infer`/`exec`/`invoke`/`agent` required | `validation_error` | false |
| `NIKA-PARSE-009` | task declares multiple verbs — exactly one required | `validation_error` | false |
| `NIKA-PARSE-010` | `timeout:` violates the quoted Go-duration contract (positive · max 24h · descending units) | `validation_error` | false |
| `NIKA-PARSE-011` | `retry:` block violates the spec shape (§retry below) | `validation_error` | false |
| `NIKA-PARSE-012` | `on_error:` block violates the spec shape (fields mutually exclusive) | `validation_error` | false |
| `NIKA-PARSE-013` | `with:`/`output:` binding uses a reserved name (`output` · `status` · `error` · `started_at` · `ended_at` · `duration_ms`) | `validation_error` | false |
| `NIKA-PARSE-014` | `secrets:` entry is not a store reference — inline literals forbidden ([01 §secrets](./01-envelope.md)) | `validation_error` | false |
| `NIKA-PARSE-015` | typed `vars:` declaration malformed (type in string/number/integer/boolean/array/object) | `validation_error` | false |
| `NIKA-PARSE-017` | duplicate mapping key — no silent last-wins | `validation_error` | false |
| `NIKA-PARSE-018` | missing required field in a verb body (`infer.prompt` · `exec.command` · `invoke.tool`) | `validation_error` | false |
| `NIKA-PARSE-019` | generic structural validation — wrong YAML shape for a field | `validation_error` | false |
| `NIKA-PARSE-020` | `workflow:` is a scalar — the envelope became an object (`workflow:` then `id:`) | `validation_error` | false |
| `NIKA-PARSE-021` | top-level `description:` — it moved into `workflow.description` | `validation_error` | false |
| `NIKA-PARSE-022` | `tasks:` is a sequence — it became a map keyed by task id | `validation_error` | false |
| `NIKA-PARSE-023` | a task carries an `id:` field — the map key IS the identity | `validation_error` | false |
| `NIKA-PARSE-024` | a task carries `depends_on:` — dead since W2 (data → `with:` bindings · control → `after:` predicates · `check --fix` migrates) | `validation_error` | false |
| `NIKA-PARSE-025` | `decode:` with `capture: structured` — that capture already IS an object (`{stdout, stderr, exit_code}`) · type the object with `returns:` instead | `validation_error` | false |
| `NIKA-DAG-001` | cycle in the precedence graph G_p = E_d ∪ E_c (incl. self-dependency · via `with:`/`after:`) | `validation_error` | false |
| `NIKA-DAG-002` | `with:`/`after:` references an undeclared task | `validation_error` | false |
| `NIKA-DAG-004` | `on_error.recover` references a task downstream of the declaring task (await would deadlock) | `validation_error` | false |
| `NIKA-DAG-005` | `after:` predicate outside the closed set (`succeeded` · `failed` · `skipped` · `terminal`) | `validation_error` | false |
| `NIKA-DAG-006` | statically dead task — an incoming edge’s pass-set excludes every reachable producer state, or the `when:` gate is false under every reachable upstream combination ([03 §gate algebra](./03-dag.md#the-gate-algebra-v2-normative)) | `validation_error` | false |
| `NIKA-DAG-007` | status compared against a literal outside the vocabulary (`success` · `failure` · `skipped` · `cancelled`) — `==` never matches, `!=` always holds | `validation_error` | false |
| `NIKA-TYPE-001` | unknown type name (in `types:` · `returns:` · an `outputs:` type) — did-you-mean when close | `validation_error` | false |
| `NIKA-TYPE-002` | recursive type reference — the `types:` graph must be acyclic | `validation_error` | false |
| `NIKA-TYPE-003` | `returns:` and `schema:` on the same task — one contract, one spelling | `validation_error` | false |
| `NIKA-TYPE-004` | `returns:` type unreachable from the declared `decode:` (an object contract over `decode: text` · …) | `validation_error` | false |
| `NIKA-TYPE-005` | a secret-carrying type in a lowered position (reserved with `secret<T>` · W4) | `security_error` | false |
| `NIKA-TYPE-006` | regex pattern outside the locked dialect (backreference · lookaround · named group · inline flags · lazy/possessive · `\b` · `\p` — [09 §the regex dialect](./09-types.md#the-regex-dialect-normative--locked)) | `validation_error` | false |
| `NIKA-TYPE-101` | run-time contract violation — the decoded value does not fit `returns:` (`exec:`/`invoke:` lane · `infer:`/`agent:` stay `NIKA-INFER-002`-class) | `validation_error` | false |
| `NIKA-VAR-001` | unresolved reference (unknown namespace entry · undeclared `env`/`vars` key) | `variable_error` | false |
| `NIKA-VAR-002` | binding cardinality — a jq binding emitted zero or multiple values (evaluation-time · data-dependent) | `variable_error` | false |
| `NIKA-VAR-003` | provably-invalid path into a declared `schema:` (static walk · [04](./04-variables.md)) | `validation_error` | false |
| `NIKA-VAR-004` | jq runtime error while evaluating a binding | `variable_error` | false |
| `NIKA-VAR-005` | static expression violation — outside the `cel-subset/0.1` grammar · chained relation · unknown function · statically-non-boolean `when:` root · jq compile error | `validation_error` | false |
| `NIKA-VAR-006` | expression type error at evaluation — cross-type compare · non-boolean `when:` value · `for_each` over a non-array | `variable_error` | false |
| `NIKA-VAR-007` | bytes value substituted into a string position | `variable_error` | false |
| `NIKA-VAR-008` | unclosed `${{` opener | `validation_error` | false |
| `NIKA-VAR-020` | bare `tasks.X` is the envelope, not a value — pick `.output` (closed projection set · 04 §namespaces) | `validation_error` | false |
| `NIKA-VAR-021` | a `tasks.*` reference outside the boundary (`with:` · `after:` · `on_error.recover` · `on_finally` parent-only · workflow `outputs:`) — hoist it into `with:` (`check --fix` applies it) | `validation_error` | false |
| `NIKA-VAR-009` | typed `outputs` value did not match its declared `type:` at run end (the output half of the callable contract · [01 §engine MUST](./01-envelope.md)) | `validation_error` | false |
| `NIKA-INFER-001` | provider call failed (HTTP error · provider refusal) | `provider_error` | engine-assessed |
| `NIKA-INFER-002` | structured output failed `schema:` validation (after any engine-internal retries) | `validation_error` | false |
| `NIKA-EXEC-001` | non-zero exit code (default capture modes · see [02 §exec](./02-verbs.md#exec--shell-command)) | `process_error` | false |
| `NIKA-EXEC-002` | spawn failure (command not found · permission) | `process_error` | false |
| `NIKA-INVOKE-001` | unknown tool (unresolvable `nika:`/`mcp:` id) | `validation_error` | false |
| `NIKA-INVOKE-002` | tool args failed the tool's schema | `validation_error` | false |
| `NIKA-AGENT-001` | `max_turns` exhausted before completion | `budget_error` | false |
| `NIKA-AGENT-002` | `max_tokens_total` exhausted before completion | `budget_error` | false |
| `NIKA-AGENT-003` | a `skills:` path does not resolve (file missing/unreadable at compose time · [02 §Agent Skills](./02-verbs.md#agent-skills--skills)) | `validation_error` | false |
| `NIKA-AGENT-004` | a `skills:` file is not a valid Agent Skill (no/unterminated/non-mapping frontmatter · missing/empty `name`/`description`) | `validation_error` | false |
| `NIKA-MCP-001` | MCP server not configured / not reachable at call time | `tool_error` | engine-assessed |
| `NIKA-MCP-002` | MCP tool call failed (transport · tool-side error) | `tool_error` | engine-assessed |
| `NIKA-SEC-001` | `exec:` blocklist hit | `security_error` | false |
| `NIKA-SEC-002` | agent tool call outside the `tools:` whitelist | `security_error` | false |
| `NIKA-SEC-003` | run-recursion bound — nested-run depth exceeded OR self-launching workflow | `security_error` | false |
| `NIKA-SEC-004` | effect outside the declared `permits:` capability boundary (fs/net/exec/tool · [01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)) | `security_error` | false |
| `NIKA-SEC-005` | SSRF block — a `nika:fetch`/`nika:notify` URL resolves to a loopback/private/link-local/metadata target (the always-on engine floor · independent of `permits:`, with ONE carve-out: an exact loopback literal in `permits.net.http` declassifies the floor for that host only · [01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)) | `security_error` | false |
| `NIKA-SEC-006` | secret flow — a `secrets.<name>` value reaches an unsanctioned sink (an `exec:` argument · an `invoke:` payload · an `infer:`/`agent:` prompt) · the diagnostic carries the **taint path** + the exact `egress:` clause that would sanction it ([10 §secret flow](./10-authority.md#secret-flow-refusals-carry-their-codes-normative) · rules in [01 §egress](./01-envelope.md#egress--optional--sanctioned-destinations-declassification)) | `security_error` | false |
| `NIKA-SEC-007` | secret egress — a tainted value reaches the workflow boundary (`outputs:` · where a result leaves the run) · the diagnostic carries the taint path ([10 §secret flow](./10-authority.md#secret-flow-refusals-carry-their-codes-normative)) | `security_error` | false |
| `NIKA-POLICY-001` | a hard `policy:` rule is violated (`require.human_gate_before` · `forbid.exec_after` · `allow.providers` · `limits.max_tasks`) — the diagnostic names rule + task + witness · check-time, before any token ([10 §policy](./10-authority.md#the-policy-block--optional--named-workflow-law)) | `security_error` | false |
| `NIKA-TIMEOUT-001` | task (or for_each iteration) exceeded `timeout:` | `timeout_error` | false |
| `NIKA-CANCEL-001` | task cancelled (workflow failure gate · user cancellation) | `cancelled` | false |
| `NIKA-BUILTIN-001` | builtin `invoke:` violates its statically-checkable arg contract (e.g. `nika:fetch` without `url:` · `nika:jq` arg shape) | `validation_error` | false |
| `NIKA-BUILTIN-DONE-001` | `nika:done` invoked outside an `agent:` loop | `validation_error` | false |


`NIKA-PARSE-016` is **retired** (never reuse): the jq-binding-contains-template
class folded into `NIKA-VAR-005` at the deep-conformance registry remap: the
allocation hole is deliberate, per the additive-never-repurposed rule above.

`NIKA-DAG-003` is **retired** (never reuse): « a `tasks.X` reference with no
declared edge » became INEXPRESSIBLE in W2 « the flow » — the `with:` binding
IS the edge (derived, never restated), and a reference outside the boundary
is `NIKA-VAR-021`. The allocation hole is deliberate.

### Taxonomy ownership · the spec table is normative · engines derive

**This table, not any engine's source code, owns the taxonomy.** A
conformant engine (the Rust reference included) *derives* its error types
from this section: every spec-relevant error it emits MUST carry a code
matching the canonical regex, in the namespace this table assigns to the
failure's scope, with the category semantics of §Categories. An engine MAY
keep richer internal error machinery (the reference engine's internal
diagnostics codes, subsystem-specific numbering, extra metadata). Internal
codes are **not** spec surface and MUST NOT leak into workflow-visible
errors (`tasks.X.error` · run reports · conformance output) in place of the
canonical form. Two consequences ·

1. **A second engine can be error-conformant from this file alone**: the
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
| `process_error` | `exec:` subprocess failure (non-zero exit · spawn) | false |
| `budget_error` | an `agent:` loop budget exhausted (`max_turns` · `max_tokens_total`) | false |
| `security_error` | SSRF · blocklist · capability denied | false |
| `timeout_error` | Task or step exceeded its timeout | false |
| `cancelled` | Workflow or task cancelled | false |
| `internal_error` | Engine bug · unexpected state | false |

---

## Retry policy

A task MAY declare a `retry:` block. Retries apply to **transient** errors only (`error.transient == true`).

### Syntax

```yaml
flaky_api:
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
`on_codes` lists canonical `NIKA-<NS>-<NNN>` codes (e.g. `NIKA-BUILTIN-FETCH-001`), not
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
api_call:
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

### Fields · exactly ONE action + an optional code filter

| Field | Effect | Downstream sees |
|---|---|---|
| `recover: <value>` | Use a recovery output: a `${{ }}` ref (e.g. another task's output) OR a literal | `status: success` · output = recover value |
| `skip: true` | Skip this task on error · **the original error stays readable** at `tasks.X.error` (status is `skipped` · error populated · the one state where both coexist · enables downstream per-code routing) | `status: skipped` · `error` = the original typed error |
| `fail_workflow: true` | Fail the whole workflow (default behavior) | n/a (workflow fails) |
| `on_codes: [<NIKA-…>]` | **Optional filter** (combinable with exactly one action above) · the action applies ONLY when the final error's `code` is listed · an unlisted code falls through to the default (fail) · the catch-side mirror of `retry.on_codes` (same regex) | per the action |

`recover:` merges the former `fallback:` (ref) + `value:` (literal) into one field
(`${{ }}` resolves to values either way · 4 modes → 3 · one way).

```yaml
# Catch-side routing · recover ONLY on timeout · any other code still fails
slow_fetch:
    invoke: { tool: "nika:fetch", args: { url: "https://slow.example.com" } }
    timeout: "30s"
    on_error:
      on_codes: [NIKA-TIMEOUT-001]
      recover: { stale: true, items: [] }
```

### `recover:` reference resolution (normative)

A `recover: ${{ tasks.X.output }}` reference is **NOT an execution-order
edge** — it is the *recovery* surface of the reference boundary
([04 §boundary](./04-variables.md#the-reference-boundary--where-tasks-may-appear) ·
projected as a `recovery` edge in `graph_format: 2`, which never schedules).
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

**Recovery × `output:` bindings (normative)** · when `recover:` fires, the
recovery value **substitutes the raw output BEFORE binding extraction**:
the task's `output:` jq bindings evaluate over the recovered value exactly
as they would over a verb response. Downstream consumers stay shape-stable
(`tasks.X.title` works whether the live call or the fallback produced the
data), which is why a recovery source SHOULD match the raw output's shape.
A binding that fails over the recovered shape errors as usual
(`NIKA-VAR-002` / `NIKA-VAR-004`) · the recovery does not mask it.

**Parse-time acyclicity rule (`NIKA-DAG-004` · `validation_error`)** · a
`recover:` reference to a task that **transitively depends on the declaring
task** (through G_p = E_d ∪ E_c) is rejected at parse time. At recovery time
such a task could never reach a terminal state (it is waiting on the failing
task): the step-3 await would deadlock. The recovery surface is exempt from
*scheduling-edge creation*, not from *acyclicity*.

Authors SHOULD keep recovery sources cheap and independent (the
fetch-chain pattern · a local `nika:read` beside a live fetch).

### Examples

```yaml
# Use cached data on API failure
api_call:
    invoke: { tool: "nika:fetch", args: { url: "https://api.example.com/data" } }
    on_error:
      recover: ${{ tasks.cached_data.output }}   # a ${{ }} ref

# Use a default on error
get_count:
    invoke:
      tool: "mcp:db/count_users"
    on_error:
      recover: 0                                 # a literal

# Skip on error · downstream may handle
optional_step:
    exec: { command: ["./optional.sh"] }
    on_error:
      skip: true

next:
    after: { optional_step: succeeded }     # strict gate · a skipped producer cancels this path
    exec: { command: ["..."] }
```

---

## Structured output validation

The `infer:` and `agent:` verbs may declare a JSON Schema for structured output. If the model returns invalid JSON or fails schema validation, an error of category `validation_error` is raised.

The engine MAY auto-retry validation failures internally (transparent to the
workflow) before surfacing the error (`NIKA-INFER-002`). This behavior is
engine-configurable: the SAME rule as [02 §infer conformance](./02-verbs.md#conformance)
(MAY · engine choice · the two sections state one contract).

```yaml
extract:
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

If a task fails with no `on_error:` recovery · the **workflow's final state
is `failure`**. What happens to the REST of the DAG is **gate-based · not a
blanket kill** ·

- **In-flight tasks drain** · an engine MUST NOT abort an unrelated running
  task because a sibling failed (industry default · GitHub Actions
  independent jobs · Argo running nodes).
- A not-yet-started task is admitted per **GATE-v2**
  ([03 §gate algebra](./03-dag.md#the-gate-algebra-v2-normative)): each of
  its edges checks the producer's settled state against that edge's
  pass-set. A value edge from the failed task does not admit → the consumer
  is `cancelled`, and the dead path propagates transitively.
- A task whose edges DO admit on failure still runs · `after: {x: failed}`
  (the failure path) · `after: {x: terminal}` (the **always-pattern**: a
  final notify/report task runs even in a failing workflow) · a
  `.status`/`.error` observation binding.
- The workflow's final state stays `failure` even when always-pattern tasks
  ran afterward (any unrecovered task failure decides it).
- **User cancellation** (Ctrl+C · API) IS a blanket kill · in-flight tasks
  are cancelled (their `on_finally:` still runs · [03](./03-dag.md#on_finally--optional--cleanup-hook--always-runs)).

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

Out of scope for v0.1 · structured retry conditions (e.g. `retry_when: ${{ error.details.status_code == 503 }}` · value-conditioned polling · see [08 H19](./08-out-of-scope.md#horizon-postures--the--did-you-think-of-x--table-2026-06-10)) · global on_error handlers (the always-pattern covers notification · §workflow-level semantics) · workflow-level circuit breakers. See [08-out-of-scope.md](./08-out-of-scope.md).

---

🦋 *Next · [06 · Stdlib contract](./06-stdlib-contract.md)*
