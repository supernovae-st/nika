// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The durable authoring round (C7): what a host keeps of one compile conversation so that,
//! after a quit, a TERM or a KILL, a person finds the request, the settled answers and the
//! question that waited, and can continue the same round explicitly under the project as it
//! is now. Pure: a round's parts in, a record value out; a record value in, a reading out.
//! The live round's own law lives here too, for the host's round to delegate to (C10): the
//! typed request it is, what an outcome settled, the questions it leaves, a re-anchored plan
//! and the receipt a replay carries.
//!
//! Evidence, never authority. A record carries no account, admission, review, consent,
//! question identity or monetary span: whatever continues it passes the host's current gates
//! again, and the compiler judges its answers against the project as it is then. Every
//! executable text (the request, each answer, an EDIT's path, base, change and original) is
//! kept with the sha256 of its exact original: a text the redactor changed is kept as
//! displayed and never continued. The compiler's continuation is kept only when the redactor
//! leaves it whole and it fits the bound; otherwise only its sha256 is kept and nothing
//! continues.
//!
//! Record (JSON, schema 1, every key always written): `{"schema": 1, "request": <text>,
//! "edit": null | {"path": null | <text>, "base": <text>, "change": <text>, "original": null
//! | <text>}, "answers": [{"key", "literal": <text>}], "questions": [{"key", "label", "type",
//! "why", "mandatory", "options": [{"key", "label"}]}], "reasons": […], "restatements": n,
//! "continuation": null | {"sha256", "value": null | <plan>, "withheld": null | "over_bound"
//! | "redacted"}, "knowledge": null | {…}, "authoring_receipt": null | {…}, "revises": null |
//! "<proposal id>"}`, where `<text>` is `{"text", "sha256"}`. The reading is strict: a key
//! this schema does not know, a key missing, a value of another type makes the whole value
//! unreadable. A value this engine cannot read (another schema, a malformed record, no
//! schema) is kept byte for byte by its host and never used.

use std::collections::BTreeMap;

use nika_event::source_id::sha256_hex;
use serde_json::{Map, Value, json};

use super::reading::CLARIFICATION_KEY;
use super::{
    AuthoringReceipt, CompileOutcome, CompileQuestion, CompileRequest, DiagnosticKind, QuestionType,
};

/// The round schema this engine writes and reads.
pub const ROUND_SCHEMA: u64 = 1;
/// The kept round's bound, serialized: far below a host journal's per-record bound. Over it,
/// the continuation, knowledge and receipt are withheld; a round still over it is not kept.
pub const ROUND_LIMIT: usize = 256 * 1024;
/// The most answers a kept round carries; a round with more is kept and never continued.
pub const MAX_ANSWERS: usize = 64;

/// A text as kept: as displayed (exact, or redacted), and the sha256 of its exact original.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeptText {
    /// The text as kept.
    pub text: String,
    /// The sha256 of the exact original text.
    pub sha256: String,
}

impl KeptText {
    fn of(exact: &str, redact: &dyn Fn(&str) -> String) -> Self {
        Self {
            text: redact(exact),
            sha256: sha256_hex(exact.as_bytes()),
        }
    }

    /// Whether the kept text is the exact original (no redaction changed it).
    #[must_use]
    pub fn is_exact(&self) -> bool {
        sha256_hex(self.text.as_bytes()) == self.sha256
    }

    fn value(&self) -> Value {
        json!({"text": self.text, "sha256": self.sha256})
    }

    fn read(value: &Value, at: &str) -> Result<Self, String> {
        let map = object(value, at, &["text", "sha256"])?;
        Ok(Self {
            text: string(map, at, "text")?,
            sha256: string(map, at, "sha256")?,
        })
    }
}

/// A revision's EDIT as kept: the workflow it revised (when it was a saved file), its exact
/// base, the human's change, and the request the base answered.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeptEdit {
    /// The project-relative path of the saved workflow revised, when it was one.
    pub path: Option<KeptText>,
    /// The exact base the change applies to.
    pub base: KeptText,
    /// The human's change.
    pub change: KeptText,
    /// The request the base answered, when known.
    pub original: Option<KeptText>,
}

