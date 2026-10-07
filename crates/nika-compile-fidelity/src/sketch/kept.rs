// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Records of authored programs kept by a conversation, without count or byte quotas.
//! These are evidence keyed by exact candidate bytes, never authorization. The compiler
//! must reconstruct and judge each
//! record again under the current request, observation and admission before it can revise it.
//! Unknown envelopes stay unchanged. Records changed by redaction are withheld entirely.

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

/// Whether this plan names these exact emitted bytes (not its pre-answer assembly).
/// This identity check is not the compiler's reconstruction or a fidelity judgment.
#[must_use]
pub fn binds(plan: &Value, source: &str) -> bool {
    let hash = sha(source);
    let final_hash = if plan.get("semantic_record").is_some() {
        plan["final"]["candidate_sha256"].as_str()
    } else {
        plan["source_revision"]["candidate_sha256"].as_str()
    };
    final_hash == Some(hash.as_str())
}

/// The effective request bound by a program's record, not a later conversation goal.
#[must_use]
pub fn original(plan: &Value) -> Option<&str> {
    plan["basis"]["read"]["effective"]
        .as_str()
        .or_else(|| plan["source_revision"]["resolved"].as_str())
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

/// The whole plan bound to `source`, or none when missing, redacted or damaged.
/// A host must send this only to EDIT with those exact bytes; it grants no carried knowledge,
/// account, consent or permission. A fresh compiler call revalidates the complete pair.
#[must_use]
pub fn plan(raw: Option<&Value>, place: Place<'_>, source: &str) -> Option<Value> {
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
/// evidence is not evicted to meet a count or byte quota. A redacted plan is not stored; the
/// earlier evidence remains. No record is a live proposal or a permission after reopening.
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
    let text = candidate_plan.to_string();
    if redact(&text) != text {
        return;
    }
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
    let kept = plan(raw.as_ref(), Place::Proposal(proposal), source);
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
