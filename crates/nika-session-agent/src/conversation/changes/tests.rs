// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::{Value, json};

use super::super::{Citations, Conversation};
use super::{diff, first, stat, written};

const HN: &str = "https://news.ycombinator.com";
const TC: &str = "https://techcrunch.com";
const DIGEST: &str = "./news/digest.md";
const SAID: &str =
    "résume https://news.ycombinator.com et https://techcrunch.com dans ./news/digest.md";

fn citations() -> Citations {
    let mut out = Citations::default();
    out.record(1, Some(("u1".into(), SAID.into())));
    out
}

/// A value the person named in u1, in `role`.
fn named(value: &str, role: &str) -> Value {
    json!({"value": value, "kind": "named", "role": role, "message": "u1", "excerpt": value})
}

/// `source` written with `rows`, then proposed: what the proposal says changed, and whether its
/// bytes are the ones the person saw.
fn proposed(
    conversation: &mut Conversation,
    lines: &Citations,
    source: &str,
    rows: &Value,
) -> (Vec<String>, bool) {
    let rows = rows.as_array().cloned().unwrap_or_default();
    let texts = (source.to_owned(), "s".to_owned());
    let reply = conversation.write(lines, texts, (&rows, &[]), &mut |_| Ok(()));
    assert!(!reply.is_error, "{reply:?}");
    let shown = conversation.propose(lines, &json!({}), ("S1".into(), Vec::new()));
    assert_eq!(shown, Ok(None));
    (conversation.changed().to_vec(), conversation.unchanged())
}

#[test]
fn a_proposal_of_the_very_bytes_says_so_and_claims_no_change() {
    let (mut conversation, lines) = (Conversation::default(), citations());
    let source = format!("nika: digest\n# fetch {HN}\n# write {DIGEST}\n");
    let first = proposed(
        &mut conversation,
        &lines,
        &source,
        &json!([named(HN, "read_source")]),
    );
    assert_eq!(first, (Vec::new(), false), "the first proposal");
    // The author binds one more value of the same bytes (QA gauntlet q04: « model … added »).
    let rows = json!([named(HN, "read_source"), named(DIGEST, "output_path")]);
    let again = proposed(&mut conversation, &lines, &source, &rows);
    assert_eq!(again, (Vec::new(), true));
}

#[test]
fn a_change_is_the_values_the_bytes_gained_or_lost_with_the_diff_size() {
    let (mut conversation, lines) = (Conversation::default(), citations());
    let before = format!("nika: digest\n# fetch {HN}\n# write {DIGEST}\n");
    proposed(
        &mut conversation,
        &lines,
        &before,
        &json!([named(HN, "read_source")]),
    );
    let after = format!("nika: digest\n# fetch {HN} {TC}\n# write {DIGEST}\n");
    let rows = json!([named(HN, "read_source"), named(TC, "read_source")]);
    let changed = proposed(&mut conversation, &lines, &after, &rows);
    let said = vec![format!("source `{TC}` added"), "+1 −1 lines".to_owned()];
    assert_eq!(changed, (said, false));
}

#[test]
fn a_value_bound_anew_that_the_bytes_already_held_is_no_change_of_its_own() {
    let (mut conversation, lines) = (Conversation::default(), citations());
    let before = format!("nika: digest\n# fetch {HN}\n# write {DIGEST}\n");
    proposed(
        &mut conversation,
        &lines,
        &before,
        &json!([named(HN, "read_source")]),
    );
    let after = format!("{before}# a note\n");
    let rows = json!([named(HN, "read_source"), named(DIGEST, "output_path")]);
    let changed = proposed(&mut conversation, &lines, &after, &rows);
    let said = vec![
        "+1 −0 lines".to_owned(),
        "first added `# a note`".to_owned(),
    ];
    assert_eq!(changed, (said, false));
}

#[test]
fn the_diff_counts_lines_once_and_quotes_the_first_written_one() {
    let (added, removed) = diff("a\nb\nb\nc\n", "b\na\nd\n\nb\n");
    assert_eq!((added.clone(), removed.clone()), (vec!["d", ""], vec!["c"]));
    assert_eq!(stat(&added, &removed), "+2 −1 lines");
    assert_eq!(
        first("first added", &["", "  d  "]),
        Some("first added `d`".into())
    );
    let long = "x".repeat(70);
    let cut = format!("first removed `{}…`", "x".repeat(60));
    assert_eq!(first("first removed", &[long.as_str()]), Some(cut));
    assert_eq!(first("first added", &["", " "]), None);
    assert!(written("path: news/digest.md", DIGEST));
    assert!(!written("path: news/other.md", DIGEST));
    assert!(!written("anything", "./"));
}
