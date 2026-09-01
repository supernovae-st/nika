# Quickstart · your first Nika workflow (5 minutes)

> Nika is just YAML. If you can read YAML, you can read Nika. This page
> builds up a real workflow in small steps: copy each block, run it,
> watch it grow.
>
> **Status** · authoring, static checking AND execution all work TODAY:
> `brew install supernovae-st/tap/nika`, then `nika check` + `nika run` on
> any file in this page. The spec text itself is v0.1.0-draft (GA hardening
> in progress); the language family v1 is already frozen — there is no
> `nika: v2`, ever.

---

## 1 · The smallest workflow

Two header lines + one task ·

```yaml
nika: hello

model: ollama/qwen3.5:4b

tasks:
  greet:
    infer:
      prompt: "Say hello in French"
```

- `nika:`: the mark that says « this is a nika file » AND the file's name ·
  kebab-case · one line · forever. It carries no version: the family is v1
  and there is no `nika: v2`, ever.
- `model:`: the default model · `<provider>/<name>` (the prefix picks the provider).
- one task · `infer:` calls the model. **`tasks:` is what makes this a
  workflow** — a nika file without it is a project file, and the type is read
  from the key, never from the filename.

> **Model note** · every step on this page runs local on
> `ollama/qwen3.5:4b` · zero key, nothing leaves your machine
> (`ollama pull qwen3.5:4b` first · or `lmstudio/…` · `llamacpp/…` ·
> `vllm/…`). Prefer cloud? Swap the one `model:` line for any of the
> <!-- canon:providers -->17<!-- /canon --> providers ·
> `mistral/mistral-small` · `anthropic/claude-haiku-4-5` ·
> `openai/gpt-5.2` · the rest of the file doesn't change.

---

## 2 · Chain two steps (a DAG)

Add a second task that uses the first one's output. The `with:` binding IS the
graph · `${{ tasks.<id>.output }}` reads a prior task's result ·

```yaml
nika: summarize-and-translate

model: ollama/qwen3.5:4b

tasks:
  summarize:
    infer:
      prompt: "Summarize in one sentence: Nika is a declarative YAML language for AI workflows."

  translate:
    with:
      summarize: ${{ tasks.summarize.output }}
    infer:
      prompt: "Translate to French: ${{ with.summarize }}"
```

Tasks with no dependency between them run in parallel · the engine resolves
the order from the `with:`/`after:` edges.

---

## 3 · Parameterize with inputs

Declare inputs once in `inputs:` · reference them anywhere with `${{ inputs.X }}`
(the same `${{ }}` syntax as GitHub Actions · it's [CEL](https://cel.dev)
inside) ·

```yaml
nika: translate-anything

inputs:
  text:
    type: string
    default: "Hello, world"
  target_lang:
    type: string
    default: "French"

model: ollama/qwen3.5:4b

tasks:
  translate:
    infer:
      prompt: "Translate to ${{ inputs.target_lang }}: ${{ inputs.text }}"
```

Override any input at launch · `--var key=value` is repeatable ·

```bash
nika run translate-anything.nika.yaml --var target_lang="Japanese"
```

