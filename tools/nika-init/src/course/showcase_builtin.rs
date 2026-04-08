// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Showcase workflows — 15 real-world workflows using builtin tools (invoke: nika:*)
//!
//! Demonstrates all 3 categories of builtin tools:
//! - **Core tools**: nika:log, nika:emit, nika:assert, nika:sleep
//! - **File tools**: nika:read, nika:write, nika:edit, nika:glob, nika:grep
//! - **Media tools**: nika:import, nika:dimensions, nika:thumbhash, nika:dominant_color,
//!   nika:thumbnail, nika:optimize, nika:pipeline, nika:phash, nika:compare, nika:svg_render
//!
//! Most workflows require NO LLM provider. Media workflows require local image files
//! or use `fetch: response: binary` to download test images.

use super::showcase::ShowcaseWorkflow;

/// All 15 builtin-tool showcase workflows
pub static SHOWCASE_BUILTIN: &[ShowcaseWorkflow] = &[
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: CORE TOOLS (1-3)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "progress-tracker",
        description: "Track multi-step progress with nika:log, nika:emit, and nika:assert",
        category: "core",
        content: PROGRESS_TRACKER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "timed-benchmark",
        description: "Benchmark steps with nika:sleep pauses and nika:log timing reports",
        category: "core",
        content: TIMED_BENCHMARK,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "validation-pipeline",
        description: "Multi-check validation with nika:assert gates and nika:emit verdicts",
        category: "core",
        content: VALIDATION_PIPELINE,
        requires_llm: false,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: FILE TOOLS (4-9)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "project-statistics",
        description: "Scan Rust files with nika:glob, grep TODOs, and log the count",
        category: "file",
        content: PROJECT_STATISTICS,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "config-generator",
        description: "Write a config with nika:write, update with nika:edit, verify with nika:read",
        category: "file",
        content: CONFIG_GENERATOR,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "log-analyzer",
        description: "Read a log file, grep for errors, and write a filtered report",
        category: "file",
        content: LOG_ANALYZER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "template-engine",
        description: "Read a template, substitute values via exec, write the rendered output",
        category: "file",
        content: TEMPLATE_ENGINE,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "codebase-search",
        description: "Glob TypeScript files, grep for a pattern, write matches to a report",
        category: "file",
        content: CODEBASE_SEARCH,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "file-backup",
        description: "Glob important files, read each, and write copies to a backup directory",
        category: "file",
        content: FILE_BACKUP,
        requires_llm: false,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: MEDIA TOOLS (10-15)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "image-info-extractor",
        description: "Import an image, extract dimensions, thumbhash, and dominant colors",
        category: "media",
        content: IMAGE_INFO_EXTRACTOR,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "thumbnail-generator",
        description: "Import an image, generate a 256px thumbnail, optimize it as artifact",
        category: "media",
        content: THUMBNAIL_GENERATOR,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "image-comparison",
        description: "Import two images, compute perceptual hashes, and compare similarity",
        category: "media",
        content: IMAGE_COMPARISON,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "pipeline-chain",
        description: "Chain thumbnail + convert + optimize in a single in-memory pipeline",
        category: "media",
        content: PIPELINE_CHAIN,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "svg-to-png",
        description: "Import an SVG, rasterize to PNG with nika:svg_render, optimize output",
        category: "media",
        content: SVG_TO_PNG,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "media-manifest",
        description: "Import multiple images, collect dimensions into a structured manifest",
        category: "media",
        content: MEDIA_MANIFEST,
        requires_llm: false,
    },
];

// ═════════════════════════════════════════════════════════════════════════════
// 01: PROGRESS TRACKER
// ═════════════════════════════════════════════════════════════════════════════

const PROGRESS_TRACKER: &str = r##"# Progress Tracker — multi-step progress with log, emit, and assert
#
# Demonstrates: nika:log, nika:emit, nika:assert
# No LLM required.
schema: "nika/workflow@0.12"

workflow: progress-tracker

