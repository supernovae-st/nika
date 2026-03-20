//! TUI State Management
//!
//! Central state for the TUI application.
//! Updated by events from the runtime, queried by panels for rendering.
//!
//! ## Module Structure
//!
//! - `types` - Core types (PanelId, TuiMode, WorkflowState, TaskState, etc.)
//! - `scroll` - Panel scroll state management
//! - `notification` - Notification system
//! - `settings` - Settings overlay state
//! - `chat_overlay` - Chat overlay for contextual AI assistance
//! - `cache` - JSON formatting cache
//!
//! ## Animation Frame Standard
//!
//! All animation frames use a standardized system based on 60 FPS:
//!
//! - **TuiState.frame**: Main frame counter, wraps at 60 (1-second cycles)
//! - **View.frame (u8)**: Per-view counter, wraps at 256 via `wrapping_add(1)`
//! - **Widget frames**: Use frame value passed from parent, divide for speed
//!
//! ### Frame Division Patterns
//!
//! | Division | Effective FPS | Use Case |
//! |----------|---------------|----------|
//! | `frame / 3` | 20 FPS | Fast spinners, flow indicators |
//! | `frame / 4` | 15 FPS | Standard spinners, edges |
//! | `frame / 6` | 10 FPS | Normal spinners |
//! | `frame / 8` | 7.5 FPS | Cursor blink, slow pulse |
//! | `frame / 10` | 6 FPS | Agent indicators |
//! | `frame / 15` | 4 FPS | Very slow animations |

// ═══════════════════════════════════════════════════════════════════════════════
// SUBMODULES
// ═══════════════════════════════════════════════════════════════════════════════

mod agent_state;
mod cache;
mod chat_overlay;
mod mcp_state;
mod notification;
mod notification_state;
mod scroll;
mod settings;
mod types;
mod ui;

#[cfg(test)]
mod tests;

// ═══════════════════════════════════════════════════════════════════════════════
// RE-EXPORTS
// ═══════════════════════════════════════════════════════════════════════════════

// Note: Some re-exports may appear unused but are for external crate API
#[allow(unused_imports)]
pub use types::{
    AgentTurnState,
    Breakpoint,
    ChatPanel,
    ContextAssembly,
    DirtyFlags,
    McpCall,
    Metrics,
    PanelId,
    SpawnedAgent,
    TaskState,
    TemplateResolution,
    TuiMode,
    WorkflowState,
    // Animation constants
    FRAME_CYCLE,
    FRAME_DIV_GLACIAL,
    FRAME_DIV_NORMAL,
};

// Scroll
#[allow(unused_imports)]
pub use scroll::{PanelScrollState, SCROLL_MARGIN};

// Notification
#[allow(unused_imports)]
pub use notification::{Notification, NotificationLevel};

// Settings
#[allow(unused_imports)]
pub use settings::{SettingsField, SettingsState};

// Chat overlay
pub use chat_overlay::{ChatOverlayMessage, ChatOverlayMessageRole, ChatOverlayState};

// Cache
pub use cache::JsonFormatCache;

// Domain slices (P1 decomposition)
pub use agent_state::AgentState;
pub use mcp_state::McpState;
pub use notification_state::NotificationState;
pub use ui::UiState;

// ═══════════════════════════════════════════════════════════════════════════════
// IMPORTS FOR TuiState
// ═══════════════════════════════════════════════════════════════════════════════

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use crate::config::NikaConfig;
use crate::event::EventKind;

use super::theme::{MissionPhase, TaskStatus};
#[allow(unused_imports)]
use super::widgets::{task_box::TokenVelocity, StatusQueue, TimelineEntry};

// ═══════════════════════════════════════════════════════════════════════════════
// TuiState STRUCT
// ═══════════════════════════════════════════════════════════════════════════════

/// Central state for the TUI application
pub struct TuiState {
    // ═══════════════════════════════════════════
    // ANIMATION STATE
    // ═══════════════════════════════════════════
    /// Frame counter (wraps at 60 for 1-second cycles at 60 FPS)
    pub frame: u8,

    // ═══════════════════════════════════════════
    // EXECUTION STATE
    // ═══════════════════════════════════════════
    /// Workflow state
    pub workflow: WorkflowState,
    /// Task states by ID
    pub tasks: HashMap<String, TaskState>,
    /// Current active task
    pub current_task: Option<String>,
    /// Task execution order (for timeline)
    pub task_order: Vec<String>,

    // ═══════════════════════════════════════════
    // SETTINGS (requires NikaConfig at construction)
    // ═══════════════════════════════════════════
    /// Settings overlay state
    pub settings: SettingsState,

    // ═══════════════════════════════════════════
    // DEBUG STATE
    // ═══════════════════════════════════════════
    /// Active breakpoints
    pub breakpoints: HashSet<Breakpoint>,
    // P0 Fix: Removed duplicate `paused` field - use workflow.paused instead via is_paused()
    /// Step mode (advance one step at a time)
    pub step_mode: bool,

    // ═══════════════════════════════════════════
    // METRICS
    // ═══════════════════════════════════════════
    /// Aggregated metrics
    pub metrics: Metrics,

    // ═══════════════════════════════════════════
    // FILTER STATE
    // ═══════════════════════════════════════════
    /// Current filter/search query
    pub filter_query: String,
    /// Filter cursor position
    pub filter_cursor: usize,

    // ═══════════════════════════════════════════
    // STATUS MESSAGES
    // ═══════════════════════════════════════════
    /// Status message queue for user feedback
    pub status_messages: StatusQueue,

    // ═══════════════════════════════════════════
    // LAZY RENDERING (TIER 4.1)
    // ═══════════════════════════════════════════
    /// Dirty flags for lazy rendering
    pub dirty: DirtyFlags,

    // ═══════════════════════════════════════════
    // JSON MEMOIZATION (TIER 4.4)
    // ═══════════════════════════════════════════
    /// Cache for formatted JSON strings
    pub json_cache: JsonFormatCache,

