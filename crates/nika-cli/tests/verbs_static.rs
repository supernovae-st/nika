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

const PLAIN: Theme = Theme::new(false, false, false);

/// The shared fixture — same shape as the e2e pipeline workflow.
const WORKFLOW: &str = r#"
nika: static-suite

model: mock/echo

permits:
  tools: ["nika:read"]
  exec: ["wc", "echo"]
  fs:
    read: ["./news.json"]

const:
  source: "./news.json"

tasks:
  gather:
    invoke:
      tool: "nika:read"
      args: { path: "${{ const.source }}" }

  probe:
    exec:
      command: ["wc", "-l", "./news.json"]

  fan:
    after:
      gather: success
    for_each: { items: ["a", "b", "c"] }
    infer:
      prompt: "Classify · ${{ item }}"
      max_tokens: 100

  think:
    with:
      gathered: ${{ tasks.gather.output }}
    after:
      probe: success
    infer:
      prompt: "Summarize · ${{ with.gathered }}"
      max_tokens: 800

  notify:
    after:
      think: success
    when: ${{ const.source != '' }}
    exec:
      command: ["echo", "done"]
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
    let out = graph::run(&path, GraphFormat::Json, PLAIN);
    assert_eq!(out.code, exit::OK);

    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    assert_eq!(doc["graph_format"], 3, "versioned envelope");
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
    assert_eq!(node("notify")["when"], "${{ const.source != '' }}");
    assert_eq!(node("fan")["fan_out"]["kind"], "list");
    assert_eq!(node("fan")["fan_out"]["count"], 3);
    // mock/echo is a proven zero (A-02): the echo never leaves the
    // process, so the interval is the exact [$0, $0] — bounded, never
    // null (null stays the honest shape for the sovereign locals).
    assert_eq!(
        node("think")["cost_interval"],
        serde_json::json!([0.0, 0.0])
    );

    // Edges: typed (graph_format 3 · kind closed enum), sorted (from, to).
    let edges: Vec<(String, String)> = doc["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|e| {
            match e["kind"].as_str().expect("kind") {
                "value" => assert!(
                    e["binding"].is_string(),
                    "a value edge carries the with: key that created it: {e}"
                ),
                "control" => assert!(
                    e["predicate"].is_string(),
                    "a control edge carries its after: predicate: {e}"
                ),
                other => panic!("unexpected edge kind `{other}` in this fixture: {e}"),
            }
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
    // The binding IS the edge: think's `with.gathered` projects as the
    // ONE value edge gather → think, named after its binding.
    let value_edge = doc["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .find(|e| e["from"] == "gather" && e["to"] == "think")
        .expect("the with: binding projects an edge");
    assert_eq!(value_edge["kind"], "value");
    assert_eq!(value_edge["binding"], "gathered");

    // Byte-stable: the projection is a pure function of the file.
    let again = graph::run(&path, GraphFormat::Json, PLAIN);
    assert_eq!(out.text, again.text, "two runs, identical bytes");
}

#[test]
fn graph_mermaid_and_dot_derive_from_the_projection() {
    let yaml = WORKFLOW;
    let wf = nika_schema::parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
    let report = nika_check::check(&wf);
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
    let broken = WORKFLOW.replace("probe: success", "ghost: success");
    let path = fixture_path("graph-broken.nika.yaml", &broken);
    let out = graph::run(&path, GraphFormat::Json, PLAIN);
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("no valid DAG order"), "{}", out.text);
}

// ─── inspect · the terminal anatomy ─────────────────────────────────────

