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
    let projected = said(&earlier(&turns, |index| [true, false, true, false][index]));
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
