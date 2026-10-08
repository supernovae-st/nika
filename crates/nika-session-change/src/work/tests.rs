// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::{Path, PathBuf};

use nika_onboard::compile::{
    AuthoringReceipt, CompileRequest, CompileStatus, DiagnosticKind, compile,
};

use super::{
    AnswerAct, Answered, Audit, Author, Authoring, AuthoringStatus, CONTRACT, Candidate,
    DecisionSeat, Intelligence, Landing, NoteKind, Rail, Request, RequestedRun, Run, RunEnd, Saved,
    Selected, Stage, ValueSource, Waiting, Work,
};
use crate::change::{
    ProjectChange, ProjectChangeSet, RunRequest, Witness, WorkflowAudit, check_on_disk,
};
use crate::outcome::{GateId, Incarnation, ProposalId, QuestionId, RefusalClass};
use crate::world::{Basis, Reach, World};

const SINK: &str = r#"nika: stock-sink

permits:
  tools: ["nika:fetch"]
  net:
    http: ["127.0.0.1"]

tasks:
  notify:
    invoke:
      tool: "nika:fetch"
      args:
        url: "http://127.0.0.1:8787/notifications/stock"
        method: POST
        headers:
          idempotency-key: "stock-1"
        body: { channel: "stock", item_ids: ["v-101"] }
"#;

fn set_with(
    root: &Path,
    changes: Vec<ProjectChange>,
    audits: Vec<WorkflowAudit>,
) -> ProjectChangeSet {
    ProjectChangeSet {
        root: root.to_path_buf(),
        goal: "alert on low stock".to_owned(),
        changes,
        run: Some(RunRequest {
            workflow: PathBuf::from("stock.nika"),
            vars: Vec::new(),
            max_cost_usd: 1.0,
            access_pin: None,
            bytes: None,
            closure: None,
        }),
        repairs: Vec::new(),
        audits,
    }
}

#[test]
fn every_run_door_exit_has_one_end_and_its_meaning() {
    let ends: Vec<(u8, RunEnd)> = [0, 1, 2, 3, 4, 130, 7]
        .into_iter()
        .map(|exit| (exit, RunEnd::of(exit)))
        .collect();
    assert_eq!(
        ends,
        [
            (0, RunEnd::Succeeded),
            (1, RunEnd::Failed),
            (2, RunEnd::RefusedFindings),
            (3, RunEnd::RefusedEnvironment),
            (4, RunEnd::Paused),
            (130, RunEnd::Interrupted),
            (7, RunEnd::Unknown(7)),
        ]
    );
    assert_eq!(RunEnd::of(0).meaning(), "succeeded");
    assert!(RunEnd::of(4).meaning().contains("--resume <trace>"));
    assert_eq!(RunEnd::of(9).meaning(), "ended with an unknown code");
}

#[test]
fn a_candidate_names_each_landing_its_bytes_and_where_a_workflow_reaches() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("stock.nika"), SINK).expect("workflow");
    let audit = check_on_disk(root.path(), Path::new("stock.nika"));
    let base = Witness::of(b"previous bytes");
    let set = set_with(
        root.path(),
        vec![
            ProjectChange::UpdateWorkflow {
                path: PathBuf::from("stock.nika"),
                before: base.clone(),
                content: SINK.to_owned(),
            },
            ProjectChange::CreateSupportingFile {
                path: PathBuf::from("notes.md"),
                content: "threshold notes".to_owned(),
            },
        ],
        vec![audit],
    );
    let proposal = ProposalId::of("the exact preview");
    let candidate = Candidate::of(proposal.clone(), &set, false, true);

    assert_eq!(candidate.proposal, proposal);
    assert!(candidate.rehearsed && !candidate.aside && candidate.run_after_save);
    let [workflow, notes] = candidate.files.as_slice() else {
        panic!("two files: {:?}", candidate.files);
    };
    assert_eq!(workflow.landing, Landing::Update);
    assert!(workflow.workflow);
    assert_eq!(workflow.bytes, Witness::of(SINK.as_bytes()));
    assert_eq!(workflow.content, SINK, "the exact bytes, not a rendering");
    assert_eq!(Witness::of(workflow.content.as_bytes()), workflow.bytes);
    assert_eq!(workflow.replaces.as_ref(), Some(&base));
    let audit = workflow.audit.as_ref().expect("the workflow's audit");
    assert_eq!(audit.world.basis, Basis::Declared);
    assert_eq!(audit.world.reach, Reach::LocalServices);

    assert_eq!(notes.landing, Landing::Create);
    assert!(!notes.workflow);
    assert_eq!(notes.content, "threshold notes");
    assert_eq!(notes.replaces, None);
    assert_eq!(notes.audit, None, "only workflows are audited");
    assert_eq!(candidate.worlds().count(), 1);
}

