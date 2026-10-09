// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The full words: the Session's own words of a block shown short of whole
//! (the typed question waiting at the answer line, the current proposal the
//! cards review, a refusal summarized), read over the frame and never
//! edited.
//!
//! The transcript keeps the Session's block untouched; a summary only
//! changes how a card shows it. `F2` (or the palette) opens this view on
//! the block at the reading position ([`read_at`]), a press on such a block
//! opens it on that block; the arrows and the page keys scroll it, and `Esc`,
//! `Enter` or `F2` closes it with the composer, its draft, the keyboard focus
//! and the reading position exactly as they were. It sends nothing and grants
//! nothing; any other key closes it and takes its ordinary path.

use crossterm::event::{KeyCode, KeyEvent};
use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::symbols::border;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::catalog::DIAGNOSTIC_KEY;
use crate::composer::Composer;
use crate::model::{Committed, Kind, Presentation, UiState};
use crate::render::ASCII_BOX;
use crate::visual::role;
use crate::workspace::cards::diagnostics::{Shown, shown};
use crate::workspace::cards::{review, shown_at};
use crate::workspace::desk::Desk;

/// The rows one `PgUp` or `PgDn` moves.
const PAGE: usize = 10;

/// Whether the conversation's cards show `block` short of whole (a recognized
/// refusal, a shortened banner): its full words are then this view's to show.
/// Every other block, a provider failure included, is painted as said.
pub(crate) fn summarized(block: &Committed) -> bool {
    !matches!(shown(block), Shown::Said)
}

/// The latest block of `transcript` the conversation shows summarized.
#[cfg(test)]
pub(crate) fn latest(
    transcript: &[Committed],
    summarized: impl Fn(&Committed) -> bool,
) -> Option<&Committed> {
    transcript.iter().rev().find(|block| summarized(block))
}

/// The latest block of `transcript` whose full words this view shows, by
/// index: one shown short of whole, or the current proposal the cards review,
/// at `proposal`.
pub(crate) fn latest_shown(transcript: &[Committed], proposal: Option<usize>) -> Option<usize> {
    (0..transcript.len())
        .rev()
        .find(|at| proposal == Some(*at) || summarized(&transcript[*at]))
}

/// The block this view opens at the conversation's reading position, by
/// index: the newest the workspace shows on the row of `point`, or on any of
/// its rows (`None`), named by the plan painting reads ([`shown_at`]), so a
/// key and a press reach one block. It is shown short of whole
/// ([`summarized`]), or it is the current proposal the cards review or the
/// question the live card carries; no other block opens.
pub(crate) fn read_at(
    state: &UiState,
    desk: &Desk,
    composer: &Composer,
    point: Option<Position>,
) -> Option<usize> {
    (desk.geometry(state.size)).filter(|_| state.presentation == Presentation::Workspace)?;
    let (area, carried) = crate::scroll::regions(state, desk, composer);
    let row = |point: Position| area.contains(point).then(|| usize::from(point.y - area.y));
    let rows = match point {
        Some(point) => row(point).map(|row| row..row + 1)?,
        None => 0..usize::from(area.height),
    };
    let review = desk.review(state.ascii);
    let current = review::summarized(state, review.as_ref()).map(|(at, _)| at);
    let shown = shown_at(state, area, (review.as_ref(), carried), rows);
    shown.into_iter().find(|at| {
        [current, carried].contains(&Some(*at)) || state.transcript.get(*at).is_some_and(summarized)
    })
}

/// What the view did with a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Viewed {
    /// It scrolled (or stayed at an end): nothing else changed.
    Read,
    /// `Esc`, `Enter` or `F2`: the view closes, everything else as it was.
    Closed,
    /// Not its key: the view closes and the key takes its ordinary path.
    Passed,
}

/// The view, open over the frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    /// What the words are, named in the frame's title.
    what: &'static str,
    /// The block's words, exactly as the Session said them.
    words: String,
    /// The first row shown.
    scroll: usize,
    /// The last first row that still fills the view, at the last frame size.
    last: usize,
}

