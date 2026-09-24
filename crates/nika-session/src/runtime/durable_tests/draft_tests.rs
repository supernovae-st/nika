// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal kept across a close: evidence without authority, its deterministic
//! re-proposal on the same fixtures as the durable-session tests, and the record's schema
//! behaviour (old, newer, malformed, redacted, truncated, altered).

use super::*;
use crate::runtime::draft::{DraftKind, Restored};

/// The draft the process-level fixture leaves: `COPY` proposed, then the process ends.
fn closed_with_a_draft(root: &Path, home: &Path) -> (crate::outcome::ProposalId, String) {
    let (mut first, _) = open(root, &[ANSWER]);
    first.enable_history(home).expect("fresh history");
    let id = proposed(first.turn(COPY));
    let bytes = first
        .pending
        .as_ref()
        .and_then(|set| set.changes.first())
        .expect("the proposed file")
        .content()
        .to_owned();
    drop(first);
    (id, bytes)
}

fn kept(runtime: &SessionRuntime) -> crate::runtime::draft::PendingDraft {
    match &runtime.restored_draft {
        Some(Restored::Usable { draft, .. }) => draft.clone(),
        other => panic!("expected a usable kept draft, got {other:?}"),
    }
}

/// A proposal still pending when the process ends is kept as evidence, never as authority:
/// the reopened session names it and its file and keeps its exact bytes, but grants nothing
/// and lands nothing.
#[test]
fn a_pending_proposal_is_kept_as_evidence_across_a_close_without_authority() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (id, bytes) = closed_with_a_draft(root.path(), home.path());
    let (mut resumed, seen) = open(root.path(), &[ANSWER]);
    let notice = resumed
        .enable_history(home.path())
        .expect("resume")
        .expect("a restore notice");
    assert!(
        notice.contains(&format!("restored proposal {id}")),
        "{notice}"
    );
    assert!(notice.contains(LANDED), "{notice}");
    assert!(notice.contains("not applied"), "{notice}");
    assert!(!notice.contains("reconfirm"), "{notice}");
    let draft = kept(&resumed);
    assert_eq!(draft.proposal, id.to_string());
    let file = draft.files.first().expect("its file");
    assert_eq!(file.path, LANDED);
    assert_eq!(file.kind, DraftKind::CreateWorkflow);
    assert_eq!(file.before, None);
    assert_eq!(file.witness, crate::change::Witness::of(bytes.as_bytes()).0);
    assert_eq!(file.text.as_deref(), Some(bytes.as_str()));
    assert!(resumed.pending_proposal().is_none());
    assert_eq!(
        refused(resumed.consent_to(&id, "yes")).class,
        RefusalClass::WrongState
    );
    assert_eq!(
        refused(resumed.consent("yes")).class,
        RefusalClass::WrongState
    );
    assert!(!root.path().join(LANDED).exists());
    assert!(seen.lock().expect("record").is_empty());
}

/// The kept draft rides every later record, so a repeated close keeps it too, until a new
/// proposal replaces it.
#[test]
fn a_kept_draft_survives_a_repeated_close_until_a_new_proposal_replaces_it() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (id, _) = closed_with_a_draft(root.path(), home.path());
    let (mut second, _) = open(root.path(), &[ANSWER]);
    second.enable_history(home.path()).expect("resume");
    let _ = second.turn(GOAL);
    drop(second);
    let (mut third, _) = open(root.path(), &[ANSWER]);
    let notice = third
        .enable_history(home.path())
        .expect("resume")
        .expect("a restore notice");
    assert!(
        notice.contains(&format!("restored proposal {id}")),
        "{notice}"
    );
    let _ = proposed(third.turn(COPY));
    assert!(
        third.restored_draft.is_none(),
        "a new proposal replaces the kept draft"
    );
    assert!(third.pending_proposal().is_some());
}

/// The typed act proposes the kept draft again without a model call; nothing lands before a
/// fresh consent, which lands exactly the kept bytes, and the draft is then spent.
#[test]
fn a_kept_draft_is_proposed_again_and_lands_only_on_a_fresh_consent() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (_, bytes) = closed_with_a_draft(root.path(), home.path());
    let (mut resumed, seen) = open(root.path(), &[ANSWER]);
    resumed.enable_history(home.path()).expect("resume");
    assert!(resumed.restored_draft_id().is_some());
    let _ = proposed(resumed.repropose_restored_draft());
    assert!(resumed.pending_proposal().is_some());
    assert!(
        resumed.restored_draft_id().is_none(),
        "the fresh proposal spends the draft"
    );
    assert!(
        !root.path().join(LANDED).exists(),
        "nothing lands before consent"
    );
    let _ = resumed.consent("yes");
    assert_eq!(
        std::fs::read_to_string(root.path().join(LANDED)).expect("landed"),
        bytes
    );
    assert!(seen.lock().expect("record").is_empty(), "no model call");
}

