//! Assembler functions composing shared content modules into tool-specific AI rule formats.
//!
//! Each AI coding assistant has its own format requirements (MDC frontmatter, globs,
//! character limits). The shared modules in `rules/shared/` are composed at compile time
//! via `include_str!()` into tool-specific outputs.
//!
//! SPDX-License-Identifier: AGPL-3.0-or-later

// ─── Shared Content Modules (compile-time embedded) ────────────────────────────

const IDENTITY: &str = include_str!("../rules/shared/identity.md");
const VERBS: &str = include_str!("../rules/shared/verbs.md");
const DATA_FLOW: &str = include_str!("../rules/shared/data-flow.md");
const STRUCTURED_OUTPUT: &str = include_str!("../rules/shared/structured-output.md");
const COMMON_MISTAKES: &str = include_str!("../rules/shared/common-mistakes.md");
const PROVIDERS: &str = include_str!("../rules/shared/providers.md");
const ADVANCED: &str = include_str!("../rules/shared/advanced.md");

// ─── Assemblers ────────────────────────────────────────────────────────────────

/// Assemble Claude Code rules (~/.claude/rules/nika.md).
/// Full reference: identity + verbs + data_flow + structured + mistakes + providers + advanced.
pub fn assemble_claude_rules() -> String {
    format!(
        "# Nika Workflow Engine\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{STRUCTURED_OUTPUT}\n\n{COMMON_MISTAKES}\n\n{PROVIDERS}\n\n{ADVANCED}\n"
    )
}

/// Assemble AGENTS.md — cross-tool, no frontmatter.
/// Identity + verbs + data_flow + mistakes. Works with any AI assistant.
pub fn assemble_agents_md() -> String {
    format!(
        "# Nika Workflow Engine\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{STRUCTURED_OUTPUT}\n\n{COMMON_MISTAKES}\n"
    )
}

/// Assemble Cursor project rules (.cursor/rules/nika-project.mdc).
/// Layer 0 — alwaysApply, identity only. Under 25 lines.
pub fn assemble_cursor_project_mdc() -> String {
    format!(
        "---\ndescription: Nika workflow engine project identity\nalwaysApply: true\n---\n\n{IDENTITY}\n"
    )
}

/// Assemble Cursor syntax rules (.cursor/rules/nika-syntax.mdc).
/// Layer 1 — globs on *.nika.yaml, verbs + data flow.
pub fn assemble_cursor_syntax_mdc() -> String {
    format!(
        "---\ndescription: Nika workflow syntax — 5 verbs and data flow\nglobs: \"**/*.nika.yaml\"\n---\n\n{VERBS}\n\n{DATA_FLOW}\n\n{STRUCTURED_OUTPUT}\n"
    )
}

/// Assemble Cursor reference rules (.cursor/rules/nika-reference.mdc).
/// Layer 2 — Agent Requested (no alwaysApply, no globs).
pub fn assemble_cursor_reference_mdc() -> String {
    format!(
        "---\ndescription: Nika reference — common mistakes, providers, advanced features\n---\n\n{COMMON_MISTAKES}\n\n{PROVIDERS}\n\n{ADVANCED}\n"
    )
}

/// Assemble Copilot instructions (.github/copilot-instructions.md).
/// Identity + verbs + data flow + structured output + mistakes.
pub fn assemble_copilot_instructions() -> String {
    format!(
        "# Nika Workflow Engine\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{STRUCTURED_OUTPUT}\n\n{COMMON_MISTAKES}\n"
    )
}

/// Assemble Windsurf rules (.windsurfrules or .windsurf/rules/nika.md).
/// Must stay under 6000 characters (Windsurf hard limit).
pub fn assemble_windsurf_rules() -> String {
    format!(
        "---\ntrigger: glob\nglobs: \"**/*.nika.yaml\"\ndescription: \"Nika YAML workflow engine rules\"\n---\n\n{IDENTITY}\n\n{VERBS}\n\n{COMMON_MISTAKES}\n"
    )
}

/// Assemble Roo Code rules (~/.roo/rules/nika.md).
/// Identity + verbs + data flow + mistakes.
pub fn assemble_roo_rules() -> String {
    format!("# Nika Workflow Engine\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{COMMON_MISTAKES}\n")
}

/// Assemble Gemini CLI rules (~/.gemini/GEMINI.md).
/// Identity + verbs + data flow.
pub fn assemble_gemini_md() -> String {
    format!(
        "# Nika Workflow Engine\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{STRUCTURED_OUTPUT}\n\n{COMMON_MISTAKES}\n"
    )
}

/// Assemble Amazon Q rules (.amazonq/rules/nika.rule.md).
/// Purpose/Instructions/Priority format.
pub fn assemble_amazonq_rules() -> String {
    format!(
        "# Nika Workflow Engine\n\n**Purpose**: YAML workflow engine for AI tasks — schema `nika/workflow@0.12`\n\n**Priority**: High — apply when editing `.nika.yaml` files\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{COMMON_MISTAKES}\n"
    )
}

/// Assemble JetBrains AI rules (.aiassistant/rules/nika.md).
/// Standard Markdown — identity + verbs + data flow.
pub fn assemble_jetbrains_rules() -> String {
    format!(
        "# Nika Workflow Engine\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{STRUCTURED_OUTPUT}\n\n{COMMON_MISTAKES}\n"
    )
}

