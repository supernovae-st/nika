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
//! `Down`, `Home` and `End` select. Every single choice opens focused (its
//! recommendation, else its first row), so the row drawn selected is the one
//! `Enter` applies and the first `Down` reaches the second; a multiple choice
//! opens with each recommendation checked, and a decision opens with nothing
//! selected, its first `Down` selecting its first act. A question the author
//! asked lists « Let Nika choose » after its offers ([`DELEGATION`]). A
//! selection sends nothing: only `Enter` does. The offers never write a
//! field. Typeahead never reaches them, because the typeahead law drops
//! `Enter` and the arrows typed while Nika works.
//!
//! `Esc` steps back one level and never answers: it empties the own reply
//! first. With nothing typed, a settings list closes at once, whatever row is
//! selected (browsing is no choice): back to the list above it (the line its
//! [`Asked`] names) or, at its top, the current choice kept. On a decision or
//! a question it empties the selection, then sends nothing at all and says
//! so, so no `no`, `cancel` or discard row is ever chosen by it.
//!
//! A selection belongs to one logical question: the epoch of the session that
//! asked it, its key and its exact offers (keys and labels, in order). That
//! question asked again under a new identity (after a settings change, behind
//! an intermediate choice) keeps an unsubmitted selection, and `Enter` then
//! binds the identity painted now. Another scope, a question that no longer
//! waits, or a selection already sent and taken starts with nothing selected.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_session::work::FeedbackKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use ratatui_textarea::{Input, Key, TextArea};

use super::Composer;
use crate::lang::Voice;
use crate::model::{Asked, Offer, Retained, Shape, Waiting};

mod rows;
pub(crate) use rows::expert_route;

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
    /// `Enter` on « Let Nika choose »: the choice is left to Nika, an act
    /// that names no offer, whatever a label reads.
    Delegate,
    /// The key was taken and nothing was sent; the hint row says why.
    Notice(&'static str),
}

/// The key a selection of the knowledge decision belongs to.
const KNOWLEDGE: &str = "knowledge";

/// The acts a decision confirms before doing them, nothing undoing them: the
/// decision's key, the act's key, and the question the hint row asks. The
/// first `Enter` on such an act asks it; the next `Enter` does the act.
const DESTRUCTIVE: [(&str, &str, &str); 2] = [
    (
        "save",
        "no",
        "Discard this proposal? Enter again · Esc keeps it",
    ),
    (
        "judgment",
        "cancel",
        "Discard the unqualified candidate? Enter again · Esc keeps it",
    ),
];

/// `Esc` while a destructive act waits for its confirmation: nothing is done.
const KEPT_IT: &str = "kept · nothing was discarded";

/// What a settings list that keeps a usable choice sends on `Esc` at its top
/// level: the Session's own word for leaving the current selection as it is.
const KEEP_CURRENT: &str = "cancel";

/// `Esc` on a decision or a question: nothing is answered by it.
pub(crate) const ESC_ANSWERS_NOTHING: &str =
    "nothing sent · Esc never answers · ↑↓ choose, then Enter";

/// `Esc` on a settings list no usable choice stands behind.
const ESC_REQUIRED: &str = "an intelligence is required · ↑↓ choose, then Enter";

/// `Enter` on a multiple choice with nothing checked and no own reply.
const NOTHING_CHECKED: &str = "nothing sent · Space checks an answer, then Enter validates";

/// The words of the « Let Nika choose » act ([`Offered::Delegate`]), as the
/// conversation echoes and records them; a conversation that knows no such act
/// receives them as an answer ([`crate::model::Conversation::delegate_bound`]).
pub(crate) const DELEGATION: &str = "I leave the choice to Nika.";

/// How far `PgUp` and `PgDn` move the selection: the rows an inline band shows.
const PAGE: usize = crate::render::surface::SHOWN;

/// The key of a workflow's approval gate ([`Scope::approval`]).
const GATE: &str = "gate";

/// What the hint row says when `Enter` finds nothing to send at an approval
/// gate: words that are no yes, no or why, or nothing typed or selected.
pub(crate) const YES_OR_NO: &str = "nothing sent · type yes or no, or ↑↓ choose, then Enter";

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

/// The one offer `words` name ([`rows::named`]), and at an approval gate
/// (`approval`) the act a whole-line yes, no or why names
/// ([`rows::yes_or_no`]).
fn named<'a>(words: &str, offers: &'a [Offer], approval: bool) -> Option<(usize, &'a Offer)> {
    rows::named(words, offers)
        .or_else(|| approval.then(|| rows::yes_or_no(words, offers)).flatten())
}

/// Whether words typed at the action `key` are the person's next line rather than its answer:
/// the optional offer about the last result, by the Session's own keys for it. Its rows alone
/// pick there ([`Scope::rows_only`]).
pub(crate) fn words_are_lines(key: &str) -> bool {
    [FeedbackKind::Adjust, FeedbackKind::Opinion]
        .iter()
        .any(|kind| kind.key() == key)
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
    /// A settings list's search index, built once when it opens: each offer's
    /// words (key, label, note) lowercased, so typing a filter allocates and
    /// lowercases nothing per row ([`rows`]). Empty where nothing filters.
    haystacks: Vec<String>,
}

