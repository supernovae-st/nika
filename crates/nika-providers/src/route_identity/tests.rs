// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Route identity owner tests. The `url` crate is authoritative: every expected
//! origin below is what it serializes. The E29 frozen table's hand-derived
//! literals are checked row for row; a row the parser contradicted would be an
//! E29 derivation defect, reported beside the original literal, never folded in.
use super::*;
use crate::admission::{DeclaredTariff, TariffUnit, UnknownCostChoice};
use crate::retry::BillingRoute;
use nika_types::cost::Cost;
use nika_types::token_usage::TokenUsage;
use proptest::prelude::*;
use std::time::Duration;

/// Public synthetic sentinel planted in endpoint paths, queries and userinfo.
const S: &str = "e30pathsentinel0123456789abcdef";
const DEEPSEEK: &str = "https://api.deepseek.com/v1/chat/completions";

/// Every form of `sentinel` a durable text must not hold: raw, percent-encoded
/// byte by byte in upper and lower hex, and JSON `\u` escaped.
fn forbidden(sentinel: &str) -> [String; 4] {
    use std::fmt::Write as _;
    let (mut upper, mut lower, mut escaped) = (String::new(), String::new(), String::new());
    for byte in sentinel.bytes() {
        write!(upper, "%{byte:02X}").expect("string write");
        write!(lower, "%{byte:02x}").expect("string write");
    }
    for c in sentinel.chars() {
        write!(escaped, "\\u{:04x}", u32::from(c)).expect("string write");
    }
    [sentinel.to_owned(), upper, lower, escaped]
}

fn assert_clean(text: &str, sentinel: &str) {
    for form in forbidden(sentinel) {
        assert!(!text.contains(&form), "{form} leaked in {text}");
    }
    assert!(!text.contains("\"endpoint\""), "endpoint key in {text}");
    assert!(
        !text.contains("requested_endpoint"),
        "requested_endpoint key in {text}"
    );
}

// (endpoint, route_origin, canonical_endpoint)
const ORIGINS: &[(&str, Option<&str>, bool)] = &[
    // ordinary origins
    (
        "https://api.deepseek.com/v1/chat/completions",
        Some("https://api.deepseek.com:443"),
        true,
    ),
    (
        "http://127.0.0.1:8080/v1/chat/completions",
        Some("http://127.0.0.1:8080"),
        false,
    ),
    (
        "https://api.example.com:8443/v1",
        Some("https://api.example.com:8443"),
        true,
    ),
    ("http://localhost/v1", Some("http://localhost:80"), false),
    // IPv6
    (
        "https://[::1]:8443/v1/chat/completions",
        Some("https://[::1]:8443"),
        true,
    ),
    (
        "https://[::1]/v1/chat/completions",
        Some("https://[::1]:443"),
        true,
    ),
    (
        "https://[0:0:0:0:0:0:0:1]/v1",
        Some("https://[::1]:443"),
        false,
    ),
    // credentials: refused, never stripped
    ("https://user:pw@api.example.com/v1", None, false),
    ("https://user@api.example.com/v1", None, false),
    ("https://:pw@api.example.com/v1", None, false),
    // encoded authority
    ("https://127.0.0.1%2Fsecret/v1", None, false),
    (
        "https://ex%61mple.com/v1",
        Some("https://example.com:443"),
        false,
    ),
    ("https://0x7f.1/v1", Some("https://127.0.0.1:443"), false),
    // backslashes: a path separator of a special scheme
    (
        "https://127.0.0.1:63010\\secret/v1",
        Some("https://127.0.0.1:63010"),
        false,
    ),
    (
        "https://api.example.com\\secret",
        Some("https://api.example.com:443"),
        false,
    ),
    // case, default ports, Unicode
    (
        "https://API.Example.COM/v1",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "HTTPS://api.example.com/v1",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "https://api.example.com:443/v1",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "http://api.example.com:80/v1",
        Some("http://api.example.com:80"),
        false,
    ),
    (
        "https://münchen.example/v1",
        Some("https://xn--mnchen-3ya.example:443"),
        false,
    ),
    // path and query variants
    (
        "https://api.example.com/v1/",
        Some("https://api.example.com:443"),
        true,
    ),
    (
        "https://api.example.com/v1/%65%33%30",
        Some("https://api.example.com:443"),
        true,
    ),
    (
        "https://api.example.com/v1?token=x",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "https://api.example.com/v1?",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "https://api.example.com/v1#frag",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "https://api.example.com/v1 /x",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "https://api.example.com",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "https://api.example.com:0/v1",
        Some("https://api.example.com:0"),
        true,
    ),
    // invalid and opaque
    ("not a url", None, false),
    ("", None, false),
    ("https://", None, false),
    ("mailto:ops@example.com", None, false),
    ("data:text/plain,hi", None, false),
    ("file:///etc/hosts", None, false),
    ("ftp://example.com/x", None, false),
    ("javascript:alert(1)", None, false),
];

