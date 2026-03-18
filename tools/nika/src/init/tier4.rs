//! Tier 4: MCP Integration Workflow (10)
//!
//! Prerequisites:
//! - LLM provider configured
//! - `nika mcp add novanet` (or other MCP server)
//!
//! Features covered:
//! - mcp: Server configuration
//! - invoke: MCP tool calls
//! - novanet_* tools: Knowledge graph integration

use super::WorkflowTemplate;

pub const TIER4_DIR: &str = "tier-4-mcp";

// =============================================================================
// WORKFLOW 10: MCP NovaNet Integration
// =============================================================================
pub const WORKFLOW_10_MCP_NOVANET: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🔌 WORKFLOW 10: MCP INTEGRATION                                              ║
# ║  Connect to external tools via Model Context Protocol                         ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  PREREQUISITES:                                                               ║
# ║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
# ║  │  # Add MCP server (NovaNet is our knowledge graph)                     │  ║
# ║  │  nika mcp add novanet                                                   │  ║
# ║  │                                                                        │  ║
# ║  │  # Or add other MCP servers                                            │  ║
# ║  │  nika mcp add perplexity    # Web search                                │  ║
# ║  │  nika mcp add filesystem    # File system access                        │  ║
# ║  │  nika mcp add github        # GitHub integration                        │  ║
# ║  │  nika mcp add slack         # Slack integration                         │  ║
# ║  │                                                                        │  ║
# ║  │  # Test MCP connection                                                 │  ║
# ║  │  nika mcp test novanet                                                  │  ║
# ║  └─────────────────────────────────────────────────────────────────────────┘  ║
# ║                                                                               ║
# ║  MCP ARCHITECTURE:                                                            ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │   ┌─────────────────────────────────────────────────────────────────┐  │   ║
# ║  │   │                    NIKA (MCP CLIENT)                            │  │   ║
# ║  │   │                                                                 │  │   ║
# ║  │   │    invoke:               invoke:               invoke:         │  │   ║
# ║  │   │    novanet_search        novanet_generate      novanet_write   │  │   ║
# ║  │   │         │                     │                     │          │  │   ║
# ║  │   └─────────┼─────────────────────┼─────────────────────┼──────────┘  │   ║
# ║  │             │                     │                     │             │   ║
# ║  │             └─────────────────────┴─────────────────────┘             │   ║
# ║  │                                   │                                   │   ║
# ║  │                          MCP Protocol                                 │   ║
# ║  │                      (JSON-RPC over stdio)                            │   ║
# ║  │                                   │                                   │   ║
# ║  │   ┌─────────────────────────────────────────────────────────────────┐  │   ║
# ║  │   │                   NOVANET (MCP SERVER)                          │  │   ║
# ║  │   │                                                                 │  │   ║
# ║  │   │  ┌───────────────┐  ┌────────────────┐  ┌──────────────────┐   │  │   ║
# ║  │   │  │ novanet_search│  │novanet_generate│  │  novanet_write   │   │  │   ║
# ║  │   │  │   (find)      │  │  (context)     │  │   (persist)      │   │  │   ║
# ║  │   │  └───────┬───────┘  └───────┬────────┘  └────────┬─────────┘   │  │   ║
# ║  │   │          │                  │                    │             │  │   ║
# ║  │   │          └──────────────────┴────────────────────┘             │  │   ║
# ║  │   │                             │                                   │  │   ║
# ║  │   │                     ┌───────┴───────┐                          │  │   ║
# ║  │   │                     │    NEO4J      │                          │  │   ║
# ║  │   │                     │ (Knowledge    │                          │  │   ║
# ║  │   │                     │   Graph)      │                          │  │   ║
# ║  │   │                     └───────────────┘                          │  │   ║
# ║  │   │                                                                 │  │   ║
# ║  │   └─────────────────────────────────────────────────────────────────┘  │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  NOVANET TOOLS (14 available):                                                ║
# ║  ┌─────────────────────┬──────────────────────────────────────────────────┐   ║
# ║  │ novanet_describe    │ Bootstrap: understand the graph schema           │   ║
# ║  │ novanet_introspect  │ Get NodeClass/ArcClass definitions              │   ║
# ║  │ novanet_search      │ Find nodes by text or properties                │   ║
# ║  │ novanet_traverse    │ Explore relationships from a node               │   ║
# ║  │ novanet_generate    │ Assemble LLM context for content generation     │   ║
# ║  │ novanet_atoms       │ Get locale-specific knowledge (terms, etc.)     │   ║
# ║  │ novanet_assemble    │ Custom context assembly strategy                │   ║
# ║  │ novanet_check       │ Validate before writing                         │   ║
# ║  │ novanet_write       │ Create/update nodes and arcs                    │   ║
# ║  │ novanet_audit       │ Quality checks (CSR metrics)                    │   ║
# ║  │ novanet_batch       │ Execute multiple operations in parallel         │   ║
# ║  │ novanet_query       │ Raw Cypher (LAST RESORT - analytics only)       │   ║
# ║  │ novanet_cache_*     │ Cache management                                │   ║
# ║  └─────────────────────┴──────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.12
workflow: mcp-novanet-demo

