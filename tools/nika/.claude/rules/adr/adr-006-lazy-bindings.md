# ADR-006: Deferred Resolution with Lazy Bindings

**Status:** Accepted
**Date:** 2026-02-21
**Context:** Nika v0.5 (MVP 8)

## Decision

Bindings can defer resolution until first access via the **`lazy: true` flag**.

```yaml
use:
  # Eager (default) - resolved immediately at task start
  eager_val: task1.result

  # Lazy (v0.5) - resolved on first access
  lazy_val:
    path: future_task.result
    lazy: true
    default: "fallback"
```

## Context

MVP 8 required flexible binding resolution for:
- Bindings to tasks that may not have completed yet
- Optional data with fallbacks
- Conditional execution patterns

## Rationale

### Why lazy bindings?

**Problem:** In complex DAGs, strict dependency ordering can be limiting:

```yaml
tasks:
  - id: fast_task
    infer: "Quick inference"
    use:
      ctx: slow_task.result  # ERROR: slow_task hasn't run yet

  - id: slow_task
    exec: "long-running-process"
```

**Solution:** Lazy bindings defer resolution:

```yaml
  - id: fast_task
    infer: "Quick inference"
    use:
      ctx:
        path: slow_task.result
        lazy: true
        default: "{}"  # Use fallback if not available
```

### Binding Resolution Timing

| Mode | When Resolved | On Missing |
|------|---------------|------------|
| Eager | Task start | Error |
| Lazy | First access | Default or error |

### Implementation

```rust
// In binding/entry.rs
pub struct UseEntry {
    pub alias: String,
    pub path: BindingPath,
    pub lazy: bool,              // v0.5: deferred resolution
    pub default: Option<Value>,  // v0.5: fallback value
}

// In binding/resolve.rs
pub enum LazyBinding {
    Resolved(Value),
    Pending { path: BindingPath, default: Option<Value> },
}

impl LazyBinding {
    pub fn resolve(&mut self, store: &DataStore) -> Result<&Value, NikaError> {
        match self {
            LazyBinding::Resolved(v) => Ok(v),
            LazyBinding::Pending { path, default } => {
                match store.get(path) {
                    Some(v) => {
                        *self = LazyBinding::Resolved(v.clone());
                        self.resolve(store)
                    }
                    None => default.as_ref().ok_or_else(|| {
                        NikaError::BindingError {
                            alias: path.to_string(),
                            reason: "Lazy binding not resolved and no default".into(),
                        }
                    })
                }
            }
        }
    }
}
```

### YAML Syntax

Short form (eager, no default):
```yaml
use:
  ctx: task1.result
```

Long form (configurable):
```yaml
use:
  ctx:
    path: task1.result
    lazy: true
    default: null
```

## Consequences

### Positive
- Flexible dependency ordering
- Graceful degradation with defaults
- Enables speculative execution patterns
- No breaking changes to existing workflows

### Negative
- Runtime errors instead of parse-time for missing lazy bindings
- Default values can mask bugs
- More complex debugging

## Validation Rules

1. Lazy bindings MUST specify a `path`
2. Lazy bindings without `default` error on access if unresolved
3. Eager bindings (default) error at task start if missing

## Related

- ADR-004: spawn_agent (uses context bindings)
- ADR-005: decompose (uses item bindings)
