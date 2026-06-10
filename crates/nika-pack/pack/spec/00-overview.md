# 00 · Overview

> The one-page vision. Read this first.

---

## What Nika is

Nika is a **declarative YAML language for AI workflows**.

It describes the **what** ·

- which LLMs to call (`infer:`)
- which commands to run (`exec:`)
- which tools to call — including fetching a URL (`invoke:`)
- which agentic loops to spawn (`agent:`)

The **how** lives in conformant engines.

---

## Why a language?

Every AI harness today reinvents the wheel · Python files · TypeScript
classes · skills crystallized into their own runtime. None of them are
**portable**.

A portable language means ·

- One YAML workflow · runs on any conformant engine
- Read · share · review · diff like any text
- The contract is the language · not the runtime

Standards work · SQL · GraphQL · OpenAPI · Dockerfile · GitHub Actions YAML. Nika is that for AI workflows.

---

## The <!-- canon:pillars -->5<!-- /canon --> pillars · immutable forever

```
1.  ENVELOPE        nika: v1
                    workflow: my-workflow-id

2.  THE 4 VERBS     infer:  exec:  invoke:  agent:

3.  DAG SHAPE       tasks · depends_on · when · for_each · output binding

4.  VARIABLES       ${{ ... }} = CEL · ONE syntax · 5 namespaces
                    vars · with · tasks · env · secrets

5.  ERROR MODEL     NIKA-<NS>-<NNN> codes · retry semantics · structured output
```

These 5 pillars are **locked forever** at `nika: v1`. Everything else (providers · builtins · extract modes · etc.) lives in the **stdlib** and evolves independently. Minor language additions are **additive** within `v1` (feature-detected · no minor version in the file).

---

## Hello world

```yaml
nika: v1
workflow: hello

model: anthropic/claude-haiku-4-5
tasks:
  - id: greet
    infer:
      prompt: "Say hello in French"
```

---

## A more representative example

```yaml
nika: v1
workflow: scrape-and-summarize

model: anthropic/claude-sonnet-4-6
tasks:
  - id: fetch_page
    invoke:
      tool: "nika:fetch"        # fetching is a TOOL, not a verb (4-verb taxonomy)
      args:
        url: "https://example.com/article"
        mode: article          # readability extraction

  - id: summarize
    depends_on: [fetch_page]
    with:
      content: ${{ tasks.fetch_page.output }}
    infer:
      prompt: "Summarize in 3 bullets · ${{ with.content }}"

  - id: write_file
    depends_on: [summarize]
    with:
      summary: ${{ tasks.summarize.output }}
    invoke:
      tool: "nika:write"
      args:
        path: "summary.md"
        content: "${{ with.summary }}"

outputs:                              # what the workflow returns · symmetric to vars:
  summary: ${{ tasks.summarize.output }}
```

3 tasks · DAG with deps · 2 verbs (`invoke:` ×2 incl `nika:fetch` · `infer:`) · variable substitution + task output reference · an `outputs:` return contract. Note each task that references `${{ tasks.X.output }}` also lists `X` in `depends_on:` — that pairing is **required** (`NIKA-DAG-003` · the engine never infers the edge). The 4th verb, `agent:` (an agentic loop · may declare a `schema:`), is shown in [examples/](../examples/).

---

## How to read the rest

| Section | What it covers |
|---|---|
| [01 envelope](./01-envelope.md) | The header · `nika: v1` · `workflow:` · typed `vars` · `env` · `secrets` |
| [02 verbs](./02-verbs.md) | The 4 verbs · signatures · semantics |
| [03 DAG](./03-dag.md) | Tasks · `depends_on` · `when` · `for_each` · output binding |
| [04 variables](./04-variables.md) | `${{ vars · with · tasks · env · secrets }}` · <!-- canon:namespaces -->5<!-- /canon --> namespaces |
| [05 errors](./05-errors.md) | Error codes · retry · structured output schemas |
| [06 stdlib contract](./06-stdlib-contract.md) | How the stdlib versions independently |
| [07 conformance](./07-conformance.md) | What « v0.1-compliant » means |
| [08 out of scope](./08-out-of-scope.md) | Explicit defer list (memory · macros · etc.) |

**Stdlib** (versioned independently · not a spec section) · [stdlib/](../stdlib/) — **<!-- canon:providers -->14<!-- /canon --> providers · <!-- canon:extract_modes -->9<!-- /canon --> extract modes · <!-- canon:builtins -->22<!-- /canon --> builtins** (6 core · 5 file · 8 data · 1 introspection · 2 network · post ADR-086/087/088 Rams sweep 2026-05-27).

---

## What's NOT in v0.1 of the language

The following are **deferred** to stdlib v0.x or beyond ·

- Memory subsystem APIs (the engine's memory subsystem · the Connectome · separate stdlib version)
- Workflow include/import (single-file workflows only in v0.1)
- Macros / templates (no preprocessing layer)
- 24 media builtins (`pdf_extract` · `chart` · `qr_validate` · etc. · stdlib v0.x)
- Persistent jobs · scheduled execution (runtime concern · daemon at v0.3)
- Streaming output (deferred)
- Multi-workflow orchestration (deferred)

See [`08-out-of-scope.md`](./08-out-of-scope.md) for the explicit list.

---

## Forever-v0.x

This spec follows the **forever-v0.x** discipline. No v1.0 release target. The 5 pillars are locked at the `nika: v1` contract · minor language additions are additive only (feature-detected · no minor version in the file) · breaking changes would ship as a new contract (`nika: v2`) with its own spec — and per forever-v0.x, that is effectively never.

In practice · we expect v1 to last 10+ years.

---

🦋 *Less but better · Rams principle 10.*
