# Nika v0.2 Workflow Patterns

Reference guide for building effective workflows with Nika's DAG execution engine.

## Schema Version

```yaml
schema: "nika/workflow@0.2"
```

v0.2 adds `invoke:` and `agent:` verbs for MCP integration.

---

## The 5 Verbs

| Verb | Purpose | Use When |
|------|---------|----------|
| `exec:` | Shell command | CLI tools, file ops, deterministic tasks |
| `fetch:` | HTTP request | APIs, webhooks, external data |
| `infer:` | LLM completion | Content generation, analysis, synthesis |
| `invoke:` | MCP tool call | Single knowledge graph operation |
| `agent:` | Autonomous agent | Multi-step exploration, complex research |

---

## Pattern 1: Fan-Out / Fan-In

Parallel execution with result aggregation.

```
        ┌─── task_a ───┐
input ──┼─── task_b ───┼── aggregate
        └─── task_c ───┘
```

```yaml
tasks:
  - id: input
    exec:
      command: "echo entity-key"

  - id: task_a
    use:
      key: input
    invoke:
      server: novanet
      tool: novanet_describe
      params:
        describe: entity
        entity_key: "{{use.key}}"

  - id: task_b
    use:
      key: input
    invoke:
      server: novanet
      tool: novanet_traverse
      params:
        start_key: "{{use.key}}"
        arc_families: ["semantic"]

  - id: task_c
    use:
      key: input
    invoke:
      server: novanet
      tool: novanet_atoms
      params:
        locale: "fr-FR"
        atom_type: "term"

  - id: aggregate
    use:
      a: task_a
      b: task_b
      c: task_c
    infer:
      prompt: |
        Synthesize findings:
        Description: {{use.a}}
        Relations: {{use.b}}
        Terms: {{use.c}}

flows:
  - source: input
    target: [task_a, task_b, task_c]
  - source: [task_a, task_b, task_c]
    target: aggregate
```

**Use cases:** UC1 (multi-locale generation), UC3 (entity knowledge retrieval), UC5 (content planning)

---

## Pattern 2: Pipeline / Sequential

Linear execution with data transformation.

```
step_1 ── step_2 ── step_3 ── step_4
```

```yaml
tasks:
  - id: discover
    invoke:
      server: novanet
      tool: novanet_search
      params:
        query: "QR code"
        kinds: ["Entity"]

  - id: analyze
    use:
      entities: discover
    infer:
      prompt: |
        Analyze entities: {{use.entities}}
        Output: prioritized list

  - id: generate
    use:
      priorities: analyze
    infer:
      prompt: |
        Generate content plan for: {{use.priorities}}

flows:
  - source: discover
    target: analyze
  - source: analyze
    target: generate
```

**Use cases:** UC2 (SEO content sprint), UC7 (quality gate pipeline)

---

## Pattern 3: Agent-Driven Exploration

Autonomous agent with tool access for complex research.

```yaml
tasks:
  - id: research
    agent:
      prompt: |
        Research entity "dynamic-qr-code" using available tools:
        - novanet_describe: Get entity details
        - novanet_traverse: Explore relationships
        - novanet_atoms: Get locale knowledge

        Build comprehensive understanding, then output:
        "RESEARCH_COMPLETE"
      mcp:
        - novanet
      max_turns: 15
      stop_conditions:
        - "RESEARCH_COMPLETE"
```

**Agent configuration:**

| Property | Required | Description |
|----------|----------|-------------|
| `prompt` | Yes | Agent instructions with goal |
| `mcp` | Yes | List of MCP servers to use |
| `max_turns` | No | Max iterations (default: 10) |
| `stop_conditions` | No | Strings that signal completion |
| `model` | No | Override model (default: workflow provider) |

**Use cases:** UC6 (multi-agent research), UC9 (full page pipeline)

---

## Pattern 4: Quality Gate

Sequential with validation checkpoints.

```
generate ── validate ─┬─ [pass] ── publish
                      └─ [fail] ── revise
```

```yaml
tasks:
  - id: generate
    infer:
      prompt: "Generate hero block content..."

  - id: validate
    use:
      content: generate
      locale_taboos: locale_context
    infer:
      prompt: |
        Validate content against locale rules:
        Content: {{use.content}}
        Taboos to avoid: {{use.locale_taboos}}

        Output JSON: { "valid": true/false, "issues": [...] }

  - id: fix
    use:
      original: generate
      validation: validate
    infer:
      prompt: |
        Fix content based on validation feedback:
        Original: {{use.original}}
        Issues: {{use.validation}}
```

**Use cases:** UC4 (locale-aware block generation), UC7 (quality pipeline)

---

## Pattern 5: Cross-Locale Orchestration

Parallel generation across multiple locales.

