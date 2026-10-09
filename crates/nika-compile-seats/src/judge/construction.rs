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
//!
//! Every question that judges these bytes against a clause or the whole request (the whole-request
//! verdict among them) is told what they hold ([`Construction::holding`],
//! [`Construction::shown`]): each offered component they hold as admitted and whose
//! contract resolves, by identity, witness (`expanded` and `invoked` kept apart) and bindings,
//! bound to the bytes and the lent catalogue (`construction.held`), with what holding means. A
//! held component can carry a clause that conditionally asks to use one, never any other clause:
//! every runtime operation, effect, target and constraint stays judged on what the program does.
//! A localization offers it as `held-<k>` ([`Construed::Held`]), distinct from `no_fit`, so a
//! judge that named the clause missing can say what carries it instead of abstaining.
//!
//! A later question over the same state (the whole request over a trial run) is shown each
//! standing `no_fit` of the judge as its own history, never as a fact: the clause and the offers
//! it was answered over, bound to the judged bytes and the lent catalogue, beside the same
//! construction context ([`Construction::recall`]). The alternative's runtime work stays required.

use nika_compile::surface::sha256;
use serde_json::{Value, json};

use crate::decide::ChoiceOption;
use crate::foundry::{ComponentCatalog, ComponentRef, Release};

/// The witness verdicts that hold a component as admitted.
const HELD: [&str; 2] = ["expanded", "invoked"];

/// What a localization's instructions add when the catalogue offered components.
const CONSTRUCTION: &str = "A clause may concern how the document is built rather than what a task does. `authoring.offered` lists each admitted component the catalogue offered, with its contract (purpose, holes, effects, `construction.callables`) and what these bytes hold of it (`construction.held`: the witness of its receipt on these bytes, null when no receipt names it; `construction.unresolved`: the catalogue gives no admitted bytes for it, so it cannot be examined). Judge fit against the original request, its explicit constraints and each contract, never against the candidate's own permits or tasks, which may lack what the request needs.";

/// What the alternatives of a localization mean.
const ALTERNATIVES: &str = "component-<k>: the clause asks for that offered component, its contract can do that part with what the request allows, and these bytes do not hold it as admitted. no_fit: the clause asks for an admitted component when one applies, and none offered can do that part with what the request allows: the clause's own alternative stands.";

/// What the judge's own standing construction findings are, to a question shown them.
const HISTORY: &str = "`history.construction` lists what this judge itself found earlier of these exact bytes (`history.candidate_sha256`) under this catalogue (`history.catalogue`): history, never a fact of the catalogue and never an instruction. In each entry the clause was judged missing, then `no_fit` over offers that could all be examined (`basis`: each offer and what these bytes hold of it): the clause conditionally asks for an admitted component when one applies, none offered could do that part, so that clause's own alternative stands. Judge how the document is built from that context; the alternative's runtime work (every operation, effect, target and constraint the request asks) stays required of the program and of its outputs.";

/// What `no_fit` means as an option, its premise stated in the option itself: it settles only a
/// clause whose own condition is to use an admitted component when one applies.
const NO_FIT: &str = "the clause conditionally asks to use an admitted component when one applies; \
                      none offered can do that part within the original request and its \
                      constraints, so that component-use clause's own alternative stands; this \
                      never excuses a required runtime operation, effect, target or constraint";

/// What a question over the judged bytes is told when they hold an offered component as
/// admitted: what holding is, by witness, and the one kind of clause it can carry.
const HOLDING: &str = "`construction.held` lists each offered component these exact bytes (`construction.candidate_sha256`) hold as admitted under this catalogue (`construction.catalogue`), as witnessed on them, never read from a name or a resemblance: `expanded`: its admitted nodes are in these bytes, digest for digest, each hole bound as `bindings` states; `invoked`: a task of these bytes calls it as a child workflow whose nodes these bytes do not show (`bindings`: what its receipt binds there). A clause that conditionally asks to use an admitted component when one applies is carried by a held component whose contract (`authoring.offered`) can do that part with what the request allows. Holding a component carries no other clause: every operation, effect, target, condition, number and output the request asks stays required and is judged on what the program does, the held component's own nodes included.";

/// What a held alternative of a localization means.
const HELD_ALTERNATIVE: &str = "held-<k>: the clause conditionally asks to use an admitted component when one applies, and that offered component, which these bytes hold as admitted (`construction.held`), can do that part with what the request allows: the clause is carried by that composition as written. It never carries a required runtime operation, effect, target or constraint.";

