// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI State Management
//!
//! Central state for the TUI application.
//! Updated by events from the runtime, queried by panels for rendering.
//!
//! ## Module Structure
//!
//! - `types` - Core types (MonitorPanel, TuiMode, WorkflowState, TaskState, etc.)
//! - `scroll` - Panel scroll state management
//! - `notification` - Notification system
//! - `settings` - Settings overlay state
//! - `chat_overlay` - Chat overlay message types (data-only, used by session persistence)
//! - `cache` - JSON formatting cache
//! - `event_handler` - Runtime event processing (handle_event, tick, animation)
//! - `workflow_ops` - Workflow status, pause/debug, retry, dirty flags
//! - `navigation` - Panel focus, MCP nav, filter/search, clipboard, status messages
//! - `timeline` - Timeline cache management
//! - `notification_ops` - Notification delegation methods
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
mod event_handler;
mod mcp_state;
mod navigation;
mod notification;
mod notification_ops;
mod notification_state;
mod scroll;
mod settings;
mod timeline;
mod types;
mod ui;
mod workflow_ops;

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
    MonitorPanel,
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

use nika_engine::config::NikaConfig;

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
    pub(crate) timeline_version: u32,
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
// TuiState CONSTRUCTOR
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
}
