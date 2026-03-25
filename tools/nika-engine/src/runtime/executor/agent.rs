//! Agent verb implementation for TaskExecutor
//!
//! Contains `run_agent` for multi-turn agentic execution loops.

use futures::FutureExt;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use tracing::{debug, instrument};

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

        // Inject JSON schema instruction if output policy requires JSON with schema
        if let Some(schema_instruction) = Self::build_json_schema_instruction(output_policy) {
            resolved_prompt.push_str(&schema_instruction);
            debug!(task_id = %task_id, "Injected JSON schema instruction into agent prompt");
        }

        // EMIT: TemplateResolved (redacted to avoid leaking secrets)
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: agent.prompt.clone(),
            result: redact_for_event(&resolved_prompt),
        });

        // Create agent params with resolved prompt
        let resolved_agent = AgentParams {
            prompt: resolved_prompt,
            ..agent.clone()
        };

        // Validate agent params
        resolved_agent.validate()?;

        // Atomic reserve tokens for budget (prevents TOCTOU with concurrent for_each)
        let estimated_tokens: u64 = resolved_agent
            .token_budget
            .map(u64::from)
            .unwrap_or_else(|| estimate_tokens(resolved_agent.prompt.len()) as u64);
        if let Err(reason) = self
            .policy_enforcer
            .write()
            .reserve_tokens(estimated_tokens)
        {
            tracing::warn!(
                task_id = %task_id,
                estimated_tokens = estimated_tokens,
                reason = %reason,
                "agent: blocked by token budget"
            );
            return Err(NikaError::PolicyViolation { reason });
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

        // Ensure resolved_agent has provider + model set for run_auto() dispatch
        let resolved_agent = AgentParams {
            provider: Some(provider_name.clone()),
            model: resolved_agent
                .model
                .clone()
                .or_else(|| self.default_model.as_ref().map(|m| m.to_string())),
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
                self.skills_map.clone(),
                self.workflow_base_dir.clone(),
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

        // Adjust reservation with actual agent token usage
        self.policy_enforcer
            .write()
            .adjust_reservation(estimated_tokens, result.total_tokens as u64);

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
