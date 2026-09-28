// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The money a host admitted or its operator stated, read before every strategy (R4 B15 · F3):
//! support, HOT, WARM, COLD and native all read the request with its directives blanked, the
//! seats included, and a ceiling no seat can be held to opens none — an admitted zero on every
//! door, any stated ceiling on a door that meters no seat, a ceiling a request read as written
//! still carries. The outcome records each directive beside the original request's identity,
//! and why a named seat stayed closed.
use nika_compile::surface::admitted;
use serde_json::{Value, json};

use crate::types::Input;
use crate::{CompileOutcome, CompileRequest, DiagnosticKind};

/// What the ladder reads of a request's money: the request it reads, the record of its
/// directives when it states any, and why its seats stay closed when they do.
pub(super) struct Money {
    pub(super) reading: CompileRequest,
    pub(super) record: Option<Value>,
    pub(super) closed: Option<String>,
}

/// The money of `request`, read before any strategy.
///
/// # Errors
/// The refused outcome of a span that is no directive, or of stated money that is malformed
/// or conflicting.
pub(super) fn read(request: &CompileRequest) -> Result<Money, Box<CompileOutcome>> {
    let read = replacement(request).unwrap_or_else(|| {
        admitted::read(request).map(|read| {
            read.map_or_else(
                || (request.clone(), None),
                |(reading, money)| (reading, Some(money)),
            )
        })
    });
    let (reading, record) = read.map_err(|why| Box::new(admitted::refused(&why)))?;
    let closed = record
        .as_ref()
        .and_then(|record| closed(record, request.stated_money));
    Ok(Money {
        reading,
        record,
        closed,
    })
}

/// On a door that meters no seat an `intent.clarification` answer replaces a creation's request:
/// its own directives are read afresh and blanked inside the answer, where the ladder reads it;
/// the words of the request it replaced state nothing. A revision's change is never replaced:
/// its money is read by [`admitted::read`].
fn replacement(
    request: &CompileRequest,
) -> Option<Result<(CompileRequest, Option<Value>), String>> {
    if !request.stated_money || !matches!(request.input, Input::Create(_)) {
        return None;
    }
    let raw = request.answers.get("intent.clarification")?;
    let text = serde_json::from_str::<Value>(raw)
        .ok()?
        .as_str()?
        .to_owned();
    let mut reading = request.clone();
    reading.stated_money = false;
    Some(
        admitted::replacement(request, &text).map(|read| match read {
            Some((blanked, money)) => {
                reading.answers.insert(
                    "intent.clarification".to_owned(),
                    json!(blanked).to_string(),
                );
                (reading, Some(money))
            }
            None => (reading, None),
        }),
    )
}

/// Why no seat may be consulted under the money a request states: an admitted zero on every
/// door, any stated ceiling on a door that meters no seat, a ceiling a request read as written
/// still carries (a seat would read it as work); `None` when a metering host holds its seats to
/// a positive ceiling itself.
fn closed(money: &Value, stated: bool) -> Option<String> {
    let amount = money["directives"]
        .as_array()?
        .iter()
        .find_map(|d| d["amount"].as_f64())?;
    if amount <= 0.0 {
        Some(
            "the stated ceiling of 0 USD admits no authoring request: no seat was consulted and no request was sent"
                .to_owned(),
        )
    } else if stated {
        Some(format!(
            "the stated ceiling of {amount} USD cannot bind an unpriced authoring seat on this door: no seat was consulted and no request was sent"
        ))
    } else if money["read_as_written"] == json!(true) {
        Some(format!(
            "the request is read as written, its ceiling of {amount} USD included, and no seat reads a ceiling as work: no seat was consulted and no request was sent"
        ))
    } else {
        None
    }
}

/// Record the money beside the outcome, and why a named seat stayed closed.
pub(super) fn record(
    request: &CompileRequest,
    money: Value,
    closed: Option<&str>,
    out: &mut CompileOutcome,
) {
    admitted::record(request, money, out);
    if let Some(why) = closed {
        crate::finding(out, DiagnosticKind::Applied, "authoring_money", why);
    }
}
