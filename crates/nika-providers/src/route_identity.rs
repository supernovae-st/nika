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
//! [`route_label`]. The account's observation has a durable form too
//! ([`project_observation`], `InferenceReceipt::durable_observation`,
//! `nika/inference-cost-observation@2`), which the admission reading law reads;
//! no journal, trace or session writer emits it yet. Pricing, consent and
//! journal judgment keep the exact in-memory endpoint. An origin groups every
//! route of one origin for display only: it never admits, prices or consents.
//!
//! The durable projections follow a closed schema. Identity fields become
//! origins. Money, counters, states, catalog constants and the selected provider
//! and model are copied as their producer wrote them and never rewritten. Only
//! named free text is judged (a declared tariff's `billing_provider`,
//! `provenance` and `version`, a call's `request_id` and `response_model`, and
//! in an observation also its `refusal` and each attempt's `note`): one holding
//! endpoint material becomes `null` with a `withheld` entry. A `withheld` entry
//! names a field of this schema by a pointer built from its static key names
//! and array indices, never from input text, so what it withholds cannot be
//! echoed by its own diagnostic. This is the whole privacy claim: text outside
//! the named fields is its producer's.

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
    pricing_value(&serde_json::from_str(raw).map_err(|_| UNREADABLE)?, private)
}

