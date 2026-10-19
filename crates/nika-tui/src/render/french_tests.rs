// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The TUI's own words in the person's language: every form of every waiting
//! state's hint has its French, and the French gives way on a narrow row as
//! the English does, a whole cue at a time; a band's title and a question's
//! header name what waits in French, the Session's words untouched.

#![allow(clippy::expect_used)]

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::lang::Lang;
use crate::model::{Asked, Offer, Shape};

/// The rows `state` paints, its band's first row apart when one is open.
fn painted(state: &UiState) -> (Vec<String>, Option<usize>) {
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(&state.waiting);
    let (width, height) = state.size;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| match state.presentation {
            Presentation::Inline => draw_inline(frame, state, &composer),
            _ => draw_focus(frame, state, &composer),
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let rows = (0..height)
        .map(|y| {
            let row: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            row.trim_end().to_owned()
        })
        .collect();
    let area = Rect::new(0, 0, width, height);
    let [transcript, _, live] = focus_areas(area, state, &composer);
    let band = (state.presentation == Presentation::Focus)
        .then(|| band(state, &composer, transcript.y, live, Panel::default()))
        .flatten()
        .map(|(band, _)| usize::from(band.y));
    (rows, band)
}

/// A band names what waits and its facts in French, a question's card names
/// the shape of its answer in French, and the Session's words above keep
/// theirs; under ASCII both lose their accents, and English keeps English.
#[test]
fn a_band_title_and_a_question_header_name_what_waits_in_french() {
    let choice = || Shape::Choice(vec![Offer::new("eur", "Euro"), Offer::new("usd", "Dollar")]);
    for (waiting, english, french, plain) in [
        (
            Waiting::asked("const.currency", asked(choice())),
            "Question · required",
            "Question · obligatoire",
            "Question - obligatoire",
        ),
        (
            Waiting::action("save", asked(choice())),
            "Decision · required",
            "Décision · obligatoire",
            "Decision - obligatoire",
        ),
    ] {
        for (lang, ascii, title) in [
            (Lang::En, false, english),
            (Lang::Fr, false, french),
            (Lang::Fr, true, plain),
        ] {
            let mut state = UiState::new(Presentation::Focus, false, (80, 24));
            state
                .transcript
                .push(Committed::new(Kind::Reply, "Quelle devise ?"));
            state.waiting = waiting.clone();
            (state.lang, state.ascii) = (lang, ascii);
            let (rows, band) = painted(&state);
            let band = band.expect("the band");
            assert_eq!(
                rows[band], title,
                "{waiting:?} {lang:?} ascii={ascii}\n{rows:#?}"
            );
            assert!(!ascii || rows.iter().all(|row| row.is_ascii()), "{rows:#?}");
        }
    }
    for (lang, header) in [
        (Lang::En, "A text answer · required"),
        (Lang::Fr, "Une réponse en texte · obligatoire"),
    ] {
        let mut state = UiState::new(Presentation::Inline, false, (80, 12));
        let words = "Which currency do the amounts use?";
        state.transcript.push(Committed::new(Kind::Question, words));
        state.waiting = Waiting::asked("const.currency", asked(Shape::Text));
        state.lang = lang;
        let (rows, _) = painted(&state);
        assert!(rows.iter().any(|row| row == header), "{lang:?}: {rows:#?}");
    }
}

fn asked(shape: Shape) -> Asked {
    Asked::new("Which?", "", true, shape, "3:q", 3)
}

/// One state of each kind the hint row names: the free prompt, the
/// intelligence list (with a way back, required, or keeping a choice), a
/// decision (an act, a gate, words beside it), the typed questions (offered
/// answers, words, several answers, a proposal's), the knowledge decision, a
/// spending question, a question in prose, the consent and the gate.
fn every_waiting() -> Vec<Waiting> {
    let choice = || Shape::Choice(vec![Offer::new("eur", "Euro")]);
    let several = Shape::Multiple(vec![Offer::new("a", "A"), Offer::new("b", "B")]);
    let mut back = asked(choice());
    back.back = Some("/intelligence".to_owned());
    let mut keeps = asked(choice());
    keeps.mandatory = false;
    let mut free = asked(choice());
    free.free_text = true;
    vec![
        Waiting::Free,
        Waiting::Choosing,
        Waiting::selection(back),
        Waiting::selection(asked(choice())),
        Waiting::selection(keeps),
        Waiting::action("save", asked(choice())),
        Waiting::action("gate", asked(choice())),
        Waiting::action("project.missed", free.clone()),
        Waiting::action("save", free),
        Waiting::asked("const.currency", asked(choice())),
        Waiting::asked("const.notes", asked(Shape::Text)),
        Waiting::asked("const.tags", asked(several.clone())),
        Waiting::proposal_choice("p1", "const.currency", asked(choice())),
        Waiting::proposal_choice("p1", "const.tags", asked(several)),
        Waiting::proposal_choice("p1", "const.notes", asked(Shape::Text)),
        Waiting::knowledge(asked(choice())),
        Waiting::Question {
            key: "unknown_cost".into(),
        },
        Waiting::Question {
            key: "const.notes".into(),
        },
        Waiting::Proposal,
        Waiting::Gate,
    ]
}

/// Every form a hint row can take has its French: the whole hint of each
/// waiting state, each narrower form, every consent word of a reviewed
/// proposal that `save & run` admits; English stays where the person writes
/// English.
#[test]
fn every_hint_form_has_its_french() {
    let french = Voice {
        lang: Lang::Fr,
        ascii: false,
    };
    for waiting in every_waiting() {
        let forms = std::iter::once(waiting.hint()).chain(narrower(&waiting).iter().copied());
        for form in forms {
            assert_ne!(french.say(form), form, "{waiting:?}: {form:?}");
            assert_eq!(Voice::default().say(form), form);
        }
    }
    for form in SAVE_RUN_HINTS
        .into_iter()
        .chain(STANDING)
        .chain([PROPOSAL_STATUS])
    {
        assert_ne!(french.say(form), form, "{form:?}");
    }
}

/// The French hint gives way as the English does: on each row it takes the
/// longest French form that holds (a whole cue shorter each time, never a
/// key parted from what it does), one row from 44 cells on, and under ASCII
/// every row is ASCII.
#[test]
fn the_french_hint_gives_way_a_whole_cue_at_a_time() {
    let composer = Composer::new();
    for waiting in every_waiting() {
        for ascii in [false, true] {
            let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
            state.waiting = waiting.clone();
            state.ascii = ascii;
            state.lang = Lang::Fr;
            let voice = state.voice();
            for width in [44_u16, 60, 92] {
                let hint = hint_lines(&state, &composer, width, (None, true));
                let text: Vec<String> = hint.iter().map(ToString::to_string).collect();
                let text = text.join("\n");
                let at = format!("{waiting:?} {width} ascii={ascii}: {text}");
                if waiting == Waiting::Free {
                    // The workspace's own free row, its narrowest form one row.
                    assert!(text.contains("Ctrl+O : commandes"), "{at}");
                    continue;
                }
                assert_eq!(hint.len(), 1, "{at}");
                assert_eq!(usize::from(wrapped_rows(&hint, width)), 1, "{at}");
                // A card that covers the conversation adds, last, the key that reads behind it,
                // where the row holds the form and that key both.
                let cue = own(&format!(" · {}", voice.say(surface::PEEK_CUE)), ascii);
                let form = text.strip_suffix(&cue).unwrap_or(&text).to_owned();
                let covers = surface::covers(&state);
                assert!(
                    covers || text == form,
                    "only a covering card names it: {at}"
                );
                let forms = std::iter::once(waiting.hint()).chain(narrower(&waiting).to_vec());
                let said: Vec<String> = forms.map(|form| own(voice.say(form), ascii)).collect();
                assert!(said.contains(&form), "{at}\n{said:#?}");
                let first = said.iter().find(|form| fits_row(form, ascii, width));
                assert_eq!(
                    first,
                    Some(&form),
                    "the longest French form that holds: {at}"
                );
                let both = format!("{form}{cue}");
                assert!(
                    !covers || text == both || !fits_row(&both, ascii, width),
                    "{at}"
                );
                assert!(!ascii || text.is_ascii(), "{at}");
            }
        }
    }
}

/// The stage line paints the Session's rail as it came, its stages named in the
/// person's language by the Session (« Brouillon ✓ … », `Lang::rail`) and an
/// earlier run's mark as the footer said it, then what the session spent and
/// where its keys are read, in that language; a narrower row gives way as the
/// English does, its newest stages kept. The renderer names no stage itself: an
/// English rail stays English, and under ASCII the Session's words keep theirs.
#[test]
fn the_stage_line_paints_the_sessions_stages_in_french() {
    let mut facts = nika_session::LifecycleFacts::new();
    (facts.saved, facts.check_clean) = (true, Some(true));
    let lifecycle = nika_session::Lifecycle::from_facts(&facts);
    let mut state = UiState::new(Presentation::Focus, false, (120, 40));
    state.rail = format!("{} (précédent ✓)", Lang::Fr.rail(&lifecycle));
    state.spent = Some("$0.31".to_owned());
    state.lang = Lang::Fr;
    assert_eq!(
        rail_line(&state, 140).to_string(),
        "Brouillon ✓ · Enregistré ✓ · Vérifié ✓ · Actif ○ · Run ○ (précédent ✓) · ~$0.31 dépensés depuis l'ouverture · ? pour les raccourcis"
    );
    let narrow = rail_line(&state, 48).to_string();
    assert!(
        unicode_width::UnicodeWidthStr::width(narrow.as_str()) <= 48,
        "{narrow}"
    );
    assert_eq!(narrow, "… Vérifié ✓ · Actif ○ · Run ○ (précédent ✓)");
    state.rail = format!("{} (earlier ✓)", lifecycle.rail());
    let english = rail_line(&state, 120).to_string();
    assert!(
        english.starts_with("Draft ✓ · Saved ✓ · Checked ✓ · Active ○ · Run ○ (earlier ✓) · "),
        "the renderer names no stage: {english}"
    );
    state.ascii = true;
    state.rail = Lang::Fr.rail(&lifecycle);
    let plain = rail_line(&state, 120).to_string();
    assert!(
        plain.starts_with("Brouillon ✓ · Enregistré ✓ · Vérifié ✓ · Actif ○ · Run ○ - "),
        "{plain}"
    );
}

/// The prompt beside the line names what waits in the person's language
/// (« Enregistrer ? › » at a proposal, « réponse › » at a question), measured
/// as painted: the line keeps the cells the prompt leaves. `action ›` and
/// `nika ›` read the same in both languages; English keeps English.
#[test]
fn the_prompt_names_what_waits_in_french() {
    for (waiting, english, french) in [
        (Waiting::Proposal, "Save? › ", "Enregistrer ? › "),
        (
            Waiting::Question {
                key: "const.notes".into(),
            },
            "reply › ",
            "réponse › ",
        ),
        (Waiting::Gate, "answer › ", "réponse › "),
        (Waiting::Free, "nika › ", "nika › "),
    ] {
        let composer = Composer::new();
        let mut state = UiState::new(Presentation::Inline, false, (80, 12));
        state.waiting = waiting.clone();
        assert_eq!(prompt_of(&state, &composer), english);
        let wide = editor_width(&state, 80);
        state.lang = Lang::Fr;
        assert_eq!(prompt_of(&state, &composer), french);
        let cells = u16::try_from(french.chars().count()).expect("cells");
        assert_eq!(editor_width(&state, 80), 80 - cells, "{waiting:?}");
        assert!(wide >= editor_width(&state, 80));
        let (rows, _) = painted(&state);
        let prompt = french.trim_end();
        assert!(
            rows.iter().any(|row| row.starts_with(prompt)),
            "{prompt}: {rows:#?}"
        );
    }
}

/// The activity card a turn keeps in the workspace says its own words in the
/// person's language: its heading, how the turn ended and in what time, the
/// author calls it counts and what it leaves out, each phase's word and the
/// call row; the Session's own words, the model and the times stay as they are,
/// and under ASCII the French loses its accents.
#[test]
fn the_activity_card_says_its_own_words_in_french() {
    use nika_display::activity::{CallState, Phase};
    use nika_display::activity_card::Update;
    use std::time::Duration;
    let card = |lang: Lang, ascii: bool| {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        (state.lang, state.ascii) = (lang, ascii);
        state.observe(Update::phase(
            Phase::Understanding,
            "reading your line",
            true,
        ));
        let call = |state, at| Update::call(1, "plan", "deepseek/v4", Phase::Authoring, state, at);
        state.observe(call(CallState::Started, Duration::ZERO));
        state.observe(call(CallState::Finished, Duration::from_secs(3)));
        state.settle_activity(Duration::from_millis(400));
        let block = (state.transcript.iter())
            .find(|block| block.kind == Kind::Activity)
            .expect("the card");
        block.text.clone()
    };
    assert_eq!(
        card(Lang::En, false),
        "Settled · took under 1 s · 1 author call · conversation routing and decision-service calls not counted\n\
         ✓ Understanding · reading your line\n\
         ○ call 1 · plan the workflow · requested deepseek/v4 · returned after 3 s"
    );
    assert_eq!(
        card(Lang::Fr, false),
        "Terminé · en moins d'1 s · 1 appel auteur · routage de la conversation et appels du service de décision non comptés\n\
         ✓ Compréhension · reading your line\n\
         ○ appel 1 · planifier le workflow · demandé deepseek/v4 · revenu après 3 s"
    );
    assert!(
        card(Lang::Fr, true).starts_with("Termine - en moins d'1 s - 1 appel auteur - routage")
    );
}
