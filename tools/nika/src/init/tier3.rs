//! Tier 3: Agent + File Tools Workflows (08-09)
//!
//! Prerequisites: LLM provider configured
//!
//! Features covered:
//! - agent: Multi-turn agentic loop with tools
//! - File tools: nika:read, nika:write, nika:edit, nika:glob, nika:grep
//! - stop_conditions: Automatic agent termination
//! - output: JSON Schema validation
//! - artifact: File persistence

use super::WorkflowTemplate;

pub const TIER3_DIR: &str = "tier-3-agent";

// =============================================================================
// WORKFLOW 08: Agent Basics
// =============================================================================
pub const WORKFLOW_08_AGENT_BASIC: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🐔 WORKFLOW 08: AGENT BASICS                                                 ║
# ║  Multi-turn agents with file tools and stop conditions                        ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  AGENT ARCHITECTURE:                                                          ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │   🐔 AGENT LOOP (multi-turn)                                           │   ║
# ║  │   ┌──────────────────────────────────────────────────────────────┐    │   ║
# ║  │   │                                                              │    │   ║
# ║  │   │   Turn 1: "Let me read the project files..."               │    │   ║
# ║  │   │           ├─► nika:glob (find files)                        │    │   ║
# ║  │   │           └─► nika:read (read content)                      │    │   ║
# ║  │   │                                                              │    │   ║
# ║  │   │   Turn 2: "I found 3 files. Analyzing..."                   │    │   ║
# ║  │   │           └─► (LLM reasoning)                                │    │   ║
# ║  │   │                                                              │    │   ║
# ║  │   │   Turn 3: "Writing summary..."                              │    │   ║
# ║  │   │           └─► nika:write (create file)                      │    │   ║
# ║  │   │                                                              │    │   ║
# ║  │   │   Turn 4: "TASK_COMPLETE: Summary written"                  │    │   ║
# ║  │   │           └─► (stop condition triggered)                     │    │   ║
# ║  │   │                                                              │    │   ║
# ║  │   └──────────────────────────────────────────────────────────────┘    │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  AVAILABLE TOOLS (5 file tools):                                              ║
# ║  ┌──────────────┬─────────────────────────────────────────────────────────┐   ║
# ║  │ nika:read    │ Read file contents                                      │   ║
# ║  │ nika:write   │ Create or overwrite file                                │   ║
# ║  │ nika:edit    │ Replace text in file                                    │   ║
# ║  │ nika:glob    │ Find files by pattern                                   │   ║
# ║  │ nika:grep    │ Search file contents                                    │   ║
# ║  └──────────────┴─────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  STOP CONDITIONS (automatic termination):                                     ║
# ║  • max_turns: Maximum iterations before forced stop                          ║
# ║  • stop_phrases: Text patterns that trigger graceful stop                    ║
# ║  • timeout: Maximum wall-clock time (future)                                 ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: agent-file-explorer

# ─────────────────────────────────────────────────────────────────────────────────
# 📝 SCENARIO
# ─────────────────────────────────────────────────────────────────────────────────
# An agent that explores a directory, analyzes its contents,
# and writes a summary report. Demonstrates multi-turn tool use.

tasks:
  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 1: Setup - Create some sample files for the agent to explore
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: setup_files
    exec:
      command: |
        mkdir -p ./agent-demo && \
        echo '# Project Alpha\n\nA demo project for testing.' > ./agent-demo/README.md && \
        echo '{"name": "alpha", "version": "1.0.0"}' > ./agent-demo/package.json && \
        echo 'def hello(): return "Hello from Python"' > ./agent-demo/main.py
      shell: true

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 2: Agent - Explore and Analyze
  # ═══════════════════════════════════════════════════════════════════════════════
  # The agent autonomously decides what tools to use

  - id: explore_agent
    agent:
      prompt: |
        You are a code analysis agent. Your task is to:

        1. Explore the ./agent-demo directory
        2. Read each file you find
        3. Analyze the project structure
        4. Write a summary report to ./agent-demo/ANALYSIS.md

        Use the available tools to accomplish this task.
        When you're done, include "TASK_COMPLETE" in your response.

      # ─────────────────────────────────────────────────────────────────────────
      # 🛠️ TOOLS: What the agent can use
      # ─────────────────────────────────────────────────────────────────────────
      tools:
        - nika:glob     # Find files by pattern
        - nika:read     # Read file contents
        - nika:write    # Create/overwrite files

      # ─────────────────────────────────────────────────────────────────────────
      # ⚙️ AGENT CONFIGURATION
      # ─────────────────────────────────────────────────────────────────────────
      max_turns: 10                    # 🔄 Maximum conversation turns
      temperature: 0.3                 # 🎯 Lower = more focused

      # ─────────────────────────────────────────────────────────────────────────
      # 🛑 STOP CONDITIONS
      # ─────────────────────────────────────────────────────────────────────────
      stop_conditions:
        phrases:                       # 🏁 Stop when agent says these
          - "TASK_COMPLETE"
          - "ANALYSIS COMPLETE"
          - "I have finished"

      # ─────────────────────────────────────────────────────────────────────────
      # 🧠 SYSTEM PROMPT
      # ─────────────────────────────────────────────────────────────────────────
      system: |
        You are a meticulous code analyst. Always:
        1. Explore thoroughly before drawing conclusions
        2. Read files completely, don't assume
        3. Provide structured, actionable analysis
        4. End with TASK_COMPLETE when done

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 3: Verify Output
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: verify_output
    use:
      agent_result: $explore_agent
    exec:
      command: "cat ./agent-demo/ANALYSIS.md 2>/dev/null || echo 'No analysis file found'"
      shell: true

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 4: Cleanup
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: cleanup
    use:
      verification: $verify_output
    exec:
      command: "rm -rf ./agent-demo"
      shell: true

