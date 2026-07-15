# 00 · Overview

> The one-page vision. Read this first.

---

## What Nika is

Nika is a **declarative YAML language for AI workflows**.

It describes the **what** ·

- which LLMs to call (`infer:`)
- which commands to run (`exec:`)
- which tools to call, including fetching a URL (`invoke:`)
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

3.  DAG SHAPE       tasks · with (data edges) · after (control) · when · for_each

4.  VARIABLES       ${{ ... }} = CEL · ONE syntax · 5 namespaces
                    vars · with · tasks · env · secrets

5.  ERROR MODEL     NIKA-<NS>-<NNN> codes · retry semantics · structured output
```

These 5 pillars are **locked forever** at `nika: v1`. Everything else (providers · builtins · extract modes · etc.) lives in the **stdlib** and evolves independently. Minor language additions are **additive** within `v1` (feature-detected · no minor version in the file).

---

## Pre-1.0 stability contract

> **`nika: v1` names the first stable family of the language. Its public
> stability begins at engine 1.0.0 — not before.**

Until the reference engine ships 1.0.0, the v1 grammar is **pre-stable** ·

- **0.x releases may break the grammar.** Surface spellings — task shape ·
  dependency syntax · namespace names · field names — can be rewritten
  deeply while the binary is `0.x`. The 5 pillars above are locked as
  **concepts**; their surface spellings are pre-stable like everything else
  until 1.0. What would actually be unclean is freezing ambiguities now and
  then spending years building checkers, editors and migrations to
  compensate for them.
- **No syntax compatibility is promised before 1.0.** No aliases · no
  deprecation cycles · no dual forms. When a form dies it leaves the parser
  entirely, in the same release that introduces its replacement.
- **Every break lands as ONE atomic window** · the spec changes first → the
  conformance oracle changes → engines re-vendor the pack → parser and
  runtime change → the whole example/template/conformance corpus migrates →
  LSP · MCP · editors · docs follow → the old form is gone. A release never
  ships two worlds.
- **Consumers pin exact spec commits** (`SPEC_PIN`) — never a moving branch.
  Cross-repo coherence is judged by pinned re-proof, not by compatibility
  layers.
- **Machine contracts version independently** of the language family ·
  `graph_format` · check `report_version` · run `plan_version` · trace ·
  lock · receipt formats each carry their own explicit integer and evolve
  by their own rules.
- **The meta-principle** · when tooling effort reveals a language defect,
  the spec changes first and the tooling teaches second. A client-side
  workaround is never permanent.

At engine 1.0.0 the grammar freezes · from then on `v1` changes are
additive and feature-detected, exactly as the pillars section states.

---

## Hello world

```yaml
nika: v1
workflow:
  id: hello

model: ollama/qwen3.5:4b
tasks:
  greet:
    infer:
      prompt: "Say hello in French"
```

---

## A more representative example

```yaml
nika: v1
workflow:
  id: scrape-and-summarize

model: mistral/mistral-large
tasks:
  fetch_page:
    invoke:
      tool: "nika:fetch"        # fetching is a TOOL, not a verb (4-verb taxonomy)
      args:
        url: "https://example.com/article"
        mode: article          # readability extraction

  summarize:
    with:
      content: ${{ tasks.fetch_page.output }}    # the binding IS the edge
    infer:
      prompt: "Summarize in 3 bullets · ${{ with.content }}"

  write_file:
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

3 tasks · 2 verbs (`invoke:` ×2 incl `nika:fetch` · `infer:`) · variable substitution + an `outputs:` return contract. Note the graph is DERIVED: each `with:` binding that references `${{ tasks.X.* }}` IS a typed edge — data and its dependency are one declaration, no invisible edges ([03](./03-dag.md)). The 4th verb, `agent:` (an agentic loop · may declare a `schema:`), is shown in [examples/](../examples/).

---

## How to read the rest

