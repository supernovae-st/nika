# ADR-004: Nested Agents with spawn_agent

**Status:** Accepted
**Date:** 2026-02-21
**Context:** Nika v0.5 (MVP 8)

## Decision

Nika agents can spawn child agents via the **`spawn_agent` internal tool** with depth protection.

```yaml
tasks:
  - id: orchestrator
    agent:
      prompt: "Decompose this task and delegate to sub-agents"
      depth_limit: 3  # Max nesting depth (default: 3, max: 10)
```

## Context

MVP 8 required recursive agent spawning for complex workflows where:
- A parent agent orchestrates multiple sub-tasks
- Each sub-task benefits from its own agentic loop
- Unbounded recursion must be prevented

## Rationale

### Why an internal tool?

We considered several approaches:

| Approach | Pros | Cons |
|----------|------|------|
| Explicit verb `spawn:` | Clear syntax | Breaks 5-verb model |
| MCP tool | Standard protocol | Wrong abstraction |
| Internal tool | Discoverable by LLM | None significant |

Internal tool wins because:
- LLM can discover and use it naturally
- Implements `rig::ToolDyn` for seamless integration
- No YAML schema changes needed
- Depth can be controlled programmatically

### spawn_agent Parameters

```json
{
  "task_id": "subtask-1",      // Required: unique ID for child
  "prompt": "Generate header", // Required: child agent goal
  "context": {"entity": "qr"}, // Optional: context data
  "max_turns": 5               // Optional: max turns (default: 10)
}
```

### Depth Protection

Prevents infinite recursion:

```
depth_limit: 3

Agent (depth=0) → spawn_agent → Agent (depth=1)
                             → spawn_agent → Agent (depth=2)
                                          → spawn_agent → Agent (depth=3)
                                                       → spawn_agent → BLOCKED
```

When `current_depth >= depth_limit`, spawn_agent returns an error instructing the agent to complete the task directly.

### Implementation

```rust
// SpawnAgentTool in runtime/spawn.rs
impl ToolDyn for SpawnAgentTool {
    fn call(&self, params: Value) -> BoxFuture<'_, Result<Value, ToolError>> {
        // 1. Check depth limit
        if self.current_depth >= self.depth_limit {
            return error("Depth limit reached, complete task directly");
        }

        // 2. Create child RigAgentLoop
        // 3. Emit AgentSpawned event
        // 4. Run child with run_auto()
        // 5. Return child result
    }
}
```

## Consequences

### Positive
- Complex workflows decompose naturally
- LLM controls orchestration strategy
- Bounded recursion prevents runaway costs
- Full observability via AgentSpawned events

### Negative
- Nested agents increase latency
- Each spawn uses its own token budget
- Debugging multi-level agents is complex

## Events

New event variant:
```rust
EventKind::AgentSpawned {
    parent_task_id: Arc<str>,
    child_task_id: Arc<str>,
    depth: u32,
    prompt: String,
}
```

## Related

- ADR-001: 5 Semantic Verbs (spawn_agent is a tool, not a verb)
- ADR-005: Decompose Modifier (static DAG expansion)
