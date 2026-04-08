// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exercise content — embedded YAML templates and solutions for Levels 1-6
//!
//! Each exercise has a template (with TODO markers) and a complete solution.
//! Templates are scaffolded files that guide learners through the concept.
//! Solutions are valid YAML that pass `nika check`.

/// Content for a single exercise
#[derive(Debug, Clone)]
pub struct ExerciseContent {
    /// Level slug (e.g., "jailbreak")
    pub level_slug: &'static str,
    /// Exercise number within the level (1-based)
    pub exercise_num: u8,
    /// Filename (e.g., "01-hello-world.nika.yaml")
    pub filename: &'static str,
    /// Exercise template with TODO markers for the learner to complete
    pub template: &'static str,
    /// Complete working solution
    pub solution: &'static str,
}

/// All exercise content for Levels 1-6 (22 exercises)
pub static EXERCISES: &[ExerciseContent] = &[
    // ═════════════════════════════════════════════════════════════════════════
    // LEVEL 01: JAILBREAK — Break free from manual commands (5 exercises)
    // ═════════════════════════════════════════════════════════════════════════

    // ── 01-01: Hello World ──────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "jailbreak",
        exercise_num: 1,
        filename: "01-hello-world.nika.yaml",
        template: JAILBREAK_01_TEMPLATE,
        solution: JAILBREAK_01_SOLUTION,
    },
    // ── 01-02: Shell Commands ───────────────────────────────────────────────
    ExerciseContent {
        level_slug: "jailbreak",
        exercise_num: 2,
        filename: "02-shell-commands.nika.yaml",
        template: JAILBREAK_02_TEMPLATE,
        solution: JAILBREAK_02_SOLUTION,
    },
    // ── 01-03: HTTP Requests ────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "jailbreak",
        exercise_num: 3,
        filename: "03-http-requests.nika.yaml",
        template: JAILBREAK_03_TEMPLATE,
        solution: JAILBREAK_03_SOLUTION,
    },
    // ── 01-04: Provider Selection ───────────────────────────────────────────
    ExerciseContent {
        level_slug: "jailbreak",
        exercise_num: 4,
        filename: "04-provider-selection.nika.yaml",
        template: JAILBREAK_04_TEMPLATE,
        solution: JAILBREAK_04_SOLUTION,
    },
    // ── 01-05: Validation ───────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "jailbreak",
        exercise_num: 5,
        filename: "05-validation.nika.yaml",
        template: JAILBREAK_05_TEMPLATE,
        solution: JAILBREAK_05_SOLUTION,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // LEVEL 02: HOT WIRE — Wire up the data flow (4 exercises)
    // ═════════════════════════════════════════════════════════════════════════

    // ── 02-01: Simple Binding ───────────────────────────────────────────────
    ExerciseContent {
        level_slug: "hot-wire",
        exercise_num: 1,
        filename: "01-simple-binding.nika.yaml",
        template: HOT_WIRE_01_TEMPLATE,
        solution: HOT_WIRE_01_SOLUTION,
    },
    // ── 02-02: Nested JSON ──────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "hot-wire",
        exercise_num: 2,
        filename: "02-nested-json.nika.yaml",
        template: HOT_WIRE_02_TEMPLATE,
        solution: HOT_WIRE_02_SOLUTION,
    },
    // ── 02-03: Transforms ───────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "hot-wire",
        exercise_num: 3,
        filename: "03-transforms.nika.yaml",
        template: HOT_WIRE_03_TEMPLATE,
        solution: HOT_WIRE_03_SOLUTION,
    },
    // ── 02-04: Env Bindings ─────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "hot-wire",
        exercise_num: 4,
        filename: "04-env-bindings.nika.yaml",
        template: HOT_WIRE_04_TEMPLATE,
        solution: HOT_WIRE_04_SOLUTION,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // LEVEL 03: FORK BOMB — Multiply your power (4 exercises)
    // ═════════════════════════════════════════════════════════════════════════

    // ── 03-01: Parallel Diamond ─────────────────────────────────────────────
    ExerciseContent {
        level_slug: "fork-bomb",
        exercise_num: 1,
        filename: "01-parallel-diamond.nika.yaml",
        template: FORK_BOMB_01_TEMPLATE,
        solution: FORK_BOMB_01_SOLUTION,
    },
    // ── 03-02: For Each Basic ───────────────────────────────────────────────
    ExerciseContent {
        level_slug: "fork-bomb",
        exercise_num: 2,
        filename: "02-for-each-basic.nika.yaml",
        template: FORK_BOMB_02_TEMPLATE,
        solution: FORK_BOMB_02_SOLUTION,
    },
    // ── 03-03: For Each Concurrent ──────────────────────────────────────────
    ExerciseContent {
        level_slug: "fork-bomb",
        exercise_num: 3,
        filename: "03-for-each-concurrent.nika.yaml",
        template: FORK_BOMB_03_TEMPLATE,
        solution: FORK_BOMB_03_SOLUTION,
    },
    // ── 03-04: Chained Pipeline ─────────────────────────────────────────────
    ExerciseContent {
        level_slug: "fork-bomb",
        exercise_num: 4,
        filename: "04-chained-pipeline.nika.yaml",
        template: FORK_BOMB_04_TEMPLATE,
        solution: FORK_BOMB_04_SOLUTION,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // LEVEL 04: ROOT ACCESS — Unlock deeper workflow power (3 exercises)
    // ═════════════════════════════════════════════════════════════════════════

    // ── 04-01: Context Files ────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "root-access",
        exercise_num: 1,
        filename: "01-context-files.nika.yaml",
        template: ROOT_ACCESS_01_TEMPLATE,
        solution: ROOT_ACCESS_01_SOLUTION,
    },
    // ── 04-02: Imports ──────────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "root-access",
        exercise_num: 2,
        filename: "02-imports.nika.yaml",
        template: ROOT_ACCESS_02_TEMPLATE,
        solution: ROOT_ACCESS_02_SOLUTION,
    },
    // ── 04-03: Inputs ───────────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "root-access",
        exercise_num: 3,
        filename: "03-inputs.nika.yaml",
        template: ROOT_ACCESS_03_TEMPLATE,
        solution: ROOT_ACCESS_03_SOLUTION,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // LEVEL 05: SHAPESHIFTER — Transform and validate output (3 exercises)
    // ═════════════════════════════════════════════════════════════════════════

    // ── 05-01: Structured Output ────────────────────────────────────────────
    ExerciseContent {
        level_slug: "shapeshifter",
        exercise_num: 1,
        filename: "01-structured-output.nika.yaml",
        template: SHAPESHIFTER_01_TEMPLATE,
        solution: SHAPESHIFTER_01_SOLUTION,
    },
    // ── 05-02: Artifacts ────────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "shapeshifter",
        exercise_num: 2,
        filename: "02-artifacts.nika.yaml",
        template: SHAPESHIFTER_02_TEMPLATE,
        solution: SHAPESHIFTER_02_SOLUTION,
    },
    // ── 05-03: Schema Retry ─────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "shapeshifter",
        exercise_num: 3,
        filename: "03-schema-retry.nika.yaml",
        template: SHAPESHIFTER_03_TEMPLATE,
        solution: SHAPESHIFTER_03_SOLUTION,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // LEVEL 06: PAY-PER-DREAM — Master the LLM landscape (3 exercises)
    // ═════════════════════════════════════════════════════════════════════════

    // ── 06-01: Multi-Provider ───────────────────────────────────────────────
    ExerciseContent {
        level_slug: "pay-per-dream",
        exercise_num: 1,
        filename: "01-multi-provider.nika.yaml",
        template: PAY_PER_DREAM_01_TEMPLATE,
        solution: PAY_PER_DREAM_01_SOLUTION,
    },
    // ── 06-02: Native Local ─────────────────────────────────────────────────
    ExerciseContent {
        level_slug: "pay-per-dream",
        exercise_num: 2,
        filename: "02-native-local.nika.yaml",
        template: PAY_PER_DREAM_02_TEMPLATE,
        solution: PAY_PER_DREAM_02_SOLUTION,
    },
    // ── 06-03: System Prompts ───────────────────────────────────────────────
    ExerciseContent {
        level_slug: "pay-per-dream",
        exercise_num: 3,
        filename: "03-system-prompts.nika.yaml",
        template: PAY_PER_DREAM_03_TEMPLATE,
        solution: PAY_PER_DREAM_03_SOLUTION,
    },
];

