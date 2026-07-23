# 10 · Authority

> A workflow's authority is **declared in the file and judged before a
> token is spent** — three blocks, three questions, never conflated:
> [`permits:`](./01-envelope.md#permits--optional--the-declared-capability-boundary)
> says what the workflow **may touch** (capability) · `policy:` says in
> what **order and shape** it may act (law · this chapter) ·
> [`secrets:` + `egress:`](./01-envelope.md#egress--optional--sanctioned-destinations-declassification)
> say where a sensitive value **may go** (flow). The three compose
> without overlap; none is negotiation material for a model.
>
> One law binds them: **required ⊆ permitted**. What a task's body
> requires (computed statically, never declared by hand) must fit inside
> what the file grants. A violation is a check refusal with a **witness**
> — the task, the effect, and (for flows) the exact taint path.

---

## The effect vocabulary (normative · closed)

An **effect** is a class of interaction with the world outside the run.
The v1 vocabulary is exactly the capability surface of
[01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)
— **four categories** (this table is the one vocabulary the checker, the
inference, the certificate, and `policy:` rules all speak — one voice,
never a parallel list):

| Effect category | Carried by | Granted by |
|---|---|---|
| `fs` (read · write) | `nika:read` · `nika:grep` · `nika:write` · `nika:edit` · file-writing media builtins | `permits.fs.read` / `permits.fs.write` (path globs) |
| `net` | `nika:fetch` · `nika:notify` (webhook) · any URL-reaching builtin | `permits.net.http` (host allowlist · SSRF floor beneath) |
| `exec` | the `exec:` verb | `permits.exec` (`false` · `true` · program allowlist) |
| `tools` | the `invoke:` verb surface | `permits.tools` (id globs) |

Two derived facts the engine computes and no author writes:

- **A task's required effects** — from its verb and its arguments (the
  builtin classification table is normative for the stdlib: each
  catalog entry declares which category it carries and which argument
  names its target). `infer:` with no tools is **pure compute**: zero
  required effects (provider egress is governed by the secrets flow
  rules, not by a capability category).
- **The workflow's needed boundary** — the union of its tasks' required
  effects, as a `permits:` block (`nika check --infer-permits` prints
  it; the round-trip law holds: the inferred block re-checks clean).

**Required ⊆ permitted is judged twice** (per 01 §permits): statically
at `nika check` (an escape = refusal before any token) and at run time
(`NIKA-SEC-004` — the dynamic cases a static check cannot see).

## The `policy:` block · *optional · named workflow law*

`permits:` bounds capability; nothing bounded **order and shape** — «
no shell after an untrusted fetch », « a human signs before anything
irreversible », « only these providers » were checkable only by eyeball.
`policy:` names those laws next to the workflow:

```yaml
policy:
  require:
    human_gate_before: [exec, write]   # these effect classes sit behind a human
  forbid:
    exec_after: [net]                  # order law · no shell downstream of the network
  allow:
    providers: [ollama, mistral]       # infer/agent provider allowlist
  limits:
    max_tasks: 50                      # workflow-shape bound
  prefer:                              # SOFT · recorded, never judged (v1)
    providers: [ollama]
  optimize: cost                       # SOFT · recorded, never judged (v1)
```

**Grammar (normative · closed at every level).** `policy:` is a mapping
of up to six **families**. Unknown families and unknown rule names are
refusals (`NIKA-PARSE`-class): the rule set is closed per minor —
patterns are named after the incidents they prevent, raw temporal logic
is never exposed.

| Family | Nature | v1 rules (closed) |
|---|---|---|
| `require:` | **hard** · judged at check | `human_gate_before: [<effect-class>…]` |
| `forbid:` | **hard** · judged at check | `exec_after: [<effect-class>…]` |
| `allow:` | **hard** · judged at check | `providers: [<provider>…]` |
| `limits:` | **hard** · judged at check | `max_tasks: <positive integer>` |
| `prefer:` | **soft** · parsed, recorded, NOT judged | `providers: [<provider>…]` (ordered) |
| `optimize:` | **soft** · parsed, recorded, NOT judged | `cost` \| `latency` \| `quality` |

`<effect-class>` is the closed set `exec · write · net · tools` — the
effect vocabulary above with `fs` split at its grain of harm (`write`;
reads are not gateable in v1). Soft families are **inert by design** in
v1: the solver that would satisfy them is research surface, and a
constraint that cannot be judged must never look judged — the check
records them (a hint names them as recorded-not-judged) and nothing
else.

**Semantics of the hard rules (normative · statically decided on the
graph — the same derived graph every judge reads, [03](./03-dag.md))** ·

- `require.human_gate_before: [C…]` — every task carrying an effect of
  a listed class has, among its ancestors (data ∪ control edges), a
  **human gate**: an `invoke:` of `nika:prompt`. The pause is the
  consent (exit 4 · resume with the answer · [02](./02-verbs.md)).
- `forbid.exec_after: [C…]` — no `exec:` task is a descendant of a task
  carrying an effect of a listed class. The order law reads the graph,
  not the schedule: any path counts, `after:` edges included.
- `allow.providers: [P…]` — every `infer:`/`agent:` task's provider
  (the segment of `model:` before `/`, or the run's default model when
  the task names none) is in the list. A provider that cannot be
  determined statically (a templated `model:`) is a violation under a
  declared allowlist — **fail-closed**, pin the literal (same doctrine
  as the permits argv rule: judge the shape you can actually verify).
- `limits.max_tasks: N` — the workflow declares at most N tasks.

**A violation is `NIKA-POLICY-001`** (`security_error` · check-time ·
before any token). The diagnostic names the rule, the offending task,
and the witness — for order rules, the path (`fetch_page → summarize →
deploy`); for gate rules, the missing ancestor; for provider rules, the
offending literal. Policy violations are never fed back to an `agent:`
model: organizational law is not negotiation material.

**Composition** · `policy:` and `permits:` are orthogonal judges over
one body: permits answers *may this task touch X at all*, policy
answers *may it do so here, in this order, unattended*. A body must
satisfy **both**. (`allow.providers` deliberately covers the one
authority `permits:` does not — the provider surface; capability stays
permits' ground, and no policy rule re-spells a permits grant.)

## Secret flow refusals carry their codes (normative)

The flow rules themselves live in
[01 §egress](./01-envelope.md#egress--optional--sanctioned-destinations-declassification)
and are unchanged. This chapter names their wire codes — the two
refusal classes were report-only until W4:

| Code | Class | Witness (in the diagnostic) |
|---|---|---|
| `NIKA-SEC-006` | a `secrets.<name>` value reaches an **unsanctioned sink** (an `exec:` argument · an `invoke:` payload · an `infer:`/`agent:` prompt) | the **taint path**, source-first (`secrets.api_key → with.tok → tasks.call.output → exec`) + the exact `egress: [{ to: "<sink>" }]` clause that would sanction it |
| `NIKA-SEC-007` | a tainted value reaches the **workflow boundary** (`outputs:` — where a result leaves the run) | the taint path into the `outputs:` entry |

Both are `security_error`, check-time, blocking. The taint path is not
decoration: it is the **minimal witness** that lets an author decide
sanction-or-fix without re-deriving the flow by eye. The secret's
*value* never appears in any diagnostic.

## The permit-parameterization taint (normative)

The fit above proves `Required ⊆ Declared` on **categories**; it says
nothing about the resolved VALUES flowing under a present block. Every
value carries an integrity label — **Integ ∈ {trusted, untrusted}** —
orthogonal to Conf (the secrets axis): literals, `const.*`, and
`secrets.*` are trusted; `inputs.*` (caller-supplied at launch),
`config.*` (deployment-supplied, outside the file), fetch/tool results,
and anything derived from them are untrusted, with monotone propagation
(one untrusted operand taints the whole interpolation). Two rules bind
untrusted values under a `permits:` block (NEP-0004 · LAW-AUTH-0325):

| Code | Class | Witness (in the diagnostic) |
|---|---|---|
| `NIKA-AUTH-007` | an interpolation reaches a **permit bound** (a host, glob, program, or env-name literal inside `permits:` · NEP-0005 counts `env:` entries among the bounds) | the bound's own path (`net.http[0]`) — a bound MUST be a literal: the boundary would be self-serve, there is nothing left to canonicalize against |
| `NIKA-AUTH-008` | an untrusted value reaches a permitted verb's **argument** and its canonical resolved form escapes the step's permit | the **taint path**, source-first (`inputs.p → args.path`) + the resolved value, its canonical form, and the bound it escaped |

Both are `security_error`, check-time, blocking. The **re-gate** never
matches raw strings: the engine canonicalizes the RESOLVED value first,
per plane — fs paths are lexically normalized (`.`/`..` resolved,
separators collapsed) before the glob match, net hosts are lowercased
(IDNA→punycode, trailing dot and default port stripped), and for exec
argv the program is argv[0] while re-entry-class tokens (`--exec`, `-c`,
`eval`…) are never covered unless the permit lists them explicitly.
`datasets/../datasets/q3.csv` is INSIDE `datasets/**`;
`../../etc/passwd` is not — a prefix matcher cannot tell them apart,
the canonical form can.

An untrusted value not resolvable at check (no default) **defers**: the
file stays valid, the run-time re-gate is mandatory (an escape fails the
task `NIKA-SEC-004` — the defense-in-depth twin of NEP-0003 law 3), and
the check report SHOULD list the deferred re-gates informationally so CI
sees the attack surface before launch.

The only door is the authored one — a task-level `declassify:` entry:

```yaml
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.p }}" }
    declassify:
      - from: inputs.p            # one binding
        to: trusted
        because: "vendor inventory path, deployment-controlled, reviewed at release time"
```

`declassify:` MUST name `from:` (one binding), `to: trusted`, and a
non-empty `because:`; the receipt records it (taint path · because ·
value digest). It lifts the TAINT law only — the value is then matched
like a literal and must still sit inside the declared boundary.
Declassify is never a permit bypass, and there is no implicit
declassification in v1.

## The data-as-code sink (normative · NEP-0006 · LAW-AUTH-0327)

The contract distinguishes an INERT read from a CODE-BEARING read.
Some artifact classes execute at load: the serialized-executable
family (the deserializer runs code · `.pkl` `.pickle` `.dill`
`.joblib` `.pt` `.pth` `.ckpt`), scripts and notebooks (`.py` `.sh`
`.bash` `.zsh` `.ps1` `.bat` `.cmd` `.rb` `.pl` `.php` `.js` `.mjs`
`.ipynb`), and executable binaries/modules (`.exe` `.dll` `.so`
`.dylib` `.wasm` `.jar`). The three classes are CLOSED and normative
(only a NEP amends them · the deliberate exclusions and their reasons
live in NEP-0006). A `nika:fetch` whose RESOLVED URL path names one ·
matched case-insensitively on the path's final extension, the query
carries no verdict · is refused at check (`NIKA-SEC-008` ·
security_error · the diagnostic names the class and both repairs) when
the URL is literal or resolvable through the taint rules above; the
unresolvable DEFERS to the run-time twin (`NIKA-SEC-004` · the same
class refused on the resolved URL · defense in depth). The honest door
is the task-level `inert: "<because>"` declaration (non-empty ·
greppable): it lifts THIS law only · never the `net.http` boundary,
never the SSRF floor, never the taint re-gate. The repair in the other
direction is to model the acquisition as the `exec` it feeds, under a
program permit review can see.

## The certificate names its effects (normative)

`nika check --json` already emits a resource certificate
([05](./05-errors.md) · attempts · llm calls · effect calls · spend).
W4 adds the **authority projection** — `certificate.effects`:

```json
{
  "effects": {
    "boundary_declared": true,
    "needed": { "fs": { "read": ["./data/**"] }, "exec": ["git"] },
    "escapes": 0
  }
}
```

`needed` is the inferred boundary (the same object `--infer-permits`
prints) · `boundary_declared` says whether the file carries a
`permits:` block · `escapes` is the count of required-outside-permitted
violations (0 in any clean report — the field exists so a certificate
consumer never has to re-derive it). The certificate stays a
projection: the judge is the check ladder, never the JSON.

## Errors (the `NIKA-POLICY` namespace · new in this chapter)

| Code | Category | Meaning |
|---|---|---|
| `NIKA-POLICY-001` | `security_error` | a hard `policy:` rule is violated (the diagnostic names rule + task + witness) |

`NIKA-SEC-006` / `NIKA-SEC-007` join the existing `NIKA-SEC` namespace
([05](./05-errors.md)); `NIKA-AUTH-007` / `NIKA-AUTH-008` join the
`NIKA-AUTH` namespace opened by NEP-0003's `NIKA-AUTH-006`.

## One obvious way (normative for linters)

- A capability boundary is spelled `permits:` — `policy.allow` never
  re-spells a permits grant (providers is the one allow rule, because
  permits has no provider category).
- A human gate is `invoke: { tool: "nika:prompt" }` — the pause IS the
  consent mechanism; a `when:` on a `config` flag is not a gate.
- Soft families record intent (`prefer` · `optimize`) — an engine that
  cannot honor them MUST still accept them (they are never judged, so
  they can never refuse).

## What v1 deliberately does not do

- **No solver.** Soft constraints are recorded, not satisfied —
  Pareto/unsat-core surfaces are research (the constraint that cannot
  be judged must never look judged).
- **No runtime policy.** Every v1 rule is decidable at `nika check` on
  the graph; rules whose truth needs runtime data are out of scope,
  deliberately (the check stays the pre-token audit).
- **No new verbs, no `approve:`.** The human gate rides the existing
  `nika:prompt` pause; a dedicated approval surface (delegation ·
  quorum · `via:`) is future work with its own chapter.
- **No policy inheritance.** A file's law is the file's — composition
  across `invoke: workflow:` calls stays the callee's own policy (the
  ceiling algebra is reserved, unwired).

## Related

- [01 · Envelope](./01-envelope.md) — `permits:` (capability) ·
  `secrets:`/`egress:` (flow) — the two blocks this chapter composes with
- [03 · DAG](./03-dag.md) — the derived graph every order rule reads
- [05 · Errors](./05-errors.md) — the error registry (POLICY + SEC rows)
- [09 · Types](./09-types.md) — `returns:`/`types:` (the value contract;
  orthogonal to authority)