/// A destination that appeared since the draft was proposed refuses the rebuild: the draft
/// created a file, and nothing may silently overwrite one. The draft stays kept.
#[test]
fn a_destination_that_appeared_since_the_draft_refuses_the_rebuild() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let _ = closed_with_a_draft(root.path(), home.path());
    let (mut resumed, _) = open(root.path(), &[ANSWER]);
    resumed.enable_history(home.path()).expect("resume");
    std::fs::write(root.path().join(LANDED), "nika: someone-else\n").expect("appeared");
    let why = refused(resumed.repropose_restored_draft());
    assert_eq!(why.class, RefusalClass::NotAllowed);
    assert!(why.text.contains("exists now"), "{}", why.text);
    assert!(resumed.pending_proposal().is_none());
    assert!(resumed.restored_draft.is_some(), "the draft stays kept");
    assert!(
        resumed.restored_draft_id().is_none(),
        "not available while its destination exists"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join(LANDED)).expect("untouched"),
        "nika: someone-else\n"
    );
}

/// Kept text that is not the proposed bytes (redacted or altered), or text that was not kept
/// at all, refuses the rebuild instead of changing the user's code.
#[test]
fn redacted_altered_or_missing_kept_text_refuses_the_rebuild() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let _ = closed_with_a_draft(root.path(), home.path());
    let (mut resumed, _) = open(root.path(), &[ANSWER]);
    resumed.enable_history(home.path()).expect("resume");
    let Some(Restored::Usable { draft, raw }) = resumed.restored_draft.take() else {
        panic!("a usable kept draft");
    };
    let mut redacted = draft.clone();
    if let Some(file) = redacted.files.first_mut() {
        file.text = Some("nika: [REDACTED]\n".to_owned());
    }
    resumed.restored_draft = Some(Restored::Usable {
        draft: redacted,
        raw: raw.clone(),
    });
    let why = refused(resumed.repropose_restored_draft());
    assert_eq!(why.class, RefusalClass::NotAllowed);
    assert!(
        why.text.contains("differs from the proposed bytes"),
        "{}",
        why.text
    );
    let mut truncated = draft;
    if let Some(file) = truncated.files.first_mut() {
        file.text = None;
    }
    resumed.restored_draft = Some(Restored::Usable {
        draft: truncated,
        raw,
    });
    let why = refused(resumed.repropose_restored_draft());
    assert!(why.text.contains("was not kept"), "{}", why.text);
    assert!(resumed.pending_proposal().is_none());
    assert!(!root.path().join(LANDED).exists());
}

/// Records this engine cannot read (a newer schema, a malformed value, no schema) are kept
/// byte for byte, ride the next record unchanged and are never used; a record without a draft
/// restores as before and has nothing to propose.
#[test]
fn unreadable_draft_records_are_kept_unread_and_old_records_restore_as_before() {
    let newer = serde_json::json!({"schema": 2, "proposal": "x", "future": true});
    let restored = Restored::from_raw(newer.clone());
    assert!(
        matches!(&restored, Restored::Unreadable { why, .. } if why.contains("schema 2")),
        "{restored:?}"
    );
    assert_eq!(restored.raw(), &newer);
    let malformed = serde_json::json!({"schema": 1, "proposal": 7});
    assert!(matches!(
        Restored::from_raw(malformed),
        Restored::Unreadable { .. }
    ));
    let unversioned = serde_json::json!({"proposal": "x", "goal": "g", "files": []});
    assert!(matches!(
        Restored::from_raw(unversioned),
        Restored::Unreadable { .. }
    ));
    let saved = serde_json::to_value(crate::runtime::history::Saved::default()).expect("saved");
    assert!(saved.get("pending").is_none(), "no draft, no new bytes");
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path(), &[ANSWER]);
    first.enable_history(home.path()).expect("fresh history");
    let _ = first.turn(GOAL);
    drop(first);
    let (mut resumed, _) = open(root.path(), &[ANSWER]);
    resumed.enable_history(home.path()).expect("resume");
    assert!(resumed.restored_draft.is_none());
    assert_eq!(
        refused(resumed.repropose_restored_draft()).class,
        RefusalClass::WrongState
    );
    resumed.restored_draft = Some(Restored::from_raw(newer.clone()));
    assert_eq!(
        refused(resumed.repropose_restored_draft()).class,
        RefusalClass::NotAllowed
    );
    assert!(resumed.restored_draft_id().is_none());
    drop(resumed);
    let (mut third, _) = open(root.path(), &[ANSWER]);
    third.enable_history(home.path()).expect("resume");
    assert_eq!(
        third.restored_draft.as_ref().map(Restored::raw),
        Some(&newer)
    );
}

