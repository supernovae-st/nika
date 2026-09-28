// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The durable cost observation (`nika/inference-cost-observation@2`: W9 with
//! amendments A1, A8-A10). Every observation a real account writes projects to
//! origins with nothing withheld and every money, counter, state and identity
//! field as written. Named free text holding endpoint material is withheld by
//! instance pointer; an unknown key is dropped and counted at its parent, never
//! named; a malformed `@2` does not project at all.
use super::*;
use crate::admission::{
    AdmissionState, DeclaredTariff, HardMonetaryCap, InferenceAdmission, TariffUnit,
    UnknownCostChoice, UnknownCostPolicy, observation_consistent, observation_readable,
    observation_route,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferResponse, StopReason, TokenUsage, UsageCompleteness,
};
use nika_types::cost::Cost;
use serde_json::json;
use std::time::Duration;

/// Public synthetic sentinel planted in the configured endpoint path.
const S: &str = "e33durablesentinel91c4e0d7";
const CATALOG: &str = "https://api.deepseek.com/v1/chat/completions";

fn gateway() -> String {
    format!("https://gateway.example/{S}/v1/chat/completions")
}

fn complete(model: &str) -> InferResponse {
    let mut usage = TokenUsage::new(100, 20);
    usage.cache_read_tokens = Some(0);
    let mut r = InferResponse::new(
        vec![ContentBlock::Text { text: "ok".into() }],
        usage,
        StopReason::EndTurn,
    );
    r.usage_completeness = UsageCompleteness::Complete;
    r.gen_ai.response_model = Some(model.into());
    r.request_id = Some("req-e33".into());
    r
}

fn choice_at(endpoint: &str) -> UnknownCostChoice {
    UnknownCostChoice::new(
        "candidate-a".into(),
        "invocation-a".into(),
        "deepseek".into(),
        "deepseek-v4-pro".into(),
        endpoint.into(),
        3,
        8192,
        Duration::from_secs(10),
    )
    .expect("choice")
}

fn choice() -> UnknownCostChoice {
    choice_at(&gateway())
}

fn widened() -> UnknownCostChoice {
    choice()
        .with_max_in_flight(3)
        .expect("within the total")
        .with_authored_retry()
}

fn declared(choice: UnknownCostChoice) -> UnknownCostChoice {
    let tariff = DeclaredTariff::new(
        &choice,
        "gateway-billing".into(),
        "EUR".into(),
        TariffUnit::PerMillionTokens,
        [0.5, 1.0, 0.1],
        "operator price sheet".into(),
        "2026-09".into(),
    )
    .expect("tariff");
    choice.with_declared_tariff(tariff).expect("scope")
}

/// An unknown-cost account bound to its scope.
fn bound(choice: UnknownCostChoice) -> InferenceAdmission {
    let policy = UnknownCostPolicy::new(
        true,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        Some(Cost::zero()),
        Some(Cost::new(10)),
    );
    InferenceAdmission::new_unknown(choice, policy)
        .expect("account")
        .for_scope("candidate-a", "invocation-a")
        .expect("bind")
}

/// An unknown-cost account's observation after one settled and one dropped
/// attempt.
fn unknown_run(choice: UnknownCostChoice) -> Value {
    let account = bound(choice);
    let endpoint = gateway();
    let mut call = account
        .reserve("deepseek", "deepseek-v4-pro", &endpoint, 8192)
        .expect("reservation");
    call.sent().expect("dispatch");
    call.settle(&complete("deepseek-v4-pro")).expect("settles");
    let mut dropped = account
        .reserve("deepseek", "deepseek-v4-pro", &endpoint, 8192)
        .expect("second reservation");
    dropped.sent().expect("dispatch");
    drop(dropped);
    account.snapshot().expect("receipt").observation()
}

/// An authored-retry account's observation after a received 429.
fn answered_run() -> Value {
    let account = bound(widened());
    let endpoint = gateway();
    let mut call = account
        .reserve("deepseek", "deepseek-v4-pro", &endpoint, 8192)
        .expect("reservation");
    call.sent().expect("dispatch");
    call.answered(429, &endpoint);
    drop(call);
    account.snapshot().expect("receipt").observation()
}

/// A catalog account after a settled call, then a dropped one.
fn catalog_run(account: &InferenceAdmission) -> Value {
    let mut call = account
        .reserve("deepseek", "deepseek-v4-pro", CATALOG, 8192)
        .expect("reservation");
    call.sent().expect("dispatch");
    call.settle(&complete("deepseek-v4-pro")).expect("settles");
    let mut dropped = account
        .reserve("deepseek", "deepseek-v4-pro", CATALOG, 8192)
        .expect("second reservation");
    dropped.sent().expect("dispatch");
    drop(dropped);
    account.snapshot().expect("receipt").observation()
}

/// `text` percent-encoded byte by byte, in upper or lower hex (the test's own
/// encoder, independent of the projection's).
fn hex_escaped(text: &str, upper: bool) -> String {
    text.bytes().fold(String::new(), |mut out, byte| {
        let _ = if upper {
            write!(out, "%{byte:02X}")
        } else {
            write!(out, "%{byte:02x}")
        };
        out
    })
}

/// `text` as JSON `\u` escapes, character by character.
fn json_escaped(text: &str) -> String {
    text.chars().fold(String::new(), |mut out, c| {
        let _ = write!(out, "\\u{:04x}", u32::from(c));
        out
    })
}

