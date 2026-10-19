// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The live conversation: the real session runtime behind the renderer's
//! beats (UX-2).
//!
//! Every `TurnOutcome` of [`nika_session::runtime::SessionRuntime`] maps to
//! the beats of [`crate::model`] one to one, and the composer's line goes to
//! `choose` · `consent` · `answer_gate` · `turn` by the same state the plain
//! loop of the CLI reads (a `yes` under `reply ›` is an answer, never a
//! consent). A run keeps the plain path: the session asks for a
//! [`Handoff`], the shell hands the terminal back, the caller's runners do
//! the run through the very path `nika run` owns, and the observation comes
//! back through `observe_run`. The optional reviewed runner retains the live
//! child and its frames across a distinct fresh Run cost question. That
//! question and the Session's one-time unknown-cost choice are both fresh
//! spending questions: typeahead from before they were painted is discarded,
//! an interruption cancels them without sending anything, and `details` reads
//! the same review's evidence without answering it.
//!
//! The native conversation requires a ready intelligence. Without one the
//! composer opens its current choices immediately, and keeps any interrupted
//! request until a usable choice resumes it through the runtime's `choose` law.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use nika_cli_host::lane::{PendingRun, RunProgress};
use nika_display::activity::Activity;
use nika_display::run_story::RunSink;
use nika_session::RunRequest;

use acquire::{ChildRead, Fetched, Proven};
use feed::{Feed, Seen};
use legs::{Leg, Legs};
use nika_display::run_story::{ChildRun, ExecutionId};
use nika_session::{KeptRun, Lifecycle, LifecycleFacts, RunFact, Stage};

use crate::lang::{Voice, fill};

/// A fresh Run may suspend at a child-owned cost question. Only the run's
/// story reaches the sender: no frame is typed, so the workspace cannot follow
/// the run, and its leg says so ([`RunReviewedObserved`] tells the frames).
pub type RunReviewed = Box<dyn Fn(&Path, &RunRequest, &Sender<String>) -> RunProgress + Send>;
/// [`RunReviewed`], the run told to the sink: its story, its frames typed
/// (`run_story::RunSink`).
pub type RunReviewedObserved = Box<dyn Fn(&Path, &RunRequest, &dyn RunSink) -> RunProgress + Send>;
use nika_session::intelligence::{IntelligenceCensus, UserIntelligencePreference};
use nika_session::outcome::ReviewId;
use nika_session::runtime::{ReasonerFactory, SessionRuntime, TurnOutcome};
use nika_session::work;

use crate::model::{
    Beat, Committed, Conversation, Handoff, Kind, Stopper, Stopping, Turn, Waiting,
};
use crate::workspace::candidate::Proposed;
use crate::workspace::header::Manifest;
use crate::workspace::inspect::Inspected;
use crate::workspace::project::{ProjectView, WorkflowView};
use nika_session::ProjectSnapshot;

/// The exit code and the trace a run left.
pub type RunOutcome = (u8, Option<PathBuf>);
/// One free check of a provider's API key, by the provider's id, sent only when the person
/// picks « Test the key » (`nika_session::reasoner::check_api_key` at the composition root):
/// what it observed, or why it could not run. Never the key.
pub type CheckKey = Box<dyn Fn(&str) -> Result<nika_session::KeyCheck, String> + Send>;
/// `nika run <workflow>` once, under the request's ceiling: (root, request).
pub type RunOnce = Box<dyn Fn(&Path, &RunRequest) -> RunOutcome + Send>;
/// `nika run --resume <trace> --answer <answer>`: (root, workflow, trace, answer).
pub type RunResume = Box<dyn Fn(&Path, &Path, &Path, &str) -> RunOutcome + Send>;

/// Where the runtime's progress lines go while a turn runs: the shell's
/// sink for the duration of one `submit_with`, nothing between turns.
type BusySlot = Arc<Mutex<Option<Feed>>>;

/// A run driven INSIDE the turn, the terminal never handed back: the
/// door executes through its own machine lane and hands each line of
/// the run's story to the busy sink as it happens; the exit code, the
/// trace the run left and the whole story come back for the observation
/// and the transcript. (root, work, sink). Only the story reaches the
/// sender: the workspace cannot follow the run, and its leg says so.
pub type RunTapped =
    Box<dyn Fn(&Path, &Work, &Sender<String>) -> (u8, Option<PathBuf>, Vec<String>) + Send>;
/// [`RunTapped`], the run told to the sink: its story, its frames typed.
pub type RunTappedObserved =
    Box<dyn Fn(&Path, &Work, &dyn RunSink) -> (u8, Option<PathBuf>, Vec<String>) + Send>;

/// The runner a run inside the turn goes to, in precedence order: a typed
/// review, a story-only review (fresh runs only), a typed tap, a story tap.
enum Runner<'a> {
    ReviewTyped(&'a RunReviewedObserved),
    Review(&'a RunReviewed),
    TapTyped(&'a RunTappedObserved),
    Tap(&'a RunTapped),
}

impl Runner<'_> {
    /// Whether this runner tells the run's frames, typed.
    fn typed(&self) -> bool {
        matches!(self, Self::ReviewTyped(_) | Self::TapTyped(_))
    }
}

/// The runners the CLI lends to the session. The two plain-path runners
/// print through the terminal the shell has handed back; the tapped
/// runner, when lent, keeps the terminal and the viewport shows the run.
pub struct Runners {
    /// `nika run <workflow>` once, under the request's ceiling.
    pub run_once: RunOnce,
    /// `nika run --resume <trace> --answer <answer>` on the same workflow.
    pub run_resume: RunResume,
    /// The same two runs inside the turn (the renderer's way when lent).
    pub run_tapped: Option<RunTapped>,
}

impl std::fmt::Debug for Runners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.run_tapped.is_some() {
            "Runners { run_once, run_resume, run_tapped }"
        } else {
            "Runners { run_once, run_resume }"
        })
    }
}

/// The work a run asks of the door: a run once, or a resume with the
/// human's answer to a gate.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Work {
    /// `nika run <workflow>` once, under the request's ceiling.
    Run(RunRequest),
    /// `nika run --resume <trace> --answer <answer>` on the same workflow.
    Resume {
        /// The workflow, relative to the root.
        workflow: PathBuf,
        /// The paused trace.
        trace: PathBuf,
        /// `task=value`, the human's own.
        answer: String,
    },
}

/// The live conversation.
pub struct Live {
    cwd: PathBuf,
    census: IntelligenceCensus,
    home: Option<PathBuf>,
    factory: Option<ReasonerFactory>,
    runtime: Option<SessionRuntime>,
    runners: Runners,
    run_review: Option<RunReviewed>,
    run_review_observed: Option<RunReviewedObserved>,
    run_tapped_observed: Option<RunTappedObserved>,
    /// The child waiting at its cost review, under the review's identity the Session holds, and
    /// whether its runner tells the frames (its answer is told the same way).
    pending_run: Option<(ReviewId, Box<PendingRun>, bool)>,
    pending: Option<(u64, Work)>,
    next_id: u64,
    busy: BusySlot,
    /// The candidate under review, folded when the last turn ended: what the
    /// workspace shows, and the identity a consent typed here answers.
    candidate: Option<Proposed>,
    /// What this host relayed of each run leg: the only identities, paths
    /// and journals its fetch and proof may reach.
    legs: Arc<Mutex<Legs>>,
    /// The last run an earlier session kept (HOME history), as read at open.
    kept: Option<Result<KeptRun, String>>,
    /// The program the host named to run `nika:jq` in the observed room, given to every
    /// runtime this Live opens.
    jq: Option<nika_session::JqHelper>,
    /// True while the current turn's runner owns execution. Once it returns, Stop and
    /// corrections can reach any preparation triggered by observing its failure.
    run_started: Arc<AtomicBool>,
    /// HOME display preferences, with no Session or Run authority.
    layout: layout::Store,
    /// The Session state the shell painted last (at its last prompt): a line typed there
    /// answers that state by its identity, or nothing ([`SessionRuntime::submit`]).
    shown: work::Waiting,
    /// Presentation scope for selections; runtime identity remains the strong shown token.
    question_epoch: u64,
    /// The list the native intelligence picker shows, with no selection authority.
    intelligence_view: intelligence::View,
    /// The words that guide that list while it is open ([`crate::model::Asked`]'s guide):
    /// its own navigation says them in its head, never into the conversation.
    picker_guide: Option<String>,
    /// The shell draws in the ASCII glyph column: the words built here for the screen take it.
    ascii: bool,
    /// The machine's zone, as the shell says it ([`Conversation::clock`]): the instants said here
    /// read on its clock. UTC until the shell says it, so no test reads the machine's.
    zone: jiff::tz::TimeZone,
    /// The model the person asked for the next workflows' AI steps, as said to the author.
    run_requested: Option<String>,
    /// The host's key check, when it lends one; sent only when the person picks it.
    check_key: Option<CheckKey>,
    /// The last key check the person asked for: the provider and what the check observed.
    key_checked: Option<(String, Result<nika_session::KeyCheck, String>)>,
    /// The key variables each provider has set, by name, as the census probed them.
    key_names: Vec<intelligence::keys::KeyNames>,
    /// Exact finite decision painted; no authority beyond the Session door.
    action: Option<actions::Pending>,
    /// The known preparation spend, in nano-USD, the Session observed when this
    /// conversation opened: what [`Conversation::spend`] counts from.
    spend_base: i128,
    /// What the trigger of each declared schedule decided, as the Session read it last
    /// ([`SessionRuntime::schedule_history`]): the rail, the list and the conversation read it.
    schedules: Vec<nika_session::ScheduleHistory>,
    /// What each schedule's line in the conversation last said, by its label.
    schedules_said: Vec<(String, crate::workspace::fired::Said)>,
    /// This conversation observed a run: the footer no longer tells an earlier session's run
    /// (live gauntlet 5, r09 B13: « Run ○ (précédent ×) » after a green run here, once a save
    /// reset the Run field).
    ran_here: bool,
    /// The language this machine's locale names, as the composition root read it
    /// ([`Self::with_locale`]): the TUI's own words take it before the person's words say one.
    locale: nika_session::Lang,
    /// The language the person's newest line that says one is written in, in this process.
    heard: Option<nika_session::Lang>,
}

