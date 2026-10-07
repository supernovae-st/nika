// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-tui-proto` — the UX-1 renderer proof (ADR-139).
//!
//! Runs the shell over the canned conversation of [`Script::demo`] in one
//! presentation, on a real terminal, so both presentations are judged on the
//! same fixture and the terminal lifecycle is proven from a PTY: the normal
//! exit, `Ctrl+C`, a panic (`--panic-after N`) and `SIGTERM` all restore the
//! terminal. It is not `nika`: nothing here reaches the session runtime.
//! `--demo-pace MS` holds each scripted turn busy that long (0, the default,
//! answers at once), so a proof can type while Nika works. `--demo-diagnostic`
//! supplies the Session's typed untrusted-knowledge refusal as a display
//! fixture; no Session or provider is called by this prototype.
//!
//! ```text
//! nika-tui-proto [--focus] [--color] [--demo-pace MS] [--panic-after N] [--exit-after N]
//! ```
//!
//! Exit codes: `0` closed · `130` two `Ctrl+C` · `143` `SIGTERM` · `2` the
//! arguments or the terminal refused.

use std::io::Write as _;
use std::process::ExitCode;
use std::sync::mpsc::Sender;
use std::time::Duration;

use nika_tui::app::{self, Options};
use nika_tui::model::{Beat, Committed, Conversation, Handoff, Kind, Presentation, Script, Turn};

fn usage() -> &'static str {
    "usage: nika-tui-proto [--focus] [--color] [--demo-pace MS] [--demo-diagnostic] [--panic-after N] [--exit-after N]"
}

/// The shell's options and the demo's own pace.
struct Flags {
    options: Options,
    pace: Duration,
    diagnostic: bool,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Flags, String> {
    let mut options = Options::new(Presentation::Inline);
    let mut pace = Duration::ZERO;
    let mut diagnostic = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--focus" => options.presentation = Presentation::Focus,
            "--inline" => options.presentation = Presentation::Inline,
            "--color" => options.color = true,
            "--demo-diagnostic" => diagnostic = true,
            "--panic-after" | "--exit-after" | "--demo-pace" => {
                let count = args
                    .next()
                    .ok_or_else(|| format!("{arg} needs a count"))?
                    .parse::<usize>()
                    .map_err(|e| format!("{arg}: {e}"))?;
                match arg.as_str() {
                    "--panic-after" => options.panic_after = Some(count),
                    "--exit-after" => options.exit_after = Some(count),
                    _ => pace = Duration::from_millis(u64::try_from(count).unwrap_or(u64::MAX)),
                }
            }
            "-h" | "--help" => return Err(usage().to_owned()),
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
    }
    Ok(Flags {
        options,
        pace,
        diagnostic,
    })
}

/// Typed evidence for the display fixture, refused before reading a release.
fn diagnostic_words() -> Option<String> {
    use nika_cli_host::compile::config::AuthoringSettings;
    use nika_session::authoring::{AuthoringContext, AuthoringError};
    let mut env = AuthoringSettings::none();
    env.knowledge = Some(std::path::PathBuf::from("/srv/foundry/release-r3"));
    let context = AuthoringContext::from_settings(&AuthoringSettings::none(), &env);
    context.refusal().cloned().map(|cause| format!(
        "{} · nothing was sent to the authoring model, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again",
        AuthoringError::Context(cause)
    ))
}

/// The demo conversation, each turn held busy for `pace` while the shell
/// shows it working.
struct Demo {
    script: Script,
    pace: Duration,
    refusal: Option<String>,
}

impl Conversation for Demo {
    fn open(&mut self) -> Vec<Beat> {
        let mut beats = self.script.open();
        if let Some(words) = self.refusal.take() {
            beats.push(Beat::Say(Committed::new(Kind::Refusal, words)));
        }
        beats
    }

    fn submit(&mut self, line: &str) -> Turn {
        Conversation::submit(&mut self.script, line)
    }

    fn submit_with(&mut self, line: &str, busy: &Sender<String>) -> Turn {
        if !self.pace.is_zero() {
            let _ = busy.send("the demo holds this turn".to_owned());
            std::thread::sleep(self.pace);
        }
        self.submit(line)
    }

    fn perform(&mut self, handoff: &Handoff) -> Vec<Beat> {
        Conversation::perform(&mut self.script, handoff)
    }

    /// The Session's own slash commands, so the chooser lists the real set.
    /// The demo still answers any line it is sent as its next scripted turn.
    fn commands(&self) -> Vec<String> {
        nika_session::runtime::SLASH_COMMANDS
            .iter()
            .map(|command| (*command).to_owned())
            .collect()
    }
}

/// `TERM` for the probe. The `disallowed_methods` ban on `std::env::var`
/// routes SECRET lookups through the vault; a display capability variable
/// is not a secret (the same allow the CLI carries for its colour chain).
#[allow(clippy::disallowed_methods)]
fn term() -> Option<String> {
    std::env::var("TERM").ok()
}

fn main() -> ExitCode {
    let Flags {
        mut options,
        pace,
        diagnostic,
    } = match parse(std::env::args().skip(1)) {
        Ok(flags) => flags,
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "{message}");
            return ExitCode::from(2);
        }
    };
    options.term = term();
    let demo = Demo {
        script: Script::demo(),
        pace,
        refusal: diagnostic.then(diagnostic_words).flatten(),
    };
    match app::run(demo, options) {
        Ok(exit) => {
            if exit.code() == 0 {
                let _ = writeln!(std::io::stdout(), "nika-tui-proto: left cleanly");
            }
            ExitCode::from(exit.code())
        }
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "nika-tui-proto: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Result<Flags, String> {
        parse(args.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn flags_parse_and_unknown_ones_refuse() {
        let Flags {
            options,
            pace,
            diagnostic,
        } = parsed(&["--focus", "--color", "--panic-after", "2"]).expect("valid");
        assert_eq!(options.presentation, Presentation::Focus);
        assert!(options.color);
        assert_eq!(options.panic_after, Some(2));
        assert_eq!(pace, Duration::ZERO, "the demo answers at once by default");
        assert!(!diagnostic);
        assert!(
            parsed(&["--demo-diagnostic"])
                .expect("fixture flag")
                .diagnostic
        );
        assert!(parsed(&["--nope"]).is_err());
        assert!(parsed(&["--exit-after"]).is_err());
        assert!(parsed(&["--demo-pace", "soon"]).is_err());
    }

    #[test]
    fn the_demo_pace_holds_each_turn() {
        let Flags { pace, .. } = parsed(&["--demo-pace", "800"]).expect("valid");
        assert_eq!(pace, Duration::from_millis(800));
        let mut demo = Demo {
            script: Script::demo(),
            pace: Duration::from_millis(30),
            refusal: None,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let started = std::time::Instant::now();
        let turn = demo.submit_with("x", &tx);
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert_eq!(rx.try_recv().as_deref(), Ok("the demo holds this turn"));
        assert!(!turn.beats.is_empty());
    }
}
