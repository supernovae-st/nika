//! Partial Workflows for DAG Fusion
//!
//! These are reusable workflow fragments that can be included
//! in other workflows via the `include:` directive.
//!
//! Usage:
//! ```yaml
//! include:
//!   - path: ./partials/logging-setup.nika.yaml
//!     prefix: log_
//! ```

use super::ContextFile;

/// Directory for partial workflows
pub const PARTIALS_DIR: &str = "partials";

/// Logging setup partial - adds logging tasks to any workflow
pub const PARTIAL_LOGGING: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# PARTIAL: LOGGING SETUP
# ═══════════════════════════════════════════════════════════════════════════════
#
# Include this partial to add structured logging to your workflow.
# Use with prefix to namespace task IDs.
#
# Usage:
#   include:
#     - path: ./partials/logging-setup.nika.yaml
#       prefix: log_
#
# This adds tasks: log_start, log_end
# Connect your workflow tasks between them.
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: logging-setup-partial
description: "Reusable logging setup tasks"

tasks:
  - id: start
    description: "Log workflow start"
    invoke:
      mcp: builtin
      tool: nika:log
      params:
        level: info
        message: "Workflow started at {{timestamp}}"

  - id: end
    description: "Log workflow completion"
    invoke:
      mcp: builtin
      tool: nika:log
      params:
        level: info
        message: "Workflow completed successfully"
"##;

/// Error handling partial - adds try/catch style error handling
pub const PARTIAL_ERROR_HANDLING: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# PARTIAL: ERROR HANDLING
# ═══════════════════════════════════════════════════════════════════════════════
#
# Include this partial for standardized error handling.
# Provides error notification and logging tasks.
#
# Usage:
#   include:
#     - path: ./partials/error-handling.nika.yaml
#       prefix: err_
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: error-handling-partial
description: "Standardized error handling tasks"

tasks:
  - id: check
    description: "Check for errors in previous task"
    infer:
      prompt: |
        Analyze the following result for errors:
        {{use.previous_result}}

        Return JSON:
        {
          "has_error": true/false,
          "error_type": "string or null",
          "error_message": "string or null",
          "recoverable": true/false
        }
      temperature: 0.1
      max_tokens: 200

  - id: notify
    description: "Send error notification"
    invoke:
      mcp: builtin
      tool: nika:log
      params:
        level: error
        message: "Error occurred: {{use.error_details}}"

  - id: recover
    description: "Attempt error recovery"
    infer:
      prompt: |
        An error occurred: {{use.error_details}}

        Suggest recovery steps:
        1. What can we try to recover?
        2. Should we retry?
        3. What should we report?

        Provide actionable recovery plan.
      temperature: 0.3
      max_tokens: 300
"##;

/// Validation partial - adds input/output validation
pub const PARTIAL_VALIDATION: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# PARTIAL: VALIDATION
# ═══════════════════════════════════════════════════════════════════════════════
#
# Include this partial for input/output validation.
# Ensures data quality before and after processing.
#
# Usage:
#   include:
#     - path: ./partials/validation.nika.yaml
#       prefix: val_
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: validation-partial
description: "Input and output validation tasks"

tasks:
  - id: validate_input
    description: "Validate workflow inputs"
    infer:
      prompt: |
        Validate the following inputs:
        {{use.inputs}}

        Check for:
        1. Required fields present
        2. Correct data types
        3. Value ranges/constraints
        4. Format compliance

        Return JSON:
        {
          "valid": true/false,
          "errors": ["list of validation errors"],
          "warnings": ["list of warnings"],
          "sanitized_inputs": {}
        }
      temperature: 0.1
      max_tokens: 400

  - id: validate_output
    description: "Validate workflow outputs"
    infer:
      prompt: |
        Validate the following outputs:
        {{use.outputs}}

        Expected schema: {{use.expected_schema}}

        Check for:
        1. Schema compliance
        2. Data completeness
        3. Quality metrics
        4. Consistency

        Return JSON:
        {
          "valid": true/false,
          "errors": [],
          "quality_score": 0-100,
          "recommendations": []
        }
      temperature: 0.1
      max_tokens: 400

  - id: sanitize
    description: "Sanitize data for safety"
    infer:
      prompt: |
        Sanitize the following data:
        {{use.raw_data}}

        Actions:
        1. Remove any PII
        2. Escape special characters
        3. Normalize formats
        4. Remove duplicates

        Return sanitized version.
      temperature: 0.1
      max_tokens: 500
"##;

/// Notification partial - adds notification capabilities
pub const PARTIAL_NOTIFICATION: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# PARTIAL: NOTIFICATIONS
# ═══════════════════════════════════════════════════════════════════════════════
#
# Include this partial for notification capabilities.
# Supports multiple notification channels.
#
# Usage:
#   include:
#     - path: ./partials/notification.nika.yaml
#       prefix: notify_
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: notification-partial
description: "Multi-channel notification tasks"

