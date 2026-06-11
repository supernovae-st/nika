# 03 · DAG shape

> A Nika workflow is a **Directed Acyclic Graph** of tasks. Each node is a
> task (one of the 4 verbs). Each edge is a dependency.
>
> The DAG semantics are minimal · `depends_on` for order · `when` for
> conditional execution · output binding via jq.

---

## Minimal DAG

```yaml
tasks:
  - id: a
    infer:
      prompt: "First"

  - id: b
    depends_on: [a]
    infer:
      prompt: "Second, after a"
```

`b` runs after `a` completes. If `a` fails (and no `on_error:` recovery), `b` does not run.

---

## Task shape · full

```yaml
- id: my_task                   # required · snake_case · unique within workflow
  depends_on: [task_a, task_b]  # optional · default []
  when: ${{ tasks.task_a.status == 'success' }}  # optional · conditional execution
  for_each: ${{ tasks.list.output }}  # optional · map this task over a collection
  retry:                        # optional · retry policy (see 05-errors.md)
    max_attempts: 3
    backoff_ms: 1000
  on_error:                     # optional · error recovery (see 05-errors.md)
    recover: ${{ tasks.cache.output }}
  timeout: "60s"                # optional · task-level timeout (Go duration string)
  with:                         # optional · variable scope injection
    data: ${{ tasks.task_a.output }}
    config: { foo: "bar" }
  infer:                        # required · one of the 4 verbs
    prompt: "..."
  output:                       # optional · named jq bindings
    result: ".choices[0].message.content"
    tokens: ".usage.total_tokens"
```

---

## Field-by-field

### `id` · **required · snake_case · unique**

```yaml
- id: research_topic
```

Match · `^[a-z][a-z0-9_]*$` (snake_case · no hyphens). Must be unique within
the workflow file.

**Why snake_case, not kebab** · task ids are referenced in CEL expressions as
`tasks.<id>.output`. In CEL (and almost every expression language) a hyphen is
the **subtraction operator** — `tasks.research-topic.output` would parse as
`tasks.research - topic.output`, a silent trap. Snake_case ids are always
clean CEL identifiers. (The workflow-level `workflow:` id stays kebab-case —
it is a resource name, never referenced inside an expression.)

### `depends_on` · *optional · default `[]`*

```yaml
- id: c
  depends_on: [a, b]
```

A list of task ids this task depends on. The engine MUST not start this task until ALL deps have completed (successfully OR with a recovered error via `on_error:`).

**Cycle detection** · the engine MUST reject any workflow with cyclic dependencies at parse time with a clear error.

**Parallel execution** · tasks with no deps between them MAY run in parallel. This is the default behavior · the engine SHOULD parallelize wherever possible.

### `when` · *optional · conditional execution*

```yaml
- id: notify
  depends_on: [build]
  when: ${{ tasks.build.status == 'success' }}
  exec:
    command: "./notify.sh"
```

#### Expression language · a documented subset of CEL

