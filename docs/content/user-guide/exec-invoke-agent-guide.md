# exec, invoke, and agent -- Complete Guide

This guide covers the three remaining verbs: `exec:` for running shell commands, `invoke:` for calling MCP tools (including Nika's 24 builtin tools), and `agent:` for multi-turn autonomous loops. Together with `infer:` and `fetch:`, these complete Nika's five-verb system.

## Part 1: exec -- Shell Commands

The `exec:` verb runs shell commands and captures their stdout as the task output.

### Simple Form

```yaml
  - id: hello
    exec: "echo 'Hello World'"
```

The command is executed directly (not through a shell). This means shell features like pipes, redirects, and variable expansion do not work in the simple form.

### Extended Form

For full shell capabilities, use the object form with `shell: true`:

```yaml
  - id: system_info
    exec:
      command: "uname -a && echo '---' && df -h | head -5"
      shell: true
      timeout: 10
```

### All exec: Fields

```yaml
- id: my_exec
  exec:
    command: "npm run build"           # Required. The command to run.
    shell: true                        # Run through sh -c (default: false)
    cwd: ./frontend                    # Working directory (default: workflow dir)
    env:                               # Additional environment variables
      NODE_ENV: production
      BUILD_ID: "{{with.build_id}}"
    timeout: 60                        # Timeout in seconds (default: 30)
```

### When to Use shell: true

| Feature | Needs `shell: true` |
|---------|:-------------------:|
| Pipes (`cmd1 \| cmd2`) | Yes |
| Command chaining (`&&`, `\|\|`) | Yes |
| Redirects (`>`, `>>`, `<`) | Yes |
| Glob expansion (`*.txt`) | Yes |
| Environment variable expansion (`$HOME`) | Yes |
| Subshells (`$(command)`) | Yes |
| Simple commands (`echo`, `date`) | No |
| Commands with arguments (`git status`) | No |

### Using Templates in Commands

Inject data from bindings into shell commands:

```yaml
  - id: process
    depends_on: [source]
    with:
      data: $source | trim
    exec:
      command: |
        echo "Processing: {{with.data}}"
        echo "{{with.data}}" | wc -c
      shell: true
```

### Environment Variables

Pass variables to the command environment:

```yaml
  - id: deploy
    exec:
      command: "./deploy.sh"
      env:
        DEPLOY_ENV: "production"
        API_URL: "https://api.example.com"
        BUILD_NUMBER: "{{with.build_num}}"
      cwd: ./scripts
      timeout: 120
```

### Working Directory

```yaml
  - id: frontend_build
    exec:
      command: "npm ci && npm run build"
      shell: true
      cwd: ./packages/frontend
```

### Multi-Line Commands

Use YAML block scalars for complex scripts:

```yaml
  - id: complex_script
    exec:
      command: |
        echo "Step 1: Prepare"
        mkdir -p output
        echo "Step 2: Process"
        for f in data/*.csv; do
          echo "Processing: $f"
          wc -l "$f"
        done
        echo "Step 3: Complete"
      shell: true
```

### Capturing JSON Output

When a command produces JSON, use `output: { format: json }` to parse it:

```yaml
  - id: json_data
    exec: |
      echo '{"users": ["Alice", "Bob", "Charlie"], "count": 3}'
    output:
      format: json

  - id: use_json
    depends_on: [json_data]
    with:
      first_user: $json_data.users[0]
      count: $json_data.count
    exec: "echo 'First user: {{with.first_user}}, Total: {{with.count}}'"
```

### Security Note

Nika includes a command blocklist that prevents execution of dangerous commands (e.g., `rm -rf /`). Environment variable names are also validated to prevent injection attacks.

## Part 2: invoke -- MCP Tool Calls

The `invoke:` verb calls tools through the Model Context Protocol (MCP). This includes Nika's 24 builtin tools and any external MCP server configured in the workflow.

### Builtin Tools (nika: prefix)

Nika's builtin tools are always available without any MCP server configuration:

```yaml
  - id: log_message
    invoke:
      tool: "nika:log"
      params:
        message: "Workflow checkpoint reached"
        level: info
```

### Shorthand Form

When a tool needs no parameters:

```yaml
  - id: get_dims
    invoke: "nika:dimensions"
```

### All invoke: Fields

```yaml
- id: my_invoke
  invoke:
    tool: "server::tool_name"         # Required (unless resource: present)
    resource: "resource://uri"        # Alternative to tool: (MCP resource)
    params:                           # Tool parameters
      key: "value"
      nested:
        field: "{{with.data}}"
    mcp: server-name                  # Explicit MCP server (if not in tool name)
    timeout: 30                       # Seconds (default: 30)
```

### Tool Name Format

Tools are addressed as `server::tool_name`:

```yaml
  # Fully qualified (server::tool)
  - id: search
    invoke:
      tool: "github::search_repositories"
      params:
        query: "nika lang:rust"

  # Builtin tools (nika: prefix)
  - id: log
    invoke:
      tool: "nika:log"
      params:
        message: "Hello"

  # With explicit mcp: field
  - id: query
    invoke:
      tool: "run_query"
      mcp: neo4j
      params:
        query: "MATCH (n) RETURN n LIMIT 5"
```

### Builtin Tools Reference

#### Tier 1 -- Always Available (5 tools)

These tools are compiled into Nika and require no external dependencies:

**nika:import** -- Import files into the Content-Addressable Store

```yaml
  - id: import_image
    invoke:
      tool: "nika:import"
      params:
        path: "./images/photo.jpg"
```

Returns a media reference with the CAS hash that can be used by other media tools.

**nika:dimensions** -- Get image dimensions

```yaml
  - id: get_size
    depends_on: [import_image]
    with:
      img: $import_image
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"
```

Returns `{ "width": 1920, "height": 1080 }`.

**nika:thumbhash** -- Generate a tiny image placeholder

```yaml
  - id: placeholder
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"
```

Returns a 25-byte hash that can be decoded into a blurred placeholder image.

**nika:dominant_color** -- Extract color palette

```yaml
  - id: colors
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
```

**nika:pipeline** -- Chain operations in memory

```yaml
  - id: process
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        operations:
          - type: thumbnail
            width: 400
          - type: convert
            format: webp
          - type: optimize
```

Chains multiple media operations without writing intermediate files to disk.

#### Tier 2 -- Media Core (6 tools)

Available by default (part of the `media-core` feature):

| Tool | Purpose | Example Params |
|------|---------|---------------|
| `nika:thumbnail` | Resize images (SIMD-accelerated Lanczos3) | `{ hash, width, height }` |
| `nika:convert` | Convert format (PNG/JPEG/WebP) | `{ hash, format: "webp" }` |
| `nika:strip` | Remove metadata from images | `{ hash }` |
| `nika:metadata` | Extract EXIF/audio/video metadata | `{ hash }` |
| `nika:optimize` | Lossless PNG optimization | `{ hash }` |
| `nika:svg_render` | SVG to PNG rasterization | `{ hash, width }` |

#### Tier 3 -- Opt-in (13 tools)

Require specific feature flags at compile time:

| Tool | Feature Flag | Purpose |
|------|-------------|---------|
| `nika:phash` | media-phash | Perceptual image hashing |
| `nika:compare` | media-phash | Visual similarity comparison |
| `nika:pdf_extract` | media-pdf | PDF text extraction |
| `nika:chart` | media-chart | Generate charts from JSON data |
| `nika:provenance` | media-provenance | C2PA content credentials (sign) |
| `nika:verify` | media-provenance | C2PA manifest verification |
| `nika:qr_validate` | media-qr | QR code decode + scan score |
| `nika:quality` | media-iqa | Image quality assessment (DSSIM/SSIM) |
| `nika:html_to_md` | fetch-markdown | HTML to Markdown |
| `nika:css_select` | fetch-html | CSS selector extraction |
| `nika:extract_metadata` | fetch-html | OG/Twitter Cards/JSON-LD metadata |
| `nika:extract_links` | fetch-html | Link classification |
| `nika:readability` | fetch-article | Article content extraction |

Check which features are available in your build:

```bash
nika features
```

### External MCP Servers

Connect to external tools by configuring MCP servers in your workflow:

```yaml
schema: nika/workflow@0.12
workflow: external-mcp

mcp:
  github:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: "{{$env.GITHUB_TOKEN}}"

  neo4j:
    command: npx
    args: ["-y", "@neo4j/mcp-neo4j"]
    env:
      NEO4J_URI: "bolt://localhost:7687"
      NEO4J_PASSWORD: "{{$env.NEO4J_PASSWORD}}"

tasks:
  - id: issues
    invoke:
      tool: "github::list_issues"
      params:
        repo: "supernovae-st/nika"
        state: open

  - id: store
    depends_on: [issues]
    with:
      data: $issues | to_json
    invoke:
      tool: "neo4j::run_query"
      params:
        query: "CREATE (n:ImportedIssues {data: $data})"
```

### SSE Transport

For MCP servers running as web services:

```yaml
mcp:
  remote_server:
    url: http://localhost:8080
    transport: sse
```

### Managing MCP Connections

```bash
# List servers in a workflow
nika mcp list -w workflow.nika.yaml

# Test a server connection
nika mcp test workflow.nika.yaml github

# List available tools on a server
nika mcp tools workflow.nika.yaml github
```

## Part 3: agent -- Multi-Turn Agent Loops

The `agent:` verb creates autonomous agents that can use tools, make decisions, and iterate until a goal is achieved. This is the most powerful verb in Nika.

### Basic Agent

```yaml
schema: nika/workflow@0.12
workflow: research-agent
provider: anthropic

tasks:
  - id: research
    agent:
      prompt: "Find the current population of the 5 largest cities in Europe."
      max_turns: 10
```

The agent will:
1. Receive the prompt
2. Decide which tools to call
3. Execute tool calls
4. Analyze results
5. Decide whether to continue or stop
6. Repeat until done or `max_turns` is reached

### All agent: Fields

```yaml
- id: my_agent
  agent:
    # Core
    prompt: "Research {{with.topic}}"         # Required. The agent's goal.
    system: "You are a researcher"            # Agent persona/instructions
    tools: [web_search, read_file]            # Available tools (name list)
    max_turns: 20                             # Max tool call iterations
    max_tokens: 4096                          # Tokens per LLM response

    # Provider
    provider: anthropic                       # Override workflow provider
    model: claude-sonnet-4-6               # Override workflow model
    temperature: 0.3                          # LLM temperature

    # MCP integration
    mcp: [github, neo4j]                      # MCP servers to expose to agent

    # Agent reuse
    from: researcher                          # Reference agents: definition

    # Skills
    skills: [writing, analysis]               # Inject skill prompts

    # Tool control
    tool_choice: auto                         # auto | required | none

    # Scope and limits
    scope: full                               # full | minimal | debug
    depth_limit: 3                            # Max spawn_agent recursion
    token_budget: 100000                      # Total token budget

    # Claude-specific
    extended_thinking: true                   # Enable thinking
    thinking_budget: 10000                    # Thinking token budget

    # Stop conditions
    stop_sequences: ["DONE"]                  # Stop generation on these strings
    completion:                               # Programmatic stop condition
      on_tool: final_answer                   # Stop when this tool is called

    # Output validation
    guardrails:
      - type: length
        max_words: 500

    # Cost control
    limits:
      max_cost_usd: 1.0                      # Maximum spend in USD
```

### Reusable Agent Definitions

Define agent personas at the workflow level and reference them:

```yaml
schema: nika/workflow@0.12
workflow: multi-agent
provider: anthropic

agents:
  researcher:
    system: |
      You are a meticulous research assistant.
      Always cite sources. Never make claims without evidence.
    tools: [web_search, read_file]
    max_turns: 15
    temperature: 0.2

  writer:
    system: |
      You are a skilled technical writer.
      Write clearly and concisely. Use examples.
    max_turns: 10
    temperature: 0.7

tasks:
  - id: research_topic
    agent:
      from: researcher
      prompt: "Research the latest developments in quantum computing."

  - id: write_article
    depends_on: [research_topic]
    with:
      research: $research_topic | trim
    agent:
      from: writer
      prompt: |
        Write a 500-word article based on this research:
        {{with.research}}
```

The `from:` field loads the agent definition and merges it with any task-level overrides.

### Agent Tools

Agents can access:

1. **Builtin tools** (nika: prefix) -- Always available
2. **MCP server tools** -- From configured MCP servers
3. **File tools** -- Read, write, edit, glob, grep (for code agents)
4. **Custom tools** -- Listed by name in the `tools:` field

```yaml
  - id: code_agent
    agent:
      prompt: "Refactor the authentication module."
      tools: [read_file, write_file, edit_file, glob, grep]
      mcp: [github]
      max_turns: 20
```

### Completion Conditions

Control when the agent stops:

```yaml
  # Stop after calling a specific tool
  - id: search_agent
    agent:
      prompt: "Find the answer and submit it."
      completion:
        on_tool: submit_answer

  # Stop on sequence
  - id: qa_agent
    agent:
      prompt: "Answer the question."
      stop_sequences: ["FINAL ANSWER:"]
```

### Agent Guardrails

Validate agent output before accepting:

```yaml
  - id: safe_agent
    agent:
      prompt: "Write a product description."
      guardrails:
        - type: length
          min_words: 50
          max_words: 200
        - type: regex
          pattern: "\\$\\d+\\.\\d{2}"
          message: "Must include a price"
        - type: llm
          judge_prompt: |
            Is this description professional and accurate?
            Answer PASS or FAIL.
          pass_pattern: "^PASS"
          on_failure: retry
```

### Token Budget and Cost Limits

Control agent costs:

```yaml
  - id: budget_agent
    agent:
      prompt: "Research and write a comprehensive report."
      token_budget: 50000          # Total tokens across all turns
      limits:
        max_cost_usd: 2.00         # Hard spend limit
      max_turns: 30
```

### Agent Scope

Control how much context the agent sees:

| Scope | Description |
|-------|-------------|
| `full` | Full tool results and conversation history |
| `minimal` | Condensed history, saves tokens |
| `debug` | Extra verbose, includes internal state |

### Spawning Sub-Agents

Agents can spawn child agents for subtasks:

```yaml
  - id: manager
    agent:
      prompt: "Coordinate a research project."
      depth_limit: 3   # Allow up to 3 levels of sub-agents
      tools: [spawn_agent]
```

The `depth_limit` prevents runaway recursion.

## Combining Verbs: Real-World Patterns

### Data Pipeline

```yaml
schema: nika/workflow@0.12
workflow: full-pipeline
provider: anthropic

tasks:
  # Step 1: exec -- Generate input data
  - id: prepare
    exec:
      command: "ls -la ./data/ | tail -5"
      shell: true

  # Step 2: fetch -- Get external data
  - id: api_data
    fetch:
      url: "https://api.example.com/latest"
      extract: jsonpath
      selector: "$.results"

  # Step 3: invoke -- Process with builtin tools
  - id: import_image
    invoke:
      tool: "nika:import"
      params:
        path: "./data/chart.png"

  # Step 4: infer -- AI analysis
  - id: analyze
    depends_on: [prepare, api_data]
    with:
      local: $prepare | trim
      remote: $api_data | to_json
    infer:
      prompt: |
        Compare local data with API data:
        Local: {{with.local}}
        Remote: {{with.remote}}

  # Step 5: agent -- Complex follow-up
  - id: deep_dive
    depends_on: [analyze]
    with:
      analysis: $analyze
    agent:
      prompt: "Based on this analysis, investigate further: {{with.analysis}}"
      max_turns: 10
```

### CI/CD Integration

```yaml
schema: nika/workflow@0.12
workflow: ci-pipeline

tasks:
  - id: lint
    exec:
      command: "npm run lint"
      cwd: ./frontend

  - id: test
    exec:
      command: "npm test -- --coverage"
      cwd: ./frontend
      timeout: 120

  - id: build
    depends_on: [lint, test]
    exec:
      command: "npm run build"
      cwd: ./frontend
      env:
        NODE_ENV: production

  - id: deploy_check
    depends_on: [build]
    with:
      test_result: $test | trim
    invoke:
      tool: "nika:assert"
      params:
        condition: "'{{with.test_result}}' != ''"
        message: "Tests must produce output"
```

### Content Production Pipeline

```yaml
schema: nika/workflow@0.12
workflow: content-production
provider: anthropic

tasks:
  - id: research
    agent:
      prompt: "Research the top 5 AI trends for 2026."
      max_turns: 15

  - id: outline
    depends_on: [research]
    with:
      research: $research
    infer:
      prompt: "Create a blog post outline from: {{with.research}}"
    structured:
      schema:
        type: object
        properties:
          title: { type: string }
          sections:
            type: array
            items:
              type: object
              properties:
                heading: { type: string }
                points: { type: array, items: { type: string } }
              required: [heading, points]
        required: [title, sections]

  - id: write_draft
    depends_on: [outline]
    with:
      outline: $outline | to_json
    infer:
      prompt: "Write a 1000-word blog post following this outline: {{with.outline}}"
      temperature: 0.7
      max_tokens: 2000

  - id: save
    depends_on: [write_draft, outline]
    with:
      draft: $write_draft
      title: $outline.title | lower | trim
    exec:
      command: "echo '{{with.draft}}' > output/{{with.title}}.md"
      shell: true
    artifact:
      path: blog-post.md
      format: text
```