#[test]
fn route_origin_and_canonical_follow_the_url_parser() {
    for (endpoint, origin, canonical) in ORIGINS {
        assert_eq!(
            route_origin(endpoint).as_deref(),
            *origin,
            "origin of {endpoint:?}"
        );
        assert_eq!(
            canonical_endpoint(endpoint),
            *canonical,
            "canonical {endpoint:?}"
        );
    }
}

/// The E29 frozen projection table (HQ `endpoint-privacy-e29/projection-table.json`),
/// row for row: (case, endpoint, E29 hand-derived origin, E29 admissible). The
/// `url` crate decides; a row it contradicts is an E29 derivation defect.
const E29_ROWS: &[(&str, &str, Option<&str>, bool)] = &[
    (
        "catalog-default",
        "https://api.deepseek.com/v1/chat/completions",
        Some("https://api.deepseek.com:443"),
        true,
    ),
    (
        "project-path",
        "https://api.scaleway.ai/e29pathsentinel0123456789abcdef/v1/chat/completions",
        Some("https://api.scaleway.ai:443"),
        true,
    ),
    (
        "loopback-path",
        "https://127.0.0.1:63010/v1/e29pathsentinel0123456789abcdef",
        Some("https://127.0.0.1:63010"),
        true,
    ),
    (
        "same-origin-other-path",
        "https://127.0.0.1:63010/v1/other/chat/completions",
        Some("https://127.0.0.1:63010"),
        true,
    ),
    (
        "trailing-slash",
        "https://127.0.0.1:63010/v1/e29pathsentinel0123456789abcdef/",
        Some("https://127.0.0.1:63010"),
        true,
    ),
    (
        "query",
        "https://127.0.0.1:63010/v1/x?token=e29pathsentinel0123456789abcdef",
        Some("https://127.0.0.1:63010"),
        false,
    ),
    (
        "userinfo",
        "https://e29pathsentinel0123456789abcdef:pw@127.0.0.1:63010/v1/x",
        None,
        false,
    ),
    (
        "fragment",
        "https://127.0.0.1:63010/v1/x#e29pathsentinel0123456789abcdef",
        Some("https://127.0.0.1:63010"),
        false,
    ),
    (
        "percent-encoded-path",
        "https://127.0.0.1:63010/v1/%65%32%39pathsentinel",
        Some("https://127.0.0.1:63010"),
        true,
    ),
    (
        "percent-in-authority",
        "https://127.0.0.1%2Fe29pathsentinel0123456789abcdef/v1/x",
        None,
        false,
    ),
    (
        "backslash-path",
        "https://127.0.0.1:63010\\e29pathsentinel0123456789abcdef/v1",
        Some("https://127.0.0.1:63010"),
        false,
    ),
    (
        "backslash-no-port",
        "https://api.example.com\\e29pathsentinel0123456789abcdef",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "uppercase-host",
        "https://API.Example.COM/v1/chat/completions",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "explicit-default-port",
        "https://api.example.com:443/v1/chat/completions",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "ipv6-port",
        "https://[::1]:8443/v1/chat/completions",
        Some("https://[::1]:8443"),
        true,
    ),
    (
        "ipv6-default",
        "https://[::1]/v1/chat/completions",
        Some("https://[::1]:443"),
        true,
    ),
    (
        "idn-host",
        "https://münchen.example/v1/chat/completions",
        Some("https://xn--mnchen-3ya.example:443"),
        false,
    ),
    (
        "port-zero",
        "https://api.example.com:0/v1",
        Some("https://api.example.com:0"),
        true,
    ),
    (
        "http-loopback",
        "http://127.0.0.1:8080/v1/chat/completions",
        Some("http://127.0.0.1:8080"),
        false,
    ),
    (
        "uppercase-scheme",
        "HTTPS://api.example.com/v1",
        Some("https://api.example.com:443"),
        false,
    ),
    (
        "whitespace",
        "https://api.example.com/v1 /x",
        Some("https://api.example.com:443"),
        false,
    ),
];

