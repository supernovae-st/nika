// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The observed spelling of a literal a clause states, held by a program the seat writes (R4
//! A11): the bounded canonical-spelling law of R4 A5
//! ([`nika_compile::surface::observed::equivalent_spellings`]), which the typed equalities
//! already hold. A binding is a column of the request's one stated source whose host-observed
//! categorical values spell, with other bytes, a literal the clause states at exact token
//! boundaries ([`nika_compile::surface::observed::stated_spellings`]).
//!
//! A program must not drop either spelling of a bound column, whether or not its `columns_read`
//! declares that column (a bracket read escapes a declaration; a column the program never reads
//! moves none of its outputs, so probing it refuses nothing): on each one-row source of the
//! seat's own example with the column set to the stated literal, to the observed spelling, and
//! to a text neither spells ([`UNMATCHED`]), it must not treat exactly one of the two spellings
//! as it treats that unmatched text (the rows spelled that way would be dropped, as a byte
//! comparison drops them), and it must not return a value on one spelling and fail on the other.
//! Every string value and key exactly equal to a probe's own text reads back to one placeholder,
//! so echoing or grouping the row's value is no difference. An error on both spellings is no
//! spelling difference: the row errs whatever the spelling, and the value laws and the run own
//! that error. What the program makes of the value itself may differ between the two spellings
//! (a label, a case, a code-point length, an encoding answer differently by their very
//! definition): that is the request's to ask and the whole-request verifier's to judge, never
//! this law's to refuse. A refused program is named with the column, both spellings and their
//! code points, for the request's one repair allowance. The literal stays the request's and the
//! program's bytes are never rewritten; the observed spelling of a stated literal is no invented
//! literal.
//!
//! A bounded law over the seat's own example rows and the host's bounded sample, never a proof
//! that two programs mean the same: a spelling the sample did not show, a column without
//! categorical values, a literal the clause does not state, case and compatibility forms bind
//! nothing, and a program treating the observed spelling some third way (neither as the stated
//! one nor as unmatched) is left to the verifier.

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

    /// The law over `proposed`: every binding, whatever columns it declares, on each one-row
    /// source of its own example (the module's law).
    pub(super) fn honored(&self, proposed: &ProposedTransform) -> Result<(), Refusal> {
        let program = proposed.jq.trim();
        let example = proposed
            .example_input
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        for bound in &self.bound {
            for row in example.iter().filter(|row| row.is_object()) {
                let probe = |text: &str| {
                    let mut row = row.clone();
                    row[bound.column.as_str()] = json!(text);
                    let output = run(program, &json!({"records": [row]}));
                    output.map(|value| read_back(value, text))
                };
                let unmatched = probe(UNMATCHED).ok();
                let how = match (probe(&bound.stated), probe(&bound.observed)) {
                    (Ok(stated), Ok(observed)) => unmatched.and_then(|none| {
                        let (kept, spelled) = (stated != none, observed != none);
                        match (kept, spelled) {
                            (true, false) => Some(DROPS_OBSERVED),
                            (false, true) => Some(DROPS_STATED),
                            _ => None,
                        }
                    }),
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

/// The probe text neither spelling is: what the program does with a value the clause does not
/// state (U+2400, the symbol for null, which no categorical value of a source spells).
const UNMATCHED: &str = "\u{2400}";
/// What the program does with the other spelling: the observed one treated as unmatched.
const DROPS_OBSERVED: &str = "the program treats the observed spelling as a value the clause does not state, so it drops the rows the source spells that way";
/// What the program does with the other spelling: the stated one treated as unmatched.
const DROPS_STATED: &str = "the program treats the stated spelling as a value the clause does not state, so it drops the rows spelled as the request states them";
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

/// The one placeholder every probe's own text reads back to.
const ECHOED: &str = "\u{0}echoed probe text\u{0}";

/// `value` with every string value and key exactly equal to the probe's own `text` read back to
/// one placeholder: an echoed or grouped value is no difference, a text built from it is.
fn read_back(value: Value, text: &str) -> Value {
    let exact = |found: String| {
        if found == text {
            ECHOED.to_owned()
        } else {
            found
        }
    };
    match value {
        Value::String(found) => Value::String(exact(found)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| read_back(item, text))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, item)| (exact(key), read_back(item, text)))
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