# ═══════════════════════════════════════════════════════════════════════════════
# FLOWS
# ═══════════════════════════════════════════════════════════════════════════════

flows:
  - source: setup_files
    target: explore_agent
  - source: explore_agent
    target: verify_output
  - source: verify_output
    target: cleanup

# ═══════════════════════════════════════════════════════════════════════════════
# 🎓 AGENT REFERENCE
# ═══════════════════════════════════════════════════════════════════════════════
#
# AGENT SYNTAX:
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ - id: my_agent                                                             │
# │   agent:                                                                    │
# │     prompt: "Your task..."        # 📝 What the agent should do           │
# │     tools:                         # 🛠️ Available tools                    │
# │       - nika:read                                                          │
# │       - nika:write                                                         │
# │     max_turns: 10                  # 🔄 Max iterations                     │
# │     temperature: 0.5               # 🎲 Creativity level                   │
# │     system: "You are..."           # 🧠 Persona/role                       │
# │     stop_conditions:               # 🛑 When to stop                       │
# │       phrases: ["DONE"]                                                    │
# └────────────────────────────────────────────────────────────────────────────┘
#
# FILE TOOLS REFERENCE:
# ┌──────────────┬─────────────────────────────────────────────────────────────┐
# │ Tool         │ Parameters                                                   │
# ├──────────────┼─────────────────────────────────────────────────────────────┤
# │ nika:read    │ { "file_path": "./file.txt" }                               │
# │ nika:write   │ { "file_path": "./out.txt", "content": "..." }              │
# │ nika:edit    │ { "file_path": "./f.txt", "old": "x", "new": "y" }          │
# │ nika:glob    │ { "pattern": "**/*.rs", "path": "./" }                      │
# │ nika:grep    │ { "pattern": "TODO", "path": "./src" }                      │
# └──────────────┴─────────────────────────────────────────────────────────────┘
#
# AGENT vs INFER:
# ┌────────────┬──────────────────────────────────────────────────────────────┐
# │            │ infer:              │ agent:                                 │
# ├────────────┼─────────────────────┼────────────────────────────────────────┤
# │ Turns      │ Single              │ Multi-turn (up to max_turns)           │
# │ Tools      │ None                │ Can use tools autonomously             │
# │ Autonomy   │ Direct answer       │ Decides own approach                   │
# │ Best for   │ Simple generation   │ Complex, multi-step tasks              │
# └────────────┴─────────────────────┴────────────────────────────────────────┘
#
# RUN THIS WORKFLOW:
# nika run workflows/tier-3-agent/08-agent-basic.nika.yaml
"##;

