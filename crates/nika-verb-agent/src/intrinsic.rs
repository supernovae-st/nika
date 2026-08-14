// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Loop intrinsics — capabilities the agent loop serves ITSELF, never
//! dispatched to the tool executor (ADR-096).
//!
//! `nika:compose` lets the model draft a Nika workflow and get the FULL
//! static verdict back in-turn: parse + Core conformance + the
//! secret-flow / permits analyses + the AARA cost-and-termination
//! certificate (`nika-schema::check` — the same ladder `nika check`
//! runs). The draft never executes here: composition yields an ARTIFACT
//! plus its certificate, and execution stays a separate, gated decision
//! («generation is not permission» — the Proposal–Certification–
//! Execution discipline · Liu Yanglet · Wang · Capponi 2026,
//! arxiv.org/abs/2605.24462). Actions-as-code beats bespoke action DSLs
//! (`CodeAct` · Wang et al. 2024, arxiv.org/abs/2402.01030); for Nika the
//! agent's «code» IS the workflow language, so the existing checker is
//! the verifier for free — and the feedback loop (draft → findings →
//! repair) is exactly the agent-facing surface `CheckReport` documents.
//!
//! Like the `nika:done` sentinel, intrinsic definitions are synthesized
//! by the loop (a poisoned upstream def can never shadow them) and are
//! still DEFAULT-DENY: absent from `tools:`, the model never sees them.

use nika_kernel::ai::provider::ToolDef;
use nika_schema::{FileId, ParseMode};

/// The workflow-drafting intrinsic (whitelist-gated like every tool).
pub const COMPOSE_TOOL: &str = "nika:compose";

/// Drafts above this size are rejected before parsing (the YAML comes
/// from the model — bound untrusted input ahead of the parser).
const MAX_DRAFT_BYTES: usize = 256 * 1024;

/// Violations echoed back per check (the model repairs from the FIRST
/// errors; a wall of 200 findings buries the signal).
const MAX_ECHOED_VIOLATIONS: usize = 12;

/// A recognized intrinsic call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intrinsic {
    /// `nika:compose` — statically check a drafted workflow.
    Compose,
}

impl Intrinsic {
    /// Recognize an intrinsic by its full tool name.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        (name == COMPOSE_TOOL).then_some(Self::Compose)
    }
}

/// Is this name LOOP-OWNED — synthesized + served by the loop itself,
/// never a source-provided def, never an executor dispatch? The ONE
/// definition of the closed two-tool set — the `nika:done` sentinel +
/// the `nika:compose` self-check intrinsic — consumed at all three sites
/// that used to spell it out separately (the def filter, the recency-pin,
/// this module's `parse`). Both are `nika:` builtins (loop-only · ADR-096)
/// the loop owns rather than dispatches.
pub(crate) fn is_loop_owned(name: &str) -> bool {
    name == crate::DONE_TOOL || name == COMPOSE_TOOL
}

/// Every loop-owned definition the whitelist admits — synthesized HERE so
/// a poisoned upstream def can never shadow them (the model sees the
/// loop's own `nika:done` + `nika:compose`, default-deny when absent
/// from `tools:`).
pub(crate) fn synthesized_defs(whitelist: &crate::Whitelist) -> Vec<ToolDef> {
    let mut defs = Vec::new();
    if whitelist.admits(crate::DONE_TOOL) {
        defs.push(done_def());
    }
    if whitelist.admits(COMPOSE_TOOL) {
        defs.push(compose_def());
    }
    defs
}

/// The synthesized `nika:done` sentinel definition (loop-owned).
fn done_def() -> ToolDef {
    ToolDef::new(
        crate::DONE_TOOL,
        "Finish the task. Pass `result` when the answer is a value; \
         omit it to finish with your final message.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "result": {
                    "description": "The final answer value (any JSON)."
                }
            }
        }),
    )
}

