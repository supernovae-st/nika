// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The measured pre-execution empty-tools profile of `codex exec`.
//!
//! Rejecting a tool item after the turn returns cannot undo a command the
//! seat already ran: on codex-cli 0.160.1 the former argv (read-only
//! sandbox, no disable flags) executed `cat` on a scratch file, `uname`
//! and two web searches before the answer came back. This profile removes
//! the tool-bearing capabilities BEFORE the turn starts, and is admitted
//! only on the codex-cli minors where that was measured against the real
//! subscription: under it the same prompt produced no `command_execution`
//! or `web_search` item, the scratch canary never reached the answer, and
//! the only remaining model-callable functions were the code-mode `exec`
//! (refused at the router: « code-mode host is disabled », nested tools
//! never run) and the catalog's inert `request_user_input_async`.
//!
//! A new codex-cli minor is a new, unmeasured tool surface: it refuses
//! until the probe is re-run and the minor is added here.

use super::{InferGradeError, refused};

/// The codex-cli `(major, minor)` lines on which this exact profile was
/// measured (2026-10-07, codex-cli 0.160.1, subscription sign-in, `gpt-6-astra`).
pub(super) const MEASURED: &[(u32, u32)] = &[(0, 160)];

/// Every tool-bearing or tool-adjacent feature this version knows, forced
/// off before execution. `--disable` of a name the CLI does not know is a
/// hard error (measured), so a renamed feature cannot pass silently.
/// `unified_exec` is absent on purpose: 0.160.1 keeps it on regardless,
/// and its handlers are registered only under `shell_tool`.
pub(super) const DISABLED_FEATURES: &[&str] = &[
    "shell_tool",
    "apps",
    "plugins",
    "remote_plugin",
    "browser_use",
    "browser_use_external",
    "browser_use_full_cdp_access",
    "computer_use",
    "in_app_browser",
    "in_app_local_automation",
    "image_generation",
    "view_image",
    "multi_agent",
    "multi_agent_v2",
    "skill_search",
    "skill_mcp_dependency_install",
    "tool_suggest",
    "sleep_tool",
    "hooks",
    "goals",
    "code_mode",
    "code_mode_only",
    "code_mode_host",
    "workspace_dependencies",
    "tool_call_mcp_elicitation",
    "memories",
    "request_permissions_tool",
    "standalone_web_search",
    "worktrees",
    "realtime_conversation",
];

/// Configuration overrides with no feature flag: hosted web search, the
/// sub-agent collaboration tools, the planning/user-input tools, skill
/// instructions and any MCP server (already excluded by
/// `--ignore-user-config`, repeated here as its own layer).
pub(super) const CONFIG_OVERRIDES: &[&str] = &[
    "web_search=\"disabled\"",
    "agents.enabled=false",
    "tools.update_plan.enabled=false",
    "tools.experimental_request_user_input.enabled=false",
    "skills.include_instructions=false",
    "cloud.skills.enabled=false",
    "mcp_servers={}",
];

/// The startup notice a disabled code-mode host emits as an `error` item:
/// positive evidence that the code-mode `exec` tool fails closed. Any other
/// `error` item still refuses the answer.
pub(super) const CODE_MODE_FAIL_CLOSED: &str =
    "Code Mode is unavailable because code-mode host is disabled.";

/// The `--disable … -c …` arguments, in the order measured.
pub(super) fn args() -> Vec<String> {
    let mut out = Vec::with_capacity(2 * (DISABLED_FEATURES.len() + CONFIG_OVERRIDES.len()));
    for feature in DISABLED_FEATURES {
        out.push("--disable".to_owned());
        out.push((*feature).to_owned());
    }
    for config in CONFIG_OVERRIDES {
        out.push("-c".to_owned());
        out.push((*config).to_owned());
    }
    out
}

/// Only a measured minor carries the profile; any other refuses, naming both.
pub(super) fn admit_version(seen: (u32, u32)) -> Result<(), InferGradeError> {
    if MEASURED.contains(&seen) {
        return Ok(());
    }
    let measured = MEASURED
        .iter()
        .map(|(major, minor)| format!("{major}.{minor}"))
        .collect::<Vec<_>>()
        .join(", ");
    Err(refused(format!(
        "codex-cli {}.{} has no measured pre-execution empty-tools profile (measured on {measured}); \
         tool disabling is not attested for this version, so no prompt is sent; no provider fallback",
        seen.0, seen.1
    )))
}

/// Judge `codex <profile> features list`: every disabled feature must be
/// listed with the effective state `false`.
pub(super) fn judge_features(listing: &str) -> Result<(), InferGradeError> {
    let mut still_on = Vec::new();
    for feature in DISABLED_FEATURES {
        let state = listing.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some(*feature)).then(|| words.last().unwrap_or_default())
        });
        if state != Some("false") {
            still_on.push(format!("{feature}={}", state.unwrap_or("absent")));
        }
    }
    if still_on.is_empty() {
        return Ok(());
    }
    Err(refused(format!(
        "codex pre-execution empty-tools profile not effective: {}; no prompt is sent; no provider fallback",
        still_on.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_disables_every_listed_feature_and_override() {
        let args = args();
        for feature in DISABLED_FEATURES {
            assert!(
                args.windows(2)
                    .any(|w| w[0] == "--disable" && w[1] == *feature),
                "{feature}"
            );
        }
        for key in [
            "web_search=\"disabled\"",
            "agents.enabled=false",
            "mcp_servers={}",
        ] {
            assert!(
                args.windows(2).any(|w| w[0] == "-c" && w[1] == key),
                "{key}"
            );
        }
        assert!(DISABLED_FEATURES.contains(&"shell_tool"));
        assert!(DISABLED_FEATURES.contains(&"code_mode_host"));
    }

    #[test]
    fn only_a_measured_minor_is_admitted() {
        assert!(admit_version((0, 160)).is_ok());
        for unmeasured in [(0, 159), (0, 161), (1, 160)] {
            let witness = admit_version(unmeasured).unwrap_err().to_string();
            assert!(witness.contains("no measured pre-execution"), "{witness}");
            assert!(witness.contains("0.160"), "{witness}");
            assert!(
                witness.contains(&format!("{}.{}", unmeasured.0, unmeasured.1)),
                "{witness}"
            );
        }
    }

    #[test]
    fn the_effective_feature_listing_must_read_false_for_each() {
        let off: String = DISABLED_FEATURES
            .iter()
            .map(|f| format!("{f}    stable    false"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(judge_features(&off).is_ok());
        let on = off.replace(
            "shell_tool    stable    false",
            "shell_tool    stable    true",
        );
        let witness = judge_features(&on).unwrap_err().to_string();
        assert!(witness.contains("shell_tool=true"), "{witness}");
        let missing = off.replace("code_mode_host    stable    false\n", "");
        let witness = judge_features(&missing).unwrap_err().to_string();
        assert!(witness.contains("code_mode_host=absent"), "{witness}");
        assert!(judge_features("Error: Unknown feature flag: x").is_err());
    }
}
