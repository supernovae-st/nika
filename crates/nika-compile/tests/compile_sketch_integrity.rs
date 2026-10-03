// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Sketch emission integrity: a value a seat proposes can neither vanish silently nor redirect
//! the inputs, destinations, bindings or permissions the accepted graph owns. Every refusal
//! happens before a complete candidate is emitted, names the task and slot, and keeps no
//! refused text on the wire. Synthetic paths and scripted providers only: no network, no key.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, NativeMode, outcome_document};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};

mod common;
use common::{Judged, Rotating};

const CANARY: &str = "sk-integrity-canary-7f3e";

/// Two reads, a jq step reading the first by its edge, a write of its result.
const TWO_READS: &str = "Lis ./a.json et ./b.json, garde les lignes actives de ./a.json et écris le résultat dans ./a-out.json";

/// A read, a conversion whose input is its edge, a write of the converted text.
const CONVERT: &str =
    "Lis ./table.csv, convertis-le en JSON et écris le résultat dans ./table.json";

fn policy() -> AuthoringPolicy {
    policy_with(0)
}

fn policy_with(repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(repairs)
}

fn task(id: &str, verb: &str, tool: Option<&str>, extra: &Value) -> Value {
    let mut t = json!({"id": id, "verb": verb, "purpose": id});
    if let Some(tool) = tool {
        t["tool"] = json!(tool);
    }
    for (k, v) in extra.as_object().unwrap() {
        t[k] = v.clone();
    }
    t
}

fn two_reads_tasks() -> Vec<Value> {
    vec![
        task(
            "read_a",
            "invoke",
            Some("nika:read"),
            &json!({"reads": ["./a.json"]}),
        ),
        task(
            "read_b",
            "invoke",
            Some("nika:read"),
            &json!({"reads": ["./b.json"]}),
        ),
        task(
            "keep",
            "invoke",
            Some("nika:jq"),
            &json!({"with": [{"name": "rows", "from": "read_a"}, {"name": "other", "from": "read_b"}]}),
        ),
        task(
            "save",
            "invoke",
            Some("nika:write"),
            &json!({"writes": ["./a-out.json"], "with": [{"name": "result", "from": "keep"}]}),
        ),
    ]
}

fn sketch_answer(tasks: &[Value]) -> String {
    json!({"name": "keep-active", "tasks": tasks, "questions": [], "gaps": [], "notes": "graph"})
        .to_string()
}

fn fills_answer(fills: &Value) -> String {
    json!({"fills": fills, "notes": "fills"}).to_string()
}

fn expression() -> Value {
    json!({"task": "keep", "field": "expression", "value": "fromjson | map(select(.active == true))"})
}

async fn compile(intent: &str, replies: Vec<String>) -> (CompileOutcome, u32) {
    compile_with(intent, replies, policy()).await
}

async fn compile_with(
    intent: &str,
    replies: Vec<String>,
    policy: AuthoringPolicy,
) -> (CompileOutcome, u32) {
    let provider = Rotating::new(replies);
    // The whole-request judge after an accepted fill is approved by the explicit double, so the
    // count is the author's calls alone.
    let judged = Judged::approving(&provider);
    let request = CompileRequest::create(intent).with_authoring_policy(policy);
    let out = compile_with_provider(&request, &judged).await.unwrap();
    (out, provider.calls.load(Ordering::SeqCst))
}

