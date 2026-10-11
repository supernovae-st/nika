// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The typed decision seat a session may consult — one operator-selected `TypeSafe` System
//! One seat (Jev), the SAME adapter `nika compile --decision-model` seats
//! ([`super::TypesafeSeat`]).
//!
//! Selection is explicit: [`DECISION_ENV`]` = typesafe/<jev>` is read ONCE when a host door
//! opens the session (a launcher keeps it persistent), or a host hands a typed selection. The key
//! is read from `TYPESAFE_API_KEY` only after that selection and nothing prints it. A seat that
//! cannot be built (no key, a malformed model, another vendor) is a visible configuration
//! refusal (`/status`), never a silent absence.
//!
//! Consumption stays the compiler's: it asks only when its reading holds a finite ambiguity
//! (WARM) — a HOT, support or deterministic reading never calls — and it revalidates the answer
//! against the options it offered (NONE included). The host supplies its admission verdict;
//! the decision service is never charged to the author's API allowance or subscription. Each
//! need makes one attempt within the adapter's deadline, with no retry or implicit call cap.
//! A finished compile closes its scope, retaining unresolved sends as Uncertain.
//!
//! Every attempt is journaled before its request can leave and settled after it; the journal
//! rides the session's persisted `inference_observations` ([`DECISION_SCHEMA`]), outside any
//! allowance and outside the no-budget priced subtotal: its cost is unknown, never zero. An
//! attempt is one physical request: a batch's request is ONE attempt (`batch`, the `items` it
//! carried and their outcomes, its `usage` once). A request the service refuses for capacity
//! keeps its own attempt (`halved`), and each half of it asked again is another, naming the
//! attempt it `halves`; items no request carries are one attempt never sent.

use std::future::Future;
use std::sync::{Arc, Mutex};

use super::{Delivery, Exchange, ExchangeError, TypesafeSeat};
use nika_onboard::compile::decide::{
    BatchFuture, ChoiceAnswer, ChoiceBatch, ChoiceFuture, ChoiceQuestion, DecisionError,
    DecisionSeat, NONE_OPTION,
};
use serde_json::{Value, json};

/// The environment name of the operator's decision-seat selection (a seat name, never a key).
pub const DECISION_ENV: &str = "NIKA_SESSION_DECISION_MODEL";

/// The schema of a decision-seat observation in `inference_observations`.
pub const DECISION_SCHEMA: &str = "nika/session-decision-seat@2";

/// The seat's cost, said every time it is recorded.
const COST: &str = "unknown — the decision seat has no catalog tariff; never priced as zero; outside any Session allowance and the no-budget priced subtotal; invoice unknown";

/// What the seat decides — and what it does not.
const ROLE: &str = "typed compiler decisions: clause reading, feasible-plan ranking, semantic verification (the whole request, then each part alone, the task a missing part points to, extra operations and the questions over a trial run); NONE allowed; never Foundry or knowledge selection, never authority";

/// The operator-selected decision seat of one session. Its `Debug` names the seat and whether it
/// is ready — never the key, never the journal.
#[derive(Clone)]
pub struct DecisionSetup {
    word: String,
    source: &'static str,
    seat: Result<Arc<TypesafeSeat>, String>,
    journal: Arc<Mutex<Vec<Value>>>,
}

impl std::fmt::Debug for DecisionSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionSetup")
            .field("seat", &self.word)
            .field("source", &self.source)
            .field("refusal", &self.refusal())
            .finish_non_exhaustive()
    }
}

impl PartialEq for DecisionSetup {
    fn eq(&self, other: &Self) -> bool {
        self.word == other.word && self.source == other.source && self.refusal() == other.refusal()
    }
}

impl Eq for DecisionSetup {}

