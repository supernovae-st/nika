// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Readable accounting observations; no text grants admission.
use super::{Cost, InferenceReceipt};
use serde_json::Value;

impl InferenceReceipt {
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
