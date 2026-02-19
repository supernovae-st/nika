# ADR-003: MCP-Only Integration with NovaNet

**Status:** Accepted
**Date:** 2026-02-18
**Context:** Nika v0.2

## Decision

Nika connects to NovaNet **exclusively via MCP protocol**. No direct Neo4j access.

```
┌──────────┐     MCP Protocol     ┌──────────────┐     Cypher     ┌────────┐
│   Nika   │ ──────────────────► │ NovaNet MCP  │ ─────────────► │ Neo4j  │
│ (Client) │                      │   (Server)   │                │   DB   │
└──────────┘                      └──────────────┘                └────────┘
```

## Context

Nika needs access to NovaNet's knowledge graph for:
- Entity context (`novanet_describe`)
- Graph traversal (`novanet_traverse`)
- Content generation (`novanet_generate`)
- Knowledge atoms (`novanet_atoms`)

Two approaches were considered:
1. **Direct Neo4j:** Nika includes neo4j driver, runs Cypher
2. **MCP proxy:** Nika calls MCP tools, NovaNet handles Neo4j

## Rationale

### Why MCP-Only?

| Factor | Direct Neo4j | MCP-Only |
|--------|--------------|----------|
| Coupling | Tight | Loose |
| Schema changes | Break Nika | Transparent |
| Security | Cypher injection risk | Validated tools |
| Caching | Manual | MCP server handles |
| Rate limiting | Manual | MCP server handles |
| Observability | Manual | MCP events |

### Zero Cypher Rule

Nika workflows MUST NOT contain raw Cypher:

```yaml
# WRONG - direct Cypher
- id: get_entity
  exec:
    command: "cypher-shell 'MATCH (e:Entity) RETURN e'"

# WRONG - embedded Cypher
- id: get_entity
  invoke:
    tool: novanet_query
    params:
      cypher: "MATCH (e:Entity {key: 'qr-code'}) RETURN e"

# RIGHT - semantic MCP tool
- id: get_entity
  invoke:
    tool: novanet_describe
    server: novanet
    params:
      entity: "qr-code"
```

### Benefits

1. **Schema independence:** NovaNet can rename nodes/arcs without breaking Nika
2. **Security:** MCP tools validate inputs, prevent injection
3. **Caching:** NovaNet MCP caches frequent queries
4. **Observability:** MCP calls emit structured events
5. **Testability:** Mock MCP client for tests

### Available MCP Tools (v0.2)

| Tool | Purpose |
|------|---------|
| `novanet_describe` | Get entity details |
| `novanet_search` | Search entities |
| `novanet_traverse` | Graph traversal |
| `novanet_assemble` | Build context |
| `novanet_atoms` | Knowledge atoms |
| `novanet_generate` | Content generation |
| `novanet_query` | Advanced queries |

## Consequences

### Positive
- Clean separation of concerns
- Nika stays simple (no Neo4j driver)
- NovaNet controls data access
- Easy to add new tools

### Negative
- Extra network hop (Nika → MCP → Neo4j)
- Must wait for NovaNet to add new tools
- MCP server must be running

## Compliance

Validation error if workflow contains:
- `neo4j://` connection strings
- `cypher:` parameter in any tool
- `bolt://` URLs

## Related

- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First
- NovaNet ADR-021: Query-First