/// The language the TUI's own words take: the one the person's newest line said (`heard`),
/// else the one the Session resumed from their earlier lines (a French history), else this
/// machine's `locale` (live gauntlet 6, r04 and r10: the top bar and the first screen spoke
/// English until the first French line).
fn speaking(
    runtime: Option<&SessionRuntime>,
    heard: Option<nika_session::Lang>,
    locale: nika_session::Lang,
) -> nika_session::Lang {
    let resumed = (runtime.map(SessionRuntime::language))
        .filter(|lang| *lang != nika_session::Lang::default());
    heard.or(resumed).unwrap_or(locale)
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live")
            .field("cwd", &self.cwd)
            .field("open", &self.runtime.is_some())
            .finish_non_exhaustive()
    }
}

impl Live {
    /// A conversation over `cwd`. `kept` is the intelligence choice found
    /// under the home, when one exists; without a usable choice the opening
    /// screen guides the person to the intelligence this conversation needs.
    #[must_use]
    pub fn new(
        cwd: PathBuf,
        census: IntelligenceCensus,
        kept: Option<UserIntelligencePreference>,
        home: Option<PathBuf>,
        factory: ReasonerFactory,
        runners: Runners,
    ) -> Self {
        let layout = layout::Store::open(home.as_deref());
        let key_names = intelligence::keys::names_of(&census.provider_context);
        let mut live = Self {
            cwd,
            census,
            home,
            factory: Some(factory),
            runtime: None,
            runners,
            run_review: None,
            run_review_observed: None,
            run_tapped_observed: None,
            pending_run: None,
            pending: None,
            next_id: 1,
            busy: Arc::new(Mutex::new(None)),
            candidate: None,
            legs: Arc::default(),
            kept: None,
            jq: None,
            run_started: Arc::default(),
            layout,
            shown: work::Waiting::Free,
            question_epoch: 0,
            intelligence_view: intelligence::View::Home,
            picker_guide: None,
            ascii: false,
            zone: jiff::tz::TimeZone::UTC,
            run_requested: None,
            check_key: None,
            key_checked: None,
            key_names,
            action: None,
            spend_base: 0,
            schedules: Vec::new(),
            schedules_said: Vec::new(),
            ran_here: false,
            locale: nika_session::Lang::default(),
            heard: None,
        };
        live.open_runtime(kept);
        live
    }

    /// Say the TUI's own words in `lang`, this machine's locale as the composition root reads
    /// it, until the person's words say a language: the top bar and the first screen speak it
    /// before the first line. A conversation the Session resumes in French speaks French.
    #[must_use]
    pub fn with_locale(mut self, lang: nika_session::Lang) -> Self {
        self.locale = lang;
        self
    }

    /// The language the TUI's own words take now ([`speaking`]).
    fn lang(&self) -> nika_session::Lang {
        speaking(self.runtime.as_ref(), self.heard, self.locale)
    }

