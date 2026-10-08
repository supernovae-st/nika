// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document revision's operations over a complete base: literal edits that keep every other
//! byte, refusals that apply nothing, a component composed and rebound through its receipt, a
//! whole replacement that claims no preservation, and the record a saved file keeps.

use crate::foundry::component::pinned;
use crate::foundry::{Component, ComponentCatalog, ComponentRef, Hole, Release, Unresolved};
use nika_compile::surface::sha256;
use serde_json::{Value, json};

use super::{Applied, apply, carried, components, nodes, record};

/// A rich base the pack ships: typed input with a default, failure and unwind edges, a `when`
/// guard, a timeout, comments and outputs.
fn rich() -> &'static str {
    nika_pack::example("12-failure-routing").expect("the pack ships 12-failure-routing")
}

/// The lines of `revised` that differ from `base`, as `(base line, revised line)`, when both
/// hold the same number of lines.
fn changed_lines(base: &str, revised: &str) -> Vec<(String, String)> {
    let (a, b): (Vec<&str>, Vec<&str>) = (base.lines().collect(), revised.lines().collect());
    assert_eq!(a.len(), b.len(), "a literal edit adds or removes no line");
    (a.iter().zip(&b))
        .filter(|(x, y)| x != y)
        .map(|(x, y)| ((*x).to_owned(), (*y).to_owned()))
        .collect()
}

#[test]
fn set_edits_one_literal_each_and_every_other_byte_stays() {
    let base = rich();
    let operations = [
        json!({"op": "set", "path": "/inputs/drill/default", "value": true}),
        // The text form a strict structured-output dialect carries.
        json!({"op": "set", "path": "tasks.release_lock.timeout", "value_json": "\"30s\"",
            "component": "", "version": "", "bindings_json": ""}),
    ];
    let applied = apply(base, (&operations, None), None, &[]).expect("both apply");
    assert!(!applied.replaced);
    assert_eq!(
        applied.changed,
        ["inputs.drill.default", "tasks.release_lock.timeout"]
    );
    assert_eq!(
        changed_lines(base, &applied.source),
        [
            (
                "    default: false".to_owned(),
                "    default: true".to_owned()
            ),
            (
                "    timeout: \"10s\"".to_owned(),
                "    timeout: \"30s\"".to_owned()
            ),
        ]
    );
    assert!(
        applied
            .source
            .contains("# Cleanup is a TASK on an `unwind` edge"),
        "comments are kept"
    );
    let wf = nika_compile::parse(&applied.source).expect("the strict parser reads it");
    assert_eq!(
        wf.tasks.len(),
        nika_compile::parse(base).expect("base").tasks.len()
    );
}

#[test]
fn a_refused_operation_applies_nothing_and_names_why() {
    let base = rich();
    let operations = [
        json!({"op": "set", "path": "/inputs/drill/default", "value": true}),
        json!({"op": "set", "path": "/const/absent", "value": 1}),
        json!({"op": "delete", "path": "/tasks/deploy"}),
        json!({"op": "set", "path": "/inputs/drill/default", "value_json": "not json"}),
        json!({"op": "set", "path": "/inputs/drill/default", "value": true, "extra": 1}),
        json!({"op": "compose", "component": {"id": "block:anything"}}),
    ];
    let why = apply(base, (&operations, None), None, &[]).expect_err("refused");
    assert_eq!(why.len(), 5, "{why:?}");
    assert!(
        why[0].starts_with("operation 1: `set const.absent`"),
        "{why:?}"
    );
    assert!(why[1].contains("`delete` is not an operation"), "{why:?}");
    assert!(
        why[2].contains("`value_json` is not one JSON value"),
        "{why:?}"
    );
    assert!(why[3].contains("`extra` is not a field"), "{why:?}");
    assert!(
        why[4].contains("no component catalogue was lent"),
        "{why:?}"
    );
    assert!(
        apply(base, (&[], None), None, &[]).is_err(),
        "nothing stated"
    );
}

/// A filter-then-report component: records older than a threshold, counted and written.
const STALE: &str = r#"nika: p90-stale-filter-report
model: mock/echo
const:
  records_path: ./data/tickets.json
  report_path: ./out/stale.json
  max_age_hours: { type: integer, value: 48 }
