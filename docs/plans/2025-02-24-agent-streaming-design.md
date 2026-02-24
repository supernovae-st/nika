# Agent Streaming Design - B + C Solution

**Date:** 2025-02-24
**Version:** v0.8.1
**Status:** Approved

## Visual Taxonomy (MUST USE)

### Verb Icons (Canonical)

| Verb | Icon | Description |
|------|------|-------------|
| `infer:` | ⚡ | LLM generation |
| `exec:` | 📟 | Shell command |
| `fetch:` | 🛰️ | HTTP request |
| `invoke:` | 🔌 | MCP tool call |
| `agent:` | 🐔 | Agentic loop (parent) |
| subagent | 🐤 | Spawned via spawn_agent |

### Emoji Themes (Matrix Decrypt)

| Theme | Emojis | Used For |
|-------|--------|----------|
| COSMIC | 🪐🌌🛰️🌙✨🔮🌟☄️🚀 | Infer verb |
| NATURE | 🌿🌺🍃🌸🌼🌻🪻🌾 | Exec verb |
| PIRATE | 🏴‍☠️⚓🦜🗡️💀🪝 | Fetch verb |
| TECH | 🔌💾🖥️📡🔋⚙️ | Invoke verb |
| ANIMAL | 🐔🐤🦊🐺🦁🐯🦋 | Agent verb |
| UNICORN | 🦄🌈🧚🦋✨💖🌸 | Special/Chat |

### 🦋 Nika Butterfly Spinner (NEW - v0.8.1)

```rust
/// Nika logo spinner - butterfly "hacking" animation
const NIKA_SPINNER: &[&str] = &["🦋", "🔮", "✨", "💫", "⚡", "🌟"];

/// Extended hacking animation with particles
const NIKA_HACKING: &[&str] = &[
    "🦋    ", "🦋✨  ", "✨🦋✨", "💫🦋💫",
    "⚡🦋⚡", "🔮🦋🔮", "✨🦋✨", "🦋💫  "
];
```

**Usage:** Status bar during agent execution, Mission Control header

## Executive Summary

Implement real-time streaming for `/agent` command using rig-core's `stream_prompt()` API with `MultiTurnStreamItem`. This replaces the blocking `agent.prompt()` call with a streaming loop that sends tokens and tool calls to the TUI in real-time.

## Problem Statement

Currently, `/agent` with MCP tools uses `agent.prompt().await` which is **blocking**:
- User sees nothing until the entire response is ready
- No feedback on what the agent is doing
- No Matrix decrypt effect (no streaming tokens)
- MCP calls happen invisibly

## Solution: B + C Combination

| Component | Description |
|-----------|-------------|
| **B: Activity Indicators** | Real-time MCP call visibility via EventLog broadcast (already wired) |
| **C: Real Streaming** | Use `stream_prompt()` + `MultiTurnStreamItem` for token-by-token streaming |

## Research Findings

### rig-core Streaming API (Context7)

```rust
use rig::streaming::StreamedAssistantContent;
use rig::agent::prompt_request::streaming::MultiTurnStreamItem;
use futures::StreamExt;

let mut stream = agent.stream_prompt("prompt").await?;

while let Some(chunk) = stream.next().await {
    match chunk? {
        MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::Text(text)
        ) => {
            // STREAMING TEXT - send to TUI
            send_token(text.text);
        }
        MultiTurnStreamItem::StreamAssistantItem(
            StreamedAssistantContent::ToolCall { tool_call, .. }
        ) => {
            // TOOL CALL - show in Mission Control
            handle_tool_call(tool_call);
        }
        MultiTurnStreamItem::FinalResponse(resp) => {
            // DONE
            finalize(resp);
        }
        _ => {}
    }
}
```

### Industry Best Practices (Perplexity Research)

| Agent | Streaming Approach | Key UX Elements |
|-------|-------------------|-----------------|
| Claude Code | Live traces, progress panels | Expandable steps, auto-dismiss |
| Mistral Vibe | Rich/Textual for effects | Slash-commands, multi-pane |
| Aider | Git-native, auto-commits | Terminal-first, lightweight |
| Cursor | Inline edits, semantic indexing | IDE-like in terminal |

### Terminal Animation Techniques (GitHub Blog)

- ANSI control sequences for cursor movement
- Manual frame rendering at 60 FPS
- Buffer management for smooth animations
- Unicode spinners: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`

## Architecture

### Current Flow (Blocking)

```
handle_chat_agent()
    │
    └── RigAgentLoop::new()
            │
            └── run_auto()
                    │
                    └── run_claude()
                            │
                            └── stream_with_tools()
                                    │
                                    └── agent.prompt().await  ← BLOCKING
                                            │
                                            └── Full response (no streaming)
```

### New Flow (Streaming)

```
handle_chat_agent()
    │
    └── RigAgentLoop::new().with_stream_tx(status_tx)
            │
            └── run_auto()
                    │
                    └── run_claude_streaming()  ← NEW METHOD
                            │
                            └── agent.stream_prompt().await
                                    │
                                    ├── MultiTurnStreamItem::Text → StreamChunk::Token
                                    ├── MultiTurnStreamItem::ToolCall → StreamChunk::McpCallStart
                                    └── MultiTurnStreamItem::FinalResponse → StreamChunk::Done
