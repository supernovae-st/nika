# Experimental Workflows

These workflows demonstrate **future features** not yet implemented in Nika v0.14.

## Status: Not Yet Implemented

| Feature | File | Target Version |
|---------|------|----------------|
| `invoke_workflow:` | workflow-composition.nika.yaml | v0.15+ |
| `checkpoint:` | graceful-shutdown-demo.nika.yaml | v0.15+ |
| `condition:` | saga-pattern.nika.yaml | v0.15+ |
| `include:` | workflow-composition.nika.yaml | v0.16+ |
| `trigger:` / `rollback_for:` | saga-pattern.nika.yaml | v0.16+ |
| Nested `tasks:` in for_each | parallel-entity-generation.nika.yaml | v0.15+ |

## Syntax Differences from v0.8

These workflows use experimental syntax that differs from the current schema:

```yaml
# Current v0.8 syntax (works)
- id: my_task
  invoke:
    mcp: novanet           # Use 'mcp:' not 'server:'
    tool: novanet_describe
    params:
      entity: "qr-code"
  use:
    ctx: previous_task.result   # Bindings under 'use:'

# Experimental syntax (not yet working)
- id: my_task
  invoke:
    server: novanet        # 'server:' not yet supported
    tool: novanet_describe
  output:
    use.ctx: result        # 'output: use.xxx' not yet supported
  condition: "{{expr}}"    # Conditions not yet supported
```

## When Will These Be Implemented?

See `ROADMAP.md` for the release schedule:
- v0.15: Basic workflow composition, checkpoint/resume
- v0.16: Full saga pattern with compensations
- v0.17: Nested for_each tasks

## Contributing

If you want to help implement these features:
1. Check the corresponding issue in GitHub
2. Read the ADR in `.claude/rules/adr/`
3. Write failing tests first (TDD)
