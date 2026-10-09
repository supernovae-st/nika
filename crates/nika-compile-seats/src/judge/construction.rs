// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A construction obligation (R5 · C13 · A5): a clause may ask how the document is built (an
//! admitted component it asks for, a condition on what the catalogue provides) rather than what a
//! task does. A runtime task can be right while such a clause is unmet, so the judge's
//! localization of a missing clause names the component the bytes lack, never an invented task.
//!
//! The door records, beside each component it offered, what the judged bytes hold of it
//! ([`assess`]): whether the lent release resolves its admitted bytes (its contract is then
//! examinable: the callables it declares beside the holes and effects the offer shows), and the
//! strongest current-byte witness of a receipt of it on those bytes (`held`). Those are
//! observations of what is present. Whether a component serves the request is the judge's,
//! decided against the original request, its explicit constraints and the component's contract:
//! the candidate's own permits, verbs or tasks are never the request's allowed reach, so they
//! can neither excuse a missing composition nor exclude a component.
//!
//! [`Construction`] turns those facts into the alternatives a localization may answer: one per
//! offered component that resolves and that the bytes do not hold as admitted, and `no_fit`. A
//! named component becomes a defect a repair composes from (the clause, the component, its holes
//! and contract); the judge's `no_fit` settles that conditional clause alone, its own alternative
//! standing, only when every offer was examinable: an offer the catalogue could not resolve
//! leaves the fit unknown. A resolver or a receipt establishes identity and expansion, never fit;
//! a name, a title, a relevance verdict or an author's claim proves none of it.

use serde_json::{Value, json};

use crate::decide::ChoiceOption;
use crate::foundry::{ComponentCatalog, ComponentRef, Release};

/// The witness verdicts that hold a component as admitted.
const HELD: [&str; 2] = ["expanded", "invoked"];

/// What a localization's instructions add when the catalogue offered components.
const CONSTRUCTION: &str = "A clause may concern how the document is built rather than what a task does. `authoring.offered` lists each admitted component the catalogue offered, with its contract (purpose, holes, effects, `construction.callables`) and what these bytes hold of it (`construction.held`: the witness of its receipt on these bytes, null when no receipt names it; `construction.unresolved`: the catalogue gives no admitted bytes for it, so it cannot be examined). Judge fit against the original request, its explicit constraints and each contract, never against the candidate's own permits or tasks, which may lack what the request needs. component-<k>: the clause asks for that offered component, its contract can do that part with what the request allows, and these bytes do not hold it as admitted. no_fit: the clause asks for an admitted component when one applies, and none offered can do that part with what the request allows: the clause's own alternative stands.";

/// What `no_fit` means as an option.
const NO_FIT: &str =
    "no offered component can do this part with what the request allows: its alternative stands";

/// Each offered component's construction status on the bytes judged: its row gains
/// `construction`, `{"held": <the strongest witness verdict a receipt of the same component in
/// the lent release gives on those bytes, or null>, "callables": <what its admitted row
/// declares>}`, or, when the lent catalogue resolves no admitted bytes for it, `{"held": …,
/// "unresolved": <why>}`. `composed` are the receipts the door's section holds, each witnessed on
/// those bytes (`judge::lent`).
pub(super) fn assess(catalog: &dyn ComponentCatalog, offered: &mut Value, composed: &[Value]) {
    let release = catalog.release();
    for row in offered.as_array_mut().into_iter().flatten() {
        let id = row["component"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let held = strongest(composed, &id, &release);
        let reference = ComponentRef::new(id)
            .at_version(release.version.clone())
            .in_release(release.snapshot_sha256.clone());
        row["construction"] = match catalog.resolve(&reference) {
            Ok(component) => json!({"held": held, "callables": component.callables}),
            Err(why) => json!({"held": held, "unresolved": why.to_string()}),
        };
    }
}

/// The strongest verdict the receipts of `id` in `release` are witnessed as: held as admitted,
/// then revised, then absent or unreadable; null when no receipt names it there. A receipt is of
/// that release only when it pins both its version and its digest: the same version under
/// another digest is another release's component, whatever its nodes.
fn strongest(composed: &[Value], id: &str, release: &Release) -> Value {
    let rank = |verdict: &str| match verdict {
        _ if HELD.contains(&verdict) => 3,
        "revised" => 2,
        _ => 1,
    };
    let pinned = |seen: &Value| {
        seen["release"]["version"] == release.version.as_str()
            && seen["release"]["snapshot_sha256"] == release.snapshot_sha256.as_str()
    };
    (composed.iter())
        .filter(|seen| seen["component"] == id && pinned(seen))
        .filter_map(|seen| seen["verdict"].as_str())
        .max_by_key(|verdict| rank(verdict))
        .map_or(Value::Null, |verdict| json!(verdict))
}

/// What a localization of a missing clause may say of how the judged bytes are built, read from
/// the engine facts of the state it judges (`authoring`, `judge::authoring`) and nothing else.
/// A state with no offer, or whose facts state no construction status (a record older than it),
/// offers nothing more than the runtime alternatives.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Construction {
    /// Every offered component, in the offer's order, when each states its status.
    offered: Vec<Value>,
}

/// What a construction answer of a localization stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Construed {
    /// The judge names an offered component that resolves and that the bytes do not hold as
    /// admitted: the defect's note, for the repair.
    Defect(String),
    /// The judge finds no offered component fitting and every offer was examinable (its admitted
    /// bytes resolve): the clause's own alternative stands, for that clause alone.
    Fallback,
    /// The judge finds none fitting, but an offer could not be examined: the fit stays unknown.
    Undecided,
}

