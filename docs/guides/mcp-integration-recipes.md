# MCP Integration Recipes

Production-ready workflows for connecting to external MCP (Model Context Protocol) servers, integrating with NovaNet knowledge graphs, and chaining tool calls across multiple MCP providers.

---

## MCP Architecture in Nika

Nika is an MCP client. It can connect to any MCP server and make its tools available to workflows through the `invoke:` verb or to agents through the `mcp:` block.

```yaml
# Workflow-level MCP configuration
mcp:
  server_name:
    command: "path/to/server"
    args: ["arg1", "arg2"]
    env:
      API_KEY: "$MY_API_KEY"

# Use in invoke: verb
invoke:
  tool: "server_name:tool_name"
  params:
    key: "value"

# Use in agent: verb
agent:
  mcp: [server_name]
  tools: [builtin]
```

### Zero Cypher Rule

Nika workflows **never** use raw Cypher queries. All database access goes through MCP `invoke:` calls. This keeps workflows portable and secure.

---

## Recipe 1: Filesystem MCP Server Integration

**Problem:** You need a workflow that uses the Anthropic Filesystem MCP server to read, write, and manage files through a standardized protocol.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: filesystem-mcp-integration
description: "Use the Filesystem MCP server for file operations"
provider: anthropic
model: claude-sonnet-4-20250514

mcp:
  filesystem:
    command: "npx"
    args: ["-y", "@anthropic/mcp-filesystem"]

artifacts:
  dir: ./output/mcp-filesystem

tasks:
  # Use the agent with filesystem MCP access
  - id: file_inventory
    agent:
      system: |
        You are a file system analyst with access to the filesystem MCP server.
        Use the filesystem tools to explore the current directory and create
        an organized inventory of all files.
      prompt: |
        Explore the current project directory and create a file inventory.
        List all files with their sizes and types.
        Call nika_complete with a structured inventory.
      mcp: [filesystem]
      tools: [builtin]
      max_turns: 6
      max_tokens: 1500
      token_budget: 15000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 100
          on_failure: retry
    artifact:
      path: file-inventory.md

  # Use infer to analyze the inventory
  - id: analysis
    depends_on: [file_inventory]
    with:
      inventory: $file_inventory
    infer:
      prompt: |
        Analyze this file inventory and provide recommendations:
        {{with.inventory | first(3000)}}

        Include:
        1. Directory structure assessment
        2. File organization suggestions
        3. Large files to review
        4. Potential cleanup opportunities
      temperature: 0.3
      max_tokens: 1000
    artifact:
      path: analysis.md
```

**Explanation:**

The `mcp:` block at the workflow level configures the Filesystem MCP server. The `npx -y @anthropic/mcp-filesystem` command installs and runs the server on demand. The agent then has access to all filesystem tools (read, write, list, etc.) through the MCP protocol.

The key insight is that MCP tools are available alongside builtin tools. The `tools: [builtin]` includes Nika's own file tools, while `mcp: [filesystem]` adds the MCP server's tools.

**Expected Output:** A file inventory and analysis report.

---

## Recipe 2: NovaNet Knowledge Graph via MCP

**Problem:** You need to query a NovaNet knowledge graph for structured data and use it in a content generation pipeline.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: novanet-knowledge-pipeline
description: "Query NovaNet knowledge graph via MCP and generate content"
provider: anthropic
model: claude-sonnet-4-20250514

mcp:
  novanet:
    command: "cargo"
    args:
      - "run"
      - "--manifest-path"
      - "../novanet/Cargo.toml"
      - "--"
      - "mcp"

artifacts:
  dir: ./output/knowledge-pipeline

tasks:
  # Query the knowledge graph for entities
  - id: query_entities
    invoke:
      tool: "novanet:query_nodes"
      params:
        node_class: "Technology"
        limit: 20

  # Query relationships
  - id: query_relationships
    invoke:
      tool: "novanet:query_arcs"
      params:
        arc_class: "DependsOn"
        source_class: "Technology"
        limit: 50

  # Query a specific technology
  - id: query_rust
    invoke:
      tool: "novanet:get_node"
      params:
        name: "Rust"
        class: "Technology"

  # Use knowledge graph data to generate content
  - id: generate_report
    depends_on: [query_entities, query_relationships, query_rust]
    with:
      entities: $query_entities
      relationships: $query_relationships
      rust_details: $query_rust
    infer:
      system: "You are a technology analyst with access to a knowledge graph."
      prompt: |
        Using this knowledge graph data, create a technology landscape report:

        Technologies: {{with.entities | first(2000)}}
        Dependency Map: {{with.relationships | first(2000)}}
        Rust Details: {{with.rust_details}}

        Include:
        1. Technology overview
        2. Dependency visualization (ASCII art)
        3. Risk assessment (single points of failure)
        4. Recommendations for technology portfolio
      temperature: 0.4
      max_tokens: 2000
    artifact:
      path: technology-report.md

  # Agent with knowledge graph access for deep analysis
  - id: deep_analysis
    depends_on: [generate_report]
    with:
      report: $generate_report
    agent:
      system: |
        You are a senior technology strategist with access to the NovaNet
        knowledge graph. Use it to validate and deepen the initial report.
      prompt: |
        Deepen this technology report with knowledge graph queries:
        {{with.report | first(2000)}}

        Query additional relationships, validate claims, and add evidence.
        Call nika_complete with the enhanced report.
      mcp: [novanet]
      tools: [builtin]
      max_turns: 6
      max_tokens: 2000
      token_budget: 20000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 300
          on_failure: retry
    artifact:
      path: enhanced-report.md
```

