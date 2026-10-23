// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One turn path for what the human sends. A line goes to whatever waits
//! through [`Conversation::submit_observed`]. While a typed question is
//! painted, every line (an offered key the human selected, or their own
//! words) is that question's answer and goes through
//! [`Conversation::answer_bound`] with the identity painted, never through
//! the general door. Everything else is shared: the echo (what was sent,
//! never that it applied), the busy row, the Stop hold, the typeahead law
//! and a queued correction's fate.
//!
//! An answer that was not taken ([`Beat::NotTaken`]) comes back exactly: its
//! words to the draft, before whatever was typed while it worked (the one
//! restoration law, `Composer::put_back`), or its offer still selected.

use std::sync::mpsc::Sender;

use crossterm::event::KeyEvent;

use super::Shell;
use crate::composer::Composer;
use crate::composer::answer::{DELEGATION, Offered, Sent, words_are_lines};
use crate::lang::Voice;
use crate::model::{Beat, Conversation, Shape, Turn, Waiting};
use crate::session::feed::Seen;

/// What the human sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Said {
    /// A line to whatever waits.
    Line(String),
    /// The answer to the typed question painted as `witness`.
    Answer {
        /// What was sent, exactly: an offered key, or the human's own words.
        text: String,
        /// The identity of the question painted when it was sent.
        witness: String,
    },
    /// The choice of the typed question painted as `witness` left to Nika, by
    /// the native control: an act, never words a label could name.
    Delegate {
        /// The identity of the question painted when it was sent.
        witness: String,
    },
    /// The offered keys the native control selected for the typed question
    /// painted as `witness` (a row, or the checked set as a JSON list): an
    /// act, never words typed that happen to read as a key.
    Pick {
        /// The exact keys sent.
        keys: String,
        /// The identity of the question painted when it was sent.
        witness: String,
    },
}

impl Said {
    /// The words sent, exactly (a delegation's, the control's own).
    pub(super) fn text(&self) -> &str {
        match self {
            Self::Line(text) | Self::Answer { text, .. } | Self::Pick { keys: text, .. } => text,
            Self::Delegate { .. } => DELEGATION,
        }
    }

    /// Echo valid picks by their displayed labels, without changing the answer sent.
    /// Only the exact painted question can explain its keys; every other line stays raw.
    /// At the optional offer about the last result, words typed are the person's next line,
    /// never its answer: they echo as typed, and only the rows picked read as their labels.
    pub(super) fn display_text(&self, waiting: &Waiting) -> String {
        if let (Self::Answer { text, .. }, Waiting::Action { key, .. }) = (self, waiting)
            && words_are_lines(key)
        {
            return text.clone();
        }
        let (
            Self::Answer { text, witness }
            | Self::Pick {
                keys: text,
                witness,
            },
            Waiting::QuestionDocument { asked, .. }
            | Waiting::ProposalChoice { asked, .. }
            | Waiting::Action { asked, .. },
        ) = (self, waiting)
        else {
            return self.text().to_owned();
        };
        if witness != &asked.witness {
            return text.clone();
        }
        let (offers, keys) = match &asked.shape {
            Shape::Choice(offers) => (offers, vec![text.clone()]),
            Shape::Multiple(offers) => {
                let keys = if text.trim_start().starts_with('[') {
                    match serde_json::from_str::<Vec<String>>(text) {
                        Ok(keys) => keys,
                        Err(_) => return text.clone(),
                    }
                } else {
                    vec![text.clone()]
                };
                (offers, keys)
            }
            Shape::Text | Shape::Literal => return text.clone(),
        };
        if keys.is_empty()
            || keys
                .iter()
                .enumerate()
                .any(|(at, key)| keys[..at].contains(key))
        {
            return text.clone();
        }
        let labels = keys
            .iter()
            .map(|key| {
                let mut matching = offers.iter().filter(|offer| offer.key == *key);
                let offer = matching.next()?;
                (matching.next().is_none() && !offer.label.is_empty())
                    .then_some(offer.label.as_str())
            })
            .collect::<Option<Vec<_>>>();
        labels.map_or_else(|| text.clone(), |labels| labels.join(", "))
    }

    /// The person's line for what was sent while `waiting` is painted, or none: their own
    /// words, never the prompt they were typed after (« action › », « choice › »: live
    /// gauntlet r07 B9, r02 B2). A row picked in the intelligence list reads by its name,
    /// never its key: a choice is `Intelligence: <name>`, the plainest of the labels its rows
    /// give the same key (the recommended row and the model found under it name one model),
    /// and a step to another list or an act Nika confirms itself (a `/…` row) adds no line,
    /// nor does `Esc` going back to the list above (its `back` line). Anything else is what
    /// was sent, a question's keys by their labels ([`Self::display_text`]).
    /// The choice left to Nika reads in the person's language (`voice`): « Je laisse le choix
    /// à Nika. » under « Laisser Nika choisir » (live gauntlet 6, r07 B3).
    pub(super) fn echo(&self, waiting: &Waiting, voice: Voice) -> Option<String> {
        if let Self::Delegate { .. } = self {
            return Some(voice.say(DELEGATION).to_owned());
        }
        if let (Self::Line(line), Waiting::Selection { asked }) = (self, waiting)
            && asked.back.as_deref() == Some(line.as_str())
        {
            return None;
        }
        if let (Self::Line(line), Waiting::Selection { asked }) = (self, waiting)
            && let Shape::Choice(offers) = &asked.shape
            && offers.iter().any(|offer| offer.key == *line)
        {
            if line.starts_with('/') {
                return None;
            }
            let name = (offers.iter())
                .filter(|offer| offer.key == *line && !offer.label.trim().is_empty())
                .map(|offer| offer.label.trim())
                .min_by_key(|label| label.matches(" · ").count())
                .unwrap_or(line.as_str());
            return Some(format!("Intelligence: {name}"));
        }
        Some(self.display_text(waiting).trim_end().to_owned())
    }