tasks:
  - id: log_start
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Progress tracker starting — 4 phases to complete"

  - id: phase_1
    depends_on: [log_start]
    invoke:
      tool: "nika:emit"
      params:
        name: "progress"
        payload:
          phase: 1
          label: "initialization"
          percent: 25

  - id: log_phase_1
    depends_on: [phase_1]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Phase 1 complete — initialization done (25%)"

  - id: phase_2
    depends_on: [log_phase_1]
    invoke:
      tool: "nika:emit"
      params:
        name: "progress"
        payload:
          phase: 2
          label: "processing"
          percent: 50

  - id: log_phase_2
    depends_on: [phase_2]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Phase 2 complete — processing done (50%)"

  - id: phase_3
    depends_on: [log_phase_2]
    invoke:
      tool: "nika:emit"
      params:
        name: "progress"
        payload:
          phase: 3
          label: "validation"
          percent: 75

  - id: log_phase_3
    depends_on: [phase_3]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Phase 3 complete — validation done (75%)"

  - id: phase_4
    depends_on: [log_phase_3]
    invoke:
      tool: "nika:emit"
      params:
        name: "progress"
        payload:
          phase: 4
          label: "finalization"
          percent: 100

  - id: assert_done
    depends_on: [phase_4]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "All 4 phases completed successfully"

  - id: log_done
    depends_on: [assert_done]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Progress tracker finished — 100% complete"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 02: TIMED BENCHMARK
// ═════════════════════════════════════════════════════════════════════════════

const TIMED_BENCHMARK: &str = r##"# Timed Benchmark — measure step durations with sleep and log
#
# Demonstrates: nika:sleep, nika:log, nika:emit, nika:assert
# No LLM required.
schema: "nika/workflow@0.12"

workflow: timed-benchmark

tasks:
  - id: log_start
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Benchmark starting — timing 3 operations"

  - id: step_1_pause
    depends_on: [log_start]
    invoke:
      tool: "nika:sleep"
      params:
        duration: "500ms"

  - id: step_1_report
    depends_on: [step_1_pause]
    with:
      t1: $step_1_pause
    invoke:
      tool: "nika:emit"
      params:
        name: "benchmark_step"
        payload:
          step: 1
          label: "fast operation"
          slept_ms: "{{with.t1}}"

  - id: step_2_pause
    depends_on: [step_1_report]
    invoke:
      tool: "nika:sleep"
      params:
        duration: "1s"

  - id: step_2_report
    depends_on: [step_2_pause]
    with:
      t2: $step_2_pause
    invoke:
      tool: "nika:emit"
      params:
        name: "benchmark_step"
        payload:
          step: 2
          label: "medium operation"
          slept_ms: "{{with.t2}}"

  - id: step_3_pause
    depends_on: [step_2_report]
    invoke:
      tool: "nika:sleep"
      params:
        duration: "200ms"

  - id: step_3_report
    depends_on: [step_3_pause]
    with:
      t3: $step_3_pause
    invoke:
      tool: "nika:emit"
      params:
        name: "benchmark_step"
        payload:
          step: 3
          label: "quick operation"
          slept_ms: "{{with.t3}}"

  - id: log_results
    depends_on: [step_3_report]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Benchmark complete — 3 steps timed: 500ms + 1s + 200ms"

  - id: assert_complete
    depends_on: [log_results]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "Benchmark finished within expected time budget"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 03: VALIDATION PIPELINE
// ═════════════════════════════════════════════════════════════════════════════

const VALIDATION_PIPELINE: &str = r##"# Validation Pipeline — multi-check validation with assert gates
#
# Demonstrates: nika:assert, nika:emit, nika:log
# No LLM required. Each assert acts as a gate — failure stops the pipeline.
schema: "nika/workflow@0.12"

workflow: validation-pipeline

tasks:
  - id: log_start
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Validation pipeline starting — 4 checks to run"

  - id: check_schema
    depends_on: [log_start]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "Schema version must be 0.12"

  - id: emit_schema_ok
    depends_on: [check_schema]
    invoke:
      tool: "nika:emit"
      params:
        name: "validation_result"
        payload:
          check: "schema_version"
          passed: true

  - id: check_tasks
    depends_on: [emit_schema_ok]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "Workflow must have at least one task"

  - id: emit_tasks_ok
    depends_on: [check_tasks]
    invoke:
      tool: "nika:emit"
      params:
        name: "validation_result"
        payload:
          check: "tasks_exist"
          passed: true

  - id: check_ids
    depends_on: [emit_tasks_ok]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "All task IDs must be unique"

  - id: emit_ids_ok
    depends_on: [check_ids]
    invoke:
      tool: "nika:emit"
      params:
        name: "validation_result"
        payload:
          check: "unique_ids"
          passed: true

  - id: check_dag
    depends_on: [emit_ids_ok]
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "DAG must have no cycles"

  - id: emit_dag_ok
    depends_on: [check_dag]
    invoke:
      tool: "nika:emit"
      params:
        name: "validation_result"
        payload:
          check: "dag_acyclic"
          passed: true

  - id: log_summary
    depends_on: [emit_dag_ok]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Validation complete — all 4 checks passed"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 04: PROJECT STATISTICS
