// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Pure reading of explicit Run words and their input question, shared with the Session door.
//! No permission, workflow checking, cost admission or execution lives here.
//! The canonical access resolver judges the value; no vocabulary is copied here.

/// Return the remaining Run words and the explicit pin; invalid or duplicate options refuse.
#[must_use]
pub fn parse(input: &str) -> Option<(String, Option<String>)> {
    let mut words = input.split_whitespace();
    let mut line = Vec::new();
    let mut access = None;
    let mut cost = false;
    while let Some(word) = words.next() {
        if !word.starts_with('-') {
            line.push(word);
            continue;
        }
        let (flag, inline) = word
            .split_once('=')
            .map_or((word, None), |(k, v)| (k, Some(v)));
        let used = match flag {
            "--access" => access.is_some(),
            "--max-cost-usd" => cost,
            _ => return None,
        };
        let value = inline.or_else(|| words.next())?;
        if used || value.is_empty() || value.starts_with('-') {
            return None;
        }
        if flag == "--access" {
            access = Some(value.to_owned());
        } else {
            cost = true;
            line.push(word);
            if inline.is_none() {
                line.push(value);
            }
        }
    }
    Some((line.join(" "), access))
}

/// `name=value` pairs the human wrote on the run line itself.
#[must_use]
pub fn inline_vars(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter(|token| {
            token.split_once('=').is_some_and(|(k, v)| {
                !k.is_empty()
                    && !v.is_empty()
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        })
        .map(|token| token.trim_matches(|c| c == ',' || c == ';').to_owned())
        .collect()
}

/// The first word of an explicit run line (EN/FR). The French imperative
/// with its object pronoun — « lance-le », « exécute-la », « relance-le » —
/// is the same verb: it reaches the same run gate (check, money, the fresh
/// Run decision), never a conversation and never a run by itself.
#[must_use]
pub fn is_run_verb(first: &str) -> bool {
    let first = first.trim_end_matches(['.', '!']);
    let verb = match first.rsplit_once('-') {
        Some((verb, "le" | "la" | "les" | "moi")) => verb,
        _ => first,
    };
    matches!(
        verb,
        "run" | "execute" | "test" | "lance" | "exécute" | "teste" | "relance" | "run:"
    )
}

/// Whether a run line is the closed grammar and nothing more: the verb,
/// a workflow name, « it », a ceiling phrase, a few fillers. Anything
/// else in the line is a meaning of its own (a change, a condition).
#[must_use]
pub fn run_line_is_plain(lower: &str) -> bool {
    const FILLERS: &[&str] = &[
        "it",
        "again",
        "the",
        "workflow",
        "once",
        "now",
        "this",
        "that",
        "le",
        "la",
        "ça",
        "encore",
        "please",
        "stp",
        "svp",
        "with",
        "a",
        "ceiling",
        "of",
        "cap",
        "max",
        "cost",
        "usd",
        "dollar",
        "dollars",
        "budget",
        "plafond",
        "de",
        "un",
        "une",
        "avec",
        "at",
        "à",
        "$",
        // Courtesy and the run's once (« lance copy.nika s'il te plaît », « lance-le une fois
        // tout de suite », supernovae-1c's UX2 rig, 2026-10-17: the line reached the
        // conversation).
        "s'il",
        "te",
        "vous",
        "plaît",
        "plait",
        "merci",
        "fois",
        "maintenant",
        "tout",
        "suite",
        "go",
    ];
    lower
        .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
        .skip(1)
        .map(|w| {
            w.trim_matches(|c: char| matches!(c, '.' | ';' | '!' | '(' | ')' | '"' | '\'' | '`'))
        })
        .filter(|w| !w.is_empty())
        .all(|w| {
            FILLERS.contains(&w)
                || std::path::Path::new(w)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("nika"))
                || w.starts_with("./")
                || w.starts_with("--max-cost-usd")
                || w.contains('=')
                || crate::money::parse(w).is_ok_and(|money| money.money_only)
                || w.trim_start_matches('$').parse::<f64>().is_ok()
        })
}

/// Whether a line asks to see what is already there, a file's content or a run's results, rather
/// than to make it again: a show, display or read verb said to Nika (« montre-moi », « affiche »,
/// « lis-moi », « show me », « display », « read it out »), or the content named (« le contenu
/// de », « the contents of »). Seeing, looking or reading in passing (« je veux voir », « read
/// the notes ») asks it only in a line that names no run. Such a line is a reading, never a Run,
/// whatever else a classifier hears in it (NIK-234, live gauntlet wave 9: « Pouvez-vous me
/// montrer le contenu des trois traductions » reran the workflow and rewrote them). A file or a
/// path the line names (`show.nika`) is no word of it.
#[must_use]
pub fn asks_to_read(input: &str) -> bool {
    // The stems that show, every form of them (« montrez », « afficher », « displayed »), and the
    // content named (« le contenu », « the contents of »: `content` alone is a glad person).
    const SHOW: &[&str] = &["montr", "affich", "contenu", "show", "display", "view"];
    const LOOK: &[&str] = &[
        "voir", "regarde", "regardez", "regarder", "lis", "lisez", "lire", "relis", "relisez",
        "relire", "see", "look", "read",
    ];
    // Said to Nika: a pronoun joined to the verb (« lis-moi », « montrez-les »), or the word after
    // it (« read me », « read it out »).
    const JOINED: &[&str] = &["moi", "nous", "le", "la", "les", "lui", "leur"];
    const AFTER: &[&str] = &["me", "us", "out", "aloud"];
    // The stems of a line that names a run (« relance », « lancer », « rerun », « exécute-le »).
    const RUNS: &[&str] = &[
        "relanc", "lanc", "rerun", "re-run", "recommen", "exécut", "execut",
    ];
    let lower = input.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (lower.split_whitespace())
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\''))
        .map(|w| w.rsplit('\'').next().unwrap_or(w))
        .filter(|w| !w.is_empty() && !w.contains(['.', '/', '=', '`', '\\']))
        .collect();
    let runs = (words.iter())
        .any(|w| matches!(*w, "run" | "runs") || RUNS.iter().any(|run| w.starts_with(run)));
    (0..words.len()).any(|at| {
        let word = words.get(at).copied().unwrap_or_default();
        let next = words.get(at + 1).copied().unwrap_or_default();
        let (verb, pronoun) = word.split_once('-').unwrap_or((word, ""));
        let joined = !pronoun.is_empty() && pronoun.split('-').all(|p| JOINED.contains(&p));
        let to = joined || AFTER.contains(&next);
        SHOW.iter().any(|show| verb.starts_with(show))
            || (matches!(verb, "content" | "contents") && next == "of")
            || (LOOK.contains(&verb) && (to || !runs))
    })
}

