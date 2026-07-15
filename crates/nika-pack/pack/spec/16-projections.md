# 16 · Projections

> The first fifteen chapters define what a workflow *is* and what a run
> *proves*. This chapter defines what a tool *sees*: the **oracle
> surface** — the single, versioned, read-only projection of a workflow
> that the CLI, the language server, and the MCP tool all serve **byte
> for byte the same**. One truth, many lenses.
>
> This is the first chapter of the post-1.0, additive arc. Everything it
> adds is a **projection over the frozen `nika: v1` IR** — never a change
> to the language. A projection is a *view*; the thing viewed does not
> move.

---

## The one projection (normative)

There is **one canonical projection** of a workflow — the graph of
[03](./03-dag.md) (`graph_format: 2`) — and every surface serves *that same
projection*. `nika inspect --format json` (CLI), `nika/semanticDocument`
(LSP), and the `nika_inspect` MCP tool all carry the **byte-identical**
canonical projection; the surfaces differ only in what presentation they
wrap around it.

A **semantic document** is the LSP-side surface: the canonical projection
*verbatim*, plus the source spans a client needs to link graph nodes back
to text, plus — when the graph cannot be built — the one-word reason why.
It is a **superset** of the bare projection (the CLI and MCP serve the
projection without editor spans), and it is the **growth point** of the
oracle: holes, actions, and capabilities land here (§the additive arc),
which is why it carries its own version.

```
semantic_document = (
  semantic_document_format   # the surface's OWN version (this chapter)
  graph?                     # the canonical projection (03 · graph_format: 2) · absent when unbuildable
  reason?                    # WHY graph is absent · one word · closed vocabulary · omitted when graph is present
  spans                      # task id → the range of its declaring `id` token
)
```

- **`semantic_document_format: 1`** — the surface names its **own**
  version, independent of the nested `graph.graph_format`. Until now the
  document versioned itself *implicitly* through `graph.graph_format: 2`;
  a projection that will grow additive fields (holes · actions ·
  capabilities · §the additive arc) needs a version of its own so a
  consumer can reason about the *surface* without unwrapping the graph.
- **`graph`** is the canonical projection of [03](./03-dag.md) *verbatim*
  (`graph_format: 2` · the derived edges · the reference identity). The
  semantic document never re-derives it — it carries it.
- **`spans`** map each task id to the LSP range of its declaring `id`
  token, so a canvas or a lens links a node back to source without
  re-scanning YAML.

## `reason` · *why the graph is absent (normative · closed)*

When the graph cannot be projected, `graph` is null and `reason` names
why in **one word**, from a closed vocabulary — never a sentence, never a
duplicate of a diagnostic (the details live in the diagnostics lane, [05](./05-errors.md)):

| `reason` | Meaning |
|---|---|
| `parse` | the source did not parse — there is no tree to project |
| `findings` | the source parsed but a check finding blocks a sound graph |

The set is **closed**: a new reason is an additive change that bumps
`semantic_document_format`. `reason` is omitted entirely when `graph` is
present (a healthy document carries no reason key) — presence is the
signal, not a sentinel value.

## The triangle law (normative)

The **canonical projection** has **exactly one producer** and is served,
byte-identical, by three surfaces:

```
nika inspect --format json     ┐  the canonical projection (graph_format: 2)
nika/semanticDocument (LSP)     ├─ byte-identical · one projection, three lenses
nika_inspect (MCP)              ┘  (the LSP additionally wraps editor spans)
```

- The three MUST serve the **same canonical-projection bytes** for the
  same input — a lens that drifts is a bug, tested by parity, not by
  convention. The LSP's semantic document carries that same projection
  *verbatim* and adds spans (a presentation superset, not a re-derivation);
  the CLI and MCP serve the projection without editor spans.
- Each surface is a **typed contract**, not a JSON bag: renaming a field
  breaks the producer's compilation (and the law tests) before it can
  break a consumer silently.

## Read discipline (normative)

- **A projection always answers.** The request is a *read*, never a
  judgment. A malformed document still returns a semantic document — with
  a `reason` and whatever spans it could recover — never an error. Judgment
  lives in `nika check` (05), never in the projection.
- **Structure only.** The projection carries names, shapes, spans, and
  derived structure — **never** an env value, a secret, or resolved
  material ([10](./10-authority.md) flow laws hold here too). A projection
  that leaked a secret would be a sink; it is not one, by construction.
- **A projection is a view over the frozen IR.** It never mutates, never
  re-orders, never re-interprets the workflow. Given the same source it is
  deterministic — the same bytes, every time.

## The additive arc (normative · forward-compat)

This surface is the plug-board for the post-1.0 oracle work. Every future
field is **additive under the version key**, and every future field is a
**projection or an edit-proposal over the frozen v1 IR** — never a new
piece of the language:

- **Additive-field discipline.** A new optional field (holes · actions ·
  capabilities) bumps `semantic_document_format` and is `skip`-omitted when
  empty. A consumer that ignores an additive field still reads the whole
  older surface correctly — the same forward-compat the `graph_format: 2`
  envelope already keeps (03). A consumer MUST ignore fields it does not
  know, never fail on them.
- **Capabilities are negotiated, not assumed.** A client announces which
  extensions it understands under `experimental.nika.*`; the server offers
  an extension only when the client asked. An un-negotiated field is never
  sent.
- **Projection, never grammar.** No field on this surface adds syntax a
  workflow author writes. Holes describe *where a workflow is incomplete*;
  actions describe *an edit a tool could apply*; both are computed **from**
  the v1 IR and validated **against** it. The language stays frozen (the
  1.0 contract) while the oracle around it grows.

## What v1 deliberately does not do (yet)

The following are **named** here so the surface reserves room for them,
and **specified in their own later windows** — not shipped by this chapter:

- **`holes`** — typed descriptions of where a workflow is incomplete (a
  missing `prompt:`, an unfilled output), each anchored to a span,
  including spans for **absent** fields. (`nika/holes` · `nika/fillHole`.)
- **`actions`** — `SemanticAction`s: edit proposals a tool can apply,
  each carrying its own verification (apply-in-memory → re-check → delta)
  so an action is never offered that `nika check` would then reject.
- **`witness`** on a finding — the computed DAG/taint path that justifies
  it, promoted from thrown-away intermediate to payload.
- **`pathToRunnable`** and the planner over actions — a route from an
  incomplete document to a checkable one, searched by BFS/Dijkstra (an
  admissible A* only once its heuristic law is written).

Each stays out of the shipped surface until its window proves it. Naming
them is a reservation, never a promise they exist.

## One obvious way (normative for linters)

- A tool that shows structure reads the semantic document — it does not
  re-parse the YAML itself (one producer, one truth).
- The version a surface reasons about is `semantic_document_format` — the
  nested `graph.graph_format` versions the graph, not the document.
- An unknown field on the surface is ignored, never rejected (additive
  forward-compat).

## Related

- [03 · DAG](./03-dag.md) — the `graph_format: 2` projection this carries
- [05 · Errors](./05-errors.md) — the diagnostics lane (judgment lives there, not here)
- [10 · Authority](./10-authority.md) — the flow laws the projection also obeys (structure only)
- [15 · Proof](./15-proof.md) — the semantic identity the projected graph commits to
