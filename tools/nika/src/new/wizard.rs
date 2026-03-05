//! Interactive wizard for `nika new`
//!
//! Provides a TUI-based workflow creation wizard using ratatui.

use super::{NewWorkflowConfig, OutputFormat, Provider, Template, Verb};
use crate::error::NikaError;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io::{self, stdout};
use std::path::PathBuf;

/// Workflow purpose category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowPurpose {
    #[default]
    Research,
    Content,
    Code,
    Data,
    Automation,
}

impl WorkflowPurpose {
    const ALL: [WorkflowPurpose; 5] = [
        WorkflowPurpose::Research,
        WorkflowPurpose::Content,
        WorkflowPurpose::Code,
        WorkflowPurpose::Data,
        WorkflowPurpose::Automation,
    ];

    fn name(&self) -> &'static str {
        match self {
            WorkflowPurpose::Research => "Research",
            WorkflowPurpose::Content => "Content",
            WorkflowPurpose::Code => "Code",
            WorkflowPurpose::Data => "Data",
            WorkflowPurpose::Automation => "Automation",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            WorkflowPurpose::Research => "Web search, analysis, summarization",
            WorkflowPurpose::Content => "Writing, translation, localization",
            WorkflowPurpose::Code => "Review, generation, documentation",
            WorkflowPurpose::Data => "ETL, validation, transformation",
            WorkflowPurpose::Automation => "Pipelines, monitoring, DevOps",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            WorkflowPurpose::Research => "🔍",
            WorkflowPurpose::Content => "📝",
            WorkflowPurpose::Code => "💻",
            WorkflowPurpose::Data => "📊",
            WorkflowPurpose::Automation => "⚙️",
        }
    }

    /// Suggest templates based on purpose
    #[allow(dead_code)]
    fn suggested_templates(&self) -> Vec<super::Template> {
        use super::Template;
        match self {
            WorkflowPurpose::Research => {
                vec![Template::AgentResearch, Template::McpIntegration]
            }
            WorkflowPurpose::Content => {
                vec![Template::BlogGenerator, Template::SimpleInfer]
            }
            WorkflowPurpose::Code => vec![Template::CodeReview, Template::SimpleExec],
            WorkflowPurpose::Data => vec![Template::ApiPipeline, Template::SimpleFetch],
            WorkflowPurpose::Automation => vec![Template::MultiProvider, Template::ApiPipeline],
        }
    }
}

/// Workflow complexity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowComplexity {
    #[default]
    Simple,
    Pipeline,
    Agent,
    Complex,
}

impl WorkflowComplexity {
    const ALL: [WorkflowComplexity; 4] = [
        WorkflowComplexity::Simple,
        WorkflowComplexity::Pipeline,
        WorkflowComplexity::Agent,
        WorkflowComplexity::Complex,
    ];

    fn name(&self) -> &'static str {
        match self {
            WorkflowComplexity::Simple => "Simple",
            WorkflowComplexity::Pipeline => "Pipeline",
            WorkflowComplexity::Agent => "Agent",
            WorkflowComplexity::Complex => "Complex",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            WorkflowComplexity::Simple => "1-2 tasks, quick scripts",
            WorkflowComplexity::Pipeline => "3-5 tasks with dependencies",
            WorkflowComplexity::Agent => "Multi-turn reasoning with tools",
            WorkflowComplexity::Complex => "Nested agents, MCP, artifacts",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            WorkflowComplexity::Simple => "📄",
            WorkflowComplexity::Pipeline => "🔗",
            WorkflowComplexity::Agent => "🐔",
            WorkflowComplexity::Complex => "🌐",
        }
    }
}

