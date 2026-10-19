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
//! `--demo-surface intelligence|multi|choice|hours|hours-closed` opens on one
//! choice surface instead of the canned conversation: a 30-model intelligence
//! list, a multiple choice, a question that recommends one source, or a
//! question offering hours with no recommendation (`hours-closed`: its author
//! closed it to its offers); whatever is sent is said back as
//! `received: <text>`, so a proof reads exactly what a key sent. `--demo-surface working` opens at the free prompt, and a line sent
//! there is prepared for about three seconds: two author calls reported,
//! Stop held, a spend known once it ends. `--demo-surface decision` opens on
//! a reviewed proposal's save decision: Save, Save and run once and Discard
//! end it, and any other line (`/meaning`, Inspect's `/show`) is said back
//! while the decision waits again. `--demo-surface gate` opens on a
//! workflow's approval gate, which takes no reply of its own: Approve and
//! Refuse end it, anything else waits again. `--ascii` paints the ASCII glyph
//! column.
//!
//! `--workspace` opens the workspace, whose empty preview is the welcome;
//! `--ascii` takes the ASCII glyph column and `--reduced-motion` draws the
//! welcome's mark final at once. The brand's tones read `COLORTERM` (24-bit
//! colour) and `TERM` (256 colours) as the `nika` binary does.
//!
//! ```text
//! nika-tui-proto [--focus|--workspace] [--color] [--ascii] [--reduced-motion] [--demo-pace MS] [--demo-surface KIND] [--panic-after N] [--exit-after N]
//! ```
//!
//! Exit codes: `0` closed · `130` two `Ctrl+C` · `143` `SIGTERM` · `2` the
//! arguments or the terminal refused.

use std::io::Write as _;
use std::process::ExitCode;
use std::sync::mpsc::Sender;
use std::time::Duration;

use nika_display::activity::{Activity, CallCost, CallMark, CallState, Phase};
use nika_tui::app::{self, Options};
use nika_tui::model::{
    Asked, Beat, Committed, Conversation, Handoff, Kind, Offer, Presentation, Script, Shape,
    Stopping, Turn, Waiting,
};

fn usage() -> &'static str {
    "usage: nika-tui-proto [--focus|--workspace] [--color] [--ascii] [--reduced-motion] [--demo-pace MS] [--demo-diagnostic] [--demo-surface intelligence|multi|choice|hours|hours-closed|working|decision|gate] [--panic-after N] [--exit-after N]"
}

/// The one choice surface `--demo-surface` opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    /// A 30-model intelligence list (a settings choice).
    Intelligence,
    /// A multiple choice question.
    Multi,
    /// A question that recommends one source among four.
    Choice,
    /// A question offering four hours, none recommended.
    Hours,
    /// The same question, its author having closed it to its offers.
    HoursClosed,
    /// The free prompt, where a line is prepared with two author calls.
    Working,
    /// A reviewed proposal's save decision: Save, Save and run once,
    /// Discard, Inspect.
    Decision,
    /// A workflow's approval gate: Approve, Refuse, Explain.
    Gate,
}

/// The shell's options and the demo's own pace.
struct Flags {
    options: Options,
    pace: Duration,
    diagnostic: bool,
    surface: Option<Surface>,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Flags, String> {
    let mut options = Options::new(Presentation::Inline);
    let mut pace = Duration::ZERO;
    let mut diagnostic = false;
    let mut surface = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--focus" => options.presentation = Presentation::Focus,
            "--inline" => options.presentation = Presentation::Inline,
            "--workspace" => options.presentation = Presentation::Workspace,
            "--color" => options.color = true,
            "--ascii" => options.ascii = true,
            "--reduced-motion" => options.reduced_motion = true,
            "--demo-diagnostic" => diagnostic = true,
            "--demo-surface" => {
                surface = Some(match args.next().as_deref() {
                    Some("intelligence") => Surface::Intelligence,
                    Some("multi") => Surface::Multi,
                    Some("choice") => Surface::Choice,
                    Some("hours") => Surface::Hours,
                    Some("hours-closed") => Surface::HoursClosed,
                    Some("working") => Surface::Working,
                    Some("decision") => Surface::Decision,
                    Some("gate") => Surface::Gate,
                    other => {
                        return Err(format!(
                            "--demo-surface needs intelligence, multi, choice, hours, hours-closed, working, decision or gate, not {other:?}"
                        ));
                    }
                });
            }
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
        surface,
    })
}

