# 01 · Envelope

> Every Nika workflow starts with one header line. That line names the
> language and pins the contract version. Everything else is the
> workflow id, optional defaults, and the task graph.

---

## Minimal envelope

```yaml
nika: v1
workflow: my-workflow-id

tasks:
  - id: ...
    ...
```

Two required lines (`nika:` + `workflow:`) and a non-empty `tasks:`. That's
the **whole minimum** to be a valid Nika workflow.

---

## Full envelope

```yaml
nika: v1                                # required · language + contract version
workflow: scrape-and-summarize          # required · kebab-case · unique within file
description: "Fetch + summarize"         # optional · human-readable

# Workflow-level default model · any task may override · <provider>/<name>
model: anthropic/claude-sonnet-4-6      # optional · ollama/llama3.1 for local

# Inputs · available as ${{ vars.<name> }} · untyped OR typed
vars:
  output_dir: "./output"                 # untyped · the value is the default
  topic:                                 # typed · enables schema-gen + validation
    type: string
    required: true
    description: "Subject to research"

# Non-sensitive runtime config · available as ${{ env.<name> }}
env:
  LOG_LEVEL: info

# Sensitive values · vault-backed · masked in logs · available as ${{ secrets.<name> }}
secrets:
  api_key:
    source: vault                        # never inline · a reference to a store
    key: prod/anthropic/api-key

# The declared capability boundary · the file IS the blast radius (optional · default-deny once present)
permits:
  net:  { http: ["api.anthropic.com"] }  # only this host · everything else denied
  fs:   { write: ["./output/**"] }       # no reads · writes only under ./output
  exec: false                            # this workflow runs zero shells

# Tasks (the DAG)
tasks:
  - id: ...
    ...

# What this workflow returns · ${{ tasks.<id>.output }} refs · untyped OR typed
outputs:
  summary: ${{ tasks.summarize.output }}
```

---

## YAML profile (normative)

A workflow file is **YAML 1.2 · core schema**. Two consequences authors hit ·

- **Anchors & aliases (`&x` / `*x`) are fully supported** — they are core
  YAML · they resolve BEFORE validation (the schema sees the expanded
  document) · legitimate for de-duplicating repeated blocks (a shared
  `retry:` policy · a common `with:` shape).
- **Merge keys (`<<:`) are NOT part of the contract** — YAML 1.2 dropped
  them (they were a 1.1 extension) · parser support varies · a portable
  workflow MUST NOT use them · the reference linter rejects them at check
  time (`NIKA-PARSE` · `validation_error`).

