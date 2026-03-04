//! Workflow templates for `nika new`
//!
//! Contains embedded templates for common workflow patterns.
//! Each template is a function that generates YAML content.

use super::Template;

/// Generate template content based on template type and workflow name
pub fn generate_template(template: Template, workflow_name: &str) -> String {
    match template {
        Template::SimpleInfer => simple_infer(workflow_name),
        Template::SimpleExec => simple_exec(workflow_name),
        Template::SimpleFetch => simple_fetch(workflow_name),
        Template::ApiPipeline => api_pipeline(workflow_name),
        Template::BlogGenerator => blog_generator(workflow_name),
        Template::CodeReview => code_review(workflow_name),
        Template::AgentResearch => agent_research(workflow_name),
        Template::AgentBrowser => agent_browser(workflow_name),
        Template::McpIntegration => mcp_integration(workflow_name),
        Template::MultiProvider => multi_provider(workflow_name),
    }
}

/// Simple infer template - basic LLM text generation
fn simple_infer(name: &str) -> String {
    format!(
        r#"# {name}
#
# Simple LLM text generation example.
# Demonstrates the infer verb with basic configuration.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable

schema: "nika/workflow@0.10"
workflow: {name}
description: "Simple text generation with LLM"

provider: claude
model: claude-sonnet-4-6

tasks:
  - id: generate
    description: "Generate creative text"
    infer:
      prompt: |
        Write a short, creative paragraph about technology.
        Be engaging and informative.
    output:
      format: text

  - id: display
    description: "Display the result"
    use:
      content: generate
    shell: |
      echo "Generated content:"
      echo "=================="
      echo "{{{{use.content}}}}"
    flow: [generate]
"#
    )
}

/// Simple shell template - shell command invocation
fn simple_exec(name: &str) -> String {
    format!(
        r#"# {name}
#
# Shell command invocation example.
# Demonstrates the exec verb with shell commands.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - None (no API keys needed)

schema: "nika/workflow@0.10"
workflow: {name}
description: "Shell command invocation workflow"

tasks:
  - id: system_info
    description: "Get system information"
    exec:
      command: |
        echo "=== System Information ==="
        echo "Date: $(date)"
        echo "User: $USER"
        echo "PWD: $PWD"
      shell: true
    output:
      format: text

  - id: list_files
    description: "List current directory"
    exec:
      command: "ls -la"
      shell: true
    output:
      format: text

  - id: summary
    description: "Create summary"
    use:
      info: system_info
      files: list_files
    exec:
      command: |
        echo "=== Workflow Complete ==="
        echo "System info collected"
        echo "Files listed"
      shell: true
    flow: [system_info, list_files]
"#
    )
}

/// Simple fetch template - HTTP requests
fn simple_fetch(name: &str) -> String {
    format!(
        r#"# {name}
#
# HTTP request example.
# Demonstrates the fetch verb with different methods.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - Internet connection

schema: "nika/workflow@0.10"
workflow: {name}
description: "HTTP request workflow"

tasks:
  - id: get_zen
    description: "Get GitHub zen quote"
    fetch:
      url: "https://api.github.com/zen"
      method: GET
      headers:
        Accept: text/plain
    output:
      format: text

  - id: get_user
    description: "Get GitHub user info"
    fetch:
      url: "https://api.github.com/users/octocat"
      method: GET
      headers:
        Accept: application/json
    output:
      format: json

  - id: display
    description: "Display results"
    use:
      zen: get_zen
      user: get_user
    shell: |
      echo "GitHub Zen: {{{{use.zen}}}}"
      echo ""
      echo "User: {{{{use.user.login}}}}"
      echo "Name: {{{{use.user.name}}}}"
    flow: [get_zen, get_user]
"#
    )
}

