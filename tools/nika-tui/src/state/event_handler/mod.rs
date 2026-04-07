//! Event handler for TuiState
//!
//! Dispatches runtime events to category-specific handler methods.
//!
//! ## Module Structure
//!
//! - `workflow` - WorkflowStarted/Completed/Failed/Aborted/Paused/Resumed
//! - `task` - TaskScheduled/Started/Completed/Failed/Skipped
//! - `agent` - AgentStart/Turn/Complete/Spawned, ContextAssembled, TemplateResolved
//! - `provider` - ProviderCalled/Responded, McpInvoke/Response/Connected/Error/Retry
//! - `telemetry` - Log, Custom, Artifact*, StructuredOutput*, Guardrail*, Http*, Media*, Vision

mod agent;
mod provider;
mod task;
mod telemetry;
mod workflow;

use nika_engine::event::EventKind;

use super::types::{FRAME_CYCLE, FRAME_DIV_GLACIAL, FRAME_DIV_NORMAL};
use super::TuiState;

/// Maximum number of entries in token_history and latency_history sparklines
const MAX_HISTORY_ENTRIES: usize = 50;

impl TuiState {
    /// Handle an event from the runtime
    pub fn handle_event(&mut self, kind: &EventKind, timestamp_ms: u64) {
        match kind {
            // ═══════════════════════════════════════════
            // WORKFLOW EVENTS
            // ═══════════════════════════════════════════
            EventKind::WorkflowStarted {
                task_count,
                generation_id,
                ..
            } => {
                self.on_workflow_started(*task_count, generation_id);
            }

            EventKind::WorkflowCompleted {
                final_output,
                total_duration_ms,
            } => {
                self.on_workflow_completed(final_output, *total_duration_ms, timestamp_ms);
            }

            EventKind::WorkflowFailed { error, .. } => {
                self.on_workflow_failed(error, timestamp_ms);
            }

            EventKind::WorkflowAborted {
                reason,
                duration_ms,
                running_tasks,
            } => {
                self.on_workflow_aborted(reason, *duration_ms, running_tasks, timestamp_ms);
            }

            EventKind::WorkflowPaused => {
                self.on_workflow_paused(timestamp_ms);
            }

            EventKind::WorkflowResumed => {
                self.on_workflow_resumed(timestamp_ms);
            }

            // ═══════════════════════════════════════════
            // TASK EVENTS
            // ═══════════════════════════════════════════
            EventKind::TaskScheduled {
                task_id,
                dependencies,
            } => {
                self.on_task_scheduled(task_id, dependencies);
            }

            EventKind::TaskStarted {
                task_id,
                verb,
                inputs,
            } => {
                self.on_task_started(task_id, verb, inputs);
            }

            EventKind::TaskCompleted {
                task_id,
                output,
                duration_ms,
            } => {
                self.on_task_completed(task_id, output, *duration_ms, timestamp_ms);
            }

            EventKind::TaskFailed {
                task_id,
                error,
                duration_ms,
                ..
            } => {
                self.on_task_failed(task_id, error, *duration_ms);
            }

            EventKind::TaskSkipped { task_id, reason } => {
                self.on_task_skipped(task_id, reason);
            }

            // ═══════════════════════════════════════════
            // CONTEXT EVENTS
            // ═══════════════════════════════════════════
            EventKind::ContextAssembled {
                sources,
                excluded,
                total_tokens,
                budget_used_pct,
                truncated,
                ..
            } => {
                self.on_context_assembled(
                    sources,
                    excluded,
                    *total_tokens,
                    *budget_used_pct,
                    *truncated,
                );
            }

            // ═══════════════════════════════════════════
            // BINDING EVENTS
            // ═══════════════════════════════════════════
            EventKind::TemplateResolved {
                task_id,
                template,
                result,
            } => {
                self.on_template_resolved(task_id, template, result, timestamp_ms);
            }

            // ═══════════════════════════════════════════
            // AGENT EVENTS
            // ═══════════════════════════════════════════
            EventKind::PresetApplied { .. } => {
                // Informational — no TUI state change needed
            }

            EventKind::AgentStart { max_turns, .. } => {
                self.on_agent_start(*max_turns);
            }

            EventKind::AgentTurn {
                turn_index,
                kind,
                metadata,
                ..
            } => {
                self.on_agent_turn(*turn_index, kind, metadata);
            }

            EventKind::AgentComplete { .. } => {
                self.on_agent_complete();
            }

            EventKind::AgentSpawned {
                parent_task_id,
                child_task_id,
                depth,
            } => {
                self.on_agent_spawned(parent_task_id, child_task_id, *depth, timestamp_ms);
            }

            // ═══════════════════════════════════════════
            // PROVIDER EVENTS
            // ═══════════════════════════════════════════
            EventKind::ProviderCalled {
                task_id,
                provider,
                model,
                prompt_len,
                ..
            } => {
                self.on_provider_called(task_id, provider, model, *prompt_len);
            }

            EventKind::ProviderResponded {
                task_id,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cost_usd,
                ttft_ms,
                finish_reason,
                ..
            } => {
                self.on_provider_responded(
                    task_id,
                    *input_tokens,
                    *output_tokens,
                    *cache_read_tokens,
                    *cost_usd,
                    ttft_ms,
                    finish_reason,
                    timestamp_ms,
                );
            }

            // ═══════════════════════════════════════════
            // MCP EVENTS
            // ═══════════════════════════════════════════
            EventKind::McpInvoke {
                task_id,
                mcp_server,
                tool,
                resource,
                call_id,
                params,
            } => {
                self.on_mcp_invoke(
                    task_id,
                    mcp_server,
                    tool,
                    resource,
                    call_id,
                    params,
                    timestamp_ms,
                );
            }

            EventKind::McpResponse {
                output_len,
                call_id,
                duration_ms,
                cached,
                is_error,
                response,
                ..
            } => {
                self.on_mcp_response(
                    *output_len,
                    call_id,
                    *duration_ms,
                    *cached,
                    *is_error,
                    response,
                    timestamp_ms,
                );
            }

            // ═══════════════════════════════════════════
            // MCP CONNECTION EVENTS
            // ═══════════════════════════════════════════
            EventKind::McpConnected { server_name, .. } => {
                self.on_mcp_connected(server_name, timestamp_ms);
            }

            EventKind::McpError {
                server_name, error, ..
            } => {
                self.on_mcp_error(server_name, error, timestamp_ms);
            }

            EventKind::McpRetry {
                server_name,
                operation,
                attempt,
                max_attempts,
                error,
                ..
            } => {
                self.on_mcp_retry(
                    server_name,
                    operation,
                    *attempt,
                    *max_attempts,
                    error,
                    timestamp_ms,
                );
            }

            // ═══════════════════════════════════════════
            // LOG / CUSTOM EVENTS
            // ═══════════════════════════════════════════
            EventKind::Log {
                level,
                message,
                task_id,
            } => {
                self.on_log(level, message, task_id, timestamp_ms);
            }

            EventKind::Custom {
                name,
                payload,
                task_id,
            } => {
                self.on_custom(name, payload, task_id, timestamp_ms);
            }

            // ═══════════════════════════════════════════
            // ARTIFACT EVENTS
            // ═══════════════════════════════════════════
            EventKind::ArtifactWritten {
                task_id,
                path,
                size,
                ..
            } => {
                self.on_artifact_written(task_id, path, *size, timestamp_ms);
            }

            EventKind::ArtifactFailed {
                task_id,
                path,
                reason,
            } => {
                self.on_artifact_failed(task_id, path, reason, timestamp_ms);
            }

            // ═══════════════════════════════════════════
            // STRUCTURED OUTPUT EVENTS
            // ═══════════════════════════════════════════
            EventKind::StructuredOutputAttempt {
                task_id,
                layer,
                layer_name,
                attempt,
                success,
                error,
            } => {
                self.on_structured_output_attempt(
                    task_id,
                    *layer,
                    layer_name,
                    *attempt,
                    *success,
                    error,
                    timestamp_ms,
                );
            }

            EventKind::StructuredOutputSuccess {
                task_id,
                layer,
                layer_name,
                total_attempts,
            } => {
                self.on_structured_output_success(
                    task_id,
                    *layer,
                    layer_name,
                    *total_attempts,
                    timestamp_ms,
                );
            }

            EventKind::StructuredOutputTimeout { .. } => {
                // Timeout is reflected via TaskFailed in the TUI
            }

            // ═══════════════════════════════════════════
            // GUARDRAIL EVENTS
            // ═══════════════════════════════════════════
            EventKind::GuardrailPassed {
                task_id,
                guardrail_type,
                description,
            } => {
                self.on_guardrail_passed(task_id, guardrail_type, description, timestamp_ms);
            }

            EventKind::GuardrailFailed {
                task_id,
                guardrail_type,
                description,
                message,
            } => {
                self.on_guardrail_failed(
                    task_id,
                    guardrail_type,
                    description,
                    message,
                    timestamp_ms,
                );
            }

            EventKind::GuardrailEscalation {
                task_id,
                guardrail_type,
                guardrail_id,
                message,
                severity,
                suggested_action,
            } => {
                self.on_guardrail_escalation(
                    task_id,
                    guardrail_type,
                    guardrail_id,
                    message,
                    severity,
                    suggested_action,
                    timestamp_ms,
                );
            }

            // ═══════════════════════════════════════════
            // HTTP TELEMETRY EVENTS
            // ═══════════════════════════════════════════
            EventKind::HttpRequest {
                task_id,
                method,
                url,
                ..
            } => {
                self.on_http_request(task_id, method, url, timestamp_ms);
            }

            EventKind::HttpResponse {
                task_id,
                status_code,
                elapsed_ms,
                ..
            } => {
                self.on_http_response(task_id, *status_code, *elapsed_ms, timestamp_ms);
            }

            // ═══════════════════════════════════════════
            // MEDIA EVENTS
            // ═══════════════════════════════════════════
            EventKind::MediaExtracted {
                task_id,
                block_count,
                ..
            } => {
                self.on_media_extracted(task_id, *block_count, timestamp_ms);
            }

            EventKind::MediaProcessed { .. } | EventKind::MediaStored { .. } => {
                // Tracked via events, no TUI notification for individual blocks
            }

            EventKind::MediaStoreFailed {
                task_id, reason, ..
            } => {
                self.on_media_store_failed(task_id, reason, timestamp_ms);
            }

            EventKind::MediaCleanup {
                removed,
                bytes_freed,
                dry_run,
            } => {
                self.on_media_cleanup(*removed, *bytes_freed, *dry_run, timestamp_ms);
            }

            EventKind::MediaIntegrityCheck { checked, warnings } => {
                self.on_media_integrity_check(*checked, *warnings, timestamp_ms);
            }

            EventKind::VisionContentResolved {
                task_id,
                image_count,
                total_bytes,
                resolve_ms,
            } => {
                self.on_vision_content_resolved(
                    task_id,
                    *image_count,
                    *total_bytes,
                    *resolve_ms,
                    timestamp_ms,
                );
            }

            // ═══════════════════════════════════════════
            // EXEC / FETCH / POLICY EVENTS (observability)
            // ═══════════════════════════════════════════
            EventKind::ExecCompleted { .. }
            | EventKind::FetchRetry { .. }
            | EventKind::FetchExhausted { .. }
            | EventKind::TaskRetry { .. }
            | EventKind::ProviderAutoRetried { .. }
            | EventKind::PolicyBlocked { .. }
            | EventKind::BootPhaseCompleted { .. }
            | EventKind::NativeModelLoaded { .. }
            | EventKind::BudgetOk { .. }
            | EventKind::BudgetExceeded { .. }
            | EventKind::BindingDefaultApplied { .. }
            | EventKind::BindingTransformApplied { .. }
            | EventKind::BindingEnvResolved { .. }
            | EventKind::BindingVaultResolved { .. }
            | EventKind::DecomposeStarted { .. }
            | EventKind::DecomposeCompleted { .. }
            | EventKind::ForEachStarted { .. }
            | EventKind::ForEachCompleted { .. }
            | EventKind::ProviderInitialized { .. }
            | EventKind::BuiltinToolInvoked { .. }
            | EventKind::StreamingDelta { .. }
            | EventKind::ExtractApplied { .. }
            | EventKind::FallbackTriggered { .. }
            | EventKind::ProviderFallback { .. }
            | EventKind::RecordCreated { .. }
            | EventKind::RecordSkipped { .. }
            | EventKind::OrchestratorStarted { .. }
            | EventKind::OrchestratorRound { .. }
            | EventKind::OrchestratorSubWorkflow { .. }
            | EventKind::OrchestratorCompleted { .. }
            | EventKind::OrchestratorFailed { .. }
            | EventKind::SecurityScanFinding { .. }
            | EventKind::ForEachItemStarted { .. }
            | EventKind::ForEachItemCompleted { .. }
            | EventKind::ForEachItemFailed { .. }
            | EventKind::TaskCancelled { .. }
            | EventKind::FallbackChainExhausted { .. }
            | EventKind::TaskFallbackTriggered { .. }
            | EventKind::RateLimitDelay { .. }
            | EventKind::TemplateResolutionFailed { .. }
            | EventKind::SchemaLoadFailed { .. }
            | EventKind::VisionContentFailed { .. }
            // Nika Shield security events — captured in trace NDJSON only
            | EventKind::TaintAnalysisComplete { .. }
            | EventKind::TrustLevelAssigned { .. }
            | EventKind::TrustElevationUsed { .. }
            | EventKind::SpotlightApplied { .. }
            | EventKind::SpotlightSkipped { .. }
            | EventKind::AgentToolRestricted { .. }
            | EventKind::CanaryInjected { .. }
            | EventKind::CanaryDetected { .. }
            | EventKind::ScanFindingDetected { .. }
            | EventKind::SkillIntegrityVerified { .. }
            | EventKind::SkillIntegrityFailed { .. }
            | EventKind::CapabilityDenied { .. }
            | EventKind::MlDetectionRun { .. }
            | EventKind::MlDetectionBlocked { .. } => {
                // Observability events: captured in trace NDJSON but don't
                // require TUI state mutations -- the TUI tracks task-level
                // status via TaskStarted/TaskCompleted/TaskFailed.
            }
        }
    }