/// Assemble Cline rules (.clinerules).
/// Plain text, no extension — identity + verbs + data flow + mistakes.
pub fn assemble_cline_rules() -> String {
    format!("# Nika Workflow Engine\n\n{IDENTITY}\n\n{VERBS}\n\n{DATA_FLOW}\n\n{COMMON_MISTAKES}\n")
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembled_claude_contains_identity() {
        let claude = assemble_claude_rules();
        assert!(claude.contains("nika/workflow@0.12"));
        assert!(claude.contains("infer:"));
        assert!(claude.contains("exec:"));
        assert!(claude.contains("fetch:"));
        assert!(claude.contains("invoke:"));
        assert!(claude.contains("agent:"));
    }

    #[test]
    fn assembled_claude_under_600_lines() {
        let claude = assemble_claude_rules();
        let lines = claude.lines().count();
        assert!(lines < 600, "Claude rules too long: {lines} lines");
    }

    #[test]
    fn assembled_cursor_project_under_25_lines() {
        let mdc = assemble_cursor_project_mdc();
        let lines = mdc.lines().count();
        assert!(lines < 25, "Cursor project mdc too long: {lines} lines");
        assert!(mdc.contains("alwaysApply: true"));
    }

    #[test]
    fn assembled_cursor_syntax_has_globs() {
        let mdc = assemble_cursor_syntax_mdc();
        assert!(mdc.contains("globs:"));
        assert!(mdc.contains(".nika.yaml"));
        assert!(mdc.contains("infer:"));
    }

    #[test]
    fn assembled_cursor_reference_is_agent_requested() {
        let mdc = assemble_cursor_reference_mdc();
        assert!(!mdc.contains("alwaysApply: true"));
    }

    #[test]
    fn assembled_copilot_has_nika_content() {
        let copilot = assemble_copilot_instructions();
        assert!(copilot.contains("nika/workflow@0.12"));
        assert!(copilot.contains("infer:"));
    }

    #[test]
    fn assembled_agents_md_is_cross_tool() {
        let agents = assemble_agents_md();
        assert!(agents.contains("nika/workflow@0.12"));
        assert!(agents.contains("nika check"));
        assert!(agents.contains("nika run"));
        assert!(!agents.contains("alwaysApply"));
        assert!(!agents.contains("globs:"));
    }

    #[test]
    fn all_assemblies_contain_five_verbs() {
        let outputs = vec![
            ("claude", assemble_claude_rules()),
            ("cursor_syntax", assemble_cursor_syntax_mdc()),
            ("copilot", assemble_copilot_instructions()),
            ("agents", assemble_agents_md()),
            ("windsurf", assemble_windsurf_rules()),
            ("roo", assemble_roo_rules()),
            ("gemini", assemble_gemini_md()),
            ("amazonq", assemble_amazonq_rules()),
            ("jetbrains", assemble_jetbrains_rules()),
            ("cline", assemble_cline_rules()),
        ];
        for (name, output) in &outputs {
            for verb in ["infer:", "exec:", "fetch:", "invoke:", "agent:"] {
                assert!(output.contains(verb), "{name} assembly missing verb {verb}");
            }
        }
    }

    #[test]
    fn no_assembly_exceeds_token_budget() {
        let max_chars = 32000; // ~8000 tokens at 4 chars/token
        let assemblies = vec![
            ("claude", assemble_claude_rules()),
            ("cursor_project", assemble_cursor_project_mdc()),
            ("cursor_syntax", assemble_cursor_syntax_mdc()),
            ("cursor_reference", assemble_cursor_reference_mdc()),
            ("copilot", assemble_copilot_instructions()),
            ("agents", assemble_agents_md()),
            ("windsurf", assemble_windsurf_rules()),
            ("roo", assemble_roo_rules()),
            ("gemini", assemble_gemini_md()),
            ("amazonq", assemble_amazonq_rules()),
            ("jetbrains", assemble_jetbrains_rules()),
            ("cline", assemble_cline_rules()),
        ];
        for (name, content) in assemblies {
            assert!(
                content.len() < max_chars,
                "{name} exceeds token budget: {} chars (max {max_chars})",
                content.len()
            );
        }
    }

    #[test]
    fn all_ai_tools_have_assemblers() {
        let _ = assemble_claude_rules();
        let _ = assemble_cursor_project_mdc();
        let _ = assemble_cursor_syntax_mdc();
        let _ = assemble_cursor_reference_mdc();
        let _ = assemble_copilot_instructions();
        let _ = assemble_windsurf_rules();
        let _ = assemble_roo_rules();
        let _ = assemble_gemini_md();
        let _ = assemble_amazonq_rules();
        let _ = assemble_jetbrains_rules();
        let _ = assemble_cline_rules();
        let _ = assemble_agents_md();
    }

    #[test]
    fn all_assemblers_mention_schema_version() {
        let assemblers: Vec<(&str, String)> = vec![
            ("claude", assemble_claude_rules()),
            ("cursor_syntax", assemble_cursor_syntax_mdc()),
            ("copilot", assemble_copilot_instructions()),
            ("windsurf", assemble_windsurf_rules()),
            ("roo", assemble_roo_rules()),
            ("gemini", assemble_gemini_md()),
            ("amazonq", assemble_amazonq_rules()),
            ("jetbrains", assemble_jetbrains_rules()),
            ("cline", assemble_cline_rules()),
            ("agents", assemble_agents_md()),
        ];
        for (name, content) in assemblers {
            assert!(
                content.contains("nika/workflow@0.12"),
                "{name} assembly missing schema version"
            );
        }
    }

    #[test]
    fn windsurf_under_6000_chars() {
        let ws = assemble_windsurf_rules();
        assert!(
            ws.len() < 6000,
            "Windsurf rules exceed 6000 char limit: {} chars",
            ws.len()
        );
    }
}
