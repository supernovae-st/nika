# 04 · Variables

> Nika uses **one substitution syntax everywhere** · `${{ ... }}` ·
> matching GitHub Actions. Inside strings · inside object values ·
> inside array elements · inside conditions. One syntax · one mental
> model.

---

## The one syntax · `${{ ... }}`

```yaml
# Inside a string
prompt: "Summarize · ${{ inputs.topic }}"

# Inside a value position
with:
  data: ${{ tasks.research.output }}

# Inside a condition (local namespaces · [03 §when](./03-dag.md))
when: ${{ with.coverage > 80 }}

# Inside an array
tools:
  - ${{ const.tool_a }}
  - ${{ const.tool_b }}
```

If you have used GitHub Actions, this is the same. If you have not, the rule is simple · **anywhere you want a value resolved at task dispatch time, wrap it in `${{ }}`**.

**What's inside `${{ }}` is [CEL](https://cel.dev)** (Common Expression
Language · the validated, non-Turing-complete standard used by Kubernetes,
Envoy, and gRPC). A bare reference like `${{ inputs.topic }}` is a CEL identifier
path that evaluates to its value; a condition like `${{ with.coverage > 80 }}`
is a CEL boolean. One expression language, everywhere: Nika does not invent a
DSL. See [03-dag.md](./03-dag.md#expression-language--a-documented-subset-of-cel) for the v0.1 CEL subset.

---

## The 5 namespaces

```
${{ inputs.X }}            typed workflow input          (declared in envelope `inputs:` · supplied by the caller at launch, or by its own `default:`)
${{ const.X }}             named constant                 (declared in envelope `const:` · a fixed value baked into the workflow)
${{ secrets.X }}           masked secret reference        (vault-backed · never in logs)
${{ with.X }}              task-level scope               (declared per-task `with:` block · the bindings ARE the data edges)
${{ tasks.X.output }}      task record reference          (or .status · .error · .duration_ms · the CLOSED projection set · BOUNDARY surfaces only — see below)
```

Five namespaces. That's it.

The first three — `inputs` · `const` · `secrets` — are the **value
authorities**, the closed family every workflow value is declared under
(LAW-SURFACE-0201 · one authority, one spelling, no alias). `with` and
`tasks` are the **runtime namespaces** (task-scope bindings and settled task
records) — legal in `${{ }}`, never value authorities.

> **⚰️ `config` was the fourth, and it is dead (2026-08-12).** Measured:
> **zero** usage in real work; its `default:` was its **only** possible
> source (nothing outside the file could supply it — `--var` reaches
> `inputs:` only); and under the taint lattice `${{ config.p }}` and
> `${{ inputs.p }}` produced **the same** `NIKA-AUTH-008` by the same taint
> path — **indiscernible**. The cell it occupied (nobody supplies it ×
> treated as untrusted) was a constant the checker was being asked to treat
> as hostile. A deployment knob is now an `inputs:` entry with
> `required: false` and a `default:`. A `${{ config.X }}` read refuses
> `NIKA-VALUES-003`, which teaches the three-authority family.

> **Dead forms (refused with a classification teaching · the E-split).**
> The pre-flip `vars:` and `env:` envelope fields are dead: a `vars:` block
> refuses with `NIKA-VALUES-001`, an `env:` block with `NIKA-VALUES-002`,
> and a `${{ vars.X }}` / `${{ env.X }}` read with the same codes. Each
> old use **classifies** into the authority its role commands — a typed
> parameter is an `inputs:` declaration, a fixed value a `const:` entry,
> non-sensitive runtime configuration an `inputs:` declaration with
> `required: false` and a `default:`, a governed store reference a
> `secrets:` entry (classify-not-rename · never a bulk rename). A
> value-namespace read outside the three-authority family
> (`${{ params.X }}` and friends) refuses with `NIKA-VALUES-003`.

### The reference boundary · where `tasks.*` may appear

Since W2 « the flow », the `tasks` namespace is **boundary-only**. A
`${{ tasks.X.* }}` reference is legal in exactly five places ·

| surface | why it is a boundary | graph effect |
|---|---|---|
| `with:` values | the binding imports the data — **the binding IS the edge** | one typed edge per reference ([03 §with](./03-dag.md)) |
| `with:` values, plural · `${{ group.<name> }}` | the fan-in fold imports a whole declared set — **this is the ONLY door it has**, tighter than `tasks.*` | one `fan-in` edge per declared member ([03 §group](./03-dag.md#group--optional--fan-in-membership--the-plural-of-a-data-edge)) |
| `after:` keys | the entry names the producer | one control edge per entry |
| `on_error.recover:` | a fallback reads a settled record | a recovery edge (parking · `NIKA-DAG-004`) |
| an `unwind` task body | cleanup reads the **producer it unwinds** — the ONLY legal target there (a sibling may still be running · the read would race) | none (the E_f attachment never enters `G_p`) |
| workflow `outputs:` | the run's exports read the settled world | none (everything is terminal at read time) |

**Everywhere else — verb fields (`prompt:` · `command:` · `args:` · …),
`when:`, `for_each:` — a `tasks.*` reference is refused at parse time**
(`NIKA-VAR-021` · `validation_error`) with a machine-applicable fix:
**hoist it into `with:`** and read the binding ·

```yaml
# ❌ NIKA-VAR-021 — the body reads the global namespace
summarize:
    infer:
      prompt: "Summarize · ${{ tasks.fetch.output }}"

# ✅ the fix `nika check --fix` applies
summarize:
    with:
      article: ${{ tasks.fetch.output }}
    infer:
      prompt: "Summarize · ${{ with.article }}"
```

The task body is a **pure function of its declared inputs** (`with` · the
three value authorities `inputs` · `const` · `secrets` · the loop
locals): every cross-task dependency is
visible at the boundary, named, and typed by its edge role. Nothing else
reads another task.

> **Bare `tasks.X` is not a value (normative · D2 · #75).** The task
> result is a record; its observable projections are the CLOSED set
> `.output` (the value) · `.status` (terminal enum) · `.error` ·
> `.duration_ms` — additions are a spec minor. An UNPROJECTED
> `${{ tasks.X }}` in any value position is a `validation_error`
> (`NIKA-VAR` · « the envelope is not a value — pick `.output` »):
> before 0.103 it silently denoted the whole envelope, the source of
> the golden-drift trap engine#524 had to teach around. No aliases —
> one meaning per spelling.

> **`group.<name>` is not a 7th namespace either.** It is the **plural reader
> of `tasks`** — same runtime family, one door instead of five (a `with:`
> value · everywhere else is `NIKA-VAR-021`). It denotes a set of task
> records, so it belongs beside `tasks` rather than beside the three value
> authorities, and the count below is unchanged. See
> [03 §group](./03-dag.md#group--optional--fan-in-membership--the-plural-of-a-data-edge)
> for the record shape and the `fan-in` pass-set.

> **Loop-locals are not a 7th namespace.** Inside a `for_each` task body, two
> extra identifiers are in scope: `${{ item }}` (the current element) and
> `${{ index }}` (its 0-based position). They are **loop-scoped locals**, alive
> only within that task's body, not global namespaces. So the count stays
> « <!-- canon:namespaces -->5<!-- /canon --> namespaces » + the for-each locals where a loop is present.

**Shadowing is structurally impossible.** Every namespace is reached
through its explicit prefix (`inputs.` · `const.` · `secrets.` ·
`with.` · `tasks.`): an `inputs.item` and the loop-local `item` never collide
(one is `inputs.item` · the other is bare `item`), a task may be named
`item` (`tasks.item.output` is unambiguous), and `with.X` never hides an
`inputs.X`. The only bare identifiers in the language are the two
loop-locals, and `for_each` does not nest within a task, so there is no
scope chain · no resolution-order subtleties · nothing to shadow. This
is by construction, not by rule.

### `${{ inputs.X }}` · typed workflow inputs

Declared once in the envelope · immutable across the workflow run · every
entry is a **typed declaration** whose `type:` speaks the full TypeExpr of
[09-types.md](./09-types.md) (the flat 6-enum is dead · `bool` is the one
boolean spelling) — validation + schema generation for callable workflows
(see [01-envelope.md](./01-envelope.md#inputs--optional--typed-workflow-inputs)) ·

```yaml
nika: research-pipeline

inputs:
  topic:
    type: string
    required: true                   # the caller MUST supply it
    description: "Subject to research"
  paragraphs:
    type: integer
    default: 5                       # MUST conform to type: (checked · NIKA-DEFAULT-001)

tasks:
  research:
    infer:
      prompt: "Research · ${{ inputs.topic }} · in ${{ inputs.paragraphs }} paragraphs"
```

A `required: true` input has no default: the caller must supply it at
launch. To supply or override an input ·
`nika run flow.nika.yaml --var topic="CEL subsets in 2026"` (repeatable ·
engine CLI concern). A `--var` value overrides the declared default and
satisfies a `required: true` input · an undeclared key is refused before the
run. See [01-envelope.md](./01-envelope.md#inputs--optional--typed-workflow-inputs)
for the launch contract.

### `${{ const.X }}` · named constants

```yaml
const:
  retries: 3                                # bare literal
  output_dir: "./output"
  window:                                   # typed constant · value MUST conform to type:
    type: integer
    value: 30

tasks:
  note:
    infer:
      prompt: "Keep the ${{ const.retries }} retries in ${{ const.output_dir }}"
```

`const` holds the fixed values baked into the workflow: either a **bare
literal** (any YAML value) or a **typed constant** `{ type, value }` whose
`value:` MUST conform to its `type:` (checked · `NIKA-DEFAULT-001`). An
object carrying BOTH `type` and `value` keys is the typed form; an object
missing either key is a bare literal object constant (the discriminator ·
so a literal like `config: { type: "custom" }` is never misread). Constants
are immutable across the run and are never caller-supplied: a value the
caller must be able to override is an `inputs:` declaration, not a constant.

### `${{ with.X }}` · task-level scope

Declared per-task · resolves at task dispatch time · the task's **import
surface** ·

> **`with:` is the data boundary, not sugar.** A `${{ tasks.X.* }}`
> reference lives ONLY in `with:` (and the other boundary surfaces above):
> each such binding creates one typed edge, and the body consumes the
> binding by its local name. `with:` is where a task's inputs are visible
> at a glance — and where the graph gets its data edges
> ([03 §with](./03-dag.md)).

```yaml
summarize:
    with:
      content: ${{ tasks.research.output }}    # value edge · research → summarize
      style: "concise"                         # literal · no edge
    infer:
      prompt: "Summarize in ${{ with.style }} style · ${{ with.content }}"
```

A binding whose evaluation errors settles the task `failure` — `on_error:`
is NOT consulted (the boundary feeds the verb; the armor covers the verb ·
[03 §gate algebra](./03-dag.md#the-gate-algebra-v2-normative)).

### `${{ tasks.X.output }}` · task record reference

Reference an upstream task's output (or status · error · duration_ms) —
at the boundary ·

```yaml
deploy:
    after: { test: success }                              # strict gate · no data from test
    with:
      coverage: ${{ tasks.test.output.coverage }}         # value edge · read the number
      artifact: ${{ tasks.build.output.artifact_path }}   # value edge · build → deploy
    when: ${{ with.coverage > 80 }}                       # local business condition
    exec:
      command: ["./deploy.sh", "${{ with.artifact }}"]
```

`tasks.X` is the task **result record**: a CEL object, NOT the bare output
value. Always write `.output` for the value · the record's fields are ·

```
${{ tasks.X.output }}                the verb's output (string · object · or bytes · per verb · see 02)
${{ tasks.X.status }}                success | failure | skipped | cancelled  (closed enum · v1)
${{ tasks.X.error }}                 typed error record · present iff status == failure (see 05)
${{ tasks.X.started_at }}            RFC 3339 start timestamp
${{ tasks.X.ended_at }}              RFC 3339 end timestamp
${{ tasks.X.duration_ms }}           execution time · integer milliseconds
${{ tasks.X.<name> }}                a named extract: binding (jq · see below)
```

#### Defined-null reads (normative · the branch-join unlock)

Reading a field of a task that reached a terminal state **never errors**:
absent values are **`null`**, deterministically ·

```
tasks.X.output   of a skipped task        → null   (incl. empty-collection for_each)
tasks.X.output   of a cancelled task      → null
tasks.X.error    when status != failure   → null   (EXCEPT on_error.skip · error stays · see 05)
tasks.X.<name>   bindings of a skipped/cancelled task → null
```

`null` is a CEL literal (`with.x != null` is in the v0.1 subset) and
a JSON value (jq's `select(. != null)` filters it). This makes the
**diamond-join** canonical · two exclusive `when:` branches + a join that
takes whichever ran ·

```yaml
pick:
    with:                                     # value edges pass on skipped · the skipped one is null
      prod: ${{ tasks.build_prod.output }}
      dev: ${{ tasks.build_dev.output }}
    invoke:
      tool: nika:jq
      args:
        input: [ "${{ with.prod }}", "${{ with.dev }}" ]
        expression: "[ .[] | select(. != null) ] | first"
```

**One obvious way · no bare alias.** `${{ tasks.X }}` is the whole result
object · the output is ALWAYS `${{ tasks.X.output }}`: there is no `tasks.X`
== output shortcut (it would make `tasks.X` both a scalar and a record · which
CEL cannot type). This matches every workflow engine · GitHub Actions
`steps.X.outputs` · Argo node context · Temporal result-vs-state · the task
result is a record, never a scalar masquerading as the namespace.

### Static binding validation against a declared `schema:` (normative)

When the producing task declares a structured-output **`schema:`** (an
`infer:` or `agent:` task · [02](./02-verbs.md)), the shape of
`tasks.X.output` is KNOWN at parse time, so a reference path INTO that
output (`${{ tasks.X.output.entities }}`) is statically checkable. The
authoring contract ·

- An engine **SHOULD** validate `tasks.X.output.<path>` references against
  the declared schema at parse time (the misspelled-key class is caught
  before any model is called).
- An engine **MUST reject invalid paths** · `NIKA-VAR-003` ·
  `variable_error`. A path step is *invalid* when ·
  1. a **member step** lands on a schema level that **declares
     `properties:`** and does NOT list the key — declaring properties
     CLOSES the level for binding (operator lock 2026-07-30 · strict by
     default, one voice with the `returns:` walk below). The level opens
     back up ONLY explicitly: `additionalProperties: true` (or a schema
     object for extras) makes undeclared siblings legal again.
     (`additionalProperties: false` on a level with no `properties:`
     also closes it — the declared empty object.)
  2. a **member step** lands on a level whose `type` excludes `object`;
  3. an **index step** lands on a level whose `type` excludes `array`.
- The static walk covers the v0.1 subset **`properties` · `items` ·
  `type` · `additionalProperties`** only. Any other construct at a level
  (`$ref` · `oneOf` / `anyOf` / `allOf` · `patternProperties` · a missing
  `type` · …) makes that level **open**: the walk stops and the engine
  **MUST NOT** reject anything beneath it.
- A level that declares **no `properties:` at all** (a bare
  `type: object`) is **open**: nothing is declared, so nothing can be
  contradicted — paths beneath it are never statically rejected.
- Tasks with NO declared schema (every `exec:` / `invoke:` task · an
  `infer:` without `schema:`) are dynamic: paths into their output are
  never statically rejected (a wrong path surfaces at run time as
  `NIKA-VAR-001`).

The balance is deliberate (2026-07-30 · supersedes the earlier
"only `additionalProperties: false` closes" reading): where the author
declared NOTHING, the check stays **sound** — open levels and dynamic
producers are never refused. Where the author DECLARED the shape, the
declaration is a contract — reading an undeclared sibling of declared
keys is the misspelled-key class and refuses loudly, with a one-line
fix either way (declare the key, or open the level with
`additionalProperties: true`). Note the runtime nuance this owns: JSON
Schema itself treats an absent `additionalProperties` as permissive at
run time, and that runtime semantic is unchanged — the static BINDING
law is stricter than the runtime validator on purpose (catching typos
before any model is called is the point of declaring a schema).

**`returns:` sharpens the walk (normative · [09-types.md](./09-types.md)).**
When the producer declares a `returns:` type instead of a raw `schema:`,
the walk runs on the type with **full precision**: the v1 type grammar
has no open construct, so every level is walkable — a member outside a
closed object is `NIKA-VAR-003` (the same code · one voice), and only
`additional: true` or an `Unknown` producer opens a level. The raw
`schema:` hatch keeps the weaker subset walk above.

### `${{ secrets.X }}` · masked secret reference

```yaml
secrets:
  api_key:
    source: vault
    key: prod/anthropic/api-key
    egress: [{ to: "nika:fetch", host: "api.anthropic.com" }]
headers:
  Authorization: "Bearer ${{ secrets.api_key }}"
```

A secret is always a **reference to a store** (the local `nika-vault` by
default), declared in the envelope `secrets:` block, never an inline
literal. The engine **masks** every resolved `secrets.X` value in logs,
traces, and journal events (it renders as `••••••`). This `inputs` / `secrets`
split is the modern secure-workflow default: non-sensitive values in `inputs`,
masked references in `secrets`.

> **The masking boundary (normative).** Masking covers the engine's OWN
> observability surface: logs · traces · journal · the `nika:inspect`
> output. It does NOT follow a secret value that the AUTHOR routes into a
> subprocess or tool that then re-emits it: a `secrets.X` put into
> `exec.env` (or a `nika:fetch` header) which the command echoes to stdout
> is captured verbatim into `tasks.X.output` and flows downstream like any
> other data: the engine cannot know that captured string IS the secret.
> **The contract:** the engine masks what IT prints; the author owns what
> they pipe a secret INTO. The `nika check` pre-flight flags **every**
> unsanctioned `secrets.X` flow into an effect (`exec` · `invoke` — and the
> provider-egress sinks `infer` / `agent`, where a secret in a prompt leaves
> the run to a third party), so the leak is caught statically before the
> run, not after. A legitimate flow is **sanctioned where the secret is
> declared** — the `egress:` list on the secret (the example above · full
> grammar in [01-envelope §egress](./01-envelope.md)): declassification is
> the owner's act, co-located with the data, never a property of the sink.

---

## Extraction bindings · `extract:`

Use `extract:` to define **named bindings** extracted from a task's raw response via a **jq expression** (the one data language). These bindings appear in the task's typed output and are referenced as `${{ tasks.X.<name> }}` ·

```yaml
api_call:
    invoke:
      tool: "nika:fetch"
      args:
        url: "https://api.example.com/v1/users"   # returns JSON · extract: jq extracts
    extract:
      user_count: ".data.users | length"
      first_user: ".data.users[0]"
      user_emails: "[.data.users[].email]"   # [...] collects the stream into an array
```

Downstream ·

```yaml
notify:
    with:
      user_count: ${{ tasks.api_call.user_count }}    # value edges · the bindings are the edges
      emails: ${{ tasks.api_call.user_emails }}
    infer:
      prompt: |
        We have ${{ with.user_count }} users.
        Emails · ${{ with.emails }}
```

#### Raw output vs named bindings · dual-accessible

When a task has an `extract:` block defining named bindings · downstream
access is **dual-accessible** ·

```yaml
api_call:
    invoke:
      tool: nika:fetch
      args: { url: "..." }
    extract:
      body: .body
      http_status: .status     # NOT `status:` — that name is reserved (the task's own .status)
```

Downstream (each form imported through a consumer's `with:` · the
reference boundary) ·

```yaml
# Raw output (whole structure · pre-binding extraction)
${{ tasks.api_call.output }}             # full raw JSON · including all fields the verb returned

# Named bindings (defined in extract: block above) · value-role fields
${{ tasks.api_call.body }}               # jq .body  · the response body
${{ tasks.api_call.http_status }}        # jq .status · the HTTP status field
${{ tasks.api_call.status }}             # RESERVED · the task's own status (success|failure|…) · NOT a binding · terminal-observation role
```

**Rules** ·
- `tasks.X.output` ALWAYS returns the raw output (unmodified value
  returned by the verb · before any binding extraction)
- `tasks.X.<name>` for any `<name>` declared in `extract:` block returns
  the extracted jq result
- `<name>` collisions with reserved words `output` · `status` · `error` ·
  `started_at` · `ended_at` · `duration_ms` are forbidden at parse time
  (`NIKA-PARSE` · `validation_error`: the rule is structural · schema-checkable
  via `propertyNames` · `NIKA-VAR-NNN` stays reserved for reference *resolution*
  and binding *evaluation* errors)
- If no `extract:` block · only `tasks.X.output` is accessible (named
  bindings are an opt-in convenience)

### Path grammar · jq (the one data language)

Output binding uses a **jq expression**: the SAME jq as the `nika:jq` builtin.
Nika has ONE data extraction-and-transform language (jq), **not two**: the former
RFC 9535 JSONPath was dropped because jq is a superset (any JSONPath query + more)
and a workflow language must not force the author (or an LLM) to choose between
two extraction syntaxes (SOTA « one obvious way · ≤2 expression layers »). The two
expression layers are **CEL** (inside `${{ }}` · conditions + value substitution)
and **jq** (inside `extract:` bindings + `nika:jq` · extraction + transform).
Reference engines use `jaq` (Rust jq) so paths behave identically everywhere.

v0.1 jq conformance subset (every engine MUST support) ·

```
.<name>                    object member
.<name>[<index>]           array index
.<name>[]                  iterate all elements (jq `.[]` · was JSONPath `[*]`)
.a.b.c                     deep path
. | map(...) | select(...) jq pipeline for reshaping / filtering
```

The subset above is the portability floor every engine MUST support. Full jq
(the `jaq` Rust impl · « full stdlib ») MAY be used **minus what the next
section refuses** (D-2026-08-11-N26 · ambient reads and the wall clock): it is
the single data extraction-and-transform language (`extract:` bindings + the
`nika:jq` builtin). ⚠️ *Full-minus-a-prose-list is a floor, not a ceiling.* The
ceiling is a named, versioned grammar — `jq-subset/0.1`, the sibling of
[`cel-subset/0.1`](./03-dag.md) — which **does not exist yet**
(D-2026-08-11-N31 creates it). Until it does, this sentence is the boundary,
and a boundary written in prose is weaker than one written in EBNF.

##### Formal grammar · jq v0.1 subset (normative · grammar version `jq-subset/0.1`)

Prose + examples are not re-implementable; this EBNF is. A conformant engine
parses exactly this grammar in an `extract:` binding and in `nika:jq` (it is a
strict subset of jq: **any full jq parser accepts every expression below**) ·

```ebnf
program  = pipeline ;
pipeline = alt , { "|" , alt } ;
alt      = cmp , { "//" , cmp } ;             (* alternative · the // operator *)
cmp      = sum , [ cmpop , sum ] ;            (* at most ONE comparison · non-associative *)
cmpop    = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
sum      = term , { ( "+" | "-" ) , term } ;
term     = unary , { ( "*" | "/" ) , unary } ;
unary    = [ "-" ] , postfix ;
postfix  = primary , { suffix } ;
suffix   = "." , IDENT
         | "[" , [ pipeline ] , "]"           (* index · [] iterates · NO slice in 0.1 *)
         | "?" ;                              (* optional · swallows the type error *)
reduce   = "reduce" , postfix , "as" , "$" , IDENT ,
           "(" , pipeline , ";" , pipeline , ")" ;   (* bounded fold · the stream IS the bound *)
format   = "@" , ( "base64" | "base64d" | "text" | "json" | "csv" | "tsv" | "uri" ) ;
primary  = "."                                (* identity *)
         | "." , IDENT
         | call | literal | object | array
         | reduce | format
         | "$" , IDENT
         | "(" , pipeline , ")" ;
call     = FUNC , [ "(" , pipeline , { ";" , pipeline } , ")" ] ;
object   = "{" , [ pair , { "," , pair } ] , "}" ;
pair     = ( IDENT | STRING ) , [ ":" , pipeline ] ;
array    = "[" , [ pipeline ] , "]" ;
literal  = NUMBER | STRING | "true" | "false" | "null" ;
```

`FUNC` is the **closed** set ·

```
length · keys · values · has · type · not · empty · error
map · map_values · select · add · join · split · flatten · range
sort · sort_by · group_by · unique · unique_by · reverse
first · last · min · max · min_by · max_by · any · all
to_entries · from_entries · with_entries · del · paths
tostring · tonumber · tojson · fromjson · getpath · setpath · leaf_paths
ascii_downcase · ascii_upcase · ltrimstr · rtrimstr · startswith · endswith
floor · ceil · fabs
```

**The set is deliberately small, and small is the safe direction**: a minor
version may only ADD (same law as `cel-subset/0.1`), so a name omitted here
costs one amendment while a name admitted early can never be withdrawn.

> ⚠️ **The first cut of this grammar refused four of THIS SPEC'S OWN canonical
> recipes** (2026-08-11 · corrected the same day). It had been verified against
> the 41 programs the corpus *uses* and never against the recipes
> [stdlib §what jq subsumes](../stdlib/builtins-v0.1.md) *recommends* —
> `reduce`, the `@base64` formats, and the `getpath`/`setpath`/`leaf_paths`
> family, which the cut-builtin table publishes as the supported way to do
> the work those builtins used to do. **A corpus of USE and a corpus of
> RECOMMENDATION are two different subjects**, and a grammar that refuses its
> own documentation would have made the spec self-contradictory the day it
> shipped. `reduce` is admitted with its stream as the bound — it folds over
> a finite stream and cannot recurse, so it terminates by construction and
> does not reopen what the table below refuses. Both corpora now pass: 40/40
> programs, 9/9 recipes.
Derived from the corpus, not invented: over **41 programs** in the shipped and
internal corpora, the functions actually used are `map · sort · last ·
fromjson · join · length`, plus paths, the pipe, object/array construction and
arithmetic. Everything above is that set plus its obvious companions.

> ### 🔴 This table is the TARGET gate, and it does not fire yet
>
> **Measured on 0.108.0, 2026-08-11.** The engine embeds a full jaq and
> the subset below is **not enforced**. A workflow whose only task is ·
>
> ```yaml
> invoke: { tool: "nika:jq", args: { input: "a LOC=120 b LOC=45",
>   expression: '[ match("LOC=([0-9]+)";"g").captures[0].string | tonumber ] | add' } }
> ```
>
> …passes `check` with **rc=0**. So do `sub` · `gsub` · `scan` · `now` ·
> `strftime`. The single production actually refused is `$ENV`, and it is
> refused by **jaq's own compiler**, not by any gate of ours.
>
> Saying so here is not a footnote: **a spec that asserts a refusal the
> engine does not make teaches a boundary that is not there.** Two rows
> of this table are security-adjacent (`now` is the ambient clock that
> N27 forbids; the regex family is the data-dependent blowup the ceiling
> exists to bound) — and a reader who trusts the table would believe both
> are already closed.
>
> The table stands as the **specification of the gate**, unchanged. What
> changes is its status: it describes what `jq-subset/0.1` MUST refuse
> when the gate ships, not what 0.108.0 refuses today. Until then the
> honest sentence is « the grammar is written, the gate is owed ».

**What the grammar refuses BY CONSTRUCTION, and why ·**

| Absent | Why |
|---|---|
| `recurse` · `..` · `while` · `until` · `repeat` | non-termination · a binding must be decidable at check |
| `def` | user-defined recursion re-introduces the same |
| `env` · `$ENV` · `input` · `inputs` · `input_filename` | ambient reads · the law above |
| `debug` · `stderr` | writes outside the value |
| `now` · `localtime` · `gmtime` · `mktime` · `strftime` | the clock · an input, never an ambient (N27) |
| `halt` · `halt_error` | process control · a data expression does not end a run |
| `test` · `match` · `sub` · `gsub` · `splits` · `scan` | regex · data-dependent blowup · deferred to a minor that pairs them with a bounded engine, not banned on principle |
| slices `.[a:b]` | omitted for 0.1 only · a minor may add |

⚠️ **A grammar bounds the SHAPE, never the MAGNITUDE.** `[range(1e9)]`
parses clean and still materialises a billion elements. That is the runtime
step budget's job (D-2026-08-11-N32), and the two ceilings are not
interchangeable: the static one buys **termination**, the runtime one buys
**size**. Neither alone is enough.

#### An expression sees only its input (normative · D-2026-08-11-N26)

> **The world of a data expression is the value it is given, and nothing
> else.** Not the process, not the clock, not the disk, not the environment.
> The control expressions of [03 §CEL](./03-dag.md) obey the same law: their
> world is the bindings they are given.

This is what makes the surrounding closure real. A data expression cannot name
a host, a path, a program or a tool — those are literal in `permits:`. It
cannot decide whether a task runs — that is CEL, and CEL is non-Turing. It
carries no effect of its own. **What remains is a transformation from a value
to a value, and that is the whole of its power.** The language is not closed
because it forbids expression; it is closed because an expression cannot
*reach* anything.

Two consequences, both normative:

- **Ambient reads are refused.** A program that reads the process environment
  (`env`, `$ENV`), the input filename, or the standard input reaches outside
  its value. ⚠️ **Measured 2026-08-11 on a shipped engine: `env.NAME` returned
  the ambient value under an absent `permits:` block and again under an
  explicit empty `permits.env`, while the static check reported the body as
  pure compute from which nothing escapes.** The reach is bounded — such a
  program is a literal in the reviewed file and cannot be interpolated from a
  model's output (`NIKA-VAR-005`) — but a computation that reads the ambient
  environment is not pure, and the certificate said it was.
- **The clock is an input, never an ambient** (D-2026-08-11-N27). `now` and
  its relatives MUST resolve to the run's start instant, which is already in
  the trace, so that a replay yields the same value forever. Reading the wall
  clock would make the same file pass on a fast machine and fail on a slow
  one — the property a replay exists to deny. A long task therefore sees the
  START instant, not its own; that is the price of determinism and it is the
  correct one.

**Why the layer is not simply removed.** The need to reshape a value does not
disappear with it: an author denied this layer reaches for `exec:` and a real
subprocess, which is strictly worse. The bounded expression layer is what
*prevents* the escape to the shell. Subtracting without replacing moves the
hole; it does not close it.

#### Binding rules (single-value · pure-jq)

- **A binding resolves to exactly ONE value.** A jq program emits a *stream*:
  `.users[]` yields N separate values, NOT an array. A binding whose program
  emits zero or multiple values is an **evaluation-time error**
  (`NIKA-VAR-002` · the emission count is data-dependent · undecidable at
  parse). The reference linter additionally WARNS at check time
  (`one-obvious-way/009`) on the statically-visible smell (a binding jq
  ending in a trailing iterator `[]` with no collecting `[ … ]` wrapper).
  A jq program that itself errors at runtime is
  `NIKA-VAR-004`. Collect a stream with `[ … ]` (`[.users[].email]` → array)
  · take one with an index (`.users[0]`) or `first(…)`. One obvious way · no
  silent first-match, no implicit array-wrap.

#### `extract:` or `nika:jq` · the choice, decided (normative for linters · 2026-08-11)

| Rule | Instead of | Write |
|---|---|---|
| `one-obvious-way/013` | `invoke: {tool: nika:jq}` reshaping **one** producer's output | an `extract:` binding on that producer |

**They are the same jq over the same value, and until now nothing said which
to reach for** — the only genuine unguarded overlap left in the language. The
rule follows from what each one IS, not from taste ·

- An `extract:` binding is **boundary work**: it costs no task, no wave and no
  node in the graph, it runs per iteration inside a `for_each`, and it carries
  a normative cardinality law (exactly one value · `NIKA-VAR-002`).
- `nika:jq` is **a task**: it has an id, a place in the DAG, its own gate and
  its own timeout — and it can read **several** producers through `with:`,
  which a binding structurally cannot, since a binding sees only the output it
  hangs from.

⇒ **One producer, reshaped: a binding. Two or more, joined: the builtin.** The
builtin stays because the many-input case is real and inexpressible otherwise;
the binding wins the single-input case because a whole task node to rename a
field is a node the graph did not need.
- **An `extract:` jq expression is pure jq over the task's raw output**: it does
  NOT contain `${{ }}` (the two expression layers never nest in one string ·
  CEL reads the namespaces · jq reads the task output). To parametrize an
  extraction by a workflow value, shape the verb's *input* with `${{ }}` ·
  the jq then runs over the result. (Exposing the read namespaces as jq
  variables, `.items[] | select(.id == $inputs.target)`, is a v0.2 candidate ·
  jq-native · additive · NOT in the v0.1 subset.)

---

## Resolution order

When a task is admitted · the engine resolves `${{ ... }}` references in this order ·

1. **Boundary first** · the `with:` bindings materialize (their `tasks.X.field` references read the settled records — this is where the data edges deliver)
2. **`when:`** evaluates over the local namespaces (`inputs` · `const` · `with` · loop locals)
3. **Body** · verb-field expressions resolve (`inputs.X` · `const.X` · `secrets.X` · `with.X` · loop locals — never `tasks.*`)
4. **Single-pass** · a substitution result is NOT re-evaluated (no nested substitution)

If a reference is unresolved · the engine raises a `NIKA-VAR-001` (undefined
variable) error — at the boundary (steps 1-2) it settles the task `failure`
with `on_error:` NOT consulted; in the body (step 3) it is task-stage work,
recoverable by `on_error:` ([03 §task states](./03-dag.md#task-states)).

### Value rendering · object → string

When a `${{ }}` reference resolves to an **object or array** and is substituted
into a **string position** (e.g. inside a `prompt:` or `command:`), it renders
as **compact JSON** · deterministic (object keys sorted · no insignificant
whitespace). Scalars render as their natural string (numbers · booleans ·
`null` → `null`). There are **no template pipe-filters** (`${{ x | json }}` is
NOT a thing · per the §locked substitution surface). To control the rendering,
extract a string with jq in `extract:` (`@json` for JSON text · `tostring` /
`@text` for scalar coercion) and reference that binding. One obvious way ·
implicit compact-JSON by default · explicit jq when you need a specific shape.

A **bytes** output (tool-determined · e.g. MCP image content · a binary
`nika:read`) is **opaque** · it flows tool→tool by reference (a `with:`
binding of `${{ tasks.fetch_img.output }}` → another tool's `content:` arg ·
or a file path for `infer.vision`). Bytes **cannot** be jq-extracted (jq is JSON-only)
nor substituted into a string position: that is an error (`NIKA-VAR-007`) ·
the engine never silently UTF-8-coerces a blob (it would corrupt the data).
For `nika:fetch` and `exec` (no binary value channel · the 9 fetch modes are
text/JSON · `raw` is text), binary is **file-mediated** · write to a path,
then read or reference the path. There is no `output_format` field · the
value carries its own type.

---

## Escaping

To embed a literal `${{` in a string · use `\${{` (backslash escape). The engine MUST honor this.

```yaml
infer:
  prompt: "The syntax \\${{ inputs.x }} is how you reference variables."
```

(Note · YAML escaping of backslash · `\\` in double-quoted strings · `\` in single-quoted or block scalars.)

**Backslash runs (normative)** · the escape counts the CONTIGUOUS backslash
run immediately before `${{` · an **odd** run escapes the opener (the island
is literal text; the escaping backslash is consumed) · an **even** run
(including zero) leaves the island **live**. Within that run, each remaining
backslash PAIR renders as one literal backslash; backslashes anywhere else
are ordinary characters (there is no general backslash processing). So
`\${{ x }}` renders the literal `${{ x }}` · `\\${{ x }}` renders one `\`
followed by the RESOLVED island · `\\\${{ x }}` renders one `\` + the
literal `${{ x }}`.

An **unclosed `${{`** (an unescaped opener with no closing `}}`) is rejected at
parse time · `NIKA-VAR-008` · `validation_error`: the substitution surface belongs
to this section, even though the YAML itself parses fine.

---

## Why one syntax everywhere

An earlier draft proposed two syntaxes (`$task_id` in value positions · `{{var}}` inside strings). That was a confusion source · v0.1 unifies on a single `${{ }}` syntax for everything.

Reasons ·

- **One mental model** · same syntax everywhere · low cognitive load (Rams principle 4 understandable)
- **GitHub Actions familiarity** · 30M+ developers already know `${{ ... }}`
- **Composable in any position** · strings · object values · array elements · conditions
- **Unique enough** · escape rarely needed
- **Future-proof** · GitHub Actions has extended this syntax for 8+ years without breaking change

---

## Forward-compat

The `${{ ... }}` substitution surface and the <!-- canon:namespaces -->5<!-- /canon --> namespaces are locked at v1. **Template pipe-filters (`${{ inputs.x | json }}` · `| upper`) are NOT a growth path** (they would duplicate builtins + push CEL toward a string-DSL). Data transforms live in the `nika:jq` builtin; the `${{ }}` surface grows only with CEL-native features: the conditional `?:`, the `has()` presence macro, and the `contains`/`startsWith`/`endsWith` string tests ship in `cel-subset/0.1` ([03 §grammar](./03-dag.md)); `all`/`exists` and `matches()` regex stay reserved for a later additive minor. jq is the single extraction-and-transform language (`extract:` + `nika:jq`).

Out of scope for v0.1 (deferred · see [`08-out-of-scope.md`](./08-out-of-scope.md)) ·
- Expression language (no arithmetic in templates)
- User-defined functions in templates
- Multi-pass substitution

---

🦋 *Next · [05 · Errors](./05-errors.md)*
