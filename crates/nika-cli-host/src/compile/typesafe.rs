// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A `TypeSafe` System One seat behind the compiler's bounded-decision capability.
//!
//! One unary `POST /v1/systemone` per question, one Choice with a `none`
//! criterion, no SDK retry loop, no fallback. The key rides only in the
//! Authorization header from `TYPESAFE_API_KEY`; nothing is logged. The seat
//! returns the selected key, the reported distribution, the concentration
//! statistic and the returned model identity; the compiler revalidates the
//! choice against the options it offered. A vendor is a seat, never an owner.
use nika_http::{HttpConfig, NetBoundary, ReqwestHttp};
use nika_kernel::http::{HttpPostDyn as _, HttpRequest};
use nika_onboard::compile::decide::{
    ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

const ENDPOINT: &str = "https://api.typesafe.ai";

pub(super) struct TypesafeSeat {
    key: String,
    model: String,
    name: String,
    base: String,
    timeout: Duration,
}

impl TypesafeSeat {
    /// `model` is the wire id (`jev-1.13.0`); the key must already be in hand.
    pub(super) fn new(key: String, model: &str) -> Result<Self, String> {
        if key.trim().is_empty() || key.chars().any(char::is_whitespace) {
            return Err("TYPESAFE_API_KEY is empty or malformed".to_owned());
        }
        #[allow(clippy::disallowed_methods)]
        // an explicit, operator-named seat endpoint override; never a credential
        let base = std::env::var("TYPESAFE_BASE_URL").unwrap_or_else(|_| ENDPOINT.to_owned());
        let parsed = url::Url::parse(&base).map_err(|e| format!("TYPESAFE_BASE_URL: {e}"))?;
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
            timeout: Duration::from_secs(20),
        })
    }
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
            let bytes = serde_json::to_vec(&body).map_err(|e| DecisionError(e.to_string()))?;
            if bytes.len() > 64_000 {
                return Err(DecisionError(
                    "decision request exceeds the 64000-byte transport cap".to_owned(),
                ));
            }
            let host = url::Url::parse(&self.base)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .ok_or_else(|| DecisionError("invalid seat base URL".to_owned()))?;
            let mut config = HttpConfig::new();
            config.net = NetBoundary::Declared(vec![host]);
            let http =
                ReqwestHttp::with_config(config).map_err(|e| DecisionError(e.to_string()))?;
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
            let response = http
                .post(request)
                .await
                .map_err(|e| DecisionError(format!("transport: {e}")))?;
            if !(200..300).contains(&response.status) {
                return Err(DecisionError(format!(
                    "typesafe http status {}",
                    response.status
                )));
            }
            let parsed: Value = serde_json::from_slice(&response.body)
                .map_err(|e| DecisionError(format!("response is not JSON: {e}")))?;
            let model = parsed
                .get("model")
                .and_then(Value::as_str)
                .ok_or_else(|| DecisionError("response lacks model".to_owned()))?
                .to_owned();
            let answer = parsed
                .get("answers")
                .and_then(|a| a.get(&question.id))
                .ok_or_else(|| DecisionError("response lacks the answer".to_owned()))?;
            if answer.get("type").and_then(Value::as_str) != Some("choice") {
                return Err(DecisionError("answer is not a choice".to_owned()));
            }
            let choice = answer
                .get("choice")
                .and_then(Value::as_str)
                .ok_or_else(|| DecisionError("answer lacks choice".to_owned()))?
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
            let mut result = ChoiceAnswer::new(choice, model);
            result.probabilities = probabilities;
            result.confidence = unit(answer.get("confidence"));
            result.input_tokens = usage
                .and_then(|u| u.get("input_tokens"))
                .and_then(Value::as_u64)
                .or_else(|| {
                    usage
                        .and_then(|u| u.get("billing_units"))
                        .and_then(Value::as_u64)
                });
            result.output_tokens = usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(Value::as_u64);
            Ok(result)
        })
    }
}