/// Wizard step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WizardStep {
    /// Choose workflow purpose (Research, Content, Code, Data, Automation)
    SelectPurpose,
    /// Choose complexity level (Simple, Pipeline, Agent, Complex)
    SelectComplexity,
    /// Choose mode: template or custom
    ChooseMode,
    /// Select template (if template mode)
    SelectTemplate,
    /// Enter workflow name
    EnterName,
    /// Select verb (if custom mode)
    SelectVerb,
    /// Select provider
    SelectProvider,
    /// Select output format
    SelectOutput,
    /// Toggle options (MCP, include, artifacts)
    ToggleOptions,
    /// Configure MCP servers
    ConfigureMcp,
    /// Preview generated YAML
    Preview,
    /// Confirm and create
    Confirm,
    /// Done - show result
    Done,
}

/// Wizard mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WizardMode {
    Template,
    Custom,
}

/// Option toggles
#[derive(Debug, Clone, Default)]
struct Options {
    with_mcp: bool,
    with_include: bool,
    with_artifacts: bool,
}

/// MCP server option for configuration
#[derive(Debug, Clone)]
struct McpServerOption {
    name: String,
    description: String,
    enabled: bool,
}

/// Wizard state
struct WizardState {
    step: WizardStep,
    /// Workflow purpose (Phase 5.1)
    purpose: WorkflowPurpose,
    /// Workflow complexity (Phase 5.1)
    complexity: WorkflowComplexity,
    mode: Option<WizardMode>,
    template: Option<Template>,
    name: String,
    name_cursor: usize,
    verb: Verb,
    provider: Provider,
    output_format: OutputFormat,
    options: Options,
    list_state: ListState,
    option_index: usize,
    /// MCP servers available for selection (Phase 5.1)
    mcp_servers: Vec<McpServerOption>,
    mcp_index: usize,
    /// Generated YAML preview (Phase 5.1)
    preview_yaml: String,
    preview_scroll: u16,
    result_path: Option<PathBuf>,
    error_message: Option<String>,
    output_dir: PathBuf,
}

impl WizardState {
    fn new(output_dir: PathBuf) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        // Pre-populate common MCP servers
        let mcp_servers = vec![
            McpServerOption {
                name: "novanet".to_string(),
                description: "NovaNet knowledge graph".to_string(),
                enabled: false,
            },
            McpServerOption {
                name: "filesystem".to_string(),
                description: "File system operations".to_string(),
                enabled: false,
            },
            McpServerOption {
                name: "web-search".to_string(),
                description: "Web search (Perplexity/Brave)".to_string(),
                enabled: false,
            },
            McpServerOption {
                name: "github".to_string(),
                description: "GitHub API integration".to_string(),
                enabled: false,
            },
        ];

        Self {
            step: WizardStep::SelectPurpose,
            purpose: WorkflowPurpose::default(),
            complexity: WorkflowComplexity::default(),
            mode: None,
            template: None,
            name: String::new(),
            name_cursor: 0,
            verb: Verb::default(),
            provider: Provider::default(),
            output_format: OutputFormat::default(),
            options: Options::default(),
            list_state,
            option_index: 0,
            mcp_servers,
            mcp_index: 0,
            preview_yaml: String::new(),
            preview_scroll: 0,
            result_path: None,
            error_message: None,
            output_dir,
        }
    }

    /// Generate YAML preview based on current state
    fn generate_preview(&mut self) {
        if let Some(template) = self.template {
            // For templates, just show template description
            self.preview_yaml = format!(
                "# Template: {}\n# {}\n\n{}",
                template.name(),
                template.description(),
                "# Full YAML will be generated on creation"
            );
        } else {
            // For custom, generate based on selections
            let mcp_block = if self.options.with_mcp {
                let enabled: Vec<_> = self.mcp_servers.iter().filter(|s| s.enabled).collect();
                if enabled.is_empty() {
                    String::new()
                } else {
                    let servers: String = enabled
                        .iter()
                        .map(|s| format!("    {}:\n      command: \"...\"\n", s.name))
                        .collect();
                    format!("\nmcp:\n  servers:\n{}", servers)
                }
            } else {
                String::new()
            };

            let artifacts_block = if self.options.with_artifacts {
                "\n\nartifacts:\n  dir: ./output/{{date}}\n  format: json".to_string()
            } else {
                String::new()
            };

            let verb_name = self.verb.name();
            self.preview_yaml = format!(
                r#"schema: "nika/workflow@0.10"
provider: {}
{}{}
tasks:
  - id: {}
    {}: "Your {} task prompt here"
"#,
                self.provider.name().to_lowercase(),
                mcp_block,
                artifacts_block,
                self.name.replace('-', "_"),
                verb_name,
                verb_name
            );
        }
    }

    fn selected_index(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    fn select_next(&mut self, max: usize) {
        let current = self.selected_index();
        let next = if current >= max - 1 { 0 } else { current + 1 };
        self.list_state.select(Some(next));
    }

    fn select_prev(&mut self, max: usize) {
        let current = self.selected_index();
        let prev = if current == 0 { max - 1 } else { current - 1 };
        self.list_state.select(Some(prev));
    }
}

