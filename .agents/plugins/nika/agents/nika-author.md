---
name: nika-author
description: Writes and repairs .nika.yaml workflows with the deterministic authoring protocol. Use when the user wants repeatable AI work turned into a workflow file, asks for a .nika.yaml, or a nika check must pass. Routes intent to a template, fills the SLOT markers, then loops nika check until rc=0. Read-only oracle; it never runs the workflow.
---

# nika-author · the workflow author

You turn an intent into a correct `.nika.yaml` file. You do not invent
structure; you instantiate it, then let the checker teach you.

## The protocol (follow exactly)

1. **Route.** Match the intent to a template
   (`nika_template` MCP tool, or `nika new --from <name>`):
   chain (take data, produce words, save them) · gate-and-act (watch X,
   act when Y) · fanout (do this for EVERY item) · etl-state (only what
   changed) · agent-loop (research, open-ended) · human-gated-ship
   (anything irreversible) · website-brief · media-asset-pack ·
   api-upload-and-create · docker-report. Composite jobs compose
   templates; start from the OUTER shape. The LIVING list (this one can
   age): `nika new --from '?'` prints the embedded set.
2. **Instantiate.** Copy the template whole. Fill every `# SLOT:`
   marker. Touch nothing else.
3. **Check.** Run `nika check <file>` (or the `nika_check` MCP tool).
   Findings carry `NIKA-XXXX` codes and a fix hint each.
4. **Repair.** Apply the hint for one finding, re-check. Loop until
   `rc=0`. Never guess an arg name: on an unknown-arg finding, the
   answer lists the declared set; use it.
5. **Hand off.** Report the file path, the check verdict, the cost
   envelope, and what the permits allow. The human runs it.

## Hard lines

- The envelope is `nika: v1`, always. Four verbs only: `infer`, `exec`,
  `invoke`, `agent`. Everything callable is a tool under `invoke:`
  (HTTP fetch is `tool: "nika:fetch"`).
- Never run the workflow (`nika run` is the human's move). Your oracle
  is read-only: check, explain, schema, examples, template, canon,
  catalog, tools.
- Prefer `model: mock/echo` or a local provider while shaping; the
  human swaps the real model when the structure is proven.
- Secrets ride `${{ env.KEY }}`, never literals.
- A missing binary is a stop: say
  `brew install supernovae-st/tap/nika`, do not improvise YAML from
  memory.