#[test]
fn the_e29_projection_table_holds_under_the_url_parser() {
    for (case, endpoint, origin, admissible) in E29_ROWS {
        assert_eq!(route_origin(endpoint).as_deref(), *origin, "E29 row {case}");
        assert_eq!(canonical_endpoint(endpoint), *admissible, "E29 row {case}");
        if let Some(origin) = route_origin(endpoint) {
            assert!(
                !origin.contains("pathsentinel"),
                "E29 row {case} leaks {origin}"
            );
        }
    }
}

/// Causal control: the old owner's string split (`CostRoute::origin`, still the
/// public review projection until E31 wires this module) keeps a backslash path
/// inside the origin; the new owner does not. When E31 delegates
/// `CostRoute::origin` to `route_origin`, this control flips with that commit.
#[test]
fn old_owner_string_origin_keeps_a_backslash_path_the_new_owner_drops() {
    let endpoint = format!("https://127.0.0.1:63010\\{S}/v1/chat/completions");
    let old = crate::admission::CostRoute {
        provider: "deepseek".into(),
        model: "deepseek-chat".into(),
        endpoint: endpoint.clone(),
    };
    assert!(
        old.origin().contains(S),
        "old owner control: {}",
        old.origin()
    );
    assert_eq!(
        route_origin(&endpoint).as_deref(),
        Some("https://127.0.0.1:63010")
    );
    assert!(!canonical_endpoint(&endpoint));
}

#[test]
fn route_label_names_the_origin_only() {
    let route = InferenceRoute::new(
        "openai".into(),
        "glm-5.2".into(),
        format!("https://api.scaleway.ai/{S}/v1/chat/completions"),
    );
    assert_eq!(
        route_label(&route),
        "openai/glm-5.2 @ https://api.scaleway.ai:443"
    );
    let other = InferenceRoute::new(
        "openai".into(),
        "glm-5.2".into(),
        "https://api.scaleway.ai/other/v1/chat/completions".into(),
    );
    assert_eq!(
        route_label(&route),
        route_label(&other),
        "same origin: one display key"
    );
    let opaque = InferenceRoute::new("openai".into(), "glm-5.2".into(), "not a url".into());
    assert_eq!(route_label(&opaque), "openai/glm-5.2 @ unknown origin");
}

fn dispatched(endpoint: &str, provider: &str, model: &str) -> InferenceCall {
    let mut call = InferenceCall::new();
    call.requested_endpoint = Some(endpoint.to_owned());
    call.route = Some(InferenceRoute::new(
        provider.into(),
        model.into(),
        endpoint.into(),
    ));
    call.usage = Some(TokenUsage::new(162, 178));
    call.usage_complete = true;
    call.response_model = Some(model.to_owned());
    call.request_id = Some("req-7f3a".to_owned());
    call
}

fn priced_call() -> InferenceCall {
    let mut call = dispatched(DEEPSEEK, "deepseek", "deepseek-v4-pro");
    let route = BillingRoute::new("deepseek".into(), "deepseek-v4-pro".into(), DEEPSEEK.into())
        .expect("safe route");
    call.pricing = Some(route.observation().to_string());
    call.estimated_usd = Some(Cost::new(918_720));
    call
}

fn unknown_call(endpoint: &str) -> InferenceCall {
    let mut call = dispatched(endpoint, "deepseek", "deepseek-chat");
    let route = BillingRoute::new("deepseek".into(), "deepseek-chat".into(), endpoint.into())
        .expect("safe route");
    call.pricing = Some(route.observation().to_string());
    call
}

