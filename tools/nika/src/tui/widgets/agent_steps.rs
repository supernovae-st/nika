//! Agent Steps — Data types only
//!
//! Widget rendering code (`AgentStepsWidget`) and unused step types removed.
//! Only `AgentPhase` and `AgentPhaseIndicator` are kept (used by chat view).

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

// Re-export VerbType from verb_type module (single source of truth)
pub use super::verb_type::VerbType;

use crate::tui::theme::Theme;

// ═══════════════════════════════════════════════════════════════════════════════
// AGENT PHASE
// ═══════════════════════════════════════════════════════════════════════════════

/// Agent execution phase for real-time status display
///
/// Shows what the agent is actually doing instead of generic "Generating..."
///
/// # Currently Used Phases
///
/// | Phase | Event Trigger | Description |
/// |-------|---------------|-------------|
/// | `Idle` | AgentComplete | No agent running |
/// | `Syncing` | AgentTurn{kind:"started"} | Agent connecting/starting |
/// | `Planning` | AgentTurn{kind:other} | Agent thinking about action |
/// | `Invoking` | McpInvoke | Calling MCP tool |
/// | `Processing` | McpResponse | Handling tool result |
/// | `Inferring` | ProviderCalled | LLM text generation |
/// | `Streaming` | First token | Outputting tokens |
///
/// # Reserved Phases (Not Currently Triggered)
///
/// | Phase | Intended Use |
/// |-------|--------------|
/// | `Routing` | Future: Selecting which tool to call |
/// | `Composing` | Future: Formulating final response |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentPhase {
    /// No agent running (default)
    #[default]
    Idle,
    /// Connecting to API / starting up
    Syncing,
    /// Analyzing context + deciding action
    Planning,
    /// Selecting which tool to call (reserved - not currently triggered)
    Routing,
    /// Calling MCP tool
    Invoking,
    /// Handling tool result
    Processing,
    /// LLM text generation
    Inferring,
    /// Formulating response (reserved - not currently triggered)
    Composing,
    /// Outputting tokens
    Streaming,
}

