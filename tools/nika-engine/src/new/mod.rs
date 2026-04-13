// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika new - Workflow scaffolding module
//!
//! Provides two modes for creating new workflows:
//! 1. **Flag Mode** - CLI flags for power users
//! 2. **Template Mode** - Pre-built templates for common use cases
//!
//! ## Templates
//!
//! | Template | Description |
//! |----------|-------------|
//! | simple-infer | Basic LLM text generation |
//! | simple-exec | Shell command execution |
//! | simple-fetch | HTTP request example |
//! | api-pipeline | Multi-step API workflow |
//! | blog-generator | Blog content pipeline |
//! | code-review | Code review assistant |
//! | agent-research | Research agent with MCP |
//! | agent-browser | Browser automation agent |
//! | mcp-integration | MCP server integration |
//! | multi-provider | Multiple LLM providers |
//!
//! ## Usage
//!
//! ```bash
//! # Template mode
//! nika new my-workflow --template blog-generator
//!
//! # Flag mode
//! nika new my-workflow --verb infer --provider claude --output json
//! ```

pub mod templates;

use crate::error::NikaError;
use std::fs;
use std::path::PathBuf;

/// Template categories for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    /// Basic single-verb workflows
    Simple,
    /// Multi-step pipelines
    Pipeline,
    /// Agent-based workflows
    Agent,
    /// MCP integration examples
    Mcp,
    /// Multi-provider configurations
    Advanced,
}

impl TemplateCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Pipeline => "Pipeline",
            Self::Agent => "Agent",
            Self::Mcp => "MCP Integration",
            Self::Advanced => "Advanced",
        }
    }
}

/// Available workflow templates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// Basic LLM text generation
    SimpleInfer,
    /// Shell command execution
    SimpleExec,
    /// HTTP request example
    SimpleFetch,
    /// API data processing pipeline
    ApiPipeline,
    /// Blog content generation
    BlogGenerator,
    /// Code review assistant
    CodeReview,
    /// Research agent with web search
    AgentResearch,
    /// Browser automation agent
    AgentBrowser,
    /// MCP server integration example
    McpIntegration,
    /// Multi-provider workflow
    MultiProvider,
    /// Data pipeline with ETL pattern
    DataPipeline,
    /// Morning briefing digest
    MorningBriefing,
    /// Git changelog generator
    GitChangelog,
    /// Parallel translation workflow
    ParallelTranslation,
    /// QA testing agent
    AgentQaTester,
}

impl Template {
    /// All available templates
    pub const ALL: &'static [Template] = &[
        Template::SimpleInfer,
        Template::SimpleExec,
        Template::SimpleFetch,
        Template::ApiPipeline,
        Template::BlogGenerator,
        Template::CodeReview,
        Template::AgentResearch,
        Template::AgentBrowser,
        Template::McpIntegration,
        Template::MultiProvider,
        Template::DataPipeline,
        Template::MorningBriefing,
        Template::GitChangelog,
        Template::ParallelTranslation,
        Template::AgentQaTester,
    ];

    /// Template name (for CLI)
    pub fn name(&self) -> &'static str {
        match self {
            Self::SimpleInfer => "simple-infer",
            Self::SimpleExec => "simple-exec",
            Self::SimpleFetch => "simple-fetch",
            Self::ApiPipeline => "api-pipeline",
            Self::BlogGenerator => "blog-generator",
            Self::CodeReview => "code-review",
            Self::AgentResearch => "agent-research",
            Self::AgentBrowser => "agent-browser",
            Self::McpIntegration => "mcp-integration",
            Self::MultiProvider => "multi-provider",
            Self::DataPipeline => "data-pipeline",
            Self::MorningBriefing => "morning-briefing",
            Self::GitChangelog => "git-changelog",
            Self::ParallelTranslation => "parallel-translation",
            Self::AgentQaTester => "agent-qa-tester",
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::SimpleInfer => "Basic LLM text generation with infer verb",
            Self::SimpleExec => "Shell command execution with exec verb",
            Self::SimpleFetch => "HTTP request with fetch verb",
            Self::ApiPipeline => "Multi-step API data processing pipeline",
            Self::BlogGenerator => "Blog content generation with research and writing",
            Self::CodeReview => "Code review assistant with file analysis",
            Self::AgentResearch => "Research agent with MCP web search",
            Self::AgentBrowser => "Browser automation agent with Playwright",
            Self::McpIntegration => "MCP server integration example",
            Self::MultiProvider => "Multi-provider workflow (Claude + OpenAI)",
            Self::DataPipeline => "ETL data pipeline with fetch, transform, and load",
            Self::MorningBriefing => "Daily digest with news, weather, and calendar",
            Self::GitChangelog => "Git commit analysis and changelog generation",
            Self::ParallelTranslation => "Multi-language translation with for_each",
            Self::AgentQaTester => "QA testing agent with test generation",
        }
    }

    /// Template category
    pub fn category(&self) -> TemplateCategory {
        match self {
            Self::SimpleInfer | Self::SimpleExec | Self::SimpleFetch => TemplateCategory::Simple,
            Self::ApiPipeline
            | Self::BlogGenerator
            | Self::CodeReview
            | Self::DataPipeline
            | Self::MorningBriefing
            | Self::GitChangelog
            | Self::ParallelTranslation => TemplateCategory::Pipeline,
            Self::AgentResearch | Self::AgentBrowser | Self::AgentQaTester => {
                TemplateCategory::Agent
            }
            Self::McpIntegration => TemplateCategory::Mcp,
            Self::MultiProvider => TemplateCategory::Advanced,
        }
    }

    /// Parse template name from string
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().find(|t| t.name() == name).copied()
    }

    /// Get the template content
    pub fn content(&self, workflow_name: &str) -> String {
        templates::generate_template(*self, workflow_name)
    }
}