/// The 30 models of the intelligence surface: names and facts as the picker paints them.
fn models() -> Vec<Offer> {
    let families = [
        (
            "anthropic/claude-sonnet-5-5",
            "Claude Sonnet 5.5",
            "$2 in / $10 out per MTok",
        ),
        (
            "anthropic/claude-opus-5-5",
            "Claude Opus 5.5",
            "$10 in / $50 out per MTok",
        ),
        (
            "deepseek/deepseek-flash",
            "DeepSeek Flash",
            "$0.30 in / $1.20 out per MTok",
        ),
        (
            "openai/gpt-6.1-sol",
            "GPT-6.1 Sol",
            "$1.25 in / $10 out per MTok",
        ),
        (
            "mistral/mistral-large-latest",
            "Mistral Large Latest",
            "$0.50 in / $1.50 out per MTok",
        ),
    ];
    (0..30)
        .map(|n| {
            let (id, name, price) = families[n % families.len()];
            let generation = n / families.len();
            let (key, label) = if generation == 0 {
                (format!("2 {id}"), name.to_owned())
            } else {
                (
                    format!("2 {id}-r{generation}"),
                    format!("{name} r{generation}"),
                )
            };
            Offer::new(key, label).noted(format!("{price} · 1M context"))
        })
        .collect()
}

/// The surface `kind` as the conversation waits on it.
fn surface(kind: Surface) -> Waiting {
    match kind {
        Surface::Working => Waiting::Free,
        Surface::Decision => Waiting::action(
            "save",
            Asked::new(
                "What should happen to this reviewed proposal?",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("yes", "Save workflow"),
                    Offer::new("save & run", "Save and run once"),
                    Offer::new("no", "Discard this proposal"),
                    Offer::new("/show", "Inspect the exact changes"),
                ]),
                "action:1",
                1,
            ),
        ),
        Surface::Gate => {
            let mut asked = Asked::new(
                "Answer this workflow confirmation",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("yes", "Approve this step"),
                    Offer::new("no", "Refuse this step"),
                    Offer::new("why?", "Explain this step"),
                ]),
                "action:gate",
                1,
            );
            asked.free_text = false;
            Waiting::action("gate", asked)
        }
        Surface::Intelligence => Waiting::selection(Asked::new(
            "Choose a model · demo",
            "",
            false,
            Shape::Choice(models()),
            "1:intelligence",
            1,
        )),
        Surface::Multi => Waiting::asked(
            "sources",
            Asked::new(
                "Which sources should the digest read?",
                "",
                true,
                Shape::Multiple(vec![
                    Offer::new("notes", "Meeting notes"),
                    Offer::new("mail", "Mail threads"),
                    Offer::new("tickets", "Open tickets"),
                    Offer::new("calendar", "Calendar"),
                ]),
                "1:sources",
                1,
            ),
        ),
        Surface::Choice => Waiting::asked(
            "source",
            Asked::new(
                "Which source should the digest read first?",
                "",
                false,
                Shape::Choice(vec![
                    Offer::new("hn", "Hacker News · news.ycombinator.com"),
                    Offer::new("tc", "TechCrunch · techcrunch.com/feed").recommended(),
                    Offer::new("verge", "The Verge · theverge.com/rss"),
                    Offer::new("lobsters", "Lobsters · lobste.rs"),
                ]),
                "1:source",
                1,
            ),
        ),
        Surface::HoursClosed => {
            let Waiting::QuestionDocument { key, mut asked, .. } = surface(Surface::Hours) else {
                return Waiting::Free;
            };
            asked.free_text = false;
            Waiting::asked(key, asked)
        }
        Surface::Hours => Waiting::asked(
            "hour",
            Asked::new(
                "At what time should the briefing arrive?",
                "",
                true,
                Shape::Choice(vec![
                    Offer::new("06:00", "6h00"),
                    Offer::new("07:00", "7h00"),
                    Offer::new("08:00", "8h00"),
                    Offer::new("09:00", "9h00"),
                ]),
                "1:hour",
                1,
            ),
        ),
    }
}

