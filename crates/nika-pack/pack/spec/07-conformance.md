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
| **Stdlib v0.1** | Runtime + the <!-- canon:providers -->17<!-- /canon --> providers + <!-- canon:extract_modes -->9<!-- /canon --> extract modes + <!-- canon:builtins -->28<!-- /canon --> builtins | Full reference-impl-equivalent engine |

A higher level **includes** the lower levels.

---

## Level 1 · Core conformance

An engine claims « Core v0.1-compliant » if it ·

1. **Parses** any valid v0.1 workflow YAML correctly
   - Accepts exactly `nika: v1` · a `workflow:` object carrying a kebab-case `id` · rejects any other `nika:` value (a scalar `workflow:` is the dead W1 form · `NIKA-PARSE-020`)
   - Validates the `workflow` identifier kebab-case
   - Validates typed `inputs` (type + required) · validates `config` / `const` / `secrets` shape
   - Recognizes the 4 verbs (`infer` · `exec` · `invoke` · `agent`)
   - Rejects unknown top-level fields with a clear error OR ignores with warning (engine's choice · documented behavior)

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
   - `${{ config.X }}` · `${{ secrets.X }}` resolve to declared namespaces
   - `when:` and `for_each:` expressions are valid **CEL** (the v0.1 subset · see 03-dag) and their references **resolve to known namespaces**: Core parses but does NOT *evaluate* them (no execution = no `tasks.X.status` to compare against · that is Runtime's job)
   - `output:` bindings are valid **jq** expressions (the one data language · see 04-variables) · `${{ }}` never appears inside a binding
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
the canonical CLI surface for that (Core conformance + the four static
guarantees below) ·

| Guarantee | What it reports · zero execution | Backed by |
|---|---|---|
| **Plan** | the wave topology · which tasks run in parallel · the critical path | DAG waves (Core §2) |
| **Cost ceiling** | the worst-case spend · `Σ (max_tokens × provider price)` across `infer:`/`agent:` tasks · before one token is spent | the `nika:inspect view: cost` model, run statically |
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
nika: v1
```

(The short `https://nika.sh/spec/v1/workflow.schema.json` form goes live
with the site launch · both will resolve to the same schema.) This is also what makes a Nika file
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
   - `fetch:` SSRF defense for private IP ranges
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
2. **Ships all 9 canonical extract modes** (per [stdlib/extract-modes-v0.1.md](../stdlib/extract-modes-v0.1.md))
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
| **Runtime behavioral fixtures** (`tests/runtime/`) | ⏳ **post-announce** | verb execution · task fields · events · they require an executing engine · they land with the reference engine's vertical slice (1.0.0) |
| **Stdlib behavioral fixtures** (`tests/stdlib/` · execution half) | ⏳ **post-announce** | provider/builtin/extract-mode *behavior* under the `mock` provider + HTTP mocks |

Run the static gate yourself · `python conformance/runner.py all`: the
runner output is the live count (counts in prose drift · the suite is the
source). A « Core v0.1-compliant » claim is FULLY testable today. « Runtime »
and « Stdlib v0.1 » claims are testable on their static halves today · their
behavioral halves when the behavioral fixtures publish.

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
│   └── stdlib/                # provider/extract/builtin canonical surface
│
├── runner.py                  # the static oracle · `all` is the CI gate
├── *_core.py                  # per-domain reference evaluators (type · decision ·
│                              # gateway · outcome · composition · yaml-profile ·
│                              # proof · projection) + their `*_selftest.py` sweeps
├── yaml-profile/              # R11 profile fixtures (valid/ + invalid/)
├── type-corpus/               # the generated type corpus (gen-type-corpus.py)
├── values/                    # the four-authority family (C2 · R3a · valid/ + invalid/)
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

---

🦋 *Next · [08 · Out of scope](./08-out-of-scope.md)*
