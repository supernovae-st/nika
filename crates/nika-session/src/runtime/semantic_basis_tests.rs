// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The source basis of a proposal the semantic route (a sketch and its fills) authored, through
//! the real Session → compiler → registry → reqwest path on loopback, with scripted seat answers:
//! a yes saves the exact bytes over an unchanged project without any run, and withdraws them when
//! a key the program reads or its source is gone; rows added never move it. Nothing leaves the
//! machine.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
use super::inference_tests::wire::{Peer, response};
use super::*;
use crate::DataLocus;
use crate::reasoner::{ProviderReasoner, test_transport};
use nika_onboard::compile::program_records::Place;
use serde_json::json;

const MODEL: &str = "deepseek/deepseek-v4-pro";
const WORK: &str = "Read inventory.json, keep the items whose stock is under 8, sort them by sku and write that JSON array to ./reorder.json.";
const ROWS: &str =
    r#"[{"sku": "B2", "stock": 3}, {"sku": "A1", "stock": 9}, {"sku": "C3", "stock": 5}]"#;
const JUDGE_APPROVES: &str = r#"{"choice":"faithful"}"#;

/// The seat's graph: read the inventory, keep and sort it, write the array.
fn graph() -> String {
    json!({"name": "reorder", "tasks": [
        {"id": "read_inventory", "verb": "invoke", "tool": "nika:read",
         "reads": ["inventory.json"], "purpose": "the inventory"},
        {"id": "pick", "verb": "invoke", "tool": "nika:jq",
         "with": [{"name": "document", "from": "read_inventory"}],
         "purpose": "keep the low stock items, sorted by sku"},
        {"id": "save", "verb": "invoke", "tool": "nika:write", "writes": ["./reorder.json"],
         "with": [{"name": "items", "from": "pick"}], "purpose": "write the array"}],
        "questions": [], "gaps": [], "notes": "graph"})
    .to_string()
}

/// The seat's fills: the one program, reading `stock` and `sku`.
fn fills() -> String {
    json!({"fills": [{"task": "pick", "field": "expression",
        "value": "fromjson | map(select(.stock < 8)) | sort_by(.sku) | map({sku, stock})"}],
        "notes": "fills"})
    .to_string()
}

/// A Session on the loopback seat, its sketch strategy observing `root`.
fn open(root: &Path) -> SessionRuntime {
    let selected = ResolvedSessionIntelligence::new(
        IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        Some(MODEL.into()),
        DataLocus::Metered {
            provider: "deepseek".into(),
        },
        true,
        None,
    );
    let reasoner = || ProviderReasoner {
        model: MODEL.into(),
        label: "DeepSeek".into(),
    };
    let mut s = SessionRuntime::open(root, selected, Box::new(reasoner()));
    s.factory = Some(Box::new(move |_| Box::new(reasoner())));
    s.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none().with_strategy("sketch"),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s
}

/// A proposal authored over `rows`: the scripted sketch, its fills and the judge's approval,
/// each a real loopback request.
struct Proposed {
    peer: Peer,
    _transport: test_transport::Installed,
    dir: tempfile::TempDir,
    s: SessionRuntime,
    id: ProposalId,
}

fn proposed(rows: &str) -> Proposed {
    let peer = Peer::start(vec![
        (200, response(&graph())),
        (200, response(&fills())),
        (200, response(JUDGE_APPROVES)),
    ]);
    let transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    std::fs::write(dir.path().join("inventory.json"), rows).expect("inventory");
    let mut s = open(dir.path());
    let out = s.turn(&format!("{WORK} budget 2 USD."));
    let TurnOutcome::Proposal { id, preview } = out else {
        panic!("a semantic proposal: {out:?}");
    };
    assert!(
        preview.contains("creates `reorder.nika` (35 lines)"),
        "{preview}"
    );
    assert_eq!(
        peer.bodies().len(),
        3,
        "the sketch, its fills and the judge"
    );
    Proposed {
        peer,
        _transport: transport,
        dir,
        s,
        id,
    }
}

impl Proposed {
    fn yes(&mut self) -> TurnOutcome {
        let id = self.id.clone();
        self.s.consent_to(&id, "yes")
    }

    /// The workflow the yes wrote, when it wrote one.
    fn saved(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.path().join("reorder.nika")).ok()
    }

    /// Nothing ran: no output, no run journal, no trace.
    fn nothing_ran(&self) {
        for path in ["reorder.json", ".nika/runs", ".nika/traces"] {
            assert!(!self.dir.path().join(path).exists(), "{path}");
        }
    }

    fn inventory(&self, text: Option<&str>) {
        let path = self.dir.path().join("inventory.json");
        match text {
            Some(text) => std::fs::write(path, text).expect("inventory"),
            None => std::fs::remove_file(path).expect("deleted"),
        }
    }
}