(YAML 1.2 core also kills the 1.1 traps — `no` is a string, not `false` ·
`3:22` is a string, not sexagesimal. The quoted-duration rule of
[03 §timeout](./03-dag.md#timeout--optional--task-level-timeout-go-duration-string)
adds the belt to those suspenders.)

---

## Field-by-field

### `nika` · **required · the contract version**

```yaml
nika: v1
```

The first line of every workflow. The key `nika` declares « this is a
Nika workflow »; the value `v1` pins the **language contract version**.

`v1` is the only value for the entire lifetime of the v1 contract. Minor
additions to the language (a new optional field, a new builtin) are
**additive** and never change this value. A future `nika: v2` would be a
deliberate breaking-change generation with its own spec, not
backward-compatible with v1 — and per `forever-v0.x`, that is
effectively never.

**Anti-pattern** · do not write `nika: v1.0` · `nika: "1"` · or
`nika: 1.0`. The value is exactly `v1`.

> **Why one field, not `apiVersion` + `schema`?** Earlier drafts used a
> Kubernetes-style `apiVersion: nika.sh/v1` (the superseded ADR-021 form)
> plus a separate `schema: nika/workflow@v1` (superseded too). That is two version-ish fields and
> ceremony a workflow file does not need. Modern specs converge on a
> single version marker — OpenAPI writes `openapi: 3.1.0`, Docker
> Compose dropped its `version:` field entirely. Nika takes the
> middle, proven path: **one field, the language name as the key, the
> contract version as the value.** The engine's internal canonical URI
> stays `https://nika.sh/spec/v1` for RDF / conformance tooling — but
> the author never types a URL.

### `workflow` · **required · kebab-case · unique within file**

```yaml
workflow: scrape-and-summarize
```

A stable identifier for the workflow. Kebab-case. Used in journal events,
traces, and error messages.

The presence of `workflow:` is also the **document-type discriminator** —
it marks this file as a workflow. Future Nika document types (if any ever
ship) would use their own top-level key; there is no separate `kind:`
field in v1.

Must match · `^[a-z][a-z0-9-]*$`.

### `description` · *optional · human-readable*

```yaml
description: "Fetch article, summarize in 3 bullets, write to disk"
```

Free-form text. Not used by the engine for execution. Useful for `nika ls`
listings + LSP hover hints.

### `model` · *optional · default model · `<provider>/<name>`*

```yaml
model: anthropic/claude-sonnet-4-6      # cloud
# model: ollama/llama3.1                # local · same shape
```

Default model for any `infer:` or `agent:` verb in this workflow, as a single
**`<provider>/<name>`** string (the LiteLLM / OpenRouter / Vercel convention —
there is no separate `provider:` field). The provider prefix selects the
backend and decides local-vs-cloud (`ollama/` · `lmstudio/` = local · the rest
= cloud). See [stdlib/providers-v0.1.md](../stdlib/providers-v0.1.md) for the
14-provider catalog.

A task may override this. If absent · each `infer:`/`agent:` task must specify
its own `model:`.

### `vars` · *optional · workflow inputs · untyped OR typed*

```yaml
vars:
  # Untyped form — the value IS the default
  output_dir: "./output"
  base_url: "https://example.com"

  # Typed form — enables validation + schema generation
  topic:
    type: string                 # string · number · integer · boolean · array · object
    required: true               # default false
    default: "Rust async 2026"   # used when the caller omits it
    description: "Subject to research"
```

Inputs available in every task via `${{ vars.<name> }}` substitution.

The **untyped form** (`name: value`) is the value's default — simplest for
a workflow you run yourself. The **typed form** (`name: { type, required,
default, description }`) lets the engine validate inputs and
**generate a callable schema** — this is what powers `nika.run_workflow`
over MCP (a caller like an agent host sees the typed inputs and knows
exactly what to pass) and UI generation. Simple stays simple; power is
there when a workflow becomes a reusable, callable unit. Typed `vars:` are
the **input** half of that callable contract; typed [`outputs:`](#outputs--optional--the-workflows-return-value--untyped-or-typed)
(below) are the **output** half.

**The discriminator (normative)** · a var whose value is an **object
carrying a string `type:` key** IS a typed declaration — `type:` must then be
one of the closed enum (`string` · `number` · `integer` · `boolean` · `array`
· `object`) or the workflow is rejected (`NIKA-PARSE` · `validation_error`).
An untyped object default that legitimately contains a `type` key
(`config: { type: "custom" }` would be misread) MUST use the typed form
explicitly · `config: { type: object, default: { type: "custom" } }`.

See [04-variables.md](./04-variables.md) for the full substitution grammar.

### `env` · *optional · non-sensitive runtime config*

```yaml
env:
  LOG_LEVEL: info
  REGION: eu-west
```

Non-sensitive configuration available via `${{ env.<name> }}`. Values may
appear in logs and traces. For anything secret, use `secrets:` instead.

### `secrets` · *optional · vault-backed · masked*

```yaml
secrets:
  api_key:
    source: vault                 # vault | env | file · never an inline value
    key: prod/anthropic/api-key
  github_token:
    source: env                   # read from the OS env var below · still masked
    key: GITHUB_TOKEN
  signing_pem:
    source: file                  # read file contents at resolve time · masked
    path: ~/.keys/signing.pem
```

The shape is **discriminated by `source:`** (required) · `vault` and `env`
require `key:` · `file` requires `path:` · an entry with neither (or an
inline literal value) is rejected (`NIKA-PARSE` · `validation_error`).

Sensitive values available via `${{ secrets.<name> }}`. A secret is always
a **reference to a store** — never an inline literal. The engine **masks**
resolved secret values in logs, traces, and journal events.

**`source` · closed enum** (the only three v0.1 values) ·

| `source` | `key` means | Use |
|---|---|---|
| `vault` (default) | path in the local `nika-vault` | the sovereign default |
| `env` | name of an OS environment variable | 12-factor / CI secrets |
| `file` | path to a file holding the value | Docker / k8s mounted secrets |

The `env` / `secrets` split is the modern secure-workflow default: non-sensitive
config in `env:` (appears in logs), masked references in `secrets:` (never
logged) — note `source: env` reads a *secret* from an env var and still masks
it, which is different from the plain `env:` block.

### `permits` · *optional · the declared capability boundary*

```yaml
permits:                          # the workflow's entire blast radius, declared in-file
  fs:   { read: ["./data/**"], write: ["./out/**"] }   # path globs (gitignore-style)
  net:  { http: ["api.example.com", "*.github.com"] }  # host allowlist — the SSRF boundary, in-file
  exec: false                     # may this workflow run shells? false | true | ["git", "cargo"] (program allowlist)
  tools: ["nika:read", "nika:write", "mcp:browser/*"]  # the builtin/MCP surface it may invoke
```

`permits:` makes the **file itself the security boundary** — an auditable
property of the workflow, not a runtime flag. It is **optional and
non-breaking**: a workflow with no `permits:` block runs exactly as today
(bounded only by the engine's own SSRF + blocklist floor).

**Semantics (normative) · once `permits:` is present, every category is
DEFAULT-DENY unless listed** ·

| Category | When listed | When the `permits:` block is present but this category is omitted |
|---|---|---|
| `fs.read` / `fs.write` | only the matching path globs are allowed | **no** filesystem read / write |
| `net.http` | only the listed hosts (globs ok) — tightens the engine SSRF floor, never loosens it | **no** outbound network |
| `exec` | `false` = no shells · `true` = any (still blocklist-gated) · array = only those program names (argv `command[0]`) | treated as `false` — **no** `exec:` |
| `tools` | only the matching `nika:` / `mcp:` ids (globs ok) | **no** `invoke:` of any tool |

So `permits: {}` is a workflow provably limited to pure compute (`infer:` +
CEL + `nika:jq`) — zero fs, zero net, zero shell, zero tools. That property
is checkable BEFORE the run.

**The engine MUST enforce `permits:` on BOTH surfaces** ·
1. **Statically** (`nika check`) · a `nika:write ./etc/x` outside `fs.write`,
   a `nika:fetch` to an unlisted host, an `exec:` under `exec: false`, an
   `invoke:` of an unlisted tool → a **lint error** (the run is refused
   before it starts).
2. **At runtime** · any effect escaping the declared set fails the task
   `NIKA-SEC-004` (`security_error` · never fed back to an `agent:` model —
   a capability boundary is not negotiation material). This catches the
   dynamic cases a static check cannot (a host computed at run time).

`permits.net.http` and the agent `tools:` whitelist compose: the agent
whitelist scopes ONE task's tools; `permits.tools` scopes the WHOLE workflow
(the union ceiling). An agent may never be granted a tool outside `permits`.

### `tasks` · **required · non-empty**

```yaml
tasks:
  - id: task_1
    ...
  - id: task_2
    ...
```

The DAG. See [03-dag.md](./03-dag.md) for the task model.

### `outputs` · *optional · the workflow's return value · untyped OR typed*

```yaml
outputs:
  # Untyped form — just a reference to a task output
  summary: ${{ tasks.synthesize.output }}

  # Typed form — declares the return shape · powers the callable-workflow output schema
  report:
    value: ${{ tasks.write_report.output }}
    type: string
    description: "The final markdown brief"
```

`outputs:` declares **what the workflow returns** — the symmetric twin of
`vars:` (what it takes in). Each entry is a name bound to a
`${{ tasks.<id>.output }}` reference (or any `${{ ... }}` expression), in the
**untyped form** (bare reference) or the **typed form**
(`{ value, type, description }`).

This single block serves three consumers ·

- **`nika run`** — prints this object as the workflow result (without `outputs:`,
  the CLI result is engine-defined and implicit).
- **`nika.run_workflow` over MCP** — a caller (agent host · parent workflow)
  receives exactly this shape. Together with typed `vars:` it forms the
  **complete callable contract** · typed in, typed out.
- **Schema generation** — typed outputs generate the *output half* of the
  callable schema (typed `vars:` generate the input half).

If `outputs:` is omitted, the workflow still runs; its result is
engine-defined (a reusable/callable workflow SHOULD declare `outputs:`). The
referenced task ids must exist (parse-time validated).

> **`outputs:` (envelope · plural) ≠ `output:` (task · singular).** The
> workflow-level `outputs:` is the *return contract*; the task-level `output:`
> ([04-variables.md](./04-variables.md#output-binding--output)) defines *named
> jq bindings* on one task. Plural-at-the-top, singular-per-task — the
> same split GitHub Actions uses for `workflow_call.outputs` vs step `outputs`.

### What leaves a run · the export contract (normative)

Exactly **three** things cross the run boundary · nothing else is promised ·

1. **The `outputs:` object** — when declared, a conformant engine's run
   command prints it as a **single JSON object on stdout** (and returns the
   same shape over MCP). Diagnostics · progress · logs go to **stderr** ·
   never interleaved into the stdout JSON.
2. **The exit code** — maps 1:1 to the workflow final state
   ([05 §workflow-level semantics](./05-errors.md#workflow-level-error-semantics)) ·
   `success` → **0** · `failure` → **non-zero** · `cancelled` → **non-zero**
   (distinct from failure's code · engine-documented). Parse/validation
   rejection (the run never started) is also non-zero.
3. **Tool side effects** — whatever the workflow's tools wrote (files ·
   notifications · MCP calls). The language does not track these; they are
   the workflow's business.

A schema'd **machine run report** (per-task statuses · timings · costs ·
ProofChain-compatible provenance) is deferred — see
[08 §Horizon postures H10](./08-out-of-scope.md#horizon-postures--the--did-you-think-of-x--table-2026-06-10) ·
until then `nika:inspect` exposes run introspection *inside* the run, and
the completion event (05) is the engine-side hook.

---

## YAML conventions · no traps

A Nika file is **YAML 1.2** (which is a strict superset of JSON — every Nika
workflow can also be written as JSON). YAML 1.2 is mandated specifically to
avoid the classic YAML 1.1 footguns that bite generated configs:

| Trap (YAML 1.1) | What happens | The rule |
|---|---|---|
| `region: no` | parsed as boolean `false` (the « Norway problem ») | YAML 1.2 keeps it a string · still, **quote** bare words that look boolean (`no` · `yes` · `on` · `off`) |
| `id: 0755` | parsed as octal `493` | **quote** numbers with leading zeros · `"0755"` |
| `at: 12:30` | parsed as sexagesimal `750` | **quote** colon-bearing scalars |
| `v: 1.10` | parsed as float `1.1` (trailing zero lost) | **quote** version-like strings · `"1.10"` |

**One rule that removes all of them** · when a scalar *could* be misread as a
number, boolean, or date, **quote it**. When in doubt, quote.

**Expressions** · a bare `${{ … }}` reference is a safe plain scalar
(`prompt: ${{ vars.topic }}` is fine). But **quote** any expression that
contains `:` `#` `[` `{` `,` or `>` so YAML does not misparse it ·

```yaml
when: "${{ tasks.x.status == 'ok' && tasks.y.count > 3 }}"   # quoted · contains > and :
prompt: ${{ vars.topic }}                                    # bare ok · no special chars
```

A conformant engine parses YAML 1.2. Authoring tools (and the AI writing
these files) should quote-by-default for the four ambiguous-scalar cases above.

---

## What the envelope is NOT

- It is NOT a place to inline credentials. Use `secrets:` with a `source` reference.
- It is NOT a place for engine runtime config (global timeouts · concurrency limits). Those live in engine config files, out of scope of the spec.
- It is NOT a place for imports / includes. v1 is single-file workflows. (Static composition is a candidate for a later additive minor — see [08-out-of-scope.md](./08-out-of-scope.md).)

---

## Examples

### Minimal

```yaml
nika: v1
workflow: hello

tasks:
  - id: greet
    infer:
      prompt: "Hello"
      model: anthropic/claude-haiku-4-5
```

### Full · with typed inputs (callable over MCP)

```yaml
nika: v1
workflow: research-pipeline
description: "Research a topic and write a markdown brief"

model: anthropic/claude-sonnet-4-6
vars:
  topic:
    type: string
    required: true
    description: "Subject to research"
  output_path:
    type: string
    default: "./brief.md"

tasks:
  - id: research
    infer:
      prompt: "Research the topic · ${{ vars.topic }} · in 5 paragraphs"

  - id: write
    depends_on: [research]
    with:
      content: ${{ tasks.research.output }}
    invoke:
      tool: "nika:write"
      args:
        path: "${{ vars.output_path }}"
        content: "${{ with.content }}"
```

---

## Conformance

A v0.1-compliant engine MUST ·

1. Reject any workflow missing `nika:` or `workflow:` with a clear error
2. Accept exactly `nika: v1` · reject any other value (`v1.0` · `1` · `v2` …) with a clear error
3. Validate `workflow` identifier kebab-case format
4. Make workflow-level `model`, `vars`, `env`, `secrets` available to all tasks as defaults
5. Validate typed `vars` (type + required) before execution · reject missing required inputs
6. Validate each typed `outputs` value against its declared `type:` at run end · a value that does not match its declared type fails the run (`NIKA-VAR-009` · `validation_error`) — the callable contract is enforced on BOTH halves (typed in via `vars`, typed out via `outputs`) · symmetric with rule 5
7. Mask resolved `secrets` values in all logs · traces · journal events
8. Enforce a declared `permits:` block on both surfaces — refuse statically-detectable escapes at check time, and fail any runtime effect outside the boundary with `NIKA-SEC-004` · once `permits:` is present every category is default-deny unless listed

---

🦋 *Next · [02 · The 4 verbs](./02-verbs.md)*

---

### Multi-line strings · canonical `|`

For multi-line `prompt:` · `system:` · `command:` · or any free-form text
field · use the **literal block** indicator `|` (keeps newlines verbatim) ·

```yaml
prompt: |
  Line 1 keeps its newline
  Line 2 keeps its newline
  Line 3 ends with a trailing newline
```

**YAML multi-line forms · ranked for Nika** ·

| Form | Newlines | Trailing newline | Verdict |
|---|---|---|---|
| `|` | preserved | preserved | **✅ canonical** · use for prompts · system · command · long strings |
| `|-` | preserved | STRIPPED | ✅ alternative · for compact prompts without trailing newline |
| `>` | folded to SPACES | preserved | **❌ forbidden in prompts** · whitespace-collapses · corrupts LLM intent |
| `>-` | folded to SPACES | STRIPPED | **❌ forbidden in prompts** · same whitespace issue |

**Why forbid `>` and `>-` in prompts** · they collapse newlines into
spaces · which often changes LLM behavior (intended paragraphs become
one long line). Engines MAY warn or reject `>` / `>-` for prompt/system/
command fields at parse time.

**Single-line strings** · prefer **unquoted** when no special chars · use
**double-quoted** `"..."` when escaping is needed (`\n` · `\t` · etc.) ·
use **single-quoted** `'...'` for literal strings with quotes.

```yaml
# Unquoted (preferred)
prompt: Say hello in French.

# Double-quoted (when escaping needed)
prompt: "Line 1\nLine 2 (escaped newline)"

# Single-quoted (literal · no escapes)
prompt: 'He said "hi"'

# Multi-line literal (most common · use this for any prompt > 1 line)
prompt: |
  You are a helpful assistant.
  Answer the user's question in 3 sentences.
```