fn rounds(out: &CompileOutcome) -> Vec<Value> {
    out.provenance.decision.as_ref().unwrap()["native"]["rounds"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The accepted candidate's document, as the plan record states it.
fn emitted(out: &CompileOutcome) -> Value {
    let source = out.provenance.plan.as_ref().unwrap()["source"]
        .as_str()
        .unwrap()
        .to_owned();
    serde_yaml_bw::from_str(&source).unwrap()
}

/// The fill round refused before emission: no candidate, no candidate digest, a diagnostic that
/// names `slot`, and no refused text anywhere on the wire.
fn refused_before_emission(out: &CompileOutcome, calls: u32, slot: &str, case: &str) {
    assert_eq!(calls, 2, "{case}: one sketch call, one fill call, no more");
    assert!(out.candidate.is_none(), "{case}: {out:#?}");
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["native"]["accepted"],
        false,
        "{case}"
    );
    let rounds = rounds(out);
    let fill = rounds
        .iter()
        .find(|r| r["phase"] == "fill")
        .expect("a fill round");
    assert!(
        fill.get("candidate_sha256").is_none_or(Value::is_null),
        "{case}: no complete candidate digest before validation: {fill:#}"
    );
    let messages = fill["diagnostics"].to_string();
    assert!(
        messages.contains(slot),
        "{case}: names `{slot}`: {messages}"
    );
    assert!(
        !outcome_document(out).to_string().contains(CANARY),
        "{case}: the refused value never reaches the wire"
    );
}

// ── Positives: the minimal valid graph still emits, exactly as the sketch owns it ──────────────

#[tokio::test]
async fn minimal_valid_fills_emit_the_graph_owned_source_destination_and_binding() {
    let (out, calls) = compile(
        TWO_READS,
        vec![
            sketch_answer(&two_reads_tasks()),
            fills_answer(&json!([expression()])),
        ],
    )
    .await;
    assert_eq!(calls, 2);
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["native"]["accepted"],
        true,
        "{out:#?}"
    );
    let doc = emitted(&out);
    let tasks = &doc["tasks"];
    assert_eq!(tasks["read_a"]["invoke"]["args"]["path"], "./a.json");
    assert_eq!(tasks["read_b"]["invoke"]["args"]["path"], "./b.json");
    assert_eq!(tasks["keep"]["invoke"]["args"]["input"], "${{ with.rows }}");
    assert_eq!(tasks["keep"]["with"]["rows"], "${{ tasks.read_a.output }}");
    assert_eq!(tasks["save"]["invoke"]["args"]["path"], "./a-out.json");
    assert_eq!(
        tasks["save"]["invoke"]["args"]["content"],
        "${{ with.result }}"
    );
    assert_eq!(doc["permits"]["fs"]["write"], json!(["./a-out.json"]));
}

#[tokio::test]
async fn an_optional_content_template_that_keeps_its_binding_is_accepted() {
    let content =
        json!({"task": "save", "field": "args.content", "value": "# Active\n${{ with.result }}\n"});
    let (out, _) = compile(
        TWO_READS,
        vec![
            sketch_answer(&two_reads_tasks()),
            fills_answer(&json!([expression(), content])),
        ],
    )
    .await;
    let doc = emitted(&out);
    assert_eq!(
        doc["tasks"]["save"]["invoke"]["args"]["content"],
        "# Active\n${{ with.result }}\n"
    );
}

#[tokio::test]
async fn a_whole_args_hole_keeps_its_derived_input() {
    let tasks = vec![
        task(
            "read_table",
            "invoke",
            Some("nika:read"),
            &json!({"reads": ["./table.csv"]}),
        ),
        task(
            "to_json",
            "invoke",
            Some("nika:convert"),
            &json!({"with": [{"name": "table", "from": "read_table"}]}),
        ),
        task(
            "save",
            "invoke",
            Some("nika:write"),
            &json!({"writes": ["./table.json"], "with": [{"name": "data", "from": "to_json"}]}),
        ),
    ];
    let valid =
        json!([{"task": "to_json", "field": "args", "value": {"from": "csv", "to": "json"}}]);
    let (out, _) = compile(CONVERT, vec![sketch_answer(&tasks), fills_answer(&valid)]).await;
    let doc = emitted(&out);
    assert_eq!(
        doc["tasks"]["to_json"]["invoke"]["args"],
        json!({"input": "${{ with.table }}", "from": "csv", "to": "json"})
    );
    // A whole-args object that replaces the derived input is refused, even with a stated value.
    let hijack = json!([{"task": "to_json", "field": "args",
        "value": {"input": CANARY, "from": "csv", "to": "json"}}]);
    let (out, calls) = compile(CONVERT, vec![sketch_answer(&tasks), fills_answer(&hijack)]).await;
    refused_before_emission(&out, calls, "to_json.args", "whole args input");
}

// ── Negatives: every malformed or overriding fill is refused before emission ────────────────────

