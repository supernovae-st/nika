// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bare `nika` on a terminal — the native session (ADR-125 · One Door ·
//! wave 4): the first run asks the human how Nika should think with them
//! (an AI app they already have · an API · a local engine · none), keeps
//! the answer beside the other user files, and opens one grounded
//! conversation over the installed engine ([`nika_session`]). No
//! temporary workflow, no trace for a chat turn, no hidden shell.
// The session owns a live terminal, like `run`: the prompt and the
// replies go to that terminal directly.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

use std::io::{BufRead, Write};
use std::path::PathBuf;

use nika_session::intelligence::{IntelligenceKind, UserIntelligencePreference};
use nika_session::reasoner::{NoReasoner, ProviderReasoner, SessionReasoner};
use nika_session::{IntelligenceCensus, ResolvedSessionIntelligence, SessionRuntime, TurnOutcome};

use crate::Theme;
use crate::verbs::exit;

/// The first run: the census, the first screen, one answer (three tries),
/// persisted under the home when one exists.
fn first_run<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    census: &IntelligenceCensus,
    home: Option<&std::path::Path>,
) -> std::io::Result<Option<UserIntelligencePreference>> {
    write!(output, "{}", census.first_screen())?;
    for _ in 0..3 {
        write!(output, "\n› ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        match census.choose(line.trim()) {
            Ok(pref) => {
                if let Some(home) = home
                    && let Err(e) = pref.save(home)
                {
                    writeln!(
                        output,
                        "  (the choice could not be saved under ~/.nika: {e} · it holds for this session)"
                    )?;
                }
                return Ok(Some(pref));
            }
            Err(why) => writeln!(output, "  {why}")?,
        }
    }
    Ok(None)
}

/// The reasoner for a resolved choice — the seat, the provider, or none.
fn reasoner_for(resolved: &ResolvedSessionIntelligence) -> Box<dyn SessionReasoner> {
    match &resolved.kind {
        #[cfg(feature = "access-harness")]
        IntelligenceKind::Harness { seat } => {
            Box::new(nika_session::reasoner::HarnessReasoner { seat: seat.clone() })
        }
        #[cfg(not(feature = "access-harness"))]
        IntelligenceKind::Harness { .. } => Box::new(NoReasoner),
        IntelligenceKind::Api { provider } => Box::new(ProviderReasoner {
            model: resolved
                .model
                .clone()
                .unwrap_or_else(|| default_model(provider)),
            label: format!("{provider} API"),
        }),
        IntelligenceKind::Local { provider } => Box::new(ProviderReasoner {
            model: resolved
                .model
                .clone()
                .unwrap_or_else(|| default_model(provider)),
            label: format!("{provider} · local"),
        }),
        _ => Box::new(NoReasoner),
    }
}

/// The provider's first cataloged model when the human named none.
fn default_model(provider: &str) -> String {
    nika_catalog::all_providers()
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(provider))
        .map_or_else(
            || format!("{provider}/default"),
            |p| format!("{provider}/{}", p.default_model),
        )
}

/// The session loop over any reader and writer (the tests drive it with
/// a cursor; `run` drives it with the terminal).
fn drive<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    census: &IntelligenceCensus,
    home: Option<&std::path::Path>,
    cwd: &std::path::Path,
) -> std::io::Result<u8> {
    let kept = home.and_then(UserIntelligencePreference::load);
    let pref = if let Some(pref) = kept {
        pref
    } else if let Some(pref) = first_run(input, output, census, home)? {
        pref
    } else {
        writeln!(
            output,
            "no choice made · `nika` asks again next time; the verbs stay: nika try · nika new · nika check · nika run"
        )?;
        return Ok(exit::OK);
    };
    let mut session =
        SessionRuntime::open_with(cwd, census.clone(), &pref, home, Box::new(reasoner_for));
    writeln!(output, "{}", session.banner())?;
    let mut asking = false;
    loop {
        write!(output, "\n{}", if asking { "› " } else { "nika › " })?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(exit::OK);
        }
        let outcome = if asking {
            asking = false;
            session.choose(line.trim())
        } else {
            session.turn(&line)
        };
        match outcome {
            TurnOutcome::Quit => return Ok(exit::OK),
            TurnOutcome::Reply(text) | TurnOutcome::Facts(text) | TurnOutcome::Help(text) => {
                if !text.is_empty() {
                    writeln!(output, "{text}")?;
                }
            }
            TurnOutcome::Ask(screen) => {
                asking = true;
                writeln!(output, "{screen}")?;
            }
            TurnOutcome::Refusal(text) => writeln!(output, "✖ {text}")?,
            _ => {}
        }
    }
}