```

## Implementation Plan

### Phase 1: Core Streaming (2-3 hours)

#### Task 1.1: Add `run_claude_streaming()` method

**File:** `src/runtime/rig_agent_loop.rs`

```rust
/// Run agent with real-time streaming (v0.8.1)
/// Uses stream_prompt() instead of prompt() for token-by-token delivery
pub async fn run_claude_streaming(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    use rig::agent::prompt_request::streaming::MultiTurnStreamItem;
    use rig::streaming::StreamedAssistantContent;
    use futures::StreamExt;

    let client = anthropic::Client::from_env();
    let model_name = self.params.model.as_deref().unwrap_or("claude-sonnet-4-6");
    let model = client.completion_model(model_name);

    // Take ownership of tools
    let tools = std::mem::take(&mut self.tools);
    let max_turns = self.params.max_turns.unwrap_or(10) as usize;

    // Build agent with tools
    let mut builder = AgentBuilder::new(model)
        .preamble(self.params.system.clone().unwrap_or_default())
        .tools(tools)
        .max_tokens(8192);

    if let Some(temp) = self.params.effective_temperature() {
        builder = builder.temperature(f64::from(temp));
    }

    let agent = builder.build();

    // Emit start event
    self.event_log.emit(EventKind::AgentTurn {
        task_id: Arc::from(self.task_id.as_str()),
        turn_index: 1,
        kind: "started".to_string(),
        metadata: None,
    });

    // v0.8.1: STREAMING with stream_prompt()
    let mut stream = agent
        .stream_prompt(&self.params.prompt)
        .max_turns(max_turns)
        .await
        .map_err(|e| NikaError::AgentExecutionError {
            task_id: self.task_id.clone(),
            reason: format!("Stream prompt failed: {}", e),
        })?;

    let mut response_text = String::new();
    let mut turn_count = 0u32;
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(item) => match item {
                // Streaming text - send to TUI
                MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::Text(text)
                ) => {
                    if let Some(ref tx) = self.stream_tx {
                        let _ = tx.try_send(StreamChunk::Token(text.text.clone()));
                    }
                    response_text.push_str(&text.text);
                }

                // Tool call - notify TUI and execute
                MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::ToolCall { tool_call, .. }
                ) => {
                    turn_count += 1;

                    // Send McpCallStart to TUI
                    if let Some(ref tx) = self.stream_tx {
                        let _ = tx.try_send(StreamChunk::McpCallStart {
                            tool: tool_call.function.name.clone(),
                            server: "agent".to_string(),
                            params: tool_call.function.arguments.clone(),
                        });
                    }

                    // Log event
                    self.event_log.emit(EventKind::McpInvoke {
                        task_id: Arc::from(self.task_id.as_str()),
                        mcp_server: "agent".to_string(),
                        tool: Some(tool_call.function.name.clone()),
                        params: serde_json::from_str(&tool_call.function.arguments).ok(),
                    });
                }

                // Thinking content (Claude extended thinking)
                MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::ReasoningDelta { reasoning, .. }
                ) => {
                    if let Some(ref tx) = self.stream_tx {
                        let _ = tx.try_send(StreamChunk::Thinking(reasoning));
                    }
                }

                // Final response
                MultiTurnStreamItem::FinalResponse(resp) => {
                    response_text = resp.response().to_string();

                    // Extract token usage if available
                    if let Some(usage) = resp.usage() {
                        total_input_tokens = usage.input_tokens;
                        total_output_tokens = usage.output_tokens;
                    }

                    // Send metrics to TUI
                    if let Some(ref tx) = self.stream_tx {
                        let _ = tx.try_send(StreamChunk::Metrics {
                            input_tokens: total_input_tokens,
                            output_tokens: total_output_tokens,
                        });
                    }
                }

                _ => {} // Ignore other variants
            },
            Err(e) => {
                tracing::warn!(error = %e, "Stream chunk error");
            }
        }
    }

    // Determine status
    let status = if self.check_stop_conditions(&response_text) {
        RigAgentStatus::StopConditionMet
    } else {
        RigAgentStatus::NaturalCompletion
    };

    // Emit completion event
    let metadata = AgentTurnMetadata {
        thinking: None,
        response_text: response_text.clone(),
        input_tokens: total_input_tokens as u32,
        output_tokens: total_output_tokens as u32,
        cache_read_tokens: 0,
        stop_reason: status.as_canonical_str().to_string(),
    };

    self.event_log.emit(EventKind::AgentTurn {
        task_id: Arc::from(self.task_id.as_str()),
        turn_index: turn_count.max(1),
        kind: status.as_canonical_str().to_string(),
        metadata: Some(metadata),
    });

    Ok(RigAgentLoopResult {
        status,
        turns: turn_count.max(1),
        final_output: serde_json::json!({ "response": response_text }),
        total_tokens: total_input_tokens + total_output_tokens,
    })
}
```

#### Task 1.2: Update `run_auto()` to use streaming

**File:** `src/runtime/rig_agent_loop.rs`

```rust
pub async fn run_auto(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    // Check explicit provider from params
    if let Some(ref provider) = self.params.provider {
        match provider.to_lowercase().as_str() {
            "claude" | "anthropic" => {
                // v0.8.1: Use streaming if stream_tx is set
                if self.stream_tx.is_some() && !self.tools.is_empty() {
                    return self.run_claude_streaming().await;
                }
                return self.run_claude().await;
            }
            // ... other providers
        }
    }

    // Auto-detect based on available API keys
    let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.is_empty());

    if has_key("ANTHROPIC_API_KEY") {
        // v0.8.1: Use streaming if stream_tx is set
        if self.stream_tx.is_some() && !self.tools.is_empty() {
            return self.run_claude_streaming().await;
        }
        return self.run_claude().await;
    }

    // ... rest of auto-detection
}
```

### Phase 2: TUI Integration (1-2 hours)

#### Task 2.1: Update StreamChunk handling in ChatView

**File:** `src/tui/views/chat.rs`

Already handles `StreamChunk::Token`, `StreamChunk::McpCallStart`, etc. No changes needed.

#### Task 2.2: Mission Control Activity Tracking

**File:** `src/tui/widgets/mission_control.rs`

Add turn progress indicator:

```rust
pub struct TurnProgress {
    pub current_turn: u32,
    pub max_turns: u32,
    pub current_tool: Option<String>,
}
```

### Phase 3: WOW Effects (2-3 hours)

#### Task 3.1: Enhanced Matrix Decrypt

**File:** `src/tui/widgets/matrix_decrypt.rs`

- Add color transition: green → white
- Add sparkle cursor: `█ ▓ ░ ✦ ✧`

#### Task 3.2: Tool Call Pulse Effect

**File:** `src/tui/widgets/mcp_call_box.rs`

- Animated border when tool is running
- Progress bar with shimmer effect

#### Task 3.3: Thinking Wave Animation

**File:** `src/tui/widgets/thinking_wave.rs` (new)

```rust
pub struct ThinkingWave {
    frame: usize,
    width: u16,
}