/// The legacy monetary marker keeps its exact bytes in the records, but no user-facing line
/// promises a reconfirmation any more.
#[test]
fn the_legacy_monetary_marker_keeps_its_bytes_but_no_longer_promises_a_reconfirmation() {
    assert_eq!(
        crate::runtime::inference::RECONFIRM,
        "monetary constraint recorded; restart requires explicit ceiling reconfirmation"
    );
    let shown =
        crate::runtime::recovery::decision_for_display(crate::runtime::inference::RECONFIRM);
    assert!(!shown.contains("reconfirm"), "{shown}");
    assert!(shown.contains("blocked"), "{shown}");
    assert_eq!(
        crate::runtime::recovery::decision_for_display("applied proposal 0123456789ab"),
        "applied proposal 0123456789ab"
    );
}

/// An update-kind draft through the real persistence: an existing workflow, a pending set
/// the change primitive builds over it (an update over its witness), one recorded boundary
/// that holds the proposal, then the process ends.
fn closed_with_an_update(root: &Path, home: &Path, before: &str, after: &str) {
    std::fs::write(root.join(LANDED), before).expect("the base");
    let (mut first, _) = open(root, &[ANSWER]);
    first.enable_history(home).expect("fresh history");
    let set = crate::change::ProjectChangeSet::workflow_at(
        root,
        "update the kept workflow",
        LANDED,
        after.to_owned(),
    )
    .expect("an update set");
    assert!(matches!(
        set.changes.first(),
        Some(crate::change::ProjectChange::UpdateWorkflow { .. })
    ));
    first.pending = Some(set);
    assert!(matches!(first.consent("/show"), TurnOutcome::Held { .. }));
    drop(first);
}

/// Every path and byte under the project, symlinks as links: a failed recovery changes none.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("readable") {
            let path = entry.expect("entry").path();
            let kind = std::fs::symlink_metadata(&path)
                .expect("metadata")
                .file_type();
            let name = path
                .strip_prefix(root)
                .expect("inside")
                .display()
                .to_string();
            if kind.is_symlink() {
                let target = std::fs::read_link(&path).expect("link");
                out.push((name, target.display().to_string().into_bytes()));
            } else if kind.is_dir() {
                stack.push(path);
            } else {
                out.push((name, std::fs::read(&path).expect("bytes")));
            }
        }
    }
    out.sort();
    out
}

const BASE: &str = "nika: kept-base\ntasks: {}\n";
const UPDATED: &str = "nika: kept-update\ntasks: {}\n";

fn resumed_with_an_update(root: &Path, home: &Path) -> (SessionRuntime, Seen) {
    closed_with_an_update(root, home, BASE, UPDATED);
    let (mut resumed, seen) = open(root, &[ANSWER]);
    let notice = resumed
        .enable_history(home)
        .expect("resume")
        .expect("a restore notice");
    assert!(notice.contains("can be proposed again"), "{notice}");
    let draft = kept(&resumed);
    let file = draft.files.first().expect("its file");
    assert_eq!(file.kind, DraftKind::UpdateWorkflow);
    assert_eq!(
        file.before.as_deref(),
        Some(crate::change::Witness::of(BASE.as_bytes()).0.as_str())
    );
    assert!(
        resumed.restored_draft_id().is_some(),
        "available over the unchanged base"
    );
    assert!(
        resumed.pending_proposal().is_none(),
        "no restored authority"
    );
    assert_eq!(
        refused(resumed.consent("yes")).class,
        RefusalClass::WrongState
    );
    (resumed, seen)
}

/// An unchanged update is proposed again as a fresh preview; the base stays until an explicit
/// consent, which lands exactly the kept bytes. No model is called.
#[test]
fn an_unchanged_update_is_proposed_again_and_lands_only_on_an_explicit_consent() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut resumed, seen) = resumed_with_an_update(root.path(), home.path());
    let preview = match resumed.repropose_restored_draft() {
        TurnOutcome::Proposal { preview, .. } => preview,
        other => panic!("expected a fresh proposal, got {other:?}"),
    };
    assert!(preview.contains(LANDED), "{preview}");
    let read = || std::fs::read_to_string(root.path().join(LANDED)).expect("the workflow");
    assert_eq!(read(), BASE, "nothing lands before consent");
    let _ = resumed.consent("yes");
    assert_eq!(read(), UPDATED);
    assert!(seen.lock().expect("record").is_empty(), "no model call");
}