// =============================================================================
// WORKFLOW 09: Structured Output + Artifacts
// =============================================================================
pub const WORKFLOW_09_STRUCTURED_OUTPUT: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  📐 WORKFLOW 09: STRUCTURED OUTPUT + ARTIFACTS                                ║
# ║  JSON Schema validation and file persistence                                  ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  STRUCTURED OUTPUT FLOW:                                                      ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐               │   ║
# ║  │   │   INPUT     │───►│    LLM      │───►│  VALIDATE   │               │   ║
# ║  │   │   PROMPT    │    │  GENERATE   │    │   SCHEMA    │               │   ║
# ║  │   └─────────────┘    └─────────────┘    └──────┬──────┘               │   ║
# ║  │                                                │                       │   ║
# ║  │                           ┌────────────────────┴────────────────────┐  │   ║
# ║  │                           │                                         │  │   ║
# ║  │                           ▼                                         ▼  │   ║
# ║  │                    ┌─────────────┐                          ┌──────────┴┐ ║
# ║  │                    │   ✅ VALID  │                          │ ❌ RETRY  │ ║
# ║  │                    │   OUTPUT    │                          │ (3 tries) │ ║
# ║  │                    └──────┬──────┘                          └───────────┘ ║
# ║  │                           │                                               │   ║
# ║  │                           ▼                                               │   ║
# ║  │                    ┌─────────────┐                                       │   ║
# ║  │                    │  ARTIFACT   │                                       │   ║
# ║  │                    │  (persist)  │                                       │   ║
# ║  │                    └─────────────┘                                       │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  JSON SCHEMA VALIDATION:                                                      ║
# ║  • Enforces specific structure on LLM output                                  ║
# ║  • Auto-retry on validation failure (up to 3 times)                           ║
# ║  • Injects schema into prompt for better compliance                           ║
# ║                                                                               ║
# ║  ARTIFACTS:                                                                   ║
# ║  • Persist task outputs to files                                              ║
# ║  • Template variables: {{task_id}}, {{date}}, {{timestamp}}                   ║
# ║  • Atomic writes (crash-safe)                                                 ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: structured-output-artifacts

tasks:
  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 1: Generate Product Data with Schema Validation
  # ═══════════════════════════════════════════════════════════════════════════════
  # The LLM MUST output JSON matching this exact schema

  - id: generate_product
    infer:
      prompt: |
        Generate a product listing for a futuristic smart home device.
        Be creative with the features and pricing.
      temperature: 0.7
      max_tokens: 500

    # ─────────────────────────────────────────────────────────────────────────────
    # 📐 OUTPUT SCHEMA - JSON Schema validation
    # ─────────────────────────────────────────────────────────────────────────────
    # The LLM output MUST match this schema. If not, Nika retries up to 3 times.

    output:
      schema:
        type: object
        required:
          - name
          - tagline
          - price
          - features
          - specifications
        properties:
          name:
            type: string
            description: "Product name (2-4 words)"
            minLength: 3
            maxLength: 50
          tagline:
            type: string
            description: "Marketing tagline (catchy, <100 chars)"
            maxLength: 100
          price:
            type: object
            required: [amount, currency]
            properties:
              amount:
                type: number
                minimum: 0
              currency:
                type: string
                enum: [USD, EUR, GBP, JPY]
          features:
            type: array
            description: "Key product features"
            minItems: 3
            maxItems: 6
            items:
              type: object
              required: [title, description]
              properties:
                title:
                  type: string
                description:
                  type: string
          specifications:
            type: object
            properties:
              dimensions:
                type: string
              weight:
                type: string
              connectivity:
                type: array
                items:
                  type: string
              power:
                type: string

    # ─────────────────────────────────────────────────────────────────────────────
    # 💾 ARTIFACT - Save output to file
    # ─────────────────────────────────────────────────────────────────────────────
    # The validated output is persisted to a file.

    artifact:
      path: ./output/products/{{task_id}}_{{date}}.json
      format: json

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 2: Generate Marketing Content with Different Schema
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: generate_marketing
    use:
      product: $generate_product
    infer:
      prompt: |
        Create marketing content for this product:

        {{use.product}}

        Generate social media posts for different platforms.
      temperature: 0.6
      max_tokens: 600

    output:
      schema:
        type: object
        required: [twitter, linkedin, instagram]
        properties:
          twitter:
            type: object
            required: [post, hashtags]
            properties:
              post:
                type: string
                maxLength: 280
              hashtags:
                type: array
                items:
                  type: string
                maxItems: 5
          linkedin:
            type: object
            required: [headline, body]
            properties:
              headline:
                type: string
                maxLength: 100
              body:
                type: string
                minLength: 100
                maxLength: 700
          instagram:
            type: object
            required: [caption, suggested_image]
            properties:
              caption:
                type: string
              suggested_image:
                type: string
                description: "Description for AI image generation"

    artifact:
      path: ./output/marketing/{{task_id}}_{{date}}.json
      format: json

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 3: Agent with Structured Output
  # ═══════════════════════════════════════════════════════════════════════════════
  # Even agents can have structured output requirements

  - id: analyze_agent
    use:
      product: $generate_product
      marketing: $generate_marketing
    agent:
      prompt: |
        Analyze the product and marketing content:

        PRODUCT:
        {{use.product}}

        MARKETING:
        {{use.marketing}}

        Your analysis should:
        1. Check message consistency across platforms
        2. Identify potential improvements
        3. Rate the overall marketing quality

        Use the file tools if needed to research competitors.
      tools:
        - nika:glob
        - nika:read
      max_turns: 5
      temperature: 0.4
      stop_conditions:
        phrases: ["ANALYSIS_COMPLETE"]

    # Agent output also validated against schema
    output:
      schema:
        type: object
        required: [consistency_score, improvements, overall_rating]
        properties:
          consistency_score:
            type: number
            minimum: 0
            maximum: 100
            description: "Message consistency across platforms (0-100)"
          improvements:
            type: array
            items:
              type: object
              required: [platform, suggestion]
              properties:
                platform:
                  type: string
                  enum: [twitter, linkedin, instagram, general]
                suggestion:
                  type: string
          overall_rating:
            type: object
            required: [score, summary]
            properties:
              score:
                type: number
                minimum: 1
                maximum: 10
              summary:
                type: string
                maxLength: 200

    artifact:
      path: ./output/analysis/{{task_id}}_{{timestamp}}.json
      format: json

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 4: Combine and Export Final Report
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: final_report
    use:
      product: $generate_product
      marketing: $generate_marketing
      analysis: $analyze_agent
    infer:
      prompt: |
        Create a final executive summary combining all our work:

        PRODUCT DATA:
        {{use.product}}

        MARKETING CONTENT:
        {{use.marketing}}

        QUALITY ANALYSIS:
        {{use.analysis}}

        Summarize everything in a brief executive report.
      temperature: 0.3
      max_tokens: 400

    artifact:
      path: ./output/reports/executive_summary_{{date}}.md
      format: markdown

