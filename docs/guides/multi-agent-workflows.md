# Multi-Agent Workflow Recipes

Production-ready workflows using the `agent:` verb for autonomous multi-turn agents with tools, guardrails, resource limits, and structured output.

---

## Agent Architecture Overview

The `agent:` verb creates an autonomous loop where the LLM decides which tools to call, processes results, and iterates until a completion condition is met.

```
agent:
  system: "..."           # System prompt defining agent's role
  prompt: "..."           # User instruction
  tools: [...]            # Available tool list
  mcp: [...]              # MCP server connections
  max_turns: N            # Maximum iteration count
  max_tokens: N           # Max tokens per response
  token_budget: N         # Total token budget across all turns
  completion:
    mode: explicit        # Agent must call nika_complete to finish
  guardrails:             # Output quality gates
    - type: length
      min_words: 200
      on_failure: retry
    - type: regex
      pattern: "(?i)keyword"
      on_failure: retry
  limits:                 # Resource controls
    max_turns: 8
    max_tokens: 50000
    max_cost_usd: 1.00
    max_duration_secs: 120
```

### Available Tools

- **Builtin**: `nika:glob`, `nika:read`, `nika:write`, `nika:edit`, `nika:grep`, `nika:log`, `nika:complete`
- **Media**: All 24 `nika:*` media tools
- **MCP**: Any MCP server configured in the `mcp:` block
- **Shorthand**: `tools: [builtin]` includes all builtin file tools

### Completion Modes

| Mode | Behavior |
|------|----------|
| `explicit` | Agent must call `nika_complete` with its final output |
| `auto` (default) | Agent completes when it stops making tool calls |

---

## Recipe 1: Research Agent with Web Sources

**Problem:** You need an autonomous agent that fetches from multiple web sources, synthesizes the information, and produces a research brief.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: research-agent
description: "Autonomous research agent with web sources and file tools"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  research_topic: "the state of WebAssembly in 2026"
  max_sources: 5

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

artifacts:
  dir: ./output/research

tasks:
  # Pre-fetch web sources for the agent
  - id: source_blog
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20

  - id: source_feed
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 15

  - id: source_hn
    fetch:
      url: "https://news.ycombinator.com/"
      extract: article
      timeout: 20

  # The research agent
  - id: research
    depends_on: [source_blog, source_feed, source_hn]
    with:
      blog: $source_blog
      feed: $source_feed
      hn: $source_hn
    agent:
      system: |
        You are a senior technology researcher with access to filesystem
        and builtin tools. Your research methodology:

        1. Analyze the provided source material
        2. Use nika_glob and nika_read to check for existing research
        3. Use nika_write to save intermediate notes
        4. Use nika_log to track your progress
        5. Synthesize findings into a comprehensive brief
        6. Call nika_complete with the final research brief

        Be thorough. Cite sources. Identify trends.
      prompt: |
        Research topic: {{inputs.research_topic}}

        Source material (from web scraping):
        Blog content: {{with.blog | first(2000)}}
        RSS feed entries: {{with.feed | first(1500)}}
        News aggregator: {{with.hn | first(1500)}}

        Produce a research brief with:
        - Executive Summary (200 words)
        - Key Findings (5-8 points with evidence)
        - Technology Landscape (current state)
        - Trend Analysis (emerging patterns)
        - Recommendations (3-5 actionable items)
        - Sources Used

        Call nika_complete when your brief is ready.
      mcp: [filesystem]
      tools: [builtin]
      max_turns: 8
      max_tokens: 2000
      token_budget: 20000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 300
          on_failure: retry
        - type: regex
          pattern: "(?i)(finding|trend|recommendation)"
          message: "Research must include findings, trends, and recommendations"
          on_failure: retry
      limits:
        max_turns: 8
        max_tokens: 40000
        max_cost_usd: 1.00
        max_duration_secs: 180
    artifact:
      path: research-brief.md

  # Generate executive summary
  - id: executive_summary
    depends_on: [research]
    with:
      brief: $research
    infer:
      prompt: |
        Write a 200-word executive summary of this research:
        {{with.brief | first(3000)}}
      temperature: 0.3
      max_tokens: 500
    artifact:
      path: executive-summary.md