#[test]
fn an_unchanged_project_saves_the_exact_semantic_candidate_without_any_run() {
    let mut p = proposed(ROWS);
    let bound = p.s.basis.is_some();
    let out = p.yes();
    let TurnOutcome::Facts(text) = out else {
        panic!("saved: {out:?}");
    };
    assert!(bound, "a source basis is bound at the proposal");
    assert!(
        text.contains(
            "sources judged again before writing: 2 recorded fact(s) of `inventory.json` hold"
        ) && text.contains("nothing has run"),
        "save must report the rejudged source facts and that nothing ran"
    );
    let workflow = p.saved().expect("the workflow is saved");
    assert_eq!(workflow.lines().count(), 35, "{workflow}");
    assert!(
        workflow.contains("map(select(.stock < 8)) | sort_by(.sku)"),
        "{workflow}"
    );
    p.nothing_ran();
    let kept = std::fs::read_to_string(p.dir.path().join("inventory.json")).expect("kept");
    assert_eq!(kept, ROWS);
    assert_eq!(p.peer.bodies().len(), 3, "a yes calls no seat");
}

#[test]
fn a_removed_key_or_a_deleted_source_withdraws_and_rows_added_still_save() {
    // `stock` renamed in every record: the program's filter would read nothing.
    let mut p = proposed(ROWS);
    p.inventory(Some(&ROWS.replace("stock", "qty")));
    let out = p.yes();
    let TurnOutcome::Refusal(refusal) = out else {
        panic!("withdrawn: {out:?}");
    };
    assert!(
        refusal
            .text
            .contains("the sources this proposal was built on changed")
            && refusal.text.contains("`stock`")
            && refusal.text.contains("nothing was written"),
        "withdrawal must identify the changed source key and that nothing was written"
    );
    assert!(p.saved().is_none());
    p.nothing_ran();
    // The source deleted.
    let mut p = proposed(ROWS);
    p.inventory(None);
    let out = p.yes();
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.text.contains("nothing was written")),
        "{out:?}"
    );
    assert!(p.saved().is_none());
    p.nothing_ran();
    // Rows added and reordered: the recorded facts still hold.
    let mut p = proposed(ROWS);
    p.inventory(Some(
        r#"[{"sku": "D4", "stock": 1}, {"sku": "C3", "stock": 5}, {"sku": "A1", "stock": 9}, {"sku": "B2", "stock": 3}]"#,
    ));
    let out = p.yes();
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("2 recorded fact(s) of `inventory.json` hold")),
        "{out:?}"
    );
    assert!(p.saved().is_some());
    p.nothing_ran();
}

#[test]
fn meaning_shows_the_semantic_reading_and_claims_no_carrier() {
    let mut p = proposed(ROWS);
    let out = format!("{:?}", p.s.turn("/meaning"));
    assert!(
        out.contains("Meaning · your request as the compiler read it"),
        "{out}"
    );
    assert!(
        out.contains("the program was judged against the whole request, not clause by clause"),
        "{out}"
    );
    for never in ["unavailable", "represented", "needs your answer"] {
        assert!(!out.contains(never), "{never}: {out}");
    }
    assert!(p.saved().is_none(), "a view writes nothing");
}

fn assert_legacy_edit_blocked(
    s: &mut SessionRuntime,
    path: &std::path::Path,
    change: &str,
    before: &str,
    peer: &Peer,
) {
    assert!(
        s.money_blocks_cognition(),
        "the earlier constrained exposure is not a zero-spend account"
    );
    assert!(s.inference_receipt().expect("receipt state").is_none());
    let held = s.revise_saved(path, change);
    assert!(
        !matches!(
            held,
            TurnOutcome::Proposal { .. } | TurnOutcome::RunRequested { .. }
        ),
        "{held:?}"
    );
    assert_eq!(
        peer.bodies().len(),
        3,
        "a new ceiling cannot replace a missing prior ledger"
    );
    assert_eq!(
        std::fs::read_to_string(s.snapshot.root.join(path)).expect("preserved base"),
        before
    );
}

