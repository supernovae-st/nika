//! Exercise content for Levels 7-12 (22 advanced exercises)
//!
//! Swiss Knife (3), Gone Rogue (3), Data Heist (4), Open Protocol (3),
//! Pixel Pirate (4), SuperNovae boss (5).
//!
//! Templates have `# TODO:` markers; solutions are complete YAML.
//! LLM exercises use `{{PROVIDER}}`/`{{MODEL}}` placeholders.

use super::exercises::ExerciseContent;

// ─── All 22 advanced exercises ───────────────────────────────────────────────

pub static EXERCISES_ADVANCED: &[ExerciseContent] = &[
    // ═══════════════════════════════════════════════════════════════════════════
    // LEVEL 07: Swiss Knife — Builtin tools via invoke:
    // ═══════════════════════════════════════════════════════════════════════════
    ExerciseContent {
        level_slug: "swiss-knife",
        exercise_num: 1,
        filename: "01-core-builtins.nika.yaml",
        template: SWISS_KNIFE_01_TEMPLATE,
        solution: SWISS_KNIFE_01_SOLUTION,
    },
    ExerciseContent {
        level_slug: "swiss-knife",
        exercise_num: 2,
        filename: "02-file-tools.nika.yaml",
        template: SWISS_KNIFE_02_TEMPLATE,
        solution: SWISS_KNIFE_02_SOLUTION,
    },
    ExerciseContent {
        level_slug: "swiss-knife",
        exercise_num: 3,
        filename: "03-sub-workflows.nika.yaml",
        template: SWISS_KNIFE_03_TEMPLATE,
        solution: SWISS_KNIFE_03_SOLUTION,
    },
    // ═══════════════════════════════════════════════════════════════════════════
    // LEVEL 08: Gone Rogue — Autonomous agents
    // ═══════════════════════════════════════════════════════════════════════════
    ExerciseContent {
        level_slug: "gone-rogue",
        exercise_num: 1,
        filename: "01-basic-agent.nika.yaml",
        template: GONE_ROGUE_01_TEMPLATE,
        solution: GONE_ROGUE_01_SOLUTION,
    },
    ExerciseContent {
        level_slug: "gone-rogue",
        exercise_num: 2,
        filename: "02-agent-skills.nika.yaml",
        template: GONE_ROGUE_02_TEMPLATE,
        solution: GONE_ROGUE_02_SOLUTION,
    },
    ExerciseContent {
        level_slug: "gone-rogue",
        exercise_num: 3,
        filename: "03-agent-guardrails.nika.yaml",
        template: GONE_ROGUE_03_TEMPLATE,
        solution: GONE_ROGUE_03_SOLUTION,
    },
    // ═══════════════════════════════════════════════════════════════════════════
    // LEVEL 09: Data Heist — Advanced fetch: extraction
    // ═══════════════════════════════════════════════════════════════════════════
    ExerciseContent {
        level_slug: "data-heist",
        exercise_num: 1,
        filename: "01-fetch-markdown.nika.yaml",
        template: DATA_HEIST_01_TEMPLATE,
        solution: DATA_HEIST_01_SOLUTION,
    },
    ExerciseContent {
        level_slug: "data-heist",
        exercise_num: 2,
        filename: "02-fetch-metadata.nika.yaml",
        template: DATA_HEIST_02_TEMPLATE,
        solution: DATA_HEIST_02_SOLUTION,
    },
    ExerciseContent {
        level_slug: "data-heist",
        exercise_num: 3,
        filename: "03-fetch-jsonpath.nika.yaml",
        template: DATA_HEIST_03_TEMPLATE,
        solution: DATA_HEIST_03_SOLUTION,
    },
    ExerciseContent {
        level_slug: "data-heist",
        exercise_num: 4,
        filename: "04-fetch-binary.nika.yaml",
        template: DATA_HEIST_04_TEMPLATE,
        solution: DATA_HEIST_04_SOLUTION,
    },
    // ═══════════════════════════════════════════════════════════════════════════
    // LEVEL 10: Open Protocol — MCP integration
    // ═══════════════════════════════════════════════════════════════════════════
    ExerciseContent {
        level_slug: "open-protocol",
        exercise_num: 1,
        filename: "01-mcp-basics.nika.yaml",
        template: OPEN_PROTOCOL_01_TEMPLATE,
        solution: OPEN_PROTOCOL_01_SOLUTION,
    },
    ExerciseContent {
        level_slug: "open-protocol",
        exercise_num: 2,
        filename: "02-mcp-tools.nika.yaml",
        template: OPEN_PROTOCOL_02_TEMPLATE,
        solution: OPEN_PROTOCOL_02_SOLUTION,
    },
    ExerciseContent {
        level_slug: "open-protocol",
        exercise_num: 3,
        filename: "03-mcp-novanet.nika.yaml",
        template: OPEN_PROTOCOL_03_TEMPLATE,
        solution: OPEN_PROTOCOL_03_SOLUTION,
    },
    // ═══════════════════════════════════════════════════════════════════════════
    // LEVEL 11: Pixel Pirate — Media pipeline
    // ═══════════════════════════════════════════════════════════════════════════
    ExerciseContent {
        level_slug: "pixel-pirate",
        exercise_num: 1,
        filename: "01-media-import.nika.yaml",
        template: PIXEL_PIRATE_01_TEMPLATE,
        solution: PIXEL_PIRATE_01_SOLUTION,
    },
    ExerciseContent {
        level_slug: "pixel-pirate",
        exercise_num: 2,
        filename: "02-media-transform.nika.yaml",
        template: PIXEL_PIRATE_02_TEMPLATE,
        solution: PIXEL_PIRATE_02_SOLUTION,
    },
    ExerciseContent {
        level_slug: "pixel-pirate",
        exercise_num: 3,
        filename: "03-media-pipeline.nika.yaml",
        template: PIXEL_PIRATE_03_TEMPLATE,
        solution: PIXEL_PIRATE_03_SOLUTION,
    },
    ExerciseContent {
        level_slug: "pixel-pirate",
        exercise_num: 4,
        filename: "04-vision.nika.yaml",
        template: PIXEL_PIRATE_04_TEMPLATE,
        solution: PIXEL_PIRATE_04_SOLUTION,
    },
    // ═══════════════════════════════════════════════════════════════════════════
    // LEVEL 12: SuperNovae — BOSS (all 5 verbs, everything combined)
    // ═══════════════════════════════════════════════════════════════════════════
    ExerciseContent {
        level_slug: "supernovae",
        exercise_num: 1,
        filename: "01-seo-mega-audit.nika.yaml",
        template: SUPERNOVAE_01_TEMPLATE,
        solution: SUPERNOVAE_01_SOLUTION,
    },
    ExerciseContent {
        level_slug: "supernovae",
        exercise_num: 2,
        filename: "02-image-pipeline.nika.yaml",
        template: SUPERNOVAE_02_TEMPLATE,
        solution: SUPERNOVAE_02_SOLUTION,
    },
    ExerciseContent {
        level_slug: "supernovae",
        exercise_num: 3,
        filename: "03-content-factory.nika.yaml",
        template: SUPERNOVAE_03_TEMPLATE,
        solution: SUPERNOVAE_03_SOLUTION,
    },
    ExerciseContent {
        level_slug: "supernovae",
        exercise_num: 4,
        filename: "04-research-agent.nika.yaml",
        template: SUPERNOVAE_04_TEMPLATE,
        solution: SUPERNOVAE_04_SOLUTION,
    },
    ExerciseContent {
        level_slug: "supernovae",
        exercise_num: 5,
        filename: "05-full-stack.nika.yaml",
        template: SUPERNOVAE_05_TEMPLATE,
        solution: SUPERNOVAE_05_SOLUTION,
    },
];

// ─── Lookup functions ────────────────────────────────────────────────────────

/// Get all advanced exercises for a given level slug
pub fn get_advanced_exercises(level_slug: &str) -> Vec<&'static ExerciseContent> {
    EXERCISES_ADVANCED
        .iter()
        .filter(|e| e.level_slug == level_slug)
        .collect()
}

/// Get a specific advanced exercise by level slug and exercise number
pub fn get_advanced_exercise(
    level_slug: &str,
    exercise_num: u8,
) -> Option<&'static ExerciseContent> {
    EXERCISES_ADVANCED
        .iter()
        .find(|e| e.level_slug == level_slug && e.exercise_num == exercise_num)
}

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 07: SWISS KNIFE
// ═════════════════════════════════════════════════════════════════════════════

// ── 07-01: Core Builtins ─────────────────────────────────────────────────────

