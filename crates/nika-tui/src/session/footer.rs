// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Scope a reply's footer separately from retained Run facts. Facts can contain
//! an incomplete preparation: presentation must not infer success from that variant.
use super::{Beat, Live, Stage, TurnOutcome};

pub(super) fn reply_label(outcome: &TurnOutcome) -> Option<&'static str> {
    match outcome {
        TurnOutcome::Facts(_)
        | TurnOutcome::Reply(_)
        | TurnOutcome::Aside(_)
        | TurnOutcome::Help(_) => Some("Latest reply above"),
        TurnOutcome::Refusal(_) => Some("Latest turn refused"),
        TurnOutcome::Cancelled(_) => Some("Preparation stopped"),
        _ => None,
    }
}

impl Live {
    /// Only a quiet reply beside a retained Run needs the distinction. A pending
    /// proposal, question or gate keeps its own actionable status, unchanged.
    pub(super) fn reply_footer(&self, label: Option<&str>) -> Vec<Beat> {
        let mut beats = self.footer();
        let run = self
            .runtime
            .as_ref()
            .is_some_and(|r| r.lifecycle().run != Stage::Pending)
            || self
                .kept
                .as_ref()
                .is_some_and(|r| r.as_ref().is_ok_and(|r| r.exit.is_some()));
        if let Some(label) = label.filter(|_| run && self.quiet_prompt()) {
            for beat in &mut beats {
                if let Beat::Status(status) = beat {
                    *status = format!("{label} · {status}");
                }
            }
        }
        beats
    }
}
