// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Showcase Infrastructure — 15 workflows demonstrating context, inputs, artifacts, and resilience
//!
//! These workflows cover the infrastructure features that make Nika production-ready:
//!
//! **Context files (1-3):**
//! - Brand voice context loading
//! - Multi-context merge (style + persona + terms)
//! - JSON config context
//!
//! **Inputs (4-6):**
//! - Parameterized workflow with multiple inputs
//! - Input defaults driving for_each iteration
//! - Input validation with fetch
//!
//! **Artifacts (7-10):**
//! - Multi-format export (text + JSON + combined)
//! - Template-based artifact paths
//! - Append mode logging
//! - Binary artifact via fetch response: binary
//!
//! **Composition (11-12):**
//! - Full infrastructure (context + inputs + artifacts + for_each)
//! - Config-driven (inputs override, context defaults)
//!
//! **Resilience (13-15):**
//! - Retry with exponential backoff
//! - Timeout control on exec and fetch
//! - fail_fast: false on for_each
//!
//! No LLM provider needed — all workflows use exec: and fetch: only.

use super::WorkflowTemplate;

/// Return all 15 showcase infrastructure workflows.
pub fn get_showcase_infra_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "01-context-brand-voice.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_01_CONTEXT_BRAND,
        },
        WorkflowTemplate {
            filename: "02-context-multi-merge.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_02_CONTEXT_MULTI,
        },
        WorkflowTemplate {
            filename: "03-context-json-config.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_03_CONTEXT_JSON,
        },
        WorkflowTemplate {
            filename: "04-inputs-parameterized.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_04_INPUTS_PARAMS,
        },
        WorkflowTemplate {
            filename: "05-inputs-with-defaults.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_05_INPUTS_DEFAULTS,
        },
        WorkflowTemplate {
            filename: "06-inputs-validation.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_06_INPUTS_VALIDATION,
        },
        WorkflowTemplate {
            filename: "07-artifact-multi-format.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_07_ARTIFACT_MULTI,
        },
        WorkflowTemplate {
            filename: "08-artifact-template-path.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_08_ARTIFACT_TEMPLATE,
        },
        WorkflowTemplate {
            filename: "09-artifact-append-mode.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_09_ARTIFACT_APPEND,
        },
        WorkflowTemplate {
            filename: "10-artifact-binary.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_10_ARTIFACT_BINARY,
        },
        WorkflowTemplate {
            filename: "11-composition-full.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_11_COMPOSITION,
        },
        WorkflowTemplate {
            filename: "12-config-driven.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_12_CONFIG_DRIVEN,
        },
        WorkflowTemplate {
            filename: "13-retry-with-backoff.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_13_RETRY_BACKOFF,
        },
        WorkflowTemplate {
            filename: "14-timeout-control.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_14_TIMEOUT,
        },
        WorkflowTemplate {
            filename: "15-fail-fast-vs-continue.nika.yaml",
            tier_dir: "showcase-infra",
            content: INFRA_15_FAIL_FAST,
        },
    ]
}

// =============================================================================
// 01: Brand Voice Context
// =============================================================================

const INFRA_01_CONTEXT_BRAND: &str = r##"# =============================================================================
# INFRASTRUCTURE 01 — Brand Voice Context
# =============================================================================
#
# Demonstrates: context.files loading a markdown file into the prompt.
# The brand guide is injected via {{context.files.brand}} so the LLM
# generates content that follows your brand voice automatically.
#
# No API key needed — uses exec: to show the template expansion.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-context-brand-voice
description: "Load a brand guide via context.files and use it in a task"

context:
  files:
    brand: ./.scratch/context/brand.md

tasks:
  - id: show_brand
    description: "Echo the loaded brand context to verify it works"
    exec:
      command: "echo 'Brand context loaded, length: {{context.files.brand}}' | wc -c"
      shell: true

  - id: generate_tagline
    depends_on: [show_brand]
    description: "Use brand voice context in an exec task"
    exec:
      command: "echo 'Following brand voice: active voice, under 20 words, benefit-led'"
      shell: true
"##;