#[test]
fn inspect_draws_the_wave_groups_with_static_facts() {
    let path = fixture_path("inspect.nika.yaml", WORKFLOW);
    let out = inspect::run(&path, PLAIN);
    assert_eq!(out.code, exit::OK, "{}", out.text);

    // Header: identity + counts + the honest cost bound. The all-mock
    // fixture is a proven zero (A-02), so the header carries the exact
    // ceiling — `est unbounded · bounded portion` stays the shape for a
    // genuinely unpriced (sovereign local) seat.
    let header = out.text.lines().next().expect("header");
    assert!(
        header.contains("static-suite · 5 tasks"),
        "header: {header}"
    );
    assert!(
        header.contains("est out ≤$0.0000"),
        "mock/echo is a proven zero: {header}"
    );

    // Waves as bordered groups: {gather,probe} · {fan,think} · notify.
    assert!(
        out.text.contains("╭ wave 1 ── 2 in parallel "),
        "{}",
        out.text
    );
    assert!(
        out.text.contains("╭ wave 2 ── 2 in parallel "),
        "{}",
        out.text
    );
    assert_eq!(
        out.text.matches("    ↓").count(),
        2,
        "two flow arrows join three waves: {}",
        out.text
    );
    assert!(out.text.contains("invoke · nika:read"), "{}", out.text);
    assert!(out.text.contains("for_each ×3"), "{}", out.text);
    assert!(
        out.text.contains("when: ${{ const.source != '' }}"),
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
            .contains("(no orphans · DAG check NIKA-DAG-001 clean)"),
        "the spec §6 DAG footer present (the parallelism/blast analysis follows it)"
    );
}

// ─── check · the rendered ladder ────────────────────────────────────────

#[test]
fn check_clean_file_exits_0_with_grep_stable_sections() {
    let path = fixture_path("check-clean.nika.yaml", WORKFLOW);
    let out = check::run(&path, false, false, None, PLAIN);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    for section in [
        "PLAN", "COST", "SECRETS", "TYPES", "TOOLS", "SCHEMA", "PERMITS",
    ] {
        assert!(out.text.contains(section), "missing section {section}");
    }
    // The clean verdict is the audited card line: what was proven, at a
    // glance (tasks · waves · permits state · cost line · hint count).
    assert!(
        out.text.contains("audited ·") && out.text.contains("wave") && out.text.contains("permits"),
        "the audited card line closes a clean report: {}",
        out.text
    );
    // mock/echo is a proven zero (A-02): the whole-mock fixture audits
    // to an exact $0 ceiling — no UNBOUNDED noise on the rehearsal plane
    // (an unpriced SOVEREIGN model still earns it; mock never did).
    assert!(
        out.text.contains("$0.0000") && !out.text.contains("UNBOUNDED"),
        "{}",
        out.text
    );
}

#[test]
fn check_dirty_file_exits_2_and_names_the_fix() {
    let dirty = WORKFLOW.replace("\"nika:read\"", "\"nika:reed\"");
    let path = fixture_path("check-dirty.nika.yaml", &dirty);
    let out = check::run(&path, false, false, None, PLAIN);
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
    let coloured = Theme::new(true, false, false);
    let out = check::run(&path, true, false, None, coloured);
    assert_eq!(out.code, exit::OK);
    assert!(!out.text.contains('\x1b'), "json is never coloured");
    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    assert_eq!(doc["clean"], true);
    assert!(doc["report_version"].is_number());
    assert!(doc["waves"].is_array());
}

#[test]
fn check_json_conformance_carries_severity_and_docs_url() {
    // The agent-loop wire: every conformance finding stamps its own
    // severity + per-code docs page (the rustc --explain move, machine
    // form). Consumers link the code without re-deriving anything.
    let broken = WORKFLOW.replace("probe: success", "ghost: success");
    let path = fixture_path("check-severity.nika.yaml", &broken);
    let out = check::run(&path, true, false, None, PLAIN);
    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    let c = &doc["conformance"][0];
    assert_eq!(c["severity"], "error");
    let url = c["docs_url"].as_str().expect("docs_url string");
    assert_eq!(
        url,
        format!(
            "https://nika.sh/language/errors/{}",
            c["code"].as_str().expect("code")
        )
    );
}

#[test]
fn check_parse_error_is_a_file_finding_exit_2() {
    let path = fixture_path("check-parse.nika.yaml", "nika: v1\nworkflow: [broken");
    let out = check::run(&path, false, false, None, PLAIN);
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("PARSE"), "{}", out.text);
}

