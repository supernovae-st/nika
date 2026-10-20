// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A file the person takes back. Their latest words about a file decide whether it is still
//! asked for: a clause that stops it from being read or used (« ne lis pas profil.md », « don't
//! read notes.md », « profil.md n'est plus lu ») withdraws every earlier mention of it, and a
//! later clause naming it otherwise asks for it again. A clause that only qualifies the reading
//! (« ne lis pas data.csv en entier ») withdraws nothing. Live gauntlet, 2026-10-15: « ne lis
//! pas profil.md, copie mon profil dans le workflow » left the request's first mention owed, and
//! every workflow without the read was refused until the person restated their whole request.

/// Phrases that stop the file named right after them (a determiner or « fichier » between).
const STOPS_BEFORE: &[&str] = &[
    "ne lis pas",
    "ne lis plus",
    "ne lis jamais",
    "ne lisez pas",
    "ne lisez plus",
    "ne lit pas",
    "ne lit plus",
    "ne lira pas",
    "ne lira plus",
    "ne pas lire",
    "ne plus lire",
    "sans lire",
    "n'utilise pas",
    "n'utilise plus",
    "n'utilisez pas",
    "n'utilisez plus",
    "ne pas utiliser",
    "ne plus utiliser",
    "oublie",
    "oubliez",
    "laisse tomber",
    "laissez tomber",
    "pas besoin de lire",
    "plus besoin de lire",
    "pas besoin de",
    "plus besoin de",
    "don't read",
    "do not read",
    "doesn't read",
    "does not read",
    "won't read",
    "no longer reads",
    "never read",
    "stop reading",
    "no need to read",
    "without reading",
    "don't use",
    "do not use",
    "stop using",
    "no need for",
    "forget",
    "drop",
    // « un fichier par soir seulement, sans journal.txt » (live gauntlet, 2026-10-16: Nika's own
    // suggestion, refused because the file stayed owed).
    "sans",
    "without",
    "pas de",
    "plus de",
    "no more",
];

/// Phrases that stop the file named right before them.
const STOPS_AFTER: &[&str] = &[
    "n'est plus lu",
    "n'est plus lue",
    "n'est pas lu",
    "n'est pas lue",
    "n'est plus utilisé",
    "n'est plus utilisée",
    "is no longer read",
    "is not read",
    "is no longer used",
    "is not used",
];

/// Words before a stopping phrase that make it a condition, never a withdrawal (« si tu ne lis
/// pas profil.md, … »).
const CONDITIONS: &[&str] = &["si", "if", "when", "quand", "lorsque", "unless", "sauf"];

/// Words that may stand between a stopping phrase and the file it stops.
const BETWEEN: &[&str] = &[
    "le", "la", "les", "l'", "de", "du", "mon", "ma", "mes", "ton", "ta", "the", "my", "your",
    "fichier", "file", "document",
];

/// Whether the person's words `intent` (their lines, in order) withdraw `path`: the last clause
/// that names it stops it from being read or used. A path never named is not withdrawn.
#[must_use]
pub fn withdrawn(intent: &str, path: &str) -> bool {
    let lower = super::lexicon::fold_apostrophes(intent).to_lowercase();
    let path = path.trim_start_matches("./").to_lowercase();
    if path.is_empty() {
        return false;
    }
    let last = clauses(&lower)
        .into_iter()
        .rfind(|clause| !occurrences(clause, &path).is_empty());
    last.is_some_and(|clause| stops(clause, &path))
}

/// Whether one lowercase `clause` stops `path`: a stopping phrase, the words [`BETWEEN`] allows,
/// then the path ending the clause; or the path opening it, then a stopping phrase.
fn stops(clause: &str, path: &str) -> bool {
    let clause = clause.trim().trim_end_matches(['.', '!']).trim();
    occurrences(clause, path).into_iter().any(|at| {
        let named_from = clause[..at].trim_end_matches("./");
        let after = clause[at + path.len()..].trim();
        let before = words(named_from);
        let unconditional = |head: &[&str]| !head.iter().any(|w| CONDITIONS.contains(w));
        let stopped_before = after.is_empty()
            && STOPS_BEFORE.iter().any(|phrase| {
                let phrase: Vec<&str> = phrase.split(' ').collect();
                (0..before.len()).any(|from| {
                    let to = from + phrase.len();
                    before.get(from..to) == Some(&phrase[..])
                        && before[to..].iter().all(|w| BETWEEN.contains(w))
                        && unconditional(&before[..from])
                })
            });
        let stopped_after =
            unconditional(&before) && STOPS_AFTER.iter().any(|phrase| after.starts_with(phrase));
        stopped_before || stopped_after
    })
}

