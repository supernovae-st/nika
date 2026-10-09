// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The offers of the typed choice on screen, and which one the human
//! selected: presentation only. A selection is never an answer and grants
//! nothing. Only `Enter` on a selection sends its key, exactly once, and the
//! shell binds it to the question painted at that moment.
//!
//! The offers take keys only while the composer holds them, nothing lists
//! commands, the draft is blank, nothing works and no answer is in flight.
//! `Up`, `Down`, `Home` and `End` select: nothing is preselected, and the first
//! `Down` selects the first offer. The offers never write the draft. A draft,
//! typed or pasted, is the answer instead. Typeahead never reaches them,
//! because the typeahead law drops `Enter` and the arrows typed while Nika
//! works, and `Home` and `End` only edit the draft. `PgUp` and `PgDn` keep
//! paging the transcript, where a long question is read.
//!
//! A selection belongs to one logical question: the epoch of the session that
//! asked it, its key and its exact offers (keys and labels, in order). That
//! question asked again under a new identity (after a settings change, behind
//! an intermediate choice) keeps an unsubmitted selection, and `Enter` then
//! binds the identity painted now. Another scope, a question that no longer
//! waits, or a selection already sent and taken starts with nothing selected.

use crossterm::event::{KeyCode, KeyEvent};

use super::Composer;
use crate::model::{Asked, Offer, Shape, Waiting};

/// What `Enter` sent while a typed question was painted, until its turn
/// returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sent {
    /// The selected offer's key.
    Offer,
    /// The draft: the human's own words.
    Draft,
}

/// What a key did to the offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Offered {
    /// Not the offers' key: it takes its ordinary path.
    Pass,
    /// The selection moved; nothing was sent.
    Moved,
    /// `Enter` on a selection: this offer's exact key answers, once.
    Answer(String),
}

/// The typed choice `waiting` paints: its key, its question and its offers
/// (at least one).
pub(crate) fn choice(waiting: &Waiting) -> Option<(&str, &Asked, &[Offer])> {
    let Waiting::QuestionDocument { key, asked } = waiting else {
        return None;
    };
    match &asked.shape {
        Shape::Choice(offers) if !offers.is_empty() => {
            Some((key.as_str(), asked, offers.as_slice()))
        }
        Shape::Choice(_) | Shape::Text | Shape::Literal => None,
    }
}

/// The logical question a selection belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    epoch: u64,
    key: String,
    offers: Vec<Offer>,
}

impl Scope {
    /// The scope of the typed choice `key` asked as `asked` with `offers`.
    fn of(key: &str, asked: &Asked, offers: &[Offer]) -> Self {
        Self {
            epoch: asked.epoch,
            key: key.to_owned(),
            offers: offers.to_vec(),
        }
    }

    /// Whether that typed choice is this logical question, whatever its
    /// identity now.
    fn holds(&self, key: &str, asked: &Asked, offers: &[Offer]) -> bool {
        self.epoch == asked.epoch && self.key == key && self.offers.as_slice() == offers
    }
}

/// The selection the composer keeps between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct Answer {
    scope: Option<Scope>,
    selected: Option<usize>,
    sent: Option<Sent>,
}

impl Answer {
    /// The selection of the typed choice `waiting` paints, when it is that
    /// question's.
    fn selection(&self, waiting: &Waiting) -> Option<usize> {
        let (key, asked, offers) = choice(waiting)?;
        let held = self
            .scope
            .as_ref()
            .is_some_and(|scope| scope.holds(key, asked, offers));
        self.selected.filter(|at| held && *at < offers.len())
    }

    /// A wait about to be painted. An intermediate decision (the intelligence
    /// choice, a question in prose such as a cost decision) keeps what was
    /// selected; the same logical choice keeps it under its new identity;
    /// anything else starts with nothing selected.
    fn follow(&mut self, waiting: &Waiting) {
        if matches!(waiting, Waiting::Choosing | Waiting::Question { .. }) {
            return;
        }
        let chosen = choice(waiting);
        let kept = chosen.is_some_and(|(key, asked, offers)| {
            (self.scope.as_ref()).is_some_and(|scope| scope.holds(key, asked, offers))
        });
        if !kept {
            self.scope = chosen.map(|(key, asked, offers)| Scope::of(key, asked, offers));
            self.selected = None;
        }
    }
}