// =============================================================================
// 02: Multi-Context Merge
// =============================================================================

const INFRA_02_CONTEXT_MULTI: &str = r##"# =============================================================================
# INFRASTRUCTURE 02 — Multi-Context Merge
# =============================================================================
#
# Demonstrates: loading multiple context files simultaneously.
# Three files (style guide, persona JSON, terminology) are loaded and
# accessible via their aliases in {{context.files.<alias>}}.
#
# This pattern is essential for building rich prompts that combine
# style rules, persona definition, and domain vocabulary.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-context-multi-merge
description: "Load multiple context files and combine them in tasks"

context:
  files:
    style: ./.scratch/context/style.md
    persona: ./.scratch/context/persona.json
    terms: ./.scratch/context/terms.md

tasks:
  - id: verify_style
    description: "Confirm style context loaded"
    exec:
      command: "echo 'Style context available: {{context.files.style}}' | head -c 80"
      shell: true

  - id: verify_persona
    description: "Confirm persona context loaded"
    exec:
      command: "echo 'Persona context available: {{context.files.persona}}' | head -c 80"
      shell: true

  - id: verify_terms
    description: "Confirm terminology context loaded"
    exec:
      command: "echo 'Terms context available: {{context.files.terms}}' | head -c 80"
      shell: true

  - id: combined
    depends_on: [verify_style, verify_persona, verify_terms]
    description: "All three contexts merge into a single task"
    exec: "echo 'All 3 context files loaded successfully'"
"##;

// =============================================================================
// 03: JSON Context Config
// =============================================================================

const INFRA_03_CONTEXT_JSON: &str = r##"# =============================================================================
# INFRASTRUCTURE 03 — JSON Context Parsing
# =============================================================================
#
# Demonstrates: loading a JSON config file via context.files.
# The JSON content is available as a string via {{context.files.config}}
# and can be used in exec: commands or passed to other tasks.
#
# Real-world use: load deployment configs, feature flags, or
# environment-specific settings into your workflow.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-context-json-config
description: "Load a JSON config file and use it in shell commands"

context:
  files:
    config: ./.scratch/context/config.json

tasks:
  - id: show_config
    description: "Display the loaded JSON configuration"
    exec:
      command: "echo 'Config loaded: {{context.files.config}}' | head -c 120"
      shell: true

  - id: extract_app_name
    depends_on: [show_config]
    description: "Use the config data in a downstream task"
    exec:
      command: "echo 'Application deployment verified'"
      shell: true
"##;

// =============================================================================
// 04: Parameterized Inputs
// =============================================================================

const INFRA_04_INPUTS_PARAMS: &str = r##"# =============================================================================
# INFRASTRUCTURE 04 — Parameterized Workflow
# =============================================================================
#
# Demonstrates: inputs: with multiple parameters and defaults.
# Run with overrides: nika run --set topic=rust --set language=fr
#
# inputs: declares parameters with default values. Access them
# via {{inputs.<name>}} in any task template.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-inputs-parameterized
description: "Workflow with parameterized inputs for topic, language, and format"

inputs:
  topic: "workflow automation"
  language: "en"
  format: "markdown"

tasks:
  - id: show_params
    description: "Display all input parameters"
    exec: "echo 'Topic: {{inputs.topic}}, Language: {{inputs.language}}, Format: {{inputs.format}}'"

  - id: generate
    depends_on: [show_params]
    description: "Use inputs to configure content generation"
    exec: "echo 'Generating {{inputs.format}} content about {{inputs.topic}} in {{inputs.language}}'"
"##;

// =============================================================================
// 05: Input Defaults with for_each
// =============================================================================

const INFRA_05_INPUTS_DEFAULTS: &str = r##"# =============================================================================
# INFRASTRUCTURE 05 — Input Defaults with for_each
# =============================================================================
#
# Demonstrates: inputs with default values driving a for_each loop.
# The count input controls how many items are generated, and the style
# input configures the output format.
#
# Override defaults: nika run --set count=3 --set style=casual
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-inputs-with-defaults
description: "Inputs with defaults driving a for_each iteration"

inputs:
  count: 5
  style: "formal"

