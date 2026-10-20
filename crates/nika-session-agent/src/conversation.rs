// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation an intelligence leads, as the Session holds it: the candidate and its
//! revisions, the values it binds with where each comes from, the choices the person delegated,
//! and the questions asked with their identities. Every check that authorizes reads the
//! person's own cited lines ([`Citations`]), never a tool's reply or the model's text.
//!
//! What the author states about a value's provenance is a claim the Session checks: words it
//! cites must be in the cited line, an accepted offer must be an option the Session showed and
//! the person answered, a kept value must be one a proposal the person saw bound. A revision
//! that drops a value the person saw bound is not proposed unless the person's own line removes
//! it; a single-valued role (the output, the run model) takes a new authorized value instead.

use std::collections::BTreeMap;

use nika_compile_fidelity::fidelity::resolution::{
    Resolution, ResolutionKind, ResolutionRole, anchored, same_literal, typed,
};
use nika_session_change::change::Witness;
use nika_session_change::outcome::QuestionId;
use nika_session_change::tools::ToolReply;
use nika_session_change::work::{
    AskedQuestion, Binding, Delegation, Offer, OfferValue, Provenance, ProvenanceKind, ValueRole,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod answer;
mod answers;
mod changes;
mod choices;
mod continuation;
mod discard;
mod edit;
mod left_model;
mod pick;
mod recall;
mod schedule;

use self::answer::Answered;
use self::answers::AnswerMeaning;
use self::choices::{NotAsked, QuestionUse};
use self::continuation::Continuation;
pub use self::discard::Discarded;
pub use self::edit::edited;
pub use self::pick::{NOT_A_CONSENT, NOT_READ_AS_CONSENT, Pick};
pub use self::recall::{RERUN_NOTE, asking_call};

/// How a question gets its identity from its context.
pub type Mint<'a> = &'a mut dyn FnMut(&str) -> QuestionId;

/// What this machine's inventory says of a model an option offers.
pub type Facts<'a> = &'a mut dyn FnMut(&str) -> Option<nika_session_change::work::ModelFacts>;

/// One of the person's lines, as the tree made it durable.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Cited {
    /// The tree line it is on: later lines have greater numbers.
    pub at: u64,
    /// The person's words, as typed.
    pub text: String,
}

/// The person's lines by citation (`u1`, `u2`, …), indexed as the tree's lines become durable.
#[derive(Clone, Debug, Default)]
pub struct Citations {
    lines: BTreeMap<String, Cited>,
    last: u64,
}

impl Citations {
    /// The tree line `at` became durable; it records the person's line `cite` when one is given.
    pub fn record(&mut self, at: u64, person: Option<(String, String)>) {
        self.last = self.last.max(at);
        if let Some((cite, text)) = person {
            self.lines.insert(cite, Cited { at, text });
        }
    }

    /// The person's line `cite`.
    #[must_use]
    pub fn get(&self, cite: &str) -> Option<&Cited> {
        self.lines.get(cite)
    }

    /// The last durable line.
    #[must_use]
    pub fn last(&self) -> u64 {
        self.last
    }

    /// Whether what was written at the tree line `written` answers the person's line at `line`:
    /// written after it, with no other line of theirs between.
    fn written_in_answer(&self, line: u64, written: u64) -> bool {
        line < written && !(self.lines.values()).any(|l| l.at > line && l.at < written)
    }

    /// Every word the person wrote from the tree line `from` on, in order: the request a
    /// candidate answers, as the laws read it.
    #[must_use]
    pub fn stated(&self, from: u64) -> String {
        let mut lines: Vec<&Cited> = self.lines.values().filter(|l| l.at >= from).collect();
        lines.sort_by_key(|line| line.at);
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        texts.join("\n")
    }

    /// Where the person's line `cite` stands in [`Citations::stated`] from the tree line `from`:
    /// its first line and the one after its last, counted in those joined words; `None` when it is
    /// not among them. Found by the citation, never by the text.
    #[must_use]
    pub fn located(&self, from: u64, cite: &str) -> Option<(usize, usize)> {
        let mut lines: Vec<(&String, &Cited)> = (self.lines.iter())
            .filter(|(_, line)| line.at >= from)
            .collect();
        lines.sort_by_key(|(_, line)| line.at);
        let mut start = 0;
        for (key, line) in lines {
            let count = line.text.split('\n').count();
            if key == cite {
                return Some((start, start + count));
            }
            start += count;
        }
        None
    }

    /// The person's first line from the tree line `from` on, when later lines follow it: the
    /// request as first stated, before the words that answer or correct it.
    #[must_use]
    pub fn first(&self, from: u64) -> Option<&str> {
        let mut lines: Vec<&Cited> = self.lines.values().filter(|l| l.at >= from).collect();
        lines.sort_by_key(|line| line.at);
        (lines.first())
            .filter(|_| lines.len() > 1)
            .map(|line| line.text.as_str())
    }
}

/// One revision of the candidate.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Candidate {
    /// Its number in this request, from 1.
    pub number: u64,
    /// The complete `.nika` document.
    pub source: String,
    /// What changed, for the person.
    pub summary: String,
    /// The selections it carries, as the author stated them and the Session checked them.
    pub rows: Vec<Resolution>,
}

/// The proposal the person was shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    /// The candidate revision it shows.
    pub(crate) number: u64,
    /// The digest of what that revision may do (its reach, permits and model).
    pub(crate) scope: String,
    /// The tree line after which the person's lines were written while it was shown.
    pub(crate) at: u64,
    /// The witness of the bytes it shows.
    pub(crate) witness: String,
    /// The bytes it shows, which the next proposal's changes are read against.
    pub(crate) source: String,
    /// The choices it showed with their defaults, by identity.
    pub(crate) choices: Vec<QuestionId>,
    /// The tree line of the person's words it cited, with the acts it asked on them.
    pub(crate) mandate: Option<(u64, Acts)>,
}

/// What a proposal lets the Session do once every check passed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Acts {
    /// Save the candidate, on the person's own words.
    pub save: bool,
    /// Run it after saving, through the run's admission.
    pub run: bool,
    /// The candidate was prepared or revised in response to the cited words. Those words
    /// must explicitly request creation or modification before saving or running it.
    pub revised: bool,
}