impl AgentPhase {
    /// Get the icon for this phase (Nika vocabulary)
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Syncing => "🦋",
            Self::Planning => "🐔",
            Self::Routing => "🔀",
            Self::Invoking => "🔌",
            Self::Processing => "⚙️",
            Self::Inferring => "⚡",
            Self::Composing => "✍️",
            Self::Streaming => "📡",
        }
    }

    /// Get the label for this phase
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Syncing => "Syncing",
            Self::Planning => "Planning",
            Self::Routing => "Routing",
            Self::Invoking => "Invoking",
            Self::Processing => "Processing",
            Self::Inferring => "Inferring",
            Self::Composing => "Composing",
            Self::Streaming => "Streaming",
        }
    }

    /// For Matrix effect: alternate between butterfly (Nika brand) and phase icon
    /// Switches every 5 frames (~500ms at 10fps)
    pub fn animated_icon(&self, frame: u8) -> &'static str {
        if *self == Self::Idle {
            return self.icon();
        }
        if frame % 10 < 5 {
            "🦋" // Nika brand icon
        } else {
            self.icon()
        }
    }

    /// Check if this phase is active (not idle)
    pub fn is_active(&self) -> bool {
        *self != Self::Idle
    }

    /// Check if this is a tool-related phase
    pub fn is_tool_phase(&self) -> bool {
        matches!(self, Self::Invoking | Self::Processing)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AGENT PHASE INDICATOR
// ═══════════════════════════════════════════════════════════════════════════════

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Katakana characters for Matrix chaos effect
const KATAKANA_CHARS: &[char] = &[
    'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ',
    'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'マ', 'ミ',
    'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ', 'ヲ', 'ン',
];

/// Compact phase indicator with Matrix effect on label
///
/// Features:
/// - Animated icon alternating between butterfly and phase icon
/// - Matrix chaos effect on label text
/// - Tool name display when invoking
/// - Animated dots "..."
#[derive(Debug, Clone)]
pub struct AgentPhaseIndicator {
    /// Current phase
    phase: AgentPhase,
    /// Tool name (for Invoking phase)
    tool_name: Option<String>,
    /// Animation frame counter
    frame: u8,
    /// RNG seed for consistent chaos per-indicator
    seed: u64,
    /// How many chars are revealed (for progressive reveal)
    revealed_chars: usize,
}

impl Default for AgentPhaseIndicator {
    fn default() -> Self {
        Self::new(AgentPhase::Idle)
    }
}

impl AgentPhaseIndicator {
    /// Create a new phase indicator
    pub fn new(phase: AgentPhase) -> Self {
        Self {
            phase,
            tool_name: None,
            frame: 0,
            seed: 42,
            revealed_chars: 0,
        }
    }

    /// Set the tool name (for Invoking phase)
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool_name = Some(tool.into());
        self
    }

    /// Set a custom seed for chaos RNG
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Update the phase
    pub fn set_phase(&mut self, phase: AgentPhase) {
        if self.phase != phase {
            self.phase = phase;
            self.revealed_chars = 0; // Reset reveal on phase change
        }
    }

    /// Set tool name
    pub fn set_tool(&mut self, tool: Option<String>) {
        self.tool_name = tool;
    }

    /// Advance animation frame
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        // Reveal one more char every 2 frames
        if self.frame % 2 == 0 {
            self.revealed_chars = self.revealed_chars.saturating_add(1);
        }
    }

    /// Get current frame
    pub fn frame(&self) -> u8 {
        self.frame
    }

    /// Get current phase
    pub fn phase(&self) -> AgentPhase {
        self.phase
    }

    /// Build a Line with Matrix effect on the label
    ///
    /// Output: `butterfly Syncingchars...` or `plug Invoking novanet_describe...`
    pub fn build_line(&self) -> Line<'static> {
        use ratatui::style::Modifier;

        if self.phase == AgentPhase::Idle {
            return Line::from(vec![]);
        }

        let mut spans = vec![];
        let mut rng = SmallRng::seed_from_u64(self.seed.wrapping_add(self.frame as u64));

        // Animated icon (butterfly <-> phase_icon)
        let icon = self.phase.animated_icon(self.frame);
        spans.push(Span::styled(
            format!("{} ", icon),
            Style::default().fg(Color::Rgb(116, 199, 236)), // Sapphire
        ));

        // Label with Matrix chaos effect
        let label = self.phase.label();
        for (i, ch) in label.chars().enumerate() {
            if i < self.revealed_chars {
                // Revealed: show actual character
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(Color::Rgb(186, 194, 222)), // Subtext1
                ));
            } else {
                // Chaos: show random katakana with color
                let chaos_char = KATAKANA_CHARS[rng.gen_range(0..KATAKANA_CHARS.len())];
                let colors = [
                    Color::Rgb(116, 199, 236), // Sapphire
                    Color::Rgb(166, 227, 161), // Green
                    Color::Rgb(249, 226, 175), // Yellow
                    Color::Rgb(137, 180, 250), // Blue
                    Color::Rgb(203, 166, 247), // Mauve
                ];
                let color = colors[rng.gen_range(0..colors.len())];
                spans.push(Span::styled(
                    chaos_char.to_string(),
                    Style::default().fg(color),
                ));
            }
        }

        // Tool name (if invoking)
        if let Some(tool) = &self.tool_name {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                tool.clone(),
                Style::default()
                    .fg(Color::Rgb(137, 180, 250)) // Blue
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // Animated dots
        let dot_count = ((self.frame / 5) % 4) as usize;
        let dots = ".".repeat(dot_count.max(1));
        spans.push(Span::styled(
            dots,
            Style::default().fg(Color::Rgb(108, 112, 134)),
        )); // Overlay0

        Line::from(spans)
    }

    /// Check if fully revealed (no more chaos)
    pub fn is_fully_revealed(&self) -> bool {
        self.revealed_chars >= self.phase.label().len()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// STUB TYPES (kept for backward compat re-exports, no rendering)
// ═══════════════════════════════════════════════════════════════════════════════

/// Step status (stub -- widget removed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Token usage tracking (stub -- widget removed)
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

impl TokenUsage {
    pub fn new(input: u64, output: u64) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
            cost_usd: 0.0,
        }
    }

    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Tool call metadata (stub -- widget removed)
