// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Render the admitted model-scope hints without auditing or selecting
//! models again. The caller owns task scoping and its output-mode policy.

/// The human boot notice for an envelope override with retained model
/// selections. Machine and quiet callers pass `human: false`.
#[must_use]
pub fn notice(
    report: &nika_check::CheckReport,
    model_override: Option<&str>,
    human: bool,
) -> Option<String> {
    let model = model_override.filter(|_| human)?;
    let rows: Vec<_> = report
        .hints
        .iter()
        .filter(|hint| hint.kind == "envelope-model")
        .map(|hint| format!("  {}", hint.advice))
        .collect();
    if rows.is_empty() {
        return None;
    }
    Some(format!(
        "model override: --model `{model}` replaces this workflow's default only\n{}",
        rows.join("\n")
    ))
}

/// The operator-selected decision service's observed configuration, without selecting a model,
/// sending a request or assigning a USD tariff to its usage. The host owns every input.
#[must_use]
pub fn decision_status(word: &str, source: &str, endpoint: Result<(String, u64), &str>) -> String {
    match endpoint {
        Ok((host, seconds)) => format!(
            "decision seat {word} ({source}, operator-selected) · {host} · typed compiler choices (reading, feasible-plan ranking, semantic verification), never knowledge selection · one attempt per finite question, {seconds} s deadline, no retry or default call cap · API and subscription authoring keep this separate observation · cost unknown, never zero"
        ),
        Err(why) => format!("decision seat {word} ({source}) · refused: {why}"),
    }
}

/// The selected subscription's cost notice for the mode supplied by its host.
/// A projection only: it neither changes retained accounts nor admits preparation or Run.
#[must_use]
pub const fn subscription_status(continuous: bool) -> &'static str {
    if continuous {
        "\n  subscription authoring: invoice unknown; preparation continues without a Session cost ceiling; historical charges are kept; Run is reviewed separately; no API fallback"
    } else {
        "\n  subscription authoring: invoice unknown; no API fallback; any retained API allowance is suspended, with its expenses preserved; using that allowance again requires a fresh TOTAL ceiling"
    }
}

#[cfg(test)]
mod tests {
    use super::subscription_status;

    #[test]
    fn subscription_cost_words_follow_the_hosts_preparation_mode() {
        let continuous = subscription_status(true);
        assert!(continuous.contains("invoice unknown"));
        assert!(continuous.contains("historical charges are kept"));
        assert!(continuous.contains("Run is reviewed separately"));
        assert!(continuous.contains("no API fallback"));
        assert!(!continuous.contains("fresh TOTAL"));
        let bounded = subscription_status(false);
        assert!(bounded.contains("allowance is suspended"));
        assert!(bounded.contains("fresh TOTAL ceiling"));
        assert!(!bounded.contains("preparation continues"));
    }
}