impl Acts {
    /// The acts a `propose` call asks for (`acts`: `save`, `run`): a run is saved first, and
    /// nothing is revised yet.
    fn asked(args: &Value) -> Self {
        let acts: Vec<&str> = (args["acts"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect();
        Self {
            save: acts.contains(&"save") || acts.contains(&"run"),
            run: acts.contains(&"run"),
            revised: false,
        }
    }
}

/// The conversation's state.
#[derive(Debug, Default)]
pub struct Conversation {
    request: u64,
    since: u64,
    candidate: Option<Candidate>,
    bindings: Vec<Binding>,
    accepted: Vec<Binding>,
    delegations: Vec<Delegation>,
    asked: Vec<AskedQuestion>,
    /// When each question was asked: the last durable line then.
    asked_at: BTreeMap<String, u64>,
    answered: BTreeMap<String, Answered>,
    shown: Option<Shown>,
    /// The tree line the current candidate was written at.
    written_at: u64,
    /// What the last proposal changed from the one the person saw, read from their bytes.
    changed: Vec<String>,
    /// Whether the last proposal shows the very bytes of the one the person saw before it.
    unchanged: bool,
    /// The values the last proposal the person saw bound, as it bound them.
    seen: Vec<Binding>,
    /// The choices the last proposal shows with their defaults ([`Conversation::open`]).
    open: Vec<AskedQuestion>,
    /// The zones the schedules of this request's proposals carried as the person saw them: a zone
    /// rides beside the candidate, never in its bytes, so a revision carries it there.
    zones: Vec<String>,
    /// Why the proposal shown cannot be saved as it is: an option of its choices the person
    /// picked instead of the default it carries, until the author proposes again.
    picked_other: Vec<String>,
    /// The first line of why the last `candidate_write` was refused, until one is accepted.
    refused_write: Option<String>,
    /// The workflow this conversation saved last: its project-relative path and the witness of
    /// the bytes saved, which a revision of it replaces.
    saved: Option<(String, String)>,
    /// Latest human line already consumed as a save/run mandate; never replayed after reopen.
    consumed_through: u64,
    /// The last revision a discard dropped with no saved workflow to return to: the next
    /// candidate's number follows it.
    discarded: u64,
    /// Exact, identity-bound clarification picks; evidence, never an open question or consent.
    continuations: BTreeMap<String, Continuation>,
    /// Descriptions of bound choices, evidence without live question authority.
    answer_meanings: BTreeMap<String, AnswerMeaning>,
    /// The offers of the questions kept outputs answered, read back from the tree after a reopen
    /// ([`Conversation::recall`]): which outputs stand apart, never a question, an answer or an
    /// authority, and never kept.
    recalled: BTreeMap<String, Vec<Offer>>,
    /// The person's lines of this request that replied to a question Nika asked, by citation:
    /// marked as the host records them ([`Conversation::replied_by`]) and read back from the tree
    /// after a reopen ([`Conversation::recall`]). Where a reply stands, never an answer, a binding
    /// or an authority, and never kept.
    replied: Vec<String>,
    /// The questions the person dropped (« annuler », « cancel »), by key, with the citation of
    /// the line that dropped each: none is answered or bound, and asked again each is refused.
    dropped: BTreeMap<String, String>,
}

/// What a Session keeps of a conversation across a reopen: the evidence, never an authority
/// (no question, no proposal).
#[derive(Serialize, Deserialize)]
struct Kept {
    version: u32,
    request: u64,
    since: u64,
    candidate: Option<KeptCandidate>,
    bindings: Vec<Binding>,
    accepted: Vec<Binding>,
    delegations: Vec<Delegation>,
    #[serde(default)]
    saved: Option<(String, String)>,
    #[serde(default)]
    consumed_through: u64,
    #[serde(default)]
    continuations: BTreeMap<String, Continuation>,
    #[serde(default)]
    answer_meanings: BTreeMap<String, AnswerMeaning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    zones: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct KeptCandidate {
    number: u64,
    source: String,
    summary: String,
    rows: Vec<Value>,
}

/// Whether `reopens` cites the person's words, in a line written after `settled` was bound,
/// that ask to change it: a settled value is asked again only on them.
fn reopened(citations: &Citations, settled: &Binding, reopens: &Value) -> bool {
    let (Some(message), Some(excerpt)) = (reopens["message"].as_str(), reopens["excerpt"].as_str())
    else {
        return false;
    };
    let since = (citations.get(&settled.provenance.message)).map_or(0, |line| line.at);
    (citations.get(message)).is_some_and(|line| line.at > since && in_line(line, excerpt, None))
}

/// Whether `binding` is the value `question` asks: its key, or its single-valued role, unless the
/// output stands apart from the question ([`stands_apart`], its own question offering `its`).
fn settles(binding: &Binding, question: &AskedQuestion, its: Option<&[Offer]>) -> bool {
    binding.key.as_deref() == Some(question.key.as_str())
        || ((question.role).is_some_and(|role| single(role) && binding.role == role)
            && !stands_apart(binding, its, &Slot::asked(&question.options)))
}

/// The value without the spellings two equal literals differ by (a leading `./`, a trailing
/// `/`).
fn bare(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix("./")
        .unwrap_or(value)
        .trim_end_matches('/')
}

/// Whether the person's line holds `excerpt`, and, for a value they typed, the value itself as
/// a whole word of it ([`typed`]): never a part of another word.
fn in_line(line: &Cited, excerpt: &str, value: Option<&str>) -> bool {
    anchored(&line.text, excerpt) && value.is_none_or(|value| typed(&line.text, value))
}

fn role_of(role: ResolutionRole) -> ValueRole {
    match role {
        ResolutionRole::ReadSource => ValueRole::ReadSource,
        ResolutionRole::OutputPath => ValueRole::OutputPath,
        ResolutionRole::RunModel => ValueRole::RunModel,
        _ => ValueRole::Value,
    }
}

fn resolution_role(role: ValueRole) -> ResolutionRole {
    match role {
        ValueRole::ReadSource => ResolutionRole::ReadSource,
        ValueRole::OutputPath => ResolutionRole::OutputPath,
        ValueRole::RunModel => ResolutionRole::RunModel,
        _ => ResolutionRole::Value,
    }
}

fn role_word(word: &str) -> Option<ValueRole> {
    match word {
        "read_source" => Some(ValueRole::ReadSource),
        "output_path" => Some(ValueRole::OutputPath),
        "run_model" => Some(ValueRole::RunModel),
        "value" => Some(ValueRole::Value),
        _ => None,
    }
}

/// The own name of an address's host: the label before its last one, longer than two letters
/// (`techcrunch` for `techcrunch.com`, `ycombinator` for `news.ycombinator.com`).
fn host_name(value: &str) -> Option<String> {
    let rest = (value.strip_prefix("https://")).or_else(|| value.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#', ':']).next()?;
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    let name = labels.len().checked_sub(2).and_then(|at| labels.get(at))?;
    (name.chars().count() > 2).then(|| name.to_lowercase())
}

/// A role, as the person reads it.
fn role_words(role: ValueRole) -> &'static str {
    match role {
        ValueRole::ReadSource => "source",
        ValueRole::OutputPath => "output",
        ValueRole::RunModel => "model",
        _ => "value",
    }
}

/// A role holds one value at a time when a workflow has one of it: its output, its model.
fn single(role: ValueRole) -> bool {
    matches!(role, ValueRole::OutputPath | ValueRole::RunModel)
}

/// The readings of one open name a question offers: two or more offered files (outputs or values)
/// that are each the longest with words removed from its start, down to the path alone
/// (`générer out/x.json`, `out/x.json`). Such a question asks where that name starts, nothing else.
fn readings_offered(options: &[Offer]) -> Option<Vec<String>> {
    let mut values: Vec<&str> = (options.iter().flat_map(|offer| &offer.values))
        .filter(|value| matches!(value.role, ValueRole::OutputPath | ValueRole::Value))
        .map(|value| bare(&value.value))
        .collect();
    values.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    values.dedup();
    let (longest, rest) = values.split_first()?;
    let alone = rest.last()?;
    let reads = |value: &&str| longest.ends_with(&format!(" {value}"));
    let open = rest.iter().all(reads) && alone.contains('/') && !alone.contains(' ');
    open.then(|| values.iter().map(|value| (*value).to_owned()).collect())
}

/// The outputs one question may fill, as a question or an output bound stands for them: the
/// outputs the question offers (with the value bound, for an output) and, when those offers read
/// one open name, its readings. An output no known question asked stands for its value alone.
#[derive(Default)]
struct Slot<'a> {
    outputs: Vec<&'a str>,
    readings: Option<Vec<String>>,
}

impl<'a> Slot<'a> {
    /// The slot of a question that offers `options`.
    fn asked(options: &'a [Offer]) -> Self {
        let outputs = (options.iter().flat_map(|offer| &offer.values))
            .filter(|value| value.role == ValueRole::OutputPath)
            .map(|value| bare(&value.value))
            .collect();
        let readings = readings_offered(options);
        Self { outputs, readings }
    }

    /// The slot of the output `binding`, its own question offering `its` when known.
    fn bound(binding: &'a Binding, its: Option<&'a [Offer]>) -> Self {
        let mut slot = its.map_or_else(Self::default, Self::asked);
        slot.outputs.push(bare(&binding.value));
        slot
    }

    /// Whether two slots stand apart: one asks where an open name starts, and every output the
    /// other may hold lies outside that name. Such an output neither settles that question nor is
    /// replaced by its answer, whichever came first; a slot offering one of the name's readings
    /// is that name's slot.
    fn apart(&self, other: &Self) -> bool {
        outside(&other.outputs, self.readings.as_deref())
            || outside(&self.outputs, other.readings.as_deref())
    }
}

/// Whether `outputs`, at least one, all lie outside the open name `readings` reads.
fn outside(outputs: &[&str], readings: Option<&[String]>) -> bool {
    readings.is_some_and(|readings| {
        !outputs.is_empty()
            && (outputs.iter()).all(|output| !readings.iter().any(|reading| reading == output))
    })
}

/// Whether the output `binding` stands apart from the slot `asked` ([`Slot::apart`]), its own
/// question offering `its` when known. A value of another role never does.
fn stands_apart(binding: &Binding, its: Option<&[Offer]>, asked: &Slot<'_>) -> bool {
    binding.role == ValueRole::OutputPath && Slot::bound(binding, its).apart(asked)
}

/// The offers of the question `binding` answers, when the conversation holds that question: the
/// one answered now, else the one recalled after a reopen ([`Conversation::recall`]).
fn options_answered<'a>(
    answered: &'a BTreeMap<String, Answered>,
    recalled: &'a BTreeMap<String, Vec<Offer>>,
    binding: &Binding,
) -> Option<&'a [Offer]> {
    let key = binding.key.as_deref()?;
    match answered.get(key) {
        Some(answer) => Some(&answer.question.options),
        None => recalled.get(key).map(Vec::as_slice),
    }
}

