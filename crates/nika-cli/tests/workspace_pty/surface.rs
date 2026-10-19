// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The open UX on the real binary: one surface above a fixed composer. The
//! palette opens as a band over the conversation, the composer on its row,
//! and closes on the screen it opened over with the draft's caret where it
//! stood. A real typed choice (the observed-column question of the
//! deterministic reading) holds the line with the request the Session keeps
//! above the question; its keys never reach the words typed ahead, which
//! wait out of view through every native size and a trip below the minimum,
//! and through the proposal's decision that one offer's exact key yields;
//! they come back once that decision is answered (its `no` offer discards it). A
//! command chosen while a Run works waits in the box for the human's turn,
//! and `Ctrl+C` says the Run is not stopped there. Keyless: nothing is saved
//! or run that the proof did not ask for.

use super::*;

/// The palette's own key.
const CTRL_O: &str = "\x0f";
/// The first entry and the first page of the palette.
const HOME: &str = "\x1b[H";
/// A ticket table without the `state` column the request names: the
/// deterministic reading asks which observed column it means.
const TICKETS: &str = "id,status,amount\n1,open,10\n2,closed,20\n3,open,30\n";
/// The request whose `state` column the table does not have.
const REQUEST: &str =
    "Read ./tickets.csv, keep only the rows whose state is open and write them to ./open.csv";
/// Words typed while the request's turn works: the draft a choice keeps.
const AHEAD: &str = "my next words";
/// The electric-blue accent, as a true-colour foreground run.
const ACCENT: &str = "38;2;76;163;255";
/// The selection's fill, as a true-colour background run.
const SELECTION: &str = "48;2;20;45;73";
/// One wait long enough to choose a command while the Run works.
const HELD: &str = r#"nika: held
permits:
  tools: ["nika:wait"]
tasks:
  hold:
    invoke: { tool: "nika:wait", args: { duration: "15s" } }
"#;
/// The native sizes a surface is proven at, the spawn size first.
const SIZES: [(u16, u16); 3] = [(120, 40), (80, 24), (180, 48)];

/// `screen` is `size`, cell for cell.
fn sized(screen: &vt::Screen, (cols, rows): (u16, u16)) -> bool {
    screen.size() == (usize::from(cols), usize::from(rows))
}

/// Whether the workspace draws at `size` (its minimum is 60 x 16).
const fn geometry_fits((cols, rows): (u16, u16)) -> bool {
    cols >= 60 && rows >= 16
}

/// The composer box's top-left corner over the row holding `prompt`: the
/// box's side stands two cells left of the prompt, its corner on the row
/// above.
fn box_corner(screen: &vt::Screen, prompt: &str) -> Option<char> {
    let lines = screen.lines();
    let row = lines.iter().position(|line| line.contains(prompt))?;
    let byte = lines[row].find(prompt)?;
    let column = lines[row][..byte].chars().count().checked_sub(2)?;
    lines.get(row.checked_sub(1)?)?.chars().nth(column)
}

/// The binary ends on `SIGTERM` with the terminal given back.
fn terminate(term: &mut Term) {
    term.signal(Signal::SIGTERM);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(143), "{status:?}\n{}", term.dump());
    term.assert_restored();
}

/// 40 · At each native size the palette opens as a band over the
/// conversation: the composer keeps its row, its box joined to the band
/// (tees where its rounded corners stood), searching moves nothing, and a
/// list longer than the band pages whole. `Esc` gives back the screen it
/// opened over, row for row, and the draft's caret where it stood: the next
/// letter lands mid-word.
#[test]
fn the_palette_opens_over_a_fixed_composer_and_gives_back_the_exact_draft() {
    let rig = Rig::new("open-ux-palette");
    let before = rig.tree();
    let mut term = rig.spawn_with(
        "40-open-ux-palette",
        &[],
        120,
        40,
        &[("NIKA_REDUCED_MOTION", "1")],
    );
    wait_workspace(&mut term);
    term.send("keep these words");
    term.send(&LEFT.repeat("words".len()));
    for size in SIZES {
        palette_round_trip(&mut term, size);
    }
    assert!(
        term.screen.hues().contains(ACCENT),
        "the accent is the electric blue: {:?}",
        term.screen.hues()
    );
    assert_eq!(rig.tree(), before, "the palette writes nothing");
    terminate(&mut term);
    rig.no_model_call("the palette opened and closed over a draft");
}

