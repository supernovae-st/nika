// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What one authoring request carries. Bounded inference sends a request of at most
//! [`BOUNDED_REQUEST_BYTES`](nika_providers::admission::BOUNDED_REQUEST_BYTES) and refuses a larger
//! one before any byte leaves, so an opening whose recalled references would take it past that
//! capacity would lose the whole call. Such an opening is fitted instead: from the end of each
//! kind's order (the door's relevance), the same share of every kind, references are shown by
//! their first line, said in their text and in their receipt with the whole entry's size and
//! digest, until the opening fits with room left for the rounds that follow it in the same
//! conversation. Only the knowledge pack's references are fitted: the request, the card, the
//! conventions, the callable contracts, the door's own instruction, the language and the
//! component list stay whole, and an opening that fits is sent unchanged.

use super::knowledge::{Reference, sha256};
use super::system_message;
use serde_json::{Value, json};

/// What one bounded request carries, in bytes of its JSON body.
const CAPACITY: usize = nika_providers::admission::BOUNDED_REQUEST_BYTES;
/// The share of it an opening leaves to the rounds that follow it in its conversation: the
/// answers, the findings and the repairs a refused candidate is sent back with.
const ROUNDS: usize = CAPACITY / 8;
/// What a request's JSON body carries besides its messages and its answer schema (the model,
/// the output bound, the roles, the keys and a provider's own schema sentence), generously.
pub(in crate::cognition) const FRAMING: usize = 4096;
/// The longest first line a reference shown by its first line keeps, in bytes.
const LINE: usize = 320;
/// What a reference shown by its first line says after it.
const SAID: &str = "(first line only: the references recalled for this request exceed what one request carries, and this one comes late in its kind's order; the whole entry stays in its release)";
/// Why a receipt's reference was shown by its first line.
const WHY: &str = "the opening exceeded what one bounded request carries";

/// The system message a door opens with — the card, the conventions, the callable contracts and
/// the references ([`system_message`]), then the door's own `instruction` — its references fitted
/// first ([`fit`]) beside the `opening` user message and the largest of the answer `schemas` its
/// conversation sends. The references' receipts in `sent` follow what is sent.
pub(in crate::cognition) fn system(
    (references, sent, from): (&mut [Reference], &mut [Value], usize),
    callables: &[Reference],
    (instruction, opening): (&str, &str),
    schemas: &[Value],
) -> String {
    let schema = (schemas.iter())
        .map(|schema| escaped(&schema.to_string()))
        .max()
        .unwrap_or_default();
    let fixed = escaped(&system_message(&[], callables))
        + escaped(instruction)
        + escaped(opening)
        + schema
        + FRAMING;
    fit(references, sent, from, fixed);
    let mut system = system_message(references, callables);
    system.push_str(instruction);
    system
}

/// The bytes a text takes inside a JSON string as the providers' bodies write it: a quote, a
/// backslash and a short control escape take two, any other control character six (`\u00XX`),
/// every other byte one.
pub(in crate::cognition) fn escaped(text: &str) -> usize {
    text.bytes()
        .map(|byte| match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 2,
            0x00..=0x1f => 6,
            _ => 1,
        })
        .sum()
}

/// The bytes one reference adds to the rendered references (`shelf::rendered`): its heading,
/// its text and the blank line after it, a skeleton between its fences.
fn weight(reference: &Reference) -> usize {
    let frame = if reference.kind == "skeleton" {
        "## \n```yaml\n\n```\n\n"
    } else {
        "## \n\n\n"
    };
    escaped(frame) + escaped(&reference.id) + escaped(&reference.text)
}

/// A reference's text shown by its first line (at most [`LINE`] bytes), with what that means.
fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim_end();
    let mut end = line.len().min(LINE);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n{SAID}", &line[..end])
}

