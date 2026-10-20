// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::super::catalog::{View, offered};
use super::*;
use crate::model::{Asked, Offer, Presentation};

fn view(presentation: Presentation) -> View {
    View {
        presentation,
        fits: true,
        scrolled: false,
        read: None,
        object: None,
    }
}

fn keys_of(state: &UiState, can_stop: bool) -> String {
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(&state.waiting);
    words(
        state,
        &composer,
        &offered(&[], view(state.presentation), state.voice()),
        can_stop,
    )
}

/// At the free prompt the conversation's keys lead, then the keys that
/// always hold and the view keys the palette offers, from its catalog.
#[test]
fn the_conversation_lists_its_keys_then_the_ones_that_always_hold() {
    let state = UiState::new(Presentation::Inline, false, (80, 24));
    let said = keys_of(&state, false);
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines[0], "Conversation");
    for line in [
        "  Alt+Enter · a new line",
        "Always",
        "  Ctrl+C · stop or interrupt; twice leaves",
        "  Ctrl+O · every command and view key",
        "  Ctrl+T · Full screen",
        "  ? · The keys of this screen",
    ] {
        assert!(lines.contains(&line), "missing {line:?}: {said}");
    }
    let hint = format!("  {}", state.waiting.hint());
    assert_eq!(lines[1], hint, "its hint row's own words first: {said}");
}

/// Each surface names its own keys: the intelligence list its filter, a
/// question its digits, a multiple choice its boxes and « Let Nika choose »
/// where the author recommends, a decision that Esc never answers, and the
/// work under way what Ctrl+C does there.
#[test]
fn every_surface_names_its_own_keys() {
    let offers = vec![
        Offer::new("a", "First"),
        Offer::new("b", "Second").recommended(),
    ];
    let asked = |shape| Asked::new("Which?", "", false, shape, "1:q", 1);
    let cases = [
        (
            Waiting::selection(asked(Shape::Choice(offers.clone()))),
            "Intelligence list",
            "type · filter the rows",
        ),
        (
            Waiting::asked("q", asked(Shape::Multiple(offers.clone()))),
            "Question",
            "Ctrl+A · check every answer, or none when all are",
        ),
        (
            Waiting::asked("q", asked(Shape::Choice(offers))),
            "Question",
            "Let Nika choose · leave the choice to Nika, in words",
        ),
        (
            Waiting::Proposal,
            "Decision",
            "Esc · never answers a decision",
        ),
        (
            Waiting::action("save", asked(Shape::Choice(Vec::new()))),
            "Decision",
            "1-9 · select that act, while nothing is typed; Enter does it",
        ),
    ];
    for (waiting, name, key) in cases {
        let mut state = UiState::new(Presentation::Focus, false, (100, 30));
        state.waiting = waiting;
        let said = keys_of(&state, false);
        assert!(said.starts_with(name), "{name}: {said}");
        assert!(said.contains(key), "{name} misses {key}: {said}");
    }
    let mut state = UiState::new(Presentation::Focus, false, (100, 30));
    state.busy = Some("● authoring".to_owned());
    let preparing = keys_of(&state, true);
    assert!(preparing.starts_with("While Nika works"), "{preparing}");
    assert!(preparing.contains("Ctrl+C · stop the preparation; again leaves"));
    let running = keys_of(&state, false);
    assert!(running.contains("Ctrl+C · a Run is never stopped here; twice leaves"));
}

/// The keys overlay reads in the person's language: its headings, its hint
/// row's words and every key's effect in French, the key names as a French
/// keyboard writes them (`Entrée`, `Échap`); its reader frame says what it is
/// and how to leave in French too. English keeps every word.
#[test]
fn the_keys_overlay_reads_in_the_persons_language() {
    use super::super::diagnostic::Diagnostic;
    use crate::lang::{Lang, Voice};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut state = UiState::new(Presentation::Inline, false, (80, 24));
    state.lang = Lang::Fr;
    let said = keys_of(&state, false);
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines[0], "Conversation", "{said}");
    assert_eq!(
        lines[1],
        "  décrivez un travail · /help · lancer : run <fichier>.nika"
    );
    for line in [
        "  Entrée · envoyer ce que vous avez écrit",
        "  Alt+Entrée · une nouvelle ligne",
        "Toujours",
        "  Ctrl+C · arrêter ou interrompre ; deux fois quitte",
        "  Ctrl+T · Plein écran",
        "  ? · Les touches de cet écran",
    ] {
        assert!(lines.contains(&line), "missing {line:?}: {said}");
    }
    for english in ["Always", "send what you wrote", "Full screen"] {
        assert!(!said.contains(english), "{english}: {said}");
    }
    state.waiting = Waiting::Proposal;
    assert!(keys_of(&state, false).starts_with("Décision"));
    state.busy = Some("● authoring".to_owned());
    let preparing = keys_of(&state, true);
    assert!(
        preparing.starts_with("Pendant que Nika travaille"),
        "{preparing}"
    );
    assert!(preparing.contains("Ctrl+C · arrêter la préparation ; encore quitte"));
    let frame = |voice: Voice| {
        let mut terminal = Terminal::new(TestBackend::new(80, 6)).expect("terminal");
        let view = Diagnostic::keys("Conversation".to_owned());
        terminal
            .draw(|frame| view.render(frame, frame.area(), voice, false))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let row = |y: u16| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>();
        (row(0), row(5))
    };
    let (top, bottom) = frame(state.voice());
    assert!(
        top.contains(" Touches · pour ce qui est à l'écran "),
        "{top}"
    );
    assert!(
        bottom.contains(" ↑↓ PgUp/PgDn défiler · Échap/Entrée revient "),
        "{bottom}"
    );
    let (top, bottom) = frame(Voice::default());
    assert!(top.contains(" Keys · for what is on screen now "), "{top}");
    assert!(
        bottom.contains(" ↑↓ PgUp/PgDn scroll · Esc/Enter returns "),
        "{bottom}"
    );
    let plain = Voice {
        ascii: true,
        ..state.voice()
    };
    let (top, bottom) = frame(plain);
    assert!(
        top.contains(" Touches - pour ce qui est a l'ecran "),
        "{top}"
    );
    assert!(
        bottom.contains(" Up/Down PgUp/PgDn defiler - Echap/Entree revient "),
        "{bottom}"
    );
}
