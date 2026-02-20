//! Browser View
//!
//! Workflow browser for selecting and previewing workflows before execution.
//!
//! # Layout
//!
//! ```text
//! ╭──────────────────────────────────────────────────────────────────────────────────╮
//! │  ⚡ NIKA WORKFLOW STUDIO                                              v0.5.1    │
//! │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
//! ├────────────────────────┬─────────────────────────────────────────────────────────┤
//! │  📁 WORKFLOWS          │  ┌─────────────────── DAG PREVIEW ───────────────────┐  │
//! │  ══════════════════    │  │                                                   │  │
//! │                        │  │            ╭──────────╮                           │  │
//! │  ▾ 📂 examples/        │  │            │  task1   │                           │  │
//! │    ├─ 📄 invoke.nika   │  │            ╰────┬─────╯                           │  │
//! │    ├─ 📄 agent.nika    │  │                 │                                 │  │
//! │    └─ 📄 fetch.nika    │  │                 ▼                                 │  │
//! │  ▸ 📂 workflows/       │  │            ╭──────────╮                           │  │
//! │                        │  │            │  task2   │                           │  │
//! │  ─────────────────     │  │            ╰──────────╯                           │  │
//! │  ► invoke.nika.yaml    │  │                                                   │  │
//! │    4 tasks · 3 flows   │  │   Tasks: 4    Flows: 3    MCP: novanet            │  │
//! │                        │  └───────────────────────────────────────────────────┘  │
//! ├────────────────────────┼─────────────────────────────────────────────────────────┤
//! │  📋 YAML PREVIEW       │  ℹ️  WORKFLOW INFO                                      │
//! │  ══════════════════    │  ══════════════════                                    │
//! │  ┌──────────────────┐  │                                                        │
//! │  │ schema: nika/0.5 │  │  ┌─────────────────────────────────────────────────┐   │
//! │  │ workflow: invoke │  │  │ VERBS        invoke ████████░░ 3                │   │
//! │  │                  │  │  │              infer  ██░░░░░░░░ 1                │   │
//! │  │ tasks:           │  │  └─────────────────────────────────────────────────┘   │
//! │  │   - id: schema   │  │                                                        │
//! │  │     invoke: ...  │  │  ╭─────────────────────────────────────────────────╮   │
//! │  └──────────────────┘  │  │           ▶▶  PRESS ENTER TO RUN  ◀◀            │   │
//! │  ▲▼ scroll             │  ╰─────────────────────────────────────────────────╯   │
//! ╰────────────────────────┴─────────────────────────────────────────────────────────╯
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::ast::Workflow;
use crate::tui::standalone::StandaloneState;
use crate::tui::theme::Theme;

/// Focused panel in the browser view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserPanel {
    /// Workflow tree (left top)
    #[default]
    Tree,
    /// DAG preview (right top)
    DagPreview,
    /// YAML preview (left bottom)
    YamlPreview,
    /// Workflow info (right bottom)
    Info,
}

impl BrowserPanel {
    /// Cycle to next panel
    pub fn next(&self) -> Self {
        match self {
            BrowserPanel::Tree => BrowserPanel::DagPreview,
            BrowserPanel::DagPreview => BrowserPanel::YamlPreview,
            BrowserPanel::YamlPreview => BrowserPanel::Info,
            BrowserPanel::Info => BrowserPanel::Tree,
        }
    }

    /// Cycle to previous panel
    pub fn prev(&self) -> Self {
        match self {
            BrowserPanel::Tree => BrowserPanel::Info,
            BrowserPanel::DagPreview => BrowserPanel::Tree,
            BrowserPanel::YamlPreview => BrowserPanel::DagPreview,
            BrowserPanel::Info => BrowserPanel::YamlPreview,
        }
    }

    /// Get panel number (1-4)
    pub fn number(&self) -> u8 {
        match self {
            BrowserPanel::Tree => 1,
            BrowserPanel::DagPreview => 2,
            BrowserPanel::YamlPreview => 3,
            BrowserPanel::Info => 4,
        }
    }