const SWISS_KNIFE_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 7 — EXERCISE 1: Core Builtin Tools
# =============================================================================
#
# Use invoke: to call nika:log, nika:sleep, nika:assert, nika:emit.
# These run INSIDE the Nika process — no API cost, no network.
#
# TOOLS COVERED:
#   nika:sleep  — Pause execution (humantime: "1s", "500ms", "2m")
#   nika:log    — Structured log entry (trace|debug|info|warn|error)
#   nika:emit   — Custom event with JSON payload
#   nika:assert — Condition check (fails task if false)
#
# INSTRUCTIONS:
#   1. Fill in the TODO markers below
#   2. Run: nika check 01-core-builtins.nika.yaml
#   3. Run: nika run 01-core-builtins.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # TODO: Use nika:sleep to pause for 1 second
  # Hint: params: { duration: "1s" }
  - id: pause
    invoke:
      tool: "nika:sleep"
      params:
        duration: "TODO: humantime format (e.g. 1s, 500ms, 2m)"

  # TODO: Use nika:log to write an info-level message
  - id: log_start
    depends_on: [pause]
    invoke:
      tool: "nika:log"
      params:
        level: "TODO: log level"
        message: "TODO: your message here"

  # TODO: Use nika:emit to emit a custom event with a payload
  - id: emit_progress
    depends_on: [log_start]
    invoke:
      tool: "nika:emit"
      params:
        name: "TODO: event name"
        payload:
          status: "TODO: custom fields"

  # TODO: Use nika:assert to check a condition
  - id: check
    depends_on: [emit_progress]
    invoke:
      tool: "nika:assert"
      params:
        condition: "TODO: true or false"
        message: "TODO: assertion message"
"##;

const SWISS_KNIFE_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 7 — EXERCISE 1: Core Builtin Tools
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: pause
    invoke:
      tool: "nika:sleep"
      params:
        duration: "1s"

  - id: log_start
    depends_on: [pause]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Swiss Knife exercise started — all core builtins demo!"

  - id: emit_progress
    depends_on: [log_start]
    invoke:
      tool: "nika:emit"
      params:
        name: "exercise_progress"
        payload:
          level: 7
          exercise: 1
          status: "in_progress"
          tools_covered: 4

  - id: check
    depends_on: [emit_progress]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "All 4 core builtins executed successfully"
"##;

// ── 07-02: File Tools ────────────────────────────────────────────────────────

const SWISS_KNIFE_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 7 — EXERCISE 2: File Tools
# =============================================================================
#
# The 5 file tools: nika:write, nika:read, nika:edit, nika:grep, nika:glob.
# Chain them: write a file, read it back, edit it, search it, find it.
#
# INSTRUCTIONS:
#   1. Fill in the TODO markers below
#   2. Run: nika run 02-file-tools.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # TODO: Write a file using nika:write
  - id: write_file
    invoke:
      tool: "nika:write"
      params:
        file_path: "TODO: output path (e.g. .scratch/report.txt)"
        content: "TODO: file content"

  # TODO: Read the file back using nika:read
  - id: read_file
    depends_on: [write_file]
    invoke:
      tool: "nika:read"
      params:
        file_path: "TODO: same path as above"

  # TODO: Edit the file using nika:edit (find-and-replace)
  - id: edit_file
    depends_on: [read_file]
    invoke:
      tool: "nika:edit"
      params:
        file_path: "TODO: same path"
        old_string: "TODO: text to find (must be unique in file)"
        new_string: "TODO: replacement text"

  # TODO: Search for the edit using nika:grep
  - id: grep_file
    depends_on: [edit_file]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "TODO: regex pattern to search"
        path: "TODO: directory to search in"

  # TODO: Find all .txt files using nika:glob
  - id: glob_files
    depends_on: [edit_file]
    invoke:
      tool: "nika:glob"
      params:
        pattern: "TODO: glob pattern (e.g. .scratch/*.txt)"
"##;

const SWISS_KNIFE_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 7 — EXERCISE 2: File Tools
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # Ensure idempotent: remove file from previous runs
  - id: cleanup
    exec:
      command: rm -f .scratch/swiss-knife-report.txt

  - id: write_file
    depends_on: [cleanup]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/swiss-knife-report.txt"
        content: |
          Swiss Knife Report
          ==================
          Status: draft
          Tools tested: write, read, edit, grep, glob

  - id: read_file
    depends_on: [write_file]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/swiss-knife-report.txt"

  - id: edit_file
    depends_on: [read_file]
    invoke:
      tool: "nika:edit"
      params:
        file_path: ".scratch/swiss-knife-report.txt"
        old_string: "Status: draft"
        new_string: "Status: verified by nika:edit"

  - id: grep_file
    depends_on: [edit_file]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "verified by nika:edit"
        path: ".scratch"

  - id: glob_files
    depends_on: [edit_file]
    invoke:
      tool: "nika:glob"
      params:
        pattern: "*.txt"
        path: ".scratch"
"##;

// ── 07-03: Sub-Workflows ─────────────────────────────────────────────────────

const SWISS_KNIFE_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 7 — EXERCISE 3: Sub-Workflows
# =============================================================================
#
# nika:run executes a nested workflow — workflow composition!
# The parent workflow waits for the child to complete and receives its output.
#
# INSTRUCTIONS:
#   1. Fill in the TODO markers below
#   2. Make sure 02-file-tools.nika.yaml exists in the same directory
#   3. Run: nika run 03-sub-workflows.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # TODO: Log that we are starting the parent workflow
  - id: log_parent_start
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "TODO: starting message"

  # TODO: Use nika:run to invoke a child workflow
  # Hint: params: { workflow: "02-file-tools.nika.yaml" }
  - id: run_child
    depends_on: [log_parent_start]
    invoke:
      tool: "nika:run"
      params:
        workflow: "TODO: path to child workflow"

  # TODO: Log the result from the child workflow
  - id: log_parent_done
    depends_on: [run_child]
    with:
      child_result: $run_child
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "TODO: use {{with.child_result}} in your message"
"##;

