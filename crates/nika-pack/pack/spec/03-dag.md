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
    config: { foo: "bar" }      # literals are fine — only tasks.* refs create edges
  after:                        # optional · the CONTROL boundary · {producer: predicate}
    task_b: success             # predicate ∈ success | failure | skipped | terminal
  when: ${{ inputs.enabled }}   # optional · LOCAL business condition · evaluated POST-gate
  for_each: ${{ with.pages }}   # optional · map this task over a collection (local namespaces)
  retry:                        # optional · retry policy (see 05-errors.md)
    max_attempts: 3
    backoff_ms: 1000
  on_error:                     # optional · error recovery (see 05-errors.md)
    recover: ${{ tasks.cache.output }}
  timeout: "60s"                # optional · task-level timeout (Go duration string)
  infer:                        # required · one of the 4 verbs
    prompt: "... ${{ with.data }} ..."
  returns: Summary              # optional · the OUTPUT CONTRACT (a named type or inline · 09-types.md)
  output:                       # optional · named jq bindings
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
clean CEL identifiers. (The workflow-level `workflow:` id stays kebab-case:
it is a resource name, never referenced inside an expression.)

### `with` · *optional · the DATA boundary — bindings that ARE edges*

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
`inputs` · `config` · `const` · `with` · and the `for_each` locals `item` / `index`.
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
size                        size(coll) · coll.size()   (collection/string length · the ONE v0.1 function · empty-check idiom)
literals                    true · false · 42 · 3.14 · 'str' · "str" · null
grouping                    ( … )
```

`size()` (collection/string length) is the ONE function in the v0.1 subset,
the canonical empty/non-empty-check idiom (`size(items) > 0`). Everything else
is **reserved** · arithmetic · CEL macros (`has()`, `all()`, `exists()`) · and
string-manipulation functions (`startsWith`, `matches`, `contains`, …): not in
the v0.1 subset, addable in a later minor (CEL is a superset, so growth is
additive and never breaking). If you need richer logic today, compute it in a
`nika:assert` builtin or an `infer:` task.

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
6. **Identifier roots resolve against the namespaces** · the 6 global
   namespaces (`inputs` · `config` · `const` · `secrets` · `with` · `tasks`)
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

**Namespaces are CEL variables** · the <!-- canon:namespaces -->6<!-- /canon --> namespaces (`inputs` · `config`
· `const` · `secrets` · `with` · `tasks`) are bound as top-level CEL variables — `tasks.*` on the
boundary surfaces only. **Inside a `for_each` task body, two
more scoped CEL variables are bound** · `item` (the current element) and `index`
(its 0-based position), available ONLY within that task (the <!-- canon:namespaces -->6<!-- /canon --> namespaces are
global · `item`/`index` are for_each-local · see `for_each` below).

#### The binding is the edge — no invisible edges

Pre-W2, a `tasks.X` reference anywhere required a matching `depends_on`
declaration and a missing one was an error (the retired `NIKA-DAG-003`
class). W2 removes the double bookkeeping in both directions: a `tasks.X`
reference is **legal only where it declares an edge by existing** (`with:` ·
`after:`) or reads a settled record on a declared surface (`on_error.recover`
· `on_finally` · workflow `outputs:`). The engine never infers a hidden
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

### `for_each` · *optional · map a task over a collection*

```yaml
scrape_all:
    with:
      pages: ${{ tasks.discover.pages }}         # the collection crosses the boundary here
    for_each: ${{ with.pages }}                  # a local read · a literal list also works
    max_parallel: 5                              # optional · cap concurrent iterations · default unbounded
    fail_fast: false                             # optional · false = keep going on errors · default true
    invoke:
      tool: nika:fetch
      args: { url: "${{ item }}", mode: article }
```

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
iterations concurrently · bounded by `max_parallel:` if set).

This is **different from Python's sequential `for` loop**. If you need
sequential iteration · set `max_parallel: 1` ·

```yaml
process_in_order:
    for_each: ${{ inputs.items }}
    max_parallel: 1                              # iterations run one-at-a-time, in order
    exec:
      command: ["process", "${{ item }}"]
