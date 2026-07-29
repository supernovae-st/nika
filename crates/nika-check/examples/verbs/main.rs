// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika verbs` — the four execution models, animated in the terminal.
//!
//! Usage: `cargo run -p nika-check --example verbs -- [infer|exec|invoke|agent|all] [--ascii] [--color=…] [--frame N] [--no-anim]`
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
mod tape;
mod theme;

use std::io::{IsTerminal, Write as _};
use std::process::ExitCode;

use theme::{ColorFlag, Theme, VerbKind};

/// One playback tick (contract §3.2 spinner cadence).
const TICK_MS: u64 = 80;

/// Bad invocation (sysexits `EX_USAGE` — additive per the contract §4 law).
const EXIT_USAGE: u8 = 64;

const USAGE: &str = "usage: verbs [infer|exec|invoke|agent|workflow|all] [--events [--json]] [--legend] [--ascii] [--color=auto|always|never] [--frame N] [--no-anim]";

/// What to show — the modes are mutually exclusive, so they are an
/// enum, not flags (the type encodes the exclusivity).
enum Mode {
    /// The per-verb storyboards.
    Theater(Vec<VerbKind>),
    /// The event tape folded live into the animated DAG.
    Workflow,
    /// Every telemetry event as a line, then the folded card.
    Tape,
    /// The canonical theme reference card.
    Legend,
}

struct Args {
    mode: Mode,
    /// NDJSON wire output (only with `--events` — the machine renderer).
    json: bool,
    color: ColorFlag,
    ascii: bool,
    frame: Option<usize>,
    no_anim: bool,
}

fn parse_args() -> Result<Args, ExitCode> {
    let mut mode: Option<Mode> = None;
    let mut json = false;
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
            "workflow" => mode = Some(Mode::Workflow),
            "all" => {
                verbs = vec![
                    VerbKind::Infer,
                    VerbKind::Exec,
                    VerbKind::Invoke,
                    VerbKind::Agent,
                ];
            }
            "--legend" => mode = Some(Mode::Legend),
            "--events" => mode = Some(Mode::Tape),
            "--json" => json = true,
            "--ascii" => ascii = true,
            "--no-anim" => no_anim = true,
            "--color=auto" => color = ColorFlag::Auto,
            "--color=always" => color = ColorFlag::Always,
            "--color=never" => color = ColorFlag::Never,
            "--frame" => {
                let Some(n) = argv.next().and_then(|n| n.parse().ok()) else {
                    eprintln!("--frame needs a number");
                    eprintln!("{USAGE}");
                    return Err(ExitCode::from(EXIT_USAGE));
                };
                frame = Some(n);
            }
            other => {
                eprintln!("unknown argument `{other}`");
                eprintln!("{USAGE}");
                return Err(ExitCode::from(EXIT_USAGE));
            }
        }
    }
    let mode = mode.unwrap_or_else(|| {
        Mode::Theater(if verbs.is_empty() {
            vec![
                VerbKind::Infer,
                VerbKind::Exec,
                VerbKind::Invoke,
                VerbKind::Agent,
            ]
        } else {
            verbs
        })
    });
    Ok(Args {
        mode,
        json,
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

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(code) => return code,
    };
    if args.json && !matches!(args.mode, Mode::Tape) {
        eprintln!(
            "--json composes with --events only (the NDJSON wire is the tape's machine renderer)"
        );
        eprintln!("{USAGE}");
        return ExitCode::from(EXIT_USAGE);
    }
    let t = Theme::from_env(args.color, args.ascii);

    match args.mode {
        Mode::Legend => print!("{}", theme::legend(t)),
        // the tape view — every telemetry event, then the folded card;
        // --json = the SAME tape as NDJSON verbatim (contract §3: the
        // machine renderer · never coloured · one event per line)
        Mode::Tape => {
            if args.json {
                match tape::render_ndjson() {
                    Ok(nd) => print!("{nd}"),
                    Err(e) => {
                        eprintln!("cannot serialize tape: {e}");
                        return ExitCode::from(70);
                    }
                }
            } else {
                print!("{}", tape::render_tape(t));
            }
        }
        // the motion view — the SAME tape, folded live into DAG lanes
        Mode::Workflow => {
            let total = tape::total_steps();
            if let Some(n) = args.frame {
                print!("{}", tape::workflow_frame(n, t));
            } else if motion_allowed(args.no_anim) {
                animate(total, |step| tape::workflow_frame(step, t));
            } else {
                print!("{}", tape::workflow_frame(total - 1, t));
            }
            println!();
        }
        Mode::Theater(verbs) => {
            for verb in verbs {
                if let Some(n) = args.frame {
                    // a single static frame — CI · screenshots · docs
                    print!("{}", scenes::frame(verb, n, t));
                } else if motion_allowed(args.no_anim) {
                    animate(scenes::steps(verb), |step| scenes::frame(verb, step, t));
                } else {
                    // reduced motion / non-TTY: the completed card, once
                    print!("{}", scenes::frame(verb, scenes::steps(verb) - 1, t));
                }
                println!();
            }
        }
    }
    ExitCode::SUCCESS
}

/// In-place playback of any pure frame function (cursor-up redraw —
/// the ONLY raw escape, and it is motion, not colour).
fn animate(total: usize, frame_fn: impl Fn(usize) -> String) {
    let mut prev = 0usize;
    for step in 0..total {
        let frame = frame_fn(step);
        if prev > 0 {
            print!("\x1b[{prev}A\x1b[J");
        }
        print!("{frame}");
        let _ = std::io::stdout().flush();
        prev = frame.lines().count();
        std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
    }
}
