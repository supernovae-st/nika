// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a route's session offers, read without a prompt.
//!
//! A run learns what its session offers on the way to its prompt and says so
//! only when it refuses. This module opens the session a run would open (the
//! registry adapter, its identity judged at `initialize`, the one-shot
//! completion profile proven for an `infer:` lane), selects the model the
//! caller names, for that session only, so the options that depend on the
//! model are the ones that model has, and reads three offers from the
//! complete configuration the session answered: the models it lists, the
//! values of its one reasoning option and the service tiers its measured
//! speed option carries. Then the session ends.
//!
//! Nothing else is set: no effort, no tier, no mode. This path holds no
//! `session/prompt`, so an offer costs the route no turn.
//!
//! An offer is one session's answer at one time. It grants nothing: a run
//! selects, reads back and refuses on its own session exactly as before,
//! whatever an offer said a moment earlier.

use std::time::Duration;

use nika_kernel::ai::harness::HarnessError;
use nika_types::access::ServiceTier;

use crate::authoring::acp::Completion;

/// How many values of one list an offer keeps ([`SessionOffer::models`],
/// [`EffortOffer::Offered::values`]). A session that lists more has the rest
/// counted, never carried.
pub const MAX_OFFERED_VALUES: usize = 64;

/// How long one kept value may be, in bytes. A longer one is counted as
/// omitted: a model or effort id is a short token, never a text.
pub const MAX_OFFERED_VALUE_BYTES: usize = 160;

/// The session shape an offer is read on: the two shapes a workflow's tasks
/// run in on a harness route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum OfferProfile {
    /// The route's audited one-shot completion profile, which an `infer:`
    /// task runs under ([`crate::meet_acp_one_shot`]).
    Completion,
    /// The route's agent session, which an `agent:` task runs in.
    Agent,
}

impl OfferProfile {
    /// The `snake_case` wire form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completion => "completion",
            Self::Agent => "agent",
        }
    }
}

/// What the session answered about the model the caller named.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelOffer {
    /// No model was named (`default`, or none): nothing was selected and the
    /// session's own model stands.
    SessionDefault {
        /// The model the session states it serves, when it states one.
        current: Option<String>,
    },
    /// The session lists the model, took the selection and reads it back.
    Confirmed {
        /// The model's native id, as the session reads it back.
        native: String,
    },
    /// Only the session's legacy model list names the model. A selection
    /// through that list answers no configuration, so nothing that depends
    /// on the model can be read and nothing was selected.
    LegacyList {
        /// The model's native id in the legacy list.
        native: String,
    },
    /// The session lists no such model. Nothing was selected.
    NotOffered,
    /// The session lists the model and answered the selection with another
    /// model, or with none.
    NotConfirmed {
        /// The model the session reported after the selection.
        reported: Option<String>,
    },
}

impl ModelOffer {
    /// Whether the configuration read after this answer describes the model
    /// the caller named (or the session's own, when none was named).
    #[must_use]
    pub const fn holds(&self) -> bool {
        matches!(self, Self::SessionDefault { .. } | Self::Confirmed { .. })
    }
}

/// The reasoning efforts the session lists for the model.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EffortOffer {
    /// The session's one reasoning option.
    Offered {
        /// The option's own id (codex-acp: `reasoning_effort`).
        option: String,
        /// Its values, verbatim, in advertised order, at most
        /// [`MAX_OFFERED_VALUES`]. A run accepts exactly these on
        /// `run.reasoning.effort`.
        values: Vec<String>,
        /// How many listed values were left out of `values`.
        omitted: usize,
        /// The value the session holds now: what a run that names no effort
        /// would keep on a session like this one.
        current: Option<String>,
    },
    /// The session lists no reasoning option for the model.
    NotOffered,
    /// The efforts were not read, and why: the model does not hold, two
    /// reasoning options are ambiguous, or the option is not a select.
    Unobserved {
        /// The reason, in the client's words.
        reason: String,
    },
}

/// The service tiers the session's measured speed option carries for the model.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceTierOffer {
    /// The measured option is listed for the model.
    Offered {
        /// The option's own id (codex-acp: `fast-mode`).
        option: String,
        /// The language's words whose native value the option lists.
        tiers: Vec<ServiceTier>,
        /// The word the session holds now, when its native value is one of
        /// the two measured ones.
        current: Option<ServiceTier>,
    },
    /// The adapter and its release are measured and the session does not
    /// list the option for the model.
    NotOffered,
    /// The tiers were not read, and why: the model does not hold, no speed
    /// control is measured for this adapter or this release, or the option
    /// is advertised in another form.
    Unobserved {
        /// The reason, in the client's words.
        reason: String,
    },
}

/// What one session offered, read without a prompt (see the module doc).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionOffer {
    /// The adapter's own name on `initialize` (`agentInfo.name`).
    pub agent_name: Option<String>,
    /// The adapter's own release on `initialize` (`agentInfo.version`).
    pub agent_version: Option<String>,
    /// The model values the session lists, verbatim, in advertised order: its
    /// model option's choices, then its legacy list's ids, at most
    /// [`MAX_OFFERED_VALUES`].
    pub models: Vec<String>,
    /// How many listed models were left out of `models`.
    pub models_omitted: usize,
    /// The answer about the model the caller named.
    pub model: ModelOffer,
    /// The reasoning efforts listed for that model.
    pub effort: EffortOffer,
    /// The service tiers listed for that model.
    pub service_tier: ServiceTierOffer,
}