    // ═══════════════════════════════════════════
    // TIMELINE CACHE (Performance Optimization)
    // ═══════════════════════════════════════════
    /// Cached timeline entries (rebuilt when timeline_version changes)
    pub cached_timeline_entries: Vec<TimelineEntry>,
    /// Version counter for cache invalidation (incremented on task state changes)
    timeline_version: u32,
    /// Version used to build the current cache
    timeline_cache_version: u32,

    // ═══════════════════════════════════════════
    // DOMAIN SLICES (P1 decomposition)
    // ═══════════════════════════════════════════
    /// UI interaction state (focus, mode, scroll, theme, tabs)
    pub ui: UiState,
    /// MCP call tracking state (calls, selection, context assembly)
    pub mcp: McpState,
    /// Agent execution tracking (turns, streaming, spawned agents)
    pub agent: AgentState,
    /// Notification management (system alerts, dismissal)
    pub notifs: NotificationState,
}

// ═══════════════════════════════════════════════════════════════════════════════
// TuiState IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════════

impl TuiState {
    /// Create new TUI state for a workflow
    pub fn new(workflow_path: &str) -> Self {
        // Load config from file, merge with env vars
        let config = NikaConfig::load().unwrap_or_default().with_env();

        Self {
            frame: 0,
            workflow: WorkflowState::new(workflow_path.to_string()),
            tasks: HashMap::new(),
            current_task: None,
            task_order: Vec::new(),
            settings: SettingsState::new(config),
            breakpoints: HashSet::new(),
            // P0 Fix: paused field removed - workflow.paused is single source of truth
            step_mode: false,
            metrics: Metrics::default(),
            filter_query: String::new(),
            filter_cursor: 0,
            status_messages: StatusQueue::new(),
            dirty: {
                let mut d = DirtyFlags::default();
                d.mark_all(); // First frame needs full redraw
                d
            },
            json_cache: JsonFormatCache::new(),
            cached_timeline_entries: Vec::new(),
            timeline_version: 0,
            timeline_cache_version: 0,
            ui: UiState::new(),
            mcp: McpState::new(),
            agent: AgentState::new(),
            notifs: NotificationState::new(),
        }
    }

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
                self.workflow.task_count = *task_count;
                self.workflow.phase = MissionPhase::Countdown;
                self.workflow.started_at = Some(Instant::now());
                self.workflow.generation_id = Some(generation_id.clone());
                // TIER 4.1: Mark all panels dirty on workflow start
                self.dirty.mark_all();
                // TIER 4.4: Clear JSON cache on workflow start
                self.json_cache.clear();
            }

            EventKind::WorkflowCompleted {
                final_output,
                total_duration_ms,
            } => {
                self.workflow.phase = MissionPhase::MissionSuccess;
                self.workflow.final_output = Some(Arc::clone(final_output));
                self.workflow.total_duration_ms = Some(*total_duration_ms);
                self.current_task = None;

                // TIER 3.4: Add success notification
                let duration_secs = *total_duration_ms as f64 / 1000.0;
                self.add_notification(Notification::success(
                    format!(
                        "Magnificent! Warped through in {:.1}s ({}/{} tasks)",
                        duration_secs, self.workflow.tasks_completed, self.workflow.task_count
                    ),
                    timestamp_ms,
                ));
                // TIER 4.1: Mark progress and status dirty
                self.dirty.progress = true;
                self.dirty.status = true;
            }

            EventKind::WorkflowFailed { error, .. } => {
                self.workflow.phase = MissionPhase::Abort;
                self.workflow.error_message = Some(error.clone());

                // TIER 3.4: Add error notification
                self.add_notification(Notification::error(
                    format!("RAWR! Mission failed: {}", error),
                    timestamp_ms,
                ));
                // TIER 4.1: Mark progress, status, and notifications dirty
                self.dirty.progress = true;
                self.dirty.status = true;
                self.dirty.notifications = true;
            }

            EventKind::WorkflowAborted {
                reason,
                duration_ms,
                running_tasks,
            } => {
                self.workflow.phase = MissionPhase::Abort;
                self.workflow.error_message = Some(format!("Aborted: {}", reason));
                self.workflow.total_duration_ms = Some(*duration_ms);
                self.current_task = None;

                // TIER 3.4: Add abort notification
                let task_info = if running_tasks.is_empty() {
                    String::new()
                } else {
                    format!(" ({} tasks interrupted)", running_tasks.len())
                };
                self.add_notification(Notification::warning(
                    format!("Mission aborted: {}{}", reason, task_info),
                    timestamp_ms,
                ));
                // Mark all relevant panels dirty
                self.dirty.progress = true;
                self.dirty.status = true;
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // TASK EVENTS
            // ═══════════════════════════════════════════
            EventKind::TaskScheduled {
                task_id,
                dependencies,
            } => {
                let deps: Vec<String> = dependencies
                    .iter()
                    .map(|s: &std::sync::Arc<str>| s.to_string())
                    .collect();
                let task = TaskState::new(task_id.to_string(), deps);
                self.tasks.insert(task_id.to_string(), task);
                self.task_order.push(task_id.to_string());
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.invalidate_timeline_cache();
            }

            EventKind::TaskStarted {
                task_id,
                verb,
                inputs,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Running;
                    task.started_at = Some(Instant::now());
                    task.input = Some(Arc::new(inputs.clone()));
                    task.task_type = Some(verb.to_string());
                }
                self.current_task = Some(task_id.to_string());

                // Update phase
                if self.workflow.phase == MissionPhase::Countdown {
                    self.workflow.phase = MissionPhase::Launch;
                } else {
                    self.workflow.phase = MissionPhase::Orbital;
                }
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.invalidate_timeline_cache();
                // TIER 4.4: Invalidate task cache on start (will need re-format later)
                self.json_cache.invalidate(&format!("task:{}", task_id));
            }

            EventKind::TaskCompleted {
                task_id,
                output,
                duration_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Success;
                    task.duration_ms = Some(*duration_ms);
                    task.output = Some(output.clone());
                }
                self.workflow.tasks_completed += 1;

                // TIER 3.4: Notify on slow tasks
                let duration_secs = *duration_ms as f64 / 1000.0;
                if *duration_ms > 30_000 {
                    self.add_notification(Notification::alert(
                        format!(
                            "Sloth mode! '{}' crawled in at {:.1}s",
                            task_id, duration_secs
                        ),
                        timestamp_ms,
                    ));
                } else if *duration_ms > 10_000 {
                    self.add_notification(Notification::warning(
                        format!("Taking its time... '{}' at {:.1}s", task_id, duration_secs),
                        timestamp_ms,
                    ));
                }

                // P1 Fix: Only clear agent state if this task was actually an agent task
                // Check task_type to avoid clearing during parallel workflows
                if let Some(task) = self.tasks.get(task_id.as_ref()) {
                    if task.task_type.as_deref() == Some("agent") {
                        self.agent.turns.clear();
                        self.agent.streaming_buffer.clear();
                        self.agent.max_turns = None;
                    }
                }
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.invalidate_timeline_cache();
                // TIER 4.4: Invalidate task cache on completion (new output)
                self.json_cache.invalidate(&format!("task:{}", task_id));
            }

            EventKind::TaskFailed {
                task_id,
                error,
                duration_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Failed;
                    task.duration_ms = Some(*duration_ms);
                    task.error = Some(error.clone());
                }
                // TIER 4.1: Mark progress, dag, and status dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.dirty.status = true;
                self.invalidate_timeline_cache();
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
                let call = McpCall {
                    call_id: call_id.clone(),
                    seq: self.mcp.seq,
                    server: mcp_server.clone(),
                    tool: tool.clone(),
                    resource: resource.clone(),
                    task_id: task_id.to_string(),
                    completed: false,
                    output_len: None,
                    timestamp_ms,
                    params: params.clone(),
                    response: None,
                    is_error: false,
                    duration_ms: None,
                };
                self.mcp.calls.push(call);
                self.mcp.seq += 1;

                // Update phase
                self.workflow.phase = MissionPhase::Rendezvous;

                // Track in metrics
                if let Some(ref tool_name) = tool {
                    let entry = self.metrics.mcp_calls.entry(tool_name.clone()).or_insert(0);
                    *entry += 1;
                }
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
            }

            EventKind::McpResponse {
                task_id: _,
                output_len,
                call_id,
                duration_ms,
                cached: _,
                is_error,
                response,
            } => {
                // Find and update the matching call by call_id
                let tool_name = self
                    .mcp
                    .calls
                    .iter()
                    .find(|c| c.call_id == *call_id)
                    .and_then(|c| c.tool.clone());

                if let Some(call) = self.mcp.calls.iter_mut().find(|c| c.call_id == *call_id) {
                    call.completed = true;
                    call.output_len = Some(*output_len);
                    call.response = response.clone();
                    call.is_error = *is_error;
                    call.duration_ms = Some(*duration_ms);
                }

                // Track MCP latency for sparkline (keep last 20 values)
                if self.metrics.latency_history.len() >= 20 {
                    self.metrics.latency_history.remove(0);
                }
                self.metrics.latency_history.push(*duration_ms);

                // TIER 3.4: Notify on slow MCP responses (> 5s)
                if *duration_ms > 5_000 {
                    let duration_secs = *duration_ms as f64 / 1000.0;
                    let tool_display = tool_name.as_deref().unwrap_or("resource");
                    self.add_notification(Notification::warning(
                        format!(
                            "Tentacles reaching... '{}' at {:.1}s",
                            tool_display, duration_secs
                        ),
                        timestamp_ms,
                    ));
                }

                // Return to orbital phase
                self.workflow.phase = MissionPhase::Orbital;
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
                // TIER 4.4: Invalidate MCP call cache on response
                self.json_cache.invalidate(&format!("mcp:{}", call_id));
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
                self.mcp.context_assembly = ContextAssembly {
                    sources: sources.clone(),
                    excluded: excluded.clone(),
                    total_tokens: *total_tokens,
                    budget_used_pct: *budget_used_pct,
                    truncated: *truncated,
                };
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
            }

            // ═══════════════════════════════════════════
            // BINDING EVENTS
            // ═══════════════════════════════════════════
            EventKind::TemplateResolved {
                task_id,
                template,
                result,
            } => {
                // Keep last 10 resolutions
                if self.agent.recent_templates.len() >= 10 {
                    self.agent.recent_templates.pop_front();
                }
                self.agent.recent_templates.push_back(TemplateResolution {
                    task_id: task_id.to_string(),
                    template: template.clone(),
                    result: result.clone(),
                    timestamp_ms,
                });
                // Mark context panel dirty (template bindings are context-related)
                self.dirty.novanet = true;
            }

            // ═══════════════════════════════════════════
            // AGENT EVENTS
            // ═══════════════════════════════════════════
            EventKind::AgentStart { max_turns, .. } => {
                self.agent.turns.clear();
                self.agent.streaming_buffer.clear();
                self.agent.max_turns = Some(*max_turns);
                // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentTurn {
                turn_index,
                kind,
                metadata,
                ..
            } => {
                // Extract tokens from metadata if present
                let tokens = metadata.as_ref().map(|m| m.total_tokens());
                // Extract thinking and response_text from metadata
                let thinking = metadata.as_ref().and_then(|m| m.thinking.clone());
                let response_text = metadata.as_ref().map(|m| m.response_text.clone());

                let turn = AgentTurnState {
                    index: *turn_index,
                    status: kind.clone(),
                    tokens,
                    tool_calls: Vec::new(),
                    thinking,
                    response_text,
                };
                // Update or add turn
                if let Some(existing) = self.agent.turns.iter_mut().find(|t| t.index == *turn_index)
                {
                    existing.status = kind.clone();
                    existing.tokens = tokens;
                    existing.thinking = turn.thinking;
                    existing.response_text = turn.response_text;
                } else {
                    self.agent.turns.push(turn);
                }
                // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentComplete { turns, .. } => {
                // Update metrics
                if let Some(last_turn) = self.agent.turns.last() {
                    if let Some(tokens) = last_turn.tokens {
                        self.metrics.token_history.push(tokens);
                    }
                }
                let _ = turns; // Used for logging
                               // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentSpawned {
                parent_task_id,
                child_task_id,
                depth,
            } => {
                // Track spawned sub-agent
                self.agent.spawned_agents.push(SpawnedAgent {
                    parent_task_id: parent_task_id.to_string(),
                    child_task_id: child_task_id.to_string(),
                    depth: *depth,
                });

                // Add notification for nested agent spawn
                self.add_notification(Notification::info(
                    format!(
                        "Hatching '{}' at depth {} - fly little one!",
                        child_task_id, depth
                    ),
                    timestamp_ms,
                ));

                // TIER 4.1: Mark reasoning and notifications dirty
                self.dirty.reasoning = true;
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // PROVIDER EVENTS
            // ═══════════════════════════════════════════
            EventKind::ProviderCalled {
                task_id,
                provider,
                model,
                prompt_len,
            } => {
                // Update task's provider info
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.provider = Some(provider.clone());
                    task.model = Some(model.clone());
                    task.prompt_len = Some(*prompt_len);
                }

                // Update metrics
                self.metrics.provider_calls += 1;
                self.metrics.last_model = Some(model.clone());

                // TIER 4.1: Mark progress dirty (for provider display)
                self.dirty.progress = true;
            }

            EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cost_usd,
                ttft_ms,
                ..
            } => {
                self.metrics.input_tokens += input_tokens;
                self.metrics.output_tokens += output_tokens;
                self.metrics.cache_read_tokens += cache_read_tokens;
                self.metrics.total_tokens += input_tokens + output_tokens;
                self.metrics.cost_usd += cost_usd;
                self.metrics
                    .token_history
                    .push(input_tokens + output_tokens);
                if let Some(ttft) = ttft_ms {
                    self.metrics.latency_history.push(*ttft);
                    // Calculate tokens/sec from TTFT and push to velocity tracker
                    // TTFT in ms, output_tokens is total - estimate avg rate
                    let ttft_secs = (*ttft as f32).max(1.0) / 1000.0;
                    let velocity = *output_tokens as f32 / ttft_secs;
                    self.metrics.token_velocity.push(velocity);
                } else if *output_tokens > 0 {
                    // Fallback: assume ~1 second if no TTFT, just track relative activity
                    self.metrics.token_velocity.push(*output_tokens as f32);
                }

                // TIER 3.4: Token usage progression with cosmic pirate emojis
                const CONTEXT_WINDOW: u64 = 100_000;
                let pct = (self.metrics.total_tokens as f64 / CONTEXT_WINDOW as f64) * 100.0;

                if pct > 95.0 {
                    self.add_notification(Notification::alert(
                        format!(
                            "ABANDON SHIP! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 85.0 {
                    self.add_notification(Notification::alert(
                        format!(
                            "Danger zone! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 70.0 {
                    self.add_notification(Notification::warning(
                        format!(
                            "Getting spicy! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 50.0 {
                    self.add_notification(Notification::info(
                        format!(
                            "Heating up... {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                }
                // TIER 4.1: Mark progress dirty (for metrics display)
                self.dirty.progress = true;
            }

            // ═══════════════════════════════════════════
            // PAUSE/RESUME EVENTS
            // ═══════════════════════════════════════════
            EventKind::WorkflowPaused => {
                // P2 Fix: Save phase before pausing for proper restoration
                self.workflow.phase_before_pause = Some(self.workflow.phase);
                self.workflow.paused = true;
                self.workflow.phase = MissionPhase::Pause;
                self.add_notification(Notification::warning(
                    "Mission paused - press SPACE to resume",
                    timestamp_ms,
                ));
                self.dirty.progress = true;
                self.dirty.status = true;
            }

            EventKind::WorkflowResumed => {
                self.workflow.paused = false;
                // P2 Fix: Restore saved phase, or infer from current state
                if let Some(phase) = self.workflow.phase_before_pause.take() {
                    self.workflow.phase = phase;
                } else if self.current_task.is_some() {
                    self.workflow.phase = MissionPhase::Orbital;
                } else {
                    self.workflow.phase = MissionPhase::Countdown;
                }
                self.add_notification(Notification::info(
                    "Mission resumed - engines back online!",
                    timestamp_ms,
                ));
                self.dirty.progress = true;
                self.dirty.status = true;
            }

            // ═══════════════════════════════════════════
            // MCP CONNECTION EVENTS
            // ═══════════════════════════════════════════
            EventKind::McpConnected { server_name, .. } => {
                self.add_notification(Notification::success(
                    format!("MCP server '{}' connected", server_name),
                    timestamp_ms,
                ));
                self.dirty.status = true;
            }

            EventKind::McpError {
                server_name, error, ..
            } => {
                self.add_notification(Notification::error(
                    format!("MCP '{}' error: {}", server_name, error),
                    timestamp_ms,
                ));
                self.dirty.status = true;
            }

            // P2 Fix: Handle MCP retry event for visibility
            EventKind::McpRetry {
                server_name,
                operation,
                attempt,
                max_attempts,
                error,
                ..
            } => {
                self.add_notification(Notification::warning(
                    format!(
                        "MCP '{}' retry {}/{} for '{}': {}",
                        server_name, attempt, max_attempts, operation, error
                    ),
                    timestamp_ms,
                ));
                self.dirty.status = true;
            }

            // Handle builtin log events
            EventKind::Log {
                level,
                message,
                task_id,
            } => {
                let prefix = match level.as_str() {
                    "error" => "[ERR]",
                    "warn" => "[WRN]",
                    "info" => "[INF]",
                    "debug" => "[DBG]",
                    "trace" => "[TRC]",
                    _ => "[LOG]",
                };
                let task_suffix = task_id
                    .as_ref()
                    .map(|t| format!(" [{}]", t))
                    .unwrap_or_default();
                self.add_notification(Notification::info(
                    format!("{} {}{}", prefix, message, task_suffix),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // Handle custom events from nika:emit
            EventKind::Custom {
                name,
                payload,
                task_id,
            } => {
                let task_suffix = task_id
                    .as_ref()
                    .map(|t| format!(" [{}]", t))
                    .unwrap_or_default();
                // Compact payload display (first 50 chars, UTF-8 safe)
                let payload_preview = if payload.is_null() {
                    String::new()
                } else {
                    let s = payload.to_string();
                    if s.chars().count() > 50 {
                        let truncated: String = s.chars().take(47).collect();
                        format!(": {}...", truncated)
                    } else {
                        format!(": {}", s)
                    }
                };
                self.add_notification(Notification::info(
                    format!("[EVT] {}{}{}", name, payload_preview, task_suffix),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
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
                let size_str = if *size < 1024 {
                    format!("{} B", size)
                } else if *size < 1024 * 1024 {
                    format!("{:.1} KB", *size as f64 / 1024.0)
                } else {
                    format!("{:.1} MB", *size as f64 / (1024.0 * 1024.0))
                };
                self.add_notification(Notification::success(
                    format!("[{}] Artifact written: {} ({})", task_id, path, size_str),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::ArtifactFailed {
                task_id,
                path,
                reason,
            } => {
                self.add_notification(Notification::error(
                    format!("[{}] Artifact failed: {} - {}", task_id, path, reason),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
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
                // Add notification for structured output attempts
                if *success {
                    self.add_notification(Notification::info(
                        format!(
                            "[{}] Layer {} ({}) attempt {} succeeded",
                            task_id, layer, layer_name, attempt
                        ),
                        timestamp_ms,
                    ));
                } else if let Some(err) = error {
                    self.add_notification(Notification::warning(
                        format!(
                            "[{}] Layer {} ({}) attempt {} failed: {}",
                            task_id, layer, layer_name, attempt, err
                        ),
                        timestamp_ms,
                    ));
                }
                self.dirty.notifications = true;
            }

            EventKind::StructuredOutputSuccess {
                task_id,
                layer,
                layer_name,
                total_attempts,
            } => {
                self.add_notification(Notification::success(
                    format!(
                        "[{}] Structured output extracted via layer {} ({}) after {} attempt(s)",
                        task_id, layer, layer_name, total_attempts
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // GUARDRAIL EVENTS
            // ═══════════════════════════════════════════
            EventKind::GuardrailPassed {
                task_id,
                guardrail_type,
                description,
            } => {
                self.add_notification(Notification::success(
                    format!(
                        "[{}] Guardrail {} passed: {}",
                        task_id, guardrail_type, description
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::GuardrailFailed {
                task_id,
                guardrail_type,
                description,
                message,
            } => {
                self.add_notification(Notification::error(
                    format!(
                        "[{}] Guardrail {} failed ({}): {}",
                        task_id, guardrail_type, description, message
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::GuardrailEscalation {
                task_id,
                guardrail_type,
                guardrail_id,
                message,
                severity,
                suggested_action,
            } => {
                let action_text = suggested_action
                    .as_ref()
                    .map(|a| format!(" Action: {}", a))
                    .unwrap_or_default();
                self.add_notification(Notification::error(
                    format!(
                        "[{}] ESCALATION ({}) {} [{}]: {}{}",
                        task_id, severity, guardrail_type, guardrail_id, message, action_text
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // MEDIA EVENTS (no TUI action needed yet)
            // ═══════════════════════════════════════════
            // ═══════════════════════════════════════════
            // HTTP TELEMETRY EVENTS
            // ═══════════════════════════════════════════
            EventKind::HttpRequest {
                task_id,
                method,
                url,
                ..
            } => {
                self.add_notification(Notification::info(
                    format!("[{}] HTTP {} {}", task_id, method, url),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::HttpResponse {
                task_id,
                status_code,
                elapsed_ms,
                ..
            } => {
                let status_label = if *status_code < 400 { "OK" } else { "ERR" };
                self.add_notification(Notification::info(
                    format!(
                        "[{}] HTTP {} {} ({}ms)",
                        task_id, status_code, status_label, elapsed_ms
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::MediaExtracted {
                task_id,
                block_count,
                ..
            } => {
                self.add_notification(Notification::info(
                    format!("[{}] Extracted {} media block(s)", task_id, block_count),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }
            EventKind::MediaProcessed { .. } | EventKind::MediaStored { .. } => {
                // Tracked via events, no TUI notification for individual blocks
            }
            EventKind::MediaStoreFailed {
                task_id, reason, ..
            } => {
                self.add_notification(Notification::error(
                    format!("[{}] Media store failed: {}", task_id, reason),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }
            EventKind::MediaCleanup {
                removed,
                bytes_freed,
                dry_run,
            } => {
                if !dry_run {
                    self.add_notification(Notification::info(
                        format!(
                            "Media cleanup: removed {} files, freed {} bytes",
                            removed, bytes_freed
                        ),
                        timestamp_ms,
                    ));
                    self.dirty.notifications = true;
                }
            }
            EventKind::MediaIntegrityCheck { checked, warnings } => {
                if *warnings > 0 {
                    self.add_notification(Notification::warning(
                        format!(
                            "Media integrity: {}/{} refs had warnings",
                            warnings, checked
                        ),
                        timestamp_ms,
                    ));
                    self.dirty.notifications = true;
                }
            }

            EventKind::VisionContentResolved {
                task_id,
                image_count,
                total_bytes,
                resolve_ms,
            } => {
                self.add_notification(Notification::info(
                    format!(
                        "Vision: {} image(s) resolved ({} bytes, {}ms) for task {}",
                        image_count, total_bytes, resolve_ms, task_id
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
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

    // ═══════════════════════════════════════════
    // STATUS MESSAGE HELPERS
    // ═══════════════════════════════════════════

    /// Show an info status message
    pub fn status_info(&mut self, message: impl Into<String>) {
        self.status_messages.info(message);
    }

    /// Show a success status message
    pub fn status_success(&mut self, message: impl Into<String>) {
        self.status_messages.success(message);
    }

    /// Show a warning status message
    pub fn status_warning(&mut self, message: impl Into<String>) {
        self.status_messages.warning(message);
    }

    /// Show an error status message
    pub fn status_error(&mut self, message: impl Into<String>) {
        self.status_messages.error(message);
    }

    /// Check if a task is a spawned subagent
    ///
    /// Returns true if the task_id appears as a child in spawned_agents.
    pub fn is_subagent(&self, task_id: &str) -> bool {
        self.agent
            .spawned_agents
            .iter()
            .any(|s| s.child_task_id == task_id)
    }

    /// Get the appropriate agent icon for a task
    ///
    /// Returns different icons for subagents vs parent agents.
    pub fn agent_icon(&self, task_id: &str) -> &'static str {
        if self.is_subagent(task_id) {
            ">" // Spawned subagent
        } else {
            ">>" // Parent agent
        }
    }

    /// Focus next panel
    pub fn focus_next(&mut self) {
        self.ui.focus = self.ui.focus.next();
    }

    /// Focus previous panel
    pub fn focus_prev(&mut self) {
        self.ui.focus = self.ui.focus.prev();
    }

    /// Focus specific panel by number (1-indexed)
    pub fn focus_panel(&mut self, num: u8) {
        self.ui.focus = match num {
            1 => PanelId::Progress,
            2 => PanelId::Dag,
            3 => PanelId::NovaNet,
            4 => PanelId::Agent,
            _ => self.ui.focus,
        };
    }

    /// Cycle tab in the currently focused panel
    pub fn cycle_tab(&mut self) {
        match self.ui.focus {
            PanelId::Progress => self.ui.mission_tab = self.ui.mission_tab.next(),
            PanelId::Dag => self.ui.dag_tab = self.ui.dag_tab.next(),
            PanelId::NovaNet => self.ui.novanet_tab = self.ui.novanet_tab.next(),
            PanelId::Agent => self.ui.reasoning_tab = self.ui.reasoning_tab.next(),
        }
    }

    // ═══════════════════════════════════════════
    // MCP NAVIGATION (TIER 1.3)
    // ═══════════════════════════════════════════

    /// Select previous MCP call
    pub fn select_prev_mcp(&mut self) {
        if self.mcp.calls.is_empty() {
            return;
        }

        self.mcp.selected_idx = match self.mcp.selected_idx {
            None => Some(self.mcp.calls.len().saturating_sub(1)), // Start from last
            Some(0) => Some(0),                                   // Stay at first
            Some(idx) => Some(idx - 1),
        };
    }

    /// Select next MCP call
    pub fn select_next_mcp(&mut self) {
        if self.mcp.calls.is_empty() {
            return;
        }

        let max_idx = self.mcp.calls.len().saturating_sub(1);
        self.mcp.selected_idx = match self.mcp.selected_idx {
            None => Some(0),                              // Start from first
            Some(idx) if idx >= max_idx => Some(max_idx), // Stay at last
            Some(idx) => Some(idx + 1),
        };
    }

    /// Select MCP call by index (for direct access)
    pub fn select_mcp(&mut self, idx: usize) {
        if idx < self.mcp.calls.len() {
            self.mcp.selected_idx = Some(idx);
        }
    }

    /// Get currently selected MCP call
    pub fn get_selected_mcp(&self) -> Option<&McpCall> {
        self.mcp
            .selected_idx
            .and_then(|idx| self.mcp.calls.get(idx))
    }

    /// Select first MCP call (g key - vim go to top)
    pub fn select_first_mcp(&mut self) {
        if !self.mcp.calls.is_empty() {
            self.mcp.selected_idx = Some(0);
        }
    }

    /// Select last MCP call (G key - vim go to bottom)
    pub fn select_last_mcp(&mut self) {
        if !self.mcp.calls.is_empty() {
            self.mcp.selected_idx = Some(self.mcp.calls.len().saturating_sub(1));
        }
    }

    // ═══════════════════════════════════════════
    // FILTER METHODS (TIER 1.5)
    // ═══════════════════════════════════════════

    /// Add character to filter query
    pub fn filter_push(&mut self, c: char) {
        self.filter_query.insert(self.filter_cursor, c);
        self.filter_cursor += 1;
    }

    /// Remove character before cursor (backspace)
    pub fn filter_backspace(&mut self) {
        if self.filter_cursor > 0 {
            self.filter_cursor -= 1;
            self.filter_query.remove(self.filter_cursor);
        }
    }

    /// Remove character at cursor (delete)
    pub fn filter_delete(&mut self) {
        if self.filter_cursor < self.filter_query.len() {
            self.filter_query.remove(self.filter_cursor);
        }
    }

    /// Move cursor left
    pub fn filter_cursor_left(&mut self) {
        if self.filter_cursor > 0 {
            self.filter_cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn filter_cursor_right(&mut self) {
        if self.filter_cursor < self.filter_query.len() {
            self.filter_cursor += 1;
        }
    }

    /// Clear filter query
    pub fn filter_clear(&mut self) {
        self.filter_query.clear();
        self.filter_cursor = 0;
    }

    /// Check if filter is active
    pub fn has_filter(&self) -> bool {
        !self.filter_query.is_empty()
    }

    /// Get filtered task IDs
    pub fn filtered_task_ids(&self) -> Vec<&String> {
        if self.filter_query.is_empty() {
            return self.task_order.iter().collect();
        }

        let query = self.filter_query.to_lowercase();
        self.task_order
            .iter()
            .filter(|id| {
                // Match task ID
                if id.to_lowercase().contains(&query) {
                    return true;
                }
                // Match task type
                if let Some(task) = self.tasks.get(*id) {
                    if let Some(task_type) = &task.task_type {
                        if task_type.to_lowercase().contains(&query) {
                            return true;
                        }
                    }
                }
                false
            })
            .collect()
    }

    /// Get filtered MCP calls
    pub fn filtered_mcp_calls(&self) -> Vec<&McpCall> {
        if self.filter_query.is_empty() {
            return self.mcp.calls.iter().collect();
        }

        let query = self.filter_query.to_lowercase();
        self.mcp
            .calls
            .iter()
            .filter(|call| {
                // Match server name
                if call.server.to_lowercase().contains(&query) {
                    return true;
                }
                // Match tool name
                if let Some(tool) = &call.tool {
                    if tool.to_lowercase().contains(&query) {
                        return true;
                    }
                }
                // Match resource URI
                if let Some(resource) = &call.resource {
                    if resource.to_lowercase().contains(&query) {
                        return true;
                    }
                }
                false
            })
            .collect()
    }

    /// Toggle pause state
    ///
    /// P0 Fix: Uses workflow.paused as single source of truth
    pub fn toggle_pause(&mut self) {
        self.workflow.paused = !self.workflow.paused;
        // Update phase to match pause state
        if self.workflow.paused {
            self.workflow.phase = MissionPhase::Pause;
        } else if self.current_task.is_some() {
            self.workflow.phase = MissionPhase::Orbital;
        } else {
            self.workflow.phase = MissionPhase::Countdown;
        }
    }

    /// Check if execution is paused (unified accessor)
    pub fn is_paused(&self) -> bool {
        self.workflow.paused
    }

    /// Check if a breakpoint should trigger
    pub fn should_break(&self, kind: &EventKind) -> bool {
        if self.breakpoints.is_empty() {
            return false;
        }

        match kind {
            EventKind::TaskStarted { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::BeforeTask(task_id.to_string())),
            EventKind::TaskCompleted { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::AfterTask(task_id.to_string())),
            EventKind::TaskFailed { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::OnError(task_id.to_string())),
            EventKind::McpInvoke { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::OnMcp(task_id.to_string())),
            EventKind::AgentTurn {
                task_id,
                turn_index,
                ..
            } => self
                .breakpoints
                .contains(&Breakpoint::OnAgentTurn(task_id.to_string(), *turn_index)),
            _ => false,
        }
    }

    /// Check if a task has a breakpoint set (TIER 2.3)
    pub fn has_breakpoint(&self, task_id: &str) -> bool {
        self.breakpoints
            .contains(&Breakpoint::BeforeTask(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::AfterTask(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::OnError(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::OnMcp(task_id.to_string()))
    }

    // ═══════════════════════════════════════════
    // TIMELINE CACHE METHODS
    // ═══════════════════════════════════════════

    /// Invalidate the timeline cache (call when task state changes)
    ///
    /// This increments the version counter, causing the next call to
    /// `ensure_timeline_cache()` to rebuild the entries.
    #[inline]
    pub fn invalidate_timeline_cache(&mut self) {
        self.timeline_version = self.timeline_version.wrapping_add(1);
    }

    /// Ensure the timeline cache is up to date
    ///
    /// Call this before rendering the progress panel to ensure
    /// `cached_timeline_entries` contains the latest data.
    /// Only rebuilds if the version has changed.
    pub fn ensure_timeline_cache(&mut self) {
        if self.timeline_cache_version != self.timeline_version {
            self.rebuild_timeline_cache();
        }
    }

    /// Rebuild the timeline cache from current task state
    fn rebuild_timeline_cache(&mut self) {
        self.cached_timeline_entries.clear();

        for id in &self.task_order {
            if let Some(task) = self.tasks.get(id) {
                let mut entry = TimelineEntry::new(&task.id, task.status);
                if let Some(ms) = task.duration_ms {
                    entry = entry.with_duration(ms);
                }
                if self.current_task.as_ref() == Some(&task.id) {
                    entry = entry.current();
                }
                entry = entry.with_breakpoint(self.has_breakpoint(&task.id));
                self.cached_timeline_entries.push(entry);
            }
        }

        self.timeline_cache_version = self.timeline_version;
    }

    /// Get content suitable for clipboard copy based on focused panel and current tab
    ///
    /// Returns the most relevant content for the current view:
    /// - Progress panel: Final output JSON or current task output
    /// - DAG panel: YAML content or task list
    /// - NovaNet panel: Selected MCP call (params + response)
    /// - Agent panel: Agent turns or thinking content
    pub fn get_copyable_content(&self) -> Option<String> {
        match self.ui.focus {
            PanelId::Progress => {
                // Priority: final output > current task output > metrics summary
                if let Some(ref output) = self.workflow.final_output {
                    Some(serde_json::to_string_pretty(output.as_ref()).unwrap_or_default())
                } else if let Some(ref task_id) = self.current_task {
                    self.tasks.get(task_id).and_then(|task| {
                        task.output
                            .as_ref()
                            .map(|o| serde_json::to_string_pretty(o.as_ref()).unwrap_or_default())
                    })
                } else {
                    // Return metrics summary
                    Some(format!(
                        "Workflow: {}\nTasks: {}/{}\nTokens: {}\nMCP calls: {}",
                        self.workflow.path,
                        self.workflow.tasks_completed,
                        self.workflow.task_count,
                        self.metrics.total_tokens,
                        self.mcp.calls.len()
                    ))
                }
            }
            PanelId::Dag => {
                // Return task list with statuses
                let mut lines = vec!["# DAG Tasks".to_string()];
                for task_id in &self.task_order {
                    if let Some(task) = self.tasks.get(task_id) {
                        let status = match task.status {
                            TaskStatus::Queued => "[.]",
                            TaskStatus::Pending => "[ ]",
                            TaskStatus::Running => "[~]",
                            TaskStatus::Success => "[x]",
                            TaskStatus::Failed => "[!]",
                            TaskStatus::Paused => "[-]",
                            TaskStatus::Skipped => "[/]",
                        };
                        let deps = if task.dependencies.is_empty() {
                            String::new()
                        } else {
                            format!(" -> {}", task.dependencies.join(", "))
                        };
                        lines.push(format!("{} {}{}", status, task_id, deps));
                    }
                }
                Some(lines.join("\n"))
            }
            PanelId::NovaNet => {
                // Return selected MCP call or all calls
                if let Some(idx) = self.mcp.selected_idx {
                    self.mcp.calls.get(idx).map(|call| {
                        let mut content = format!(
                            "# MCP Call #{}: {}\n\n",
                            call.seq + 1,
                            call.tool.as_deref().unwrap_or("resource")
                        );
                        content.push_str("## Request\n");
                        if let Some(ref params) = call.params {
                            content.push_str(
                                &serde_json::to_string_pretty(params).unwrap_or_default(),
                            );
                        }
                        content.push_str("\n\n## Response\n");
                        if let Some(ref response) = call.response {
                            content.push_str(
                                &serde_json::to_string_pretty(response).unwrap_or_default(),
                            );
                        } else if !call.completed {
                            content.push_str("(pending...)");
                        }
                        content
                    })
                } else if !self.mcp.calls.is_empty() {
                    // Return summary of all MCP calls
                    let mut lines = vec!["# MCP Calls".to_string()];
                    for call in &self.mcp.calls {
                        let status = if call.completed { "[x]" } else { "[~]" };
                        let tool = call.tool.as_deref().unwrap_or("resource");
                        let duration = call
                            .duration_ms
                            .map(|d| format!(" {}ms", d))
                            .unwrap_or_default();
                        lines.push(format!(
                            "{} #{} {}:{}{}",
                            status,
                            call.seq + 1,
                            call.server,
                            tool,
                            duration
                        ));
                    }
                    Some(lines.join("\n"))
                } else {
                    None
                }
            }
            PanelId::Agent => {
                // Return agent turns or thinking content
                if self.agent.turns.is_empty() {
                    return None;
                }

                let mut content = String::from("# Agent Turns\n\n");
                for turn in &self.agent.turns {
                    content.push_str(&format!("## Turn {}\n", turn.index + 1));
                    if let Some(ref thinking) = turn.thinking {
                        content.push_str("### Thinking\n");
                        content.push_str(thinking);
                        content.push_str("\n\n");
                    }
                    if let Some(ref response) = turn.response_text {
                        content.push_str("### Response\n");
                        content.push_str(response);
                        content.push_str("\n\n");
                    }
                    if !turn.tool_calls.is_empty() {
                        content.push_str("### Tool Calls\n");
                        for tool in &turn.tool_calls {
                            content.push_str(&format!("- {}\n", tool));
                        }
                        content.push('\n');
                    }
                }
                Some(content)
            }
        }
    }

    // ═══════════════════════════════════════════
    // RETRY SUPPORT (TIER 1.2)
    // ═══════════════════════════════════════════

    /// Check if the workflow is in a failed state (can be retried)
    pub fn is_failed(&self) -> bool {
        self.workflow.phase == MissionPhase::Abort || self.workflow.error_message.is_some()
    }

    /// Check if the workflow completed successfully
    pub fn is_success(&self) -> bool {
        self.workflow.phase == MissionPhase::MissionSuccess
    }

    /// Check if the workflow is still running
    pub fn is_running(&self) -> bool {
        matches!(
            self.workflow.phase,
            MissionPhase::Countdown
                | MissionPhase::Launch
                | MissionPhase::Orbital
                | MissionPhase::Rendezvous
        )
    }

    /// Reset state for retry - clears failed tasks and resets workflow phase
    ///
    /// Returns the list of task IDs that were reset (previously failed)
    pub fn reset_for_retry(&mut self) -> Vec<String> {
        let mut reset_tasks = Vec::new();

        // Reset workflow state
        self.workflow.phase = MissionPhase::Preflight;
        self.workflow.error_message = None;
        self.workflow.final_output = None;
        self.workflow.total_duration_ms = None;
        self.workflow.tasks_completed = 0;
        self.workflow.started_at = None;

        // Reset all failed tasks to pending
        for (task_id, task) in &mut self.tasks {
            if task.status == TaskStatus::Failed {
                task.status = TaskStatus::Pending;
                task.duration_ms = None;
                task.error = None;
                task.output = None;
                reset_tasks.push(task_id.clone());
            }
        }

        // Clear current task
        self.current_task = None;

        // Clear agent turns
        self.agent.turns.clear();

        // Reset metrics
        self.metrics = Metrics::default();

        // Clear MCP calls (keep for reference? or clear?)
        // For now, keep them as history but mark workflow as ready for retry
        self.mcp.seq = 0;

        reset_tasks
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NOTIFICATION METHODS (TIER 3.4)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Add a notification (TIER 3.4)
    ///
    /// Delegates to NotificationState slice, then marks dirty.
    pub fn add_notification(&mut self, notification: Notification) {
        self.notifs.push(notification);
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Get active (non-dismissed) notifications
    pub fn active_notifications(&self) -> Vec<&Notification> {
        self.notifs.active()
    }

    /// Get count of active notifications
    pub fn active_notification_count(&self) -> usize {
        self.notifs.active_count()
    }

    /// Dismiss the most recent notification
    pub fn dismiss_notification(&mut self) {
        self.notifs.dismiss_latest();
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Dismiss all notifications
    pub fn dismiss_all_notifications(&mut self) {
        self.notifs.dismiss_all();
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Clear all notifications (removes from list entirely)
    pub fn clear_notifications(&mut self) {
        self.notifs.items.clear();
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Get current DAG version for cache invalidation
    ///
    /// Uses timeline_version since DAG changes track task changes.
    pub fn dag_version(&self) -> u32 {
        self.timeline_version
    }

    /// Clear all dirty flags after render completes
    ///
    /// Call this at the end of render_unified_frame() to reset
    /// dirty state for the next frame.
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// P3 Fix: Dismiss error message
    /// Clears the workflow error message without resetting the entire workflow state
    pub fn dismiss_error(&mut self) -> bool {
        if self.workflow.error_message.is_some() {
            self.workflow.error_message = None;
            self.dirty.progress = true;
            self.dirty.status = true;
            true
        } else {
            false
        }
    }
}
