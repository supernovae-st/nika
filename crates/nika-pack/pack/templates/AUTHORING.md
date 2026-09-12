# Workflow authoring details

Paths and commands in this guide resolve from the repository root. Read the sections needed for the workflow you are authoring; this guide does not run effects.

## Build and validate the requested workflow

Use a matching template when it supplies the required structure. Adapt the workflow to the task while preserving the language contract; validation, rather than a model-confidence claim, establishes static correctness.

```
INTENT ──route──▶ TEMPLATE ──fill──▶ DRAFT ──check──▶ ERRORS ──repair──▶ ✓
                  (copy · never        slots only      each error
                   invent structure)                   names its fix
```

1. **Route** · `templates/README.md` maps intent → a canonical
   skeleton (its routing table IS the list — never enumerate it here:
   a hand-typed count went stale the day the shelf grew). Composite
   jobs compose templates.
2. **Instantiate** · copy the template · fill every `<SLOT: …>` value ·
   comments may explain other decisions, but only a value can make the
   checker refuse an unfilled scaffold. For a different structure, use the relevant spec contract and validate it; do not invent unsupported fields.
3. **Check** · `python conformance/runner.py validate <file>` (this
   repo's oracle) or `nika check` (engine). NEVER ship unchecked.
4. **Repair from the error** · the codes are prescriptive ·
   `NIKA-PARSE` = the YAML shape is wrong — the message names the key
   and what the schema allows there (exactly-one-verb · snake_case id ·
   quoted duration · unknown field) ·
   `NIKA-PARSE-002` = a missing envelope field — add `nika:`, or a
   non-empty `tasks:` (a file with no `nika:` key is not a nika file) ·
   `NIKA-PARSE-003` = `nika:` is not a kebab-case id (`^[a-z][a-z0-9-]*$`) —
   it carries the file's NAME, never a version ·
   `NIKA-DAG-001` = break the dependency cycle ·
   `NIKA-DAG-002` = a `with:`/`after:` entry names a task that doesn't exist ·
   `NIKA-VAR-021` = a `tasks.*` reference outside the boundary — hoist it into `with:` ·
   `NIKA-VALUES-001` / `NIKA-VALUES-002` = a dead `vars:` / `env:` block (or
   `${{ vars.X }}` / `${{ env.X }}` read) — classify each use: typed parameter
   → `inputs:` · a knob the deployment supplies → an `inputs:` entry with
   `required: false` and a `default:` · fixed value → `const:` · store
   reference → `secrets:` ·
   `NIKA-VALUES-003` = a `${{ }}` value read outside the three authorities
   (`inputs` · `const` · `secrets`) — the namespace is closed ·
   `NIKA-TYPE-001` = a PascalCase name in type position — named types are
   gone; write the type expression INLINE in `returns:` ·
   `NIKA-SEC-015` = the order law — an `exec:` task sits transitively
   downstream of a net-effecting task (`nika:fetch` · `nika:notify`).
   Consume the fetched value with a builtin (`nika:jq` · `nika:validate`)
   instead of a shell, or drop the edge. The diagnostic names the PATH,
   which is the witness. **Unconditional** — no block declares it and none
   can disable it ·
   `NIKA-DAG-004` = your `recover:` points DOWNSTREAM of the failing
   task (deadlock) — recover from an upstream or independent source ·
   `NIKA-VAR-001` = declare the name or fix the typo ·
   `NIKA-VAR-003` = the path into a declared `schema:` names a key the
   schema forbids — fix the path or the schema ·
   `NIKA-VAR-005` = the `${{ }}` body is outside the CEL v0.1 subset
   (chained relation · unknown function · bare non-boolean `when:`
   root) — or a jq binding doesn't compile ·
   `NIKA-VAR-008` = unclosed `${{` ·
   `NIKA-BUILTIN` = a builtin's args are wrong (the message cites
   builtins-v0.1.md · e.g. `nika:write` without `content:`) ·
   `NIKA-PROVIDER` = `model:` needs a canonical `<provider>/<name>` —
   the message lists the valid prefixes.
   Repair and re-check affected findings. If the same failure persists without new evidence, diagnose the blocker instead of repeating an unchanged attempt.