    /// Update elapsed time and animation frame (call on each render frame)
    ///
    /// Animation frame wraps at FRAME_CYCLE (60) for 1-second cycles at 60 FPS.
    pub fn tick(&mut self) {
        // Update elapsed time
        if let Some(started) = self.workflow.started_at {
            self.workflow.elapsed_ms = started.elapsed().as_millis() as u64;
        }

        // Advance animation frame (wraps at FRAME_CYCLE for 1-second cycles)
        self.frame = self.frame.wrapping_add(1) % FRAME_CYCLE;

        // Expire old status messages
        self.status_messages.tick();
    }

    /// Get spinner character for current frame
    /// Uses braille spinner
    /// Divides by FRAME_DIV_NORMAL (6) for ~10 FPS animation
    pub fn spinner_char(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let idx = (self.frame / FRAME_DIV_NORMAL) as usize % SPINNER.len();
        SPINNER[idx]
    }

    /// Get rocket animation character for current frame
    /// Used during Launch phase
    /// Divides by FRAME_DIV_GLACIAL (15) for ~4 FPS animation
    pub fn rocket_char(&self) -> char {
        const ROCKET: &[char] = &['🚀', '🔥', '💨', '✨'];
        let idx = (self.frame / FRAME_DIV_GLACIAL) as usize % ROCKET.len();
        ROCKET[idx]
    }
}