**Explanation:**

This workflow demonstrates the Nika-NovaNet integration pattern. NovaNet exposes its knowledge graph through MCP tools like `query_nodes`, `query_arcs`, and `get_node`. Nika workflows use `invoke:` to call these tools directly, or agents can use them through the `mcp:` block.

The Zero Cypher rule is enforced: no raw database queries appear in the YAML. All data access goes through the MCP tool interface.

**Expected Output:** A technology landscape report enhanced with knowledge graph data.

---

## Recipe 3: Multi-MCP Tool Chaining

**Problem:** You need to chain tools from multiple MCP servers in a single workflow -- for example, using a filesystem server, a database server, and a search server together.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: multi-mcp-orchestration
description: "Chain tools from multiple MCP servers"
provider: anthropic
model: claude-sonnet-4-20250514

mcp:
  filesystem:
    command: "npx"
    args: ["-y", "@anthropic/mcp-filesystem"]
  novanet:
    command: "cargo"
    args: ["run", "--manifest-path", "../novanet/Cargo.toml", "--", "mcp"]
    env:
      RUST_BACKTRACE: "1"

artifacts:
  dir: ./output/multi-mcp

tasks:
  # Use filesystem MCP to read configuration
  - id: read_config
    agent:
      system: "Read the project configuration files using filesystem tools."
      prompt: "Find and read the main configuration file. Call nika_complete with its contents."
      mcp: [filesystem]
      tools: [builtin]
      max_turns: 4
      max_tokens: 1000
      token_budget: 8000
      completion:
        mode: explicit

  # Use NovaNet MCP to query knowledge graph
  - id: query_knowledge
    invoke:
      tool: "novanet:query_nodes"
      params:
        node_class: "Project"
        limit: 10

  # Combine data from both MCP sources
  - id: synthesize
    depends_on: [read_config, query_knowledge]
    with:
      config: $read_config
      knowledge: $query_knowledge
    infer:
      prompt: |
        Synthesize data from two MCP sources:

        Filesystem config: {{with.config | first(1000)}}
        Knowledge graph: {{with.knowledge | first(1000)}}

        Create a project status report combining configuration data
        with knowledge graph context.
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: synthesis-report.md

  # Agent with access to ALL MCP servers
  - id: comprehensive_audit
    depends_on: [synthesize]
    with:
      report: $synthesize
    agent:
      system: |
        You are a systems auditor with access to both filesystem
        and knowledge graph tools. Use both to verify and enrich
        the project report.
      prompt: |
        Audit this report using both MCP sources:
        {{with.report | first(2000)}}

        Call nika_complete with the audited version.
      mcp: [filesystem, novanet]
      tools: [builtin]
      max_turns: 8
      max_tokens: 2000
      token_budget: 20000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 200
          on_failure: retry
    artifact:
      path: audit-report.md
```

**Explanation:**

Multiple MCP servers are configured at the workflow level and can be used in different combinations:
- `invoke:` verb calls tools from a specific MCP server by name prefix (`novanet:query_nodes`)
- `mcp: [filesystem]` gives an agent access to just the filesystem server
- `mcp: [filesystem, novanet]` gives an agent access to both servers simultaneously

This enables powerful cross-system workflows where data flows from one MCP server through the LLM and into another.

**Expected Output:** A synthesis report combining filesystem and knowledge graph data, plus an audited version.

---

## Recipe 4: MCP with Invoke Verb Patterns

**Problem:** You need to use MCP tools directly in non-agent tasks using the `invoke:` verb.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: invoke-mcp-patterns
description: "Direct MCP tool invocation patterns with invoke:"
provider: anthropic
model: claude-sonnet-4-20250514

mcp:
  novanet:
    command: "cargo"
    args: ["run", "--manifest-path", "../novanet/Cargo.toml", "--", "mcp"]

artifacts:
  dir: ./output/invoke-patterns

tasks:
  # Direct MCP invoke (no agent needed)
  - id: list_technologies
    invoke:
      tool: "novanet:query_nodes"
      params:
        node_class: "Technology"
        limit: 10
    artifact:
      path: technologies.json
      format: json

  # Chain MCP invokes with dependencies
  - id: get_rust_details
    depends_on: [list_technologies]
    invoke:
      tool: "novanet:get_node"
      params:
        name: "Rust"
        class: "Technology"

  # Use MCP results in an infer task
  - id: analyze
    depends_on: [list_technologies, get_rust_details]
    with:
      all_tech: $list_technologies
      rust: $get_rust_details
    infer:
      prompt: |
        Analyze the technology landscape:
        Technologies: {{with.all_tech}}
        Rust details: {{with.rust}}

        How does Rust fit in the broader ecosystem?
      temperature: 0.4
      max_tokens: 1000
    artifact:
      path: analysis.md

  # Use builtin invoke tools directly
  - id: log_completion
    depends_on: [analyze]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "MCP invoke pipeline complete"
```