/// Run the interactive wizard
pub fn run_wizard(output_dir: PathBuf) -> Result<PathBuf, NikaError> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run wizard loop
    let result = run_wizard_loop(&mut terminal, output_dir);

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_wizard_loop(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
    output_dir: PathBuf,
) -> Result<PathBuf, NikaError> {
    let mut state = WizardState::new(output_dir);

    loop {
        // Draw UI
        terminal.draw(|f| draw_wizard(f, &state))?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Clear error on any key
            state.error_message = None;

            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    return Err(NikaError::ValidationError {
                        reason: "Wizard cancelled".to_string(),
                    });
                }
                _ => {}
            }

            match state.step {
                WizardStep::SelectPurpose => handle_select_purpose(&mut state, key.code),
                WizardStep::SelectComplexity => handle_select_complexity(&mut state, key.code),
                WizardStep::ChooseMode => handle_choose_mode(&mut state, key.code),
                WizardStep::SelectTemplate => handle_select_template(&mut state, key.code),
                WizardStep::EnterName => handle_enter_name(&mut state, key.code),
                WizardStep::SelectVerb => handle_select_verb(&mut state, key.code),
                WizardStep::SelectProvider => handle_select_provider(&mut state, key.code),
                WizardStep::SelectOutput => handle_select_output(&mut state, key.code),
                WizardStep::ToggleOptions => handle_toggle_options(&mut state, key.code),
                WizardStep::ConfigureMcp => handle_configure_mcp(&mut state, key.code),
                WizardStep::Preview => handle_preview(&mut state, key.code),
                WizardStep::Confirm => handle_confirm(&mut state, key.code)?,
                WizardStep::Done => {
                    if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                        return state.result_path.clone().ok_or(NikaError::ValidationError {
                            reason: "No result path".to_string(),
                        });
                    }
                }
            }
        }
    }
}

fn handle_select_purpose(state: &mut WizardState, key: KeyCode) {
    let count = WorkflowPurpose::ALL.len();
    match key {
        KeyCode::Up | KeyCode::Char('k') => state.select_prev(count),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(count),
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.purpose = WorkflowPurpose::ALL[state.selected_index()];
            state.list_state.select(Some(0));
            state.step = WizardStep::SelectComplexity;
        }
        _ => {}
    }
}

fn handle_select_complexity(state: &mut WizardState, key: KeyCode) {
    let count = WorkflowComplexity::ALL.len();
    match key {
        KeyCode::Up | KeyCode::Char('k') => state.select_prev(count),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(count),
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.complexity = WorkflowComplexity::ALL[state.selected_index()];
            state.list_state.select(Some(0));
            state.step = WizardStep::ChooseMode;
        }
        KeyCode::Backspace => {
            state.step = WizardStep::SelectPurpose;
            state.list_state.select(Some(0));
        }
        _ => {}
    }
}

