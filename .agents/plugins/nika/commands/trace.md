---
description: Inspect a Nika run's result, trace integrity and recovery evidence.
argument-hint: [trace-or-workflow]
allowed-tools: Bash(nika trace:*), Bash(nika explain:*), Read, Glob
---

Resolve `$ARGUMENTS` to the requested run. If omitted, use `nika trace ls` and
identify the selected workflow and trace explicitly; ask only if selection
would be ambiguous. Read `nika trace show <trace>`, `nika trace outputs <trace>`
and `nika trace verify <trace>`.

Report lifecycle result, causal failing tasks, relevant outputs, integrity and
remaining uncertainty. A broken chain is an integrity failure; it does not by
itself establish why bytes changed. INCOMPLETE is missing lifecycle evidence,
not success or proof of no effects. A missing journal likewise does not prove
execution never started. Decode unknown findings with `nika explain NIKA-XXXX`.

For recovery, provide the applicable command with original inputs, model, cap
and scope preserved. Paused runs use `--resume <trace> --answer <task>=<value>`
with the user's actual answer. `--from <task-id>` and `--task <task-id>` have
execution consequences: reconcile partial effects before proposing a retry.
This diagnostic command has no run tool; return execution to the coordinating
conversation rather than claiming that only a human can launch it.

`nika trace replay` re-renders existing records. `nika trace verify
--replay <fresh-trace>` compares journals without re-executing. Use
`nika trace evidence <trace>` when an auditor pack is requested and
`nika trace export <trace>` when telemetry export is requested.