impl KeptEdit {
    /// The EDIT as a live round carries it: `(base, change, original)`, as kept.
    #[must_use]
    pub fn texts(&self) -> (String, String, Option<String>) {
        (
            self.base.text.clone(),
            self.change.text.clone(),
            self.original.as_ref().map(|o| o.text.clone()),
        )
    }

    fn is_exact(&self) -> bool {
        self.base.is_exact()
            && self.change.is_exact()
            && self.path.as_ref().is_none_or(KeptText::is_exact)
            && self.original.as_ref().is_none_or(KeptText::is_exact)
    }

    fn value(&self) -> Value {
        json!({
            "path": self.path.as_ref().map(KeptText::value),
            "base": self.base.value(),
            "change": self.change.value(),
            "original": self.original.as_ref().map(KeptText::value),
        })
    }

    fn read(value: &Value) -> Result<Self, String> {
        let at = "edit";
        let map = object(value, at, &["path", "base", "change", "original"])?;
        Ok(Self {
            path: nullable(field(map, at, "path")?, |v| KeptText::read(v, "edit.path"))?,
            base: KeptText::read(field(map, at, "base")?, "edit.base")?,
            change: KeptText::read(field(map, at, "change")?, "edit.change")?,
            original: nullable(field(map, at, "original")?, |v| {
                KeptText::read(v, "edit.original")
            })?,
        })
    }
}

/// One settled answer: the compiler's key and the JSON literal it took.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeptAnswer {
    /// The question's semantic key.
    pub key: String,
    /// The JSON literal bound to it.
    pub literal: KeptText,
}

/// One admissible answer of a choice question.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeptOption {
    /// The answer, verbatim.
    pub key: String,
    /// What choosing it means.
    pub label: String,
}

/// A question as the compile document shows it: evidence to display, never a question to
/// answer (a continuation asks the compiler again).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeptQuestion {
    /// The semantic key.
    pub key: String,
    /// The human wording.
    pub label: String,
    /// The answer shape: `text` · `literal` · `choice`.
    pub kind: String,
    /// Why the compiler asks it.
    pub why: String,
    /// Whether it blocks a Ready candidate.
    pub mandatory: bool,
    /// The admissible answers of a choice question.
    pub options: Vec<KeptOption>,
}

impl KeptQuestion {
    fn value(&self) -> Value {
        json!({
            "key": self.key,
            "label": self.label,
            "type": self.kind,
            "why": self.why,
            "mandatory": self.mandatory,
            "options": self.options.iter().map(|o| json!({"key": o.key, "label": o.label})).collect::<Vec<_>>(),
        })
    }

    fn read(value: &Value, at: &str) -> Result<Self, String> {
        let map = object(
            value,
            at,
            &["key", "label", "type", "why", "mandatory", "options"],
        )?;
        let options = array(map, at, "options")?
            .iter()
            .enumerate()
            .map(|(i, option)| {
                let at = format!("{at}.options[{i}]");
                let map = object(option, &at, &["key", "label"])?;
                Ok(KeptOption {
                    key: string(map, &at, "key")?,
                    label: string(map, &at, "label")?,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            key: string(map, at, "key")?,
            label: string(map, at, "label")?,
            kind: string(map, at, "type")?,
            why: string(map, at, "why")?,
            mandatory: field(map, at, "mandatory")?
                .as_bool()
                .ok_or_else(|| format!("`{at}.mandatory` is not a boolean"))?,
            options,
        })
    }
}

/// The compiler's continuation as kept: its value, or only its sha256 and why it was withheld.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeptPlan {
    /// The sha256 of the continuation's exact serialization.
    pub sha256: String,
    /// The continuation, when kept whole.
    pub value: Option<Value>,
    /// Why it was withheld: `over_bound` · `redacted`.
    pub withheld: Option<String>,
}

impl KeptPlan {
    fn value(&self) -> Value {
        json!({"sha256": self.sha256, "value": self.value, "withheld": self.withheld})
    }

    fn read(value: &Value) -> Result<Self, String> {
        let at = "continuation";
        let map = object(value, at, &["sha256", "value", "withheld"])?;
        Ok(Self {
            sha256: string(map, at, "sha256")?,
            value: nullable(field(map, at, "value")?, |v| Ok(v.clone()))?,
            withheld: nullable(field(map, at, "withheld")?, |v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "`continuation.withheld` is not a string".to_owned())
            })?,
        })
    }
}

