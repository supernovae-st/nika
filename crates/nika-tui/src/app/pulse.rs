// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The live line while a turn works, and the line that says how it ended.
//!
//! Preparing: « Preparing · 45s · 12 author calls · ~$0.12 · Ctrl+C stop », then
//! the Session's own phase words: the cost is the sum of the calls that returned priced, as
//! the Session settled them (« + 2 unpriced » beside it when some returned with no price, and
//! nothing at all while none is priced). Other work leads with its own words
//! (« reading what the run left · 3s · Ctrl+C twice leaves »). A run the
//! turn drives, followed typed:
//! « Running veille-tech.nika · 12s · 6/14 tasks · now: pick
//! (claude-sonnet-5-5) · $0.012 of $0.25 · Ctrl+C twice leaves » (a Run is
//! never stopped here). Once that run settled, while the turn goes on, the
//! line says how it ended, as its card and the run's header do, never that it
//! runs: « veille-tech.nika · Failed after 16.5s · 3 tasks ran · 1 failed ·
//! $0.0340 · 1 model call », then the Session's phase words. Every fact is one the shell
//! observed: the Session's typed calls, the run's typed frames, its own clock;
//! an unknown one is left out, never a placeholder. The facts are composed at
//! most twice a second; the busy marker keeps its own motion, none under
//! reduced motion. At the end, a preparation that called its author says
//! « Prepared in 2m 10s · 35 calls · $0.91 » (« Stopped after … » once the
//! person stopped it) where no activity card already says it.

use std::io;
use std::time::{Duration, Instant};

use nika_display::activity::{Activity, CallCost, CallState};
use nika_display::run_story::RunState;
use nika_display::state::{RunView, TaskRow, TaskState};

use super::Shell;
use crate::lang::Voice;
use crate::model::{Beat, Committed, Conversation, Kind, Presentation};
use crate::render::money;
use crate::workspace::live::LiveRun;

/// The least time between two compositions of the live facts.
const REFRESH: Duration = Duration::from_millis(500);

/// How long other work runs before its line says its time and what two
/// presses of `Ctrl+C` do: a short turn keeps its own words alone.
const LASTS: Duration = Duration::from_secs(2);

/// What one turn's live line observed so far.
#[derive(Debug)]
pub(super) struct Pulse {
    started: Instant,
    /// The workflow author's physical calls, counted as the activity card
    /// counts them: each start is a call, and a return first seen returned.
    calls: u32,
    running: Option<u32>,
    /// What the calls that returned priced settled, in nano-USD, and how many returned with no
    /// price: as the Session stated each call's cost, never estimated here.
    settled: (i128, u32),
    unpriced: u32,
    /// When the shell saw the turn ask a run, which the line then follows.
    run_since: Option<Instant>,
    /// When the facts were last composed, and what they said.
    composed: Option<(Instant, String)>,
    /// The language and glyph column its own words take.
    voice: Voice,
}

impl Pulse {
    /// A turn that began at `started`.
    pub(super) fn new(started: Instant) -> Self {
        Self {
            started,
            calls: 0,
            running: None,
            settled: (0, 0),
            unpriced: 0,
            run_since: None,
            composed: None,
            voice: Voice::default(),
        }
    }

    /// The same pulse, its own words said in `voice` (English by default).
    pub(super) fn speaking(mut self, voice: Voice) -> Self {
        self.voice = voice;
        self
    }

    /// One typed activity of the Session.
    pub(super) fn observe(&mut self, activity: &Activity) {
        let Some(call) = &activity.call else {
            return;
        };
        if call.state == CallState::Started || self.running != Some(call.ordinal) {
            self.calls += 1;
        }
        self.running = (call.state == CallState::Started).then_some(call.ordinal);
        if call.state == CallState::Finished {
            match call.cost {
                Some(CallCost::Priced(nano)) => {
                    self.settled = (self.settled.0.saturating_add(nano), self.settled.1 + 1);
                }
                Some(CallCost::Unpriced) => self.unpriced += 1,
                _ => {}
            }
        }
    }