tasks:
  - id: build_list
    description: "Generate a JSON array based on the count input"
    exec:
      command: "echo '[\"item-1\",\"item-2\",\"item-3\",\"item-4\",\"item-5\"]'"
      shell: true

  - id: process_items
    depends_on: [build_list]
    for_each: "$build_list"
    as: item
    description: "Process each item with the configured style"
    exec: "echo 'Processing {{with.item}} in {{inputs.style}} style'"
"##;

// =============================================================================
// 06: Input Validation
// =============================================================================

const INFRA_06_INPUTS_VALIDATION: &str = r##"# =============================================================================
# INFRASTRUCTURE 06 — Input Validation
# =============================================================================
#
# Demonstrates: required inputs (no default = must be provided) and
# inputs with defaults as fallback values.
#
# Run: nika run --set url=https://httpbin.org/get
# Omitting --set url will use the default.
#
# The depth input defaults to 1 if not overridden.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-inputs-validation
description: "Inputs with required-like semantics and depth defaults"

inputs:
  url: "https://httpbin.org/get"
  depth: 1

tasks:
  - id: validate
    description: "Validate that the URL input is not empty"
    exec:
      command: "echo 'Validating URL: {{inputs.url}} at depth {{inputs.depth}}'"
      shell: true

  - id: crawl
    depends_on: [validate]
    description: "Fetch the URL with the configured depth"
    fetch:
      url: "{{inputs.url}}"
      method: GET
      timeout: 10
"##;

// =============================================================================
// 07: Multi-Format Artifact Export
// =============================================================================

const INFRA_07_ARTIFACT_MULTI: &str = r##"# =============================================================================
# INFRASTRUCTURE 07 — Multi-Format Export
# =============================================================================
#
# Demonstrates: writing multiple artifact files from different tasks.
# Each task produces output in a different format (text, json, yaml).
#
# artifacts.dir sets the base output directory.
# Each task's artifact: block specifies path and format.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-artifact-multi-format
description: "Export task results as text, JSON, and YAML artifacts"

artifacts:
  dir: ./.scratch/output/multi-format

tasks:
  - id: gen_text
    description: "Generate plain text content"
    exec: "echo 'Infrastructure workflows demonstrate context, inputs, and artifacts.'"
    artifact:
      path: report.txt
      format: text

  - id: gen_json
    description: "Generate JSON content"
    exec:
      command: "echo '{\"status\":\"ok\",\"features\":[\"context\",\"inputs\",\"artifacts\"]}'"
      shell: true
    artifact:
      path: report.json
      format: json

  - id: gen_yaml
    depends_on: [gen_text, gen_json]
    with:
      text_result: $gen_text
      json_result: $gen_json
    description: "Generate combined output"
    exec: "echo 'text: {{with.text_result}}'"
    artifact:
      path: combined.txt
      format: text
"##;

// =============================================================================
// 08: Artifact Template Path
// =============================================================================

const INFRA_08_ARTIFACT_TEMPLATE: &str = r##"# =============================================================================
# INFRASTRUCTURE 08 — Artifact with Template Path
# =============================================================================
#
# Demonstrates: using template expressions in artifact paths.
# The artifact path uses the workflow name and task ID to create
# a structured output directory automatically.
#
# This pattern keeps outputs organized when running many workflows.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-artifact-template-path
description: "Artifact paths with template-based directory structure"

artifacts:
  dir: ./.scratch/output/template-path

tasks:
  - id: gen_report
    description: "Generate a report saved to a templated path"
    exec: "echo 'Report generated at structured path'"
    artifact:
      path: "reports/gen_report.md"
      format: text

  - id: gen_summary
    depends_on: [gen_report]
    with:
      report: $gen_report
    description: "Generate a summary alongside the report"
    exec: "echo 'Summary of: {{with.report}}'"
    artifact:
      path: "summaries/gen_summary.md"
      format: text
"##;

// =============================================================================
// 09: Append Mode Logging
// =============================================================================