/// One choice surface whose every answer is said back exactly: what a proof reads.
struct Surfaced {
    kind: Surface,
    /// Turns prepared so far (the `working` surface's spend is known after one).
    prepared: usize,
}

/// How long the `working` surface prepares each line.
const PREPARING: Duration = Duration::from_millis(3_200);

/// One author call of the `working` surface's preparation, as its Session reports it: each
/// returns priced (`$0.12`, then `$0.19`: the turn's `$0.31`).
fn author_call(ordinal: u32, state: CallState) -> Activity {
    let mut activity = Activity::now(Phase::Authoring, "writing the workflow");
    let mut mark = CallMark::new(ordinal, "plan", "demo/author", state);
    if state == CallState::Finished {
        let nano = if ordinal == 1 {
            120_000_000
        } else {
            190_000_000
        };
        mark = mark.costing(CallCost::Priced(nano));
    }
    activity.call = Some(mark);
    activity
}

/// The decision's acts that end it; any other line leaves it waiting.
const ENDS_DECISION: [&str; 3] = ["yes", "save & run", "no"];

/// The approval gate's acts that end it.
const ENDS_GATE: [&str; 2] = ["yes", "no"];

impl Surfaced {
    fn received(text: &str) -> Turn {
        Turn::new(
            vec![
                Beat::Say(Committed::new(Kind::Reply, format!("received: {text}"))),
                Beat::Wait(Waiting::Free),
            ],
            None,
        )
    }

    /// What `text` got: said back, and the decision or the gate asked again
    /// unless it ended it.
    fn answered(&self, text: &str) -> Turn {
        let mut turn = Self::received(text);
        let ends = match self.kind {
            Surface::Decision => ENDS_DECISION.contains(&text),
            Surface::Gate => ENDS_GATE.contains(&text),
            _ => true,
        };
        if !ends {
            turn.beats.pop();
            turn.beats.push(Beat::Wait(surface(self.kind)));
        }
        turn
    }
}

impl Conversation for Surfaced {
    fn open(&mut self) -> Vec<Beat> {
        vec![
            Beat::Say(Committed::new(Kind::Banner, "Nika · demo surface")),
            Beat::Wait(surface(self.kind)),
        ]
    }

    fn submit(&mut self, line: &str) -> Turn {
        self.answered(line)
    }

    /// The decision's palette lists the commands a person reads beside it.
    fn commands(&self) -> Vec<String> {
        if self.kind != Surface::Decision {
            return Vec::new();
        }
        ["/meaning", "/why", "/show", "/status"]
            .map(str::to_owned)
            .to_vec()
    }

    /// The `working` surface prepares the line: two author calls, reported as
    /// they start and return, then the words said back.
    fn submit_observed(
        &mut self,
        line: &str,
        busy: &Sender<String>,
        seen: &nika_tui::session::feed::Seen,
    ) -> Turn {
        if self.kind != Surface::Working {
            return self.answered(line);
        }
        let _ = busy.send("● authoring · demo/author".to_owned());
        for ordinal in 1..=2 {
            seen.activity(author_call(ordinal, CallState::Started));
            std::thread::sleep(PREPARING / 2);
            seen.activity(author_call(ordinal, CallState::Finished));
        }
        self.prepared += 1;
        let mut turn = Self::received(line);
        // The stage line the spend so far follows, as a Session reports it.
        let rail = Beat::Rail("Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○".to_owned());
        turn.beats.insert(1, rail);
        turn
    }