fn handle_configure_mcp(state: &mut WizardState, key: KeyCode) {
    let count = state.mcp_servers.len();
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if state.mcp_index > 0 {
                state.mcp_index -= 1;
            } else {
                state.mcp_index = count.saturating_sub(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.mcp_index < count.saturating_sub(1) {
                state.mcp_index += 1;
            } else {
                state.mcp_index = 0;
            }
        }
        KeyCode::Char(' ') => {
            if state.mcp_index < count {
                state.mcp_servers[state.mcp_index].enabled =
                    !state.mcp_servers[state.mcp_index].enabled;
            }
        }
        KeyCode::Enter => {
            state.generate_preview();
            state.step = WizardStep::Preview;
        }
        KeyCode::Backspace => {
            state.step = WizardStep::ToggleOptions;
            state.option_index = 0;
        }
        _ => {}
    }
}

fn handle_preview(state: &mut WizardState, key: KeyCode) {
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            state.preview_scroll = state.preview_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.preview_scroll = state.preview_scroll.saturating_add(1);
        }
        KeyCode::Enter => {
            state.step = WizardStep::Confirm;
        }
        KeyCode::Backspace => {
            if state.options.with_mcp {
                state.step = WizardStep::ConfigureMcp;
            } else {
                state.step = WizardStep::ToggleOptions;
            }
        }
        KeyCode::Char('e') => {
            // Edit - go back to beginning for custom workflows
            if state.mode == Some(WizardMode::Custom) {
                state.step = WizardStep::EnterName;
            }
        }
        _ => {}
    }
}

fn handle_choose_mode(state: &mut WizardState, key: KeyCode) {
    match key {
        KeyCode::Up | KeyCode::Char('k') => state.select_prev(2),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(2),
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.mode = Some(if state.selected_index() == 0 {
                WizardMode::Template
            } else {
                WizardMode::Custom
            });
            state.list_state.select(Some(0));
            state.step = if state.mode == Some(WizardMode::Template) {
                WizardStep::SelectTemplate
            } else {
                WizardStep::EnterName
            };
        }
        _ => {}
    }
}

fn handle_select_template(state: &mut WizardState, key: KeyCode) {
    let count = Template::ALL.len();
    match key {
        KeyCode::Up | KeyCode::Char('k') => state.select_prev(count),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(count),
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.template = Some(Template::ALL[state.selected_index()]);
            state.step = WizardStep::EnterName;
        }
        KeyCode::Backspace => {
            state.step = WizardStep::ChooseMode;
            state.list_state.select(Some(0));
        }
        _ => {}
    }
}

fn handle_enter_name(state: &mut WizardState, key: KeyCode) {
    match key {
        KeyCode::Char(c) => {
            // Only allow valid workflow name characters
            if c.is_alphanumeric() || c == '-' || c == '_' {
                state.name.insert(state.name_cursor, c);
                state.name_cursor += 1;
            }
        }
        KeyCode::Backspace => {
            if state.name_cursor > 0 {
                state.name_cursor -= 1;
                state.name.remove(state.name_cursor);
            }
        }
        KeyCode::Delete => {
            if state.name_cursor < state.name.len() {
                state.name.remove(state.name_cursor);
            }
        }
        KeyCode::Left => {
            if state.name_cursor > 0 {
                state.name_cursor -= 1;
            }
        }
        KeyCode::Right => {
            if state.name_cursor < state.name.len() {
                state.name_cursor += 1;
            }
        }
        KeyCode::Home => state.name_cursor = 0,
        KeyCode::End => state.name_cursor = state.name.len(),
        KeyCode::Enter => {
            if state.name.is_empty() {
                state.error_message = Some("Name cannot be empty".to_string());
            } else if state.name.len() < 2 {
                state.error_message = Some("Name must be at least 2 characters".to_string());
            } else {
                state.list_state.select(Some(0));
                state.step = if state.mode == Some(WizardMode::Template) {
                    WizardStep::Confirm
                } else {
                    WizardStep::SelectVerb
                };
            }
        }
        KeyCode::Tab | KeyCode::BackTab => {
            // Auto-suggest name from template
            if state.template.is_some() && state.name.is_empty() {
                state.name = "my-workflow".to_string();
                state.name_cursor = state.name.len();
            }
        }
        _ => {}
    }
}

