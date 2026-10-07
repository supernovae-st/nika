// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Slice B: a composed request keeps every occurrence, branch, named result and guard it states.
//! Synthetic sentinels only (two sources, two destinations, one approval before both writes);
//! the expected source→write→result mapping is built here, never read from the proposal.
//! Scripted providers and approving or refusing judge doubles: no network, no key, no model
//! capability, and no claim that a scripted judgment qualifies model semantics.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode, Strategy,
    outcome_document,
};
use nika_compile_cognition::compile_with_provider;
use nika_compile_fidelity::sketch::{Sketch, document, structural_laws};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

mod common;
use common::{Judged, Rotating};

const INTENT: &str = "Copie ./alpha.txt dans ./out/alpha.txt et ./beta.txt dans ./out/beta.txt. Demande mon accord une seule fois avant les deux écritures. Nomme les résultats alpha et beta.";
/// The parts of [`INTENT`] the verifier asks alone (none restricts).
const PARTS: [&str; 3] = [
    "Copie ./alpha.txt dans ./out/alpha.txt et ./beta.txt dans ./out/beta.txt",
    "Demande mon accord une seule fois avant les deux écritures",
    "Nomme les résultats alpha et beta",
];

/// The independent oracle: each source, the write that must carry it, and the result name.
const BRANCHES: [(&str, &str, &str, &str); 2] = [
    ("./alpha.txt", "write_alpha", "./out/alpha.txt", "alpha"),
    ("./beta.txt", "write_beta", "./out/beta.txt", "beta"),
];

fn policy(native: NativeMode, repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(repairs)
}

fn task(id: &str, tool: &str, extra: &Value) -> Value {
    let mut t = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
    for (k, v) in extra.as_object().unwrap() {
        t[k] = v.clone();
    }
    t
}

fn tasks() -> Vec<Value> {
    vec![
        task(
            "read_alpha",
            "nika:read",
            &json!({"reads": ["./alpha.txt"]}),
        ),
        task("read_beta", "nika:read", &json!({"reads": ["./beta.txt"]})),
        task("approve", "nika:prompt", &json!({})),
        task(
            "write_alpha",
            "nika:write",
            &json!({"writes": ["./out/alpha.txt"], "with": [{"name": "text", "from": "read_alpha"}], "gated_by": "approve"}),
        ),
        task(
            "write_beta",
            "nika:write",
            &json!({"writes": ["./out/beta.txt"], "with": [{"name": "text", "from": "read_beta"}], "gated_by": "approve"}),
        ),
    ]
}

fn outputs() -> Value {
    json!([{"name": "alpha", "from": "write_alpha"}, {"name": "beta", "from": "write_beta"}])
}

fn sketch_answer(tasks: &[Value], outputs: Option<Value>) -> String {
    let mut answer = json!({"name": "two-copies", "tasks": tasks, "questions": [], "gaps": [], "notes": "graph"});
    if let Some(outputs) = outputs {
        answer["outputs"] = outputs;
    }
    answer.to_string()
}

fn fills() -> String {
    json!({"fills": [{"task": "approve", "field": "args.message", "value": "Écrire les deux copies ?"}], "notes": "fills"})
        .to_string()
}

async fn compile(native: NativeMode, repairs: u32, replies: Vec<String>) -> (CompileOutcome, u32) {
    let provider = Rotating::new(replies);
    let judged = Judged::approving(&provider);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(native, repairs));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    (out, provider.calls.load(Ordering::SeqCst))
}

fn native(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["native"].clone()
}

fn emitted(out: &CompileOutcome) -> Value {
    let source = out.provenance.plan.as_ref().unwrap()["source"]
        .as_str()
        .unwrap()
        .to_owned();
    serde_yaml_bw::from_str(&source).unwrap()
}

/// Every branch of the independent oracle is realized: its read, its gated write of that read,
/// its named result on that write.
fn assert_branches(doc: &Value) {
    for (source, write, destination, result) in BRANCHES {
        let task = &doc["tasks"][write];
        assert_eq!(
            task["invoke"]["args"]["path"], destination,
            "{write}: {doc:#}"
        );
        let read = task["with"]["text"].as_str().unwrap();
        let reader = read
            .trim_start_matches("${{ tasks.")
            .trim_end_matches(".output }}");
        assert_eq!(
            doc["tasks"][reader]["invoke"]["args"]["path"], source,
            "{write}: {doc:#}"
        );
        assert_eq!(
            task["when"], "${{ with.approved == true }}",
            "{write} is gated"
        );
        assert_eq!(
            task["with"]["approved"], "${{ tasks.approve.output }}",
            "{write}"
        );
        assert_eq!(
            doc["outputs"][result],
            format!("${{{{ tasks.{write}.output }}}}"),
            "{doc:#}"
        );
    }
    assert_eq!(
        doc["outputs"].as_object().map(serde_json::Map::len),
        Some(BRANCHES.len()),
        "exactly the named results, no invented `result`: {doc:#}"
    );
}

// ── Named results: pure parse and the real cognitive door ──────────────────────────────────────

#[test]
fn the_pure_parse_emits_exactly_the_named_results() {
    let record = json!({"name": "two-copies", "tasks": tasks(), "outputs": outputs()});
    let doc = document(&Sketch::from_json(&record).unwrap(), &[]);
    assert_eq!(
        doc["outputs"],
        json!({"alpha": "${{ tasks.write_alpha.output }}", "beta": "${{ tasks.write_beta.output }}"})
    );
}