/// Open the native session on this terminal.
#[must_use]
pub fn run(_theme: Theme) -> u8 {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut output = std::io::stdout();
    let census = IntelligenceCensus::take();
    let home = nika_cli_host::probe::home_dir();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match drive(&mut input, &mut output, &census, home.as_deref(), &cwd) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("nika: session I/O failed: {error}");
            exit::ENV
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// The first run asks, keeps the answer under the home, opens the
    /// session, answers a fact without any model, and closes on `/quit`.
    /// Nothing is written into the project.
    #[test]
    fn the_first_run_asks_once_then_the_facts_answer() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        std::fs::write(
            project.path().join("hello.nika.yaml"),
            "nika: hello\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("workflow");
        let census = IntelligenceCensus::empty();
        let mut input = Cursor::new(b"9\n4\nwhat workflows are here?\n/quit\n".to_vec());
        let mut output = Vec::new();
        let code = drive(
            &mut input,
            &mut output,
            &census,
            Some(home.path()),
            project.path(),
        )
        .expect("io");
        assert_eq!(code, exit::OK);
        let text = String::from_utf8(output).expect("utf8");
        assert!(
            text.contains("Choose which AI answers your questions here"),
            "{text}"
        );
        assert!(text.contains("`9` is not a choice"), "{text}");
        assert!(text.contains("nika · session"), "{text}");
        assert!(text.contains("no conversational AI"), "{text}");
        assert!(text.contains("hello.nika.yaml"), "the fact answers: {text}");
        assert!(
            UserIntelligencePreference::load(home.path()).is_some(),
            "the choice holds"
        );
        let entries: Vec<_> = std::fs::read_dir(project.path())
            .expect("dir")
            .flatten()
            .collect();
        assert_eq!(entries.len(), 1, "nothing written into the project");
    }

    /// A second run never asks again: the saved choice opens the session.
    #[test]
    fn a_second_run_never_asks_again() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        UserIntelligencePreference::new(IntelligenceKind::None, None)
            .save(home.path())
            .expect("saved");
        let mut input = Cursor::new(b"/help\n".to_vec());
        let mut output = Vec::new();
        let code = drive(
            &mut input,
            &mut output,
            &IntelligenceCensus::empty(),
            Some(home.path()),
            project.path(),
        )
        .expect("io");
        assert_eq!(code, exit::OK, "EOF closes the session cleanly");
        let text = String::from_utf8(output).expect("utf8");
        assert!(!text.contains("Choose which AI"), "{text}");
        assert!(text.contains("/intelligence"), "the help card: {text}");
    }

    /// `/intelligence` asks again in-session and the next line answers.
    #[test]
    fn the_intelligence_is_rechosen_on_the_next_line() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        UserIntelligencePreference::new(IntelligenceKind::None, None)
            .save(home.path())
            .expect("saved");
        let mut input = Cursor::new(b"/intelligence\n4\n/quit\n".to_vec());
        let mut output = Vec::new();
        let code = drive(
            &mut input,
            &mut output,
            &IntelligenceCensus::empty(),
            Some(home.path()),
            project.path(),
        )
        .expect("io");
        assert_eq!(code, exit::OK);
        let text = String::from_utf8(output).expect("utf8");
        assert!(
            text.contains("Choose which AI answers"),
            "asks again: {text}"
        );
        assert!(text.contains("kept"), "the new choice is kept: {text}");
    }
}
