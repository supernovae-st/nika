---
description: Scaffold a workflow from an embedded template, then audit it clean
argument-hint: "[template] [file.nika.yaml]"
allowed-tools: Bash(nika new:*), Bash(nika try:*), Bash(nika check:*), Read, Edit
---

Scaffold from a template, never from scratch — then audit until clean.

Arguments: `$ARGUMENTS` (template + destination; either may be missing).

1. No template named? `nika try` and pick the closest to the
   user's intent (say which and why, one line). No destination? Derive a
   kebab-case `<name>.nika.yaml` from the intent.
2. `nika new <template> <file>` — the scriptable scaffold.
3. Adapt the file to the user's actual task: the envelope stays
   `nika: <id>` (the id lives ON the tag) + a `tasks:` MAP keyed by
   task id · exactly ONE verb per task · prefer `invoke:` builtins
   over `exec:` (native-first) · every `infer:` carries `max_tokens`
   (the cost ceiling depends on it) · templated inputs ride an
   `inputs:` declaration, supplied with `--var key=value` at run time
   · any effect needs a `permits:` block (absent = zero authority).
4. `nika check <file>` — repair from the diagnostics until exit 0, then
   `nika check <file> --native-strict` (any remaining `exec:` needs its
   ledger entry). **Never hand the human a file that does not pass.**
5. Close with the run lines: `nika run <file> --model mock/echo`
   (offline preview · zero keys) and the real form with their model —
   plus `--max-cost-usd <n>` when spend matters.