fn assert_restored_account(
    s: &mut SessionRuntime,
    prior: &nika_providers::InferenceReceipt,
    peer: &Peer,
) {
    let notice = s.restore_state().expect("project checkpoint");
    let restored = s.inference_receipt().expect("receipt").expect(&notice);
    assert_eq!(restored.state, nika_providers::AdmissionState::Closed);
    assert_eq!(restored.limit, prior.limit);
    assert_eq!(restored.estimated, prior.estimated);
    assert_eq!(restored.active, prior.active);
    assert_eq!(restored.held_unknown, prior.held_unknown);
    assert_eq!(restored.attempts, prior.attempts);
    assert!(
        s.money_blocks_cognition(),
        "restore grants no fresh allowance"
    );
    assert_eq!(peer.bodies().len(), 3, "restoring accounting calls nobody");
}

fn assert_revised_save(
    s: &mut SessionRuntime,
    path: &std::path::Path,
    before: &str,
    change: &str,
    peer: &Peer,
    prior: Option<&nika_providers::InferenceReceipt>,
) {
    let out = s.revise_saved(path, change);
    let TurnOutcome::Proposal { id, preview } = out else {
        panic!("semantic revision: {out:?}");
    };
    assert!(preview.contains("reorder.nika"), "{preview}");
    assert_eq!(
        std::fs::read_to_string(s.snapshot.root.join(path)).expect("unchanged before yes"),
        before
    );
    let out = s.consent_to(&id, "yes");
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("applied")),
        "{out:?}"
    );
    let after = std::fs::read_to_string(s.snapshot.root.join(path)).expect("revised bytes");
    assert!(
        after.contains("stock < 6") && after.contains("sort_by(.sku)"),
        "{after}"
    );
    assert!(!after.contains("stock < 8"));
    assert_eq!(
        peer.bodies().len(),
        6,
        "links, typed fills and judge; no fresh graph"
    );
    assert!(
        !s.snapshot.root.join("reorder.json").exists(),
        "Save never runs"
    );
    if let Some(prior) = prior {
        let receipt = s
            .inference_receipt()
            .expect("receipt")
            .expect("same account");
        assert_eq!(
            &receipt.attempts[..prior.attempts.len()],
            prior.attempts.as_slice()
        );
        assert_eq!(receipt.attempts.len(), prior.attempts.len() + 3);
        assert!(receipt.estimated.nano_usd > prior.estimated.nano_usd);
        assert_eq!(
            receipt.estimated.nano_usd,
            prior.estimated.nano_usd
                + receipt.attempts[prior.attempts.len()..]
                    .iter()
                    .map(|a| a.estimated.expect("settled revision call").nano_usd)
                    .sum::<i128>()
        );
    }
    assert!(
        nika_onboard::compile::program_records::plan(
            s.programs.as_ref(),
            Place::Saved("reorder.nika"),
            &after
        )
        .is_some()
    );
}