impl Scope {
    /// The scope of the typed choice `key` asked as `asked` with `offers`.
    fn of(waiting: &Waiting, key: &str, asked: &Asked, offers: &[Offer]) -> Self {
        let kind = ScopeKind::of(waiting);
        let haystacks = if kind == ScopeKind::Settings {
            offers.iter().map(rows::haystack).collect()
        } else {
            Vec::new()
        };
        Self {
            haystacks,
            kind,
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

    /// Whether this is a workflow's approval gate: its own field takes a
    /// whole-line yes, no or why that picks its act ([`rows::yes_or_no`]),
    /// never other words, so nothing answers it for the person.
    fn approval(&self) -> bool {
        self.kind == ScopeKind::Action && self.key == GATE
    }

    /// Whether this is the optional offer about the last result ([`words_are_lines`]): words
    /// typed there, even a row's whole label, are the person's own line, never a pick of the row
    /// they name; its rows alone pick.
    fn rows_only(&self) -> bool {
        self.kind == ScopeKind::Action && words_are_lines(&self.key)
    }

    /// Whether its own field takes words: a reply, a settings list's filter,
    /// or an approval gate's yes or no. A question the author asked always
    /// takes a reply, closed or not: the Session reads it, a delegation
    /// (« choisis pour moi ») too; only a decision that takes no reply keeps
    /// its rows alone.
    fn takes_words(&self) -> bool {
        self.free_text || self.asks() || self.approval()
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

    /// Whether this is a question the author asked (a typed question, or a
    /// proposal's choice), never a decision, a setting or the knowledge choice.
    fn asks(&self) -> bool {
        matches!(self.kind, ScopeKind::Question | ScopeKind::Proposal)
    }

    /// Whether its offers wear the digit that selects them: a question the
    /// author asked, or a decision (Save, Discard, a missed run's handling).
    /// A settings list keeps its digits for the expert route.
    fn numbered(&self) -> bool {
        self.asks() || self.kind == ScopeKind::Action
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
    /// The destructive act whose confirmation the next `Enter` gives, by its
    /// offer's index ([`DESTRUCTIVE`]); any other key drops it.
    confirm: Option<usize>,
    /// The human's own reply to the typed choice: a field of its own, so the
    /// ordinary draft never changes under a question. It belongs to the
    /// logical question, as the selection does.
    own: TextArea<'static>,
}

/// The own reply's field, empty: wrapped and marked as the line is, its
/// cursor shown only while the composer holds the keys.
fn own_field(focused: bool, voice: Voice) -> TextArea<'static> {
    let mut own = TextArea::default();
    own.set_wrap_mode(super::WRAP);
    own.set_cursor_line_style(Style::default());
    own.set_cursor_style(if focused {
        super::CURSOR
    } else {
        Style::default()
    });
    own.set_placeholder_text(voice.say("or type your own reply"));
    own.set_placeholder_style(super::PLACEHOLDER);
    own
}

impl Answer {
    /// An empty own field for this selection's question, its placeholder saying
    /// what typing does there: a reply, a settings list's filter, or nothing
    /// (a choice that takes no words of its own).
    fn emptied(&self, focused: bool, voice: Voice) -> TextArea<'static> {
        let mut own = own_field(focused, voice);
        own.set_placeholder_text(self.placeholder(voice));
        if self
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.takes_words())
        {
            own.set_cursor_style(Style::default());
        }
        own
    }

    /// The own field on screen says what typing does in `voice`, its words kept.
    pub(super) fn speak(&mut self, voice: Voice) {
        let words = self.placeholder(voice);
        self.own.set_placeholder_text(words);
    }

    /// What typing does under this selection's question, said in `voice`: a
    /// reply, an approval's two words, nothing (a choice that takes no words
    /// of its own), a settings list's filter.
    fn placeholder(&self, voice: Voice) -> &'static str {
        voice.say(match self.scope.as_ref() {
            Some(scope) if !scope.free_text && scope.approval() => "or type yes or no",
            Some(scope) if !scope.takes_words() => "choose an offered answer",
            Some(scope) if scope.kind == ScopeKind::Settings => "type to filter",
            _ => "or type your own reply",
        })
    }

    /// The own reply's cursor follows the keys, as the line's does.
    pub(super) fn focus(&mut self, focused: bool) {
        let cursor = if focused && self.scope.as_ref().is_none_or(Scope::takes_words) {
            super::CURSOR
        } else {
            self.own.cursor_line_style()
        };
        self.own.set_cursor_style(cursor);
    }
}

impl Answer {
    /// Whether this selection belongs to the typed choice `waiting` paints.
    fn holds(&self, waiting: &Waiting) -> bool {
        choice(waiting).is_some_and(|(key, asked, offers)| {
            (self.scope.as_ref()).is_some_and(|scope| scope.holds(waiting, key, asked, offers))
        })
    }

