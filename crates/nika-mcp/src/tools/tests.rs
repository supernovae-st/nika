// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

/// Every tool the oracle serves declares its behaviour, and names
/// itself. The hints are what a client reads to stop interrupting a
/// human on a read-only call; a blank title is a tool that never
/// introduced itself. Found 2026-07-28: all nine served NONE, so a
/// pure-read oracle felt exactly as dangerous as a shell.
#[test]
fn every_served_tool_declares_read_only_behaviour_and_a_title() {
    let catalog = catalog();
    let tools = catalog.as_array().expect("the catalog is an array");
    assert!(!tools.is_empty(), "the oracle serves no tools at all");
    for tool in tools {
        let name = tool["name"].as_str().expect("every tool has a name");
        assert!(
            !tool["title"].as_str().unwrap_or_default().is_empty(),
            "{name} serves no display title — add it to display_title()"
        );
        let hints = &tool["annotations"];
        assert_eq!(hints["readOnlyHint"], json!(true), "{name} readOnlyHint");
        assert_eq!(hints["destructiveHint"], json!(false), "{name} destructive");
        assert_eq!(hints["idempotentHint"], json!(true), "{name} idempotent");
        assert_eq!(hints["openWorldHint"], json!(false), "{name} openWorld");
    }
}

/// `nika_inspect` serves the projection VERBATIM — byte-equal with
/// `nika_graph::project` on the same source (one projector, three
/// protocols: this pin is the MCP leg of the LSP's parity law).
#[test]
fn inspect_serves_the_canonical_projection_verbatim() {
    let yaml = "nika: w\npermits: { exec: [\"true\"] }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"true\"] }\n";
    let out = inspect(&serde_json::json!({ "workflow": yaml })).expect("clean");
    let got: Value = serde_json::from_str(&out).expect("json");
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    let report = nika_check::check(&wf);
    let expected = serde_json::to_value(nika_graph::project(&wf, &report)).expect("serializes");
    assert_eq!(got, expected);
    assert_eq!(got["graph_format"], 3, "in-payload version");
}

/// Findings → null graph + the one-word reason (the LSP contract,
/// MCP leg) — never a projection of an unproven DAG.
#[test]
fn inspect_refuses_findings_with_a_reason() {
    let yaml = "nika: w\ntasks:\n  a:\n    after:\n      b: success\n    exec: { command: [\"true\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"true\"] }\n";
    let out = inspect(&serde_json::json!({ "workflow": yaml })).expect("answers");
    let got: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(got["graph"], Value::Null);
    assert_eq!(got["reason"], "findings");
}

/// #320 repro 3, closed: a hallucinated model over the MCP lane was
/// the LAST surface still auditing green — the rung now reds it with
/// the same payload keys the CLI --json lane carries.
#[test]
fn check_reds_a_bare_model_id_with_the_shared_rung() {
    let yaml = "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"gpt-5-turbo\" }\n";
    let err = check(&serde_json::json!({ "workflow": yaml })).expect_err("dirty");
    assert!(err.contains("\"models_resolve\": false"), "{err}");
    assert!(
        err.contains("gpt-5-turbo") && err.contains("bare model id"),
        "{err}"
    );
    assert!(
        err.contains("\"code\": \"NIKA-PROVIDER\""),
        "the prefix half stamps the FORM law: {err}"
    );
}

#[test]
fn check_reds_a_cataloged_but_unresolvable_provider() {
    let yaml = "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n";
    let err = check(&serde_json::json!({ "workflow": yaml })).expect_err("dirty");
    assert!(
        err.contains("`azure` does not resolve") || err.contains("provider `azure`"),
        "{err}"
    );
    assert!(
        !err.contains("NIKA-PROVIDER"),
        "azure class stays engine-local: {err}"
    );
}

#[test]
fn check_stays_clean_when_every_model_resolves() {
    let yaml = "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
    let ok = check(&serde_json::json!({ "workflow": yaml })).expect("clean");
    assert!(ok.contains("clean"), "{ok}");
}

