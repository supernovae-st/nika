// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The offers of the typed choice on screen, which one the human selected,
//! and the human's own reply: presentation only. A selection is never an
//! answer and grants nothing. Only `Enter` sends, exactly once: the own reply
//! when it holds words, else the selected offer's key, and the shell binds it
//! to the question painted at that moment.
//!
//! The typed choice opens as the surface above the line (choice mode): the
//! ordinary draft waits out of view, untouched, its caret and selection
//! with it, and the line shows the own reply's field instead
//! ([`Answer::own`]). The offers take keys while the composer holds them,
//! nothing lists commands, nothing works and no answer is in flight. `Up`,
//! `Down`, `Home` and `End` select: nothing is preselected, and the first
//! `Down` selects the first offer. The offers never write a field. Typeahead
//! never reaches them, because the typeahead law drops `Enter` and the arrows
//! typed while Nika works.
//!
//! A selection belongs to one logical question: the epoch of the session that
//! asked it, its key and its exact offers (keys and labels, in order). That
//! question asked again under a new identity (after a settings change, behind
//! an intermediate choice) keeps an unsubmitted selection, and `Enter` then
//! binds the identity painted now. Another scope, a question that no longer
//! waits, or a selection already sent and taken starts with nothing selected.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use ratatui_textarea::{Input, Key, TextArea};

use super::Composer;
use crate::model::{Asked, Offer, Retained, Shape, Waiting};

/// What `Enter` sent while a typed question was painted, until its turn
/// returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sent {
    /// The selected offer's key.
    Offer,
    /// The draft: the human's own words, under a question with no offers.
    Draft,
    /// The own reply typed in the typed choice's own field.
    Own,
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

/// The key a selection of the knowledge decision belongs to.
const KNOWLEDGE: &str = "knowledge";

/// The typed choice `waiting` paints: its key, its question and its offers
/// (at least one). A typed question's, or the knowledge decision a held line
/// waits for, whose offers are the exact lines the Session reads.
pub(crate) fn choice(waiting: &Waiting) -> Option<(&str, &Asked, &[Offer])> {
    let (key, asked) = match waiting {
        Waiting::QuestionDocument { key, asked }
        | Waiting::ProposalChoice { key, asked, .. }
        | Waiting::Action { key, asked } => (key.as_str(), asked),
        Waiting::Knowledge { asked } => (KNOWLEDGE, asked),
        Waiting::Selection { asked } => ("intelligence", asked),
        _ => return None,
    };
    match &asked.shape {
        Shape::Choice(offers) | Shape::Multiple(offers) if !offers.is_empty() => {
            Some((key, asked, offers.as_slice()))
        }
        Shape::Choice(_) | Shape::Multiple(_) | Shape::Text | Shape::Literal => None,
    }
}

/// The semantic door owning a surface. Its kind cannot collide with a
/// question's free-form key, even when that key is `intelligence` or `knowledge`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Question,
    Proposal,
    Knowledge,
    Settings,
    Action,
}

impl ScopeKind {
    fn of(waiting: &Waiting) -> Self {
        match waiting {
            Waiting::ProposalChoice { .. } => Self::Proposal,
            Waiting::Selection { .. } => Self::Settings,
            Waiting::Action { .. } => Self::Action,
            Waiting::Knowledge { .. } => Self::Knowledge,
            _ => Self::Question,
        }
    }
}

/// The logical question a selection belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    kind: ScopeKind,
    proposal: Option<String>,
    epoch: u64,
    key: String,
    offers: Vec<Offer>,
    label: String,
    why: String,
    multiple: bool,
    free_text: bool,
    retained: Option<Retained>,
}

impl Scope {
    /// The scope of the typed choice `key` asked as `asked` with `offers`.
    fn of(waiting: &Waiting, key: &str, asked: &Asked, offers: &[Offer]) -> Self {
        Self {
            kind: ScopeKind::of(waiting),
            proposal: match waiting {
                Waiting::ProposalChoice { proposal, .. } => Some(proposal.clone()),
                Waiting::Action { asked, .. } => Some(asked.witness.clone()),
                _ => None,
            },
            epoch: asked.epoch,
            key: key.to_owned(),
            offers: offers.to_vec(),
            label: asked.label.clone(),
            why: asked.why.clone(),
            multiple: matches!(asked.shape, Shape::Multiple(_)),
            free_text: asked.free_text,
            retained: asked.retained.clone(),
        }
    }

