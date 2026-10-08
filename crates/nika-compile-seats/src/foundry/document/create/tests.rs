// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document door's own laws, zero calls: what an answer makes of the document (written,
//! composed over the author's envelope, edited over the last document), what it refuses, what the
//! record claims and what binds a READY document to its final bytes.

use super::{Made, ROUTE, bind, language, made, preservation, receipts, record};
use crate::foundry::component::pinned;
use crate::foundry::{Component, ComponentCatalog, ComponentRef, Hole, Release, Unresolved, reuse};
use nika_compile::surface::{initial, literal_projection, sha256};
use nika_compile::{CompileOutcome, CompileStatus};
use serde_json::{Value, json};

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

/// The author's own envelope: the name and the boundary the request states, no task yet.
const ENVELOPE: &str = r#"nika: stale-tickets-report
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
"#;

/// A complete document the author writes itself: typed inputs, a constant, a business condition.
const WRITTEN: &str = r#"nika: weekly-digest
inputs:
  team: { type: string, required: true }
  verbose: { type: bool, required: false, default: false }
const:
  window_days: 7
permits:
  tools: ["nika:log"]
tasks:
  announce:
    invoke: { tool: "nika:log", args: { level: info, message: "digest for ${{ inputs.team }}" } }
  debug_dump:
    when: "${{ inputs.verbose == true }}"
    invoke: { tool: "nika:log", args: { level: debug, message: "window ${{ const.window_days }}" } }
"#;

const VERSION: &str = "fixture-document-r1";
const SNAPSHOT: &str = "2222222222222222222222222222222222222222222222222222222222222222";

/// A catalogue of one release holding the stale-filter component.
struct Shelf;