/// Primary verb for a workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verb {
    /// LLM text generation
    #[default]
    Infer,
    /// Shell command execution
    Exec,
    /// HTTP requests
    Fetch,
    /// MCP tool invocation
    Invoke,
    /// Multi-turn agentic loop
    Agent,
}

impl Verb {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Infer => "infer",
            Self::Exec => "exec",
            Self::Fetch => "fetch",
            Self::Invoke => "invoke",
            Self::Agent => "agent",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Infer => "LLM text generation (Claude, OpenAI, etc.)",
            Self::Exec => "Shell command execution",
            Self::Fetch => "HTTP requests (GET, POST, etc.)",
            Self::Invoke => "MCP tool invocation",
            Self::Agent => "Multi-turn agentic loop with tools",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "infer" => Some(Self::Infer),
            "exec" => Some(Self::Exec),
            "fetch" => Some(Self::Fetch),
            "invoke" => Some(Self::Invoke),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// LLM provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    #[default]
    Claude,
    OpenAI,
    Mistral,
    Groq,
    DeepSeek,
    Gemini,
    Native,
}

impl Provider {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::OpenAI => "openai",
            Self::Mistral => "mistral",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Gemini => "gemini",
            Self::Native => "native",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Claude => "claude-sonnet-4-6",
            Self::OpenAI => "gpt-4o",
            Self::Mistral => "mistral-large-latest",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::DeepSeek => "deepseek-chat",
            Self::Gemini => "gemini-2.0-flash",
            Self::Native => "llama3.2-1b-q4",
        }
    }

    pub fn env_var(&self) -> &'static str {
        // Delegate to core::find_provider for the canonical env var name
        crate::core::find_provider(self.name())
            .map(|p| p.env_var)
            .unwrap_or("ANTHROPIC_API_KEY")
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let provider = crate::core::find_provider(name)?;
        match provider.id {
            "anthropic" => Some(Self::Claude),
            "openai" => Some(Self::OpenAI),
            "mistral" => Some(Self::Mistral),
            "groq" => Some(Self::Groq),
            "deepseek" => Some(Self::DeepSeek),
            "gemini" => Some(Self::Gemini),
            "native" => Some(Self::Native),
            _ => None,
        }
    }
}

/// Output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Yaml,
    File,
}

impl OutputFormat {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::File => "file",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "yaml" => Some(Self::Yaml),
            "file" => Some(Self::File),
            _ => None,
        }
    }
}

/// Configuration for generating a new workflow
#[derive(Debug, Clone)]
pub struct NewWorkflowConfig {
    /// Workflow name (also used for filename)
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Primary verb
    pub verb: Verb,
    /// LLM provider
    pub provider: Provider,
    /// Model name (defaults to provider's default)
    pub model: Option<String>,
    /// Output format
    pub output_format: OutputFormat,
    /// Include MCP configuration
    pub with_mcp: bool,
    /// Include subworkflow example
    pub with_include: bool,
    /// Include artifact output configuration
    pub with_artifacts: bool,
    /// Output directory (defaults to current dir)
    pub output_dir: PathBuf,
}

