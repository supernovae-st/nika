// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use crate::admission::{CostHostEvidence, CostReview, CostRoute};
use serde_json::json;
fn legacy() -> Value {
    serde_json::from_str(include_str!("fixture.json")).unwrap()
}
#[test]
fn reduced_legacy_report_keeps_unknown_quote_separate_and_never_creates_numeric_authority() {
    let observation = legacy();
    let report = LegacyCostReport::read(std::slice::from_ref(&observation)).unwrap();
    assert_eq!(report.known.nano_usd, 18_531_392);
    assert_eq!(report.reserved_unknown.nano_usd, 1_400_340_480);
    assert_eq!(report.previous_limit.nano_usd, 15_000_000_000);
    let route = CostRoute {
        provider: "deepseek".into(),
        model: "deepseek-v4-pro".into(),
        endpoint: "https://api.deepseek.com/v1/chat/completions".into(),
    };
    let review = CostReview::new(
        "project-and-record-and-input".into(),
        "fresh-invocation".into(),
        route.clone(),
        CostHostEvidence::unmanaged_interactive_local(),
        Some(Cost::new(10_000_000_000)),
        None,
    )
    .unwrap()
    .for_session()
    .after_legacy(report);
    let text = review.question();
    assert!(text.contains("legacy exposure retained"));
    assert!(text.contains("not a final charge or a proven bound"));
    assert!(text.contains("no guaranteed TOTAL ceiling"));
    assert!(text.contains("only the NEW invocation"));
    let account = review
        .confirm("project-and-record-and-input", &route)
        .unwrap();
    assert!(account.snapshot().unwrap().unknown_cost.is_some());
    assert!(account.amend(Cost::new(50_000_000_000)).is_err());
    assert!(account.checkpoint(b"project").is_err());
    assert_eq!(observation, legacy());
    account.close("invocation completed").unwrap();
    let closed = account.snapshot().unwrap().durable_observation();
    assert!(LegacyCostReport::read(&[observation.clone(), closed.clone()]).is_ok());
    assert!(LegacyCostReport::read(&[observation, closed.clone(), closed]).is_err());
}
#[test]
fn malformed_zero_active_and_new_uncertainty_refuse_without_reconstructing_attempts() {
    let bad: [fn(&mut Value); 11] = [
        |v| v["known_subtotal_nano_usd"] = json!("0"),
        |v| v["limit_nano_usd"] = json!("0"),
        |v| v["limit_nano_usd"] = json!("1"),
        |v| v["state"] = json!("Open"),
        |v| v["attempts"][1]["id"] = json!(0),
        |v| v["attempts"][1]["sent"] = json!(false),
        |v| v["attempts"][1]["reserved_nano_usd"] = json!("-1"),
        |v| v["attempts"][1]["note"] = json!("reserved"),
        |v| v["unknown_calls"] = json!(0),
        |v| v["unbudgeted"] = json!(true),
        |v| v["schema"] = json!("unknown@5"),
    ];
    for change in bad {
        let mut observation = legacy();
        change(&mut observation);
        assert!(LegacyCostReport::read(&[observation]).is_err());
    }
    assert!(LegacyCostReport::read(&[]).is_err());
    assert!(LegacyCostReport::read(&[legacy(), legacy()]).is_err());
    assert!(LegacyCostReport::read(&[legacy(), Value::Null]).is_err());
}
