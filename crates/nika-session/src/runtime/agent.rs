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
mod reopened;
mod store;
mod system;
mod tools;

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use nika_fs::OwnedDir;
use nika_onboard::compile::TriggerRequirement;
use nika_providers::InferenceAdmission;
use nika_session_agent::conversation::{
    Acts, Conversation, NOT_A_CONSENT, NOT_READ_AS_CONSENT, Pick,
};
use nika_session_agent::{
    Agent, AgentEvent, EntryKind, Observed, Outcome, QueueMode, QueuedState, Relay, Steering, Tree,
};
use nika_session_change::review::runs_line;
use nika_session_change::tools::SessionTools;
use nika_session_intelligence::reasoner::agent_model::{AgentModel, model_calls, tool_steps};
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

/// Questions, interpreted selections and optional recovery context prepared for a turn.
type PreparedLine = (Vec<QuestionId>, Vec<String>, Option<String>);

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
    authorized: Option<(ProposalId, Acts, String)>,
    pub(super) answering: Option<QuestionId>,
    /// The card of the proposal the conversation showed last: the goal its preview states,
    /// the choices it shows included, and the keys of those choices.
    card: Option<(String, Vec<String>)>,
    steering: Steering,
    cancel: CancelCtx,
    leading: Option<led::Leading>,
    /// The last run of the candidate, for the author with the person's next line.
    run_note: Option<String>,
    /// What the conversation's verifications kept, shared with every turn's desk: the verdicts
    /// that declined bytes and the last that made a document ready.
    verified: Arc<Mutex<Verified>>,
}

