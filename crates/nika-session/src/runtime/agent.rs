// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation the Session's selected intelligence leads (ADR-153): the person's line goes
//! to one loop over the selected route and the Session's tools, and the loop's outcome comes
//! back as the Session's own — the questions asked with their identities, a proposal through the
//! consent door, a reply, a stop or a refusal. Saving and running stay the consent door's acts:
//! the person's own words can open it for the exact candidate they were shown, nothing else.
//!
//! The tree is kept beside the conversation history (in memory without history) and the
//! conversation's evidence — values, provenance, delegations — with the history's state;
//! questions and proposals never survive a reopen. A host door keeps the round driver when
//! [`DRIVER_ENV`] says `rounds`, read once when it opens the Session; a route that cannot lead a
//! conversation (a subscription seat, no intelligence) keeps it too.

mod conversation;
mod desk;
mod store;
mod tools;

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use nika_fs::OwnedDir;
use nika_session_agent::{Agent, AgentEvent, EntryKind, Outcome, Tree};
use nika_session_intelligence::reasoner::agent_model::AgentModel;
use serde_json::Value;

use self::conversation::{Acts, Conversation};
use self::desk::SessionDesk;
use self::store::TreeFile;
use self::tools::{Decided, Toolbox};
use super::decision::{is_no, is_save_and_run, is_yes, local_command_of};
use super::{SessionRuntime, TurnOutcome};
use crate::change::{ProjectChangeSet, Witness};
use crate::outcome::{ProposalId, QuestionId, Refusal, RefusalClass};
use crate::work::{AskedQuestion, AskedState, Waiting, Work};

/// The name a host door reads once when it opens the Session: `rounds` keeps the round driver.
pub(super) const DRIVER_ENV: &str = "NIKA_SESSION_DRIVER";

/// The instructions every request of the conversation starts with.
const SYSTEM: &str = include_str!("../../assets/author_system.md");

/// The conversation the Session holds between turns.
pub(super) struct Driver {
    conversation: Conversation,
    tree: Option<Tree>,
    store: TreeFile,
    damaged: Option<String>,
    authorized: Option<(ProposalId, Acts)>,
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
        Self {
            conversation: Conversation::default(),
            tree: None,
            store,
            damaged: None,
            authorized: None,
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
        driver.conversation = kept.and_then(Conversation::restored).unwrap_or_default();
        driver
    }

    /// The questions that wait for the person now.
    fn open(&self) -> Vec<&AskedQuestion> {
        (self.conversation.questions().iter())
            .filter(|q| q.state == AskedState::Open)
            .collect()
    }
}

fn refusal(class: RefusalClass, text: impl Into<String>) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(class, text))
}

fn unix_ms() -> u64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
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
    let text = format!(
        "{SYSTEM}\nThis Session: the person chose `{model}` to prepare with; offer the models of this machine through `models`."
    );
    let system = EntryKind::System { text };
    (tree.append(system, at, |line| {
        nika_session_agent::Store::append(&mut *store, line)
    }))
    .map_err(|e| e.to_string())?;
    Ok(tree)
}

impl SessionRuntime {
    /// Whether the selected intelligence leads this Session's conversation: the host door kept
    /// the agent driver and the selected route can lead one.
    pub(super) fn agent_leads(&self) -> bool {
        self.agent.is_some() && self.reasoner.agent_model().is_some()
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
    /// one admitted dispatch around the whole run.
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
        let Some(mut model) = self.reasoner.agent_model() else {
            return refusal(
                RefusalClass::WrongState,
                "this intelligence does not lead a conversation",
            );
        };
        let Some(mut driver) = self.agent.take() else {
            return refusal(
                RefusalClass::WrongState,
                "no conversation is open in this Session",
            );
        };
        if let Some(effort) = effort {
            model = model.with_effort(effort);
        }
        let (account, entered) = match self.enter_dispatch(Some(model.model())) {
            Ok(pair) => pair,
            Err(why) => {
                self.agent = Some(driver);
                return refusal(
                    RefusalClass::NotAllowed,
                    format!("{why} · nothing was sent"),
                );
            }
        };
        if let Some(account) = account {
            model = model.with_admission(account);
        }
        let outcome = self.drive(&mut driver, line, &mut model);
        self.agent = Some(driver);
        self.leave_paid_dispatch(entered);
        outcome
    }

