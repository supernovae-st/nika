// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the native picker changes beside the author model: which decision seat this
//! conversation's finite choices consult. The Jev seat is only ever the one its host named (its
//! environment when the session opened, or a context it handed since); choosing it, or the
//! author alone, takes effect from the next turn through the authoring context. Nothing here
//! calls a model or writes a file.

use super::{SessionRuntime, TurnOutcome};
use crate::authoring::{AuthoringContext, DecisionSetup};
use crate::intelligence::{AUTHOR_ALONE, IntelligenceKind, UserIntelligencePreference};
use crate::outcome::{Refusal, RefusalClass};

/// The decision seat a conversation consults for its finite choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecisionChoice {
    /// The Jev seat the host named: typed decisions.
    Jev,
    /// No seat: the author decides alone.
    AuthorAlone,
}

impl SessionRuntime {
    /// The Jev seat this session can consult, the one its host named, with its refusal when it
    /// cannot be consulted here; `None` when no host named one.
    #[must_use]
    pub fn jev_seat(&self) -> Option<&DecisionSetup> {
        self.jev_seat.as_ref()
    }

    /// This conversation's decision seat from its next turn: Jev, or the author alone. Applied
    /// through the authoring context, a refusal the earlier seat caused cleared, and held for
    /// this conversation; nothing is written or called. Jev is refused, the current choice
    /// unchanged, while no host named a seat or while it cannot be consulted (a missing key);
    /// the author alone is never refused.
    pub fn choose_decision(&mut self, choice: DecisionChoice) -> TurnOutcome {
        let (seat, words) = match choice {
            DecisionChoice::AuthorAlone => (
                None,
                "From the next turn the author decides alone: no decision seat is consulted."
                    .to_owned(),
            ),
            DecisionChoice::Jev => {
                let Some(seat) = self.jev_seat.clone() else {
                    return refused(
                        "Jev is unavailable: no decision seat is configured here. Name one with \
                         NIKA_SESSION_DECISION_MODEL (typesafe/<jev>) and TYPESAFE_API_KEY, then \
                         reopen Nika. The current choice is kept.",
                    );
                };
                if let Some(why) = seat.refusal() {
                    return refused(&format!(
                        "Jev is unavailable: {why}. The current choice is kept."
                    ));
                }
                let words = format!(
                    "From the next turn {} makes this conversation's typed decisions.",
                    seat.model()
                );
                (Some(seat), words)
            }
        };
        self.authoring_context = (self.authoring_context.clone()).with_decision_replaced(seat);
        TurnOutcome::Facts(words)
    }
}

impl SessionRuntime {
    /// The operator's saved default, as read when the session opened or saved since.
    #[must_use]
    pub fn operator_default(&self) -> Option<&UserIntelligencePreference> {
        self.operator_default.as_ref()
    }

    /// Keep this conversation's intelligence as the operator's default for new conversations:
    /// the kind and model in use, the decision seat it consults (by name) or the author alone,
    /// and the time, saved atomically under the home (`~/.nika/session-intelligence.json`). No
    /// key or secret value is written, and this conversation is unchanged. Refused, nothing
    /// written, without a home or a usable intelligence.
    pub fn save_operator_default(&mut self) -> TurnOutcome {
        let Some(home) = self.home.clone() else {
            return refused("No home directory is available here: nothing was saved.");
        };
        let usable = self.chosen
            && self.intelligence.ready
            && !matches!(self.intelligence.kind, IntelligenceKind::None);
        if !usable {
            return refused("Choose a usable intelligence first: nothing was saved.");
        }
        let decision =
            (self.authoring_context.decision()).map_or(AUTHOR_ALONE, DecisionSetup::model);
        let decides = match self.authoring_context.decision() {
            Some(seat) => format!("{} makes the typed decisions", seat.model()),
            None => "the author decides alone".to_owned(),
        };
        let pref = UserIntelligencePreference::new(
            self.intelligence.kind.clone(),
            self.intelligence.model.clone(),
        )
        .deciding(decision);
        if let Err(error) = pref.save(&home) {
            return refused(&format!("The default was not saved: {error}."));
        }
        let model = (pref.model.as_deref())
            .map_or_else(|| "the provider's own model".to_owned(), str::to_owned);
        self.operator_default = Some(pref);
        TurnOutcome::Facts(format!(
            "Saved as the default for new conversations: {model}, and {decides}. This \
             conversation is unchanged."
        ))
    }
}

fn refused(why: &str) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(RefusalClass::IntelligenceRefused, why))
}