/// How the author binds its own choice in a choice the person left to it: a new output's name as
/// derived, a source as delegated (the author choices the selection law admits).
fn chosen_as(role: Option<ValueRole>) -> &'static str {
    if role == Some(ValueRole::OutputPath) {
        "derived"
    } else {
        "delegated"
    }
}

/// Project offered values in order, lending model facts only from the host's inventory.
fn question_options(question: &Value, facts: Facts<'_>) -> Vec<Offer> {
    (question["options"].as_array().into_iter().flatten())
        .map(|option| {
            let values = (option["values"].as_array().into_iter().flatten())
                .filter_map(|v| {
                    let role = role_word(v["role"].as_str()?)?;
                    let name = v["name"].as_str().map(str::to_owned);
                    let value = v["value"].as_str()?;
                    // A model offered carries the inventory's facts, never the author's.
                    let choice = (role == ValueRole::RunModel)
                        .then(|| facts(value))
                        .flatten();
                    Some(OfferValue::new(role, value, name).with_choice(choice))
                })
                .collect();
            let recommended = option["recommended"].as_bool().unwrap_or(false);
            let label = option["label"].as_str().unwrap_or_default();
            Offer::new(
                option["key"].as_str().unwrap_or_default(),
                label,
                recommended,
            )
            .with_values(values)
        })
        .collect()
}

impl Conversation {
    /// The values the candidate binds.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The choices the person delegated.
    #[must_use]
    pub fn delegations(&self) -> &[Delegation] {
        &self.delegations
    }

    /// The questions asked now, open or after their prerequisites.
    #[must_use]
    pub fn questions(&self) -> &[AskedQuestion] {
        &self.asked
    }

    /// Supersede outstanding questions when the person explicitly requests a separate saved
    /// workflow run. This binds no answer or value and returns only the identities to close.
    /// The person dropped the questions asked now with their line `cite` (« annuler »): none is
    /// answered and nothing is bound, and each asked again is refused as dropped. Returns the
    /// keys and identities of the questions dropped.
    pub fn drop_questions(&mut self, cite: &str) -> Vec<(String, QuestionId)> {
        let asked = std::mem::take(&mut self.asked);
        for question in &asked {
            self.dropped.insert(question.key.clone(), cite.to_owned());
        }
        asked.into_iter().map(|q| (q.key, q.id)).collect()
    }

    pub fn supersede_questions(&mut self) -> Vec<QuestionId> {
        std::mem::take(&mut self.asked)
            .into_iter()
            .map(|q| q.id)
            .collect()
    }

    /// Answer only the question named by the host's already-validated identity. Other grouped
    /// questions retain their offers and cannot consume matching keys from this reply.
    pub fn answered_one_by(
        &mut self,
        id: &QuestionId,
        cite: &str,
        line: &str,
        read: &BTreeMap<String, Pick>,
    ) -> Vec<String> {
        self.answering_one(id, (cite, line), false, |conversation| {
            conversation.answered_by(cite, line, read)
        })
    }

    /// The person left the choice of the question the host named to Nika, by the native
    /// control, never by words: its recommended offer is bound as delegated (with none, the
    /// choice is the author's), whatever the label of another offer reads. `line` is the
    /// person's recorded words of that act, `cite` their citation. Other grouped questions
    /// keep waiting, untouched.
    pub fn delegated_one_by(&mut self, id: &QuestionId, cite: &str, line: &str) -> Vec<String> {
        self.answering_one(id, (cite, line), true, |conversation| {
            conversation.answered_with(cite, line, |_| Some(Pick::Delegated))
        })
    }

