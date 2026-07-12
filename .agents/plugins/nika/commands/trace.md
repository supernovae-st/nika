---
description: Read a run's flight recorder — verdict, failing task, tamper check, and the exact resume line if paused
argument-hint: [trace-or-workflow]
allowed-tools: Bash(nika trace:*), Bash(nika explain:*), Read, Glob
---

Read the journal, not a memory of the terminal — `.nika/traces/` is
the only truth about what a run did.

Target: `$ARGUMENTS` (no argument? `nika trace ls` and take the `★`
newest — say which one you picked).

1. `nika trace show <trace>` — the verdict card. Then
   `nika trace outputs <trace>` — per-task verb · duration · tokens ·
   bounded preview. The FIRST red task is the root cause; downstream
   reds are fallout.
2. `nika trace verify <trace>` — exit 0 chain intact · 2 broken (the
   journal was altered — report this FIRST) · 3 pre-chain.
3. Report in this order:
   - **Verdict** — completed · failed at `<task>` · paused on
     `<prompt>`.
   - **Root cause** — the failing task, its finding code decoded via
     `nika explain NIKA-XXXX` (fold the teaching in, never
     paraphrase from memory).
   - **Integrity** — chain intact or not.
   - **Next command** — paused:
     `nika run <file> --resume <trace> --answer <task>=<value>`
     (confirm gates take booleans) · fixed upstream:
     `nika run <file> --from <task-id>` · one flaky task:
     `nika run <file> --task <task-id>`.
4. Never re-run anything yourself — `nika trace replay` re-renders
   (never re-executes) if the human wants to watch it again; the run
   line is theirs. Dashboards? `nika trace export <trace>` emits
   OTLP/JSON lines any OTel viewer reads.