impl Composer {
    /// A wait the shell applies, in order: the selection follows the logical
    /// question it belongs to ([`Answer`]'s rules).
    pub(crate) fn follow(&mut self, waiting: &Waiting) {
        self.answer.follow(waiting);
    }

    /// The offer selected for the typed choice `waiting` paints, if any.
    pub(crate) fn offer_selected(&self, waiting: &Waiting) -> Option<usize> {
        self.answer.selection(waiting)
    }

    /// Whether the offers of `waiting` take a selection now: a typed choice is
    /// painted, nothing lists commands, the draft is blank and no answer is in
    /// flight. A press may select then; keys also need the composer to hold
    /// them ([`Self::offers_armed`]).
    pub(crate) fn offers_open(&self, waiting: &Waiting) -> bool {
        choice(waiting).is_some()
            && self.listing().is_none()
            && self.is_blank()
            && self.answer.sent.is_none()
    }

    /// Whether the offers of `waiting` take keys now: they are open and the
    /// composer holds the keys.
    pub(crate) fn offers_armed(&self, waiting: &Waiting) -> bool {
        self.focused() && self.offers_open(waiting)
    }

    /// One key for the offers of `waiting`, before the composer reads it.
    /// Only `Enter` on a selection answers, with that offer's exact key, and
    /// the offers are inert from then until the answer's turn returns.
    pub(crate) fn offer_key(&mut self, waiting: &Waiting, key: KeyEvent) -> Offered {
        if !key.modifiers.is_empty() || !self.offers_armed(waiting) {
            return Offered::Pass;
        }
        let Some((_, _, offers)) = choice(waiting) else {
            return Offered::Pass;
        };
        self.answer.follow(waiting);
        let at = self.answer.selection(waiting);
        let last = offers.len().saturating_sub(1);
        let next = match key.code {
            KeyCode::Down => at.map_or(0, |at| at.saturating_add(1).min(last)),
            KeyCode::Up => at.map_or(last, |at| at.saturating_sub(1)),
            KeyCode::Home => 0,
            KeyCode::End => last,
            KeyCode::Enter => {
                // Nothing selected: `Enter` is the composer's, the draft the answer.
                let Some(offer) = at.and_then(|at| offers.get(at)) else {
                    return Offered::Pass;
                };
                self.answer.sent = Some(Sent::Offer);
                return Offered::Answer(offer.key.clone());
            }
            _ => return Offered::Pass,
        };
        self.answer.selected = Some(next);
        Offered::Moved
    }

    /// A press on the offer at `index` of `waiting`: it is selected and
    /// nothing is sent. `false` when the offers take no selection now.
    pub(crate) fn select_offer(&mut self, waiting: &Waiting, index: usize) -> bool {
        let listed = choice(waiting).is_some_and(|(_, _, offers)| index < offers.len());
        if !listed || !self.offers_open(waiting) {
            return false;
        }
        self.answer.follow(waiting);
        self.answer.selected = Some(index);
        true
    }

    /// The human's own words left with `Enter` as the typed question's
    /// answer: in flight until its turn returns.
    pub(crate) fn answer_drafted(&mut self) {
        self.answer.sent = Some(Sent::Draft);
    }