/// One palette round trip at `size` over the draft `keep these words`, its
/// caret before `words`.
fn palette_round_trip(term: &mut Term, size: (u16, u16)) {
    let at = format!("{}x{}", size.0, size.1);
    if !sized(&term.screen, size) {
        term.resize(size.0, size.1);
    }
    term.wait_workspace_frame(&format!("{at}: the draft at rest"), |screen| {
        sized(screen, size) && screen.contains("nika › keep these words")
    });
    let rest = term.screen.lines();
    let line = term
        .screen
        .row_of("nika › keep these words")
        .expect("the composer row");
    // The composer stands in a box where the conversation has a column of
    // its own (from 100 columns); stacked at 80, it is a line.
    let boxed = size.0 >= 100;
    if boxed {
        assert_eq!(
            box_corner(&term.screen, "nika › keep these words"),
            Some('╭'),
            "{at}: a rounded box at rest\n{}",
            term.dump()
        );
    }
    term.send(CTRL_O);
    term.wait_workspace_frame(
        &format!("{at}: the palette over the conversation"),
        |screen| screen.contains("Commands · choose an action") && screen.contains("commands ›"),
    );
    let open = term.screen.lines();
    if boxed {
        assert_eq!(
            box_corner(&term.screen, "commands ›"),
            Some('├'),
            "{at}: the box is not joined to the band\n{}",
            term.dump()
        );
    }
    assert_eq!(
        term.screen.row_of("commands ›"),
        Some(line),
        "{at}: the composer moved"
    );
    assert_eq!(
        open[0],
        rest[0],
        "{at}: the header changed\n{}",
        term.dump()
    );
    if size == (80, 24) {
        palette_pages(term, &at);
    }
    term.send("stat");
    term.wait_workspace_frame(&format!("{at}: the search"), |screen| {
        screen.contains("commands › stat") && screen.contains("› /status")
    });
    assert_eq!(
        term.screen.row_of("commands › stat"),
        Some(line),
        "{at}: the search moved"
    );
    term.send(ESC);
    term.wait_workspace_frame(&format!("{at}: the palette closed"), |screen| {
        !screen.contains("commands ›") && screen.contains("nika › keep these words")
    });
    assert_eq!(
        term.screen.lines(),
        rest,
        "{at}: closing changed the screen\n{}",
        term.dump()
    );
    term.send("X");
    term.wait_workspace_frame(
        &format!("{at}: the next letter where the caret stood"),
        |screen| screen.contains("nika › keep these Xwords"),
    );
    term.send("\x7f");
    term.wait_workspace_frame(&format!("{at}: the draft as it was"), |screen| {
        screen.contains("nika › keep these words")
    });
}

/// The palette lists more than its band holds: `End` shows the last page
/// (the first entry gone), `Home` the first again.
fn palette_pages(term: &mut Term, at: &str) {
    term.send(END);
    term.wait_workspace_frame(&format!("{at}: the last page"), |screen| {
        !screen.contains("/help") && screen.contains("commands ›")
    });
    term.send(HOME);
    term.wait_workspace_frame(&format!("{at}: the first page"), |screen| {
        screen.contains("› /help") && screen.contains("commands ›")
    });
}