#[tokio::test]
async fn two_named_results_survive_the_cognitive_door_and_the_record() {
    let (out, calls) = compile(
        NativeMode::Sketch,
        0,
        vec![sketch_answer(&tasks(), Some(outputs())), fills()],
    )
    .await;
    assert_eq!(calls, 2);
    assert_eq!(
        native(&out)["accepted"],
        true,
        "{:#}",
        native(&out)["rounds"]
    );
    assert_branches(&emitted(&out));
    let consumed = &native(&out)["rounds"][0]["proposed_sketch"];
    assert_eq!(
        consumed["outputs"],
        outputs(),
        "the record keeps what the compiler consumed"
    );
}

#[tokio::test]
async fn a_malformed_output_list_is_refused_at_the_sketch_phase() {
    let cases = [
        (
            "unknown target",
            json!([{"name": "alpha", "from": "write_gamma"}]),
        ),
        (
            "duplicate name",
            json!([{"name": "alpha", "from": "write_alpha"}, {"name": "alpha", "from": "write_beta"}]),
        ),
        ("empty name", json!([{"name": "", "from": "write_alpha"}])),
        (
            "extra key",
            json!([{"name": "alpha", "from": "write_alpha", "path": "./x"}]),
        ),
        ("not a list", json!({"alpha": "write_alpha"})),
    ];
    for (case, outputs) in cases {
        let (out, calls) = compile(
            NativeMode::Sketch,
            0,
            vec![sketch_answer(&tasks(), Some(outputs)), fills()],
        )
        .await;
        assert_eq!(calls, 1, "{case}: never filled");
        assert!(out.candidate.is_none(), "{case}");
        let first = native(&out)["rounds"][0].clone();
        assert!(
            first["diagnostics"].to_string().contains("outputs"),
            "{case}: a sketch refusal names the outputs: {first:#}"
        );
    }
}

#[tokio::test]
async fn a_sketch_that_omits_requested_results_keeps_the_historical_result_only() {
    // Omission is the historical shape: one `result`, never the requested names. This control
    // documents that omission does NOT satisfy a request for named results.
    let (out, _) = compile(
        NativeMode::Sketch,
        0,
        vec![sketch_answer(&tasks(), None), fills()],
    )
    .await;
    let doc = emitted(&out);
    assert_eq!(
        doc["outputs"],
        json!({"result": "${{ tasks.write_beta.output }}"})
    );
    assert!(doc["outputs"].get("alpha").is_none());
}

// ── Dataflow: each write consumes its own source; task order does not matter ───────────────────

#[tokio::test]
async fn independent_branches_keep_their_mapping_under_task_permutation() {
    let mut reversed = tasks();
    // Writes still follow their reads and the prompt; the two branches swap places.
    reversed.swap(0, 1);
    reversed.swap(3, 4);
    let (out, _) = compile(
        NativeMode::Sketch,
        0,
        vec![sketch_answer(&reversed, Some(outputs())), fills()],
    )
    .await;
    assert_branches(&emitted(&out));
}

#[tokio::test]
async fn a_second_edge_a_write_would_silently_drop_is_a_required_template() {
    // One write bound to both reads: without a content template the assembler would write only
    // the first edge. The content hole is then required, never optional.
    let mut graph = tasks();
    graph[3]["with"] =
        json!([{"name": "text", "from": "read_alpha"}, {"name": "more", "from": "read_beta"}]);
    let (out, _) = compile(
        NativeMode::Sketch,
        0,
        vec![sketch_answer(&graph, Some(outputs())), fills()],
    )
    .await;
    let fill = native(&out)["rounds"][1].clone();
    assert!(
        fill["diagnostics"]
            .to_string()
            .contains("write_alpha.args.content"),
        "{fill:#}"
    );
    assert!(out.candidate.is_none());
}

// ── Gate: a guard on each effect, never decorative ─────────────────────────────────────────────

#[tokio::test]
async fn removing_one_guard_or_replacing_it_by_a_control_edge_is_refused() {
    let mut ungated = tasks();
    ungated[4].as_object_mut().unwrap().remove("gated_by");
    let mut after_only = ungated.clone();
    after_only[4]["after"] = json!(["approve"]);
    for (case, graph) in [
        ("one guard removed", ungated),
        ("after instead of a guard", after_only),
    ] {
        let (out, _) = compile(
            NativeMode::Sketch,
            0,
            vec![sketch_answer(&graph, Some(outputs())), fills()],
        )
        .await;
        assert!(
            out.candidate.is_none(),
            "{case}: {:#}",
            native(&out)["rounds"]
        );
        assert_ne!(native(&out)["accepted"], true, "{case}");
    }
}

// ── Occurrences: a COLD plan that would merge two reads hands the request to the sketch door ───

fn cold_plan() -> String {
    json!({"steps": [
        {"op": "read", "detail": "./alpha.txt", "evidence": "Copie ./alpha.txt dans ./out/alpha.txt"},
        {"op": "read", "detail": "./beta.txt", "evidence": "./beta.txt dans ./out/beta.txt"}
    ], "effects": [
        {"verb": "write", "target": "./out/alpha.txt", "policy": "human_first", "evidence": "Demande mon accord une seule fois avant les deux écritures"},
        {"verb": "write", "target": "./out/beta.txt", "policy": "human_first", "evidence": "Demande mon accord une seule fois avant les deux écritures"}
    ], "obligations": [], "constraints": [], "unknowns": []})
    .to_string()
}