    /// The answer's turn returned, `not_taken` when it says the answer was not
    /// taken. An offer sent and taken is spent and never selected again; an
    /// offer not taken stays selected; a draft's answer leaves the selection
    /// as it was. Returns what was in flight.
    pub(crate) fn answer_returned(&mut self, not_taken: bool) -> Option<Sent> {
        let sent = self.answer.sent.take();
        if sent == Some(Sent::Offer) && !not_taken {
            self.answer = Answer::default();
        }
        sent
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Keys whose bytes an answer must keep: spaces, a symbol, a slash.
    fn offers() -> Vec<Offer> {
        vec![
            Offer::new(" eur ", "Euro"),
            Offer::new("€", "the euro sign"),
            Offer::new("usd/cad", "two currencies"),
        ]
    }

    /// The typed choice `key` with `offers`, asked as `witness` in `epoch`.
    fn asked(key: &str, offers: Vec<Offer>, witness: &str, epoch: u64) -> Waiting {
        let shape = Shape::Choice(offers);
        let question = Asked::new("Which currency?", "", true, shape, witness, epoch);
        Waiting::asked(key, question)
    }

    /// The typed question `key` of `shape`, asked as `w1` in epoch 7.
    fn document(key: &str, shape: Shape) -> Waiting {
        Waiting::asked(key, Asked::new("Which?", "", true, shape, "w1", 7))
    }

    fn choosing(witness: &str) -> Waiting {
        asked("const.currency", offers(), witness, 7)
    }

    /// A composer holding the keys, the choice painted as `waiting`.
    fn composer(waiting: &Waiting) -> Composer {
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(waiting);
        composer
    }

    #[test]
    fn nothing_is_preselected_and_the_arrows_select_within_the_offers() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        assert_eq!(composer.offer_selected(&painted), None, "no preselection");
        assert!(composer.offers_armed(&painted));
        let mut press = |code| composer.offer_key(&painted, key(code));
        assert_eq!(press(KeyCode::Enter), Offered::Pass, "nothing selected");
        assert_eq!(press(KeyCode::Down), Offered::Moved);
        assert_eq!(composer.offer_selected(&painted), Some(0), "the first Down");
        for (code, at) in [
            (KeyCode::Down, 1),
            (KeyCode::Down, 2),
            (KeyCode::Down, 2),
            (KeyCode::Home, 0),
            (KeyCode::Up, 0),
            (KeyCode::End, 2),
            (KeyCode::Up, 1),
        ] {
            assert_eq!(composer.offer_key(&painted, key(code)), Offered::Moved);
            assert_eq!(composer.offer_selected(&painted), Some(at), "{code:?}");
        }
        let mut fresh = self::composer(&painted);
        assert_eq!(fresh.offer_key(&painted, key(KeyCode::Up)), Offered::Moved);
        assert_eq!(
            fresh.offer_selected(&painted),
            Some(2),
            "Up starts at the end"
        );
        // Not the offers' keys: they take their ordinary path.
        for code in [
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::Char('1'),
        ] {
            assert_eq!(
                fresh.offer_key(&painted, key(code)),
                Offered::Pass,
                "{code:?}"
            );
        }
        let shifted = KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT);
        assert_eq!(fresh.offer_key(&painted, shifted), Offered::Pass);
        assert!(fresh.text().is_empty(), "the offers never write the draft");
    }

    /// `Enter` answers with the selected offer's exact key, once: the offers
    /// are inert until that answer's turn returns, and a key taken is spent.
    #[test]
    fn enter_answers_with_the_exact_key_once_and_a_taken_key_is_spent() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        composer.offer_key(&painted, key(KeyCode::Down));
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Enter)),
            Offered::Answer(" eur ".to_owned())
        );
        for code in [KeyCode::Enter, KeyCode::Down, KeyCode::Up, KeyCode::End] {
            assert_eq!(
                composer.offer_key(&painted, key(code)),
                Offered::Pass,
                "{code:?} while the answer is in flight"
            );
        }
        assert!(!composer.select_offer(&painted, 1), "a press is inert too");
        assert_eq!(composer.answer_returned(false), Some(Sent::Offer));
        assert_eq!(composer.offer_selected(&painted), None, "spent");
        // The same question shown again never brings the spent selection back.
        composer.follow(&choosing("w2"));
        assert_eq!(composer.offer_selected(&choosing("w2")), None);
        assert!(
            composer.offers_armed(&choosing("w2")),
            "armed for the new wait"
        );
    }

    /// An offer the conversation did not take stays selected (its key never
    /// fills the draft); an answer in words leaves the selection as it was.
    #[test]
    fn an_offer_not_taken_stays_selected_and_a_draft_answer_keeps_the_selection() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        composer.offer_key(&painted, key(KeyCode::End));
        let sent = composer.offer_key(&painted, key(KeyCode::Enter));
        assert_eq!(sent, Offered::Answer("usd/cad".to_owned()));
        assert_eq!(composer.answer_returned(true), Some(Sent::Offer));
        assert_eq!(composer.offer_selected(&painted), Some(2));
        assert!(composer.text().is_empty());
        assert!(composer.offers_armed(&painted), "armed again");
        composer.answer_drafted();
        assert!(
            !composer.offers_open(&painted),
            "inert while words are in flight"
        );
        assert_eq!(composer.answer_returned(false), Some(Sent::Draft));
        assert_eq!(composer.offer_selected(&painted), Some(2));
        assert_eq!(composer.answer_returned(false), None, "nothing in flight");
    }

    /// The same logical question under a new identity (behind the intelligence
    /// choice or a prose decision) keeps an unsubmitted selection; changed
    /// offers, another key, another session, another shape or a question that
    /// no longer waits starts with nothing selected.
    #[test]
    fn a_selection_survives_a_new_identity_of_its_question_and_nothing_else() {
        let painted = choosing("w1");
        let selected = |waiting: &Waiting| {
            let mut composer = composer(&painted);
            composer.offer_key(&painted, key(KeyCode::Down));
            composer.offer_key(&painted, key(KeyCode::Down));
            composer.follow(waiting);
            let back = choosing("w9");
            composer.follow(&back);
            composer.offer_selected(&back)
        };
        let prose = Waiting::Question {
            key: "unknown_cost".to_owned(),
        };
        for between in [Waiting::Choosing, prose, choosing("w2")] {
            assert_eq!(selected(&between), Some(1), "{between:?}");
        }
        let mut relabelled = offers();
        relabelled[1].label = "euro".to_owned();
        let mut reordered = offers();
        reordered.swap(0, 1);
        let text = document("const.currency", Shape::Text);
        for between in [
            asked("const.currency", relabelled, "w2", 7),
            asked("const.currency", reordered, "w2", 7),
            asked("const.other", offers(), "w2", 7),
            asked("const.currency", offers(), "w2", 8),
            text,
            Waiting::Free,
            Waiting::Proposal,
            Waiting::Gate,
        ] {
            assert_eq!(selected(&between), None, "{between:?}");
        }
    }

    /// The offers are inert while the draft holds words (typed or pasted),
    /// while the chooser lists, and for keys while the composer does not
    /// hold them; a press still selects there, never into the draft.
    #[test]
    fn the_offers_take_keys_only_from_an_empty_draft_that_holds_the_keys() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        composer.paste("eur");
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Down)),
            Offered::Pass
        );
        assert!(!composer.select_offer(&painted, 0));
        composer.clear();
        composer.toggle_palette();
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Down)),
            Offered::Pass
        );
        composer.close_palette();
        composer.paste("/s");
        assert!(!composer.offers_open(&painted), "a slash draft");
        composer.clear();
        composer.set_focused(false);
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Down)),
            Offered::Pass
        );
        assert!(composer.select_offer(&painted, 2), "a press selects");
        assert!(!composer.select_offer(&painted, 3), "past the offers");
        assert_eq!(composer.offer_selected(&painted), Some(2));
        assert!(composer.text().is_empty());
        for waiting in [Waiting::Free, Waiting::Gate, Waiting::Proposal] {
            assert!(!composer.offers_open(&waiting), "{waiting:?}");
            assert_eq!(composer.offer_selected(&waiting), None);
        }
    }

    /// A question in prose (a legacy key, a cost decision) is never a typed
    /// choice, whatever its key: it lists no offers, takes no selection and
    /// answers no key. A typed text, literal or empty choice offers nothing
    /// either, so `Enter` stays the composer's there.
    #[test]
    fn a_question_in_prose_never_takes_an_offer() {
        let shown = [
            Waiting::Question {
                key: "const.currency".to_owned(),
            },
            Waiting::Question {
                key: "run_cost".to_owned(),
            },
            document("const.currency", Shape::Text),
            document("const.currency", Shape::Literal),
            document("const.currency", Shape::Choice(Vec::new())),
        ];
        for waiting in shown {
            let mut composer = composer(&waiting);
            assert!(choice(&waiting).is_none(), "{waiting:?}");
            assert!(!composer.offers_open(&waiting), "{waiting:?}");
            for code in [KeyCode::Down, KeyCode::Up, KeyCode::Enter] {
                let pressed = composer.offer_key(&waiting, key(code));
                assert_eq!(pressed, Offered::Pass, "{waiting:?} {code:?}");
            }
            assert!(!composer.select_offer(&waiting, 0), "{waiting:?}");
            assert_eq!(composer.offer_selected(&waiting), None, "{waiting:?}");
            assert!(composer.text().is_empty(), "{waiting:?}");
        }
    }
}