/// 41 · A real typed choice holds the line: the Session's question, the
/// request it keeps and the observed columns, in a band over the
/// conversation, while the words typed during the request's turn wait out
/// of view. It opens on its first offer, the one `Enter` would send; the own
/// reply takes letters (no radio filled meanwhile), never the draft; `PgUp`
/// moves nothing hidden. The
/// selection survives each native size and a trip below the minimum, and
/// `Enter` sends its exact key once: the Session takes it as an offered
/// answer and the proposal follows as a decision that holds the line, the
/// draft still out of view. The decision's `no` offer (« Discard this
/// proposal »), selected where the offers put it and sent with `Enter`
/// (`Esc` never answers a decision): the draft is back in the box, the
/// conversation keeps the Session's record of the offered key, and nothing
/// is saved.
#[test]
fn a_real_typed_choice_holds_the_line_and_gives_the_draft_back_once_answered() {
    let rig = Rig::new("open-ux-choice");
    std::fs::write(rig.path("tickets.csv"), TICKETS).expect("tickets");
    let before = rig.tree();
    let mut term = rig.spawn_with(
        "41-open-ux-choice",
        &[],
        120,
        40,
        &[("NIKA_REDUCED_MOTION", "1")],
    );
    wait_workspace(&mut term);
    ask_the_column(&mut term);
    nothing_reaches_the_draft(&mut term);
    walk_the_offers(&mut term);
    for size in [(80, 24), (180, 48), (50, 14), (120, 40)] {
        term.resize(size.0, size.1);
        let what = format!("{}x{}: the choice and its selection", size.0, size.1);
        let held = |screen: &vt::Screen| {
            sized(screen, size) && screen.contains("› 2 (●) status") && !screen.contains(AHEAD)
        };
        // Below the minimum the focus view draws it; a workspace frame ends hidden.
        if geometry_fits(size) {
            term.wait_workspace_frame(&what, held);
        } else {
            term.wait_until(&what, held);
        }
    }
    assert!(
        term.screen.contains("Question · required"),
        "{}",
        term.dump()
    );
    term.send("\r");
    // The proposal's decision holds the line now; its band covers the
    // conversation and the words typed ahead stay out of view.
    term.wait_workspace_frame(
        "one offer's key sent, the decision holds the line",
        |screen| {
            action_holds(screen) && screen.contains("What should happen") && !screen.contains(AHEAD)
        },
    );
    assert_eq!(rig.tree(), before, "binding is neither Save nor Run");
    // The decision's `no` offer, selected natively: Discard asks once, the second `Enter`
    // discards.
    select_offer(&mut term, "Discard this proposal");
    term.send("\r");
    term.wait_workspace_frame("Discard asks once", |screen| {
        screen.contains("Discard this proposal? Enter again")
    });
    assert_eq!(rig.tree(), before, "asking discards nothing");
    term.send("\r");
    term.wait_workspace_frame("the proposal discarded, the draft back", |screen| {
        screen.contains("discarded · nothing was written")
            && (screen.lines().iter()).any(|row| row.contains("nika ›") && row.contains(AHEAD))
    });
    recorded(&mut term, &["«status»", "one of the offered answers"]);
    assert_eq!(rig.tree(), before, "a discarded proposal writes nothing");
    terminate(&mut term);
    rig.no_model_call("an offered key bound, its proposal discarded");
}

/// The Session's own record of a value, every one of `words` in it (« Answer taken · …
/// «status» one of the offered answers »), in the conversation the decision no longer
/// covers: read from the latest row back, eight pages at most, then the latest row again.
fn recorded(term: &mut Term, words: &[&str]) {
    let mut seen = vec![false; words.len()];
    for page in 0..=8 {
        for (seen, word) in seen.iter_mut().zip(words) {
            *seen |= term.screen.contains(word);
        }
        if seen.iter().all(|seen| *seen) {
            break;
        }
        if page < 8 {
            term.keys(PAGE_UP);
        }
    }
    assert!(
        seen.iter().all(|seen| *seen),
        "the record {words:?} within eight pages\n{}",
        term.dump()
    );
    term.keys(END);
}

/// The offers a proposal's decision paints, in their order (`save & run` only when the
/// change can run).
const DECISION_OFFERS: [&str; 5] = [
    "Save workflow",
    "Save these changes",
    "Save and run once",
    "Discard this proposal",
    "Inspect the exact changes",
];

