// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The observed spelling of a literal a clause states, held by a program the seat writes (R4
//! A11): the bounded canonical-spelling law of R4 A5
//! ([`nika_compile::surface::observed::equivalent_spellings`]), which the typed equalities
//! already hold. A binding is a column of the request's one stated source whose host-observed
//! categorical values spell, with other bytes, a literal the clause states at exact token
//! boundaries ([`nika_compile::surface::observed::stated_spellings`]).
//!
//! A program reading a bound column must treat both spellings alike: on each one-row source of
//! the seat's own example with the column set to the stated literal, then to the observed
//! spelling, both runs return a value, and they agree once every string value and key exactly
//! equal to the observed spelling is read back as the stated one (a program that echoes or
//! groups by the value is not refused for echoing it). A value on one spelling and an error on
//! the other is a spelling difference; an error on both is none: the row errs whatever the
//! spelling, the program is equally undefined there, and the value laws and the run own that
//! error. A program that compares bytes selects no row the source spells the other way: it is
//! refused naming the column, both spellings and their code points, for the request's one
//! repair allowance. The literal stays the request's and the program's bytes are never
//! rewritten; the observed spelling of a stated literal is no invented literal.
//!
//! A bounded law over the seat's own example rows and the host's bounded sample, never a proof
//! that two programs mean the same: a spelling the sample did not show, a column without
//! categorical values, a literal the clause does not state, case and compatibility forms bind
//! nothing. A program that transforms the value's text (embeds it in a longer text, changes its
//! case) is not read back and is refused: a possible false refusal the law reports rather than
//! infers what the text became.

use super::{ProposedTransform, Refusal, run};
use nika_compile::surface::observed::{for_intent, stated_spellings};
use serde_json::{Map, Value, json};

/// The refusal's lead, which the one repair allowance recognizes (`domain::repairable`).
pub(super) const LEAD: &str = "the source spells a literal the clause states with other bytes";

/// A column of the stated source, a literal the clause states, and the spelling the host
/// observed among that column's categorical values.
struct Bound {
    column: String,
    stated: String,
    observed: String,
}

/// The host-observed categorical values of the request's one stated source, and the literals one
/// clause states that they spell with other bytes (named by the transform laws' signatures).
pub(in crate::cognition) struct Spelled {
    values: Map<String, Value>,
    bound: Vec<Bound>,
}

impl Spelled {
    /// The bindings of `clause` (the request's own words) over the host's observed `world`.
    pub(super) fn of(world: Option<&Value>, intent: &str, clause: &str) -> Self {
        let values = categorical(world, intent);
        let columns = for_intent(world, intent).unwrap_or_default();
        let mut bound = Vec::new();
        for (column, spellings) in &values {
            let observed: Vec<String> = spellings
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            for (stated, observed) in stated_spellings(clause, &observed, &columns) {
                let column = column.clone();
                bound.push(Bound {
                    column,
                    stated,
                    observed,
                });
            }
        }
        Self { values, bound }
    }

    /// Show the seat the categorical values the host observed, when it observed any: the text
    /// its program compares, as the source spells it.
    pub(super) fn inform(&self, state: &mut Value) {
        if !self.values.is_empty() {
            state["observed_values"] = Value::Object(self.values.clone());
        }
    }

    /// Whether `literal` is the observed spelling of a literal the clause states: no invented
    /// literal.
    pub(super) fn observes(&self, literal: &str) -> bool {
        self.bound.iter().any(|b| b.observed == literal)
    }

    /// The law over `proposed`: each binding of a column it reads, on each one-row source of its
    /// own example. A value on one spelling and an error on the other is refused; an error on
    /// both is no spelling difference (the module's law).
    pub(super) fn honored(&self, proposed: &ProposedTransform) -> Result<(), Refusal> {
        let program = proposed.jq.trim();
        let example = proposed
            .example_input
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let read = |b: &&Bound| proposed.columns_read.contains(&b.column);
        for bound in self.bound.iter().filter(read) {
            for row in example.iter().filter(|row| row.is_object()) {
                let source = |spelling: &str| {
                    let mut row = row.clone();
                    row[bound.column.as_str()] = json!(spelling);
                    json!({"records": [row]})
                };
                let as_stated = run(program, &source(&bound.stated));
                let as_observed = run(program, &source(&bound.observed));
                let how = match (as_stated, as_observed) {
                    (Ok(stated), Ok(observed)) => {
                        let read = read_back(observed, &bound.observed, &bound.stated);
                        (read != stated).then_some(OTHER)
                    }
                    (Ok(_), Err(_)) => Some(FAILS_OBSERVED),
                    (Err(_), Ok(_)) => Some(FAILS_STATED),
                    (Err(_), Err(_)) => None,
                };
                if let Some(how) = how {
                    return Err(Refusal(bound.refusal(how)));
                }
            }
        }
        Ok(())
    }
}

/// What the program does with the other spelling: another result.
const OTHER: &str =
    "comparing bytes to one of them selects no row the source spells with the other";
/// What the program does with the other spelling: an error on the observed one.
const FAILS_OBSERVED: &str =
    "the program fails on the observed spelling where it returns a value on the stated one";
/// What the program does with the other spelling: an error on the stated one.
const FAILS_STATED: &str =
    "the program fails on the stated spelling where it returns a value on the observed one";

impl Bound {
    /// The concrete defect the repair is told: the column, both spellings and their code points,
    /// and `how` the program told them apart.
    fn refusal(&self, how: &str) -> String {
        format!(
            "{LEAD}: in `{}` the clause states `{}` ({}) and the source holds `{}` ({}), the same text under Unicode canonical equivalence; the program must treat both exactly alike (keep the stated spelling and also match the observed one): {how}",
            self.column,
            self.stated,
            points(&self.stated),
            self.observed,
            points(&self.observed)
        )
    }
}

/// The code points of `text` (`U+0065 U+0301`): two spellings that render alike, told apart.
fn points(text: &str) -> String {
    let points: Vec<String> = text
        .chars()
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect();
    points.join(" ")
}

/// `value` with every string value and key exactly equal to the observed spelling read back as the
/// stated one: an echoed or grouped value, never a text built from it.
fn read_back(value: Value, observed: &str, stated: &str) -> Value {
    let exact = |text: String| {
        if text == observed {
            stated.to_owned()
        } else {
            text
        }
    };
    match value {
        Value::String(text) => Value::String(exact(text)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| read_back(item, observed, stated))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, item)| (exact(key), read_back(item, observed, stated)))
                .collect(),
        ),
        other => other,
    }
}

/// The host-observed categorical values of the request's one stated source, by column: nothing
/// when the request states several sources or none, or the host observed none or several rows
/// of it, or did not read it.
fn categorical(world: Option<&Value>, intent: &str) -> Map<String, Value> {
    let paths = nika_compile::stated_sources(intent);
    let [path] = paths.as_slice() else {
        return Map::new();
    };
    let bare = |p: &str| p.strip_prefix("./").unwrap_or(p).to_owned();
    let rows = world.and_then(|w| w["observed"].as_array());
    let mut matched = rows
        .into_iter()
        .flatten()
        .filter(|row| row["path"].as_str().is_some_and(|p| bare(p) == bare(path)));
    match (matched.next(), matched.next()) {
        (Some(row), None) if row["state"] == "observed" => {
            row["values"].as_object().cloned().unwrap_or_default()
        }
        _ => Map::new(),
    }
}
