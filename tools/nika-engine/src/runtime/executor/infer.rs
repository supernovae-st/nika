//! Infer verb implementation for TaskExecutor
//!
//! Contains `run_infer`, `run_infer_vision`, and `check_infer_guardrails`.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tracing::{debug, instrument, warn};

use crate::ast::output::{OutputPolicy, SchemaRef};
use crate::ast::InferParams;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::{ContextSource, EventKind};
use crate::provider::rig::{InferOptions, StreamChunk};
use crate::runtime::{InferCallback, StructuredOutputEngine};
use crate::store::RunContext;

use base64::Engine;

use super::verbs::{detect_image_media_type, estimate_tokens, redact_for_event};
use super::TaskExecutor;

impl TaskExecutor {
    #[instrument(skip(self, infer, bindings, datastore, output_policy), fields(%task_id))]
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

        // Resolve {{with.alias}} templates in prompt and system prompt (Bug 1)
        let mut prompt = template_resolve(&infer.prompt, bindings, datastore)?.into_owned();
        let resolved_system = match &infer.system {
            Some(sys) => Some(template_resolve(sys, bindings, datastore)?.into_owned()),
            None => None,
        };

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

        // Pre-read file-based from_example for prompt injection
        let cached_example = if let Some(policy) = output_policy {
            if let Some(SchemaRef::File(ref path)) = policy.from_example {
                match tokio::fs::read_to_string(path).await {
                    Ok(content) => serde_json::from_str(&content).ok(),
                    Err(e) => {
                        debug!(task_id = %task_id, "Failed to pre-read from_example '{}': {}", path, e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Inject JSON schema instruction if output policy requires JSON with schema
        if let Some(schema_instruction) =
            Self::build_json_schema_instruction(output_policy, cached_example.as_ref())
        {
            prompt.push_str(&schema_instruction);
            debug!(task_id = %task_id, "Injected JSON schema instruction into infer prompt");
        }

        // EMIT: TemplateResolved (redacted to avoid leaking secrets)
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: infer.prompt.clone(),
            result: redact_for_event(&prompt),
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

        // POLICY CHECK: token budget (atomic reserve to prevent TOCTOU with concurrent for_each)
        let estimated_tokens = estimate_tokens(prompt.len());
        if let Err(reason) = self
            .policy_enforcer
            .write()
            .reserve_tokens(estimated_tokens)
        {
            tracing::warn!(
                task_id = %task_id,
                estimated_tokens = estimated_tokens,
                reason = %reason,
                "infer: blocked by token budget"
            );
            return Err(NikaError::PolicyViolation { reason });
        }

        // ═══════════════════════════════════════════════════════════════════
        // VISION DISPATCH — must run BEFORE Layer 0
        // ═══════════════════════════════════════════════════════════════════
        // Layer 0 uses text-only tool injection which ignores content: parts.
        // Vision must bypass structured output and go directly to infer_vision.
        if has_content {
            return self
                .run_infer_vision(
                    task_id,
                    infer,
                    &prompt,
                    bindings,
                    datastore,
                    &provider,
                    provider_name,
                    model,
                    resolved_system.as_deref(),
                    estimated_tokens,
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
                            .infer_with_tools(
                                &prompt,
                                tools,
                                model,
                                infer.max_tokens,
                                resolved_system.as_deref(),
                            )
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
                                            let result_str = result.value.to_string();
                                            let est_in = estimate_tokens(prompt.len());
                                            let est_out = estimate_tokens(result_str.len());
                                            let cost = crate::provider::cost::ProviderKind::parse(
                                                provider_name,
                                            )
                                            .map(|pk| {
                                                crate::provider::cost::calculate_cost(
                                                    pk,
                                                    model.unwrap_or("default"),
                                                    est_in,
                                                    est_out,
                                                )
                                            })
                                            .unwrap_or(0.0);
                                            self.event_log.emit(EventKind::ProviderResponded {
                                                task_id: Arc::clone(task_id),
                                                request_id: None,
                                                input_tokens: est_in,
                                                output_tokens: est_out,
                                                cache_read_tokens: 0,
                                                ttft_ms: None,
                                                finish_reason: "stop".to_string(),
                                                cost_usd: if cost.is_finite() { cost } else { 0.0 },
                                            });
                                            debug!(
                                                task_id = %task_id,
                                                layer = result.layer,
                                                "Layer 0 + validation succeeded"
                                            );
                                            // Adjust token reservation before early return
                                            let est_actual = estimate_tokens(result_str.len());
                                            self.policy_enforcer
                                                .write()
                                                .adjust_reservation(estimated_tokens, est_actual);
                                            return Ok(result_str);
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
                                    let est_in = estimate_tokens(prompt.len());
                                    let est_out = estimate_tokens(tool_result.len());
                                    let cost =
                                        crate::provider::cost::ProviderKind::parse(provider_name)
                                            .map(|pk| {
                                                crate::provider::cost::calculate_cost(
                                                    pk,
                                                    model.unwrap_or("default"),
                                                    est_in,
                                                    est_out,
                                                )
                                            })
                                            .unwrap_or(0.0);
                                    self.event_log.emit(EventKind::ProviderResponded {
                                        task_id: Arc::clone(task_id),
                                        request_id: None,
                                        input_tokens: est_in,
                                        output_tokens: est_out,
                                        cache_read_tokens: 0,
                                        ttft_ms: None,
                                        finish_reason: "stop".to_string(),
                                        cost_usd: if cost.is_finite() { cost } else { 0.0 },
                                    });
                                    // Adjust token reservation before early return
                                    self.policy_enforcer
                                        .write()
                                        .adjust_reservation(estimated_tokens, est_in + est_out);
                                    return Ok(tool_result);
                                }
                            }
                            Err(e) => {
                                // BUG 10: MaxTurnError(0) is an expected skip, not a real error.
                                // This happens when the provider doesn't support tool_choice
                                // or when structured output uses a fast-path bypass.
                                let err_str = e.to_string();
                                let is_expected_skip = err_str.contains("MaxTurnError")
                                    || err_str.contains("max turn limit: 0");
                                let error_msg = if is_expected_skip {
                                    "tool injection skipped (not supported by provider)".to_string()
                                } else {
                                    err_str
                                };

                                debug!(
                                    task_id = %task_id,
                                    error = %error_msg,
                                    skipped = is_expected_skip,
                                    "Layer 0 {}, falling through to streaming",
                                    if is_expected_skip { "skipped" } else { "failed" }
                                );
                                self.event_log.emit(EventKind::StructuredOutputAttempt {
                                    task_id: Arc::clone(task_id),
                                    layer: 0,
                                    layer_name: "tool_injection".to_string(),
                                    attempt: 1,
                                    success: false,
                                    error: Some(error_msg),
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
            infer.temperature.is_some() || infer.max_tokens.is_some() || resolved_system.is_some();

        let stream_result = if has_llm_options {
            // Use InferOptions for temperature, max_tokens, system prompt (resolved)
            let options = InferOptions {
                model: model.map(|s| s.to_string()),
                temperature: infer.temperature,
                max_tokens: infer.max_tokens,
                system: resolved_system.clone(),
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

        // Adjust reservation with actual token count
        let actual_tokens = stream_result.input_tokens + stream_result.output_tokens;
        self.policy_enforcer
            .write()
            .adjust_reservation(estimated_tokens, actual_tokens);

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
            request_id: stream_result.request_id.clone(),
            input_tokens: stream_result.input_tokens,
            output_tokens: stream_result.output_tokens,
            cache_read_tokens: stream_result.cached_input_tokens,
            ttft_ms: stream_result.ttft_ms,
            finish_reason: "stop".to_string(),
            cost_usd: if cost.is_finite() { cost } else { 0.0 },
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
                            .with_original_prompt(prompt.to_string())
                            .with_provider_context(
                                provider_name.to_string(),
                                model.unwrap_or("unknown").to_string(),
                            );

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

                    // Return validated JSON as string — check guardrails first
                    let structured_output = result.value.to_string();
                    self.check_infer_guardrails(task_id, infer, &structured_output)?;
                    return Ok(structured_output);
                }
            }
        }

        // Run guardrails before returning the final output
        self.check_infer_guardrails(task_id, infer, &stream_result.text)?;

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
        provider_name: &str,
        model: Option<&str>,
        resolved_system: Option<&str>,
        reserved_tokens: u64,
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
            .filter(|p| {
                matches!(
                    p,
                    crate::ast::content::ContentPart::Image { .. }
                        | crate::ast::content::ContentPart::ImageUrl { .. }
                )
            })
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
                    // Bug 39: reject unsupported formats with clear error
                    if media_type.is_none() {
                        return Err(NikaError::ValidationError {
                            reason: format!(
                                "Vision image has unsupported format (CAS: {}). Supported: PNG, JPEG, GIF, WebP",
                                resolved_source
                            ),
                        });
                    }
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
                    image_count += 1; // Bug 40: count ImageUrl in telemetry
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

        let vision_work =
            provider.infer_vision(user_content, model, resolved_system, infer.max_tokens);
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
            .adjust_reservation(reserved_tokens, est_in + est_out);

        let cost = crate::provider::cost::ProviderKind::parse(provider_name)
            .map(|pk| {
                crate::provider::cost::calculate_cost(
                    pk,
                    model.unwrap_or("default"),
                    est_in,
                    est_out,
                )
            })
            .unwrap_or(0.0);

        self.event_log.emit(EventKind::ProviderResponded {
            task_id: Arc::clone(task_id),
            request_id: None,
            input_tokens: est_in,
            output_tokens: est_out,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: "stop".to_string(),
            cost_usd: if cost.is_finite() { cost } else { 0.0 },
        });

        Ok(vision_result)
    }

    /// Run guardrails configured on an infer task against the output.
    ///
    /// Emits GuardrailPassed/GuardrailFailed events and returns an error
    /// if any guardrail with `on_failure: fail` triggers.
    fn check_infer_guardrails(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        output: &str,
    ) -> Result<(), NikaError> {
        if infer.guardrails.is_empty() {
            return Ok(());
        }

        use crate::ast::guardrails::{immediate_failures, run_sync_guardrails};
        let results = run_sync_guardrails(&infer.guardrails, output);

        for result in &results {
            if result.passed {
                self.event_log.emit(EventKind::GuardrailPassed {
                    task_id: Arc::clone(task_id),
                    guardrail_type: result.guardrail_type.clone(),
                    description: result.guardrail_id.clone(),
                });
            } else {
                self.event_log.emit(EventKind::GuardrailFailed {
                    task_id: Arc::clone(task_id),
                    guardrail_type: result.guardrail_type.clone(),
                    description: result.guardrail_id.clone(),
                    message: result
                        .message
                        .clone()
                        .unwrap_or_else(|| "Guardrail check failed".to_string()),
                });
            }
        }

        let failures = immediate_failures(&results);
        if !failures.is_empty() {
            let msgs: Vec<String> = failures
                .iter()
                .map(|r| {
                    format!(
                        "{}: {}",
                        r.guardrail_type,
                        r.message.as_deref().unwrap_or("failed")
                    )
                })
                .collect();
            return Err(NikaError::GuardrailViolation {
                task_id: task_id.to_string(),
                violations: msgs,
            });
        }

        Ok(())
    }
}