/// Select the decision's offer `label` with `Down`, as many presses as the painted offers
/// put between the selected one (`› `) and it (one more when none is selected yet: the first
/// `Down` selects the first offer), then wait until it is the selected one.
fn select_offer(term: &mut Term, label: &str) {
    let rows = term.screen.lines();
    let painted: Vec<&String> = (rows.iter())
        .filter(|row| DECISION_OFFERS.iter().any(|offer| row.contains(offer)))
        .collect();
    let from = (painted.iter()).position(|row| row.contains("› "));
    let to = (painted.iter()).position(|row| row.contains(label));
    let Some(to) = to else {
        panic!("the decision's offer `{label}`\n{}", term.dump());
    };
    let presses = match from {
        Some(from) if from <= to => to - from,
        Some(_) => panic!("`{label}` above the selection\n{}", term.dump()),
        None => to + 1,
    };
    for _ in 0..presses {
        term.send(DOWN);
    }
    term.wait_workspace_frame(&format!("`{label}` selected"), |screen| {
        (screen.lines().iter()).any(|row| row.contains("› ") && row.contains(label))
    });
}

/// The request and the words typed while its turn works leave in ONE write:
/// the typed choice holds the line, the words wait out of view.
fn ask_the_column(term: &mut Term) {
    term.send(&format!("{REQUEST}\r{AHEAD}"));
    term.wait_workspace_frame("the typed choice holds the line", |screen| {
        screen.contains("Question · required")
            && screen.contains("Request · Read ./tickets.csv")
            && screen.contains("Which observed field in")
            && screen.contains("type your own reply")
    });
    assert!(
        !term.screen.contains(AHEAD),
        "the draft shows\n{}",
        term.dump()
    );
    assert!(
        term.screen.hues().contains(ACCENT),
        "{:?}",
        term.screen.hues()
    );
}

/// The question opens on its first offer, the one `Enter` would send; the
/// own reply's field takes letters (the radio unfilled while they would be
/// sent) and gives them back; `PgUp` reads the conversation behind the card,
/// the line still the own reply's, and any other key brings the card back as
/// it was, its first offer selected, the key spent.
fn nothing_reaches_the_draft(term: &mut Term) {
    term.wait_workspace_frame("the question opened on its first offer", |screen| {
        screen.contains("› 1 (●) id")
    });
    term.send("usd");
    term.wait_workspace_frame("the own reply's letters", |screen| {
        screen.contains("reply › usd") && screen.contains("› 1 ( ) id") && !screen.contains(AHEAD)
    });
    term.send("\x7f\x7f\x7f");
    term.wait_workspace_frame("the own reply emptied", |screen| {
        screen.contains("type your own reply")
            && !screen.contains("usd")
            && screen.contains("› 1 (●) id")
    });
    let shown = term.screen.lines();
    term.send(PAGE_UP);
    // Behind the card the conversation reads whole (the notice that the draft waits in the
    // box included); the line keeps the own reply's empty field, never the draft.
    term.wait_workspace_frame("the conversation behind the card", |screen| {
        screen.contains("back to the card")
            && !screen.contains("1 (●) id")
            && screen.contains("or type your own reply")
    });
    term.send("x");
    term.wait_workspace_frame("the card back as it was", |screen| screen.lines() == shown);
}

/// From the first offer the question opened on, the arrows walk the three offers,
/// `Home` and `End` reach either end, and `status` stays selected, filled.
fn walk_the_offers(term: &mut Term) {
    for (key, offer) in [
        (DOWN, "› 2 (●) status"),
        (END, "› 3 (●) amount"),
        (HOME, "› 1 (●) id"),
        (DOWN, "› 2 (●) status"),
    ] {
        term.send(key);
        term.wait_workspace_frame(&format!("{offer} selected"), |screen| {
            screen.contains(offer) && !screen.contains(AHEAD)
        });
    }
    assert!(
        term.screen.hues().contains(SELECTION),
        "{:?}",
        term.screen.hues()
    );
}