#[tokio::test]
async fn every_fill_outside_the_holes_or_of_the_wrong_shape_is_refused_by_name() {
    let with = |extra: Value| json!([expression(), extra]);
    let cases: Vec<(&str, &str, Value)> = vec![
        (
            "ghost task",
            "fills[1]",
            with(json!({"task": "ghost", "field": "prompt", "value": CANARY})),
        ),
        (
            "undeclared field",
            "fills[1]",
            with(json!({"task": "keep", "field": "temperature", "value": CANARY})),
        ),
        (
            "duplicate fill",
            "keep.expression",
            with(json!({"task": "keep", "field": "expression", "value": CANARY})),
        ),
        (
            "missing value",
            "fills[0]",
            json!([{"task": "keep", "field": "expression"}]),
        ),
        (
            "null value",
            "keep.expression",
            json!([{"task": "keep", "field": "expression", "value": null}]),
        ),
        (
            "wrong value kind",
            "keep.expression",
            json!([{"task": "keep", "field": "expression", "value": 42}]),
        ),
        ("required hole unfilled", "keep.expression", json!([])),
        (
            "extra key in a fill",
            "fills[0]",
            json!([{"task": "keep", "field": "expression",
            "value": "fromjson", "api_key": CANARY}]),
        ),
        // A jq input redirected to the other read: both paths are permitted reads.
        (
            "args.input override",
            "keep.args.input",
            with(json!({"task": "keep", "field": "args.input", "value": "${{ with.other }}"})),
        ),
        // A read redirected to the other stated file, which the read permits already cover.
        (
            "args.path override",
            "read_a.args.path",
            with(json!({"task": "read_a", "field": "args.path", "value": "./b.json"})),
        ),
        (
            "whole args on a write",
            "save.args",
            with(
                json!({"task": "save", "field": "args", "value": {"path": "./a-out.json", "content": CANARY}}),
            ),
        ),
        // A content template that drops the edge the write is bound to.
        (
            "content without its binding",
            "save.args.content",
            with(json!({"task": "save", "field": "args.content", "value": CANARY})),
        ),
    ];
    for (case, slot, fills) in cases {
        let (out, calls) = compile(
            TWO_READS,
            vec![sketch_answer(&two_reads_tasks()), fills_answer(&fills)],
        )
        .await;
        refused_before_emission(&out, calls, slot, case);
    }
}

// ── Negatives: a malformed graph is refused before any fill ─────────────────────────────────────

#[tokio::test]
async fn a_malformed_graph_is_refused_by_name_before_any_fill() {
    let mutate = |k: usize, key: &str, value: Value| {
        let mut tasks = two_reads_tasks();
        tasks[k][key] = value;
        tasks
    };
    let cases: Vec<(&str, &str, Vec<Value>)> = vec![
        (
            "unknown task key",
            "tasks[0]",
            mutate(0, "api_key", json!(CANARY)),
        ),
        (
            "non-string path item",
            "tasks[0].reads",
            mutate(0, "reads", json!(["./a.json", 7])),
        ),
        (
            "reads not an array",
            "tasks[0].reads",
            mutate(0, "reads", json!("./a.json")),
        ),
        (
            "non-string tool",
            "tasks[2].tool",
            mutate(2, "tool", json!(7)),
        ),
        ("non-string id", "tasks[1].id", mutate(1, "id", json!(7))),
        (
            "edge without from",
            "tasks[2].with[0]",
            mutate(2, "with", json!([{"name": "rows"}])),
        ),
        (
            "edge with an extra key",
            "tasks[2].with[0]",
            mutate(
                2,
                "with",
                json!([{"name": "rows", "from": "read_a", "token": CANARY}]),
            ),
        ),
        (
            "empty edge name",
            "keep",
            mutate(2, "with", json!([{"name": "", "from": "read_a"}])),
        ),
        (
            "invalid edge name",
            "keep",
            mutate(2, "with", json!([{"name": "rows-a", "from": "read_a"}])),
        ),
        (
            "repeated edge name",
            "keep",
            mutate(
                2,
                "with",
                json!([{"name": "rows", "from": "read_a"}, {"name": "rows", "from": "read_b"}]),
            ),
        ),
    ];
    for (case, slot, tasks) in cases {
        let (out, calls) = compile(
            TWO_READS,
            vec![sketch_answer(&tasks), fills_answer(&json!([expression()]))],
        )
        .await;
        assert_eq!(calls, 1, "{case}: the refused graph is never filled");
        assert!(out.candidate.is_none(), "{case}");
        let rounds = rounds(&out);
        assert_eq!(rounds.len(), 1, "{case}: {rounds:#?}");
        let messages = rounds[0]["diagnostics"].to_string();
        assert!(
            messages.contains(slot),
            "{case}: names `{slot}`: {messages}"
        );
        assert!(
            !outcome_document(&out).to_string().contains(CANARY),
            "{case}"
        );
    }
}

