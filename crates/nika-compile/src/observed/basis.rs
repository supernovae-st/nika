// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The source basis of a candidate (C9 · F4): the source facts its decision recorded
//! (`decision.grounding` — every key a rule reads — and `decision.numbers` — a number policy
//! chosen from observed kinds alone), judged again against a fresh host observation of the same
//! sources by the grounding law that admitted them. A proposal is consented to as the program
//! those facts justified: a key renamed or removed from a declared header, a source no longer
//! observed, a key some sampled records now lack, or a number field whose sampled values are no
//! longer all numbers moves its basis. New, removed or reordered rows, a column order and a peek
//! hash never do: that is the data the program reads at run, not a fact its lowering relied on.
//!
//! Every recorded dependency is judged, whatever its labels say (a recorded grade or an
//! `admissible` flag is never taken as evidence), each against the one fresh row of its exact
//! source. A dependency the fresh observation does not cover, or a record this law cannot read,
//! is unjudged, never assumed to hold: a decision that is not an object, a record present but not
//! a list, an entry that is not an object, a field the law reads of another type, or a kind count
//! that is not a count. A record that is absent is a legacy decision's, and no dependency. The
//! canonical spellings a text equality matched (`decision.spellings`) stay the bounded sample law
//! they are, not a basis.

use super::grounding::{self, Grade};
use serde_json::Value;

/// The raw kinds of a sampled value the number law reads as a number (`crate::observation`).
const NUMBER_KINDS: [&str; 2] = ["number", "number_text"];

/// What a fresh observation says of a candidate's recorded source basis.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Basis {
    /// The decision records no source dependency: nothing to judge (never a proof that the
    /// candidate reads nothing).
    None,
    /// Every recorded dependency holds on the fresh observation: how many were judged.
    Holds(usize),
    /// The dependencies that no longer hold, each in words: the source, the key, then and now.
    Moved(Vec<String>),
    /// The dependencies the fresh observation does not cover (no single row for their exact
    /// source) or whose record this law cannot read: neither holding nor moved.
    Unjudged(Vec<String>),
}

impl Basis {
    /// The sources a decision's basis names, each once, in record order: what a host observes
    /// again before it judges the basis.
    #[must_use]
    pub fn sources(decision: Option<&Value>) -> Vec<String> {
        let mut sources: Vec<String> = Vec::new();
        for entry in keys(decision).chain(policies(decision)) {
            if let Some(source) = entry["source"].as_str()
                && !sources.iter().any(|s| s == source)
            {
                sources.push(source.to_owned());
            }
        }
        sources
    }
}

/// One dependency, judged.
enum Verdict {
    Holds,
    Moved(String),
    Unjudged(String),
}

/// Judge `decision`'s recorded source basis against `fresh`, a host observation of its sources
/// (`{"observed": [rows], "kinds": {path: kinds}}`, the rows and kinds [`crate::observation`]
/// describes); `intent` is the request whose own column list may assert a key.
#[must_use]
pub fn basis(decision: Option<&Value>, fresh: Option<&Value>, intent: &str) -> Basis {
    judge(decision, fresh, &stated_keys(intent))
}

/// Judge against the actual effective request that produced the proposal, including its
/// question-bound answers. A fresh zero-call compile recovers answered assertions using the
/// same grounding and replay laws; incoming decision labels never supply an assertion. The
/// request keeps the observation of its compile round, while `fresh` is the use-time observation.
/// A host must retain this request beside the exact candidate bytes, after any whole replacement.
#[must_use]
pub fn basis_for(
    request: &crate::CompileRequest,
    decision: Option<&Value>,
    fresh: Option<&Value>,
) -> Basis {
    judge(decision, fresh, &assertions(request))
}

/// A request's column list asserts keys for its one named source, never another file.
fn stated_keys(intent: &str) -> Vec<(String, String)> {
    let sources = crate::stated_sources(intent);
    let [source] = sources.as_slice() else {
        return Vec::new();
    };
    crate::columns::columns_hint(intent)
        .into_iter()
        .map(|key| (source.clone(), key))
        .collect()
}

/// Derive evidence afresh, rather than treating a recorded `user_asserted` grade as evidence.
fn assertions(request: &crate::CompileRequest) -> Vec<(String, String)> {
    let Some(request) = effective_request(request) else {
        return Vec::new();
    };
    let words = match &request.input {
        crate::types::Input::Create(intent) => intent.clone(),
        crate::types::Input::Edit { .. } => crate::revise_intent(&request).unwrap_or_default(),
    };
    let mut supported = stated_keys(&words);
    if request
        .answers
        .keys()
        .any(|key| key.starts_with("const.rule_field_"))
        && let Ok(out) = crate::compile(&request)
    {
        for entry in keys(out.provenance.decision.as_ref()) {
            if entry["grade"] == Grade::UserAsserted.word()
                && entry["bound_by"] == "answer"
                && let (Some(source), Some(key)) =
                    (entry["source"].as_str(), entry["field"].as_str())
            {
                let pair = (source.to_owned(), key.to_owned());
                if !supported.contains(&pair) {
                    supported.push(pair);
                }
            }
        }
    }
    supported
}