/// Get all exercises for a given level slug
pub fn get_exercises(level_slug: &str) -> Vec<&'static ExerciseContent> {
    EXERCISES
        .iter()
        .filter(|e| e.level_slug == level_slug)
        .collect()
}

/// Get a specific exercise by level slug and exercise number
pub fn get_exercise(level_slug: &str, num: u8) -> Option<&'static ExerciseContent> {
    EXERCISES
        .iter()
        .find(|e| e.level_slug == level_slug && e.exercise_num == num)
}

/// Get ALL exercises across all levels (1-12)
pub fn all_exercises() -> Vec<&'static ExerciseContent> {
    let mut all: Vec<&'static ExerciseContent> = EXERCISES.iter().collect();
    all.extend(super::exercises_advanced::EXERCISES_ADVANCED.iter());
    all
}

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 01: JAILBREAK
// ═════════════════════════════════════════════════════════════════════════════

// ── 01-01: Hello World ──────────────────────────────────────────────────────

const JAILBREAK_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 1: Hello World
# =============================================================================
#
# Your first Nika workflow. Learn the three parts every workflow needs:
# schema declaration, workflow name, and a task with the infer: verb.
#
# CONCEPTS:
#   - schema: "nika/workflow@0.12" — required header
#   - workflow: — human-readable name
#   - infer: — LLM text generation (shorthand and full form)
#
# RUN:   nika run 01-hello-world.nika.yaml
# CHECK: nika check 01-hello-world.nika.yaml
# =============================================================================

# TODO: Understand the schema declaration — every workflow needs this line:
schema: "nika/workflow@0.12"

# TODO: Understand the workflow name — a short kebab-case identifier:
workflow: hello-world

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with your hello world tasks!'"

  # TODO: Create a task with id "hello" that uses infer: in shorthand form
  #       (just a string prompt asking the LLM to say hello)

  # TODO: Create a task with id "hello_detailed" that uses infer: in full form
  #       with prompt:, system:, temperature: 0.7, and max_tokens: 150
  #       Make it depend on the "hello" task with depends_on:
"##;

