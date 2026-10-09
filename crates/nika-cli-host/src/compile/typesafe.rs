// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A `TypeSafe` System One seat behind the compiler's bounded-decision capability.
//!
//! One unary `POST /v1/systemone` per question, or per request of a batch of independent
//! questions ([`batch`]: one, or its halves after a refusal for capacity), in the wire form of
//! `decide::system_one` (this seat owns only the transport): each a Choice with a `none`
//! criterion, no SDK retry loop, no fallback. The key rides only in the Authorization header;
//! [`TypesafeSeat::from_env`] reads it from `TYPESAFE_API_KEY` only after
//! a door named the seat, and nothing prints it (the seat has no `Debug`). The seat returns the
//! selected key, the reported distribution, the concentration statistic and the returned model
//! identity; the compiler revalidates the choice against the options it offered. `nika compile`
//! (`--decision-model typesafe/<jev>`) and the Session (an operator-selected seat) share this ONE
//! adapter. A vendor is a seat, never an owner.
pub mod batch;
pub mod session;

use nika_http::{HttpConfig, NetBoundary, ReqwestHttp};
use nika_kernel::http::{HttpPostDyn as _, HttpRequest, HttpResponse};
pub use nika_onboard::compile::decide::system_one::Usage;
use nika_onboard::compile::decide::{
    BatchFuture, ChoiceAnswer, ChoiceBatch, ChoiceFuture, ChoiceQuestion, DecisionError,
    DecisionSeat, system_one,
};
use serde_json::Value;
use std::{future::Future, pin::Pin, time::Duration};

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
    /// Refused before any byte left (serialization failed, or a client could not be built).
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
    /// The usage a response reported although it answered nothing usable (a malformed answer);
    /// all unknown when no response was read.
    pub usage: Usage,
}

/// The object-safe future of one exchange.
pub type ExchangeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Exchange, ExchangeError>> + Send + 'a>>;

/// The seat `typesafe/<model>` names, opened from the environment for a door that seats it for
/// many rounds (Serve), with its key to withhold. Errors as [`TypesafeSeat::from_env`].
///
/// # Errors
/// The key is absent, empty or malformed, or the endpoint is refused.
pub fn seat(model: &str) -> Result<nika_onboard::remote_door::decision::Opened, String> {
    let seat = TypesafeSeat::from_env(model)?;
    let key = nika_kernel::secret::Secret::new(seat.key.as_str());
    Ok((Box::new(seat), key))
}

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

    /// The key this seat resolved, borrowed by this crate's private capture policy so the key
    /// is withheld from captured Text; never printed, logged or exposed outside the crate.
    pub(crate) fn key(&self) -> &str {
        &self.key
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
            let body = system_one::request(&self.model, &[question]);
            let response = self.post(&body).await?;
            let status = response.status;
            let answered = |message: String, usage: Usage| ExchangeError {
                error: DecisionError(message),
                delivery: Delivery::Responded(status),
                usage,
            };
            if let Some(refusal) = system_one::refusal(status, &response.body) {
                return Err(answered(refusal.to_string(), Usage::default()));
            }
            let parsed: Value = serde_json::from_slice(&response.body)
                .map_err(|e| answered(format!("response is not JSON: {e}"), Usage::default()))?;
            let (answer, billing_units) = system_one::answer(&parsed, &question.id)
                .map_err(|e| answered(e, Usage::of(&parsed)))?;
            Ok(Exchange {
                answer,
                status,
                billing_units,
            })
        })
    }

    /// The ONE single-attempt POST of `body` to the seat's endpoint, the key in its header only:
    /// the response whatever its status, or how far the request went.
    async fn post(&self, body: &Value) -> Result<HttpResponse, ExchangeError> {
        let unsent = |message: String| ExchangeError {
            error: DecisionError(message),
            delivery: Delivery::NotSent,
            usage: Usage::default(),
        };
        let bytes = serde_json::to_vec(body).map_err(|e| unsent(e.to_string()))?;
        // Jev's context is token-based, not a JSON byte count. The selected service
        // validates its context; a local byte threshold must not impersonate that limit.
        let host = url::Url::parse(&self.base)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .ok_or_else(|| unsent("invalid seat base URL".to_owned()))?;
        let http = decision_http(host).map_err(unsent)?;
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
        http.post(request).await.map_err(|e| ExchangeError {
            error: DecisionError(format!("transport: {e}")),
            delivery: Delivery::Unknown,
            usage: Usage::default(),
        })
    }
}

/// The seat's client: the endpoint host only, no redirect hop and no protocol-NACK replay, so a
/// question is one physical request. A client that could still replay a request on its own is
/// refused before any byte leaves, as the authoring transport is.
fn decision_http(host: String) -> Result<ReqwestHttp, String> {
    let mut config = HttpConfig::new();
    config.net = NetBoundary::Declared(vec![host]);
    config.retry_protocol_nacks = false;
    single_attempt(ReqwestHttp::with_config(config).map_err(|e| e.to_string())?)
}

/// `http`, only when it sends each request once.
fn single_attempt(http: ReqwestHttp) -> Result<ReqwestHttp, String> {
    if http.supports_single_attempt() {
        Ok(http)
    } else {
        Err("the decision transport would replay a refused request on its own; not sent".to_owned())
    }
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
    /// The batch's requests ([`TypesafeSeat::exchange_each`]): each item bound by its id.
    fn choose_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchFuture<'a> {
        Box::pin(async move { self.exchange_each(batch).await.answers })
    }
}

#[cfg(test)]
mod regression_tests;
#[cfg(test)]
mod wire_tests;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A refused request is never replayed. The seat's own client is a single-attempt client: the
    /// `nika-http` protocol-NACK test binds that property to ONE request reaching a peer that
    /// refuses every HTTP/2 stream, against three for a default client. A default client, which
    /// would replay, is refused before it can send.
    #[test]
    fn the_decision_client_never_replays_a_refused_request() {
        use nika_kernel::http::HttpPostDyn;
        let seat = decision_http("api.typesafe.ai".to_owned()).unwrap();
        assert!(
            HttpPostDyn::supports_single_attempt(&seat),
            "the decision client could replay a refused request"
        );
        let replaying = ReqwestHttp::new().unwrap();
        assert!(!HttpPostDyn::supports_single_attempt(&replaying));
        assert!(
            single_attempt(replaying).is_err(),
            "a client that replays was accepted for a metered exchange"
        );
    }

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
        let (answer, units) = system_one::answer(&body, "q").unwrap();
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
        let (answer, units) = system_one::answer(&tokens, "q").unwrap();
        assert_eq!(
            (answer.input_tokens, answer.output_tokens, units),
            (Some(40), Some(2), None)
        );
        assert!(system_one::answer(&json!({"model": "m", "answers": {}}), "q").is_err());
        assert!(
            system_one::answer(
                &json!({"answers": {"q": {"type": "choice", "choice": "x"}}}),
                "q"
            )
            .is_err()
        );
    }
}