    /// Whether that typed choice is this logical question, whatever its
    /// identity now.
    fn holds(&self, waiting: &Waiting, key: &str, asked: &Asked, offers: &[Offer]) -> bool {
        self.kind == ScopeKind::of(waiting)
            && self.proposal.as_deref()
                == match waiting {
                    Waiting::ProposalChoice { proposal, .. } => Some(proposal.as_str()),
                    Waiting::Action { asked, .. } => Some(asked.witness.as_str()),
                    _ => None,
                }
            && self.epoch == asked.epoch
            && self.key == key
            && self.offers == offers
            && self.label == asked.label
            && self.why == asked.why
            && self.multiple == matches!(asked.shape, Shape::Multiple(_))
            && self.free_text == asked.free_text
            && self.retained == asked.retained
    }
}

/// The selection the composer keeps between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct Answer {
    scope: Option<Scope>,
    suspended: Option<Box<Self>>,
    selected: Option<usize>,
    checked: Vec<usize>,
    sent: Option<Sent>,
    /// The human's own reply to the typed choice: a field of its own, so the
    /// ordinary draft never changes under a question. It belongs to the
    /// logical question, as the selection does.
    own: TextArea<'static>,
}

/// The own reply's field, empty: wrapped and marked as the line is, its
/// cursor shown only while the composer holds the keys.
fn own_field(focused: bool) -> TextArea<'static> {
    let mut own = TextArea::default();
    own.set_wrap_mode(super::WRAP);
    own.set_cursor_line_style(Style::default());
    own.set_cursor_style(if focused {
        super::CURSOR
    } else {
        Style::default()
    });
    own.set_placeholder_text("or type your own reply");
    own.set_placeholder_style(super::PLACEHOLDER);
    own
}

impl Answer {
    /// The own reply's cursor follows the keys, as the line's does.
    pub(super) fn focus(&mut self, focused: bool) {
        let cursor = if focused && self.scope.as_ref().is_none_or(|scope| scope.free_text) {
            super::CURSOR
        } else {
            self.own.cursor_line_style()
        };
        self.own.set_cursor_style(cursor);
    }
}

impl Answer {
    /// The selection of the typed choice `waiting` paints, when it is that
    /// question's.
    fn selection(&self, waiting: &Waiting) -> Option<usize> {
        let (key, asked, offers) = choice(waiting)?;
        let held = self
            .scope
            .as_ref()
            .is_some_and(|scope| scope.holds(waiting, key, asked, offers));
        let count = offers.len() + usize::from(matches!(asked.shape, Shape::Multiple(_)));
        self.selected.filter(|at| held && *at < count)
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
        let settings = matches!(waiting, Waiting::Selection { .. });
        if settings
            && self
                .scope
                .as_ref()
                .is_some_and(|scope| scope.kind != ScopeKind::Settings)
        {
            let previous = std::mem::take(self);
            self.suspended = Some(Box::new(previous));
        } else if !settings && let Some(previous) = self.suspended.take() {
            *self = *previous;
        }
        let kept = chosen.is_some_and(|(key, asked, offers)| {
            (self.scope.as_ref()).is_some_and(|scope| scope.holds(waiting, key, asked, offers))
        });
        if !kept {
            self.scope = chosen.map(|(key, asked, offers)| Scope::of(waiting, key, asked, offers));
            self.selected = None;
            self.checked.clear();
            self.own = own_field(true);
            if chosen.is_some_and(|(_, asked, _)| !asked.free_text) {
                self.own.set_placeholder_text("choose an offered answer");
                self.own.set_cursor_style(Style::default());
            }
        }
    }
}

impl Composer {
    /// A wait the shell applies, in order: the selection follows the logical
    /// question it belongs to ([`Answer`]'s rules), and a typed choice holds
    /// the line while it waits.
    pub(crate) fn follow(&mut self, waiting: &Waiting) {
        self.answer.follow(waiting);
        self.chooser.hold(choice(waiting).is_some());
    }

    /// The offer selected for the typed choice `waiting` paints, if any.
    pub(crate) fn offer_selected(&self, waiting: &Waiting) -> Option<usize> {
        self.answer.selection(waiting)
    }