#[tokio::test]
async fn two_occurrences_the_plan_cannot_keep_go_to_the_sketch_door_with_every_call_kept() {
    let (out, calls) = compile(
        NativeMode::Escalate,
        1,
        vec![
            cold_plan(),
            sketch_answer(&tasks(), Some(outputs())),
            fills(),
        ],
    )
    .await;
    let route = out.provenance.decision.as_ref().unwrap()["route"].to_string();
    assert!(route.contains("native: sketch"), "{route}");
    // The plan never collapsed the two reads into one step.
    let merged = out.provenance.decision.as_ref().unwrap()["cold_samples"].to_string();
    assert!(!merged.contains("./alpha.txt ; ./beta.txt"), "{merged}");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let roles: Vec<&str> = receipt
        .context
        .iter()
        .map(|c| c["call"].as_str().unwrap())
        .collect();
    assert_eq!(
        &roles[..3],
        ["plan", "sketch", "fill"],
        "the paid plan call stays first: {roles:?}"
    );
    assert_eq!(calls, 3);
    assert_branches(&emitted(&out));
    assert!(
        !outcome_document(&out)
            .to_string()
            .contains("candidate_lines")
    );
}

#[tokio::test]
async fn without_a_sketch_door_or_its_budget_the_composition_is_named_and_no_call_is_hidden() {
    for (case, native, repairs) in [
        ("no door", NativeMode::Off, 1),
        ("no budget", NativeMode::Escalate, 0),
    ] {
        let (out, calls) = compile(
            native,
            repairs,
            vec![
                cold_plan(),
                sketch_answer(&tasks(), Some(outputs())),
                fills(),
            ],
        )
        .await;
        assert_eq!(calls, 1, "{case}: only the plan call was made");
        assert!(out.candidate.is_none(), "{case}");
        let text = serde_json::to_string(
            &out.diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(
            text.contains("`read`") && text.contains("sketch"),
            "{case}: the inability is named: {text}"
        );
    }
}

// ── Omission, null and empty outputs keep distinct meanings ────────────────────────────────────

#[test]
fn omitted_null_and_empty_outputs_keep_their_distinct_meanings() {
    let emit = |outputs: Option<Value>| {
        let mut record = json!({"name": "two-copies", "tasks": tasks()});
        if let Some(outputs) = outputs {
            record["outputs"] = outputs;
        }
        document(&Sketch::from_json(&record).unwrap(), &[])
    };
    let legacy = json!({"result": "${{ tasks.write_beta.output }}"});
    assert_eq!(
        emit(None)["outputs"],
        legacy,
        "omitted: the historical single result"
    );
    assert_eq!(
        emit(Some(Value::Null))["outputs"],
        legacy,
        "null: the same as omitted"
    );
    assert!(
        emit(Some(json!([]))).get("outputs").is_none(),
        "[]: no workflow output"
    );
}

// ── Agent and loop controls: exact when stated, historical when omitted, never invented ───────

const AGENT_INTENT: &str = "Lis ./notes.md, fais-le analyser par un agent limité à 3 tours avec seulement nika:jq et nika:date, puis écris l'analyse dans ./out/analyse.md";

fn agent_tasks(agent: &Value) -> Vec<Value> {
    let mut analyst = json!({"id": "analyst", "verb": "agent", "purpose": "analyse the notes",
        "with": [{"name": "notes", "from": "read_notes"}]});
    for (k, v) in agent.as_object().unwrap() {
        analyst[k] = v.clone();
    }
    vec![
        task("read_notes", "nika:read", &json!({"reads": ["./notes.md"]})),
        analyst,
        task(
            "save",
            "nika:write",
            &json!({"writes": ["./out/analyse.md"], "with": [{"name": "analysis", "from": "analyst"}]}),
        ),
    ]
}

fn parsed(tasks: &[Value]) -> Sketch {
    Sketch::from_json(&json!({"name": "agent", "tasks": tasks})).unwrap()
}

#[test]
fn an_agents_stated_turns_and_tools_are_emitted_exactly_and_omission_stays_historical() {
    let stated = document(
        &parsed(&agent_tasks(
            &json!({"max_turns": 3, "tools": ["nika:jq", "nika:date"]}),
        )),
        &[],
    );
    let agent = &stated["tasks"]["analyst"]["agent"];
    assert_eq!(agent["max_turns"], 3);
    assert_eq!(agent["tools"], json!(["nika:jq", "nika:date"]));
    let permitted = stated["permits"]["tools"].as_array().unwrap();
    for tool in ["nika:jq", "nika:date"] {
        assert!(
            permitted.contains(&json!(tool)),
            "{tool} is a derived requirement: {permitted:?}"
        );
    }
    let omitted = document(&parsed(&agent_tasks(&json!({}))), &[]);
    assert_eq!(
        omitted["tasks"]["analyst"]["agent"]["max_turns"], 4,
        "historical"
    );
    assert_eq!(
        omitted["tasks"]["analyst"]["agent"]["tools"],
        json!([]),
        "historical"
    );
    let empty = document(&parsed(&agent_tasks(&json!({"tools": []}))), &[]);
    assert_eq!(
        empty["tasks"]["analyst"]["agent"]["tools"],
        json!([]),
        "explicit: no tool"
    );
}

#[test]
fn a_misplaced_or_malformed_control_is_refused_by_name() {
    let intent = AGENT_INTENT;
    let cases: Vec<(&str, Vec<Value>, &str)> = vec![
        (
            "turns on an invoke",
            {
                let mut t = agent_tasks(&json!({}));
                t[0]["max_turns"] = json!(3);
                t
            },
            "`read_notes` states `max_turns`",
        ),
        (
            "zero turns",
            agent_tasks(&json!({"max_turns": 0})),
            "`analyst` states `max_turns` outside",
        ),
        (
            "too many turns",
            agent_tasks(&json!({"max_turns": 1001})),
            "`analyst` states `max_turns` outside",
        ),
        (
            "a glob tool",
            agent_tasks(&json!({"tools": ["nika:*"]})),
            "`analyst` lists a tool that is not one named",
        ),
        (
            "a repeated tool",
            agent_tasks(&json!({"tools": ["nika:jq", "nika:jq"]})),
            "`analyst` lists the tool `nika:jq` twice",
        ),
        (
            "fail_fast without a loop",
            agent_tasks(&json!({"fail_fast": true})),
            "`analyst` states `fail_fast`",
        ),
    ];
    for (case, tasks, expected) in cases {
        let refusals = structural_laws(&parsed(&tasks), intent, &[]).join("\n");
        assert!(refusals.contains(expected), "{case}: {refusals}");
    }
    for (case, agent) in [
        ("fractional turns", json!({"max_turns": 2.5})),
        ("string turns", json!({"max_turns": "3"})),
        ("tools not a list", json!({"tools": "nika:jq"})),
        ("string fail_fast", json!({"fail_fast": "yes"})),
    ] {
        let record = json!({"name": "agent", "tasks": agent_tasks(&agent)});
        let error = Sketch::from_json(&record).unwrap_err();
        assert!(error.contains("tasks[1]"), "{case}: {error}");
    }
}

#[tokio::test]
async fn an_agent_with_stated_pure_tools_crosses_the_cognitive_door_and_an_effectful_tool_is_refused()
 {
    let fills = json!({"fills": [{"task": "analyst", "field": "prompt", "value": "Analyse ces notes : ${{ with.notes }}"}], "notes": "fills"}).to_string();
    let answer = |agent: &Value| {
        json!({"name": "agent", "tasks": agent_tasks(agent), "questions": [], "gaps": [], "notes": "graph"}).to_string()
    };
    let provider = Rotating::new(vec![
        answer(&json!({"max_turns": 3, "tools": ["nika:jq", "nika:date"]})),
        fills.clone(),
    ]);
    let judged = Judged::approving(&provider);
    let request =
        CompileRequest::create(AGENT_INTENT).with_authoring_policy(policy(NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    let rounds = native(&out)["rounds"].clone();
    let consumed = &rounds[0]["proposed_sketch"]["tasks"][1];
    assert_eq!(consumed["max_turns"], 3, "{rounds:#}");
    assert_eq!(consumed["tools"], json!(["nika:jq", "nika:date"]));
    assert_eq!(consumed["defaulted"], json!([]));
    assert_eq!(native(&out)["accepted"], true, "{rounds:#}");
    let agent = &emitted(&out)["tasks"]["analyst"]["agent"];
    assert_eq!(
        (agent["max_turns"].clone(), agent["tools"].clone()),
        (json!(3), json!(["nika:jq", "nika:date"]))
    );
    // An effectful agent tool is the named still-open part of B: refused, never silently granted.
    let provider = Rotating::new(vec![answer(&json!({"tools": ["nika:write"]})), fills]);
    let judged = Judged::approving(&provider);
    let out = compile_with_provider(&request, &judged).await.unwrap();
    let first = native(&out)["rounds"][0]["diagnostics"].to_string();
    assert!(
        first.contains("`analyst` lists `nika:write`, a tool with effects"),
        "{first}"
    );
    assert!(out.candidate.is_none());
}

#[tokio::test]
async fn a_loop_stated_to_stop_at_the_first_failure_is_emitted_so_and_omission_is_marked_defaulted()
{
    let intent = "Lis ./items.json, rédige une ligne pour chaque élément en t'arrêtant à la première erreur, puis écris le tout dans ./out/lignes.md";
    let graph = |each: &Value| {
        let mut line = json!({"id": "line", "verb": "infer", "purpose": "one line per item", "for_each": "split"});
        for (k, v) in each.as_object().unwrap() {
            line[k] = v.clone();
        }
        json!({"name": "loop", "tasks": [
            task("read_items", "nika:read", &json!({"reads": ["./items.json"]})),
            task("split", "nika:jq", &json!({"with": [{"name": "document", "from": "read_items"}]})),
            line,
            task("save", "nika:write", &json!({"writes": ["./out/lignes.md"], "with": [{"name": "lines", "from": "line"}]})),
        ], "questions": [], "gaps": [], "notes": "graph"})
        .to_string()
    };
    let fills = json!({"fills": [
        {"task": "split", "field": "expression", "value": "fromjson"},
        {"task": "line", "field": "prompt", "value": "Une ligne pour : ${{ item }}"}
    ], "notes": "fills"})
    .to_string();
    let request =
        CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Sketch, 0));
    for (each, fail_fast, defaulted) in [
        (json!({"fail_fast": true}), json!(true), json!([])),
        (json!({}), json!(false), json!(["fail_fast"])),
    ] {
        let provider = Rotating::new(vec![graph(&each), fills.clone()]);
        let judged = Judged::approving(&provider);
        let out = compile_with_provider(&request, &judged).await.unwrap();
        let rounds = native(&out)["rounds"].clone();
        assert_eq!(
            rounds[0]["proposed_sketch"]["tasks"][2]["defaulted"], defaulted,
            "{rounds:#}"
        );
        assert_eq!(native(&out)["accepted"], true, "{rounds:#}");
        assert_eq!(
            emitted(&out)["tasks"]["line"]["for_each"]["fail_fast"],
            fail_fast
        );
    }
}

// ── The transition keeps every existing refusal, Plan positive, budget bound and failed call ───

fn roles(out: &CompileOutcome) -> Vec<String> {
    out.provenance
        .authoring
        .as_ref()
        .map(|r| {
            r.context
                .iter()
                .map(|c| c["call"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn route(out: &CompileOutcome) -> String {
    out.provenance.decision.as_ref().unwrap()["route"].to_string()
}

#[tokio::test]
async fn two_reads_feeding_one_destination_keep_their_plan() {
    let intent = "Lis ./alpha.txt et ./beta.txt et écris leur fusion dans ./out/fusion.txt.";
    let plan = json!({"steps": [
        {"op": "read", "detail": "./alpha.txt", "evidence": "Lis ./alpha.txt"},
        {"op": "read", "detail": "./beta.txt", "evidence": "./beta.txt"}
    ], "effects": [
        {"verb": "write", "target": "./out/fusion.txt", "policy": "automatic", "evidence": "écris leur fusion dans ./out/fusion.txt"}
    ], "obligations": [], "constraints": [], "unknowns": []})
    .to_string();
    let provider = Rotating::new(vec![plan]);
    let judged = Judged::approving(&provider);
    let request =
        CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Escalate, 1));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    assert!(!route(&out).contains("native: sketch"), "{}", route(&out));
    assert!(
        !roles(&out).iter().any(|r| r.starts_with("sketch")),
        "{:?}",
        roles(&out)
    );
}

#[tokio::test]
async fn a_composition_the_merge_refuses_stays_refused() {
    // The second read cites evidence the request never wrote: the merge's own anchoring refuses
    // it (after its one evidence repair); a composition never bypasses that refusal.
    let mut plan: Value = serde_json::from_str(&cold_plan()).unwrap();
    plan["steps"][1]["evidence"] = json!("./beta.txt vers un dossier inventé");
    let (out, calls) = compile(
        NativeMode::Escalate,
        1,
        vec![
            plan.to_string(),
            plan.to_string(),
            sketch_answer(&tasks(), Some(outputs())),
            fills(),
        ],
    )
    .await;
    assert!(
        !route(&out).contains("sketch for branches"),
        "{}",
        route(&out)
    );
    // The refused plan round escalates like any plan without a candidate (no longer to source):
    // the sketch door opens as the escalation, never as the composition the merge refused.
    assert!(
        route(&out).contains("native: sketch after the plan"),
        "{}",
        route(&out)
    );
    assert_eq!(&roles(&out)[..2], ["plan", "repair"], "{:?}", roles(&out));
    assert!(calls >= 2);
}

#[tokio::test]
async fn a_larger_allowance_lets_the_sketch_door_repair_within_the_same_bound() {
    // repairs 2: the sketch door gets 1, so one refused sketch is repaired; every call stays.
    let mut duplicate = outputs();
    duplicate[1]["name"] = json!("alpha");
    let (out, calls) = compile(
        NativeMode::Escalate,
        2,
        vec![
            cold_plan(),
            sketch_answer(&tasks(), Some(duplicate)),
            sketch_answer(&tasks(), Some(outputs())),
            fills(),
        ],
    )
    .await;
    assert_eq!(calls, 4, "{:?}", roles(&out));
    assert_eq!(
        roles(&out),
        ["plan", "sketch", "sketch-repair", "fill", "judge_request"],
        "the paid plan, the refused sketch, its repair, the fill, the request judgment"
    );
    assert_branches(&emitted(&out));
}

/// Answers its script in order, failing (as a provider error) where the script says so.
struct Failing {
    replies: Vec<Option<String>>,
    calls: AtomicU32,
}

impl ProviderInferDyn for Failing {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
        match self.replies.get(index).cloned().flatten() {
            Some(text) => Ok(InferResponse::new(
                vec![ContentBlock::Text { text }],
                TokenUsage::new(10, 10),
                StopReason::EndTurn,
            )),
            None => Err(ProviderError::Other {
                reason: "scripted failure".to_owned(),
            }),
        }
    }
}

#[tokio::test]
async fn a_failed_sketch_call_is_kept_and_nothing_is_retried_or_hidden() {
    let provider = Failing {
        replies: vec![Some(cold_plan()), None],
        calls: AtomicU32::new(0),
    };
    let judged = Judged::approving(&provider);
    let request =
        CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Escalate, 1));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        2,
        "the plan and one failed sketch call"
    );
    assert_eq!(roles(&out), ["plan", "sketch"]);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(
        receipt.context[1]["result"]["failure_kind"],
        "provider_error"
    );
    assert!(out.candidate.is_none());
}

