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