/// The lowered explicit Run line, without interpreting its remaining words.
#[must_use]
pub fn run_prefix(input: &str) -> Option<String> {
    let lower = input.trim().to_lowercase();
    let first = lower
        .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
        .next()?;
    is_run_verb(first).then_some(lower)
}

/// The question for one declared input, in the product's words.
#[must_use]
pub fn input_question(workflow: &std::path::Path, name: &str, remaining: usize) -> String {
    let more = if remaining > 1 {
        format!(" ({} more after this one)", remaining - 1)
    } else {
        String::new()
    };
    format!(
        "`{}` declares an input it needs before it runs: `{name}`{more}\n  reply on the next line with its value (`input.{name}`) · `cancel` drops the run",
        workflow.display()
    )
}

#[cfg(test)]
mod tests {
    use super::{asks_to_read, inline_vars, is_run_verb, parse, run_line_is_plain, run_prefix};

    /// A line asking to show, display or read what is there is a reading (NIK-234), whatever
    /// run it also names; seeing or reading in passing is one only in a line naming no run, and
    /// a file named `show.nika` is no word of the line.
    #[test]
    fn a_line_asking_to_see_the_results_is_a_reading() {
        for line in [
            "Merci. Pouvez-vous me montrer le contenu des trois traductions, pour que je les relise ?",
            "montre-moi le résultat",
            "Affiche le fichier produit",
            "lis-moi la traduction",
            "Lisez-les, s'il vous plaît",
            "pouvez-vous m’afficher ce qu'il a écrit ?",
            "je voudrais voir les traductions",
            "Show me the translations",
            "Can you display the output?",
            "read me what it wrote",
            "What are the contents of notes-en/tarifs-2027.md?",
            "and the content of the report?",
            "relance-le et montre-moi le résultat",
        ] {
            assert!(asks_to_read(line), "{line}");
        }
        for line in [
            "relance-le",
            "Le fichier source est rétabli, peux-tu relancer le workflow enregistré tel quel ?",
            "lance-le pour voir si ça marche",
            "rerun it so I can see the result",
            "run show.nika",
            "Augmente le budget du run à 1 dollar et relance.",
            "Vous pouvez l'activer pour moi, directement ici ?",
            "relance avec un plafond de 1 $",
            "Je suis content du résultat, relance-le",
            "",
        ] {
            assert!(!asks_to_read(line), "{line}");
        }
    }

    #[test]
    fn run_words_keep_their_existing_closed_grammar() {
        for line in ["Run it", "lance-le", "exécute-la", "relance-les!"] {
            assert!(run_prefix(line).is_some(), "{line}");
        }
        assert!(!is_run_verb("running"));
        assert!(run_prefix("please summarize it").is_none());
        assert!(run_line_is_plain("run one.nika budget 2 usd name=Nika"));
        assert!(!run_line_is_plain("run it after deleting the source"));
        for line in [
            "lance copy.nika s'il te plaît",
            "lance-le une fois",
            "relance-le tout de suite stp",
            "lance-le maintenant, merci",
        ] {
            assert!(run_line_is_plain(line), "{line}");
        }
        assert!(!run_line_is_plain("lance-le une fois par jour"));
        assert_eq!(
            inline_vars("run one.nika name=Nika; n=2, empty= =missing"),
            vec!["name=Nika", "n=2"]
        );
    }

    #[test]
    fn explicit_pin_is_removed_from_words_but_kept_verbatim() {
        for value in ["codex", "claude-code", "mock", "other.nika"] {
            for flag in [format!("--access {value}"), format!("--access={value}")] {
                assert_eq!(
                    parse(&format!("run one.nika {flag} name=Nika --max-cost-usd 0.1")),
                    Some((
                        "run one.nika name=Nika --max-cost-usd 0.1".into(),
                        Some(value.into())
                    ))
                );
            }
        }
        assert_eq!(parse("run one.nika"), Some(("run one.nika".into(), None)));
    }

    #[test]
    fn invalid_flags_cannot_be_ignored_or_turn_into_filenames() {
        for line in [
            "run it --access",
            "run it --access=",
            "run it --access --max-cost-usd 1",
            "run it --access=codex --access=codex",
            "run it --access codex --access=api",
            "run it --unknown=codex",
            "run it --access-extra=codex",
            "run it --max-cost-usd=1 --max-cost-usd 2",
            "run it --max-cost-usd",
            "run it --",
            "run it -x",
        ] {
            assert!(parse(line).is_none(), "{line}");
        }
    }
}
