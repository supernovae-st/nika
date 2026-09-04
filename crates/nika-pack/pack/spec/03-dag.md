# 03 · The flow — four graphs

> A Nika workflow is a **Directed Acyclic Graph** of tasks. Each node is a
> task (one of the 4 verbs). Each edge is **typed** and **derived from a
> declaration**: a `with:` binding is a *data* edge · an `after:` entry is a
> *control* edge. There is no third way to connect two tasks.
>
> Since W2 « the flow », `tasks.*` crosses a task boundary through exactly
> two doors — `with:` (data · observations) and `after:` (control) — and the
> engine computes the graph FROM those doors. `depends_on` is dead
> (`NIKA-PARSE-024` · `nika check --fix` migrates it).

---

## Minimal DAG

```yaml
tasks:
  a:
    infer:
      prompt: "First"

  b:
    with:
      prev: ${{ tasks.a.output }}     # ← the binding IS the edge (a → b · value)
    infer:
      prompt: "Second, after a · ${{ with.prev }}"
```

`b` runs after `a` settles. The `with:` binding both **names the data** the
task consumes and **declares the edge** that orders it: data and its
dependency are one declaration · no invisible edges.

For an ordering with **no data** (run the deploy after the tests, consume
nothing), use `after:` ·

```yaml
  deploy:
    after:
      tests: success        # ← control edge · state, never data
    exec:
      command: ["./deploy.sh"]
```

---

## Task shape · full

```yaml
my_task:                        # the map KEY is the identity · snake_case · unique
  with:                         # optional · the DATA boundary · each tasks.* ref = one typed edge
    data: ${{ tasks.task_a.output }}
    opts: { foo: "bar" }        # literals are fine — only tasks.* refs create edges
    legs: ${{ group.probes }}   # a fan-in fold · one edge per declared member
  group: probes                 # optional · fan-in MEMBERSHIP · this task joins the set
  after:                        # optional · the CONTROL boundary · {producer: predicate}
    task_b: success             # predicate ∈ success | failure | skipped | terminal | unwind
  when: ${{ inputs.enabled }}   # optional · LOCAL business condition · evaluated POST-gate
  for_each:                     # optional · map this task over a collection (local namespaces)
    items: ${{ with.pages }}    #   the collection · evaluated ONCE, before the fan-out
    max_parallel: 5             #   optional · cap concurrent iterations
    fail_fast: false            #   optional · false = finish the batch
  retry:                        # optional · retry policy (see 05-errors.md)
    max_attempts: 3
    backoff_ms: 1000
  on_error:                     # optional · error recovery (see 05-errors.md)
    recover: ${{ tasks.cache.output }}
  timeout: "60s"                # optional · task-level timeout (Go duration string)
  infer:                        # required · one of the 4 verbs
    prompt: "... ${{ with.data }} ..."
  returns:                      # optional · the OUTPUT CONTRACT · the type expression, INLINE (09-types.md)
    object:
      summary: string
  extract:                      # optional · named jq bindings
    result: ".choices[0].message.content"
    tokens: ".usage.total_tokens"
```

---

## Field-by-field

### the task key · **the identity · snake_case · unique**

```yaml
tasks:
  research_topic:
    ...
```

Since W1 « the map », a task's identity IS its map key — there is no `id:`
field (a lingering one is `NIKA-PARSE-023`; a `tasks:` sequence is
`NIKA-PARSE-022`). Keys match `^[a-z][a-z0-9_]*$` (snake_case · no hyphens);
a duplicate key is refused by the YAML layer itself (PARSE-007 mechanics).
Source order is presentation only — the graph alone schedules.

**Why snake_case, not kebab** · task ids are referenced in CEL expressions as
`tasks.<id>.output`. In CEL (and almost every expression language) a hyphen is
the **subtraction operator**: `tasks.research-topic.output` would parse as
`tasks.research - topic.output`, a silent trap. Snake_case ids are always
clean CEL identifiers. (The file-level `nika:` name stays kebab-case:
it is a resource name, never referenced inside an expression.)

### `with` · *optional · the DATA boundary — bindings that ARE edges*

> **A name collision worth knowing about.** In Nix, `with` is the
> canonical *hidden-dependency* construct: `with pkgs;` pulls an
> unknown set of names into scope, and a reader cannot tell where an
> identifier came from without evaluating the set. nika's `with:` is the
> **exact opposite** — every binding is named at the boundary, and each
> one *creates a visible edge*. Same token, inverted property. If you
> arrive from Nix, read this block as `let … in`, not as `with`.

```yaml
summarize:
    with:
      article: ${{ tasks.fetch.output }}     # value edge        · fetch → summarize
      took_ms: ${{ tasks.fetch.duration_ms }}  # terminal-observation edge
      style: "concise"                       # literal · no edge
    infer:
      prompt: "Summarize (${{ with.style }}) · ${{ with.article }}"
```

`with:` is where a task **imports the outside world**. Every
`${{ tasks.X.<field> }}` reference inside a `with:` value creates one
**static, typed edge** `X → this-task`, and the edge's *role* follows the
field's shape ·