/// Every form of `secret` a durable text must not hold.
fn leaks_of(value: &Value, secret: &str) -> Vec<String> {
    [
        secret.to_owned(),
        hex_escaped(secret, true),
        hex_escaped(secret, false),
        json_escaped(secret),
    ]
    .into_iter()
    .filter(|form| holds_text(value, form))
    .collect()
}

/// Whether any decoded string or object key of `value` contains `form`. The
/// serialized JSON is never scanned: its escapes (`\\` for `\`) would hide a
/// backslash form from the search.
fn holds_text(value: &Value, form: &str) -> bool {
    match value {
        Value::String(text) => text.contains(form),
        Value::Array(items) => items.iter().any(|item| holds_text(item, form)),
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| key.contains(form) || holds_text(value, form)),
        _ => false,
    }
}

fn leaks(value: &Value) -> Vec<String> {
    leaks_of(value, S)
}

/// The fields W9 copies at an object, compared one by one.
fn assert_copied(written: &Value, durable: &Value, keys: &[&str], at: &str) {
    for key in keys {
        assert_eq!(written.get(key), durable.get(key), "{at}/{key}");
    }
}

/// What a real account's observation keeps at each object: every money,
/// counter, state and identity field, and its engine phrases.
const TOP_KEYS: &[&str] = &[
    "billed_nano_usd",
    "known_subtotal_nano_usd",
    "limit_nano_usd",
    "overridden_defaults",
    "refusal",
    "scoped_to_declared_free",
    "state",
    "unbudgeted",
    "unknown_calls",
];
const CHOICE_KEYS: &[&str] = &[
    "authored_retry",
    "candidate",
    "invocation",
    "max_in_flight",
    "max_output_tokens",
    "max_requests",
    "model",
    "provider",
    "timeout_ms",
];
const UNKNOWN_KEYS: &[&str] = &[
    "currency",
    "estimated_nano_usd",
    "id",
    "native_estimated_nano",
    "note",
    "request_id",
    "response_model",
    "sent",
    "usage",
];
const CATALOG_KEYS: &[&str] = &[
    "as_of",
    "billing_provider",
    "currency",
    "estimated_nano_usd",
    "id",
    "model",
    "note",
    "reserved_nano_usd",
    "sent",
    "source",
    "source_sha256",
    "usage",
];

fn assert_projects_as_written(name: &str, written: &Value) {
    let durable = project_observation(written).expect("an @1 observation projects");
    assert_eq!(
        durable["schema"], "nika/inference-cost-observation@2",
        "{name}"
    );
    assert_eq!(durable["withheld"], json!([]), "{name}: {durable}");
    assert!(leaks(&durable).is_empty(), "{name}: {durable}");
    assert_eq!(
        project_observation(&durable).as_ref(),
        Some(&durable),
        "{name}: identity"
    );
    assert!(observation_readable(&durable), "{name}");
    assert_eq!(
        observation_consistent(&durable),
        observation_consistent(written),
        "{name}: the arithmetic reads the same"
    );
    assert_copied(written, &durable, TOP_KEYS, name);
    let choice = &written["unknown_cost"];
    if choice.is_null() {
        assert_eq!(durable["unknown_cost"], Value::Null, "{name}");
    } else {
        let projected = &durable["unknown_cost"];
        assert_copied(choice, projected, CHOICE_KEYS, name);
        assert_eq!(projected.get("endpoint"), None, "{name}");
        let endpoint = choice["endpoint"].as_str().expect("an endpoint");
        assert_eq!(projected["origin"], json!(route_origin(endpoint)), "{name}");
    }
    let attempts = written["unknown_attempts"]
        .as_array()
        .expect("unknown attempts");
    for (i, attempt) in attempts.iter().enumerate() {
        let projected = &durable["unknown_attempts"][i];
        assert_copied(attempt, projected, UNKNOWN_KEYS, name);
        assert_eq!(
            projected["choice"], durable["unknown_cost"],
            "{name}: the same choice"
        );
    }
    for (i, attempt) in written["attempts"]
        .as_array()
        .expect("attempts")
        .iter()
        .enumerate()
    {
        let projected = &durable["attempts"][i];
        assert_copied(attempt, projected, CATALOG_KEYS, name);
        let endpoint = attempt["endpoint"].as_str().expect("an endpoint");
        assert_eq!(projected["origin"], json!(route_origin(endpoint)), "{name}");
        assert_eq!(projected.get("endpoint"), None, "{name}");
    }
}

#[test]
fn every_observation_a_real_account_writes_projects_with_nothing_withheld() {
    let catalog = InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account");
    let unbudgeted = InferenceAdmission::unbudgeted();
    let closed = InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account");
    closed.close("run ended").expect("close");
    let fresh = InferenceAdmission::new(Cost::zero()).expect("account");
    let observer = InferenceAdmission::observe_declared_free();
    let gateway_accounts = [
        ("unknown", unknown_run(choice())),
        ("widened", unknown_run(widened())),
        ("declared", unknown_run(declared(choice()))),
        ("answered", answered_run()),
    ];
    for (name, written) in &gateway_accounts {
        // The scanner sees the sentinel where the exact form keeps it.
        assert!(!leaks(written).is_empty(), "{name}: {written}");
    }
    for (name, written) in [
        ("fresh", fresh.snapshot().expect("receipt").observation()),
        ("catalog", catalog_run(&catalog)),
        ("unbudgeted", catalog_run(&unbudgeted)),
        ("closed", closed.snapshot().expect("receipt").observation()),
        (
            "observer",
            observer.snapshot().expect("receipt").observation(),
        ),
    ]
    .into_iter()
    .chain(gateway_accounts)
    {
        assert_projects_as_written(name, &written);
    }
    let answered = answered_run();
    assert_eq!(
        answered["unknown_attempts"][0]["note"],
        "answered HTTP 429; usage and USD cost unknown"
    );
    let observer = observer.snapshot().expect("receipt").observation();
    assert_eq!(observer["scoped_to_declared_free"], true);
    assert_eq!(observer["unbudgeted"], true);
}

