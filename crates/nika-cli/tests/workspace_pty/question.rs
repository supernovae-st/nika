// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The real Live question, its retained text and a distinct newer draft.
//! No provider or implicit Save/Run; the same native frame owns the reply.

use super::*;

#[test]
fn an_unread_live_answer_and_newer_draft_survive_without_binding_or_writes() {
    let rig = Rig::new("typed-question");
    let before = rig.tree();
    let mut term = rig.spawn_with(
        "22-typed-question",
        &[],
        120,
        40,
        &[("NIKA_REDUCED_MOTION", "1")],
    );
    wait_workspace(&mut term);
    term.send("aggregate-by-key\r");
    term.wait_workspace_frame("the real currency question", |screen| {
        screen.contains("the currency code") && screen.contains("reply ›")
    });
    question_sizes(&mut term, &rig, &before);
    read_full_question(&mut term, &rig, &before);
    let words = "use euro or dollars\nbut keep these words  ";
    term.send(&format!("\x1b[200~{words}\x1b[201~"));
    term.wait_workspace_frame("the multiline answer is still a draft", |screen| {
        screen.contains("use euro or dollars") && screen.contains("but keep these words")
    });
    // Enter attempts that answer; the following keys remain newer unsent words.
    let newer = "newer draft";
    term.send(&format!("\r{newer}"));
    term.wait_workspace_frame("unread answer and newer words retained", |screen| {
        screen.contains("not taken")
            && screen.contains(newer)
            && screen.contains("reply ›")
            && !screen.contains("Save? ›")
    });
    assert_eq!(rig.tree(), before, "no workflow or business write");
    // With the caret after the retained words, erase that known exact draft.
    // Backspace edits, it cannot answer. Unit tests assert every retained byte.
    term.send(&"\x7f".repeat(words.chars().count() + 1 + newer.chars().count()));
    term.wait_workspace_frame("the cleared draft still answers this question", |screen| {
        review_rows(screen).iter().any(|row| {
            row.split_once("reply ›")
                .is_some_and(|(_, words)| words.trim().is_empty())
        })
    });
    term.send("EUR\r");
    term.wait_workspace_frame("one explicit value yields a proposal", |screen| {
        screen.contains("Save? ›")
            && screen.contains("aggregate-by-key.nika")
            && screen.contains("Answer taken")
            && screen.contains("«EUR»")
            && screen.contains("as you typed it")
    });
    assert_eq!(rig.tree(), before, "binding is neither Save nor Run");
    for (cols, rows) in [(80, 24), (180, 48), (120, 40)] {
        term.resize(cols, rows);
        term.wait_workspace_frame("the unsaved proposal at a native size", |screen| {
            screen.size() == (usize::from(cols), usize::from(rows)) && screen.contains("Save? ›")
        });
        assert_eq!(rig.tree(), before, "viewing and resize grant no effect");
    }
    read_full_proposal(&mut term, &rig, &before);
    term.signal(Signal::SIGTERM);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(143), "{status:?}\n{}", term.dump());
    term.assert_restored();
}

/// Reading the Session's complete preview is view only; Enter returns to the
/// exact pending draft rather than submitting it as a consent or revision.
fn read_full_proposal(term: &mut Term, rig: &Rig, before: &[(String, Vec<u8>)]) {
    let draft = "hold for review";
    term.send(draft);
    term.wait_workspace_frame("a separate proposal draft before reading", |screen| {
        screen
            .lines()
            .iter()
            .any(|row| row.contains("Save? ›") && row.contains(draft))
    });
    term.send("\x1bOQ");
    term.wait_workspace_frame("the whole proposal in a read-only reader", |screen| {
        screen.lines()[0].contains("Full proposal")
            && screen.contains("the Session's words, read only")
            && screen.contains("Nothing has run yet")
            && screen.contains("currency: EUR")
    });
    assert_eq!(rig.tree(), before, "reading grants no Save or Run");
    term.send("\r");
    term.wait_workspace_frame("reader Enter returns without sending the draft", |screen| {
        !screen.lines()[0].contains("Full proposal")
            && screen
                .lines()
                .iter()
                .any(|row| row.contains("Save? ›") && row.contains(draft))
    });
    assert_eq!(rig.tree(), before, "closing the reader grants no effect");
}

