# Research Report: Agent Termination and Stop Conditions

## Summary

This research analyzes how major agent frameworks handle loop termination and stop conditions. The findings reveal a consistent pattern across frameworks: **composable termination conditions** with multiple trigger types, all implemented as first-class abstractions rather than ad-hoc checks.

## Key Findings

### 1. MCP Sequential Thinking Pattern

**Source:** [modelcontextprotocol/servers - sequentialthinking](https://github.com/modelcontextprotocol/servers/blob/main/src/sequentialthinking/index.ts)

The MCP sequential thinking server uses **explicit completion signaling** via structured output:

```typescript
inputSchema: {
  nextThoughtNeeded: z.boolean().describe("Whether another thought step is needed"),
  thoughtNumber: z.number().int().min(1),
  totalThoughts: z.number().int().min(1),
  needsMoreThoughts: z.boolean().optional()
}
```

**Key insight:** The agent explicitly declares when it's done via `nextThoughtNeeded: false`. This is a **declarative termination** pattern where the LLM signals completion rather than the framework detecting it.

**Recommendation for Nika:**
- Add a `nika:complete` builtin tool that agents can call to signal completion
- The tool should accept a `result` parameter for the final output
- This provides explicit completion vs. implicit detection via stop_conditions

---

### 2. OpenAI Swarm Pattern

**Source:** [openai/swarm - core.py](https://github.com/openai/swarm/blob/main/swarm/core.py)

OpenAI Swarm uses a simple but effective pattern:

```python
while len(history) - init_len < max_turns and active_agent:
    # ... execute turn ...

    if not message.tool_calls or not execute_tools:
        debug_print(debug, "Ending turn.")
        break
```

**Termination triggers:**
1. **Max turns reached**: `max_turns: int = float("inf")`
2. **No tool calls**: Agent responds with text only (natural completion)
3. **Agent handoff**: `if result.agent: active_agent = result.agent` (transfers control)
4. **execute_tools=False**: Disables tool execution (single-shot mode)

**Key insight:** Natural completion occurs when the agent chooses not to call any tools. This is the most common pattern across frameworks.

**Recommendation for Nika:**
- Nika already has this via `RigAgentStatus::NaturalCompletion`
- Consider adding `execute_tools: false` equivalent for single-shot agent mode

---

### 3. Anthropic Computer Use Pattern

**Source:** [anthropic-quickstarts/computer-use-demo/loop.py](https://github.com/anthropics/anthropic-quickstarts/blob/main/computer-use-demo/computer_use_demo/loop.py)

Anthropic's reference implementation uses an **infinite loop with tool-based termination**:

```python
async def sampling_loop(...):
    while True:
        # ... API call ...

        tool_result_content: list[BetaToolResultBlockParam] = []
        for content_block in response_params:
            if isinstance(content_block, dict) and content_block.get("type") == "tool_use":
                # Process tool
                ...

        if not tool_result_content:
            return messages  # Exit when no tools called
```

**Key insight:** The loop continues indefinitely until the model stops requesting tool use. There's no max_turns by default - it relies entirely on the model's judgment.

**Cost mitigation strategies observed:**
1. **Image truncation**: `only_n_most_recent_images` limits context size
2. **Prompt caching**: Uses Anthropic's prompt caching to reduce costs
3. **Token-efficient tools beta**: `"token-efficient-tools-2025-02-19"` flag

**Recommendation for Nika:**
- Add `context_truncation` strategy for long-running agents
- Implement cost estimation hooks for real-time budget tracking

---

### 4. Microsoft AutoGen Termination Conditions

**Source:** [microsoft/autogen - conditions/_terminations.py](https://github.com/microsoft/autogen/blob/main/python/packages/autogen-agentchat/src/autogen_agentchat/conditions/_terminations.py)

AutoGen has the **most sophisticated termination system** with composable conditions:

| Condition | Description |
|-----------|-------------|
| `StopMessageTermination` | Terminates when a `StopMessage` is received |
| `MaxMessageTermination` | Terminates after N messages |
| `TextMentionTermination` | Terminates when specific text is found |
| `TokenUsageTermination` | Terminates when token budget exceeded |
| `HandoffTermination` | Terminates on agent handoff to target |
| `TimeoutTermination` | Terminates after duration (implicit) |
| `FunctionalTermination` | Custom async/sync function evaluation |

**Implementation pattern:**

```python
class TokenUsageTermination(TerminationCondition):
    def __init__(
        self,
        max_total_token: int | None = None,
        max_prompt_token: int | None = None,
        max_completion_token: int | None = None,
    ) -> None:
        ...

    @property
    def terminated(self) -> bool:
        return (
            (self._max_total_token and self._total_token_count >= self._max_total_token)
            or (self._max_prompt_token and self._prompt_token_count >= self._max_prompt_token)
            or (self._max_completion_token and self._completion_token_count >= self._max_completion_token)
        )

    async def __call__(self, messages: Sequence) -> StopMessage | None:
        if self.terminated:
            raise TerminatedException("Already reached")
        # ... accumulate tokens ...
        if self.terminated:
            return StopMessage(content=f"Token limit reached", source="TokenUsageTermination")
        return None

    async def reset(self) -> None:
        self._total_token_count = 0
```

**Key insights:**
1. **Composable**: Conditions can be combined with `|` (OR) or `&` (AND)
2. **Stateful**: Each condition tracks its own state
3. **Resettable**: `reset()` method for reuse
4. **Source attribution**: `StopMessage` includes `source` field for debugging

**Recommendation for Nika:**
- Implement `TerminationCondition` trait with `check()` and `reset()`
- Support composition: `conditions: [text_mention, token_limit]` in YAML
- Add detailed stop reason to `RigAgentLoopResult`

---

### 5. Comparison with Nika's Current Implementation

**Current Nika implementation** (`src/ast/agent.rs` + `src/runtime/rig_agent_loop.rs`):

```rust
pub struct AgentParams {
    pub max_turns: Option<u32>,           // Max turns limit
    pub token_budget: Option<u32>,        // Token budget
    pub stop_conditions: Vec<String>,     // Text patterns
    pub stop_sequences: Vec<String>,      // LLM stop sequences
    pub depth_limit: Option<u32>,         // Nested agent limit
}

fn check_stop_conditions(&self, output: &str) -> bool {
    self.params.stop_conditions
        .iter()
        .any(|cond| output.contains(cond))
}
```

**Comparison:**

| Feature | AutoGen | Swarm | Anthropic | Nika |
|---------|---------|-------|-----------|------|
| Max turns | via `MaxMessageTermination` | `max_turns` param | None (infinite) | `max_turns` |
| Token budget | `TokenUsageTermination` | No | No | `token_budget` |
| Text match | `TextMentionTermination` | No | No | `stop_conditions` |
| Natural completion | Implicit | `not tool_calls` | `not tool_result_content` | `NaturalCompletion` |
| Composable | Yes (`\|` and `&`) | No | No | No |
| Functional | `FunctionalTermination` | No | No | No |
| Reset | Yes | No | No | No |
| Source attribution | Yes | No | No | Partial |

---

## Actionable Recommendations for Nika

### Priority 1: Structured Termination Conditions

Replace `stop_conditions: Vec<String>` with a composable system:

```yaml
# New YAML syntax proposal
agent:
  prompt: "Research and summarize"
  termination:
    - type: text_mention
      pattern: "COMPLETE"
    - type: max_turns
      limit: 10
    - type: token_budget
      total: 100000
      prompt: 50000
      completion: 50000
    - type: natural_completion  # No tool calls
    - type: tool_call           # Specific tool called
      tool: nika:complete
    mode: any  # any | all (default: any)
```

**Rust implementation:**

```rust
pub trait TerminationCondition: Send + Sync {
    fn check(&mut self, turn: &AgentTurn) -> Option<StopReason>;
    fn reset(&mut self);
    fn name(&self) -> &'static str;
}

pub enum StopReason {
    TextMention { pattern: String, source: String },
    MaxTurns { limit: u32, actual: u32 },
    TokenBudget { budget: u32, used: u32, kind: TokenKind },
    NaturalCompletion,
    ToolCall { tool: String },
    Timeout { duration: Duration },
    Custom { reason: String },
}
```

### Priority 2: Add `nika:complete` Tool

Allow agents to explicitly signal completion:

```yaml
tasks:
  - id: research
    agent:
      prompt: "Research X. Call nika:complete when done."
      tools: [nika:complete, nika:read, nika:write]
```

Tool definition:
```rust
pub struct CompleteToolParams {
    pub result: serde_json::Value,  // Final result
    pub summary: Option<String>,    // Human-readable summary
    pub confidence: Option<f32>,    // 0.0-1.0 confidence score
}
```

### Priority 3: Cost Optimization Strategies

Based on Anthropic's computer-use patterns:

1. **Context truncation**: Limit message history to N most recent
2. **Image/artifact filtering**: Remove old screenshots/files from context
3. **Real-time cost tracking**: Emit `CostEstimate` events

```yaml
agent:
  prompt: "..."
  cost_control:
    max_cost_usd: 1.00
    context_window: 50_000  # Truncate older messages
    image_retention: 5      # Keep only 5 most recent images
```

### Priority 4: Improved Observability

Add detailed termination metadata to events:

```rust
EventKind::AgentComplete {
    task_id: Arc<str>,
    status: RigAgentStatus,
    stop_reason: StopReason,        // NEW: Detailed reason
    total_turns: u32,
    total_tokens: u64,
    total_cost_usd: Option<f64>,    // NEW: Cost if calculable
    termination_source: String,     // NEW: Which condition triggered
}
```

### Priority 5: Declarative vs. Imperative Mode

Support both patterns:

```yaml
# Declarative: Framework detects completion
agent:
  termination:
    - type: text_mention
      pattern: "DONE"

# Imperative: Agent signals completion
agent:
  tools: [nika:complete]
  termination:
    - type: tool_call
      tool: nika:complete
```

---

## Sources

1. [MCP Sequential Thinking Server](https://github.com/modelcontextprotocol/servers/blob/main/src/sequentialthinking/index.ts) - nextThoughtNeeded pattern
2. [OpenAI Swarm](https://github.com/openai/swarm/blob/main/swarm/core.py) - Simple max_turns + natural completion
3. [Anthropic Computer Use Demo](https://github.com/anthropics/anthropic-quickstarts/blob/main/computer-use-demo/computer_use_demo/loop.py) - Infinite loop with tool-based exit
4. [Microsoft AutoGen Terminations](https://github.com/microsoft/autogen/blob/main/python/packages/autogen-agentchat/src/autogen_agentchat/conditions/_terminations.py) - Composable conditions

## Methodology

- **Tools used**: GitHub API, raw file fetching, codebase analysis
- **Pages analyzed**: 8 source files across 4 major frameworks
- **Time period covered**: Current main branches (March 2026)

## Confidence Level

**High** - Primary sources (actual framework code) were analyzed directly. Patterns are consistent across multiple independent implementations.

## Further Research Suggestions

1. **LangGraph termination**: Investigate LangChain's newer graph-based agent termination
2. **Cost tracking implementations**: Research token pricing APIs for accurate cost estimation
3. **Checkpoint/resume**: How frameworks handle agent state persistence for long-running tasks
4. **Circuit breaker patterns**: Automatic retry/backoff strategies for API failures

---

*Research conducted: 2026-03-06*
*Nika version: v0.20.1*
