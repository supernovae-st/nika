// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation the Session's selected intelligence leads (ADR-153): the person's line goes
//! to one loop over the selected route and the Session's tools, and the loop's outcome comes
//! back as the Session's own — the questions asked with their identities, a proposal through the
//! consent door, a reply, a stop or a refusal. Saving and running stay the consent door's acts:
//! the person's own words can open it for the exact candidate they were shown, nothing else.
//!
//! An API or a local route is asked by Nika's own loop; a subscription seat chosen over ACP leads
//! with its agent's own loop ([`led`]), reaching the same tools over MCP. Both record the same
//! tree, obey the same Stop and steering, and end in the same outcomes.
//!
//! The tree is kept beside the conversation history (in memory without history) and the
//! conversation's evidence — values, provenance, delegations — with the history's state;
//! questions and proposals never survive a reopen. A host door keeps the round driver when
//! [`DRIVER_ENV`] says `rounds`, read once when it opens the Session; a route that cannot lead a
//! conversation (a seat over its native connection, no intelligence) keeps it too.

mod desk;
mod led;
mod store;
mod tools;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use nika_fs::OwnedDir;
use nika_providers::InferenceAdmission;
use nika_session_agent::conversation::{Acts, Conversation};
use nika_session_agent::{
    Agent, AgentEvent, EntryKind, Observed, Outcome, QueueMode, QueuedState, Relay, Steering, Tree,
};
use nika_session_change::tools::SessionTools;
use nika_session_intelligence::reasoner::agent_model::{AgentModel, tool_steps};
use nika_types::access::HarnessTransport;
use nika_types::cancel::CancelCtx;
use serde_json::Value;

use self::desk::{SessionDesk, Verified, Verifier};
use self::store::TreeFile;
use self::tools::{Decided, Toolbox};
use super::decision::{is_no, is_save_and_run, is_yes, local_command_of};
use super::{SessionRuntime, TurnOutcome};
use crate::change::{ProjectChangeSet, Witness};
use crate::outcome::{ProposalId, QuestionId, Refusal, RefusalClass, StopReach, Stopped};
use crate::work::{AskedQuestion, AskedState, Waiting, Work};

/// The name a host door reads once when it opens the Session: `rounds` keeps the round driver.
pub(super) const DRIVER_ENV: &str = "NIKA_SESSION_DRIVER";

/// The instructions every request of the conversation starts with.
const SYSTEM: &str = include_str!("../../assets/author_system.md");

/// The conversation the Session holds between turns: its tools (and the relay an ACP agent
/// reaches them through), its tree, the person's queued lines, the turn's Stop and the agent
/// leading it, when a seat's agent does.
pub(super) struct Driver {
    toolbox: Arc<Toolbox>,
    observed: Arc<Observed>,
    relay: Arc<Relay>,
    tree: Option<Tree>,
    store: TreeFile,
    damaged: Option<String>,
    authorized: Option<(ProposalId, Acts)>,
    steering: Steering,
    cancel: CancelCtx,
    leading: Option<led::Leading>,
    /// What the conversation's verifications kept, shared with every turn's desk: the verdicts
    /// that declined bytes and the last that made a document ready.
    verified: Arc<Mutex<Verified>>,
}

impl Driver {
    /// The driver a host door opens, unless the environment keeps the round driver.
    pub(super) fn from_env() -> Option<Self> {
        #[allow(clippy::disallowed_methods)]
        // a driver NAME, never a secret
        let word = std::env::var(DRIVER_ENV).unwrap_or_default();
        (!word.trim().eq_ignore_ascii_case("rounds")).then(|| Self::with_store(TreeFile::memory()))
    }

    fn with_store(store: TreeFile) -> Self {
        let toolbox = Arc::new(Toolbox::new(Conversation::default(), store.citations()));
        // Both loops reach the tools through one observer: each real call is a tool step.
        let observed = Arc::new(Observed::new(Arc::clone(&toolbox) as Arc<dyn SessionTools>));
        let relay = Arc::new(Relay::new(Arc::clone(&observed) as Arc<dyn SessionTools>));
        Self {
            toolbox,
            observed,
            relay,
            tree: None,
            store,
            damaged: None,
            authorized: None,
            steering: Steering::new(),
            cancel: CancelCtx::new(),
            leading: None,
            verified: Arc::default(),
        }
    }

