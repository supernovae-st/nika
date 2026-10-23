// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a proof that cannot read its screen fails"
)]
//! Reception proofs of the choice surfaces on a real PTY, at three sizes, over the prototype's
//! `--demo-surface` fixtures (no Session, no provider): a 30-model settings list pages a band
//! at a time, typed words filter it and `Enter` applies the row they select, `Esc` steps back
//! one level and never answers until the list's own top; a multiple choice checks with Space
//! and validates the whole set with `Enter`. Each fixture says back exactly what it received.

mod qa_support;

use qa_support::{FREE, Term, assert_restored, exit_code};

/// The sizes these surfaces are judged at.
const SIZES: [(u16, u16); 3] = [(80, 24), (120, 40), (180, 48)];

const DOWN: &str = "\x1b[B";
const PAGE_DOWN: &str = "\x1b[6~";
const PAGE_UP: &str = "\x1b[5~";
const ESC: &str = "\x1b";

fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

fn intelligence(cols: u16, rows: u16) -> Term {
    let mut term = Term::proto(&["--demo-surface", "intelligence"], cols, rows);
    term.wait_until("the model list", |screen| {
        screen.contains("Choose a model") && screen.contains("1-8 of 30")
    });
    term
}

#[test]
fn a_long_settings_list_pages_filters_and_applies_the_row_its_words_select() {
    for (cols, rows) in SIZES {
        let mut term = intelligence(cols, rows);
        assert!(term.screen.contains("Claude Sonnet 5.5"), "{}", term.dump());
        term.send(PAGE_DOWN);
        term.wait_text("9-16 of 30");
        term.send("sonnet");
        term.wait_text("6 of 30 match");
        term.send(" r3");
        term.wait_text("1 of 30 match");
        assert!(
            term.screen.contains("Claude Sonnet 5.5 r3"),
            "{}",
            term.dump()
        );
        term.send("\r");
        term.wait_text("received: 2 anthropic/claude-sonnet-5-5-r3");
        // The person's line names the model picked, never its key.
        assert!(
            term.screen.seen("Intelligence: Claude Sonnet 5.5 r3"),
            "{}",
            term.dump()
        );
        assert!(!term.screen.seen("› 2 anthropic"), "{}", term.dump());
        term.wait_prompt(FREE);
        leave(&mut term);
    }
}

/// Typed words go first, alone; then one `Esc` closes the list, a row selected or not: the
/// selection is browsing, never a choice, and the current choice is kept.
#[test]
fn esc_empties_the_filter_then_closes_the_list_at_once() {
    for (cols, rows) in SIZES {
        let mut term = intelligence(cols, rows);
        term.send(DOWN);
        term.send("gpt");
        term.wait_text("6 of 30 match");
        term.send(ESC);
        term.wait_until("the filter emptied", |screen| {
            screen.contains("1-8 of 30") && !screen.contains("match")
        });
        assert!(!term.screen.seen("received:"), "{}", term.dump());
        assert!(
            term.screen.contains("› ("),
            "a row stays selected: {}",
            term.dump()
        );
        term.send(ESC);
        term.wait_text("received: cancel");
        leave(&mut term);
    }
}

/// A sentence typed over the list matches no row: the band says `Enter` sends it as a message
/// with the intelligence unchanged, never how to type an id, and `Enter` sends those words.
#[test]
fn a_sentence_over_the_list_is_sent_as_a_message() {
    for (cols, rows) in SIZES {
        let mut term = intelligence(cols, rows);
        term.send("hi there");
        term.wait_text("0 of 30 match");
        term.wait_text("Enter sends it as a message");
        assert!(!term.screen.contains("<model>"), "{}", term.dump());
        term.send("\r");
        term.wait_text("received: hi there");
        leave(&mut term);
    }
}

fn choice(cols: u16, rows: u16, args: &[&str]) -> Term {
    let mut all = vec!["--demo-surface", "choice"];
    all.extend_from_slice(args);
    let mut term = Term::proto(&all, cols, rows);
    term.wait_until("the question", |screen| {
        screen.contains("Which source") && screen.contains("Let Nika choose")
    });
    term
}

