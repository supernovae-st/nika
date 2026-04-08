//! Agent verb implementation for TaskExecutor
//!
//! Contains `run_agent` for multi-turn agentic execution loops.

use futures::FutureExt;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use tracing::{debug, instrument, warn};

use crate::ast::output::OutputPolicy;
use crate::ast::AgentParams;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::EventKind;
use crate::mcp::McpClient;
use crate::runtime::RigAgentLoop;
use crate::store::RunContext;

use super::verbs::{estimate_tokens, redact_for_event};
use super::TaskExecutor;

impl TaskExecutor {
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

        // Pre-read file-based from_example for prompt injection
        // Template-resolve path first (e.g. from_example: "cache/{{inputs.locale}}.json")
        let cached_example = if let Some(policy) = output_policy {
            if let Some(crate::ast::output::SchemaRef::File(ref path)) = policy.from_example {
                let resolved_path = if path.contains("{{") {
                    template_resolve(path, bindings, datastore)?.into_owned()
                } else {
                    path.clone()
                };
                match tokio::fs::read_to_string(&resolved_path).await {
                    Ok(content) => match serde_json::from_str(&content) {
                        Ok(value) => Some(value),
                        Err(e) => {
                            warn!(
                                task_id = %task_id,
                                path = %resolved_path,
                                error = %e,
                                "from_example file contains invalid JSON, ignoring"
                            );
                            None
                        }
                    },
                    Err(e) => {
                        debug!(task_id = %task_id, "Failed to pre-read from_example '{}': {}", resolved_path, e);
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
            resolved_prompt.push_str(&schema_instruction);
            debug!(task_id = %task_id, "Injected JSON schema instruction into agent prompt");
        }

        // EMIT: TemplateResolved (redacted to avoid leaking secrets)
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: agent.prompt.clone(),
            result: redact_for_event(&resolved_prompt),
        });

        // Resolve {{with.alias}} templates in system prompt
        let resolved_system = match &agent.system {
            Some(sys) => Some(template_resolve(sys, bindings, datastore)?.into_owned()),
            None => None,
        };

        // ── Nika Shield: agent tool restriction (Sprint 2 Item 3a) ─────────
        // Compute the policy once at executor level, apply at agent dispatch.
        // Replaces the (has_untrusted, elevated) boolean pair with a state-
        // collapsed enum (R4) so the impossible "elevated AND restricted"
        // combination is unrepresentable.
        let task_trust_elevated = crate::runtime::builtin::run::current_task_elevated();
        let has_untrusted = self.agent_has_untrusted_inputs(bindings, datastore);
        let dangerous: Arc<[String]> = Arc::from(self.shield.dangerous_tools());
        let tool_policy = nika_core::capabilities::AgentToolPolicy::for_task(
            has_untrusted,
            task_trust_elevated,
            Arc::clone(&dangerous),
        );
        let (kept_tools, removed_tools) = tool_policy.apply_to(agent.tools.clone());
        for removed in &removed_tools {
            self.event_log.emit(EventKind::AgentToolRestricted {
                task_id: Arc::clone(task_id),
                removed_tool: removed.clone(),
                reason: "task has untrusted inputs and is not trust: elevated".to_string(),
            });
        }

        // Create agent params with resolved prompt, system, and the
        // shield-filtered tool list.
        let resolved_agent = AgentParams {
            prompt: resolved_prompt,
            system: resolved_system,
            tools: kept_tools,
            ..agent.clone()
        };

        // Validate agent params
        resolved_agent.validate()?;

        // RAII token reservation (W6.4): if the agent is cancelled or panics
        // before adjust(), Drop releases the reserved tokens back to the budget.
        let estimated_tokens: u64 = resolved_agent
            .token_budget
            .map(u64::from)
            .unwrap_or_else(|| estimate_tokens(resolved_agent.prompt.len()) as u64);
        let mut token_reservation = crate::runtime::policy::TokenReservation::new(
            Arc::clone(&self.policy_enforcer),
            estimated_tokens,
        )
        .map_err(|reason| {
            tracing::warn!(
                task_id = %task_id,
                estimated_tokens = estimated_tokens,
                reason = %reason,
                "agent: blocked by token budget"
            );
            NikaError::PolicyViolation { reason }
        })?;

        // EMIT: AgentStart event
        self.event_log.emit(EventKind::AgentStart {
            task_id: Arc::clone(task_id),
            max_turns: resolved_agent.effective_max_turns(),
            mcp_servers: resolved_agent.mcp.clone(),
        });

        // Resolve templates in model and provider fields
        let resolved_model = match &resolved_agent.model {
            Some(m) => Some(template_resolve(m, bindings, datastore)?.into_owned()),
            None => None,
        };
        let resolved_provider: Option<nika_core::ProviderName> = match &resolved_agent.provider {
            Some(p) => {
                // Template-resolve the provider string, then parse into ProviderName
                let resolved_str = template_resolve(p.as_str(), bindings, datastore)?.into_owned();
                Some(nika_core::ProviderName::parse(&resolved_str))
            }
            None => None,
        };

        // Parse slash syntax: model: "groq/llama-3.3-70b" → provider=groq, model=llama-3.3-70b
        let (resolved_provider, resolved_model) = if resolved_provider.is_none() {
            if let Some(ref model_str) = resolved_model {
                if let Some(slash_pos) = model_str.find('/') {
                    let prefix = &model_str[..slash_pos];
                    let rest = &model_str[slash_pos + 1..];
                    if !rest.is_empty() && !prefix.is_empty() {
                        (
                            Some(nika_core::ProviderName::parse(prefix)),
                            Some(rest.to_string()),
                        )
                    } else {
                        (resolved_provider, resolved_model)
                    }
                } else {
                    (resolved_provider, resolved_model)
                }
            } else {
                (resolved_provider, resolved_model)
            }
        } else {
            (resolved_provider, resolved_model)
        };

        // Build provider chain: explicit chain > single provider > workflow default
        let effective_chain: Vec<String> = if let Some(ref chain) = resolved_agent.provider_chain {
            chain.iter().map(|p| p.to_string()).collect()
        } else {
            vec![resolved_provider
                .as_ref()
                .map(|p| p.to_string())
                .unwrap_or_else(|| self.default_provider.to_string())]
        };

        // Resolve provider with fallback chain support.
        // Try each provider in the chain; validate availability (API key / custom endpoint).
        // On failure, emit ProviderFallback event and try the next provider.
        let (provider_name, provider_chain_idx): (String, usize) = {
            let mut last_error: Option<String> = None;
            let mut found_provider: Option<(String, usize)> = None;

            for (i, name) in effective_chain.iter().enumerate() {
                if name == "mock" {
                    found_provider = Some((name.clone(), i));
                    break;
                }
                // Check custom endpoints first
                if self.custom_endpoints.contains_key(name.as_str()) {
                    found_provider = Some((name.clone(), i));
                    break;
                }
                // Check catalog provider has env key
                match crate::core::find_provider(name) {
                    Some(p) if crate::secrets::has_provider_key(p) || !p.requires_key => {
                        found_provider = Some((name.clone(), i));
                        break;
                    }
                    Some(p) => {
                        let err_msg = format!("Missing API key: {} not set", p.env_var);
                        if effective_chain.len() > 1 && i < effective_chain.len() - 1 {
                            self.event_log.emit(EventKind::ProviderFallback {
                                task_id: Arc::clone(task_id),
                                from: name.clone(),
                                to: effective_chain[i + 1].clone(),
                                reason: err_msg.clone(),
                            });
                            tracing::warn!(
                                task_id = %task_id,
                                from = %name,
                                to = %effective_chain[i + 1],
                                "Agent provider fallback: {} → {}",
                                name,
                                effective_chain[i + 1]
                            );
                        }
                        last_error = Some(err_msg);
                    }
                    None => {
                        let err_msg = format!("Unknown provider: '{}'", name);
                        if effective_chain.len() > 1 && i < effective_chain.len() - 1 {
                            self.event_log.emit(EventKind::ProviderFallback {
                                task_id: Arc::clone(task_id),
                                from: name.clone(),
                                to: effective_chain[i + 1].clone(),
                                reason: err_msg.clone(),
                            });
                            tracing::warn!(
                                task_id = %task_id,
                                from = %name,
                                to = %effective_chain[i + 1],
                                "Agent provider fallback: {} → {}",
                                name,
                                effective_chain[i + 1]
                            );
                        }
                        last_error = Some(err_msg);
                    }
                }
            }

            match found_provider {
                Some(found) => found,
                None => {
                    if effective_chain.len() > 1 {
                        return Err(NikaError::FallbackChainExhausted {
                            providers: effective_chain.join(", "),
                            last_error: last_error.unwrap_or_default(),
                        });
                    } else {
                        return Err(NikaError::MissingApiKey {
                            provider: effective_chain.first().cloned().unwrap_or_default(),
                        });
                    }
                }
            }
        };

        // Resolve model via ModelResolver (task > workflow > provider default)
        // provider_chain_idx > 0 triggers incompatibility check for fallback substitution
        let resolved_model_id = nika_core::catalogs::ModelResolver::resolve(
            resolved_model.as_deref(),
            self.default_model.as_deref(),
            &provider_name,
            provider_chain_idx,
            resolved_model.as_deref(),
        )
        .model_id;

        // Ensure resolved_agent has provider + model set for run_auto() dispatch
        let resolved_agent = AgentParams {
            provider: Some(nika_core::ProviderName::parse(&provider_name)),
            model: Some(resolved_model_id),
            ..resolved_agent
        };

        // Build MCP client map for this agent
        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        for mcp_name in &resolved_agent.mcp {
            let client = self.get_mcp_client(mcp_name).await?;
            mcp_clients.insert(mcp_name.clone(), client);
        }

        // Create rig-based agent loop (with policy enforcement for tool calls)
        // Pass workflow_base_dir so agent's builtin file tools (glob, read, etc.)
        // operate in the workflow's directory, not the process cwd.
        let agent_loop = RigAgentLoop::new(
            task_id.to_string(),
            resolved_agent,
            self.event_log.clone(),
            mcp_clients,
            Some(Arc::clone(&self.policy_enforcer)),
            Some(self.workflow_base_dir.clone()),
        )?;

        // Wire skill injection if the agent has skills and the workflow defines a skills map
        let agent_loop = if agent
            .skills
            .as_ref()
            .is_some_and(|s: &Vec<String>| !s.is_empty())
            && !self.skills_map.is_empty()
        {
            debug!(
                task_id = %task_id,
                skills = ?agent.skills,
                "Wiring skill injection into agent loop"
            );
            agent_loop.with_skills(
                Arc::clone(&self.skill_injector),
                (*self.skills_map).clone(),
                self.skills_base_dir.clone(),
            )
        } else {
            agent_loop
        };

        // Inject DynamicSubmitTool if structured output is configured.
        // For agent verb, submit_result is available but NOT forced --
        // the agent calls it when ready (unlike infer: which uses tool_choice: Required).
        let mut agent_loop = if let Some(policy) = output_policy {
            if policy.is_structured() {
                if let Some(schema_ref) = &policy.schema {
                    let schema_value = match schema_ref {
                        crate::ast::output::SchemaRef::Inline(v) => Some(v.clone()),
                        crate::ast::output::SchemaRef::File(path) => {
                            let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                                NikaError::ValidationError {
                                    reason: format!("Failed to read schema file '{}': {}", path, e),
                                }
                            })?;
                            let v: serde_json::Value =
                                serde_json::from_str(&content).map_err(|e| {
                                    NikaError::ValidationError {
                                        reason: format!(
                                            "Invalid JSON in schema file '{}': {}",
                                            path, e
                                        ),
                                    }
                                })?;
                            Some(v)
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
        // mock provider uses run_mock(), real providers use run() which dispatches
        // based on AgentParams.provider
        let result = if provider_name.as_str() == "mock" {
            agent_loop.run_mock().await?
        } else {
            // Use run() which dispatches to the right rig-core client
            // based on the provider field we just set.
            // Wrap in catch_unwind to convert rig-core panics to NikaErrors.
            let run_future = agent_loop.run();
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

        // Adjust reservation with actual agent token usage
        token_reservation.adjust(result.total_tokens as u64);

        // Log routing status for observability
        if result.status.is_flagged() {
            tracing::warn!(
                task_id = %task_id,
                confidence = ?result.status.confidence(),
                "Agent completed with FlaggedForReview — output accepted but flagged for human review"
            );
        }
        if result.status.is_escalated() {
            tracing::warn!(
                task_id = %task_id,
                confidence = ?result.status.confidence(),
                "Agent escalated — confidence too low, result may be unreliable"
            );
        }

        // EMIT: AgentComplete event
        self.event_log.emit(EventKind::AgentComplete {
            task_id: Arc::clone(task_id),
            turns: result.turns as u32,
            stop_reason: nika_event::AgentStopReason::from(format!("{:?}", result.status)),
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
        // Agent returns {"response": <value>} -- value may be a string, object, array, number, etc.
        // Strings are returned as-is (no extra quotes), other types are serialized to JSON.
        // If "response" key is missing, fall back to the entire final_output.
        let response = match result.final_output.get("response") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => {
                // No "response" key -- serialize the entire final_output if non-empty
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
