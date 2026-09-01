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

### Blame polarity (normative · when a refusal names a bound)

A refusal that trips a *bound* also names WHO set that bound. The
polarity is a vocabulary, kebab-case on the wire, carried by the
**refusal itself** — the typed error a verb raises when the bound is
hit, beside the reason the bound exists ·

| Polarity | Who is named |
|---|---|
| `by-the-value` | the VALUE violated the bound — a written literal the schema rejects |
| `by-the-caller` | the CALLER wrote the bound the failure tripped — the task's own `max_turns:`, exhausted |
| `by-the-contract` | NEITHER — the bound is a default this specification declares, applied to an absent key (`max_turns` at its [02](./02-verbs.md) default of 10): the refusal names the rule that declares it |

The third exists because the first two, alone, force a lie. A default
inserted by normalization belongs to no one present: blaming the value
accuses text nobody wrote, and blaming the caller accuses a key nobody
typed. So the polarity names the contract instead, and the author reads
where to go and change it.

The set is closed but **extensible by law** — a future rule may add a
polarity, so a consumer matches with a wildcard arm rather than
assuming three forever.

**Where it is spoken, measured 2026-08-13 · `nika 0.108.0`.** The
polarity is decided where the bound is *chosen*, not where it is
tripped: an `agent:` task declaring `max_turns:` gets `by-the-caller`
with the source « the task's own `max_turns:` », and one that omits it
gets `by-the-contract` with the source « spec 02-verbs.md §agent ·
`max_turns` default 10 ». That pairing — a polarity AND the rule that
set the bound — is what makes the refusal actionable.

