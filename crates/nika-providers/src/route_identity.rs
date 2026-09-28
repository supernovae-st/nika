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
//! The runtime's trace writers call [`durable_calls`], [`durable_pricing`] and
//! [`route_label`]. Pricing, consent and journal judgment keep the exact
//! in-memory endpoint, and no admission, review, retry, wire, journal or session
//! path calls this module yet. An origin groups every route of one origin for
//! display only: it never admits, prices or consents.
//!
//! The durable projections follow a closed schema. Identity fields become
//! origins. Money, counters, states, catalog constants and the selected provider
//! and model are copied as their producer wrote them and never read. Only named
//! free text is judged (a declared tariff's `billing_provider`, `provenance` and
//! `version`, a call's `request_id` and `response_model`): one holding endpoint
//! material becomes `null` with a `withheld` entry. A `withheld` entry names a
//! field of this schema, never input text, so what it withholds cannot be echoed
//! by its own diagnostic.

use std::borrow::Cow;
use std::fmt::Write as _;

use nika_types::cost::{InferenceCall, InferenceRoute};
use serde_json::{Map, Value};

/// A label's origin when the endpoint has none to project.
const UNKNOWN_ORIGIN: &str = "unknown origin";

/// The table schema `BillingRoute::observation` writes, with or without a tariff.
const TARIFF_SCHEMA: &str = "nika/inference-admission@1.1";
/// What a catalog tariff or `unknown` pricing keeps as written: rates, states and
/// catalog constants.
const TARIFF_KEYS: &[&str] = &[
    "as_of",
    "billing_provider",
    "cached_rate",
    "currency",
    "input_rate",
    "kind",
    "limits_source",
    "output_rate",
    "route_source",
    "source",
    "source_sha256",
    "table_schema",
    "unit",
    "usd_conversion",
];
/// What a vendored snapshot estimate keeps as written: its table pins.
const SNAPSHOT_KEYS: &[&str] = &[
    "as_of",
    "currency",
    "kind",
    "source",
    "source_sha256_16",
    "table_schema",
];
/// What an operator-declared tariff keeps as written.
const DECLARED_KEYS: &[&str] = &[
    "currency",
    "kind",
    "nano_per_token",
    "unit",
    "usd_conversion",
];
/// An operator-declared tariff's free text, judged for endpoint material, with the
/// pointer its `withheld` entry names.
const DECLARED_TEXT: &[(&str, &str)] = &[
    ("billing_provider", "/billing_provider"),
    ("provenance", "/provenance"),
    ("version", "/version"),
];
/// A pricing route's own keys.
const ROUTE_KEYS: [&str; 3] = ["endpoint", "model", "provider"];

/// Why a durable object withholds a field: a closed vocabulary.
const ENDPOINT_MATERIAL: &str = "endpoint_material";
const UNREADABLE: &str = "unreadable";
const UNRECOGNIZED_KIND: &str = "unrecognized_kind";
const UNRECOGNIZED_KEY: &str = "unrecognized_key";

/// A pricing kind: the keys it keeps as written, and its judged free text.
type Shape = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
);

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

/// The durable projection of per-dispatch call records: one JSON object per call.
///
/// Each object has exactly these keys: `requested_origin`; `route` as
/// `{provider, model, origin}`; `usage`, `usage_complete` and `estimated_usd` as
/// recorded; `estimate_known`, whether [`InferenceCall::known_estimate`] holds
/// (the ledger's own verdict: a known call is debited under its [`route_label`],
/// any other is counted unpriced, never as zero); `request_id` and
/// `response_model`, judged as free text; `pricing`, the [`durable_pricing`] of
/// its provenance; and `withheld`, which names each field withheld and why.
///
/// A field later added to `InferenceCall` stays out until this owner projects
/// it. The records themselves are unchanged and keep their exact endpoints for
/// pricing and identity.
#[must_use]
pub fn durable_calls(calls: &[InferenceCall]) -> Value {
    Value::Array(calls.iter().map(durable_call).collect())
}

