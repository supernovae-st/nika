## Data Flow

### Bindings (`with:` + `{{with.alias}}`)

```yaml
- id: use_data
  with:
    data: $upstream_task
    temp: $weather.data.temperature
    fallback: $task.path ?? "default"
    key: $env.API_KEY
  infer: "Temperature is {{with.temp}}, key: {{with.key}}"
```

### Dependencies

```yaml
- id: step2
  depends_on: [step1]       # Ordering without data binding
```

### Pipe transforms (64 available)

```yaml
{{with.data | upper | trim}}
{{with.items | flatten | unique | join(", ")}}
{{with.result | default("none") | upper}}
```

**String**: `upper`, `lower`, `trim`, `length`, `to_string`, `replace(a, b)`, `truncate(N)`
**Array**: `first`, `last`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
**Query**: `pluck(field)`, `where(field, val)`, `pick(f1, f2)`, `omit(f1, f2)`, `sort_by(field)`, `group_by(field)`, `merge`
**Type**: `to_json`, `parse_json`, `parse_yaml`, `to_number`, `to_bool`, `type_of`
**JQ**: `jq(expr)` — full jq stdlib (100+ functions)
**System**: `shell` — mandatory for template bindings in `shell: true` commands

### for_each (parallel loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
    fail_fast: false
  infer: "Process: {{with.item}}"
```

**CRITICAL**: `for_each` output is a JSON **array**. Use `{{with.results | first}}` or `{{with.results[0].field}}`.

### Workflow header fields

```yaml
schema: "nika/workflow@0.12"     # Required
workflow: my-workflow              # Optional (defaults to filename)
description: "What it does"       # Optional
provider: anthropic                # Default provider
model: claude-sonnet-4-20250514   # Default model

inputs:                            # Workflow parameters
  topic: "default value"
context:                           # File context bindings
  files:
    readme: ./README.md
skills:                            # Prompt augmentation files
  writing: ./skills/writing.md
artifacts:                         # Output persistence
  dir: ./output
  format: markdown
```
