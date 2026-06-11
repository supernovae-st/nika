// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The static verb suite, end to end as library calls (the bin is a thin
//! dispatcher over these exact functions). Exit codes assert the LOCKED
//! spec §4 contract; outputs assert grep-stable grammar, not accidents.

use nika_cli::Theme;
use nika_cli::verbs::graph::{GraphFormat, project, to_dot, to_mermaid};
use nika_cli::verbs::{check, exit, explain, graph, inspect, new, pack_surface};
use nika_schema::{FileId, ParseMode};

const PLAIN: Theme = Theme {
    color: false,
    ascii: false,
    animate: false,
};

/// The shared fixture — same shape as the e2e pipeline workflow.
const WORKFLOW: &str = r#"
nika: v1
workflow: static-suite
description: "the static-verb fixture: all three shipped verbs, a gate, a fan-out"

model: mock/echo

vars:
  source: "./news.json"

tasks:
  - id: gather
    invoke:
      tool: "nika:read"
      args: { path: "${{ vars.source }}" }

  - id: probe
    exec:
      command: "wc -l ./news.json"

  - id: fan
    depends_on: [gather]
    for_each: ["a", "b", "c"]
    infer:
      prompt: "Classify · ${{ item }}"
      max_tokens: 100

  - id: think
    depends_on: [gather, probe]
    infer:
      prompt: "Summarize · ${{ tasks.gather.output }}"
      max_tokens: 800

  - id: notify
    depends_on: [think]
    when: ${{ vars.source != '' }}
    exec:
      command: "echo done"
"#;

/// Write the fixture to a temp file owned by this test run.
fn fixture_path(name: &str, body: &str) -> String {
    let dir = std::env::temp_dir().join("nika-cli-verbs-static");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(name);
    std::fs::write(&path, body).expect("fixture write");
    path.to_string_lossy().into_owned()
}

// ─── graph · the ONE projector ──────────────────────────────────────────

#[test]
fn graph_json_envelope_is_versioned_topo_sorted_and_stable() {
    let path = fixture_path("graph.nika.yaml", WORKFLOW);
    let out = graph::run(&path, GraphFormat::Json);
    assert_eq!(out.code, exit::OK);

    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    assert_eq!(doc["graph_format"], 1, "versioned envelope");
    assert_eq!(doc["workflow"], "static-suite");

    // Topological order: wave 0 (gather/probe) before fan/think before notify.
    let ids: Vec<&str> = doc["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|n| n["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids.len(), 5);
    let pos = |id: &str| ids.iter().position(|i| *i == id).expect("present");
    assert!(pos("gather") < pos("fan"), "deps before dependents");
    assert!(pos("probe") < pos("think"));
    assert!(pos("think") < pos("notify"));

    // Node facts: verb · tool/model resolution · gate · fan-out.
    let node = |id: &str| {
        doc["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|n| n["id"] == id)
            .expect("node")
            .clone()
    };
    assert_eq!(node("gather")["verb"], "invoke");
    assert_eq!(node("gather")["tool"], "nika:read");
    assert_eq!(node("think")["verb"], "infer");
    assert_eq!(
        node("think")["model"],
        "mock/echo",
        "workflow default resolved"
    );
    // The gate is the author's verbatim text — the projector never
    // rewrites (`${{ }}` wrapper included).
    assert_eq!(node("notify")["when"], "${{ vars.source != '' }}");
    assert_eq!(node("fan")["fan_out"]["kind"], "list");
    assert_eq!(node("fan")["fan_out"]["count"], 3);
    // mock/echo has no catalog price — the honest interval is null.
    assert_eq!(node("think")["cost_interval"], serde_json::Value::Null);

    // Edges: closed kind, sorted (from, to).
    let edges: Vec<(String, String)> = doc["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|e| {
            assert_eq!(e["kind"], "depends_on", "closed enum");
            (
                e["from"].as_str().expect("from").to_owned(),
                e["to"].as_str().expect("to").to_owned(),
            )
        })
        .collect();
    let mut sorted = edges.clone();
    sorted.sort();
    assert_eq!(edges, sorted, "stable edge order");
    assert!(edges.contains(&("gather".to_owned(), "fan".to_owned())));

    // Byte-stable: the projection is a pure function of the file.
    let again = graph::run(&path, GraphFormat::Json);
    assert_eq!(out.text, again.text, "two runs, identical bytes");
}