/// The durable form of one pricing provenance text, or `null` when the text is
/// not JSON or not one of the four kinds a producer writes (a catalog tariff or
/// `unknown` observation, a vendored snapshot estimate, an operator-declared
/// tariff); each call element's `withheld` then says which.
///
/// The object keeps its kind's own keys as written and names its `route` by
/// origin. A declared tariff's free text is judged against the endpoints of
/// `calls` and of its own route. A key outside its kind is dropped and counted,
/// never named.
#[must_use]
pub fn durable_pricing(pricing: &str, calls: &[InferenceCall]) -> Value {
    let mut private = Vec::new();
    for call in calls {
        call_material(call, &mut private);
    }
    pricing_object(pricing, &private).unwrap_or(Value::Null)
}

fn durable_call(call: &InferenceCall) -> Value {
    let mut private = Vec::new();
    call_material(call, &mut private);
    let mut withheld = Withheld::default();
    let pricing = match call
        .pricing
        .as_deref()
        .map(|raw| pricing_object(raw, &private))
    {
        None => Value::Null,
        Some(Ok(pricing)) => pricing,
        Some(Err(reason)) => {
            withheld.note("/pricing", reason);
            Value::Null
        }
    };
    let response_model = withheld.text("/response_model", call.response_model.as_deref(), &private);
    let request_id = withheld.text("/request_id", call.request_id.as_deref(), &private);
    serde_json::json!({
        "requested_origin": call.requested_endpoint.as_deref().and_then(route_origin),
        "route": call.route.as_ref().map(|route| serde_json::json!({
            "provider": route.provider,
            "model": route.model,
            "origin": route_origin(&route.endpoint),
        })),
        "usage": call.usage,
        "usage_complete": call.usage_complete,
        "response_model": response_model,
        "request_id": request_id,
        "pricing": pricing,
        "estimated_usd": call.estimated_usd,
        "estimate_known": call.known_estimate().is_some(),
        "withheld": withheld.into_value(),
    })
}

/// A pricing text's durable object, or why it is withheld whole.
fn pricing_object(raw: &str, private: &[String]) -> Result<Value, &'static str> {
    let Value::Object(fields) = serde_json::from_str::<Value>(raw).map_err(|_| UNREADABLE)? else {
        return Err(UNRECOGNIZED_KIND);
    };
    let (copied, text) = pricing_shape(&fields).ok_or(UNRECOGNIZED_KIND)?;
    let route = match fields.get("route") {
        None => None,
        Some(Value::Object(route)) => Some(route),
        Some(_) => return Err(UNRECOGNIZED_KIND),
    };
    let mut private = private.to_vec();
    if let Some(endpoint) = route
        .and_then(|route| route.get("endpoint"))
        .and_then(Value::as_str)
    {
        private_material(endpoint, &mut private);
    }
    let mut withheld = Withheld::default();
    let mut durable = Map::new();
    let mut dropped = 0;
    for (key, value) in &fields {
        if copied.contains(&key.as_str()) {
            durable.insert(key.clone(), value.clone());
        } else if let Some((name, field)) = text.iter().find(|(name, _)| *name == key.as_str()) {
            let judged = withheld.judge(field, value, &private);
            durable.insert((*name).to_owned(), judged);
        } else if key != "route" {
            dropped += 1;
        }
    }
    if let Some(route) = route {
        durable.insert("route".to_owned(), durable_route(route, &mut withheld));
    }
    withheld.dropped("", dropped);
    durable.insert("withheld".to_owned(), withheld.into_value());
    Ok(Value::Object(durable))
}

/// The kind a producer wrote, read from its `kind` and `table_schema`.
fn pricing_shape(fields: &Map<String, Value>) -> Option<Shape> {
    let schema = fields.get("table_schema").and_then(Value::as_str);
    match (fields.get("kind").and_then(Value::as_str)?, schema) {
        ("catalog_estimate_not_invoice" | "unknown", Some(TARIFF_SCHEMA)) => {
            Some((TARIFF_KEYS, &[]))
        }
        ("catalog_estimate_not_invoice", Some(schema))
            if schema == nika_catalog::PRICING_SCHEMA =>
        {
            Some((SNAPSHOT_KEYS, &[]))
        }
        ("user_declared_estimate_not_invoice", _) => Some((DECLARED_KEYS, DECLARED_TEXT)),
        _ => None,
    }
}