    /// Whether the offers of `waiting` take a selection now: a typed choice is
    /// painted, nothing lists commands and no answer is in flight; the
    /// ordinary draft waits out of view whatever it holds. A press may select
    /// then; keys also need the composer to hold them
    /// ([`Self::offers_armed`]).
    pub(crate) fn offers_open(&self, waiting: &Waiting) -> bool {
        choice(waiting).is_some() && self.listing().is_none() && self.answer.sent.is_none()
    }

    /// Whether `waiting`'s typed choice holds the line (choice mode): nothing
    /// lists commands, and the own reply's field stands in the draft's place.
    pub(crate) fn choosing(&self, waiting: &Waiting) -> bool {
        choice(waiting).is_some() && self.listing().is_none()
    }

    /// The own reply as typed, exactly.
    pub(crate) fn own_reply(&self) -> String {
        self.answer.own.lines().join("\n")
    }

    /// One key for the own reply's field: a character, a correction, a move
    /// along the line, or a line break (`Alt+Enter`, `Ctrl+J`). `false` when
    /// the key is not an edit (the shell's keys, the offers' keys, `Enter`).
    pub(crate) fn own_key(&mut self, key: KeyEvent) -> bool {
        if self
            .answer
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.free_text)
        {
            return false;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let own = &mut self.answer.own;
        let typed = match key.code {
            KeyCode::Enter if alt || shift || ctrl => {
                own.insert_newline();
                return true;
            }
            KeyCode::Char('j') if ctrl => {
                own.insert_newline();
                return true;
            }
            KeyCode::Char(c) if ctrl && matches!(c, 'c' | 't' | 'o' | 'l' | 'd' | 'z') => {
                return false;
            }
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Delete => Key::Delete,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            _ => return false,
        };
        own.input(Input {
            key: typed,
            ctrl,
            alt,
            shift,
        });
        true
    }

