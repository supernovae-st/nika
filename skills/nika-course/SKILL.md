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

## Course Structure (12 Levels — Liberation Theme)

### Level 1: Jailbreak (5 exercises)
Break free from manual commands. Learn `exec:` and basic workflows.

### Level 2: Hot Wire (4 exercises)
Hot-wire the network. Master `fetch:` for HTTP requests and APIs.

### Level 3: Fork Bomb (4 exercises)
Multiply your power. DAG patterns, `depends_on`, and parallel execution.

### Level 4: Root Access (3 exercises)
Unlock the LLM. First `infer:` prompts with provider setup.

### Level 5: Shapeshifter (3 exercises)
Transform data with `with:` bindings and pipe transforms.

### Level 6: Pay-Per-Dream (3 exercises)
Structured output, JSON schemas, and output validation.

### Level 7: Swiss Knife (3 exercises)
Builtin tools via `invoke:` — nika:log, nika:emit, nika:assert.

### Level 8: Gone Rogue (3 exercises)
Autonomous agents with `agent:`, tools, and stop conditions.

### Level 9: Data Heist (4 exercises)
Advanced `fetch:` extraction — markdown, article, metadata, links.

### Level 10: Open Protocol (3 exercises)
MCP integration — `invoke:` external tools and NovaNet.

### Level 11: Pixel Pirate (4 exercises)
Media pipeline — import, thumbnail, vision, CAS workflows.

### Level 12: SuperNovae (5 exercises) — BOSS
Final boss. Orchestrate everything — full production workflows.

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
| 1 (Jailbreak) | Missing `schema:` line | Always start with `schema: nika/workflow@0.12` |
| 2 (Hot Wire) | Wrong HTTP method for fetch | Check `method:` field (default GET) |
| 3 (Fork Bomb) | Circular `depends_on:` | DAG must be acyclic — no circular refs |
| 4 (Root Access) | Missing provider API key | Set env var or `nika keys set` |
| 5 (Shapeshifter) | `with: { x: task }` without `$` | Use `$task_id` with dollar prefix |
| 6 (Pay-Per-Dream) | Missing `required:` in schema | Always list required properties |
| 7 (Swiss Knife) | Wrong tool name | Use `nika:` prefix for builtins |
| 8 (Gone Rogue) | Agent loops forever | Set `max_turns`, add stop conditions |

## Progress Tracking

The constellation map (`nika course status`) shows:
- Completed exercises (filled stars)
- Current exercise (blinking)
- Locked levels (dimmed)

Each level unlocks after completing all exercises in the previous level.
