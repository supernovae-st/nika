// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Text helpers the compiler and the onboarding surface share: the routing stopwords and
//! the banner reader. Re-homed here at the member split so the member never depends back
//! on `nika-onboard` (ADR-137 · the ADR-115 law).

/// Function words + Nika envelope keywords that carry zero routing signal —
/// stripped from the query so an all-boilerplate `--from` (`the` · `workflow`
/// · `template`) lists the set instead of spuriously routing (every template
/// shares `workflow:`/`tasks:`/… so those terms separate nothing).
pub const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "and",
    "the",
    "to",
    "of",
    "in",
    "on",
    "for",
    "with",
    "that",
    "this",
    "then",
    "than",
    "into",
    "from",
    "by",
    "as",
    "at",
    "is",
    "are",
    "be",
    "it",
    "its",
    "or",
    "i",
    "me",
    "my",
    "we",
    "you",
    "no",
    "such",
    "nika",
    "workflow",
    "model",
    "vars",
    "tasks",
    "id",
    "template",
    "slot",
    "kebab",
    "case",
    "do",
    "stuff",
    "thing",
    "things",
    "something",
];

/// The banner lines of a gallery entry: the leading `#` comment block, stripped.
#[must_use]
pub fn banner_lines(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in body.lines() {
        let Some(rest) = line.strip_prefix('#') else {
            if line.trim().is_empty() {
                continue;
            }
            break; // the first YAML line closes the banner
        };
        let rest = rest.trim();
        if rest.is_empty()
            || rest.starts_with("SPDX-License-Identifier")
            || rest.starts_with("yaml-language-server")
            || rest.starts_with("Copyright")
        {
            continue;
        }
        out.push(rest);
    }
    out
}

/// A banner segment that only labels (a bare word or a `T<n>` tag), never a sentence.
#[must_use]
pub fn is_label(seg: &str) -> bool {
    !seg.contains(' ')
        || seg
            .split_once(' ')
            .is_some_and(|(head, _)| head.len() == 2 && head.starts_with('T'))
}

/// One line for a menu row — the entry's sentence, without the labels
/// that precede it (the row prints the name and the facet itself, so
/// repeating them would spend the row's width on nothing).
///
/// The labels are a PREFIX, and only a prefix: `human-gated-ship`'s own
/// title carries a `·` inside its sentence, so taking the trail after
/// the LAST separator loses two thirds of it and lands on an ASCII
/// diagram. Drop the leading labels, keep everything after them.
///
/// A title that is labels all the way down is a classification line
/// (`showcase · T2 chain · finance / freelance`) — the sentence is then
/// the paragraph under it.
#[must_use]
pub fn banner_sentence(body: &str) -> Option<String> {
    let lines = banner_lines(body);
    let first = lines.first()?;
    let rest: Vec<&str> = first.split(" · ").skip_while(|seg| is_label(seg)).collect();
    let trail = rest.join(" · ");
    if trail.split_whitespace().count() >= 4 {
        return Some(trail);
    }
    lines
        .get(1)
        .map(|l| (*l).trim().to_owned())
        .filter(|l| !l.is_empty())
}