    /// The driver of a Session whose history opened in `dir`: the tree kept there, read back
    /// whole (a damaged tree is reported, never reset), and the conversation's kept evidence.
    fn resumed(dir: io::Result<OwnedDir>, kept: Option<&Value>) -> Self {
        let mut driver = match dir {
            Ok(dir) => Self::with_store(TreeFile::home(dir)),
            Err(error) => {
                let mut driver = Self::with_store(TreeFile::memory());
                driver.damaged = Some(error.to_string());
                return driver;
            }
        };
        match driver.store.text() {
            Ok(None) => {}
            Ok(Some(text)) => match Tree::replay(&text) {
                Ok(tree) => {
                    driver.store.index(&text);
                    driver.tree = Some(tree);
                }
                Err(error) => driver.damaged = Some(error.to_string()),
            },
            Err(error) => driver.damaged = Some(error.to_string()),
        }
        let restored = kept.and_then(Conversation::restored).unwrap_or_default();
        driver.toolbox.with(|conversation| *conversation = restored);
        driver
    }

    /// The questions that wait for the person now.
    fn open(&self) -> Vec<AskedQuestion> {
        self.toolbox.with(|conversation| {
            (conversation.questions().iter())
                .filter(|q| q.state == AskedState::Open)
                .cloned()
                .collect()
        })
    }

    /// The Stop of the turn about to start.
    pub(super) fn stop_with(&mut self, cancel: CancelCtx) {
        self.cancel = cancel;
    }
}

fn refusal(class: RefusalClass, text: impl Into<String>) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(class, text))
}

fn could_not_start(why: &str) -> TurnOutcome {
    refusal(
        RefusalClass::Io,
        format!("the conversation could not start: {why}"),
    )
}

fn unix_ms() -> u64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}

/// The instructions every request of a conversation prepared with `chosen` starts with.
fn instructions(chosen: &str) -> String {
    format!(
        "{SYSTEM}\nThis Session: the person chose `{chosen}` to prepare with; offer the models of this machine through `models`."
    )
}

/// A new tree: its header, then the instructions every request starts with.
fn start_tree(store: &mut TreeFile, root: &std::path::Path, model: &str) -> Result<Tree, String> {
    let at = unix_ms();
    let project = blake3::hash(root.as_os_str().as_encoded_bytes())
        .to_hex()
        .to_string();
    let session = format!("s{at}");
    let mut tree = Tree::new(session, project, at, |line| {
        nika_session_agent::Store::append(&mut *store, line)
    })
    .map_err(|e| e.to_string())?;
    let system = EntryKind::System {
        text: instructions(model),
    };
    (tree.append(system, at, |line| {
        nika_session_agent::Store::append(&mut *store, line)
    }))
    .map_err(|e| e.to_string())?;
    Ok(tree)
}

/// The person's lines a run returned unsent, quoted; none when it returned none.
fn unsent(queued: &[(QueueMode, String)]) -> Option<String> {
    (!queued.is_empty()).then(|| {
        let lines: Vec<String> = queued
            .iter()
            .map(|(_, line)| format!("« {line} »"))
            .collect();
        format!("not sent: {}", lines.join(" · "))
    })
}

impl SessionRuntime {
    /// Whether the selected intelligence leads this Session's conversation: the host door kept
    /// the agent driver and the selected route can lead one.
    pub(super) fn agent_leads(&self) -> bool {
        self.agent.is_some()
            && (self.reasoner.agent_model().is_some() || self.agent_seat().is_some())
    }

    /// The subscription seat whose ACP agent leads the conversation: the seat the person chose,
    /// over the ACP connection they chose.
    fn agent_seat(&self) -> Option<String> {
        (self.reasoner.harness_transport() == HarnessTransport::Acp)
            .then(|| self.reasoner.authoring_harness())
            .flatten()
    }

    /// The person's lines queued while a turn of the conversation is under way: a steering line
    /// enters after the calls under way (an ACP agent is asked to stop and reads it next), a
    /// follow-up line when the turn would end, and Stop returns both unsent. None while no
    /// conversation is led.
    #[must_use]
    pub fn steering(&self) -> Option<Steering> {
        self.agent.as_ref().map(|driver| driver.steering.clone())
    }