// ═════════════════════════════════════════════════════════════════════════════

const PROJECT_STATISTICS: &str = r##"# Project Statistics — scan Rust files, grep TODOs, log the count
#
# Demonstrates: nika:glob, nika:grep, nika:log
# No LLM required. Runs on the current working directory.
schema: "nika/workflow@0.12"

workflow: project-statistics

tasks:
  - id: find_rust_files
    invoke:
      tool: "nika:glob"
      params:
        pattern: "**/*.rs"

  - id: log_file_count
    depends_on: [find_rust_files]
    with:
      files: $find_rust_files
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Found Rust files: {{with.files}}"

  - id: grep_todos
    depends_on: [find_rust_files]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "TODO|FIXME|HACK"
        glob: "*.rs"
        output_mode: "count"

  - id: log_todo_count
    depends_on: [grep_todos]
    with:
      todos: $grep_todos
    invoke:
      tool: "nika:log"
      params:
        level: "warn"
        message: "TODO/FIXME/HACK count: {{with.todos}}"

  - id: grep_unsafe
    depends_on: [find_rust_files]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "unsafe "
        glob: "*.rs"
        output_mode: "count"

  - id: log_unsafe_count
    depends_on: [grep_unsafe]
    with:
      unsafe_count: $grep_unsafe
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Unsafe blocks found: {{with.unsafe_count}}"

  - id: emit_stats
    depends_on: [log_file_count, log_todo_count, log_unsafe_count]
    invoke:
      tool: "nika:emit"
      params:
        name: "project_stats"
        payload:
          scan_type: "rust"
          checks: ["file_count", "todos", "unsafe_blocks"]
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 05: CONFIG GENERATOR
// ═════════════════════════════════════════════════════════════════════════════

const CONFIG_GENERATOR: &str = r##"# Config Generator — write, edit, and verify a config file
#
# Demonstrates: nika:write, nika:edit, nika:read, nika:log
# No LLM required. Uses .scratch/ for output.
schema: "nika/workflow@0.12"

workflow: config-generator