/// A fetch of a stated page, its text written to a stated file.
const FETCH: &str = "Récupère https://example.org/news et écris le texte dans ./news.md";

fn fetch_tasks() -> Vec<Value> {
    vec![
        task(
            "get_news",
            "invoke",
            Some("nika:fetch"),
            &json!({"hosts": ["example.org"]}),
        ),
        task(
            "save",
            "invoke",
            Some("nika:write"),
            &json!({"writes": ["./news.md"], "with": [{"name": "page", "from": "get_news"}]}),
        ),
    ]
}

#[tokio::test]
async fn a_fetch_mode_is_judged_by_the_builtin_contract_before_emission() {
    let url = json!({"task": "get_news", "field": "args.url", "value": "https://example.org/news"});
    let valid = json!([url, {"task": "get_news", "field": "args.mode", "value": "text"}]);
    let (out, _) = compile(
        FETCH,
        vec![sketch_answer(&fetch_tasks()), fills_answer(&valid)],
    )
    .await;
    assert_eq!(
        emitted(&out)["tasks"]["get_news"]["invoke"]["args"]["mode"],
        "text"
    );
    // A mode outside the closed extract set is refused by the fetch contract, unechoed.
    let unknown = json!([url, {"task": "get_news", "field": "args.mode", "value": CANARY}]);
    let (out, calls) = compile(
        FETCH,
        vec![sketch_answer(&fetch_tasks()), fills_answer(&unknown)],
    )
    .await;
    refused_before_emission(&out, calls, "get_news", "unknown extract mode");
    let diagnostics = rounds(&out)[1]["diagnostics"].to_string();
    assert!(diagnostics.contains("nika:fetch"), "{diagnostics}");
}

#[tokio::test]
async fn a_refused_fill_round_is_repaired_within_the_same_budget() {
    let ghost = json!([expression(), {"task": "ghost", "field": "prompt", "value": CANARY}]);
    let (out, calls) = compile_with(
        TWO_READS,
        vec![
            sketch_answer(&two_reads_tasks()),
            fills_answer(&ghost),
            fills_answer(&json!([expression()])),
        ],
        policy_with(1),
    )
    .await;
    // One sketch, one refused fill, one repaired fill: the existing budget, no reset.
    assert_eq!(calls, 3);
    let rounds = rounds(&out);
    assert_eq!(rounds.len(), 3, "{rounds:#?}");
    assert!(
        rounds[1].get("candidate_sha256").is_none(),
        "{:#}",
        rounds[1]
    );
    assert!(rounds[1]["diagnostics"].to_string().contains("fills[1]"));
    assert!(rounds[2]["candidate_sha256"].is_string(), "{:#}", rounds[2]);
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["native"]["accepted"],
        true
    );
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.context[1]["call"], "fill");
    assert_eq!(receipt.context[2]["call"], "fill-repair");
    assert!(!outcome_document(&out).to_string().contains(CANARY));
}

// ── A2 · a builtin's own path argument is bound to its task's stated reach ─────────────────────

/// One builtin whose path argument the sketch does not derive, its arguments around that path,
/// and two paths of the same kind: the task's own and another task's.
struct PathTool {
    tool: &'static str,
    path_arg: &'static str,
    own: &'static str,
    other: &'static str,
    args: Value,
}

fn path_tools() -> Vec<PathTool> {
    vec![
        PathTool {
            tool: "nika:chart",
            path_arg: "out",
            own: "./out/a.svg",
            other: "./out/b.svg",
            args: json!({"data": [{"x": "a", "y": 1}], "chart": {"type": "bar", "x": "x", "y": "y"}}),
        },
        PathTool {
            tool: "nika:image_fx",
            path_arg: "out",
            own: "./out/a.png",
            other: "./out/b.png",
            args: json!({"input": "./in.png", "ops": [{"grayscale": {}}]}),
        },
        PathTool {
            tool: "nika:image_generate",
            path_arg: "output_dir",
            own: "./out/a",
            other: "./out/b.md",
            args: json!({"prompt": "a lighthouse at dawn"}),
        },
        PathTool {
            tool: "nika:tts_generate",
            path_arg: "output_dir",
            own: "./out/a",
            other: "./out/b.md",
            args: json!({"text": "hello"}),
        },
    ]
}