#[test]
fn a_receipt_writes_its_durable_observation_and_keeps_its_exact_one() {
    let account = InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account");
    catalog_run(&account);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(
        receipt.observation()["schema"],
        "nika/inference-cost-observation@1"
    );
    assert_eq!(
        Some(receipt.durable_observation()),
        project_observation(&receipt.observation())
    );
    assert_eq!(receipt.observation()["attempts"][0]["endpoint"], CATALOG);
}

#[test]
fn widened_and_retried_choices_keep_their_keys_and_a_sequential_one_adds_none() {
    let sequential = unknown_run(choice());
    let durable = project_observation(&sequential).expect("projects");
    assert_eq!(durable["unknown_cost"].get("max_in_flight"), None);
    assert_eq!(durable["unknown_cost"].get("authored_retry"), None);
    let durable = project_observation(&unknown_run(widened())).expect("projects");
    assert_eq!(durable["unknown_cost"]["max_in_flight"], 3);
    assert_eq!(durable["unknown_cost"]["authored_retry"], true);
}

/// The `withheld` entries of a durable object, as `(field, reason, count)`.
fn entries(durable: &Value) -> Vec<(String, String, u64)> {
    durable["withheld"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|w| {
            (
                w["field"].as_str().unwrap_or_default().to_owned(),
                w["reason"].as_str().unwrap_or_default().to_owned(),
                w["count"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn named_free_text_holding_endpoint_material_is_withheld_by_pointer() {
    let written = unknown_run(declared(choice()));
    let tail = format!("/{S}/v1/chat/completions");
    let forms = [
        tail.clone(),
        tail.replace('/', "\\"),
        hex_escaped(&tail, true),
        json_escaped(&tail),
    ];
    for form in forms {
        let mut planted = written.clone();
        let tariff = &mut planted["unknown_cost"]["declared_tariff"];
        tariff["billing_provider"] = json!(format!("biller {form}"));
        tariff["provenance"] = json!(format!("sheet {form}"));
        tariff["version"] = json!(format!("v {form}"));
        planted["unknown_attempts"][0]["request_id"] = json!(format!("req {form}"));
        planted["unknown_attempts"][0]["response_model"] = json!(format!("model {form}"));
        planted["unknown_attempts"][1]["note"] = json!(format!("dropped at {form}"));
        planted["refusal"] = json!(format!("refused at {form}"));
        let durable = project_observation(&planted).expect("projects");
        assert!(leaks(&durable).is_empty(), "{form}: {durable}");
        let tariff = &durable["unknown_cost"]["declared_tariff"];
        for key in ["billing_provider", "provenance", "version"] {
            assert_eq!(tariff[key], Value::Null, "{form} {key}");
        }
        let first = &durable["unknown_attempts"][0];
        assert_eq!(first["request_id"], Value::Null, "{form}");
        assert_eq!(first["response_model"], Value::Null, "{form}");
        assert_eq!(first["note"], written["unknown_attempts"][0]["note"]);
        let second = &durable["unknown_attempts"][1];
        assert_eq!(second["note"], Value::Null, "{form}");
        assert_eq!(
            second["request_id"],
            written["unknown_attempts"][1]["request_id"]
        );
        assert_eq!(durable["refusal"], Value::Null, "{form}");
        let withheld = entries(&durable);
        for field in [
            "/refusal",
            "/unknown_attempts/0/request_id",
            "/unknown_attempts/0/response_model",
            "/unknown_attempts/1/note",
            "/unknown_cost/declared_tariff/billing_provider",
            "/unknown_cost/declared_tariff/provenance",
            "/unknown_cost/declared_tariff/version",
        ] {
            assert!(
                withheld.contains(&(field.to_owned(), "endpoint_material".to_owned(), 0)),
                "{form} {field}: {withheld:?}"
            );
        }
        assert_copied(
            &written,
            &durable,
            &["known_subtotal_nano_usd", "state", "unknown_calls"],
            &form,
        );
    }
}

#[test]
fn an_unknown_key_is_dropped_and_counted_never_named() {
    let written = unknown_run(declared(choice()));
    let key = format!("key_{S}");
    let mut planted = written.clone();
    planted[&key] = json!(1);
    planted["unknown_cost"][&key] = json!(2);
    planted["unknown_cost"]["declared_tariff"][&key] = json!(3);
    planted["unknown_attempts"][0][&key] = json!(4);
    planted["unknown_attempts"][1][&key] = json!(5);
    let durable = project_observation(&planted).expect("projects");
    assert!(leaks(&durable).is_empty(), "{durable}");
    let dropped: Vec<(String, u64)> = entries(&durable)
        .into_iter()
        .filter(|(_, reason, _)| reason == "unrecognized_key")
        .map(|(field, _, count)| (field, count))
        .collect();
    for (field, count) in [
        ("", 1),
        ("/unknown_attempts/0", 1),
        ("/unknown_attempts/1", 1),
        ("/unknown_cost", 1),
        ("/unknown_cost/declared_tariff", 1),
    ] {
        assert!(
            dropped.contains(&(field.to_owned(), count)),
            "{field}: {dropped:?}"
        );
    }
}

#[test]
fn only_a_written_observation_projects() {
    let written = unknown_run(choice());
    let durable = project_observation(&written).expect("@1 projects");
    assert_eq!(
        project_observation(&durable),
        Some(durable.clone()),
        "@2 is itself"
    );
    for other in [
        Value::Null,
        json!([written.clone()]),
        json!("nika/inference-cost-observation@1"),
        json!({"schema": "nika/inference-cost-observation@9"}),
        json!({"state": "Open"}),
    ] {
        assert_eq!(project_observation(&other), None, "{other}");
    }
}

#[test]
fn a_recorded_route_projects_to_its_origin() {
    let endpoint = gateway();
    assert_eq!(project_route(&Value::Null), Value::Null);
    let exact = json!({"provider": "deepseek", "model": "deepseek-v4-pro", "endpoint": endpoint});
    let origin = json!({"provider": "deepseek", "model": "deepseek-v4-pro",
        "origin": "https://gateway.example:443"});
    assert_eq!(project_route(&exact), origin);
    assert_eq!(project_route(&origin), origin, "an origin route is itself");
    let unreadable = json!({"provider": "deepseek", "model": "m", "endpoint": "not a url"});
    assert_eq!(
        project_route(&unreadable),
        json!({"provider": "deepseek", "model": "m", "origin": null})
    );
    assert_eq!(
        observation_route(&project_observation(&unknown_run(choice())).expect("projects")),
        origin,
        "the durable observation's recorded route is its origin"
    );
}

#[test]
fn a_choice_an_attempt_and_a_billing_route_name_their_origin() {
    assert_eq!(
        choice().origin().as_deref(),
        Some("https://gateway.example:443")
    );
    let account = InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account");
    catalog_run(&account);
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(
        receipt.attempts[0].origin().as_deref(),
        Some("https://api.deepseek.com:443")
    );
    let route = crate::retry::BillingRoute::new(
        "deepseek".into(),
        "deepseek-v4-pro".into(),
        CATALOG.into(),
    )
    .expect("route");
    assert_eq!(
        route.origin().as_deref(),
        Some("https://api.deepseek.com:443")
    );
    assert_eq!(receipt.state, AdmissionState::Uncertain, "the dropped call");
}

// ---------------------------------------------------------------------------
// Frozen public fixtures (E35): the `@1` bytes an account writes, and the `@2`
// derived by hand from W9 with amendments A1 and A8-A10, never by running the
// projection. Their sha256 is recorded before the projection is implemented.
// ---------------------------------------------------------------------------

/// What a declared (EUR), widened, authored-retry account writes after one
/// settled and one dropped request to the sentinel gateway.
const FIXTURE_ACCOUNT: &str = r#"{
"schema": "nika/inference-cost-observation@1",
"known_subtotal_nano_usd": "0", "unknown_calls": 2,
"unknown_cost": {"candidate": "candidate-a", "invocation": "invocation-a",
  "provider": "deepseek", "model": "deepseek-v4-pro",
  "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions",
  "max_requests": 3, "max_in_flight": 3, "authored_retry": true, "max_output_tokens": 8192,
  "timeout_ms": 10000,
  "declared_tariff": {"provider": "deepseek", "model": "deepseek-v4-pro",
    "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions",
    "billing_provider": "gateway-billing", "currency": "EUR", "unit": "per_million_tokens",
    "nano_per_token": [500, 1000, 100], "provenance": "operator price sheet",
    "version": "2026-09"}},
"unknown_attempts": [
 {"id": 0,
  "choice": {"candidate": "candidate-a", "invocation": "invocation-a",
    "provider": "deepseek", "model": "deepseek-v4-pro",
    "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions",
    "max_requests": 3, "max_in_flight": 3, "authored_retry": true,
    "max_output_tokens": 8192, "timeout_ms": 10000,
    "declared_tariff": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions",
      "billing_provider": "gateway-billing", "currency": "EUR", "unit": "per_million_tokens",
      "nano_per_token": [500, 1000, 100], "provenance": "operator price sheet",
      "version": "2026-09"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions"},
    "billing_provider": "gateway-billing", "currency": "EUR", "unit": "per_million_tokens",
    "nano_per_token": [500, 1000, 100], "provenance": "operator price sheet",
    "version": "2026-09", "usd_conversion": null},
  "sent": true,
  "usage": {"input_tokens": 100, "output_tokens": 20, "cache_read_tokens": 0,
    "cache_write_tokens": null, "cache_creation_tokens": null, "reasoning_tokens": null,
    "thinking_tokens": null, "audio_input_tokens": null, "audio_output_tokens": null,
    "image_input_tokens": null, "image_output_tokens": null, "video_input_tokens": null,
    "accepted_prediction_tokens": null, "rejected_prediction_tokens": null,
    "total_tokens": null, "search_context_tokens": null, "citation_tokens": null,
    "num_requests": null},
  "estimated_nano_usd": null, "native_estimated_nano": "70000", "currency": "EUR",
  "response_model": "deepseek-v4-pro", "request_id": "req-e33",
  "note": "completed; USD cost unknown"},
 {"id": 1,
  "choice": {"candidate": "candidate-a", "invocation": "invocation-a",
    "provider": "deepseek", "model": "deepseek-v4-pro",
    "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions",
    "max_requests": 3, "max_in_flight": 3, "authored_retry": true,
    "max_output_tokens": 8192, "timeout_ms": 10000,
    "declared_tariff": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions",
      "billing_provider": "gateway-billing", "currency": "EUR", "unit": "per_million_tokens",
      "nano_per_token": [500, 1000, 100], "provenance": "operator price sheet",
      "version": "2026-09"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "endpoint": "https://gateway.example/e33durablesentinel91c4e0d7/v1/chat/completions"},
    "billing_provider": "gateway-billing", "currency": "EUR", "unit": "per_million_tokens",
    "nano_per_token": [500, 1000, 100], "provenance": "operator price sheet",
    "version": "2026-09", "usd_conversion": null},
  "sent": true, "usage": null, "estimated_nano_usd": null, "native_estimated_nano": null,
  "currency": null, "response_model": null, "request_id": null,
  "note": "possibly billed; no automatic retry"}],
"overridden_defaults": ["0", "10"], "limit_nano_usd": null, "billed_nano_usd": null,
"state": "Uncertain",
"refusal": "sent request ended without usable settlement; unknown charge retained",
"attempts": []
}"#;

/// `FIXTURE_ACCOUNT` at `@2`, derived by hand: endpoints become origins;
/// everything else is kept; nothing is withheld.
const EXPECTED_ACCOUNT: &str = r#"{
"schema": "nika/inference-cost-observation@2",
"known_subtotal_nano_usd": "0", "unknown_calls": 2,
"unknown_cost": {"candidate": "candidate-a", "invocation": "invocation-a",
  "provider": "deepseek", "model": "deepseek-v4-pro", "origin": "https://gateway.example:443",
  "max_requests": 3, "max_in_flight": 3, "authored_retry": true, "max_output_tokens": 8192,
  "timeout_ms": 10000,
  "declared_tariff": {"provider": "deepseek", "model": "deepseek-v4-pro",
    "origin": "https://gateway.example:443", "billing_provider": "gateway-billing",
    "currency": "EUR", "unit": "per_million_tokens", "nano_per_token": [500, 1000, 100],
    "provenance": "operator price sheet", "version": "2026-09"}},
"unknown_attempts": [
 {"id": 0,
  "choice": {"candidate": "candidate-a", "invocation": "invocation-a",
    "provider": "deepseek", "model": "deepseek-v4-pro", "origin": "https://gateway.example:443",
    "max_requests": 3, "max_in_flight": 3, "authored_retry": true,
    "max_output_tokens": 8192, "timeout_ms": 10000,
    "declared_tariff": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "origin": "https://gateway.example:443", "billing_provider": "gateway-billing",
      "currency": "EUR", "unit": "per_million_tokens", "nano_per_token": [500, 1000, 100],
      "provenance": "operator price sheet", "version": "2026-09"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "origin": "https://gateway.example:443"},
    "billing_provider": "gateway-billing", "currency": "EUR", "unit": "per_million_tokens",
    "nano_per_token": [500, 1000, 100], "provenance": "operator price sheet",
    "version": "2026-09", "usd_conversion": null, "withheld": []},
  "sent": true,
  "usage": {"input_tokens": 100, "output_tokens": 20, "cache_read_tokens": 0,
    "cache_write_tokens": null, "cache_creation_tokens": null, "reasoning_tokens": null,
    "thinking_tokens": null, "audio_input_tokens": null, "audio_output_tokens": null,
    "image_input_tokens": null, "image_output_tokens": null, "video_input_tokens": null,
    "accepted_prediction_tokens": null, "rejected_prediction_tokens": null,
    "total_tokens": null, "search_context_tokens": null, "citation_tokens": null,
    "num_requests": null},
  "estimated_nano_usd": null, "native_estimated_nano": "70000", "currency": "EUR",
  "response_model": "deepseek-v4-pro", "request_id": "req-e33",
  "note": "completed; USD cost unknown"},
 {"id": 1,
  "choice": {"candidate": "candidate-a", "invocation": "invocation-a",
    "provider": "deepseek", "model": "deepseek-v4-pro", "origin": "https://gateway.example:443",
    "max_requests": 3, "max_in_flight": 3, "authored_retry": true,
    "max_output_tokens": 8192, "timeout_ms": 10000,
    "declared_tariff": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "origin": "https://gateway.example:443", "billing_provider": "gateway-billing",
      "currency": "EUR", "unit": "per_million_tokens", "nano_per_token": [500, 1000, 100],
      "provenance": "operator price sheet", "version": "2026-09"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "deepseek-v4-pro",
      "origin": "https://gateway.example:443"},
    "billing_provider": "gateway-billing", "currency": "EUR", "unit": "per_million_tokens",
    "nano_per_token": [500, 1000, 100], "provenance": "operator price sheet",
    "version": "2026-09", "usd_conversion": null, "withheld": []},
  "sent": true, "usage": null, "estimated_nano_usd": null, "native_estimated_nano": null,
  "currency": null, "response_model": null, "request_id": null,
  "note": "possibly billed; no automatic retry"}],
"overridden_defaults": ["0", "10"], "limit_nano_usd": null, "billed_nano_usd": null,
"state": "Uncertain",
"refusal": "sent request ended without usable settlement; unknown charge retained",
"attempts": [], "withheld": []
}"#;