```

**Explanation:**

This workflow pre-fetches web content using three different extract modes (markdown, feed, article), then feeds it to a research agent. The agent has access to:

- **MCP tools** (`filesystem`): Can read/write to the local filesystem via the MCP protocol
- **Builtin tools** (`[builtin]`): Can glob, read, write, edit, grep files in the project
- **Explicit completion**: The agent must call `nika_complete` with its final output, preventing premature termination
- **Guardrails**: Output must be 300+ words and contain key research terminology
- **Resource limits**: Capped at $1.00, 180 seconds, and 40K tokens to prevent runaway costs

**Expected Output:** A comprehensive research brief and executive summary.

---

## Recipe 2: Code Review Agent

**Problem:** You need an autonomous agent that explores a codebase, identifies issues, and produces a structured review report.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: code-review-agent
description: "Autonomous code reviewer with file exploration and structured output"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  focus_areas: "error handling, security, performance"
  severity_threshold: "medium"

artifacts:
  dir: ./output/code-review

tasks:
  # Agent explores the codebase
  - id: review
    agent:
      system: |
        You are a senior code reviewer performing a thorough review.
        Available tools:
        - nika_glob: Find files matching patterns (e.g., "**/*.rs", "src/**/*.ts")
        - nika_read: Read file contents
        - nika_grep: Search for patterns across the codebase
        - nika_log: Log your progress as you work

        Your review methodology:
        1. Use nika_glob to discover the project structure
        2. Use nika_grep to search for common issues:
           - "unwrap()" — unhandled errors
           - "TODO" — incomplete implementation
           - "unsafe" — memory safety concerns
           - "password|secret|key" — hardcoded secrets
        3. Use nika_read to examine suspicious files in detail
        4. Use nika_log to track each finding as you discover it
        5. Call nika_complete with your full review

        Focus areas: {{inputs.focus_areas}}
        Minimum severity to report: {{inputs.severity_threshold}}
      prompt: |
        Perform a comprehensive code review of this project.
        Focus on: {{inputs.focus_areas}}

        For each finding:
        - File path and line numbers
        - Severity: critical / high / medium / low
        - Category: security / performance / reliability / maintainability
        - Description of the issue
        - Suggested fix with code example

        Call nika_complete with your complete review report.
      tools:
        - "nika:glob"
        - "nika:read"
        - "nika:grep"
        - "nika:log"
      max_turns: 12
      max_tokens: 2000
      token_budget: 30000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 300
          on_failure: retry
        - type: regex
          pattern: "(?i)(issue|finding|recommendation|severity)"
          message: "Review must include findings with severity ratings"
          on_failure: retry
        - type: regex
          pattern: "(?i)(critical|high|medium|low)"
          message: "Review must use severity levels"
          on_failure: retry
      limits:
        max_turns: 12
        max_tokens: 60000
        max_cost_usd: 2.00
        max_duration_secs: 300
    artifact:
      path: raw-review.md

  # Convert to structured JSON
  - id: structured_report
    depends_on: [review]
    with:
      raw: $review
    infer:
      prompt: |
        Convert this code review into structured JSON:

        {{with.raw | first(4000)}}

        Required fields:
        - project_name, review_date
        - overall_score (1-100)
        - findings: array of { severity, category, file, description, fix }
        - summary: 3-sentence overview
        - metrics: { files_reviewed, issues_found, critical_count, high_count }
      response_format: json
      temperature: 0.1
      max_tokens: 3000
    structured:
      schema:
        type: object
        properties:
          project_name:
            type: string
          overall_score:
            type: integer
          findings:
            type: array
            items:
              type: object
              properties:
                severity:
                  type: string
                  enum: ["critical", "high", "medium", "low"]
                category:
                  type: string
                file:
                  type: string
                description:
                  type: string
              required: [severity, category, description]
          metrics:
            type: object
            properties:
              files_reviewed:
                type: integer
              issues_found:
                type: integer
            required: [files_reviewed, issues_found]
          summary:
            type: string
        required: [project_name, overall_score, findings, summary]
    artifact:
      path: review-report.json
      format: json

  # Generate actionable PR comment
  - id: pr_comment
    depends_on: [structured_report]
    with:
      report: $structured_report
    infer:
      prompt: |
        Generate a GitHub PR comment from this review:
        {{with.report}}

        Format:
        ## Code Review Summary
        Overall Score: X/100
        ...critical findings first...
        ...then high/medium/low...
        ### Quick Fixes (things that can be fixed in this PR)
        ### Follow-up Items (for future PRs)
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: pr-comment.md
```

**Explanation:**

The code review agent uses four specific tools:
- `nika:glob` to discover the project structure
- `nika:grep` to search for anti-patterns across the codebase
- `nika:read` to examine individual files in detail
- `nika:log` to track progress during the multi-turn review

The `max_turns: 12` gives the agent enough iterations to explore a medium-sized codebase. The three guardrails ensure the output contains findings with severity ratings. The structured report task converts the free-form review into validated JSON.

