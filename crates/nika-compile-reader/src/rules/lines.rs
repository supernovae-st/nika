// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A line filter over a text source: « extrais toutes les lignes de titre markdown (celles
//! qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne »,
//! « keep only the lines containing ERROR », « las líneas que terminan en ; ». The lines
//! kept are those whose text starts with, contains or ends with a literal the request
//! states. A line has no columns: the clause compares the line itself (`.`), and the rule
//! runs over the lines of the source and writes lines back, as they are and in order — a
//! tail that says so (« telles quelles », « in order », « one per line ») binds nothing
//! more. Anything else after the literal is not this grammar: the clause stays unread.
use super::{Clause, Comparator, Junction, Operand, Rule, Shape};

/// The words that name lines, six languages, folded.
const LINE_WORDS: &[&str] = &[
    "lines", "line", "lignes", "ligne", "líneas", "lineas", "línea", "linea", "righe", "riga",
    "zeilen", "zeile", "linhas", "linha",
];

/// The predicate phrases (folded) and the comparison each states over the line.
const PREDICATES: &[(&str, Comparator)] = &[
    ("that start with", Comparator::StartsWith),
    ("which start with", Comparator::StartsWith),
    ("starting with", Comparator::StartsWith),
    ("that begin with", Comparator::StartsWith),
    ("which begin with", Comparator::StartsWith),
    ("beginning with", Comparator::StartsWith),
    ("qui commencent par", Comparator::StartsWith),
    ("qui commence par", Comparator::StartsWith),
    ("commençant par", Comparator::StartsWith),
    ("commencant par", Comparator::StartsWith),
    ("qui débutent par", Comparator::StartsWith),
    ("qui debutent par", Comparator::StartsWith),
    ("que empiezan por", Comparator::StartsWith),
    ("que empiezan con", Comparator::StartsWith),
    ("que empiecen por", Comparator::StartsWith),
    ("que empiecen con", Comparator::StartsWith),
    ("que comienzan por", Comparator::StartsWith),
    ("que comienzan con", Comparator::StartsWith),
    ("che iniziano con", Comparator::StartsWith),
    ("che iniziano per", Comparator::StartsWith),
    ("che cominciano con", Comparator::StartsWith),
    ("que começam com", Comparator::StartsWith),
    ("que comecam com", Comparator::StartsWith),
    ("que comecem com", Comparator::StartsWith),
    ("que iniciam com", Comparator::StartsWith),
    ("that contain", Comparator::Contains),
    ("which contain", Comparator::Contains),
    ("containing", Comparator::Contains),
    ("that include", Comparator::Contains),
    ("qui contiennent", Comparator::Contains),
    ("qui contient", Comparator::Contains),
    ("contenant", Comparator::Contains),
    ("que contienen", Comparator::Contains),
    ("que contengan", Comparator::Contains),
    ("que incluyen", Comparator::Contains),
    ("che contengono", Comparator::Contains),
    ("contenenti", Comparator::Contains),
    ("que contêm", Comparator::Contains),
    ("que contem", Comparator::Contains),
    ("contendo", Comparator::Contains),
    ("that end with", Comparator::EndsWith),
    ("which end with", Comparator::EndsWith),
    ("ending with", Comparator::EndsWith),
    ("qui finissent par", Comparator::EndsWith),
    ("qui se terminent par", Comparator::EndsWith),
    ("finissant par", Comparator::EndsWith),
    ("se terminant par", Comparator::EndsWith),
    ("que terminan en", Comparator::EndsWith),
    ("que terminan con", Comparator::EndsWith),
    ("que acaban en", Comparator::EndsWith),
    ("que acaban con", Comparator::EndsWith),
    ("che finiscono con", Comparator::EndsWith),
    ("che terminano con", Comparator::EndsWith),
    ("que terminam com", Comparator::EndsWith),
    ("que acabam com", Comparator::EndsWith),
];

/// German puts the literal between « die mit » and the verb: « Zeilen, die mit # beginnen ».
const GERMAN: &[(&str, &str, Comparator)] = &[
    ("die mit ", " beginnen", Comparator::StartsWith),
    ("die mit ", " anfangen", Comparator::StartsWith),
    ("die mit ", " enden", Comparator::EndsWith),
    ("die mit ", " aufhören", Comparator::EndsWith),
    ("die ", " enthalten", Comparator::Contains),
];

/// Quantity words before the literal that a prefix or suffix comparison already covers
/// (« un ou plusieurs # »: a line that starts with one # starts with the literal).
const ONE_OR_MORE: &[&str] = &[
    "un ou plusieurs",
    "une ou plusieurs",
    "one or more",
    "uno o más",
    "uno o mas",
    "una o más",
    "una o mas",
    "uno o più",
    "uno o piu",
    "una o più",
    "una o piu",
    "ein oder mehrere",
    "eine oder mehrere",
    "um ou mais",
    "uma ou mais",
    "au moins un",
    "au moins une",
    "at least one",
    "al menos un",
    "al menos una",
    "almeno un",
    "almeno una",
    "mindestens ein",
    "mindestens eine",
    "pelo menos um",
    "pelo menos uma",
];