impl ThinkingWave {
    const WAVE: &'static [char] = &['∿', '∾', '≈', '~', '⌇'];

    pub fn tick(&mut self) {
        self.frame = (self.frame + 1) % 20;
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Render wave animation
    }
}
```

### Phase 4: Testing (1 hour)

#### Task 4.1: Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_streaming_sends_tokens() {
        // Mock stream_prompt
        // Verify StreamChunk::Token sent for each text chunk
    }

    #[tokio::test]
    async fn test_streaming_handles_tool_calls() {
        // Verify StreamChunk::McpCallStart sent for tool calls
    }
}
```

#### Task 4.2: Integration Test

```bash
# Manual test
nika chat
> /agent Generate a landing page for QR Code AI --mcp novanet
# Verify:
# - Tokens stream in real-time
# - MCP calls visible in Mission Control
# - Matrix decrypt effect works
# - Final metrics displayed
```

## File Changes Summary

| File | Changes |
|------|---------|
| `src/runtime/rig_agent_loop.rs` | Add `run_claude_streaming()`, update `run_auto()` |
| `src/tui/widgets/mission_control.rs` | Add `TurnProgress` struct |
| `src/tui/widgets/matrix_decrypt.rs` | Enhance with color transition |
| `src/tui/widgets/mcp_call_box.rs` | Add pulse animation |
| `src/tui/widgets/thinking_wave.rs` | New file for wave animation |
| `src/tui/widgets/mod.rs` | Export new widgets |

## Timeline

| Phase | Effort | Description |
|-------|--------|-------------|
| Phase 1 | 2-3h | Core streaming implementation |
| Phase 2 | 1-2h | TUI integration |
| Phase 3 | 2-3h | WOW effects |
| Phase 4 | 1h | Testing |
| **Total** | **6-9h** | |

## Success Criteria

1. ✅ Tokens stream in real-time (no waiting for full response)
2. ✅ MCP tool calls visible in Mission Control during execution
3. ✅ Matrix decrypt effect triggers on streaming tokens
4. ✅ Turn progress visible in status bar
5. ✅ Final metrics (tokens, cost) displayed after completion
6. ✅ All existing tests pass
7. ✅ No performance regression (60 FPS maintained)

## Rollback Plan

If streaming causes issues:
1. Keep `run_claude()` as fallback
2. Add feature flag: `--no-stream` CLI option
3. Detect streaming support at runtime

## References

- [rig-core patterns.md](https://github.com/0xplaygrounds/rig/blob/main/skills/rig/references/patterns.md)
- [GitHub Copilot CLI Animation Blog](https://github.blog/engineering/from-pixels-to-characters-the-engineering-behind-github-copilot-clis-animated-ascii-banner/)
- [Ratatui Documentation](https://github.com/ratatui)
- [Claude Code UX Patterns](https://www.aiuxdesign.guide/patterns/agent-status-monitoring)