#[test]
fn a_kept_run_is_not_the_current_result_and_names_only_what_was_observed() {
    let run = Run::new(
        false,
        Some("stock.nika".to_owned()),
        Some(4),
        Some(".nika/traces/a.ndjson".to_owned()),
        None,
        Some("ab12".to_owned()),
        None,
        None,
    );
    assert!(!run.current);
    assert_eq!(run.end, Some(RunEnd::Paused));
    let json = serde_json::to_value(&run).expect("serializes");
    assert_eq!(json["end"], serde_json::json!({"end": "paused"}));
    assert_eq!(json["execution"], serde_json::Value::Null);
    let unknown = serde_json::to_value(Run::new(true, None, Some(9), None, None, None, None, None))
        .expect("serializes");
    assert_eq!(
        unknown["end"],
        serde_json::json!({"end": "unknown", "exit": 9})
    );
}

#[test]
fn waiting_states_carry_the_identity_an_answer_names() {
    let consent = Waiting::Consent {
        proposal: ProposalId::of("preview"),
    };
    let json = serde_json::to_value(&consent).expect("serializes");
    assert_eq!(json["kind"], "consent");
    assert_eq!(
        json["proposal"],
        serde_json::Value::String(ProposalId::of("preview").as_str().to_owned()),
        "the full digest travels, not the short display"
    );
    let gate = Waiting::Gate {
        gate: GateId::new(Path::new(".nika/traces/t.ndjson"), "approve"),
    };
    assert_eq!(
        serde_json::to_value(&gate).expect("serializes"),
        serde_json::json!({"kind": "gate", "gate": {"trace": ".nika/traces/t.ndjson", "task": "approve"}})
    );
    assert_eq!(
        serde_json::to_value(Waiting::Free).expect("serializes"),
        serde_json::json!({"kind": "free"})
    );
}