A `--var` value overrides the declared default · satisfies a
`required: true` input (see
[spec/01-envelope.md](./spec/01-envelope.md#inputs--optional--typed-workflow-inputs)) ·
and an unknown key is refused before anything runs. A fixed value the
caller never overrides is a `const:` entry instead (a bare literal ·
`${{ const.X }}`). A value the **deployment** supplies is an `inputs:` entry
carrying `required: false` and a `default:` — same namespace, same read.

There are 5 variable namespaces · the three value authorities `inputs` ·
`const` · `secrets` plus the runtime `with` · `tasks`.
See [spec/04-variables.md](./spec/04-variables.md).

---

## 4 · Use the other verbs

There are exactly **4 verbs**: `infer` (call a model) · `exec` (run a
command) · `invoke` (call a tool) · `agent` (run an agentic loop).
Everything else (fetching a URL, querying a DB, writing a file) is a
**tool** reached with `invoke`. Here `invoke` fetches a page (the
`nika:fetch` builtin), then `infer` summarizes ·

```yaml
nika: fetch-and-summarize

model: ollama/qwen3.5:4b

tasks:
  fetch_page:
    invoke:
      tool: "nika:fetch"        # fetch is a builtin tool, not a verb
      args:
        url: "https://example.com"
        mode: article           # extract readable article text

  summarize:
    with:
      fetch_page: ${{ tasks.fetch_page.output }}
    infer:
      prompt: "Summarize: ${{ with.fetch_page }}"

  save:
    with:
      summarize: ${{ tasks.summarize.output }}
    invoke:
      tool: "nika:write"        # a stdlib builtin (nika: namespace)
      args:
        path: "./summary.md"
        content: "${{ with.summarize }}"

outputs:                        # what the workflow RETURNS · symmetric to inputs:
  summary: ${{ tasks.summarize.output }}
```

Tools are `<namespace>:<path>` · `nika:*` are stdlib builtins ·
`mcp:<server>/<tool>` are external MCP tools. See [spec/02-verbs.md](./spec/02-verbs.md).

> **One rule to internalize** · whenever a task's `${{ tasks.X.output }}`,
> another task's data crosses ONLY through `with:` (the binding is the edge) — `when:` and verb bodies read local names.
> the engine rejects an undeclared reference (`NIKA-DAG-003`), it does not
> guess the edge. Every example above pairs the two.

### The 4 verbs at a glance

```yaml
infer:  { prompt: "Summarize ${{ inputs.text }}" }            # call a model
exec:   { command: ["cargo", "test", "--lib"] }               # argv · no implicit shell
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

## 5 · Before you spend a token

The cheapest authoring order, measured on a 40+ task paid extract run:

1. **`nika check --native-strict`** until zero findings and zero hints.
2. **Probe every new builtin** in a one-task file on `mock/echo` *before*
   wiring it after a paid `infer:`.
3. **Freeze the extract schema type** (`type: integer` for numeric
   facts — never `enum: ["0","1","3"]`) before adding retry, `anyOf`,
   or more infer.
4. **The model extracts facts. `nika:jq` (or `nika:decide`) is the law.**
   Do not pay a second infer to pick a level. Hint `infer-as-law`.
   `nika check --json` must report `paid_ready: true` before you leave
   `mock/` (`.next` is the first repair; `.compiled` is the proven-law
   bit).
   The shape is
   [`13-extract-then-law`](./examples/13-extract-then-law.nika.yaml).
   Prove the law on const fixtures (`unproven-law`). The named bundle
   is [`14-decide-publish`](./examples/14-decide-publish.nika.yaml).
5. **An agent that drafts a workflow checks it in the loop.** Grant
   `nika:compose` on `agent.tools` after `nika:done`. Iterate on the
   check JSON until `valid`. Never a standalone `invoke:`
   (`NIKA-BUILTIN-COMPOSE-001`). Checking never executes. Shape:
   [`15-compose-self-check`](./examples/15-compose-self-check.nika.yaml).
6. **Pin the glob** (`exclude: "**/README.md"`) before a fan-out infer
   classifies the table of contents.

Then, and only then, swap `model:` to a paid seat.

## 6 · Check it · run it

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

You touched all 5 pillars · the **envelope** (`nika: <name>` + `tasks:`) · the
**4 verbs** · the **DAG** (`with:`/`after:` edges + task outputs) · **variables**
(`${{ }}` · <!-- canon:namespaces -->5<!-- /canon --> namespaces) · and the start of the **error model** (engines
return `NIKA-<NS>-<NNN>` codes · see [spec/05-errors.md](./spec/05-errors.md)) ·
plus the workflow's **`outputs:`** return contract (what `nika run` prints + what
a caller receives).

## Where to go next

- **[spec/](./spec/)**: the full specification (~30 pages · the contract)
- **[templates/](./templates/)**: writing your own? Instantiate a
  skeleton (6 valid, slot-marked) instead of starting blank, the
  deterministic path agents follow ([protocol](AGENTS.md))
- **[stdlib/](./stdlib/)**: the <!-- canon:providers -->17<!-- /canon --> providers · <!-- canon:extract_modes -->10<!-- /canon --> extract modes · <!-- canon:builtins -->28<!-- /canon --> builtins
- **[examples/](./examples/)**: the numbered path + the jobs, all shipped and CI-gated (the count lives in [examples/manifest.yaml](./examples/manifest.yaml))
- **[README.md](./README.md)**: why a language · repo layout · governance

---

🦋 *Less but better · one file · runs anywhere.*
