// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error code fix suggestions for Nika.
//!
//! Pure lookup table: `error code → actionable fix hint`.
//! Used by the display layer (which only has the error code string from events)
//! and by `NikaError::fix_suggestion()` in the engine crate.

/// Look up a fix suggestion by error code string (e.g., "NIKA-044").
///
/// Returns a short, actionable hint that helps users resolve the error.
/// This is a pure function with no side effects or dependencies.
pub fn fix_suggestion_for_code(code: &str) -> Option<&'static str> {
    match code {
        // ═══════════════════════════════════════════════════════════════════
        // Workflow errors (000-009)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-001" | "NIKA-095" => Some("Check YAML syntax: indentation and quoting"),
        "NIKA-002" => Some("Use schema: \"nika/workflow@0.12\""),
        "NIKA-003" => Some("Check the file path exists"),
        "NIKA-004" => Some("Check workflow structure matches schema"),
        "NIKA-005" => Some("Check YAML against schemas/nika-workflow.schema.json"),
        "NIKA-006" => {
            Some("Set NIKA_HOME environment variable to specify Nika home directory")
        }
        // ═══════════════════════════════════════════════════════════════════
        // Schema errors (010-019)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-013" => {
            Some("Check the schema file path is correct relative to the workflow file")
        }
        "NIKA-014" => Some("Ensure the schema file contains valid JSON (not YAML)"),
        // ═══════════════════════════════════════════════════════════════════
        // DAG errors (020-029)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-020" => Some("Remove circular dependencies from your workflow"),
        "NIKA-021" => Some("Add the missing task or fix the dependency reference"),
        "NIKA-022" => Some("Each task must have a unique ID. Rename one of the duplicate tasks."),
        "NIKA-026" => Some("Fix the root task failure, then re-run the workflow"),
        "NIKA-027" => Some("Task was cancelled. Check workflow execution logs."),
        "NIKA-028" => {
            Some("Internal error — concurrency semaphore closed unexpectedly. Please report this bug.")
        }
        // ═══════════════════════════════════════════════════════════════════
        // Provider errors (030-039)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-030" => Some("Add provider configuration to your workflow"),
        "NIKA-031" => Some("Check API key and provider availability"),
        "NIKA-032" => {
            Some("Run `nika setup` to configure a provider, or set the env var (e.g. ANTHROPIC_API_KEY)")
        }
        "NIKA-033" => Some("Check configuration value is valid"),
        "NIKA-034" => Some(
            "Add model: to your workflow header or task (e.g., model: claude-sonnet-4-20250514)",
        ),
        "NIKA-035" => Some("Add [endpoints.<name>] to ~/.config/nika/config.toml"),
        "NIKA-036" => Some("Check endpoint URL, network, and API key"),
        "NIKA-037" => Some("Add more providers to routing.fallback or check provider health"),
        "NIKA-038" => {
            Some("Increase max_duration_secs in workflow header or optimize slow tasks")
        }
        // ═══════════════════════════════════════════════════════════════════
        // Binding/Template errors (040-049)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-041" | "NIKA-074" => Some("Use {{with.alias}} format with with: block"),
        "NIKA-042" => Some("Verify the binding alias exists in with: block or task outputs"),
        "NIKA-043" => Some("Check binding value type matches expected type"),
        "NIKA-044" => {
            Some("Check command syntax, working directory, and shell availability")
        }
        "NIKA-045" => Some("Check URL, network connectivity, and response size limits"),
        "NIKA-046" => {
            Some("Check extract mode name, CSS selector syntax, or enable required feature")
        }
        "NIKA-047" => {
            Some("Check invoke params are valid JSON and template bindings resolve correctly")
        }
        // ═══════════════════════════════════════════════════════════════════
        // Path/Task errors (050-059)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-050" => Some("Use format: task_id.field.subfield"),
        "NIKA-052" => Some("Add '?? default' or ensure task outputs JSON"),
        "NIKA-053" => {
            Some("Use shell: true to opt-in to shell execution, or use a different command")
        }
        "NIKA-055" => {
            Some("Task IDs must be snake_case: lowercase letters, digits, underscores")
        }
        "NIKA-056" => Some("Default values must be valid JSON. Strings must be quoted."),
        // ═══════════════════════════════════════════════════════════════════
        // Output errors (060-069)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-060" => Some("Ensure output is valid JSON"),
        "NIKA-061" => Some("Fix output to match declared schema"),
        "NIKA-062" => Some("Check data structure is serializable"),
        // ═══════════════════════════════════════════════════════════════════
        // With block errors (070-089)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-071" => Some("Declare the alias in with: block before referencing"),
        "NIKA-072" => Some("Provide a default value or ensure non-null output"),
        "NIKA-073" => Some("Check the path - accessing field on non-object"),
        "NIKA-075" => {
            Some("Check: nika keys set <service> or use ?? default. Run: nika doctor --fix")
        }
        "NIKA-080" => Some("Verify the task_id exists in your workflow"),
        "NIKA-081" => Some("Add depends_on: [source_task] to this task"),
        "NIKA-082" => Some("Remove the circular dependency"),
        "NIKA-083" => {
            Some("Check for circular dependencies or unresolvable task graph structures")
        }
        // ═══════════════════════════════════════════════════════════════════
        // JSONPath/IO errors (090-099)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-090" => Some("Use simple paths like $.field.subfield"),
        "NIKA-093" => Some("Check file path and permissions"),
        "NIKA-094" => Some("Check JSON syntax"),
        "NIKA-096" => Some("Check command/URL is valid"),
        "NIKA-097" => Some("The workflow was cancelled. No action needed."),
        "NIKA-098" => Some("A task panicked unexpectedly. This is likely a bug — check task logic."),
        // ═══════════════════════════════════════════════════════════════════
        // MCP errors (100-109)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-100" => Some("Check MCP server is running and configured correctly"),
        "NIKA-101" => Some("Check MCP command and args in workflow config"),
        "NIKA-102" => Some("Check tool parameters and MCP server logs"),
        "NIKA-103" => Some("Verify the resource URI exists"),
        "NIKA-104" => Some("Check MCP server compatibility"),
        "NIKA-105" => Some("Add MCP server config to workflow 'mcp:' section"),
        "NIKA-106" => Some("Check MCP server is returning valid JSON responses"),
        "NIKA-107" => Some("Review the tool's parameter schema"),
        "NIKA-108" => Some("Check MCP server's tool schema definitions"),
        "NIKA-109" => {
            Some("MCP server is slow or unresponsive. Check network and server health.")
        }
        "NIKA-110" => Some("Check MCP tool parameters and server logs"),
        // ═══════════════════════════════════════════════════════════════════
        // Agent errors (110-119)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-112" => {
            Some("One or more guardrails failed. Check guardrail config or adjust on_failure action")
        }
        "NIKA-113" => {
            Some("Check agent prompt is not empty and max_turns is valid (1-100)")
        }
        "NIKA-114" => {
            Some("Increase limits or set on_limit_reached.action to complete_partial")
        }
        "NIKA-115" => Some("Check LLM provider API key and network connectivity"),
        "NIKA-116" => Some("Check Claude API response and streaming connection"),
        // ═══════════════════════════════════════════════════════════════════
        // Resilience errors (120-129)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-121" => Some("Increase timeout or check for slow operations"),
        // ═══════════════════════════════════════════════════════════════════
        // TUI/Config errors (130-139)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-130" => Some("Check terminal compatibility and size"),
        "NIKA-135" => Some("Check ~/.config/nika/config.toml for syntax errors"),
        // ═══════════════════════════════════════════════════════════════════
        // Policy/Boot/Startup errors (165-171)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-165" => {
            Some("This action was blocked by security policy. Check nika.toml [policy] section.")
        }
        "NIKA-166" => Some("Boot sequence failed. Run 'nika doctor' to diagnose."),
        "NIKA-167" => {
            Some("Check directory permissions and run 'nika init' to create required directories")
        }
        "NIKA-171" => Some(
            "Decompose expansion timed out. Try reducing max_items or check MCP server performance.",
        ),
        // ═══════════════════════════════════════════════════════════════════
        // Tool errors (200-213)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-200" => Some("Check file path and permissions. Use Read before Edit."),
        "NIKA-210" => Some("Check builtin tool parameters and configuration"),
        "NIKA-212" => {
            Some("Check the parameter format matches the expected JSON schema")
        }
        "NIKA-213" => Some("The condition evaluated to false"),
        // ═══════════════════════════════════════════════════════════════════
        // Context errors (250)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-250" => Some("Check the file path exists and is readable"),
        // ═══════════════════════════════════════════════════════════════════
        // Media errors (251-259)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-251" | "NIKA-252" | "NIKA-253" | "NIKA-254" | "NIKA-255" | "NIKA-256"
        | "NIKA-257" | "NIKA-258" | "NIKA-259" => {
            Some("Check media content and CAS store configuration")
        }
        // ═══════════════════════════════════════════════════════════════════
        // Pkg URI errors (260-261)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-260" => Some(
            "Use format: pkg:@scope/name@version/path (e.g., pkg:@supernovae/skills@1.0.0/rust.md)",
        ),
        "NIKA-261" => Some(
            "Check package name and version. Run 'nika pkg list' to see installed packages.",
        ),
        // ═══════════════════════════════════════════════════════════════════
        // Skill errors (270)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-270" => Some(
            "Ensure skill file exists and is readable. Check pkg: URI format if using packages.",
        ),
        // ═══════════════════════════════════════════════════════════════════
        // Artifact errors (280-285)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-280" => Some(
            "Check the artifact path is within the workflow directory and does not contain path traversal patterns",
        ),
        "NIKA-281" => Some("Check file permissions and disk space"),
        "NIKA-282" => {
            Some("Increase artifacts.max_size in workflow or reduce output size")
        }
        "NIKA-285" => Some(
            "A workflow is currently running. Use --force to override or wait for completion",
        ),
        // ═══════════════════════════════════════════════════════════════════
        // Structured Output errors (300-303)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-300" => {
            Some("Check the LLM response format matches the expected JSON Schema")
        }
        "NIKA-301" => Some(
            "Fix JSON output to match the declared schema. Check required fields and types.",
        ),
        "NIKA-302" => Some(
            "The LLM could not repair the output. Consider simplifying the schema or providing more context.",
        ),
        "NIKA-303" => Some(
            "All validation layers failed. Check your schema is valid and the prompt provides enough context for the LLM to generate conforming output.",
        ),
        // ═══════════════════════════════════════════════════════════════════
        // Course errors (310-314)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-310" => Some("Run `nika init --course` to create a course"),
        "NIKA-311" => Some("Review the exercise instructions and fix your workflow"),
        "NIKA-312" => Some(
            "Complete all exercises in the prerequisite level before unlocking this one",
        ),
        "NIKA-313" => {
            Some("Delete .nika/course-progress.json and restart the course")
        }
        "NIKA-314" => {
            Some("Check file permissions and that the course directory exists")
        }
        // ═══════════════════════════════════════════════════════════════════
        // Record compression errors (320-324)
        // ═══════════════════════════════════════════════════════════════════
        "NIKA-320" => Some(
            "Compression is non-fatal — output was truncated. Check the compression model.",
        ),
        "NIKA-321" => {
            Some("The compression model returned invalid JSON. Try a different model.")
        }
        "NIKA-322" => Some(
            "Lower confidence_threshold in record: spec or use a more capable model.",
        ),
        "NIKA-323" => Some("Increase max_tokens in record: spec."),
        "NIKA-324" => Some(
            "Add a 'summary' agent to agents: block or configure a default provider.",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_return_suggestion() {
        let codes = [
            "NIKA-001", "NIKA-020", "NIKA-041", "NIKA-053", "NIKA-100", "NIKA-200", "NIKA-300",
        ];
        for code in &codes {
            assert!(
                fix_suggestion_for_code(code).is_some(),
                "Expected suggestion for {code}"
            );
        }
    }

    #[test]
    fn unknown_code_returns_none() {
        assert!(fix_suggestion_for_code("NIKA-999").is_none());
        assert!(fix_suggestion_for_code("").is_none());
        assert!(fix_suggestion_for_code("INVALID").is_none());
    }

    #[test]
    fn alias_codes_share_suggestion() {
        // NIKA-001 and NIKA-095 share the same suggestion
        assert_eq!(
            fix_suggestion_for_code("NIKA-001"),
            fix_suggestion_for_code("NIKA-095")
        );
        // NIKA-041 and NIKA-074 share the same suggestion
        assert_eq!(
            fix_suggestion_for_code("NIKA-041"),
            fix_suggestion_for_code("NIKA-074")
        );
    }
}