/// A base changed since the close refuses the rebuild with the exact diagnostic, and the
/// failed recovery touches nothing: no file, no pending proposal, the draft still kept.
#[test]
fn an_update_whose_base_changed_is_refused_without_side_effects() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut resumed, _) = resumed_with_an_update(root.path(), home.path());
    std::fs::write(root.path().join(LANDED), "nika: someone-else\n").expect("changed");
    let before = tree(root.path());
    let why = refused(resumed.repropose_restored_draft());
    assert_eq!(why.class, RefusalClass::NotAllowed);
    assert!(
        why.text
            .contains(&format!("`{LANDED}` changed since the draft was proposed")),
        "{}",
        why.text
    );
    assert_eq!(tree(root.path()), before);
    assert!(resumed.pending_proposal().is_none());
    assert!(resumed.restored_draft.is_some(), "the draft stays kept");
    assert!(
        resumed.restored_draft_id().is_none(),
        "not available over a changed base"
    );
}

/// A base removed since the close refuses the rebuild: the draft updated a file that no
/// longer exists, and nothing creates it instead.
#[test]
fn an_update_whose_base_was_removed_is_refused_without_side_effects() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut resumed, _) = resumed_with_an_update(root.path(), home.path());
    std::fs::remove_file(root.path().join(LANDED)).expect("removed");
    let before = tree(root.path());
    let why = refused(resumed.repropose_restored_draft());
    assert_eq!(why.class, RefusalClass::NotAllowed);
    assert!(
        why.text.contains(&format!(
            "`{LANDED}` no longer exists; the draft updated it"
        )),
        "{}",
        why.text
    );
    assert_eq!(tree(root.path()), before);
    assert!(!root.path().join(LANDED).exists());
    assert!(resumed.pending_proposal().is_none());
    assert!(resumed.restored_draft.is_some(), "the draft stays kept");
    assert!(resumed.restored_draft_id().is_none());
}

/// A base replaced by a symlink is never witnessed through it: the no-follow witness refuses,
/// the link and its target stay untouched.
#[cfg(unix)]
#[test]
fn an_update_whose_base_is_now_a_symlink_is_refused_without_following_it() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let (mut resumed, _) = resumed_with_an_update(root.path(), home.path());
    std::fs::remove_file(root.path().join(LANDED)).expect("removed");
    std::os::unix::fs::symlink("notes/brief.md", root.path().join(LANDED)).expect("symlink");
    let before = tree(root.path());
    let why = refused(resumed.repropose_restored_draft());
    assert_eq!(why.class, RefusalClass::NotAllowed);
    assert!(
        why.text.contains("exists but cannot be witnessed"),
        "{}",
        why.text
    );
    assert_eq!(tree(root.path()), before);
    assert!(
        std::fs::symlink_metadata(root.path().join(LANDED))
            .expect("the link")
            .file_type()
            .is_symlink()
    );
    assert!(resumed.pending_proposal().is_none());
    assert!(resumed.restored_draft_id().is_none());
}

/// A secret in the proposed bytes goes through the real capture, serialization and load: the
/// history on disk never holds it, the kept text is the redacted one while its witness is the
/// original bytes', the notice promises no re-proposal, and the rebuild refuses without
/// changing the user's file.
#[test]
fn a_secret_in_a_kept_draft_is_redacted_on_disk_and_the_rebuild_refuses() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let secret = "sk-fixturekey0123456789abcdef";
    let after = format!("nika: kept-with-a-key\nconst:\n  hint: \"{secret}\"\ntasks: {{}}\n");
    closed_with_an_update(root.path(), home.path(), BASE, &after);
    let dir = history_dir(home.path(), root.path());
    for entry in std::fs::read_dir(&dir).expect("history") {
        let path = entry.expect("entry").path();
        if path.is_file() {
            let bytes = std::fs::read(&path).expect("record");
            assert!(
                !String::from_utf8_lossy(&bytes).contains(secret),
                "{} holds the secret",
                path.display()
            );
        }
    }
    let (mut resumed, _) = open(root.path(), &[ANSWER]);
    let notice = resumed
        .enable_history(home.path())
        .expect("resume")
        .expect("a restore notice");
    assert!(notice.contains("cannot be proposed again"), "{notice}");
    assert!(
        !notice.contains("can be proposed again without"),
        "{notice}"
    );
    assert!(resumed.restored_draft_id().is_none(), "never available");
    let draft = kept(&resumed);
    let file = draft.files.first().expect("its file");
    assert!(
        file.text
            .as_deref()
            .is_some_and(|t| t.contains("[redacted]") && !t.contains(secret)),
        "{file:?}"
    );
    assert_eq!(file.witness, crate::change::Witness::of(after.as_bytes()).0);
    let before = tree(root.path());
    let why = refused(resumed.repropose_restored_draft());
    assert!(
        why.text.contains("differs from the proposed bytes"),
        "{}",
        why.text
    );
    assert_eq!(tree(root.path()), before);
    assert_eq!(
        std::fs::read_to_string(root.path().join(LANDED)).expect("the base"),
        BASE
    );
}
