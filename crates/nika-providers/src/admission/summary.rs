// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Readable accounting observations; no text grants admission.
use super::{Cost, InferenceAdmission, InferenceReceipt};
use serde_json::Value;

impl InferenceAdmission {
    /// The account's observed monetary scope before transport, with the host's actual time.
    /// This is diagnostic text only; the host still owns the durable boundary and its marker.
    #[must_use]
    pub fn dispatch_note(&self, selected_model: Option<&str>, recorded_at: &str) -> String {
        let (route, bound) = match self.snapshot() {
            Ok(receipt) => receipt.dispatch_scope(selected_model.unwrap_or("the selected route")),
            Err(error) => ("an unreadable account".to_owned(), error.to_string()),
        };
        format!(
            "{route} · {bound} · recorded {recorded_at} before transport; no settlement followed, so its request(s) may have been sent and billed · usage and cost unknown"
        )
    }
}

impl InferenceReceipt {
    /// Monetary identity shown by a host before transport; no new authority.
    #[must_use]
    fn dispatch_scope(&self, selected_model: &str) -> (String, String) {
        match &self.unknown_cost {
            Some(choice) => (
                format!(
                    "{}/{} at {}",
                    choice.provider(),
                    choice.model(),
                    choice.origin().unwrap_or_else(|| "unknown origin".into())
                ),
                serde_json::to_value(choice)
                    .ok()
                    .and_then(|v| v["max_requests"].as_u64())
                    .map_or_else(
                        || "its bounded requests".to_owned(),
                        |n| format!("at most {n} request(s)"),
                    ),
            ),
            None => (
                selected_model.into(),
                format!("catalog allowance {}", self.limit),
            ),
        }
    }
    /// A dispatch refusal with its accounting scope; never a provider invoice.
    #[must_use]
    pub fn dispatch_refusal(&self) -> Option<String> {
        self.refusal.as_ref().map(|reason| {
            let scope = if self.unbudgeted {
                "no-budget observation"
            } else {
                "catalog admission"
            };
            format!("{scope}: {reason}; billed cost unknown")
        })
    }
    /// Human-readable numeric accounting, without implying an invoice or fresh authority.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.unknown_cost.is_some() {
            return format!(
                "explicit unknown cost · known USD subtotal {} · unknown calls {} · {:?} · invoice unknown · fresh review required for another invocation",
                self.estimated, self.unknown_calls, self.state
            );
        }
        let provenance = self.attempts.last().map_or_else(String::new, |a| {
            format!(
                " · {}/{} at {} · tariff {} ({})",
                a.tariff.provider, a.model, a.endpoint, a.tariff.source, a.tariff.as_of
            )
        });
        let refusal = self
            .refusal
            .as_ref()
            .map_or_else(String::new, |s| format!(" · {s}"));
        format!(
            "catalog admission: allowance {} · estimated {} · reserved {} · charge-unknown {} · available {} · {:?} · billed cost unknown{provenance}{refusal}",
            self.limit, self.estimated, self.active, self.held_unknown, self.available, self.state
        )
    }
}

/// Summarize selected priced, unbudgeted observations and host-recorded interruptions.
/// The caller excludes other seats' observations; unknown evidence stays unknown.
#[must_use]
pub fn unbudgeted_summary(observed: &[&Value], interrupted: usize) -> Option<String> {
    let sent: usize = observed
        .iter()
        .filter_map(|o| o["attempts"].as_array())
        .map(|attempts| attempts.iter().filter(|a| a["sent"] == true).count())
        .sum();
    if sent == 0 && interrupted == 0 {
        return None;
    }
    let estimate = observed
        .iter()
        .try_fold(0i128, |total, o| {
            o["known_subtotal_nano_usd"]
                .as_str()?
                .parse::<i128>()
                .ok()?
                .checked_add(total)
        })
        .map_or_else(|| "unreadable".to_owned(), |n| Cost::new(n).to_string());
    let unsettled: u64 = observed
        .iter()
        .filter_map(|o| o["unknown_calls"].as_u64())
        .sum();
    let interrupted = match interrupted {
        0 => String::new(),
        n => format!(
            " · {n} no-budget dispatch(es) left without a recorded settlement may have been billed; usage and cost unknown"
        ),
    };
    Some(format!(
        "no-budget observation (outside any allowance or cap): {sent} priced call(s) sent · catalog estimate {estimate} of complete usage · {unsettled} without usable settlement · invoice unknown{interrupted}"
    ))
}

/// The host's diagnostic when no callable account exists. Names no new authority.
#[must_use]
pub fn unadmitted_summary(note: Option<&str>, refusal: Option<&str>, zero: bool) -> String {
    note.map(str::to_owned)
        .or_else(|| {
            refusal.map(|reason| format!("earlier Session monetary admission refused: {reason}"))
        })
        .unwrap_or_else(|| {
            if zero {
                "catalog allowance is zero; no paid inference admitted; billed cost unknown"
            } else {
                "no qualified catalog admission account; billed cost unknown"
            }
            .into()
        })
}

/// Diagnostic text for a host's durable no-budget dispatch marker, at the host's actual time.
/// The host owns the marker and its persistence; this text never grants an allowance.
#[must_use]
pub fn unbudgeted_dispatch_note(model: &str, decision: Option<&str>, recorded_at: &str) -> String {
    let decision = decision.map_or_else(String::new, |seat| {
        format!(
            " · the operator-selected decision seat {seat} may also have been called (cost unknown)"
        )
    });
    format!(
        "{model}{decision} · no Session budget: observed, no allowance or cap · recorded {recorded_at} before transport; no settlement followed, so its request(s) may have been sent and billed · usage and cost unknown"
    )
}

