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
//! as it treats that unmatched text (a byte comparison treats the rows spelled that way as
//! unmatched: it drops them, or keeps the rows a negation excludes), and it must not return a
//! value on one spelling and fail on the other.
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
//! A program that stops with an error on the unmatched text shows no treatment of it to compare
//! (B21 D1: an error on U+2400 left the law silent and a byte comparison READY, summing 0 where
//! 42 was due). The law then reads that treatment from a value the host observed in the bound
//! column that the clause does not state ([`Bound::unstated`]), probed the same way. When the
//! program stops with an error on that value too, or the host observed none, the law cannot
//! judge the program: it refuses nothing on that ground, and the program's record, an applied
//! finding and the decision's `unjudged_spellings` say so, naming the column, both spellings,
//! their code points and every text tried ([`Spelled::qualify`]). The verifier shows those notes
//! to the judges of the candidate running that program, so the clause and the whole request are
//! settled by a judge that reads them, never silently. A program that treats the chosen value
//! as a special case still escapes: the observed value is a stand-in, not a proof.
//!
//! A bounded law over the seat's own example rows and the host's bounded sample, never a proof
//! that two programs mean the same: a spelling the sample did not show, a column without
//! categorical values, a literal the clause does not state, case and compatibility forms bind
//! nothing, and a program treating the observed spelling some third way (neither as the stated
//! one nor as unmatched) is left to the verifier.

use super::super::verify::UNJUDGED_SPELLINGS as UNJUDGED_KEY;
use super::{CompileOutcome, DiagnosticKind, ProposedTransform, Refusal, run};
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
    /// The first value the host observed in `column` that the clause does not state
    /// ([`unstated`]): the column's own stand-in for a value the clause does not state, probed
    /// when the program stops with an error on [`UNMATCHED`] (B21 T1).
    unstated: Option<String>,
}

/// The host-observed categorical values of the request's one stated source, and the literals one
/// clause states that they spell with other bytes (named by the transform laws' signatures).
pub(in crate::cognition) struct Spelled {
    values: Map<String, Value>,
    bound: Vec<Bound>,
    /// The clause's own words, which the note on a program the law cannot judge names.
    clause: String,
}

impl Spelled {
    /// The bindings of `clause` (the request's own words) over the host's observed `world`.
    pub(super) fn of(world: Option<&Value>, intent: &str, clause: &str) -> Self {
        let values = categorical(world, intent);
        let columns = for_intent(world, intent).unwrap_or_default();
        let mut bound = Vec::new();
        for (column, spellings) in &values {
            let held: Vec<String> = spellings
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let pairs = stated_spellings(clause, &held, &columns);
            let unstated = unstated(clause, &held, &columns, &pairs);
            for (stated, observed) in pairs {
                let column = column.clone();
                bound.push(Bound {
                    column,
                    stated,
                    observed,
                    unstated: unstated.clone(),
                });
            }
        }
        let clause = clause.to_owned();
        Self {
            values,
            bound,
            clause,
        }
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
        self.judge(proposed).map(|_| ())
    }

    /// What the law could not judge of an accepted program (B21 T1), each binding once: a note
    /// naming the clause, the program, the column, both spellings, their code points and every
    /// text tried, told in an applied finding and kept, once, in the decision's
    /// `unjudged_spellings` for the judges of the candidate running that program. It refuses
    /// nothing; the notes are returned for the program's own record.
    pub(super) fn qualify(
        &self,
        proposed: &ProposedTransform,
        out: &mut CompileOutcome,
    ) -> Vec<Value> {
        let program = proposed.jq.trim();
        let unjudged = self.judge(proposed).unwrap_or_default();
        let notes: Vec<Value> = unjudged
            .iter()
            .map(|(bound, tried)| bound.note(&self.clause, program, tried))
            .collect();
        for (bound, _) in &unjudged {
            let told = bound.unjudged(&self.clause);
            crate::finding(out, DiagnosticKind::Applied, "authoring_transform", told);
        }
        if !notes.is_empty() {
            let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
            let mut kept = decision[UNJUDGED_KEY]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for note in &notes {
                if !kept.contains(note) {
                    kept.push(note.clone());
                }
            }
            decision[UNJUDGED_KEY] = json!(kept);
            out.provenance.decision = Some(decision);
        }
        notes
    }

