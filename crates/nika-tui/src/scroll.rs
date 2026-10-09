// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Full-screen transcript scrolling in rendered rows, including long cards.

use crate::composer::Composer;
use crate::model::{Presentation, UiState};
use crate::render::{block_lines, content_rows, live_rows, question};
use crate::workspace::{cards, desk::Desk, focus::Region, screen};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};

fn area(state: &UiState, desk: &Desk, composer: &Composer) -> Rect {
    regions(state, desk, composer).0
}

/// The transcript's rows on the frame of `state`, and the block the live
/// question card carries there (the workspace panel's [`question::carried`]):
/// the regions painting reads, from the same live area.
fn regions(state: &UiState, desk: &Desk, composer: &Composer) -> (Rect, Option<usize>) {
    let area = Rect::new(0, 0, state.size.0, state.size.1);
    if state.presentation == Presentation::Workspace
        && let Some(geometry) = desk.geometry(state.size)
    {
        let shown = desk.screen(state.ascii);
        let [_, transcript, _, live] = screen::panel_areas(&geometry, state, composer, &shown);
        let carried = question::carried(state, composer, live, screen::boxed(&geometry));
        return (transcript, carried);
    }
    let live = live_rows(state, composer, area.width, area.height);
    let [transcript, _, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(live),
    ])
    .areas(area);
    (transcript, None)
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
    let (area, carried) = regions(state, desk, composer);
    let rows = if state.presentation == Presentation::Workspace {
        // The review and the carried question the workspace paints with; the
        // focus view standing in below the minimum paints every block as said.
        let review = (desk.geometry(state.size)).and_then(|_| desk.review(state.ascii));
        cards::height(state, area, (review.as_ref(), carried))
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
        content_rows(&lines, area.width)
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
    keep(state, desk, composer, before);
}

/// [`preserve_reading`] when the update also edits the draft, whose rows are
/// the live area's: the transcript above it keeps the reading position.
pub(crate) fn preserve_reading_and_draft<T>(
    state: &mut UiState,
    desk: &Desk,
    composer: &mut Composer,
    update: impl FnOnce(&mut UiState, &mut Composer) -> T,
) -> T {
    let before = (state.focus_scroll > 0).then(|| maximum(state, desk, composer));
    let out = update(state, composer);
    keep(state, desk, composer, before);
    out
}

