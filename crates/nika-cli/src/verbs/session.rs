// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bare `nika` on a terminal — the native session (ADR-125 · One Door ·
//! wave 4): the session opens at once on the human's question (« What do
//! you want to automate? »); the facts and the deterministic compiler
//! answer without any setup, and the first time a turn needs an
//! intelligence the session asks how Nika should think with them (an AI
//! app they already have · an API · a local engine · none), keeps the
//! answer beside the other user files, and resumes the very line that
//! waited. One grounded conversation over the installed engine
//! ([`nika_session`]): no temporary workflow, no trace for a chat turn, no
//! hidden shell.
// The session owns a live terminal, like `run`: the prompt and the
// replies go to that terminal directly.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

use std::io::{BufRead, Write};
use std::path::PathBuf;

use nika_session::intelligence::{IntelligenceKind, UserIntelligencePreference};
use nika_session::reasoner::{NoReasoner, ProviderReasoner, SessionReasoner};
use nika_session::{
    IntelligenceCensus, ResolvedSessionIntelligence, RunRequest, SessionRuntime, TurnOutcome,
};

use crate::verbs::run::RenderMode;
use nika_dap::resume::ResumeRequest;

use crate::Theme;
use crate::verbs::exit;
use nika_cli_host::lane::{ChildSlot, drive_child};
use nika_cli_host::lines::{PerCallLines, read_burst};

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
    theme: Theme,
) -> std::io::Result<u8> {
    // The kept choice opens the session as chosen; without one the session
    // opens all the same and asks the first screen in context, the first
    // time a turn needs an intelligence.
    let mut session = match home.and_then(UserIntelligencePreference::load) {
        Some(pref) => {
            SessionRuntime::open_with(cwd, census.clone(), &pref, home, Box::new(reasoner_for))
        }
        None => SessionRuntime::open_unchosen(cwd, census.clone(), home, Box::new(reasoner_for)),
    };
    // A truthful line while the compiler works under a seat — to the
    // terminal the human watches, never a percentage, never an ETA.
    session.on_progress(Box::new(|line| {
        use std::io::Write as _;
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }));
    let recovered = match home {
        Some(home) => match session.enable_history(home) {
            Ok(notice) => notice,
            Err(why) => {
                writeln!(output, "✖ {why}")?;
                return Ok(exit::ENV);
            }
        },
        None => Some("conversation is temporary: no home directory is available".to_owned()),
    };
    writeln!(output, "{}", session.banner())?;
    if let Some(notice) = recovered {
        writeln!(output, "{notice}")?;
    }
    if let Some(notice) = session.restore_state() {
        writeln!(output, "{notice}")?;
    }
    // The line goes where the MACHINE's state says (ADR-133 · #1464): the
    // runtime owns what waits — the first screen, a proposal, a gate, an
    // authoring question (each its own prompt: a `yes` never crosses from
    // one to another). The door keeps no bit of its own.
    loop {
        let prompt = if session.pending_choice() {
            "› "
        } else if session.pending_proposal().is_some() {
            "apply? › "
        } else if session.waiting_gate().is_some() {
            "answer › "
        } else if session.pending_question().is_some()
            || session.pending_input().is_some()
            || session.pending_activation().is_some()
        {
            "reply › "
        } else {
            "nika › "
        };
        write!(output, "\n{prompt}")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(exit::OK);
        }
        let outcome = if session.pending_choice() {
            session.choose(line.trim())
        } else if session.pending_proposal().is_some() {
            session.consent(line.trim())
        } else if session.waiting_gate().is_some() {
            session.answer_gate(line.trim())
        } else {
            session.turn(&line)
        };
        if handle_outcome(output, &mut session, outcome, theme)? {
            return Ok(exit::OK);
        }
    }
}