impl SessionOffer {
    /// Construct (INV-019): the three answers, with no adapter identity and
    /// no model list until [`Self::with_agent`] and [`Self::with_models`].
    #[must_use]
    pub const fn new(
        model: ModelOffer,
        effort: EffortOffer,
        service_tier: ServiceTierOffer,
    ) -> Self {
        Self {
            agent_name: None,
            agent_version: None,
            models: Vec::new(),
            models_omitted: 0,
            model,
            effort,
            service_tier,
        }
    }

    /// Record the adapter's own name and release.
    #[must_use]
    pub fn with_agent(mut self, name: Option<String>, version: Option<String>) -> Self {
        self.agent_name = name;
        self.agent_version = version;
        self
    }

    /// Record the models the session lists, bounded like every offered list.
    #[must_use]
    pub fn with_models(mut self, listed: Vec<String>) -> Self {
        let (models, omitted) = bounded(listed);
        self.models = models;
        self.models_omitted = omitted;
        self
    }
}

/// Keep at most [`MAX_OFFERED_VALUES`] values of at most
/// [`MAX_OFFERED_VALUE_BYTES`] bytes each, in order, and count the rest.
#[must_use]
pub fn bounded(listed: Vec<String>) -> (Vec<String>, usize) {
    let total = listed.len();
    let kept: Vec<String> = listed
        .into_iter()
        .filter(|value| !value.is_empty() && value.len() <= MAX_OFFERED_VALUE_BYTES)
        .take(MAX_OFFERED_VALUES)
        .collect();
    let omitted = total - kept.len();
    (kept, omitted)
}

/// Read what the route `seat` offers under `profile` for `model`, on a fresh
/// session that receives no prompt and is ended before this returns.
///
/// `seat` is a registry id (`codex`, `claude-code`); `model` is the model as
/// a workflow names it (`provider/name`), `None` or `default` for the
/// session's own. `timeout` bounds the whole reading, the adapter's start
/// included; when it passes, the session and its process group are ended.
///
/// # Errors
///
/// What a run on the same route meets before its prompt: the adapter is
/// absent, silent or outside its pin ([`HarnessError::Unavailable`]), its
/// identity or the completion profile is refused, or the session dies. A
/// model, an effort or a tier the session does not list is never an error
/// here: it is the answer ([`ModelOffer`], [`EffortOffer`],
/// [`ServiceTierOffer`]).
pub async fn session_offer(
    seat: &str,
    profile: OfferProfile,
    model: Option<&str>,
    timeout: Duration,
) -> Result<SessionOffer, HarnessError> {
    let harness = crate::seat_from_id(seat)
        .map_err(|reason| HarnessError::Unavailable { reason })?
        .ok_or_else(|| HarnessError::Unavailable {
            reason: format!("no registry row builds the `{seat}` ACP adapter"),
        })?;
    let harness = match profile {
        OfferProfile::Completion => harness.for_completion(Completion::Infer)?,
        OfferProfile::Agent => harness,
    };
    harness.offer(model, timeout).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_is_kept_in_order_up_to_the_bound_and_the_rest_is_counted() {
        let listed: Vec<String> = (0..MAX_OFFERED_VALUES + 3)
            .map(|n| format!("model-{n}"))
            .collect();
        let (kept, omitted) = bounded(listed);
        assert_eq!(kept.len(), MAX_OFFERED_VALUES);
        assert_eq!(kept.first().map(String::as_str), Some("model-0"));
        assert_eq!(kept.last().map(String::as_str), Some("model-63"));
        assert_eq!(omitted, 3);
    }

    #[test]
    fn an_empty_or_over_long_value_is_counted_and_never_carried() {
        let long = "m".repeat(MAX_OFFERED_VALUE_BYTES + 1);
        let exact = "m".repeat(MAX_OFFERED_VALUE_BYTES);
        let (kept, omitted) = bounded(vec![
            "gpt-5.5".to_owned(),
            String::new(),
            long,
            exact.clone(),
        ]);
        assert_eq!(kept, vec!["gpt-5.5".to_owned(), exact]);
        assert_eq!(omitted, 2);
    }

    #[test]
    fn only_a_default_or_a_confirmed_model_holds_for_what_is_read_after() {
        assert!(ModelOffer::SessionDefault { current: None }.holds());
        assert!(
            ModelOffer::Confirmed {
                native: "gpt-5.5".to_owned()
            }
            .holds()
        );
        assert!(!ModelOffer::NotOffered.holds());
        assert!(!ModelOffer::NotConfirmed { reported: None }.holds());
        assert!(
            !ModelOffer::LegacyList {
                native: "gpt-5.5".to_owned()
            }
            .holds()
        );
    }
}