/// A kept round.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RoundRecord {
    /// [`ROUND_SCHEMA`].
    pub schema: u64,
    /// The request, verbatim as asked.
    pub request: KeptText,
    /// A revision's EDIT.
    pub edit: Option<KeptEdit>,
    /// The settled answers, by key.
    pub answers: Vec<KeptAnswer>,
    /// The open questions, the waiting one first.
    pub questions: Vec<KeptQuestion>,
    /// The compiler's reasons for the open questions.
    pub reasons: Vec<String>,
    /// How many times a clause was restated in words.
    pub restatements: u8,
    /// The compiler's continuation.
    pub continuation: Option<KeptPlan>,
    /// The knowledge record of the call that authored the continuation (evidence).
    pub knowledge: Option<Value>,
    /// The subscription receipt of the call that authored the continuation (evidence).
    pub authoring_receipt: Option<Value>,
    /// The kept proposal a revision round revises.
    pub revises: Option<String>,
}

/// The keys of a schema-1 record, in the order written.
const RECORD_KEYS: [&str; 11] = [
    "schema",
    "request",
    "edit",
    "answers",
    "questions",
    "reasons",
    "restatements",
    "continuation",
    "knowledge",
    "authoring_receipt",
    "revises",
];

/// Why a readable round cannot be continued.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unusable {
    /// A kept executable text differs from its exact original (redacted when kept, or
    /// altered since).
    Redacted(&'static str),
    /// The continuation was withheld (`over_bound` · `redacted`).
    Withheld(String),
    /// A kept value no longer matches the sha256 recorded with it (damaged or edited).
    Altered(&'static str),
    /// More answers than a round may carry.
    TooManyAnswers(usize),
    /// No question waited.
    NoQuestion,
}

impl std::fmt::Display for Unusable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Redacted(field) => write!(
                f,
                "its {field} is not the exact text you gave (redacted when it was kept, or altered since); continuing it would change your request"
            ),
            Self::Withheld(why) => write!(
                f,
                "the compiler's continuation was not kept ({})",
                why.replace('_', " ")
            ),
            Self::Altered(field) => write!(
                f,
                "its {field} no longer matches the digest kept with it (damaged or edited)"
            ),
            Self::TooManyAnswers(n) => {
                write!(
                    f,
                    "it carries {n} answers; a round carries at most {MAX_ANSWERS}"
                )
            }
            Self::NoQuestion => write!(f, "no question waited in it"),
        }
    }
}

impl RoundRecord {
    /// Whether this round can be continued: every executable text exact, the continuation kept
    /// whole (or never settled), at most [`MAX_ANSWERS`] answers, a question waiting.
    ///
    /// # Errors
    /// Why it cannot ([`Unusable`]).
    pub fn continuable(&self) -> Result<(), Unusable> {
        if !self.request.is_exact() {
            return Err(Unusable::Redacted("request"));
        }
        if self.answers.len() > MAX_ANSWERS {
            return Err(Unusable::TooManyAnswers(self.answers.len()));
        }
        if self.answers.iter().any(|a| !a.literal.is_exact()) {
            return Err(Unusable::Redacted("answer"));
        }
        if self.edit.as_ref().is_some_and(|edit| !edit.is_exact()) {
            return Err(Unusable::Redacted("revision"));
        }
        if let Some(plan) = &self.continuation {
            if let Some(why) = &plan.withheld {
                return Err(Unusable::Withheld(why.clone()));
            }
            let kept = plan.value.as_ref().map(Value::to_string);
            if kept.is_none_or(|text| sha256_hex(text.as_bytes()) != plan.sha256) {
                return Err(Unusable::Altered("continuation"));
            }
        }
        if self.questions.is_empty() {
            return Err(Unusable::NoQuestion);
        }
        Ok(())
    }

    /// The question that waited.
    #[must_use]
    pub fn pending(&self) -> Option<&KeptQuestion> {
        self.questions.first()
    }

    /// The settled answers as the compiler takes them (`key → JSON literal`).
    #[must_use]
    pub fn answer_map(&self) -> BTreeMap<String, String> {
        self.answers
            .iter()
            .map(|a| (a.key.clone(), a.literal.text.clone()))
            .collect()
    }

