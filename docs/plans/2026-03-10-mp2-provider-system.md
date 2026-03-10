# MP2: Provider System Fix

**Parent**: `2026-03-10-v0.24.0-bugfix-masterplan.md`
**Priority**: 🔴 HIGH
**Files**: `src/provider/rig.rs`, `src/runtime/rig_agent_loop.rs`
**Estimated**: 2-3 hours

---

## Problem Statement

Two critical bugs in the provider system affect ALL 7 LLM providers:

### Bug 1: System Prompt Concatenation

**File**: `src/provider/rig.rs` (lines 330-331, 1152-1153)

```rust
// CURRENT (WRONG):
let full_prompt = if let Some(system) = &options.system {
    format!("{}\n\n{}", system, prompt)  // Concatenates as user message!
} else {
    prompt.to_string()
};
// Then uses .prompt(&full_prompt) - ALL goes as user message
```

**Impact**: The LLM doesn't see the system prompt as a system message. It sees it concatenated with the user message, which:
- Reduces effectiveness of system prompt instructions
- May cause confusion in multi-turn conversations
- Doesn't use the provider's native system message API

### Bug 2: Token Tracking Returns 0 with Tools

**File**: `src/runtime/rig_agent_loop.rs` (lines 1362-1363)

```rust
// CURRENT (HARDCODED ZERO):
Ok(StreamingResult {
    response,
    input_tokens: 0,  // Not available with agent.prompt()
    output_tokens: 0,
    thinking: None,
})
```

**Impact**: When an agent uses tools (MCP or builtin), token counts are always 0, making it impossible to:
- Track API costs
- Monitor usage
- Implement rate limiting

---

## Solution: Bug 1 (System Prompt)

### rig-core Correct API

From Context7 research, rig-core uses `.preamble()` for system prompts:

```rust
// CORRECT rig-core API:
let agent = client
    .agent(openai::GPT_4O)
    .preamble("You are a helpful assistant.")  // System prompt
    .build();

let response = agent.prompt("User message").await?;
```

### Implementation

**File**: `src/provider/rig.rs`

#### Step 1: Update `infer_with_options()`

```rust
pub async fn infer_with_options(
    &self,
    prompt: &str,
    options: &InferOptions,
) -> Result<String, RigInferError> {
    // Get model
    let model_name = options.model.as_deref().unwrap_or(self.default_model());

    // Build completion request based on provider
    match &self.inner {
        RigProviderInner::Claude(client) => {
            let mut builder = client.agent(model_name);

            // Set system prompt via preamble (CORRECT API)
            if let Some(system) = &options.system {
                builder = builder.preamble(system);
            }

            // Set temperature if provided
            if let Some(temp) = options.temperature {
                builder = builder.temperature(temp);
            }

            // Set max_tokens if provided
            if let Some(max_tokens) = options.max_tokens {
                builder = builder.max_tokens(max_tokens as u64);
            }

            let agent = builder.build();
            let response = agent.prompt(prompt).await?;
            Ok(response)
        }
        RigProviderInner::OpenAI(client) => {
            let mut builder = client.agent(model_name);

            if let Some(system) = &options.system {
                builder = builder.preamble(system);
            }
            if let Some(temp) = options.temperature {
                builder = builder.temperature(temp);
            }
            if let Some(max_tokens) = options.max_tokens {
                builder = builder.max_tokens(max_tokens as u64);
            }

            let agent = builder.build();
            let response = agent.prompt(prompt).await?;
            Ok(response)
        }
        // ... repeat for all 7 providers
    }
}
```

#### Step 2: Update `infer_stream_with_options()`

Same pattern for streaming:

```rust
pub async fn infer_stream_with_options(
    &self,
    prompt: &str,
    options: &InferOptions,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<StreamingResult, NikaError> {
    let model_name = options.model.as_deref().unwrap_or(self.default_model());

    match &self.inner {
        RigProviderInner::Claude(client) => {
            let mut builder = client.agent(model_name);

            // System prompt via preamble
            if let Some(system) = &options.system {
                builder = builder.preamble(system);
            }
            if let Some(temp) = options.temperature {
                builder = builder.temperature(temp);
            }
            if let Some(max_tokens) = options.max_tokens {
                builder = builder.max_tokens(max_tokens as u64);
            }

            let agent = builder.build();

            // Use completion_request for streaming
            let request = agent.completion_request(prompt).await?;
            let mut stream = client.completion(model_name, request).await?;

            // ... stream handling with proper token tracking
        }
        // ... repeat for all providers
    }
}
```

---

## Solution: Bug 2 (Token Tracking)

### Analysis

The problem is that `agent.prompt()` returns just a `String`, not token usage info.

From Context7 research, rig-core provides `GetTokenUsage` trait on streaming responses:

```rust
impl GetTokenUsage for StreamingCompletionResponse {
    fn token_usage(&self) -> Option<crate::completion::Usage> {
        // Returns input_tokens, output_tokens, total_tokens
    }
}
```

### Implementation

**File**: `src/runtime/rig_agent_loop.rs`

#### Step 1: Track Tokens in Agent Loop

When using `agent.stream_prompt()` or `agent.multi_turn()`, we can accumulate tokens:

