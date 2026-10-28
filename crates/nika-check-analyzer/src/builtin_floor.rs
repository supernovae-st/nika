// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static builtin floor and the unpriced-cloud class: what the catalog
//! says a workflow's priced `invoke:` tasks must spend on their cheapest path,
//! and whether a seat is a third-party cloud route with no price. Descended
//! verbatim from `nika-runtime`'s `admit.rs` (2026-09-28 · B9 phase C): pure
//! reads of the workflow and the catalog, which this plane already judges
//! against, moved so the runtime's 15k wall has room for the dispatch guard
//! that shares them. The runtime's launch gates still own every refusal.

use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};

use crate::media_quote::MediaQuote;

/// A recognized third-party cloud seat with no snapshot row. Unknown
/// providers stay unknown (never promoted to cloud — but NOT spared:
/// the resolved-id walk's unresolvable arm refuses them under a cap
/// through `nika_providers::resolve_refusal`, #1368). Mock and local
/// are the sparing arms — unpriced, never this class.
#[must_use]
pub fn unpriced_cloud_seat(model: &str) -> bool {
    if model == "mock" || model.starts_with("mock/") {
        return false;
    }
    let provider = model.split_once('/').map_or(model, |(p, _)| p);
    let Some(entry) = nika_catalog::find_provider(provider) else {
        return false;
    };
    let local = entry
        .tags
        .iter()
        .any(|tag| matches!(tag, nika_catalog::Tag::Local))
        || entry
            .data_policy
            .is_some_and(|policy| policy.zdr == "local");
    if local {
        return false;
    }
    nika_catalog::find_pricing_for(model).is_none()
}

/// Unavoidable catalog spend of priced `invoke:` tasks (cheapest path:
/// `when:` closed → $0 · first-try · known `n:` · known `for_each`
/// length): each paid media call's floor ([`crate::media_quote()`] · xAI's
/// per-image price, or the dated table's per-image or per-character rate).
/// A provider or model only the run decides, a call priced from usage only
/// and an expression `for_each` stay off this floor — the dispatch guard
/// and the mid-run ledger own what statics cannot see.
#[must_use]
pub fn priced_builtin_floor(wf: &RawWorkflow) -> f64 {
    wf.tasks.iter().map(|t| invoke_static_floor(&t.value)).sum()
}

fn invoke_static_floor(task: &RawTask) -> f64 {
    let RawAction::Invoke(inv) = &task.action else {
        return 0.0;
    };
    let Some(tool) = inv.tool() else {
        return 0.0;
    };
    let Some(args) = inv.args.as_ref() else {
        return 0.0;
    };
    match crate::media_quote(&tool.value, &args.value) {
        #[allow(clippy::cast_precision_loss)] // a literal list length is a task count
        MediaQuote::Priced { floor, .. } => floor.usd().unwrap_or(0.0) * floor_calls(task) as f64,
        _ => 0.0,
    }
}

/// How many times the cheapest static path runs `task`: the multiplier
/// [`priced_builtin_floor`] prices a paid media call by. None under a `when:`
/// gate (closed on that path) or over an expression `for_each` (a count only
/// the run knows, judged by the dispatch guard · NIKA-1704); a literal
/// `for_each`'s length; else one. A renderer that names the floor beside an
/// estimate counts calls the way the launch gate does.
#[must_use]
pub fn floor_calls(task: &RawTask) -> u64 {
    if task.when.is_some() {
        return 0;
    }
    match task.for_each.as_ref().map(|f| &f.value) {
        None => 1,
        Some(ForEachValue::List(arr)) => {
            u64::try_from(arr.as_array().map_or(1, Vec::len)).unwrap_or(u64::MAX)
        }
        // Unknown count: cheapest path cannot claim a floor (NIKA-1704).
        Some(ForEachValue::Expression(_)) => 0,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and runtime ship together"
        )]
        _ => unreachable!("unsupported for_each form"),
    }
}

#[cfg(test)]
mod tests {
    use super::{floor_calls, priced_builtin_floor};
    use nika_schema::{FileId, ParseMode, parse};

    /// The calls the launch floor counts, per task: one plain call, none behind a `when:` gate
    /// or over an expression fan-out, a literal fan-out's length. The floor multiplies each
    /// priced call by exactly that count, so a renderer reading it states the run's figure.
    #[test]
    fn the_floor_counts_a_literal_fan_out_and_never_a_gated_or_unknown_call() {
        let say = "invoke: { tool: \"nika:tts_generate\", args: { model: tts-1, text: \"hello \
                   there\", output_dir: ./out } }";
        let wf = parse(
            &format!(
                "nika: floor\ninputs:\n  go: {{ type: bool }}\n  words: {{ type: {{ array: string }} \
                 }}\npermits: {{ tools: [\"nika:tts_generate\"] }}\ntasks:\n  once:\n    \
                 {say}\n  three:\n    for_each: {{ items: [a, b, c] }}\n    {say}\n  gated:\n    \
                 when: \"${{{{ inputs.go == true }}}}\"\n    {say}\n  unknown:\n    for_each: {{ items: \
                 \"${{{{ inputs.words }}}}\" }}\n    {say}\n"
            ),
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture");
        assert!(
            crate::analyze(&wf).is_ok(),
            "the fixture is a valid workflow"
        );
        let calls: Vec<(&str, u64)> = (wf.tasks.iter())
            .map(|t| (t.value.id.value.as_str(), floor_calls(&t.value)))
            .collect();
        assert_eq!(
            calls,
            [("once", 1), ("three", 3), ("gated", 0), ("unknown", 0)]
        );
        let per_call = priced_builtin_floor(
            &parse(
                &format!(
                    "nika: one\npermits: {{ tools: [\"nika:tts_generate\"] }}\ntasks:\n  once:\n    \
                     {say}\n"
                ),
                FileId::new(0),
                ParseMode::Strict,
            )
            .expect("fixture"),
        );
        assert!(per_call > 0.0, "{per_call}");
        let floor = priced_builtin_floor(&wf);
        assert!(
            (floor - 4.0 * per_call).abs() < 1e-12,
            "one plain call and three fanned ones: {floor} vs {per_call}"
        );
    }
}
