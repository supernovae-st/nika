// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

const SOURCE: &str = "tasks:\n  fetch:\n    url: a\n  keep:\n    expression: b\n";

fn pairs(edits: &[(&str, &str)]) -> Vec<(String, String)> {
    (edits.iter())
        .map(|(old, new)| ((*old).to_owned(), (*new).to_owned()))
        .collect()
}

#[test]
fn several_edits_land_as_one_revision_whatever_their_order() {
    let edits = pairs(&[("expression: b", "expression: c"), ("url: a", "url: z")]);
    let want = "tasks:\n  fetch:\n    url: z\n  keep:\n    expression: c\n";
    assert_eq!(edited(SOURCE, &edits), Ok(want.to_owned()));
}

#[test]
fn each_old_passage_is_read_in_the_original_not_in_an_earlier_edit() {
    // The second old passage is what the first edit writes: it is not in the original.
    let edits = pairs(&[("url: a", "url: b2"), ("url: b2", "url: c")]);
    let refused = edited(SOURCE, &edits);
    assert_eq!(
        refused,
        Err("edit 2: `old` must occur exactly once in the candidate; it occurs 0 times".to_owned())
    );
}

#[test]
fn overlapping_edits_are_refused_and_name_both() {
    let edits = pairs(&[("fetch:\n    url: a", "fetch:"), ("url: a\n  keep", "keep")]);
    let refused = edited(SOURCE, &edits);
    assert_eq!(
        refused,
        Err("edits 1 and 2 overlap: each old passage must stand apart".to_owned())
    );
}

#[test]
fn one_edit_keeps_the_one_passage_words() {
    let twice = edited("a a", &pairs(&[("a", "b")]));
    let none = edited(SOURCE, &pairs(&[("", "b")]));
    assert_eq!(
        twice,
        Err("`old` must occur exactly once in the candidate; it occurs 2 times".to_owned())
    );
    assert!(none.is_err(), "an empty old passage replaces nothing");
    assert!(edited(SOURCE, &[]).is_err(), "no edit, no revision");
}

/// A document with comments, an opaque block prompt, literal values, an explicit default and a
/// field left absent: an edit replaces only its passage, and every other byte stays as it was.
#[test]
fn an_edit_keeps_every_byte_it_does_not_replace() {
    let source = "# The morning digest, as the person reviewed it.\nnika: digest\nmodel: vllm/oux-author\ninputs:\n  limit: { type: integer, default: 5 }\ntasks:\n  fetch:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://news.ycombinator.com\", method: GET }  # one GET\n  summarize:\n    infer:\n      prompt: |\n        Summarize in 5 points at most, inventing nothing.\n        Keep the links as they are: ${{ tasks.fetch.output }}\n";
    let (old, new) = ("5 points at most", "7 points at most");
    let revised = edited(source, &pairs(&[(old, new)])).expect("one passage");
    let at = source.find(old).expect("the passage");
    assert_eq!(revised.get(..at), source.get(..at), "every byte before it");
    assert_eq!(
        revised.get(at + new.len()..),
        source.get(at + old.len()..),
        "every byte after it"
    );
    assert!(
        !revised.contains("max_tokens"),
        "a field left absent stays absent"
    );
}