impl DecisionSetup {
    /// The seat the environment names ([`DECISION_ENV`]), read now; `None` when nothing is named.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        #[allow(clippy::disallowed_methods)]
        // a seat NAME, never a secret: the key is read only after this explicit selection
        let word = std::env::var(DECISION_ENV)
            .ok()
            .filter(|w| !w.trim().is_empty())?;
        Some(Self::build(&word, "environment", TypesafeSeat::from_env))
    }

    /// A host's typed selection with the key in hand (`None`: no key) and, for a gateway or a
    /// loopback test peer, an explicit endpoint.
    #[must_use]
    pub fn with_key(word: &str, key: Option<String>, base: Option<&str>) -> Self {
        Self::build(word, "host", |model| match key {
            None => Err("TYPESAFE_API_KEY is required for a typesafe decision seat".to_owned()),
            Some(key) => match base {
                Some(base) => TypesafeSeat::with_base(key, model, base),
                None => TypesafeSeat::new(key, model),
            },
        })
    }

    fn build(
        word: &str,
        source: &'static str,
        make: impl FnOnce(&str) -> Result<TypesafeSeat, String>,
    ) -> Self {
        let word = word.trim().to_owned();
        let seat = match word.strip_prefix("typesafe/") {
            Some(model)
                if !model.is_empty() && !model.chars().any(|c| c.is_whitespace() || c == '/') =>
            {
                make(model).map(Arc::new)
            }
            Some(_) => Err("the typesafe decision model is empty or malformed".to_owned()),
            None => Err(format!(
                "`{word}` is not a session decision seat — only typesafe/<jev> is wired here (e.g. typesafe/jev-1.13.0)"
            )),
        };
        Self {
            word,
            source,
            seat,
            journal: Arc::default(),
        }
    }

    /// The selected seat (`typesafe/jev-1.13.0`).
    #[must_use]
    pub fn model(&self) -> &str {
        &self.word
    }

    /// Where the selection came from: `environment` · `host`.
    #[must_use]
    pub fn source(&self) -> &'static str {
        self.source
    }

    /// Why the seat cannot be consulted at all, when it cannot.
    #[must_use]
    pub fn refusal(&self) -> Option<&str> {
        self.seat.as_ref().err().map(String::as_str)
    }

    /// The `/status` words: the seat, its endpoint and bounds — or its refusal.
    #[must_use]
    pub fn line(&self) -> String {
        nika_display::model_scope::decision_status(
            &self.word,
            self.source,
            self.seat
                .as_ref()
                .map(|seat| (seat.endpoint_host(), seat.timeout().as_secs()))
                .map_err(String::as_str),
        )
    }

    /// One observation per compile that needed the seat, as the session persists them.
    #[must_use]
    pub fn observations(&self) -> Vec<Value> {
        match self.journal.lock() {
            Ok(journal) => journal.clone(),
            Err(_) => vec![json!({
                "schema": DECISION_SCHEMA,
                "seat": self.word,
                "unbudgeted": true,
                "state": "Uncertain",
                "journal": "unreadable; calls may have been sent",
                "cost": COST,
            })],
        }
    }

    /// The seat the compiler consults for ONE compile; `allowed` is the money law's verdict.
    #[must_use]
    pub fn consult(&self, allowed: Result<(), String>) -> SessionSeat {
        SessionSeat {
            word: self.word.clone(),
            seat: self.seat.as_ref().ok().cloned(),
            allowed: match (&self.seat, allowed) {
                (Err(why), _) => Err(why.clone()),
                (Ok(_), verdict) => verdict,
            },
            journal: Arc::clone(&self.journal),
            entry: Mutex::new(None),
        }
    }
}

/// The journaling seat the compiler sees for one compile; consumption closes its scope.
pub struct SessionSeat {
    word: String,
    seat: Option<Arc<TypesafeSeat>>,
    allowed: Result<(), String>,
    journal: Arc<Mutex<Vec<Value>>>,
    entry: Mutex<Option<usize>>,
}

