// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document door as a compile runs it: the answer it asks the author for, then SCRIPTED
//! seats over the real compile entry with a lent catalogue (an admitted component expanded at
//! creation; the created document remembered, reopened and changed in words). What an answer
//! makes of the document and the record that follows it are tested where those laws live
//! (`nika_compile_seats::foundry::document::create`).

use super::{DocumentAnswer, answer_schema};
use nika_compile::surface::sha256;
use nika_compile_seats::foundry::component::pinned;
use nika_compile_seats::foundry::{
    Component, ComponentCatalog, ComponentRef, Hole, Release, Unresolved,
};
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
    /// The release's admitted row, as a real release lists it: what the author is shown.
    fn entries(&self) -> Vec<Value> {
        vec![json!({"id": "block:stale-filter-report", "kind": "block",
            "title": "Records older than a threshold, counted and written",
            "purpose": "Keep the records whose age_hours exceeds the threshold, write them.",
            "file": "blocks/stale-filter-report.nika", "file_sha256": sha256(STALE),
            "holes": [{"name": "const.records_path", "owner": "human"},
                      {"name": "const.report_path", "owner": "human"},
                      {"name": "const.max_age_hours", "owner": "human"}],
            "effects": ["fs.read", "fs.write"]})]
    }
}

fn answer(value: Value) -> DocumentAnswer {
    serde_json::from_value(value).expect("a document answer")
}

fn compose(hours: u32) -> Value {
    json!({"op": "compose", "component": "block:stale-filter-report", "version": VERSION,
        "bindings_json": format!("{{\"const.records_path\": \"./in/tickets.json\", \"const.report_path\": \"./out/report.json\", \"const.max_age_hours\": {hours}}}")})
}

#[test]
fn the_answer_schema_widens_the_whole_source_answer_with_the_documents_operations() {
    let schema = answer_schema();
    assert_eq!(schema["additionalProperties"], false, "{schema:#}");
    let required: Vec<&str> = (schema["required"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    for key in [
        "candidate",
        "candidate_lines",
        "operations",
        "questions",
        "gaps",
        "notes",
    ] {
        assert!(required.contains(&key), "{key}: {required:?}");
    }
    let ops = &schema["properties"]["operations"]["items"]["properties"]["op"]["enum"];
    for op in ["set", "insert_text", "rename", "compose", "rebind"] {
        assert!(
            ops.as_array().is_some_and(|all| all.contains(&json!(op))),
            "{op}: {ops}"
        );
    }
}

#[test]
fn an_answer_writes_its_document_whole_or_line_by_line() {
    let whole = answer(json!({"candidate": WRITTEN})).written();
    assert_eq!(whole.as_deref(), Some(WRITTEN));
    let lines: Vec<&str> = WRITTEN.split('\n').collect();
    let joined = answer(json!({"candidate_lines": lines})).written();
    assert_eq!(joined.as_deref(), Some(WRITTEN));
    // Blank text is no document: operations then apply to the last one the door made.
    let blank = answer(json!({"candidate": "  \n", "candidate_lines": ["", " "]}));
    assert_eq!(blank.written(), None);
    // The text wins when both are stated.
    let both = answer(json!({"candidate": WRITTEN, "candidate_lines": ["nika: other"]}));
    assert_eq!(both.written().as_deref(), Some(WRITTEN));
}

/// The audit's request for the stale-records report (its phrasing kept verbatim).
const STALE_INTENT: &str = "Create a new workflow that reads ./in/tickets.json. Each row has an id and a numeric age_hours. Keep records whose age_hours is strictly greater than 48, preserve their input order, and write a JSON object containing count and ids to ./out/report.json. Expose the selected records as the named output stale. Use an applicable admitted Foundry component when the catalogue provides one, with these exact paths and threshold bound; otherwise construct the same requested work. Missing catalogue coverage must not remove the report requirement. The request authorizes only the stated file read/write and the read, jq and write tools.";

/// A SCRIPTED seat over the real compile entry: the authoring answers in order (the document door
/// and the revision door both ask under a schema with `operations`), and every verifier question
/// approved by its own option. It records the opening of each authoring call.
struct Scripted {
    answers: std::sync::Mutex<Vec<String>>,
    openings: std::sync::Mutex<Vec<String>>,
}

impl Scripted {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers: std::sync::Mutex::new(answers),
            openings: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn openings(&self) -> Vec<String> {
        self.openings.lock().expect("openings").clone()
    }
}

/// The option a verifier question offers for an approval.
fn approval(schema: &Value) -> &'static str {
    let keys = schema["properties"]["choice"]["enum"].to_string();
    let offered = |key: &str| keys.contains(&format!("\"{key}\""));
    ["faithful", "only_requested", "consistent", "carried"]
        .into_iter()
        .find(|key| offered(key))
        .unwrap_or("omitted")
}

impl nika_kernel::ai::provider::ProviderInferDyn for Scripted {
    async fn infer(
        &self,
        request: nika_kernel::ai::provider::InferRequest,
    ) -> Result<nika_kernel::ai::provider::InferResponse, nika_kernel::ai::provider::ProviderError>
    {
        use nika_kernel::ai::provider::{
            ContentBlock, InferResponse, ProviderError, ResponseFormat, StopReason, TokenUsage,
        };
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            return Err(ProviderError::Other {
                reason: "a structured answer was expected".to_owned(),
            });
        };
        let authoring = schema["properties"]["operations"].is_object();
        let text = if authoring {
            let opening = (request.messages.get(1)).map_or_else(String::new, |m| format!("{m:?}"));
            self.openings.lock().expect("openings").push(opening);
            let mut answers = self.answers.lock().expect("answers");
            (!answers.is_empty())
                .then(|| answers.remove(0))
                .ok_or_else(|| ProviderError::Other {
                    reason: "the scripted seat has no further answer".to_owned(),
                })?
        } else {
            json!({"choice": approval(schema)}).to_string()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(10, 5),
            StopReason::EndTurn,
        ))
    }
}

