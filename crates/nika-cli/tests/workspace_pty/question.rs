// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The real Live question, its retained text and a distinct newer draft.
//! The kept intelligence reads each typed reply (a scripted loopback: the
//! routed lines are asserted exactly); no implicit Save or Run and no workflow
//! effect; the same native frame owns the reply.
//! The proposal an answer completes waits as a finite action (`action ›`),
//! which still takes typed words.

use super::*;

#[test]
fn an_unread_live_answer_and_newer_draft_survive_without_binding_or_writes() {
    // The kept intelligence routes each typed reply: the two-line one, naming two currencies,
    // is UNKNOWN to it and binds nothing; `EUR` is the ANSWER.
    let rig = Rig::scripted("typed-question", &["UNKNOWN", "ANSWER"]);
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
            && !action_prompt_shown(screen)
    });
    assert_eq!(rig.tree(), before, "no workflow or business write");
    rig.routed(&[words.trim()], "a reply that binds nothing");
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
    // The consent now holds the line as a finite action (`action ›`); its band covers the
    // conversation, and the full proposal's reader below shows the value bound (`currency: EUR`).
    term.wait_workspace_frame("one explicit value yields a proposal", |screen| {
        action_holds(screen) && screen.contains("aggregate-by-key.nika")
    });
    assert_eq!(rig.tree(), before, "binding is neither Save nor Run");
    rig.routed(&[words.trim(), "EUR"], "each typed reply, read once");
    for (cols, rows) in [(80, 24), (180, 48), (120, 40)] {
        term.resize(cols, rows);
        term.wait_workspace_frame("the unsaved proposal at a native size", |screen| {
            screen.size() == (usize::from(cols), usize::from(rows)) && action_holds(screen)
        });
        assert_eq!(rig.tree(), before, "viewing and resize grant no effect");
    }
    read_full_proposal(&mut term, &rig, &before);
    rig.routed(&[words.trim(), "EUR"], "viewing and resize read no reply");
    rig.no_run("binding, viewing and resize run nothing");
    term.signal(Signal::SIGTERM);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(143), "{status:?}\n{}", term.dump());
    term.assert_restored();
}

/// Reading the Session's complete preview is view only; Enter returns to the
/// exact pending draft rather than submitting it as a consent or revision.
/// The decision's own reply field holds the draft on the action's row.
fn read_full_proposal(term: &mut Term, rig: &Rig, before: &[(String, Vec<u8>)]) {
    let draft = "hold for review";
    term.send(draft);
    term.wait_workspace_frame("a separate proposal draft before reading", |screen| {
        screen
            .lines()
            .iter()
            .any(|row| row.contains("action ›") && row.contains(draft))
    });
    term.send("\x1bOQ");
    term.wait_workspace_frame("the whole proposal in a read-only reader", |screen| {
        screen.lines()[0].contains("Full proposal")
            && screen.contains("the Session's words, read only")
            && screen.contains("nothing ran on the originals")
            && screen.contains("currency: EUR")
    });
    assert_eq!(rig.tree(), before, "reading grants no Save or Run");
    term.send("\r");
    term.wait_workspace_frame("reader Enter returns without sending the draft", |screen| {
        !screen.lines()[0].contains("Full proposal")
            && action_holds(screen)
            && screen
                .lines()
                .iter()
                .any(|row| row.contains("action ›") && row.contains(draft))
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
                && !action_prompt_shown(screen)
        });
        let words = question_region_rows(&term.screen)
            .expect("the current question region")
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        // The workspace always reads with a chosen intelligence, so the
        // Session's rule for a reply nothing reads is never attached; the
        // question's own reply line is the rule beside the input.
        assert!(
            words.contains("reply on the next line"),
            "the reply rule must be visible beside the input at {cols}x{rows}: {words}"
        );
        assert!(
            !words.contains("no intelligence reads this reply"),
            "a chosen intelligence is never said to be absent at {cols}x{rows}: {words}"
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
            && screen.contains("reply on the next line")
    });
    assert!(
        !term.screen.contains("no intelligence reads this reply"),
        "the whole words never deny the chosen intelligence\n{}",
        term.dump()
    );
    assert_eq!(rig.tree(), before, "question reading grants no effect");
    term.send("\r");
    term.wait_workspace_frame("reader returns the exact unsent question draft", |screen| {
        !screen.lines()[0].contains("Full question")
            && screen
                .lines()
                .iter()
                .any(|row| row.contains("reply ") && row.contains(draft))
            && !screen.contains("Save?")
            && !action_prompt_shown(screen)
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
        // The deterministic reader asked it; nothing was answered.
        rig.no_model_call(&format!("{tag}: the question asked and resized"));
    }
}

/// The scripted loopback reads a judge's closed choice where the route carries it: the vLLM
/// route sends the keys in its own JSON schema (`response_format`) and ends the turn with the
/// options; a JSON mode that folds the schema into the turn carries it on the turn's last line.
#[test]
fn the_scripted_loopback_reads_a_judge_choice_where_the_route_carries_it() {
    let keys = ["faithful", "unfaithful", "none"];
    let schema = serde_json::json!({"type": "object", "properties": {"choice": {"enum": keys}}});
    let turn = concat!(
        "STATE:\n{\"request\": \"aggregate-by-key\"}\n\n",
        "OPTIONS:\n- faithful: it does the request",
    );
    let format = serde_json::json!({
        "type": "json_schema",
        "json_schema": {"name": "choice", "schema": schema},
    });
    let routed = serde_json::json!({
        "messages": [{"role": "user", "content": turn}],
        "response_format": format,
    });
    let expected = Asked::Choice(keys.iter().map(|key| (*key).to_owned()).collect());
    assert_eq!(Asked::of(&routed), expected, "{routed:#}");
    let folded = format!("{turn}\n{schema}");
    let folded = serde_json::json!({"messages": [{"role": "user", "content": folded}]});
    assert_eq!(Asked::of(&folded), expected, "{folded:#}");
}

/// A small native decision exposes what Save would apply beside unsent input.
/// Reading or changing size cannot create that workflow or execute it.
#[test]
fn the_compact_proposal_keeps_its_identity_effects_and_unsent_input_in_view() {
    // The kept intelligence routes the typed value: `EUR` is the ANSWER.
    let rig = Rig::scripted("compact-decision", &["ANSWER"]);
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
        action_holds(screen) && screen.contains("aggregate-by-key.nika")
    });
    // Typed at the decision, the words wait in its own reply field.
    let draft = "hold this decision";
    term.send(draft);
    term.wait_workspace_frame("an unsent consent draft", |screen| {
        screen
            .lines()
            .iter()
            .any(|row| row.contains("action ›") && row.contains(draft))
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
            // The proposal a yes answers rides the card's border beside its
            // reader key, by where it lands, never by its identity's hex; the
            // status row keeps the scoped standing, the card the material facts.
            let footed = words.contains("aggregate-by-key.nika · F2");
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
                    .any(|row| row.contains("action ›") && row.contains(draft))
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
    rig.routed(&["EUR"], "a value, then the decision read at every size");
    rig.no_run("the decision read at every size");
}