#[test]
fn catalog_lists_the_validate_and_learn_tools() {
    let c = catalog();
    let names: Vec<&str> = c
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str))
        .collect();
    assert_eq!(
        names,
        [
            "nika_check",
            "nika_inspect",
            "nika_explain",
            "nika_schema",
            "nika_examples",
            "nika_template",
            "nika_canon",
            "nika_catalog",
            "nika_tools"
        ]
    );
    // Each tool carries a JSON-Schema inputSchema (the client validates args).
    for t in c.as_array().expect("array") {
        assert_eq!(t["inputSchema"]["type"], "object");
    }
}

/// #1184 · the tool an agent OBEYS must teach the filter, not just
/// carry the field. A description that says « pick REAL model ids
/// from here » over a list where a fifth of the rows cannot be
/// reached is an instruction to author a workflow that will not run.
#[test]
fn the_catalog_tool_tells_an_agent_to_filter_on_resolves() {
    let c = catalog();
    let entry = c
        .as_array()
        .expect("array")
        .iter()
        .find(|t| t["name"] == "nika_catalog")
        .expect("nika_catalog is advertised");
    let text = entry["description"].as_str().expect("a description");
    assert!(
        text.contains("`resolves`"),
        "the description must name the field it wants filtered: {text}",
    );
    assert!(
        text.contains("resolves` is true"),
        "the description must state the DIRECTION of the filter: {text}",
    );
}

/// #1184 · the payload itself. Marks come from the wire layer or
/// they come from nowhere — a projection where every row reads
/// `false` is `catalog_export()` with the chain dropped.
#[test]
fn the_catalog_payload_marks_what_this_binary_can_resolve() {
    let out = execute("nika_catalog", &json!({})).expect("nika_catalog runs");
    let value: Value = serde_json::from_str(&out).expect("nika_catalog emits JSON");
    let providers = value["providers"].as_array().expect("providers");
    let resolving: Vec<&str> = providers
        .iter()
        .filter(|p| p["resolves"] == Value::Bool(true))
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert!(
        !resolving.is_empty(),
        "zero resolving rows means the chain was dropped",
    );
    assert!(
        resolving.len() < providers.len(),
        "this build seats fewer vendors than the catalog carries — \
         a payload marking every row runnable is not measuring",
    );
    for id in &resolving {
        assert!(
            nika_providers::CANONICAL_IDS.contains(id),
            "`{id}` is marked resolving but no adapter carries it",
        );
    }
    for id in nika_providers::CANONICAL_IDS {
        if providers.iter().any(|p| p["id"] == id) {
            assert!(resolving.contains(&id), "`{id}` resolves but is unmarked");
        }
    }
}

#[test]
fn catalog_and_tools_payloads_are_the_versioned_wire_json() {
    let out = execute("nika_catalog", &json!({})).expect("nika_catalog runs");
    let value: Value = serde_json::from_str(&out).expect("nika_catalog emits JSON");
    assert_eq!(value["catalog_version"], 1, "the locked v1 wire marker");
    assert!(
        !value["providers"].as_array().expect("providers").is_empty(),
        "the embedded catalog is never empty",
    );

    let out = execute("nika_tools", &json!({})).expect("nika_tools runs");
    let value: Value = serde_json::from_str(&out).expect("nika_tools emits JSON");
    assert_eq!(value["tools_version"], 1, "the locked v1 wire marker");
    assert!(
        !value["tools"].as_array().expect("tools").is_empty(),
        "the embedded builtin set is never empty",
    );
}

