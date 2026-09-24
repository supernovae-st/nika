// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A `TypeSafe` System One seat behind the compiler's bounded-decision capability.
//!
//! One unary `POST /v1/systemone` per question, one Choice with a `none`
//! criterion, no SDK retry loop, no fallback. The key rides only in the
//! Authorization header; [`TypesafeSeat::from_env`] reads it from `TYPESAFE_API_KEY` only after
//! a door named the seat, and nothing prints it (the seat has no `Debug`). The seat returns the
//! selected key, the reported distribution, the concentration statistic and the returned model
//! identity; the compiler revalidates the choice against the options it offered. `nika compile`
//! (`--decision-model typesafe/<jev>`) and the Session (an operator-selected seat) share this ONE
//! adapter. A vendor is a seat, never an owner.
pub mod session;

use nika_http::{HttpConfig, NetBoundary, ReqwestHttp};
use nika_kernel::http::{HttpPostDyn as _, HttpRequest};
use nika_onboard::compile::decide::{
    ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, time::Duration};

const ENDPOINT: &str = "https://api.typesafe.ai";

/// The deadline of the ONE request a question sends.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// A `TypeSafe` System One decision seat. It holds its key; it has no `Debug` on purpose.
pub struct TypesafeSeat {
    key: String,
    model: String,
    name: String,
    base: String,
    timeout: Duration,
}

/// How far one request went: never sent, an unknown outcome (the transport failed after it
/// may have left), or a response with its HTTP status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Delivery {
    /// Refused before any byte left (an oversized request, a client that could not be built).
    NotSent,
    /// The transport failed: the request may have been received and billed.
    Unknown,
    /// A response came back with this status.
    Responded(u16),
}

/// One exchange with the seat: the answer and what the wire reported beside it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Exchange {
    /// The answer the compiler revalidates.
    pub answer: ChoiceAnswer,
    /// The response status.
    pub status: u16,
    /// The seat's reported billing units — never relabelled as input tokens.
    pub billing_units: Option<u64>,
}

/// A failed exchange: the typed error and how far the request went.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExchangeError {
    /// What failed.
    pub error: DecisionError,
    /// Whether the request left, and what came back.
    pub delivery: Delivery,
}

/// The object-safe future of one exchange.
pub type ExchangeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Exchange, ExchangeError>> + Send + 'a>>;

impl TypesafeSeat {
    /// `model` is the wire id (`jev-1.13.0`); the key must already be in hand. The endpoint is
    /// `TYPESAFE_BASE_URL` when the operator names one, else the public service.
    ///
    /// # Errors
    /// An empty or malformed key, or an endpoint that is neither https nor http on loopback.
    pub fn new(key: String, model: &str) -> Result<Self, String> {
        #[allow(clippy::disallowed_methods)]
        // an explicit, operator-named seat endpoint override; never a credential
        let base = std::env::var("TYPESAFE_BASE_URL").unwrap_or_else(|_| ENDPOINT.to_owned());
        Self::with_base(key, model, &base)
    }

