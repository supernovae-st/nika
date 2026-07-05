# 04 · Variables

> Nika uses **one substitution syntax everywhere** · `${{ ... }}` ·
> matching GitHub Actions. Inside strings · inside object values ·
> inside array elements · inside conditions. One syntax · one mental
> model.

---

## The one syntax · `${{ ... }}`

```yaml
# Inside a string
prompt: "Summarize · ${{ vars.topic }}"

# Inside a value position
with:
  data: ${{ tasks.research.output }}

# Inside a condition
when: ${{ tasks.test.status == 'success' }}

# Inside an array
tools:
  - ${{ vars.tool_a }}
  - ${{ vars.tool_b }}
```

If you have used GitHub Actions, this is the same. If you have not, the rule is simple · **anywhere you want a value resolved at task dispatch time, wrap it in `${{ }}`**.

**What's inside `${{ }}` is [CEL](https://cel.dev)** (Common Expression
Language · the validated, non-Turing-complete standard used by Kubernetes,
Envoy, and gRPC). A bare reference like `${{ vars.topic }}` is a CEL identifier
path that evaluates to its value; a condition like `${{ tasks.test.coverage > 80 }}`
is a CEL boolean. One expression language, everywhere: Nika does not invent a
DSL. See [03-dag.md](./03-dag.md#expression-language--a-documented-subset-of-cel) for the v0.1 CEL subset.

---

## The 5 namespaces

```
${{ vars.X }}             workflow inputs               (declared in envelope `vars:` · untyped or typed)
${{ with.X }}             task-level scope               (declared per-task `with:` block)
${{ tasks.X.output }}      task output reference          (or .status · .error · .duration_ms)
${{ env.X }}              environment variable           (non-sensitive runtime config)
${{ secrets.X }}          masked secret reference        (vault-backed · never in logs)
```

Five namespaces. That's it.

> **Loop-locals are not a 6th namespace.** Inside a `for_each` task body, two
> extra identifiers are in scope: `${{ item }}` (the current element) and
> `${{ index }}` (its 0-based position). They are **loop-scoped locals**, alive
> only within that task's body, not global namespaces. So the count stays
> « <!-- canon:namespaces -->5<!-- /canon --> namespaces » + the for-each locals where a loop is present.

**Shadowing is structurally impossible.** Every namespace is reached
through its explicit prefix (`vars.` · `with.` · `tasks.` · `env.` ·
`secrets.`): a `vars.item` and the loop-local `item` never collide
(one is `vars.item` · the other is bare `item`), a task may be named
`item` (`tasks.item.output` is unambiguous), and `with.X` never hides a
`vars.X`. The only bare identifiers in the language are the two
loop-locals, and `for_each` does not nest within a task, so there is no
scope chain · no resolution-order subtleties · nothing to shadow. This
is by construction, not by rule.

### `${{ vars.X }}` · workflow inputs

Declared once in the envelope · immutable across the workflow run · may be
**untyped** (the value is the default) or **typed** (enables validation +
schema generation for callable workflows · see [01-envelope.md](./01-envelope.md#vars--optional--workflow-inputs--untyped-or-typed)) ·

```yaml
nika: v1
workflow: research-pipeline

vars:
  topic: "Rust async runtimes 2026"    # untyped · value is the default
  output_dir: "./output"

tasks:
  - id: research
    infer:
      prompt: "Research · ${{ vars.topic }}"
```

To supply or override an input at launch ·
`nika run flow.nika.yaml --var topic="CEL subsets in 2026"` (repeatable ·
engine CLI concern). A `--var` value overrides the declared default and
satisfies a `required: true` var · an undeclared key is refused before the
run. See [01-envelope.md](./01-envelope.md#vars--optional--workflow-inputs--untyped-or-typed)
for the launch contract.

### `${{ with.X }}` · task-level scope

Declared per-task · resolves at task dispatch time · often references upstream task outputs ·

> **`with:` is optional sugar.** You can always reference `${{ tasks.X.output }}`
> directly inside any verb field. `with:` exists to (a) pre-bind + **alias**
> upstream values to short local names for readable prompts/commands, and (b)
> make a task's inputs explicit at a glance. Use it when it helps readability;
> skip it when a direct `${{ tasks.X.output }}` is clearer.

```yaml
- id: summarize
  depends_on: [research]
  with:
    content: ${{ tasks.research.output }}
    style: "concise"
  infer:
    prompt: "Summarize in ${{ with.style }} style · ${{ with.content }}"
```

### `${{ tasks.X.output }}` · task output reference

Reference any upstream task's output (or status · error · duration_ms) ·

```yaml
- id: deploy
  depends_on: [build, test]
  when: ${{ tasks.test.status == 'success' && tasks.test.output.coverage > 80 }}
  exec:
    command: "./deploy.sh ${{ tasks.build.output.artifact_path }}"
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
${{ tasks.X.<name> }}                a named output: binding (jq · see below)
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

`null` is a CEL literal (`tasks.X.output != null` is in the v0.1 subset) and
a JSON value (jq's `select(. != null)` filters it). This makes the
**diamond-join** canonical · two exclusive `when:` branches + a join that
takes whichever ran ·

```yaml
- id: pick
  depends_on: [build_prod, build_dev]      # exactly one ran · the other is skipped (null)
  invoke:
    tool: nika:jq
    args:
      input: [ "${{ tasks.build_prod.output }}", "${{ tasks.build_dev.output }}" ]
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
- An engine **MUST reject only provably-invalid paths** ·
  `NIKA-VAR-003` · `variable_error`. A path step is *provably invalid*
  when ·
  1. a **member step** lands on a schema level that declares
     `additionalProperties: false` and does NOT list the key in
     `properties`;
  2. a **member step** lands on a level whose `type` excludes `object`;
  3. an **index step** lands on a level whose `type` excludes `array`.
- The static walk covers the v0.1 subset **`properties` · `items` ·
  `type` · `additionalProperties`** only. Any other construct at a level
  (`$ref` · `oneOf` / `anyOf` / `allOf` · `patternProperties` · a missing
  `type` · …) makes that level **open**: the walk stops and the engine
  **MUST NOT** reject anything beneath it.
- Tasks with NO declared schema (every `exec:` / `invoke:` task · an
  `infer:` without `schema:`) are dynamic: paths into their output are
  never statically rejected (a wrong path surfaces at run time as
  `NIKA-VAR-001`).

This keeps the check **sound** (zero false rejections · a valid workflow
is never refused) while making the declared-schema path the
better-tooling path (one more reason structured output is the default
authoring style).

### `${{ env.X }}` · environment variable

```yaml
env:
  LOG_LEVEL: info
infer:
  prompt: "Running at ${{ env.LOG_LEVEL }} verbosity"
```

`env` holds **non-sensitive** runtime config. Values may appear in logs +
traces. For anything secret, use `secrets:` (below) instead: never put a
credential in `env`.

**Declared-only · no ambient OS fallback** · `${{ env.X }}` resolves ONLY
against the envelope `env:` block. An entry absent from the block is
`NIKA-VAR-001`: the engine never silently reads the OS environment (a
workflow's inputs are all visible in the file · sovereignty + portability).
To pass an OS value in, do it explicitly at launch
(`nika run flow.yaml --env LOG_LEVEL="$LOG_LEVEL"` · engine CLI concern).

### `${{ secrets.X }}` · masked secret reference

```yaml
secrets:
  api_key:
    source: vault
    key: prod/anthropic/api-key
headers:
  Authorization: "Bearer ${{ secrets.api_key }}"
```

A secret is always a **reference to a store** (the local `nika-vault` by
default), declared in the envelope `secrets:` block, never an inline
literal. The engine **masks** every resolved `secrets.X` value in logs,
traces, and journal events (it renders as `••••••`). This `env` / `secrets`
split is the modern secure-workflow default: non-sensitive config in `env`,
masked references in `secrets`.

> **The masking boundary (normative).** Masking covers the engine's OWN
> observability surface: logs · traces · journal · the `nika:inspect`
> output. It does NOT follow a secret value that the AUTHOR routes into a
> subprocess or tool that then re-emits it: a `secrets.X` put into
> `exec.env` (or a `nika:fetch` header) which the command echoes to stdout
> is captured verbatim into `tasks.X.output` and flows downstream like any
> other data: the engine cannot know that captured string IS the secret.
> **The contract:** the engine masks what IT prints; the author owns what
> they pipe a secret INTO. The `nika check` pre-flight (lint) flags a
> `secrets.X` reaching an `exec` capture or a tool whose output is bound,
> so the leak is caught statically before the run, not after.

---

## Output binding · `output:`

Use `output:` to define **named bindings** extracted from a task's raw response via a **jq expression** (the one data language). These bindings appear in the task's typed output and are referenced as `${{ tasks.X.<name> }}` ·

```yaml
- id: api_call
  invoke:
    tool: "nika:fetch"
    args:
      url: "https://api.example.com/v1/users"   # returns JSON · output: jq extracts
  output:
    user_count: ".data.users | length"
    first_user: ".data.users[0]"
    user_emails: "[.data.users[].email]"   # [...] collects the stream into an array
```

Downstream ·

```yaml
- id: notify
  depends_on: [api_call]
  infer:
    prompt: |
      We have ${{ tasks.api_call.user_count }} users.
      Emails · ${{ tasks.api_call.user_emails }}
```

#### Raw output vs named bindings · dual-accessible

When a task has an `output:` block defining named bindings · downstream
access is **dual-accessible** ·

```yaml
- id: api_call
  invoke:
    tool: nika:fetch
    args: { url: "..." }
  output:
    body: .body
    http_status: .status     # NOT `status:` — that name is reserved (the task's own .status)
```

Downstream ·

```yaml
# Raw output (whole structure · pre-binding extraction)
${{ tasks.api_call.output }}             # full raw JSON · including all fields the verb returned

# Named bindings (defined in output: block above)
${{ tasks.api_call.body }}               # jq .body  · the response body
${{ tasks.api_call.http_status }}        # jq .status · the HTTP status field
${{ tasks.api_call.status }}             # RESERVED · the task's own status (success|failure|…) · NOT a binding
```

**Rules** ·
- `tasks.X.output` ALWAYS returns the raw output (unmodified value
  returned by the verb · before any binding extraction)
- `tasks.X.<name>` for any `<name>` declared in `output:` block returns
  the extracted jq result
- `<name>` collisions with reserved words `output` · `status` · `error` ·
  `started_at` · `ended_at` · `duration_ms` are forbidden at parse time
  (`NIKA-PARSE` · `validation_error`: the rule is structural · schema-checkable
  via `propertyNames` · `NIKA-VAR-NNN` stays reserved for reference *resolution*
  and binding *evaluation* errors)
- If no `output:` block · only `tasks.X.output` is accessible (named
  bindings are an opt-in convenience)

### Path grammar · jq (the one data language)

Output binding uses a **jq expression**: the SAME jq as the `nika:jq` builtin.
Nika has ONE data extraction-and-transform language (jq), **not two**: the former
RFC 9535 JSONPath was dropped because jq is a superset (any JSONPath query + more)
and a workflow language must not force the author (or an LLM) to choose between
two extraction syntaxes (SOTA « one obvious way · ≤2 expression layers »). The two
expression layers are **CEL** (inside `${{ }}` · conditions + value substitution)
and **jq** (inside `output:` bindings + `nika:jq` · extraction + transform).
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
(the `jaq` Rust impl · « full stdlib ») MAY be used: it is the single data
extraction-and-transform language (`output:` bindings + the `nika:jq` builtin).

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
- **An `output:` jq expression is pure jq over the task's raw output**: it does
  NOT contain `${{ }}` (the two expression layers never nest in one string ·
  CEL reads the namespaces · jq reads the task output). To parametrize an
  extraction by a workflow value, shape the verb's *input* with `${{ }}` ·
  the jq then runs over the result. (Exposing the read namespaces as jq
  variables, `.items[] | select(.id == $vars.target)`, is a v0.2 candidate ·
  jq-native · additive · NOT in the v0.1 subset.)

---

## Resolution order

When a task is about to run · the engine resolves `${{ ... }}` references in this order ·

1. **Parse** the expression inside `${{ }}` (one of · `vars.X` · `with.X` · `tasks.X.field` · `env.X` · or a `when:` condition expression)
2. **Lookup** the value from the appropriate namespace
3. **Substitute** the value into the position
4. **Single-pass** · substitution result is NOT re-evaluated (no nested substitution)

If a reference is unresolved · the engine raises a `NIKA-VAR-001` (undefined variable) error.

### Value rendering · object → string

When a `${{ }}` reference resolves to an **object or array** and is substituted
into a **string position** (e.g. inside a `prompt:` or `command:`), it renders
as **compact JSON** · deterministic (object keys sorted · no insignificant
whitespace). Scalars render as their natural string (numbers · booleans ·
`null` → `null`). There are **no template pipe-filters** (`${{ x | json }}` is
NOT a thing · per the §locked substitution surface). To control the rendering,
extract a string with jq in `output:` (`@json` for JSON text · `tostring` /
`@text` for scalar coercion) and reference that binding. One obvious way ·
implicit compact-JSON by default · explicit jq when you need a specific shape.

A **bytes** output (tool-determined · e.g. MCP image content · a binary
`nika:read`) is **opaque** · it flows tool→tool by reference
(`${{ tasks.fetch_img.output }}` → another tool's `content:` arg · or a file
path for `infer.vision`). Bytes **cannot** be jq-extracted (jq is JSON-only)
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
  prompt: "The syntax \\${{ var.x }} is how you reference variables."
```

(Note · YAML escaping of backslash · `\\` in double-quoted strings · `\` in single-quoted or block scalars.)

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

The `${{ ... }}` substitution surface and the <!-- canon:namespaces -->5<!-- /canon --> namespaces are locked at v1. **Template pipe-filters (`${{ vars.x | json }}` · `| upper`) are NOT a growth path** (they would duplicate builtins + push CEL toward a string-DSL). Data transforms live in the `nika:jq` builtin; the `${{ }}` surface grows only with CEL-native features: the conditional `?:`, the `has()` presence macro, and the `contains`/`startsWith`/`endsWith` string tests ship in `cel-subset/0.1` ([03 §grammar](./03-dag.md)); `all`/`exists` and `matches()` regex stay reserved for a later additive minor. jq is the single extraction-and-transform language (`output:` + `nika:jq`).

Out of scope for v0.1 (deferred · see [`08-out-of-scope.md`](./08-out-of-scope.md)) ·
- Expression language (no arithmetic in templates)
- User-defined functions in templates
- Multi-pass substitution

---

🦋 *Next · [05 · Errors](./05-errors.md)*
