// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Non-streaming inference methods for `RigProvider`.
//!
//! Covers simple text, vision, tool-based, and options-based inference.
//! Also includes capability checks (vision, thinking, structured output).

use super::*;

impl RigProvider {
    /// Shared low-level POST to /chat/completions for OpenAI-compatible endpoints.
    ///
    /// Returns the parsed JSON response body + token usage. Both `raw_openai_compat_infer`
    /// and `infer_with_tools` (OpenAiCompat arm) delegate here, eliminating HTTP code
    /// duplication and ensuring token tracking works in both paths.
    pub(super) async fn raw_chat_completion(
        http_client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        body: serde_json::Value,
        timeout: std::time::Duration,
    ) -> Result<(serde_json::Value, u64, u64), RigInferError> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

        let mut req = http_client.post(&url).json(&body).timeout(timeout);
        if !api_key.is_empty() {
            req = req.bearer_auth(api_key);
        }

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                RigInferError::Timeout {
                    duration_ms: timeout.as_millis() as u64,
                }
            } else {
                RigInferError::PromptError(format!("HTTP error: {e}"))
            }
        })?;

        let status = resp.status();
        let body_text = resp.text().await.map_err(|e| {
            RigInferError::PromptError(format!("failed to read response body: {e}"))
        })?;

        if !status.is_success() {
            // H2: Truncate error body to avoid leaking internal infra details
            let truncated = if body_text.len() > 500 {
                format!("{}...(truncated)", &body_text[..500])
            } else {
                body_text.clone()
            };
            return Err(RigInferError::PromptError(format!(
                "HTTP {status}: {truncated}"
            )));
        }

        let json: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| RigInferError::PromptError(format!("invalid JSON response: {e}")))?;

        // T15: Extract token usage from response
        let prompt_tokens = json
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion_tokens = json
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok((json, prompt_tokens, completion_tokens))
    }

    /// Raw HTTP completion for OpenAI-compatible endpoints.
    ///
    /// Bypasses rig-core deserialization entirely — extracts `choices[0].message.content`
    /// from the raw JSON response. This avoids deserialization failures with vLLM, Ollama,
    /// and other servers that add non-standard fields (annotations, reasoning, stop_reason).
    #[allow(clippy::too_many_arguments)]
    /// Returns `(content, prompt_tokens, completion_tokens)`.
    pub(super) async fn raw_openai_compat_infer(
        http_client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        model: &str,
        messages: Vec<serde_json::Value>,
        max_tokens: u64,
        temperature: Option<f64>,
        timeout: std::time::Duration,
    ) -> Result<(String, u64, u64), RigInferError> {
        use nika_core::catalogs::capabilities::{model_capabilities, TokenLimitParam};

        // Infer provider from base_url for capability lookup
        let provider_hint = if base_url.contains("api.openai.com") {
            "openai"
        } else if base_url.contains("openrouter.ai") {
            "openrouter"
        } else {
            // Custom endpoint — safe defaults (max_tokens, allow temperature)
            "custom"
        };
        let caps = model_capabilities(provider_hint, model);

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
        });
        match caps.token_limit_param {
            TokenLimitParam::MaxCompletionTokens => {
                body["max_completion_tokens"] = serde_json::json!(max_tokens);
            }
            TokenLimitParam::MaxTokens => {
                body["max_tokens"] = serde_json::json!(max_tokens);
            }
        }
        if let Some(temp) = temperature {
            if caps.supports_temperature {
                body["temperature"] = serde_json::json!(temp);
            } else {
                tracing::warn!(model, "temperature stripped — model does not support it");
            }
        }

        let (json, prompt_tokens, completion_tokens) =
            Self::raw_chat_completion(http_client, base_url, api_key, body, timeout).await?;

        let content = json["choices"]
            .get(0)
            .and_then(|c| c["message"]["content"].as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                RigInferError::PromptError(
                    "no content in response choices[0].message.content".into(),
                )
            })?;

        Ok((content, prompt_tokens, completion_tokens))
    }

    /// Simple text completion (infer) using rig-core
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `model` - Model identifier (uses default if None)
    ///
    /// # Returns
    /// The completion text from the model
    pub async fn infer(
        &self,
        prompt: &str,
        model: Option<&str>,
        max_tokens: Option<u64>,
    ) -> Result<String, RigInferError> {
        /// Maximum time to wait for a single infer() completion (5 minutes).
        /// Prevents hung LLM calls from blocking the runtime indefinitely.
        const INFER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = model.unwrap_or_else(|| self.default_model());
        let effective_max_tokens = max_tokens.unwrap_or(8192);

        match self {
            RigProvider::OpenAiCompat {
                raw_base_url,
                raw_api_key,
                timeout_secs,
                http_client,
                ..
            } => {
                let compat_timeout = std::time::Duration::from_secs(*timeout_secs);
                let messages = vec![serde_json::json!({"role": "user", "content": prompt})];
                let (content, prompt_tokens, completion_tokens) = Self::raw_openai_compat_infer(
                    http_client,
                    raw_base_url,
                    raw_api_key,
                    model_id,
                    messages,
                    effective_max_tokens,
                    None,
                    compat_timeout,
                )
                .await?;
                tracing::debug!(prompt_tokens, completion_tokens, "OpenAiCompat infer usage");
                Ok(content)
            }
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                // Native inference uses direct API, not rig-core agent
                // Model must be pre-loaded via load_native_model()
                timeout(
                    INFER_TIMEOUT,
                    runtime.infer(prompt, super::super::native::ChatOptions::default()),
                )
                .await
                .map_err(|_| RigInferError::Timeout {
                    duration_ms: INFER_TIMEOUT.as_millis() as u64,
                })?
                .map(|r| r.message.content)
                .map_err(|e: super::super::native::NativeError| {
                    RigInferError::PromptError(e.to_string())
                })
            }
            // All rig-core providers (Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi)
            _ => {
                let (rig_max, token_params) =
                    token_limit_for_model(self.name(), model_id, effective_max_tokens);
                dispatch_rig!(self, |client| {
                    let mut builder = client.agent(model_id);
                    if let Some(mt) = rig_max {
                        builder = builder.max_tokens(mt);
                    }
                    if let Some(params) = token_params.clone() {
                        builder = builder.additional_params(params);
                    }
                    let agent = builder.build();
                    timeout(INFER_TIMEOUT, agent.prompt(prompt))
                        .await
                        .map_err(|_| RigInferError::Timeout {
                            duration_ms: INFER_TIMEOUT.as_millis() as u64,
                        })?
                        .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
                })
            }
        }
    }

    /// Vision inference: send multimodal content (text + images) to the LLM.
    ///
    /// Builds a `Message::User` with mixed text + base64 image parts,
    /// then uses `agent.prompt(message)` to send it. The agent handles
    /// provider-specific message formatting automatically.
    ///
    /// # Arguments
    /// * `user_content` - Pre-built rig UserContent items (text + images)
    /// * `model` - Optional model override
    /// * `system` - Optional system prompt
    /// * `max_tokens` - Optional max tokens
    ///
    /// # Errors
    /// Returns `RigInferError::VisionNotSupported` for DeepSeek provider.
    /// Native vision requires a VisionHf model to be loaded.
    pub async fn infer_vision(
        &self,
        user_content: Vec<rig::completion::message::UserContent>,
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<String, RigInferError> {
        use rig::completion::message::Message;
        use rig::OneOrMany;

        /// Maximum time to wait for a vision inference call (5 minutes).
        const VISION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        // Early return: DeepSeek does not support vision at all
        if matches!(self, RigProvider::DeepSeek(_)) {
            return Err(RigInferError::VisionNotSupported(
                "DeepSeek does not support vision/multimodal content".to_string(),
            ));
        }

        // Early return: Native vision uses NativeRuntime directly (not rig-core)
        #[cfg(feature = "native-inference")]
        if let RigProvider::Native(runtime) = self {
            if !runtime.supports_vision() {
                return Err(RigInferError::VisionNotSupported(
                    "Native model does not support vision. Load a vision model via \
                     NativeModelKind::VisionHf (e.g., `nika model vision <model_id> --isq Q4K`)"
                        .to_string(),
                ));
            }
            let (prompt_text, vision_images) =
                super::provider_streaming::extract_native_vision_parts(&user_content)?;
            let options = super::super::native::ChatOptions {
                max_tokens,
                ..Default::default()
            };
            let response = timeout(
                VISION_TIMEOUT,
                runtime.infer_vision(&prompt_text, vision_images, options),
            )
            .await
            .map_err(|_| RigInferError::Timeout {
                duration_ms: VISION_TIMEOUT.as_millis() as u64,
            })?
            .map_err(|e: super::super::native::NativeError| {
                RigInferError::PromptError(e.to_string())
            })?;
            return Ok(response.message.content);
        }

        let model_id = model.unwrap_or_else(|| self.default_model());
        let max_tok = max_tokens.map(|v| v.max(16)).map(u64::from).unwrap_or(8192);
        let (rig_max, token_params) = token_limit_for_model(self.name(), model_id, max_tok);

        let message = Message::User {
            content: OneOrMany::many(user_content).map_err(|_| {
                RigInferError::VisionNotSupported("content parts list is empty".to_string())
            })?,
        };

        macro_rules! vision_prompt {
            ($client:expr) => {{
                let mut builder = $client.agent(model_id);
                if let Some(mt) = rig_max {
                    builder = builder.max_tokens(mt);
                }
                if let Some(ref params) = token_params {
                    builder = builder.additional_params(params.clone());
                }
                if let Some(sys) = system {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                timeout(VISION_TIMEOUT, agent.prompt(message))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: VISION_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }};
        }

        match self {
            RigProvider::Claude(client) => vision_prompt!(client),
            RigProvider::OpenAI(client) => vision_prompt!(client),
            RigProvider::Mistral(client) => vision_prompt!(client),
            RigProvider::Groq(client) => vision_prompt!(client),
            RigProvider::Gemini(client) => vision_prompt!(client),
            RigProvider::XAi(client) => vision_prompt!(client),
            RigProvider::OpenAiCompat {
                client,
                timeout_secs,
                ..
            } => {
                let compat_timeout = std::time::Duration::from_secs(*timeout_secs);
                let mut builder = client.agent(model_id);
                if let Some(mt) = rig_max {
                    builder = builder.max_tokens(mt);
                }
                if let Some(ref params) = token_params {
                    builder = builder.additional_params(params.clone());
                }
                if let Some(sys) = system {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                timeout(compat_timeout, agent.prompt(message))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: compat_timeout.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            // DeepSeek and Native are handled above via early returns.
            // These arms exist for exhaustiveness in case the early returns are refactored.
            RigProvider::DeepSeek(_) => Err(RigInferError::VisionNotSupported(
                "DeepSeek does not support vision".to_string(),
            )),
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(_) => Err(RigInferError::VisionNotSupported(
                "Native provider requires NativeRuntime path for vision".to_string(),
            )),
        }
    }

    /// Vision inference with streaming output.
    ///
    /// Same as `infer_vision` but streams response tokens via an mpsc channel.
    /// Native vision uses a non-streaming fallback (sends full response as Done chunk).
    pub async fn infer_vision_stream(
        &self,
        user_content: Vec<rig::completion::message::UserContent>,
        tx: mpsc::Sender<StreamChunk>,
        model: Option<&str>,
        system: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Result<StreamResult, RigInferError> {
        use rig::completion::message::Message;
        use rig::OneOrMany;

        /// Maximum time to wait for a vision stream call (5 minutes).
        const VISION_STREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        // Early return: DeepSeek does not support vision at all
        if matches!(self, RigProvider::DeepSeek(_)) {
            return Err(RigInferError::VisionNotSupported(
                "DeepSeek does not support vision/multimodal content".to_string(),
            ));
        }

        // Early return: Native vision — non-streaming fallback via NativeRuntime
        // NativeRuntime.infer_vision_stream() exists but rig's StreamChunk protocol
        // differs from the native mpsc stream, so we use non-streaming + Done chunk.
        #[cfg(feature = "native-inference")]
        if let RigProvider::Native(runtime) = self {
            if !runtime.supports_vision() {
                return Err(RigInferError::VisionNotSupported(
                    "Native model does not support vision. Load a vision model via \
                     NativeModelKind::VisionHf (e.g., `nika model vision <model_id> --isq Q4K`)"
                        .to_string(),
                ));
            }
            let (prompt_text, vision_images) =
                super::provider_streaming::extract_native_vision_parts(&user_content)?;
            let options = super::super::native::ChatOptions {
                max_tokens,
                ..Default::default()
            };
            let response = timeout(
                VISION_STREAM_TIMEOUT,
                runtime.infer_vision(&prompt_text, vision_images, options),
            )
            .await
            .map_err(|_| RigInferError::Timeout {
                duration_ms: VISION_STREAM_TIMEOUT.as_millis() as u64,
            })?
            .map_err(|e: super::super::native::NativeError| {
                RigInferError::PromptError(e.to_string())
            })?;
            // Send full response as a single Done chunk (non-streaming fallback)
            let text = response.message.content;
            if let Err(e) = tx.send(StreamChunk::Done(text.clone())).await {
                tracing::warn!(error = %e, "Vision result channel closed — TUI may not show output");
            }
            return Ok(StreamResult {
                text,
                ..Default::default()
            });
        }

        let model_id = model.unwrap_or_else(|| self.default_model());
        let max_tok = max_tokens.map(|v| v.max(16)).map(u64::from).unwrap_or(8192);
        let (rig_max, token_params) = token_limit_for_model(self.name(), model_id, max_tok);

        let message = Message::User {
            content: OneOrMany::many(user_content).map_err(|_| {
                RigInferError::VisionNotSupported("content parts list is empty".to_string())
            })?,
        };

        let mut response_buf = String::with_capacity(4096);
        let mut result = StreamResult::default();

        macro_rules! vision_stream {
            ($client:expr, $is_anthropic:expr) => {{
                let model = $client.completion_model(model_id);
                let mut builder = model.completion_request(message);
                if let Some(mt) = rig_max {
                    builder = builder.max_tokens(mt);
                }
                if let Some(ref params) = token_params {
                    builder = builder.additional_params(params.clone());
                }
                if let Some(sys) = system {
                    builder = builder.preamble(sys.to_string());
                }
                let request = builder.build();
                let stream_start = Instant::now();
                let mut stream = model
                    .stream(request)
                    .await
                    .map_err(|e| RigInferError::PromptError(e.to_string()))?;
                consume_rig_stream(
                    &mut stream,
                    &tx,
                    &mut response_buf,
                    &mut result,
                    $is_anthropic,
                    stream_start,
                )
                .await?;
            }};
        }

        // Use endpoint-specific timeout for OpenAiCompat, default for others
        let effective_timeout = match self {
            RigProvider::OpenAiCompat { timeout_secs, .. } => {
                std::time::Duration::from_secs(*timeout_secs)
            }
            _ => VISION_STREAM_TIMEOUT,
        };

        // Apply overall timeout to prevent slow-drip streams running forever
        timeout(effective_timeout, async {
            match self {
                RigProvider::Claude(client) => vision_stream!(client, true),
                RigProvider::OpenAI(client) => vision_stream!(client, false),
                RigProvider::Mistral(client) => vision_stream!(client, false),
                RigProvider::Groq(client) => vision_stream!(client, false),
                RigProvider::Gemini(client) => vision_stream!(client, false),
                RigProvider::XAi(client) => vision_stream!(client, false),
                RigProvider::OpenAiCompat { client, .. } => vision_stream!(client, false),
                // DeepSeek and Native are handled above via early returns.
                // These arms exist for exhaustiveness in case the early returns are refactored.
                RigProvider::DeepSeek(_) => {
                    return Err(RigInferError::VisionNotSupported(
                        "DeepSeek does not support vision".to_string(),
                    ))
                }
                RigProvider::Mock => {
                    unreachable!(
                        "mock provider generates responses in executor, not via RigProvider"
                    )
                }
                #[cfg(feature = "native-inference")]
                RigProvider::Native(_) => {
                    return Err(RigInferError::VisionNotSupported(
                        "Native provider requires NativeRuntime path for vision".to_string(),
                    ))
                }
            }
            Ok::<(), RigInferError>(())
        })
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: effective_timeout.as_millis() as u64,
        })??;

        result.text = response_buf;
        Ok(result)
    }

    /// Infer with injected tools for structured output enforcement.
    ///
    /// Builds a single-turn agent with the given tools and `tool_choice: Required`.
    /// The LLM is forced to call one of the injected tools, returning structured output
    /// as the tool call arguments. Used by DynamicSubmitTool (Layer 0).
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `tools` - Tools to inject (typically a single DynamicSubmitTool)
    /// * `model` - Optional model override
    /// * `max_tokens` - Optional max tokens for the response (default: 8192)
    ///
    /// # Returns
    /// `(content, prompt_tokens, completion_tokens)` — tokens are non-zero for
    /// OpenAiCompat (from API response), zero for rig-core providers (no access).
    pub async fn infer_with_tools(
        &self,
        prompt: &str,
        tools: Vec<Box<dyn ToolDyn>>,
        model: Option<&str>,
        max_tokens: Option<u32>,
        system: Option<&str>,
    ) -> Result<(String, u64, u64), RigInferError> {
        use rig::agent::AgentBuilder;
        use rig::message::ToolChoice as RigToolChoice;

        /// Maximum time for tool-injection structured output (5 minutes).
        const TOOLS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = model.unwrap_or_else(|| self.default_model());
        let max_tok = max_tokens.map(|v| v.max(16) as u64).unwrap_or(8192);
        let (rig_max, token_params) = token_limit_for_model(self.name(), model_id, max_tok);

        macro_rules! build_agent_with_tools {
            ($client:expr) => {{
                let mut builder = AgentBuilder::new($client.completion_model(model_id))
                    .tools(tools)
                    .tool_choice(RigToolChoice::Required);
                if let Some(mt) = rig_max {
                    builder = builder.max_tokens(mt);
                }
                if let Some(ref params) = token_params {
                    builder = builder.additional_params(params.clone());
                }
                if let Some(sys) = system {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                agent
                    .prompt(prompt)
                    .await
                    .map(|s| (s, 0u64, 0u64)) // rig-core doesn't expose token counts
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }};
        }

        let effective_timeout = match self {
            RigProvider::OpenAiCompat { timeout_secs, .. } => {
                std::time::Duration::from_secs(*timeout_secs)
            }
            _ => TOOLS_TIMEOUT,
        };

        let result = timeout(effective_timeout, async {
            match self {
                RigProvider::OpenAiCompat {
                    raw_base_url,
                    raw_api_key,
                    http_client,
                    ..
                } => {
                    // Bypass rig-core agent.prompt() to avoid deserialization
                    // failures with vLLM/Ollama non-standard response fields.
                    // Convert ToolDyn definitions to OpenAI tool format, send raw
                    // HTTP via raw_chat_completion(), and extract tool_calls.
                    let mut openai_tools = Vec::new();
                    for tool in &tools {
                        let def = tool.definition(String::new()).await;
                        openai_tools.push(serde_json::json!({
                            "type": "function",
                            "function": {
                                "name": def.name,
                                "description": def.description,
                                "parameters": def.parameters,
                            }
                        }));
                    }

                    let mut messages = Vec::new();
                    if let Some(sys) = system {
                        messages.push(serde_json::json!({"role": "system", "content": sys}));
                    }
                    messages.push(serde_json::json!({"role": "user", "content": prompt}));

                    let mut body = serde_json::json!({
                        "model": model_id,
                        "messages": messages,
                        "tools": openai_tools,
                        "tool_choice": "required",
                    });
                    // Apply correct token limit field (max_tokens vs max_completion_tokens)
                    use nika_core::catalogs::capabilities::{model_capabilities, TokenLimitParam};
                    let provider_hint = if raw_base_url.contains("api.openai.com") {
                        "openai"
                    } else if raw_base_url.contains("openrouter.ai") {
                        "openrouter"
                    } else {
                        "custom"
                    };
                    match model_capabilities(provider_hint, model_id).token_limit_param {
                        TokenLimitParam::MaxCompletionTokens => {
                            body["max_completion_tokens"] = serde_json::json!(max_tok);
                        }
                        TokenLimitParam::MaxTokens => {
                            body["max_tokens"] = serde_json::json!(max_tok);
                        }
                    }

                    let (json, prompt_tokens, completion_tokens) = Self::raw_chat_completion(
                        http_client,
                        raw_base_url,
                        raw_api_key,
                        body,
                        effective_timeout,
                    )
                    .await?;

                    // Primary: extract tool call arguments
                    let arguments = json["choices"]
                        .get(0)
                        .and_then(|c| c["message"]["tool_calls"].get(0))
                        .and_then(|tc| tc["function"]["arguments"].as_str())
                        .map(|s| s.to_string());

                    if let Some(args) = arguments {
                        Ok((args, prompt_tokens, completion_tokens))
                    } else {
                        // Fallback: content field (some vLLM models respond
                        // with JSON in content instead of tool calls)
                        json["choices"]
                            .get(0)
                            .and_then(|c| c["message"]["content"].as_str())
                            .map(|s| s.to_string())
                            .map(|s| (s, prompt_tokens, completion_tokens))
                            .ok_or_else(|| {
                                RigInferError::PromptError(
                                    "no tool_calls or content in response".into(),
                                )
                            })
                    }
                }
                RigProvider::Mock => {
                    unreachable!(
                        "mock provider generates responses in executor, not via RigProvider"
                    )
                }
                #[cfg(feature = "native-inference")]
                RigProvider::Native(_) => Err(RigInferError::PromptError(
                    "Native inference does not support tool-based structured output".to_string(),
                )),
                // All rig-core providers
                _ => dispatch_rig!(self, |client| build_agent_with_tools!(client)),
            }
        })
        .await
        .map_err(|_| RigInferError::Timeout {
            duration_ms: effective_timeout.as_millis() as u64,
        })?;
        result
    }

    /// Text completion with full control over LLM parameters
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `options` - LLM control options (model, temperature, max_tokens, system)
    ///
    /// # Returns
    /// The completion text from the model
    ///
    /// # Example
    /// ```ignore
    /// let options = InferOptions {
    ///     temperature: Some(0.7),
    ///     max_tokens: Some(2000),
    ///     system: Some("You are a helpful assistant.".to_string()),
    ///     ..Default::default()
    /// };
    /// let result = provider.infer_with_options("Explain Rust", &options).await?;
    /// ```
    pub async fn infer_with_options(
        &self,
        prompt: &str,
        options: &InferOptions,
    ) -> Result<String, RigInferError> {
        /// Maximum time to wait for an infer_with_options call (5 minutes).
        const OPTIONS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        let model_id = options
            .model
            .as_deref()
            .unwrap_or_else(|| self.default_model());
        // Clamp to 16 minimum — OpenAI rejects < 16, no provider benefits from < 16
        let max_tokens = options.max_tokens.unwrap_or(8192).max(16);

        let effective_temperature = effective_temperature_for_model(model_id, options.temperature);
        let (rig_max, token_params) =
            token_limit_for_model(self.name(), model_id, max_tokens as u64);

        // Use system prompt as preamble (not concatenated into user prompt)
        let user_prompt = prompt.to_string();

        macro_rules! build_and_prompt {
            ($client:expr) => {{
                let mut builder = $client.agent(model_id);
                if let Some(mt) = rig_max {
                    builder = builder.max_tokens(mt);
                }
                if let Some(ref params) = token_params {
                    builder = builder.additional_params(params.clone());
                }
                if let Some(system) = &options.system {
                    builder = builder.preamble(system);
                }
                if let Some(temp) = effective_temperature {
                    builder = builder.temperature(temp);
                }
                if let Some(ref params) = options.additional_params {
                    builder = builder.additional_params(params.clone());
                }
                let agent = builder.build();
                timeout(OPTIONS_TIMEOUT, agent.prompt(&user_prompt))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: OPTIONS_TIMEOUT.as_millis() as u64,
                    })?
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }};
        }

        match self {
            RigProvider::OpenAiCompat {
                raw_base_url,
                raw_api_key,
                timeout_secs,
                http_client,
                ..
            } => {
                let compat_timeout = std::time::Duration::from_secs(*timeout_secs);
                let mut messages = Vec::new();
                if let Some(system) = &options.system {
                    messages.push(serde_json::json!({"role": "system", "content": system}));
                }
                messages.push(serde_json::json!({"role": "user", "content": user_prompt}));
                let (content, prompt_tokens, completion_tokens) = Self::raw_openai_compat_infer(
                    http_client,
                    raw_base_url,
                    raw_api_key,
                    model_id,
                    messages,
                    max_tokens as u64,
                    effective_temperature,
                    compat_timeout,
                )
                .await?;
                tracing::debug!(
                    prompt_tokens,
                    completion_tokens,
                    "OpenAiCompat infer_with_options usage"
                );
                Ok(content)
            }
            RigProvider::Mock => {
                unreachable!("mock provider generates responses in executor, not via RigProvider")
            }
            #[cfg(feature = "native-inference")]
            RigProvider::Native(runtime) => {
                // Native inference uses ChatOptions from native module
                let chat_options = super::super::native::ChatOptions {
                    temperature: effective_temperature.map(|t| t as f32),
                    max_tokens: options.max_tokens,
                    ..Default::default()
                };
                timeout(OPTIONS_TIMEOUT, runtime.infer(&user_prompt, chat_options))
                    .await
                    .map_err(|_| RigInferError::Timeout {
                        duration_ms: OPTIONS_TIMEOUT.as_millis() as u64,
                    })?
                    .map(|r| r.message.content)
                    .map_err(|e: super::super::native::NativeError| {
                        RigInferError::PromptError(e.to_string())
                    })
            }
            // All rig-core providers
            _ => dispatch_rig!(self, |client| build_and_prompt!(client)),
        }
    }
}

// StreamChunk, StreamResult, and consume_rig_stream are in stream.rs

impl RigProvider {
    /// Check if this provider supports native structured output via `response_format: json_schema`.
    ///
    /// Uses the resolved provider type rather than a string name, correctly handling
    /// custom endpoints (OpenAiCompat) which are OpenAI-compatible and support response_format.
    pub fn supports_native_structured_output(&self) -> bool {
        matches!(
            self,
            RigProvider::OpenAI(_)
                | RigProvider::OpenAiCompat { .. }
                | RigProvider::Groq(_)
                | RigProvider::DeepSeek(_)
                | RigProvider::XAi(_)
        )
    }

    /// True only for Anthropic/Claude — controls `is_anthropic` param in
    /// `consume_rig_stream` (thinking block capture, stop_reason mapping).
    pub fn is_anthropic(&self) -> bool {
        matches!(self, RigProvider::Claude(_))
    }

    /// True if this provider supports vision/multimodal content.
    /// Used to give an early, clear error before attempting the call.
    pub fn supports_vision(&self) -> bool {
        !matches!(self, RigProvider::DeepSeek(_) | RigProvider::Mock)
    }

    /// True if extended thinking (chain-of-thought) is supported.
    pub fn supports_thinking(&self) -> bool {
        matches!(self, RigProvider::Claude(_))
    }
}