    /// Supply actual monetary scope evidence for this host, before opening beats.
    #[must_use]
    pub fn with_cost_host_evidence(mut self, evidence: nika_session::CostHostEvidence) -> Self {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_cost_host_evidence(evidence);
        }
        self
    }

    /// Run the observed room's `nika:jq` steps through `helper`, a bounded process of the
    /// binary hosting this session; without one a jq step is never rehearsed.
    #[must_use]
    pub fn with_jq_helper(mut self, helper: nika_session::JqHelper) -> Self {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.with_jq_helper(helper.clone());
        }
        self.jq = Some(helper);
        self
    }

    /// Install and remove the trigger a declared schedule needs through `host`, on the person's
    /// own yes (D1): a person's own terminal on a Mac lends [`nika_session::LaunchdHost`].
    /// Without one the Session keeps [`nika_session::DeclaredOnly`]: a landed declaration stays
    /// declared, said with the command that installs it, and nothing is asked.
    #[must_use]
    pub fn with_trigger_host(mut self, host: Box<dyn nika_session::TriggerHost>) -> Self {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.with_trigger_host(host);
        }
        self
    }

    /// Lend the picker a key check (« Test the key »), sent only when the person picks it,
    /// never at open; without one the row is not offered.
    #[must_use]
    pub fn with_key_check(mut self, check: CheckKey) -> Self {
        self.check_key = Some(check);
        self
    }

    /// Keep a real child alive across a distinct fresh Run cost decision.
    #[must_use]
    pub fn with_run_review(mut self, runner: RunReviewed) -> Self {
        self.run_review = Some(runner);
        self
    }

    /// [`Self::with_run_review`], the run's frames told typed: it takes
    /// precedence over a story-only review.
    #[must_use]
    pub fn with_run_review_observed(mut self, runner: RunReviewedObserved) -> Self {
        self.run_review_observed = Some(runner);
        self
    }

    /// The run inside the turn, its frames told typed: it takes precedence
    /// over `Runners::run_tapped` (a review still goes first for a fresh run).
    #[must_use]
    pub fn with_run_tapped_observed(mut self, runner: RunTappedObserved) -> Self {
        self.run_tapped_observed = Some(runner);
        self
    }

    /// The runner `work` goes to inside the turn, when one is lent.
    fn runner(&self, work: &Work) -> Option<Runner<'_>> {
        let fresh = matches!(work, Work::Run(_));
        let review = (self.run_review_observed.as_ref().filter(|_| fresh))
            .map(Runner::ReviewTyped)
            .or_else(|| (self.run_review.as_ref().filter(|_| fresh)).map(Runner::Review));
        review
            .or_else(|| self.run_tapped_observed.as_ref().map(Runner::TapTyped))
            .or_else(|| self.runners.run_tapped.as_ref().map(Runner::Tap))
    }

    fn open_runtime(&mut self, pref: Option<UserIntelligencePreference>) {
        let Some(factory) = self.factory.take() else {
            return;
        };
        let mut runtime = match pref {
            Some(pref) => SessionRuntime::open_with(
                &self.cwd,
                self.census.clone(),
                &pref,
                self.home.as_deref(),
                factory,
            ),
            None => SessionRuntime::open_unchosen(
                &self.cwd,
                self.census.clone(),
                self.home.as_deref(),
                factory,
            ),
        };
        runtime.enable_continuous_preparation();
        if let Some(helper) = &self.jq {
            runtime.with_jq_helper(helper.clone());
        }
        // The plain loop prints progress lines to stdout; here the viewport
        // owns stdout: each typed activity reaches the shell's card and busy
        // row while the turn runs (`submit_observed` arms the sink), and is
        // dropped between turns. The typed hook replaces the line hook here,
        // so no activity is told twice.
        let slot = Arc::clone(&self.busy);
        runtime.on_activity(Arc::new(move |activity: &Activity| {
            if let Ok(guard) = slot.lock()
                && let Some(feed) = guard.as_ref()
            {
                feed.activity(activity);
            }
        }));
        self.question_epoch = asked::epoch();
        self.runtime = Some(runtime);
    }

    /// The beats that open a runtime: banner, history, restored state.
    fn opening_beats(&mut self) -> Vec<Beat> {
        let (heard, locale) = (self.heard, self.locale);
        let Some(runtime) = self.runtime.as_mut() else {
            return Vec::new();
        };
        let mut beats = vec![Beat::Say(Committed::new(Kind::Banner, runtime.banner()))];
        // The Session's own notice that the conversation was restored, when it gives one.
        let mut restored = false;
        match self.home.as_deref() {
            Some(home) => match runtime.enable_history(home) {
                Ok(Some(notice)) => {
                    restored = true;
                    beats.push(Beat::Say(Committed::new(Kind::Notice, notice)));
                }
                Ok(None) => {}
                Err(why) => {
                    beats.push(Beat::Say(Committed::new(Kind::Refusal, why.to_string())));
                    beats.push(Beat::Quit);
                    return beats;
                }
            },
            None => beats.push(Beat::Say(Committed::new(
                Kind::Notice,
                "conversation is temporary: no home directory is available",
            ))),
        }
        if let Some(notice) = runtime.restore_state() {
            // A restored live gate includes its exact question in this owner text.
            let kind = if matches!(runtime.waiting(), work::Waiting::Gate { .. }) {
                Kind::Gate
            } else {
                Kind::Notice
            };
            beats.push(Beat::Say(Committed::new(kind, notice)));
        }
        if !intelligence::ready(runtime) {
            let _ = runtime.turn("/intelligence");
            let guide = intelligence::guidance(runtime, speaking(Some(runtime), heard, locale));
            // The opening list's guide stands in its head while it is open, as every list's
            // does, so the welcome card never says a choice is pending once it is made (live
            // gauntlet, 16 October: « A message typed here waits for your choice. » stayed in
            // it after the choice). With no list to show, the conversation says what is missing.
            if intelligence::listed(runtime) && runtime.pending_choice() {
                self.picker_guide = Some(guide);
            } else {
                beats.push(Beat::Say(Committed::new(Kind::Notice, guide)));
            }
        }
        let voice = Voice {
            lang: speaking(Some(runtime), heard, locale),
            ascii: self.ascii,
        };
        beats.extend(earlier(
            runtime.kept_turns(),
            |index| runtime.kept_turn_is_human(index),
            (voice, restored),
        ));
        let kept = runtime.kept_run();
        if let Some(line) = kept_line(kept.as_ref(), voice) {
            beats.push(Beat::Say(Committed::new(Kind::Notice, line)));
        }
        if let (Some(Ok(run)), Ok(mut legs)) = (&kept, self.legs.lock()) {
            legs.kept(run);
        }
        self.kept = kept;
        if let Some(notice) = self.layout.notice() {
            beats.push(Beat::Say(Committed::new(Kind::Notice, notice)));
        }
        beats.extend(self.schedule_news());
        beats.extend(self.footer());
        beats.push(self.wait());
        beats
    }

    /// Read what each declared schedule's trigger decided ([`SessionRuntime::schedule_history`],
    /// a read only), then say in the conversation each schedule with news since its line was
    /// last said: at the opening, every one that decided anything or cannot be read.
    fn schedule_news(&mut self) -> Vec<Beat> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Vec::new();
        };
        let voice = Voice {
            lang: self.lang(),
            ascii: self.ascii,
        };
        self.schedules = runtime.schedule_history();
        let mut beats = Vec::new();
        for history in &self.schedules {
            let before = (self.schedules_said.iter())
                .find(|(label, _)| *label == history.label)
                .map(|(_, said)| *said);
            if crate::workspace::fired::news(before, crate::workspace::fired::state(history)) {
                let line = crate::workspace::fired::said(history, (voice, &self.zone));
                beats.push(Beat::Say(Committed::new(Kind::Notice, line)));
            }
        }
        self.schedules_said = (self.schedules.iter())
            .map(|history| {
                (
                    history.label.clone(),
                    crate::workspace::fired::state(history),
                )
            })
            .collect();
        beats
    }

    /// The rail and the status row, told apart from the run an earlier
    /// session kept ([`footer_beats`]); nothing while no runtime is open.
    fn footer(&self) -> Vec<Beat> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Vec::new();
        };
        let quiet = self.quiet_prompt();
        let voice = Voice {
            lang: runtime.language(),
            ascii: self.ascii,
        };
        // Declared is never active: the rail's activation field reads ● only once a fire of the
        // saved workflow's schedule is proved, and back to declared once its trigger is removed.
        let mut lifecycle = runtime.lifecycle();
        let saved = (runtime.work().saved).map(|saved| saved.workflow.display().to_string());
        let schedule =
            (saved.as_deref()).and_then(|path| crate::workspace::fired::of(&self.schedules, path));
        if lifecycle.active == Stage::Declared
            && schedule.is_some_and(crate::workspace::fired::proved)
        {
            lifecycle.active = Stage::Working;
        }
        // Still declared, its ◐ says which: declared only, or a firer standing that has not fired.
        let standing = (lifecycle.active == Stage::Declared)
            .then(|| crate::workspace::fired::standing(schedule, voice));
        // An earlier session's run is told only while this conversation observed none.
        let kept = self.kept.as_ref().filter(|_| !self.ran_here);
        let [rail, status] = footer_beats(lifecycle, runtime.status_line(), kept, (quiet, voice));
        let rail = match (rail, standing) {
            (Beat::Rail(words), Some(word)) => Beat::Rail(with_activation(&words, word)),
            (rail, _) => rail,
        };
        vec![rail, status]
    }

    /// The prompt the shell paints next, remembering the Session state it stands for: the
    /// shell sends a line only once this prompt is painted (its typeahead law), so the line
    /// answers exactly this state, by identity, or nothing.
    fn wait(&mut self) -> Beat {
        let snapshot = self.runtime.as_ref().map(SessionRuntime::work);
        self.shown = snapshot
            .as_ref()
            .map_or(work::Waiting::Free, |work| work.waiting.clone());
        let mut waiting = snapshot.as_ref().map_or(Waiting::Free, |work| {
            asked::capture(work, self.question_epoch)
        });
        if let (Some(snapshot), Some(runtime)) = (&snapshot, &self.runtime) {
            let voice = Voice {
                lang: self.lang(),
                ascii: self.ascii,
            };
            // The last check's verdict word, said in the language the picker speaks now.
            let checked = (self.key_checked.as_ref()).map(|(provider, checked)| {
                let (word, _) = intelligence::keys::verdict(provider, checked, voice);
                (provider.as_str(), word)
            });
            let shown = intelligence::Shown {
                view: &self.intelligence_view,
                run: self.run_requested.as_deref(),
                saved: runtime.operator_default(),
                jev: (runtime.jev_seat()).map(|seat| (seat.model(), seat.refusal())),
                keys: &self.key_names,
                testable: self.check_key.is_some(),
                checked: (checked.as_ref()).map(|(provider, word)| (*provider, word.as_str())),
                voice,
            };
            intelligence::project(&mut waiting, snapshot, &shown);
        }
        if let Waiting::Selection { asked } = &mut waiting {
            asked.guide.clone_from(&self.picker_guide);
        } else {
            self.picker_guide = None;
        }
        self.action = None;
        let voice = Voice {
            lang: self.lang(),
            ascii: self.ascii,
        };
        if let (Some(runtime), Some(snapshot)) = (&self.runtime, &snapshot)
            && let Some((pending, native)) = actions::capture(runtime, snapshot, voice)
        {
            self.action = Some(pending);
            waiting = native;
        }
        if !matches!(waiting, Waiting::Selection { .. }) {
            self.intelligence_view = intelligence::View::Home;
        }
        Beat::Wait(waiting)
    }

    /// Whether the runtime's prompt is quiet; a retained run keeps its own footer.
    fn quiet_prompt(&self) -> bool {
        self.runtime.as_ref().is_none_or(|runtime| {
            matches!(
                runtime.waiting(),
                work::Waiting::Free | work::Waiting::IntelligenceChoice
            )
        })
    }

    /// One outcome to beats, and the handoff it asks for.
    fn map(&mut self, outcome: TurnOutcome) -> (Vec<Beat>, Option<Handoff>) {
        let reply = footer::reply_label(&outcome);
        self.mapped(outcome, reply)
    }

    /// A Run's own observation to beats: its status is that Run's, never a
    /// reply beside a retained Run, so its footer takes no reply label. From
    /// now on the footer tells no earlier session's run.
    fn observed(&mut self, outcome: TurnOutcome) -> (Vec<Beat>, Option<Handoff>) {
        self.ran_here = true;
        self.mapped(outcome, None)
    }

    /// [`Self::map`] with the footer's `reply` label, as its caller scopes it.
    fn mapped(
        &mut self,
        outcome: TurnOutcome,
        reply: Option<&str>,
    ) -> (Vec<Beat>, Option<Handoff>) {
        let mut beats = Vec::new();
        // A turn that says something in the conversation (a picked row's result, a refusal)
        // takes the head of the intelligence list from the guide its navigation set there.
        let guides = matches!(&outcome, TurnOutcome::Facts(text) if text.is_empty())
            || matches!(&outcome, TurnOutcome::Ask(_));
        if !guides {
            self.picker_guide = None;
        }
        match outcome {
            TurnOutcome::Quit => return (self.quit_beats(), None),
            TurnOutcome::RunReviewed { review, approve } => {
                return (self.reviewed(&review, approve), None);
            }
            // Beside a spending decision that still waits, the Session's aside (the evidence, or
            // why the line answered nothing) is shown as the decision's own question.
            TurnOutcome::Aside(text) if self.shown.requires_fresh_input() => {
                beats.push(Beat::Say(Committed::new(Kind::Question, text)));
            }
            TurnOutcome::Reply(text)
            | TurnOutcome::Facts(text)
            | TurnOutcome::Help(text)
            | TurnOutcome::Aside(text) => {
                if !text.is_empty() {
                    beats.push(Beat::Say(Committed::new(Kind::Reply, text)));
                }
            }
            TurnOutcome::Ask(screen) => beats.extend(self.first_screen(screen)),
            // The choice landed and the waiting line resumed under it: the
            // choice's fact, then whatever that line became.
            TurnOutcome::Resumed { notice, outcome } => {
                beats.push(Beat::Say(Committed::new(Kind::Notice, notice)));
                let (rest, again) = self.map(*outcome);
                beats.extend(rest);
                return (beats, again);
            }
            TurnOutcome::Proposal { id, preview } => {
                beats.push(Beat::Say(Committed::proposal(id, preview)));
            }
            TurnOutcome::Held { preview, .. } => beats.push(self.held_changes(preview)),
            TurnOutcome::RunRequested { report, run } => {
                self.run_started.store(true, Ordering::Release);
                beats.push(Beat::Say(Committed::new(Kind::Report, report)));
                let label = format!(
                    "running `{}` once · ceiling ${}",
                    run.workflow.display(),
                    nika_display::vocab::stated_usd(run.max_cost_usd)
                );
                beats.push(Beat::Say(Committed::new(Kind::Report, label.clone())));
                let work = Work::Run(run);
                return self.run_or_handoff(work, label, beats);
            }
            TurnOutcome::Question { key, question, .. } => {
                beats.push(Beat::Say(self.question(&key, question)));
            }
            TurnOutcome::GateAsk { question, .. } => {
                beats.push(Beat::Say(Committed::new(Kind::Gate, question)));
            }
            TurnOutcome::ResumeRequested {
                workflow,
                trace,
                answer,
            } => {
                self.run_started.store(true, Ordering::Release);
                let label = format!("resuming `{}` with your answer", workflow.display());
                beats.push(Beat::Say(Committed::new(Kind::Report, label.clone())));
                let work = Work::Resume {
                    workflow,
                    trace,
                    answer,
                };
                return self.run_or_handoff(work, label, beats);
            }
            TurnOutcome::Refusal(why) => {
                beats.push(Beat::Say(Committed::new(Kind::Refusal, why.to_string())));
            }
            // The preparation stopped at the human's request: the Session's
            // own words; no proposal came of it and nothing waits for consent.
            TurnOutcome::Cancelled(note) => beats.push(Beat::Cancelled(note)),
            _ => {}
        }
        beats.extend(self.schedule_news());
        beats.extend(self.reply_footer(reply));
        beats.push(self.wait());
        (beats, None)
    }

    /// The first screen, in context or on `/intelligence`: the runtime now waits for the choice
    /// (`waiting()` reads it), and the list's own guide is its head's, never a message of the
    /// conversation (live gauntlet r02 to r09: « Choose an intelligence below. » stayed in the
    /// history). Without a choice waiting, the screen is said as a reply.
    fn first_screen(&mut self, screen: String) -> Option<Beat> {
        let lang = self.lang();
        let guide = (self.runtime.as_ref())
            .filter(|runtime| runtime.pending_choice())
            .map(|runtime| intelligence::guidance(runtime, lang));
        self.picker_guide = guide;
        self.picker_guide
            .is_none()
            .then(|| Beat::Say(Committed::new(Kind::Reply, screen)))
    }

    /// The exact bytes open on what they change in each file they replace, line by line
    /// against the file as it is now (live gauntlet r06 B8: « Inspect the exact changes »
    /// printed the file whole, and the one changed line had to be found).
    fn held_changes(&self, preview: String) -> Beat {
        let compared = self.runtime.as_ref().and_then(|runtime| {
            let candidate = runtime.work().candidate?;
            changes::against_saved(&runtime.snapshot.root, &candidate)
        });
        let text = match compared {
            Some(changes) => format!("{changes}\n\n{preview}"),
            None => preview,
        };
        Beat::Say(Committed::new(Kind::Proposal, text))
    }

    /// Dispatch an admitted Run or resume without adding another reply footer or prompt.
    fn run_or_handoff(
        &mut self,
        work: Work,
        label: String,
        mut beats: Vec<Beat>,
    ) -> (Vec<Beat>, Option<Handoff>) {
        if self.runner(&work).is_some() {
            beats.extend(self.run_inline(&work));
            (beats, None)
        } else {
            (beats, Some(self.keep(work, label)))
        }
    }

    /// Preserve the cost wording or capture the logical question's strong identity.
    fn question(&self, key: &str, question: String) -> Committed {
        if key == "unknown_cost" {
            return Committed::new(Kind::Question, authoring_cost_question(&question));
        }
        match self.runtime.as_ref() {
            // The trigger's decision follows the declaration that just landed: the Session's
            // report of what it wrote reads above the question; its options are the rows.
            Some(runtime) => match runtime.waiting_activation_choices() {
                Some((asked, _, options, _)) if asked == key && key == actions::INSTALL => {
                    Committed::new(Kind::Question, without_options(&question, &options))
                }
                // An activation question shown with its rows reads as its sentence and one short
                // line: its options are the rows, never repeated as text.
                Some((asked, said, ..)) if asked == key => Committed::new(Kind::Question, said),
                _ => asked::question(&runtime.work(), key, question, self.question_epoch),
            },
            None => Committed::new(Kind::Question, question),
        }
    }

    /// Leaving at a cost review drops the held child and sends nothing.
    fn quit_beats(&mut self) -> Vec<Beat> {
        let mut beats = match self.pending_run.take() {
            Some(_) => self.declined_run("Run cost decision cancelled; nothing sent."),
            None => Vec::new(),
        };
        beats.push(Beat::Quit);
        beats
    }

    fn keep(&mut self, work: Work, label: String) -> Handoff {
        let id = self.next_id;
        self.next_id += 1;
        self.pending = Some((id, work));
        Handoff { id, label }
    }

    /// The run inside the turn: the tapped runner executes while the busy
    /// row shows each line of the run's story; the story is then
    /// committed as one block, the observation follows (a result, a
    /// failure, or a gate that waits for the human), the prompt returns.
    fn run_inline(&mut self, work: &Work) -> Vec<Beat> {
        let root = self
            .runtime
            .as_ref()
            .map_or_else(|| self.cwd.clone(), |r| r.snapshot.root.clone());
        let feed = self.feed();
        // The request, before its child exists: the shell learns the workflow
        // and the exact bytes at that path now, never a run identity.
        let (workflow, resume) = match work {
            Work::Run(run) => (run.workflow.display().to_string(), false),
            Work::Resume { workflow, .. } => (workflow.display().to_string(), true),
        };
        let look = (self.runtime.as_ref()).and_then(|r| look::take(&r.snapshot, &workflow));
        let world = (self.runtime.as_ref()).and_then(|r| asked_world(&r.work(), &workflow));
        let Some(runner) = self.runner(work) else {
            return Vec::new();
        };
        let typed = runner.typed();
        feed.asked(workflow, resume, typed, (look, world));
        let progress = match (runner, work) {
            (Runner::ReviewTyped(review), Work::Run(run)) => review(&root, run, &feed),
            (Runner::Review(review), Work::Run(run)) => review(&root, run, feed.busy()),
            (Runner::TapTyped(tap), _) => RunProgress::Complete(tap(&root, work, &feed)),
            (Runner::Tap(tap), _) => RunProgress::Complete(tap(&root, work, feed.busy())),
            _ => return Vec::new(),
        };
        match progress {
            RunProgress::Complete(result) => self.run_result(result),
            RunProgress::Review(pending) => {
                let question = pending.question();
                // The Session holds the review; this host holds only the child (dropped, it
                // sends nothing).
                let Some(runtime) = self.runtime.as_mut() else {
                    return Vec::new();
                };
                let review = runtime.run_review_asked(&question, &pending.details());
                self.pending_run = Some((review, pending, typed));
                vec![
                    Beat::Say(Committed::new(Kind::Question, question)),
                    self.wait(),
                ]
            }
            _ => vec![Beat::Say(Committed::new(
                Kind::Refusal,
                "unsupported Run review response",
            ))],
        }
    }

    /// The sink of the turn under way: the shell's busy row and queue, or a
    /// row nobody reads between turns.
    fn feed(&self) -> Feed {
        let voice = Voice {
            lang: self.lang(),
            ascii: self.ascii,
        };
        (self.busy.lock().ok())
            .and_then(|guard| guard.clone())
            .unwrap_or_else(|| Feed::new(std::sync::mpsc::channel().0, None))
            .speaking(voice)
    }

    /// A declined (or interrupted) Run review: the child got no answer,
    /// nothing was sent, nothing ran — a typed « not run », never an
    /// observed exit code, so the status keeps the last real run.
    fn declined_run(&mut self, story: &str) -> Vec<Beat> {
        let mut beats = vec![Beat::Say(Committed::new(Kind::Run, story))];
        let Some(runtime) = self.runtime.as_mut() else {
            beats.push(Beat::Quit);
            return beats;
        };
        let outcome = runtime.observe_declined_run();
        let (more, _) = self.map(outcome);
        beats.extend(more);
        beats
    }

    fn run_result(
        &mut self,
        (code, trace, story): (u8, Option<PathBuf>, Vec<String>),
    ) -> Vec<Beat> {
        // Observation may begin model-led repair before this turn ends. The child has
        // returned, so that preparation must be stoppable through the existing token.
        self.run_started.store(false, Ordering::Release);
        let mut beats = Vec::new();
        if !story.is_empty() {
            beats.push(Beat::Say(Committed::new(Kind::Run, story.join("\n"))));
        }
        let Some(runtime) = self.runtime.as_mut() else {
            beats.push(Beat::Quit);
            return beats;
        };
        // The leg this host relayed for this request, when frames came: its
        // identity rides the observation into HOME history. What the stream
        // told the person rides it too: a run refused before its first frame
        // says why there alone, and the conversation hears the same reason.
        let leg = (self.legs.lock().ok()).and_then(|legs| legs.newest().map(Leg::kept_run));
        let outcome =
            runtime.observe_run_told(code, trace.as_deref(), leg.unwrap_or_default(), &story);
        let (more, again) = self.observed(outcome);
        beats.extend(more);
        if again.is_some() {
            self.pending = None;
            beats.push(Beat::Say(Committed::new(
                Kind::Notice,
                "the observation asked for another run; say « run it » again when you want it",
            )));
            beats.push(self.wait());
        }
        beats
    }
}