/// The synthesized `nika:compose` definition (loop-owned, like the
/// `nika:done` sentinel def).
fn compose_def() -> ToolDef {
    ToolDef::new(
        COMPOSE_TOOL,
        "Statically check a Nika workflow draft you wrote. Returns the full \
         `nika check` verdict as JSON: conformance violations (with codes and \
         repair hints), secret-flow findings, permits escapes, and the \
         termination/cost certificate. Iterate until `valid` is true, then \
         deliver the draft as your result — checking never executes it.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "workflow_yaml": {
                    "type": "string",
                    "description": "The complete workflow YAML draft."
                }
            },
            "required": ["workflow_yaml"]
        }),
    )
}

/// The outcome of one compose check, for telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComposeOutcome {
    /// Parsed AND zero Core-conformance violations.
    pub(crate) valid: bool,
    /// Conformance violation count (parse failure counts as 1).
    pub(crate) violations: u32,
}

/// Run the compose gate: parse → full static check → JSON feedback.
///
/// Returns `(content, is_error, outcome)` — `is_error` follows the
/// agentic feedback convention (the model treats it as a failed tool and
/// repairs); it is NEVER fatal to the loop.
pub(crate) fn run_compose(args: &serde_json::Value) -> (String, bool, ComposeOutcome) {
    let Some(yaml) = args.get("workflow_yaml").and_then(|v| v.as_str()) else {
        return (
            "nika:compose requires `workflow_yaml` (string): pass the complete \
             workflow draft to check."
                .to_owned(),
            true,
            ComposeOutcome {
                valid: false,
                violations: 1,
            },
        );
    };
    if yaml.len() > MAX_DRAFT_BYTES {
        return (
            format!(
                "draft too large ({} bytes > {MAX_DRAFT_BYTES}): split the workflow \
                 or trim embedded data.",
                yaml.len()
            ),
            true,
            ComposeOutcome {
                valid: false,
                violations: 1,
            },
        );
    }

    let workflow = match nika_schema::parse(yaml, FileId::new(0), ParseMode::Strict) {
        Ok(wf) => wf,
        Err(err) => {
            return (
                serde_json::json!({
                    "valid": false,
                    "stage": "parse",
                    "error": err.to_string(),
                })
                .to_string(),
                true,
                ComposeOutcome {
                    valid: false,
                    violations: 1,
                },
            );
        }
    };

    let report = nika_check::check(&workflow);
    let valid = report.conformance.is_empty();
    #[allow(clippy::cast_possible_truncation)] // violation counts ≪ u32::MAX
    let violations = report.conformance.len() as u32;

    let echoed: Vec<serde_json::Value> = report
        .conformance
        .iter()
        .take(MAX_ECHOED_VIOLATIONS)
        .map(|v| {
            serde_json::json!({
                "code": v.code,
                "message": v.message,
            })
        })
        .collect();

    let content = serde_json::json!({
        "valid": valid,
        "stage": "check",
        "conformance": echoed,
        "conformance_total": violations,
        "secret_leaks": report.secret_leaks.len(),
        "secret_egresses": report.secret_egresses.len(),
        "capability_escapes": report.capability_escapes.len(),
        "waves": report.waves.len(),
        "certificate": certificate_summary(&report.certificate),
    })
    .to_string();

    (content, !valid, ComposeOutcome { valid, violations })
}