    /// `answer` over the question named `id` alone: the others are set apart while it binds
    /// and come back after, and a continuation it carries (the delegation act's when
    /// `delegated`) is kept under `cite`.
    fn answering_one(
        &mut self,
        id: &QuestionId,
        (cite, line): (&str, &str),
        delegated: bool,
        answer: impl FnOnce(&mut Self) -> Vec<String>,
    ) -> Vec<String> {
        let continuation = self.continuation(id, line, delegated);
        let (current, others): (Vec<_>, Vec<_>) = std::mem::take(&mut self.asked)
            .into_iter()
            .partition(|question| question.id == *id);
        let (open, unopened): (Vec<_>, Vec<_>) = std::mem::take(&mut self.open)
            .into_iter()
            .partition(|question| question.id == *id);
        let asked = current.first().map(|question| question.key.clone());
        self.asked = current;
        self.open = open;
        let said = answer(self);
        if let Some(key) = asked {
            self.answered_after_shown(&key);
        }
        if let Some(continuation) = continuation {
            self.continuations.insert(cite.to_owned(), continuation);
        }
        self.asked.extend(others);
        self.open.extend(unopened);
        said
    }

    /// The identities of the questions asked now.
    #[must_use]
    pub fn asked_ids(&self) -> Vec<QuestionId> {
        self.asked.iter().map(|q| q.id.clone()).collect()
    }

    /// The candidate, when one was written.
    #[must_use]
    pub fn candidate(&self) -> Option<&Candidate> {
        self.candidate.as_ref()
    }

    /// Whether the candidate's revision waits unproposed: neither the last shown revision nor
    /// the bytes already saved. A saved witness suppresses only this reminder; it restores no
    /// proposal, consent or Run authority when a conversation reopens.
    #[must_use]
    pub fn unproposed(&self) -> bool {
        self.candidate.as_ref().is_some_and(|candidate| {
            let saved = self
                .saved
                .as_ref()
                .is_some_and(|(_, witness)| *witness == Witness::of(candidate.source.as_bytes()).0);
            !saved
                && self
                    .shown
                    .as_ref()
                    .is_none_or(|shown| shown.number != candidate.number)
        })
    }

    /// The Session saved this conversation's workflow at `path` with `bytes`: a later revision
    /// of it replaces that file.
    pub fn saved_at(&mut self, path: &str, bytes: &str) {
        let path = path.trim_start_matches("./").to_owned();
        self.saved = Some((path, Witness::of(bytes.as_bytes()).0));
    }

    /// The workflow this conversation saved last: its project-relative path and the witness of
    /// the bytes saved.
    #[must_use]
    pub fn saved(&self) -> Option<(&str, &str)> {
        (self.saved.as_ref()).map(|(path, witness)| (path.as_str(), witness.as_str()))
    }

    /// What a call that needs the candidate answers when none stands (edit, check, inspect,
    /// verify, trial, propose): when the last `candidate_write` was refused, that refusal and the
    /// first line of its reason, and that the whole candidate is written again.
    #[must_use]
    pub fn no_candidate(&self) -> String {
        match &self.refused_write {
            Some(why) => format!(
                "no candidate yet: your last candidate_write was refused ({why}); fix that and \
                 call candidate_write again, since there is nothing yet for candidate_edit to edit"
            ),
            None => "no candidate yet: write one with candidate_write".to_owned(),
        }
    }

    /// A refused `candidate_write`: its reply, the first line of its reason kept for an edit
    /// that finds no candidate.
    fn refused(&mut self, why: String) -> ToolReply {
        self.refused_write = why.lines().next().map(str::to_owned);
        ToolReply::error(why)
    }

    /// The tree line the current request starts on: the person's words from it on are what a
    /// candidate answers.
    #[must_use]
    pub fn since(&self) -> u64 {
        self.since
    }

    /// `ask`: bind the answers the author read in the person's line, refuse what the person
    /// already settled (with its value, to the author) and what a default can stand for (a
    /// recommended option, an output in the project), each with its reason, and ask the rest
    /// together. `mint` gives a question its identity from its context. The turn ends when
    /// something is asked.
    pub fn ask(
        &mut self,
        citations: &Citations,
        args: &Value,
        call: Option<&str>,
        (mint, facts): (Mint<'_>, Facts<'_>),
    ) -> ToolReply {
        let mut bound = Vec::new();
        let mut refused = Vec::new();
        for row in args["answered"].as_array().into_iter().flatten() {
            match self.bind_answer(citations, row) {
                Ok(key) => bound.push(key),
                Err(why) => refused.push(json!({"key": row["key"], "why": why})),
            }
        }
        let mut asked = Vec::new();
        let questions = args["questions"].as_array().into_iter().flatten();
        for (ordinal, question) in questions.enumerate() {
            // A run model the person left to Nika is taken from its recommended offer, unasked.
            let at = (call, ordinal);
            if let Some(taken) = self.take_left_model(question, citations, at, mint, facts) {
                refused.push(taken);
                continue;
            }
            match self.question(
                question,
                citations,
                (call, ordinal),
                mint,
                facts,
                QuestionUse::Ask,
            ) {
                // A default can stand for it: the proposal shows it in `open` instead.
                Ok(one) => match choices::defaulted(&one, &question["reopens"], citations) {
                    Some(not) => refused.push(not.refusal(&one.key)),
                    None => asked.push(one),
                },
                Err(settled) => refused.push(settled),
            }
        }
        let open: Vec<Value> = (asked.iter())
            .map(|q| json!({"key": q.key, "id": q.id.as_str(), "state": q.state}))
            .collect();
        let text = json!({"asked": open, "bound": bound, "refused": refused}).to_string();
        if asked.is_empty() {
            return ToolReply::ok(text);
        }
        let at = citations.last();
        for question in &asked {
            self.asked_at.insert(question.key.clone(), at);
            self.answer_meanings.remove(&question.key);
            // A settled value the person's later words reopened is the question's now.
            let (answered, recalled) = (&self.answered, &self.recalled);
            let its = |b: &Binding| options_answered(answered, recalled, b);
            (self.bindings).retain(|b| !settles(b, question, its(b)));
        }
        self.asked = asked;
        ToolReply::ends_turn(text)
    }

    /// One answer the author read in the person's line: bound by its question key.
    fn bind_answer(&mut self, citations: &Citations, row: &Value) -> Result<String, String> {
        let text = |field: &str| row[field].as_str().unwrap_or_default().to_owned();
        let (key, value, message, excerpt) =
            (text("key"), text("value"), text("message"), text("excerpt"));
        let line = citations
            .get(&message)
            .ok_or_else(|| format!("`{message}` is no line of the person's"))?;
        if value.is_empty() || !in_line(line, &excerpt, Some(&value)) {
            return Err(format!(
                "`{value}` is not in the person's line {message} as cited; bind only what they typed"
            ));
        }
        let asked = (self.answered.get(&key).map(|answer| &answer.question))
            .or_else(|| self.asked.iter().find(|q| q.key == key));
        let role = asked.and_then(|q| q.role).unwrap_or(ValueRole::Value);
        // A line the Session read as the answer to a question Nika asked keeps that question: this
        // key's own when it recorded the line, else the one that did. Any other line states a
        // typed value.
        let recorded = (self.answered.get(&key))
            .filter(|answer| answer.cite == message)
            .or_else(|| (self.answered.values()).find(|answer| answer.cite == message))
            .map(|answer| answer.question.clone());
        let reply = line.text.clone();
        let provenance = Provenance::new(ProvenanceKind::Answered, &message).with_excerpt(&excerpt);
        self.bindings
            .retain(|b| b.key.as_deref() != Some(key.as_str()));
        self.bindings
            .push(Binding::new(role, &value, provenance).with_key(&key));
        self.asked.retain(|q| q.key != key);
        self.answer_meanings.remove(&key);
        if let Some(question) = recorded {
            self.remember_typed(&key, &question, (&message, &reply));
        }
        Ok(key)
    }