/// A GREEN answer carries the affirmative contract, not just the
/// word « clean ».
///
/// The oracle used to return one line naming only the risk grade,
/// while the report beside it held the wave plan, the boundary in
/// force, the cost ceiling and the journey. Measured 2026-08-20: the
/// JSON key set is identical clean and dirty, so nothing was gated —
/// the facts were computed and dropped at the render. This tool's own
/// description says an agent consults it before handing a file to a
/// human, and a green is precisely when that agent has something to
/// report.
///
/// Asserts the FACTS, never the sentence: a pin that quotes its own
/// prose passes on a reword into nonsense.
#[test]
fn a_green_answer_carries_the_affirmative_contract() {
    let wf = "nika: t\nmodel: mock/echo\npermits:\n  fs:\n    write: [\"./out.md\"]\n  tools: [nika:write]\ntasks:\n  a:\n    invoke:\n      tool: nika:write\n      args: { path: \"./out.md\", content: \"x\" }\n";
    let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
    assert!(out.contains("clean"), "still a green verdict · {out}");
    for fact in [
        "task(s)",
        "wave(s)",
        "permits tools:nika:write write:./out.md",
        "est out",
        "destination(s)",
        "data ",
    ] {
        assert!(out.contains(fact), "a green names `{fact}` · {out}");
    }

    // A file with NO boundary must say so in the same breath, because
    // absent is zero authority and « clean » alone hides it.
    let bare =
        "nika: t\ntasks:\n  a:\n    invoke:\n      tool: nika:log\n      args: { message: hi }\n";
    let out = execute("nika_check", &json!({ "workflow": bare }));
    if let Ok(text) = out {
        assert!(
            text.contains("no permits declared"),
            "an absent boundary is named on the green line · {text}"
        );
    }
}

#[test]
fn check_a_clean_workflow_is_ok() {
    let wf = "nika: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
    let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
    assert!(out.contains("clean"), "{out}");
}

/// The workflow that leaves the native path: every one of the TEN
/// finding surfaces is clean, and it still must not read as green.
///
/// This is the false green the operator met in Cursor. An agent
/// consults THIS oracle before handing a file over; it used to get a
/// bare "✔ clean" that named nothing, while the identical file
/// failed `nika check --native-strict` in the shell and was refused
/// by the hook in front of `nika run`. The `is_clean` mirror law — a
/// check that fails the shell must not read as success over MCP —
/// is what this pins. (The fixture declares its `net.http`: post-D1
/// the exec URL is a net USE — undeclared it would be a PERMITS
/// escape, and this test row is about the hint gate, not the escape.)
const LEAVES_THE_NATIVE_PATH: &str = "nika: t\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  grab:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";

#[test]
fn check_is_strict_about_paid_ready_by_default() {
    let wf = "nika: t\nmodel: mock/echo\ntasks:\n  judge:\n    infer:\n      prompt: |\n        Read the note and assign a belt.\n      max_tokens: 32\noutputs:\n  r: ${{ tasks.judge.output }}\n";
    let err = execute("nika_check", &json!({ "workflow": wf }))
        .expect_err("an infer that names the law is not a green by default");
    assert!(err.contains("paid-ready"), "{err}");
    assert!(err.contains("infer-as-law"), "{err}");
    assert!(
        err.contains("13-extract-then-law"),
        "the refusal must name the one-way: {err}"
    );
}

#[test]
fn check_is_strict_about_the_native_path_by_default() {
    let err = execute("nika_check", &json!({ "workflow": LEAVES_THE_NATIVE_PATH }))
        .expect_err("an exec a builtin covers is not a green by default");
    assert!(err.contains("native-first"), "{err}");
    assert!(
        err.contains("nika:fetch"),
        "the refusal must name the builtin that replaces it: {err}"
    );
}

#[test]
fn advisory_mode_still_names_what_it_did_not_enforce() {
    // Opting out must not hand back the SAME sentence a genuinely
    // clean workflow gets — that would just relocate the false
    // green behind a flag.
    let out = execute(
        "nika_check",
        &json!({ "workflow": LEAVES_THE_NATIVE_PATH, "native_strict": false }),
    )
    .expect("advisory mode returns a verdict");
    assert!(out.contains("advisory"), "{out}");
    assert!(out.contains("native-first"), "{out}");
    assert!(
        out.contains("--native-strict"),
        "advisory mode must say which posture WOULD refuse it: {out}"
    );
}