/// A tail the lines mode realizes by construction, folded: the kept lines are written as
/// they are, in the source order, one per line.
const BY_CONSTRUCTION: &[&str] = &[
    "as they are",
    "as is",
    "as-is",
    "unchanged",
    "verbatim",
    "in order",
    "in the same order",
    "in their order",
    "in source order",
    "one per line",
    "each on its own line",
    "telles quelles",
    "tels quels",
    "tel quel",
    "telle quelle",
    "dans l'ordre",
    "dans le même ordre",
    "dans le meme ordre",
    "une par ligne",
    "un par ligne",
    "tal cual",
    "tal como están",
    "tal como estan",
    "sin cambios",
    "en el mismo orden",
    "en orden",
    "una por línea",
    "una por linea",
    "uno por línea",
    "uno por linea",
    "così come sono",
    "cosi come sono",
    "invariate",
    "nello stesso ordine",
    "in ordine",
    "una per riga",
    "uno per riga",
    "unverändert",
    "unverandert",
    "wie sie sind",
    "in der reihenfolge",
    "in derselben reihenfolge",
    "eine pro zeile",
    "einen pro zeile",
    "tal como estão",
    "tal como estao",
    "sem alterações",
    "sem alteracoes",
    "na mesma ordem",
    "em ordem",
    "uma por linha",
    "um por linha",
];

/// Where the literal ends: a closing parenthesis, a comma, a semicolon, an arrow, a
/// conjunction or a sequencer.
const TERMINATORS: &[&str] = &[
    ")", ",", ";", " →", " ->", " et ", " and ", " y ", " e ", " und ", " puis ", " then ",
    " luego ", " poi ", " dann ", " depois ",
];

/// The quotes a literal may wear.
const QUOTES: &[char] = &['"', '\'', '`', '«', '»', '“', '”', '‘', '’'];

fn whole_word_at(lower: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(at) = lower[from..].find(word) {
        let at = from + at;
        let end = at + word.len();
        let before = lower[..at].chars().next_back();
        let after = lower[end..].chars().next();
        let bounded = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
        if bounded(before) && bounded(after) {
            return Some(end);
        }
        from = end;
    }
    None
}

/// The literal region cut at its first terminator, or before a path and its connector. The
/// first character is the literal's own (« ; », « ) »), and a quoted literal ends at its
/// closing quote.
fn cut_literal(region: &str) -> usize {
    let lead = region.len() - region.trim_start().len();
    let body = &region[lead..];
    let Some(first) = body.chars().next() else {
        return lead;
    };
    if QUOTES.contains(&first) {
        let close = match first {
            '«' => '»',
            '“' => '”',
            '‘' => '’',
            same => same,
        };
        if let Some(at) = body[first.len_utf8()..].find(close) {
            return lead + first.len_utf8() + at + close.len_utf8();
        }
    }
    let mut end = body.len();
    for terminator in TERMINATORS {
        if let Some(at) = body[first.len_utf8()..].find(terminator) {
            end = end.min(first.len_utf8() + at);
        }
    }
    if let Some(at) = body.find(" ./").or_else(|| body.find(" ~/")) {
        // « # de ./rando/guide.md »: the connector before the path is not the literal.
        let before = body[..at].trim_end();
        let cut = before.rfind(' ').unwrap_or(before.len());
        end = end.min(cut);
    }
    lead + end
}

/// Whether a clause only says what a lines rule does by construction (« as they are, in
/// order », « telles quelles, une par ligne »), folded.
#[must_use]
pub fn by_construction_tail(text: &str) -> bool {
    let lower = text.to_lowercase();
    !lower.trim().is_empty() && tail_by_construction(&lower)
}

fn strip_literal(raw: &str) -> &str {
    let mut literal = raw.trim();
    for prefix in ONE_OR_MORE {
        if let Some(rest) = literal
            .strip_prefix(prefix)
            .filter(|rest| rest.starts_with(' '))
        {
            literal = rest.trim();
            break;
        }
    }
    literal.trim_matches(|c: char| QUOTES.contains(&c)).trim()
}

/// Whether the tail after the literal only says what the lines mode does by itself.
fn tail_by_construction(tail: &str) -> bool {
    tail.split([',', ';', ')', '('])
        .map(|segment| segment.trim().trim_end_matches('.').trim())
        .filter(|segment| !segment.is_empty())
        .all(|segment| BY_CONSTRUCTION.contains(&segment))
}