    /// The line, when the conversation's intelligence takes it: a free line while no
    /// authoring round, run input, activation or paused run owns the next line, once an
    /// intelligence is chosen and ready. A command, an explicit run or activate line and the
    /// knowledge choice keep their own doors. The line meets the money admission and the
    /// knowledge hold every line that reaches a model meets. `None` leaves it to the round
    /// driver's turn.
    pub(super) fn agent_turn(&mut self, line: &str) -> Option<TurnOutcome> {
        let input = line.trim();
        let owned = self.authoring.is_some()
            || self.run_inputs.is_some()
            || self.activation.is_some()
            || self.pending_gate.is_some();
        let door = input.is_empty()
            || input.starts_with('/')
            || super::knowledge::is_choice(input)
            || super::schedule::is_activate(input)
            || self.local_run_line(line);
        let unready = !self.chosen || !self.intelligence.ready;
        if owned || door || unready || !self.agent_leads() {
            return None;
        }
        let answering = (self.agent.as_ref())
            .and_then(|d| d.tree.as_ref())
            .is_some_and(|tree| tree.parked().is_some());
        if let Err(refused) = self.admit_money(line, answering, false) {
            return Some(refused);
        }
        if let Some(held) = self.hold_for_knowledge(line, true) {
            return Some(held);
        }
        Some(self.agent_line(line))
    }

    /// One line through the conversation, under the same gates a reasoner call meets: the money
    /// that blocks cognition, an intelligence that reads, the effort the route can carry, and
    /// one admitted dispatch around the whole run (a seat's dispatch rides the subscription).
    fn agent_line(&mut self, line: &str) -> TurnOutcome {
        if let Some(why) = self.agent.as_ref().and_then(|d| d.damaged.clone()) {
            return refusal(
                RefusalClass::Io,
                format!(
                    "the conversation kept for this project cannot be read back ({why}) · nothing was reset · `{DRIVER_ENV}=rounds` opens the round driver"
                ),
            );
        }
        if self.money_blocks_cognition() {
            return self.cognition_money_refusal();
        }
        if !self.reads_answers() {
            return refusal(
                RefusalClass::NoIntelligence,
                "no intelligence reads this conversation · `/intelligence` chooses one",
            );
        }
        let effort = match self.authoring_context.reasoning_asked() {
            Ok(effort) => effort,
            Err(why) => {
                return refusal(
                    RefusalClass::NotAllowed,
                    format!("{why} · nothing was sent"),
                );
            }
        };
        let model = self.reasoner.agent_model();
        let seat = model.is_none().then(|| self.agent_seat()).flatten();
        if model.is_none() && seat.is_none() {
            return refusal(
                RefusalClass::WrongState,
                "this intelligence does not lead a conversation",
            );
        }
        let Some(mut driver) = self.agent.take() else {
            return refusal(
                RefusalClass::WrongState,
                "no conversation is open in this Session",
            );
        };
        let (account, entered) = match self.enter_dispatch(model.as_ref().map(AgentModel::model)) {
            Ok(pair) => pair,
            Err(why) => {
                self.agent = Some(driver);
                return refusal(
                    RefusalClass::NotAllowed,
                    format!("{why} · nothing was sent"),
                );
            }
        };
        let outcome = match (model, seat) {
            (Some(mut model), _) => {
                if let Some(effort) = effort {
                    model = model.with_effort(effort);
                }
                // The verifier's calls are admitted under the line's own dispatch, as the
                // author's are.
                let judged = account.clone();
                if let Some(account) = account {
                    model = model.with_admission(account);
                }
                self.drive(&mut driver, line, &mut model, judged)
            }
            (None, Some(seat)) => self.drive_led(&mut driver, line, &seat, effort),
            (None, None) => refusal(
                RefusalClass::WrongState,
                "this intelligence does not lead a conversation",
            ),
        };
        self.agent = Some(driver);
        self.leave_paid_dispatch(entered);
        outcome
    }

    /// A turn of the conversation begins: its tree started when none was, the tools given the
    /// turn's capabilities, and the questions a waiting run asked answered by the line about to
    /// be cited. The desk verifies a document under `account`, the line's admitted dispatch.
    /// Returns the questions asked before the line.
    fn agent_begin(
        &self,
        driver: &mut Driver,
        chosen: &str,
        account: Option<InferenceAdmission>,
    ) -> Result<Vec<QuestionId>, String> {
        if driver.tree.is_none() {
            let started = start_tree(&mut driver.store, &self.snapshot.root, chosen)?;
            driver.tree = Some(started);
        }
        let probes = (self.census.as_ref()).map(|c| c.provider_context.clone());
        let desk = SessionDesk {
            root: self.snapshot.root.clone(),
            probes: probes.unwrap_or_default(),
            knowledge: self.authoring_context.knowledge().cloned(),
            verifier: Verifier {
                seat: self.seat.clone(),
                context: self.authoring_context.clone(),
                account,
                kept: Arc::clone(&driver.verified),
            },
        };
        let asker = self.questions.asker();
        let mint = move |context: &str| QuestionId::new(Witness::of(context.as_bytes()).0, &asker);
        driver.toolbox.begin(Box::new(desk), Box::new(mint));
        driver
            .observed
            .watch(self.progress.listener().map(tool_steps));
        let waiting = (driver.tree.as_ref())
            .filter(|tree| tree.parked().is_some())
            .map(Tree::next_cite);
        Ok(driver.toolbox.with(|conversation| {
            let before = conversation.asked_ids();
            if let Some(cite) = waiting {
                conversation.answered_by(&cite);
            }
            before
        }))
    }