fn handle_select_verb(state: &mut WizardState, key: KeyCode) {
    let verbs = [
        Verb::Infer,
        Verb::Exec,
        Verb::Fetch,
        Verb::Invoke,
        Verb::Agent,
    ];
    match key {
        KeyCode::Up | KeyCode::Char('k') => state.select_prev(verbs.len()),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(verbs.len()),
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.verb = verbs[state.selected_index()];
            state.list_state.select(Some(0));
            state.step = WizardStep::SelectProvider;
        }
        KeyCode::Backspace => {
            state.step = WizardStep::EnterName;
        }
        _ => {}
    }
}

fn handle_select_provider(state: &mut WizardState, key: KeyCode) {
    let providers = [
        Provider::Claude,
        Provider::OpenAI,
        Provider::Mistral,
        Provider::Groq,
        Provider::DeepSeek,
        Provider::Ollama,
    ];
    match key {
        KeyCode::Up | KeyCode::Char('k') => state.select_prev(providers.len()),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(providers.len()),
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.provider = providers[state.selected_index()];
            state.list_state.select(Some(0));
            state.step = WizardStep::SelectOutput;
        }
        KeyCode::Backspace => {
            state.step = WizardStep::SelectVerb;
            state.list_state.select(Some(0));
        }
        _ => {}
    }
}

fn handle_select_output(state: &mut WizardState, key: KeyCode) {
    let formats = [OutputFormat::Text, OutputFormat::Json, OutputFormat::Yaml];
    match key {
        KeyCode::Up | KeyCode::Char('k') => state.select_prev(formats.len()),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(formats.len()),
        KeyCode::Enter | KeyCode::Char(' ') => {
            state.output_format = formats[state.selected_index()];
            state.option_index = 0;
            state.step = WizardStep::ToggleOptions;
        }
        KeyCode::Backspace => {
            state.step = WizardStep::SelectProvider;
            state.list_state.select(Some(0));
        }
        _ => {}
    }
}

fn handle_toggle_options(state: &mut WizardState, key: KeyCode) {
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if state.option_index > 0 {
                state.option_index -= 1;
            } else {
                state.option_index = 2;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.option_index < 2 {
                state.option_index += 1;
            } else {
                state.option_index = 0;
            }
        }
        KeyCode::Char(' ') => match state.option_index {
            0 => state.options.with_mcp = !state.options.with_mcp,
            1 => state.options.with_include = !state.options.with_include,
            2 => state.options.with_artifacts = !state.options.with_artifacts,
            _ => {}
        },
        KeyCode::Enter => {
            state.step = WizardStep::Confirm;
        }
        KeyCode::Backspace => {
            state.step = WizardStep::SelectOutput;
            state.list_state.select(Some(0));
        }
        _ => {}
    }
}

fn handle_confirm(state: &mut WizardState, key: KeyCode) -> Result<(), NikaError> {
    match key {
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
            // Create the workflow
            let result = if let Some(template) = state.template {
                super::create_from_template(&state.name, template, &state.output_dir)
            } else {
                let config = NewWorkflowConfig {
                    name: state.name.clone(),
                    description: None,
                    verb: state.verb,
                    provider: state.provider,
                    model: None,
                    output_format: state.output_format,
                    with_mcp: state.options.with_mcp,
                    with_include: state.options.with_include,
                    with_artifacts: state.options.with_artifacts,
                    output_dir: state.output_dir.clone(),
                };
                config.write()
            };

            match result {
                Ok(path) => {
                    state.result_path = Some(path);
                    state.step = WizardStep::Done;
                }
                Err(e) => {
                    state.error_message = Some(e.to_string());
                }
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Backspace => {
            // Go back
            state.step = if state.mode == Some(WizardMode::Template) {
                WizardStep::EnterName
            } else {
                WizardStep::ToggleOptions
            };
        }
        _ => {}
    }
    Ok(())
}

fn draw_wizard(f: &mut Frame, state: &WizardState) {
    let size = f.area();

    // Create centered layout
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(20),
            Constraint::Min(3),
        ])
        .split(size);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(60),
            Constraint::Min(5),
        ])
        .split(vertical[1]);

    let area = horizontal[1];

    // Draw border
    let block = Block::default()
        .title(" nika new - Workflow Wizard ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    // Draw step content
    match state.step {
        WizardStep::SelectPurpose => draw_select_purpose(f, inner, state),
        WizardStep::SelectComplexity => draw_select_complexity(f, inner, state),
        WizardStep::ChooseMode => draw_choose_mode(f, inner, state),
        WizardStep::SelectTemplate => draw_select_template(f, inner, state),
        WizardStep::EnterName => draw_enter_name(f, inner, state),
        WizardStep::SelectVerb => draw_select_verb(f, inner, state),
        WizardStep::SelectProvider => draw_select_provider(f, inner, state),
        WizardStep::SelectOutput => draw_select_output(f, inner, state),
        WizardStep::ToggleOptions => draw_toggle_options(f, inner, state),
        WizardStep::ConfigureMcp => draw_configure_mcp(f, inner, state),
        WizardStep::Preview => draw_preview(f, inner, state),
        WizardStep::Confirm => draw_confirm(f, inner, state),
        WizardStep::Done => draw_done(f, inner, state),
    }

    // Draw error if present
    if let Some(ref err) = state.error_message {
        draw_error(f, size, err);
    }
}