impl Construction {
    /// The construction alternatives of the judged `state`.
    #[must_use]
    pub fn of(state: &Value) -> Self {
        let rows = (state["authoring"]["offered"]["components"].as_array()).cloned();
        let rows = rows.unwrap_or_default();
        if rows.iter().any(|row| !row["construction"].is_object()) {
            return Self::default();
        }
        Self { offered: rows }
    }

    /// Whether the catalogue resolved `row`'s admitted bytes: its contract is examinable.
    fn examinable(row: &Value) -> bool {
        row["construction"]["unresolved"].is_null()
    }

    /// Whether the bytes hold `row` as admitted.
    fn held(row: &Value) -> bool {
        (row["construction"]["held"].as_str()).is_some_and(|verdict| HELD.contains(&verdict))
    }

    /// Whether `row` resolves and the bytes do not hold it as admitted.
    fn open(row: &Value) -> bool {
        Self::examinable(row) && !Self::held(row)
    }

    /// The alternatives as options: `component-<k>` for each open component (`k`: its place in
    /// the offer), then `no_fit`; none without an offer.
    #[must_use]
    pub fn options(&self) -> Vec<ChoiceOption> {
        let mut options: Vec<ChoiceOption> = (self.offered.iter().enumerate())
            .filter(|(_, row)| Self::open(row))
            .map(|(k, row)| {
                let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
                let (id, version) = (&row["component"]["id"], &row["component"]["version"]);
                let described = format!(
                    "`{}` (release {}) · {}: {}",
                    text(id),
                    text(version),
                    text(&row["title"]),
                    lacking(row)
                );
                ChoiceOption::new(format!("component-{k}"), described)
            })
            .collect();
        if !self.offered.is_empty() {
            options.push(ChoiceOption::new("no_fit", NO_FIT));
        }
        options
    }

    /// `instructions` followed by what the alternatives mean, when there are any.
    #[must_use]
    pub fn told(&self, instructions: String) -> String {
        if self.offered.is_empty() {
            return instructions;
        }
        format!("{instructions} {CONSTRUCTION}")
    }

    /// What the answer `key` says of the construction; `None` for a key that is not one of its
    /// alternatives.
    #[must_use]
    pub fn read(&self, key: &str) -> Option<Construed> {
        if self.offered.is_empty() {
            return None;
        }
        if key == "no_fit" {
            return Some(if self.offered.iter().all(Self::examinable) {
                Construed::Fallback
            } else {
                Construed::Undecided
            });
        }
        let k: usize = key.strip_prefix("component-")?.parse().ok()?;
        let row = self.offered.get(k).filter(|row| Self::open(row))?;
        Some(Construed::Defect(noted(row)))
    }

    /// The basis of a construction answer on its question's record: for `no_fit`, every offered
    /// component with what the bytes hold of it and whether the alternative stands; for a named
    /// component, its identity. Any other answer leaves the record as it is.
    pub fn annotate(&self, record: Option<&mut Value>, answer: Option<&str>) {
        let (Some(record), Some(answer)) = (record, answer) else {
            return;
        };
        let basis = |row: &Value| json!({"component": row["component"], "construction": row["construction"]});
        match self.read(answer) {
            Some(Construed::Defect(_)) => {
                let k = answer
                    .trim_start_matches("component-")
                    .parse::<usize>()
                    .ok();
                let row = k.and_then(|k| self.offered.get(k)).map(basis);
                record["construction"] = json!({"named": row});
            }
            Some(construed) => {
                let offered: Vec<Value> = self.offered.iter().map(basis).collect();
                let stands = construed == Construed::Fallback;
                record["construction"] = json!({"no_fit": {"alternative_stands": stands,
                    "offered": offered}});
            }
            None => {}
        }
    }
}

/// What the bytes hold of an open component, as an option and a note say it.
fn lacking(row: &Value) -> &'static str {
    match row["construction"]["held"].as_str() {
        Some("revised") => "these bytes hold it changed from its admitted nodes",
        _ => "these bytes do not hold it",
    }
}

/// The defect a named component leaves, as the repair reads it: the component by reference, what
/// the bytes hold of it, its holes with who fills each (and the producer's note), and what its
/// contract reaches.
fn noted(row: &Value) -> String {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    let listed = |items: Vec<String>| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items.join(", ")
        }
    };
    let all = |value: &Value| -> Vec<String> {
        (value.as_array().into_iter().flatten()).map(text).collect()
    };
    let holes: Vec<String> = (row["holes"].as_array().into_iter().flatten())
        .map(|hole| match hole["note"].as_str() {
            Some(note) => format!("{} ({}: {note})", text(&hole["name"]), text(&hole["owner"])),
            None => format!("{} ({})", text(&hole["name"]), text(&hole["owner"])),
        })
        .collect();
    format!(
        "the judge points to the admitted component `{}` of release {} ({}): {}; compose it by that reference, each hole bound as its owner and contract state (holes: {}) from the request, its answers, the observed world or what they establish, a question only for a value its human owner must give and none of them gives, never the component's own literal its contract does not grant; it declares the effects {} through {}, which grant nothing: the document's own permits grant exactly what it reaches; a copy by hand is no composition",
        text(&row["component"]["id"]),
        text(&row["component"]["version"]),
        text(&row["title"]),
        lacking(row),
        listed(holes),
        listed(all(&row["effects"])),
        listed(all(&row["construction"]["callables"])),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