/// A hand-written adversarial `@1` on the short route `https://gw.example/t9x/v1`:
/// its tail `/t9x/v1` planted raw, percent-encoded (both cases), `\u`-escaped and
/// with `/` written as `\` in bounded free text, the catalog tail
/// `/v1/chat/completions` in a catalog note, and unknown keys whose names hold
/// the path segment at four levels.
const FIXTURE_PLANTED: &str = r#"{
"schema": "nika/inference-cost-observation@1",
"known_subtotal_nano_usd": "123", "unknown_calls": 2, "k_t9x": "top",
"unknown_cost": {"candidate": "c", "invocation": "i", "provider": "deepseek", "model": "m",
  "endpoint": "https://gw.example/t9x/v1", "max_requests": 2, "max_output_tokens": 64,
  "timeout_ms": 1000, "w_t9x": 1,
  "declared_tariff": {"provider": "deepseek", "model": "m",
    "endpoint": "https://gw.example/t9x/v1", "billing_provider": "biller /t9x/v1",
    "currency": "USD", "unit": "per_million_tokens", "nano_per_token": [1, 2, 3],
    "provenance": "sheet", "version": "v1"}},
"unknown_attempts": [
 {"id": 0, "x_t9x": true,
  "choice": {"candidate": "c", "invocation": "i", "provider": "deepseek", "model": "m",
    "endpoint": "https://gw.example/t9x/v1", "max_requests": 2, "max_output_tokens": 64,
    "timeout_ms": 1000,
    "declared_tariff": {"provider": "deepseek", "model": "m",
      "endpoint": "https://gw.example/t9x/v1", "billing_provider": "biller /t9x/v1",
      "currency": "USD", "unit": "per_million_tokens", "nano_per_token": [1, 2, 3],
      "provenance": "sheet", "version": "v1"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "m", "endpoint": "https://gw.example/t9x/v1"},
    "billing_provider": "biller /t9x/v1", "currency": "USD", "unit": "per_million_tokens",
    "nano_per_token": [1, 2, 3], "provenance": "sheet", "version": "v1",
    "usd_conversion": null},
  "sent": true, "usage": {"input_tokens": 7, "output_tokens": 3},
  "estimated_nano_usd": null, "native_estimated_nano": null, "currency": "USD",
  "response_model": "m \\u002f\\u0074\\u0039\\u0078\\u002f\\u0076\\u0031",
  "request_id": "req %2F%74%39%78%2F%76%31", "note": "completed; USD cost unknown"},
 {"id": 1, "y_t9x": 1, "z_t9x": 2,
  "choice": {"candidate": "c", "invocation": "i", "provider": "deepseek", "model": "m",
    "endpoint": "https://gw.example/t9x/v1", "max_requests": 2, "max_output_tokens": 64,
    "timeout_ms": 1000,
    "declared_tariff": {"provider": "deepseek", "model": "m",
      "endpoint": "https://gw.example/t9x/v1", "billing_provider": "biller /t9x/v1",
      "currency": "USD", "unit": "per_million_tokens", "nano_per_token": [1, 2, 3],
      "provenance": "sheet", "version": "v1"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "m", "endpoint": "https://gw.example/t9x/v1"},
    "billing_provider": "biller /t9x/v1", "currency": "USD", "unit": "per_million_tokens",
    "nano_per_token": [1, 2, 3], "provenance": "sheet", "version": "v1",
    "usd_conversion": null},
  "sent": true, "usage": null, "estimated_nano_usd": null, "native_estimated_nano": null,
  "currency": null, "response_model": null, "request_id": "req %2f%74%39%78%2f%76%31",
  "note": "dropped at \\t9x\\v1"}],
