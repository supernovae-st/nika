// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-tui-proto` — the UX-1 renderer proof (ADR-139).
//!
//! Runs the shell over the canned conversation of [`Script::demo`] in one
//! presentation, on a real terminal, so both presentations are judged on the
//! same fixture and the terminal lifecycle is proven from a PTY: the normal
//! exit, `Ctrl+C`, a panic (`--panic-after N`) and `SIGTERM` all restore the
//! terminal. It is not `nika`: nothing here reaches the session runtime.
//!
//! ```text
//! nika-tui-proto [--focus] [--color] [--panic-after N] [--exit-after N]
//! ```
//!
//! Exit codes: `0` closed · `130` two `Ctrl+C` · `143` `SIGTERM` · `2` the
//! arguments or the terminal refused.

use std::io::Write as _;
use std::process::ExitCode;

use nika_tui::app::{self, Options};
use nika_tui::model::{Presentation, Script};

fn usage() -> &'static str {
    "usage: nika-tui-proto [--focus] [--color] [--panic-after N] [--exit-after N]"
}

fn parse(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::new(Presentation::Inline);
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--focus" => options.presentation = Presentation::Focus,
            "--inline" => options.presentation = Presentation::Inline,
            "--color" => options.color = true,
            "--panic-after" | "--exit-after" => {
                let count = args
                    .next()
                    .ok_or_else(|| format!("{arg} needs a count"))?
                    .parse::<usize>()
                    .map_err(|e| format!("{arg}: {e}"))?;
                if arg == "--panic-after" {
                    options.panic_after = Some(count);
                } else {
                    options.exit_after = Some(count);
                }
            }
            "-h" | "--help" => return Err(usage().to_owned()),
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
    }
    Ok(options)
}

/// `TERM` for the probe. The `disallowed_methods` ban on `std::env::var`
/// routes SECRET lookups through the vault; a display capability variable
/// is not a secret (the same allow the CLI carries for its colour chain).
#[allow(clippy::disallowed_methods)]
fn term() -> Option<String> {
    std::env::var("TERM").ok()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "{message}");
            return ExitCode::from(2);
        }
    };
    let mut options = options;
    options.term = term();
    match app::run(Script::demo(), options).await {
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

    #[test]
    fn flags_parse_and_unknown_ones_refuse() {
        let options = parse(
            ["--focus", "--color", "--panic-after", "2"]
                .into_iter()
                .map(str::to_owned),
        )
        .expect("valid");
        assert_eq!(options.presentation, Presentation::Focus);
        assert!(options.color);
        assert_eq!(options.panic_after, Some(2));
        assert!(parse(["--nope"].into_iter().map(str::to_owned)).is_err());
        assert!(parse(["--exit-after"].into_iter().map(str::to_owned)).is_err());
    }
}
