// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Season 2 of the compiler war room: the obligation ledger and its READY law, the trigger
//! requirement beside the candidate, contradictory and verified bounds, one approval one
//! gate, the item over the request's own material, the effect repeated per item, the
//! duplicated destination, context and structure duties, the located set and the ranking
//! that asks its count.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, TriggerKind, TriggerStatus,
    outcome_document,
};
use serde_json::{Value, json};

/// The answer-round door the CLI control uses: a trusted recorded plan replayed for its
/// intent, zero provider calls.
fn replay(intent: &str, record: &Value, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent).with_plan(record.clone());
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    nika_compile::compile(&request).unwrap()
}

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

fn document(out: &CompileOutcome) -> Value {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.check_preview.as_ref().unwrap().report.is_clean(),
        "{out:#?}"
    );
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

fn label(out: &CompileOutcome, key: &str) -> String {
    match out.questions.iter().find(|q| q.key == key) {
        Some(question) => question.label.clone(),
        None => panic!("no question {key}: {out:#?}"),
    }
}

fn tasks(doc: &Value) -> &serde_json::Map<String, Value> {
    doc["tasks"].as_object().unwrap()
}

const MODEL: (&str, &str) = ("model", r#""mock/echo""#);
const RULE: (&str, &str) = (
    "const.rule_expression",
    r#"".records | map(select((.amount | tonumber) > 100))""#,
);
const CHAPTERS: &str = "For each of the four files ./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md, at most 2 at a time, write a two-sentence summary. Then merge the summaries in exactly that order into ./out/digest.md, with one heading per file named after the file.";
const CHAPTER_FILES: [&str; 4] = [
    "./chapters/01-intro.md",
    "./chapters/02-method.md",
    "./chapters/03-results.md",
    "./chapters/04-limits.md",
];
fn chapters_record() -> Value {
    let mut bindings: Vec<Value> = CHAPTER_FILES
        .iter()
        .map(|p| json!({"role": "path", "literal": p}))
        .collect();
    bindings.push(json!({"role": "path", "literal": "./out/digest.md"}));
    json!({"operations":[
        {"op":"read","detail":CHAPTER_FILES.join(" ; "),"evidence":"./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md","categories":[]},
        {"op":"draft","detail":"a two-sentence summary","evidence":"For each of the four files ./chapters/01-intro.md, ./chapters/02-method.md, ./chapters/03-results.md and ./chapters/04-limits.md, at most 2 at a time, write a two-sentence summary","categories":[]}],
      "effects":[{"verb":"write","target":"./out/digest.md","policy":"automatic","evidence":"merge the summaries in exactly that order into ./out/digest.md, with one heading per file named after the file","policy_literal":null}],
      "obligations":[],"bindings":bindings,
      "constraints":["at most 2 at a time","in exactly that order","with one heading per file named after the file"],
      "unknowns":[],"trigger":"For each of the four files","strategy":"cold"})
}

// ── the obligation ledger: every stated duty is carried, or nothing is READY ───────
// The ledger is the typed truth a product projects: one duty per stated demand with its
// kind, its state and the element carrying it. A READY candidate has no unresolved duty;
// a constraint no step can carry ends INCOMPLETE naming it, never a green run on a dropped
// instruction.
#[test]
fn the_ledger_names_the_carrier_of_every_stated_duty_and_refuses_a_silent_one() {
    let out = replay(CHAPTERS, &chapters_record(), &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let record = out.provenance.decision.clone().unwrap();
    let ledger = record["ledger"].as_array().unwrap();
    let duties: Vec<(&str, &str, &str)> = ledger
        .iter()
        .map(|d| {
            (
                d["kind"].as_str().unwrap(),
                d["state"].as_str().unwrap(),
                d["realized_by"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(
        duties,
        [
            ("transformation", "realized", "draft"),
            ("effect", "realized", "write_output"),
            ("format", "realized", "for_each"),
            ("identity", "realized", "draft_fold"),
            ("identity", "realized", "draft_fold"),
            ("cardinality", "realized", "draft"),
        ],
        "{record:#}"
    );
    assert!(
        ledger.iter().all(|d| d["state"] != "unresolved"),
        "{record:#}"
    );
    // The plan record stays the replayable identity of the plan: no ledger inside it.
    assert!(
        out.provenance
            .plan
            .as_ref()
            .unwrap()
            .get("ledger")
            .is_none()
    );
    // A gated write carries its gate; a measurable bound is verified at run by a law.
    let out = replay(HEADLINE, &headline_record(None), &[MODEL]);
    let record = out.provenance.decision.clone().unwrap();
    assert_eq!(record["ledger"][0]["realized_by"], "draft", "{record:#}");
    let mut gated = headline_record(None);
    gated["effects"][0]["policy"] = json!("human_first");
    gated["constraints"] = json!(["a single line, under 90 characters"]);
    let out = replay(HEADLINE, &gated, &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let record = out.provenance.decision.clone().unwrap();
    let by_kind: Vec<(&str, &str, &str)> = record["ledger"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["kind"].as_str().unwrap(),
                d["realized_by"].as_str().unwrap_or("-"),
                d["note"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert!(
        by_kind.contains(&("gate", "write_output_review", "-")),
        "{record:#}"
    );
    assert!(
        by_kind.contains(&("cardinality", "draft_bounds", "verified at run")),
        "{record:#}"
    );
    // A constraint with no step to carry it: INCOMPLETE naming the instruction, no candidate.
    let intent = "Read ./draft.md and write it to ./final.md in a warm tone.";
    let record = json!({"operations":[
        {"op":"read","detail":"./draft.md","evidence":"Read ./draft.md","categories":[]}],
      "effects":[{"verb":"write","target":"./final.md","policy":"automatic","evidence":"write it to ./final.md","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":["in a warm tone"],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown
                && d.target == "format"
                && d.message.contains("in a warm tone")
                && d.message.contains("silent obligation")),
        "{out:#?}"
    );
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    assert_eq!(ledger[0]["kind"], "effect", "{ledger:#}");
    assert_eq!(ledger[0]["state"], "realized");
    assert_eq!(ledger[1]["kind"], "format");
    assert_eq!(ledger[1]["state"], "unresolved");
    // Metamorphic: the same request without the tone instruction is READY.
    let mut plain = record;
    plain["constraints"] = json!([]);
    let out = replay("Read ./draft.md and write it to ./final.md.", &plain, &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}

// ── a cadence or an event is a requirement beside the candidate, never a dropped clause ─
// nika#1720: the portable program bytes carry no cadence, hook id or secret; the outcome
// states the trigger requirement next to the candidate, the ledger records the clause as
// carried by that requirement, and a sequencing head ("once …") states none.
const MORNING: &str = "Every morning, read ./tickets.json, draft a short digest of the open tickets and write it to ./out/digest.md.";
fn morning_record(trigger: &str) -> Value {
    json!({"operations":[
        {"op":"read","detail":"./tickets.json","evidence":"read ./tickets.json","categories":[]},
        {"op":"draft","detail":"a short digest of the open tickets","evidence":"draft a short digest of the open tickets","categories":[]}],
      "effects":[{"verb":"write","target":"./out/digest.md","policy":"automatic","evidence":"write it to ./out/digest.md","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./tickets.json"},{"role":"path","literal":"./out/digest.md"}],
      "constraints":[],"unknowns":[],"trigger":trigger,"strategy":"cold"})
}

#[test]
fn a_cadence_or_an_event_is_a_trigger_requirement_beside_the_candidate() {
    let out = replay(MORNING, &morning_record("Every morning"), &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let requirement = out.requested_trigger.as_ref().expect("a cadence is stated");
    assert_eq!(requirement.kind, TriggerKind::Schedule);
    assert_eq!(requirement.status, TriggerStatus::RequiresBinding);
    assert_eq!(requirement.source_hint.as_deref(), Some("Every morning"));
    assert!(requirement.payload_input.is_none(), "{requirement:#?}");
    let doc = document(&out);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    assert!(
        !doc.to_string().to_lowercase().contains("morning"),
        "the bytes carry no cadence: {doc:#}"
    );
    let wire = outcome_document(&out);
    assert_eq!(wire["requested_trigger"]["kind"], "schedule", "{wire:#}");
    assert_eq!(wire["requested_trigger"]["status"], "requires_binding");
    assert_eq!(wire["requested_trigger"]["source_hint"], "Every morning");
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    assert!(
        ledger
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "trigger"
                && d["state"] == "realized"
                && d["realized_by"] == "requested_trigger"),
        "{ledger:#}"
    );
    // An outside event, in another language, with no material of its own: the item is the
    // payload the trigger supplies.
    let intent =
        "Dès qu'un ticket arrive, rédige un accusé de réception et écris-le dans ./out/accuse.md.";
    let record = json!({"operations":[
        {"op":"draft","detail":"un accusé de réception","evidence":"rédige un accusé de réception","categories":[]}],
      "effects":[{"verb":"write","target":"./out/accuse.md","policy":"automatic","evidence":"écris-le dans ./out/accuse.md","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":"Dès qu'un ticket arrive","strategy":"cold"});
    let out = replay(intent, &record, &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let requirement = out.requested_trigger.as_ref().expect("an event is stated");
    assert_eq!(requirement.kind, TriggerKind::Event);
    assert_eq!(requirement.payload_input.as_deref(), Some("item"));
    // A sequencing head and a plain request state no requirement.
    let out = replay(
        HEADLINE,
        &headline_record(Some("once the brief is read")),
        &[MODEL],
    );
    assert!(out.requested_trigger.is_none(), "{out:#?}");
    assert!(outcome_document(&out)["requested_trigger"].is_null());
    let out = replay(
        MORNING,
        &morning_record("Every morning").tap_trigger_null(),
        &[MODEL],
    );
    assert!(out.requested_trigger.is_none(), "{out:#?}");
}

trait TapTriggerNull {
    fn tap_trigger_null(self) -> Self;
}
impl TapTriggerNull for Value {
    fn tap_trigger_null(mut self) -> Self {
        self["trigger"] = Value::Null;
        self
    }
}

// ── contradictory bounds on the produced content are refused, never run ──────────
#[test]
fn contradictory_bounds_on_the_produced_content_are_refused_never_run() {
    let intent = "Read ./notes.md and write ./out/report.md: the report must be exactly 5 lines and at least 12 lines long, both are mandatory.";
    let record = |constraints: Value| {
        json!({"operations":[
            {"op":"read","detail":"./notes.md","evidence":"Read ./notes.md","categories":[]},
            {"op":"draft","detail":"the report","evidence":"write ./out/report.md: the report","categories":[]}],
          "effects":[{"verb":"write","target":"./out/report.md","policy":"automatic","evidence":"write ./out/report.md","policy_literal":null}],
          "obligations":[],"bindings":[],"constraints":constraints,"unknowns":[],"trigger":null,"strategy":"cold"})
    };
    let out = replay(
        intent,
        &record(json!(["exactly 5 lines", "at least 12 lines long"])),
        &[MODEL],
    );
    assert_eq!(out.status, CompileStatus::Refused, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::RequiresHuman
                && d.message.contains("exactly 5 lines")
                && d.message.contains("at least 12 lines long")),
        "{out:#?}"
    );
    assert!(keys(&out).contains(&"intent.clarification"));
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    let contradicted = ledger
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"] == "cardinality" && d["state"] == "contradicted")
        .count();
    assert_eq!(contradicted, 2, "{ledger:#}");
    // Compatible bounds are carried by the draft's prompt and the request is READY.
    let out = replay(
        intent,
        &record(json!(["at least 3 lines", "5 lines"])),
        &[MODEL],
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}

// ── a stated bound on the drafted text is verified at run, not only prompted ──────
#[test]
fn a_stated_bound_on_the_drafted_text_is_verified_at_run_not_only_prompted() {
    let intent = "Read ./notes/brief.md and write a 3-bullet summary of under 150 words to ./out/summary.md.";
    let record = json!({"operations":[
        {"op":"read","detail":"./notes/brief.md","evidence":"Read ./notes/brief.md","categories":[]},
        {"op":"draft","detail":"a 3-bullet summary of under 150 words","evidence":"write a 3-bullet summary of under 150 words","categories":[]}],
      "effects":[{"verb":"write","target":"./out/summary.md","policy":"automatic","evidence":"write a 3-bullet summary of under 150 words to ./out/summary.md","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":["3 bullets", "under 150 words", "in a warm tone"],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[MODEL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    let bounds = &tasks(&doc)["draft_bounds"];
    assert_eq!(bounds["invoke"]["tool"], "nika:jq", "{doc:#}");
    assert_eq!(bounds["with"]["body"], "${{ tasks.draft.output.body }}");
    let expression = bounds["invoke"]["args"]["expression"].as_str().unwrap();
    assert!(
        expression.contains("== 3") && expression.contains("< 150"),
        "{expression}"
    );
    assert!(
        expression.contains(r#"select(test("^\\s*([-*•]|[0-9]+[.)])\\s+"))"#),
        "{expression}"
    );
    assert!(expression.contains(r#"scan("\\S+")"#), "{expression}");
    assert_eq!(
        bounds["after"],
        json!({"draft_admit": "success"}),
        "{bounds:#}"
    );
    let admit = &tasks(&doc)["draft_bounds_admit"];
    assert_eq!(admit["invoke"]["tool"], "nika:assert");
    assert!(
        admit["invoke"]["args"]["message"]
            .as_str()
            .unwrap()
            .contains("3 bullets; under 150 words"),
        "{admit:#}"
    );
    assert_eq!(
        tasks(&doc)["write_output"]["after"],
        json!({"draft_bounds_admit": "success"}),
        "the write waits for the verified bounds: {doc:#}"
    );
    // The prompt still carries the tone; the bounds are realized by the law, not the prompt.
    let prompt = tasks(&doc)["draft"]["infer"]["prompt"].as_str().unwrap();
    assert!(prompt.contains("in a warm tone"), "{prompt}");
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    let cardinality: Vec<(&str, &str)> = ledger
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"] == "cardinality")
        .map(|d| {
            (
                d["realized_by"].as_str().unwrap(),
                d["note"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        cardinality,
        [
            ("draft_bounds", "verified at run"),
            ("draft_bounds", "verified at run")
        ],
        "{ledger:#}"
    );
    // No measurable bound: no law, no extra task, the write follows the draft's admit.
    let mut plain = record;
    plain["constraints"] = json!(["in a warm tone"]);
    let doc = document(&replay(intent, &plain, &[MODEL]));
    assert!(tasks(&doc).get("draft_bounds").is_none(), "{doc:#}");
    assert_eq!(
        tasks(&doc)["write_output"]["after"],
        json!({"draft_admit": "success"})
    );
}

// ── one approval clause covering several effects is one gate ─────────────────────
// The sealed release seed asked for one confirmation before a POST and a write; the
// assembler emitted one prompt per effect and the runner's single resume could not finish.
#[test]
fn one_approval_covering_several_effects_is_one_gate_two_approvals_are_two() {
    let intent = "Read ./draft.md, then ask me to confirm before you POST it to http://127.0.0.1:18471/hooks/x. Only after I say yes: do the POST, then write it to ./out/sent.md.";
    let record = json!({"operations":[
        {"op":"read","detail":"./draft.md","evidence":"Read ./draft.md","categories":[]}],
      "effects":[
        {"verb":"send","target":"POST it to http://127.0.0.1:18471/hooks/x","policy":"human_first","evidence":"ask me to confirm before you POST it to http://127.0.0.1:18471/hooks/x","policy_literal":null},
        {"verb":"write","target":"./out/sent.md","policy":"human_first","evidence":"write it to ./out/sent.md","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./draft.md"},{"role":"url","literal":"http://127.0.0.1:18471/hooks/x"},{"role":"path","literal":"./out/sent.md"}],
      "constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    let prompts: Vec<&String> = tasks(&doc)
        .iter()
        .filter(|(_, node)| node["invoke"]["tool"] == "nika:prompt")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(prompts, ["approval_review"], "{doc:#}");
    let message = tasks(&doc)["approval_review"]["invoke"]["args"]["message"]
        .as_str()
        .unwrap();
    assert!(
        message.contains("1) write ./out/sent.md") && message.contains("2) send · POST it to"),
        "{message}"
    );
    for task in ["write_output", "send"] {
        assert_eq!(
            tasks(&doc)[task]["when"],
            "${{ with.approved == true }}",
            "{task}: {doc:#}"
        );
        assert_eq!(
            tasks(&doc)[task]["with"]["approved"],
            "${{ tasks.approval_review.output }}",
            "{task}: {doc:#}"
        );
    }
    let ledger = out.provenance.decision.clone().unwrap()["ledger"].clone();
    let gates: Vec<&str> = ledger
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"] == "gate")
        .map(|d| d["realized_by"].as_str().unwrap())
        .collect();
    assert_eq!(gates, ["approval_review", "approval_review"], "{ledger:#}");
    // Two approvals, one per effect: two gates, each before its own effect.
    let intent = "Read ./draft.md. Ask me before writing it to ./out/a.md. Ask me again before sending it to http://127.0.0.1:18471/hooks/x.";
    let record = json!({"operations":[
        {"op":"read","detail":"./draft.md","evidence":"Read ./draft.md","categories":[]}],
      "effects":[
        {"verb":"write","target":"./out/a.md","policy":"human_first","evidence":"Ask me before writing it to ./out/a.md","policy_literal":null},
        {"verb":"send","target":"sending it to http://127.0.0.1:18471/hooks/x","policy":"human_first","evidence":"Ask me again before sending it to http://127.0.0.1:18471/hooks/x","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./draft.md"},{"role":"path","literal":"./out/a.md"},{"role":"url","literal":"http://127.0.0.1:18471/hooks/x"}],
      "constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    let mut prompts: Vec<&String> = tasks(&doc)
        .iter()
        .filter(|(_, node)| node["invoke"]["tool"] == "nika:prompt")
        .map(|(id, _)| id)
        .collect();
    prompts.sort();
    assert_eq!(prompts, ["send_review", "write_output_review"], "{doc:#}");
}

// ── a trigger over the request's own material never declares an item ────────────
// Two sealed seeds compiled READY and died at run time on a missing `inputs.item`: "once
// all three are done" (a sequencing trigger over a read brief) and "pour chaque ligne de
// niveau critique" (a per-row trigger over a read CSV) both declared an input no run could
// supply. The item is the material of an invocation only when the request supplies none.
const HEADLINE: &str = "Take the product brief in ./brief.md and write a formal headline (a single line, under 90 characters) to ./out/headline.txt. Once the brief is read, nothing else runs.";
fn headline_record(trigger: Option<&str>) -> Value {
    json!({"operations":[
        {"op":"read","detail":"./brief.md","evidence":"Take the product brief in ./brief.md","categories":[]},
        {"op":"draft","detail":"a formal headline (a single line, under 90 characters)","evidence":"write a formal headline (a single line, under 90 characters) to ./out/headline.txt","categories":[]}],
      "effects":[{"verb":"write","target":"./out/headline.txt","policy":"automatic","evidence":"write a formal headline (a single line, under 90 characters) to ./out/headline.txt","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":trigger,"strategy":"cold"})
}

#[test]
fn a_trigger_over_the_request_s_own_material_never_declares_an_item() {
    let sequenced = replay(
        HEADLINE,
        &headline_record(Some("once the brief is read")),
        &[MODEL],
    );
    assert_eq!(sequenced.status, CompileStatus::Ready, "{sequenced:#?}");
    let doc = document(&sequenced);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    let prompt = tasks(&doc)["draft"]["infer"]["prompt"].as_str().unwrap();
    assert!(!prompt.contains("inputs.item"), "{prompt}");
    // Metamorphic pair: the sequencing trigger changes nothing in the emitted source.
    let plain = replay(HEADLINE, &headline_record(None), &[MODEL]);
    assert_eq!(sequenced.candidate, plain.candidate);
    // No material of its own: the item IS the material, as before.
    let intent = "For each incoming brief, write a formal headline to ./out/headline.txt.";
    let record = json!({"operations":[
        {"op":"draft","detail":"a formal headline","evidence":"write a formal headline to ./out/headline.txt","categories":[]}],
      "effects":[{"verb":"write","target":"./out/headline.txt","policy":"automatic","evidence":"write a formal headline to ./out/headline.txt","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":"For each incoming brief","strategy":"cold"});
    let per_item = replay(intent, &record, &[MODEL]);
    assert_eq!(per_item.status, CompileStatus::Ready, "{per_item:#?}");
    let doc = document(&per_item);
    assert_eq!(doc["inputs"]["item"]["required"], true, "{doc:#}");
}

// ── an outbound effect repeated per item is asked, never sent once in silence ───
const ALERTS: &str = "Read ./alerts.csv (columns id,level,message). For each critical row, send a POST to http://127.0.0.1:18471/hooks with a JSON body {id, message}, one request per alert, nothing for the other levels. At the end write ./out/sent.json: the array of the ids sent, in file order.";
const ALERTS_FOLD: &str = "Read ./alerts.csv (columns id,level,message). For each row, compute whether it is critical. Then send one POST to http://127.0.0.1:18471/hooks with the critical rows as a JSON body and write ./out/sent.json with their ids.";
const RULE_CRITICAL: (&str, &str) = (
    "const.rule_expression",
    r#"".records | map(select(.level == \"critical\"))""#,
);
fn alerts_record(intent: &str, compute: &str, send: &str, trigger: &str) -> Value {
    json!({"operations":[
        {"op":"read","detail":"./alerts.csv","evidence":"Read ./alerts.csv (columns id,level,message)","categories":[]},
        {"op":"compute","detail":"critical","evidence":compute,"categories":[]}],
      "effects":[
        {"verb":"send","target":"a POST to http://127.0.0.1:18471/hooks","policy":"automatic","evidence":send,"policy_literal":null},
        {"verb":"write","target":"./out/sent.json","policy":"automatic","evidence":"write ./out/sent.json","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./alerts.csv"},{"role":"url","literal":"http://127.0.0.1:18471/hooks"},{"role":"path","literal":"./out/sent.json"}],
      "constraints":[],"unknowns":[],"trigger":trigger,"strategy":"cold",
      "intent_check": intent.contains(send)})
}

#[test]
fn an_outbound_effect_repeated_per_item_of_a_read_source_is_asked_never_sent_once() {
    let record = alerts_record(
        ALERTS,
        "For each critical row",
        "send a POST to http://127.0.0.1:18471/hooks with a JSON body {id, message}, one request per alert",
        "For each critical row",
    );
    let out = replay(ALERTS, &record, &[RULE_CRITICAL]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown
                && d.target == "send"
                && d.message.contains("once per item")
                && d.message.contains("For each critical row")),
        "{out:#?}"
    );
    // The effect a later sentence states applies to the whole result: one POST, no item,
    // no clarification. The distributive trigger over the rows stays a trigger.
    let folded = alerts_record(
        ALERTS_FOLD,
        "For each row, compute whether it is critical",
        "send one POST to http://127.0.0.1:18471/hooks with the critical rows as a JSON body",
        "For each row",
    );
    let out = replay(ALERTS_FOLD, &folded, &[RULE_CRITICAL]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert!(doc.get("inputs").is_none(), "{doc:#}");
    assert_eq!(
        tasks(&doc)["send"]["invoke"]["tool"],
        "nika:fetch",
        "{doc:#}"
    );
    assert_eq!(tasks(&doc)["send"]["invoke"]["args"]["method"], "POST");
    assert_eq!(
        doc["const"]["send_endpoint"],
        "http://127.0.0.1:18471/hooks"
    );
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}"
    );
}

// ── distinct files bound to one drafted text are asked, never duplicated ────────
// The sealed "three headline variants, one per tone" seed drafted once and wrote the same
// body to three files; the run would have been green on wrong content.
const VARIANTS: &str = "Take the product brief in ./brief.md and write three headline variants, one per tone: formal, playful and urgent, each to its own file ./out/headline-formal.txt, ./out/headline-playful.txt and ./out/headline-urgent.txt.";
fn variants_record() -> Value {
    json!({"operations":[
        {"op":"read","detail":"./brief.md","evidence":"Take the product brief in ./brief.md","categories":[]},
        {"op":"draft","detail":"three headline variants, one per tone: formal, playful and urgent","evidence":"write three headline variants, one per tone: formal, playful and urgent","categories":[]}],
      "effects":[
        {"verb":"write","target":"./out/headline-formal.txt","policy":"automatic","evidence":"each to its own file ./out/headline-formal.txt","policy_literal":null},
        {"verb":"write","target":"./out/headline-playful.txt","policy":"automatic","evidence":"./out/headline-playful.txt","policy_literal":null},
        {"verb":"write","target":"./out/headline-urgent.txt","policy":"automatic","evidence":"./out/headline-urgent.txt","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"})
}

#[test]
fn distinct_files_bound_to_one_drafted_text_are_asked_never_duplicated() {
    let out = replay(VARIANTS, &variants_record(), &[MODEL]);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Unknown
                && d.message.contains("./out/headline-formal.txt")
                && d.message.contains("./out/headline-playful.txt")
                && d.message.contains("same drafted text")),
        "{out:#?}"
    );
    // Two renderings of one computed result (JSON and CSV) are not a duplicate.
    let intent = "Read ./data/orders.csv, keep only the rows whose amount is strictly greater than 100, and write those rows to ./out/big.json and to ./out/big.csv.";
    let record = json!({"operations":[
        {"op":"read","detail":"./data/orders.csv","evidence":"Read ./data/orders.csv","categories":[]},
        {"op":"compute","detail":"keep only the rows whose amount is strictly greater than 100","evidence":"keep only the rows whose amount is strictly greater than 100","categories":[]}],
      "effects":[
        {"verb":"write","target":"./out/big.json","policy":"automatic","evidence":"write those rows to ./out/big.json","policy_literal":null},
        {"verb":"write","target":"./out/big.csv","policy":"automatic","evidence":"to ./out/big.csv","policy_literal":null}],
      "obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[RULE]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert_eq!(
        tasks(&doc)["write_output"]["with"]["content"],
        "${{ tasks.compute.output }}",
        "{doc:#}"
    );
    assert_eq!(
        tasks(&doc)["write_big"]["with"]["content"],
        "${{ tasks.big_csv.output }}"
    );
}

// ── a context sentence and a structure law bind no operation ─────────────────────
// Sealed lanes on wave27: « the file has the columns … » and « nothing else » became format
// duties no read→write plan could carry; « no language model » was obeyed by luck or broken
// in silence.
fn ledger_duties(out: &CompileOutcome) -> Vec<(String, String, String)> {
    let ledger = &out.provenance.decision.as_ref().unwrap()["ledger"];
    ledger
        .as_array()
        .unwrap_or_else(|| panic!("no duties: {ledger:#}"))
        .iter()
        .map(|d| {
            (
                d["kind"].as_str().unwrap().to_owned(),
                d["state"].as_str().unwrap().to_owned(),
                d["realized_by"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_context_sentence_and_a_closure_bind_no_operation_and_are_recorded() {
    let intent = "Read ./people.json, which has the fields name and city, and write it to ./out/people-copy.json. Nothing else.";
    let record = json!({"operations":[
        {"op":"read","detail":"./people.json","evidence":"Read ./people.json","categories":[]}],
      "effects":[{"verb":"write","target":"./out/people-copy.json","policy":"automatic","evidence":"write it to ./out/people-copy.json","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./people.json"},{"role":"path","literal":"./out/people-copy.json"}],
      "constraints":["which has the fields name and city","Nothing else."],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let duties = ledger_duties(&out);
    assert!(
        duties.contains(&("context".into(), "realized".into(), "the material".into())),
        "{duties:?}"
    );
    assert!(
        duties.contains(&(
            "structure".into(),
            "realized".into(),
            "the emitted shape".into()
        )),
        "{duties:?}"
    );
    assert!(
        !duties.iter().any(|(_, state, _)| state == "unresolved"),
        "{duties:?}"
    );
}

#[test]
fn a_no_model_law_refuses_a_drafting_plan_and_admits_a_typed_one() {
    let intent =
        "Read ./brief.md and write a short summary to ./out/summary.md. No language model.";
    let record = json!({"operations":[
        {"op":"read","detail":"./brief.md","evidence":"Read ./brief.md","categories":[]},
        {"op":"draft","detail":"a short summary","evidence":"write a short summary to ./out/summary.md","categories":[]}],
      "effects":[{"verb":"write","target":"./out/summary.md","policy":"automatic","evidence":"write a short summary to ./out/summary.md","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./brief.md"},{"role":"path","literal":"./out/summary.md"}],
      "constraints":["No language model."],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[MODEL]);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("forbids a language model")),
        "{out:#?}"
    );
    let duties = ledger_duties(&out);
    assert!(
        duties.contains(&("structure".into(), "unresolved".into(), String::new())),
        "{duties:?}"
    );
    // The same law over a plan that infers nothing holds by construction.
    let intent = "Read ./brief.md and write it to ./out/copy.md. No language model.";
    let record = json!({"operations":[
        {"op":"read","detail":"./brief.md","evidence":"Read ./brief.md","categories":[]}],
      "effects":[{"verb":"write","target":"./out/copy.md","policy":"automatic","evidence":"write it to ./out/copy.md","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./brief.md"},{"role":"path","literal":"./out/copy.md"}],
      "constraints":["No language model."],"unknowns":[],"trigger":null,"strategy":"cold"});
    let out = replay(intent, &record, &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        ledger_duties(&out).contains(&(
            "structure".into(),
            "realized".into(),
            "the emitted shape".into()
        )),
        "{:?}",
        ledger_duties(&out)
    );
}

// ── a quantified request without a corpus asks where the items live ───────────────
// wave28 v2-52: « for each … » over items the request never locates compiled to a program
// with a required `inputs.item` the run could not supply; the seed wanted the question.
#[test]
fn a_quantified_request_without_a_corpus_asks_where_the_items_live() {
    let intent = "For each invoice, extract the vendor and the total and write the records to ./out/totals.json.";
    let record = json!({"operations":[
        {"op":"extract","detail":"the vendor and the total","evidence":"extract the vendor and the total","categories":[]}],
      "effects":[{"verb":"write","target":"./out/totals.json","policy":"automatic","evidence":"write the records to ./out/totals.json","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./out/totals.json"}],
      "constraints":[],"unknowns":[],"trigger":"For each invoice","strategy":"cold"});
    let asked = replay(intent, &record, &[MODEL]);
    assert!(asked.candidate.is_none(), "{asked:#?}");
    assert!(keys(&asked).contains(&"const.source_glob"), "{asked:#?}");
    assert!(label(&asked, "const.source_glob").contains("For each invoice"));
    let out = replay(
        intent,
        &record,
        &[MODEL, ("const.source_glob", r#""./invoices/*.md""#)],
    );
    let doc = document(&out);
    assert!(doc["inputs"].get("item").is_none(), "{doc:#}");
    assert_eq!(tasks(&doc)["glob_source"]["invoke"]["tool"], "nika:glob");
    assert_eq!(doc["const"]["source_glob"], "./invoices/*.md");
}

// ── a ranking without its count asks how many rows to keep ───────────────────────
// wave28 v2-53: « die meistverkauften Artikel » with no count compiled to a full descending
// sort; the seed wanted the count asked. The answer bounds the sort.
#[test]
fn a_ranking_without_its_count_asks_how_many_rows_then_keeps_them() {
    let intent = "Read ./shop/sales.csv (columns item,units) and write the top-selling items by units to ./out/top.csv.";
    let record = json!({"operations":[
        {"op":"read","detail":"./shop/sales.csv","evidence":"Read ./shop/sales.csv (columns item,units)","categories":[]},
        {"op":"compute","detail":"the top-selling items by units","evidence":"the top-selling items by units","categories":[]}],
      "effects":[{"verb":"write","target":"./out/top.csv","policy":"automatic","evidence":"write the top-selling items by units to ./out/top.csv","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./shop/sales.csv"},{"role":"path","literal":"./out/top.csv"}],
      "constraints":[],"unknowns":[],"trigger":null,"strategy":"cold",
      "rules":[{"text":"the top-selling items by units","clauses":[],"junction":"and","summary":false,
                "shape":{"group_by":null,"aggregations":[],"sort_by":"units","descending":true,"columns":["item","units"],"derived":[]}}]});
    let asked = replay(intent, &record, &[]);
    assert!(asked.candidate.is_none(), "{asked:#?}");
    assert_eq!(keys(&asked), ["const.top_n"], "{asked:#?}");
    assert!(label(&asked, "const.top_n").contains("top-selling"));
    let out = replay(intent, &record, &[("const.top_n", r#""3""#)]);
    let doc = document(&out);
    let expression = tasks(&doc)["compute"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap();
    assert!(expression.contains("| .[:3] |"), "{expression}");
}