"overridden_defaults": [null, "10"], "limit_nano_usd": null, "billed_nano_usd": null,
"state": "Uncertain", "refusal": "cut at /t9x/v1",
"attempts": [{"id": 0, "model": "deepseek-v4-pro",
  "endpoint": "https://api.deepseek.com/v1/chat/completions", "sent": true,
  "estimated_nano_usd": "123", "reserved_nano_usd": "999",
  "usage": {"input_tokens": 1, "output_tokens": 1}, "billing_provider": "deepseek",
  "currency": "USD", "source": "https://api-docs.deepseek.com/quick_start/pricing",
  "as_of": "2026-09-01", "source_sha256": "abc", "note": "sent to /v1/chat/completions"}]
}"#;

/// `FIXTURE_PLANTED` at `@2`, derived by hand: each planted text is null with an
/// instance pointer, each unknown key is dropped and counted at its own object,
/// and a pricing object withholds its own text in its own list.
const EXPECTED_PLANTED: &str = r#"{
"schema": "nika/inference-cost-observation@2",
"known_subtotal_nano_usd": "123", "unknown_calls": 2,
"unknown_cost": {"candidate": "c", "invocation": "i", "provider": "deepseek", "model": "m",
  "origin": "https://gw.example:443", "max_requests": 2, "max_output_tokens": 64,
  "timeout_ms": 1000,
  "declared_tariff": {"provider": "deepseek", "model": "m", "origin": "https://gw.example:443",
    "billing_provider": null, "currency": "USD", "unit": "per_million_tokens",
    "nano_per_token": [1, 2, 3], "provenance": "sheet", "version": "v1"}},
