// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::change::{ProjectChange, ProjectChangeSet};
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::outcome::ProposalId;
use crate::reasoner::NoReasoner;
use crate::runtime::tests::ready;

const WORKFLOW: &str = "nika: saved\ntasks:\n  done:\n    exec:\n      argv: [printf, ok]\n";

fn open(root: &Path) -> SessionRuntime {
    SessionRuntime::open(
        root,
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    )
}

fn save(root: &Path, input: &str) -> ConsentRecord {
    let mut session = open(root);
    assert!(session.admit_money(input, false, false).is_ok());
    let path = PathBuf::from("saved.nika");
    let changes = vec![if root.join(&path).exists() {
        ProjectChange::UpdateWorkflow {
            path,
            before: Witness::of(WORKFLOW.as_bytes()),
            content: WORKFLOW.into(),
        }
    } else {
        ProjectChange::CreateWorkflow {
            path,
            content: WORKFLOW.into(),
        }
    }];
    let set = ProjectChangeSet {
        root: root.into(),
        goal: input.into(),
        changes,
        run: None,
        repairs: vec![],
        audits: vec![],
    };
    let id = ProposalId::of(&session.draft_preview(&set));
    session.bind_proposal_money(&id);
    let applied = set.apply().expect("saved");
    session.save_proposal_money(&set, &id);
    assert_eq!(session.evidence_applied(&set, &id, &applied), "");
    ConsentRecord::read_all(root)
        .expect("consents")
        .pop()
        .expect("Save")
}

#[test]
fn reopening_keeps_the_exact_saved_ceiling_without_restoring_authority() {
    for (input, amount, source) in [
        (
            "Prepare the workflow. Budget: $0.",
            0.0_f64,
            MonetarySource::Explicit,
        ),
        (
            "Prepare the workflow. Budget: $0.07.",
            0.07_f64,
            MonetarySource::Explicit,
        ),
        (
            "Prepare the workflow.",
            crate::runtime::DEFAULT_CEILING_USD,
            MonetarySource::SessionDefault,
        ),
    ] {
        let dir = tempfile::tempdir().expect("root");
        save(dir.path(), input);
        let mut reopened = open(dir.path());
        assert!(reopened.money.saved.is_empty());
        assert!(reopened.requested_run.is_none());
        assert!(reopened.pending.is_none());
        assert!(reopened.money.account.is_none());
        let result = reopened
            .run_money("run ./saved.nika", Path::new("./saved.nika"), None)
            .expect("saved constraint");
        assert_eq!(result.to_bits(), amount.to_bits());
        let decision = reopened.monetary_decision().expect("observed");
        assert_eq!(decision.source, source);
        assert_eq!(decision.input, input);
        assert_eq!(decision.original_intent, input);
        assert_eq!(
            decision.proposal, None,
            "a receipt grants no proposal authority"
        );
        assert!(reopened.money.account.is_none());
        assert!(
            reopened.requested_run.is_none(),
            "reading the ceiling never requests a Run"
        );
    }
}

#[test]
fn a_changed_saved_file_cannot_inherit_the_receipt_or_a_default() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    std::fs::write(
        dir.path().join("saved.nika"),
        format!("{WORKFLOW}# changed\n"),
    )
    .expect("changed");
    let mut reopened = open(dir.path());
    // A changed file is no missing proof: it refuses, and a stated ceiling is not asked.
    assert!(matches!(
        reopened.run_money("run saved.nika", Path::new("saved.nika"), None),
        Err(super::super::Unadmitted::Refused(_))
    ));
    assert_eq!(
        reopened.monetary_decision().expect("refusal").effective_usd,
        None
    );
    assert!(reopened.requested_run.is_none());
}

#[test]
fn the_latest_save_owns_the_ceiling_even_when_bytes_are_identical() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    save(dir.path(), "Prepare the workflow. Budget: $0.03.");
    let mut reopened = open(dir.path());
    assert_eq!(
        reopened
            .run_money("run saved.nika", Path::new("saved.nika"), None)
            .expect("latest")
            .to_bits(),
        0.03_f64.to_bits()
    );
}