    /// What the finished calls settled, as the line says it: `~$0.12`, with `+ 2 unpriced` when
    /// some returned with no price; nothing while no call is priced.
    fn cost(&self) -> Option<String> {
        let (nano, priced) = self.settled;
        if priced == 0 {
            return None;
        }
        let spent = money(crate::session::spend::usd(nano));
        Some(match self.unpriced {
            0 => format!("~{spent}"),
            n => (self.voice.say(UNPRICED))
                .replace("{spent}", &spent)
                .replace("{n}", &n.to_string()),
        })
    }

    /// The turn asked a run at `now`: the line follows it from then on.
    pub(super) fn run_asked(&mut self, now: Instant) {
        self.run_since = Some(now);
    }

    /// Whether the turn asked a run the shell observed.
    pub(super) fn ran(&self) -> bool {
        self.run_since.is_some()
    }

    /// The author calls counted.
    #[cfg(test)]
    pub(super) fn calls(&self) -> u32 {
        self.calls
    }

    /// The live line at `now`, composed again at most every [`REFRESH`]:
    /// the run `leg` the desk follows, whether `Ctrl+C` stops the
    /// preparation (`stop`), and the Session's own phase words.
    pub(super) fn line(
        &mut self,
        now: Instant,
        leg: Option<&LiveRun>,
        stop: bool,
        phases: &str,
    ) -> String {
        if let Some((at, said)) = &self.composed
            && now.saturating_duration_since(*at) < REFRESH
        {
            return said.clone();
        }
        let voice = self.voice;
        let mut facts = Vec::new();
        match (leg, self.run_since) {
            // One truth per screen: a settled run reads as it ended, never as running.
            (Some(leg), Some(_)) if leg.reported().is_some() => {
                facts.push(file_name(leg.workflow()).to_owned());
                facts.push(ran(leg, voice).unwrap_or_else(|| voice.say(PAUSED).to_owned()));
                if !phases.is_empty() {
                    facts.push(phases.to_owned());
                }
                facts.push(voice.say(TWICE_LEAVES).to_owned());
            }
            (Some(leg), Some(since)) => {
                facts.push(
                    voice
                        .say(RUNNING)
                        .replace("{file}", file_name(leg.workflow())),
                );
                facts.push(clock(now.saturating_duration_since(since)));
                run_facts(leg.run_view(), &mut facts, voice);
                facts.push(voice.say(TWICE_LEAVES).to_owned());
            }
            // A preparation Stop reaches leads with its own facts, the
            // Session's phase words after them; any other work leads with
            // its own words, as a narrow row keeps its head.
            _ if stop => {
                facts.push(voice.say("Preparing").to_owned());
                facts.push(clock(now.saturating_duration_since(self.started)));
                if self.calls > 0 {
                    facts.push(voice.count(counted(self.calls), "author call"));
                }
                facts.extend(self.cost());
                facts.push(voice.say("Ctrl+C stop").to_owned());
                if !phases.is_empty() {
                    facts.push(phases.to_owned());
                }
            }
            _ => {
                let what = if phases.is_empty() {
                    voice.say("Working")
                } else {
                    phases
                };
                facts.push(what.to_owned());
                // Its time and what two presses do once it lasts (2 s).
                let took = now.saturating_duration_since(self.started);
                if took >= LASTS {
                    facts.push(clock(took));
                    if self.calls > 0 {
                        facts.push(voice.count(counted(self.calls), "author call"));
                    }
                    facts.extend(self.cost());
                    facts.push(voice.say(TWICE_LEAVES).to_owned());
                }
            }
        }
        let said = facts.join(" · ");
        self.composed = Some((now, said.clone()));
        said
    }

