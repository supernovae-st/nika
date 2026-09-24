// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The copy family: « copy ./a.txt as is to ./out/b.txt », « copie ./a.txt tel quel, octet
//! pour octet, dans ./out/b.txt », « copia ./a.txt tal cual en ./out/b.txt » — one source
//! file read, one destination written with what was read, no language step. The identity
//! words between the two paths (as is, tel quel, byte for byte, unverändert) state what the
//! copy does by construction; they bind nothing else. Preparing or making a copy names the
//! same object, provided its entire object is literal files joined by identity and a
//! destination connector. A marketing, translated or otherwise qualified copy is not it.
//! Each file is its whole literal (a quoted or capitalized name keeps its spaces). A copy
//! that names no destination, names its source again, or leaves a lowercase multiword name
//! open reads what it can and asks for the exact file: never a guess, never the source.
use super::super::objects;
use super::super::paths::{self, PathShape};
use super::super::plan::{Binding, Effect, EffectPolicy, EffectVerb, Op, Step};
use super::Reading;

/// The copy heads, six languages, folded as the clause is (lowercase, accents kept).
const COPY_HEADS: &[&str] = &[
    "copy", "copie", "copiez", "copier", "copia", "copiate", "copiare", "kopiere", "kopier",
    "kopieren", "copie", "copiem",
];

/// Fidelity adverbs a copy may carry beside its identity words (« copie exactement »).
const EXACTLY: &[&str] = &[
    "exactement",
    "exactly",
    "identiquement",
    "fidèlement",
    "faithfully",
];

/// One file of a copy clause.
struct Operand {
    /// The file as the clause names it, spaces included.
    literal: String,
    /// The byte span of its words (quotes included) in the clause.
    start: usize,
    end: usize,
    /// The words settle it (a file token, a quoted or capitalized name) or leave it open.
    settled: bool,
}