    /// The selection of the typed choice `waiting` paints, when it is that
    /// question's and its row is shown ([`rows`]).
    fn selection(&self, waiting: &Waiting) -> Option<usize> {
        let (_, asked, offers) = choice(waiting)?;
        let multiple = matches!(asked.shape, Shape::Multiple(_));
        self.selected
            .filter(|at| self.holds(waiting) && self.rows(offers, multiple).contains(at))
    }

    /// A wait about to be painted. An intermediate decision (the intelligence
    /// choice, a question in prose such as a cost decision) keeps what was
    /// selected; the same logical choice keeps it under its new identity;
    /// anything else starts afresh, as it opens.
    fn follow(&mut self, waiting: &Waiting, voice: Voice) {
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
            // Every single choice opens focused, so the row drawn selected is the one
            // `Enter` applies and `Down` reaches the second: a settings list on its
            // recommendation, a question on the author's one, else on their first row.
            // A multiple choice opens with each recommendation checked; a decision opens
            // on nothing. Nothing is sent until `Enter`.
            let asks = self.scope.as_ref().is_some_and(Scope::asks);
            let first = |offers: &[Offer]| offers.iter().position(|offer| !offer.hidden());
            self.selected = chosen.and_then(|(_, asked, offers)| match asked.shape {
                _ if settings => {
                    (asked.preselect.filter(|at| *at < offers.len())).or_else(|| first(offers))
                }
                Shape::Choice(_) if asks => rows::recommendation(offers).or_else(|| first(offers)),
                _ => None,
            });
            self.checked = (chosen.filter(|_| asks))
                .filter(|(_, asked, _)| matches!(asked.shape, Shape::Multiple(_)))
                .map(|(_, _, offers)| {
                    (offers.iter().enumerate())
                        .filter(|(_, offer)| offer.recommended)
                        .map(|(at, _)| at)
                        .collect()
                })
                .unwrap_or_default();
            self.own = self.emptied(true, voice);
            self.confirm = None;
        }
    }
}

impl Composer {
    /// A wait the shell applies, in order: the selection follows the logical
    /// question it belongs to ([`Answer`]'s rules), and a typed choice holds
    /// the line while it waits.
    pub(crate) fn follow(&mut self, waiting: &Waiting) {
        self.answer.follow(waiting, self.voice);
        self.chooser.hold(choice(waiting).is_some());
    }

    /// The offer selected for the typed choice `waiting` paints, if any.
    pub(crate) fn offer_selected(&self, waiting: &Waiting) -> Option<usize> {
        self.answer.selection(waiting)
    }

    /// The rows `waiting`'s typed choice shows now, by each offer's own index,
    /// then a multiple choice's Submit row: what painting, the pointer and
    /// the keys read ([`rows`]). Every offer while the selection belongs to
    /// another question.
    pub(crate) fn offer_rows(&self, waiting: &Waiting) -> Vec<usize> {
        let Some((_, asked, offers)) = choice(waiting) else {
            return Vec::new();
        };
        let multiple = matches!(asked.shape, Shape::Multiple(_));
        if self.answer.holds(waiting) {
            self.answer.rows(offers, multiple)
        } else {
            let shown = (offers.iter().enumerate())
                .filter(|(_, offer)| !offer.hidden())
                .map(|(index, _)| index);
            let asks = matches!(
                ScopeKind::of(waiting),
                ScopeKind::Question | ScopeKind::Proposal
            );
            (shown.chain(rows::delegation(asks, offers)))
                .chain(multiple.then_some(offers.len()))
                .collect()
        }
    }