impl Default for NewWorkflowConfig {
    fn default() -> Self {
        Self {
            name: "my-workflow".to_string(),
            description: None,
            verb: Verb::default(),
            provider: Provider::default(),
            model: None,
            output_format: OutputFormat::default(),
            with_mcp: false,
            with_include: false,
            with_artifacts: false,
            output_dir: PathBuf::from("."),
        }
    }
}

impl NewWorkflowConfig {
    /// Generate the workflow YAML content
    pub fn generate(&self) -> String {
        let model = self
            .model
            .as_deref()
            .unwrap_or_else(|| self.provider.default_model());

        let description = self
            .description
            .as_deref()
            .unwrap_or("Generated by nika new");

        let mut yaml = String::new();

        // Header comment
        yaml.push_str(&format!(
            "# {}\n#\n# Generated by `nika new`\n#\n",
            self.name
        ));
        yaml.push_str(&format!("# Usage:\n#   nika {}.nika.yaml\n#\n", self.name));
        yaml.push_str(&format!(
            "# Requirements:\n#   - {} environment variable\n\n",
            self.provider.env_var()
        ));

        // Schema and metadata
        yaml.push_str("schema: \"nika/workflow@0.12\"\n");
        yaml.push_str(&format!("workflow: {}\n", self.name));
        yaml.push_str(&format!("description: \"{}\"\n\n", description));

        // Provider and model
        yaml.push_str(&format!("provider: {}\n", self.provider.name()));
        yaml.push_str(&format!("model: {}\n\n", model));

        // Optional: artifacts configuration
        if self.with_artifacts {
            yaml.push_str("artifacts:\n");
            yaml.push_str("  dir: ./output/{{workflow_name}}\n");
            yaml.push_str("  format: json\n");
            yaml.push_str("  manifest: true\n\n");
        }

        // Optional: MCP configuration
        if self.with_mcp {
            yaml.push_str("mcp:\n");
            yaml.push_str("  perplexity:\n");
            yaml.push_str("    command: npx\n");
            yaml.push_str("    args: [\"-y\", \"@perplexity-ai/mcp-server\"]\n\n");
        }

        // Optional: include configuration
        if self.with_include {
            yaml.push_str("# Uncomment to include a subworkflow\n");
            yaml.push_str("# include:\n");
            yaml.push_str("#   - path: ./utils/common-tasks.nika.yaml\n");
            yaml.push_str("#     prefix: common\n\n");
        }

        // Tasks section
        yaml.push_str("tasks:\n");
        yaml.push_str(&self.generate_primary_task());

        yaml
    }

    /// Generate the primary task based on verb
    fn generate_primary_task(&self) -> String {
        match self.verb {
            Verb::Infer => self.generate_infer_task(),
            Verb::Exec => self.generate_exec_task(),
            Verb::Fetch => self.generate_fetch_task(),
            Verb::Invoke => self.generate_invoke_task(),
            Verb::Agent => self.generate_agent_task(),
        }
    }

    fn generate_infer_task(&self) -> String {
        let mut task = String::new();
        task.push_str("  - id: generate\n");
        task.push_str("    description: \"Generate text with LLM\"\n");
        task.push_str("    infer:\n");
        task.push_str("      prompt: |\n");
        task.push_str("        Write a brief summary about AI workflows.\n");
        task.push_str("        Be concise and informative.\n");
        task.push_str("    output:\n");
        task.push_str(&format!("      format: {}\n", self.output_format.name()));

        if self.output_format == OutputFormat::Json {
            task.push_str("      schema:\n");
            task.push_str("        type: object\n");
            task.push_str("        required: [summary]\n");
            task.push_str("        properties:\n");
            task.push_str("          summary:\n");
            task.push_str("            type: string\n");
        }

        task
    }

    fn generate_exec_task(&self) -> String {
        let mut task = String::new();
        task.push_str("  - id: run_command\n");
        task.push_str("    description: \"Execute shell command\"\n");
        task.push_str("    exec: |\n");
        task.push_str("      echo \"Hello from Nika!\"\n");
        task.push_str("      date\n");
        task.push_str("    output:\n");
        task.push_str("      format: text\n");
        task
    }