/// The review's own closing line, and the line that also names `details`:
/// the same words as the first screen of a fresh Run cost question.
const REVIEW_CHOICE: &str = "Continue once? yes / no";
const CHOICES: &str = "Continue once? yes / no / details";
/// What the Session's history records as the person's side of its own note on a recovery.
const RECOVERY: &str = "(recovery)";

/// The recent turns HOME history kept, repainted as history: what was said in an earlier
/// session, never replayed. One notice says the conversation was kept (live gauntlet 6, r02:
/// one card said the recovery three times, in English and French): the Session's, when it said
/// the conversation was `restored`, else a header of the TUI's own in `voice`; the Session's
/// notes on a recovery (`(recovery)`) are no turn of the conversation and say what its notice
/// says, so they are not repainted.
fn earlier(
    turns: &[(String, String)],
    human: impl Fn(usize) -> bool,
    (voice, restored): (Voice, bool),
) -> Vec<Beat> {
    let kept: Vec<(usize, &(String, String))> = (turns.iter().enumerate())
        .filter(|(index, (said, _))| said != RECOVERY || human(*index))
        .collect();
    if kept.is_empty() {
        return Vec::new();
    }
    let mut beats = Vec::new();
    if !restored {
        beats.push(Beat::Say(Committed::new(
            Kind::Notice,
            voice.say("earlier in this conversation · kept in your history · nothing is replayed"),
        )));
    }
    for (index, (said, reply)) in kept {
        let input = if human(index) {
            Committed::new(Kind::Human, said.clone())
        } else {
            let context = voice.say("earlier context · {said}");
            Committed::new(Kind::Notice, fill(context, &[("{said}", said)]))
        };
        beats.push(Beat::Say(input));
        beats.push(Beat::Say(Committed::new(Kind::Reply, reply.clone())));
    }
    beats
}