    /// The words a settings list's rows are filtered by, as typed; `None`
    /// while nothing filters them.
    pub(crate) fn offer_filter(&self, waiting: &Waiting) -> Option<String> {
        if !self.answer.holds(waiting) {
            return None;
        }
        self.answer
            .terms()
            .map(|_| self.own_reply().trim().to_owned())
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

    /// The offer of `waiting`'s single choice the own reply's words name
    /// ([`rows::named`]): the one `Enter` picks while they are typed. A
    /// settings list's words filter it instead, and a multiple choice's
    /// `Enter` validates its checked answers.
    pub(crate) fn offer_named(&self, waiting: &Waiting) -> Option<usize> {
        let (_, asked, offers) = choice(waiting)?;
        if matches!(
            waiting,
            Waiting::Selection { .. } | Waiting::Knowledge { .. }
        ) || matches!(asked.shape, Shape::Multiple(_))
        {
            return None;
        }
        let approval = matches!(waiting, Waiting::Action { key, .. } if key == GATE);
        named(&self.own_reply(), offers, approval).map(|(index, _)| index)
    }

    /// One key for the own reply's field: a character, a correction, a move
    /// along the line, or a line break (`Alt+Enter`, `Ctrl+J`). `false` when
    /// the key is not an edit (the shell's keys, the offers' keys, `Enter`).
    pub(crate) fn own_key(&mut self, key: KeyEvent) -> bool {
        if (self.answer.scope.as_ref()).is_some_and(|scope| !scope.takes_words()) {
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
        if own.input(Input {
            key: typed,
            ctrl,
            alt,
            shift,
        }) {
            self.answer.refilter();
        }
        true
    }

    /// Pasted text into the own reply's field, as data.
    pub(crate) fn paste_own(&mut self, text: &str) {
        if (self.answer.scope.as_ref()).is_some_and(|scope| !scope.takes_words()) {
            return;
        }
        (self.answer.own).insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
        self.answer.refilter();
    }

    /// Words of an own reply that was not taken back in its field, first,
    /// before whatever it holds now.
    pub(crate) fn put_back_own(&mut self, words: &str) {
        let rest = self.own_reply();
        self.answer.own = self.answer.emptied(self.focused(), self.voice);
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
    #[cfg(test)]
    pub(crate) fn offer_checked(&self, waiting: &Waiting, index: usize) -> bool {
        self.answer.holds(waiting) && self.answer.checked.contains(&index)
    }

    /// The offers checked for this exact logical question, in offer order:
    /// the question is compared once, never once per offer.
    pub(crate) fn offers_checked(&self, waiting: &Waiting) -> Vec<usize> {
        if !self.answer.holds(waiting) {
            return Vec::new();
        }
        let mut checked = self.answer.checked.clone();
        checked.sort_unstable();
        checked
    }

    /// How many currently offered answers are checked.
    #[cfg(test)]
    pub(crate) fn checked_count(&self, waiting: &Waiting) -> usize {
        self.offers_checked(waiting).len()
    }

    /// Check every offered answer that does not stand alone, or none when all those are.
    /// Checking never sends.
    fn check_all(&mut self, offers: &[Offer]) {
        let shared: Vec<usize> = (offers.iter().enumerate())
            .filter(|(_, offer)| !offer.exclusive)
            .map(|(index, _)| index)
            .collect();
        let every = shared
            .iter()
            .all(|index| self.answer.checked.contains(index));
        self.answer.checked = if every { Vec::new() } else { shared };
    }

    /// Toggle one offered answer: one that stands alone clears the others, and another clears
    /// it. Navigation and toggling never send.
    fn toggle_offer(&mut self, index: usize, offers: &[Offer]) {
        if let Some(at) = self.answer.checked.iter().position(|held| *held == index) {
            self.answer.checked.remove(at);
            return;
        }
        let alone = |at: usize| offers.get(at).is_some_and(|offer| offer.exclusive);
        if alone(index) {
            self.answer.checked.clear();
        } else {
            self.answer.checked.retain(|held| !alone(*held));
        }
        self.answer.checked.push(index);
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
        self.answer.follow(waiting, self.voice);
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
        self.answer.own = self.answer.emptied(true, self.voice);
        self.history.push(own.clone());
        self.answer.sent = Some(Sent::Own);
        Some(own)
    }

    /// `Esc` one level at a time: the own reply's words first. With nothing
    /// typed, a settings list sends the line back to the list above or, at its
    /// top, keeps the current choice, whatever row is selected (`at`); a
    /// decision or a question empties the selection, then is never answered by
    /// it: nothing is sent and the hint row says so.
    fn escape(
        &mut self,
        waiting: &Waiting,
        asked: &Asked,
        (at, confirming): (Option<usize>, Option<usize>),
    ) -> Offered {
        // A destructive act waiting for its confirmation is kept, its row still selected.
        if confirming.is_some() {
            return Offered::Notice(KEPT_IT);
        }
        if !self.own_reply().is_empty() {
            self.answer.own = self.answer.emptied(self.focused(), self.voice);
            return Offered::Moved;
        }
        let settings = matches!(waiting, Waiting::Selection { .. });
        if at.is_some() && !settings {
            self.answer.selected = None;
            return Offered::Moved;
        }
        if !settings {
            return Offered::Notice(ESC_ANSWERS_NOTHING);
        }
        let line = match (&asked.back, asked.mandatory) {
            (Some(back), _) => back.clone(),
            (None, false) => KEEP_CURRENT.to_owned(),
            (None, true) => return Offered::Notice(ESC_REQUIRED),
        };
        self.answer.sent = Some(Sent::Offer);
        Offered::Answer(line)
    }

    /// One key for the offers. A multiple choice has a final Submit row;
    /// Space on an option toggles its checkbox, `Ctrl+A` checks every one
    /// (or none, when all are), and Enter on any row sends the whole checked
    /// set. A question that recommends an option has a « Let Nika choose »
    /// row before it: Enter there leaves the choice to Nika ([`Offered::Delegate`]), an act
    /// that names no offer. Under a question the author asked or a decision, with nothing typed
    /// yet, a digit `1`–`9` moves the selection to that offer and nothing else (it never acts);
    /// once words are typed, digits are words.
    pub(crate) fn offer_key(&mut self, waiting: &Waiting, key: KeyEvent) -> Offered {
        let all = key.code == KeyCode::Char('a') && key.modifiers == KeyModifiers::CONTROL;
        if !(key.modifiers.is_empty() || all) || !self.offers_armed(waiting) {
            return Offered::Pass;
        }
        let Some((_, asked, offers)) = choice(waiting) else {
            return Offered::Pass;
        };
        self.answer.follow(waiting, self.voice);
        // A confirmation asked stands for the next key alone: `Enter` again does the act.
        let confirming = self.answer.confirm.take();
        let multiple = matches!(asked.shape, Shape::Multiple(_));
        let at = self.answer.selection(waiting);
        // The keys move among the rows shown, by position; a row keeps its
        // offer's own index ([`rows`]).
        let rows = self.answer.rows(offers, multiple);
        let shown = at.and_then(|at| rows.iter().position(|row| *row == at));
        let last = rows.len().saturating_sub(1);
        let typed = !self.own_reply().is_empty();
        let numbered = self.answer.scope.as_ref().is_some_and(Scope::numbered);
        let next = match key.code {
            KeyCode::Char('a') if all => {
                if !multiple || typed {
                    return Offered::Pass;
                }
                self.check_all(offers);
                return Offered::Moved;
            }
            KeyCode::Char(digit @ '1'..='9') if numbered && !typed => {
                let offer = digit.to_digit(10).and_then(|n| usize::try_from(n).ok());
                let place = (offer.and_then(|n| n.checked_sub(1)))
                    .filter(|at| *at < offers.len())
                    .and_then(|at| rows.iter().position(|row| *row == at));
                let Some(place) = place else {
                    return Offered::Pass;
                };
                place
            }
            KeyCode::Down => shown.map_or(0, |at| at.saturating_add(1).min(last)),
            KeyCode::Up => shown.map_or(last, |at| at.saturating_sub(1)),
            // A page of the inline band at a time, stopping at either end.
            KeyCode::PageDown => shown.unwrap_or(0).saturating_add(PAGE).min(last),
            KeyCode::PageUp => shown.unwrap_or(0).saturating_sub(PAGE),
            KeyCode::Home => 0,
            KeyCode::End => last,
            KeyCode::Tab if multiple => last,
            KeyCode::Esc => return self.escape(waiting, asked, (at, confirming)),
            KeyCode::Char(' ') if multiple && self.own_reply().is_empty() => {
                if let Some(index) = at.filter(|index| *index < offers.len()) {
                    self.toggle_offer(index, offers);
                    return Offered::Moved;
                }
                // Space on Submit or « Let Nika choose » belongs to the
                // control, not an invisible free reply that disables
                // subsequent checkbox toggles.
                return if at.is_some_and(|at| at >= offers.len()) || !asked.free_text {
                    Offered::Moved
                } else {
                    Offered::Pass
                };
            }
            KeyCode::Enter => return self.enter(waiting, (at, confirming), offers, multiple),
            _ => return Offered::Pass,
        };
        self.answer.selected = rows.get(next).copied();
        Offered::Moved
    }

    /// `Enter` on the offers of `waiting`, the row `at` selected (`confirming` the act whose
    /// confirmation was asked, if any).
    fn enter(
        &mut self,
        waiting: &Waiting,
        (at, confirming): (Option<usize>, Option<usize>),
        offers: &[Offer],
        multiple: bool,
    ) -> Offered {
        // « Let Nika choose » (an own reply first): the choice is left to
        // Nika, an act that names no offer, the offers untouched.
        if at == Some(offers.len() + 1) {
            if let Some(own) = self.send_own() {
                return Offered::Answer(own);
            }
            self.answer.sent = Some(Sent::Offer);
            return Offered::Delegate;
        }
        // `Enter` validates the whole checked set from any row (an own
        // reply first); `Space` alone toggles.
        if multiple {
            return match self.submit_choices(waiting) {
                Offered::Moved => Offered::Notice(NOTHING_CHECKED),
                other => other,
            };
        }
        // A settings list applies the row its filter selected; a command
        // line (`/…`) or words matching no row are sent as typed.
        if matches!(waiting, Waiting::Selection { .. }) {
            let command = self.own_reply().trim_start().starts_with('/');
            if let Some(offer) = at.and_then(|at| offers.get(at)).filter(|_| !command) {
                self.answer.sent = Some(Sent::Offer);
                return Offered::Answer(offer.key.clone());
            }
            return self.send_own().map_or(Offered::Pass, Offered::Answer);
        }
        self.enter_one(offers, at, confirming)
    }

    /// `Enter` on a single choice: words naming exactly one offer pick it, as
    /// selecting it would (typing an act's name does that act, and at an
    /// approval gate a yes, a no or a why does its act), except at the optional
    /// offer about the last result, whose rows alone pick ([`Scope::rows_only`]);
    /// then the own reply's words; then the selected offer (`at`), once.
    fn enter_one(
        &mut self,
        offers: &[Offer],
        at: Option<usize>,
        confirming: Option<usize>,
    ) -> Offered {
        let own = self.own_reply();
        let scope = self.answer.scope.as_ref();
        let approval = scope.is_some_and(Scope::approval);
        let by_name = (!scope.is_some_and(Scope::rows_only))
            .then(|| named(&own, offers, approval))
            .flatten();
        if let Some((index, offer)) = by_name {
            self.answer.own = self.answer.emptied(true, self.voice);
            self.history.push(own);
            self.answer.selected = Some(index);
            if let Some(question) = self.confirms(offer, index, confirming) {
                return Offered::Notice(question);
            }
            self.answer.sent = Some(Sent::Offer);
            return Offered::Answer(offer.key.clone());
        }
        // An approval gate never sends other words: a command typed there is
        // sent (the Session answers it beside the gate), nothing else.
        let words = own.trim();
        if approval && !words.is_empty() && !words.starts_with('/') {
            return Offered::Notice(YES_OR_NO);
        }
        if let Some(own) = self.send_own() {
            return Offered::Answer(own);
        }
        let Some((index, offer)) = at.and_then(|at| Some((at, offers.get(at)?))) else {
            return Offered::Pass;
        };
        if let Some(question) = self.confirms(offer, index, confirming) {
            return Offered::Notice(question);
        }
        self.answer.sent = Some(Sent::Offer);
        Offered::Answer(offer.key.clone())
    }

    /// The question a destructive act of a decision asks before it is done
    /// ([`DESTRUCTIVE`]): the first `Enter` on the act at `index` asks it and
    /// arms the confirmation; the next one, while still `confirming` it, does
    /// the act. `None` for any other act, or one confirmed.
    fn confirms(
        &mut self,
        offer: &Offer,
        index: usize,
        confirming: Option<usize>,
    ) -> Option<&'static str> {
        let scope = (self.answer.scope.as_ref()).filter(|scope| scope.kind == ScopeKind::Action)?;
        let (_, _, question) = (DESTRUCTIVE.iter())
            .find(|(decision, act, _)| *decision == scope.key && *act == offer.key)?;
        if confirming == Some(index) {
            return None;
        }
        self.answer.confirm = Some(index);
        Some(question)
    }

    /// A press selects a radio option or toggles a checkbox; it sends nothing.
    pub(crate) fn select_offer(&mut self, waiting: &Waiting, index: usize) -> bool {
        let Some((_, asked, offers)) = choice(waiting) else {
            return false;
        };
        let multiple = matches!(asked.shape, Shape::Multiple(_));
        if !self.offers_open(waiting) {
            return false;
        }
        self.answer.follow(waiting, self.voice);
        if !self.answer.rows(offers, multiple).contains(&index) {
            return false;
        }
        self.answer.confirm = None;
        self.answer.selected = Some(index);
        if multiple && index < offers.len() {
            self.toggle_offer(index, offers);
        }
        true
    }

    /// The human's own words left with `Enter` as the typed question's
    /// answer: in flight until its turn returns.
    pub(crate) fn answer_drafted(&mut self) {
        self.answer.sent = Some(Sent::Draft);
    }

    /// What the last `Enter` on the offers sent, until its turn returns: the selected keys
    /// ([`Sent::Offer`]) or the human's own reply ([`Sent::Own`]), never told apart by bytes.
    pub(crate) fn in_flight(&self) -> Option<Sent> {
        self.answer.sent
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
            Offered::Notice(ESC_ANSWERS_NOTHING),
            "Esc never sends the decision's `no` row"
        );
        assert!(composer.offers_armed(&next), "nothing is in flight");
    }

    /// `Esc` steps back one level and never answers a decision: the own
    /// reply's words first, then the selection, then a notice; the `no`,
    /// `cancel` and discard rows are only ever sent by `Enter` on them.
    #[test]
    fn esc_clears_words_then_the_selection_and_never_answers_a_decision() {
        let save = Waiting::Action {
            key: "save".into(),
            asked: Asked::new(
                "What should happen to this reviewed proposal?",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("yes", "Save workflow"),
                    Offer::new("no", "Discard this proposal"),
                ]),
                "action:3",
                3,
            ),
        };
        let knowledge = Waiting::knowledge(Asked::new(
            "not admitted",
            "",
            false,
            Shape::Choice(vec![
                Offer::new("/knowledge embedded", "use the embedded knowledge"),
                Offer::new("cancel", "drop your message"),
            ]),
            "3:knowledge",
            3,
        ));
        for painted in [save, knowledge, choosing("w1"), action("action:4")] {
            let mut composer = composer(&painted);
            composer.offer_key(&painted, key(KeyCode::End));
            if composer.own_key(key(KeyCode::Char('x'))) {
                assert_eq!(
                    composer.offer_key(&painted, key(KeyCode::Esc)),
                    Offered::Moved
                );
                assert_eq!(composer.own_reply(), "", "{painted:?}: the words first");
                assert!(composer.offer_selected(&painted).is_some(), "{painted:?}");
            }
            assert_eq!(
                composer.offer_key(&painted, key(KeyCode::Esc)),
                Offered::Moved
            );
            assert_eq!(composer.offer_selected(&painted), None, "{painted:?}");
            for _ in 0..3 {
                assert_eq!(
                    composer.offer_key(&painted, key(KeyCode::Esc)),
                    Offered::Notice(ESC_ANSWERS_NOTHING),
                    "{painted:?}"
                );
            }
            assert!(composer.offers_armed(&painted), "{painted:?}: nothing sent");
        }
    }

    /// On a settings list `Esc` closes at once when nothing is typed, a row
    /// selected or not: back to the list above when one is named, else the
    /// current choice kept; typed words go first, alone. A required choice
    /// keeps the list and says why.
    #[test]
    fn esc_on_settings_goes_back_one_level_or_keeps_the_current_choice() {
        let list = |mandatory: bool, back: Option<&str>| {
            let mut asked = Asked::new(
                "Choose a model · deepseek",
                "",
                mandatory,
                Shape::Choice(vec![Offer::new("2 deepseek/flash", "Flash")]),
                "7:intelligence",
                7,
            );
            asked.back = back.map(str::to_owned);
            Waiting::selection(asked)
        };
        for (painted, sent) in [
            (list(false, Some("/intelligence")), "/intelligence"),
            (list(true, Some("/intelligence")), "/intelligence"),
            (list(false, None), KEEP_CURRENT),
        ] {
            for typed in ["", "fla"] {
                let mut composer = composer(&painted);
                composer.offer_key(&painted, key(KeyCode::Down));
                composer.paste_own(typed);
                if !typed.is_empty() {
                    assert_eq!(
                        composer.offer_key(&painted, key(KeyCode::Esc)),
                        Offered::Moved,
                        "the typed words first, alone"
                    );
                    assert_eq!(composer.own_reply(), "");
                    assert_eq!(composer.offer_selected(&painted), Some(0), "kept");
                    assert!(composer.offers_armed(&painted), "nothing sent yet");
                }
                assert_eq!(
                    composer.offer_key(&painted, key(KeyCode::Esc)),
                    Offered::Answer(sent.to_owned()),
                    "{sent}: one press closes, the selection is no choice"
                );
                assert!(!composer.offers_open(&painted), "{sent} is in flight");
            }
        }
        let required = list(true, None);
        let mut composer = composer(&required);
        composer.offer_key(&required, key(KeyCode::Down));
        assert_eq!(
            composer.offer_key(&required, key(KeyCode::Esc)),
            Offered::Notice(ESC_REQUIRED)
        );
        assert!(composer.offers_armed(&required));
    }

    /// Told the person writes French, the own field says what typing does in
    /// French from the next question it follows on: a reply, a settings list's
    /// filter, an approval's two words; English stays the default.
    #[test]
    fn the_own_field_says_what_typing_does_in_the_persons_language() {
        use crate::lang::{Lang, Voice};
        let french = Voice {
            lang: Lang::Fr,
            ascii: false,
        };
        let painted = choosing("w1");
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.speak(french);
        composer.follow(&painted);
        assert_eq!(
            composer.answer.own.placeholder_text(),
            "ou tapez votre propre réponse"
        );
        let settings = Waiting::selection(Asked::new(
            "Choose a model",
            "",
            false,
            Shape::Choice(vec![Offer::new("2 p/m", "Model")]),
            "8:intelligence",
            8,
        ));
        composer.follow(&settings);
        assert_eq!(composer.answer.own.placeholder_text(), "tapez pour filtrer");
        // The field on screen follows at once, its words kept.
        composer.follow(&painted);
        composer.paste_own("euro");
        composer.speak(Voice {
            ascii: true,
            ..french
        });
        assert_eq!(
            composer.answer.own.placeholder_text(),
            "ou tapez votre propre reponse"
        );
        assert_eq!(composer.own_reply(), "euro");
        let english = self::composer(&choosing("w3"));
        assert_eq!(
            english.answer.own.placeholder_text(),
            "or type your own reply"
        );
    }

    /// Emptied by `Esc` or by `Enter`, the own field keeps the words that say
    /// what typing does under that question.
    #[test]
    fn an_emptied_field_keeps_the_placeholder_of_its_question() {
        let settings = Waiting::selection(Asked::new(
            "Choose a model",
            "",
            false,
            Shape::Choice(vec![Offer::new("2 p/m", "Model")]),
            "8:intelligence",
            8,
        ));
        let mut composer = composer(&settings);
        composer.paste_own("mo");
        assert_eq!(
            composer.offer_key(&settings, key(KeyCode::Esc)),
            Offered::Moved
        );
        assert_eq!(
            composer.answer.own.placeholder_text(),
            "type to filter",
            "the filter's words stay after Esc"
        );
        let painted = choosing("w1");
        let mut composer = self::composer(&painted);
        composer.paste_own("euro");
        composer.offer_key(&painted, key(KeyCode::Enter));
        assert_eq!(
            composer.answer.own.placeholder_text(),
            "or type your own reply"
        );
    }

    /// A question opens on its first row, so `Down` reaches the second; `Esc`
    /// empties the selection, and then `Enter` sends nothing and the first
    /// `Down` selects the first row again.
    #[test]
    fn a_question_opens_on_its_first_row_and_the_arrows_select_within_the_offers() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
        assert_eq!(composer.offer_selected(&painted), Some(0), "opens focused");
        assert!(composer.offers_armed(&painted));
        let mut press = |code| composer.offer_key(&painted, key(code));
        assert_eq!(press(KeyCode::Down), Offered::Moved);
        assert_eq!(composer.offer_selected(&painted), Some(1), "Down: row 2");
        let mut press = |code| composer.offer_key(&painted, key(code));
        assert_eq!(press(KeyCode::Esc), Offered::Moved);
        assert_eq!(composer.offer_selected(&painted), None, "Esc empties it");
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
        assert_eq!(fresh.offer_key(&painted, key(KeyCode::Esc)), Offered::Moved);
        assert_eq!(fresh.offer_key(&painted, key(KeyCode::Up)), Offered::Moved);
        assert_eq!(
            fresh.offer_selected(&painted),
            Some(2),
            "from nothing, Up starts at the end"
        );
        // Not the offers' keys: they take their ordinary path. A digit past
        // the offers is a word.
        for code in [KeyCode::Tab, KeyCode::Char('4'), KeyCode::Char('0')] {
            assert_eq!(
                fresh.offer_key(&painted, key(code)),
                Offered::Pass,
                "{code:?}"
            );
        }
        // A digit with nothing typed moves the selection to that offer, and
        // sends nothing.
        assert_eq!(
            fresh.offer_key(&painted, key(KeyCode::Char('1'))),
            Offered::Moved
        );
        assert_eq!(fresh.offer_selected(&painted), Some(0));
        assert!(fresh.offers_armed(&painted), "nothing sent");
        let shifted = KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT);
        assert_eq!(fresh.offer_key(&painted, shifted), Offered::Pass);
        assert!(fresh.text().is_empty(), "the offers never write the draft");
    }

    /// `PgDn` and `PgUp` move the selection a page of the inline band at a
    /// time and stop at either end; a multiple choice's Submit row is the
    /// last stop. Paging selects only: nothing is sent.
    #[test]
    fn page_keys_move_the_selection_a_band_page_and_stop_at_the_ends() {
        let many: Vec<Offer> = (0..30)
            .map(|i| Offer::new(format!("2 p/m-{i}"), format!("Model {i}")))
            .collect();
        let single = Waiting::selection(Asked::new(
            "Choose a model · p",
            "",
            false,
            Shape::Choice(many.clone()),
            "7:intelligence",
            7,
        ));
        let mut composer = composer(&single);
        for (code, at) in [
            (KeyCode::PageDown, 8),
            (KeyCode::PageDown, 16),
            (KeyCode::PageDown, 24),
            (KeyCode::PageDown, 29),
            (KeyCode::PageDown, 29),
            (KeyCode::PageUp, 21),
            (KeyCode::Home, 0),
            (KeyCode::PageUp, 0),
        ] {
            assert_eq!(composer.offer_key(&single, key(code)), Offered::Moved);
            assert_eq!(composer.offer_selected(&single), Some(at), "{code:?}");
        }
        assert!(composer.offers_armed(&single), "paging sends nothing");
        let multiple = document("sources", Shape::Multiple(many));
        let mut composer = self::composer(&multiple);
        for _ in 0..4 {
            composer.offer_key(&multiple, key(KeyCode::PageDown));
        }
        assert_eq!(composer.offer_selected(&multiple), Some(30), "Submit");
        assert_eq!(
            composer.checked_count(&multiple),
            0,
            "paging checks nothing"
        );
    }

    /// `Enter` answers with the selected offer's exact key, once: the offers
    /// are inert until that answer's turn returns, and a key taken is spent.
    /// The question asked again opens afresh, on its first row.
    #[test]
    fn enter_answers_with_the_exact_key_once_and_a_taken_key_is_spent() {
        let painted = choosing("w1");
        let mut composer = composer(&painted);
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
        // The same question shown again opens afresh, armed, on its first row.
        composer.follow(&choosing("w2"));
        assert_eq!(composer.offer_selected(&choosing("w2")), Some(0));
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
    /// no longer waits starts afresh, on its first row.
    #[test]
    fn a_selection_survives_a_new_identity_of_its_question_and_nothing_else() {
        let painted = choosing("w1");
        let selected = |waiting: &Waiting| {
            let mut composer = composer(&painted);
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
            assert_eq!(selected(&between), Some(0), "{between:?}");
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
            composer.offer_key(&painted, key(KeyCode::Esc)),
            Offered::Moved
        );
        assert_eq!(
            composer.offer_key(&painted, key(KeyCode::Enter)),
            Offered::Pass,
            "nothing to send"
        );
        composer.offer_key(&painted, key(KeyCode::Down));
        for c in "both currencies".chars() {
            assert!(composer.own_key(key(KeyCode::Char(c))));
        }
        assert!(composer.own_key(key(KeyCode::Backspace)));
        composer.paste_own("s");
        assert_eq!(composer.own_reply(), "both currencies");
        assert!(!composer.own_key(key(KeyCode::Enter)), "Enter is no edit");
        let stop = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!composer.own_key(stop), "Ctrl+C keeps its place");
        let sent = composer.offer_key(&painted, key(KeyCode::Enter));
        assert_eq!(sent, Offered::Answer("both currencies".to_owned()));
        assert!(composer.own_reply().is_empty(), "the field is taken");
        assert!(!composer.offers_open(&painted), "in flight");
        assert_eq!(composer.text(), "my draft", "the draft never changed");
        assert_eq!(composer.answer_returned(false), Some(Sent::Own));
        // Kept in history, as any line `Enter` sends.
        composer.clear();
        assert_eq!(composer.handle(key(KeyCode::Up)), ComposerAction::Edited);
        assert_eq!(composer.text(), "both currencies");
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

#[cfg(test)]
mod recommended_tests;

#[cfg(test)]
mod cost_tests;
