---
name: nika-authoring
description: Author, check and repair Nika workflows (.nika.yaml files — the workflow language for AI). Use when writing or editing a *.nika.yaml file, converting a repeated AI task or prompt chain into a workflow, or when nika check reports NIKA-XXXX findings to fix.
---

# Authoring Nika workflows

Nika turns repeatable AI work into files: one `.nika.yaml`, four verbs,
audited **before** it runs. You author the file; `nika check` is the
oracle; the human runs it.

## The loop (always)

1. **Start from a template or example**, never from scratch:
   `nika examples list` · `nika examples show <slug>` ·
   `nika new --from <template> <file>.nika.yaml`
2. **Write the file.** Envelope is always `nika: v1` +
   `workflow: <kebab-id>` + `tasks:`.
3. **Check it**: `nika check <file>` (exit 0 = clean · 2 = findings),
   then `nika check --native-strict <file>` — it fails on any
   `native-first` hint (an `exec:` a builtin covers).
4. **Repair from the diagnostics** — they name the exact task, reference
   and fix. Unknown code? `nika explain NIKA-XXXX`.
5. Repeat 3–4 until clean. **Never hand a file to the human that does
   not pass `nika check`** — and pass `--native-strict` too, unless
   every remaining `exec:` is in the exec ledger (below).
6. The human (or CI) runs it: `nika run <file>`. Preview offline with
   `--model mock/echo`; run locally with `--model ollama/<model>`.

## The four verbs (exactly one per task)

- `infer:` — an LLM call (`prompt`, `schema?` for typed output,
  `max_tokens?`)
- `exec:` — a shell command (`command`, `capture: text|structured`) ·
  **last resort**: run the native-first interrogation first (below)
- `invoke:` — a builtin or MCP tool (`tool`, `args`) · HTTP fetch is
  `tool: "nika:fetch"`, a tool, not a verb
- `agent:` — a bounded multi-turn loop (`prompt`, `tools` allowlist,
  `max_turns`, `max_tokens_total`)

## Native-first (the law)

The order is `invoke: nika:*` → `invoke: mcp:<server>/<tool>` →
`exec:`. Before writing ANY `exec:`, answer in your head:

1. **Which builtin replaces it?** `nika tools --json` is the catalog.
   HTTP (curl/wget/helper fetch) → `nika:fetch` · uploads →
   `multipart:` · site crawls → `traverse:` · file plumbing
   (cat/tee/cp/mkdir) → `nika:read`/`nika:write` (`create_dirs: true`) ·
   JSON shaping (jq/sed) → `nika:jq` (or an `output:` binding) ·
   in-place edits → `nika:edit` · image/speech provider calls →
   `nika:image_generate`/`nika:tts_generate`.
2. **Which MCP tool replaces it?** A product API deserves an MCP
   server, never a helper script.
3. **Neither?** Name the exact gap — then `exec:` is legitimate
   (build tools · git · a product CLI with no MCP surface yet) and
   goes in the ledger.

Never write a helper script (`node bin/helper.mjs …`) that wraps
HTTP/files/JSON — that is `native-first/005`, the exact failure class
this law exists for.

## Exec ledger (mandatory when any exec remains)

Every surviving `exec:` gets a row in the workflow's header comment:

```
# EXEC LEDGER ·
# | task | command | why no native path | unlock that removes it |
```

`--native-strict` + a complete ledger = a reviewable workflow.

## Discipline

- References: `${{ tasks.<id>.output }}` · `${{ vars.x }}` ·
  `${{ env.KEY }}` · `${{ secrets.X }}` (never inline a credential).
- A task that reads another task's output MUST declare it in
  `depends_on: [<id>]`.
- Models are `provider/name` (`ollama/llama3.2:3b` local-first ·
  `mock/echo` offline preview).
- Declare the blast radius: `nika check --infer-permits <file>` prints
  the tightest `permits:` block — paste it in (default-deny from then on).
- Structured output: give `infer:` a `schema:`; add
  `additionalProperties: false` for a deterministic shape.
- Auth rides `headers: { x-api-key: "${{ secrets.KEY }}" }` (masked ·
  declared in `secrets:` with its `egress:` sink) — never `exec: curl`
  for the sake of a header.