    /// Send through the conversation's one door for this kind of line: a line
    /// to whatever waits, an answer by the identity painted.
    pub(super) fn send<C: Conversation + ?Sized>(
        &self,
        conversation: &mut C,
        busy: &Sender<String>,
        seen: &Seen,
    ) -> Turn {
        match self {
            Self::Line(line) => conversation.submit_observed(line, busy, seen),
            Self::Answer { text, witness } => conversation.answer_bound(text, witness, busy, seen),
            Self::Delegate { witness } => conversation.delegate_bound(witness, busy, seen),
            Self::Pick { keys, witness } => conversation.pick_bound(keys, witness, busy, seen),
        }
    }
}

/// The choice of the typed question `waiting` paints, left to Nika by its
/// control; any other wait (none offers that row) takes the control's words
/// as a line, as [`routed`] does, in the person's language (`voice`): the
/// person's own line, it never tells the Session another language.
pub(super) fn delegated(waiting: &Waiting, voice: Voice) -> Said {
    match waiting {
        Waiting::QuestionDocument { asked, .. }
        | Waiting::ProposalChoice { asked, .. }
        | Waiting::Action { asked, .. } => Said::Delegate {
            witness: asked.witness.clone(),
        },
        _ => routed(waiting, voice.say(DELEGATION).to_owned()),
    }
}

/// The keys the native control selected for the typed question `waiting`
/// paints, as that act ([`Said::Pick`]); any other wait takes them as a line,
/// as [`routed`] does.
pub(super) fn picked(waiting: &Waiting, keys: String) -> Said {
    match routed(waiting, keys) {
        Said::Answer { text, witness } => Said::Pick {
            keys: text,
            witness,
        },
        said => said,
    }
}

/// What `line`, sent while `waiting` is painted, is: the typed question's
/// answer bound to the identity painted, or a line to whatever waits (a
/// question kept in prose, a cost decision, a consent, a gate, the choice of
/// intelligence, the free prompt).
pub(super) fn routed(waiting: &Waiting, line: String) -> Said {
    match waiting {
        Waiting::QuestionDocument { asked, .. }
        | Waiting::ProposalChoice { asked, .. }
        | Waiting::Action { asked, .. } => Said::Answer {
            text: line,
            witness: asked.witness.clone(),
        },
        _ => Said::Line(line),
    }
}

/// The words of the answer `beats` say was not taken.
fn not_taken(beats: &[Beat]) -> Option<String> {
    beats.iter().find_map(|beat| match beat {
        Beat::NotTaken(words) => Some(words.clone()),
        _ => None,
    })
}

/// Where an answer the conversation did not take goes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Unsent {
    /// Nothing came back.
    Nothing,
    /// These exact words go back to the draft.
    Words(String),
    /// These exact words go back to the typed choice's own reply field.
    Own(String),
    /// The offer sent stays selected; its key never fills the draft.
    Selected,
}

/// The turn of an answer returned with `beats`, before they are applied:
/// what `composer` had in flight is settled, so the wait they paint arms the
/// offers afresh, and what was not taken goes back where it came from.
fn returned(composer: &mut Composer, beats: &[Beat]) -> Unsent {
    let refused = not_taken(beats);
    let sent = composer.answer_returned(refused.is_some());
    match refused {
        Some(_) if sent == Some(Sent::Offer) => Unsent::Selected,
        Some(words) if sent == Some(Sent::Own) => Unsent::Own(words),
        Some(words) => Unsent::Words(words),
        None => Unsent::Nothing,
    }
}

/// What a key did to the painted offers, on the shell's key path.
pub(super) enum Picked {
    /// Not theirs: the composer reads it.
    Pass,
    /// The selection moved; nothing was sent.
    Moved,
    /// `Enter` on a selection: its key, bound to the question painted.
    Sent(Said),
    /// `Esc` with nothing left to undo on a decision: its card is set aside, unanswered.
    SetAside,
}

/// The hint once words that were not taken are back in the box.
const BACK_IN_THE_BOX: &str = "not taken · answer text restored for editing";
/// The hint once an offer that was not taken is still selected.
const STILL_SELECTED: &str = "not taken · your choice is still selected";
/// The hint once an answer was not taken and nothing of it is in view.
const NOTHING_SENT: &str = "not taken · original text stays in the conversation";