const INFRA_09_ARTIFACT_APPEND: &str = r##"# =============================================================================
# INFRASTRUCTURE 09 — Append Mode Logging
# =============================================================================
#
# Demonstrates: artifact mode: append — multiple tasks write to the
# same file sequentially. Each task appends its output instead of
# overwriting.
#
# This pattern is perfect for building log files, audit trails,
# or incremental reports across a workflow.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-artifact-append-mode
description: "Three tasks append output to the same log file"

artifacts:
  dir: ./.scratch/output/append-mode

tasks:
  - id: step_1
    description: "First log entry"
    exec: "echo '[STEP 1] Workflow started — initializing pipeline'"
    artifact:
      path: workflow.log
      mode: append

  - id: step_2
    depends_on: [step_1]
    description: "Second log entry"
    exec: "echo '[STEP 2] Processing data — transformations applied'"
    artifact:
      path: workflow.log
      mode: append

  - id: step_3
    depends_on: [step_2]
    description: "Third log entry"
    exec: "echo '[STEP 3] Pipeline complete — all steps succeeded'"
    artifact:
      path: workflow.log
      mode: append
"##;

// =============================================================================
// 10: Binary Artifact
// =============================================================================

const INFRA_10_ARTIFACT_BINARY: &str = r##"# =============================================================================
# INFRASTRUCTURE 10 — Binary Artifact
# =============================================================================
#
# Demonstrates: fetch with response: binary to store binary data in
# the content-addressable store (CAS), then save it as an artifact.
#
# Binary response mode stores the fetched data as a CAS hash instead
# of returning the body as text. The hash can be used in downstream
# tasks or saved as an artifact with format: binary.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-artifact-binary
description: "Fetch binary content and store it as an artifact"

artifacts:
  dir: ./.scratch/output/binary

tasks:
  - id: fetch_image
    description: "Download a PNG image in binary mode"
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  - id: confirm
    depends_on: [fetch_image]
    with:
      hash: $fetch_image
    description: "Confirm the binary was stored in CAS"
    exec: "echo 'Binary stored with CAS hash: {{with.hash}}'"
"##;

// =============================================================================
// 11: Full Composition
// =============================================================================

const INFRA_11_COMPOSITION: &str = r##"# =============================================================================
# INFRASTRUCTURE 11 — Full Infrastructure Composition
# =============================================================================
#
# Demonstrates: combining context + inputs + artifacts + for_each
# in a single workflow. This is the pattern for production workflows
# that load configuration, accept parameters, iterate over data,
# and persist all results.
#
# A real content pipeline: load brand voice, accept topic input,
# generate multiple outputs, save each as an artifact.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-composition-full
description: "Context + inputs + artifacts + for_each in one workflow"

context:
  files:
    brand: ./.scratch/context/brand.md

inputs:
  topic: "developer tools"
  output_count: 3

artifacts:
  dir: ./.scratch/output/composition

tasks:
  - id: prepare
    description: "Build a list of subtopics to iterate over"
    exec:
      command: "echo '[\"getting-started\",\"best-practices\",\"troubleshooting\"]'"
      shell: true

  - id: generate_each
    depends_on: [prepare]
    for_each: "$prepare"
    as: subtopic
    description: "Generate content for each subtopic using brand context"
    exec: "echo 'Content for {{inputs.topic}}/{{with.subtopic}} following brand guidelines'"
    artifact:
      path: "articles/{{with.subtopic}}.txt"
      format: text

  - id: manifest
    depends_on: [generate_each]
    description: "Write a manifest of all generated files"
    exec: "echo 'Generated {{inputs.output_count}} articles about {{inputs.topic}}'"
    artifact:
      path: manifest.txt
      format: text
"##;

// =============================================================================
// 12: Config-Driven Workflow
// =============================================================================

const INFRA_12_CONFIG_DRIVEN: &str = r##"# =============================================================================
# INFRASTRUCTURE 12 — Config-Driven Workflow
# =============================================================================
#
# Demonstrates: inputs providing runtime overrides while context
# provides baseline defaults. The pattern separates "what can change
# per-run" (inputs) from "what stays consistent" (context files).
#
# Context loads a JSON config with stable defaults.
# Inputs let callers override specific values at runtime.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-config-driven
description: "Inputs override runtime behavior while context provides defaults"