// ── Path counts are not branches: a merged result written twice keeps its plan ────────────────

const MERGED: &str = "Lis ./a.txt et ./b.txt, fusionne-les, écris le résultat fusionné dans ./out/x.txt et une copie dans ./out/y.txt.";

fn merged_plan() -> String {
    json!({"steps": [
        {"op": "read", "detail": "./a.txt", "evidence": "Lis ./a.txt"},
        {"op": "read", "detail": "./b.txt", "evidence": "./b.txt"},
        {"op": "draft", "detail": "la fusion des deux", "evidence": "fusionne-les"}
    ], "effects": [
        {"verb": "write", "target": "./out/x.txt", "policy": "automatic", "evidence": "écris le résultat fusionné dans ./out/x.txt"},
        {"verb": "write", "target": "./out/y.txt", "policy": "automatic", "evidence": "une copie dans ./out/y.txt"}
    ], "obligations": [], "constraints": [], "unknowns": []})
    .to_string()
}

#[tokio::test]
async fn two_sources_merged_and_written_twice_keep_their_plan_under_every_policy() {
    // Before the occurrence split (4f0c880b) this plan composed as one COLD plan asking for its
    // model; with two sources and two destinations but no source-to-destination pairing it is
    // not independent branches: neither named as a composition nor paid a sketch call.
    for native in [NativeMode::Off, NativeMode::Escalate] {
        let provider = Rotating::new(vec![merged_plan()]);
        let judged = Judged::approving(&provider);
        let request = CompileRequest::create(MERGED).with_authoring_policy(policy(native, 1));
        let out = compile_with_provider(&request, &judged).await.unwrap();
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            1,
            "{native:?}: the plan alone"
        );
        assert_eq!(roles(&out), ["plan"], "{native:?}");
        assert_eq!(
            out.provenance.strategy,
            Some(Strategy::Cold),
            "{native:?}: {}",
            route(&out)
        );
        assert!(
            route(&out).contains("compose: single"),
            "{native:?}: {}",
            route(&out)
        );
        assert!(!route(&out).contains("composition") && !route(&out).contains("sketch"));
        assert_eq!(
            out.questions
                .iter()
                .map(|q| q.key.as_str())
                .collect::<Vec<_>>(),
            ["model"],
            "{native:?}: the plan's own question survives"
        );
        assert!(
            !out.diagnostics
                .iter()
                .any(|d| d.message.contains("independent branches")),
            "{native:?}: {:?}",
            out.diagnostics
        );
    }
}

