//! Verb implementations for TaskExecutor
//!
//! Contains the five verb execution methods:
//! - `run_infer`: LLM text generation
//! - `run_exec`: Shell command execution
//! - `run_fetch`: HTTP requests
//! - `run_invoke`: MCP tool calls
//! - `run_agent`: Multi-turn agentic loops

use futures::FutureExt;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

use crate::ast::output::OutputPolicy;
use crate::ast::{AgentParams, ExecParams, FetchParams, InferParams, InvokeParams};
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::{ContextSource, EventKind};
use crate::mcp::McpClient;
use crate::provider::rig::{InferOptions, StreamChunk};
use crate::runtime::policy::PolicyDecision;
use crate::runtime::{BuiltinToolRouter, InferCallback, RigAgentLoop, StructuredOutputEngine};
use crate::store::RunContext;
use crate::util::{EXEC_TIMEOUT, INVOKE_TASK_DEADLINE};

use base64::Engine;

use super::TaskExecutor;

/// Estimate token count from character length using ceiling division.
///
/// Uses ~4 chars/token heuristic. Ceiling division ensures non-empty strings
/// always produce at least 1 token (fixes off-by-one where `len / 4 == 0`
/// for strings shorter than 4 characters).
#[inline]
fn estimate_tokens(char_len: usize) -> u64 {
    char_len.div_ceil(4) as u64
}

/// Detect image MIME type from magic bytes and return rig-core ImageMediaType.
fn detect_image_media_type(data: &[u8]) -> Option<rig::completion::message::ImageMediaType> {
    use rig::completion::message::ImageMediaType;
    if data.len() < 4 {
        return None;
    }
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some(ImageMediaType::PNG)
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageMediaType::JPEG)
    } else if data.starts_with(b"GIF8") {
        Some(ImageMediaType::GIF)
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(ImageMediaType::WEBP)
    } else {
        None
    }
}