Everything inside `${{ ... }}` — both value substitution and `when:`
conditions — is **[CEL](https://cel.dev) (Common Expression Language)**, the
validated, non-Turing-complete, side-effect-free expression standard used by
Kubernetes (ValidatingAdmissionPolicy), Kyverno, Envoy, and gRPC. Nika does
**not** invent an expression DSL — it adopts the standard. (This supersedes the « custom minimal DSL » framing.)

**Why CEL** · it is *common* (millions of K8s users), *comprehensible*
(reads like a boolean expression), *validated* (a published spec + multiple
conformant implementations), *safe* (non-Turing-complete · bounded · no side
effects) and *portable* (zero parser drift between engines). A hand-rolled
DSL would be none of those.

**The v0.1 subset** (the only CEL features a conformant engine must support) ·

```
identifier / field access   vars.topic · tasks.build.status · with.content
index access                tasks.list.output[0] · obj['key-with-dash']
comparison                  == · != · < · <= · > · >=
boolean                     && · || · !
membership                  in            (e.g. status in ['success','skipped'])
size                        size(coll) · coll.size()   (collection/string length · the ONE v0.1 function · empty-check idiom)
literals                    true · false · 42 · 3.14 · 'str' · "str" · null
grouping                    ( … )
```

`size()` (collection/string length) is the ONE function in the v0.1 subset —
the canonical empty/non-empty-check idiom (`size(items) > 0`). Everything else
is **reserved** · arithmetic · CEL macros (`has()`, `all()`, `exists()`) · and
string-manipulation functions (`startsWith`, `matches`, `contains`, …) — not in
the v0.1 subset, addable in a later minor (CEL is a superset, so growth is
additive and never breaking). If you need richer logic today, compute it in a
`nika:assert` builtin or an `infer:` task.

##### Formal grammar · CEL v0.1 subset (normative · grammar version `cel-subset/0.1`)

Prose + examples are not re-implementable; this EBNF is. A conformant engine
parses exactly this grammar inside `${{ }}` (it is a strict subset of
[cel-spec](https://github.com/google/cel-spec) — any full CEL parser accepts
every expression below) ·

```ebnf
expr     = or ;
or       = and , { "||" , and } ;
and      = rel , { "&&" , rel } ;
rel      = unary , [ relop , unary ] ;        (* at most ONE relation · non-associative ·
                                                 `a < b < c` is a parse error *)
relop    = "==" | "!=" | "<" | "<=" | ">" | ">=" | "in" ;
unary    = { "!" } , postfix ;
postfix  = primary , { "." , IDENT , [ "(" , ")" ]
                     | "[" , expr , "]" } ;
primary  = literal | list | call | IDENT | "(" , expr , ")" ;
call     = "size" , "(" , expr , ")" ;
list     = "[" , [ expr , { "," , expr } ] , "]" ;
literal  = INT | FLOAT | STRING | "true" | "false" | "null" ;

IDENT    = /[A-Za-z_][A-Za-z0-9_]*/ ;          (* `true·false·null·in` are reserved words *)
INT      = /-?[0-9]+/ ;
FLOAT    = /-?[0-9]+\.[0-9]+/ ;
STRING   = /'([^'\\]|\\.)*'/ | /"([^"\\]|\\.)*"/ ;   (* escapes · \\ \' \" \n \t *)
```

**Side constraints (normative)** ·

1. **The only callable is `size`** · free form `size(x)` (exactly 1 argument)
   and method form `x.size()` (exactly 0 arguments). Any other call suffix is
   a parse error (reserved for future minors).
2. **Precedence** (tightest → loosest) · postfix (`.` `[]`) → `!` → relational
   (`==` `!=` `<` `<=` `>` `>=` `in`) → `&&` → `||`. Parentheses override.
3. **Relations do not chain** · `rel` admits at most one `relop`
   (non-associative) — `a == b == c` must be written `(a == b) == c` if that
   is really meant.
4. **No implicit coercion** · the subset is strongly typed per CEL ·
   comparing values of different types (`42 == "42"`) is an evaluation error
   (`NIKA-VAR` · `variable_error`) · not `false`.
5. **`when:` is boolean** · statically-non-boolean-SHAPED roots (a bare
   string/number literal · a bare reference with no relation or boolean
   operator) MUST be rejected at parse time (`NIKA-VAR-005` ·
   `validation_error`); an expression that passes the static shape check
   but evaluates non-boolean fails at evaluation (`NIKA-VAR-006` ·
   `variable_error`). See §`when:` shape rules below.
6. **Identifier roots resolve against the namespaces** · the 5 global
   namespaces (`vars` · `with` · `tasks` · `env` · `secrets`) plus the two
   `for_each` loop-locals (`item` · `index`) per
   [04-variables.md](./04-variables.md) §Resolution order · an unresolvable
   root is `NIKA-VAR-001`.

The grammar is versioned (`cel-subset/0.1`) · later minors may only ADD
productions (arithmetic · macros · string functions) — never change the
meaning of an expression that parses today.

**Namespaces are CEL variables** · the <!-- canon:namespaces -->5<!-- /canon --> namespaces (`vars` · `with` · `tasks`
· `env` · `secrets`) are bound as top-level CEL variables. `tasks.<id>.status`
etc. resolve against the live DAG state. **Inside a `for_each` task body, two
more scoped CEL variables are bound** · `item` (the current element) and `index`
(its 0-based position) — available ONLY within that task (the <!-- canon:namespaces -->5<!-- /canon --> namespaces are
global · `item`/`index` are for_each-local · see `for_each` below).

#### Referencing a task requires an explicit `depends_on`

If a task references `tasks.<id>` inside a `${{ }}` expression — in `when:` ·
`with:` · `for_each:` · or any verb field (`prompt:` · `command:` · `args:` ·
…) — that task **MUST** declare `<id>` in its `depends_on:`. The engine **rejects the workflow at parse
time** otherwise (`NIKA-DAG-003` · `validation_error`) — it does **not** silently
infer the edge (a verb-body reference is an edge too · no invisible edges).

```yaml
# ❌ REJECTED at parse — `when:` reads tasks.test but no depends_on
- id: deploy
  when: ${{ tasks.test.status == 'success' }}
  exec: { command: "./deploy.sh" }

# ✅ CORRECT — the reference is backed by an explicit edge
- id: deploy
  depends_on: [test]
  when: ${{ tasks.test.status == 'success' }}
  exec: { command: "./deploy.sh" }
```

**Why explicit, not inferred** · an inferred edge is an invisible edge — it
makes the DAG harder to read and lets a typo (`tasks.tset`) silently change
ordering. Requiring the declaration keeps the graph honest: every dependency
is visible in `depends_on`, and a dangling reference is a loud parse error, not
a race. (This is the one rule an LLM most often gets wrong — so it fails fast.)

**Two surfaces are deliberately NOT in this rule** ·

- **`output:`** — its values are pure **jq** over the task's OWN raw output
  ([04 §output binding](./04-variables.md#output-binding--output)) · they never
  contain `${{ }}` · `tasks.X` cannot legitimately appear there.
- **`on_error:` / `on_finally:`** — a `recover:` reference reads a *fallback
  source*, an `on_finally:` reads its *own parent* · neither is an
  execution-order edge ([05 §on_error](./05-errors.md#error-recovery--on_error)
  defines the recovery-time resolution semantics).

**Implementation** · an engine MAY embed a CEL library (e.g. the Rust
`cel-interpreter` crate) OR hand-roll the small v0.1 subset above — both are
conformant because the subset is exactly CEL. The Core conformance suite tests
the subset against the CEL spec.

A `when:` expression evaluates to a boolean. If `false`, the task is
**skipped** (not failed) · status `skipped` · downstream sees it as completed.

Common patterns ·

```yaml
when: ${{ tasks.build.status == 'success' }}
when: ${{ tasks.test.output.coverage > 80 }}
when: ${{ vars.env == 'production' }}
when: ${{ tasks.a.status == 'success' && tasks.b.status == 'success' }}
when: ${{ tasks.deploy.status in ['success', 'skipped'] }}
when: ${{ !(tasks.test.status == 'failure') }}
```


### `when:` shape rules · boolean-only · one rule, two enforcement times

```yaml
- id: send_alert
  depends_on: [check]
  when: ${{ tasks.check.alert_count > 0 }}     # CEL expression evaluating to bool
  invoke: { ... }
```

`when:` accepts exactly two forms · a **`${{ }}` CEL expression** (the general
case) or the **YAML boolean literal `true` / `false`** (the always/never
pattern · `when: true` runs the task regardless of upstream outcome · see
§Task states). Anything else is rejected.

**Parse time (MUST · `NIKA-VAR-005` · `validation_error`)** — statically
non-boolean-SHAPED roots are rejected before any execution ·
```yaml
when: ${{ vars.threshold }}                    # ❌ bare reference · no relation/boolean operator
when: ${{ tasks.X.output }}                    # ❌ bare reference
when: ${{ 'production' }}                      # ❌ bare literal
when: "literal string"                          # ❌ neither ${{ }} nor a YAML boolean
```

**Evaluation time (`NIKA-VAR-006` · `variable_error`)** — an expression whose
*shape* is boolean but whose runtime value is not (a typed comparison across
types · a reference that resolves non-boolean through an operator the static
pass could not see) fails when evaluated.

For non-boolean values · use explicit comparison ·
```yaml
when: ${{ vars.threshold > 0 }}                # explicit > comparison
when: ${{ vars.message != "" }}                # empty string check
when: ${{ size(vars.items) > 0 }}              # collection size check
```

---

### `for_each` · *optional · map a task over a collection*

```yaml
- id: scrape_all
  for_each: ${{ vars.urls }}                  # a static list OR a prior task's array output
  max_parallel: 5                              # optional · cap concurrent iterations · default unbounded
  fail_fast: false                             # optional · false = keep going on errors · default true
  with:
    page: ${{ item }}                          # ${{ item }} = the current element
  invoke:
    tool: nika:fetch
    args: { url: "${{ with.page }}" }
```

`for_each` runs the task **once per element** of the collection. Inside the
task body, `${{ item }}` resolves to the current element (and `${{ index }}`
to its zero-based position). The collection is either a literal list or a
reference to an upstream task's array output — this is the **matrix /
fan-out** pattern familiar from GitHub Actions.

#### ⚠️ Parallel by default

By default · `for_each` iterations run **in parallel** (engine spawns all
iterations concurrently · bounded by `max_parallel:` if set).

This is **different from Python's sequential `for` loop**. If you need
sequential iteration · set `max_parallel: 1` ·

```yaml
- id: process_in_order
  for_each: ${{ vars.items }}
  max_parallel: 1                              # iterations run one-at-a-time, in order
  exec:
    command: "process ${{ item }}"
```

#### `max_parallel:` · *optional · cap concurrent iterations*

```yaml
for_each: ${{ vars.urls }}     # 1000 URLs
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
for_each: ${{ vars.urls }}
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

- The task's output is the **array of per-iteration outputs**, in input
  order · referenced downstream as `${{ tasks.scrape_all.output }}`
  (an array) · `${{ tasks.scrape_all.output[0] }}` for one element.
- **`output:` bindings apply per iteration** — each binding's jq runs over
  that iteration's raw response · downstream `tasks.X.<name>` is the
  **array of that binding's per-iteration values**, input order (so
  `tasks.X.output` = array of raw outputs · `tasks.X.title` = array of
  titles · positions align).
- **A failed iteration contributes `null`** at its index (in `.output`
  AND in every named binding) — positional alignment survives partial
  failure (the zip patterns stay sound). Per-iteration
  `on_error: { recover: … }` substitutes its recovery value instead.
- The collection MUST be an array (a literal list or an upstream array
  output). A non-array collection (object · string · number · `null`)
  is an evaluation error (`NIKA-VAR-006` · `variable_error`).
- `for_each` is **bounded fan-out**, not recursion · a task cannot
  `for_each` over its own output. The DAG stays acyclic.
- If the collection is empty · the task is `skipped` (status `skipped`).
- `when:` is evaluated **once** before the fan-out · `retry:` /
  `on_error:` / **`timeout:`** apply **per iteration** — the timeout
  clock covers one element's execution including its own retries (and
  backoff sleeps · wall-clock). There is **no whole-fan-out timer** in
  v0.1 (bound total work via `max_parallel:` + the per-iteration cap).
- `max_parallel:` + `fail_fast:` apply uniformly across all iterations.
- `on_finally:` (see below) runs **once** after all iterations complete
  (success OR failure) — `item` / `index` are NOT in scope there (there
  is no current element after the fan-out).

This is the one construct that lets a v1 workflow process a
runtime-computed number of items (N files · N search hits · N pages)
without statically enumerating tasks.

### `timeout` · *optional · task-level timeout (Go duration string)*

```yaml
- id: long_task
  timeout: "5m"             # 5 minutes
  exec:
    command: "./long-running.sh"
```

Hard timeout for the entire task (including any retries and their backoff
sleeps · wall-clock). If exceeded · the task fails with a typed timeout error
(`NIKA-TIMEOUT-001`). On a `for_each` task the clock applies **per iteration**
(§for_each semantics). A timeout error is **catchable** by `on_error:`
(recover/skip like any failure) but never retryable (`transient: false` · the
timeout already covered the retries by definition).

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

### `with` · *optional · variable scope injection*

```yaml
- id: summarize
  depends_on: [research]
  with:
    content: ${{ tasks.research.output }}   # task output reference
    style: "concise"                        # literal value
    config:                           # nested object
      max_words: 100
  infer:
    prompt: "Summarize · style ${{ with.style }} · ${{ with.content }}"
```

Injects variables into the task's scope. The variables are referenced
via `${{ with.<name> }}` substitution within the task body.

See [04-variables.md](./04-variables.md) for the full substitution grammar.

### `retry` · *optional · retry policy*

See [05-errors.md](./05-errors.md).

### `on_error` · *optional · error recovery*

See [05-errors.md](./05-errors.md).

### `output` · *optional · output binding*

```yaml
- id: api_call
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

Defines named bindings extracted from the verb's raw response via a jq expression. These bindings are available downstream as `${{ tasks.task_id.user_count }}`, `${{ tasks.task_id.first_user }}`, etc.

If `output` is absent · the task output defaults to the verb's raw response, referenced as `${{ tasks.task_id.output }}`.

---

## DAG execution model

A v0.1-compliant engine MUST ·

1. **Parse** · validate envelope · validate task ids unique · validate verbs · validate `depends_on` references resolve
2. **Topology** · compute topological order · detect cycles · reject with error if cyclic
3. **Schedule** · group tasks into waves (each wave = tasks whose deps are all done) · execute each wave in parallel (engine MAY use a thread/task pool · configurable concurrency)
4. **Evaluate `when`** · before starting each task · skip if false
5. **Execute** · run the verb · capture output · bind via jq
6. **Propagate** · on success · advance · on failure · honor `retry:` · then `on_error:` · then fail downstream
7. **Complete** · workflow done when all tasks reached terminal state (success · failure · skipped)

---

## Task states

| State | Meaning |
|---|---|
| `pending` | Task has not started · waiting on deps |
| `running` | Task is currently executing |
| `success` | Task completed successfully |
| `failure` | Task failed (after retries · no `on_error:` recovery) |
| `skipped` | Task was skipped (`when` evaluated false) |
| `cancelled` | Task was cancelled (workflow cancellation or upstream failure) |

A downstream task sees an upstream's status via `${{ tasks.task_id.status }}`.
**Only the four terminal states are observable from expressions** (the closed
enum of [04](./04-variables.md#-taskxoutput--task-output-reference)) —
`pending` / `running` exist in run reports and events, never inside `${{ }}`
(a dependent's expressions evaluate only once all its deps are terminal).

**The gate.** The default `depends_on` behavior (no `when:`) is to run only
when all deps are `success` or `skipped` — any dep ending `failure` or
`cancelled` makes the default gate unsatisfiable and the task is `cancelled`.
An **explicit `when:`** REPLACES the default gate · it is evaluated once all
deps are terminal, whatever their status · `true` → run (the always-pattern ·
`when: true` literally) · `false` → `skipped`. Workflow-failure interaction ·
[05 §workflow-level semantics](./05-errors.md#workflow-level-error-semantics).

> **`depends_on` IS the success-gate.** Do NOT write
> `when: ${{ tasks.X.status == 'success' }}` as a plain gate — it is **redundant**
> (`depends_on` already requires success). Use `when:` ONLY for conditions BEYOND
> the default gate · a value check (`tasks.X.output.coverage > 80`) · an env check
> · or to **exclude a skipped** upstream (`when: status == 'success'` is meaningful
> only when X may be `skipped` via `on_error: skip`).

---

## Examples

### Linear chain

```yaml
tasks:
  - id: a
    infer: { prompt: "Step 1" }
  - id: b
    depends_on: [a]
    infer: { prompt: "Step 2 · prev was ${{ with.prev }}" }
    with: { prev: ${{ tasks.a.output }} }
  - id: c
    depends_on: [b]
    infer: { prompt: "Step 3 · prev was ${{ with.prev }}" }
    with: { prev: ${{ tasks.b.output }} }
```

### Parallel fan-out

```yaml
tasks:
  - id: setup
    exec: { command: "./prepare.sh" }
  - id: analyze_a
    depends_on: [setup]
    infer: { prompt: "Analyze A" }
  - id: analyze_b
    depends_on: [setup]
    infer: { prompt: "Analyze B" }
  - id: analyze_c
    depends_on: [setup]
    infer: { prompt: "Analyze C" }
  - id: merge
    depends_on: [analyze_a, analyze_b, analyze_c]
    with:
      a: ${{ tasks.analyze_a.output }}
      b: ${{ tasks.analyze_b.output }}
      c: ${{ tasks.analyze_c.output }}
    infer:
      prompt: "Merge · ${{ with.a }} · ${{ with.b }} · ${{ with.c }}"
```

`analyze_a` · `analyze_b` · `analyze_c` run in parallel after `setup` · `merge` runs after all three.

### Conditional branch

```yaml
tasks:
  - id: check
    exec: { command: "./check-env.sh", capture: structured }

  - id: build_prod
    depends_on: [check]
    when: ${{ tasks.check.output.env == 'production' }}
    exec: { command: "./build.sh --release" }

  - id: build_dev
    depends_on: [check]
    when: ${{ tasks.check.output.env != 'production' }}
    exec: { command: "./build.sh --debug" }

  - id: deploy
    depends_on: [build_prod, build_dev]
    exec: { command: "./deploy.sh" }
```

Exactly one of `build_prod` or `build_dev` runs · the other is skipped · `deploy` runs after both (one success + one skipped).

### Map fan-out (`for_each`)

```yaml
tasks:
  - id: discover
    invoke:
      tool: "nika:fetch"
      args:
        url: "https://example.com/sitemap.xml"
        mode: sitemap
    output:
      pages: ".urls[]"

  - id: summarize
    depends_on: [discover]
    for_each: ${{ tasks.discover.pages }}
    with:
      page: ${{ item }}
    invoke:
      tool: "nika:fetch"
      args:
        url: ${{ with.page }}
        mode: article

  - id: digest
    depends_on: [summarize]
    with:
      summaries: ${{ tasks.summarize.output }}      # array of per-page outputs
    infer:
      prompt: "Write a digest from these summaries · ${{ with.summaries }}"
```

`discover` finds N pages · `summarize` runs once per page (parallel,
bounded) · `digest` consumes the array of all summaries. N is computed at
runtime — no static enumeration.

### Output shape · *no `output_format` field · shape is per-verb*

There is **no `output_format` task field**. The raw output shape is determined
**per verb** — the single source of truth is the `.output` table in
[02-verbs.md](./02-verbs.md#what--tasksidoutput--holds--per-verb) ·

- `infer:` → string · or the schema object when `schema:` is set
- `exec:` → stdout string · or `{stdout, stderr, exit_code}` when `capture: structured`
- `invoke:` → the tool's response (tool-determined · string · object · or bytes)
- `agent:` → final message string · or the schema object when `schema:` is set

To **force JSON validation** of a raw output, use the per-verb mechanism that
already owns it (`infer`/`agent` `schema:` · `exec` `capture: structured`) or
the `nika:validate` builtin — never a duplicate task-level type enum (a single
source of truth · Rams 4 understandable). A `output_format` field was drafted
in pre-public hardening and **removed** · it duplicated `capture`/`schema` and
its default table had drifted out of sync with 02-verbs (the very drift a
single source prevents).

### `on_finally` · *optional · cleanup hook · ALWAYS runs*

```yaml
- id: process
  exec:
    command: "./process.sh > /tmp/output.json"
  on_finally:                                  # runs always · success/fail/timeout/cancel
    - exec:
        command: "rm -f /tmp/output.json"
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
- **Cleanup tasks have access to** `${{ tasks.<parent>.status }}` and
  `${{ tasks.<parent>.error }}` to branch behavior (e.g. only-on-error
  notification).
- **Default cleanup timeout** · 30 seconds per cleanup task (overridable
  per cleanup task via `timeout:` field).
- **Failed parent task's `on_finally:` runs BEFORE** the error propagates
  upward in the DAG (gives cleanup a chance to undo side effects).
- **Engine MUST execute** `on_finally:` on cancel (Ctrl+C) and timeout —
  for a task that **started**. A task that never ran (`skipped` gate ·
  cancelled-before-start) runs NO `on_finally:` (there is nothing to clean
  up) — a record that must land on EVERY outcome is a **terminal
  `when: true` task** (the always-pattern · §Task states), not a cleanup
  hook.
- **Engine MAY skip** `on_finally:` only if the workflow process itself
  crashes (SIGSEGV · OOM · hard kill).

#### Use cases

```yaml
# 1 · cleanup temp files (scratch_dir declared in envelope vars:)
on_finally:
  - exec: { command: "rm -rf ${{ vars.scratch_dir }}" }

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

---

## One obvious way · control-flow preference rules (normative for lints)

Several intents are *expressible* two ways; the spec names ONE as canonical.
These rules are informative for authors and **normative for linters** — a
conformant linter (the reference `one-obvious-way` rule set) warns on the
discouraged form ·

| Intent | ✅ The one way | ❌ Discouraged · why |
|---|---|---|
| « run B only if A succeeded » | `depends_on: [a]` alone — success-gating is the **default edge semantic** (a failed dependency cancels dependents · §Task states) | `depends_on: [a]` + `when: ${{ tasks.a.status == 'success' }}` — redundant restatement of the default |
| « run B even if A failed » | an explicit `when:` (it replaces the default gate · §Task states « to run regardless ») — `when: ${{ tasks.a.status in ['success','failure'] }}` reads the intent precisely | encoding it via `on_error: { skip: true }` on A — that changes A's contract for B's benefit |
| « retry on transient failure » | `retry:` — the ONE retry shape (`max_attempts` · `backoff_*` · `on_codes`) | a `when:`-guarded duplicate task · a self-referencing recovery chain |
| « provide a fallback value » | `on_error: { recover: … }` — the route stays *in the failing task* | a second task `when: ${{ tasks.a.status == 'failure' }}` for a mere value — use a task only when real *work* runs on failure |
| « cleanup that always runs » | `on_finally:` | a terminal task depending on everything with a permissive `when:` |
| « time-bound an iteration » | `timeout:` on the `for_each` task — it applies **per iteration** (§for_each semantics) | per-element timing tricks inside the body · a whole-fan-out timer (none exists in v0.1) |
| « cap fan-out concurrency » | `max_parallel:` | manual sharding into N sequential tasks |

The dividing line, stated once · **`when:` reads state to decide *whether* a
task runs · `on_error:`/`retry:` decide *what happens inside* a task's own
failure · `depends_on` is pure ordering.** A construct that restates another
construct's default is noise; a construct that smuggles another's job is a
trap. The reference validator ships these as warnings (`one-obvious-way/001`
…`/007` · table order) — never hard errors (the discouraged forms are legal ·
just not canonical).

## Forward-compat

v1 ships with these task fields · `id` · `depends_on` · `when` · `for_each` · `max_parallel` · `fail_fast` · `retry` · `on_error` · `timeout` · `on_finally` · `with` · `output` · plus the verb selector. Additional fields may be added in minor bumps (additive only). (Output *shape* is per-verb · not a task field · see [02-verbs.md](./02-verbs.md#what--tasksidoutput--holds--per-verb).)

Out of scope for v1 · `parallel:` for explicit concurrency control · `include:` for sub-workflow composition (workaround · `exec: nika run sub.yaml`). See [08-out-of-scope.md](./08-out-of-scope.md).

---

🦋 *Next · [04 · Variables](./04-variables.md)*