    /// The kept round in words — the request, the settled answers, the question that waited,
    /// the proposal it revises: evidence that names no host's protocol (a host adds its own way
    /// on, as for `compile::meaning`).
    #[must_use]
    pub fn summary(&self) -> String {
        let answers: Vec<String> = self
            .answers
            .iter()
            .map(|a| format!("{} → {}", a.key, literal_words(&a.literal.text)))
            .collect();
        let settled = if answers.is_empty() {
            "no answer settled".to_owned()
        } else {
            format!("settled {}", answers.join(" · "))
        };
        let waiting = self.pending().map_or_else(String::new, |q| {
            format!(" · waiting: « {} » ({})", q.label, q.kind)
        });
        let revises = self
            .revises
            .as_ref()
            .map_or_else(String::new, |id| format!(" · it revises proposal {id}"));
        format!("« {} » · {settled}{waiting}{revises}", self.request.text)
    }

    /// Whether `content` is the exact base this round revises as a proposal's revision.
    #[must_use]
    pub fn revises_base(&self, content: &str) -> bool {
        self.revises.is_some()
            && self
                .edit
                .as_ref()
                .is_some_and(|edit| sha256_hex(content.as_bytes()) == edit.base.sha256)
    }

    /// The subscription receipt of the call that authored the continuation, as the compiler's
    /// type, when one was kept as [`Capture`] writes it. Every key is read with its own type (a
    /// count a whole number, the context a list); only the token counts and the backend may be
    /// `null`, read as absent. A receipt with a key missing or of another type is refused
    /// whole (`None`): nothing is ever read as zero, empty or absent that was not written so.
    #[must_use]
    pub fn authoring_receipt(&self) -> Option<AuthoringReceipt> {
        let kept = self.authoring_receipt.as_ref()?.as_object()?;
        let count = |key: &str| kept.get(key)?.as_u64();
        let nullable = |key: &str| match kept.get(key)? {
            Value::Null => Some(None),
            value => value.as_u64().map(Some),
        };
        let mut receipt = AuthoringReceipt::new(kept.get("model")?.as_str()?);
        receipt.calls = u32::try_from(count("calls")?).ok()?;
        receipt.input_tokens = nullable("input_tokens")?;
        receipt.output_tokens = nullable("output_tokens")?;
        receipt.elapsed_ms = count("elapsed_ms")?;
        receipt.context.clone_from(kept.get("context")?.as_array()?);
        receipt.backend = Some(kept.get("backend")?.clone()).filter(|b| !b.is_null());
        Some(receipt)
    }

    /// The record as the value a host keeps.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let answers: Vec<Value> = self
            .answers
            .iter()
            .map(|a| json!({"key": a.key, "literal": a.literal.value()}))
            .collect();
        let values = [
            json!(self.schema),
            self.request.value(),
            self.edit.as_ref().map_or(Value::Null, KeptEdit::value),
            Value::Array(answers),
            Value::Array(self.questions.iter().map(KeptQuestion::value).collect()),
            json!(self.reasons),
            json!(self.restatements),
            self.continuation
                .as_ref()
                .map_or(Value::Null, KeptPlan::value),
            self.knowledge.clone().unwrap_or(Value::Null),
            self.authoring_receipt.clone().unwrap_or(Value::Null),
            json!(self.revises),
        ];
        let map: Map<String, Value> = RECORD_KEYS
            .iter()
            .map(|k| (*k).to_owned())
            .zip(values)
            .collect();
        Value::Object(map)
    }

    /// A schema-1 record read strictly, or why it is malformed.
    fn read(value: &Value) -> Result<Self, String> {
        let at = "record";
        let map = object(value, at, &RECORD_KEYS)?;
        let answers = array(map, at, "answers")?
            .iter()
            .enumerate()
            .map(|(i, answer)| {
                let at = format!("answers[{i}]");
                let map = object(answer, &at, &["key", "literal"])?;
                Ok(KeptAnswer {
                    key: string(map, &at, "key")?,
                    literal: KeptText::read(field(map, &at, "literal")?, &at)?,
                })
            })
            .collect::<Result<_, String>>()?;
        let questions = array(map, at, "questions")?
            .iter()
            .enumerate()
            .map(|(i, q)| KeptQuestion::read(q, &format!("questions[{i}]")))
            .collect::<Result<_, String>>()?;
        let reasons = array(map, at, "reasons")?
            .iter()
            .map(|r| {
                r.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "`reasons` holds a value that is not a string".to_owned())
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            schema: field(map, at, "schema")?
                .as_u64()
                .ok_or_else(|| "`schema` is not a number".to_owned())?,
            request: KeptText::read(field(map, at, "request")?, "request")?,
            edit: nullable(field(map, at, "edit")?, KeptEdit::read)?,
            answers,
            questions,
            reasons,
            restatements: field(map, at, "restatements")?
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| "`restatements` is not a count".to_owned())?,
            continuation: nullable(field(map, at, "continuation")?, KeptPlan::read)?,
            knowledge: nullable(field(map, at, "knowledge")?, |v| record(v, "knowledge"))?,
            authoring_receipt: nullable(field(map, at, "authoring_receipt")?, |v| {
                record(v, "authoring_receipt")
            })?,
            revises: nullable(field(map, at, "revises")?, |v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "`revises` is not a string".to_owned())
            })?,
        })
    }
}