"unknown_attempts": [
 {"id": 0,
  "choice": {"candidate": "c", "invocation": "i", "provider": "deepseek", "model": "m",
    "origin": "https://gw.example:443", "max_requests": 2, "max_output_tokens": 64,
    "timeout_ms": 1000,
    "declared_tariff": {"provider": "deepseek", "model": "m",
      "origin": "https://gw.example:443", "billing_provider": null, "currency": "USD",
      "unit": "per_million_tokens", "nano_per_token": [1, 2, 3], "provenance": "sheet",
      "version": "v1"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "m", "origin": "https://gw.example:443"},
    "billing_provider": null, "currency": "USD", "unit": "per_million_tokens",
    "nano_per_token": [1, 2, 3], "provenance": "sheet", "version": "v1",
    "usd_conversion": null,
    "withheld": [{"field": "/billing_provider", "reason": "endpoint_material"}]},
  "sent": true, "usage": {"input_tokens": 7, "output_tokens": 3},
  "estimated_nano_usd": null, "native_estimated_nano": null, "currency": "USD",
  "response_model": null, "request_id": null, "note": "completed; USD cost unknown"},
 {"id": 1,
  "choice": {"candidate": "c", "invocation": "i", "provider": "deepseek", "model": "m",
    "origin": "https://gw.example:443", "max_requests": 2, "max_output_tokens": 64,
    "timeout_ms": 1000,
    "declared_tariff": {"provider": "deepseek", "model": "m",
      "origin": "https://gw.example:443", "billing_provider": null, "currency": "USD",
      "unit": "per_million_tokens", "nano_per_token": [1, 2, 3], "provenance": "sheet",
      "version": "v1"}},
  "pricing": {"kind": "user_declared_estimate_not_invoice",
    "route": {"provider": "deepseek", "model": "m", "origin": "https://gw.example:443"},
    "billing_provider": null, "currency": "USD", "unit": "per_million_tokens",
    "nano_per_token": [1, 2, 3], "provenance": "sheet", "version": "v1",
    "usd_conversion": null,
    "withheld": [{"field": "/billing_provider", "reason": "endpoint_material"}]},
  "sent": true, "usage": null, "estimated_nano_usd": null, "native_estimated_nano": null,
  "currency": null, "response_model": null, "request_id": null, "note": null}],