/// The decision seat a session opens with, from the host's context and the operator's saved
/// default: a default deciding alone opens it that way; a default naming a seat, when the host
/// named none, opens with that seat built by its name (its key read through the seat's own
/// boundary, never here) while it can be consulted, and keeps it to show and choose otherwise,
/// so a missing key never blocks the author. A seat the host named stands. Returns the context
/// and the Jev seat the session can choose.
pub(super) fn saved_decision(
    context: AuthoringContext,
    saved: Option<&UserIntelligencePreference>,
) -> (AuthoringContext, Option<DecisionSetup>) {
    let named = context.decision().cloned();
    match saved.and_then(|saved| saved.decision.as_deref()) {
        Some(AUTHOR_ALONE) => (context.with_decision_replaced(None), named),
        Some(word) if named.is_none() => {
            let seat = DecisionSetup::named(word);
            let context = if seat.refusal().is_none() {
                context.with_decision_replaced(Some(seat.clone()))
            } else {
                context
            };
            (context, Some(seat))
        }
        _ => (context, named),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::authoring::{AuthoringContext, AuthoringContextError};
    use crate::intelligence::IntelligenceCensus;
    use crate::reasoner::NoReasoner;

    /// A session whose host named no decision seat, whatever this machine's environment names.
    fn session() -> (tempfile::TempDir, SessionRuntime) {
        let root = tempfile::tempdir().expect("project");
        let mut session = SessionRuntime::open_unchosen(
            root.path(),
            IntelligenceCensus::empty(),
            None,
            Box::new(|_| Box::new(NoReasoner)),
        );
        session.jev_seat = None;
        session.set_authoring_context(AuthoringContext::default());
        (root, session)
    }

    fn seat(key: Option<&str>) -> DecisionSetup {
        DecisionSetup::with_key(
            "typesafe/jev-1.13.0",
            key.map(str::to_owned),
            Some("http://127.0.0.1:1"),
        )
    }

    /// The author alone clears the refusal a keyless Jev caused and is never refused; Jev comes
    /// back only as the seat the host named, and a keyless one is refused with its reason, the
    /// current choice kept.
    #[test]
    fn the_author_alone_is_never_blocked_and_jev_is_only_the_named_seat() {
        let (_root, mut session) = session();
        let none = session.choose_decision(DecisionChoice::Jev);
        assert!(
            matches!(&none, TurnOutcome::Refusal(r) if r.text.contains("no decision seat is configured")),
            "{none:?}"
        );
        session.set_authoring_context(AuthoringContext::default().with_decision(Some(seat(None))));
        assert!(matches!(
            session.authoring_context().refusal(),
            Some(AuthoringContextError::Decision { .. })
        ));
        let alone = session.choose_decision(DecisionChoice::AuthorAlone);
        assert!(matches!(alone, TurnOutcome::Facts(ref w) if w.contains("decides alone")));
        assert_eq!(session.authoring_context().decision(), None);
        assert_eq!(session.authoring_context().refusal(), None, "not blocked");
        let keyless = session.choose_decision(DecisionChoice::Jev);
        assert!(
            matches!(&keyless, TurnOutcome::Refusal(r) if r.text.contains("TYPESAFE_API_KEY")),
            "{keyless:?}"
        );
        assert_eq!(session.authoring_context().decision(), None, "kept");
        assert_eq!(session.authoring_context().refusal(), None);
        session.set_authoring_context(
            AuthoringContext::default().with_decision(Some(seat(Some("k")))),
        );
        let _ = session.choose_decision(DecisionChoice::AuthorAlone);
        assert_eq!(session.authoring_context().decision(), None);
        let jev = session.choose_decision(DecisionChoice::Jev);
        assert!(matches!(jev, TurnOutcome::Facts(ref w) if w.contains("typesafe/jev-1.13.0")));
        assert_eq!(
            session
                .authoring_context()
                .decision()
                .map(DecisionSetup::model),
            Some("typesafe/jev-1.13.0")
        );
        assert_eq!(session.authoring_context().refusal(), None);
        assert_eq!(
            session.jev_seat().map(DecisionSetup::refusal),
            Some(None),
            "the seat the host named"
        );
    }

    /// A saved default deciding alone opens that way; one naming a seat opens with it only while
    /// it can be consulted (a refused seat is kept to show, never applied, so the author is not
    /// blocked); a seat the host named stands.
    #[test]
    fn a_saved_decision_applies_only_a_seat_that_can_be_consulted() {
        let api = || {
            UserIntelligencePreference::new(
                IntelligenceKind::Api {
                    provider: "anthropic".into(),
                },
                Some("anthropic/claude-sonnet-5-5".into()),
            )
        };
        let (alone, kept) = saved_decision(
            AuthoringContext::default().with_decision(Some(seat(Some("k")))),
            Some(&api().deciding(AUTHOR_ALONE)),
        );
        assert_eq!(alone.decision(), None);
        assert_eq!(alone.refusal(), None);
        assert_eq!(
            kept.map(|s| s.model().to_owned()).as_deref(),
            Some("typesafe/jev-1.13.0")
        );
        let (malformed, shown) = saved_decision(
            AuthoringContext::default(),
            Some(&api().deciding("typesafe/a b")),
        );
        assert_eq!(malformed.decision(), None, "never applied");
        assert_eq!(malformed.refusal(), None, "the author is not blocked");
        assert!(shown.is_some_and(|seat| seat.refusal().is_some()));
        let (named, seat_named) = saved_decision(
            AuthoringContext::default(),
            Some(&api().deciding("typesafe/jev-1.13.0")),
        );
        let seat_named = seat_named.expect("the saved seat is kept");
        assert_eq!(
            (seat_named.model(), seat_named.source()),
            ("typesafe/jev-1.13.0", "default")
        );
        assert_eq!(
            named.decision().is_some(),
            seat_named.refusal().is_none(),
            "applied exactly when its key lets it be consulted"
        );
        assert_eq!(named.refusal(), None);
        let host = seat(Some("k"));
        let (stands, _) = saved_decision(
            AuthoringContext::default().with_decision(Some(host.clone())),
            Some(&api().deciding("typesafe/jev-9")),
        );
        assert_eq!(stands.decision(), Some(&host), "the host's seat stands");
        let (untouched, none) = saved_decision(AuthoringContext::default(), None);
        assert_eq!((untouched.decision(), none), (None, None));
    }

    /// A conversation keyed for `DeepSeek`, opened under `home` with `DeepSeek V4 Pro`.
    fn deepseek(home: Option<&std::path::Path>) -> (tempfile::TempDir, SessionRuntime) {
        let root = tempfile::tempdir().expect("project");
        let census = IntelligenceCensus::new(vec![], vec!["deepseek".into()], vec![]);
        let pref = UserIntelligencePreference::new(
            IntelligenceKind::Api {
                provider: "deepseek".into(),
            },
            Some("deepseek/deepseek-v4-pro".into()),
        );
        let session = SessionRuntime::open_with(
            root.path(),
            census,
            &pref,
            home,
            Box::new(|_| Box::new(NoReasoner)),
        );
        (root, session)
    }

    /// "Set as default" saves the kind and model in use with the decision seat by name (or the
    /// author alone) and no key; a new conversation saved with the author alone opens that way.
    /// Without a home or a usable intelligence nothing is written.
    #[test]
    fn set_as_default_saves_the_choice_and_its_decision_seat_by_name() {
        let home = tempfile::tempdir().expect("home");
        let (_root, mut session) = deepseek(Some(home.path()));
        session.set_authoring_context(
            AuthoringContext::default().with_decision(Some(seat(Some("k")))),
        );
        let saved = session.save_operator_default();
        assert!(
            matches!(saved, TurnOutcome::Facts(ref w) if w.contains("deepseek/deepseek-v4-pro"))
        );
        let back = UserIntelligencePreference::load(home.path()).expect("saved");
        assert_eq!(back.model.as_deref(), Some("deepseek/deepseek-v4-pro"));
        assert_eq!(back.decision.as_deref(), Some("typesafe/jev-1.13.0"));
        assert_eq!(session.operator_default(), Some(&back));
        let written = std::fs::read_to_string(UserIntelligencePreference::path_under(home.path()))
            .expect("the file");
        assert!(!written.contains("\"k\""), "never a key: {written}");
        let _ = session.choose_decision(DecisionChoice::AuthorAlone);
        assert!(
            matches!(session.save_operator_default(), TurnOutcome::Facts(ref w) if w.contains("decides alone"))
        );
        let back = UserIntelligencePreference::load(home.path()).expect("saved");
        assert_eq!(back.decision.as_deref(), Some(AUTHOR_ALONE));
        let (_root, reopened) = deepseek(Some(home.path()));
        assert_eq!(
            reopened.authoring_context().decision(),
            None,
            "opens deciding alone"
        );
        assert_eq!(reopened.operator_default(), Some(&back));
        let (_root, mut homeless) = deepseek(None);
        assert!(matches!(
            homeless.save_operator_default(),
            TurnOutcome::Refusal(_)
        ));
        let elsewhere = tempfile::tempdir().expect("another home");
        let (_root, mut unchosen) = {
            let root = tempfile::tempdir().expect("project");
            let session = SessionRuntime::open_unchosen(
                root.path(),
                IntelligenceCensus::empty(),
                Some(elsewhere.path()),
                Box::new(|_| Box::new(NoReasoner)),
            );
            (root, session)
        };
        assert!(matches!(
            unchosen.save_operator_default(),
            TurnOutcome::Refusal(_)
        ));
        assert!(
            UserIntelligencePreference::load(elsewhere.path()).is_none(),
            "nothing written"
        );
    }
}