/// Fold the complete replacement as the cognition door does. The host discards the previous
/// round's answers and continuation before capturing a replacement request; a retained plan
/// still has to pass the compiler's own anchoring and stale-observation laws on replay.
fn effective_request(request: &crate::CompileRequest) -> Option<crate::CompileRequest> {
    let mut effective = request.clone();
    if matches!(request.input, crate::types::Input::Create(_))
        && let Some(raw) = effective.answers.remove("intent.clarification")
    {
        let value =
            crate::literal_answer(Some(&raw), "intent.clarification", &mut crate::initial())?;
        let text = value.as_str().filter(|text| !text.trim().is_empty())?;
        effective = effective.with_replaced_input(crate::lexicon::fold_apostrophes(text));
    }
    Some(effective)
}

fn judge(decision: Option<&Value>, fresh: Option<&Value>, supported: &[(String, String)]) -> Basis {
    let verdicts: Vec<Verdict> = unreadable(decision)
        .into_iter()
        .map(Verdict::Unjudged)
        .chain(keys(decision).map(|entry| key(entry, fresh, supported)))
        .chain(policies(decision).map(|entry| policy(entry, fresh)))
        .collect();
    if verdicts.is_empty() {
        return Basis::None;
    }
    let (mut moved, mut unjudged) = (Vec::new(), Vec::new());
    for verdict in &verdicts {
        match verdict {
            Verdict::Moved(why) if !moved.contains(why) => moved.push(why.clone()),
            Verdict::Unjudged(why) if !unjudged.contains(why) => unjudged.push(why.clone()),
            Verdict::Holds | Verdict::Moved(_) | Verdict::Unjudged(_) => {}
        }
    }
    if !moved.is_empty() {
        Basis::Moved(moved)
    } else if !unjudged.is_empty() {
        Basis::Unjudged(unjudged)
    } else {
        Basis::Holds(verdicts.len())
    }
}

/// Every key the decision records a rule reading, admissible or not.
fn keys(decision: Option<&Value>) -> impl Iterator<Item = &Value> {
    listed(decision, "grounding")
}

/// Every number policy the decision chose from observed kinds alone (no question asked).
fn policies(decision: Option<&Value>) -> impl Iterator<Item = &Value> {
    listed(decision, "numbers").filter(|entry| entry["bound_by"] == "observed numbers")
}

/// The entries of one record that this law reads (objects); [`unreadable`] names the rest.
fn listed<'a>(decision: Option<&'a Value>, record: &str) -> impl Iterator<Item = &'a Value> {
    decision
        .and_then(|d| d.get(record))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.is_object())
}