# ═══════════════════════════════════════════════════════════════════════════════
# MCP SERVER CONFIGURATION
# ═══════════════════════════════════════════════════════════════════════════════
# Define the MCP servers this workflow uses.
# Secrets like NEO4J_PASSWORD are resolved via `nika` daemon.

mcp:
  # ─────────────────────────────────────────────────────────────────────────────
  # 🧠 NovaNet - Knowledge Graph MCP Server
  # ─────────────────────────────────────────────────────────────────────────────
  novanet:
    command: cargo
    args:
      - run
      - --manifest-path
      - ../novanet/tools/novanet-mcp/Cargo.toml
      - --
      - serve
    env:
      NEO4J_URI: bolt://localhost:7687
      NEO4J_USERNAME: neo4j
      NEO4J_PASSWORD: "${nika:neo4j}"   # 🔐 Resolved by nika daemon

  # ─────────────────────────────────────────────────────────────────────────────
  # 🔍 Perplexity - Web Search (optional)
  # ─────────────────────────────────────────────────────────────────────────────
  # Uncomment if you have Perplexity API key:
  # perplexity:
  #   command: npx
  #   args: ["-y", "@anthropic/mcp-server-perplexity"]
  #   env:
  #     PERPLEXITY_API_KEY: "${nika:perplexity}"

# ═══════════════════════════════════════════════════════════════════════════════
# TASKS
# ═══════════════════════════════════════════════════════════════════════════════

tasks:
  # ───────────────────────────────────────────────────────────────────────────────
  # 📊 TASK 1: Describe Schema
  # ───────────────────────────────────────────────────────────────────────────────
  # First, understand what's in the knowledge graph

  - id: describe_schema
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        describe: schema

  # ───────────────────────────────────────────────────────────────────────────────
  # 🔍 TASK 2: Search for Entities
  # ───────────────────────────────────────────────────────────────────────────────
  # Find relevant nodes in the graph

  - id: search_entities
    depends_on: [describe_schema]
    with:
      schema: $describe_schema
    invoke:
      mcp: novanet
      tool: novanet_search
      params:
        query: "QR code"
        kinds: ["Entity"]
        mode: fulltext
        limit: 5

  # ───────────────────────────────────────────────────────────────────────────────
  # 🧭 TASK 3: Traverse Relationships
  # ───────────────────────────────────────────────────────────────────────────────
  # Explore what's connected to the found entity

  - id: traverse_entity
    depends_on: [search_entities]
    with:
      results: $search_entities
    invoke:
      mcp: novanet
      tool: novanet_traverse
      params:
        start_key: "qr-code"    # Entity key to start from
        direction: both
        max_depth: 2
        arc_families:
          - ownership
          - semantic
        include_properties: true

  # ───────────────────────────────────────────────────────────────────────────────
  # 🌍 TASK 4: Get Locale-Specific Knowledge
  # ───────────────────────────────────────────────────────────────────────────────
  # Retrieve terms and expressions for a specific locale

  - id: get_locale_atoms
    invoke:
      mcp: novanet
      tool: novanet_atoms
      params:
        locale: fr-FR
        atom_type: all
        limit: 20

  # ───────────────────────────────────────────────────────────────────────────────
  # ⭐ TASK 5: Generate LLM Context
  # ───────────────────────────────────────────────────────────────────────────────
  # This is the MAIN tool - assembles context for content generation

  - id: generate_context
    depends_on: [traverse_entity, get_locale_atoms]
    with:
      traversal: $traverse_entity
      atoms: $get_locale_atoms
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        focus_key: "qr-code"        # Starting entity
        locale: fr-FR               # Target locale
        mode: page                  # page or block
        token_budget: 4000          # Max context size
        include_examples: true

  # ───────────────────────────────────────────────────────────────────────────────
  # ✍️ TASK 6: Generate Content Using Context
  # ───────────────────────────────────────────────────────────────────────────────
  # Now use the assembled context with an LLM

  - id: generate_content
    depends_on: [generate_context]
    with:
      context: $generate_context
    infer:
      prompt: |
        Using the following knowledge context, generate a landing page intro
        paragraph in French for our QR Code AI product.

        ═══════════════════════════════════════════════════════════════════
        KNOWLEDGE CONTEXT FROM NOVANET:
        ═══════════════════════════════════════════════════════════════════
        {{with.context}}

        ═══════════════════════════════════════════════════════════════════

        Requirements:
        - Write in natural, native French
        - Use provided terminology exactly
        - Incorporate cultural expressions if available
        - Keep it under 150 words
        - Make it compelling for French readers
      temperature: 0.6
      max_tokens: 300
      system: |
        You are a native French copywriter. Use the provided NovaNet context
        to ensure terminology consistency and cultural appropriateness.

  # ───────────────────────────────────────────────────────────────────────────────
  # ✅ TASK 7: Validate Before Writing
  # ───────────────────────────────────────────────────────────────────────────────
  # Always check before writing to the graph

  - id: check_write
    depends_on: [generate_content]
    with:
      content: $generate_content
    invoke:
      mcp: novanet
      tool: novanet_check
      params:
        operation: upsert_node
        class: PageNative
        key: "landing-qr-code:fr-FR"
        properties:
          locale: fr-FR
          content: "{{with.content}}"
          generated_at: "2024-01-01T00:00:00Z"

  # ───────────────────────────────────────────────────────────────────────────────
  # 💾 TASK 8: Write to Graph (if check passes)
  # ───────────────────────────────────────────────────────────────────────────────
  # Persist the generated content

  - id: write_content
    depends_on: [check_write]
    with:
      check_result: $check_write
      content: $generate_content
    invoke:
      mcp: novanet
      tool: novanet_write
      params:
        operation: upsert_node
        class: PageNative
        key: "landing-qr-code:fr-FR"
        locale: fr-FR              # Auto-creates FOR_LOCALE arc
        properties:
          locale: fr-FR
          content: "{{with.content}}"
          generated_at: "2024-01-01T00:00:00Z"

  # ───────────────────────────────────────────────────────────────────────────────
  # 📊 TASK 9: Audit Quality
  # ───────────────────────────────────────────────────────────────────────────────
  # Check data quality after writing

  - id: audit_quality
    depends_on: [write_content]
    with:
      write_result: $write_content
    invoke:
      mcp: novanet
      tool: novanet_audit
      params:
        target: coverage
        scope:
          locale: fr-FR