/// `make` runs the tool and writes its own path; `keep` writes the other path from its output.
fn path_tool_answer(case: &PathTool) -> String {
    let mut make = json!({"writes": [case.own]});
    if case.tool == "nika:image_fx" {
        make["reads"] = json!(["./in.png"]);
    }
    sketch_answer(&[
        task("make", "invoke", Some(case.tool), &make),
        task(
            "keep",
            "invoke",
            Some("nika:write"),
            &json!({"writes": [case.other], "with": [{"name": "made", "from": "make"}]}),
        ),
    ])
}

fn path_tool_intent(case: &PathTool) -> String {
    let input = if case.tool == "nika:image_fx" {
        " à partir de ./in.png"
    } else {
        ""
    };
    format!(
        "Produis le résultat{input} dans {} puis enregistre son compte rendu dans {}",
        case.own, case.other
    )
}

fn path_tool_fill(case: &PathTool, path: &str) -> Value {
    let mut args = case.args.clone();
    args[case.path_arg] = json!(path);
    json!([{"task": "make", "field": "args", "value": args}])
}

#[tokio::test]
async fn a_builtin_path_argument_cannot_point_at_another_tasks_permitted_path() {
    for case in path_tools() {
        let intent = path_tool_intent(&case);
        // Positive: the tool's own stated path is emitted exactly.
        let (out, _) = compile(
            &intent,
            vec![
                path_tool_answer(&case),
                fills_answer(&path_tool_fill(&case, case.own)),
            ],
        )
        .await;
        let accepted = out.provenance.decision.as_ref().unwrap()["native"]["accepted"] == true;
        assert!(
            accepted,
            "{}: own path accepted: {:#}",
            case.tool,
            rounds(&out).last().unwrap()
        );
        assert_eq!(
            emitted(&out)["tasks"]["make"]["invoke"]["args"][case.path_arg],
            case.own,
            "{}",
            case.tool
        );
        // Hijack: the other task's path, which the derived permits already grant.
        let (out, calls) = compile(
            &intent,
            vec![
                path_tool_answer(&case),
                fills_answer(&path_tool_fill(&case, case.other)),
            ],
        )
        .await;
        refused_before_emission(&out, calls, "make", case.tool);
        let diagnostics = rounds(&out)[1]["diagnostics"].to_string();
        assert!(
            diagnostics.contains(case.path_arg),
            "{}: {diagnostics}",
            case.tool
        );
        assert!(
            !diagnostics.contains(case.other),
            "{}: never echoed: {diagnostics}",
            case.tool
        );
    }
}

// ── A2 · a fill's own names are untrusted input ─────────────────────────────────────────────────

#[tokio::test]
async fn an_undeclared_task_field_or_key_is_named_by_index_never_echoed() {
    const SHORT: &str = "zq7tok";
    const LONG: &str = "sk_canary_identifier_name_long_0123456789_abcdefghij";
    for name in [SHORT, LONG] {
        let cases: Vec<(&str, Value)> = vec![
            (
                "unknown task",
                json!([expression(), {"task": name, "field": "prompt", "value": "x"}]),
            ),
            (
                "unknown field",
                json!([expression(), {"task": "keep", "field": name, "value": "x"}]),
            ),
            (
                "unknown args field",
                json!([expression(), {"task": "keep", "field": format!("args.{name}"), "value": "x"}]),
            ),
            (
                "unknown key",
                json!([expression(), {"task": "keep", "field": "expression", "value": "x", name: 1}]),
            ),
            (
                "unknown key, unknown task",
                json!([expression(), {"task": name, "field": name, "value": "x", name: 1}]),
            ),
        ];
        for (case, fills) in cases {
            let (out, calls) = compile(
                TWO_READS,
                vec![sketch_answer(&two_reads_tasks()), fills_answer(&fills)],
            )
            .await;
            let case = format!("{case} ({name})");
            refused_before_emission(&out, calls, "fills[1]", &case);
            assert!(
                !outcome_document(&out).to_string().contains(name),
                "{case}: the name never reaches the record"
            );
        }
    }
}

