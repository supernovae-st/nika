// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document door as a compile runs it: the answer it asks the author for, then SCRIPTED
//! seats over the real compile entry with a lent catalogue (an admitted component expanded at
//! creation; the created document remembered, reopened and changed in words). What an answer
//! makes of the document and the record that follows it are tested where those laws live
//! (`nika_compile_seats::foundry::document::create`).

use super::{DocumentAnswer, answer_schema};
use crate::rehearse::{
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
    RehearsedOutput, RoomEvidence, Spent,
};
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
/// approved by its own option. It records the opening of each authoring call and the STATE each
/// verifier question carried.
struct Scripted {
    answers: std::sync::Mutex<Vec<String>>,
    openings: std::sync::Mutex<Vec<String>>,
    states: std::sync::Mutex<Vec<Value>>,
    /// Whether the whole request is judged from the transmitted engine facts alone: unfaithful
    /// when the catalogue offered a component and none is composed, else faithful.
    conditional: bool,
    /// Whether the whole request is always doubted (`unfaithful`), every other question approved.
    doubting: bool,
    /// The kind of each verifier question asked: `whole`, `observed` or `part`.
    asked: std::sync::Mutex<Vec<&'static str>>,
}

impl Scripted {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers: std::sync::Mutex::new(answers),
            openings: std::sync::Mutex::new(Vec::new()),
            states: std::sync::Mutex::new(Vec::new()),
            conditional: false,
            doubting: false,
            asked: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The same seat, always doubting the whole request and approving every other question.
    fn doubting(answers: Vec<String>) -> Self {
        Self {
            doubting: true,
            ..Self::new(answers)
        }
    }

    /// How many verifier questions of `kind` were asked.
    fn asked(&self, kind: &str) -> usize {
        let asked = self.asked.lock().expect("asked");
        asked.iter().filter(|k| **k == kind).count()
    }

    /// The same seat, judging the whole request from the engine facts it is shown.
    fn conditional(answers: Vec<String>) -> Self {
        Self {
            conditional: true,
            ..Self::new(answers)
        }
    }

    fn openings(&self) -> Vec<String> {
        self.openings.lock().expect("openings").clone()
    }

    /// The engine facts every verifier question showed, the same in each.
    fn judged_facts(&self) -> Value {
        let states = self.states.lock().expect("states").clone();
        let facts: Vec<&Value> = states.iter().filter_map(|s| s.get("authoring")).collect();
        assert!(
            !facts.is_empty(),
            "the judge was shown the facts: {states:#?}"
        );
        assert!(facts.iter().all(|f| *f == facts[0]), "{facts:#?}");
        facts[0].clone()
    }
}

