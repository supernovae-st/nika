// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Records of authored programs kept by a conversation, without count or byte quotas.
//! These are evidence keyed by exact candidate bytes, never authorization. The compiler
//! must reconstruct and judge each
//! record again under the current request, observation and admission before it can revise it.
//! Unknown envelopes stay unchanged. A record the redactor changes is kept with each changed
//! text withheld whole and its pointer listed, and is never replayed: it stays evidence of the
//! program, never a continuation. A record with a text that would carry a credential into the
//! next text is not kept: neither text alone shows it.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Which reviewed proposal or saved project file the program answered. Identical bytes at
/// another place do not imply the same request or prohibitions; a proposal is not a Save.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Place<'a> {
    /// A live or retained proposal identity; it grants no permission after reopen.
    Proposal(&'a str),
    /// A project-relative file that an explicit Save actually wrote.
    Saved(&'a str),
}

impl Place<'_> {
    fn value(self) -> Option<Value> {
        match self {
            Self::Proposal(id) if !id.is_empty() && id.len() <= 256 => {
                Some(json!({"proposal": id}))
            }
            Self::Saved(path) if relative(path) => Some(json!({"path": path})),
            _ => None,
        }
    }
}

fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Whether this plan names these exact emitted bytes (not its pre-answer assembly): a semantic
/// record's final bytes, a source revision's revised bytes, or the bytes a created document
/// settled (`document`, written once no mandatory question is left; before, it binds none).
/// This identity check is not the compiler's reconstruction or a fidelity judgment.
#[must_use]
pub fn binds(plan: &Value, source: &str) -> bool {
    let hash = sha(source);
    let final_hash = if plan.get("semantic_record").is_some() {
        plan["final"]["candidate_sha256"].as_str()
    } else if plan.get("source_revision").is_some() {
        plan["source_revision"]["candidate_sha256"].as_str()
    } else {
        plan["document"]["candidate_sha256"].as_str()
    };
    final_hash == Some(hash.as_str())
}

/// The effective request bound by a program's record, not a later conversation goal.
#[must_use]
pub fn original(plan: &Value) -> Option<&str> {
    (plan["basis"]["read"]["effective"].as_str())
        .or_else(|| plan["source_revision"]["resolved"].as_str())
        .or_else(|| plan["document"]["request"].as_str())
}

fn entries(raw: &Value) -> Option<&Vec<Value>> {
    let map = raw.as_object()?;
    if map.len() != 3 || raw["version"] != 1 {
        return None;
    }
    let rows = raw.get("entries")?.as_array()?;
    let last = raw.get("last_saved")?;
    (last.is_null() || last.as_str().is_some_and(relative)).then_some(rows)
}