    /// The run over the selected API or local route, by Nika's own loop: a line that answers
    /// the call the conversation waits on, or a new prompt.
    fn drive(
        &mut self,
        driver: &mut Driver,
        line: &str,
        model: &mut AgentModel,
        account: Option<InferenceAdmission>,
    ) -> TurnOutcome {
        // A seat's agent no longer leads once the route is an API or a local one.
        driver.leading = None;
        let before = match self.agent_begin(driver, model.model(), account) {
            Ok(before) => before,
            Err(why) => return could_not_start(&why),
        };
        let now = unix_ms;
        let Driver {
            toolbox,
            observed,
            tree,
            store,
            steering,
            cancel,
            ..
        } = &mut *driver;
        let Some(tree) = tree.as_mut() else {
            toolbox.end();
            return refusal(RefusalClass::Io, "the conversation could not start");
        };
        let parked = tree.parked().is_some();
        let mut events = |_: AgentEvent| {};
        let mut agent = Agent::new(tree, store, &**observed, &now)
            .with_steering(steering)
            .with_cancel(cancel);
        let outcome = if parked {
            agent.answer(line, model, &mut events)
        } else {
            agent.prompt(line, model, &mut events)
        };
        let reach = agent.stop_reach();
        drop(agent);
        self.agent_end(driver, before, (outcome, reach))
    }

    /// The turn ended: what it decided, the questions no longer asked closed, the request read
    /// again, and the run's outcome as the Session's own.
    fn agent_end(
        &mut self,
        driver: &mut Driver,
        before: Vec<QuestionId>,
        (outcome, reach): (Outcome, Option<StopReach>),
    ) -> TurnOutcome {
        driver.observed.watch(None);
        let decided = driver.toolbox.end();
        let (still, since) = driver
            .toolbox
            .with(|conversation| (conversation.asked_ids(), conversation.since()));
        for id in before.into_iter().filter(|id| !still.contains(id)) {
            self.questions.close(Some(id));
        }
        let request = (driver.store.citations().lock())
            .map(|c| c.stated(since))
            .unwrap_or_default();
        self.intent.goal = request.lines().next().map(str::to_owned);
        self.settle_run(driver, (outcome, reach), decided)
    }

    /// The run's outcome as the Session's own.
    fn settle_run(
        &mut self,
        driver: &mut Driver,
        (outcome, reach): (Outcome, Option<StopReach>),
        decided: Decided,
    ) -> TurnOutcome {
        let said = driver
            .tree
            .as_ref()
            .map(Tree::last_said)
            .unwrap_or_default();
        match outcome {
            Outcome::Parked { queued, .. } => {
                // A question waits: the proposal shown is no longer what the next line answers.
                self.pending = None;
                let open = driver.open();
                let key = open.first().map(|q| q.key.clone()).unwrap_or_default();
                let mut text: Vec<String> = Vec::new();
                text.extend((!said.is_empty()).then_some(said));
                text.extend(open.iter().map(|q| q.question.clone()));
                text.extend(unsent(&queued));
                TurnOutcome::Question {
                    key,
                    question: text.join("\n"),
                }
            }
            Outcome::Answered { text } => {
                let current = driver.toolbox.with(|c| c.candidate().map(|c| c.number));
                match decided.proposed {
                    Some(revision) if current == Some(revision) => {
                        self.agent_propose(driver, &text, decided.acts)
                    }
                    _ => TurnOutcome::Reply(text),
                }
            }
            Outcome::Stopped { .. } => {
                let unsent = (driver.steering.records().into_iter())
                    .filter(|queued| queued.state == QueuedState::Returned)
                    .collect();
                let candidate = driver.toolbox.with(|c| c.candidate().map(|c| c.number));
                let reach = reach.unwrap_or(StopReach::BetweenSteps);
                TurnOutcome::Stopped(Stopped::new(reach, unsent, candidate))
            }
            Outcome::Failed { error } => refusal(
                RefusalClass::IntelligenceRefused,
                format!("the conversation could not go on: {error} · what was said is kept"),
            ),
            _ => refusal(
                RefusalClass::IntelligenceRefused,
                "the conversation ended unread",
            ),
        }
    }