const SWISS_KNIFE_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 7 — EXERCISE 3: Sub-Workflows
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: log_parent_start
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Parent workflow starting — will invoke child workflow"

  - id: run_child
    depends_on: [log_parent_start]
    invoke:
      tool: "nika:run"
      params:
        workflow: "02-file-tools.nika.yaml"

  - id: log_parent_done
    depends_on: [run_child]
    with:
      child_result: $run_child
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Child workflow completed: {{with.child_result}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 08: GONE ROGUE
// ═════════════════════════════════════════════════════════════════════════════

// ── 08-01: Basic Agent ───────────────────────────────────────────────────────

const GONE_ROGUE_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 8 — EXERCISE 1: Basic Agent
# =============================================================================
#
# agent: creates an LLM that loops autonomously, calling tools until done.
#
# HOW IT WORKS:
#   1. Agent receives a prompt (its mission)
#   2. LLM decides which tool to call (or responds with text)
#   3. Tool result is fed back to the LLM
#   4. Loop continues until: nika:complete, max_turns, or token_budget
#
# KEY FIELDS (inside agent: block):
#   prompt:        — The agent's mission
#   tools:         — Available tools: [builtin], [nika:log, nika:complete], etc.
#   max_turns:     — Max loop iterations (safety limit)
#   max_tokens:    — Max tokens per LLM response
#   token_budget:  — Total token budget across ALL turns
#   tool_choice:   — "auto" (default), "required", "none"
#
# provider: and model: go at WORKFLOW level (inherited by all tasks)
#
# INSTRUCTIONS:
#   1. Fill in the TODO markers below
#   2. Run: nika run 01-basic-agent.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: analyzer
    agent:
      prompt: |
        You are a project analysis agent. Your mission:
        1. Log "Starting analysis" using nika_log
        2. Analyze the Nika workflow engine architecture:
           - 5 verbs: infer, exec, fetch, invoke, agent
           - DAG scheduler for parallel execution
           - YAML DSL with schema validation
        3. Log your key findings
        4. Call nika_complete with a brief summary report
      # TODO: Set the tools the agent can use
      # Hint: [builtin] gives access to all nika:* tools
      tools: [builtin]  # TODO: change this value
      # TODO: Set safety limits
      max_turns: 5  # TODO: change this value (e.g. 8)
      max_tokens: 500  # TODO: change this value (e.g. 800)
      token_budget: 5000  # TODO: change this value (e.g. 6000)
"##;

const GONE_ROGUE_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 8 — EXERCISE 1: Basic Agent
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: analyzer
    agent:
      prompt: |
        You are a project analysis agent. Your mission:
        1. Log "Starting analysis" using nika_log
        2. Analyze the Nika workflow engine architecture:
           - 5 verbs: infer, exec, fetch, invoke, agent
           - DAG scheduler for parallel execution
           - YAML DSL with schema validation
        3. Log your key findings
        4. Call nika_complete with a brief summary report
      tools: [builtin]
      max_turns: 8
      max_tokens: 800
      token_budget: 6000
      tool_choice: auto
    artifact:
      path: output/analysis-report.md
"##;

// ── 08-02: Agent Skills ──────────────────────────────────────────────────────

const GONE_ROGUE_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 8 — EXERCISE 2: Agent Skills & Completion
# =============================================================================
#
# completion: controls HOW the agent signals "I'm done":
#   explicit — Agent must call nika:complete (recommended for complex tasks)
#   natural  — Completes when LLM stops making tool calls
#   pattern  — Completes when output matches a regex
#
# Chain agents: second agent receives first agent's output via with: bindings.
#
# INSTRUCTIONS:
#   1. Set completion modes on both agents
#   2. Run: nika run 02-agent-skills.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  # Agent 1: Research (explicit completion)
  - id: explicit_agent
    agent:
      prompt: |
        Research 3 innovative use cases for YAML workflow engines.
        For each, provide a title and 2-sentence description.
        Log each use case with nika_log.
        When done, call nika_complete with your ranked list.
      tools: [builtin]
      max_turns: 6
      max_tokens: 800
      token_budget: 5000
      # TODO: Set completion mode to explicit
      completion:
        mode: explicit  # TODO: change this value

  # Agent 2: Refine (chained — uses output from Agent 1)
  - id: refine_agent
    depends_on: [explicit_agent]
    with:
      research: $explicit_agent
    agent:
      prompt: |
        Review these use cases and pick the single best one:
        {{with.research}}
        Expand it into a 200-word pitch.
        Call nika_complete with your final pitch.
      tools: [builtin]
      max_turns: 4
      max_tokens: 600
      token_budget: 3000
      # TODO: Set completion mode with signal configuration
      completion:
        mode: explicit  # TODO: change this value
        signal:
          tool: nika:complete
          fields:
            required: [result]
            optional: [confidence]
"##;

const GONE_ROGUE_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 8 — EXERCISE 2: Agent Skills & Completion
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: explicit_agent
    agent:
      prompt: |
        Research 3 innovative use cases for YAML workflow engines.
        For each, provide a title and 2-sentence description.
        Log each use case with nika_log.
        When done, call nika_complete with your ranked list.
      tools: [builtin]
      max_turns: 6
      max_tokens: 800
      token_budget: 5000
      completion:
        mode: explicit

  - id: refine_agent
    depends_on: [explicit_agent]
    with:
      research: $explicit_agent
    agent:
      prompt: |
        Review these use cases and pick the single best one:
        {{with.research}}
        Expand it into a 200-word pitch.
        Call nika_complete with your final pitch.
      tools: [builtin]
      max_turns: 4
      max_tokens: 600
      token_budget: 3000
      completion:
        mode: explicit
        signal:
          tool: nika:complete
          fields:
            required: [result]
            optional: [confidence]
    artifact:
      path: output/best-use-case-pitch.md
"##;

// ── 08-03: Agent Guardrails ──────────────────────────────────────────────────

const GONE_ROGUE_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 8 — EXERCISE 3: Agent Guardrails & Limits
# =============================================================================
#
# guardrails: validate agent output BEFORE accepting it:
#   type: length — Check word count bounds (min_words, max_words)
#   type: regex  — Check output matches a pattern
#   type: schema — Validate JSON against a JSON Schema
#   type: llm    — Use a secondary LLM for validation
#
# limits: control cost and prevent runaway agents:
#   max_turns:         — Hard cap on loop iterations
#   max_cost_usd:      — Dollar cost ceiling
#   max_duration_secs: — Wall-clock timeout
#
# INSTRUCTIONS:
#   1. Add guardrails (length + regex) and limits
#   2. Run: nika run 03-agent-guardrails.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: writer_agent
    agent:
      prompt: |
        Write a technical article about declarative workflow engines.
        Requirements:
        - Between 200 and 400 words
        - Must start with "In the era of"
        - Must include at least one code example
        Log your progress, then call nika_complete with the article.
      tools: [builtin]
      max_turns: 8
      max_tokens: 1000
      token_budget: 8000
      # TODO: Add guardrails to validate output quality
      guardrails:
        # TODO: Length guardrail (200-400 words)
        - type: length  # TODO: change this value
          min_words: 100  # TODO: change this value
          max_words: 500  # TODO: change this value
          on_failure: retry
        # TODO: Regex guardrail (must start with "In the era of")
        - type: regex  # TODO: change this value
          pattern: ".*"  # TODO: change this regex
          message: "Output did not match expected pattern"  # TODO: change this message
          on_failure: retry
      # TODO: Add cost control limits
      limits:
        max_turns: 10  # TODO: change this value
        max_cost_usd: 0.50  # TODO: change this value
        max_duration_secs: 120  # TODO: change this value
      completion:
        mode: explicit
"##;

const GONE_ROGUE_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 8 — EXERCISE 3: Agent Guardrails & Limits
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: writer_agent
    agent:
      prompt: |
        Write a technical article about declarative workflow engines.
        Requirements:
        - Between 200 and 400 words
        - Must start with "In the era of"
        - Must include at least one code example
        Log your progress, then call nika_complete with the article.
      tools: [builtin]
      max_turns: 8
      max_tokens: 1000
      token_budget: 8000
      guardrails:
        - type: length
          min_words: 200
          max_words: 400
          on_failure: retry
        - type: regex
          pattern: "^In the era of"
          message: "Article must begin with 'In the era of'"
          on_failure: retry
      limits:
        max_turns: 10
        max_cost_usd: 0.50
        max_duration_secs: 120
      completion:
        mode: explicit
    artifact:
      path: output/declarative-article.md
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 09: DATA HEIST
// ═════════════════════════════════════════════════════════════════════════════

// ── 09-01: Fetch Markdown ────────────────────────────────────────────────────

const DATA_HEIST_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 1: Fetch Markdown & Article
# =============================================================================
#
# extract: markdown — Full HTML to clean Markdown (htmd library)
# extract: article  — Main content only, strips nav/ads/sidebars (Readability)
#
# REQUIRES: cargo install nika --features "fetch-markdown,fetch-article"
#
# INSTRUCTIONS:
#   1. Set the correct extract modes
#   2. Run: nika run 01-fetch-markdown.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  # TODO: Fetch a webpage and convert to full Markdown
  - id: as_markdown
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown  # TODO: change this value
      timeout: 20

  # TODO: Fetch the same page but extract only the article content
  - id: as_article
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown  # TODO: change this value
      timeout: 20

  # Compare both outputs with an LLM
  - id: compare
    depends_on: [as_markdown, as_article]
    with:
      markdown: $as_markdown
      article: $as_article
    infer:
      prompt: |
        Compare these two extraction modes:

        ## Full Markdown (first 2000 chars)
        {{with.markdown | first(2000)}}

        ## Article Only (first 2000 chars)
        {{with.article | first(2000)}}

        Which is more useful for LLM consumption and why? Be specific.
      max_tokens: 500
"##;

const DATA_HEIST_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 1: Fetch Markdown & Article
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: as_markdown
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20
    artifact:
      path: output/rust-blog-full.md

  - id: as_article
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: article
      timeout: 20
    artifact:
      path: output/rust-blog-article.md

  - id: compare
    depends_on: [as_markdown, as_article]
    with:
      markdown: $as_markdown
      article: $as_article
    infer:
      prompt: |
        Compare these two extraction modes:

        ## Full Markdown (first 2000 chars)
        {{with.markdown | first(2000)}}

        ## Article Only (first 2000 chars)
        {{with.article | first(2000)}}

        Which is more useful for LLM consumption and why? Be specific.
      max_tokens: 500
    artifact:
      path: output/extraction-comparison.md
"##;

// ── 09-02: Fetch Metadata ────────────────────────────────────────────────────

const DATA_HEIST_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 2: Fetch Metadata, Links & Selectors
# =============================================================================
#
# extract: metadata — OG tags, Twitter Cards, JSON-LD, SEO tags (JSON)
# extract: links    — Link classification: internal/external, nav/content/footer
# extract: selector — Raw HTML of elements matching a CSS selector
#                     (requires selector: field)
#
# REQUIRES: cargo install nika --features fetch-html
#
# INSTRUCTIONS:
#   1. Set the correct extract modes and add the CSS selector
#   2. Run: nika run 02-fetch-metadata.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # TODO: Extract metadata (OG, Twitter, JSON-LD, SEO)
  - id: get_metadata
    fetch:
      url: "https://github.com"
      extract: metadata  # TODO: change this value
      timeout: 15

  # TODO: Extract and classify all links on the page
  - id: get_links
    fetch:
      url: "https://github.com"
      extract: links  # TODO: change this value
      timeout: 15

  # TODO: Extract specific HTML elements using a CSS selector
  # The selector: field is REQUIRED with extract: selector
  - id: get_headings
    fetch:
      url: "https://httpbin.org/html"
      extract: selector  # TODO: change this value
      selector: "h1"  # TODO: change this value
      timeout: 15
"##;

const DATA_HEIST_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 2: Fetch Metadata, Links & Selectors
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: get_metadata
    fetch:
      url: "https://github.com"
      extract: metadata
      timeout: 15
    artifact:
      path: output/github-metadata.json
      format: json

  - id: get_links
    fetch:
      url: "https://github.com"
      extract: links
      timeout: 15
    artifact:
      path: output/github-links.json
      format: json

  - id: get_headings
    fetch:
      url: "https://httpbin.org/html"
      extract: selector
      selector: "h1, p"
      timeout: 15
    artifact:
      path: output/httpbin-headings.html
"##;

// ── 09-03: Fetch JSONPath ────────────────────────────────────────────────────

const DATA_HEIST_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 3: JSONPath, Feed & llm_txt
# =============================================================================
#
# extract: jsonpath — Query JSON APIs with JSONPath (zero deps, always available)
#                     Requires selector: with a JSONPath expression
# extract: feed    — Parse RSS/Atom/JSON Feed (requires fetch-feed feature)
# extract: llm_txt — AI-era content discovery (checks /.well-known/llm.txt)
#
# INSTRUCTIONS:
#   1. Set the correct extract modes and JSONPath selector
#   2. Run: nika run 03-fetch-jsonpath.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # TODO: Query a JSON API using JSONPath
  - id: api_jsonpath
    fetch:
      url: "https://api.github.com"
      extract: jsonpath  # TODO: change this value
      selector: "$.current_user_url"  # TODO: change this value
      timeout: 10

  # TODO: Parse an RSS feed into structured data
  - id: rss_feed
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed  # TODO: change this value
      timeout: 15

  # TODO: Check for AI content discovery files (llm.txt)
  - id: llm_discovery
    fetch:
      url: "https://docs.anthropic.com"
      extract: llm_txt  # TODO: change this value
      timeout: 10
"##;

const DATA_HEIST_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 3: JSONPath, Feed & llm_txt
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: api_jsonpath
    fetch:
      url: "https://api.github.com"
      extract: jsonpath
      selector: "$.current_user_url"
      timeout: 10
    artifact:
      path: output/github-jsonpath.json
      format: json

  - id: rss_feed
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 15
    artifact:
      path: output/rust-feed.json
      format: json

  - id: llm_discovery
    fetch:
      url: "https://docs.anthropic.com"
      extract: llm_txt
      timeout: 10
    artifact:
      path: output/anthropic-llm-txt.md
"##;

// ── 09-04: Fetch Binary ──────────────────────────────────────────────────────

const DATA_HEIST_04_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 4: Binary & Full Response
# =============================================================================
#
# response: binary — Download raw bytes into CAS, returns the hash
#                    Perfect for images, PDFs, binary files
# response: full   — JSON with { status, headers, body, url }
#                    Perfect for debugging redirects, checking security headers
#
# No feature flags needed — both are always available.
#
# INSTRUCTIONS:
#   1. Set the correct response modes
#   2. Run: nika run 04-fetch-binary.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # TODO: Download a binary file (image) into CAS
  - id: download_image
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary  # TODO: change this value
      timeout: 15

  # TODO: Get the full HTTP response including headers
  - id: inspect_headers
    fetch:
      url: "https://httpbin.org/headers"
      response: full  # TODO: change this value
      timeout: 10

  # Log both results
  - id: log_results
    depends_on: [download_image, inspect_headers]
    with:
      image: $download_image
      headers: $inspect_headers
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Image hash: {{with.image}} | Headers: {{with.headers}}"
"##;

const DATA_HEIST_04_SOLUTION: &str = r##"# =============================================================================
# LEVEL 9 — EXERCISE 4: Binary & Full Response
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: download_image
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15
    artifact:
      path: output/downloaded-image.png
      format: binary

  - id: inspect_headers
    fetch:
      url: "https://httpbin.org/headers"
      response: full
      timeout: 10
    artifact:
      path: output/full-response.json
      format: json

  - id: log_results
    depends_on: [download_image, inspect_headers]
    with:
      image: $download_image
      headers: $inspect_headers
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Image hash: {{with.image}} | Headers: {{with.headers}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 10: OPEN PROTOCOL
// ═════════════════════════════════════════════════════════════════════════════

// ── 10-01: MCP Basics ────────────────────────────────────────────────────────

const OPEN_PROTOCOL_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 10 — EXERCISE 1: MCP Basics
# =============================================================================
#
# mcp: configures external tool servers (Model Context Protocol).
# invoke: calls MCP tools by name — Nika routes to the right server.
#
# The MCP handshake:
#   1. Nika launches the MCP server process
#   2. Server exposes its tool list via the MCP protocol
#   3. Nika discovers tools at runtime (list_directory, read_file, etc.)
#   4. invoke: with mcp: field routes calls to the named server
#
# SETUP: npm install -g @anthropic/mcp-filesystem
#
# INSTRUCTIONS:
#   1. Configure the MCP server and fill in tool names
#   2. Run: nika run 01-mcp-basics.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

# TODO: Add MCP server configuration
mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "TODO: MCP server package (e.g. @anthropic/mcp-filesystem)"

tasks:
  # TODO: List files using an MCP tool
  - id: list_files
    invoke:
      mcp: filesystem
      tool: "TODO: MCP tool name (e.g. list_directory)"
      params:
        path: "TODO: directory to list"

  # TODO: Read a file using an MCP tool
  - id: read_file
    depends_on: [list_files]
    invoke:
      mcp: filesystem
      tool: "TODO: MCP tool name (e.g. read_file)"
      params:
        path: "TODO: file path to read"
"##;

const OPEN_PROTOCOL_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 10 — EXERCISE 1: MCP Basics
# =============================================================================

schema: "nika/workflow@0.12"

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

tasks:
  - id: list_files
    invoke:
      mcp: filesystem
      tool: "list_directory"
      params:
        path: "."

  - id: read_file
    depends_on: [list_files]
    invoke:
      mcp: filesystem
      tool: "read_file"
      params:
        path: "./01-mcp-basics.nika.yaml"
    artifact:
      path: output/mcp-read-result.txt
"##;

// ── 10-02: MCP Tools ─────────────────────────────────────────────────────────

const OPEN_PROTOCOL_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 10 — EXERCISE 2: Agent + MCP Tools
# =============================================================================
#
# An agent that uses MCP tools AUTONOMOUSLY.
# The agent discovers tools at runtime via the MCP protocol.
#
# Inside agent: block:
#   mcp: [server_name]  — List of MCP servers the agent can access
#   tools: [builtin]    — Also include builtin nika:* tools
#
# The agent sees BOTH MCP tools and builtin tools in its tool list.
#
# INSTRUCTIONS:
#   1. Give the agent access to MCP tools and builtin tools
#   2. Run: nika run 02-mcp-tools.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

tasks:
  - id: explorer
    agent:
      prompt: |
        You are a file explorer agent. Your mission:
        1. List the current directory
        2. Read any .yaml files you find
        3. Log what you discover with nika_log
        4. Complete with a summary of the project structure
      # TODO: Give the agent access to MCP servers
      # Hint: mcp: [filesystem]
      mcp: [filesystem]  # TODO: change this value
      # TODO: Also include builtin tools
      tools: [builtin]  # TODO: change this value
      max_turns: 10
      max_tokens: 600
      token_budget: 5000
      completion:
        mode: explicit
"##;

const OPEN_PROTOCOL_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 10 — EXERCISE 2: Agent + MCP Tools
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

tasks:
  - id: explorer
    agent:
      prompt: |
        You are a file explorer agent. Your mission:
        1. List the current directory
        2. Read any .yaml files you find
        3. Log what you discover with nika_log
        4. Complete with a summary of the project structure
      mcp: [filesystem]
      tools: [builtin]
      max_turns: 10
      max_tokens: 600
      token_budget: 5000
      completion:
        mode: explicit
    artifact:
      path: output/exploration-report.md
"##;

// ── 10-03: MCP NovaNet ───────────────────────────────────────────────────────

const OPEN_PROTOCOL_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 10 — EXERCISE 3: NovaNet Knowledge Graph via MCP
# =============================================================================
#
# NovaNet is Nika's brain — a knowledge graph accessible ONLY via MCP.
# Zero Cypher rule: Nika NEVER uses raw Cypher. Use invoke: only.
#
# NovaNet MCP tools:
#   novanet:search   — Search nodes by label/properties
#   novanet:get      — Get a node by ID
#   novanet:create   — Create a new node
#   novanet:link     — Create an arc between nodes
#   novanet:traverse — Walk the graph from a node
#   novanet:query    — Run a stored query by name
#   novanet:schema   — Get the graph schema
#
# SETUP: Requires NovaNet MCP server running
#
# INSTRUCTIONS:
#   1. Configure the NovaNet MCP server and fill in tool params
#   2. Run: nika run 03-mcp-novanet.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

# TODO: Configure the NovaNet MCP server
mcp:
  novanet:
    command: "TODO: server command"
    args:
      - "TODO: server arguments"

tasks:
  # TODO: Get the knowledge graph schema
  - id: get_schema
    invoke:
      mcp: novanet
      tool: "novanet:schema"
      params: {}

  # TODO: Search for nodes by label
  - id: search_nodes
    depends_on: [get_schema]
    invoke:
      mcp: novanet
      tool: "novanet:search"
      params:
        label: "TODO: node label to search for"
        limit: 10

  # TODO: Create a new knowledge node
  - id: create_node
    depends_on: [search_nodes]
    invoke:
      mcp: novanet
      tool: "novanet:create"
      params:
        label: "TODO: node label"
        properties:
          name: "TODO: node name"
          description: "TODO: node description"
"##;

const OPEN_PROTOCOL_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 10 — EXERCISE 3: NovaNet Knowledge Graph via MCP
# =============================================================================

schema: "nika/workflow@0.12"

mcp:
  novanet:
    command: "cargo"
    args:
      - "run"
      - "--manifest-path"
      - "../novanet/Cargo.toml"
      - "--"
      - "mcp"

tasks:
  - id: get_schema
    invoke:
      mcp: novanet
      tool: "novanet:schema"
      params: {}
    artifact:
      path: output/novanet-schema.json
      format: json

  - id: search_nodes
    depends_on: [get_schema]
    invoke:
      mcp: novanet
      tool: "novanet:search"
      params:
        label: "Concept"
        limit: 10
    artifact:
      path: output/novanet-search.json
      format: json

  - id: create_node
    depends_on: [search_nodes]
    invoke:
      mcp: novanet
      tool: "novanet:create"
      params:
        label: "Concept"
        properties:
          name: "Workflow Orchestration"
          description: "The practice of coordinating multiple tasks in a defined sequence or DAG"
    artifact:
      path: output/novanet-created.json
      format: json
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 11: PIXEL PIRATE
// ═════════════════════════════════════════════════════════════════════════════

// ── 11-01: Media Import ──────────────────────────────────────────────────────

const PIXEL_PIRATE_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 1: Media Import & Analysis
# =============================================================================
#
# CAS = Content-Addressable Store. Files stored by their hash.
# Tier 1 tools (always-on, no feature flag):
#   nika:dimensions    — Image width/height from headers (~0.1ms)
#   nika:thumbhash     — 25-byte compact image placeholder
#   nika:dominant_color — Color palette extraction
#
# CAS hash pattern: {{with.alias.hash}}
#   When fetch response: binary stores a file, it returns a JSON object.
#   .hash is the CAS hash for use with all nika:* tools.
#
# INSTRUCTIONS:
#   1. Fill in the correct tool names and CAS hash references
#   2. Run: nika run 01-media-import.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  # Download an image into CAS
  - id: download
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  # TODO: Get image dimensions (reads only headers, ~0.1ms)
  - id: get_dims
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dimensions"  # TODO: change this value
      params:
        hash: "{{with.img.hash}}"  # TODO: change this value

  # TODO: Generate a thumbhash placeholder
  - id: get_thumbhash
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbhash"  # TODO: change this value
      params:
        hash: "{{with.img.hash}}"  # TODO: change this value

  # TODO: Extract dominant colors
  - id: get_colors
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dominant_color"  # TODO: change this value
      params:
        hash: "{{with.img.hash}}"  # TODO: change this value
        count: 5
"##;

const PIXEL_PIRATE_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 1: Media Import & Analysis
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: download
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  - id: get_dims
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"
    artifact:
      path: output/dimensions.json
      format: json

  - id: get_thumbhash
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.hash}}"
    artifact:
      path: output/thumbhash.json
      format: json

  - id: get_colors
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.hash}}"
        count: 5
    artifact:
      path: output/colors.json
      format: json
