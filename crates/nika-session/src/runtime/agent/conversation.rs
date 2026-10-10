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
use nika_session_change::outcome::QuestionId;
use nika_session_change::tools::ToolReply;
use nika_session_change::work::{
    AskedQuestion, Binding, Delegation, Offer, OfferValue, Provenance, ProvenanceKind, ValueRole,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One of the person's lines, as the tree made it durable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cited {
    /// The tree line it is on: later lines have greater numbers.
    pub(crate) at: u64,
    /// The person's words, as typed.
    pub(crate) text: String,
}

/// The person's lines by citation (`u1`, `u2`, …), indexed as the tree's lines become durable.
#[derive(Clone, Debug, Default)]
pub(crate) struct Citations {
    lines: BTreeMap<String, Cited>,
    last: u64,
}

impl Citations {
    /// The tree line `at` became durable; it records the person's line `cite` when one is given.
    pub(crate) fn record(&mut self, at: u64, person: Option<(String, String)>) {
        self.last = self.last.max(at);
        if let Some((cite, text)) = person {
            self.lines.insert(cite, Cited { at, text });
        }
    }

    /// The person's line `cite`.
    pub(crate) fn get(&self, cite: &str) -> Option<&Cited> {
        self.lines.get(cite)
    }

    /// The last durable line.
    pub(crate) fn last(&self) -> u64 {
        self.last
    }

    /// Every word the person wrote from the tree line `from` on, in order: the request a
    /// candidate answers, as the laws read it.
    pub(crate) fn stated(&self, from: u64) -> String {
        let mut lines: Vec<&Cited> = self.lines.values().filter(|l| l.at >= from).collect();
        lines.sort_by_key(|line| line.at);
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        texts.join("\n")
    }
}

/// One revision of the candidate.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    /// Its number in this request, from 1.
    pub(crate) number: u64,
    /// The complete `.nika` document.
    pub(crate) source: String,
    /// What changed, for the person.
    pub(crate) summary: String,
    /// The selections it carries, as the author stated them and the Session checked them.
    pub(crate) rows: Vec<Resolution>,
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
}

/// What a proposal lets the Session do once every check passed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Acts {
    /// Save the candidate, on the person's own words.
    pub(crate) save: bool,
    /// Run it after saving, through the run's admission.
    pub(crate) run: bool,
}

/// The conversation's state.
#[derive(Debug, Default)]
pub(crate) struct Conversation {
    request: u64,
    since: u64,
    candidate: Option<Candidate>,
    bindings: Vec<Binding>,
    accepted: Vec<Binding>,
    delegations: Vec<Delegation>,
    asked: Vec<AskedQuestion>,
    answered: BTreeMap<String, (AskedQuestion, String)>,
    shown: Option<Shown>,
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
}

#[derive(Serialize, Deserialize)]
struct KeptCandidate {
    number: u64,
    source: String,
    summary: String,
    rows: Vec<Value>,
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

/// A role holds one value at a time when a workflow has one of it: its output, its model.
fn single(role: ValueRole) -> bool {
    matches!(role, ValueRole::OutputPath | ValueRole::RunModel)
}

impl Conversation {
    /// The values the candidate binds.
    pub(crate) fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The choices the person delegated.
    pub(crate) fn delegations(&self) -> &[Delegation] {
        &self.delegations
    }

    /// The questions asked now, open or after their prerequisites.
    pub(crate) fn questions(&self) -> &[AskedQuestion] {
        &self.asked
    }

    /// The identities of the questions asked now.
    pub(crate) fn asked_ids(&self) -> Vec<QuestionId> {
        self.asked.iter().map(|q| q.id.clone()).collect()
    }

    /// The candidate, when one was written.
    pub(crate) fn candidate(&self) -> Option<&Candidate> {
        self.candidate.as_ref()
    }

    /// The tree line the current request starts on: the person's words from it on are what a
    /// candidate answers.
    pub(crate) fn since(&self) -> u64 {
        self.since
    }

    /// The person's line `cite` answered the questions asked now: they answer nothing again.
    pub(crate) fn answered_by(&mut self, cite: &str) {
        for question in std::mem::take(&mut self.asked) {
            self.answered
                .insert(question.key.clone(), (question, cite.to_owned()));
        }
    }