#[test]
fn semantic_reopen_preserves_records_and_never_replaces_a_prior_budgeted_exposure() {
    for (budgeted, restore_ledger) in [(false, false), (true, false), (true, true)] {
        const CHANGE: &str = "Keep the items whose stock is under 6 instead. Budget 2 USD.";
        let links = json!({"supersedes": [{
        "replaces": "keep the items whose stock is under 8",
        "by": "Keep the items whose stock is under 6 instead"
    }], "adds": [], "notes": "replace the filter, preserve sorting and destination"});
        let peer = Peer::start(vec![
            (200, response(&graph())),
            (200, response(&fills())),
            (200, response(JUDGE_APPROVES)),
            (200, response(&links.to_string())),
            (200, response(&fills().replace("stock < 8", "stock < 6"))),
            (200, response(JUDGE_APPROVES)),
        ]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().expect("root");
        let home = tempfile::tempdir().expect("private history");
        std::fs::write(dir.path().join("inventory.json"), ROWS).expect("inventory");
        let mut s = open(dir.path());
        s.enable_history(home.path()).expect("history");
        // History alone preserves the old budgeted negative. A separate unbudgeted creation
        // and a complete ledger restored closed prove the two valid continuations.
        let input = if budgeted {
            format!("{WORK} Budget 2 USD.")
        } else {
            WORK.to_owned()
        };
        let out = s.turn(&input);
        let TurnOutcome::Proposal { id, .. } = out else {
            panic!("base proposal: {out:?}");
        };
        let out = s.consent_to(&id, "yes");
        assert!(
            matches!(&out, TurnOutcome::Facts(t) if t.contains("applied")),
            "{out:?}"
        );
        let path = PathBuf::from("reorder.nika");
        let before = std::fs::read_to_string(dir.path().join(&path)).expect("saved base");
        assert!(
            nika_onboard::compile::program_records::plan(
                s.programs.as_ref(),
                Place::Saved("reorder.nika"),
                &before
            )
            .is_some()
        );
        assert_eq!(peer.bodies().len(), 3);
        let prior = budgeted.then(|| s.inference_receipt().expect("receipt").expect("account"));
        drop(s);

        let mut s = open(dir.path());
        s.enable_history(home.path()).expect("reopened history");
        assert_eq!(peer.bodies().len(), 3, "reopening calls nobody");
        assert_eq!(s.last_workflow.as_ref(), Some(&path));
        assert!(s.pending_proposal().is_none() && s.money.account.is_none());
        assert!(
            nika_onboard::compile::program_records::plan(
                s.programs.as_ref(),
                Place::Saved("reorder.nika"),
                &before
            )
            .is_some()
        );
        if restore_ledger {
            assert_restored_account(&mut s, prior.as_ref().expect("previous account"), &peer);
        }
        s.admit_money(CHANGE, false, false)
            .expect("this revision's explicit admission");
        if budgeted && !restore_ledger {
            assert_legacy_edit_blocked(&mut s, &path, CHANGE, &before, &peer);
            continue;
        }
        if restore_ledger {
            let admitted = s
                .inference_receipt()
                .expect("receipt")
                .expect("amended account");
            let prior = prior.as_ref().expect("previous account");
            assert_eq!(admitted.state, nika_providers::AdmissionState::Open);
            assert_eq!(
                admitted.estimated, prior.estimated,
                "ceiling is total, not a reset"
            );
            assert_eq!(admitted.attempts, prior.attempts);
            assert_eq!(peer.bodies().len(), 3, "amending does not dispatch");
        }
        assert_revised_save(
            &mut s,
            &path,
            &before,
            CHANGE,
            &peer,
            prior.as_ref().filter(|_| restore_ledger),
        );
    }
}

#[test]
fn a_pending_edit_receives_its_exact_record_and_keeps_the_original_when_compile_fails() {
    let mut p = proposed(ROWS);
    let set = p.s.pending.take().expect("proposal");
    let original = set.clone();
    let expected = nika_onboard::compile::program_records::plan(
        p.s.programs.as_ref(),
        Place::Proposal(&p.id.to_string()),
        set.changes[0].content(),
    )
    .expect("the proposed semantic record");
    let out =
        p.s.revise_pending_with(set, "Use a threshold of 6 instead.", |_, request, _| {
            assert_eq!(request.plan.as_ref(), Some(&expected));
            Err(crate::authoring::AuthoringError::Runtime(
                "injected failure before dispatch".into(),
            ))
        });
    assert!(matches!(out, TurnOutcome::Refusal(_)));
    assert_eq!(p.s.pending.as_ref(), Some(&original));
    assert_eq!(
        p.peer.bodies().len(),
        3,
        "a failed injected revision made no provider call"
    );
    let changed = original.changes[0]
        .content()
        .replace("stock < 8", "stock < 2");
    assert!(
        nika_onboard::compile::program_records::plan(
            p.s.programs.as_ref(),
            Place::Proposal(&p.id.to_string()),
            &changed
        )
        .is_none()
    );
    p.nothing_ran();
}

/// Historical execution does not become a current check or result on reopen.
#[test]
fn saved_semantic_work_reopens_without_claiming_it_never_ran_or_is_still_checked() {
    let peer = Peer::start(vec![
        (200, response(&graph())),
        (200, response(&fills())),
        (200, response(JUDGE_APPROVES)),
    ]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("history");
    std::fs::write(dir.path().join("inventory.json"), ROWS).expect("inventory");
    let mut first = open(dir.path());
    first.enable_history(home.path()).expect("history");
    assert!(matches!(first.turn(WORK), TurnOutcome::Proposal { .. }));
    assert!(matches!(first.consent("yes"), TurnOutcome::Facts(_)));
    let _ = first.observe_run(0, None); // synthetic host observation, no execution
    assert!(first.status_line().contains("run succeeded"));
    drop(first);
    let mut later = open(dir.path());
    later.enable_history(home.path()).expect("resume");
    let _ = later.restore_state();
    assert_eq!(
        later.kept_run().expect("run").expect("readable").exit,
        Some(0)
    );
    assert_eq!(
        later.status_line(),
        "Saved · no current Run result · `reorder.nika`"
    );
    assert_eq!(later.last_check_clean, None);
    assert_eq!(later.last_run, None);
    assert_eq!(peer.bodies().len(), 3, "reopening calls nobody");
    assert!(!dir.path().join("reorder.json").exists());
}
