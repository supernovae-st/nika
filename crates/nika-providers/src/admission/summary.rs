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