```

#### `max_parallel:` · *optional · cap concurrent iterations*

```yaml
for_each: ${{ inputs.urls }}   # 1000 URLs
max_parallel: 5                # at most 5 in-flight at any time
```

- **Default · unbounded** (subject to engine-wide concurrency budget · v0.3
  daemon adds workflow-level cap).
- **Positive integer** · `1` to `n`. `1` = sequential.
- **Engine impl** · `tokio::sync::Semaphore` (or equivalent) · iterations
  acquire a permit before executing · release on completion.
- **Use cases** · rate-limiting provider APIs · avoiding resource
  exhaustion · compliance with concurrency limits.

#### `fail_fast:` · *optional · abort-on-error policy*

```yaml
for_each: ${{ inputs.urls }}
fail_fast: false                # default true · false = process all even if some fail
```

- **Default · `true`** · first iteration error aborts remaining iterations ·
  parent task transitions to `failure` status immediately.
- **`fail_fast: false`** · iteration errors are collected · remaining
  iterations keep running · parent task transitions to `failure` (with
  per-iteration error details) ONLY after all iterations complete.
- **Use cases** · « process N URLs · report which failed but don't abort »
  (false) vs « if any LLM call fails, the whole batch is invalid » (true).

#### Semantics (closed at v1)

- **Every expression in the task body is re-evaluated PER ITERATION** with
  `item`/`index` bound: `with:`, the verb fields (`prompt:` · `command:` ·
  `args:` · …), `when:`, AND the `output:` bindings. (A binding that does
  not reference `item`/`index` evaluates to the same value every iteration —
  expressions are pure over settled state — so an engine MAY materialize it
  once; the observable behavior is identical.) The only expression evaluated
  strictly once is the `for_each:` collection itself (pre-fan-out surface ·
  above).
- The task's output is the **array of per-iteration outputs**, in input
  order · referenced downstream as `${{ tasks.scrape_all.output }}`
  (an array) · `${{ tasks.scrape_all.output[0] }}` for one element.
- **`output:` bindings apply per iteration**: each binding's jq runs over
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
  v0.1 (bound total work via `max_parallel:` + the per-iteration cap).
- `max_parallel:` + `fail_fast:` apply uniformly across all iterations.
- `on_finally:` (see below) runs **once** after all iterations complete
  (success OR failure): `item` / `index` are NOT in scope there (there
  is no current element after the fan-out).

This is the one construct that lets a v1 workflow process a
runtime-computed number of items (N files · N search hits · N pages)
without statically enumerating tasks.

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
  returns: Summary              # a name declared in types: — or an inline type expression
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

### `output` · *optional · output binding*

```yaml
api_call:
    invoke:
      tool: "nika:fetch"
      args:
        url: "https://api.example.com/data"
        mode: raw
    output:
      user_count: ".data.users | length"
      first_user: ".data.users[0]"
      raw: "."
```

Defines named bindings extracted from the verb's raw response via a jq expression. These bindings are available downstream as `${{ tasks.task_id.user_count }}`, `${{ tasks.task_id.first_user }}`, etc. — imported through a consumer's `with:` like any output (a named binding is a **value**-role field).

If `output` is absent · the task output defaults to the verb's raw response, referenced as `${{ tasks.task_id.output }}`.

---

## The four graphs (normative)

A checked workflow denotes **four edge sets**, each derived from one
declaration surface — nothing else creates an edge ·

| graph | derived from | role | schedules? |
|---|---|---|---|
| **E_d** · data | `with:` bindings referencing `tasks.*` | `value` · `terminal-observation` · `failure-observation` (per field shape · §with) | yes |
| **E_c** · control | `after:` entries | `control` (with its predicate) | yes |
| **E_r** · recovery | `on_error.recover:` references | `recovery` — a parking read at recovery time, NOT an execution-order edge (`NIKA-DAG-004` guards the deadlock) | no |
| **E_f** · finally | `on_finally:` attachment (parent → its cleanup units) | `finally` — cleanup ordering after the parent settles | no (cleanup units are not tasks) |

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

---

## DAG execution model

A conformant engine MUST ·

1. **Parse** · validate envelope · tasks map · verbs · `after:` predicates
   known (`NIKA-DAG-005`) · every `with:`/`after:` edge target declared
   (`NIKA-DAG-002`) · `tasks.*` confined to the boundary (`NIKA-VAR-021`) ·
   `depends_on` refused (`NIKA-PARSE-024`)
2. **Derive** · E_d from `with:` bindings (role per field shape) · E_c from
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
    output:
      pages: "map(.loc)"   # sitemap output IS the root array of {loc, …} · a binding is single-valued, so collect the URLs into one array

  summarize:
    with:
      pages: ${{ tasks.discover.pages }}
    for_each: ${{ with.pages }}
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

### `on_finally` · *optional · cleanup hook · ALWAYS runs*

```yaml
process:
    exec:
      shell: "./process.sh > /tmp/output.json"   # redirect → the explicit shell door
    on_finally:                                  # runs always · success/fail/timeout/cancel
      - exec:
          command: ["rm", "-f", "/tmp/output.json"]
      - invoke:
          tool: nika:emit
          args: { event: "task_completed", task_id: "process" }
