// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_compile::{CompileStatus, DiagnosticKind};
use nika_compile_reader::{lexicon, shape};
use serde_json::{Value, json};

use super::{Witness, check, mentions, normalized, runs};

const TOP: &str = "Read ./rows.json, keep the top 2 rows by points and write them to ./top.json";
const TOP_RULE: &str = "keep the top 2 rows by points";

/// A candidate whose `admit` task asserts, with `message`, the output of a check task.
const CHECKED: &str = r#"nika: top
permits:
  tools: ["nika:jq", "nika:assert"]
tasks:
  ranked:
    invoke: { tool: "nika:jq", args: { input: "[]", expression: "length == 2" } }
  admit:
    with: { ok: "${{ tasks.ranked.output }}" }
    invoke: { tool: "nika:assert", args: { condition: "${{ with.ok }}", message: "MESSAGE" } }
"#;

fn checked(message: &str) -> String {
    CHECKED.replace("MESSAGE", message)
}

/// The kinds and carriers of every property `intent` states over `candidate`.
fn carried(intent: &str, candidate: &str) -> Vec<(&'static str, Option<String>)> {
    let witness = Witness::of(intent, candidate);
    (witness.properties().iter())
        .map(|p| (p.kind, p.carrier.clone()))
        .collect()
}

/// A task asserting `condition`, bound `with` (none for null) and skipped by a `when:` or not.
fn asserted(condition: &Value, with: &Value, when: bool) -> Value {
    let mut task = json!({"invoke": {"tool": "nika:assert",
        "args": {"condition": condition, "message": "the order"}}});
    if !with.is_null() {
        task["with"] = with.clone();
    }
    if when {
        task["when"] = json!("${{ inputs.go }}");
    }
    task
}

#[test]
fn a_stated_computation_names_its_typed_properties_in_its_own_words() {
    let required = Witness::required(TOP);
    let named: Vec<(&str, &str)> = (required.iter())
        .map(|p| (p.kind, p.evidence.as_str()))
        .collect();
    assert_eq!(named, [("order", TOP_RULE), ("limit", TOP_RULE)]);
    assert!(required.iter().all(|p| p.carrier.is_none()));
    for (intent, kind) in [
        (
            "Read ./tickets.json, count the rows where status is open and write the count to ./open.json",
            "count",
        ),
        (
            "Read ./sales.csv, compute the total of the amount column per client and write it to ./totals.json",
            "total",
        ),
        (
            "Read ./tickets.json, sort the rows by amount and write them to ./sorted.json",
            "order",
        ),
    ] {
        let required = Witness::required(intent);
        let kinds: Vec<&str> = required.iter().map(|p| p.kind).collect();
        assert_eq!(kinds, [kind], "{intent}: {required:?}");
        assert!(
            intent.contains(&required[0].evidence),
            "{intent}: {required:?}"
        );
    }
    assert!(Witness::required("Write the text hello to ./out/result.txt.").is_empty());
}

/// A bound on produced text is no property a check carries: what a model writes is never
/// asserted, and the room runs no model step, so the bound stays the judge's.
#[test]
fn a_bound_on_produced_text_states_no_property() {
    for intent in [
        "Read ./notes.md and write ./out/report.md: the report must be exactly 5 lines.",
        "Read ./notes/brief.md and write a 3-bullet summary of under 150 words to ./out/summary.md.",
    ] {
        let required = Witness::required(intent);
        assert!(required.is_empty(), "{intent}: {required:?}");
    }
    // The reader keeps each of these bounds as a constraint of the plan; none becomes a property.
    for intent in [
        "Read ./notes/brief.md and write a summary to ./out/summary.md. The summary is under 150 words.",
        "Read ./notes.md and write ./out/report.md. The report has exactly 5 lines.",
        "Read ./notes.md and write ./out/report.md. Keep the tone formal.",
        "Lis ./notes.md et écris un résumé dans ./out/resume.md. Le résumé doit faire moins de 150 mots.",
    ] {
        let reading = lexicon::read(&lexicon::fold_apostrophes(intent));
        assert!(
            !reading.plan.constraints.is_empty(),
            "{intent}: the reader keeps the bound"
        );
        let required = Witness::required(intent);
        assert!(required.is_empty(), "{intent}: {required:?}");
    }
}