    fn generate_fetch_task(&self) -> String {
        let mut task = String::new();
        task.push_str("  - id: fetch_data\n");
        task.push_str("    description: \"Fetch data from API\"\n");
        task.push_str("    fetch:\n");
        task.push_str("      url: \"https://api.github.com/zen\"\n");
        task.push_str("      method: GET\n");
        task.push_str("      headers:\n");
        task.push_str("        Accept: application/json\n");
        task.push_str("    output:\n");
        task.push_str("      format: text\n");
        task
    }

    fn generate_invoke_task(&self) -> String {
        let mut task = String::new();
        task.push_str("  - id: invoke_tool\n");
        task.push_str("    description: \"Invoke MCP tool\"\n");

        if self.with_mcp {
            task.push_str("    invoke:\n");
            task.push_str("      server: perplexity\n");
            task.push_str("      tool: perplexity_search\n");
            task.push_str("      params:\n");
            task.push_str("        query: \"latest AI news\"\n");
        } else {
            task.push_str("    # NOTE: Add MCP configuration above or use --with-mcp flag\n");
            task.push_str("    invoke:\n");
            task.push_str("      server: your-server\n");
            task.push_str("      tool: your-tool\n");
            task.push_str("      params:\n");
            task.push_str("        key: value\n");
        }

        task.push_str("    output:\n");
        task.push_str("      format: json\n");
        task
    }

    fn generate_agent_task(&self) -> String {
        let mut task = String::new();
        task.push_str("  - id: agent_task\n");
        task.push_str("    description: \"Multi-turn agent loop\"\n");
        task.push_str("    agent:\n");
        task.push_str("      prompt: |\n");
        task.push_str("        You are a helpful assistant.\n");
        task.push_str("        Answer the user's question concisely.\n\n");
        task.push_str("        Question: What are the benefits of workflow automation?\n");
        task.push_str("      max_turns: 5\n");

        if self.with_mcp {
            task.push_str("      mcp: [perplexity]\n");
        }

        task.push_str("    output:\n");
        task.push_str(&format!("      format: {}\n", self.output_format.name()));

        if self.output_format == OutputFormat::Json {
            task.push_str("      schema:\n");
            task.push_str("        type: object\n");
            task.push_str("        required: [answer]\n");
            task.push_str("        properties:\n");
            task.push_str("          answer:\n");
            task.push_str("            type: string\n");
        }

        task
    }

    /// Write the workflow to a file
    pub fn write(&self) -> Result<PathBuf, NikaError> {
        let filename = format!("{}.nika.yaml", self.name);
        let path = self.output_dir.join(&filename);

        // Check if file already exists
        if path.exists() {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "File already exists: {}. Use a different name or delete the existing file.",
                    path.display()
                ),
            });
        }

        // Generate and write content
        let content = self.generate();
        fs::write(&path, content)?;

        Ok(path)
    }
}

/// Create a new workflow from a template
pub fn create_from_template(
    name: &str,
    template: Template,
    output_dir: &std::path::Path,
) -> Result<PathBuf, NikaError> {
    let filename = format!("{}.nika.yaml", name);
    let path = output_dir.join(&filename);

    // Check if file already exists
    if path.exists() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "File already exists: {}. Use a different name or delete the existing file.",
                path.display()
            ),
        });
    }

    // Generate content from template
    let content = template.content(name);
    fs::write(&path, content)?;

    Ok(path)
}