/// What a line did to the question it was typed for travels as that question's witness, the act's
/// word and the act's own fields, nothing else; a snapshot where no line answered carries no key.
#[test]
fn an_answer_act_travels_with_its_question_and_only_its_own_fields() {
    let asker = std::sync::Arc::new(Incarnation);
    let witness = "0f".repeat(32);
    let question = QuestionId::new(witness.clone(), &asker);
    let wire =
        |act| serde_json::to_value(Answered::new(question.clone(), act)).expect("serializes");
    let key = || "const.destination_path".to_owned();
    let bound = |reading| AnswerAct::Bound {
        key: key(),
        value: "sortie.txt".to_owned(),
        reading,
    };
    for (reading, word) in [
        (ValueSource::AsTyped, "as_typed"),
        (ValueSource::OfferedKey, "offered_key"),
        (ValueSource::ModelRead, "model_read"),
        (ValueSource::SeatDefault, "seat_default"),
    ] {
        assert_eq!(
            wire(bound(reading)),
            serde_json::json!({"question": witness, "act": "bound",
                "key": "const.destination_path", "value": "sortie.txt", "reading": word})
        );
    }
    assert_eq!(
        wire(AnswerAct::Dropped { key: key() }),
        serde_json::json!({"question": witness, "act": "dropped", "key": "const.destination_path"})
    );
    assert_eq!(
        wire(AnswerAct::Restated { key: key() }),
        serde_json::json!({"question": witness, "act": "restated", "key": "const.destination_path"})
    );
    let waits = AnswerAct::Waits {
        key: key(),
        why: "the reply names no value".to_owned(),
    };
    assert_eq!(
        wire(waits),
        serde_json::json!({"question": witness, "act": "waits",
            "key": "const.destination_path", "why": "the reply names no value"})
    );
    let refused = AnswerAct::Refused {
        class: RefusalClass::StaleRevision,
    };
    assert_eq!(
        wire(refused),
        serde_json::json!({"question": witness, "act": "refused", "class": "stale_revision"})
    );
    let rail = Rail {
        draft: Stage::Pending,
        saved: Stage::Pending,
        checked: Stage::Pending,
        active: Stage::Pending,
        run: Stage::Pending,
    };
    let request = Request::new(None, Vec::new(), Vec::new());
    let work = Work::new(
        PathBuf::from("/project"),
        request,
        Waiting::Free,
        None,
        None,
        None,
        None,
        rail,
    );
    let quiet = serde_json::to_value(&work).expect("serializes");
    assert!(quiet.get("answered").is_none(), "{quiet}");
    let restated = Answered::new(question.clone(), AnswerAct::Restated { key: key() });
    let told = serde_json::to_value(work.with_answered(Some(restated))).expect("serializes");
    assert_eq!(told["answered"]["act"], "restated");
    assert_eq!(told["answered"]["question"], serde_json::json!(witness));
}

#[test]
fn a_snapshot_carries_its_contract_and_every_part() {
    let rail = Rail {
        draft: Stage::Done,
        saved: Stage::Done,
        checked: Stage::Attention,
        active: Stage::Pending,
        run: Stage::Failed,
    };
    let work = Work::new(
        PathBuf::from("/project"),
        Request::new(
            Some("alert on low stock".to_owned()),
            vec!["threshold 6".to_owned()],
            Vec::new(),
        ),
        Waiting::Free,
        None,
        Some(Saved::new(PathBuf::from("stock.nika"), Some(false), None)),
        Some(RequestedRun::new(
            PathBuf::from("stock.nika"),
            &["base=http://127.0.0.1:8787".to_owned()],
            World::default(),
        )),
        Some(Run::new(true, None, Some(1), None, None, None, None, None)),
        rail,
    );
    let json = serde_json::to_value(&work).expect("serializes");
    assert_eq!(json["contract"], CONTRACT);
    assert_eq!(
        json["request"]["decisions"],
        serde_json::json!(["threshold 6"])
    );
    assert_eq!(json["saved"]["check_clean"], false);
    assert_eq!(json["saved"]["world"], serde_json::Value::Null);
    assert_eq!(json["requested"]["inputs"], serde_json::json!(["base"]));
    assert_eq!(json["requested"]["world"]["basis"], "not_audited");
    assert_eq!(json["run"]["end"], serde_json::json!({"end": "failed"}));
    assert_eq!(json["rail"]["checked"], "attention");
    assert_eq!(json["candidate"], serde_json::Value::Null);
}

#[test]
fn every_lifecycle_stage_has_a_contract_stage() {
    use nika_onboard::lifecycle::{Lifecycle, LifecycleFacts, RunFact};
    let mut facts = LifecycleFacts::new();
    facts.saved = true;
    facts.check_clean = Some(false);
    facts.declared_active = Some(false);
    facts.run = RunFact::GateWaits;
    let rail = Rail::from(&Lifecycle::from_facts(&facts));
    assert_eq!(
        rail,
        Rail {
            draft: Stage::Done,
            saved: Stage::Done,
            checked: Stage::Attention,
            active: Stage::Paused,
            run: Stage::Paused,
        }
    );
}