/// The STATE a verifier question carries (`STATE:` up to its options), parsed.
fn state_of(request: &nika_kernel::ai::provider::InferRequest) -> Option<Value> {
    use nika_kernel::ai::provider::ContentBlock;
    let text: String = (request.messages.iter())
        .flat_map(|message| message.content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let start = text.find("STATE:\n")? + "STATE:\n".len();
    let end = text[start..]
        .find("\n\nOPTIONS:")
        .map_or(text.len(), |at| start + at);
    serde_json::from_str(&text[start..end]).ok()
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
            let state = state_of(&request);
            let whole = schema["properties"]["choice"]["enum"].to_string();
            let kind = if whole.contains("\"unfaithful\"") {
                "whole"
            } else if whole.contains("\"unexercised\"") {
                "observed"
            } else {
                "part"
            };
            self.asked.lock().expect("asked").push(kind);
            let choice = match &state {
                Some(_) if self.doubting && kind == "whole" => "unfaithful",
                Some(state) if self.conditional && whole.contains("\"unfaithful\"") => {
                    let facts = &state["authoring"];
                    let offered = facts["offered"]["total"].as_u64().unwrap_or(0) > 0;
                    let composed = (facts["composed"].as_array().into_iter().flatten())
                        .any(|seen| seen["verdict"] == "expanded");
                    if offered && !composed {
                        "unfaithful"
                    } else {
                        "faithful"
                    }
                }
                _ => approval(schema),
            };
            if let Some(state) = state {
                self.states.lock().expect("states").push(state);
            }
            json!({"choice": choice}).to_string()
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
    compiled_under(request, seat, &Shelf).await
}

/// The compile of `request` with `catalog` lent.
async fn compiled_under(
    request: &crate::CompileRequest,
    seat: &Scripted,
    catalog: &dyn ComponentCatalog,
) -> crate::CompileOutcome {
    let cognition = crate::Cognition {
        provider: Some(seat),
        seat: None,
    };
    crate::compile_with_cognition_composed(request, cognition, None, Some(catalog))
        .await
        .expect("compiles")
}

/// The compile of `request` with `host`'s room and `catalog` lent.
async fn compiled_with(
    request: &crate::CompileRequest,
    seat: &Scripted,
    host: Option<&dyn Rehearse>,
    catalog: Option<&dyn ComponentCatalog>,
) -> crate::CompileOutcome {
    let cognition = crate::Cognition {
        provider: Some(seat),
        seat: None,
    };
    crate::compile_with_cognition_composed(request, cognition, host, catalog)
        .await
        .expect("compiles")
}

/// A catalogue of another release that offers no component.
struct Bare;

impl ComponentCatalog for Bare {
    fn release(&self) -> Release {
        Release::new(
            "fixture-document-r0",
            SNAPSHOT,
            "nika-knowledge-release-profile/r1",
        )
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        Err(Unresolved::Unknown(reference.id.clone()))
    }
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

/// The judge reads the engine facts the request conditions on (A5, T3), as the door recorded them
/// for the bytes it made: the catalogue release it was lent, the components it offered its
/// author, and each receipt witnessed on those bytes (composed: expanded). The same bytes written
/// whole under the same catalogue carry no receipt: nothing is composed, the offer stands.
#[tokio::test]
async fn the_judge_reads_the_lent_release_and_what_the_document_composed() {
    let request = stale_request();
    let release = Shelf.release().record();
    let author = Scripted::new(vec![door(ENVELOPE, &[compose(48)])]);
    let out = compiled(&request, &author).await;
    assert_eq!(
        out.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        out.diagnostics
    );
    let facts = author.judged_facts();
    assert_eq!(facts["catalogue"], release);
    assert_eq!(facts["offered"]["total"], 1, "{facts:#}");
    let offered = &facts["offered"]["components"][0];
    assert_eq!(offered["component"]["id"], "block:stale-filter-report");
    let composed = facts["composed"].as_array().cloned().unwrap_or_default();
    assert_eq!(composed.len(), 1, "{facts:#}");
    assert_eq!(composed[0]["component"], "block:stale-filter-report");
    assert_eq!(composed[0]["verdict"], "expanded");
    let written = out.candidate.clone().expect("the expanded document");
    let author = Scripted::new(vec![door(&written, &[])]);
    let out = compiled(&request, &author).await;
    assert_eq!(
        out.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        out.diagnostics
    );
    let facts = author.judged_facts();
    assert_eq!(
        (&facts["catalogue"], &facts["offered"]["total"]),
        (&release, &json!(1))
    );
    assert_eq!(facts["composed"], json!([]));
}

/// The stale-tickets request with the pack lent beside the catalogue.
fn stale_request() -> crate::CompileRequest {
    crate::CompileRequest::create(STALE_INTENT)
        .with_authoring_policy(policy())
        .with_authoring_knowledge(pack())
}

/// The document the stale-filter component expands into, as a valid composition makes it.
async fn expanded() -> String {
    let author = Scripted::new(vec![door(ENVELOPE, &[compose(48)])]);
    let out = compiled(&stale_request(), &author).await;
    out.candidate.expect("the expanded document")
}

/// A receipt a whole rewrite left behind is never shown as current composition (A5, review F1).
/// The first document composes the block over an envelope granting a path no word of the
/// request composes, and is refused; the author rewrites the whole document. The record keeps the earlier
/// receipt as lineage, but the judge is shown it witnessed on the rewritten bytes: expanded when
/// the rewrite keeps the component's nodes, absent when it renames them away.
#[tokio::test]
async fn a_receipt_a_rewrite_left_behind_is_shown_absent_never_composed() {
    let invented = ENVELOPE.replace(
        "read: [\"./in/tickets.json\"]",
        "read: [\"./in/tickets.json\", \"./zz/qqq.json\"]",
    );
    assert_ne!(invented, ENVELOPE);
    let kept = expanded().await;
    let renamed = [
        ("read_records", "load_tickets"),
        ("parse_records", "decode_tickets"),
        ("report_text", "render_report"),
        ("write_report", "save_report"),
    ];
    let dropped = renamed
        .iter()
        .fold(kept.clone(), |text, (old, new)| text.replace(old, new));
    for (rewrite, verdict) in [(kept, "expanded"), (dropped, "absent")] {
        let author = Scripted::new(vec![door(&invented, &[compose(48)]), door(&rewrite, &[])]);
        let out = compiled(&stale_request(), &author).await;
        assert_eq!(
            out.status,
            crate::CompileStatus::Ready,
            "{:#?}",
            out.diagnostics
        );
        assert_eq!(
            calls(&out)
                .iter()
                .filter(|c| c.starts_with("document"))
                .count(),
            2
        );
        let record = &out.provenance.plan.as_ref().expect("the record")["document_create"];
        let lineage = record["components"].as_array().map_or(0, Vec::len);
        assert_eq!(lineage, 1, "the earlier receipt is kept: {record:#}");
        let facts = author.judged_facts();
        assert_eq!(facts["composed"][0]["verdict"], verdict, "{facts:#}");
    }
}

/// The request's conditional ("use an applicable admitted component when the catalogue provides
/// one; otherwise construct the same requested work") is judged on what the catalogue offered
/// (A5, review F2): the same written document is shown the stale-filter offer under a catalogue
/// that has it and an empty offer under one that has none, nothing composed either way. A judge
/// deciding from those facts alone rejects the first (held, its author not asked again) and
/// accepts the second.
#[tokio::test]
async fn the_conditional_foundry_clause_is_judged_on_what_the_catalogue_offered() {
    let written = expanded().await;
    let cases: [(&dyn ComponentCatalog, u64, crate::CompileStatus); 2] = [
        (&Shelf, 1, crate::CompileStatus::Incomplete),
        (&Bare, 0, crate::CompileStatus::Ready),
    ];
    for (catalog, offered, status) in cases {
        let author = Scripted::conditional(vec![door(&written, &[])]);
        let out = compiled_under(&stale_request(), &author, catalog).await;
        let facts = author.judged_facts();
        assert_eq!(facts["offered"]["total"], offered, "{facts:#}");
        assert_eq!(facts["composed"], json!([]));
        assert_eq!(facts["catalogue"], catalog.release().record());
        assert_eq!(out.status, status, "{:#?}", out.diagnostics);
        assert_eq!(
            calls(&out)
                .iter()
                .filter(|c| c.starts_with("document"))
                .count(),
            1
        );
    }
}

/// A revision is judged on its own facts, never its base's (A5): the creation composes the block
/// (its judge shown it expanded), then a change in words rewrites the whole document with the
/// block's tasks renamed away. The revision records its own facts on the revised bytes: the
/// receipt it carries is shown absent there, beside the same lent release and offer, never the
/// creation's expanded witness.
#[tokio::test]
async fn a_revision_is_judged_on_its_own_facts_never_its_bases() {
    let author = Scripted::new(vec![door(ENVELOPE, &[compose(48)])]);
    let created = compiled(&stale_request(), &author).await;
    assert_eq!(
        created.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        created.diagnostics
    );
    assert_eq!(author.judged_facts()["composed"][0]["verdict"], "expanded");
    let base = created.candidate.clone().expect("the created bytes");
    let record = created.provenance.plan.clone().expect("its record");
    let renamed = [
        ("read_records", "load_tickets"),
        ("parse_records", "decode_tickets"),
        ("report_text", "render_report"),
        ("write_report", "save_report"),
        ("value: 48 }", "value: 72 }"),
    ];
    let rewritten = renamed
        .iter()
        .fold(base.clone(), |text, (old, new)| text.replace(old, new));
    assert_ne!(rewritten, base);
    let answer = json!({"supersedes": [], "adds": [], "notes": "", "operations": [],
        "replace": rewritten});
    let editor = Scripted::new(vec![answer.to_string()]);
    let edit = crate::CompileRequest::edit(base, "Raise the age threshold to 72 hours.")
        .with_authoring_policy(policy())
        .with_plan(record);
    let revised = compiled(&edit, &editor).await;
    assert_eq!(
        revised.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        revised.diagnostics
    );
    assert_eq!(revised.candidate.as_deref(), Some(rewritten.as_str()));
    let facts = editor.judged_facts();
    assert_eq!(
        facts["composed"][0]["component"],
        "block:stale-filter-report"
    );
    assert_eq!(facts["composed"][0]["verdict"], "absent", "{facts:#}");
    assert_eq!(facts["catalogue"], Shelf.release().record());
    assert_eq!(facts["offered"]["total"], 1);
}

/// The tickets workflow a session saved after creating it, and the exact draft its author gave
/// for raising the threshold from 48 to 72 hours (the source alone, no run or judgment kept).
const SAVED: &str = include_str!("../../../tests/fixtures/edit_trial/base.nika");
const DRAFT: &str = include_str!("../../../tests/fixtures/edit_trial/draft.nika");
/// The tickets the room copies in, and the report the draft's run writes from them.
const TICKETS: &str = r#"[{"id":"fresh-10","age_hours":10},{"id":"stale-60","age_hours":60},{"id":"boundary-72","age_hours":72},{"id":"stale-90","age_hours":90}]"#;
const REPORT: &str = r#"{"count":1,"ids":["stale-90"]}"#;

/// A room that runs each candidate it is shown to completion (the tickets copied in, the
/// report written) and keeps every candidate it ran.
#[derive(Default)]
struct Room {
    shown: std::sync::Mutex<Vec<String>>,
}

impl Rehearse for Room {
    fn bound(&self) -> std::time::Duration {
        std::time::Duration::from_secs(10)
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        _inputs: &'a [String],
        _targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            self.shown.lock().expect("shown").push(candidate.to_owned());
            ran(candidate)
        })
    }
}

