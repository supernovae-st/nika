// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The revision door over a complete base no semantic record binds: one revision call states
//! operations, the compiler applies them (`nika_compile_seats::foundry::document`), finishes
//! the result and has the round's judge read it; a refused operation leaves the base kept.

use nika_compile::surface::sha256;
use nika_compile_seats::foundry::component::pinned;
use nika_compile_seats::foundry::document::carried;
use nika_compile_seats::foundry::{
    Component, ComponentCatalog, ComponentRef, Hole, Release, Unresolved,
};
use serde_json::{Value, json};

/// A rich base the pack ships: typed input with a default, failure and unwind edges, a `when`
/// guard, a timeout, comments and outputs.
fn rich() -> &'static str {
    nika_pack::example("12-failure-routing").expect("the pack ships 12-failure-routing")
}

/// The lines of `revised` that differ from `base`, as `(base line, revised line)`, when both
/// hold the same number of lines.
fn changed_lines(base: &str, revised: &str) -> Vec<(String, String)> {
    let (a, b): (Vec<&str>, Vec<&str>) = (base.lines().collect(), revised.lines().collect());
    assert_eq!(a.len(), b.len(), "a literal edit adds or removes no line");
    (a.iter().zip(&b))
        .filter(|(x, y)| x != y)
        .map(|(x, y)| ((*x).to_owned(), (*y).to_owned()))
        .collect()
}

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

/// The person's saved workflow a revision starts from: valid and checked, one task of its own.
const SAVED: &str = r#"nika: stale-tickets-report
# The person's own boundary: the files the request names.
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
  tools: ["nika:read", "nika:jq", "nika:write", "nika:log"]
tasks:
  announce:
    invoke: { tool: "nika:log", args: { level: info, message: "stale tickets report" } }
"#;

const VERSION: &str = "fixture-document-r1";
const SNAPSHOT: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn release() -> Release {
    Release::new(VERSION, SNAPSHOT, "nika-knowledge-release-profile/r1")
}

/// A catalogue of one release holding the stale-filter component.
struct Shelf;

impl ComponentCatalog for Shelf {
    fn release(&self) -> Release {
        release()
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        reference.block_name()?;
        pinned(reference, &self.release())?;
        if reference.id != "block:stale-filter-report" {
            return Err(Unresolved::Unknown(reference.id.clone()));
        }
        let mut component = Component::new(
            "block:stale-filter-report",
            release(),
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
    fn entries(&self) -> Vec<Value> {
        vec![
            json!({"id": "block:stale-filter-report", "title": "Stale records report",
            "purpose": "count the records older than a threshold and write a report",
            "holes": [{"name": "const.max_age_hours", "owner": "human"}]}),
        ]
    }
}

/// A seat answering each revision call with its next scripted answer (the last one repeats),
/// and every judge question favourably (the judge's own laws are tested elsewhere); every call
/// is counted by role, and every request it received is kept.
struct Seat {
    revisions: std::sync::Mutex<Vec<Value>>,
    roles: std::sync::Mutex<Vec<String>>,
    asked: std::sync::Mutex<Vec<String>>,
}

impl Seat {
    fn new(revision: Value) -> Self {
        Self::sequence(vec![revision])
    }

    fn sequence(revisions: Vec<Value>) -> Self {
        Self {
            revisions: std::sync::Mutex::new(revisions),
            roles: std::sync::Mutex::new(Vec::new()),
            asked: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The last message of each revision call, in order.
    fn told(&self) -> Vec<String> {
        self.asked.lock().expect("asked").clone()
    }

    fn roles(&self) -> Vec<String> {
        self.roles.lock().expect("roles").clone()
    }
}

impl nika_kernel::ai::provider::ProviderInferDyn for Seat {
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
        let answer = if schema["properties"]["operations"].is_object() {
            self.roles
                .lock()
                .expect("roles")
                .push("revision".to_owned());
            let last = (request.messages.last())
                .map(|message| format!("{message:?}"))
                .unwrap_or_default();
            self.asked.lock().expect("asked").push(last);
            let mut queue = self.revisions.lock().expect("revisions");
            let next = if queue.len() > 1 {
                queue.remove(0)
            } else {
                queue[0].clone()
            };
            next.to_string()
        } else {
            self.roles.lock().expect("roles").push("judge".to_owned());
            let keys = schema["properties"]["choice"]["enum"].to_string();
            let choice = if keys.contains("\"faithful\"") {
                "faithful"
            } else if keys.contains("\"only_requested\"") {
                "only_requested"
            } else if keys.contains("\"consistent\"") {
                "consistent"
            } else if keys.contains("\"no_task\"") {
                "omitted"
            } else {
                "carried"
            };
            json!({"choice": choice}).to_string()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text: answer }],
            TokenUsage::new(10, 5),
            StopReason::EndTurn,
        ))
    }
}