/// A field this law reads from a recorded entry, and the JSON type it must have when present.
type Typed = (&'static str, fn(&Value) -> bool);

/// Why the records this law reads cannot be read, each once: a decision that is not an object, a
/// record present but not a list, an entry that is not an object or holds a field this law reads
/// with another type. An absent record is a legacy decision's, never unreadable.
fn unreadable(decision: Option<&Value>) -> Vec<String> {
    let Some(decision) = decision else {
        return Vec::new();
    };
    let Some(records) = decision.as_object() else {
        return vec!["the recorded decision is not an object".to_owned()];
    };
    let read: [(&str, &[Typed]); 2] = [
        (
            "grounding",
            &[
                ("grade", Value::is_string),
                ("revision", Value::is_string),
                ("in_every_sampled_record", Value::is_boolean),
            ],
        ),
        ("numbers", &[("bound_by", Value::is_string)]),
    ];
    let mut why: Vec<String> = Vec::new();
    for (record, fields) in read {
        let Some(present) = records.get(record) else {
            continue;
        };
        let Some(entries) = present.as_array() else {
            why.push(format!("the recorded `{record}` is not a list"));
            continue;
        };
        let bad = entries.iter().any(|entry| {
            entry.as_object().is_none_or(|e| {
                fields
                    .iter()
                    .any(|(field, typed)| e.get(*field).is_some_and(|v| !typed(v)))
                    || (record == "numbers"
                        && !matches!(
                            e.get("bound_by").and_then(Value::as_str),
                            Some(
                                "observed numbers"
                                    | "answer"
                                    | "pending"
                                    | "unobserved"
                                    | "grounding"
                                    | "default"
                            )
                        ))
            })
        });
        if bad {
            why.push(format!("a recorded `{record}` entry cannot be read"));
        }
    }
    why
}

/// One grounded key, graded again on the fresh row of its source by [`grounding::grade`].
fn key(entry: &Value, fresh: Option<&Value>, supported: &[(String, String)]) -> Verdict {
    let (Some(source), Some(key)) = (entry["source"].as_str(), entry["field"].as_str()) else {
        return Verdict::Unjudged("a recorded key names no source or no field".to_owned());
    };
    let Some(row) = grounding::row(fresh, source) else {
        return Verdict::Unjudged(format!("`{source}` was not observed again"));
    };
    let bare = |path: &str| path.strip_prefix("./").unwrap_or(path).to_owned();
    let stated: Vec<String> = supported
        .iter()
        .filter(|(path, _)| bare(path) == bare(source))
        .map(|(_, key)| key.clone())
        .collect();
    let asserted = stated.iter().any(|name| name == key);
    let Some(seen) = grounding::seen_in(fresh, Some(row)) else {
        // A key asserted over a source never observed stays asserted while it is still not
        // observed; a source that was observed and shows no record now has moved.
        let observed_then = entry["revision"].as_str().is_some_and(is_digest);
        if !observed_then {
            return if asserted {
                Verdict::Holds
            } else {
                Verdict::Unjudged(format!(
                    "`{key}` in `{source}` has no observed or request-bound assertion"
                ))
            };
        }
        return Verdict::Moved(format!(
            "`{source}` is {} now: `{key}` cannot be read from it",
            state_words(row)
        ));
    };
    let (grade, everywhere) = grounding::grade(key, Some(&seen), &stated);
    if grade == Grade::Inferred {
        // A bounded sample disproves nothing a request asserted.
        if asserted && !seen.declared && !seen.complete {
            return Verdict::Holds;
        }
        return Verdict::Moved(if seen.declared || seen.complete {
            format!(
                "`{source}` no longer has `{key}`: it has {}",
                seen.all.join(", ")
            )
        } else {
            format!("`{key}` is no longer in the sampled records of `{source}`")
        });
    }
    if entry["in_every_sampled_record"] == true && !everywhere {
        return Verdict::Moved(format!(
            "`{key}` is now missing from some sampled records of `{source}`"
        ));
    }
    Verdict::Holds
}

/// One number policy chosen from kinds that were all numbers: every sampled value of the field
/// must still be one (a value that is not would now be asked — skip or fail).
fn policy(entry: &Value, fresh: Option<&Value>) -> Verdict {
    let (Some(source), Some(field)) = (entry["source"].as_str(), entry["field"].as_str()) else {
        return Verdict::Unjudged(
            "a recorded number policy names no source or no field".to_owned(),
        );
    };
    let Some(row) = grounding::row(fresh, source) else {
        return Verdict::Unjudged(format!("`{source}` was not observed again"));
    };
    if grounding::seen(Some(row)).is_none() {
        return Verdict::Moved(format!(
            "`{source}` is {} now: the values of `{field}` cannot be read from it",
            state_words(row)
        ));
    }
    let Some(counts) = kinds(fresh, source)
        .and_then(|k| k.get("keys"))
        .and_then(|k| k.get(field))
        .and_then(Value::as_object)
    else {
        return Verdict::Unjudged(format!(
            "the kinds of `{field}` in `{source}` were not observed again"
        ));
    };
    // A count is a whole number: anything else cannot say the values are all numbers.
    let mut others: Vec<String> = Vec::new();
    for (kind, n) in counts {
        let Some(n) = n.as_u64() else {
            return Verdict::Unjudged(format!(
                "the kinds of `{field}` in `{source}` cannot be read"
            ));
        };
        if n > 0 && !NUMBER_KINDS.contains(&kind.as_str()) {
            others.push(format!("{kind} {n}"));
        }
    }
    if others.is_empty() {
        return Verdict::Holds;
    }
    Verdict::Moved(format!(
        "`{field}` in `{source}` now has sampled values that are not numbers ({})",
        others.join(" · ")
    ))
}

/// The kinds a fresh observation recorded for the one path `source` names.
fn kinds<'a>(fresh: Option<&'a Value>, source: &str) -> Option<&'a Value> {
    let bare = |p: &str| p.strip_prefix("./").unwrap_or(p).to_owned();
    let mut matched = fresh?
        .get("kinds")?
        .as_object()?
        .iter()
        .filter(|(path, _)| bare(path) == bare(source));
    let (_, first) = matched.next()?;
    matched.next().is_none().then_some(first)
}

/// Whether a recorded revision is the digest of an observed peek (not a state word).
fn is_digest(revision: &str) -> bool {
    revision.len() == 64 && revision.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A row's state, in words.
fn state_words(row: &Value) -> &'static str {
    match row["state"].as_str() {
        Some("absent") => "absent",
        Some("unreadable") => "unreadable",
        Some("outside_project") => "outside the project",
        Some("empty") => "empty",
        _ => "not readable as records",
    }
}

#[cfg(test)]
mod tests;