impl<C: Conversation + 'static> Shell<C> {
    /// A line the composer sent, routed by what is painted now ([`routed`]):
    /// an answer in words is in flight until its turn returns.
    pub(super) fn said(&mut self, line: String) -> Said {
        let said = routed(&self.state.waiting, line);
        if matches!(said, Said::Answer { .. }) {
            self.composer.answer_drafted();
        }
        said
    }

    /// A key for the painted offers, before the composer's own: at rest only,
    /// with the keys on the composer. `Enter` on a selection sends that
    /// offer's exact key, bound to the question painted now.
    pub(super) fn pick(&mut self, key: KeyEvent) -> Picked {
        if self.conversation.is_none() || self.state.busy.is_some() || !self.composer_has_keys() {
            return Picked::Pass;
        }
        match self.composer.offer_key(&self.state.waiting, key) {
            Offered::Pass => Picked::Pass,
            Offered::Moved => Picked::Moved,
            Offered::Notice(words) => {
                self.state.completion = Some(crate::render::own(words, self.state.ascii));
                Picked::Moved
            }
            Offered::Answer(sent) => Picked::Sent(self.offered(sent)),
            Offered::Delegate => Picked::Sent(delegated(&self.state.waiting, self.state.voice())),
            Offered::SetAside => Picked::SetAside,
        }
    }

    /// What `Enter` sent from the painted offers, as the act it was: the
    /// selected keys ([`picked`]) or the human's own reply typed beside them
    /// ([`routed`]), never told apart by their bytes.
    pub(super) fn offered(&self, sent: String) -> Said {
        if self.composer.in_flight() == Some(Sent::Offer) {
            picked(&self.state.waiting, sent)
        } else {
            routed(&self.state.waiting, sent)
        }
    }

    /// The turn of what was sent returned with `beats`, before they are
    /// applied ([`returned`]): what was in flight is settled.
    pub(super) fn answered(&mut self, beats: &[Beat]) -> Unsent {
        returned(&mut self.composer, beats)
    }

    /// Once the turn's beats are applied and the typeahead law has run,
    /// what was not taken goes back: its words to the box, exactly, before
    /// whatever was typed while it worked (unless a spending question took
    /// the box: the echo keeps them), or its offer, still selected while its
    /// question is painted again. The hint row says which.
    pub(super) fn give_back(&mut self, unsent: Unsent, fresh: bool) {
        let notice = match unsent {
            Unsent::Nothing => return,
            Unsent::Words(words) if !fresh => {
                self.keep_reading(|_, composer| composer.put_back(&words));
                BACK_IN_THE_BOX
            }
            // Its question still holds the line: back in its own field.
            Unsent::Own(words) if self.composer.choosing(&self.state.waiting) => {
                self.composer.put_back_own(&words);
                BACK_IN_THE_BOX
            }
            // The question moved on: the words wait in the draft, never lost.
            Unsent::Own(words) if !fresh => {
                self.keep_reading(|_, composer| composer.put_back(&words));
                BACK_IN_THE_BOX
            }
            Unsent::Selected if self.composer.offer_selected(&self.state.waiting).is_some() => {
                STILL_SELECTED
            }
            Unsent::Words(_) | Unsent::Own(_) | Unsent::Selected => NOTHING_SENT,
        };
        self.state.completion = Some(notice.to_owned());
    }
}