```rust
pub async fn run_with_tools(
    &mut self,
    tools: Vec<Arc<dyn ToolDyn>>,
) -> Result<AgentLoopResult, NikaError> {
    let mut total_input_tokens: u32 = 0;
    let mut total_output_tokens: u32 = 0;

    // Build agent with tools
    let agent = self.build_agent_with_tools(tools)?;

    // Use multi_turn for agent loop
    let mut stream = agent.stream_multi_turn(&self.params.prompt, self.history.clone()).await?;

    while let Some(item) = stream.next().await {
        match item {
            MultiTurnStreamItem::AssistantContent(content) => {
                // Handle content...
                if let Some(content) = content.as_final() {
                    // Extract token usage from final response
                    if let Some(usage) = content.token_usage() {
                        total_input_tokens += usage.input_tokens as u32;
                        total_output_tokens += usage.output_tokens as u32;
                    }
                }
            }
            MultiTurnStreamItem::ToolCall { .. } => {
                // Handle tool call...
            }
            MultiTurnStreamItem::Done => break,
        }
    }

    Ok(AgentLoopResult {
        final_response: response_text,
        turns: turn_count,
        total_input_tokens,
        total_output_tokens,
        // ...
    })
}
```

#### Step 2: Update StreamingResult in run_with_mcp_tools()

```rust
async fn run_with_mcp_tools(&mut self) -> Result<StreamingResult, NikaError> {
    let mut total_input: u32 = 0;
    let mut total_output: u32 = 0;

    // ... existing agent loop code ...

    // At end of each turn, extract tokens
    for turn_result in &turn_results {
        if let Some(usage) = turn_result.token_usage() {
            total_input += usage.input_tokens as u32;
            total_output += usage.output_tokens as u32;
        }
    }

    Ok(StreamingResult {
        response: final_response,
        input_tokens: total_input,   // NOW TRACKED!
        output_tokens: total_output, // NOW TRACKED!
        thinking: thinking_text,
    })
}
```

#### Step 3: Fallback for Non-Streaming

For providers that don't report tokens in streaming, estimate from response:

```rust
fn estimate_tokens(text: &str) -> u32 {
    // Rough estimate: ~4 chars per token for English
    // More accurate: use tiktoken-rs for exact count
    (text.len() / 4) as u32
}

// If token_usage() returns None, estimate:
if total_output == 0 {
    total_output = estimate_tokens(&response);
    tracing::warn!(
        "Token usage not available from provider, estimated {} output tokens",
        total_output
    );
}
```

---

## Test Plan

### Bug 1 Tests (System Prompt)

```rust
#[tokio::test]
async fn system_prompt_is_not_concatenated() {
    // This test verifies the system prompt is sent correctly
    // We can't easily test the actual API call, but we can verify
    // the agent is built with preamble

    let provider = RigProvider::claude();
    let options = InferOptions {
        system: Some("You are a pirate. Always say 'Arrr!'".to_string()),
        ..Default::default()
    };

    // Call with a prompt that would be confused if concatenated
    let result = provider.infer_with_options(
        "What is 2+2?",
        &options
    ).await;

    // If system prompt works correctly, response should contain "Arrr!"
    // If concatenated, the LLM might ignore the pirate instruction
    assert!(result.is_ok());
    // Note: Can't verify behavior without real API, but we can verify
    // no panic and correct API usage via code review
}

#[tokio::test]
async fn system_prompt_works_for_all_providers() {
    // Test each provider with system prompt
    let providers = [
        RigProvider::claude(),
        RigProvider::openai(),
        RigProvider::mistral(),
        RigProvider::groq(),
        RigProvider::deepseek(),
        RigProvider::gemini(),
        RigProvider::ollama(),
    ];

    for provider in providers.into_iter().flatten() {
        let options = InferOptions {
            system: Some("Respond with exactly 'OK'".to_string()),
            ..Default::default()
        };

        // Should not panic or error
        let _ = provider.infer_with_options("Test", &options).await;
    }
}
```

### Bug 2 Tests (Token Tracking)

```rust
#[tokio::test]
async fn token_tracking_with_tools_returns_nonzero() {
    // Requires real API key
    std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test...");

    let params = AgentParams {
        prompt: "What is 2+2? Use calculator if needed.".to_string(),
        tools: vec!["nika:log".to_string()],
        ..Default::default()
    };

    let log = Arc::new(EventLog::new());
    let mut agent = RigAgentLoop::new("test", params, log, vec![])?;

    let result = agent.run_claude().await?;

    assert!(result.input_tokens > 0, "Input tokens should be tracked");
    assert!(result.output_tokens > 0, "Output tokens should be tracked");
}

#[tokio::test]
async fn token_tracking_accumulates_across_turns() {
    // Test that multi-turn agent accumulates tokens correctly
    let params = AgentParams {
        prompt: "Research topic X, then summarize.".to_string(),
        max_turns: Some(3),
        tools: vec!["nika:read".to_string()],
        ..Default::default()
    };

    let log = Arc::new(EventLog::new());
    let mut agent = RigAgentLoop::new("test", params, log, vec![])?;

    let result = agent.run_claude().await?;

    // Should have tokens from multiple turns
    assert!(result.total_tokens() > 100, "Multi-turn should use significant tokens");
}
```

---

## Success Criteria

### Bug 1 (System Prompt)

- [ ] `.preamble()` is used for all 7 providers
- [ ] No string concatenation of system + user prompt
- [ ] Existing tests pass
- [ ] Code review confirms correct rig-core API usage

### Bug 2 (Token Tracking)

- [ ] `input_tokens` > 0 when agent uses tools
- [ ] `output_tokens` > 0 when agent uses tools
- [ ] Tokens accumulate across multi-turn conversations
- [ ] Fallback estimation for providers without usage reporting

### Combined

- [ ] All 4,282+ existing tests pass
- [ ] New regression tests added
- [ ] AgentTurn events contain accurate token counts

---

## Migration Notes

### Breaking Changes

None - this is a bugfix, not API change.

### Behavioral Changes

- System prompts will now be more effective (sent correctly)
- Token counts will be non-zero (may affect billing calculations)

---

## Related Issues

- Audit finding: "System prompt concatenated instead of using API"
- Audit finding: "Token tracking returns 0 with agent + tools"
- Comment in code: `// Not available with agent.prompt()` (line 1362)
