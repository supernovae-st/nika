// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::{Path, PathBuf};

use super::{
    Audit, CONTRACT, Candidate, Landing, Rail, Request, RequestedRun, Run, RunEnd, Saved, Stage,
    Waiting, Work,
};
use crate::change::{
    ProjectChange, ProjectChangeSet, RunRequest, Witness, WorkflowAudit, check_on_disk,
};
use crate::outcome::{GateId, ProposalId};
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
    assert_eq!(workflow.replaces.as_ref(), Some(&base));
    let audit = workflow.audit.as_ref().expect("the workflow's audit");
    assert_eq!(audit.world.basis, Basis::Declared);
    assert_eq!(audit.world.reach, Reach::LocalServices);

    assert_eq!(notes.landing, Landing::Create);
    assert!(!notes.workflow);
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