permits:
  fs: { read: ["./data/tickets.json"], write: ["./out/stale.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks:
  read_records:
    invoke: { tool: "nika:read", args: { path: "${{ const.records_path }}" } }
  parse_records:
    with: { raw: "${{ tasks.read_records.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.raw }}", expression: "fromjson" } }
  stale:
    with: { rows: "${{ tasks.parse_records.output }}", hours: "${{ const.max_age_hours }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: { rows: "${{ with.rows }}", hours: "${{ with.hours }}" }
        expression: "(.hours | tonumber) as $h | [.rows[] | select(.age_hours > $h)]"
  report_text:
    with: { stale: "${{ tasks.stale.output }}" }
    invoke: { tool: "nika:jq", args: { input: { stale: "${{ with.stale }}" }, expression: "{count: (.stale | length), ids: [.stale[].id]} | tojson" } }
  write_report:
    with: { content: "${{ tasks.report_text.output }}" }
    invoke: { tool: "nika:write", args: { path: "${{ const.report_path }}", content: "${{ with.content }}" } }
outputs:
  stale: ${{ tasks.stale.output }}
"#;

/// The person's document: its own name and boundary over its own files, nothing else yet.
const PARENT: &str = r#"nika: stale-tickets-report
# The person's own boundary: the files the request names.
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks: {}
"#;

const VERSION: &str = "fixture-document-r1";
const SNAPSHOT: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn release() -> Release {
    Release::new(VERSION, SNAPSHOT, "nika-knowledge-release-profile/r1")
}

/// A catalogue of one release holding the stale-filter component.
struct Shelf;

impl ComponentCatalog for Shelf {
    fn release(&self) -> Release {
        release()
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        reference.block_name()?;
        pinned(reference, &self.release())?;
        if reference.id != "block:stale-filter-report" {
            return Err(Unresolved::Unknown(reference.id.clone()));
        }
        let mut component = Component::new(
            "block:stale-filter-report",
            release(),
            "blocks/stale-filter-report.nika",
            sha256(STALE),
            STALE,
        );
        component.holes = vec![
            Hole::new("const.records_path", "human", None),
            Hole::new("const.report_path", "human", None),
            Hole::new("const.max_age_hours", "human", None),
        ];
        Ok(component)
    }
    fn entries(&self) -> Vec<Value> {
        vec![
            json!({"id": "block:stale-filter-report", "title": "Stale records report",
            "purpose": "count the records older than a threshold and write a report",
            "holes": [{"name": "const.max_age_hours", "owner": "human"}]}),
        ]
    }
}

fn compose(hours: i64) -> Value {
    json!({"op": "compose",
        "component": {"id": "block:stale-filter-report", "version": VERSION},
        "bindings": {"const.records_path": "./in/tickets.json",
            "const.report_path": "./out/report.json", "const.max_age_hours": hours}})
}

fn composed() -> Applied {
    apply(PARENT, (&[compose(48)], None), Some(&Shelf), &[]).expect("composed")
}

#[test]
fn compose_expands_an_admitted_component_with_its_receipt() {
    let applied = composed();
    assert_eq!(applied.changed, ["component block:stale-filter-report"]);
    let [receipt] = applied.receipts.as_slice() else {
        panic!("one receipt: {:?}", applied.receipts);
    };
    assert_eq!(receipt["component"]["id"], "block:stale-filter-report");
    assert_eq!(receipt["component"]["release"]["version"], VERSION);
    assert_eq!(receipt["candidate_sha256"], sha256(&applied.source));
    let witnessed = crate::foundry::witness::witness(receipt, &applied.source);
    assert_eq!(witnessed["verdict"], "expanded", "{witnessed}");
    // Every line of the person's document stays, in order, but the empty `tasks: {}` the
    // component's tasks now fill: the merge inserts, it never rewrites what was there.
    let mut lines = applied.source.lines();
    for kept in PARENT.lines().filter(|line| *line != "tasks: {}") {
        assert!(
            lines.any(|line| line == kept),
            "`{kept}` is kept in order:\n{}",
            applied.source
        );
    }
    assert!(
        applied
            .source
            .contains("max_age_hours: { type: integer, value: 48 }")
    );
    assert!(
        !applied.source.contains("p90-stale-filter-report"),
        "no name inherited"
    );
    // An unknown component, a later release and an open hole are refused, never guessed.
    let unknown = json!({"op": "compose", "component": {"id": "block:absent"}, "bindings": {}});
    let later = json!({"op": "compose",
        "component": {"id": "block:stale-filter-report", "version": "fixture-document-r2"},
        "bindings": {}});
    let open = json!({"op": "compose", "component": {"id": "block:stale-filter-report"},
        "bindings": {"const.max_age_hours": 48}});
    for refused in [unknown, later, open] {
        assert!(
            apply(
                PARENT,
                (std::slice::from_ref(&refused), None),
                Some(&Shelf),
                &[]
            )
            .is_err(),
            "{refused}"
        );
    }
}

#[test]
fn rebind_changes_the_bound_value_and_carries_the_receipt() {
    let first = composed();
    let rebind = json!({"op": "rebind", "component": "block:stale-filter-report",
        "bindings_json": "{\"const.max_age_hours\": 72}"});
    let second = apply(
        &first.source,
        (&[rebind], None),
        Some(&Shelf),
        &first.receipts,
    )
    .expect("rebound");
    assert_eq!(
        changed_lines(&first.source, &second.source),
        [(
            "  max_age_hours: { type: integer, value: 48 }".to_owned(),
            "  max_age_hours: { type: integer, value: 72 }".to_owned()
        )]
    );
    let [receipt] = second.receipts.as_slice() else {
        panic!("the one receipt, carried: {:?}", second.receipts);
    };
    assert_eq!(receipt["revises"], sha256(&first.source));
    assert_eq!(receipt["candidate_sha256"], sha256(&second.source));
    let bound = (receipt["bindings"].as_array().into_iter().flatten())
        .find(|row| row["path"] == "const.max_age_hours")
        .expect("the bound hole");
    assert_eq!(bound["bound"], 72);
    let witnessed = crate::foundry::witness::witness(receipt, &second.source);
    assert_eq!(witnessed["verdict"], "expanded", "{witnessed}");
    // A rebind with no receipt to revise is refused.
    let lone = json!({"op": "rebind", "component": {"id": "block:stale-filter-report"},
        "bindings": {"const.max_age_hours": 72}});
    assert!(apply(&first.source, (&[lone], None), Some(&Shelf), &[]).is_err());
}

#[test]
fn a_replacement_must_parse_and_claims_no_preservation() {
    let base = rich();
    assert!(apply(base, (&[], Some("nika: [")), None, &[]).is_err());
    let both = [json!({"op": "set", "path": "/inputs/drill/default", "value": true})];
    assert!(apply(base, (&both, Some(PARENT)), None, &[]).is_err());
    let replaced = apply(base, (&[], Some(PARENT)), None, &[]).expect("replaced");
    assert!(replaced.replaced);
    assert_eq!(replaced.source, PARENT);
}

#[test]
fn the_record_binds_the_revised_bytes_and_carries_the_receipts() {
    let applied = composed();
    let record = record(
        (PARENT, &applied.source),
        "Report the stale tickets\nChange: older than 48 hours",
        "intent-sha",
        &applied,
    );
    assert!(nika_compile_fidelity::sketch::kept::binds(
        &record,
        &applied.source
    ));
    assert!(!nika_compile_fidelity::sketch::kept::binds(&record, PARENT));
    assert_eq!(
        nika_compile_fidelity::sketch::kept::original(&record),
        Some("Report the stale tickets\nChange: older than 48 hours")
    );
    assert_eq!(record["document_revision"]["mode"], "operations");
    assert_eq!(carried(Some(&record)), applied.receipts);
    assert!(carried(None).is_empty());
    // A created document's settled record carries the receipts of what its creation composed.
    let created = json!({"document": {"components": applied.receipts}});
    assert_eq!(carried(Some(&created)), applied.receipts);
}

#[test]
fn the_seat_reads_every_literal_node_and_the_lent_components() {
    let listed = nodes(rich()).expect("readable");
    let rows = listed.as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r["path"] == "/inputs/drill/default" && r["value"] == false),
        "{listed}"
    );
    assert!(
        rows.iter()
            .any(|r| r["path"] == "/tasks/release_lock/timeout" && r["value"] == "10s"),
        "{listed}"
    );
    assert!(nodes("nika: [").is_none());
    let offered = components(Some(&Shelf));
    assert_eq!(
        offered[0]["component"],
        json!({"id": "block:stale-filter-report", "version": VERSION})
    );
    assert_eq!(components(None), json!([]));
}