#[tokio::test]
async fn an_image_fx_input_cannot_read_another_tasks_permitted_file() {
    let intent = "Lis ./other.png et ./in.png, applique un filtre à ./in.png dans ./out/a.png puis enregistre un compte rendu dans ./out/b.md";
    let answer = sketch_answer(&[
        task(
            "peek",
            "invoke",
            Some("nika:read"),
            &json!({"reads": ["./other.png"]}),
        ),
        task(
            "make",
            "invoke",
            Some("nika:image_fx"),
            &json!({"reads": ["./in.png"], "writes": ["./out/a.png"]}),
        ),
        task(
            "keep",
            "invoke",
            Some("nika:write"),
            &json!({"writes": ["./out/b.md"], "with": [{"name": "made", "from": "make"}, {"name": "seen", "from": "peek"}]}),
        ),
    ]);
    let fill = |input: &str| {
        json!([{"task": "make", "field": "args",
            "value": {"input": input, "out": "./out/a.png", "ops": [{"grayscale": {}}]}}])
    };
    let content = json!({"task": "keep", "field": "args.content", "value": "${{ with.made }} ${{ with.seen }}"});
    let with_content = |input: &str| {
        let mut fills = fill(input);
        fills.as_array_mut().unwrap().push(content.clone());
        fills
    };
    let (out, _) = compile(
        intent,
        vec![answer.clone(), fills_answer(&with_content("./in.png"))],
    )
    .await;
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["native"]["accepted"],
        true,
        "{:#}",
        rounds(&out).last().unwrap()
    );
    // `./other.png` is a permitted read of the sketch, but not of this task.
    let (out, calls) = compile(
        intent,
        vec![answer, fills_answer(&with_content("./other.png"))],
    )
    .await;
    refused_before_emission(&out, calls, "`input`", "image_fx input");
}

// ── A2b · a reach the tool always needs is judged while the sketch can still be repaired ───────

/// Sketch call, a refused sketch, its repair, then one fill: the graph defect is caught at the
/// sketch phase and repaired there, inside the same budget, never frozen into an unfillable round.
async fn repaired_graph(intent: &str, wrong: Vec<Value>, right: Vec<Value>, fills: &Value) {
    let (out, calls) = compile_with(
        intent,
        vec![
            sketch_answer(&wrong),
            sketch_answer(&right),
            fills_answer(fills),
        ],
        policy_with(1),
    )
    .await;
    let rounds = rounds(&out);
    assert_eq!(rounds[0]["phase"], "sketch", "{rounds:#?}");
    let first = rounds[0]["diagnostics"].to_string();
    assert!(
        first.contains("`make`"),
        "the sketch round names the task: {first}"
    );
    assert!(rounds[0].get("candidate_sha256").is_none());
    assert_eq!(calls, 3, "sketch, sketch-repair, fill: {rounds:#?}");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let roles: Vec<&str> = receipt
        .context
        .iter()
        .map(|c| c["call"].as_str().unwrap())
        .collect();
    // The existing whole-request judgment follows the accepted fill (approved by the double).
    assert_eq!(roles, ["sketch", "sketch-repair", "fill", "judge_request"]);
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["native"]["accepted"],
        true,
        "{:#}",
        rounds.last().unwrap()
    );
}

#[tokio::test]
async fn an_edit_path_stated_only_as_a_write_is_repaired_in_the_graph() {
    let edit = |extra: &Value| vec![task("make", "invoke", Some("nika:edit"), extra)];
    repaired_graph(
        "Remplace TODO par DONE dans ./notes.md",
        edit(&json!({"writes": ["./notes.md"]})),
        edit(&json!({"reads": ["./notes.md"], "writes": ["./notes.md"]})),
        &json!([{"task": "make", "field": "args", "value": {"find": "TODO", "replace": "DONE"}}]),
    )
    .await;
}

#[tokio::test]
async fn a_chart_that_states_no_write_is_repaired_in_the_graph_and_inline_rows_need_no_read() {
    let chart = |extra: &Value| vec![task("make", "invoke", Some("nika:chart"), extra)];
    repaired_graph(
        "Trace un graphique en barres dans ./out/a.svg",
        // A sibling write realizes the stated path, so only the chart's own reach is missing.
        {
            let mut wrong = chart(&json!({}));
            wrong.push(task(
                "keep",
                "invoke",
                Some("nika:write"),
                &json!({"writes": ["./out/a.svg"], "with": [{"name": "made", "from": "make"}]}),
            ));
            wrong
        },
        chart(&json!({"writes": ["./out/a.svg"]})),
        &json!([{"task": "make", "field": "args", "value": {
            "data": [{"x": "a", "y": 1}], "chart": {"type": "bar", "x": "x", "y": "y"}, "out": "./out/a.svg"}}]),
    )
    .await;
}
