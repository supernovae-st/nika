// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Infer verb implementation for TaskExecutor
//!
//! Contains `run_infer`, `run_infer_vision`, and `check_infer_guardrails`.

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, instrument, warn};

use nika_core::catalogs::ModelResolver;

use crate::provider::rig::RigProvider;

use crate::ast::output::{OutputPolicy, SchemaRef};
use crate::ast::structured::StructuredOutputSpec;
use crate::ast::InferParams;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::{ContextSource, EventKind};
use crate::provider::rig::{build_response_format_params, InferOptions, StreamChunk};
use crate::runtime::{InferCallback, StructuredOutputEngine};
use crate::store::RunContext;

use base64::Engine;

use super::verbs::{
    detect_image_media_type, estimate_tokens, json_value_size_estimate, redact_for_event,
};
use super::TaskExecutor;
use crate::error_domains::ProviderError;

use crate::runtime::structured_retry;

// W16-A0: single-source ProviderResponded emission helper. Every
// `EventKind::ProviderResponded` in this file routes through this free
// function so that field additions/renames are a single-site change
// (invariant #24). The helper lives in nika-verb-infer so both the
// engine bridge and the verb crate's own `run()` share one emission
// call site.
use nika_verb_infer::emit_provider_responded;
use nika_verb_infer::VerbInferError;

/// Convert a `VerbInferError` to the engine's `NikaError` at the bridge boundary.
///
/// S17-A1: Invariant #25 — wildcard arm catches future variants added to the
/// `#[non_exhaustive]` `VerbInferError` enum, keeping the engine compilable
/// and triageable from logs. Pattern matches `map_verb_exec_error` (exec.rs)
/// and `map_verb_invoke_error` (invoke.rs).
fn map_verb_infer_error(err: VerbInferError) -> NikaError {
    match err {
        VerbInferError::Validation { reason } => NikaError::ValidationError { reason },
        VerbInferError::Provider(provider_err) => NikaError::ProviderApiError {
            message: provider_err.to_string(),
        },
        VerbInferError::Cancelled { task_id } => NikaError::TaskCancelled {
            task_id,
            reason: "workflow cancelled during infer".to_string(),
        },
        VerbInferError::NoTextContent => NikaError::ProviderApiError {
            message: "provider returned no text content".to_string(),
        },
        // VerbInferError is `#[non_exhaustive]` (invariant #25). When the
        // verb crate grows new variants (e.g. StructuredOutputFailed,
        // GuardrailViolation from the W14-B2 surgery), they fall through
        // here and must be explicitly mapped in a follow-up commit.
        other => NikaError::ProviderApiError {
            message: format!("infer: unmapped verb error variant: {other:?}"),
        },
    }
}

impl TaskExecutor {
    /// Build an [`InferCallback`] that calls `provider.infer()` with optional model override.
    ///
    /// Used by L0 safety-net, main streaming path (L2-L3), and repair path (L4).
    /// All three sites share the same pattern: clone provider, wrap in Arc, strip think tags.
    ///
    /// When `max_tokens` is passed by the engine (from the task's configured value),
    /// the callback uses `infer_with_options` to respect that limit instead of the
    /// hardcoded 8192 default.
    fn make_infer_callback(provider: &RigProvider, model: Option<&str>) -> InferCallback {
        let provider = provider.clone();
        let model_for_retry = model.map(|s| s.to_string());
        Arc::new(move |retry_prompt: String, max_tokens: Option<u32>| {
            let provider = provider.clone();
            let model = model_for_retry.clone();
            Box::pin(async move {
                let result = if max_tokens.is_some() {
                    let opts = InferOptions {
                        model: model.clone(),
                        max_tokens,
                        ..Default::default()
                    };
                    provider.infer_with_options(&retry_prompt, &opts).await
                } else {
                    provider.infer(&retry_prompt, model.as_deref(), None).await
                };
                result
                    .map(|s| super::verbs::strip_think_tags(&s))
                    .map_err(|e| NikaError::ProviderApiError {
                        message: format!("structured output retry failed: {}", e),
                    })
            })
        })
    }

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

        // Read the calling task's `trust: elevated` flag from the
        // task_local set by execute_task_iteration (Item 0.F). Avoids
        // threading a new param through the run_infer signature.
        let task_trust_elevated = nika_kernel::task_local::current_task_elevated();

        // ── Nika Shield: per-binding spotlight wrapping (Item 1) ───────────
        // Hybrid trust resolution (R1 corrected):
        //   1. Fast path — `bindings.source_task_id(alias)` for Task-sourced
        //      bindings, look up trust via `datastore.get_trust(task_id)`.
        //   2. Slow path — `bindings.is_input_sourced(alias)` for Input
        //      sources, derive trust from `datastore.invocation_source()`.
        //   3. LoopVar — conservatively wrap unless elevated (Sprint 3 will
        //      thread for_each parent trust through).
        //
        // Owned `String` from `value_as_prompt_str` (P0-2) — borrow checker
        // would otherwise reject the clone-on-mutate pattern.
        use nika_core::trust::TrustLevel;
        use std::borrow::Cow;

        let wrapped_cow: Cow<'_, ResolvedBindings> = 'spotlight: {
            if !self.shield.spotlight_enabled() || task_trust_elevated {
                let reason = if !self.shield.spotlight_enabled() {
                    "policy.spotlight=false"
                } else {
                    "trust: elevated"
                };
                self.event_log.emit(EventKind::SpotlightSkipped {
                    task_id: Arc::clone(task_id),
                    reason: reason.to_string(),
                });
                break 'spotlight Cow::Borrowed(bindings);
            }

            // Pre-pass: collect aliases that need wrapping. SmallVec keeps
            // the trusted-path zero-allocation (4 untrusted bindings is the
            // common ceiling for fan-in tasks).
            let mut untrusted: smallvec::SmallVec<[(String, TrustLevel, String); 4]> =
                smallvec::SmallVec::new();
            for (alias, _value) in bindings.iter() {
                // Fast path: Task source via set_with_source.
                if let Some(src_task_id) = bindings.source_task_id(alias) {
                    if let Some(t) = datastore.get_trust(src_task_id) {
                        if t.is_untrusted() {
                            untrusted.push((alias.to_string(), t, src_task_id.to_string()));
                        }
                    }
                    continue;
                }
                // Slow path: workflow inputs.
                if bindings.is_input_sourced(alias) {
                    let trust = datastore.invocation_source().input_trust();
                    if trust.is_untrusted() {
                        untrusted.push((alias.to_string(), trust, format!("input.{alias}")));
                    }
                    continue;
                }
                // for_each loop var — conservative wrap when invocation
                // source is untrusted. Sprint 3 will refine.
                if bindings.is_loop_var_sourced(alias) {
                    let trust = datastore.invocation_source().input_trust();
                    if trust.is_untrusted() {
                        untrusted.push((alias.to_string(), trust, format!("loop.{alias}")));
                    }
                }
            }

            if untrusted.is_empty() {
                break 'spotlight Cow::Borrowed(bindings);
            }

            // Clone-on-mutate: rewrite each untrusted binding with the
            // fenced version. Owned String from value_as_prompt_str.
            let mut wrapped = bindings.clone();
            for (alias, trust, label) in &untrusted {
                let Some(value) = wrapped.get(alias) else {
                    continue;
                };
                let raw = super::verbs::value_as_prompt_str(value);
                let fenced = self.shield.fence().wrap_untrusted(&raw, label, *trust);
                wrapped.set(alias.clone(), serde_json::Value::String(fenced));
                self.event_log.emit(EventKind::SpotlightApplied {
                    task_id: Arc::clone(task_id),
                    binding_alias: alias.clone(),
                    trust_level: trust.to_string(),
                });
            }
            Cow::Owned(wrapped)
        };
        let prompt_bindings: &ResolvedBindings = wrapped_cow.as_ref();

        // Resolve {{with.alias}} templates in prompt and system prompt (Bug 1)
        // — both use the spotlight-wrapped bindings so untrusted content is
        // fenced before substitution.
        let mut prompt = match template_resolve(&infer.prompt, prompt_bindings, datastore) {
            Ok(resolved) => resolved.into_owned(),
            Err(e) => {
                self.event_log.emit(EventKind::TemplateResolutionFailed {
                    task_id: Arc::clone(task_id),
                    template: infer.prompt.clone(),
                    error: e.to_string(),
                });
                return Err(e);
            }
        };
        let resolved_system = match &infer.system {
            Some(sys) => match template_resolve(sys, prompt_bindings, datastore) {
                Ok(resolved) => Some(resolved.into_owned()),
                Err(e) => {
                    self.event_log.emit(EventKind::TemplateResolutionFailed {
                        task_id: Arc::clone(task_id),
                        template: sys.clone(),
                        error: e.to_string(),
                    });
                    return Err(e);
                }
            },
            None => None,
        };