fn draw_step_header(f: &mut Frame, area: Rect, title: &str, step: usize, total: usize) {
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("[{}/{}] ", step, total),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(header, area);
}

fn draw_select_purpose(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "What kind of workflow?", 1, 11);

    let items: Vec<ListItem> = WorkflowPurpose::ALL
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", p.icon()), Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{:<12}", p.name()),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(p.description(), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut list_state = state.list_state;
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let help = Paragraph::new("[j/k] Navigate  [Enter] Select  [Esc] Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_select_complexity(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Workflow complexity", 2, 11);

    let items: Vec<ListItem> = WorkflowComplexity::ALL
        .iter()
        .map(|c| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", c.icon()), Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{:<12}", c.name()),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(c.description(), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut list_state = state.list_state;
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    // Show purpose context
    let context = Paragraph::new(Line::from(vec![
        Span::styled("Purpose: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} {}", state.purpose.icon(), state.purpose.name()),
            Style::default().fg(Color::Cyan),
        ),
    ]))
    .alignment(Alignment::Right);
    f.render_widget(context, chunks[0]);

    let help = Paragraph::new("[j/k] Navigate  [Enter] Select  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_configure_mcp(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Configure MCP servers", 9, 11);

    let checkbox = |checked: bool| if checked { "[x]" } else { "[ ]" };

    let items: Vec<ListItem> = state
        .mcp_servers
        .iter()
        .enumerate()
        .map(|(i, server)| {
            ListItem::new(Line::from(vec![
                Span::raw(if i == state.mcp_index { "> " } else { "  " }),
                Span::styled(
                    checkbox(server.enabled),
                    Style::default().fg(if server.enabled {
                        Color::Green
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:<15}", server.name),
                    Style::default().fg(Color::White),
                ),
                Span::styled(&server.description, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, chunks[1]);

    let help = Paragraph::new("[j/k] Navigate  [Space] Toggle  [Enter] Continue  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_preview(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Preview workflow YAML", 10, 11);

    // Split preview lines and apply scroll
    let lines: Vec<Line> = state
        .preview_yaml
        .lines()
        .skip(state.preview_scroll as usize)
        .take(chunks[1].height as usize)
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + state.preview_scroll as usize + 1;
            Line::from(vec![
                Span::styled(
                    format!("{:3} │ ", line_num),
                    Style::default().fg(Color::DarkGray),
                ),
                // Syntax highlighting for YAML
                if line.trim().starts_with('#') {
                    Span::styled(line, Style::default().fg(Color::DarkGray))
                } else if line.contains(':') {
                    let parts: Vec<&str> = line.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        Line::from(vec![
                            Span::styled(parts[0], Style::default().fg(Color::Cyan)),
                            Span::styled(":", Style::default().fg(Color::White)),
                            Span::styled(parts[1], Style::default().fg(Color::Green)),
                        ])
                        .spans
                        .into_iter()
                        .collect::<Vec<_>>()
                        .first()
                        .cloned()
                        .unwrap_or_else(|| Span::raw(line))
                    } else {
                        Span::raw(line)
                    }
                } else {
                    Span::raw(line)
                },
            ])
        })
        .collect();

    let preview = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Generated YAML ")
                .title_alignment(Alignment::Left),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(preview, chunks[1]);

    let help = Paragraph::new("[j/k] Scroll  [Enter] Confirm  [e] Edit  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_choose_mode(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Choose workflow creation mode", 3, 11);

    let items: Vec<ListItem> = vec![
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("Template", Style::default().fg(Color::Green)),
            Span::raw(" - Start from a pre-built template"),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("Custom", Style::default().fg(Color::Yellow)),
            Span::raw("   - Build step-by-step"),
        ])),
    ];

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut list_state = state.list_state;
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let help = Paragraph::new("[j/k] Navigate  [Enter] Select  [Esc] Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_select_template(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Select a template", 4, 11);

    let items: Vec<ListItem> = Template::ALL
        .iter()
        .map(|t| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<15}", t.name()),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(t.description(), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut list_state = state.list_state;
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let help = Paragraph::new("[j/k] Navigate  [Enter] Select  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_enter_name(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    let step = 5;
    draw_step_header(f, chunks[0], "Enter workflow name", step, 11);

    // Input field
    let input = Paragraph::new(Line::from(vec![
        Span::raw(&state.name),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]))
    .style(Style::default().fg(Color::White))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(input, chunks[1]);

    // Filename preview
    let preview = format!(
        "File: {}.nika.yaml",
        if state.name.is_empty() {
            "<name>"
        } else {
            &state.name
        }
    );
    let preview_widget = Paragraph::new(preview).style(Style::default().fg(Color::DarkGray));
    f.render_widget(preview_widget, chunks[2]);

    let help = Paragraph::new("[Enter] Continue  [Tab] Auto-fill  [Backspace] Delete")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[4]);
}