impl TaskExecutor {
    pub(super) async fn run_infer(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
        output_policy: Option<&OutputPolicy>,
    ) -> Result<String, NikaError> {
        // Validate infer params (empty prompt, invalid temperature)
        infer.validate()?;

        // Resolve {{with.alias}} templates
        let mut prompt = template_resolve(&infer.prompt, bindings, datastore)?.into_owned();

        // Validate resolved prompt is not empty (could happen if template resolves to empty)
        // Skip this check when content is present (vision mode — prompt is optional)
        let has_content = infer.content.as_ref().is_some_and(|c| !c.is_empty());
        if prompt.trim().is_empty() && !has_content {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Resolved prompt is empty (task: {}). Check your template bindings.",
                    task_id
                ),
            });
        }

        // Inject JSON schema instruction if output policy requires JSON with schema
        if let Some(schema_instruction) = Self::build_json_schema_instruction(output_policy) {
            prompt.push_str(&schema_instruction);
            debug!(task_id = %task_id, "Injected JSON schema instruction into infer prompt");
        }

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: infer.prompt.clone(),
            result: prompt.to_string(),
        });

        // EMIT: ContextAssembled - capture binding sources used in prompt
        let bindings_value = bindings.to_value();
        let sources: Vec<ContextSource> = bindings_value
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(alias, value)| ContextSource {
                        node: alias.clone(),
                        tokens: estimate_tokens(value.to_string().len()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total_tokens = estimate_tokens(prompt.len());

        self.event_log.emit(EventKind::ContextAssembled {
            task_id: Arc::clone(task_id),
            sources,
            excluded: Vec::new(), // No exclusion logic in simple infer
            total_tokens,
            budget_used_pct: 0.0, // No budget concept in executor
            truncated: false,
        });

        // Use task-level override or workflow default
        let provider_name = infer.provider.as_deref().unwrap_or(&self.default_provider);

        // Mock provider support for testing (no API call)
        // Generates a generic JSON response with common test fields
        if provider_name == "mock" {
            // For vision content, include content metadata in mock response
            let vision_info = if has_content {
                let parts = infer.content.as_ref().unwrap();
                let image_count = parts
                    .iter()
                    .filter(|p| {
                        matches!(
                            p,
                            crate::ast::content::ContentPart::Image { .. }
                                | crate::ast::content::ContentPart::ImageUrl { .. }
                        )
                    })
                    .count();
                let text_count = parts
                    .iter()
                    .filter(|p| matches!(p, crate::ast::content::ContentPart::Text { .. }))
                    .count();
                serde_json::json!({
                    "vision": true,
                    "image_count": image_count,
                    "text_count": text_count,
                    "total_parts": parts.len(),
                })
            } else {
                serde_json::json!({ "vision": false })
            };

            // EMIT: ProviderCalled for mock (consistent with non-mock path)
            self.event_log.emit(EventKind::ProviderCalled {
                task_id: Arc::clone(task_id),
                provider: "mock".to_string(),
                model: infer.model.as_deref().unwrap_or("mock-model").to_string(),
                prompt_len: prompt.len(),
            });

            let mock_response = serde_json::json!({
                "mock": true,
                "task_id": task_id.as_ref(),
                "name": "mock_value",
                "age": 25,
                "value": 42,
                "result": "mock_result",
                "status": "success",
                "message": "Mock response generated",
                "items": ["item1", "item2", "item3"],
                "keywords": ["mock", "test", "nika"],
                "key_phrases": ["mock response", "test workflow"],
                "content": format!("Mock content for task {}", task_id),
                "prompt_len": prompt.len(),
                "vision_info": vision_info,
                "user": {
                    "name": "Mock User",
                    "email": "mock@example.com",
                    "address": {
                        "street": "123 Mock St",
                        "city": "Mockville",
                        "country": "Mockland"
                    }
                },
                "metadata": {
                    "created_at": "2024-01-15T14:30:00Z",
                    "version": 1
                }
            });
            let mock_response_str = mock_response.to_string();
            self.event_log.emit(EventKind::ProviderResponded {
                task_id: Arc::clone(task_id),
                request_id: Some("mock-request".to_string()),
                input_tokens: estimate_tokens(prompt.len()),
                output_tokens: estimate_tokens(mock_response_str.len()),
                cache_read_tokens: 0,
                ttft_ms: Some(0),
                finish_reason: "mock".to_string(),
                cost_usd: 0.0,
            });
            return Ok(mock_response_str);
        }

        // Get cached rig provider
        let provider = self.get_rig_provider(provider_name)?;

        // Resolve model: task override -> workflow default -> provider default
        let model = infer.model.as_deref().or(self.default_model.as_deref());

        // EMIT: ProviderCalled
        self.event_log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: provider_name.to_string(),
            model: model
                .unwrap_or_else(|| provider.default_model())
                .to_string(),
            prompt_len: prompt.len(),
        });

        // POLICY CHECK: token budget
        // Estimate tokens for budget check (actual usage tracked after call)
        let estimated_tokens = estimate_tokens(prompt.len());
        {
            let policy = self.policy_enforcer.read();
            let decision = policy.check_token_spend(estimated_tokens);
            if let PolicyDecision::Block(reason) = decision {
                tracing::warn!(
                    task_id = %task_id,
                    estimated_tokens = estimated_tokens,
                    reason = %reason,
                    "infer: blocked by token budget"
                );
                return Err(NikaError::PolicyViolation { reason });
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // VISION DISPATCH — must run BEFORE Layer 0
        // ═══════════════════════════════════════════════════════════════════
        // Layer 0 uses text-only tool injection which ignores content: parts.
        // Vision must bypass structured output and go directly to infer_vision.
        if has_content {
            return self
                .run_infer_vision(
                    task_id, infer, &prompt, bindings, datastore, &provider, model,
                )
                .await;
        }

        // ═══════════════════════════════════════════════════════════════════
        // LAYER 0: Tool Injection (DynamicSubmitTool)
        // ═══════════════════════════════════════════════════════════════════
        // If structured output is configured, try tool injection first.
        // The LLM is forced to call submit_result() with schema-compliant JSON.
        // If it succeeds, we still validate the result. If it fails, we fall
        // through to streaming + post-processing (Layers 1-3).
        if let Some(policy) = output_policy {
            if policy.is_structured() {
                if let Some(schema_ref) = &policy.schema {
                    // Resolve schema to Value
                    let schema_value = match schema_ref {
                        crate::ast::output::SchemaRef::Inline(v) => Ok(v.clone()),
                        crate::ast::output::SchemaRef::File(path) => {
                            tokio::fs::read_to_string(path)
                                .await
                                .map_err(|e| NikaError::SchemaFailed {
                                    details: format!("Failed to read schema '{}': {}", path, e),
                                })
                                .and_then(|content| {
                                    serde_json::from_str(&content).map_err(|e| {
                                        NikaError::SchemaFailed {
                                            details: format!(
                                                "Invalid JSON in schema '{}': {}",
                                                path, e
                                            ),
                                        }
                                    })
                                })
                        }
                    };

                    if let Err(ref e) = schema_value {
                        warn!(
                            task_id = %task_id,
                            error = %e,
                            "Layer 0: schema resolution failed, skipping tool injection"
                        );
                        self.event_log.emit(EventKind::StructuredOutputAttempt {
                            task_id: Arc::clone(task_id),
                            layer: 0,
                            layer_name: "tool_injection".to_string(),
                            attempt: 0,
                            success: false,
                            error: Some(format!("Schema resolution failed: {}", e)),
                        });
                    }

                    if let Ok(schema_value) = schema_value {
                        let submit_tool =
                            crate::runtime::submit_tool::DynamicSubmitTool::new(schema_value);
                        let tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![Box::new(submit_tool)];

                        debug!(
                            task_id = %task_id,
                            "Layer 0: attempting tool injection via DynamicSubmitTool"
                        );

                        self.event_log.emit(EventKind::StructuredOutputAttempt {
                            task_id: Arc::clone(task_id),
                            layer: 0,
                            layer_name: "tool_injection".to_string(),
                            attempt: 1,
                            success: false, // Will be updated on success
                            error: None,
                        });

                        match provider
                            .infer_with_tools(&prompt, tools, model, infer.max_tokens)
                            .await
                        {
                            Ok(tool_result) => {
                                debug!(
                                    task_id = %task_id,
                                    result_len = tool_result.len(),
                                    "Layer 0: tool injection succeeded"
                                );

                                // Still validate through the engine as safety net
                                if let Some(spec) = policy.to_structured_spec() {
                                    let mut engine = StructuredOutputEngine::new(
                                        spec,
                                        Arc::new(self.event_log.clone()),
                                    );

                                    match engine.validate(task_id.as_ref(), &tool_result).await {
                                        Ok(result) => {
                                            // Emit success ONLY after validation passes
                                            self.event_log.emit(
                                                EventKind::StructuredOutputAttempt {
                                                    task_id: Arc::clone(task_id),
                                                    layer: 0,
                                                    layer_name: "tool_injection".to_string(),
                                                    attempt: 1,
                                                    success: true,
                                                    error: None,
                                                },
                                            );
                                            debug!(
                                                task_id = %task_id,
                                                layer = result.layer,
                                                "Layer 0 + validation succeeded"
                                            );
                                            return Ok(result.value.to_string());
                                        }
                                        Err(e) => {
                                            // Emit failure when validation rejects tool output
                                            self.event_log.emit(
                                                EventKind::StructuredOutputAttempt {
                                                    task_id: Arc::clone(task_id),
                                                    layer: 0,
                                                    layer_name: "tool_injection".to_string(),
                                                    attempt: 1,
                                                    success: false,
                                                    error: Some(e.to_string()),
                                                },
                                            );
                                            debug!(
                                                task_id = %task_id,
                                                error = %e,
                                                "Layer 0 result failed validation, falling through"
                                            );
                                        }
                                    }
                                } else {
                                    // No spec — tool injection result used as-is
                                    self.event_log.emit(EventKind::StructuredOutputAttempt {
                                        task_id: Arc::clone(task_id),
                                        layer: 0,
                                        layer_name: "tool_injection".to_string(),
                                        attempt: 1,
                                        success: true,
                                        error: None,
                                    });
                                    return Ok(tool_result);
                                }
                            }
                            Err(e) => {
                                debug!(
                                    task_id = %task_id,
                                    error = %e,
                                    "Layer 0 failed, falling through to streaming"
                                );
                                self.event_log.emit(EventKind::StructuredOutputAttempt {
                                    task_id: Arc::clone(task_id),
                                    layer: 0,
                                    layer_name: "tool_injection".to_string(),
                                    attempt: 1,
                                    success: false,
                                    error: Some(e.to_string()),
                                });
                                // Fall through to streaming path
                            }
                        }
                    }
                }
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // STREAMING PATH (Layers 1-3 fallback)
        // ═══════════════════════════════════════════════════════════════════
        // Use infer_stream_with_options when LLM control options are set
        // Otherwise fall back to infer_stream.
        // We discard the stream chunks (no TUI display in executor mode) but keep the StreamResult metrics.
        let (tx, _rx) = mpsc::channel::<StreamChunk>(64);
        let has_llm_options =
            infer.temperature.is_some() || infer.max_tokens.is_some() || infer.system.is_some();

        let stream_result = if has_llm_options {
            // Use InferOptions for temperature, max_tokens, system prompt
            let options = InferOptions {
                model: model.map(|s| s.to_string()),
                temperature: infer.temperature,
                max_tokens: infer.max_tokens,
                system: infer.system.clone(),
            };
            provider
                .infer_stream_with_options(&prompt, tx, &options)
                .await
                .map_err(|e| NikaError::ProviderApiError {
                    message: e.to_string(),
                })?
        } else {
            // Fallback: use original infer_stream
            provider
                .infer_stream(&prompt, tx, model)
                .await
                .map_err(|e| NikaError::ProviderApiError {
                    message: e.to_string(),
                })?
        };

        // Record actual token spend
        let actual_tokens = stream_result.input_tokens + stream_result.output_tokens;
        self.policy_enforcer
            .write()
            .record_token_spend(actual_tokens);

        // EMIT: ProviderResponded with accurate token counts and cost from streaming response
        let cost = crate::provider::cost::ProviderKind::parse(provider_name)
            .map(|pk| {
                crate::provider::cost::calculate_cost(
                    pk,
                    model.unwrap_or("default"),
                    stream_result.input_tokens,
                    stream_result.output_tokens,
                )
            })
            .unwrap_or(0.0);
        self.event_log.emit(EventKind::ProviderResponded {
            task_id: Arc::clone(task_id),
            request_id: None,
            input_tokens: stream_result.input_tokens,
            output_tokens: stream_result.output_tokens,
            cache_read_tokens: stream_result.cached_input_tokens,
            ttft_ms: None,
            finish_reason: "stop".to_string(),
            cost_usd: cost,
        });

        // Structured output validation via StructuredOutputEngine (Layers 1-3)
        // If output policy requires JSON with schema, validate and repair the output
        if let Some(policy) = output_policy {
            if policy.is_structured() {
                if let Some(spec) = policy.to_structured_spec() {
                    debug!(
                        task_id = %task_id,
                        "Validating structured output via StructuredOutputEngine (Layers 1-3)"
                    );

                    // Create inference callback for Layer 2 & 3
                    // This allows the engine to actually call the LLM for retries and repairs
                    let infer_callback: InferCallback = {
                        let provider = provider.clone();
                        let model_for_retry = model.map(|s| s.to_string());
                        Arc::new(move |retry_prompt: String| {
                            let provider = provider.clone();
                            let model = model_for_retry.clone();
                            Box::pin(async move {
                                provider
                                    .infer(&retry_prompt, model.as_deref())
                                    .await
                                    .map_err(|e| NikaError::ProviderApiError {
                                        message: format!("structured output retry failed: {}", e),
                                    })
                            })
                        })
                    };

                    let mut engine =
                        StructuredOutputEngine::new(spec, Arc::new(self.event_log.clone()))
                            .with_infer_callback(infer_callback)
                            .with_original_prompt(prompt.to_string());

                    // Validate through defense system (Layers 1-3)
                    let result = engine
                        .validate(task_id.as_ref(), &stream_result.text)
                        .await?;

                    debug!(
                        task_id = %task_id,
                        layer = result.layer,
                        layer_name = %result.layer_name,
                        attempts = result.total_attempts,
                        "Structured output validated successfully"
                    );

                    // Return validated JSON as string
                    return Ok(result.value.to_string());
                }
            }
        }

        Ok(stream_result.text)
    }

    /// Vision inference: resolve content parts, base64-encode CAS images, call provider.
    ///
    /// Dispatched from `run_infer` BEFORE structured output Layer 0 to ensure
    /// vision content parts are never intercepted by text-only tool injection.
    #[allow(clippy::too_many_arguments)]
    async fn run_infer_vision(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        prompt: &str,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
        provider: &crate::provider::rig::RigProvider,
        model: Option<&str>,
    ) -> Result<String, NikaError> {
        const MAX_VISION_IMAGE_PARTS: usize = 20;
        const MAX_VISION_TOTAL_BYTES: u64 = 100 * 1024 * 1024;

        let resolve_start = Instant::now();
        let content = infer
            .content
            .as_ref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "run_infer_vision called without content".to_string(),
            })?;

        let image_part_count = content
            .iter()
            .filter(|p| matches!(p, crate::ast::content::ContentPart::Image { .. }))
            .count();
        if image_part_count > MAX_VISION_IMAGE_PARTS {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Vision content has {} image parts (max {})",
                    image_part_count, MAX_VISION_IMAGE_PARTS
                ),
            });
        }

        let mut user_content: Vec<rig::completion::message::UserContent> = Vec::new();
        let mut image_count: u32 = 0;
        let mut total_bytes: u64 = 0;

        if !prompt.trim().is_empty() {
            user_content.push(rig::completion::message::UserContent::text(prompt));
        }

        for part in content {
            match part {
                crate::ast::content::ContentPart::Text { text } => {
                    let resolved = template_resolve(text, bindings, datastore)?.into_owned();
                    user_content.push(rig::completion::message::UserContent::text(resolved));
                }
                crate::ast::content::ContentPart::Image { source, detail } => {
                    let resolved_source =
                        template_resolve(source, bindings, datastore)?.into_owned();
                    let cas_read = self.cas.read(&resolved_source);
                    let image_data = tokio::select! {
                        result = cas_read => {
                            result.map_err(|e| NikaError::ProviderApiError {
                                message: format!("Vision: CAS read '{}': {}", resolved_source, e),
                            })?
                        }
                        _ = self.cancel_token.cancelled() => {
                            return Err(NikaError::TaskCancelled {
                                task_id: task_id.to_string(),
                                reason: "cancelled during vision CAS read".to_string(),
                            });
                        }
                    };

                    total_bytes += image_data.len() as u64;
                    image_count += 1;

                    if total_bytes > MAX_VISION_TOTAL_BYTES {
                        return Err(NikaError::ValidationError {
                            reason: format!(
                                "Vision content exceeds {} MB",
                                MAX_VISION_TOTAL_BYTES / (1024 * 1024)
                            ),
                        });
                    }

                    let media_type = detect_image_media_type(&image_data);
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&image_data);
                    let rig_detail = Some(match detail {
                        crate::ast::content::ImageDetail::Low => {
                            rig::completion::message::ImageDetail::Low
                        }
                        crate::ast::content::ImageDetail::High => {
                            rig::completion::message::ImageDetail::High
                        }
                        crate::ast::content::ImageDetail::Auto => {
                            rig::completion::message::ImageDetail::Auto
                        }
                    });
                    user_content.push(rig::completion::message::UserContent::image_base64(
                        b64, media_type, rig_detail,
                    ));
                }
                crate::ast::content::ContentPart::ImageUrl { url, detail } => {
                    let resolved_url = template_resolve(url, bindings, datastore)?.into_owned();
                    // SECURITY: SSRF protection
                    if !resolved_url.starts_with("https://") && !resolved_url.starts_with("http://")
                    {
                        return Err(NikaError::ValidationError {
                            reason: format!(
                                "image_url must use http(s)://, got: {}",
                                &resolved_url.chars().take(50).collect::<String>()
                            ),
                        });
                    }
                    let rig_detail = Some(match detail {
                        crate::ast::content::ImageDetail::Low => {
                            rig::completion::message::ImageDetail::Low
                        }
                        crate::ast::content::ImageDetail::High => {
                            rig::completion::message::ImageDetail::High
                        }
                        crate::ast::content::ImageDetail::Auto => {
                            rig::completion::message::ImageDetail::Auto
                        }
                    });
                    user_content.push(rig::completion::message::UserContent::image_url(
                        resolved_url,
                        None,
                        rig_detail,
                    ));
                }
            }
        }

        let resolve_ms = resolve_start.elapsed().as_millis() as u64;

        self.event_log.emit(EventKind::VisionContentResolved {
            task_id: Arc::clone(task_id),
            image_count,
            total_bytes,
            resolve_ms,
        });

        debug!(
            task_id = %task_id,
            image_count,
            total_bytes,
            resolve_ms,
            "Vision content resolved, calling infer_vision"
        );

        let vision_work = provider.infer_vision(
            user_content,
            model,
            infer.system.as_deref(),
            infer.max_tokens,
        );
        let vision_result = tokio::select! {
            result = vision_work => {
                result.map_err(|e| NikaError::ProviderApiError { message: e.to_string() })?
            }
            _ = self.cancel_token.cancelled() => {
                return Err(NikaError::TaskCancelled {
                    task_id: task_id.to_string(),
                    reason: "cancelled during vision inference".to_string(),
                });
            }
        };

        let est_in = estimate_tokens(prompt.len());
        let est_out = estimate_tokens(vision_result.len());
        self.policy_enforcer
            .write()
            .record_token_spend(est_in + est_out);

        self.event_log.emit(EventKind::ProviderResponded {
            task_id: Arc::clone(task_id),
            request_id: None,
            input_tokens: est_in,
            output_tokens: est_out,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: "stop".to_string(),
            cost_usd: 0.0,
        });

        Ok(vision_result)
    }

    pub(super) async fn run_exec(
        &self,
        task_id: &Arc<str>,
        params: &ExecParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<String, NikaError> {
        // Resolve {{with.alias}} templates
        // Note: Shell escaping is NOT applied by default.
        // For values that need shell escaping, use {{with.alias|shell}} syntax.
        let resolved_cmd = template_resolve(&params.command, bindings, datastore)?;

        // SECURITY CHECK: validate command for control characters and blocklist
        crate::runtime::security::validate_exec_command(&resolved_cmd)?;

        // POLICY CHECK: exec verb
        let policy_decision = self.policy_enforcer.read().check_exec(&resolved_cmd);
        if let PolicyDecision::Block(reason) = policy_decision {
            tracing::warn!(
                task_id = %task_id,
                command = %resolved_cmd,
                reason = %reason,
                "exec: blocked by policy"
            );
            return Err(NikaError::PolicyViolation { reason });
        }

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: params.command.clone(),
            result: resolved_cmd.to_string(),
        });

        // Use per-task timeout if specified, otherwise fall back to global default
        let exec_deadline = params
            .timeout
            .map(std::time::Duration::from_secs)
            .unwrap_or(EXEC_TIMEOUT);

        // Shell-free execution by default, opt-in to shell mode
        // Support for env vars
        let output = if params.shell == Some(true) {
            // Shell mode: use sh -c (preserves shell metacharacters like ;, |, &&)
            tracing::debug!(task_id = %task_id, "exec: using shell mode (sh -c)");
            let mut cmd = tokio::process::Command::new("sh");
            cmd.arg("-c").arg(resolved_cmd.as_ref());

            // Strip sensitive env vars from child process
            crate::runtime::security::strip_sensitive_env_vars(&mut cmd);

            // Set working directory if specified
            if let Some(ref cwd) = params.cwd {
                cmd.current_dir(cwd);
            }

            // Add environment variables if specified (validate first)
            if let Some(ref env_vars) = params.env {
                let pairs: Vec<(String, String)> = env_vars
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                crate::runtime::security::validate_env_vars(&pairs)?;
                for (key, value) in env_vars {
                    let resolved_value = template_resolve(value, bindings, datastore)?;
                    cmd.env(key, resolved_value.as_ref());
                }
            }

            tokio::time::timeout(exec_deadline, cmd.output())
                .await
                .map_err(|_| {
                    NikaError::Execution(format!(
                        "Command timed out after {}s",
                        exec_deadline.as_secs()
                    ))
                })?
                .map_err(|e| NikaError::Execution(format!("Failed to execute command: {}", e)))?
        } else {
            // Shell-free mode (default): parse with shlex, execute directly
            tracing::debug!(task_id = %task_id, "exec: using shell-free mode (shlex)");
            let parts = shlex::split(&resolved_cmd).ok_or_else(|| {
                NikaError::Execution(format!(
                    "Failed to parse command (unbalanced quotes?): {}",
                    resolved_cmd
                ))
            })?;

            if parts.is_empty() {
                return Err(NikaError::Execution("Empty command".to_string()));
            }

            let mut cmd = tokio::process::Command::new(&parts[0]);
            cmd.args(&parts[1..]);

            // Strip sensitive env vars from child process
            crate::runtime::security::strip_sensitive_env_vars(&mut cmd);

            // Set working directory if specified
            if let Some(ref cwd) = params.cwd {
                cmd.current_dir(cwd);
            }

            // Add environment variables if specified (validate first)
            if let Some(ref env_vars) = params.env {
                let pairs: Vec<(String, String)> = env_vars
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                crate::runtime::security::validate_env_vars(&pairs)?;
                for (key, value) in env_vars {
                    let resolved_value = template_resolve(value, bindings, datastore)?;
                    cmd.env(key, resolved_value.as_ref());
                }
            }

            tokio::time::timeout(exec_deadline, cmd.output())
                .await
                .map_err(|_| {
                    NikaError::Execution(format!(
                        "Command timed out after {}s",
                        exec_deadline.as_secs()
                    ))
                })?
                .map_err(|e| NikaError::Execution(format!("Failed to execute command: {}", e)))?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NikaError::Execution(format!("Command failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[instrument(skip(self, bindings, datastore), fields(url = %fetch.url))]
    pub(super) async fn run_fetch(
        &self,
        task_id: &Arc<str>,
        fetch: &FetchParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<String, NikaError> {
        // Resolve {{with.alias}} templates
        let url = template_resolve(&fetch.url, bindings, datastore)?;

        // POLICY CHECK: fetch verb
        let policy_decision = self.policy_enforcer.read().check_fetch(&url);
        if let PolicyDecision::Block(reason) = policy_decision {
            tracing::warn!(
                task_id = %task_id,
                url = %url,
                reason = %reason,
                "fetch: blocked by policy"
            );
            return Err(NikaError::PolicyViolation { reason });
        }

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: fetch.url.clone(),
            result: url.to_string(),
        });

        // Select HTTP client based on follow_redirects setting
        // Default behavior (None or Some(true)) uses the shared client with redirects enabled
        // When follow_redirects = false, create a one-off client without redirect following
        let http_client: std::borrow::Cow<'_, reqwest::Client> =
            if fetch.follow_redirects == Some(false) {
                tracing::debug!(
                    task_id = %task_id,
                    "fetch: using no-redirect client (follow_redirects=false)"
                );
                std::borrow::Cow::Owned(
                    reqwest::Client::builder()
                        .timeout(crate::util::FETCH_TIMEOUT)
                        .connect_timeout(crate::util::CONNECT_TIMEOUT)
                        .redirect(reqwest::redirect::Policy::none())
                        .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")))
                        .build()
                        .map_err(|e| NikaError::ProviderApiError {
                            message: format!("HTTP client build failed: {e}"),
                        })?,
                )
            } else {
                std::borrow::Cow::Borrowed(&self.http_client)
            };

        // Build request based on HTTP method
        let mut request = if fetch.method.eq_ignore_ascii_case("POST") {
            http_client.post(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("PUT") {
            http_client.put(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("DELETE") {
            http_client.delete(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("PATCH") {
            http_client.patch(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("HEAD") {
            http_client.head(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("OPTIONS") {
            http_client.request(reqwest::Method::OPTIONS, url.as_ref())
        } else {
            http_client.get(url.as_ref()) // Default to GET
        };

        // Add headers
        for (key, value) in &fetch.headers {
            let resolved_value = template_resolve(value, bindings, datastore)?;
            request = request.header(key, resolved_value.as_ref());
        }

        // Handle json field - takes precedence over body
        // Auto-serializes to JSON string and sets Content-Type: application/json
        if let Some(ref json_value) = fetch.json {
            // Serialize JSON value to string
            let json_body =
                serde_json::to_string(json_value).map_err(|e| NikaError::InvalidJson {
                    details: format!("Failed to serialize json body: {e}"),
                })?;

            // Set Content-Type header if not already set
            if !fetch
                .headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("content-type"))
            {
                request = request.header("Content-Type", "application/json");
            }

            request = request.body(json_body);
        } else if let Some(body) = &fetch.body {
            // Add body if present (only if json not set)
            let resolved_body = template_resolve(body, bindings, datastore)?;
            request = request.body(resolved_body.into_owned());
        }

        // Apply per-request timeout if specified (overrides client default)
        if let Some(timeout_secs) = fetch.timeout {
            request = request.timeout(std::time::Duration::from_secs(timeout_secs));
        }

        // Retry configuration
        let max_attempts = fetch.retry.as_ref().map_or(1, |r| r.max_attempts.max(1));
        let backoff_ms = fetch.retry.as_ref().map_or(1000, |r| r.backoff_ms);
        let multiplier = fetch.retry.as_ref().map_or(2.0, |r| r.multiplier);

        // Check if request can be cloned (required for retry)
        let can_retry = request.try_clone().is_some();
        if !can_retry && max_attempts > 1 {
            tracing::debug!(
                task_id = %task_id,
                "fetch: retry disabled (request body cannot be cloned)"
            );
        }

        let effective_max_attempts = if can_retry { max_attempts } else { 1 };
        let mut last_error: Option<NikaError> = None;
        let mut current_request = Some(request);

        for attempt in 1..=effective_max_attempts {
            // Get the request for this attempt
            let req = if attempt == 1 {
                // First attempt: use the original request
                current_request
                    .take()
                    .expect("request should exist on first attempt")
            } else {
                // Subsequent attempts: we already verified we can clone
                // The original request was moved, but we stored a clone
                current_request.take().expect("cloned request should exist")
            };

            // Clone for potential next retry (before sending consumes the request)
            if attempt < effective_max_attempts {
                current_request = req.try_clone();
            }

            match req.send().await {
                Ok(response) => {
                    // Check for server errors that should be retried
                    if response.status().is_server_error() && attempt < effective_max_attempts {
                        let status = response.status();
                        tracing::warn!(
                            task_id = %task_id,
                            attempt = attempt,
                            status = %status,
                            "fetch: server error, retrying..."
                        );
                        last_error = Some(NikaError::Execution(format!(
                            "HTTP server error: {}",
                            status
                        )));

                        // Exponential backoff
                        let delay_ms = backoff_ms * (multiplier.powi((attempt - 1) as i32) as u64);
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        continue;
                    }

                    // Success or non-retryable error status

                    // Check response mode BEFORE consuming the body
                    if fetch.response.as_deref() == Some("full") {
                        let status = response.status().as_u16();
                        let headers: serde_json::Map<String, serde_json::Value> = response
                            .headers()
                            .iter()
                            .map(|(k, v)| {
                                (
                                    k.to_string(),
                                    serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
                                )
                            })
                            .collect();
                        let final_url = response.url().to_string();
                        let body = response.text().await.map_err(|e| {
                            NikaError::Execution(format!("Failed to read response: {}", e))
                        })?;
                        return Ok(serde_json::json!({
                            "status": status,
                            "headers": headers,
                            "body": body,
                            "url": final_url,
                        })
                        .to_string());
                    }

                    const MAX_RESPONSE_SIZE: u64 = 50 * 1024 * 1024;
                    if let Some(len) = response.content_length() {
                        if len > MAX_RESPONSE_SIZE {
                            return Err(NikaError::Execution(format!(
                                "Response too large ({} bytes, max {} bytes)",
                                len, MAX_RESPONSE_SIZE
                            )));
                        }
                    }
                    let raw_body = response.text().await.map_err(|e| {
                        NikaError::Execution(format!("Failed to read response: {}", e))
                    })?;
                    if raw_body.len() as u64 > MAX_RESPONSE_SIZE {
                        return Err(NikaError::Execution(format!(
                            "Response body too large ({} bytes, max {} bytes)",
                            raw_body.len(),
                            MAX_RESPONSE_SIZE
                        )));
                    }
                    return Ok(raw_body);
                }
                Err(e) => {
                    // Network errors are retryable
                    if attempt < effective_max_attempts {
                        tracing::warn!(
                            task_id = %task_id,
                            attempt = attempt,
                            error = %e,
                            "fetch: request failed, retrying..."
                        );
                        last_error =
                            Some(NikaError::Execution(format!("HTTP request failed: {}", e)));

                        // Exponential backoff
                        let delay_ms = backoff_ms * (multiplier.powi((attempt - 1) as i32) as u64);
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        continue;
                    }

                    return Err(NikaError::Execution(format!(
                        "HTTP request failed after {} attempts: {}",
                        effective_max_attempts, e
                    )));
                }
            }
        }

        // Should not reach here, but just in case
        Err(last_error.unwrap_or_else(|| {
            NikaError::Execution("HTTP request failed: unknown error".to_string())
        }))
    }

    /// Execute an invoke action (MCP tool call or resource read)
    ///
    /// # Template Resolution
    ///
    /// Templates like `{{with.variable}}` in params are resolved before calling the MCP tool.
    /// This enables for_each iterations to pass dynamic values to MCP tools.
    #[instrument(skip(self, bindings, datastore), fields(mcp = ?invoke.mcp))]
    pub(super) async fn run_invoke(
        &self,
        task_id: &Arc<str>,
        invoke: &InvokeParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<String, NikaError> {
        // Validate invoke params (tool XOR resource)
        invoke.validate()?;

        // Generate unique call_id for correlation
        let call_id = Uuid::new_v4().to_string();
        let start_time = Instant::now();

        // Resolve templates FIRST, then emit event with resolved params
        // This fixes the bug where TUI showed literal {{with.topic}} instead of resolved values
        let resolved_params = if let Some(ref original_params) = invoke.params {
            let params_str = serde_json::to_string(original_params)
                .map_err(|e| NikaError::Execution(format!("Failed to serialize params: {}", e)))?;
            let resolved_str = template_resolve(&params_str, bindings, datastore)?;
            Some(
                serde_json::from_str::<serde_json::Value>(&resolved_str).map_err(|e| {
                    NikaError::Execution(format!(
                        "Failed to parse resolved params '{}': {}",
                        resolved_str, e
                    ))
                })?,
            )
        } else {
            None
        };

        // EMIT: McpInvoke event (with RESOLVED params for TUI display)
        // For builtin tools, mcp is None - use "builtin" as server name
        let mcp_server = invoke.mcp.clone().unwrap_or_else(|| "builtin".to_string());
        self.event_log.emit(EventKind::McpInvoke {
            task_id: Arc::clone(task_id),
            call_id: call_id.clone(),
            mcp_server,
            tool: invoke.tool.clone(),
            resource: invoke.resource.clone(),
            params: resolved_params.clone(),
        });

        // Check for builtin nika_* tools
        if let Some(tool) = &invoke.tool {
            if BuiltinToolRouter::is_builtin(tool) {
                // Use already-resolved params
                let params = resolved_params
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".to_string());

                // Dispatch to builtin router
                let dispatch_result = self.builtin_router.dispatch(tool, params).await;
                let duration_ms = start_time.elapsed().as_millis() as u64;

                match dispatch_result {
                    Ok(result) => {
                        let result_value: serde_json::Value = serde_json::from_str(&result)
                            .unwrap_or_else(|_| serde_json::Value::String(result.clone()));

                        // EMIT: McpResponse event for builtin tool (success)
                        self.event_log.emit(EventKind::McpResponse {
                            task_id: Arc::clone(task_id),
                            call_id,
                            output_len: result.len(),
                            duration_ms,
                            cached: false,
                            is_error: false,
                            response: Some(result_value.clone()),
                        });

                        return Ok(result_value.to_string());
                    }
                    Err(e) => {
                        // EMIT: McpResponse event for builtin tool (error)
                        self.event_log.emit(EventKind::McpResponse {
                            task_id: Arc::clone(task_id),
                            call_id,
                            output_len: 0,
                            duration_ms,
                            cached: false,
                            is_error: true,
                            response: Some(serde_json::json!({"error": e.to_string()})),
                        });

                        return Err(e);
                    }
                }
            }
        }

        // Get or create MCP client (real or mock depending on config)
        // Mcp is Option<String>, validate() guarantees Some for non-builtin tools
        let mcp_name = invoke
            .mcp
            .as_ref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "MCP server name required for non-builtin tools".to_string(),
            })?;

        // Race MCP work against both deadline timeout AND cancellation token.
        // This ensures MCP calls abort promptly on workflow cancellation instead of
        // waiting up to INVOKE_TASK_DEADLINE (5 min).
        let mcp_work = async {
            let client = self.get_mcp_client(mcp_name).await?;

            let is_error = false;
            let result = if let Some(tool) = &invoke.tool {
                // Tool call path - use already-resolved params
                let params = resolved_params.clone().unwrap_or(serde_json::Value::Null);
                // Use call_tool_with_retry_events for McpRetry event emission
                let tool_result = client
                    .call_tool_with_retry_events(tool, params, task_id, &self.event_log)
                    .await?;

                // Check if tool returned an error
                if tool_result.is_error {
                    // Emit response event before returning error
                    let duration_ms = start_time.elapsed().as_millis() as u64;
                    let error_text = tool_result.text();
                    self.event_log.emit(EventKind::McpResponse {
                        task_id: Arc::clone(task_id),
                        call_id: call_id.clone(),
                        output_len: error_text.len(),
                        duration_ms,
                        cached: false,
                        is_error: true,
                        response: Some(serde_json::json!({"error": error_text.clone()})),
                    });
                    return Err(NikaError::McpToolError {
                        tool: tool.clone(),
                        reason: error_text,
                        error_code: None,
                    });
                }

                // Process media content (if any)
                if tool_result.has_media() {
                    use crate::mcp::types::ContentBlock;
                    use crate::media::{CasStore, MediaProcessor};

                    let media_blocks = tool_result.media_blocks();
                    let content_types: Vec<String> = media_blocks
                        .iter()
                        .map(|b| match b {
                            ContentBlock::Text { .. } => unreachable!("media_blocks filters text"),
                            ContentBlock::Image { .. } => "image".to_string(),
                            ContentBlock::Audio { .. } => "audio".to_string(),
                            ContentBlock::Resource(_) => "resource".to_string(),
                            ContentBlock::ResourceLink { .. } => "resource_link".to_string(),
                        })
                        .collect();

                    self.event_log.emit(EventKind::MediaExtracted {
                        task_id: Arc::clone(task_id),
                        block_count: media_blocks.len() as u32,
                        content_types,
                    });

                    let workspace_root = datastore.workspace_root();
                    let store = CasStore::workspace_default(&workspace_root);
                    // Use shared per-run budget from RunContext (not a fresh one per invoke)
                    let processor = MediaProcessor::with_shared_budget(
                        store,
                        std::sync::Arc::clone(datastore.media_budget()),
                    );

                    let process_results = processor
                        .process_all(&tool_result.content, task_id.as_ref())
                        .await;

                    // Process all results: emit events for EVERY block first,
                    // then fail on non-recoverable errors. This ensures the trace
                    // is complete even when the task fails partway through.
                    let mut media_refs = Vec::new();
                    let mut fatal_error: Option<crate::media::MediaError> = None;
                    for result in process_results {
                        match result {
                            Ok((media_ref, store_result)) => {
                                self.event_log.emit(EventKind::MediaProcessed {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    mime_type: media_ref.mime_type.clone(),
                                    size_bytes: media_ref.size_bytes,
                                });
                                self.event_log.emit(EventKind::MediaStored {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    path: media_ref.path.display().to_string(),
                                    size_bytes: media_ref.size_bytes,
                                    verified: store_result.verified,
                                    deduplicated: store_result.deduplicated,
                                    pipeline_ms: store_result.pipeline_ms,
                                });
                                media_refs.push(media_ref);
                            }
                            Err((block_index, error)) => {
                                self.event_log.emit(EventKind::MediaStoreFailed {
                                    task_id: Arc::clone(task_id),
                                    hash: String::new(),
                                    reason: format!("block {block_index}: {error}"),
                                });
                                // Capture first non-recoverable error for task failure
                                if fatal_error.is_none() && !error.is_recoverable() {
                                    fatal_error = Some(error);
                                }
                            }
                        }
                    }

                    // If any non-recoverable error occurred, fail the task
                    if let Some(error) = fatal_error {
                        return Err(NikaError::MediaError(error));
                    }

                    // Stage media refs in side-channel for runner to pick up
                    datastore.set_media(task_id, media_refs);
                }

                // Text output flows as before (backward compat)
                let text = tool_result.text();
                serde_json::from_str(&text).unwrap_or_else(|_| {
                    tracing::trace!(task = %task_id, "MCP tool returned non-JSON text, wrapping as string");
                    serde_json::Value::String(text)
                })
            } else if let Some(resource) = &invoke.resource {
                // Resource read path — now handles blob data via media pipeline
                let content = client.read_resource(resource).await?;

                // If resource has a blob, process it through the media pipeline
                if let Some(blob) = &content.blob {
                    use crate::mcp::types::ContentBlock;
                    use crate::media::{CasStore, MediaProcessor};

                    let mime = content
                        .mime_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".to_string());
                    let block = ContentBlock::Resource(
                        crate::mcp::types::ResourceContent::new(resource.clone())
                            .with_blob(blob.clone())
                            .with_optional_mime(content.mime_type.clone()),
                    );

                    tracing::debug!(
                        task_id = %task_id,
                        resource = %resource,
                        mime = %mime,
                        blob_len = blob.len(),
                        "Resource read returned blob data, processing through media pipeline"
                    );

                    self.event_log.emit(EventKind::MediaExtracted {
                        task_id: Arc::clone(task_id),
                        block_count: 1,
                        content_types: vec!["resource_blob".to_string()],
                    });

                    let workspace_root = datastore.workspace_root();
                    let store = CasStore::workspace_default(&workspace_root);
                    let processor = MediaProcessor::with_shared_budget(
                        store,
                        std::sync::Arc::clone(datastore.media_budget()),
                    );
                    let results = processor.process_all(&[block], task_id.as_ref()).await;
                    let mut media_refs = Vec::new();
                    for result in results {
                        match result {
                            Ok((media_ref, store_result)) => {
                                self.event_log.emit(EventKind::MediaProcessed {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    mime_type: media_ref.mime_type.clone(),
                                    size_bytes: media_ref.size_bytes,
                                });
                                self.event_log.emit(EventKind::MediaStored {
                                    task_id: Arc::clone(task_id),
                                    hash: media_ref.hash.clone(),
                                    path: media_ref.path.display().to_string(),
                                    size_bytes: media_ref.size_bytes,
                                    verified: store_result.verified,
                                    deduplicated: store_result.deduplicated,
                                    pipeline_ms: store_result.pipeline_ms,
                                });
                                media_refs.push(media_ref);
                            }
                            Err((idx, error)) => {
                                tracing::warn!(task_id = %task_id, error = %error, "Resource blob media processing failed");
                                self.event_log.emit(EventKind::MediaStoreFailed {
                                    task_id: Arc::clone(task_id),
                                    hash: format!("blob_index:{}", idx),
                                    reason: error.to_string(),
                                });
                            }
                        }
                    }
                    if !media_refs.is_empty() {
                        datastore.set_media(task_id, media_refs);
                    }
                }

                // Text output (if any)
                content
                    .text
                    .map(|t| {
                        serde_json::from_str(&t).unwrap_or_else(|_| {
                            tracing::trace!(task = %task_id, "MCP resource returned non-JSON text, wrapping as string");
                            serde_json::Value::String(t)
                        })
                    })
                    .unwrap_or(serde_json::Value::Null)
            } else {
                return Err(NikaError::Execution(
                    "invoke: task requires either 'tool' or 'resource' field".to_string(),
                ));
            };

            Ok::<(serde_json::Value, bool, Arc<McpClient>), NikaError>((result, is_error, client))
        };

        // Use per-task timeout if specified, otherwise fall back to global deadline
        let deadline = invoke
            .timeout
            .map(std::time::Duration::from_secs)
            .unwrap_or(INVOKE_TASK_DEADLINE);

        let mcp_result = tokio::select! {
            result = tokio::time::timeout(deadline, mcp_work) => {
                result.map_err(|_| NikaError::McpTimeout {
                    name: mcp_name.clone(),
                    operation: format!("invoke task (deadline {}s)", deadline.as_secs()),
                    timeout_secs: deadline.as_secs(),
                })?
            }
            _ = self.cancel_token.cancelled() => {
                Err(NikaError::TaskCancelled {
                    task_id: task_id.to_string(),
                    reason: "workflow cancelled during MCP invoke".to_string(),
                })
            }
        }?;

        let (result, is_error, client) = mcp_result;

        // EMIT: McpResponse event (with full response for TUI display)
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.event_log.emit(EventKind::McpResponse {
            task_id: Arc::clone(task_id),
            call_id,
            output_len: result.to_string().len(),
            duration_ms,
            cached: client.was_last_call_cached(),
            is_error,
            response: Some(result.clone()),
        });

        // Return JSON string representation
        Ok(result.to_string())
    }

    /// Execute an agent action (agentic execution with tool calling loop)
    #[instrument(skip(self, bindings, datastore, output_policy), fields(max_turns = %agent.effective_max_turns()))]
    pub(super) async fn run_agent(
        &self,
        task_id: &Arc<str>,
        agent: &AgentParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
        output_policy: Option<&OutputPolicy>,
    ) -> Result<String, NikaError> {
        // Resolve {{with.alias}} templates in prompt
        let mut resolved_prompt =
            template_resolve(&agent.prompt, bindings, datastore)?.into_owned();

        // Inject JSON schema instruction if output policy requires JSON with schema
        if let Some(schema_instruction) = Self::build_json_schema_instruction(output_policy) {
            resolved_prompt.push_str(&schema_instruction);
            debug!(task_id = %task_id, "Injected JSON schema instruction into agent prompt");
        }

        // EMIT: TemplateResolved event
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: agent.prompt.clone(),
            result: resolved_prompt.to_string(),
        });

        // Create agent params with resolved prompt
        let resolved_agent = AgentParams {
            prompt: resolved_prompt,
            ..agent.clone()
        };

        // Validate agent params
        resolved_agent.validate()?;

        // POLICY CHECK: token budget
        // Estimate tokens for budget check - use token_budget from agent params if set,
        // otherwise estimate from prompt length
        let estimated_tokens: u64 = resolved_agent
            .token_budget
            .map(u64::from)
            .unwrap_or_else(|| estimate_tokens(resolved_agent.prompt.len()) as u64);
        {
            let policy = self.policy_enforcer.read();
            let decision = policy.check_token_spend(estimated_tokens);
            if let PolicyDecision::Block(reason) = decision {
                tracing::warn!(
                    task_id = %task_id,
                    estimated_tokens = estimated_tokens,
                    reason = %reason,
                    "agent: blocked by token budget"
                );
                return Err(NikaError::PolicyViolation { reason });
            }
        }

        // EMIT: AgentStart event
        self.event_log.emit(EventKind::AgentStart {
            task_id: Arc::clone(task_id),
            max_turns: resolved_agent.effective_max_turns(),
            mcp_servers: resolved_agent.mcp.clone(),
        });

        // Get provider name (task override or workflow default)
        // Clone to avoid borrow conflict when moving resolved_agent into RigAgentLoop
        let provider_name: String = resolved_agent
            .provider
            .clone()
            .unwrap_or_else(|| self.default_provider.to_string());

        // Ensure resolved_agent has the provider set for run_auto() dispatch
        let resolved_agent = AgentParams {
            provider: Some(provider_name.clone()),
            ..resolved_agent
        };

        // Build MCP client map for this agent
        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        for mcp_name in &resolved_agent.mcp {
            let client = self.get_mcp_client(mcp_name).await?;
            mcp_clients.insert(mcp_name.clone(), client);
        }

        // Create rig-based agent loop
        let agent_loop = RigAgentLoop::new(
            task_id.to_string(),
            resolved_agent,
            self.event_log.clone(),
            mcp_clients,
        )?;

        // Inject DynamicSubmitTool if structured output is configured.
        // For agent verb, submit_result is available but NOT forced —
        // the agent calls it when ready (unlike infer: which uses tool_choice: Required).
        let mut agent_loop = if let Some(policy) = output_policy {
            if policy.is_structured() {
                if let Some(schema_ref) = &policy.schema {
                    let schema_value = match schema_ref {
                        crate::ast::output::SchemaRef::Inline(v) => Some(v.clone()),
                        crate::ast::output::SchemaRef::File(path) => {
                            match tokio::fs::read_to_string(path).await {
                                Ok(content) => match serde_json::from_str(&content) {
                                    Ok(v) => Some(v),
                                    Err(e) => {
                                        warn!(
                                            task_id = %task_id,
                                            path = %path,
                                            error = %e,
                                            "Agent: invalid JSON in schema file, skipping tool injection"
                                        );
                                        None
                                    }
                                },
                                Err(e) => {
                                    warn!(
                                        task_id = %task_id,
                                        path = %path,
                                        error = %e,
                                        "Agent: failed to read schema file, skipping tool injection"
                                    );
                                    None
                                }
                            }
                        }
                    };
                    if let Some(schema) = schema_value {
                        debug!(
                            task_id = %task_id,
                            "Agent: injecting DynamicSubmitTool for structured output"
                        );
                        agent_loop.with_structured_output(schema)
                    } else {
                        agent_loop
                    }
                } else {
                    agent_loop
                }
            } else {
                agent_loop
            }
        } else {
            agent_loop
        };

        let start = std::time::Instant::now();

        // Run agent with appropriate provider
        // mock provider uses run_mock(), real providers use run_auto() which dispatches
        // based on AgentParams.provider (claude/openai)
        let result = if provider_name.as_str() == "mock" {
            agent_loop.run_mock().await?
        } else {
            // Use run_auto() which dispatches to run_claude() or run_openai()
            // based on the provider field we just set.
            // Wrap in catch_unwind to convert rig-core panics to NikaErrors.
            let run_future = agent_loop.run_auto();
            match std::panic::AssertUnwindSafe(run_future)
                .catch_unwind()
                .await
            {
                Ok(result) => result?,
                Err(panic_info) => {
                    let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic in agent execution".to_string()
                    };
                    tracing::error!(
                        task_id = %task_id,
                        panic_message = %msg,
                        "Agent execution panicked (likely rig-core internal error)"
                    );
                    return Err(NikaError::AgentExecutionError {
                        task_id: task_id.to_string(),
                        reason: format!("Agent panicked: {}", msg),
                    });
                }
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        // Record actual token spend
        self.policy_enforcer
            .write()
            .record_token_spend(result.total_tokens as u64);

        // EMIT: AgentComplete event
        self.event_log.emit(EventKind::AgentComplete {
            task_id: Arc::clone(task_id),
            turns: result.turns as u32,
            stop_reason: format!("{:?}", result.status),
        });

        tracing::info!(
            task_id = %task_id,
            turns = result.turns,
            status = ?result.status,
            tokens = result.total_tokens,
            duration_ms = duration_ms,
            "Agent loop completed"
        );

        // Process any media content staged during agent tool calls (H1 side-channel)
        let staged_media = agent_loop.drain_media();
        if !staged_media.is_empty() {
            tracing::info!(
                task_id = %task_id,
                block_count = staged_media.len(),
                "agent: processing staged media from tool calls"
            );

            use crate::media::{CasStore, MediaProcessor};

            self.event_log.emit(EventKind::MediaExtracted {
                task_id: Arc::clone(task_id),
                block_count: staged_media.len() as u32,
                content_types: staged_media
                    .iter()
                    .map(|b| match b {
                        crate::mcp::types::ContentBlock::Image { .. } => "image".to_string(),
                        crate::mcp::types::ContentBlock::Audio { .. } => "audio".to_string(),
                        crate::mcp::types::ContentBlock::Resource(_) => "resource".to_string(),
                        crate::mcp::types::ContentBlock::ResourceLink { .. } => {
                            "resource_link".to_string()
                        }
                        crate::mcp::types::ContentBlock::Text { .. } => "text".to_string(),
                    })
                    .collect(),
            });

            let workspace_root = datastore.workspace_root();
            let store = CasStore::workspace_default(&workspace_root);
            let processor = MediaProcessor::with_shared_budget(
                store,
                std::sync::Arc::clone(datastore.media_budget()),
            );

            let process_results = processor.process_all(&staged_media, task_id.as_ref()).await;
            let mut media_refs = Vec::new();
            for result in process_results {
                match result {
                    Ok((media_ref, store_result)) => {
                        self.event_log.emit(EventKind::MediaProcessed {
                            task_id: Arc::clone(task_id),
                            hash: media_ref.hash.clone(),
                            mime_type: media_ref.mime_type.clone(),
                            size_bytes: media_ref.size_bytes,
                        });
                        self.event_log.emit(EventKind::MediaStored {
                            task_id: Arc::clone(task_id),
                            hash: media_ref.hash.clone(),
                            path: media_ref.path.display().to_string(),
                            size_bytes: media_ref.size_bytes,
                            verified: store_result.verified,
                            deduplicated: store_result.deduplicated,
                            pipeline_ms: store_result.pipeline_ms,
                        });
                        media_refs.push(media_ref);
                    }
                    Err((block_index, error)) => {
                        self.event_log.emit(EventKind::MediaStoreFailed {
                            task_id: Arc::clone(task_id),
                            hash: String::new(),
                            reason: format!("agent block {block_index}: {error}"),
                        });
                        // For agent media, we log but don't fail the task
                        // (the agent already completed successfully)
                        tracing::warn!(
                            task_id = %task_id,
                            block_index = block_index,
                            error = %error,
                            "agent: media processing failed for staged block"
                        );
                    }
                }
            }
            if !media_refs.is_empty() {
                datastore.set_media(task_id, media_refs);
            }
        }

        // Telemetry: log non-string response types for observability
        if let Some(v) = result.final_output.get("response") {
            if !v.is_string() {
                tracing::debug!(
                    task_id = %task_id,
                    response_type = match v {
                        serde_json::Value::Object(_) => "object",
                        serde_json::Value::Array(_) => "array",
                        serde_json::Value::Number(_) => "number",
                        serde_json::Value::Bool(_) => "boolean",
                        serde_json::Value::Null => "null",
                        _ => "unknown",
                    },
                    "Agent response is non-string JSON, serializing"
                );
            }
        }

        // Extract response from final_output wrapper
        // Agent returns {"response": <value>} — value may be a string, object, array, number, etc.
        // Strings are returned as-is (no extra quotes), other types are serialized to JSON.
        // If "response" key is missing, fall back to the entire final_output.
        let response = match result.final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => {
                // No "response" key — serialize the entire final_output if non-empty
                if result.final_output.is_object()
                    && result.final_output.as_object().is_none_or(|o| o.is_empty())
                {
                    tracing::warn!(
                        task_id = %task_id,
                        "Agent returned empty final_output, using empty string"
                    );
                    String::new()
                } else {
                    tracing::debug!(
                        task_id = %task_id,
                        "Agent final_output missing 'response' key, using full output"
                    );
                    result.final_output.to_string()
                }
            }
        };

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn resource_text_non_json_returns_string() {
        let text = "Hello, this is plain text from a resource";
        let content_text: Option<String> = Some(text.to_string());
        let result: serde_json::Value = content_text
            .map(|t| serde_json::from_str(&t).unwrap_or(serde_json::Value::String(t)))
            .unwrap_or(serde_json::Value::Null);
        assert!(
            result.is_string(),
            "Non-JSON text should be String, not Null"
        );
        assert_eq!(result.as_str().unwrap(), text);
    }

    #[test]
    fn resource_text_json_returns_parsed() {
        let text = r#"{"key": "value"}"#;
        let content_text: Option<String> = Some(text.to_string());
        let result: serde_json::Value = content_text
            .map(|t| serde_json::from_str(&t).unwrap_or(serde_json::Value::String(t)))
            .unwrap_or(serde_json::Value::Null);
        assert!(result.is_object());
    }

    #[test]
    fn resource_text_none_returns_null() {
        let content_text: Option<String> = None;
        let result: serde_json::Value = content_text
            .map(|t| serde_json::from_str(&t).unwrap_or(serde_json::Value::String(t)))
            .unwrap_or(serde_json::Value::Null);
        assert!(result.is_null());
    }

    // ========================================================================
    // Wave 2: Deep Audit - Bug-Proving Tests
    // ========================================================================

    // ---- BUG: run_agent response extraction loses non-string JSON ----
    // In verbs.rs lines 1153-1158:
    //   let response = result.final_output
    //       .get("response")
    //       .and_then(|v| v.as_str())
    //       .unwrap_or("");
    //
    // If the agent's response is a JSON object (e.g., from structured output),
    // `as_str()` returns None because it's not a string, and the entire response
    // is silently replaced with an empty string.
    //
    // FIX: Use a more robust extraction:
    //   let response = match result.final_output.get("response") {
    //       Some(Value::String(s)) => s.clone(),
    //       Some(v) => v.to_string(), // Serialize non-string JSON
    //       None => String::new(),
    //   };
    #[test]
    fn wave2_run_agent_response_extraction_loses_json_objects() {
        // Simulate what run_agent does when extracting the response
        // The agent wraps its output as: serde_json::json!({ "response": response })

        // Case 1: String response - works fine
        let output_string = serde_json::json!({ "response": "Hello world" });
        let extracted_string = output_string
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(extracted_string, "Hello world", "String extraction works");

        // Case 2: JSON object response - BUG: silently lost
        let output_object = serde_json::json!({
            "response": {
                "title": "AI Blog Post",
                "content": "This is a structured response",
                "metadata": { "word_count": 42 }
            }
        });
        let extracted_object = output_object
            .get("response")
            .and_then(|v| v.as_str()) // Returns None for objects!
            .unwrap_or("");

        // BUG PROVEN: The entire structured response is lost, replaced with ""
        assert_eq!(
            extracted_object, "",
            "BUG PROVEN: JSON object response is silently replaced with empty string. \
             The response field exists and contains valid JSON, but as_str() returns None \
             for non-string JSON values."
        );

        // Show what the correct extraction would look like
        let correct_extraction = output_object
            .get("response")
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        assert!(
            !correct_extraction.is_empty(),
            "Correct extraction should preserve the JSON object"
        );
        assert!(
            correct_extraction.contains("AI Blog Post"),
            "Correct extraction should contain the title"
        );

        // Case 3: Array response - also lost
        let output_array = serde_json::json!({
            "response": ["item1", "item2", "item3"]
        });
        let extracted_array = output_array
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            extracted_array, "",
            "BUG PROVEN: Array responses are also silently lost"
        );

        // Case 4: Numeric response - also lost
        let output_number = serde_json::json!({ "response": 42 });
        let extracted_number = output_number
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            extracted_number, "",
            "BUG PROVEN: Numeric responses are also silently lost"
        );

        // Case 5: Boolean response - also lost
        let output_bool = serde_json::json!({ "response": true });
        let extracted_bool = output_bool
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            extracted_bool, "",
            "BUG PROVEN: Boolean responses are also silently lost"
        );
    }

    // ========================================================================
    // TDD tests for agent response extraction fix
    // ========================================================================

    #[test]
    fn agent_response_preserves_json_object() {
        let mut output = serde_json::Map::new();
        let obj = serde_json::json!({"title": "Hello", "score": 42});
        output.insert("response".to_string(), obj.clone());
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, r#"{"title":"Hello","score":42}"#);
    }

    #[test]
    fn agent_response_preserves_json_array() {
        let mut output = serde_json::Map::new();
        let arr = serde_json::json!(["item1", "item2", "item3"]);
        output.insert("response".to_string(), arr);
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, r#"["item1","item2","item3"]"#);
    }

    #[test]
    fn agent_response_preserves_string() {
        let mut output = serde_json::Map::new();
        output.insert(
            "response".to_string(),
            serde_json::Value::String("Hello world".to_string()),
        );
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "Hello world");
    }

    #[test]
    fn agent_response_handles_number() {
        let mut output = serde_json::Map::new();
        output.insert("response".to_string(), serde_json::json!(42));
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "42");
    }

    #[test]
    fn agent_response_handles_boolean() {
        let mut output = serde_json::Map::new();
        output.insert("response".to_string(), serde_json::json!(true));
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "true");
    }

    #[test]
    fn agent_response_handles_null() {
        let mut output = serde_json::Map::new();
        output.insert("response".to_string(), serde_json::Value::Null);
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "null");
    }

    #[test]
    fn agent_response_handles_missing_key() {
        let output = serde_json::Map::new();
        let final_output = serde_json::Value::Object(output);

        let response = match final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };

        assert_eq!(response, "");
    }

    // =========================================================================
    // Vision Helper Tests
    // =========================================================================

    #[test]
    fn detect_image_media_type_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::PNG));
    }

    #[test]
    fn detect_image_media_type_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::JPEG));
    }

    #[test]
    fn detect_image_media_type_gif() {
        let data = b"GIF89a\x00\x00";
        let result = super::detect_image_media_type(data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::GIF));
    }

    #[test]
    fn detect_image_media_type_webp() {
        let data = b"RIFF\x00\x00\x00\x00WEBP";
        let result = super::detect_image_media_type(data);
        assert_eq!(result, Some(rig::completion::message::ImageMediaType::WEBP));
    }

    #[test]
    fn detect_image_media_type_unknown() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, None);
    }

    #[test]
    fn detect_image_media_type_too_small() {
        let data = [0x89, 0x50];
        let result = super::detect_image_media_type(&data);
        assert_eq!(result, None);
    }
}
