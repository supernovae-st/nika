// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a native reception proof fails when it cannot read its screen"
)]
//! Native witnesses for contextual object expansion in one workspace.
//! These exercise the real renderer on a PTY over the hermetic demo; they do
//! not qualify provider calls, question semantics or successful business effects.

#[path = "qa_support/child.rs"]
mod child;
mod qa_support;

use child::{BUSY, DONE, Release, SECOND, STOPPED};
use qa_support::{FREE, FREE_HINT, Term, assert_restored, exit_code};
use std::time::Duration;
use unicode_width::UnicodeWidthStr as _;

const EXPAND: &str = "[+] Expand · F4";
const RESTORE: &str = "[-] Restore · F4";
const F4: &str = "\x1bOS";
const F6: &str = "\x1b[17~";
const RULE: &str = "── ◌ this conversation";
const DRAFT: &str = "nika › keep this draft";

#[test]
fn qa_child_host() {
    child::host();
}

/// Read the actual composer row, excluding a submitted-line echo and any
/// appended character. The right pane may continue after its separator.
fn draft_is_exact(screen: &qa_support::vt::Screen) -> bool {
    draft_with_prompt(screen, DRAFT)
}

fn draft_with_prompt(screen: &qa_support::vt::Screen, prompt: &str) -> bool {
    screen.lines().iter().any(|row| {
        let Some((before, after)) = row.split_once(prompt) else {
            return false;
        };
        before
            .chars()
            .all(|c| c.is_whitespace() || matches!(c, '│' | '|'))
            && after
                .split(['│', '|'])
                .next()
                .is_some_and(|tail| tail.trim().is_empty())
    })
}

/// Read the pane edge after the actual draft, independently of the layout
/// calculation. A wide expanded object must move this edge to the left.
fn conversation_right(screen: &qa_support::vt::Screen) -> Option<usize> {
    screen.lines().iter().find_map(|row| {
        let start = row.find(DRAFT)? + DRAFT.len();
        let edge = start + row[start..].find('│')?;
        Some(row[..edge].width())
    })
}

fn mouse_restored(term: &Term) {
    for mode in [1000, 1002, 1003, 1006] {
        assert_eq!(
            term.screen.mode(mode),
            Some(false),
            "mode {mode}\n{}",
            term.dump()
        );
    }
}

fn open_workspace(term: &mut Term) {
    term.wait_prompt(FREE);
    term.send("\x14");
    term.send("keep this draft");
    term.wait_workspace_frame("the workspace with the exact unsent draft", |screen| {
        screen.on_alt() && draft_is_exact(screen) && screen.contains("this conversation")
    });
}

fn leave(term: &mut Term) {
    leave_with_mouse_mode(term, Some(false));
}

fn leave_with_mouse_mode(term: &mut Term, expected: Option<bool>) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
    for mode in [1000, 1002, 1003, 1006] {
        assert_eq!(
            term.screen.mode(mode),
            expected,
            "mode {mode}\n{}",
            term.dump()
        );
    }
}