/// A question that recommends a source opens on it: `Enter` alone accepts it, nothing was
/// sent before; « Let Nika choose » follows the offers and sends the delegation in words.
/// The selection survives a terminal shrunk below the workspace minimum and grown back.
#[test]
fn a_recommendation_is_preselected_and_let_nika_choose_sends_the_delegation() {
    for (cols, rows) in SIZES {
        let mut term = choice(cols, rows, &[]);
        assert!(term.screen.contains("recommended"), "{}", term.dump());
        assert!(
            term.screen.contains("› 2 (●) TechCrunch"),
            "{}",
            term.dump()
        );
        assert!(!term.screen.seen("received:"), "{}", term.dump());
        term.send("\r");
        term.wait_text("received: tc");
        leave(&mut term);

        let mut term = choice(cols, rows, &[]);
        for _ in 0..3 {
            term.send(DOWN);
        }
        term.wait_until("Let Nika choose selected", |screen| {
            screen.contains("› Let Nika choose")
        });
        term.send("\r");
        term.wait_text("received: I leave the choice to Nika.");
        leave(&mut term);
    }
    // A digit only moves the selection; Enter sends.
    let mut term = choice(120, 40, &[]);
    term.send("3");
    term.wait_text("› 3 (●) The Verge");
    assert!(!term.screen.seen("received:"), "{}", term.dump());
    term.send("\r");
    term.wait_text("received: verge");
    leave(&mut term);
    let mut term = choice(80, 24, &[]);
    term.resize(50, 14);
    term.wait_until("the narrow frame", |screen| screen.contains("TechCrunch"));
    term.resize(80, 24);
    term.wait_until("the frame grown back", |screen| {
        screen.contains("› 2 (●) TechCrunch") && screen.contains("Let Nika choose")
    });
    term.send("\r");
    term.wait_text("received: tc");
    leave(&mut term);
}

/// The ASCII glyph column keeps every fact of the question: the selection, the
/// recommendation and the delegation row, with no glyph outside ASCII on their rows.
#[test]
fn a_recommended_question_reads_whole_in_the_ascii_glyph_column() {
    let mut term = choice(80, 24, &["--ascii"]);
    let rows = term.screen.lines();
    let band: Vec<&String> = rows
        .iter()
        .filter(|row| {
            ["TechCrunch", "Hacker News", "Let Nika choose"]
                .iter()
                .any(|w| row.contains(w))
        })
        .collect();
    assert_eq!(band.len(), 3, "{}", term.dump());
    assert!(band.iter().all(|row| row.is_ascii()), "{band:#?}");
    assert!(
        term.screen.contains("> 2 (*) TechCrunch"),
        "{}",
        term.dump()
    );
    assert!(term.screen.contains("recommended"), "{}", term.dump());
    term.send("\r");
    term.wait_text("received: tc");
    leave(&mut term);
}

fn hours(cols: u16, rows: u16, args: &[&str]) -> Term {
    let mut all = vec!["--demo-surface", "hours"];
    all.extend_from_slice(args);
    let mut term = Term::proto(&all, cols, rows);
    term.wait_until("the hour list", |screen| {
        screen.contains("At what time") && screen.contains("9h00")
    });
    term
}

