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
use serde_json::Value;

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
/// length). Templated provider/`n` and expression `for_each` stay off
/// this floor — the mid-run ledger still owns what statics cannot see.
#[must_use]
pub fn priced_builtin_floor(wf: &RawWorkflow) -> f64 {
    wf.tasks.iter().map(|t| invoke_static_floor(&t.value)).sum()
}

fn invoke_static_floor(task: &RawTask) -> f64 {
    if task.when.is_some() {
        return 0.0;
    }
    let RawAction::Invoke(inv) = &task.action else {
        return 0.0;
    };
    let Some(tool) = inv.tool() else {
        return 0.0;
    };
    let Some(args) = inv.args.as_ref() else {
        return 0.0;
    };
    let Some(provider) = media_call_provider(&tool.value, &args.value) else {
        return 0.0;
    };
    let Some(per) = nika_catalog::builtin_provider_floor_usd(&tool.value, provider) else {
        return 0.0;
    };
    per * static_n(&args.value) * static_iterations(task)
}

/// The provider a paid-media builtin call (`nika:image_generate` ·
/// `nika:tts_generate`) bills, read from its arguments the way the builtin
/// reads them ([`nika_catalog::builtin_media_provider`]): the explicit
/// `provider:`, else the one its `model:` prefix names. `None` for any other
/// tool, when the arguments name no provider, or when the provider (or the
/// model it would be inferred from) is a template: only the run decides it.
/// The same law judges a literal at check and a rendered call at dispatch.
#[must_use]
pub fn media_call_provider<'a>(tool: &str, args: &'a Value) -> Option<&'a str> {
    let provider = args.get("provider").and_then(Value::as_str);
    let model = args.get("model").and_then(Value::as_str);
    let templated = match provider {
        Some(provider) => is_template(provider),
        None => model.is_some_and(is_template),
    };
    if templated {
        return None;
    }
    nika_catalog::builtin_media_provider(tool, provider, model)
}

/// The template a paid-media builtin call's provider is decided by, when
/// only the run can resolve it: a templated `provider:`, or a templated
/// `model:` the provider would be inferred from. `None` for a literal
/// provider ([`media_call_provider`]), for arguments naming none, and for
/// any other tool. The check lists such a call as decided at run; the run
/// judges it before its request.
#[must_use]
pub fn media_call_template<'a>(tool: &str, args: &'a Value) -> Option<&'a str> {
    let decider = match args.get("provider").and_then(Value::as_str) {
        Some(provider) => provider,
        None => args.get("model").and_then(Value::as_str)?,
    };
    // A named provider makes the catalog answer for a paid-media builtin only.
    let paid_media = nika_catalog::builtin_media_provider(tool, Some(decider), None).is_some();
    (paid_media && is_template(decider)).then_some(decider)
}

/// The paid-media call an armed cap must refuse before its request (C-D3),
/// judged on arguments that are already rendered: the provider it bills,
/// paired with `None` when the catalog cannot price that provider (no cap
/// can bound it), or with the call's per-call floor (× its `n:`) when that
/// floor exceeds `remaining_usd`. `None` when the call is not paid media,
/// is unmetered, or fits. The runtime's dispatch guard and the `agent:`
/// loop's tool calls speak their refusals from this one verdict.
#[must_use]
pub fn media_cap_refusal<'a>(
    tool: &str,
    args: &'a Value,
    remaining_usd: f64,
) -> Option<(&'a str, Option<f64>)> {
    let provider = media_call_provider(tool, args)?;
    match nika_catalog::builtin_media_price(tool, provider) {
        nika_catalog::MediaPrice::Unknown => Some((provider, None)),
        nika_catalog::MediaPrice::PerCall(per) => {
            let floor = per * static_n(args);
            (floor > remaining_usd).then_some((provider, Some(floor)))
        }
        _ => None,
    }
}

/// Why a returned paid-media call that reported no usable `cost_usd` is
/// absent from the spend (C-D3): the catalog has no price for the provider
/// it billed (`missing_catalog_price`), or a priced provider did not report
/// what it billed (`provider_did_not_report_usage`). `None` for local, mock
/// and keyless media and for any other tool. `value` is the call's output or
/// its arguments: both name the provider the way the builtin resolved it.
#[must_use]
pub fn media_spend_unpriced(tool: &str, value: &Value) -> Option<nika_types::cost::UnpricedReason> {
    let provider = media_call_provider(tool, value)?;
    match nika_catalog::builtin_media_price(tool, provider) {
        nika_catalog::MediaPrice::Unknown => {
            Some(nika_types::cost::UnpricedReason::MissingCatalogPrice)
        }
        nika_catalog::MediaPrice::PerCall(_) => {
            Some(nika_types::cost::UnpricedReason::ProviderDidNotReportUsage)
        }
        _ => None,
    }
}