/// The pointer uses the action that was actually painted, including its
/// Unicode cell width. No guessed offset from the former destination switch.
fn click_action(term: &mut Term, label: &str) {
    let rows = term.screen.lines();
    let y = rows
        .iter()
        .position(|row| row.contains(label))
        .expect("the visible contextual action");
    let row = &rows[y];
    let byte = row.rfind(label).expect("action on its row");
    let x = u16::try_from(row[..byte].width()).expect("terminal column") + 2;
    let y = u16::try_from(y).expect("terminal row") + 1;
    term.send(&format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m"));
}

/// A resize witness needs the newly painted action at this width: a screen
/// parser's resized old cells alone cannot establish a fresh native frame.
fn action_at_width(screen: &qa_support::vt::Screen, label: &str, cols: u16) -> bool {
    screen.lines().iter().any(|row| {
        row.find(label)
            .is_some_and(|byte| row[..byte].width() + label.width() == usize::from(cols))
    })
}

#[test]
fn contextual_expansion_is_one_action_and_preserves_the_same_draft() {
    let mut term = Term::proto_with(&[], 120, 40, &[("NO_COLOR", "1")]);
    open_workspace(&mut term);
    assert!(term.screen.contains(EXPAND), "{}", term.dump());
    assert!(!term.screen.contains("Workbench"), "{}", term.dump());
    assert!(!term.screen.contains("[Session]"), "{}", term.dump());
    assert!(
        term.screen.row_of(EXPAND).is_some_and(|row| row > 1),
        "the action belongs to the object rather than the project header"
    );
    assert_eq!(
        term.screen.row_of(RULE),
        None,
        "the normal side-by-side view"
    );
    let restored_edge = conversation_right(&term.screen).expect("conversation beside object");
    let title_row = term.screen.row_of("this conversation");

    term.send(F4);
    term.wait_workspace_frame("the expanded object and the same composer", |screen| {
        screen.contains(RESTORE)
            && screen.row_of(RULE).is_none()
            && conversation_right(screen).is_some_and(|edge| edge < restored_edge)
            && screen.row_of("this conversation") == title_row
            && draft_is_exact(screen)
    });
    assert!(
        !term.screen.contains(EXPAND),
        "one action, with its current effect"
    );
    assert!(!term.screen.seen("Which file holds the notes to digest?"));

    click_action(&mut term, RESTORE);
    term.wait_workspace_frame("restored by its contextual action", |screen| {
        screen.contains(EXPAND)
            && conversation_right(screen) == Some(restored_edge)
            && screen.row_of(RULE).is_none()
            && draft_is_exact(screen)
    });
    click_action(&mut term, EXPAND);
    term.wait_workspace_frame("expanded by its contextual action", |screen| {
        screen.contains(RESTORE)
            && conversation_right(screen).is_some_and(|edge| edge < restored_edge)
            && screen.row_of(RULE).is_none()
            && draft_is_exact(screen)
    });
    term.send(F4);
    term.wait_workspace_frame("restored by the compatible keyboard route", |screen| {
        screen.contains(EXPAND)
            && conversation_right(screen) == Some(restored_edge)
            && screen.row_of(RULE).is_none()
            && draft_is_exact(screen)
    });
    assert!(term.screen.hues().is_empty(), "{:?}", term.screen.hues());
    assert_eq!(
        term.screen.beyond(),
        0,
        "no cells addressed past the screen"
    );
    assert!(!term.screen.seen(qa_support::QUESTION));
    leave(&mut term);
}

#[test]
fn contextual_expansion_survives_native_sizes_and_a_synchronized_fallback() {
    let mut term = Term::proto_with(&[], 120, 40, &[("NO_COLOR", "1")]);
    open_workspace(&mut term);
    term.wait_text(EXPAND);
    for (cols, rows) in [(80, 24), (120, 40), (180, 48), (120, 40)] {
        term.resize(cols, rows);
        term.wait_workspace_frame("the restored workspace at the new native size", |screen| {
            screen.size() == (usize::from(cols), usize::from(rows))
                && action_at_width(screen, EXPAND, cols)
                && draft_is_exact(screen)
                && if cols < 120 {
                    screen.lines()[0].contains("[Conversation]")
                } else {
                    !screen.lines()[0].contains("[Conversation]")
                }
        });
        let restored_rule = term.screen.row_of(RULE);
        let restored_edge = conversation_right(&term.screen);
        let restored_title = term.screen.row_of("this conversation");
        term.send(F4);
        term.wait_workspace_frame("the expanded object at the same native size", |screen| {
            action_at_width(screen, RESTORE, cols)
                && if cols < 100 {
                    screen.row_of(RULE) > restored_rule
                } else {
                    screen.row_of(RULE).is_none()
                        && conversation_right(screen)
                            .zip(restored_edge)
                            .is_some_and(|(expanded, restored)| expanded < restored)
                        && screen.row_of("this conversation") == restored_title
                }
                && draft_is_exact(screen)
        });
        if cols < 100 {
            assert!(term.screen.row_of(RULE) > restored_rule, "{}", term.dump());
        }
        let prompt = term.screen.row_of(DRAFT).expect("the composer");
        assert!(prompt >= usize::from(rows) / 2, "{}", term.dump());
        click_action(&mut term, RESTORE);
        term.wait_workspace_frame("the exact restored workspace", |screen| {
            action_at_width(screen, EXPAND, cols)
                && draft_is_exact(screen)
                && screen.row_of(RULE) == restored_rule
                && conversation_right(screen) == restored_edge
        });
        assert_eq!(term.screen.row_of(RULE), restored_rule);
    }

    term.send(F4);
    term.wait_text(RESTORE);
    term.resize(80, 24);
    term.wait_workspace_frame("the expanded folded workspace", |screen| {
        screen.size() == (80, 24)
            && action_at_width(screen, RESTORE, 80)
            && screen.lines()[0].contains("[Conversation]")
            && draft_is_exact(screen)
    });
    term.send(F6);
    term.wait_workspace_frame("the object holds keyboard focus", |screen| {
        screen.lines()[0].contains("[Object]") && screen.contains(RESTORE)
    });

    term.resize(59, 15);
    term.wait_workspace_frame("a fresh fallback frame before dependent input", |screen| {
        screen.size() == (59, 15)
            && screen.contains(FREE_HINT)
            && draft_is_exact(screen)
            && screen.contains("60x16")
            && !screen.contains("F6 panel")
            && !screen.contains(RESTORE)
    });
    term.send(F4);
    term.send("!");
    term.wait_workspace_frame("F4 consumed before the terminal grows", |screen| {
        screen
            .lines()
            .iter()
            .any(|row| row == "nika › keep this draft!")
    });
    term.send("\x7f");
    term.wait_workspace_frame("the original draft restored byte for byte", |screen| {
        screen.lines().iter().any(|row| row == DRAFT)
    });
    term.resize(80, 24);
    term.wait_workspace_frame("expansion and object focus return together", |screen| {
        screen.size() == (80, 24)
            && screen.lines()[0].contains("[Object]")
            && action_at_width(screen, RESTORE, 80)
            && draft_is_exact(screen)
    });
    assert!(term.screen.hues().is_empty(), "{:?}", term.screen.hues());
    assert!(!term.screen.seen("Which file holds the notes to digest?"));
    leave(&mut term);
}

/// Concrete folded-region access changes focus only. The project overlay
/// withdraws the object action; returning restores it and the same composer.
/// The ASCII witness covers the new chrome, not all legacy transcript glyphs.
#[test]
fn folded_region_access_and_ascii_chrome_keep_the_draft_and_object() {
    for ascii in [false, true] {
        let mode = if ascii {
            "big:0:workspace-ascii"
        } else {
            "big:0:workspace"
        };
        let expand = if ascii { "[+] Expand - F4" } else { EXPAND };
        let restore = if ascii { "[-] Restore - F4" } else { RESTORE };
        let prompt = if ascii { "nika >" } else { FREE };
        let draft = if ascii {
            "nika > keep this draft"
        } else {
            DRAFT
        };
        let mut term = child::spawn(mode, None, 80, 24);
        term.wait_prompt(prompt);
        term.send("keep this draft");
        term.wait_workspace_frame("folded workspace and its unsent draft", |screen| {
            screen.lines()[0].ends_with("Project [Conversation] Object")
                && action_at_width(screen, expand, 80)
                && draft_with_prompt(screen, draft)
        });
        let original_title =
            term.screen.lines()[term.screen.row_of(expand).expect("object title")].clone();
        click_action(&mut term, "Project");
        term.wait_workspace_frame("the folded project holds the keys", |screen| {
            screen.lines()[0].ends_with("[Project] Conversation Object")
                && !screen.contains(expand)
                && draft_with_prompt(screen, draft)
        });
        click_action(&mut term, "Object");
        term.wait_workspace_frame("the same object is reachable again", |screen| {
            screen.lines()[0].ends_with("Project Conversation [Object]")
                && action_at_width(screen, expand, 80)
                && draft_with_prompt(screen, draft)
        });
        assert_eq!(
            term.screen.lines()[term.screen.row_of(expand).expect("same object title")],
            original_title
        );
        click_action(&mut term, expand);
        term.wait_workspace_frame("the same object expands in either glyph column", |screen| {
            screen.lines()[0].ends_with("Project Conversation [Object]")
                && action_at_width(screen, restore, 80)
                && draft_with_prompt(screen, draft)
        });
        if ascii {
            assert!(term.screen.lines()[0].is_ascii());
            let y = term.screen.row_of(restore).expect("the action row");
            assert!(term.screen.lines()[y].is_ascii());
        }
        click_action(&mut term, "Conversation");
        term.wait_workspace_frame("the conversation recovers the keys", |screen| {
            screen.lines()[0].ends_with("Project [Conversation] Object")
                && action_at_width(screen, restore, 80)
                && draft_with_prompt(screen, draft)
        });
        assert!(
            !term.screen.seen(DONE),
            "a draft was submitted\n{}",
            term.dump()
        );
        assert!(!term.screen.seen(SECOND));
        assert!(term.screen.hues().is_empty());
        assert_eq!(term.screen.beyond(), 0);
        leave(&mut term);
    }
}

/// View-only keyboard actions keep the confirmation of a pending Stop.
#[test]
fn pending_stop_feedback_survives_palette_and_scrolling() {
    for presentation in ["inline", "focus", "workspace"] {
        let release = Release::new(&format!("stop-keys-{presentation}"));
        let mode = if presentation == "workspace" {
            "slow-stop:100:workspace"
        } else {
            "slow-stop:100"
        };
        let mut term = child::spawn(mode, Some(release.path()), 120, 40);
        term.wait_text(FREE);
        if presentation == "focus" {
            term.send("\x14");
            term.wait_workspace_frame("focus presentation before the turn", |screen| {
                screen.on_alt() && screen.contains(FREE)
            });
        }
        term.send("work\r");
        term.wait_text(BUSY);
        term.send("keep this draft");
        term.wait_text(DRAFT);
        term.send("\x03");
        term.wait_text("stopping the preparation");
        term.send("\x0f");
        term.wait_text("commands ›");
        term.send("\x0f");
        term.wait_workspace_frame("palette closed after Stop", |screen| {
            !screen.contains("commands ›") && draft_is_exact(screen)
        });
        assert!(
            term.screen.contains("stopping the preparation"),
            "{}",
            term.dump()
        );
        term.send("\x1b[5~");
        term.settle(Duration::from_millis(150));
        term.send("\x1b[F");
        term.wait_workspace_frame("returned to live after Stop", |screen| {
            draft_is_exact(screen)
        });
        assert!(
            term.screen.contains("stopping the preparation"),
            "{}",
            term.dump()
        );
        assert!(
            term.screen.contains("Ctrl+C again leaves now"),
            "{}",
            term.dump()
        );
        assert!(
            !term.screen.seen(STOPPED),
            "request is not settlement\n{}",
            term.dump()
        );
        release.open();
        term.wait_text(STOPPED);
        assert!(!term.screen.seen("qa Stop observed · calls=2"));
        assert!(!term.screen.seen(DONE));
        assert!(!term.screen.seen(SECOND));
        assert!(draft_is_exact(&term.screen), "{}", term.dump());
        // Inline never enables mouse capture; full-screen modes restore it.
        leave_with_mouse_mode(&mut term, (presentation != "inline").then_some(false));
    }
}

/// A palette or folded workspace must not intercept Stop. The controlled
/// preparation settles only after release: the first press requests Stop,
/// preserves the draft and stays live; the observed end proves one callback.
/// This qualifies the native preparation route, not provider or Run stopping.
fn shrink_busy_workspace(term: &mut Term) {
    term.resize(80, 24);
    term.wait_workspace_frame("the freshly folded expanded frame", |screen| {
        action_at_width(screen, RESTORE, 80)
            && screen.lines()[0].contains("[Conversation]")
            && draft_is_exact(screen)
    });
    term.send(F6);
    term.wait_workspace_frame("object focus before shrinking", |screen| {
        screen.lines()[0].contains("[Object]") && action_at_width(screen, RESTORE, 80)
    });
    let mark = term.mark();
    term.resize(59, 15);
    term.spin_until_bytes(mark, b"\x1b[2J");
    term.wait_workspace_frame("the fresh small-terminal busy frame", |screen| {
        screen.size() == (59, 15)
            && screen.contains(BUSY)
            && screen.lines().iter().any(|row| row == DRAFT)
            && !screen.contains(RESTORE)
    });
}

/// Stop consumes its press; leaving after settlement takes two new presses.
fn leave_after_settlement(term: &mut Term) {
    term.send("\x03");
    term.wait_text("any key stays");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
    mouse_restored(term);
}

#[test]
fn stop_is_reachable_from_an_expanded_palette_and_the_small_terminal() {
    for small in [false, true] {
        let release = Release::new(if small {
            "contextual-stop-small"
        } else {
            "contextual-stop-palette"
        });
        let mut term = child::spawn("slow-stop:0:workspace", Some(release.path()), 120, 40);
        term.wait_text(FREE);
        term.send("work\r");
        term.wait_text(BUSY);
        term.send("keep this draft");
        term.wait_text(DRAFT);
        term.send(F4);
        term.wait_text(RESTORE);
        if small {
            shrink_busy_workspace(&mut term);
        } else {
            term.send("\x0f");
            term.wait_text("commands ›");
        }
        term.send("\x03");
        term.wait_text("stopping the preparation");
        term.settle(Duration::from_millis(400));
        assert!(
            term.screen.contains("stopping the preparation"),
            "{}",
            term.dump()
        );
        assert!(
            term.screen.contains("Ctrl+C again leaves now"),
            "{}",
            term.dump()
        );
        assert!(
            !term.screen.seen(STOPPED),
            "request is not settlement\n{}",
            term.dump()
        );
        assert!(
            !term.screen.seen(DONE),
            "a draft was submitted\n{}",
            term.dump()
        );
        assert!(!term.screen.seen(SECOND), "{}", term.dump());
        assert!(draft_is_exact(&term.screen), "{}", term.dump());
        if !small {
            click_action(&mut term, RESTORE);
            term.wait_workspace_frame("a view-only click after Stop", |screen| {
                screen.contains(EXPAND) && draft_is_exact(screen)
            });
            assert!(
                term.screen.contains("stopping the preparation"),
                "{}",
                term.dump()
            );
            assert!(
                term.screen.contains("Ctrl+C again leaves now"),
                "{}",
                term.dump()
            );
            assert!(
                !term.screen.contains("Preparing: Ctrl+C requests Stop"),
                "{}",
                term.dump()
            );
        }
        release.open();
        term.wait_workspace_frame("Stop observed once, with the same draft", |screen| {
            screen.seen(STOPPED) && draft_is_exact(screen)
        });
        assert!(!term.screen.seen("qa Stop observed · calls=2"));
        assert!(
            !term.screen.seen(DONE),
            "a draft was submitted\n{}",
            term.dump()
        );
        assert!(!term.screen.seen(SECOND));
        assert_eq!(
            term.screen
                .lines()
                .iter()
                .filter(|row| row.contains("qa Stop observed"))
                .count(),
            1
        );
        if small {
            term.resize(80, 24);
            term.wait_workspace_frame("expansion and object focus recover after Stop", |screen| {
                screen.lines()[0].contains("[Object]")
                    && action_at_width(screen, RESTORE, 80)
                    && draft_is_exact(screen)
            });
        }
        leave_after_settlement(&mut term);
    }
}

/// A full-frame reader owns pointer input as well as keys. Clicking where
/// a hidden object action or folded region was must not alter the workspace.
#[test]
fn the_diagnostic_does_not_click_through_to_the_workspace() {
    for (cols, rows) in [(120, 40), (80, 24)] {
        let release = Release::new(&format!("diagnostic-pointer-{cols}"));
        let mut term = child::spawn(
            "slow-diagnostic:0:workspace",
            Some(release.path()),
            cols,
            rows,
        );
        term.wait_text(FREE);
        term.send("keep this draft");
        term.wait_workspace_frame("the workspace before opening its diagnostic", |screen| {
            screen.contains(EXPAND) && draft_is_exact(screen)
        });
        let mut cells = Vec::new();
        for label in [EXPAND, "Object"] {
            if let Some((y, row)) = term
                .screen
                .lines()
                .iter()
                .enumerate()
                .find(|(_, row)| row.contains(label))
            {
                let byte = row.rfind(label).expect("visible action");
                cells.push((row[..byte].width() + 2, y + 1));
            }
        }
        let heading = term.screen.lines()[0].clone();
        term.send("\x1bOQ");
        term.wait_workspace_frame("the full-frame read-only diagnostic", |screen| {
            screen.lines()[0].contains("Full diagnostic") && screen.contains("ADMISSION_UNTRUSTED")
        });
        for (x, y) in cells {
            term.send(&format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m"));
        }
        term.send("\x1b");
        term.wait_workspace_frame("the workspace returns after the reader closes", |screen| {
            !screen.lines()[0].contains("Full diagnostic") && draft_is_exact(screen)
        });
        assert!(
            term.screen.contains(EXPAND),
            "a hidden click expanded the object\n{}",
            term.dump()
        );
        assert!(!term.screen.contains(RESTORE), "{}", term.dump());
        assert_eq!(
            term.screen.lines()[0],
            heading,
            "a hidden click moved focus"
        );
        assert!(!term.screen.seen(SECOND));
        leave(&mut term);
    }
}

/// Resizing or expanding the object cannot submit the partially written
/// answer, replace its question, or turn that answer into Save consent.
#[test]
fn a_pending_question_and_its_answer_survive_expansion_palette_and_resize() {
    // The expanded conversation wraps the question. Its actual words and
    // requirement key must survive; a single-line search would reject wrapping.
    let question_visible = |screen: &qa_support::vt::Screen| {
        [
            "Which file holds the notes",
            "digest?",
            "(const.source_path)",
        ]
        .into_iter()
        .all(|part| screen.contains(part))
    };
    let mut term = Term::proto_with(&[], 120, 40, &[("NO_COLOR", "1")]);
    term.wait_prompt(FREE);
    term.send("\x14");
    term.wait_workspace_frame("workspace before the question", |screen| {
        screen.on_alt() && screen.contains("this conversation") && screen.contains(FREE)
    });
    term.send("digest my monday notes\r");
    term.wait_workspace_frame("the question actually waits for an answer", |screen| {
        question_visible(screen) && screen.contains(qa_support::REPLY)
    });
    term.send("./notes/lundi.md");
    let answer = "reply › ./notes/lundi.md";
    term.wait_workspace_frame("the unsent answer belongs to the question", |screen| {
        draft_with_prompt(screen, answer) && question_visible(screen)
    });
    term.send("\x0f");
    term.wait_workspace_frame("commands temporarily over the pending answer", |screen| {
        screen.contains("commands ›")
    });
    term.send(F4);
    term.wait_workspace_frame("expansion returns to the same pending answer", |screen| {
        screen.contains(RESTORE)
            && !screen.contains("commands ›")
            && draft_with_prompt(screen, answer)
            && question_visible(screen)
    });
    for (cols, rows) in [(80, 24), (180, 48), (120, 40)] {
        term.resize(cols, rows);
        term.wait_workspace_frame("question and answer after a native resize", |screen| {
            screen.size() == (usize::from(cols), usize::from(rows))
                && screen.contains(RESTORE)
                && question_visible(screen)
                && draft_with_prompt(screen, answer)
        });
        assert!(
            !term.screen.seen(qa_support::PROPOSAL),
            "an answer was submitted\n{}",
            term.dump()
        );
        assert!(!term.screen.seen(qa_support::SAVED));
        assert!(!term.screen.seen(qa_support::RESULT));
    }
    term.send("\r");
    term.wait_workspace_frame(
        "one explicit answer produces a review, not a save",
        |screen| screen.contains(qa_support::PROPOSAL) && screen.contains(qa_support::APPLY),
    );
    assert!(!term.screen.seen(qa_support::SAVED));
    assert!(!term.screen.seen(qa_support::RESULT));
    leave(&mut term);
}
