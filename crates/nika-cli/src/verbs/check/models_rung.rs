// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MODELS rung of the check ladder (#320) + the pricing preflight
//! (#213) — the verb's door onto the judges, which live in
//! [`nika_cli_host::models_rung`] (One Door · wave 2b: the MCP oracle
//! folds the same layered verdicts and reaches the host, never this
//! crate). The finding TYPE lives beside its renderer
//! (`nika_display::check_render` · the 15k descent).

pub(crate) use nika_cli_host::models_rung::{
    access_decisions, boot_access_fields, capacity_findings, thinking_findings, unresolvable_models,
};
pub(crate) use nika_display::check_render::VerdictLayers;
#[cfg(test)]
use nika_schema::raw::RawWorkflow;

/// The `check --json` rows — the ONE lane-row shape
/// ([`nika_service_execution::access::lane_rows`]) every machine surface
/// carries, resolved under `pin` (`check --access`). The verb reads the
/// rows off the plan it already resolved; this door serves the tests.
#[cfg(test)]
pub(super) fn access_plan_rows(
    wf: &RawWorkflow,
    report: &nika_check::CheckReport,
    pin: Option<&str>,
) -> Vec<serde_json::Value> {
    nika_service_execution::access::lane_rows(&nika_cli_host::access::resolve_plan(
        wf, report, None, pin,
    ))
}

#[cfg(test)]
mod tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn infer_wf(model: &str, max_tokens: &str, thinking: &str) -> String {
        format!(
            "nika: w\ntasks:\n  t:\n    infer:\n      prompt: hi\n      model: {model}\n      \
             max_tokens: {max_tokens}\n      thinking: {thinking}\n"
        )
    }

    /// The wiring pin: the judgment must reach the VERDICT — a finding
    /// computed but never folded into `clean` is the false-green class
    /// this arc exists to close. Drives the real `check` verb end to
    /// end; deleting the `findings.extend(thinking_findings(..))` fold
    /// turns this red while the judgment's own tests (nika-check's
    /// `thinking` module) stay green.
    #[test]
    fn a_thinking_finding_turns_the_check_red() {
        let dir =
            std::env::temp_dir().join(format!("nika-cli-thinking-rung-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let theme = crate::Theme::new(false, true, false);

        let bad = dir.join("thinking-budget-at-cap.nika.yaml");
        std::fs::write(
            &bad,
            infer_wf("mock/echo", "100", "{ enabled: true, budget_tokens: 100 }"),
        )
        .expect("fixture");
        let out = crate::verbs::check::run(bad.to_str().expect("utf8"), false, false, None, theme);
        assert_eq!(
            out.code, 2,
            "the judgment reaches the verdict: {}",
            out.text
        );
        assert!(
            out.text.contains("MODELS") && out.text.contains("budget_tokens"),
            "the finding row renders under the rung: {}",
            out.text
        );

        // Control: the legal twin stays green through the same verb.
        let ok_path = dir.join("thinking-budget-under-cap.nika.yaml");
        std::fs::write(
            &ok_path,
            infer_wf("mock/echo", "100", "{ enabled: true, budget_tokens: 50 }"),
        )
        .expect("fixture");
        let ok =
            crate::verbs::check::run(ok_path.to_str().expect("utf8"), false, false, None, theme);
        assert_eq!(ok.code, 0, "the legal twin stays green: {}", ok.text);
    }

    /// The wrapper maps the check crate's rows into this rung's finding
    /// shape — the model seat, the ONE task, the why, and never a
    /// conjured spec code.
    #[test]
    fn the_wrapper_maps_thinking_findings_into_model_findings() {
        let wf = parse(
            infer_wf("mock/echo", "100", "{ enabled: true, budget_tokens: 100 }").as_str(),
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let rows = super::thinking_findings(&wf);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].tasks, vec!["t"], "{rows:?}");
        assert!(rows[0].why.contains("budget_tokens"), "{rows:?}");
        assert!(rows[0].code.is_none(), "engine-local, no conjured code");
    }
}