    /// The same seat on an explicit endpoint (a host's typed value, a loopback test peer).
    ///
    /// # Errors
    /// An empty or malformed key, or an endpoint that is neither https nor http on loopback.
    pub fn with_base(key: String, model: &str, base: &str) -> Result<Self, String> {
        if key.trim().is_empty() || key.chars().any(char::is_whitespace) {
            return Err("TYPESAFE_API_KEY is empty or malformed".to_owned());
        }
        let parsed = url::Url::parse(base).map_err(|e| format!("TYPESAFE_BASE_URL: {e}"))?;
        let loopback = parsed
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");
        if !(parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback)) {
            return Err("TYPESAFE_BASE_URL must be https, or http on loopback".to_owned());
        }
        Ok(Self {
            key,
            model: model.to_owned(),
            name: format!("typesafe/{model}"),
            base: base.trim_end_matches('/').to_owned(),
            timeout: TIMEOUT,
        })
    }

    /// The seat a door explicitly named, its key read now from `TYPESAFE_API_KEY` — the one
    /// sanctioned env→secret boundary of this seat. Nothing reads the key before a door named
    /// the seat: an ambient key is never consent.
    ///
    /// # Errors
    /// The key is absent, empty or malformed, or the endpoint is refused.
    pub fn from_env(model: &str) -> Result<Self, String> {
        #[allow(clippy::disallowed_methods)]
        // the sanctioned env→secret boundary for an explicitly named seat (compose.rs precedent)
        let key = std::env::var("TYPESAFE_API_KEY")
            .map_err(|_| "TYPESAFE_API_KEY is required for a typesafe decision seat".to_owned())?;
        Self::new(key, model)
    }

    /// The wire model id (`jev-1.13.0`).
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The host the requests go to (never a credential, never a path).
    #[must_use]
    pub fn endpoint_host(&self) -> String {
        url::Url::parse(&self.base)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_else(|| "unparsable endpoint".to_owned())
    }

    /// The deadline of one request.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Exactly one physical request for `question`, with what the wire reported: no retry, no
    /// fallback. [`DecisionSeat::choose`] is this exchange's answer.
    #[must_use]
    pub fn exchange<'a>(&'a self, question: &'a ChoiceQuestion) -> ExchangeFuture<'a> {
        Box::pin(async move {
            let unsent = |message: String| ExchangeError {
                error: DecisionError(message),
                delivery: Delivery::NotSent,
            };
            let criteria: serde_json::Map<String, Value> = question
                .options
                .iter()
                .map(|o| (o.key.clone(), Value::String(o.description.clone())))
                .collect();
            let body = json!({
                "model": self.model,
                "state": question.state,
                "questions": {question.id.clone(): {"type": "choice", "instructions": question.instructions, "criteria": criteria}}
            });
            let bytes = serde_json::to_vec(&body).map_err(|e| unsent(e.to_string()))?;
            if bytes.len() > 64_000 {
                return Err(unsent(
                    "decision request exceeds the 64000-byte transport cap".to_owned(),
                ));
            }
            let host = url::Url::parse(&self.base)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .ok_or_else(|| unsent("invalid seat base URL".to_owned()))?;
            let mut config = HttpConfig::new();
            config.net = NetBoundary::Declared(vec![host]);
            let http = ReqwestHttp::with_config(config).map_err(|e| unsent(e.to_string()))?;
            let mut request = HttpRequest::post(format!("{}/v1/systemone", self.base));
            request.follow_redirects = false;
            request.timeout = Some(self.timeout);
            request
                .headers
                .insert("authorization".to_owned(), format!("Bearer {}", self.key));
            request
                .headers
                .insert("content-type".to_owned(), "application/json".to_owned());
            request.body = Some(bytes::Bytes::from(bytes));
            let response = http.post(request).await.map_err(|e| ExchangeError {
                error: DecisionError(format!("transport: {e}")),
                delivery: Delivery::Unknown,
            })?;
            let status = response.status;
            let answered = |message: String| ExchangeError {
                error: DecisionError(message),
                delivery: Delivery::Responded(status),
            };
            if !(200..300).contains(&status) {
                return Err(answered(format!("typesafe http status {status}")));
            }
            let parsed: Value = serde_json::from_slice(&response.body)
                .map_err(|e| answered(format!("response is not JSON: {e}")))?;
            let (answer, billing_units) = parse(&parsed, &question.id).map_err(answered)?;
            Ok(Exchange {
                answer,
                status,
                billing_units,
            })
        })
    }
}