"##;

// ── 11-02: Media Transform ───────────────────────────────────────────────────

const PIXEL_PIRATE_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 2: Media Transforms
# =============================================================================
#
# Tier 2 tools (media-core feature):
#   nika:thumbnail — SIMD-accelerated Lanczos3 resize
#   nika:convert   — Format conversion (PNG/JPEG/WebP)
#   nika:optimize  — Lossless PNG optimization (oxipng)
#
# Each reads from CAS and writes the result back to CAS.
#
# REQUIRES: cargo install nika --features media-core
#
# INSTRUCTIONS:
#   1. Fill in tool names and CAS hash references
#   2. Run: nika run 02-media-transform.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: download
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20
      retry:
        max_attempts: 2
        backoff_ms: 1000
        multiplier: 2.0

  # TODO: Create a 256px thumbnail — change tool name and params
  - id: make_thumbnail
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:thumbnail"  # TODO: change this value
      params:
        hash: "{{with.photo.hash}}"  # TODO: change this value
        width: 256  # TODO: change this value
        format: "jpeg"  # TODO: change this value (png|jpeg|webp)

  # TODO: Convert the original to WebP format — change tool name and params
  - id: convert_webp
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:convert"  # TODO: change this value
      params:
        hash: "{{with.photo.hash}}"  # TODO: change this value
        format: "webp"  # TODO: change this value

  # TODO: Convert to PNG first (optimize only works on PNG!) — change tool name and params
  - id: convert_png
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:convert"  # TODO: change this value
      params:
        hash: "{{with.photo.hash}}"  # TODO: change this value
        format: "png"  # TODO: change this value

  # TODO: Optimize the PNG losslessly — change tool name and params
  - id: optimize
    depends_on: [convert_png]
    with:
      png: $convert_png
    invoke:
      tool: "nika:optimize"  # TODO: change this value
      params:
        hash: "{{with.png.hash}}"  # TODO: change this value
        level: 3