/// A JSON literal as a person reads it: a string without its quotes, anything else as written.
fn literal_words(literal: &str) -> String {
    serde_json::from_str::<Value>(literal)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| literal.to_owned())
}

/// An object whose every key is known here, or why it is not one.
fn object<'v>(
    value: &'v Value,
    at: &str,
    known: &[&str],
) -> Result<&'v Map<String, Value>, String> {
    let map = value
        .as_object()
        .ok_or_else(|| format!("`{at}` is not an object"))?;
    match map.keys().find(|key| !known.contains(&key.as_str())) {
        Some(unknown) => Err(format!("unknown field `{unknown}` in `{at}`")),
        None => Ok(map),
    }
}

fn field<'v>(map: &'v Map<String, Value>, at: &str, key: &str) -> Result<&'v Value, String> {
    map.get(key)
        .ok_or_else(|| format!("missing field `{key}` in `{at}`"))
}

fn string(map: &Map<String, Value>, at: &str, key: &str) -> Result<String, String> {
    field(map, at, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("`{at}.{key}` is not a string"))
}

fn array<'v>(map: &'v Map<String, Value>, at: &str, key: &str) -> Result<&'v Vec<Value>, String> {
    field(map, at, key)?
        .as_array()
        .ok_or_else(|| format!("`{at}.{key}` is not a list"))
}

fn nullable<T>(
    value: &Value,
    read: impl FnOnce(&Value) -> Result<T, String>,
) -> Result<Option<T>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        read(value).map(Some)
    }
}

/// An evidence record: any object, kept as it was.
fn record(value: &Value, at: &str) -> Result<Value, String> {
    if value.is_object() {
        Ok(value.clone())
    } else {
        Err(format!("`{at}` is not an object"))
    }
}

/// A kept round as this engine reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoundReading {
    /// A round this engine reads.
    Usable {
        /// The round.
        record: Box<RoundRecord>,
        /// The exact value the host kept, re-saved unchanged.
        raw: Value,
    },
    /// A value this engine cannot read: kept unchanged, never used.
    Unreadable {
        /// The exact value the host kept.
        raw: Value,
        /// Why it cannot be read.
        why: String,
    },
}

impl RoundReading {
    /// Read a kept value.
    #[must_use]
    pub fn from_raw(raw: Value) -> Self {
        let why = match raw.get("schema").and_then(Value::as_u64) {
            Some(ROUND_SCHEMA) => match RoundRecord::read(&raw) {
                Ok(record) => {
                    return Self::Usable {
                        record: Box::new(record),
                        raw,
                    };
                }
                Err(error) => format!("a malformed round record ({error})"),
            },
            Some(schema) => format!("round schema {schema}; this engine reads {ROUND_SCHEMA}"),
            None => "a round record without a schema".to_owned(),
        };
        Self::Unreadable { raw, why }
    }

    /// The exact value kept: re-saved unchanged.
    #[must_use]
    pub fn raw(&self) -> &Value {
        match self {
            Self::Usable { raw, .. } | Self::Unreadable { raw, .. } => raw,
        }
    }

    /// The round, or why this engine cannot read it.
    ///
    /// # Errors
    /// Why the kept value cannot be read.
    pub fn record(&self) -> Result<&RoundRecord, &str> {
        match self {
            Self::Usable { record, .. } => Ok(record),
            Self::Unreadable { why, .. } => Err(why),
        }
    }