/// A pricing route named by origin; any other key it carries is dropped and
/// counted.
fn durable_route(route: &Map<String, Value>, withheld: &mut Withheld) -> Value {
    let origin = route
        .get("endpoint")
        .and_then(Value::as_str)
        .and_then(route_origin);
    let dropped = route
        .keys()
        .filter(|key| !ROUTE_KEYS.contains(&key.as_str()))
        .count();
    withheld.dropped("/route", dropped);
    serde_json::json!({
        "provider": route.get("provider"),
        "model": route.get("model"),
        "origin": origin,
    })
}

/// What one durable object withheld, as `(field, reason, dropped keys)`. A field
/// is always one of this module's schema pointers, never input text.
#[derive(Default)]
struct Withheld(Vec<(&'static str, &'static str, usize)>);

impl Withheld {
    fn note(&mut self, field: &'static str, reason: &'static str) {
        self.0.push((field, reason, 0));
    }

    /// Count the keys dropped from the object at `field`, without naming them.
    fn dropped(&mut self, field: &'static str, count: usize) {
        if count > 0 {
            self.0.push((field, UNRECOGNIZED_KEY, count));
        }
    }

    /// A free-text value as written, or `null` when its text holds endpoint
    /// material.
    fn judge(&mut self, field: &'static str, value: &Value, private: &[String]) -> Value {
        let text = match value {
            Value::String(text) => Cow::Borrowed(text.as_str()),
            other => Cow::Owned(other.to_string()),
        };
        if holds(&text, private) {
            self.note(field, ENDPOINT_MATERIAL);
            return Value::Null;
        }
        value.clone()
    }

    /// An optional free-text field, judged as [`Self::judge`] does.
    fn text(&mut self, field: &'static str, value: Option<&str>, private: &[String]) -> Value {
        match value {
            Some(text) if holds(text, private) => {
                self.note(field, ENDPOINT_MATERIAL);
                Value::Null
            }
            other => other.map_or(Value::Null, |text| Value::String(text.to_owned())),
        }
    }

    /// The `withheld` array, sorted by field then reason.
    fn into_value(mut self) -> Value {
        self.0.sort_unstable();
        let entries = self.0.into_iter().map(|(field, reason, count)| {
            let mut entry = Map::new();
            entry.insert("field".to_owned(), Value::from(field));
            entry.insert("reason".to_owned(), Value::from(reason));
            if count > 0 {
                entry.insert("count".to_owned(), Value::from(count));
            }
            Value::Object(entry)
        });
        Value::Array(entries.collect())
    }
}

/// The host-private parts of every endpoint a call names.
fn call_material(call: &InferenceCall, private: &mut Vec<String>) {
    for endpoint in call
        .requested_endpoint
        .iter()
        .chain(call.route.as_ref().map(|route| &route.endpoint))
    {
        private_material(endpoint, private);
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

/// Whether `text` holds a private part in a form the E29 oracle scans: as
/// written, with every `/` written as `\`, percent-encoded byte by byte in either
/// hex case, or `\u`-escaped character by character.
fn holds(text: &str, private: &[String]) -> bool {
    private.iter().any(|part| {
        [
            Cow::Borrowed(part.as_str()),
            Cow::Owned(part.replace('/', "\\")),
            Cow::Owned(percent(part, true)),
            Cow::Owned(percent(part, false)),
            Cow::Owned(escaped(part)),
        ]
        .iter()
        .any(|form| text.contains(form.as_ref()))
    })
}

/// `part` percent-encoded byte by byte.
fn percent(part: &str, upper: bool) -> String {
    let mut encoded = String::with_capacity(part.len() * 3);
    for byte in part.bytes() {
        encoded.push('%');
        for nibble in [byte >> 4, byte & 0x0f] {
            let digit = char::from_digit(u32::from(nibble), 16);
            encoded.extend(digit.map(|d| if upper { d.to_ascii_uppercase() } else { d }));
        }
    }
    encoded
}

/// `part` as JSON `\u` escapes, character by character.
fn escaped(part: &str) -> String {
    let mut encoded = String::with_capacity(part.len() * 6);
    for c in part.chars() {
        let _ = write!(encoded, "\\u{:04x}", u32::from(c));
    }
    encoded
}

#[cfg(test)]
mod tests;
