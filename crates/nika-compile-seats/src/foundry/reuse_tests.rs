// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Executable reuse, from a reference to the candidate's bytes: resolution against a release,
//! the binding laws, the bounded literal edit, the expansion and its Check, the witness and the
//! revision of a bound value. Synthetic components here; the admitted release's own blocks are
//! exercised by the knowledge door's tests.

use nika_compile::surface::{literal_projection, sha256};
use nika_compile::{AuthoringKnowledge, CompileRequest, KnowledgeReference};
use serde_json::{Value, json};

use super::bind::{Binding, BindingError, EditRefusal, edit_literal, kind};
use super::component::{
    Component, ComponentCatalog, ComponentRef, Hole, Release, Unresolved, pinned,
};
use super::instance::{ExpandError, adopt, expand, instantiate};
use super::witness::{reuse, reuse_of, revise, witness};
use super::{trace, traced};

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

/// The human's document: its own name, its own boundary over its own files, nothing else yet.
const PARENT: &str = r#"nika: stale-tickets-report
# The person's own boundary: the files the request names.
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks: {}
"#;

const VERSION: &str = "fixture-reuse-r1";
const RELEASE: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn release() -> Release {
    Release::new(VERSION, RELEASE, "nika-knowledge-release-profile/r1")
}

fn component(source: &str) -> Component {
    let mut component = Component::new(
        "block:stale-filter-report",
        release(),
        "blocks/stale-filter-report.nika",
        sha256(source),
        source,
    );
    component.holes = vec![
        Hole::new("const.records_path", "human", None),
        Hole::new("const.report_path", "human", None),
        Hole::new(
            "const.max_age_hours",
            "human",
            Some("the age the request states, in hours".to_owned()),
        ),
    ];
    component.effects = vec!["fs.read".to_owned(), "fs.write".to_owned()];
    component.authority = vec!["permits.fs".to_owned(), "permits.tools".to_owned()];
    component.callables = vec![
        "nika:read".to_owned(),
        "nika:jq".to_owned(),
        "nika:write".to_owned(),
    ];
    component
}

/// A catalogue of one release, as a door's admitted snapshot answers.
struct Shelf(Vec<Component>);

impl ComponentCatalog for Shelf {
    fn release(&self) -> Release {
        release()
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        reference.block_name()?;
        pinned(reference, &self.release())?;
        (self.0.iter())
            .find(|c| c.id == reference.id)
            .cloned()
            .ok_or_else(|| Unresolved::Unknown(reference.id.clone()))
    }
}

fn bound(hours: i64) -> Vec<Binding> {
    vec![
        Binding::new("const.records_path", json!("./in/tickets.json")),
        Binding::new("const.report_path", json!("./out/report.json")),
        Binding::new("const.max_age_hours", json!(hours)),
    ]
}

/// The bytes of `edited` that differ from `base`: the one changed span of each, by common prefix
/// and suffix.
fn changed<'a>(base: &'a str, edited: &'a str) -> (&'a str, &'a str) {
    let prefix = (base.bytes().zip(edited.bytes()))
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = (base[prefix..]
        .bytes()
        .rev()
        .zip(edited[prefix..].bytes().rev()))
    .take_while(|(a, b)| a == b)
    .count();
    (
        &base[prefix..base.len() - suffix],
        &edited[prefix..edited.len() - suffix],
    )
}

