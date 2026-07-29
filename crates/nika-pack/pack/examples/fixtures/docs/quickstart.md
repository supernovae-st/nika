# Quickstart

Install the engine, then audit a workflow before you run it:

```bash
nika check my-workflow.nika.yaml
```

The audit prints the plan, the cost ceiling and the capability boundary.
Nothing executes until you say `nika run`.

## What you get

- A wave-by-wave plan of every task.
- A cost floor, before a single token is spent.
- The declared `permits:` block, read back to you.