    /// The run: a line that answers the call the conversation waits on, or a new prompt.
    fn drive(&mut self, driver: &mut Driver, line: &str, model: &mut AgentModel) -> TurnOutcome {
        let probes = (self.census.as_ref()).map(|c| c.provider_context.clone());
        let mut desk = SessionDesk {
            root: self.snapshot.root.clone(),
            probes: probes.unwrap_or_default(),
            knowledge: self.authoring_context.knowledge().cloned(),
        };
        let asker = self.questions.asker();
        let mut mint =
            move |context: &str| QuestionId::new(Witness::of(context.as_bytes()).0, &asker);
        let now = unix_ms;
        let Driver {
            conversation,
            tree,
            store,
            ..
        } = &mut *driver;
        if tree.is_none() {
            match start_tree(store, &self.snapshot.root, model.model()) {
                Ok(started) => *tree = Some(started),
                Err(why) => {
                    let text = format!("the conversation could not start: {why}");
                    return refusal(RefusalClass::Io, text);
                }
            }
        }
        let Some(tree) = tree.as_mut() else {
            return refusal(RefusalClass::Io, "the conversation could not start");
        };
        let before = conversation.asked_ids();
        let parked = tree.parked().is_some();
        if parked {
            conversation.answered_by(&tree.next_cite());
        }
        let toolbox = Toolbox::new(conversation, store.citations(), &mut desk, &mut mint);
        let mut events = |_: AgentEvent| {};
        let mut agent = Agent::new(tree, store, &toolbox, &now);
        let outcome = if parked {
            agent.answer(line, model, &mut events)
        } else {
            agent.prompt(line, model, &mut events)
        };
        drop(agent);
        let decided = toolbox.decided();
        drop(toolbox);
        let still = conversation.asked_ids();
        for id in before.into_iter().filter(|id| !still.contains(id)) {
            self.questions.close(Some(id));
        }
        let request = (store.citations().lock())
            .map(|c| c.stated(conversation.since()))
            .unwrap_or_default();
        self.intent.goal = request.lines().next().map(str::to_owned);
        self.settle_run(driver, outcome, decided)
    }

    /// The run's outcome as the Session's own.
    fn settle_run(
        &mut self,
        driver: &mut Driver,
        outcome: Outcome,
        decided: Decided,
    ) -> TurnOutcome {
        let said = driver
            .tree
            .as_ref()
            .map(Tree::last_said)
            .unwrap_or_default();
        match outcome {
            Outcome::Parked { .. } => {
                // A question waits: the proposal shown is no longer what the next line answers.
                self.pending = None;
                let open = driver.open();
                let key = open.first().map(|q| q.key.clone()).unwrap_or_default();
                let mut text: Vec<String> = Vec::new();
                text.extend((!said.is_empty()).then_some(said));
                text.extend(open.iter().map(|q| q.question.clone()));
                TurnOutcome::Question {
                    key,
                    question: text.join("\n"),
                }
            }
            Outcome::Answered { text } => {
                let current = driver.conversation.candidate().map(|c| c.number);
                match decided.proposed {
                    Some(revision) if current == Some(revision) => {
                        self.agent_propose(driver, &text, decided.acts)
                    }
                    _ => TurnOutcome::Reply(text),
                }
            }
            Outcome::Stopped { .. } => TurnOutcome::Cancelled(
                "preparation stopped · the conversation is kept · a request already sent may still be billed"
                    .to_owned(),
            ),
            Outcome::Failed { error } => refusal(
                RefusalClass::IntelligenceRefused,
                format!("the conversation could not go on: {error} · what was said is kept"),
            ),
            _ => refusal(RefusalClass::IntelligenceRefused, "the conversation ended unread"),
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
        let Some(candidate) = driver.conversation.candidate() else {
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
            Some(driver) => work.with_conversation(
                driver.conversation.bindings().to_vec(),
                driver.conversation.delegations().to_vec(),
                driver.conversation.questions().to_vec(),
            ),
            None => work,
        }
    }

    /// What a history keeps of the conversation, once one started: a Session the round driver
    /// leads keeps its records' bytes unchanged.
    pub(super) fn agent_kept(&self) -> Option<Value> {
        (self.agent.as_ref())
            .filter(|d| d.tree.is_some())
            .map(|d| d.conversation.kept())
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