| referenced field | edge role | the edge admits this task when X settles… |
|---|---|---|
| `.output` · `.<named binding>` | **value** | `success` **or** `skipped` (the value of a skipped task is defined-`null` · [04](./04-variables.md#defined-null-reads-normative--the-branch-join-unlock)) |
| `.status` · `.duration_ms` · `.started_at` · `.ended_at` | **terminal-observation** | **any** terminal state (`success` · `failure` · `skipped` · `cancelled`) — you asked to OBSERVE the outcome, so every outcome admits |
| `.error` | **failure-observation** | `failure` **or** `skipped` — a skip may carry a PRESERVED error (`on_error: skip` · [05 §Fields](./05-errors.md)); a decision-skip's error reads defined-`null`. A recovered task settles `success` · the edge does not admit |

An expression with N references creates **N edges** (the graph is what CAN
be required · the trace records what actually was). Two edges from the same
producer may carry two different roles — the roles compose (§gate algebra).

The task's **body consumes its bindings** — `${{ with.article }}` — never the
global `tasks.*` namespace: `with:`/`after:` are the only doors
([04 §the reference boundary](./04-variables.md#the-reference-boundary--where-tasks-may-appear) ·
`NIKA-VAR-021` teaches the hoist and `nika check --fix` applies it).

**Binding materialization is boundary work, not task work (normative).**
Once the gate admits the task, its `with:` bindings evaluate. A binding whose
evaluation errors (an unresolvable path · a cross-type operation · any
`NIKA-VAR` evaluation error) settles the task **`failure` — its `on_error:`
is NOT consulted**: `on_error`/`retry` govern the *verb run*, not the
boundary. (Same law as a `when:` evaluation error · §Task states.)

### `after` · *optional · the CONTROL boundary — state, never data*

```yaml
deploy:
    after:
      tests: success          # run only if tests settled success
      scan: success
    exec:
      command: ["./deploy.sh"]
```

`after:` is a map `{producer-task: predicate}`. Each entry creates one
**control edge** whose predicate names the producer states that admit this
task ·

| predicate | admits when the producer settles… |
|---|---|
| `success` | `success` |
| `failure` | `failure` |
| `skipped` | `skipped` |
| `terminal` | any terminal state — `success` · `failure` · `skipped` · **`cancelled`** (the always-pattern: « run once X is settled, whatever happened » · cancelled IS terminal) |

The predicate set is **closed** (an unknown predicate is `NIKA-DAG-005`) and
an `after:` target must be a declared task (`NIKA-DAG-002`).

`after:` carries **no data**: the body cannot read the producer through it.
To *branch on* an outcome, observe it through `with:` — the pairing is
idiomatic ·

```yaml
report:
    after:
      pipeline: terminal                       # run whatever happened…
    with:
      outcome: ${{ tasks.pipeline.status }}    # …and OBSERVE what happened
    infer:
      prompt: "Write the run report · pipeline ended ${{ with.outcome }}"
```

(The `.status` binding is a terminal-observation edge — same pass-set as
`after: terminal`, so the two edges agree. §gate algebra makes this
composition law precise.)

**Do not restate a `with:` edge.** An `after:` entry on a producer you
already bind through `with:` is meaningful ONLY if it *tightens* the gate
(`after: {x: success}` + a value edge = run on `success` only, excluding
the skipped-`null` case). A non-tightening restatement (`after: {x: terminal}`
next to a value edge) changes nothing and the reference linter warns
(`one-obvious-way/010`).

### `group` · *optional · fan-in MEMBERSHIP — the plural of a data edge*

```yaml
tasks:
  leg_hygiene:  { group: probes, exec: { command: ["./hygiene.sh"] } }
  leg_machete:  { group: probes, exec: { command: ["./machete.sh"] } }
  leg_licenses: { group: probes, exec: { command: ["./licenses.sh"] } }

  summary:
    with:
      legs: ${{ group.probes }}          # ← ONE binding · every member folded
    invoke:
      tool: nika:jq
      args:
        input: "${{ with.legs }}"
        expression: 'map(select(.output.exit_code != 0)) | {failed: length}'
```

`group:` names a set this task **joins**. A consumer folds the whole set with a
single `${{ group.<name> }}` binding in its `with:`, and gets **one array of
member records**. It is not a third way to connect two tasks — it is the
**plural of the data edge already described above**: each member contributes
exactly one edge, derived exactly as a `with:` reference is.

**Why the language needed a plural.** The three fan-in spellings an author
reaches for first are all refused, and none of the refusals teaches a way
forward ·

```yaml
after: { "leg_*": success }              # NIKA-DAG-002 — read as a literal task name
with:  { l: ${{ tasks.leg_*.output }} }  # NIKA-VAR-005 — `*` is not in the CEL subset
with:  { l: ${{ tasks }} }               # the namespace is not a value
```

So the fold moves out of the language and into a shell script beside it — and
the moment it does, it stops being checkable, stops appearing in the graph, and
stops being covered by any law in this chapter. Measured on the studio's own
ledger workflows: the summary task alone carried **31 % of the file** as
hand-listed bindings, `after:` entries and argv, for a fold the graph could
have derived.

#### Declared membership, never a pattern (normative)

A group **exists iff at least one task declares it**. There is no glob, no
prefix match, no regex. That is the load-bearing choice, and the reason is a
failure mode, not taste ·

- **A rename must be an ERROR, not a smaller fold.** With `leg_*`, renaming
  `leg_licenses` to `license_check` silently drops it from every ledger that
  globbed it — the run stays **green** while covering less. With a declared
  name, the member simply stops declaring `group: probes`, and any reference
  to a group nobody declares is **`NIKA-DAG-008`**. The failure is loud and
  it lands at check time, before the ledger lies.
- **An empty group is the same fact as an absent one**, so one code covers
  both: a fold can never harvest zero members and read as clean.
- **The reference boundary is untouched.** `group.<name>` is legal in a
  `with:` value and **nowhere else** — one door, where `tasks.*` has five.
  `when:`, verb bodies, `on_error.recover:`, `extract:` and workflow
  `outputs:` all refuse it with the existing `NIKA-VAR-021`
  ([04 §the reference boundary](./04-variables.md#the-reference-boundary--where-tasks-may-appear)).
  Nothing in that law needed widening to admit the fold.
- **The graph stays statically derivable**, so every law in this chapter
  survives unchanged: members are edges, so a task folding a group it belongs
  to is a 1-cycle (`NIKA-DAG-001`) with no special case; the projection sees
  the fan-in the way it sees any other edge; wave scheduling is unchanged.

`group` is **not an extra namespace**. It is the plural reader of the `tasks`
runtime namespace — same family, stricter placement. The three value
authorities (`inputs` · `const` · `secrets`) and the namespace
count are both untouched ([04 §the namespaces](./04-variables.md)).

#### The member record (normative · closed at v1)

Each member contributes one record, and the array is ordered by **declaration
order** — the source order of the `tasks:` map, not completion order. Source
order does not schedule anything (§the task key); it is used here only to make
the fold's shape **deterministic and stable across re-runs**, which completion
order would not be.

```
[ { id, status, output, duration_ms, error }, … ]
```

| field | what it carries |
|---|---|
| `id` | the member's task key — the only field a fold cannot reconstruct from the others |
| `status` | `success` · `failure` · `skipped` · `cancelled` (the closed enum) |
| `output` | the member's output · defined-`null` when it did not settle `success` |
| `duration_ms` | integer milliseconds |
| `error` | the typed error record · defined-`null` when there is none |

**`status` and `output` are both required, and neither is redundant** — this is
the subtle half of the contract. `status` alone is not enough: a member
carrying `on_error: { skip: true }` settles **`skipped`**, so a leg that
genuinely failed is indistinguishable by status from a leg that was never
meant to run, and the red/green fact survives **only inside the output** (an
`exit_code`, a body, a count). `output` alone is not enough either: it reads
defined-`null` for a cancelled member, which says nothing about why. A fold
that judges a ledger reads both.

The record set is CLOSED at v1; adding a field is a spec minor, exactly as for
the `tasks.X` projection set.

#### The fan-in edge · role and pass-set (normative)

A `${{ group.<name> }}` binding creates **one edge per member**, role
**`fan-in`**, joining `E_d` (§the four graphs) ·

```
fan-in edge               {success, failure, skipped, cancelled}
```

The pass-set is **all four settled states** — the same as
`after: { x: terminal }` and a `.status` observation. It is deliberately NOT
the intersection of its members' field roles: a value edge admits on
`{success, skipped}` and a failure-observation on `{failure, skipped}`, so an
intersection would leave `{skipped}` and every fold would be
**`NIKA-DAG-006`-dead on arrival**. The fold is a *terminal observation of
each member*, which is what a report is: **it runs whatever happened.** The
per-member truth lives in the record, not in the gate.

Consequence, stated plainly: a fan-in edge can never be the thing that proves
a task dead. A member that can only settle `{skipped · cancelled}` (the
documented `when: false` never-pattern) still admits the fold — where
`after: { that_member: success }` on the same producer would be `NIKA-DAG-006`.
That contrast is the whole value of the construct.

#### The rest of the contract

- A member that fans out (`for_each`) contributes its positional array as
  `output`, under the observability rule in §for_each semantics.
- An **`unwind` task may not join a group** (`NIKA-DAG-009`): cleanup is an
  `E_f` attachment that never enters `G_p`, so a fan-in edge from it would
  have no wave to schedule against.
- A group name matches `^[a-z][a-z0-9_]*$`, like a task key. A group name and
  a task key MAY coincide — the roots disambiguate structurally
  (`group.probes` vs `tasks.probes`), the same argument that lets a task be
  named `item` (§shadowing, 04).
- A bare `${{ group }}` names no group and is `NIKA-DAG-008`.
- `group:` declares membership only. It carries no predicate, no ordering and
  no data — everything about *what admits* lives in the pass-set above, and
  everything about *what is read* lives in the record.

### `depends_on` · **dead — the teaching survives**

`depends_on` died in W2 « the flow ». It conflated three intents the
language now spells ·

| the old spelling meant… | the W2 spelling |
|---|---|
| « B consumes A's output » | a `with:` binding — the data IS the edge |
| « B runs only after A worked » (no data) | `after: { a: success }` |
| « B runs once A is settled, whatever happened » (the `when: true` pattern) | `after: { a: terminal }` |

A task carrying `depends_on:` is refused at parse time (`NIKA-PARSE-024` ·
`validation_error`) and `nika check --fix` migrates it mechanically **when
the observable behavior is provably unchanged** — the ambiguous cases
(a producer that may skip · a `when:` that used to replace the gate · a
status-only reference · an output read on a producer that may settle
skipped · a complex expression) produce a diagnostic with the candidate
rewrites and their semantic deltas, and STOP for a human decision: the
codemod is *equivalence-or-stop*, it never guesses.

**The one semantic the old form cannot express anymore** · a bare
`depends_on: [a]` on a producer that may settle `skipped` admitted on
`{success, skipped}` with no data read. W2 makes you choose: consume the
value (`with:` · keeps `{success, skipped}` · the skipped value is `null`) ·
require success (`after: {a: success}` · a skipped producer now cancels
you) · or accept every outcome (`after: {a: terminal}`). Choosing is the
point — the old spelling hid the choice.

### `when` · *optional · LOCAL business condition · evaluated POST-gate*

```yaml
notify:
    with:
      warnings: ${{ tasks.build.output.warnings }}
    when: ${{ with.warnings == 0 }}            # local read · the edge came from with:
    exec:
      command: ["./notify.sh"]
```

`when:` decides **whether an admitted task runs**. It is evaluated *after*
the gate (§gate algebra) and it reads **local namespaces only** ·
`inputs` · `const` · `with` · and the `for_each` locals `item` / `index`.
A `tasks.*` reference inside `when:` is refused at parse time
(`NIKA-VAR-021` · « hoist it into `with:` » — the binding creates the edge,
`when:` reads the binding).

- `when:` evaluates `false` → the task settles **`skipped`** (never
  `cancelled` — skipped is a *decision*, cancelled is a *dead path*).
  Downstream value edges pass on skipped (their bindings read `null`).
- `when:` evaluates `true` → the verb runs.
- `when:` is NOT a gate replacement. The pre-W2 « an explicit `when:`
  replaces the default gate » law is dead: the gate always applies, `when:`
  refines it. The old always-pattern (`when: true` to run on a failed
  upstream) is now `after: { x: terminal }` — visible in the graph, not
  smuggled through a condition.

#### Expression language · a documented subset of CEL

Everything inside `${{ ... }}` (both value substitution and `when:`
conditions) is **[CEL](https://cel.dev) (Common Expression Language)**, the
validated, non-Turing-complete, side-effect-free expression standard used by
Kubernetes (ValidatingAdmissionPolicy), Kyverno, Envoy, and gRPC. Nika does
**not** invent an expression DSL: it adopts the standard. (This supersedes the « custom minimal DSL » framing.)

**Why CEL** · it is *common* (millions of K8s users), *comprehensible*
(reads like a boolean expression), *validated* (a published spec + multiple
conformant implementations), *safe* (non-Turing-complete · bounded · no side
effects) and *portable* (zero parser drift between engines). A hand-rolled
DSL would be none of those.

**The v0.1 subset** (the only CEL features a conformant engine must support) ·

```
identifier / field access   inputs.topic · with.content · item.url
index access                with.pages[0] · obj['key-with-dash']
comparison                  == · != · < · <= · > · >=
boolean                     && · || · !
membership                  in            (e.g. with.status in ['success','skipped'])
callables                   size(coll) · has(ref) · coll.size() · str.contains(s) · str.startsWith(s) · str.endsWith(s)
literals                    true · false · 42 · 3.14 · 'str' · "str" · null
grouping                    ( … )
```

The callable set is closed: `size(x)` · `has(x)` · `x.size()` ·
`x.contains(s)` · `x.startsWith(s)` · `x.endsWith(s)`. Arithmetic, the
`all()` / `exists()` macros, `matches()` regex, and every other callable are
**reserved** for a later additive minor. If you need richer logic today,
compute it in a `nika:assert` builtin or an `infer:` task.

##### Formal grammar · CEL v0.1 subset (normative · grammar version `cel-subset/0.1`)

Prose + examples are not re-implementable; this EBNF is. A conformant engine
parses exactly this grammar inside `${{ }}` (it is a strict subset of
[cel-spec](https://github.com/google/cel-spec): any full CEL parser accepts
every expression below) ·

```ebnf
expr     = ternary ;
ternary  = or , [ "?" , expr , ":" , ternary ] ;   (* conditional value · cond MUST be boolean ·
                                                      right-associative · `a ? b : c ? d : e` =
                                                      `a ? b : (c ? d : e)` · loosest precedence *)
or       = and , { "||" , and } ;
and      = rel , { "&&" , rel } ;
rel      = unary , [ relop , unary ] ;        (* at most ONE relation · non-associative ·
                                                 `a < b < c` is a parse error *)
relop    = "==" | "!=" | "<" | "<=" | ">" | ">=" | "in" ;
unary    = { "!" } , postfix ;
postfix  = primary , { "." , IDENT , [ "(" , [ expr ] , ")" ]
                     | "[" , expr , "]" } ;
primary  = literal | list | call | IDENT | "(" , expr , ")" ;
call     = ( "size" | "has" ) , "(" , expr , ")" ;
list     = "[" , [ expr , { "," , expr } ] , "]" ;
literal  = INT | FLOAT | STRING | "true" | "false" | "null" ;

IDENT    = /[A-Za-z_][A-Za-z0-9_]*/ ;          (* `true·false·null·in` are reserved words *)
INT      = /-?[0-9]+/ ;
FLOAT    = /-?[0-9]+\.[0-9]+/ ;
STRING   = /'([^'\\]|\\.)*'/ | /"([^"\\]|\\.)*"/ ;   (* escapes · \\ \' \" \n \t *)
```

**Side constraints (normative)** ·

1. **The callables are a CLOSED set** · the free functions `size(x)` and
   `has(x)` (each exactly 1 argument); the zero-arg method `x.size()`; and the
   one-arg string methods `x.contains(s)` · `x.startsWith(s)` · `x.endsWith(s)`
   (substring / prefix / suffix tests · case-sensitive · operands MUST be
   strings). `has(x)` is the presence macro · `true` iff the reference `x`
   resolves to a defined, non-`null` value (the safe way to test an optional
   field before reading it · never raises `NIKA-VAR-001`). **No regex**:
   `matches()` is reserved (ReDoS surface · a later minor). Any other call
   suffix is a parse error.
2. **Precedence** (tightest → loosest) · postfix (`.` `[]`) → `!` → relational
   (`==` `!=` `<` `<=` `>` `>=` `in`) → `&&` → `||` → ternary (`?:`).
   Parentheses override. The ternary `cond ? a : b` requires a **boolean**
   `cond` (a non-boolean condition is `NIKA-VAR-006`) · `a` and `b` may be any
   value and need not share a type: it is value-selection, not a relation, so
   it does NOT count against the one-relation rule.
3. **Relations do not chain** · `rel` admits at most one `relop`
   (non-associative): `a == b == c` must be written `(a == b) == c` if that
   is really meant.
4. **No implicit coercion** · the subset is strongly typed per CEL ·
   comparing values of different types (`42 == "42"`) is an evaluation error
   (`NIKA-VAR` · `variable_error`) · not `false`. (`null` is the one
   universal comparand · `x == null` / `x != null` are legal against any
   type — the defined-null law of [04](./04-variables.md) depends on it.)
5. **`when:` is boolean** · statically-non-boolean-SHAPED roots (a bare
   string/number literal · a bare reference with no relation or boolean
   operator) MUST be rejected at parse time (`NIKA-VAR-005` ·
   `validation_error`); an expression that passes the static shape check
   but evaluates non-boolean fails at evaluation (`NIKA-VAR-006` ·
   `variable_error`). See §`when:` shape rules below.
6. **Identifier roots resolve against the namespaces** · the 5 global
   namespaces (`inputs` · `const` · `secrets` · `with` · `tasks`)
   plus the two
   `for_each` loop-locals (`item` · `index`) per
   [04-variables.md](./04-variables.md) §Resolution order — and the `tasks`
   root is legal ONLY on the boundary surfaces
   ([04 §the reference boundary](./04-variables.md#the-reference-boundary--where-tasks-may-appear) ·
   elsewhere it is `NIKA-VAR-021`) · an unresolvable root is `NIKA-VAR-001`.

The grammar is versioned (`cel-subset/0.1`) · later minors may only ADD
productions (arithmetic · `matches()` regex · further macros), never change
the meaning of an expression that parses today. The conditional `?:`, the
`has()` macro, and the `contains`/`startsWith`/`endsWith` string tests are IN
`cel-subset/0.1` (they are standard CEL · any full CEL parser accepts them).

**Conditional value selection (the common shape)** · `?:` is what lets a
*value* field branch without a `nika:jq` detour ·

```yaml
# pick a model / a path / a prompt by condition — anywhere a value is taken
model:  ${{ inputs.env == 'prod' ? 'mistral/mistral-large' : 'ollama/qwen3.5:9b' }}
prompt: ${{ has(inputs.style) ? inputs.style : 'be concise' }}
when:   ${{ with.scan_log.contains('ERROR') }}      # branch on substring · the log arrived via with:
```

**Namespaces are CEL variables** · the <!-- canon:namespaces -->5<!-- /canon --> namespaces (`inputs`
· `const` · `secrets` · `with` · `tasks`) are bound as top-level CEL variables — `tasks.*` on the
boundary surfaces only. **Inside a `for_each` task body, two
more scoped CEL variables are bound** · `item` (the current element) and `index`
(its 0-based position), available ONLY within that task (the <!-- canon:namespaces -->5<!-- /canon --> namespaces are
global · `item`/`index` are for_each-local · see `for_each` below).

#### The binding is the edge — no invisible edges

Pre-W2, a `tasks.X` reference anywhere required a matching `depends_on`
declaration and a missing one was an error (the retired `NIKA-DAG-003`
class). W2 removes the double bookkeeping in both directions: a `tasks.X`
reference is **legal only where it declares an edge by existing** (`with:` ·
`after:`) or reads a settled record on a declared surface (`on_error.recover`
· an `unwind` task · workflow `outputs:`). The engine never infers a hidden
edge and never asks you to restate a visible one — **the binding IS the
edge · no invisible edges** · and a reference outside those surfaces is
`NIKA-VAR-021` with a machine-applicable fix (hoist into `with:`).

```yaml
# ❌ REJECTED at parse — the verb body reads the global namespace
deploy:
    exec: { command: ["./deploy.sh", "${{ tasks.build.output }}"] }

# ✅ CORRECT — the boundary imports · the body consumes the binding
deploy:
    with:
      artifact: ${{ tasks.build.output }}
    exec: { command: ["./deploy.sh", "${{ with.artifact }}"] }
```

**Why a boundary, not free references** · a reference buried in a prompt is
an invisible dependency: it makes the DAG unreadable and couples the body's
text to the graph's shape. The boundary keeps every import visible at the
top of the task, gives the edge a NAME (`with.artifact` — renameable,
hoverable, typed in W3), and makes the body a pure function of its declared
inputs. (This is the one rule an LLM most often gets wrong, so the fix is
machine-applicable: `nika check --fix` hoists the reference for you.)

**Implementation** · an engine MAY embed a CEL library (e.g. the Rust
`cel-interpreter` crate) OR hand-roll the small v0.1 subset above: both are
conformant because the subset is exactly CEL. The Core conformance suite tests
the subset against the CEL spec.

A `when:` expression evaluates to a boolean. If `false`, the task is
**skipped** (not failed) · status `skipped` · downstream value edges pass.

Common patterns ·

```yaml
when: ${{ inputs.env == 'production' }}
when: ${{ with.coverage > 80 }}                       # the number arrived via with:
when: ${{ size(with.findings) > 0 }}
when: ${{ has(inputs.style) && inputs.style != 'none' }}
when: ${{ item.kind == 'article' }}                   # for_each-local
```


### `when:` shape rules · boolean-only · one rule, two enforcement times

```yaml
send_alert:
    with:
      alert_count: ${{ tasks.check.output.alert_count }}
    when: ${{ with.alert_count > 0 }}     # CEL expression evaluating to bool
    invoke: { ... }
```

`when:` accepts exactly two forms · a **`${{ }}` CEL expression** (the general
case) or the **YAML boolean literal `true` / `false`** (`when: false` is the
never-run switch; `when: true` restates the default and the linter warns).
Anything else is rejected.

**Parse time (MUST · `NIKA-VAR-005` · `validation_error`)**: statically
non-boolean-SHAPED roots are rejected before any execution ·
```yaml
when: ${{ inputs.threshold }}                  # ❌ bare reference · no relation/boolean operator
when: ${{ with.report }}                       # ❌ bare reference
when: ${{ 'production' }}                      # ❌ bare literal
when: "literal string"                          # ❌ neither ${{ }} nor a YAML boolean
```

**Evaluation time (`NIKA-VAR-006` · `variable_error`)**: an expression whose
*shape* is boolean but whose runtime value is not (a typed comparison across
types · a reference that resolves non-boolean through an operator the static
pass could not see) fails when evaluated.

For non-boolean values · use explicit comparison ·
```yaml
when: ${{ inputs.threshold > 0 }}              # explicit > comparison
when: ${{ inputs.message != "" }}              # empty string check
when: ${{ size(inputs.items) > 0 }}            # collection size check
```

---

### `for_each` · *optional · fan this task out over a collection*

```yaml
scrape_all:
    with:
      pages: ${{ tasks.discover.pages }}         # the collection crosses the boundary here
    for_each:
      items: ${{ with.pages }}                   # a local read · a literal list also works
      max_parallel: 5                            # optional · cap concurrent iterations · default unbounded
      fail_fast: false                           # optional · false = keep going on errors · default true
    invoke:
      tool: nika:fetch
      args: { url: "${{ item }}", mode: article }
```

> **One block, and that is the whole point.** `max_parallel` and
> `fail_fast` were once task-level fields. Measured over the corpus they
> appeared **45 and 39 times, and ZERO times without `for_each`** — they
> never had an autonomous existence, so they were sub-fields wearing the
> costume of fields. Folding them in costs the language two fields and
> buys something the old shape could not: **the concurrency is declared
> where the fan-out is**, so `for_each:` no longer reads like a
> sequential loop with unrelated knobs beside it. There is no bare
> `for_each: <expr>` form — one construct, one spelling.

`for_each` runs the task **once per element** of the collection. Inside the
task body, `${{ item }}` resolves to the current element (and `${{ index }}`
to its zero-based position). The collection is a literal list, an `inputs.*`
list, or an upstream array imported through `with:` — the **matrix /
fan-out** pattern familiar from GitHub Actions.

**The collection expression is a pre-fan-out surface (normative)** · it is
evaluated ONCE, before any iteration exists, so `item` / `index` are not in
scope there — directly, or transitively through a `with:` binding it reads
(a `for_each:` that reads `with.X` where binding `X` itself references
`item`/`index` is circular and rejected statically · `NIKA-VAR-005`). Like
every body surface it reads local namespaces only — an upstream collection
crosses through `with:` (`NIKA-VAR-021` teaches the hoist).

#### ⚠️ Parallel by default

By default · `for_each` iterations run **in parallel** (engine spawns all
iterations concurrently · bounded by `for_each.max_parallel:` if set).

This is **different from Python's sequential `for` loop**. If you need
sequential iteration · set `max_parallel: 1` ·

```yaml
process_in_order:
    for_each:
      items: ${{ inputs.items }}
      max_parallel: 1                            # iterations run one-at-a-time, in order
    exec:
      command: ["process", "${{ item }}"]
```

#### `for_each.max_parallel:` · *optional · cap concurrent iterations*

```yaml
for_each:
  items: ${{ inputs.urls }}    # 1000 URLs
  max_parallel: 5              # at most 5 in-flight at any time
```

- **Default · unbounded** (subject to engine-wide concurrency budget · v0.3
  daemon adds workflow-level cap).
- **Positive integer** · `1` to `n`. `1` = sequential.
- **Engine impl** · `tokio::sync::Semaphore` (or equivalent) · iterations
  acquire a permit before executing · release on completion.
- **Use cases** · rate-limiting provider APIs · avoiding resource
  exhaustion · compliance with concurrency limits.

#### `for_each.fail_fast:` · *optional · abort-on-error policy*

```yaml
for_each:
  items: ${{ inputs.urls }}
  fail_fast: false             # default true · false = process all even if some fail
```

- **Default · `true`** · first iteration error aborts remaining iterations ·
  parent task transitions to `failure` status immediately.
- **`fail_fast: false`** · iteration errors are collected · remaining
  iterations keep running · parent task transitions to `failure` (with
  per-iteration error details) ONLY after all iterations complete.
- **`on_error:` runs BEFORE `fail_fast` sees an iteration** · a task-level
  `on_error: { recover: … }` settles every failed item as `recovered`, so
  `fail_fast: true` under a blanket `recover` has no observable effect
  (measured on the reference engine · byte-identical journals). `fail_fast`
  is the batch's stop policy; `on_error` is the item's outcome · never both
  for the same intent.
- **Every item's terminal is recorded** · the fan-out's terminal frame
  carries `items` (one row per item in input order · `index` · `item` ·
  `status` ∈ `ok` · `recovered` · `failed` · `never_started` · `code` and
  `message` when an error was recorded) · see [17 §the kind vocabulary](./17-trace.md#the-kind-vocabulary-normative--closed-per-minor).
- **Use cases** · « process N URLs · report which failed but don't abort »
  (false) vs « if any LLM call fails, the whole batch is invalid » (true).

#### Semantics (closed at v1)

- **Every expression in the task body is re-evaluated PER ITERATION** with
  `item`/`index` bound: `with:`, the verb fields (`prompt:` · `command:` ·
  `args:` · …), `when:`, AND the `extract:` bindings. (A binding that does
  not reference `item`/`index` evaluates to the same value every iteration —
  expressions are pure over settled state — so an engine MAY materialize it
  once; the observable behavior is identical.) The only expression evaluated
  strictly once is the `for_each:` collection itself (pre-fan-out surface ·
  above).
- The task's output is the **array of per-iteration outputs**, in input
  order · referenced downstream as `${{ tasks.scrape_all.output }}`
  (an array) · `${{ tasks.scrape_all.output[0] }}` for one element.
- **`extract:` bindings apply per iteration**: each binding's jq runs over
  that iteration's raw response · downstream `tasks.X.<name>` is the
  **array of that binding's per-iteration values**, input order (so
  `tasks.X.output` = array of raw outputs · `tasks.X.title` = array of
  titles · positions align).
- **A failed iteration contributes `null`** at its index (in `.output`
  AND in every named binding): positional alignment survives partial
  failure (the zip patterns stay sound). Per-iteration
  `on_error: { recover: … }` substitutes its recovery value instead.
- **Where `.output` is observable** · the positional array is the
  task's `.output` only when the **parent settles `success`**, i.e.
  every element either succeeded, was `on_error: skip`-ped (→ `null` at
  its index) or `on_error: recover`-ed (→ the recovery value). That is
  the zip-sound surface a downstream task reads. An **UNRECOVERED**
  iteration error transitions the parent to `failure` (per `fail_fast`),
  and the failed parent's `.output` is **`null`**: NOT a partial array ·
  the per-iteration errors surface in the failure detail, not as output
  (a downstream task on a plain value edge from the failed parent is
  cancelled · the positional array is observable only on a `success`
  settle). To keep the array across a partial failure, handle errors per
  iteration (`on_error: skip` is the « process N · report which failed ·
  don't abort » idiom).
- The collection MUST be an array (a literal list or an upstream array
  imported through `with:`). A non-array collection (object · string ·
  number · `null`) is an evaluation error (`NIKA-VAR-006` ·
  `variable_error`). **The skipped-upstream corollary** · a value edge
  passes on a skipped producer and its binding reads `null`, so a
  fan-out over that binding fails with `NIKA-VAR-006` unless the author
  gates it (`after: {producer: success}` or a `when:` size check).
- `for_each` is **bounded fan-out**, not recursion · a task cannot
  `for_each` over its own output. The DAG stays acyclic.
- If the collection is empty · the task is `skipped` (status `skipped`).
- `when:` is evaluated **once** before the fan-out · `retry:` /
  `on_error:` / **`timeout:`** apply **per iteration**: the timeout
  clock covers one element's execution including its own retries (and
  backoff sleeps · wall-clock). There is **no whole-fan-out timer** in
  v0.1 (bound total work via `for_each.max_parallel:` + the per-iteration cap).
- `for_each.max_parallel:` + `for_each.fail_fast:` apply uniformly across all iterations.
- an `unwind` task (see below) runs **once** after all iterations complete
  (success OR failure): `item` / `index` are NOT in scope there (there
  is no current element after the fan-out).

This is the one construct that lets a v1 workflow process a
runtime-computed number of items (N files · N search hits · N pages)
without statically enumerating tasks.

#### Where this sits · deliberately between two named dataflow models

`for_each` is not a loop that got restricted. It is a specific point on a
line the dataflow literature drew in the 1980s and 90s, and the position is
the whole reason the language stays checkable ·

```
   SDF · synchronous dataflow          Lee & Messerschmitt 1987
     token rates FIXED at compile time
     ⇒ static schedule · bounded memory · deadlock-freedom · ALL DECIDABLE
     ⇒ but a data-dependent count cannot be expressed AT ALL

          ▼   nika `for_each` sits HERE   ▼
     · cardinality N is DATA-DEPENDENT (not SDF)
     · the collection is a PRE-FAN-OUT surface, evaluated EXACTLY ONCE
     · NO feedback from the iterations back into the graph
     · a task cannot fan out over its own output
     ⇒ N is unknown at check time · the PROGRAM stays finite and analysable

   BDF · Boolean dataflow              Buck & Lee 1993 · Lee & Parks,
     data-dependent switch/select      Proc. IEEE 83(5), 1995
     ⇒ TURING-COMPLETE
     ⇒ bounded-memory scheduling and strong consistency become UNDECIDABLE
       (Buck 1993 · deadlock-freedom specifically is NOT what is proved)
```

In the vendor-neutral workflow-pattern vocabulary (Russell, ter Hofstede,
van der Aalst, Mulyar, *Workflow Control-Flow Patterns: A Revised View*,
BPM-06-22, 2006) this is exactly **WCP-14 supported** (multiple instances
with run-time knowledge of N) and **WCP-15 structurally refused** (multiple
instances *without* a priori run-time knowledge — new iterations added while
the fan-out is already running). WCP-15 is a switch whose decision re-enters
the graph. That is the BDF line, and crossing it costs the decidability of
everything in [§static liveness](#static-liveness-check-time--normative).

**So the refusal of `while:` is not conservatism.** It is the same line,
named twice.

#### The name, examined rather than left alone

`for_each` reads sequential and runs **parallel**. That is a real
mismatch, and the empirical literature on keyword choice is not on its
side: Stefik & Siebert (ACM TOCE 13(4), 2013) showed novice accuracy
varies measurably with keyword choice — Perl's was statistically
indistinguishable from a language whose keywords were drawn *at random*
— and Lappi et al. (Software Quality Journal, 2023) replicated it. Two
better names exist: `map:` (states the semantics) and `fan_out:` (states
the execution).

**It keeps the name — but three of the arguments usually offered for
keeping it do not survive contact, and they are struck here rather than
left for a reviewer to strike.**

❌ *« every neighbour uses this word, so a reader guesses it »* —
**self-refuting.** The neighbours spell it `scatter` (WDL/CWL), `Map`
(ASL), `withItems` (Argo), `matrix` (GHA). `for_each` is a **fifth**
spelling, not one of the four, so « a reader of those four guesses ours »
has no support whatsoever.

**The precedent that DOES support it — checked, 2026-08-12.** Terraform's
`for_each` is the fifth-spelling ancestor, and its applies are indeed
concurrent by default: *« Graph walking is done in parallel: a node is
walked as soon as all of its dependencies are walked »*, bounded by a
semaphore whose `-parallelism` *« Defaults to 10 »* (`internals/graph.mdx`
· `plan.mdx`). So the word does arrive with a concurrent connotation from
the largest infrastructure-as-code ecosystem — **bounded, not unbounded**,
which is exactly what `for_each.max_parallel:` expresses here.

⭐ **And the same page hands us a stronger law than the naming one.**
Terraform requires `for_each` keys to be known before any remote call, and
refuses them outright when they are not: *« Keys in the `for_each` argument
cannot be the result of or rely on the result of impure functions,
including `uuid`, `bcrypt`, or `timestamp` »*. **Unknowns travel in the
VALUES, never in the STRUCTURE.** That is the same line this section draws
between SDF and BDF, reached independently by a declarative system with no
dataflow vocabulary — and it is why a fan-out cardinality that depended on
its own iterations would not be a feature but a different language.

❌ *« parallel-by-default is pillar 3, locked »* — **a category error.**
Pillar 3 constrains the **semantics**, not the **spelling**. `map:` with
parallel-by-default satisfies pillar 3 identically. A semantic lock
cannot defend a naming choice, and using it that way is precisely the
after-the-fact justification this spec tries not to commit.

❌ *« renaming changes only the first-read expectation »* — **the word
« only » is doing illegitimate work.** The studies cited above exist
*because* first-read expectation is measurable. Their real limit is
scope: they measure novice accuracy on general-purpose languages, and
whether that transfers to one field name in a workflow DSL is
**unknown**. That is the honest caveat, not a dismissal.

**What actually survives** is a migration cost against a bounded and
partly-measured confusion cost ·

- A rename touches every existing workflow, doc and example, for zero
  semantic gain.
- **42% of `for_each` tasks in the corpus also set `max_parallel`**
  (45 of 106, derived). Setting a concurrency cap *proves* the author
  saw the concurrency. The remaining 58% prove nothing either way — so
  42% is a **floor on informed authorship**, not a comprehension rate.
- `for_each.max_parallel:` is therefore the load-bearing mitigation: it is the
  field that makes concurrency visible at the call site, and it is
  already reached for by a large minority unprompted.

**Nobody has measured this specific case.** This is a judgement recorded
with its evidence, its cost, and its three dead arguments — not a
result. If someone runs the experiment (`for_each` vs `map:` vs
`fan_out:`, first-read expectation of ordering) the finding wins over
this paragraph.

### `timeout` · *optional · task-level timeout (Go duration string)*

```yaml
long_task:
    timeout: "5m"             # 5 minutes
    exec:
      command: ["./long-running.sh"]
```

Hard timeout for the entire task (including any retries and their backoff
sleeps · wall-clock). If exceeded · the task fails with a typed timeout error
(`NIKA-TIMEOUT-001`). On a `for_each` task the clock applies **per iteration**
(§for_each semantics). A timeout error is **catchable** by `on_error:`
(recover/skip like any failure) but never retryable (`transient: false` · the
timeout already covered the retries by definition).

On an `infer:`/`agent:` task the declared `timeout:` also **governs the
provider HTTP deadline** — and when none is declared the default is per
provider class (local ≥300s · cloud 30s · 600s transport ceiling on a
fully-silent connection). One place specs it ·
[stdlib/providers-v0.1.md §Transport deadline](../stdlib/providers-v0.1.md#transport-deadline--the-task-timeout-governs-the-provider-call).

**Format · Go-duration / Kubernetes-style string** `[0-9]+(\.[0-9]+)?(ns|us|µs|ms|s|m|h)`.

```yaml
timeout: "500ms"           # half a second
timeout: "30s"             # 30 seconds
timeout: "5m"              # 5 minutes
timeout: "1h30m"           # compound · 1.5 hours
timeout: "2.5s"            # fractional · 2500 ms
```

**Rules** ·
- MUST be a **quoted YAML string** · unquoted reject (`30s` unquoted parses as string OK but `30` unquoted parses as integer · ambiguous · forbidden).
- Positive · `> 0`.
- Maximum · `24h`. Tasks needing longer should split into a workflow chain.
- Compound units · combine in descending order (`1h30m500ms` ✓ · `30m1h` ✗).
- Unit suffixes (case-sensitive) · `ns` · `us` (or `µs`) · `ms` · `s` · `m` · `h`. No `d`/`w` (use compound · `48h` instead of `2d`).

**Why a duration string (not `timeout_ms: 30000`)** ·
- Industry standard · Go `time.ParseDuration` · Kubernetes resource limits · Prometheus rules.
- Reads naturally · `"5m"` beats `300000`.
- One field for all granularities · `ns` to `h`.
- Quoted-string requirement defeats YAML 1.2 numeric traps (Norway · sexagesimal · float drift).

### `retry` · *optional · retry policy*

See [05-errors.md](./05-errors.md).

### `on_error` · *optional · error recovery*

See [05-errors.md](./05-errors.md). Its `recover:` value is a **boundary
surface**: it may read `tasks.*` (a fallback source is a settled record) —
the reference is a *recovery edge* in the graph projection, and the
anti-deadlock law (`NIKA-DAG-004` · the source must not be downstream of
the declaring task) is unchanged.

### `returns` · *optional · the output contract*

```yaml
summarize:
  with: { article: "${{ tasks.fetch.output }}" }
  infer:
    prompt: "Summarize · ${{ with.article }}"
  returns:                      # the type expression, INLINE · named types are gone
    object:
      summary: string
```

Declares **what `tasks.X.output` is** — the typed door. Per-verb
mechanics (structured-output compilation for `infer:`/`agent:` ·
`decode:` + run-time fit for `exec:` · refinement for `invoke:`), the
type grammar, the lattice and the JSON-Schema lowering all live in
[09-types.md](./09-types.md). Two laws to know from here ·

- `returns:` and a verb-level `schema:` on one task = `NIKA-TYPE-003`
  (one contract, one spelling — `schema:` stays the out-of-core hatch).
- No `returns:` = the output is `Unknown` — gradual and honest: the
  static walk stops, nothing is invented ([04](./04-variables.md)).

Downstream, the contract types every value edge: a consumer binding
`${{ tasks.X.output }}` imports `optional<returns(X)>` (a skipped
producer reads defined-`null` · [09 §typed value edges](./09-types.md#typed-value-edges-normative)).

---

### `extract` · *optional · extraction bindings*

```yaml
api_call:
    invoke:
      tool: "nika:fetch"
      args:
        url: "https://api.example.com/data"
        mode: raw
    extract:
      user_count: ".data.users | length"
      first_user: ".data.users[0]"
      raw: "."
```

Defines named bindings extracted from the verb's raw response via a jq expression. These bindings are available downstream as `${{ tasks.task_id.user_count }}`, `${{ tasks.task_id.first_user }}`, etc. — imported through a consumer's `with:` like any output (a named binding is a **value**-role field).

If `extract` is absent · the task output defaults to the verb's raw response, referenced as `${{ tasks.task_id.output }}`.

The two trees are disjoint and both stay readable · `tasks.X.output` is the
raw response, `tasks.X.<name>` is an extraction of it. Declaring `extract:`
adds named siblings; it never replaces `output`.

#### Why the field is `extract:` and not `output:` (the rename, and what it cost)

The field was called `output:` until 2026-08-12. That name **stated a
falsehood**, and the falsehood was measured twice on the shipped 0.108.0
binary ·

1. **The parser forbids the field's own name inside itself.** A binding
   `output: { output: "." }` is refused — `NIKA-PARSE-013`, *« output binding
   `output` collides with a reserved field »*. It is one of six reserved
   names (`output` · `status` · `error` · `started_at` · `ended_at` ·
   `duration_ms`), but it is the only one that is also **the name of the
   block doing the refusing**.
2. **`tasks.X.output` stayed RAW when `output:` was declared.** A task
   declaring `output: { picked: ".foo" }` still served the unextracted verb
   response at `tasks.X.output`, alongside `tasks.X.picked`. Both reads
   checked green in the same file. The block adds named siblings; it never
   touched `output`.

So the field was named after the one thing it neither produces nor may
contain. It also sat one letter from the envelope's `outputs:` (the run's
exports), making singular-vs-plural carry a semantic distinction that nothing
in the spelling signalled.

**The replacement is `extract:`** — and the argument is deliberately *not*
that it is clearer. Furnas et al. measured that intuition dead in 1987 (*The
vocabulary problem in human-system communication*, CACM 30(11) — two people
pick the same name for the same thing under 20 % of the time; « the idea of
an obvious, self-evident, or natural term is a myth »). The argument is that
the old word **stated a falsehood** and the new one states a fact ·

- `extract:` names the **operation** (run this jq over the raw response),
  which is what the block does and all it does.
- It cannot be confused with `outputs:`: different word, not different
  number.
- The raw/named duality became readable — `tasks.X.output` is the response,
  `tasks.X.<name>` is an extraction of it.

**What did NOT change.** The rename is key-to-key and nothing else
(equivalence-or-stop). `tasks.X.output` — the record read — is untouched, and
so is the envelope's `outputs:`. The reserved-binding list is also untouched:
`extract: { output: "." }` is **still refused** (`NIKA-PARSE-013`), because a
binding may not shadow a record projection. That refusal now carries a reason
the spelling can state — you cannot name a binding after the raw response —
where before it was the block forbidding its own name. The reserved list dies
separately, with the `tasks.X.out.<name>` disjoint tree; until then it stands.

**Status · executed in this repo 2026-08-12 · the engine leg landed in 0.109.0 (2026-08-18).**
Measured here at execution · **17 workflow files + 6 spec fences** carry the
field (the earlier « 12 in this repo » estimate was low). The rename lands in
the engine parser, this schema, the corpus, the VS Code extension and the
website; **this repo's leg is done and its own oracle moved with it** (schema
`$defs/task`, `conformance/deep_static.py`, `conformance/runner.py`,
`scripts/showcase-projector.py`). The shipped 0.108.0 binary refused
`extract:` with `NIKA-PARSE-005` until 0.109.0 — the same lead the corpus
then carried on the envelope, not a new class of divergence. The migration is `canon/migrations.yaml` row
`mig-r4-task-extract-replaces-output` (`old_form: output` · `new_form:
extract` · mechanical 1:1, equivalence-or-stop) with a `canon/tombstones.yaml`
entry for the dead spelling, per the discipline every prior rename followed.

---

## The four graphs (normative)

A checked workflow denotes **four edge sets**, each derived from one
declaration surface — nothing else creates an edge ·

| graph | derived from | role | schedules? |
|---|---|---|---|
| **E_d** · data | `with:` bindings referencing `tasks.*` — **and `group.*`, one edge per declared member** (§group) | `value` · `terminal-observation` · `failure-observation` (per field shape · §with) · `fan-in` (§group) | yes |
| **E_c** · control | `after:` entries | `control` (with its predicate) | yes |
| **E_r** · recovery | `on_error.recover:` references | `recovery` — a parking read at recovery time, NOT an execution-order edge (`NIKA-DAG-004` guards the deadlock) | no |
| **E_f** · finally | `after: {x: unwind}` (producer → its cleanup task · and cleanup → cleanup) | `finally` — cleanup ordering after the parent settles | no · cleanup units are not tasks and never enter `G_p` — but they ARE **nodes** in the projection (`kind: "finally"` · format 3), because a judge that cannot see an effect carrier cannot govern it |

**The precedence graph is `G_p = E_d ∪ E_c`** · it MUST be acyclic
(`NIKA-DAG-001` · including self-edges) · Kahn wave scheduling runs over
G_p exactly as before — roles never change *precedence*, only *admission*
(§gate algebra). An edge target that is not a declared task is
`NIKA-DAG-002`.

## The gate algebra v2 (normative)

Every scheduling edge carries a **pass-set** — the producer's settled states
that admit the consumer ·

```
value edge                {success, skipped}
terminal-observation      {success, failure, skipped, cancelled}
failure-observation       {failure, skipped}
fan-in (§group)           {success, failure, skipped, cancelled}
control · success         {success}
control · failure         {failure}
control · skipped         {skipped}
control · terminal        {success, failure, skipped, cancelled}
```

(Pass-sets are **context-free**: an edge's role and predicate alone
determine admission, never the rest of the program. `failure-observation`
includes `skipped` because a skip may carry a preserved error — and when it
does not, the read is defined-`null`, total either way.)

**GATE-v2** · a task is admitted iff **EVERY** incoming edge's producer
settled **inside that edge's pass-set**. Any settled producer outside a
pass-set settles the consumer **`cancelled`** — and the cancellation
propagates transitively along every edge whose pass-set excludes
`cancelled` (dead-path elimination). Multiple edges from the same producer
compose by **intersection**: all of them must admit.

**The dispatch pipeline** · once every producer of a task has settled ·

```
1. GATE      every edge's producer ∈ its pass-set?     no → cancelled (dead path)
2. BINDINGS  with: values materialize                  eval error → failure (on_error NOT consulted)
3. WHEN      local condition evaluates                 false → skipped · eval error → failure (on_error NOT consulted)
4. VERB      the verb runs                             retry: / on_error: govern THIS stage only
```

The gate itself **cannot error** — pass-sets are structural, there is no
user expression in step 1. The boundary (steps 2-3) can: those errors settle
the task `failure` and are **outside `on_error` scope** (the armor covers
the verb, not the boundary that feeds it).

**The migration table** — how the three W2 spellings propagate, next to the
dead form they replace ·

| producer X settles | `with:` value edge | `after: {x: success}` | `after: {x: terminal}` | *(dead)* `depends_on: [x]` |
|---|---|---|---|---|
| `success` | run (binding = value) | run | run | ran |
| `skipped` | **run** (binding = `null`) | **cancelled** | run | ran |
| `failure` | cancelled | cancelled | **run** | cancelled |
| `cancelled` | cancelled | cancelled | **run** (terminal includes cancelled) | cancelled |

Choose knowingly · the value edge keeps the old default (skipped passes ·
read `null` · the diamond-join unlock) · `success` is the strict gate ·
`terminal` is the always-pattern (the report / cleanup / notify class —
pair it with a `.status` observation to branch on what happened).

### Static liveness (check-time · normative)

The gate algebra is decidable **before any run**. `check` computes each
task's statically-reachable settled-state set (a task with no `when:` and no
skip route can never settle `skipped` · a literal `when: false` can never
settle `success`/`failure` · `cancelled` is always reachable) and folds it
along G_p ·

- an incoming edge whose pass-set excludes **every** reachable producer
  state makes the consumer **provably dead** — cancelled on every possible
  run. That program is refused · **`NIKA-DAG-006`**. The same code covers a
  `when:` gate that is false under every reachable combination of upstream
  status observations. (This is why `after: {x: skipped}` on a producer
  that cannot skip is a check error, not a silent never-fires edge.)
- a status observation compared against a literal outside the vocabulary
  (`success` · `failure` · `skipped` · `cancelled`) can never match — `==`
  is always false, `!=` always true. Refused · **`NIKA-DAG-007`**.

A literal `when: false` alone is **not** a finding — it is the documented
never-pattern (feature-flag). The task settles `skipped` by explicit
intent, and downstream edges judge that state like any other.

### What that liveness check IS · soundness, and what it costs elsewhere

`NIKA-DAG-006` is not a nika invention. It is a **sound but incomplete
approximation** of condition (iii) of **soundness**, the workflow-net
correctness property defined by van der Aalst (1997/1998) and surveyed
in *Soundness of workflow nets: classification, decidability, and
analysis* (Formal Aspects of Computing 23(3):333–363, 2011). Two
attributions worth keeping straight: the condition **names** below are
the 2011 survey's, not the 1997 paper's, and that survey classifies
**eight** soundness notions — « structural soundness » in the complexity
results cited further down is one of those eight, not one of these
three. The three conditions of classical soundness are ·

| Soundness condition | Where nika earns it |
|---|---|
| (i) **option to complete** — the final state is reachable from every reachable state | structural · `G_p` is finite and acyclic, so a topological order exists and the wave scheduler drains it in at most \|V\| waves. A task that cannot start still *settles* (`cancelled` propagates along every edge whose pass-set excludes its producer) — there is no wait state that no edge can leave. |
| (ii) **proper completion** — nothing is left running when the workflow ends | structural · **every task settles exactly once**, into exactly one of four terminal states. There is no construct that puts a second thread of control on a node, so there is no residual token to leave behind. |
| (iii) **no dead transitions** — every activity fires in at least one execution | **approximated · sound, not complete** · `NIKA-DAG-006` / `NIKA-DAG-007`, above, refused before any run. See the limit below. |

> **The limit, stated honestly (measured on 0.108.0).** The reachability
> pass folds a per-task possible-status set and tests **each edge
> separately** against it — an *independent product*, which
> over-approximates the reachable joint assignments. It therefore never
> sees a death that depends on a **correlation between two producers of
> a common ancestor**. A worked counterexample that `check` accepts
> today ·
>
> ```yaml
> tasks:
>   upstream:       { exec: { command: ["true"] } }
>   branch_success: { after: { upstream: success }, exec: { command: ["true"] } }
>   branch_failure: { after: { upstream: failure }, exec: { command: ["true"] } }
>   dead:
>     after: { branch_success: success, branch_failure: success }
>     exec: { command: ["true"] }
> ```
>
> `upstream` settles into exactly ONE state, so `branch_success` and
> `branch_failure` can never both be `success` in the same run — `dead`
> is cancelled on every possible execution. `check` reports
> `✔ GATES no task proven dead`.
>
> The direction of the error is the part that matters: the pass is
> **sound** (a refused task is genuinely dead — no false positives) and
> **incomplete** (a dead task can slip through — false negatives). It
> never refuses a live program. Tightening it to the true joint set is a
> known, bounded piece of work; claiming it already decides condition
> (iii) would not be.

#### The two declared back-offs (normative · conformance)

Per-gate, the analysis is **exact**: it enumerates the referenced
upstream status sets in Kleene three-valued logic (a non-status atom
evaluates *Unknown*, and an Unknown gate is never declared dead). The
approximation above is at the **joint** level only.

That exactness is bought with two bounds, and a conformant engine MUST
declare them because they change the verdict ·

| Bound | Reference engine | Behaviour past it |
|---|---|---|
| distinct tasks referenced by one gate | **6** (4⁶ = 4096 leaf evaluations per gate) | the gate is treated **satisfiable** — it widens, never narrows |
| items in one gate's `in [...]` list | **256** | same widening. A status list has ≤4 meaningful values, so a longer one is adversarial padding (a 3.6 MiB gate costs ≈0.9 s to enumerate) |

Both back-offs move in the **sound** direction: past the bound the
checker gives up on proving death, never on admitting life. The
observable consequence is that **the same contradiction is refused at 6
gate references and accepted at 7** — that is the contract, not a bug,
and it is stated here because a conformance suite cannot test a
threshold it was never told about.

This is the shape the surrounding literature converges on: an exact
analysis is purchasable only against a **declared bound** on the input,
and where the bound is absent the static gate is abandoned rather than
faked (cf. Kubernetes KEP-3488, which drops static CEL cost estimation
exactly where `maxItems`/`maxLength` are unavailable).

The reason this is worth stating: **soundness is EXPSPACE-complete in
general** (Blondin, Mazowiecki, Offtermatt, LICS 2022 · arXiv:2201.05588 ·
generalised soundness PSPACE-complete), and PTIME only for the free-choice
subclass (van der Aalst 1998, Corollary 1 — **not** Esparza et al.
arXiv:1704.04190, which cites that result as background rather than
establishing it). In nika, (i) and (ii) hold **by shape** and (iii) is
approximated in polynomial time over `G_p` — because the shape was
chosen to make the question tractable, not because the check is clever.
The honest scoreboard: two conditions structural, one condition sound
but not yet complete, and a general problem that is EXPSPACE-complete.
Argo, GitHub Actions, Airflow and Step Functions accept a branch that
can never fire at all.

> **The permanent rule this buys.** A construct that cancels an **arbitrary
> region** — rather than propagating along declared edges — is a *reset arc*,
> and reset arcs make soundness **undecidable** — classical, structural and
> generalised alike (the last of those closed by Blondin, Finkel, Hofman,
> Mazowiecki, Offtermatt, LICS 2024 · DOI 10.1145/3661814.3662086; the others
> were already known). That paper also exhibits a variant that *stays*
> decidable (1-in-between soundness), so « every variant » would be too
> strong. YAWL cancellation regions and BPMN
> cancel/compensate/error events are exactly this. nika's cancellation is
> monotone propagation along edges that already exist, computed from
> context-free pass-sets — it never leaves the decidable class. **This is a
> permanent refusal, not a defer.**
>
> The same cliff closes the general OR-join: its accepted formalization is a
> reset net. See [08 §antivalues](./08-out-of-scope.md).

---

## DAG execution model

A conformant engine MUST ·

1. **Parse** · validate envelope · tasks map · verbs · `after:` predicates
   known (`NIKA-DAG-005`) · every `with:`/`after:` edge target declared
   (`NIKA-DAG-002`) · `tasks.*` confined to the boundary (`NIKA-VAR-021`) ·
   `depends_on` refused (`NIKA-PARSE-024`)
2. **Derive** · E_d from `with:` bindings (role per field shape · **plus one
   `fan-in` edge per member of every folded `group.*`** · a fold naming a
   group no task declares is `NIKA-DAG-008` · an `unwind` member is
   `NIKA-DAG-009`) · E_c from
   `after:` (predicate per entry) · G_p = E_d ∪ E_c · detect cycles
   (`NIKA-DAG-001`) · refuse statically dead tasks + out-of-vocabulary
   status literals (`NIKA-DAG-006` · `NIKA-DAG-007` · §static liveness) ·
   record E_r/E_f for projection + recovery/cleanup
3. **Schedule** · Kahn waves over G_p · execute each wave in parallel
   (engine MAY use a thread/task pool · configurable concurrency)
4. **Admit** · per task, once all edge-producers settled · apply GATE-v2
   (per-edge pass-sets · dead-path cancellation)
5. **Materialize** · `with:` bindings · then `when:` (local) — boundary
   errors settle `failure`, `on_error` NOT consulted
6. **Execute** · run the verb · capture output · bind via jq · `retry:` then
   `on_error:` govern this stage
7. **Complete** · workflow done when all tasks reached terminal state
   (success · failure · skipped · cancelled)

---

## Task states

| State | Meaning |
|---|---|
| `pending` | Task has not started · waiting on producers |
| `running` | Task is currently executing |
| `success` | Task completed successfully |
| `failure` | Task failed (after retries · no `on_error:` recovery · or a boundary error) |
| `skipped` | Task was skipped (`when:` evaluated false · empty `for_each` collection) |
| `cancelled` | Task was cancelled (a gate edge did not admit · workflow cancellation) |

A downstream task observes an upstream's status through a `with:` binding
(`${{ tasks.X.status }}` · a terminal-observation edge).
**Only the four terminal states are observable from expressions** (the closed
enum of [04](./04-variables.md#-taskxoutput--task-output-reference)):
`pending` / `running` exist in run reports and events, never inside `${{ }}`
(an edge's pass-set is checked only once its producer is terminal).

**Skipped is a decision · cancelled is a dead path (normative).** `when:
false` and an empty `for_each` settle `skipped` — the workflow CHOSE not to
run the task, and downstream value edges pass (reading `null`). A gate edge
that does not admit settles `cancelled` — the path is dead, and the
cancellation cascades. The two never substitute for each other.

**A boundary that fails to EVALUATE is a task failure — outside `on_error`
scope** (normative) · the gate decides IF the task runs; the boundary
(`with:` materialization · `when:`) feeds it; `on_error` governs the verb
run itself. A binding or `when:` whose evaluation errors (an unresolvable
root · a cross-type compare · any `NIKA-VAR` evaluation error) settles the
task `failure` — its `on_error` is NOT consulted — and downstream
failure-observation edges see it. Contrast · the same evaluation error in a
verb-body position (`args:` · `prompt:` · …) is task-stage work and IS
recoverable by that task's `on_error`.

---

## Examples

### Linear chain

```yaml
tasks:
  a:
    infer: { prompt: "Step 1" }
  b:
    with: { prev: "${{ tasks.a.output }}" }
    infer: { prompt: "Step 2 · prev was ${{ with.prev }}" }
  c:
    with: { prev: "${{ tasks.b.output }}" }
    infer: { prompt: "Step 3 · prev was ${{ with.prev }}" }
```

### Parallel fan-out

```yaml
tasks:
  setup:
    exec: { command: ["./prepare.sh"] }
  analyze_a:
    after: { setup: success }
    infer: { prompt: "Analyze A" }
  analyze_b:
    after: { setup: success }
    infer: { prompt: "Analyze B" }
  analyze_c:
    after: { setup: success }
    infer: { prompt: "Analyze C" }
  merge:
    with:
      a: ${{ tasks.analyze_a.output }}
      b: ${{ tasks.analyze_b.output }}
      c: ${{ tasks.analyze_c.output }}
    infer:
      prompt: "Merge · ${{ with.a }} · ${{ with.b }} · ${{ with.c }}"
```

`analyze_a` · `analyze_b` · `analyze_c` run in parallel after `setup`
(control edges — they consume nothing from it) · `merge` runs after all
three (value edges — the bindings are the fan-in).

### Conditional branch

```yaml
tasks:
  check:
    exec: { command: ["./check-env.sh"], capture: structured }

  build_prod:
    with: { env_name: "${{ tasks.check.output.env }}" }
    when: ${{ with.env_name == 'production' }}
    exec: { command: ["./build.sh", "--release"] }

  build_dev:
    with: { env_name: "${{ tasks.check.output.env }}" }
    when: ${{ with.env_name != 'production' }}
    exec: { command: ["./build.sh", "--debug"] }

  deploy:
    with:
      prod: ${{ tasks.build_prod.output }}     # null if that branch was skipped
      dev: ${{ tasks.build_dev.output }}
    exec: { command: ["./deploy.sh"] }
```

Exactly one of `build_prod` or `build_dev` runs · the other is skipped ·
`deploy` runs after both (value edges pass on skipped · the skipped
branch's binding is `null` · [04 §defined-null](./04-variables.md)).

### Map fan-out (`for_each`)

```yaml
tasks:
  discover:
    invoke:
      tool: "nika:fetch"
      args:
        url: "https://example.com/sitemap.xml"
        mode: sitemap
    extract:
      pages: "map(.loc)"   # sitemap output IS the root array of {loc, …} · a binding is single-valued, so collect the URLs into one array

  summarize:
    with:
      pages: ${{ tasks.discover.pages }}
    for_each:
      items: ${{ with.pages }}
    invoke:
      tool: "nika:fetch"
      args:
        url: ${{ item }}
        mode: article

  digest:
    with:
      summaries: ${{ tasks.summarize.output }}      # array of per-page outputs
    infer:
      prompt: "Write a digest from these summaries · ${{ with.summaries }}"
```

`discover` finds N pages · `summarize` runs once per page (parallel,
bounded) · `digest` consumes the array of all summaries. N is computed at
runtime: no static enumeration.

### Run-whatever-happened (the report pattern)

```yaml
tasks:
  pipeline:
    exec: { command: ["./run-pipeline.sh"] }

  report:
    after: { pipeline: terminal }                 # success · failure · skipped · cancelled
    with:
      outcome: ${{ tasks.pipeline.status }}       # observe it (terminal-observation edge)
      problem: ${{ tasks.pipeline.error }}        # ⚠ failure-observation — see below
    infer:
      prompt: "Report · pipeline ${{ with.outcome }} · ${{ with.problem }}"
```

⚠ **Composition caveat** · the `problem` binding is a failure-observation
edge (pass-set `{failure}`): adding it to `report` narrows the composed gate
to `{failure}` ∩ `{terminal}` = failures only. To report on EVERY outcome,
observe `.status` alone — or split a failure-path task from an
always-path task. The gate algebra is honest: what you bind is what you
require.

### Output shape · *no `output_format` field · shape is per-verb*

There is **no `output_format` task field**. The raw output shape is determined
**per verb**: the single source of truth is the `.output` table in
[02-verbs.md](./02-verbs.md#what--tasksidoutput--holds--per-verb) ·

- `infer:` → string · or the schema object when `schema:` is set
- `exec:` → stdout string · or `{stdout, stderr, exit_code}` when `capture: structured`
- `invoke:` → the tool's response (tool-determined · string · object · or bytes)
- `agent:` → final message string · or the schema object when `schema:` is set

To **force JSON validation** of a raw output, use the per-verb mechanism that
already owns it (`infer`/`agent` `schema:` · `exec` `capture: structured`) or
the `nika:validate` builtin, never a duplicate task-level type enum (a single
source of truth · Rams 4 understandable). A `output_format` field was drafted
in pre-public hardening and **removed** · it duplicated `capture`/`schema` and
its default table had drifted out of sync with 02-verbs (the very drift a
single source prevents).

### `unwind` · *a settle-state on `after:` · cleanup that ALWAYS runs*

Cleanup is a **task**, not a nested block. It declares itself with the
`unwind` predicate on `after:` ·

```yaml
tasks:
  process:
    exec:
      shell: "./process.sh > /tmp/output.json"

  drop_temp:
    after: { process: unwind }                  # an E_f edge · never schedules in G_p
    exec:
      command: ["rm", "-f", "/tmp/output.json"]

  announce:
    after: { drop_temp: unwind }                # cleanup chains like anything else
    invoke:
      tool: nika:emit
      args: { event: "task_completed", task_id: "process" }
```

> **Why this replaced `on_finally:`.** The old shape nested a *list of
> mini-tasks* inside a task. Measured, those units carried real verbs
> (`invoke` 8 · `exec` 2), real `when:`, real `timeout:`, real permits,
> and since `graph_format: 3` they are real **nodes**. They were tasks in
> everything but name, living in a **second grammar** the rest of the
> language did not share: unreferenceable, unreusable, untestable. One
> construct, one grammar — a task body now appears in exactly ONE place.

#### What `unwind` guarantees (normative · closed at v1)

An `unwind` edge is **not** a settle-state comparison like `success` or
`terminal`. It is the **E_f** attachment ([§the four graphs](#the-four-graphs-normative))
and it carries three properties no `after: {…: terminal}` task can have ·

1. **It fires on cancel (Ctrl+C) and on timeout**, for a producer that
   **started**. A producer that never ran (gate did not admit · `when:
   false` · cancelled-before-start) unwinds nothing — there is nothing to
   clean up. *A record that must land on EVERY workflow outcome is a
   terminal `after: {…: terminal}` task, not an unwind.*
2. **It runs BEFORE the producer's failure settles outward**, so cleanup
   can undo a side effect before anything downstream observes the
   failure.
3. **Its own failure does not propagate.** The producer's status reflects
   its own verb only — unwind is best-effort by construction, and its
   errors are logged.

#### The rest of the contract

- **Never in `G_p`.** `unwind` edges do not schedule, do not participate
  in cycle detection, and do not enter wave assignment. An engine that
  adds them to the precedence graph is wrong.
- **Ordering** · multiple tasks unwinding the same producer run in
  declaration order; chaining is just another `unwind` edge (see
  `announce` above).
- **Default timeout** · 30 seconds, overridable with the task's own
  `timeout:`.
- **What it may read** · the producer only (`${{ tasks.<producer>.status
  }}` · `.error` · `.output` — settled by definition when unwind runs).
  Any other task is `NIKA-VAR-021`: a sibling may still be RUNNING, so
  the read would race.
- **`for_each` producers** · the unwind runs **once**, after every
  iteration has completed. `item` / `index` are not in scope — there is
  no current element after a fan-out.
- **Engine MAY skip** only if the workflow process itself dies
  (SIGSEGV · OOM · hard kill).


## One obvious way · control-flow preference rules (normative for lints)

Several intents are *expressible* two ways; the spec names ONE as canonical.
These rules are informative for authors and **normative for linters**: a
conformant linter (the reference `one-obvious-way` rule set) warns on the
discouraged form ·

| Rule | Intent | ✅ The one way | ❌ Discouraged · why |
|---|---|---|---|
| `/010` | « B consumes A's output » | a `with:` binding — the data IS the edge | adding a non-tightening `after:` entry next to it (`after: {a: terminal}` beside a value edge changes nothing) |
| `/002` | « depend on a skippable producer » | decide the skip path: `after: {a: success}` (skip cancels me) or read the value (`with:` · skip passes as `null`) | an `on_error: { skip: true }` producer whose dependents never acknowledge the skip either way |
| `/003` | « retry on transient failure » | `retry:` · the ONE retry shape (`max_attempts` · `backoff_*` · `on_codes`) | an `after: {a: failure}` duplicate of the failing task · a self-referencing recovery chain |
| `/004` | « provide a fallback value » | `on_error: { recover: … }` · the route stays *in the failing task* | a second task `after: {a: failure}` for a mere value · use a task only when real *work* runs on failure |
| `/005` | « cleanup that always runs » | `after: {x: unwind}` · or ONE terminal report task | a task with `after: {…: terminal}` on everything — a cleanup smuggled into the graph |
| `/006` | « time-bound an iteration » | `timeout:` on the fan-out task · it applies **per iteration** (§for_each semantics) | per-element timing tricks inside the body · a whole-fan-out timer (none exists in v0.1) |
| `/007` | « cap fan-out concurrency » | `for_each.max_parallel:` | manual sharding into N sequential tasks |

(`one-obvious-way/001` — the pre-W2 « redundant success `when:` » class —
is **retired**: its discouraged form, a `tasks.*` status test inside
`when:`, is no longer merely discouraged but ILLEGAL (`NIKA-VAR-021`).
Rule ids are stable identifiers: retired ids are never reused.)

The dividing line, stated once · **`with:` imports data (and IS the data
edge) · `after:` orders on state (and IS the control edge) · `when:` reads
LOCAL values to decide *whether* an admitted task runs · `on_error:`/`retry:`
decide *what happens inside* a task's own failure.** A construct that
restates another construct's default is noise; a construct that smuggles
another's job is a trap. The reference validator ships these as warnings
(the `Rule` column above · stable ids), never hard errors (the discouraged
forms are legal · just not canonical).

## Native-first · preference rules (normative for lints)

The sibling ruleset for the VERB choice: `exec:` is the escape hatch,
never the default path. An `exec:` whose literal command a stdlib
builtin (or an MCP tool) covers trades portability, the capability
boundary and the audit certificate for a subprocess. A conformant
linter (the reference `native-first` rule set) warns on each class ·

| Rule | Fires on (literal command head/fragments) | The native path |
|---|---|---|
| `native-first/001 exec-http` | `curl` · `wget` · `xh` · `http(s)` · an interpreter one-liner around `fetch(`/`axios`/`http.request` | `nika:fetch` (uploads · `multipart:` · crawls · `traverse:`) |
| `native-first/002 exec-file` | `cat` · `tee` · `cp` · `mv` · `mkdir` · `touch` · `head` · `tail` · `ls` | `nika:read` / `nika:write` (`create_dirs: true`) / `nika:glob` |
| `native-first/003 exec-data` | `jq` · `sed` · `awk` | `nika:jq` (or an `extract:` binding) for JSON · `nika:edit` for in-place literal file edits |
| `native-first/004 exec-media` | an image/speech provider endpoint in the command (`images/generations` · `/v1/audio/speech` · …) | `nika:image_generate` / `nika:tts_generate` |
| `native-first/005 exec-helper` | an interpreter (`node` · `python` · `sh` · …) running a script file | inventory the helper · HTTP→`nika:fetch` · files→`nika:read`/`nika:write` · JSON→`nika:jq` · YAML/TOML/CSV in or out→`nika:convert` (then `nika:jq`) · a product API→an MCP server (`mcp:<server>/<tool>`) · a helper script is not one of the genuine subprocesses that stay silent below, so a ledger row records the intent without clearing this rule |
| `native-first/006 exec-utility` | a utility with an EXACT builtin · `sleep` · `date` · `uuidgen` · `sha256sum`/`shasum`/`md5sum`/`b3sum` · `yq` · `grep`/`rg`/`ag` · `find` | the one builtin AND its argument shape, not a family · `nika:wait` (`duration:`) · `nika:date` (`op:`) · `nika:uuid` · `nika:hash` (`algo:`) · `nika:convert` (`from:`/`to:`) · `nika:grep` · `nika:glob` |

Rules are DETERMINISTIC on literal fragments (a templated command head
makes no claim) · at most one warning per task, most specific first
(helper ≻ media ≻ http ≻ file ≻ data) · `nika run …` nesting and
genuine subprocesses (`cargo` · `git` · `make` · a product CLI with no
MCP surface yet) stay silent. Warnings, never hard errors — but a
STRICT authoring posture (a CI gate · an agent's final check) MAY
promote them to failures; the reference engine ships that posture as
`nika check --native-strict`. When an `exec:` legitimately remains,
the author records it in the **exec ledger** (task · command · why no
native path · the unlock that would remove it) — the workflow header
comment is the conventional home.

## Graph projection (`graph_format: 3`)

The DAG has ONE canonical machine-readable view: the **graph document**
a conforming implementation emits for a *checked* workflow (the
reference engine: `nika inspect <file> --format json`; the MCP surface
mirrors it). Clients — editor canvases, graph renderers, agents —
consume THIS document, never a private re-parse of the YAML. Without a
valid DAG there is no projection: the document is defined only for a
workflow whose conformance report is clean.

```json
{
  "graph_format": 3,
  "workflow": "release-notes",
  "nodes": [
    {
      "id": "gather", "kind": "task", "verb": "invoke", "tool": "nika:read",
      "when": null, "fan_out": null,
      "permits": ["fs.read:README.md"], "cost_interval": null
    },
    {
      "id": "think", "kind": "task", "verb": "infer", "model": "mistral/mistral-small",
      "when": null, "fan_out": null, "permits": [],
      "cost_interval": [0.0002, 0.0031],
      "timeout_ms": 60000, "outputs": ["summary"]
    },
    {
      "id": "publish", "kind": "task", "verb": "exec",
      "when": null, "fan_out": null, "permits": ["exec: ./publish.sh"],
      "cost_interval": null, "on_error": "recover"
    },
    {
      "id": "drop_temp", "kind": "finally", "verb": "exec",
      "when": null, "fan_out": null,
      "permits": ["exec: /bin/rm"], "cost_interval": null, "timeout_ms": 30000
    }
  ],
  "edges": [
    { "from": "gather", "to": "think", "kind": "value", "binding": "readme" },
    { "from": "think", "to": "publish", "kind": "control", "predicate": "success" },
    { "from": "gather", "to": "publish", "kind": "recovery" },
    { "from": "publish", "to": "drop_temp", "kind": "finally" }
  ]
}
```

**The envelope.** The number moves only for a change of MEANING, never
for new fields. Two such moves have happened ·

- **format 2** was the W2 reshape (typed edges): a format-1 reader
  assuming every edge is an ordering dependency would mis-read an
  observation edge.
- **format 3** admits **cleanup units as nodes**. A format-2 reader
  assumes every node is a task and every node belongs to `G_p` — it
  would schedule cleanup, or count it in a wave. The `kind` field is
  what makes the two populations distinguishable, and a reader that does
  not know `kind` cannot be trusted to keep them apart.

Formats 1 and 2 are **dead**: no producer, no consumer, no compat
fallback. **A reader MUST refuse a format it does not speak rather than
guess** — this is the whole point of moving the number, and it is the
protection format 2 could not offer, because a silently-ignored node is
a verdict rendered on a graph the judge did not fully see.

Within a format, evolution is **additive only**: new fields and new edge
`kind` values may appear in the SAME format number; readers MUST ignore
fields and edge kinds they do not know (fold-tolerance — the same law
the run stream follows). Node `kind` is the exception that forced the
bump: it is not additive, because its absence has a meaning
(*everything is a task*) that is now false.

**What a bump costs downstream, stated rather than discovered.** A
format-pinned reader stops reading the day the producer moves. That is
the *intended* behaviour — refusing beats guessing — but it is a real
migration, not a free one, and the producer owes consumers three things ·

1. **The bump is announced before it ships.** A consumer pinned to
   format 2 (`graph_format === 2`) is not broken by the spec; it is
   broken by the day the engine starts emitting 3. Those are different
   dates and the gap is the migration window.
2. **The engine SHOULD be able to emit the older format on request** for
   the length of that window. The precedent worth copying is
   Kubernetes' split CEL environments: a **narrow, version-pinned gate
   on what may be written**, and a **permissive runtime that still reads
   everything already persisted**. Write-time strict, read-time
   tolerant — that is what makes a rollback safe by construction.
3. **The marker must not reach the interop boundary.** Rust editions are
   the clean form: crates of different editions link, because the
   edition changes the parser and never the artifact. A projection
   format that splits its own consumer ecosystem has bought the split it
   was meant to avoid.

The v1 position: format 3 is the wire, format 2 is dead, and any
consumer pinned to 2 MUST be migrated rather than served a downgrade
forever. The window is a courtesy with an end, not a second format.

### Cleanup units are nodes (normative · new in format 3)

Every `unwind` task is projected as a node. Before format 3 they were
executed, they checked permits, and they emitted trace events — but they
had **no place in the derived graph**, so every graph-shaped judge
(order · consent · flow) was blind to them while the runtime ran them.
A judge that cannot see an effect carrier cannot govern it, and a green
from such a judge is a claim about a subset.

A cleanup unit is an ORDINARY task ([§unwind](#unwind--a-settle-state-on-after--cleanup-that-always-runs))
— author-named, referenceable, described exactly like any other node.
Only `kind` tells the two populations apart, which is the whole reason
the field forced the bump.

| Field | Rule |
|---|---|
| `id` | the author's task id — the same name `after: {…: unwind}` attaches to, and the same name a later cleanup chains from. Cleanup units are not anonymous. |
| `kind` | `"finally"` |
| `verb` · `permits` · `timeout_ms` | as for any node — `permits` is the reason this projection exists · `timeout_ms` defaults to 30000 (§unwind) |
| `when` · `fan_out` · `cost_interval` | present-as-null · a cleanup unit takes no gate and no fan-out |

**They are NOT in `G_p`.** `finally` edges never schedule (the `E_f` row
above) and never participate in cycle detection or wave assignment. A
reader that adds them to a precedence graph is wrong. The edges are ·

- producer → its cleanup task · `kind: "finally"`
- cleanup → cleanup · `kind: "finally"` (a unit that attaches to another
  unit chains after it, like anything else declaring `after:`)

**Nodes are topologically sorted** in wave order (over G_p), and the order
is stable across runs of the projector — stable input, stable layout.

**Node fields.** `id` and `verb` (one of the four) are always present.
Three field families follow, and their absence rules are part of the
wire contract:

| Presence | Fields | Rule |
|---|---|---|
| always | `id` · `kind` · `verb` · `permits` | `kind` is `"task"` or `"finally"` (format 3 · a reader MUST branch on it before assuming a node schedules) · `permits` may be empty — per-task capability attribution (`exec:` · `fs.read:` · `fs.write:` · `net.http:` · `tool:` families, deterministic order), the un-aggregated voice of the same effect walk `infer_permits` folds into the workflow boundary |
| present-as-null when undeclared | `when` · `fan_out` · `cost_interval` | `when` carries the business-condition source (`"true"`/`"false"` literal or the CEL island — POST-gate, never the gate itself) · `fan_out` is `{ "kind": "list" \| "expression" }` with `count` only for the literal-list form · `cost_interval` is `[min_path, worst_case]` USD for **priced inference tasks only** (no price, no interval — never a fabricated 0) |
| absent when undeclared | `tool` · `model` · `retry_max_attempts` · `timeout_ms` · `on_error` · `outputs` | declared POLICY, projected so clients read it here instead of re-parsing YAML: `tool` for `invoke` tasks · `model` as resolved `provider/name` (task override else workflow default) · `retry.max_attempts` (05) · `timeout:` as parsed milliseconds (unambiguous where the source string is not) · `on_error:` action (`recover` · `skip`) · declared `extract:` binding names in source order (04) |

**Edges** carry `from` · `to` · `kind` — and per kind ·

| `kind` | extra fields | derived from |
|---|---|---|
| `value` | `binding` (the `with:` key that created it) | a `.output` / named-binding reference in `with:` |
| `terminal-observation` | `binding` | a `.status`/`.duration_ms`/`.started_at`/`.ended_at` reference in `with:` |
| `failure-observation` | `binding` | an `.error` reference in `with:` |
| `control` | `predicate` (`success` · `failure` · `skipped` · `terminal`) | an `after:` entry |
| `recovery` | — | an `on_error.recover:` reference (source task → declaring task · a parking read, not an ordering edge) |
| `finally` | — | **reserved for the GRAPH** · W2 emits no `finally` edges. ⚠️ The reason given here was *"no runtime identity yet · no events · no trace rows"* and that is **measured FALSE** (2026-08-11): a cleanup unit emits `permit_checked` carrying `plane: on_finally · gate: cleanup #N · decision: attempt`. It HAS runtime identity; what it lacks is a place in the derived graph. **The two are different, and conflating them is how a cleanup unit became invisible to every graph-shaped judge while the runtime executed it** — the defect behind D-2026-08-11-N19 (complete mediation). The edge kind stays unemitted until cleanup units enter the graph; the trace contract already sees them |

One `with:` binding whose expression references N tasks yields N edges
(each carrying the same `binding` name). The `kind` enum is CLOSED at six —
new kinds arrive additively with the spec, and unknown kinds fall under the
reader-tolerance rule.

**Spans are presentation, never truth.** The graph document carries NO
source positions. A surface that pairs the graph with source ranges (the
LSP `nika/semanticDocument` — `{graph, reason, spans}`) wraps THIS document
verbatim and adds its presentation layer outside it: byte-for-byte, the
`graph` member IS the CLI/MCP document (the three-protocol parity law).

**The static law.** The graph document describes the workflow as
WRITTEN — it never carries run state (no statuses, no live costs, no
durations). Run truth lives in the run stream and the trace; a client
that paints run state onto this graph joins the two by task `id`.

## Forward-compat

v1 ships with these task fields · `with` · `after` · `when` · `for_each` · `retry` · `on_error` · `timeout` · `extract` · `returns` · `lift` · plus the verb selector. (`max_parallel` and `fail_fast` are sub-fields of `for_each`; `on_finally` is dead — cleanup is a task on an `unwind` edge.) Additional fields may be added in minor bumps (additive only). (Output *shape* is per-verb · not a task field · see [02-verbs.md](./02-verbs.md#what--tasksidoutput--holds--per-verb).)

Out of scope for v1 · `parallel:` for explicit concurrency control · `include:` for sub-workflow composition (workaround · `exec: nika run sub.yaml`). See [08-out-of-scope.md](./08-out-of-scope.md).

---

🦋 *Next · [04 · Variables](./04-variables.md)*
