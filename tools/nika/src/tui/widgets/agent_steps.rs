//! Agent Steps Widget - Real-time feedback for agent actions (Option C: Full Detail)
//!
//! Provides Claude Code-like step-by-step feedback with comprehensive metadata:
//!
//! ```text
//! ╭─ agent: Orchestrate QR code generation ─────────────────────────────────╮
//! │ [agent] ● Orchestrating task... (turn 1/5)                    ⏱ 2.3s    │
//! │   ├─ [invoke] ✓ novanet_describe                              ⏱ 0.8s    │
//! │   │    └─ server: novanet | params: {entity: "qr-code"}                 │
//! │   │    └─ result: {"name": "QR Code", "desc": "..."} (245 chars)        │
//! │   ├─ [infer] ● Generating landing page content...             ⏱ 1.2s    │
//! │   │    └─ model: claude-sonnet-4-6 | tokens: 1,234 in → 567 out         │
//! │   │    └─ streaming: ████████░░ 80% (453/567 tokens)                    │
//! │   └─ [exec] ○ npm run build (pending)                                   │
//! │                                                                         │
//! │ 📊 Total: 1,801 tokens | $0.0054 | 3 tools called                       │
//! ╰─────────────────────────────────────────────────────────────────────────╯
//! ```
//!
//! ## Features
//!
//! - **Verb-colored tag pills**: `[infer]` with verb-specific colors (PLUQQY-style)
//! - **Nested verb hierarchy**: Agent → Invoke → Fetch with tree lines
//! - **Token tracking**: Input/output tokens with cost estimation
//! - **Tool metadata**: MCP server, params preview, result preview
//! - **Streaming progress**: Real-time token counter with progress bar
//! - **Error details**: Error code, message, suggestion
//! - **Collapsible groups**: Expand/collapse multi-turn agents
//!
//! ## Verb Colors (ADR-001 compliant)
//!
//! - infer  → Violet (#6366f1) - LLM generation
//! - exec   → Green  (#22c55e) - Shell commands
//! - fetch  → Cyan   (#06b6d4) - HTTP requests
//! - invoke → Blue   (#3b82f6) - MCP tool calls
//! - agent  → Gold   (#eab308) - Agentic loops

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

// Re-export VerbType from dag module (single source of truth)
pub use super::dag::VerbType;

// ═══════════════════════════════════════════════════════════════════════════════
// COLOR PALETTE
// ═══════════════════════════════════════════════════════════════════════════════

// Status colors
const COLOR_RUNNING: Color = Color::Rgb(250, 204, 21); // Yellow/Gold - in progress
const COLOR_SUCCESS: Color = Color::Rgb(34, 197, 94); // Green - completed
const COLOR_ERROR: Color = Color::Rgb(239, 68, 68); // Red - failed
const COLOR_MUTED: Color = Color::Rgb(88, 110, 117); // Gray - tree lines
const COLOR_CONTENT: Color = Color::Rgb(147, 161, 161); // Light gray - step text
const COLOR_TOKENS: Color = Color::Rgb(168, 162, 158); // Warm gray - token counts
const COLOR_COST: Color = Color::Rgb(251, 191, 36); // Amber - cost display
const COLOR_BORDER: Color = Color::Rgb(71, 85, 105); // Slate - box borders
const COLOR_HEADER: Color = Color::Rgb(248, 250, 252); // Near-white - headers
const COLOR_DIMMED: Color = Color::Rgb(100, 116, 139); // Slate-500 - secondary text

/// Spinner frames for running steps
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Progress bar characters
const PROGRESS_FILLED: char = '█';
const PROGRESS_EMPTY: char = '░';

/// Step status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepStatus {
    #[default]
    Pending, // Not started yet
    Running,   // Currently executing (shows spinner)
    Completed, // Finished successfully
    Failed,    // Failed with error
    Skipped,   // Skipped (conditional)
}