#[test]
fn an_unaudited_workflow_keeps_no_reach() {
    let audit = Audit::of(&WorkflowAudit {
        path: PathBuf::from("x.nika"),
        clean: false,
        findings: vec!["NIKA-PARSE · nope".to_owned()],
        hints: Vec::new(),
        effects: Vec::new(),
        world: World::default(),
        bytes: None,
        closure: None,
    });
    assert!(!audit.clean);
    assert_eq!(audit.world.basis, Basis::NotAudited);
}

#[test]
fn a_requested_run_keeps_input_names_and_never_their_values() {
    let requested = RequestedRun::new(
        PathBuf::from("stock.nika"),
        &[
            "base=http://127.0.0.1:8787".to_owned(),
            "token=secret-value".to_owned(),
            "flag".to_owned(),
        ],
        World::default(),
    );
    assert_eq!(requested.inputs, ["base", "token", "flag"]);
    let json = serde_json::to_string(&requested).expect("serializes");
    assert!(
        !json.contains("secret-value") && !json.contains("8787"),
        "values stay with the run request: {json}"
    );
}

#[test]
fn a_document_revision_is_read_from_its_record_with_each_component_witnessed() {
    use super::DocumentRevision;
    let record = serde_json::json!({
        "mode": "operations",
        "base_sha256": "base",
        "candidate_sha256": "candidate",
        "changed": ["const.window_hours", "component block:stale-filter-report"],
        "preservation": "by construction",
        "components": [{
            "component": {"id": "block:stale-filter-report",
                "release": {"version": "r1", "snapshot_sha256": "snapshot"},
                "file_sha256": "file"},
            "bindings": [{"path": "const.max_age_hours", "bound": 72}],
        }],
    });
    let revision = DocumentRevision::of(&record, &["expanded".to_owned()]).expect("a revision");
    assert_eq!(revision.mode, "operations");
    assert_eq!(revision.candidate_sha256, "candidate");
    assert_eq!(
        revision.changed,
        ["const.window_hours", "component block:stale-filter-report"]
    );
    let [component] = revision.components.as_slice() else {
        panic!("one component: {revision:?}");
    };
    assert_eq!(component.id, "block:stale-filter-report");
    assert_eq!(
        (component.version.as_deref(), component.release.as_deref()),
        (Some("r1"), Some("snapshot"))
    );
    assert_eq!(component.bindings[0].path, "const.max_age_hours");
    assert_eq!(component.bindings[0].value, serde_json::json!(72));
    assert_eq!(component.witness, "expanded");
    // A component no witness was given for is said unwitnessed, never assumed expanded.
    let unwitnessed = DocumentRevision::of(&record, &[]).expect("a revision");
    assert_eq!(unwitnessed.components[0].witness, "unwitnessed");
    // A record that names no mode or no candidate is no revision.
    assert!(DocumentRevision::of(&serde_json::json!({"mode": "operations"}), &[]).is_none());
    assert!(DocumentRevision::of(&serde_json::json!({}), &[]).is_none());
}