**Expected Output:** Raw review markdown, structured JSON report, and a GitHub PR comment.

---

## Recipe 3: Customer Support Agent

**Problem:** You need an agent that can answer customer questions by searching a knowledge base, checking FAQs, and providing structured responses.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: support-agent
description: "Customer support agent with knowledge base access and guardrails"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  customer_query: "How do I configure multiple providers in Nika?"
  customer_tier: "pro"

context:
  files:
    faq: ./context/faq.md
    policies: ./context/support-policies.md

artifacts:
  dir: ./output/support

tasks:
  # Support agent with knowledge base
  - id: handle_query
    agent:
      system: |
        You are a friendly, knowledgeable customer support agent for Nika.
        Customer tier: {{inputs.customer_tier}}

        Support policies: {{context.files.policies | first(1000)}}

        Your workflow:
        1. Search the knowledge base (nika_glob + nika_grep) for relevant docs
        2. Read relevant documentation (nika_read)
        3. Use nika_log to note your reasoning
        4. Formulate a clear, helpful response
        5. Call nika_complete with your response

        Rules:
        - Be empathetic and professional
        - Include specific steps (numbered)
        - Reference documentation when possible
        - If you cannot find an answer, say so honestly
        - Never share internal pricing or unreleased features
        - Pro tier customers get priority and extended support
      prompt: |
        Customer question: {{inputs.customer_query}}
        Customer tier: {{inputs.customer_tier}}

        FAQ reference: {{context.files.faq | first(2000)}}

        Search the knowledge base, find the answer, and provide a helpful response.
        Call nika_complete when ready.
      tools:
        - "nika:glob"
        - "nika:read"
        - "nika:grep"
        - "nika:log"
      max_turns: 6
      max_tokens: 1500
      token_budget: 12000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 100
          max_words: 800
          on_failure: retry
        - type: regex
          pattern: "(?i)(step \\d|\\d\\.)"
          message: "Response must include numbered steps"
          on_failure: retry
      limits:
        max_turns: 6
        max_tokens: 20000
        max_cost_usd: 0.50
        max_duration_secs: 60
    artifact:
      path: response.md

  # Classify the query for analytics
  - id: classify_query
    depends_on: [handle_query]
    with:
      response: $handle_query
    infer:
      prompt: |
        Classify this customer support interaction:

        Query: {{inputs.customer_query}}
        Response: {{with.response | first(500)}}
        Customer tier: {{inputs.customer_tier}}

        Return classification with category, complexity, resolution_type, and satisfaction_prediction.
      response_format: json
      temperature: 0.1
      max_tokens: 500
    structured:
      schema:
        type: object
        properties:
          category:
            type: string
            enum: ["configuration", "billing", "bug_report", "feature_request", "how_to", "other"]
          complexity:
            type: string
            enum: ["simple", "moderate", "complex"]
          resolution_type:
            type: string
            enum: ["self_service", "agent_assisted", "escalation"]
          satisfaction_prediction:
            type: integer
        required: [category, complexity, resolution_type]
    artifact:
      path: classification.json
      format: json
