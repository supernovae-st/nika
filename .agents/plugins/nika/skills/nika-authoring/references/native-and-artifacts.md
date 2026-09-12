# Native and artifacts

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## The one way (take the default, and the checker goes quiet)

Every authoring decision has a default. Take it unless the job forces
otherwise, in this order:

1. **Shape before content.** Resolve unfamiliar graph, binding and
   permit shapes from a relevant example before extending them. Preserve
   a working structure when the task only needs a small edit.
2. **One job, one task, one verb.** If a task needs an "and then", it is
   two tasks. The verb IS the key.
3. **Pick the verb by execution model, not convenience.** `invoke:` when
   something callable already does it · `infer:` when a model must
   produce judgement or language · `agent:` when the number of steps
   cannot be known in advance and must be bounded · `exec:` only when
   the first three genuinely cannot.
4. **Classify every value before writing it.** Caller-supplied →
   `inputs:` · deployment-supplied → an `inputs:` entry with
   `required: false` and a `default:` · fixed here → `const:` ·
   credential → `secrets:`. If you cannot name the class, you do not yet
   know what the value is.
5. **Bind, never reach.** A task needing another's output binds it in
   `with:`. Reaching for `tasks.*` anywhere else is `NIKA-VAR-021`.
6. **Order only when no data flows.** `after:` is pure sequencing; if
   data flows the `with:` binding already IS the edge. Never both.
7. **Bound the spend where it is spent.** Every `infer:` carries
   `max_tokens`; every `agent:` carries `max_turns` and
   `max_tokens_total`. A ceiling the checker can compute beats a cap
   someone has to remember to pass.
8. **Reconcile the boundary with the body and intended effects.**
   `nika check <file> --infer-permits` proposes grants; review them
   against the authorized scope before applying them. Expect a
   `NIKA-AUTH-006` the moment you save the permit-less draft (the
   write-time hook checks on your behalf): that finding is this step
   working, not a mistake to patch around. Read the review notes the
   inference prints and supply the paths it says it cannot compute.
9. **Fail on purpose.** Transient failure → `retry:` · expected absence
   → `on_error: on_codes + recover:` · cleanup for a producer that started →
   an ordinary task you name, declaring `after: { producer: unwind }`.
   This includes cancellation and timeout; a producer that never started
   unwinds nothing. Cleanup is best-effort, and process death can prevent it.
   `terminal` is a settled-state dependency, not a substitute for this
   cleanup lane. Swallowing an error is never the plan.
10. **Prove it before handing it over.** `nika check` clean, then
    `--native-strict`, then a golden pin if the workflow is hermetic.
    Report those checks and the run line, or execute it when authorized.

## Native-first (the law)

The order is `invoke: nika:*` → `invoke: mcp:<server>/<tool>` →
`exec:`. Before writing ANY `exec:`, answer in your head:

1. **Which builtin replaces it?** The embedded set spans SIX families.
   Assume one exists before assuming it does not — most `exec:` lines
   written by agents are a builtin the author never looked for.

   | Family | Every builtin in it |
   |---|---|
   | CORE | `nika:log` · `nika:emit` · `nika:assert` · `nika:prompt` · `nika:done` · `nika:wait` |
   | FILE | `nika:read` · `nika:write` · `nika:edit` · `nika:glob` · `nika:grep` |
   | DATA | `nika:jq` · `nika:json_diff` · `nika:json_merge_patch` · `nika:validate` · `nika:convert` · `nika:uuid` · `nika:date` · `nika:hash` · `nika:decide` |
   | NETWORK | `nika:fetch` · `nika:notify` |
   | INTROSPECTION | `nika:compose` (agent-loop only · calling a child is `invoke: { workflow: … }` · §Composition) · `nika:inspect` |
   | MEDIA | `nika:chart` · `nika:image_generate` · `nika:image_fx` · `nika:tts_generate` |

   The NAMES above are canon — that is the whole set. The argument
   CONTRACTS are not: read them from `nika catalog --tools`
   (`--json` for the model-facing JSON Schemas) before calling one,
   and never guess an arg name.

   The reflexes worth memorising: HTTP (curl/wget/helper fetch) →
   `nika:fetch` · file plumbing (cat/tee/cp/mkdir) →
   `nika:read`/`nika:write` (`create_dirs: true` creates the missing parents of a FILE — no empty write stands in for `mkdir`) · JSON shaping
   (jq/sed) → `nika:jq` or an `extract:` binding · in-place edits →
   `nika:edit` · finding files (`find`/`ls`) → `nika:glob` · searching
   them (`grep`/`rg`) → `nika:grep` · `date`/`uuidgen`/`shasum` →
   `nika:date`/`nika:uuid`/`nika:hash` · format conversion →
   `nika:convert` · schema checks → `nika:validate` · image styling
   (ImageMagick / PIL / dither scripts) → `nika:image_fx`
   (deterministic — same input+args = same bytes, the artifact sha256
   joins the trace chain).