#[test]
fn the_strict_flag_is_declared_on_the_tool_that_honours_it() {
    let listed = catalog();
    let tools = listed.as_array().expect("a tool array");
    let check_tool = tools
        .iter()
        .find(|t| t["name"] == "nika_check")
        .expect("nika_check is served");
    let strict = &check_tool["inputSchema"]["properties"]["native_strict"];
    assert_eq!(
        strict["default"],
        json!(true),
        "the agent-facing oracle defaults to the posture its run gate uses"
    );
}

#[test]
fn check_a_broken_workflow_is_an_error_carrying_the_findings() {
    // A dangling `after:` edge — a DAG finding the ladder catches.
    // Dirty is an `Err` (→ isError:true) so a wired agent's repair
    // loop triggers, mirroring the CLI's exit-2-on-dirty; the full
    // report still rides the text so the model repairs from it.
    let wf =
        "nika: t\ntasks:\n  a:\n    after:\n      ghost: success\n    exec: { command: [\"x\"] }\n";
    let err = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
    assert!(err.contains("findings") && err.contains("NIKA-"), "{err}");
}

fn dirty_payload(workflow: &str) -> (String, Value) {
    let error =
        execute("nika_check", &json!({ "workflow": workflow })).expect_err("fixture is dirty");
    let json_start = error.find('{').expect("the full report rides the error");
    let payload = serde_json::from_str(&error[json_start..]).expect("valid report JSON");
    (error, payload)
}

#[test]
fn empty_source_names_its_deduplicated_explain_action_exactly() {
    let (_, payload) = dirty_payload("");
    assert_eq!(
        payload["next_actions"],
        json!(["nika_explain NIKA-PARSE-002"]),
        "{payload:#}"
    );
    // Two PARSE-002 rows (missing `nika` · missing `tasks`) share one
    // next action. The row `message` also carries the diagnostic
    // hand-off (`· → nika explain …`); do not count that phrase in
    // the serialized report — `next_actions` is the dedup clock.
    let findings = payload["findings"].as_array().expect("findings[]");
    assert_eq!(
        findings.len(),
        2,
        "empty source refuses both envelope fields: {payload:#}"
    );
    assert!(
        findings
            .iter()
            .all(|f| f["gate"] == "PARSE" && f["kind"] == "parse"),
        "PARSE codes must not wear the CONFORM ladder: {payload:#}"
    );
    assert_eq!(
        payload["next_actions"].as_array().map(Vec::len),
        Some(1),
        "duplicate findings must not duplicate the next action: {payload:#}"
    );
}

#[test]
fn dirty_actions_match_the_distinct_serialized_codes_one_for_one() {
    let wf = "nika: t\ntasks:\n  a:\n    after: { ghost: success }\n    exec: { command: [\"x\"] }\n  b:\n    with:\n      x: ${{ tasks.ghost.output }}\n      y: ${{ tasks.ghost.output }}\n    exec: { command: [\"x\"] }\n";
    let (_, payload) = dirty_payload(wf);
    let mut codes = payload["findings"]
        .as_array()
        .expect("aggregated findings")
        .iter()
        .filter_map(|finding| finding["code"].as_str())
        .collect::<Vec<_>>();
    let finding_count = codes.len();
    codes.sort_unstable();
    codes.dedup();
    let expected = codes
        .iter()
        .map(|code| format!("nika_explain {code}"))
        .collect::<Vec<_>>();

    assert!(
        codes.len() >= 2,
        "fixture must carry multiple codes: {payload:#}"
    );
    assert!(
        finding_count > codes.len(),
        "fixture must carry a duplicate code: {payload:#}"
    );
    assert_eq!(payload["next_actions"], json!(expected), "{payload:#}");
}

#[test]
fn check_missing_arg_is_a_tool_error() {
    let error = execute("nika_check", &json!({})).expect_err("workflow source is required");
    assert_eq!(error, "missing `workflow` (the *.nika.yaml source)");
    assert!(
        !error.contains("nika explain"),
        "no source means no code: {error}"
    );
}