**Explanation:**

The `invoke:` verb allows direct tool calls without an agent loop. This is more efficient than using an `agent:` when you know exactly which tool to call and with what parameters. The tool is specified as `server_name:tool_name`, and parameters are passed directly.

Builtin tools (`nika:*`) are always available and do not require an MCP configuration block. External MCP tools require their server to be configured in the `mcp:` block.

**Expected Output:** Technology data, analysis, and a completion log.

---

## Recipe 5: Hybrid MCP-Builtin Workflow

**Problem:** You need a workflow that combines Nika's builtin media tools with external MCP servers for a complete content pipeline.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: hybrid-mcp-builtin
description: "Combine builtin media tools with external MCP servers"
provider: anthropic
model: claude-sonnet-4-20250514

mcp:
  filesystem:
    command: "npx"
    args: ["-y", "@anthropic/mcp-filesystem"]

artifacts:
  dir: ./output/hybrid-pipeline

tasks:
  # Builtin: Download and process an image
  - id: download_image
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  # Builtin: Optimize through media pipeline
  - id: optimize_image
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        steps:
          - op: thumbnail
            width: 600
          - op: strip
          - op: convert
            format: webp
    artifact:
      path: optimized.webp
      format: binary

  # Builtin: Generate color palette
  - id: colors
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
        count: 5

  # Builtin: Generate a chart
  - id: metrics_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Content Pipeline Metrics"
        width: 800
        height: 500
        series:
          - name: "Processing Time (ms)"
            data: [120, 85, 45, 200, 30]
        labels: ["Download", "Resize", "Strip", "Convert", "Upload"]
    artifact:
      path: metrics-chart.png
      format: binary

  # MCP Agent: File management and organization
  - id: organize_output
    depends_on: [optimize_image, colors, metrics_chart]
    with:
      optimized: $optimize_image
      palette: $colors
      chart: $metrics_chart
    agent:
      system: |
        You are a content pipeline manager. Use filesystem and builtin tools
        to organize the pipeline output and create a manifest.
      prompt: |
        Pipeline results:
        - Optimized image: {{with.optimized}}
        - Color palette: {{with.palette}}
        - Metrics chart: {{with.chart}}

        Create a manifest file documenting all outputs.
        Call nika_complete with the manifest content.
      mcp: [filesystem]
      tools: [builtin]
      max_turns: 4
      max_tokens: 1000
      token_budget: 8000
      completion:
        mode: explicit
    artifact:
      path: pipeline-manifest.md
```

**Explanation:**

This workflow shows the natural separation between builtin and MCP tools:
- **Builtin tools** (`nika:pipeline`, `nika:dominant_color`, `nika:chart`): Used directly via `invoke:` for deterministic media processing
- **MCP tools** (`filesystem`): Used through the agent for flexible file management

The builtin tools are faster and more predictable, while MCP tools provide extensibility. Combining both creates powerful hybrid pipelines.

**Expected Output:** Optimized WebP image, chart PNG, and a pipeline manifest.

---

## Key Patterns for MCP Integration

### MCP Server Configuration

```yaml
mcp:
  server_name:
    command: "path/to/binary"
    args: ["--flag", "value"]
    env:
      API_KEY: "$ENV_VAR"
```

### Direct Invoke vs Agent Access

| Pattern | Use When |
|---------|----------|
| `invoke: tool: "server:tool"` | You know exactly what to call |
| `agent: mcp: [server]` | The LLM needs to decide which tools to use |

### Tool Naming Convention

```
server_name:tool_name   # MCP tools
nika:tool_name          # Builtin tools (always available)
```

### Common MCP Servers

| Server | Package | Purpose |
|--------|---------|---------|
| Filesystem | `@anthropic/mcp-filesystem` | File operations |
| NovaNet | Cargo binary | Knowledge graph |
| Custom | Any MCP-compatible server | Domain-specific tools |
