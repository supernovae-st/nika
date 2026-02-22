---
name: nika-binding
description: Nika data binding syntax reference (use: and {{use.alias}})
---

# Nika Data Binding

## Overview

Nika uses a two-part binding system:
1. **Output capture:** `output.use.ctx: alias` — Store task output
2. **Template reference:** `{{use.alias}}` — Use stored data

## Basic Pattern

```yaml
tasks:
  - id: fetch_data
    invoke:
      tool: novanet_describe
      server: novanet
      params:
        entity: "qr-code"
    output:
      use.ctx: entity_data    # Capture output as 'entity_data'

  - id: generate_content
    infer:
      prompt: |
        Generate content for: {{use.entity_data.display_name}}
        Description: {{use.entity_data.description}}
    flow: [fetch_data]        # Explicit dependency
```

## Output Specification

```yaml
output:
  use.ctx: <alias>            # Store entire output
  use.ctx.<field>: <alias>    # Store specific field (planned)
```

| Field | Type | Description |
|-------|------|-------------|
| `use.ctx` | string | Alias for the captured data |

## Template Syntax

```yaml
prompt: "{{use.alias}}"           # Full value
prompt: "{{use.alias.field}}"     # Nested field
prompt: "{{use.alias[0]}}"        # Array index
prompt: "{{use.alias.field[0].name}}"  # Deep nesting
```

## Examples

### Simple String Binding

```yaml
- id: step1
  infer: "Generate a title"
  output:
    use.ctx: title

- id: step2
  infer: "Expand on: {{use.title}}"
  flow: [step1]
```

### Object Field Access

```yaml
- id: get_entity
  invoke:
    tool: novanet_describe
    server: novanet
    params:
      entity: "qr-code"
  output:
    use.ctx: entity

- id: use_fields
  infer: |
    Entity: {{use.entity.display_name}}
    Key: {{use.entity.key}}
    Description: {{use.entity.description}}
  flow: [get_entity]
```

### Array Iteration (with for_each)

```yaml
- id: get_pages
  invoke:
    tool: novanet_traverse
    server: novanet
    params:
      start: "project:qrcode-ai"
      arc: "HAS_PAGE"
  output:
    use.ctx: pages

- id: process_each
  for_each:
    items: $pages
    as: page
    concurrency: 3
  infer: "Summarize: {{page.title}}"
  flow: [get_pages]
```

## Flow Dependencies

**Important:** Bindings require explicit `flow:` dependencies.

```yaml
# WRONG - no flow dependency, binding may not exist
- id: step2
  infer: "{{use.step1_result}}"  # May fail!

# RIGHT - explicit dependency
- id: step2
  infer: "{{use.step1_result}}"
  flow: [step1]  # Ensures step1 completes first
```

## Multiple Dependencies

```yaml
- id: combine
  infer: |
    Title: {{use.title}}
    Body: {{use.body}}
    Footer: {{use.footer}}
  flow: [get_title, get_body, get_footer]  # All three must complete
```

## Environment Variables

Use `$VAR` syntax for environment variables:

```yaml
env:
  MODEL: claude-3-opus
  LOCALE: fr-FR

tasks:
  - id: generate
    infer:
      prompt: "Generate in {{use.entity.locale}}"
      model: $MODEL  # Uses env var
```

## Error Codes

| Code | Error | Fix |
|------|-------|-----|
| NIKA-040 | Binding not found | Add `output.use.ctx:` to source task |
| NIKA-041 | Invalid template | Check `{{}}` syntax |
| NIKA-042 | Missing flow dependency | Add `flow: [source_task]` |
| NIKA-043 | Field not found | Check JSON path in `{{use.alias.field}}` |

## Debugging Bindings

```bash
# See all bindings in trace
cargo run -- trace show <id> | grep "use.ctx"

# Debug template resolution
RUST_LOG=nika::binding=debug cargo run -- run workflow.yaml
```

## Best Practices

1. **Descriptive aliases:** Use `entity_context` not `ctx1`
2. **Explicit flows:** Always declare dependencies
3. **Validate first:** `cargo run -- validate` catches binding errors
4. **Use TUI:** Visual binding inspection

## Related Skills

- `/nika-run` — Run workflows
- `/nika-diagnose` — Debug issues
- `/workflow-validate` — Validation details