// ── A write citing its own source still pairs: written paths are the writes' targets ──────────

const COPIES: &str = "Copie ./alpha.txt dans ./out/alpha.txt et ./beta.txt dans ./out/beta.txt.";

/// Lawful (automatic writes, nothing to approve): each read cites only its source, each write
/// cites its source and its destination — the natural clause of the request.
fn cited_copies_plan() -> String {
    json!({"steps": [
        {"op": "read", "detail": "./alpha.txt", "evidence": "Copie ./alpha.txt"},
        {"op": "read", "detail": "./beta.txt", "evidence": "./beta.txt"}
    ], "effects": [
        {"verb": "write", "target": "./out/alpha.txt", "policy": "automatic", "evidence": "./alpha.txt dans ./out/alpha.txt"},
        {"verb": "write", "target": "./out/beta.txt", "policy": "automatic", "evidence": "./beta.txt dans ./out/beta.txt"}
    ], "obligations": [], "constraints": [], "unknowns": []})
    .to_string()
}

fn ungated_copies() -> Vec<Value> {
    let write = |id: &str, path: &str, from: &str| {
        task(
            id,
            "nika:write",
            &json!({"writes": [path], "with": [{"name": "text", "from": from}]}),
        )
    };
    vec![
        task(
            "read_alpha",
            "nika:read",
            &json!({"reads": ["./alpha.txt"]}),
        ),
        task("read_beta", "nika:read", &json!({"reads": ["./beta.txt"]})),
        write("write_alpha", "./out/alpha.txt", "read_alpha"),
        write("write_beta", "./out/beta.txt", "read_beta"),
    ]
}