```

**Explanation:**

The support agent uses `context.files` to load FAQ and support policies that persist across all turns. The guardrails enforce response quality:
- **Length**: Between 100-800 words (not too short, not too long)
- **Regex**: Must include numbered steps for actionable guidance
- **on_failure: retry**: If guardrails fail, the agent gets another chance with feedback

The `max_cost_usd: 0.50` and `max_duration_secs: 60` limits prevent expensive or slow interactions. The classification task adds analytics metadata for tracking support quality.

**Expected Output:** A customer-facing response and a JSON classification for analytics.

---

## Recipe 4: Data Analysis Agent with Chart Generation

**Problem:** You need an agent that can analyze data, generate visualizations, and produce a structured report with guardrails.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: data-analysis-agent
description: "Autonomous data analyst with chart generation and guardrails"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  analysis_focus: "user engagement metrics"
  report_audience: "executive team"

artifacts:
  dir: ./output/data-analysis

tasks:
  # Fetch sample data
  - id: fetch_data
    fetch:
      url: "https://jsonplaceholder.typicode.com/users"
      timeout: 15

  # Generate charts for the agent to analyze
  - id: engagement_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "User Engagement by Segment"
        width: 900
        height: 600
        series:
          - name: "Active Users"
            data: [450, 320, 180, 95, 55]
          - name: "Power Users"
            data: [120, 85, 45, 20, 10]
        labels: ["Enterprise", "Pro", "Starter", "Trial", "Free"]
    artifact:
      path: engagement-chart.png
      format: binary

  - id: trend_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "line"
        title: "Monthly Active Users (6-month trend)"
        width: 900
        height: 500
        series:
          - name: "MAU"
            data: [8500, 9200, 9800, 10500, 11200, 12100]
          - name: "DAU"
            data: [2100, 2400, 2650, 2900, 3100, 3400]
        labels: ["Oct", "Nov", "Dec", "Jan", "Feb", "Mar"]
    artifact:
      path: trend-chart.png
      format: binary

  # Analysis agent with vision
  - id: analysis_agent
    depends_on: [fetch_data, engagement_chart, trend_chart]
    with:
      data: $fetch_data
      engagement: $engagement_chart
      trends: $trend_chart
    agent:
      system: |
        You are a senior data analyst preparing a report for the {{inputs.report_audience}}.
        Focus area: {{inputs.analysis_focus}}

        You have access to:
        - Charts (provided as images in the conversation)
        - Raw data (provided in the prompt)
        - File tools (nika_glob, nika_read, nika_write)
        - Logging (nika_log)

        Methodology:
        1. Analyze the provided data and charts
        2. Write intermediate findings to files
        3. Log your analytical reasoning
        4. Produce a comprehensive analysis
        5. Call nika_complete with the final report

        Requirements:
        - Include specific numbers and percentages
        - Reference the charts in your analysis
        - Provide "so what" context for each finding
        - End with actionable recommendations
      prompt: |
        Analyze {{inputs.analysis_focus}} for the {{inputs.report_audience}}.

        Raw data:
        {{with.data | first(2000)}}

        Charts have been generated showing:
        1. User engagement by segment (bar chart)
        2. Monthly active/daily active user trends (line chart)

        Produce a data-driven analysis with:
        - Executive Summary (3 sentences)
        - Key Metrics Dashboard (top 5 KPIs with trend indicators)
        - Segment Analysis (per customer tier)
        - Growth Trends (MoM changes, forecasts)
        - Risk Factors (churn indicators, engagement drops)
        - Recommendations (prioritized, with expected impact)

        Call nika_complete with your full report.
      tools:
        - "nika:glob"
        - "nika:read"
        - "nika:write"
        - "nika:log"
      max_turns: 8
      max_tokens: 2500
      token_budget: 25000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 400
          max_words: 2500
          on_failure: retry
        - type: regex
          pattern: "(?i)recommendation"
          message: "Report must include recommendations"
          on_failure: retry
        - type: regex
          pattern: "\\d+%"
          message: "Report must include percentage metrics"
          on_failure: retry
      limits:
        max_turns: 8
        max_tokens: 50000
        max_cost_usd: 2.00
        max_duration_secs: 180
    artifact:
      path: analysis-report.md

  # Generate structured KPIs
  - id: kpi_dashboard
    depends_on: [analysis_agent]
    with:
      report: $analysis_agent
    infer:
      prompt: |
        Extract structured KPIs from this analysis:
        {{with.report | first(2000)}}

        Return JSON with kpis array (name, value, trend, target).
      response_format: json
      temperature: 0.1
      max_tokens: 1000
    structured:
      schema:
        type: object
        properties:
          kpis:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                value:
                  type: string
                trend:
                  type: string
                  enum: ["up", "down", "stable"]
              required: [name, value, trend]
          overall_health:
            type: string
            enum: ["excellent", "good", "concerning", "critical"]
        required: [kpis, overall_health]
    artifact:
      path: kpi-dashboard.json
      format: json
```

**Explanation:**

This workflow generates charts before launching the agent, so the agent has visual context for its analysis. The three guardrails work together:
1. **Length**: 400-2500 words for executive-appropriate depth
2. **Regex ("recommendation")**: Must include actionable recommendations
3. **Regex ("\\d+%")**: Must include specific percentage metrics

The `limits` block provides four safety nets: turn count, token budget, cost cap, and duration limit. If any limit is reached, the agent returns whatever partial results it has.

**Expected Output:** Two chart PNGs, an analysis report, and a structured KPI dashboard.

---

## Recipe 5: Multi-Step Documentation Agent

**Problem:** You need an agent that can explore a codebase, write documentation, and iteratively refine it based on what it discovers.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: documentation-agent
description: "Agent that explores code and writes comprehensive documentation"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  doc_topic: "API reference"
  target_audience: "developers integrating with the API"

artifacts:
  dir: ./output/documentation