        // Auto-inject workflow-level skills into infer system prompt.
        // Skills are prepended so the LLM sees domain instructions before the task prompt.
        // Agent tasks handle their own skill injection via the agent loop.
        let resolved_system = if !self.skills_map.is_empty() {
            let all_skills: Vec<&str> = self.skills_map.keys().map(|s| s.as_str()).collect();
            let injected = self
                .skill_injector
                .inject(
                    resolved_system.as_deref(),
                    &all_skills,
                    &self.skills_map,
                    &self.skills_base_dir,
                )
                .await?;
            if injected.is_empty() {
                None
            } else {
                Some(injected)
            }
        } else {
            resolved_system
        };

        // Apply response_format instruction to system prompt
        // Skip when structured: is present (it has its own schema-based prompt injection)
        let has_structured = output_policy.is_some_and(|p| p.is_structured());
        let resolved_system = match infer.response_format {
            Some(crate::ast::ResponseFormat::Json) if !has_structured => {
                let instruction = "IMPORTANT: You MUST respond with valid JSON only. No explanatory text, no markdown code fences — only raw JSON.";
                Some(match resolved_system {
                    Some(sys) => format!("{sys}\n\n{instruction}"),
                    None => instruction.to_string(),
                })
            }
            Some(crate::ast::ResponseFormat::Markdown) if !has_structured => {
                let instruction = "IMPORTANT: You MUST respond in Markdown format.";
                Some(match resolved_system {
                    Some(sys) => format!("{sys}\n\n{instruction}"),
                    None => instruction.to_string(),
                })
            }
            _ => resolved_system, // Text, None, or structured: handles its own format
        };