"##;

const PIXEL_PIRATE_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 2: Media Transforms
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: download
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20
      retry:
        max_attempts: 2
        backoff_ms: 1000
        multiplier: 2.0

  - id: make_thumbnail
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.photo.hash}}"
        width: 256
        format: "jpeg"
    artifact:
      path: output/photo-thumb-256.jpg
      format: binary

  - id: convert_webp
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:convert"
      params:
        hash: "{{with.photo.hash}}"
        format: "webp"
        quality: 85
    artifact:
      path: output/photo-converted.webp
      format: binary

  - id: convert_png
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:convert"
      params:
        hash: "{{with.photo.hash}}"
        format: "png"
    artifact:
      path: output/photo-converted.png
      format: binary

  - id: optimize
    depends_on: [convert_png]
    with:
      png: $convert_png
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.png.hash}}"
        level: 3
        strip: true
    artifact:
      path: output/photo-optimized.png
      format: binary
"##;

// ── 11-03: Media Pipeline ────────────────────────────────────────────────────

const PIXEL_PIRATE_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 3: Pipeline Chain
# =============================================================================
#
# nika:pipeline chains operations in-memory:
# 1 CAS read -> N in-memory transforms -> 1 CAS write.
# Budget charged ONCE for the final output, not per step.
#
# Available pipeline steps:
#   thumbnail: resize (width, height)
#   strip:     remove EXIF/metadata (decode + re-encode)
#   convert:   change format (png, jpeg, webp) with optional quality
#   optimize:  lossless PNG compression (level 1-6)
#
# INSTRUCTIONS:
#   1. Chain 3 operations in a single pipeline
#   2. Run: nika run 03-media-pipeline.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: download
    fetch:
      url: "https://picsum.photos/1200/800.jpg"
      response: binary
      timeout: 20

  # TODO: Chain 3 operations in one pipeline call
  - id: process
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "TODO: CAS hash reference"
        steps:
          # TODO: Step 1 — Resize to 400px wide
          - op: "TODO: operation name"
            width: "TODO: pixels"
          # TODO: Step 2 — Strip EXIF metadata for privacy
          - op: "TODO: operation name"
          # TODO: Step 3 — Convert to WebP for web delivery
          - op: "TODO: operation name"
            format: "TODO: target format"

  - id: log_result
    depends_on: [process]
    with:
      result: $process
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Pipeline complete: {{with.result}}"
"##;

const PIXEL_PIRATE_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 3: Pipeline Chain
# =============================================================================

schema: "nika/workflow@0.12"

tasks:
  - id: download
    fetch:
      url: "https://picsum.photos/1200/800.jpg"
      response: binary
      timeout: 20

  - id: process
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.photo.hash}}"
        steps:
          - op: thumbnail
            width: 400
          - op: strip
          - op: convert
            format: webp
            quality: 85
    artifact:
      path: output/pipeline-result.webp
      format: binary

  - id: log_result
    depends_on: [process]
    with:
      result: $process
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Pipeline complete: {{with.result}}"
"##;

// ── 11-04: Vision ────────────────────────────────────────────────────────────

const PIXEL_PIRATE_04_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 4: Multimodal Vision
# =============================================================================
#
# content: sends images to vision-capable LLMs (infer: verb).
# CAS hashes are auto-resolved to base64 — paths never leak to APIs.
#
# content: array with two part types:
#   - type: image   — CAS image (source: hash, detail: low|high)
#   - type: text    — Text prompt
#
# Supported providers: Claude, OpenAI, Gemini, Groq, xAI
# NOT supported: DeepSeek (returns VisionNotSupported error)
#
# INSTRUCTIONS:
#   1. Build a content: array with an image part and a text part
#   2. Run: nika run 04-vision.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: download_photo
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  # TODO: Send the image to an LLM for description
  - id: describe_image
    depends_on: [download_photo]
    with:
      photo: $download_photo
    infer:
      # TODO: Use content: array with image + text parts
      content:
        - type: image  # TODO: change this value
          source: "{{with.photo.hash}}"  # TODO: change this value
          detail: high  # TODO: change this value (low or high)
        - type: text  # TODO: change this value
          text: "Describe this image in detail"  # TODO: change this value
      max_tokens: 500
"##;

const PIXEL_PIRATE_04_SOLUTION: &str = r##"# =============================================================================
# LEVEL 11 — EXERCISE 4: Multimodal Vision
# =============================================================================

schema: "nika/workflow@0.12"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: download_photo
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  - id: describe_image
    depends_on: [download_photo]
    with:
      photo: $download_photo
    infer:
      content:
        - type: image
          source: "{{with.photo.hash}}"
          detail: high
        - type: text
          text: |
            Describe this image in detail. Include:
            1. The main subject and composition
            2. Colors and lighting
            3. Mood and atmosphere
            4. Any text or notable details
      max_tokens: 500
      temperature: 0.3
    artifact:
      path: output/image-description.md
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 12: SUPERNOVAE (BOSS)
// ═════════════════════════════════════════════════════════════════════════════

// ── 12-01: SEO Mega Audit ────────────────────────────────────────────────────

