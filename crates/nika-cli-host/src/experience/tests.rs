// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The router's obligatory situations — every case the checkpoint
//! names gets exactly one primary action, at most two alternatives,
//! and never a run: the table below IS the product contract.

use super::*;

fn workspace_state() -> ExperienceStateV1 {
    ExperienceStateV1 {
        context_mode: ContextModeV1::Workspace,
        evidence: ContextEvidenceV1::HostSelection,
        root: Some("/work/acme".to_owned()),
        writable: true,
        inventory_complete: true,
        workflow: WorkflowStateV1::Absent,
        ..ExperienceStateV1::chat_only()
    }
}

#[test]
fn chat_without_folder_discovers_and_never_scans() {
    let action = route(&ExperienceStateV1::chat_only());
    assert_eq!(action.action_id, "discover_example");
    assert!(!action.preview_required, "discovery mutates nothing");
    assert!(action.nothing_has_run);
    assert!(action.alternatives.contains(&"choose_project"));
}

#[test]
fn chat_only_with_a_repeated_task_chooses_a_project_first() {
    let state = ExperienceStateV1 {
        intent: IntentClassV1::Create,
        ..ExperienceStateV1::chat_only()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "choose_project");
    assert!(action.alternatives.contains(&"discover_example"));
}

#[test]
fn an_attachment_is_never_workspace_evidence() {
    // An attached file proves itself, not a repository: the state
    // stays chat-only and the router never names a project.
    let state = ExperienceStateV1 {
        evidence: ContextEvidenceV1::Attachment,
        ..ExperienceStateV1::chat_only()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "discover_example");
}

#[test]
fn multi_root_requires_an_explicit_selection() {
    let state = ExperienceStateV1 {
        context_mode: ContextModeV1::MultiRootUnselected,
        evidence: ContextEvidenceV1::HostSelection,
        ..ExperienceStateV1::chat_only()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "select_root");
}

#[test]
fn version_drift_degrades_every_richer_move() {
    // UX107-02: an incoherent train outranks even a paused run — a
    // stale kit teaches a stale language.
    let state = ExperienceStateV1 {
        versions_coherent: false,
        workflow: WorkflowStateV1::Paused,
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "align_versions");
    assert!(action.blocked_by.contains(&"version_drift"));
}

#[test]
fn a_paused_run_outranks_everything_under_a_proven_root() {
    let state = ExperienceStateV1 {
        workflow: WorkflowStateV1::Paused,
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "answer_pending_decision");
    // The reply resumes execution — the same explicit human gate as
    // every mutating door (refuter finding C5).
    assert!(action.consent_required, "answering resumes the run");
}

#[test]
fn a_failed_run_is_understood_before_anything_new() {
    let state = ExperienceStateV1 {
        workflow: WorkflowStateV1::Failed,
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "understand_latest_failure");
}

#[test]
fn findings_repair_and_never_run() {
    let state = ExperienceStateV1 {
        workflow: WorkflowStateV1::Findings,
        findings: 3,
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "repair_findings");
    assert!(action.preview_required, "a fix mutates — preview first");
    assert!(action.consent_required);
}

#[test]
fn a_truncated_inventory_tells_the_truth_before_founding() {
    // P0-4: zero-under-truncation is unknown — the deep scan speaks
    // before any founding CTA presumes an empty workspace.
    let state = ExperienceStateV1 {
        workflow: WorkflowStateV1::Unknown,
        inventory_complete: false,
        ..workspace_state()
    };
    assert_eq!(route(&state).action_id, "deep_scan");
}

#[test]
fn a_clean_workflow_opens_and_the_run_stays_a_human_ask() {
    let state = ExperienceStateV1 {
        workflow: WorkflowStateV1::Clean,
        workflow_path: Some("wf.nika.yaml".to_owned()),
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "open_workflow");
    assert!(
        !action.action_id.contains("run"),
        "no door hides a run (D-004)"
    );
}

#[test]
fn several_workflows_open_the_chooser_never_a_first_pick() {
    let state = ExperienceStateV1 {
        workflow: WorkflowStateV1::Several,
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "open_project_workflows");
    assert!(!action.consent_required, "a chooser reads, never mutates");
}

#[test]
fn create_intent_previews_before_any_file_exists() {
    let state = ExperienceStateV1 {
        intent: IntentClassV1::Create,
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "preview_draft");
    assert!(action.preview_required);
    assert!(action.consent_required);
}