#[test]
fn check_unreadable_file_is_an_environment_error_exit_3() {
    let out = check::run("/nonexistent/missing.nika.yaml", false, false, None, PLAIN);
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
        let out = explain::run(wire, PLAIN);
        assert_eq!(out.code, exit::OK, "{wire}");
        // The stable output contract: code · category · severity · slug.
        assert!(
            out.text
                .starts_with("NIKA-440 · verb · error · exec-non-zero-exit"),
            "{}",
            out.text
        );
        assert!(!out.text.trim().is_empty());
    }
}

#[test]
fn explain_refuses_an_unknown_code_exit_2() {
    let out = explain::run("NIKA-9999", PLAIN);
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

    // the showroom listing graduated to the organized surface
    // (verbs::examples — tiers · titles · chips · full filenames); the
    // corpus pins live in its own unit tests. Here: the machine truth
    // (the pack) still resolves everything the listing names.
    let slugs = nika_pack::example_slugs();
    assert!(!slugs.is_empty(), "pack carries examples");
    let list = nika_cli::verbs::examples::list(PLAIN);
    assert_eq!(list.code, exit::OK);
    for slug in &slugs {
        assert!(
            list.text.contains(&format!("{slug}.nika.yaml")),
            "listing names `{slug}` with its extension"
        );
    }

    // `try <slug>` EXECUTES (the L3 run verb
    // shipped). Its behavior is pinned at the binary plane in
    // tests/run_verb.rs (the static suite can't drive a real run).
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

    let out = new::run(template, Some(&dest_str), false);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(
        out.text.contains("SLOT"),
        "points at the slots: {}",
        out.text
    );

    // The own-corpus law: what we scaffold must pass our own ladder.
    let checked = check::run(&dest_str, false, false, None, PLAIN);
    assert_eq!(
        checked.code,
        exit::OK,
        "the shipped template must be check-clean: {}",
        checked.text
    );

    // Refuse-overwrite is the default posture.
    let refused = new::run(template, Some(&dest_str), false);
    assert_eq!(refused.code, exit::ENV);
    assert!(refused.text.contains("--force"));
    // And --force is the explicit override.
    assert_eq!(new::run(template, Some(&dest_str), true).code, exit::OK);
}

#[test]
fn new_answers_the_discovery_query_as_a_success() {
    // `?` is the canonical discovery query — first-class since the
    // 2026-07-07 field walk (it used to reuse the unknown-template error
    // and exit 2 · a documented command must not read as a failure). The
    // `embedded set:` wire line survives verbatim for the editor probes,
    // and a passed dest is never written.
    let out = new::run("?", Some("/tmp/never-written.nika.yaml"), false);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(out.text.contains("embedded set:"), "{}", out.text);
    assert!(!std::path::Path::new("/tmp/never-written.nika.yaml").exists());
    for name in nika_pack::template_names() {
        assert!(out.text.contains(&name), "set names {name}");
    }
}

#[test]
fn new_refuses_gibberish_and_names_the_set() {
    // Zero-evidence text (no shared term with any template body) keeps
    // the honest unknown-template error + the wire-contract set line.
    let out = new::run(
        "zzzz qqqq xxxx",
        Some("/tmp/never-written.nika.yaml"),
        false,
    );
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("no template or intent matches"));
    assert!(out.text.contains("embedded set:"));
}