#[test]
fn a_literal_edit_changes_only_the_span_the_parser_proves_and_keeps_every_other_byte() {
    let source = "nika: x\n# records_path: ./data/tickets.json stays a comment\nconst:\n  records_path: ./data/tickets.json\n  also: \"./data/tickets.json\"\npermits:\n  fs: { read: [\"./data/tickets.json\"] }\ntasks: {}\n";
    let edited = edit_literal(source, "const.records_path", &json!("./in/t.json")).unwrap();
    assert_eq!(
        changed(source, &edited),
        ("./data/tickets.json", "\"./in/t.json\"")
    );
    assert!(edited.contains("# records_path: ./data/tickets.json stays a comment"));
    assert!(edited.contains("  also: \"./data/tickets.json\"\n"));
    assert!(edited.contains("read: [\"./data/tickets.json\"]"));
    let projection = literal_projection(&edited).unwrap();
    assert_eq!(projection["const"]["records_path"], "./in/t.json");
    assert_eq!(projection["const"]["also"], "./data/tickets.json");
    // A typed constant keeps its declaration; its value is the literal.
    let typed = edit_literal(STALE, "const.max_age_hours", &json!(72)).unwrap();
    assert_eq!(changed(STALE, &typed), ("48", "72"));
    // A value already held is the same bytes.
    assert_eq!(
        edit_literal(STALE, "const.max_age_hours", &json!(48)).unwrap(),
        STALE
    );
}

#[test]
fn a_literal_edit_refuses_what_it_cannot_prove() {
    let block = "nika: x\nconst:\n  note: |\n    a block scalar\ntasks: {}\n";
    assert_eq!(
        edit_literal(block, "const.note", &json!("b")),
        Err(EditRefusal::Unlocated("const.note".to_owned()))
    );
    assert_eq!(
        edit_literal(STALE, "const.absent", &json!("b")),
        Err(EditRefusal::Absent("const.absent".to_owned()))
    );
    assert_eq!(
        edit_literal("tasks: [", "const.x", &json!(1)),
        Err(EditRefusal::NotADocument)
    );
    // Two values that each write the same literal at one path cannot exist; two spans that
    // would each prove the edit are refused rather than chosen (an alias makes one).
    let alias = "nika: x\nconst:\n  a: &shared ./p.json\n  b: *shared\ntasks: {}\n";
    assert!(edit_literal(alias, "const.b", &json!("./q.json")).is_err());
}

#[test]
fn a_reference_is_a_typed_object_and_only_a_block_resolves() {
    let shelf = Shelf(vec![component(STALE)]);
    let exact = ComponentRef::from_value(
        &json!({"id": "block:stale-filter-report", "version": VERSION, "release": RELEASE}),
    )
    .unwrap();
    assert_eq!(
        shelf.resolve(&exact).unwrap().id,
        "block:stale-filter-report"
    );
    // Text that looks like an id is no reference: only the closed object is read.
    for not_a_reference in [
        json!("block:stale-filter-report"),
        json!({"id": "block:stale-filter-report", "verison": VERSION}),
        json!({"id": " block:stale-filter-report"}),
        json!({"id": 7}),
        json!({}),
    ] {
        assert!(
            matches!(
                ComponentRef::from_value(&not_a_reference),
                Err(Unresolved::Malformed(_))
            ),
            "{not_a_reference}"
        );
    }
    let resolve = |id: &str| shelf.resolve(&ComponentRef::new(id));
    assert_eq!(
        resolve("pattern:filter-by-age"),
        Err(Unresolved::NotExecutable {
            id: "pattern:filter-by-age".to_owned(),
            kind: "pattern".to_owned()
        })
    );
    assert!(matches!(
        resolve("skill:report"),
        Err(Unresolved::NotExecutable { .. })
    ));
    assert!(matches!(
        resolve("Block:stale-filter-report"),
        Err(Unresolved::NotAComponent(_))
    ));
    assert!(matches!(
        resolve("block:"),
        Err(Unresolved::NotAComponent(_))
    ));
    assert!(matches!(
        resolve("block:../escape"),
        Err(Unresolved::NotAComponent(_))
    ));
    assert_eq!(
        resolve("block:missing"),
        Err(Unresolved::Unknown("block:missing".to_owned()))
    );
    let old = ComponentRef::new("block:stale-filter-report").at_version("fixture-reuse-r0");
    assert!(matches!(
        shelf.resolve(&old),
        Err(Unresolved::VersionMismatch { .. })
    ));
    let other = ComponentRef::new("block:stale-filter-report").in_release("2".repeat(64));
    assert!(matches!(
        shelf.resolve(&other),
        Err(Unresolved::ReleaseMismatch { .. })
    ));
    // A component whose bytes are not the pinned ones is refused by its resolver.
    let row = json!({"id": "block:x", "file": "blocks/x.nika", "file_sha256": sha256(STALE)});
    assert_eq!(
        Component::from_row(release(), &row, Some(b"nika: tampered\n")),
        Err(Unresolved::Integrity("block:x".to_owned()))
    );
    assert_eq!(
        Component::from_row(release(), &row, None),
        Err(Unresolved::Integrity("block:x".to_owned()))
    );
}