/// 42 · The same choice without colour, then in ASCII, at 80 × 24: no colour
/// run under `NO_COLOR`, nothing beyond ASCII with `--ascii`, the question,
/// the kept request and the selection readable: it opens on its first offer
/// and `Down` reaches the second.
#[test]
fn the_typed_choice_reads_without_colour_and_in_ascii() {
    type Case<'a> = (&'a str, &'a [&'a str], &'a str, &'a str);
    let cases: &[Case<'_>] = &[
        (
            "42-open-ux-choice-no-color",
            &[],
            "› 1 (●) id",
            "› 2 (●) status",
        ),
        (
            "43-open-ux-choice-ascii",
            &["--ascii"],
            "> 1 (*) id",
            "> 2 (*) status",
        ),
    ];
    for (tag, args, selected, second) in cases {
        let rig = Rig::new(tag);
        std::fs::write(rig.path("tickets.csv"), TICKETS).expect("tickets");
        let env = [("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")];
        // Opened wide (the project's workflows in view), then 80 x 24.
        let mut term = rig.spawn_with(tag, args, 120, 40, &env);
        wait_workspace(&mut term);
        term.resize(80, 24);
        term.wait_workspace_frame("the workspace at 80x24", |screen| sized(screen, (80, 24)));
        term.send(&format!("{REQUEST}\r"));
        term.wait_workspace_frame("the typed choice without colour", |screen| {
            screen.contains("Question") && screen.contains("Request")
        });
        term.wait_workspace_frame("the first offer selected as it opens", |screen| {
            screen.contains(selected)
        });
        term.send(DOWN);
        term.wait_workspace_frame("Down reaches the second offer", |screen| {
            screen.contains(second) && !screen.contains(selected)
        });
        if !args.is_empty() {
            assert_renderer_ascii(&term.text());
        }
        let raw = String::from_utf8_lossy(&term.raw);
        for hue in ["38;2;", "48;2;", "38;5;", "48;5;"] {
            assert!(
                !raw.contains(hue),
                "{tag}: a colour under NO_COLOR: {hue:?}"
            );
        }
        terminate(&mut term);
        rig.no_model_call(&format!("{tag}: the typed choice asked and selected"));
    }
}

/// 44 · While a Run works the palette still opens over the conversation;
/// `Enter` on a command keeps it in the box for the human's turn, the hint
/// saying so, and `Ctrl+C` says the Run is not stopped here. Once the Run
/// settles the command still waits; the human's own `Enter` sends it.
#[test]
fn a_command_chosen_while_a_run_works_waits_for_the_turn() {
    let rig = Rig::new("open-ux-busy");
    std::fs::write(rig.path("held.nika"), HELD).expect("held");
    let mut term = rig.spawn_with(
        "44-open-ux-busy",
        &[],
        120,
        40,
        &[("NIKA_REDUCED_MOTION", "1")],
    );
    wait_workspace(&mut term);
    term.send("run held.nika\r");
    term.wait_until("the Run under way", |screen| {
        screen.contains("graph · the bytes") && screen.contains("it was asked over")
    });
    term.send(CTRL_O);
    term.wait_workspace_frame("the palette while the Run works", |screen| {
        screen.contains("Commands · choose an action")
            && screen.contains("Enter keeps it for your turn")
    });
    term.send("status");
    term.wait_workspace_frame("the command found", |screen| screen.contains("› /status"));
    term.send("\r");
    term.wait_workspace_frame("the command waits in the box", |screen| {
        screen.contains("nika › /status") && screen.contains("Nika keeps working · /status waits")
    });
    term.send("\x03");
    term.wait_workspace_frame("Ctrl+C says the Run is not stopped here", |screen| {
        screen.contains("a Run is under way and is not stopped")
    });
    term.wait_workspace_frame("the Run settled, the command still waiting", |screen| {
        screen.contains("settled · succeeded") && screen.contains("nika › /status")
    });
    term.send("\r");
    // Sent, the box is empty again (the conversation echoes the line).
    term.wait_workspace_frame("the command sent by the human's own Enter", |screen| {
        screen.contains("Ask, change, or run")
    });
    terminate(&mut term);
    rig.no_model_call("a run, the palette and a waiting command");
}