tasks:
  - id: cleanup
    exec:
      command: "rm -f .scratch/app-config.toml"
      shell: true

  - id: write_config
    depends_on: [cleanup]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/app-config.toml"
        content: |
          [server]
          host = "127.0.0.1"
          port = 3000
          debug = true

          [database]
          url = "postgres://localhost:5432/dev"
          pool_size = 5

          [logging]
          level = "debug"
          format = "json"

  - id: log_written
    depends_on: [write_config]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Config file created at .scratch/app-config.toml"

  - id: edit_port
    depends_on: [log_written]
    invoke:
      tool: "nika:edit"
      params:
        file_path: ".scratch/app-config.toml"
        old_string: "port = 3000"
        new_string: "port = 8080"

  - id: edit_debug
    depends_on: [edit_port]
    invoke:
      tool: "nika:edit"
      params:
        file_path: ".scratch/app-config.toml"
        old_string: 'debug = true'
        new_string: 'debug = false'

  - id: edit_log_level
    depends_on: [edit_debug]
    invoke:
      tool: "nika:edit"
      params:
        file_path: ".scratch/app-config.toml"
        old_string: 'level = "debug"'
        new_string: 'level = "info"'

  - id: verify_config
    depends_on: [edit_log_level]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/app-config.toml"

  - id: log_verified
    depends_on: [verify_config]
    with:
      config: $verify_config
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Final config verified:\n{{with.config}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 06: LOG ANALYZER
// ═════════════════════════════════════════════════════════════════════════════

const LOG_ANALYZER: &str = r##"# Log Analyzer — read a log, grep errors, write a filtered report
#
# Demonstrates: nika:write, nika:read, nika:grep, nika:write (report), nika:log
# No LLM required. Creates a sample log then analyzes it.
schema: "nika/workflow@0.12"

workflow: log-analyzer

tasks:
  - id: cleanup
    exec:
      command: "rm -f .scratch/sample.log .scratch/error-report.txt"
      shell: true

  - id: create_sample_log
    depends_on: [cleanup]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/sample.log"
        content: |
          2026-03-22T10:00:01Z INFO  server: Listening on 0.0.0.0:8080
          2026-03-22T10:00:05Z INFO  server: Connection from 192.168.1.100
          2026-03-22T10:00:06Z ERROR database: Connection pool exhausted
          2026-03-22T10:00:07Z WARN  server: Slow query detected (2.3s)
          2026-03-22T10:00:08Z INFO  server: Request completed 200 OK
          2026-03-22T10:00:09Z ERROR auth: Invalid token for user_42
          2026-03-22T10:00:10Z INFO  server: Health check passed
          2026-03-22T10:00:11Z ERROR database: Deadlock detected on table users
          2026-03-22T10:00:12Z WARN  cache: Redis reconnecting (attempt 3)
          2026-03-22T10:00:13Z INFO  server: Request completed 200 OK

  - id: read_log
    depends_on: [create_sample_log]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/sample.log"

  - id: grep_errors
    depends_on: [create_sample_log]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "ERROR"
        path: ".scratch"
        glob: "*.log"
        output_mode: "content"

  - id: grep_warnings
    depends_on: [create_sample_log]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "WARN"
        path: ".scratch"
        glob: "*.log"
        output_mode: "content"

  - id: write_report
    depends_on: [grep_errors, grep_warnings]
    with:
      errors: $grep_errors
      warnings: $grep_warnings
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/error-report.txt"
        content: |
          === Error Report ===
          Generated: 2026-03-22

          --- ERRORS ---
          {{with.errors}}

          --- WARNINGS ---
          {{with.warnings}}

  - id: log_done
    depends_on: [write_report]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Log analysis complete — report written to .scratch/error-report.txt"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 07: TEMPLATE ENGINE
// ═════════════════════════════════════════════════════════════════════════════

const TEMPLATE_ENGINE: &str = r##"# Template Engine — read template, substitute values, write output
#
# Demonstrates: nika:write, nika:read, exec: for substitution, nika:write output
# No LLM required.
schema: "nika/workflow@0.12"

workflow: template-engine

tasks:
  - id: cleanup
    exec:
      command: "rm -f .scratch/template.html .scratch/rendered.html"
      shell: true

  - id: write_template
    depends_on: [cleanup]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/template.html"
        content: |
          <!DOCTYPE html>
          <html>
          <head><title>__TITLE__</title></head>
          <body>
            <h1>__HEADING__</h1>
            <p>Welcome to __PROJECT__.</p>
            <p>Version: __VERSION__</p>
            <footer>Built with __TOOL__ on __DATE__</footer>
          </body>
          </html>

  - id: read_template
    depends_on: [write_template]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/template.html"

  - id: substitute
    depends_on: [read_template]
    with:
      tpl: $read_template
    exec:
      command: |
        echo '{{with.tpl}}' | \
          sed 's/__TITLE__/Nika Showcase/g' | \
          sed 's/__HEADING__/Builtin Tool Showcase/g' | \
          sed 's/__PROJECT__/SuperNovae/g' | \
          sed 's/__VERSION__/0.40.2/g' | \
          sed 's/__TOOL__/Nika/g' | \
          sed 's/__DATE__/2026-03-22/g'
      shell: true

  - id: write_output
    depends_on: [substitute]
    with:
      rendered: $substitute
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/rendered.html"
        content: "{{with.rendered}}"

  - id: log_done
    depends_on: [write_output]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Template rendered to .scratch/rendered.html"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 08: CODEBASE SEARCH
// ═════════════════════════════════════════════════════════════════════════════

const CODEBASE_SEARCH: &str = r##"# Codebase Search — glob files, grep for patterns, write matches
#
# Demonstrates: nika:glob, nika:grep, nika:write, nika:log
# No LLM required. Searches for common patterns in the working directory.
schema: "nika/workflow@0.12"

workflow: codebase-search

tasks:
  - id: cleanup
    exec:
      command: "rm -f .scratch/search-report.md"
      shell: true

  - id: find_all_files
    invoke:
      tool: "nika:glob"
      params:
        pattern: "**/*.rs"

  - id: grep_unwrap
    depends_on: [find_all_files]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "\\.unwrap\\(\\)"
        glob: "*.rs"
        output_mode: "files_with_matches"

  - id: grep_clone
    depends_on: [find_all_files]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "\\.clone\\(\\)"
        glob: "*.rs"
        output_mode: "count"

  - id: grep_todo
    depends_on: [find_all_files]
    invoke:
      tool: "nika:grep"
      params:
        pattern: "TODO|FIXME|XXX"
        glob: "*.rs"
        output_mode: "content"

  - id: write_report
    depends_on: [grep_unwrap, grep_clone, grep_todo, cleanup]
    with:
      unwrap_files: $grep_unwrap
      clone_count: $grep_clone
      todo_matches: $grep_todo
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/search-report.md"
        content: |
          # Codebase Search Report

          ## Files with .unwrap()
          {{with.unwrap_files}}

          ## .clone() usage count
          {{with.clone_count}}

          ## TODO/FIXME/XXX
          {{with.todo_matches}}

  - id: log_done
    depends_on: [write_report]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Codebase search complete — report at .scratch/search-report.md"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 09: FILE BACKUP
// ═════════════════════════════════════════════════════════════════════════════

const FILE_BACKUP: &str = r##"# File Backup — glob files, read each, write copies to backup
#
# Demonstrates: nika:write (setup), nika:glob, nika:read, nika:write (backup), nika:log
# No LLM required. Creates sample files, then backs them up.
schema: "nika/workflow@0.12"

workflow: file-backup

tasks:
  - id: cleanup
    exec:
      command: "rm -rf .scratch/source .scratch/backup"
      shell: true

  - id: setup_dir
    depends_on: [cleanup]
    exec:
      command: "mkdir -p .scratch/source .scratch/backup"
      shell: true

  - id: create_file_1
    depends_on: [setup_dir]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/source/notes.txt"
        content: |
          Meeting notes from 2026-03-22:
          - Reviewed media pipeline architecture
          - Decided on CAS-first approach
          - Next: implement pipeline chaining

  - id: create_file_2
    depends_on: [setup_dir]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/source/config.yaml"
        content: |
          project: nika
          version: 0.40.2
          features:
            - media-core
            - media-phash
            - fetch-html

  - id: create_file_3
    depends_on: [setup_dir]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/source/todo.md"
        content: |
          # TODO List
          - [x] Implement CAS store
          - [x] Add media tools
          - [ ] Write showcase workflows
          - [ ] Publish to crates.io

  - id: read_file_1
    depends_on: [create_file_1]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/source/notes.txt"

  - id: read_file_2
    depends_on: [create_file_2]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/source/config.yaml"

  - id: read_file_3
    depends_on: [create_file_3]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/source/todo.md"

  - id: backup_file_1
    depends_on: [read_file_1]
    with:
      content: $read_file_1
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/backup/notes.txt"
        content: "{{with.content}}"

  - id: backup_file_2
    depends_on: [read_file_2]
    with:
      content: $read_file_2
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/backup/config.yaml"
        content: "{{with.content}}"

  - id: backup_file_3
    depends_on: [read_file_3]
    with:
      content: $read_file_3
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/backup/todo.md"
        content: "{{with.content}}"

  - id: verify_backup
    depends_on: [backup_file_1, backup_file_2, backup_file_3]
    invoke:
      tool: "nika:glob"
      params:
        pattern: ".scratch/backup/*"

  - id: log_done
    depends_on: [verify_backup]
    with:
      backed_up: $verify_backup
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Backup complete — files: {{with.backed_up}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 10: IMAGE INFO EXTRACTOR
// ═════════════════════════════════════════════════════════════════════════════

const IMAGE_INFO_EXTRACTOR: &str = r##"# Image Info Extractor — import, dimensions, thumbhash, dominant color
#
# Demonstrates: fetch + nika:dimensions, nika:thumbhash, nika:dominant_color
# No LLM required. Downloads a test image via fetch: response: binary.
schema: "nika/workflow@0.12"

workflow: image-info-extractor

tasks:
  - id: download
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
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

  - id: log_summary
    depends_on: [get_dims, get_thumbhash, get_colors]
    with:
      dims: $get_dims
      thumb: $get_thumbhash
      colors: $get_colors
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Image analysis complete — dims: {{with.dims}}, thumbhash: {{with.thumb}}, colors: {{with.colors}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 11: THUMBNAIL GENERATOR
// ═════════════════════════════════════════════════════════════════════════════

const THUMBNAIL_GENERATOR: &str = r##"# Thumbnail Generator — import, resize to 256px, optimize
#
# Demonstrates: fetch binary, nika:thumbnail, nika:optimize
# No LLM required. Requires media-core + media-optimize features.
schema: "nika/workflow@0.12"

workflow: thumbnail-generator

tasks:
  - id: download
    retry:
      max_attempts: 2
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  - id: make_thumbnail
    depends_on: [download]
    with:
      photo: $download
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.photo.hash}}"
        width: 256
        format: "png"

  - id: optimize_thumb
    depends_on: [make_thumbnail]
    with:
      thumb: $make_thumbnail
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.thumb.hash}}"
        level: 3
        strip: true
    artifact:
      path: output/thumbnail-256-optimized.png
      format: binary

  - id: log_done
    depends_on: [optimize_thumb]
    with:
      result: $optimize_thumb
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Thumbnail generated and optimized: {{with.result}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 12: IMAGE COMPARISON
// ═════════════════════════════════════════════════════════════════════════════

const IMAGE_COMPARISON: &str = r##"# Image Comparison — import two images, perceptual hash, compare
#
# Demonstrates: fetch binary, nika:phash, nika:compare, nika:log
# No LLM required. Requires media-phash feature.
schema: "nika/workflow@0.12"

workflow: image-comparison

tasks:
  - id: download_a
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  - id: download_b
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://httpbin.org/image/jpeg"
      response: binary
      timeout: 15

  - id: phash_a
    depends_on: [download_a]
    with:
      img: $download_a
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.hash}}"

  - id: phash_b
    depends_on: [download_b]
    with:
      img: $download_b
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.hash}}"

  - id: compare
    depends_on: [download_a, download_b]
    with:
      img_a: $download_a
      img_b: $download_b
    invoke:
      tool: "nika:compare"
      params:
        hash_a: "{{with.img_a.hash}}"
        hash_b: "{{with.img_b.hash}}"
    artifact:
      path: output/comparison.json
      format: json

  - id: log_result
    depends_on: [phash_a, phash_b, compare]
    with:
      ha: $phash_a
      hb: $phash_b
      cmp: $compare
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Comparison: hash_a={{with.ha}}, hash_b={{with.hb}}, distance={{with.cmp}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 13: PIPELINE CHAIN
// ═════════════════════════════════════════════════════════════════════════════

const PIPELINE_CHAIN: &str = r##"# Pipeline Chain — thumbnail + convert + optimize in one call
#
# Demonstrates: nika:pipeline with chained steps (1 read, N transforms, 1 write)
# No LLM required. Requires media-core + media-optimize features.
schema: "nika/workflow@0.12"

workflow: pipeline-chain

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
          - op: convert
            format: webp
            quality: 85
          - op: strip
    artifact:
      path: output/pipeline-result.webp
      format: binary

  - id: get_dims
    depends_on: [process]
    with:
      result: $process
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.result.hash}}"

  - id: log_result
    depends_on: [get_dims]
    with:
      result: $process
      dims: $get_dims
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Pipeline output: {{with.dims}} — hash: {{with.result}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 14: SVG TO PNG
// ═════════════════════════════════════════════════════════════════════════════

const SVG_TO_PNG: &str = r##"# SVG to PNG — import SVG, rasterize, optimize
#
# Demonstrates: nika:write (SVG), nika:import, nika:svg_render, nika:optimize
# No LLM required. Requires media-svg + media-optimize features.
schema: "nika/workflow@0.12"

workflow: svg-to-png

tasks:
  - id: cleanup
    exec:
      command: "rm -f .scratch/icon.svg"
      shell: true

  - id: create_svg
    depends_on: [cleanup]
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/icon.svg"
        content: |
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <circle cx="50" cy="50" r="45" fill="#4A90D9" stroke="#2C5F8A" stroke-width="3"/>
            <text x="50" y="55" text-anchor="middle" fill="white" font-size="24" font-family="sans-serif">N</text>
          </svg>

  - id: import_svg
    depends_on: [create_svg]
    invoke:
      tool: "nika:import"
      params:
        path: ".scratch/icon.svg"

  - id: render_png
    depends_on: [import_svg]
    with:
      svg: $import_svg
    invoke:
      tool: "nika:svg_render"
      params:
        hash: "{{with.svg.hash}}"
        width: 512

  - id: optimize_png
    depends_on: [render_png]
    with:
      rendered: $render_png
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.rendered.hash}}"
        level: 2
        strip: true
    artifact:
      path: output/icon-512.png
      format: binary

  - id: get_dims
    depends_on: [optimize_png]
    with:
      final_img: $optimize_png
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.final_img.hash}}"

  - id: log_done
    depends_on: [get_dims]
    with:
      dims: $get_dims
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "SVG rendered to PNG — dimensions: {{with.dims}}"
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 15: MEDIA MANIFEST
// ═════════════════════════════════════════════════════════════════════════════