    /// One question to ask, or why the Session answers it itself (the person settled it).
    fn question(
        &self,
        question: &Value,
        citations: &Citations,
        (call, ordinal): (Option<&str>, usize),
        mint: Mint<'_>,
        facts: Facts<'_>,
        purpose: QuestionUse,
    ) -> Result<AskedQuestion, Value> {
        let key = question["key"].as_str().unwrap_or_default();
        let role = question["role"].as_str().and_then(role_word);
        let text = question["question"].as_str().unwrap_or_default();
        let context = format!(
            "agent question\n{}\n{}\n{ordinal}\n{key}\n{text}\n{}",
            self.request,
            call.unwrap_or_default(),
            question["options"]
        );
        let options = question_options(question, facts);
        // Values carry their roles even when the optional question role is omitted.
        // Omitting that field cannot reopen an output or model the person settled. Where an open
        // name starts is that name's question alone: another output never settles it, and its
        // answer never settles another output's question.
        let slot = Slot::asked(&options);
        let settled = self.bindings.iter().find(|binding| {
            let its = options_answered(&self.answered, &self.recalled, binding);
            let matches = binding.key.as_deref() == Some(key)
                || ((role.is_some_and(|role| single(role) && binding.role == role)
                    || options
                        .iter()
                        .flat_map(|offer| &offer.values)
                        .any(|value| single(value.role) && binding.role == value.role))
                    && !stands_apart(binding, its, &slot));
            matches
                && !(purpose == QuestionUse::Propose
                    && self.proposable_output_default(binding, key, &options))
        });
        if let Some(settled) = settled
            && !reopened(citations, settled, &question["reopens"])
        {
            return Err(json!({"key": key, "reason": NotAsked::Settled.word(),
                "why": "already settled by the person",
                "settled": {"value": settled.value, "message": settled.provenance.message}}));
        }
        if let Some(cite) = self.dropped.get(key) {
            let why = format!(
                "the person dropped this question with {cite}: do not ask it again; go on without it, or say what you will do instead"
            );
            return Err(json!({"key": key, "reason": NotAsked::Dropped.word(), "why": why}));
        }
        if let Some(message) = self.left_to_author(key) {
            let why = format!(
                "the person left it to you: choose it, bind it as {} citing their words, and say what you chose",
                chosen_as(role)
            );
            let reason = NotAsked::Delegated.word();
            let delegated = json!({"message": message});
            return Err(json!({"key": key, "reason": reason, "why": why, "delegated": delegated}));
        }
        let mut offered = options
            .iter()
            .flat_map(|offer: &Offer| &offer.values)
            .peekable();
        let bound = |v: &OfferValue| {
            (self.bindings.iter()).any(|b| {
                b.role == v.role
                    && same_literal(&b.value, &v.value)
                    && (v.role != ValueRole::Value || b.key.as_deref() == Some(key))
            })
        };
        if offered.peek().is_some() && offered.all(bound) {
            return Err(json!({"key": key, "reason": NotAsked::Bound.word(),
                "why": "every value it offers is already bound: nothing is left to ask"}));
        }
        let after = (question["after"].as_array().into_iter().flatten())
            .filter_map(|k| k.as_str().map(str::to_owned))
            .collect();
        let free_text = question["free_text"].as_bool().unwrap_or(true);
        let multi_select = question["multi_select"].as_bool().unwrap_or(false);
        let mut asked = AskedQuestion::new(mint(&context), key, text)
            .after(after)
            .with_options(options, free_text, multi_select);
        if let Some(why) = question["why"].as_str() {
            asked = asked.with_why(why);
        }
        if let Some(role) = role {
            asked = asked.with_role(role);
        }
        Ok(asked)
    }