        // ── Nika Shield: canary token injection (Item 2) ───────────────────
        // Suffix-only injection (P0-1) preserves the provider prefix-cache
        // hit rate. The injected `[trace_id=…]` block is the canary the
        // detector looks for in LLM responses.
        let resolved_system = if self.shield.canary_enabled() {
            let base = resolved_system.as_deref().unwrap_or("");
            let injected = self.shield.canary().inject_into_system_prompt(base);
            self.event_log.emit(EventKind::CanaryInjected {
                task_id: Arc::clone(task_id),
            });
            Some(injected)
        } else {
            resolved_system
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

        // Pre-read file-based from_example for prompt injection.
        // Fail fast if the file is missing or contains invalid JSON — continuing
        // would waste an API call that L2 validation will reject anyway.
        // SECURITY: reject paths with traversal patterns before reading.
        // Template-resolve path first (e.g. from_example: "cache/{{inputs.locale | lower}}.json")
        let cached_example = if let Some(policy) = output_policy {
            if let Some(SchemaRef::File(ref path)) = policy.from_example {
                let resolved_path = if path.contains("{{") {
                    template_resolve(path, bindings, datastore)?.into_owned()
                } else {
                    path.clone()
                };
                Self::validate_schema_path(&resolved_path)?;
                let content = match tokio::fs::read_to_string(&resolved_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        self.event_log.emit(EventKind::SchemaLoadFailed {
                            task_id: Arc::clone(task_id),
                            schema_path: resolved_path.clone(),
                            error: e.to_string(),
                        });
                        return Err(NikaError::SchemaFailed {
                            details: format!(
                                "Failed to read from_example '{}': {}",
                                resolved_path, e
                            ),
                        });
                    }
                };
                let value: Value = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(e) => {
                        self.event_log.emit(EventKind::SchemaLoadFailed {
                            task_id: Arc::clone(task_id),
                            schema_path: resolved_path.clone(),
                            error: e.to_string(),
                        });
                        return Err(NikaError::SchemaFailed {
                            details: format!(
                                "Invalid JSON in from_example '{}': {}",
                                resolved_path, e
                            ),
                        });
                    }
                };
                Some(value)
            } else {
                None
            }
        } else {
            None
        };

        // Also fail fast for file-based schema: references.
        // Same rationale: a missing/invalid schema file will waste an API call.
        if let Some(policy) = output_policy {
            if let Some(SchemaRef::File(ref path)) = policy.schema {
                Self::validate_schema_path(path)?;
                if policy.is_structured() && !tokio::fs::try_exists(path).await.unwrap_or(false) {
                    self.event_log.emit(EventKind::SchemaLoadFailed {
                        task_id: Arc::clone(task_id),
                        schema_path: path.clone(),
                        error: "file does not exist".to_string(),
                    });
                    return Err(NikaError::SchemaFailed {
                        details: format!("Schema file '{}' does not exist", path,),
                    });
                }
            }
        }

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
                        tokens: estimate_tokens(json_value_size_estimate(value)),
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

        // Resolve templates in model and provider fields
        // e.g. model: "{{inputs.fast_model | default('claude-haiku-4-5')}}"
        let resolved_model = match &infer.model {
            Some(m) => Some(template_resolve(m, bindings, datastore)?.into_owned()),
            None => None,
        };
        let resolved_provider: Option<String> = match &infer.provider {
            Some(p) => Some(template_resolve(p.as_str(), bindings, datastore)?.into_owned()),
            None => None,
        };

        // Parse slash syntax: model: "groq/llama-3.3-70b" → provider=groq, model=llama-3.3-70b
        let (resolved_provider, resolved_model) = if resolved_provider.is_none() {
            if let Some(ref model_str) = resolved_model {
                if let Some((ep, model)) = parse_model_slash(model_str) {
                    // Check if prefix looks like a HuggingFace org (not a known provider)
                    let is_known = crate::core::find_provider(ep).is_some()
                        || self.custom_endpoints.contains_key(ep)
                        || ep == "native"
                        || ep == "mock";
                    if !is_known {
                        return Err(NikaError::ProviderNotConfigured {
                            provider: format!(
                                "'{}' (from model: '{}'). Did you mean 'native/{}'? \
                                 Or define [endpoints.{}] in nika.toml",
                                ep, model_str, model_str, ep
                            ),
                        });
                    }
                    (Some(ep.to_string()), Some(model.to_string()))
                } else {
                    (resolved_provider, resolved_model)
                }
            } else {
                (resolved_provider, resolved_model)
            }
        } else {
            // Explicit provider set — if model also has slash, warn about ignored prefix
            if let Some(ref model_str) = resolved_model {
                if let Some((ep, _)) = parse_model_slash(model_str) {
                    if let Some(ref prov) = resolved_provider {
                        if ep != prov.as_str() {
                            warn!(
                                task_id = %task_id,
                                "model '{}' contains provider prefix '{}' but explicit provider: '{}' is set — \
                                 using '{}'. Remove provider: or fix the model prefix.",
                                model_str, ep, prov, prov
                            );
                        }
                    }
                }
            }
            (resolved_provider, resolved_model)
        };

        // Build provider chain: explicit chain > single provider > auto-inferred > workflow default
        let effective_chain: Vec<String> = if let Some(ref chain) = infer.provider_chain {
            chain.iter().map(|p| p.to_string()).collect()
        } else {
            let auto_provider = resolved_model
                .as_ref()
                .and_then(|m| ModelResolver::infer_provider_from_model(m))
                .map(|p| p.to_string());
            vec![resolved_provider
                .clone()
                .or(auto_provider)
                .unwrap_or_else(|| self.default_provider.to_string())]
        };

        // Resolve provider with fallback chain support.
        // Try each provider in the chain; if get_rig_provider fails (missing API key),
        // emit ProviderFallback event and try the next provider.
        let (provider_name_owned, provider_idx) = {
            let mut resolved_idx = 0;
            let mut last_error: Option<NikaError> = None;
            let mut found = false;

            for (i, name) in effective_chain.iter().enumerate() {
                if name == "mock" {
                    // Mock always succeeds — don't need to check
                    resolved_idx = i;
                    found = true;
                    break;
                }
                match self.get_rig_provider(name) {
                    Ok(_) => {
                        resolved_idx = i;
                        found = true;
                        break;
                    }
                    Err(e) => {
                        if effective_chain.len() > 1 && i < effective_chain.len() - 1 {
                            self.event_log.emit(EventKind::ProviderFallback {
                                task_id: Arc::clone(task_id),
                                from: name.clone(),
                                to: effective_chain[i + 1].clone(),
                                reason: format!("{}", e),
                            });
                            tracing::warn!(
                                task_id = %task_id,
                                from = %name,
                                to = %effective_chain[i + 1],
                                error = %e,
                                "Provider fallback: {} → {}",
                                name,
                                effective_chain[i + 1]
                            );
                        }
                        last_error = Some(e);
                    }
                }
            }

            if !found {
                if effective_chain.len() > 1 {
                    return Err(NikaError::from(
                        crate::error_domains::ProviderError::FallbackChainExhausted {
                            last_provider: effective_chain.last().cloned().unwrap_or_default(),
                            last_error: last_error.map(|e| format!("{}", e)).unwrap_or_default(),
                        },
                    ));
                } else {
                    return Err(
                        last_error.unwrap_or_else(|| NikaError::ProviderNotConfigured {
                            provider: "none (empty provider chain)".to_string(),
                        }),
                    );
                }
            }

            (effective_chain[resolved_idx].clone(), resolved_idx)
        };
        let _ = provider_idx; // Used for tracing if needed
        let provider_name = &provider_name_owned;

        // Mock provider support for testing (no API call)
        // Generates a generic JSON response with common test fields
        if provider_name == "mock" {
            // Mock failure simulation: NIKA_MOCK_FAIL_COUNT=N makes the first N calls fail
            // with a transient error (retryable). Used for testing retry + backoff.
            use std::sync::atomic::{AtomicU32, Ordering};
            static MOCK_CALL_COUNTER: AtomicU32 = AtomicU32::new(0);
            if let Ok(fail_count_str) = std::env::var("NIKA_MOCK_FAIL_COUNT") {
                if let Ok(fail_count) = fail_count_str.parse::<u32>() {
                    let call_num = MOCK_CALL_COUNTER.fetch_add(1, Ordering::SeqCst);
                    if call_num < fail_count {
                        return Err(NikaError::ProviderApiError {
                            message: format!(
                                "Mock failure simulation: call {} of {} (NIKA_MOCK_FAIL_COUNT={})",
                                call_num + 1,
                                fail_count,
                                fail_count
                            ),
                        });
                    }
                }
            }

            // For vision content, include content metadata in mock response
            let vision_info = if has_content {
                let parts = match infer.content.as_ref() {
                    Some(p) => p,
                    None => {
                        return Ok("mock: {\"error\": \"vision content is empty\"}".to_string());
                    }
                };
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
                model: resolved_model
                    .as_deref()
                    .unwrap_or("mock-model")
                    .to_string(),
                prompt_len: prompt.len(),
                endpoint_url: None,
            });

            // If structured output is configured, generate schema-conforming JSON
            // so E2E tests can validate the entire structured output pipeline.
            let mock_response = if let Some(policy) = output_policy {
                if let Some(schema_ref) = &policy.schema {
                    let schema_value = match schema_ref {
                        crate::ast::output::SchemaRef::Inline(v) => v.clone(),
                        crate::ast::output::SchemaRef::File(path) => {
                            // Try to load file schema for mock (best-effort).
                            // Basic path traversal guard even in mock mode.
                            if path.contains("..") {
                                tracing::warn!(path = %path, "mock: schema path contains '..' — skipping");
                                serde_json::Value::Null
                            } else {
                                match tokio::fs::read_to_string(path).await {
                                    Ok(content) => serde_json::from_str(&content)
                                        .unwrap_or(serde_json::Value::Null),
                                    Err(_) => serde_json::Value::Null,
                                }
                            }
                        }
                    };
                    if !schema_value.is_null() {
                        crate::runtime::mock_json::generate_mock_json(&schema_value)
                    } else {
                        Self::generic_mock_json(task_id, &prompt, &vision_info)
                    }
                } else {
                    Self::generic_mock_json(task_id, &prompt, &vision_info)
                }
            } else {
                Self::generic_mock_json(task_id, &prompt, &vision_info)
            };
            let mock_response_str = mock_response.to_string();
            emit_provider_responded(
                &self.event_log,
                task_id,
                Some("mock-request".to_string()),
                estimate_tokens(prompt.len()),
                estimate_tokens(mock_response_str.len()),
                0,
                Some(0),
                nika_event::FinishReason::Mock,
                0.0,
            );
            // Run guardrails on mock output (same as non-mock path)
            self.check_infer_guardrails(task_id, infer, &mock_response_str)?;
            return Ok(mock_response_str);
        }

        // Resolve provider from endpoint catalog or named endpoint
        let provider = self.get_rig_provider(provider_name)?;

        // Resolve model once via ModelResolver (task > workflow > provider default)
        let resolved_m = ModelResolver::resolve(
            resolved_model.as_deref(),
            self.default_model.as_deref(),
            provider_name,
            provider_idx,
            resolved_model.as_deref(),
        );
        let model_id = resolved_m.model_id;
        let model = Some(model_id.as_str());

        // Per-provider temperature validation (M-orig8)
        if let Some(temp) = infer.temperature {
            if let Some(pk) = crate::provider::cost::ProviderKind::parse(provider_name) {
                let max = pk.max_temperature();
                if temp > max {
                    return Err(NikaError::ValidationError {
                        reason: format!(
                            "temperature {temp} exceeds {provider_name} maximum ({max})"
                        ),
                    });
                }
            }
        }

        // EMIT: ProviderCalled (use canonical provider name, not YAML alias)
        self.event_log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: provider.name().to_string(),
            model: model_id.clone(),
            prompt_len: prompt.len(),
            endpoint_url: None,
        });

        // POLICY CHECK: token budget (atomic reserve to prevent TOCTOU with concurrent for_each).
        // RAII guard (W6.4): if the task is cancelled or errors before adjust(),
        // Drop releases the reserved tokens back to the budget.
        let estimated_tokens = estimate_tokens(prompt.len());
        let mut token_reservation = crate::runtime::policy::TokenReservation::new(
            Arc::clone(&self.policy_enforcer),
            estimated_tokens,
        )
        .map_err(|reason| {
            tracing::warn!(
                task_id = %task_id,
                estimated_tokens = estimated_tokens,
                reason = %reason,
                "infer: blocked by token budget"
            );
            NikaError::PolicyViolation { reason }
        })?;

        // ═══════════════════════════════════════════════════════════════════
        // VISION DISPATCH — must run BEFORE Layer 0
        // ═══════════════════════════════════════════════════════════════════
        // Layer 0 uses text-only tool injection which ignores content: parts.
        // Vision must bypass structured output and go directly to infer_vision.
        if has_content {
            let vision_result = self
                .run_infer_vision(
                    task_id,
                    infer,
                    &prompt,
                    bindings,
                    datastore,
                    &provider,
                    provider_name,
                    &model_id,
                    resolved_system.as_deref(),
                    &mut token_reservation,
                )
                .await?;

            // If structured output is configured, validate vision output through L2-L4
            if let Some(policy) = output_policy {
                if policy.is_structured() {
                    if let Some(spec) = policy.to_structured_spec() {
                        let spec = resolve_from_example_in_spec(spec, &cached_example);
                        let infer_callback = Self::make_infer_callback(&provider, None);
                        let mut engine = StructuredOutputEngine::new(
                            spec.clone(),
                            Arc::new(self.event_log.clone()),
                        )
                        .with_infer_callback(infer_callback)
                        .with_original_prompt(prompt.to_string())
                        .with_provider_context(provider_name.to_string(), model_id.clone())
                        .with_max_tokens(infer.max_tokens)
                        .with_workflow_dir(self.workflow_base_dir.clone());

                        if let Some(ref repair_model) = spec.repair_model {
                            let trimmed = repair_model.trim();
                            if !trimmed.is_empty() {
                                let repair_callback =
                                    Self::make_infer_callback(&provider, Some(trimmed));
                                engine = engine
                                    .with_repair_callback(repair_callback)
                                    .with_repair_model_name(trimmed.to_string());
                            }
                        }

                        match engine.validate(task_id.as_ref(), &vision_result).await {
                            Ok(result) => {
                                return Ok(result.value.to_string());
                            }
                            Err(e) => {
                                tracing::warn!(
                                    task = %task_id,
                                    error = %e,
                                    "Vision + structured: validation failed, returning raw output"
                                );
                            }
                        }
                    }
                }
            }

            return Ok(vision_result);
        }

        // ═══════════════════════════════════════════════════════════════════
        // LAYER 0: Tool Injection (DynamicSubmitTool)
        // ═══════════════════════════════════════════════════════════════════
        // If structured output is configured, try tool injection first.
        // The LLM is forced to call submit_result() with schema-compliant JSON.
        // If it succeeds, we still validate the result. If it fails, we fall
        // through to streaming + post-processing (Layers 1-3).
        if let Some(policy) = output_policy {
            // Respect enable_tool_injection: false — skip L0 entirely
            let should_inject_tool = policy
                .source_structured_spec
                .as_ref()
                .map(|s| s.enable_tool_injection_or_default())
                .unwrap_or(true);
            if policy.is_structured() && should_inject_tool {
                // FIX(A2): Resolve schema from EITHER `schema:` or `from_example:`.
                // Previously only checked policy.schema, skipping Layer 0 tool injection
                // entirely when users provided from_example instead of schema.
                // Respect strict mode for from_example schema derivation
                let use_strict = policy
                    .source_structured_spec
                    .as_ref()
                    .and_then(|s| s.strict)
                    .unwrap_or(false);
                let derive_schema = if use_strict {
                    crate::ast::structured::json_to_schema_strict as fn(&Value) -> Value
                } else {
                    crate::ast::structured::json_to_schema
                };

                let schema_value: Result<Value, NikaError> =
                    if let Some(example_ref) = &policy.from_example {
                        // Derive JSON Schema from example (same as StructuredOutputEngine::load_schema)
                        match example_ref {
                            crate::ast::output::SchemaRef::Inline(v) => Ok(derive_schema(v)),
                            crate::ast::output::SchemaRef::File(path) => {
                                // Template-resolve path (e.g. "cache/{{inputs.locale | lower}}.json")
                                // Must match the resolution done in the prompt injection path (line ~129)
                                let resolved_path = if path.contains("{{") {
                                    template_resolve(path, bindings, datastore)?.into_owned()
                                } else {
                                    path.clone()
                                };
                                tokio::fs::read_to_string(&resolved_path)
                                    .await
                                    .map_err(|e| NikaError::SchemaFailed {
                                        details: format!(
                                            "Failed to read example '{}': {}",
                                            resolved_path, e
                                        ),
                                    })
                                    .and_then(|content| {
                                        let example: Value = serde_json::from_str(&content)
                                            .map_err(|e| NikaError::SchemaFailed {
                                                details: format!(
                                                    "Invalid JSON in example '{}': {}",
                                                    resolved_path, e
                                                ),
                                            })?;
                                        Ok(derive_schema(&example))
                                    })
                            }
                        }
                    } else if let Some(schema_ref) = &policy.schema {
                        // Standard: resolve schema directly
                        match schema_ref {
                            crate::ast::output::SchemaRef::Inline(v) => Ok(v.clone()),
                            crate::ast::output::SchemaRef::File(path) => {
                                let resolved_path = if path.contains("{{") {
                                    template_resolve(path, bindings, datastore)?.into_owned()
                                } else {
                                    path.clone()
                                };
                                tokio::fs::read_to_string(&resolved_path)
                                    .await
                                    .map_err(|e| NikaError::SchemaFailed {
                                        details: format!(
                                            "Failed to read schema '{}': {}",
                                            resolved_path, e
                                        ),
                                    })
                                    .and_then(|content| {
                                        serde_json::from_str(&content).map_err(|e| {
                                            NikaError::SchemaFailed {
                                                details: format!(
                                                    "Invalid JSON in schema '{}': {}",
                                                    resolved_path, e
                                                ),
                                            }
                                        })
                                    })
                            }
                        }
                    } else {
                        // is_structured() returned true but neither schema nor from_example
                        // is set — defensive fallback, should not happen
                        Err(NikaError::SchemaFailed {
                            details: "Structured output configured but no schema or from_example"
                                .to_string(),
                        })
                    };

                {
                    // BUG-1 FIX: Create inference callback for Layer 0 safety net
                    // validation so L3 (retry with feedback) and L4 (LLM repair)
                    // are enabled when L0a/L0b output fails schema validation.
                    let l0_infer_callback = Self::make_infer_callback(&provider, model);

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

                    // ───────────────────────────────────────────────────────
                    // LAYER 0a: Native response_format (OpenAI-compatible)
                    // ───────────────────────────────────────────────────────
                    let (layer_0a_attempted, maybe_l0a) = if let Ok(ref sv) = schema_value {
                        self.try_layer_0a_response_format(
                            &provider,
                            &model_id,
                            &prompt,
                            infer,
                            policy,
                            sv,
                            &l0_infer_callback,
                            task_id,
                            provider_name,
                            &resolved_system,
                            &mut token_reservation,
                            &cached_example,
                        )
                        .await?
                    } else {
                        (false, None)
                    };
                    if let Some(result) = maybe_l0a {
                        self.check_infer_guardrails(task_id, infer, &result)?;
                        return Ok(result);
                    }

                    // Skip Layer 0b when Layer 0a was already attempted to avoid
                    // double API calls and double billing.
                    if !layer_0a_attempted {
                        if let Ok(ref sv) = schema_value {
                            if let Some(result) = self
                                .try_layer_0b_tool_injection(
                                    &provider,
                                    &model_id,
                                    &prompt,
                                    infer,
                                    policy,
                                    sv,
                                    &l0_infer_callback,
                                    task_id,
                                    provider_name,
                                    &resolved_system,
                                    &mut token_reservation,
                                    &cached_example,
                                )
                                .await?
                            {
                                self.check_infer_guardrails(task_id, infer, &result)?;
                                return Ok(result);
                            }
                        }
                    } // end !layer_0a_attempted guard
                }
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // S17-A2: VERB CRATE DELEGATION (simple text path)
        // ═══════════════════════════════════════════════════════════════════
        // For plain text inference (no vision, no structured output, no
        // extended thinking), delegate to nika_verb_infer::run(). The verb
        // crate calls Provider::infer() via the kernel trait (which routes
        // through streaming internally since S17-A0), emits ProviderResponded
        // via the shared helper (invariant #24), and returns InferOutput.
        //
        // This path replaces the engine's streaming retry loop for the simple
        // case — task-level retry in runner.rs handles transient failures.
        // Structured output (L2-L3 retry), vision, and extended thinking
        // still use the engine's streaming path below.
        if !has_structured && !has_content && infer.extended_thinking != Some(true) {
            use nika_kernel::caps::InferCaps;
            use nika_kernel::provider::ProviderExtras;

            let provider_arc: Arc<dyn nika_kernel::provider::Provider> =
                Arc::new(provider.clone());
            let policy_clone = self.policy_enforcer.read().clone();
            let clock = nika_clock::SystemClock;
            let fs = nika_fs::TokioFs;

            let infer_caps = InferCaps::new(
                provider_arc,
                &fs,
                &policy_clone,
                &clock,
                &self.cancel_token,
                &self.workflow_base_dir,
            );

            let infer_input = nika_verb_infer::InferInput {
                prompt: &prompt,
                system: resolved_system.as_deref(),
                model: &model_id,
                temperature: infer.temperature.map(|t| t as f32),
                max_tokens: infer.max_tokens,
                thinking_budget: None,
                extra: ProviderExtras::default(),
                task_id: Arc::clone(task_id),
            };

            // P1 fix: match the streaming path's 4-attempt retry for transient
            // errors (500, 502, 503, 429, timeout). Without this, tasks that
            // relied on the engine's implicit retry would fail on first error.
            const VERB_BACKOFF_MS: [u64; 3] = [1_000, 3_000, 10_000];
            const VERB_MAX_ATTEMPTS: usize = 4;

            let mut last_err: Option<NikaError> = None;
            for attempt in 0..VERB_MAX_ATTEMPTS {
                if attempt > 0 {
                    let delay_ms = VERB_BACKOFF_MS[attempt - 1];
                    let error_str = last_err
                        .as_ref()
                        .map(|e| e.to_string())
                        .unwrap_or_default();
                    warn!(
                        task_id = %task_id,
                        attempt = attempt + 1,
                        delay_ms,
                        error = %error_str,
                        "Verb crate transient error, retrying after {}ms...",
                        delay_ms
                    );
                    self.event_log.emit(EventKind::ProviderAutoRetried {
                        task_id: Arc::clone(task_id),
                        attempt: (attempt + 1) as u32,
                        max_attempts: VERB_MAX_ATTEMPTS as u32,
                        delay_ms,
                        error: error_str,
                    });
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }

                match nika_verb_infer::run(&infer_input, &infer_caps, &self.event_log).await {
                    Ok(output) => {
                        token_reservation.adjust(
                            output.response.usage.input_tokens
                                + output.response.usage.output_tokens,
                        );
                        self.check_infer_guardrails(task_id, infer, &output.text)?;
                        return Ok(output.text);
                    }
                    Err(e) => {
                        let nika_err = map_verb_infer_error(e);
                        if structured_retry::is_retryable(&nika_err)
                            && attempt + 1 < VERB_MAX_ATTEMPTS
                        {
                            last_err = Some(nika_err);
                            continue;
                        }
                        return Err(nika_err);
                    }
                }
            }

            // Unreachable: the loop either returns Ok or Err on the last attempt.
            return Err(last_err.unwrap_or_else(|| NikaError::ProviderApiError {
                message: "verb infer retry loop exhausted without result".to_string(),
            }));
        }

        // ═══════════════════════════════════════════════════════════════════
        // STREAMING PATH (Layers 1-3 fallback)
        // ═══════════════════════════════════════════════════════════════════
        // Used for: structured output retry (L2-L3), extended thinking,
        // and vision fallback. Simple text infer delegates above (S17-A2).
        let infer_start = Instant::now();
        let has_llm_options = infer.temperature.is_some()
            || infer.max_tokens.is_some()
            || resolved_system.is_some()
            || infer.extended_thinking == Some(true);

        // Build options once (reused across retry attempts)
        // Provider-aware capabilities: token param, temperature, thinking mechanism.
        use nika_core::catalogs::capabilities::{model_capabilities, TokenLimitParam};
        let caps = model_capabilities(provider_name, model_id.as_str());
        let is_openai_reasoning = caps.token_limit_param == TokenLimitParam::MaxCompletionTokens;

        let additional_params = if infer.extended_thinking == Some(true) {
            let budget = infer.thinking_budget.unwrap_or(4096);
            if is_openai_reasoning {
                // OpenAI reasoning models: use reasoning_effort + max_completion_tokens
                Some(serde_json::json!({
                    "reasoning_effort": "high",
                    "max_completion_tokens": infer.max_tokens.unwrap_or(budget.min(16384) as u32)
                }))
            } else if provider_name == "anthropic" {
                // Claude: native thinking block
                if let Some(temp) = infer.temperature {
                    if (temp - 1.0).abs() > f64::EPSILON {
                        tracing::warn!(
                            temperature = temp,
                            "Ignoring temperature={temp} — extended thinking requires temperature=1.0"
                        );
                    }
                }
                Some(serde_json::json!({
                    "thinking": { "type": "enabled", "budget_tokens": budget }
                }))
            } else if caps.supports_thinking {
                // Provider supports thinking but we don't have a specific implementation
                tracing::warn!(
                    provider = provider_name,
                    "extended_thinking mechanism not implemented for {provider_name} — ignoring"
                );
                None
            } else {
                tracing::warn!(
                    provider = provider_name,
                    "extended_thinking is not supported on {provider_name} — ignoring"
                );
                None
            }
        } else {
            None
        };

        let effective_max_tokens = if infer.extended_thinking == Some(true) {
            if is_openai_reasoning {
                // OpenAI: don't set max_tokens — use max_completion_tokens in additional_params
                None
            } else if provider_name == "anthropic" {
                let budget = infer.thinking_budget.unwrap_or(4096);
                let budget_u32 = u32::try_from(budget).unwrap_or(u32::MAX);
                Some(infer.max_tokens.unwrap_or(budget_u32.saturating_add(8192)))
            } else {
                infer.max_tokens
            }
        } else {
            infer.max_tokens
        };

        // Auto-set temperature 0.0 for structured output when not explicitly specified.
        // This ensures deterministic extraction across runs.
        let is_structured = output_policy.is_some_and(|p| p.is_structured());
        let effective_temperature = if infer.extended_thinking == Some(true) {
            // Claude requires temperature=1.0 for thinking; others: pass through
            if provider_name == "anthropic" {
                Some(1.0)
            } else {
                infer.temperature
            }
        } else if infer.temperature.is_none() && is_structured {
            Some(0.0)
        } else {
            infer.temperature
        };

        let options = if has_llm_options || effective_temperature.is_some() {
            Some(InferOptions {
                model: model.map(|s| s.to_string()),
                temperature: effective_temperature,
                max_tokens: effective_max_tokens,
                system: resolved_system.clone(),
                additional_params,
            })
        } else {
            None
        };

        // Retry loop for transient HTTP errors (500, 502, 503, 429, timeout).
        // Backoff schedule: 0s, 1s, 3s, 10s (4 attempts max).
        // This is a default safety net — if the task has its own retry: config,
        // the task-level retry in runner.rs handles broader retry logic.
        const BACKOFF_DELAYS_MS: [u64; 3] = [1_000, 3_000, 10_000];
        const MAX_PROVIDER_ATTEMPTS: usize = 4;

        let mut last_error: Option<NikaError> = None;
        let mut stream_result = None;

        for attempt in 0..MAX_PROVIDER_ATTEMPTS {
            if attempt > 0 {
                let delay_ms = BACKOFF_DELAYS_MS[attempt - 1];
                let error_str = last_error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());
                warn!(
                    task_id = %task_id,
                    attempt = attempt + 1,
                    max_attempts = MAX_PROVIDER_ATTEMPTS,
                    delay_ms,
                    error = %error_str,
                    "Transient provider error, retrying after {}ms...",
                    delay_ms
                );
                self.event_log.emit(EventKind::ProviderAutoRetried {
                    task_id: Arc::clone(task_id),
                    attempt: (attempt + 1) as u32,
                    max_attempts: MAX_PROVIDER_ATTEMPTS as u32,
                    delay_ms,
                    error: error_str,
                });
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }

            // Non-streaming path: provider API still requires a channel, so we
            // spawn a drain task to avoid SendError warnings in the provider.
            let (tx, mut rx) = mpsc::channel::<StreamChunk>(32);
            tokio::spawn(async move { while rx.recv().await.is_some() {} });
            let call_result = if let Some(ref opts) = options {
                provider
                    .infer_stream_with_options(&prompt, tx, opts)
                    .await
                    .map_err(|e| {
                        NikaError::from(ProviderError::ApiError {
                            message: e.to_string(),
                        })
                    })
            } else {
                provider
                    .infer_stream(&prompt, tx, model, None)
                    .await
                    .map_err(|e| {
                        NikaError::from(ProviderError::ApiError {
                            message: e.to_string(),
                        })
                    })
            };

            match call_result {
                Ok(result) => {
                    stream_result = Some(result);
                    break;
                }
                Err(e) => {
                    if structured_retry::is_retryable(&e) && attempt + 1 < MAX_PROVIDER_ATTEMPTS {
                        last_error = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        let stream_result = stream_result.ok_or_else(|| NikaError::ProviderApiError {
            message: "retry loop exited without producing a result".to_string(),
        })?;

        // Extract <think>...</think> blocks from reasoning models (Qwen, DeepSeek-R1)
        // Captures thinking content for observability, strips tags from output.
        let mut stream_result = stream_result;
        let (thinking_content, clean_text) =
            super::verbs::extract_thinking_tags(&stream_result.text);
        stream_result.text = clean_text;
        if let Some(ref thinking) = thinking_content {
            debug!(
                task_id = %task_id,
                thinking_len = thinking.len(),
                "Extracted <think> content from OpenAI-compat response"
            );
        }

        // Adjust reservation with actual token count
        let actual_tokens = stream_result.input_tokens + stream_result.output_tokens;
        token_reservation.adjust(actual_tokens);

        // EMIT: ProviderResponded with accurate token counts and cost from streaming response
        let infer_duration = infer_start.elapsed();
        let cost = if let Some(hourly_rate) = self.endpoint_hourly_rate(provider_name) {
            // Custom endpoint with hourly_rate: use time-based cost
            crate::provider::cost::calculate_hourly_cost(infer_duration.as_secs_f64(), hourly_rate)
        } else {
            // Cloud provider: use token-based cost
            provider
                .cost_provider_kind()
                .map(|pk| {
                    crate::provider::cost::calculate_cost_with_cache(
                        pk,
                        &model_id,
                        stream_result.input_tokens,
                        stream_result.output_tokens,
                        stream_result.cached_input_tokens,
                    )
                })
                .unwrap_or(0.0)
        };
        emit_provider_responded(
            &self.event_log,
            task_id,
            stream_result.request_id.clone(),
            stream_result.input_tokens,
            stream_result.output_tokens,
            stream_result.cached_input_tokens,
            stream_result.ttft_ms,
            stream_result
                .finish_reason
                .clone()
                .unwrap_or(nika_event::FinishReason::Stop),
            if cost.is_finite() { cost } else { 0.0 },
        );

        // Structured output validation via StructuredOutputEngine (Layers 1-3)
        // If output policy requires JSON with schema, validate and repair the output
        if let Some(policy) = output_policy {
            if policy.is_structured() {
                if let Some(spec) = policy.to_structured_spec() {
                    let spec = resolve_from_example_in_spec(spec, &cached_example);
                    debug!(
                        task_id = %task_id,
                        "Validating structured output via StructuredOutputEngine (Layers 1-3)"
                    );

                    // Create inference callback for Layer 2 & 3
                    // This allows the engine to actually call the LLM for retries and repairs
                    let infer_callback = Self::make_infer_callback(&provider, model);

                    let mut engine =
                        StructuredOutputEngine::new(spec.clone(), Arc::new(self.event_log.clone()))
                            .with_infer_callback(infer_callback)
                            .with_original_prompt(prompt.to_string())
                            .with_provider_context(provider_name.to_string(), model_id.clone())
                            .with_max_tokens(infer.max_tokens)
                            .with_workflow_dir(self.workflow_base_dir.clone());

                    // Wire repair_model: use a different (cheaper) model for Layer 4 repair
                    // Resolve templates (e.g. "{{inputs.fast_model}}")
                    let resolved_repair_model = match &spec.repair_model {
                        Some(m) => Some(template_resolve(m, bindings, datastore)?.into_owned()),
                        None => None,
                    };
                    if let Some(ref repair_model_name) = resolved_repair_model {
                        let trimmed = repair_model_name.trim();
                        if trimmed.is_empty() {
                            tracing::warn!(
                                task_id = %task_id,
                                "repair_model resolved to empty string, using default model for repair"
                            );
                        } else {
                            let repair_callback =
                                Self::make_infer_callback(&provider, Some(trimmed));
                            engine = engine
                                .with_repair_callback(repair_callback)
                                .with_repair_model_name(trimmed.to_string());
                        }
                    }

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

    /// Layer 0a: attempt native `response_format: json_schema` structured output.
    ///
    /// For providers that support `response_format: json_schema` (OpenAI, Groq,
    /// DeepSeek, xAI, custom OpenAI-compat endpoints), use the provider's native
    /// structured output instead of tool injection. This avoids the MaxTurnError
    /// that `tool_choice: Required` causes.
    ///
    /// Returns `(attempted, result)`:
    /// - `(false, None)` — provider does not support it, skip
    /// - `(true, Some(json))` — attempted and succeeded
    /// - `(true, None)` — attempted but failed validation, fall through
    #[allow(clippy::too_many_arguments)]
    async fn try_layer_0a_response_format(
        &self,
        provider: &RigProvider,
        model_id: &str,
        prompt: &str,
        infer: &InferParams,
        policy: &OutputPolicy,
        schema_value: &Value,
        l0_infer_callback: &InferCallback,
        task_id: &Arc<str>,
        provider_name: &str,
        resolved_system: &Option<String>,
        token_reservation: &mut crate::runtime::policy::TokenReservation,
        cached_example: &Option<Value>,
    ) -> Result<(bool, Option<String>), NikaError> {
        if !provider.supports_native_structured_output() {
            return Ok((false, None));
        }

        debug!(
            task_id = %task_id,
            provider = %provider_name,
            "Layer 0a: using native response_format for structured output"
        );

        let rf_params = build_response_format_params(schema_value);
        // Non-streaming structured output path: drain receiver to avoid SendError warnings
        let (tx_rf, mut rx_rf) = mpsc::channel::<StreamChunk>(32);
        tokio::spawn(async move { while rx_rf.recv().await.is_some() {} });
        let rf_options = InferOptions {
            model: Some(model_id.to_string()),
            temperature: infer.temperature,
            max_tokens: infer.max_tokens,
            system: resolved_system.clone(),
            additional_params: Some(rf_params),
        };

        match provider
            .infer_stream_with_options(prompt, tx_rf, &rf_options)
            .await
        {
            Ok(stream_result) => {
                // Validate through StructuredOutputEngine as safety net
                if let Some(spec) = policy.to_structured_spec() {
                    let spec = resolve_from_example_in_spec(spec, cached_example);
                    let mut engine =
                        StructuredOutputEngine::new(spec, Arc::new(self.event_log.clone()))
                            .with_infer_callback(l0_infer_callback.clone())
                            .with_original_prompt(prompt.to_string())
                            .with_provider_context(provider_name.to_string(), model_id.to_string())
                            .with_max_tokens(infer.max_tokens);

                    match engine.validate(task_id.as_ref(), &stream_result.text).await {
                        Ok(result) => {
                            self.event_log.emit(EventKind::StructuredOutputAttempt {
                                task_id: Arc::clone(task_id),
                                layer: 0,
                                layer_name: "response_format".to_string(),
                                attempt: 1,
                                success: true,
                                error: None,
                            });
                            let result_str =
                                super::verbs::strip_think_tags(&result.value.to_string());
                            let cost = provider
                                .cost_provider_kind()
                                .map(|pk| {
                                    crate::provider::cost::calculate_cost_with_cache(
                                        pk,
                                        model_id,
                                        stream_result.input_tokens,
                                        stream_result.output_tokens,
                                        stream_result.cached_input_tokens,
                                    )
                                })
                                .unwrap_or(0.0);
                            emit_provider_responded(
                                &self.event_log,
                                task_id,
                                stream_result.request_id.clone(),
                                stream_result.input_tokens,
                                stream_result.output_tokens,
                                stream_result.cached_input_tokens,
                                stream_result.ttft_ms,
                                stream_result
                                    .finish_reason
                                    .clone()
                                    .unwrap_or(nika_event::FinishReason::Stop),
                                if cost.is_finite() { cost } else { 0.0 },
                            );
                            token_reservation
                                .adjust(stream_result.input_tokens + stream_result.output_tokens);
                            return Ok((true, Some(result_str)));
                        }
                        Err(e) => {
                            debug!(
                                task_id = %task_id,
                                error = %e,
                                "Layer 0a: response_format result failed validation, falling through"
                            );
                            self.event_log.emit(EventKind::StructuredOutputAttempt {
                                task_id: Arc::clone(task_id),
                                layer: 0,
                                layer_name: "response_format".to_string(),
                                attempt: 1,
                                success: false,
                                error: Some(e.to_string()),
                            });
                        }
                    }
                } else {
                    // No spec — use the raw text
                    self.event_log.emit(EventKind::StructuredOutputAttempt {
                        task_id: Arc::clone(task_id),
                        layer: 0,
                        layer_name: "response_format".to_string(),
                        attempt: 1,
                        success: true,
                        error: None,
                    });
                    token_reservation
                        .adjust(stream_result.input_tokens + stream_result.output_tokens);
                    // BUGFIX SF2: Emit ProviderResponded before early return
                    let cost = provider
                        .cost_provider_kind()
                        .map(|pk| {
                            crate::provider::cost::calculate_cost_with_cache(
                                pk,
                                model_id,
                                stream_result.input_tokens,
                                stream_result.output_tokens,
                                stream_result.cached_input_tokens,
                            )
                        })
                        .unwrap_or(0.0);
                    emit_provider_responded(
                        &self.event_log,
                        task_id,
                        stream_result.request_id.clone(),
                        stream_result.input_tokens,
                        stream_result.output_tokens,
                        stream_result.cached_input_tokens,
                        stream_result.ttft_ms,
                        stream_result
                            .finish_reason
                            .clone()
                            .unwrap_or(nika_event::FinishReason::Stop),
                        if cost.is_finite() { cost } else { 0.0 },
                    );
                    return Ok((true, Some(stream_result.text)));
                }
            }
            Err(e) => {
                debug!(
                    task_id = %task_id,
                    error = %e,
                    "Layer 0a: response_format failed, falling through to tool injection"
                );
                self.event_log.emit(EventKind::StructuredOutputAttempt {
                    task_id: Arc::clone(task_id),
                    layer: 0,
                    layer_name: "response_format".to_string(),
                    attempt: 1,
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }

        Ok((true, None))
    }

    /// Layer 0b: tool injection via DynamicSubmitTool.
    ///
    /// Forces the LLM to call `submit_result()` with schema-compliant JSON by
    /// injecting a tool whose parameters match the target schema. If it succeeds
    /// the result is still validated through `StructuredOutputEngine` as a safety
    /// net (L2-L4 are available via the `l0_infer_callback`).
    ///
    /// Returns:
    /// - `Ok(Some(json))` — tool injection succeeded and validated
    /// - `Ok(None)` — failed or not supported, fall through to streaming
    #[allow(clippy::too_many_arguments)]
    async fn try_layer_0b_tool_injection(
        &self,
        provider: &RigProvider,
        model_id: &str,
        prompt: &str,
        infer: &InferParams,
        policy: &OutputPolicy,
        schema_value: &Value,
        l0_infer_callback: &InferCallback,
        task_id: &Arc<str>,
        provider_name: &str,
        resolved_system: &Option<String>,
        token_reservation: &mut crate::runtime::policy::TokenReservation,
        cached_example: &Option<Value>,
    ) -> Result<Option<String>, NikaError> {
        let submit_tool = crate::runtime::submit_tool::DynamicSubmitTool::new(schema_value.clone());
        let tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![Box::new(submit_tool)];

        debug!(
            task_id = %task_id,
            "Layer 0b: attempting tool injection via DynamicSubmitTool"
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
                prompt,
                tools,
                Some(model_id),
                infer.max_tokens,
                resolved_system.as_deref(),
            )
            .await
        {
            Ok((tool_result, api_prompt_tokens, api_completion_tokens)) => {
                debug!(
                    task_id = %task_id,
                    result_len = tool_result.len(),
                    "Layer 0: tool injection succeeded"
                );

                // Still validate through the engine as safety net
                if let Some(spec) = policy.to_structured_spec() {
                    let spec = resolve_from_example_in_spec(spec, cached_example);
                    let mut engine =
                        StructuredOutputEngine::new(spec, Arc::new(self.event_log.clone()))
                            .with_infer_callback(l0_infer_callback.clone())
                            .with_original_prompt(prompt.to_string())
                            .with_provider_context(provider_name.to_string(), model_id.to_string())
                            .with_max_tokens(infer.max_tokens);

                    match engine.validate(task_id.as_ref(), &tool_result).await {
                        Ok(result) => {
                            // Emit success ONLY after validation passes
                            self.event_log.emit(EventKind::StructuredOutputAttempt {
                                task_id: Arc::clone(task_id),
                                layer: 0,
                                layer_name: "tool_injection".to_string(),
                                attempt: 1,
                                success: true,
                                error: None,
                            });
                            let result_str =
                                super::verbs::strip_think_tags(&result.value.to_string());
                            // Use real tokens from API when available (OpenAiCompat),
                            // fall back to estimation for rig-core providers (tokens=0)
                            let (est_in, est_out) =
                                if api_prompt_tokens > 0 || api_completion_tokens > 0 {
                                    (api_prompt_tokens, api_completion_tokens)
                                } else {
                                    (
                                        estimate_tokens(prompt.len()),
                                        estimate_tokens(result_str.len()),
                                    )
                                };
                            let cost = provider
                                .cost_provider_kind()
                                .map(|pk| {
                                    crate::provider::cost::calculate_cost_with_cache(
                                        pk, model_id, est_in, est_out,
                                        0, // No cache info for non-streaming
                                    )
                                })
                                .unwrap_or(0.0);
                            emit_provider_responded(
                                &self.event_log,
                                task_id,
                                None,
                                est_in,
                                est_out,
                                0,
                                None,
                                // L0b: non-streaming, finish reason unavailable
                                nika_event::FinishReason::Stop,
                                if cost.is_finite() { cost } else { 0.0 },
                            );
                            debug!(
                                task_id = %task_id,
                                layer = result.layer,
                                "Layer 0 + validation succeeded"
                            );
                            // Adjust token reservation before early return
                            token_reservation.adjust(est_in + est_out);
                            return Ok(Some(result_str));
                        }
                        Err(e) => {
                            // Emit failure when validation rejects tool output
                            self.event_log.emit(EventKind::StructuredOutputAttempt {
                                task_id: Arc::clone(task_id),
                                layer: 0,
                                layer_name: "tool_injection".to_string(),
                                attempt: 1,
                                success: false,
                                error: Some(e.to_string()),
                            });
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
                    // Use real tokens from API when available (OpenAiCompat),
                    // fall back to estimation for rig-core providers (tokens=0)
                    let (est_in, est_out) = if api_prompt_tokens > 0 || api_completion_tokens > 0 {
                        (api_prompt_tokens, api_completion_tokens)
                    } else {
                        (
                            estimate_tokens(prompt.len()),
                            estimate_tokens(tool_result.len()),
                        )
                    };
                    let cost = provider
                        .cost_provider_kind()
                        .map(|pk| {
                            crate::provider::cost::calculate_cost_with_cache(
                                pk, model_id, est_in, est_out,
                                0, // No cache info for non-streaming
                            )
                        })
                        .unwrap_or(0.0);
                    emit_provider_responded(
                        &self.event_log,
                        task_id,
                        None,
                        est_in,
                        est_out,
                        0,
                        None,
                        // L0b tool: non-streaming, finish reason unavailable
                        nika_event::FinishReason::Stop,
                        if cost.is_finite() { cost } else { 0.0 },
                    );
                    // Adjust token reservation before early return
                    token_reservation.adjust(est_in + est_out);
                    return Ok(Some(tool_result));
                }
            }
            Err(e) => {
                // BUG 10: MaxTurnError(0) is an expected skip, not a real error.
                // This happens when the provider doesn't support tool_choice
                // or when structured output uses a fast-path bypass.
                let err_str = e.to_string();
                let is_expected_skip =
                    err_str.contains("MaxTurnError") || err_str.contains("max turn limit: 0");
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

        Ok(None)
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
        _provider_name: &str,
        model_id: &str,
        resolved_system: Option<&str>,
        token_reservation: &mut crate::runtime::policy::TokenReservation,
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
                            match result {
                                Ok(data) => data,
                                Err(e) => {
                                    self.event_log.emit(EventKind::VisionContentFailed {
                                        task_id: Arc::clone(task_id),
                                        source: resolved_source.clone(),
                                        stage: "cas_read".to_string(),
                                        error: e.to_string(),
                                    });
                                    return Err(ProviderError::ApiError {
                                        message: format!("Vision: CAS read '{}': {}", resolved_source, e),
                                    }.into());
                                }
                            }
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
                        self.event_log.emit(EventKind::VisionContentFailed {
                            task_id: Arc::clone(task_id),
                            source: resolved_source.clone(),
                            stage: "unsupported_format".to_string(),
                            error: "Supported: PNG, JPEG, GIF, WebP".to_string(),
                        });
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
                    // SECURITY: scheme check
                    if !resolved_url.starts_with("https://") && !resolved_url.starts_with("http://")
                    {
                        return Err(NikaError::ValidationError {
                            reason: format!(
                                "image_url must use http(s)://, got: {}",
                                &resolved_url.chars().take(50).collect::<String>()
                            ),
                        });
                    }
                    // SECURITY: SSRF protection — block internal/metadata endpoints
                    let parsed =
                        url::Url::parse(&resolved_url).map_err(|e| NikaError::ValidationError {
                            reason: format!("ImageUrl is not a valid URL: {}", e),
                        })?;
                    let host = parsed
                        .host_str()
                        .ok_or_else(|| NikaError::ValidationError {
                            reason: "ImageUrl has no host".to_string(),
                        })?;
                    let h = host.to_lowercase();
                    let h_normalized = h.trim_start_matches('[').trim_end_matches(']');
                    if crate::runtime::policy::is_ssrf_blocked(h_normalized) {
                        return Err(NikaError::FetchError {
                            reason: format!(
                                "SSRF protection: image_url host '{}' is blocked",
                                h_normalized
                            ),
                        });
                    }
                    // DNS rebinding check — resolve hostname and verify resolved IP
                    if crate::runtime::policy::resolve_and_check_ssrf(h_normalized).await {
                        return Err(NikaError::FetchError {
                            reason: format!(
                                "SSRF protection: image_url host '{}' resolves to blocked IP (DNS rebinding)",
                                h_normalized
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

        let vision_work = provider.infer_vision(
            user_content,
            Some(model_id),
            resolved_system,
            infer.max_tokens,
        );
        let vision_result = tokio::select! {
            result = vision_work => {
                result.map_err(|e| ProviderError::ApiError { message: e.to_string() })?
            }
            _ = self.cancel_token.cancelled() => {
                return Err(NikaError::TaskCancelled {
                    task_id: task_id.to_string(),
                    reason: "cancelled during vision inference".to_string(),
                });
            }
        };

        // Strip <think> blocks from reasoning models (Qwen, DeepSeek-R1)
        let vision_result = super::verbs::strip_think_tags(&vision_result);

        // Vision token estimate: text tokens + approximate image tokens.
        // Image tokens vary by provider/resolution, but ~85 tokens per 512×512
        // tile is a reasonable approximation (1 token per ~750 bytes).
        let image_tokens: usize = if total_bytes > 0 {
            ((total_bytes / 750).max(85)) as usize
        } else {
            0
        };
        let est_in = estimate_tokens(prompt.len()) + image_tokens as u64;
        let est_out = estimate_tokens(vision_result.len());
        token_reservation.adjust(est_in + est_out);

        let cost = provider
            .cost_provider_kind()
            .map(|pk| {
                crate::provider::cost::calculate_cost_with_cache(
                    pk, model_id, est_in, est_out, 0, // Vision: no cache info available yet
                )
            })
            .unwrap_or(0.0);

        emit_provider_responded(
            &self.event_log,
            task_id,
            None,
            est_in,
            est_out,
            0,
            None,
            // Vision path: no streaming, finish reason unavailable
            nika_event::FinishReason::Stop,
            if cost.is_finite() { cost } else { 0.0 },
        );

        // Run guardrails on vision output (same as non-vision path)
        self.check_infer_guardrails(task_id, infer, &vision_result)?;

        Ok(vision_result)
    }

    /// Run guardrails configured on an infer task against the output.
    ///
    /// Generate the generic mock JSON response (used when no structured schema).
    fn generic_mock_json(
        task_id: &Arc<str>,
        prompt: &str,
        vision_info: &serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
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
        })
    }

    /// SECURITY: Validate schema/from_example file paths against directory traversal.
    /// Rejects `..` components to prevent escaping the workflow directory
    /// (e.g., `../../.env`, `../../../etc/passwd`).
    fn validate_schema_path(path: &str) -> Result<(), NikaError> {
        let p = std::path::Path::new(path);
        if p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(NikaError::SchemaFailed {
                details: format!(
                    "Path traversal ('..') not allowed in schema/from_example: '{path}'"
                ),
            });
        }
        Ok(())
    }

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
                    guardrail_type: nika_event::GuardrailType::parse(&result.guardrail_type)
                        .unwrap_or(nika_event::GuardrailType::Regex),
                    description: result.guardrail_id.clone(),
                });
            } else {
                self.event_log.emit(EventKind::GuardrailFailed {
                    task_id: Arc::clone(task_id),
                    guardrail_type: nika_event::GuardrailType::parse(&result.guardrail_type)
                        .unwrap_or(nika_event::GuardrailType::Regex),
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

/// Replace a file-based `from_example` with the already-loaded inline value.
/// This prevents `StructuredOutputEngine` from re-reading the file using a
/// potentially unresolved template path (BUG-1: from_example template resolution).
fn resolve_from_example_in_spec(
    mut spec: StructuredOutputSpec,
    cached_example: &Option<Value>,
) -> StructuredOutputSpec {
    if let Some(ref example) = cached_example {
        spec.from_example = Some(SchemaRef::Inline(example.clone()));
    }
    spec
}

/// Parse slash syntax in model: field.
/// `model: "groq/llama-3.3-70b"` → Some(("groq", "llama-3.3-70b"))
/// Splits on FIRST slash only: `"native/Qwen/Qwen3-8B"` → Some(("native", "Qwen/Qwen3-8B"))
fn parse_model_slash(model: &str) -> Option<(&str, &str)> {
    let slash_pos = model.find('/')?;
    let prefix = &model[..slash_pos];
    let rest = &model[slash_pos + 1..];
    if prefix.is_empty() || rest.is_empty() {
        return None;
    }
    Some((prefix, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_validate_schema_path_rejects_traversal() {
        assert!(TaskExecutor::validate_schema_path("../../.env").is_err());
        assert!(TaskExecutor::validate_schema_path("../secrets/key").is_err());
        assert!(TaskExecutor::validate_schema_path("schemas/../../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_schema_path_allows_safe_paths() {
        assert!(TaskExecutor::validate_schema_path("schema.json").is_ok());
        assert!(TaskExecutor::validate_schema_path("./schemas/user.json").is_ok());
        assert!(TaskExecutor::validate_schema_path("config/nested/deep.json").is_ok());
    }

    #[test]
    fn test_generic_mock_json_structure() {
        let task_id: Arc<str> = Arc::from("test-task");
        let vision_info = serde_json::json!({ "vision": false });
        let result = TaskExecutor::generic_mock_json(&task_id, "test prompt", &vision_info);

        assert_eq!(result["mock"], true);
        assert_eq!(result["task_id"], "test-task");
        assert!(result["name"].is_string());
        assert!(result["age"].is_number());
        assert!(result["items"].is_array());
        assert!(result["user"]["name"].is_string());
        assert!(result["metadata"]["version"].is_number());
    }

    #[test]
    fn test_generic_mock_json_tracks_prompt_length() {
        let task_id: Arc<str> = Arc::from("t");
        let vision = serde_json::json!({ "vision": false });
        let prompt = "a".repeat(500);
        let result = TaskExecutor::generic_mock_json(&task_id, &prompt, &vision);
        assert_eq!(result["prompt_len"], 500);
    }

    #[test]
    fn test_generic_mock_json_includes_vision_info() {
        let task_id: Arc<str> = Arc::from("t");
        let vision = serde_json::json!({ "vision": true, "image_count": 2 });
        let result = TaskExecutor::generic_mock_json(&task_id, "prompt", &vision);
        assert_eq!(result["vision_info"]["vision"], true);
        assert_eq!(result["vision_info"]["image_count"], 2);
    }

    #[test]
    fn test_estimate_tokens_approximation() {
        // 4 chars per token is the standard heuristic
        assert_eq!(estimate_tokens(400), 100);
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(3), 1); // rounds up
    }

    #[test]
    fn test_parse_model_slash_basic() {
        assert_eq!(
            parse_model_slash("groq/llama-3.3-70b"),
            Some(("groq", "llama-3.3-70b"))
        );
    }

    #[test]
    fn test_parse_model_slash_nested_path() {
        // Split on FIRST slash: native/Qwen/Qwen3-8B → provider=native, model=Qwen/Qwen3-8B
        assert_eq!(
            parse_model_slash("native/Qwen/Qwen3-8B"),
            Some(("native", "Qwen/Qwen3-8B"))
        );
    }

    #[test]
    fn test_parse_model_slash_no_slash() {
        assert_eq!(parse_model_slash("claude-sonnet-4-6"), None);
    }

    #[test]
    fn test_parse_model_slash_empty_parts() {
        assert_eq!(parse_model_slash("/llama"), None); // empty prefix
        assert_eq!(parse_model_slash("groq/"), None); // empty model
    }

    #[test]
    fn test_parse_model_slash_named_endpoint() {
        // Named endpoint from nika.toml: model: "h100/Qwen/Qwen3-8B"
        assert_eq!(
            parse_model_slash("h100/Qwen/Qwen3-8B"),
            Some(("h100", "Qwen/Qwen3-8B"))
        );
        // Ollama-style single model
        assert_eq!(
            parse_model_slash("ollama/llama3.2"),
            Some(("ollama", "llama3.2"))
        );
    }
}