    /// The candidate the intelligence proposed, as the proposal the consent door answers: its
    /// exact bytes at a fresh destination, the same preview and identity as any proposal. The
    /// acts the person's words authorized for it are kept for the turn's door to perform.
    fn agent_propose(
        &mut self,
        driver: &mut Driver,
        said: &str,
        acts: Option<Acts>,
    ) -> TurnOutcome {
        let Some(candidate) = driver.toolbox.with(|c| c.candidate().cloned()) else {
            return TurnOutcome::Reply(said.to_owned());
        };
        let root = self.snapshot.root.clone();
        let Some(path) = crate::review::destination(&root, &candidate.source) else {
            return refusal(
                RefusalClass::AuthoringRefused,
                "the candidate has no representable destination in the project · nothing was proposed",
            );
        };
        let at = path.display().to_string();
        let set = match ProjectChangeSet::workflow_at(
            &root,
            &candidate.summary,
            &at,
            candidate.source.clone(),
        ) {
            Ok(set) => set,
            Err(error) => return TurnOutcome::Refusal(Refusal::from_change(&error)),
        };
        let bytes = self.draft_preview(&set);
        let id = ProposalId::of(&bytes);
        self.bind_proposal_money(&id);
        self.pending = Some(set);
        driver.authorized = acts.map(|acts| (id.clone(), acts));
        TurnOutcome::Proposal {
            id,
            preview: format!("{said}\n\n{bytes}"),
        }
    }

    /// The consent the person's own words gave for the proposal this turn made, taken once:
    /// the turn's door answers it through the consent door (`save & run`, or `yes` to save).
    pub(super) fn agent_authorized(&mut self) -> Option<(ProposalId, &'static str)> {
        let (id, acts) = self.agent.as_mut()?.authorized.take()?;
        let word = if acts.run { "save & run" } else { "yes" };
        (self.pending_proposal().as_ref() == Some(&id)).then_some((id, word))
    }

    /// What the next line answers when the conversation's intelligence asked: one question, or
    /// several together.
    pub(super) fn agent_waiting(&self) -> Option<Waiting> {
        let open = self.agent.as_ref()?.open();
        match open.as_slice() {
            [] => None,
            [one] => Some(Waiting::Question {
                key: one.key.clone(),
                id: one.id.clone(),
            }),
            many => Some(Waiting::Questions {
                ids: many.iter().map(|q| q.id.clone()).collect(),
            }),
        }
    }

    /// The first question the conversation's intelligence asks now.
    pub(super) fn agent_question_id(&self) -> Option<QuestionId> {
        self.agent.as_ref()?.open().first().map(|q| q.id.clone())
    }

    /// Whether `id` is a question the conversation's intelligence asks now.
    pub(super) fn agent_asks(&self, id: &QuestionId) -> bool {
        (self.agent.as_ref()).is_some_and(|d| d.open().iter().any(|q| q.id == *id))
    }

    /// Whether the conversation's intelligence reads a line typed at a proposal: open
    /// language, never the consent protocol (`yes` · `no` · `save & run` · a command · `show`).
    pub(super) fn agent_reads_at_consent(&self, line: &str) -> bool {
        let line = line.trim();
        let protocol = is_yes(line)
            || is_no(line)
            || is_save_and_run(line)
            || line == "show"
            || line.starts_with('/')
            || local_command_of(line).is_some();
        self.agent_leads() && !line.is_empty() && !protocol
    }

    /// The work snapshot with what the conversation holds.
    pub(super) fn with_agent(&self, work: Work) -> Work {
        match &self.agent {
            Some(driver) => driver
                .toolbox
                .with(|conversation| {
                    work.with_conversation(
                        conversation.bindings().to_vec(),
                        conversation.delegations().to_vec(),
                        conversation.questions().to_vec(),
                    )
                })
                .with_queued(driver.steering.records()),
            None => work,
        }
    }

    /// What a history keeps of the conversation, once one started: a Session the round driver
    /// leads keeps its records' bytes unchanged.
    pub(super) fn agent_kept(&self) -> Option<Value> {
        (self.agent.as_ref())
            .filter(|d| d.tree.is_some())
            .map(|d| d.toolbox.with(|conversation| conversation.kept()))
    }

    /// The conversation a history resumes, in the history's own directory.
    pub(super) fn resume_agent(&mut self, dir: io::Result<OwnedDir>, kept: Option<&Value>) {
        if self.agent.is_some() {
            self.agent = Some(Driver::resumed(dir, kept));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
