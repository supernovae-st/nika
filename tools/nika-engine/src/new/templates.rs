// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
        Template::DataPipeline => data_pipeline(workflow_name),
        Template::MorningBriefing => morning_briefing(workflow_name),
        Template::GitChangelog => git_changelog(workflow_name),
        Template::ParallelTranslation => parallel_translation(workflow_name),
        Template::AgentQaTester => agent_qa_tester(workflow_name),
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

schema: "nika/workflow@0.12"
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
    with:
      content: $generate
    exec:
      command: |
        echo "Generated content:"
        echo "=================="
        echo "{{{{with.content}}}}"
      shell: true
    depends_on: [generate]
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

schema: "nika/workflow@0.12"
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
    with:
      info: $system_info
      files: $list_files
    exec:
      command: |
        echo "=== Workflow Complete ==="
        echo "System info collected"
        echo "Files listed"
      shell: true
    depends_on: [system_info, list_files]
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

schema: "nika/workflow@0.12"
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
    with:
      zen: $get_zen
      user: $get_user
    exec:
      command: |
        echo "GitHub Zen: {{{{with.zen}}}}"
        echo ""
        echo "User: {{{{with.user.login}}}}"
        echo "Name: {{{{with.user.name}}}}"
      shell: true
    depends_on: [get_zen, get_user]
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

schema: "nika/workflow@0.12"
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
    with:
      posts: $fetch_data
    infer:
      prompt: |
        Analyze these blog posts and provide a summary:

        {{{{with.posts | to_json}}}}

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
    depends_on: [fetch_data]

  - id: generate_report
    description: "Generate markdown report"
    with:
      analysis: $analyze
    infer:
      prompt: |
        Create a brief markdown report based on this analysis:

        {{{{with.analysis | to_json}}}}

        Include:
        - Summary statistics
        - Key topics identified
        - Recommendations
    output:
      format: text
    artifact:
      path: report.md
      format: text
    depends_on: [analyze]
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

schema: "nika/workflow@0.12"
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
    with:
      research: $research
    infer:
      prompt: |
        Based on this research, create a detailed blog post outline:

        {{{{with.research | to_json}}}}

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
    depends_on: [research]

  - id: write
    description: "Write the article"
    with:
      research: $research
      outline: $outline
    infer:
      prompt: |
        Write a complete blog post following this outline:

        {{{{with.outline}}}}

        Research context:
        {{{{with.research | to_json}}}}

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
    depends_on: [outline]

  - id: metadata
    description: "Generate SEO metadata"
    with:
      article: $write
    infer:
      prompt: |
        Generate SEO metadata for this article:

        {{{{with.article | truncate: 500}}}}

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
    depends_on: [write]
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