fn relative(path: &str) -> bool {
    let path = std::path::Path::new(path);
    !path.as_os_str().is_empty()
        && path.components().all(|c| {
            matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

/// The key a plan kept with withheld texts carries (`false`): no round replays it.
const REPLAYABLE: &str = "replayable";

/// The key that lists, as JSON pointers, the texts of a kept plan the redactor changed.
const REDACTIONS: &str = "redactions";

/// What a kept plan holds in place of each text the redactor changed. The whole text is
/// withheld, never kept partly redacted: the lines after a value or a block the redactor
/// opened can still hold the credential.
const WITHHELD: &str = "[redacted]";

/// The whole plan bound to `source`, or none when missing, redacted or damaged.
/// A host must send this only to EDIT with those exact bytes; it grants no carried knowledge,
/// account, consent or permission. A fresh compiler call revalidates the complete pair. A plan
/// kept with redacted texts is evidence only and is never returned here, so no round replays
/// bytes the redactor changed.
#[must_use]
pub fn plan(raw: Option<&Value>, place: Place<'_>, source: &str) -> Option<Value> {
    stored(raw, place, source).filter(|plan| plan[REPLAYABLE].as_bool() != Some(false))
}

/// The plan stored for `place` and these exact bytes, replayable or not.
fn stored(raw: Option<&Value>, place: Place<'_>, source: &str) -> Option<Value> {
    let key = place.value()?;
    let wanted = sha(source);
    entries(raw?)?.iter().rev().find_map(|row| {
        let value = row.get("plan")?;
        (row.as_object()?.len() == 4
            && row["place"] == key
            && row["candidate_sha256"].as_str() == Some(wanted.as_str())
            && row["plan_sha256"].as_str() == Some(sha(&value.to_string()).as_str())
            && binds(value, source))
        .then(|| value.clone())
    })
}

/// Remember a compiler plan beside the exact bytes it emitted, before or after Save. The
/// evidence is not evicted to meet a count or byte quota. The redactor reads each text the plan
/// holds, never its compact encoding: a plan it changes is kept with each of those texts
/// withheld whole, listed and marked never to be replayed ([`plan`]), so a credential is never
/// stored and the record of the program is kept without it. A plan with a text that would
/// carry a credential into the next one (a value opened at its end, a block left open) is not
/// stored: neither text alone shows it. `redact` returns a text unchanged when it finds no
/// credential: any other change counts as a redaction. No record is a live proposal or a
/// permission after reopening.
pub fn remember(
    raw: &mut Option<Value>,
    place: Place<'_>,
    source: &str,
    candidate_plan: Option<&Value>,
    redact: &dyn Fn(&str) -> String,
) {
    let Some(key) = place.value() else {
        return;
    };
    let key_text = key.to_string();
    if redact(&key_text) != key_text {
        return;
    }
    let Some(candidate_plan) = candidate_plan.filter(|p| binds(p, source)) else {
        return;
    };
    let Some(candidate_plan) = redacted(candidate_plan, redact) else {
        return;
    };
    let text = candidate_plan.to_string();
    if raw.as_ref().is_some_and(|raw| entries(raw).is_none()) {
        return;
    }
    let mut next = raw
        .clone()
        .unwrap_or_else(|| json!({"version": 1, "entries": [], "last_saved": null}));
    let Some(rows) = next["entries"].as_array_mut() else {
        return;
    };
    let hash = sha(source);
    rows.retain(|row| {
        row["place"] != key || row["candidate_sha256"].as_str() != Some(hash.as_str())
    });
    rows.push(json!({"place": key, "candidate_sha256": hash, "plan_sha256": sha(&text), "plan": candidate_plan}));
    *raw = Some(next);
}

/// Record the project-relative file actually saved. This is a conversational selection,
/// never proof that the file still exists, has unchanged bytes or may run.
pub fn saved(
    raw: &mut Option<Value>,
    path: &str,
    source: &str,
    proposal: &str,
    redact: &dyn Fn(&str) -> String,
) {
    let Some(key) = Place::Saved(path).value() else {
        return;
    };
    if redact(path) != path || raw.as_ref().is_none_or(|raw| entries(raw).is_none()) {
        return;
    }
    let kept = stored(raw.as_ref(), Place::Proposal(proposal), source);
    let mut next = raw.clone();
    if let Some(rows) = next.as_mut().and_then(|raw| raw["entries"].as_array_mut()) {
        // This Save supersedes that file's earlier meaning even when the new plan is withheld.
        rows.retain(|row| row["place"] != key);
    }
    remember(&mut next, Place::Saved(path), source, kept.as_ref(), redact);
    if let Some(next) = next.as_mut() {
        next["last_saved"] = json!(path);
    }
    *raw = next;
}

/// `plan` with every text it holds (each string value and each object key) read by `redact`.
/// Nothing changed: the plan as it is. Something changed: the plan with each changed text
/// replaced whole by [`WITHHELD`], its pointer listed under `redactions` (built from those
/// names, so the list holds no secret), and `replayable: false`. `None`, so nothing is stored,
/// for a changed plan that is not an object (it cannot carry that mark) and for a plan holding
/// a text that [`runs_on`]: read alone, neither that text nor the next one shows the credential
/// they share, which the plan's single compact line showed.
fn redacted(plan: &Value, redact: &dyn Fn(&str) -> String) -> Option<Value> {
    if any_runs_on(plan, redact) {
        return None;
    }
    let mut kept = plan.clone();
    let mut changed = Vec::new();
    redact_texts(&mut kept, &mut String::new(), redact, &mut changed);
    if changed.is_empty() {
        return Some(kept);
    }
    let object = kept.as_object_mut()?;
    object.insert(REDACTIONS.to_owned(), json!(changed));
    object.insert(REPLAYABLE.to_owned(), json!(false));
    Some(kept)
}

/// Withhold in place every text under `value` that `redact` changes, pushing its pointer. A key
/// it changes is renamed [`WITHHELD`] (`#2`, `#3` … when that name is taken).
fn redact_texts(
    value: &mut Value,
    at: &mut String,
    redact: &dyn Fn(&str) -> String,
    changed: &mut Vec<String>,
) {
    match value {
        Value::String(text) => {
            if redact(text) != *text {
                changed.push(at.clone());
                WITHHELD.clone_into(text);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                let length = at.len();
                at.push('/');
                at.push_str(&index.to_string());
                redact_texts(item, at, redact, changed);
                at.truncate(length);
            }
        }
        Value::Object(map) => {
            for (key, mut item) in std::mem::take(map) {
                let renamed = redact(&key) != key;
                let named = if renamed { WITHHELD } else { key.as_str() };
                let mut name = named.to_owned();
                let mut copy = 1;
                while map.contains_key(&name) {
                    copy += 1;
                    name = format!("{named}#{copy}");
                }
                let length = at.len();
                at.push('/');
                at.push_str(&name.replace('~', "~0").replace('/', "~1"));
                if renamed {
                    changed.push(at.clone());
                }
                redact_texts(&mut item, at, redact, changed);
                at.truncate(length);
                map.insert(name, item);
            }
        }
        _ => {}
    }
}

/// Whether a text of `value`, a string value or an object key, [`runs_on`].
fn any_runs_on(value: &Value, redact: &dyn Fn(&str) -> String) -> bool {
    match value {
        Value::String(text) => runs_on(text, redact),
        Value::Array(items) => items.iter().any(|item| any_runs_on(item, redact)),
        Value::Object(map) => map
            .iter()
            .any(|(key, item)| runs_on(key, redact) || any_runs_on(item, redact)),
        _ => false,
    }
}

/// Whether `redact` would hide something written after `text` that `text` does not hold: the
/// line after it (a private-key block left open), a letter right after it that does not just
/// join a span of `text` already hidden (a value opened at its end and left empty, `token=`),
/// or the lines of a YAML block value whose header ends `text` and which `redact` hides
/// (`token: |`, `secret: >-`). A credential split that way between two texts of a plan is in
/// neither alone.
fn runs_on(text: &str, redact: &dyn Fn(&str) -> String) -> bool {
    let alone = redact(text);
    let same_line = redact(&format!("{text}x"));
    let header = text.lines().next_back().filter(|last| opens_block(last));
    !redact(&format!("{text}\nx")).ends_with("\nx")
        || (same_line != alone && same_line.strip_suffix('x') != Some(alone.as_str()))
        || header.is_some_and(|last| redact(last) != last)
}

/// Whether `line` ends with a YAML block scalar header (`key: |`, `key: >-`, `key: |2 # note`):
/// the value is the lines that follow it, whatever text holds them.
fn opens_block(line: &str) -> bool {
    let Some((_, header)) = line.rsplit_once(": ") else {
        return false;
    };
    let header = header
        .split_once(" #")
        .map_or(header, |(head, _)| head)
        .trim();
    let mut indicators = header.chars();
    matches!(indicators.next(), Some('|' | '>'))
        && indicators.all(|c| matches!(c, '-' | '+' | '1'..='9'))
}

/// The previous Save's conversational selection, read without executing or approving it.
#[must_use]
pub fn last_saved(raw: Option<&Value>) -> Option<&str> {
    let raw = raw?;
    entries(raw)?;
    raw["last_saved"].as_str().filter(|path| relative(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(source: &str, meaning: &str) -> Value {
        json!({"semantic_record": 1, "basis": {"read": {"effective": meaning}},
            "final": {"candidate_sha256": sha(source)}})
    }

    /// A created document binds the bytes it settled and states the request they answer; before
    /// it settles its record binds nothing, and it never binds other bytes.
    #[test]
    fn a_settled_created_document_binds_its_bytes_and_states_its_request() {
        let source = "nika: created\ntasks: {}\n";
        let mut native = json!({"strategy": "native", "intent_sha256": "i", "source": source,
            "document_create": {"mode": "written", "request": "Count the pages"}});
        assert!(!binds(&native, source), "nothing settled yet");
        native["document"] = json!({"version": 1, "candidate_sha256": sha(source),
            "request": "Count the pages", "base_sha256": null, "mode": "written",
            "components": []});
        assert!(binds(&native, source));
        assert!(!binds(&native, "nika: other\n"));
        assert_eq!(original(&native), Some("Count the pages"));
    }

    #[test]
    fn records_are_bound_to_place_and_bytes_and_a_proposal_does_not_replace_a_save() {
        let mut raw = None;
        let source = "same program bytes";
        let first = record(source, "Never delete a file");
        let second = record(source, "Never send an email");
        remember(
            &mut raw,
            Place::Proposal("first"),
            source,
            Some(&first),
            &str::to_owned,
        );
        saved(&mut raw, "a.nika", source, "first", &str::to_owned);
        remember(
            &mut raw,
            Place::Proposal("second"),
            source,
            Some(&second),
            &str::to_owned,
        );
        assert_eq!(
            plan(raw.as_ref(), Place::Saved("a.nika"), source),
            Some(first.clone())
        );
        assert!(plan(raw.as_ref(), Place::Saved("b.nika"), source).is_none());
        saved(&mut raw, "b.nika", source, "second", &str::to_owned);
        assert_eq!(
            plan(raw.as_ref(), Place::Saved("a.nika"), source),
            Some(first)
        );
        assert_eq!(
            plan(raw.as_ref(), Place::Saved("b.nika"), source),
            Some(second)
        );
        assert!(plan(raw.as_ref(), Place::Saved("b.nika"), "different bytes").is_none());
        saved(&mut raw, "a.nika", source, "withheld", &str::to_owned);
        assert!(plan(raw.as_ref(), Place::Saved("a.nika"), source).is_none());
    }

    #[test]
    fn records_are_not_evicted_and_integrity_redaction_and_version_checks_remain() {
        let mut raw = None;
        for n in 0..20 {
            let source = format!("program {n}");
            remember(
                &mut raw,
                Place::Proposal("p"),
                &source,
                Some(&record(&source, "work")),
                &str::to_owned,
            );
        }
        for n in 0..20 {
            let source = format!("program {n}");
            assert_eq!(
                plan(raw.as_ref(), Place::Proposal("p"), &source),
                Some(record(&source, "work"))
            );
        }
        assert!(plan(raw.as_ref(), Place::Proposal("p"), "program 19").is_some());
        let before = raw.clone();
        remember(
            &mut raw,
            Place::Proposal("p"),
            "secret",
            Some(&record("secret", "work")),
            &|_| "redacted".into(),
        );
        assert_eq!(raw, before);
        let mut damaged = raw.clone();
        if let Some(raw) = damaged.as_mut() {
            raw["entries"][19]["plan"]["extra"] = json!(true);
        }
        assert!(plan(damaged.as_ref(), Place::Proposal("p"), "program 19").is_none());
        let future = json!({"version": 2, "entries": [], "last_saved": "workflow.nika"});
        raw = Some(future.clone());
        remember(
            &mut raw,
            Place::Proposal("p"),
            "program",
            Some(&record("program", "work")),
            &str::to_owned,
        );
        saved(&mut raw, "new.nika", "program", "p", &str::to_owned);
        assert_eq!(raw, Some(future));
        assert!(last_saved(raw.as_ref()).is_none());
        raw = before;
        for invalid in ["../outside.nika", "/outside.nika", ""] {
            saved(&mut raw, invalid, "program 19", "p", &str::to_owned);
        }
        assert!(last_saved(raw.as_ref()).is_none());
    }

    #[test]
    fn historical_world_is_kept_whole_beyond_the_former_plan_and_envelope_quotas() {
        let source = "exact program";
        let mut raw = None;
        let mut small = record(source, "read input and write output");
        small["basis"]["world"] = crate::observed::basis::keep(Some(&json!({
            "observed": [{"path": "./in.json", "state": "observed", "columns": ["amount"]}],
        })));
        remember(
            &mut raw,
            Place::Proposal("p"),
            source,
            Some(&small),
            &str::to_owned,
        );
        saved(&mut raw, "a.nika", source, "p", &str::to_owned);
        let text = raw.as_ref().map(Value::to_string).unwrap_or_default();
        let reopened = serde_json::from_str(&text).ok();
        assert_eq!(
            plan(reopened.as_ref(), Place::Saved("a.nika"), source),
            Some(small.clone())
        );
        let mut large = small.clone();
        large["basis"]["world"] = crate::observed::basis::keep(Some(&json!({
            "observed": [{"path": "./in.json", "value": "x".repeat(512 * 1024)}],
        })));
        assert!(large.to_string().len() > 256 * 1024);
        remember(
            &mut raw,
            Place::Proposal("large"),
            source,
            Some(&large),
            &str::to_owned,
        );
        saved(&mut raw, "large.nika", source, "large", &str::to_owned);
        let encoded = raw.as_ref().map(Value::to_string).unwrap_or_default();
        assert!(encoded.len() > 256 * 1024);
        let reopened = serde_json::from_str(&encoded).ok();
        assert_eq!(last_saved(reopened.as_ref()), Some("large.nika"));
        assert_eq!(
            plan(reopened.as_ref(), Place::Proposal("large"), source),
            Some(large.clone())
        );
        assert_eq!(
            plan(reopened.as_ref(), Place::Saved("large.nika"), source),
            Some(large)
        );
        assert_eq!(
            plan(reopened.as_ref(), Place::Saved("a.nika"), source),
            Some(small),
            "storing the large record leaves the earlier saved evidence intact"
        );
    }

    /// A plan the redactor changes is kept as evidence: the credential never stored, every
    /// identity it holds kept whole, never returned to replay, and the Save that follows keeps it
    /// for that file. A key the redactor changes is listed by its redacted name only.
    #[test]
    fn a_redacted_plan_is_kept_as_evidence_and_never_replayed() {
        let source = "nika: pull\ntasks: {}\n";
        let redact = |text: &str| text.replace("SECRET", "[redacted]");
        let mut native = record(source, "Pull the feed");
        native["source"] = json!("url: https://x.test/feed?token=SECRET");
        native["document"] = json!({"components": [{"component": {"id": "block:filter"}}]});
        let mut raw = None;
        remember(
            &mut raw,
            Place::Proposal("p"),
            source,
            Some(&native),
            &redact,
        );
        let kept = raw
            .as_ref()
            .map_or(Value::Null, |raw| raw["entries"][0]["plan"].clone());
        assert_eq!(kept["source"], json!("[redacted]"), "withheld whole");
        assert_eq!(kept["redactions"], json!(["/source"]));
        assert_eq!(kept["replayable"], json!(false));
        assert_eq!(
            kept["document"], native["document"],
            "identities are kept whole"
        );
        assert_eq!(
            kept["final"], native["final"],
            "the record still binds its bytes"
        );
        let text = raw.as_ref().map(Value::to_string).unwrap_or_default();
        assert!(!text.contains("SECRET"), "{text}");
        assert!(plan(raw.as_ref(), Place::Proposal("p"), source).is_none());
        saved(&mut raw, "pull.nika", source, "p", &redact);
        assert_eq!(last_saved(raw.as_ref()), Some("pull.nika"));
        assert!(plan(raw.as_ref(), Place::Saved("pull.nika"), source).is_none());
        assert_eq!(
            stored(raw.as_ref(), Place::Saved("pull.nika"), source),
            Some(kept),
            "the Save keeps the record of the file"
        );
        let mut keyed = record(source, "Pull the feed");
        keyed["answers"] = json!({"SECRET": "an answer", "plain": "kept"});
        let mut raw = None;
        remember(
            &mut raw,
            Place::Proposal("k"),
            source,
            Some(&keyed),
            &redact,
        );
        let text = raw.as_ref().map(Value::to_string).unwrap_or_default();
        assert!(!text.contains("SECRET"), "{text}");
        assert!(text.contains("\"/answers/[redacted]\""), "{text}");
        assert!(text.contains("\"plain\":\"kept\""), "{text}");
    }

    /// No text of a kept record holds the credential: not a value or a key of a nested object
    /// or array, not a pointer it lists; each changed text is withheld whole. A plan that is not
    /// an object is not stored.
    #[test]
    fn no_text_of_a_kept_record_holds_the_secret() {
        let source = "nika: pull\ntasks: {}\n";
        let redact = |text: &str| text.replace("SECRET", "[redacted]");
        let mut nested = record(source, "Pull the SECRET feed");
        nested["document"] = json!({"components": [{"component": {"id": "block:filter"},
            "args": {"SECRET": ["SECRET", {"key SECRET": "SECRET"}], "list": [["a SECRET"], 7]}}]});
        let mut raw = None;
        remember(
            &mut raw,
            Place::Proposal("p"),
            source,
            Some(&nested),
            &redact,
        );
        let text = raw.as_ref().map(Value::to_string).unwrap_or_default();
        assert!(!text.contains("SECRET"), "{text}");
        let kept = raw
            .as_ref()
            .map_or(Value::Null, |raw| raw["entries"][0]["plan"].clone());
        assert_eq!(kept["replayable"], json!(false), "{text}");
        assert_eq!(kept["basis"]["read"]["effective"], json!("[redacted]"));
        let mut listed: Vec<String> =
            serde_json::from_value(kept["redactions"].clone()).unwrap_or_default();
        listed.sort();
        listed.dedup();
        let args = "/document/components/0/args";
        assert_eq!(
            listed,
            [
                "/basis/read/effective".to_owned(),
                format!("{args}/[redacted]"),
                format!("{args}/[redacted]/0"),
                format!("{args}/[redacted]/1/[redacted]"),
                format!("{args}/list/0/0"),
            ],
            "{text}"
        );
        for plan in [json!(["SECRET"]), json!("SECRET")] {
            let mut raw = None;
            remember(&mut raw, Place::Proposal("q"), source, Some(&plan), &redact);
            assert_eq!(raw, None, "{plan}");
        }
    }

    /// A credential that one text opens and the next one holds is in neither text alone, as it
    /// was in the plan's single compact line: such a plan is not stored. A value hidden inside
    /// its own text keeps the plan.
    #[test]
    fn a_credential_shared_by_two_texts_is_not_stored() {
        let source = "nika: pull\ntasks: {}\n";
        // Hides a `pin=` value to the end of its line and all that follows `BEGIN`, as the
        // Session's redactor does with `token=` values and private-key blocks.
        let redact = |text: &str| {
            if let Some((head, _)) = text.split_once("BEGIN") {
                return format!("{head}[block]");
            }
            text.split('\n')
                .map(|line| match line.split_once("pin=") {
                    Some((head, value)) if !value.is_empty() => format!("{head}pin=[hidden]"),
                    _ => line.to_owned(),
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        for shared in [
            json!(["pin=", "1234"]),
            json!({"pin=": "1234"}),
            json!(["BEGIN", "1234"]),
        ] {
            let mut plan = record(source, "Pull the feed");
            plan["document"] = json!({ "observed": shared });
            let mut raw = None;
            remember(&mut raw, Place::Proposal("p"), source, Some(&plan), &redact);
            assert_eq!(raw, None, "{shared}");
        }
        let mut plan = record(source, "Pull the feed");
        plan["document"] = json!({"observed": ["pin=1234", "then"]});
        let mut raw = None;
        remember(&mut raw, Place::Proposal("p"), source, Some(&plan), &redact);
        let kept = raw
            .as_ref()
            .map_or(Value::Null, |raw| raw["entries"][0]["plan"].clone());
        assert_eq!(kept["document"]["observed"], json!(["[redacted]", "then"]));
    }

    #[test]
    fn every_saved_program_and_its_proposal_remain_readable_after_reopen() {
        let mut raw = None;
        for n in 0..24 {
            let source = format!("exact program {n}");
            let proposal = format!("proposal-{n}");
            remember(
                &mut raw,
                Place::Proposal(&proposal),
                &source,
                Some(&record(&source, &format!("request {n}"))),
                &str::to_owned,
            );
            saved(
                &mut raw,
                &format!("workflow-{n}.nika"),
                &source,
                &proposal,
                &str::to_owned,
            );
        }
        let encoded = raw.as_ref().map(Value::to_string).unwrap_or_default();
        let reopened: Option<Value> = serde_json::from_str(&encoded).ok();
        assert_eq!(last_saved(reopened.as_ref()), Some("workflow-23.nika"));
        assert_eq!(
            reopened.as_ref().map(|raw| &raw["version"]),
            Some(&json!(1))
        );
        for n in 0..24 {
            let source = format!("exact program {n}");
            let proposal = format!("proposal-{n}");
            let path = format!("workflow-{n}.nika");
            let expected = record(&source, &format!("request {n}"));
            for place in [Place::Proposal(&proposal), Place::Saved(&path)] {
                assert_eq!(
                    plan(reopened.as_ref(), place, &source),
                    Some(expected.clone())
                );
            }
        }
    }
}