#[test]
fn check_surfaces_non_conformance_findings_too() {
    // A schema whose `required` key is absent from `properties` COMPILES
    // (legal JSON Schema · no PARSE-019) but is a satisfiability smell →
    // it lands in `schema_lints`, NOT `conformance`. The old code rendered
    // only `conformance` → an empty "✖ findings" body for this whole class
    // (the P1). The full-report render must surface it.
    let wf = "nika: t\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: x\n      max_tokens: 10\n      schema: { type: object, properties: { a: { type: string } }, required: [b] }\n";
    let out = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
    assert!(out.contains("findings"), "flags not-clean: {out}");
    assert!(
        out.contains("schema_lints") && out.contains('b'),
        "the non-conformance finding (required `b` not in properties) is rendered, \
         not dropped: {out}"
    );
}

/// The thinking laws ride the same `model_findings` rail as the resolver
/// refusals (the CLI twin's fold at `check/mod.rs` — pinned negative).
#[test]
fn check_surfaces_the_thinking_laws_like_the_cli_twin() {
    let wf = "nika: t\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: x\n      max_tokens: 10\n      thinking: { enabled: true, budget_tokens: 10 }\n";
    let out = execute("nika_check", &json!({ "workflow": wf }))
        .expect_err("a thinking-law violation is dirty");
    assert!(
        out.contains("model_findings") && out.contains("budget_tokens"),
        "the thinking finding rides model_findings, CLI-shaped: {out}"
    );
}
/// the risk grade on EVERY audited card — a bare « ✔ clean » that
/// never names the rope is the false-green shape this oracle exists to
/// kill (a declared effect is Supervised, and the agent reading this
/// verdict must see it).
#[test]
fn the_clean_verdict_names_the_risk_grade() {
    let wf = "nika: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
    let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
    assert!(out.contains("clean"), "{out}");
    assert!(
        out.contains("risk supervised"),
        "the grade rides the clean verdict: {out}"
    );
}

/// The dirty lane IS the machine contract (a model repairs from this
/// JSON) — it must carry the same `risk_grade` key the CLI `--json`
/// verdict stamps (lowercase, like `check --json`), or the two machine
/// lanes disagree about the one verdict.
#[test]
fn the_findings_payload_carries_the_risk_grade_like_the_cli_json_lane() {
    // A dangling `after:` edge — dirty, no grants, nothing uncapped.
    let wf =
        "nika: t\ntasks:\n  a:\n    after:\n      ghost: success\n    exec: { command: [\"x\"] }\n";
    let err = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
    let json_start = err.find('{').expect("the report rides the error as JSON");
    let payload: Value =
        serde_json::from_str(&err[json_start..]).expect("the report is valid JSON");
    assert_eq!(
        payload["risk_grade"],
        json!("low"),
        "lowercase, like the CLI lane: {payload:#}"
    );
}

/// The field tracks the REPORT, it is not a stamped constant: an
/// agent loop without `max_tokens_total` (the audit's P0-6 fixture)
/// grades Unbounded even behind a findings verdict.
#[test]
fn the_findings_payloads_grade_reflects_the_report() {
    let wf = "nika: t\nmodel: anthropic/claude-sonnet-4-6\npermits: { tools: [\"nika:read\"] }\ntasks:\n  a:\n    agent: { prompt: go, tools: [\"nika:read\"], max_turns: 100 }\n    after: { ghost: success }\n";
    let err = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
    let json_start = err.find('{').expect("the report rides the error as JSON");
    let payload: Value =
        serde_json::from_str(&err[json_start..]).expect("the report is valid JSON");
    assert_eq!(
        payload["risk_grade"],
        json!("unbounded"),
        "max_turns bounds turns, never tokens: {payload:#}"
    );
}

#[test]
fn explain_a_known_code_teaches_it() {
    let out = execute("nika_explain", &json!({ "code": "NIKA-VAR-001" })).expect("ran");
    assert!(!out.is_empty());
    let bare = execute("nika_explain", &json!({ "code": "VAR-001" })).expect("ran");
    assert_eq!(out, bare, "the bare form normalizes to the same code");
}