    /// How a preparation that called its author ended, after `took`, having
    /// spent `spent` USD as the Session estimates it: `None` without a call.
    /// One the person `stopped` says so, never that it prepared.
    pub(super) fn prepared(
        &self,
        took: Duration,
        spent: Option<f64>,
        stopped: bool,
    ) -> Option<String> {
        if self.calls == 0 || self.ran() {
            return None;
        }
        let ended = self.voice.say(if stopped {
            "Stopped after"
        } else {
            "Prepared in"
        });
        let mut facts = vec![
            format!("{ended} {}", took_words(took)),
            self.voice.count(counted(self.calls), "call"),
        ];
        facts.extend(spent.filter(|usd| *usd > 0.0).map(money));
        Some(facts.join(" · "))
    }
}

/// How a run that settled ended, as the live line says it while its turn ends: in
/// what time, its tasks as the run's result counts them ([`tally`]), its
/// metered spend at the result's precision ([`run_money`]) and its model
/// calls, from the settlement it declared (else the fold of its frames). A
/// paused run, whose gate still asks, has none yet. The conversation keeps
/// the Session's summary of the run instead ([`super::run_card`]).
pub(super) fn ran(leg: &LiveRun, voice: Voice) -> Option<String> {
    ran_after(leg, Spent::default(), voice)
}

/// What the legs before a resumed leg spent, as their settlements declared it: their metered
/// total and their model calls. A run a gate split is one run, its cost every leg's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Spent {
    pub(super) usd: Option<f64>,
    pub(super) calls: u32,
}

/// What the legs `leg` continued spent, followed along each one's `resumed_from` among the
/// legs observed here (`past`); a link not observed here ends the walk.
pub(super) fn before(leg: &LiveRun, past: &[LiveRun]) -> Spent {
    let mut total = Spent::default();
    let mut next = leg.resumed_from().map(str::to_owned);
    let mut links = 0;
    while let Some(run) = next.take() {
        links += 1;
        let earlier = (past.iter()).find(|earlier| earlier.trace_id().as_deref() == Some(&run));
        let Some((earlier, settled)) = earlier
            .and_then(|earlier| Some((earlier, earlier.settlement()?)))
            .filter(|_| links <= past.len())
        else {
            break;
        };
        let spend = &settled.spend;
        total.usd = match (total.usd, spend.total_cost_usd) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
        let calls = spend.priced_calls.saturating_add(spend.unpriced_calls);
        total.calls = total.calls.saturating_add(calls);
        next = earlier.resumed_from().map(str::to_owned);
    }
    total
}

/// [`ran`] for a resumed leg, counting what the legs `before` it spent.
pub(super) fn ran_after(leg: &LiveRun, before: Spent, voice: Voice) -> Option<String> {
    let settled = leg.settlement()?;
    let view = leg.run_view();
    let (word, timed) = match settled.state {
        // The person's « no » at a gate stopped it: never « ran », though its settlement says so.
        RunState::Succeeded if leg.refused_gate() => {
            ("Stopped at your no", "Stopped at your no after")
        }
        RunState::Succeeded => ("Ran", "Ran in"),
        RunState::Failed => ("Failed", "Failed after"),
        RunState::Cancelled => ("Cancelled", "Cancelled after"),
        RunState::Paused => return None,
        _ => ("Settled", "Settled after"),
    };
    let took = settled
        .elapsed_ms
        .or_else(|| Some(view.elapsed_ms).filter(|ms| *ms > 0));
    let mut facts = vec![match took {
        Some(ms) => format!(
            "{} {}",
            voice.say(timed),
            took_words(Duration::from_millis(ms))
        ),
        None => voice.say(word).to_owned(),
    }];
    let rows = view.rows();
    if rows.is_empty() {
        let tasks = (settled.tasks.as_ref()).map_or(0, |tally| tally.total);
        if tasks > 0 {
            facts.push(voice.count(counted(tasks), "task"));
        }
    } else {
        facts.extend(tally(rows, voice));
    }
    let spend = &settled.spend;
    let usd = (spend.total_cost_usd).or_else(|| (view.priced_calls > 0).then_some(view.cost_usd));
    let usd = match (usd, before.usd) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
    facts.extend(usd.map(run_money));
    let calls =
        (spend.priced_calls.saturating_add(spend.unpriced_calls)).saturating_add(before.calls);
    if calls > 0 {
        facts.push(voice.count(counted(calls), "model call"));
    }
    Some(facts.join(" · "))
}