/// The words before the source: a copy head (fidelity adverbs aside), or a prepare, make
/// or create verb whose whole object is a copy of the file, not produced prose. The copy
/// family claims such a clause before any head reads it; no cue joins the reader's table.
fn copy_prefix(words: &[&str]) -> bool {
    let Some((head, rest)) = words.split_first() else {
        return false;
    };
    if COPY_HEADS.contains(head) {
        let rest = &rest[rest.iter().take_while(|w| exactly(w)).count()..];
        return matches!(rest, [] | ["of" | "from" | "de"])
            || file_noun(rest)
            || matches!(rest, ["of" | "from" | "de", tail @ ..] if file_noun(tail))
            || rest == ["du", "fichier"];
    }
    let (noun, connectors, articles): (&str, &[&str], &[&str]) = match *head {
        "prepare" | "make" | "create" => ("copy", &["of", "from"], &["a", "the"]),
        "prépare" | "préparez" | "préparer" | "fais" | "faites" | "crée" | "créez" => {
            ("copie", &["de", "du"], &["la", "une"])
        }
        _ => return false,
    };
    let rest = if rest.first().is_some_and(|word| articles.contains(word)) {
        &rest[1..]
    } else {
        rest
    };
    let rest = if noun == "copy" {
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

fn exactly(word: &str) -> bool {
    EXACTLY.contains(&word)
}

/// A file noun with its determiners: `file`, `le fichier`, `un nouveau fichier`, `a new file`.
fn file_noun(words: &[&str]) -> bool {
    match words {
        ["file" | "fichier"] => true,
        [first, rest @ ..] => {
            matches!(
                *first,
                "the" | "a" | "an" | "le" | "la" | "un" | "une" | "new" | "nouveau"
            ) && file_noun(rest)
        }
        [] => false,
    }
}

/// Words that only say the copy is faithful (`tel quel`, `as is`, `exactement`).
fn identity(words: &[&str]) -> bool {
    words.iter().all(|word| exactly(word)) || objects::identity_only(&words.join(" "))
}

/// Every word between the files is identity, a destination connector, or a file noun.
/// In particular, finding two paths alone does not account for transformations or gates.
fn destination_bridge(words: &[&str]) -> bool {
    words.iter().enumerate().any(|(at, word)| {
        matches!(
            *word,
            "to" | "into" | "in" | "dans" | "vers" | "en" | "nach" | "para"
        ) && identity(&words[..at])
            && (words[at + 1..].is_empty() || file_noun(&words[at + 1..]))
    })
}

/// The files of a clause in order, duplicates kept; `None` when a path-like literal is
/// no file (a folder, a glob, a placeholder): such a clause is not a copy.
fn operands(original: &str) -> Option<Vec<Operand>> {
    let operand = |(shape, start, end): (PathShape, usize, usize)| match shape {
        PathShape::File(literal) => Some(Operand {
            literal,
            start,
            end,
            settled: true,
        }),
        PathShape::Placeholder(literal)
            if literal.contains(' ') && !literal.contains(['<', '{', '$']) =>
        {
            let settled = literal.starts_with(char::is_uppercase);
            Some(Operand {
                literal,
                start,
                end,
                settled,
            })
        }
        _ => None,
    };
    paths::located(original).into_iter().map(operand).collect()
}

fn words(span: &str) -> Vec<&str> {
    span.split_whitespace().collect()
}

/// The clause as a copy: its source, its destination when it names a distinct settled
/// file, and the words naming the copy (`une copie`, `Copie`) its question asks about.
fn parse(text: &str, original: &str) -> Option<(Operand, Option<Operand>, String)> {
    let mut files = operands(original)?.into_iter();
    let (source, second) = (files.next()?, files.next());
    if files.next().is_some() {
        return None;
    }
    // A leading filler (`please`, `ensuite`) is off the lowered clause: skip as many words.
    let skip = original
        .split_whitespace()
        .count()
        .checked_sub(text.split_whitespace().count())?;
    let prefix: Vec<&str> = original
        .get(..source.start)?
        .split_whitespace()
        .skip(skip)
        .collect();
    let head = prefix.join(" ").to_lowercase();
    if !copy_prefix(&words(&head)) {
        return None;
    }
    let mut last = source.end;
    if let Some(destination) = &second {
        let bridge = original.get(source.end..destination.start)?.to_lowercase();
        if !destination_bridge(&words(&bridge)) {
            return None;
        }
        last = destination.end;
    }
    let tail = original.get(last..)?;
    let tail = tail
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if !identity(&words(&tail)) {
        return None;
    }
    // The source named again (`./a.txt` and `a.txt` alike) is never a destination.
    let file = |path: &str| path.trim_start_matches("./").to_lowercase();
    let destination =
        second.filter(|d| d.settled && file(d.literal.as_str()) != file(source.literal.as_str()));
    let noun = prefix
        .iter()
        .position(|word| matches!(word.to_lowercase().as_str(), "copy" | "copie"));
    let named = match noun {
        Some(at) if at > 0 => prefix[at - 1..=at].join(" "),
        _ => prefix.first().map_or("copy", |head| *head).to_owned(),
    };
    Some((source, destination, named))
}

/// A literal as a plan field: a name with spaces travels quoted, so every later reading
/// of the field takes it whole.
fn field(literal: &str) -> String {
    if literal.contains(' ') {
        format!("\"{literal}\"")
    } else {
        literal.to_owned()
    }
}

/// A clause that copies one file to another: a read of the source and a write of what
/// was read. A destination it does not settle is a write the compiler asks the file for.
pub(super) fn read(text: &str, original: &str, reading: &mut Reading) -> bool {
    let Some((source, destination, named)) = parse(text, original) else {
        return false;
    };
    // An open source keeps the word before it: its reading stays the placeholder the
    // compiler asks about, never the truncated token.
    let detail = if source.settled {
        field(&source.literal)
    } else {
        let before = original.get(..source.start).unwrap_or_default().trim_end();
        let from = before.len() - before.split_whitespace().next_back().map_or(0, str::len);
        original
            .get(from..source.end)
            .unwrap_or_default()
            .to_owned()
    };
    for operand in [Some(&source), destination.as_ref()].into_iter().flatten() {
        if operand.settled {
            reading.plan.bindings.push(Binding {
                role: "path",
                literal: operand.literal.clone(),
            });
        }
    }
    reading.plan.push_step(Step {
        op: Op::Read,
        evidence: original.to_owned(),
        detail,
        categories: Vec::new(),
    });
    let target = match &destination {
        Some(destination) => field(&destination.literal),
        None => named,
    };
    super::push_effect(
        &mut reading.plan,
        Effect {
            verb: EffectVerb::Write,
            target,
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