#[test]
fn a_legacy_or_unreadable_receipt_never_supplies_a_default() {
    for corrupt in [false, true] {
        let dir = tempfile::tempdir().expect("root");
        save(dir.path(), "Prepare the workflow. Budget: $0.07.");
        let receipt = dir.path().join(".nika").join(FILE);
        if corrupt {
            std::fs::write(receipt, "{bad\n").expect("corrupt");
        } else {
            std::fs::remove_file(receipt).expect("legacy Save");
        }
        let mut reopened = open(dir.path());
        // A Save that left no receipt proves no ceiling (one stated admits the Run); an
        // unreadable receipt refuses as it is.
        let unadmitted = reopened.run_money("run saved.nika", Path::new("saved.nika"), None);
        assert_eq!(
            matches!(unadmitted, Err(super::super::Unadmitted::Unproven(_))),
            !corrupt,
            "{unadmitted:?}"
        );
        assert_eq!(
            matches!(unadmitted, Err(super::super::Unadmitted::Refused(_))),
            corrupt,
            "{unadmitted:?}"
        );
        let refused = reopened.monetary_decision().expect("refusal");
        assert_eq!(refused.effective_usd, None);
        assert!(
            refused
                .refusal
                .as_deref()
                .expect("reason")
                .contains("explicit Run ceiling")
        );
        assert_eq!(
            reopened
                .run_money("run saved.nika", Path::new("saved.nika"), Some(0.02))
                .expect("fresh explicit ceiling")
                .to_bits(),
            0.02_f64.to_bits()
        );
    }
}

#[test]
fn an_old_receipt_cannot_cover_a_later_save_without_evidence() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    let file = dir.path().join(".nika").join(FILE);
    let earlier = std::fs::read(&file).expect("earlier receipt");
    save(dir.path(), "Prepare the workflow. Budget: $0.03.");
    std::fs::write(file, earlier).expect("later receipt unavailable");
    let mut reopened = open(dir.path());
    // The later Save's ceiling cannot be proved: a ceiling the person states admits the Run.
    assert!(matches!(
        reopened.run_money("run saved.nika", Path::new("saved.nika"), None),
        Err(super::super::Unadmitted::Unproven(_))
    ));
}

#[test]
fn the_same_bytes_at_another_path_do_not_inherit_a_saved_constraint() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    std::fs::copy(dir.path().join("saved.nika"), dir.path().join("other.nika"))
        .expect("independent file");
    let mut reopened = open(dir.path());
    assert_eq!(
        reopened
            .run_money("run other.nika", Path::new("other.nika"), None)
            .expect("independent default")
            .to_bits(),
        crate::runtime::DEFAULT_CEILING_USD.to_bits()
    );
    assert_eq!(
        reopened.monetary_decision().expect("observation").source,
        MonetarySource::SessionDefault
    );
}

#[test]
fn a_nonfinite_or_negative_kept_amount_cannot_be_admitted() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    let file = dir.path().join(".nika").join(FILE);
    let original = std::fs::read_to_string(&file).expect("receipt");
    let mut receipt: serde_json::Value = serde_json::from_str(original.trim()).expect("JSON");
    for amount in [serde_json::json!(-1), serde_json::Value::Null] {
        receipt["bindings"][0]["effective_usd"] = amount;
        std::fs::write(&file, format!("{receipt}\n")).expect("invalid receipt");
        let mut reopened = open(dir.path());
        assert!(
            reopened
                .run_money("run saved.nika", Path::new("saved.nika"), None)
                .is_err()
        );
        assert_eq!(
            reopened.monetary_decision().expect("refused").effective_usd,
            None
        );
    }
}

#[cfg(unix)]
#[test]
fn a_receipt_symlink_is_refused_and_a_workflow_alias_keeps_its_ceiling() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    symlink("saved.nika", dir.path().join("alias.nika")).expect("workflow alias");
    let mut reopened = open(dir.path());
    assert_eq!(
        reopened
            .run_money("run alias.nika", Path::new("alias.nika"), None)
            .expect("same identity")
            .to_bits(),
        0.07_f64.to_bits()
    );
    let file = dir.path().join(".nika").join(FILE);
    let outside = tempfile::NamedTempFile::new().expect("outside");
    std::fs::copy(&file, outside.path()).expect("copied");
    std::fs::remove_file(&file).expect("remove owned file");
    symlink(outside.path(), &file).expect("foreign receipt link");
    let mut reopened = open(dir.path());
    assert!(
        reopened
            .run_money("run saved.nika", Path::new("saved.nika"), None)
            .is_err()
    );
    assert_eq!(
        reopened.monetary_decision().expect("refused").effective_usd,
        None
    );
}

