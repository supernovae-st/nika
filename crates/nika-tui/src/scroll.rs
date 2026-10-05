// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Full-screen transcript scrolling in rendered rows, including long cards.

use crate::composer::Composer;
use crate::model::{Presentation, UiState};
use crate::render::{block_lines, live_rows, wrapped_rows};
use crate::workspace::{cards, desk::Desk, focus::Region, geometry::Geometry, screen};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};

fn area(state: &UiState, desk: &Desk, composer: &Composer) -> Rect {
    let area = Rect::new(0, 0, state.size.0, state.size.1);
    if state.presentation == Presentation::Workspace
        && let Some(geometry) = Geometry::of(area, desk.pins())
    {
        return screen::panel_areas(&geometry, state, composer)[1];
    }
    let live = live_rows(state, composer, area.width, area.height);
    let [transcript, _, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(live),
    ])
    .areas(area);
    transcript
}

/// Move a page, with one shared row, clamped to the actually rendered content.
pub(crate) fn page(state: &mut UiState, desk: &Desk, composer: &Composer, older: bool) {
    let viewport = area(state, desk, composer);
    rows(
        state,
        desk,
        composer,
        older,
        usize::from(viewport.height.saturating_sub(1)).max(1),
    );
}

/// Move a bounded number of rendered rows without changing keyboard focus.
pub(crate) fn rows(
    state: &mut UiState,
    desk: &Desk,
    composer: &Composer,
    older: bool,
    step: usize,
) {
    let maximum = maximum(state, desk, composer);
    let current = state.focus_scroll.min(maximum);
    state.focus_scroll = if older {
        current.saturating_add(step).min(maximum)
    } else {
        current.saturating_sub(step)
    };
}

fn maximum(state: &UiState, desk: &Desk, composer: &Composer) -> usize {
    let area = area(state, desk, composer);
    let rows = if state.presentation == Presentation::Workspace {
        cards::height(state, area)
    } else {
        let lines: Vec<_> = state
            .transcript
            .iter()
            .flat_map(|block| {
                block_lines(block, state.color, state.ascii)
                    .into_iter()
                    .chain([ratatui::text::Line::default()])
            })
            .collect();
        usize::from(wrapped_rows(&lines, area.width))
    };
    rows.saturating_sub(usize::from(area.height))
}

/// Keep the current reading position when observed activity or a turn adds
/// content. The latest-row view still follows new content automatically.
pub(crate) fn preserve_reading(
    state: &mut UiState,
    desk: &Desk,
    composer: &Composer,
    update: impl FnOnce(&mut UiState),
) {
    let before = (state.focus_scroll > 0).then(|| maximum(state, desk, composer));
    update(state);
    if let Some(before) = before {
        let after = maximum(state, desk, composer);
        state.focus_scroll = if after >= before {
            state.focus_scroll.saturating_add(after - before)
        } else {
            state.focus_scroll.saturating_sub(before - after)
        }
        .min(after);
    }
}

/// End returns a scrolled transcript to its latest row; otherwise the composer
/// or focused object keeps its normal End key.
pub(crate) fn end(state: &mut UiState, desk: &Desk, key: KeyEvent) -> bool {
    let conversation = state.presentation == Presentation::Focus
        || (state.presentation == Presentation::Workspace
            && (!crate::workspace::geometry::fits(state.size)
                || desk.focus.region == Region::Conversation));
    if conversation
        && state.focus_scroll > 0
        && key.code == KeyCode::End
        && key.modifiers == KeyModifiers::NONE
    {
        state.focus_scroll = 0;
        state.interrupt_armed = false;
        state.completion = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::{Committed, Kind};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn observed_activity_and_final_blocks_keep_a_scrolled_reading_position() {
        for presentation in [Presentation::Workspace, Presentation::Focus] {
            let mut state = UiState::new(presentation, false, (120, 40));
            state.transcript.push(Committed::new(
                Kind::Reply,
                (0..100)
                    .map(|n| format!("history {n:03}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
            let desk = Desk::new();
            let composer = Composer::new();
            rows(&mut state, &desk, &composer, true, 30);
            let top = maximum(&state, &desk, &composer) - state.focus_scroll;
            for n in 0..20 {
                preserve_reading(&mut state, &desk, &composer, |state| {
                    state.observe_activity(&format!("observed step {n}"));
                });
                assert_eq!(maximum(&state, &desk, &composer) - state.focus_scroll, top);
            }
            preserve_reading(&mut state, &desk, &composer, |state| {
                state.apply(crate::model::Beat::Say(Committed::new(
                    Kind::Reply,
                    "the actual result",
                )));
            });
            assert_eq!(maximum(&state, &desk, &composer) - state.focus_scroll, top);
            state.focus_scroll = 0;
            preserve_reading(&mut state, &desk, &composer, |state| {
                state.observe_activity("a new update");
            });
            assert_eq!(state.focus_scroll, 0);
        }
    }

    #[test]
    fn pages_reach_the_beginning_middle_and_end_of_one_long_question() {
        for presentation in [Presentation::Workspace, Presentation::Focus] {
            let mut state = UiState::new(presentation, false, (120, 40));
            state.transcript.push(Committed::new(
                Kind::Question,
                (0..150)
                    .map(|n| format!("question line {n:03}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
            let desk = Desk::new();
            let composer = Composer::new();
            let viewport = area(&state, &desk, &composer);
            let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
            let shown = |terminal: &Terminal<TestBackend>| {
                let b = terminal.backend().buffer();
                (viewport.y..viewport.bottom())
                    .map(|y| {
                        (viewport.x..viewport.right())
                            .map(|x| b[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let mut saw_middle = false;
            loop {
                terminal
                    .draw(|frame| crate::render::render_transcript(frame, &state, viewport))
                    .expect("draw");
                let text = shown(&terminal);
                saw_middle |= text.contains("question line 075");
                if text.contains("question line 000") {
                    break;
                }
                let before = state.focus_scroll;
                page(&mut state, &desk, &composer, true);
                assert!(state.focus_scroll > before, "cannot reach the first line");
            }
            assert!(saw_middle);
            assert!(end(
                &mut state,
                &desk,
                KeyEvent::new(KeyCode::End, KeyModifiers::NONE)
            ));
            terminal
                .draw(|frame| crate::render::render_transcript(frame, &state, viewport))
                .expect("draw latest");
            assert!(shown(&terminal).contains("question line 149"));
            assert!(
                !end(
                    &mut state,
                    &desk,
                    KeyEvent::new(KeyCode::End, KeyModifiers::NONE)
                ),
                "End is now the composer's key"
            );
        }
    }
}