/// API pipeline template - multi-step data processing
fn api_pipeline(name: &str) -> String {
    format!(
        r#"# {name}
#
# Multi-step API data processing pipeline.
# Fetches data, transforms it with LLM, and saves results.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable
#   - Internet connection

schema: "nika/workflow@0.10"
workflow: {name}
description: "API data processing pipeline"

provider: claude
model: claude-sonnet-4-6

artifacts:
  dir: ./output/{{{{workflow_name}}}}
  format: json
  manifest: true

tasks:
  - id: fetch_data
    description: "Fetch data from API"
    fetch:
      url: "https://jsonplaceholder.typicode.com/posts?_limit=5"
      method: GET
      headers:
        Accept: application/json
    output:
      format: json

  - id: analyze
    description: "Analyze fetched data"
    use:
      posts: fetch_data
    infer:
      prompt: |
        Analyze these blog posts and provide a summary:

        {{{{use.posts | to_yaml}}}}

        Return JSON with:
        - total_posts: number of posts
        - avg_title_length: average title length
        - topics: list of main topics covered
    output:
      format: json
      schema:
        type: object
        required: [total_posts, topics]
        properties:
          total_posts:
            type: integer
          avg_title_length:
            type: number
          topics:
            type: array
            items:
              type: string
    artifact:
      path: analysis.json
    flow: [fetch_data]

  - id: generate_report
    description: "Generate markdown report"
    use:
      analysis: analyze
    infer:
      prompt: |
        Create a brief markdown report based on this analysis:

        {{{{use.analysis | to_yaml}}}}

        Include:
        - Summary statistics
        - Key topics identified
        - Recommendations
    output:
      format: text
    artifact:
      path: report.md
      format: text
    flow: [analyze]
"#
    )
}

/// Blog generator template - content generation pipeline
fn blog_generator(name: &str) -> String {
    format!(
        r#"# {name}
#
# Blog content generation pipeline.
# Researches topics, creates outline, writes article.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable

schema: "nika/workflow@0.10"
workflow: {name}
description: "Blog content generation pipeline"

provider: claude
model: claude-sonnet-4-6

inputs:
  topic:
    type: string
    description: "Blog topic to write about"
    default: "artificial intelligence in 2026"
  word_count:
    type: integer
    description: "Target word count"
    default: 1500

artifacts:
  dir: ./content/{{{{date}}}}/{{{{workflow_name}}}}
  format: json
  manifest: true

tasks:
  - id: research
    description: "Research the topic"
    infer:
      prompt: |
        Research the topic: "{{{{inputs.topic}}}}"

        Provide:
        1. Key points to cover
        2. Current trends
        3. Expert opinions
        4. Data and statistics

        Return as structured JSON.
    output:
      format: json
      schema:
        type: object
        required: [key_points, trends]
        properties:
          key_points:
            type: array
            items:
              type: string
          trends:
            type: array
            items:
              type: string
          statistics:
            type: array
            items:
              type: string
    artifact:
      path: research.json

  - id: outline
    description: "Create article outline"
    use:
      research: research
    infer:
      prompt: |
        Based on this research, create a detailed blog post outline:

        {{{{use.research | to_yaml}}}}

        Create an outline with:
        - Compelling title
        - Introduction hook
        - 4-5 main sections with subsections
        - Conclusion with CTA

        Target word count: {{{{inputs.word_count}}}}
    output:
      format: text
    artifact:
      path: outline.md
      format: text
    flow: [research]

  - id: write
    description: "Write the article"
    use:
      research: research
      outline: outline
    infer:
      prompt: |
        Write a complete blog post following this outline:

        {{{{use.outline}}}}

        Research context:
        {{{{use.research | to_yaml}}}}

        Requirements:
        - Engaging, professional tone
        - Target: {{{{inputs.word_count}}}} words
        - Include practical examples
        - End with clear CTA

        Output as markdown.
    output:
      format: text
    artifact:
      path: article.md
      format: text
    flow: [outline]

  - id: metadata
    description: "Generate SEO metadata"
    use:
      article: write
    infer:
      prompt: |
        Generate SEO metadata for this article:

        {{{{use.article | truncate: 500}}}}

        Return JSON with:
        - seo_title (55-60 chars)
        - meta_description (155-160 chars)
        - keywords (5-7 terms)
        - og_description
    output:
      format: json
      schema:
        type: object
        required: [seo_title, meta_description, keywords]
        properties:
          seo_title:
            type: string
          meta_description:
            type: string
          keywords:
            type: array
            items:
              type: string
          og_description:
            type: string
    artifact:
      path: metadata.json
    flow: [write]
"#
    )
}