#[tokio::test]
async fn copies_whose_writes_cite_their_sources_are_independent_branches_under_every_policy() {
    // Escalate: the sketch door, both mappings kept (source → write → destination).
    let provider = Rotating::new(vec![
        cited_copies_plan(),
        sketch_answer(&ungated_copies(), None),
        json!({"fills": [], "notes": "nothing to fill"}).to_string(),
    ]);
    let judged = Judged::approving(&provider);
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Escalate, 1));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    assert!(
        route(&out).contains("native: sketch for branches the plan cannot keep apart"),
        "Escalate: {} {:?}",
        route(&out),
        out.diagnostics
    );
    assert_eq!(&roles(&out)[..2], ["plan", "sketch"], "{:?}", roles(&out));
    let doc = emitted(&out);
    for (source, write, destination, _) in BRANCHES {
        assert_eq!(
            doc["tasks"][write]["invoke"]["args"]["path"], destination,
            "{doc:#}"
        );
        let reader = doc["tasks"][write]["with"]["text"]
            .as_str()
            .unwrap()
            .trim_start_matches("${{ tasks.")
            .trim_end_matches(".output }}")
            .to_owned();
        assert_eq!(
            doc["tasks"][&reader]["invoke"]["args"]["path"], source,
            "{doc:#}"
        );
    }
    // Off: the composition is named, nothing more is sent, no plan folds the two reads.
    let provider = Rotating::new(vec![cited_copies_plan()]);
    let judged = Judged::approving(&provider);
    let request = CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Off, 1));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "Off: the plan alone"
    );
    assert_eq!(roles(&out), ["plan"]);
    assert!(out.candidate.is_none(), "Off: {}", route(&out));
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("2 independent branches")
                && d.message.contains("native: off")),
        "Off: {} {:?}",
        route(&out),
        out.diagnostics
    );
}