tasks:
  - id: format_message
    description: "Format notification message"
    infer:
      prompt: |
        Create a notification message:

        Event type: {{use.event_type}}
        Details: {{use.details}}
        Recipient: {{use.recipient}}
        Channel: {{use.channel}}

        Format appropriately for the channel:
        - Slack: Use markdown, emojis
        - Email: HTML-safe, structured
        - SMS: Concise, <160 chars

        Return formatted message.
      temperature: 0.3
      max_tokens: 300

  - id: send_slack
    description: "Send Slack notification (placeholder)"
    invoke:
      mcp: builtin
      tool: nika:log
      params:
        level: info
        message: "[SLACK] {{use.formatted_message}}"

  - id: send_email
    description: "Send email notification (placeholder)"
    invoke:
      mcp: builtin
      tool: nika:log
      params:
        level: info
        message: "[EMAIL] {{use.formatted_message}}"

  - id: completion_summary
    description: "Create completion summary notification"
    infer:
      prompt: |
        Create a completion summary:

        Workflow: {{use.workflow_name}}
        Duration: {{use.duration}}
        Status: {{use.status}}
        Results: {{use.results}}

        Create a brief, informative summary suitable for notification.
        Include key metrics and any action items.
      temperature: 0.3
      max_tokens: 250
"##;

/// Quality check partial - adds quality assurance steps
pub const PARTIAL_QUALITY_CHECK: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# PARTIAL: QUALITY CHECKS
# ═══════════════════════════════════════════════════════════════════════════════
#
# Include this partial for quality assurance checks.
# Useful for content generation workflows.
#
# Usage:
#   include:
#     - path: ./partials/quality-check.nika.yaml
#       prefix: qa_
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: quality-check-partial
description: "Quality assurance check tasks"

tasks:
  - id: check_grammar
    description: "Check grammar and spelling"
    infer:
      prompt: |
        Review this content for grammar and spelling:

        {{use.content}}

        Check for:
        1. Spelling errors
        2. Grammar issues
        3. Punctuation problems
        4. Awkward phrasing

        Return JSON:
        {
          "score": 0-100,
          "issues": [{"type": "...", "text": "...", "suggestion": "..."}],
          "corrected_content": "..."
        }
      temperature: 0.1
      max_tokens: 800

  - id: check_tone
    description: "Verify tone and style"
    infer:
      prompt: |
        Analyze the tone of this content:

        {{use.content}}

        Target tone: {{use.target_tone}}

        Evaluate:
        1. Does it match target tone?
        2. Is it consistent throughout?
        3. Is it appropriate for audience?

        Return JSON:
        {
          "matches_target": true/false,
          "detected_tone": "...",
          "consistency_score": 0-100,
          "suggestions": []
        }
      temperature: 0.2
      max_tokens: 400

  - id: check_factual
    description: "Flag potential factual claims for review"
    infer:
      prompt: |
        Identify factual claims in this content:

        {{use.content}}

        List any claims that should be fact-checked:
        1. Statistics and numbers
        2. Dates and events
        3. Quotes and attributions
        4. Technical specifications

        Return JSON:
        {
          "claims_to_verify": [
            {"claim": "...", "type": "...", "confidence": 0-100}
          ],
          "high_risk_count": 0
        }
      temperature: 0.2
      max_tokens: 500

  - id: final_score
    description: "Calculate final quality score"
    use:
      grammar: $check_grammar
      tone: $check_tone
      factual: $check_factual
    infer:
      prompt: |
        Calculate final quality score:

        Grammar check: {{use.grammar}}
        Tone check: {{use.tone}}
        Factual check: {{use.factual}}

        Weighted scores:
        - Grammar: 40%
        - Tone: 30%
        - Factual risk: 30%

        Return JSON:
        {
          "final_score": 0-100,
          "grade": "A/B/C/D/F",
          "ready_for_publish": true/false,
          "blocking_issues": [],
          "recommendations": []
        }
      temperature: 0.1
      max_tokens: 300
"##;

/// Returns all partial workflow files
pub fn get_partial_files() -> Vec<ContextFile> {
    vec![
        ContextFile {
            filename: "logging-setup.nika.yaml",
            dir: PARTIALS_DIR,
            content: PARTIAL_LOGGING,
        },
        ContextFile {
            filename: "error-handling.nika.yaml",
            dir: PARTIALS_DIR,
            content: PARTIAL_ERROR_HANDLING,
        },
        ContextFile {
            filename: "validation.nika.yaml",
            dir: PARTIALS_DIR,
            content: PARTIAL_VALIDATION,
        },
        ContextFile {
            filename: "notification.nika.yaml",
            dir: PARTIALS_DIR,
            content: PARTIAL_NOTIFICATION,
        },
        ContextFile {
            filename: "quality-check.nika.yaml",
            dir: PARTIALS_DIR,
            content: PARTIAL_QUALITY_CHECK,
        },
    ]
}