# ═══════════════════════════════════════════════════════════════════════════════
# 🎓 MCP REFERENCE
# ═══════════════════════════════════════════════════════════════════════════════
#
# MCP SERVER CONFIGURATION:
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ mcp:                                                                       │
# │   my_server:                      # Server name (use in invoke: mcp:)     │
# │     command: npx                  # Executable                            │
# │     args: ["-y", "@scope/pkg"]    # Arguments                             │
# │     env:                          # Environment variables                 │
# │       API_KEY: "${nika:key}"       # Resolved by nika daemon               │
# │       STATIC_VAR: "value"         # Static value                          │
# └────────────────────────────────────────────────────────────────────────────┘
#
# INVOKE SYNTAX:
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ - id: my_task                                                             │
# │   invoke:                                                                  │
# │     mcp: server_name             # Which MCP server                       │
# │     tool: tool_name              # Which tool to call                     │
# │     params:                      # Tool parameters                        │
# │       param1: value1                                                      │
# │       param2: "{{with.binding}}"  # Can use bindings                      │
# └────────────────────────────────────────────────────────────────────────────┘
#
# NOVANET TOOL SELECTION (see mcp-tool-selection.md):
# ┌────────────────────────┬───────────────────────────────────────────────────┐
# │ Need                   │ Use This Tool                                     │
# ├────────────────────────┼───────────────────────────────────────────────────┤
# │ Find nodes             │ novanet_search (NOT novanet_query)               │
# │ Explore relationships  │ novanet_traverse (NOT novanet_query)             │
# │ LLM context            │ novanet_generate (orchestrates everything)       │
# │ Schema info            │ novanet_introspect                               │
# │ Locale knowledge       │ novanet_atoms                                    │
# │ Write data             │ novanet_check → novanet_write                    │
# │ Custom analytics ONLY  │ novanet_query (LAST RESORT)                      │
# └────────────────────────┴───────────────────────────────────────────────────┘
#
# SETUP:
# 1. nika mcp add novanet              # Add NovaNet server
# 2. nika mcp test novanet             # Verify connection
# 3. Ensure Neo4j is running          # docker-compose up -d neo4j
#
# RUN THIS WORKFLOW:
# nika run workflows/tier-4-mcp/10-mcp-novanet.nika.yaml
"##;

/// Returns all Tier 4 workflows (10)
pub fn get_tier4_workflows() -> Vec<WorkflowTemplate> {
    vec![WorkflowTemplate {
        filename: "10-mcp-novanet.nika.yaml",
        tier_dir: TIER4_DIR,
        content: WORKFLOW_10_MCP_NOVANET,
    }]
}
