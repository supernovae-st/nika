// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Route identity: the one owner projection of a provider endpoint
//! (`nika/route-identity@1`, E29 design).
//!
//! A provider route is host-private beyond its origin. A durable record or a
//! network document names it `{provider, model, origin}`; the exact endpoint
//! (path, query, userinfo, fragment) stays in process memory, in a host's IPC
//! with its own lane and in private witness preimages. The origin comes from the
//! WHATWG parse the transport itself connects with (the `url` crate, the law of
//! `nika_types::net`): a hand-rolled string split disagrees with it on `\`,
//! userinfo, percent-escapes and case.
//!
//! These are additive primitives. Pricing, consent and journal judgment keep the
//! exact in-memory endpoint, and no admission, review, retry, wire, journal,
//! runtime or session path calls this module yet. An origin groups every route of
//! one origin for display only: it never admits, prices or consents.

use nika_types::cost::{InferenceCall, InferenceRoute};
use serde_json::Value;

/// A label's origin when the endpoint has none to project.
const UNKNOWN_ORIGIN: &str = "unknown origin";

/// `scheme://host:port` of an endpoint, with the effective port written out
/// (443 for `https`, 80 for `http`), from `url::Url::parse`.
///
/// `None` when the endpoint does not parse, its scheme is neither `https` nor
/// `http`, or it carries userinfo: a credential is refused, never stripped.
/// Hosts are the parser's own serialization (lowercase, IDNA punycode, bracketed
/// IPv6, normalized IPv4), so the origin is where the request actually goes.
#[must_use]
pub fn route_origin(endpoint: &str) -> Option<String> {
    let url = url::Url::parse(endpoint).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?;
    let port = url.port_or_known_default()?;
    Some(format!("{}://{host}:{port}", url.scheme()))
}

/// Whether an endpoint is canonical enough to be an exact unknown-cost identity:
/// `https`, a host, no userinfo, no query, no fragment, and byte-identical to its
/// own `url::Url` serialization.
///
/// A raw form the parser would rewrite (upper case, an explicit default port, a
/// Unicode host, a backslash, a space) is refused, never normalized: its bytes
/// would name one identity while the transport dispatched another.
#[must_use]
pub fn canonical_endpoint(endpoint: &str) -> bool {
    url::Url::parse(endpoint).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.as_str() == endpoint
    })
}

/// `{provider}/{model} @ {origin}`: a route's display and aggregation key.
///
/// Two endpoints of one origin share a label. The label is presentation only:
/// accounting keeps each call's own record, and nothing admits, prices or
/// consents by it.
#[must_use]
pub fn route_label(route: &InferenceRoute) -> String {
    let origin = route_origin(&route.endpoint);
    format!(
        "{}/{} @ {}",
        route.provider,
        route.model,
        origin.as_deref().unwrap_or(UNKNOWN_ORIGIN)
    )
}

/// The durable projection of per-dispatch call records: one JSON object per call
/// with explicit origin keys.
///
/// Each object carries `requested_origin`, `route` as `{provider, model,
/// origin}`, the unchanged `usage`, `usage_complete`, `response_model`,
/// `request_id` and `estimated_usd`, and `pricing` as the parsed provenance
/// object whose every `endpoint` became `origin`. Pricing text that does not
/// parse becomes `null`, never raw text. Any remaining string that still holds
/// one of the call's endpoint paths, queries or userinfos becomes `null`.
///
/// The fields are an allowlist: a field later added to `InferenceCall` stays out
/// until this owner projects it. The records themselves are unchanged and keep
/// their exact endpoints for pricing and identity.
#[must_use]
pub fn durable_calls(calls: &[InferenceCall]) -> Value {
    Value::Array(calls.iter().map(durable_call).collect())
}

fn durable_call(call: &InferenceCall) -> Value {
    let mut private = Vec::new();
    for endpoint in call
        .requested_endpoint
        .iter()
        .chain(call.route.as_ref().map(|route| &route.endpoint))
    {
        private_material(endpoint, &mut private);
    }
    let pricing = call
        .pricing
        .as_deref()
        .map(|raw| durable_pricing(raw, &mut private));
    let mut durable = serde_json::json!({
        "requested_origin": call.requested_endpoint.as_deref().and_then(route_origin),
        "route": call.route.as_ref().map(|route| serde_json::json!({
            "provider": route.provider,
            "model": route.model,
            "origin": route_origin(&route.endpoint),
        })),
        "usage": call.usage,
        "usage_complete": call.usage_complete,
        "response_model": call.response_model,
        "request_id": call.request_id,
        "pricing": pricing,
        "estimated_usd": call.estimated_usd,
    });
    withhold(&mut durable, &private);
    durable
}

/// Parsed pricing provenance with every endpoint key projected to its origin.
fn durable_pricing(raw: &str, private: &mut Vec<String>) -> Value {
    let Ok(mut pricing) = serde_json::from_str::<Value>(raw) else {
        return Value::Null;
    };
    project_endpoints(&mut pricing, private);
    pricing
}

fn project_endpoints(value: &mut Value, private: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, projected) in [
                ("endpoint", "origin"),
                ("requested_endpoint", "requested_origin"),
            ] {
                if let Some(endpoint) = map.remove(key) {
                    let origin = endpoint.as_str().and_then(|endpoint| {
                        private_material(endpoint, private);
                        route_origin(endpoint)
                    });
                    map.insert(projected.into(), origin.map_or(Value::Null, Value::String));
                }
            }
            for child in map.values_mut() {
                project_endpoints(child, private);
            }
        }
        Value::Array(items) => {
            for item in items {
                project_endpoints(item, private);
            }
        }
        _ => {}
    }
}

/// The host-private parts of an endpoint: what follows its authority, both as
/// written and as parsed, and any userinfo. A part without a letter or digit
/// names nothing (a lone `/` would match every URL) and is skipped.
fn private_material(endpoint: &str, private: &mut Vec<String>) {
    let mut add = |part: &str| {
        if part.chars().any(char::is_alphanumeric) && !private.iter().any(|known| known == part) {
            private.push(part.to_owned());
        }
    };
    if let Some((_, rest)) = endpoint.split_once("://") {
        let end = rest.find(['/', '\\', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        add(tail);
        if let Some((userinfo, _)) = authority.rsplit_once('@') {
            add(userinfo);
        }
    }
    if let Ok(url) = url::Url::parse(endpoint) {
        let parsed = &url[url::Position::BeforePath..];
        add(parsed);
        add(url.username());
        add(url.password().unwrap_or_default());
    }
}

/// Null every string that holds private material, and drop every key that does.
fn withhold(value: &mut Value, private: &[String]) {
    match value {
        Value::String(text) if private.iter().any(|part| text.contains(part.as_str())) => {
            *value = Value::Null;
        }
        Value::Object(map) => {
            map.retain(|key, _| !private.iter().any(|part| key.contains(part.as_str())));
            for child in map.values_mut() {
                withhold(child, private);
            }
        }
        Value::Array(items) => {
            for item in items {
                withhold(item, private);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