/// Code review template - code analysis assistant
fn code_review(name: &str) -> String {
    format!(
        r#"# {name}
#
# Code review assistant workflow.
# Analyzes code for issues, suggests improvements.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable

schema: "nika/workflow@0.10"
workflow: {name}
description: "Code review assistant"

provider: claude
model: claude-sonnet-4-6

inputs:
  file_path:
    type: string
    description: "Path to file to review"
    default: "src/main.rs"

tasks:
  - id: read_code
    description: "Read the source file"
    shell: |
      if [ -f "{{{{inputs.file_path}}}}" ]; then
        cat "{{{{inputs.file_path}}}}"
      else
        echo "File not found: {{{{inputs.file_path}}}}"
        exit 1
      fi
    output:
      format: text

  - id: analyze
    description: "Analyze code quality"
    use:
      code: read_code
    infer:
      prompt: |
        Perform a comprehensive code review on this code:

        ```
        {{{{use.code}}}}
        ```

        Analyze for:
        1. Code quality and best practices
        2. Potential bugs or issues
        3. Performance concerns
        4. Security vulnerabilities
        5. Readability and maintainability

        Return JSON with categorized findings.
    output:
      format: json
      schema:
        type: object
        required: [quality_score, issues, suggestions]
        properties:
          quality_score:
            type: integer
            minimum: 0
            maximum: 100
          issues:
            type: array
            items:
              type: object
              properties:
                severity:
                  type: string
                  enum: [critical, high, medium, low]
                line:
                  type: integer
                description:
                  type: string
          suggestions:
            type: array
            items:
              type: string
          security_concerns:
            type: array
            items:
              type: string
    flow: [read_code]

  - id: report
    description: "Generate review report"
    use:
      analysis: analyze
      code: read_code
    infer:
      prompt: |
        Create a markdown code review report based on:

        Analysis:
        {{{{use.analysis | to_yaml}}}}

        Include:
        - Overall summary
        - Quality score with explanation
        - Detailed findings by severity
        - Specific recommendations with code examples
        - Action items
    output:
      format: text
    flow: [analyze]
"#
    )
}

/// Agent research template - research agent with MCP
fn agent_research(name: &str) -> String {
    format!(
        r#"# {name}
#
# Research agent with web search capabilities.
# Uses MCP servers for real-time web research.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable
#   - PERPLEXITY_API_KEY environment variable

schema: "nika/workflow@0.10"
workflow: {name}
description: "Research agent with MCP web search"

provider: claude
model: claude-sonnet-4-6

mcp:
  perplexity:
    command: npx
    args: ["-y", "@perplexity-ai/mcp-server"]

inputs:
  research_topic:
    type: string
    description: "Topic to research"
    default: "latest developments in AI agents"

tasks:
  - id: research
    description: "Conduct web research"
    agent:
      prompt: |
        You are a thorough research agent.

        Research the topic: "{{{{inputs.research_topic}}}}"

        Use perplexity_search_web to:
        1. Find the latest news and developments
        2. Identify key players and companies
        3. Gather statistics and data
        4. Find expert opinions

        Compile a comprehensive research report.
        End with "RESEARCH_COMPLETE" when done.
      mcp: [perplexity]
      max_turns: 10
    output:
      format: json
      schema:
        type: object
        required: [topic, findings, sources]
        properties:
          topic:
            type: string
          findings:
            type: array
            items:
              type: object
              properties:
                category:
                  type: string
                content:
                  type: string
          key_players:
            type: array
            items:
              type: string
          statistics:
            type: array
            items:
              type: string
          sources:
            type: array
            items:
              type: string

  - id: synthesize
    description: "Synthesize findings into report"
    use:
      research: research
    infer:
      prompt: |
        Create a summary from this research:

        {{{{use.research | to_yaml}}}}

        Format as a professional brief with:
        - Summary (3-5 sentences)
        - Key findings
        - Implications
        - Recommendations
    output:
      format: text
    flow: [research]
"#
    )
}