"overridden_defaults": [null, "10"], "limit_nano_usd": null, "billed_nano_usd": null,
"state": "Uncertain", "refusal": null,
"attempts": [{"id": 0, "model": "deepseek-v4-pro", "origin": "https://api.deepseek.com:443",
  "sent": true, "estimated_nano_usd": "123", "reserved_nano_usd": "999",
  "usage": {"input_tokens": 1, "output_tokens": 1}, "billing_provider": "deepseek",
  "currency": "USD", "source": "https://api-docs.deepseek.com/quick_start/pricing",
  "as_of": "2026-09-01", "source_sha256": "abc", "note": null}],
"withheld": [
  {"field": "", "reason": "unrecognized_key", "count": 1},
  {"field": "/attempts/0/note", "reason": "endpoint_material"},
  {"field": "/refusal", "reason": "endpoint_material"},
  {"field": "/unknown_attempts/0", "reason": "unrecognized_key", "count": 1},
  {"field": "/unknown_attempts/0/choice/declared_tariff/billing_provider",
    "reason": "endpoint_material"},
  {"field": "/unknown_attempts/0/request_id", "reason": "endpoint_material"},
  {"field": "/unknown_attempts/0/response_model", "reason": "endpoint_material"},
  {"field": "/unknown_attempts/1", "reason": "unrecognized_key", "count": 2},
  {"field": "/unknown_attempts/1/choice/declared_tariff/billing_provider",
    "reason": "endpoint_material"},
  {"field": "/unknown_attempts/1/note", "reason": "endpoint_material"},
  {"field": "/unknown_attempts/1/request_id", "reason": "endpoint_material"},
  {"field": "/unknown_cost", "reason": "unrecognized_key", "count": 1},
  {"field": "/unknown_cost/declared_tariff/billing_provider", "reason": "endpoint_material"}]
}"#;

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("a frozen fixture parses")
}

#[test]
fn the_frozen_account_fixture_is_what_the_serializer_writes() {
    assert_eq!(
        unknown_run(declared(widened())),
        parse(FIXTURE_ACCOUNT),
        "the @1 fixture is the serializer's own output"
    );
}

#[test]
fn the_frozen_fixtures_project_to_their_hand_derived_durable_form() {
    for (name, fixture, expected) in [
        ("account", FIXTURE_ACCOUNT, EXPECTED_ACCOUNT),
        ("planted", FIXTURE_PLANTED, EXPECTED_PLANTED),
    ] {
        let expected = parse(expected);
        assert_eq!(
            project_observation(&parse(fixture)).as_ref(),
            Some(&expected),
            "{name}"
        );
        assert_eq!(
            project_observation(&expected).as_ref(),
            Some(&expected),
            "{name}: a well-formed @2 is itself"
        );
        assert!(observation_readable(&expected), "{name}");
    }
}

#[test]
fn every_planted_form_is_gone_and_the_scanner_saw_it_first() {
    let tail = "/t9x/v1";
    let forms = [
        tail.to_owned(),
        tail.replace('/', "\\"),
        hex_escaped(tail, true),
        hex_escaped(tail, false),
        json_escaped(tail),
        "t9x".to_owned(),
    ];
    let written = parse(FIXTURE_PLANTED);
    let durable = project_observation(&written).expect("projects");
    for form in &forms {
        assert!(holds_text(&written, form), "{form} planted");
        assert!(!holds_text(&durable, form), "{form}: {durable}");
    }
    assert!(!holds_text(&durable, "/v1/chat/completions"));
}

