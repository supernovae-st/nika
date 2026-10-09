// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opening with a selection: the census's own words hold for this conversation, the operator's
//! kept choice is neither read nor written, words the census does not read open nothing, and
//! the conversation's history resumes the choice on a later open without a selection.

use super::*;
use crate::host::tests::world;

fn selected(runtime: &SessionRuntime) -> (String, &'static str) {
    let selected = (runtime.work().intelligence.and_then(|i| i.selected)).expect("a selection");
    (selected.kind, selected.scope)
}

#[test]
fn an_opener_selection_holds_for_this_conversation_and_writes_nothing() {
    let root = world();
    let home = tempfile::tempdir().expect("home");
    let local = IntelligenceKind::Local {
        provider: "ollama".to_owned(),
    };
    UserIntelligencePreference::new(local, None)
        .save(home.path())
        .expect("the operator's kept choice");
    let kept = UserIntelligencePreference::path_under(home.path());
    let before = std::fs::read(&kept).expect("the operator's bytes");
    let (runtime, _) = open_bare(root.path(), Some(home.path()), None, Some("4")).expect("opened");
    assert_eq!(selected(&runtime), ("none".to_owned(), "conversation"));
    drop(runtime);
    let refused = open_bare(root.path(), Some(home.path()), None, Some("9"));
    assert!(
        matches!(&refused, Err(why) if why.contains("not a choice")),
        "words the census does not read open nothing"
    );
    let (resumed, _) = open_bare(root.path(), Some(home.path()), None, None).expect("opened");
    assert_eq!(
        selected(&resumed),
        ("none".to_owned(), "conversation"),
        "the conversation's own choice resumes over the operator's"
    );
    drop(resumed);
    assert_eq!(std::fs::read(&kept).expect("the operator's bytes"), before);
}

/// The machine door's command line: exactly `session --json`, the display flags the front door
/// ignores, at most one `--intelligence` with its words; anything else is not this door, and a
/// misplaced `--fix` stays the front door's to teach.
#[test]
fn the_machine_door_reads_its_one_selection_and_nothing_else() {
    let argv = |words: &[&str]| -> Vec<std::ffi::OsString> {
        words.iter().map(std::ffi::OsString::from).collect()
    };
    let named = |words: &[&str]| machine_selection(&argv(words));
    assert_eq!(named(&["session", "--json"]), Some(None));
    assert_eq!(named(&["--json", "session", "--ascii"]), Some(None));
    assert_eq!(
        named(&["session", "--color", "never", "--json"]),
        Some(None)
    );
    assert_eq!(
        named(&["session", "--json", "--intelligence", "1 acp:claude-code"]),
        Some(Some("1 acp:claude-code".to_owned()))
    );
    assert_eq!(
        named(&["session", "--json", "--intelligence=4"]),
        Some(Some("4".to_owned()))
    );
    for other in [
        &["session"][..],
        &["run", "--json"],
        &["session", "--json", "--intelligence"],
        &[
            "session",
            "--json",
            "--intelligence",
            "1",
            "--intelligence",
            "4",
        ],
        &["session", "--json", "--verbose"],
        &["session", "--json", "--fix"],
        &["session", "session", "--json"],
    ] {
        assert_eq!(named(other), None, "{other:?}");
    }
}