const JAILBREAK_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 1: Hello World (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: hello-world

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: hello
    infer: "Say hello to the world in 3 different languages. Keep it short and fun!"

  - id: hello_detailed
    depends_on: [hello]
    infer:
      prompt: "Now say goodbye to the world in 3 different languages. Be poetic!"
      system: "You are a multilingual poet who writes with warmth and elegance."
      temperature: 0.7
      max_tokens: 150
"##;

// ── 01-02: Shell Commands ───────────────────────────────────────────────────

const JAILBREAK_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 2: Shell Commands
# =============================================================================
#
# The exec: verb runs shell commands. No LLM provider needed!
#
# CONCEPTS:
#   - exec: shorthand (shell-free, secure by default)
#   - exec: full form with command:, shell: true, timeout:, env:
#   - cwd: to change working directory
#
# RUN: nika run 02-shell-commands.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: shell-commands

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with shell command tasks!'"

  # TODO: Create a task "list_files" that uses exec: in shorthand form
  #       to run "ls -la" (shell-free mode, the default)

  # TODO: Create a task "system_info" that uses exec: full form with:
  #       - command: "uname -s && whoami && date '+%Y-%m-%d'"
  #       - shell: true (enables pipes and chaining)

  # TODO: Create a task "with_timeout" that runs a command with:
  #       - command: "echo 'Processing...'"
  #       - shell: true
  #       - timeout: 5 (seconds)

  # TODO: Create a task "with_env" that uses env: variables:
  #       - command: "echo \"Hello $GREETING from $LOCATION\""
  #       - shell: true
  #       - env: with GREETING and LOCATION values
"##;

const JAILBREAK_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 2: Shell Commands (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: shell-commands

tasks:
  - id: list_files
    exec: "ls -la"

  - id: system_info
    exec:
      command: "uname -s && whoami && date '+%Y-%m-%d'"
      shell: true

  - id: with_timeout
    exec:
      command: "echo 'Processing...'"
      shell: true
      timeout: 5

  - id: with_env
    exec:
      command: "echo \"Hello $GREETING from $LOCATION\""
      shell: true
      env:
        GREETING: "Nika"
        LOCATION: "the workflow engine"
"##;

// ── 01-03: HTTP Requests ────────────────────────────────────────────────────

const JAILBREAK_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 3: HTTP Requests
# =============================================================================
#
# The fetch: verb makes HTTP requests. No LLM provider needed!
#
# CONCEPTS:
#   - GET request with url: and headers:
#   - POST request with method:, json:
#   - extract: jsonpath for querying JSON responses
#   - response: full for status + headers + body
#
# RUN: nika run 03-http-requests.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: http-requests

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with fetch: tasks!'"

  # TODO: Create a task "get_ip" that fetches your public IP
  #       - url: "https://httpbin.org/ip"

  # TODO: Create a task "post_data" that sends a POST request
  #       - url: "https://httpbin.org/post"
  #       - method: POST
  #       - json: with name: "Nika" and version: "0.38"

  # TODO: Create a task "with_headers" that sends custom headers
  #       - url: "https://httpbin.org/get"
  #       - headers: with Accept: "application/json"
  #       - response: full (to see status, headers, and body)

  # TODO: Create a task "extract_origin" that uses jsonpath extraction
  #       - url: "https://httpbin.org/ip"
  #       - extract: jsonpath
  #       - selector: "$.origin"
"##;

const JAILBREAK_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 3: HTTP Requests (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: http-requests

tasks:
  - id: get_ip
    fetch:
      url: "https://httpbin.org/ip"

  - id: post_data
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      json:
        name: "Nika"
        version: "0.38"

  - id: with_headers
    fetch:
      url: "https://httpbin.org/get"
      headers:
        Accept: "application/json"
      response: full

  - id: extract_origin
    fetch:
      url: "https://httpbin.org/ip"
      extract: jsonpath
      selector: "$.origin"
"##;

// ── 01-04: Provider Selection ───────────────────────────────────────────────

const JAILBREAK_04_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 4: Provider Selection
# =============================================================================
#
# Nika supports multiple LLM providers. Set a workflow-level default,
# then override per-task with provider: and model: at the task level.
#
# CONCEPTS:
#   - provider: / model: at workflow level (default for all tasks)
#   - provider: / model: at task level (override for one task)
#   - system:, temperature:, max_tokens: in infer: block
#
# SETUP: nika keys set openai (or any provider you have)
# RUN:   nika run 04-provider-selection.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: provider-selection

# TODO: Set a workflow-level provider and model
#       (use placeholders: provider: "{{PROVIDER}}", model: "{{MODEL}}")

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with infer: tasks using different providers!'"

  # TODO: Create a task "quick_draft" with infer: shorthand
  #       Uses the workflow-level default provider

  # TODO: Create a task "detailed_analysis" that overrides provider and model
  #       at the task level with provider: and model: fields
  #       Use infer: full form with system:, temperature:, max_tokens:

  # TODO: Create a task "final_summary" that depends on both tasks above
  #       and uses yet another provider/model combination
"##;

const JAILBREAK_04_SOLUTION: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 4: Provider Selection (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: provider-selection

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: quick_draft
    infer: "List 3 benefits of declarative workflow engines. Be concise."

  - id: detailed_analysis
    infer:
      prompt: "Explain the difference between imperative and declarative automation in 2 paragraphs."
      system: "You are a software architecture instructor. Use clear examples."
      temperature: 0.5
      max_tokens: 300

  - id: final_summary
    depends_on: [quick_draft, detailed_analysis]
    infer:
      prompt: "Write a one-sentence conclusion about why declarative workflows matter."
      temperature: 0.3
      max_tokens: 100
"##;

// ── 01-05: Validation ───────────────────────────────────────────────────────

const JAILBREAK_05_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 5: Validation & DAG
# =============================================================================
#
# Build a multi-task workflow and validate it with nika check.
# Combine all verbs and features from this level into one workflow.
#
# CONCEPTS:
#   - depends_on: for task ordering (DAG edges)
#   - Parallel execution (tasks without dependencies run simultaneously)
#   - nika check validates structure, IDs, DAG (no cycles)
#
# CHECK: nika check 05-validation.nika.yaml
# RUN:   nika run 05-validation.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: validation-dag

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with a multi-task DAG!'"

  # TODO: Create a task "gather_data" using exec: to collect system info

  # TODO: Create a task "fetch_api" using fetch: to call an API
  #       This should run in PARALLEL with gather_data (no depends_on)

  # TODO: Create a task "process" that depends on BOTH gather_data and fetch_api
  #       Use depends_on: [gather_data, fetch_api] to create a fan-in point

  # TODO: Create a task "report" that depends on "process"
  #       This forms the final link in the DAG chain
  #
  # The DAG should look like:
  #   gather_data ──┐
  #                  ├──→ process ──→ report
  #   fetch_api ────┘
"##;

const JAILBREAK_05_SOLUTION: &str = r##"# =============================================================================
# LEVEL 1 — EXERCISE 5: Validation & DAG (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: validation-dag

tasks:
  - id: gather_data
    exec:
      command: "uname -s && whoami"
      shell: true

  - id: fetch_api
    fetch:
      url: "https://httpbin.org/ip"

  - id: process
    depends_on: [gather_data, fetch_api]
    exec:
      command: "echo 'Both data sources ready. Processing...'"
      shell: true

  - id: report
    depends_on: [process]
    exec:
      command: "echo 'Final report generated. DAG complete.'"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 02: HOT WIRE
// ═════════════════════════════════════════════════════════════════════════════

// ── 02-01: Simple Binding ───────────────────────────────────────────────────

const HOT_WIRE_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 1: Simple Binding
# =============================================================================
#
# Bindings connect task outputs to task inputs. The with: block declares
# aliases, and {{with.alias}} templates inject the values.
#
# CONCEPTS:
#   - with: block for declaring bindings
#   - $task_id syntax to reference a task's output ($ prefix required)
#   - {{with.alias}} template injection in prompts, commands, URLs
#   - Automatic depends_on inference from with: references
#
# RUN: nika run 01-simple-binding.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: simple-binding

tasks:
  # Step 1: Produce data
  - id: get_date
    exec:
      command: "date '+%A, %B %d, %Y'"
      shell: true

  - id: get_hostname
    exec: "hostname"

  # TODO: Create a task "show_date" with:
  #       - with: block that binds "today" to $get_date
  #       - exec: command that uses {{with.today}} in the output
  #       (No depends_on needed — with: auto-infers it)

  # TODO: Create a task "combine" with:
  #       - with: block that binds BOTH $get_date and $get_hostname
  #         (e.g., date_str: $get_date, host: $get_hostname)
  #       - exec: command that uses BOTH {{with.date_str}} and {{with.host}}
"##;

const HOT_WIRE_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 1: Simple Binding (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: simple-binding

tasks:
  - id: get_date
    exec:
      command: "date '+%A, %B %d, %Y'"
      shell: true

  - id: get_hostname
    exec: "hostname"

  - id: show_date
    with:
      today: $get_date
    exec:
      command: "echo 'Today is: {{with.today}}'"
      shell: true

  - id: combine
    with:
      date_str: $get_date
      host: $get_hostname
    exec:
      command: "echo 'Report from {{with.host}} on {{with.date_str}}'"
      shell: true
"##;

// ── 02-02: Nested JSON ─────────────────────────────────────────────────────

const HOT_WIRE_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 2: Nested JSON Access
# =============================================================================
#
# When tasks return JSON, you can navigate into nested structures
# using dot notation, array indexing, and the ?? default operator.
#
# CONCEPTS:
#   - Dot notation: {{with.data.field.nested}}
#   - Array indexing: {{with.data.items[0]}}
#   - Default operator in with: block: email: $task.user.email ?? "fallback"
#   - parse_json transform to convert string output to JSON
#
# RUN: nika run 02-nested-json.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: nested-json

tasks:
  - id: json_source
    exec:
      command: |
        echo '{"user": {"name": "Nika", "level": 2}, "tags": ["workflow", "ai", "yaml"], "score": 42}'
      shell: true

  # TODO: Create a task "read_nested" with:
  #       - with: block that binds data: $json_source | parse_json
  #       - exec: command that accesses:
  #         - {{with.data.user.name}} (dot notation into nested object)
  #         - {{with.data.tags[0]}} (array indexing)
  #         - {{with.data.score}} (top-level field)

  # TODO: Create a task "with_defaults" with:
  #       - with: block that uses ?? in the binding (NOT in the template):
  #         email: $json_source.user.email ?? "unknown"
  #         missing: $json_source.missing_field ?? "not-found"
  #       - exec: command that uses {{with.email}} and {{with.missing}}
"##;

const HOT_WIRE_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 2: Nested JSON Access (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: nested-json

tasks:
  - id: json_source
    exec:
      command: |
        echo '{"user": {"name": "Nika", "level": 2}, "tags": ["workflow", "ai", "yaml"], "score": 42}'
      shell: true

  - id: read_nested
    with:
      data: $json_source | parse_json
    exec:
      command: |
        echo "User: {{with.data.user.name}}"
        echo "First tag: {{with.data.tags[0]}}"
        echo "Score: {{with.data.score}}"
      shell: true

  - id: with_defaults
    with:
      email: $json_source.user.email ?? "unknown"
      missing: $json_source.missing_field ?? "not-found"
    exec:
      command: |
        echo "Email: {{with.email}}"
        echo "Missing: {{with.missing}}"
      shell: true
"##;

// ── 02-03: Transforms ──────────────────────────────────────────────────────

const HOT_WIRE_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 3: Pipe Transforms
# =============================================================================
#
# Transforms modify data in the binding pipeline using pipe syntax.
# They work in both with: blocks and {{...}} templates.
#
# CONCEPTS:
#   - String transforms: upper, lower, trim, length
#   - Collection transforms: sort, unique, first, last, join
#   - Pipe chaining: $source | parse_json | sort | first
#   - Template transforms: {{with.data | upper | trim}}
#
# RUN: nika run 03-transforms.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: transforms

tasks:
  - id: text_source
    exec:
      command: "echo '  Hello, Nika World!  '"
      shell: true

  - id: array_source
    exec:
      command: |
        echo '["banana", "apple", "cherry", "apple", "date"]'
      shell: true

  # TODO: Create a task "string_ops" with:
  #       - with: block applying transforms to $text_source:
  #         - uppercased: $text_source | upper
  #         - trimmed: $text_source | trim
  #         - clean_upper: $text_source | trim | upper (chained)
  #       - exec: command that displays all three values

  # TODO: Create a task "collection_ops" with:
  #       - with: block applying transforms to $array_source:
  #         - sorted: $array_source | parse_json | sort
  #         - unique_items: $array_source | parse_json | sort | unique
  #         - first_item: $array_source | parse_json | sort | unique | first
  #         - joined: $array_source | parse_json | sort | unique | join(", ")
  #       - exec: command that displays the results

  # TODO: Create a task "template_transforms" with:
  #       - with: that binds raw data (no transforms in with:)
  #         - text: $text_source
  #         - fruits: $array_source
  #       - exec: command using transforms INSIDE templates:
  #         - {{with.text | trim | upper}}
  #         - {{with.fruits | parse_json | length}}
"##;

const HOT_WIRE_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 3: Pipe Transforms (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: transforms

tasks:
  - id: text_source
    exec:
      command: "echo '  Hello, Nika World!  '"
      shell: true

  - id: array_source
    exec:
      command: |
        echo '["banana", "apple", "cherry", "apple", "date"]'
      shell: true

  - id: string_ops
    with:
      uppercased: $text_source | upper
      trimmed: $text_source | trim
      clean_upper: $text_source | trim | upper
    exec:
      command: |
        echo "Uppercased: {{with.uppercased}}"
        echo "Trimmed: [{{with.trimmed}}]"
        echo "Chained: {{with.clean_upper}}"
      shell: true

  - id: collection_ops
    with:
      sorted: $array_source | parse_json | sort
      unique_items: $array_source | parse_json | sort | unique
      first_item: $array_source | parse_json | sort | unique | first
      joined: $array_source | parse_json | sort | unique | join(", ")
    exec:
      command: |
        echo "Sorted: {{with.sorted}}"
        echo "Unique: {{with.unique_items}}"
        echo "First: {{with.first_item}}"
        echo "Joined: {{with.joined}}"
      shell: true

  - id: template_transforms
    with:
      text: $text_source
      fruits: $array_source
    exec:
      command: |
        echo "Clean text: {{with.text | trim | upper}}"
        echo "Fruit count: {{with.fruits | parse_json | length}}"
      shell: true
"##;

// ── 02-04: Env Bindings ────────────────────────────────────────────────────

const HOT_WIRE_04_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 4: Environment Bindings
# =============================================================================
#
# Access environment variables in your workflows with $env.VAR syntax.
# Combine env vars with task output bindings.
#
# CONCEPTS:
#   - $env.VAR to read environment variables
#   - Combining $env with $task_id in the same with: block
#   - Fallback with ?? when env var might not be set
#
# RUN: MY_NAME=Nika nika run 04-env-bindings.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: env-bindings

tasks:
  - id: get_time
    exec:
      command: "date '+%H:%M'"
      shell: true

  # TODO: Create a task "env_demo" with:
  #       - with: block that reads environment variables:
  #         - user: $env.USER (your system username)
  #         - home: $env.HOME (home directory)
  #       - exec: command that displays both values

  # TODO: Create a task "combined" with:
  #       - with: block that combines env vars AND task output:
  #         - user: $env.USER
  #         - time: $get_time
  #       - exec: command: "echo 'Hello {{with.user}}, the time is {{with.time}}'"

  # TODO: Create a task "with_fallback" with:
  #       - with: block using ?? for missing env vars:
  #         - name: $env.MY_NAME ?? "Anonymous"
  #       - exec: command that displays the name (falls back to "Anonymous")
"##;

const HOT_WIRE_04_SOLUTION: &str = r##"# =============================================================================
# LEVEL 2 — EXERCISE 4: Environment Bindings (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: env-bindings

tasks:
  - id: get_time
    exec:
      command: "date '+%H:%M'"
      shell: true

  - id: env_demo
    with:
      user: $env.USER
      home: $env.HOME
    exec:
      command: |
        echo "User: {{with.user}}"
        echo "Home: {{with.home}}"
      shell: true

  - id: combined
    with:
      user: $env.USER
      time: $get_time
    exec:
      command: "echo 'Hello {{with.user}}, the time is {{with.time}}'"
      shell: true

  - id: with_fallback
    with:
      name: $env.MY_NAME ?? "Anonymous"
    exec:
      command: "echo 'Welcome, {{with.name}}!'"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 03: FORK BOMB
// ═════════════════════════════════════════════════════════════════════════════

// ── 03-01: Parallel Diamond ────────────────────────────────────────────────

const FORK_BOMB_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 1: Parallel Diamond
# =============================================================================
#
# The diamond pattern: one task fans out to parallel tasks, then
# a final task fans back in to gather all results.
#
#        source
#       /  |  \
#      v   v   v     <- FAN-OUT (parallel)
#     a    b    c
#      \   |   /
#       v  v  v      <- FAN-IN (convergence)
#       summary
#
# CONCEPTS:
#   - Fan-out: multiple tasks depending on the same upstream task
#   - Fan-in: one task depending on multiple upstream tasks
#   - Parallel execution: independent tasks run simultaneously
#   - with: to gather multiple outputs at the convergence point
#
# RUN:   nika run 01-parallel-diamond.nika.yaml
# GRAPH: nika workflow graph 01-parallel-diamond.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: parallel-diamond

tasks:
  # Source task: the root of the diamond
  - id: source
    exec:
      command: "echo 'Nika workflow engine'"
      shell: true

  # TODO: Create 3 parallel tasks (branch_a, branch_b, branch_c)
  #       Each should:
  #       - depends_on: [source]
  #       - with: { data: $source }
  #       - exec: a command that processes the data differently
  #       Since they all depend only on "source" (not each other),
  #       they run in PARALLEL automatically

  # TODO: Create a task "summary" that fans in ALL three branches:
  #       - depends_on: [branch_a, branch_b, branch_c]
  #       - with: bindings for all three outputs
  #       - exec: command that displays the combined results
"##;

const FORK_BOMB_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 1: Parallel Diamond (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: parallel-diamond

tasks:
  - id: source
    exec:
      command: "echo 'Nika workflow engine'"
      shell: true

  - id: branch_a
    depends_on: [source]
    with:
      data: $source
    exec:
      command: "echo 'Branch A processed: {{with.data}} [uppercase mode]'"
      shell: true

  - id: branch_b
    depends_on: [source]
    with:
      data: $source
    exec:
      command: "echo 'Branch B processed: {{with.data}} [analysis mode]'"
      shell: true

  - id: branch_c
    depends_on: [source]
    with:
      data: $source
    exec:
      command: "echo 'Branch C processed: {{with.data}} [validation mode]'"
      shell: true

  - id: summary
    depends_on: [branch_a, branch_b, branch_c]
    with:
      a: $branch_a
      b: $branch_b
      c: $branch_c
    exec:
      command: |
        echo "=== Diamond Complete ==="
        echo "A: {{with.a}}"
        echo "B: {{with.b}}"
        echo "C: {{with.c}}"
      shell: true
"##;

// ── 03-02: For Each Basic ──────────────────────────────────────────────────

const FORK_BOMB_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 2: For Each (Basic)
# =============================================================================
#
# for_each runs a task once per item in a list. Each iteration
# gets the current item via the as: variable.
#
# CONCEPTS:
#   - for_each: [list of items] — iterate over a static list
#   - as: variable_name — name for the current item
#   - {{with.variable}} — access the current item in templates (as: value goes into with:)
#
# RUN: nika run 02-for-each-basic.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: for-each-basic

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with for_each tasks!'"

  # TODO: Create a task "greet_languages" with:
  #       - for_each: ["English", "French", "Japanese"]
  #       - as: lang
  #       - exec: command that prints "Hello in {{with.lang}}!"

  # TODO: Create a task "process_numbers" with:
  #       - for_each: [1, 2, 3, 4, 5]
  #       - as: num
  #       - exec: command that prints "Processing item {{with.num}}..."
"##;

const FORK_BOMB_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 2: For Each Basic (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: for-each-basic

tasks:
  - id: greet_languages
    for_each: ["English", "French", "Japanese"]
    as: lang
    exec:
      command: "echo 'Hello in {{with.lang}}!'"
      shell: true

  - id: process_numbers
    for_each: [1, 2, 3, 4, 5]
    as: num
    exec:
      command: "echo 'Processing item {{with.num}}...'"
      shell: true
"##;

// ── 03-03: For Each Concurrent ─────────────────────────────────────────────

const FORK_BOMB_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 3: For Each Concurrent
# =============================================================================
#
# Control parallelism with concurrency: and error handling with fail_fast:.
#
# CONCEPTS:
#   - concurrency: N — limit how many iterations run in parallel
#   - fail_fast: true/false — stop all iterations on first failure
#   - for_each with dynamic lists from task output
#
# RUN: nika run 03-for-each-concurrent.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: for-each-concurrent

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with concurrent for_each tasks!'"

  # TODO: Create a task "generate_list" that produces a JSON array
  #       exec: command that echoes '["alpha", "bravo", "charlie", "delta", "echo"]'

  # TODO: Create a task "process_batch" with:
  #       - depends_on: [generate_list]
  #       - with: { items: $generate_list | parse_json }
  #       - for_each: "{{with.items}}"
  #       - as: item
  #       - concurrency: 2 (only 2 iterations run at a time)
  #       - fail_fast: false (continue even if one fails)
  #       - exec: command that processes each item
"##;

const FORK_BOMB_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 3: For Each Concurrent (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: for-each-concurrent

tasks:
  - id: generate_list
    exec:
      command: |
        echo '["alpha", "bravo", "charlie", "delta", "echo"]'
      shell: true

  - id: process_batch
    depends_on: [generate_list]
    with:
      items: $generate_list | parse_json
    for_each: "{{with.items}}"
    as: item
    concurrency: 2
    fail_fast: false
    exec:
      command: "echo 'Processing: {{with.item}}'"
      shell: true
"##;

// ── 03-04: Chained Pipeline ────────────────────────────────────────────────

const FORK_BOMB_04_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 4: Chained Pipeline
# =============================================================================
#
# Build a multi-stage pipeline where each for_each feeds into the next.
# Stage 1 generates data, Stage 2 transforms it, Stage 3 aggregates.
#
# CONCEPTS:
#   - Multi-stage for_each: output of one feeds the next
#   - Combining depends_on with for_each
#   - Final aggregation after all iterations complete
#
# RUN: nika run 04-chained-pipeline.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: chained-pipeline

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with a chained pipeline!'"

  # TODO: Create a task "urls" that produces a JSON array of URLs
  #       exec: echo '["https://httpbin.org/ip", "https://httpbin.org/uuid", "https://httpbin.org/user-agent"]'

  # TODO: Create a task "fetch_all" with:
  #       - depends_on: [urls]
  #       - with: { endpoints: $urls | parse_json }
  #       - for_each: "{{with.endpoints}}"
  #       - as: url
  #       - fetch: with url: "{{with.url}}"

  # TODO: Create a task "done" that depends on fetch_all
  #       exec: prints a completion message
"##;

const FORK_BOMB_04_SOLUTION: &str = r##"# =============================================================================
# LEVEL 3 — EXERCISE 4: Chained Pipeline (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: chained-pipeline

tasks:
  - id: urls
    exec:
      command: |
        echo '["https://httpbin.org/ip", "https://httpbin.org/uuid", "https://httpbin.org/user-agent"]'
      shell: true

  - id: fetch_all
    depends_on: [urls]
    with:
      endpoints: $urls | parse_json
    for_each: "{{with.endpoints}}"
    as: url
    fetch:
      url: "{{with.url}}"

  - id: done
    depends_on: [fetch_all]
    exec:
      command: "echo 'All endpoints fetched successfully!'"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 04: ROOT ACCESS
// ═════════════════════════════════════════════════════════════════════════════

// ── 04-01: Context Files ───────────────────────────────────────────────────

const ROOT_ACCESS_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 4 — EXERCISE 1: Context Files
# =============================================================================
#
# Load external files as context for your tasks. The context.files block
# makes file contents available in your prompts and commands.
#
# CONCEPTS:
#   - context.files: load .md, .json, .txt files
#   - {{context.files.alias}} to inject file contents
#   - Separating data from workflow logic
#   - provider: / model: required for infer: tasks
#
# RUN: nika run 01-context-files.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: context-files

# TODO: Add provider and model (use {{PROVIDER}} and {{MODEL}} placeholders)

# TODO: Add a context block with files:
#       context:
#         files:
#           readme: ./MISSION.md
#
# (MISSION.md exists in this level directory)

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with context-aware tasks!'"

  # TODO: Create a task "summarize" that uses context files
  #       in an infer: prompt via {{context.files.readme}}

  # TODO: Create a task "show_context" that depends on summarize
  #       exec: command confirming context was loaded
"##;

const ROOT_ACCESS_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 4 — EXERCISE 1: Context Files (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: context-files

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

context:
  files:
    readme: ./MISSION.md

tasks:
  - id: summarize
    infer:
      prompt: |
        Summarize this document in 3 bullet points:

        {{context.files.readme}}
      max_tokens: 200

  - id: show_context
    depends_on: [summarize]
    exec:
      command: "echo 'Context was loaded and used for summarization.'"
      shell: true
"##;

// ── 04-02: Imports ─────────────────────────────────────────────────────────

const ROOT_ACCESS_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 4 — EXERCISE 2: Multi-Step Data Pipeline
# =============================================================================
#
# Build a 4-stage data pipeline: extract, transform, validate, load.
# Combine fetch:, exec:, with: bindings, and depends_on into a real pipeline.
#
# CONCEPTS:
#   - Multi-stage pipeline with depends_on chaining
#   - Data flowing through with: bindings at each stage
#   - Mixing verbs (fetch + exec) in a single workflow
#   - Pipeline pattern: extract → transform → validate → load
#
# RUN: nika run 02-imports.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: data-pipeline

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with a data pipeline!'"

  # TODO: Create a task "extract" that fetches data from an API
  #       fetch: url: "https://httpbin.org/json"

  # TODO: Create a task "transform" that processes the fetched data
  #       - depends_on: [extract]
  #       - with: { raw: $extract | parse_json }
  #       - exec: command that transforms the data

  # TODO: Create a task "validate" that checks the transformed data
  #       - depends_on: [transform]
  #       - with: { data: $transform }
  #       - exec: command that validates the output

  # TODO: Create a task "load" that finalizes the pipeline
  #       - depends_on: [validate]
  #       - exec: command that reports completion
"##;

const ROOT_ACCESS_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 4 — EXERCISE 2: Multi-Step Data Pipeline (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: data-pipeline

tasks:
  - id: extract
    fetch:
      url: "https://httpbin.org/json"

  - id: transform
    depends_on: [extract]
    with:
      raw: $extract | parse_json
    exec:
      command: "echo 'Transformed data from: {{with.raw}}'"
      shell: true

  - id: validate
    depends_on: [transform]
    with:
      data: $transform
    exec:
      command: "echo 'Validation passed for: {{with.data}}'"
      shell: true

  - id: load
    depends_on: [validate]
    exec:
      command: "echo 'Pipeline complete. Data loaded successfully.'"
      shell: true
"##;

// ── 04-03: Inputs ──────────────────────────────────────────────────────────

const ROOT_ACCESS_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 4 — EXERCISE 3: Inputs
# =============================================================================
#
# Make workflows reusable with inputs: — declare parameters with defaults
# that can be overridden from the CLI with --input key=value.
#
# CONCEPTS:
#   - inputs: block with name, type, default values
#   - {{inputs.name}} to use input values in tasks
#   - CLI override: nika run file.yaml --input name=value
#
# RUN: nika run 03-inputs.nika.yaml
# RUN: nika run 03-inputs.nika.yaml --input target=Mars --input count=5
# =============================================================================

schema: "nika/workflow@0.12"

workflow: inputs-demo

# TODO: Add an inputs block with:
#       inputs:
#         target:
#           type: string
#           default: "World"
#           description: "Who to greet"
#         count:
#           type: number
#           default: 3
#           description: "How many greetings"

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with input-driven tasks!'"

  # TODO: Create a task "greet" that uses {{inputs.target}} in a prompt

  # TODO: Create a task "repeat" that uses {{inputs.count}} in a command
"##;

const ROOT_ACCESS_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 4 — EXERCISE 3: Inputs (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: inputs-demo

inputs:
  target:
    type: string
    default: "World"
    description: "Who to greet"
  count:
    type: number
    default: 3
    description: "How many greetings"

tasks:
  - id: greet
    exec:
      command: "echo 'Hello, {{inputs.target}}!'"
      shell: true

  - id: repeat
    depends_on: [greet]
    exec:
      command: "echo 'Generating {{inputs.count}} variations for {{inputs.target}}...'"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 05: SHAPESHIFTER
// ═════════════════════════════════════════════════════════════════════════════

// ── 05-01: Structured Output ───────────────────────────────────────────────

const SHAPESHIFTER_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 5 — EXERCISE 1: Structured Output
# =============================================================================
#
# Force LLM output to match a JSON Schema. The structured: block at
# task level validates the response and rejects anything that doesn't conform.
#
# CONCEPTS:
#   - structured: block with schema: (inline JSON Schema)
#   - type, properties, required fields
#   - LLM output validated against the schema automatically
#   - provider: / model: required for infer: tasks
#
# RUN: nika run 01-structured-output.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: structured-output

# TODO: Add provider and model (use {{PROVIDER}} and {{MODEL}} placeholders)

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with structured output tasks!'"

  # TODO: Create a task "extract_info" with:
  #       - structured: block at task level with schema:
  #           schema:
  #             type: object
  #             properties:
  #               name:
  #                 type: string
  #               category:
  #                 type: string
  #                 enum: ["tech", "science", "art"]
  #               confidence:
  #                 type: number
  #             required: [name, category, confidence]
  #       - infer: with prompt asking the LLM to extract structured data

  # TODO: Create a task "use_structured" that depends on extract_info
  #       - with: { result: $extract_info | parse_json }
  #       - exec: command accessing {{with.result.name}} and {{with.result.category}}
"##;

const SHAPESHIFTER_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 5 — EXERCISE 1: Structured Output (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: structured-output

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: extract_info
    structured:
      schema:
        type: object
        properties:
          name:
            type: string
          category:
            type: string
            enum: ["tech", "science", "art"]
          confidence:
            type: number
        required: [name, category, confidence]
    infer:
      prompt: |
        Classify this topic: "Quantum computing breakthrough at MIT"
        Extract the name, category, and your confidence score (0-1).

  - id: use_structured
    depends_on: [extract_info]
    with:
      result: $extract_info | parse_json
    exec:
      command: |
        echo "Name: {{with.result.name}}"
        echo "Category: {{with.result.category}}"
        echo "Confidence: {{with.result.confidence}}"
      shell: true
"##;

// ── 05-02: Artifacts ───────────────────────────────────────────────────────

const SHAPESHIFTER_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 5 — EXERCISE 2: Artifacts
# =============================================================================
#
# Save task output to disk with the artifact: block.
# format: controls serialization (text, json, yaml, binary).
# mode: controls write behavior (overwrite, append, unique, fail).
#
# CONCEPTS:
#   - artifact: block with path: template
#   - format: text | json | yaml | binary (serialization)
#   - mode: overwrite | append | unique | fail (write behavior)
#   - Artifacts saved relative to .nika/artifacts/
#
# RUN: nika run 02-artifacts.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: artifacts-demo

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with artifact tasks!'"

  # TODO: Create a task "generate_text" with:
  #       - exec: a command that produces text output
  #       - artifact:
  #           path: output/report.txt

  # TODO: Create a task "generate_json" with:
  #       - exec: a command that produces JSON output
  #       - artifact:
  #           path: output/data.json
  #           format: json

  # TODO: Create a task "append_log" with:
  #       - depends_on: [generate_text, generate_json]
  #       - exec: a command that produces a log line
  #       - artifact:
  #           path: output/pipeline.log
  #           mode: append
"##;

const SHAPESHIFTER_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 5 — EXERCISE 2: Artifacts (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: artifacts-demo

tasks:
  - id: generate_text
    exec:
      command: "echo 'Pipeline report generated at: 2026-03-22'"
      shell: true
    artifact:
      path: output/report.txt

  - id: generate_json
    exec:
      command: |
        echo '{"status": "complete", "items_processed": 42}'
      shell: true
    artifact:
      path: output/data.json
      format: json

  - id: append_log
    depends_on: [generate_text, generate_json]
    exec:
      command: "echo 'Pipeline finished successfully'"
      shell: true
    artifact:
      path: output/pipeline.log
      mode: append
"##;

// ── 05-03: Schema Retry ────────────────────────────────────────────────────

const SHAPESHIFTER_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 5 — EXERCISE 3: Schema Retry
# =============================================================================
#
# When the LLM output fails schema validation, Nika can auto-retry
# with feedback about what went wrong via structured: max_retries.
#
# CONCEPTS:
#   - structured: { max_retries: 3 } for auto-retry on schema validation failure
#   - The LLM receives the validation error and corrects itself
#   - retry: is for fetch: tasks only (network retries)
#
# RUN: nika run 03-schema-retry.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: schema-retry

# TODO: Add provider and model (use {{PROVIDER}} and {{MODEL}} placeholders)

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with retry + structured output tasks!'"

  # TODO: Create a task "strict_extraction" with:
  #       - structured: block with schema: and max_retries: 3
  #           schema:
  #             type: object
  #             properties:
  #               items:
  #                 type: array
  #                 items:
  #                   type: object
  #                   properties:
  #                     name: { type: string }
  #                     score: { type: number }
  #                   required: [name, score]
  #                 minItems: 3
  #                 maxItems: 3
  #             required: [items]
  #       - infer: prompt asking for a list of exactly 3 items

  # TODO: Create a task "display" that reads the validated output
  #       - with: { data: $strict_extraction | parse_json }
  #       - exec: display the items
"##;

const SHAPESHIFTER_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 5 — EXERCISE 3: Schema Retry (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: schema-retry

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: strict_extraction
    structured:
      schema:
        type: object
        properties:
          items:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                score:
                  type: number
              required: [name, score]
            minItems: 3
            maxItems: 3
        required: [items]
      max_retries: 3
    infer:
      prompt: |
        List exactly 3 programming languages with a popularity score from 0 to 100.
        Return as JSON with an "items" array containing objects with "name" and "score".

  - id: display
    depends_on: [strict_extraction]
    with:
      data: $strict_extraction | parse_json
    exec:
      command: "echo 'Validated output: {{with.data}}'"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// LEVEL 06: PAY-PER-DREAM
// ═════════════════════════════════════════════════════════════════════════════

// ── 06-01: Multi-Provider ──────────────────────────────────────────────────

const PAY_PER_DREAM_01_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 6 — EXERCISE 1: Multi-Provider
# =============================================================================
#
# Use 3+ different providers in a single workflow. Each task picks
# the best provider for the job: fast draft, quality review, cheap eval.
#
# CONCEPTS:
#   - provider: / model: overrides at task level
#   - Mixing providers for different strengths
#   - Cost/speed/quality tradeoffs
#
# SETUP: Configure at least 2 providers with nika keys set <name>
# RUN:   nika run 01-multi-provider.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: multi-provider

# TODO: Set a workflow-level provider (the default)

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with multi-provider tasks!'"

  # TODO: Create a task "fast_draft" using a fast/cheap provider
  #       - provider: and model: at task level
  #       - infer: prompt for a quick first draft
  #       - temperature: 0.8 (creative)

  # TODO: Create a task "quality_review" using a premium provider
  #       - depends_on: [fast_draft]
  #       - with: { draft: $fast_draft }
  #       - Different provider: and model: from above
  #       - infer: prompt to refine the draft
  #       - temperature: 0.3 (precise)

  # TODO: Create a task "final_eval" using a third provider
  #       - depends_on: [quality_review]
  #       - with: { review: $quality_review }
  #       - Yet another provider: and model:
  #       - infer: prompt to evaluate the final output
  #       - temperature: 0.2 (analytical)
"##;

const PAY_PER_DREAM_01_SOLUTION: &str = r##"# =============================================================================
# LEVEL 6 — EXERCISE 1: Multi-Provider (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: multi-provider

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: fast_draft
    provider: openai
    model: "gpt-4o-mini"
    infer:
      prompt: "Write a 3-sentence pitch for an AI-powered recipe generator app."
      temperature: 0.8
      max_tokens: 200

  - id: quality_review
    depends_on: [fast_draft]
    with:
      draft: $fast_draft
    provider: groq
    model: "llama-3.3-70b-versatile"
    infer:
      prompt: |
        Improve this pitch for clarity and impact. Keep it to 3 sentences:

        {{with.draft}}
      temperature: 0.3
      max_tokens: 200

  - id: final_eval
    depends_on: [quality_review]
    with:
      review: $quality_review
    provider: gemini
    model: "gemini-2.0-flash"
    infer:
      prompt: |
        Rate this pitch from 1-10 and explain why in one sentence:

        {{with.review}}
      temperature: 0.2
      max_tokens: 150
"##;

// ── 06-02: Native Local ────────────────────────────────────────────────────

const PAY_PER_DREAM_02_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 6 — EXERCISE 2: Native Local Models
# =============================================================================
#
# Run LLMs locally with provider: native and GGUF models.
# No API key needed — the model runs on your machine.
#
# CONCEPTS:
#   - provider: native for local inference
#   - model: path to a .gguf file or HuggingFace model ID
#   - nika model pull to download models
#   - Trade-off: slower but free and private
#
# SETUP:
#   nika model pull TheBloke/Llama-2-7B-Chat-GGUF:Q4_K_M
# RUN: nika run 02-native-local.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: native-local

# TODO: Set provider: native at workflow level

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with native local inference tasks!'"

  # TODO: Create a task "local_infer" with:
  #       - provider: native
  #       - model: a GGUF model path or HuggingFace ID
  #       - infer:
  #           prompt: "What are 3 benefits of running LLMs locally?"
  #           temperature: 0.7
  #           max_tokens: 200

  # TODO: Create a task "compare" that runs the same prompt
  #       but with a different (smaller or larger) model
  #       to demonstrate model selection trade-offs
"##;

const PAY_PER_DREAM_02_SOLUTION: &str = r##"# =============================================================================
# LEVEL 6 — EXERCISE 2: Native Local Models (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: native-local

provider: native

tasks:
  - id: local_infer
    provider: native
    model: "TheBloke/Llama-2-7B-Chat-GGUF:Q4_K_M"
    infer:
      prompt: "What are 3 benefits of running LLMs locally? Be concise."
      temperature: 0.7
      max_tokens: 200

  - id: compare
    depends_on: [local_infer]
    provider: native
    model: "TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF:Q4_K_M"
    infer:
      prompt: "What are 3 benefits of running LLMs locally? Be concise."
      temperature: 0.7
      max_tokens: 200
"##;

// ── 06-03: System Prompts ──────────────────────────────────────────────────

const PAY_PER_DREAM_03_TEMPLATE: &str = r##"# =============================================================================
# LEVEL 6 — EXERCISE 3: System Prompts & Advanced Inference
# =============================================================================
#
# Master system prompts, temperature tuning, and extended thinking.
# System prompts shape the LLM's persona and behavior.
#
# CONCEPTS:
#   - system: for persona/instruction injection
#   - temperature: fine-tuning (0.0 deterministic to 2.0 chaotic)
#   - extended_thinking: for reasoning-heavy tasks (Claude only)
#   - Combining system + temperature for precise control
#
# RUN: nika run 03-system-prompts.nika.yaml
# =============================================================================

schema: "nika/workflow@0.12"

workflow: system-prompts

tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with system prompt tasks!'"

  # TODO: Create a task "poet" with:
  #       - infer:
  #           prompt: "Write about the sunrise"
  #           system: "You are a haiku poet. Respond only in haiku format (5-7-5)."
  #           temperature: 0.9

  # TODO: Create a task "analyst" with:
  #       - infer:
  #           prompt: "Evaluate the market for AI workflow tools"
  #           system: "You are a senior market analyst. Use bullet points. Be data-driven."
  #           temperature: 0.2
  #           max_tokens: 300

  # TODO: Create a task "creative_vs_precise" that depends on both
  #       - with: { poem: $poet, analysis: $analyst }
  #       - infer:
  #           prompt: combining both outputs and asking for a comparison
  #           system: "You are a writing coach comparing creative vs analytical styles."
  #           temperature: 0.5
"##;

const PAY_PER_DREAM_03_SOLUTION: &str = r##"# =============================================================================
# LEVEL 6 — EXERCISE 3: System Prompts & Advanced Inference (Solution)
# =============================================================================

schema: "nika/workflow@0.12"

workflow: system-prompts

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: poet
    infer:
      prompt: "Write about the sunrise"
      system: "You are a haiku poet. Respond only in haiku format (5-7-5 syllables)."
      temperature: 0.9
      max_tokens: 50

  - id: analyst
    infer:
      prompt: "Evaluate the market for AI workflow tools"
      system: "You are a senior market analyst. Use bullet points. Be data-driven and concise."
      temperature: 0.2
      max_tokens: 300

  - id: creative_vs_precise
    depends_on: [poet, analyst]
    with:
      poem: $poet
      analysis: $analyst
    infer:
      prompt: |
        Compare these two outputs — one creative, one analytical:

        CREATIVE (Haiku):
        {{with.poem}}

        ANALYTICAL (Market Report):
        {{with.analysis}}

        In 2 sentences, explain how tone and temperature shape LLM output.
      system: "You are a writing coach who understands both creative and technical writing."
      temperature: 0.5
      max_tokens: 200
"##;

// ═════════════════════════════════════════════════════════════════════════════
// TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exercise_count() {
        assert_eq!(
            EXERCISES.len(),
            22,
            "Must have exactly 22 exercises for Levels 1-6"
        );
    }

    #[test]
    fn test_exercise_count_per_level() {
        let counts: Vec<(&str, usize)> = vec![
            ("jailbreak", 5),
            ("hot-wire", 4),
            ("fork-bomb", 4),
            ("root-access", 3),
            ("shapeshifter", 3),
            ("pay-per-dream", 3),
        ];
        for (slug, expected) in counts {
            let actual = get_exercises(slug).len();
            assert_eq!(
                actual, expected,
                "Level '{}' should have {} exercises, got {}",
                slug, expected, actual
            );
        }
    }

    #[test]
    fn test_exercise_numbers_sequential() {
        let slugs = [
            "jailbreak",
            "hot-wire",
            "fork-bomb",
            "root-access",
            "shapeshifter",
            "pay-per-dream",
        ];
        for slug in slugs {
            let exercises = get_exercises(slug);
            for (i, ex) in exercises.iter().enumerate() {
                assert_eq!(
                    ex.exercise_num,
                    (i + 1) as u8,
                    "Exercise {} in '{}' should have num {}",
                    i,
                    slug,
                    i + 1
                );
            }
        }
    }

    #[test]
    fn test_all_templates_have_todos() {
        for ex in EXERCISES {
            assert!(
                ex.template.contains("TODO"),
                "Template for {}/{} ({}) must contain TODO markers",
                ex.level_slug,
                ex.exercise_num,
                ex.filename
            );
        }
    }

    #[test]
    fn test_no_solutions_have_todos() {
        for ex in EXERCISES {
            assert!(
                !ex.solution.contains("TODO"),
                "Solution for {}/{} ({}) must not contain TODO markers",
                ex.level_slug,
                ex.exercise_num,
                ex.filename
            );
        }
    }

    #[test]
    fn test_all_have_schema_declaration() {
        for ex in EXERCISES {
            assert!(
                ex.template.contains("nika/workflow@0.12"),
                "Template for {}/{} must reference schema nika/workflow@0.12",
                ex.level_slug,
                ex.exercise_num
            );
            assert!(
                ex.solution.contains("nika/workflow@0.12"),
                "Solution for {}/{} must reference schema nika/workflow@0.12",
                ex.level_slug,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_all_solutions_have_schema_line() {
        for ex in EXERCISES {
            assert!(
                ex.solution.contains("schema: \"nika/workflow@0.12\""),
                "Solution for {}/{} must have schema: declaration",
                ex.level_slug,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_all_solutions_have_workflow_name() {
        for ex in EXERCISES {
            assert!(
                ex.solution.contains("workflow:"),
                "Solution for {}/{} must have workflow: name",
                ex.level_slug,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_all_solutions_have_tasks() {
        for ex in EXERCISES {
            assert!(
                ex.solution.contains("tasks:"),
                "Solution for {}/{} must have tasks: block",
                ex.level_slug,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_filenames_follow_convention() {
        for ex in EXERCISES {
            assert!(
                ex.filename.ends_with(".nika.yaml"),
                "Filename '{}' must end with .nika.yaml",
                ex.filename
            );
            assert!(
                ex.filename.starts_with(&format!("{:02}-", ex.exercise_num)),
                "Filename '{}' must start with {:02}-",
                ex.filename,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_get_exercises_returns_correct_order() {
        let jailbreak = get_exercises("jailbreak");
        assert_eq!(jailbreak.len(), 5);
        assert_eq!(jailbreak[0].exercise_num, 1);
        assert_eq!(jailbreak[4].exercise_num, 5);
    }

    #[test]
    fn test_get_exercises_empty_for_unknown() {
        let unknown = get_exercises("nonexistent");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_get_exercise_found() {
        let ex = get_exercise("jailbreak", 1);
        assert!(ex.is_some());
        let ex = ex.unwrap();
        assert_eq!(ex.level_slug, "jailbreak");
        assert_eq!(ex.exercise_num, 1);
        assert_eq!(ex.filename, "01-hello-world.nika.yaml");
    }

    #[test]
    fn test_get_exercise_not_found() {
        assert!(get_exercise("jailbreak", 99).is_none());
        assert!(get_exercise("nonexistent", 1).is_none());
    }

    #[test]
    fn test_templates_not_empty() {
        for ex in EXERCISES {
            assert!(
                !ex.template.trim().is_empty(),
                "Template for {}/{} must not be empty",
                ex.level_slug,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_solutions_not_empty() {
        for ex in EXERCISES {
            assert!(
                !ex.solution.trim().is_empty(),
                "Solution for {}/{} must not be empty",
                ex.level_slug,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_yaml_basic_validity() {
        // Verify that solutions parse as valid YAML
        for ex in EXERCISES {
            // Strip comment-only lines and check for basic YAML structure
            let yaml_lines: Vec<&str> = ex
                .solution
                .lines()
                .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
                .collect();
            assert!(
                !yaml_lines.is_empty(),
                "Solution for {}/{} must have non-comment YAML content",
                ex.level_slug,
                ex.exercise_num
            );
        }
    }

    #[test]
    fn test_level_slugs_match_known_levels() {
        let valid_slugs = [
            "jailbreak",
            "hot-wire",
            "fork-bomb",
            "root-access",
            "shapeshifter",
            "pay-per-dream",
        ];
        for ex in EXERCISES {
            assert!(
                valid_slugs.contains(&ex.level_slug),
                "Exercise {}/{} has invalid level_slug '{}'",
                ex.level_slug,
                ex.exercise_num,
                ex.level_slug
            );
        }
    }

    #[test]
    fn test_no_duplicate_exercises() {
        for (i, a) in EXERCISES.iter().enumerate() {
            for b in EXERCISES.iter().skip(i + 1) {
                assert!(
                    !(a.level_slug == b.level_slug && a.exercise_num == b.exercise_num),
                    "Duplicate exercise: {}/{}",
                    a.level_slug,
                    a.exercise_num
                );
            }
        }
    }
}
