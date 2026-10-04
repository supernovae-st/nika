// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Every recorded rule is bound to its words by the law that created it, re-run where a record
//! enters (R4 S0; the E14 near-misses, rounds 1 to 5): a record is data the caller hands back,
//! and no field of it (a strategy word, a receipt) selects a weaker law.
//! - A rule whose words the closed grammar reads is that reading, under the request's column
//!   hint, the observed columns or none: every clause, junction, flag and shape key, so a key
//!   the record dropped is no default the words did not state.
//! - A line filter is what `line_filter` reads from its words.
//! - A verified program stands only where the grammar reads no typed rule (the transform flow
//!   asks a program only then); `Plan::anchored` binds it to the compute step it realizes.
//! - A rule whose words neither reads is a seat's typed computation: it must be the fixpoint of
//!   the law that admitted it (`nika_compile_fidelity::predicate::rederives`). That law grounds
//!   a field, a value, a number or a limit somewhere in the request, and a comparator, an
//!   aggregate, a junction or a direction nowhere: another such element it admits can replace
//!   the recorded one unseen. That hole is open; a rule the grammar reads never falls back to
//!   this weaker law.
//! - Any other rule cannot be re-derived here: it is refused by name, never trusted.
//! - A written literal (a `content` binding) is the one the reader reads as a write's content.
//! - An identity (no clause, stage, program or flag) is a conversion's: bound only where the
//!   reader reads its words as that very conversion, never by a seat's law.
use nika_compile_reader::plan::Plan;
use nika_compile_reader::rules::{self, Rule};
use serde_json::Value;

/// The first recorded literal or rule its law does not re-derive, named with what differs, or
/// `None` when every one is bound to its words.
#[must_use]
pub fn unbound(plan: &Plan, intent: &str, observed: Option<Vec<String>>) -> Option<String> {
    if let Some(why) = unread_content(plan, intent) {
        return Some(why);
    }
    let mut hints = vec![
        nika_compile_reader::columns::columns_hint(intent),
        Vec::new(),
    ];
    hints.extend(observed);
    plan.rules
        .iter()
        .find_map(|rule| unbound_rule(rule, plan, intent, &hints))
}

/// A written literal is the reader's own: the request's reading states the same literal as a
/// write's content, so a record never writes text the request only quotes, matches or names
/// (a record without one is not read again).
fn unread_content(plan: &Plan, intent: &str) -> Option<String> {
    let mut stated = plan
        .bindings
        .iter()
        .filter(|b| b.role == "content")
        .peekable();
    stated.peek()?;
    let read = nika_compile_reader::lexicon::read(intent).plan.bindings;
    let unread = stated.find(|b| !read.contains(b))?;
    Some(format!(
        "the recorded content {} is not a literal the request writes",
        unread.literal
    ))
}

fn unbound_rule(rule: &Rule, plan: &Plan, intent: &str, hints: &[Vec<String>]) -> Option<String> {
    let text = rule.text();
    if !rule.filters() && !rule.shaped() && !rule.summary() && !rule.lines() {
        let converted = nika_compile_reader::lexicon::read(text)
            .plan
            .rules
            .contains(rule);
        return (!converted).then(|| {
            format!("the recorded identity for `{text}` is no conversion its words state")
        });
    }
    let mut readings: Vec<Rule> = hints
        .iter()
        .filter_map(|hint| rules::synthesize(text, hint))
        .collect();
    if rule.verified_program().is_some() {
        return (!readings.is_empty()).then(|| {
            format!("the recorded program for `{text}` stands where its words state a typed rule")
        });
    }
    readings.extend(rules::line_filter(text));
    if readings.iter().any(|reading| reading == rule) {
        return None;
    }
    let seat = |hint: &Vec<String>| crate::predicate::rederives(rule, intent, &plan.slots, hint);
    if readings.is_empty() && hints.iter().any(seat) {
        return None;
    }
    Some(match readings.first() {
        Some(reading) => format!(
            "the recorded rule for `{text}` is not what its words say ({})",
            difference(&rule.to_json(), &reading.to_json(), "rule")
        ),
        None => format!("the recorded rule for `{text}` cannot be re-derived from its words"),
    })
}

/// The first place a recorded rule and the reading of its words part, as `path: recorded X,
/// the words read Y`.
fn difference(recorded: &Value, read: &Value, path: &str) -> String {
    match (recorded, read) {
        (Value::Object(a), Value::Object(b)) => {
            let keys = a.keys().chain(b.keys().filter(|k| !a.contains_key(*k)));
            for key in keys {
                let (x, y) = (a.get(key), b.get(key));
                if x != y {
                    let (x, y) = (x.unwrap_or(&Value::Null), y.unwrap_or(&Value::Null));
                    return difference(x, y, &format!("{path}.{key}"));
                }
            }
            format!("{path}: the same fields")
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            match a.iter().zip(b).enumerate().find(|(_, (x, y))| x != y) {
                Some((at, (x, y))) => difference(x, y, &format!("{path}[{at}]")),
                None => format!("{path}: the same items"),
            }
        }
        _ => format!("{path}: recorded {recorded}, the words read {read}"),
    }
}
