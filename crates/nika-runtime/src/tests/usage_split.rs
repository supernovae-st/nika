// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Q01 · the metered terminal's usage-split receipt.

use super::*;

/// Q01 · the metered call's SPLIT rides the terminal frame beside the
/// `tokens` it always carried. The RECONCILE probe's openai usage
/// (prompt 5015 of which 4992 cached · one completion token) priced at
/// `0.00075285` cold and would price at `0.000378` warm: with one
/// integer on the frame those two are the SAME sight. With the split a
/// reader recomputes the number — and `tokens` keeps its meaning.
#[test]
fn a_metered_terminal_carries_the_usage_split_beside_tokens() {
    let mut usage = nika_kernel::provider::TokenUsage::new(5015, 1);
    usage.cache_read_tokens = Some(4992);
    let split = crate::usage::UsageSplit::of(&usage)
        .served_by(Some("gpt-4o-mini-2024-07-18".to_owned()), None);
    let ran = task::RanTask {
        usage: Some(Box::new(split)),
        decisions: Vec::new(),
        note: "infer · openai/gpt-4o-mini".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        items: None,
        result: task::RunResult::Success {
            value: Value::String("y".to_owned()),
            tokens: Some(1),
            recovered_from: None,
            warning: None,
            child: None,
            cost_usd: Some(0.000_752_85),
            cost_unpriced: None,
            model: Some("openai/gpt-4o-mini".to_owned()),
            access: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle::settle_ran(
        "ask",
        ran,
        None,
        &TRUSTED,
        &[],
        None,
        &mut ok,
        &mut stamper,
        &mut sink,
    );
    let frame = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    let int = |key: &str| {
        frame
            .fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| match &f.value {
                FieldValue::Int(n) => *n,
                other => panic!("{key} is not an int: {other:?}"),
            })
    };
    assert_eq!(int("tokens"), Some(1), "`tokens` keeps its meaning");
    assert_eq!(int("tokens_in"), Some(5015));
    assert_eq!(int("tokens_out"), Some(1));
    assert_eq!(int("tokens_cache_read"), Some(4992));
    assert_eq!(int("tokens_cache_write"), None, "unreported stays absent");
    assert!(
        frame.fields.iter().any(|f| f.key == "model_served"
            && matches!(&f.value, FieldValue::String(s) if s == "gpt-4o-mini-2024-07-18")),
        "the responder names itself"
    );
}