    /// `ask`: bind the answers the author read in the person's line, refuse what the person
    /// already settled (with its value, to the author), ask the rest together. `mint` gives a
    /// question its identity from its context. The turn ends when something is asked.
    pub(crate) fn ask(
        &mut self,
        citations: &Citations,
        args: &Value,
        call: Option<&str>,
        mint: &mut dyn FnMut(&str) -> QuestionId,
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
            match self.question(question, call, ordinal, mint) {
                Ok(question) => asked.push(question),
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
        let asked = (self.answered.get(&key).map(|(q, _)| q))
            .or_else(|| self.asked.iter().find(|q| q.key == key));
        let role = asked.and_then(|q| q.role).unwrap_or(ValueRole::Value);
        let provenance = Provenance::new(ProvenanceKind::Answered, &message).with_excerpt(&excerpt);
        self.bindings
            .retain(|b| b.key.as_deref() != Some(key.as_str()));
        self.bindings
            .push(Binding::new(role, &value, provenance).with_key(&key));
        self.asked.retain(|q| q.key != key);
        Ok(key)
    }

    /// One question to ask, or why the Session answers it itself (the person settled it).
    fn question(
        &self,
        question: &Value,
        call: Option<&str>,
        ordinal: usize,
        mint: &mut dyn FnMut(&str) -> QuestionId,
    ) -> Result<AskedQuestion, Value> {
        let key = question["key"].as_str().unwrap_or_default();
        let role = question["role"].as_str().and_then(role_word);
        let settled = self.bindings.iter().find(|b| {
            b.key.as_deref() == Some(key) || role.is_some_and(|r| single(r) && b.role == r)
        });
        if let Some(settled) = settled {
            return Err(json!({"key": key, "why": "already settled by the person",
                "settled": {"value": settled.value, "message": settled.provenance.message}}));
        }
        let text = question["question"].as_str().unwrap_or_default();
        let context = format!(
            "agent question\n{}\n{}\n{ordinal}\n{key}\n{text}\n{}",
            self.request,
            call.unwrap_or_default(),
            question["options"]
        );
        let options = (question["options"].as_array().into_iter().flatten())
            .map(|option| {
                let values = (option["values"].as_array().into_iter().flatten())
                    .filter_map(|v| {
                        let role = role_word(v["role"].as_str()?)?;
                        let name = v["name"].as_str().map(str::to_owned);
                        Some(OfferValue::new(role, v["value"].as_str()?, name))
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
            .collect();
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
    pub(crate) fn write(
        &mut self,
        citations: &Citations,
        (source, summary): (String, String),
        (rows, removed): (&[Value], &[Value]),
        check: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> ToolReply {
        if let Err(findings) = check(&source) {
            return ToolReply::error(format!("the document does not parse: {findings}"));
        }
        let rows = match Resolution::read_all(rows) {
            Ok(rows) => rows,
            Err(why) => return ToolReply::error(format!("the selections cannot be read: {why}")),
        };
        let mut refusals = Vec::new();
        let mut released = Vec::new();
        for row in removed {
            match Self::removal(citations, row) {
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
        if !refusals.is_empty() {
            return ToolReply::error(refusals.join("\n"));
        }
        self.accepted
            .retain(|kept| !released.iter().any(|v| same_literal(v, &kept.value)));
        // What a proposal the person saw bound stays theirs while the document carries it.
        for kept in &self.accepted {
            let bound = bindings.iter().any(|b| same_literal(&b.value, &kept.value));
            if !bound && source.contains(bare(&kept.value)) {
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
        // An answer bound by its question stays bound beside the document's own selections.
        for answer in self.bindings.iter().filter(|b| b.key.is_some()) {
            if !bindings
                .iter()
                .any(|b| same_literal(&b.value, &answer.value))
            {
                bindings.push(answer.clone());
            }
        }
        let number = self.candidate.as_ref().map_or(1, |c| c.number + 1);
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

    /// A value the person's own cited words remove from what a revision they saw bound.
    fn removal(citations: &Citations, row: &Value) -> Result<String, String> {
        let text = |field: &str| row[field].as_str().unwrap_or_default();
        let (value, message, excerpt) = (text("value"), text("message"), text("excerpt"));
        match citations.get(message) {
            Some(line) if in_line(line, excerpt, None) => Ok(value.to_owned()),
            _ => Err(format!(
                "`{value}` is stated as removed by « {excerpt} », which is not in the person's line {message}"
            )),
        }
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
                let offered = question.as_deref().and_then(|key| self.answered.get(key));
                let fits = offered.is_some_and(|(asked, by)| {
                    *by == message
                        && asked.options.iter().any(|o| {
                            Some(o.key.as_str()) == option.as_deref()
                                && (o.values.iter())
                                    .any(|v| v.role == role && same_literal(&v.value, value))
                        })
                });
                if !fits {
                    return Err(format!(
                        "`{value}` is stated as offered, but no option the person accepted with {message} carries it"
                    ));
                }
                let provenance = Provenance::new(ProvenanceKind::Offered, &message)
                    .with_offer(question.unwrap_or_default(), option);
                Ok(Binding::new(role, value, provenance))
            }
            kind => {
                let excerpt = row.excerpt.clone().unwrap_or_default();
                let line = citations.get(&message).ok_or_else(|| {
                    format!("`{value}` cites `{message}`, no line of the person's")
                })?;
                let typed = (kind == ResolutionKind::Answered).then_some(value);
                if !in_line(line, &excerpt, typed) {
                    return Err(format!(
                        "`{value}` cites « {excerpt} », which is not in the person's line {message}"
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
    pub(crate) fn dropped(&self) -> Vec<String> {
        let replaced =
            |role: ValueRole| single(role) && self.bindings.iter().any(|b| b.role == role);
        (self.accepted.iter())
            .filter(|kept| {
                !(self.bindings.iter()).any(|b| same_literal(&b.value, &kept.value))
                    && !replaced(kept.role)
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
    pub(crate) fn selections(&self) -> (Vec<Resolution>, Vec<Resolution>) {
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
                Resolution::new(&b.value, kind, resolution_role(b.role))
            })
            .collect();
        (authored, host)
    }

    /// `propose`: the candidate is shown to the person. With acts, the person's own line
    /// authorizes them only when it was written after this exact revision's proposal was shown
    /// and the candidate may do nothing more than that revision could (`scope`). Returns the
    /// acts to perform, or why none is authorized (the proposal then waits for consent).
    pub(crate) fn propose(
        &mut self,
        citations: &Citations,
        args: &Value,
        scope: String,
    ) -> Result<Option<Acts>, String> {
        let number = (self.candidate.as_ref())
            .map(|c| c.number)
            .ok_or("no candidate to propose")?;
        let acts: Vec<&str> = (args["acts"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect();
        let wanted = Acts {
            save: acts.contains(&"save") || acts.contains(&"run"),
            run: acts.contains(&"run"),
        };
        let authorized = if wanted.save {
            let said = &args["authorized_by"];
            let message = said["message"].as_str().unwrap_or_default();
            let excerpt = said["excerpt"].as_str().unwrap_or_default();
            match (citations.get(message), &self.shown) {
                (Some(line), Some(shown))
                    if in_line(line, excerpt, None)
                        && line.at > shown.at
                        && shown.scope == scope =>
                {
                    Ok(Some(wanted))
                }
                (Some(_), Some(shown)) if shown.scope != scope => Err(format!(
                    "the person's {message} answered the proposal of revision {}; this candidate may do something else, so it waits for their consent",
                    shown.number
                )),
                _ => Err(format!(
                    "`{message}` does not authorize it: the words must be the person's, written after the proposal they designate"
                )),
            }
        } else {
            Ok(None)
        };
        self.accepted.clone_from(&self.bindings);
        if !matches!(authorized, Ok(Some(_))) {
            self.shown = Some(Shown {
                number,
                scope,
                at: citations.last(),
            });
        }
        authorized
    }

    /// `new_request`: the person replaced the request; nothing of the former one remains, and
    /// their words from the replacing line on are the new request.
    pub(crate) fn replace(&mut self, citations: &Citations, args: &Value) -> ToolReply {
        let message = args["message"].as_str().unwrap_or_default();
        let excerpt = args["excerpt"].as_str().unwrap_or_default();
        match citations.get(message) {
            Some(line) if in_line(line, excerpt, None) => {
                let request = self.request + 1;
                *self = Self {
                    request,
                    since: line.at,
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
    pub(crate) fn kept(&self) -> Value {
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
        };
        serde_json::to_value(kept).unwrap_or(Value::Null)
    }

    /// A conversation read back from what a Session kept: its evidence, no question and no
    /// proposal. `None` for a value this engine does not read.
    pub(crate) fn restored(value: &Value) -> Option<Self> {
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
            accepted: kept.accepted,
            delegations: kept.delegations,
            ..Self::default()
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