2. **Which MCP tool replaces it?** A product API deserves an MCP
   server, never a helper script.
3. **Neither?** Name the exact gap — then `exec:` is legitimate
   (build tools · git · a product CLI with no MCP surface yet) and
   goes in the ledger.

Never write a helper script (`node bin/helper.mjs …`, `python3
bin/thing.py …`) that wraps HTTP/files/JSON — that is
`native-first/005`, the exact failure class this law exists for.

### When the boundary pushes back (the reason glue gets written)

Two refusals send authors reaching for a scripting language. Neither
one wants a script; both have a native recipe.

**`NIKA-SEC-004` — an untrusted value reached an effect argument.** An
`inputs:`-supplied or fetched value that check cannot resolve DEFERS to
a mandatory run-time re-gate; escaping that re-gate is SEC-004. The
diagnostic talks about the capability boundary, so the reflex is to
widen `permits:` — **that reflex is the trap, and it dead-ends.**

**The door is `lift:`** — a task-level list, the ONLY sanctioned lift
(spec 10 §the authored doors). One construct, two laws: `taint` and
`data-as-code`; the law is a PARAMETER of the door, never a second
spelling (`declassify:` and `inert:` were those spellings, and are dead).
This is a complete nine-key file (checked on 0.109 · rc=0):

```yaml
nika: load-reviewed-path
inputs:
  p:
    type: string
permits:
  tools: ["nika:read"]
  fs:
    read: ["./reviewed"]
tasks:
  load:
    invoke: { tool: nika:read, args: { path: "${{ inputs.p }}" } }
    lift:
      - law: taint              # the law this task opens
        from: inputs.p          # ONE binding
        because: "deployment-controlled path, reviewed at release time"
```

`law:` and `because:` are required on every entry, `from:` on `taint`
only (forbidden on `data-as-code`); `because:` must be non-empty — it is
recorded in the receipt with the taint path and the value digest. It
lifts the TAINT law only: the value is still matched against the
declared boundary, so this is never a permit bypass. A lift that would
not have fired is refused (`NIKA-AUTH-011`), so dead lifts cannot
accumulate.

**Why the staging recipe is the wrong first move.** Landing the value
in a file with `nika:write` and passing the PATH as argv looks safe, and
it is — until the CLI has to READ that file back. That read adds
`fs.read`, which completes the lethal trifecta, which makes a dominating
human gate mandatory. Measured in a real session: the chain runs shim →
`fs.read` → trifecta → mandatory gate → a gate that cannot be answered
(see the run notes on `nika:prompt`). Reach for `lift:` first.
Staging remains correct where the value genuinely must not touch a
command line AND nothing reads the file back inside the same workflow.

**`NIKA-SEC-009` — the trifecta.** Untrusted input, private data and an
egress in one task is refused as a shape, not as an accident. The move
is to keep the trifecta INCOMPLETE rather than to smuggle a leg through
a subprocess: take the fetched value as `nika:fetch` metadata or text
and do NOT add an `fs.read` of local content in the same flow.

If a genuine gap survives both recipes, `exec:` is legitimate — name the
exact missing capability in the ledger. A helper that exists to dodge a
refusal is the refusal winning.

## Exec ledger (mandatory when any exec remains)

Every surviving `exec:` gets a row in the workflow's header comment:

```
# EXEC LEDGER ·
# | task | command | why no native path | unlock that removes it |
```

The ledger is for the REVIEWER, not for the checker — it never silences
a finding. `--native-strict` judges the SHAPE of the exec: a real tool
passes, a script wrapper fails, and a row in the ledger changes neither.
If the wrapper is genuinely unavoidable, the honest move is to say so to
the human at handoff, not to expect a green.

## Discipline

- References: `${{ inputs.x }}` · `${{ const.x }}` ·
  `${{ secrets.X }}` · `${{ tasks.<id>.output }}`
  · `${{ with.alias }}` (never inline a credential) · and inside a
  `for_each:` body only, the loop-scoped `${{ item }}` · `${{ index }}`.
- Quote any scalar that STARTS with `${{` inside a FLOW mapping —
  `with: { body: "${{ tasks.a.output }}" }` — or YAML reads the `{{`
  as a nested map (`NIKA-PARSE-001`). In block style the quotes are
  optional.
- In `outputs:` bind `${{ tasks.<id>.output }}` — never the bare
  `${{ tasks.<id> }}`: that binds the ENVELOPE (status + timestamps),
  so `nika test` goldens drift red on every run. `nika check` teaches
  this as `[envelope-output]`; fix the binding, never re-baseline
  around it.
- A task that reads another task's output binds it in `with:` —
  `with: { alias: "${{ tasks.<id>.output }}" }` — and the body reads
  `${{ with.alias }}` (the binding IS the edge; `tasks.*` anywhere
  else is NIKA-VAR-021). Pure ordering is `after: { <id>: success }`.
  Use the predicates in the [task-modifier table](language.md); `nika spec --schema`
  and `nika check` own the accepted spellings (`NIKA-DAG-005` otherwise).