# ═══════════════════════════════════════════════════════════════════════════════
# FLOWS
# ═══════════════════════════════════════════════════════════════════════════════

flows:
  - source: generate_product
    target: generate_marketing
  - source: generate_marketing
    target: analyze_agent
  - source: analyze_agent
    target: final_report

# ═══════════════════════════════════════════════════════════════════════════════
# 🎓 STRUCTURED OUTPUT REFERENCE
# ═══════════════════════════════════════════════════════════════════════════════
#
# OUTPUT SCHEMA SYNTAX:
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ output:                                                                    │
# │   schema:                          # JSON Schema definition               │
# │     type: object                                                          │
# │     required: [field1, field2]     # Required fields                      │
# │     properties:                                                           │
# │       field1:                                                             │
# │         type: string               # string, number, boolean, array, obj  │
# │         minLength: 1                                                      │
# │         maxLength: 100                                                    │
# │       field2:                                                             │
# │         type: number                                                      │
# │         minimum: 0                                                        │
# │         maximum: 100                                                      │
# │       field3:                                                             │
# │         type: array                                                       │
# │         items:                                                            │
# │           type: string                                                    │
# │         minItems: 1                                                       │
# │         maxItems: 10                                                      │
# │       field4:                                                             │
# │         type: string                                                      │
# │         enum: [option1, option2]   # Allowed values                       │
# └────────────────────────────────────────────────────────────────────────────┘
#
# ARTIFACT SYNTAX:
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ artifact:                                                                  │
# │   path: ./output/{{task_id}}_{{date}}.json   # File path with templates   │
# │   format: json                                # json, yaml, markdown, raw │
# └────────────────────────────────────────────────────────────────────────────┘
#
# TEMPLATE VARIABLES:
# ┌─────────────────┬──────────────────────────────────────────────────────────┐
# │ {{task_id}}     │ The task's id (e.g., "generate_product")                │
# │ {{date}}        │ Current date (YYYY-MM-DD)                               │
# │ {{timestamp}}   │ Unix timestamp                                          │
# │ {{workflow}}    │ Workflow name                                           │
# │ {{run_id}}      │ Unique run identifier                                   │
# └─────────────────┴──────────────────────────────────────────────────────────┘
#
# VALIDATION BEHAVIOR:
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ 1. LLM generates output                                                   │
# │ 2. Nika validates against schema                                          │
# │ 3. If invalid → retry with error feedback (up to 3 times)                │
# │ 4. If still invalid → task fails with NIKA-060/061 error                 │
# │ 5. If valid → save artifact (if configured)                               │
# └────────────────────────────────────────────────────────────────────────────┘
#
# SETUP:
# mkdir -p ./output/products ./output/marketing ./output/analysis ./output/reports
#
# RUN THIS WORKFLOW:
# nika run workflows/tier-3-agent/09-structured-output.nika.yaml
"##;

/// Returns all Tier 3 workflows (08-09)
pub fn get_tier3_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "08-agent-basic.nika.yaml",
            tier_dir: TIER3_DIR,
            content: WORKFLOW_08_AGENT_BASIC,
        },
        WorkflowTemplate {
            filename: "09-structured-output.nika.yaml",
            tier_dir: TIER3_DIR,
            content: WORKFLOW_09_STRUCTURED_OUTPUT,
        },
    ]
}