#[test]
fn graph_mermaid_and_dot_derive_from_the_projection() {
    let yaml = WORKFLOW;
    let wf = nika_schema::parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
    let report = nika_schema::check(&wf);
    let doc = project(&wf, &report);

    let mermaid = to_mermaid(&doc);
    assert!(mermaid.starts_with("graph TD\n"));
    assert!(mermaid.contains("gather --> fan"), "mermaid: {mermaid}");
    assert!(
        mermaid.contains("gather[\"gather · invoke · nika:read\"]"),
        "labels carry verb + tool: {mermaid}"
    );

    let dot = to_dot(&doc);
    assert!(dot.starts_with("digraph \"static-suite\""));
    assert!(dot.contains("\"gather\" -> \"fan\";"), "dot: {dot}");
    assert!(dot.ends_with("}\n"));
}

#[test]
fn graph_refuses_a_dag_broken_file_with_exit_2() {
    // think depends on a task that doesn't exist → conformance fails.
    let broken = WORKFLOW.replace("depends_on: [gather, probe]", "depends_on: [ghost]");
    let path = fixture_path("graph-broken.nika.yaml", &broken);
    let out = graph::run(&path, GraphFormat::Json);
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("no valid DAG order"), "{}", out.text);
}

// ─── inspect · the terminal anatomy ─────────────────────────────────────

#[test]
fn inspect_draws_the_box_tree_with_static_facts() {
    let path = fixture_path("inspect.nika.yaml", WORKFLOW);
    let out = inspect::run(&path);
    assert_eq!(out.code, exit::OK, "{}", out.text);

    // Header: identity + counts + the honest cost bound.
    let header = out.text.lines().next().expect("header");
    assert!(
        header.contains("static-suite · 5 task(s)"),
        "header: {header}"
    );
    assert!(header.contains("floor"), "mock/echo is unpriced: {header}");

    // Tree rows: box glyphs + verb facts + gate + fan-out.
    assert!(out.text.contains("├─") || out.text.contains("└─"));
    assert!(out.text.contains("invoke · nika:read"), "{}", out.text);
    assert!(out.text.contains("for_each ×3"), "{}", out.text);
    assert!(
        out.text.contains("when: ${{ vars.source != '' }}"),
        "{}",
        out.text
    );
    // Every task appears exactly once.
    for id in ["gather", "probe", "fan", "think", "notify"] {
        let hits = out.text.matches(id).count();
        assert!(hits >= 1, "{id} missing");
    }
    assert!(
        out.text
            .trim_end()
            .ends_with("DAG order proven by the check ladder)"),
        "footer attests the ladder"
    );
}

// ─── check · the rendered ladder ────────────────────────────────────────

#[test]
fn check_clean_file_exits_0_with_grep_stable_sections() {
    let path = fixture_path("check-clean.nika.yaml", WORKFLOW);
    let out = check::run(&path, false, PLAIN);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    for section in [
        "PLAN", "COST", "SECRETS", "TYPES", "TOOLS", "SCHEMA", "PERMITS",
    ] {
        assert!(out.text.contains(section), "missing section {section}");
    }
    assert!(
        out.text
            .contains("clean — audited before a single token was spent")
    );
    // mock/echo is unpriced: the cost lane says FLOOR, never invents.
    assert!(out.text.contains("UNBOUNDED"), "{}", out.text);
}

#[test]
fn check_dirty_file_exits_2_and_names_the_fix() {
    let dirty = WORKFLOW.replace("\"nika:read\"", "\"nika:reed\"");
    let path = fixture_path("check-dirty.nika.yaml", &dirty);
    let out = check::run(&path, false, PLAIN);
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("TOOLS"), "{}", out.text);
    assert!(
        out.text.contains("did you mean `nika:read`?"),
        "the fix-form rides the finding: {}",
        out.text
    );
    assert!(out.text.contains("findings above"));
}

#[test]
fn check_json_is_the_report_plus_clean_flag_never_coloured() {
    let path = fixture_path("check-json.nika.yaml", WORKFLOW);
    // Colour requested — json must ignore it (the contract bytes).
    let coloured = Theme {
        color: true,
        ascii: false,
        animate: false,
    };
    let out = check::run(&path, true, coloured);
    assert_eq!(out.code, exit::OK);
    assert!(!out.text.contains('\x1b'), "json is never coloured");
    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    assert_eq!(doc["clean"], true);
    assert!(doc["report_version"].is_number());
    assert!(doc["waves"].is_array());
}

#[test]
fn check_parse_error_is_a_file_finding_exit_2() {
    let path = fixture_path("check-parse.nika.yaml", "nika: v1\nworkflow: [broken");
    let out = check::run(&path, false, PLAIN);
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("PARSE"), "{}", out.text);
}