/// A question without a recommendation opens on its first row, so the row drawn selected is
/// the one `Enter` applies: `Down` reaches the second hour and `Enter` sends it. (A founder
/// pressed `Down Enter` on such a list meaning 7h and got 6h, the list having opened on
/// nothing.) Typed words leave no radio filled: `Enter` sends them, never a row.
#[test]
fn a_question_opens_on_its_first_row_and_down_enter_takes_the_second() {
    for (cols, rows) in SIZES {
        let mut term = hours(cols, rows, &[]);
        assert!(term.screen.contains("› 1 (●) 6h00"), "{}", term.dump());
        assert!(term.screen.contains("  2 ( ) 7h00"), "{}", term.dump());
        term.send(DOWN);
        term.wait_text("› 2 (●) 7h00");
        assert!(term.screen.contains("  1 ( ) 6h00"), "{}", term.dump());
        assert!(!term.screen.seen("received:"), "{}", term.dump());
        term.send("\r");
        term.wait_text("received: 07:00");
        // The echo reads the hour picked, never the prompt it was answered after.
        assert!(term.screen.seen("› 7h00"), "{}", term.dump());
        assert!(!term.screen.seen("reply › 7h00"), "{}", term.dump());
        leave(&mut term);
    }
    // The row selected survives a terminal shrunk below the workspace minimum and grown back.
    let mut term = hours(80, 24, &[]);
    term.send(DOWN);
    term.wait_text("› 2 (●) 7h00");
    term.resize(50, 14);
    // The last hour is past the rows a shrunk screen keeps: it reads again once repainted.
    term.wait_until("the narrow frame", |screen| {
        screen.contains("› 2 (●) 7h00") && screen.contains("9h00")
    });
    term.resize(80, 24);
    term.wait_until("the frame grown back", |screen| {
        screen.contains("› 2 (●) 7h00") && screen.contains("9h00")
    });
    term.send("\r");
    term.wait_text("received: 07:00");
    leave(&mut term);
    let mut term = hours(80, 24, &[]);
    term.send("vers 7h30");
    term.wait_text("› 1 ( ) 6h00");
    assert!(!term.screen.contains("(●)"), "{}", term.dump());
    term.send("\r");
    term.wait_text("received: vers 7h30");
    leave(&mut term);
    let mut term = hours(80, 24, &["--ascii"]);
    assert!(term.screen.contains("> 1 (*) 6h00"), "{}", term.dump());
    term.send(DOWN);
    term.wait_text("> 2 (*) 7h00");
    term.send("\r");
    term.wait_text("received: 07:00");
    leave(&mut term);
}

/// While a line is prepared, one live line says what Nika does, for how long, the author
/// calls so far, what the returned ones cost as the Session priced them, and what `Ctrl+C`
/// does; once it ends, one line says how long it took, its calls and its known spend. In the
/// ASCII glyph column every fact stays, and the line survives a terminal shrunk below the
/// workspace minimum and grown back.
#[test]
fn a_preparation_says_its_live_facts_then_how_it_ended() {
    for (cols, rows) in SIZES {
        let mut term = Term::proto(&["--demo-surface", "working"], cols, rows);
        term.wait_prompt(FREE);
        term.send("digest my notes\r");
        term.wait_text("Preparing · ");
        term.wait_text("1 author call · Ctrl+C stop");
        // The first call returned priced while the second works: its cost, live.
        term.wait_text("2 author calls · ~$0.12 · Ctrl+C stop");
        term.wait_text("received: digest my notes");
        term.wait_text("Prepared in ");
        assert!(term.screen.contains("· 2 calls · $0.31"), "{}", term.dump());
        // The stage line keeps the session's spend so far, after its stages.
        term.wait_text("Run ○ · ~$0.31 spent since opening");
        leave(&mut term);
    }
    let mut term = Term::proto(&["--demo-surface", "working", "--ascii"], 80, 24);
    term.wait_text("nika >");
    term.resize(50, 14);
    term.resize(80, 24);
    term.wait_text("nika >");
    term.send("digest my notes\r");
    term.wait_text("Preparing - ");
    term.wait_text("1 author call - Ctrl+C stop");
    term.wait_text("2 author calls - ~$0.12 - Ctrl+C stop");
    term.wait_text("received: digest my notes");
    term.wait_text("- 2 calls - $0.31");
    // Shrunk below the workspace minimum and grown back while it prepares, the turn ends
    // and says so whole.
    term.send("again\r");
    term.wait_text("Preparing - ");
    term.resize(50, 14);
    term.resize(80, 24);
    term.wait_text("received: again");
    term.wait_until("the second end line", |screen| {
        screen
            .transcript()
            .iter()
            .filter(|line| line.contains("- 2 calls - $0.31"))
            .count()
            == 2
    });
    leave(&mut term);
}

