// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::app::commands::diagnostic::Viewed;
use crate::composer::Composer;
use crate::model::{Asked, Committed, Handoff, Offer, Presentation, UiState};
use crate::workspace::cards::review::fixture;
use crate::workspace::desk::Desk;
use crate::workspace::object::Paint;

fn waiting(key: &str, token: &str, witness: &str) -> Waiting {
    Waiting::Action {
        key: key.to_owned(),
        asked: Asked::new(
            "A decision still waits",
            "",
            true,
            Shape::Choice(vec![Offer::new(token, "Read the details")]),
            witness,
            1,
        ),
    }
}

fn said(text: &str, witness: &str) -> Said {
    Said::Answer {
        text: text.to_owned(),
        witness: witness.to_owned(),
    }
}

fn turn(key: &str, token: &str, witness: &str, block: Committed) -> Turn {
    Turn {
        beats: vec![Beat::Say(block), Beat::Wait(waiting(key, token, witness))],
        handoff: None,
    }
}

#[test]
fn each_native_read_opens_only_the_exact_words_returned_by_that_act() {
    for (key, token, kind) in [
        ("save", "/show", Kind::Proposal),
        ("run_cost", "details", Kind::Question),
        ("unknown_cost", "details", Kind::Question),
        ("gate", "why?", Kind::Reply),
    ] {
        let candidate = fixture::candidate(fixture::id(), false);
        let request = Request::capture(
            &said(token, "action:old"),
            &waiting(key, token, "action:old"),
            Some(&candidate),
        )
        .expect("listed native read");
        let words = "Exact owner evidence\nUSD 0.07; bound is unchanged\n  spaces stay  ";
        let result = turn(key, token, "action:new", Committed::new(kind, words));
        let view = request
            .completed(&result, Some(&candidate))
            .expect("reader");
        assert_eq!(view.words(), words);
    }
}

#[test]
fn ordinary_lines_stale_tokens_and_unlisted_or_effectful_answers_do_not_open_a_reader() {
    let candidate = fixture::candidate(fixture::id(), false);
    let shown = waiting("save", "/show", "action:current");
    for answer in [
        Said::Line("/show".to_owned()),
        said("/show", "action:old"),
        said("yes", "action:current"),
        said("save & run", "action:current"),
    ] {
        assert!(Request::capture(&answer, &shown, Some(&candidate)).is_none());
    }
    let duplicate = match shown.clone() {
        Waiting::Action { key, mut asked } => {
            asked.shape = Shape::Choice(vec![
                Offer::new("/show", "Inspect"),
                Offer::new("/show", "Duplicate"),
            ]);
            Waiting::Action { key, asked }
        }
        other => panic!("expected the action fixture, got {other:?}"),
    };
    assert!(
        Request::capture(
            &said("/show", "action:current"),
            &duplicate,
            Some(&candidate)
        )
        .is_none()
    );
    assert!(Request::capture(&said("/show", "action:current"), &shown, None).is_none());
    let aside = fixture::candidate(fixture::id(), true);
    assert!(Request::capture(&said("/show", "action:current"), &shown, Some(&aside)).is_none());
    // A workflow's choice may literally say "why?"; it is never a read-only act.
    assert!(
        Request::capture(
            &said("why?", "action:current"),
            &waiting("gate_choice", "why?", "action:current"),
            None,
        )
        .is_none()
    );
}

#[test]
fn a_failed_read_changed_proposal_or_nonrenewed_menu_never_opens_old_details() {
    let candidate = fixture::candidate(fixture::id(), false);
    let request = Request::capture(
        &said("/show", "action:old"),
        &waiting("save", "/show", "action:old"),
        Some(&candidate),
    )
    .expect("read");
    let block = Committed::new(Kind::Proposal, "exact held preview");
    let result = turn("save", "/show", "action:new", block.clone());
    let replacement = fixture::candidate(ProposalId::of("replacement"), false);
    let aside = fixture::candidate(fixture::id(), true);
    for current in [None, Some(&replacement), Some(&aside)] {
        assert!(request.completed(&result, current).is_none());
    }
    for extra in [
        Beat::NotTaken("/show".to_owned()),
        Beat::Cancelled("stopped".to_owned()),
        Beat::Quit,
        Beat::Say(Committed::new(Kind::Refusal, "not taken")),
        Beat::Wait(Waiting::Free),
        Beat::Wait(waiting("save", "/show", "action:old")),
        Beat::Wait(waiting("run_cost", "details", "action:new")),
    ] {
        let mut rejected = turn("save", "/show", "action:new", block.clone());
        rejected.beats.push(extra);
        assert!(request.completed(&rejected, Some(&candidate)).is_none());
    }
    let mut handoff = turn("save", "/show", "action:new", block);
    handoff.handoff = Some(Handoff {
        id: 1,
        label: "running".to_owned(),
    });
    assert!(request.completed(&handoff, Some(&candidate)).is_none());
    let wrong = turn(
        "save",
        "/show",
        "action:new",
        Committed::new(Kind::Reply, "old preview elsewhere"),
    );
    assert!(request.completed(&wrong, Some(&candidate)).is_none());
    let foreign = turn(
        "save",
        "/show",
        "action:new",
        Committed::proposal(ProposalId::of("foreign"), "foreign proposal"),
    );
    assert!(request.completed(&foreign, Some(&candidate)).is_none());
}

fn frame(state: &UiState, composer: &Composer, detail: Option<&mut Diagnostic>) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            crate::app::draw_frame(
                frame,
                state,
                composer,
                &Desk::new(),
                Paint {
                    ascii: state.ascii,
                    color: false,
                    elapsed: std::time::Duration::ZERO,
                    reduced_motion: true,
                },
                detail,
            );
        })
        .expect("draw");
    let cells = terminal.backend().buffer();
    (0..24)
        .map(|y| (0..80).map(|x| cells[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn details_cover_the_menu_at_eighty_columns_and_return_without_picking_or_losing_the_draft() {
    let candidate = fixture::candidate(fixture::id(), false);
    let request = Request::capture(
        &said("/show", "action:old"),
        &waiting("save", "/show", "action:old"),
        Some(&candidate),
    )
    .expect("read");
    let result = turn(
        "save",
        "/show",
        "action:new",
        Committed::new(
            Kind::Proposal,
            "The complete inspected changes\nCeiling USD 0.07\nExact file replacement: report.nika",
        ),
    );
    let mut reader = request
        .completed(&result, Some(&candidate))
        .expect("reader");
    let mut state = UiState::new(Presentation::Workspace, false, (80, 24));
    state.waiting = waiting("save", "/show", "action:new");
    let mut composer = Composer::new();
    composer.paste("keep this draft");
    composer.follow(&state.waiting);
    assert_eq!(composer.text(), "keep this draft");
    let before = frame(&state, &composer, None);
    assert!(before.contains("A decision still waits"), "{before}");
    let shown = frame(&state, &composer, Some(&mut reader));
    assert!(shown.contains("Full proposal"), "{shown}");
    assert!(shown.contains("The complete inspected changes"), "{shown}");
    assert!(shown.contains("Ceiling USD 0.07"), "{shown}");
    assert!(
        !shown.contains("A decision still waits"),
        "reader covers the menu"
    );
    for code in [KeyCode::Esc, KeyCode::Enter, KeyCode::F(2)] {
        assert_eq!(
            reader.key(KeyEvent::new(code, KeyModifiers::NONE)),
            Viewed::Closed
        );
        assert_eq!(composer.offer_selected(&state.waiting), None);
        assert_eq!(composer.text(), "keep this draft");
        assert_eq!(frame(&state, &composer, None), before);
    }
}
