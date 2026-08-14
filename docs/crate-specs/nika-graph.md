# nika-graph — the canonical graph projection

**Layer** L0 · **admitted** 2026-07-13 (split from `nika-schema` at the
crate-size cap · the nika-dap/nika-models precedents) · **publish** planned
with the workspace train.

## What it is

ONE projector: `project(&RawWorkflow, &CheckReport) -> GraphDoc` — the
versioned `graph_format: 3` document (spec 03 §graph projection format 2 ·
typed edges). Every
surface that draws or reasons about a workflow graph consumes THIS
document, never a private re-derivation:

- `nika inspect --format json` prints it verbatim (`--format
  mermaid|dot|ascii` render it — the renderers stay CLI-side);
- `nika lsp` serves it verbatim inside `nika/semanticDocument` (plus a
  presentation wrapper of task-id spans);
- future canvases (VS Code webview · site 3D) read the same bytes.

## The shape (wire contract · additive, spec-first)

`GraphDoc { graph_format: 3, workflow, nodes[], edges[] }` — edges are
typed `{from, to, kind, predicate?, binding?}` — nodes are
topologically sorted (wave order · stable layouts); `Node` carries the
static facts only (verb · tool · resolved model · when · fan_out ·
permits attribution · cost interval · retry/timeout/on_error · declared
outputs); `edges[].kind` is a closed enum (`depends_on` today).

Evolution law: a NEW field lands in spec 03 first, the format bump is
additive, and this crate follows — never the reverse.

## Discipline

- The source enums are `#[non_exhaustive]` upstream → every match
  carries a fail-loud wildcard (a future verb panics at projection
  time instead of emitting a silently-wrong document).
- No I/O, no YAML re-parse, no color (hex hues live with the renderers
  and their parity gates — this crate is bytes-in, facts-out).
- Metrics: projector-only — nothing to drift; the law tests live with
  the consumers (CLI render tests · LSP byte-parity test).