/// What starts a turn of the conversation: the person's line, or Nika's own note to the author
/// (a failed run's facts), which no person wrote and which authorizes nothing.
#[derive(Clone, Copy)]
pub(super) enum Said<'a> {
    Line(&'a str),
    Note(&'a str),
}

/// What Nika tells the author of a failed run of its candidate.
const REPAIR: &str = "The run the person asked for failed; its facts follow. Diagnose the failure from its evidence and the relevant current inputs. Distinguish a defect in the workflow from an unavailable dependency (for example a missing input file, credential or service). For an unavailable dependency, keep the saved workflow unchanged and briefly explain what needs restoring and how the person can retry; ask only for information you actually need. Do not invent replacement data or revise a correct workflow merely because its environment failed. If the workflow itself is defective, read it, prepare the necessary repaired revision and propose it, saying in the person's language what failed and what you changed. A later user correction takes precedence over this diagnostic task. It runs again only when the person says so.";

impl Driver {
    /// The driver a host door opens, unless the environment keeps the round driver.
    pub(super) fn from_env() -> Option<Self> {
        #[allow(clippy::disallowed_methods)]
        // a driver NAME, never a secret
        let word = std::env::var(DRIVER_ENV).unwrap_or_default();
        (!word.trim().eq_ignore_ascii_case("rounds")).then(|| Self::with_store(TreeFile::memory()))
    }

    /// Keep the exact saved workflow as the next revision's destination.
    pub(super) fn saved(&self, path: &str, bytes: &str) {
        self.toolbox
            .with(|conversation| conversation.saved_at(path, bytes));
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
            answering: None,
            card: None,
            steering: Steering::new(),
            cancel: CancelCtx::new(),
            leading: None,
            run_note: None,
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
    pub(super) fn open(&self) -> Vec<AskedQuestion> {
        self.toolbox.with(|conversation| {
            (conversation.questions().iter())
                .filter(|q| q.state == AskedState::Open)
                .cloned()
                .collect()
        })
    }

    /// The keys of the choices `set`'s card shows, when `set` is the proposal the conversation
    /// showed last.
    fn shown(&self, set: Option<&ProjectChangeSet>) -> Option<&[String]> {
        let (goal, keys) = self.card.as_ref()?;
        set.is_some_and(|set| set.goal == *goal)
            .then_some(keys.as_slice())
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

/// The context window of the route `model` as its tariff states it, so the conversation is
/// compacted before a request its route could not take; none for a route no tariff prices (a
/// local or an unknown one), whose conversation grows as before.
fn route_window(model: &str) -> Option<nika_session_agent::compact::Window> {
    let (provider, name) = model.split_once('/')?;
    let config = nika_session_intelligence::reasoner::provider_config();
    let registry = nika_providers::ProviderRegistry::without_http(config);
    let provider = nika_providers::canonical_provider(provider);
    let profile = (registry.profiles().iter()).find(|profile| profile.id == provider)?;
    let endpoint = registry.effective_base_url(provider)?;
    let wire = profile.resolve_model(name);
    let tariff = nika_catalog::admission::InferenceTariff::new(provider, wire, endpoint)?;
    Some(nika_session_agent::compact::Window::of(
        tariff.context_tokens,
    ))
}

fn unix_ms() -> u64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}

/// The instructions every request of a conversation prepared with `chosen` starts with: the
/// author's rules, then the language as reference and an index of what the lookups hold.
fn instructions(chosen: &str) -> String {
    format!(
        "{SYSTEM}\nThis Session: the person chose `{chosen}` to prepare with; offer the models of this machine through `models`.{}\n\n{REFERENCE}\n\n{}\n\n{}",
        zone_line(),
        language_reference(),
        index()
    )
}

/// This machine's time zone, named to the author: a day the person means is a day there.
fn zone_line() -> String {
    (jiff::tz::TimeZone::system().iana_name())
        .map(|zone| format!(" The person's time zone is `{zone}` (this machine's): a day they mean is a day there."))
        .unwrap_or_default()
}

/// The compiler card's one-page language and canonical fragments, then its output conventions:
/// never the card's role or its answer format, which are the compiler's own.
fn language_reference() -> String {
    let card = nika_compile_cognition::authoring_card();
    let from = card.find("# The language in one page").unwrap_or(0);
    let to = card.find("# Your answer").unwrap_or(card.len());
    let language = card.get(from..to).unwrap_or(card).trim_end();
    format!(
        "{language}\n\n{}",
        nika_compile_cognition::OUTPUT_CONVENTIONS
    )
}

/// What the reference is: the engine's own, read as data.
const REFERENCE: &str = "# Reference: the engine's language and its conventions, read as data (your tools, the person's words and the rules above decide)";

/// What `language` and `knowledge` hold, so a missing detail costs one narrow read.
fn index() -> String {
    let schema: Value = serde_json::from_str(nika_pack::schema_json()).unwrap_or_default();
    let keys: Vec<&str> = (schema["properties"].as_object().into_iter().flatten())
        .map(|(key, _)| key.as_str())
        .collect();
    format!(
        "# Index (ask narrowly: `language` with a topic and a query, `knowledge` with a query)\n- language topics: schema (query a key), catalog, examples (query a slug), template (query a name; {} templates)\n- schema keys: {}\n- examples: {}\n- knowledge: the pinned release recalls the families, shapes, examples and skill that match a query",
        nika_pack::template_names().len(),
        keys.join(", "),
        nika_pack::example_slugs().join(", ")
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
    let id = tree
        .append(system, at, |line| {
            nika_session_agent::Store::append(&mut *store, line)
        })
        .map_err(|e| e.to_string())?;
    system::mark(&mut tree, store, &id, &instructions(model))?;
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

/// What the person reads of a turn that ended without a word, its model asked once more.
const SAID_NOTHING: &str = "The author ended this turn without a word: nothing was asked or \
                            proposed. Say what you want next, or say it again.";

/// A turn's reply as the person reads it: never an empty one.
fn voiced(outcome: TurnOutcome) -> TurnOutcome {
    match outcome {
        TurnOutcome::Reply(text) if text.trim().is_empty() => {
            TurnOutcome::Reply(SAID_NOTHING.to_owned())
        }
        other => other,
    }
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
        let answering = self.agent.as_ref().is_some_and(|driver| {
            driver.answering.is_some()
                || driver
                    .tree
                    .as_ref()
                    .is_some_and(|tree| tree.parked().is_some())
        });
        // A line at a proposal continues the work it shows: it amends the money decision only
        // with an amount of its own, never replacing the request the decision binds.
        let continuing = answering || self.pending.is_some();
        if let Err(refused) = self.admit_money(line, continuing, false) {
            return Some(refused);
        }
        if let Some(held) = self.hold_for_knowledge(line, true) {
            return Some(held);
        }
        if let Some(outcome) = self.natural_run_turn(line) {
            return Some(outcome);
        }
        Some(self.agent_said(Said::Line(line)))
    }

    /// Record a natural rerun as the person's own line, ending only an agent's parked question.
    /// Its facts are not fabricated as bound answers; Run still validates its current bytes.
    pub(super) fn record_agent_rerun(&mut self, line: &str) -> Result<(), String> {
        let Some(driver) = self.agent.as_mut() else {
            return Ok(());
        };
        let Some(tree) = driver.tree.as_mut() else {
            return Ok(());
        };
        let parked = tree.parked().map(|(call, _)| call.to_owned());
        let note = "The person explicitly requested the named or current existing workflow. Nika is validating that run; no new candidate was requested.";
        let store = &mut driver.store;
        tree.append_user_read(
            line,
            (parked, None),
            Some(note.to_owned()),
            unix_ms(),
            |entry| nika_session_agent::Store::append(&mut *store, entry),
        )
        .map_err(|error| error.to_string())?;
        let closed = driver.toolbox.with(Conversation::supersede_questions);
        for id in closed {
            self.questions.close(Some(id));
        }
        Ok(())
    }

    /// Nika's own note to the conversation's author, as a turn with no person's line: none when
    /// no conversation leads or a question waits for the person (a proposal the person's words
    /// acted on waits for nothing more).
    fn agent_note(&mut self, note: &str) -> Option<TurnOutcome> {
        let tree = self.agent.as_ref().and_then(|d| d.tree.as_ref());
        let waits = |tree: &Tree| tree.parked().is_some_and(|(_, name)| name != "propose");
        if !self.agent_leads() || tree.is_none_or(waits) {
            return None;
        }
        Some(self.agent_said(Said::Note(note)))
    }

    /// What a finished run of the conversation's candidate leaves its author (`said`: the run's
    /// facts as the person reads them): a failure goes back to it at once, the repaired revision
    /// proposed beside the facts; a success is noted for the person's next line.
    pub(super) fn after_run(&mut self, exit: u8, said: String) -> TurnOutcome {
        let candidate = |d: &Driver| d.toolbox.with(|c| c.candidate().is_some());
        if !self.agent_leads() || !self.agent.as_ref().is_some_and(candidate) {
            return TurnOutcome::Facts(said);
        }
        if exit == 0 {
            if let Some(driver) = self.agent.as_mut() {
                driver.run_note = Some(format!("The last run the person asked for:\n{said}"));
            }
            return TurnOutcome::Facts(said);
        }
        match self.agent_note(&format!("{REPAIR}\n{said}")) {
            Some(TurnOutcome::Proposal { id, preview }) => TurnOutcome::Proposal {
                id,
                preview: format!("{said}\n\n{preview}"),
            },
            Some(TurnOutcome::Reply(text)) => TurnOutcome::Reply(format!("{said}\n\n{text}")),
            Some(other) => other,
            None => TurnOutcome::Facts(said),
        }
    }

    /// One line through the conversation, under the same gates a reasoner call meets: the money
    /// that blocks cognition, an intelligence that reads, the effort the route can carry, and
    /// one admitted dispatch around the whole run (a seat's dispatch rides the subscription).
    fn agent_said(&mut self, said: Said<'_>) -> TurnOutcome {
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
                self.drive(&mut driver, said, &mut model, judged)
            }
            (None, Some(seat)) => self.drive_led(&mut driver, said, &seat, effort),
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
    /// turn's capabilities, and the questions a waiting run asked answered by `line`, about to be
    /// cited. The desk verifies a document under `account`, the line's admitted dispatch.
    /// Returns the questions asked before the line and what the Session read it to pick.
    fn agent_begin(
        &mut self,
        driver: &mut Driver,
        chosen: &str,
        line: &str,
        account: Option<InferenceAdmission>,
    ) -> Result<PreparedLine, String> {
        if driver.tree.is_none() {
            let started = start_tree(&mut driver.store, &self.snapshot.root, chosen)?;
            driver.tree = Some(started);
        }
        let recovery = if line.is_empty() {
            None
        } else {
            let recovery = reopened::close_expired_ask(driver)?;
            if let Some(tree) = driver.tree.as_mut() {
                system::refresh(tree, &mut driver.store, chosen)?;
            }
            recovery
        };
        let probes = (self.census.as_ref()).map(|c| c.provider_context.clone());
        let desk = SessionDesk {
            root: self.snapshot.root.clone(),
            probes: probes.unwrap_or_default(),
            knowledge: self.authoring_context.knowledge().cloned(),
            admitted: None,
            saved: driver
                .toolbox
                .with(|c| c.saved().map(|(p, w)| (p.to_owned(), w.to_owned()))),
            judged: (driver.store.dir()).and_then(|dir| dir.try_clone().ok()),
            verifier: Verifier {
                seat: self.seat.clone(),
                context: self.authoring_context.clone(),
                account,
                kept: Arc::clone(&driver.verified),
                trial: self.trial_lent(),
            },
        };
        let asker = self.questions.asker();
        let mint = move |context: &str| QuestionId::new(Witness::of(context.as_bytes()).0, &asker);
        driver.toolbox.begin(Box::new(desk), Box::new(mint));
        // The choices a proposal shows close with it: a line typed after it answers none.
        if driver.shown(self.pending.as_ref()).is_none() {
            driver.card = None;
            driver.toolbox.with(Conversation::close_choices);
        }
        driver
            .observed
            .watch(self.progress.listener().map(tool_steps));
        let waiting = (driver.tree.as_ref())
            // A bound answer has already passed the Session's identity/state checks. Another
            // question from the same group can outlive the agent's intervening completed turn.
            .filter(|tree| tree.parked().is_some() || driver.answering.is_some())
            .map(Tree::next_cite);
        let read = match waiting {
            Some(_) => self.read_answers(driver, line),
            None => BTreeMap::new(),
        };
        Ok(driver.toolbox.with(|conversation| {
            let before = conversation.asked_ids();
            let said = waiting
                .map(|cite| match &driver.answering {
                    Some(id) => conversation.answered_one_by(id, &cite, line, &read),
                    None => conversation.answered_by(&cite, line, &read),
                })
                .unwrap_or_default();
            (before, said, recovery)
        }))
    }

    /// The Session's own reading of `line` against each waiting question no whole-line protocol
    /// settles: one bounded label call each, metered as every reading is; a reading that cannot
    /// run picks nothing.
    fn read_answers(&mut self, driver: &Driver, line: &str) -> BTreeMap<String, Pick> {
        let questions = driver
            .toolbox
            .with(|conversation| conversation.to_read(line));
        let mut read = BTreeMap::new();
        for question in questions.into_iter().filter(|question| {
            driver
                .answering
                .as_ref()
                .is_none_or(|id| question.id == *id)
        }) {
            let phase = crate::activity::Phase::Understanding;
            self.activity(&crate::activity::Activity::now(
                phase,
                "reading your answer",
            ));
            let pick = match self.reason_with_money(&Pick::prompt(&question, line), true) {
                Ok(reply) => Pick::read(&reply.text, &question),
                Err(_) => Pick::Unread,
            };
            read.insert(question.key, pick);
        }
        read
    }

    /// The run over the selected API or local route, by Nika's own loop: a line that answers
    /// the call the conversation waits on, or a new prompt.
    fn drive(
        &mut self,
        driver: &mut Driver,
        said: Said<'_>,
        model: &mut AgentModel,
        account: Option<InferenceAdmission>,
    ) -> TurnOutcome {
        // A seat's agent no longer leads once the route is an API or a local one.
        driver.leading = None;
        let line = match said {
            Said::Line(line) => line,
            Said::Note(_) => "",
        };
        let run_note = driver.run_note.take().filter(|_| !line.is_empty());
        let (before, read, recovery) = match self.agent_begin(driver, model.model(), line, account)
        {
            Ok(begun) => begun,
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
        // The loop's model calls reach the host as typed activity (they were discarded).
        let mut calls = (self.progress.listener()).map(|hook| model_calls(hook, model.model()));
        let mut events = |event: AgentEvent| {
            if let Some(calls) = calls.as_mut() {
                calls(&event);
            }
        };
        let pending = || toolbox.with(|c| c.unproposed());
        let mut agent = Agent::new(tree, store, &**observed, &now)
            .with_steering(steering)
            .with_cancel(cancel)
            .with_reading(&read)
            .with_note(reopened::with_run_note(run_note, recovery))
            .with_pending(&pending);
        if let Some(window) = route_window(model.model()) {
            agent = agent.with_window(window);
        }
        let outcome = match (said, parked) {
            (Said::Note(note), _) => agent.note(note, model, &mut events),
            (Said::Line(line), true) => agent.answer(line, model, &mut events),
            (Said::Line(line), false) => agent.prompt(line, model, &mut events),
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
        voiced(self.settle_run(driver, (outcome, reach), decided))
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
            Outcome::Parked { name, queued, .. } if name == "propose" => {
                // The proposal ended the turn: it is the turn's outcome, said in the words of the
                // message that called `propose` (else its summary); the next line answers it.
                let current = driver.toolbox.with(|c| c.candidate().cloned());
                let Some(candidate) = current.filter(|c| decided.proposed == Some(c.number)) else {
                    return TurnOutcome::Reply(said);
                };
                let text = if said.is_empty() {
                    candidate.summary
                } else {
                    said
                };
                let authorized = decided.acts.zip(decided.cited);
                match self.agent_propose(driver, &text, (authorized, decided.trigger)) {
                    TurnOutcome::Proposal { id, preview } => TurnOutcome::Proposal {
                        id,
                        preview: unsent(&queued)
                            .map_or(preview.clone(), |not| format!("{preview}\n{not}")),
                    },
                    other => other,
                }
            }
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
                        let authorized = decided.acts.zip(decided.cited);
                        self.agent_propose(driver, &text, (authorized, decided.trigger))
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
    /// acts the person's words authorized for it are kept for the turn's door to perform; the
    /// schedule it carries is shown in its card and kept for « activate » once it is saved.
    fn agent_propose(
        &mut self,
        driver: &mut Driver,
        said: &str,
        (acts, trigger): (Option<(Acts, String)>, Option<TriggerRequirement>),
    ) -> TurnOutcome {
        let Some(candidate) = driver.toolbox.with(|c| c.candidate().cloned()) else {
            return TurnOutcome::Reply(said.to_owned());
        };
        let root = self.snapshot.root.clone();
        let saved = driver
            .toolbox
            .with(|c| c.saved().map(|(p, w)| (p.to_owned(), w.to_owned())));
        let read = driver
            .verified
            .lock()
            .ok()
            .map(|kept| kept.read().clone())
            .unwrap_or_default();
        let at = match desk::landing(&root, &candidate.source, saved.as_ref(), &read) {
            Ok(at) => at,
            Err(why) => return refusal(RefusalClass::AuthoringRefused, why),
        };
        // What the person reviews: the doubt the verifier stated and the choices the proposal
        // shows are in its goal, so the preview and the identity the consent answers cover them.
        let doubt = (driver.verified.lock())
            .ok()
            .and_then(|kept| kept.doubt().map(str::to_owned));
        let (goal, keys) = driver.toolbox.with(|c| {
            let keys: Vec<String> = c.open().iter().map(|q| q.key.clone()).collect();
            (c.card(&candidate.summary, doubt.as_deref()), keys)
        });
        // The schedule is part of what the person consents to, beside the bytes: saving
        // declares nothing; « activate » declares it once the workflow is saved.
        let goal = match &trigger {
            Some(trigger) => format!("{goal}\nRuns\n{}", runs_line(Some(trigger))),
            None => goal,
        };
        let set = match ProjectChangeSet::workflow_at(&root, &goal, &at, candidate.source.clone()) {
            Ok(set) => set,
            Err(error) => return TurnOutcome::Refusal(Refusal::from_change(&error)),
        };
        let mut bytes = self.draft_preview(&set);
        let id = ProposalId::of(&bytes);
        // The trial of these very bytes, bound to the proposal that shows them.
        let tried = (driver.verified.lock())
            .ok()
            .and_then(|mut kept| kept.take_tried());
        if let Some(tried) = tried {
            self.bind_trial((&id, &set), tried, &mut bytes);
        }
        self.bind_proposal_money(&id);
        // The source basis of these bytes, bound to the identity the person reviews.
        let seen = (driver.verified.lock()).map(|kept| kept.read().clone());
        self.bind_read_basis(&id, &set, &seen.unwrap_or_default());
        self.pending = Some(set);
        self.pending_trigger = trigger;
        driver.card = Some((goal, keys));
        driver.authorized = acts.map(|(acts, cited)| (id.clone(), acts, cited));
        // A value the person saw bound and this revision changes is said in words.
        let changed = driver.toolbox.with(|c| c.changed().join(" · "));
        let said = if changed.is_empty() {
            said.to_owned()
        } else {
            format!("{said}\n\nChanged from the proposal you saw: {changed}.")
        };
        TurnOutcome::Proposal {
            id,
            preview: format!("{said}\n\n{bytes}"),
        }
    }

    /// The person's consent `line` to the proposal waiting (`set`), before anything lands: why it
    /// cannot save it (a question asked, or a choice open, that its card does not show; an
    /// option the person picked instead of a default it carries), else, while choices it shows
    /// are open, the line recorded as their answer to the proposal, binding each default as the
    /// option they picked with it.
    pub(super) fn agent_consents(
        &mut self,
        set: &ProjectChangeSet,
        line: &str,
    ) -> Result<(), String> {
        let Some(driver) = self.agent.as_mut() else {
            return Ok(());
        };
        let shown = (driver.shown(Some(set)).map(<[String]>::to_vec)).unwrap_or_default();
        if let Some(why) = driver.toolbox.with(|c| c.unsaved(&shown)) {
            return Err(why);
        }
        let open: Vec<String> = driver
            .toolbox
            .with(|c| c.open().iter().map(|q| q.key.clone()).collect());
        let Some(tree) = driver.tree.as_mut().filter(|_| !open.is_empty()) else {
            return Ok(());
        };
        let parked = (tree.parked())
            .filter(|(_, name)| *name == "propose")
            .map(|(call, _)| call.to_owned());
        let keys: Vec<String> = open.iter().map(|key| format!("`{key}`")).collect();
        let note = format!(
            "The person consented to the proposal with this line: Nika bound the default of each choice it showed ({}) as the option they picked.",
            keys.join(", ")
        );
        let store = &mut driver.store;
        let (_, cite) = (tree.append_user_read(line, (parked, None), Some(note), unix_ms(), |l| {
            nika_session_agent::Store::append(&mut *store, l)
        }))
        .map_err(|error| error.to_string())?;
        driver.toolbox.with(|c| c.consented(&cite));
        Ok(())
    }

    /// The consent the person's own words gave for the proposal this turn made, taken once:
    /// the turn's door answers it through the consent door (`save & run`, or `yes` to save)
    /// when the cited line consents ([`Pick::consented`], read by the Session's own reading).
    ///
    /// # Errors
    ///
    /// Why the claim authorizes nothing: the proposal then waits at the door.
    pub(super) fn agent_authorized(
        &mut self,
    ) -> Result<Option<(ProposalId, &'static str)>, &'static str> {
        let Some((id, acts, cited)) = self.agent.as_mut().and_then(|d| d.authorized.take()) else {
            return Ok(None);
        };
        if self.pending_proposal().as_ref() != Some(&id) {
            return Ok(None);
        }
        let asked = QuestionId::new(id.to_string(), &self.questions.asker());
        let read = |prompt: &str| self.reason_with_money(prompt, true).ok().map(|r| r.text);
        if Pick::consented(&cited, acts, asked, read) {
            return Ok(Some((id, if acts.run { "save & run" } else { "yes" })));
        }
        if let Some(driver) = self.agent.as_mut() {
            driver.run_note = Some(NOT_READ_AS_CONSENT.to_owned());
        }
        Err(NOT_A_CONSENT)
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

    /// The choice belongs to the proposal currently shown, not to another question or draft.
    pub(super) fn agent_proposal_asks(&self, id: &QuestionId) -> bool {
        self.agent.as_ref().is_some_and(|driver| {
            driver.shown(self.pending.as_ref()).is_some()
                && driver.toolbox.with(|conversation| {
                    conversation
                        .open()
                        .iter()
                        .any(|question| question.id == *id && question.state == AskedState::Open)
                })
        })
    }

    /// A validated proposal choice is an answer, never a command or consent token. It uses
    /// the same model, money and knowledge boundaries as an ordinary conversation answer.
    pub(super) fn agent_proposal_answer(&mut self, id: &QuestionId, line: &str) -> TurnOutcome {
        if let Some(driver) = self.agent.as_mut() {
            driver.authorized = None;
        }
        if !self.chosen || !self.intelligence.ready || !self.agent_leads() {
            return self.answer_refused(
                Some(id.clone()),
                refusal(RefusalClass::NoIntelligence, "the selected intelligence cannot answer this choice; `/intelligence` chooses a usable route"),
            );
        }
        // A held raw line resumes through the ordinary turn parser. Keep this typed choice
        // instead, so repairing a refused knowledge source cannot turn an offer key into a
        // command or consent. The host retains its exact selection for resubmission.
        if self.knowledge_offered() {
            return self.answer_refused(
                Some(id.clone()),
                refusal(RefusalClass::NotAllowed, "the configured knowledge source is unavailable; `/knowledge embedded` selects the built-in knowledge, then send this retained choice again · nothing was sent"),
            );
        }
        if let Err(refused) = self.admit_money(line, true, false) {
            return self.answer_refused(Some(id.clone()), refused);
        }
        if let Some(driver) = self.agent.as_mut() {
            driver.answering = Some(id.clone());
        }
        let outcome = self.agent_said(Said::Line(line));
        if let Some(driver) = self.agent.as_mut() {
            driver.answering = None;
        }
        outcome
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
        let Some(driver) = &self.agent else {
            return work;
        };
        // The choices the proposal waiting shows, while it waits.
        let shows = driver.shown(self.pending.as_ref()).is_some();
        driver
            .toolbox
            .with(|conversation| {
                let open = (conversation.open().iter()).filter(|_| shows).cloned();
                work.with_conversation(
                    conversation.bindings().to_vec(),
                    conversation.delegations().to_vec(),
                    conversation.questions().to_vec(),
                )
                .with_open_choices(open.collect())
            })
            .with_queued(driver.steering.records())
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
            let driver = Driver::resumed(dir, kept);
            if self.last_workflow.is_none() && driver.tree.is_some() && driver.damaged.is_none() {
                // A saved destination is historical context, not consent to replay a Run.
                // The next explicit request still checks its current bytes and effects.
                self.last_workflow = driver.toolbox.with(|conversation| {
                    let (path, _) = conversation.saved()?;
                    super::natural_run::relative_workflow(&self.snapshot.root, Path::new(path))
                });
                // The rail keeps the historical Save fact, just as for the round driver's
                // program record. This field grants no pending consent, check or Run.
                self.consented.clone_from(&self.last_workflow);
            }
            self.agent = Some(driver);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