#[test]
fn the_compilers_last_word_keeps_its_status_questions_notes_and_candidate_bytes() {
    let ready = compile(&CompileRequest::create(
        "Read ./notes/brief.md and write it to ./out/copy.md",
    ))
    .expect("compiles");
    let source = ready.candidate.as_deref().expect("a Ready candidate");
    let seen = Authoring::of(&ready);
    assert_eq!(seen.status, AuthoringStatus::Ready);
    assert!(seen.questions.is_empty(), "{:?}", seen.questions);
    assert_eq!(seen.candidate, Some(Witness::of(source.as_bytes())));
    assert_eq!(
        seen.draft.as_deref(),
        Some(source),
        "the draft is those very bytes"
    );
    assert_eq!(seen.calls, None, "the deterministic reading made no call");
    assert_eq!(seen.diagnostics.len(), ready.diagnostics.len());
    for (note, diagnostic) in seen.diagnostics.iter().zip(&ready.diagnostics) {
        assert_eq!(note.kind, NoteKind::from(diagnostic.kind));
        assert_eq!(
            (&note.target, &note.message),
            (&diagnostic.target, &diagnostic.message)
        );
    }

    let asking = compile(&CompileRequest::create("aggregate-by-key")).expect("compiles");
    let seen = Authoring::of(&asking);
    assert_eq!(seen.status, AuthoringStatus::Incomplete);
    let keys: Vec<&str> = asking.questions.iter().map(|q| q.key.as_str()).collect();
    assert!(!keys.is_empty(), "the skeleton asks its values");
    assert_eq!(
        seen.questions, keys,
        "every question, by key, in the compiler's order"
    );
    assert_eq!(
        seen.draft, asking.candidate,
        "a waiting question keeps the draft it is asked about, exact or absent"
    );
}

#[test]
fn the_calls_receipt_is_carried_as_reported_and_unknown_usage_stays_unknown() {
    let mut outcome = compile(&CompileRequest::create(
        "Read ./notes/brief.md and write it to ./out/copy.md",
    ))
    .expect("compiles");
    let mut receipt = AuthoringReceipt::new("acme/author-model");
    receipt.calls = 3;
    receipt.output_tokens = Some(812);
    receipt.elapsed_ms = 4_200;
    receipt.backend = Some(serde_json::json!({
        "transport": "acp_harness", "adapter": "claude-code", "observed_model": "author-model-served"
    }));
    outcome.provenance.authoring = Some(receipt);
    let calls = Authoring::of(&outcome)
        .calls
        .expect("the receipt rides the snapshot");
    assert_eq!(calls.requested_model, "acme/author-model");
    assert_eq!((calls.calls, calls.elapsed_ms), (3, 4_200));
    assert_eq!((calls.input_tokens, calls.output_tokens), (None, Some(812)));
    let json = serde_json::to_value(&calls).expect("serializes");
    assert_eq!(
        json["input_tokens"],
        serde_json::Value::Null,
        "unknown is never zero"
    );
    assert_eq!(json["backend"]["observed_model"], "author-model-served");
}

/// A receipt of three recorded calls: an answered `document` call carrying everything a context
/// entry may hold (its proposed object, the served model name, references by id), a
/// `document-repair` that timed out with a malformed digest and free text where an identifier
/// belongs, and a `revision` cut at its output limit with unreported usage.
fn three_recorded_calls(instruction: &str, schema: &str) -> AuthoringReceipt {
    let mut receipt = AuthoringReceipt::new("acme/author-model");
    receipt.calls = 2;
    receipt.elapsed_ms = 900;
    receipt.input_tokens = Some(1_000);
    receipt.context = vec![
        serde_json::json!({
            "call": "document", "instruction_sha256": instruction, "schema_sha256": schema,
            "message_bytes": 4_096, "references": [{"id": "block:x"}, {"id": "skill:y"}],
            "max_output_tokens": 16_384, "timeout_ms": 600_000, "elapsed_ms": 700,
            "result": {"stop_reason": "EndTurn", "usage_reported": true,
                       "input_tokens": 1_000, "output_tokens": 250},
            "reasoning": {"configured": "high", "transmitted": "unobserved", "served": "unknown",
                          "reasoning_tokens": null, "response_model": "served-name-x"},
            "response": {"sha256": "b".repeat(64), "bytes": 812, "blocks": 1},
            "proposed": {"decoded": true, "object": {"tasks": "SECRET-PLAN-TEXT"}},
        }),
        serde_json::json!({
            "call": "document-repair", "instruction_sha256": "NOT A DIGEST", "schema_sha256": schema,
            "message_bytes": 512, "max_output_tokens": 4_096, "timeout_ms": 600_000,
            "elapsed_ms": 200, "result": {"failure_kind": "timeout"},
            "reasoning": {"configured": "high; drop table", "reasoning_tokens": null},
            "error": "provider said: SECRET-ERROR-TEXT",
        }),
        serde_json::json!({
            "call": "revision", "message_bytes": 128,
            "result": {"stop_reason": "MaxTokens", "usage_reported": false,
                       "input_tokens": null, "output_tokens": null},
        }),
    ];
    receipt
}