#[derive(Debug, Clone)]
pub struct ToolCallMetadata {
    pub server: String,
    pub tool: String,
    pub params_preview: Option<String>,
    pub result_preview: Option<String>,
    pub result_bytes: Option<usize>,
}

impl ToolCallMetadata {
    pub fn new(server: impl Into<String>, tool: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            tool: tool.into(),
            params_preview: None,
            result_preview: None,
            result_bytes: None,
        }
    }
}

/// Single agent step (stub -- widget removed)
#[derive(Debug, Clone)]
pub struct AgentStep {
    pub description: String,
    pub verb: VerbType,
    pub status: StepStatus,
    pub started_at: Option<std::time::Instant>,
    pub duration: Option<std::time::Duration>,
    pub error: Option<String>,
    pub tokens: Option<TokenUsage>,
    pub tool_call: Option<ToolCallMetadata>,
    pub frame: u8,
    pub expanded: bool,
    pub children: Vec<AgentStep>,
}

impl AgentStep {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            verb: VerbType::Unknown,
            status: StepStatus::Pending,
            started_at: None,
            duration: None,
            error: None,
            tokens: None,
            tool_call: None,
            frame: 0,
            expanded: false,
            children: Vec::new(),
        }
    }

    pub fn running(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            verb: VerbType::Unknown,
            status: StepStatus::Running,
            started_at: Some(std::time::Instant::now()),
            duration: None,
            error: None,
            tokens: None,
            tool_call: None,
            frame: 0,
            expanded: true,
            children: Vec::new(),
        }
    }

    pub fn completed(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            verb: VerbType::Unknown,
            status: StepStatus::Completed,
            started_at: None,
            duration: None,
            error: None,
            tokens: None,
            tool_call: None,
            frame: 0,
            expanded: false,
            children: Vec::new(),
        }
    }

    pub fn for_verb(verb: VerbType, description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            verb,
            status: StepStatus::Running,
            started_at: Some(std::time::Instant::now()),
            duration: None,
            error: None,
            tokens: None,
            tool_call: None,
            frame: 0,
            expanded: true,
            children: Vec::new(),
        }
    }

    pub fn for_invoke(
        tool: impl Into<String>,
        server: impl Into<String>,
        params: Option<String>,
    ) -> Self {
        let tool_str: String = tool.into();
        let meta = ToolCallMetadata {
            server: server.into(),
            tool: tool_str.clone(),
            params_preview: params,
            result_preview: None,
            result_bytes: None,
        };
        Self {
            description: format!("Calling {}", tool_str),
            verb: VerbType::Invoke,
            status: StepStatus::Running,
            started_at: Some(std::time::Instant::now()),
            duration: None,
            error: None,
            tokens: None,
            tool_call: Some(meta),
            frame: 0,
            expanded: true,
            children: Vec::new(),
        }
    }

    pub fn with_tokens(mut self, tokens: TokenUsage) -> Self {
        self.tokens = Some(tokens);
        self
    }

    pub fn with_tool_call(mut self, tool_call: ToolCallMetadata) -> Self {
        self.tool_call = Some(tool_call);
        self
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = StepStatus::Failed;
        self.error = Some(error.into());
        if let Some(started) = self.started_at {
            self.duration = Some(started.elapsed());
        }
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        for child in &mut self.children {
            child.tick();
        }
    }

    pub fn add_child(&mut self, child: AgentStep) {
        self.children.push(child);
    }

    pub fn total_tokens(&self) -> TokenUsage {
        let mut total = TokenUsage::default();
        if let Some(t) = &self.tokens {
            total.input_tokens += t.input_tokens;
            total.output_tokens += t.output_tokens;
            total.cost_usd += t.cost_usd;
        }
        for child in &self.children {
            let child_tokens = child.total_tokens();
            total.input_tokens += child_tokens.input_tokens;
            total.output_tokens += child_tokens.output_tokens;
            total.cost_usd += child_tokens.cost_usd;
        }
        total
    }

    pub fn start(&mut self) {
        self.status = StepStatus::Running;
        self.started_at = Some(std::time::Instant::now());
    }

    pub fn complete(&mut self) {
        self.status = StepStatus::Completed;
        if let Some(started) = self.started_at {
            self.duration = Some(started.elapsed());
        }
    }

    pub fn mark_failed(&mut self) {
        self.status = StepStatus::Failed;
        if let Some(started) = self.started_at {
            self.duration = Some(started.elapsed());
        }
    }
}