/// Print one turn's outcome and act on it (a run the turn requested is
/// driven here, its observation fed back). Answers `true` only for
/// `Quit` — the door closes with `exit::OK`.
fn handle_outcome<W: Write>(
    output: &mut W,
    session: &mut SessionRuntime,
    outcome: TurnOutcome,
    theme: Theme,
) -> std::io::Result<bool> {
    match outcome {
        TurnOutcome::Quit => return Ok(true),
        TurnOutcome::Reply(text)
        | TurnOutcome::Facts(text)
        | TurnOutcome::Help(text)
        | TurnOutcome::Aside(text) => {
            if !text.is_empty() {
                writeln!(output, "{text}")?;
            }
        }
        TurnOutcome::Ask(screen) => writeln!(output, "{screen}")?,
        // The choice landed and the line that waited for it resumed: the
        // choice's fact first, then whatever that line became.
        TurnOutcome::Resumed { notice, outcome } => {
            writeln!(output, "{notice}")?;
            return handle_outcome(output, session, *outcome, theme);
        }
        TurnOutcome::Proposal { preview, .. } | TurnOutcome::Held { preview, .. } => {
            writeln!(output, "{preview}")?;
        }
        TurnOutcome::RunRequested { report, run } => {
            writeln!(output, "{report}")?;
            writeln!(
                output,
                "running `{}` once · ceiling ${:.2}",
                run.workflow.display(),
                run.max_cost_usd
            )?;
            output.flush()?;
            let (code, trace) = run_once(&session.snapshot.root, &run, theme);
            observed(output, session.observe_run(code, trace.as_deref()))?;
        }
        // An authoring question and a runtime gate print the same way; the
        // prompt that follows (`reply ›` · `answer ›`) names which one waits.
        TurnOutcome::Question { question, .. } | TurnOutcome::GateAsk { question, .. } => {
            writeln!(output, "{question}")?;
        }
        TurnOutcome::ResumeRequested {
            workflow,
            trace,
            answer,
        } => {
            writeln!(output, "resuming `{}` with your answer", workflow.display())?;
            output.flush()?;
            let (code, newest) =
                run_resume(&session.snapshot.root, &workflow, &trace, &answer, theme);
            observed(output, session.observe_run(code, newest.as_deref()))?;
        }
        TurnOutcome::Refusal(text) => writeln!(output, "✖ {text}")?,
        _ => {}
    }
    Ok(false)
}

/// Print what the session observed of a run; a gate's question is printed
/// too — the machine now waits for the answer, and the prompt says so.
fn observed<W: Write>(output: &mut W, outcome: TurnOutcome) -> std::io::Result<()> {
    match outcome {
        TurnOutcome::GateAsk { question, .. } => writeln!(output, "{question}"),
        TurnOutcome::Facts(line) => writeln!(output, "{line}"),
        TurnOutcome::Refusal(why) => writeln!(output, "{why}"),
        _ => Ok(()),
    }
}

/// Resume the SAME run with the human's answer, through the path `nika run
/// --resume` owns; the exit code and the trace the resume left.
fn run_resume(
    root: &std::path::Path,
    workflow: &std::path::Path,
    trace: &std::path::Path,
    answer: &str,
    theme: Theme,
) -> (u8, Option<std::path::PathBuf>) {
    let file = root.join(workflow).display().to_string();
    let resume = ResumeRequest {
        trace: Some(trace.to_path_buf()),
        from: None,
        answers: vec![answer.to_owned()],
        compat: None,
        allow_unverified: false,
    };
    let verdict = crate::verbs::run::run_verdict(
        &file,
        false,
        None,
        theme,
        RenderMode::Thread,
        false,
        None,
        None,
        crate::verbs::run::inputs::InputBindings::Operator(&[]),
        Some(&resume),
        false,
        None,
        false,
        None,
        false,
        false,
        None,
    );
    (verdict.code, verdict.trace)
}

/// The run the human consented to, through the SAME path as `nika run`
/// (the door executes; the session observes): the exit code, and the
/// trace the run left when it left one.
fn run_once(
    root: &std::path::Path,
    run: &RunRequest,
    theme: Theme,
) -> (u8, Option<std::path::PathBuf>) {
    let file = root.join(&run.workflow).display().to_string();
    let verdict = crate::verbs::run::run_verdict(
        &file,
        false,
        None,
        theme,
        RenderMode::Thread,
        false,
        None,
        None,
        crate::verbs::run::inputs::InputBindings::Operator(&run.vars),
        None,
        false,
        None,
        false,
        Some(run.max_cost_usd),
        false,
        false,
        None,
    );
    (verdict.code, verdict.trace)
}

/// `TERM` as the renderer's probe reads it. The `disallowed_methods` ban on
/// `std::env::var` routes SECRET lookups through the vault; a display
/// capability variable is not a secret (the same allow `main.rs` carries).
#[allow(clippy::disallowed_methods)]
fn term_name() -> Option<String> {
    std::env::var("TERM").ok()
}