impl Diagnostic {
    /// The view of `block`'s own words, from their top: the whole proposal
    /// for a proposal's preview, the whole question for a question's words,
    /// the full diagnostic for anything else.
    pub(crate) fn of(block: &Committed) -> Self {
        let what = match block.kind {
            Kind::Proposal => "Full proposal",
            Kind::Question => "Full question",
            _ => "Full diagnostic",
        };
        Self {
            what,
            words: block.text.clone(),
            scroll: 0,
            last: usize::MAX,
        }
    }

    /// The words shown.
    #[cfg(test)]
    pub(crate) fn words(&self) -> &str {
        &self.words
    }

    /// Read one key.
    pub(crate) fn key(&mut self, key: KeyEvent) -> Viewed {
        if !key.modifiers.is_empty() {
            return Viewed::Passed;
        }
        self.scroll = match key.code {
            KeyCode::Esc | KeyCode::Enter => return Viewed::Closed,
            code if code == DIAGNOSTIC_KEY => return Viewed::Closed,
            KeyCode::Up => self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll.saturating_sub(PAGE),
            KeyCode::PageDown => self.scroll.saturating_add(PAGE),
            KeyCode::Home => 0,
            KeyCode::End => usize::MAX,
            _ => return Viewed::Passed,
        }
        .min(self.last);
        Viewed::Read
    }

    /// The rows inside the frame of `area`.
    fn inner(area: Rect) -> Rect {
        Block::bordered().inner(area)
    }

    /// Fit the scroll to the rows the words take on a frame of `size`, before
    /// drawing: the last row never scrolls above the view's bottom.
    pub(crate) fn fit(&mut self, size: (u16, u16)) {
        let inner = Self::inner(Rect::new(0, 0, size.0, size.1));
        let lines: Vec<Line<'_>> = self.words.lines().map(Line::raw).collect();
        let rows = crate::render::content_rows(&lines, inner.width.max(1));
        self.last = rows.saturating_sub(usize::from(inner.height));
        self.scroll = self.scroll.min(self.last);
    }

