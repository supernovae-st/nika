// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source revision evidence: pure request projection, exact byte binding and bounded questions.
//! The core lends its literal parser and obligation-ledger projection, then owns outcomes,
//! question types, findings and judgment. This owner restores no authority and performs no I/O.

use super::{Decided, Edit, Parse};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The version of source revision and question records; the wire format is unchanged.
const VERSION: u64 = 1;

/// Only the request facts the pure revision law reads; no compile outcome or authority.
#[derive(Debug)]
#[non_exhaustive]
pub struct Request<'a> {
    /// Base bytes for an EDIT; absent for CREATE.
    pub source: Option<&'a str>,
    /// A textual change; absent for another edit operation.
    pub change: Option<&'a str>,
    /// The stated request the base answers, if no revision record supersedes it.
    pub original: Option<&'a str>,
    /// Prior evidence, never permission.
    pub plan: Option<&'a Value>,
    /// The caller's exact answer literals.
    pub answers: &'a BTreeMap<String, String>,
    /// Effective intent identity, computed by the compile facade's existing law.
    pub intent_sha256: String,
}

impl<'a> Request<'a> {
    /// Project the core request without carrying its outcome or authority.
    #[must_use]
    pub fn new(
        source: Option<&'a str>,
        change: Option<&'a str>,
        original: Option<&'a str>,
        plan: Option<&'a Value>,
        answers: &'a BTreeMap<String, String>,
        intent_sha256: String,
    ) -> Self {
        Self {
            source,
            change,
            original,
            plan,
            answers,
            intent_sha256,
        }
    }
}

/// Evidence of an applied edit, or the same pending record and its question descriptions.
#[derive(Debug)]
#[non_exhaustive]
pub enum Step {
    /// Applied source revision record; still subject to core Check and judgment.
    Done(Value),
    /// Pending record and pure question descriptions, converted to core types by the facade.
    Ask(Value, Vec<Value>),
}

