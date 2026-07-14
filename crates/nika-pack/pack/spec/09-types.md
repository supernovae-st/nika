# 09 · Types

> Nika has a **decidable type core**. A workflow declares named types
> once (`types:`), tasks declare what they return (`returns:`), and the
> engine checks every deep reference, compiles structured-output
> contracts, and types every value edge — **before a token is spent**.
> The type language is deliberately small: every judgment (subtyping ·
> path validity · contract compilation) terminates, by construction.
>
> One direction, one truth: a Nika type **lowers** to JSON Schema
> 2020-12 (the lingua franca every structured-output provider speaks).
> The schema sent to a provider is a *projection* of the type — never a
> second truth to keep in sync. A raw JSON Schema stays usable as an
> escape hatch (`schema:` · [02](./02-verbs.md)); its static analysis is
> honestly weaker ([04 §static binding validation](./04-variables.md#static-binding-validation-against-a-declared-schema-normative)).

---

## The `types:` block · *optional · named type declarations*

```yaml
nika: v1
workflow:
  id: research-pipeline

types:
  Summary:
    object:
      title: string
      bullets: { array: string }
      score: { integer: { min: 0, max: 100 } }

tasks:
  summarize:
    with: { article: "${{ tasks.fetch.output }}" }
    infer:
      prompt: "Summarize · ${{ with.article }}"
    returns: Summary
```

- A type name matches `^[A-Z][A-Za-z0-9]*$` (PascalCase — disjoint from
  task ids and from the lowercase primitive names by construction).
- A name may reference other named types. References must be **acyclic**:
  a recursive type is rejected (`NIKA-TYPE-002` — unbounded recursion
  would make subtyping and lowering undecidable; v1 keeps every judgment
  total).
- An unknown name (in `types:`, `returns:`, an `outputs:` `type:`, or a
  field position) is `NIKA-TYPE-001`, with a did-you-mean fix when a
  declared name is close.

## The type grammar (closed · v1)

A **type expression** is one of ·

| Form | Meaning |
|---|---|
| `null` · `bool` · `integer` · `number` · `string` · `bytes` · `uri` · `path` · `duration` · `timestamp` | the **10** primitives (`null` may be written as the bare YAML null scalar or the string) |
| `PascalName` | a reference to a declared named type |
| `{ array: T }` | homogeneous list |
| `{ map: T }` | string-keyed map with homogeneous values |
| `{ object: { field: T, … } }` | **closed** record — see §closed objects |
| `{ union: [T, …] }` | untagged union (≥ 2 members) — **nullability is spelled here**: `{ union: [T, null] }` |
| `{ optional: T }` | **field-presence modifier — legal ONLY at a field position** inside `{ object: … }` (§optional is presence, not null) |
| `{ enum: ["a", "b", …] }` | closed string enumeration (≥ 1 member · unique) |
| `{ integer: { min?, max? } }` · `{ number: { min?, max? } }` | bounded numerics (inclusive bounds) |
| `{ string: { pattern?, min_len?, max_len? } }` | refined string (`pattern` is a regular expression **without backreferences or lookaround** — linear-time matchable) |

Three constructors are **declared and reserved** — they parse nowhere in
v1 and their semantics land with their owning wave ·

| Reserved | Owner | Why it is named now |
|---|---|---|
| `result<T>` | Outcome IR (W5) | a task outcome is a *contract*, not a convention — the slot is reserved so no user type squats the name |
| `artifact<mime>` | artifact lanes (W5) | `outputs:` already speak it informally; the typed form needs runtime identity first |
| `secret<T>` | authority wave (W4) | a secret is a *confidentiality*, not a shape — see §secrets never lower |
| `money` | decision core (W-DEC) | an amount is **fixed-point + ISO-4217 currency, never a binary float** — a `money` that is semantically an empty string would be a lie; the name is reserved until the decision core brings the real representation |

### Closed objects (normative)

`{ object: … }` is **closed by default**: at run time, a member outside
the declared fields is a violation; statically, a reference to an
undeclared member is **provably invalid** (`NIKA-VAR-003` — the same
code, the same class, one voice with the schema walk of
[04](./04-variables.md#static-binding-validation-against-a-declared-schema-normative)).
Opt out per object with `additional: true` ·

```yaml
types:
  Loose: { object: { id: string }, additional: true }
```

### Optional is presence, not null (normative)

**Absence and null are different facts**, and the grammar keeps them
apart ·

```yaml
types:
  Story:
    object:
      headline: string                     # required · present · non-null
      byline: { optional: string }         # MAY BE ABSENT · when present: a string (null refused)
      score: { union: [integer, null] }    # required · present · may be NULL
      note: { optional: { union: [string, null] } }   # may be absent · may be null
```

- `{ optional: T }` is a **field-presence modifier**: the field may be
  omitted; when present, its value must inhabit `T` — a present `null`
  is a violation unless `T` itself admits null.
- **Nullability is a union**: `{ union: [T, null] }` — one spelling,
  no `nullable<>` alias.
- `optional:` anywhere OUTSIDE a field position (a `returns:`, an array
  element, a union member, a `types:` root) is refused —
  `NIKA-TYPE-001`-class, with the teaching « optional is a
  field-presence modifier — for a nullable value write
  `union: [T, null]` ».
- Lowering: an optional field leaves `required` and lowers as
  `lower(T)` — **never** an implicit `anyOf` with null (JSON Schema's
  `required` carries presence · `type: null` carries nullability · the
  two never blur).

### Normalization (normative)

Two type expressions are compared **after normalization** — and the
canonical forms are what make `⊑` antisymmetric **as equality**: two
types that admit the same values normalize to the same form.

1. Unions flatten (`union` inside `union`), deduplicate structurally,
   and order canonically (member order never carries meaning).
2. A one-member union collapses to its member.
3. A union member **subsumed** by another member collapses away:
   `union: [integer, number]` **is** `number`, `union: [Enum, string]`
   where every enum is a string **is** `string`. (Named references are
   nominal at normalization time — a reference never subsumes and is
   never subsumed; it resolves only in the relations.)
4. A union with an `Unknown` member **absorbs to `Unknown`** — joining
   with no information is no information. (`Unknown` has no authorable
   surface; this arises only from inference joins.)
5. An **unbounded refinement is its primitive**: `{ integer: {} }` is
   `integer`, `{ number: {} }` is `number`, `{ string: {} }` is
   `string`. Bounds must be numbers with `min ≤ max` (lengths:
   non-negative integers with `min_len ≤ max_len`) — an empty range is
   refused at parse (`NIKA-TYPE-001`), never silently inhabited by
   nothing.
6. Field optionality is **not** a type — it lives on the field slot and
   never rewrites into the value type (§optional is presence, not null).

## The relations · `⊑` (subtyping) · `~` (consistency) · `⊑~` (assignability) (normative)

Three relations, never conflated — the soundness core of this chapter ·

| Relation | Nature | Laws |
|---|---|---|
| `A ⊑ B` | **static subtyping** over KNOWN types | reflexive · transitive · **antisymmetric** (a partial order — `Unknown` is comparable only to itself) |
| `A ~ B` | **gradual consistency** | reflexive · **symmetric** · NOT transitive · `Unknown ~ T` for every T · structural elsewhere |
| `A ⊑~ B` | **assignability** (consistent subtyping — what every static judge consumes: the walk · `TYPE-004` · the static fit) | `⊑` with `Unknown` accepting at the leaves — exactly Siek-Taha consistent subtyping |

Internal forms (an engine's IR MAY carry them · **never authorable
surface**): `Never` (bottom — no value inhabits it · the honest result
of a disjoint meet) and `Unknown` (absence of static information — the
gradual type). There is no authorable `Any`: the permissive role is
`Unknown`'s, and a lattice top has no v1 use case. Contradictions are
DIAGNOSTICS (refusals), never a type.

`A ⊑ B` (« every A value is a B value ») is decidable and total ·

- every type `⊑` itself (after normalization) ·
- `integer ⊑ number` ·
- `{ enum: E } ⊑ string` · `{ enum: E1 } ⊑ { enum: E2 }` iff `E1 ⊆ E2` ·
- bounded numerics: `{ integer: {min: a, max: b} } ⊑ { integer: {min: c, max: d} }`
  iff `c ≤ a` and `b ≤ d` (absent bound = unbounded) · same for `number`
  · a bounded form `⊑` its bare primitive ·
- refined strings: `⊑` bare `string` always; between two refined strings,
  `⊑` holds iff the bounds nest and the patterns are **syntactically
  equal** (regex inclusion is not decided — honest, never guessed) ·
- `uri ⊑ string` · `path ⊑ string` · `duration ⊑ string` ·
  `timestamp ⊑ string` (the newtypes narrow strings; nothing else) ·
- `array/map`: covariant in the element type ·
- objects: **width + depth + openness** — `A ⊑ B` iff every required
  field of B exists (required) in A with the field type in `⊑`, and an
  optional B field declared in A nests in `⊑` (an optional A field
  never serves a REQUIRED B slot — it may be absent); a closed B admits
  no A field outside B's declared fields (`additional: true` on B lifts
  that restriction); and an **open A** (`additional: true`) — whose
  inhabitants carry undeclared keys with ANY value — fits only an open
  B that declares **no field A leaves free** (`F_B ⊆ F_A`). An open
  object is never below a closed one ·
- unions: `A ⊑ union U` iff `A ⊑` some member; `union U ⊑ B` iff
  **every** member `⊑ B` ·
- `Unknown ⊑ Unknown` only — in the ORDER, `Unknown` is comparable to
  nothing else (antisymmetry survives). The permissive behavior lives
  in `~` and `⊑~`: `Unknown ~ T` always, and `A ⊑~ B` accepts wherever
  an `Unknown` leaf stands in for missing knowledge. Consistency is
  deliberately NOT transitive — an `Unknown` in the middle never
  launders an unrelated pair (`null ~ Unknown ~ bool`, yet `null ⋢ bool`
  and `null ⋢~ bool` — property-pinned in both evaluators).

**Join** (`⊔`) is `union_of` — the language HAS unions, so the join is
always expressible. **Meet** (`⊓`) is honest three-way: **exact** when
computable (structural intersection · nested bounds · enum
intersection), **`Never`** when the pair is provably disjoint
(`string ⊓ integer = Never` — impossibility, NOT `Unknown`:
insufficient information and impossibility are different facts and are
never interchanged), and **not-computed** (reported as such) where an
exact meet is not implemented — never guessed. Engines use both
internally (edge typing · inference); neither is authorable surface.

## Lowering · Nika type → JSON Schema 2020-12 (normative · one direction)

`lower(T)` is the **single** projection every consumer uses — the
structured-output contract for `infer:`/`agent:`, the callable-workflow
schema, the editor's completion source. It is total on the v1 grammar ·

| Nika type | JSON Schema |
|---|---|
| `null` | `{ "type": "null" }` |
| `bool` | `{ "type": "boolean" }` |
| `integer` | `{ "type": "integer" }` |
| `number` | `{ "type": "number" }` |
| `string` | `{ "type": "string" }` |
| `bytes` | `{ "type": "string", "contentEncoding": "base64" }` |
| `uri` | `{ "type": "string", "format": "uri" }` |
| `path` | `{ "type": "string" }` (the path meaning is Nika-side · no portable format) |
| `duration` | `{ "type": "string", "pattern": "^[0-9]+(\\.[0-9]+)?(ns\|us\|µs\|ms\|s\|m\|h)([0-9]+(\\.[0-9]+)?(ns\|us\|µs\|ms\|s\|m\|h))*$" }` (the quoted Go-duration of [01](./01-envelope.md)) |
| `timestamp` | `{ "type": "string", "format": "date-time" }` |
| `money` | `{ "type": "string" }` (decimal + currency refinements land with the decision core) |
| `{ enum: E }` | `{ "type": "string", "enum": E }` |
| `{ integer: {min,max} }` | `{ "type": "integer", "minimum": min, "maximum": max }` (absent bound omitted) · same shape for `number` |
| `{ string: {pattern,min_len,max_len} }` | `{ "type": "string", "pattern": …, "minLength": …, "maxLength": … }` |
| `{ array: T }` | `{ "type": "array", "items": lower(T) }` |
| `{ map: T }` | `{ "type": "object", "additionalProperties": lower(T) }` |
| `{ object: F }` closed | `{ "type": "object", "properties": …, "required": [non-optional fields], "additionalProperties": false }` |
| `{ object: F, additional: true }` | same, without `"additionalProperties": false` |
| `{ union: [T…] }` | `{ "anyOf": [lower(T)…] }` |
| named reference | **inlined** at its use site (acyclicity makes this total · no `$ref` in the lowered document, maximizing provider compatibility) |

Two laws ride the table ·

- **One direction.** There is no `raise(schema)` — an authored JSON
  Schema is never reverse-engineered into a Nika type. The hatch stays a
  hatch.
- **Secrets never lower.** No position that lowers (a `returns:`, an
  `outputs:` type, a callable-workflow input) may contain `secret<…>`
  when it lands in W4 — a type that would ship a secret's *shape* to a
  provider is refused statically (`NIKA-TYPE-005`). Reserved now so the
  hole never opens.

### The regex dialect (normative · locked)

« No backreferences or lookaround » alone does not make two engines
agree — the dialect is a **closed whitelist**, validated identically by
both evaluators at declaration time ·

- **Accepted constructs** · literals · `.` · character classes
  `[…]`/`[^…]` (ranges · the classes below) · the perl classes `\d \D
  \w \W \s \S` · escaped metacharacters (`\.` `\+` …) · quantifiers
  `* + ?` and `{m}` `{m,}` `{m,n}` (greedy only) · alternation `|` ·
  groups `(…)` and `(?:…)` · anchors `^` `$`.
- **Refused at declaration** (out of dialect · `NIKA-TYPE-006`) ·
  backreferences (`\1`…) · lookaround (`(?=` `(?!` `(?<=` `(?<!`) ·
  named groups (`(?P<`/`(?<name>`) · inline flags (`(?i)` …) · lazy or
  possessive quantifiers (`*?` `++` …) · `\b`/`\B` word boundaries ·
  unicode property classes (`\p{…}`) · octal/hex classes beyond `\xHH`.
- **Semantics** · matching is **unanchored search** (the JSON Schema
  `pattern` convention — anchor explicitly with `^…$`) · `.` does not
  match newline · the perl classes are **Unicode-aware** in the engine
  and the reference evaluator (a provider judging the lowered schema
  may be more or less generous — the ENGINE is the judge, the lowered
  pattern is advisory transport).
- **Limits** · a pattern is ≤ 512 characters · engines MUST match in
  linear time (the dialect is regular — RE2-class).
- **Invalid or out-of-dialect pattern** = `NIKA-TYPE-006` at
  declaration, in BOTH evaluators, with the offending construct named.

## `returns:` · the task's output contract (normative)

`returns:` declares **what a task's `.output` is**. One field, one
meaning, per verb ·

| Verb | What `returns:` does | Decode mechanics |
|---|---|---|
| `infer:` | compiles `lower(returns)` as the structured-output contract — the reply **is** validated against it (the same enforcement lane as `schema:` · [02](./02-verbs.md#infer--llm-inference)) | provider-side structured output · engine-side validation |
| `agent:` | same as `infer:`, over the loop's **final message** | same |
| `exec:` | asserts the **decoded** value: `Type(decoded) ⊑ returns` at run time (`NIKA-TYPE-101` on violation) | explicit `decode:` — see below |
| `invoke:` | the tool's canonical contract stays the truth; `returns:` **refines** it (statically checked against a builtin's declared output shape when the engine knows one · otherwise asserted at run time like `exec:`) | tool-defined |

- `returns:` takes a named type or an inline type expression.
- **One contract per task** · `returns:` and a verb-level `schema:` on
  the same task is `NIKA-TYPE-003` (two spellings of one contract — the
  one-obvious-way law). `schema:` alone stays legal: it is the
  out-of-core hatch, with the weaker static walk.
- **Gradual and honest** · no `returns:` = the output is `Unknown`.
  Nothing is invented: completion offers nothing beneath it, the walk
  stops, and every read is a run-time concern (exactly today's
  schema-less behavior — [04](./04-variables.md)).
- The static walk of [04 §static binding validation](./04-variables.md#static-binding-validation-against-a-declared-schema-normative)
  runs on `returns:` types with **full precision**: the v1 type grammar
  has no open construct, so every level is walkable (closed objects
  refuse phantom members as `NIKA-VAR-003`; `additional: true` and
  `Unknown` make a level open, the walk stops — sound, never guessy).

### `decode:` · how `exec:` bytes become a value (normative)

The type never silently changes how the runtime reads bytes — decoding
is **declared** ·

```yaml
tasks:
  stats:
    exec:
      command: ["jq", "-c", ".stats", "report.json"]
      decode: json               # text (default) · json · jsonl · bytes
    returns: { object: { count: integer, mean: number } }
```

- `decode:` applies to the captured **raw byte stream** (`capture:
  stdout` · `stderr` · `combined`) — the pipeline is
  `raw bytes → decode → value`, never `bytes → lossy string → decode` ·
  `text` (default — strict UTF-8; invalid UTF-8 settles the task
  `failure`, honestly: an author who wants octets says `bytes`;
  trailing newline trimmed as today) · `json` (parse one JSON document
  from the bytes) · `jsonl` (newline-delimited JSON into an array) ·
  `bytes` (no decoding · the value is the opaque octets, base64 at any
  JSON boundary).
- `decode:` with `capture: structured` is rejected (`NIKA-PARSE-025`) —
  the structured capture *is* already an object
  (`{ stdout, stderr, exit_code }`); a `returns:` on such a task types
  that object directly.
- A `decode: json`/`jsonl` whose stream does not parse settles the task
  `failure` (`NIKA-EXEC` lane · the decode is task-stage work, inside
  `on_error:` scope like every verb-stage failure · [03 §dispatch
  pipeline](./03-dag.md#the-gate-algebra-v2-normative)).
- Static coherence: a `returns:` whose type cannot come out of the
  declared decode (an `object` contract over `decode: text` · anything
  over `decode: bytes` except `bytes`) is `NIKA-TYPE-004` at check time.
- `decode: artifact-ref` is **reserved** (artifact lanes · W5).

## Typed value edges (normative)

A `with:` binding's type is **derived, never declared** ·

- `with.x` bound to `${{ tasks.P.output }}` has the producer's
  **business type — `returns(P)`, unpolluted**. That a task may settle
  `skipped` is an OUTCOME fact (`Outcome(P) = Success(T) · Skipped ·
  Failure · Cancelled` — the Outcome IR of W5), never a silent rewrite
  of its contract into `union[T, null]`.
  **Explicit W2-compatibility decision (debt · witness W2-Q3, owed
  W5)** · under W2's gate algebra a skipped producer passes a value
  edge and the binding reads defined-`null`
  ([03 §gate algebra](./03-dag.md#the-gate-algebra-v2-normative)) —
  a run-time state the static type deliberately does NOT fold into
  `returns(P)`. A consumer that must branch on availability reads the
  `.status` observation (the closed enum) — availability is observed,
  never typed. The Outcome IR resolves the debt properly; until then
  the gap is NAMED here, pinned by a runtime witness fixture and a
  property in both evaluators, and is a decision — not a consequence
  of the type core.
- A deep read (`tasks.P.output.count`) types as the walked field type.
- `.status` observations type `{ enum: ["success","failure","skipped","cancelled"] }`
  (the closed vocabulary of [03](./03-dag.md) — `NIKA-DAG-007` guards the
  literals) · `.duration_ms` types `integer` · `.error` types the error
  record of [05](./05-errors.md) (an `object`, `additional: true`).
- Producer without `returns:` → the binding is `Unknown` (gradual).

The **workflow-contract halves stay flat for now** · typed `vars:` and
typed `outputs:` keep their flat closed enum (`string · number · integer
· boolean · array · object` · [01](./01-envelope.md)) until the
input-authority window (G9) widens BOTH halves to this grammar in one
deliberate break — the callable contract never speaks two type languages
at once. This chapter's grammar binds `types:` and `returns:` today.

## Errors (the `NIKA-TYPE` namespace · new in this chapter)

| Code | Failure | Category | `transient` |
|---|---|---|---|
| `NIKA-TYPE-001` | unknown type name (in `types:` · `returns:` · an `outputs:` type) — did-you-mean when close | `validation_error` | false |
| `NIKA-TYPE-002` | recursive type reference — the `types:` graph must be acyclic | `validation_error` | false |
| `NIKA-TYPE-003` | `returns:` and `schema:` on the same task — one contract, one spelling | `validation_error` | false |
| `NIKA-TYPE-004` | `returns:` type unreachable from the declared `decode:` (an object over `decode: text` · …) | `validation_error` | false |
| `NIKA-TYPE-005` | a secret-carrying type in a lowered position (reserved with `secret<T>` · W4) | `security_error` | false |
| `NIKA-TYPE-101` | run-time contract violation — the decoded value does not fit `returns:` (`exec:`/`invoke:` lane; `infer:`/`agent:` violations stay `NIKA-INFER-002`-class, one voice with the structured-output lane) | `validation_error` | false |

## One obvious way (normative for linters)

| Rule | Instead of | Write |
|---|---|---|
| `one-obvious-way/011` | `schema:` on an `infer:`/`agent:` task whose shape fits the v1 type grammar | `returns:` (the typed door — the hatch is for out-of-core shapes) |
| `one-obvious-way/012` | an inline `returns:` object repeated across tasks | a named type in `types:` (one declaration, N references) |

## What v1 deliberately does not do

- **No recursion** in named types (undecidable walks — `NIKA-TYPE-002`).
- **No generics / type parameters** (the reserved `result<T>` ·
  `artifact<mime>` · `secret<T>` are engine-owned constructors, not a
  user-parameterizable surface).
- **No regex inclusion** judgments (syntactic equality only — honest).
- **No implicit coercions** — `integer ⊑ number` is the single numeric
  widening; a string is never silently a number.
- **No reverse lowering** — JSON Schema in, Nika types out, does not
  exist. The hatch stays a hatch.

---

## Related

- [01 · Envelope](./01-envelope.md) — typed `vars:` (the callable-input
  half) · `outputs:` (the return half)
- [02 · Verbs](./02-verbs.md) — `returns:`/`decode:` rows per verb ·
  `schema:` the hatch
- [03 · DAG](./03-dag.md) — the value-edge semantics `returns:` types
- [04 · Variables](./04-variables.md) — the static walk `returns:`
  sharpens
- [05 · Errors](./05-errors.md) — the `NIKA-TYPE` registry rows