/// Group of agent steps (stub -- widget removed)
#[derive(Debug, Clone)]
pub struct AgentStepGroup {
    pub title: String,
    pub steps: Vec<AgentStep>,
    pub status: StepStatus,
    pub collapsed: bool,
    pub max_turns: Option<usize>,
    pub current_turn: usize,
}

impl AgentStepGroup {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            steps: Vec::new(),
            status: StepStatus::Pending,
            collapsed: false,
            max_turns: None,
            current_turn: 0,
        }
    }

    pub fn with_turns(title: impl Into<String>, max_turns: usize) -> Self {
        Self {
            title: title.into(),
            steps: Vec::new(),
            status: StepStatus::Pending,
            collapsed: false,
            max_turns: Some(max_turns),
            current_turn: 0,
        }
    }

    pub fn start(&mut self) {
        self.status = StepStatus::Running;
    }

    pub fn add_step(&mut self, step: AgentStep) {
        self.steps.push(step);
    }

    pub fn add_running_step(&mut self, description: impl Into<String>) {
        self.steps.push(AgentStep::running(description));
    }

    pub fn complete_current_step(&mut self) {
        if let Some(step) = self
            .steps
            .iter_mut()
            .rev()
            .find(|s| s.status == StepStatus::Running)
        {
            step.complete();
        }
    }

    pub fn fail_current_step(&mut self, error: impl Into<String>) {
        if let Some(step) = self
            .steps
            .iter_mut()
            .rev()
            .find(|s| s.status == StepStatus::Running)
        {
            step.fail(error);
        }
        self.status = StepStatus::Failed;
    }

    pub fn complete(&mut self) {
        for step in &mut self.steps {
            if step.status == StepStatus::Running {
                step.complete();
            }
        }
        self.status = StepStatus::Completed;
    }

    pub fn fail(&mut self) {
        for step in &mut self.steps {
            if step.status == StepStatus::Running {
                step.mark_failed();
            }
        }
        self.status = StepStatus::Failed;
    }

    pub fn is_running(&self) -> bool {
        self.steps.iter().any(|s| s.status == StepStatus::Running)
    }

    pub fn total_tokens(&self) -> TokenUsage {
        let mut total = TokenUsage::default();
        for step in &self.steps {
            let step_tokens = step.total_tokens();
            total.input_tokens += step_tokens.input_tokens;
            total.output_tokens += step_tokens.output_tokens;
            total.cost_usd += step_tokens.cost_usd;
        }
        total
    }

    pub fn tick(&mut self) {
        for step in &mut self.steps {
            step.tick();
        }
    }

    pub fn completed_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Completed)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Failed)
            .count()
    }
}

/// Agent steps widget (stub -- rendering removed)
pub struct AgentStepsWidget<'a> {
    group: &'a AgentStepGroup,
    theme: &'a Theme,
}

