// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika verbs` — the four execution models, animated in the terminal.
//!
//! Usage: `cargo run -p nika-schema --example verbs -- [infer|exec|invoke|agent|all] [--ascii] [--color=…] [--frame N] [--no-anim]`
//!
//! Each verb plays as an ASCII storyboard: the `${{ }}` binding travels
//! into the verb, the spinner ticks on the running line (contract §3.2
//! braille · 80 ms), and the card settles on its `✔` final state. The
//! animation IS the data — every frame is a pure function (see
//! `scenes.rs`), so `--frame N` renders any moment statically, tests
//! pin frames byte-exact, and reduced motion is just « the last frame ».
//!
//! Motion gates (orthogonal to colour — `NO_COLOR` kills colour, NOT
//! motion): animation requires a TTY, no `NIKA_REDUCED_MOTION=1`, and
//! no `--no-anim`/`--frame`; otherwise the final frame prints once.

// A console demo's whole job is printing — same exemption as the
// nika-catalog-verify binary (the established precedent).
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

mod scenes;
#[path = "../check/theme.rs"]
mod theme;

use std::io::{IsTerminal, Write as _};
use std::process::ExitCode;

use theme::{ColorFlag, Theme, VerbKind};

/// One playback tick (contract §3.2 spinner cadence).
const TICK_MS: u64 = 80;

const USAGE: &str = "usage: verbs [infer|exec|invoke|agent|all] [--legend] [--ascii] [--color=auto|always|never] [--frame N] [--no-anim]";

struct Args {
    legend: bool,
    verbs: Vec<VerbKind>,
    color: ColorFlag,
    ascii: bool,
    frame: Option<usize>,
    no_anim: bool,
}

fn parse_args() -> Result<Args, ExitCode> {
    let mut legend = false;
    let mut verbs: Vec<VerbKind> = Vec::new();
    let mut color = ColorFlag::Auto;
    let mut ascii = false;
    let mut frame: Option<usize> = None;
    let mut no_anim = false;
    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "infer" => verbs.push(VerbKind::Infer),
            "exec" => verbs.push(VerbKind::Exec),
            "invoke" => verbs.push(VerbKind::Invoke),
            "agent" => verbs.push(VerbKind::Agent),
            "all" => {
                verbs = vec![
                    VerbKind::Infer,
                    VerbKind::Exec,
                    VerbKind::Invoke,
                    VerbKind::Agent,
                ];
            }
            "--legend" => legend = true,
            "--ascii" => ascii = true,
            "--no-anim" => no_anim = true,
            "--color=auto" => color = ColorFlag::Auto,
            "--color=always" => color = ColorFlag::Always,
            "--color=never" => color = ColorFlag::Never,
            "--frame" => {
                let Some(n) = argv.next().and_then(|n| n.parse().ok()) else {
                    eprintln!("--frame needs a number");
                    eprintln!("{USAGE}");
                    return Err(ExitCode::from(2));
                };
                frame = Some(n);
            }
            other => {
                eprintln!("unknown argument `{other}`");
                eprintln!("{USAGE}");
                return Err(ExitCode::from(2));
            }
        }
    }
    if verbs.is_empty() {
        verbs = vec![
            VerbKind::Infer,
            VerbKind::Exec,
            VerbKind::Invoke,
            VerbKind::Agent,
        ];
    }
    Ok(Args {
        legend,
        verbs,
        color,
        ascii,
        frame,
        no_anim,
    })
}

/// Whether playback is allowed — TTY + no reduced-motion request.
/// (`NIKA_REDUCED_MOTION` is a terminal-contract env read, same
/// exemption class as the theme's `NO_COLOR`.)
#[allow(clippy::disallowed_methods)]
fn motion_allowed(no_anim: bool) -> bool {
    !no_anim
        && std::io::stdout().is_terminal()
        && std::env::var_os("NIKA_REDUCED_MOTION").is_none_or(|v| v == "0")
}

/// Play one verb's storyboard in place (cursor-up redraw). The ONLY
/// raw escape here is cursor motion — colour stays in the theme seam.
fn play(verb: VerbKind, t: Theme) {
    let total = scenes::steps(verb);
    let mut prev_lines = 0usize;
    for step in 0..total {
        let frame = scenes::frame(verb, step, t);
        if prev_lines > 0 {
            // move up over the previous frame and clear to the end
            print!("\x1b[{prev_lines}A\x1b[J");
        }
        print!("{frame}");
        let _ = std::io::stdout().flush();
        prev_lines = frame.lines().count();
        std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
    }
    println!();
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(code) => return code,
    };
    let t = Theme::from_env(args.color, args.ascii);

    if args.legend {
        print!("{}", scenes::legend(t));
        return ExitCode::SUCCESS;
    }

    for verb in &args.verbs {
        if let Some(n) = args.frame {
            // a single static frame — CI · screenshots · docs
            print!("{}", scenes::frame(*verb, n, t));
        } else if motion_allowed(args.no_anim) {
            play(*verb, t);
        } else {
            // reduced motion / non-TTY: the completed card, once
            print!("{}", scenes::frame(*verb, scenes::steps(*verb) - 1, t));
        }
        println!();
    }
    ExitCode::SUCCESS
}