/// The room's report of one completed run of `candidate`, every receipt agreeing with the bytes
/// it spent.
fn ran(candidate: &str) -> RehearsalReport {
    let (source, target) = ("./in/tickets.json", "./out/report.json");
    let input = Digest::of(TICKETS.as_bytes());
    let mut observed = Observation::none();
    observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
    let copy = CopyReceipt::new(
        source,
        input.clone(),
        Some(input),
        Held::Whole(TICKETS.to_owned()),
    );
    observed.copies = vec![copy];
    let state = FinalState::File {
        digest: Digest::of(REPORT.as_bytes()),
        held: Held::Whole(REPORT.to_owned()),
    };
    observed.finals = vec![FinalReceipt::new(target, state)];
    observed.ledger = LedgerFacts::clean(vec![target.into()]);
    observed.spent = Spent::new(TICKETS.len() as u64, REPORT.len() as u64);
    RehearsalReport::new(
        Rehearsal::Passed {
            outputs: vec![RehearsedOutput::new(target, REPORT)],
        },
        Attempt::Completed { elapsed_ms: 2 },
        EffectCounts::none(),
        sha256(candidate),
    )
    .with_admitted_digest("synthetic-admission")
    .with_room(RoomEvidence::new(true, true))
    .with_observation(observed)
}