```yaml
tasks:
  - id: entity_context
    invoke:
      server: novanet
      tool: novanet_describe
      params:
        describe: entity
        entity_key: "qr-code-generator"

  # Parallel locale generation
  - id: gen_fr
    use:
      entity: entity_context
    invoke:
      server: novanet
      tool: novanet_generate
      params:
        focus_key: homepage
        locale: "fr-FR"
        mode: page

  - id: gen_de
    use:
      entity: entity_context
    invoke:
      server: novanet
      tool: novanet_generate
      params:
        focus_key: homepage
        locale: "de-DE"
        mode: page

  # ... more locales

  - id: report
    use:
      fr: gen_fr
      de: gen_de
    infer:
      prompt: |
        Summarize generation results across locales:
        French: {{use.fr}}
        German: {{use.de}}

flows:
  - source: entity_context
    target: [gen_fr, gen_de]
  - source: [gen_fr, gen_de]
    target: report
```

**Use cases:** UC1 (multi-locale pages), UC5 (coverage analysis), UC8 (cross-locale orchestration)

---

## Pattern 6: Competitive Intelligence

Multi-source data gathering with synthesis.

```yaml
tasks:
  - id: our_entity
    invoke:
      server: novanet
      tool: novanet_describe
      params:
        describe: entity
        entity_key: "{{use.entity}}"

  - id: similar_entities
    use:
      key: input
    invoke:
      server: novanet
      tool: novanet_traverse
      params:
        start_key: "{{use.key}}"
        arc_families: ["semantic"]
        target_kinds: ["Entity"]

  - id: industry_analysis
    use:
      key: input
    invoke:
      server: novanet
      tool: novanet_traverse
      params:
        start_key: "{{use.key}}"
        direction: outgoing
        arc_families: ["semantic"]

  - id: competitive_report
    use:
      ours: our_entity
      competitors: similar_entities
      industries: industry_analysis
    infer:
      prompt: |
        Generate competitive intelligence report:
        Our Entity: {{use.ours}}
        Similar: {{use.competitors}}
        Industries: {{use.industries}}
```

**Use cases:** UC10 (competitive intelligence)

---

## MCP Configuration

Configure MCP servers in the workflow:

```yaml
mcp:
  novanet:
    command: /path/to/novanet-mcp
    args: []
    env:
      NOVANET_MCP_NEO4J_URI: bolt://localhost:7687
      NOVANET_MCP_NEO4J_USER: neo4j
      NOVANET_MCP_NEO4J_PASSWORD: novanetpassword
```

**NovaNet MCP Tools:**

| Tool | Purpose |
|------|---------|
| `novanet_describe` | Schema/entity information |
| `novanet_search` | Fulltext/property search |
| `novanet_traverse` | Graph traversal |
| `novanet_assemble` | Context assembly |
| `novanet_atoms` | Knowledge atoms (terms, expressions) |
| `novanet_generate` | Full generation context |

---

## Data Binding

Use `use:` block to pass data between tasks:

```yaml
tasks:
  - id: producer
    exec:
      command: "echo hello"

  - id: consumer
    use:
      message: producer
    infer:
      prompt: "Process: {{use.message}}"
```

**Binding resolution:**
- `{{use.alias}}` - Full output of referenced task
- Works in: `infer.prompt`, `invoke.params`, `agent.prompt`

---

## Flow Declaration

Explicit dependency declaration:

```yaml
flows:
  # Single source to single target
  - source: task_a
    target: task_b

  # Single source to multiple targets (fan-out)
  - source: input
    target: [task_a, task_b, task_c]

  # Multiple sources to single target (fan-in)
  - source: [task_a, task_b]
    target: aggregate
```

---

## Best Practices

1. **Keep tasks focused** - One responsibility per task
2. **Use invoke: for single operations** - Direct tool calls
3. **Use agent: for exploration** - When multiple tool calls needed
4. **Fan-out for parallelism** - Maximize concurrent execution
5. **Add quality gates** - Validation before final output
6. **Limit agent turns** - Set `max_turns` to prevent runaway

---

## Example Workflows

| File | Pattern | Tasks |
|------|---------|-------|
| `uc1-generate-page-multilingual.nika.yaml` | Fan-out/in | 7 |
| `uc2-seo-content-sprint.nika.yaml` | Pipeline | 6 |
| `uc3-entity-knowledge-retrieval.nika.yaml` | Fan-out/in | 7 |
| `uc4-block-generation-locale-aware.nika.yaml` | Quality gate | 8 |
| `uc5-semantic-content-planning.nika.yaml` | Cross-locale | 9 |
| `uc6-multi-agent-research.nika.yaml` | Agent-driven | 4 |
| `uc7-quality-gate-pipeline.nika.yaml` | Quality gate | 9 |
| `uc8-cross-locale-orchestration.nika.yaml` | Cross-locale | 9 |
| `uc9-full-page-pipeline.nika.yaml` | Agent + pipeline | 11 |
| `uc10-competitive-intelligence.nika.yaml` | Intelligence | 10 |