const SUPERNOVAE_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 1: SEO Mega Audit
# =============================================================================
#
# Combines: fetch (4 extract modes), infer (analysis), agent (guardrails)
# This is a BOSS exercise — it requires concepts from Levels 2, 5, 8, 9.
#
# INSTRUCTIONS:
#   1. Set correct extract and response modes for all fetch tasks
#   2. Add guardrails and completion mode to the agent
#   3. Run: nika run 01-seo-mega-audit.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  # Phase 1: Scrape with 4 different extract/response modes
  - id: scrape_metadata
    fetch:
      url: "https://github.com"
      extract: metadata  # TODO: change this — mode for SEO metadata (OG, Twitter, JSON-LD)
      timeout: 20

  - id: scrape_content
    fetch:
      url: "https://github.com"
      extract: markdown  # TODO: change this — mode for clean Markdown
      timeout: 20

  - id: scrape_links
    fetch:
      url: "https://github.com"
      extract: links  # TODO: change this — mode for link classification
      timeout: 20

  - id: check_headers
    fetch:
      url: "https://github.com"
      response: full  # TODO: change this — mode for full HTTP response
      timeout: 15

  # Phase 2: LLM Analysis (combines all scraped data)
  - id: seo_analysis
    depends_on: [scrape_metadata, scrape_content, scrape_links, check_headers]
    with:
      metadata: $scrape_metadata
      content: $scrape_content
      links: $scrape_links
      headers: $check_headers
    infer:
      prompt: |
        Analyze this site's SEO:

        ## Metadata
        {{with.metadata}}

        ## Content (first 2000 chars)
        {{with.content | first(2000)}}

        ## Links
        {{with.links}}

        ## Headers
        {{with.headers}}

        Score each area /100 and list top 3 issues.
      temperature: 0.3
      max_tokens: 2000

  # Phase 3: Agent executive summary with guardrails
  - id: executive_summary
    depends_on: [seo_analysis]
    with:
      analysis: $seo_analysis
    agent:
      prompt: |
        Create an executive SEO summary from this analysis:
        {{with.analysis}}

        Include: scorecard, critical issues, quick wins, 30-day roadmap.
        Call nika_complete with your final report.
      tools: [builtin]
      max_turns: 6
      max_tokens: 2000
      token_budget: 10000
      # TODO: Add guardrails (length + regex for "roadmap")
      guardrails:
        - type: length  # TODO: change this
          min_words: 100  # TODO: change this
          on_failure: retry
        - type: regex  # TODO: change this
          pattern: "(?i)roadmap"  # TODO: change this — regex for "roadmap"
          message: "Must include a roadmap section"  # TODO: change this
          on_failure: retry
      # TODO: Set completion mode
      completion:
        mode: explicit  # TODO: change this
"##;

const SUPERNOVAE_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 1: SEO Mega Audit
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: scrape_metadata
    fetch:
      url: "https://github.com"
      extract: metadata
      timeout: 20
    artifact:
      path: output/seo-metadata.json
      format: json

  - id: scrape_content
    fetch:
      url: "https://github.com"
      extract: markdown
      timeout: 20

  - id: scrape_links
    fetch:
      url: "https://github.com"
      extract: links
      timeout: 20
    artifact:
      path: output/seo-links.json
      format: json

  - id: check_headers
    fetch:
      url: "https://github.com"
      response: full
      timeout: 15
    artifact:
      path: output/seo-headers.json
      format: json

  - id: seo_analysis
    depends_on: [scrape_metadata, scrape_content, scrape_links, check_headers]
    with:
      metadata: $scrape_metadata
      content: $scrape_content
      links: $scrape_links
      headers: $check_headers
    infer:
      prompt: |
        Analyze this site's SEO:

        ## Metadata
        {{with.metadata}}

        ## Content (first 2000 chars)
        {{with.content | first(2000)}}

        ## Links
        {{with.links}}

        ## Headers
        {{with.headers}}

        Score each area /100 and list top 3 issues.
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: output/seo-analysis.md

  - id: executive_summary
    depends_on: [seo_analysis]
    with:
      analysis: $seo_analysis
    agent:
      prompt: |
        Create an executive SEO summary from this analysis:
        {{with.analysis}}

        Include: scorecard, critical issues, quick wins, 30-day roadmap.
        Call nika_complete with your final report.
      tools: [builtin]
      max_turns: 6
      max_tokens: 2000
      token_budget: 10000
      guardrails:
        - type: length
          min_words: 200
          on_failure: retry
        - type: regex
          pattern: "(?i)roadmap"
          message: "Must include a roadmap section"
          on_failure: retry
      completion:
        mode: explicit
    artifact:
      path: output/seo-executive-summary.md
"##;

// ── 12-02: Image Pipeline ────────────────────────────────────────────────────

const SUPERNOVAE_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 2: Image Pipeline
# =============================================================================
#
# Combines: fetch binary, media tools, nika:pipeline, nika:chart, vision
# This is a BOSS exercise — it requires concepts from Levels 9, 11.
#
# INSTRUCTIONS:
#   1. Complete the vision report task with content: array
#   2. Run: nika run 02-image-pipeline.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  # Phase 1: Download images into CAS
  - id: download_png
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  - id: download_photo
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  # Phase 2: Analyze images
  - id: analyze_png
    depends_on: [download_png]
    with:
      img: $download_png
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"

  - id: colors_photo
    depends_on: [download_photo]
    with:
      img: $download_photo
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.hash}}"
        count: 5

  # Phase 3: Process with pipeline
  - id: process_photo
    depends_on: [download_photo]
    with:
      photo: $download_photo
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.photo.hash}}"
        steps:
          - op: thumbnail
            width: 400
          - op: strip
          - op: convert
            format: webp

  # Phase 4: Generate comparison chart
  - id: chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Image Processing Results"
        width: 800
        height: 500
        series:
          - name: "Original"
            data: [20000, 150000]
          - name: "Processed"
            data: [8000, 25000]
        labels: ["PNG", "Photo"]

  # TODO: Phase 5 — Vision report: send chart to LLM with analysis data
  - id: vision_report
    depends_on: [analyze_png, colors_photo, process_photo, chart]
    with:
      dims: $analyze_png
      colors: $colors_photo
      pipeline: $process_photo
      chart_img: $chart
    infer:
      # TODO: Build content: array with image (chart) + text (analysis)
      content:
        - type: image  # TODO: change this — image part
          source: "{{with.chart_img.hash}}"  # TODO: change this — chart CAS hash
          detail: high
        - type: text  # TODO: change this — text part
          text: |
            Analyze this image processing pipeline:
            Dimensions: {{with.dims}}
            Colors: {{with.colors}}
            Pipeline result: {{with.pipeline}}
            Write a technical summary with recommendations.
      max_tokens: 1500
"##;

const SUPERNOVAE_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 2: Image Pipeline
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: download_png
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  - id: download_photo
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  - id: analyze_png
    depends_on: [download_png]
    with:
      img: $download_png
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"

  - id: colors_photo
    depends_on: [download_photo]
    with:
      img: $download_photo
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.hash}}"
        count: 5

  - id: process_photo
    depends_on: [download_photo]
    with:
      photo: $download_photo
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.photo.hash}}"
        steps:
          - op: thumbnail
            width: 400
          - op: strip
          - op: convert
            format: webp
    artifact:
      path: output/processed-photo.webp
      format: binary

  - id: chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Image Processing Results"
        width: 800
        height: 500
        series:
          - name: "Original"
            data: [20000, 150000]
          - name: "Processed"
            data: [8000, 25000]
        labels: ["PNG", "Photo"]

  - id: vision_report
    depends_on: [analyze_png, colors_photo, process_photo, chart]
    with:
      dims: $analyze_png
      colors: $colors_photo
      pipeline: $process_photo
      chart_img: $chart
    infer:
      content:
        - type: image
          source: "{{with.chart_img.hash}}"
          detail: high
        - type: text
          text: |
            Analyze this image processing pipeline:

            ## Size Comparison Chart (above)
            Bar chart showing original vs processed file sizes.

            ## Image Dimensions
            {{with.dims}}

            ## Dominant Colors
            {{with.colors}}

            ## Pipeline Result
            {{with.pipeline}}

            Write a technical summary covering:
            1. Size reduction percentages
            2. Color palette analysis
            3. Pipeline efficiency
            4. Recommendations for web delivery
      max_tokens: 1500
      temperature: 0.3
    artifact:
      path: output/vision-pipeline-report.md
"##;

// ── 12-03: Content Factory ───────────────────────────────────────────────────

const SUPERNOVAE_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 3: Content Factory
# =============================================================================
#
# Combines: infer (structured), for_each (parallel), inputs, translation
# This is a BOSS exercise — it requires concepts from Levels 3, 5, 6.
#
# INSTRUCTIONS:
#   1. Add for_each to the write_sections task
#   2. Run: nika run 03-content-factory.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  topic:
    type: string
    default: "AI Workflow Engines in 2025"
  audience:
    type: string
    default: "senior developers"

tasks:
  # Phase 1: Research via fetch
  - id: research
    fetch:
      url: "https://news.ycombinator.com/"
      extract: article
      timeout: 20

  # Phase 2: Generate structured outline (JSON)
  - id: outline
    depends_on: [research]
    with:
      sources: $research
    infer:
      prompt: |
        Create a blog post outline about "{{inputs.topic}}" for {{inputs.audience}}.
        Use these sources: {{with.sources | first(2000)}}
        Return JSON with: title, sections (array of {heading, key_points, word_count_target})
      response_format: json
      temperature: 0.5
      max_tokens: 1000

  # TODO: Phase 3 — Write sections in parallel using for_each
  - id: write_sections
    depends_on: [outline]
    with:
      plan: $outline
    # TODO: Add for_each to iterate over outline sections
    # Hint: for_each: "$outline.sections"
    for_each: "$outline.sections"  # TODO: change this — iterate over sections
    # TODO: Name the loop variable
    as: section  # TODO: change this — loop variable name
    # TODO: Set parallelism
    concurrency: 3  # TODO: change this — number of parallel tasks
    infer:
      prompt: |
        Write this section of a blog post about "{{inputs.topic}}":
        Heading: {{with.section.heading}}
        Key points: {{with.section.key_points}}
        Target: {{with.section.word_count_target}} words
      temperature: 0.6
      max_tokens: 1200

  # Phase 4: Translate to 3 languages (for_each over language list)
  - id: translate
    depends_on: [write_sections]
    with:
      content: $write_sections
    for_each:
      - { code: "fr-FR", name: "French" }
      - { code: "es-ES", name: "Spanish" }
      - { code: "de-DE", name: "German" }
    as: lang
    concurrency: 3
    infer:
      prompt: |
        Translate this into {{with.lang.name}} ({{with.lang.code}}):
        {{with.content | first(3000)}}
        Keep Markdown formatting. Technical terms stay in English.
      temperature: 0.3
      max_tokens: 3000