#[test]
fn explain_mirrors_the_complete_value_and_exec_finding_lessons() {
    for (source, code) in [
        (
            "nika: teaching\nvars: {x: hi}\ntasks: {}\n",
            "NIKA-VALUES-001",
        ),
        (
            "nika: teaching\ntasks:\n  say:\n    exec: ls -la\n",
            "NIKA-PARSE-019",
        ),
        (
            "nika: teaching\ntasks:\n  say:\n    exec: {command: 'ls -la'}\n",
            "NIKA-PARSE-019",
        ),
    ] {
        let refusal = nika_schema::parse(
            source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect_err("fixture is refused");
        let lesson = match &refusal {
            nika_schema::SchemaError::Validation { message, .. } => message.clone(),
            _ => refusal.to_string(),
        };
        let message = execute("nika_check", &json!({ "workflow": source }))
            .expect_err("parse refusal is the actual MCP error");
        let cli_words = message.replace("`nika_check` with `fix: true`", "`nika check --fix`");
        assert!(cli_words.contains(&lesson), "{message}");
        let explained = execute("nika_explain", &json!({"code": code})).expect("explained");
        let oracle_words = lesson.replace("`nika check --fix`", "`nika_check` with `fix: true`");
        assert!(explained.contains(&oracle_words), "{explained}");
        assert!(!explained.contains("`nika check --fix`"), "{explained}");
    }
}

#[test]
fn explain_an_unknown_code_is_a_tool_error() {
    assert!(execute("nika_explain", &json!({ "code": "NIKA-GHOST-999" })).is_err());
}

#[test]
fn explain_a_printed_hint_kind_teaches_like_the_cli() {
    let out = execute("nika_explain", &json!({ "code": "jq-as-map" })).expect("hint kind");
    assert!(out.contains("jq-as-map · hint"), "{out}");
    assert!(out.contains("($name | map"), "{out}");
    let numbered = execute("nika_explain", &json!({ "code": "native-first/006" })).expect("006");
    assert!(numbered.contains("nika:wait"), "{numbered}");
}

/// One voice with the CLI (gauntlet 2026-07-12): a failed run's
/// per-builtin / per-provider code must TEACH over MCP too — the
/// agent debugging a trace calls this tool, not the terminal.
/// PROMPT-001 carries the full contract lesson (first-run gate ·
/// 2026-07-31): the agent reads the same exits the CLI teaches.
#[test]
fn explain_teaches_the_runtime_namespaces_like_the_cli() {
    let b = execute(
        "nika_explain",
        &json!({ "code": "NIKA-BUILTIN-PROMPT-001" }),
    )
    .expect("builtin namespace teaches");
    assert!(b.contains("the `nika:prompt` contract") && b.contains("on_codes"));
    assert!(
        b.contains("--answer <task>=<value>") && b.contains("`default:`"),
        "the contract lesson rides MCP too: {b}"
    );
    // A builtin WITHOUT a contract entry keeps the namespace voice.
    let generic = execute("nika_explain", &json!({ "code": "NIKA-BUILTIN-FETCH-001" }))
        .expect("namespace teaches");
    assert!(
        generic.contains("per-builtin runtime diagnostic"),
        "{generic}"
    );
    let p = execute("nika_explain", &json!({ "code": "NIKA-PROVIDER-007" }))
        .expect("provider namespace teaches");
    assert!(p.contains("provider-adapter"));
}

#[test]
fn explain_teaches_jq_value_input_through_the_shared_lesson() {
    let out = execute("nika_explain", &json!({ "code": "NIKA-BUILTIN-JQ-001" }))
        .expect("jq contract teaches");
    assert!(
        out.contains("Strings are valid input and stay strings"),
        "{out}"
    );
    assert!(out.contains("expression: fromjson"), "{out}");
    assert!(out.contains("specific cause"), "{out}");
}

#[test]
fn unknown_tool_is_an_error() {
    assert!(execute("nika_nonexistent", &json!({})).is_err());
}

// ── the LEARNING surface (agents can learn, not just validate) ──────

#[test]
fn catalog_lists_the_learning_tools_too() {
    let c = catalog();
    let names: Vec<&str> = c
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str))
        .collect();
    for expected in [
        "nika_check",
        "nika_inspect",
        "nika_explain",
        "nika_schema",
        "nika_examples",
        "nika_template",
        "nika_canon",
    ] {
        assert!(names.contains(&expected), "{expected} missing: {names:?}");
    }
}