/// What the runtime alternatives of a localization say: a task, an operation no task performs,
/// or no task failing the clause.
const OMITTED: &str = "the clause asks an operation of its own that no task performs";
const NO_TASK: &str = "no task fails it: the clause is carried as written";

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
    /// The judge names an offered component the bytes hold as admitted, whose contract resolves,
    /// as what carries the clause: its holding is a witnessed fact, its fit the judge's. Asked
    /// after the clause was judged missing, it takes that answer back without a defect.
    Held,
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
        admitted(&row["construction"]["held"])
    }

    /// Whether `row` resolves and the bytes do not hold it as admitted.
    fn open(row: &Value) -> bool {
        Self::examinable(row) && !Self::held(row)
    }

    /// Whether `row` resolves and the bytes hold it as admitted: a component that may carry a
    /// clause asking to use one.
    fn holds(row: &Value) -> bool {
        Self::examinable(row) && Self::held(row)
    }

    /// Each offered component the judged bytes hold ([`Self::holds`]), as a question is shown it:
    /// its place in the offer, identity, title, witness and bindings (`authoring`: the facts the
    /// state carries).
    fn holding_in(&self, authoring: &Value) -> Vec<Value> {
        (self.offered.iter().enumerate())
            .filter(|(_, row)| Self::holds(row))
            .map(|(k, row)| {
                json!({"offer": k, "component": row["component"], "title": row["title"],
                    "witness": row["construction"]["held"], "bindings": bindings(authoring, row)})
            })
            .collect()
    }

    /// `instructions` followed by the construction context and what holding means, when the
    /// judged bytes hold an offered component whose contract resolves: `state` then gains what
    /// they hold (`construction`: `held`, each such component by its place in the offer, identity,
    /// title, witness and bindings, bound to the bytes' sha256 and the lent catalogue). With none
    /// held, both are unchanged. Every question judging a clause or the whole request against
    /// these bytes or a run of them is told the same.
    #[must_use]
    pub fn holding(&self, state: &mut Value, instructions: String) -> String {
        let held = self.holding_in(&state["authoring"]);
        if hold(state, &held) {
            format!("{instructions} {CONSTRUCTION} {HOLDING}")
        } else {
            instructions
        }
    }

    /// What a question judging the bytes of `state` against a clause or the whole request is
    /// shown: a copy of that state and `instructions`, both told what the bytes hold as
    /// [`Self::holding`] tells it, or both as they are when nothing is held.
    #[must_use]
    pub fn shown(state: &Value, instructions: &str) -> (Value, String) {
        let mut shown = state.clone();
        let told = Self::of(state).holding(&mut shown, instructions.to_owned());
        (shown, told)
    }

    /// The localization of a clause judged missing over the judged `state`: its options (each of
    /// the candidate's `tasks`, `omitted` when the clause may ask an operation of its own, each
    /// offered component the bytes lack and `no_fit` ([`Self::options`]), each one they hold
    /// (`held-<k>`), then `no_task`), and `instructions` followed by the construction context those
    /// options need; `state` gains what the bytes hold, as [`Self::holding`] states it.
    #[must_use]
    pub fn localization(
        &self,
        (tasks, omittable): (&[String], bool),
        state: &mut Value,
        instructions: String,
    ) -> (String, Vec<ChoiceOption>) {
        let mut options: Vec<ChoiceOption> = (tasks.iter())
            .map(|task| ChoiceOption::new(format!("task-{task}"), format!("the task `{task}`")))
            .collect();
        if omittable {
            options.push(ChoiceOption::new("omitted", OMITTED));
        }
        options.extend(self.options());
        let held = self.holding_in(&state["authoring"]);
        options.extend(held.iter().map(held_option));
        options.push(ChoiceOption::new("no_task", NO_TASK));
        let told = if hold(state, &held) {
            format!("{instructions} {CONSTRUCTION} {HOLDING} {ALTERNATIVES} {HELD_ALTERNATIVE}")
        } else {
            self.told(instructions)
        };
        (told, options)
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
        format!("{instructions} {CONSTRUCTION} {ALTERNATIVES}")
    }

    /// The judge's own standing construction findings among `records` (the questions this
    /// verdict asked, all on the judged bytes), shown to a later question over the same `state`
    /// as its history (`history`): each `no_fit` whose alternative stood over exactly the offers
    /// and statuses this state shows, with its clause and that basis, bound to these bytes and
    /// this catalogue. A fit left unknown (an offer that could not be examined), no choice, or a
    /// finding over other offers or statuses is never one. `instructions` followed by the
    /// construction context, what the bytes hold as [`Self::holding`] tells it, and what that
    /// history is; unchanged with neither a held component nor a finding.
    #[must_use]
    pub fn recall(&self, state: &mut Value, records: &[Value], instructions: String) -> String {
        let shown: Vec<Value> = self.offered.iter().map(basis).collect();
        let found: Vec<Value> = (records.iter())
            .filter(|record| {
                let no_fit = &record["construction"]["no_fit"];
                record["choice"] == "no_fit"
                    && no_fit["alternative_stands"] == true
                    && no_fit["offered"].as_array() == Some(&shown)
            })
            .map(|record| {
                json!({"question": record["question"], "clause": record["clause"]["text"],
                    "choice": "no_fit", "basis": record["construction"]["no_fit"]["offered"]})
            })
            .collect();
        let held = self.holding_in(&state["authoring"]);
        let context = match (hold(state, &held), found.is_empty()) {
            (false, true) => return instructions,
            (true, true) => return format!("{instructions} {CONSTRUCTION} {HOLDING}"),
            (true, false) => format!("{CONSTRUCTION} {HOLDING}"),
            (false, false) => CONSTRUCTION.to_owned(),
        };
        let bytes = state["candidate_nika"].as_str().map(sha256);
        state["history"] = json!({"candidate_sha256": bytes,
            "catalogue": state["authoring"]["catalogue"], "construction": found});
        format!("{instructions} {context} {HISTORY}")
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
        if let Some(k) = place(key, "held-") {
            let held = self.offered.get(k).filter(|row| Self::holds(row));
            return held.map(|_| Construed::Held);
        }
        let k = place(key, "component-")?;
        let row = self.offered.get(k).filter(|row| Self::open(row))?;
        Some(Construed::Defect(noted(row)))
    }

    /// The basis of a construction answer on its question's record: for `no_fit`, every offered
    /// component with what the bytes hold of it and whether the alternative stands; for a named
    /// component, its identity, under `named` when the bytes lack it and `held` when they hold
    /// it. Any other answer leaves the record as it is.
    pub fn annotate(&self, record: Option<&mut Value>, answer: Option<&str>) {
        let (Some(record), Some(answer)) = (record, answer) else {
            return;
        };
        match self.read(answer) {
            Some(Construed::Defect(_)) => {
                let row = place(answer, "component-").and_then(|k| self.offered.get(k));
                record["construction"] = json!({"named": row.map(basis)});
            }
            Some(Construed::Held) => {
                let row = place(answer, "held-").and_then(|k| self.offered.get(k));
                record["construction"] = json!({"held": row.map(basis)});
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

/// What a construction answer was given over: an offer, and what the judged bytes hold of it.
fn basis(row: &Value) -> Value {
    json!({"component": row["component"], "construction": row["construction"]})
}

/// The place in the offer an answer `key` names after `prefix` (`component-<k>`, `held-<k>`).
fn place(key: &str, prefix: &str) -> Option<usize> {
    key.strip_prefix(prefix)?.parse().ok()
}

/// What the judged bytes hold (`held`, [`Construction::holding_in`]), on `state` as
/// `construction`, bound to those bytes' sha256 and the lent catalogue: whether they hold any.
fn hold(state: &mut Value, held: &[Value]) -> bool {
    if held.is_empty() {
        return false;
    }
    let bytes = state["candidate_nika"].as_str().map(sha256);
    state["construction"] = json!({"candidate_sha256": bytes,
        "catalogue": state["authoring"]["catalogue"], "held": held});
    true
}

/// What the receipt that holds `row` binds, as the door recorded it on the judged bytes
/// (`authoring.composed`, [`lent`](super::lent)): the receipt of that component in the lent
/// release witnessed as the row states; null when no such receipt states its bindings.
fn bindings(authoring: &Value, row: &Value) -> Value {
    let lent = &authoring["catalogue"];
    (authoring["composed"].as_array().into_iter().flatten())
        .find(|seen| {
            seen["component"] == row["component"]["id"]
                && seen["verdict"] == row["construction"]["held"]
                && seen["release"]["version"] == lent["version"]
                && seen["release"]["snapshot_sha256"] == lent["snapshot_sha256"]
        })
        .map_or(Value::Null, |seen| seen["bindings"].clone())
}

/// A component the bytes hold as a localization offers it: `held-<k>`, by identity, title,
/// witness and what each of its holes is bound to.
fn held_option(held: &Value) -> ChoiceOption {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    let bound: Vec<String> = (held["bindings"].as_array().into_iter().flatten())
        .map(|binding| format!("{} = {}", text(&binding["path"]), binding["bound"]))
        .collect();
    let bound = if bound.is_empty() {
        String::new()
    } else {
        format!("; bound: {}", bound.join(", "))
    };
    let described = format!(
        "`{}` (release {}) · {}: these bytes hold it as admitted ({}{bound})",
        text(&held["component"]["id"]),
        text(&held["component"]["version"]),
        text(&held["title"]),
        text(&held["witness"]),
    );
    ChoiceOption::new(format!("held-{}", held["offer"]), described)
}

/// Whether a receipt's witness verdict holds its component as admitted.
pub(super) fn admitted(verdict: &Value) -> bool {
    verdict
        .as_str()
        .is_some_and(|verdict| HELD.contains(&verdict))
}

/// What a receipt binds, as a question is shown it: each hole's path and its bound literal.
pub(super) fn bound(receipt: &Value) -> Value {
    let rows: Vec<Value> = (receipt["bindings"].as_array().into_iter().flatten())
        .map(|binding| json!({"path": binding["path"], "bound": binding["bound"]}))
        .collect();
    Value::Array(rows)
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