- Models are `provider/name` (`ollama/llama3.2:3b` local-first ·
  `mock/echo` simulated inference).
- Timeouts are quoted Go-durations (`timeout: "7m"`) — give local
  providers ≥300s: thinking models routinely think past 30s.
- Determinism is declared, not hoped: `run:` carries `entropy:`
  (`ambient` — the default when `run:` is absent — · `none` ·
  `{ seeded: <n> }`) and `clock:` (`system` · `virtual`). A
  contradictory declared pair refuses at parse; an `entropy: none`
  that still consumes randomness refuses at check.
- Structured output: give `infer:` a `schema:`; add
  `additionalProperties: false` for a deterministic shape.
  Numeric facts are `type: integer` + a numeric `enum`. The model
  extracts facts; `nika:jq` / `nika:decide` is the law
  (`13-extract-then-law`).
- Auth rides `headers: { x-api-key: "${{ secrets.KEY }}" }` (masked ·
  declared in `secrets:` with its `egress:` sink) — never `exec: curl`
  for the sake of a header.

### Writing JSON: build the VALUE, never the braces

Hand-writing JSON punctuation around an interpolation is the one way to
get a green check, a green run, and an unreadable artifact. Both halves
below were measured on 2026-07-28 with a value containing a quote and a
newline. This is a complete nine-key file (checked on 0.109 · rc=0):

```yaml
nika: write-json-value
inputs:
  v:
    type: string
permits:
  tools: ["nika:jq", "nika:write"]
  fs:
    write: ["./out/**"]
tasks:
  # ✗ green everywhere, and the artifact does not parse.
  naive:
    with: { v: "${{ inputs.v }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/naive.json", content: '{"value": "${{ with.v }}"}' }
      # writes  {"value": "he said "hi"
      # and a newline"}                       ← malformed. Nothing warned.

  # ✓ build the object as a VALUE, then write the value whole.
  build:
    with: { v: "${{ inputs.v }}" }
    invoke:
      tool: "nika:jq"
      args: { expression: "{ value: .v }", input: { v: "${{ with.v }}" } }
  safe:
    with: { obj: "${{ tasks.build.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/safe.json", content: "${{ with.obj }}", create_dirs: true }
      # writes  {"value":"he said \"hi\"\nand a newline"}   ← parses.
```

**The law: an interpolation may be a whole JSON value or a whole string,
never a fragment welded between quotes you typed.** A structured value
interpolated into a string argument is serialized as JSON by the engine,
with escaping. Two ways to get one: `nika:jq` (any reshaping), or an
`infer:` carrying a `schema:` (a model's typed output, the shape
`meeting-actions` uses to land its artifact).

**Then validate the artifact twice, because well-formed is not right.**
Parse it, AND spot-check one value against a number you computed by
hand. A green run that emits malformed JSON is a failure with a
checkmark on it; a green run that emits well-formed JSON full of wrong
values is the same failure wearing a better disguise, and `json.loads`
cannot see it.

**Prove `nika:jq` against known answers.** The engine corrects its
upstream `scan` implementation to collect every match, so
`[.s | scan("\\S+")]` on `{"s":"one two three"}` yields all three
words. Do not preserve a workaround for the retired first-match bug.
Keep the actual invariant: assert the expected values, not only valid
JSON or a green exit, including empty input and regex flags where used.

## When the language pushes back, that is a finding

Every friction you meet writing a workflow is a datum about the spec or
the engine. Do not route around it silently — a workaround written once
is a workaround written forever, and the next author meets the same wall
with no trace that you were here.

**The triggers.** Any one of these is enough:

```
  a green `nika check` whose run dies
  a diagnostic that does not name the remedy that WORKS
  a builtin that returns a plausible, wrong answer
  a form you had to guess because no surface teaches it
  a workaround you write "just this once"
  two lines of the SAME output that disagree
  a green run that produces a malformed artifact
```

**The move, in order.**

1. **Reproduce** it small. Never "I think it does X" — the exact file and
   the exact command.
2. **Locate** it: a `file:line` in the engine, or a `§` in the spec.
3. **Write it down** where it survives the session — a local finding
   or plan entry; post an issue only when external reporting is in scope.
4. **Fix at the source.** The spec repo, not a vendored copy of it: a
   mirror is re-vendored and your fix disappears.
5. **Leave a ratchet** — a test, a hint, a gate. Without one the class
   comes back.

**Why this earns its place in an authoring skill.** Writing a five-task
provenance workflow on 2026-07-28 surfaced three defects in twenty
minutes, none of them being looked for: a builtin's required arg missing
from the example that taught it, a `fetch → jq` pipe that hands text to a
tool expecting a value, and a scalar interpolated into literal JSON that
produced an unquoted field — green check, green run, unreadable artifact.

That is the ordinary rate. The language is good and it is not finished,
and the only reason it converges is that the people using it report what
they hit. **A workflow you had to fight is worth more as a bug report
than as a file.**