fn policy() -> crate::AuthoringPolicy {
    crate::AuthoringPolicy::new("mock/authoring", 4096, std::time::Duration::from_secs(2))
        .with_native(crate::NativeMode::Escalate)
}

/// The revision answer with no destination link: operations only, in the strict text form.
fn operations(operations: &[Value]) -> Value {
    json!({"supersedes": [], "adds": [], "notes": "", "operations": operations, "replace": ""})
}

async fn revised(
    base: &str,
    change: &str,
    plan: Option<Value>,
    seat: &Seat,
) -> crate::CompileOutcome {
    let mut request = crate::CompileRequest::edit(base, change).with_authoring_policy(policy());
    request.plan = plan;
    crate::compile_with_cognition_composed(
        &request,
        crate::Cognition {
            provider: Some(seat),
            seat: None,
        },
        None,
        Some(&Shelf),
    )
    .await
    .expect("compiles")
}

fn route(out: &crate::CompileOutcome) -> Vec<String> {
    (out.provenance.decision.as_ref())
        .and_then(|d| d["route"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|step| step.as_str().map(str::to_owned))
        .collect()
}

/// A rich hand-written base no record binds used to be kept as it is, no seat asked (the
/// change was no destination edit). It is now revised over the whole document: the seat states
/// two literal edits, the bytes outside them are the base's, the result is checked and judged,
/// and the record binds the revised bytes.
#[tokio::test]
async fn a_rich_record_less_base_is_revised_in_place_and_judged() {
    let base = rich();
    let seat = Seat::new(operations(&[
        json!({"op": "set", "path": "/inputs/drill/default", "value_json": "true",
            "component": "", "version": "", "bindings_json": ""}),
        json!({"op": "set", "path": "/tasks/release_lock/timeout", "value_json": "\"30s\"",
            "component": "", "version": "", "bindings_json": ""}),
    ]));
    let out = revised(
        base,
        "Rehearse the failure path by default and give the lock release thirty seconds",
        None,
        &seat,
    )
    .await;
    assert_eq!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.clone().expect("a candidate");
    assert_eq!(
        changed_lines(base, &candidate),
        [
            (
                "    default: false".to_owned(),
                "    default: true".to_owned()
            ),
            (
                "    timeout: \"10s\"".to_owned(),
                "    timeout: \"30s\"".to_owned()
            ),
        ]
    );
    assert!(
        route(&out)
            .iter()
            .any(|s| s == nika_compile_seats::foundry::document::ROUTE),
        "{:?}",
        route(&out)
    );
    let decision = out.provenance.decision.as_ref().expect("decision");
    assert_eq!(decision["document_revision"]["mode"], "operations");
    assert_eq!(
        decision["document_revision"]["changed"],
        json!(["inputs.drill.default", "tasks.release_lock.timeout"])
    );
    assert_eq!(decision["forensic"]["door"]["name"], "document_revision");
    let plan = out.provenance.plan.as_ref().expect("a record");
    assert!(nika_compile_fidelity::sketch::kept::binds(plan, &candidate));
    let roles = seat.roles();
    assert_eq!(roles.first().map(String::as_str), Some("revision"));
    assert_eq!(
        roles.iter().filter(|r| *r == "revision").count(),
        1,
        "one revision call: {roles:?}"
    );
    assert!(roles.iter().any(|r| r == "judge"), "judged: {roles:?}");
}

/// A component composed in one revision is rebound in the next, through the record the first
/// left: 48 hours, then 72, the receipt following the bytes, nothing else changed.
#[tokio::test]
async fn a_composed_component_is_rebound_in_the_next_revision() {
    let compose_48 = Seat::new(operations(&[json!({"op": "compose",
        "component": "block:stale-filter-report", "version": VERSION,
        "bindings_json": "{\"const.records_path\": \"./in/tickets.json\", \"const.report_path\": \"./out/report.json\", \"const.max_age_hours\": 48}",
        "path": "", "value_json": ""})]));
    let first = revised(
        SAVED,
        "Report the tickets older than 48 hours",
        None,
        &compose_48,
    )
    .await;
    assert_eq!(first.status, crate::CompileStatus::Ready, "{first:#?}");
    let candidate = first.candidate.clone().expect("composed");
    let reuse =
        &first.provenance.decision.as_ref().expect("decision")["knowledge_qualification"]["reuse"];
    assert_eq!(reuse["expanded"], 1, "{reuse}");

    let rebind_72 = Seat::new(operations(&[json!({"op": "rebind",
        "component": "block:stale-filter-report",
        "bindings_json": "{\"const.max_age_hours\": 72}",
        "path": "", "value_json": "", "version": ""})]));
    let second = revised(
        &candidate,
        "Use 72 hours instead",
        first.provenance.plan.clone(),
        &rebind_72,
    )
    .await;
    assert_eq!(second.status, crate::CompileStatus::Ready, "{second:#?}");
    let revised_source = second.candidate.clone().expect("rebound");
    assert_eq!(
        changed_lines(&candidate, &revised_source),
        [(
            "  max_age_hours: { type: integer, value: 48 }".to_owned(),
            "  max_age_hours: { type: integer, value: 72 }".to_owned()
        )]
    );
    let plan = second.provenance.plan.as_ref().expect("a record");
    let receipts = carried(Some(plan));
    let [receipt] = receipts.as_slice() else {
        panic!("one carried receipt: {plan}");
    };
    assert_eq!(receipt["revises"], sha256(&candidate));
    assert_eq!(receipt["candidate_sha256"], sha256(&revised_source));
    assert_eq!(
        nika_compile_fidelity::sketch::kept::original(plan),
        Some("Report the tickets older than 48 hours\nChange: Use 72 hours instead")
    );
}

/// An operation the document refuses leaves no candidate: the base is kept and every reason
/// is named; nothing is judged.
#[tokio::test]
async fn a_refused_revision_keeps_the_base_and_names_why() {
    let seat = Seat::new(operations(&[json!({"op": "set", "path": "/const/absent",
        "value_json": "1", "component": "", "version": "", "bindings_json": ""})]));
    let out = revised(rich(), "Set the absent constant to one", None, &seat).await;
    assert_ne!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        (out.diagnostics.iter())
            .any(|d| d.message.contains("The revision is not kept: operation 0")),
        "{out:#?}"
    );
    assert_eq!(
        seat.roles(),
        ["revision", "revision"],
        "told back once; the same refusal again brings nothing new, nothing judged"
    );
}