/// A stock report whose filter is a multi-line jq expression in a `|` block scalar, the usual
/// form of a long expression (the live threshold correction stopped here).
const BLOCK: &str = r#"nika: stock-alerts
# Items under their threshold are reported.
permits:
  fs: { read: ["./stock.json"], write: ["./out/alerts.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks:
  read_stock:
    invoke: { tool: "nika:read", args: { path: "./stock.json" } }
  evaluate:
    with: { rows: "${{ tasks.read_stock.output }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: "${{ with.rows }}"
        expression: |
          fromjson
          | map(select(.stock < .threshold))
  write_alerts:
    with: { alerts: "${{ tasks.evaluate.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/alerts.json", content: "${{ with.alerts }}" } }
"#;

#[test]
fn a_block_scalar_expression_is_set_in_place_and_proven() {
    nika_compile::parse(BLOCK).expect("the base parses");
    let set = json!({"op": "set", "path": "/tasks/evaluate/invoke/args/expression",
        "value_json": "\"fromjson\\n| map(select(.stock <= .threshold))\\n\"",
        "component": "", "version": "", "bindings_json": "", "key": "", "text": "", "to": ""});
    let applied = apply(BLOCK, (&[set], None), None, &[]).expect("the editor sets it");
    assert_eq!(
        changed_lines(BLOCK, &applied.source),
        [(
            "          | map(select(.stock < .threshold))".to_owned(),
            "          | map(select(.stock <= .threshold))".to_owned()
        )],
        "only the filter line changes, the block scalar kept"
    );
    assert_eq!(applied.changed, ["tasks.evaluate.invoke.args.expression"]);
    assert_eq!((applied.verified, applied.constructed), (1, 0));
    let revised = record((BLOCK, &applied.source), "the request", "intent", &applied);
    assert!(
        (revised["document_revision"]["preservation"].as_str())
            .is_some_and(|claim| claim.starts_with("verified")),
        "{revised}"
    );
}

#[test]
fn a_typed_constant_named_whole_is_set_at_its_value() {
    let base = "nika: window\nconst:\n  max_age_hours: { type: integer, value: 48 } # hours\ntasks:\n  t:\n    exec:\n      command: [\"echo\", \"${{ const.max_age_hours }}\"]\n";
    let set = json!({"op": "set", "path": "/const/max_age_hours", "value": 72});
    let applied = apply(base, (&[set], None), None, &[]).expect("set at its value");
    assert_eq!(
        applied.source,
        base.replace("value: 48 }", "value: 72 }"),
        "the type and the comment stay"
    );
    assert_eq!(applied.changed, ["const.max_age_hours.value"]);
}

#[test]
fn a_task_renamed_takes_its_references_and_nothing_else() {
    let base = "nika: chain\ntasks:\n  first:\n    invoke: { tool: \"nika:log\", args: { message: \"first of all\" } }\n  second:\n    with: { prior: \"${{ tasks.first.output }}\" }\n    invoke: { tool: \"nika:log\", args: { message: \"${{ with.prior }}\" } }\n";
    let rename = json!({"op": "rename", "path": "tasks.first", "to": "opening"});
    let applied = apply(base, (&[rename], None), None, &[]).expect("renamed");
    assert!(
        applied.source.contains("  opening:\n"),
        "{}",
        applied.source
    );
    assert!(applied.source.contains("${{ tasks.opening.output }}"));
    assert!(
        applied.source.contains("message: \"first of all\""),
        "text that only spells the name is kept"
    );
    assert_eq!(applied.verified, 1);
}

#[test]
fn a_field_of_another_operation_is_refused_never_dropped() {
    let set = json!({"op": "set", "path": "/tasks/first", "value": 1, "to": "x"});
    let why = apply("nika: x\ntasks: {}\n", (&[set], None), None, &[]).expect_err("refused");
    assert!(why[0].contains("carries no `to`"), "{why:?}");
}

/// A value given directly is the value even when it is null, an empty string or an empty list;
/// a missing value, or an empty text form of one, states none and is refused.
#[test]
fn a_direct_null_or_empty_value_is_a_value_and_a_missing_one_is_refused() {
    let base = "nika: values\nconst:\n  note: \"keep\"\n  tags: [\"a\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:log\", args: { message: \"${{ const.note }}\" } }\n";
    for (value, line) in [
        (json!(null), "  note: null"),
        (json!(""), "  note: \"\""),
        (json!([]), "  note: []"),
    ] {
        let set = json!({"op": "set", "path": "/const/note", "value": value});
        let applied = apply(base, (&[set], None), None, &[]).expect("a direct value is set");
        assert!(
            applied.source.contains(&format!("{line}\n")),
            "{value}: {}",
            applied.source
        );
        assert_eq!(applied.verified, 1);
    }
    let push = json!({"op": "push", "path": "/const/tags", "value": ""});
    let pushed = apply(base, (&[push], None), None, &[]).expect("an empty string is pushed");
    assert!(
        pushed.source.contains("tags: [\"a\", \"\"]"),
        "{}",
        pushed.source
    );
    for missing in [
        json!({"op": "set", "path": "/const/note"}),
        json!({"op": "set", "path": "/const/note", "value_json": ""}),
    ] {
        let why =
            apply(base, (std::slice::from_ref(&missing), None), None, &[]).expect_err("no value");
        assert!(why[0].contains("states no `value`"), "{missing}: {why:?}");
    }
    let extra = json!({"op": "remove", "path": "/const/note", "value": null});
    let why = apply(base, (&[extra], None), None, &[]).expect_err("a value on a remove");
    assert!(why[0].contains("carries no `value`"), "{why:?}");
}
