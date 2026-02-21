# ADR-005: Runtime DAG Expansion with decompose

**Status:** Accepted
**Date:** 2026-02-21
**Context:** Nika v0.5 (MVP 8)

## Decision

Tasks can expand into multiple parallel tasks at runtime via the **`decompose:` modifier**.

```yaml
tasks:
  - id: generate_all_entities
    decompose:
      strategy: semantic    # semantic | static | nested
      traverse: HAS_CHILD   # Arc type to follow
      source: $entity       # Starting node
      max_items: 10         # Optional limit
    infer: "Generate content for {{use.item}}"
```

## Context

MVP 8 required dynamic parallelism where:
- The number of iterations isn't known until runtime
- Items come from graph traversal (NovaNet)
- Each item runs the same task template

## Rationale

### Why not just use for_each?

`for_each:` requires a static array or binding at workflow parse time:

```yaml
# Static - works but inflexible
for_each: ["fr-FR", "en-US", "de-DE"]

# Binding - requires upstream task to produce array
for_each: "{{use.locales}}"
```

`decompose:` handles the case where items are discovered via MCP:

```yaml
# Dynamic - discovers items via graph traversal
decompose:
  traverse: HAS_ENTITY
  source: "project:qrcode-ai"
```

### Strategies

| Strategy | Source | Use Case |
|----------|--------|----------|
| `semantic` | MCP traversal | Follow graph arcs |
| `static` | Inline array | Known items |
| `nested` | Recursive expansion | Tree structures |

### Implementation

1. **Parse phase:** `decompose:` modifier stored on Task
2. **Execution phase:** Executor calls MCP to discover items
3. **Expansion phase:** Task clones into N parallel tasks
4. **Collection phase:** Results aggregated in original order

```rust
// In executor.rs
if let Some(decompose) = &task.decompose {
    let items = self.resolve_decompose_items(decompose).await?;
    for item in items {
        let child_task = task.clone_with_item(item);
        spawned_tasks.push(tokio::spawn(self.execute(child_task)));
    }
}
```

### decompose vs spawn_agent

| Feature | decompose | spawn_agent |
|---------|-----------|-------------|
| Control | Declarative YAML | LLM decides |
| Timing | Expansion before run | On-demand during run |
| Cost | Predictable | Variable |
| Use case | Known pattern | Dynamic orchestration |

## Consequences

### Positive
- Graph-driven parallelism without hardcoded lists
- Clean YAML syntax
- Respects DAG execution model
- Results collected automatically

### Negative
- MCP call adds latency before task starts
- `max_items` needed to prevent explosion
- More complex DAG visualization

## Compliance

The `decompose:` modifier MUST NOT change task semantics:
- Each expanded task runs the same verb
- Results are arrays, not single values
- Parent task ID is preserved with suffix

## Related

- ADR-004: spawn_agent (runtime agent nesting)
- ADR-001: 5 Semantic Verbs (decompose is a modifier, not a verb)
