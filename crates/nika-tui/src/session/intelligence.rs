// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native conversation needs an intelligence. The Session still owns parsing,
//! selection and any interrupted request; this host cannot opt into its offline mode.

use super::{Live, SessionRuntime, TurnOutcome};
use nika_session::intelligence::IntelligenceKind;

mod groups;
pub(super) use groups::project;

pub(super) fn ready(runtime: &SessionRuntime) -> bool {
    runtime.intelligence_chosen()
        && runtime.intelligence.ready
        && !matches!(runtime.intelligence.kind, IntelligenceKind::None)
}

pub(super) fn guidance(runtime: &SessionRuntime) -> String {
    let available = runtime
        .work()
        .intelligence
        .is_some_and(|i| !i.options.is_empty());
    let mut words = if available && ready(runtime) {
        "Choose an intelligence below. To name a model, use /intelligence 2 provider/model on a configured connection."
    } else if available {
        "Choose an intelligence below. A message typed here waits for your choice. To name a model, use /intelligence 2 provider/model on a configured connection."
    } else {
        "This conversation needs an AI connection. Connect an account or configure a provider, then reopen Nika. /providers shows the current connections."
    }.to_owned();
    if let Some(why) = runtime
        .intelligence
        .why
        .as_deref()
        .filter(|_| !ready(runtime))
    {
        words.push('\n');
        words.push_str(why);
    }
    words
}

impl Live {
    /// Refuse only the offline selection and an initial cancellation. Neither can erase the
    /// Session's kept request; cancelling a change with a usable current route stays ordinary.
    pub(super) fn require_intelligence(&mut self, line: &str) -> Option<TurnOutcome> {
        let runtime = self.runtime.as_mut()?;
        let trimmed = line.trim();
        let direct = trimmed.strip_prefix("/intelligence ");
        if !runtime.pending_choice() && direct.is_none() {
            return None;
        }
        let answer = direct.unwrap_or(trimmed).trim();
        if runtime.pending_choice() {
            if trimmed == "/intelligence" {
                self.intelligence_group = None;
            } else if let Some(group) = groups::requested(&runtime.work(), trimmed) {
                let guidance = groups::guidance(&runtime.work(), &group);
                self.intelligence_group = Some(group);
                return Some(TurnOutcome::Facts(guidance.to_owned()));
            }
        }
        let offline = self
            .census
            .choose(answer)
            .is_ok_and(|pref| matches!(pref.kind, IntelligenceKind::None));
        let cannot_cancel = !ready(runtime) && nika_session::authoring::is_cancel(answer);
        if !offline && !cannot_cancel {
            if runtime.pending_choice() && !ready(runtime) {
                return Some(runtime.request_before_intelligence(line));
            }
            return None;
        }
        if !runtime.pending_choice() {
            let _ = runtime.turn("/intelligence");
        }
        Some(TurnOutcome::Facts(if offline {
            "An AI connection is required for this conversation. The offline choice was not applied; your current selection and any waiting message are kept."
                .to_owned()
        } else {
            "Choose an AI connection to continue. Your waiting message is kept while this guide remains open."
                .to_owned()
        }))
    }
}

/// Hermetic host-mechanics fixtures select a configured route whose injected reasoner never
/// uses a provider. This exercises the same native opening contract as a real selection.
#[cfg(test)]
pub(crate) fn fixture() -> (super::IntelligenceCensus, super::UserIntelligencePreference) {
    let mut census = super::IntelligenceCensus::empty();
    census.locals.push("ollama".into());
    let preference = super::UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".into(),
        },
        None,
    );
    (census, preference)
}