/// The last run an earlier session kept, in one line: evidence of what was
/// observed then; its journal is verified again only when its proof opens.
fn kept_line(kept: Option<&Result<KeptRun, String>>, voice: Voice) -> Option<String> {
    Some(match kept? {
        Ok(run) => {
            let id: String = run
                .execution
                .as_deref()
                .unwrap_or("unrecorded")
                .chars()
                .take(13)
                .collect();
            let workflow = run.workflow.as_deref().unwrap_or("(not recorded)");
            let exit = run
                .exit
                .map_or_else(|| "not recorded".to_owned(), |e| e.to_string());
            let line = voice.say(
                "last run, observed in an earlier session · {id} of `{workflow}` · exit {exit} · its proof is read again from its journal when you open it · nothing replays",
            );
            fill(
                line,
                &[("{id}", &id), ("{workflow}", workflow), ("{exit}", &exit)],
            )
        }
        Err(why) => {
            let line = voice.say(
                "the last run's record is unreadable ({why}) · kept unchanged · nothing replays",
            );
            fill(line, &[("{why}", why)])
        }
    })
}

/// The rail and the status row of this session, told apart from the last run
/// an earlier session kept. The rail names its stages in the person's language,
/// in the Session's words ([`nika_session::Lang::rail`]). While no run or gate
/// is observed here and nothing but a choice waits, the rail's Run field adds
/// that run's stage (« Run ○ (earlier ✓) ») and the status row opens with it
/// (« last run ✓ exit 0 in an earlier session · Saved · no current Run result ·
/// … »), first so that a narrow row keeps it. Checked and Run stay this
/// session's facts and the Session's words follow unchanged; a kept run without
/// an exit adds nothing.
fn footer_beats(
    lifecycle: Lifecycle,
    status: String,
    kept: Option<&Result<KeptRun, String>>,
    (quiet, voice): (bool, Voice),
) -> [Beat; 2] {
    let rail = voice.lang.rail(&lifecycle);
    let earlier = match kept {
        Some(Ok(run)) if quiet && lifecycle.run == Stage::Pending => {
            run.exit.map(|exit| (run.workflow.as_deref(), exit))
        }
        _ => None,
    };
    let Some((workflow, exit)) = earlier else {
        return [Beat::Rail(rail), Beat::Status(status)];
    };
    let mut facts = LifecycleFacts::new();
    facts.run = RunFact::Exit(exit);
    let glyph = Lifecycle::from_facts(&facts).run.glyph();
    // The status names the saved workflow; another one is named here.
    let other = workflow.filter(|w| !status.contains(&format!("`{w}`")));
    let phrase = voice.say(if other.is_some() {
        "last run of `{workflow}` {glyph} exit {exit} in an earlier session"
    } else {
        "last run {glyph} exit {exit} in an earlier session"
    });
    let (glyph, exit) = (glyph.to_string(), exit.to_string());
    let values = [
        ("{workflow}", other.unwrap_or("")),
        ("{glyph}", glyph.as_str()),
        ("{exit}", exit.as_str()),
    ];
    let note = fill(phrase, &values);
    let status = if status.is_empty() {
        note
    } else {
        format!("{note} · {status}")
    };
    let earlier = voice.say("{rail} (earlier {glyph})");
    let rail = fill(earlier, &[("{glyph}", glyph.as_str()), ("{rail}", &rail)]);
    [Beat::Rail(rail), Beat::Status(status)]
}

/// `rail` with `word` beside its activation field's glyph, the fourth of its five fields
/// (« Actif ◐ en attente »): the rest as the Session and the footer drew it.
fn with_activation(rail: &str, word: &str) -> String {
    let mut fields: Vec<String> = rail.split(" · ").map(str::to_owned).collect();
    if let Some(field) = fields.get_mut(3) {
        field.push(' ');
        field.push_str(word);
    }
    fields.join(" · ")
}

/// The Session's one-time unknown-cost question, told apart from Save and
/// Run and closing on the three choices; the Session's sentences stay whole.
fn authoring_cost_question(question: &str) -> String {
    let body = question
        .strip_suffix(REVIEW_CHOICE)
        .map_or(question, str::trim_end);
    format!(
        "Fresh authoring cost decision · this request only; approving it never saves or runs anything\n{body}\n{CHOICES}"
    )
}

impl Conversation for Live {
    fn arrangement(&self) -> Option<crate::workspace::geometry::Arrangement> {
        self.layout.current()
    }

    fn keep_arrangement(
        &mut self,
        arrangement: crate::workspace::geometry::Arrangement,
    ) -> Vec<Beat> {
        self.layout
            .keep(arrangement)
            .map(|notice| Beat::Say(Committed::new(Kind::Notice, notice)))
            .into_iter()
            .collect()
    }