#[test]
fn a_read_only_root_degrades_to_an_in_chat_preview() {
    let state = ExperienceStateV1 {
        intent: IntentClassV1::Create,
        writable: false,
        ..workspace_state()
    };
    let action = route(&state);
    assert_eq!(action.action_id, "preview_in_chat");
    assert!(action.blocked_by.contains(&"root_read_only"));
}

#[test]
fn every_route_caps_alternatives_at_two_and_runs_nothing() {
    // The exhaustive sweep: whatever the state, the contract holds —
    // one primary, ≤2 alternatives, nothing_has_run, and no action id
    // that smuggles an execution.
    let modes = [
        ContextModeV1::ChatOnly,
        ContextModeV1::Workspace,
        ContextModeV1::MultiRootUnselected,
    ];
    let workflows = [
        WorkflowStateV1::Absent,
        WorkflowStateV1::Unknown,
        WorkflowStateV1::Clean,
        WorkflowStateV1::Several,
        WorkflowStateV1::Findings,
        WorkflowStateV1::Paused,
        WorkflowStateV1::Failed,
    ];
    let intents = [
        IntentClassV1::Create,
        IntentClassV1::Discover,
        IntentClassV1::Continue,
        IntentClassV1::Unknown,
    ];
    for mode in modes {
        for workflow in workflows {
            for intent in intents {
                for writable in [false, true] {
                    for coherent in [false, true] {
                        let state = ExperienceStateV1 {
                            context_mode: mode,
                            workflow,
                            intent,
                            writable,
                            versions_coherent: coherent,
                            ..workspace_state()
                        };
                        let action = route(&state);
                        assert!(action.alternatives.len() <= 2, "{action:?}");
                        assert!(action.nothing_has_run, "{action:?}");
                        assert!(
                            !action.action_id.starts_with("run"),
                            "no primary is a run: {action:?}"
                        );
                        assert!(!action.label.is_empty() && !action.why_now.is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn the_preview_renders_strict_ascii_and_never_says_free() {
    let preview = WorkflowPreviewV1 {
        schema_version: 1,
        goal: "Every Monday, turn tickets into priorities".to_owned(),
        inputs: vec!["tickets.json".to_owned()],
        steps: vec!["Read".to_owned(), "Rank".to_owned(), "Write".to_owned()],
        output: "out/priorities.md".to_owned(),
        reads: vec!["tickets.json".to_owned()],
        writes: vec!["out/priorities.md".to_owned()],
        network: vec![],
        cost: PreviewCostV1::Unpriced,
        nothing_has_run: true,
    };
    let ascii = preview.render_ascii();
    assert!(ascii.is_ascii(), "strict ASCII projection: {ascii}");
    assert!(ascii.contains("nothing has run"));
    assert!(
        !ascii.to_lowercase().contains("free"),
        "unpriced is never worded free: {ascii}"
    );
    assert!(ascii.contains("unpriced"));

    let priced = WorkflowPreviewV1 {
        cost: PreviewCostV1::Ceiling { usd: 0.04 },
        network: vec!["api.openai.com".to_owned()],
        ..preview
    };
    let ascii = priced.render_ascii();
    assert!(ascii.contains("ceiling <= $0.04"));
    assert!(ascii.contains("api.openai.com"));
}

#[test]
fn every_mutating_action_carries_its_gates_structurally() {
    // The refuter's C5 lesson, made structural: the gate contract is
    // asserted per action ID across the whole state space, never ad
    // hoc per arm. Mutating actions preview+consent; the resume-class
    // action consents; everything else stays gate-free (a gate on a
    // read would train users to click through gates).
    let mutating = ["repair_findings", "preview_draft"];
    let consent_only = ["answer_pending_decision"];
    let modes = [
        ContextModeV1::ChatOnly,
        ContextModeV1::Workspace,
        ContextModeV1::MultiRootUnselected,
    ];
    let workflows = [
        WorkflowStateV1::Absent,
        WorkflowStateV1::Unknown,
        WorkflowStateV1::Clean,
        WorkflowStateV1::Several,
        WorkflowStateV1::Findings,
        WorkflowStateV1::Paused,
        WorkflowStateV1::Failed,
    ];
    let intents = [
        IntentClassV1::Create,
        IntentClassV1::Discover,
        IntentClassV1::Continue,
        IntentClassV1::Unknown,
    ];
    for mode in modes {
        for workflow in workflows {
            for intent in intents {
                for writable in [false, true] {
                    let state = ExperienceStateV1 {
                        context_mode: mode,
                        workflow,
                        intent,
                        writable,
                        ..workspace_state()
                    };
                    let action = route(&state);
                    if mutating.contains(&action.action_id) {
                        assert!(
                            action.preview_required && action.consent_required,
                            "mutating action must be fully gated: {action:?}"
                        );
                    } else if consent_only.contains(&action.action_id) {
                        assert!(
                            action.consent_required,
                            "resume-class action must carry consent: {action:?}"
                        );
                    } else {
                        assert!(
                            !action.consent_required && !action.preview_required,
                            "a read-only action never trains gate-clicking: {action:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_mermaid_projection_speaks_the_same_content_as_ascii() {
    let preview = WorkflowPreviewV1 {
        schema_version: 1,
        goal: "Rank tickets".to_owned(),
        inputs: vec!["tickets.json".to_owned()],
        steps: vec!["Read".to_owned(), "Say \"rank\"".to_owned()],
        output: "out/priorities.md".to_owned(),
        reads: vec![],
        writes: vec![],
        network: vec![],
        cost: PreviewCostV1::Unknown,
        nothing_has_run: true,
    };
    let mermaid = preview.render_mermaid();
    assert!(mermaid.starts_with("flowchart LR"));
    assert!(mermaid.contains("in[\"tickets.json\"]"));
    assert!(mermaid.contains("out[\"out/priorities.md\"]"));
    assert!(mermaid.contains("s1 --> s2"));
    // A quote in a step name can never break out of its node.
    assert!(mermaid.contains("Say 'rank'"), "{mermaid}");
    assert!(!mermaid.contains("\"rank\""), "{mermaid}");
}

#[test]
fn the_context_receipt_says_chat_only_or_names_its_scope() {
    let none = ContextReceiptV1 {
        schema_version: 1,
        source: ContextEvidenceV1::None,
        root: None,
        writable: false,
        remote: None,
        assumptions: vec![],
    };
    assert!(none.render_line().contains("no project attached"));

    let remote = ContextReceiptV1 {
        schema_version: 1,
        source: ContextEvidenceV1::HostSelection,
        root: Some("/work/api".to_owned()),
        writable: false,
        remote: Some("container dev-api".to_owned()),
        assumptions: vec![],
    };
    let line = remote.render_line();
    assert!(line.contains("/work/api"), "{line}");
    assert!(line.contains("container dev-api"), "{line}");
    assert!(line.contains("read-only"), "{line}");
}

#[test]
fn the_privacy_receipt_keeps_unknown_retention_unknown() {
    let receipt = PrivacyReceiptV1 {
        schema_version: 1,
        provider: "cloud-provider".to_owned(),
        model: None,
        data_classes: vec!["prompt".to_owned()],
        excluded: vec![".env".to_owned()],
        destinations: vec!["provider-api.example".to_owned()],
        retention: RetentionV1::Unknown,
    };
    let json = serde_json::to_string(&receipt).expect("serializes");
    assert!(json.contains("\"unknown\""), "{json}");
    for banned in ["private", "secure", "free"] {
        assert!(!json.contains(banned), "no comfort adjective: {json}");
    }
}

#[test]
fn the_authority_receipt_names_the_denied_lane() {
    let receipt = AuthorityReceiptV1 {
        schema_version: 1,
        reads: vec!["tickets.json".to_owned()],
        writes: vec!["out/priorities.md".to_owned()],
        network: vec![],
        effects: vec!["create_file".to_owned()],
        denied: vec!["shell".to_owned(), "deploy".to_owned()],
        source: "workflow.permits".to_owned(),
    };
    let json = serde_json::to_value(&receipt).expect("serializes");
    assert_eq!(json["source"], "workflow.permits");
    assert_eq!(json["denied"][0], "shell");
}

#[test]
fn the_state_serializes_with_its_schema_version() {
    let state = workspace_state();
    let json = serde_json::to_value(&state).expect("serializes");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["context_mode"], "workspace");
    assert_eq!(json["evidence"], "host_selection");
}
