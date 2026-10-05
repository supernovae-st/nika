// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The laws a semantic revision is held to (R4 F), pure over the recorded graph and fills and
//! the reader's obligation ledgers.
//!
//! [`preserved`]: a revision keeps the base graph's structure exactly: the same tasks in the same
//! order, each with its verb and tool, its reads, writes and hosts (effects), its data and
//! control edges, its gate and its limits, and the same named outputs; a gate's own words stay.
//! It may change only the fills of other tasks, which it names.
//!
//! [`delta`]: what the revision supersedes and adds is stated explicitly (`stated`:
//! `{"supersedes": [{"replaces", "by"}], "adds": [<change clause>]}`). Each link names a clause of the
//! original request (the evidence of a base duty, stated once in the original words) and a clause
//! of the change (the evidence of a change duty, stated verbatim in the change). A kind shared by
//! two duties never links them. A prohibition, an effect or a gate is never superseded here, a
//! link to an absent, repeated or ambiguous clause is refused, and a program change without a
//! link or an addition, or a link without a program change, is refused. Every other base duty
//! is kept.
//!
//! [`additions`]: every clause of the change is either the `by` of one link or stated in `adds`;
//! one that is neither is refused, so an unlinked replacement never passes as an addition (a
//! concatenation is not a supersession). A stated addition is consumed like a supersession:
//! [`resolved`] reads it beside the replacements, so no added duty is only recorded. An added
//! effect or gate is refused: the revision keeps the base graph's structure, so no fill carries it.

use serde_json::{Value, json};

/// The fields of a sketch task a revision keeps exactly, each with what it holds.
const KEPT: [(&str, &str); 13] = [
    ("id", "identity"),
    ("verb", "verb"),
    ("tool", "tool"),
    ("reads", "effects"),
    ("writes", "effects"),
    ("hosts", "effects"),
    ("with", "data edges"),
    ("after", "control edges"),
    ("for_each", "control edges"),
    ("gated_by", "gate"),
    ("max_turns", "limits"),
    ("tools", "limits"),
    ("fail_fast", "limits"),
];

/// The duty kinds a revision never supersedes: the program's structure carries them.
const STRUCTURAL: [&str; 2] = ["effect", "gate"];