    fn commands(&self) -> Vec<String> {
        // `/restore` completes only while a kept draft can be proposed again.
        let mut commands: Vec<String> = self
            .runtime
            .as_ref()
            .map_or_else(
                || nika_session::runtime::SLASH_COMMANDS.to_vec(),
                SessionRuntime::slash_commands,
            )
            .into_iter()
            .map(str::to_owned)
            .collect();
        // `/new` begins a new conversation (the Session's door, not its command list), before
        // `/quit`.
        if !commands.iter().any(|command| command == NEW) {
            let at = (commands.iter()).position(|command| command == "/quit");
            commands.insert(at.unwrap_or(commands.len()), NEW.to_owned());
        }
        commands
    }

    fn open(&mut self) -> Vec<Beat> {
        let beats = self.opening_beats();
        self.fold_candidate();
        self.spend_base = (self.runtime.as_ref())
            .and_then(|runtime| spend::prepared_nano(&runtime.cost_observations()))
            .unwrap_or(0);
        beats
    }

    fn spend(&self) -> Option<f64> {
        let now = spend::prepared_nano(&self.runtime.as_ref()?.cost_observations())?;
        let spent = now.checked_sub(self.spend_base)?;
        (spent > 0).then(|| spend::usd(spent))
    }

    fn language(&self) -> Option<nika_session::Lang> {
        self.runtime.as_ref().map(|_| self.lang())
    }

    fn glyphs(&mut self, ascii: bool) {
        self.ascii = ascii;
    }

    fn clock(&mut self, zone: &jiff::tz::TimeZone) {
        self.zone = zone.clone();
    }

    /// What the trigger of a declared schedule decided while the door stood open, read
    /// again from the Session's history of its arm ledger (a read only): its line once, and
    /// the rail with it.
    fn idle(&mut self) -> Vec<Beat> {
        let mut beats = self.schedule_news();
        if !beats.is_empty() {
            let rail = (self.footer().into_iter()).filter(|beat| matches!(beat, Beat::Rail(_)));
            beats.extend(rail);
        }
        beats
    }

    fn submit_with(&mut self, line: &str, busy: &Sender<String>) -> Turn {
        self.lent(Feed::new(busy.clone(), None), line)
    }

    fn submit_observed(&mut self, line: &str, busy: &Sender<String>, seen: &Seen) -> Turn {
        self.lent(Feed::new(busy.clone(), Some(seen.clone())), line)
    }

    fn answer_bound(
        &mut self,
        text: &str,
        witness: &str,
        busy: &Sender<String>,
        seen: &Seen,
    ) -> Turn {
        self.lent_answer(Feed::new(busy.clone(), Some(seen.clone())), text, witness)
    }

    fn delegate_bound(&mut self, witness: &str, busy: &Sender<String>, seen: &Seen) -> Turn {
        self.lent_delegation(Feed::new(busy.clone(), Some(seen.clone())), witness)
    }

    fn pick_bound(
        &mut self,
        keys: &str,
        witness: &str,
        busy: &Sender<String>,
        seen: &Seen,
    ) -> Turn {
        self.lent_pick(Feed::new(busy.clone(), Some(seen.clone())), keys, witness)
    }

    fn submit(&mut self, line: &str) -> Turn {
        let turn = self.answer(line);
        self.fold_candidate();
        turn
    }

    /// A spending decision waits (the Session's rule): only a line typed after it was shown
    /// answers it. Any other waiting state keeps its typeahead and grants nothing here.
    fn fresh_input_required(&self) -> bool {
        (self.runtime.as_ref()).is_some_and(|runtime| runtime.waiting().requires_fresh_input())
    }

    fn cancel_pending(&mut self) -> Vec<Beat> {
        let beats = self.cancelled();
        self.fold_candidate();
        beats
    }

    /// A fresh preparation token for the turn about to start (the Session's
    /// `begin_preparation_turn`). A turn answering the retained Run review
    /// arms none. While this turn hands a Run to its runner the stop cancels
    /// nothing: a Run keeps its own doors. After the runner returns, the same
    /// token can stop model-led preparation of a repair.
    fn stopper(&mut self) -> Option<Stopper> {
        if self.pending_run.is_some() {
            return None;
        }
        let preparation = self.runtime.as_mut()?.begin_preparation_turn();
        let run = Arc::new(AtomicBool::new(false));
        self.run_started = Arc::clone(&run);
        Some(Box::new(move || {
            if run.load(Ordering::Acquire) {
                Stopping::RunUnderway
            } else {
                preparation.cancel();
                Stopping::Requested
            }
        }))
    }

    /// A stop that raced the turn's result: the Session withdraws what its
    /// cancelled preparation left pending (never a gate, a Run or a cost
    /// decision, and no consent is recorded) and says so in its own words.
    fn withdraw_stopped(&mut self) -> Vec<Beat> {
        let Some(note) =
            (self.runtime.as_mut()).and_then(SessionRuntime::withdraw_cancelled_preparation)
        else {
            return Vec::new();
        };
        self.fold_candidate();
        // The same beat as a stop the turn itself answered: the card reads
        // « Stopped by you », never « Settled », above a withdrawn result.
        let mut beats = vec![Beat::Cancelled(note)];
        beats.extend(self.footer());
        beats.push(self.wait());
        beats
    }

    fn busy_label(&self, line: &str) -> Option<String> {
        if self.pending_run.is_some() {
            return Some("answering the fresh Run cost question".into());
        }
        let runtime = self.runtime.as_ref()?;
        if runtime.waiting_cost_choice() {
            return Some("answering the fresh authoring cost question".into());
        }
        let label = if runtime.pending_choice() && !self.converses(line) {
            "seating the intelligence you chose"
        } else if runtime.pending_proposal().is_some() {
            if line.trim().is_empty() {
                return None;
            }
            "reviewing your reply"
        } else if runtime.waiting_gate().is_some() {
            "answering the gate"
        } else if line.trim_start().starts_with('/') {
            return None;
        } else {
            "working through your words"
        };
        Some(label.to_owned())
    }

    fn perform(&mut self, handoff: &Handoff) -> Vec<Beat> {
        let beats = self.performed(handoff);
        self.fold_candidate();
        beats
    }

    fn project(&self) -> Option<ProjectView> {
        let runtime = self.runtime.as_ref()?;
        let view = project_view(runtime, self.home.as_deref());
        Some(view.scheduling(self.schedules.clone()))
    }

    /// The look (the crate-private `look::take`): only a workflow the runtime's snapshot
    /// lists, read once below its root. Nothing here joins the look to the
    /// conversation, its facts or a consent.
    fn inspect(&mut self, path: &str) -> Option<Inspected> {
        look::take(&self.runtime.as_ref()?.snapshot, path)
    }

    /// The candidate folded when the last turn ended (`candidate::take`).
    fn candidate(&self) -> Option<Proposed> {
        self.candidate.clone()
    }

    /// A file the leg `execution` reported writing (as this host relayed
    /// it), read now below the root; any other path is refused unread.
    fn fetch(&mut self, execution: &ExecutionId, path: &str) -> Option<Fetched> {
        let root = &self.runtime.as_ref()?.snapshot.root;
        let legs = self.legs.lock().ok()?;
        let leg = legs.find(execution);
        Some(match leg {
            Some(leg) if leg.written.iter().any(|w| w == path) => acquire::fetch(root, path),
            _ => Fetched::refused(path, "not a file this run reported writing"),
        })
    }

    /// The Proof of the journal the leg `execution` settled with, bound to
    /// what this host relayed of it (never a path the renderer names). A
    /// pure acquisition: a kept leg forgets what an earlier reading lent
    /// before the capture, and nothing is lent until [`Self::adopt`]. The
    /// ledger is not held while the journal is read and verified.
    fn prove(&mut self, execution: &ExecutionId) -> Option<Proven> {
        let root = self.runtime.as_ref()?.snapshot.root.clone();
        let asked = {
            let mut legs = self.legs.lock().ok()?;
            (legs.find_mut(execution)).map(|leg| {
                leg.forget_history();
                (leg.trace(), leg.proof_expectation())
            })
        };
        let proven = match asked {
            Some((Some(trace), expect)) => acquire::prove(&root, &trace, &expect),
            Some((None, _)) => return Some(Proven::refused("", "its settlement named no journal")),
            None => {
                return Some(Proven::refused(
                    "",
                    "this run's settlement was not observed here",
                ));
            }
        };
        if let Ok(mut legs) = self.legs.lock()
            && let Some(leg) = legs.find_mut(execution)
        {
            leg.captured(&proven);
        }
        Some(proven)
    }