```

`on_finally:` declares **cleanup tasks** that run after the parent task
completes · REGARDLESS of outcome (success · failure · timeout · cancel).

#### Semantics (closed at v1)

- **List of mini-tasks** · zero or more · each with its own verb (`exec:` ·
  `invoke:` · or `infer:` · `agent:` rarely used here).
- **Runs sequentially** in declared order · cleanup-task-N starts after
  cleanup-task-(N-1) completes.
- **Cleanup errors are LOGGED but DO NOT propagate** · the parent task's
  final status reflects ONLY the main verb's outcome · NOT the cleanup
  outcomes (best-effort semantics).
- **The parent is the only readable task (normative · W2)** · inside an
  `on_finally:` block, `tasks.*` may reference the PARENT task only
  (`${{ tasks.<parent>.status }}` · `.error` · `.output` — settled by
  definition when cleanup runs). Any other task is `NIKA-VAR-021`: a
  sibling may still be RUNNING when this parent's cleanup fires — the
  read would race, and pre-W2 engines silently allowed it (the class is
  now inexpressible).
- **Default cleanup timeout** · 30 seconds per cleanup task (overridable
  per cleanup task via `timeout:` field).
- **Failed parent task's `on_finally:` runs BEFORE** the failure settles
  outward in the DAG (gives cleanup a chance to undo side effects).
- **Engine MUST execute** `on_finally:` on cancel (Ctrl+C) and timeout,
  for a task that **started**. A task that never ran (a gate that did not
  admit · `when: false` · cancelled-before-start) runs NO `on_finally:`
  (there is nothing to clean up). A record that must land on EVERY
  workflow outcome is a **terminal `after: {…: terminal}` task** (the
  always-pattern · §gate algebra), not a cleanup hook.
- **Engine MAY skip** `on_finally:` only if the workflow process itself
  crashes (SIGSEGV · OOM · hard kill).

#### Use cases

```yaml
# 1 · cleanup temp files (scratch_dir declared in envelope const:)
on_finally:
  - exec: { command: ["rm", "-rf", "${{ const.scratch_dir }}"] }   # argv: the constant cannot break out

# 2 · always-emit completion event
on_finally:
  - invoke:
      tool: nika:emit
      args: { event: "task_done", status: "${{ tasks.process.status }}" }

# 3 · on-error notification only
on_finally:
  - when: ${{ tasks.process.status == 'failure' }}
    invoke:
      tool: nika:fetch
      args:
        url: "https://hooks.slack.com/..."
        method: POST
        body: { text: "Task failed · ${{ tasks.process.error }}" }