#[test]
fn a_check_reads_what_a_task_made_and_is_never_skipped() {
    let made = json!({"ok": "${{ tasks.ranked.output }}"});
    for (condition, with) in [
        (json!("${{ with.ok }}"), made.clone()),
        (json!(" ${{ with.ok == true }} "), made.clone()),
    ] {
        let task = asserted(&condition, &with, false);
        assert_eq!(check(&task), Some("the order"), "{task}");
    }
    // A task reads another's output only through `with:`: the direct form is refused by the
    // language before any check, so it carries nothing.
    for (condition, with, when) in [
        (json!("${{ tasks.ranked.output }}"), Value::Null, false),
        (json!("${{ with.ok }}"), made.clone(), true),
        (json!(true), Value::Null, false),
        (json!("${{ true }}"), Value::Null, false),
        (json!("${{ const.cap > 0 }}"), Value::Null, false),
        (json!("${{ inputs.ok }}"), Value::Null, false),
        (
            json!("${{ with.ok }}"),
            json!({"ok": "${{ const.ok }}"}),
            false,
        ),
        (json!("${{ with.okay }}"), made.clone(), false),
        (json!("tasks.ranked.output"), Value::Null, false),
    ] {
        let task = asserted(&condition, &with, when);
        assert_eq!(check(&task), None, "{task}");
    }
    let jq = json!({"invoke": {"tool": "nika:jq",
        "args": {"condition": "${{ tasks.ranked.output }}", "message": "the order"}}});
    assert_eq!(check(&jq), None);
    assert!(mentions("with.ok && x", "with.ok") && !mentions("with.okay", "with.ok"));
}

#[test]
fn an_assert_quoting_the_words_carries_every_property_they_state() {
    let both = [
        ("order", Some("admit".to_owned())),
        ("limit", Some("admit".to_owned())),
    ];
    let none = [("order", None), ("limit", None)];
    for (message, expected) in [
        ("the rows are not « keep the top 2 rows by points »", &both),
        ("not   KEEP THE TOP 2 rows BY points", &both),
        ("the rows are not the top 2 rows by points", &none),
        ("", &none),
    ] {
        assert_eq!(
            carried(TOP, &checked(message)),
            expected.as_slice(),
            "{message}"
        );
    }
    assert_eq!(normalized(" Keep  the\ttop "), "keep the top");
}

#[test]
fn the_requests_own_typed_rule_carries_its_properties_and_nothing_else_does() {
    let folded = lexicon::fold_apostrophes(TOP);
    let mut reading = lexicon::read(&folded);
    shape::promote_stated_rules(&mut reading.plan, &folded);
    let lowered = reading.plan.rules[0].jq();
    let candidate = |expression: &str| {
        let quoted = serde_json::to_string(expression).expect("a JSON string");
        format!(
            "nika: top\ntasks:\n  rank:\n    invoke: {{ tool: \"nika:jq\", args: {{ input: \"[]\", expression: {quoted} }} }}\n"
        )
    };
    assert_eq!(
        carried(TOP, &candidate(&lowered)),
        [
            ("order", Some("rank".to_owned())),
            ("limit", Some("rank".to_owned()))
        ]
    );
    let handwritten = carried(TOP, &candidate("sort_by(.points) | reverse | .[:2]"));
    assert_eq!(handwritten, [("order", None), ("limit", None)]);
    let task = json!({"invoke": {"tool": "nika:jq", "args": {"expression": lowered}}});
    assert!(runs(&task, &lowered) && !runs(&task, "."));
}

#[test]
fn an_uncarried_property_holds_the_candidate_with_its_remedy_and_drops_its_record() {
    let mut out = nika_compile::initial();
    nika_compile::finish(checked("no quotation"), &mut out);
    out.provenance.plan = Some(json!({"strategy": "native"}));
    let ready = out.status;
    let witness = Witness::of(TOP, &checked("no quotation"));
    let defect = witness.defect().expect("two uncarried properties");
    assert_eq!(defect.kind, "witness");
    let stated: Value = serde_json::from_str(&defect.message).expect("one JSON object");
    assert_eq!(stated["kind"], "unwitnessed");
    assert_eq!(
        stated["properties"],
        json!([{"kind": "order", "evidence": TOP_RULE}, {"kind": "limit", "evidence": TOP_RULE}])
    );
    assert!(witness.hold(&mut out));
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert_eq!(
        out.candidate.as_deref(),
        Some(checked("no quotation").as_str())
    );
    assert_eq!(out.provenance.plan, None);
    let named: Vec<&str> = (out.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification" && d.kind == DiagnosticKind::Unknown)
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(named.len(), 2, "{named:?}");
    assert!(
        named.iter().all(|said| said.contains(TOP_RULE)),
        "{named:?}"
    );
    let recorded = &out.provenance.decision.as_ref().expect("a decision")["witness"];
    assert_eq!(recorded, &witness.to_json());
    // Every property carried: nothing is named and the outcome is untouched.
    let quoting = Witness::of(TOP, &checked("not « keep the top 2 rows by points »"));
    assert!(quoting.defect().is_none());
    let mut kept = nika_compile::initial();
    nika_compile::finish(checked("x"), &mut kept);
    assert!(!quoting.hold(&mut kept));
    assert_eq!(kept.status, ready);
}