/// List all available templates with descriptions
pub fn list_templates() -> Vec<(&'static str, &'static str, &'static str)> {
    Template::ALL
        .iter()
        .map(|t| (t.name(), t.description(), t.category().display_name()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_template_names() {
        assert_eq!(Template::SimpleInfer.name(), "simple-infer");
        assert_eq!(Template::BlogGenerator.name(), "blog-generator");
    }

    #[test]
    fn test_template_from_name() {
        assert_eq!(
            Template::from_name("simple-infer"),
            Some(Template::SimpleInfer)
        );
        assert_eq!(
            Template::from_name("blog-generator"),
            Some(Template::BlogGenerator)
        );
        assert_eq!(Template::from_name("invalid"), None);
    }

    #[test]
    fn test_verb_from_name() {
        assert_eq!(Verb::from_name("infer"), Some(Verb::Infer));
        assert_eq!(Verb::from_name("EXEC"), Some(Verb::Exec));
        assert_eq!(Verb::from_name("Agent"), Some(Verb::Agent));
        assert_eq!(Verb::from_name("invalid"), None);
    }

    #[test]
    fn test_provider_from_name() {
        assert_eq!(Provider::from_name("claude"), Some(Provider::Claude));
        assert_eq!(Provider::from_name("anthropic"), Some(Provider::Claude));
        assert_eq!(Provider::from_name("GPT"), Some(Provider::OpenAI));
        assert_eq!(Provider::from_name("invalid"), None);
    }

    #[test]
    fn test_default_config() {
        let config = NewWorkflowConfig::default();
        assert_eq!(config.name, "my-workflow");
        assert_eq!(config.verb, Verb::Infer);
        assert_eq!(config.provider, Provider::Claude);
        assert!(!config.with_mcp);
    }

    #[test]
    fn test_generate_basic_workflow() {
        let config = NewWorkflowConfig::default();
        let yaml = config.generate();

        assert!(yaml.contains("schema: \"nika/workflow@0.12\""));
        assert!(yaml.contains("workflow: my-workflow"));
        assert!(yaml.contains("provider: claude"));
        assert!(yaml.contains("infer:"));
    }

    #[test]
    fn test_generate_with_mcp() {
        let config = NewWorkflowConfig {
            with_mcp: true,
            ..Default::default()
        };
        let yaml = config.generate();

        assert!(yaml.contains("mcp:"));
        assert!(yaml.contains("perplexity:"));
    }

    #[test]
    fn test_generate_with_artifacts() {
        let config = NewWorkflowConfig {
            with_artifacts: true,
            ..Default::default()
        };
        let yaml = config.generate();

        assert!(yaml.contains("artifacts:"));
        assert!(yaml.contains("dir:"));
    }

    #[test]
    fn test_generate_exec_task() {
        let config = NewWorkflowConfig {
            verb: Verb::Exec,
            ..Default::default()
        };
        let yaml = config.generate();

        assert!(yaml.contains("exec: |"));
        assert!(yaml.contains("echo \"Hello from Nika!\""));
    }

    #[test]
    fn test_generate_fetch_task() {
        let config = NewWorkflowConfig {
            verb: Verb::Fetch,
            ..Default::default()
        };
        let yaml = config.generate();

        assert!(yaml.contains("fetch:"));
        assert!(yaml.contains("url:"));
        assert!(yaml.contains("method: GET"));
    }

    #[test]
    fn test_generate_agent_task() {
        let config = NewWorkflowConfig {
            verb: Verb::Agent,
            ..Default::default()
        };
        let yaml = config.generate();

        assert!(yaml.contains("agent:"));
        assert!(yaml.contains("max_turns:"));
    }

    #[test]
    fn test_generate_json_output() {
        let config = NewWorkflowConfig {
            output_format: OutputFormat::Json,
            ..Default::default()
        };
        let yaml = config.generate();

        assert!(yaml.contains("format: json"));
        assert!(yaml.contains("schema:"));
    }

    #[test]
    fn test_write_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let config = NewWorkflowConfig {
            name: "test-workflow".to_string(),
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let path = config.write().unwrap();
        assert!(path.exists());
        assert!(path.ends_with("test-workflow.nika.yaml"));

        // Verify content
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("workflow: test-workflow"));
    }

    #[test]
    fn test_write_workflow_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let config = NewWorkflowConfig {
            name: "existing".to_string(),
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        // Create file first
        let path = temp_dir.path().join("existing.nika.yaml");
        std::fs::write(&path, "existing content").unwrap();

        // Should fail
        let result = config.write();
        assert!(result.is_err());
    }

    #[test]
    fn test_create_from_template() {
        let temp_dir = TempDir::new().unwrap();
        let path =
            create_from_template("my-simple", Template::SimpleInfer, temp_dir.path()).unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("workflow: my-simple"));
    }

    #[test]
    fn test_list_templates() {
        let templates = list_templates();
        assert!(templates.len() >= 10);

        // Check format
        for (name, description, category) in &templates {
            assert!(!name.is_empty());
            assert!(!description.is_empty());
            assert!(!category.is_empty());
        }
    }

    #[test]
    fn test_all_templates_generate() {
        for template in Template::ALL {
            let content = template.content("test-workflow");
            assert!(content.contains("schema: \"nika/workflow@0.12\""));
            assert!(content.contains("workflow: test-workflow"));
        }
    }
}