```

(Inside `on_finally:`, the parent's record is the local context — its
`when:` reads the parent directly; the `with:`/hoist law governs TASK
bodies, not cleanup mini-tasks, whose only legal `tasks.*` target is the
parent.)

---

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
| `/005` | « cleanup that always runs » | `on_finally:` (per task) · or ONE terminal report task | a task with `after: {…: terminal}` on everything — a cleanup smuggled into the graph |
| `/006` | « time-bound an iteration » | `timeout:` on the `for_each` task · it applies **per iteration** (§for_each semantics) | per-element timing tricks inside the body · a whole-fan-out timer (none exists in v0.1) |
| `/007` | « cap fan-out concurrency » | `max_parallel:` | manual sharding into N sequential tasks |

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
| `native-first/003 exec-data` | `jq` · `sed` · `awk` | `nika:jq` (or an `output:` binding) for JSON · `nika:edit` for in-place literal file edits |
| `native-first/004 exec-media` | an image/speech provider endpoint in the command (`images/generations` · `/v1/audio/speech` · …) | `nika:image_generate` / `nika:tts_generate` |
| `native-first/005 exec-helper` | an interpreter (`node` · `python` · `sh` · …) running a script file | inventory the helper · HTTP→`nika:fetch` · files→`nika:read`/`nika:write` · JSON→`nika:jq` · YAML/TOML/CSV in or out→`nika:convert` (then `nika:jq`) · a product API→an MCP server (`mcp:<server>/<tool>`) · keep only a genuine subprocess, recorded in the exec ledger |

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

## Graph projection (`graph_format: 2`)

The DAG has ONE canonical machine-readable view: the **graph document**
a conforming implementation emits for a *checked* workflow (the
reference engine: `nika inspect <file> --format json`; the MCP surface
mirrors it). Clients — editor canvases, graph renderers, agents —
consume THIS document, never a private re-parse of the YAML. Without a
valid DAG there is no projection: the document is defined only for a
workflow whose conformance report is clean.

```json
{
  "graph_format": 2,
  "workflow": "release-notes",
  "nodes": [
    {
      "id": "gather", "verb": "invoke", "tool": "nika:read",
      "when": null, "fan_out": null,
      "permits": ["fs.read:README.md"], "cost_interval": null
    },
    {
      "id": "think", "verb": "infer", "model": "mistral/mistral-small",
      "when": null, "fan_out": null, "permits": [],
      "cost_interval": [0.0002, 0.0031],
      "timeout_ms": 60000, "outputs": ["summary"]
    },
    {
      "id": "publish", "verb": "exec",
      "when": null, "fan_out": null, "permits": ["exec: ./publish.sh"],
      "cost_interval": null, "on_error": "recover"
    }
  ],
  "edges": [
    { "from": "gather", "to": "think", "kind": "value", "binding": "readme" },
    { "from": "think", "to": "publish", "kind": "control", "predicate": "success" },
    { "from": "gather", "to": "publish", "kind": "recovery" }
  ]
}
```

**The envelope.** `graph_format: 2` is the W2 reshape (typed edges — a
breaking change of MEANING, not just of fields: a v1 reader assuming every
edge is an ordering dependency would mis-read an observation edge, so the
format number moved). Format 1 is **dead**: no producer, no consumer, no
compat fallback survives W2 — a reader MUST refuse a format it does not
speak rather than guess. Within format 2, evolution is **additive only**:
new fields and new edge `kind` values may appear in the SAME format number;
readers MUST ignore fields and edge kinds they do not know
(fold-tolerance — the same law the run stream follows).

**Nodes are topologically sorted** in wave order (over G_p), and the order
is stable across runs of the projector — stable input, stable layout.

**Node fields.** `id` and `verb` (one of the four) are always present.
Three field families follow, and their absence rules are part of the
wire contract:

| Presence | Fields | Rule |
|---|---|---|
| always | `id` · `verb` · `permits` | `permits` may be empty — per-task capability attribution (`exec:` · `fs.read:` · `fs.write:` · `net.http:` · `tool:` families, deterministic order), the un-aggregated voice of the same effect walk `infer_permits` folds into the workflow boundary |
| present-as-null when undeclared | `when` · `fan_out` · `cost_interval` | `when` carries the business-condition source (`"true"`/`"false"` literal or the CEL island — POST-gate, never the gate itself) · `fan_out` is `{ "kind": "list" \| "expression" }` with `count` only for the literal-list form · `cost_interval` is `[min_path, worst_case]` USD for **priced inference tasks only** (no price, no interval — never a fabricated 0) |
| absent when undeclared | `tool` · `model` · `retry_max_attempts` · `timeout_ms` · `on_error` · `outputs` | declared POLICY, projected so clients read it here instead of re-parsing YAML: `tool` for `invoke` tasks · `model` as resolved `provider/name` (task override else workflow default) · `retry.max_attempts` (05) · `timeout:` as parsed milliseconds (unambiguous where the source string is not) · `on_error:` action (`recover` · `skip` · `fail_workflow`) · declared `output:` binding names in source order (04) |

**Edges** carry `from` · `to` · `kind` — and per kind ·

| `kind` | extra fields | derived from |
|---|---|---|
| `value` | `binding` (the `with:` key that created it) | a `.output` / named-binding reference in `with:` |
| `terminal-observation` | `binding` | a `.status`/`.duration_ms`/`.started_at`/`.ended_at` reference in `with:` |
| `failure-observation` | `binding` | an `.error` reference in `with:` |
| `control` | `predicate` (`success` · `failure` · `skipped` · `terminal`) | an `after:` entry |
| `recovery` | — | an `on_error.recover:` reference (source task → declaring task · a parking read, not an ordering edge) |
| `finally` | — | **reserved** · cleanup units have no runtime identity yet (no events · no trace rows), so W2 emits no `finally` edges — the kind is named so the enum is complete when the trace contract (W5) gives cleanup units identity |

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

v1 ships with these task fields · `with` · `after` · `when` · `for_each` · `max_parallel` · `fail_fast` · `retry` · `on_error` · `timeout` · `on_finally` · `output` · plus the verb selector. Additional fields may be added in minor bumps (additive only). (Output *shape* is per-verb · not a task field · see [02-verbs.md](./02-verbs.md#what--tasksidoutput--holds--per-verb).)

Out of scope for v1 · `parallel:` for explicit concurrency control · `include:` for sub-workflow composition (workaround · `exec: nika run sub.yaml`). See [08-out-of-scope.md](./08-out-of-scope.md).

---

🦋 *Next · [04 · Variables](./04-variables.md)*