fn is_template(s: &str) -> bool {
    s.contains("${{")
}

fn static_n(args: &Value) -> f64 {
    #[allow(clippy::cast_precision_loss)] // image `n:` is capped at 10
    args.get("n")
        .and_then(Value::as_u64)
        .map_or(1.0, |n| n.max(1) as f64)
}

fn static_iterations(task: &RawTask) -> f64 {
    match task.for_each.as_ref().map(|f| &f.value) {
        None => 1.0,
        Some(ForEachValue::List(arr)) => {
            #[allow(clippy::cast_precision_loss)] // literal list length is a task count
            {
                arr.as_array().map_or(1, Vec::len) as f64
            }
        }
        // Unknown count: cheapest path cannot claim a floor (NIKA-1704).
        Some(ForEachValue::Expression(_)) => 0.0,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and runtime ship together"
        )]
        _ => unreachable!("unsupported for_each form"),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use serde_json::json;

    use super::{media_call_template, media_cap_refusal, media_spend_unpriced};
    use nika_types::cost::UnpricedReason;

    #[test]
    fn a_templated_paid_media_provider_is_decided_at_run() {
        let image = "nika:image_generate";
        let args = json!({ "provider": "${{ item }}", "prompt": "a star" });
        assert_eq!(media_call_template(image, &args), Some("${{ item }}"));
        let args = json!({ "model": "${{ inputs.model }}", "text": "hi" });
        assert_eq!(
            media_call_template("nika:tts_generate", &args),
            Some("${{ inputs.model }}")
        );
        for args in [
            json!({ "provider": "openai", "prompt": "a star" }),
            json!({ "model": "gpt-image-1", "prompt": "a star" }),
            json!({ "prompt": "a star" }),
        ] {
            assert_eq!(media_call_template(image, &args), None, "{args}");
        }
        let args = json!({ "provider": "${{ item }}" });
        assert_eq!(
            media_call_template("nika:image_fx", &args),
            None,
            "not a paid-media builtin"
        );
    }

    #[test]
    fn the_cap_refuses_unpriced_media_and_a_priced_floor_above_what_is_left() {
        let image = "nika:image_generate";
        let openai = json!({ "provider": "openai", "prompt": "a star" });
        assert_eq!(
            media_cap_refusal(image, &openai, 100.0),
            Some(("openai", None))
        );
        let speech = json!({ "model": "eleven_multilingual_v2", "text": "hi" });
        assert_eq!(
            media_cap_refusal("nika:tts_generate", &speech, 100.0),
            Some(("elevenlabs", None))
        );
        let xai = json!({ "provider": "xai", "prompt": "a star", "n": 3 });
        assert_eq!(media_cap_refusal(image, &xai, 1.0), None, "$0.06 fits $1");
        let refused = media_cap_refusal(image, &xai, 0.05).expect("$0.06 exceeds $0.05");
        assert_eq!(refused.0, "xai");
        let floor = refused.1.expect("a priced floor");
        assert!((floor - 0.06).abs() < 1e-12, "{floor}");
        for args in [
            json!({ "provider": "mock", "prompt": "a star" }),
            json!({ "provider": "local", "prompt": "a star" }),
        ] {
            assert_eq!(media_cap_refusal(image, &args, 0.0), None, "{args}");
        }
    }

    #[test]
    fn returned_paid_media_without_spend_names_why_it_is_unpriced() {
        let image = "nika:image_generate";
        assert_eq!(
            media_spend_unpriced(image, &json!({ "provider": "gemini" })),
            Some(UnpricedReason::MissingCatalogPrice)
        );
        assert_eq!(
            media_spend_unpriced(image, &json!({ "provider": "xai" })),
            Some(UnpricedReason::ProviderDidNotReportUsage)
        );
        assert_eq!(
            media_spend_unpriced("nika:tts_generate", &json!({ "provider": "mock" })),
            None
        );
        assert_eq!(
            media_spend_unpriced("nika:fetch", &json!({ "provider": "openai" })),
            None
        );
    }
}