impl<'a> AgentStepsWidget<'a> {
    pub fn new(group: &'a AgentStepGroup, theme: &'a Theme) -> Self {
        Self { group, theme }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verb_type_colors() {
        let colors: Vec<Color> = vec![
            VerbType::Infer.color(),
            VerbType::Exec.color(),
            VerbType::Fetch.color(),
            VerbType::Invoke.color(),
            VerbType::Agent.color(),
        ];

        for color in &colors {
            assert!(matches!(color, Color::Rgb(_, _, _)));
        }

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
    fn test_agent_phase_icons() {
        assert_eq!(AgentPhase::Idle.icon(), "");
        assert_eq!(AgentPhase::Syncing.icon(), "🦋");
        assert_eq!(AgentPhase::Planning.icon(), "🐔");
        assert_eq!(AgentPhase::Invoking.icon(), "🔌");
        assert_eq!(AgentPhase::Streaming.icon(), "📡");
    }

    #[test]
    fn test_agent_phase_active() {
        assert!(!AgentPhase::Idle.is_active());
        assert!(AgentPhase::Syncing.is_active());
        assert!(AgentPhase::Planning.is_active());
        assert!(AgentPhase::Invoking.is_active());
    }

    #[test]
    fn test_agent_phase_tool() {
        assert!(!AgentPhase::Idle.is_tool_phase());
        assert!(!AgentPhase::Syncing.is_tool_phase());
        assert!(AgentPhase::Invoking.is_tool_phase());
        assert!(AgentPhase::Processing.is_tool_phase());
    }

    #[test]
    fn test_phase_indicator_creation() {
        let indicator = AgentPhaseIndicator::new(AgentPhase::Syncing);
        assert_eq!(indicator.phase(), AgentPhase::Syncing);
        assert!(!indicator.is_fully_revealed());
    }

    #[test]
    fn test_phase_indicator_set_phase() {
        let mut indicator = AgentPhaseIndicator::new(AgentPhase::Syncing);
        indicator.set_phase(AgentPhase::Invoking);
        assert_eq!(indicator.phase(), AgentPhase::Invoking);
    }

    #[test]
    fn test_phase_indicator_tick() {
        let mut indicator = AgentPhaseIndicator::new(AgentPhase::Syncing);
        assert_eq!(indicator.frame(), 0);
        indicator.tick();
        assert_eq!(indicator.frame(), 1);
    }

    #[test]
    fn test_phase_indicator_idle_builds_empty_line() {
        let indicator = AgentPhaseIndicator::new(AgentPhase::Idle);
        let line = indicator.build_line();
        assert!(line.spans.is_empty());
    }

    #[test]
    fn test_step_status_default() {
        let status = StepStatus::default();
        assert_eq!(status, StepStatus::Pending);
    }

    #[test]
    fn test_token_usage() {
        let tokens = TokenUsage::new(100, 50);
        assert_eq!(tokens.total(), 150);
    }

    #[test]
    fn test_agent_step_lifecycle() {
        let mut step = AgentStep::new("Writing file.py");
        assert_eq!(step.status, StepStatus::Pending);

        step.start();
        assert_eq!(step.status, StepStatus::Running);

        step.complete();
        assert_eq!(step.status, StepStatus::Completed);
    }

    #[test]
    fn test_agent_step_failure() {
        let mut step = AgentStep::running("Compiling...");
        step.fail("Compilation error");
        assert_eq!(step.status, StepStatus::Failed);
    }

    #[test]
    fn test_agent_step_for_verb() {
        let step = AgentStep::for_verb(VerbType::Infer, "Generating content");
        assert_eq!(step.verb, VerbType::Infer);
        assert_eq!(step.status, StepStatus::Running);
    }

    #[test]
    fn test_agent_step_group() {
        let mut group = AgentStepGroup::new("Test group");
        group.start();
        group.add_running_step("Step 1");
        group.complete_current_step();
        group.add_running_step("Step 2");
        group.complete();

        assert_eq!(group.completed_count(), 2);
        assert_eq!(group.status, StepStatus::Completed);
    }
}
