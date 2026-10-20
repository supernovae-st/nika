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

mod defaults;
mod desk;
mod led;
mod notes;
mod proof;
mod reopened;
mod store;
mod system;
mod tools;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use nika_fs::OwnedDir;
use nika_onboard::compile::TriggerRequirement;
use nika_providers::InferenceAdmission;
use nika_session_agent::conversation::{
    Acts, Conversation, NOT_A_CONSENT, NOT_READ_AS_CONSENT, Pick, asking_call,
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
use super::decision::{is_no, is_plain_yes, is_save_and_run, is_yes, local_command_of};
use super::{SessionRuntime, TurnOutcome};
use crate::change::{ProjectChangeSet, Witness};
use crate::outcome::{ProposalId, QuestionId, Refusal, RefusalClass, StopReach, Stopped};
use crate::work::{AskedQuestion, AskedState, Waiting, Work};

/// The name a host door reads once when it opens the Session: `rounds` keeps the round driver.
pub(super) const DRIVER_ENV: &str = "NIKA_SESSION_DRIVER";

/// The instructions every request of the conversation starts with.
const SYSTEM: &str = include_str!("../../assets/author_system.md");

/// Questions, interpreted selections, optional recovery context and the `ask` call a late reply
/// names, prepared for a turn.
type PreparedLine = (Vec<QuestionId>, Vec<String>, Option<String>, Option<String>);

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
    /// The answer to the question `answering` names is the person's delegation act, by the
    /// host's own control: it binds as delegated, never as the words it records.
    pub(super) delegating: bool,
    /// The card of the proposal the conversation showed last: the goal its preview states,
    /// the choices it shows included, and the keys of those choices.
    card: Option<(String, Vec<String>)>,
    steering: Steering,
    cancel: CancelCtx,
    leading: Option<led::Leading>,
    /// The last run of the candidate, for the author with the person's next line.
    run_note: Option<String>,
    /// The person's lines before this conversation led, to begin its tree with.
    carried: Vec<String>,
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
    /// A driver with an empty conversation kept in memory: a new conversation without history.
    pub(super) fn fresh() -> Self {
        Self::with_store(TreeFile::memory())
    }

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

    /// The conversation's current candidate with the request it revises: the conversation line
    /// that request starts on, the candidate's number in it, and its bytes.
    pub(super) fn current_candidate(&self) -> Option<(u64, u64, String)> {
        self.toolbox.with(|conversation| {
            let request = conversation.since();
            (conversation.candidate())
                .map(|candidate| (request, candidate.number, candidate.source.clone()))
        })
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
            delegating: false,
            card: None,
            steering: Steering::new(),
            cancel: CancelCtx::new(),
            leading: None,
            run_note: None,
            carried: Vec::new(),
            verified: Arc::default(),
        }
    }

    /// The driver of a Session whose history opened in `dir`: the tree kept there, read back
    /// whole (a damaged tree is reported, never reset), and the conversation's kept evidence.
    pub(super) fn resumed(dir: io::Result<OwnedDir>, kept: Option<&Value>) -> Self {
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
        let mut restored = kept.and_then(Conversation::restored).unwrap_or_default();
        // Which question each kept output answered, read back from the tree it was asked in:
        // evidence of which outputs stand apart, never a question, an answer or an authority.
        if let Some(tree) = &driver.tree {
            let citations = driver.store.citations();
            let citations = citations.lock().unwrap_or_else(PoisonError::into_inner);
            restored.recall(tree, &citations);
        }
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

/// The exact skeleton a line asks for by its name: the name alone (the welcome's demo,
/// `aggregate-by-key`; the first lesson, `hello`), or the name beside words that only ask to see,
/// try or run that demo (« montre-moi cette démo aggregate-by-key dont tu parles sur l'écran
/// d'accueil », « run aggregate-by-key »). It is the compiler's to settle with no model: its
/// records travel inside it and its one open value is asked. Live gauntlet, 2026-10-15: under
/// Claude Sonnet the conversation's author asked for a data file the demo does not need, and a
/// demo it showed in the chat could not be saved, no request of the person's standing behind it.
/// A line that adds a path, an address or a number asks for more than the demo: its author reads
/// it.
pub(super) fn demo_named(line: &str) -> Option<String> {
    if asks_the_welcome_example(line) {
        return Some(crate::WELCOME_DEMO.to_owned());
    }
    let mut names = nika_pack::template_names();
    names.extend(["hello".to_owned(), "01-hello".to_owned()]);
    let line = line.trim();
    if names.iter().any(|name| name == line) {
        return Some(line.to_owned());
    }
    let words: Vec<String> = (line.split_whitespace())
        .map(|word| {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
            word.to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let mut found: Vec<&String> = words.iter().filter(|word| names.contains(word)).collect();
    found.dedup();
    let [name] = found[..] else {
        return None;
    };
    let asks = words.iter().any(|word| DEMO_CUES.contains(&word.as_str()));
    let adds = line.contains(['/', '@'])
        || (words.iter()).any(|word| word != name && word.chars().any(|c| c.is_ascii_digit()));
    (asks && !adds && words.len() <= 24).then(|| name.clone())
}

/// Whether a line asks for the example the welcome screen offers, by where it is shown rather
/// than by its name (« montre-moi l'exemple de l'écran d'accueil », « the example on the welcome
/// screen »): only seeing or trying it, nothing more. Live gauntlet r08, 2026-10-16: the author
/// answered with the smallest example it knew, on a local model, instead of the welcome demo.
fn asks_the_welcome_example(line: &str) -> bool {
    let lower = line.to_lowercase().replace('\u{2019}', "'");
    let place = ["écran d'accueil", "ecran d'accueil", "welcome screen"]
        .iter()
        .any(|place| lower.contains(place));
    let example = ["exemple", "example", "démo", "demo"]
        .iter()
        .any(|word| lower.contains(word));
    // A path, an address or a number asks for more than the demo: its author reads it.
    let adds = lower.contains(['/', '@']) || lower.chars().any(|c| c.is_ascii_digit());
    place && example && !adds && lower.split_whitespace().count() <= 16
}

/// The words that only ask to see, try or run a demo.
const DEMO_CUES: &[&str] = &[
    "démo",
    "demo",
    "démonstration",
    "demonstration",
    "exemple",
    "example",
    "essaie",
    "essaye",
    "essayer",
    "teste",
    "tester",
    "try",
    "montre",
    "montre-moi",
    "montrez-moi",
    "show",
    "voir",
    "affiche",
    "affiche-moi",
    "lance",
    "lance-la",
    "lance-le",
    "run",
    "exécute",
    "execute",
];

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

/// What Nika says with the first of the person's lines a starting conversation carries.
const CARRIED_NOTE: &str = "These lines were said before you led this conversation: another intelligence, or none, read them and nothing was prepared from them. They are the person's own words; continue from them when the person refers to them.";

/// The person's lines a starting conversation carries ([`SessionRuntime::carry_into`]), cited
/// in its new tree before the line that starts it.
fn append_carried(driver: &mut Driver) -> Result<(), String> {
    let carried = std::mem::take(&mut driver.carried);
    let Driver { tree, store, .. } = driver;
    let Some(tree) = tree.as_mut() else {
        return Ok(());
    };
    carried.iter().enumerate().try_for_each(|(k, line)| {
        let note = (k == 0).then(|| CARRIED_NOTE.to_owned());
        tree.append_user_read(line.as_str(), (None, None), note, unix_ms(), |entry| {
            nika_session_agent::Store::append(&mut *store, entry)
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
    })
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

/// [`NOT_A_CONSENT`] in French.
const NOT_A_CONSENT_FR: &str = "la ligne citée n'est pas lue comme un consentement : rien n'a été \
                                enregistré ni lancé · « oui » l'enregistre, « save & run » \
                                l'enregistre et la lance";

impl SessionRuntime {
    /// Whether the selected intelligence leads this Session's conversation: the host door kept
    /// the agent driver and the selected route can lead one.
    pub(super) fn agent_leads(&self) -> bool {
        self.agent.is_some()
            && (self.reasoner.agent_model().is_some() || self.agent_seat().is_some())
    }

    /// The person's lines of the conversation, the newest first.
    pub(super) fn person_lines_newest_first(&self) -> Vec<String> {
        let Some(tree) = self.agent.as_ref().and_then(|driver| driver.tree.as_ref()) else {
            return Vec::new();
        };
        (tree.branch().into_iter().rev())
            .filter_map(|entry| match &entry.kind {
                EntryKind::User { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
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

    /// Whether `line` belongs to a door of the Session rather than to the conversation's model:
    /// a waiting round, value, activation, install decision or paused run owns it, or it is a
    /// command, an engine fact, an explicit run, activate or deactivate line, or a demo's name.
    /// Whether a question the conversation's author asked waits for the person's answer (a
    /// parked `ask`): their next line is that answer.
    pub(super) fn author_asks(&self) -> bool {
        self.agent.as_ref().is_some_and(|driver| {
            driver.answering.is_some()
                || (driver.tree.as_ref())
                    .and_then(Tree::parked)
                    .is_some_and(|(_, name)| name == "ask")
        })
    }

    pub(super) fn agent_yields(&self, line: &str) -> bool {
        let input = line.trim();
        // The install decision a landed declaration asks owns its answer, whatever the line:
        // a row key (« declared »), a label, a yes (live gauntlet wave 5, 2026-10-17).
        let owned = self.authoring.is_some()
            || self.run_inputs.is_some()
            || self.activation.is_some()
            || self.install_offer.is_some()
            || self.pending_gate.is_some();
        // The engine's own answers /help promises without AI (« est-ce que X est valide »,
        // « explique NIKA-… »), and « désactive-le », are the Session's doors.
        let door = input.is_empty()
            || input.starts_with('/')
            || self.deactivates(input)
            || crate::facts::answer(input, &self.snapshot, &self.snapshot.root).is_some()
            || super::knowledge::is_choice(input)
            || super::schedule::is_activate(input)
            || super::schedule::activation_cadence(input).is_some()
            || self.schedules_the_saved(input).is_some()
            || self.schedule_change(input).is_some()
            || self.activates_saved_period(input)
            || demo_named(input).is_some()
            || self.local_run_line(line);
        owned || door
    }

    /// The line, when the conversation's intelligence takes it: a free line while no
    /// authoring round, run input, activation or paused run owns the next line, once an
    /// intelligence is chosen and ready. A command, an explicit run or activate line and the
    /// knowledge choice keep their own doors. The line meets the money admission and the
    /// knowledge hold every line that reaches a model meets. `None` leaves it to the round
    /// driver's turn.
    pub(super) fn agent_turn(&mut self, line: &str) -> Option<TurnOutcome> {
        let unready = !self.chosen || !self.intelligence.ready;
        if self.agent_yields(line) || unready || !self.agent_leads() {
            return None;
        }
        // A ceiling stated alone for the proposal waiting binds it as the consent door does: the
        // same bytes under a fresh identity, with no author (QA gauntlet q03).
        if self.pending.is_some() && self.states_money_only(line) {
            return Some(self.consent(line.trim()));
        }
        // A ceiling set alone with nothing waiting is the saved workflow's next runs' (live
        // gauntlet r09, 2026-10-17: « Mets le plafond à 0,001 $ » met the model's « C'est noté »,
        // and the next run kept $0.10).
        if self.pending.is_none()
            && let Some(workflow) = self.last_workflow.clone()
            && self.sets_the_ceiling(line).is_some()
        {
            return Some(self.set_run_ceiling(line, workflow));
        }
        let answering = self.agent.as_ref().is_some_and(|driver| {
            driver.answering.is_some()
                || driver
                    .tree
                    .as_ref()
                    .is_some_and(|tree| tree.parked().is_some())
        });
        // « annuler » at a question the author asked drops it, with no model call.
        if answering && let Some(dropped) = self.drop_agent_question(line) {
            return Some(dropped);
        }
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

    /// « On continue » with a proposal waiting shows it again as it stands, whoever prepares:
    /// continuing never writes it anew (live gauntlet r07 B5: after an intelligence switch, the
    /// ready proposal was re-authored for minutes and lost its `on_error`; wave 6 r07 B1,
    /// 2026-10-18: after a switch to an intelligence that leads no conversation, the line reached
    /// the compiler and a free proposal was re-authored for 13 minutes into model steps). A line
    /// a round, a run's input, an activation or an install decision owns is theirs.
    pub(super) fn continue_pending(&self, line: &str) -> Option<TurnOutcome> {
        let owned = self.authoring.is_some()
            || self.run_inputs.is_some()
            || self.activation.is_some()
            || self.install_offer.is_some()
            || self.pending_gate.is_some();
        if owned || !Self::is_continue(line) {
            return None;
        }
        let set = self.pending.as_ref()?;
        let id = self.proposal_id(set);
        let lead = self.language.pick(
            "The proposal still waits for your answer, as it stands:",
            "La proposition attend toujours votre réponse, telle quelle :",
        );
        let preview = format!("{lead}\n{}", self.proposal_preview(set));
        Some(TurnOutcome::Proposal { id, preview })
    }

    /// Whether a line only asks to go on (« on continue », « continue là où on en était », « go
    /// on »), its courtesy aside: never consent, never a change.
    pub(super) fn is_continue(line: &str) -> bool {
        const FORMS: &[&str] = &[
            "continue",
            "continuez",
            "on continue",
            "continuons",
            "continue là où on en était",
            "on continue là où on en était",
            "continuons là où on en était",
            "continue là où on s'était arrêté",
            "reprends",
            "reprenez",
            "on reprend",
            "reprends là où on en était",
            "go on",
            "carry on",
            "keep going",
            "let's continue",
            "continue where we left off",
            "let's continue where we left off",
        ];
        const COURTESY: &[&str] = &[
            "stp", "svp", "please", "merci", "alors", "ok", "s'il", "te", "vous", "plaît", "plait",
        ];
        let lowered = line.replace('\u{2019}', "'").to_lowercase();
        let words: Vec<&str> = (lowered.split(|c: char| c.is_whitespace() || ",;:!?.".contains(c)))
            .filter(|word| !word.is_empty())
            .collect();
        let start = words.iter().take_while(|w| COURTESY.contains(w)).count();
        let end = words.len()
            - words
                .iter()
                .rev()
                .take_while(|w| COURTESY.contains(w))
                .count();
        let said = words
            .get(start..end.max(start))
            .unwrap_or_default()
            .join(" ");
        FORMS.contains(&said.as_str())
    }

    /// A cancel word alone (« cancel », « annuler », « laisse tomber »: the hosts' one reader,
    /// [`crate::authoring::is_cancel`]) typed at the questions the conversation's author asked
    /// drops them (gauntlet wave 3): recorded as the person's line that ends the asking call,
    /// with one fact for the author, the questions close with no binding, and asked again each
    /// is refused as dropped. No model is called; the conversation goes on with the person's
    /// next line. None for any other line, when no question of the author waits, and when one
    /// of them offers those very words.
    fn drop_agent_question(&mut self, line: &str) -> Option<TurnOutcome> {
        if !crate::authoring::is_cancel(line) {
            return None;
        }
        let driver = self.agent.as_mut()?;
        // A cancel word one of the questions offers (an option « Annuler ») answers it instead,
        // as the TUI reads it at a proposal's choice.
        let said = line.trim();
        let offers = |question: &nika_session_change::work::AskedQuestion| {
            (question.options.iter()).any(|offer| {
                offer.key.trim().eq_ignore_ascii_case(said)
                    || offer.label.trim().eq_ignore_ascii_case(said)
            })
        };
        let waiting = driver.toolbox.with(|c| {
            let asked = c.questions();
            !asked.is_empty() && !asked.iter().any(offers)
        });
        if !waiting {
            return None;
        }
        let tree = driver.tree.as_mut()?;
        let parked = (tree.parked())
            .filter(|(_, name)| *name == "ask")
            .map(|(call, _)| call.to_owned());
        let cite = tree.next_cite();
        let dropped = driver.toolbox.with(|c| c.drop_questions(&cite));
        let keys: Vec<String> = (dropped.iter())
            .map(|(key, _)| format!("`{key}`"))
            .collect();
        let note = format!(
            "The person dropped your question {} with these words: nothing is bound. Do not ask it again; go on without it, or say what you will do instead.",
            keys.join(", ")
        );
        let store = &mut driver.store;
        let recorded = tree.append_user_read(
            line.trim(),
            (parked, None),
            Some(note),
            unix_ms(),
            |entry| nika_session_agent::Store::append(&mut *store, entry),
        );
        for (_, id) in dropped {
            self.questions.close(Some(id));
        }
        if let Err(error) = recorded {
            return Some(refusal(
                RefusalClass::Io,
                format!("the dropped question could not be recorded · {error}"),
            ));
        }
        Some(TurnOutcome::Facts(
            "question dropped · nothing was bound · the conversation goes on: say what you want next".to_owned(),
        ))
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
        let note = nika_session_agent::conversation::RERUN_NOTE;
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
    pub(super) fn agent_note(&mut self, note: &str) -> Option<TurnOutcome> {
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
        if exit == 0 || !self.agent.as_ref().is_some_and(candidate) {
            self.note_run(&said);
            return TurnOutcome::Facts(said);
        }
        if !self.agent_leads() {
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

    /// A run the conversation's author does not repair at once (a success, a run of a workflow
    /// it holds no candidate of, a run refused before or around its execution) reaches it with
    /// the person's next line. Live gauntlet, 2026-10-15: right after a run refused by the
    /// environment (exit 3), « pourquoi le run a échoué ? » met « aucun run n'a échoué ».
    pub(super) fn note_run(&mut self, said: &str) {
        if !self.agent_leads() {
            return;
        }
        if let Some(driver) = self.agent.as_mut() {
            driver.run_note = Some(format!("The last run the person asked for:\n{said}"));
        }
    }

    /// What Nika itself did beside the conversation (a schedule declared in `nika.yaml`), told
    /// to the author with the person's next line, after any run it was told: what it says of the
    /// project is then what the project file says (live gauntlet r02 and r08, 2026-10-16: right
    /// after « applied · wrote `nika.yaml` » the author said « rien n'est déclaré »).
    pub(super) fn note_for_author(&mut self, said: &str) {
        if !self.agent_leads() {
            return;
        }
        if let Some(driver) = self.agent.as_mut() {
            let note = match driver.run_note.take() {
                Some(run) => format!("{run}\n\n{said}"),
                None => said.to_owned(),
            };
            driver.run_note = Some(note);
        }
    }

    /// The person discarded the proposal the conversation showed: the conversation's candidate
    /// returns to the saved workflow's bytes as they stand (none without a saved one), and the
    /// author learns it with the person's next line, so a later change never revises the
    /// discarded draft (live gauntlet r08, 2026-10-16: it came back as a second workflow).
    pub(super) fn agent_discarded(&mut self) {
        if !self.agent_leads() {
            return;
        }
        let root = self.snapshot.root.clone();
        let Some(driver) = self.agent.as_ref() else {
            return;
        };
        let saved = (driver.toolbox).with(|c| c.saved().map(|(path, _)| path.to_owned()));
        let bytes = saved.and_then(|path| std::fs::read_to_string(root.join(path)).ok());
        let discarded = driver.toolbox.with(|c| c.discarded(bytes));
        if let Some(note) = discarded.note() {
            self.note_for_author(&note);
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

    /// The host's note beside the person's `line` with the Session's spend so far as its first
    /// fact, as `/status` says it (QA gauntlet q09: asked what the preparation cost, the author
    /// answered « je n'ai accès à aucune mesure » while the status said $2.32), once any model
    /// request was observed. A turn with no line of the person's (Nika's own note) carries none,
    /// and an agent leading over ACP, whose subscription the Session never meters, reads none
    /// either.
    pub(super) fn with_spend(&self, note: Option<String>, line: &str) -> Option<String> {
        if line.is_empty() || !self.spent_anything() {
            return note;
        }
        let summary = self.inference_line();
        let spent = summary.lines().next().unwrap_or_default();
        let fact =
            format!("The Session's spend so far, as the person's `/status` shows it: {spent}");
        // First, so a recovery note still ends on its state line.
        Some(match note {
            Some(note) => format!("{fact}\n\n{note}"),
            None => fact,
        })
    }

    /// Whether this Session's preparation observed a model request yet, as its spend line counts
    /// them: before any, that line says nothing worth reading.
    fn spent_anything(&self) -> bool {
        let called = |observed: &Value| observed["calls"].as_array().is_some_and(|c| !c.is_empty());
        self.money.preparation.is_some() && self.cost_observations().iter().any(called)
    }

    /// What `/details` says of the workflow this Session's conversation built, when no compile
    /// round read one: that its intelligence wrote it as a document the engine checked and
    /// verified, its last revision, the workflow it saved, and what preparation spent. None
    /// before the conversation wrote or saved anything.
    pub(super) fn conversation_details(&self) -> Option<String> {
        let driver = self.agent.as_ref()?;
        let (revision, saved) = driver.toolbox.with(|conversation| {
            let revision = conversation.candidate().map(|candidate| candidate.number);
            (
                revision,
                conversation.saved().map(|(path, _)| path.to_owned()),
            )
        });
        let saved =
            saved.or_else(|| (self.last_workflow.as_ref()).map(|p| p.display().to_string()));
        if revision.is_none() && saved.is_none() {
            return None;
        }
        let mut text = "\n  built by the conversation: the intelligence above wrote it as a document · the engine checked and verified each revision before proposing it".to_owned();
        if let Some(revision) = revision {
            let _ = write!(text, "\n  last revision: {revision}");
        }
        if let Some(saved) = saved {
            let _ = write!(text, " · last saved: `{saved}`");
        }
        let spent = self.inference_line();
        let _ = write!(text, "\n  {}", spent.lines().next().unwrap_or_default());
        Some(text)
    }

    /// The Run ceiling a proposal of this turn binds when the person stated it, for its
    /// verifier: the amount the work's own words admitted, and the one saved with the workflow
    /// this conversation saved (`saved`), which a revision landing there keeps.
    fn ceiling_beside(&self, saved: Option<&str>) -> desk::Ceiling {
        desk::Ceiling {
            stated: self.stated_ceiling(),
            saved: saved.and_then(|path| {
                let usd = self.saved_ceiling(path)?.effective_usd?;
                Some((path.to_owned(), usd))
            }),
        }
    }

    /// The desk a turn's tools work at: the project, the provider facts, the pinned knowledge,
    /// the saved workflow and its ceiling, and the verifier that judges a document under
    /// `account`, the line's admitted dispatch.
    fn session_desk(&self, driver: &Driver, account: Option<InferenceAdmission>) -> SessionDesk {
        let probes = (self.census.as_ref()).map(|c| c.provider_context.clone());
        let saved = driver
            .toolbox
            .with(|c| c.saved().map(|(path, _)| path.to_owned()));
        SessionDesk {
            ceiling: self.ceiling_beside(saved.as_deref()),
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
        }
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
            append_carried(driver)?;
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
        let desk = self.session_desk(driver, account);
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
        // A delegation act is no words to read: it names its question's own recommendation.
        let read = match waiting {
            Some(_) if !driver.delegating => self.read_answers(driver, line),
            _ => BTreeMap::new(),
        };
        // A line answering a parked `ask` is that question's reply, as the tree records it. With
        // no `ask` waiting, a validated answer to another question of an earlier `ask` replies to
        // that call, found by the question's identity on the branch (live and kept alike): with no
        // proposal pending, or with one pending that does not show that question, whose answer
        // the Session admits before the consent (ADR-153). A choice a proposal shows names no
        // `ask`. A proposal's call still parked is closed first: settled by the consent door, or
        // no longer waiting as it is.
        let parked = (driver.tree.as_ref())
            .and_then(Tree::parked)
            .map(|(_, name)| name.to_owned());
        let late = match (parked.as_deref(), &driver.answering, driver.tree.as_ref()) {
            (None | Some("propose"), Some(id), Some(tree)) => asking_call(tree, id),
            _ => None,
        };
        if late.is_some() && parked.as_deref() == Some("propose") {
            let why = if self.pending.is_some() {
                reopened::ANSWERED_FIRST
            } else {
                reopened::SETTLED
            };
            reopened::close_parked_propose(driver, why)?;
        }
        let asks = parked.as_deref() == Some("ask") || late.is_some();
        Ok(driver.toolbox.with(|conversation| {
            let before = conversation.asked_ids();
            let said = waiting
                .map(|cite| {
                    if asks {
                        conversation.replied_by(&cite);
                    }
                    match &driver.answering {
                        Some(id) if driver.delegating => {
                            conversation.delegated_one_by(id, &cite, line)
                        }
                        Some(id) => conversation.answered_one_by(id, &cite, line, &read),
                        None => conversation.answered_by(&cite, line, &read),
                    }
                })
                .unwrap_or_default();
            (before, said, recovery, late)
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

    /// A conversation about to start (no tree yet) begins with what the person said before it
    /// led, as their own cited lines: the request the Session kept and their latest lines, read
    /// by another intelligence or by none (live gauntlet, 2026-10-15: after a switch from two
    /// unusable seats, « lance la tâche qu'on avait définie plus tôt » met « Je n'ai pas trace …
    /// aucun échange avant ce message », and the paths of that request were no words of theirs).
    pub(super) fn carry_into(&self, driver: &mut Driver) {
        if driver.tree.is_none() {
            driver.carried = self.carried_lines();
        }
    }

    /// The person's lines before this conversation's intelligence led it, oldest first: up to
    /// three of their latest lines, with the request the Session kept first when it is not among
    /// them; never a command, a consent word or one of Nika's own marks.
    fn carried_lines(&self) -> Vec<String> {
        let goal = (self.intent.goal.as_deref())
            .map(str::trim)
            .filter(|goal| !goal.is_empty());
        let mut said: Vec<&str> = (self.recent.iter().rev())
            .map(|(line, _)| line.trim())
            .filter(|line| {
                !line.is_empty() && !line.starts_with(['(', '/']) && !is_yes(line) && !is_no(line)
            })
            .take(3)
            .collect();
        said.reverse();
        let mut lines: Vec<String> = Vec::new();
        if let Some(goal) = goal.filter(|goal| !said.contains(goal)) {
            lines.push(goal.to_owned());
        }
        lines.extend(said.iter().map(|line| (*line).to_owned()));
        lines.dedup();
        lines
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
        self.carry_into(driver);
        let run_note = driver.run_note.take().filter(|_| !line.is_empty());
        let run_note = self.with_spend(run_note, line);
        let (before, read, recovery, late) =
            match self.agent_begin(driver, model.model(), line, account) {
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
            .with_pending(&pending)
            .with_late_reply(late);
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
        // The words beside the call that ended the turn, as the person reads them: never the
        // model's reasoning (`notes`).
        let said = (driver.tree.as_ref())
            .map(|tree| notes::beside_a_call(&tree.last_said(), self.language))
            .unwrap_or_default();
        match outcome {
            Outcome::Parked { name, queued, .. } if name == "propose" => {
                // The proposal ended the turn: it is the turn's outcome, said in the words of the
                // message that called `propose` (else its summary); the next line answers it.
                let current = driver.toolbox.with(|c| c.candidate().cloned());
                let Some(candidate) = current.filter(|c| decided.proposed == Some(c.number)) else {
                    return TurnOutcome::Reply(self.refused_said(&decided, said));
                };
                let text = if said.is_empty() {
                    candidate.summary
                } else {
                    said
                };
                let authorized = decided.acts.zip(decided.cited);
                match self.agent_propose(
                    driver,
                    &text,
                    (authorized, decided.trigger, decided.reused),
                ) {
                    TurnOutcome::Proposal { id, preview } => TurnOutcome::Proposal {
                        id,
                        preview: unsent(&queued)
                            .map_or(preview.clone(), |not| format!("{preview}\n{not}")),
                    },
                    other => other,
                }
            }
            Outcome::Parked { queued, .. } => self.parked_question(driver, said, &queued),
            Outcome::Answered { text } => {
                let current = driver.toolbox.with(|c| c.candidate().map(|c| c.number));
                match decided.proposed {
                    Some(revision) if current == Some(revision) => {
                        let authorized = decided.acts.zip(decided.cited);
                        self.agent_propose(
                            driver,
                            &text,
                            (authorized, decided.trigger, decided.reused),
                        )
                    }
                    _ => TurnOutcome::Reply(self.refused_said(&decided, text)),
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
                match self.language {
                    crate::Lang::Fr => format!(
                        "la conversation n'a pas pu continuer : {error} · ce qui a été dit est gardé"
                    ),
                    _ => {
                        format!("the conversation could not go on: {error} · what was said is kept")
                    }
                },
            ),
            _ => refusal(
                RefusalClass::IntelligenceRefused,
                "the conversation ended unread",
            ),
        }
    }

    /// A question waits: the proposal shown is no longer what the next line answers. A park that
    /// asks nothing and says nothing is no question: the person is told how to go on (live
    /// gauntlet wave 6 r02 B2, 2026-10-18: an empty question card, a free composer, and « quoi ? »
    /// for an answer).
    fn parked_question(
        &mut self,
        driver: &Driver,
        said: String,
        queued: &[(QueueMode, String)],
    ) -> TurnOutcome {
        self.pending = None;
        let open = driver.open();
        let key = open.first().map(|q| q.key.clone()).unwrap_or_default();
        let mut text: Vec<String> = Vec::new();
        text.extend((!said.is_empty()).then_some(said));
        text.extend(open.iter().map(|q| q.question.clone()));
        if text.is_empty() {
            text.push(
                self.language
                    .pick(NOTHING_ASKED, NOTHING_ASKED_FR)
                    .to_owned(),
            );
        }
        text.extend(unsent(queued));
        TurnOutcome::Question {
            key,
            question: text.join("\n"),
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
        (acts, trigger, reused): (
            Option<(Acts, String)>,
            Option<TriggerRequirement>,
            Option<u64>,
        ),
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
        // A new workflow lands at the file the person's request names, as the verifier read it:
        // their replies to Nika's questions, bound or not, are those questions' answers.
        let citations = (driver.store.citations().lock())
            .map(|c| c.clone())
            .unwrap_or_default();
        let requested =
            (driver.toolbox).with(|c| desk::requested(c, &citations, &candidate.source));
        let known = (saved.as_ref(), &read);
        let at = match desk::landing(&root, &candidate.source, known, requested.as_deref()) {
            Ok(at) => at,
            Err(why) => return refusal(RefusalClass::AuthoringRefused, why),
        };
        let asked = (acts.is_none(), trigger.as_ref());
        if let Some(stands) = self.stands_as_saved((&root, &at), &candidate.source, asked, said) {
            return stands;
        }
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
        // The proposal shown before this one, read before the new set, card and money binding:
        // its identity and its set.
        let shown =
            (self.pending.as_ref()).map(|before| (self.proposal_id(before), before.clone()));
        // A revision of a saved workflow stating no ceiling keeps the one saved with it.
        self.keep_saved_ceiling(&at);
        let mut bytes = self.draft_preview(&set);
        let id = ProposalId::of(&bytes);
        // The trial of these very bytes, bound to the proposal that shows them.
        let tried = (driver.verified.lock())
            .ok()
            .and_then(|mut kept| kept.take_tried());
        match tried {
            Some((Ok(tried), words)) => self.bind_trial((&id, &set), (tried, words), &mut bytes),
            Some((Err(why), _)) => self.untried(&why, &mut bytes),
            None => {
                // No trial of its own: the revision shown, proposed again under the verdict the
                // person saw, may carry the rehearsal of those very bytes from that proposal only.
                let shown = (shown.as_ref()).map(|(old, before)| (old, before));
                let reused = reused == Some(candidate.number);
                self.carry_rehearsal((shown, reused), (&id, &set), &mut bytes);
            }
        }
        self.bind_proposal_money(&id);
        // The source basis of these bytes, bound to the identity the person reviews.
        let seen = (driver.verified.lock()).map(|kept| kept.read().clone());
        self.bind_read_basis(&id, &set, &seen.unwrap_or_default());
        self.pending = Some(set);
        self.pending_trigger = trigger;
        driver.card = Some((goal, keys));
        driver.authorized = acts.map(|(acts, cited)| (id.clone(), acts, cited));
        let said = self.changed_words(driver, said);
        TurnOutcome::Proposal {
            id,
            preview: format!("{said}\n\n{bytes}"),
        }
    }

    /// The very bytes saved at `at`, under the same schedule and with no act asked, leave
    /// nothing to decide (NIK-196, a real French journey, 2026-10-16: the saved workflow came
    /// back with Save, Save and run once, and Discard): the person is told it stands as saved
    /// and what moves it on, and the author that its proposal changed nothing.
    fn stands_as_saved(
        &mut self,
        (root, at): (&std::path::Path, &str),
        source: &str,
        (no_acts, trigger): (bool, Option<&TriggerRequirement>),
        said: &str,
    ) -> Option<TurnOutcome> {
        let same_schedule = trigger.is_none_or(|t| {
            t.cron.is_some() && t.cron == self.last_trigger.as_ref().and_then(|l| l.cron.clone())
        });
        let on_disk = std::fs::read_to_string(root.join(at)).ok();
        if !(no_acts
            && same_schedule
            && self.pending.is_none()
            && on_disk.as_deref() == Some(source))
        {
            return None;
        }
        let unchanged = self.saved_as_it_stands(at);
        self.note_for_author(&format!(
            "Your last proposal was the very bytes of the saved workflow `{at}`: the Session told the person nothing changed and how to run or activate it. Propose again only for a change the person asks."
        ));
        Some(TurnOutcome::Facts(format!("{said}\n\n{unchanged}")))
    }

    /// The words of a turn that shows no proposal: the author's own, then, when a proposal it
    /// asked for was refused, what the turn did, whatever those words claim (live gauntlet wave 9
    /// r07, 2026-10-20: « C'est enregistré et lancé » while nothing was proposed, saved or run).
    /// Beside a proposal shown before and still waiting, the turn changed nothing and that
    /// proposal still waits: never « nothing was proposed » beside a proposal the person can
    /// still save (live gauntlet wave 10 r08, 2026-10-20).
    fn refused_said(&self, decided: &Decided, said: String) -> String {
        if !decided.refused {
            return said;
        }
        let fact = if self.pending.is_some() {
            self.language.pick(
                "This turn changed nothing: Nika's check refused the revision, and the proposal shown before still waits for your answer.",
                "Ce tour n'a rien changé : la vérification de Nika a refusé la révision, et la proposition affichée avant attend toujours votre réponse.",
            )
        } else {
            self.language.pick(
                "Nothing was proposed, saved or run in this turn: Nika's check refused the draft.",
                "Rien n'a été proposé, enregistré ni lancé pendant ce tour : la vérification de Nika a refusé le brouillon.",
            )
        };
        if said.trim().is_empty() {
            fact.to_owned()
        } else {
            format!("{said}\n\n{fact}")
        }
    }

    /// What this revision changes in the bytes the person saw, said in words, and the very same
    /// bytes said as such, whatever the author's own words claim.
    fn changed_words(&self, driver: &Driver, said: &str) -> String {
        let (changed, unchanged) = driver
            .toolbox
            .with(|c| (c.changed().join(" · "), c.unchanged()));
        let fr = self.language == crate::Lang::Fr;
        if unchanged && fr {
            format!("{said}\n\nMême workflow que la proposition vue : ses octets n'ont pas changé.")
        } else if unchanged {
            format!("{said}\n\nSame workflow as the proposal you saw: its bytes did not change.")
        } else if changed.is_empty() {
            said.to_owned()
        } else if fr {
            format!("{said}\n\nChangé depuis la proposition vue : {changed}.")
        } else {
            format!("{said}\n\nChanged from the proposal you saw: {changed}.")
        }
    }

    /// What the person reads when a proposal repeats the saved workflow `at` byte for byte, in
    /// their language: nothing to decide, and the gestures that move it on.
    fn saved_as_it_stands(&self, at: &str) -> String {
        let lang = self.language;
        let mut text = match lang {
            super::Lang::Fr => format!(
                "Rien n'a changé : `{at}` est enregistré tel quel, octet pour octet · « lance-le » le lance une fois"
            ),
            _ => format!(
                "Nothing changed: `{at}` is saved as it stands, byte for byte · « run it » runs it once"
            ),
        };
        let root = &self.snapshot.root;
        let installed = |label: &str| self.trigger_host.installed(root, label);
        let declared =
            super::schedule::declared_state(root, std::path::Path::new(at), lang, &installed);
        let scheduled = (self.last_trigger.as_ref()).filter(|t| t.cron.is_some());
        match (declared, scheduled) {
            (Some(state), _) => {
                text.push_str(" · ");
                text.push_str(&state);
            }
            (None, Some(trigger)) => {
                let when = trigger.source_hint.as_deref().unwrap_or_default();
                let says = lang.pick(
                    "« activate » declares its schedule",
                    "« active-le » déclare sa planification",
                );
                for part in [" · ", says, " « ", when, " »"] {
                    text.push_str(part);
                }
            }
            (None, None) => {}
        }
        text
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
        // Said in the person's language (live gauntlet wave 6 r08 B7, 2026-10-18).
        Err(self.language.pick(NOT_A_CONSENT, NOT_A_CONSENT_FR))
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
        // The default the candidate already carries needs no revision: bound, the proposal shown
        // waits on with its next choice or its save decision, and the author is not called.
        if let Some(kept) = self.keep_default(id, line) {
            return kept;
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
            || is_plain_yes(line)
            || is_no(line)
            || is_save_and_run(line)
            || line == "show"
            || line.starts_with('/')
            || local_command_of(line).is_some();
        // A ceiling stated alone is the consent door's monetary amendment, never a revision.
        let ceiling = self.states_money_only(line);
        self.agent_leads() && !line.is_empty() && !protocol && !ceiling
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

/// What a park that asks nothing says: the run waits on the person's next line.
const NOTHING_ASKED: &str =
    "I have no question waiting for you: tell me how to go on, or what to change.";

/// [`NOTHING_ASKED`] in French.
const NOTHING_ASKED_FR: &str =
    "Je n'ai pas de question en attente : dites-moi comment continuer, ou ce qu'il faut changer.";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