/// A line filter the text states, or `None` when the text is not this grammar.
#[must_use]
pub fn line_filter(text: &str) -> Option<Rule> {
    let text = text.trim();
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return None;
    }
    let after_lines = LINE_WORDS
        .iter()
        .filter_map(|word| whole_word_at(&lower, word))
        .min()?;
    let rest = &lower[after_lines..];
    let (literal_at, literal_end, predicate_end, comparator) = predicate(rest)?;
    // Between the line word and the predicate: a short description of the lines (« de
    // titre markdown (celles »), never a path or another clause.
    let between = &rest[..literal_at];
    if between.contains("./") || between.split_whitespace().count() > 8 {
        return None;
    }
    let raw = &text[after_lines + literal_at..after_lines + literal_end];
    let literal = strip_literal(raw);
    if literal.is_empty() || literal.contains("./") {
        return None;
    }
    let tail = &lower[after_lines + predicate_end..];
    let tail = tail
        .trim_start()
        .strip_prefix(|c: char| TERMINATORS.iter().any(|t| t.starts_with(c) && t.len() == 1))
        .unwrap_or(tail);
    if !tail_by_construction(tail) {
        return None;
    }
    Some(Rule {
        text: text.to_owned(),
        clauses: vec![Clause::new(
            ".",
            comparator,
            Operand::Text(literal.to_owned()),
        )],
        junction: Junction::And,
        summary: false,
        shape: Shape::default(),
        lines: true,
    })
}

/// The literal's byte span in `rest`, the end of the whole predicate (German closes it
/// with the verb: « die mit # beginnen ») and the comparison, from the earliest predicate.
fn predicate(rest: &str) -> Option<(usize, usize, usize, Comparator)> {
    let mut best: Option<(usize, usize, usize, usize, Comparator)> = None;
    let earlier = |best: &Option<(usize, usize, usize, usize, Comparator)>, at: usize| {
        best.is_none_or(|(found, _, _, _, _)| at < found)
    };
    for (phrase, comparator) in PREDICATES {
        if let Some(at) = rest.find(phrase) {
            let start = at + phrase.len();
            if !rest[start..].starts_with(' ') {
                continue;
            }
            let end = start + cut_literal(&rest[start..]);
            if earlier(&best, at) {
                best = Some((at, start, end, end, *comparator));
            }
        }
    }
    for (opener, closer, comparator) in GERMAN {
        if let Some(at) = rest.find(opener)
            && let Some(close) = rest[at + opener.len()..].find(closer)
        {
            let start = at + opener.len();
            let end = start + close;
            if earlier(&best, at) {
                best = Some((at, start, end, end + closer.len(), *comparator));
            }
        }
    }
    let (_, start, end, consumed, comparator) = best?;
    (end > start).then_some((start, end, consumed, comparator))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jq_of(text: &str) -> String {
        line_filter(text).expect(text).jq()
    }

    #[test]
    fn a_line_filter_reads_six_languages() {
        let cases = [
            (
                "extrais toutes les lignes de titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne",
                "startswith(\"#\")",
            ),
            (
                "keep only the lines containing ERROR",
                "contains(\"ERROR\")",
            ),
            (
                "the lines that start with #, as they are, in order",
                "startswith(\"#\")",
            ),
            ("las líneas que terminan en ;", "endswith(\";\")"),
            ("le righe che contengono « TODO »", "contains(\"TODO\")"),
            ("die Zeilen, die mit # beginnen", "startswith(\"#\")"),
            (
                "as linhas que começam com #, na mesma ordem",
                "startswith(\"#\")",
            ),
        ];
        for (text, predicate) in cases {
            let jq = jq_of(text);
            assert!(jq.contains(predicate), "{text}: {jq}");
            assert!(
                jq.starts_with("[.records[] | select((. | tostring | "),
                "{jq}"
            );
            assert!(
                jq.ends_with("| join(\"\\n\") | if length > 0 then . + \"\\n\" else . end"),
                "{jq}"
            );
        }
    }

    #[test]
    fn a_line_filter_is_a_lines_rule_that_round_trips() {
        let rule = line_filter("the lines that start with #").expect("a line filter");
        assert!(rule.lines());
        assert_eq!(rule.over_lines(), Some(rule.clone()));
        assert!(rule.fields().is_empty(), "{:?}", rule.fields());
        assert_eq!(Rule::from_json(&rule.to_json()), Some(rule.clone()));
        assert_eq!(
            rule.guard(),
            "(.records | type) == \"array\" and all(.records[]; type == \"string\")"
        );
        // The negation of a text comparison is its complement.
        let negated = Clause::new(
            ".",
            Comparator::Contains.negated(),
            Operand::Text("x".into()),
        );
        assert_eq!(negated.jq(), "((. | tostring | contains(\"x\")) | not)");
    }

    #[test]
    fn a_line_filter_refuses_what_it_does_not_read() {
        for text in [
            "keep only the lines containing ERROR in ./out/errors.txt",
            "the lines",
            "the lines that start with",
            "the rows whose amount > 100",
            "the lines that start with # and sort them",
            "extract the headline from ./guide.md",
        ] {
            assert!(line_filter(text).is_none(), "{text}");
        }
    }
}