/// The certificate as bounded SCALARS — never the full witness.
///
/// `RunCertificate.derivation` is one row PER TASK; serializing it whole
/// turns a 256 KiB draft into MB of feedback that re-rides every turn's
/// transcript clone (O(turns²) growth · the amplification the review
/// caught). The model repairs from codes + the headline bounds; the
/// per-task witness exists for local `audit()` re-verification, not for
/// the model. Same `.len()`-not-`.clone()` discipline the sibling fields
/// already use.
fn certificate_summary(cert: &nika_check::RunCertificate) -> serde_json::Value {
    serde_json::json!({
        "task_attempts": cert.task_attempts.constant,
        "llm_calls": cert.llm_calls.constant,
        "effect_calls": cert.effect_calls.constant,
        "span_attempts": cert.span_attempts,
        "parametric_terms": cert.task_attempts.terms.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_DRAFT: &str = r#"nika: composed-by-agent
tasks:
  greet:
    exec:
      command: ["echo", "hello"]
"#;

    #[test]
    fn a_valid_draft_returns_the_certificate() {
        let (content, is_error, outcome) =
            run_compose(&serde_json::json!({"workflow_yaml": VALID_DRAFT}));
        assert!(!is_error, "{content}");
        assert!(outcome.valid);
        assert_eq!(outcome.violations, 0);
        let json: serde_json::Value = serde_json::from_str(&content).expect("JSON feedback");
        assert_eq!(json["valid"], serde_json::json!(true));
        assert!(
            json["certificate"].is_object(),
            "the AARA certificate rides in the feedback: {json}"
        );
    }

    #[test]
    fn an_unparseable_draft_feeds_back_the_parse_error() {
        let (content, is_error, outcome) =
            run_compose(&serde_json::json!({"workflow_yaml": "nika: v1\nworkflow: [unclosed"}));
        assert!(is_error);
        assert!(!outcome.valid);
        let json: serde_json::Value = serde_json::from_str(&content).expect("JSON feedback");
        assert_eq!(json["stage"], serde_json::json!("parse"));
    }

    #[test]
    fn a_conformance_violation_feeds_back_code_and_message() {
        // An `after:` edge on a task that does not exist — a classic DAG
        // error (NIKA-DAG-002).
        let draft = r#"nika: broken
tasks:
  a:
    after:
      ghost: success
    exec:
      command: ["echo", "x"]
"#;
        let (content, is_error, outcome) =
            run_compose(&serde_json::json!({"workflow_yaml": draft}));
        assert!(is_error);
        assert!(!outcome.valid);
        assert!(outcome.violations >= 1);
        let json: serde_json::Value = serde_json::from_str(&content).expect("JSON feedback");
        assert_eq!(json["stage"], serde_json::json!("check"));
        let first = &json["conformance"][0];
        assert!(
            first["code"]
                .as_str()
                .is_some_and(|c| c.starts_with("NIKA-")),
            "prescriptive code rides: {json}"
        );
    }

    #[test]
    fn missing_yaml_arg_is_a_usage_error() {
        let (content, is_error, _) = run_compose(&serde_json::json!({}));
        assert!(is_error);
        assert!(content.contains("workflow_yaml"));
    }

    #[test]
    fn oversized_drafts_are_rejected_before_parsing() {
        let huge = format!("nika: x\n# {}", "y".repeat(MAX_DRAFT_BYTES));
        let (content, is_error, _) = run_compose(&serde_json::json!({"workflow_yaml": huge}));
        assert!(is_error);
        assert!(content.contains("too large"));
    }

    #[test]
    fn a_mid_size_draft_is_accepted_and_the_exact_cap_is_inclusive() {
        // Mid-range: a ~2 KiB valid draft must pass — kills the
        // `256 * 1024` → `256 + 1024` constant mutant (under which the
        // cap collapses to 1280 bytes), which the extreme-size test
        // above can never see.
        let mid = format!("{VALID_DRAFT}# {}\n", "p".repeat(2048));
        let (content, is_error, _) = run_compose(&serde_json::json!({"workflow_yaml": mid}));
        assert!(!is_error, "{content}");

        // Boundary: EXACTLY the cap is accepted (`>` is exclusive — the
        // `>=` mutant rejects the == case). Padding lands in a comment,
        // so the draft stays valid YAML.
        let pad = MAX_DRAFT_BYTES - VALID_DRAFT.len() - 3;
        let exact = format!("{VALID_DRAFT}# {}\n", "q".repeat(pad));
        assert_eq!(exact.len(), MAX_DRAFT_BYTES);
        let (content, is_error, _) = run_compose(&serde_json::json!({"workflow_yaml": exact}));
        assert!(!is_error, "exactly-at-cap must be accepted: {content}");
    }

    #[test]
    fn intrinsic_parse_recognizes_only_the_closed_set() {
        assert_eq!(Intrinsic::parse(COMPOSE_TOOL), Some(Intrinsic::Compose));
        assert_eq!(Intrinsic::parse("nika:compose"), Some(Intrinsic::Compose));
        // Any OTHER tool — a real dispatched builtin, an MCP tool — is not
        // a loop intrinsic.
        assert_eq!(Intrinsic::parse("nika:read"), None);
        assert_eq!(Intrinsic::parse("mcp:srv/tool"), None);
    }
}