/// The tasks whose fills `revised` changes against `base` (each `{"sketch", "fills"}` as a
/// semantic record holds them), when every structural part is kept; otherwise why not, each part
/// named by what it holds.
///
/// # Errors
/// Each structural part the revision changes.
pub fn preserved(base: &Value, revised: &Value) -> Result<Vec<String>, Vec<String>> {
    let mut why = Vec::new();
    let tasks = |record: &Value| {
        record["sketch"]["tasks"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let (before, after) = (tasks(base), tasks(revised));
    let ids = |list: &[Value]| -> Vec<Value> { list.iter().map(|t| t["id"].clone()).collect() };
    if ids(&before) != ids(&after) {
        why.push("a task was added, removed or reordered".to_owned());
        return Err(why);
    }
    for (old, new) in before.iter().zip(&after) {
        let id = old["id"].as_str().unwrap_or_default();
        for (field, holds) in KEPT {
            if old.get(field) != new.get(field) {
                why.push(format!("`{id}` changes its {holds} (`{field}`)"));
            }
        }
    }
    if base["sketch"].get("outputs") != revised["sketch"].get("outputs") {
        why.push("the named outputs change".to_owned());
    }
    let gates: Vec<&str> = (before.iter())
        .filter(|t| t["tool"] == "nika:prompt")
        .filter_map(|t| t["id"].as_str())
        .collect();
    let mut changed = Vec::new();
    for task in before.iter().filter_map(|t| t["id"].as_str()) {
        if fills_of(base, task) == fills_of(revised, task) {
            continue;
        }
        if gates.contains(&task) {
            why.push(format!("the gate `{task}` changes its words"));
        } else {
            changed.push(task.to_owned());
        }
    }
    if why.is_empty() {
        Ok(changed)
    } else {
        Err(why)
    }
}

/// One task's fills, sorted by field.
fn fills_of(record: &Value, task: &str) -> Vec<(String, Value)> {
    let mut fills: Vec<(String, Value)> = (record["fills"].as_array().into_iter().flatten())
        .filter(|fill| fill["task"] == task)
        .map(|fill| {
            (
                fill["field"].as_str().unwrap_or_default().to_owned(),
                fill["value"].clone(),
            )
        })
        .collect();
    fills.sort_by(|a, b| a.0.cmp(&b.0));
    fills
}

/// The explicit obligation delta of a revision: `stated` (`{"supersedes": [{"replaces", "by"}],
/// "adds": [<change clause>]}`) checked against the original request's duties
/// (`original_ledger`, read from `original`) and the change's (`change_ledger`, read from
/// `change`), and `changed`, the tasks the program revises; `resolved` is the request the
/// revision consumed ([`resolved`]). `{"original", "change", "resolved", "links", "adds",
/// "superseded": [{"evidence", "duties"}], "added", "kept"}`; or why not.
///
/// # Errors
/// Each link or consistency law the revision breaks.
pub fn delta(
    original: &str,
    original_ledger: &Value,
    change: &str,
    change_ledger: &Value,
    resolved: &str,
    stated: &Value,
    changed: &[String],
) -> Result<Value, Vec<String>> {
    let duties = |ledger: &Value| ledger.as_array().cloned().unwrap_or_default();
    let (base, asked) = (duties(original_ledger), duties(change_ledger));
    let links = stated["supersedes"].as_array().cloned().unwrap_or_default();
    let adds = stated_adds(stated);
    let mut why = Vec::new();
    let (mut replaced, mut by_clauses, mut superseded) = (Vec::new(), Vec::new(), Vec::new());
    for link in &links {
        let (Some(old), Some(new)) = (link["replaces"].as_str(), link["by"].as_str()) else {
            why.push("a link names no original clause or no change clause".to_owned());
            continue;
        };
        why.extend(link_laws(original, &base, change, &asked, old, new));
        if replaced.contains(&old) || by_clauses.contains(&new) {
            why.push(format!("« {old} » or « {new} » is linked twice"));
        }
        replaced.push(old);
        by_clauses.push(new);
        let of: Vec<Value> = base
            .iter()
            .filter(|d| d["evidence"] == old)
            .cloned()
            .collect();
        superseded.push(json!({"evidence": old, "duties": of}));
    }
    if changed.is_empty() && !links.is_empty() {
        why.push("a superseded clause is stated but the program does not change".to_owned());
    } else if !changed.is_empty() && links.is_empty() && adds.is_empty() {
        why.push("the program changes but no superseded or added clause is stated".to_owned());
    }
    if let Err(unstated) = additions(change_ledger, stated) {
        why.extend(unstated);
    }
    let mut seen = Vec::new();
    why.retain(|reason| {
        let first = !seen.contains(reason);
        seen.push(reason.clone());
        first
    });
    let added: Vec<Value> = (asked.iter())
        .filter(|d| adds.iter().any(|clause| d["evidence"] == *clause))
        .cloned()
        .collect();
    if !why.is_empty() {
        return Err(why);
    }
    let kept: Vec<Value> = (base.iter())
        .filter(|d| !replaced.iter().any(|old| d["evidence"] == *old))
        .cloned()
        .collect();
    Ok(
        json!({"original": original, "change": change, "resolved": resolved, "links": links,
        "adds": adds, "superseded": superseded, "added": added, "kept": kept}),
    )
}

/// The request a revision is read, filled and judged as (the contract it consumes): `original`
/// with each linked clause replaced by its change clause, then each of the change's `added`
/// clauses ([`additions`]), in order, as a sentence of its own. Every replaced clause is found
/// once in `original` itself, never in text another link produced; no two may overlap or nest,
/// and the replacements are applied together, so the result never depends on the links' order
/// and a refusal rewrites nothing. Every other word stays, so the unchanged obligations are read
/// as before; a superseded clause is gone from it, and an added one is read beside them.
///
/// # Errors
/// A link whose original clause is not stated exactly once in `original`, that names no clause
/// on either side, or whose clause overlaps another link's.
pub fn resolved(original: &str, links: &Value, added: &[String]) -> Result<String, Vec<String>> {
    let mut spans: Vec<(usize, usize, &str)> = Vec::new();
    let mut why = Vec::new();
    for link in links.as_array().into_iter().flatten() {
        let named = |key: &str| link[key].as_str().filter(|text| !text.is_empty());
        let (Some(old), Some(new)) = (named("replaces"), named("by")) else {
            why.push("a link names no original clause or no change clause".to_owned());
            continue;
        };
        let mut found = original.match_indices(old);
        match (found.next(), found.next()) {
            (Some((at, _)), None) => spans.push((at, at + old.len(), new)),
            _ => why.push(format!(
                "« {old} » is not stated exactly once in the request it revises"
            )),
        }
    }
    spans.sort_by_key(|&(start, _, _)| start);
    for pair in spans.windows(2) {
        if let [(a, a_end, _), (b, b_end, _)] = pair
            && b < a_end
        {
            let (first, second) = (&original[*a..*a_end], &original[*b..*b_end]);
            why.push(format!(
                "« {first} » and « {second} » overlap in the request it revises"
            ));
        }
    }
    if !why.is_empty() {
        return Err(why);
    }
    let mut text = original.to_owned();
    for (start, end, new) in spans.iter().rev() {
        text.replace_range(*start..*end, new);
    }
    for clause in added {
        text.truncate(text.trim_end().len());
        if !text.is_empty() && !text.ends_with(['.', '!', '?']) {
            text.push('.');
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(clause);
        if !clause.ends_with(['.', '!', '?']) {
            text.push('.');
        }
    }
    Ok(text)
}

/// The clauses `stated` adds (its `adds`), as written.
fn stated_adds(stated: &Value) -> Vec<String> {
    (stated["adds"].as_array().into_iter().flatten())
        .filter_map(|clause| clause.as_str().map(str::to_owned))
        .collect()
}

/// The change's additions, in the change's order: each clause of `change_ledger` that `stated`
/// names in `adds`. Every clause of the change is either the `by` of one link or an addition; a
/// stated addition is a clause of the change and never also a `by`. A prohibition is an addition
/// like any duty; an effect or a gate is not: the revision keeps the base graph's structure, so
/// no fill could carry it.
///
/// # Errors
/// Each link whose `by` the change does not state, each change clause neither linked nor added,
/// each addition the change does not state or that is also linked, and each added clause of a
/// kind the program's structure carries.
pub fn additions(change_ledger: &Value, stated: &Value) -> Result<Vec<String>, Vec<String>> {
    let accounted = accounted(change_ledger, stated)?;
    let why: Vec<String> = (accounted.iter())
        .filter_map(|(clause, kind)| {
            kind.map(|kind| {
                format!(
                    "the change adds « {clause} », an {kind} the program's structure carries: not revised here"
                )
            })
        })
        .collect();
    if why.is_empty() {
        Ok(accounted.into_iter().map(|(clause, _)| clause).collect())
    } else {
        Err(why)
    }
}

/// The change's additions ([`additions`]) each with the structural kind it carries (`effect`,
/// `gate`), if any: the accounting law alone, leaving to the route what it does with a
/// structural addition (a source revision adds a destination; a semantic one refuses it).
///
/// # Errors
/// Each change clause neither linked nor added, and each addition the change does not state or
/// that is also linked.
pub fn accounted(
    change_ledger: &Value,
    stated: &Value,
) -> Result<Vec<(String, Option<&'static str>)>, Vec<String>> {
    let by: Vec<&str> = (stated["supersedes"].as_array().into_iter().flatten())
        .filter_map(|link| link["by"].as_str())
        .collect();
    let adds = stated_adds(stated);
    let of_change = clauses(change_ledger);
    let mut why = Vec::new();
    for new in &by {
        if !of_change.iter().any(|c| c == *new) {
            why.push(format!("« {new} » is not a clause the change states"));
        }
    }
    for clause in &adds {
        if !of_change.iter().any(|c| c == clause.as_str()) {
            why.push(format!(
                "« {clause} » is added but is not a clause the change states"
            ));
        } else if by.contains(&clause.as_str()) {
            why.push(format!("« {clause} » is both linked and added"));
        }
    }
    let mut added = Vec::new();
    for clause in of_change.iter().filter_map(Value::as_str) {
        if by.contains(&clause) {
            continue;
        }
        if !adds.iter().any(|a| a == clause) {
            why.push(format!(
                "the change states « {clause} », but the revision neither supersedes a clause with it nor adds it"
            ));
            continue;
        }
        let structural = (change_ledger.as_array().into_iter().flatten())
            .filter(|d| d["evidence"] == clause && d["state"] != "refused")
            .find_map(|d| {
                let kind = d["kind"].as_str()?;
                STRUCTURAL.iter().copied().find(|s| *s == kind)
            });
        added.push((clause.to_owned(), structural));
    }
    if why.is_empty() { Ok(added) } else { Err(why) }
}

/// The reader's clauses of a ledger: each duty's evidence, once, in order (what a seat may name
/// as `replaces` or `by`).
#[must_use]
pub fn clauses(ledger: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for evidence in (ledger.as_array().into_iter().flatten()).map(|d| d["evidence"].clone()) {
        if !out.contains(&evidence) {
            out.push(evidence);
        }
    }
    out
}

/// The schema of a revision's links: `{"supersedes": [{"replaces", "by"}], "adds", "notes"}`,
/// and, for a destination the change adds, the optional `like`: the destination the base writes
/// whose write the new one copies.
#[must_use]
pub fn links_schema() -> Value {
    json!({"type": "object", "additionalProperties": false,
        "required": ["supersedes", "adds", "notes"], "properties": {"notes": {"type": "string"},
        "adds": {"type": "array", "items": {"type": "string"}},
        "like": {"type": "string"},
        "supersedes": {"type": "array", "items": {"type": "object", "additionalProperties": false,
            "required": ["replaces", "by"],
            "properties": {"replaces": {"type": "string"}, "by": {"type": "string"}}}}}})
}

/// Why one link `old` → `new` does not hold.
fn link_laws(
    original: &str,
    base: &[Value],
    change: &str,
    asked: &[Value],
    old: &str,
    new: &str,
) -> Vec<String> {
    let mut why = Vec::new();
    let targets: Vec<&Value> = base.iter().filter(|d| d["evidence"] == old).collect();
    if targets.is_empty() || old.is_empty() {
        why.push(format!("« {old} » is not a clause of the original request"));
    } else if original.matches(old).count() != 1 {
        why.push(format!(
            "« {old} » is ambiguous: the original request states it more than once"
        ));
    }
    if targets.iter().any(|d| d["state"] == "refused") {
        why.push(format!(
            "« {old} » is a prohibition: a revision never supersedes it"
        ));
    } else if let Some(kind) = (targets.iter())
        .filter_map(|d| d["kind"].as_str())
        .find(|kind| STRUCTURAL.contains(kind))
    {
        why.push(format!(
            "« {old} » is an {kind} the program's structure carries: not revised here"
        ));
    }
    let stated = asked.iter().filter(|d| d["evidence"] == new).count();
    if new.is_empty() || !change.contains(new) || stated == 0 {
        why.push(format!("« {new} » is not a clause the change states"));
    }
    why
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "Keep the orders whose status is paid. Keep the orders whose total is above 10. Never delete any file.";
    const CHANGE: &str = "Keep the orders whose status is shipped instead of paid.";

    fn duty(evidence: &str, kind: &str, state: &str) -> Value {
        json!({"evidence": evidence, "kind": kind, "state": state, "note": null, "realized_by": null})
    }

    /// Two filters of the same kind and a prohibition, as the reader records them.
    fn original_ledger() -> Value {
        json!([
            duty(
                "Keep the orders whose status is paid",
                "transformation",
                "unresolved"
            ),
            duty(
                "Keep the orders whose status is paid",
                "filter",
                "unresolved"
            ),
            duty(
                "Keep the orders whose total is above 10",
                "filter",
                "unresolved"
            ),
            duty("Never delete any file", "effect", "refused"),
        ])
    }

    fn change_ledger() -> Value {
        json!([duty(
            "Keep the orders whose status is shipped instead of paid",
            "format",
            "unresolved"
        )])
    }

    fn link(old: &str, new: &str) -> Value {
        json!([{"replaces": old, "by": new}])
    }

    /// What a seat states: its links and its additions.
    fn stated(links: &Value, adds: &[&str]) -> Value {
        json!({"supersedes": links, "adds": adds})
    }

    fn judged(links: &Value, changed: &[&str]) -> Result<Value, Vec<String>> {
        let changed: Vec<String> = changed.iter().map(|s| (*s).to_owned()).collect();
        delta(
            ORIGINAL,
            &original_ledger(),
            CHANGE,
            &change_ledger(),
            "",
            &stated(links, &[]),
            &changed,
        )
    }

    const NEW: &str = "Keep the orders whose status is shipped instead of paid";

    #[test]
    fn an_explicit_link_supersedes_one_clause_and_keeps_its_same_kind_sibling() {
        let out = judged(
            &link("Keep the orders whose status is paid", NEW),
            &["keep"],
        )
        .unwrap();
        let superseded = out["superseded"].as_array().unwrap();
        assert_eq!(superseded.len(), 1, "one clause, whatever its duty count");
        assert_eq!(superseded[0]["duties"].as_array().unwrap().len(), 2);
        let kept: Vec<&str> = (out["kept"].as_array().unwrap().iter())
            .filter_map(|d| d["evidence"].as_str())
            .collect();
        assert_eq!(
            kept,
            [
                "Keep the orders whose total is above 10",
                "Never delete any file"
            ]
        );
        assert_eq!(
            out["added"],
            json!([]),
            "the linked change clause is no addition"
        );
    }

    #[test]
    fn a_link_to_an_absent_ambiguous_prohibited_or_unstated_clause_is_refused() {
        let refused = |links: Value, needle: &str| {
            let why = judged(&links, &["keep"]).unwrap_err();
            assert!(why.iter().any(|w| w.contains(needle)), "{needle}: {why:?}");
        };
        refused(
            link("Keep the paid orders", NEW),
            "not a clause of the original request",
        );
        refused(link("Never delete any file", NEW), "a prohibition");
        refused(
            link("Keep the orders whose status is paid", "ship them"),
            "not a clause the change states",
        );
        let twice = "Keep the orders whose status is paid. Keep the orders whose status is paid.";
        let why = delta(
            twice,
            &original_ledger(),
            CHANGE,
            &change_ledger(),
            "",
            &stated(&link("Keep the orders whose status is paid", NEW), &[]),
            &["keep".to_owned()],
        )
        .unwrap_err();
        assert!(why.iter().any(|w| w.contains("ambiguous")), "{why:?}");
        let both = json!([{"replaces": "Keep the orders whose status is paid", "by": NEW},
                          {"replaces": "Keep the orders whose total is above 10", "by": NEW}]);
        refused(both, "linked twice");
        let effect = json!([duty(
            "write them to ./out/paid.json",
            "effect",
            "unresolved"
        )]);
        let why = delta(
            "write them to ./out/paid.json",
            &effect,
            CHANGE,
            &change_ledger(),
            "",
            &stated(&link("write them to ./out/paid.json", NEW), &[]),
            &["keep".to_owned()],
        )
        .unwrap_err();
        assert!(
            why.iter().any(|w| w.contains("structure carries")),
            "{why:?}"
        );
    }

    #[test]
    fn a_change_needs_its_link_and_a_link_needs_its_change() {
        let why = judged(&json!([]), &["keep"]).unwrap_err();
        assert!(why[0].contains("no superseded or added clause"), "{why:?}");
        // The change's clause, unlinked and unstated, never passes as an addition.
        assert!(
            why.iter()
                .any(|w| w.contains("neither supersedes a clause with it nor adds it")),
            "{why:?}"
        );
        let why = judged(&link("Keep the orders whose status is paid", NEW), &[]).unwrap_err();
        assert!(why[0].contains("does not change"), "{why:?}");
        let why = judged(&json!([]), &[]).unwrap_err();
        assert!(
            why.iter().any(|w| w.contains("neither supersedes")),
            "a change clause is always accounted for: {why:?}"
        );
    }

    /// A change that supersedes one clause and adds another (the reader's ledger of each).
    const BOTH: &str = "Keep the orders whose status is shipped instead of paid. Keep only the orders whose currency is EUR.";
    const EUR: &str = "Keep only the orders whose currency is EUR";

    fn both_ledger() -> Value {
        json!([
            duty(NEW, "format", "unresolved"),
            duty(EUR, "filter", "unresolved")
        ])
    }

    #[test]
    fn a_stated_addition_is_consumed_and_recorded_beside_the_supersession() {
        let links = link("Keep the orders whose status is paid", NEW);
        let said = stated(&links, &[EUR]);
        assert_eq!(additions(&both_ledger(), &said).unwrap(), [EUR]);
        let read = resolved(ORIGINAL, &links, &[EUR.to_owned()]).unwrap();
        assert_eq!(
            read,
            format!(
                "Keep the orders whose status is shipped instead of paid. Keep the orders whose total is above 10. Never delete any file. {EUR}."
            )
        );
        let out = delta(
            ORIGINAL,
            &original_ledger(),
            BOTH,
            &both_ledger(),
            &read,
            &said,
            &["keep".to_owned()],
        )
        .unwrap();
        let added: Vec<&str> = (out["added"].as_array().unwrap().iter())
            .filter_map(|d| d["evidence"].as_str())
            .collect();
        assert_eq!(added, [EUR]);
        assert_eq!(out["adds"], json!([EUR]));
    }

    #[test]
    fn an_unstated_misstated_doubled_or_structural_addition_is_refused() {
        let links = link("Keep the orders whose status is paid", NEW);
        let refused = |said: Value, ledger: Value, needle: &str| {
            let why = additions(&ledger, &said).unwrap_err();
            assert!(why.iter().any(|w| w.contains(needle)), "{needle}: {why:?}");
        };
        refused(
            stated(&links, &[]),
            both_ledger(),
            "neither supersedes a clause with it nor adds it",
        );
        refused(
            stated(&links, &["Keep the cheap orders"]),
            both_ledger(),
            "is not a clause the change states",
        );
        refused(
            stated(&links, &[EUR, NEW]),
            both_ledger(),
            "both linked and added",
        );
        let write = "write the kept orders to ./out/eur.json";
        let effect = json!([
            duty(NEW, "format", "unresolved"),
            duty(write, "effect", "unresolved")
        ]);
        refused(stated(&links, &[write]), effect, "structure carries");
        // An added prohibition is a duty like any other, never structure.
        let never = "Never touch ./out/eur.json";
        let prohibition = json!([
            duty(NEW, "format", "unresolved"),
            duty(never, "effect", "refused")
        ]);
        assert_eq!(
            additions(&prohibition, &stated(&links, &[never])).unwrap(),
            [never]
        );
    }

    #[test]
    fn the_resolved_request_replaces_only_the_linked_clause_in_place() {
        let links = link("Keep the orders whose status is paid", NEW);
        assert_eq!(
            resolved(ORIGINAL, &links, &[]).unwrap(),
            "Keep the orders whose status is shipped instead of paid. Keep the orders whose total is above 10. Never delete any file."
        );
        assert_eq!(resolved(ORIGINAL, &json!([]), &[]).unwrap(), ORIGINAL);
        let absent = resolved(ORIGINAL, &link("Keep the paid orders", NEW), &[]).unwrap_err();
        assert!(absent[0].contains("not stated exactly once"), "{absent:?}");
        let twice = format!("{ORIGINAL} Keep the orders whose status is paid.");
        assert!(
            resolved(
                &twice,
                &link("Keep the orders whose status is paid", NEW),
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn a_swap_of_two_clauses_is_the_same_whatever_the_links_order() {
        let original = "Keep the paid orders. Keep the open tickets.";
        let forward = json!([{"replaces": "Keep the paid orders", "by": "Keep the open tickets"},
                             {"replaces": "Keep the open tickets", "by": "Keep the paid orders"}]);
        let backward = json!([forward[1].clone(), forward[0].clone()]);
        let swapped = "Keep the open tickets. Keep the paid orders.";
        assert_eq!(resolved(original, &forward, &[]).unwrap(), swapped);
        assert_eq!(resolved(original, &backward, &[]).unwrap(), swapped);
    }

    #[test]
    fn overlapping_or_nested_clauses_are_refused_without_any_rewrite() {
        let original = "Keep the orders whose status is paid.";
        for links in [
            json!([{"replaces": "orders whose status", "by": "a"}, {"replaces": "status is paid", "by": "b"}]),
            json!([{"replaces": "Keep the orders", "by": "a"}, {"replaces": "the orders", "by": "b"}]),
        ] {
            let why = resolved(original, &links, &[]).unwrap_err();
            assert!(why.iter().any(|w| w.contains("overlap")), "{why:?}");
        }
    }

    fn record(status: &str, gate: &str) -> Value {
        json!({"sketch": {"tasks": [
            {"id": "read", "verb": "invoke", "tool": "nika:read", "reads": ["./a.json"]},
            {"id": "keep", "verb": "invoke", "tool": "nika:jq", "with": [{"name": "d", "from": "read"}]},
            {"id": "approve", "verb": "invoke", "tool": "nika:prompt", "with": [{"name": "k", "from": "keep"}]},
            {"id": "save", "verb": "invoke", "tool": "nika:write", "writes": ["./out.json"],
             "with": [{"name": "k", "from": "keep"}], "gated_by": "approve"}],
            "outputs": [{"name": "kept", "from": "save"}]},
            "fills": [{"task": "keep", "field": "expression", "value": format!("select(.status == \"{status}\")")},
                      {"task": "approve", "field": "args.message", "value": gate}]})
    }

    #[test]
    fn a_revision_changes_only_named_fills_and_keeps_every_structural_part() {
        let base = record("paid", "Write?");
        assert_eq!(
            preserved(&base, &record("shipped", "Write?")).unwrap(),
            ["keep"]
        );
        assert_eq!(preserved(&base, &base).unwrap(), Vec::<String>::new());
        let mutate = |pointer: &str, value: Value| {
            let mut revised = record("shipped", "Write?");
            *revised.pointer_mut(pointer).unwrap() = value;
            preserved(&base, &revised).unwrap_err()
        };
        assert!(mutate("/sketch/tasks/3/gated_by", Value::Null)[0].contains("gate"));
        assert!(mutate("/sketch/tasks/3/writes", json!(["./other.json"]))[0].contains("effects"));
        assert!(mutate("/sketch/tasks/1/with", json!([]))[0].contains("data edges"));
        assert!(mutate("/sketch/outputs", json!([]))[0].contains("named outputs"));
        assert!(
            mutate("/sketch/tasks/0/id", json!("load"))[0].contains("added, removed or reordered")
        );
        let why = preserved(&base, &record("paid", "Delete everything?")).unwrap_err();
        assert!(
            why[0].contains("the gate `approve` changes its words"),
            "{why:?}"
        );
    }
}