impl StepStatus {
    /// Get status indicator (char, color) based on frame for animation
    pub fn indicator(&self, frame: u8) -> (char, Color) {
        match self {
            StepStatus::Pending => ('○', COLOR_MUTED),
            StepStatus::Running => {
                let idx = (frame as usize) % SPINNER_FRAMES.len();
                (SPINNER_FRAMES[idx], COLOR_RUNNING)
            }
            StepStatus::Completed => ('✓', COLOR_SUCCESS),
            StepStatus::Failed => ('✗', COLOR_ERROR),
            StepStatus::Skipped => ('◌', COLOR_MUTED),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TOKEN USAGE (Option C: Full Token Tracking)
// ═══════════════════════════════════════════════════════════════════════════════

/// Token usage statistics for a step (infer: and agent:)
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    /// Input tokens (prompt + context)
    pub input_tokens: u32,
    /// Output tokens (response)
    pub output_tokens: u32,
    /// Cache read tokens (if supported by provider)
    pub cache_read_tokens: Option<u32>,
    /// Cache write tokens (if supported by provider)
    pub cache_write_tokens: Option<u32>,
}

impl TokenUsage {
    pub fn new(input: u32, output: u32) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: None,
            cache_write_tokens: None,
        }
    }

    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// Estimate cost in USD (Claude pricing: $3/M input, $15/M output)
    pub fn estimated_cost(&self) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * 3.0;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * 15.0;
        input_cost + output_cost
    }

    /// Format as "1,234 in → 567 out"
    pub fn format_compact(&self) -> String {
        format!(
            "{} in → {} out",
            format_number(self.input_tokens),
            format_number(self.output_tokens)
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TOOL CALL METADATA (Option C: MCP/Tool Details)
// ═══════════════════════════════════════════════════════════════════════════════

/// Metadata for tool/MCP calls (invoke: verb)
#[derive(Debug, Clone, Default)]
pub struct ToolCallMetadata {
    /// Tool name (e.g., "novanet_describe")
    pub tool_name: String,
    /// MCP server name (e.g., "novanet")
    pub server_name: Option<String>,
    /// Tool parameters as JSON string (truncated for display)
    pub params_preview: Option<String>,
    /// Tool result preview (truncated)
    pub result_preview: Option<String>,
    /// Result size in bytes/chars
    pub result_size: Option<usize>,
    /// Call duration
    pub call_duration: Option<Duration>,
    /// HTTP status code (for fetch:)
    pub http_status: Option<u16>,
    /// Retry count if retried
    pub retry_count: u8,
}

impl ToolCallMetadata {
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            ..Default::default()
        }
    }

    pub fn with_server(mut self, server: impl Into<String>) -> Self {
        self.server_name = Some(server.into());
        self
    }

    pub fn with_params(mut self, params: impl Into<String>) -> Self {
        let params = params.into();
        self.params_preview = Some(truncate_json(&params, 80));
        self
    }

    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        let result = result.into();
        self.result_size = Some(result.len());
        self.result_preview = Some(truncate_json(&result, 60));
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.call_duration = Some(duration);
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// STREAMING PROGRESS (Option C: Real-time Progress)
// ═══════════════════════════════════════════════════════════════════════════════

/// Streaming progress for infer: steps
#[derive(Debug, Clone, Default)]
pub struct StreamingProgress {
    /// Tokens generated so far
    pub tokens_generated: u32,
    /// Expected total tokens (if known)
    pub tokens_expected: Option<u32>,
    /// Characters received
    pub chars_received: usize,
    /// Is currently streaming
    pub is_streaming: bool,
    /// Chunks received count
    pub chunks_received: u32,
    /// Last chunk timestamp
    pub last_chunk_at: Option<Instant>,
}

impl StreamingProgress {
    pub fn new() -> Self {
        Self {
            is_streaming: true,
            ..Default::default()
        }
    }

    pub fn progress_percent(&self) -> Option<u8> {
        self.tokens_expected.map(|expected| {
            if expected == 0 {
                100
            } else {
                ((self.tokens_generated as f32 / expected as f32) * 100.0).min(100.0) as u8
            }
        })
    }

    /// Render a progress bar: "████████░░ 80%"
    pub fn render_progress_bar(&self, width: usize) -> String {
        match self.progress_percent() {
            Some(percent) => {
                let filled = (width * percent as usize) / 100;
                let empty = width - filled;
                format!(
                    "{}{} {}%",
                    PROGRESS_FILLED.to_string().repeat(filled),
                    PROGRESS_EMPTY.to_string().repeat(empty),
                    percent
                )
            }
            None => {
                // Indeterminate: show token count only
                format!("{} tokens", format_number(self.tokens_generated))
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ERROR DETAILS (Option C: Rich Error Information)
// ═══════════════════════════════════════════════════════════════════════════════

/// Detailed error information for failed steps
#[derive(Debug, Clone, Default)]
pub struct ErrorDetails {
    /// Error code (e.g., "NIKA-102")
    pub code: Option<String>,
    /// Error message
    pub message: String,
    /// Suggestion for fixing
    pub suggestion: Option<String>,
    /// Stack trace or context (truncated)
    pub context: Option<String>,
    /// Is recoverable (can retry)
    pub recoverable: bool,
}

impl ErrorDetails {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Default::default()
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Format error for display: "[NIKA-102] Message"
    pub fn format_short(&self) -> String {
        match &self.code {
            Some(code) => format!("[{}] {}", code, self.message),
            None => self.message.clone(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MODEL INFO (Option C: LLM Provider Details)
// ═══════════════════════════════════════════════════════════════════════════════

/// Model/provider information for infer: and agent: steps
#[derive(Debug, Clone, Default)]
pub struct ModelInfo {
    /// Model name (e.g., "claude-sonnet-4-6")
    pub model: String,
    /// Provider name (e.g., "claude", "openai")
    pub provider: Option<String>,
    /// Temperature setting
    pub temperature: Option<f32>,
    /// Max tokens setting
    pub max_tokens: Option<u32>,
    /// System prompt preview (truncated)
    pub system_prompt_preview: Option<String>,
}

impl ModelInfo {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// Format as "claude-sonnet-4-6 (Claude)"
    pub fn format_display(&self) -> String {
        match &self.provider {
            Some(provider) => format!("{} ({})", self.model, provider),
            None => self.model.clone(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Format a number with thousands separator: 1234567 → "1,234,567"
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Truncate JSON string for preview, preserving structure hints
fn truncate_json(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    // Try to find a good break point
    let truncated = &s[..max_len.min(s.len())];

    // Find last complete key-value or bracket
    if let Some(last_comma) = truncated.rfind(',') {
        format!("{}...", &truncated[..last_comma])
    } else if let Some(last_brace) = truncated.rfind('{') {
        format!("{}...}}", &truncated[..=last_brace])
    } else {
        format!("{}...", truncated)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AGENT STEP (Option C: Full Detail Version)
// ═══════════════════════════════════════════════════════════════════════════════

/// A single step in an agent action (Option C: Full Detail Version)
///
/// Contains comprehensive metadata for Claude Code-like feedback:
/// - Token tracking (input/output with cost estimation)
/// - Tool metadata (MCP server, params, result preview)
/// - Streaming progress (real-time token counter)
/// - Error details (code, message, suggestion)
/// - Nested children for verb hierarchy visualization
#[derive(Debug, Clone)]
pub struct AgentStep {
    // ═══════════════════════════════════════════════════════════════════════════
    // CORE FIELDS (original)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Step description (e.g., "Writing order-to-payment.py")
    pub description: String,
    /// Step status
    pub status: StepStatus,
    /// When this step started
    pub started_at: Instant,
    /// Duration (set when completed)
    pub duration: Option<Duration>,
    /// Optional detail (e.g., file path, tool name)
    pub detail: Option<String>,
    /// Animation frame
    pub frame: u8,
    /// Verb that triggered this step (for color-coded rendering)
    pub verb: Option<VerbType>,
    /// Parent verb (for nested calls - e.g., agent calling invoke)
    pub parent_verb: Option<VerbType>,
    /// Nesting depth (0 = root, 1 = first nested, etc.)
    pub depth: u8,

    // ═══════════════════════════════════════════════════════════════════════════
    // OPTION C: ENHANCED DETAIL FIELDS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Token usage for infer:/agent: steps
    pub tokens: Option<TokenUsage>,
    /// Tool call metadata for invoke: steps
    pub tool_call: Option<ToolCallMetadata>,
    /// Streaming progress for running infer: steps
    pub streaming: Option<StreamingProgress>,
    /// Error details for failed steps
    pub error: Option<ErrorDetails>,
    /// Model info for infer:/agent: steps
    pub model: Option<ModelInfo>,
    /// Nested child steps (for agent: calling other verbs)
    pub children: Vec<AgentStep>,
    /// Task ID (for trace correlation)
    pub task_id: Option<String>,
    /// Turn number (for agent: multi-turn)
    pub turn_number: Option<u32>,
    /// Max turns (for agent: context)
    pub max_turns: Option<u32>,
    /// Command executed (for exec:)
    pub command: Option<String>,
    /// Exit code (for exec:)
    pub exit_code: Option<i32>,
    /// URL fetched (for fetch:)
    pub url: Option<String>,
    /// Is this step collapsed in the UI
    pub collapsed: bool,
}

impl AgentStep {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            status: StepStatus::Pending,
            started_at: Instant::now(),
            duration: None,
            detail: None,
            frame: 0,
            verb: None,
            parent_verb: None,
            depth: 0,
            // Option C fields
            tokens: None,
            tool_call: None,
            streaming: None,
            error: None,
            model: None,
            children: Vec::new(),
            task_id: None,
            turn_number: None,
            max_turns: None,
            command: None,
            exit_code: None,
            url: None,
            collapsed: false,
        }
    }

    pub fn running(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            status: StepStatus::Running,
            started_at: Instant::now(),
            duration: None,
            detail: None,
            frame: 0,
            verb: None,
            parent_verb: None,
            depth: 0,
            // Option C fields
            tokens: None,
            tool_call: None,
            streaming: None,
            error: None,
            model: None,
            children: Vec::new(),
            task_id: None,
            turn_number: None,
            max_turns: None,
            command: None,
            exit_code: None,
            url: None,
            collapsed: false,
        }
    }

    pub fn completed(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            status: StepStatus::Completed,
            started_at: Instant::now(),
            duration: Some(Duration::ZERO),
            detail: None,
            frame: 0,
            verb: None,
            parent_verb: None,
            depth: 0,
            // Option C fields
            tokens: None,
            tool_call: None,
            streaming: None,
            error: None,
            model: None,
            children: Vec::new(),
            task_id: None,
            turn_number: None,
            max_turns: None,
            command: None,
            exit_code: None,
            url: None,
            collapsed: false,
        }
    }

    /// Create a step for a specific verb
    pub fn for_verb(verb: VerbType, description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            status: StepStatus::Running,
            started_at: Instant::now(),
            duration: None,
            detail: None,
            frame: 0,
            verb: Some(verb),
            parent_verb: None,
            depth: 0,
            // Option C fields
            tokens: None,
            tool_call: None,
            streaming: None,
            error: None,
            model: None,
            children: Vec::new(),
            task_id: None,
            turn_number: None,
            max_turns: None,
            command: None,
            exit_code: None,
            url: None,
            collapsed: false,
        }
    }

    /// Create a nested step (called by parent verb)
    pub fn nested(parent: VerbType, verb: VerbType, description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            status: StepStatus::Running,
            started_at: Instant::now(),
            duration: None,
            detail: None,
            frame: 0,
            verb: Some(verb),
            parent_verb: Some(parent),
            depth: 1,
            // Option C fields
            tokens: None,
            tool_call: None,
            streaming: None,
            error: None,
            model: None,
            children: Vec::new(),
            task_id: None,
            turn_number: None,
            max_turns: None,
            command: None,
            exit_code: None,
            url: None,
            collapsed: false,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_verb(mut self, verb: VerbType) -> Self {
        self.verb = Some(verb);
        self
    }

    pub fn with_parent(mut self, parent: VerbType) -> Self {
        self.parent_verb = Some(parent);
        self.depth = 1;
        self
    }

    pub fn with_depth(mut self, depth: u8) -> Self {
        self.depth = depth;
        self
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // OPTION C: BUILDER METHODS FOR ENHANCED DETAILS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set token usage for infer:/agent: steps
    pub fn with_tokens(mut self, tokens: TokenUsage) -> Self {
        self.tokens = Some(tokens);
        self
    }

    /// Set tool call metadata for invoke: steps
    pub fn with_tool_call(mut self, tool_call: ToolCallMetadata) -> Self {
        self.tool_call = Some(tool_call);
        self
    }

    /// Set streaming progress for running infer: steps
    pub fn with_streaming(mut self, streaming: StreamingProgress) -> Self {
        self.streaming = Some(streaming);
        self
    }

    /// Set error details for failed steps
    pub fn with_error(mut self, error: ErrorDetails) -> Self {
        self.error = Some(error);
        self.status = StepStatus::Failed;
        self
    }

    /// Set model info for infer:/agent: steps
    pub fn with_model(mut self, model: ModelInfo) -> Self {
        self.model = Some(model);
        self
    }

    /// Set task ID for trace correlation
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Set turn number for agent: multi-turn
    pub fn with_turn(mut self, turn: u32, max: Option<u32>) -> Self {
        self.turn_number = Some(turn);
        self.max_turns = max;
        self
    }

    /// Set command for exec: steps
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Set exit code for exec: steps
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    /// Set URL for fetch: steps
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Set collapsed state
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Add a child step (for nested verb hierarchy)
    pub fn add_child(&mut self, child: AgentStep) {
        self.children.push(child);
    }

    /// Check if this step has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Get total tokens (self + children)
    pub fn total_tokens(&self) -> TokenUsage {
        let mut total = self.tokens.clone().unwrap_or_default();

        for child in &self.children {
            let child_tokens = child.total_tokens();
            total.input_tokens += child_tokens.input_tokens;
            total.output_tokens += child_tokens.output_tokens;
            if let Some(cache_read) = child_tokens.cache_read_tokens {
                *total.cache_read_tokens.get_or_insert(0) += cache_read;
            }
            if let Some(cache_write) = child_tokens.cache_write_tokens {
                *total.cache_write_tokens.get_or_insert(0) += cache_write;
            }
        }

        total
    }

    /// Get total cost (self + children)
    pub fn total_cost(&self) -> f64 {
        self.total_tokens().estimated_cost()
    }

    /// Count total tool calls (self + children)
    pub fn total_tool_calls(&self) -> usize {
        let self_count = if self.tool_call.is_some() { 1 } else { 0 };
        self_count + self.children.iter().map(|c| c.total_tool_calls()).sum::<usize>()
    }

    /// Start the step
    pub fn start(&mut self) {
        self.status = StepStatus::Running;
        self.started_at = Instant::now();
    }

    /// Complete the step successfully
    pub fn complete(&mut self) {
        self.status = StepStatus::Completed;
        self.duration = Some(self.started_at.elapsed());
    }

    /// Fail the step
    pub fn fail(&mut self) {
        self.status = StepStatus::Failed;
        self.duration = Some(self.started_at.elapsed());
    }

    /// Tick animation frame
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.duration.unwrap_or_else(|| self.started_at.elapsed())
    }
}

/// A group of steps for one agent turn/message
#[derive(Debug, Clone, Default)]
pub struct AgentStepGroup {
    /// The prompt/command that triggered these steps
    pub prompt: String,
    /// Steps in execution order
    pub steps: Vec<AgentStep>,
    /// Overall status
    pub status: StepStatus,
    /// When the group started
    pub started_at: Option<Instant>,
    /// Total duration
    pub duration: Option<Duration>,
    /// Primary verb for this group
    pub verb: Option<VerbType>,
}

impl AgentStepGroup {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            steps: Vec::new(),
            status: StepStatus::Pending,
            started_at: None,
            duration: None,
            verb: None,
        }
    }

    /// Create a group for a specific verb
    pub fn for_verb(verb: VerbType, prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            steps: Vec::new(),
            status: StepStatus::Pending,
            started_at: None,
            duration: None,
            verb: Some(verb),
        }
    }

    pub fn with_verb(mut self, verb: VerbType) -> Self {
        self.verb = Some(verb);
        self
    }

    /// Start executing this group
    pub fn start(&mut self) {
        self.status = StepStatus::Running;
        self.started_at = Some(Instant::now());
    }

    /// Add a step
    pub fn add_step(&mut self, step: AgentStep) {
        self.steps.push(step);
    }

    /// Add a running step (convenience)
    pub fn add_running(&mut self, description: impl Into<String>) {
        self.steps.push(AgentStep::running(description));
    }

    /// Complete the current running step
    pub fn complete_current(&mut self) {
        if let Some(step) = self
            .steps
            .iter_mut()
            .rev()
            .find(|s| s.status == StepStatus::Running)
        {
            step.complete();
        }
    }

    /// Fail the current running step
    pub fn fail_current(&mut self) {
        if let Some(step) = self
            .steps
            .iter_mut()
            .rev()
            .find(|s| s.status == StepStatus::Running)
        {
            step.fail();
        }
        self.status = StepStatus::Failed;
    }

    /// Complete the entire group
    pub fn complete(&mut self) {
        // Complete any remaining running steps
        for step in &mut self.steps {
            if step.status == StepStatus::Running {
                step.complete();
            }
        }
        self.status = StepStatus::Completed;
        if let Some(started) = self.started_at {
            self.duration = Some(started.elapsed());
        }
    }

    /// Tick all running steps
    pub fn tick(&mut self) {
        for step in &mut self.steps {
            if step.status == StepStatus::Running {
                step.tick();
            }
        }
    }

    /// Check if any step is running
    pub fn is_running(&self) -> bool {
        self.steps.iter().any(|s| s.status == StepStatus::Running)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // OPTION C: AGGREGATION METHODS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get total tokens across all steps (including nested children)
    pub fn total_tokens(&self) -> TokenUsage {
        let mut total = TokenUsage::default();
        for step in &self.steps {
            let step_tokens = step.total_tokens();
            total.input_tokens += step_tokens.input_tokens;
            total.output_tokens += step_tokens.output_tokens;
            if let Some(cache_read) = step_tokens.cache_read_tokens {
                *total.cache_read_tokens.get_or_insert(0) += cache_read;
            }
            if let Some(cache_write) = step_tokens.cache_write_tokens {
                *total.cache_write_tokens.get_or_insert(0) += cache_write;
            }
        }
        total
    }

    /// Get total estimated cost in USD
    pub fn total_cost(&self) -> f64 {
        self.total_tokens().estimated_cost()
    }

    /// Count total tool calls across all steps
    pub fn total_tool_calls(&self) -> usize {
        self.steps.iter().map(|s| s.total_tool_calls()).sum()
    }

    /// Count completed steps
    pub fn completed_steps(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Completed)
            .count()
    }

    /// Count failed steps
    pub fn failed_steps(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Failed)
            .count()
    }

    /// Format a summary line: "📊 Total: 1,801 tokens | $0.0054 | 3 tools called"
    pub fn format_summary(&self) -> String {
        let tokens = self.total_tokens();
        let cost = tokens.estimated_cost();
        let tool_calls = self.total_tool_calls();

        format!(
            "📊 Total: {} tokens | ${:.4} | {} tools called",
            format_number(tokens.total()),
            cost,
            tool_calls
        )
    }
}

/// Widget to render agent steps (Claude Code-like)
pub struct AgentStepsWidget<'a> {
    group: &'a AgentStepGroup,
    show_prompt: bool,
    compact: bool,
}

impl<'a> AgentStepsWidget<'a> {
    pub fn new(group: &'a AgentStepGroup) -> Self {
        Self {
            group,
            show_prompt: true,
            compact: false,
        }
    }

    pub fn show_prompt(mut self, show: bool) -> Self {
        self.show_prompt = show;
        self
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Render a verb tag pill: [infer] with verb color
    fn render_verb_pill(verb: VerbType) -> Vec<Span<'a>> {
        let color = verb.color();
        vec![
            Span::styled("[", Style::default().fg(color)),
            Span::styled(
                verb.label(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("] ", Style::default().fg(color)),
        ]
    }

    /// Render indentation for nested steps
    fn render_indent(depth: u8, parent_verb: Option<VerbType>) -> Vec<Span<'a>> {
        if depth == 0 {
            return vec![];
        }

        let mut spans = Vec::new();
        for _ in 0..depth {
            // Use parent verb color for indent line if available
            let color = parent_verb.map(|v| v.color()).unwrap_or(COLOR_MUTED);
            spans.push(Span::styled("│ ", Style::default().fg(color)));
        }
        spans
    }

    /// Render a detail sub-line with tree continuation
    fn render_detail_line(
        depth: u8,
        parent_verb: Option<VerbType>,
        is_last_detail: bool,
        content: Vec<Span<'a>>,
    ) -> Line<'a> {
        let mut spans = Vec::new();

        // Indentation for depth
        for _ in 0..depth {
            let color = parent_verb.map(|v| v.color()).unwrap_or(COLOR_MUTED);
            spans.push(Span::styled("│ ", Style::default().fg(color)));
        }

        // Tree continuation
        let tree_char = if is_last_detail { "└─ " } else { "├─ " };
        spans.push(Span::styled(tree_char, Style::default().fg(COLOR_MUTED)));

        // Content
        spans.extend(content);

        Line::from(spans)
    }

    /// Render a collapsed step (summary only)
    fn render_collapsed_step(&self, step: &AgentStep, index: usize) -> Vec<Line<'a>> {
        let is_last = index == self.group.steps.len() - 1;
        let (indicator, status_color) = step.status.indicator(step.frame);

        let mut spans = Vec::new();

        // Indentation
        spans.extend(Self::render_indent(step.depth, step.parent_verb));

        // Verb pill
        if let Some(verb) = step.verb {
            spans.extend(Self::render_verb_pill(verb));
        }

        // Collapsed indicator
        spans.push(Span::styled("▶ ", Style::default().fg(COLOR_DIMMED)));

        // Status
        let tree = if is_last { "└" } else { "├" };
        let status_char = match step.status {
            StepStatus::Running => format!("{}", indicator),
            StepStatus::Completed => "✓".to_string(),
            StepStatus::Failed => "✗".to_string(),
            _ => "○".to_string(),
        };
        spans.push(Span::styled(
            format!("{} {} ", tree, status_char),
            Style::default().fg(status_color),
        ));

        // Description
        spans.push(Span::styled(
            step.description.clone(),
            Style::default().fg(COLOR_CONTENT),
        ));

        // Children count
        if !step.children.is_empty() {
            spans.push(Span::styled(
                format!(" ({} nested)", step.children.len()),
                Style::default().fg(COLOR_DIMMED),
            ));
        }

        vec![Line::from(spans)]
    }

    /// Render an expanded step with full Option C details
    fn render_expanded_step(&self, step: &'a AgentStep, index: usize) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let is_last = index == self.group.steps.len() - 1;
        let (indicator, status_color) = step.status.indicator(step.frame);

        // Main step line
        let mut spans = Vec::new();

        // Indentation
        spans.extend(Self::render_indent(step.depth, step.parent_verb));

        // Verb pill
        if let Some(verb) = step.verb {
            spans.extend(Self::render_verb_pill(verb));
        }

        // Tree/status indicator
        match step.status {
            StepStatus::Running => {
                let color = step.verb.map(|v| v.color()).unwrap_or(status_color);
                spans.push(Span::styled(
                    format!("{} ", indicator),
                    Style::default().fg(color),
                ));
            }
            StepStatus::Completed => {
                let tree = if is_last && step.children.is_empty() {
                    "└"
                } else {
                    "├"
                };
                spans.push(Span::styled(
                    format!("{} ✓ ", tree),
                    Style::default().fg(COLOR_SUCCESS),
                ));
            }
            StepStatus::Failed => {
                spans.push(Span::styled("✗ ", Style::default().fg(COLOR_ERROR)));
            }
            StepStatus::Pending | StepStatus::Skipped => {
                let tree = if is_last { "└" } else { "├" };
                spans.push(Span::styled(
                    format!("{} ○ ", tree),
                    Style::default().fg(COLOR_MUTED),
                ));
            }
        }

        // Description
        let desc_color = if step.status == StepStatus::Running {
            step.verb.map(|v| v.color()).unwrap_or(COLOR_CONTENT)
        } else {
            COLOR_CONTENT
        };
        spans.push(Span::styled(
            step.description.clone(),
            Style::default().fg(desc_color),
        ));

        // Turn info for agent steps
        if let (Some(turn), Some(max)) = (step.turn_number, step.max_turns) {
            spans.push(Span::styled(
                format!(" (turn {}/{})", turn, max),
                Style::default().fg(COLOR_DIMMED),
            ));
        }

        // Duration
        if step.status == StepStatus::Completed || step.status == StepStatus::Failed {
            if let Some(duration) = step.duration {
                if duration.as_millis() > 100 {
                    spans.push(Span::styled(
                        format!(" ⏱ {:.1}s", duration.as_secs_f64()),
                        Style::default().fg(COLOR_MUTED),
                    ));
                }
            }
        }

        lines.push(Line::from(spans));

        // Detail sub-lines (Option C)
        let detail_depth = step.depth + 1;
        let detail_verb = step.verb;

        // Model info for infer:/agent:
        if let Some(ref model) = step.model {
            let mut content = vec![
                Span::styled("model: ", Style::default().fg(COLOR_DIMMED)),
                Span::styled(
                    model.format_display(),
                    Style::default().fg(COLOR_CONTENT),
                ),
            ];
            if let Some(temp) = model.temperature {
                content.push(Span::styled(
                    format!(" | temp: {:.1}", temp),
                    Style::default().fg(COLOR_DIMMED),
                ));
            }
            lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));
        }

        // Token usage for infer:/agent:
        if let Some(ref tokens) = step.tokens {
            let content = vec![
                Span::styled("tokens: ", Style::default().fg(COLOR_DIMMED)),
                Span::styled(
                    tokens.format_compact(),
                    Style::default().fg(COLOR_TOKENS),
                ),
                Span::styled(
                    format!(" (${:.4})", tokens.estimated_cost()),
                    Style::default().fg(COLOR_COST),
                ),
            ];
            lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));
        }

        // Streaming progress for running infer:
        if let Some(ref streaming) = step.streaming {
            if streaming.is_streaming {
                let content = vec![
                    Span::styled("streaming: ", Style::default().fg(COLOR_DIMMED)),
                    Span::styled(
                        streaming.render_progress_bar(10),
                        Style::default().fg(COLOR_RUNNING),
                    ),
                ];
                lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));
            }
        }

        // Tool call metadata for invoke:
        if let Some(ref tool) = step.tool_call {
            let mut content = vec![
                Span::styled("tool: ", Style::default().fg(COLOR_DIMMED)),
                Span::styled(&tool.tool_name, Style::default().fg(COLOR_CONTENT)),
            ];
            if let Some(ref server) = tool.server_name {
                content.push(Span::styled(
                    format!(" @ {}", server),
                    Style::default().fg(COLOR_DIMMED),
                ));
            }
            lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));

            // Params preview
            if let Some(ref params) = tool.params_preview {
                let content = vec![
                    Span::styled("params: ", Style::default().fg(COLOR_DIMMED)),
                    Span::styled(params.clone(), Style::default().fg(COLOR_MUTED)),
                ];
                lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));
            }

            // Result preview
            if let Some(ref result) = tool.result_preview {
                let mut content = vec![
                    Span::styled("result: ", Style::default().fg(COLOR_DIMMED)),
                    Span::styled(result.clone(), Style::default().fg(COLOR_MUTED)),
                ];
                if let Some(size) = tool.result_size {
                    content.push(Span::styled(
                        format!(" ({} chars)", format_number(size as u32)),
                        Style::default().fg(COLOR_DIMMED),
                    ));
                }
                lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));
            }
        }

        // Command for exec:
        if let Some(ref command) = step.command {
            let content = vec![
                Span::styled("$ ", Style::default().fg(COLOR_DIMMED)),
                Span::styled(command.clone(), Style::default().fg(COLOR_CONTENT)),
            ];
            lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));

            // Exit code
            if let Some(code) = step.exit_code {
                let (color, prefix) = if code == 0 {
                    (COLOR_SUCCESS, "exit")
                } else {
                    (COLOR_ERROR, "exit")
                };
                let content = vec![
                    Span::styled(format!("{}: ", prefix), Style::default().fg(COLOR_DIMMED)),
                    Span::styled(format!("{}", code), Style::default().fg(color)),
                ];
                lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));
            }
        }

        // URL for fetch:
        if let Some(ref url) = step.url {
            let content = vec![
                Span::styled("url: ", Style::default().fg(COLOR_DIMMED)),
                Span::styled(url.clone(), Style::default().fg(COLOR_CONTENT)),
            ];
            lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));
        }

        // Error details
        if let Some(ref error) = step.error {
            let mut content = vec![Span::styled(
                error.format_short(),
                Style::default().fg(COLOR_ERROR),
            )];
            if error.recoverable {
                content.push(Span::styled(
                    " (recoverable)",
                    Style::default().fg(COLOR_DIMMED),
                ));
            }
            lines.push(Self::render_detail_line(detail_depth, detail_verb, false, content));

            // Suggestion
            if let Some(ref suggestion) = error.suggestion {
                let content = vec![
                    Span::styled("💡 ", Style::default().fg(COLOR_RUNNING)),
                    Span::styled(suggestion.clone(), Style::default().fg(COLOR_CONTENT)),
                ];
                lines.push(Self::render_detail_line(detail_depth, detail_verb, true, content));
            }
        }

        // Nested children (recursive)
        for (child_idx, child) in step.children.iter().enumerate() {
            let child_is_last = child_idx == step.children.len() - 1;

            // Recursively render child with increased depth
            let child_lines = self.render_child_step(child, detail_depth, child_is_last);
            lines.extend(child_lines);
        }

        lines
    }

    /// Render a nested child step
    fn render_child_step(&self, step: &'a AgentStep, parent_depth: u8, is_last: bool) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let (indicator, status_color) = step.status.indicator(step.frame);

        // Main child line
        let mut spans = Vec::new();

        // Indentation for parent depth
        for _ in 0..parent_depth {
            let color = step.parent_verb.map(|v| v.color()).unwrap_or(COLOR_MUTED);
            spans.push(Span::styled("│ ", Style::default().fg(color)));
        }

        // Tree branch
        let tree = if is_last { "└─" } else { "├─" };
        spans.push(Span::styled(format!("{} ", tree), Style::default().fg(COLOR_MUTED)));

        // Verb pill
        if let Some(verb) = step.verb {
            spans.extend(Self::render_verb_pill(verb));
        }

        // Status indicator
        let status_char = match step.status {
            StepStatus::Running => format!("{}", indicator),
            StepStatus::Completed => "✓".to_string(),
            StepStatus::Failed => "✗".to_string(),
            _ => "○".to_string(),
        };
        spans.push(Span::styled(
            format!("{} ", status_char),
            Style::default().fg(status_color),
        ));

        // Description
        spans.push(Span::styled(
            step.description.clone(),
            Style::default().fg(COLOR_CONTENT),
        ));

        // Duration
        if let Some(duration) = step.duration {
            if duration.as_millis() > 100 {
                spans.push(Span::styled(
                    format!(" ⏱ {:.1}s", duration.as_secs_f64()),
                    Style::default().fg(COLOR_MUTED),
                ));
            }
        }

        lines.push(Line::from(spans));

        // Child details (tool call, tokens, etc.) - simplified for nested
        if let Some(ref tool) = step.tool_call {
            let content = vec![
                Span::styled("→ ", Style::default().fg(COLOR_DIMMED)),
                Span::styled(&tool.tool_name, Style::default().fg(COLOR_MUTED)),
            ];
            lines.push(Self::render_detail_line(
                parent_depth + 1,
                step.verb,
                true,
                content,
            ));
        }

        lines
    }

    /// Render to lines (for embedding in chat) - Option C: Full Detail Version
    pub fn to_lines(&self) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        // Prompt line with optional verb pill and turn info
        if self.show_prompt && !self.group.prompt.is_empty() {
            let mut prompt_spans = Vec::new();

            // Add verb pill if group has a verb
            if let Some(verb) = self.group.verb {
                prompt_spans.extend(Self::render_verb_pill(verb));
            }

            prompt_spans.push(Span::styled("> ", Style::default().fg(COLOR_MUTED)));
            prompt_spans.push(Span::styled(
                self.group.prompt.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));

            lines.push(Line::from(prompt_spans));
        }

        // Steps with verb-colored rendering and Option C details
        for (i, step) in self.group.steps.iter().enumerate() {
            if step.collapsed {
                // Collapsed step - show summary only
                lines.extend(self.render_collapsed_step(step, i));
            } else {
                // Expanded step - show full details
                lines.extend(self.render_expanded_step(step, i));
            }
        }

        // Summary line with total tokens/cost/tools (Option C)
        if (self.group.status == StepStatus::Completed || self.group.status == StepStatus::Failed)
            && !self.group.steps.is_empty()
        {
            let tokens = self.group.total_tokens();
            if tokens.total() > 0 || self.group.total_tool_calls() > 0 {
                lines.push(Line::from(vec![])); // Blank line

                let mut summary_spans = vec![Span::styled("📊 ", Style::default().fg(COLOR_DIMMED))];

                // Total tokens
                summary_spans.push(Span::styled(
                    format_number(tokens.total()),
                    Style::default().fg(COLOR_TOKENS),
                ));
                summary_spans.push(Span::styled(" tokens", Style::default().fg(COLOR_DIMMED)));

                // Cost
                let cost = tokens.estimated_cost();
                if cost > 0.0 {
                    summary_spans.push(Span::styled(" | ", Style::default().fg(COLOR_MUTED)));
                    summary_spans.push(Span::styled(
                        format!("${:.4}", cost),
                        Style::default().fg(COLOR_COST),
                    ));
                }

                // Tool calls
                let tool_calls = self.group.total_tool_calls();
                if tool_calls > 0 {
                    summary_spans.push(Span::styled(" | ", Style::default().fg(COLOR_MUTED)));
                    summary_spans.push(Span::styled(
                        format!("{} tools", tool_calls),
                        Style::default().fg(COLOR_DIMMED),
                    ));
                }

                lines.push(Line::from(summary_spans));
            }

            // Final status
            if self.group.status == StepStatus::Completed {
                let color = self.group.verb.map(|v| v.color()).unwrap_or(COLOR_SUCCESS);
                lines.push(Line::from(vec![Span::styled(
                    "✓ Done!",
                    Style::default().fg(color),
                )]));
            } else if self.group.status == StepStatus::Failed {
                lines.push(Line::from(vec![Span::styled(
                    "✗ Failed",
                    Style::default().fg(COLOR_ERROR),
                )]));
            }
        }

        lines
    }
}