/// Fit an opening's references under the capacity. `fixed` weighs everything else the opening
/// sends (its escaped messages without the references, its answer schema and [`FRAMING`]); the
/// references from `from` on are the knowledge pack's, in the door's order. While the opening
/// would exceed the capacity less the rounds' share, the reference furthest down its kind's order
/// as a share of that kind's length (the last of every kind first, then each kind's next in
/// proportion to its length; among equals, the kind that appears later first) is shown by its
/// first line when that is shorter, and its row in `sent` (the references' receipts, in the same
/// order) becomes the receipt of what is sent with the whole entry's bytes and digest. Returns
/// how many references were fitted.
pub(in crate::cognition) fn fit(
    references: &mut [Reference],
    sent: &mut [Value],
    from: usize,
    fixed: usize,
) -> usize {
    let budget = CAPACITY - ROUNDS;
    let mut total = fixed + references.iter().map(weight).sum::<usize>();
    if total <= budget {
        return 0;
    }
    // Each kind's references after `from`, in the door's order, the kinds as they first appear.
    let mut kinds: Vec<(&'static str, Vec<usize>)> = Vec::new();
    for (at, reference) in references.iter().enumerate().skip(from) {
        match kinds.iter_mut().find(|(kind, _)| *kind == reference.kind) {
            Some((_, list)) => list.push(at),
            None => kinds.push((reference.kind, vec![at])),
        }
    }
    // (index, place in its kind from 1, its kind's length, its kind's order), furthest first:
    // place/length compared as products, never as floats.
    let mut order: Vec<(usize, usize, usize, usize)> = (kinds.iter().enumerate())
        .flat_map(|(kind, (_, list))| {
            (list.iter().enumerate()).map(move |(place, at)| (*at, place + 1, list.len(), kind))
        })
        .collect();
    order.sort_by(|a, b| (b.1 * a.2).cmp(&(a.1 * b.2)).then(b.3.cmp(&a.3)));
    let mut fitted = 0;
    for (at, ..) in order {
        if total <= budget {
            break;
        }
        let whole = &references[at];
        let shown = Reference::new(whole.id.clone(), whole.kind, first_line(&whole.text));
        let (before, after) = (weight(whole), weight(&shown));
        if after >= before {
            continue;
        }
        if let Some(row) = sent.get_mut(at) {
            let mut receipt = shown.receipt();
            receipt["first_line_only"] = json!({
                "bytes": whole.text.len(),
                "sha256": sha256(&whole.text),
                "why": WHY,
            });
            *row = receipt;
        }
        total -= before - after;
        references[at] = shown;
        fitted += 1;
    }
    fitted
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn reference(kind: &'static str, id: &str, bytes: usize) -> Reference {
        Reference::new(
            id,
            kind,
            format!("{kind} {id} — its title\n{}", "x".repeat(bytes)),
        )
    }

    /// Two embedded references, then a pack of three patterns, three blocks and three examples.
    fn opening() -> Vec<Reference> {
        let mut references = vec![
            reference("skeleton", "skeleton:embedded", 2_000),
            reference("family", "family:F1", 50),
        ];
        for kind in ["pattern", "block", "example"] {
            for n in 1..=3 {
                references.push(reference(kind, &format!("{kind}:{n}"), 40_000));
            }
        }
        references
    }

    fn receipts(references: &[Reference]) -> Vec<Value> {
        references.iter().map(Reference::receipt).collect()
    }

    fn total(references: &[Reference], fixed: usize) -> usize {
        fixed + references.iter().map(weight).sum::<usize>()
    }

    #[test]
    fn escaped_counts_what_a_json_string_writes() {
        let text = "a \"quoted\" path\\to\n\tline\r\u{8}\u{c}\u{1} é → ✓";
        let written = serde_json::to_string(text).unwrap();
        assert_eq!(escaped(text), written.len() - 2, "{written}");
        let reference = Reference::new("block:b", "block", text);
        let rendered = nika_compile_seats::shelf::rendered(std::slice::from_ref(&reference), &[]);
        let header = nika_compile_seats::shelf::rendered(&[], &[]);
        assert_eq!(weight(&reference), escaped(&rendered) - escaped(&header));
        let skeleton = Reference::new("skeleton:s", "skeleton", "nika: s\n");
        let rendered = nika_compile_seats::shelf::rendered(std::slice::from_ref(&skeleton), &[]);
        assert_eq!(weight(&skeleton), escaped(&rendered) - escaped(&header));
    }

    #[test]
    fn an_opening_that_fits_is_sent_unchanged() {
        let mut references = opening();
        let mut sent = receipts(&references);
        let (before, rows) = (references.clone(), sent.clone());
        assert_eq!(fit(&mut references, &mut sent, 2, 100_000), 0);
        assert_eq!((references, sent), (before, rows));
    }

    /// Past the capacity, the last block, then the last pattern... in turn from the last kind,
    /// each shown by its first line and receipted with its whole entry, until the opening fits;
    /// the embedded references and the heads of each kind stay whole.
    #[test]
    fn the_last_of_each_kind_is_shown_by_its_first_line_until_the_opening_fits() {
        let mut references = opening();
        let mut sent = receipts(&references);
        let whole = references.clone();
        let budget = CAPACITY - ROUNDS;
        // Four references too many: one more than a turn over the three kinds.
        let fixed = budget - total(&references, 0) + 4 * 40_000 - 1_000;
        let fitted = fit(&mut references, &mut sent, 2, fixed);
        assert_eq!(fitted, 4);
        assert!(total(&references, fixed) <= budget);
        let shown: Vec<&str> = (references.iter())
            .filter(|r| r.text.contains(SAID))
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(shown, ["pattern:3", "block:3", "example:2", "example:3"]);
        for (at, reference) in references.iter().enumerate() {
            let row = &sent[at];
            assert_eq!(row["id"], reference.id.as_str());
            assert_eq!(row["bytes"], reference.text.len());
            assert_eq!(row["sha256"], sha256(&reference.text));
            if shown.contains(&reference.id.as_str()) {
                assert_eq!(
                    reference.text,
                    format!("{} {} — its title\n{SAID}", reference.kind, reference.id)
                );
                assert_eq!(row["first_line_only"]["bytes"], whole[at].text.len());
                assert_eq!(row["first_line_only"]["sha256"], sha256(&whole[at].text));
                assert_eq!(row["first_line_only"]["why"], WHY);
            } else {
                assert_eq!(reference, &whole[at], "{} stays whole", reference.id);
                assert!(row.get("first_line_only").is_none());
            }
        }
    }

    /// A short kind gives the same share of its order as a long one: past the last of each kind,
    /// the longer kind's next to last goes before the short kind's first.
    #[test]
    fn every_kind_gives_the_same_share_of_its_order() {
        let mut references = vec![
            reference("skeleton", "skeleton:1", 40_000),
            reference("skeleton", "skeleton:2", 40_000),
        ];
        for n in 1..=6 {
            references.push(reference("pattern", &format!("pattern:{n}"), 40_000));
        }
        let mut sent = receipts(&references);
        let budget = CAPACITY - ROUNDS;
        let fixed = budget - total(&references, 0) + 4 * 40_000 - 1_000;
        assert_eq!(fit(&mut references, &mut sent, 0, fixed), 4);
        let shown: Vec<&str> = (references.iter())
            .filter(|r| r.text.contains(SAID))
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(shown, ["skeleton:2", "pattern:4", "pattern:5", "pattern:6"]);
    }

    /// A reference its first line would not shorten stays whole, and an opening the pack
    /// cannot bring under the capacity is fitted as far as it goes, never refused here.
    #[test]
    fn what_cannot_shrink_stays_whole_and_the_door_still_sends() {
        let mut references = vec![
            reference("pattern", "pattern:big", 40_000),
            Reference::new("family:short", "family", "one line"),
        ];
        let mut sent = receipts(&references);
        let fitted = fit(&mut references, &mut sent, 0, CAPACITY);
        assert_eq!(fitted, 1);
        assert!(references[0].text.contains(SAID));
        assert_eq!(references[1].text, "one line");
        assert!(sent[1].get("first_line_only").is_none());
    }

    /// The first line is cut on a character boundary, never inside one.
    #[test]
    fn a_long_first_line_is_cut_on_a_character_boundary() {
        let text = format!("{}\nrest", "é".repeat(400));
        let shown = first_line(&text);
        let (line, said) = shown.split_once('\n').unwrap();
        assert!(line.len() <= LINE);
        assert!(line.chars().all(|c| c == 'é'));
        assert_eq!(said, SAID);
    }
}