#[cfg(test)]
mod bridge_tests;

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;
    use crate::model::{Asked, Committed, Handoff, Kind, Offer, Shape};

    /// A conversation that records every door a line leaves by.
    #[derive(Default)]
    struct Doors {
        used: Vec<String>,
    }

    impl Conversation for Doors {
        fn open(&mut self) -> Vec<Beat> {
            Vec::new()
        }

        fn submit(&mut self, line: &str) -> Turn {
            self.used.push(format!("submit {line}"));
            Turn {
                beats: vec![Beat::Wait(Waiting::Free)],
                handoff: None,
            }
        }

        fn submit_with(&mut self, line: &str, busy: &Sender<String>) -> Turn {
            self.used.push(format!("submit_with {line}"));
            let _ = busy;
            self.submit(line)
        }

        fn submit_observed(&mut self, line: &str, busy: &Sender<String>, seen: &Seen) -> Turn {
            self.used.push(format!("submit_observed {line}"));
            let _ = seen;
            self.submit_with(line, busy)
        }

        fn delegate_bound(&mut self, witness: &str, _: &Sender<String>, _: &Seen) -> Turn {
            self.used.push(format!("delegate_bound {witness}"));
            Turn {
                beats: vec![Beat::Wait(Waiting::Free)],
                handoff: None,
            }
        }

        fn answer_bound(
            &mut self,
            text: &str,
            witness: &str,
            _: &Sender<String>,
            _: &Seen,
        ) -> Turn {
            self.used.push(format!("answer_bound {witness} [{text}]"));
            Turn {
                beats: vec![
                    Beat::Say(Committed::new(Kind::Refusal, "not the question shown")),
                    Beat::NotTaken(text.to_owned()),
                ],
                handoff: None,
            }
        }

        fn perform(&mut self, _: &Handoff) -> Vec<Beat> {
            Vec::new()
        }
    }

    /// The sinks a turn lends, nobody reading them.
    fn sinks() -> (Sender<String>, Seen) {
        let (busy, _) = std::sync::mpsc::channel();
        let (queue, _) = std::sync::mpsc::sync_channel(1);
        let gap = std::sync::Arc::new(crate::session::feed::Gap::default());
        (busy, Seen::new(queue, gap))
    }

    fn typed(shape: Shape, witness: &str) -> Waiting {
        Waiting::asked(
            "const.currency",
            Asked::new("Which currency?", "", true, shape, witness, 3),
        )
    }

    fn currencies() -> Shape {
        Shape::Choice(vec![
            Offer::new("eur", "Euro"),
            Offer::new(" usd ", "US dollar, the key keeps its spaces"),
        ])
    }

    /// While a typed question is painted, the human's own words (a command
    /// too) are its answer, bound to the painted identity, exact; anything
    /// else painted keeps the line a line.
    #[test]
    fn a_line_at_a_painted_typed_question_is_its_answer_and_nothing_else_is() {
        let words = "  the CSV export, not the JSON one ";
        for shape in [currencies(), Shape::Text, Shape::Literal] {
            for line in [words, "/intelligence", "cancel", ""] {
                assert_eq!(
                    routed(&typed(shape.clone(), "w-painted"), line.to_owned()),
                    Said::Answer {
                        text: line.to_owned(),
                        witness: "w-painted".to_owned()
                    }
                );
            }
        }
        let held = Asked::new("not admitted", "", false, currencies(), "7:knowledge", 7);
        for waiting in [
            Waiting::Free,
            Waiting::Choosing,
            Waiting::knowledge(held),
            Waiting::Proposal,
            Waiting::Gate,
            Waiting::Question {
                key: "unknown_cost".to_owned(),
            },
            Waiting::Question {
                key: "const.source_path".to_owned(),
            },
        ] {
            assert_eq!(
                routed(&waiting, words.to_owned()),
                Said::Line(words.to_owned()),
                "{waiting:?}"
            );
        }
    }

    /// The one worker dispatch: an answer goes through `answer_bound` exactly
    /// once with its exact bytes and the painted witness, and never through a
    /// general door; a line goes through `submit_observed`.
    #[test]
    fn an_answer_leaves_by_the_bound_door_once_and_a_line_by_the_general_one() {
        let (busy, seen) = sinks();
        let mut doors = Doors::default();
        let answer = routed(&typed(currencies(), "w-7"), " usd ".to_owned());
        let turn = answer.send(&mut doors, &busy, &seen);
        assert_eq!(doors.used, ["answer_bound w-7 [ usd ]"]);
        assert_eq!(not_taken(&turn.beats).as_deref(), Some(" usd "));
        let mut doors = Doors::default();
        Said::Line("hello".to_owned()).send(&mut doors, &busy, &seen);
        assert_eq!(
            doors.used,
            ["submit_observed hello", "submit_with hello", "submit hello"]
        );
        assert_eq!(answer.text(), " usd ");
    }

    /// The choice left to Nika reads in the person's language (live gauntlet 6, r07 B3:
    /// « Laisser Nika choisir » echoed « I leave the choice to Nika. » in a French bubble): the
    /// act's echo is French under a French voice, English under the default one, and the line
    /// the control sends where no typed question is painted is the person's language too; the
    /// act itself still leaves by the delegation door, its recorded words unchanged.
    #[test]
    fn a_delegation_echoes_in_the_persons_language() {
        let french = Voice {
            lang: crate::lang::Lang::Fr,
            ascii: false,
        };
        let waiting = typed(currencies(), "w-9");
        let act = delegated(&waiting, french);
        assert_eq!(
            act.echo(&waiting, french).as_deref(),
            Some("Je laisse le choix à Nika.")
        );
        assert_eq!(
            act.echo(&waiting, Voice::default()).as_deref(),
            Some(DELEGATION)
        );
        assert_eq!(
            act.text(),
            DELEGATION,
            "the recorded words are the control's own"
        );
        assert_eq!(
            delegated(&Waiting::Free, french),
            Said::Line("Je laisse le choix à Nika.".to_owned()),
            "no question painted: the person's words, as a line"
        );
        assert_eq!(
            nika_session::Lang::of("Je laisse le choix à Nika."),
            Some(nika_session::Lang::Fr),
            "a line that never tells the Session another language"
        );
    }

    /// The « Let Nika choose » act leaves by the delegation door once, bound to
    /// the painted witness, never as an answer in words; it echoes the
    /// control's words. A conversation without that door gets the words through
    /// `answer_bound`, as before.
    #[test]
    fn a_delegation_leaves_by_its_own_door_bound_to_the_painted_question() {
        let (busy, seen) = sinks();
        let waiting = typed(currencies(), "w-9");
        let act = delegated(&waiting, Voice::default());
        assert_eq!(
            act,
            Said::Delegate {
                witness: "w-9".to_owned()
            }
        );
        let mut doors = Doors::default();
        act.send(&mut doors, &busy, &seen);
        assert_eq!(doors.used, ["delegate_bound w-9"]);
        assert_eq!(act.text(), DELEGATION);
        assert_eq!(act.display_text(&waiting), DELEGATION);
        let mut script = crate::model::Script::new(Vec::new(), Vec::new());
        let turn = act.send(&mut script, &busy, &seen);
        assert_eq!(not_taken(&turn.beats).as_deref(), Some(DELEGATION));
        assert_eq!(
            delegated(&Waiting::Free, Voice::default()),
            Said::Line(DELEGATION.to_owned()),
            "no question painted: the words, as a line"
        );
    }

    /// `Enter` on a selected offer sends its exact key, bound to the identity
    /// painted NOW: after the question was asked again under a new identity
    /// (same epoch, key and offers), the selection is the human's and the
    /// witness is the new one; the old identity answers nothing.
    #[test]
    fn a_kept_selection_answers_with_its_exact_key_bound_to_the_new_identity() {
        let first = typed(currencies(), "w-old");
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(&first);
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        composer.offer_key(&first, press(KeyCode::Down));
        composer.offer_key(&first, press(KeyCode::Down));
        composer.follow(&Waiting::Choosing);
        let again = typed(currencies(), "w-new");
        composer.follow(&again);
        let sent = composer.offer_key(&again, press(KeyCode::Enter));
        assert_eq!(
            sent,
            Offered::Answer(" usd ".to_owned()),
            "the kept selection"
        );
        let Offered::Answer(key) = sent else {
            return;
        };
        let said = routed(&again, key);
        assert_eq!(
            said,
            Said::Answer {
                text: " usd ".to_owned(),
                witness: "w-new".to_owned()
            }
        );
        let (busy, seen) = sinks();
        let mut doors = Doors::default();
        said.send(&mut doors, &busy, &seen);
        assert_eq!(doors.used, ["answer_bound w-new [ usd ]"]);
        assert!(!doors.used.iter().any(|door| door.contains("w-old")));
    }

    /// A stale answer in words, refused with the question asked again under a
    /// new identity: its exact words come back first and what the human typed
    /// while it worked follows, neither replaced, and the offers arm again for
    /// the question painted after. An offer that was not taken stays selected
    /// and never fills the draft; an answer taken returns nothing.
    #[test]
    fn a_refused_answer_comes_back_exactly_before_what_was_typed_meanwhile() {
        use crate::composer::ComposerAction;
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let painted = typed(currencies(), "w-1");
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(&painted);
        let words = "  the CSV export\nnot the JSON ";
        composer.paste(words);
        let action = composer.handle(key(KeyCode::Enter));
        assert_eq!(action, ComposerAction::Submit(words.to_owned()));
        let said = routed(&painted, words.to_owned());
        composer.answer_drafted();
        assert!(!composer.offers_open(&painted), "in flight");
        for typed_meanwhile in "and keep the header".chars() {
            composer.handle(key(KeyCode::Char(typed_meanwhile)));
        }
        let (busy, seen) = sinks();
        let mut doors = Doors::default();
        let mut beats = said.send(&mut doors, &busy, &seen).beats;
        assert_eq!(doors.used, [format!("answer_bound w-1 [{words}]")]);
        let again = typed(currencies(), "w-2");
        beats.push(Beat::Wait(again.clone()));
        assert_eq!(
            returned(&mut composer, &beats),
            Unsent::Words(words.to_owned())
        );
        for beat in &beats {
            if let Beat::Wait(waiting) = beat {
                composer.follow(waiting);
            }
        }
        composer.put_back(words);
        assert_eq!(composer.text(), format!("{words}\nand keep the header"));
        composer.clear();
        assert!(
            composer.offers_armed(&again),
            "armed for the wait painted after"
        );
        // The question asked again opens on its first row: `Enter` applies it.
        assert_eq!(composer.offer_selected(&again), Some(0));
        let sent = composer.offer_key(&again, key(KeyCode::Enter));
        assert_eq!(sent, Offered::Answer("eur".to_owned()));
        let refused = [
            Beat::Say(Committed::new(Kind::Refusal, "stale")),
            Beat::NotTaken("eur".to_owned()),
            Beat::Wait(again.clone()),
        ];
        assert_eq!(returned(&mut composer, &refused), Unsent::Selected);
        assert_eq!(composer.offer_selected(&again), Some(0));
        assert!(composer.text().is_empty(), "a key never fills the draft");
        let taken = [Beat::Wait(Waiting::Free)];
        assert_eq!(returned(&mut composer, &taken), Unsent::Nothing);
    }

    /// An own reply typed in the typed choice's own field leaves before any
    /// selection; refused, its exact words come back to that field (never to
    /// the draft, which never changed) for the question painted again.
    #[test]
    fn an_own_reply_not_taken_comes_back_to_its_own_field() {
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let painted = typed(currencies(), "w-1");
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.paste("my unsent draft");
        composer.follow(&painted);
        for c in "dollars".chars() {
            assert!(composer.own_key(key(KeyCode::Char(c))));
        }
        composer.offer_key(&painted, key(KeyCode::Down));
        let sent = composer.offer_key(&painted, key(KeyCode::Enter));
        assert_eq!(sent, Offered::Answer("dollars".to_owned()));
        let refused = [
            Beat::Say(Committed::new(Kind::Refusal, "stale")),
            Beat::NotTaken("dollars".to_owned()),
            Beat::Wait(painted.clone()),
        ];
        assert_eq!(
            returned(&mut composer, &refused),
            Unsent::Own("dollars".to_owned())
        );
        composer.put_back_own("dollars");
        assert_eq!(composer.own_reply(), "dollars");
        assert_eq!(
            composer.text(),
            "my unsent draft",
            "the draft never changed"
        );
    }

    /// The words of the first answer the beats say was not taken, whatever
    /// beats surround it; none when the answer was taken.
    #[test]
    fn the_words_not_taken_are_read_from_the_beats_exactly() {
        let words = "line one\n  line two ";
        let beats = [
            Beat::Say(Committed::new(Kind::Refusal, "stale")),
            Beat::Wait(Waiting::Free),
            Beat::NotTaken(words.to_owned()),
        ];
        assert_eq!(not_taken(&beats).as_deref(), Some(words));
        assert_eq!(not_taken(&[Beat::Wait(Waiting::Free)]), None);
        // Each notice fits a narrow hint row and claims nothing was taken.
        for hint in [BACK_IN_THE_BOX, STILL_SELECTED, NOTHING_SENT] {
            assert!(hint.chars().count() <= 60, "{hint}");
            assert!(hint.starts_with("not taken · "), "{hint}");
            assert!(
                !hint.contains("applied") && !hint.contains("answered"),
                "{hint}"
            );
        }
    }

    #[test]
    fn multi_submit_binds_the_current_question_identity_and_settings_use_their_own_door() {
        let shape = Shape::Multiple(vec![Offer::new("a", "First"), Offer::new("b", "Second")]);
        let waiting = typed(shape.clone(), "new-identity");
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(&waiting);
        composer.select_offer(&waiting, 1);
        let Offered::Answer(text) = composer.submit_choices(&waiting) else {
            panic!("explicit Submit");
        };
        assert_eq!(
            routed(&waiting, text),
            Said::Answer {
                text: r#"["b"]"#.into(),
                witness: "new-identity".into()
            }
        );
        let settings = Waiting::Selection {
            asked: Asked::new(
                "Intelligence",
                "",
                false,
                Shape::Choice(vec![Offer::new("2 deepseek", "DeepSeek")]),
                "settings",
                3,
            ),
        };
        assert_eq!(
            routed(&settings, "2 deepseek".into()),
            Said::Line("2 deepseek".into())
        );
        let proposal = Waiting::proposal_choice(
            "candidate",
            "format",
            Asked::new(
                "Format",
                "",
                false,
                Shape::Choice(vec![Offer::new("yes", "Markdown")]),
                "composite-proposal-question",
                3,
            ),
        );
        assert_eq!(
            routed(&proposal, "yes".into()),
            Said::Answer {
                text: "yes".into(),
                witness: "composite-proposal-question".into()
            }
        );
    }

    #[test]
    fn finite_action_echo_names_the_act_and_sends_the_same_scoped_protocol() {
        let asked = Asked::new(
            "Save?",
            "",
            true,
            Shape::Choice(vec![Offer::new("yes", "Save workflow")]),
            "action:7",
            7,
        );
        let waiting = Waiting::Action {
            key: "save".into(),
            asked,
        };
        let said = routed(&waiting, "yes".into());
        assert_eq!(said.display_text(&waiting), "Save workflow");
        assert_eq!(said.text(), "yes");
        let (busy, seen) = sinks();
        let mut doors = Doors::default();
        said.send(&mut doors, &busy, &seen);
        assert_eq!(doors.used, ["answer_bound action:7 [yes]"]);
        assert_eq!(
            routed(&waiting, "change the title".into()).display_text(&waiting),
            "change the title"
        );
    }

    #[test]
    fn choice_echo_uses_labels_but_sends_the_exact_keys_once() {
        let single = Shape::Choice(vec![Offer::new("detaille", "Détaillé")]);
        let multiple = Shape::Multiple(vec![
            Offer::new("chiffre_affaires", "Chiffre d'affaires"),
            Offer::new("classement_clients", "Classement clients"),
        ]);
        for (shape, raw, label) in [
            (single, "detaille", "Détaillé"),
            (
                multiple,
                r#"["chiffre_affaires","classement_clients"]"#,
                "Chiffre d'affaires, Classement clients",
            ),
        ] {
            for proposal in [false, true] {
                let asked = Asked::new("Question", "", false, shape.clone(), "current", 1);
                let waiting = if proposal {
                    Waiting::proposal_choice("proposal", "detail", asked)
                } else {
                    Waiting::asked("detail", asked)
                };
                let said = routed(&waiting, raw.into());
                assert_eq!(said.display_text(&waiting), label);
                assert_eq!(said.text(), raw);
                let (busy, seen) = sinks();
                let mut doors = Doors::default();
                said.send(&mut doors, &busy, &seen);
                assert_eq!(doors.used, [format!("answer_bound current [{raw}]")]);
            }
        }
    }

    #[test]
    fn choice_echo_preserves_invalid_free_and_stale_answers_verbatim() {
        let waiting = typed(
            Shape::Multiple(vec![
                Offer::new("revenue", "Revenue"),
                Offer::new("clients", "Customer ranking"),
            ]),
            "current",
        );
        for raw in [
            "  only revenue, and do not run  ",
            "unknown",
            r#"["revenue","unknown"]"#,
            r#"["revenue","revenue"]"#,
            r#"["revenue",3]"#,
            r#"["revenue""#,
            "[]",
        ] {
            let said = routed(&waiting, raw.into());
            assert_eq!(said.display_text(&waiting), raw);
            assert_eq!(said.text(), raw);
        }
        let said = routed(&waiting, r#"["revenue","clients"]"#.into());
        for other in [
            typed(
                Shape::Multiple(vec![
                    Offer::new("revenue", "Other revenue"),
                    Offer::new("clients", "Other clients"),
                ]),
                "stale",
            ),
            typed(Shape::Text, "current"),
            typed(Shape::Literal, "current"),
            Waiting::Free,
        ] {
            assert_eq!(said.display_text(&other), said.text());
        }
        let raw = r#"["revenue","clients"]"#;
        assert_eq!(Said::Line(raw.into()).display_text(&waiting), raw);
    }

    /// A row picked in the intelligence list echoes its name, never its key, even where the
    /// recommended row and the model found under it share that key; a step to another list
    /// echoes nothing; typed words and a typed command echo as typed, and a question's answer
    /// as its label, never after a prompt (live gauntlet r02 B2, r07 B9).
    #[test]
    fn an_intelligence_row_echoes_its_name_never_its_key() {
        let settings = Waiting::selection(Asked::new(
            "Choose a model · Agent account",
            "",
            false,
            Shape::Choice(vec![
                Offer::new("1 claude-code", "claude-code"),
                Offer::new("1 codex", ""),
                Offer::new("/intelligence connections", "Other choice…"),
            ]),
            "4:intelligence",
            4,
        ));
        let echo = |line: &str| routed(&settings, line.into()).echo(&settings, Voice::default());
        assert_eq!(
            echo("1 claude-code").as_deref(),
            Some("Intelligence: claude-code")
        );
        assert_eq!(echo("1 codex").as_deref(), Some("Intelligence: 1 codex"));
        assert_eq!(echo("/intelligence connections"), None);
        for words in ["hi there", "/intelligence", "1 claude"] {
            assert_eq!(echo(words).as_deref(), Some(words));
        }
        let home = Waiting::selection(Asked::new(
            "Intelligence for this conversation",
            "",
            false,
            Shape::Choice(vec![
                Offer::new(
                    "2 anthropic/claude-sonnet-5-5",
                    "Recommended for you · Claude Sonnet 5.5",
                ),
                Offer::new("cancel", "Keep current · GPT-6 Astra"),
                Offer::new("2 anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5"),
            ]),
            "5:intelligence",
            5,
        ));
        assert_eq!(
            routed(&home, "2 anthropic/claude-sonnet-5-5".into())
                .echo(&home, Voice::default())
                .as_deref(),
            Some("Intelligence: Claude Sonnet 5.5"),
            "one model under two rows reads by its name"
        );
        // `Esc` in a list below home sends its way back: no « You · choice › /intelligence ».
        let Waiting::Selection { mut asked } = settings.clone() else {
            panic!("a settings list")
        };
        asked.back = Some("/intelligence".to_owned());
        let below = Waiting::selection(asked);
        let back = routed(&below, "/intelligence".into()).echo(&below, Voice::default());
        assert_eq!(back, None, "the step back is no message");
        let question = typed(currencies(), "current");
        assert_eq!(
            routed(&question, " usd ".into())
                .echo(&question, Voice::default())
                .as_deref(),
            Some("US dollar, the key keeps its spaces")
        );
        let decision = Waiting::Action {
            key: "save".into(),
            asked: Asked::new(
                "Save?",
                "",
                true,
                Shape::Choice(vec![Offer::new("save & run", "Save and run once")]),
                "action:8",
                8,
            ),
        };
        for (line, echoed) in [
            ("save & run", "Save and run once"),
            ("J'ai changé d'avis", "J'ai changé d'avis"),
            ("/intelligence", "/intelligence"),
        ] {
            assert_eq!(
                routed(&decision, line.into())
                    .echo(&decision, Voice::default())
                    .as_deref(),
                Some(echoed),
                "no « action › » before the person's words"
            );
        }
        assert_eq!(
            Said::Line("/status".into())
                .echo(&Waiting::Free, Voice::default())
                .as_deref(),
            Some("/status"),
            "no « nika › » either"
        );
    }

    #[test]
    fn choice_echo_requires_exact_unambiguous_offer_keys() {
        let waiting = typed(currencies(), "current");
        assert_eq!(
            routed(&waiting, " usd ".into()).display_text(&waiting),
            "US dollar, the key keeps its spaces"
        );
        for raw in ["usd", " eur ", r#"["eur"]"#] {
            assert_eq!(routed(&waiting, raw.into()).display_text(&waiting), raw);
        }
        let ambiguous = typed(
            Shape::Choice(vec![Offer::new("a", "First"), Offer::new("a", "Second")]),
            "current",
        );
        assert_eq!(routed(&ambiguous, "a".into()).display_text(&ambiguous), "a");
    }

    /// A selected offer leaves as the native act it is: its exact keys by the
    /// pick door, echoed by label. Words that read as a key, or open as a
    /// list does, stay words; a conversation that knows no pick door gets the
    /// keys as an answer, as before.
    #[test]
    fn a_selected_offer_leaves_as_a_pick_and_own_words_as_an_answer() {
        let waiting = typed(currencies(), "w-3");
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(&waiting);
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert_eq!(
            composer.offer_selected(&waiting),
            Some(0),
            "a single choice opens focused on its first row"
        );
        let sent = composer.offer_key(&waiting, press(KeyCode::Enter));
        assert_eq!(sent, Offered::Answer("eur".to_owned()));
        assert_eq!(composer.in_flight(), Some(Sent::Offer));
        let pick = picked(&waiting, "eur".to_owned());
        assert_eq!(
            pick,
            Said::Pick {
                keys: "eur".to_owned(),
                witness: "w-3".to_owned()
            }
        );
        assert_eq!(pick.display_text(&waiting), "Euro");
        let (busy, seen) = sinks();
        let mut doors = Doors::default();
        pick.send(&mut doors, &busy, &seen);
        assert_eq!(
            doors.used,
            ["answer_bound w-3 [eur]"],
            "no pick door: an answer"
        );
        for words in ["eur", "[urgent] the CSV one"] {
            assert_eq!(
                routed(&waiting, words.to_owned()),
                Said::Answer {
                    text: words.to_owned(),
                    witness: "w-3".to_owned()
                },
                "own words stay words"
            );
        }
        assert_eq!(
            picked(&Waiting::Free, "eur".to_owned()),
            Said::Line("eur".to_owned())
        );
    }

    /// At the optional offer about the last result, the same bytes echo two ways: rows picked
    /// read as their labels, words typed as typed (they are the person's next line). A key typed
    /// at any other action still echoes the offer it names.
    #[test]
    fn the_offer_about_a_result_echoes_own_words_as_typed_and_picks_by_label() {
        let offers = vec![
            Offer::new("depth", "Depth of the summary"),
            Offer::new("topics", "Topics covered"),
            Offer::new("later", "Later"),
        ];
        let witness = "action:9";
        let action = |key: &str, shape: Shape| Waiting::Action {
            key: key.to_owned(),
            asked: Asked::new(
                "What would you like to adjust?",
                "",
                false,
                shape,
                witness,
                9,
            ),
        };
        let typed = |text: &str| Said::Answer {
            text: text.to_owned(),
            witness: witness.to_owned(),
        };
        let chosen = |keys: &str| Said::Pick {
            keys: keys.to_owned(),
            witness: witness.to_owned(),
        };
        let adjust = action("adjust", Shape::Multiple(offers.clone()));
        let set = r#"["depth","topics"]"#;
        assert_eq!(typed(set).display_text(&adjust), set);
        assert_eq!(
            chosen(set).display_text(&adjust),
            "Depth of the summary, Topics covered"
        );
        let opinion = action("opinion", Shape::Choice(offers));
        assert_eq!(typed("later").display_text(&opinion), "later");
        assert_eq!(chosen("later").display_text(&opinion), "Later");
        let save = action(
            "save",
            Shape::Choice(vec![Offer::new("yes", "Save workflow")]),
        );
        assert_eq!(
            typed("yes").display_text(&save),
            "Save workflow",
            "a typed key still names its offer"
        );
    }

    /// A conversation recording each door a line leaves by, the native pick door included.
    #[derive(Default)]
    struct Recorded {
        used: Vec<String>,
    }

    impl Conversation for Recorded {
        fn open(&mut self) -> Vec<Beat> {
            Vec::new()
        }

        fn submit(&mut self, line: &str) -> Turn {
            self.used.push(format!("submit {line}"));
            Turn {
                beats: vec![Beat::Wait(Waiting::Free)],
                handoff: None,
            }
        }

        fn answer_bound(
            &mut self,
            text: &str,
            witness: &str,
            _: &Sender<String>,
            _: &Seen,
        ) -> Turn {
            self.used.push(format!("answer_bound {witness} [{text}]"));
            Turn {
                beats: vec![Beat::Wait(Waiting::Free)],
                handoff: None,
            }
        }

        fn pick_bound(&mut self, keys: &str, witness: &str, _: &Sender<String>, _: &Seen) -> Turn {
            self.used.push(format!("pick_bound {witness} [{keys}]"));
            Turn {
                beats: vec![Beat::Wait(Waiting::Free)],
                handoff: None,
            }
        }

        fn perform(&mut self, _: &Handoff) -> Vec<Beat> {
            Vec::new()
        }
    }

    /// The optional offer about the last result, as the action of `kind` painted as `action:9`,
    /// with the Session's own choices: an opinion's (none recommended, « Later » alone), else an
    /// adjustment's.
    fn about_the_result(kind: nika_session::work::FeedbackKind) -> Waiting {
        let offers = |list: &[(&str, &str)]| -> Vec<Offer> {
            (list.iter())
                .map(|(key, label)| {
                    let offer = Offer::new(*key, *label);
                    if matches!(*key, "later" | "nothing") {
                        offer.exclusive()
                    } else {
                        offer
                    }
                })
                .collect()
        };
        let shape = if kind == nika_session::work::FeedbackKind::Opinion {
            Shape::Choice(offers(&[
                ("suitable", "Suitable for this use"),
                ("changes", "Still needs changes"),
                ("wrong", "Wrong result"),
                ("unevaluated", "Not evaluated yet"),
                ("later", "Later"),
            ]))
        } else {
            Shape::Multiple(offers(&[
                ("depth", "Depth of the summary"),
                ("topics", "Topics covered"),
                ("nothing", "Nothing to change"),
                ("later", "Later"),
            ]))
        };
        let mut asked = Asked::new("About this result?", "", false, shape, "action:9", 9);
        asked.free_text = true;
        Waiting::Action {
            key: kind.key().to_owned(),
            asked,
        }
    }

    /// What `Enter` leaves as after `typed` in the own reply of the choice `waiting` paints (a row
    /// first selected with `Down` when `down`): the composer's act, as what it was sent, and the
    /// line the shell makes of it ([`Shell::offered`]: the native pick, else the words routed).
    fn enter_after(
        waiting: &Waiting,
        typed: &str,
        down: bool,
    ) -> (Offered, Option<Sent>, Option<Said>) {
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(waiting);
        for c in typed.chars() {
            composer.own_key(press(KeyCode::Char(c)));
        }
        if down {
            composer.offer_key(waiting, press(KeyCode::Down));
        }
        let sent = composer.offer_key(waiting, press(KeyCode::Enter));
        let said = match &sent {
            Offered::Answer(text) if composer.in_flight() == Some(Sent::Offer) => {
                Some(picked(waiting, text.clone()))
            }
            Offered::Answer(text) => Some(routed(waiting, text.clone())),
            _ => None,
        };
        (sent, composer.in_flight(), said)
    }

    /// The doors `said` leaves by.
    fn doors(said: &Said) -> Vec<String> {
        let (busy, seen) = sinks();
        let mut conversation = Recorded::default();
        said.send(&mut conversation, &busy, &seen);
        conversation.used
    }

    /// At the optional offer about the last result, words typed are the person's own line, even a
    /// row's whole label in any letter case or « later »: `Enter` sends them as typed through the
    /// words door, never as a pick of that row through the pick door, so no opinion is recorded
    /// from words.
    #[test]
    fn a_label_typed_at_the_offer_about_a_result_leaves_as_the_persons_words() {
        let opinion = about_the_result(nika_session::work::FeedbackKind::Opinion);
        let typed = [
            "Suitable for this use",
            "suitable for this use",
            " SUITABLE FOR THIS USE ",
            "Later",
            "later",
            "Wrong result",
        ];
        for words in typed {
            let (sent, as_what, said) = enter_after(&opinion, words, false);
            assert_eq!(sent, Offered::Answer(words.to_owned()), "{words:?}");
            assert_eq!(as_what, Some(Sent::Own), "{words:?}: the person's own line");
            let Some(said) = said else {
                panic!("{words:?}: Enter sent nothing")
            };
            assert_eq!(
                said,
                Said::Answer {
                    text: words.to_owned(),
                    witness: "action:9".to_owned()
                },
                "{words:?}"
            );
            assert_eq!(
                doors(&said),
                [format!("answer_bound action:9 [{words}]")],
                "{words:?}: never the pick door"
            );
        }
    }

    /// Its rows answer it natively: a row selected, then `Enter`, leaves as the pick of its key
    /// through the pick door; `Enter` with nothing typed and nothing selected sends nothing (the
    /// opinion opens on nothing: no default stands for the person). At an adjustment too, words
    /// typed stay words.
    #[test]
    fn a_row_selected_at_the_offer_about_a_result_is_its_pick_and_nothing_selected_sends_nothing() {
        let opinion = about_the_result(nika_session::work::FeedbackKind::Opinion);
        assert_eq!(
            enter_after(&opinion, "", false),
            (Offered::Pass, None, None),
            "nothing typed, nothing selected"
        );
        let (sent, as_what, said) = enter_after(&opinion, "", true);
        assert_eq!(sent, Offered::Answer("suitable".to_owned()));
        assert_eq!(as_what, Some(Sent::Offer));
        let Some(said) = said else {
            panic!("Enter picked the row")
        };
        assert_eq!(
            said,
            Said::Pick {
                keys: "suitable".to_owned(),
                witness: "action:9".to_owned()
            }
        );
        assert_eq!(doors(&said), ["pick_bound action:9 [suitable]"]);
        let adjust = about_the_result(nika_session::work::FeedbackKind::Adjust);
        let (sent, as_what, _) = enter_after(&adjust, "Later", false);
        assert_eq!(
            (sent, as_what),
            (Offered::Answer("Later".to_owned()), Some(Sent::Own))
        );
    }

    /// Elsewhere a name typed whole still picks its offer, as selecting it would: an action that is
    /// no offer about a result (« Save workflow »), and a typed question (« euro »).
    #[test]
    fn a_name_typed_whole_elsewhere_still_picks_its_offer() {
        let mut asked = Asked::new(
            "Save it?",
            "",
            false,
            Shape::Choice(vec![
                Offer::new("yes", "Save workflow"),
                Offer::new("no", "Discard"),
            ]),
            "action:8",
            8,
        );
        asked.free_text = true;
        let save = Waiting::Action {
            key: "save".to_owned(),
            asked,
        };
        let (sent, as_what, said) = enter_after(&save, "Save workflow", false);
        assert_eq!(
            (sent, as_what),
            (Offered::Answer("yes".to_owned()), Some(Sent::Offer))
        );
        assert_eq!(
            said,
            Some(Said::Pick {
                keys: "yes".to_owned(),
                witness: "action:8".to_owned()
            })
        );
        let question = typed(currencies(), "w-5");
        let (sent, as_what, _) = enter_after(&question, "euro", false);
        assert_eq!(
            (sent, as_what),
            (Offered::Answer("eur".to_owned()), Some(Sent::Offer))
        );
    }
}