    /// Only the `working` surface prepares: Stop reaches it.
    fn stopper(&mut self) -> Option<nika_tui::model::Stopper> {
        (self.kind == Surface::Working)
            .then(|| -> nika_tui::model::Stopper { Box::new(|| Stopping::Requested) })
    }

    /// The `working` surface's preparation, priced once it ended: $0.31 a turn.
    fn spend(&self) -> Option<f64> {
        let turns = u32::try_from(self.prepared).unwrap_or(u32::MAX);
        (turns > 0).then(|| 0.31 * f64::from(turns))
    }

    fn answer_bound(
        &mut self,
        text: &str,
        _witness: &str,
        _busy: &Sender<String>,
        _seen: &nika_tui::session::feed::Seen,
    ) -> Turn {
        self.answered(text)
    }

    fn perform(&mut self, _handoff: &Handoff) -> Vec<Beat> {
        Vec::new()
    }
}

/// Typed evidence for the display fixture, refused before reading a release.
fn diagnostic_words() -> Option<String> {
    use nika_cli_host::compile::config::AuthoringSettings;
    use nika_session::authoring::{AuthoringContext, AuthoringError};
    let mut env = AuthoringSettings::none();
    // A long named source makes the raw diagnostic exceed an inline viewport.
    env.knowledge = Some(std::path::PathBuf::from(format!(
        "/srv/foundry/{}release-r3",
        "display-evidence/".repeat(24)
    )));
    let context = AuthoringContext::from_settings(&AuthoringSettings::none(), &env);
    context.refusal().cloned().map(|cause| format!(
        "{} · this workflow-authoring request was not sent, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again",
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

/// Whether `COLORTERM` proves 24-bit colour, read as the `nika` binary reads
/// it (`nika_cli_host::output::truecolor_env`); a display capability, not a
/// secret, under the same allow as [`term`].
#[allow(clippy::disallowed_methods)]
fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|value| value == "truecolor" || value == "24bit")
}

fn main() -> ExitCode {
    let Flags {
        mut options,
        pace,
        diagnostic,
        surface,
    } = match parse(std::env::args().skip(1)) {
        Ok(flags) => flags,
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "{message}");
            return ExitCode::from(2);
        }
    };
    options.term = term();
    options.truecolor = truecolor();
    let ran = if let Some(kind) = surface {
        app::run(Surfaced { kind, prepared: 0 }, options)
    } else {
        let demo = Demo {
            script: Script::demo(),
            pace,
            refusal: diagnostic.then(diagnostic_words).flatten(),
        };
        app::run(demo, options)
    };
    match ran {
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
#[allow(clippy::expect_used, clippy::panic)]
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
            surface,
        } = parsed(&["--focus", "--color", "--panic-after", "2"]).expect("valid");
        assert_eq!(options.presentation, Presentation::Focus);
        assert!(options.color);
        assert!(!options.ascii && !options.reduced_motion);
        assert_eq!(options.panic_after, Some(2));
        let welcome = parsed(&["--workspace", "--ascii", "--reduced-motion"]).expect("valid");
        assert_eq!(welcome.options.presentation, Presentation::Workspace);
        assert!(welcome.options.ascii && welcome.options.reduced_motion);
        assert!(!welcome.options.color, "colour is asked, never assumed");
        assert_eq!(pace, Duration::ZERO, "the demo answers at once by default");
        assert!(!diagnostic);
        assert_eq!(surface, None, "the canned conversation by default");
        for (word, kind) in [
            ("intelligence", Surface::Intelligence),
            ("multi", Surface::Multi),
            ("choice", Surface::Choice),
            ("hours", Surface::Hours),
            ("hours-closed", Surface::HoursClosed),
            ("working", Surface::Working),
            ("decision", Surface::Decision),
            ("gate", Surface::Gate),
        ] {
            let flags = parsed(&["--demo-surface", word]).expect("a surface");
            assert_eq!(flags.surface, Some(kind));
        }
        assert!(parsed(&["--demo-surface", "consent"]).is_err());
        assert!(
            parsed(&["--ascii"])
                .expect("the glyph column")
                .options
                .ascii
        );
        assert!(parsed(&["--demo-surface"]).is_err());
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

    /// The demo cannot name a question's identity: a typed answer reaches
    /// none of its doors (its held turn would say so on the busy row), plays
    /// no scripted turn, and comes back exactly.
    #[test]
    fn a_typed_answer_reaches_no_door_of_the_demo() {
        let mut demo = Demo {
            script: Script::demo(),
            pace: Duration::from_millis(30),
            refusal: None,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let (queue, _) = std::sync::mpsc::sync_channel(1);
        let seen = nika_tui::session::feed::Seen::new(queue, std::sync::Arc::default());
        let remaining = demo.script.remaining();
        let turn = demo.answer_bound(" eur ", "witness-1", &tx, &seen);
        assert!(rx.try_recv().is_err(), "no door was used");
        assert_eq!(demo.script.remaining(), remaining, "no scripted turn");
        assert_eq!(turn.beats.last(), Some(&Beat::NotTaken(" eur ".to_owned())));
        assert!(turn.handoff.is_none());
    }

    /// Each surface opens on its own choice and says back exactly what it received, through
    /// either door.
    #[test]
    fn a_surface_opens_on_its_choice_and_says_back_what_it_received() {
        let mut intelligence = Surfaced {
            kind: Surface::Intelligence,
            prepared: 0,
        };
        let opened = intelligence.open();
        let Some(Beat::Wait(Waiting::Selection { asked, .. })) = opened.last() else {
            panic!("a settings list: {opened:?}")
        };
        let Shape::Choice(offers) = &asked.shape else {
            panic!("one choice")
        };
        assert_eq!(offers.len(), 30);
        let said = intelligence.submit("2 anthropic/claude-sonnet-5-5");
        assert_eq!(
            said.beats.first(),
            Some(&Beat::Say(Committed::new(
                Kind::Reply,
                "received: 2 anthropic/claude-sonnet-5-5"
            )))
        );
        let mut multi = Surfaced {
            kind: Surface::Multi,
            prepared: 0,
        };
        assert!(matches!(
            multi.open().last(),
            Some(Beat::Wait(Waiting::QuestionDocument { .. }))
        ));
        let (tx, _) = std::sync::mpsc::channel();
        let (queue, _) = std::sync::mpsc::sync_channel(1);
        let seen = nika_tui::session::feed::Seen::new(queue, std::sync::Arc::default());
        let answered = multi.answer_bound(r#"["notes","mail"]"#, "1:sources", &tx, &seen);
        assert_eq!(
            answered.beats.first(),
            Some(&Beat::Say(Committed::new(
                Kind::Reply,
                r#"received: ["notes","mail"]"#
            )))
        );
        // The decision ends on an act and waits again after anything else.
        let mut decision = Surfaced {
            kind: Surface::Decision,
            prepared: 0,
        };
        assert_eq!(decision.commands()[0], "/meaning");
        let asked = |turn: &Turn| match turn.beats.last() {
            Some(Beat::Wait(Waiting::Action { key, .. })) => Some(key.clone()),
            _ => None,
        };
        assert_eq!(asked(&decision.submit("/meaning")).as_deref(), Some("save"));
        let shown = decision.answer_bound("/show", "action:1", &tx, &seen);
        assert_eq!(asked(&shown).as_deref(), Some("save"));
        assert_eq!(
            decision
                .answer_bound("save & run", "action:1", &tx, &seen)
                .beats
                .last(),
            Some(&Beat::Wait(Waiting::Free))
        );
    }
}