/// The words of a text, an elided article kept with its apostrophe (`l'`, `n'utilise`).
fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || matches!(c, '«' | '»' | '"' | '`'))
        .filter(|word| !word.is_empty())
        .flat_map(|word| match word.find('\'') {
            // « l'exemple » is two words: the article and the rest.
            Some(at) if at <= 2 && word.len() > at + 1 && !word.starts_with("n'") => {
                vec![&word[..=at], &word[at + 1..]]
            }
            _ => vec![word],
        })
        .collect()
}

/// The clauses of the person's words: their lines and sentences, cut at a comma, a semicolon, a
/// colon, a parenthesis and the conjunctions that join two acts.
fn clauses(lower: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for sentence in super::lexicon::split_sentences(lower) {
        for part in sentence.split(['\n', ',', ';', ':', '(', ')']) {
            let mut rest = part;
            while let Some(at) = [" et ", " and ", " puis ", " then ", " mais ", " but "]
                .iter()
                .filter_map(|joint| rest.find(joint).map(|at| (at, joint.len())))
                .min()
            {
                out.push(&rest[..at.0]);
                rest = &rest[at.0 + at.1..];
            }
            out.push(rest);
        }
    }
    out
}

/// Where `path` stands whole in `text`: no letter, digit or path character runs into it.
fn occurrences(text: &str, path: &str) -> Vec<usize> {
    let joined = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.');
    text.match_indices(path)
        .map(|(at, _)| at)
        .filter(|&at| {
            let before = text[..at].chars().next_back();
            let after = text[at + path.len()..].chars().next();
            !before.is_some_and(|c| joined(c) && c != '.')
                && !after.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::withdrawn;

    const REQUEST: &str = "Chaque matin, recupere les offres, score-les par rapport a mon \
                           profil.md et ecris jobs.md.";

    /// The person's later words that stop the file take it back; their latest words decide.
    #[test]
    fn a_later_line_that_stops_a_file_withdraws_it() {
        for line in [
            "ne lis pas profil.md, copie mon profil dans le workflow",
            "Ne lis plus le fichier ./profil.md.",
            "don't read profil.md and copy my profile into the workflow",
            "profil.md n'est plus lu, donc ses modifications ne comptent pas",
            "oublie profil.md",
            "alors ne lis pas profil.md",
            "le workflow ne lit plus profil.md",
            "un fichier par soir seulement, sans profil.md",
            "plus de profil.md",
        ] {
            assert!(
                withdrawn(&format!("{REQUEST}\n{line}"), "profil.md"),
                "{line}"
            );
        }
        // Named again after the withdrawal, it is asked for again.
        let again = format!("{REQUEST}\nne lis pas profil.md\nfinalement, lis profil.md");
        assert!(!withdrawn(&again, "profil.md"));
    }

    /// A clause that qualifies the reading, another file, or no later word at all withdraws
    /// nothing.
    #[test]
    fn a_qualified_or_unrelated_negation_withdraws_nothing() {
        for line in [
            "ne lis pas profil.md en entier, juste le résumé",
            "ne lis pas notes.md",
            "garde profil.md à jour",
            "ne lis pas profil.md.bak",
            "si tu ne lis pas profil.md, le score sera faux",
            "garde le score sans toucher profil.md",
        ] {
            assert!(
                !withdrawn(&format!("{REQUEST}\n{line}"), "profil.md"),
                "{line}"
            );
        }
        assert!(!withdrawn(REQUEST, "profil.md"));
        assert!(!withdrawn(REQUEST, "absent.md"));
    }
}