| Section | What it covers |
|---|---|
| [01 envelope](./01-envelope.md) | The header · `nika: v1` · `workflow:` · typed `vars` · `env` · `secrets` |
| [02 verbs](./02-verbs.md) | The 4 verbs · signatures · semantics |
| [03 DAG](./03-dag.md) | Tasks · `with:` data edges · `after:` control · `when` · `for_each` · the four graphs |
| [04 variables](./04-variables.md) | `${{ vars · with · tasks · env · secrets }}` · <!-- canon:namespaces -->5<!-- /canon --> namespaces |
| [05 errors](./05-errors.md) | Error codes · retry · structured output schemas |
| [06 stdlib contract](./06-stdlib-contract.md) | How the stdlib versions independently |
| [07 conformance](./07-conformance.md) | What « v0.1-compliant » means |
| [08 out of scope](./08-out-of-scope.md) | Explicit defer list (memory · macros · etc.) |
| [09 types](./09-types.md) | The decidable type core · `types:` · `returns:` · `decode:` · the lattice · JSON-Schema lowering |
| [10 authority](./10-authority.md) | The authority system · the effect vocabulary · `policy:` (named workflow law) · secret-flow codes · `certificate.effects` |
| [11 decision](./11-decision.md) | The decision contract · portable Decision Bundle · Evidence IR (two lattices) · Belnap logic · fixed-point Decision IR · abstention · `nika:decide` |
| [12 gateway](./12-gateway.md) | The gateway contracts · Deployment Bundle · ExecutionBackend (capabilities · lowering · readback) · AgentRuntimeAdapter (FidelityReport · AuthorityDelta) · the separation laws |
| [13 outcomes](./13-outcomes.md) | The Outcome IR · TerminalClass × Cause × Payload · the normative transition table (one source: canon) · `trace_format: 2` |
| [14 composition](./14-composition.md) | Workflow calling workflow · `invoke: workflow:` (tagged union) · the CallableContract · the ten composition laws · the trace forest |
| [15 proof](./15-proof.md) | The proof layer · the semantic hash (H = H(domain ‖ version ‖ JCS(IR))) · `nika.lock` (pin by default) · `assert:` (StaticProof · TraceVerified · Unknown) · the one receipt |
| [16 projections](./16-projections.md) | The oracle surface · one canonical projection (graph_format:2) served byte-identical across CLI · LSP · MCP · the LSP semantic document (`semantic_document_format: 1`) wraps it with spans + one-word `reason` · the additive arc (holes · actions · capabilities over the frozen IR) |

**Stdlib** (versioned independently · not a spec section) · [stdlib/](../stdlib/): **<!-- canon:providers -->16<!-- /canon --> providers · <!-- canon:extract_modes -->9<!-- /canon --> extract modes · <!-- canon:builtins -->28<!-- /canon --> builtins** (6 core · 5 file · 8 data · 2 network · 2 introspection · 4 media).

---

## What's NOT in v0.1 of the language

The following are **deferred** to stdlib v0.x or beyond ·

- Memory subsystem APIs (the engine's memory subsystem · the Connectome · separate stdlib version)
- Workflow include/import (single-file workflows only in v0.1)
- Macros / templates (no preprocessing layer)
- 22 media builtins (`pdf_extract` · `ocr` · `qr_validate` · etc. · stdlib v0.x)
- Persistent jobs · scheduled execution (runtime concern · daemon at v0.3)
- Streaming output (deferred)
- Multi-workflow orchestration (deferred)

See [`08-out-of-scope.md`](./08-out-of-scope.md) for the explicit list.

---

## Frozen language envelope

The **language** envelope is frozen at `nika: v1` forever. **There is no `nika: v2` — ever.** The version marker names the one language family; deep grammar changes happen INSIDE `v1` while the reference engine is pre-1.0 (per the [pre-1.0 stability contract](#pre-10-stability-contract) above), and after engine 1.0.0 changes are additive only (feature-detected · no minor version in the file). (This is the **language** version, independent of any engine version: the reference engine ships its own semver toward a 1.0 release, which does not touch `nika: v1`.)

In practice · we expect `nika: v1` to last 10+ years.

---

🦋 *Less but better · Rams principle 10.*
