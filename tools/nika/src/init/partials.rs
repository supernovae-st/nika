//! Partial Workflows for DAG Fusion
//!
//! These are reusable workflow fragments that can be imported
//! in other workflows via the `imports:` directive.
//!
//! Usage:
//! ```yaml
//! imports:
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
#   imports:
#     - path: ./partials/logging-setup.nika.yaml
#       prefix: log_
#
# This adds tasks: log_start, log_end
# Connect your workflow tasks between them.
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
workflow: logging-setup-partial
description: "Reusable logging setup tasks"

tasks:
  - id: start
    description: "Log workflow start"
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Workflow started"

  - id: end
    description: "Log workflow completion"
    invoke:
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
#   imports:
#     - path: ./partials/error-handling.nika.yaml
#       prefix: err_
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
workflow: error-handling-partial
description: "Standardized error handling tasks"

inputs:
  previous_result: "No result provided"
  error_details: "Unknown error"

tasks:
  - id: check
    description: "Check for errors in previous task"
    infer:
      prompt: |
        Analyze the following result for errors:
        {{inputs.previous_result}}

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
    depends_on: [check]
    with:
      check_result: $check
    invoke:
      tool: nika:log
      params:
        level: error
        message: "Error check result: {{with.check_result}}"

  - id: recover
    description: "Attempt error recovery"
    depends_on: [check]
    with:
      check_result: $check
    infer:
      prompt: |
        An error check returned: {{with.check_result}}

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
#   imports:
#     - path: ./partials/validation.nika.yaml
#       prefix: val_
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
workflow: validation-partial
description: "Input and output validation tasks"

inputs:
  data: "Sample input data for validation"

tasks:
  - id: validate_input
    description: "Validate workflow inputs"
    infer:
      prompt: |
        Validate the following inputs:
        {{inputs.data}}

        Check for:
        1. Required fields present
        2. Correct data types
        3. Value ranges/constraints
        4. Format compliance

        Return JSON:
        {
          "valid": true/false,
          "errors": ["list of validation errors"],
          "warnings": ["list of warnings"]
        }
      temperature: 0.1
      max_tokens: 400

  - id: validate_output
    description: "Validate workflow outputs"
    depends_on: [validate_input]
    with:
      validation: $validate_input
    infer:
      prompt: |
        Review the input validation result:
        {{with.validation}}

        Assess:
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
    depends_on: [validate_input]
    with:
      validation: $validate_input
    infer:
      prompt: |
        Based on validation: {{with.validation}}

        Sanitize the original data:
        {{inputs.data}}

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
#   imports:
#     - path: ./partials/notification.nika.yaml
#       prefix: notify_
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
workflow: notification-partial
description: "Multi-channel notification tasks"

inputs:
  event_type: "workflow_complete"
  details: "Workflow finished successfully"
  recipient: "team"
  channel: "slack"

tasks:
  - id: format_message
    description: "Format notification message"
    infer:
      prompt: |
        Create a notification message:

        Event type: {{inputs.event_type}}
        Details: {{inputs.details}}
        Recipient: {{inputs.recipient}}
        Channel: {{inputs.channel}}

        Format appropriately for the channel:
        - Slack: Use markdown, emojis
        - Email: HTML-safe, structured
        - SMS: Concise, <160 chars

        Return formatted message.
      temperature: 0.3
      max_tokens: 300

  - id: send_slack
    description: "Send Slack notification (placeholder)"
    depends_on: [format_message]
    with:
      formatted: $format_message
    invoke:
      tool: nika:log
      params:
        level: info
        message: "[SLACK] {{with.formatted}}"

  - id: send_email
    description: "Send email notification (placeholder)"
    depends_on: [format_message]
    with:
      formatted: $format_message
    invoke:
      tool: nika:log
      params:
        level: info
        message: "[EMAIL] {{with.formatted}}"

  - id: completion_summary
    description: "Create completion summary notification"
    depends_on: [send_slack, send_email]
    with:
      slack_result: $send_slack
      email_result: $send_email
    infer:
      prompt: |
        Create a completion summary:

        Slack notification sent: {{with.slack_result}}
        Email notification sent: {{with.email_result}}

        Original event: {{inputs.event_type}} - {{inputs.details}}

        Create a brief, informative summary suitable for logging.
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

schema: "nika/workflow@0.12"
workflow: quality-check-partial
description: "Quality assurance check tasks"

inputs:
  content: "Sample content for quality review"
  target_tone: "professional"

tasks:
  - id: check_grammar
    description: "Check grammar and spelling"
    infer:
      prompt: |
        Review this content for grammar and spelling:

        {{inputs.content}}

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

        {{inputs.content}}

        Target tone: {{inputs.target_tone}}

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

        {{inputs.content}}

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
    depends_on: [check_grammar, check_tone, check_factual]
    with:
      grammar: $check_grammar
      tone: $check_tone
      factual: $check_factual
    infer:
      prompt: |
        Calculate final quality score:

        Grammar check: {{with.grammar}}
        Tone check: {{with.tone}}
        Factual check: {{with.factual}}

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