/// The title counts the checked answers; `Ctrl+A` checks every one, then none.
#[test]
fn a_multiple_choice_checks_with_space_and_enter_validates_the_whole_set() {
    for (cols, rows) in SIZES {
        let mut term = Term::proto(&["--demo-surface", "multi"], cols, rows);
        term.wait_until("the multiple choice", |screen| {
            screen.contains("Which sources") && screen.contains("Submit")
        });
        assert!(term.screen.contains("0 of 4 selected"), "{}", term.dump());
        term.send("\x01");
        term.wait_text("4 of 4 selected");
        term.send("\x01");
        term.wait_text("0 of 4 selected");
        term.send("\r");
        term.wait_text("Space checks an answer");
        assert!(!term.screen.seen("received:"), "{}", term.dump());
        term.send(DOWN);
        term.send(" ");
        term.wait_text("1 of 4 selected");
        term.send(DOWN);
        term.send(DOWN);
        term.send(" ");
        term.wait_text("2 of 4 selected");
        term.send("\r");
        term.wait_text(r#"received: ["notes","tickets"]"#);
        leave(&mut term);
    }
}

/// A question mark with nothing typed opens the keys of what is on screen, in its own frame;
/// Esc (or a second question mark) returns with the choice exactly as it was and nothing sent.
/// Once words are typed, a question mark is a word. The ASCII glyph column reads the same keys.
#[test]
fn a_question_mark_reads_the_keys_of_the_screen_and_esc_returns() {
    for (cols, rows) in SIZES {
        let mut term = Term::proto(&["--demo-surface", "multi", "--focus"], cols, rows);
        term.wait_until("the multiple choice", |screen| {
            screen.contains("Which sources") && screen.contains("Submit")
        });
        term.send("?");
        // One frame may reach the screen in several reads: wait for the whole card.
        term.wait_until("the keys of the screen", |screen| {
            screen.contains("Keys · for what is on screen now")
                && screen.contains("Ctrl+A · check every answer")
                && screen.contains("Ctrl+O · every command")
        });
        term.send(ESC);
        term.wait_until("the choice back", |screen| {
            screen.contains("Submit") && !screen.contains("Keys · for what")
        });
        assert!(!term.screen.seen("received:"), "{}", term.dump());
        leave(&mut term);
    }
    let mut term = Term::proto(&["--demo-surface", "working", "--focus", "--ascii"], 80, 24);
    term.wait_text("nika >");
    term.send("?");
    term.wait_text("Keys - for what is on screen now");
    assert!(term.screen.contains("Conversation"), "{}", term.dump());
    term.send("?");
    term.wait_until("closed by a second question mark", |screen| {
        !screen.contains("Keys - for what")
    });
    term.send("why?");
    term.wait_text("why?");
    assert!(!term.screen.contains("Keys - for what"), "{}", term.dump());
    leave(&mut term);
}

/// A decision opens on no act selected; a digit moves the selection to that act and never acts
/// (nothing is received), and `Enter` does the selected act, once.
#[test]
fn a_digit_selects_a_decisions_act_and_only_enter_acts() {
    let mut term = Term::proto(&["--demo-surface", "decision"], 120, 40);
    term.wait_until("the save decision", |screen| {
        screen.contains("reviewed proposal") && screen.contains("4 ( ) Inspect")
    });
    assert!(
        !term.screen.contains("(●)"),
        "no act preselected: {}",
        term.dump()
    );
    term.send("2");
    term.wait_text("› 2 (●) Save and run once");
    term.send("3");
    term.wait_text("› 3 (●) Discard this proposal");
    term.send("2");
    term.wait_text("› 2 (●) Save and run once");
    assert!(
        !term.screen.seen("received:"),
        "a digit never acts: {}",
        term.dump()
    );
    term.send("\r");
    term.wait_text("received: save & run");
    leave(&mut term);
}

fn decision(cols: u16, rows: u16, args: &[&str]) -> Term {
    let mut all = vec!["--demo-surface", "decision"];
    all.extend_from_slice(args);
    let mut term = Term::proto(&all, cols, rows);
    term.wait_until("the save decision", |screen| {
        screen.contains("reviewed proposal") && screen.contains("Inspect the exact changes")
    });
    term
}

/// A decision's card covers the conversation on a full screen: its hint names `PgUp`, which
/// reads the conversation behind the card; the page keys scroll it, and any other key brings
/// the card back as it was, the key spent: nothing answered, selected or typed (live gauntlet
/// 5, r06 B9: the mandatory card hid the activity behind it). `Esc` with nothing left to undo
/// names the key too.
#[test]
fn pgup_reads_the_conversation_behind_a_card_and_any_other_key_brings_it_back() {
    let full = ["--focus", "--workspace"].iter().cycle();
    for ((cols, rows), presentation) in SIZES.into_iter().zip(full) {
        let mut term = decision(cols, rows, &[presentation]);
        let at = format!("{presentation} {cols}x{rows}");
        // The whole frame drawn: the card covers the conversation and its hint names the key.
        term.wait_until("the card over the conversation", |screen| {
            !screen.contains("demo surface") && screen.contains("PgUp: history")
        });
        term.send(PAGE_UP);
        // The card steps away: the conversation shows and the card's rows are gone.
        term.wait_until("the conversation behind the card", |screen| {
            screen.contains("demo surface")
                && screen.contains("any other key: back to the card")
                && !screen.contains("Inspect the exact changes")
        });
        term.send(PAGE_DOWN);
        term.wait_until("still behind the card", |screen| {
            screen.contains("demo surface") && screen.contains("back to the card")
        });
        term.send("y");
        term.wait_until("the card back", |screen| {
            screen.contains("Inspect the exact changes") && !screen.contains("back to the card")
        });
        term.send("\r");
        term.wait_text("nothing sent");
        assert!(
            !term.screen.seen("received:"),
            "{at}: the key answered nothing\n{}",
            term.dump()
        );
        term.send(ESC);
        term.wait_text("Esc never answers · PgUp: history");
        leave(&mut term);
    }
}

/// A command beside a decision answers visibly: the decision's band hides the conversation, so
/// what its turn said opens in the reader, and `Esc` returns to the decision as it was. Typing
/// an act's name fills that act's radio and `Enter` does that act: `Inspect the exact changes`
/// reads the changes the same way. The ASCII glyph column keeps every fact.
#[test]
fn a_command_beside_a_decision_reads_its_answer_and_an_acts_name_picks_it() {
    for (cols, rows) in SIZES {
        let mut term = decision(cols, rows, &[]);
        term.send("\x0f");
        term.wait_text("commands ›");
        term.send("meaning\r");
        term.wait_until("the answer read", |screen| {
            screen.contains("Answer to /meaning") && screen.contains("received: /meaning")
        });
        term.send(ESC);
        term.wait_until("the decision back", |screen| {
            !screen.contains("Answer to /meaning") && screen.contains("4 ( ) Inspect")
        });
        term.send("Inspect the exact changes");
        term.wait_text("4 (●) Inspect the exact changes");
        term.send("\r");
        term.wait_until("the changes read", |screen| {
            screen.contains("Answer to /show") && screen.contains("received: /show")
        });
        term.send(ESC);
        term.wait_until("the decision back again", |screen| {
            !screen.contains("Answer to /show") && screen.contains("4 ( ) Inspect")
        });
        assert!(!term.screen.seen("received: yes"), "{}", term.dump());
        leave(&mut term);
    }
    let mut term = decision(80, 24, &["--ascii"]);
    term.send("/why\r");
    term.wait_until("the answer read in ASCII", |screen| {
        screen.contains("Answer to /why - the Session's words, read only")
            && screen.contains("received: /why")
    });
    term.send(ESC);
    term.wait_until("the decision back in ASCII", |screen| {
        !screen.contains("Answer to /why") && screen.contains("4 ( ) Inspect")
    });
    leave(&mut term);
}

fn gate(cols: u16, rows: u16, args: &[&str]) -> Term {
    let mut all = vec!["--demo-surface", "gate"];
    all.extend_from_slice(args);
    let mut term = Term::proto(&all, cols, rows);
    term.wait_until("the approval gate", |screen| {
        screen.contains("workflow confirmation") && screen.contains("Explain this step")
    });
    term
}

/// At an approval gate a typed whole-line yes or no picks its act: its radio fills while the
/// word is typed, and `Enter` sends that act once. Other words send nothing and say so, `Esc`
/// empties them and never answers no, and the ASCII glyph column keeps every fact.
#[test]
fn an_approval_gate_takes_a_typed_yes_or_no() {
    for (cols, rows) in SIZES {
        let mut term = gate(cols, rows, &[]);
        term.send("oui");
        term.wait_text("1 (●) Approve this step");
        assert!(!term.screen.seen("received:"), "{}", term.dump());
        term.send("\r");
        term.wait_text("received: yes");
        leave(&mut term);
    }
    let mut term = gate(80, 24, &[]);
    term.send("peut-être");
    term.wait_text("peut-être");
    term.send("\r");
    term.wait_text("type yes or no");
    assert!(!term.screen.seen("received:"), "{}", term.dump());
    term.send(ESC);
    term.wait_until("the words emptied", |screen| !screen.contains("peut-être"));
    term.send(ESC);
    term.wait_text("Esc never answers");
    assert!(!term.screen.seen("received:"), "{}", term.dump());
    term.send("non");
    term.wait_text("2 (●) Refuse this step");
    term.send("\r");
    term.wait_text("received: no");
    leave(&mut term);
    let mut term = gate(80, 24, &["--ascii"]);
    term.send("valide");
    term.wait_text("1 (*) Approve this step");
    term.send("\r");
    term.wait_text("received: yes");
    leave(&mut term);
}

/// Every card can be left (NIK-227: a recovery card with one act could only be left by quitting).
/// `Esc` with nothing left to undo on a decision its conversation sets aside steps the card away,
/// unanswered, says what stays undecided and gives the line back: the next words are an ordinary
/// line, and the card does not come back over them.
#[test]
fn esc_sets_a_decision_aside_and_gives_the_line_back() {
    for (cols, rows) in SIZES {
        let mut term = Term::proto(&["--demo-surface", "recovery"], cols, rows);
        term.wait_until("the recovery card", |screen| {
            screen.contains("Adapt the workflow to the changed files")
        });
        term.send(ESC);
        term.wait_until("the card set aside", |screen| {
            screen.contains("stays not run")
                && !screen.contains("Adapt the workflow to the changed files")
        });
        assert!(
            !term.screen.seen("received:"),
            "Esc answered nothing: {}",
            term.dump()
        );
        term.send("what now?\r");
        term.wait_text("received: what now?");
        assert!(
            !term
                .screen
                .contains("Adapt the workflow to the changed files"),
            "{}",
            term.dump()
        );
        term.wait_prompt(FREE);
        leave(&mut term);
    }
}

/// A list question its author closed to its offers still takes the person's typed line: a
/// delegation (« choisis pour moi ») or a value (« vers 7h30 ») typed there reaches the Session
/// as the reply, never dropped, and no row is applied in its place.
#[test]
fn a_closed_list_question_sends_the_typed_line() {
    for words in ["choisis pour moi", "vers 7h30"] {
        let mut term = Term::proto(&["--demo-surface", "hours-closed"], 80, 24);
        term.wait_until("the closed hour list", |screen| {
            screen.contains("At what time") && screen.contains("9h00")
        });
        term.send(words);
        term.wait_text(words);
        term.send("\r");
        term.wait_text(&format!("received: {words}"));
        assert!(!term.screen.seen("received: 06:00"), "{}", term.dump());
        assert!(!term.screen.seen("nothing sent"), "{}", term.dump());
        leave(&mut term);
    }
}

/// Discard asks once before it is done: on the save decision `Down` three times reaches
/// « Discard this proposal », named on the hint row as what `Enter` does; the first `Enter` asks
/// « Discard this proposal? » and discards nothing, `Esc` keeps the proposal, and only a second
/// `Enter` discards it.
#[test]
fn discard_asks_once_before_it_is_done() {
    let mut term = decision(120, 40, &[]);
    for _ in 0..3 {
        term.send(DOWN);
    }
    term.wait_text("Enter acts: Discard this proposal");
    term.send("\r");
    term.wait_text("Discard this proposal? Enter again");
    assert!(!term.screen.seen("received:"), "{}", term.dump());
    term.send(ESC);
    term.wait_text("kept · nothing was discarded");
    assert!(!term.screen.seen("received:"), "{}", term.dump());
    term.send("\r");
    term.wait_text("Discard this proposal? Enter again");
    term.send("\r");
    term.wait_text("received: no");
    leave(&mut term);
}
