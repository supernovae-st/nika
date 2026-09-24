// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The bounded decision seat a session may consult — one operator-selected `TypeSafe` System
//! One seat (Jev), the SAME adapter `nika compile --decision-model` seats
//! ([`nika_cli_host::compile::typesafe::TypesafeSeat`]).
//!
//! Selection is explicit: [`DECISION_ENV`]` = typesafe/<jev>` is read ONCE when a host door
//! opens the session (a launcher keeps it persistent), or a host hands a typed selection. The key
//! is read from `TYPESAFE_API_KEY` only after that selection and nothing prints it. A seat that
//! cannot be built (no key, a malformed model, another vendor) is a visible configuration
//! refusal (`/status`), never a silent absence.
//!
//! Consumption stays the compiler's: it asks only when its reading holds a finite ambiguity
//! (WARM) — a HOT, support or deterministic reading never calls — and it revalidates the answer
//! against the options it offered (NONE included). The seat is consulted only under an API model
//! seat whose dispatch rides the session's no-budget observation: a numeric allowance, a zero
//! allowance, an unknown-cost scope or a closed account is never charged with the seat's unknown
//! cost, and a need met there is answered with a refusal the outcome records — never a claim of
//! use. At most [`MAX_DECISION_CALLS`] calls per compile, one attempt each within the adapter's
//! deadline; no retry.
//!
//! Every attempt is journaled before its request can leave and settled after it; the journal
//! rides the session's persisted `inference_observations` ([`DECISION_SCHEMA`]), outside any
//! allowance and outside the no-budget priced subtotal: its cost is unknown, never zero.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nika_cli_host::compile::typesafe::{Delivery, TypesafeSeat};
use nika_onboard::compile::decide::{
    ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat, NONE_OPTION,
};
use nika_providers::{AdmissionState, InferenceAdmission};
use serde_json::{Value, json};

/// The environment name of the operator's decision-seat selection (a seat name, never a key).
pub const DECISION_ENV: &str = "NIKA_SESSION_DECISION_MODEL";

/// The calls one compile may make to the decision seat; the next need is refused unsent.
pub const MAX_DECISION_CALLS: usize = 3;

/// The schema of a decision-seat observation in `inference_observations`.
pub const DECISION_SCHEMA: &str = "nika/session-decision-seat@1";

/// The seat's cost, said every time it is recorded.
const COST: &str = "unknown — the decision seat has no catalog tariff; never priced as zero; outside any Session allowance and the no-budget priced subtotal; invoice unknown";

/// What the seat decides — and what it does not.
const ROLE: &str = "compiler routing: which operation an ambiguous clause asks for (WARM), NONE allowed; never Foundry or knowledge selection, never authority";

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
        match &self.seat {
            Ok(seat) => format!(
                "decision seat {} ({}, operator-selected) · {} · compiler routing of an ambiguous clause, never knowledge selection · at most {MAX_DECISION_CALLS} call(s) per seated compile, one attempt each, {} s deadline, no retry · consulted only for a finite ambiguity under an API seat without a numeric allowance · cost unknown, never zero",
                self.word,
                self.source,
                seat.endpoint_host(),
                seat.timeout().as_secs()
            ),
            Err(why) => format!(
                "decision seat {} ({}) · refused: {why}",
                self.word, self.source
            ),
        }
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
    pub(crate) fn consult(&self, allowed: Result<(), String>) -> SessionSeat {
        SessionSeat {
            word: self.word.clone(),
            seat: self.seat.as_ref().ok().cloned(),
            allowed: match (&self.seat, allowed) {
                (Err(why), _) => Err(why.clone()),
                (Ok(_), verdict) => verdict,
            },
            calls: AtomicUsize::new(0),
            journal: Arc::clone(&self.journal),
            entry: Mutex::new(None),
        }
    }
}

/// The money law for one seated compile: the seat may be charged only on the session's no-budget
/// observation (an operator-selected, explicitly unbudgeted use), never against a number.
pub(crate) fn admit(admission: Option<&InferenceAdmission>) -> Result<(), String> {
    let Some(account) = admission else {
        return Err("the selected intelligence is not a priced API route this session observes (local, subscription or unpriced); an external decision service is not consulted".to_owned());
    };
    let receipt = account
        .snapshot()
        .map_err(|e| format!("the Session account is unreadable ({e}); no decision call"))?;
    if receipt.state != AdmissionState::Open {
        return Err(format!(
            "the Session account is {:?}; no further paid call, the decision seat included",
            receipt.state
        ));
    }
    if !receipt.unbudgeted {
        return Err("this Session holds a numeric allowance or an unknown-cost scope; the decision seat has no catalog tariff, so its unknown cost is never charged against it".to_owned());
    }
    Ok(())
}

/// The capped, journaling seat the compiler sees for one compile.
pub(crate) struct SessionSeat {
    word: String,
    seat: Option<Arc<TypesafeSeat>>,
    allowed: Result<(), String>,
    calls: AtomicUsize,
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
                "max_calls": MAX_DECISION_CALLS,
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
    pub(crate) fn receipt(&self) -> Option<Value> {
        let journal = self.journal.lock().ok()?;
        let index = (*self.entry.lock().ok()?)?;
        journal.get(index).cloned()
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
                return Err(self.unsent(
                    question,
                    "refused",
                    "not consulted: the seat is unavailable",
                ));
            };
            if self.calls.fetch_add(1, Ordering::SeqCst) >= MAX_DECISION_CALLS {
                return Err(self.unsent(
                    question,
                    "capped",
                    &format!(
                        "reached its cap of {MAX_DECISION_CALLS} call(s) for this request; not sent"
                    ),
                ));
            }
            // Journaled BEFORE the request can leave: an interruption leaves « may have been sent ».
            let slot = self.record(|o| {
                bump(o, "calls_sent");
                bump(o, "unknown_calls");
                let slot = push(
                    o,
                    json!({
                        "question": question.id,
                        "options": question.keys(),
                        "sent": true,
                        "outcome": "in_flight",
                    }),
                );
                settle_state(o);
                slot
            });
            let result = seat.exchange(question).await;
            self.record(|o| {
                let settled = match &result {
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
                    Err(failure) => json!({
                        "question": question.id,
                        "options": question.keys(),
                        "sent": failure.delivery != Delivery::NotSent,
                        "outcome": match failure.delivery {
                            Delivery::Responded(_) => "http_error",
                            Delivery::NotSent => "not_sent",
                            _ => "transport_error",
                        },
                        "status": match failure.delivery {
                            Delivery::Responded(status) => json!(status),
                            _ => Value::Null,
                        },
                        "error": failure.error.0,
                    }),
                };
                let left = settled["sent"] == true;
                if let (Some(slot), Some(attempts)) = (slot, o["attempts"].as_array_mut())
                    && let Some(attempt) = attempts.get_mut(slot)
                {
                    *attempt = settled;
                }
                if !left {
                    // refused by the adapter before any byte left: not a sent call after all
                    unbump(o, "calls_sent");
                    unbump(o, "unknown_calls");
                }
                settle_state(o);
            });
            result
                .map(|exchange| exchange.answer)
                .map_err(|failure| failure.error)
        })
    }
}

#[cfg(test)]
pub(crate) mod tests;