#[test]
fn a_revision_after_reopen_keeps_the_saved_ceiling_and_rejects_stale_bytes() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    let base = Witness::of(WORKFLOW.as_bytes());
    let mut reopened = open(dir.path());
    assert!(
        reopened
            .admit_money("Change the output", false, false)
            .is_ok()
    );
    assert!(
        reopened
            .bind_revision_money(Path::new("saved.nika"), "Change the output", &base)
            .is_ok()
    );
    assert_eq!(
        reopened
            .money
            .draft
            .as_ref()
            .expect("revision ceiling")
            .effective_usd,
        Some(0.07)
    );
    assert_eq!(
        reopened.monetary_decision().expect("current").effective_usd,
        Some(0.07)
    );
    assert!(reopened.money.account.is_none());
    assert!(reopened.pending.is_none() && reopened.requested_run.is_none());
    let changed = format!("{WORKFLOW}# externally changed\n");
    std::fs::write(dir.path().join("saved.nika"), &changed).expect("changed");
    let mut reopened = open(dir.path());
    assert!(
        reopened
            .admit_money("Change the output", false, false)
            .is_ok()
    );
    assert!(
        reopened
            .bind_revision_money(
                Path::new("saved.nika"),
                "Change the output",
                &Witness::of(changed.as_bytes())
            )
            .is_err()
    );
    assert_eq!(
        reopened.monetary_decision().expect("refusal").effective_usd,
        None
    );
    let explicit = "Change the output. Budget: $0.02.";
    assert!(reopened.admit_money(explicit, false, false).is_ok());
    assert!(
        reopened
            .bind_revision_money(
                Path::new("saved.nika"),
                explicit,
                &Witness::of(changed.as_bytes())
            )
            .is_ok()
    );
    assert_eq!(
        reopened
            .money
            .draft
            .as_ref()
            .expect("explicit revision ceiling")
            .effective_usd,
        Some(0.02)
    );
}

#[test]
fn a_newer_save_overrides_an_older_sessions_live_binding() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    let mut older = open(dir.path());
    let consent = ConsentRecord::read_all(dir.path())
        .expect("journal")
        .pop()
        .expect("Save");
    let resolved = dir
        .path()
        .join("saved.nika")
        .canonicalize()
        .expect("identity");
    let witness = Witness::of(WORKFLOW.as_bytes());
    let decision = restore(dir.path(), &consent, &resolved, Some(&witness))
        .expect("receipt")
        .expect("constraint");
    older.money.saved.push(SavedMoney {
        path: PathBuf::from("saved.nika"),
        resolved: Some(resolved),
        witness,
        decision,
    });
    save(dir.path(), "Prepare the workflow. Budget: $0.");
    assert_eq!(
        older
            .run_money("run saved.nika", Path::new("saved.nika"), None)
            .expect("newer ceiling")
            .to_bits(),
        0.0_f64.to_bits()
    );
    assert!(older.money.account.is_none() && older.requested_run.is_none());
}

#[test]
fn identical_duplicate_receipts_are_harmless_but_conflicting_provenance_refuses() {
    let dir = tempfile::tempdir().expect("root");
    save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    let file = dir.path().join(".nika").join(FILE);
    let line = std::fs::read_to_string(&file).expect("receipt");
    std::fs::write(&file, format!("{line}{line}")).expect("same evidence twice");
    let mut reopened = open(dir.path());
    assert_eq!(
        reopened
            .run_money("run saved.nika", Path::new("saved.nika"), None)
            .expect("identical")
            .to_bits(),
        0.07_f64.to_bits()
    );
    let mut conflict: serde_json::Value = serde_json::from_str(line.trim()).expect("JSON");
    conflict["bindings"][0]["input"] = serde_json::json!("different provenance");
    std::fs::write(&file, format!("{line}{conflict}\n")).expect("conflicting evidence");
    let mut reopened = open(dir.path());
    assert!(matches!(
        reopened.run_money("run saved.nika", Path::new("saved.nika"), None),
        Err(super::super::Unadmitted::Refused(_))
    ));
    assert_eq!(
        reopened.monetary_decision().expect("refused").effective_usd,
        None
    );
}

#[test]
fn project_defaults_and_explicit_overrides_keep_their_saved_provenance() {
    for (input, amount, source) in [
        (
            "Prepare the workflow.",
            0.19_f64,
            MonetarySource::ProjectDefault,
        ),
        (
            "Prepare the workflow. Budget: $0.07.",
            0.07_f64,
            MonetarySource::Override,
        ),
    ] {
        let dir = tempfile::tempdir().expect("root");
        let project = dir.path().join("nika.yaml");
        std::fs::write(&project, "nika: money-test\nceiling: 0.19\n").expect("project default");
        let mut initial = open(dir.path());
        initial
            .admit_money(input, false, false)
            .expect("initial ceiling");
        let initial_project_file = initial
            .monetary_decision()
            .expect("initial observation")
            .project_file
            .clone()
            .expect("initial project provenance");
        save(dir.path(), input);
        std::fs::write(&project, "nika: money-test\nceiling: 0.99\n").expect("new default");
        let mut reopened = open(dir.path());
        assert_eq!(
            reopened
                .run_money("run saved.nika", Path::new("saved.nika"), None)
                .expect("saved ceiling")
                .to_bits(),
            amount.to_bits()
        );
        let decision = reopened.monetary_decision().expect("observation");
        assert_eq!(decision.source, source);
        assert_eq!(decision.project_default_usd, Some(0.19));
        assert_eq!(decision.project_file.as_ref(), Some(&initial_project_file));
        assert_eq!(decision.input, input);
        assert_eq!(
            decision.explicit_amount.is_some(),
            source == MonetarySource::Override
        );
        assert!(reopened.money.account.is_none() && reopened.requested_run.is_none());
    }
}