/// A refused operation is told back to the same seat, the base intact: its next statement holds
/// and only that statement's edit reaches the bytes, then the round's judge reads them.
#[tokio::test]
async fn a_refused_operation_is_told_back_and_the_restatement_is_kept() {
    let refused = json!({"op": "set", "path": "/const/absent", "value_json": "1",
        "component": "", "version": "", "bindings_json": ""});
    let valid = json!({"op": "set", "path": "/inputs/drill/default", "value_json": "true",
        "component": "", "version": "", "bindings_json": ""});
    let seat = Seat::sequence(vec![operations(&[refused]), operations(&[valid])]);
    let out = revised(rich(), "Rehearse the failure path by default", None, &seat).await;
    assert_eq!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.clone().expect("a candidate");
    assert_eq!(
        changed_lines(rich(), &candidate),
        [(
            "    default: false".to_owned(),
            "    default: true".to_owned()
        )]
    );
    let roles = seat.roles();
    assert_eq!(roles[..2], ["revision", "revision"], "{roles:?}");
    assert!(roles[2..].iter().all(|role| role == "judge"), "{roles:?}");
    let told = seat.told();
    assert!(
        told[1].contains("operation 0") && told[1].contains("the base is unchanged"),
        "the refusal itself is told back: {}",
        told[1]
    );
    // The refused first attempt was paid and stays in the receipt beside the one kept.
    let receipt = out
        .provenance
        .authoring
        .as_ref()
        .expect("the authoring receipt");
    let revisions = (receipt.context.iter())
        .filter(|call| {
            call["call"]
                .as_str()
                .is_some_and(|r| r.starts_with("revision"))
        })
        .count();
    assert_eq!(revisions, 2, "{:?}", receipt.context);
    assert!(receipt.calls >= 2, "{receipt:?}");
}

/// Destination links stated where no destination edit applies are told back, never guessed into
/// operations; the operations stated next are kept.
#[tokio::test]
async fn links_where_none_apply_are_told_back_and_operations_are_kept() {
    let links = json!({"supersedes": [{"replaces": "a", "by": "b"}], "adds": [], "notes": "",
        "operations": [], "replace": ""});
    let valid = json!({"op": "set", "path": "/tasks/release_lock/timeout",
        "value_json": "\"30s\"", "component": "", "version": "", "bindings_json": ""});
    let seat = Seat::sequence(vec![links, operations(&[valid])]);
    let out = revised(rich(), "Give the lock release thirty seconds", None, &seat).await;
    assert_eq!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    assert!(
        seat.told()[1].contains("destination links where no destination edit applies"),
        "{:?}",
        seat.told()
    );
}

