// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source compatibility of `nika_session::{change, consent, outcome, review}` and of the
//! session's root re-exports after the four modules moved to the size-cap member below the
//! session, `nika_session_change` (2026-10-06 · ADR-144). This file is an external consumer:
//! it compiles against the session paths, and they name the very same items — the same types
//! (a value of one path IS a value of the other), the same functions (their signatures carry
//! the member's types) and the same constants — never copies. The one observable difference
//! is a type's name at run time, which now names the member.

use std::any::type_name;
use std::path::Path;

use nika_session::change::{ProjectChange, ProjectChangeSet, Witness, WorkflowAudit};
use nika_session::outcome::{ProposalId, RefusalClass};
use nika_session_change as member;

/// Each function of the session path, typed with the member's types: a session-side copy of
/// these items could not be assigned here.
const CHECK_ON_DISK: fn(&Path, &Path) -> member::change::WorkflowAudit =
    nika_session::change::check_on_disk;
const GATE_TASKS: fn(&str) -> Vec<String> = nika_session::review::gate_tasks;
const PROPOSAL_OF: fn(&str) -> member::outcome::ProposalId = nika_session::ProposalId::of;

#[test]
fn the_session_paths_name_the_members_items() {
    // One type under the module path, the root path and the member's path.
    let at_root: nika_session::ProjectChangeSet = ProjectChangeSet::project_change(
        Path::new("/r"),
        "goal",
        ProjectChange::CreateProjectFile {
            content: "nika: p\n".to_owned(),
        },
    );
    let owned: member::change::ProjectChangeSet = at_root.clone();
    assert_eq!(at_root, owned);
    let class: member::outcome::RefusalClass = RefusalClass::WrongState;
    assert_eq!(class, nika_session::RefusalClass::WrongState);
    // The functions answer as the member's, under every path.
    let preview = owned.preview();
    assert_eq!(PROPOSAL_OF(&preview), ProposalId::of(&preview));
    assert_eq!(
        nika_session::Witness::of(b"bytes"),
        member::change::Witness::of(b"bytes")
    );
    assert_eq!(Witness::of(b"bytes").short().len(), 8);
    let audit: WorkflowAudit = CHECK_ON_DISK(Path::new("/nonexistent-root"), Path::new("w.nika"));
    assert!(!audit.clean, "an unreadable workflow is never clean");
    assert_eq!(GATE_TASKS("not a workflow"), Vec::<String>::new());
    assert_eq!(
        nika_session::consent::CONSENTS_FILE,
        member::consent::CONSENTS_FILE
    );
    assert_eq!(
        nika_session::review::WORKFLOWS_DIR,
        member::review::WORKFLOWS_DIR
    );
}

/// What changed for a consumer that looks at metadata: a type's run-time name names the
/// member (the same name under every path, and no longer the session's).
#[test]
fn only_the_run_time_type_name_names_the_member() {
    for (session, owner) in [
        (
            type_name::<nika_session::ProjectChangeSet>(),
            type_name::<member::change::ProjectChangeSet>(),
        ),
        (
            type_name::<nika_session::ProposalId>(),
            type_name::<member::outcome::ProposalId>(),
        ),
        (
            type_name::<nika_session::ConsentRecord>(),
            type_name::<member::consent::ConsentRecord>(),
        ),
    ] {
        assert_eq!(session, owner);
        assert!(owner.starts_with("nika_session_change::"), "{owner}");
    }
}