/// Agent browser template - browser automation
fn agent_browser(name: &str) -> String {
    format!(
        r#"# {name}
#
# Browser automation agent.
# Uses Playwright MCP for web automation.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable
#   - Playwright MCP server

schema: "nika/workflow@0.10"
workflow: {name}
description: "Browser automation agent"

provider: claude
model: claude-sonnet-4-6

mcp:
  playwright:
    command: npx
    args: ["-y", "@anthropic/mcp-server-playwright"]

inputs:
  target_url:
    type: string
    description: "URL to automate"
    default: "https://example.com"

tasks:
  - id: browse
    description: "Automate browser interaction"
    agent:
      prompt: |
        You are a browser automation agent.

        Navigate to: {{{{inputs.target_url}}}}

        Tasks:
        1. Open the page
        2. Wait for content to load
        3. Extract the main heading and key content
        4. Take a screenshot
        5. Report what you found

        Use the playwright tools to interact with the browser.
        End with "AUTOMATION_COMPLETE" when done.
      mcp: [playwright]
      max_turns: 15
    output:
      format: json
      schema:
        type: object
        required: [url, title, content_summary]
        properties:
          url:
            type: string
          title:
            type: string
          content_summary:
            type: string
          links_found:
            type: integer
          screenshot_path:
            type: string

  - id: report
    description: "Generate automation report"
    use:
      result: browse
    shell: |
      echo "=== Browser Automation Report ==="
      echo "URL: {{{{use.result.url}}}}"
      echo "Title: {{{{use.result.title}}}}"
      echo ""
      echo "Summary:"
      echo "{{{{use.result.content_summary}}}}"
    flow: [browse]
"#
    )
}

/// MCP integration template - MCP server usage
fn mcp_integration(name: &str) -> String {
    format!(
        r#"# {name}
#
# MCP server integration example.
# Demonstrates using multiple MCP servers.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable
#   - FIRECRAWL_API_KEY environment variable

schema: "nika/workflow@0.10"
workflow: {name}
description: "MCP server integration example"

provider: claude
model: claude-sonnet-4-6

mcp:
  firecrawl:
    command: npx
    args: ["-y", "firecrawl-mcp"]

  filesystem:
    command: npx
    args: ["-y", "@anthropic/mcp-server-filesystem", "--root", "."]

tasks:
  - id: scrape_page
    description: "Scrape a webpage"
    invoke:
      server: firecrawl
      tool: firecrawl_scrape
      params:
        url: "https://news.ycombinator.com"
        formats: ["markdown"]
        onlyMainContent: true
    output:
      format: json

  - id: analyze_content
    description: "Analyze scraped content"
    use:
      page: scrape_page
    infer:
      prompt: |
        Analyze this webpage content:

        {{{{use.page.markdown | truncate: 2000}}}}

        Extract:
        - Top 5 story headlines
        - Main topics covered
        - Content sentiment
    output:
      format: json
      schema:
        type: object
        required: [headlines, topics]
        properties:
          headlines:
            type: array
            items:
              type: string
            maxItems: 5
          topics:
            type: array
            items:
              type: string
          sentiment:
            type: string
            enum: [positive, neutral, negative]
    flow: [scrape_page]

  - id: save_analysis
    description: "Save analysis to file"
    use:
      analysis: analyze_content
    invoke:
      server: filesystem
      tool: write_file
      params:
        path: "analysis-output.json"
        content: "{{{{use.analysis | to_json}}}}"
    flow: [analyze_content]
"#
    )
}