#[test]
fn duplicate_nested_attempts_are_withheld_each_at_its_own_index() {
    let withheld = entries(&parse(EXPECTED_PLANTED));
    let durable = project_observation(&parse(FIXTURE_PLANTED)).expect("projects");
    assert_eq!(entries(&durable), withheld);
    for (field, reason, count) in [
        ("/unknown_attempts/0/request_id", "endpoint_material", 0),
        ("/unknown_attempts/1/request_id", "endpoint_material", 0),
        ("/unknown_attempts/0", "unrecognized_key", 1),
        ("/unknown_attempts/1", "unrecognized_key", 2),
    ] {
        let entry = (field.to_owned(), reason.to_owned(), count);
        assert!(withheld.contains(&entry), "{entry:?}");
    }
    for elided in ["/unknown_attempts/request_id", "/unknown_attempts"] {
        assert!(
            withheld.iter().all(|(field, _, _)| field != elided),
            "an index was elided: {elided}"
        );
    }
}

#[test]
fn a_malformed_durable_observation_projects_to_nothing() {
    type Break = fn(&mut Value);
    let cases: [(&str, Break); 16] = [
        ("an unknown key", |o| {
            o["extra"] = json!(1);
        }),
        ("a leftover endpoint", |o| {
            o["unknown_cost"]["endpoint"] = json!("https://gw.example/t9x/v1");
        }),
        ("a path inside an origin", |o| {
            o["attempts"][0]["origin"] = json!("https://api.deepseek.com:443/v1");
        }),
        ("a pricing route that keeps its endpoint", |o| {
            o["unknown_attempts"][0]["pricing"]["route"]["endpoint"] = json!("x");
        }),
        ("an unknown pricing key", |o| {
            o["unknown_attempts"][1]["pricing"]["extra"] = json!(null);
        }),
        ("a counter written as text", |o| {
            o["unknown_calls"] = json!("2");
        }),
        ("a negative counter", |o| {
            o["unknown_calls"] = json!(-1);
        }),
        ("money written as a number", |o| {
            o["attempts"][0]["reserved_nano_usd"] = json!(999);
        }),
        ("a missing state", |o| {
            o.as_object_mut().map(|fields| fields.remove("state"));
        }),
        ("a kept text listed as withheld", |o| {
            o["refusal"] = json!("kept");
        }),
        ("a withheld field that names text", |o| {
            o["withheld"][2]["field"] = json!("/refusal_t9x");
        }),
        ("an out-of-range index", |o| {
            let entry = json!({"field": "/unknown_attempts/9/note", "reason": "endpoint_material"});
            if let Some(list) = o["withheld"].as_array_mut() {
                list.insert(11, entry);
            }
        }),
        ("an unsorted withheld list", |o| {
            if let Some(list) = o["withheld"].as_array_mut() {
                list.swap(0, 1);
            }
        }),
        ("a duplicate withheld entry", |o| {
            let first = o["withheld"][0].clone();
            if let Some(list) = o["withheld"].as_array_mut() {
                list.insert(0, first);
            }
        }),
        ("a dropped-key entry without its count", |o| {
            o["withheld"][0]
                .as_object_mut()
                .map(|entry| entry.remove("count"));
        }),
        ("a reason outside the vocabulary", |o| {
            o["withheld"][2]["reason"] = json!("because");
        }),
    ];
    let base = parse(EXPECTED_PLANTED);
    assert_eq!(project_observation(&base).as_ref(), Some(&base));
    for (name, break_it) in cases {
        let mut observation = base.clone();
        break_it(&mut observation);
        assert_eq!(project_observation(&observation), None, "{name}");
        assert!(!observation_readable(&observation), "{name}");
    }
}

#[test]
fn money_counters_and_authority_survive_the_projection_exactly() {
    let mut written = unknown_run(declared(choice()));
    written["known_subtotal_nano_usd"] = json!(i128::MAX.to_string());
    written["unknown_calls"] = json!(u64::MAX);
    written["overridden_defaults"] = json!([i128::MIN.to_string(), "0"]);
    written["unknown_attempts"][0]["native_estimated_nano"] = json!(i128::MAX.to_string());
    written["unknown_attempts"][0]["usage"]["input_tokens"] = json!(u64::MAX);
    let durable = project_observation(&written).expect("projects");
    for pointer in [
        "/known_subtotal_nano_usd",
        "/unknown_calls",
        "/overridden_defaults",
        "/state",
        "/refusal",
        "/unknown_cost/candidate",
        "/unknown_cost/invocation",
        "/unknown_cost/max_requests",
        "/unknown_cost/declared_tariff/nano_per_token",
        "/unknown_attempts/0/native_estimated_nano",
        "/unknown_attempts/0/usage",
        "/unknown_attempts/0/pricing/nano_per_token",
        "/unknown_attempts/1/sent",
    ] {
        assert_eq!(
            written.pointer(pointer),
            durable.pointer(pointer),
            "{pointer}"
        );
    }
    assert_eq!(
        observation_consistent(&durable),
        observation_consistent(&written)
    );
    // One origin, two exact routes: the origin is shared, the authority is not.
    let (a, b) = (
        "https://gateway.example/tenant-a/v1",
        "https://gateway.example/tenant-b/v1",
    );
    assert_eq!(choice_at(a).origin(), choice_at(b).origin());
    let account = bound(choice_at(a));
    assert!(
        account
            .reserve("deepseek", "deepseek-v4-pro", b, 8192)
            .is_err(),
        "the other exact route is refused"
    );
    let durable = account.snapshot().expect("receipt").durable_observation();
    assert_eq!(
        durable["unknown_cost"]["origin"],
        "https://gateway.example:443"
    );
    assert_eq!(
        durable["state"], "Open",
        "an observation is never authority"
    );
}
