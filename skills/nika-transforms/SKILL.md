---
name: nika-transforms
description: >-
  Complete reference for all 65 Nika pipe transforms used in {{...}} templates.
  Covers null safety (19 transforms fail on null — always use default()),
  confusing transforms explained (shell vs exec, base64_decode vs nika:decode,
  jq() power), and transform-vs-builtin-tool decision guide. Use when writing
  template expressions, debugging NIKA-072 null errors, or choosing between
  pipe transforms and builtin tools (schema nika/workflow@0.12).
globs:
  - "**/*.nika.yaml"
---

# Nika Pipe Transforms Reference

Transforms are applied with `|` in template expressions: `{{with.data | upper | trim}}`

## ⚠️ Null Safety (Critical)

**19 transforms fail on null input with NIKA-072.** Always guard with `default()`:

```yaml
# ❌ NIKA-072 if result is null
infer: "Process: {{with.result | upper}}"

# ✅ Safe
infer: "Process: {{with.result | default('none') | upper}}"
```

## All 65 Transforms

### String (9)

| Transform | Description |
|-----------|-------------|
| `upper` | Uppercase |
| `lower` | Lowercase |
| `trim` | Remove leading/trailing whitespace |
| `trim_start` | Remove leading whitespace only |
| `trim_end` | Remove trailing whitespace only |
| `length` | String length (number) |
| `to_string` | Convert any value to string |
| `replace(a, b)` | Replace all occurrences of `a` with `b` |
| `truncate(N)` | Keep first N characters |

### Array (9)

| Transform | Description |
|-----------|-------------|
| `first` | First element |
| `last` | Last element |
| `flatten` | Flatten nested arrays one level |
| `reverse` | Reverse array |
| `sort` | Sort ascending |
| `unique` | Remove duplicates |
| `compact` | Remove null values from array |
| `keys` | Array of object keys |
| `values` | Array of object values |

### Aggregation (7)

| Transform | Description |
|-----------|-------------|
| `add` | Sum numbers OR concatenate strings/arrays |
| `sum` | Numeric sum only |
| `avg` | Average |
| `min` | Minimum value |
| `max` | Maximum value |
| `min_by(field)` | Object with minimum field value |
| `max_by(field)` | Object with maximum field value |

### Numeric (5)

| Transform | Description |
|-----------|-------------|
| `to_number` | Parse string to number |
| `round` | Round to nearest integer |
| `abs` | Absolute value |
| `ceil` | Round up |
| `floor` | Round down |

### Type Conversion (5)

| Transform | Description |
|-----------|-------------|
| `to_bool` | Convert to boolean |
| `to_json` | Serialize to JSON string |
| `parse_json` | Deserialize JSON string |
| `parse_yaml` | Deserialize YAML string |
| `type_of` | Returns type name as string |

### Logic (1)

| Transform | Description |
|-----------|-------------|
| `not` | Boolean negation |

### Introspection (1)

| Transform | Description |
|-----------|-------------|
| `has(key)` | True if object has key |

### Parametric (4)

| Transform | Description |
|-----------|-------------|
| `join(", ")` | Join array elements with separator |
| `split(",")` | Split string into array |
| `default("fallback")` | Return fallback if null/empty |
| `slice(start, end)` | Array or string slice |

### Query (8)

| Transform | Description |
|-----------|-------------|
| `pluck(field)` | Extract field from each object in array |
| `where(field, val)` | Filter array: keep where field == val |
| `pick(f1, f2)` | Keep only listed fields from object |
| `omit(f1, f2)` | Remove listed fields from object |
| `sort_by(field)` | Sort array of objects by field |
| `group_by(field)` | Group array into object keyed by field |
| `merge` | Deep merge two objects |
| `regex(pattern)` | Extract regex matches |

### String Test (3)

| Transform | Description |
|-----------|-------------|
| `starts_with(str)` | Boolean: starts with string |
| `ends_with(str)` | Boolean: ends with string |
| `contains(str)` | Boolean: contains string |

### URL (4)

| Transform | Description |
|-----------|-------------|
| `url_host` | Extract hostname |
| `url_path` | Extract path |
| `url_without_query` | URL without query string |
| `url_normalize` | Normalize URL |

