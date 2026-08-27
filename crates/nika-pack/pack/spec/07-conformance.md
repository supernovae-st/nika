# 07 · Conformance

> An engine MAY claim « v0.1-compliant » if it passes the conformance
> test suite at [`../conformance/tests/`](../conformance/). This document
> defines the conformance levels and what each requires.

---

## Notation

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL
NOT**, **SHOULD**, **SHOULD NOT**, **MAY**, and **OPTIONAL** in this
specification are to be interpreted as described in [BCP 14](https://www.rfc-editor.org/info/bcp14)
([RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) + [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174))
when, and only when, they appear in all capitals.

---

## Conformance levels

Three nested levels · increasing scope ·

| Level | What it covers | Use case |
|---|---|---|
| **Core** | Parse + validate · DAG semantics · variable resolution · error structure | Linters · spec editors · static analyzers |
| **Runtime** | Core + verb execution | Working engine (with own provider/tool impls) |
| **Stdlib v0.1** | Runtime + the <!-- canon:providers -->17<!-- /canon --> providers + <!-- canon:extract_modes -->10<!-- /canon --> extract modes + <!-- canon:builtins -->28<!-- /canon --> builtins | Full reference-impl-equivalent engine |

A higher level **includes** the lower levels.

---

## Level 1 · Core conformance

An engine claims « Core v0.1-compliant » if it ·

1. **Parses** any valid v0.1 workflow YAML correctly
   - Accepts a kebab-case `nika:` id (`^[a-z][a-z0-9-]*$`) · rejects any other shape, `v1` included (`NIKA-PARSE-003`) — the key carries the file's NAME, and the version slot is gone forever and losslessly
   - Reads the document TYPE from `tasks:` (present ⇒ workflow · absent ⇒ project), never from the filename
   - Validates typed `inputs` (type + required) · validates `const` / `secrets` shape
   - Recognizes the 4 verbs (`infer` · `exec` · `invoke` · `agent`)
   - **Rejects** unknown fields with a clear error (`NIKA-PARSE-005` · strict) — at every level, not only the top (D-2026-08-11-N20)

2. **Computes DAG topology** correctly
   - Derives E_d from `with:` bindings (role per referenced field) and E_c from `after:` entries · G_p = E_d ∪ E_c ([03 §four graphs](./03-dag.md#the-four-graphs-normative))
   - Detects cycles in G_p · rejects with `NIKA-DAG-001`
   - Detects a `with:`/`after:` reference to an undeclared task · rejects with `NIKA-DAG-002`
   - Rejects an `after:` predicate outside the closed set · `NIKA-DAG-005`
   - Rejects `depends_on:` (dead form) · `NIKA-PARSE-024` · and a `tasks.*` reference outside the boundary · `NIKA-VAR-021`
   - Detects an `on_error.recover` reference to a task downstream of the declaring task · rejects with `NIKA-DAG-004` (the await would deadlock · [05](./05-errors.md#recover-reference-resolution-normative))
   - Computes topological waves for parallel execution

3. **Resolves variable references** correctly (static · reference-resolution · NOT runtime evaluation)
   - `${{ inputs.x }}` · `${{ const.x }}` resolve to declared envelope `inputs:` / `const:` entries
   - `${{ with.x }}` resolves to a declared task `with:` key
   - `${{ tasks.X.field }}` resolves to a declared upstream task + a valid field name
   - `${{ secrets.X }}` resolves to a declared `secrets:` entry
   - `when:` and `for_each:` expressions are valid **CEL** (the v0.1 subset · see 03-dag) and their references **resolve to known namespaces**: Core parses but does NOT *evaluate* them (no execution = no `tasks.X.status` to compare against · that is Runtime's job)
   - `extract:` bindings are valid **jq** expressions (the one data language · see 04-variables) · `${{ }}` never appears inside a binding
   - Reports undefined references with `NIKA-VAR-001` · static expression violations with `NIKA-VAR-005` (the deep-static layer · CEL subset parse · jq compile · `when:` boolean shape)

4. **Produces typed errors** matching the v0.1 spec
   - `code` follows `NIKA-<NAMESPACE>-<NNN>` format
   - `category` is one of the closed enum values
   - `transient` correctly set

5. **Passes** all tests in `conformance/tests/core/`

A Core-compliant engine does NOT execute verbs and does NOT evaluate `when:` / `for_each:` over runtime state. It parses · validates · builds the DAG · resolves variable *references* (syntax + namespace validity) · produces typed errors. Runtime evaluation of conditions and iteration is Level 2.

### `nika check` · the static pre-flight (the audit-before-it-runs surface)

Because the language is **statically analyzable by construction** (the DAG
is acyclic, `for_each` is bounded, CEL is non-Turing, and effects are
declared), a conformant engine can answer « what will this workflow do, cost,
and touch? » with **zero API calls and zero tokens spent**. `nika check` is
the canonical CLI surface for that (Core conformance + the static
guarantees below) ·

| Guarantee | What it reports · zero execution | Backed by |
|---|---|---|
| **Plan** | the wave topology · which tasks run in parallel · the critical path | DAG waves (Core §2) |
| **Cost ceiling** | the worst-case spend · `Σ (max_tokens × provider price)` across `infer:`/`agent:` tasks · before one token is spent | the `nika:inspect view: cost` model, run statically |
| **Energy ceiling** | the worst-case watt-hours over the same tasks, per scope class · the honesty law below | a sourced `Wh` per million OUTPUT tokens, catalog data |
| **Secret leak** | every `secrets.X` that flows into an `exec` capture or a tool whose output is bound (the masking boundary · [04 §secrets](./04-variables.md)) | reference graph |
| **Capability escape** | any effect outside a declared `permits:` block: a write outside `fs.write`, a fetch to an unlisted host, an `exec` under `exec: false`, an unlisted tool | `permits:` ([01](./01-envelope.md)) |
| **Provider parity** | (`--providers`) that the workflow uses zero provider-specific fields → the same `schema:` runs identically on all <!-- canon:providers -->17<!-- /canon --> providers (incl. the 5 local) | the closed verb-field set |

This is the property no other AI workflow runner gives: **GitHub Actions,
Temporal, and LangGraph tell you nothing (and charge you nothing back)
until you run.** A Nika workflow is auditable for cost, capabilities,
secrets, and portability *as a static fact about the file*. `nika check` is
an engine CLI surface (not a separate conformance level: it composes Core
validation with the cost/secret/permits/parity reports); the guarantees it
surfaces ARE normative (they derive from Core conformance + the `permits:`
and `secrets:` MUSTs), the CLI ergonomics around them are the engine's.

### The spend-honesty law · *normative · cost and energy alike*

The two spend readings are one doctrine in two units. Money is the
volatile one — it rots with every price change and means nothing on a
local provider; watt-hours are the durable one. Both speak **the same
four words**, and neither may print a number that hides what it could
not bound ·

| Word | What it claims |
|---|---|
| **floor** (`≥`) | the provable minimum · the real figure can only be higher |
| **ceiling** (`≤`) | the provable worst case · a task's declared cap × a sourced rate, summed over the bounded tasks |
| **UNBOUNDED** | a task with no provable limit, **named** one by one, carrying the why (`no max_tokens declared` · the author can fix it) |
| **unpriced** | the task will spend, and no sourced rate exists for its model |

The law (MUST) ·

1. **Name the bounded part, name every unbounded task, and never print
   a total that hides either.** A surface that cannot honor this prints
   nothing rather than a wrong number. The two rungs MUST read ONE
   classification of the tasks — the same task has the same shape on
   both — and an unbounded task is named **once**, not once per rung:
   the second rung counts it and points at the first. Two adjacent
   readings that describe one task differently teach the reader to
   distrust both.
2. **Unknown stays unknown.** A figure the engine cannot source renders
   `unpriced`, never zero. A local model is `unpriced` — the operator's
   watts — never « free ». Absence of a rate is not a rate of zero, and
   `0` is the one rendering a reader takes as a promise.
3. **A proven zero is not an unknown.** A task whose `for_each`
   iterates a literal EMPTY collection provably never runs: it is
   counted **never-run** and gets no row. A ceiling over a task that
   cannot execute would be invented, and two adjacent rungs would
   describe the same task differently. *(Measured residual, 2026-08-13 ·
   `nika 0.108.0` — the ENERGY rung honours this; the COST rung still
   prints such a task at `$0.0000`, so the two rungs do disagree about
   it today. The `never-run` class exists on the energy arm only. The
   law is the target for both.)*
4. **Watt-hours sum inside a scope class and never across one.** An
   energy fact declares what it covers — `gpu` (accelerator only) ·
   `device` (whole host) · `fleet` (host + idle + datacenter overhead)
   — and the same model differs by roughly 2× between the ends of that
   ladder. A mixed workflow reports one subtotal per class: not a
   refusal, and never a sum that describes nothing.
5. **A figure carries its origin and its date, or it does not enter.**
   Every energy rate is stated per million OUTPUT tokens (decode
   dominates measured inference; a per-total figure would dilute the
   number with nearly-free prefill), and rides a closed provenance —
   `measured-local` · `independent-measured` · `vendor-claim` ·
   `independent-estimate` — with the month it was measured. Modelling
   may enter, wearing its label; it may never enter as a silent
   default. Figures rot with hardware generations: a dateless or
   sourceless one is refused rather than believed.

The rate table itself is engine data, not language surface — as is any
budget that *enforces* a ceiling (§Observability in
[08](./08-out-of-scope.md) defers those). What is normative here is the
reading: what a spend claim is allowed to say.

### Editor tooling · the canonical JSON Schema

The spec ships a canonical **JSON Schema** at
[`schemas/workflow.schema.json`](../schemas/) describing the envelope +
task shape + verb argument structures. It is the machine-readable companion to
this prose spec (kept in sync · the prose is normative on conflict).

Editors (VS Code · Zed · JetBrains · Neovim) pick it up via the standard
`yaml.schemas` association (or a `# yaml-language-server: $schema=…` modeline)
to give **autocomplete + inline validation** as you type, the same DX as
GitHub Actions and Docker Compose. A working modeline today:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/supernovae-st/nika-spec/main/schemas/workflow.schema.json
nika: my-workflow
```

#### The address MUST become versioned and immutable (normative · D-2026-08-11-N24)

⚠️ **The modeline above points at a moving branch, and that is a defect with
two independent causes.**

*The first is ours.* Since [§unknown key](#an-unknown-key-is-an-error-at-every-level-normative--d-2026-08-11-n20)
an unknown key is an error, so this schema's `additionalProperties: false` is
no longer a lint preference — it is the accepted surface of the language. Served
from `main`, that surface moves under every editor in the world **while no file
has changed**. A workflow that validated on Monday shows red on Tuesday because
a schema it never named was edited. This is precisely the drift the strict rule
exists to prevent, arriving through the back door.

*The second is the host's.* `raw.githubusercontent.com` fronts a cache that
serves stale bytes: a schema can be correct at the commit and wrong at the
reader for an unspecified window. **Immutable upstream is not immutable at the
point of use** — measured in this workspace, 2026-08-07.

**The shape.** One address per minor, frozen forever ·

```yaml
# yaml-language-server: $schema=https://nika.sh/schema/v0.1/workflow.schema.json
nika: my-workflow
```

This is what JSON Schema, OpenAPI, Kubernetes and CWL all do, and it composes
with the `nika:` mark the envelope already carries. A file written in 2027
still reads in 2030 against **its own** schema. Until the address ships, the
modeline above is the pragmatic form and this paragraph is the standing debt —
stated, not hidden.

This is also what makes a Nika file
pleasant (and trap-free) for an AI to author: the schema constrains the shape
before the engine ever runs. CEL expressions and jq expressions inside string
fields are validated by the engine (Core level), not the JSON Schema.

**Use case** · linters · spec editors · LSP server intelligence · static analyzers.

---

## Level 2 · Runtime conformance

An engine claims « Runtime v0.1-compliant » if it satisfies Core conformance PLUS ·

1. **Executes the 4 verbs** with correct semantics
   - `infer:` calls a configured provider · returns response
   - `exec:` runs the command in a shell · honors timeout + security
   - `invoke:` resolves and calls the tool · returns response
   - `agent:` runs the multi-turn loop · honors max_turns + tools whitelist

2. **Honors task fields** correctly
   - GATE-v2 admission · every incoming edge's producer settles inside that edge's pass-set, else `cancelled` (dead-path · [03 §gate algebra](./03-dag.md#the-gate-algebra-v2-normative))
   - `when` skips when false (POST-gate · local namespaces)
   - `timeout` (Go-duration string · `"30s"` · `"5m"` etc.) hard-kills on timeout
   - `retry` strategy honored on transient errors
   - `on_error` recovery honored on terminal errors
   - `with` scope injected into task body
   - `output` binding via jq

3. **Implements security policies**
   - `exec:` blocklist (engine SHOULD ship a sane default · MAY allow override)
   - `invoke: nika:fetch` SSRF defense for private IP ranges
   - `invoke:` capability checks (engine's choice of capability model)

4. **Emits workflow events**
   - `task.started` · `task.completed` · `task.failed` · `task.skipped`
   - `workflow.started` · `workflow.completed` · `workflow.failed`
   - Event payload includes `task_id` · timestamp · status · duration

5. **Passes** all tests in `conformance/tests/runtime/`

Runtime-compliant engines may bring **their own** provider implementations · tool implementations · MCP server registries. They are not required to ship the canonical stdlib.

**Use case** · custom engines for specialized environments (embedded · WASM · custom LLM gateway · etc.).

---

## Level 3 · Stdlib v0.1 conformance

An engine claims « Stdlib v0.1-compliant » if it satisfies Runtime conformance PLUS ·

1. **Ships all <!-- canon:providers -->17<!-- /canon --> canonical providers** (per [stdlib/providers-v0.1.md](../stdlib/providers-v0.1.md))
2. **Ships all <!-- canon:extract_modes -->10<!-- /canon --> canonical extract modes** (per [stdlib/extract-modes-v0.1.md](../stdlib/extract-modes-v0.1.md))
3. **Ships at least all <!-- canon:builtins -->28<!-- /canon --> canonical builtins** (core 6 + file 5 + data 9 + network 2 + introspection 2 + media 4 · the remaining deferred media builtins are optional · the byte-determinism clauses of `nika:image_fx` and `nika:chart` are part of the bar — an engine that cannot honor them is not conformant, no waiver)
4. **Passes** all tests in `conformance/tests/stdlib/`

A Stdlib-compliant engine is functionally equivalent to the reference implementation for any workflow that uses only the canonical stdlib elements.

**Use case** · the default level for production engines.

---

## Suite status · v0.1 (honest)

What is populated TODAY vs what lands with the reference engine ·

| Layer | Status | What it proves |
|---|---|---|
| **Core fixtures** (`tests/core/`) | ✅ populated · runner-executable | parse + validate + DAG + variables + errors · the full Level-1 static contract |
| **Deep-static fixtures** (`tests/deep/`) | ✅ populated · runner-executable | the expression layer the schema cannot see · the normative CEL EBNF parsed for real · jq compile · duration grammar · schema-meta · `when:` shape · binding purity |
| **Stdlib static surface** (`tests/stdlib/`) | ✅ populated · runner-executable | the stdlib **names + shapes** layer · provider prefixes · the closed `nika:*` builtin set · extract modes · checkable with zero execution (lists derive from [`canon.yaml`](../canon.yaml)) |
| **Examples as conformance inputs** (`examples/`) | ✅ executed by the runner `all` gate | every shipped example MUST validate at the full static level |
| **Runtime behavioral fixtures** (`tests/runtime/`) | ✅ **measured by command** (2026-07-30) | verb execution · task statuses/outputs · events · the trace chain — driven through the PUBLIC doors (`nika run --json` · `nika trace verify`) by [`scripts/runtime-differential.py`](../scripts/runtime-differential.py) · every fixture agrees with the released engine |
| **Stdlib behavioral fixtures** (`tests/stdlib/behavioral/`) | 🌗 **no-network half measured** (2026-07-30) | builtin *behavior* through the run door — `nika:jq` executes for real (typed result) · the `nika:write`→`nika:read` pair roundtrips under a declared `permits.fs` · `nika:convert` converts (from:/to: required) — same differential, same sweep as the runtime tier. The network half (fetch under HTTP mocks · provider behavior beyond `mock/echo`) stays post-announce |

(The provider prefix list is a **registry, not grammar** — the freeze
holds the form, the membership grows additively under the registration
policy in [providers-v0.1.md §Registration
policy](../stdlib/providers-v0.1.md) · no NEP for a membership row, a
NEP for any change to the form.)

Run the static gate yourself · `python conformance/runner.py all`: the
runner output is the live count (counts in prose drift · the suite is the
source). Run the behavioral tier yourself ·
`NIKA_BIN=<engine> python3 scripts/runtime-differential.py` — same law,
the differential's summary is the live count. A « Core v0.1-compliant »
claim is FULLY testable today; « Runtime » claims are testable on both
halves (static fixtures + the behavioral differential) · « Stdlib v0.1 »
on its static half, its behavioral half when those fixtures publish.

---

## Conformance test structure

```
conformance/
├── tests/
│   ├── core/                  # Level-1 fixtures · parse · validate · DAG · variables · errors
│   ├── deep/                  # deep-static layer · CEL subset parse · jq compile ·
│   │                          # durations · schema-meta · when shape · binding purity
│   ├── lints/                 # advisory-lint fixtures
│   ├── runtime/               # verb execution · task fields · events (behavioral half)
│   │   ├── trace/            #   the journal, judged by the walk (a verdict)
│   │   ├── receipt/          #   the receipt, judged by its reading (what projects)
│   │   ├── registry/         #   a cache record × an operator floor, judged by admission
│   │   ├── energy/           #   a workflow, judged by the energy reading it must report
│   │   └── resume/           #   a journal × a compat declaration, judged by the version crossing
│   └── stdlib/                # provider/extract/builtin canonical surface
│
├── runner.py                  # the static oracle · `all` is the CI gate
├── *_core.py                  # per-domain reference evaluators (type · decision ·
│                              # gateway · outcome · composition · yaml-profile ·
│                              # proof · projection) + their `*_selftest.py` sweeps
├── yaml-profile/              # R11 profile fixtures (valid/ + invalid/)
├── type-corpus/               # the generated type corpus (gen-type-corpus.py)
├── values/                    # the three-authority family (C2 · R3a · valid/ + invalid/)
├── types/                     # io-declaration predicate vocabulary (C2 · R3b · valid/ + invalid/)
├── gates/                     # the after: predicate vocabulary (C2 · R5 · valid/ + invalid/)
└── runner-protocol.md         # how to run the suite against any engine
```

**The directory is the source** — this sketch is a map, not the inventory
(`python3 conformance/runner.py all` prints the live fixture counts).
Each fixture is a pair · `input.yaml` (the workflow to feed) + `expected.json` (the expected output or error structure).

For tests that require executing against real LLMs / networks · the suite uses the `mock` provider and HTTP mocks to keep tests deterministic.

---

## Runner protocol

A conformance runner ·

1. Reads each `input.yaml`
2. Pipes it to the engine being tested (`engine run --input -`)
3. Captures the engine's output (stdout JSON · structured)
4. Compares against `expected.json`
5. Reports pass/fail per test · final summary

See `conformance/runner-protocol.md` for the exact JSON wire format.

---

## Claiming conformance

The public claim string is **« Nika v1 Conformant — <Level> (spec <commit>) »**
· Level ∈ `Core` · `Runtime` · `Stdlib v0.1` · the spec commit is the pin
the suite ran against. One form everywhere — badges · READMEs · listings ·
release notes. (In normative sentences, « v0.1-compliant » names the level
*bar* an engine satisfies; the string above is the one public *claim*.)

To make the claim · an engine ·

1. MUST pass the conformance suite at the claimed level
2. MUST name the level and the spec commit in the claim (the format above)
3. MAY open a PR on [supernovae-st/nika-spec](https://github.com/supernovae-st/nika-spec) to be listed in `CONFORMANT_IMPLEMENTATIONS.md`

The claim is earned by passing the suite, never by declaration
([NEP-0000 §Relationship to conformance](../governance/nep-0000-the-nep-process.md)).
The reference implementation [supernovae-st/nika](https://github.com/supernovae-st/nika) targets Stdlib v0.1 conformance.

---

## Versioning

A conformance claim is **specific to a spec version**. As the stdlib evolves to v0.2 · engines re-claim conformance against the new suite. The Core conformance level is stable forever within v1 of the language.

### An unknown key is an error, at every level (normative · D-2026-08-11-N20)

An engine MUST refuse a key it does not know (`NIKA-PARSE-005`). It MUST NOT
accept-and-warn. **This clause used to leave the choice to the engine, and a
choice here is not a detail: it is the whole growth model.** Two conformant
engines were entitled to disagree on every future key — one refusing a file
the other ran — which is the definition of a language that has not decided
what it is.

**Why refusal is the safe direction, and acceptance is not.** The instinct
runs the other way: accept-and-warn looks forward-compatible, since a file
written for a later minor would still run on an older engine. It is the wrong
instinct here, because **a key in this language can REMOVE authority.** A
future rule that forbids something would be silently dropped by an older
engine, and the file would run with **more** authority than its author
granted — failing open, in the one language whose premise is that an absent
grant means zero. Refusal fails closed: the file does not run at all, and the
author learns why.

**What it buys, and it is not small.** Every key this spec reserves for a later
minor lands **additively and for free** under refusal: no valid file uses one
today, so admitting it later breaks nothing. Adding to a closed space is always
compatible; the closure is what makes the growth safe. It also
catches the misspelling — `permit:` written for `permits:` — which today is
accepted in silence and yields *zero authority* without saying so, the most
expensive typo the language can carry.

**Precedent, and it is not ours.** CWL treats an unrecognised field as *a
fatal error*; the field its readers may ignore is the advisory one, not the
structural one. A serious specification refuses.

⚠️ **A corollary that is not optional.** Under strict refusal, the published
JSON Schema (`additionalProperties: false`) can no longer be served from a
moving branch: it needs a **versioned, immutable address** (one per minor).
Otherwise the editors of the world watch the accepted surface move under them
while no file has changed — the drift the refusal exists to prevent, arriving
by the back door.

---

🦋 *Next · [08 · Out of scope](./08-out-of-scope.md)*