tasks:
  # Prepare project context
  - id: project_context
    exec: |
      echo '{"project": "nika", "language": "rust", "framework": "none", "doc_format": "markdown"}'

  # Documentation agent with extensive file access
  - id: doc_writer
    depends_on: [project_context]
    with:
      context: $project_context
    agent:
      system: |
        You are a technical documentation specialist.
        Target audience: {{inputs.target_audience}}

        Methodology:
        1. nika_glob to discover source files and existing docs
        2. nika_read to examine key source files
        3. nika_grep to find function signatures, types, constants
        4. nika_write to save documentation drafts
        5. nika_read your draft, then revise and improve
        6. nika_log to track your progress
        7. Call nika_complete with the final documentation

        Writing standards:
        - Every public function gets a description, parameters, return type, example
        - Group related functions by module
        - Include a "Quick Start" section
        - Use consistent Markdown formatting
        - Include code examples that actually compile
      prompt: |
        Write {{inputs.doc_topic}} documentation.
        Project: {{with.context}}

        Steps:
        1. Explore the project structure
        2. Find all public APIs
        3. Document each with examples
        4. Write a Quick Start guide
        5. Add a troubleshooting section

        Call nika_complete with the final documentation.
      tools:
        - "nika:glob"
        - "nika:read"
        - "nika:write"
        - "nika:grep"
        - "nika:log"
      max_turns: 10
      max_tokens: 2500
      token_budget: 30000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 500
          on_failure: retry
        - type: regex
          pattern: "(?i)(quick start|getting started)"
          message: "Documentation must include a Quick Start section"
          on_failure: retry
        - type: regex
          pattern: "```"
          message: "Documentation must include code examples"
          on_failure: retry
      limits:
        max_turns: 10
        max_tokens: 60000
        max_cost_usd: 3.00
        max_duration_secs: 300
    artifact:
      path: api-reference.md

  # Generate a summary document
  - id: summary
    depends_on: [doc_writer]
    with:
      docs: $doc_writer
    infer:
      prompt: |
        Create a one-page summary of this documentation:
        {{with.docs | first(4000)}}

        Include: overview, most important functions, common use cases, and links to sections.
      temperature: 0.3
      max_tokens: 800
    artifact:
      path: documentation-summary.md
```

**Explanation:**

The documentation agent has `max_turns: 10`, giving it enough iterations to explore, write drafts, and refine. The key pattern is:

1. **Explore** (turns 1-3): Use `nika_glob` and `nika_grep` to understand the codebase
2. **Draft** (turns 4-6): Use `nika_write` to save initial documentation
3. **Refine** (turns 7-9): Use `nika_read` to review the draft, then improve
4. **Complete** (turn 10): Call `nika_complete` with the final version

The three guardrails ensure the documentation includes a Quick Start section, code examples, and sufficient depth (500+ words).

**Expected Output:** Complete API reference documentation and a one-page summary.

---

## Key Patterns for Agent Workflows

### Guardrail Types

```yaml
guardrails:
  # Length: enforce word count range
  - type: length
    min_words: 200
    max_words: 2000
    on_failure: retry

  # Regex: enforce content requirements
  - type: regex
    pattern: "(?i)recommendation"
    message: "Must include recommendations"
    on_failure: retry
```

### on_failure Options

| Option | Behavior |
|--------|----------|
| `retry` | Re-run the agent with guardrail feedback (default) |
| `escalate` | Flag for human review |
| `fail` | Immediately fail the task |

### Resource Limits

```yaml
limits:
  max_turns: 8          # Max tool-call iterations
  max_tokens: 50000     # Total token budget
  max_cost_usd: 2.00    # Maximum cost in USD
  max_duration_secs: 300 # Maximum wall-clock time
```

### Agent Status Outcomes

| Status | Meaning |
|--------|---------|
| `end_turn` | Natural completion (no more tool calls) |
| `tool_complete` | Explicit completion (called `nika_complete`) |
| `tool_complete_high` | High confidence completion |
| `tool_complete_low` | Low confidence (may retry) |
| `max_turns` | Turn limit reached |
| `max_tokens` | Token budget exceeded |
| `max_cost` | Cost limit reached |
| `max_duration` | Time limit reached |

### Best Practices

1. **Always use `completion: mode: explicit`** for important tasks. This forces the agent to explicitly call `nika_complete`, preventing premature termination.

2. **Set resource limits** to prevent runaway costs. A good starting point: `max_cost_usd: 1.00`, `max_duration_secs: 120`.

3. **Use guardrails for quality gates**. The `retry` on_failure mode gives the agent feedback about what was missing, allowing it to self-correct.

4. **Pre-fetch data** before the agent loop. Using `fetch:` tasks before the agent reduces the number of agent turns needed.

5. **Use `nika_log` for observability**. The agent can log its reasoning, making debugging easier.