#[test]
fn a_binding_is_judged_against_the_holes_and_the_literal_kinds_the_component_holds() {
    let stale = component(STALE);
    let refused = |bindings: Vec<Binding>| instantiate(&stale, &bindings).unwrap_err();
    assert_eq!(
        refused(vec![Binding::new(
            "tasks.stale.invoke.tool",
            json!("nika:fetch")
        )]),
        BindingError::UnknownHole("tasks.stale.invoke.tool".to_owned())
    );
    assert_eq!(
        refused(vec![Binding::new("const.max_age_hours", json!("72"))]),
        BindingError::Incompatible {
            path: "const.max_age_hours".to_owned(),
            held: "integer",
            given: "text"
        }
    );
    assert_eq!(
        refused(vec![Binding::new("const.records_path", json!(72))]),
        BindingError::Incompatible {
            path: "const.records_path".to_owned(),
            held: "text",
            given: "integer"
        }
    );
    assert_eq!(
        refused(vec![Binding::new(
            "const.records_path",
            json!("${{ secrets.token }}")
        )]),
        BindingError::Expression("const.records_path".to_owned())
    );
    assert_eq!(
        refused(vec![
            Binding::new("const.max_age_hours", json!(72)),
            Binding::new("const.max_age_hours", json!(96)),
        ]),
        BindingError::Duplicate("const.max_age_hours".to_owned())
    );
    // A text that looks like a component id is a literal like any other.
    let looks_like_an_id = instantiate(
        &stale,
        &[Binding::new("const.report_path", json!("block:other@v1"))],
    )
    .unwrap();
    assert_eq!(
        literal_projection(&looks_like_an_id.source).unwrap()["const"]["report_path"],
        "block:other@v1"
    );
    assert_eq!(kind(&json!(1.5)), "number");
    assert_eq!(kind(&json!(2)), "integer");
}