/// Question words, the answer line and the exact draft survive each native size.
fn question_sizes(term: &mut Term, rig: &Rig, before: &[(String, Vec<u8>)]) {
    for (cols, rows) in [(60, 18), (80, 24), (180, 48), (120, 40)] {
        term.resize(cols, rows);
        term.wait_workspace_frame("the pending question at a native size", |screen| {
            screen.size() == (usize::from(cols), usize::from(rows))
                && question_words_visible(screen)
                && screen.contains("reply ")
                && !screen.contains("Save?")
        });
        let words = question_region_rows(&term.screen)
            .expect("the current question region")
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            words.contains("it is taken exactly as you type it"),
            "the reply rule must be visible beside the input at {cols}x{rows}: {words}"
        );
        assert!(
            !words.contains("the whole question: F2"),
            "a cue cannot replace reply words that fit at {cols}x{rows}: {words}"
        );
        assert_eq!(rig.tree(), before, "question resize grants no effect");
    }
}

/// Find the actual conversation columns from its native title separators.
/// A question label repeated in the status cannot qualify its visible words.
fn question_words_visible(screen: &vt::Screen) -> bool {
    question_region_rows(screen).is_some_and(|rows| {
        rows.iter().any(|row| {
            row.trim_start_matches([' ', '│', '|'])
                .starts_with("the currency code")
        })
    })
}

/// The actual question columns, shared by label and complete reply-rule proofs.
fn question_region_rows(screen: &vt::Screen) -> Option<Vec<String>> {
    let width = screen.size().0;
    let rows = screen.lines();
    let (start, end) = if width >= 120 {
        let title = rows.iter().find(|row| row.starts_with("Project"))?;
        let mut cell = 0;
        let mut edges = Vec::new();
        for glyph in title.chars() {
            if matches!(glyph, '│' | '|') {
                edges.push(cell);
            }
            cell += unicode_width::UnicodeWidthChar::width(glyph).unwrap_or(0);
        }
        let [left, right, ..] = edges.as_slice() else {
            return None;
        };
        (left + 1, *right)
    } else {
        (0, width)
    };
    Some(
        rows.iter()
            .map(|row| {
                let mut cell = 0;
                row.chars()
                    .filter(|glyph| {
                        let here = cell;
                        cell += unicode_width::UnicodeWidthChar::width(*glyph).unwrap_or(0);
                        (start..end).contains(&here)
                    })
                    .collect()
            })
            .collect(),
    )
}

/// Complete current words remain read only, beside the exact unsent answer.
fn read_full_question(term: &mut Term, rig: &Rig, before: &[(String, Vec<u8>)]) {
    let draft = "keep this question draft";
    term.send(draft);
    term.wait_workspace_frame("a question draft before reading", |screen| {
        screen
            .lines()
            .iter()
            .any(|row| row.contains("reply ") && row.contains(draft))
    });
    term.send("\x1bOQ");
    term.wait_workspace_frame("the full question in a read-only reader", |screen| {
        screen.lines()[0].contains("Full question")
            && screen.contains("the Session's words, read only")
            && screen.contains("the currency code")
            && screen.contains("no intelligence reads this reply")
    });
    assert_eq!(rig.tree(), before, "question reading grants no effect");
    term.send("\r");
    term.wait_workspace_frame("reader returns the exact unsent question draft", |screen| {
        !screen.lines()[0].contains("Full question")
            && screen
                .lines()
                .iter()
                .any(|row| row.contains("reply ") && row.contains(draft))
            && !screen.contains("Save?")
    });
    assert_eq!(rig.tree(), before, "reader Enter submits no answer");
    term.send(&"\x7f".repeat(draft.chars().count()));
    term.wait_workspace_frame("reader draft erased without answering", |screen| {
        review_rows(screen).iter().any(|row| {
            row.split_once("reply ")
                .is_some_and(|(_, words)| words.trim_matches([' ', '›', '>']).is_empty())
        })
    });
}