5. **Match constructs to proof** · need a construct you haven't used?
   The coverage matrix (docs `examples/overview` · generated) names
   the canonical example that exercises it — read it, don't guess.

Frequent authoring constraints checked by the oracle: one verb per task — the verb IS the task key (`infer:` /
`exec:` / `invoke:` / `agent:` · NEVER a `verb:` field with flattened
args) · snake_case task ids · kebab-case `nika:` · every
`${{ tasks.X }}` reference in `when:`/`with:`/`for_each:`/verb fields
lives at the BOUNDARY: `with:` values (the binding IS the edge) · `after:`
keys · `on_error.recover` · an `unwind` task (its producer only) · workflow
`outputs:` (the ONLY other exemptions · `extract:` is pure jq
— `${{ }}` never appears there at all — and `on_error.recover:` / an
`unwind` body read recovery/producer state · 03 §carve-out) · `invoke`
arguments live under `args:` (not `input:` / `params:`) · quote any
YAML scalar that starts with `${{` (an unquoted leading `${{` breaks
the YAML parse) · `when:` is a `${{ }}` CEL boolean OR the literal
`true`/`false` — a bare string is rejected · CEL callables are a closed
set: `size(x)` · `has(x)` · `x.size()` · `x.contains(s)` ·
`x.startsWith(s)` · `x.endsWith(s)` · `nika:write` without `content:` is
rejected ·
`nika:done` outside `agent.tools` is rejected.

(Every rule above is enforced STATICALLY by this repo's oracle — the
last four landed 2026-06-11 from eval failure clusters · check catches
them all before any model spends a token.)

One style rule the oracle cannot catch · when a task declares
`schema:`, write the prompt NATURALLY — never say « respond in JSON »
or paraphrase the schema in prose. The engine owns the format
negotiation; a prompt that re-states it fights the engine and degrades
weak-model output (the eval measures exactly this).

**Extract facts, then the law.** A model may produce closed, cited
semantic *facts*. Scoring, routing, publish/abstain is `nika:jq` or
`nika:decide` — never a second `infer:` to "pick the level". Numeric
facts are `type: integer` with a numeric `enum` (`-1|0|1|3`); a
string enum of digits (`"0"|"1"|"3"`) is the shape models do not emit
(JSON `3`). The engine hints `digit-string-enum`. The shape is
`examples/13-extract-then-law.nika.yaml`. The named bundle is
`examples/14-decide-publish.nika.yaml`. Prove the law on const
fixtures (`unproven-law`) before leaving `mock/`. An agent that
*writes* Nika grants `nika:compose` on `agent.tools` after
`nika:done` and iterates on the check JSON until `valid`
(`examples/15-compose-self-check.nika.yaml`). A standalone
`invoke: nika:compose` is `NIKA-BUILTIN-COMPOSE-001`. Checking
never executes the draft.

**Paid-infer order** (cheaper than discovering this with tokens) ·
`nika check --native-strict` → one-task `mock/echo` probe of every
new builtin → freeze the extract schema type → pin the glob
(`exclude: "**/README.md"`) → then wire a paid model.

**After valid, review the shape and judgment.** Inspect relevant examples
when a construct is unfamiliar. Use `for_each` for collections and
`invoke: { workflow: }` for child workflows; verify deterministic rules
with known-answer fixtures and `nika:jq`, not a second infer.
For an execution-ready handoff, require a clean `nika check --native-strict`
and preserve the intended authority. Before paid inference, also require
`paid_ready: true` and inspect the paid blockers. Use `.next` to locate a
repair, then inspect the remaining findings and `.compiled`; these fields
do not grant execution permission or establish that effects occurred.
Explain any remaining non-blocking hint in the file (CONVENTIONS §10).
The engine hints `infer-as-law` when a prompt asks the model to assign a
belt. A second infer whose schema is a language enum is language, not the
law. Check the selected oracle mode when interpreting `infer-as-law` and
`digit-string-enum` findings; static validity, paid readiness and execution permission are distinct.

The judgment layer (after validity) is the 12 patterns ·
docs `guides/patterns` — deterministic core · parallel by default ·
typed boundaries · leashed fan-outs · the three gates · sovereignty ·
budgets · evidence lands · jq once · callable outputs · mock-first.
