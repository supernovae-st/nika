# Quickstart · your first Nika workflow (5 minutes)

> Nika is just YAML. If you can read YAML, you can read Nika. This page
> builds up a real workflow in 5 small steps: copy each block, run it,
> watch it grow.
>
> **Status** · authoring, static checking AND execution all work TODAY:
> `brew install supernovae-st/tap/nika`, then `nika check` + `nika run` on
> any file in this page. The spec text itself is v0.1.0-draft (GA hardening
> in progress); the language envelope `nika: v1` is already frozen.

---

## 1 · The smallest workflow

Two header lines + one task ·

```yaml
nika: v1
workflow: hello

model: anthropic/claude-haiku-4-5

tasks:
  - id: greet
    infer:
      prompt: "Say hello in French"
```

- `nika: v1`: the language contract (one line · forever).
- `workflow:`: a name for this file.
- `model:`: the default model · `<provider>/<name>` (the prefix picks the provider).
- one task · `infer:` calls the model.

---

## 2 · Chain two steps (a DAG)

Add a second task that uses the first one's output. `depends_on` builds the
graph · `${{ tasks.<id>.output }}` reads a prior task's result ·

```yaml
nika: v1
workflow: summarize-and-translate

model: anthropic/claude-haiku-4-5

tasks:
  - id: summarize
    infer:
      prompt: "Summarize in one sentence: Nika is a declarative YAML language for AI workflows."

  - id: translate
    depends_on: [summarize]
    infer:
      prompt: "Translate to French: ${{ tasks.summarize.output }}"
```

Tasks with no dependency between them run in parallel · the engine resolves
the order from `depends_on`.

---

## 3 · Parameterize with variables

Declare inputs once in `vars:` · reference them anywhere with `${{ vars.X }}`
(the same `${{ }}` syntax as GitHub Actions · it's [CEL](https://cel.dev)
inside) ·

```yaml
nika: v1
workflow: translate-anything

vars:
  text: "Hello, world"
  target_lang: "French"

model: anthropic/claude-haiku-4-5

tasks:
  - id: translate
    infer:
      prompt: "Translate to ${{ vars.target_lang }}: ${{ vars.text }}"
```

There are 5 variable namespaces · `vars` · `with` · `tasks` · `env` ·
`secrets`. See [spec/04-variables.md](./spec/04-variables.md).

---

## 4 · Use the other verbs

There are exactly **4 verbs**: `infer` (call a model) · `exec` (run a
command) · `invoke` (call a tool) · `agent` (run an agentic loop).
Everything else (fetching a URL, querying a DB, writing a file) is a
**tool** reached with `invoke`. Here `invoke` fetches a page (the
`nika:fetch` builtin), then `infer` summarizes ·

```yaml
nika: v1
workflow: fetch-and-summarize

model: anthropic/claude-haiku-4-5

tasks:
  - id: fetch_page
    invoke:
      tool: "nika:fetch"        # fetch is a builtin tool, not a verb
      args:
        url: "https://example.com"
        mode: article           # extract readable article text

  - id: summarize
    depends_on: [fetch_page]
    infer:
      prompt: "Summarize: ${{ tasks.fetch_page.output }}"

  - id: save
    depends_on: [summarize]
    invoke:
      tool: "nika:write"        # a stdlib builtin (nika: namespace)
      args:
        path: "./summary.md"
        content: "${{ tasks.summarize.output }}"

outputs:                        # what the workflow RETURNS · symmetric to vars:
  summary: ${{ tasks.summarize.output }}
```

Tools are `<namespace>:<path>` · `nika:*` are stdlib builtins ·
`mcp:<server>/<tool>` are external MCP tools. See [spec/02-verbs.md](./spec/02-verbs.md).

> **One rule to internalize** · whenever a task's `${{ tasks.X.output }}`,
> `with:`, or `when:` references another task, declare it in `depends_on:`.
> the engine rejects an undeclared reference (`NIKA-DAG-003`), it does not
> guess the edge. Every example above pairs the two.

### The 4 verbs at a glance

```yaml
infer:  { prompt: "Summarize ${{ vars.text }}" }              # call a model
exec:   { command: "cargo test --workspace --lib" }           # run a process
invoke: { tool: "nika:fetch", args: { url: "https://..." } }  # call a tool
agent:                                                         # agentic loop
  prompt: "Review the diff"
  tools: ["nika:read", "nika:done"]   # default-deny · grant explicitly
  schema: { type: object, required: [findings] }   # optional · structured final message
```

Exactly four · `fetch` is not among them (it's the `nika:fetch` tool via
`invoke:`). See [spec/02-verbs.md](./spec/02-verbs.md) and the runnable
[examples/](./examples/).

---

## 5 · Check it · run it

With the reference engine installed (`brew install supernovae-st/tap/nika`) ·

```bash
nika check summarize-and-translate.nika.yaml   # static audit, before a single token is spent
nika run summarize-and-translate.nika.yaml     # execute, locally, today
```

No engine handy? Validate against this repo's oracle (zero install beyond
python3) ·

```bash
python3 conformance/runner.py validate summarize-and-translate.nika.yaml
```

The same file runs on **any** v0.1-compliant engine. The language is the
contract, the runtime is an implementation detail.

---

## What you just learned

You touched all 5 pillars · the **envelope** (`nika: v1` + `workflow:`) · the
**4 verbs** · the **DAG** (`depends_on` + task outputs) · **variables**
(`${{ }}` · <!-- canon:namespaces -->5<!-- /canon --> namespaces) · and the start of the **error model** (engines
return `NIKA-<NS>-<NNN>` codes · see [spec/05-errors.md](./spec/05-errors.md)) ·
plus the workflow's **`outputs:`** return contract (what `nika run` prints + what
a caller receives).

## Zero-cloud · local-first

Swap the model prefix to run fully local · no API key, nothing leaves your
machine ·

```yaml
model: ollama/llama3.1        # or lmstudio/... · llamacpp/... · vllm/...
```

## Where to go next

- **[spec/](./spec/)**: the full specification (~30 pages · the contract)
- **[templates/](./templates/)**: writing your own? Instantiate a
  skeleton (6 valid, slot-marked) instead of starting blank, the
  deterministic path agents follow ([protocol](AGENTS.md))
- **[stdlib/](./stdlib/)**: the <!-- canon:providers -->14<!-- /canon --> providers · <!-- canon:extract_modes -->9<!-- /canon --> extract modes · <!-- canon:builtins -->23<!-- /canon --> builtins
- **[examples/](./examples/)**: 7 foundation workflows (full v0.1 construct coverage · 19 more pending for GA)
- **[README.md](./README.md)**: why a language · repo layout · governance

---

🦋 *Less but better · one file · runs anywhere.*