// ── Oracle sensitivity: each mapping mutant is lawful structure the oracle must reject ────────
//
// The structural laws do not know which source the request paired with which destination; these
// mutants are sketches they accept. The independent oracle (BRANCHES) must reject every one for
// its own reason, and on the real cognitive door a refusing judgment must keep its verdict and
// calls and leave no READY candidate. A scripted judge proves the wiring, never a model's
// semantic judgment.

fn with_source(task: &mut Value, source: &str) {
    task["with"] = json!([{"name": "text", "from": source}]);
}

/// The four mapping mutants: (label, tasks, outputs, the oracle's exact rejection).
fn mutants() -> Vec<(&'static str, Vec<Value>, Value, [&'static str; 2])> {
    let crossed_outputs =
        json!([{"name": "alpha", "from": "write_beta"}, {"name": "beta", "from": "write_alpha"}]);
    let mut crossed_sources = tasks();
    with_source(&mut crossed_sources[3], "read_beta");
    with_source(&mut crossed_sources[4], "read_alpha");
    let mut one_source = tasks();
    with_source(&mut one_source[4], "read_alpha");
    vec![
        (
            "crossed output references",
            tasks(),
            crossed_outputs,
            [
                r#"left: String("${{ tasks.write_beta.output }}")"#,
                r#"right: "${{ tasks.write_alpha.output }}""#,
            ],
        ),
        (
            "crossed source edges",
            crossed_sources,
            outputs(),
            [r#"left: String("./beta.txt")"#, r#"right: "./alpha.txt""#],
        ),
        (
            "one requested output dropped",
            tasks(),
            json!([{"name": "alpha", "from": "write_alpha"}]),
            ["left: Null", r#"right: "${{ tasks.write_beta.output }}""#],
        ),
        (
            "both writes read one source",
            one_source,
            outputs(),
            [r#"left: String("./alpha.txt")"#, r#"right: "./beta.txt""#],
        ),
    ]
}

#[test]
fn every_mapping_mutant_is_lawful_structure_the_independent_oracle_rejects() {
    // Positive control: the faithful sketch passes the same oracle.
    let faithful = json!({"name": "two-copies", "tasks": tasks(), "outputs": outputs()});
    assert_branches(&document(&Sketch::from_json(&faithful).unwrap(), &[]));
    for (label, tasks, outputs, rejection) in mutants() {
        let record = json!({"name": "two-copies", "tasks": tasks, "outputs": outputs});
        let sketch = Sketch::from_json(&record).expect(label);
        assert_eq!(
            structural_laws(&sketch, INTENT, &[]),
            Vec::<String>::new(),
            "{label}: lawful structure"
        );
        let doc = document(&sketch, &[]);
        let verdict = std::panic::catch_unwind(|| assert_branches(&doc));
        let message = verdict
            .expect_err(label)
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        for part in rejection {
            assert!(
                message.contains(part),
                "{label}: rejected for another reason: {message}"
            );
        }
    }
}

/// The option each verifier question offers when it approves: the whole request, a clause or
/// part, an observed run, the extra-operation question and the task question.
const APPROVALS: [&str; 5] = [
    "faithful",
    "carried",
    "consistent",
    "only_requested",
    "no_task",
];

/// A judge double that refuses: the whole-request question is answered `unfaithful`, each clause
/// or part `missing`, a task question (and an observed run, or the extra-operation question) with
/// the first task it offers (`task-<id>`), else `omitted`; each is counted. Every other call goes
/// to the wrapped provider.
struct Refusing<'a, P> {
    inner: &'a P,
    judged: AtomicU32,
}

impl<P: ProviderInferDyn> ProviderInferDyn for Refusing<'_, P> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            return self.inner.infer(request).await;
        };
        let keys = schema["properties"]["choice"]["enum"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !keys.iter().any(|k| APPROVALS.iter().any(|a| k == a)) {
            return self.inner.infer(request).await;
        }
        self.judged.fetch_add(1, Ordering::SeqCst);
        let offered = |key: &str| keys.iter().find(|k| *k == key);
        let task = |k: &&Value| k.as_str().is_some_and(|k| k.starts_with("task-"));
        let key = offered("unfaithful")
            .or_else(|| offered("missing"))
            .or_else(|| keys.iter().find(task))
            .or_else(|| offered("omitted"))
            .cloned()
            .unwrap();
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: json!({"choice": key}).to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

#[tokio::test]
async fn a_refusing_judgment_keeps_its_verdict_and_calls_and_no_mutant_is_ready() {
    for (label, tasks, outputs, _) in mutants() {
        let provider = Rotating::new(vec![sketch_answer(&tasks, Some(outputs.clone())), fills()]);
        let judge = Refusing {
            inner: &provider,
            judged: AtomicU32::new(0),
        };
        let request =
            CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Sketch, 0));
        let out = compile_with_provider(&request, &judge).await.unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{label}");
        assert!(out.candidate.is_none(), "{label}: no candidate");
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            2,
            "{label}: sketch and fill"
        );
        assert_eq!(
            judge.judged.load(Ordering::SeqCst),
            7,
            "{label}: the request, its three parts and the task question of each"
        );
        assert_eq!(
            roles(&out),
            [
                "sketch",
                "fill",
                "judge_request",
                "judge_part",
                "judge_point",
                "judge_part",
                "judge_point",
                "judge_part",
                "judge_point"
            ],
            "{label}"
        );
        // Each part of the request, asked alone and found missing, is a defect named whole, with
        // the task the judge names as its reason (the first the candidate's document orders).
        let verified = &out.provenance.decision.as_ref().unwrap()["semantic_verification"];
        assert_eq!(
            verified[0]["defects"],
            json!(PARTS),
            "{label}: {verified:#}"
        );
        let notes: Vec<Value> = (PARTS.iter())
            .map(|part| json!({"defect": part, "note": "the judge points to the task approve"}))
            .collect();
        assert_eq!(verified[0]["notes"], json!(notes), "{label}: {verified:#}");
        let receipt = out.provenance.authoring.as_ref().unwrap();
        for call in &receipt.context[2..] {
            assert_eq!(
                call["result"]["usage_reported"], true,
                "{label}: the judgment call is kept"
            );
        }
        assert_eq!(
            native(&out)["rounds"][0]["proposed_sketch"]["outputs"],
            outputs,
            "{label}: the judged sketch is the record"
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.message.contains("does not carry")
                    && d.message.contains("nothing is READY")),
            "{label}: the refusing verdict is named: {:?}",
            out.diagnostics
        );
    }
}