context:
  files:
    defaults: ./.scratch/context/config.json

inputs:
  deploy_target: "staging"
  log_level: "debug"

tasks:
  - id: load_config
    description: "Show the base configuration from context"
    exec:
      command: "echo 'Base config: {{context.files.defaults}}' | head -c 100"
      shell: true

  - id: apply_overrides
    depends_on: [load_config]
    description: "Apply input overrides to the configuration"
    exec: "echo 'Deploy target: {{inputs.deploy_target}}, Log level: {{inputs.log_level}}'"

  - id: deploy
    depends_on: [apply_overrides]
    description: "Execute deployment with merged configuration"
    exec: "echo 'Deploying to {{inputs.deploy_target}} with log_level={{inputs.log_level}}'"
"##;

// =============================================================================
// 13: Retry with Backoff
// =============================================================================

const INFRA_13_RETRY_BACKOFF: &str = r##"# =============================================================================
# INFRASTRUCTURE 13 — Retry with Exponential Backoff
# =============================================================================
#
# Demonstrates: retry: configuration with max_attempts, delay_ms,
# and exponential backoff multiplier.
#
# retry:
#   max_attempts: 3    — try up to 3 times total
#   delay_ms: 1000     — wait 1s before first retry
#   backoff: 2.0       — double the delay each retry (1s, 2s, 4s)
#
# Essential for unreliable external APIs and network calls.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-retry-with-backoff
description: "Fetch with retry, delay, and exponential backoff"

tasks:
  - id: fetch_api
    description: "Call an API with retry protection"
    fetch:
      url: "https://httpbin.org/get"
      method: GET
      timeout: 10
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0

  - id: process
    depends_on: [fetch_api]
    with:
      data: $fetch_api
    description: "Process the fetched data after successful retry"
    exec: "echo 'API response received after retries: {{with.data}}' | head -c 80"
"##;

// =============================================================================
// 14: Timeout Control
// =============================================================================

const INFRA_14_TIMEOUT: &str = r##"# =============================================================================
# INFRASTRUCTURE 14 — Timeout Control
# =============================================================================
#
# Demonstrates: timeout: on exec and fetch tasks.
# timeout is specified in SECONDS (the parser converts to ms).
#
# Task-level timeout prevents any single task from blocking the
# entire workflow. Combine with retry: for graceful degradation.
#
# The fast_task completes instantly. The guarded_fetch has a
# 10-second timeout on its HTTP request.
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-timeout-control
description: "Timeout settings on exec and fetch tasks"

tasks:
  - id: fast_task
    description: "A quick command with a generous timeout"
    exec:
      command: "echo 'Completed in well under the timeout'"
      timeout: 5

  - id: guarded_fetch
    depends_on: [fast_task]
    description: "HTTP request with a 10-second timeout guard"
    fetch:
      url: "https://httpbin.org/delay/1"
      method: GET
      timeout: 10

  - id: report
    depends_on: [guarded_fetch]
    with:
      result: $guarded_fetch
    description: "Report that both tasks completed within their timeouts"
    exec: "echo 'All tasks completed within their timeout windows'"
"##;

// =============================================================================
// 15: Fail-Fast vs Continue
// =============================================================================

const INFRA_15_FAIL_FAST: &str = r##"# =============================================================================
# INFRASTRUCTURE 15 — Fail-Fast vs Continue
# =============================================================================
#
# Demonstrates: fail_fast: false on for_each iteration.
# By default, for_each stops on the first error. Setting
# fail_fast: false lets all iterations run, collecting errors
# at the end instead of aborting early.
#
# This pattern is essential for batch processing where partial
# results are better than no results (e.g., processing a list
# of URLs where some may be down).
# =============================================================================

schema: "nika/workflow@0.12"
workflow: infra-fail-fast-vs-continue
description: "for_each with fail_fast: false to continue past errors"