### Encoding (4)

| Transform | Description |
|-----------|-------------|
| `base64_encode` | Encode to base64 string |
| `base64_decode` | Decode base64 → STRING (not CAS blob!) |
| `content_hash` | Blake3 hash of content |
| `unique_urls` | Deduplicate URL list |

### JQ (1)

| Transform | Description |
|-----------|-------------|
| `jq(expr)` | Full jq stdlib via jaq-core (100+ functions) |

### System (1)

| Transform | Description |
|-----------|-------------|
| `shell` | Escape value for safe shell interpolation (NOT execution) |

## Confusing Transforms Explained

### `shell` — Escaping, NOT Execution

```yaml
# shell escapes a VALUE for safe use in shell commands
# It does NOT execute anything

# ❌ NIKA-053 — unescaped binding in shell: true
exec:
  command: "echo {{with.message}}"
  shell: true

# ✅ | shell wraps in single quotes with proper escaping
exec:
  command: "echo {{with.message | shell}}"
  shell: true
```

### `base64_decode` → String, NOT CAS blob

```yaml
# base64_decode produces a STRING — cannot go to media pipeline
# For binary data from APIs (Gemini, fal.ai, Stability AI):

# ❌ WRONG — produces string, not CAS blob
- id: use_image
  with:
    data: $api_response.image_b64
  fetch:
    # Can't use base64_decode result with nika:thumbnail

# ✅ CORRECT — nika:decode stores in CAS, returns hash
- id: decode_image
  invoke:
    tool: nika:decode
    params:
      data: "{{with.b64}}"
      mime_type: "image/png"
# Returns: { hash: "blake3:...", mime_type, size_bytes }
```

### `jq(expr)` — Full jq Power

```yaml
# Access the entire jaq-core stdlib (100+ functions)
with:
  # Complex transformations
  filtered: "$data | jq('.users[] | select(.active) | .name')"
  nested: "$data | jq('{names: [.items[].title], count: (.items | length)}')"
  # Group and reshape
  grouped: "$data | jq('group_by(.category) | map({key: .[0].category, value: map(.id)}) | from_entries')"
```

### `add` vs `sum`

```yaml
# add: polymorphic — numbers, strings, or arrays
with:
  total_num: "$numbers | add"       # sum of numbers
  combined_str: "$strings | add"    # concatenate strings
  merged_arr: "$arrays | add"       # flatten one level

# sum: numeric ONLY
with:
  total: "$prices | sum"  # must be array of numbers
```

## Transform vs Builtin Tool — Decision Guide

| Need | Use Transform | Use Builtin Tool |
|------|---------------|-----------------|
| Simple field access | `$task.field` | — |
| String ops | `upper`, `trim`, `replace` | — |
| Filter array | `where(field, val)` | `nika:filter` (complex conditions) |
| Map array | `pluck(field)` | `nika:map` (computed fields) |
| Group data | `group_by(field)` | `nika:group_by` (complex grouping) |
| Deep merge | — | `nika:json_merge` |
| jq expression | `jq(expr)` | `nika:jq` (complex / reusable) |
| Base64 to string | `base64_decode` | — |
| Base64 to CAS blob | — | `nika:decode` (ALWAYS) |

## Chaining Example

```yaml
- id: process
  with:
    data: $fetch_result.items
  infer: |
    Process these items: {{with.data | where('active', true) | sort_by('priority') | pluck('name') | join(', ')}}
```

## Transforms That Do NOT Exist

```yaml
# ❌ These do not exist — common wrong guesses
| pad(N, char)      # → use jq('@sh') or infer
| enumerate         # → use nika:map with idx param
| zip_with          # → use nika:zip tool
| map(expr)         # → use nika:map tool
```

## Related Skills

- `/nika-workflow-syntax` — data binding syntax (`with:`, `$task_id`, `??` fallback)
- `/nika-exec` — the `| shell` transform in shell commands
- `/nika-for-each` — transforms on for_each array outputs
- `/nika-invoke` — when to use builtin tools vs transforms