    /// The round, when this engine reads it and it can be continued.
    #[must_use]
    pub fn continuable(&self) -> Option<&RoundRecord> {
        self.record()
            .ok()
            .filter(|record| record.continuable().is_ok())
    }

    /// The round in the words a host builds its read-only lines from.
    ///
    /// # Errors
    /// Why the kept value cannot be read.
    pub fn words(&self) -> Result<RoundWords, &str> {
        let record = self.record()?;
        Ok(RoundWords {
            summary: record.summary(),
            asked: record.pending().map(|q| q.why.clone()),
            blocked: record.continuable().err().map(|why| why.to_string()),
        })
    }
}

/// A kept round in words, evidence that names no host's protocol (a host adds its own way on).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RoundWords {
    /// [`RoundRecord::summary`].
    pub summary: String,
    /// Why the compiler asked the question that waited, when one waited.
    pub asked: Option<String>,
    /// Why it cannot be continued ([`RoundRecord::continuable`]), `None` when it can.
    pub blocked: Option<String>,
}

/// The typed request a live round is: its EDIT (`(base, change, original)`, the request the
/// base answered) or a CREATE of `intent`, every answer by key, the plan it replays once one
/// settled, and the monetary directives the host's gate admitted — spans carried as data,
/// never read again here.
#[must_use]
pub fn request(
    intent: &str,
    edit: Option<&(String, String, Option<String>)>,
    answers: &BTreeMap<String, String>,
    plan: Option<&Value>,
    money: &[std::ops::Range<usize>],
) -> CompileRequest {
    let mut request = match edit {
        Some((base, change, original)) => original.iter().fold(
            CompileRequest::edit(base.clone(), change.clone()),
            |edit, original| edit.with_original_intent(original.clone()),
        ),
        None => CompileRequest::create(intent.to_owned()),
    };
    for (key, literal) in answers {
        request = request.answer(key.clone(), literal.clone());
    }
    if let Some(plan) = plan {
        request = request.with_plan(plan.clone());
    }
    if !money.is_empty() {
        request = request.with_admitted_money(money.to_vec());
    }
    request
}

/// The admitted spans of an EDIT's `change`, exactly as the EDIT holds it (B15): the money
/// law's own directives of that change when the host's gate admitted money in the line it came
/// from, none otherwise, so an earlier request's spans never ride along.
#[must_use]
pub fn change_money(change: &str, admitted: bool) -> Vec<std::ops::Range<usize>> {
    super::money::directives(change)
        .map(|found| found.found.into_iter().map(|d| d.span).collect())
        .ok()
        .filter(|_| admitted)
        .unwrap_or_default()
}

/// The exact request a Ready outcome was compiled from, as a host keeps it beside the proposal's
/// bytes: the round's own `request` (its answers and the plan it continued, never replaced) with
/// the observation that round was given, the one the compiler recorded in the outcome's plan
/// (`observed_world`) when the host's record names that very observation by its identity
/// (`decision.session.observed`, `world_sha256`; its rows are a summary, never an identity). A
/// round given none keeps none, whatever an earlier round left in its plan; `None` when an
/// attached observation was not recorded, or its record names no identity (an older record):
/// that round's request cannot be rebuilt, and a host keeps no basis rather than read another.
#[must_use]
pub fn compiled(request: CompileRequest, out: &CompileOutcome) -> Option<CompileRequest> {
    let record = out
        .provenance
        .decision
        .as_ref()
        .map(|d| &d["session"]["observed"]);
    let Some(attached) = record.filter(|record| record["attached"] == true) else {
        return Some(request);
    };
    let world = out.provenance.plan.as_ref()?.get("observed_world")?;
    (attached["world_sha256"].as_str()? == crate::knowledge::pin::world_sha256(world))
        .then(|| request.with_knowledge(world.clone()))
}

/// What an outcome settled for a live round: the plan a strategy settled (a plan that still
/// carries unknown work is never replayed), with the knowledge record of the call that
/// authored it when the native door presented a pack, and that call's receipt when a
/// subscription harness made it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Settled {
    /// The compiler's continuation, replayed on every answer round.
    pub plan: Value,
    /// The knowledge record of the call that authored it.
    pub knowledge: Option<Value>,
    /// The subscription receipt of the call that authored it.
    pub receipt: Option<AuthoringReceipt>,
}