impl SessionSeat {
    /// Apply `edit` to this compile's observation (created on the first need); returns what
    /// `edit` returns, or `None` when the journal is unavailable.
    fn record<T>(&self, edit: impl FnOnce(&mut Value) -> T) -> Option<T> {
        let mut journal = self.journal.lock().ok()?;
        let mut entry = self.entry.lock().ok()?;
        let index = if let Some(index) = *entry {
            index
        } else {
            journal.push(json!({
                "schema": DECISION_SCHEMA,
                "kind": "decision_seat",
                "role": ROLE,
                "seat": self.word,
                "endpoint_host": self.seat.as_ref().map(|s| s.endpoint_host()),
                "selection": "operator-selected",
                "unbudgeted": true,
                "state": "Open",
                "max_calls": null,
                "scope_ended": false,
                "retries": 0,
                "calls_sent": 0,
                "unknown_calls": 0,
                "attempts": [],
                "cost": COST,
            }));
            let index = journal.len() - 1;
            *entry = Some(index);
            index
        };
        journal.get_mut(index).map(edit)
    }

    /// This compile's observation, when the compiler needed the seat.
    pub fn receipt(&self) -> Option<Value> {
        let journal = self.journal.lock().ok()?;
        let index = (*self.entry.lock().ok()?)?;
        journal.get(index).cloned()
    }

    /// End this compile's scope without turning an unknown price into a zero charge.
    #[must_use]
    pub fn finish(self) -> Option<Value> {
        self.close();
        self.receipt()
    }

    fn close(&self) {
        if self.receipt().is_some() {
            self.record(|o| {
                settle_state(o);
                o["scope_ended"] = json!(true);
                if o["state"] == "Open" {
                    o["state"] = json!("Closed");
                }
            });
        }
    }

    /// Record an attempt that never left, and return its error.
    fn unsent(&self, question: &ChoiceQuestion, outcome: &str, why: &str) -> DecisionError {
        self.record(|o| {
            push(
                o,
                json!({"question": question.id, "sent": false, "outcome": outcome, "error": why}),
            );
            if outcome == "refused" {
                o["refused"] = json!(why);
            }
        });
        DecisionError(format!("decision seat {} {why}", self.word))
    }

    /// Record a batch the seat was refused for as ONE attempt that never left, and return each
    /// item's error.
    fn refused(&self, batch: &ChoiceBatch, why: &str) -> Vec<Result<ChoiceAnswer, DecisionError>> {
        self.record(|o| {
            let items: Vec<Value> = (batch.items.iter())
                .map(|item| json!({"question": item.question.id, "options": item.question.keys()}))
                .collect();
            push(
                o,
                json!({"batch": batch.id, "items": items, "sent": false, "outcome": "refused",
                    "error": why}),
            );
            o["refused"] = json!(why);
        });
        let error = DecisionError(format!("decision seat {} {why}", self.word));
        vec![Err(error); batch.items.len()]
    }
}

fn push(observation: &mut Value, attempt: Value) -> usize {
    if let Some(attempts) = observation["attempts"].as_array_mut() {
        attempts.push(attempt);
        attempts.len() - 1
    } else {
        observation["attempts"] = json!([attempt]);
        0
    }
}

fn bump(observation: &mut Value, field: &str) {
    let n = observation[field].as_u64().unwrap_or(0);
    observation[field] = json!(n + 1);
}

fn unbump(observation: &mut Value, field: &str) {
    let n = observation[field].as_u64().unwrap_or(0);
    observation[field] = json!(n.saturating_sub(1));
}

/// Whether any sent attempt is still without a response: its charge is unknown.
fn settle_state(observation: &mut Value) {
    let unresolved = observation["attempts"].as_array().is_some_and(|attempts| {
        attempts.iter().any(|a| {
            a["sent"] == true && (a["outcome"] == "in_flight" || a["outcome"] == "transport_error")
        })
    });
    observation["state"] = json!(if unresolved { "Uncertain" } else { "Open" });
}