    /// Lend the kept leg `execution` what `proven` (the reading this host
    /// captured last for it, a verified journal) records: its written names
    /// and child relations, for reading only.
    fn adopt(&mut self, execution: &ExecutionId, proven: &Proven) -> bool {
        let Ok(mut legs) = self.legs.lock() else {
            return false;
        };
        legs.find_mut(execution)
            .is_some_and(|leg| leg.adopt(proven))
    }

    /// The journal of the child run the leg `execution`'s task `task`
    /// called: only the relation this host kept for that task, and only
    /// while it is the one asked; anything else is refused unread.
    fn child(
        &mut self,
        execution: &ExecutionId,
        task: &str,
        relation: &ChildRun,
    ) -> Option<ChildRead> {
        let root = &self.runtime.as_ref()?.snapshot.root;
        let legs = self.legs.lock().ok()?;
        let trace = relation.trace_id.as_deref().unwrap_or_default();
        Some(match legs.find(execution).map(|leg| leg.child(task)) {
            Some(Some(kept)) if kept == relation => acquire::read_child(root, kept),
            Some(Some(_)) => {
                ChildRead::refused(trace, "the task's relation changed since it was asked")
            }
            Some(None) => {
                ChildRead::refused(trace, "no child relation this host kept for that task")
            }
            None => ChildRead::refused(trace, "this run's frames were not observed here"),
        })
    }

    /// The last run an earlier session kept, as read at open.
    fn kept_run(&self) -> Option<Result<KeptRun, String>> {
        self.kept.clone()
    }
}

impl Live {
    /// One submitted line with `feed` lent to the runtime for the turn.
    fn lent(&mut self, feed: Feed, line: &str) -> Turn {
        if let Ok(mut guard) = self.busy.lock() {
            *guard = Some(feed.with_legs(Arc::clone(&self.legs)));
        }
        let turn = self.submit(line);
        if let Ok(mut guard) = self.busy.lock() {
            *guard = None;
        }
        turn
    }

    /// Fold the Session's candidate once a turn, a performed work, a
    /// cancellation or the opening ended: on that turn's thread, never while
    /// the shell draws.
    fn fold_candidate(&mut self) {
        let voice = Voice {
            lang: self.lang(),
            ascii: self.ascii,
        };
        self.candidate = (self.runtime.as_ref())
            .and_then(|runtime| candidate::take(runtime, self.candidate.as_ref(), voice));
    }

    /// The Session decided the cost review this host shows: one yes answers the held child
    /// once; anything else drops it, and nothing is sent.
    fn reviewed(&mut self, review: &ReviewId, approve: bool) -> Vec<Beat> {
        let Some((held, pending, typed)) = self.pending_run.take() else {
            return vec![self.wait()];
        };
        if held != *review {
            self.pending_run = Some((held, pending, typed));
            return vec![self.wait()];
        }
        if !approve {
            drop(pending);
            return self.declined_run(
                "Run cost decision cancelled; nothing sent. Request Run again for a fresh review.",
            );
        }
        let feed = self.feed();
        let result = if typed {
            (*pending).answer_observed(true, &feed)
        } else {
            (*pending).answer(true, feed.busy())
        };
        self.run_result(result)
    }

    /// One submitted line to the state that waits for it.
    fn answer(&mut self, line: &str) -> Turn {
        // The person's words decide the language from here, as the Session reads them.
        if let Some(lang) = nika_session::Lang::of(line) {
            self.heard = Some(lang);
        }
        if line.trim() == NEW {
            return self.new_conversation();
        }
        if let Some(turn) = self.refused_connection(line) {
            return turn;
        }
        if let Some(turn) = self.converse_over_list(line) {
            return turn;
        }
        if let Some(outcome) = self.require_intelligence(line) {
            let refused = matches!(outcome, TurnOutcome::Refusal(_));
            let (mut beats, handoff) = self.map(outcome);
            if refused && !line.trim().starts_with('/') {
                beats.push(Beat::NotTaken(line.to_owned()));
            }
            return Turn { beats, handoff };
        }
        let choosing = matches!(self.shown, work::Waiting::IntelligenceChoice);
        let outcome = {
            let Some(runtime) = self.runtime.as_mut() else {
                return Turn {
                    beats: vec![Beat::Quit],
                    handoff: None,
                };
            };
            // The Session routes the line to what waits, by the identity this host showed at
            // the prompt the line was typed at: the candidate on screen answers a consent (none
            // shown: only leaving or declining goes through); a gate or a question answers only
            // the one painted, so a state that appeared since takes nothing from this line.
            let shown = match &self.shown {
                work::Waiting::Consent { .. } => {
                    match self.candidate.as_ref().filter(|shown| !shown.aside()) {
                        Some(shown) => work::Waiting::Consent {
                            proposal: shown.id().clone(),
                        },
                        None => work::Waiting::Free,
                    }
                }
                waiting => waiting.clone(),
            };
            runtime.submit(line, &shown)
        };
        let retained = choosing
            && self
                .runtime
                .as_ref()
                .is_some_and(SessionRuntime::pending_choice)
            && !line.trim().is_empty()
            && !line.trim().starts_with('/')
            && !nika_session::authoring::is_cancel(line);
        let (mut beats, handoff) = self.map(outcome);
        if retained {
            beats.push(Beat::NotTaken(line.to_owned()));
        }
        Turn { beats, handoff }
    }

    /// An interruption: a fresh spending question it cancels, nothing else.
    fn cancelled(&mut self) -> Vec<Beat> {
        if let Some(declined) = (self.runtime.as_mut()).and_then(SessionRuntime::decline_run_review)
        {
            return self.map(declined).0;
        }
        // The Session's own answer path: the interruption never becomes a yes.
        match (self.runtime.as_mut()).and_then(SessionRuntime::decline_cost_choice) {
            Some(declined) => self.map(declined).0,
            None => Vec::new(),
        }
    }

    /// The handed-off work, performed with the terminal handed back.
    fn performed(&mut self, handoff: &Handoff) -> Vec<Beat> {
        let Some((id, work)) = self.pending.take() else {
            return vec![self.wait()];
        };
        if id != handoff.id {
            return vec![
                Beat::Say(Committed::new(
                    Kind::Refusal,
                    "the terminal was handed back for work the session no longer holds",
                )),
                self.wait(),
            ];
        }
        let root = self
            .runtime
            .as_ref()
            .map_or_else(|| self.cwd.clone(), |r| r.snapshot.root.clone());
        let (code, trace) = match &work {
            Work::Run(run) => (self.runners.run_once)(&root, run),
            Work::Resume {
                workflow,
                trace,
                answer,
            } => (self.runners.run_resume)(&root, workflow, trace, answer),
        };
        self.run_started.store(false, Ordering::Release);
        let Some(runtime) = self.runtime.as_mut() else {
            return vec![Beat::Quit];
        };
        let outcome = runtime.observe_run(code, trace.as_deref());
        let (mut beats, again) = self.observed(outcome);
        if again.is_some() {
            self.pending = None;
            beats.push(Beat::Say(Committed::new(
                Kind::Notice,
                "the observation asked for another run; say « run it » again when you want it",
            )));
            beats.push(self.wait());
        }
        beats
    }
}

/// The project as this session observed it when it opened, lent read-only to
/// the workspace: the runtime's own snapshot in words, never a new walk of
/// the disk. The session runs in this process, so its host is `local`. No
/// run is pinned: the Session lends no typed identity of a run in flight or
/// paused (the gate's id names its trace and task, not its workflow).
fn project_view(runtime: &SessionRuntime, home: Option<&Path>) -> ProjectView {
    let snapshot = &runtime.snapshot;
    let root = &snapshot.root;
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| root.display().to_string());
    let workflows = snapshot
        .workflows
        .iter()
        .map(|w| WorkflowView::new(&w.path, w.name.as_deref(), w.clean, w.findings, w.tasks))
        .collect();
    let complete = !(snapshot.truncated || snapshot.walk_truncated);
    let view = ProjectView::new("local", name, shown_path(root, home))
        .with_git(snapshot.git_root.is_some())
        .governed(governing(snapshot, home))
        .listing(workflows, complete);
    match selection::seat(runtime) {
        Some(seat) => view.seated(seat),
        None => view,
    }
}

/// A path as the human reads it: home-relative under the home.
fn shown_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// What governs the root: the project file the Session's discovery found (in
/// the root, or in an ancestor named by its path from the root), none, or
/// one it refused.
fn governing(snapshot: &ProjectSnapshot, home: Option<&Path>) -> Manifest {
    if snapshot.project_error.is_some() {
        return Manifest::Refused;
    }
    let Some(file) = snapshot.project_file.as_deref() else {
        return Manifest::Absent;
    };
    let up = file
        .parent()
        .and_then(|dir| snapshot.root.ancestors().position(|a| a == dir));
    match (up, file.file_name()) {
        (Some(0), _) => Manifest::Here,
        (Some(up), Some(name)) => {
            Manifest::Above(format!("{}{}", "../".repeat(up), name.to_string_lossy()))
        }
        _ => Manifest::Above(shown_path(file, home)),
    }
}

