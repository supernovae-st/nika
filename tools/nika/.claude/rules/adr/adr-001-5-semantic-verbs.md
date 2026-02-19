# ADR-001: 5 Semantic Verbs

**Status:** Accepted
**Date:** 2026-02-18
**Context:** Nika v0.2

## Decision

Nika workflows use exactly **5 semantic verbs**:

| Verb | Purpose | Added |
|------|---------|-------|
| `infer:` | LLM text generation | v0.1 |
| `exec:` | Shell command execution | v0.1 |
| `fetch:` | HTTP request | v0.1 |
| `invoke:` | MCP tool call | v0.2 |
| `agent:` | Multi-turn agentic loop | v0.2 |

## Context

When designing Nika's workflow language, we needed to balance:
- **Simplicity:** Few verbs to learn
- **Completeness:** Cover all AI workflow patterns
- **Composability:** Verbs combine naturally

## Rationale

### Why not fewer verbs?

We considered merging `fetch:` into `exec:` (shell curl), but:
- `fetch:` has built-in retry, timeout, and JSON parsing
- Shell commands don't compose with bindings well
- HTTP is too common to make awkward

We considered merging `invoke:` into `infer:`, but:
- MCP tools are deterministic, LLM calls are not
- Different error handling (retry vs. no-retry)
- Clear separation of concerns

### Why not more verbs?

We rejected:
- `transform:` — Use `infer:` with a transformation prompt
- `validate:` — Use `exec:` with a validation script
- `branch:` — Use DAG `flow:` with conditional tasks
- `loop:` — Use `for_each:` (not a verb, a modifier)

Each rejected verb could be expressed with existing verbs.

### Why these specific 5?

| Verb | Irreducible Because |
|------|---------------------|
| `infer:` | Core AI capability, non-deterministic |
| `exec:` | System integration, deterministic |
| `fetch:` | Network I/O with HTTP semantics |
| `invoke:` | MCP protocol, tool-based AI |
| `agent:` | Multi-turn loops with tool use |

## Consequences

### Positive
- Easy to learn (5 keywords)
- Clear mental model
- Each verb has distinct error codes (NIKA-0XX ranges)

### Negative
- Some patterns require verb combinations
- No native "wait" or "delay" verb (use `exec: sleep`)

## Compliance

Every task MUST have exactly one verb:

```yaml
# VALID
- id: step1
  infer: "Generate text"

# INVALID - two verbs
- id: step1
  infer: "Generate text"
  exec: "echo done"

# INVALID - no verb
- id: step1
  output:
    use.ctx: result
```

## Related

- ADR-002: YAML-First
- ADR-003: MCP-Only Integration
