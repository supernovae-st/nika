// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

#[test]
fn reopened_host_context_is_not_a_you_bubble_even_with_identical_human_words() {
    let turns = [
        ("save & run".to_owned(), "literal human input".to_owned()),
        ("save & run".to_owned(), "(run requested)".to_owned()),
        ("(run)".to_owned(), "literal human input".to_owned()),
        ("(run)".to_owned(), "run observed".to_owned()),
    ];
    let projected = said(&earlier(
        &turns,
        |index| [true, false, true, false][index],
        (Voice::default(), false),
    ));
    assert_eq!(
        &projected[1..],
        &[
            (Kind::Human, "save & run".to_owned()),
            (Kind::Reply, "literal human input".to_owned()),
            (Kind::Notice, "earlier context · save & run".to_owned()),
            (Kind::Reply, "(run requested)".to_owned()),
            (Kind::Human, "(run)".to_owned()),
            (Kind::Reply, "literal human input".to_owned()),
            (Kind::Notice, "earlier context · (run)".to_owned()),
            (Kind::Reply, "run observed".to_owned()),
        ]
    );
}

/// One notice says a reopened conversation was kept (live gauntlet 6, r02: one card said the
/// recovery three times, in English and French): under the Session's notice that it was
/// restored, the earlier turns follow with no header of the TUI's; without one, the TUI's header
/// says what they are. The Session's own notes on a recovery repeat its notice and are no turn
/// of the conversation: they are not repainted, a person's line that reads the same is.
#[test]
fn a_reopened_conversation_says_once_that_it_was_kept() {
    let note = "[An earlier model request has unknown API usage or billing. Your messages are kept; \
                nothing was replayed.]";
    let turns = [
        (
            "bonjour".to_owned(),
            "Bonjour, que voulez-vous automatiser ?".to_owned(),
        ),
        ("(recovery)".to_owned(), note.to_owned()),
        (
            "(recovery)".to_owned(),
            "a line the person typed".to_owned(),
        ),
    ];
    let human = |index: usize| [true, false, true][index];
    let restored = said(&earlier(&turns, human, (Voice::default(), true)));
    assert_eq!(
        restored,
        [
            (Kind::Human, "bonjour".to_owned()),
            (
                Kind::Reply,
                "Bonjour, que voulez-vous automatiser ?".to_owned()
            ),
            (Kind::Human, "(recovery)".to_owned()),
            (Kind::Reply, "a line the person typed".to_owned()),
        ],
        "no header, no recovery note"
    );
    let unrestored = said(&earlier(&turns, human, (Voice::default(), false)));
    assert_eq!(
        unrestored.first(),
        Some(&(
            Kind::Notice,
            "earlier in this conversation · kept in your history · nothing is replayed".to_owned()
        ))
    );
    assert!(
        unrestored.iter().all(|(_, words)| words != note),
        "{unrestored:?}"
    );
    let only_notes = [("(recovery)".to_owned(), note.to_owned())];
    assert!(earlier(&only_notes, |_| false, (Voice::default(), false)).is_empty());
}

/// Reopened over the same home, a conversation reads the Session's notice that it was restored
/// and no header of the TUI's beside it; the earlier turn follows (live gauntlet 6, r02).
#[test]
fn a_reopened_conversation_reads_the_sessions_notice_alone() {
    let room = Room::new("reopened-once");
    let home = Room::new("reopened-once-home");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let open = || {
        let (census, pref) = super::super::intelligence::fixture();
        let mut live = Live::new(
            room.0.clone(),
            census,
            Some(pref),
            Some(home.0.clone()),
            Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
            runners(),
        );
        let opened = live.open();
        (live, opened)
    };
    let (mut first, _) = open();
    let _ = first.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    drop(first);
    let (_, reopened) = open();
    let said = said(&reopened);
    // The Session says it in the restored language: the earlier line was French.
    let notices = (said.iter())
        .filter(|(kind, words)| *kind == Kind::Notice && words.contains("conversation reprise"))
        .count();
    assert_eq!(notices, 1, "{said:#?}");
    // The earlier line was French: the TUI's header would read in French.
    let header = |words: &str| {
        words.contains("earlier in this conversation")
            || words.contains("plus tôt dans cette conversation")
    };
    assert!(!(said.iter()).any(|(_, words)| header(words)), "{said:#?}");
    assert!(
        (said.iter()).any(|(kind, words)| *kind == Kind::Human && words.contains("notes/brief.md")),
        "the earlier turn follows: {said:#?}"
    );
}