fn draw_select_verb(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Select primary verb", 6, 11);

    let verbs = [
        Verb::Infer,
        Verb::Exec,
        Verb::Fetch,
        Verb::Invoke,
        Verb::Agent,
    ];
    let items: Vec<ListItem> = verbs
        .iter()
        .map(|v| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<8}", v.name()),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(v.description(), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut list_state = state.list_state;
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let help = Paragraph::new("[j/k] Navigate  [Enter] Select  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_select_provider(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Select LLM provider", 7, 11);

    let providers = [
        Provider::Claude,
        Provider::OpenAI,
        Provider::Mistral,
        Provider::Groq,
        Provider::DeepSeek,
        Provider::Ollama,
    ];
    let items: Vec<ListItem> = providers
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<10}", p.name()),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    format!("Model: {}", p.default_model()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut list_state = state.list_state;
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let help = Paragraph::new("[j/k] Navigate  [Enter] Select  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_select_output(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Select output format", 8, 11);

    let formats = [OutputFormat::Text, OutputFormat::Json, OutputFormat::Yaml];
    let items: Vec<ListItem> = formats
        .iter()
        .map(|fmt| {
            let desc = match fmt {
                OutputFormat::Text => "Plain text output",
                OutputFormat::Json => "Structured JSON with schema validation",
                OutputFormat::Yaml => "YAML formatted output",
                OutputFormat::File => "Write to file",
            };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<6}", fmt.name()),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(desc, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut list_state = state.list_state;
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let help = Paragraph::new("[j/k] Navigate  [Enter] Select  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_toggle_options(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Toggle additional options", 9, 11);

    let checkbox = |checked: bool| if checked { "[x]" } else { "[ ]" };

    let items: Vec<ListItem> = vec![
        ListItem::new(Line::from(vec![
            Span::raw(if state.option_index == 0 { "> " } else { "  " }),
            Span::styled(
                checkbox(state.options.with_mcp),
                Style::default().fg(if state.options.with_mcp {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
            Span::raw(" "),
            Span::styled("MCP Integration", Style::default().fg(Color::White)),
            Span::raw(" - Include MCP server configuration"),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw(if state.option_index == 1 { "> " } else { "  " }),
            Span::styled(
                checkbox(state.options.with_include),
                Style::default().fg(if state.options.with_include {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
            Span::raw(" "),
            Span::styled("Include Example", Style::default().fg(Color::White)),
            Span::raw(" - Add subworkflow include"),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw(if state.option_index == 2 { "> " } else { "  " }),
            Span::styled(
                checkbox(state.options.with_artifacts),
                Style::default().fg(if state.options.with_artifacts {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
            Span::raw(" "),
            Span::styled("Artifacts", Style::default().fg(Color::White)),
            Span::raw(" - Include artifact output config"),
        ])),
    ];

    let list = List::new(items);
    f.render_widget(list, chunks[1]);

    let help = Paragraph::new("[j/k] Navigate  [Space] Toggle  [Enter] Continue  [Backspace] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_confirm(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    draw_step_header(f, chunks[0], "Confirm workflow creation", 11, 11);

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Name:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(&state.name, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  File:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}.nika.yaml", state.name),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    if let Some(template) = state.template {
        lines.push(Line::from(vec![
            Span::styled("  Template: ", Style::default().fg(Color::DarkGray)),
            Span::styled(template.name(), Style::default().fg(Color::Green)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("  Verb:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(state.verb.name(), Style::default().fg(Color::Green)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Provider: ", Style::default().fg(Color::DarkGray)),
            Span::styled(state.provider.name(), Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Output:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.output_format.name(),
                Style::default().fg(Color::White),
            ),
        ]));

        let mut options = vec![];
        if state.options.with_mcp {
            options.push("MCP");
        }
        if state.options.with_include {
            options.push("Include");
        }
        if state.options.with_artifacts {
            options.push("Artifacts");
        }
        if !options.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Options:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(options.join(", "), Style::default().fg(Color::Magenta)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "  Create this workflow? ",
            Style::default().fg(Color::White),
        ),
        Span::styled("[Y/n]", Style::default().fg(Color::Green)),
    ]));

    let summary = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(summary, chunks[1]);

    let help = Paragraph::new("[Y] Create  [N/Backspace] Go back  [Esc] Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_done(f: &mut Frame, area: Rect, state: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);

    let title = Paragraph::new(Line::from(vec![Span::styled(
        "SUCCESS!",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )]));
    f.render_widget(title, chunks[0]);

    let path = state
        .result_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Created: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&path, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Run: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("nika {}", path), Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
    ];

    let content = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(content, chunks[1]);

    let help = Paragraph::new("[Enter] Exit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

fn draw_error(f: &mut Frame, size: Rect, message: &str) {
    let area = Rect {
        x: 5,
        y: size.height.saturating_sub(3),
        width: size.width.saturating_sub(10),
        height: 2,
    };

    let error = Paragraph::new(Line::from(vec![
        Span::styled(" ERROR: ", Style::default().fg(Color::White).bg(Color::Red)),
        Span::styled(format!(" {} ", message), Style::default().fg(Color::Red)),
    ]));

    f.render_widget(Clear, area);
    f.render_widget(error, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_state_new() {
        let state = WizardState::new(PathBuf::from("."));
        assert_eq!(state.step, WizardStep::SelectPurpose);
        assert!(state.name.is_empty());
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn test_wizard_state_navigation() {
        let mut state = WizardState::new(PathBuf::from("."));

        state.select_next(3);
        assert_eq!(state.selected_index(), 1);

        state.select_next(3);
        assert_eq!(state.selected_index(), 2);

        state.select_next(3);
        assert_eq!(state.selected_index(), 0); // Wraps

        state.select_prev(3);
        assert_eq!(state.selected_index(), 2); // Wraps back
    }
}
