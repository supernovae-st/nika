// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_types::access::{AccessClass, AccessPlan, BillingClass};

use super::*;

fn lane() -> AccessPlan {
    AccessPlan::new(
        "openai/gpt-6-astra",
        "openai",
        "codex",
        AccessClass::Harness,
        BillingClass::Unknown,
        true,
        Vec::new(),
    )
}

fn keys(fields: &[(&'static str, FieldValue)]) -> Vec<&'static str> {
    fields.iter().map(|(k, _)| *k).collect()
}

fn text<'a>(fields: &'a [(&'static str, FieldValue)], key: &str) -> &'a str {
    match fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v) {
        Some(FieldValue::String(s)) => s,
        other => panic!("{key}: {other:?}"),
    }
}

/// A lane without a requirement stamps exactly the historical five keys,
/// in their historical order: the projection moved, the frame did not.
#[test]
fn a_lane_without_a_requirement_keeps_the_historical_frame() {
    let mut fields = Vec::new();
    push_access_fields(&mut fields, None, Some(&lane()), None);
    assert_eq!(
        keys(&fields),
        ["model", "provider", "access", "access_id", "billing"]
    );
    assert_eq!(text(&fields, "model"), "openai/gpt-6-astra");
    assert_eq!(text(&fields, "access"), "harness");
    assert_eq!(text(&fields, "billing"), "unknown");
}

#[test]
fn a_planless_model_still_derives_from_its_prefix() {
    let mut fields = Vec::new();
    push_access_fields(&mut fields, Some("mock/echo"), None, None);
    assert_eq!(keys(&fields), ["model", "provider", "access", "billing"]);
    assert_eq!(text(&fields, "access"), "mock");
}

#[test]
fn a_refused_seat_names_what_to_pin() {
    let refused =
        nika_types::access::AccessRefused::new("codex", "exited 1").with_next_ready("openai");
    let mut fields = Vec::new();
    push_access_refused_field(&mut fields, Some(&refused));
    let json: serde_json::Value =
        serde_json::from_str(text(&fields, "access_refused")).expect("json");
    assert_eq!(
        json,
        serde_json::json!({"seat":"codex","witness":"exited 1","next_ready":"openai",
            "pin":"--access openai"})
    );
    let mut none = Vec::new();
    push_access_refused_field(&mut none, None);
    assert!(none.is_empty());
}