impl DecisionSeat for SessionSeat {
    fn name(&self) -> &str {
        &self.word
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            if let Err(why) = &self.allowed {
                return Err(self.unsent(question, "refused", &format!("not consulted: {why}")));
            }
            let Some(seat) = self.seat.clone() else {
                let why = "not consulted: the seat is unavailable";
                return Err(self.unsent(question, "refused", why));
            };
            let in_flight = json!({"question": question.id, "options": question.keys(),
                "sent": true, "outcome": "in_flight"});
            let settle = |result: &Result<Exchange, ExchangeError>| settled_one(question, result);
            let result = (self
                .attempt(in_flight, seat.exchange(question), settle)
                .await)
                .ok_or_else(|| DecisionError(UNAVAILABLE.into()))?;
            result
                .map(|exchange| exchange.answer)
                .map_err(|failure| failure.error)
        })
    }

    /// The batch's requests ([`TypesafeSeat::exchange_each`]), each journaled as ONE attempt
    /// before it can leave (one call sent, its cost unknown) and settled with its items'
    /// outcomes and its usage once. An empty batch asks nothing and records nothing.
    fn choose_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchFuture<'a> {
        Box::pin(async move {
            if batch.items.is_empty() {
                return Vec::new();
            }
            if let Err(why) = &self.allowed {
                return self.refused(batch, &format!("not consulted: {why}"));
            }
            let Some(seat) = self.seat.clone() else {
                return self.refused(batch, "not consulted: the seat is unavailable");
            };
            seat.exchanged(batch, self).await.answers
        })
    }
}

/// Why nothing was sent when the journal cannot record the attempt first.
const UNAVAILABLE: &str = "decision journal unavailable; nothing sent";

impl SessionSeat {
    /// One attempt journaled `in_flight` BEFORE its request can leave (one call sent, its cost
    /// unknown; an interruption leaves « may have been sent »), then settled with what `settle`
    /// makes of the result and how long it took (`elapsed_ms`). `None`: the journal is
    /// unavailable and nothing was sent.
    async fn attempt<T>(
        &self,
        in_flight: Value,
        send: impl Future<Output = T>,
        settle: impl FnOnce(&T) -> Value,
    ) -> Option<T> {
        let slot = self.begin_attempt(in_flight)?;
        let started = std::time::Instant::now();
        let result = send.await;
        let mut settled = settle(&result);
        settled["elapsed_ms"] = json!(u64::try_from(started.elapsed().as_millis()).ok());
        self.settle_attempt(slot, settled);
        Some(result)
    }

    /// Journal an attempt in flight: one call sent, its cost unknown. `None`: unavailable.
    fn begin_attempt(&self, in_flight: Value) -> Option<usize> {
        self.record(|o| {
            bump(o, "calls_sent");
            bump(o, "unknown_calls");
            let slot = push(o, in_flight);
            settle_state(o);
            slot
        })
    }

    /// Settle the attempt at `slot`: a request refused before any byte left is no sent call.
    fn settle_attempt(&self, slot: usize, settled: Value) {
        self.record(|o| {
            if settled["sent"] != true {
                unbump(o, "calls_sent");
                unbump(o, "unknown_calls");
            }
            if let Some(attempt) = o["attempts"].as_array_mut().and_then(|a| a.get_mut(slot)) {
                *attempt = settled;
            }
            settle_state(o);
        });
    }
}

/// The Session journals each physical request of a batch as its own attempt.
impl super::batch::Requests for SessionSeat {
    fn sending(&self, record: Value) -> Result<usize, DecisionError> {
        (self.begin_attempt(record)).ok_or_else(|| DecisionError(UNAVAILABLE.into()))
    }
    fn settled(&self, slot: usize, record: Value) {
        self.settle_attempt(slot, record);
    }
    fn unsent(&self, record: Value) {
        self.record(|o| push(o, record));
    }
}