/// The continuation `out` settled, when a strategy settled one.
#[must_use]
pub fn settled(out: &CompileOutcome) -> Option<Settled> {
    out.provenance.strategy.as_ref()?;
    Some(Settled {
        plan: out.provenance.plan.clone()?,
        knowledge: crate::knowledge::pin::presented_knowledge(out),
        receipt: out
            .provenance
            .authoring
            .as_ref()
            .filter(|r| {
                r.backend
                    .as_ref()
                    .is_some_and(|b| b["kind"] == "harness_infer")
            })
            .cloned(),
    })
}

/// The questions `out` leaves a live round to ask, in the compiler's order, with the
/// compiler's reasons for them (its unknown and missed diagnostics): the mandatory questions,
/// and a revision's clause dispositions (`gap.N`) too — never a revision's
/// `intent.clarification` (a replacement request would be a fresh CREATE, never its EDIT).
#[must_use]
pub fn open(out: &CompileOutcome, revision: bool) -> (Vec<CompileQuestion>, Vec<String>) {
    let questions = out
        .questions
        .iter()
        .filter(|q| q.mandatory || (revision && q.key.starts_with("gap.")))
        .filter(|q| !(revision && q.key == CLARIFICATION_KEY))
        .cloned()
        .collect();
    let reasons = out
        .diagnostics
        .iter()
        .filter(|d| matches!(d.kind, DiagnosticKind::Unknown | DiagnosticKind::Missed))
        .map(|d| d.message.clone())
        .collect();
    (questions, reasons)
}

/// The plan an answer round's outcome re-anchored to a changed source (its observation or the
/// keys it asked again moved, R4 A6), when neither plan carries an approval: a verified or
/// pending transform stays bound to the plan that authored it.
#[must_use]
pub fn reanchored(recorded: Option<&Value>, out: &CompileOutcome) -> Option<Value> {
    let (recorded, plan) = (recorded?, out.provenance.plan.as_ref()?);
    let moved = ["observed_world", "reasked"]
        .iter()
        .any(|k| recorded.get(*k) != plan.get(*k));
    let approval = ["verified_transform", "pending_transform"]
        .iter()
        .any(|k| recorded.get(*k).is_some() || plan.get(*k).is_some());
    (moved && !approval).then(|| plan.clone())
}

/// A replayed round's outcome names the subscription receipt of the round that authored its
/// plan (`receipt`, marked `carried_from_authoring_round`), unless it names its own.
pub fn carry_receipt(out: &mut CompileOutcome, receipt: Option<&AuthoringReceipt>) {
    if out.provenance.authoring.is_some() {
        return;
    }
    if let Some(mut receipt) = receipt.cloned() {
        if let Some(backend) = receipt.backend.as_mut().and_then(Value::as_object_mut) {
            backend.insert("carried_from_authoring_round".into(), Value::Bool(true));
        }
        out.provenance.authoring = Some(receipt);
    }
}

/// A round being kept: its parts, each passed through the host's redactor.
pub struct Capture<'r> {
    record: RoundRecord,
    redact: &'r dyn Fn(&str) -> String,
}

impl<'r> Capture<'r> {
    /// A round over `request`, verbatim as asked.
    #[must_use]
    pub fn new(request: &str, redact: &'r dyn Fn(&str) -> String) -> Self {
        Self {
            record: RoundRecord {
                schema: ROUND_SCHEMA,
                request: KeptText::of(request, redact),
                edit: None,
                answers: Vec::new(),
                questions: Vec::new(),
                reasons: Vec::new(),
                restatements: 0,
                continuation: None,
                knowledge: None,
                authoring_receipt: None,
                revises: None,
            },
            redact,
        }
    }

    /// A revision's EDIT: the saved workflow's path when it was one, the exact base, the
    /// change, the request the base answered.
    #[must_use]
    pub fn edit(
        mut self,
        path: Option<&str>,
        base: &str,
        change: &str,
        original: Option<&str>,
    ) -> Self {
        let redact = self.redact;
        self.record.edit = Some(KeptEdit {
            path: path.map(|p| KeptText::of(p, redact)),
            base: KeptText::of(base, redact),
            change: KeptText::of(change, redact),
            original: original.map(|o| KeptText::of(o, redact)),
        });
        self
    }