#[test]
fn schema_returns_the_embedded_json_schema() {
    let out = execute("nika_schema", &json!({})).expect("ran");
    // The real schema: parses as JSON and declares the canonical $id.
    let v: Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(
        v["$id"], "https://nika.sh/spec/v1/workflow.schema.json",
        "the canonical $id travels with the schema"
    );
}

#[test]
fn examples_without_slug_returns_a_metadata_derived_index() {
    let out = execute("nika_examples", &json!({})).expect("ran");
    let slugs = nika_pack::example_slugs();
    let rows: Vec<Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).expect("each index row is JSON"))
        .collect();
    assert_eq!(rows.len(), slugs.len(), "one row per embedded example");
    assert!(!rows.is_empty(), "the pack ships examples");

    for (row, slug) in rows.iter().zip(&slugs) {
        let body = nika_pack::example(slug).expect("a listed slug resolves");
        let meta = nika_pack::meta(slug, body);
        assert_eq!(row["slug"], slug.as_str(), "slug derives from the pack");
        assert_eq!(row["form"], json!(meta.verbs), "form derives from verbs");
        assert_eq!(
            row["one_line"], meta.title,
            "one line derives from the header"
        );
        assert_eq!(
            row["cost"],
            json!({ "tasks": meta.tasks }),
            "cost derives from the task count"
        );
        assert_eq!(
            row.as_object().expect("row object").len(),
            4,
            "the index contract is exactly slug · form · one_line · cost"
        );
    }
}

#[test]
fn examples_with_slug_returns_the_source() {
    let slug = nika_pack::example_slugs()
        .first()
        .cloned()
        .expect("pack has examples");
    let out = execute("nika_examples", &json!({ "slug": slug })).expect("ran");
    assert!(
        out.contains("nika: hello"),
        "a real workflow source (nine-key identity): {out}"
    );
}

#[test]
fn examples_with_unknown_slug_is_a_tool_error_naming_the_list() {
    let err =
        execute("nika_examples", &json!({ "slug": "no-such-example" })).expect_err("unknown slug");
    assert!(err.contains("unknown example"), "{err}");
}

/// RAMS-11: the oracle walks the CLI's routing door on plain words —
/// the SAME query `nika new` routes lands the SAME entry here, and
/// the interpretation is SAID in a leading YAML comment.
#[test]
fn examples_route_plain_words_through_the_one_door() {
    let out =
        execute("nika_examples", &json!({ "slug": "chase unpaid invoices" })).expect("routes");
    assert!(
        out.starts_with("# routed: `chase unpaid invoices` → example `invoice-chaser`"),
        "the routing is said: {out}"
    );
    assert!(out.contains("nika: "), "the body follows: {out}");
}

/// RAMS-11, the honest floor: plain words below the confidence bar
/// clarify with the closest names — never a silent guess.
#[test]
fn examples_clarify_below_the_bar_instead_of_guessing() {
    let err = execute("nika_examples", &json!({ "slug": "do stuff with things" }))
        .expect_err("vague words clarify");
    assert!(err.contains("doesn't route confidently"), "{err}");
    assert!(err.contains("·"), "names candidates: {err}");
}

/// The slug/name is a KEY into the compile-time embedded pack
/// (`nika_pack` · `include_dir!`), never a filesystem path — so path
/// traversal, absolute paths, null bytes and injection are structurally
/// impossible to turn into a read, not merely defended. This guards that
/// invariant: any refactor that makes `example()`/`template()` touch the
/// fs from the argument fails here. (Backed by the 2026-07-03 adversarial
/// MCP e2e: 10/10 abusive slugs → clean errors, 0 leaks.)
#[test]
fn examples_and_templates_reject_adversarial_keys_as_plain_lookups() {
    let evil = [
        "../../../etc/passwd",
        "/etc/passwd",
        "%2e%2e%2f",
        "01-hello/../../secret",
        "inject\n\rion",
        "01-hello\0x",
    ];
    for key in evil {
        let e = execute("nika_examples", &json!({ "slug": key }))
            .expect_err("adversarial slug must be an unknown-key error");
        assert!(
            e.contains("unknown example") && !e.contains("root:"),
            "traversal leaked or crashed for {key:?}: {e}"
        );
        let t = execute("nika_template", &json!({ "name": key }))
            .expect_err("adversarial template name must be an unknown-key error");
        assert!(t.contains("unknown template"), "for {key:?}: {t}");
    }
}