Two surfaces do **not** carry it today and this text used to claim they
did: `nika check` speaks no polarity (it judges statically, before any
bound is consumed), and the run receipt has no blame field. The claim
belonged to the proposal that became this section and was carried
forward without being run. A conformant engine MAY widen the carrier;
what is normative is the vocabulary, the pairing with a source, and the
rule that a contract-set default is never imputed to a value nobody
wrote or a caller who typed nothing.

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
| `NIKA-YAML` | YAML profile refusals (anchors · aliases · merge keys · tags · caps · encoding — the R11 law set · [01 §YAML profile](./01-envelope.md#yaml-profile-normative) · pedagogical upgrades of the `NIKA-PARSE` floor) | 001-099 |
| `NIKA-COMP` | Composition (`invoke: workflow:` · [14-composition](./14-composition.md)) | 001-099 |
| `NIKA-DECIDE` | Decision bundles + evidence (`nika:decide` · [11-decision](./11-decision.md)) | 001-099 |
| `NIKA-PORT` | Gateway artifacts (deployment bundle · lowering · fidelity · [12-gateway](./12-gateway.md)) | 001-099 |
| `NIKA-ASSERT` | assertion obligations ([15-proof](./15-proof.md)) · ⚰️ the `assert:` ENVELOPE KEY was removed 2026-08-11 · the range stays RESERVED, unemitted, for the day a property returns as an addition | 001-099 |
| `NIKA-LOCK` | `nika.lock` pin violations ([15-proof](./15-proof.md)) | 001-099 |
| `NIKA-REG` | Registry client refusals (resolve · digest · advisory · [registry-v0.1](../registry/registry-v0.1.md) · allocated for the reference engine's `registry:` resolver — engine ADR-106) | 001-099 |
| `NIKA-AUTH` | Authority · the `permits:` boundary contract (absent-block refusal · literal bounds · taint into a permitted argument · dead grants · lift discipline · [10-authority](./10-authority.md)) | 001-099 |
| `NIKA-DEFAULT` | Declared defaults · a `default:` (or typed `const:`) value that violates its own declared type ([09-types](./09-types.md) · R3b) | 001-099 |
| `NIKA-DRIFT` | Declared-but-unused drift · advisory check hints, never audit failures (the reverse of the hard used-but-undeclared surface) | 001-099 |
| `NIKA-VALUES` | Value namespaces · the dead pre-flip `vars:`/`env:` forms + reads outside the three-authority family (`inputs` · `const` · `secrets` · [04 §values](./04-variables.md)) | 001-099 |

This table allocates every namespace in use; the machine registry
([`canon/diagnostics/registry.yaml`](../canon/diagnostics/registry.yaml))
carries the per-code rows. Two rows run **ahead of `canon.yaml`
`error_namespaces`** — `NIKA-YAML` (kernel-ahead · CF-05) and `NIKA-REG`
(engine ADR-106) — declared, never silent: `scripts/canon-projectors.py`
cross-checks this table against the canon list in both directions on every
gate run, with exactly those two as the declared skips. A v0.1-compliant engine MUST use these namespaces for the canonical categories. New error codes MAY be added in minor bumps (additive · never repurposed).

### Concrete v0.1 codes · the normative floor

The codes below are **allocated**: a conformant engine emits exactly these
codes for these failures (it MAY add more within a namespace's range · never
repurpose). This closes the « placeholder » gap: a second engine matches
these from this file alone.

| Code | Failure | Category | `transient` |
|---|---|---|---|
| `NIKA-PARSE-001` | the YAML itself does not parse (syntax error) | `parse_error` | false |
| `NIKA-PARSE-002` | missing envelope field (`nika:` · a non-empty `tasks:`) — a file with no `nika:` key is not a nika file; `workflow:` no longer exists to be missing ([01 §envelope](./01-envelope.md)) | `validation_error` | false |
| `NIKA-PARSE-003` | `nika:` is not a kebab-case id (`^[a-z][a-z0-9-]*$`) — the key carries the file's NAME, not a version; the version slot is gone forever and losslessly (there is no `nika: v2`, ever · [01 §nika](./01-envelope.md)) | `parse_error` | false |
| `NIKA-PARSE-004` | RESERVED — retired with the nine-key envelope (2026-08-12): the id moved onto `nika:` and `NIKA-PARSE-003` judges it; the number is never reused | `validation_error` | false |
| `NIKA-PARSE-005` | unknown field — strict mode rejects anything outside the closed v1 set | `validation_error` | false |
| `NIKA-PARSE-006` | task id violates `^[a-z][a-z0-9_]*$` (snake_case · CEL-safe · no hyphens) | `validation_error` | false |
| `NIKA-PARSE-007` | duplicate task id within the workflow | `validation_error` | false |
| `NIKA-PARSE-008` | task declares no verb — exactly one of `infer`/`exec`/`invoke`/`agent` required | `validation_error` | false |
| `NIKA-PARSE-009` | task declares multiple verbs — exactly one required | `validation_error` | false |
| `NIKA-PARSE-010` | `timeout:` violates the quoted Go-duration contract (positive · max 24h · descending units) | `validation_error` | false |
| `NIKA-PARSE-011` | `retry:` block violates the spec shape (§retry below) | `validation_error` | false |
| `NIKA-PARSE-012` | `on_error:` block violates the spec shape (fields mutually exclusive) | `validation_error` | false |
| `NIKA-PARSE-013` | `with:`/`extract:` binding uses a reserved name (`output` · `status` · `error` · `started_at` · `ended_at` · `duration_ms`) | `validation_error` | false |
| `NIKA-PARSE-014` | `secrets:` entry is not a store reference — inline literals forbidden ([01 §secrets](./01-envelope.md)) | `validation_error` | false |
| `NIKA-PARSE-017` | duplicate mapping key — no silent last-wins | `validation_error` | false |
| `NIKA-PARSE-018` | missing required field in a verb body (`infer.prompt` · `exec.command` · `invoke.tool`) | `validation_error` | false |
| `NIKA-PARSE-019` | generic structural validation — wrong YAML shape for a field | `validation_error` | false |
| `NIKA-PARSE-022` | `tasks:` is a sequence — it became a map keyed by task id | `validation_error` | false |
| `NIKA-PARSE-023` | a task carries an `id:` field — the map key IS the identity | `validation_error` | false |
| `NIKA-PARSE-024` | a task carries `depends_on:` — dead since W2 (data → `with:` bindings · control → `after:` predicates · `check --fix` migrates) | `validation_error` | false |
| `NIKA-PARSE-025` | `decode:` with `capture: structured` — that capture already IS an object (`{stdout, stderr, exit_code}`) · type the object with `returns:` instead | `validation_error` | false |
| `NIKA-PARSE-026` | a declared `entropy` × `clock` contradiction — `ambient × virtual` cannot be both live and reproducible ([01 §determinism](./01-envelope.md)) | `validation_error` | false |
| `NIKA-PARSE-027` | the other declared contradiction — an `entropy: none` or `seeded` run declaring `clock: system`: a reproducible run cannot read the wall clock ([01 §determinism](./01-envelope.md)) | `validation_error` | false |
| `NIKA-PARSE-028` | `entropy: none` while a structural randomness source is consumed (a live retry `jitter` · `nika:uuid`) — the claim and the run disagree ([01 §determinism](./01-envelope.md)) | `validation_error` | false |
| `NIKA-COMP-001` | an `invoke: workflow:` target is not statically resolvable (templated · malformed · unpinned registry ref · [14 §form](./14-composition.md#the-form-normative)) | `validation_error` | false |
| `NIKA-COMP-002` | the child workflow's effect boundary exceeds `Authority(parent) ∩ declared` ([14 laws 3/4](./14-composition.md#the-ten-laws-normative--g22--constitution-103)) | `security_error` | false |
| `NIKA-COMP-003` | the static call graph is not acyclic (self-launch · cycle · [14 law 7](./14-composition.md#the-ten-laws-normative--g22--constitution-103) · `NIKA-SEC-003` is the runtime backstop) | `validation_error` | false |
| `NIKA-COMP-004` | the typed call does not compose (args ⋢ inputs, or outputs ⋢ returns · [14 law 2](./14-composition.md#the-ten-laws-normative--g22--constitution-103)) | `validation_error` | false |
| `NIKA-DAG-001` | cycle in the precedence graph G_p = E_d ∪ E_c (incl. self-dependency · via `with:`/`after:`) | `validation_error` | false |
| `NIKA-DAG-002` | `with:`/`after:` references an undeclared task | `validation_error` | false |
| `NIKA-DAG-004` | `on_error.recover` references a task downstream of the declaring task (await would deadlock) | `validation_error` | false |
| `NIKA-DAG-005` | `after:` predicate outside the closed set (`success` · `failure` · `skipped` · `terminal`) | `validation_error` | false |
| `NIKA-DAG-006` | statically dead task — an incoming edge’s pass-set excludes every reachable producer state, or the `when:` gate is false under every reachable upstream combination ([03 §gate algebra](./03-dag.md#the-gate-algebra-v2-normative)) | `validation_error` | false |
| `NIKA-DAG-007` | status compared against a literal outside the vocabulary (`success` · `failure` · `skipped` · `cancelled`) — `==` never matches, `!=` always holds | `validation_error` | false |
| `NIKA-DAG-008` | a `${{ group.<name> }}` fold names a group **no task declares** — including a bare `${{ group }}`, and including the group left empty by a renamed member. Membership is declared, never matched, precisely so a rename is an error here instead of a silently smaller fold ([03 §group](./03-dag.md#declared-membership-never-a-pattern-normative)) | `validation_error` | false |
| `NIKA-DAG-009` | an `unwind` task declares `group:` — cleanup is an `E_f` attachment that never enters `G_p`, so a `fan-in` edge from it would have no wave to schedule against ([03 §group](./03-dag.md#the-rest-of-the-contract-1)) | `validation_error` | false |
| `NIKA-TYPE-001` | unknown type name (in `returns:` · an `inputs:`/`outputs:` type) — with named `types:` retired, a PascalCase name in type position resolves to nothing and the refusal teaches the inline form | `validation_error` | false |
| `NIKA-TYPE-003` | `returns:` and `schema:` on the same task — one contract, one spelling | `validation_error` | false |
| `NIKA-TYPE-004` | `returns:` type unreachable from the declared `decode:` (an object contract over `decode: text` · …) | `validation_error` | false |
| `NIKA-TYPE-005` | a secret-carrying type in a lowered position (reserved with `secret<T>` · W4) | `security_error` | false |
| `NIKA-TYPE-006` | regex pattern outside the locked dialect (backreference · lookaround · named group · inline flags · lazy/possessive · `\b` · `\p` — [09 §the regex dialect](./09-types.md#the-regex-dialect-normative--locked)) | `validation_error` | false |
| `NIKA-TYPE-101` | run-time contract violation — the decoded value does not fit `returns:` (`exec:`/`invoke:` lane · `infer:`/`agent:` stay `NIKA-INFER-002`-class) | `validation_error` | false |
| `NIKA-DEFAULT-001` | a `default:` that does not conform to its own `type:` — the declaration would hand a caller a value its type forbids ([09 §types](./09-types.md)) | `validation_error` | false |
| `NIKA-VAR-001` | unresolved reference (unknown namespace entry · undeclared `inputs`/`const`/`secrets`/`with` key) | `variable_error` | false |
| `NIKA-VAR-002` | binding cardinality — a jq binding emitted zero or multiple values (evaluation-time · data-dependent) | `variable_error` | false |
| `NIKA-VAR-003` | provably-invalid path into a declared `schema:` (static walk · [04](./04-variables.md)) | `validation_error` | false |
| `NIKA-VAR-004` | jq runtime error while evaluating a binding | `variable_error` | false |
| `NIKA-VAR-005` | static expression violation — outside the `cel-subset/0.1` grammar · chained relation · unknown function · statically-non-boolean `when:` root · jq compile error | `validation_error` | false |
| `NIKA-VAR-006` | expression type error at evaluation — cross-type compare · non-boolean `when:` value · `for_each` over a non-array | `variable_error` | false |
| `NIKA-VAR-007` | bytes value substituted into a string position | `variable_error` | false |
| `NIKA-VAR-008` | unclosed `${{` opener | `validation_error` | false |
| `NIKA-VAR-020` | bare `tasks.X` is the envelope, not a value — pick `.output` (closed projection set · 04 §namespaces) | `validation_error` | false |
| `NIKA-VAR-021` | a `tasks.*` reference outside the boundary (`with:` · `after:` · `on_error.recover` · an `unwind` task reading its producer · workflow `outputs:`) — hoist it into `with:` (`check --fix` applies it) | `validation_error` | false |
| `NIKA-VAR-009` | typed `outputs` value did not match its declared `type:` at run end (the output half of the callable contract · [01 §engine MUST](./01-envelope.md)) | `validation_error` | false |
| `NIKA-VALUES-001` | the pre-flip envelope `vars:` block — dead since the E-split; classify each use into the authority its role commands ([04 §values](./04-variables.md)) | `validation_error` | false |
| `NIKA-VALUES-002` | the pre-flip envelope `env:` block — dead since the E-split; a deployment knob is an `inputs:` entry with `required: false`, `exec.env` is one subprocess's OS environment ([04 §values](./04-variables.md)) | `validation_error` | false |
| `NIKA-VALUES-003` | a value-namespace read outside the three-authority family (`inputs` · `const` · `secrets`) (`${{ params.X }}` and friends) ([04 §values](./04-variables.md)) | `validation_error` | false |
| `NIKA-INFER-001` | provider call failed (HTTP error · provider refusal) | `provider_error` | engine-assessed |
| `NIKA-INFER-003` | the provider reported no token usage for a priced model — the ledger cannot bill the call honestly (fail-closed · the usage-absence gate, R3-F1) | `validation_error` | false |
| `NIKA-INFER-002` | structured output failed `schema:` validation (after any engine-internal retries) | `validation_error` | false |
| `NIKA-INFER-004` | the provider spent tokens yet the visible answer is empty — a thinking model ate the budget on its reasoning trace (fail-closed · raise `max_tokens` or use a no-think variant) | `validation_error` | false |
| `NIKA-INFER-003` | the provider reported no token usage for a priced model — the ledger cannot bill the call honestly, so the call fails closed (the usage-absence gate · R3-F1) | `validation_error` | false |
| `NIKA-EXEC-001` | non-zero exit code (default capture modes · see [02 §exec](./02-verbs.md#exec--shell-command)) | `process_error` | false |
| `NIKA-EXEC-002` | spawn failure (command not found · permission) | `process_error` | false |
| `NIKA-INVOKE-001` | unknown tool (unresolvable `nika:`/`mcp:` id) | `validation_error` | false |
| `NIKA-INVOKE-002` | tool args failed the tool's schema | `validation_error` | false |
| `NIKA-AGENT-001` | `max_turns` exhausted before completion | `budget_error` | false |
| `NIKA-AGENT-002` | `max_tokens_total` exhausted before completion | `budget_error` | false |
| `NIKA-AGENT-003` | a `skills:` path does not resolve (file missing/unreadable at compose time · a `${{ }}` template or a glob, refused at parse) — judged only INSIDE the declared read boundary: an ungranted path is `NIKA-AUTH-006` / `NIKA-SEC-004` before the file is opened ([02 §Agent Skills](./02-verbs.md#agent-skills--skills)) | `validation_error` | false |
| `NIKA-AGENT-004` | a `skills:` file is not a valid Agent Skill (no/unterminated/non-mapping frontmatter · missing/empty `name`/`description`) | `validation_error` | false |
| `NIKA-AGENT-005` | the provider reported no token usage for a priced model — every budget and ledger reads the turn as free, so the loop fails closed (the usage-absence gate · R3-F1) | `budget_error` | false |
| `NIKA-MCP-001` | MCP server not configured / not reachable at call time | `tool_error` | engine-assessed |
| `NIKA-MCP-002` | MCP tool call failed (transport · tool-side error) | `tool_error` | engine-assessed |
| `NIKA-SEC-001` | `exec:` blocklist hit | `security_error` | false |
| `NIKA-SEC-002` | agent tool call outside the `tools:` whitelist | `security_error` | false |
| `NIKA-SEC-003` | run-recursion bound — nested-run depth exceeded OR self-launching workflow | `security_error` | false |
| `NIKA-SEC-004` | effect outside the declared `permits:` capability boundary (fs/net/exec/tool · [01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)) · ⚠️ **`env` is the fifth category since D-2026-08-11-N22 and this code does NOT yet cover it** — measured 2026-08-11 on the shipped binary: an in-process expression reads the ambient environment under an absent `permits:` block and under an explicit empty `permits.env`, and the static check reports the body as pure compute. Stated as an obligation, not a guarantee: a spec that listed `env` here before the engine enforced it would be the parallel-list defect this vocabulary exists to prevent · ✅ **the measured hole is CLOSED since 2026-08-15** — re-measured that day on the same three authority shapes (absent block · explicit empty `permits.env` · granted `permits.env`): all three now REFUSE at `nika check` with `NIKA-VAR-005`, naming the class the program reached for. The remedy is D-2026-08-11-N26's SUBTRACTION, not coverage by this code: `env` is withheld from the function set every jq seam compiles with, so the program never becomes runnable. The first half of the ⚠️ therefore STANDS — this code still does not make `env` a boundary category — while the evidence sentence behind it no longer holds | `security_error` | false |
| `NIKA-SEC-005` | SSRF block — a `nika:fetch`/`nika:notify` URL resolves to a loopback/private/link-local/metadata target (the always-on engine floor · independent of `permits:`, with ONE carve-out: an exact loopback literal in `permits.net.http` declassifies the floor for that host only · [01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)) | `security_error` | false |
| `NIKA-SEC-006` | secret flow — a `secrets.<name>` value reaches an unsanctioned sink (an `exec:` argument · an `invoke:` payload · an `infer:`/`agent:` prompt) · the diagnostic carries the **taint path** + the exact `egress:` clause that would sanction it ([10 §secret flow](./10-authority.md#secret-flow-refusals-carry-their-codes-normative) · rules in [01 §egress](./01-envelope.md#egress--optional--sanctioned-destinations-declassification)) | `security_error` | false |
| `NIKA-SEC-007` | secret egress — a tainted value reaches the workflow boundary (`outputs:` · where a result leaves the run) · the diagnostic carries the taint path ([10 §secret flow](./10-authority.md#secret-flow-refusals-carry-their-codes-normative)) | `security_error` | false |
| `NIKA-SEC-008` | a `nika:fetch` whose RESOLVED URL path names a data-as-code class — matched case-insensitively on the final extension; the diagnostic names the class and both repairs ([10 §authority](./10-authority.md)) | `security_error` | false |
| `NIKA-SEC-009` | lethal trifecta complete — the declared boundary grants private read (`fs.read` non-empty) + untrusted ingress (a `nika:fetch` builtin invoked · an `mcp:*` tool invoked · an `agent:` whose whitelist admits ingress · **v2.2: an `exec:` task, `permits.exec` itself the ingress channel**) + external egress (`net.http` non-empty · an escaping `fs.write` glob · `exec` enabled), the untrusted content **reaches** an egress-capable task's effect surface (a realized flow), and no blocking `invoke: nika:prompt` (no `default:`) dominates it · one finding per ungated tainted egress task, witness-named `source → sink` (NEP-0002 v2.0 · the Rule of Two as a static check · SEC-008 stays allocated to the AUTH plane) | `security_error` | false |
| `NIKA-SEC-010` | the approval-capability law is violated (NEP-0013 · the 6th invariant) — a rate-limited approval burst (`approval.rate_limited` · the N+1th distinct mint of a run, never a queue) · an approval whose resolved content hash differs from the shown hash (`approval.content_mismatch`) · a ticket replayed outside its run/step/hash scope (`approval.scope_mismatch`) · or the static heterogeneous-batch refusal (one prompt whose descendant closure unleashes two or more of `exec · write · net`) | `security_error` | false |
| `NIKA-SEC-011` | preview-commit divergence — the commit digest recomputed at the sink over the exact bytes about to fire differs from the preview digest judged at resolution (one bit of rendered argv · a permuted context field · a mutated tool argument) · the step refuses fail-closed and the receipt carries `divergence: {preview, commit}` — judged = executed at the action scale (NEP-0015 · F-P6) | `security_error` | false |
| `NIKA-SEC-012` | unordered shared writes — two tasks incomparable in the DAG closure whose literal `nika:write`/`nika:edit` paths collide with no ordering edge (`after:` · `with:`) to serialize them, or a `for_each` fan writing one constant path · parallelism is safe exactly where the writes are provably disjoint (NEP-0014 law 1 · F-P15) | `security_error` | false |
| `NIKA-SEC-014` | the affirmative-consent law — a confirm-mode human gate (`invoke: nika:prompt` · mode absent or `confirm`) reaches an egress-capable task over a route no affirmative gate closes: a REFUSED confirm settles success with value `false`, so a bare `after: { gate: success }` edge, a `when:` that never reads the answer, and a `when:` provably true on the refusal all let the effect through · the gate is credited only when every route consumes the answer and proves false on it (the Kleene-falsifiable `when:` · `when: false` · a closer confirm gate owns its closure) · an undecidable gate defers to the advisory hint, never a refusal ([10 §the affirmative-consent law](./10-authority.md#the-affirmative-consent-law--normative--nep-0020) · NEP-0020 · P0-2) | `security_error` | false |
| `NIKA-SEC-015` | the order law — an `exec:` task sits transitively downstream of a net-effecting task (`nika:fetch` · `nika:notify`) over the derived graph (`with:` data edges ∪ `after:` control edges): content the workflow did not author must not reach a shell · the diagnostic names the PATH, which is the witness · **UNCONDITIONAL** — no block declares it and none can disable it ([10 §the unconditional laws](./10-authority.md)) | `security_error` | false |
| `NIKA-AUTH-006` | an absent `permits:` block — DeclaredPermits is empty, so every effect the body requires is refused before any token; the diagnostic carries the inferred block, ready to paste ([10 §authority](./10-authority.md)) | `security_error` | false |
| `NIKA-AUTH-007` | an interpolation reaches a permit BOUND (a host · glob · program · env name inside `permits:`) — a bound must be a literal, or the boundary is self-serve ([10 §authority](./10-authority.md)) | `security_error` | false |
| `NIKA-AUTH-008` | an untrusted value reaches a permitted verb's ARGUMENT and its canonical resolved form escapes the step's permit — the diagnostic carries the taint path ([10 §authority](./10-authority.md)) | `security_error` | false |
| `NIKA-AUTH-009` | an `env:` entry naming one of the engine's dangerous variables — an inert dead grant, flagged rather than silently passed ([01 §permits](./01-envelope.md)) | `security_error` | false |
| `NIKA-AUTH-011` | a `lift:` entry whose named law would not have fired on this task — a trapdoor that lifts nothing is refused, never a silent no-op, because a lift's whole value is being countable and reviewable ([10 §the authored doors](./10-authority.md)) | `validation_error` | false |
| `NIKA-AUTH-010` | a `net.http` entry carrying a `*` anywhere but as the whole bare-`*` entry — hosts are exact names, and the bare `*` stays the explicit, visible escape ([01 §permits](./01-envelope.md)) | `security_error` | false |
| `NIKA-DECIDE-001` | the decision bundle is malformed or violates its own laws (float weight · undeclared evidence key in rules · identity key feeding a technical dimension · missing contradictory fixture · monotonicity violated by the bundle's own fixtures · [11 §nika:decide](./11-decision.md#nikadecide--the-deterministic-kernel-as-a-builtin)) | `validation_error` | false |
| `NIKA-DECIDE-002` | the evidence snapshot does not satisfy the bundle's evidence schema (type misfit · unauthorized source · integrity below the declared floor · undeclared key · [11 §evidence IR](./11-decision.md#evidence-ir-normative--g14)) | `validation_error` | false |
| `NIKA-PORT-001` | a gateway artifact (deployment bundle · capabilities report · lowering report · fidelity report · authority delta) is malformed or violates its laws (an unknown promoted · a `permissive_unsafe` row without refusal · the disclosure ⊆-chain violated · a child authority exceeding its parent · [12 §errors](./12-gateway.md#errors-the-nika-port-namespace--new-in-this-chapter)) | `validation_error` | false |
| `NIKA-PORT-002` | authority lowering is `permissive_unsafe` — the backend would allow what the declared boundary forbids · refused with the divergence witness ([12 §ExecutionBackend](./12-gateway.md#executionbackend--the-enforcement-contract-normative)) | `security_error` | false |
| `NIKA-TIMEOUT-001` | task (or for_each iteration) exceeded `timeout:` | `timeout_error` | false |
| `NIKA-CANCEL-001` | task cancelled (workflow failure gate · user cancellation) | `cancelled` | false |
| `NIKA-ASSERT-001` | ⚰️ RESERVED, not emitted in v0.1 (the envelope key it judged was removed 2026-08-11) · an assertion claims a level the evidence does not support (a `StaticProof` the IR cannot decide · a mis-leveled obligation · [15 §assert](./15-proof.md#assert--the-authors-obligations-normative)) | `validation_error` | false |
| `NIKA-LOCK-001` | a dependency resolved that `nika.lock` does not pin, or a hand-edited lock digest does not match ([15 §lock](./15-proof.md#nikalock--the-single-lock-normative--f7)) | `validation_error` | false |
| `NIKA-BUILTIN-001` | builtin `invoke:` violates its statically-checkable arg contract (e.g. `nika:fetch` without `url:` · `nika:jq` arg shape) | `validation_error` | false |
| `NIKA-BUILTIN-DONE-001` | `nika:done` invoked outside an `agent:` loop | `validation_error` | false |
| `NIKA-DRIFT-001` | declared-but-unused — an `inputs:`/`const:`/`secrets:` name or a `permits:` entry (exec program · tool glob · net host · fs path) that nothing in the body references · **advisory check hint — never fails the audit** (the report's `is_clean` ignores hints; dead declarations are smell, not failure) · the reverse direction (used-but-undeclared) is the hard `NIKA-VAR-001`/`NIKA-DAG-002`/`NIKA-SEC-004` surface, so **no `NIKA-DRIFT-002` exists** (an unemittable code would be dead weight — the no-duplication law is structural: hard codes name references, drift names declarations, never the same yaml site) · a dynamic consumer POISONS the used set (a shell-form exec hides its programs · an exec child hides the fs path sets · a dynamic URL/path hides the host/path set · an `agent:` whitelist dispatches dynamically — glob ⊆ glob is undecidable) and the category stays silent rather than risk a false positive · the fs read set DOES model the two decidable runtime gates (`nika:glob`'s literal walk root · a `nika:fetch` `multipart:` file part's literal path) · reference implementation nika#661 | `validation_error` | false |


`NIKA-PARSE-015` is **retired** (never reuse): the typed-`vars:` 6-enum class
died with the E-split (R3b · LAW-GRAMMAR-0211) — the `type:` field of
`inputs:`/`outputs:` declarations speaks the full TypeExpr of
[09-types](./09-types.md), so what PARSE-015 refused (the rich forms) is
admitted, and what stays outside the grammar refuses via `NIKA-TYPE-001`.
The allocation hole is deliberate, per the additive-never-repurposed rule above.

`NIKA-PARSE-020` · `NIKA-PARSE-021` are **retired** (never reuse): the
envelope key `workflow:` died entirely at the envelope nuke (2026-08-12). It
existed only to house `id:` and `description:`; the description died the same
day (**one consumer across five reading surfaces**, and the `semantic_hash`
came back byte-identical with it and without it) and the id moved onto
`nika:`, so the object had nothing left to hold. Both migration teachings lost
their destination. The allocation holes are deliberate.

`NIKA-TYPE-002` is **retired** (never reuse): « the `types:` graph must be
acyclic » has no object — named types died with the `types:` block
([09-types](./09-types.md)). A type expression is self-contained, so there is
no graph left to cycle.

`NIKA-DEFAULT-002` is **retired** (never reuse): « a `config:` entry with no
`default:` » has no object — the `config:` block died at the envelope nuke and
a deployment knob is now an `inputs:` entry with `required: false`, which
`--var` can reach.

`NIKA-POLICY-001` and `NIKA-SEC-013` are **retired** (never reuse), and the
whole `NIKA-POLICY` **namespace** with them: the `policy:` block died at the
envelope nuke. Measured, it was **fail-open** — a human gate with no `policy:`
block passed, the same gate carrying an unrelated clause refused; on the
corpus, 26 gates escaped the endorsement rule and the 8 punished were the ones
that had declared something else. A law that binds only the file that opts
into it binds nothing. The surviving half is `NIKA-SEC-015`, which fires with
no declaration at all.

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

> **`retry:` and `on_error:` are two STAGES, not two answers (normative ·
> the reason they stay two fields).** They look like one decision — « what
> happens when this task fails » — and the merged spelling
> (`on_error: { retry: …, then: … }`) has been proposed more than once. It is
> refused, on two grounds ·
>
> 1. **They fire at different times.** `retry:` governs the attempt loop and
>    fires on the FIRST failure; `on_error:` fires exactly once, on the LAST
>    one, after `retry:` is exhausted (§`on_error:` below · §recover
>    resolution step 1). Nesting a « keep trying » block inside a field named
>    *on error* would put the two on the same clock in the reader's head, and
>    they are not.
> 2. **The corpus does not pay for it.** Measured 2026-08-12 over every
>    `*.nika.yaml` in the studio (873 files · 3 093 tasks · walked from disk,
>    not grepped) · `on_error:` alone **240** tasks · `retry:` alone **41** ·
>    **both on one task 31**. Co-occurrence is **31/312 = 9.9 %** of the
>    tasks that carry either. A merge would fold 31 sites and hand the other
>    281 an indentation level they have no sibling to share — including 41
>    `retry:`-only tasks that would gain a wrapper named after an event they
>    are trying to avoid. That is ceremony, and the number says so.
>
> Both stages govern the **verb run only**. Neither is consulted for a
> boundary failure — a `with:` binding that fails to materialize, or a `when:`
> that fails to evaluate, settles the task `failure` with the armor bypassed
> ([03 §the dispatch pipeline](./03-dag.md#the-gate-algebra-v2-normative)).

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
```

### Fields · exactly ONE action + an optional code filter

| Field | Effect | Downstream sees |
|---|---|---|
| `recover: <value>` | Use a recovery output: a `${{ }}` ref (e.g. another task's output) OR a literal | `status: success` · output = recover value |
| `skip: true` | Skip this task on error · **the original error stays readable** at `tasks.X.error` (status is `skipped` · error populated · the one state where both coexist · enables downstream per-code routing) | `status: skipped` · `error` = the original typed error |
| `on_codes: [<NIKA-…>]` | **Optional filter** (combinable with exactly one action above) · the action applies ONLY when the final error's `code` is listed · an unlisted code falls through to the default (fail) · the catch-side mirror of `retry.on_codes` (same regex) | per the action |

`recover:` merges the former `fallback:` (ref) + `value:` (literal) into one field
(`${{ }}` resolves to values either way · 4 modes → 3 · one way).

> ⚰️ **`fail_workflow: true` is REMOVED (2026-08-11) · 3 modes → 2.** It was a
> keyword whose entire meaning was *"do the default"* — this spec said so in its
> own three voices: the commented example read *"explicit · same as no
> `on_error`"*, the table entry read *"(default behavior)"*, and combined with
> `on_codes:` **both branches fail**, since an unlisted code *"falls through to
> the default (fail)"*. A no-op in every combination is a comment wearing a
> keyword's syntax; write a YAML comment. **The consolidation above had already
> been run once and stopped one short** — three ways to spell one action is
> still two too many when one of them does nothing. An author who wants the
> default omits `on_error:`; an author who wants to SAY they chose it writes
> `# fails the workflow · deliberate`. Measured cost before removal: 2 files in
> the whole corpus, zero of them a real workflow.

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
projected as a `recovery` edge in `graph_format: 3`, which never schedules).
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

**Recovery × `extract:` bindings (normative)** · when `recover:` fires, the
recovery value **substitutes the raw output BEFORE binding extraction**:
the task's `extract:` jq bindings evaluate over the recovered value exactly
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
    after: { optional_step: success }       # strict gate · a skipped producer cancels this path
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
      prompt: "Extract entities from · ${{ inputs.text }}"
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
- A task whose edges DO admit on failure still runs · `after: {x: failure}`
  (the failure path) · `after: {x: terminal}` (the **always-pattern**: a
  final notify/report task runs even in a failing workflow) · a
  `.status`/`.error` observation binding.
- The workflow's final state stays `failure` even when always-pattern tasks
  ran afterward (any unrecovered task failure decides it).
- **User cancellation** (Ctrl+C · API) IS a blanket kill · in-flight tasks
  are cancelled (the tasks that `unwind` them still run · [03](./03-dag.md#unwind--a-settle-state-on-after--cleanup-that-always-runs)).

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
