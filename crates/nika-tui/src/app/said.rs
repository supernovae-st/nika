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
use crate::composer::answer::{Offered, Sent};
use crate::model::{Beat, Conversation, Turn, Waiting};
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
}

impl Said {
    /// The words sent, exactly.
    pub(super) fn text(&self) -> &str {
        match self {
            Self::Line(text) | Self::Answer { text, .. } => text,
        }
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
        }
    }
}

/// What `line`, sent while `waiting` is painted, is: the typed question's
/// answer bound to the identity painted, or a line to whatever waits (a
/// question kept in prose, a cost decision, a consent, a gate, the choice of
/// intelligence, the free prompt).
fn routed(waiting: &Waiting, line: String) -> Said {
    match waiting {
        Waiting::QuestionDocument { asked, .. } => Said::Answer {
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
            Offered::Answer(offer) => Picked::Sent(routed(&self.state.waiting, offer)),
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
            Unsent::Selected if self.composer.offer_selected(&self.state.waiting).is_some() => {
                STILL_SELECTED
            }
            Unsent::Words(_) | Unsent::Selected => NOTHING_SENT,
        };
        self.state.completion = Some(notice.to_owned());
    }
}

#[cfg(test)]
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
        for waiting in [
            Waiting::Free,
            Waiting::Choosing,
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
        composer.offer_key(&again, key(KeyCode::Down));
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
}