    /// The settled answers, by key.
    #[must_use]
    pub fn answers(mut self, answers: &BTreeMap<String, String>) -> Self {
        let redact = self.redact;
        self.record.answers = answers
            .iter()
            .map(|(key, literal)| KeptAnswer {
                key: key.clone(),
                literal: KeptText::of(literal, redact),
            })
            .collect();
        self
    }

    /// The open questions, the waiting one first, as the compile document shows them.
    #[must_use]
    pub fn questions(mut self, questions: &[CompileQuestion]) -> Self {
        let redact = self.redact;
        self.record.questions = questions
            .iter()
            .map(|q| KeptQuestion {
                key: q.key.clone(),
                label: redact(&q.label),
                kind: match q.answer_type {
                    QuestionType::Text => "text",
                    QuestionType::Literal => "literal",
                    QuestionType::Choice => "choice",
                    _ => "other",
                }
                .to_owned(),
                why: redact(&q.why),
                mandatory: q.mandatory,
                options: q
                    .options
                    .iter()
                    .map(|o| KeptOption {
                        key: redact(&o.key),
                        label: redact(&o.label),
                    })
                    .collect(),
            })
            .collect();
        self
    }

    /// The compiler's reasons for the open questions.
    #[must_use]
    pub fn reasons(mut self, reasons: &[String]) -> Self {
        let redact = self.redact;
        self.record.reasons = reasons.iter().map(|r| redact(r)).collect();
        self
    }

    /// How many times a clause was restated in words.
    #[must_use]
    pub fn restatements(mut self, restatements: u8) -> Self {
        self.record.restatements = restatements;
        self
    }

    /// The compiler's continuation: kept whole only when the redactor leaves it unchanged.
    #[must_use]
    pub fn continuation(mut self, plan: Option<&Value>) -> Self {
        self.record.continuation = plan.map(|plan| {
            let exact = plan.to_string();
            let whole = (self.redact)(&exact) == exact;
            KeptPlan {
                sha256: sha256_hex(exact.as_bytes()),
                value: whole.then(|| plan.clone()),
                withheld: (!whole).then(|| "redacted".to_owned()),
            }
        });
        self
    }

    /// The knowledge record of the call that authored the continuation: evidence, kept only
    /// when it is a record the redactor leaves unchanged.
    #[must_use]
    pub fn knowledge(mut self, record: Option<&Value>) -> Self {
        self.record.knowledge = record
            .filter(|r| r.is_object() && self.unchanged(r))
            .cloned();
        self
    }

    /// The subscription receipt of the call that authored the continuation: evidence, kept
    /// only when the redactor leaves it unchanged.
    #[must_use]
    pub fn authoring_receipt(mut self, receipt: Option<&AuthoringReceipt>) -> Self {
        self.record.authoring_receipt = receipt
            .map(|r| {
                json!({
                    "model": r.model,
                    "calls": r.calls,
                    "input_tokens": r.input_tokens,
                    "output_tokens": r.output_tokens,
                    "elapsed_ms": r.elapsed_ms,
                    "context": r.context,
                    "backend": r.backend,
                })
            })
            .filter(|r| self.unchanged(r));
        self
    }

    /// The kept proposal a revision round revises.
    #[must_use]
    pub fn revises(mut self, proposal: Option<&str>) -> Self {
        self.record.revises = proposal.map(str::to_owned);
        self
    }

    fn unchanged(&self, value: &Value) -> bool {
        let text = value.to_string();
        (self.redact)(&text) == text
    }

    /// The record value, bounded: over [`ROUND_LIMIT`] the continuation is withheld and the
    /// knowledge and receipt dropped; a round still over it is not kept (`None`).
    #[must_use]
    pub fn finish(mut self) -> Option<Value> {
        let within = |value: &Value| value.to_string().len() <= ROUND_LIMIT;
        let value = self.record.to_value();
        if within(&value) {
            return Some(value);
        }
        if let Some(plan) = self.record.continuation.as_mut() {
            plan.value = None;
            plan.withheld = Some("over_bound".to_owned());
        }
        self.record.knowledge = None;
        self.record.authoring_receipt = None;
        Some(self.record.to_value()).filter(within)
    }
}

#[cfg(test)]
mod tests;