/// A result Check refuses is told back in Check's own words, the base intact: a write to a path
/// the base does not grant is refused; the next statement also grants it and is kept.
#[tokio::test]
async fn a_result_check_refuses_is_told_back_with_its_words() {
    let base = r#"nika: one-write
permits:
  fs: { write: ["./out/a.json"] }
  tools: ["nika:write"]
tasks:
  save:
    invoke: { tool: "nika:write", args: { path: "./out/a.json", content: "{}" } }
"#;
    let moved = json!({"op": "set", "path": "/tasks/save/invoke/args/path",
        "value_json": "\"./out/b.json\"", "component": "", "version": "", "bindings_json": ""});
    let granted = json!({"op": "push", "path": "/permits/fs/write",
        "value_json": "\"./out/b.json\"", "component": "", "version": "", "bindings_json": ""});
    let seat = Seat::sequence(vec![
        operations(std::slice::from_ref(&moved)),
        operations(&[moved, granted]),
    ]);
    let out = revised(base, "Save to ./out/b.json instead", None, &seat).await;
    assert_eq!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.clone().expect("a candidate");
    // READY means Check found the write granted: the path is both written and permitted.
    assert_eq!(candidate.matches("./out/b.json").count(), 2, "{candidate}");
    assert!(
        candidate.contains("./out/a.json\""),
        "the earlier grant is kept: {candidate}"
    );
    let told = seat.told();
    assert!(
        told[1].contains("Check refuses the candidate")
            && told[1].contains("the base is unchanged"),
        "Check's refusal is told back: {}",
        told[1]
    );
}

/// A seat cycling between two refused statements (A, B, A) brings nothing new on the third and
/// the talk ends there, with no repair bound set: every refusal told back is remembered.
#[tokio::test]
async fn refusals_cycling_between_two_statements_end_without_a_bound() {
    let refused = |path: &str| {
        operations(&[json!({"op": "set", "path": path, "value_json": "1",
            "component": "", "version": "", "bindings_json": ""})])
    };
    let (a, b) = (refused("/const/absent"), refused("/const/missing"));
    let seat = Seat::sequence(vec![a.clone(), b.clone(), a, b.clone(), b]);
    assert!(policy().repairs.is_none(), "no repair bound in this policy");
    let out = revised(rich(), "Set the constant to one", None, &seat).await;
    assert_ne!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none());
    assert_eq!(
        seat.roles(),
        ["revision", "revision", "revision"],
        "A, B, then A again: nothing new"
    );
}

/// A created document's settled record is the history of its bytes, never an answer round of
/// its creation: a change to them reaches the revision with the request they answer, the result
/// is bound to the revised bytes, and that request is still the one the new record states.
#[tokio::test]
async fn a_change_to_a_created_documents_bytes_is_revised_with_its_request_known() {
    let base = rich();
    let created = "Rehearse the deployment and release the lock afterwards";
    let record = json!({"strategy": "native",
        "intent_sha256": nika_compile::intent_sha256(created), "source": base,
        "questions": [], "gaps": [], "trigger": null,
        "document_create": {"mode": "written", "request": created},
        "document": {"version": 1, "candidate_sha256": nika_compile::surface::sha256(base),
            "request": created, "base_sha256": null, "mode": "written", "components": []}});
    let seat = Seat::new(operations(&[json!({"op": "set",
        "path": "/inputs/drill/default", "value_json": "true",
        "component": "", "version": "", "bindings_json": ""})]));
    let change = "Rehearse the failure path by default";
    let out = revised(base, change, Some(record), &seat).await;
    assert_eq!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    assert!(
        !out.diagnostics.iter().any(|d| d.target == "recorded_plan"),
        "{:?}",
        out.diagnostics
    );
    let candidate = out.candidate.clone().expect("a candidate");
    assert_eq!(
        changed_lines(base, &candidate),
        [(
            "    default: false".to_owned(),
            "    default: true".to_owned()
        )]
    );
    assert!(seat.told()[0].contains(created), "{}", seat.told()[0]);
    let plan = out.provenance.plan.as_ref().expect("a record");
    assert!(nika_compile_fidelity::sketch::kept::binds(plan, &candidate));
    let original = nika_compile_fidelity::sketch::kept::original(plan).unwrap_or_default();
    assert!(
        original.contains(created) && original.contains(change),
        "{original}"
    );
}