tasks:
  - id: build_urls
    description: "Create a list of URLs to process"
    exec:
      command: "echo '[\"https://httpbin.org/get\",\"https://httpbin.org/status/200\",\"https://httpbin.org/headers\"]'"
      shell: true

  - id: fetch_all
    depends_on: [build_urls]
    for_each: "$build_urls"
    as: url
    fail_fast: false
    concurrency: 2
    description: "Fetch each URL, continuing even if some fail"
    fetch:
      url: "{{with.url}}"
      method: GET
      timeout: 10

  - id: summary
    depends_on: [fetch_all]
    description: "Report results after all URLs are processed"
    exec: "echo 'Batch processing complete — all URLs attempted'"
"##;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infra_workflow_count() {
        let workflows = get_showcase_infra_workflows();
        assert_eq!(
            workflows.len(),
            15,
            "Should have exactly 15 infra workflows"
        );
    }

    #[test]
    fn test_infra_filenames_unique() {
        let workflows = get_showcase_infra_workflows();
        let mut names: Vec<&str> = workflows.iter().map(|w| w.filename).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "All filenames must be unique");
    }

    #[test]
    fn test_infra_all_have_schema() {
        for w in get_showcase_infra_workflows() {
            assert!(
                w.content.contains("schema: \"nika/workflow@0.12\""),
                "Workflow {} missing schema",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_all_have_workflow_name() {
        for w in get_showcase_infra_workflows() {
            assert!(
                w.content.contains("workflow:"),
                "Workflow {} missing workflow: declaration",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_all_have_tasks() {
        for w in get_showcase_infra_workflows() {
            assert!(
                w.content.contains("tasks:"),
                "Workflow {} missing tasks: section",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_all_valid_yaml() {
        for w in get_showcase_infra_workflows() {
            let parsed: Result<serde_json::Value, _> = serde_saphyr::from_str(w.content);
            assert!(
                parsed.is_ok(),
                "Workflow {} is not valid YAML: {:?}",
                w.filename,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_infra_tier_dir() {
        for w in get_showcase_infra_workflows() {
            assert_eq!(
                w.tier_dir, "showcase-infra",
                "Workflow {} should be in showcase-infra tier",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_filenames_numbered() {
        for w in get_showcase_infra_workflows() {
            assert!(
                w.filename.starts_with(char::is_numeric),
                "Workflow {} should start with a number",
                w.filename
            );
            assert!(
                w.filename.ends_with(".nika.yaml"),
                "Workflow {} should end with .nika.yaml",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_no_llm_required() {
        // Infrastructure workflows should not require LLM providers
        for w in get_showcase_infra_workflows() {
            assert!(
                !w.content.contains("{{PROVIDER}}"),
                "Workflow {} should not require LLM provider",
                w.filename
            );
            assert!(
                !w.content.contains("{{MODEL}}"),
                "Workflow {} should not require LLM model",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_context_workflows_have_context() {
        let workflows = get_showcase_infra_workflows();
        for w in &workflows[..3] {
            assert!(
                w.content.contains("context:"),
                "Context workflow {} should have context: block",
                w.filename
            );
            assert!(
                w.content.contains("files:"),
                "Context workflow {} should have files: section",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_inputs_workflows_have_inputs() {
        let workflows = get_showcase_infra_workflows();
        for w in &workflows[3..6] {
            assert!(
                w.content.contains("inputs:"),
                "Inputs workflow {} should have inputs: block",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_artifact_workflows_have_artifacts() {
        let workflows = get_showcase_infra_workflows();
        for w in &workflows[6..10] {
            assert!(
                w.content.contains("artifact:") || w.content.contains("artifacts:"),
                "Artifact workflow {} should have artifact(s) config",
                w.filename
            );
        }
    }

    #[test]
    fn test_infra_resilience_workflows() {
        let workflows = get_showcase_infra_workflows();
        assert!(
            workflows[12].content.contains("retry:"),
            "Workflow 13 should demonstrate retry:"
        );
        assert!(
            workflows[13].content.contains("timeout:"),
            "Workflow 14 should demonstrate timeout:"
        );
        assert!(
            workflows[14].content.contains("fail_fast:"),
            "Workflow 15 should demonstrate fail_fast:"
        );
    }
}
