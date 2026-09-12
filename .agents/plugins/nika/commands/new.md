---
description: Scaffold a workflow from an embedded template, then audit it clean
argument-hint: "[template] [file.nika.yaml]"
allowed-tools: Bash(nika new:*), Bash(nika try:*), Bash(nika check:*), Read, Edit
---

Create the requested workflow from an appropriate template, preserving an
existing file if it already owns the task. Complete the check and repairs.

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
   ledger entry). Return the exact unresolved finding if blocked; do not call that file ready.
5. Return the checked artifact and applicable run line with the user's model
   and authorized `--max-cost-usd <n>`. `mock/echo` changes envelope inference
   only; task model pins and real effects remain. This command has no run
   tool; the coordinating conversation handles already authorized execution.