/// The settled attempt of one question's request: its outcome, status, choice and usage, or how
/// far it went and why it failed (a malformed reply keeps the usage it reported).
fn settled_one(question: &ChoiceQuestion, result: &Result<Exchange, ExchangeError>) -> Value {
    match result {
        Ok(exchange) => {
            let answer = &exchange.answer;
            let outcome = if answer.choice == NONE_OPTION {
                "none"
            } else if question.keys().contains(&answer.choice) {
                "chosen"
            } else {
                "outside_options"
            };
            json!({
                "question": question.id,
                "options": question.keys(),
                "sent": true,
                "outcome": outcome,
                "status": exchange.status,
                "choice": answer.choice,
                "model": answer.model,
                "confidence": answer.confidence,
                "usage": {
                    "input_tokens": answer.input_tokens,
                    "output_tokens": answer.output_tokens,
                    "billing_units": exchange.billing_units,
                },
            })
        }
        Err(failure) => {
            let (outcome, status) = match failure.delivery {
                Delivery::Responded(status) => ("http_error", json!(status)),
                Delivery::NotSent => ("not_sent", Value::Null),
                _ => ("transport_error", Value::Null),
            };
            let mut failed = json!({
                "question": question.id,
                "options": question.keys(),
                "sent": failure.delivery != Delivery::NotSent,
                "outcome": outcome,
                "status": status,
                "error": failure.error.0,
            });
            if failure.usage.reported() {
                failed["usage"] = failure.usage.record();
            }
            failed
        }
    }
}

impl Drop for SessionSeat {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod batch_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::expect_used, clippy::panic, clippy::disallowed_methods)]
    fn a_poisoned_journal_refuses_before_transport() {
        let setup = DecisionSetup::with_key(
            "typesafe/jev-test",
            Some("fixture".into()),
            Some("http://127.0.0.1:1"),
        );
        let shared = Arc::clone(&setup.journal);
        assert!(
            std::thread::spawn(move || {
                let _guard = shared.lock().expect("journal");
                panic!("synthetic poison");
            })
            .join()
            .is_err()
        );
        let seat = setup.consult(Ok(()));
        let question = ChoiceQuestion::new("q", "pick", json!({}), vec![]);
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(seat.choose(&question));
        assert!(
            matches!(result, Err(DecisionError(why)) if why == "decision journal unavailable; nothing sent")
        );
        assert_eq!(setup.observations()[0]["state"], "Uncertain");
    }

    /// The observation names every question the seat may decide, a pointer asked for any missing
    /// part (never for a restriction alone) and the questions over a trial run among them; a
    /// consultation the money law refuses records its question unsent.
    #[test]
    #[allow(clippy::expect_used)]
    fn the_observation_names_every_question_the_seat_decides() {
        let setup = DecisionSetup::with_key(
            "typesafe/jev-test",
            Some("fixture".into()),
            Some("http://127.0.0.1:1"),
        );
        let seat = setup.consult(Err("the allowance is spent".into()));
        let question = ChoiceQuestion::new("verify-point-0", "pick", json!({}), vec![]);
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(seat.choose(&question));
        assert!(
            matches!(&result, Err(DecisionError(why)) if why == "decision seat typesafe/jev-test not consulted: the allowance is spent"),
            "{result:?}"
        );
        let observation = &setup.observations()[0];
        assert_eq!(
            observation["role"],
            "typed compiler decisions: clause reading, feasible-plan ranking, semantic verification (the whole request, then each part alone, the task a missing part points to, extra operations and the questions over a trial run); NONE allowed; never Foundry or knowledge selection, never authority"
        );
        assert_eq!(
            observation["attempts"],
            json!([{"question": "verify-point-0", "sent": false, "outcome": "refused",
                "error": "not consulted: the allowance is spent"}])
        );
        assert_eq!(
            observation["refused"],
            "not consulted: the allowance is spent"
        );
        assert_eq!(observation["calls_sent"], 0);
    }
}
