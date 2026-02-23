//! Agent Steps Widget - Real-time feedback for agent actions
//!
//! Provides Claude Code / Vibe-like step-by-step feedback with verb-specific colors:
//!
//! ```text
//! > Design event schemas for order-to-payment
//! [infer] ● Generating content...
//! [exec]  └ Running npm build
//! [invoke]└ Calling novanet_describe
//! ✓ Done!
//! ```
//!
//! Each verb has a distinctive color (PLUQQY-style tag pills):
//! - infer  → Violet (#6366f1) - LLM generation
//! - exec   → Green  (#22c55e) - Shell commands
//! - fetch  → Cyan   (#06b6d4) - HTTP requests
//! - invoke → Blue   (#3b82f6) - MCP tool calls
//! - agent  → Gold   (#eab308) - Agentic loops
//!
//! Nested verbs show parent color border for visual hierarchy.

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

// ═══════════════════════════════════════════════════════════════════════════════
// VERB COLORS (Tailwind-inspired, Solarized-compatible)
// ═══════════════════════════════════════════════════════════════════════════════

/// Verb type for color-coded feedback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerbType {
    #[default]
    Infer,  // LLM text generation
    Exec,   // Shell command
    Fetch,  // HTTP request
    Invoke, // MCP tool call
    Agent,  // Agentic loop (parent)
}

impl VerbType {
    /// Get the color for this verb type
    pub fn color(&self) -> Color {
        match self {
            VerbType::Infer => Color::Rgb(99, 102, 241),   // Violet - #6366f1
            VerbType::Exec => Color::Rgb(34, 197, 94),     // Green  - #22c55e
            VerbType::Fetch => Color::Rgb(6, 182, 212),    // Cyan   - #06b6d4
            VerbType::Invoke => Color::Rgb(59, 130, 246),  // Blue   - #3b82f6
            VerbType::Agent => Color::Rgb(234, 179, 8),    // Gold   - #eab308
        }
    }

    /// Get the icon for this verb type
    pub fn icon(&self) -> &'static str {
        match self {
            VerbType::Infer => "⚡",
            VerbType::Exec => "📟",
            VerbType::Fetch => "🛰",
            VerbType::Invoke => "🔌",
            VerbType::Agent => "🐔",
        }
    }

    /// Get the label for this verb type
    pub fn label(&self) -> &'static str {
        match self {
            VerbType::Infer => "infer",
            VerbType::Exec => "exec",
            VerbType::Fetch => "fetch",
            VerbType::Invoke => "invoke",
            VerbType::Agent => "agent",
        }
    }

    /// Parse verb type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "infer" => Some(VerbType::Infer),
            "exec" => Some(VerbType::Exec),
            "fetch" => Some(VerbType::Fetch),
            "invoke" => Some(VerbType::Invoke),
            "agent" => Some(VerbType::Agent),
            _ => None,
        }
    }
}

// Status colors (unchanged)
const COLOR_RUNNING: Color = Color::Rgb(250, 204, 21); // Yellow/Gold - in progress
const COLOR_SUCCESS: Color = Color::Rgb(34, 197, 94); // Green - completed
const COLOR_ERROR: Color = Color::Rgb(239, 68, 68);   // Red - failed
const COLOR_MUTED: Color = Color::Rgb(88, 110, 117);  // Gray - tree lines
const COLOR_CONTENT: Color = Color::Rgb(147, 161, 161); // Light gray - step text

/// Spinner frames for running steps
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

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

/// A single step in an agent action
#[derive(Debug, Clone)]
pub struct AgentStep {
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

    /// Render to lines (for embedding in chat)
    pub fn to_lines(&self) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        // Prompt line with optional verb pill
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

        // Steps with verb-colored rendering
        for (i, step) in self.group.steps.iter().enumerate() {
            let is_last = i == self.group.steps.len() - 1;
            let (indicator, status_color) = step.status.indicator(step.frame);

            let mut spans = Vec::new();

            // Add indentation for nested steps
            spans.extend(Self::render_indent(step.depth, step.parent_verb));

            // Add verb pill if step has a verb
            if let Some(verb) = step.verb {
                spans.extend(Self::render_verb_pill(verb));
            }

            // Tree/status indicator
            match step.status {
                StepStatus::Running => {
                    // Spinner with verb color or default
                    let color = step.verb.map(|v| v.color()).unwrap_or(status_color);
                    spans.push(Span::styled(
                        format!("{} ", indicator),
                        Style::default().fg(color),
                    ));
                }
                StepStatus::Completed => {
                    // Checkmark with success color
                    let tree = if is_last { "└" } else { "├" };
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
                        format!("{} ", tree),
                        Style::default().fg(COLOR_MUTED),
                    ));
                }
            }

            // Description (with verb-tinted color for running steps)
            let desc_color = if step.status == StepStatus::Running {
                step.verb.map(|v| v.color()).unwrap_or(COLOR_CONTENT)
            } else {
                COLOR_CONTENT
            };
            spans.push(Span::styled(
                step.description.clone(),
                Style::default().fg(desc_color),
            ));

            // Detail (e.g., tool name, file path)
            if let Some(ref detail) = step.detail {
                spans.push(Span::styled(
                    format!(" ({})", detail),
                    Style::default().fg(COLOR_MUTED),
                ));
            }

            // Duration for completed steps
            if step.status == StepStatus::Completed {
                if let Some(duration) = step.duration {
                    if duration.as_millis() > 100 {
                        spans.push(Span::styled(
                            format!(" {:.1}s", duration.as_secs_f64()),
                            Style::default().fg(COLOR_MUTED),
                        ));
                    }
                }
            }

            lines.push(Line::from(spans));
        }

        // Final status line with optional verb color
        if self.group.status == StepStatus::Completed && !self.group.steps.is_empty() {
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
    // VERB TYPE TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_verb_type_colors() {
        // Verify all verbs have distinct colors
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
    }

    #[test]
    fn test_verb_type_icons() {
        assert_eq!(VerbType::Infer.icon(), "⚡");
        assert_eq!(VerbType::Exec.icon(), "📟");
        assert_eq!(VerbType::Fetch.icon(), "🛰");
        assert_eq!(VerbType::Invoke.icon(), "🔌");
        assert_eq!(VerbType::Agent.icon(), "🐔");
    }

    #[test]
    fn test_verb_type_from_str() {
        assert_eq!(VerbType::from_str("infer"), Some(VerbType::Infer));
        assert_eq!(VerbType::from_str("EXEC"), Some(VerbType::Exec));
        assert_eq!(VerbType::from_str("Fetch"), Some(VerbType::Fetch));
        assert_eq!(VerbType::from_str("invoke"), Some(VerbType::Invoke));
        assert_eq!(VerbType::from_str("agent"), Some(VerbType::Agent));
        assert_eq!(VerbType::from_str("unknown"), None);
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
}