/// A Save made in this Session, still live: its Session, the proposal it applied and its consent.
fn live_save(root: &Path, input: &str) -> (SessionRuntime, ProposalId) {
    let mut session = open(root);
    assert!(session.admit_money(input, false, false).is_ok());
    let set = ProjectChangeSet {
        root: root.into(),
        goal: input.into(),
        changes: vec![ProjectChange::CreateWorkflow {
            path: PathBuf::from("saved.nika"),
            content: WORKFLOW.into(),
        }],
        run: None,
        repairs: vec![],
        audits: vec![],
    };
    let id = ProposalId::of(&session.draft_preview(&set));
    session.bind_proposal_money(&id);
    let applied = set.apply().expect("saved");
    session.save_proposal_money(&set, &id);
    assert_eq!(session.evidence_applied(&set, &id, &applied), "");
    (session, id)
}

#[test]
fn a_run_of_this_sessions_save_names_its_proposal_and_a_reopen_names_none() {
    let dir = tempfile::tempdir().expect("root");
    let input = "Prepare the workflow. Budget: $0.07.";
    let (mut live, id) = live_save(dir.path(), input);
    assert!(
        dir.path().join(".nika").join(FILE).is_file(),
        "the receipt is durable"
    );
    let ceiling = live
        .run_money("run saved.nika", Path::new("saved.nika"), None)
        .expect("saved ceiling");
    assert_eq!(ceiling.to_bits(), 0.07_f64.to_bits());
    let decision = live.monetary_decision().expect("observed");
    assert_eq!(decision.proposal.as_ref(), Some(&id));
    assert_eq!(decision.source, MonetarySource::Explicit);
    assert!(live.money.account.is_none() && live.requested_run.is_none());
    drop(live);
    let mut reopened = open(dir.path());
    let ceiling = reopened
        .run_money("run saved.nika", Path::new("saved.nika"), None)
        .expect("saved ceiling");
    assert_eq!(ceiling.to_bits(), 0.07_f64.to_bits());
    let decision = reopened.monetary_decision().expect("observed");
    assert_eq!(decision.proposal, None, "a receipt grants no proposal");
    assert!(reopened.pending.is_none() && reopened.requested_run.is_none());
}

#[test]
fn a_newer_save_elsewhere_keeps_its_own_ceiling_without_this_sessions_proposal() {
    let dir = tempfile::tempdir().expect("root");
    let (mut live, id) = live_save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    save(dir.path(), "Prepare the workflow. Budget: $0.");
    let ceiling = live
        .run_money("run saved.nika", Path::new("saved.nika"), None)
        .expect("newer ceiling");
    assert_eq!(ceiling.to_bits(), 0.0_f64.to_bits());
    let decision = live.monetary_decision().expect("observed");
    assert_ne!(decision.proposal.as_ref(), Some(&id));
    assert_eq!(decision.proposal, None);
}

#[test]
fn an_unreadable_receipt_refuses_a_live_save_and_names_no_proposal() {
    let dir = tempfile::tempdir().expect("root");
    let (mut live, _) = live_save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    let file = dir.path().join(".nika").join(FILE);
    let receipt_text = std::fs::read_to_string(&file).expect("receipt");
    std::fs::write(&file, format!("{receipt_text}{{not a receipt\n")).expect("corrupt");
    assert!(matches!(
        live.run_money("run saved.nika", Path::new("saved.nika"), None),
        Err(super::super::Unadmitted::Refused(_))
    ));
    let decision = live.monetary_decision().expect("refused");
    assert_eq!(decision.effective_usd, None);
    assert_eq!(decision.proposal, None);
}

#[test]
fn a_one_turn_ceiling_names_no_proposal_and_leaves_the_saved_binding() {
    let dir = tempfile::tempdir().expect("root");
    let (mut live, id) = live_save(dir.path(), "Prepare the workflow. Budget: $0.07.");
    let stated = live
        .run_money(
            "run saved.nika budget 2 USD",
            Path::new("saved.nika"),
            Some(2.0),
        )
        .expect("stated ceiling");
    assert_eq!(stated.to_bits(), 2.0_f64.to_bits());
    assert_eq!(live.monetary_decision().expect("stated").proposal, None);
    let saved = live
        .run_money("run saved.nika", Path::new("saved.nika"), None)
        .expect("saved ceiling");
    assert_eq!(saved.to_bits(), 0.07_f64.to_bits());
    assert_eq!(
        live.monetary_decision().expect("saved").proposal.as_ref(),
        Some(&id)
    );
}