    /// Get panel title
    pub fn title(&self) -> &'static str {
        match self {
            BrowserPanel::Tree => "WORKFLOWS",
            BrowserPanel::DagPreview => "DAG PREVIEW",
            BrowserPanel::YamlPreview => "YAML PREVIEW",
            BrowserPanel::Info => "WORKFLOW INFO",
        }
    }

    /// Get panel icon
    pub fn icon(&self) -> &'static str {
        match self {
            BrowserPanel::Tree => "📁",
            BrowserPanel::DagPreview => "🔷",
            BrowserPanel::YamlPreview => "📋",
            BrowserPanel::Info => "ℹ️",
        }
    }
}

/// Parsed workflow info for display
#[derive(Debug, Clone, Default)]
pub struct WorkflowInfo {
    /// Workflow name
    pub name: String,
    /// Number of tasks
    pub task_count: usize,
    /// Number of flows
    pub flow_count: usize,
    /// MCP servers used
    pub mcp_servers: Vec<String>,
    /// Verb counts (verb name -> count)
    pub verb_counts: HashMap<String, usize>,
    /// Schema version
    pub schema: String,
    /// Parse error if any
    pub error: Option<String>,
}

impl WorkflowInfo {
    /// Parse workflow info from YAML content
    pub fn from_yaml(yaml: &str) -> Self {
        match serde_yaml::from_str::<Workflow>(yaml) {
            Ok(workflow) => {
                let mut verb_counts = HashMap::new();

                for task in &workflow.tasks {
                    let verb = Self::get_verb_name(&task.action);
                    *verb_counts.entry(verb.to_string()).or_insert(0) += 1;
                }

                let mcp_servers = workflow
                    .mcp
                    .as_ref()
                    .map(|mcp| mcp.keys().cloned().collect())
                    .unwrap_or_default();

                // Extract name from schema (e.g., "nika/workflow@0.5" -> "workflow@0.5")
                let name = workflow
                    .schema
                    .split('/')
                    .next_back()
                    .unwrap_or(&workflow.schema)
                    .to_string();

                Self {
                    name,
                    task_count: workflow.tasks.len(),
                    flow_count: workflow.flows.len(),
                    mcp_servers,
                    verb_counts,
                    schema: workflow.schema.clone(),
                    error: None,
                }
            }
            Err(e) => Self {
                error: Some(e.to_string()),
                ..Default::default()
            },
        }
    }

    /// Get the verb name from a TaskAction
    fn get_verb_name(action: &crate::ast::TaskAction) -> &'static str {
        use crate::ast::TaskAction;
        match action {
            TaskAction::Infer { .. } => "infer",
            TaskAction::Exec { .. } => "exec",
            TaskAction::Fetch { .. } => "fetch",
            TaskAction::Invoke { .. } => "invoke",
            TaskAction::Agent { .. } => "agent",
        }
    }
}

/// Browser view state
#[derive(Debug)]
pub struct BrowserView {
    /// Standalone state (tree, history, etc.)
    pub standalone: StandaloneState,
    /// Currently focused panel
    pub focused_panel: BrowserPanel,
    /// Parsed workflow info for selected file
    pub workflow_info: Option<WorkflowInfo>,
    /// YAML scroll offset
    pub yaml_scroll: u16,
    /// DAG scroll offset
    pub dag_scroll: u16,
}

impl BrowserView {
    /// Create a new browser view
    pub fn new(root: PathBuf) -> Self {
        let standalone = StandaloneState::new(root);
        let workflow_info = if !standalone.preview_content.is_empty() {
            Some(WorkflowInfo::from_yaml(&standalone.preview_content))
        } else {
            None
        };

        Self {
            standalone,
            focused_panel: BrowserPanel::Tree,
            workflow_info,
            yaml_scroll: 0,
            dag_scroll: 0,
        }
    }

    /// Get currently selected workflow path
    pub fn selected_workflow(&self) -> Option<&Path> {
        self.standalone.selected_workflow()
    }

    /// Navigate up in the current panel
    pub fn navigate_up(&mut self) {
        match self.focused_panel {
            BrowserPanel::Tree => {
                self.standalone.browser_up();
                self.update_workflow_info();
            }
            BrowserPanel::YamlPreview => {
                self.yaml_scroll = self.yaml_scroll.saturating_sub(1);
            }
            BrowserPanel::DagPreview => {
                self.dag_scroll = self.dag_scroll.saturating_sub(1);
            }
            BrowserPanel::Info => {}
        }
    }

