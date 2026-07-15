# 14 · Composition

> A workflow can call another workflow — as a **typed, bounded,
> authority-contained** step, not as a shell-out. The form is a tagged
> union on the verb you already know: `invoke:` carries **exactly one**
> of `tool:` or `workflow:`. No fifth verb, no magic `nika:workflow`
> builtin, no path-shaped `target:` string — the field IS the
> semantics, the same command/shell law generalized.
>
> **A word of discipline.** Everything below is the CONTRACT. An engine
> may say a composition is *specified* the moment it parses and checks;
> it may only say *proven* once it has demonstrated, together, on real
> child execution: static resolution · typed I/O · effect containment ·
> inherited budgets · cycle detection · the trace forest · nested
> receipts · semantic resume · and reference-model parity. Until that
> whole receipt exists, « proven » is not a word this contract lets an
> implementation use.

---

## The form (normative)

```yaml
tasks:
  audit:
    invoke:
      workflow: "./audits/site-audit.nika.yaml"   # a STATIC path …
      args:                                        # … or registry:owner/name@version (pinned)
        url: "${{ vars.target }}"
    returns: AuditReport      # the child's typed outputs — composed, not re-declared
```

- `invoke:` is a **tagged union** — exactly one of `tool:` | `workflow:`,
  plus `args:`. Both present, or neither, is a validation error
  (`NIKA-PARSE` · the same shape as two verbs on one task).
- `workflow:` is a **static target**: a filesystem path OR a pinned
  `registry:owner/name@version`. A `${{ }}`-templated target is refused
  at check — a call graph you cannot draw before the run is a call
  graph you cannot bound (`NIKA-COMP-001`).

## The CallableContract (normative · G20)

Everything invocable — a builtin, an MCP tool, a local child, a
registry child, a pure decision, a structured human interaction — has
ONE shape:

```
Callable<I, O, E, ε, ρ, δ>
   I  inputs (typed)      O  outputs (typed)     E  error surface
   ε  effects             ρ  resources           δ  the guarantee levels
```

A child's contract is **DERIVED from its body, never annotated**: its
`I`/`O` are the child's `inputs:`/`outputs:` ([09](./09-types.md)), its
`ε` the inferred effect boundary ([10](./10-authority.md)), its `ρ` the
symbolic certificate ([05](./05-errors.md)), its `E` the codes its body
can raise. The parent's checker consumes the contract; the parent never
restates it. One catalog serves every surface (checker · runtime ·
agent · LSP · MCP · registry · lockfile) — the partial-view drift of
today's separate invoke/agent/LSP tables ends here.

## The ten laws (normative · G22 · constitution §10.3)

A composition is well-formed iff **all ten** hold:

1. **Static target** — the callee is resolvable before the run (path or
   pinned registry ref · never templated).
2. **Typed call** — the parent's `args:` fit the child's `inputs:` and
   the child's `outputs:` fit the parent's `returns:` (the assignable
   relation of [09](./09-types.md) · one type core, composed).
3. **Zero implicit authority** — `Authority(child) ⊆ Authority(parent)
   ∩ declared`. A child never gains a capability the parent lacks, and
   never more than the call-site declares. Import-side, this is the
   containment law of [12](./12-gateway.md).
4. **Effect containment** — the child's effect boundary is a subset of
   the parent's; a child effect outside it is `NIKA-COMP-002` at check,
   `NIKA-SEC-004` at run.
5. **Resources summed** — the child's symbolic certificate composes into
   the parent's (`Bound(parent) ⊇ Bound(parent-own) + Bound(child)` —
   two `Bound`s sum to a `Bound`, the algebra closes).
6. **Inherited deadlines/budgets** — the child runs under `min(parent
   remaining, child declared)` for time and cost; a child cannot outlive
   or outspend its caller.
7. **Acyclic call graph** — the static call graph is acyclic; a literal
   self-launch or a static cycle is refused at check (`NIKA-COMP-003`).
   The runtime depth cap (`NIKA-SEC-003`) is **defense in depth**, not
   the primary guard — a templated target that could recurse is
   bounded there, fail-closed.
8. **Trace forest** — the child keeps its OWN hash-chain; the parent's
   trace records the child's `{semantic_hash, plan_id, trace_id,
   chain_head, outcome}` (the [13](./13-outcomes.md) Outcome, composed)
   — a forest of chains, never one flattened stream.
9. **Composed receipts** — nested receipts compose by Merkle: the
   parent's receipt commits to the child's receipt digest, so a proof
   of the whole contains a proof of each part.
10. **Semantic cache identity** — a child result is cached by the
    child's **semantic identity** (its canonical Semantic IR · W6),
    never by its path or the call site — two call sites that resolve to
    the same semantic child share the cache; the same path resolving to
    two different bodies does not.

## Agent capability closure (normative · G25)

An `agent:` task's reachable surface has TWO public fields, kept
separate: `tools:` and `workflows:`. Internally they are one tagged
`CapabilitySet`; a glob resolves to an EXACT set in the lockfile
(hashed and shown). An agent can never reach a path-shaped or dynamic
target — the closure is static, the same law as the top-level form.

## Errors (the `NIKA-COMP` namespace · new in this chapter)

| Code | Category | Meaning |
|---|---|---|
| `NIKA-COMP-001` | `validation_error` | a `workflow:` target is not statically resolvable (templated · malformed · unpinned registry ref) |
| `NIKA-COMP-002` | `security_error` | the child's effect boundary exceeds the parent ∩ declared (law 3/4) |
| `NIKA-COMP-003` | `validation_error` | the static call graph is not acyclic (self-launch · cycle · law 7) |
| `NIKA-COMP-004` | `validation_error` | the typed call does not compose (args ⋢ inputs, or outputs ⋢ returns · law 2) |

`NIKA-SEC-003` (run-recursion depth) stays the runtime backstop for the
templated-target case.

## `nika:run` and the sibling-run workaround (normative note)

The composition floor of today — `exec: nika run sub.yaml --output json`
+ `capture: stdout` ([08](./08-out-of-scope.md)) — is **superseded, not
contradicted**: the native `invoke: workflow:` form gives the same
sibling run its typed contract, its effect containment, and its trace
forest, none of which survive a process boundary. The workaround stays
valid; the native form is what a linter recommends.

## What v1 deliberately does not do

- **No dynamic dispatch.** A target is static — no computed workflow
  names, ever (the acyclicity and the cost story both depend on it).
- **No cross-workflow orchestration language.** Nika composes typed
  calls; a pipeline-of-workflows scheduler is a different problem
  ([08](./08-out-of-scope.md) · Temporal/Airflow class · out forever).
- **No session callables yet.** `Callable` reserves `Session`
  (protocol-shaped) beside today's `Unary`; the v1 form is unary.
- **No rewrite/e-graph equivalence.** Semantic-equivalence rewriting of
  compositions is LAB (never auto · benchmark before any core
  dependency).

## Related

- [09 · Types](./09-types.md) — the typed call (law 2)
- [10 · Authority](./10-authority.md) — effect containment (laws 3/4)
- [12 · Gateway](./12-gateway.md) — the import-side containment this
  generalizes
- [13 · Outcomes](./13-outcomes.md) — the child Outcome the trace forest
  records