schema: "nika/workflow@0.12"
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
    exec:
      command: |
        if [ -f "{{{{inputs.file_path}}}}" ]; then
          cat "{{{{inputs.file_path}}}}"
        else
          echo "File not found: {{{{inputs.file_path}}}}"
          exit 1
        fi
      shell: true
    output:
      format: text

  - id: analyze
    description: "Analyze code quality"
    with:
      code: $read_code
    infer:
      prompt: |
        Perform a comprehensive code review on this code:

        ```
        {{{{with.code}}}}
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
    depends_on: [read_code]

  - id: report
    description: "Generate review report"
    with:
      analysis: $analyze
      code: $read_code
    infer:
      prompt: |
        Create a markdown code review report based on:

        Analysis:
        {{{{with.analysis | to_json}}}}

        Include:
        - Overall summary
        - Quality score with explanation
        - Detailed findings by severity
        - Specific recommendations with code examples
        - Action items
    output:
      format: text
    depends_on: [analyze]
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

schema: "nika/workflow@0.12"
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

        Use perplexity_search to:
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
    with:
      research: $research
    infer:
      prompt: |
        Create a summary from this research:

        {{{{with.research | to_json}}}}

        Format as a professional brief with:
        - Summary (3-5 sentences)
        - Key findings
        - Implications
        - Recommendations
    output:
      format: text
    depends_on: [research]
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

schema: "nika/workflow@0.12"
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
    with:
      result: $browse
    exec:
      command: |
        echo "=== Browser Automation Report ==="
        echo "URL: {{{{with.result.url}}}}"
        echo "Title: {{{{with.result.title}}}}"
        echo ""
        echo "Summary:"
        echo "{{{{with.result.content_summary}}}}"
      shell: true
    depends_on: [browse]
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

schema: "nika/workflow@0.12"
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
    with:
      page: $scrape_page
    infer:
      prompt: |
        Analyze this webpage content:

        {{{{with.page.markdown | truncate: 2000}}}}

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
    depends_on: [scrape_page]

  - id: save_analysis
    description: "Save analysis to file"
    with:
      analysis: $analyze_content
    invoke:
      server: filesystem
      tool: write_file
      params:
        path: "analysis-output.json"
        content: "{{{{with.analysis | to_json}}}}"
    depends_on: [analyze_content]
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

schema: "nika/workflow@0.12"
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
    with:
      claude: $claude_response
      openai: $openai_response
    infer:
      prompt: |
        Compare these two AI responses:

        Claude's response:
        {{{{with.claude}}}}

        OpenAI's response:
        {{{{with.openai}}}}

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
    depends_on: [claude_response, openai_response]

  - id: final_answer
    description: "Generate synthesized answer"
    with:
      comparison: $compare
      claude: $claude_response
      openai: $openai_response
    infer:
      prompt: |
        Create the best possible answer by synthesizing insights from both AI responses:

        Comparison analysis:
        {{{{with.comparison | to_json}}}}

        Original question: {{{{inputs.question}}}}

        Provide a comprehensive, well-structured answer.
    output:
      format: text
    depends_on: [compare]
"#
    )
}

/// Data pipeline template - ETL pattern
fn data_pipeline(name: &str) -> String {
    format!(
        r#"# {name}
#
# ETL data pipeline workflow.
# Extracts data from API, transforms with LLM, loads to output.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable
#   - Internet connection

schema: "nika/workflow@0.12"
workflow: {name}
description: "ETL data pipeline with fetch, transform, and load"

provider: claude
model: claude-sonnet-4-6

inputs:
  source_url:
    type: string
    description: "Data source URL"
    default: "https://jsonplaceholder.typicode.com/posts?_limit=10"
  output_format:
    type: string
    description: "Output format (json/csv)"
    default: "json"

artifacts:
  dir: ./data/{{{{date}}}}/{{{{workflow_name}}}}
  format: json
  manifest: true

tasks:
  # Extract: Fetch raw data
  - id: extract
    description: "Extract data from source"
    fetch:
      url: "{{{{inputs.source_url}}}}"
      method: GET
      headers:
        Accept: application/json
    output:
      format: json

  # Transform: Clean and enrich data
  - id: transform
    description: "Transform and enrich data"
    with:
      raw_data: $extract
    infer:
      prompt: |
        Transform this raw data into a clean, enriched format:

        Raw Data:
        {{{{with.raw_data | to_json}}}}

        Requirements:
        1. Clean any malformed entries
        2. Add a "processed_at" timestamp
        3. Calculate summary statistics
        4. Categorize items if applicable

        Return as JSON with:
        - records: cleaned data array
        - stats: summary statistics object
        - metadata: processing info
    output:
      format: json
      schema:
        type: object
        required: [records, stats]
        properties:
          records:
            type: array
          stats:
            type: object
            properties:
              total_records:
                type: integer
              categories:
                type: object
          metadata:
            type: object
    artifact:
      path: transformed.json
    depends_on: [extract]

  # Load: Save processed data
  - id: load
    description: "Load data to output"
    with:
      data: $transform
    exec:
      command: |
        echo "Data pipeline complete!"
        echo "Records processed: {{{{with.data.stats.total_records}}}}"
        echo "Output saved to artifacts directory"
      shell: true
    depends_on: [transform]
"#
    )
}

/// Morning briefing template - daily digest
fn morning_briefing(name: &str) -> String {
    format!(
        r#"# {name}
#
# Morning briefing workflow.
# Generates a daily digest with news, weather, and tasks.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable

schema: "nika/workflow@0.12"
workflow: {name}
description: "Daily digest with news, weather, and tasks"

provider: claude
model: claude-sonnet-4-6

inputs:
  location:
    type: string
    description: "Location for weather"
    default: "San Francisco, CA"
  interests:
    type: array
    description: "Topics of interest"
    default: ["technology", "AI", "startups"]

artifacts:
  dir: ./briefings/{{{{date}}}}
  format: json
  manifest: true

tasks:
  # Get current date/time
  - id: get_datetime
    description: "Get current date and time"
    exec:
      command: "date '+%A, %B %d, %Y at %H:%M'"
      shell: true
    output:
      format: text

  # Generate weather summary (mock - replace with real API)
  - id: weather
    description: "Generate weather summary"
    infer:
      prompt: |
        Generate a realistic weather forecast for {{{{inputs.location}}}} for today.

        Include:
        - Current temperature
        - High/low for the day
        - Conditions (sunny, cloudy, rain, etc.)
        - Recommendation (umbrella, sunscreen, etc.)

        Return as JSON.
    output:
      format: json
      schema:
        type: object
        required: [temperature, high, low, conditions]
        properties:
          temperature:
            type: string
          high:
            type: string
          low:
            type: string
          conditions:
            type: string
          recommendation:
            type: string

  # Generate news summary
  - id: news
    description: "Generate news highlights"
    infer:
      prompt: |
        Generate 5 plausible tech/AI news headlines for today.
        These should be realistic examples of current industry news.

        Topics of interest: {{{{inputs.interests | to_json}}}}

        Return as JSON array of objects with title and summary.
    output:
      format: json
      schema:
        type: object
        required: [headlines]
        properties:
          headlines:
            type: array
            items:
              type: object
              properties:
                title:
                  type: string
                summary:
                  type: string

  # Compile briefing
  - id: compile
    description: "Compile morning briefing"
    with:
      datetime: $get_datetime
      weather: $weather
      news: $news
    infer:
      prompt: |
        Create a friendly morning briefing email based on:

        Date: {{{{with.datetime}}}}

        Weather:
        {{{{with.weather | to_json}}}}

        News Headlines:
        {{{{with.news | to_json}}}}

        Format as a professional but warm morning email.
        Include:
        - Greeting with date
        - Weather summary
        - Top news highlights
        - Motivational closing
    output:
      format: text
    artifact:
      path: briefing.md
      format: text
    depends_on: [get_datetime, weather, news]
"#
    )
}

/// Git changelog template
fn git_changelog(name: &str) -> String {
    format!(
        r#"# {name}
#
# Git changelog generator workflow.
# Analyzes git commits and generates a changelog.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable
#   - Git repository

schema: "nika/workflow@0.12"
workflow: {name}
description: "Git commit analysis and changelog generation"

provider: claude
model: claude-sonnet-4-6

inputs:
  commit_range:
    type: string
    description: "Git commit range"
    default: "HEAD~10..HEAD"
  output_format:
    type: string
    description: "Output format"
    default: "markdown"

artifacts:
  dir: ./changelogs
  format: text
  manifest: true

tasks:
  # Get git log
  - id: git_log
    description: "Get git commit history"
    exec:
      command: |
        git log {{{{inputs.commit_range}}}} \
          --pretty=format:'%h|%s|%an|%ad' \
          --date=short 2>/dev/null || echo "No commits found"
      shell: true
    output:
      format: text

  # Get detailed diff stats
  - id: diff_stats
    description: "Get diff statistics"
    exec:
      command: |
        git diff --stat {{{{inputs.commit_range}}}} 2>/dev/null || echo "No changes"
      shell: true
    output:
      format: text

  # Analyze commits
  - id: analyze
    description: "Analyze commit patterns"
    with:
      log: $git_log
      stats: $diff_stats
    infer:
      prompt: |
        Analyze these git commits and categorize them:

        Commit Log (hash|subject|author|date):
        {{{{with.log}}}}

        Diff Statistics:
        {{{{with.stats}}}}

        Categorize each commit as:
        - feat: New features
        - fix: Bug fixes
        - docs: Documentation
        - refactor: Code refactoring
        - test: Testing
        - chore: Maintenance

        Return JSON with categorized commits.
    output:
      format: json
      schema:
        type: object
        required: [commits, summary]
        properties:
          commits:
            type: object
            properties:
              feat:
                type: array
              fix:
                type: array
              docs:
                type: array
              refactor:
                type: array
              test:
                type: array
              chore:
                type: array
          summary:
            type: object
            properties:
              total_commits:
                type: integer
              files_changed:
                type: integer
              contributors:
                type: array
    depends_on: [git_log, diff_stats]

  # Generate changelog
  - id: changelog
    description: "Generate formatted changelog"
    with:
      analysis: $analyze
    infer:
      prompt: |
        Generate a professional changelog from this analysis:

        {{{{with.analysis | to_json}}}}

        Format: {{{{inputs.output_format}}}}

        Include:
        - Version header (use date as version)
        - Sections for each category (Features, Bug Fixes, etc.)
        - Contributor acknowledgments
        - Keep it concise and scannable
    output:
      format: text
    artifact:
      path: CHANGELOG-{{{{date}}}}.md
      format: text
    depends_on: [analyze]
"#
    )
}

/// Parallel translation template
fn parallel_translation(name: &str) -> String {
    format!(
        r#"# {name}
#
# Parallel translation workflow.
# Translates content to multiple languages using for_each.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable

schema: "nika/workflow@0.12"
workflow: {name}
description: "Multi-language translation with for_each"

provider: claude
model: claude-sonnet-4-6

inputs:
  source_text:
    type: string
    description: "Text to translate"
    default: "Welcome to our platform! We help businesses scale with AI."
  source_lang:
    type: string
    description: "Source language"
    default: "English"

artifacts:
  dir: ./translations/{{{{date}}}}
  format: json
  manifest: true

tasks:
  # Define target languages
  - id: setup
    description: "Setup translation targets"
    exec:
      command: "echo 'Preparing translations...'"
      shell: true
    output:
      format: text

  # Parallel translation to all languages
  - id: translate
    description: "Translate to multiple languages"
    for_each: ["French", "Spanish", "German", "Japanese", "Chinese", "Portuguese"]
    as: target_lang
    concurrency: 6
    infer:
      prompt: |
        Translate the following text from {{{{inputs.source_lang}}}} to {{{{with.target_lang}}}}.

        Source text:
        "{{{{inputs.source_text}}}}"

        Requirements:
        - Maintain the tone and intent
        - Use natural, native-sounding language
        - Preserve any technical terms appropriately

        Return JSON with translation details.
    output:
      format: json
      schema:
        type: object
        required: [language, translation]
        properties:
          language:
            type: string
          translation:
            type: string
          notes:
            type: string
    artifact:
      path: "{{{{with.target_lang | lowercase}}}}.json"
    depends_on: [setup]

  # Compile all translations
  - id: compile
    description: "Compile translation summary"
    with:
      translations: $translate
    infer:
      prompt: |
        Create a translation summary report from:

        {{{{with.translations | to_json}}}}

        Include:
        - Source text and language
        - Table of all translations
        - Any notable localization challenges
        - Recommendations for each market
    output:
      format: text
    artifact:
      path: summary.md
      format: text
    depends_on: [translate]
"#
    )
}

/// QA testing agent template
fn agent_qa_tester(name: &str) -> String {
    format!(
        r#"# {name}
#
# QA testing agent workflow.
# Generates test cases and validates functionality.
#
# Usage:
#   nika {name}.nika.yaml
#
# Requirements:
#   - ANTHROPIC_API_KEY environment variable

schema: "nika/workflow@0.12"
workflow: {name}
description: "QA testing agent with test generation"

provider: claude
model: claude-sonnet-4-6

inputs:
  feature_description:
    type: string
    description: "Feature to test"
    default: "User login with email and password"
  test_types:
    type: array
    description: "Types of tests to generate"
    default: ["unit", "integration", "edge_cases"]

artifacts:
  dir: ./test-reports/{{{{date}}}}
  format: json
  manifest: true

tasks:
  # Analyze the feature
  - id: analyze_feature
    description: "Analyze feature requirements"
    infer:
      prompt: |
        Analyze this feature for testing:

        Feature: {{{{inputs.feature_description}}}}

        Identify:
        1. Core functionality to test
        2. Input parameters and types
        3. Expected outputs
        4. Error conditions
        5. Security considerations

        Return structured analysis as JSON.
    output:
      format: json
      schema:
        type: object
        required: [core_functionality, inputs, outputs]
        properties:
          core_functionality:
            type: array
            items:
              type: string
          inputs:
            type: array
            items:
              type: object
          outputs:
            type: array
          error_conditions:
            type: array
          security_considerations:
            type: array

  # Generate test cases using agent
  - id: generate_tests
    description: "Generate comprehensive test cases"
    with:
      analysis: $analyze_feature
    agent:
      prompt: |
        You are a QA Engineer. Generate test cases for this feature:

        Feature Analysis:
        {{{{with.analysis | to_json}}}}

        Test Types Required: {{{{inputs.test_types | to_json}}}}

        For each test type, generate:
        1. Test name and description
        2. Preconditions
        3. Test steps
        4. Expected results
        5. Priority (high/medium/low)

        Use the following tools:
        - nika:log to track your progress
        - nika:assert to validate test case format

        Generate at least 3 test cases per type.
        End with "TEST_GENERATION_COMPLETE" when done.
      max_turns: 8
      tools: [nika:log, nika:assert]
    output:
      format: json
      schema:
        type: object
        required: [test_cases]
        properties:
          test_cases:
            type: array
            items:
              type: object
              properties:
                id:
                  type: string
                name:
                  type: string
                type:
                  type: string
                priority:
                  type: string
                preconditions:
                  type: array
                steps:
                  type: array
                expected_results:
                  type: array
    artifact:
      path: test_cases.json
    depends_on: [analyze_feature]

  # Generate test report
  - id: report
    description: "Generate test report"
    with:
      analysis: $analyze_feature
      tests: $generate_tests
    infer:
      prompt: |
        Create a professional QA test plan document:

        Feature Analysis:
        {{{{with.analysis | to_json}}}}

        Generated Test Cases:
        {{{{with.tests | to_json}}}}

        Include:
        1. Executive Summary
        2. Test Scope
        3. Test Cases (organized by type)
        4. Risk Assessment
        5. Recommended Test Coverage
        6. Next Steps

        Format as markdown.
    output:
      format: text
    artifact:
      path: test_plan.md
      format: text
    depends_on: [generate_tests]
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
    fn test_data_pipeline_template() {
        let content = data_pipeline("etl-test");
        assert!(content.contains("workflow: etl-test"));
        assert!(content.contains("artifacts:"));
        assert!(content.contains("extract"));
        assert!(content.contains("transform"));
        assert!(content.contains("load"));
    }

    #[test]
    fn test_morning_briefing_template() {
        let content = morning_briefing("briefing-test");
        assert!(content.contains("workflow: briefing-test"));
        assert!(content.contains("get_datetime"));
        assert!(content.contains("compile"));
        assert!(content.contains("briefings"));
    }

    #[test]
    fn test_git_changelog_template() {
        let content = git_changelog("changelog-test");
        assert!(content.contains("workflow: changelog-test"));
        assert!(content.contains("git_log"));
        assert!(content.contains("analyze"));
        assert!(content.contains("changelog"));
    }

    #[test]
    fn test_parallel_translation_template() {
        let content = parallel_translation("translate-test");
        assert!(content.contains("workflow: translate-test"));
        assert!(content.contains("for_each:"));
        assert!(content.contains("concurrency:"));
        assert!(content.contains("translations"));
    }

    #[test]
    fn test_agent_qa_tester_template() {
        let content = agent_qa_tester("qa-test");
        assert!(content.contains("workflow: qa-test"));
        assert!(content.contains("agent:"));
        assert!(content.contains("analyze_feature"));
        assert!(content.contains("generate_tests"));
    }

    #[test]
    fn test_all_templates_valid_yaml() {
        for template in Template::ALL {
            let content = generate_template(*template, "test");
            // Basic YAML validation - should contain schema
            assert!(
                content.contains("schema: \"nika/workflow@0.12\""),
                "Template {} missing schema",
                template.name()
            );
        }
    }
}