fn declared_call(endpoint: &str, provenance: &str) -> InferenceCall {
    let choice = UnknownCostChoice::new(
        "candidate-a".into(),
        "invocation-a".into(),
        "openai".into(),
        "glm-5.2".into(),
        endpoint.into(),
        1,
        8192,
        Duration::from_secs(120),
    )
    .expect("exact choice");
    let tariff = DeclaredTariff::new(
        &choice,
        "scaleway".into(),
        "EUR".into(),
        TariffUnit::PerMillionTokens,
        [0.0, 0.0, 0.0],
        provenance.into(),
        "2026-09-28".into(),
    )
    .expect("declared tariff");
    let mut call = dispatched(endpoint, "openai", "glm-5.2");
    call.pricing = Some(tariff.observation().to_string());
    call
}

#[test]
fn a_priced_call_keeps_its_evidence_and_names_origins() {
    let calls = [priced_call()];
    let durable = durable_calls(&calls);
    let call = &durable[0];
    assert_eq!(call["requested_origin"], "https://api.deepseek.com:443");
    assert_eq!(
        call["route"],
        serde_json::json!({"provider": "deepseek", "model": "deepseek-v4-pro",
            "origin": "https://api.deepseek.com:443"})
    );
    assert_eq!(call["pricing"]["kind"], "catalog_estimate_not_invoice");
    assert_eq!(
        call["pricing"]["route"]["origin"],
        "https://api.deepseek.com:443"
    );
    assert!(call["pricing"]["input_rate"].is_number(), "{call}");
    // Public tariff provenance survives: only a string holding the endpoint's own
    // path material is withheld.
    for source in ["source", "route_source", "limits_source"] {
        assert!(
            call["pricing"][source]
                .as_str()
                .is_some_and(|url| url.starts_with("https://api-docs.deepseek.com/")),
            "{source} kept: {call}"
        );
    }
    assert_eq!(call["usage"]["input_tokens"], 162);
    assert_eq!(call["usage_complete"], true);
    assert_eq!(call["request_id"], "req-7f3a");
    assert_eq!(
        call["estimated_usd"],
        serde_json::json!(calls[0].estimated_usd)
    );
    let text = durable.to_string();
    assert_clean(&text, "chat/completions");
    assert_eq!(
        calls[0].pricing.as_deref().map(|p| p.contains(DEEPSEEK)),
        Some(true)
    );
}

#[test]
fn an_unknown_call_on_a_private_path_keeps_no_path_material() {
    for endpoint in [
        format!("https://127.0.0.1:63010/v1/{S}/chat/completions"),
        format!("https://api.scaleway.ai/{S}/v1/chat/completions"),
        format!("https://127.0.0.1:63010/v1/{}", forbidden(S)[1]),
        format!("https://127.0.0.1:63010/v1/{S}/"),
    ] {
        let calls = [unknown_call(&endpoint)];
        let text = durable_calls(&calls).to_string();
        assert_clean(&text, S);
        assert!(text.contains("\"kind\":\"unknown\""), "{text}");
        assert!(text.contains("\"requested_origin\""), "{text}");
        assert!(
            calls[0]
                .pricing
                .as_deref()
                .is_some_and(|p| p.contains(&endpoint))
        );
    }
}

#[test]
fn a_declared_call_projects_its_route_and_withholds_a_path_in_provenance() {
    let endpoint = format!("https://api.scaleway.ai/{S}/v1/chat/completions");
    let calls = [
        declared_call(&endpoint, "https://prices.example/scaleway-2026-09"),
        declared_call(&endpoint, &format!("copied from {endpoint}")),
    ];
    let durable = durable_calls(&calls);
    assert_clean(&durable.to_string(), S);
    assert_eq!(
        durable[0]["pricing"]["kind"],
        "user_declared_estimate_not_invoice"
    );
    assert_eq!(
        durable[0]["pricing"]["route"]["origin"],
        "https://api.scaleway.ai:443"
    );
    assert_eq!(
        durable[0]["pricing"]["provenance"],
        "https://prices.example/scaleway-2026-09"
    );
    assert_eq!(durable[1]["pricing"]["provenance"], serde_json::Value::Null);
    assert_eq!(durable[1]["pricing"]["currency"], "EUR");
}