    /// Navigate down in the current panel
    pub fn navigate_down(&mut self) {
        match self.focused_panel {
            BrowserPanel::Tree => {
                self.standalone.browser_down();
                self.update_workflow_info();
            }
            BrowserPanel::YamlPreview => {
                let max_scroll = self
                    .standalone
                    .preview_content
                    .lines()
                    .count()
                    .saturating_sub(10) as u16;
                if self.yaml_scroll < max_scroll {
                    self.yaml_scroll += 1;
                }
            }
            BrowserPanel::DagPreview => {
                self.dag_scroll += 1;
            }
            BrowserPanel::Info => {}
        }
    }

    /// Update workflow info when selection changes
    fn update_workflow_info(&mut self) {
        self.workflow_info = if !self.standalone.preview_content.is_empty() {
            Some(WorkflowInfo::from_yaml(&self.standalone.preview_content))
        } else {
            None
        };
        self.yaml_scroll = 0;
        self.dag_scroll = 0;
    }

    /// Cycle to next panel
    pub fn next_panel(&mut self) {
        self.focused_panel = self.focused_panel.next();
    }

    /// Cycle to previous panel
    pub fn prev_panel(&mut self) {
        self.focused_panel = self.focused_panel.prev();
    }

    /// Focus a specific panel by number (1-4)
    pub fn focus_panel(&mut self, number: u8) {
        self.focused_panel = match number {
            1 => BrowserPanel::Tree,
            2 => BrowserPanel::DagPreview,
            3 => BrowserPanel::YamlPreview,
            4 => BrowserPanel::Info,
            _ => self.focused_panel,
        };
    }

