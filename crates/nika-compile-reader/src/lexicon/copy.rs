// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The copy family: « copy ./a.txt as is to ./out/b.txt », « copie ./a.txt tel quel, octet
//! pour octet, dans ./out/b.txt », « copia ./a.txt tal cual en ./out/b.txt » — one source
//! file read, one destination written with what was read, no language step. The identity
//! words between the two paths (as is, tel quel, byte for byte, unverändert) state what the
//! copy does by construction; they bind nothing else. Preparing a copy names the same
//! object, provided its entire object is two literal files joined by identity and a
//! destination connector. A marketing, translated or otherwise qualified copy is not it.
use super::super::objects;
use super::super::paths::{self, PathShape};
use super::super::plan::{Binding, Effect, EffectPolicy, EffectVerb, Op, Step};
use super::Reading;

/// The copy heads, six languages, folded as the clause is (lowercase, accents kept).
const COPY_HEADS: &[&str] = &[
    "copy", "copie", "copiez", "copier", "copia", "copiate", "copiare", "kopiere", "kopier",
    "kopieren", "copie", "copiem",
];

/// The object of an existing prepare head must name a copy, not produced prose. This
/// disambiguates existing heads through their object; it adds no head or operation cue.
fn copy_prefix(words: &[&str]) -> bool {
    let Some((head, rest)) = words.split_first() else {
        return false;
    };
    if COPY_HEADS.contains(head) {
        return matches!(rest, [] | ["of" | "from" | "de"])
            || file_noun(rest)
            || matches!(rest, ["of" | "from" | "de", tail @ ..] if file_noun(tail))
            || rest == ["du", "fichier"];
    }
    let (noun, connectors, articles): (&str, &[&str], &[&str]) = match *head {
        "prepare" => ("copy", &["of", "from"], &["a", "the"]),
        "prépare" | "préparez" | "préparer" => ("copie", &["de", "du"], &["la", "une"]),
        _ => return false,
    };
    let rest = if rest.first().is_some_and(|word| articles.contains(word)) {
        &rest[1..]
    } else {
        rest
    };
    let rest = if *head == "prepare" {
        rest.strip_prefix(&["file"]).unwrap_or(rest)
    } else {
        rest
    };
    let [found, connector, tail @ ..] = rest else {
        return false;
    };
    *found == noun
        && connectors.contains(connector)
        && if *connector == "du" {
            tail == ["fichier"]
        } else {
            tail.is_empty() || file_noun(tail)
        }
}

fn file_noun(words: &[&str]) -> bool {
    matches!(
        words,
        ["file" | "fichier"] | ["the", "file"] | ["le", "fichier"]
    )
}

/// Every word between the files is identity, a destination connector, or a file noun.
/// In particular, finding two paths alone does not account for transformations or gates.
fn destination_bridge(words: &[&str]) -> bool {
    words.iter().enumerate().any(|(at, word)| {
        matches!(
            *word,
            "to" | "into" | "in" | "dans" | "vers" | "en" | "nach" | "para"
        ) && objects::identity_only(&words[..at].join(" "))
            && (words[at + 1..].is_empty() || file_noun(&words[at + 1..]))
    })
}

/// Read only a complete literal copy shape. Path tokens are structural boundaries;
/// no non-file path or unconsumed prose may hide alongside the two files.
fn literal_pair(text: &str, original: &str) -> Option<(String, String)> {
    let words: Vec<_> = text.split_whitespace().collect();
    let paths: Vec<_> = words
        .iter()
        .enumerate()
        .filter_map(|(at, word)| paths::token(word).map(|shape| (at, shape)))
        .collect();
    let [(from, PathShape::File(_)), (to, PathShape::File(_))] = paths.as_slice() else {
        return None;
    };
    if !copy_prefix(&words[..*from])
        || !destination_bridge(&words[from + 1..*to])
        || !objects::identity_only(&words[to + 1..].join(" "))
    {
        return None;
    }
    // Recover the original literals, never the lowercased matching tokens.
    match paths::literals(original).as_slice() {
        [PathShape::File(source), PathShape::File(destination)] => {
            Some((source.clone(), destination.clone()))
        }
        _ => None,
    }
}

/// A clause that copies one file to another: exactly two literal files with no residue.
pub(super) fn read(text: &str, original: &str, reading: &mut Reading) -> bool {
    let Some((source, destination)) = literal_pair(text, original) else {
        return false;
    };
    for path in [&source, &destination] {
        reading.plan.bindings.push(Binding {
            role: "path",
            literal: path.clone(),
        });
    }
    reading.plan.push_step(Step {
        op: Op::Read,
        evidence: original.to_owned(),
        detail: source.clone(),
        categories: Vec::new(),
    });
    super::push_effect(
        &mut reading.plan,
        Effect {
            verb: EffectVerb::Write,
            target: destination.clone(),
            evidence: original.to_owned(),
            policy: EffectPolicy::Automatic,
            policy_literal: None,
        },
    );
    true
}

#[cfg(test)]
mod tests {
    use super::super::super::plan::{EffectVerb, Op};
    use super::super::read as read_intent;

    #[test]
    fn a_copy_is_a_read_and_a_write_of_what_was_read_in_six_languages() {
        for intent in [
            "Copy ./fete/consignes.txt as is, byte for byte, to ./out/consignes-copie.txt",
            "copie ./fete/consignes.txt tel quel, octet pour octet, dans ./out/consignes-copie.txt",
            "Copia ./fete/consignes.txt tal cual en ./out/consignes-copie.txt",
            "Copia ./fete/consignes.txt così com'è in ./out/consignes-copie.txt",
            "Kopiere ./fete/consignes.txt unverändert nach ./out/consignes-copie.txt",
            "Copia ./fete/consignes.txt tal e qual para ./out/consignes-copie.txt",
        ] {
            let reading = read_intent(intent);
            let ops: Vec<Op> = reading.plan.steps.iter().map(|s| s.op).collect();
            assert_eq!(ops, [Op::Read], "{intent}: {:?}", reading.plan);
            assert_eq!(
                reading.plan.steps[0].detail, "./fete/consignes.txt",
                "{intent}"
            );
            assert_eq!(
                reading.plan.effects.len(),
                1,
                "{intent}: {:?}",
                reading.plan
            );
            assert_eq!(reading.plan.effects[0].verb, EffectVerb::Write);
            assert_eq!(reading.plan.effects[0].target, "./out/consignes-copie.txt");
            assert!(
                reading.unresolved.is_empty(),
                "{intent}: {:?}",
                reading.unresolved
            );
            assert!(
                reading.plan.constraints.is_empty(),
                "{intent}: {:?}",
                reading.plan.constraints
            );
        }
    }

    #[test]
    fn a_copy_with_one_path_or_a_folder_is_not_the_copy_family() {
        for intent in ["Copy ./a.txt somewhere safe", "Copy ./in/ to ./out/"] {
            let reading = read_intent(intent);
            assert!(
                reading
                    .plan
                    .effects
                    .iter()
                    .all(|e| e.verb != EffectVerb::Write)
                    || !reading.unresolved.is_empty()
                    || reading.plan.steps.is_empty(),
                "{intent}: {:?}",
                reading.plan
            );
        }
    }
}