/// The run inside the renderer's turn: this binary's own machine lane
/// (`nika run --json`) as a child whose pipes never touch the terminal
/// the viewport owns. Each frame the lane prints becomes one line of the
/// run's story, handed to the busy sink as it happens and kept for the
/// block the transcript commits; the exit code is the child's, the trace
/// the settle frame names. A human gate pauses headless (exit 4): the
/// session asks it in the viewport and the answer resumes through here.
fn run_tapped(
    root: &std::path::Path,
    work: &nika_tui::session::Work,
    busy: &std::sync::mpsc::Sender<String>,
    slot: &ChildSlot,
) -> (u8, Option<std::path::PathBuf>, Vec<String>) {
    use nika_tui::session::Work;
    let mut args: Vec<String> = vec!["run".to_owned()];
    match work {
        Work::Run(run) => {
            args.push(root.join(&run.workflow).display().to_string());
            args.push("--json".to_owned());
            args.push("--max-cost-usd".to_owned());
            args.push(format!("{}", run.max_cost_usd));
            for var in &run.vars {
                args.push("--var".to_owned());
                args.push(var.clone());
            }
        }
        Work::Resume {
            workflow,
            trace,
            answer,
        } => {
            args.push(root.join(workflow).display().to_string());
            args.push("--json".to_owned());
            args.push("--resume".to_owned());
            args.push(trace.display().to_string());
            args.push("--answer".to_owned());
            args.push(answer.clone());
        }
        _ => {
            return (
                exit::ENV,
                None,
                vec!["a kind of work this door cannot run".to_owned()],
            );
        }
    }
    let Ok(exe) = std::env::current_exe() else {
        return (
            exit::ENV,
            None,
            vec!["this binary cannot name itself".to_owned()],
        );
    };
    drive_child(&exe, &args, root, busy, slot)
}

/// Open the native session behind the terminal renderer (bare `nika` on a terminal ·
/// ADR-139 · UX-2): the same runtime, the same census and kept choice, the
/// same two run paths lent as runners, and the tapped runner that keeps
/// the terminal: a run shows inside the viewport, its gate asks there.
///
/// A terminal the renderer cannot take (`TERM=dumb` · one that never
/// answers the cursor-position report the inline viewport anchors on)
/// gets the plain session instead — the same session, said once on
/// stderr, never a dead door (UX-2 · the terminal matrix).
#[must_use]
pub fn run_tui(theme: Theme) -> u8 {
    use nika_tui::session::{Live, Runners};
    let mut options = nika_tui::app::Options::new(nika_tui::model::Presentation::Inline);
    options.color = theme.color;
    options.term = term_name();
    let taken = match nika_tui::app::enter(&options) {
        Ok(taken) => taken,
        Err(error) => {
            let _ = writeln!(
                std::io::stderr(),
                "nika: the renderer cannot take this terminal ({error}) · the plain session opens instead"
            );
            return run(theme);
        }
    };
    let census = IntelligenceCensus::take();
    let home = nika_cli_host::probe::home_dir();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let kept = home.as_deref().and_then(UserIntelligencePreference::load);
    let child: ChildSlot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let slot = std::sync::Arc::clone(&child);
    let runners = Runners {
        run_once: Box::new(move |root, run| run_once(root, run, theme)),
        run_resume: Box::new(move |root, workflow, trace, answer| {
            run_resume(root, workflow, trace, answer, theme)
        }),
        run_tapped: Some(Box::new(move |root, work, busy| {
            run_tapped(root, work, busy, &slot)
        })),
    };
    let live = Live::new(cwd, census, kept, home, Box::new(reasoner_for), runners);
    let left = nika_tui::app::run_on(taken, live, options);
    // A run still in flight when the door leaves is ended, never orphaned:
    // the engine cancels on SIGTERM and its trace says so.
    if let Some(pid) = child.lock().ok().and_then(|guard| *guard)
        && let Ok(pid) = i32::try_from(pid)
    {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGTERM,
        );
    }
    match left {
        Ok(left) => left.code(),
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "nika: {error}");
            exit::ENV
        }
    }
}