impl Widget for AgentStepsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = self.to_lines();

        for (i, line) in lines.iter().enumerate() {
            if i >= area.height as usize {
                break;
            }
            let y = area.y + i as u16;
            buf.set_line(area.x, y, line, area.width);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════════
    // VERB TYPE TESTS (VerbType is defined in dag.rs, re-exported here)
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_verb_type_colors() {
        // Verify all verbs have distinct non-gray colors
        let colors: Vec<Color> = vec![
            VerbType::Infer.color(),
            VerbType::Exec.color(),
            VerbType::Fetch.color(),
            VerbType::Invoke.color(),
            VerbType::Agent.color(),
        ];

        // All colors should be RGB
        for color in &colors {
            assert!(matches!(color, Color::Rgb(_, _, _)));
        }

        // All colors should be unique
        for (i, c1) in colors.iter().enumerate() {
            for (j, c2) in colors.iter().enumerate() {
                if i != j {
                    assert_ne!(c1, c2, "Colors should be unique");
                }
            }
        }
    }

    #[test]
    fn test_verb_type_labels() {
        assert_eq!(VerbType::Infer.label(), "infer");
        assert_eq!(VerbType::Exec.label(), "exec");
        assert_eq!(VerbType::Fetch.label(), "fetch");
        assert_eq!(VerbType::Invoke.label(), "invoke");
        assert_eq!(VerbType::Agent.label(), "agent");
        assert_eq!(VerbType::Unknown.label(), "unknown");
    }

    #[test]
    fn test_verb_type_parse() {
        assert_eq!(VerbType::parse("infer"), VerbType::Infer);
        assert_eq!(VerbType::parse("EXEC"), VerbType::Exec);
        assert_eq!(VerbType::parse("Fetch"), VerbType::Fetch);
        assert_eq!(VerbType::parse("invoke"), VerbType::Invoke);
        assert_eq!(VerbType::parse("agent"), VerbType::Agent);
        assert_eq!(VerbType::parse("unknown_verb"), VerbType::Unknown);
    }

    #[test]
    fn test_verb_type_try_parse() {
        assert_eq!(VerbType::try_parse("infer"), Some(VerbType::Infer));
        assert_eq!(VerbType::try_parse("EXEC"), Some(VerbType::Exec));
        assert_eq!(VerbType::try_parse("Fetch"), Some(VerbType::Fetch));
        assert_eq!(VerbType::try_parse("invoke"), Some(VerbType::Invoke));
        assert_eq!(VerbType::try_parse("agent"), Some(VerbType::Agent));
        assert_eq!(VerbType::try_parse("xyz"), None);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // STEP STATUS TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_step_status_indicators() {
        assert_eq!(StepStatus::Pending.indicator(0).0, '○');
        assert_eq!(StepStatus::Completed.indicator(0).0, '✓');
        assert_eq!(StepStatus::Failed.indicator(0).0, '✗');

        // Running should animate
        let (c1, _) = StepStatus::Running.indicator(0);
        let (c2, _) = StepStatus::Running.indicator(1);
        assert!(SPINNER_FRAMES.contains(&c1));
        assert!(SPINNER_FRAMES.contains(&c2));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // AGENT STEP TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_agent_step_lifecycle() {
        let mut step = AgentStep::new("Writing file.py");
        assert_eq!(step.status, StepStatus::Pending);

        step.start();
        assert_eq!(step.status, StepStatus::Running);

        step.complete();
        assert_eq!(step.status, StepStatus::Completed);
        assert!(step.duration.is_some());
    }

    #[test]
    fn test_agent_step_fail() {
        let mut step = AgentStep::running("Compiling...");
        step.fail();
        assert_eq!(step.status, StepStatus::Failed);
    }

    #[test]
    fn test_agent_step_for_verb() {
        let step = AgentStep::for_verb(VerbType::Infer, "Generating content");
        assert_eq!(step.verb, Some(VerbType::Infer));
        assert_eq!(step.status, StepStatus::Running);
        assert_eq!(step.depth, 0);
    }

    #[test]
    fn test_agent_step_nested() {
        let step = AgentStep::nested(VerbType::Agent, VerbType::Invoke, "Calling MCP tool");
        assert_eq!(step.verb, Some(VerbType::Invoke));
        assert_eq!(step.parent_verb, Some(VerbType::Agent));
        assert_eq!(step.depth, 1);
    }

    #[test]
    fn test_running_step_with_detail() {
        let step = AgentStep::running("Calling MCP tool")
            .with_detail("novanet_describe")
            .with_verb(VerbType::Invoke);

        assert_eq!(step.detail, Some("novanet_describe".to_string()));
        assert_eq!(step.verb, Some(VerbType::Invoke));
    }

    #[test]
    fn test_step_tick() {
        let mut step = AgentStep::running("Test");
        let frame1 = step.frame;
        step.tick();
        let frame2 = step.frame;
        assert_eq!(frame2, frame1.wrapping_add(1));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // AGENT STEP GROUP TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_agent_step_group() {
        let mut group = AgentStepGroup::new("Design event schemas");
        group.start();

        group.add_running("Writing order-to-payment.py");
        assert!(group.is_running());

        group.complete_current();
        group.add_running("Optimizing imports");
        group.complete_current();

        group.complete();
        assert_eq!(group.status, StepStatus::Completed);
        assert_eq!(group.steps.len(), 2);
    }

    #[test]
    fn test_agent_step_group_for_verb() {
        let group = AgentStepGroup::for_verb(VerbType::Agent, "Multi-turn task");
        assert_eq!(group.verb, Some(VerbType::Agent));
        assert_eq!(group.prompt, "Multi-turn task");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // WIDGET RENDERING TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_steps_widget_lines() {
        let mut group = AgentStepGroup::new("Test prompt");
        group.add_step(AgentStep::completed("Step 1"));
        group.add_step(AgentStep::completed("Step 2"));
        group.status = StepStatus::Completed;

        let widget = AgentStepsWidget::new(&group);
        let lines = widget.to_lines();

        assert!(!lines.is_empty());
        // Should have prompt + 2 steps + Done
        assert!(lines.len() >= 4);
    }

    #[test]
    fn test_steps_widget_with_verb() {
        let mut group = AgentStepGroup::for_verb(VerbType::Infer, "Generate content");
        group.add_step(AgentStep::for_verb(VerbType::Infer, "Sending to LLM"));
        group.status = StepStatus::Running;

        let widget = AgentStepsWidget::new(&group);
        let lines = widget.to_lines();

        // Lines should contain verb pill spans
        assert!(!lines.is_empty());
        // First line (prompt) should have verb pill
        let first_line = &lines[0];
        assert!(!first_line.spans.is_empty());
    }

    #[test]
    fn test_steps_widget_nested_verbs() {
        let mut group = AgentStepGroup::for_verb(VerbType::Agent, "Orchestrate task");
        group.add_step(AgentStep::for_verb(VerbType::Agent, "Planning"));
        group.add_step(AgentStep::nested(
            VerbType::Agent,
            VerbType::Invoke,
            "Calling novanet_describe",
        ));
        group.add_step(AgentStep::nested(
            VerbType::Agent,
            VerbType::Infer,
            "Generating response",
        ));

        let widget = AgentStepsWidget::new(&group);
        let lines = widget.to_lines();

        // Should have prompt + 3 steps
        assert!(lines.len() >= 4);

        // Nested steps should have depth indicator
        let nested_step = &group.steps[1];
        assert_eq!(nested_step.depth, 1);
        assert_eq!(nested_step.parent_verb, Some(VerbType::Agent));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: TOKEN USAGE TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_token_usage_new() {
        let tokens = TokenUsage::new(1000, 500);
        assert_eq!(tokens.input_tokens, 1000);
        assert_eq!(tokens.output_tokens, 500);
        assert_eq!(tokens.cache_read_tokens, None);
        assert_eq!(tokens.cache_write_tokens, None);
    }

    #[test]
    fn test_token_usage_total() {
        let tokens = TokenUsage::new(1000, 500);
        assert_eq!(tokens.total(), 1500);
    }

    #[test]
    fn test_token_usage_estimated_cost() {
        // $3/M input, $15/M output
        let tokens = TokenUsage::new(1_000_000, 100_000);
        let cost = tokens.estimated_cost();
        // 1M input = $3, 100K output = $1.5
        assert!((cost - 4.5).abs() < 0.001);
    }

    #[test]
    fn test_token_usage_format_compact() {
        let tokens = TokenUsage::new(1234, 567);
        assert_eq!(tokens.format_compact(), "1,234 in → 567 out");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: TOOL CALL METADATA TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_tool_call_metadata_new() {
        let meta = ToolCallMetadata::new("novanet_describe");
        assert_eq!(meta.tool_name, "novanet_describe");
        assert_eq!(meta.server_name, None);
    }

    #[test]
    fn test_tool_call_metadata_builder() {
        let meta = ToolCallMetadata::new("novanet_generate")
            .with_server("novanet")
            .with_params(r#"{"entity": "qr-code"}"#)
            .with_result(r#"{"name": "QR Code"}"#)
            .with_duration(Duration::from_millis(150));

        assert_eq!(meta.tool_name, "novanet_generate");
        assert_eq!(meta.server_name, Some("novanet".to_string()));
        assert!(meta.params_preview.is_some());
        assert!(meta.result_preview.is_some());
        assert_eq!(meta.result_size, Some(19));
        assert_eq!(meta.call_duration, Some(Duration::from_millis(150)));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: STREAMING PROGRESS TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_streaming_progress_new() {
        let progress = StreamingProgress::new();
        assert!(progress.is_streaming);
        assert_eq!(progress.tokens_generated, 0);
        assert_eq!(progress.tokens_expected, None);
    }

    #[test]
    fn test_streaming_progress_percent() {
        let mut progress = StreamingProgress::new();
        progress.tokens_generated = 80;
        progress.tokens_expected = Some(100);
        assert_eq!(progress.progress_percent(), Some(80));
    }

    #[test]
    fn test_streaming_progress_percent_none() {
        let progress = StreamingProgress::new();
        assert_eq!(progress.progress_percent(), None);
    }

    #[test]
    fn test_streaming_progress_bar() {
        let mut progress = StreamingProgress::new();
        progress.tokens_generated = 50;
        progress.tokens_expected = Some(100);
        let bar = progress.render_progress_bar(10);
        assert!(bar.contains("50%"));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: ERROR DETAILS TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_error_details_new() {
        let error = ErrorDetails::new("Connection failed");
        assert_eq!(error.message, "Connection failed");
        assert_eq!(error.code, None);
        assert!(!error.recoverable);
    }

    #[test]
    fn test_error_details_builder() {
        let error = ErrorDetails::new("API rate limited")
            .with_code("NIKA-102")
            .with_suggestion("Wait 60 seconds and retry");

        assert_eq!(error.code, Some("NIKA-102".to_string()));
        assert_eq!(
            error.suggestion,
            Some("Wait 60 seconds and retry".to_string())
        );
    }

    #[test]
    fn test_error_details_format_short() {
        let error = ErrorDetails::new("Failed").with_code("NIKA-100");
        assert_eq!(error.format_short(), "[NIKA-100] Failed");

        let error_no_code = ErrorDetails::new("Failed");
        assert_eq!(error_no_code.format_short(), "Failed");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: MODEL INFO TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_model_info_new() {
        let info = ModelInfo::new("claude-sonnet-4-6");
        assert_eq!(info.model, "claude-sonnet-4-6");
        assert_eq!(info.provider, None);
    }

    #[test]
    fn test_model_info_builder() {
        let info = ModelInfo::new("gpt-4o").with_provider("openai");
        assert_eq!(info.model, "gpt-4o");
        assert_eq!(info.provider, Some("openai".to_string()));
    }

    #[test]
    fn test_model_info_format_display() {
        let info = ModelInfo::new("claude-sonnet-4-6").with_provider("claude");
        assert_eq!(info.format_display(), "claude-sonnet-4-6 (claude)");

        let info_no_provider = ModelInfo::new("llama3.2");
        assert_eq!(info_no_provider.format_display(), "llama3.2");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: AGENT STEP ENHANCED BUILDER TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_agent_step_with_tokens() {
        let step = AgentStep::for_verb(VerbType::Infer, "Generating")
            .with_tokens(TokenUsage::new(500, 200));

        assert!(step.tokens.is_some());
        assert_eq!(step.tokens.unwrap().total(), 700);
    }

    #[test]
    fn test_agent_step_with_tool_call() {
        let step = AgentStep::for_verb(VerbType::Invoke, "Calling tool")
            .with_tool_call(ToolCallMetadata::new("novanet_describe").with_server("novanet"));

        assert!(step.tool_call.is_some());
        let tool = step.tool_call.unwrap();
        assert_eq!(tool.tool_name, "novanet_describe");
        assert_eq!(tool.server_name, Some("novanet".to_string()));
    }

    #[test]
    fn test_agent_step_with_model() {
        let step = AgentStep::for_verb(VerbType::Infer, "Generating")
            .with_model(ModelInfo::new("claude-sonnet-4-6").with_provider("claude"));

        assert!(step.model.is_some());
        assert_eq!(
            step.model.unwrap().format_display(),
            "claude-sonnet-4-6 (claude)"
        );
    }

    #[test]
    fn test_agent_step_with_error() {
        let step = AgentStep::for_verb(VerbType::Fetch, "Fetching")
            .with_error(ErrorDetails::new("Timeout").with_code("NIKA-050"));

        assert!(step.error.is_some());
        assert_eq!(step.status, StepStatus::Failed);
        assert_eq!(step.error.unwrap().format_short(), "[NIKA-050] Timeout");
    }

    #[test]
    fn test_agent_step_with_turn() {
        let step = AgentStep::for_verb(VerbType::Agent, "Agent turn").with_turn(3, Some(10));

        assert_eq!(step.turn_number, Some(3));
        assert_eq!(step.max_turns, Some(10));
    }

    #[test]
    fn test_agent_step_with_command() {
        let step = AgentStep::for_verb(VerbType::Exec, "Running build")
            .with_command("npm run build")
            .with_exit_code(0);

        assert_eq!(step.command, Some("npm run build".to_string()));
        assert_eq!(step.exit_code, Some(0));
    }

    #[test]
    fn test_agent_step_with_url() {
        let step = AgentStep::for_verb(VerbType::Fetch, "Fetching API")
            .with_url("https://api.example.com/data");

        assert_eq!(step.url, Some("https://api.example.com/data".to_string()));
    }

    #[test]
    fn test_agent_step_with_task_id() {
        let step = AgentStep::for_verb(VerbType::Infer, "Processing").with_task_id("task-123");

        assert_eq!(step.task_id, Some("task-123".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: NESTED CHILDREN AND AGGREGATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_agent_step_add_child() {
        let mut parent = AgentStep::for_verb(VerbType::Agent, "Orchestrating");
        assert!(!parent.has_children());

        parent.add_child(AgentStep::for_verb(VerbType::Invoke, "Child invoke"));
        assert!(parent.has_children());
        assert_eq!(parent.children.len(), 1);
    }

    #[test]
    fn test_agent_step_total_tokens_with_children() {
        let mut parent = AgentStep::for_verb(VerbType::Agent, "Orchestrating")
            .with_tokens(TokenUsage::new(100, 50));

        let child1 =
            AgentStep::for_verb(VerbType::Infer, "Child 1").with_tokens(TokenUsage::new(200, 100));
        let child2 =
            AgentStep::for_verb(VerbType::Infer, "Child 2").with_tokens(TokenUsage::new(300, 150));

        parent.add_child(child1);
        parent.add_child(child2);

        let total = parent.total_tokens();
        assert_eq!(total.input_tokens, 600); // 100 + 200 + 300
        assert_eq!(total.output_tokens, 300); // 50 + 100 + 150
        assert_eq!(total.total(), 900);
    }

    #[test]
    fn test_agent_step_total_tool_calls() {
        let mut parent = AgentStep::for_verb(VerbType::Agent, "Orchestrating");

        let child1 = AgentStep::for_verb(VerbType::Invoke, "Child 1")
            .with_tool_call(ToolCallMetadata::new("tool_a"));
        let child2 = AgentStep::for_verb(VerbType::Invoke, "Child 2")
            .with_tool_call(ToolCallMetadata::new("tool_b"));
        let child3 = AgentStep::for_verb(VerbType::Infer, "Child 3"); // no tool call

        parent.add_child(child1);
        parent.add_child(child2);
        parent.add_child(child3);

        assert_eq!(parent.total_tool_calls(), 2);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // OPTION C: AGENT STEP GROUP AGGREGATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_agent_step_group_total_tokens() {
        let mut group = AgentStepGroup::new("Test prompt");

        group.add_step(
            AgentStep::for_verb(VerbType::Infer, "Step 1").with_tokens(TokenUsage::new(500, 200)),
        );
        group.add_step(
            AgentStep::for_verb(VerbType::Infer, "Step 2").with_tokens(TokenUsage::new(300, 150)),
        );

        let total = group.total_tokens();
        assert_eq!(total.input_tokens, 800);
        assert_eq!(total.output_tokens, 350);
        assert_eq!(total.total(), 1150);
    }

    #[test]
    fn test_agent_step_group_total_cost() {
        let mut group = AgentStepGroup::new("Test prompt");

        // 1M input = $3, 500K output = $7.5
        group.add_step(
            AgentStep::for_verb(VerbType::Infer, "Step 1")
                .with_tokens(TokenUsage::new(1_000_000, 500_000)),
        );

        let cost = group.total_cost();
        assert!((cost - 10.5).abs() < 0.001);
    }

    #[test]
    fn test_agent_step_group_total_tool_calls() {
        let mut group = AgentStepGroup::new("Test prompt");

        group.add_step(
            AgentStep::for_verb(VerbType::Invoke, "Tool 1")
                .with_tool_call(ToolCallMetadata::new("tool_a")),
        );
        group.add_step(
            AgentStep::for_verb(VerbType::Invoke, "Tool 2")
                .with_tool_call(ToolCallMetadata::new("tool_b")),
        );
        group.add_step(AgentStep::for_verb(VerbType::Infer, "No tool"));

        assert_eq!(group.total_tool_calls(), 2);
    }

    #[test]
    fn test_agent_step_group_completed_failed_counts() {
        let mut group = AgentStepGroup::new("Test prompt");

        let mut step1 = AgentStep::for_verb(VerbType::Infer, "Step 1");
        step1.complete();

        let mut step2 = AgentStep::for_verb(VerbType::Exec, "Step 2");
        step2.complete();

        let step3 =
            AgentStep::for_verb(VerbType::Fetch, "Step 3").with_error(ErrorDetails::new("Failed"));

        group.add_step(step1);
        group.add_step(step2);
        group.add_step(step3);

        assert_eq!(group.completed_steps(), 2);
        assert_eq!(group.failed_steps(), 1);
    }

    #[test]
    fn test_agent_step_group_format_summary() {
        let mut group = AgentStepGroup::new("Test prompt");

        group.add_step(
            AgentStep::for_verb(VerbType::Infer, "Step 1").with_tokens(TokenUsage::new(1000, 500)),
        );
        group.add_step(
            AgentStep::for_verb(VerbType::Invoke, "Step 2")
                .with_tool_call(ToolCallMetadata::new("tool_a")),
        );

        let summary = group.format_summary();
        assert!(summary.contains("1,500 tokens"));
        assert!(summary.contains("1 tools called"));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // HELPER FUNCTION TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_truncate_json_short() {
        let short = r#"{"a": 1}"#;
        assert_eq!(truncate_json(short, 50), short);
    }

    #[test]
    fn test_truncate_json_long() {
        let long = r#"{"name": "QR Code", "description": "A very long description"}"#;
        let truncated = truncate_json(long, 40);
        assert!(truncated.len() < long.len());
        assert!(truncated.ends_with("...") || truncated.ends_with("...}"));
    }
}