impl<C: Conversation + 'static> Shell<C> {
    /// A run's story was just committed: once per run (a story committed again adds none), what
    /// it spent counts in the session's spend, and its steps follow its story where they say
    /// more than the Session's summary of it ([`super::run_card::card`]).
    pub(super) fn card_run(&mut self) {
        let Some(leg) = self.desk.live.as_ref() else {
            return;
        };
        let execution = leg.execution();
        if execution.is_none() || execution == self.carded {
            return;
        }
        // A paused leg is no end yet: its gate still asks, and its spend counts with its resume.
        let Some(settled) = (leg.settlement()).filter(|settled| settled.state != RunState::Paused)
        else {
            return;
        };
        self.carded = execution;
        // A run a gate split is one run: what its paused legs spent counts with it.
        let before = before(leg, &self.desk.past);
        let metered = settled.spend.total_cost_usd;
        self.run_spent += metered.unwrap_or_default() + before.usd.unwrap_or_default();
        let Some(card) = super::run_card::card(leg, self.state.voice()) else {
            return;
        };
        let (state, desk, composer) = (&mut self.state, &self.desk, &self.composer);
        crate::scroll::preserve_reading(state, desk, composer, |state| {
            let card = crate::render::own(&card, state.ascii);
            state.apply(Beat::Say(Committed::new(Kind::Report, card)));
        });
    }

    /// What this session spent so far, as known: the Session's preparation
    /// estimate and the runs' metered spend, nothing while neither is known.
    pub(super) fn spend_so_far(&mut self) {
        let prepared = self.conversation.as_ref().and_then(Conversation::spend);
        let ran = (self.run_spent > 0.0).then_some(self.run_spent);
        let spent = match (prepared, ran) {
            (Some(prepared), Some(ran)) => Some(prepared + ran),
            (prepared, ran) => prepared.or(ran),
        };
        self.state.spent = spent.filter(|usd| *usd > 0.0).map(money);
    }

    /// The turn ended after `took`: a preparation that called its author says
    /// once how it ended (`stopped` by the person, or not), with what it spent
    /// since `before` (the Session's estimate), where no activity card already
    /// says it (the workspace's).
    pub(super) fn say_prepared(
        &mut self,
        took: Duration,
        before: Option<f64>,
        stopped: bool,
    ) -> io::Result<()> {
        let Some(pulse) = self.pulse.take() else {
            return Ok(());
        };
        if self.state.presentation == Presentation::Workspace {
            return Ok(());
        }
        let after = self.conversation.as_ref().and_then(Conversation::spend);
        let spent = match (before, after) {
            (Some(before), Some(after)) => Some(after - before),
            (None, after) => after,
            (Some(_), None) => None,
        };
        let Some(line) = pulse
            .speaking(self.state.voice())
            .prepared(took, spent, stopped)
        else {
            return Ok(());
        };
        let line = crate::render::own(&line, self.state.ascii);
        self.keep_reading(|state, _| state.transcript.push(Committed::new(Kind::Notice, line)));
        self.commit_inline()
    }
}

/// Tasks done of all, the task under way with its model, and the spend
/// against the ceiling, as the run's own frames folded them.
fn run_facts(view: &RunView, facts: &mut Vec<String>, voice: Voice) {
    let rows = view.rows();
    if !rows.is_empty() {
        let done = (rows.iter())
            .filter(|row| {
                matches!(
                    row.state,
                    TaskState::Ok | TaskState::Failed | TaskState::Skipped | TaskState::Cancelled
                )
            })
            .count();
        let tasks = (voice.say("{done}/{all} tasks"))
            .replace("{done}", &done.to_string())
            .replace("{all}", &rows.len().to_string());
        facts.push(tasks);
    }
    let under_way =
        (rows.iter()).find(|row| matches!(row.state, TaskState::Running | TaskState::Retrying));
    if let Some(row) = under_way {
        let now = match row.model.as_deref() {
            Some(model) => voice
                .say("now: {id} ({model})")
                .replace("{model}", file_name(model)),
            None => voice.say("now: {id}").to_owned(),
        };
        facts.push(now.replace("{id}", &row.id));
    }
    let spent = (view.priced_calls > 0).then_some(view.cost_usd);
    match (spent, view.ceiling_usd) {
        (Some(spent), Some(ceiling)) => {
            let against = (voice.say("{spent} of {ceiling}"))
                .replace("{spent}", &money(spent))
                .replace("{ceiling}", &money(ceiling));
            facts.push(against);
        }
        (Some(spent), None) => facts.push(money(spent)),
        _ => {}
    }
}