/// The answer to `id` in one System One response, with the billing units it reported.
fn parse(parsed: &Value, id: &str) -> Result<(ChoiceAnswer, Option<u64>), String> {
    let model = parsed
        .get("model")
        .and_then(Value::as_str)
        .ok_or("response lacks model")?
        .to_owned();
    let answer = parsed
        .get("answers")
        .and_then(|a| a.get(id))
        .ok_or("response lacks the answer")?;
    if answer.get("type").and_then(Value::as_str) != Some("choice") {
        return Err("answer is not a choice".to_owned());
    }
    let choice = answer
        .get("choice")
        .and_then(Value::as_str)
        .ok_or("answer lacks choice")?
        .to_owned();
    let mut probabilities = BTreeMap::new();
    if let Some(map) = answer.get("probabilities").and_then(Value::as_object) {
        for (key, value) in map {
            if let Some(p) = unit(Some(value)) {
                probabilities.insert(key.clone(), p);
            }
        }
    }
    let usage = parsed.get("usage");
    let count = |name: &str| usage.and_then(|u| u.get(name)).and_then(Value::as_u64);
    let mut result = ChoiceAnswer::new(choice, model);
    result.probabilities = probabilities;
    result.confidence = unit(answer.get("confidence"));
    // Billing units are the seat's own accounting, not tokens: never relabelled as input tokens.
    result.input_tokens = count("input_tokens");
    result.output_tokens = count("output_tokens");
    Ok((result, count("billing_units")))
}

fn unit(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|p| (0.0..=1.0).contains(p))
}

impl DecisionSeat for TypesafeSeat {
    fn name(&self) -> &str {
        &self.name
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.exchange(question)
                .await
                .map(|exchange| exchange.answer)
                .map_err(|failure| failure.error)
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn an_endpoint_must_be_https_or_loopback_http_and_the_key_well_formed() {
        assert!(
            TypesafeSeat::with_base("k".into(), "jev-1.13.0", "https://api.typesafe.ai").is_ok()
        );
        assert!(TypesafeSeat::with_base("k".into(), "jev-1.13.0", "http://127.0.0.1:9").is_ok());
        assert!(TypesafeSeat::with_base("k".into(), "jev-1.13.0", "http://example.com").is_err());
        assert!(
            TypesafeSeat::with_base(" ".into(), "jev-1.13.0", "https://api.typesafe.ai").is_err()
        );
        assert!(
            TypesafeSeat::with_base("a b".into(), "jev-1.13.0", "https://api.typesafe.ai").is_err()
        );
        let seat =
            TypesafeSeat::with_base("k".into(), "jev-1.13.0", "https://api.typesafe.ai/").unwrap();
        assert_eq!(seat.name(), "typesafe/jev-1.13.0");
        assert_eq!(seat.endpoint_host(), "api.typesafe.ai");
        assert_eq!(seat.timeout(), TIMEOUT);
    }

    #[test]
    fn billing_units_are_reported_as_billing_units_never_as_input_tokens() {
        let body = json!({"model": "jev-1.13.0", "answers": {"q": {"type": "choice", "choice": "lookup",
            "probabilities": {"lookup": 0.9, "none": 0.1, "bad": 3.0}, "confidence": 0.8}},
            "usage": {"billing_units": 7}});
        let (answer, units) = parse(&body, "q").unwrap();
        assert_eq!(answer.choice, "lookup");
        assert_eq!(
            answer.input_tokens, None,
            "billing units relabelled as input tokens"
        );
        assert_eq!(units, Some(7));
        assert_eq!(
            answer.probabilities.len(),
            2,
            "an out-of-range probability is dropped"
        );
        let tokens = json!({"model": "m", "answers": {"q": {"type": "choice", "choice": "none"}},
            "usage": {"input_tokens": 40, "output_tokens": 2}});
        let (answer, units) = parse(&tokens, "q").unwrap();
        assert_eq!(
            (answer.input_tokens, answer.output_tokens, units),
            (Some(40), Some(2), None)
        );
        assert!(parse(&json!({"model": "m", "answers": {}}), "q").is_err());
        assert!(
            parse(
                &json!({"answers": {"q": {"type": "choice", "choice": "x"}}}),
                "q"
            )
            .is_err()
        );
    }
}