    /// Pasted text into the own reply's field, as data.
    pub(crate) fn paste_own(&mut self, text: &str) {
        if self
            .answer
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.free_text)
        {
            return;
        }
        (self.answer.own).insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
    }

    /// Words of an own reply that was not taken back in its field, first,
    /// before whatever it holds now.
    pub(crate) fn put_back_own(&mut self, words: &str) {
        let rest = self.own_reply();
        self.answer.own = own_field(self.focused());
        self.paste_own(words);
        if !rest.trim().is_empty() {
            self.paste_own("\n");
            self.paste_own(&rest);
        }
    }

    /// Draw the own reply's field into `area`.
    pub(crate) fn render_own(&self, area: Rect, buf: &mut Buffer) {
        (&self.answer.own).render(area, buf);
    }

    /// Whether the offers of `waiting` take keys now: they are open and the
    /// composer holds the keys.
    pub(crate) fn offers_armed(&self, waiting: &Waiting) -> bool {
        self.focused() && self.offers_open(waiting)
    }

    /// Whether a checkbox belongs to this exact logical question.
    pub(crate) fn offer_checked(&self, waiting: &Waiting, index: usize) -> bool {
        let Some((key, asked, offers)) = choice(waiting) else {
            return false;
        };
        self.answer
            .scope
            .as_ref()
            .is_some_and(|scope| scope.holds(waiting, key, asked, offers))
            && self.answer.checked.contains(&index)
    }

    /// How many currently offered answers are checked.
    #[cfg(test)]
    pub(crate) fn checked_count(&self, waiting: &Waiting) -> usize {
        choice(waiting).map_or(0, |(_, _, offers)| {
            (0..offers.len())
                .filter(|index| self.offer_checked(waiting, *index))
                .count()
        })
    }

    /// Toggle one offered answer. Navigation and toggling never send.
    fn toggle_offer(&mut self, index: usize) {
        if let Some(at) = self.answer.checked.iter().position(|held| *held == index) {
            self.answer.checked.remove(at);
        } else {
            self.answer.checked.push(index);
        }
    }

    /// Explicit Submit, shared by the keyboard and the painted button.
    /// The selected keys are serialized losslessly in the Session's offer order.
    pub(crate) fn submit_choices(&mut self, waiting: &Waiting) -> Offered {
        if !self.offers_open(waiting) {
            return Offered::Pass;
        }
        let Some((_, asked, offers)) = choice(waiting) else {
            return Offered::Pass;
        };
        if !matches!(asked.shape, Shape::Multiple(_)) {
            return Offered::Pass;
        }
        self.answer.follow(waiting);
        if let Some(own) = self.send_own() {
            return Offered::Answer(own);
        }
        let keys: Vec<&str> = offers
            .iter()
            .enumerate()
            .filter(|(index, _)| self.answer.checked.contains(index))
            .map(|(_, offer)| offer.key.as_str())
            .collect();
        if keys.is_empty() {
            return Offered::Moved;
        }
        let Ok(text) = serde_json::to_string(&keys) else {
            return Offered::Moved;
        };
        self.answer.sent = Some(Sent::Offer);
        Offered::Answer(text)
    }

    /// Send an own reply before any offered selection, without touching the draft.
    fn send_own(&mut self) -> Option<String> {
        let own = self.own_reply();
        if own.trim().is_empty() {
            return None;
        }
        self.answer.own = own_field(true);
        self.history.push(own.clone());
        self.answer.sent = Some(Sent::Own);
        Some(own)
    }

    /// One key for the offers. A multiple choice has a final Submit row;
    /// Space or Enter on an option only toggles its checkbox.
    pub(crate) fn offer_key(&mut self, waiting: &Waiting, key: KeyEvent) -> Offered {
        if !key.modifiers.is_empty() || !self.offers_armed(waiting) {
            return Offered::Pass;
        }
        let Some((_, asked, offers)) = choice(waiting) else {
            return Offered::Pass;
        };
        self.answer.follow(waiting);
        let multiple = matches!(asked.shape, Shape::Multiple(_));
        let at = self.answer.selection(waiting);
        let last = offers.len().saturating_sub(1) + usize::from(multiple);
        let next = match key.code {
            KeyCode::Down => at.map_or(0, |at| at.saturating_add(1).min(last)),
            KeyCode::Up => at.map_or(last, |at| at.saturating_sub(1)),
            KeyCode::Home => 0,
            KeyCode::End => last,
            KeyCode::Tab if multiple => last,
            KeyCode::Esc if matches!(waiting, Waiting::Action { .. }) => {
                let Some(cancel) = offers
                    .iter()
                    .find(|offer| matches!(offer.key.as_str(), "no" | "cancel"))
                else {
                    return Offered::Moved;
                };
                self.answer.sent = Some(Sent::Offer);
                return Offered::Answer(cancel.key.clone());
            }
            KeyCode::Esc if matches!(waiting, Waiting::Selection { .. }) => {
                if asked.mandatory {
                    return Offered::Moved;
                }
                self.answer.sent = Some(Sent::Offer);
                return Offered::Answer("cancel".to_owned());
            }
            KeyCode::Char(' ') if multiple && self.own_reply().is_empty() => {
                if let Some(index) = at.filter(|index| *index < offers.len()) {
                    self.toggle_offer(index);
                    return Offered::Moved;
                }
                // Space on Submit belongs to the control, not an invisible
                // free reply that disables subsequent checkbox toggles.
                return if at == Some(offers.len()) || !asked.free_text {
                    Offered::Moved
                } else {
                    Offered::Pass
                };
            }
            KeyCode::Enter => {
                if let Some(own) = self.send_own() {
                    return Offered::Answer(own);
                }
                if multiple {
                    if at == Some(offers.len()) {
                        return self.submit_choices(waiting);
                    }
                    if let Some(index) = at {
                        self.toggle_offer(index);
                    }
                    return Offered::Moved;
                }
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

    /// A press selects a radio option or toggles a checkbox; it sends nothing.
    pub(crate) fn select_offer(&mut self, waiting: &Waiting, index: usize) -> bool {
        let Some((_, asked, offers)) = choice(waiting) else {
            return false;
        };
        let multiple = matches!(asked.shape, Shape::Multiple(_));
        let count = offers.len() + usize::from(multiple);
        if index >= count || !self.offers_open(waiting) {
            return false;
        }
        self.answer.follow(waiting);
        self.answer.selected = Some(index);
        if multiple && index < offers.len() {
            self.toggle_offer(index);
        }
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
        if matches!(sent, Some(Sent::Offer | Sent::Own)) && !not_taken {
            let suspended = self.answer.suspended.take();
            self.answer = Answer {
                suspended,
                ..Answer::default()
            };
        }
        sent
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::composer::ComposerAction;

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

    fn action(witness: &str) -> Waiting {
        let mut asked = Asked::new(
            "Continue this run once?",
            "",
            true,
            Shape::Choice(vec![
                Offer::new("yes", "Run once"),
                Offer::new("no", "Cancel this run"),
            ]),
            witness,
            9,
        );
        asked.free_text = false;
        Waiting::Action {
            key: "run_cost".into(),
            asked,
        }
    }

    #[test]
    fn a_finite_action_has_no_default_and_only_enter_sends_the_exact_key_once() {
        let painted = action("action:1");
        let mut composer = composer(&painted);
        assert_eq!(composer.offer_selected(&painted), None);
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Enter)),
            Offered::Pass
        );
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Down)),
            Offered::Moved
        );
        assert_eq!(composer.offer_selected(&painted), Some(0));
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Enter)),
            Offered::Answer("yes".into())
        );
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Enter)),
            Offered::Pass
        );
        assert_eq!(composer.answer_returned(false), Some(Sent::Offer));
        let next = action("action:2");
        composer.follow(&next);
        assert_eq!(
            composer.offer_selected(&next),
            None,
            "new decision has no old yes selected"
        );
        assert_eq!(
            composer.offer_key(&next, key(KeyCode::Enter)),
            Offered::Pass
        );
        assert_eq!(
            composer.offer_key(&next, key(KeyCode::Esc)),
            Offered::Answer("no".into())
        );
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

    /// The offers take keys whatever the draft waiting out of view holds
    /// (typed or pasted words never make them inert, and a slash draft opens
    /// no list over the typed choice), never while the palette lists, and for
    /// keys only while the composer holds them; a press still selects there,
    /// and nothing writes the draft.
    #[test]
    fn the_offers_take_keys_whatever_the_hidden_draft_holds() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        composer.paste("eur");
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Down)),
            Offered::Moved,
            "words in the draft change nothing"
        );
        assert!(composer.select_offer(&painted, 1));
        assert_eq!(composer.text(), "eur", "the draft stays exact");
        composer.clear();
        composer.paste("/s");
        assert!(
            composer.listing().is_none(),
            "no slash list over the choice"
        );
        assert!(composer.offers_open(&painted));
        composer.clear();
        composer.toggle_palette();
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Down)),
            Offered::Pass
        );
        composer.close_palette();
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

    /// The own reply has its own field: typed words, corrections and pastes
    /// land there and never in the draft; `Enter` sends them before any
    /// selection, empties the field, keeps them in history and puts the
    /// offers in flight; with no words and no selection it sends nothing.
    #[test]
    fn the_own_reply_is_sent_before_a_selection_and_never_touches_the_draft() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        composer.paste("my draft");
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Enter)),
            Offered::Pass,
            "nothing to send"
        );
        composer.offer_key(&painted, key(KeyCode::Down));
        for c in "two currencies".chars() {
            assert!(composer.own_key(key(KeyCode::Char(c))));
        }
        assert!(composer.own_key(key(KeyCode::Backspace)));
        composer.paste_own("s");
        assert_eq!(composer.own_reply(), "two currencies");
        assert!(!composer.own_key(key(KeyCode::Enter)), "Enter is no edit");
        let stop = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!composer.own_key(stop), "Ctrl+C keeps its place");
        let sent = composer.offer_key(&painted, key(KeyCode::Enter));
        assert_eq!(sent, Offered::Answer("two currencies".to_owned()));
        assert!(composer.own_reply().is_empty(), "the field is taken");
        assert!(!composer.offers_open(&painted), "in flight");
        assert_eq!(composer.text(), "my draft", "the draft never changed");
        assert_eq!(composer.answer_returned(false), Some(Sent::Own));
        // Kept in history, as any line `Enter` sends.
        composer.clear();
        assert_eq!(composer.handle(key(KeyCode::Up)), ComposerAction::Edited);
        assert_eq!(composer.text(), "two currencies");
    }

    /// An own reply that was not taken comes back to its field first, before
    /// what it holds now; the same logical question keeps the field under a
    /// new identity, and another question starts it empty.
    #[test]
    fn an_own_reply_comes_back_to_its_field_and_follows_its_question() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        for c in "later words".chars() {
            composer.own_key(key(KeyCode::Char(c)));
        }
        composer.put_back_own("first words");
        assert_eq!(composer.own_reply(), "first words\nlater words");
        composer.follow(&choosing("w2"));
        assert_eq!(
            composer.own_reply(),
            "first words\nlater words",
            "the same question"
        );
        let other = asked("const.other", offers(), "w3", 7);
        composer.follow(&other);
        assert!(composer.own_reply().is_empty(), "another question");
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

#[cfg(test)]
mod multi_tests;