// ── A program bound to several edges reads them all; its input stays the sketch's ─────────────

#[tokio::test]
async fn a_fill_never_replaces_the_input_a_program_reads_from_its_edges() {
    // The composed graph, with the alpha write fed by a program over both reads.
    let mut graph = tasks();
    graph.insert(
        2,
        task(
            "join",
            "nika:jq",
            &json!({"with": [{"name": "alpha", "from": "read_alpha"}, {"name": "beta", "from": "read_beta"}]}),
        ),
    );
    graph[4]["with"] = json!([{"name": "text", "from": "join"}]);
    let hostile = json!({"fills": [
        {"task": "approve", "field": "args.message", "value": "Écrire les deux copies ?"},
        {"task": "join", "field": "args", "value": {"input": "${{ with.alpha }}"}},
        {"task": "join", "field": "expression", "value": ".alpha + .beta"}
    ], "notes": "fills"});
    let (out, calls) = compile(
        NativeMode::Sketch,
        0,
        vec![sketch_answer(&graph, Some(outputs())), hostile.to_string()],
    )
    .await;
    assert_eq!(calls, 2, "the sketch and its one fill round: {out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let fill = native(&out)["rounds"][1]["diagnostics"].to_string();
    assert!(
        fill.contains("`join.args` is owned by the sketch"),
        "{fill}"
    );
}

// ── A computation the plan cannot state: the sketch door next, no program round first (R5) ──

const PAIRS: &str = "Read ./punches.json, pair each employee's punches in time order and write the pairs to ./out/pairs.json.";

fn pairs_plan() -> String {
    json!({"steps": [
        {"op": "read", "detail": "./punches.json", "evidence": "Read ./punches.json"},
        {"op": "compute", "detail": "pair each employee's punches in time order", "evidence": "pair each employee's punches in time order"}
    ], "effects": [
        {"verb": "write", "target": "./out/pairs.json", "policy": "automatic", "evidence": "write the pairs to ./out/pairs.json"}
    ], "obligations": [], "constraints": [], "unknowns": []})
    .to_string()
}

#[tokio::test]
async fn a_computation_the_plan_cannot_state_goes_to_the_sketch_door_without_a_program_round() {
    // Under escalate the plan's own limit is observed before any program round is paid for: the
    // sketch door composes the request next, its program one typed fill.
    let escalate = Rotating::new(vec![pairs_plan(), "{}".to_owned()]);
    let request =
        CompileRequest::create(PAIRS).with_authoring_policy(policy(NativeMode::Escalate, 2));
    let out = compile_with_provider(&request, &escalate).await.unwrap();
    let called = roles(&out);
    assert_eq!(
        called.first().map(String::as_str),
        Some("plan"),
        "{called:?}"
    );
    assert!(!called.iter().any(|r| r == "transform"), "{called:?}");
    assert!(called.iter().any(|r| r == "sketch"), "{called:?}");
    assert!(
        route(&out).contains("compose: the plan's computation goes to the sketch door"),
        "{}",
        route(&out)
    );
    let door = &outcome_document(&out)["provenance"]["decision"]["forensic"]["door"];
    assert_eq!(
        door["reason"], "plan_computation_needs_the_sketch_door",
        "{door}"
    );
    // The plan alone (no sketch door) keeps its program round.
    let off = Rotating::new(vec![pairs_plan(), "{}".to_owned()]);
    let request = CompileRequest::create(PAIRS).with_authoring_policy(policy(NativeMode::Off, 2));
    let out = compile_with_provider(&request, &off).await.unwrap();
    assert_eq!(roles(&out)[..2], ["plan", "transform"], "{}", route(&out));
}
