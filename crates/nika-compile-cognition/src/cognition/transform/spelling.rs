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
//! spelling, its outputs agree once the observed spelling is read back as the stated one (a
//! program that echoes or groups by the value is not refused for echoing it). A program that
//! compares bytes selects no row the source spells the other way: it is refused naming the
//! column, both spellings and their code points, for the request's one repair allowance. The
//! literal stays the request's and the program's bytes are never rewritten; the observed
//! spelling of a stated literal is no invented literal.
//!
//! A bounded law over the seat's own example rows and the host's bounded sample, never a proof
//! that two programs mean the same: a spelling the sample did not show, a column without
//! categorical values, a literal the clause does not state, case and compatibility forms bind
//! nothing, and a program that embeds the value in a longer text is refused unless it reads the
//! same once read back.

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
    /// own example.
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
                let (Ok(as_stated), Ok(as_observed)) = (as_stated, as_observed) else {
                    continue;
                };
                if read_back(as_observed, &bound.observed, &bound.stated) != as_stated {
                    return Err(Refusal(bound.refusal()));
                }
            }
        }
        Ok(())
    }
}

impl Bound {
    /// The concrete defect the repair is told.
    fn refusal(&self) -> String {
        format!(
            "{LEAD}: in `{}` the clause states `{}` ({}) and the source holds `{}` ({}), the same text under Unicode canonical equivalence; the program must treat both exactly alike (keep the stated spelling and also match the observed one): comparing bytes to one of them selects no row the source spells with the other",
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

/// `value` with the observed spelling read back as the stated one, in every string and key.
fn read_back(value: Value, observed: &str, stated: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(observed, stated)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| read_back(item, observed, stated))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, item)| {
                    let key = key.replace(observed, stated);
                    (key, read_back(item, observed, stated))
                })
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