const MEDIA_MANIFEST: &str = r##"# Media Manifest — import multiple images, collect dimensions into manifest
#
# Demonstrates: fetch binary, nika:dimensions, nika:thumbhash, nika:log, nika:write
# No LLM required. Downloads 3 test images and builds a manifest.
schema: "nika/workflow@0.12"

workflow: media-manifest

tasks:
  - id: cleanup
    exec:
      command: "rm -f .scratch/media-manifest.json"
      shell: true

  - id: download_png
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  - id: download_jpeg
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://httpbin.org/image/jpeg"
      response: binary
      timeout: 15

  - id: download_webp
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://httpbin.org/image/webp"
      response: binary
      timeout: 15

  - id: dims_png
    depends_on: [download_png]
    with:
      img: $download_png
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"

  - id: dims_jpeg
    depends_on: [download_jpeg]
    with:
      img: $download_jpeg
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"

  - id: dims_webp
    depends_on: [download_webp]
    with:
      img: $download_webp
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"

  - id: thumb_png
    depends_on: [download_png]
    with:
      img: $download_png
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.hash}}"

  - id: thumb_jpeg
    depends_on: [download_jpeg]
    with:
      img: $download_jpeg
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.hash}}"

  - id: thumb_webp
    depends_on: [download_webp]
    with:
      img: $download_webp
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.hash}}"

  - id: write_manifest
    depends_on: [dims_png, dims_jpeg, dims_webp, thumb_png, thumb_jpeg, thumb_webp, cleanup]
    with:
      d_png: $dims_png
      d_jpeg: $dims_jpeg
      d_webp: $dims_webp
      t_png: $thumb_png
      t_jpeg: $thumb_jpeg
      t_webp: $thumb_webp
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/media-manifest.json"
        content: |
          {
            "generated": "2026-03-22",
            "images": [
              {
                "format": "png",
                "dimensions": "{{with.d_png}}",
                "thumbhash": "{{with.t_png}}"
              },
              {
                "format": "jpeg",
                "dimensions": "{{with.d_jpeg}}",
                "thumbhash": "{{with.t_jpeg}}"
              },
              {
                "format": "webp",
                "dimensions": "{{with.d_webp}}",
                "thumbhash": "{{with.t_webp}}"
              }
            ]
          }
    artifact:
      path: output/media-manifest.json
      format: json

  - id: log_done
    depends_on: [write_manifest]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Media manifest written with 3 images — PNG, JPEG, WebP"
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_showcase_builtin_count() {
        assert_eq!(
            SHOWCASE_BUILTIN.len(),
            15,
            "Should have exactly 15 builtin-tool showcase workflows"
        );
    }

    #[test]
    fn test_showcase_builtin_names_unique() {
        let mut names: Vec<&str> = SHOWCASE_BUILTIN.iter().map(|w| w.name).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "All showcase names must be unique");
    }

    #[test]
    fn test_showcase_builtin_all_have_schema() {
        for w in SHOWCASE_BUILTIN {
            assert!(
                w.content.contains("schema: \"nika/workflow@0.12\""),
                "Workflow '{}' must declare schema",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_builtin_all_have_workflow_name() {
        for w in SHOWCASE_BUILTIN {
            assert!(
                w.content.contains("workflow:"),
                "Workflow '{}' must have workflow: declaration",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_builtin_all_have_tasks() {
        for w in SHOWCASE_BUILTIN {
            assert!(
                w.content.contains("tasks:"),
                "Workflow '{}' must have tasks: section",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_builtin_all_use_invoke() {
        for w in SHOWCASE_BUILTIN {
            assert!(
                w.content.contains("invoke:"),
                "Workflow '{}' must use invoke: for builtin tools",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_builtin_all_use_nika_tools() {
        for w in SHOWCASE_BUILTIN {
            assert!(
                w.content.contains("\"nika:"),
                "Workflow '{}' must call at least one nika:* tool",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_builtin_no_llm_required() {
        for w in SHOWCASE_BUILTIN {
            assert!(
                !w.requires_llm,
                "Workflow '{}' should not require an LLM",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_builtin_valid_yaml() {
        for w in SHOWCASE_BUILTIN {
            // Skip templates with {{PROVIDER}} / {{MODEL}} placeholders
            if w.content.contains("{{PROVIDER}}") || w.content.contains("{{MODEL}}") {
                continue;
            }
            let parsed: Result<serde_json::Value, _> = serde_saphyr::from_str(w.content);
            assert!(
                parsed.is_ok(),
                "Workflow '{}' must be valid YAML: {:?}",
                w.name,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_showcase_builtin_categories() {
        let core_count = SHOWCASE_BUILTIN
            .iter()
            .filter(|w| w.category == "core")
            .count();
        let file_count = SHOWCASE_BUILTIN
            .iter()
            .filter(|w| w.category == "file")
            .count();
        let media_count = SHOWCASE_BUILTIN
            .iter()
            .filter(|w| w.category == "media")
            .count();
        assert_eq!(core_count, 3, "Should have 3 core workflows");
        assert_eq!(file_count, 6, "Should have 6 file workflows");
        assert_eq!(media_count, 6, "Should have 6 media workflows");
    }

    #[test]
    fn test_showcase_builtin_core_tools_covered() {
        let all_content: String = SHOWCASE_BUILTIN.iter().map(|w| w.content).collect();
        assert!(all_content.contains("\"nika:log\""), "Must use nika:log");
        assert!(all_content.contains("\"nika:emit\""), "Must use nika:emit");
        assert!(
            all_content.contains("\"nika:assert\""),
            "Must use nika:assert"
        );
        assert!(
            all_content.contains("\"nika:sleep\""),
            "Must use nika:sleep"
        );
    }

    #[test]
    fn test_showcase_builtin_file_tools_covered() {
        let all_content: String = SHOWCASE_BUILTIN.iter().map(|w| w.content).collect();
        assert!(all_content.contains("\"nika:read\""), "Must use nika:read");
        assert!(
            all_content.contains("\"nika:write\""),
            "Must use nika:write"
        );
        assert!(all_content.contains("\"nika:edit\""), "Must use nika:edit");
        assert!(all_content.contains("\"nika:glob\""), "Must use nika:glob");
        assert!(all_content.contains("\"nika:grep\""), "Must use nika:grep");
    }

    #[test]
    fn test_showcase_builtin_media_tools_covered() {
        let all_content: String = SHOWCASE_BUILTIN.iter().map(|w| w.content).collect();
        assert!(
            all_content.contains("\"nika:import\""),
            "Must use nika:import"
        );
        assert!(
            all_content.contains("\"nika:dimensions\""),
            "Must use nika:dimensions"
        );
        assert!(
            all_content.contains("\"nika:thumbhash\""),
            "Must use nika:thumbhash"
        );
        assert!(
            all_content.contains("\"nika:dominant_color\""),
            "Must use nika:dominant_color"
        );
        assert!(
            all_content.contains("\"nika:thumbnail\""),
            "Must use nika:thumbnail"
        );
        assert!(
            all_content.contains("\"nika:optimize\""),
            "Must use nika:optimize"
        );
        assert!(
            all_content.contains("\"nika:pipeline\""),
            "Must use nika:pipeline"
        );
        assert!(
            all_content.contains("\"nika:phash\""),
            "Must use nika:phash"
        );
        assert!(
            all_content.contains("\"nika:compare\""),
            "Must use nika:compare"
        );
        assert!(
            all_content.contains("\"nika:svg_render\""),
            "Must use nika:svg_render"
        );
    }
}