"##;

const SUPERNOVAE_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 3: Content Factory
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  topic:
    type: string
    default: "AI Workflow Engines in 2025"
  audience:
    type: string
    default: "senior developers"

tasks:
  - id: research
    fetch:
      url: "https://news.ycombinator.com/"
      extract: article
      timeout: 20
    artifact:
      path: output/research.md

  - id: outline
    depends_on: [research]
    with:
      sources: $research
    infer:
      prompt: |
        Create a blog post outline about "{{inputs.topic}}" for {{inputs.audience}}.
        Use these sources: {{with.sources | first(2000)}}
        Return JSON with: title, sections (array of {heading, key_points, word_count_target})
      response_format: json
      temperature: 0.5
      max_tokens: 1000
    artifact:
      path: output/outline.json
      format: json

  - id: write_sections
    depends_on: [outline]
    with:
      plan: $outline
    for_each: "$outline.sections"
    as: section
    concurrency: 3
    infer:
      prompt: |
        Write this section of a blog post about "{{inputs.topic}}":
        Heading: {{with.section.heading}}
        Key points: {{with.section.key_points}}
        Target: {{with.section.word_count_target}} words
      temperature: 0.6
      max_tokens: 1200
    artifact:
      path: "output/sections/{{with.section.heading | lower | trim}}.md"

  - id: translate
    depends_on: [write_sections]
    with:
      content: $write_sections
    for_each:
      - { code: "fr-FR", name: "French" }
      - { code: "es-ES", name: "Spanish" }
      - { code: "de-DE", name: "German" }
    as: lang
    concurrency: 3
    infer:
      prompt: |
        Translate this into {{with.lang.name}} ({{with.lang.code}}):
        {{with.content | first(3000)}}
        Keep Markdown formatting. Technical terms stay in English.
      temperature: 0.3
      max_tokens: 3000
    artifact:
      path: "output/translations/{{with.lang.code}}.md"
"##;

// ── 12-04: Research Agent ────────────────────────────────────────────────────

const SUPERNOVAE_04_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 4: Research Agent
# =============================================================================
#
# Combines: agent + MCP + fetch + with: bindings
# This is a BOSS exercise — it requires concepts from Levels 8, 9, 10.
#
# INSTRUCTIONS:
#   1. Configure the agent with MCP access, guardrails, and completion
#   2. Run: nika run 04-research-agent.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

tasks:
  # Phase 1: Gather data from multiple sources
  - id: scrape_source
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20

  - id: scrape_feed
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 15

  # Phase 2: Agent analyzes everything
  - id: research_agent
    depends_on: [scrape_source, scrape_feed]
    with:
      source: $scrape_source
      feed: $scrape_feed
    agent:
      prompt: |
        You are a senior technical researcher. Your mission:

        ## Source Material
        {{with.source | first(3000)}}

        ## Feed Entries
        {{with.feed | first(2000)}}

        Tasks:
        1. Log "Starting research" with nika_log
        2. Analyze the source material for key trends
        3. Use the filesystem MCP tools to write your findings
        4. Produce a research brief with 5 key insights
        5. Call nika_complete with your final research brief
      # TODO: Configure MCP access and tools
      mcp: [filesystem]  # TODO: change this — MCP server list
      tools: [builtin]  # TODO: change this — tool list
      max_turns: 10
      max_tokens: 1500
      token_budget: 12000
      # TODO: Set completion mode
      completion:
        mode: explicit  # TODO: change this
      # TODO: Add guardrails (length + keyword check)
      guardrails:
        - type: length  # TODO: change this — length or regex?
          min_words: 100  # TODO: change this
          on_failure: retry
        - type: regex  # TODO: change this — length or regex?
          pattern: "(?i)insight"  # TODO: change this — regex to require 'insights' keyword
          message: "Research brief must include insights"  # TODO: change this — error message
          on_failure: retry

  # Phase 3: Summary
  - id: summary
    depends_on: [research_agent]
    with:
      research: $research_agent
    infer:
      prompt: |
        Summarize this research brief in 200 words:
        {{with.research}}
      max_tokens: 400
"##;

const SUPERNOVAE_04_SOLUTION: &str = r##"# =============================================================================
# LEVEL 12 — BOSS 4: Research Agent
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

tasks:
  - id: scrape_source
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20
    artifact:
      path: output/rust-blog.md

  - id: scrape_feed
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 15
    artifact:
      path: output/rust-feed.json
      format: json

  - id: research_agent
    depends_on: [scrape_source, scrape_feed]
    with:
      source: $scrape_source
      feed: $scrape_feed
    agent:
      prompt: |
        You are a senior technical researcher. Your mission:

        ## Source Material
        {{with.source | first(3000)}}

        ## Feed Entries
        {{with.feed | first(2000)}}

        Tasks:
        1. Log "Starting research" with nika_log
        2. Analyze the source material for key trends
        3. Use the filesystem MCP tools to write your findings
        4. Produce a research brief with 5 key insights
        5. Call nika_complete with your final research brief
      mcp: [filesystem]
      tools: [builtin]
      max_turns: 10
      max_tokens: 1500
      token_budget: 12000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 150
          on_failure: retry
        - type: regex
          pattern: "(?i)insight"
          message: "Research brief must include insights"
          on_failure: retry
    artifact:
      path: output/research-brief.md

  - id: summary
    depends_on: [research_agent]
    with:
      research: $research_agent
    infer:
      prompt: |
        Summarize this research brief in 200 words:
        {{with.research}}
      max_tokens: 400
      temperature: 0.3
    artifact:
      path: output/research-summary.md
"##;

// ── 12-05: Full Stack ────────────────────────────────────────────────────────

const SUPERNOVAE_05_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 12 — BOSS FINAL: Full Stack
# =============================================================================
#
# ALL 5 VERBS in one workflow: exec, fetch, invoke, infer, agent.
# Plus: MCP, media pipeline, structured output, vision, limits.
#
# NEW CONCEPT — limits: block (inside agent:)
#   max_turns:         — Max loop iterations
#   max_tokens:        — Total token budget (input + output)
#   max_cost_usd:      — Cost ceiling per execution
#   max_duration_secs: — Wall-clock timeout
#
# This is the FINAL BOSS. It requires mastery of ALL 11 previous levels.
#
# INSTRUCTIONS:
#   1. Configure the final agent with MCP, tools, guardrails, limits
#   2. Run: nika run 05-full-stack.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

inputs:
  topic:
    type: string
    default: "The Future of AI Orchestration"

tasks:
  # ── VERB 1: exec ─────────────────────────────────────────────
  - id: setup
    exec:
      command: |
        echo "Starting full-stack workflow"
        echo "Topic: {{inputs.topic}}"
        date +%Y-%m-%dT%H:%M:%S
      shell: true

  # ── VERB 2: fetch ────────────────────────────────────────────
  - id: scrape_data
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20

  - id: download_image
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  # ── VERB 3: invoke (builtin tools) ──────────────────────────
  - id: log_start
    depends_on: [setup]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Full-stack workflow initiated"

  - id: process_image
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.hash}}"
        steps:
          - op: thumbnail
            width: 300
          - op: convert
            format: webp

  - id: generate_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Workflow Verb Usage"
        width: 800
        height: 500
        series:
          - name: "Frequency"
            data: [15, 12, 20, 8, 5]
        labels: ["exec", "fetch", "invoke", "infer", "agent"]

  # ── VERB 4: infer (structured + vision) ─────────────────────
  - id: analyze
    depends_on: [scrape_data, generate_chart]
    with:
      content: $scrape_data
      chart: $generate_chart
    infer:
      content:
        - type: image
          source: "{{with.chart.hash}}"
          detail: high
        - type: text
          text: |
            Analyze this chart and content about {{inputs.topic}}:
            {{with.content | first(2000)}}
            Return JSON with: summary, key_insights (array), score (1-100)
      response_format: json
      max_tokens: 1000

  # ── VERB 5: agent ────────────────────────────────────────────
  # TODO: Configure the final synthesis agent
  - id: final_agent
    depends_on: [analyze, process_image, log_start]
    with:
      analysis: $analyze
      image: $process_image
    agent:
      prompt: |
        You are the final synthesis agent. You have:

        ## Analysis Results
        {{with.analysis}}

        ## Processed Image
        {{with.image}}

        Create a comprehensive report covering:
        1. Data analysis findings
        2. Image processing results
        3. Overall assessment
        4. Next steps

        Use nika_log for progress. Call nika_complete when done.
      # TODO: Give the agent MCP access
      mcp: [filesystem]  # TODO: change this — MCP server list
      # TODO: Give the agent builtin tools
      tools: [builtin]  # TODO: change this — tool list
      # TODO: Set per-response token limit
      max_tokens: 1500  # TODO: change this
      # TODO: Set completion mode
      completion:
        mode: explicit  # TODO: change this
      # TODO: Add guardrails
      guardrails:
        - type: length  # TODO: change this
          min_words: 200  # TODO: change this
          on_failure: retry
      # TODO: Add limits block (turns, cost, duration, token budget)
      # This is the BOSS way to control agent resources.
      limits:
        max_turns: 12  # TODO: change this
        max_tokens: 15000  # TODO: change this — total token budget
        max_cost_usd: 0.50  # TODO: change this
        max_duration_secs: 120  # TODO: change this

  # Completion log (exec)
  - id: done
    depends_on: [final_agent]
    with:
      result: $final_agent
    exec:
      command: |
        echo "============================================"
        echo "  FULL STACK WORKFLOW COMPLETE"
        echo "  5 verbs. MCP. Media. Vision. Agent."
        echo "  Report: {{with.result | length}} chars"
        echo "============================================"
      shell: true