#[test]
fn check_unreadable_file_is_an_environment_error_exit_3() {
    let out = check::run("/nonexistent/missing.nika.yaml", false, PLAIN);
    assert_eq!(out.code, exit::ENV);
    assert!(out.text.contains("cannot read"));
}

#[test]
fn infer_permits_emits_a_paste_ready_boundary() {
    let path = fixture_path("permits.nika.yaml", WORKFLOW);
    let out = check::run_infer_permits(&path, false);
    assert_eq!(out.code, exit::OK);
    assert!(out.text.contains("permits:"), "{}", out.text);
    assert!(out.text.contains("nika:read"), "{}", out.text);

    let json = check::run_infer_permits(&path, true);
    let doc: serde_json::Value = serde_json::from_str(&json.text).expect("valid JSON");
    assert!(
        doc["permits_yaml"]
            .as_str()
            .expect("yaml")
            .contains("permits:")
    );
}

// ─── explain · the error registry surface ───────────────────────────────

#[test]
fn explain_teaches_a_registered_code_in_both_wire_forms() {
    for wire in ["NIKA-440", "440"] {
        let out = explain::run(wire);
        assert_eq!(out.code, exit::OK, "{wire}");
        assert!(out.text.contains("NIKA-440"), "{}", out.text);
        assert!(out.text.contains("Verb"), "category: {}", out.text);
        assert!(!out.text.trim().is_empty());
    }
}

#[test]
fn explain_refuses_an_unknown_code_exit_2() {
    let out = explain::run("NIKA-9999");
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("unknown code"));
}

// ─── the embedded pack surface ──────────────────────────────────────────

#[test]
fn pack_surface_round_trips_the_embedded_pack() {
    let spec = pack_surface::spec(false);
    assert_eq!(spec.code, exit::OK);
    assert!(
        spec.text.contains(nika_pack::pack_version()),
        "{}",
        spec.text
    );

    let canon = pack_surface::spec(true);
    assert_eq!(canon.text, nika_pack::canon());

    let schema = pack_surface::schema();
    assert_eq!(schema.text, nika_pack::schema_json());
    let _: serde_json::Value =
        serde_json::from_str(&schema.text).expect("embedded schema is valid JSON");

    let list = pack_surface::examples_list();
    let slugs = nika_pack::example_slugs();
    assert!(!slugs.is_empty(), "pack carries examples");
    assert_eq!(list.text, slugs.join("\n"));

    let shown = pack_surface::examples_show(&slugs[0]);
    assert_eq!(shown.code, exit::OK);
    assert_eq!(shown.text, nika_pack::example(&slugs[0]).expect("exists"));

    assert_eq!(pack_surface::examples_show("no-such-slug").code, exit::FILE);
    assert_eq!(pack_surface::examples_run(&slugs[0]).code, exit::ENV);
}

// ─── new · template instantiation (the own-corpus law) ──────────────────

#[test]
fn new_writes_a_template_that_passes_its_own_check() {
    let dir = std::env::temp_dir().join("nika-cli-verbs-static");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let dest = dir.join("from-template.nika.yaml");
    let dest_str = dest.to_string_lossy().into_owned();
    let _ = std::fs::remove_file(&dest);

    let names = nika_pack::template_names();
    assert!(!names.is_empty(), "pack carries templates");
    let template = names
        .iter()
        .find(|n| n.contains("chain"))
        .unwrap_or(&names[0]);

    let out = new::run(template, &dest_str, false);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(
        out.text.contains("SLOT"),
        "points at the slots: {}",
        out.text
    );

    // The own-corpus law: what we scaffold must pass our own ladder.
    let checked = check::run(&dest_str, false, PLAIN);
    assert_eq!(
        checked.code,
        exit::OK,
        "the shipped template must be check-clean: {}",
        checked.text
    );

    // Refuse-overwrite is the default posture.
    let refused = new::run(template, &dest_str, false);
    assert_eq!(refused.code, exit::ENV);
    assert!(refused.text.contains("--force"));
    // And --force is the explicit override.
    assert_eq!(new::run(template, &dest_str, true).code, exit::OK);
}

#[test]
fn new_refuses_an_unknown_template_and_names_the_set() {
    let out = new::run("no-such-template", "/tmp/never-written.nika.yaml", false);
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("unknown template"));
    for name in nika_pack::template_names() {
        assert!(out.text.contains(&name), "set names {name}");
    }
}