    /// Paint the view over `area`: a frame titled with what it is and how to
    /// leave, the words wrapped inside, scrolled.
    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect, ascii: bool, color: bool) {
        let (sep, set) = if ascii {
            (" - ", ASCII_BOX)
        } else {
            (" · ", border::PLAIN)
        };
        let title = format!(" {}{sep}the Session's words, read only ", self.what);
        let keys = if ascii {
            " Up/Down PgUp/PgDn scroll - Esc/Enter returns "
        } else {
            " ↑↓ PgUp/PgDn scroll · Esc/Enter returns "
        };
        let block = Block::bordered()
            .border_set(set)
            .border_style(role::style(Role::Dim, color))
            .title(Line::styled(title, role::style(Role::Strong, color)))
            .title_bottom(Line::styled(keys, role::style(Role::Dim, color)));
        let scroll = u16::try_from(self.scroll).unwrap_or(u16::MAX);
        let words: Vec<Line<'_>> = self.words.lines().map(Line::raw).collect();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(words)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0)),
            area,
        );
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crossterm::event::KeyModifiers;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::model::Kind;
    use unicode_width::UnicodeWidthStr;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// The conversation's rows of the frame the shell paints for `state`.
    fn conversation(state: &UiState, desk: &Desk, composer: &Composer) -> Vec<String> {
        let (width, height) = state.size;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let paint = crate::workspace::object::Paint {
            ascii: state.ascii,
            color: false,
            elapsed: std::time::Duration::ZERO,
            reduced_motion: true,
        };
        terminal
            .draw(|frame| {
                assert!(crate::workspace::desk::draw(
                    frame, desk, paint, state, composer
                ));
            })
            .expect("draw");
        let (area, _) = crate::scroll::regions(state, desk, composer);
        let buffer = terminal.backend().buffer();
        (area.top()..area.bottom())
            .map(|y| {
                (area.left()..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// The cell where `words` are first painted among the `rows` of the
    /// conversation in `area`.
    fn found(area: Rect, rows: &[String], words: &str) -> Option<Position> {
        rows.iter().enumerate().find_map(|(y, row)| {
            let x = u16::try_from(row[..row.find(words)?].width()).ok()?;
            Some(Position::new(area.x + x, area.y + u16::try_from(y).ok()?))
        })
    }

    /// `F2` at the reading position and a press on a painted row name one
    /// block through the plan painting reads: at the latest rows `new`;
    /// scrolled back, `old` (never `new` below it), a press on its words
    /// `seen` the same block, on the human's line nothing. Blocks said after
    /// it keep the reading position, its row and that block.
    fn named_at_the_reading_position(old: &Committed, seen: &str, new: &Committed) {
        for size in [(120, 40), (80, 24)] {
            let mut state = UiState::new(Presentation::Workspace, false, size);
            let mut desk = Desk::new();
            desk.view = Some(crate::model::demo_project());
            let composer = Composer::new();
            let human = Committed::new(Kind::Human, "digest my notes");
            state.transcript.extend([human, old.clone()]);
            for n in 0..40 {
                let reply = Committed::new(Kind::Reply, format!("reply {n}"));
                state.transcript.push(reply);
            }
            state.transcript.push(new.clone());
            let newest = state.transcript.len() - 1;
            assert_eq!(read_at(&state, &desk, &composer, None), Some(newest));
            crate::scroll::rows(&mut state, &desk, &composer, true, usize::MAX);
            assert_eq!(read_at(&state, &desk, &composer, None), Some(1), "{size:?}");
            let rows = conversation(&state, &desk, &composer);
            let (area, _) = crate::scroll::regions(&state, &desk, &composer);
            let at = found(area, &rows, seen).expect("the old block in view");
            assert_eq!(read_at(&state, &desk, &composer, Some(at)), Some(1));
            let line = found(area, &rows, "digest my notes").expect("the line in view");
            assert_eq!(read_at(&state, &desk, &composer, Some(line)), None);
            crate::scroll::preserve_reading(&mut state, &desk, &composer, |state| {
                let again = Committed::new(Kind::Human, "again");
                state.transcript.extend([again, new.clone()]);
            });
            assert_eq!(conversation(&state, &desk, &composer), rows, "{size:?}");
            assert_eq!(read_at(&state, &desk, &composer, None), Some(1));
            assert_eq!(read_at(&state, &desk, &composer, Some(at)), Some(1));
        }
    }

    #[test]
    fn the_reading_position_and_a_press_name_one_block() {
        let refusal = Committed::new(Kind::Refusal, untrusted_refusal());
        named_at_the_reading_position(&refusal, "Could not continue", &refusal);
    }

    /// A press on a summarized refusal's card names that refusal, wherever on
    /// the card; a press on the human's line names nothing; inline and the
    /// focus view, which paint every block as said, name nothing.
    #[test]
    fn a_press_on_a_summary_card_names_its_block() {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        let (desk, composer) = (Desk::new(), Composer::new());
        state
            .transcript
            .push(Committed::new(Kind::Human, "digest my notes"));
        state
            .transcript
            .push(Committed::new(Kind::Refusal, untrusted_refusal()));
        let rows = conversation(&state, &desk, &composer);
        let (area, _) = crate::scroll::regions(&state, &desk, &composer);
        let title = found(area, &rows, "Could not continue").expect("its card");
        let human = found(area, &rows, "digest my notes").expect("the line");
        assert_eq!(read_at(&state, &desk, &composer, Some(title)), Some(1));
        assert_eq!(read_at(&state, &desk, &composer, Some(human)), None);
        assert_eq!(read_at(&state, &desk, &composer, None), Some(1));
        for presentation in [Presentation::Inline, Presentation::Focus] {
            state.presentation = presentation;
            assert_eq!(read_at(&state, &desk, &composer, Some(title)), None);
            assert_eq!(read_at(&state, &desk, &composer, None), None);
        }
    }

    #[test]
    fn the_latest_summarized_block_is_found_and_kept_word_for_word() {
        let said = Committed::new(Kind::Reply, "plain words");
        let first = Committed::new(Kind::Refusal, "ADMISSION_UNTRUSTED first");
        let last = Committed::new(Kind::Refusal, "ADMISSION_UNTRUSTED last\n  detail");
        let transcript = [first, said.clone(), last.clone(), said];
        let summarized = |block: &Committed| block.text.starts_with("ADMISSION_UNTRUSTED");
        assert_eq!(latest(&transcript, summarized), Some(&last));
        assert_eq!(latest(&transcript, |_| false), None);
        assert_eq!(Diagnostic::of(&last).words(), last.text);
    }

    #[test]
    fn the_keys_scroll_close_or_pass_and_never_leave_the_words() {
        let text = (0..40).map(|n| format!("raw line {n}")).collect::<Vec<_>>();
        let block = Committed::new(Kind::Refusal, text.join("\n"));
        let mut view = Diagnostic::of(&block);
        view.fit((60, 12));
        // 40 rows inside a 10-row frame: the last top row is 30.
        assert_eq!(view.key(key(KeyCode::End)), Viewed::Read);
        assert_eq!(view.scroll, 30);
        assert_eq!(view.key(key(KeyCode::Down)), Viewed::Read);
        assert_eq!(view.scroll, 30, "never past the bottom");
        assert_eq!(view.key(key(KeyCode::PageUp)), Viewed::Read);
        assert_eq!(view.scroll, 20);
        assert_eq!(view.key(key(KeyCode::Home)), Viewed::Read);
        assert_eq!(view.scroll, 0);
        assert_eq!(view.key(key(KeyCode::Esc)), Viewed::Closed);
        assert_eq!(view.key(key(KeyCode::Enter)), Viewed::Closed);
        assert_eq!(view.key(key(DIAGNOSTIC_KEY)), Viewed::Closed);
        for other in [
            key(KeyCode::Char('y')),
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
        ] {
            assert_eq!(view.key(other), Viewed::Passed, "{other:?}");
        }
        assert_eq!(view.words(), block.text);
    }

    /// What a refused seated turn adds to the configuration refusal, as the
    /// Session's source writes it (the presenter's own tests pin it there).
    const NOT_SENT: &str = " · this workflow-authoring request was not sent, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again";

    /// The Session's own refusal of a turn whose environment names a release
    /// with no trusted identity: the parser and the strict door produce the
    /// cause, the Session's error type its sentence.
    fn untrusted_refusal() -> String {
        use nika_cli_host::compile::config::AuthoringSettings;
        use nika_session::authoring::{AuthoringContext, AuthoringError};
        let mut env = AuthoringSettings::none();
        env.knowledge = Some(std::path::PathBuf::from("/srv/foundry/release-r3"));
        let context = AuthoringContext::from_settings(&AuthoringSettings::none(), &env);
        let cause = context.refusal().cloned();
        let cause = cause.expect("a release the environment names carries no trusted identity");
        format!("{}{NOT_SENT}", AuthoringError::Context(cause))
    }

    /// A provider failure in the Session's recovery card: it keeps its words.
    fn provider_failure() -> String {
        let reason = nika_session::authoring::AuthoringError::Seat(
            "deepseek answered 503 Service Unavailable".to_owned(),
        );
        nika_display::front_door::recovery::card(
            "I couldn't use the authoring seat for this part",
            &reason.to_string(),
            Some("seat: deepseek/deepseek-chat"),
            &["your request: « digest notes.md »".to_owned()],
            "No workflow output was written or Run requested; the selected model may have received this turn's context: a failed call can still have been sent.",
        )
    }

    /// Against the composed presenter: the view is offered for the refusal
    /// the cards summarize and for nothing else (a provider failure keeps its
    /// words on its card), and it shows the Session's raw words, the admission
    /// code and the stated scope included, word for word.
    #[test]
    fn the_view_is_offered_for_the_recognized_refusal_alone_with_its_raw_words() {
        let refusal = Committed::new(Kind::Refusal, untrusted_refusal());
        let failure = Committed::new(Kind::Refusal, provider_failure());
        let reply = Committed::new(Kind::Reply, "a reply");
        assert!(summarized(&refusal), "{}", refusal.text);
        assert!(!summarized(&failure), "{}", failure.text);
        let transcript = [reply.clone(), refusal.clone(), failure.clone()];
        assert_eq!(latest(&transcript, summarized), Some(&refusal));
        assert_eq!(latest(&[reply, failure], summarized), None);
        let view = Diagnostic::of(&refusal);
        assert_eq!(view.words(), refusal.text);
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).expect("terminal");
        terminal
            .draw(|frame| view.render(frame, frame.area(), false, false))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let shown: String = (0..12)
            .map(|y| (1..99).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ");
        let shown = shown.split_whitespace().collect::<Vec<_>>().join(" ");
        for words in [
            "ADMISSION_UNTRUSTED: no trusted expected identity",
            "this workflow-authoring request was not sent, nothing was written",
        ] {
            assert!(shown.contains(words), "{words}: {shown}");
        }
    }

    /// The reader opens the latest summarized block: the current proposal at
    /// its index, or a recognized refusal after it, never another proposal;
    /// it names the whole proposal and the whole question truthfully and
    /// holds their exact bytes, odd spacing and line ends included, while a
    /// refusal keeps its title.
    #[test]
    fn the_current_proposal_opens_whole_under_its_own_title() {
        use crate::workspace::cards::review::fixture;
        let preview = format!("{}\r\n  trailing spaces  \n\n", fixture::PREVIEW);
        let proposal = Committed::proposal(fixture::id(), preview.clone());
        let older = Committed::new(Kind::Proposal, "an older preview");
        let refusal = Committed::new(Kind::Refusal, untrusted_refusal());
        let reply = Committed::new(Kind::Reply, "a reply");
        let transcript = [older.clone(), proposal.clone(), reply.clone()];
        assert_eq!(latest_shown(&transcript, Some(1)), Some(1));
        assert_eq!(latest_shown(&transcript, None), None);
        assert_eq!(latest_shown(&transcript, Some(9)), None, "no such block");
        let after = [proposal.clone(), refusal.clone()];
        assert_eq!(latest_shown(&after, Some(0)), Some(1));
        let view = Diagnostic::of(&proposal);
        assert_eq!(view.words().as_bytes(), preview.as_bytes());
        let words = "the currency code\r\n  (The compiler cannot invent this.)  \n";
        let question = Committed::question("3:q-1", words);
        assert_eq!(
            Diagnostic::of(&question).words().as_bytes(),
            words.as_bytes()
        );
        for (block, title) in [
            (&proposal, "Full proposal"),
            (&question, "Full question"),
            (&refusal, "Full diagnostic"),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(70, 8)).expect("terminal");
            terminal
                .draw(|frame| Diagnostic::of(block).render(frame, frame.area(), false, false))
                .expect("draw");
            let top: String = (0..70)
                .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
                .collect();
            assert!(top.contains(title), "{top}");
            assert!(top.contains("the Session's words, read only"), "{top}");
        }
        assert_eq!(older.text, "an older preview");
    }

    #[test]
    fn the_view_paints_the_words_inside_a_titled_frame_in_both_glyph_columns() {
        let block = Committed::new(
            Kind::Refusal,
            "NIKA-E042 untrusted release\nnothing was sent",
        );
        let view = Diagnostic::of(&block);
        for ascii in [false, true] {
            let mut terminal = Terminal::new(TestBackend::new(60, 8)).expect("terminal");
            terminal
                .draw(|frame| view.render(frame, frame.area(), ascii, false))
                .expect("draw");
            let buffer = terminal.backend().buffer();
            let rows: Vec<String> = (0..8)
                .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect())
                .collect();
            assert!(rows[0].contains("Full diagnostic"), "{rows:#?}");
            assert!(rows[1].contains("NIKA-E042 untrusted release"), "{rows:#?}");
            assert!(rows[2].contains("nothing was sent"), "{rows:#?}");
            assert!(rows[7].contains("Esc/Enter returns"), "{rows:#?}");
            if ascii {
                assert!(rows.iter().all(|row| row.is_ascii()), "{rows:#?}");
            }
        }
    }
}