"##;

const SUPERNOVAE_05_SOLUTION: &str = r##"# =============================================================================
# LEVEL 12 — BOSS FINAL: Full Stack
# =============================================================================

schema: "nika/workflow@0.12"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

mcp:
  filesystem:
    command: "npx"
    args:
      - "-y"
      - "@anthropic/mcp-filesystem"

inputs:
  topic:
    type: string
    default: "The Future of AI Orchestration"

tasks:
  # ── VERB 1: exec ─────────────────────────────────────────────
  - id: setup
    exec:
      command: |
        echo "Starting full-stack workflow"
        echo "Topic: {{inputs.topic}}"
        date +%Y-%m-%dT%H:%M:%S
      shell: true

  # ── VERB 2: fetch ────────────────────────────────────────────
  - id: scrape_data
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20

  - id: download_image
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  # ── VERB 3: invoke (builtin tools) ──────────────────────────
  - id: log_start
    depends_on: [setup]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Full-stack workflow initiated"

  - id: process_image
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.hash}}"
        steps:
          - op: thumbnail
            width: 300
          - op: convert
            format: webp
    artifact:
      path: output/processed-image.webp
      format: binary

  - id: generate_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Workflow Verb Usage"
        width: 800
        height: 500
        series:
          - name: "Frequency"
            data: [15, 12, 20, 8, 5]
        labels: ["exec", "fetch", "invoke", "infer", "agent"]

  # ── VERB 4: infer (structured + vision) ─────────────────────
  - id: analyze
    depends_on: [scrape_data, generate_chart]
    with:
      content: $scrape_data
      chart: $generate_chart
    infer:
      content:
        - type: image
          source: "{{with.chart.hash}}"
          detail: high
        - type: text
          text: |
            Analyze this chart and content about {{inputs.topic}}:
            {{with.content | first(2000)}}
            Return JSON with: summary, key_insights (array), score (1-100)
      response_format: json
      max_tokens: 1000
      temperature: 0.3
    artifact:
      path: output/analysis.json
      format: json

  # ── VERB 5: agent ────────────────────────────────────────────
  - id: final_agent
    depends_on: [analyze, process_image, log_start]
    with:
      analysis: $analyze
      image: $process_image
    agent:
      prompt: |
        You are the final synthesis agent. You have:

        ## Analysis Results
        {{with.analysis}}

        ## Processed Image
        {{with.image}}

        Create a comprehensive report covering:
        1. Data analysis findings
        2. Image processing results
        3. Overall assessment
        4. Next steps

        Use nika_log for progress. Call nika_complete when done.
      mcp: [filesystem]
      tools: [builtin]
      max_tokens: 2000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 200
          on_failure: retry
      limits:
        max_turns: 10
        max_tokens: 50000
        max_cost_usd: 1.00
        max_duration_secs: 180
    artifact:
      path: output/final-report.md

  # Completion log (exec)
  - id: done
    depends_on: [final_agent]
    with:
      result: $final_agent
    exec:
      command: |
        echo "============================================"
        echo "  FULL STACK WORKFLOW COMPLETE"
        echo "  5 verbs. MCP. Media. Vision. Agent."
        echo "  Report: {{with.result | length}} chars"
        echo "============================================"
      shell: true
    artifact:
      path: output/completion-log.txt
"##;

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_total_advanced_exercises() {
        // L7:3 + L8:3 + L9:4 + L10:3 + L11:4 + L12:5 = 22
        assert_eq!(
            EXERCISES_ADVANCED.len(),
            22,
            "Must have exactly 22 advanced exercises"
        );
    }

    #[test]
    fn test_exercise_counts_per_level() {
        let expected = [
            ("swiss-knife", 3),
            ("gone-rogue", 3),
            ("data-heist", 4),
            ("open-protocol", 3),
            ("pixel-pirate", 4),
            ("supernovae", 5),
        ];
        for (slug, count) in expected {
            let found = get_advanced_exercises(slug).len();
            assert_eq!(
                found, count,
                "Level '{}' should have {} exercises, found {}",
                slug, count, found
            );
        }
    }

    #[test]
    fn test_exercise_numbers_sequential() {
        let levels = [
            "swiss-knife",
            "gone-rogue",
            "data-heist",
            "open-protocol",
            "pixel-pirate",
            "supernovae",
        ];
        for slug in levels {
            let exercises = get_advanced_exercises(slug);
            for (i, ex) in exercises.iter().enumerate() {
                assert_eq!(
                    ex.exercise_num,
                    (i + 1) as u8,
                    "Exercise at index {} in level '{}' should be number {}",
                    i,
                    slug,
                    i + 1
                );
            }
        }
    }

    #[test]
    fn test_all_templates_have_todo() {
        for ex in EXERCISES_ADVANCED {
            assert!(
                ex.template.contains("TODO"),
                "Template for {}/{} must contain at least one TODO marker",
                ex.level_slug,
                ex.filename
            );
        }
    }

    #[test]
    fn test_no_solutions_have_todo() {
        for ex in EXERCISES_ADVANCED {
            assert!(
                !ex.solution.contains("TODO"),
                "Solution for {}/{} must NOT contain TODO markers",
                ex.level_slug,
                ex.filename
            );
        }
    }

    #[test]
    fn test_all_have_schema_declaration() {
        for ex in EXERCISES_ADVANCED {
            assert!(
                ex.solution.contains("schema: \"nika/workflow@0.12\""),
                "Solution for {}/{} must declare schema",
                ex.level_slug,
                ex.filename
            );
            assert!(
                ex.template.contains("schema: \"nika/workflow@0.12\""),
                "Template for {}/{} must declare schema",
                ex.level_slug,
                ex.filename
            );
        }
    }

    #[test]
    fn test_filenames_match_pattern() {
        for ex in EXERCISES_ADVANCED {
            assert!(
                ex.filename.ends_with(".nika.yaml"),
                "Filename '{}' must end with .nika.yaml",
                ex.filename
            );
            assert!(
                ex.filename.starts_with(&format!("{:02}", ex.exercise_num)),
                "Filename '{}' must start with zero-padded exercise number {:02}",
                ex.filename,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_get_advanced_exercise() {
        let ex = get_advanced_exercise("swiss-knife", 1);
        assert!(ex.is_some());
        assert_eq!(ex.unwrap().filename, "01-core-builtins.nika.yaml");
    }

    #[test]
    fn test_get_advanced_exercise_not_found() {
        assert!(get_advanced_exercise("nonexistent", 1).is_none());
        assert!(get_advanced_exercise("swiss-knife", 99).is_none());
    }

    #[test]
    fn test_supernovae_boss_is_comprehensive() {
        let boss_exercises = get_advanced_exercises("supernovae");
        assert_eq!(
            boss_exercises.len(),
            5,
            "SuperNovae boss must have 5 exercises"
        );

        // Boss 5 (Full Stack) should reference all 5 verbs
        let full_stack = get_advanced_exercise("supernovae", 5).unwrap();
        assert!(
            full_stack.solution.contains("exec:"),
            "Full Stack solution must use exec:"
        );
        assert!(
            full_stack.solution.contains("fetch:"),
            "Full Stack solution must use fetch:"
        );
        assert!(
            full_stack.solution.contains("invoke:"),
            "Full Stack solution must use invoke:"
        );
        assert!(
            full_stack.solution.contains("infer:"),
            "Full Stack solution must use infer:"
        );
        assert!(
            full_stack.solution.contains("agent:"),
            "Full Stack solution must use agent:"
        );
    }

    #[test]
    fn test_level_slugs_match_levels_rs() {
        let expected_slugs = [
            "swiss-knife",
            "gone-rogue",
            "data-heist",
            "open-protocol",
            "pixel-pirate",
            "supernovae",
        ];
        for slug in expected_slugs {
            assert!(
                !get_advanced_exercises(slug).is_empty(),
                "Must have exercises for level slug '{}'",
                slug
            );
        }
    }

    #[test]
    fn test_llm_exercises_use_provider_placeholder() {
        let llm_levels = ["gone-rogue", "supernovae"];
        for slug in llm_levels {
            for ex in get_advanced_exercises(slug) {
                assert!(
                    ex.template.contains("{{PROVIDER}}") || ex.template.contains("{{MODEL}}"),
                    "LLM exercise {}/{} should use provider/model placeholders",
                    ex.level_slug,
                    ex.filename
                );
            }
        }
    }
}