/// Each call's receipt passes through the allowlist: exact recorded facts, unknown usage kept
/// unknown, a failed call's engine kind, and nothing of its prompt, answer, proposed object,
/// served model name or error text, even when the receipt holds them.
#[test]
fn each_call_is_projected_through_the_allowlist_and_never_its_text() {
    let mut outcome = compile(&CompileRequest::create(
        "Read ./notes/brief.md and write it to ./out/copy.md",
    ))
    .expect("compiles");
    let instruction = "a".repeat(64);
    let schema = "0123456789abcdef".repeat(4);
    let receipt = three_recorded_calls(&instruction, &schema);
    outcome.provenance.authoring = Some(receipt);
    let calls = Authoring::of(&outcome).calls.expect("the receipt rides");
    let [answered, failed, revised] = calls.per_call.as_slice() else {
        panic!("one projection per recorded call: {:?}", calls.per_call);
    };
    assert_eq!(answered.call.as_deref(), Some("document"));
    assert_eq!(
        answered.instruction_sha256.as_deref(),
        Some(instruction.as_str())
    );
    assert_eq!(answered.schema_sha256.as_deref(), Some(schema.as_str()));
    assert_eq!(
        (answered.message_bytes, answered.references),
        (Some(4_096), Some(2))
    );
    assert_eq!(
        (
            answered.max_output_tokens,
            answered.timeout_ms,
            answered.elapsed_ms
        ),
        (Some(16_384), Some(600_000), Some(700))
    );
    assert_eq!(answered.stop_reason.as_deref(), Some("EndTurn"));
    assert_eq!(answered.failure_kind, None);
    assert_eq!(answered.usage_reported, Some(true));
    assert_eq!(
        (answered.input_tokens, answered.output_tokens),
        (Some(1_000), Some(250))
    );
    assert_eq!(answered.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(answered.reasoning_tokens, None, "unreported, never zero");

    assert_eq!(
        failed.call.as_deref(),
        Some("document-repair"),
        "the real role, whole"
    );
    assert_eq!(
        failed.instruction_sha256, None,
        "a malformed digest is not projected"
    );
    assert_eq!(failed.references, None, "the receipt recorded none");
    assert_eq!(failed.failure_kind.as_deref(), Some("timeout"));
    assert_eq!(failed.stop_reason, None);
    assert_eq!(failed.usage_reported, None, "the receipt does not say");
    assert_eq!((failed.input_tokens, failed.output_tokens), (None, None));
    assert_eq!(failed.reasoning_effort, None, "only an identifier passes");

    assert_eq!(revised.call.as_deref(), Some("revision"));
    assert_eq!(revised.stop_reason.as_deref(), Some("MaxTokens"));
    assert_eq!(revised.usage_reported, Some(false));
    assert_eq!(
        (revised.input_tokens, revised.output_tokens),
        (None, None),
        "unreported usage stays unknown"
    );

    // The totals stay the receipt's own: nothing is summed from the calls.
    assert_eq!((calls.calls, calls.elapsed_ms), (2, 900));
    assert_eq!(
        (calls.input_tokens, calls.output_tokens),
        (Some(1_000), None)
    );
    let json = serde_json::to_string(&calls).expect("serializes");
    for secret in [
        "SECRET-PLAN-TEXT",
        "SECRET-ERROR-TEXT",
        "served-name-x",
        "block:x",
        "drop table",
    ] {
        assert!(!json.contains(secret), "{secret} leaked: {json}");
    }
    let wire = serde_json::to_value(&calls).expect("serializes");
    assert_eq!(wire["per_call"][1]["input_tokens"], serde_json::Value::Null);
    assert_eq!(wire["per_call"][0]["references"], 2);
}

/// A selection is the operator's kept default until this conversation names its own.
#[test]
fn a_selection_names_whose_choice_it_is() {
    let kept = Selected::new("api", Some("deepseek".into()), None);
    assert_eq!(
        serde_json::to_value(&kept).expect("json")["scope"],
        "operator_default"
    );
    let own = kept.for_conversation();
    assert_eq!(
        serde_json::to_value(&own).expect("json")["scope"],
        "conversation"
    );
}

#[test]
fn the_intelligence_is_a_selection_and_says_so_on_the_wire() {
    let selected = Selected::new("harness", Some("claude-code".into()), Some("acp".into()))
        .resolved(None, "through your Claude account".into(), None, true);
    let author = Author::new("harness", None, None).through("claude-code".into(), "acp".into());
    let decision = DecisionSeat::new("typesafe/jev-1.13.0".into(), Some("no key".into()));
    let intelligence =
        Intelligence::new(Some(selected), author, Some(decision), Some("max".into()));
    let work = Work::new(
        PathBuf::from("/project"),
        Request::default(),
        Waiting::Free,
        None,
        None,
        None,
        None,
        Rail {
            draft: Stage::Pending,
            saved: Stage::Pending,
            checked: Stage::Pending,
            active: Stage::Pending,
            run: Stage::Pending,
        },
    );
    let bare = serde_json::to_value(&work).expect("serializes");
    assert_eq!(bare["intelligence"], serde_json::Value::Null);
    let json =
        serde_json::to_value(work.with_intelligence(Some(intelligence))).expect("serializes");
    assert_eq!(
        json["intelligence"],
        serde_json::json!({
            "selected": {
                "kind": "harness", "via": "claude-code", "transport": "acp", "model": null,
                "locus": "through your Claude account", "ready": true, "refusal": null,
                "scope": "operator_default"
            },
            "author": {
                "kind": "harness", "model": null, "seat": "claude-code", "transport": "acp",
                "why": null
            },
            "decision": {"model": "typesafe/jev-1.13.0", "refusal": "no key"},
            "effort": "max"
        })
    );
}

#[test]
fn every_compiler_status_and_note_kind_has_one_name_on_the_wire() {
    let statuses = [
        (CompileStatus::Ready, AuthoringStatus::Ready, "ready"),
        (
            CompileStatus::Incomplete,
            AuthoringStatus::Incomplete,
            "incomplete",
        ),
        (CompileStatus::Refused, AuthoringStatus::Refused, "refused"),
    ];
    for (status, named, wire) in statuses {
        assert_eq!(AuthoringStatus::from(status), named);
        assert_eq!(serde_json::to_value(named).expect("serializes"), wire);
    }
    let kinds = [
        (DiagnosticKind::Applied, NoteKind::Applied, "applied"),
        (DiagnosticKind::Missed, NoteKind::Missed, "missed"),
        (DiagnosticKind::Unknown, NoteKind::Unknown, "unknown"),
        (
            DiagnosticKind::RequiresHuman,
            NoteKind::RequiresHuman,
            "requires_human",
        ),
        (DiagnosticKind::Refused, NoteKind::Refused, "refused"),
    ];
    for (kind, named, wire) in kinds {
        assert_eq!(NoteKind::from(kind), named);
        assert_eq!(serde_json::to_value(named).expect("serializes"), wire);
    }
}

#[test]
fn only_spending_decisions_require_a_line_typed_after_they_were_shown() {
    use crate::outcome::ReviewId;
    let review = Waiting::RunReview {
        review: ReviewId::new(1, "Fresh Run cost decision", "challenge n-1"),
    };
    assert!(review.requires_fresh_input());
    assert!(Waiting::CostChoice.requires_fresh_input());
    for other in [
        Waiting::Free,
        Waiting::IntelligenceChoice,
        Waiting::Input {
            name: "base".to_owned(),
        },
        Waiting::Activation {
            key: "project.timezone".to_owned(),
        },
    ] {
        assert!(!other.requires_fresh_input(), "{other:?}");
    }
}

/// The waiting question travels as the compiler asks it: its own question document, field for
/// field (a choice's options in the compiler's order), beside the unchanged identity an answer
/// names; a question that does not take the next line is never shown as the one that does.
#[test]
fn the_waiting_question_travels_as_the_compiler_asks_it() {
    use std::sync::Arc;

    use nika_onboard::compile::{ChoiceOffer, CompileQuestion, QuestionType, outcome_document};

    use crate::outcome::{Incarnation, QuestionId};

    let asking = compile(&CompileRequest::create("aggregate-by-key")).expect("compiles");
    let mut question = asking
        .questions
        .first()
        .cloned()
        .expect("the skeleton asks");
    let asker = Arc::new(Incarnation);
    let on = |key: &str| Waiting::Question {
        key: key.to_owned(),
        id: QuestionId::new("ab".repeat(32), &asker),
    };
    let rail = Rail {
        draft: Stage::Pending,
        saved: Stage::Pending,
        checked: Stage::Pending,
        active: Stage::Pending,
        run: Stage::Pending,
    };
    let work = |waiting: Waiting, question: Option<&CompileQuestion>| {
        let request = Request::default();
        let snapshot = Work::new(
            PathBuf::from("/p"),
            request,
            waiting,
            None,
            None,
            None,
            None,
            rail,
        );
        serde_json::to_value(snapshot.with_question(question)).expect("serializes")
    };
    let as_compiled = |question: &CompileQuestion| {
        let mut outcome = asking.clone();
        outcome.questions = vec![question.clone()];
        outcome_document(&outcome)["questions"][0].clone()
    };

    question.key = "const.mode".to_owned();
    question.label = "Which mode?".to_owned();
    question.answer_type = QuestionType::Choice;
    question.why = "The request names two modes.".to_owned();
    question.mandatory = false;
    question.options = vec![
        ChoiceOffer::new("z-last", "Last"),
        ChoiceOffer::new("a-first", "First"),
    ];
    let json = work(on("const.mode"), Some(&question));
    assert_eq!(
        json["question"],
        serde_json::json!({
            "key": "const.mode", "label": "Which mode?", "type": "choice",
            "why": "The request names two modes.", "mandatory": false,
            "options": [{"key": "z-last", "label": "Last"}, {"key": "a-first", "label": "First"}],
        })
    );
    assert_eq!(json["question"], as_compiled(&question));
    assert_eq!(
        json["waiting"],
        serde_json::json!({"kind": "question", "key": "const.mode", "id": "ab".repeat(32)}),
        "the identity an answer names is unchanged"
    );

    question.options.clear();
    question.mandatory = true;
    for (shape, spelled) in [
        (QuestionType::Text, "text"),
        (QuestionType::Literal, "literal"),
    ] {
        question.answer_type = shape;
        let json = work(on("const.mode"), Some(&question));
        assert_eq!(json["question"]["type"], spelled);
        assert_eq!(json["question"]["mandatory"], true);
        assert!(json["question"].get("options").is_none(), "{json}");
        assert_eq!(json["question"], as_compiled(&question));
    }

    for (waiting, question) in [
        (on("const.other"), Some(&question)),
        (on("const.mode"), None),
        (Waiting::Free, Some(&question)),
        (Waiting::IntelligenceChoice, Some(&question)),
    ] {
        let json = work(waiting, question);
        assert!(json.get("question").is_none(), "{json}");
    }
}