#[test]
fn graph_cost_interval_attributes_each_priced_task_to_itself() {
    // Two priced infer tasks with DIFFERENT token bounds: the projector
    // must attach each task's own interval (a swapped find would invert
    // the order — the relative assert is price-change-proof).
    let priced = r#"
nika: priced-pair
model: anthropic/claude-sonnet-4-6

tasks:
  small:
    infer:
      prompt: "a"
      max_tokens: 100

  large:
    with:
      draft: ${{ tasks.small.output }}
    infer:
      prompt: "b · ${{ with.draft }}"
      max_tokens: 800

outputs:
  result: ${{ tasks.large.output }}
"#;
    let path = fixture_path("priced.nika.yaml", priced);
    let out = graph::run(&path, GraphFormat::Json, PLAIN);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    let interval = |id: &str| -> [f64; 2] {
        let node = doc["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|n| n["id"] == id)
            .expect("node");
        let pair = node["cost_interval"].as_array().expect("priced interval");
        [
            pair[0].as_f64().expect("min"),
            pair[1].as_f64().expect("max"),
        ]
    };
    let small = interval("small");
    let large = interval("large");
    assert!(small[1] > 0.0, "priced model yields a real ceiling");
    assert!(
        small[1] < large[1],
        "each task carries ITS OWN bound: small {small:?} vs large {large:?}"
    );
    assert!(small[0] <= small[1] && large[0] <= large[1], "min ≤ max");
}

#[test]
fn graph_nodes_always_carry_the_permits_field() {
    // Spec §6 envelope contract: `permits` is present on EVERY node (an
    // array · empty until the per-task effects projector ships). A
    // consumer reading node.permits must never get `undefined`.
    let path = fixture_path("permits-field.nika.yaml", WORKFLOW);
    let out = graph::run(&path, GraphFormat::Json, PLAIN);
    assert_eq!(out.code, exit::OK);
    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    for node in doc["nodes"].as_array().expect("nodes") {
        assert!(
            node["permits"].is_array(),
            "permits must serialize as an array on {}: {node}",
            node["id"]
        );
    }
}

#[test]
fn graph_dedups_duplicate_reference_edges() {
    // ONE binding whose expression reads the same producer twice must
    // not lie about cardinality: N refs to the SAME task collapse to
    // one typed edge (the `after:` map cannot even express a duplicate —
    // a repeated key is refused by the YAML layer).
    let dup = WORKFLOW.replace(
        "gathered: ${{ tasks.gather.output }}",
        "gathered: \"${{ tasks.gather.output }} + ${{ tasks.gather.output }}\"",
    );
    let path = fixture_path("dup-edges.nika.yaml", &dup);
    let out = graph::run(&path, GraphFormat::Json, PLAIN);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("valid JSON");
    let gather_think = doc["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|e| e["from"] == "gather" && e["to"] == "think")
        .count();
    assert_eq!(gather_think, 1, "duplicate reference collapses to one edge");
}

/// The SKILLS rung (#473) — three postures through the REAL check verb
/// over real files: green (the skill loads + parses · exit 0 · the rung
/// is visible), missing file (`NIKA-AGENT-003` · exit 2), malformed file
/// (`NIKA-AGENT-004` · exit 2 · the explain pointer rides the row) — and
/// the `--json` machine contract (`skills_resolve` · `skill_findings[]`).
#[test]
fn check_skills_rung_greens_reds_and_teaches() {
    let dir = std::env::temp_dir().join(format!("nika-skills-rung-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let good = dir.join("good-SKILL.md");
    std::fs::write(&good, "---\nname: g\ndescription: d\n---\nbody\n").expect("fixture");
    let bad = dir.join("bad-SKILL.md");
    std::fs::write(&bad, "no frontmatter\n").expect("fixture");
    let ghost = dir.join("ghost-SKILL.md");

    let wf_with = |skill: &std::path::Path, name: &str| {
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!(
                // the skills: fs edge lives INSIDE the boundary (#473) — the
                // grant is judged BEFORE the reader runs, so every posture
                // below tests the RUNG, never the boundary refusal.
                "nika: w\npermits:\n  fs:\n    read: [\"{p}\"]\nmodel: mock/echo\ntasks:\n  go:\n    agent: {{ prompt: \"hi\", skills: [\"{p}\"] }}\n",
                p = skill.display()
            ),
        )
        .expect("workflow fixture");
        path.to_str().expect("utf8 path").to_owned()
    };

    // GREEN — the rung names the count, the audit stays clean (exit 0).
    let green = check::run(
        &wf_with(&good, "green.nika.yaml"),
        false,
        false,
        None,
        PLAIN,
    );
    assert_eq!(green.code, exit::OK, "{}", green.text);
    assert!(
        green.text.contains("SKILLS") && green.text.contains("1 skill(s) resolve"),
        "the green rung is visible: {}",
        green.text
    );

    // MISSING — NIKA-AGENT-003 · exit 2 · the row names the task + the fix.
    let missing_path = wf_with(&ghost, "missing.nika.yaml");
    let missing = check::run(&missing_path, false, false, None, PLAIN);
    assert_eq!(missing.code, exit::FILE, "{}", missing.text);
    assert!(
        missing.text.contains("[NIKA-AGENT-003 · skills] task `go`"),
        "the row leads with the code: {}",
        missing.text
    );
    assert!(
        missing.text.contains("fix: nika explain NIKA-AGENT-003"),
        "the explain pointer teaches: {}",
        missing.text
    );

    // MALFORMED — NIKA-AGENT-004 · exit 2 · the defect names the repair.
    let malformed = check::run(&wf_with(&bad, "bad.nika.yaml"), false, false, None, PLAIN);
    assert_eq!(malformed.code, exit::FILE, "{}", malformed.text);
    assert!(
        malformed.text.contains("NIKA-AGENT-004") && malformed.text.contains("frontmatter"),
        "the defect teaches the shape: {}",
        malformed.text
    );

    // The machine surface: clean=false · skills_resolve=false · the row
    // carries task/code/docs_url.
    let out = check::run(&missing_path, true, false, None, PLAIN);
    assert_eq!(out.code, exit::FILE);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(payload["clean"], false);
    assert_eq!(payload["skills_resolve"], false);
    assert_eq!(payload["skill_findings"][0]["task"], "go");
    assert_eq!(payload["skill_findings"][0]["code"], "NIKA-AGENT-003");
    assert!(
        payload["skill_findings"][0]["docs_url"]
            .as_str()
            .expect("docs_url")
            .ends_with("/NIKA-AGENT-003"),
        "{payload:#}"
    );
    // …and the green twin: skills_resolve=true · NO skill_findings key.
    let out = check::run(&wf_with(&good, "green.nika.yaml"), true, false, None, PLAIN);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(payload["clean"], true);
    assert_eq!(payload["skills_resolve"], true);
    assert!(payload.get("skill_findings").is_none(), "{payload:#}");
}

/// The DAG map rides the accents check verdict (the audit reads as the
/// graph it judged) — and never the sober register. (Lives here off the
/// src loc-cap: the 1500-line law bit check.rs at the merge.)
#[test]
fn accents_check_verdict_carries_the_dag_map() {
    let yaml = "nika: m\ntasks:\n  one:\n    infer: { prompt: hi, max_tokens: 5, model: \"mock/echo\" }\n  two:\n    with:\n      prev: ${{ tasks.one.output }}\n    infer: { prompt: \"${{ with.prev }}\", max_tokens: 5, model: \"mock/echo\" }\n";
    let dir = std::env::temp_dir().join(format!("nika-check-map-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("map.nika.yaml");
    std::fs::write(&file, yaml).expect("write");
    let path = file.to_string_lossy().into_owned();

    let mut accents = nika_cli::Theme::new(true, false, false);
    accents.accents = true;
    let out = nika_cli::verbs::check::run(&path, false, false, None, accents);
    assert_eq!(out.code, nika_cli::verbs::exit::OK, "{}", out.text);
    assert!(
        out.text.contains("───▶") || out.text.contains("wave 1"),
        "the map rides the accents verdict: {}",
        out.text
    );

    let sober = nika_cli::verbs::check::run(
        &path,
        false,
        false,
        None,
        nika_cli::Theme::new(false, false, false),
    );
    assert!(
        !sober.text.contains("───▶"),
        "sober keeps its shape: {}",
        sober.text
    );
    let _ = std::fs::remove_dir_all(&dir);
}
