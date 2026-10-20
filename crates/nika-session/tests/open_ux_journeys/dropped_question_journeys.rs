// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A cancel word at a question the author asked drops it (gauntlet wave 3).

use crate::scenarios::{DROP, SIMPLE};
use crate::{outcome_kind, run, waiting};

/// « annuler » at the author's question closes it with no binding and no model call; the author
/// reads that the person dropped it, with the person's next line, and asking it again is refused.
#[test]
fn a_cancel_word_drops_the_authors_question_with_no_call() {
    let journey = run("question_dropped");
    assert_eq!(
        waiting(journey.step(0)),
        "question",
        "{:#}",
        journey.step(0)
    );
    let dropped = journey.step(1);
    assert_eq!(dropped["line"], DROP);
    assert_eq!(outcome_kind(dropped), "facts", "{dropped:#}");
    let said = dropped["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        said.starts_with("question dropped · nothing was bound"),
        "{said}"
    );
    assert_eq!(waiting(dropped), "free", "{dropped:#}");
    assert_eq!(
        journey.agent.len(),
        3,
        "no call for « annuler »: the next line's ask, then the reply"
    );
    let next = journey.agent[1].text();
    // The person's words end the asking call, with Nika's one fact beside them.
    let answered = format!("The person answered, cited as u2:\n{DROP}\n\nNika (not the person):\n");
    assert!(next.contains(&answered), "{next}");
    assert!(
        next.contains("The person dropped your question `format`"),
        "{next}"
    );
    assert!(next.contains(&format!("{SIMPLE}\n(cited as u3)")), "{next}");
    let again = journey.agent[2].last();
    assert!(again.contains(r#""reason":"dropped""#), "{again}");
    assert!(again.contains(r#""asked":[]"#), "{again}");
    assert_eq!(
        outcome_kind(journey.step(2)),
        "reply",
        "{:#}",
        journey.step(2)
    );
}

/// A cancel word that one of the question's offers holds answers it: the author reads the pick.
#[test]
fn a_cancel_word_the_question_offers_answers_it() {
    let journey = run("cancel_offered");
    let picked = journey.step(1);
    assert_eq!(picked["line"], DROP);
    let said = picked["outcome"]["text"].as_str().unwrap_or_default();
    assert!(!said.contains("question dropped"), "{picked:#}");
    assert_eq!(journey.agent.len(), 2, "the author reads the pick");
    let read = journey.agent[1].last();
    assert!(read.contains("annuler"), "{read}");
}