fn sha(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

/// Revise the exact base through the existing typed destination law.
///
/// # Errors
/// Why the original record, change, destination or byte substitution does not bind.
pub fn revise(
    request: &Request<'_>,
    stated: &Value,
    ledger: &dyn Fn(&str) -> Value,
    parse: Parse<'_>,
) -> Result<Step, String> {
    original_of(request).and_then(|original| revised(request, stated, &original, ledger, parse))
}

/// The revision of `request`, answering `original`, under the seat's typed `stated` links.
fn revised(
    request: &Request<'_>,
    stated: &Value,
    original: &str,
    ledger: &dyn Fn(&str) -> Value,
    parse: Parse<'_>,
) -> Result<Step, String> {
    let (source, change) = request
        .source
        .zip(request.change)
        .ok_or("only a change in words is revised")?;
    let (original_ledger, change_ledger) = (ledger(original), ledger(change));
    let document = parse(source).ok_or("the base does not parse as a workflow")?;
    let answer = |key: &str| {
        let literal = request.answers.get(key)?;
        serde_json::from_str::<String>(literal)
            .ok()
            .or_else(|| Some(literal.clone()))
    };
    let (decided, adds) = super::decide(
        &document,
        original,
        (&original_ledger, &change_ledger),
        stated,
        &answer,
    )?;
    let base_sha = sha(source);
    let edit = match decided {
        Decided::Edit(edit) => edit,
        Decided::Ask(open) => {
            let questions = open
                .iter()
                .map(|(key, options)| ask(key, options))
                .collect();
            let pending = json!({"strategy": "native",
                "intent_sha256": request.intent_sha256,
                "source_question": {"version": VERSION, "base_sha256": base_sha,
                    "original": original, "stated": stated,
                    "open": open.iter().map(|(k, o)| json!({"key": k, "options": o})).collect::<Vec<_>>()}});
            return Ok(Step::Ask(pending, questions));
        }
    };
    let done = super::apply(source, &edit, parse)?;
    let links = stated["supersedes"].clone();
    let resolved = super::resolved(original, &links, &adds, &edit).map_err(|w| w.join("; "))?;
    let (kind, old, new, superseded) = match &edit {
        Edit::Replace { old, new } => ("replace", old, new, json!([{"path": old, "by": new}])),
        Edit::Add { like, new } => ("add", like, new, json!([])),
    };
    Ok(Step::Done(json!({
        "strategy": "native",
        "intent_sha256": request.intent_sha256,
        "source": done.source,
        "questions": [], "gaps": [], "trigger": null,
        "superseded": superseded,
        "source_revision": {
            "version": VERSION, "edit": kind,
            "base_sha256": base_sha,
            "candidate_sha256": sha(&done.source),
            "path": old, "by": new, "slots": done.slots,
            "original": original, "change": change, "resolved": resolved,
            "supersedes": links, "adds": stated["adds"].clone(), "like": stated["like"].clone(),
        },
    })))
}

/// The bounded choice a revision asks: which of `options` (exact paths) its `key` takes.
fn ask(key: &str, options: &[String]) -> Value {
    let label = match key {
        super::ASK_DESTINATION => "Which destination does the change replace?",
        super::ASK_LIKE => "Which existing destination does the added one copy?",
        _ if options.is_empty() => "Which path should the new destination be?",
        _ => "Which path does the change name as the destination?",
    };
    let (answer_type, why) = if options.is_empty() {
        let why = "The change replaces a destination but states no new path; the compiler never invents one. Answer the new path, relative to the project.";
        ("text", why.to_owned())
    } else {
        let why = "The change and the request leave it open between the paths the workflow states; the compiler never guesses.".to_owned();
        ("choice", why)
    };
    json!({"key": key, "label": label, "answer_type": answer_type, "why": why,
        "options": options.iter().map(|o| json!({"key": o, "label": format!("`{o}`")})).collect::<Vec<_>>()})
}

/// The words the base answers: the resolved words its source revision record states (the record
/// must bind the base: its very bytes), else the request's own.
fn original_of(request: &Request<'_>) -> Result<String, String> {
    let revision = request.plan.map_or(&Value::Null, |r| &r["source_revision"]);
    if revision.is_null() {
        return request
            .original
            .map(str::to_owned)
            .ok_or_else(|| "the request this base answers is not stated".to_owned());
    }
    let Some(source) = request.source else {
        return Err("a source revision binds an edit only".to_owned());
    };
    if revision["candidate_sha256"].as_str() != Some(sha(source).as_str()) {
        return Err("the base is not the workflow its revision record wrote (stale or altered): the record does not bind it".to_owned());
    }
    Ok(revision["resolved"].as_str().unwrap_or_default().to_owned())
}

/// Decide a recorded question again only on the base and intent it originally named.
///
/// # Errors
/// A stale question record or an unprovable answer keeps the base unchanged.
pub fn answered(
    intent_sha256: &str,
    record: &Value,
    request: &Request<'_>,
    ledger: &dyn Fn(&str) -> Value,
    parse: Parse<'_>,
) -> Result<Step, String> {
    let question = &record["source_question"];
    let bound = request.source.is_some_and(|source| {
        question["base_sha256"].as_str() == Some(sha(source).as_str())
            && record["intent_sha256"].as_str() == Some(intent_sha256)
    });
    if !bound {
        return Err(
            "the base or the request is not the one this question was asked on (stale or altered)"
                .to_owned(),
        );
    }
    revised(
        request,
        &question["stated"],
        question["original"].as_str().unwrap_or_default(),
        ledger,
        parse,
    )
}

/// Explain only the exact changed slots the applied record names.
#[must_use]
pub fn applied(record: &Value) -> String {
    let revision = &record["source_revision"];
    let (old, new) = (
        revision["path"].as_str().unwrap_or_default(),
        revision["by"].as_str().unwrap_or_default(),
    );
    if revision["edit"] == "add" {
        format!(
            "`{new}` is added beside `{old}`, both kept: only the copied write and its permit are new, every other field of the base is proven unchanged."
        )
    } else {
        format!(
            "`{old}` is replaced by `{new}`: only its parsed slots changed, every other field of the base is proven unchanged."
        )
    }
}

/// Whether an answer round's `request` replays `record` on the base it revised: a record with no
/// `source_revision` binds as before; one with it replays only when the request's base is the
/// revision's base and its edit, made again, writes the recorded bytes.
///
/// # Errors
/// Why the record does not bind this base.
pub fn rebound(record: &Value, source: Option<&str>, parse: Parse<'_>) -> Result<(), String> {
    let revision = &record["source_revision"];
    if revision.is_null() {
        return Ok(());
    }
    let Some(source) = source else {
        return Err("a source revision replays on an edit only".to_owned());
    };
    if revision["base_sha256"].as_str() != Some(sha(source).as_str()) {
        return Err(
            "the base is not the one this revision was made from (stale or altered)".to_owned(),
        );
    }
    let (Some(path), Some(by)) = (revision["path"].as_str(), revision["by"].as_str()) else {
        return Err("the revision record names no destination".to_owned());
    };
    let edit = if revision["edit"] == "add" {
        Edit::Add {
            like: path.to_owned(),
            new: by.to_owned(),
        }
    } else {
        Edit::Replace {
            old: path.to_owned(),
            new: by.to_owned(),
        }
    };
    let again = super::apply(source, &edit, parse)?;
    let written = record["source"].as_str().unwrap_or_default();
    if again.source != written
        || revision["candidate_sha256"].as_str() != Some(sha(written).as_str())
    {
        return Err("the recorded candidate is not the edit of its base (altered)".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_question_refuses_another_base_or_intent_before_any_projection() {
        let answers = BTreeMap::new();
        let request = Request {
            source: Some("base"),
            change: Some("change"),
            original: Some("original"),
            plan: None,
            answers: &answers,
            intent_sha256: "current-intent".to_owned(),
        };
        let question = json!({"intent_sha256": "original-intent",
            "source_question": {"base_sha256": sha("base")}});
        let projections = std::cell::Cell::new(0);
        let never_ledger = |_: &str| {
            projections.set(projections.get() + 1);
            Value::Null
        };
        let never_parse = |_: &str| {
            projections.set(projections.get() + 1);
            None
        };
        assert!(
            answered(
                "current-intent",
                &question,
                &request,
                &never_ledger,
                &never_parse
            )
            .is_err()
        );
        let stale = Request {
            source: Some("altered base"),
            ..request
        };
        assert!(
            answered(
                "original-intent",
                &question,
                &stale,
                &never_ledger,
                &never_parse
            )
            .is_err()
        );
        assert_eq!(projections.get(), 0, "stale evidence never projects");
    }

    #[test]
    fn a_record_never_binds_create_or_changed_base_bytes() {
        let record = json!({"source_revision": {"base_sha256": sha("base")}});
        let parses = std::cell::Cell::new(0);
        let never_parse = |_: &str| {
            parses.set(parses.get() + 1);
            None
        };
        assert!(rebound(&record, None, &never_parse).is_err());
        assert!(rebound(&record, Some("base\n"), &never_parse).is_err());
        assert!(rebound(&json!({}), None, &never_parse).is_ok());
        assert_eq!(parses.get(), 0, "unbound evidence never parses");
        assert_eq!(
            sha("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
