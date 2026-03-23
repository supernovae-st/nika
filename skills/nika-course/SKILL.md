---
name: nika-course
description: >-
  Guide through the Nika interactive learning course (12 levels, 44 exercises).
  Track progress with constellation map, provide hints, validate exercises, and
  explain concepts. Use when users want to learn Nika, follow the course, check
  progress, get hints, or complete exercises in the Nika YAML workflow engine
  (schema nika/workflow@0.12).
---

# Nika Learning Course Guide

Interactive 12-level course with 44 exercises teaching Nika from basics to mastery.

## Course Commands

```bash
nika init --course           # Generate the full 12-level course (44 exercises)
nika course status            # Show constellation progress map
nika course next              # Open the next incomplete exercise
nika course check             # Validate all exercises
nika course check 3           # Validate level 3 only
nika course hint 3.2          # Get progressive hints for exercise 3.2
nika course run 2.1           # Run a specific exercise
nika course info              # Show course overview
nika course info 5            # Show level 5 details
nika course reset 3           # Reset level 3 exercises
```

## Course Structure (12 Levels)

### Level 1: First Light -- Basics
- 1.1: Hello World (`exec:` verb)
- 1.2: First prompt (`infer:` verb)
- 1.3: First fetch (`fetch:` verb)
- 1.4: Task chaining (`depends_on:`)

### Level 2: Binary Stars -- Data Flow
- 2.1: With bindings (`with:` block)
- 2.2: Template expressions (`{{with.x}}`)
- 2.3: Environment variables (`{{$env.VAR}}`)
- 2.4: Fallback operator (`??`)

### Level 3: Nebula -- Inputs & Context
- 3.1: Workflow inputs (`inputs:` block)
- 3.2: Input defaults
- 3.3: Context files (`context:` block)
- 3.4: Combining inputs and context

### Level 4: Red Giant -- Parallel Execution
- 4.1: Parallel tasks (no depends_on)
- 4.2: for_each basics
- 4.3: for_each with objects
- 4.4: Fan-out / fan-in pattern

### Level 5: Supernova -- Artifacts
- 5.1: Basic artifact output
- 5.2: Artifact formats (text, json, markdown)
- 5.3: Artifact templates
- 5.4: Per-iteration artifacts

### Level 6: Pulsar -- Structured Output
- 6.1: Basic JSON schema
- 6.2: Nested schemas
- 6.3: Structured + binding downstream
- 6.4: Structured + retry

### Level 7: Quasar -- Fetch Mastery
- 7.1: HTTP methods and headers
- 7.2: Extract modes (markdown, article)
- 7.3: API integration patterns
- 7.4: Binary fetch for media

### Level 8: Magnetar -- Agent Loops
- 8.1: Basic agent with tools
- 8.2: Guardrails and safety
- 8.3: Agent + structured output
- 8.4: Multi-agent pipeline

### Level 9: White Dwarf -- MCP & Invoke
- 9.1: Builtin nika:* tools
- 9.2: MCP server configuration
- 9.3: Media pipeline basics

### Level 10: Neutron Star -- Advanced Patterns
- 10.1: Diamond DAG
- 10.2: Multi-provider workflows
- 10.3: Complex binding chains

### Level 11: Black Hole -- Production Patterns
- 11.1: Error handling and retry
- 11.2: Logging and tracing
- 11.3: Performance optimization

### Level 12: Liberation -- Mastery
- 12.1: Full production pipeline
- 12.2: Custom workflow design

## Helping Users

### When a user is stuck on an exercise:

1. Ask which exercise number they are on
2. Run `nika course hint <exercise>` for progressive hints (3 tiers)
3. Explain the concept without giving the full solution
4. If they are really stuck, show the key syntax pattern
5. Have them validate: `nika course check <level>`

### Concept Explanations

**Bindings**: Data flows between tasks via `with:` blocks. The `$` prefix references a task id. Templates `{{with.alias}}` inject the bound value.

**DAG**: Directed Acyclic Graph. Tasks without dependencies run in parallel. `depends_on:` creates explicit ordering. No circular deps allowed.

**for_each**: Expands one task into N parallel iterations. Each iteration gets the value via `as:` alias, accessed as `{{with.alias}}`.

**Structured output**: Forces LLM to return JSON matching a schema. The schema uses JSON Schema syntax within YAML.

**Artifacts**: Save task output to files. Configure with `artifact:` on a task and `artifacts: { dir: ./path }` at workflow level.

## Exercise Validation

When a user thinks they completed an exercise:

```bash
# Validate specific exercise
nika course check <level>

# Run the exercise to see output
nika course run <level>.<exercise>
```

Validation checks:
- YAML syntax is valid
- Schema version is correct
- Required fields are present
- Task structure matches exercise requirements
- `nika check` passes

## Common Student Mistakes

| Level | Common Mistake | Fix |
|-------|---------------|-----|
| 1 | Missing `schema:` line | Always start with `schema: nika/workflow@0.12` |
| 2 | `with: { x: task }` without `$` | Use `$task_id` with dollar prefix |
| 3 | `{{inputs.x}}` without `inputs:` block | Define `inputs:` at workflow level |
| 4 | `for_each:` without `as:` | Always pair them |
| 5 | Artifacts dir not set | Add `artifacts: { dir: ./output }` |
| 6 | Missing `required:` in schema | Always list required properties |
| 8 | No `nika_complete` in tools | Agent needs completion signal |

## Progress Tracking

The constellation map (`nika course status`) shows:
- Completed exercises (filled stars)
- Current exercise (blinking)
- Locked levels (dimmed)

Each level unlocks after completing all exercises in the previous level.