/// The Session's `question` without the line that numbers its `options` as text
/// (`1 Install and activate · 2 Keep declared only`): the rows say them.
fn without_options(question: &str, options: &[work::Offer]) -> String {
    let labels: Vec<String> = (options.iter().enumerate())
        .map(|(at, offer)| format!("{} {}", at + 1, offer.label))
        .collect();
    let numbered = labels.join(" · ");
    (question.lines())
        .filter(|line| line.trim() != numbered)
        .collect::<Vec<_>>()
        .join("\n")
}

pub mod acquire;
mod actions;
mod new_conversation;
use new_conversation::NEW;
mod asked;
mod candidate;
mod changes;
pub mod feed;
mod footer;
mod intelligence;
mod layout;
pub(crate) mod legs;
mod look;
mod selection;
pub(crate) mod spend;
mod typed_answer;

/// The one audit fold of a look, for the workspace's own tests.
/// Where an asked workflow reaches, as the Session declared it: the requested run's check when it
/// names this workflow, else the bytes the last consent saved there. A workflow only named carries
/// none.
fn asked_world(
    work: &nika_session::work::Work,
    workflow: &str,
) -> Option<nika_session::world::World> {
    let named = |path: &Path| path == Path::new(workflow);
    (work.requested.as_ref())
        .filter(|requested| named(&requested.workflow))
        .map(|requested| requested.world.clone())
        .or_else(|| {
            (work.saved.as_ref())
                .filter(|saved| named(&saved.workflow))
                .and_then(|saved| saved.world.clone())
        })
}

#[cfg(test)]
pub(crate) fn judge_for_tests(path: &str, witness: String, source: &str) -> Inspected {
    look::judge(path.to_owned(), witness, source.to_owned())
}

#[cfg(all(test, unix))]
impl Live {
    /// The Session this conversation holds, read by the crate's own tests.
    pub(crate) fn session_for_tests(&self) -> Option<&SessionRuntime> {
        self.runtime.as_ref()
    }
}

#[cfg(all(test, unix))]
mod tests;

/// The project view a live session lends: its runtime's snapshot, in words.
#[cfg(all(test, unix))]
#[allow(clippy::expect_used, clippy::panic)]
mod project_view_tests {
    use super::*;
    use nika_session::ScriptedReasoner;

    /// A temporary directory, removed when the test ends.
    struct Room(PathBuf);

    impl Room {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let path = std::env::temp_dir()
                .join(format!("nika-tui-ws-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&path).expect("room");
            Self(path)
        }
    }

    impl Drop for Room {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A live conversation over `root` with no intelligence chosen: nothing
    /// reasons and nothing runs.
    fn live(root: &Path, home: Option<PathBuf>) -> Live {
        Live::new(
            root.to_path_buf(),
            IntelligenceCensus::empty(),
            None,
            home,
            Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
            Runners {
                run_once: Box::new(|_, _| panic!("nothing runs here")),
                run_resume: Box::new(|_, _, _, _| panic!("nothing resumes here")),
                run_tapped: None,
            },
        )
    }

    #[test]
    fn the_live_view_projects_the_snapshot_the_runtime_holds() {
        let room = Room::new("project");
        std::fs::create_dir_all(room.0.join(".git")).expect("a git root");
        std::fs::write(room.0.join("nika.yaml"), "nika: demo\n").expect("manifest");
        let project = room.0.join("ventures").join("one");
        std::fs::create_dir_all(&project).expect("a project below it");
        std::fs::write(
            project.join("alpha.nika"),
            "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("alpha");
        std::fs::write(
            project.join("beta.nika"),
            "nika: beta\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n",
        )
        .expect("beta");
        let view = live(&project, Some(room.0.clone()))
            .project()
            .expect("an open runtime lends its project");
        assert_eq!(
            (
                view.host.as_str(),
                view.name.as_str(),
                view.location.as_str()
            ),
            ("local", "one", "~/ventures/one")
        );
        assert_eq!(view.git, Some(true));
        assert_eq!(
            view.manifest,
            Some(Manifest::Above("../../nika.yaml".to_owned())),
            "the parent file governs, named where it is"
        );
        assert!(view.complete);
        let judged: Vec<(&str, Option<&str>, bool, usize)> = view
            .workflows
            .iter()
            .map(|w| (w.path.as_str(), w.name.as_deref(), w.clean, w.tasks))
            .collect();
        assert_eq!(
            judged,
            [
                ("alpha.nika", Some("alpha"), true, 1),
                ("beta.nika", Some("beta"), false, 1)
            ]
        );
        assert_eq!(view.seat, None, "no intelligence chosen, none named");
        assert!(view.pinned.is_none(), "no typed run identity is lent");
    }

    #[test]
    fn the_selected_model_keeps_its_bytes_and_uses_ascii_chrome() {
        use nika_session::intelligence::IntelligenceKind;
        let room = Room::new("model-chrome");
        for model in ["deepseek/selected-model", "deepseek/private-été·beta"] {
            let mut census = IntelligenceCensus::empty();
            census.api_keys.push("deepseek".into());
            let preference = UserIntelligencePreference::new(
                IntelligenceKind::Api {
                    provider: "deepseek".into(),
                },
                Some(model.into()),
            );
            let mut runtime = SessionRuntime::open_with(
                &room.0,
                census,
                &preference,
                None,
                Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
            );
            runtime.set_authoring_context(nika_session::authoring::AuthoringContext::default());
            assert!(
                runtime.intelligence_chosen(),
                "explicit preference is selected"
            );
            let seat = project_view(&runtime, None).seat.expect("selection");
            assert_eq!(seat, format!("{model} - deepseek API, metered"));
            assert_eq!(
                seat.is_ascii(),
                model.is_ascii(),
                "only model data may be Unicode"
            );
        }
    }

    /// A resolved preparation model; painting must never ask this reasoner.
    struct ResolvedModel(Option<String>);

    impl nika_session::SessionReasoner for ResolvedModel {
        fn name(&self) -> String {
            "fixture connection".to_owned()
        }

        fn reason(&mut self, _: &str) -> Result<nika_session::Reply, nika_session::ReasonError> {
            panic!("projecting a selection must not ask a model")
        }

        fn authoring_model(&self) -> Option<String> {
            self.0.clone()
        }
    }

    #[test]
    fn preparation_names_the_resolved_default_without_claiming_a_response() {
        use nika_session::intelligence::IntelligenceKind;
        let room = Room::new("resolved-model");
        for (local, configured, resolved, expected) in [
            (
                false,
                None,
                Some("deepseek/default"),
                "deepseek/default - deepseek API, metered; Decision: same model",
            ),
            (
                true,
                None,
                Some("ollama/local"),
                "ollama/local - ollama, on this machine; Decision: same model",
            ),
            (
                false,
                Some("deepseek/selected"),
                Some("deepseek/default"),
                "deepseek/selected - deepseek API, metered; Decision: same model",
            ),
            (
                false,
                None,
                None,
                "model chosen by provider - deepseek API, metered",
            ),
        ] {
            let mut census = IntelligenceCensus::empty();
            let kind = if local {
                census.locals.push("ollama".into());
                IntelligenceKind::Local {
                    provider: "ollama".into(),
                }
            } else {
                census.api_keys.push("deepseek".into());
                IntelligenceKind::Api {
                    provider: "deepseek".into(),
                }
            };
            let preference = UserIntelligencePreference::new(kind, configured.map(str::to_owned));
            let resolved = resolved.map(str::to_owned);
            let mut runtime = SessionRuntime::open_with(
                &room.0,
                census,
                &preference,
                None,
                Box::new(move |_| Box::new(ResolvedModel(resolved.clone()))),
            );
            // No operator decision seat: the author judges its own candidate.
            runtime.set_authoring_context(nika_session::authoring::AuthoringContext::default());
            assert_eq!(project_view(&runtime, None).seat.as_deref(), Some(expected));
            assert_eq!(runtime.intelligence.model.as_deref(), configured);
        }
    }

    #[test]
    fn a_refused_project_file_and_an_absent_one_are_named() {
        let room = Room::new("refused");
        std::fs::write(room.0.join("nika.yaml"), "not: [valid\n").expect("a broken file");
        let view = live(&room.0, None).project().expect("view");
        assert_eq!(view.manifest, Some(Manifest::Refused));
        let bare = Room::new("bare");
        let view = live(&bare.0, None).project().expect("view");
        assert_eq!(view.manifest, Some(Manifest::Absent));
        assert_eq!(view.git, Some(false));
        assert!(
            view.location.starts_with('/'),
            "no home given: the path stays whole: {}",
            view.location
        );
    }
}