    /// Render the browser view
    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        // Split into top and bottom halves
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        // Split top into left (tree) and right (DAG preview)
        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunks[0]);

        // Split bottom into left (YAML) and right (info)
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        self.render_tree(top[0], buf, theme);
        self.render_dag_preview(top[1], buf, theme);
        self.render_yaml_preview(bottom[0], buf, theme);
        self.render_info(bottom[1], buf, theme);
    }

    /// Render workflow tree panel
    fn render_tree(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let is_focused = self.focused_panel == BrowserPanel::Tree;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                BrowserPanel::Tree.icon(),
                BrowserPanel::Tree.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Build list items
        let items: Vec<ListItem> = self
            .standalone
            .browser_entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let indent = "  ".repeat(entry.depth);
                let icon = if entry.is_dir { "📂" } else { "📄" };

                let style = if i == self.standalone.browser_index {
                    Style::default().bg(theme.highlight).fg(theme.text_primary)
                } else {
                    Style::default().fg(theme.text_muted)
                };

                let text = format!("{}{} {}", indent, icon, entry.display_name);
                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items);
        Widget::render(list, inner, buf);
    }

    /// Render DAG preview panel
    fn render_dag_preview(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let is_focused = self.focused_panel == BrowserPanel::DagPreview;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                BrowserPanel::DagPreview.icon(),
                BrowserPanel::DagPreview.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Generate ASCII DAG from workflow info
        let dag_text = if let Some(info) = &self.workflow_info {
            if let Some(error) = &info.error {
                format!("⚠️ Parse error:\n{}", error)
            } else {
                self.generate_dag_ascii(info)
            }
        } else {
            "No workflow selected".to_string()
        };

        let paragraph = Paragraph::new(dag_text)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false })
            .scroll((self.dag_scroll, 0));

        paragraph.render(inner, buf);
    }

    /// Generate ASCII DAG visualization
    fn generate_dag_ascii(&self, info: &WorkflowInfo) -> String {
        let mut lines = Vec::new();

        lines.push(format!("   Workflow: {}", info.name));
        lines.push(String::new());

        // Simple vertical task list (can be enhanced later)
        lines.push("   ╭──────────────────╮".to_string());
        lines.push("   │                  │".to_string());

        for i in 0..info.task_count {
            lines.push(format!("   │     task-{}       │", i + 1));
            if i < info.task_count - 1 {
                lines.push("   │        ↓         │".to_string());
            }
        }

        if info.task_count > 0 {
            lines.push("   ╰──────────────────╯".to_string());
        }

        lines.push(String::new());
        lines.push(format!(
            "   Tasks: {}    Flows: {}",
            info.task_count, info.flow_count
        ));

        if !info.mcp_servers.is_empty() {
            lines.push(format!("   MCP: {}", info.mcp_servers.join(", ")));
        }

        lines.join("\n")
    }

    /// Render YAML preview panel
    fn render_yaml_preview(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let is_focused = self.focused_panel == BrowserPanel::YamlPreview;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                BrowserPanel::YamlPreview.icon(),
                BrowserPanel::YamlPreview.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        let content = if self.standalone.preview_content.is_empty() {
            "No file selected".to_string()
        } else {
            self.standalone.preview_content.clone()
        };

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_muted))
            .wrap(Wrap { trim: false })
            .scroll((self.yaml_scroll, 0));

        paragraph.render(inner, buf);
    }

    /// Render workflow info panel
    fn render_info(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let is_focused = self.focused_panel == BrowserPanel::Info;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                BrowserPanel::Info.icon(),
                BrowserPanel::Info.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        let content = if let Some(info) = &self.workflow_info {
            if let Some(error) = &info.error {
                format!("⚠️ Parse error:\n{}", error)
            } else {
                self.format_workflow_info(info)
            }
        } else {
            "No workflow selected".to_string()
        };

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false });

        paragraph.render(inner, buf);
    }

    /// Format workflow info for display
    fn format_workflow_info(&self, info: &WorkflowInfo) -> String {
        let mut lines = Vec::new();

        lines.push(format!("Schema: {}", info.schema));
        lines.push(format!("Name: {}", info.name));
        lines.push(String::new());

        // Verb distribution
        lines.push("VERBS".to_string());
        lines.push("─────────────────────────".to_string());

        let max_count = info.verb_counts.values().max().copied().unwrap_or(1);
        for (verb, count) in &info.verb_counts {
            let bar_len = (*count * 20) / max_count.max(1);
            let bar = "█".repeat(bar_len);
            let empty = "░".repeat(20 - bar_len);
            lines.push(format!("{:10} {}{} {}", verb, bar, empty, count));
        }

        lines.push(String::new());

        // MCP servers
        if !info.mcp_servers.is_empty() {
            lines.push("MCP SERVERS".to_string());
            lines.push("─────────────────────────".to_string());
            for server in &info.mcp_servers {
                lines.push(format!("  ● {}", server));
            }
            lines.push(String::new());
        }

        // Run prompt
        lines.push("╭─────────────────────────────────────────────╮".to_string());
        lines.push("│         ▶▶  PRESS ENTER TO RUN  ◀◀         │".to_string());
        lines.push("╰─────────────────────────────────────────────╯".to_string());

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_panel_cycle() {
        let panel = BrowserPanel::Tree;
        assert_eq!(panel.next(), BrowserPanel::DagPreview);
        assert_eq!(panel.next().next(), BrowserPanel::YamlPreview);
        assert_eq!(panel.next().next().next(), BrowserPanel::Info);
        assert_eq!(panel.next().next().next().next(), BrowserPanel::Tree);
    }

    #[test]
    fn test_browser_panel_numbers() {
        assert_eq!(BrowserPanel::Tree.number(), 1);
        assert_eq!(BrowserPanel::DagPreview.number(), 2);
        assert_eq!(BrowserPanel::YamlPreview.number(), 3);
        assert_eq!(BrowserPanel::Info.number(), 4);
    }

    #[test]
    fn test_workflow_info_from_valid_yaml() {
        // Note: Workflow struct doesn't have a `workflow:` field
        // The name is extracted from the schema (e.g., "nika/workflow@0.5" -> "workflow@0.5")
        // InvokeParams uses `mcp` field for server name, not `server`
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Generate something"
  - id: task2
    invoke:
      mcp: novanet
      tool: novanet_describe
"#;
        let info = WorkflowInfo::from_yaml(yaml);
        assert!(info.error.is_none(), "Parse error: {:?}", info.error);
        assert_eq!(info.name, "workflow@0.5"); // Extracted from schema
        assert_eq!(info.task_count, 2);
        assert_eq!(info.verb_counts.get("infer"), Some(&1));
        assert_eq!(info.verb_counts.get("invoke"), Some(&1));
    }

    #[test]
    fn test_workflow_info_from_invalid_yaml() {
        let yaml = "not: valid: yaml:";
        let info = WorkflowInfo::from_yaml(yaml);
        assert!(info.error.is_some());
    }
}