/// Open the native session on this terminal.
#[must_use]
pub fn run(theme: Theme) -> u8 {
    let mut input = PerCallLines::new(read_burst);
    let mut output = std::io::stdout();
    let census = IntelligenceCensus::take();
    let home = nika_cli_host::probe::home_dir();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match drive(
        &mut input,
        &mut output,
        &census,
        home.as_deref(),
        &cwd,
        theme,
    ) {
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

    #[test]
    fn a_reopened_terminal_restores_history_and_refuses_corruption() {
        let project = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        let census = IntelligenceCensus::empty();
        let invoke = |lines: &[u8]| {
            let mut input = Cursor::new(lines.to_vec());
            let mut output = Vec::new();
            let code = drive(
                &mut input,
                &mut output,
                &census,
                Some(home.path()),
                project.path(),
                Theme::new(false, false, false),
            )
            .expect("drive");
            (code, String::from_utf8(output).expect("text"))
        };
        assert_eq!(invoke(b"4\nwhat workflows are here?\n/quit\n").0, exit::OK);
        let (code, text) = invoke(b"/quit\n");
        assert_eq!(code, exit::OK);
        assert!(text.contains("conversation restored"), "{text}");
        let histories = home.path().join(".nika/sessions");
        let folder = std::fs::read_dir(histories)
            .expect("histories")
            .next()
            .expect("one project")
            .expect("entry")
            .path();
        let journal = folder.join("events.ndjson");
        std::fs::write(&journal, "broken").expect("corrupt fixture");
        let (code, text) = invoke(b"what workflows are here?\n");
        assert_eq!(code, exit::ENV);
        assert!(text.contains("history unavailable"), "{text}");
        assert_eq!(
            std::fs::read_to_string(journal).expect("retained"),
            "broken"
        );
    }

    /// The first run opens at once on the human's question, answers a fact
    /// without any model or setup, asks the first screen only when a turn
    /// needs an intelligence (a typo keeps the request waiting), keeps the
    /// answer under the home and resumes that very line. Nothing is
    /// written into the project.
    #[test]
    fn the_first_run_opens_at_once_and_asks_only_when_a_turn_needs_it() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        std::fs::write(
            project.path().join("hello.nika"),
            "nika: hello\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("workflow");
        let census = IntelligenceCensus::empty();
        let mut input = Cursor::new(
            b"what workflows are here?\nhello there, how are you today?\n9\n4\n/quit\n".to_vec(),
        );
        let mut output = Vec::new();
        let code = drive(
            &mut input,
            &mut output,
            &census,
            Some(home.path()),
            project.path(),
            Theme::new(false, false, false),
        )
        .expect("io");
        assert_eq!(code, exit::OK);
        let text = String::from_utf8(output).expect("utf8");
        assert!(
            text.contains("What do you want to automate?"),
            "the session opens on the question: {text}"
        );
        assert!(!text.contains("nika · session"), "no engine banner: {text}");
        let fact = text.find("hello.nika").expect("the fact answers");
        let ask = text
            .find("Nika needs an intelligence for this part")
            .expect("the first screen is asked in context");
        assert!(fact < ask, "the fact answered before any choice: {text}");
        assert!(
            !text[..ask].contains("Choose which AI"),
            "nothing was asked before a turn needed it: {text}"
        );
        assert!(text.contains("`9` is not a choice"), "{text}");
        assert!(text.contains("no conversational AI"), "{text}");
        assert!(
            text.contains("the facts still answer"),
            "the waiting line resumed under the choice: {text}"
        );
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
            Theme::new(false, false, false),
        )
        .expect("io");
        assert_eq!(code, exit::OK, "EOF closes the session cleanly");
        let text = String::from_utf8(output).expect("utf8");
        assert!(!text.contains("Choose which AI"), "{text}");
        assert!(!text.contains("Nika needs an intelligence"), "{text}");
        assert!(text.contains("/intelligence"), "the help card: {text}");
    }

    /// The door's line source takes its lock per line: one fill per line
    /// read, nothing held between lines, EOF when the source is dry.
    #[test]
    fn the_line_source_fills_once_per_line_and_holds_nothing_between() {
        let mut fills = 0usize;
        let mut cursor = Cursor::new(b"one\ntwo\n".to_vec());
        let mut lines = PerCallLines::new(|buf: &mut Vec<u8>| {
            fills += 1;
            cursor.read_until(b'\n', buf)
        });
        let mut line = String::new();
        assert_eq!(lines.read_line(&mut line).expect("io"), 4);
        assert_eq!(line, "one\n");
        line.clear();
        assert_eq!(lines.read_line(&mut line).expect("io"), 4);
        assert_eq!(line, "two\n");
        line.clear();
        assert_eq!(lines.read_line(&mut line).expect("io"), 0, "EOF");
        drop(lines);
        assert_eq!(fills, 3, "one fill per line, one for the EOF");
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
            Theme::new(false, false, false),
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

#[cfg(test)]
mod run_tests;