    /// `candidate_write` and `candidate_edit`: a complete document whose selections the
    /// Session checks; `check` parses it. A value a proposal the person saw bound stays bound,
    /// with its own provenance, while the document still carries it; one the person's cited
    /// words remove (`removed`) is no longer theirs to keep.
    pub fn write(
        &mut self,
        citations: &Citations,
        (source, summary): (String, String),
        (rows, removed): (&[Value], &[Value]),
        check: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> ToolReply {
        if let Err(findings) = check(&source) {
            return self.refused(format!("the document does not parse: {findings}"));
        }
        let rows = match Resolution::read_all(rows) {
            Ok(rows) => rows,
            Err(why) => return self.refused(format!("the selections cannot be read: {why}")),
        };
        let mut refusals = Vec::new();
        let mut released = Vec::new();
        for row in removed {
            match self.removal(citations, row) {
                Ok(value) => released.push(value),
                Err(why) => refusals.push(why),
            }
        }
        let mut bindings = Vec::new();
        for row in &rows {
            match self.provenance(citations, row) {
                Ok(binding) => bindings.push(binding),
                Err(why) => refusals.push(why),
            }
        }
        // A value kept from a proposal the person saw must still be in the document (or beside it).
        for row in rows.iter().filter(|r| r.kind == ResolutionKind::Retained) {
            if !self.carries(&source, &row.value) {
                refusals.push(format!(
                    "`{}` is stated as kept, but the document no longer carries it: keep it, or state in `removed` the person's words that remove it",
                    row.value
                ));
            }
        }
        if !refusals.is_empty() {
            return self.refused(refusals.join("\n"));
        }
        self.refused_write = None;
        self.accepted
            .retain(|kept| !released.iter().any(|v| same_literal(v, &kept.value)));
        // What a proposal the person saw bound stays theirs while the document carries it.
        for kept in &self.accepted {
            let bound = bindings.iter().any(|b| same_literal(&b.value, &kept.value));
            if !bound && self.carries(&source, &kept.value) {
                bindings.push(kept.clone());
            }
        }
        for row in rows.iter().filter(|r| r.kind == ResolutionKind::Delegated) {
            if let (Some(message), Some(excerpt)) = (&row.message, &row.excerpt)
                && !(self.delegations.iter())
                    .any(|d| d.message == *message && d.excerpt == *excerpt)
            {
                let delegation = Delegation::new(message, excerpt, role_of(row.role));
                self.delegations.push(delegation);
            }
        }
        // An answer bound by its question stays bound beside the document's own selections,
        // unless the person's own words removed it. Once shown as an accepted value, the
        // document-presence check above owns retention: carrying the old answer here would
        // hide a value the new candidate silently dropped.
        let answers = (self.bindings.iter()).filter(|b| {
            b.key.is_some()
                && !self.accepted.contains(b)
                && !released.iter().any(|value| same_literal(value, &b.value))
        });
        for answer in answers {
            if !bindings.iter().any(|b| b == answer) {
                bindings.push(answer.clone());
            }
        }
        let number = (self.candidate.as_ref()).map_or(self.discarded + 1, |c| c.number + 1);
        self.written_at = citations.last();
        self.candidate = Some(Candidate {
            number,
            source,
            summary,
            rows,
        });
        self.bindings = bindings;
        let bound: Vec<Value> = (self.bindings.iter())
            .map(|b| {
                json!({"value": b.value, "role": b.role, "kind": b.provenance.kind,
                    "message": b.provenance.message})
            })
            .collect();
        ToolReply::ok(json!({"revision": number, "bindings": bound}).to_string())
    }

    /// Whether the candidate `source` carries `value`, one a proposal the person saw bound: in its
    /// bytes, or beside them as the zone a schedule they saw carried (as `open` reads the zone
    /// shown beside the candidate), while a binding of theirs still holds it: once their answer
    /// changed it, the old zone is carried no more.
    fn carries(&self, source: &str, value: &str) -> bool {
        source.contains(bare(value))
            || (self.zones.iter().any(|zone| zone == value)
                && (self.bindings.iter()).any(|binding| same_literal(&binding.value, value)))
    }

    /// A value the person's own cited words remove from what a revision they saw bound: the
    /// words must be theirs and name the value.
    fn removal(&self, citations: &Citations, row: &Value) -> Result<String, String> {
        let text = |field: &str| row[field].as_str().unwrap_or_default();
        let (value, message, excerpt) = (text("value"), text("message"), text("excerpt"));
        let replaced_by = row["replaced_by"].as_str();
        match citations.get(message) {
            Some(line) if in_line(line, excerpt, None) && self.names(excerpt, value) => {
                Ok(value.to_owned())
            }
            // Words that give it its replacement, typed in the same line, remove it too.
            Some(line)
                if in_line(line, excerpt, None)
                    && replaced_by.is_some_and(|new| typed(&line.text, new)) =>
            {
                Ok(value.to_owned())
            }
            // So does the line that left its choice to Nika: it is replaced within that choice.
            Some(line) if in_line(line, excerpt, None) && self.left_with(message, value) => {
                Ok(value.to_owned())
            }
            Some(line) if in_line(line, excerpt, None) => Err(format!(
                "« {excerpt} » does not name `{value}`: a value the person saw bound is removed only by words that name it (its address, its host's name or the name it was offered under), by words that give its replacement (`replaced_by`, typed in that line) or by a choice of it they left to you; ask them whether they meant to drop it"
            )),
            _ => Err(format!(
                "`{value}` is stated as removed by « {excerpt} », which is not in the person's line {message}"
            )),
        }
    }

    /// Whether the person's line `message` left the choice of `value`'s role to Nika.
    fn left_with(&self, message: &str, value: &str) -> bool {
        let role = (self.accepted.iter()).find(|kept| same_literal(&kept.value, value));
        role.is_some_and(|kept| {
            (self.delegations.iter()).any(|d| d.message == message && d.scope == kept.role)
        })
    }

    /// Whether the person's words `excerpt` name `value`: the value as written, its host's own
    /// name (`techcrunch` for `https://techcrunch.com/feed/`), a name an offer gave it, or the
    /// words that bound it.
    fn names(&self, excerpt: &str, value: &str) -> bool {
        let words = excerpt.to_lowercase();
        let says = |name: &str| {
            let name = name.trim().to_lowercase();
            name.chars().count() > 2 && words.contains(&name)
        };
        if says(bare(value)) || host_name(value).is_some_and(|name| says(&name)) {
            return true;
        }
        let questions = (self.asked.iter()).chain(self.answered.values().map(|a| &a.question));
        let offered = (questions.flat_map(|q| q.options.iter().flat_map(|o| &o.values)))
            .filter(|offered| same_literal(&offered.value, value))
            .filter_map(|offered| offered.name.as_deref());
        let bound = (self.accepted.iter().chain(&self.bindings))
            .filter(|binding| same_literal(&binding.value, value))
            .filter_map(|binding| binding.provenance.excerpt.as_deref());
        offered.chain(bound).any(says)
    }

    /// The provenance a selection has, checked against what the Session holds.
    fn provenance(&self, citations: &Citations, row: &Resolution) -> Result<Binding, String> {
        let value = row.value.as_str();
        let message = row.message.clone().unwrap_or_default();
        let role = role_of(row.role);
        match row.kind {
            ResolutionKind::Retained => (self.accepted.iter())
                .find(|b| same_literal(&b.value, value) && b.role == role)
                .cloned()
                .ok_or_else(|| {
                    format!("`{value}` is stated as kept, but no proposal the person saw bound it")
                }),
            ResolutionKind::Offered => {
                let (question, option) = (row.question.clone(), row.option.clone());
                let answer = question.as_deref().and_then(|key| self.answered.get(key));
                let fits = answer.is_some_and(|answer| {
                    answer.offers(&message, option.as_deref(), (role, value))
                });
                if !fits {
                    let (question, option) = (question.as_deref(), option.as_deref());
                    return Err(self.not_picked(question, option, (&message, value)));
                }
                let question = question.unwrap_or_default();
                let provenance = Provenance::new(ProvenanceKind::Offered, &message)
                    .with_offer(&question, option);
                Ok(Binding::new(role, value, provenance).with_key(question))
            }
            kind => {
                let excerpt = row.excerpt.clone().unwrap_or_default();
                let line = citations.get(&message).ok_or_else(|| {
                    format!("`{value}` cites `{message}`, no line of the person's")
                })?;
                if !in_line(line, &excerpt, None) {
                    return Err(format!(
                        "`{value}` cites « {excerpt} », which is not in the person's line {message}"
                    ));
                }
                // The words are theirs, the value is not what they typed: they named it. Said as
                // the excerpt's fault, the author retried other excerpts of the same line for
                // ever (live gauntlet, 2026-10-16: Grok 4.7 cited « Grok 4.7 (xAI) » as the
                // answered `xai/grok-4.7` four times, refused each time as not in the line).
                if kind == ResolutionKind::Answered && !in_line(line, &excerpt, Some(value)) {
                    return Err(format!(
                        "`{value}` is stated as answered, which says the person typed it, but their line {message} does not spell `{value}`: if « {excerpt} » names it, state it as named (kind `named`, the same excerpt); otherwise ask them"
                    ));
                }
                // Words Nika had to ask about did not name the value: the answer settles it.
                if let Some(asked) = (kind == ResolutionKind::Named)
                    .then(|| self.asked_about(value, line.at))
                    .flatten()
                {
                    return Err(format!(
                        "`{value}` answers `{asked}`, which Nika asked because « {excerpt} » did not settle it: bind it from the person's answer (offered or delegated), never as named"
                    ));
                }
                let kind = match kind {
                    ResolutionKind::Named => ProvenanceKind::Named,
                    ResolutionKind::Delegated => ProvenanceKind::Delegated,
                    ResolutionKind::Derived => ProvenanceKind::Derived,
                    _ => ProvenanceKind::Answered,
                };
                let provenance = Provenance::new(kind, &message).with_excerpt(&excerpt);
                Ok(Binding::new(role, value, provenance))
            }
        }
    }

    /// Why the candidate cannot be proposed as it is: each value a proposal the person saw
    /// bound that the candidate dropped unasked. A single-valued role that took another
    /// authorized value replaced it; nothing was dropped.
    #[must_use]
    pub fn dropped(&self) -> Vec<String> {
        // An open name's answer replaces that name's slot only, never another output.
        let apart = |a: &Binding, b: &Binding| {
            let its = |binding: &Binding| options_answered(&self.answered, &self.recalled, binding);
            stands_apart(a, its(a), &Slot::bound(b, its(b)))
        };
        let replaced = |kept: &Binding| {
            single(kept.role)
                && (self.bindings.iter()).any(|b| b.role == kept.role && !apart(kept, b))
        };
        (self.accepted.iter())
            .filter(|kept| {
                !(self.bindings.iter()).any(|b| same_literal(&b.value, &kept.value))
                    && !replaced(kept)
            })
            .map(|kept| {
                format!(
                    "`{}` was bound by the person's {} and is no longer in the candidate: keep it, or state in `removed` the person's words that remove it",
                    kept.value, kept.provenance.message
                )
            })
            .collect()
    }

    /// The candidate's selections for the judge: the ones the author states on the person's
    /// words (their words are read again), and the ones the Session verified itself (an
    /// accepted offer, a kept value, an answer bound by its question).
    #[must_use]
    pub fn selections(&self) -> (Vec<Resolution>, Vec<Resolution>) {
        let authored = (self.candidate.iter().flat_map(|c| &c.rows))
            .filter(|r| {
                matches!(
                    r.kind,
                    ResolutionKind::Named
                        | ResolutionKind::Delegated
                        | ResolutionKind::Derived
                        | ResolutionKind::Answered
                ) && !(self.bindings.iter())
                    .any(|b| b.key.is_some() && same_literal(&b.value, &r.value))
            })
            .cloned()
            .collect();
        let host = (self.bindings.iter())
            .filter(|b| {
                b.key.is_some()
                    || matches!(
                        b.provenance.kind,
                        ProvenanceKind::Offered | ProvenanceKind::Retained
                    )
                    || self.accepted.contains(b)
            })
            .map(|b| {
                let kind = match b.provenance.kind {
                    ProvenanceKind::Offered => ResolutionKind::Offered,
                    ProvenanceKind::Answered => ResolutionKind::Answered,
                    _ => ResolutionKind::Retained,
                };
                let mut row = Resolution::new(&b.value, kind, resolution_role(b.role));
                row.message = Some(b.provenance.message.clone());
                row.excerpt.clone_from(&b.provenance.excerpt);
                row.question.clone_from(&b.provenance.question);
                row.option.clone_from(&b.provenance.option);
                row
            })
            .collect();
        (authored, host)
    }

    /// `propose`: the candidate is shown to the person, with the choices `open` shows beside it
    /// with their defaults ([`Conversation::open_choices`]). With acts, the person's own line
    /// may explicitly request preparation, saving and running before the first candidate, or
    /// answer a shown revision whose effect scope stays unchanged (`scope`); a proposal that
    /// shows choices waits for the person's consent to them, their next line. Returns the acts to
    /// perform, or why none is authorized (the proposal then waits for consent).
    ///
    /// # Errors
    ///
    /// When there is no candidate, or when the cited line does not authorize the acts asked for.
    pub fn propose(
        &mut self,
        citations: &Citations,
        args: &Value,
        (scope, open): (String, Vec<AskedQuestion>),
    ) -> Result<Option<Acts>, String> {
        let (number, witness, source) = (self.candidate.as_ref())
            .map(|c| {
                (
                    c.number,
                    Witness::of(c.source.as_bytes()).0,
                    c.source.clone(),
                )
            })
            .ok_or_else(|| self.no_candidate())?;
        let wanted = Acts::asked(args);
        let authorization = self.authorization_line(citations, &args["authorized_by"]);
        let before = self.shown.as_ref().map(|shown| shown.source.as_str());
        (self.changed, self.unchanged) = self.changes_from(before, &source);
        let authorized = if wanted.save && !open.is_empty() {
            // Choices shown with their defaults are the person's to see: no earlier line consents.
            Err("this proposal shows choices with their defaults the person has not seen: it waits for their consent".to_owned())
        } else if wanted.save {
            let cited = args["authorized_by"]["message"]
                .as_str()
                .unwrap_or_default();
            let message = authorization.map_or(cited, |(message, _)| message);
            match (authorization.map(|(_, line)| line), &self.shown) {
                // An explicit preparation mandate may precede its first candidate. Its exact
                // human words are still read independently by Pick::consented, after the whole
                // candidate passed verification. Only exact bound clarification picks may
                // follow it; their meaning also reaches the independent consent reading.
                // If a proposal has already shown defaults, the author rewrites after it, or
                // proposes its very bytes again once the person picked each default they carry.
                (Some(line), _)
                    if line.at >= self.since
                        && line.at > self.consumed_through
                        && self.mandate_continues(citations, line.at, (number, &scope, wanted)) =>
                {
                    Ok(Some(Acts {
                        revised: true,
                        ..wanted
                    }))
                }
                // Only the person's words written after the proposal shown answer it: those that
                // ran a failed revision authorize nothing, whatever the candidate does now.
                (Some(line), Some(shown))
                    if line.at > shown.at && line.at > self.consumed_through =>
                {
                    let revised = shown.number != number;
                    if shown.scope != scope {
                        Err(format!(
                            "the person's {message} answered the proposal of revision {}; this candidate may do something else, so it waits for their consent",
                            shown.number
                        ))
                    } else if revised && !citations.written_in_answer(line.at, self.written_at) {
                        Err(format!(
                            "the person's {message} answered the proposal of revision {}; revision {number} was not written in answer to it, so it waits for their consent",
                            shown.number
                        ))
                    } else {
                        Ok(Some(Acts { revised, ..wanted }))
                    }
                }
                _ => Err(format!(
                    "`{message}` does not authorize it: the words must be the person's, written after the proposal they designate"
                )),
            }
        } else {
            Ok(None)
        };
        if matches!(authorized, Ok(Some(_)))
            && let Some((_, line)) = authorization
        {
            self.consumed_through = line.at;
        }
        self.keep_zone(citations, args, &open);
        self.accepted.clone_from(&self.bindings);
        self.seen.clone_from(&self.bindings);
        self.supersede_covered(&open);
        self.open = open;
        self.picked_other.clear();
        // What the person is shown now: a line authorizes acts only when written after it, so
        // one line authorizes one proposal, never a later revision (a repair after a run).
        self.shown = Some(Shown {
            number,
            scope,
            at: citations.last(),
            witness,
            source,
            choices: self.open.iter().map(|choice| choice.id.clone()).collect(),
            mandate: authorization.map(|(_, line)| (line.at, wanted)),
        });
        authorized
    }

    /// The zone a proposal's schedule carries, as the person sees it: it rides beside every
    /// later revision.
    fn keep_zone(&mut self, citations: &Citations, args: &Value, open: &[AskedQuestion]) {
        if let Ok(Some((_, zone))) = self.schedule(citations, args, open)
            && !self.zones.contains(&zone)
        {
            self.zones.push(zone);
        }
    }

    /// What the last proposal changed from the one the person saw before it, in words read
    /// from their bytes: each value bound then or now that the bytes gained or lost, by its role,
    /// with the size of the diff; else the diff's size and its first lines. Empty for the first
    /// proposal and for the very same bytes ([`Conversation::unchanged`]).
    #[must_use]
    pub fn changed(&self) -> &[String] {
        &self.changed
    }

    /// Whether the last proposal shows the very bytes of the one the person saw before it.
    #[must_use]
    pub fn unchanged(&self) -> bool {
        self.unchanged
    }

    /// What a proposal of `after` changes from the one the person saw (`before`, its bytes), and
    /// whether those are its very bytes. QA gauntlet q03 and q04: a change said from the values
    /// the author bound (« model … added ») under bytes and a proposal id that never changed.
    fn changes_from(&self, before: Option<&str>, after: &str) -> (Vec<String>, bool) {
        let Some(before) = before else {
            return (Vec::new(), false);
        };
        if before == after {
            return (Vec::new(), true);
        }
        let (added, removed) = changes::diff(before, after);
        let mut said = self.changes_since_seen(before, after);
        let valued = !said.is_empty();
        said.push(changes::stat(&added, &removed));
        if !valued {
            said.extend(changes::first("first added", &added));
            said.extend(changes::first("first removed", &removed));
        }
        (said, false)
    }

    /// The verdict of the proposal the person was shown, while the candidate is still the
    /// revision it showed: their words consenting to it are no reason to judge its bytes again.
    #[must_use]
    pub fn shown_verdict(&self) -> Option<&str> {
        let shown = self.shown.as_ref()?;
        let current = self.candidate.as_ref().map(|c| c.number);
        (current == Some(shown.number)).then_some(shown.scope.as_str())
    }

    /// The values bound now against the ones the last proposal the person saw bound, each said
    /// only where the bytes changed with it: one dropped is in `before` and not in `after`, one
    /// added is in `after` and not in `before`.
    fn changes_since_seen(&self, before: &str, after: &str) -> Vec<String> {
        let held = |value: &str| changes::written(before, value);
        let holds = |value: &str| changes::written(after, value);
        let seen = |value: &str| (self.seen.iter()).any(|kept| same_literal(&kept.value, value));
        let now = |value: &str| self.bindings.iter().any(|b| same_literal(&b.value, value));
        let mut said = Vec::new();
        let mut roles: Vec<ValueRole> = Vec::new();
        for binding in self.seen.iter().chain(&self.bindings) {
            if !roles.contains(&binding.role) {
                roles.push(binding.role);
            }
        }
        for role in roles {
            let of = |list: &[Binding], keep: &dyn Fn(&str) -> bool| -> Vec<String> {
                (list.iter())
                    .filter(|b| b.role == role && keep(&b.value))
                    .map(|b| format!("`{}`", b.value))
                    .collect()
            };
            let gone = of(&self.seen, &|value| {
                !now(value) && held(value) && !holds(value)
            });
            let added = of(&self.bindings, &|value| {
                !seen(value) && !held(value) && holds(value)
            });
            let what = role_words(role);
            match (gone.is_empty(), added.is_empty()) {
                (false, false) => said.push(format!(
                    "{what} {} replaced by {}",
                    gone.join(", "),
                    added.join(", ")
                )),
                (false, true) => said.push(format!("{what} {} dropped", gone.join(", "))),
                (true, false) if !self.accepted.is_empty() => {
                    said.push(format!("{what} {} added", added.join(", ")));
                }
                _ => {}
            }
        }
        said
    }

    /// `new_request`: the person replaced the request; nothing of the former one remains, and
    /// their words from the replacing line on are the new request.
    pub fn replace(&mut self, citations: &Citations, args: &Value) -> ToolReply {
        let message = args["message"].as_str().unwrap_or_default();
        let excerpt = args["excerpt"].as_str().unwrap_or_default();
        match citations.get(message) {
            Some(line) if in_line(line, excerpt, None) => {
                let request = self.request + 1;
                *self = Self {
                    request,
                    since: line.at,
                    // A new request keeps the workflow saved: a revision with its name replaces it.
                    saved: self.saved.take(),
                    consumed_through: self.consumed_through,
                    ..Self::default()
                };
                ToolReply::ok(json!({"request": request}).to_string())
            }
            _ => ToolReply::error(format!(
                "`{message}` with « {excerpt} » is no line of the person's: a replacement is theirs to make"
            )),
        }
    }

    /// What a Session keeps across a reopen.
    #[must_use]
    pub fn kept(&self) -> Value {
        let candidate = self.candidate.as_ref().map(|c| KeptCandidate {
            number: c.number,
            source: c.source.clone(),
            summary: c.summary.clone(),
            rows: c.rows.iter().map(Resolution::to_json).collect(),
        });
        let kept = Kept {
            version: 1,
            request: self.request,
            since: self.since,
            candidate,
            bindings: self.bindings.clone(),
            accepted: self.accepted.clone(),
            delegations: self.delegations.clone(),
            saved: self.saved.clone(),
            consumed_through: self.consumed_through,
            continuations: self.continuations.clone(),
            answer_meanings: self.answer_meanings.clone(),
            zones: self.zones.clone(),
        };
        serde_json::to_value(kept).unwrap_or(Value::Null)
    }

    /// A conversation read back from what a Session kept: its evidence, no question and no
    /// proposal. `None` for a value this engine does not read.
    #[must_use]
    pub fn restored(value: &Value) -> Option<Self> {
        let kept: Kept = serde_json::from_value(value.clone()).ok()?;
        if kept.version != 1 {
            return None;
        }
        let candidate = match kept.candidate {
            Some(c) => Some(Candidate {
                number: c.number,
                source: c.source,
                summary: c.summary,
                rows: Resolution::read_all(&c.rows).ok()?,
            }),
            None => None,
        };
        Some(Self {
            request: kept.request,
            since: kept.since,
            candidate,
            bindings: kept.bindings,
            seen: kept.accepted.clone(),
            accepted: kept.accepted,
            delegations: kept.delegations,
            saved: kept.saved,
            consumed_through: kept.consumed_through,
            continuations: kept.continuations,
            answer_meanings: kept.answer_meanings,
            zones: kept.zones,
            ..Self::default()
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