/// Multi-provider template - using multiple LLM providers
fn multi_provider(name: &str) -> String {
    format!(
        r#"# {name}
#
# Multi-provider workflow example.
# Uses different LLM providers for different tasks.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable
#   - OPENAI_API_KEY environment variable

schema: "nika/workflow@0.10"
workflow: {name}
description: "Multi-provider LLM workflow"

# Default provider (can be overridden per task)
provider: claude
model: claude-sonnet-4-6

inputs:
  question:
    type: string
    description: "Question to ask multiple providers"
    default: "What are the key trends in AI for 2026?"

tasks:
  - id: claude_response
    description: "Get response from Claude"
    provider: claude
    model: claude-sonnet-4-6
    infer:
      prompt: |
        Answer this question concisely:
        {{{{inputs.question}}}}
    output:
      format: text

  - id: openai_response
    description: "Get response from OpenAI"
    provider: openai
    model: gpt-4o
    infer:
      prompt: |
        Answer this question concisely:
        {{{{inputs.question}}}}
    output:
      format: text

  - id: compare
    description: "Compare responses"
    use:
      claude: claude_response
      openai: openai_response
    infer:
      prompt: |
        Compare these two AI responses:

        Claude's response:
        {{{{use.claude}}}}

        OpenAI's response:
        {{{{use.openai}}}}

        Analyze:
        1. Key similarities
        2. Key differences
        3. Which response is more comprehensive
        4. Synthesis of best points from both
    output:
      format: json
      schema:
        type: object
        required: [similarities, differences, recommendation]
        properties:
          similarities:
            type: array
            items:
              type: string
          differences:
            type: array
            items:
              type: string
          more_comprehensive:
            type: string
            enum: [claude, openai, both]
          recommendation:
            type: string
    flow: [claude_response, openai_response]

  - id: final_answer
    description: "Generate synthesized answer"
    use:
      comparison: compare
      claude: claude_response
      openai: openai_response
    infer:
      prompt: |
        Create the best possible answer by synthesizing insights from both AI responses:

        Comparison analysis:
        {{{{use.comparison | to_yaml}}}}

        Original question: {{{{inputs.question}}}}

        Provide a comprehensive, well-structured answer.
    output:
      format: text
    flow: [compare]
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_infer_template() {
        let content = simple_infer("test-workflow");
        assert!(content.contains("workflow: test-workflow"));
        assert!(content.contains("infer:"));
        assert!(content.contains("provider: claude"));
    }

    #[test]
    fn test_simple_exec_template() {
        let content = simple_exec("exec-test");
        assert!(content.contains("workflow: exec-test"));
        assert!(content.contains("exec:"));
        assert!(content.contains("shell: true"));
    }

    #[test]
    fn test_simple_fetch_template() {
        let content = simple_fetch("fetch-test");
        assert!(content.contains("workflow: fetch-test"));
        assert!(content.contains("fetch:"));
        assert!(content.contains("method: GET"));
    }

    #[test]
    fn test_api_pipeline_template() {
        let content = api_pipeline("pipeline-test");
        assert!(content.contains("workflow: pipeline-test"));
        assert!(content.contains("artifacts:"));
        assert!(content.contains("fetch:"));
        assert!(content.contains("infer:"));
    }

    #[test]
    fn test_blog_generator_template() {
        let content = blog_generator("blog-test");
        assert!(content.contains("workflow: blog-test"));
        assert!(content.contains("inputs:"));
        assert!(content.contains("research"));
        assert!(content.contains("outline"));
        assert!(content.contains("write"));
    }

    #[test]
    fn test_code_review_template() {
        let content = code_review("review-test");
        assert!(content.contains("workflow: review-test"));
        assert!(content.contains("quality_score"));
        assert!(content.contains("issues"));
    }

    #[test]
    fn test_agent_research_template() {
        let content = agent_research("research-test");
        assert!(content.contains("workflow: research-test"));
        assert!(content.contains("agent:"));
        assert!(content.contains("mcp:"));
        assert!(content.contains("perplexity"));
    }

    #[test]
    fn test_agent_browser_template() {
        let content = agent_browser("browser-test");
        assert!(content.contains("workflow: browser-test"));
        assert!(content.contains("playwright"));
        assert!(content.contains("agent:"));
    }

    #[test]
    fn test_mcp_integration_template() {
        let content = mcp_integration("mcp-test");
        assert!(content.contains("workflow: mcp-test"));
        assert!(content.contains("invoke:"));
        assert!(content.contains("firecrawl"));
        assert!(content.contains("filesystem"));
    }

    #[test]
    fn test_multi_provider_template() {
        let content = multi_provider("multi-test");
        assert!(content.contains("workflow: multi-test"));
        assert!(content.contains("provider: claude"));
        assert!(content.contains("provider: openai"));
        assert!(content.contains("compare"));
    }

    #[test]
    fn test_all_templates_valid_yaml() {
        for template in Template::ALL {
            let content = generate_template(*template, "test");
            // Basic YAML validation - should contain schema
            assert!(
                content.contains("schema: \"nika/workflow@0.10\""),
                "Template {} missing schema",
                template.name()
            );
        }
    }
}