/// A revision doubt only a run decides is settled by one trial of its exact bytes (the EDIT
/// trial seam): the session's saved tickets workflow, changed in words under the request it was
/// created from (whose stated paths the room's copy answers to), answered whole by the author's
/// exact draft; the judge doubts the whole request, carries every part and finds
/// nothing extra. With no room, nothing decides the doubt: held after one authoring call. That
/// rejection carried to an unchanged round asks the judge nothing. With a room, the revision's
/// exact bytes run once (the session's own world untouched: only the room's copy is read) and
/// the judge, shown that run, finds it consistent: READY on the same draft, with no second
/// authoring call, the carried round resuming with the one question over the run.
#[tokio::test]
async fn a_revision_doubt_is_settled_by_one_trial_of_its_exact_bytes() {
    let change = "Keep the tickets whose age_hours is strictly greater than 72 instead of 48.";
    let edit = crate::CompileRequest::edit(SAVED, change)
        .with_original_intent(STALE_INTENT)
        .with_authoring_policy(policy());
    let answer = json!({"supersedes": [], "adds": [], "notes": "", "operations": [],
        "replace": DRAFT})
    .to_string();
    let authored = |out: &crate::CompileOutcome| {
        calls(out)
            .iter()
            .filter(|c| c.starts_with("revision"))
            .count()
    };
    let author = Scripted::doubting(vec![answer.clone()]);
    let held = compiled_with(&edit, &author, None, None).await;
    assert_ne!(
        held.status,
        crate::CompileStatus::Ready,
        "{:#?}",
        held.diagnostics
    );
    assert_eq!(held.candidate.as_deref(), Some(DRAFT));
    assert_eq!(
        (
            authored(&held),
            author.asked("whole"),
            author.asked("observed")
        ),
        (1, 1, 0)
    );
    let decision = held.provenance.decision.clone().unwrap_or_default();
    let rejected = decision["semantic_verification"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let carried = edit.clone().with_declined(rejected);
    let author = Scripted::doubting(vec![answer.clone()]);
    let again = compiled_with(&carried, &author, None, None).await;
    assert_ne!(again.status, crate::CompileStatus::Ready);
    assert_eq!(
        (author.asked("whole"), author.asked("part")),
        (0, 0),
        "nothing asked again"
    );
    for (request, whole) in [(&edit, 1), (&carried, 0)] {
        let room = Room::default();
        let author = Scripted::doubting(vec![answer.clone()]);
        let out = compiled_with(request, &author, Some(&room), None).await;
        assert_eq!(
            out.status,
            crate::CompileStatus::Ready,
            "{:#?}",
            out.diagnostics
        );
        assert_eq!(out.candidate.as_deref(), Some(DRAFT));
        assert_eq!(authored(&out), 1, "no second authoring call");
        assert_eq!(
            (author.asked("whole"), author.asked("observed")),
            (whole, 1)
        );
        let shown = room.shown.lock().expect("shown").clone();
        assert!(
            shown.iter().all(|ran| ran == DRAFT) && !shown.is_empty(),
            "{shown:?}"
        );
    }
}
