// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The terminal ask (ADR-099 · "interactively it asks") — a paused
//! fold-lane run with a human present continues in-process: the gate's
//! question prints on stderr, the answer reads from stdin, and the
//! driver resumes over the just-written trace (the SAME plan-fold +
//! F-P4 ticket path a manual `--resume --answer` takes — one behavior,
//! attested the same way).
//!
//! The ask is sugar over the durable pause, never a third mechanism:
//! Ctrl-D (or three unparseable tries) leaves the paused trace + its
//! taught resume line as the escape — nothing is lost by walking away.

use std::io::{BufRead, IsTerminal as _, Write};

use nika_runtime::WorkflowPause;

use crate::{TaskState, Theme};

/// One ask's outcome.
pub(super) enum Asked {
    /// The answer, in the `--answer` VALUE spelling (`true`/`false` for
    /// confirm · the chosen element for choice · the raw line for input)
    /// — fed through `parse_answers` so typing matches the CLI flag.
    Answer(String),
    /// No usable answer (EOF · three unparseable tries) — the durable
    /// pause stands, its resume line already taught.
    Escape,
}

/// Is a human reachable? The question rides stderr and the answer rides
/// stdin — BOTH must be terminals (stdout may be piped: `> out.json`
/// with a human present still deserves the ask).
pub(super) fn tty_present() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Ask the paused gate's question on the real terminal seams.
pub(super) fn ask_on_tty(pause: &WorkflowPause, theme: Theme) -> Asked {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = std::io::stderr().lock();
    ask(pause, &mut input, &mut out, theme)
}

/// The pure ask over injected seams (unit-pinned): render the gate's
/// question in its mode's own answer shape, read one line, parse.
/// `confirm` follows the `[y/N]` convention (empty = No — deny by
/// default, never a silent approve); `choice` takes the value or its
/// 1-based index; `input` takes the line verbatim (empty allowed —
/// the mode's contract). Unparseable confirm/choice re-asks twice,
/// then escapes to the durable pause.
pub(super) fn ask(
    pause: &WorkflowPause,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    theme: Theme,
) -> Asked {
    let glyph = theme.glyph(TaskState::Paused, 0);
    let task = &pause.task;
    let message = pause.message.as_deref().unwrap_or("answer required");
    for _ in 0..3 {
        let shape = match pause.mode.as_str() {
            "confirm" => "  [y/N] ".to_owned(),
            "choice" => {
                use std::fmt::Write as _;
                let mut list = String::new();
                for (n, choice) in pause.choices.iter().enumerate() {
                    // write! to a String is infallible.
                    let _ = write!(list, "\n      {} · {choice}", n + 1);
                }
                format!("{list}\n    pick a number or the value: ")
            }
            _ => "\n    > ".to_owned(),
        };
        let _ = write!(out, "\n  {glyph}{task} · {message}{shape}");
        let _ = out.flush();
        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => return Asked::Escape, // EOF — walk away, the pause stands
            Ok(_) => {}
        }
        let line = line.trim();
        match pause.mode.as_str() {
            "confirm" => match line.to_ascii_lowercase().as_str() {
                "" | "n" | "no" | "false" => return Asked::Answer("false".to_owned()),
                "y" | "yes" | "true" => return Asked::Answer("true".to_owned()),
                _ => {
                    let _ = writeln!(out, "    answer y or n (Ctrl-D leaves the run paused)");
                }
            },
            "choice" => {
                if let Some(value) = pick_choice(&pause.choices, line) {
                    return Asked::Answer(value);
                }
                let _ = writeln!(
                    out,
                    "    pick 1-{} or type a listed value (Ctrl-D leaves the run paused)",
                    pause.choices.len()
                );
            }
            _ => return Asked::Answer(line.to_owned()),
        }
    }
    Asked::Escape
}

/// Resolve a `choice` reply: the exact value wins, else a 1-based index
/// into the shown list. Anything else is no pick.
fn pick_choice(choices: &[String], line: &str) -> Option<String> {
    if choices.iter().any(|c| c == line) {
        return Some(line.to_owned());
    }
    let n: usize = line.parse().ok()?;
    (1..=choices.len())
        .contains(&n)
        .then(|| choices[n - 1].clone())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);

    fn confirm() -> WorkflowPause {
        WorkflowPause::new(
            "approve".into(),
            "confirm".into(),
            Some("ship?".into()),
            vec![],
        )
    }

    fn run_ask(pause: &WorkflowPause, typed: &str) -> (Asked, String) {
        let mut input = typed.as_bytes();
        let mut out = Vec::new();
        let asked = ask(pause, &mut input, &mut out, PLAIN);
        (asked, String::from_utf8(out).expect("utf8"))
    }

    #[test]
    fn confirm_speaks_y_slash_capital_n_and_parses_the_family() {
        for (typed, expected) in [
            ("y\n", "true"),
            ("yes\n", "true"),
            ("TRUE\n", "true"),
            ("n\n", "false"),
            ("no\n", "false"),
            // Empty = No — the [y/N] capital: deny by default, never a
            // silent approve on a stray Enter.
            ("\n", "false"),
        ] {
            let (asked, screen) = run_ask(&confirm(), typed);
            let Asked::Answer(value) = asked else {
                panic!("`{typed:?}` answers, never escapes");
            };
            assert_eq!(value, expected, "typed {typed:?}");
            assert!(screen.contains("approve · ship?"), "{screen}");
            assert!(screen.contains("[y/N]"), "{screen}");
        }
    }

    #[test]
    fn eof_and_three_strikes_escape_to_the_durable_pause() {
        // Ctrl-D straight away — the pause stands.
        let (asked, _) = run_ask(&confirm(), "");
        assert!(matches!(asked, Asked::Escape));
        // Three unparseable tries — never an invented answer.
        let (asked, screen) = run_ask(&confirm(), "peut-être\nbof\n42\n");
        assert!(matches!(asked, Asked::Escape));
        assert!(
            screen.matches("answer y or n").count() == 3,
            "each miss teaches: {screen}"
        );
    }

    #[test]
    fn choice_takes_the_value_or_its_index_and_input_rides_verbatim() {
        let choice = WorkflowPause::new(
            "pick".into(),
            "choice".into(),
            Some("which title?".into()),
            vec!["alpha".into(), "beta".into()],
        );
        let (asked, screen) = run_ask(&choice, "2\n");
        assert!(
            matches!(asked, Asked::Answer(v) if v == "beta"),
            "index picks"
        );
        assert!(
            screen.contains("1 · alpha") && screen.contains("2 · beta"),
            "{screen}"
        );
        let (asked, _) = run_ask(&choice, "alpha\n");
        assert!(
            matches!(asked, Asked::Answer(v) if v == "alpha"),
            "value picks"
        );
        let (asked, _) = run_ask(&choice, "3\ngamma\nzz\n");
        assert!(matches!(asked, Asked::Escape), "out-of-range never invents");

        let input = WorkflowPause::new(
            "otp".into(),
            "input".into(),
            Some("paste it".into()),
            vec![],
        );
        let (asked, _) = run_ask(&input, "s3cret line\n");
        assert!(matches!(asked, Asked::Answer(v) if v == "s3cret line"));
        // Empty input is a VALID answer (the mode's contract), never a retry.
        let (asked, _) = run_ask(&input, "\n");
        assert!(matches!(asked, Asked::Answer(v) if v.is_empty()));
    }
}