/// Move a scrolled position by what the content below it gained or lost.
fn keep(state: &mut UiState, desk: &Desk, composer: &Composer, before: Option<usize>) {
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
    use crate::workspace::geometry::{Arrangement, Geometry, Layout as WorkspaceLayout};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn transcript_bounds_follow_the_actual_arrangement_after_a_resize() {
        let mut desk = Desk::new();
        let composer = Composer::new();
        for size in [(80, 24), (120, 40), (180, 48)] {
            let mut state = UiState::new(Presentation::Workspace, false, size);
            state
                .transcript
                .push(Committed::new(Kind::Reply, "history\n".repeat(120)));
            for layout in [WorkspaceLayout::Session, WorkspaceLayout::Workbench] {
                desk.arrange(
                    Arrangement::of(layout)
                        .with_conversation_width(Some(650))
                        .with_conversation_height(Some(250)),
                );
                let geometry = desk.geometry(size).expect("workspace");
                let shown = desk.screen(state.ascii);
                let expected = screen::panel_areas(&geometry, &state, &composer, &shown)[1];
                assert_eq!(area(&state, &desk, &composer), expected);
                state.focus_scroll = 0;
                page(&mut state, &desk, &composer, true);
                assert_eq!(
                    state.focus_scroll,
                    usize::from(expected.height.saturating_sub(1)).max(1)
                );
            }
        }
    }

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
    fn a_draft_and_a_notice_set_at_a_turn_end_keep_a_scrolled_reading_position() {
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
            let mut composer = Composer::new();
            rows(&mut state, &desk, &composer, true, 30);
            let top = maximum(&state, &desk, &composer) - state.focus_scroll;
            preserve_reading_and_draft(&mut state, &desk, &mut composer, |state, composer| {
                composer.paste("a queued correction\non three\nlines");
                state
                    .transcript
                    .push(Committed::new(Kind::Notice, "it is in the box"));
            });
            assert_eq!(maximum(&state, &desk, &composer) - state.focus_scroll, top);
        }
    }

    #[test]
    fn content_past_u16_rows_keeps_both_ends_reachable_in_both_presentations() {
        let text = (0..70_000)
            .map(|n| format!("line {n:05}"))
            .collect::<Vec<_>>()
            .join("\n");
        for presentation in [Presentation::Workspace, Presentation::Focus] {
            let mut state = UiState::new(presentation, false, (120, 40));
            state.transcript.push(Committed::new(Kind::Reply, &text));
            let desk = Desk::new();
            let composer = Composer::new();
            let viewport = area(&state, &desk, &composer);
            let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
            let paint = |terminal: &mut Terminal<TestBackend>, state: &UiState| {
                terminal
                    .draw(|frame| crate::render::render_transcript(frame, state, viewport))
                    .expect("window draws");
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
            assert!(paint(&mut terminal, &state).contains("line 69999"));
            rows(&mut state, &desk, &composer, true, usize::MAX);
            assert!(state.focus_scroll > usize::from(u16::MAX));
            assert!(paint(&mut terminal, &state).contains("line 00000"));
            rows(&mut state, &desk, &composer, false, 35_000);
            assert!(paint(&mut terminal, &state).contains("line 35001"));
            assert!(end(
                &mut state,
                &desk,
                KeyEvent::new(KeyCode::End, KeyModifiers::NONE)
            ));
            assert!(paint(&mut terminal, &state).contains("line 69999"));
            assert_eq!(state.transcript[0].text, text);
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
    /// Full workspace rendering and the scroll owner consume the SAME selected model.
    /// The wheel's row primitive is tested here; actual SGR routing stays in `mouse_pty`.
    fn selected_frame(
        terminal: &mut Terminal<TestBackend>,
        state: &UiState,
        desk: &Desk,
        composer: &Composer,
    ) -> (String, String) {
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
            .expect("workspace frame");
        let buffer = terminal.backend().buffer();
        let text = |rect: Rect| {
            (rect.y..rect.bottom())
                .map(|y| {
                    (rect.x..rect.right())
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        (
            text(area(state, desk, composer)),
            text(Rect::new(0, 0, state.size.0, state.size.1)),
        )
    }

    const MODEL: &str = "claude-code/claude-fable-5-1[1m]";
    const DRAFT: &str = "draft stays unsent";

    fn selected_workspace(size: (u16, u16), ascii: bool) -> (UiState, Desk, Composer) {
        use crate::model::Waiting;
        use crate::workspace::{
            pinned::Pinned,
            project::{ProjectView, Target, WorkflowView},
        };
        let mut state = UiState::new(Presentation::Workspace, false, size);
        state.ascii = ascii;
        state.waiting = Waiting::Question {
            key: "required_input".into(),
        };
        state.transcript.push(Committed::new(
            Kind::Reply,
            (0..10)
                .map(|n| format!("earlier line {n:03}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
        state.transcript.push(Committed::new(
            Kind::Question,
            (0..60)
                .map(|n| format!("question line {n:03}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
        let mut desk = Desk::new();
        desk.view = Some(
            ProjectView::new("local", "studio", "./studio")
                .listing(
                    vec![WorkflowView::new(
                        "release.nika",
                        Some("release"),
                        true,
                        0,
                        1,
                    )],
                    true,
                )
                .seated(format!("{MODEL} - selected for preparation"))
                .pinning(Pinned::new(
                    "studio",
                    "release.nika",
                    "#043",
                    nika_display::state::TaskState::Paused,
                    "waiting for approval",
                )),
        );
        desk.opened = Some(Target::Workflow("release.nika".into()));
        let mut composer = Composer::new();
        composer.paste(DRAFT);
        (state, desk, composer)
    }

    #[test]
    fn selected_header_and_preview_share_scroll_geometry_and_keep_the_draft() {
        use crate::model::Waiting;
        use crate::workspace::desk::Route;
        for size in [(60, 16), (80, 24), (120, 40)] {
            for ascii in [false, true] {
                let (mut state, mut desk, composer) = selected_workspace(size, ascii);
                let geometry =
                    Geometry::of(Rect::new(0, 0, size.0, size.1), desk.pins()).expect("geometry");
                let view = desk.screen(ascii);
                let measured = screen::panel_areas(&geometry, &state, &composer, &view);
                assert_eq!(area(&state, &desk, &composer), measured[1]);
                assert!(measured[1].height >= 1);
                let mut unseated = view.clone();
                unseated.thread = view.thread.clone().seated(None);
                assert_eq!(
                    measured,
                    screen::panel_areas(&geometry, &state, &composer, &unseated),
                    "the selection, said in the header, never moves the conversation's rows"
                );
                let mut terminal =
                    Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
                let (latest, full) = selected_frame(&mut terminal, &state, &desk, &composer);
                for visible in ["release.nika", DRAFT] {
                    assert!(full.contains(visible), "{size:?} {visible}: {full}");
                }
                // The header is the selection's home: whole where its row has
                // room, else cut beside `/status`, which says it whole.
                let whole = format!("Prepare with: {MODEL}");
                let cut = size.0 < 100 && full.contains("/status");
                assert!(full.contains(&whole) || cut, "{size:?}: {full}");
                assert!(latest.contains("question line 059"), "{size:?}: {latest}");
                let mut saw_middle = latest.contains("question line 030");
                for _ in 0..100 {
                    assert_eq!(
                        desk.route(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), size),
                        Route::Older
                    );
                    page(&mut state, &desk, &composer, true);
                    let (shown, _) = selected_frame(&mut terminal, &state, &desk, &composer);
                    saw_middle |= shown.contains("question line 030");
                    if shown.contains("question line 000") {
                        break;
                    }
                }
                let (first, _) = selected_frame(&mut terminal, &state, &desk, &composer);
                assert!(first.contains("question line 000"), "{size:?}: {first}");
                assert!(saw_middle, "PageUp skipped the middle at {size:?}");
                let top = maximum(&state, &desk, &composer) - state.focus_scroll;
                preserve_reading(&mut state, &desk, &composer, |state| {
                    state.busy = Some("checking the fixture locally".into());
                    state.observe_activity("checked fixture source");
                });
                assert_eq!(maximum(&state, &desk, &composer) - state.focus_scroll, top);
                let (held, full) = selected_frame(&mut terminal, &state, &desk, &composer);
                assert!(held.contains("question line 000"), "{size:?}: {held}");
                assert!(full.contains(DRAFT));
                assert!(end(
                    &mut state,
                    &desk,
                    KeyEvent::new(KeyCode::End, KeyModifiers::NONE)
                ));
                let (at_end, _) = selected_frame(&mut terminal, &state, &desk, &composer);
                assert!(
                    at_end.contains("checked fixture source"),
                    "{size:?}: {at_end}"
                );
                let focus = desk.focus.region;
                rows(&mut state, &desk, &composer, true, 3);
                let (wheel_up, _) = selected_frame(&mut terminal, &state, &desk, &composer);
                assert_ne!(wheel_up, at_end, "wheel rows did not move at {size:?}");
                rows(&mut state, &desk, &composer, false, 3);
                assert_eq!(
                    selected_frame(&mut terminal, &state, &desk, &composer).0,
                    at_end
                );
                assert_eq!(desk.focus.region, focus);
                assert_eq!(composer.text(), DRAFT);
                assert!(matches!(state.waiting, Waiting::Question { .. }));
                assert!(!end(
                    &mut state,
                    &desk,
                    KeyEvent::new(KeyCode::End, KeyModifiers::NONE)
                ));
            }
        }
    }

    /// The scroll bounds read the review the workspace paints with: while the
    /// current proposal reads as its review, the measure counts the reviewed
    /// rows, never the Session's longer words; below the minimum the focus
    /// view standing in paints and measures every block as said.
    #[test]
    fn the_scroll_bounds_read_the_review_the_workspace_paints() {
        use crate::model::Waiting;
        use crate::workspace::cards::review::fixture;
        use nika_session::ProposalId;
        let long = format!("{}\n{}", fixture::PREVIEW, "cost and policy\n".repeat(40));
        let id = ProposalId::of(&long);
        let mut desk = Desk::new();
        desk.proposed(Some(fixture::candidate(id.clone(), false)));
        let composer = Composer::new();
        for size in [(80, 24), (120, 40), (180, 48), (50, 14)] {
            let proposal = Committed::proposal(id.clone(), long.clone());
            let state = fixture::state(size, proposal, Waiting::Proposal);
            let viewport = area(&state, &desk, &composer);
            let said = cards::height(&state, viewport, (None, None));
            let review = (desk.geometry(size)).and_then(|_| desk.review(state.ascii));
            let measured = cards::height(&state, viewport, (review.as_ref(), None));
            assert_eq!(review.is_some(), size != (50, 14), "{size:?}");
            let shorter = review.is_none() || measured < said;
            assert!(shorter, "{size:?}: {measured} {said}");
            let most = measured.saturating_sub(usize::from(viewport.height));
            assert_eq!(maximum(&state, &desk, &composer), most, "{size:?}");
        }
    }
}
