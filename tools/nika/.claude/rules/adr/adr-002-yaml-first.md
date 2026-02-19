# ADR-002: YAML-First Workflow Definition

**Status:** Accepted
**Date:** 2026-02-18
**Context:** Nika v0.1

## Decision

Nika workflows are defined in **YAML files** as the single source of truth.

```yaml
schema: nika/workflow@0.2
workflow: my-workflow
description: "What this workflow does"

tasks:
  - id: step1
    infer: "Generate something"
```

## Context

We considered multiple workflow definition formats:
- JSON
- TOML
- Custom DSL
- Programmatic API (Rust/Python)

## Rationale

### Why YAML?

| Factor | YAML | JSON | TOML | DSL |
|--------|------|------|------|-----|
| Human readable | ✅ | ⚠️ | ✅ | ✅ |
| Comments | ✅ | ❌ | ✅ | ✅ |
| Multi-line strings | ✅ | ❌ | ⚠️ | ✅ |
| Industry standard | ✅ | ✅ | ⚠️ | ❌ |
| IDE support | ✅ | ✅ | ⚠️ | ❌ |
| Schema validation | ✅ | ✅ | ⚠️ | ❌ |

YAML wins on:
- **Multi-line prompts:** LLM prompts are often long
- **Comments:** Document workflow intent inline
- **Familiarity:** DevOps teams know YAML (K8s, GitHub Actions)

### Why not a programmatic API?

```rust
// Rejected approach
let workflow = Workflow::new("my-workflow")
    .task("step1", Infer::new("Generate something"))
    .task("step2", Exec::new("echo done").depends_on("step1"))
    .build();
```

Problems:
- Requires Rust knowledge
- Harder to version control diffs
- No declarative validation
- Can't share across languages

### YAML enables

1. **Static analysis:** Validate before execution
2. **DAG visualization:** Extract graph from file
3. **Portability:** Any language can parse YAML
4. **Git-friendly:** Clean diffs, easy review

## Consequences

### Positive
- Workflows are versionable artifacts
- Non-programmers can edit workflows
- Schema validation catches errors early
- IDE extensions provide completion

### Negative
- Complex logic requires multiple tasks
- No conditionals (use DAG branches instead)
- Template syntax (`{{use.alias}}`) is string-based

## Schema Evolution

```yaml
# v0.1 (original)
schema: nika/workflow@0.1

# v0.2 (current - added invoke: and agent:)
schema: nika/workflow@0.2
```

Schema version is required and validated.

## Related

- ADR-001: 5 Semantic Verbs
- ADR-003: MCP-Only Integration
