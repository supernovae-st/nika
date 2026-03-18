# Nika Expert Workflows

Advanced workflow examples showcasing all Nika v0.19 features combined.

## Overview

These workflows demonstrate production-grade patterns combining:

- **Artifacts (v0.18)**: Atomic writes, templates, multiple outputs, manifest generation
- **Structured Outputs (v0.19)**: JSON Schema validation, retry loops, error feedback
- **Agent Verb**: MCP tools, spawn_agent, depth_limit, extended_thinking
- **For_each**: Parallel execution with per-iteration artifacts
- **Complex DAGs**: Diamond patterns, conditional execution, error recovery

## Workflows

### 1. Multilingual Content Pipeline

**File**: `multilingual-content-pipeline.nika.yaml`

Generates SEO-optimized landing pages across multiple locales.

```
Features:
├── Workflow-level artifact defaults with templates
├── JSON Schema validation at every step
├── for_each parallel generation (4 locales)
├── Multiple artifacts per task (JSON + YAML)
├── Quality assessment with approval gates
└── Comprehensive manifest generation
```

**DAG Pattern**: Linear with parallel branch

```
load_entity ──┬──> generate_locales ──> quality ──> report ──> output
              │        (for_each)
define_locales ────────────────────────────────────────────────────────┘
```

---

### 2. Autonomous Research Agent

**File**: `autonomous-research-agent.nika.yaml`

Multi-agent research system with deep reasoning capabilities.

```
Features:
├── Agent verb with max_turns and depth_limit
├── spawn_agent for delegating to sub-agents
├── Extended thinking (8k-16k token budget)
├── MCP integration (Perplexity for web search)
├── Structured outputs with retry loops
└── Multi-format artifact output (JSON + Markdown)
```

**DAG Pattern**: Agent hierarchy with synthesis

```
research_planner ──> run_investigations ──> synthesize ──> report
(extended_thinking)     (for_each)        (16k thinking)
                          │
                    ┌─────┴─────┐
                    │spawn_agent│
                    └───────────┘
```

---

### 3. Data Processing Pipeline

**File**: `data-processing-pipeline.nika.yaml`

Production-grade ETL pipeline with comprehensive auditing.

```
Features:
├── Complex DAG with diamond dependencies
├── Parallel data acquisition from 3 sources
├── Per-source validation with quality scores
├── Diamond convergence for data merge
├── Batch processing with concurrency control
├── Multiple artifact modes (overwrite, append, unique)
└── Comprehensive audit trail
```

**DAG Pattern**: Diamond with batch processing

```
┌────────────┐  ┌────────────┐  ┌────────────┐
│ fetch_a    │  │ fetch_b    │  │ fetch_c    │  (PARALLEL)
└─────┬──────┘  └─────┬──────┘  └─────┬──────┘
      │               │               │
      ▼               ▼               ▼
┌────────────┐  ┌────────────┐  ┌────────────┐
│ validate_a │  │ validate_b │  │ validate_c │  (PARALLEL)
└─────┬──────┘  └─────┬──────┘  └─────┬──────┘
      └───────────────┼───────────────┘
                      ▼
              ┌───────────────┐
              │ merge_data    │  (CONVERGENCE)
              └───────┬───────┘
                      ▼
              ┌───────────────┐
              │ process_batch │  (FOR_EACH)
              └───────┬───────┘
                      ▼
              ┌───────────────┐
              │ aggregate     │
              └───────────────┘
```

---

### 4. Knowledge Graph Content Generation

**File**: `knowledge-graph-content-gen.nika.yaml`

Native content generation from NovaNet knowledge graph.

```
Features:
├── Full NovaNet MCP tool suite (7 tools)
├── Context assembly with token budget
├── ADR-033 denomination forms (text/title/abbrev/url)
├── Culturally-native generation (NOT translation)
├── Extended thinking for generation
├── Per-locale artifacts (JSON + YAML)
└── Quality validation with approval gates
```

**MCP Tools Used**:
- `novanet_introspect` - Schema discovery
- `novanet_describe` - Entity details
- `novanet_search` - Relationship mapping (mode: walk)
- `novanet_context` - Knowledge retrieval (mode: knowledge)
- `novanet_context` - Context assembly (mode: block, ADR-033)

**DAG Pattern**: Knowledge graph driven

```
introspect ──> describe ──> search(walk) ──> context(knowledge) ──> context(block) ──> generate ──> validate ──> report
                              │           │          │
                              └───────────┴──────────┘
                                     (for_each: locales)
```

---

## Running Expert Workflows

```bash
# 1. Multilingual Content Pipeline
nika examples/expert/multilingual-content-pipeline.nika.yaml

# 2. Autonomous Research Agent (with custom topic)
nika examples/expert/autonomous-research-agent.nika.yaml -- topic="AI regulation 2026"

# 3. Data Processing Pipeline
nika examples/expert/data-processing-pipeline.nika.yaml

# 4. Knowledge Graph Content (requires NovaNet MCP)
# First, start NovaNet MCP server:
cd ../novanet/tools/novanet-mcp && cargo run &
# Then run:
nika examples/expert/knowledge-graph-content-gen.nika.yaml
```

## Requirements

| Workflow | ANTHROPIC_API_KEY | PERPLEXITY_API_KEY | NovaNet MCP | Neo4j |
|----------|-------------------|--------------------| ------------|-------|
| Multilingual Content | ✅ Required | ❌ Optional | ❌ No | ❌ No |
| Autonomous Research | ✅ Required | ✅ Optional | ❌ No | ❌ No |
| Data Processing | ✅ Required | ❌ No | ❌ No | ❌ No |
| Knowledge Graph | ✅ Required | ❌ No | ✅ Required | ✅ Required |

## Key Patterns Demonstrated

### 1. Artifact Templates

```yaml
artifacts:
  dir: ./output/{{date}}/{{workflow_name}}/{{uuid}}
  format: json
  manifest: true

tasks:
  - id: task1
    artifact:
      - path: data/{{task_id}}.json
      - path: logs/{{task_id}}-{{timestamp}}.log
        mode: append
```

### 2. JSON Schema Validation with Retry

```yaml
output:
  format: json
  schema:
    type: object
    required: [field1, field2]
    properties:
      field1:
        type: string
        maxLength: 100
  max_retries: 3  # Auto-retry on validation failure
```

### 3. Agent with spawn_agent

```yaml
agent:
  prompt: |
    You have spawn_agent available for delegation...
  mcp: [novanet, perplexity]
  max_turns: 10
  depth_limit: 2  # Spawned agents can spawn once more
  extended_thinking: true
  thinking_budget: 16000
```

### 4. for_each with Artifacts

```yaml
- id: process_items
  for_each: "$items"
  as: item
  concurrency: 4
  fail_fast: false

  artifact:
    - path: results/{{with.item.id}}.json
    - path: audit.log
      mode: append  # All iterations append to same log
```

### 5. Diamond DAG Pattern

```yaml
tasks:
  - id: fetch_a
  - id: fetch_b
  - id: fetch_c

  - id: validate_a
    depends_on: [fetch_a]
  - id: validate_b
    depends_on: [fetch_b]
  - id: validate_c
    depends_on: [fetch_c]

  - id: merge_all
    depends_on: [validate_a, validate_b, validate_c]  # Diamond convergence
```

---

## Version Compatibility

These workflows require:
- **Nika v0.19.0+** (Structured Output Enforcement)
- **Schema**: `nika/workflow@0.10`

---

*Expert workflows created for Nika v0.19.1*
*SuperNovae Studio © 2026*