#[test]
fn a_failed_dispatch_and_unreadable_pricing_never_echo_raw_text() {
    let endpoint = format!("https://127.0.0.1:63010\\{S}/v1");
    let mut failed = InferenceCall::new();
    failed.requested_endpoint = Some(endpoint.clone());
    let mut unreadable = dispatched(
        &format!("https://127.0.0.1:63010/{S}"),
        "deepseek",
        "deepseek-chat",
    );
    unreadable.pricing = Some(format!("route {S} unparsable"));
    let mut echoed = unknown_call(&format!("https://127.0.0.1:63010/v1/{S}"));
    echoed.response_model = Some(format!("model at /v1/{S}"));
    let calls = [failed, unreadable, echoed];
    let durable = durable_calls(&calls);
    assert_clean(&durable.to_string(), S);
    assert_eq!(durable[0]["requested_origin"], "https://127.0.0.1:63010");
    assert_eq!(durable[0]["route"], serde_json::Value::Null);
    assert_eq!(durable[1]["pricing"], serde_json::Value::Null);
    assert_eq!(durable[2]["response_model"], serde_json::Value::Null);
    assert_eq!(durable[2]["pricing"]["kind"], "unknown");
}

#[test]
fn credentials_in_an_endpoint_never_reach_the_projection() {
    let endpoint = format!("https://{S}:{S}@api.example.com/v1/chat/completions");
    let calls = [dispatched(&endpoint, "openai", "gpt-oss-120b")];
    let durable = durable_calls(&calls);
    assert_clean(&durable.to_string(), S);
    assert_eq!(durable[0]["requested_origin"], serde_json::Value::Null);
    assert_eq!(durable[0]["route"]["origin"], serde_json::Value::Null);
}

/// The records themselves are untouched: the existing serde of a call, the
/// exact in-memory identity, keeps its full endpoint byte for byte.
#[test]
fn the_record_serde_is_unchanged_and_keeps_the_exact_endpoint() {
    let mut call = InferenceCall::new();
    call.requested_endpoint = Some("https://gateway.example/v1/chat/completions".into());
    call.route = Some(InferenceRoute::new(
        "deepseek".into(),
        "deepseek-chat".into(),
        "https://gateway.example/v1/chat/completions".into(),
    ));
    let text = serde_json::to_string(&call).expect("record serde");
    assert_eq!(
        text,
        r#"{"requested_endpoint":"https://gateway.example/v1/chat/completions","route":{"provider":"deepseek","model":"deepseek-chat","endpoint":"https://gateway.example/v1/chat/completions"},"usage":null,"usage_complete":false,"response_model":null,"request_id":null,"pricing":null,"estimated_usd":null}"#
    );
    let durable = durable_calls(std::slice::from_ref(&call)).to_string();
    assert_eq!(
        durable,
        r#"[{"estimated_usd":null,"pricing":null,"request_id":null,"requested_origin":"https://gateway.example:443","response_model":null,"route":{"model":"deepseek-chat","origin":"https://gateway.example:443","provider":"deepseek"},"usage":null,"usage_complete":false}]"#
    );
}

#[test]
fn durable_calls_of_nothing_is_an_empty_array() {
    assert_eq!(durable_calls(&[]), serde_json::json!([]));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Whatever the path, query or fragment, an origin is `scheme://host:port` and
    /// holds none of them.
    #[test]
    fn an_origin_never_holds_path_query_or_fragment(
        path in "[a-zA-Z0-9._~!$&'()*+,;=:@%/\\\\-]{0,40}",
        query in proptest::option::of("[a-zA-Z0-9=&%]{1,12}"),
    ) {
        let endpoint = match &query {
            Some(q) => format!("https://api.example.com:8443/{path}?{q}"),
            None => format!("https://api.example.com:8443/{path}"),
        };
        if let Some(origin) = route_origin(&endpoint) {
            prop_assert_eq!(origin, "https://api.example.com:8443");
        }
    }

    /// A sentinel planted anywhere in an endpoint's path never reaches the durable
    /// text, raw, percent-encoded or escaped.
    #[test]
    fn a_planted_path_sentinel_never_reaches_durable_text(
        prefix in "[a-z0-9/]{0,12}",
        suffix in "[a-z0-9/]{0,12}",
        backslash in any::<bool>(),
    ) {
        let separator = if backslash { "\\" } else { "/" };
        let endpoint = format!("https://127.0.0.1:63010{separator}{prefix}{S}{suffix}");
        let calls = [unknown_call(&endpoint)];
        let text = durable_calls(&calls).to_string();
        for form in forbidden(S) {
            prop_assert!(!text.contains(&form));
        }
    }
}