#[test]
fn the_pending_question_keeps_its_words_and_composer_across_sizes_and_appearance() {
    type AppearanceCase<'a> = (&'a str, &'a [&'a str], &'a [(&'a str, &'a str)]);
    let cases: &[AppearanceCase<'_>] = &[
        ("27-question-color", &[], &[("NIKA_REDUCED_MOTION", "1")]),
        (
            "28-question-no-color",
            &[],
            &[("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")],
        ),
        (
            "29-question-ascii",
            &["--ascii"],
            &[("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")],
        ),
    ];
    for (tag, args, env) in cases {
        // Each appearance starts a fresh project and home, with no restored question history.
        let rig = Rig::new(tag);
        let before = rig.tree();
        let mut term = rig.spawn_with(tag, args, 120, 40, env);
        wait_workspace(&mut term);
        term.send("aggregate-by-key\r");
        term.wait_workspace_frame("the actual question before appearance resize", |screen| {
            screen.contains("the currency code") && screen.contains("reply ")
        });
        question_sizes(&mut term, &rig, &before);
        if !args.is_empty() {
            assert_renderer_ascii(&term.text());
        }
        if env.iter().any(|(key, _)| *key == "NO_COLOR") {
            let raw = String::from_utf8_lossy(&term.raw);
            for hue in ["38;2;", "48;2;", "38;5;", "48;5;"] {
                assert!(!raw.contains(hue), "a color under NO_COLOR: {hue:?}");
            }
        }
        assert_eq!(rig.tree(), before, "appearance grants no Save or Run");
        term.signal(Signal::SIGTERM);
        let status = term.finish();
        assert_eq!(exit_code(status), Some(143), "{status:?}\n{}", term.dump());
        term.assert_restored();
    }
}

/// A small native decision exposes what Save would apply beside unsent input.
/// Reading or changing size cannot create that workflow or execute it.
#[test]
fn the_compact_proposal_keeps_its_identity_effects_and_unsent_input_in_view() {
    let rig = Rig::new("compact-decision");
    let before = rig.tree();
    let mut term = rig.spawn_with(
        "30-compact-decision",
        &[],
        120,
        40,
        &[("NIKA_REDUCED_MOTION", "1")],
    );
    wait_workspace(&mut term);
    term.send("aggregate-by-key\r");
    term.wait_workspace_frame("a real currency question before consent", |screen| {
        screen.contains("the currency code") && screen.contains("reply ")
    });
    term.send("EUR\r");
    term.wait_workspace_frame("the current unsaved proposal", |screen| {
        screen.contains("Save? ") && screen.contains("aggregate-by-key.nika")
    });
    let draft = "hold this decision";
    term.send(draft);
    term.wait_workspace_frame("an unsent consent draft", |screen| {
        screen
            .lines()
            .iter()
            .any(|row| row.contains("Save? ") && row.contains(draft))
    });
    for (cols, rows) in [(80, 24), (80, 30), (99, 30), (80, 40), (120, 40)] {
        term.resize(cols, rows);
        term.wait_workspace_frame("the material Save facts and input at this size", |screen| {
            if screen.size() != (usize::from(cols), usize::from(rows)) {
                return false;
            }
            let words = if cols >= 120 {
                review_text(screen)
            } else {
                let lines = screen.lines();
                let Some(title) = lines
                    .iter()
                    .position(|row| row.contains("this conversation"))
                else {
                    return false;
                };
                lines[title + 1..]
                    .join(" ")
                    .replace(['│', '|'], " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            // The identity a yes answers rides the card's border beside its
            // reader key, whatever else names the proposal; the status row
            // keeps the scoped standing, the card the material facts.
            let footed = words.split("proposal ").skip(1).any(|rest| {
                rest.split_once(" · F2: whole words")
                    .is_some_and(|(id, _)| {
                        id.len() == 12 && id.chars().all(|c| c.is_ascii_hexdigit())
                    })
            });
            let facts = [
                "Not saved yet",
                "creates aggregate-by-key.nika",
                "when it runs",
            ];
            footed
                && facts.iter().all(|fact| words.contains(fact))
                && screen
                    .lines()
                    .iter()
                    .any(|row| row.contains("Save? ") && row.contains(draft))
        });
        assert_eq!(
            rig.tree(),
            before,
            "the same decision grants no Save or Run"
        );
    }
    term.signal(Signal::SIGTERM);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(143), "{status:?}\n{}", term.dump());
    term.assert_restored();
}