#[test]
fn an_expansion_is_the_component_bound_into_the_document_checked_and_receipted() {
    let stale = component(STALE);
    let instance = instantiate(&stale, &bound(48)).unwrap();
    assert!(instance.open.is_empty(), "{:?}", instance.open);
    let expansion = expand(PARENT, &instance).unwrap();
    let candidate = &expansion.candidate;
    assert!(expansion.ready, "{:#}", expansion.receipt["check"]);
    // The parent's own bytes stay: its name, its comment above its boundary, its boundary.
    assert!(
        candidate.starts_with("nika: stale-tickets-report\nconst:\n"),
        "{candidate}"
    );
    assert!(
        candidate
            .contains("\n# The person's own boundary: the files the request names.\npermits:\n")
    );
    // Every line of the parent stays, in its order; only its empty `tasks: {}` opens up.
    let mut rest = candidate.lines();
    for line in PARENT.lines().filter(|line| *line != "tasks: {}") {
        assert!(
            rest.any(|kept| kept == line),
            "{line:?} lost from\n{candidate}"
        );
    }
    assert!(
        candidate
            .contains("  fs: { read: [\"./in/tickets.json\"], write: [\"./out/report.json\"] }\n")
    );
    // Nothing of the component's probe world or authority is inherited.
    for probe in [
        "./data/tickets.json",
        "./out/stale.json",
        "mock/echo",
        "p90-stale-filter-report",
    ] {
        assert!(
            !candidate.contains(probe),
            "{probe} leaked into\n{candidate}"
        );
    }
    let document = literal_projection(candidate).unwrap();
    assert_eq!(
        document["const"]["max_age_hours"],
        json!({"type": "integer", "value": 48})
    );
    assert_eq!(
        document["tasks"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        [
            "parse_records",
            "read_records",
            "report_text",
            "stale",
            "write_report"
        ]
    );
    let receipt = &expansion.receipt;
    assert_eq!(receipt["component"]["id"], "block:stale-filter-report");
    assert_eq!(receipt["component"]["release"]["version"], VERSION);
    assert_eq!(receipt["component"]["release"]["snapshot_sha256"], RELEASE);
    assert_eq!(receipt["component"]["file_sha256"], sha256(STALE));
    assert_eq!(receipt["candidate_sha256"], sha256(candidate));
    assert_eq!(receipt["authority"]["inherited"], false);
    assert_eq!(receipt["not_inherited"]["model"], "mock/echo");
    assert_eq!(
        receipt["not_inherited"]["permits"]["fs"]["read"][0],
        "./data/tickets.json"
    );
    let hours = &receipt["bindings"][2];
    assert_eq!(
        (
            hours["path"].as_str(),
            hours["component_literal"].clone(),
            hours["bound"].clone()
        ),
        (Some("const.max_age_hours"), json!(48), json!(48))
    );
    // Every produced node is named with the digest of what the candidate holds there.
    let digest = |section: &str, name: &str| json!(sha256(&document[section][name].to_string()));
    assert_eq!(receipt["nodes"]["tasks"]["stale"], digest("tasks", "stale"));
    assert_eq!(
        receipt["nodes"]["const"]["report_path"],
        digest("const", "report_path")
    );
    assert_eq!(
        receipt["nodes"]["outputs"]["stale"],
        digest("outputs", "stale")
    );
    assert_eq!(witness(receipt, candidate)["verdict"], "expanded");
}

#[test]
fn an_open_hole_a_collision_or_a_missing_boundary_is_never_papered_over() {
    let stale = component(STALE);
    // An unbound hole: the probe's own path is never taken for the request's.
    let partial = instantiate(&stale, &bound(48)[..2]).unwrap();
    assert_eq!(partial.open, ["const.max_age_hours"]);
    assert_eq!(
        expand(PARENT, &partial),
        Err(ExpandError::Binding(BindingError::Unbound(vec![
            "const.max_age_hours".to_owned()
        ])))
    );
    // A name the document already holds is refused, not renamed.
    let instance = instantiate(&stale, &bound(48)).unwrap();
    let taken = PARENT.replace(
        "tasks: {}\n",
        "tasks:\n  stale:\n    invoke: { tool: \"nika:read\", args: { path: \"./in/tickets.json\" } }\n",
    );
    assert_eq!(
        expand(&taken, &instance),
        Err(ExpandError::Collision {
            section: "tasks".to_owned(),
            name: "stale".to_owned()
        })
    );
    // No boundary of the person's: the component's permits never stand in, and Check refuses.
    let bare = "nika: no-boundary\ntasks: {}\n";
    let expansion = expand(bare, &instance).unwrap();
    assert!(!expansion.ready);
    assert!(
        !expansion.candidate.contains("permits"),
        "{}",
        expansion.candidate
    );
    let codes = expansion.receipt["check"]["findings"].to_string();
    assert!(codes.contains("NIKA-AUTH-006"), "{codes}");
    assert_eq!(expansion.receipt["authority"]["inherited"], false);
    // The boundary the body needs is stated for the person to grant, never granted.
    assert!(
        expansion.receipt["authority"]["document_needs"]
            .to_string()
            .contains("./in/tickets.json"),
        "{:#}",
        expansion.receipt["authority"]
    );
}

#[test]
fn removing_the_expansion_fails_the_witness_even_when_its_text_stays_in_context() {
    let stale = component(STALE);
    let expansion = expand(PARENT, &instantiate(&stale, &bound(48)).unwrap()).unwrap();
    let receipt = expansion.receipt.clone();
    // The component's whole text stays in the author's pack; the candidate is the parent alone.
    let shown = [KnowledgeReference {
        kind: "block".to_owned(),
        id: "block:stale-filter-report".to_owned(),
        text: format!("Stale filter\n```yaml\n{STALE}```"),
    }];
    let without = PARENT.replace("tasks: {}", "tasks:\n  noop:\n    invoke: { tool: \"nika:read\", args: { path: \"./in/tickets.json\" } }");
    assert_eq!(witness(&receipt, &without)["verdict"], "absent");
    let record = reuse(&shown, std::slice::from_ref(&receipt), Some(&without));
    assert_eq!(
        (record["expanded"].clone(), record["absent"].clone()),
        (json!(0), json!(1))
    );
    // The component's task lines quoted inside a prompt: the old lexical trace counted them as
    // most of the block; the witness finds no node of the expansion.
    let quoted = STALE
        .lines()
        .skip_while(|line| *line != "tasks:")
        .skip(1)
        .take_while(|line| *line != "outputs:")
        .fold(String::new(), |mut text, line| {
            text.push_str("      ");
            text.push_str(line);
            text.push('\n');
            text
        });
    let mimic = format!(
        "nika: mimic\nmodel: mock/echo\npermits: {{}}\ntasks:\n  explain:\n    infer:\n      prompt: |\n{quoted}\n"
    );
    let overlap = trace(&shown, &mimic);
    assert_eq!(
        overlap["references"][0]["overlap"], "most_lines",
        "{overlap:#}"
    );
    assert_eq!(witness(&receipt, &mimic)["verdict"], "absent");
    let none = reuse(&shown, &[], Some(&mimic));
    assert_eq!(none["references"][0]["use"], "consulted");
    assert_eq!(none["expanded"], 0);
    // The expansion itself: expanded, bound to these exact bytes.
    let held = reuse(
        &shown,
        std::slice::from_ref(&receipt),
        Some(&expansion.candidate),
    );
    assert_eq!(held["expanded"], 1, "{held:#}");
    assert_eq!(held["references"].as_array().unwrap().len(), 1);
    assert_eq!(
        held["references"][0]["witness"]["candidate_sha256"],
        sha256(&expansion.candidate)
    );
}

#[test]
fn a_revision_from_48_to_72_changes_the_bound_literal_and_nothing_else() {
    let stale = component(STALE);
    let expansion = expand(PARENT, &instantiate(&stale, &bound(48)).unwrap()).unwrap();
    let change = [Binding::new("const.max_age_hours", json!(72))];
    let (revised, receipt) =
        revise(&expansion.candidate, &expansion.receipt, &stale, &change).unwrap();
    assert_eq!(changed(&expansion.candidate, &revised), ("48", "72"));
    let (before, after) = (
        literal_projection(&expansion.candidate).unwrap(),
        literal_projection(&revised).unwrap(),
    );
    let mut expected = before.clone();
    expected["const"]["max_age_hours"]["value"] = json!(72);
    assert_eq!(after, expected);
    assert_eq!(receipt["bindings"][2]["bound"], 72);
    assert_eq!(receipt["bindings"][2]["component_literal"], 48);
    assert_eq!(receipt["revises"], expansion.receipt["candidate_sha256"]);
    assert_eq!(receipt["candidate_sha256"], sha256(&revised));
    assert_eq!(receipt["check"]["ready"], true, "{:#}", receipt["check"]);
    assert_eq!(witness(&receipt, &revised)["verdict"], "expanded");
    // The old receipt on the new bytes: every node is there, one changed since.
    let old = witness(&expansion.receipt, &revised);
    assert_eq!(old["verdict"], "revised");
    assert_eq!(old["nodes"]["changed"], json!(["const.max_age_hours"]));
    assert_eq!(old["bindings_not_held"], json!(["const.max_age_hours"]));
    // A change outside the bound holes, of another kind, or on a stale receipt revises nothing.
    assert!(
        revise(
            &revised,
            &receipt,
            &stale,
            &[Binding::new("const.max_age_hours", json!("72h"))]
        )
        .is_err()
    );
    assert!(
        revise(
            &revised,
            &receipt,
            &stale,
            &[Binding::new("tasks.stale.invoke.tool", json!("x"))]
        )
        .is_err()
    );
    assert!(revise(&revised, &expansion.receipt, &stale, &change).is_err());
}

#[test]
fn a_legacy_lexical_trace_reads_as_consultation_never_as_reuse() {
    let legacy = json!({
        "found": 2,
        "trace": {"law": "lexical", "instantiated": 1, "adapted": 1, "references": [
            {"id": "block:remind", "use": "instantiated"}, {"id": "example:x", "use": "adapted"}
        ]}
    });
    let read = reuse_of(&legacy);
    assert_eq!(read["expanded"], 0);
    assert_eq!(read["consulted"], 2);
    assert_eq!(read["legacy_lexical_trace"]["instantiated"], 1);
    assert_eq!(reuse_of(&json!({"found": 0})), Value::Null);
    let current = json!({"reuse": {"expanded": 1}});
    assert_eq!(reuse_of(&current)["expanded"], 1);
}

#[test]
fn the_qualification_record_states_reuse_from_receipts_and_never_instantiation_from_overlap() {
    let mut out = nika_compile::surface::initial();
    out.candidate = Some(STALE.to_owned());
    let pack = AuthoringKnowledge {
        references: vec![KnowledgeReference {
            kind: "block".to_owned(),
            id: "block:stale-filter-report".to_owned(),
            text: format!("```yaml\n{STALE}```"),
        }],
        ..AuthoringKnowledge::default()
    };
    let qualified = CompileRequest::create("x").with_authoring_knowledge(pack);
    traced(&qualified, json!({"found": 1}), &mut out);
    let record = &out.provenance.decision.as_ref().unwrap()["knowledge_qualification"];
    assert!(record.get("trace").is_none(), "{record:#}");
    assert_eq!(record["reuse"]["consulted"], 1);
    assert_eq!(record["reuse"]["expanded"], 0);
    assert_eq!(record["lexical_overlap"]["most_lines"], 1);
    assert!(!record.to_string().contains("instantiated"), "{record:#}");
}

#[test]
fn an_editor_inserting_the_exact_entries_gets_the_same_node_receipt() {
    let stale = component(STALE);
    let instance = instantiate(&stale, &bound(48)).unwrap();
    let entries = instance.entries().unwrap();
    let text = |section: &str, name: &str| {
        let entry = entries
            .iter()
            .find(|e| e.section == section && e.name == name);
        entry.map(|e| e.text.clone()).unwrap()
    };
    assert_eq!(
        text("const", "max_age_hours"),
        "{ type: integer, value: 48 }"
    );
    assert_eq!(text("const", "records_path"), "\"./in/tickets.json\"");
    assert_eq!(text("outputs", "stale"), "${{ tasks.stale.output }}");
    assert!(text("tasks", "stale").starts_with("with: { rows:"));
    assert!(text("tasks", "stale").contains("\ninvoke:\n  tool: \"nika:jq\"\n  args:\n"));
    assert_eq!(entries.iter().filter(|e| e.section == "tasks").count(), 5);
    // Another editor writes the same entries under the person's keys, its own way.
    let mut doc = String::from("nika: stale-tickets-report\nconst:\n");
    let mut tasks = String::from("tasks:\n");
    let mut outputs = String::from("outputs:\n");
    for entry in &entries {
        use std::fmt::Write as _;
        let _ = match entry.section.as_str() {
            "tasks" => {
                let _ = writeln!(tasks, "    {}:", entry.name);
                for line in entry.text.lines() {
                    let _ = writeln!(tasks, "        {line}");
                }
                Ok(())
            }
            "const" => writeln!(doc, "    {}: {}", entry.name, entry.text),
            _ => writeln!(outputs, "    {}: {}", entry.name, entry.text),
        };
    }
    let permits = PARENT
        .split("permits:\n")
        .nth(1)
        .unwrap()
        .replace("tasks: {}\n", "");
    let candidate = format!("{doc}permits:\n{permits}{tasks}{outputs}");
    let adopted = adopt(&candidate, &instance).unwrap();
    assert!(adopted.ready, "{:#}", adopted.receipt["check"]);
    let expanded = expand(PARENT, &instance).unwrap();
    assert_eq!(adopted.receipt["nodes"], expanded.receipt["nodes"]);
    assert_ne!(adopted.candidate, expanded.candidate);
    assert_eq!(
        witness(&expanded.receipt, &candidate)["verdict"],
        "expanded"
    );
    // What the bound component does not write is no adoption of it.
    let changed = expanded.candidate.replace("value: 48", "value: 49");
    assert!(matches!(
        adopt(&changed, &instance),
        Err(ExpandError::Unproven(_))
    ));
    assert!(matches!(
        adopt(PARENT, &instance),
        Err(ExpandError::Unproven(_))
    ));
}

#[test]
fn a_hole_in_the_components_authority_is_never_bound_and_never_open() {
    let mut stale = component(STALE);
    stale
        .holes
        .push(Hole::new("permits.fs.read", "human", None));
    let refused = instantiate(
        &stale,
        &[Binding::new(
            "permits.fs.read",
            json!(["./in/tickets.json"]),
        )],
    );
    assert_eq!(
        refused.unwrap_err(),
        BindingError::Authority("permits.fs.read".to_owned())
    );
    // Its permits never reach the document: the hole is closed by construction, and the
    // person's own boundary is the one Check judges.
    let instance = instantiate(&stale, &bound(48)).unwrap();
    assert!(instance.open.is_empty(), "{:?}", instance.open);
    let expansion = expand(PARENT, &instance).unwrap();
    assert!(expansion.ready, "{:#}", expansion.receipt["check"]);
    assert!(!expansion.candidate.contains("./data/tickets.json"));
}

#[test]
fn a_literal_with_quotes_newlines_and_other_scripts_round_trips_through_the_proof() {
    for value in [
        json!("say \"hi\"\nthen 'bye'"),
        json!("Écarte les lignes — 東京 ✓"),
        json!("a: b, [c] {d} # not a comment"),
        json!(["./in/a.json", "./in/b.json"]),
        json!({"type": "object", "required": ["id"]}),
    ] {
        let source = format!(
            "nika: x\nconst:\n  target: {}\n  other: kept # a comment\ntasks: {{}}\n",
            if value.is_string() {
                "./data/tickets.json".to_owned()
            } else {
                value_like(&value)
            }
        );
        let edited = edit_literal(&source, "const.target", &value).unwrap();
        let projection = literal_projection(&edited).unwrap();
        assert_eq!(projection["const"]["target"], value, "{edited}");
        assert!(
            edited.ends_with("  other: kept # a comment\ntasks: {}\n"),
            "{edited}"
        );
    }
}

/// A held literal of the same kind as `value`, written in flow form.
fn value_like(value: &Value) -> String {
    if value.is_array() {
        "[\"./data/x.json\"]".to_owned()
    } else {
        "{ type: string }".to_owned()
    }
}