/// The versioned complete numeric checkpoint or completed-scope report for this project.
/// A codec refusal is retained as diagnostic data, never converted into an empty account.
#[must_use]
pub fn accounting_checkpoint(
    account: Option<&InferenceAdmission>,
    observations: &[Value],
    project: &[u8],
) -> Option<Value> {
    let numeric = || {
        account.map(|a| {
            a.checkpoint(project)
                .unwrap_or_else(|e| Value::String(e.to_string()))
        })
    };
    if account.is_some_and(|a| {
        !a.snapshot()
            .is_ok_and(|r| r.state == super::AdmissionState::Closed && r.unknown_cost.is_some())
    }) {
        return numeric();
    }
    super::CompletedCostReport::read(observations)
        .map_or_else(|_| numeric(), |report| Some(report.checkpoint(project)))
}

/// The decision seat's line: what it was asked, sent and refused; its cost unknown, never zero.
#[must_use]
pub fn decision_summary(observations: &[serde_json::Value], schema: &str) -> Option<String> {
    let seats: Vec<&serde_json::Value> = observations
        .iter()
        .filter(|o| is_decision_observation(o, schema))
        .collect();
    if seats.is_empty() {
        return None;
    }
    let attempts: Vec<&serde_json::Value> = seats
        .iter()
        .filter_map(|o| o["attempts"].as_array())
        .flatten()
        .collect();
    let count = |outcome: &str| attempts.iter().filter(|a| a["outcome"] == outcome).count();
    let sent = attempts.iter().filter(|a| a["sent"] == true).count();
    let unresolved = count("in_flight") + count("transport_error");
    let refused = count("refused") + count("capped");
    let mut names: Vec<&str> = seats.iter().filter_map(|o| o["seat"].as_str()).collect();
    names.dedup();
    let usage: u64 = attempts
        .iter()
        .filter_map(|a| a["usage"]["input_tokens"].as_u64())
        .chain(
            attempts
                .iter()
                .filter_map(|a| a["usage"]["output_tokens"].as_u64()),
        )
        .sum();
    Some(format!(
        "decision seat {} (operator-selected, outside any allowance or cap): {sent} call(s) sent · {} answered · {unresolved} without a response · {refused} need(s) refused before sending · {usage} token(s) reported · cost unknown (no catalog tariff), never zero; not in the no-budget subtotal · invoice unknown",
        names.join(", "),
        count("chosen") + count("none") + count("outside_options")
    ))
}

/// Exact historical and current decision-seat observation schemas, never an admission judgment.
#[must_use]
pub fn is_decision_observation(observation: &Value, current: &str) -> bool {
    observation["schema"] == current || observation["schema"] == "nika/session-decision-seat@1"
}

/// No-budget observations filtered by their exact provenance; companion decisions stay separate.
#[must_use]
pub fn unbudgeted_observation_summary(
    observations: &[Value],
    decision_schema: &str,
    interrupted: usize,
) -> Option<String> {
    let observed: Vec<_> = observations
        .iter()
        .filter(|o| o["unbudgeted"] == true && !is_decision_observation(o, decision_schema))
        .collect();
    unbudgeted_summary(&observed, interrupted)
}

/// Compose the existing informational projections without changing any admission or observation.
#[must_use]
pub fn observation_details(costs: &[Value], history: &[Value], decision_schema: &str) -> String {
    let decision = decision_summary(costs, decision_schema)
        .map_or_else(String::new, |line| format!(" · {line}"));
    let unread = match costs.iter().filter(|o| !o.is_object()).count() {
        0 => String::new(),
        n => format!(" · {n} cost observation(s) unreadable: never read as settled"),
    };
    let legacy = super::LegacyCostReport::summary_of(history)
        .or_else(|| super::CompletedCostReport::summary_of(history))
        .map(|line| format!(" · {line}"))
        .unwrap_or_default();
    format!("{unread}{decision}{legacy}")
}

/// Presentation of a host's retained account facts; this never admits a request.
#[must_use]
pub fn account_status(
    receipt: Result<Option<InferenceReceipt>, String>,
    reconfirm: bool,
    historical: usize,
    interrupted: &str,
    note: Option<&str>,
    refusal: Option<&str>,
    zero: bool,
) -> String {
    match receipt {
        Ok(Some(receipt)) => receipt.summary(),
        Ok(None) if reconfirm => format!(
            "restored inference exposure is unknown; no new catalog allowance can be inferred; billed cost unknown · {historical} historical cost observation(s), without authority{interrupted}"
        ),
        Ok(None) => unadmitted_summary(note, refusal, zero),
        Err(error) => format!("catalog admission unavailable: {error}; no paid call admitted"),
    }
}
/// Human-facing scope over supplied facts; subscription selection does not restore API admission.
#[must_use]
pub fn inference_summary(
    account: &str,
    observed: Option<&str>,
    details: &str,
    subscription: bool,
    gate: bool,
) -> String {
    let scope = if subscription {
        "Subscription invoice unknown; retained API accounting (not its admission)"
    } else {
        "Session inference (separate from proposal/Run)"
    };
    let observed = observed.map_or_else(String::new, |line| format!(" · {line}"));
    let account = format!("{scope}: {account}{observed}{details}");
    if gate {
        format!(
            "confirm-gate monetary amendment held; no paid inference admitted; paused Run unchanged; answer yes or no separately\n{account}"
        )
    } else {
        account
    }
}
/// The host's unsettled-dispatch count remains unknown, never a synthetic zero price.
#[must_use]
pub fn interrupted_note(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!(
            " · {count} paid dispatch(es) left without a recorded settlement may have been billed; usage and cost unknown"
        )
    }
}