/// A run's tasks as its result counts them (« 9 tasks ran (6 from cache) · 2 skipped »): the
/// tasks that ran, those a cache answered, then those skipped, failed or never run, one count
/// per state, never the settlement's records beside them (live gauntlet r02: « 20 tasks » on
/// the card against « 9 tasks ran (6 from cache) · 2 skipped » in its result).
pub(super) fn tally(rows: &[TaskRow], voice: Voice) -> Vec<String> {
    let count = |keep: &dyn Fn(&TaskRow) -> bool| rows.iter().filter(|row| keep(row)).count();
    let ran = count(&|row| row.state == TaskState::Ok && !row.cached);
    let cached = count(&|row| row.state == TaskState::Ok && row.cached);
    let mut head = voice.count(ran, "task ran");
    if cached > 0 {
        let from = voice
            .say("({cached} from cache)")
            .replace("{cached}", &cached.to_string());
        head = format!("{head} {from}");
    }
    let skipped = count(&|row| row.state == TaskState::Skipped);
    let failed = count(&|row| row.state == TaskState::Failed);
    let unrun = count(&|row| matches!(row.state, TaskState::Cancelled | TaskState::Pending));
    let mut facts = vec![head];
    for (counted, unit) in [(skipped, "skipped"), (failed, "failed"), (unrun, "not run")] {
        if counted > 0 {
            facts.push(voice.count(counted, unit));
        }
    }
    facts
}

/// A run's metered spend at the precision its result states it (« cost · $0.0046 »), so the
/// card and the result read one figure (live gauntlet r09 B12: « $0.005 » against « cost ·
/// $0.0046 », « <$0.001 » against « cost · $0.0004 »).
pub(super) fn run_money(usd: f64) -> String {
    format!("${usd:.4}")
}

/// The last part of a `/`-separated name: a workflow's file, a model's name.
pub(super) fn file_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// A count of the run's or the turn's facts, as [`Voice::count`] reads it.
pub(super) fn counted(count: u32) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}

/// The words of a run under way, `{file}` its workflow.
const RUNNING: &str = "Running {file}";

/// What two presses of `Ctrl+C` do while a run goes on.
const TWICE_LEAVES: &str = "Ctrl+C twice leaves";

/// A settled run paused at its gate, on the live line.
const PAUSED: &str = "paused · a gate asks you";

/// The settled calls' cost beside those that returned with no price.
const UNPRICED: &str = "~{spent} + {n} unpriced";

/// Whole seconds while they count: `45s`, `2m 10s`.
pub(super) fn clock(took: Duration) -> String {
    let secs = took.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m {:02}s", secs / 60, secs % 60)
    }
}

/// A time that ended: tenths under a minute (`16.5s`), whole seconds past it.
/// The tenths round as the Session words a run's time (the milliseconds over a
/// thousand, to one decimal), so the run's report and the Session's summary of
/// the same run read one duration: 9857 ms is `9.9s` here and « 9.9 s » there.
#[allow(clippy::cast_precision_loss)] // a display magnitude with one decimal
pub(super) fn took_words(took: Duration) -> String {
    if took.as_secs() < 60 {
        format!("{:.1}s", took.as_millis() as f64 / 1000.0)
    } else {
        clock(took)
    }
}

#[cfg(test)]
mod tests;