/// A pricing object's durable form (W2-W5), or why it is withheld whole.
fn pricing_value(value: &Value, private: &[String]) -> Result<Value, &'static str> {
    let Value::Object(fields) = value else {
        return Err(UNRECOGNIZED_KIND);
    };
    let (copied, text) = pricing_shape(fields).ok_or(UNRECOGNIZED_KIND)?;
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
    for (key, value) in fields {
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
/// is always a pointer of this module's schema, made of its static key names and
/// array indices, never of input text.
#[derive(Default)]
struct Withheld(Vec<(String, &'static str, usize)>);

impl Withheld {
    fn note(&mut self, field: impl Into<String>, reason: &'static str) {
        self.0.push((field.into(), reason, 0));
    }

    /// Count the keys dropped from the object at `field`, without naming them.
    fn dropped(&mut self, field: impl Into<String>, count: usize) {
        if count > 0 {
            self.0.push((field.into(), UNRECOGNIZED_KEY, count));
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

/// The account's observation as it writes it, and its durable form.
const OBSERVATION: &str = "nika/inference-cost-observation@1";
const DURABLE_OBSERVATION: &str = "nika/inference-cost-observation@2";

/// The counters `TokenUsage` serializes: a durable usage holds these only.
const USAGE_KEYS: &[&str] = &[
    "accepted_prediction_tokens",
    "audio_input_tokens",
    "audio_output_tokens",
    "cache_creation_tokens",
    "cache_read_tokens",
    "cache_write_tokens",
    "citation_tokens",
    "image_input_tokens",
    "image_output_tokens",
    "input_tokens",
    "num_requests",
    "output_tokens",
    "reasoning_tokens",
    "rejected_prediction_tokens",
    "search_context_tokens",
    "thinking_tokens",
    "total_tokens",
    "video_input_tokens",
];

/// The type a copied field keeps: the account's own serialization.
#[derive(Clone, Copy)]
enum Kind {
    /// A string.
    Text,
    /// A string or null.
    OptText,
    /// A decimal `i128` (nano-currency) written as a string.
    Nano,
    /// A decimal `i128` string, or null.
    OptNano,
    /// A non-negative integer.
    Count,
    /// A boolean.
    Flag,
    /// `true`: a key the account writes only when it holds.
    Set,
    /// `Open`, `Closed` or `Uncertain`.
    State,
    /// The two overridden defaults, each a decimal `i128` string or null.
    Defaults,
    /// Three non-negative integer rates.
    Rates,
    /// Token counters by their `TokenUsage` names, each an integer or null; or null.
    Usage,
}

impl Kind {
    fn fits(self, value: &Value) -> bool {
        let nano = |value: &Value| value.as_str().is_some_and(|v| v.parse::<i128>().is_ok());
        match self {
            Self::Text => value.is_string(),
            Self::OptText => value.is_string() || value.is_null(),
            Self::Nano => nano(value),
            Self::OptNano => value.is_null() || nano(value),
            Self::Count => value.is_u64(),
            Self::Flag => value.is_boolean(),
            Self::Set => *value == Value::Bool(true),
            Self::State => matches!(value.as_str(), Some("Open" | "Closed" | "Uncertain")),
            Self::Defaults => value
                .as_array()
                .is_some_and(|d| d.len() == 2 && d.iter().all(|v| v.is_null() || nano(v))),
            Self::Rates => value
                .as_array()
                .is_some_and(|r| r.len() == 3 && r.iter().all(Value::is_u64)),
            Self::Usage => {
                value.is_null()
                    || value.as_object().is_some_and(|usage| {
                        usage.iter().all(|(name, v)| {
                            USAGE_KEYS.contains(&name.as_str()) && (v.is_null() || v.is_u64())
                        })
                    })
            }
        }
    }
}

/// How the durable observation carries one known key.
#[derive(Clone, Copy)]
enum Carry {
    /// The schema name, written `@2`.
    Schema,
    /// Copied as written, once it holds the account's own type.
    Copy(Kind),
    /// Named free text (F1, amendment A10): kept, or null with a `withheld`
    /// entry when it holds endpoint material of the source observation.
    Text,
    /// The exact endpoint (`endpoint` at `@1`), written as its origin.
    Origin,
    /// A nested durable object; null too where the flag allows it.
    Object(&'static [Field], bool),
    /// A list of durable objects.
    List(&'static [Field]),
    /// A pricing provenance object (W2-W5), or null. At `@1`, one whose projection
    /// the `@2` reader refuses (a key of another type) makes the input malformed.
    Pricing,
}

/// One known key: its durable name, how it is carried, whether it is always written.
type Field = (&'static str, Carry, bool);

/// W9 with amendment A1: the observation, its choice, a declared tariff, an
/// unknown-cost attempt and a catalog attempt.
const OBSERVATION_FIELDS: &[Field] = &[
    ("schema", Carry::Schema, true),
    ("known_subtotal_nano_usd", Carry::Copy(Kind::Nano), true),
    ("unknown_calls", Carry::Copy(Kind::Count), true),
    ("unknown_cost", Carry::Object(CHOICE_FIELDS, true), true),
    ("unknown_attempts", Carry::List(UNKNOWN_FIELDS), true),
    ("overridden_defaults", Carry::Copy(Kind::Defaults), true),
    ("limit_nano_usd", Carry::Copy(Kind::OptNano), true),
    ("billed_nano_usd", Carry::Copy(Kind::OptNano), true),
    ("state", Carry::Copy(Kind::State), true),
    ("refusal", Carry::Text, true),
    ("attempts", Carry::List(CATALOG_FIELDS), true),
    ("unbudgeted", Carry::Copy(Kind::Set), false),
    ("scoped_to_declared_free", Carry::Copy(Kind::Set), false),
];
const CHOICE_FIELDS: &[Field] = &[
    ("candidate", Carry::Copy(Kind::Text), true),
    ("invocation", Carry::Copy(Kind::Text), true),
    ("provider", Carry::Copy(Kind::Text), true),
    ("model", Carry::Copy(Kind::Text), true),
    ("origin", Carry::Origin, true),
    ("max_requests", Carry::Copy(Kind::Count), true),
    ("max_in_flight", Carry::Copy(Kind::Count), false),
    ("authored_retry", Carry::Copy(Kind::Set), false),
    ("max_output_tokens", Carry::Copy(Kind::Count), true),
    ("timeout_ms", Carry::Copy(Kind::Count), true),
    (
        "declared_tariff",
        Carry::Object(DECLARED_FIELDS, true),
        true,
    ),
];
const DECLARED_FIELDS: &[Field] = &[
    ("provider", Carry::Copy(Kind::Text), true),
    ("model", Carry::Copy(Kind::Text), true),
    ("origin", Carry::Origin, true),
    ("billing_provider", Carry::Text, true),
    ("currency", Carry::Copy(Kind::Text), true),
    ("unit", Carry::Copy(Kind::Text), true),
    ("nano_per_token", Carry::Copy(Kind::Rates), true),
    ("provenance", Carry::Text, true),
    ("version", Carry::Text, true),
];
const UNKNOWN_FIELDS: &[Field] = &[
    ("id", Carry::Copy(Kind::Count), true),
    ("choice", Carry::Object(CHOICE_FIELDS, false), true),
    ("pricing", Carry::Pricing, true),
    ("sent", Carry::Copy(Kind::Flag), true),
    ("usage", Carry::Copy(Kind::Usage), true),
    ("estimated_nano_usd", Carry::Copy(Kind::OptNano), true),
    ("native_estimated_nano", Carry::Copy(Kind::OptNano), true),
    ("currency", Carry::Copy(Kind::OptText), true),
    ("response_model", Carry::Text, true),
    ("request_id", Carry::Text, true),
    ("note", Carry::Text, true),
];
const CATALOG_FIELDS: &[Field] = &[
    ("id", Carry::Copy(Kind::Count), true),
    ("model", Carry::Copy(Kind::Text), true),
    ("origin", Carry::Origin, true),
    ("sent", Carry::Copy(Kind::Flag), true),
    ("estimated_nano_usd", Carry::Copy(Kind::OptNano), true),
    ("reserved_nano_usd", Carry::Copy(Kind::Nano), true),
    ("usage", Carry::Copy(Kind::Usage), true),
    ("billing_provider", Carry::Copy(Kind::Text), true),
    ("currency", Carry::Copy(Kind::Text), true),
    ("source", Carry::Copy(Kind::Text), true),
    ("as_of", Carry::Copy(Kind::Text), true),
    ("source_sha256", Carry::Copy(Kind::Text), true),
    ("note", Carry::Text, true),
];

/// The durable form of a cost observation (`nika/inference-cost-observation@2`,
/// W9 with amendments A1 and A8-A10).
///
/// An `@1` projects to the same closed schema with every endpoint written as its
/// origin ([`route_origin`]), every money, counter, state and identity value
/// copied as written, and the named free text (a declared tariff's
/// `billing_provider`, `provenance` and `version`, each attempt's `request_id`,
/// `response_model` and `note`, and the `refusal`) kept unless it holds material
/// of an endpoint the observation names, then null. `withheld` lists each such
/// field by its instance pointer (`/unknown_attempts/0/request_id`) and each
/// object's count of unknown keys, which are dropped and never named.
///
/// An `@2` is returned unchanged only when it is exactly what a projection
/// writes: its known keys with their types, canonical origins, and a sorted
/// `withheld` that names only null text or objects it holds. Anything else,
/// another schema or a malformed `@1` or `@2`, is `None`: a reader refuses it
/// as unreadable, never repairs it. What an `@1` projects to is an `@2` this
/// function returns unchanged: an `@1` whose pricing object projects to one the
/// `@2` reading refuses (a key of another type) is malformed.
#[must_use]
pub fn project_observation(observation: &Value) -> Option<Value> {
    let schema = observation.get("schema").and_then(Value::as_str)?;
    if schema == DURABLE_OBSERVATION {
        let mut body = observation.clone();
        let listed = body.as_object_mut()?.remove("withheld")?;
        let mut walk = Walk::new(true, &[]);
        walk.object(OBSERVATION_FIELDS, &body, "")?;
        return lists(&listed, &walk.allowed).then(|| observation.clone());
    }
    if schema != OBSERVATION {
        return None;
    }
    let mut private = Vec::new();
    endpoints(observation, &mut private);
    let mut walk = Walk::new(false, &private);
    let mut durable = walk.object(OBSERVATION_FIELDS, observation, "")?;
    durable["withheld"] = walk.withheld.into_value();
    Some(durable)
}

/// A recorded route named by origin (W11): `{provider, model, endpoint}` becomes
/// `{provider, model, origin}` and an origin route keeps its canonical origin.
/// An origin that is not one, or none, is null; a non-object is null.
#[must_use]
pub fn project_route(route: &Value) -> Value {
    let Value::Object(fields) = route else {
        return Value::Null;
    };
    let origin = match (fields.get("endpoint"), fields.get("origin")) {
        (Some(endpoint), _) => endpoint.as_str().and_then(route_origin),
        (None, Some(origin)) => origin.as_str().filter(|o| is_origin(o)).map(str::to_owned),
        (None, None) => None,
    };
    serde_json::json!({"provider": fields.get("provider"), "model": fields.get("model"),
        "origin": origin})
}

impl crate::InferenceReceipt {
    /// This receipt's observation in its durable form ([`project_observation`]):
    /// what a journal row, a trace or a saved session may keep, while
    /// [`Self::observation`] stays the exact `@1` in memory. Null when the account
    /// wrote something outside the closed schema, a drift its producer tests fail
    /// on and a record every reader refuses as unreadable.
    #[must_use]
    pub fn durable_observation(&self) -> Value {
        project_observation(&self.observation()).unwrap_or(Value::Null)
    }
}

/// Whether `origin` is exactly the origin [`route_origin`] writes for itself.
fn is_origin(origin: &str) -> bool {
    route_origin(origin).as_deref() == Some(origin)
}

/// The host-private parts of every endpoint an observation names, wherever it
/// names one.
fn endpoints(value: &Value, private: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if let Value::String(endpoint) = value
                    && key == "endpoint"
                {
                    private_material(endpoint, private);
                } else {
                    endpoints(value, private);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                endpoints(item, private);
            }
        }
        _ => {}
    }
}

/// One walk over an observation: the projection of an `@1`, or the check of an
/// `@2`, which must already be exactly what a projection writes.
struct Walk<'a> {
    /// The input is `@2`: nothing is judged, dropped or rewritten.
    durable: bool,
    /// Endpoint material of the source observation (`@1` only).
    private: &'a [String],
    /// What an `@1` projection withholds.
    withheld: Withheld,
    /// The `withheld` entries an `@2` may carry: a null text or pricing field,
    /// or an object that may have dropped keys.
    allowed: Vec<(String, &'static str)>,
}

impl<'a> Walk<'a> {
    fn new(durable: bool, private: &'a [String]) -> Self {
        Self {
            durable,
            private,
            withheld: Withheld::default(),
            allowed: Vec::new(),
        }
    }

    /// The durable object of `fields` at pointer `at`, or `None` when a known key
    /// is missing or ill-typed, or an `@2` holds an unknown key.
    fn object(&mut self, fields: &'static [Field], value: &Value, at: &str) -> Option<Value> {
        let mut durable = Map::new();
        let mut dropped = 0;
        for (key, value) in value.as_object()? {
            let written = |field: &&Field| self.key(field.0, field.1) == key.as_str();
            let Some(&(name, carry, _)) = fields.iter().find(written) else {
                dropped += 1;
                continue;
            };
            let carried = self.carry(carry, value, &format!("{at}/{name}"))?;
            durable.insert(name.to_owned(), carried);
        }
        if fields
            .iter()
            .any(|(name, _, always)| *always && !durable.contains_key(*name))
        {
            return None;
        }
        if self.durable {
            if dropped > 0 {
                return None;
            }
            self.allowed.push((at.to_owned(), UNRECOGNIZED_KEY));
        } else {
            self.withheld.dropped(at, dropped);
        }
        Some(Value::Object(durable))
    }

    /// The key an input names a field by: an `@1` names its origin `endpoint`.
    fn key(&self, name: &'static str, carry: Carry) -> &'static str {
        match carry {
            Carry::Origin if !self.durable => "endpoint",
            _ => name,
        }
    }

    fn carry(&mut self, carry: Carry, value: &Value, at: &str) -> Option<Value> {
        match carry {
            Carry::Schema => Some(Value::from(DURABLE_OBSERVATION)),
            Carry::Copy(kind) => kind.fits(value).then(|| value.clone()),
            Carry::Text => self.text(value, at),
            Carry::Origin => match value {
                Value::String(endpoint) if !self.durable => {
                    Some(route_origin(endpoint).map_or(Value::Null, Value::String))
                }
                Value::String(origin) => is_origin(origin).then(|| value.clone()),
                Value::Null if self.durable => Some(Value::Null),
                _ => None,
            },
            Carry::Object(_, true) if value.is_null() => Some(Value::Null),
            Carry::Object(fields, _) => self.object(fields, value, at),
            Carry::List(fields) => {
                let items = value.as_array()?.iter().enumerate();
                let durable: Option<Vec<Value>> = items
                    .map(|(index, item)| self.object(fields, item, &format!("{at}/{index}")))
                    .collect();
                durable.map(Value::Array)
            }
            Carry::Pricing if value.is_null() => {
                if self.durable {
                    self.allowed.push((at.to_owned(), UNRECOGNIZED_KIND));
                }
                Some(Value::Null)
            }
            Carry::Pricing if self.durable => durable_pricing_reads(value).then(|| value.clone()),
            // A projection the `@2` reader would refuse is a malformed `@1`.
            Carry::Pricing => match pricing_value(value, self.private) {
                Ok(durable) => durable_pricing_reads(&durable).then_some(durable),
                Err(reason) => {
                    self.withheld.note(at, reason);
                    Some(Value::Null)
                }
            },
        }
    }

    /// Named free text: a string or null. At `@1` it is judged against the
    /// observation's endpoint material; at `@2` a null may be listed as withheld.
    fn text(&mut self, value: &Value, at: &str) -> Option<Value> {
        match value {
            Value::Null => {
                if self.durable {
                    self.allowed.push((at.to_owned(), ENDPOINT_MATERIAL));
                }
                Some(Value::Null)
            }
            Value::String(text) if !self.durable && holds(text, self.private) => {
                self.withheld.note(at, ENDPOINT_MATERIAL);
                Some(Value::Null)
            }
            Value::String(_) => Some(value.clone()),
            _ => None,
        }
    }
}

/// Whether a durable pricing object (W2-W5) is one a projection writes: its
/// kind's keys with scalar values, named text that is a string or null, a
/// `{provider, model, origin}` route, and a `withheld` naming only its own fields.
fn durable_pricing_reads(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    let Some((copied, text)) = pricing_shape(fields) else {
        return false;
    };
    let mut allowed = vec![(String::new(), UNRECOGNIZED_KEY)];
    for (key, value) in fields {
        let fits = if copied.contains(&key.as_str()) {
            match key.as_str() {
                "nano_per_token" => Kind::Rates.fits(value),
                _ => !value.is_array() && !value.is_object(),
            }
        } else if let Some((_, pointer)) = text.iter().find(|(name, _)| *name == key.as_str()) {
            if value.is_null() {
                allowed.push(((*pointer).to_owned(), ENDPOINT_MATERIAL));
            }
            value.is_string() || value.is_null()
        } else if key == "route" {
            allowed.push(("/route".to_owned(), UNRECOGNIZED_KEY));
            value.as_object().is_some_and(|route| {
                route.len() == 3
                    && ["provider", "model"]
                        .iter()
                        .all(|k| route.get(*k).is_some_and(|v| v.is_string() || v.is_null()))
                    && route.get("origin").is_some_and(|origin| {
                        origin.is_null() || origin.as_str().is_some_and(is_origin)
                    })
            })
        } else {
            key == "withheld"
        };
        if !fits {
            return false;
        }
    }
    fields
        .get("withheld")
        .is_some_and(|listed| lists(listed, &allowed))
}

/// Whether `listed` is a `withheld` list a projection writes: sorted, unique
/// entries, each one `allowed`, a dropped-key entry with its positive count and
/// any other entry with none.
fn lists(listed: &Value, allowed: &[(String, &'static str)]) -> bool {
    let Some(entries) = listed.as_array() else {
        return false;
    };
    let mut previous: Option<(&str, &str)> = None;
    entries.iter().all(|entry| {
        let Some(entry) = entry.as_object() else {
            return false;
        };
        let field = entry.get("field").and_then(Value::as_str);
        let reason = entry.get("reason").and_then(Value::as_str);
        let (Some(field), Some(reason)) = (field, reason) else {
            return false;
        };
        let counted = reason == UNRECOGNIZED_KEY;
        let arity = if counted { 3 } else { 2 };
        let count = entry.get("count").and_then(Value::as_u64);
        let shaped = entry.len() == arity && (!counted || count.is_some_and(|n| n > 0));
        let known = allowed.iter().any(|(f, r)| f == field && *r == reason);
        let ordered = previous.is_none_or(|before| before < (field, reason));
        previous = Some((field, reason));
        shaped && known && ordered
    })
}

#[cfg(test)]
mod observation_tests;
#[cfg(test)]
mod tests;