impl ComponentCatalog for Shelf {
    fn release(&self) -> Release {
        Release::new(VERSION, SNAPSHOT, "nika-knowledge-release-profile/r1")
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        reference.block_name()?;
        pinned(reference, &self.release())?;
        if reference.id != "block:stale-filter-report" {
            return Err(Unresolved::Unknown(reference.id.clone()));
        }
        let mut component = Component::new(
            "block:stale-filter-report",
            self.release(),
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
}

fn compose(hours: u32) -> Value {
    json!({"op": "compose", "component": "block:stale-filter-report", "version": VERSION,
        "bindings_json": format!("{{\"const.records_path\": \"./in/tickets.json\", \"const.report_path\": \"./out/report.json\", \"const.max_age_hours\": {hours}}}")})
}

/// The author's envelope with the component composed into it at `hours`.
fn composed(hours: u32) -> Made {
    made(
        Some(ENVELOPE.to_owned()),
        &[compose(hours)],
        None,
        Some(&Shelf),
    )
    .expect("composed")
}

#[test]
fn a_written_document_is_taken_whole_and_claims_no_preservation() {
    let made = made(Some(WRITTEN.to_owned()), &[], None, None).expect("written");
    assert_eq!((made.mode, made.source.as_str()), ("written", WRITTEN));
    assert_eq!((made.base_sha256.as_deref(), made.operations), (None, 0));
    assert!(made.receipts.is_empty());
    assert!(preservation(&made).starts_with("none claimed"), "{made:?}");
}

#[test]
fn a_component_is_composed_into_the_authors_envelope_with_its_receipt() {
    let made = composed(48);
    assert_eq!(made.mode, "composed");
    assert_eq!(made.base_sha256.as_deref(), Some(sha256(ENVELOPE).as_str()));
    assert_eq!(
        (made.operations, made.verified, made.constructed),
        (1, 0, 1)
    );
    assert!(
        preservation(&made).starts_with("by construction"),
        "{made:?}"
    );
    // The envelope stays the author's: the component's own name, model and permits never land.
    let source = made.source.as_str();
    assert!(
        source.starts_with("nika: stale-tickets-report\n"),
        "{source}"
    );
    assert!(!source.contains("p90-stale-filter-report"), "{source}");
    assert!(!source.contains("./data/tickets.json"), "{source}");
    let read = literal_projection(source).expect("literal");
    assert_eq!(
        read["const"]["records_path"], "./in/tickets.json",
        "{read:#}"
    );
    assert_eq!(read["const"]["report_path"], "./out/report.json");
    let hours = json!({"type": "integer", "value": 48});
    assert_eq!(read["const"]["max_age_hours"], hours);
    assert_eq!(read["permits"]["fs"]["read"], json!(["./in/tickets.json"]));
    let wf = nika_compile::parse(source).expect("the composed document parses");
    let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    let tasks = [
        "read_records",
        "parse_records",
        "stale",
        "report_text",
        "write_report",
    ];
    assert_eq!(ids, tasks);
    // The receipt is witnessed on these bytes, digest for digest.
    let [receipt] = made.receipts.as_slice() else {
        panic!("one receipt: {:?}", made.receipts);
    };
    assert_eq!(receipt["component"]["id"], "block:stale-filter-report");
    let witnessed = reuse(&[], &made.receipts, Some(source));
    let counts = (witnessed["expanded"].as_u64(), witnessed["absent"].as_u64());
    assert_eq!(counts, (Some(1), Some(0)));
}

#[test]
fn operations_over_the_last_document_edit_only_what_they_address() {
    let last = composed(48);
    // A later round states one edit over the last document, with no text of its own.
    let edit = json!({"op": "set", "path": "/outputs/stale",
        "value_json": "\"${{ tasks.stale.output }}\""});
    let rebind = json!({"op": "rebind", "component": "block:stale-filter-report",
        "bindings_json": "{\"const.max_age_hours\": 72}"});
    let made = made(None, &[edit, rebind], Some(&last), Some(&Shelf)).expect("edited");
    assert_eq!(made.base_sha256, Some(sha256(&last.source)));
    assert_eq!((made.verified, made.constructed), (1, 1));
    let changed: Vec<(&str, &str)> = (last.source.lines().zip(made.source.lines()))
        .filter(|(a, b)| a != b)
        .collect();
    let moved = [(
        "  max_age_hours: { type: integer, value: 48 }",
        "  max_age_hours: { type: integer, value: 72 }",
    )];
    assert_eq!(
        changed, moved,
        "only the bound literal moves; the set restates an equal value"
    );
    // The receipt followed the rebind: still witnessed as expanded on the new bytes.
    let witnessed = reuse(&[], &made.receipts, Some(&made.source));
    assert_eq!(witnessed["expanded"].as_u64(), Some(1), "{witnessed:#}");
}

#[test]
fn an_answer_with_nothing_to_apply_to_is_refused_by_its_reason() {
    let empty = made(None, &[], None, None).expect_err("no document");
    assert!(empty[0].contains("states no document"), "{empty:?}");
    let blind = made(None, &[compose(48)], None, Some(&Shelf)).expect_err("no base");
    assert!(blind[0].contains("need a document"), "{blind:?}");
    // A component the catalogue does not hold is refused by name; nothing is applied.
    let unknown = json!({"op": "compose", "component": "block:nowhere", "version": VERSION,
        "bindings_json": "{}"});
    let envelope = || Some(ENVELOPE.to_owned());
    let why = made(envelope(), &[unknown], None, Some(&Shelf)).expect_err("unknown component");
    assert!(why[0].starts_with("operation 0"), "{why:?}");
    // No catalogue lent: a compose is refused, never guessed from the shown text.
    let lent = made(envelope(), &[compose(48)], None, None).expect_err("no catalogue");
    assert!(lent[0].contains("no component catalogue"), "{lent:?}");
}

/// A READY outcome of the door: the native record its conclusion wrote, with the door's section.
fn concluded(status: CompileStatus, candidate: &str) -> CompileOutcome {
    let mut out = initial();
    out.status = status;
    out.candidate = Some(candidate.to_owned());
    let receipt = json!({"component": {"id": "block:stale-filter-report"}});
    out.provenance.plan = Some(json!({
        "strategy": "native", "intent_sha256": "i", "source": "pre-answer",
        "questions": [], "gaps": [],
        "document_create": {"route": ROUTE, "mode": "composed", "resolved": "the request",
            "base_sha256": "envelope", "operations": 1,
            "changed": ["component block:stale-filter-report"], "preservation": "p",
            "components": [receipt]},
    }));
    out.provenance.decision = Some(json!({"document_create": {"components": [receipt]}}));
    out
}

#[test]
fn a_ready_document_is_bound_to_its_final_bytes_and_nothing_else_is() {
    let final_bytes = "nika: x\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";
    let mut ready = concluded(CompileStatus::Ready, final_bytes);
    bind(&mut ready);
    let plan = ready.provenance.plan.as_ref().expect("plan");
    assert_eq!(
        plan["strategy"], "native",
        "the native record stays replayable"
    );
    assert_eq!(plan["source"], "pre-answer");
    let document = &plan["document"];
    assert_eq!(document["version"], 1);
    assert_eq!(document["candidate_sha256"], json!(sha256(final_bytes)));
    assert_eq!(document["request"], "the request");
    assert_eq!(document["base_sha256"], Value::Null);
    assert_eq!(document["mode"], "composed");
    assert_eq!(document["components"].as_array().map(Vec::len), Some(1));
    assert!(plan.get("source_revision").is_none(), "{plan:#}");
    assert_eq!(receipts(&ready).len(), 1);
    // The decision is restated on the same final bytes: their digest, each receipt witnessed on
    // them (this receipt names no node there, so nothing is claimed expanded).
    let decided = &ready.provenance.decision.as_ref().expect("decision")["document_create"];
    assert_eq!(decided["candidate_sha256"], json!(sha256(final_bytes)));
    let counts = (
        decided["reuse"]["expanded"].as_u64(),
        decided["reuse"]["absent"].as_u64(),
    );
    assert_eq!(counts, (Some(0), Some(1)));
    // A question still open: no final bytes are bound yet, and the decision is left as it was.
    let mut open = concluded(CompileStatus::Incomplete, final_bytes);
    bind(&mut open);
    let unbound = open.provenance.plan.as_ref().expect("plan");
    assert!(unbound.get("document").is_none(), "{unbound:#}");
    let left = &open.provenance.decision.as_ref().expect("decision")["document_create"];
    assert!(
        left.get("candidate_sha256").is_none() && left.get("reuse").is_none(),
        "{left:#}"
    );
    // An answer round replays the record, never the door: the decision is restated from the
    // record's section, on the final bytes.
    let mut replayed = concluded(CompileStatus::Ready, final_bytes);
    replayed.provenance.decision = None;
    bind(&mut replayed);
    let restated = &replayed.provenance.decision.as_ref().expect("restated")["document_create"];
    assert_eq!(
        restated["candidate_sha256"],
        json!(sha256(final_bytes)),
        "{restated:#}"
    );
    let made = (restated["mode"].as_str(), restated["operations"].as_u64());
    assert_eq!(made, (Some("composed"), Some(1)));
    let base = (restated["base_sha256"].as_str(), restated["route"].as_str());
    assert_eq!(base, (Some("envelope"), Some(ROUTE)));
    assert_eq!(restated["components"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        restated["reuse"]["absent"], 1,
        "a receipt naming no node is never expanded"
    );
    // Another door's record is never touched.
    let mut other = concluded(CompileStatus::Ready, final_bytes);
    other.provenance.plan = Some(json!({"strategy": "native", "source": "s"}));
    bind(&mut other);
    assert_eq!(
        other.provenance.plan,
        Some(json!({"strategy": "native", "source": "s"}))
    );
    assert!(receipts(&initial()).is_empty());
}

/// While a mandatory question holds the candidate back, the door's record witnesses each receipt
/// on the document the door made: the expansion is there, so it is never reported absent for
/// want of a candidate, and no candidate digest is claimed.
#[test]
fn a_document_a_question_holds_back_is_witnessed_on_the_doors_own_bytes() {
    let made = composed(48);
    let mut held = initial();
    held.candidate = None;
    held.provenance.plan = Some(json!({"strategy": "native", "source": made.source}));
    record("the request", &made, &mut held);
    let decided = &held.provenance.decision.as_ref().expect("decision")["document_create"];
    assert_eq!(decided["candidate_sha256"], Value::Null, "{decided:#}");
    let counts = (
        decided["reuse"]["expanded"].as_u64(),
        decided["reuse"]["absent"].as_u64(),
    );
    assert_eq!(counts, (Some(1), Some(0)));
    let how = (decided["mode"].as_str(), decided["operations"].as_u64());
    assert_eq!(how, (Some("composed"), Some(1)));
    // The continuation carries the door's section for the bind an answer round makes.
    let section = &held.provenance.plan.as_ref().expect("plan")["document_create"];
    assert_eq!(section["resolved"], "the request");
    assert_eq!(section["components"].as_array().map(Vec::len), Some(1));
    assert_eq!(section["base_sha256"], json!(sha256(ENVELOPE)));
    assert_eq!(section["operations"], 1);
    // Another door's plan never receives the section.
    let mut other = initial();
    other.provenance.plan = Some(json!({"strategy": "hot"}));
    record("the request", &made, &mut other);
    assert_eq!(other.provenance.plan, Some(json!({"strategy": "hot"})));
}

#[test]
fn the_language_the_author_reads_is_the_specs_own_schema() {
    let sent: Value = serde_json::from_str(&language()).expect("compact JSON");
    let spec: Value = serde_json::from_str(nika_pack::schema_json()).expect("the pack's schema");
    assert_eq!(sent, spec);
    assert!(
        sent["properties"]["tasks"].is_object(),
        "the whole envelope rides it"
    );
}