#[test]
fn template_without_name_lists_the_skeletons() {
    let out = execute("nika_template", &json!({})).expect("ran");
    assert!(out.contains("chain"), "the chain skeleton is listed: {out}");
}

#[test]
fn template_with_name_returns_the_skeleton() {
    let out = execute("nika_template", &json!({ "name": "chain" })).expect("ran");
    assert!(
        out.contains("nika: ") && out.contains("SLOT"),
        "a fillable skeleton with SLOT markers: {out}"
    );
}

#[test]
fn canon_returns_the_ssot() {
    let out = execute("nika_canon", &json!({})).expect("ran");
    assert!(
        out.contains("verbs") && out.contains("builtins"),
        "the canon SSOT covers verbs + builtins: {out}"
    );
}

/// W3-F2 · this oracle reads no files: a composed workflow's CLEAN
/// answer says its children went unjudged, and names them.
#[test]
fn a_composed_workflows_clean_answer_names_its_unjudged_children() {
    let parent = "nika: parent\ntasks:\n  child:\n    invoke: { workflow: ./child.nika.yaml }\n";
    let ok = execute(
        "nika_check",
        &json!({ "workflow": parent, "native_strict": false }),
    )
    .unwrap_or_else(|e| panic!("clean on the source: {e}"));
    assert!(ok.contains("✔ clean"), "{ok}");
    assert!(
        ok.contains("composition unjudged") && ok.contains("./child.nika.yaml"),
        "the clean answer names what it did not read: {ok}"
    );
    let verbose = execute(
        "nika_check",
        &json!({ "workflow": parent, "native_strict": false, "verbose": true }),
    )
    .expect("clean");
    let start = verbose
        .find('{')
        .expect("the object rides the verbose answer");
    let obj: Value = serde_json::from_str(&verbose[start..]).expect("valid JSON");
    assert_eq!(obj["judged"]["composition"], false, "{obj:#}");
    assert_eq!(
        obj["judged"]["children"],
        json!(["./child.nika.yaml"]),
        "{obj:#}"
    );
}

/// W3-F8 · `verbose: true` returns the verdict object on a clean answer
/// — the same keys a dirty answer carries; without it the answer stays
/// the prose line.
#[test]
fn verbose_returns_the_verdict_object_on_a_clean_answer() {
    let yaml = "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
    let terse = execute("nika_check", &json!({ "workflow": yaml })).expect("clean");
    assert!(!terse.contains("\"verdicts\""), "terse by default: {terse}");
    let verbose =
        execute("nika_check", &json!({ "workflow": yaml, "verbose": true })).expect("clean");
    let start = verbose.find('{').expect("object");
    let obj: Value = serde_json::from_str(&verbose[start..]).expect("valid JSON");
    assert_eq!(obj["clean"], true, "{obj:#}");
    assert_eq!(obj["verdicts"]["valid"], true, "{obj:#}");
    assert!(
        obj.get("risk_grade").is_some() && obj.get("judged").is_some(),
        "{obj:#}"
    );
}

/// W3-F7 · the oracle's next actions name the door an agent without a
/// shell can open.
#[test]
fn next_actions_name_the_oracles_own_door() {
    let (_, payload) = dirty_payload("nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n");
    let actions = payload["next_actions"].as_array().expect("actions");
    assert!(!actions.is_empty(), "{payload:#}");
    assert!(
        actions
            .iter()
            .all(|a| a.as_str().is_some_and(|s| s.starts_with("nika_explain "))),
        "{actions:?}"
    );
}