    /// The law over `proposed` on each one-row source of its own example: a refusal, or each
    /// binding whose treatment of a value the clause does not state the program shows none of
    /// (an error on every text [`Bound::tried`] offers), once, with the texts tried.
    fn judge(&self, proposed: &ProposedTransform) -> Result<Vec<(&Bound, Vec<String>)>, Refusal> {
        let program = proposed.jq.trim();
        let example = proposed
            .example_input
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let mut unjudged: Vec<(&Bound, Vec<String>)> = Vec::new();
        for bound in &self.bound {
            for row in example.iter().filter(|row| row.is_object()) {
                let probe = |text: &str| {
                    let mut row = row.clone();
                    row[bound.column.as_str()] = json!(text);
                    let output = run(program, &json!({"records": [row]}));
                    output.map(|value| read_back(value, text))
                };
                let how = match (probe(&bound.stated), probe(&bound.observed)) {
                    (Ok(stated), Ok(observed)) => {
                        let tried = bound.tried();
                        if let Some(none) = tried.iter().find_map(|text| probe(text).ok()) {
                            dropped(stated != none, observed != none)
                        } else {
                            if !unjudged.iter().any(|(b, _)| std::ptr::eq(*b, bound)) {
                                unjudged.push((bound, tried));
                            }
                            None
                        }
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
        Ok(unjudged)
    }
}

/// How a program treating one spelling as it treats a value the clause does not state tells the
/// two apart: `kept` when the stated spelling is treated otherwise, `spelled` when the observed
/// one is.
fn dropped(kept: bool, spelled: bool) -> Option<&'static str> {
    match (kept, spelled) {
        (true, false) => Some(DROPS_OBSERVED),
        (false, true) => Some(DROPS_STATED),
        _ => None,
    }
}

/// The first value `held` in a column (as the host observed it) that the clause does not state
/// (B21 T1): not within the clause's own text, case aside; no canonical spelling of it stated at
/// token boundaries; and not, case aside, either spelling of a literal the clause binds there.
/// An empty value is within every text, so it is never chosen. `None` when there is none.
fn unstated(
    clause: &str,
    held: &[String],
    columns: &[String],
    pairs: &[(String, String)],
) -> Option<String> {
    let lower = clause.to_lowercase();
    held.iter()
        .find(|value| {
            let lowered = value.to_lowercase();
            let spells = |text: &String| text.to_lowercase() == lowered;
            !lower.contains(&lowered)
                && !pairs
                    .iter()
                    .any(|(stated, observed)| spells(stated) || spells(observed))
                && stated_spellings(clause, std::slice::from_ref(*value), columns).is_empty()
        })
        .cloned()
}

/// The probe text neither spelling is: what the program does with a value the clause does not
/// state (U+2400, the symbol for null, which no categorical value of a source spells).
const UNMATCHED: &str = "\u{2400}";
/// What the program does with the other spelling: the observed one treated as unmatched. Neutral
/// words (B21 T4): under a negated literal the program keeps the rows it should exclude, so the
/// defect says how the two spellings are told apart, never that rows are dropped.
const DROPS_OBSERVED: &str = "the program treats the observed spelling as it treats a value the clause does not state, not as it treats the stated spelling";
/// What the program does with the other spelling: the stated one treated as unmatched.
const DROPS_STATED: &str = "the program treats the stated spelling as it treats a value the clause does not state, not as it treats the observed spelling";
/// What the program does with the other spelling: an error on the observed one.
const FAILS_OBSERVED: &str =
    "the program fails on the observed spelling where it returns a value on the stated one";
/// What the program does with the other spelling: an error on the stated one.
const FAILS_STATED: &str =
    "the program fails on the stated spelling where it returns a value on the observed one";
/// Why the law could not judge a program over a binding (B21 T1).
const UNJUDGED: &str = "the program stops with an error on every text the law tried that the clause does not state, so the spelling law could not compare its treatment of the observed spelling with its treatment of the stated one; the judges settle the clause";

impl Bound {
    /// The texts whose treatment stands for a value the clause does not state: [`UNMATCHED`],
    /// then the column's own [`Bound::unstated`] value when the host observed one.
    fn tried(&self) -> Vec<String> {
        std::iter::once(UNMATCHED.to_owned())
            .chain(self.unstated.clone())
            .collect()
    }

    /// The note on `program`, which the law could not judge over this binding after trying
    /// `tried`: kept in the program's record and shown to the judges of its candidate.
    fn note(&self, clause: &str, program: &str, tried: &[String]) -> Value {
        json!({
            "clause": clause,
            "program": program,
            "column": self.column,
            "stated": self.stated,
            "stated_code_points": points(&self.stated),
            "observed": self.observed,
            "observed_code_points": points(&self.observed),
            "tried": tried,
            "why": UNJUDGED,
        })
    }

    /// The applied finding on a program the law could not judge over this binding.
    fn unjudged(&self, clause: &str) -> String {
        format!(
            "The spelling law could not judge the seat's program for `{}`: in `{}` the clause states `{}` ({}) and the source holds `{}` ({}); {UNJUDGED}.",
            clause.trim(),
            self.column,
            self.stated,
            points(&self.stated),
            self.observed,
            points(&self.observed)
        )
    }

    /// The concrete defect the repair is told: the column, both spellings and their code points,
    /// and `how` the program told them apart.
    fn refusal(&self, how: &str) -> String {
        format!(
            "{LEAD}: in `{}` the clause states `{}` ({}) and the source holds `{}` ({}), the same text under Unicode canonical equivalence; the program must treat both exactly alike (whatever it does with the stated spelling, it does with the observed one): {how}",
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