fn policy() -> crate::AuthoringPolicy {
    crate::AuthoringPolicy::new("mock/authoring", 8192, std::time::Duration::from_secs(5))
        .with_native(crate::NativeMode::Escalate)
}

/// The document door's answer: the author's document and the operations over it.
fn door(candidate: &str, operations: &[Value]) -> String {
    json!({"candidate": candidate, "candidate_lines": [], "operations": operations,
        "questions": [], "gaps": [], "notes": "scripted"})
    .to_string()
}

/// A pack the session lends beside the catalogue: it does not hold the component.
fn pack() -> nika_compile::AuthoringKnowledge {
    nika_compile::AuthoringKnowledge {
        references: vec![nika_compile::KnowledgeReference {
            kind: "skill".to_owned(),
            id: "skill:report-writing".to_owned(),
            text: "Write a short report from a computed result.".to_owned(),
        }],
        ..nika_compile::AuthoringKnowledge::default()
    }
}

async fn compiled(request: &crate::CompileRequest, seat: &Scripted) -> crate::CompileOutcome {
    let cognition = crate::Cognition {
        provider: Some(seat),
        seat: None,
    };
    crate::compile_with_cognition_composed(request, cognition, None, Some(&Shelf))
        .await
        .expect("compiles")
}

/// The roles every call of the outcome's receipt journals.
fn calls(out: &crate::CompileOutcome) -> Vec<String> {
    (out.provenance.authoring.as_ref())
        .map(|receipt| {
            (receipt.context.iter())
                .filter_map(|call| call["call"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A fresh request, a catalogue lent beside a pack that lacks the component: the author's first
/// call composes it by identity into its own envelope; the expansion is receipted, witnessed on
/// the final bytes and READY only after the strict parser, Check and the judge.
#[tokio::test]
async fn a_lent_component_is_expanded_at_creation_and_witnessed_on_the_final_bytes() {
    let author = Scripted::new(vec![door(ENVELOPE, &[compose(48)])]);
    let request = crate::CompileRequest::create(STALE_INTENT)
        .with_authoring_policy(policy())
        .with_authoring_knowledge(pack());
    let out = compiled(&request, &author).await;
    assert_eq!(
        out.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        out.diagnostics
    );
    assert_eq!(calls(&out).first().map(String::as_str), Some("document"));
    let authored = calls(&out)
        .iter()
        .filter(|c| c.starts_with("document"))
        .count();
    assert_eq!(authored, 1, "{:?}", calls(&out));
    let candidate = out.candidate.clone().expect("a candidate");
    // The author was shown the admitted component by identity, holes and effects, not its bytes.
    let opening = &author.openings()[0];
    assert!(opening.contains("block:stale-filter-report"), "{opening}");
    assert!(!opening.contains("p90-stale-filter-report"), "{opening}");
    let decision = out.provenance.decision.as_ref().expect("decision");
    let created = &decision["document_create"];
    assert_eq!(created["mode"], "composed", "{created:#}");
    let [receipt] = created["components"]
        .as_array()
        .expect("receipts")
        .as_slice()
    else {
        panic!("one receipt: {created:#}");
    };
    assert_eq!(receipt["component"]["id"], "block:stale-filter-report");
    assert_eq!(receipt["component"]["release"]["version"], VERSION);
    assert_eq!(receipt["component"]["release"]["snapshot_sha256"], SNAPSHOT);
    assert_eq!(receipt["component"]["file_sha256"], json!(sha256(STALE)));
    assert_eq!(
        receipt["candidate_sha256"],
        json!(sha256(&candidate)),
        "bound to the final bytes"
    );
    // Every hole bound to the request's own literal (the receipt lists them by path).
    let mut bound: Vec<(&str, &Value)> = (receipt["bindings"].as_array().into_iter().flatten())
        .map(|b| (b["path"].as_str().unwrap_or_default(), &b["bound"]))
        .collect();
    bound.sort_by_key(|(path, _)| *path);
    assert_eq!(
        bound,
        [
            ("const.max_age_hours", &json!(48)),
            ("const.records_path", &json!("./in/tickets.json")),
            ("const.report_path", &json!("./out/report.json")),
        ]
    );
    assert_eq!(created["reuse"]["expanded"], 1, "{created:#}");
    // The qualification record tells the expansion apart from what was only shown: the skill
    // and the folded recall are consulted, the component alone is expanded.
    let qualification = &decision["knowledge_qualification"];
    assert_eq!(qualification["reuse"]["expanded"], 1, "{qualification:#}");
    let rows = qualification["reuse"]["references"]
        .as_array()
        .expect("rows");
    let used = |id: &str| rows.iter().find(|r| r["id"] == id).map(|r| &r["use"]);
    let block = used("block:stale-filter-report");
    assert_eq!(block, Some(&json!("expanded")), "{qualification:#}");
    let skill = used("skill:report-writing");
    assert_eq!(skill, Some(&json!("consulted")), "shown, never reused");
    let consulted = rows.iter().filter(|r| r["use"] == "consulted").count();
    assert_eq!(qualification["reuse"]["consulted"], json!(consulted));
    assert_eq!(
        rows.len(),
        consulted + 1,
        "nothing else is claimed: {rows:#?}"
    );
    let read = nika_compile::surface::literal_projection(&candidate).expect("literal");
    let boundary = json!({"fs": {"read": ["./in/tickets.json"], "write": ["./out/report.json"]},
        "tools": ["nika:read", "nika:jq", "nika:write"]});
    assert_eq!(
        read["permits"], boundary,
        "the author's boundary, never the component's"
    );
    assert_eq!(read["outputs"]["stale"], "${{ tasks.stale.output }}");
    let plan = out.provenance.plan.as_ref().expect("a plan");
    assert_eq!(
        plan["document"]["candidate_sha256"],
        json!(sha256(&candidate))
    );
    assert_eq!(plan["document"]["mode"], "composed");
    assert_eq!(
        plan["document"]["components"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        created["candidate_sha256"],
        json!(sha256(&candidate)),
        "one final digest"
    );
    // The machine document every transport emits carries the record as it is.
    let wire = nika_compile::outcome_document(&out);
    assert_eq!(wire["provenance"]["plan"]["document"], plan["document"]);
    assert_eq!(wire["provenance"]["decision"]["document_create"], *created);
}

/// The created document as a session keeps it: remembered beside its exact bytes, given back for
/// them on reopen, then changed in words. The record-less document revision reads the request
/// and the composed receipt from it, rebinds 48 to 72 and moves nothing else.
#[tokio::test]
async fn a_created_component_is_remembered_reopened_and_rebound_by_a_new_change() {
    use nika_compile_fidelity::sketch::kept::{Place, binds, original, plan, remember};
    let author = Scripted::new(vec![door(ENVELOPE, &[compose(48)])]);
    let request = crate::CompileRequest::create(STALE_INTENT).with_authoring_policy(policy());
    let created = compiled(&request, &author).await;
    assert_eq!(
        created.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        created.diagnostics
    );
    let base = created.candidate.clone().expect("a candidate");
    let record = created.provenance.plan.clone().expect("a record");
    let mut kept = None;
    let place = Place::Saved("stale-report.nika");
    remember(&mut kept, place, &base, Some(&record), &|text| {
        text.to_owned()
    });
    let reopened = plan(kept.as_ref(), place, &base).expect("the record binds the saved bytes");
    assert_eq!(reopened, record);
    assert_eq!(original(&reopened), Some(STALE_INTENT));
    // A new change in words over the reopened bytes: never an answer round of the creation.
    let rebind = json!({"op": "rebind", "path": "", "value_json": "",
        "component": "block:stale-filter-report", "version": "",
        "bindings_json": "{\"const.max_age_hours\": 72}"});
    let links = json!({"supersedes": [], "adds": [], "notes": "", "operations": [rebind],
        "replace": ""});
    let editor = Scripted::new(vec![links.to_string()]);
    let edit = crate::CompileRequest::edit(base.clone(), "Raise the age threshold to 72 hours.")
        .with_authoring_policy(policy())
        .with_plan(reopened);
    let revised = compiled(&edit, &editor).await;
    assert_eq!(
        revised.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        revised.diagnostics
    );
    assert_eq!(
        calls(&revised).first().map(String::as_str),
        Some("revision")
    );
    let opening = &editor.openings()[0];
    assert!(
        opening.contains("block:stale-filter-report"),
        "the receipt was told: {opening}"
    );
    let bytes = revised.candidate.clone().expect("revised bytes");
    let moved: Vec<(&str, &str)> = (base.lines().zip(bytes.lines()))
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(
        moved,
        [(
            "  max_age_hours: { type: integer, value: 48 }",
            "  max_age_hours: { type: integer, value: 72 }"
        )],
        "only the bound literal moves"
    );
    assert_eq!(base.lines().count(), bytes.lines().count());
    let next = revised
        .provenance
        .plan
        .as_ref()
        .expect("the revision's record");
    assert!(binds(next, &bytes), "{next:#}");
    let said = original(next).expect("the request the revised bytes answer");
    assert!(
        said.contains(STALE_INTENT) && said.contains("72 hours"),
        "{said}"
    );
    let components = next["document_revision"]["components"]
        .as_array()
        .expect("receipts");
    let witnessed = nika_compile_seats::foundry::reuse(&[], components, Some(&bytes));
    assert_eq!(
        witnessed["expanded"], 1,
        "the receipt followed the bytes: {witnessed:#}"
    );
}
